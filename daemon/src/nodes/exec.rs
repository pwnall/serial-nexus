//! Exec codec node (design §7.6): the escape hatch — a codec whose transform is
//! a child process, so protocol tools under any license run unmodified behind a
//! documented, non-linking interface (§13).
//!
//! **The child protocol (ADR §15.22).** The child speaks the shared envelope
//! (`serial-nexus-codec-api`) on stdin and stdout. The multiplexed side is carried on a
//! *reserved channel identity* — the empty string, which the graph forbids as a
//! real channel identity (§3), so it never collides. Hostward, the daemon frames
//! the raw device bytes as `data("", …)` into stdin; the child parses the device's
//! proprietary framing and emits `data(<channel>, …)` on stdout, which the daemon
//! fans out. Targetward, the daemon frames a channel write as `data(<channel>, …)`
//! into stdin; the child re-frames it and emits `data("", …)` on stdout, which the
//! daemon writes to the device. stderr passes through to daemon diagnostics.
//!
//! **Lifecycle (§7.6).** A crashed child faults the node and restarts with
//! backoff; the restart count is observable state (item 4). The child runs as the
//! daemon's user.
//!
//! **Not a pure §5 interior node (ADR §15.22).** Unlike an in-process codec, the
//! exec codec is a *child-pipe boundary*: it holds a bounded merge queue feeding
//! the child's stdin plus the child's pipes. Its stdin-feeding and stdout-reading
//! pumps run as **concurrently-polled** futures (`pump_child`), so the daemon
//! never deadlocks against itself and a parked targetward emit never starves the
//! hostward feed. The single child pipe still couples the two directions at the
//! child under a *sustained* targetward stall (the child's stdout backs up, so it
//! stops reading stdin) — a documented property, stronger than §9's head-of-line
//! note (which preserves hostward), bounded by the merge queue depth.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use serial_nexus_codec_api::{Event, EventKind, FrameDecoder};
use serial_nexus_core::Chunk;
use serial_nexus_core::config::{MAX_TIMER_MS, NodeConfig};
use serial_nexus_core::graph::{EndpointAddr, Facing};
use serial_nexus_core::state::{NodeState, NodeStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::boundary::{self, TaskSet};
use crate::cell::CriticalCell;
use crate::nodes::codec::UnconfiguredChannels;
use crate::runtime::{
    CHANNEL_CAP, DataFrame, DropCounters, HostwardChannelStat, LossCounter, READ_BUF, SharedFanOut,
    SharedTargetEdge, TeardownLoss, Wiring, data_frames, forward_targetward, route_channel_data,
};
use crate::tap::TapFeed;

/// The reserved wire channel identity for the multiplexed (device) side (§15.22).
/// The graph forbids an empty real channel identity, so this never collides.
const MUX_CHANNEL: &str = "";

/// The exec codec's validated attribute schema (§7.6). Deserialized from the
/// opaque config table; a schema failure is structural and fails the load (§11).
///
/// `deny_unknown_fields` for the reason every table in `core/src/config.rs` carries
/// it — §11's third review-hardened rule (§15.34): *unknown configuration keys are
/// refused naming the key, so a typo cannot silently become a default*. A codec's
/// opaque attribute table is the same door in a different wall, and it was the one
/// left open: `restart_backoffms` or `enviroment` loaded clean and quietly restored
/// the built-in default, which is the single shape of configuration error that shows
/// up in neither `dump` (it round-trips the value the operator never set) nor
/// `state`. Serde names the offending key and lists the legal ones, so the operator
/// meets the same sentence the node schema gives them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecAttributes {
    /// The child command and its arguments (required, non-empty).
    argv: Vec<String>,
    /// Extra environment for the child.
    #[serde(default)]
    env: HashMap<String, String>,
    /// Backoff before restarting a crashed child. Capped at
    /// [`serial_nexus_core::config::MAX_TIMER_MS`] by [`parse_attributes`] (invariant 13).
    #[serde(default = "default_backoff_ms")]
    restart_backoff_ms: u64,
}

fn default_backoff_ms() -> u64 {
    200
}

/// Parse and validate the exec attribute table (§8/§11: structural on failure).
///
/// **Every numeric knob here is range-checked** (AGENTS invariant 13). Codec
/// attributes are opaque to `GraphConfig::validate`, so this is where the exec
/// codec's own numerics get the treatment the schema's other timers get in
/// `config.rs` — and against the *same* [`MAX_TIMER_MS`] constant, so the two timer
/// families cannot drift apart. This function is called from both `load`
/// (`daemon.rs`'s `precheck_codecs`) and `add-node`, in both cases **before**
/// anything is created and before a `--replace` teardown, so the §11 atomicity
/// guarantee comes for free: an out-of-range value refuses the whole operation with
/// the running graph untouched and nothing spawned.
pub fn parse_attributes(attributes: &toml::Table) -> Result<(), String> {
    let attrs = ExecAttributes::deserialize(attributes.clone())
        .map_err(|e| format!("exec codec attributes: {e}"))?;
    if attrs.argv.is_empty() {
        return Err("exec codec attributes: argv must be non-empty".to_owned());
    }
    // CORE-3: this was the one millisecond timer in the schema with no cap. An
    // operator slipping three digits (`6000000` for six seconds, or `86400000` for
    // "a day" — the exact value the leg's `reconnect_initial_ms` cap was written
    // against) loaded clean and then, on the one event the backoff exists for, never
    // respawned the crashed child again for the life of the daemon, with the node
    // reporting `faulted … retrying` throughout. The wording deliberately mirrors
    // `ValidationError::NumericOutOfRange` so an operator meets one sentence about
    // out-of-range numbers, not two.
    if attrs.restart_backoff_ms > MAX_TIMER_MS {
        return Err(format!(
            "exec codec attributes: restart_backoff_ms = {}, above the maximum {MAX_TIMER_MS} \
             (a numeric field is range-checked before anything is created, §11)",
            attrs.restart_backoff_ms
        ));
    }
    Ok(())
}

#[derive(Default)]
struct ChannelStat {
    delivered_hostward: Cell<u64>,
    discarded_unattached: Cell<u64>,
    active: Cell<bool>,
}

/// The §5 hostward accounting rule, shared with the in-process codec and the leg
/// through the one [`route_channel_data`] implementation (SIMP-1). The two codecs'
/// `ChannelStat`s stay separate structs — the counter *names* are part of `state`'s
/// contract — but their arithmetic is now literally the same code, so an amendment
/// to it cannot land on one kind and miss the other while byte-identical traffic
/// reports different numbers. `add_dropped_full` keeps its no-op default: a slow
/// consumer's loss is charged to that consumer's own [`DropCounters`] at the
/// boundary that dropped it.
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

pub struct ExecCodecNode {
    pub name: String,
    faces: Facing,
    channels: Vec<String>,
    attrs: ExecAttributes,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    /// The multiplexed side's own hostward drops (the codec falling behind the
    /// serial), surfaced so the §5 loss stays located. Claimed at start.
    mux_counters: Option<Arc<DropCounters>>,
    /// The multiplexed side's live edge binding (§15.35): whether an upstream is
    /// attached and, when it is writable, the targetward sender and lock the child's
    /// remux output rides. `connect`/`disconnect` mutate it under the running pumps.
    mux_edge: Option<SharedTargetEdge>,
    /// Restart attempts since the first start — both a failed spawn and a post-crash
    /// respawn bump it, so a child that can never spawn (bad argv / ENOENT) is visibly
    /// retrying rather than reporting a frozen zero (observable state, §7.6).
    restart_count: Rc<Cell<u64>>,
    /// Bytes the child emitted device-bound on the reserved mux channel that had no
    /// targetward serial path (a read-only / hostward-only edge), or whose write to
    /// the device failed — a §5 loss kept located and attributable rather than
    /// silently lost.
    mux_discarded_targetward: Rc<Cell<u64>>,
    /// Bytes that could not be framed into the child's stdin envelope at all, so the
    /// tail of a chunk never reached the child (`data_frames`' residual, RV-9).
    /// Unreachable for any sane channel identity — each fragment provably fits the
    /// frame bound — and counted rather than truncated in silence (§5 all-loss-counted,
    /// invariant 3 "fragment, never skip-on-error, count any residual").
    unframable_discarded: Rc<Cell<u64>>,
    /// Channel identities the child emitted on that this node has no channel for,
    /// bounded and named (CODEC-1). Shares the codec node's one implementation, so
    /// the two kinds report the same loss under the same names.
    unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
    /// Shared with the supervisor task, which flips it to faulted on a crash and
    /// back to active once a child is running. Carries the transition timestamp
    /// (§7), so a restart loop's `faulted` stamp moves with each real restart.
    status: Rc<CriticalCell<NodeState>>,
    /// Whether a child process is currently up and pumping — the supervisor's half
    /// of the status story, published so [`Self::set_upstream_attached`] can tell
    /// "no upstream" from "no child" (CODEC-1). Between a crash and the respawn the
    /// supervisor's `Faulted` stamp is the only truthful status, and edge surgery
    /// must leave it alone.
    child_live: Rc<Cell<bool>>,
    /// Targetward bytes destroyed because this node was torn down while they were
    /// still queued for it (§5, notes §3.31): what its pumps never got to look at,
    /// as distinct from what they looked at and decided to discard.
    teardown_loss: TeardownLoss,
    tasks: TaskSet,
}

impl ExecCodecNode {
    pub fn create(config: &NodeConfig) -> ExecCodecNode {
        let NodeConfig::Codec {
            name,
            faces,
            channels,
            attributes,
            ..
        } = config
        else {
            unreachable!("ExecCodecNode::create called with non-Codec config");
        };
        // Attributes were validated at instantiate (parse_attributes); deserialize
        // again here into the owned schema. Infallible after validation.
        let attrs = ExecAttributes::deserialize(attributes.clone())
            .expect("exec attributes validated at instantiate");
        let stats = channels
            .iter()
            .map(|c| (c.clone(), Rc::new(ChannelStat::default())))
            .collect();
        ExecCodecNode {
            teardown_loss: TeardownLoss::default(),
            name: name.clone(),
            faces: *faces,
            channels: channels.clone(),
            attrs,
            stats: Rc::new(stats),
            mux_counters: None,
            mux_edge: None,
            restart_count: Rc::new(Cell::new(0)),
            mux_discarded_targetward: Rc::new(Cell::new(0)),
            unframable_discarded: Rc::new(Cell::new(0)),
            unconfigured: Rc::new(CriticalCell::new(UnconfiguredChannels::default())),
            status: Rc::new(CriticalCell::new(NodeState::new(NodeStatus::Active))),
            child_live: Rc::new(Cell::new(false)),
            tasks: TaskSet::default(),
        }
    }

    /// Claim every host-facing endpoint's targetward receiver and park it in a
    /// draining task, for the `start` paths that return before the pump is built.
    ///
    /// A node that comes up `waiting` still owns its endpoints, and their senders are
    /// still live in `GraphState::endpoint_targetward` and in every attached writer
    /// origin. Leaving a receiver in the wiring plan drops it when `load` finishes,
    /// closing the channel under those senders — MAP-1's chain, which for a pty origin
    /// ends `read_and_poll` and takes presence latching, `handle_last_close`, termios
    /// reconciliation and detach-release with it, wedging the endpoint's lock on a
    /// holder that has gone away. A waiting node must be inert, not destructive
    /// (§15.8), and the drained bytes are counted rather than lost (§5).
    fn drain_unwired_channels(&mut self, wiring: &mut Wiring) {
        // Whichever side faces host carries the arbitrated targetward channel: the
        // channels for a demultiplexer, the multiplexed endpoint for a re-multiplexer.
        // Sweep both so neither `start` exit can leak a receiver.
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
            let rx = self.teardown_loss.watch(rx);
            let discarded = self.mux_discarded_targetward.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(bytes) = rx.recv().await {
                    discarded.add(bytes.len() as u64);
                }
            }));
        }
    }

    pub fn start(&mut self, wiring: &mut Wiring) {
        if self.faces != Facing::Target {
            // Standalone re-multiplexer: deferred work (§14), not a malfunction —
            // §7.5/§14 promise it "loads and waits" (mirrors the in-process codec).
            self.status.with_mut(|s| {
                s.set(NodeStatus::Waiting {
                    reason: "standalone exec re-multiplexer orientation (faces=host) has no \
                             driver; deferred work (§14)"
                        .to_owned(),
                });
            });
            self.drain_unwired_channels(wiring);
            return;
        }
        // The multiplexed side's machinery exists whether or not an upstream is
        // attached today (§15.35): every task starts, parked on the endpoint's inbox
        // and its origin slot, so a later `connect` needs no restart and no channel
        // receiver is ever dropped under its still-live senders (MAP-1, §15.8).
        let mux = EndpointAddr::node(&self.name);
        let mux_inbox = wiring.target_inbox.remove(&mux);
        let mux_edge = wiring
            .target_edges
            .remove(&mux)
            .unwrap_or_else(crate::runtime::TargetEdge::new);
        self.mux_edge = Some(mux_edge.clone());
        self.mux_counters = wiring.target_counters.remove(&mux);
        if !mux_edge.with(|e| e.attached) {
            self.status.with_mut(|s| {
                s.set(NodeStatus::Waiting {
                    reason: "multiplexed side has no attached upstream".to_owned(),
                });
            });
        }

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

        // Merge everything the child reads on stdin into one tagged source: the raw
        // hostward device stream (tagged with the reserved multiplexed channel) and
        // each channel's targetward writes (tagged with the channel identity). The
        // forwarders outlive child restarts, so the merged source survives them.
        let (src_tx, src_rx) = mpsc::channel::<(String, Chunk)>(CHANNEL_CAP);
        if let Some(mut inbox) = mux_inbox {
            let src_tx = src_tx.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                // One forwarder across every upstream edge this node is given
                // (§15.35): park on the inbox, drain the edge until `disconnect`
                // closes it, park again.
                while let Some(mut rx) = inbox.recv().await {
                    while let Some(chunk) = rx.recv().await {
                        if src_tx.send((MUX_CHANNEL.to_owned(), chunk)).await.is_err() {
                            return;
                        }
                    }
                }
            }));
        }
        for (ch, rx) in channel_rxs {
            let rx = self.teardown_loss.watch(rx);
            let src_tx = src_tx.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(chunk) = rx.recv().await {
                    if src_tx.send((ch.clone(), chunk)).await.is_err() {
                        break;
                    }
                }
            }));
        }
        drop(src_tx);

        // The supervisor owns the merged source and the routing outputs, and manages
        // the child's lifecycle (spawn, pump, restart-with-backoff, §7.6).
        self.tasks
            .push(tokio::task::spawn_local(supervise(SuperviseArgs {
                argv: self.attrs.argv.clone(),
                env: self.attrs.env.clone().into_iter().collect(),
                backoff_ms: self.attrs.restart_backoff_ms,
                src_rx,
                mux_edge,
                channel_sinks,
                channel_feeds,
                stats: self.stats.clone(),
                restart_count: self.restart_count.clone(),
                mux_discarded_targetward: self.mux_discarded_targetward.clone(),
                unframable_discarded: self.unframable_discarded.clone(),
                unconfigured: self.unconfigured.clone(),
                status: self.status.clone(),
                child_live: self.child_live.clone(),
            })));
    }

    /// Re-report status after edge surgery on `endpoint` (§15.35). Only the
    /// multiplexed side decides `active`/`waiting`; a channel endpoint faces host.
    pub fn set_upstream_attached(&mut self, endpoint: &EndpointAddr, attached: bool) {
        if !endpoint.is_default() || endpoint.node != self.name {
            return;
        }
        // CODEC-1: surgery decides only whether a *running* child has an upstream to
        // carry. While no child is up, the supervisor's `Faulted{child exited;
        // restarting}` stamp is the only truthful status, and it stands for the whole
        // `restart_backoff_ms` wait — legal up to [`MAX_TIMER_MS`], an hour. Nothing
        // would correct an overwrite until the respawn, so a `connect` landing in that
        // window reported a dead exec codec as `active` for as long as the operator had
        // configured it to back off. Skipping costs nothing: the respawn re-decides
        // active-vs-waiting off this same live edge slot, which is why the supervisor
        // consults the slot rather than latching a status at start.
        if !self.child_live.get() {
            return;
        }
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
        // `self.stats` is built from `self.channels` in `create`, so every channel has
        // exactly one stat — iterate it directly, no `Option` dance.
        let channels: serde_json::Map<String, Value> = self
            .stats
            .iter()
            .map(|(ch, stat)| {
                let obj = json!({
                    "status": if stat.active.get() { "active" } else { "waiting" },
                    "delivered_hostward": stat.delivered_hostward.get(),
                    "discarded_unattached": stat.discarded_unattached.get(),
                });
                (ch.clone(), obj)
            })
            .collect();
        let mut obj = json!({
            "codec": "exec",
            "faces": self.faces.to_string(),
            "restart_count": self.restart_count.get(),
            // Bytes that never reached the child because the envelope refused to
            // frame them (§5 all-loss-counted; unreachable for a sane channel id).
            "discarded_unframable": self.unframable_discarded.get(),
            // A **floor, not a total**, and the one kind whose figure is (§5, §15.50,
            // notes §3.31). What is watched is the host-facing per-channel queues the
            // forwarders read from; those forwarders then push into `src_tx`, a second
            // *internal* merged queue `pump_child` reads, and a chunk that has already
            // moved into that stage is beyond this handle's reach. So a torn-down exec
            // can destroy more than it reports — never less, which is the direction §5
            // requires when a figure has to be inexact.
            //
            // Closing it no longer needs a new mechanism: since `serial` and `leg`
            // adopted the ledger the inbox is generic over its item type (notes §3.55),
            // so the merge queue needs only a `TeardownBytes` impl for its
            // `(String, Chunk)` and the same `watch`/`drain` pair. What it needs is a
            // guard, and the guard is the hard half — "a chunk is sitting in the merge
            // queue" is not something an RPC ack can make true, so it wants a child
            // that has stopped reading its stdin.
            "discarded_at_teardown": self.teardown_loss.bytes(),
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
                "discarded_targetward": self.mux_discarded_targetward.get(),
            },
            "channels": channels,
        });
        // CODEC-1's three fields, written by the one shared reporter the in-process
        // codec uses, so the two kinds cannot name the same loss differently.
        if let Value::Object(map) = &mut obj {
            self.unconfigured.with(|u| u.report_into(map));
        }
        obj
    }

    /// Ask this node's tasks to stop, without waiting (§16.1, BND-1). The child
    /// process is `kill_on_drop`, so aborting the supervisor is the whole signal;
    /// the method exists so the daemon can signal every node uniformly.
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
    ///
    /// **A floor, not a total — the one kind whose figure is** (§5, §15.50). The ledger
    /// watches the host-facing per-channel queues; those forwarders push into `src_tx`,
    /// a second *internal* merged queue `pump_child` reads, and a chunk already moved
    /// into that stage is beyond this handle's reach. So a torn-down exec can destroy
    /// more than it reports — never less, which is the direction §5 requires when a
    /// figure has to be inexact. `state_extra` carries the same caveat at the wire
    /// field, and `Node::discarded_at_teardown` carries it at the dispatch; it is
    /// restated here because this is the method both of those call, and a reader who
    /// arrives at it directly would otherwise take an exact number away.
    pub fn discarded_at_teardown(&self) -> u64 {
        self.teardown_loss.bytes()
    }
}

struct SuperviseArgs {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    backoff_ms: u64,
    src_rx: mpsc::Receiver<(String, Chunk)>,
    mux_edge: SharedTargetEdge,
    channel_sinks: HashMap<String, SharedFanOut>,
    channel_feeds: HashMap<String, TapFeed>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    restart_count: Rc<Cell<u64>>,
    mux_discarded_targetward: Rc<Cell<u64>>,
    unframable_discarded: Rc<Cell<u64>>,
    unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
    status: Rc<CriticalCell<NodeState>>,
    child_live: Rc<Cell<bool>>,
}

/// Supervise the child: (re)spawn it, pump envelope frames both ways until it
/// dies, then fault, back off, and restart (§7.6). The merged source and routing
/// outputs persist across restarts, so a restarted child resumes cleanly.
async fn supervise(mut a: SuperviseArgs) {
    // Fixed restart backoff (§7.6): a constant wait between a crash and the respawn.
    let mut backoff = boundary::Backoff::fixed(a.backoff_ms);
    loop {
        let mut cmd = tokio::process::Command::new(&a.argv[0]);
        cmd.args(&a.argv[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in &a.env {
            cmd.env(k, v);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                a.child_live.set(false);
                a.restart_count.set(a.restart_count.get() + 1);
                a.status.with_mut(|s| {
                    s.set(NodeStatus::Faulted {
                        reason: format!(
                            "spawn {:?}: {e}; retrying (count {})",
                            a.argv[0],
                            a.restart_count.get()
                        ),
                    });
                });
                backoff.sleep().await;
                continue;
            }
        };
        // The child is up, but a node with no upstream has nothing to carry: report
        // `waiting` until an edge attaches (§7.5/§15.8, §15.35). Consulting the live
        // slot here rather than latching a status at start keeps the supervisor and
        // `connect` from racing to describe the same node — and publishing liveness
        // first is the other half of that: from here until the child dies, edge
        // surgery is free to re-decide active-vs-waiting (CODEC-1).
        a.child_live.set(true);
        a.status.with_mut(|s| {
            s.set(if a.mux_edge.with(|e| e.attached) {
                NodeStatus::Active
            } else {
                NodeStatus::Waiting {
                    reason: "multiplexed side has no attached upstream".to_owned(),
                }
            })
        });

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        // Pump both directions concurrently until the child dies or the source
        // closes. The outcome distinguishes the two so teardown does not respawn.
        let routing = Routing {
            mux_edge: &a.mux_edge,
            channel_sinks: &a.channel_sinks,
            channel_feeds: &a.channel_feeds,
            stats: &a.stats,
            mux_discarded_targetward: &a.mux_discarded_targetward,
            unframable_discarded: &a.unframable_discarded,
            unconfigured: &a.unconfigured,
        };
        let end = pump_child(stdin, stdout, stderr, &mut a.src_rx, &routing).await;

        let _ = child.kill().await;
        // No child again until the next successful spawn, so whatever status this
        // iteration stamps below is the supervisor's alone to keep (CODEC-1).
        a.child_live.set(false);
        match end {
            // The merged source closed: the node was torn down (its forwarders
            // dropped their senders) or its upstream is gone. Stop; do not respawn.
            PumpEnd::SourceClosed => return,
            PumpEnd::ChildDied => {
                a.restart_count.set(a.restart_count.get() + 1);
                a.status.with_mut(|s| {
                    s.set(NodeStatus::Faulted {
                        reason: format!(
                            "child exited; restarting (count {})",
                            a.restart_count.get()
                        ),
                    });
                });
                backoff.sleep().await;
            }
        }
    }
}

/// Why a child's pump ended: the child died (respawn it) or the merged source
/// closed (the node was torn down / its upstream is gone — stop).
enum PumpEnd {
    ChildDied,
    SourceClosed,
}

/// The routing outputs a child's stdout is decoded into: the multiplexed-side
/// targetward path to the device and its lock, and each channel's hostward
/// fan-out. Borrowed for a child's lifetime.
struct Routing<'a> {
    mux_edge: &'a SharedTargetEdge,
    channel_sinks: &'a HashMap<String, SharedFanOut>,
    channel_feeds: &'a HashMap<String, TapFeed>,
    stats: &'a Rc<HashMap<String, Rc<ChannelStat>>>,
    mux_discarded_targetward: &'a Rc<Cell<u64>>,
    unframable_discarded: &'a Rc<Cell<u64>>,
    /// Identities the child emitted on that the node has no channel for (CODEC-1).
    unconfigured: &'a Rc<CriticalCell<UnconfiguredChannels>>,
}

/// Pump one child instance. The stdin-feeding and stdout-reading loops run as
/// **concurrently-polled** futures via [`boundary::race3`] — not two branches of a
/// single loop — so a `write_all(stdin)` blocked on a full pipe never starves the
/// stdout reader (which keeps draining stdout, unblocking the child, which drains
/// stdin), and a targetward `route_event` parked on backpressure or a stolen lock
/// never starves the hostward stdin feed. This is what keeps the two directions
/// independent across the single child pipe pair (the deadlock and the
/// hostward-starvation the coupled version would suffer). stderr is drained as a
/// third future so it is dropped with the pump rather than leaking a task.
///
/// Returns [`PumpEnd::ChildDied`] on a broken stdin, a stdout EOF/error, or a
/// malformed frame; [`PumpEnd::SourceClosed`] when the merged source ends.
async fn pump_child(
    mut stdin: tokio::process::ChildStdin,
    mut stdout: tokio::process::ChildStdout,
    mut stderr: tokio::process::ChildStderr,
    src_rx: &mut mpsc::Receiver<(String, Chunk)>,
    routing: &Routing<'_>,
) -> PumpEnd {
    // stdin: frame each tagged chunk and write it to the child. A chunk larger than
    // one frame — the raw device stream on the reserved mux channel is a full serial
    // read, up to READ_BUF == MAX_FRAME_SIZE, so it overflows the envelope header —
    // is fragmented into consecutive data frames rather than dropped (the child
    // reassembles per channel), preserving §5's no-drop / all-loss-counted invariant
    // just as the leg does (§15.24).
    let feed = async {
        while let Some((channel, bytes)) = src_rx.recv().await {
            for item in data_frames(channel.as_str(), &bytes) {
                match item {
                    DataFrame::Piece(_len, frame) => {
                        if stdin.write_all(&frame).await.is_err() || stdin.flush().await.is_err() {
                            return PumpEnd::ChildDied; // child stdin broke
                        }
                    }
                    // The envelope refused a piece, so this many source bytes never
                    // reached the child. Unreachable for a sane channel identity, but
                    // §5 counts every lost byte rather than truncating in silence
                    // (invariant 3's third clause, RV-9).
                    DataFrame::Residual(n) => {
                        routing.unframable_discarded.add(n as u64);
                    }
                }
            }
        }
        PumpEnd::SourceClosed
    };
    // stdout: decode envelope frames and route them.
    let read = async {
        let mut decoder = FrameDecoder::new();
        let mut readbuf = vec![0u8; READ_BUF];
        loop {
            match stdout.read(&mut readbuf).await {
                Ok(0) | Err(_) => return PumpEnd::ChildDied, // EOF/error: child died
                Ok(k) => {
                    decoder.push(&readbuf[..k]);
                    loop {
                        match decoder.next_event() {
                            Ok(Some(ev)) => route_event(ev, routing).await,
                            Ok(None) => break,
                            Err(e) => {
                                tracing::warn!(target: "exec-codec", "child emitted a malformed frame: {e}");
                                return PumpEnd::ChildDied; // protocol violation ≈ crash
                            }
                        }
                    }
                }
            }
        }
    };
    // stderr → diagnostics, drained in bounded fixed-size reads (never `.lines()`:
    // a child that emits no newline — a `\r` progress spinner, binary/hex output,
    // `cat /dev/urandom 1>&2` — would grow one String without bound and drive the
    // long-lived daemon toward OOM, and the escape hatch runs arbitrary third-party
    // tools, §7.6/§13). Capped at READ_BUF like stdout; each chunk is logged lossily.
    // Then the stream drains to EOF and parks so it never ends the pump on its own —
    // only stdin/stdout death, or a stream that has closed, does that (§16.1
    // park-don't-teardown).
    let errs = async {
        let mut buf = vec![0u8; READ_BUF];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break, // EOF/error: stop draining, then park
                Ok(k) => tracing::warn!(
                    target: "exec-codec",
                    "child stderr: {}",
                    String::from_utf8_lossy(&buf[..k]).trim_end()
                ),
            }
        }
        boundary::park().await
    };
    // Concurrently-polled halves (§15.22): a blocked `write_all(stdin)` never starves
    // the stdout reader, and a targetward `route_event` parked on backpressure or a
    // stolen lock never starves the hostward stdin feed.
    boundary::race3(feed, read, errs).await
}

/// Route one event the child emitted: a frame on the reserved multiplexed channel
/// goes targetward to the device (gated on holding the serial's lock); a frame on
/// a real channel is fanned out hostward to that channel's consumers.
async fn route_event(ev: Event, routing: &Routing<'_>) {
    let Routing {
        mux_edge,
        channel_sinks,
        channel_feeds,
        stats,
        mux_discarded_targetward,
        unconfigured,
        ..
    } = routing;
    match ev.kind {
        EventKind::Data(bytes) => {
            if ev.channel.as_str() == MUX_CHANNEL {
                // Capture the length *before* the send moves the chunk, so every exit
                // from this branch can attribute the loss (CODEXEC-2: the
                // `reacquire_held`-failed path used to return early, skipping the
                // counter its sibling maintained).
                let n = bytes.len() as u64;
                let lost: &Cell<u64> = mux_discarded_targetward;
                // Targetward remux output → the device, backpressured (§5). Gated on
                // the exec codec holding the serial lock (§6).
                // Re-read the live edge per event (§15.35), and — unlike the codec
                // and the map — **never park** on an unattached one. This route runs
                // inside the child's single stdout decode loop, so a parked mux event
                // stalls every *hostward* channel event queued behind it: one
                // detached device edge would stop delivery to local consumers that
                // have nothing to do with it. A shared pump counts; only a
                // per-endpoint pump can afford to backpressure. That policy is why
                // only the send-and-charge *tail* below is shared with the codec and
                // the map, never the whole pump (SIMP-2, invariant 14).
                match mux_edge.origin() {
                    // Both of the tail's exits — the serial endpoint went away under
                    // us (its node was removed at runtime, or the graph was replaced)
                    // and the targetward channel closed between the grant and the send
                    // — are the same loss on the same counter, charged inside the
                    // helper. CODEXEC-2 was the first of those two returning early
                    // without charging anything.
                    Some(origin) => {
                        forward_targetward(&origin, n, || Some(bytes))
                            .await
                            .charge(lost);
                    }
                    // No targetward serial path (a read-only / hostward-only mux edge):
                    // the child's device-bound bytes have nowhere to go. Count the loss
                    // so it stays located and attributable, never silently dropped (§5).
                    None => lost.add(n),
                }
            } else {
                let n = bytes.len() as u64;
                // CODEC-1: the child emitted on an identity this node has no channel
                // for. §8 still governs the bytes — an announcement never grows the
                // graph, and `docs/codec-authors.md` tells authors so — but §5
                // governs the accounting, so they are counted and the identity is
                // named. Without the name nothing in the daemon could distinguish a
                // mis-spelled configured channel from a device multiplexing a stream
                // the operator never enumerated; both look like a `waiting` channel.
                let Some(s) = stats.get(ev.channel.as_str()) else {
                    unconfigured.with_mut(|u| u.record(ev.channel.as_str(), n));
                    return;
                };
                // The one shared per-channel hostward routing block (SIMP-1), the same
                // call the in-process codec makes: latch active, mirror to this
                // channel's tap hub for taps and the replay ring (§17) *outside* the
                // fan-out's accounting so a spy never masks a real consumer's absence
                // (§5, invariant 9), then fan out to the graph sinks. A slow consumer's
                // full-buffer drop is charged to that consumer; an all-`Closed`/empty
                // sink set, or a channel with no fan-out at all, to this channel's
                // unattached counter.
                route_channel_data(
                    &bytes,
                    channel_feeds.get(ev.channel.as_str()),
                    channel_sinks.get(ev.channel.as_str()),
                    Some(&**s),
                );
            }
        }
        // CODEC-4: the reserved identity is the *multiplexed side*, not a channel
        // (§15.22) — the data arm above has always known that, and these three did
        // not. A child announcing or closing the raw device stream says nothing about
        // the graph's channels, and it has no per-channel stat by construction (the
        // graph forbids an empty channel identity), so `stats.get("")` missed and the
        // reserved name was filed as an *unconfigured channel*: `unconfigured_channels:
        // [""]` in `state` plus the mis-spelled-channel WARN fired on an empty name,
        // diagnosing a well-formed child as an operator typo. The arms are narrowed
        // rather than removed — a real unconfigured identity still lands below.
        EventKind::Open | EventKind::Close if ev.channel.as_str() == MUX_CHANNEL => {}
        EventKind::Error(msg) if ev.channel.as_str() == MUX_CHANNEL => {
            tracing::debug!(target: "exec-codec", "child multiplexed-side error: {msg}");
        }
        EventKind::Open => match stats.get(ev.channel.as_str()) {
            Some(s) => s.active.set(true),
            // An announcement on an identity the operator never enumerated: no bytes
            // to charge, but the name is the diagnosis (CODEC-1).
            None => unconfigured.with_mut(|u| u.record(ev.channel.as_str(), 0)),
        },
        EventKind::Close => {
            if let Some(s) = stats.get(ev.channel.as_str()) {
                s.active.set(false);
            }
        }
        EventKind::Error(msg) => {
            tracing::debug!(target: "exec-codec", channel = %ev.channel, "child channel error: {msg}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The routing fixture the tests below share, plus the two counters they assert
    /// on. Held together so a new `Routing` field breaks one place, not five.
    struct Fixture {
        mux_edge: SharedTargetEdge,
        sinks: HashMap<String, SharedFanOut>,
        feeds: HashMap<String, TapFeed>,
        stats: Rc<HashMap<String, Rc<ChannelStat>>>,
        mux_discarded: Rc<Cell<u64>>,
        unframable: Rc<Cell<u64>>,
        unconfigured: Rc<CriticalCell<UnconfiguredChannels>>,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                // Attached, read-only: the mux side exists but cannot write, which
                // is the case these tests are about (an unattached one parks).
                mux_edge: crate::runtime::TargetEdge::read_only(),
                sinks: HashMap::new(),
                feeds: HashMap::new(),
                stats: Rc::new(HashMap::new()),
                mux_discarded: Rc::new(Cell::new(0)),
                unframable: Rc::new(Cell::new(0)),
                unconfigured: Rc::new(CriticalCell::new(UnconfiguredChannels::default())),
            }
        }

        fn routing(&self) -> Routing<'_> {
            Routing {
                mux_edge: &self.mux_edge,
                channel_sinks: &self.sinks,
                channel_feeds: &self.feeds,
                stats: &self.stats,
                mux_discarded_targetward: &self.mux_discarded,
                unframable_discarded: &self.unframable,
                unconfigured: &self.unconfigured,
            }
        }
    }

    #[tokio::test]
    async fn mux_targetward_drop_is_counted_when_no_serial_path() {
        // A child emits device-bound data on the reserved mux channel, but the
        // multiplexed side has no targetward serial path (a read-only / hostward-only
        // edge). The bytes have nowhere to go and must be counted, not silently
        // dropped (§5 all-loss-counted; CODEXEC-3 regression guard).
        let f = Fixture::new();
        let payload = Chunk::from_static(b"device-bound bytes");
        let n = payload.len() as u64;
        route_event(Event::data(MUX_CHANNEL, payload), &f.routing()).await;
        assert_eq!(f.mux_discarded.get(), n);
    }

    #[tokio::test]
    async fn mux_targetward_drop_is_counted_when_the_endpoint_was_torn_down() {
        // CODEXEC-2: with a targetward path present but its endpoint torn down (the
        // serial node removed at runtime), `reacquire_held` fails and the
        // device-bound bytes are lost — the branch that used to return early without
        // touching the counter its sibling maintained (§5 all-loss-counted).
        use serial_nexus_core::lock::{Arbitration, EndpointLock, OriginId, WriteMode};
        use tokio::sync::broadcast;

        let f = Fixture::new();
        let (tx, _rx) = mpsc::channel::<Chunk>(4);
        let id = OriginId(1);
        let mut lock = EndpointLock::new(Arbitration::Exclusive);
        // Registered on-demand (never granted), so `reacquire_held` cannot take it;
        // then the endpoint is closed, which is what makes the reclaim give up.
        lock.register(id, "mux", WriteMode::OnDemand);
        let (notifier, _nrx) = broadcast::channel(16);
        let cell: crate::runtime::SharedLock =
            Rc::new(crate::runtime::LockCell::new("serial", lock, notifier));
        cell.close();
        f.mux_edge.with_mut(|e| {
            e.registered = Some((cell, id));
            e.writer = Some(tx);
        });

        let payload = Chunk::from_static(b"device-bound bytes");
        let n = payload.len() as u64;
        route_event(Event::data(MUX_CHANNEL, payload), &f.routing()).await;
        assert_eq!(
            f.mux_discarded.get(),
            n,
            "a torn-down endpoint is still loss"
        );
    }

    #[tokio::test]
    async fn hostward_all_sinks_closed_counts_unattached_loss() {
        // F1/DM-3: a channel whose only consumer was cascade-removed leaves a
        // permanently `Closed` sink. The bytes reach nobody, so they must land on
        // `discarded_unattached` and NOT on `delivered_hostward` (§5).
        let mut f = Fixture::new();
        let stat = Rc::new(ChannelStat::default());
        f.stats = Rc::new(HashMap::from([("console".to_owned(), stat.clone())]));
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        drop(rx); // consumer gone: the sink is permanently Closed
        let counters = Arc::new(DropCounters::default());
        {
            let fanout = crate::runtime::FanOutList::new();
            fanout.attach(crate::runtime::AttachedSink {
                target: "consumer".parse().expect("address parses"),
                tx,
                counters: counters.clone(),
            });
            f.sinks.insert("console".to_owned(), fanout);
        }

        let payload = Chunk::from_static(b"hostward bytes");
        let n = payload.len() as u64;
        route_event(Event::data("console", payload), &f.routing()).await;
        assert_eq!(stat.discarded_unattached.get(), n);
        assert_eq!(stat.delivered_hostward.get(), 0);
        assert_eq!(counters.dropped_full(), 0, "a dead sink is not 'slow'");
    }

    #[tokio::test]
    async fn hostward_live_sink_counts_delivered_not_unattached() {
        let mut f = Fixture::new();
        let stat = Rc::new(ChannelStat::default());
        f.stats = Rc::new(HashMap::from([("console".to_owned(), stat.clone())]));
        let (tx, mut rx) = mpsc::channel::<Chunk>(4);
        let fanout = crate::runtime::FanOutList::new();
        fanout.attach(crate::runtime::AttachedSink {
            target: "consumer".parse().expect("address parses"),
            tx,
            counters: Arc::new(DropCounters::default()),
        });
        f.sinks.insert("console".to_owned(), fanout);

        let payload = Chunk::from_static(b"hostward bytes");
        let n = payload.len() as u64;
        route_event(Event::data("console", payload), &f.routing()).await;
        assert_eq!(stat.delivered_hostward.get(), n);
        assert_eq!(stat.discarded_unattached.get(), 0);
        let got = rx.try_recv().expect("the live sink received the chunk");
        assert_eq!(got.len() as u64, n);
    }

    /// LEGD-2 (exec half): `fan_out` reports `live = true` for a sink whose bounded
    /// buffer was **full** — deliberately, it is still a live consumer — so
    /// crediting the whole chunk on `live` counted bytes as delivered that no
    /// consumer ever received. Only what a sink actually took is credited, which
    /// `fan_out` now reports as [`crate::runtime::FanOut::delivered`] rather than
    /// leaving the caller to derive it. The *mixed* fan-out (one sink Ok, one Full),
    /// where the first correction under-counted instead, is pinned once for all three
    /// producers in `runtime::tests`, since the arithmetic is one function.
    #[tokio::test]
    async fn delivered_hostward_credits_only_what_a_full_sink_took() {
        let mut f = Fixture::new();
        let stat = Rc::new(ChannelStat::default());
        f.stats = Rc::new(HashMap::from([("console".to_owned(), stat.clone())]));
        // Depth 1, never drained: the first chunk is taken, the second finds it full.
        let (tx, _rx) = mpsc::channel::<Chunk>(1);
        let counters = Arc::new(DropCounters::default());
        let fanout = crate::runtime::FanOutList::new();
        fanout.attach(crate::runtime::AttachedSink {
            target: "consumer".parse().expect("address parses"),
            tx,
            counters: counters.clone(),
        });
        f.sinks.insert("console".to_owned(), fanout);

        route_event(
            Event::data("console", Chunk::from_static(b"hello")),
            &f.routing(),
        )
        .await;
        route_event(
            Event::data("console", Chunk::from_static(b"world")),
            &f.routing(),
        )
        .await;

        assert_eq!(counters.dropped_full(), 5, "the full sink was charged");
        assert_eq!(
            stat.delivered_hostward.get(),
            5,
            "only the chunk a consumer actually took is delivered"
        );
        assert_eq!(
            stat.discarded_unattached.get(),
            0,
            "a full sink is live, so this is not an unattached loss"
        );
    }

    /// CODEC-1 (exec half): a child emitting on an identity the node has no channel
    /// for still has its bytes dropped (§8: an announcement never grows the graph),
    /// but they are counted and the identity is **named** — the diagnosis that used
    /// to exist nowhere, not even as a log line.
    #[tokio::test]
    async fn hostward_data_on_an_unconfigured_channel_is_counted_and_named() {
        // `Fixture` configures no channels at all, so any identity is unconfigured.
        let f = Fixture::new();
        route_event(
            Event::data("gps", Chunk::from_static(b"$GPGGA")),
            &f.routing(),
        )
        .await;
        route_event(Event::open("gps"), &f.routing()).await;
        route_event(
            Event::data("gps", Chunk::from_static(b"$GPRMC")),
            &f.routing(),
        )
        .await;

        f.unconfigured.with(|u| {
            let mut obj = serde_json::Map::new();
            u.report_into(&mut obj);
            assert_eq!(obj["discarded_unconfigured_channel"], json!(12));
            assert_eq!(
                obj["unconfigured_channels"],
                json!(["gps"]),
                "the identity is named once, however often it appears"
            );
            assert_eq!(obj["unconfigured_overflow"], json!(0));
        });
        assert_eq!(
            f.mux_discarded.get(),
            0,
            "an unconfigured hostward identity is not a targetward loss"
        );
    }

    /// CODEC-4: the reserved empty identity is the multiplexed side (§15.22), not a
    /// channel, in the lifecycle arms as much as in the data arm. A child announcing
    /// or closing the raw device stream used to file the reserved name as an
    /// *unconfigured channel* — `unconfigured_channels: [""]` in `state`, plus the
    /// mis-spelled-channel WARN on an empty name — which reads as an operator typo
    /// on a child that is behaving exactly as `docs/codec-authors.md` documents.
    #[tokio::test]
    async fn lifecycle_events_on_the_reserved_identity_are_not_unconfigured_channels() {
        let f = Fixture::new();
        route_event(Event::open(MUX_CHANNEL), &f.routing()).await;
        route_event(Event::close(MUX_CHANNEL), &f.routing()).await;
        route_event(
            Event::error(MUX_CHANNEL, "device framing violation"),
            &f.routing(),
        )
        .await;

        f.unconfigured.with(|u| {
            let mut obj = serde_json::Map::new();
            u.report_into(&mut obj);
            assert_eq!(
                obj["unconfigured_channels"],
                json!([]),
                "the reserved multiplexed identity is not a channel the operator forgot"
            );
            assert_eq!(obj["discarded_unconfigured_channel"], json!(0));
        });

        // Narrowed, not removed: a *real* unconfigured identity still lands.
        route_event(Event::open("gps"), &f.routing()).await;
        f.unconfigured.with(|u| {
            let mut obj = serde_json::Map::new();
            u.report_into(&mut obj);
            assert_eq!(obj["unconfigured_channels"], json!(["gps"]));
        });
    }

    /// The node fixture for the status tests: an exec codec built straight from a
    /// `NodeConfig`, with no supervisor running, so a test can stand in for the
    /// supervisor and drive `child_live`/`status` the way it does.
    fn exec_node(name: &str) -> ExecCodecNode {
        let attributes: toml::Table = "argv = [\"/bin/true\"]"
            .parse()
            .expect("test attributes parse");
        ExecCodecNode::create(&NodeConfig::Codec {
            name: name.to_owned(),
            codec: "exec".to_owned(),
            faces: Facing::Target,
            channels: vec!["c0".to_owned()],
            arbitration: serial_nexus_core::graph::Arbitration::default(),
            replay_ring: 0,
            attributes,
        })
    }

    /// CODEC-1: mux-edge surgery must not overwrite the supervisor's `Faulted` stamp
    /// while the crashed child sits in restart backoff. The backoff is configuration
    /// (`restart_backoff_ms`, legal up to `MAX_TIMER_MS` — an hour), and between the
    /// kill and the respawn nothing else re-decides the status: a `connect` landing in
    /// that window reported a dead exec codec as `active` for the whole wait, which is
    /// the one direction §15.8's honest-state rule cannot tolerate being wrong in.
    #[test]
    fn mux_edge_surgery_during_restart_backoff_keeps_the_faulted_stamp() {
        let mut node = exec_node("mux");
        let mux = EndpointAddr::node("mux");

        // The supervisor's crash stamp: the child is gone and the backoff is running.
        node.child_live.set(false);
        node.status.with_mut(|s| {
            s.set(NodeStatus::Faulted {
                reason: "child exited; restarting (count 1)".to_owned(),
            })
        });

        node.set_upstream_attached(&mux, true);
        let after_connect = node.status();
        let still_faulted = matches!(
            after_connect.status(),
            NodeStatus::Faulted { reason } if reason.contains("child exited")
        );
        assert!(
            still_faulted,
            "a connect overwrote the supervisor's fault: {:?}",
            after_connect.status()
        );
        node.set_upstream_attached(&mux, false);
        let after_disconnect = node.status();
        assert!(
            matches!(after_disconnect.status(), NodeStatus::Faulted { .. }),
            "a disconnect overwrote the supervisor's fault: {:?}",
            after_disconnect.status()
        );

        // With a child up, surgery is the authority again — the whole point of §15.35.
        node.child_live.set(true);
        node.set_upstream_attached(&mux, true);
        assert!(
            matches!(node.status().status(), NodeStatus::Active),
            "a running child with an upstream is active"
        );
        node.set_upstream_attached(&mux, false);
        assert!(
            matches!(node.status().status(), NodeStatus::Waiting { .. }),
            "a running child with no upstream waits"
        );
    }

    /// CODEC-2: §11's third review-hardened rule — *unknown configuration keys are
    /// refused naming the key, so a typo cannot silently become a default* — reaches
    /// the codec's opaque attribute table too. `restart_backoffms` used to load clean
    /// and quietly restore the 200 ms default: the one configuration error visible in
    /// neither `dump` nor `state`.
    #[test]
    fn unknown_attribute_keys_are_refused_naming_the_key() {
        let attrs = |src: &str| -> toml::Table { src.parse().expect("test attributes parse") };

        for typo in ["restart_backoffms", "enviroment", "argvv"] {
            let src = format!("argv = [\"/bin/true\"]\n{typo} = 1");
            let Err(err) = parse_attributes(&attrs(&src)) else {
                panic!("a misspelled `{typo}` must be refused, not silently defaulted");
            };
            assert!(err.contains(typo), "names the offending key: {err}");
        }

        // The legal table still loads, so the refusal is the unknown key and not
        // exec attribute tables in general.
        assert!(
            parse_attributes(&attrs(
                "argv = [\"/bin/true\"]\nrestart_backoff_ms = 250\n[env]\nTERM = \"dumb\""
            ))
            .is_ok()
        );
    }

    /// CORE-3: `restart_backoff_ms` is range-checked *structurally*, in the pure
    /// `parse_attributes` both `load` and `add-node` run before anything is created
    /// — so the §11 atomicity guarantee holds for free and a `--replace` cannot tear
    /// a good graph down for a config that will be refused. Before this it was the
    /// one millisecond timer in the schema with no cap: `86400000` ("a day", three
    /// slipped digits) loaded clean and then never respawned a crashed child again,
    /// with the node reporting `faulted … retrying` for the life of the daemon.
    #[test]
    fn restart_backoff_ms_is_range_checked_structurally() {
        let attrs = |src: &str| -> toml::Table { src.parse().expect("test attributes parse") };

        // Both ends of the range are legal (0 = respawn immediately).
        assert!(parse_attributes(&attrs("argv = [\"/bin/true\"]\nrestart_backoff_ms = 0")).is_ok());
        let at_max = format!("argv = [\"/bin/true\"]\nrestart_backoff_ms = {MAX_TIMER_MS}");
        assert!(parse_attributes(&attrs(&at_max)).is_ok());

        let err = parse_attributes(&attrs(
            "argv = [\"/bin/true\"]\nrestart_backoff_ms = 86400000",
        ))
        .expect_err("a backoff longer than the cap is refused");
        assert!(err.contains("restart_backoff_ms"), "names the field: {err}");
        assert!(err.contains("86400000"), "names the value: {err}");
        assert!(
            err.contains(&MAX_TIMER_MS.to_string()),
            "names the maximum, the same one the other timers use: {err}"
        );
    }
}
