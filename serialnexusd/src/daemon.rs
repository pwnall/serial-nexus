//! The daemon's graph state and the RPC method implementations (design §10,
//! §11). Mutations run on the current-thread runtime, so a `RefCell` serializes
//! them with no locks (plan §2). Verbs: `load`/`dump`/`state`/`teardown`/
//! `shutdown` (phase 2) plus `rotate`/`subscribe` (phase 3) plus the arbitration
//! surface `lock`/`unlock`/`send` with `--steal`/`--lease`/`--wait` (phase 4).
//!
//! **The two-lane control plane (§15.20).** Every lock transition — acquire,
//! release, steal, lease expiry — is a *synchronous critical section* on the
//! runtime thread: borrow the [`EndpointLock`], mutate, drop the borrow. A verb
//! that cannot complete immediately (`lock --wait`, `send`'s acquire-with-timeout)
//! registers in the lock's FIFO waiter queue and suspends holding *nothing* — no
//! borrows, no locks, only its queue slot — then re-attempts inside a fresh
//! critical section when woken. "Mutations are serialized" (§10) therefore
//! survives unchanged while concurrent connections flow past a parked waiter. The
//! load-bearing discipline, with the same review status as §15.18's AsyncFd rule:
//! **a `RefCell` borrow never crosses an `.await`** — every method below borrows,
//! computes, drops, and only then awaits.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::config::GraphConfig;
use nexus_core::graph::{EndpointAddr, WriteMode};
use nexus_core::lock::{Acquire, Arbitration, OriginId, Steal};
use nexus_rpc::{Notification, RpcError, error_codes};
use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast, mpsc};

use crate::nodes::Node;
use crate::runtime::SharedLock;

/// Depth of the notification broadcast buffer (§10 `subscribe`). A subscriber
/// that falls this far behind sees a `Lagged` skip rather than blocking the
/// daemon — state snapshots are cumulative, so a dropped one loses nothing.
const NOTIFY_CAPACITY: usize = 64;

/// Default acquire-with-timeout for `send` when the caller names none (§6): the
/// CLI joins the waiter queue and fails with the locked error at this deadline.
const DEFAULT_SEND_TIMEOUT_MS: u64 = 2000;

/// Base for synthetic `send` origin ids, kept clear of the wiring's edge-assigned
/// ids (which count up from 0) so a transient CLI origin never collides with a
/// real one on the same endpoint (§6).
const SEND_ORIGIN_BASE: u64 = 1 << 40;

/// Daemon-specific error codes, in the reserved application range (§10).
pub mod app_errors {
    use nexus_rpc::error_codes::APP_ERROR_BASE;
    /// `load` attempted on a non-empty graph (§11 load-on-empty).
    pub const LOAD_NONEMPTY: i64 = APP_ERROR_BASE - 1;
    /// A structural validation failure (§4).
    pub const STRUCTURAL: i64 = APP_ERROR_BASE - 2;
    /// A `lock`/`send` was refused because another origin holds the endpoint's
    /// write lock (§6) — a plain contended acquire, or a `send` at its deadline.
    pub const LOCKED: i64 = APP_ERROR_BASE - 3;
}

#[derive(Default)]
struct GraphState {
    config: GraphConfig,
    nodes: Vec<Node>,
    /// Host-facing endpoint **display** (`usb0`, or a codec channel `mux/console`)
    /// → its write lock (§6), shared with the origin read tasks. The daemon mutates
    /// it on `lock`/`unlock`/`send` and reports it in `state`.
    endpoint_locks: HashMap<String, SharedLock>,
    /// Host-facing endpoint display → a targetward sender into it, so `send` can
    /// inject a line as a transient origin (§6).
    endpoint_targetward: HashMap<String, mpsc::Sender<Chunk>>,
    /// Writing origin **display** (a PTY node name, or a codec's multiplexed side by
    /// its node name) → (its endpoint's lock, its origin id), for resolving a
    /// `lock`/`unlock` by origin name to the right lock (§6).
    origin_locks: HashMap<String, (SharedLock, OriginId)>,
    /// Monotonic allocator for transient `send` origin ids (§6).
    next_send_origin: Cell<u64>,
}

/// The running daemon: graph state, a shutdown signal, and the `subscribe`
/// notification broadcast.
pub struct Daemon {
    state: RefCell<GraphState>,
    pub shutdown: Notify,
    notifier: broadcast::Sender<Notification>,
}

/// The outcome of a (possibly waiting) acquisition attempt (§15.20). Distinguishes
/// a fresh grant (which runs purge-on-acquire and advances the generation) from an
/// idempotent re-acquire.
enum WaitOutcome {
    /// A fresh grant: the caller now holds the lock (purge-on-acquire applies).
    Fresh,
    /// The caller already held the lock; no purge.
    AlreadyHeld,
    /// The origin is `write = never` and cannot hold the lock.
    ReadOnly,
    /// The deadline elapsed before a grant (only reachable with a finite deadline).
    TimedOut,
    /// The endpoint was torn down or removed while waiting (§6/§15.20): the waiter
    /// leaves the queue with a defined error rather than parking forever.
    Closed,
}

impl Daemon {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(NOTIFY_CAPACITY);
        Daemon {
            state: RefCell::new(GraphState {
                next_send_origin: Cell::new(SEND_ORIGIN_BASE),
                ..GraphState::default()
            }),
            shutdown: Notify::new(),
            notifier,
        }
    }

    /// A receiver for the `subscribe` stream (§10). Each subscribed connection
    /// holds one; the daemon publishes id-less notifications to all of them.
    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notifier.subscribe()
    }

    /// Publish a full state snapshot to subscribers (§10: status transitions and
    /// counter snapshots). A no-op when nobody is listening, so the periodic
    /// tick costs nothing on an unsubscribed daemon. This is the observability
    /// *floor*; lock transitions are additionally delivered as immediate `lock`
    /// notifications by the [`crate::runtime::LockCell`] (§10, §15.20).
    pub fn emit_state_snapshot(&self) {
        if self.notifier.receiver_count() == 0 {
            return;
        }
        let snapshot = self.state();
        let _ = self
            .notifier
            .send(Notification::new("state", Some(snapshot)));
    }

    /// Route one RPC method to its implementation (§10 verb surface). Async
    /// because the arbitration verbs may wait (`lock --wait`, `send`); every other
    /// verb resolves without awaiting, so their cost is unchanged.
    pub async fn dispatch(&self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        match method {
            "load" => self.load(parse_config_param(params)?),
            "dump" => Ok(self.dump()),
            "state" => Ok(self.state()),
            // The stream itself is served by the connection task (control.rs);
            // dispatch just acknowledges the subscription (§10).
            "subscribe" => Ok(json!({ "subscribed": true })),
            "rotate" => self.rotate(params),
            "lock" => self.lock(params).await,
            "unlock" => self.unlock(params),
            "send" => self.send(params).await,
            "teardown" => Ok(self.teardown()),
            "shutdown" => {
                self.shutdown.notify_one();
                Ok(json!({ "shutting_down": true }))
            }
            other => Err(RpcError::method_not_found(other)),
        }
    }

    /// `load` (§11): accepted only on an empty graph, structurally atomic. A
    /// structural error creates nothing; environmental failures fault nodes
    /// without failing the load (§15.8).
    fn load(&self, config: GraphConfig) -> Result<Value, RpcError> {
        let mut st = self.state.borrow_mut();
        if !st.nodes.is_empty() {
            return Err(RpcError::new(
                app_errors::LOAD_NONEMPTY,
                "load requires an empty graph — teardown first (or use load --replace)",
            ));
        }

        // Full structural validation before anything is created (§4, §11):
        // duplicate node names plus the three graph rules and name checks.
        let errors = config.validate();
        if !errors.is_empty() {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            return Err(RpcError::new(
                app_errors::STRUCTURAL,
                format!("structural error: {}", messages[0]),
            )
            .with_data(json!({ "errors": messages })));
        }

        // Instantiate nodes. An environmental failure faults the node (kept);
        // only an unimplemented node kind aborts the load, and then nothing is
        // committed.
        let mut nodes = Vec::with_capacity(config.nodes.len());
        for nc in &config.nodes {
            match Node::instantiate(nc) {
                Ok(node) => nodes.push(node),
                Err(reason) => {
                    for mut n in nodes {
                        n.teardown();
                    }
                    return Err(RpcError::invalid_params(format!(
                        "node {}: {reason}",
                        nc.name()
                    )));
                }
            }
        }

        // Wire the data plane from the validated edges, then start each node's
        // tasks (§5). Building the plan before the config moves keeps it borrow-
        // clean; `start` spawns onto the current-thread LocalSet. The notifier is
        // handed to each endpoint's lock so a lock transition emits an immediate
        // notification (§10).
        let mut wiring = crate::runtime::Wiring::build(&config, &self.notifier);
        // Keep clones of the write locks (§6) and per-endpoint targetward senders
        // so the control plane can acquire, release, and `send`; the same `Rc`s are
        // handed to the origin read tasks below. The wiring keys by endpoint address
        // (a codec has many); the RPC surface addresses by display string
        // (`usb0`, `mux/console`), so convert here.
        st.endpoint_locks = wiring
            .endpoint_locks
            .iter()
            .map(|(a, l)| (a.to_string(), l.clone()))
            .collect();
        st.endpoint_targetward = wiring
            .host_targetward_tx
            .iter()
            .map(|(a, t)| (a.to_string(), t.clone()))
            .collect();
        st.origin_locks = wiring
            .origin_locks
            .iter()
            .map(|(a, (l, id))| (a.to_string(), (l.clone(), *id)))
            .collect();
        st.nodes = nodes;
        st.config = config;
        for node in &mut st.nodes {
            node.start(&mut wiring);
        }
        Ok(json!({ "loaded": st.nodes.len() }))
    }

    /// `dump` (§11): configuration only, in exactly the load format. Returns the
    /// structured config; the CLI renders TOML.
    fn dump(&self) -> Value {
        serde_json::to_value(&self.state.borrow().config).expect("config serializes")
    }

    /// `state` (§10): observed status per node — never persisted, and disjoint
    /// from configuration by construction (§15.8).
    fn state(&self) -> Value {
        let st = self.state.borrow();
        let nodes: Vec<Value> = st
            .nodes
            .iter()
            .map(|n| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(n.name()));
                merge_into(&mut obj, serde_json::to_value(n.status()).unwrap());
                merge_into(&mut obj, n.state_extra());
                // Each of the node's host-facing endpoints reports its write-lock
                // state (§6: holder, waiters, per-origin purge counters, most recent
                // steal). A single-endpoint node (serial) reports it top-level as
                // `.lock`; a codec reports each channel's lock under
                // `.channels[channel].lock`. Observed state, disjoint from
                // configuration (§15.8).
                for (display, lock) in &st.endpoint_locks {
                    let addr: EndpointAddr = display.parse().expect("address is infallible");
                    if addr.node != n.name() {
                        continue;
                    }
                    let snap = serde_json::to_value(lock.borrow().snapshot())
                        .expect("lock snapshot serializes");
                    if addr.is_default() {
                        obj.insert("lock".into(), snap);
                    } else {
                        let channels = obj
                            .entry("channels")
                            .or_insert_with(|| Value::Object(serde_json::Map::new()));
                        if let Some(chmap) = channels.as_object_mut() {
                            let ch = chmap
                                .entry(addr.endpoint.clone())
                                .or_insert_with(|| Value::Object(serde_json::Map::new()));
                            if let Some(chobj) = ch.as_object_mut() {
                                chobj.insert("lock".into(), snap);
                            }
                        }
                    }
                }
                Value::Object(obj)
            })
            .collect();
        json!({ "nodes": nodes })
    }

    /// `rotate` (§7.3): rotate a log node's file on demand. Names the node in
    /// `params.node`; errors if it is unknown or not a log node.
    fn rotate(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node = params
            .as_ref()
            .and_then(|p| p.get("node"))
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'node' in params"))?;
        let st = self.state.borrow();
        let target = st
            .nodes
            .iter()
            .find(|n| n.name() == node)
            .ok_or_else(|| RpcError::invalid_params(format!("unknown node {node:?}")))?;
        match target.rotate() {
            Ok(rotated_to) => Ok(json!({ "node": node, "rotated_to": rotated_to })),
            Err(reason) => Err(RpcError::invalid_params(reason)),
        }
    }

    /// `lock` (§6): a named origin acquires its endpoint's exclusive write lock.
    /// `--steal` takes it from the current holder (recorded in state); `--wait`
    /// joins the FIFO queue and suspends until granted; `--lease-ms` auto-releases
    /// after a duration, guarded so a stale timer never releases a later grant.
    /// A plain, un-waited contended acquire fails fast with [`app_errors::LOCKED`].
    async fn lock(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let p = LockParams::parse(&params)?;
        let (cell, id) = self.resolve_origin(&p.origin)?;

        if p.steal {
            return self.grant_by_steal(&cell, &p.origin, id, p.lease_ms);
        }

        // Acquire, waiting in the FIFO queue if `--wait`, else fail-fast.
        let outcome = if p.wait {
            self.wait_for_grant(cell.clone(), id, None).await
        } else {
            let single = cell.borrow_mut().acquire(id);
            match single {
                Acquire::Granted => WaitOutcome::Fresh,
                Acquire::AlreadyHeld => WaitOutcome::AlreadyHeld,
                Acquire::ReadOnly => WaitOutcome::ReadOnly,
                Acquire::Denied { held_by } => return Err(self.locked_error(&cell, held_by)),
            }
        };

        match outcome {
            WaitOutcome::Fresh => {
                self.purge_on_acquire(&cell, &p.origin, id);
                self.after_grant(&cell, id, p.lease_ms);
                Ok(json!({ "origin": p.origin, "held": true, "acquired": true }))
            }
            WaitOutcome::AlreadyHeld => {
                // Re-arm a lease on the existing grant if asked; no purge. Bump the
                // generation first (`renew`) so the *prior* lease timer can no
                // longer fire — otherwise the earlier, shorter deadline would win
                // and defeat the extension (§6 lease renewal).
                if let Some(ms) = p.lease_ms {
                    let generation = cell.borrow_mut().renew(id);
                    if let Some(generation) = generation {
                        cell.emit_change();
                        self.spawn_lease(cell.clone(), id, generation, ms);
                    }
                }
                Ok(json!({ "origin": p.origin, "held": true, "acquired": false }))
            }
            WaitOutcome::ReadOnly => Err(RpcError::invalid_params(format!(
                "origin {:?} is write=never and cannot hold the lock",
                p.origin
            ))),
            WaitOutcome::Closed => Err(RpcError::new(
                app_errors::LOCKED,
                format!(
                    "endpoint behind origin {:?} was torn down while waiting",
                    p.origin
                ),
            )),
            WaitOutcome::TimedOut => unreachable!("--wait passes no deadline"),
        }
    }

    /// `unlock` (§6): release the endpoint's write lock if the named origin holds
    /// it, then wake the FIFO head so a waiter is granted next. Releasing when you
    /// do not hold it is reported, not an error.
    fn unlock(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let origin = origin_param(&params)?.to_owned();
        let (cell, id) = self.resolve_origin(&origin)?;
        let released = cell.borrow_mut().release(id);
        if released {
            cell.wake_waiters();
            cell.emit_change();
        }
        Ok(json!({ "origin": origin, "released": released }))
    }

    /// `send` (§6): deliver one line targetward through a named *endpoint*, with
    /// the CLI acting as a transient origin. It registers a synthetic origin,
    /// acquires with a timeout (or `--steal`s), writes the line, releases, and
    /// unregisters — one atomic daemon-side operation. A contended send fails with
    /// [`app_errors::LOCKED`] at its deadline; the transient origin is always
    /// cleaned up, even on cancellation.
    async fn send(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let p = SendParams::parse(&params)?;
        let (cell, sender) = {
            let st = self.state.borrow();
            let cell = st.endpoint_locks.get(&p.endpoint).cloned().ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "{:?} is not a host-facing endpoint with a write lock",
                    p.endpoint
                ))
            })?;
            let sender = st
                .endpoint_targetward
                .get(&p.endpoint)
                .cloned()
                .ok_or_else(|| RpcError::internal("endpoint has no targetward path"))?;
            (cell, sender)
        };
        let id = self.next_send_origin();
        // Register the transient origin (§6). The guard unregisters it on every
        // exit path — success, timeout, or a dropped connection — and wakes the
        // next waiter, so a cancelled `send` costs nothing but its queue slot.
        cell.borrow_mut().register(id, "send", WriteMode::OnDemand);
        let guard = TransientOrigin {
            cell: cell.clone(),
            id,
            disarm: Cell::new(false),
        };

        // Acquire the floor (steal, or join the queue with a deadline).
        if p.steal {
            let _ = cell.borrow_mut().steal(id);
            // Wake waiters so a parked `lock --wait` on the same origin observes it
            // now holds and returns, rather than parking forever (a steal removed it
            // from the queue). Other waiters simply re-check and re-park.
            cell.wake_waiters();
            cell.emit_change();
        } else {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(p.timeout_ms);
            match self.wait_for_grant(cell.clone(), id, Some(deadline)).await {
                WaitOutcome::Fresh | WaitOutcome::AlreadyHeld => cell.emit_change(),
                WaitOutcome::TimedOut => {
                    return Err(RpcError::new(
                        app_errors::LOCKED,
                        format!("endpoint {:?} is locked; send timed out", p.endpoint),
                    ));
                }
                WaitOutcome::ReadOnly => {
                    return Err(RpcError::internal("send origin registered write=never"));
                }
                WaitOutcome::Closed => {
                    return Err(RpcError::new(
                        app_errors::LOCKED,
                        format!("endpoint {:?} was torn down while sending", p.endpoint),
                    ));
                }
            }
        }

        // Deliver the line targetward — a real backpressure point, but no `RefCell`
        // borrow is held across this await (§15.20).
        let mut bytes = p.line.into_bytes();
        bytes.push(b'\n');
        let sent = bytes.len();
        let delivered = sender.send(Chunk::from(bytes)).await.is_ok();

        // Release + unregister the transient origin, then wake the next waiter.
        cell.borrow_mut().unregister(id);
        cell.wake_waiters();
        cell.emit_change();
        guard.disarm.set(true);

        if delivered {
            Ok(json!({ "endpoint": p.endpoint, "sent": sent, "delivered": true }))
        } else {
            Err(RpcError::internal(format!(
                "endpoint {:?} targetward closed",
                p.endpoint
            )))
        }
    }

    // --- arbitration helpers ---------------------------------------------------

    /// Resolve a `lock`/`unlock` origin name to its endpoint's lock and origin id
    /// (§6). The origin (a target-facing writer) feeds exactly one endpoint (§4).
    fn resolve_origin(&self, origin: &str) -> Result<(SharedLock, OriginId), RpcError> {
        let st = self.state.borrow();
        st.origin_locks
            .get(origin)
            .map(|(cell, id)| (cell.clone(), *id))
            .ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "{origin:?} is not a writable origin on any endpoint"
                ))
            })
    }

    fn next_send_origin(&self) -> OriginId {
        let st = self.state.borrow();
        let id = st.next_send_origin.get();
        st.next_send_origin.set(id.wrapping_add(1));
        OriginId(id)
    }

    /// Steal the lock for `origin` (§6): take it from the current holder, purge the
    /// stealer's own pre-grant backlog, record the theft, and optionally set a
    /// lease. Steal bypasses the FIFO queue without destroying it.
    fn grant_by_steal(
        &self,
        cell: &SharedLock,
        origin: &str,
        id: OriginId,
        lease_ms: Option<u64>,
    ) -> Result<Value, RpcError> {
        let outcome = cell.borrow_mut().steal(id);
        match outcome {
            Steal::ReadOnly => Err(RpcError::invalid_params(format!(
                "origin {origin:?} is write=never and cannot hold the lock"
            ))),
            Steal::Stolen { previous } => {
                let stole_from = previous.and_then(|p| cell.borrow().label(p).map(str::to_owned));
                self.purge_on_acquire(cell, origin, id);
                // Wake waiters: a `lock --wait` parked on the *stolen* origin now
                // holds the lock and must return (the steal dropped it from the
                // queue); other waiters re-check and re-park (§6/§15.20).
                cell.wake_waiters();
                self.after_grant(cell, id, lease_ms);
                Ok(json!({
                    "origin": origin,
                    "held": true,
                    "acquired": true,
                    "stole_from": stole_from,
                }))
            }
        }
    }

    /// Purge-on-acquire (§6): on a fresh EXCLUSIVE grant, drain and discard the
    /// origin's pre-grant targetward backlog **now** — synchronously, before this
    /// grant reply reaches the client — so a correct acquire-before-write client's
    /// later command is never mistaken for stale pre-grant input. Free-for-all has
    /// no acquisition, so a grant there must not disturb in-flight bytes. Draining
    /// a fd is synchronous, so no borrow crosses an await.
    fn purge_on_acquire(&self, cell: &SharedLock, origin: &str, id: OriginId) {
        if cell.borrow().arbitration() != Arbitration::Exclusive {
            return;
        }
        let purged = {
            let st = self.state.borrow();
            st.nodes
                .iter()
                .find(|n| n.name() == origin)
                .map_or(0, |n| n.purge_origin())
        };
        if purged > 0 {
            cell.borrow_mut().record_purge(id, purged);
        }
    }

    /// Common tail of a fresh grant: emit the immediate lock-change notification
    /// (§10) and, if a lease was requested, spawn a generation-guarded timer.
    fn after_grant(&self, cell: &SharedLock, id: OriginId, lease_ms: Option<u64>) {
        cell.emit_change();
        if let Some(ms) = lease_ms {
            let generation = cell.borrow().generation();
            self.spawn_lease(cell.clone(), id, generation, ms);
        }
    }

    /// Spawn a lease timer (§6): after `ms`, release the lock **only if this exact
    /// grant still holds it** — guarded by the grant generation captured now, so a
    /// stale timer can never release a later grant. Firing follows the normal
    /// release path (wake the queue head, notify).
    fn spawn_lease(&self, cell: SharedLock, id: OriginId, generation: u64, ms: u64) {
        tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            // The endpoint may have been torn down while the lease ran; do nothing
            // (the orphaned cell has no live consumers).
            if cell.is_closed() {
                return;
            }
            let fired = {
                let mut g = cell.borrow_mut();
                if g.holder() == Some(id) && g.generation() == generation {
                    g.release(id);
                    true
                } else {
                    false
                }
            };
            if fired {
                cell.wake_waiters();
                cell.emit_change();
            }
        });
    }

    /// The waiting lane of the two-lane control plane (§15.20): try to acquire in a
    /// synchronous critical section; if denied, join the FIFO queue and suspend on
    /// the lock's `Notify` (with an optional deadline) holding **nothing**, then
    /// re-attempt when woken. Cancel-safe: the [`WaiterGuard`] dequeues on any
    /// early drop (deadline, dropped connection, teardown) and wakes the next head.
    async fn wait_for_grant(
        &self,
        cell: SharedLock,
        id: OriginId,
        deadline: Option<tokio::time::Instant>,
    ) -> WaitOutcome {
        let guard = WaiterGuard {
            cell: cell.clone(),
            id,
            disarm: Cell::new(false),
        };
        loop {
            // The endpoint may have been torn down while we waited (teardown /
            // removal); leave the queue with a defined error rather than re-parking
            // forever (§6/§15.20). Checked each iteration, including right after a
            // wake — teardown calls `close()` (which wakes) before the maps clear.
            if cell.is_closed() {
                return WaitOutcome::Closed;
            }

            // Register interest BEFORE the check so a wake landing between the
            // check and the await is not lost (`Notify` lost-wakeup discipline).
            let notified = cell.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let settled = {
                let mut g = cell.borrow_mut();
                match g.acquire(id) {
                    Acquire::Granted => Some(WaitOutcome::Fresh),
                    Acquire::AlreadyHeld => Some(WaitOutcome::AlreadyHeld),
                    Acquire::ReadOnly => Some(WaitOutcome::ReadOnly),
                    Acquire::Denied { .. } => {
                        g.enqueue(id);
                        None
                    }
                }
            };
            if let Some(outcome) = settled {
                guard.disarm.set(true);
                return outcome;
            }

            match deadline {
                None => notified.await,
                Some(dl) => {
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = tokio::time::sleep_until(dl) => return WaitOutcome::TimedOut,
                    }
                }
            }
        }
    }

    fn locked_error(&self, cell: &SharedLock, held_by: OriginId) -> RpcError {
        let holder = cell.borrow().label(held_by).map(str::to_owned);
        RpcError::new(
            app_errors::LOCKED,
            format!(
                "endpoint is locked by {}",
                holder.as_deref().unwrap_or("another origin")
            ),
        )
        .with_data(json!({ "held_by": holder }))
    }

    fn teardown(&self) -> Value {
        let mut st = self.state.borrow_mut();
        let count = st.nodes.len();
        // Close every endpoint lock first: a parked `lock --wait`/`send` waiter may
        // hold an `Rc` clone that outlives these map entries, so `close()` (which
        // wakes it) lets it leave the queue with the defined teardown error rather
        // than parking forever (§6/§15.20). Done before dropping the nodes so the
        // wake precedes any task teardown.
        for cell in st.endpoint_locks.values() {
            cell.close();
        }
        for mut n in st.nodes.drain(..) {
            n.teardown();
        }
        st.config = GraphConfig::default();
        st.endpoint_locks.clear();
        st.endpoint_targetward.clear();
        st.origin_locks.clear();
        json!({ "torn_down": count })
    }

    /// Tear down all nodes on clean shutdown (unlink PTY symlinks, drop ports).
    pub fn teardown_all(&self) {
        let _ = self.teardown();
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

/// A parked waiter's cleanup handle (§15.20). While armed, dropping it — a
/// deadline, a dropped control connection, teardown, or endpoint removal —
/// dequeues the origin and wakes the next head, so a cancelled waiter costs
/// nothing but its queue slot. Disarmed once the wait resolves.
struct WaiterGuard {
    cell: SharedLock,
    id: OriginId,
    disarm: Cell<bool>,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        if self.disarm.get() {
            return;
        }
        let free = {
            let mut g = self.cell.borrow_mut();
            g.dequeue(self.id);
            g.holder().is_none()
        };
        if free {
            self.cell.wake_waiters();
        }
        self.cell.emit_change();
    }
}

/// A `send`'s transient origin registration. While armed, dropping it removes the
/// synthetic origin (releasing the lock if it held it) and wakes the next waiter —
/// so a `send` that times out or whose connection drops leaves no phantom origin
/// on the endpoint (§6). Disarmed once `send` cleans up on its success path.
struct TransientOrigin {
    cell: SharedLock,
    id: OriginId,
    disarm: Cell<bool>,
}

impl Drop for TransientOrigin {
    fn drop(&mut self) {
        if self.disarm.get() {
            return;
        }
        let free = {
            let mut g = self.cell.borrow_mut();
            g.unregister(self.id);
            g.holder().is_none()
        };
        if free {
            self.cell.wake_waiters();
        }
        self.cell.emit_change();
    }
}

/// Parsed `lock` params (§6).
struct LockParams {
    origin: String,
    steal: bool,
    wait: bool,
    lease_ms: Option<u64>,
}

impl LockParams {
    fn parse(params: &Option<Value>) -> Result<LockParams, RpcError> {
        let p = params
            .as_ref()
            .ok_or_else(|| RpcError::invalid_params("missing params"))?;
        let origin = p
            .get("origin")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'origin' in params"))?
            .to_owned();
        Ok(LockParams {
            origin,
            steal: p.get("steal").and_then(Value::as_bool).unwrap_or(false),
            wait: p.get("wait").and_then(Value::as_bool).unwrap_or(false),
            lease_ms: p.get("lease_ms").and_then(Value::as_u64),
        })
    }
}

/// Parsed `send` params (§6).
struct SendParams {
    endpoint: String,
    line: String,
    timeout_ms: u64,
    steal: bool,
}

impl SendParams {
    fn parse(params: &Option<Value>) -> Result<SendParams, RpcError> {
        let p = params
            .as_ref()
            .ok_or_else(|| RpcError::invalid_params("missing params"))?;
        let endpoint = p
            .get("endpoint")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'endpoint' in params"))?
            .to_owned();
        let line = p
            .get("line")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'line' in params"))?
            .to_owned();
        Ok(SendParams {
            endpoint,
            line,
            timeout_ms: p
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_SEND_TIMEOUT_MS),
            steal: p.get("steal").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

fn merge_into(target: &mut serde_json::Map<String, Value>, source: Value) {
    if let Value::Object(m) = source {
        for (k, v) in m {
            target.insert(k, v);
        }
    }
}

/// Extract the required `origin` string from an `unlock` request's params.
fn origin_param(params: &Option<Value>) -> Result<&str, RpcError> {
    params
        .as_ref()
        .and_then(|p| p.get("origin"))
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("missing 'origin' in params"))
}

fn parse_config_param(params: Option<Value>) -> Result<GraphConfig, RpcError> {
    let params = params.ok_or_else(|| RpcError::invalid_params("missing params"))?;
    let config = params
        .get("config")
        .ok_or_else(|| RpcError::invalid_params("missing 'config' in params"))?;
    serde_json::from_value(config.clone())
        .map_err(|e| RpcError::new(error_codes::INVALID_PARAMS, format!("invalid config: {e}")))
}
