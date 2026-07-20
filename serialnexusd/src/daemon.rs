//! The daemon's graph state and the RPC method implementations (design §10,
//! §11). Mutations run on the current-thread runtime, so a `RefCell` serializes
//! them with no locks (plan §2). Verbs: `load`/`dump`/`state`/`teardown`/
//! `shutdown` (phase 2, load-on-empty + structural atomicity) plus `rotate` and
//! `subscribe` (phase 3).

use std::cell::RefCell;
use std::collections::HashMap;

use nexus_core::config::GraphConfig;
use nexus_core::lock::{Acquire, OriginId};
use nexus_rpc::{Notification, RpcError, error_codes};
use serde_json::{Value, json};
use tokio::sync::{Notify, broadcast};

use crate::nodes::Node;
use crate::runtime::SharedLock;

/// Depth of the notification broadcast buffer (§10 `subscribe`). A subscriber
/// that falls this far behind sees a `Lagged` skip rather than blocking the
/// daemon — state snapshots are cumulative, so a dropped one loses nothing.
const NOTIFY_CAPACITY: usize = 64;

/// Daemon-specific error codes, in the reserved application range (§10).
pub mod app_errors {
    use nexus_rpc::error_codes::APP_ERROR_BASE;
    /// `load` attempted on a non-empty graph (§11 load-on-empty).
    pub const LOAD_NONEMPTY: i64 = APP_ERROR_BASE - 1;
    /// A structural validation failure (§4).
    pub const STRUCTURAL: i64 = APP_ERROR_BASE - 2;
    /// A `lock`/`send` was refused because another origin holds the endpoint's
    /// write lock (§6).
    pub const LOCKED: i64 = APP_ERROR_BASE - 3;
}

#[derive(Default)]
struct GraphState {
    config: GraphConfig,
    nodes: Vec<Node>,
    /// Host-facing endpoint (serial node name) → its write lock (§6), shared with
    /// the origin read tasks. The daemon mutates it on `lock`/`unlock`.
    endpoint_locks: HashMap<String, SharedLock>,
    /// Writing origin (PTY node name) → (its endpoint's lock, its origin id), for
    /// resolving a `lock`/`unlock` by origin name to the right lock (§6).
    origin_locks: HashMap<String, (SharedLock, OriginId)>,
}

/// The running daemon: graph state, a shutdown signal, and the `subscribe`
/// notification broadcast.
pub struct Daemon {
    state: RefCell<GraphState>,
    pub shutdown: Notify,
    notifier: broadcast::Sender<Notification>,
}

impl Daemon {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(NOTIFY_CAPACITY);
        Daemon {
            state: RefCell::new(GraphState::default()),
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
    /// tick costs nothing on an unsubscribed daemon.
    pub fn emit_state_snapshot(&self) {
        if self.notifier.receiver_count() == 0 {
            return;
        }
        let snapshot = self.state();
        let _ = self
            .notifier
            .send(Notification::new("state", Some(snapshot)));
    }

    /// Route one RPC method to its implementation (§10 verb surface).
    pub fn dispatch(&self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        match method {
            "load" => self.load(parse_config_param(params)?),
            "dump" => Ok(self.dump()),
            "state" => Ok(self.state()),
            // The stream itself is served by the connection task (control.rs);
            // dispatch just acknowledges the subscription (§10).
            "subscribe" => Ok(json!({ "subscribed": true })),
            "rotate" => self.rotate(params),
            "lock" => self.lock(params),
            "unlock" => self.unlock(params),
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

        // Full structural validation before anything is created (§4, §11).
        let errors = config.to_model().validate();
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
        // clean; `start` spawns onto the current-thread LocalSet.
        let mut wiring = crate::runtime::Wiring::build(&config);
        // Keep clones of the write locks (§6) so the control plane can acquire and
        // release them; the same `Rc`s are handed to the origin read tasks below.
        st.endpoint_locks = wiring.endpoint_locks.clone();
        st.origin_locks = wiring.origin_locks.clone();
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
                // A host-facing endpoint reports its write-lock state (§6: holder,
                // waiters, per-origin purge counters). Observed state, disjoint
                // from configuration (§15.8).
                if let Some(lock) = st.endpoint_locks.get(n.name()) {
                    obj.insert(
                        "lock".into(),
                        serde_json::to_value(lock.borrow().snapshot())
                            .expect("lock snapshot serializes"),
                    );
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

    /// `lock` (§6): a named origin explicitly acquires its endpoint's exclusive
    /// write lock. A fresh grant makes the origin the sole writer targetward;
    /// another origin holding it is refused with [`app_errors::LOCKED`], naming
    /// the holder. `--steal`, `--wait`, and `--lease` land in a later slice.
    fn lock(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let origin = origin_param(&params)?;
        let st = self.state.borrow();
        let (lock, id) = st.origin_locks.get(origin).ok_or_else(|| {
            RpcError::invalid_params(format!(
                "{origin:?} is not a writable origin on any endpoint"
            ))
        })?;
        // Bind the outcome so the mutable borrow is released before the label
        // lookup below (a Denied read-borrows the same lock).
        let outcome = lock.borrow_mut().acquire(*id);
        match outcome {
            Acquire::Granted => Ok(json!({ "origin": origin, "held": true, "acquired": true })),
            Acquire::AlreadyHeld => {
                Ok(json!({ "origin": origin, "held": true, "acquired": false }))
            }
            Acquire::Denied { held_by } => {
                let holder = lock.borrow().label(held_by).map(str::to_owned);
                Err(RpcError::new(
                    app_errors::LOCKED,
                    format!(
                        "endpoint is locked by {}",
                        holder.as_deref().unwrap_or("another origin")
                    ),
                )
                .with_data(json!({ "held_by": holder })))
            }
            Acquire::ReadOnly => Err(RpcError::invalid_params(format!(
                "origin {origin:?} is write=never and cannot hold the lock"
            ))),
        }
    }

    /// `unlock` (§6): release the endpoint's write lock if the named origin holds
    /// it. Releasing when you do not hold it is reported, not an error.
    fn unlock(&self, params: Option<Value>) -> Result<Value, RpcError> {
        let origin = origin_param(&params)?;
        let st = self.state.borrow();
        let (lock, id) = st.origin_locks.get(origin).ok_or_else(|| {
            RpcError::invalid_params(format!(
                "{origin:?} is not a writable origin on any endpoint"
            ))
        })?;
        let released = lock.borrow_mut().release(*id);
        Ok(json!({ "origin": origin, "released": released }))
    }

    fn teardown(&self) -> Value {
        let mut st = self.state.borrow_mut();
        let count = st.nodes.len();
        for mut n in st.nodes.drain(..) {
            n.teardown();
        }
        st.config = GraphConfig::default();
        st.endpoint_locks.clear();
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

fn merge_into(target: &mut serde_json::Map<String, Value>, source: Value) {
    if let Value::Object(m) = source {
        for (k, v) in m {
            target.insert(k, v);
        }
    }
}

/// Extract the required `origin` string from a `lock`/`unlock` request's params.
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
