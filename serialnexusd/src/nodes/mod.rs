//! Boundary node runtimes (design §7). Phase 2 lands the serial and PTY nodes.
//!
//! Slice 1 (this): real environmental setup — the PTY pair, baseline termios,
//! packet mode and symlink; the serial open with TIOCEXCL — so `state` reports
//! the truth, and environmental failure faults a node without failing the
//! operation that created it (§15.8). Slice 2 wires the data plane so bytes flow
//! serial↔PTY and adds presence gating.

pub mod pty;
pub mod serial;

use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;

/// A live node: its operator-facing name and its environment-owned status.
pub enum Node {
    Serial(serial::SerialNode),
    Pty(pty::PtyNode),
}

impl Node {
    /// Instantiate a node from configuration. Never returns `Err` for an
    /// environmental problem — the node comes up faulted instead (§15.8); `Err`
    /// is reserved for a node kind not yet implemented in this phase.
    pub fn instantiate(config: &NodeConfig) -> Result<Node, String> {
        match config {
            NodeConfig::Serial { .. } => Ok(Node::Serial(serial::SerialNode::create(config))),
            NodeConfig::Pty { .. } => Ok(Node::Pty(pty::PtyNode::create(config))),
            NodeConfig::Log { .. } => Err("log nodes land in phase 3".to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Node::Serial(n) => &n.name,
            Node::Pty(n) => &n.name,
        }
    }

    pub fn status(&self) -> NodeStatus {
        match self {
            Node::Serial(n) => n.status(),
            Node::Pty(n) => n.status(),
        }
    }

    /// Observed, non-config state for the `state` verb (pts path, resolved
    /// device path, client-present, counters — grows through later phases).
    pub fn state_extra(&self) -> serde_json::Value {
        match self {
            Node::Serial(n) => n.state_extra(),
            Node::Pty(n) => n.state_extra(),
        }
    }

    /// Release environment on teardown/shutdown (unlink the PTY symlink; drop
    /// the serial port).
    pub fn teardown(&mut self) {
        if let Node::Pty(n) = self {
            n.teardown();
        }
    }
}
