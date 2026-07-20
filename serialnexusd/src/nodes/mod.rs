//! Boundary node runtimes (design §7). Phase 2 lands the serial and PTY nodes.
//!
//! Slice 1 (this): real environmental setup — the PTY pair, baseline termios,
//! packet mode and symlink; the serial open with TIOCEXCL — so `state` reports
//! the truth, and environmental failure faults a node without failing the
//! operation that created it (§15.8). Slice 2 wires the data plane so bytes flow
//! serial↔PTY and adds presence gating.

pub mod log;
pub mod pty;
pub mod serial;

use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;

/// A live node: its operator-facing name and its environment-owned status.
pub enum Node {
    Serial(serial::SerialNode),
    Pty(pty::PtyNode),
    Log(log::LogNode),
}

impl Node {
    /// Instantiate a node from configuration. Never returns `Err` for an
    /// environmental problem — the node comes up faulted instead (§15.8); `Err`
    /// is reserved for a node kind not yet implemented in this phase.
    pub fn instantiate(config: &NodeConfig) -> Result<Node, String> {
        Ok(match config {
            NodeConfig::Serial { .. } => Node::Serial(serial::SerialNode::create(config)),
            NodeConfig::Pty { .. } => Node::Pty(pty::PtyNode::create(config)),
            NodeConfig::Log { .. } => Node::Log(log::LogNode::create(config)),
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Node::Serial(n) => &n.name,
            Node::Pty(n) => &n.name,
            Node::Log(n) => &n.name,
        }
    }

    pub fn status(&self) -> NodeStatus {
        match self {
            Node::Serial(n) => n.status(),
            Node::Pty(n) => n.status(),
            Node::Log(n) => n.status(),
        }
    }

    /// Observed, non-config state for the `state` verb (pts path, resolved
    /// device path, client-present, counters — grows through later phases).
    pub fn state_extra(&self) -> serde_json::Value {
        match self {
            Node::Serial(n) => n.state_extra(),
            Node::Pty(n) => n.state_extra(),
            Node::Log(n) => n.state_extra(),
        }
    }

    /// Start this node's data-plane tasks, taking its channels out of the
    /// wiring plan (§5). Called from `load` after instantiation and validation.
    pub fn start(&mut self, wiring: &mut crate::runtime::Wiring) {
        match self {
            Node::Serial(n) => {
                let hostward = wiring.serial_hostward.remove(&n.name).unwrap_or_default();
                let targetward = wiring.serial_targetward.remove(&n.name);
                n.start(hostward, targetward);
            }
            Node::Pty(n) => {
                let hostward = wiring.consumer_hostward.remove(&n.name);
                let targetward = wiring.pty_targetward.remove(&n.name);
                let counters = wiring.consumer_counters.remove(&n.name);
                n.start(hostward, targetward, counters);
            }
            Node::Log(n) => {
                let hostward = wiring.consumer_hostward.remove(&n.name);
                let counters = wiring.consumer_counters.remove(&n.name);
                n.start(hostward, counters);
            }
        }
    }

    /// Rotate a log node's file on demand (§7.3). Errors for a non-log node or a
    /// faulted log; returns the number the next completed rotation carries.
    pub fn rotate(&self) -> Result<u64, String> {
        match self {
            Node::Log(n) => n.rotate(),
            _ => Err(format!("node {} is not a log node", self.name())),
        }
    }

    /// Release environment on teardown/shutdown: stop data-plane tasks, unlink
    /// the PTY symlink, drop the serial port, flush and close the log writer.
    pub fn teardown(&mut self) {
        match self {
            Node::Serial(n) => n.teardown(),
            Node::Pty(n) => n.teardown(),
            Node::Log(n) => n.teardown(),
        }
    }
}
