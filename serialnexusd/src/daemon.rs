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
    /// `remove-node` refused because the node still has attached edges and
    /// `--cascade` was not given (§11).
    pub const HAS_EDGES: i64 = APP_ERROR_BASE - 4;
    /// `add-node` by raw path or serial number failed because the device is not
    /// present, so its identity cannot be captured (§12).
    pub const DEVICE_ABSENT: i64 = APP_ERROR_BASE - 5;
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
    state: CriticalCell<GraphState>,
    pub shutdown: Notify,
    notifier: broadcast::Sender<Notification>,
    /// Device-identity resolver (§12), rooted at `--dev-root` (a fixture seam in
    /// tests; `/` in production). Shared read-only with every serial node.
    resolver: nexus_core::Resolver,
    /// The configuration snapshot path (§11): after every successful *config*
    /// mutation the daemon writes the current config here (atomically), and startup
    /// prefers it, so incremental surgery survives a daemon restart. `None` disables
    /// persistence (unit tests via [`Daemon::default`]).
    state_file: Option<std::path::PathBuf>,
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
    pub fn new(resolver: nexus_core::Resolver, state_file: Option<std::path::PathBuf>) -> Self {
        let (notifier, _) = broadcast::channel(NOTIFY_CAPACITY);
        Daemon {
            state: CriticalCell::new(GraphState {
                next_send_origin: Cell::new(SEND_ORIGIN_BASE),
                ..GraphState::default()
            }),
            shutdown: Notify::new(),
            notifier,
            resolver,
            state_file,
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
            "dump" => Ok(self.dump()),
            "state" => Ok(self.state()),
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

        // `--replace` clears the running graph first (teardown-then-load, §11). The
        // config is already validated, so this only fires for a config that will
        // load. `teardown` takes its own borrow, so run it before ours.
        if replace {
            self.teardown();
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
                match Node::instantiate(nc, &self.resolver) {
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
            let mut node = Node::instantiate(&node_cfg, &self.resolver).map_err(|reason| {
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
            for (a, l) in wiring.endpoint_locks.iter() {
                st.endpoint_locks.insert(a.to_string(), l.clone());
            }
            for (a, t) in wiring.host_targetward_tx.iter() {
                st.endpoint_targetward.insert(a.to_string(), t.clone());
            }
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
            let idx = st
                .nodes
                .iter()
                .position(|n| n.name() == name)
                .ok_or_else(|| RpcError::invalid_params(format!("unknown node {name:?}")))?;

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

            // Tear the node down (release env, flush log) and drop it.
            let mut node = st.nodes.remove(idx);
            node.teardown();

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
                // If this endpoint is a *writer* (a PTY/codec side that fed a
                // surviving host endpoint's lock), unregister it from that lock so it
                // does not linger as a phantom origin — or, if it held the lock,
                // permanently wedge the surviving endpoint as locked with no recovery
                // (§6/§15.20: a torn-down origin leaves the lock cleanly).
                if let Some((host_lock, origin_id)) = st.origin_locks.remove(disp) {
                    let released = host_lock.with_mut(|g| g.unregister(origin_id));
                    if released {
                        // It held the lock; releasing must wake the queue and notify,
                        // exactly like a normal detach-release.
                        host_lock.wake_waiters();
                        host_lock.emit_change();
                    }
                }
            }
            // A surviving origin whose target endpoint was this node keeps a clone of
            // that (now-removed) lock; evict it so `lock`/`send` no longer resolve to a
            // dead endpoint.
            st.origin_locks.retain(|_, (lock, _)| {
                !removed_locks.iter().any(|rl| std::rc::Rc::ptr_eq(rl, lock))
            });

            // Drop the node's edges and the node itself from configuration.
            st.config
                .edges
                .retain(|e| e.a.node != name && e.b.node != name);
            st.config.nodes.retain(|n| n.name() != name);

            Ok(json!({ "removed": name, "cascaded_edges": attached }))
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
            json!({ "nodes": nodes })
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
            let target = st
                .nodes
                .iter()
                .find(|n| n.name() == node)
                .ok_or_else(|| RpcError::invalid_params(format!("unknown node {node:?}")))?;
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
            let target = st
                .nodes
                .iter()
                .find(|n| n.name() == node)
                .ok_or_else(|| RpcError::invalid_params(format!("unknown node {node:?}")))?;
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
            for mut n in st.nodes.drain(..) {
                n.teardown();
            }
            st.config = GraphConfig::default();
            st.endpoint_locks.clear();
            st.endpoint_targetward.clear();
            st.origin_locks.clear();
            json!({ "torn_down": count })
        })
    }

    /// Tear down all nodes on clean shutdown (unlink PTY symlinks, drop ports).
    pub fn teardown_all(&self) {
        let _ = self.teardown();
    }
}

impl Default for Daemon {
    fn default() -> Self {
        // Production `/` resolver and no persistence; tests that need a fixture
        // root or a state file call `Daemon::new(..)` explicitly.
        Self::new(nexus_core::Resolver::new("/"), None)
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

/// Whether a verb changes configuration and so warrants a state-file snapshot
/// (§11). Read-only verbs (`state`, `dump`, `subscribe`), arbitration (`lock`/
/// `unlock`/`send`), `rotate`, and `shutdown` never touch config.
fn is_config_mutation(method: &str) -> bool {
    matches!(
        method,
        "load" | "load-replace" | "teardown" | "add-node" | "remove-node"
    )
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
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
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
        let mut f = std::fs::File::create(&tmp)?;
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
