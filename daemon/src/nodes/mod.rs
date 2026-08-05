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
pub mod map;
pub mod pty;
pub mod serial;

use serial_nexus_core::config::NodeConfig;
use serial_nexus_core::graph::EndpointAddr;
use serial_nexus_core::resolver::Resolver;
use serial_nexus_core::state::NodeState;

/// A live node: its operator-facing name and its environment-owned status.
pub enum Node {
    Serial(serial::SerialNode),
    Pty(pty::PtyNode),
    Log(log::LogNode),
    Codec(codec::CodecNode),
    Exec(exec::ExecCodecNode),
    Leg(leg::LegNode),
    Map(map::MapNode),
}

impl Node {
    /// Instantiate a node from configuration. Never returns `Err` for an
    /// environmental problem — the node comes up faulted instead (§15.8); `Err`
    /// is reserved for a structural failure the daemon aborts the load on (a bad
    /// codec attribute schema, or — defensively — an unknown codec the daemon's
    /// pre-check did not already catch). `registry` is the compiled-in codec set
    /// (§8/§15.26), consulted to build codec nodes.
    pub fn instantiate(
        config: &NodeConfig,
        resolver: &Resolver,
        registry: &crate::registry::Registry,
    ) -> Result<Node, String> {
        Ok(match config {
            NodeConfig::Serial { .. } => Node::Serial(serial::SerialNode::create(config, resolver)),
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
                registry.build(codec_name, attributes)?,
            )),
            NodeConfig::Leg { .. } => Node::Leg(leg::LegNode::create(config)),
            // A map node (§7.8): a stateless interior character transform. Its
            // mapping names were validated structurally before any teardown; a parse
            // failure here is a defensive structural `Err`, never a panic.
            NodeConfig::Map { .. } => Node::Map(map::MapNode::create(config)?),
        })
    }

    /// The serial node behind this handle, for the serial-signal verbs (§7.1).
    pub fn as_serial(&self) -> Option<&serial::SerialNode> {
        match self {
            Node::Serial(n) => Some(n),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Node::Serial(n) => &n.name,
            Node::Pty(n) => &n.name,
            Node::Log(n) => &n.name,
            Node::Codec(n) => &n.name,
            Node::Exec(n) => &n.name,
            Node::Leg(n) => &n.name,
            Node::Map(n) => &n.name,
        }
    }

    /// This node's observed status together with the moment it entered it (§7: "a
    /// status of `active | waiting | faulted` with reason and timestamp"). Every
    /// node kind keeps a [`NodeState`] rather than a bare `NodeStatus`, so a
    /// recovery poll that re-reports the same condition does not reset the stamp.
    pub fn status(&self) -> NodeState {
        match self {
            Node::Serial(n) => n.status(),
            Node::Pty(n) => n.status(),
            Node::Log(n) => n.status(),
            Node::Codec(n) => n.status(),
            Node::Exec(n) => n.status(),
            Node::Leg(n) => n.status(),
            Node::Map(n) => n.status(),
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
            Node::Map(n) => n.state_extra(),
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
                // A serial node's sole endpoint is host-facing, always: §7.1's
                // `faces = "target"` output-leg role has no driver and is a
                // *structural validation error* (`ValidationError::SerialFacesTarget`),
                // so a target-facing serial can no longer reach `start` — it would
                // seize the port with TIOCEXCL and be wired to nothing (DM-1,
                // deferred work §7.1/§14). Claiming the host-facing entries
                // unconditionally is therefore exact, not an assumption; if the role
                // is ever implemented it wires a *different* set of entries here.
                let addr = EndpointAddr::node(&n.name);
                let hostward = wiring
                    .host_fanout
                    .remove(&addr)
                    .unwrap_or_else(crate::runtime::FanOutList::new);
                let targetward = wiring.host_targetward_rx.remove(&addr);
                let tap_feed = wiring.tap_feeds.remove(&addr);
                n.start(hostward, targetward, tap_feed);
            }
            Node::Pty(n) => {
                let addr = EndpointAddr::node(&n.name);
                let inbox = wiring.target_inbox.remove(&addr);
                let edge = wiring.target_edges.remove(&addr);
                let counters = wiring.target_counters.remove(&addr);
                n.start(inbox, edge, counters);
            }
            Node::Log(n) => {
                let addr = EndpointAddr::node(&n.name);
                let inbox = wiring.target_inbox.remove(&addr);
                let counters = wiring.target_counters.remove(&addr);
                n.start(inbox, counters);
            }
            Node::Codec(n) => n.start(wiring),
            Node::Exec(n) => n.start(wiring),
            Node::Leg(n) => n.start(wiring),
            Node::Map(n) => n.start(wiring),
        }
    }

    /// The readiness handle for this node's inbound artifact, if it has one a caller
    /// can race the daemon to create (§15.42). Only a `role = "listen"` leg does: it
    /// is the one node kind whose `start` publishes an address that the reply to
    /// `load` / `add-node` implicitly promises. Call it *after* [`Self::start`].
    pub fn listen_barrier(&self) -> Option<crate::nodes::leg::ListenBarrier> {
        match self {
            Node::Leg(n) => n.listen_barrier(),
            _ => None,
        }
    }

    /// An edge was just attached to this node's target-facing `endpoint` (§15.35).
    ///
    /// The channels themselves are already live — the daemon filled the endpoint's
    /// inbox and origin slot, which the node's running tasks re-read — so this is
    /// only about the node's *reported* status: an interior node that came up
    /// `waiting` for want of an upstream is now doing its job, and `state` has to
    /// say so. Boundary nodes whose status does not depend on an edge ignore it.
    pub fn edge_attached(&mut self, endpoint: &EndpointAddr) {
        match self {
            Node::Codec(n) => n.set_upstream_attached(endpoint, true),
            Node::Exec(n) => n.set_upstream_attached(endpoint, true),
            Node::Map(n) => n.set_upstream_attached(endpoint, true),
            _ => {}
        }
    }

    /// The edge on this node's target-facing `endpoint` was removed (§15.35). The
    /// mirror of [`Self::edge_attached`]: an interior node with no upstream is
    /// `waiting`, which is the same honest state it would have loaded in.
    pub fn edge_detached(&mut self, endpoint: &EndpointAddr) {
        match self {
            Node::Codec(n) => n.set_upstream_attached(endpoint, false),
            Node::Exec(n) => n.set_upstream_attached(endpoint, false),
            Node::Map(n) => n.set_upstream_attached(endpoint, false),
            _ => {}
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

    /// Ask this node's workers to stop, and return immediately — the cheap half of
    /// teardown (§16.1, BND-1/LOG-1).
    ///
    /// [`Self::teardown`] blocks: it joins a serial reader thread, a pty writer
    /// thread, a log flush. Paid per node inside one critical section on the single
    /// runtime thread, `load --replace` and shutdown stall the whole daemon for the
    /// *sum* of every node's stop latency. Signalling every node first and only then
    /// tearing them down turns that sum into (roughly) a maximum: by the time the
    /// first join is entered, every other node's worker is already winding down.
    ///
    /// Each kind sets its stop flags, aborts its tasks and returns; nothing is joined
    /// and no environment is released here, so `teardown` must still run afterwards
    /// (it is idempotent with respect to this).
    pub fn signal_stop(&mut self) {
        match self {
            Node::Serial(n) => n.signal_stop(),
            Node::Pty(n) => n.signal_stop(),
            Node::Log(n) => n.signal_stop(),
            Node::Codec(n) => n.signal_stop(),
            Node::Exec(n) => n.signal_stop(),
            Node::Leg(n) => n.signal_stop(),
            Node::Map(n) => n.signal_stop(),
        }
    }

    /// Release environment on teardown/shutdown: stop data-plane tasks, unlink
    /// the PTY symlink, drop the serial port, flush and close the log writer.
    /// Callers tearing down more than one node should run [`Self::signal_stop`]
    /// over all of them first (BND-1).
    pub fn teardown(&mut self) {
        match self {
            Node::Serial(n) => n.teardown(),
            Node::Pty(n) => n.teardown(),
            Node::Log(n) => n.teardown(),
            Node::Codec(n) => n.teardown(),
            Node::Exec(n) => n.teardown(),
            Node::Leg(n) => n.teardown(),
            Node::Map(n) => n.teardown(),
        }
    }

    /// Targetward bytes this node destroyed at teardown, for the verb that removed it
    /// to report (§5, notes §3.31).
    ///
    /// Only the interior kinds that own a host-facing targetward queue can answer
    /// anything but `0`, and only after [`Self::signal_stop`] has run — that is where
    /// the queue is drained and counted. Every other kind returns `0` because it has
    /// no such queue, not because its loss is unreported: a boundary node's own
    /// discards live on its `state` counters, which survive it in every path but a
    /// removal.
    pub fn discarded_at_teardown(&self) -> u64 {
        match self {
            Node::Map(n) => n.discarded_at_teardown(),
            Node::Codec(n) => n.discarded_at_teardown(),
            Node::Exec(n) => n.discarded_at_teardown(),
            // Boundary kinds: `serial` and `leg` own targetward queues too and lose
            // them the same way, but their receivers are also fed to
            // `boundary::drain_to_quiescence` on the purge-on-reconnect path (§7.1,
            // §7.4), so adopting the shared inbox means moving that helper onto it —
            // a §16.5 one-rule-one-place change rather than the four lines the
            // interior kinds needed. Recorded as the remaining sibling in notes
            // §3.31 rather than half-done here: a counter that reports `0` while
            // bytes are being destroyed is worse than no counter at all.
            //
            // `pty` and `log` have no host-facing targetward queue of this shape at
            // all — the pty's undelivered payload is a held `pending` slot inside its
            // reader (also §3.31), and the log is target-facing.
            Node::Serial(_) | Node::Pty(_) | Node::Log(_) | Node::Leg(_) => 0,
        }
    }
}
