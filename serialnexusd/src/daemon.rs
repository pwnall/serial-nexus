//! The daemon's graph state and the RPC method implementations (design §10,
//! §11). Mutations run on the current-thread runtime, so a `RefCell` serializes
//! them with no locks (plan §2). Phase 2 implements `load`/`dump`/`state`/
//! `teardown`/`shutdown` with load-on-empty and structural atomicity.

use std::cell::RefCell;

use nexus_core::config::GraphConfig;
use nexus_rpc::{RpcError, error_codes};
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::nodes::Node;

/// Daemon-specific error codes, in the reserved application range (§10).
pub mod app_errors {
    use nexus_rpc::error_codes::APP_ERROR_BASE;
    /// `load` attempted on a non-empty graph (§11 load-on-empty).
    pub const LOAD_NONEMPTY: i64 = APP_ERROR_BASE - 1;
    /// A structural validation failure (§4).
    pub const STRUCTURAL: i64 = APP_ERROR_BASE - 2;
}

#[derive(Default)]
struct GraphState {
    config: GraphConfig,
    nodes: Vec<Node>,
}

/// The running daemon: graph state plus a shutdown signal.
pub struct Daemon {
    state: RefCell<GraphState>,
    pub shutdown: Notify,
}

impl Daemon {
    pub fn new() -> Self {
        Daemon {
            state: RefCell::new(GraphState::default()),
            shutdown: Notify::new(),
        }
    }

    /// Route one RPC method to its implementation (§10 verb surface).
    pub fn dispatch(&self, method: &str, params: Option<Value>) -> Result<Value, RpcError> {
        match method {
            "load" => self.load(parse_config_param(params)?),
            "dump" => Ok(self.dump()),
            "state" => Ok(self.state()),
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
                Value::Object(obj)
            })
            .collect();
        json!({ "nodes": nodes })
    }

    fn teardown(&self) -> Value {
        let mut st = self.state.borrow_mut();
        let count = st.nodes.len();
        for mut n in st.nodes.drain(..) {
            n.teardown();
        }
        st.config = GraphConfig::default();
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

fn parse_config_param(params: Option<Value>) -> Result<GraphConfig, RpcError> {
    let params = params.ok_or_else(|| RpcError::invalid_params("missing params"))?;
    let config = params
        .get("config")
        .ok_or_else(|| RpcError::invalid_params("missing 'config' in params"))?;
    serde_json::from_value(config.clone())
        .map_err(|e| RpcError::new(error_codes::INVALID_PARAMS, format!("invalid config: {e}")))
}
