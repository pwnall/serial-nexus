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
//! channel's targetward task then parks its one framed chunk and, once the stealer
//! releases, *reclaims* the lock ahead of the on-demand FIFO queue — held-priority
//! reclaim (§15.23), which restated §6 as "FIFO among on-demand contenders, beneath
//! held reclaim" precisely so a `--wait` waiter could not inherit a stolen
//! demultiplexer's lock and corrupt the framing. The §6 stall, with commands
//! delayed, never dropped.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use serde_json::{Value, json};
use serial_nexus_codec_api::{Codec, Event, EventKind};
use serial_nexus_core::Chunk;
use serial_nexus_core::config::NodeConfig;
use serial_nexus_core::graph::{EndpointAddr, Facing};
use serial_nexus_core::state::{NodeState, NodeStatus};
use tokio::sync::mpsc;

use crate::boundary::TaskSet;
use crate::cell::CriticalCell;
use crate::runtime::{
    DropCounters, EdgeInbox, HostwardChannelStat, LossCounter, SharedFanOut, SharedTargetEdge,
    TargetwardInbox, TeardownLoss, Wiring, await_origin, forward_targetward, frame_ranges,
    route_channel_data,
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

/// The §5 hostward accounting rule, shared with the exec codec and the leg through
/// the one [`route_channel_data`] implementation (SIMP-1). The struct stays this
/// node's own — the counters' *names* are part of `state`'s contract — and only the
/// arithmetic is shared. [`HostwardChannelStat::add_dropped_full`] is left at its
/// no-op default deliberately: a slow consumer's full-buffer loss is charged to that
/// consumer's own [`DropCounters`] at the boundary that dropped it (§5), and the
/// codec's narrower `discarded_unattached` means "reached no live consumer at all".
impl HostwardChannelStat for ChannelStat {
    fn set_active(&self) {
        self.active.set(true);
    }

    fn add_delivered(&self, n: u64) {
        self.delivered_hostward
            .set(self.delivered_hostward.get() + n);
    }

    fn unattached(&self) -> &dyn LossCounter {
        &self.discarded_unattached
    }
}

/// The largest number of *distinct* unconfigured channel identities a codec node
/// remembers (CODEC-1). Same cap, same reasoning as the leg's `unbound` list
/// (LEG-2): the identities come from outside the operator's configuration — a
/// transform's decode, or an exec child's stdout — so an unbounded list is an
/// unbounded allocation driven by the wire.
pub(crate) const MAX_UNCONFIGURED: usize = 256;

/// The largest stored length of one such identity, in bytes.
pub(crate) const MAX_UNCONFIGURED_ID_LEN: usize = 64;

/// Appended to a truncated identity, so `state` never shows a shortened name as
/// though it were the real one.
const TRUNCATION_MARKER: &str = "…(truncated)";

/// Channel identities the transform decoded that this node is **not** configured
/// for, and the bytes they carried (CODEC-1, design §5 "loss is always visible and
/// attributable").
///
/// The bytes are still dropped — §8's rule that an announcement never grows the
/// graph governs a codec's channels exactly as it does a leg's, and
/// `docs/codec-authors.md` states it outright. What was missing is the *diagnosis*.
/// A mis-spelled configured channel is already visible (it sits at `waiting` /
/// `delivered_hostward: 0`, §8's configured-but-unannounced signal); nothing
/// anywhere named the identity actually on the wire, which is the only thing that
/// distinguishes a typo from a device multiplexing a stream the operator never
/// enumerated.
///
/// This is `leg::UnboundSet`'s design reused rather than re-derived: a capped,
/// insertion-ordered `Vec`, a `HashSet` for dedup, per-identity truncation with an
/// explicit marker, and an overflow *occurrence* count for what the cap refused.
/// It lives in this module because the codec node is its first user and the exec
/// codec shares this one copy (`nodes/exec.rs`), so the two cannot drift.
#[derive(Default)]
pub(crate) struct UnconfiguredChannels {
    order: Vec<String>,
    seen: HashSet<String>,
    /// Occurrences the cap refused to record — *not* distinct identities, which
    /// cannot be counted without remembering them, which is the thing being bounded.
    /// A repeat of an already-recorded identity is not an overflow.
    overflow: u64,
    /// Bytes discarded on an unconfigured identity, whether or not the identity
    /// itself was recordable.
    bytes: u64,
}

impl UnconfiguredChannels {
    /// Record `n` bytes discarded on unconfigured identity `id`. `n == 0` records
    /// the identity alone, which is what an `open` on it amounts to.
    ///
    /// The first sighting also logs once at WARN — the dedup *is* the rate limit, so
    /// an unconfigured channel screaming at 1 MB/s costs exactly one line.
    pub(crate) fn record(&mut self, id: &str, n: u64) {
        self.bytes += n;
        let id = truncate_identity(id);
        if self.seen.contains(id.as_ref()) {
            return;
        }
        if self.order.len() >= MAX_UNCONFIGURED {
            self.overflow += 1;
            return;
        }
        tracing::warn!(
            target: "codec",
            channel = %id,
            "decoded data on a channel identity this node is not configured for; \
             dropped and counted as discarded_unconfigured_channel (§5)"
        );
        let id = id.into_owned();
        self.seen.insert(id.clone());
        self.order.push(id);
    }

    /// Write the three §5 fields into a node's `state_extra` object. One writer for
    /// both node kinds, so the codec and the exec codec cannot come to report the
    /// same loss under two different names.
    pub(crate) fn report_into(&self, obj: &mut serde_json::Map<String, Value>) {
        obj.insert(
            "discarded_unconfigured_channel".to_owned(),
            json!(self.bytes),
        );
        obj.insert("unconfigured_channels".to_owned(), json!(self.order));
        obj.insert("unconfigured_overflow".to_owned(), json!(self.overflow));
    }
}

/// Bound the stored length of a decoded identity, marking a truncation explicitly.
/// Truncation lands on a `char` boundary: the identity is transform-supplied UTF-8
/// and a split code point would not survive JSON.
fn truncate_identity(id: &str) -> Cow<'_, str> {
    if id.len() <= MAX_UNCONFIGURED_ID_LEN {
        return Cow::Borrowed(id);
    }
    let mut end = MAX_UNCONFIGURED_ID_LEN;
    while end > 0 && !id.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}{TRUNCATION_MARKER}", &id[..end]))
}

/// The prefix every demux-failure fault reason carries, so the demux task can tell
/// *its own* fault from a status set elsewhere (an edge detach) and clear only the
/// former.
const DEMUX_FAULT_PREFIX: &str = "codec demux error: ";

/// The node's demux-side protocol health (WIRE-1, design §7.5 / §5).
///
/// `Codec::demux` returning `Err` is the only way the trait can express §7.5's
/// sanctioned "treats any framing violation as a protocol error and never resyncs"
/// policy — and a codec that never resyncs is, after its first violation,
/// permanently unable to deliver anything. The daemon used to answer that with one
/// `tracing::warn!` and nothing else: no counter, no `state` field, and a node that
/// went on reporting `active` with `framing_errors: 0` (that is `resync_count()`,
/// whose trait default is 0 for exactly the codecs that never resync) while 100% of
/// the device's output vanished. Unreachable through the shipped registry — the
/// reference codec resyncs by length guidance and always returns `Ok` — and fully
/// reachable through the first-class out-of-tree codec surface (§15.26).
///
/// So a refusal now **faults the node** instead of only logging it: `active` is a
/// claim about delivery, and a transform refusing the stream is not delivering. The
/// fault is deliberately *not* latched — a transform that decodes the next chunk
/// clears it — because an `Err`-then-`Ok` codec is legal too and latching would
/// misreport a transient violation as a dead node for the rest of the session.
#[derive(Default)]
struct DemuxHealth {
    /// `Codec::demux` refusals since the node started.
    errors: Cell<u64>,
    /// Multiplexed *input* bytes a refusing `demux` did **not** turn into hostward
    /// payload: the chunk as it arrived, less the `data` bytes the same call emitted
    /// before it failed.
    ///
    /// The subtraction is not a refinement, it is a correction. Nothing in the
    /// `serial-nexus-codec-api` contract says a transform must emit nothing before returning
    /// `Err`, and the realistic shape does the opposite: a non-resyncing framer
    /// decodes the good frames out of a 64 KiB chunk and refuses on the corrupt tail.
    /// Every one of those events is still routed and credited to its channel's
    /// `delivered_hostward` below, so charging the whole chunk here reported the same
    /// payload as delivered *and* as lost — §5 wants loss attributable, and a counter
    /// that double-counts is worse than a coarse one, because it makes the two
    /// numbers irreconcilable. Framing overhead of the salvaged frames is still
    /// charged (the trait cannot report consumption), which errs toward reporting
    /// loss rather than hiding it; and a call that emits *more* than it was handed —
    /// legal, a transform flushing frames buffered from earlier chunks — saturates at
    /// zero rather than wrapping.
    discarded_hostward: Cell<u64>,
    /// Whether the node currently carries *this* task's fault, so the success path
    /// costs one non-atomic load per chunk rather than a status read.
    faulted: Cell<bool>,
    /// The most recent refusal's message, surfaced in `state` so the diagnosis does
    /// not live only in the daemon log. Also the rate limiter: an unchanged message
    /// neither re-logs nor re-stamps the node's status (§7's transition stamp).
    last: CriticalCell<Option<String>>,
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
    /// Identities the transform decoded that this node has no channel for, bounded
    /// and named (CODEC-1). Shared with the demux task.
    unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
    /// The demux-side protocol health (WIRE-1). Shared with the demux task, which
    /// is what faults the node on a `Codec::demux` refusal.
    demux: Rc<DemuxHealth>,
    /// Targetward bytes destroyed because this node was torn down while they were
    /// still queued for it (§5, notes §3.31): what its pumps never got to look at,
    /// as distinct from what they looked at and decided to discard.
    teardown_loss: TeardownLoss,
    tasks: TaskSet,
    /// The node's observed status *and the moment it entered it* (§7). Shared with
    /// the demux task (WIRE-1) the way the exec codec shares its own, so a framing
    /// refusal can be reported as a fault rather than only logged.
    status: Rc<CriticalCell<NodeState>>,
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
            teardown_loss: TeardownLoss::default(),
            name: name.clone(),
            codec_name: codec_name.clone(),
            faces: *faces,
            channels: channels.clone(),
            codec: Rc::new(CriticalCell::new(codec)),
            stats: Rc::new(stats),
            mux_counters: None,
            mux_edge: None,
            mux_discarded_targetward: Rc::new(Cell::new(0)),
            unconfigured: Rc::new(CriticalCell::new(UnconfiguredChannels::default())),
            demux: Rc::new(DemuxHealth::default()),
            tasks: TaskSet::default(),
            status: Rc::new(CriticalCell::new(NodeState::new(NodeStatus::Active))),
        }
    }

    /// Claim every host-facing endpoint's targetward receiver and park it in a
    /// draining task, for the `start` paths that return before the data plane is
    /// built (§15.8).
    ///
    /// A node that comes up `waiting` still owns its endpoints, and those endpoints
    /// still have live senders. Leaving the receivers in the wiring plan drops them
    /// when `load` finishes, which is indistinguishable to a writer from the graph
    /// being torn down — see the MAP-1 chain in `start`. Draining keeps every writer
    /// healthy and every lost byte counted (§5).
    ///
    /// **Reachability, stated honestly (SIMPB-5).** The one caller is `start`'s
    /// `faces = "host"` exit, and for a re-multiplexer it is the *multiplexed*
    /// endpoint that faces host while every channel faces target — so
    /// `Wiring::build` never puts a channel receiver in `host_targetward_rx` and
    /// this sweep finds exactly one. It still sweeps both, as cheap insurance
    /// against a future early return in `start` (v12's edge surgery removed the
    /// second one, which is what made the channel arm dead), and charges everything
    /// it finds to the one node-level counter — the shape the exec codec already
    /// had. Note what this is *not*: the read-only demux (`write_mode = "never"` on
    /// a `faces = "target"` codec's multiplexed edge) never comes through here at
    /// all. That case is handled by `channel_targetward`'s `await_origin` → `None`
    /// arm, and that arm is load-bearing — `p9_unwired_interior.rs` pins it.
    fn drain_unwired_channels(&mut self, wiring: &mut Wiring) {
        let addrs: Vec<EndpointAddr> = std::iter::once(EndpointAddr::node(&self.name))
            .chain(
                self.channels
                    .iter()
                    .map(|ch| EndpointAddr::channel(&self.name, ch)),
            )
            .collect();
        for addr in addrs {
            let Some(rx) = wiring.host_targetward_rx.remove(&addr) else {
                continue;
            };
            self.tasks
                .push(tokio::task::spawn_local(unwired_targetward_drain(
                    self.teardown_loss.watch(rx),
                    self.mux_discarded_targetward.clone(),
                )));
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
            self.status.with_mut(|s| {
                s.set(NodeStatus::Waiting {
                    reason: "standalone re-multiplexer orientation (faces=host) has no driver; \
                             deferred work (§14) — a leg node re-multiplexes through its own \
                             link codec"
                        .to_owned(),
                })
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
            self.status.with_mut(|s| {
                s.set(NodeStatus::Waiting {
                    reason: "multiplexed side has no attached upstream".to_owned(),
                })
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
                DemuxSinks {
                    channel_sinks,
                    channel_feeds,
                    stats: self.stats.clone(),
                    unconfigured: self.unconfigured.clone(),
                    demux: self.demux.clone(),
                    status: self.status.clone(),
                },
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
                self.teardown_loss.watch(rx),
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
        // An edge attach/detach is the operator's own act, so it wins over a demux
        // fault the transform may be sitting in (WIRE-1): the fault flag is cleared
        // with it, and the next refusal re-reports one.
        self.demux.faulted.set(false);
        self.status.with_mut(|s| {
            s.set(if attached {
                NodeStatus::Active
            } else {
                NodeStatus::Waiting {
                    reason: "multiplexed side has no attached upstream".to_owned(),
                }
            })
        });
    }

    pub fn status(&self) -> NodeState {
        self.status.with(|s| s.clone())
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
        let mut obj = json!({
            "codec": self.codec_name,
            "faces": self.faces.to_string(),
            // The transform's *own* framing accounting (`resync_count`), which is 0
            // by trait default for exactly the codecs that never resync — which is
            // why `demux_errors` beside it is the daemon's own count and not a
            // duplicate of this one (WIRE-1).
            "framing_errors": framing_errors,
            // WIRE-1: `Codec::demux` refusals, the multiplexed input bytes they cost,
            // and the last message — the §7.5 never-resync policy's only signal, which
            // used to be a log line and nothing else.
            "demux_errors": self.demux.errors.get(),
            "last_demux_error": self.demux.last.with(|l| l.clone()),
            // The multiplexed side's own hostward drops (the codec falling behind
            // the serial), so the loss stays located and attributable (§5).
            "discarded_at_teardown": self.teardown_loss.bytes(),
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
                // Targetward bytes drained at an unwired multiplexed endpoint (the
                // re-multiplexer orientation, §14) — inert rather than destructive,
                // and counted rather than silent (§5).
                "discarded_targetward": mux_discarded_targetward,
                // Multiplexed bytes a refusing `demux` never turned into events.
                "discarded_hostward": self.demux.discarded_hostward.get(),
            },
            "channels": channels,
        });
        // CODEC-1's three fields, written by the one shared reporter so the codec and
        // the exec codec name the same loss the same way.
        if let Value::Object(map) = &mut obj {
            self.unconfigured.with(|u| u.report_into(map));
        }
        obj
    }

    /// Ask this node's tasks to stop, without waiting (§16.1, BND-1). An interior
    /// codec owns no blocking thread and no environment to release, so signalling
    /// *is* its whole teardown; the method exists so the daemon can signal every
    /// node uniformly before it pays any node's join cost.
    ///
    /// It needs no `impl Drop`: [`TaskSet`] aborts what it holds when the node value
    /// dies, so "a node's tasks die with the node" holds by type rather than by a
    /// hand-written copy of this body (SIMPB-10).
    pub fn signal_stop(&mut self) {
        // Count what teardown destroys before `abort_all` drops the futures the
        // queues live in — the ordering is the fix (§5, notes §3.31).
        self.teardown_loss.drain();
        self.tasks.abort_all();
    }

    pub fn teardown(&mut self) {
        self.signal_stop();
    }

    /// Targetward bytes this node destroyed at teardown (§5, notes §3.31). `0` until
    /// `signal_stop` has run, which is where the queues are drained and counted.
    pub fn discarded_at_teardown(&self) -> u64 {
        self.teardown_loss.bytes()
    }
}

/// Everything the hostward demux task writes into: the per-channel fan-outs, tap
/// feeds and stats, plus the three node-level signals a decode can raise — an
/// unconfigured identity (CODEC-1), a `Codec::demux` refusal (WIRE-1), and the node
/// status the latter faults. Bundled because the task took five parameters before
/// and the honest count is eight.
struct DemuxSinks {
    channel_sinks: HashMap<String, SharedFanOut>,
    channel_feeds: HashMap<String, TapFeed>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
    demux: Rc<DemuxHealth>,
    status: Rc<CriticalCell<NodeState>>,
}

impl DemuxSinks {
    /// Record a `Codec::demux` refusal (WIRE-1): count it, charge what the refusal
    /// actually cost, remember the message and fault the node. Only a *changed*
    /// message re-logs and re-stamps the status, so a transform failing on every
    /// chunk of a firehose costs one WARN line and one status transition, not one per
    /// chunk.
    ///
    /// `salvaged` is the `data` payload this same `demux` call emitted before
    /// failing, which the caller sums off the event list; it is subtracted because
    /// those bytes are routed and credited to their channels, so charging them here
    /// too reports one payload twice (see [`DemuxHealth::discarded_hostward`]).
    fn note_demux_error(&self, chunk_len: u64, salvaged: u64, reason: String) {
        let d = &self.demux;
        d.errors.set(d.errors.get() + 1);
        d.discarded_hostward.add(chunk_len.saturating_sub(salvaged));
        let changed = d.last.with_mut(|last| {
            if last.as_deref() == Some(reason.as_str()) {
                return false;
            }
            *last = Some(reason.clone());
            true
        });
        if !changed && d.faulted.get() {
            return;
        }
        tracing::warn!(target: "codec", "{DEMUX_FAULT_PREFIX}{reason}");
        self.status.with_mut(|s| {
            s.set(NodeStatus::Faulted {
                reason: format!("{DEMUX_FAULT_PREFIX}{reason}"),
            })
        });
        d.faulted.set(true);
    }

    /// The transform decoded a chunk after a refusal, so the violation was transient
    /// and the node is delivering again. Only *this* task's fault is cleared — a
    /// `waiting` set by an edge detach, or a fault set elsewhere, is left alone.
    fn clear_demux_fault(&self) {
        self.demux.faulted.set(false);
        self.status.with_mut(|s| {
            if matches!(s.status(), NodeStatus::Faulted { reason } if reason.starts_with(DEMUX_FAULT_PREFIX))
            {
                s.set(NodeStatus::Active);
            }
        });
    }
}

/// Hostward demux task: drain the multiplexed stream, decode per-channel events,
/// and fan each channel's data out to its consumers (lossy `try_send` at the
/// consuming boundary, §5). The codec borrow is synchronous and dropped before the
/// fan-out and before the next `recv().await`.
async fn hostward_demux(
    codec: Rc<CriticalCell<Box<dyn Codec>>>,
    mut inbox: EdgeInbox,
    out: DemuxSinks,
) {
    // One task across every upstream edge this node is given over its life (§15.35):
    // it parks on the inbox while unattached, drains an edge until `disconnect`
    // closes it, then parks again — so the codec's framing state and its channels'
    // tasks survive edge surgery untouched.
    while let Some(mut mux_rx) = inbox.recv().await {
        while let Some(chunk) = mux_rx.recv().await {
            let mut events = Vec::new();
            let refused = codec.with_mut(|c| {
                c.demux(&chunk, &mut |ev| events.push(ev))
                    .err()
                    .map(|e| e.to_string())
            });
            match refused {
                // WIRE-1: a refusal is the §7.5 never-resync policy speaking, and it
                // is now visible in `state` (counter, byte cost, message, faulted
                // node) rather than only in the log. The cost is the chunk *less*
                // whatever the same call already emitted — those events are routed
                // and credited below, and a partial decode is the normal shape of a
                // non-resyncing framer, not an exotic one.
                Some(reason) => {
                    let salvaged: u64 = events
                        .iter()
                        .map(|ev| match &ev.kind {
                            EventKind::Data(bytes) => bytes.len() as u64,
                            _ => 0,
                        })
                        .sum();
                    out.note_demux_error(chunk.len() as u64, salvaged, reason)
                }
                None if out.demux.faulted.get() => out.clear_demux_fault(),
                None => {}
            }
            for ev in events {
                let stat = out.stats.get(ev.channel.as_str());
                match ev.kind {
                    EventKind::Data(bytes) => {
                        let n = bytes.len() as u64;
                        // CODEC-1: an identity this node has no channel for. §8 still
                        // governs the bytes — an announcement never grows the graph,
                        // so they are dropped — but §5 governs the *accounting*, so
                        // they are counted and the identity is named. Nothing else in
                        // the daemon could answer "what is actually on the wire?",
                        // which is the whole question a channel typo raises.
                        let Some(s) = stat else {
                            out.unconfigured
                                .with_mut(|u| u.record(ev.channel.as_str(), n));
                            continue;
                        };
                        // The one shared per-channel hostward routing block (SIMP-1):
                        // latch active, mirror to this channel's tap hub for taps and
                        // the replay ring (§17) independent of whether a graph consumer
                        // is bound, then fan out to the graph sinks alone (§5, F1). A
                        // channel that reached no live consumer — none bound, or every
                        // sink permanently `Closed` after a cascade removal —
                        // discards-with-count inside the helper.
                        route_channel_data(
                            &bytes,
                            out.channel_feeds.get(ev.channel.as_str()),
                            out.channel_sinks.get(ev.channel.as_str()),
                            Some(&**s),
                        );
                    }
                    EventKind::Open => match stat {
                        Some(s) => s.active.set(true),
                        // An announcement on an identity the operator never
                        // enumerated: no bytes to charge, but the name is the
                        // diagnosis (CODEC-1).
                        None => out
                            .unconfigured
                            .with_mut(|u| u.record(ev.channel.as_str(), 0)),
                    },
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

/// Keep an unwired host-facing endpoint's targetward receiver alive, discarding and
/// counting what arrives (§5) — the drain [`CodecNode::drain_unwired_channels`]
/// parks on whatever that defensive sweep finds.
///
/// The receiver's *senders* stay live — in `GraphState::endpoint_targetward` and in
/// every attached writer origin — so dropping it would close the channel underneath
/// them, and for a pty origin the failed write ends `read_and_poll` along with
/// presence latching, last-close handling and detach-release (MAP-1). Draining makes
/// the node inert instead of destructive, with the loss attributable in `state`.
async fn unwired_targetward_drain(rx: TargetwardInbox, discarded: Rc<Cell<u64>>) {
    while let Some(bytes) = rx.recv().await {
        discarded.add(bytes.len() as u64);
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
    rx: TargetwardInbox,
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
        let Some(origin) = await_origin(&mux_edge).await else {
            stat.discarded_targetward.add(total as u64);
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
                stat.discarded_targetward.add((total - off) as u64);
                break;
            }
            // The shared send-and-charge step (SIMP-2): gate on holding the serial's
            // write lock (the codec's held origin) — a `send --steal` transiently
            // ousts it and the framed piece parks in `framed` until the origin
            // reclaims the lock ahead of the on-demand FIFO queue (held-priority
            // reclaim, §15.23) — then hand the frame to the device. Either exit costs
            // the same undelivered tail: the upstream endpoint was torn down, or its
            // channel closed between the grant and the send. Stop framing *this* chunk but
            // keep the task — a `connect` may give this codec a new upstream, and the
            // receiver's senders stay live either way (§5, §15.35).
            let delivered =
                forward_targetward(&origin, (total - off) as u64, || Some(Chunk::from(framed)))
                    .await
                    .charge(&stat.discarded_targetward);
            if !delivered {
                break;
            }
            stat.accepted_targetward
                .set(stat.accepted_targetward.get() + piece_len);
        }
    }
}

/// Guards for the signals a *custom* transform raises — WIRE-1 (a `Codec::demux`
/// refusal had no counter, no state field and no status change), CODEC-1 (data on an
/// unconfigured identity vanished with nothing naming it), the codec half of LEGD-2
/// (`delivered_hostward` credited bytes a full sink never took), and 37-CODEC-3 (the
/// targetward mirror: a `Codec::mux` refusing a fragment must charge the residual).
/// None of these need the reference codec — that is the whole point. The *shipped*
/// transform never returns `Err` in either direction, so every arm here is reachable
/// only through the §15.26 out-of-tree surface and its subject is a stub.
#[cfg(test)]
mod signal_tests {
    use super::*;
    use crate::runtime::{SharedLock, TargetEdge, frame_payload_cap};
    use serial_nexus_codec_api::CodecError;
    use serial_nexus_core::lock::{Arbitration, EndpointLock, OriginId, WriteMode};
    use tokio::sync::broadcast;

    /// One scripted `demux` call: decode the chunk onto a channel, refuse it, or —
    /// the shape a real non-resyncing framer actually produces — decode the leading
    /// `n` bytes onto a channel and *then* refuse the rest.
    ///
    /// The third variant exists because its absence hid a defect: with `Refuse`
    /// emitting nothing, no guard here could see that the refusal charge and the
    /// per-channel delivery credit were counting the same bytes.
    #[derive(Clone, Copy)]
    enum Step {
        Emit(&'static str),
        Refuse(&'static str),
        /// `(channel, payload bytes emitted, refusal reason)`. The payload is the
        /// head of the input; asking for more than the chunk holds pads it, which
        /// models the other legal shape — a transform flushing a frame it assembled
        /// partly out of an *earlier* chunk and then refusing.
        EmitThenRefuse(&'static str, usize, &'static str),
    }

    /// A transform that follows a script — the codec §7.5 sanctions and the shipped
    /// registry does not contain: one that "treats any framing violation as a
    /// protocol error and never resyncs", which the trait can only express as `Err`.
    ///
    /// The `mux` half is scripted by count rather than by step, because the
    /// targetward caller is a fragmentation loop: what matters is *which fragment*
    /// gets refused, not what the event carried.
    struct StubCodec {
        script: Vec<Step>,
        calls: usize,
        /// How many `mux` calls to accept before refusing this one and every one
        /// after it. `usize::MAX` for the hostward tests, which never mux.
        mux_ok: usize,
        mux_calls: usize,
    }

    impl Codec for StubCodec {
        fn name(&self) -> &str {
            "stub"
        }

        fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError> {
            let step = self
                .script
                .get(self.calls)
                .copied()
                .unwrap_or(Step::Refuse("past the script"));
            self.calls += 1;
            match step {
                Step::Emit(channel) => {
                    emit(Event::data(channel, input.to_vec()));
                    Ok(())
                }
                Step::Refuse(why) => Err(CodecError::Framing(why.to_owned())),
                Step::EmitThenRefuse(channel, take, why) => {
                    let mut payload = input[..take.min(input.len())].to_vec();
                    payload.resize(take, b'x'); // bytes buffered from an earlier chunk
                    emit(Event::data(channel, payload));
                    Err(CodecError::Framing(why.to_owned()))
                }
            }
        }

        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError> {
            let nth = self.mux_calls;
            self.mux_calls += 1;
            if nth >= self.mux_ok {
                return Err(CodecError::Framing(format!("refused piece {nth}")));
            }
            if let EventKind::Data(bytes) = &event.kind {
                out.extend_from_slice(bytes);
            }
            Ok(())
        }
    }

    /// The node-level signals `hostward_demux` writes into, held together so a test
    /// asserts on the same values `state_extra` renders.
    struct Signals {
        demux: Rc<DemuxHealth>,
        unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
        status: Rc<CriticalCell<NodeState>>,
        stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    }

    /// Run `hostward_demux` to completion over one edge carrying `chunks`, with the
    /// node configured for `channels` and each channel wired to `sinks`.
    async fn run_demux(
        script: Vec<Step>,
        channels: &[&str],
        chunks: &[&[u8]],
        sinks: HashMap<String, SharedFanOut>,
    ) -> Signals {
        let stats: Rc<HashMap<String, Rc<ChannelStat>>> = Rc::new(
            channels
                .iter()
                .map(|c| ((*c).to_owned(), Rc::new(ChannelStat::default())))
                .collect(),
        );
        let signals = Signals {
            demux: Rc::new(DemuxHealth::default()),
            unconfigured: Rc::new(CriticalCell::new(UnconfiguredChannels::default())),
            status: Rc::new(CriticalCell::new(NodeState::new(NodeStatus::Active))),
            stats: stats.clone(),
        };
        let codec: Rc<CriticalCell<Box<dyn Codec>>> =
            Rc::new(CriticalCell::new(Box::new(StubCodec {
                script,
                calls: 0,
                mux_ok: usize::MAX, // hostward only: nothing here muxes
                mux_calls: 0,
            })));

        let (inbox_tx, inbox_rx) = mpsc::channel(1);
        let (edge_tx, edge_rx) = mpsc::channel(chunks.len().max(1));
        for c in chunks {
            edge_tx
                .send(Chunk::copy_from_slice(c))
                .await
                .expect("the demux task has not started yet, so the buffer takes it");
        }
        drop(edge_tx); // the edge closes once drained
        inbox_tx.send(edge_rx).await.expect("inbox takes the edge");
        drop(inbox_tx); // ...and so does the inbox, so the task returns

        hostward_demux(
            codec,
            inbox_rx,
            DemuxSinks {
                channel_sinks: sinks,
                channel_feeds: HashMap::new(),
                stats,
                unconfigured: signals.unconfigured.clone(),
                demux: signals.demux.clone(),
                status: signals.status.clone(),
            },
        )
        .await;
        signals
    }

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
    /// `mux_tx` — the shape `connect` leaves behind (§15.35). Shared with the
    /// reference-codec `tests` module below, which needs the identical fixture.
    pub(super) fn bound_edge(mux_tx: mpsc::Sender<Chunk>) -> SharedTargetEdge {
        let (lock, id) = held_lock();
        let slot = TargetEdge::new();
        slot.with_mut(|e| {
            e.attached = true;
            e.registered = Some((lock, id));
            e.writer = Some(mux_tx);
        });
        slot
    }

    /// One consumer sink of depth `cap`, plus its receiver and drop counters.
    fn sink(cap: usize) -> (SharedFanOut, mpsc::Receiver<Chunk>, Arc<DropCounters>) {
        let (tx, rx) = mpsc::channel(cap);
        let counters = Arc::new(DropCounters::default());
        let fanout = crate::runtime::FanOutList::new();
        fanout.attach(crate::runtime::AttachedSink {
            target: "consumer".parse().expect("address parses"),
            tx,
            counters: counters.clone(),
        });
        (fanout, rx, counters)
    }

    /// WIRE-1: a `Codec::demux` refusal is counted, charged the multiplexed bytes it
    /// cost, kept as a message in `state`, and — because a codec that never resyncs
    /// is permanently unable to deliver — **faults the node**. Before this the whole
    /// response was one `tracing::warn!`: the node went on reporting `active` with
    /// `framing_errors: 0` and every counter frozen while 100% of the device's
    /// output disappeared.
    #[tokio::test]
    async fn a_demux_refusal_is_counted_charged_and_faults_the_node() {
        let s = run_demux(
            vec![Step::Refuse("bad CRC"), Step::Refuse("bad CRC")],
            &["c0"],
            &[b"aaaaaaaa", b"bbbb"],
            HashMap::new(),
        )
        .await;

        assert_eq!(s.demux.errors.get(), 2, "both refusals are counted");
        assert_eq!(
            s.demux.discarded_hostward.get(),
            12,
            "the multiplexed bytes a refusing demux never decoded are charged (§5)"
        );
        assert_eq!(
            s.demux.last.with(|l| l.clone()).as_deref(),
            Some("framing error: bad CRC"),
            "the diagnosis reaches state, not only the log"
        );
        let status = s.status.with(|st| st.status().clone());
        assert!(
            matches!(&status, NodeStatus::Faulted { reason } if reason.contains("bad CRC")),
            "a refusing transform is not delivering, so the node is not active: {status:?}"
        );
    }

    /// WIRE-1: the fault is *not* latched. A transform that decodes the next chunk
    /// was suffering a transient violation, and reporting it as a dead node for the
    /// rest of the session would be its own misdiagnosis.
    #[tokio::test]
    async fn a_recovered_demux_clears_its_own_fault() {
        let s = run_demux(
            vec![Step::Refuse("bad CRC"), Step::Emit("c0")],
            &["c0"],
            &[b"junk", b"good"],
            HashMap::new(),
        )
        .await;

        assert_eq!(s.demux.errors.get(), 1);
        assert!(!s.demux.faulted.get());
        assert!(
            matches!(s.status.with(|st| st.status().clone()), NodeStatus::Active),
            "the node delivers again, so it says so"
        );
    }

    /// CODEC-1: data decoded onto an identity the node has no channel for is still
    /// dropped (§8: an announcement never grows the graph), but it is now counted
    /// and the identity is *named*. The name is the whole finding: a mis-spelled
    /// configured channel was already visible sitting at `waiting`, while nothing
    /// anywhere said what was actually on the wire.
    #[tokio::test]
    async fn data_on_an_unconfigured_channel_is_counted_and_named() {
        let s = run_demux(
            vec![Step::Emit("gps"), Step::Emit("gps"), Step::Emit("c0")],
            &["c0"],
            &[b"1234", b"567", b"ok"],
            HashMap::new(),
        )
        .await;

        s.unconfigured.with(|u| {
            let mut obj = serde_json::Map::new();
            u.report_into(&mut obj);
            assert_eq!(obj["discarded_unconfigured_channel"], json!(7));
            assert_eq!(obj["unconfigured_channels"], json!(["gps"]));
            assert_eq!(obj["unconfigured_overflow"], json!(0));
        });
        // The configured channel is untouched by its neighbour's noise.
        assert_eq!(s.stats["c0"].discarded_unattached.get(), 2);
    }

    /// CODEC-1's bound: the identity list is capped, deduplicated, truncated with an
    /// explicit marker on a `char` boundary, and the occurrences the cap refused are
    /// counted — the leg's `UnboundSet` terms (LEG-2), reused rather than re-derived,
    /// because these identities come from the wire and not from configuration.
    #[test]
    fn unconfigured_identities_are_capped_deduplicated_and_truncated() {
        let mut set = UnconfiguredChannels::default();
        for i in 0..MAX_UNCONFIGURED + 50 {
            set.record(&format!("ch-{i}"), 1);
        }
        assert_eq!(set.order.len(), MAX_UNCONFIGURED, "the list is bounded");
        assert_eq!(set.overflow, 50, "what the cap refused is counted");
        assert_eq!(set.bytes, (MAX_UNCONFIGURED + 50) as u64, "no byte is lost");

        // A repeat is neither duplicated nor counted as overflow, but its bytes count.
        let before = set.overflow;
        set.record("ch-0", 4);
        assert_eq!(set.order.len(), MAX_UNCONFIGURED);
        assert_eq!(set.overflow, before);

        // A multi-byte identity truncates on a char boundary, marked.
        let mut set = UnconfiguredChannels::default();
        set.record(&"€".repeat(MAX_UNCONFIGURED_ID_LEN), 0);
        let stored = &set.order[0];
        assert!(stored.ends_with(TRUNCATION_MARKER), "{stored}");
        assert!(
            stored.len() <= MAX_UNCONFIGURED_ID_LEN + TRUNCATION_MARKER.len(),
            "{stored}"
        );
    }

    /// LEGD-2 (codec half): `fan_out` reports `live = true` for a sink whose bounded
    /// buffer was **full** — deliberately, it is still a live consumer — so crediting
    /// the whole chunk on `live` counted bytes as delivered that nobody received.
    /// Only what a sink actually took is credited now, reported as
    /// [`crate::runtime::FanOut::delivered`] rather than derived by the caller. The
    /// *mixed* fan-out (one sink Ok, one Full), where the first correction
    /// under-counted instead, is pinned once for all three producers in
    /// `runtime::tests`, since the arithmetic is one function.
    #[tokio::test]
    async fn delivered_hostward_credits_only_what_a_sink_took() {
        let (fanout, _rx, counters) = sink(1);
        let sinks = HashMap::from([("c0".to_owned(), fanout)]);
        // Two 5-byte chunks, one consumer of depth 1 that never drains: the first
        // chunk is taken, the second finds the buffer full.
        let s = run_demux(
            vec![Step::Emit("c0"), Step::Emit("c0")],
            &["c0"],
            &[b"hello", b"world"],
            sinks,
        )
        .await;

        assert_eq!(counters.dropped_full(), 5, "the full sink was charged");
        assert_eq!(
            s.stats["c0"].delivered_hostward.get(),
            5,
            "only the chunk a consumer actually took is delivered"
        );
        assert_eq!(
            s.stats["c0"].discarded_unattached.get(),
            0,
            "a full sink is live, so this is not an unattached loss"
        );
    }

    /// A refusal charges the bytes it lost, **not** the ones it delivered on the way.
    /// A non-resyncing framer's realistic failure is a partial one — decode the good
    /// frames out of the chunk, refuse on the corrupt tail — and every event it
    /// emitted first is still routed and credited to its channel below. Charging the
    /// whole chunk reported that payload as delivered *and* as discarded at the same
    /// time, on a node the same refusal faults.
    ///
    /// Fail-first: against `note_demux_error(chunk.len(), …)` this run charged 12
    /// instead of 4 while `delivered_hostward` was 8 — 20 bytes accounted for a
    /// 12-byte chunk.
    #[tokio::test]
    async fn a_partial_decode_charges_only_the_bytes_the_refusal_lost() {
        let (fanout, mut rx, _counters) = sink(4);
        let s = run_demux(
            // 12 bytes in, 8 decoded onto c0, then the tail is refused.
            vec![Step::EmitThenRefuse("c0", 8, "bad CRC on the tail frame")],
            &["c0"],
            &[b"aaaaaaaabbbb"],
            HashMap::from([("c0".to_owned(), fanout)]),
        )
        .await;

        assert_eq!(
            &rx.try_recv().expect("the decoded prefix was routed")[..],
            b"aaaaaaaa",
            "the premise: events emitted before the error are still delivered"
        );
        assert_eq!(s.stats["c0"].delivered_hostward.get(), 8);
        assert_eq!(
            s.demux.discarded_hostward.get(),
            4,
            "only the tail the transform refused is loss"
        );
        assert_eq!(
            s.stats["c0"].delivered_hostward.get() + s.demux.discarded_hostward.get(),
            12,
            "delivered + discarded must reconcile against the chunk, not exceed it"
        );
        assert_eq!(s.demux.errors.get(), 1, "the refusal is still a refusal");
        assert!(
            matches!(
                s.status.with(|st| st.status().clone()),
                NodeStatus::Faulted { .. }
            ),
            "a partial decode is still a refusal, so the node still faults (WIRE-1)"
        );
    }

    /// 37-CODEC-3, the targetward mirror of WIRE-1: a transform that refuses a
    /// *fragment* mid-chunk must charge the undelivered residual and stop framing.
    ///
    /// The branch is the one AGENTS.md §4 names a tripwire — "never skip-on-encode
    /// error" — and §15.27 records that exact bug shipping three times in three
    /// framers before review caught the last one. Here it was unreachable and
    /// therefore unguarded: the reference codec's fragments provably fit the frame
    /// bound ([`frame_ranges`] is sized off [`frame_payload_cap`]), so nothing in the
    /// shipped registry can refuse a piece, and a regression that simply `break`ed
    /// without the charge would have compiled, passed, and shipped. A `mux` that errs
    /// after the first piece is reachable only through the §15.26 out-of-tree
    /// surface, which is what this stub stands in for.
    ///
    /// The assertion is the §5 reconciliation, not the two counters separately:
    /// `accepted + discarded == total` for the whole source chunk, so no arithmetic
    /// that loses or double-counts a byte can pass.
    #[tokio::test]
    async fn a_mux_refusal_mid_chunk_charges_the_residual_and_stops_framing() {
        let channel = "c0";
        // Three fragments: the first is accepted, the second refused, the third never
        // attempted. A chunk of exactly one fragment could not tell "charged the
        // residual" from "charged the chunk".
        let cap = frame_payload_cap(channel);
        let total = 2 * cap + 7;
        let payload: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

        let (in_tx, in_rx) = mpsc::channel::<Chunk>(1);
        let (mux_tx, mut mux_rx) = mpsc::channel::<Chunk>(8);
        let codec: Rc<CriticalCell<Box<dyn Codec>>> =
            Rc::new(CriticalCell::new(Box::new(StubCodec {
                script: Vec::new(), // targetward only: nothing here demuxes
                calls: 0,
                mux_ok: 1,
                mux_calls: 0,
            })));
        let stat = Rc::new(ChannelStat::default());

        in_tx
            .send(Chunk::from(payload))
            .await
            .expect("the task has not started, so the buffer takes it");
        drop(in_tx); // close the source so the task drains its one chunk and returns
        channel_targetward(
            channel.to_owned(),
            TargetwardInbox::new(in_rx),
            bound_edge(mux_tx),
            codec,
            stat.clone(),
        )
        .await;

        assert_eq!(
            stat.accepted_targetward.get(),
            cap as u64,
            "the piece the transform framed is the only one delivered"
        );
        assert_eq!(
            stat.discarded_targetward.get(),
            (total - cap) as u64,
            "the refusal costs the whole undelivered tail, not one fragment"
        );
        assert_eq!(
            stat.accepted_targetward.get() + stat.discarded_targetward.get(),
            total as u64,
            "every source byte is either accepted targetward or charged as loss (§5)"
        );

        let mut frames = 0usize;
        while mux_rx.try_recv().is_ok() {
            frames += 1;
        }
        assert_eq!(
            frames, 1,
            "framing stops at the refusal; the third fragment is never attempted"
        );
    }

    /// The saturating half of the same arithmetic: a transform may flush frames it
    /// buffered from *earlier* chunks and then refuse, emitting more payload than the
    /// chunk it was handed. That is not negative loss — it is none — and it must not
    /// wrap a `u64` counter into `state`.
    #[tokio::test]
    async fn a_refusal_that_emits_more_than_it_was_handed_charges_nothing() {
        let (fanout, _rx, _counters) = sink(4);
        let s = run_demux(
            // 64 bytes of payload out of a 4-byte chunk: the surplus is frame data
            // the transform had buffered from the previous chunk.
            vec![
                Step::Emit("c0"),
                Step::EmitThenRefuse("c0", 64, "short frame"),
            ],
            &["c0"],
            &[b"aaaa", b"bbbb"],
            HashMap::from([("c0".to_owned(), fanout)]),
        )
        .await;

        assert_eq!(
            s.demux.discarded_hostward.get(),
            0,
            "the refusing call emitted its whole input; nothing was lost"
        );
        assert_eq!(s.demux.errors.get(), 1);
    }
}

#[cfg(all(test, feature = "codec-reference"))]
mod tests {
    use super::*;
    // The held-lock / bound-edge fixture lives with the stub-codec guards above,
    // since both modules need the identical shape and only one of them is gated on
    // the reference codec.
    use super::signal_tests::bound_edge;
    use crate::runtime::TargetEdge;

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
                    TargetwardInbox::new(rx),
                    TargetEdge::read_only(),
                    Rc::new(CriticalCell::new(Box::new(
                        serial_nexus_codec_reference::ReferenceCodec::new(),
                    ) as Box<dyn Codec>)),
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
            serial_nexus_codec_reference::ReferenceCodec::new(),
        )));
        let stat = Rc::new(ChannelStat::default());
        let edge = bound_edge(mux_tx);

        in_tx.send(Chunk::from(payload.clone())).await.unwrap();
        drop(in_tx); // close the source so the task drains its one chunk and returns

        channel_targetward(
            channel,
            TargetwardInbox::new(in_rx),
            edge,
            codec.clone(),
            stat.clone(),
        )
        .await;

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
