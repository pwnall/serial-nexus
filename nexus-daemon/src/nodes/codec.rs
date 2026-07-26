//! Codec node (design §7.5): the interior demux/remux protocol transform. The
//! compiled-in codec registry that instantiates these (§8/§15.26) lives in
//! [`crate::registry`]; this module is the running node.
//!
//! **Orientation.** Phase 5 implements the **demultiplexer** (`faces = target`):
//! the multiplexed side is the node's default endpoint, facing the device across
//! a serial; N channel endpoints face host consumers. Hostward, raw multiplexed
//! bytes are `demux`ed into per-channel events and fanned out; targetward,
//! per-channel writes are `mux`ed back into the multiplexed stream and forwarded
//! to the device. The **re-multiplexer** (`faces = host`) is the mirror, and a
//! standalone instance of it has no driver yet: §7.5 says such a node "is accepted
//! by validation but waits for a driver", so it comes up **waiting** with a §14
//! reason — the config stays loadable and the gap is visible in state (§15.8).
//!
//! **Interior contract (§5).** The codec holds only parser state (a partial
//! frame, bounded by the frame size) — no queues. It runs on the async runtime;
//! the synchronous `demux`/`mux` transforms execute in the task's context, and
//! the bounded mpsc channels to its serial and PTY neighbours are *their* boundary
//! buffers, not the codec's.
//!
//! **The held lock (§6).** The demultiplexer's edge to the serial holds that
//! endpoint's write lock permanently (any other writer would corrupt the mux
//! framing). A `send --steal` at the serial ousts the codec transiently; each
//! channel's targetward task then parks its one framed chunk and re-acquires the
//! lock (FIFO) once the stealer releases — the §6 stall, with commands delayed,
//! never dropped.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use codec_api::{Codec, Event, EventKind};
use nexus_core::Chunk;
use nexus_core::config::NodeConfig;
use nexus_core::graph::{EndpointAddr, Facing};
use nexus_core::state::{NodeState, NodeStatus};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::cell::CriticalCell;
use crate::runtime::{
    DropCounters, EdgeInbox, SharedFanOut, SharedTargetEdge, Wiring, await_origin, frame_ranges,
    reacquire_held,
};
use crate::tap::TapFeed;

/// Per-channel observed counters (§7.5). All access is on the one runtime thread,
/// so `Cell` suffices.
#[derive(Default)]
struct ChannelStat {
    /// Bytes handed hostward to at least one *live* consumer of this channel
    /// (device → consumers). A per-consumer slow-buffer drop is counted separately
    /// at that boundary (§5).
    delivered_hostward: Cell<u64>,
    /// Bytes discarded because this channel reached no live consumer — either it is
    /// configured with none bound, or every attached one has been cascade-removed
    /// and its sink is permanently `Closed`. A §5 loss counted where it happens,
    /// not silently dropped.
    discarded_unattached: Cell<u64>,
    /// Channel bytes forwarded targetward to the device. Freezes while the codec
    /// does not hold the serial's write lock — the observable §6 stall on a stolen
    /// held lock (item 6).
    accepted_targetward: Cell<u64>,
    /// Channel bytes that could not be framed targetward and were therefore dropped
    /// — a §5 loss counted where it happens. Unreachable for the envelope codec
    /// (each oversize chunk is fragmented so every piece provably fits a frame); a
    /// defensive count for a custom transform whose `mux` refuses a piece.
    discarded_targetward: Cell<u64>,
    /// Whether the channel has been seen active (an `open`, or any `data`).
    active: Cell<bool>,
}

pub struct CodecNode {
    pub name: String,
    codec_name: String,
    faces: Facing,
    channels: Vec<String>,
    /// The transform, shared between the hostward demux task and each channel's
    /// targetward mux task; borrowed only synchronously, never across an await.
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    /// Hostward drops the serial reader counted because this codec's multiplexed
    /// side fell behind (its bounded intake was full) — a §5 loss, surfaced so it
    /// stays located and attributable. Claimed from the wiring at start.
    mux_counters: Option<Arc<DropCounters>>,
    /// The multiplexed side's live edge binding (§15.35): whether an upstream is
    /// attached and, when it is writable, the targetward sender and lock every
    /// channel frames through. `connect`/`disconnect` mutate it under the running
    /// tasks.
    mux_edge: Option<SharedTargetEdge>,
    /// Targetward bytes arriving at the *multiplexed* endpoint that this node has no
    /// path for — the re-multiplexer orientation, whose driver is deferred (§14).
    /// They are drained rather than dropped (see `drain_unwired_channels`) so a
    /// writer's task survives, and counted here so the loss stays attributable (§5).
    mux_discarded_targetward: Rc<Cell<u64>>,
    tasks: Vec<JoinHandle<()>>,
    /// The node's observed status *and the moment it entered it* (§7).
    status: NodeState,
}

impl CodecNode {
    /// Create the node from configuration and a pre-built codec (the registry
    /// validated the name and attributes at instantiate time, §8/§11).
    pub fn create(config: &NodeConfig, codec: Box<dyn Codec>) -> CodecNode {
        let NodeConfig::Codec {
            name,
            codec: codec_name,
            faces,
            channels,
            ..
        } = config
        else {
            unreachable!("CodecNode::create called with non-Codec config");
        };
        let stats = channels
            .iter()
            .map(|c| (c.clone(), Rc::new(ChannelStat::default())))
            .collect();
        CodecNode {
            name: name.clone(),
            codec_name: codec_name.clone(),
            faces: *faces,
            channels: channels.clone(),
            codec: Rc::new(CriticalCell::new(codec)),
            stats: Rc::new(stats),
            mux_counters: None,
            mux_edge: None,
            mux_discarded_targetward: Rc::new(Cell::new(0)),
            tasks: Vec::new(),
            status: NodeState::new(NodeStatus::Active),
        }
    }

    /// Claim every channel's targetward receiver and park it in a draining task,
    /// for the `start` paths that return before the data plane is built (§15.8).
    ///
    /// A node that comes up `waiting` still owns its endpoints, and those endpoints
    /// still have live senders. Leaving the receivers in the wiring plan drops them
    /// when `load` finishes, which is indistinguishable to a writer from the graph
    /// being torn down — see the MAP-1 chain in `start`. Draining keeps every writer
    /// healthy and every lost byte counted.
    fn drain_unwired_channels(&mut self, wiring: &mut Wiring) {
        // Whichever side faces host owns the arbitrated targetward channel: the
        // channels for a demultiplexer, the multiplexed endpoint for a re-multiplexer.
        // Sweep both rather than assuming an orientation, so neither `start` exit can
        // leak a receiver.
        let addrs = std::iter::once((None, EndpointAddr::node(&self.name))).chain(
            self.channels
                .iter()
                .map(|ch| (Some(ch.clone()), EndpointAddr::channel(&self.name, ch))),
        );
        for (channel, addr) in addrs.collect::<Vec<_>>() {
            let Some(rx) = wiring.host_targetward_rx.remove(&addr) else {
                continue;
            };
            // A per-channel stat exists for a channel endpoint; the multiplexed
            // endpoint has none, so its discards are counted against the node's
            // mux-side counter instead of being lost.
            match channel.and_then(|ch| self.stats.get(&ch).cloned()) {
                Some(stat) => self
                    .tasks
                    .push(tokio::task::spawn_local(channel_targetward_drain(rx, stat))),
                None => self
                    .tasks
                    .push(tokio::task::spawn_local(mux_targetward_drain(
                        rx,
                        self.mux_discarded_targetward.clone(),
                    ))),
            }
        }
    }

    /// Wire and start the demultiplexer's data plane, claiming the codec's own
    /// endpoints out of the (endpoint-keyed) wiring plan.
    pub fn start(&mut self, wiring: &mut Wiring) {
        if self.faces != Facing::Target {
            // Re-multiplexer (faces=host): §7.5 orients it as the demultiplexer's
            // mirror, and §14 defers the driver for a *standalone* instance — a leg
            // node re-multiplexes through its own link codec, so nothing in-tree
            // drives this one. §7.5/§14 promise it "loads and waits", which is the
            // waiting/faulted state family's `waiting` arm (§15.8): the environment
            // it needs is simply not there yet, and no environmental failure has
            // occurred. Faulting here misreported deferred work as a malfunction.
            self.status.set(NodeStatus::Waiting {
                reason: "standalone re-multiplexer orientation (faces=host) has no driver; \
                         deferred work (§14) — a leg node re-multiplexes through its own \
                         link codec"
                    .to_owned(),
            });
            // Same rule as the no-upstream exit below: a waiting node must be inert,
            // not destructive. A re-multiplexer's *multiplexed* side faces host, so it
            // is the one carrying a live targetward receiver here.
            self.drain_unwired_channels(wiring);
            return;
        }

        // Multiplexed side (the default endpoint, target-facing): raw hostward in,
        // raw targetward out. Without an attached serial there is no data path yet —
        // the node waits (§15.8) but every task still starts, parked on the endpoint's
        // inbox and its origin slot, so a later `connect` needs no restart (§15.35).
        // A waiting node must be *inert*, not destructive: the channels' targetward
        // receivers are kept and drained rather than dropped, since their senders stay
        // live in `GraphState::endpoint_targetward` and in every attached writer
        // origin. That is MAP-1's chain — a pty origin's next write fails,
        // `read_and_poll` returns, and presence latching, `handle_last_close`, termios
        // reconciliation and detach-release go with it, wedging the channel's lock on
        // a holder that has gone away while its bytes vanish uncounted (§15.8).
        let mux = EndpointAddr::node(&self.name);
        let mux_inbox = wiring.target_inbox.remove(&mux);
        self.mux_edge = wiring.target_edges.remove(&mux);
        self.mux_counters = wiring.target_counters.remove(&mux);
        if !self.mux_attached() {
            self.status.set(NodeStatus::Waiting {
                reason: "multiplexed side has no attached upstream".to_owned(),
            });
        }

        // Per-channel hostward fan-outs, targetward receivers, and tap feeds.
        let mut channel_sinks: HashMap<String, SharedFanOut> = HashMap::new();
        let mut channel_feeds: HashMap<String, TapFeed> = HashMap::new();
        let mut channel_rxs: Vec<(String, mpsc::Receiver<Chunk>)> = Vec::new();
        for ch in &self.channels {
            let addr = EndpointAddr::channel(&self.name, ch);
            if let Some(sinks) = wiring.host_fanout.remove(&addr) {
                channel_sinks.insert(ch.clone(), sinks);
            }
            if let Some(feed) = wiring.tap_feeds.remove(&addr) {
                channel_feeds.insert(ch.clone(), feed);
            }
            if let Some(rx) = wiring.host_targetward_rx.remove(&addr) {
                channel_rxs.push((ch.clone(), rx));
            }
        }

        // Hostward: demux the multiplexed stream and fan each channel out (§5),
        // mirroring to each channel's tap hub for taps and the replay ring (§17).
        if let Some(inbox) = mux_inbox {
            self.tasks.push(tokio::task::spawn_local(hostward_demux(
                self.codec.clone(),
                inbox,
                channel_sinks,
                channel_feeds,
                self.stats.clone(),
            )));
        }

        // Targetward: one task per channel, framing its writes back into the
        // multiplexed stream — only if the multiplexed side can write to the device
        // (its edge is held/on-demand, giving a targetward sender and a lock).
        //
        // Otherwise — a `write_mode = "never"` multiplexed edge, which validation
        // explicitly permits as the read-only demux, or an unattached mux side — the
        // channel receivers must still be *kept alive and drained*, never dropped
        // (MAP-1's shape, which the map node hit first). Their senders stay live in
        // `GraphState::endpoint_targetward` and in every writer origin attached to a
        // channel, so dropping them would close the channel under a live writer: the
        // next targetward write fails, and for a pty origin that ends `read_and_poll`
        // and with it presence latching, last-close handling, termios reconciliation
        // and detach-release — a healthy-looking graph whose console silently stops
        // reporting its client. Draining makes a read-only demux inert instead, with
        // the loss counted where it happens (§5).
        let mux_edge = self
            .mux_edge
            .clone()
            .unwrap_or_else(crate::runtime::TargetEdge::new);
        for (ch, rx) in channel_rxs {
            let Some(stat) = self.stats.get(&ch).cloned() else {
                continue;
            };
            self.tasks.push(tokio::task::spawn_local(channel_targetward(
                ch,
                rx,
                mux_edge.clone(),
                self.codec.clone(),
                stat,
            )));
        }
    }

    /// Whether the multiplexed side currently has an upstream edge (§15.35).
    fn mux_attached(&self) -> bool {
        self.mux_edge
            .as_ref()
            .is_some_and(|e| e.with(|s| s.attached))
    }

    /// Re-report status after edge surgery on `endpoint` (§15.35). Only the
    /// multiplexed side decides `active`/`waiting`: a channel endpoint faces host,
    /// and its own liveness is already reported per channel from the demux stats.
    pub fn set_upstream_attached(&mut self, endpoint: &EndpointAddr, attached: bool) {
        if !endpoint.is_default() || endpoint.node != self.name {
            return;
        }
        self.status.set(if attached {
            NodeStatus::Active
        } else {
            NodeStatus::Waiting {
                reason: "multiplexed side has no attached upstream".to_owned(),
            }
        });
    }

    pub fn status(&self) -> NodeState {
        self.status.clone()
    }

    pub fn state_extra(&self) -> Value {
        // Codec-specific counters (§7.5). Borrow the transform synchronously — no
        // task holds the borrow across an await, so this never contends.
        // `delivered_hostward` counts channel bytes handed to the consumer boundary
        // (a slow consumer's own drops are counted at that boundary, §5);
        // `accepted_targetward` counts channel bytes handed into the serial's
        // targetward channel (the device-write handoff, not device consumption), and
        // freezes while the demux does not hold the serial lock (§6). `status` is
        // `active` once any data has crossed the channel, else `waiting`.
        let framing_errors = self.codec.with(|c| c.resync_count());
        let mux_discarded_targetward = self.mux_discarded_targetward.get();
        let channels: serde_json::Map<String, Value> = self
            .channels
            .iter()
            .map(|ch| {
                // `self.stats` is built from `self.channels` in `create`, so every
                // channel has a stat — index directly (no Option handling).
                let stat = &self.stats[ch];
                let obj = json!({
                    "status": if stat.active.get() { "active" } else { "waiting" },
                    "delivered_hostward": stat.delivered_hostward.get(),
                    "discarded_unattached": stat.discarded_unattached.get(),
                    "accepted_targetward": stat.accepted_targetward.get(),
                    "discarded_targetward": stat.discarded_targetward.get(),
                });
                (ch.clone(), obj)
            })
            .collect();
        json!({
            "codec": self.codec_name,
            "faces": self.faces.to_string(),
            "framing_errors": framing_errors,
            // The multiplexed side's own hostward drops (the codec falling behind
            // the serial), so the loss stays located and attributable (§5).
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
                // Targetward bytes drained at an unwired multiplexed endpoint (the
                // re-multiplexer orientation, §14) — inert rather than destructive,
                // and counted rather than silent (§5).
                "discarded_targetward": mux_discarded_targetward,
            },
            "channels": channels,
        })
    }

    /// Ask this node's tasks to stop, without waiting (§16.1, BND-1). An interior
    /// codec owns no blocking thread and no environment to release, so signalling
    /// *is* its whole teardown; the method exists so the daemon can signal every
    /// node uniformly before it pays any node's join cost.
    pub fn signal_stop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }

    pub fn teardown(&mut self) {
        self.signal_stop();
    }
}

impl Drop for CodecNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

/// Hostward demux task: drain the multiplexed stream, decode per-channel events,
/// and fan each channel's data out to its consumers (lossy `try_send` at the
/// consuming boundary, §5). The codec borrow is synchronous and dropped before the
/// fan-out and before the next `recv().await`.
async fn hostward_demux(
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    mut inbox: EdgeInbox,
    channel_sinks: HashMap<String, SharedFanOut>,
    channel_feeds: HashMap<String, TapFeed>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
) {
    // One task across every upstream edge this node is given over its life (§15.35):
    // it parks on the inbox while unattached, drains an edge until `disconnect`
    // closes it, then parks again — so the codec's framing state and its channels'
    // tasks survive edge surgery untouched.
    while let Some(mut mux_rx) = inbox.recv().await {
        while let Some(chunk) = mux_rx.recv().await {
            let mut events = Vec::new();
            codec.with_mut(|c| {
                if let Err(e) = c.demux(&chunk, &mut |ev| events.push(ev)) {
                    tracing::warn!("codec demux error: {e}");
                }
            });
            for ev in events {
                let stat = stats.get(ev.channel.as_str());
                match ev.kind {
                    EventKind::Data(bytes) => {
                        let n = bytes.len() as u64;
                        if let Some(s) = stat {
                            s.active.set(true);
                        }
                        // Mirror to this channel's tap hub for taps and the replay ring
                        // (§17), independent of whether a graph consumer is bound — a
                        // tapped-but-unconsumed channel still reaches its observer.
                        if let Some(feed) = channel_feeds.get(ev.channel.as_str()) {
                            feed.mirror(&bytes);
                        }
                        // Fan out to this channel's consumers through the one shared
                        // helper (§5, F1). A channel that reached no live consumer —
                        // none bound, or every sink permanently `Closed` after a cascade
                        // removal — discards-with-count; data on an unconfigured channel
                        // (no stat) is noise from the mux and simply dropped —
                        // announced-but-unbound is a leg concern (§7.4).
                        if let Some(sinks) = channel_sinks.get(ev.channel.as_str()) {
                            // `channel_sinks` and `stats` are both keyed by the
                            // configured channel list, so a bound sink always has a
                            // stat; `scratch` is the defensive arm — its count is
                            // thrown away, but the bytes are still delivered.
                            let scratch = Cell::new(0u64);
                            let unattached = stat.map_or(&scratch, |s| &s.discarded_unattached);
                            if sinks.broadcast(&bytes, unattached).live
                                && let Some(s) = stat
                            {
                                s.delivered_hostward.set(s.delivered_hostward.get() + n);
                            }
                        } else if let Some(s) = stat {
                            s.discarded_unattached.set(s.discarded_unattached.get() + n);
                        }
                    }
                    EventKind::Open => {
                        if let Some(s) = stat {
                            s.active.set(true);
                        }
                    }
                    EventKind::Close => {
                        if let Some(s) = stat {
                            s.active.set(false);
                        }
                    }
                    EventKind::Error(msg) => {
                        tracing::debug!(channel = %ev.channel, "codec channel error: {msg}");
                    }
                }
            }
        }
    }
}

/// Keep a read-only demux's channel receiver alive, discarding and counting what
/// arrives (§5) — the codec's instance of the MAP-1 shape.
///
/// A codec whose multiplexed edge is `write_mode = "never"` (validation's documented
/// read-only demux) or unattached has no path to the device, but its channel
/// endpoints still carry live senders: the `send` verb's clone and every writer
/// origin attached to a channel. Dropping the receiver would close the channel under
/// those senders, and for a pty origin the failed write ends its reader task along
/// with presence latching, last-close handling and detach-release. Draining keeps the
/// writers healthy and makes the loss visible in `state` instead of silent.
async fn channel_targetward_drain(mut rx: mpsc::Receiver<Chunk>, stat: Rc<ChannelStat>) {
    while let Some(bytes) = rx.recv().await {
        // The arriving bytes are what is lost; nothing is framed, so this is charged
        // as a targetward discard rather than a framing refusal.
        stat.discarded_targetward
            .set(stat.discarded_targetward.get() + bytes.len() as u64);
    }
}

/// Keep an unwired *multiplexed* endpoint's targetward receiver alive, discarding
/// and counting what arrives (§5) — the re-multiplexer's half of the same rule
/// `channel_targetward_drain` serves for channels.
async fn mux_targetward_drain(mut rx: mpsc::Receiver<Chunk>, discarded: Rc<Cell<u64>>) {
    while let Some(bytes) = rx.recv().await {
        discarded.set(discarded.get() + bytes.len() as u64);
    }
}

/// Targetward task for one channel: frame each write into the multiplexed stream
/// and forward it to the device, gated on the codec holding the serial's write
/// lock (§6). A write larger than one frame — an uncapped `send` line or a
/// packet-mode PTY read up to READ_BUF == MAX_FRAME_SIZE, which the channel-id
/// header pushes over the frame bound — is fragmented into consecutive data frames
/// rather than dropped, mirroring the leg and the exec codec (§5 no-drop /
/// all-loss-counted, §15.24). Each framed piece parks here (bounded: one chunk)
/// while the lock is stolen, and is delivered once the codec re-acquires — delayed,
/// never dropped.
async fn channel_targetward(
    channel: String,
    mut rx: mpsc::Receiver<Chunk>,
    mux_edge: SharedTargetEdge,
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    stat: Rc<ChannelStat>,
) {
    while let Some(bytes) = rx.recv().await {
        let total = bytes.len();
        // Re-read the multiplexed side's live edge per chunk (§15.35). An
        // *unattached* mux side — never connected, or one a `disconnect` just
        // removed — parks inside `await_origin`: this channel's targetward buffer
        // fills and its writers backpressure, which is §5's targetward contract and
        // the same stall a steal produces. A `write_mode = "never"` mux edge
        // (validation's documented read-only demux) is attached and will never
        // become writable, so it drains-and-counts instead — inert rather than
        // destructive, and never wedging a writer forever (MAP-1: closing the
        // receiver under a pty origin ends that origin's reader task and takes
        // presence latching and detach-release with it).
        let Some((mux_tx, serial_lock, mux_id)) = await_origin(&mux_edge).await else {
            stat.discarded_targetward
                .set(stat.discarded_targetward.get() + total as u64);
            continue;
        };
        // Fragment on the shared boundary helper (§5/§15.27): identical piece
        // ranges to the envelope framers, but each range is framed through this
        // codec's pluggable `mux` and lock-gated per piece rather than encoded
        // eagerly like `data_frames`.
        for (off, end) in frame_ranges(channel.as_str(), total) {
            let piece_len = (end - off) as u64;
            let mut framed = Vec::new();
            let muxed = codec.with_mut(|c| {
                c.mux(
                    &Event::data(channel.as_str(), bytes.slice(off..end)),
                    &mut framed,
                )
                .is_ok()
            });
            if !muxed {
                // Defensive: each fragment provably fits the frame bound for the
                // envelope codec, so this is unreachable there; a custom transform
                // that still refuses a piece must not drop silently — count the
                // undelivered residual (§5 all-loss-is-counted).
                stat.discarded_targetward
                    .set(stat.discarded_targetward.get() + (total - off) as u64);
                break;
            }
            // Gate on holding the serial's write lock (the codec's held origin). A
            // `send --steal` transiently ousts it; re-acquire FIFO once the stealer
            // releases. The framed piece is parked in `framed` meanwhile.
            if !reacquire_held(&serial_lock, mux_id).await {
                // The upstream endpoint was torn down. Count the undelivered tail
                // and stop framing *this* chunk, but keep the task: a `connect` may
                // give this codec a new upstream, and the receiver's senders are
                // still live either way (§5, §15.35).
                stat.discarded_targetward
                    .set(stat.discarded_targetward.get() + (total - off) as u64);
                break;
            }
            if mux_tx.send(Chunk::from(framed)).await.is_err() {
                stat.discarded_targetward
                    .set(stat.discarded_targetward.get() + (total - off) as u64);
                break;
            }
            stat.accepted_targetward
                .set(stat.accepted_targetward.get() + piece_len);
        }
    }
}

#[cfg(all(test, feature = "codec-reference"))]
mod tests {
    use super::*;
    use crate::runtime::{SharedLock, TargetEdge};
    use nexus_core::lock::{Arbitration, EndpointLock, OriginId, WriteMode};
    use tokio::sync::broadcast;

    /// A serial lock whose held demux origin already owns the write lock, so the
    /// codec's `reacquire_held` fast path returns immediately (no parking).
    fn held_lock() -> (SharedLock, OriginId) {
        let id = OriginId(1);
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        lock.register(id, "demux", WriteMode::Held); // acquires the lock on attach
        let (notifier, _rx) = broadcast::channel(16);
        (
            Rc::new(crate::runtime::LockCell::new("mux", lock, notifier)),
            id,
        )
    }

    /// A multiplexed-side edge slot bound to `held_lock`'s origin, writing into
    /// `mux_tx` — the shape `connect` leaves behind (§15.35).
    fn bound_edge(mux_tx: mpsc::Sender<Chunk>) -> SharedTargetEdge {
        let (lock, id) = held_lock();
        let slot = TargetEdge::new();
        slot.with_mut(|e| {
            e.attached = true;
            e.registered = Some((lock, id));
            e.writer = Some(mux_tx);
        });
        slot
    }

    /// A read-only demux (`write_mode = "never"` on the multiplexed edge, which
    /// validation permits) keeps its channel receivers alive and counts what it
    /// discards, instead of dropping them under live senders. Dropping was the
    /// defect the map node hit first (review 26, MAP-1): the next targetward write
    /// then fails, and a pty origin's reader task ends with presence latching and
    /// detach-release still owed. Here: the channel sender stays usable across many
    /// writes, and every discarded byte is attributable in `state`.
    #[tokio::test]
    async fn a_read_only_demux_drains_its_channels_instead_of_closing_them() {
        let stat = Rc::new(ChannelStat::default());
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                // A `write_mode = "never"` mux edge: *attached*, so the channel
                // drains-and-counts rather than parking. (An unattached side — no
                // edge, or one a `disconnect` removed — backpressures instead, which
                // is the other half of the rule and a different test.)
                let task = tokio::task::spawn_local(channel_targetward(
                    "console".to_owned(),
                    rx,
                    TargetEdge::read_only(),
                    Rc::new(CriticalCell::new(
                        Box::new(codec_reference::ReferenceCodec::new()) as Box<dyn Codec>,
                    )),
                    stat.clone(),
                ));
                for _ in 0..3 {
                    // A sender whose receiver had been dropped would fail here — the
                    // exact failure that killed the writer's task in the defect.
                    tx.send(Chunk::copy_from_slice(b"reboot\n"))
                        .await
                        .expect("a read-only demux must still accept writes");
                }
                drop(tx);
                let _ = task.await;
            })
            .await;
        assert_eq!(
            stat.discarded_targetward.get(),
            21,
            "every discarded byte is counted where it is lost (§5)"
        );
    }

    /// XC-NODROP-1: a targetward chunk larger than one frame (once the channel-id
    /// header is added) is fragmented into consecutive data frames and reassembled
    /// byte-exact by `demux`, with nothing dropped — the codec mirror of the leg's
    /// no-drop round-trip (§5 all-loss-counted, §15.24).
    #[tokio::test]
    async fn targetward_oversize_chunk_is_fragmented_never_dropped() {
        // A 7-byte channel id: the envelope header pushes a READ_BUF-sized read over
        // MAX_FRAME_SIZE, so a single `mux` would fail — the task must fragment.
        let channel = "console".to_owned();
        let payload: Vec<u8> = (0..100_001u32).map(|i| (i % 251) as u8).collect();

        let (in_tx, in_rx) = mpsc::channel::<Chunk>(4);
        let (mux_tx, mut mux_rx) = mpsc::channel::<Chunk>(64);
        let codec: Rc<CriticalCell<Box<dyn Codec>>> = Rc::new(CriticalCell::new(Box::new(
            codec_reference::ReferenceCodec::new(),
        )));
        let stat = Rc::new(ChannelStat::default());
        let edge = bound_edge(mux_tx);

        in_tx.send(Chunk::from(payload.clone())).await.unwrap();
        drop(in_tx); // close the source so the task drains its one chunk and returns

        channel_targetward(channel, in_rx, edge, codec.clone(), stat.clone()).await;

        // Every framed piece round-trips through `demux` byte-exact, with no loss.
        let mut reassembled: Vec<u8> = Vec::new();
        let mut frames = 0usize;
        while let Ok(frame) = mux_rx.try_recv() {
            frames += 1;
            codec.with_mut(|c| {
                c.demux(&frame, &mut |ev| {
                    assert_eq!(ev.channel.as_str(), "console");
                    if let EventKind::Data(bytes) = ev.kind {
                        reassembled.extend_from_slice(&bytes);
                    }
                })
                .unwrap();
            });
        }
        assert!(
            frames >= 2,
            "an oversize chunk must span multiple frames (got {frames})"
        );
        assert_eq!(
            reassembled, payload,
            "reassembled targetward bytes must be byte-exact"
        );
        assert_eq!(stat.accepted_targetward.get(), payload.len() as u64);
        assert_eq!(stat.discarded_targetward.get(), 0);
    }
}
