//! The daemon's graph state and the RPC method implementations (design §10,
//! §11). Mutations run on the current-thread runtime, so a single-threaded cell
//! serializes them with no locks (plan §2). Verbs: `load`/`dump`/`state`/
//! `teardown`/`shutdown` (phase 2) plus `rotate`/`subscribe` (phase 3) plus the
//! arbitration surface `lock`/`unlock`/`send` with `--steal`/`--lease`/`--wait`
//! (phase 4).
//!
//! **The two-lane control plane (§15.20).** Every lock transition — acquire,
//! release, steal, lease expiry — is a *synchronous critical section* on the
//! runtime thread: `with_mut` the [`EndpointLock`] and mutate. A verb that cannot
//! complete immediately (`lock --wait`, `send`'s acquire-with-timeout) registers
//! in the lock's FIFO waiter queue and suspends holding *nothing* — no borrows, no
//! locks, only its queue slot — then re-attempts inside a fresh critical section
//! when woken. "Mutations are serialized" (§10) therefore survives unchanged while
//! concurrent connections flow past a parked waiter. The load-bearing discipline —
//! **a state borrow never crosses an `.await`** — is no longer a review rule but a
//! compile-shape fact: all daemon state lives in [`CriticalCell`], reachable only
//! inside a synchronous `with`/`with_mut` closure, so a borrow *cannot* span an
//! await (§16.2). Raw `std::cell::RefCell` is banned in this crate by clippy.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::config::GraphConfig;
use nexus_core::graph::{EndpointAddr, WriteMode};
use nexus_core::lock::{Acquire, Arbitration, OriginId, Steal};
use nexus_rpc::{Notification, RpcError, error_codes};
use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast, mpsc};

use crate::cell::CriticalCell;
use crate::nodes::Node;
use crate::registry::Registry;
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

/// File mode for the persisted configuration snapshot (§11/§15.9), owner-only
/// (SEC-4). It records exec-codec argv and environment and names every console, so
/// it gets the control socket's own 0600 posture rather than whatever the umask
/// happens to be — `--socket-group` widens the *socket*, deliberately not this.
const STATE_FILE_MODE: u32 = 0o600;

/// Daemon-specific error codes, in the reserved application range (§10). Defined
/// once in [`nexus_rpc::AppError`] (§16.8); these are const projections of that
/// single registry, so the call sites keep naming `app_errors::X` while the codes,
/// names, and docs table all derive from one source.
pub mod app_errors {
    use nexus_rpc::AppError;
    pub const LOAD_NONEMPTY: i64 = AppError::LoadNonEmpty.code();
    pub const STRUCTURAL: i64 = AppError::Structural.code();
    pub const LOCKED: i64 = AppError::Locked.code();
    pub const HAS_EDGES: i64 = AppError::HasEdges.code();
    pub const DEVICE_ABSENT: i64 = AppError::DeviceAbsent.code();
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
    /// Host-facing endpoint **display** → its live fan-out list (§15.35), so
    /// `connect`/`disconnect` can join and leave a *running* producer's broadcast
    /// without restarting it.
    host_fanout: HashMap<String, crate::runtime::SharedFanOut>,
    /// Target-facing endpoint **display** → the sending half of its edge inbox
    /// (§15.35): how `connect` hands a running consumer its new hostward receiver.
    target_inbox: HashMap<String, crate::runtime::EdgeInboxTx>,
    /// Target-facing endpoint **display** → its [`DropCounters`](crate::runtime::DropCounters).
    /// Owned by the endpoint, not the edge, so a reconnect does not reset an
    /// operator's loss history.
    target_counters: HashMap<String, std::sync::Arc<crate::runtime::DropCounters>>,
    /// Target-facing endpoint **display** → its live edge binding (§15.35), the slot
    /// `connect` fills and `disconnect` clears under the endpoint's running task.
    target_edges: HashMap<String, crate::runtime::SharedTargetEdge>,
    /// Monotonic allocator for edge-assigned origin ids, so an origin registered by
    /// a live `connect` never collides with one the initial wiring assigned (which
    /// counts up from 0) or with a transient `send` origin (which starts at
    /// [`SEND_ORIGIN_BASE`]).
    next_edge_origin: Cell<u64>,
    /// Monotonic allocator for transient `send` origin ids (§6).
    next_send_origin: Cell<u64>,
    /// Host-facing endpoint **display** → its tap hub (§5 ring, §17 taps). The
    /// daemon registers taps here on `tap.open`, and reports open taps in `state`.
    /// The hub's ingest task self-terminates when the producer's feed sender drops
    /// (teardown/removal), so no handle join is tracked.
    tap_hubs: HashMap<String, crate::tap::SharedTapHub>,
    /// Monotonic, daemon-unique allocator for tap ids (§17), so `state` and
    /// `tap.close` can name a tap unambiguously across endpoints and connections.
    next_tap_id: Cell<u64>,
}

impl GraphState {
    /// Detach every tap on the hubs this graph state still owns and tell their
    /// clients why (§17, TAP-1). `teardown`, `load --replace` and `remove-node` all
    /// drop hub handles out from under connection-side `OpenTap`s, which survive —
    /// so without this the tap vanishes from `state.taps` while its client sits on a
    /// live connection receiving nothing, with no notification and no error.
    ///
    /// `retain` decides which endpoints go: `remove-node` names the removed node's
    /// endpoints, `teardown`/`load --replace` take everything.
    fn detach_taps(&mut self, reason: crate::tap::TapCloseReason, retain: impl Fn(&str) -> bool) {
        self.tap_hubs.retain(|endpoint, hub| {
            if retain(endpoint) {
                return true;
            }
            let closed = hub.with_mut(|h| h.detach_all(reason));
            if !closed.is_empty() {
                tracing::info!(
                    endpoint = %endpoint,
                    taps = closed.len(),
                    reason = reason.as_str(),
                    "detached open taps: their endpoint is gone"
                );
            }
            false
        });
    }

    /// Undo one edge's *runtime* wiring: stop the bytes, clear the write path,
    /// leave the lock cleanly, and purge what the departing origin still held
    /// (§15.35). Configuration is the caller's to update.
    ///
    /// Shared by `disconnect` and by `remove-node --cascade`, deliberately: both
    /// remove edges from a running graph, and two implementations of "leave the
    /// lock cleanly" is how the phantom holder came back the first time (§15.27,
    /// §16 one-rule-one-place).
    fn detach_edge_runtime(&mut self, host: &str, target: &str) -> DetachedEdge {
        // 1. Stop the bytes. Dropping the producer's sink closes this edge's hostward
        //    channel, so the consumer drains what it already holds and then parks on
        //    its inbox — no chunk is lost to the detach itself.
        if let Some(fanout) = self.host_fanout.get(host) {
            fanout.detach(&target.parse().expect("address parses"));
        }

        // 2. Clear the consumer's write path before releasing the lock, so the origin
        //    takes no *new* chunk from its queue under a lock it no longer owns.
        //    (A chunk already parked inside `tx.send(...).await` may still land: the
        //    writer holds a clone of the host endpoint's sender, taken before the
        //    await. That is bounded — one in-flight chunk per origin — and is the
        //    same window `remove-node` has always had; closing it would mean an
        //    epoch on the targetward channel, which §14 defers as the PTY per-open
        //    epoch's sibling.)
        if let Some(slot) = self.target_edges.get(target) {
            slot.with_mut(|e| {
                e.writer = None;
                e.attached = false;
            });
        }

        // 3. Leave the lock cleanly (§6/§15.20). `unregister` drops this origin
        //    whether it was the holder or a queued waiter, and the wake lets the FIFO
        //    head take a lock the departing origin was holding — the phantom-holder
        //    fix, applied on the edge path before the bug could recur.
        //
        //    The registration comes from the edge slot, **not** from `origin_locks`.
        //    The lock registers every origin so `state` can list it; `origin_locks`
        //    holds only the ones `lock`/`unlock` can address (mode ≠ `never`).
        //    Unregistering through the narrower map leaves a read-only origin — a log
        //    edge, say — in `state` for an edge that no longer exists, and one more of
        //    them after every connect/disconnect cycle.
        let mut released_lock = false;
        self.origin_locks.remove(target);
        let registered = self
            .target_edges
            .get(target)
            .and_then(|slot| slot.with(|e| e.registered.clone()));
        if let Some((lock, origin_id)) = registered {
            released_lock = lock.with(|g| g.holder() == Some(origin_id));
            lock.with_mut(|g| g.unregister(origin_id));
            lock.wake_waiters();
            lock.emit_change();
        }
        if let Some(slot) = self.target_edges.get(target) {
            slot.with_mut(|e| e.registered = None);
        }

        // 4. Purge the departing origin's un-flushed targetward backlog (§6
        //    purge-on-acquire's sibling): bytes it typed while it still held the lock
        //    must not surface later under whoever takes the endpoint next.
        //
        //    Scope, stated rather than implied: `purge_origin` reaches a **pty**'s
        //    kernel buffer and nothing else — that is the case §6 wrote the rule for,
        //    a human typing ahead of a grant. An interior origin (codec, exec, map,
        //    leg) has no such buffer; what it holds sits in its own bounded channel,
        //    which the node keeps and will deliver if it is wired again. Draining
        //    that on a *disconnect* would be a design decision about whose bytes they
        //    are, and §6's purge rule does not make it — so this reports 0 for those
        //    kinds rather than pretending to have purged something.
        // `EndpointAddr`'s parse is infallible — every display form round-trips — so
        // the node component is always available here.
        let mut purged_bytes = 0;
        let target_addr: EndpointAddr = target.parse().expect("address parses");
        if let Some(node) = self.nodes.iter().find(|n| n.name() == target_addr.node) {
            purged_bytes = node.purge_origin();
        }
        // 5. Let the consuming node re-report its status: an interior node with no
        //    upstream is `waiting`, honestly (§7.5/§15.8).
        if let Some(node) = self.nodes.iter_mut().find(|n| n.name() == target_addr.node) {
            node.edge_detached(&target_addr);
        }
        DetachedEdge {
            released_lock,
            purged_bytes,
        }
    }

    /// Drop every per-endpoint handle belonging to `node` (§15.35). Called on
    /// removal, so the maps do not accumulate handles to endpoints the graph no
    /// longer has across add/remove cycles.
    fn forget_endpoints(&mut self, endpoints: &[String]) {
        for ep in endpoints {
            self.host_fanout.remove(ep);
            self.target_inbox.remove(ep);
            self.target_counters.remove(ep);
            self.target_edges.remove(ep);
        }
    }

    /// Find a node by name, or the shared `unknown node` error (§10) — the lookup
    /// idiom used by every node-targeted verb.
    fn node(&self, name: &str) -> Result<&Node, RpcError> {
        self.nodes
            .iter()
            .find(|n| n.name() == name)
            .ok_or_else(|| RpcError::invalid_params(format!("unknown node {name:?}")))
    }

    /// The index of a node by name, or the shared `unknown node` error — the
    /// removal path needs the position to `remove` it.
    fn node_index(&self, name: &str) -> Result<usize, RpcError> {
        self.nodes
            .iter()
            .position(|n| n.name() == name)
            .ok_or_else(|| RpcError::invalid_params(format!("unknown node {name:?}")))
    }

    /// Fold a freshly-built [`Wiring`](crate::runtime::Wiring)'s host-endpoint
    /// locks, per-endpoint targetward senders, and writer-origin locks into the
    /// display-keyed state maps (§6). The RPC surface addresses endpoints by display
    /// string (`usb0`, `mux/console`) while the wiring keys by `EndpointAddr`, so the
    /// keys are converted here — one place for both `load` (on an empty graph) and
    /// `add-node` (`origin_locks` is empty for an edgeless node, so its loop is a
    /// no-op there). Must run before each node's `start`, which drains the wiring.
    fn absorb_wiring(&mut self, wiring: &crate::runtime::Wiring) {
        for (a, l) in &wiring.endpoint_locks {
            self.endpoint_locks.insert(a.to_string(), l.clone());
        }
        for (a, t) in &wiring.host_targetward_tx {
            self.endpoint_targetward.insert(a.to_string(), t.clone());
        }
        for (a, (l, id)) in &wiring.origin_locks {
            self.origin_locks.insert(a.to_string(), (l.clone(), *id));
        }
        // The edge-surgery handles (§15.35). Absorbed the same way and at the same
        // time as the lock handles above, so `connect` on a node added a moment ago
        // reaches exactly the machinery `load` would have wired.
        for (a, f) in &wiring.host_fanout {
            self.host_fanout.insert(a.to_string(), f.clone());
        }
        for (a, t) in &wiring.target_inbox_tx {
            self.target_inbox.insert(a.to_string(), t.clone());
        }
        for (a, c) in &wiring.target_counters {
            self.target_counters.insert(a.to_string(), c.clone());
        }
        for (a, e) in &wiring.target_edges {
            self.target_edges.insert(a.to_string(), e.clone());
        }
        // Keep the edge-origin allocator clear of every id this wiring assigned.
        // Taken from the plan's own counter rather than from `origin_locks`, which
        // omits `never` origins: a graph whose only edge is `never` would otherwise
        // leave the floor at 0 and hand the next live `connect` an id already
        // registered on that very lock, where the two would alias — unregistering
        // one would drop the other.
        if wiring.next_origin > self.next_edge_origin.get() {
            self.next_edge_origin.set(wiring.next_origin);
        }
    }
}

/// What [`GraphState::detach_edge_runtime`] had to undo, reported rather than done
/// silently: an operator who disconnects a writer that held the write lock has
/// changed who may write, and one whose client had bytes queued has lost them by
/// design (§5 all-loss-is-visible, §6).
struct DetachedEdge {
    released_lock: bool,
    purged_bytes: u64,
}

/// The running daemon: graph state, a shutdown signal, and the `subscribe`
/// notification broadcast.
pub struct Daemon {
    state: CriticalCell<GraphState>,
    pub shutdown: Notify,
    notifier: broadcast::Sender<Notification>,
    /// Device-identity resolver (§12), rooted at `--dev-root` (a fixture seam in
    /// tests; `/` in production). Shared read-only with every serial node.
    resolver: nexus_core::Resolver,
    /// The compiled-in codec registry (§8/§15.26): the set of codecs this daemon
    /// can instantiate. Shared read-only with the graph so `load`/`add-node` build
    /// codec nodes from it and `info` reports its names. An embedding binary passes
    /// its own registry (built-ins plus registered codecs) into [`crate::run`].
    registry: Rc<Registry>,
    /// The configuration snapshot path (§11): after every successful *config*
    /// mutation the daemon writes the current config here (atomically), and startup
    /// prefers it, so incremental surgery survives a daemon restart. `None` disables
    /// persistence (unit tests via [`Daemon::default`]).
    state_file: Option<std::path::PathBuf>,
    /// A per-boot nonce reported by `info` (§10/§11.8). Tap byte offsets are only
    /// meaningful within one daemon process; on restart the offsets reset to 0 and
    /// this nonce changes, so a client (the browser history of §17) keyed on it
    /// detects the reset and starts a fresh history instead of splicing across it.
    instance: u64,
    /// How many control connections have actually issued `subscribe` (OBS-1). The
    /// broadcast channel's own `receiver_count()` cannot answer this: every
    /// connection takes a receiver at accept, before any verb, so it counts
    /// *connections*. `Cell` because the daemon is single-threaded by construction
    /// (§5) — the control plane and the data plane share one runtime thread.
    subscribers: Cell<usize>,
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

/// A per-boot instance nonce (§11.8). A fresh `RandomState` is seeded from the OS RNG
/// once per process, so hashing a per-process input yields a value that differs across
/// daemon restarts without pulling in a randomness dependency; process id and a
/// nanosecond timestamp are folded in as belt-and-suspenders so even an unlucky seed
/// collision resolves. Uniqueness across restarts — not cryptographic strength — is all
/// that is required: it exists only so a client detects an offset reset.
fn new_instance_nonce() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    if let Ok(d) = SystemTime::now().duration_since(UNIX_EPOCH) {
        h.write_u128(d.as_nanos());
    }
    h.finish()
}

impl Daemon {
    pub fn new(
        resolver: nexus_core::Resolver,
        state_file: Option<std::path::PathBuf>,
        registry: Rc<Registry>,
    ) -> Self {
        let (notifier, _) = broadcast::channel(NOTIFY_CAPACITY);
        Daemon {
            state: CriticalCell::new(GraphState {
                next_send_origin: Cell::new(SEND_ORIGIN_BASE),
                ..GraphState::default()
            }),
            shutdown: Notify::new(),
            notifier,
            resolver,
            registry,
            state_file,
            instance: new_instance_nonce(),
            subscribers: Cell::new(0),
        }
    }

    /// Snapshot the current configuration to the state file (§11/§15.9), atomically
    /// (tmp + rename) so a crash mid-write cannot corrupt it. Called after every
    /// successful config mutation. A write failure is logged but never fails the
    /// mutation or corrupts the running graph — the graph is authoritative; the
    /// file is a convenience for the next start (§15.9).
    fn snapshot_config(&self) {
        let Some(path) = &self.state_file else {
            return;
        };
        // Serialize inside the critical section; the borrow can't escape the closure.
        let text = match self.state.with(|st| toml::to_string(&st.config)) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("state snapshot serialize failed: {e}");
                return;
            }
        };
        if let Err(e) = atomic_write(path, text.as_bytes()) {
            tracing::warn!(path = %path.display(), "state snapshot write failed: {e}");
        }
    }

    /// A receiver for the `subscribe` stream (§10). Each subscribed connection
    /// holds one; the daemon publishes id-less notifications to all of them.
    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notifier.subscribe()
    }

    /// Count one connection as a subscriber (OBS-1). Called when a connection's
    /// `subscribe` verb succeeds, not when it merely connects — the distinction
    /// [`Self::emit_state_snapshot`] gates on.
    pub(crate) fn add_subscriber(&self) {
        self.subscribers.set(self.subscribers.get() + 1);
    }

    /// Drop one subscriber (the connection closed, or its task unwound).
    pub(crate) fn remove_subscriber(&self) {
        self.subscribers
            .set(self.subscribers.get().saturating_sub(1));
    }

    /// Connections currently subscribed to the notification stream (§10).
    pub(crate) fn subscriber_count(&self) -> usize {
        self.subscribers.get()
    }

    /// Publish a full state snapshot to subscribers (§10: status transitions and
    /// counter snapshots). A no-op when nobody is listening, so the periodic
    /// tick costs nothing on an unsubscribed daemon. This is the observability
    /// *floor*; lock transitions are additionally delivered as immediate `lock`
    /// notifications by the [`crate::runtime::LockCell`] (§10, §15.20).
    ///
    /// Gated on the *subscriber* tally, not `receiver_count()` (OBS-1): every
    /// connection takes a broadcast receiver at accept, so the receiver count is a
    /// connection count and a `serialnexusctl state` or a `tap`-only client would
    /// have had the whole graph serialized at 5 Hz on its behalf.
    pub fn emit_state_snapshot(&self) {
        if self.subscriber_count() == 0 {
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
        let result = match method {
            "load" => {
                // `replace` (§11) is read before the params move into the config parse.
                let replace = params
                    .as_ref()
                    .and_then(|p| p.get("replace"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.load(parse_config_param(params)?, replace)
            }
            "add-node" => self.add_node(params),
            "remove-node" => self.remove_node(params),
            "connect" => self.connect(params),
            "disconnect" => self.disconnect(params),
            "dump" => Ok(self.dump()),
            "state" => Ok(self.state()),
            "info" => Ok(self.info()),
            "ports" => Ok(self.ports()),
            // The stream itself is served by the connection task (control.rs);
            // dispatch just acknowledges the subscription (§10).
            "subscribe" => Ok(json!({ "subscribed": true })),
            "rotate" => self.rotate(params),
            "send-break" => self.send_break(params).await,
            "set-modem" => self.set_modem(params),
            "pulse-dtr" => self.pulse_dtr(params).await,
            "lock" => self.lock(params).await,
            "unlock" => self.unlock(params),
            "send" => self.send(params).await,
            "teardown" => Ok(self.teardown()),
            "shutdown" => {
                self.shutdown.notify_one();
                Ok(json!({ "shutting_down": true }))
            }
            other => Err(RpcError::method_not_found(other)),
        };
        // Persist configuration after a successful config mutation (§11). Read-only
        // verbs and the arbitration/rotate traffic never touch config, so they are
        // not snapshotted; clean shutdown (SIGTERM → `teardown_all`) bypasses
        // dispatch, so it preserves the graph for the next start rather than
        // persisting an empty one.
        if result.is_ok() && is_config_mutation(method) {
            self.snapshot_config();
        }
        result
    }

    /// Structural pre-check for every codec node's *name and attribute schema*
    /// (§8/§11/§15.26). Codec attributes are opaque to `GraphConfig::validate`, so
    /// this validates them here — **purely** (a codec factory / `exec::parse_attributes`
    /// only constructs and checks; no fds, no tasks) and **before** anything is
    /// created or torn down. That makes both an unknown codec name *and* a bad
    /// attribute table structural, nothing created, and — critically — caught
    /// before a `--replace` teardown, so a structurally-invalid config never
    /// destroys a good running graph (§15.26, and this method's caller in `load`).
    /// An unknown codec's error carries `data.available` so tools discover
    /// capabilities; a known codec's schema error carries the codec's own message.
    fn precheck_codecs(&self, nodes: &[nexus_core::config::NodeConfig]) -> Result<(), RpcError> {
        for nc in nodes {
            let nexus_core::config::NodeConfig::Codec {
                codec,
                name,
                attributes,
                ..
            } = nc
            else {
                continue;
            };
            // The exec codec is a child process, not a registry entry (§7.6); its
            // argv/env schema is validated here so a bad exec table is structural too.
            if codec == "exec" {
                if let Err(reason) = crate::nodes::exec::parse_attributes(attributes) {
                    return Err(structural_error(&format!("node {name:?}: {reason}"), None));
                }
                continue;
            }
            if !self.registry.contains(codec) {
                let available = self.registry.codec_names();
                let message =
                    format!("node {name:?}: unknown codec {codec:?}; available: {available:?}");
                return Err(structural_error(&message, Some(json!(available))));
            }
            // Known codec: validate its attribute schema by attempting to build it
            // (the factory is the schema; construction is pure and discarded here).
            if let Err(reason) = self.registry.build(codec, attributes) {
                return Err(structural_error(&format!("node {name:?}: {reason}"), None));
            }
        }
        Ok(())
    }

    /// `load` (§11): structurally atomic. Accepted only on an empty graph unless
    /// `replace`, which composes teardown-then-load (§11) so a full-file edit needs
    /// no manual teardown. A structural error creates nothing (and, under
    /// `replace`, is caught *before* the running graph is torn down, so a bad
    /// config never destroys a good one); environmental failures fault nodes
    /// without failing the load (§15.8).
    fn load(&self, config: GraphConfig, replace: bool) -> Result<Value, RpcError> {
        // Full structural validation before anything is created or torn down (§4,
        // §11): duplicate node names plus the three graph rules and name checks.
        let errors = config.validate();
        if !errors.is_empty() {
            let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
            return Err(RpcError::new(
                app_errors::STRUCTURAL,
                format!("structural error: {}", messages[0]),
            )
            .with_data(json!({ "errors": messages })));
        }

        // Codec names and attribute schemas are structural too (§8/§11/§15.26) but
        // opaque to `validate()`; check them here, before anything is created or
        // torn down, so a bad `--replace` config (unknown codec OR bad attributes)
        // never destroys a good graph.
        self.precheck_codecs(&config.nodes)?;

        // `--replace` clears the running graph first (teardown-then-load, §11). The
        // config is already validated, so this only fires for a config that will
        // load. `teardown` takes its own borrow, so run it before ours. Open taps
        // are told the graph was replaced, not that the daemon was torn down (§17).
        if replace {
            self.teardown_with(crate::tap::TapCloseReason::GraphReplaced);
        }

        self.state.with_mut(|st| {
            if !st.nodes.is_empty() {
                return Err(RpcError::new(
                    app_errors::LOAD_NONEMPTY,
                    "load requires an empty graph — teardown first (or use load --replace)",
                ));
            }

            // Instantiate nodes. An environmental failure faults the node (kept);
            // only an unimplemented node kind aborts the load, and then nothing is
            // committed.
            let mut nodes = Vec::with_capacity(config.nodes.len());
            for nc in &config.nodes {
                match Node::instantiate(nc, &self.resolver, &self.registry) {
                    Ok(node) => nodes.push(node),
                    Err(reason) => {
                        // Roll back the partially-built graph with the same
                        // signal-all-then-join shape as `teardown` (BND-1), so an
                        // aborted load does not stall the runtime thread for the sum
                        // of the already-started nodes' stop latencies.
                        for n in &mut nodes {
                            n.signal_stop();
                        }
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
            // handed to the origin read tasks below. Clear first so this stays a
            // replace (load runs on an empty graph, but the clears make that explicit).
            st.endpoint_locks.clear();
            st.endpoint_targetward.clear();
            st.origin_locks.clear();
            // …and the edge-surgery handles with them, symmetrically with `teardown`
            // (§15.35). Leaving these to be overwritten key-by-key by `absorb_wiring`
            // would let a new graph reusing an endpoint *name* inherit the old
            // endpoint's fan-out and edge slot, whose live sender still feeds a
            // consumer that no longer exists.
            st.host_fanout.clear();
            st.target_inbox.clear();
            st.target_counters.clear();
            st.target_edges.clear();
            // Any hub still here belongs to the outgoing graph; its taps must learn
            // their endpoint is gone rather than go quiet (§17, TAP-1). Under
            // `--replace` the teardown above already did this, so this is the
            // idempotent backstop for a `load` onto a graph that was emptied some
            // other way.
            st.detach_taps(crate::tap::TapCloseReason::GraphReplaced, |_| false);
            st.absorb_wiring(&wiring);
            spawn_tap_hubs(st, &mut wiring);
            st.nodes = nodes;
            st.config = config;
            for node in &mut st.nodes {
                node.start(&mut wiring);
            }
            Ok(json!({ "loaded": st.nodes.len() }))
        })
    }

    /// `add-node` (§10/§11): add one node to a running graph. The node arrives with
    /// no edges (wiring edges is the separate `connect` verb); its endpoints are
    /// wired self-contained — a host-facing endpoint gets its lock and targetward
    /// channel, a target-facing endpoint sits idle until connected. For a serial
    /// node, the device is resolved to a canonical identity at add time and echoed
    /// back (§12): a raw-path/serial add requires the device present; an identity
    /// add never does. Validated against the same structural rules as `load`
    /// (§11) — a duplicate name or illegal identity creates nothing.
    fn add_node(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node_val = params
            .as_ref()
            .and_then(|p| p.get("node"))
            .ok_or_else(|| RpcError::invalid_params("missing 'node' in params"))?;
        let mut node_cfg: nexus_core::config::NodeConfig = serde_json::from_value(node_val.clone())
            .map_err(|e| RpcError::invalid_params(format!("invalid node config: {e}")))?;

        // Resolve a serial node's device to a canonical identity at add time (§12),
        // before taking the state borrow (the resolver does filesystem I/O). The
        // captured identity replaces the operator input in config, so `dump`
        // round-trips it and the config survives a cold start.
        let mut echo = serde_json::Map::new();
        if let nexus_core::config::NodeConfig::Serial { device, .. } = &mut node_cfg {
            match self.resolver.resolve_input(device) {
                Ok(resolved) => {
                    *device = resolved.identity.clone();
                    echo.insert("identity".into(), json!(resolved.identity));
                    echo.insert("description".into(), json!(resolved.description));
                    echo.insert("kind".into(), json!(resolved.kind.label()));
                    echo.insert(
                        "resolved_path".into(),
                        json!(resolved.path.map(|p| p.display().to_string())),
                    );
                    if let Some(w) = resolved.warning {
                        echo.insert("warning".into(), json!(w));
                    }
                }
                Err(nexus_core::ResolveError::NotPresent { input }) => {
                    return Err(RpcError::new(
                        app_errors::DEVICE_ABSENT,
                        format!(
                            "device {input:?} is not present; add by a usb:/by-path: identity to configure it while absent (§12)"
                        ),
                    ));
                }
                Err(nexus_core::ResolveError::Malformed { input, reason }) => {
                    return Err(RpcError::invalid_params(format!(
                        "device {input:?}: {reason}"
                    )));
                }
            }
        }
        let node_name = node_cfg.name().to_owned();

        // Codec name + attribute schema are structural (§8/§11/§15.26): reject
        // before touching the running graph.
        self.precheck_codecs(std::slice::from_ref(&node_cfg))?;

        self.state.with_mut(|st| {
            // Validate the candidate graph (current + new node, edges unchanged) with
            // the same rules as `load` (§11): duplicate name, name/identity legality,
            // leg/codec config. Nothing is created on a structural error.
            let mut candidate = st.config.clone();
            candidate.nodes.push(node_cfg.clone());
            let errors = candidate.validate();
            if !errors.is_empty() {
                let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                return Err(RpcError::new(
                    app_errors::STRUCTURAL,
                    format!("structural error: {}", messages[0]),
                )
                .with_data(json!({ "errors": messages })));
            }

            // Instantiate the node (environmental failure faults it, §15.8; only a bad
            // codec kind/schema Errs, and then nothing is committed).
            let mut node =
                Node::instantiate(&node_cfg, &self.resolver, &self.registry).map_err(|reason| {
                    RpcError::invalid_params(format!("node {node_name}: {reason}"))
                })?;

            // Wire the node's own endpoints (no edges): build a partial plan from a
            // single-node config and merge its host-endpoint lock + targetward sender
            // into the daemon maps. `start` claims its endpoints from the plan.
            let mini = GraphConfig {
                nodes: vec![node_cfg.clone()],
                edges: Vec::new(),
            };
            let mut wiring = crate::runtime::Wiring::build(&mini, &self.notifier);
            st.absorb_wiring(&wiring);
            spawn_tap_hubs(st, &mut wiring);
            node.start(&mut wiring);
            st.nodes.push(node);
            st.config.nodes.push(node_cfg);

            let mut result = serde_json::Map::new();
            result.insert("added".into(), json!(node_name));
            result.append(&mut echo);
            Ok(Value::Object(result))
        })
    }

    /// `remove-node [--cascade]` (§10/§11): remove one node. Refused while edges are
    /// attached unless `--cascade`, which also removes those edges; removal tears
    /// down the node's environment (flushing a log queue within the bounded wait,
    /// §7.3), closes its endpoint locks so parked waiters leave with the defined
    /// error (§6/§15.20), and prunes it from the wiring maps. Surviving neighbors
    /// self-heal: a dropped channel simply stops delivering (a closed `try_send`).
    fn remove_node(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let name = params
            .as_ref()
            .and_then(|p| p.get("node"))
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'node' in params"))?
            .to_owned();
        let cascade = params
            .as_ref()
            .and_then(|p| p.get("cascade"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        self.state.with_mut(|st| {
            let idx = st.node_index(&name)?;

            // Edges touching this node (either endpoint). Refuse a non-cascade removal
            // while any remain (§11).
            let attached = st
                .config
                .edges
                .iter()
                .filter(|e| e.a.node == name || e.b.node == name)
                .count();
            if attached > 0 && !cascade {
                return Err(RpcError::new(
                    app_errors::HAS_EDGES,
                    format!("node {name:?} has {attached} attached edge(s); use --cascade"),
                ));
            }

            // The node's endpoint display addresses, from its config shape.
            let endpoints: Vec<String> = st
                .config
                .nodes
                .iter()
                .find(|n| n.name() == name)
                .map(|n| {
                    n.shape()
                        .endpoints
                        .iter()
                        .map(|ep| EndpointAddr::new(&name, ep.name.clone()).to_string())
                        .collect()
                })
                .unwrap_or_default();

            // Tear the node down (release env, flush log) and drop it. Signalling
            // stop before joining costs the runtime thread one node's stop latency
            // rather than a sum here, and keeps all three teardown paths one shape
            // (BND-1).
            // Undo each attached edge's runtime wiring first, through the *same*
            // helper `disconnect` uses (§15.35): the producer's sink goes, the
            // consumer's slot is cleared, and a lock-holding origin leaves cleanly.
            // Doing it here rather than relying on channel closure is what keeps a
            // surviving producer's fan-out from accumulating dead sinks across
            // add/remove cycles, and keeps the two removal paths from drifting.
            let cascaded: Vec<(String, String)> = st
                .config
                .edges
                .iter()
                .filter(|e| e.a.node == name || e.b.node == name)
                .filter_map(|e| {
                    st.config
                        .edge_ends_of(e)
                        .map(|(h, t)| (h.to_string(), t.to_string()))
                })
                .collect();
            for (host, target) in &cascaded {
                st.detach_edge_runtime(host, target);
            }

            let mut node = st.nodes.remove(idx);
            node.signal_stop();
            node.teardown();

            // Tell any tap on this node's endpoints that its endpoint is gone (§17,
            // TAP-1): the connection-side `OpenTap` handles outlive the hub, so a
            // bare `tap_hubs.remove` leaves the client silently receiving nothing.
            st.detach_taps(crate::tap::TapCloseReason::EndpointRemoved, |ep| {
                !endpoints.iter().any(|e| e == ep)
            });

            // Close and prune the node's host-endpoint locks (wake parked waiters), and
            // prune its targetward/origin entries. Keep the removed locks to also evict
            // surviving origins that fed them (their target is gone).
            let mut removed_locks: Vec<SharedLock> = Vec::new();
            for disp in &endpoints {
                if let Some(lock) = st.endpoint_locks.remove(disp) {
                    lock.close();
                    removed_locks.push(lock);
                }
                st.endpoint_targetward.remove(disp);
                // (The endpoint's tap hub was dropped by `detach_taps` above, which
                // also told its taps why; its ingest task self-terminates when the
                // node's teardown dropped the producer's feed sender.)
                // If this endpoint is a *writer* (a PTY/codec side that fed a
                // surviving host endpoint's lock), unregister it from that lock so it
                // does not linger as a phantom origin — or, if it held the lock,
                // permanently wedge the surviving endpoint as locked with no recovery
                // (§6/§15.20: a torn-down origin leaves the lock cleanly).
                if let Some((host_lock, origin_id)) = st.origin_locks.remove(disp) {
                    // Unregister the removed writer from the surviving host lock. It may
                    // have been the holder (detach-release) *or* a queued `lock --wait`
                    // waiter — `unregister` drops it from the FIFO queue either way. Wake
                    // unconditionally so a parked waiter for this now-gone origin
                    // re-evaluates and leaves with the defined error, rather than parking
                    // until the next unrelated wake of this surviving lock (§6/§15.20).
                    // Other waiters simply re-check `acquire` and re-park.
                    host_lock.with_mut(|g| g.unregister(origin_id));
                    host_lock.wake_waiters();
                    host_lock.emit_change();
                }
            }
            // A surviving origin whose target endpoint was this node keeps a clone of
            // that (now-removed) lock; evict it so `lock`/`send` no longer resolve to a
            // dead endpoint.
            st.origin_locks.retain(|_, (lock, _)| {
                !removed_locks.iter().any(|rl| std::rc::Rc::ptr_eq(rl, lock))
            });

            // Drop this node's per-endpoint edge-surgery handles (§15.35), which are
            // handles to endpoints the graph no longer has.
            st.forget_endpoints(&endpoints);

            // Drop the node's edges and the node itself from configuration.
            st.config
                .edges
                .retain(|e| e.a.node != name && e.b.node != name);
            st.config.nodes.retain(|n| n.name() != name);

            Ok(json!({ "removed": name, "cascaded_edges": attached }))
        })
    }

    /// `connect` (§10/§15.35): attach one edge to a running graph.
    ///
    /// The whole point is that this is *not* a special case. The candidate graph —
    /// current configuration plus the one new edge — runs the same
    /// `GraphConfig::validate` a `load` runs, so orientation pairing, the
    /// one-edge-per-target rule, the acyclicity rule, the codec-mux write-mode
    /// corollary and held-origin uniqueness are all enforced here with nothing
    /// created on a violation (§4, §6, §11). The runtime side is then a plain
    /// mutation: the producer's live fan-out gains a sink and the consumer's edge
    /// slot is filled, both under running tasks, so a mid-stream connect joins the
    /// stream at the join point rather than restarting anything.
    fn connect(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let edge = parse_edge_param(params.as_ref())?;
        self.state.with_mut(|st| {
            // Validate the candidate graph before touching anything (§11 atomicity).
            let mut candidate = st.config.clone();
            candidate.edges.push(edge.clone());
            let errors = candidate.validate();
            if !errors.is_empty() {
                let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                return Err(RpcError::new(
                    app_errors::STRUCTURAL,
                    format!("structural error: {}", messages[0]),
                )
                .with_data(json!({ "errors": messages })));
            }
            // A duplicate needs no check of its own: re-adding an existing edge gives
            // its target endpoint two, which §4 rule 2 refuses above by name. One
            // rule, one place (§16) — a second implementation here would be dead
            // code that only ever disagreed with the validator.

            // Post-validation the edge is host↔target; `effective_write_mode` is the
            // one implementation of the two configuration-to-runtime promotions
            // (§16, invariant 12) and is consulted here exactly as `Wiring::build`
            // consults it, so a live edge and a loaded one register the same mode.
            let (host, target) = match candidate.edge_ends_of(&edge) {
                Some(pair) => pair,
                None => {
                    return Err(RpcError::invalid_params(
                        "edge must join one host-facing endpoint to one target-facing endpoint",
                    ));
                }
            };
            let mode = candidate.effective_write_mode(&edge);
            // Keep the parsed addresses; the display forms key the daemon's maps.
            let (host_addr, target_addr) = (host.clone(), target.clone());
            let (host, target) = (host_addr.to_string(), target_addr.to_string());

            let fanout = st.host_fanout.get(&host).cloned().ok_or_else(|| {
                RpcError::invalid_params(format!("host-facing endpoint {host:?} is not wired"))
            })?;
            let inbox = st.target_inbox.get(&target).cloned().ok_or_else(|| {
                RpcError::invalid_params(format!("target-facing endpoint {target:?} is not wired"))
            })?;
            let slot = st.target_edges.get(&target).cloned().ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "target-facing endpoint {target:?} has no edge slot"
                ))
            })?;
            let counters = st.target_counters.get(&target).cloned().unwrap_or_default();

            // Register the origin on the host endpoint's lock before any byte can
            // flow, so arbitration is decided from the first chunk (§6).
            let origin_id = OriginId(st.next_edge_origin.get());
            st.next_edge_origin.set(origin_id.0 + 1);
            if let Some(lock) = st.endpoint_locks.get(&host) {
                lock.with_mut(|l| l.register(origin_id, target.clone(), mode));
                // Record the registration for *every* mode, so `disconnect` can undo
                // it; only a writable one also gets a targetward path and a
                // `lock`/`unlock` address (§6).
                let writer = (mode != WriteMode::Never)
                    .then(|| st.endpoint_targetward.get(&host).cloned())
                    .flatten();
                if writer.is_some() {
                    st.origin_locks
                        .insert(target.clone(), (lock.clone(), origin_id));
                }
                slot.with_mut(|e| {
                    e.attached = true;
                    e.registered = Some((lock.clone(), origin_id));
                    e.writer = writer;
                });
                // A `held` origin is granted the lock inside `register` when the
                // endpoint is free — a *fresh exclusive grant*, which §6 says runs
                // purge-on-acquire before the origin's bytes flow. At load that was
                // vacuous (nothing had written yet); on a running graph the new
                // origin may have a backlog from an earlier attachment, and firing it
                // is the stale-command hazard §6 exists to prevent. Synchronously,
                // at grant time, exactly as the `lock`/`send` verbs do.
                let granted = lock.with(|g| g.holder() == Some(origin_id));
                if granted {
                    let purged = st
                        .nodes
                        .iter()
                        .find(|n| n.name() == target_addr.node)
                        .map_or(0, |n| n.purge_origin());
                    if purged > 0 {
                        lock.with_mut(|g| g.record_purge(origin_id, purged));
                    }
                }
                // Wake the endpoint's waiters. A `held` origin registered while
                // someone else holds the lock is *not* granted by `register`, and its
                // pump may already be parked in `reacquire_held`; without a wake it
                // sits there until an unrelated transition happens to nudge the lock.
                lock.wake_waiters();
                lock.emit_change();
            } else {
                slot.with_mut(|e| e.attached = true);
            }

            // Hostward: the same channel `Wiring::build` would have made, at the same
            // depth (the producing node's consumer drop policy, §5/§7.1).
            let (htx, hrx) =
                mpsc::channel(crate::runtime::hostward_depth(&candidate, &host_addr.node));
            fanout.attach(crate::runtime::AttachedSink {
                target: target_addr.clone(),
                tx: htx,
                counters,
            });
            // Hand the consumer its receiver. A *full* inbox would mean the endpoint
            // already has an unconsumed edge, which validation just ruled out. A
            // *closed* one means the consuming endpoint has no pump at all — a pty
            // whose setup faulted, or a `faces = "host"` codec/exec channel, whose
            // driver is deferred (§14). That is the same dead edge `load` would have
            // produced for the same graph, so it is not a refusal (§15.8:
            // environmental failure never fails a configuration operation) — but it
            // is not silent either, because "connected" and "no bytes will ever flow"
            // must not look alike.
            let live = inbox.try_send(hrx).is_ok();
            if !live {
                tracing::warn!(
                    host = %host,
                    target = %target,
                    "connected an edge whose consumer has no running pump; no bytes \
                     will flow until that node comes up"
                );
            }

            // Let the consuming node update its own status (an interior node that was
            // `waiting` for an upstream is now `active`).
            if let Some(node) = st.nodes.iter_mut().find(|n| n.name() == target_addr.node) {
                node.edge_attached(&target_addr);
            }

            st.config.edges.push(edge);
            Ok(json!({
                "connected": { "a": host, "b": target },
                "write_mode": mode.to_string(),
                // Whether the consuming endpoint actually has a pump to receive on.
                // `false` is a real, reachable state (a faulted pty, a deferred
                // re-multiplexer orientation) and the caller is told rather than left
                // to infer it from silence.
                "consumer_live": live,
            }))
        })
    }

    /// `disconnect` (§10/§15.35): remove one edge from a running graph.
    ///
    /// The reverse of `connect`, with one clause that is not symmetric and is the
    /// whole reason this verb needs care: **a lock-holding origin must release and
    /// purge on its way out.** That is §15.27's phantom-holder lesson — a writer
    /// removed while holding an endpoint's lock leaves it wedged as locked by
    /// someone who no longer exists, with no recovery — applied to the edge
    /// operation before the bug could recur here (§6, §15.35).
    fn disconnect(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let edge = parse_edge_param(params.as_ref())?;
        self.state.with_mut(|st| {
            let Some(index) = st.config.edges.iter().position(|e| same_edge(e, &edge)) else {
                return Err(RpcError::invalid_params(format!(
                    "no edge between {} and {}",
                    edge.a, edge.b
                )));
            };
            let existing = st.config.edges[index].clone();
            let Some((host, target)) = st.config.edge_ends_of(&existing) else {
                // A same-facing or dangling edge cannot be in a validated config.
                st.config.edges.remove(index);
                return Ok(json!({ "disconnected": { "a": edge.a, "b": edge.b } }));
            };
            let (host, target) = (host.to_string(), target.to_string());

            let detached = st.detach_edge_runtime(&host, &target);
            let (released_lock, purged) = (detached.released_lock, detached.purged_bytes);
            st.config.edges.remove(index);
            Ok(json!({
                "disconnected": { "a": host, "b": target },
                // Reported rather than silent: an operator who disconnects a writer
                // that held the lock has changed who may write, and one whose client
                // had bytes queued has lost them by design (§5 all-loss-is-visible).
                "released_lock": released_lock,
                "purged_bytes": purged,
            }))
        })
    }

    /// `dump` (§11): configuration only, in exactly the load format. Returns the
    /// structured config; the CLI renders TOML.
    fn dump(&self) -> Value {
        self.state
            .with(|st| serde_json::to_value(&st.config).expect("config serializes"))
    }

    /// `state` (§10): observed status per node — never persisted, and disjoint
    /// from configuration by construction (§15.8).
    fn state(&self) -> Value {
        self.state.with(|st| {
            let nodes: Vec<Value> = st
                .nodes
                .iter()
                .map(|n| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("name".into(), json!(n.name()));
                    // §7's "a status of active | waiting | faulted with reason and
                    // timestamp": `NodeState` serializes flat as
                    // `{status, reason?, since_unix_ms}`, and carries no `name` of its
                    // own, so the merge lands all three beside the name with nothing
                    // shadowed (STATE-1).
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
                        let snap = serde_json::to_value(lock.with(|l| l.snapshot()))
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
            // Open taps, connection-scoped and read-only (§17): each reports its id,
            // the endpoint it observes, and its own drop counter (§5). Zero taps →
            // an empty array. Taps are state, never configuration, so `dump` is
            // unaffected (§8/§11).
            let mut taps = Vec::new();
            // Per-endpoint observation of the tap/ring machinery (§5, DM-5). The
            // endpoint-level `feed_dropped` used to be rendered *only* inside a per-tap
            // object, so on an endpoint with a ring and no open tap — the default for
            // every endpoint since §15.32 — the counter existed and nobody could read
            // it. §5's accounting doctrine is that loss is always visible; a counter
            // nobody can reach is not. Reported here where it is always reachable, and
            // still mirrored onto each tap for compatibility.
            let mut endpoints = Vec::new();
            // Sorted, so `state` reads the same twice running rather than in
            // `HashMap` order.
            let mut hubs: Vec<_> = st.tap_hubs.iter().collect();
            hubs.sort_by(|a, b| a.0.cmp(b.0));
            for (endpoint, hub) in hubs {
                let snap = hub.with(|h| h.snapshot());
                endpoints.push(json!({
                    "endpoint": endpoint,
                    "feed_dropped": snap.feed_dropped,
                    "taps": snap.taps.len(),
                }));
                for (id, dropped) in snap.taps {
                    taps.push(json!({
                        "tap": id,
                        "endpoint": endpoint,
                        "dropped": dropped,
                        "feed_dropped": snap.feed_dropped,
                    }));
                }
            }
            json!({ "nodes": nodes, "taps": taps, "endpoints": endpoints })
        })
    }

    /// `tap.open` (§17): register a connection-scoped, read-only tap on a
    /// host-facing endpoint. `out` is the serving connection's outbound tap channel;
    /// the hub delivers this endpoint's hostward bytes into it as [`TapMsg`]s, which
    /// the connection base64-frames into `tap.data` notifications. With `--replay`
    /// and a configured ring (§5), the ring snapshot is spliced in ahead of the live
    /// stream. Returns the RPC result plus an [`OpenTap`] handle the connection keeps
    /// so it can `tap.close` the tap or close it when the connection drops. Errors if
    /// the endpoint is unknown or not host-facing (only host-facing endpoints have a
    /// hub) — a tap observes a hostward stream, so a target-facing endpoint has none.
    pub(crate) fn tap_open(
        &self,
        params: Option<Value>,
        out: mpsc::Sender<crate::tap::TapMsg>,
    ) -> Result<(Value, crate::tap::OpenTap), RpcError> {
        let endpoint = params
            .as_ref()
            .and_then(|p| p.get("endpoint"))
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'endpoint' in params"))?;
        let replay = params
            .as_ref()
            .and_then(|p| p.get("replay"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.state.with_mut(|st| {
            let hub = st.tap_hubs.get(endpoint).cloned().ok_or_else(|| {
                RpcError::invalid_params(format!("no host-facing endpoint {endpoint:?} to tap"))
            })?;
            let tap_id = st.next_tap_id.get();
            st.next_tap_id.set(tap_id + 1);
            let dropped = Rc::new(Cell::new(0));
            let reg = hub.with_mut(|h| h.register(tap_id, out, dropped, replay));
            let result = json!({
                "tap": tap_id,
                "endpoint": endpoint,
                "replay_bytes": reg.replay_bytes,
                // The endpoint offset this tap's stream begins at (§11.8): a
                // reconnecting client trims replay overlap against the last offset it
                // stored, so a reload never duplicates ring bytes into its history.
                "from_offset": reg.from_offset,
                // Which offset space `from_offset` counts in (§11.8, §15.38). Unique
                // per hub instance and never reused, so a client holding stored
                // scrollback can tell an ordinary reconnect — where its frontier is
                // still meaningful and replay overlap must be trimmed — from a hub
                // rebuild (`load --replace`, `add-node`, `remove-node`), where offsets
                // restarted at 0 beneath it and the frontier means nothing. Both look
                // identical in offsets alone, which is why this is reported rather than
                // inferred. `info.instance` remains the *per-boot* nonce; this is the
                // per-endpoint-hub one it never claimed to be.
                "epoch": reg.epoch,
                // Bytes this endpoint has already lost at its producer→hub feed hop
                // (§5, TAP-1b). The offset space counts delivered bytes only — which
                // is what keeps replay's splice exact — so this is the baseline for
                // the `gap_before` deltas `tap.data` carries from here on.
                "feed_dropped": reg.feed_dropped,
            });
            Ok((result, crate::tap::OpenTap { tap_id, hub }))
        })
    }

    /// `info` (§10/§15.26): the daemon's capability surface — its version, the wire
    /// and envelope protocol versions, and the registered codec names — so tools
    /// (and a version-skewed CLI) discover what this possibly-custom daemon supports
    /// rather than assume it. An unknown codec in configuration fails structurally
    /// with this same list in its `data.available` (§8). Pure observation; touches
    /// no graph state.
    fn info(&self) -> Value {
        json!({
            "daemon_version": crate::VERSION,
            "wire_version": crate::WIRE_VERSION,
            "envelope_version": crate::ENVELOPE_VERSION,
            "codecs": self.registry.codec_names(),
            // Per-boot nonce (§11.8): tap byte offsets are only comparable within one
            // instance, so a client keys its stored history on this and starts fresh
            // when it changes across a restart.
            "instance": self.instance,
        })
    }

    /// `ports` (§12/§15.35): the resolver's enumeration face — every serial device
    /// on this machine, the identity that would bind it, and whether a node in the
    /// running graph already has it.
    ///
    /// **Passive.** The whole answer is readlinks, directory listings and sysfs
    /// reads ([`Resolver::enumerate_ports`](nexus_core::Resolver::enumerate_ports));
    /// no candidate device is opened, because opening a USB-serial adapter asserts
    /// DTR and resets the board behind it. That is a behavioural contract, not an
    /// implementation detail — `ports` is the verb an operator runs to *look*.
    ///
    /// `bound_to` answers "is this one already spoken for" by resolving each serial
    /// node's stored identity to its current path and matching on the path, so an
    /// adapter bound by `usb:` identity, by `by-path:`, or by a raw path all report
    /// bound — where comparing identity strings would only have caught the first.
    /// Both sides are canonicalized before the comparison, because the raw-path
    /// forms resolve *literally*: a node configured as
    /// `raw:/dev/serial/by-id/usb-FTDI_…` names the same device as the enumerated
    /// `/dev/ttyUSB0` through a symlink, and a textual comparison would report it
    /// free and invite an operator to bind it twice.
    fn ports(&self) -> Value {
        let candidates = self.resolver.enumerate_ports();
        self.state.with(|st| {
            // Each serial node's currently-resolved path, so the match below is a
            // path comparison rather than an identity-spelling comparison.
            let bound: Vec<(String, Option<std::path::PathBuf>, &str)> = st
                .config
                .nodes
                .iter()
                .filter_map(|n| match n {
                    nexus_core::config::NodeConfig::Serial { name, device, .. } => Some((
                        name.clone(),
                        self.resolver.resolve_current_path(device),
                        device.as_str(),
                    )),
                    _ => None,
                })
                .collect();
            // Compare through `canonicalize`, falling back to the literal path when it
            // fails (an absent device), so a symlinked raw path still matches.
            let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
            let bound: Vec<(String, Option<std::path::PathBuf>, &str)> = bound
                .into_iter()
                .map(|(name, path, device)| (name, path.map(|p| real(&p)), device))
                .collect();
            let ports: Vec<Value> = candidates
                .iter()
                .map(|c| {
                    let canonical = real(&c.path);
                    let holder = bound.iter().find(|(_, path, device)| {
                        path.as_deref() == Some(canonical.as_path()) || *device == c.identity
                    });
                    json!({
                        "identity": c.identity,
                        "kind": c.kind.label(),
                        "path": c.path.display().to_string(),
                        "description": c.description,
                        "by_id": c.by_id,
                        "warning": c.warning,
                        // The node that already binds this device, or null when it
                        // is a free candidate an `add-node` could take.
                        "bound_to": holder.map(|(name, _, _)| name.clone()),
                    })
                })
                .collect();
            json!({ "ports": ports })
        })
    }

    /// `rotate` (§7.3): rotate a log node's file on demand. Names the node in
    /// `params.node`; errors if it is unknown or not a log node.
    fn rotate(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node = params
            .as_ref()
            .and_then(|p| p.get("node"))
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("missing 'node' in params"))?;
        self.state.with(|st| {
            let target = st.node(node)?;
            match target.rotate() {
                Ok(rotated_to) => Ok(json!({ "node": node, "rotated_to": rotated_to })),
                Err(reason) => Err(RpcError::invalid_params(reason)),
            }
        })
    }

    /// Resolve a named serial node to its open port for a signal verb (§7.1),
    /// dropping the state borrow before the caller awaits (§15.20). Errors if the
    /// node is missing, not a serial node, or its device is not currently open.
    fn serial_port(&self, node: &str) -> Result<std::rc::Rc<serial2::SerialPort>, RpcError> {
        self.state.with(|st| {
            let target = st.node(node)?;
            let serial = target.as_serial().ok_or_else(|| {
                RpcError::invalid_params(format!("node {node:?} is not a serial node"))
            })?;
            serial.port().ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "serial node {node:?} has no open port (device absent/faulted)"
                ))
            })
        })
    }

    /// `send-break` (§7.1): assert a serial break on the named node for a duration.
    async fn send_break(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node = node_param(&params)?.to_owned();
        let ms = u64_param(&params, "ms").unwrap_or(250);
        let port = self.serial_port(&node)?;
        crate::nodes::serial::send_break(&port, ms)
            .await
            .map_err(|e| RpcError::invalid_params(format!("send-break on {node:?}: {e}")))?;
        Ok(json!({ "node": node, "break_ms": ms }))
    }

    /// `set-modem` (§7.1): drive DTR and/or RTS on the live port (a `null` line is
    /// left untouched). Acts on the live port only, not configuration (§15.8).
    fn set_modem(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node = node_param(&params)?.to_owned();
        let dtr = bool_param(&params, "dtr");
        let rts = bool_param(&params, "rts");
        let port = self.serial_port(&node)?;
        crate::nodes::serial::set_modem(&port, dtr, rts)
            .map_err(|e| RpcError::invalid_params(format!("set-modem on {node:?}: {e}")))?;
        Ok(json!({ "node": node, "dtr": dtr, "rts": rts }))
    }

    /// `pulse-dtr` (§7.1): pulse DTR to `assert` for a duration, then to `!assert`
    /// — the classic auto-reset toggle.
    async fn pulse_dtr(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let node = node_param(&params)?.to_owned();
        let ms = u64_param(&params, "ms").unwrap_or(100);
        let assert = bool_param(&params, "assert").unwrap_or(true);
        let port = self.serial_port(&node)?;
        crate::nodes::serial::pulse_dtr(&port, ms, assert)
            .await
            .map_err(|e| RpcError::invalid_params(format!("pulse-dtr on {node:?}: {e}")))?;
        Ok(json!({ "node": node, "pulse_ms": ms, "assert": assert }))
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
            let single = cell.with_mut(|g| g.acquire(id));
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
                    let generation = cell.with_mut(|g| g.renew(id));
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
        let released = cell.with_mut(|g| g.release(id));
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
        let (cell, sender) = self.state.with(|st| {
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
            // The endpoint advertises a targetward path, but that path can still be
            // dead — a node torn down while this endpoint survives, or any future node
            // kind that stops draining a host-facing endpoint. (Interior nodes are no
            // longer allowed to *cause* this: a codec, exec or map with no writable
            // path drains its receivers rather than dropping them, precisely because
            // dropping one closes the channel under live writer origins and kills a
            // pty's reader task — MAP-1's chain. The check stays as the honest
            // backstop.) A closed sender means a dead path: fail with a defined,
            // meaningful error here rather than registering the transient origin,
            // running the lock dance, and then failing opaquely at delivery with a
            // -32603 (RUNTIME-1).
            if sender.is_closed() {
                return Err(RpcError::invalid_params(format!(
                    "endpoint {:?} cannot accept targetward writes (its interior node has no writable path to the device)",
                    p.endpoint
                )));
            }
            Ok::<_, RpcError>((cell, sender))
        })?;
        let id = self.next_send_origin();
        // Register the transient origin (§6). The guard unregisters it on every
        // exit path — success, timeout, or a dropped connection — and wakes the
        // next waiter, so a cancelled `send` costs nothing but its queue slot.
        cell.with_mut(|g| g.register(id, "send", WriteMode::OnDemand));
        let guard = TransientOrigin {
            cell: cell.clone(),
            id,
            disarm: Cell::new(false),
        };

        // Acquire the floor (steal, or join the queue with a deadline).
        if p.steal {
            let _ = cell.with_mut(|g| g.steal(id));
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

        // Deliver the line targetward — a real backpressure point, but no state
        // borrow is held across this await (structurally, via `CriticalCell`; §16.2).
        let mut bytes = p.line.into_bytes();
        bytes.push(b'\n');
        let sent = bytes.len();
        let delivered = sender.send(Chunk::from(bytes)).await.is_ok();

        // Release + unregister the transient origin, then wake the next waiter.
        cell.with_mut(|g| g.unregister(id));
        cell.wake_waiters();
        cell.emit_change();
        guard.disarm.set(true);

        if delivered {
            Ok(json!({ "endpoint": p.endpoint, "sent": sent, "delivered": true }))
        } else {
            // The endpoint was torn down between the pre-flight serviceability check
            // and delivery (e.g. a concurrent `remove-node`). Report the same defined
            // teardown error the wait path uses, not an opaque -32603 (RUNTIME-1).
            Err(RpcError::new(
                app_errors::LOCKED,
                format!("endpoint {:?} was torn down while sending", p.endpoint),
            ))
        }
    }

    // --- arbitration helpers ---------------------------------------------------

    /// Resolve a `lock`/`unlock` origin name to its endpoint's lock and origin id
    /// (§6). The origin (a target-facing writer) feeds exactly one endpoint (§4).
    fn resolve_origin(&self, origin: &str) -> Result<(SharedLock, OriginId), RpcError> {
        self.state.with(|st| {
            st.origin_locks
                .get(origin)
                .map(|(cell, id)| (cell.clone(), *id))
                .ok_or_else(|| {
                    RpcError::invalid_params(format!(
                        "{origin:?} is not a writable origin on any endpoint"
                    ))
                })
        })
    }

    fn next_send_origin(&self) -> OriginId {
        self.state.with(|st| {
            let id = st.next_send_origin.get();
            st.next_send_origin.set(id.wrapping_add(1));
            OriginId(id)
        })
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
        let outcome = cell.with_mut(|g| g.steal(id));
        match outcome {
            Steal::ReadOnly => Err(RpcError::invalid_params(format!(
                "origin {origin:?} is write=never and cannot hold the lock"
            ))),
            // Stealing from yourself is a no-op in the state machine (LOCK-4): the
            // grant, the queue and the generation all survive, so the holder's own
            // in-flight bytes are *not* purged and its lease is not voided. Mirror
            // the `WaitOutcome::AlreadyHeld` arm above: re-arm a lease if asked
            // (renewing first so the prior, shorter timer cannot fire), no purge, and
            // report `acquired: false` because nothing changed hands.
            Steal::AlreadyHeld => {
                if let Some(ms) = lease_ms {
                    let generation = cell.with_mut(|g| g.renew(id));
                    if let Some(generation) = generation {
                        cell.emit_change();
                        self.spawn_lease(cell.clone(), id, generation, ms);
                    }
                }
                Ok(json!({
                    "origin": origin,
                    "held": true,
                    "acquired": false,
                    "stole_from": Value::Null,
                }))
            }
            Steal::Stolen { previous } => {
                let stole_from =
                    previous.and_then(|p| cell.with(|g| g.label(p).map(str::to_owned)));
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
        if cell.with(|g| g.arbitration()) != Arbitration::Exclusive {
            return;
        }
        let purged = self.state.with(|st| {
            st.nodes
                .iter()
                .find(|n| n.name() == origin)
                .map_or(0, |n| n.purge_origin())
        });
        if purged > 0 {
            cell.with_mut(|g| g.record_purge(id, purged));
        }
    }

    /// Common tail of a fresh grant: emit the immediate lock-change notification
    /// (§10) and, if a lease was requested, spawn a generation-guarded timer.
    fn after_grant(&self, cell: &SharedLock, id: OriginId, lease_ms: Option<u64>) {
        cell.emit_change();
        if let Some(ms) = lease_ms {
            let generation = cell.with(|g| g.generation());
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
            let fired = cell.with_mut(|g| {
                if g.holder() == Some(id) && g.generation() == generation {
                    g.release(id);
                    true
                } else {
                    false
                }
            });
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

            let settled = cell.with_mut(|g| match g.acquire(id) {
                Acquire::Granted => Some(WaitOutcome::Fresh),
                Acquire::AlreadyHeld => Some(WaitOutcome::AlreadyHeld),
                Acquire::ReadOnly => Some(WaitOutcome::ReadOnly),
                Acquire::Denied { .. } => {
                    g.enqueue(id);
                    None
                }
            });
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
        let holder = cell.with(|g| g.label(held_by).map(str::to_owned));
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
        self.teardown_with(crate::tap::TapCloseReason::Teardown)
    }

    /// `teardown` (§11), naming the reason its open taps are told (§17): a plain
    /// `teardown`/shutdown, or the teardown half of `load --replace`.
    ///
    /// **Stop is signalled to every node before any node is joined** (BND-1). A
    /// node's stop-join blocks the single runtime thread (`boundary::stop_join`
    /// parks until its reader thread observes the flag at its next poll wake), so
    /// joining them one at a time inside this one critical section cost the whole
    /// daemon — every console, every connection — the *sum* of the nodes' stop
    /// latencies. Signalling first lets the latencies overlap: the second pass
    /// joins threads that have already been told to leave. The alternative the
    /// review rejected — a shorter poll interval — buys the same latency with
    /// permanently more idle wakeups (§15.19).
    fn teardown_with(&self, reason: crate::tap::TapCloseReason) -> Value {
        self.state.with_mut(|st| {
            let count = st.nodes.len();
            // Close every endpoint lock first: a parked `lock --wait`/`send` waiter may
            // hold an `Rc` clone that outlives these map entries, so `close()` (which
            // wakes it) lets it leave the queue with the defined teardown error rather
            // than parking forever (§6/§15.20). Done before dropping the nodes so the
            // wake precedes any task teardown.
            for cell in st.endpoint_locks.values() {
                cell.close();
            }
            // Pass 1: tell every node to stop. Cheap and non-blocking.
            for n in &mut st.nodes {
                n.signal_stop();
            }
            // Pass 2: join them, now that they are all already unwinding.
            for mut n in st.nodes.drain(..) {
                n.teardown();
            }
            st.config = GraphConfig::default();
            st.endpoint_locks.clear();
            st.endpoint_targetward.clear();
            st.origin_locks.clear();
            // The edge-surgery handles go with them (§15.35): a stale fan-out or edge
            // slot for an endpoint the graph no longer has is a handle to nothing, and
            // leaving them would grow these maps across every `load --replace`.
            st.host_fanout.clear();
            st.target_inbox.clear();
            st.target_counters.clear();
            st.target_edges.clear();
            // Drop the tap hub handles (§17) — telling each open tap its endpoint is
            // gone, since the connection-side handles outlive this map (TAP-1). Each
            // ingest task self-terminates as the node teardowns above drop the
            // producers' feed senders.
            st.detach_taps(reason, |_| false);
            json!({ "torn_down": count })
        })
    }

    /// Tear down all nodes on clean shutdown (unlink PTY symlinks, drop ports).
    pub fn teardown_all(&self) {
        let _ = self.teardown();
    }
}

#[cfg(test)]
impl Daemon {
    /// Test seam: advertise one host-facing endpoint — its write lock and its
    /// targetward sender — without building a graph, so the control-plane tests can
    /// drive the *waiting* lane (`send`, `lock --wait`) directly. `GraphState` is
    /// private to this module, which is why the seam lives here rather than in
    /// `control.rs`'s test module.
    pub(crate) fn advertise_endpoint_for_test(
        &self,
        display: &str,
        lock: SharedLock,
        targetward: mpsc::Sender<Chunk>,
    ) {
        self.state.with_mut(|st| {
            st.endpoint_locks.insert(display.to_owned(), lock);
            st.endpoint_targetward
                .insert(display.to_owned(), targetward);
        });
    }

    /// Test seam: register one endpoint's tap hub, as `spawn_tap_hubs` would.
    pub(crate) fn advertise_tap_hub_for_test(&self, display: &str, hub: crate::tap::SharedTapHub) {
        self.state
            .with_mut(|st| st.tap_hubs.insert(display.to_owned(), hub));
    }
}

impl Default for Daemon {
    fn default() -> Self {
        // Production `/` resolver, the built-in codec registry, and no persistence;
        // tests that need a fixture root, a custom registry, or a state file call
        // `Daemon::new(..)` explicitly.
        Self::new(
            nexus_core::Resolver::new("/"),
            None,
            Rc::new(Registry::with_builtins()),
        )
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
        let free = self.cell.with_mut(|g| {
            g.dequeue(self.id);
            g.holder().is_none()
        });
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
        let free = self.cell.with_mut(|g| {
            g.unregister(self.id);
            g.holder().is_none()
        });
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

/// Spawn one tap-hub ingest task per host-facing endpoint in `wiring`, recording
/// each hub handle in the daemon state (§17). The task drains its feed channel — the
/// producer mirrors hostward bytes into it (only while a tap or ring wants them) —
/// and appends to the ring plus fans out to the registered taps. It self-terminates
/// when the producer's feed sender drops (node teardown/removal), so no join handle
/// is tracked; runs on the current-thread `LocalSet`, like every node task. Drains
/// `tap_hub_setup` so a later `node.start` (which claims `tap_feeds`) is unaffected.
fn spawn_tap_hubs(st: &mut GraphState, wiring: &mut crate::runtime::Wiring) {
    for (addr, setup) in std::mem::take(&mut wiring.tap_hub_setup) {
        let hub = setup.hub;
        let mut feed_rx = setup.feed_rx;
        st.tap_hubs.insert(addr.to_string(), hub.clone());
        tokio::task::spawn_local(async move {
            while let Some(chunk) = feed_rx.recv().await {
                hub.with_mut(|h| h.ingest(&chunk));
            }
        });
    }
}

/// Build a structural error (`app_errors::STRUCTURAL`) with `data.errors = [msg]`,
/// plus `data.available = [...]` when the failure is an unknown codec (§8/§15.26).
fn structural_error(message: &str, available: Option<Value>) -> RpcError {
    let mut data = serde_json::Map::new();
    data.insert("errors".into(), json!([message]));
    if let Some(available) = available {
        data.insert("available".into(), available);
    }
    RpcError::new(app_errors::STRUCTURAL, message.to_owned()).with_data(Value::Object(data))
}

/// Whether a verb changes configuration and so warrants a state-file snapshot
/// (§11). Read-only verbs (`state`, `dump`, `subscribe`), arbitration (`lock`/
/// `unlock`/`send`), `rotate`, and `shutdown` never touch config.
fn is_config_mutation(method: &str) -> bool {
    matches!(
        method,
        // Edge surgery is configuration too (§15.35): without these two here, a
        // rewiring an operator just performed would evaporate on the next restart
        // and the persisted config would silently diverge from `dump` — a
        // fail-*silent* shape, which is why it is spelled out rather than assumed.
        "load" | "teardown" | "add-node" | "remove-node" | "connect" | "disconnect"
    )
}

/// Whether two edges join the same endpoint pair, in either order. An edge is
/// undirected in configuration — orientation comes from the endpoints' facings
/// (§15.3) — so `disconnect` must find the edge however the operator spells it.
fn same_edge(x: &nexus_core::config::EdgeConfig, y: &nexus_core::config::EdgeConfig) -> bool {
    (x.a == y.a && x.b == y.b) || (x.a == y.b && x.b == y.a)
}

/// Parse `connect`/`disconnect` params into an [`EdgeConfig`](nexus_core::config::EdgeConfig)
/// — the same `[[edge]]` shape `load` accepts, so an operator writes one thing.
/// `write_mode` is optional and defaults exactly as configuration does.
fn parse_edge_param(params: Option<&Value>) -> Result<nexus_core::config::EdgeConfig, RpcError> {
    let params = params.ok_or_else(|| RpcError::invalid_params("missing params"))?;
    // Accept both the flat `{a, b, write_mode}` spelling and a nested `{edge: {…}}`,
    // since the latter is what a caller holding a `[[edge]]` table already has.
    let value = params.get("edge").unwrap_or(params);
    let mut obj = value
        .as_object()
        .cloned()
        .ok_or_else(|| RpcError::invalid_params("edge params must be an object"))?;
    // An explicit `null` write_mode means "unspecified", not "invalid": the CLI and
    // the web editor both send the field unconditionally.
    if obj.get("write_mode").is_some_and(Value::is_null) {
        obj.remove("write_mode");
    }
    // `EdgeConfig` is `deny_unknown_fields`, so a typo is named rather than ignored
    // — the §11 rule applied to an edge operation (§15.35).
    serde_json::from_value(Value::Object(obj))
        .map_err(|e| RpcError::invalid_params(format!("invalid edge: {e}")))
}

/// Write `bytes` to `path` atomically: create the parent directory, write a
/// sibling temp file, then rename over the target (atomic on one filesystem), so a
/// crash mid-write leaves the previous snapshot intact (§11/§15.9).
///
/// Durable against power loss too (§16.6): the temp file is fsynced *before* the
/// rename — a rename that reaches disk while the file's bytes do not would yield a
/// truncated snapshot after an outage — and the parent directory is fsynced *after*
/// the rename, so the rename itself is durable. Config mutations are rare, so the
/// two fsyncs cost nothing measurable; what they remove is a corrupt state file.
///
/// **Mode 0600, explicitly** (SEC-4). The snapshot carries exec-codec argv and
/// environment, and it named the consoles a reader could then go and tap; inheriting
/// the umask published it at 0664 on a default box. The mode is set on the *temp*
/// file, which is the inode the rename installs, so the permissive window is closed
/// too — and re-applied to the open handle in case a stale temp sibling survived a
/// crash with a wider mode. The packaged unit's `StateDirectoryMode=0700` stays as
/// the outer layer; this is the file's own.
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => {
            std::fs::create_dir_all(d)?;
            d
        }
        // No parent (a bare filename): the directory is the current one.
        _ => std::path::Path::new("."),
    };
    let tmp = {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        path.with_file_name(name)
    };
    // Write and fsync the temp file's contents to disk before the rename.
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(STATE_FILE_MODE)
            .open(&tmp)?;
        // `mode` applies only at creation; a leftover temp sibling from a crash
        // would keep its old (possibly umask-wide) mode, so narrow it explicitly.
        f.set_permissions(std::fs::Permissions::from_mode(STATE_FILE_MODE))?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    // fsync the directory so the rename entry is durable. Best-effort: a filesystem
    // that rejects a directory fsync must not fail an otherwise-successful snapshot.
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Extract the required `node` string from a node-targeted verb's params.
fn node_param(params: &Option<Value>) -> Result<&str, RpcError> {
    params
        .as_ref()
        .and_then(|p| p.get("node"))
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("missing 'node' in params"))
}

/// An optional `u64` params field.
fn u64_param(params: &Option<Value>, key: &str) -> Option<u64> {
    params
        .as_ref()
        .and_then(|p| p.get(key))
        .and_then(Value::as_u64)
}

/// An optional `bool` params field.
fn bool_param(params: &Option<Value>, key: &str) -> Option<bool> {
    params
        .as_ref()
        .and_then(|p| p.get(key))
        .and_then(Value::as_bool)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("snx-atomic-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// RUNTIME-1 regression: a `send` to a host endpoint whose targetward receiver
    /// has been dropped — the state an interior codec/exec channel leaves behind
    /// when it cannot route targetward (read-only or unattached mux) — fails fast
    /// with a *defined* `invalid_params` error, not an opaque `-32603` internal
    /// error, and without running the lock dance first.
    #[test]
    fn send_to_unserviceable_endpoint_fails_with_defined_error() {
        use crate::runtime::LockCell;
        use nexus_core::lock::EndpointLock;

        let daemon = Daemon::new(
            nexus_core::Resolver::new("/"),
            None,
            Rc::new(Registry::with_builtins()),
        );

        // Advertise a host endpoint (a lock + a targetward sender) whose receiver is
        // already gone — exactly what codec.start leaves when it drops the per-channel
        // rx on the Waiting / read-only path.
        let (notifier, _keep) = broadcast::channel::<Notification>(4);
        let lock = Rc::new(LockCell::new(
            "mux/console",
            EndpointLock::new(Arbitration::Exclusive),
            notifier,
        ));
        let (tx, targetward_rx) = mpsc::channel::<Chunk>(1);
        drop(targetward_rx); // interior node dropped its receiver: tx.is_closed()
        daemon.state.with_mut(|st| {
            st.endpoint_locks.insert("mux/console".to_owned(), lock);
            st.endpoint_targetward.insert("mux/console".to_owned(), tx);
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(daemon.dispatch(
                "send",
                Some(json!({ "endpoint": "mux/console", "line": "reboot" })),
            ))
            .expect_err("send to a closed targetward endpoint must fail");
        assert_eq!(
            err.code,
            error_codes::INVALID_PARAMS,
            "expected a defined invalid_params error, not an opaque internal (-32603)"
        );
    }

    /// TAP-1: `teardown` (and, by the same path, `load --replace` and
    /// `remove-node`) drops the endpoint's tap hub while the connection-side
    /// `OpenTap` handles survive. Every open tap must be told — a terminal
    /// `tap.closed` on its own channel naming the endpoint and the reason — and the
    /// hub must remember it was orphaned, so the client's next `tap.close` fails
    /// instead of reporting a success that closed nothing. It must also disappear
    /// from `state.taps`, which it already did; the silence was the defect.
    #[test]
    fn teardown_tells_open_taps_their_endpoint_is_gone() {
        let daemon = Daemon::default();
        let (hub, _active, _fd) = crate::tap::TapHub::new("usb0", 64);
        daemon.advertise_tap_hub_for_test("usb0", hub.clone());

        let (tx, mut rx) = mpsc::channel::<crate::tap::TapMsg>(8);
        let (result, handle) = daemon
            .tap_open(Some(json!({ "endpoint": "usb0" })), tx)
            .expect("the endpoint has a hub");
        let tap_id = result["tap"].as_u64().unwrap();
        assert_eq!(result["feed_dropped"], json!(0));

        let torn = daemon.teardown();
        assert_eq!(torn, json!({ "torn_down": 0 }));

        match rx.try_recv() {
            Ok(crate::tap::TapMsg::Closed {
                tap_id: id,
                endpoint,
                reason,
            }) => {
                assert_eq!(id, tap_id);
                assert_eq!(endpoint, "usb0");
                assert_eq!(reason, "teardown");
            }
            _ => panic!("an orphaned tap must be told its endpoint is gone"),
        }
        // The surviving handle knows it is dead, which is what turns the next
        // `tap.close` into an error instead of a hollow success (control.rs).
        assert!(handle.is_orphaned());
        assert_eq!(handle.endpoint(), "usb0");
        assert_eq!(daemon.state()["taps"], json!([]));
    }

    /// DM-5: endpoint-level `feed_dropped` used to be rendered only inside a per-tap
    /// object, so on a ring-only endpoint — the default for *every* endpoint since
    /// §15.32 — the counter could not be read at all. §5's doctrine is that loss is
    /// always visible.
    #[test]
    fn feed_dropped_is_readable_on_an_endpoint_with_no_open_tap() {
        let daemon = Daemon::default();
        let (hub, _active, feed_dropped) = crate::tap::TapHub::new("usb0", 64);
        daemon.advertise_tap_hub_for_test("usb0", hub);
        feed_dropped.fetch_add(4096, std::sync::atomic::Ordering::Relaxed);

        let state = daemon.state();
        assert_eq!(state["taps"], json!([]), "no tap is open");
        assert_eq!(
            state["endpoints"],
            json!([{ "endpoint": "usb0", "feed_dropped": 4096, "taps": 0 }]),
            "the endpoint's own loss must be reachable without a tap"
        );
    }

    /// OBS-1: the 5 Hz full-graph snapshot must be gated on connections that
    /// actually issued `subscribe`, not on broadcast receivers — every connection
    /// takes a receiver at accept, before any verb.
    #[test]
    fn state_snapshots_are_gated_on_subscribers_not_connections() {
        let daemon = Daemon::default();
        // A merely-connected client: it holds a receiver but never subscribed.
        let mut rx = daemon.subscribe();
        daemon.emit_state_snapshot();
        assert!(
            rx.try_recv().is_err(),
            "an unsubscribed connection must not cost a snapshot"
        );

        daemon.add_subscriber();
        daemon.emit_state_snapshot();
        assert!(rx.try_recv().is_ok(), "a subscriber gets the snapshot");

        daemon.remove_subscriber();
        assert_eq!(daemon.subscriber_count(), 0);
        daemon.emit_state_snapshot();
        assert!(rx.try_recv().is_err(), "and the tally comes back down");
    }

    /// SEC-4: the configuration snapshot carries exec-codec argv and environment and
    /// names every console, so it is created owner-only rather than at the umask's
    /// discretion (0664 was observed). The temp sibling gets the same mode, so the
    /// permissive window is closed too.
    #[test]
    fn state_file_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir();
        let target = dir.join("state.toml");
        let tmp = dir.join("state.toml.tmp");

        // Plant a world-readable stale temp sibling: a crash could leave one, and
        // the rename would otherwise install its mode.
        std::fs::write(&tmp, b"stale").unwrap();
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o666)).unwrap();

        atomic_write(&target, b"secret = true").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, STATE_FILE_MODE,
            "state file must be 0600, got {mode:o}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `atomic_write` durably replaces the target: the bytes round-trip, the temp
    /// sibling is consumed by the rename (no partial file left behind), and a second
    /// write overwrites cleanly.
    ///
    /// **Comment-pinned (§16.6).** The durability path is: fsync the temp file
    /// *before* the rename, then fsync the parent directory *after* it. Those
    /// `sync_all` syscalls are not directly observable from a unit test, so this
    /// asserts the observable atomic-write contract; the `sync_all` calls themselves
    /// are pinned by this note against a future refactor that drops them (a
    /// `strace -e trace=fsync,rename` on this test shows both fsyncs, as an optional
    /// spot check).
    #[test]
    fn atomic_write_replaces_durably() {
        let dir = scratch_dir();
        let target = dir.join("state.toml");
        let tmp = dir.join("state.toml.tmp");

        atomic_write(&target, b"first").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        assert!(!tmp.exists(), "the rename must consume the temp sibling");

        // A second write atomically replaces the contents (different length, so a
        // non-atomic partial write would be detectable).
        atomic_write(&target, b"second-and-longer").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second-and-longer");
        assert!(!tmp.exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
