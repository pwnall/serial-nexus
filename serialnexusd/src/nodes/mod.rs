//! Boundary node runtimes (design §7). Phase 2 lands the serial and PTY nodes.
//!
//! Slice 1 (this): real environmental setup — the PTY pair, baseline termios,
//! packet mode and symlink; the serial open with TIOCEXCL — so `state` reports
//! the truth, and environmental failure faults a node without failing the
//! operation that created it (§15.8). Slice 2 wires the data plane so bytes flow
//! serial↔PTY and adds presence gating.

pub mod codec;
pub mod exec;
pub mod leg;
pub mod log;
pub mod pty;
pub mod serial;

use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nexus_core::graph::EndpointAddr;

/// A live node: its operator-facing name and its environment-owned status.
pub enum Node {
    Serial(serial::SerialNode),
    Pty(pty::PtyNode),
    Log(log::LogNode),
    Codec(codec::CodecNode),
    Exec(exec::ExecCodecNode),
    Leg(leg::LegNode),
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
            // A codec node (§7.5/§7.6). The exec codec is a child process, hosted
            // separately; every other codec is an in-process transform built from
            // the registry. A bad codec name or attribute schema is structural — it
            // aborts the load with nothing created (§8, §11), returning `Err` here.
            NodeConfig::Codec {
                codec: codec_name,
                attributes,
                ..
            } if codec_name == "exec" => {
                exec::parse_attributes(attributes)?;
                Node::Exec(exec::ExecCodecNode::create(config))
            }
            NodeConfig::Codec {
                codec: codec_name,
                attributes,
                ..
            } => Node::Codec(codec::CodecNode::create(
                config,
                codec::build_codec(codec_name, attributes)?,
            )),
            NodeConfig::Leg { .. } => Node::Leg(leg::LegNode::create(config)),
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Node::Serial(n) => &n.name,
            Node::Pty(n) => &n.name,
            Node::Log(n) => &n.name,
            Node::Codec(n) => &n.name,
            Node::Exec(n) => &n.name,
            Node::Leg(n) => &n.name,
        }
    }

    pub fn status(&self) -> NodeStatus {
        match self {
            Node::Serial(n) => n.status(),
            Node::Pty(n) => n.status(),
            Node::Log(n) => n.status(),
            Node::Codec(n) => n.status(),
            Node::Exec(n) => n.status(),
            Node::Leg(n) => n.status(),
        }
    }

    /// Observed, non-config state for the `state` verb (pts path, resolved
    /// device path, client-present, counters — grows through later phases).
    pub fn state_extra(&self) -> serde_json::Value {
        match self {
            Node::Serial(n) => n.state_extra(),
            Node::Pty(n) => n.state_extra(),
            Node::Log(n) => n.state_extra(),
            Node::Codec(n) => n.state_extra(),
            Node::Exec(n) => n.state_extra(),
            Node::Leg(n) => n.state_extra(),
        }
    }

    /// Start this node's data-plane tasks, taking its endpoints' channels out of
    /// the wiring plan (§5). Called from `load` after instantiation and validation.
    /// Single-endpoint boundary nodes (serial, pty, log) claim their sole endpoint
    /// (the node's default address); the interior codec claims its multiplexed side
    /// and every channel itself.
    pub fn start(&mut self, wiring: &mut crate::runtime::Wiring) {
        match self {
            Node::Serial(n) => {
                let addr = EndpointAddr::node(&n.name);
                let hostward = wiring.host_sinks.remove(&addr).unwrap_or_default();
                let targetward = wiring.host_targetward_rx.remove(&addr);
                n.start(hostward, targetward);
            }
            Node::Pty(n) => {
                let addr = EndpointAddr::node(&n.name);
                let hostward = wiring.target_hostward_rx.remove(&addr);
                let targetward = wiring.target_targetward_tx.remove(&addr);
                let counters = wiring.target_counters.remove(&addr);
                let lock = wiring.origin_locks.remove(&addr);
                n.start(hostward, targetward, counters, lock);
            }
            Node::Log(n) => {
                let addr = EndpointAddr::node(&n.name);
                let hostward = wiring.target_hostward_rx.remove(&addr);
                let counters = wiring.target_counters.remove(&addr);
                n.start(hostward, counters);
            }
            Node::Codec(n) => n.start(wiring),
            Node::Exec(n) => n.start(wiring),
            Node::Leg(n) => n.start(wiring),
        }
    }

    /// Drain and discard this origin's pre-grant targetward backlog, returning
    /// the count of bytes discarded (§6 purge-on-acquire). Only a PTY origin has
    /// a backlog to purge; every other node kind returns 0.
    pub fn purge_origin(&self) -> u64 {
        match self {
            Node::Pty(n) => n.purge_origin(),
            _ => 0,
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
            Node::Codec(n) => n.teardown(),
            Node::Exec(n) => n.teardown(),
            Node::Leg(n) => n.teardown(),
        }
    }
}
