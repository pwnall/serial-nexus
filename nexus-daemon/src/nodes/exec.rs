//! Exec codec node (design §7.6): the escape hatch — a codec whose transform is
//! a child process, so protocol tools under any license run unmodified behind a
//! documented, non-linking interface (§13).
//!
//! **The child protocol (ADR §15.22).** The child speaks the shared envelope
//! (`codec-api`) on stdin and stdout. The multiplexed side is carried on a
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

use codec_api::{Event, EventKind, FrameDecoder};
use nexus_core::Chunk;
use nexus_core::config::NodeConfig;
use nexus_core::graph::{EndpointAddr, Facing};
use nexus_core::state::{NodeState, NodeStatus};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::boundary;
use crate::cell::CriticalCell;
use crate::runtime::{
    CHANNEL_CAP, DataFrame, DropCounters, READ_BUF, SharedFanOut, SharedTargetEdge, Wiring,
    data_frames, reacquire_held,
};
use crate::tap::TapFeed;

/// The reserved wire channel identity for the multiplexed (device) side (§15.22).
/// The graph forbids an empty real channel identity, so this never collides.
const MUX_CHANNEL: &str = "";

/// The exec codec's validated attribute schema (§7.6). Deserialized from the
/// opaque config table; a schema failure is structural and fails the load (§11).
#[derive(Debug, Deserialize)]
struct ExecAttributes {
    /// The child command and its arguments (required, non-empty).
    argv: Vec<String>,
    /// Extra environment for the child.
    #[serde(default)]
    env: HashMap<String, String>,
    /// Backoff before restarting a crashed child.
    #[serde(default = "default_backoff_ms")]
    restart_backoff_ms: u64,
}

fn default_backoff_ms() -> u64 {
    200
}

/// Parse and validate the exec attribute table (§8/§11: structural on failure).
pub fn parse_attributes(attributes: &toml::Table) -> Result<(), String> {
    let attrs = ExecAttributes::deserialize(attributes.clone())
        .map_err(|e| format!("exec codec attributes: {e}"))?;
    if attrs.argv.is_empty() {
        return Err("exec codec attributes: argv must be non-empty".to_owned());
    }
    Ok(())
}

#[derive(Default)]
struct ChannelStat {
    delivered_hostward: Cell<u64>,
    discarded_unattached: Cell<u64>,
    active: Cell<bool>,
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
    /// Shared with the supervisor task, which flips it to faulted on a crash and
    /// back to active once a child is running. Carries the transition timestamp
    /// (§7), so a restart loop's `faulted` stamp moves with each real restart.
    status: Rc<CriticalCell<NodeState>>,
    tasks: Vec<JoinHandle<()>>,
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
            status: Rc::new(CriticalCell::new(NodeState::new(NodeStatus::Active))),
            tasks: Vec::new(),
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
            let Some(mut rx) = wiring.host_targetward_rx.remove(&addr) else {
                continue;
            };
            let discarded = self.mux_discarded_targetward.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(bytes) = rx.recv().await {
                    discarded.set(discarded.get() + bytes.len() as u64);
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
        for (ch, mut rx) in channel_rxs {
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
                status: self.status.clone(),
            })));
    }

    /// Re-report status after edge surgery on `endpoint` (§15.35). Only the
    /// multiplexed side decides `active`/`waiting`; a channel endpoint faces host.
    pub fn set_upstream_attached(&mut self, endpoint: &EndpointAddr, attached: bool) {
        if !endpoint.is_default() || endpoint.node != self.name {
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
        json!({
            "codec": "exec",
            "faces": self.faces.to_string(),
            "restart_count": self.restart_count.get(),
            // Bytes that never reached the child because the envelope refused to
            // frame them (§5 all-loss-counted; unreachable for a sane channel id).
            "discarded_unframable": self.unframable_discarded.get(),
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
                "discarded_targetward": self.mux_discarded_targetward.get(),
            },
            "channels": channels,
        })
    }

    /// Ask this node's tasks to stop, without waiting (§16.1, BND-1). The child
    /// process is `kill_on_drop`, so aborting the supervisor is the whole signal;
    /// the method exists so the daemon can signal every node uniformly.
    pub fn signal_stop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }

    pub fn teardown(&mut self) {
        self.signal_stop();
    }
}

impl Drop for ExecCodecNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
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
    status: Rc<CriticalCell<NodeState>>,
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
        // `connect` from racing to describe the same node.
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
        };
        let end = pump_child(stdin, stdout, stderr, &mut a.src_rx, &routing).await;

        let _ = child.kill().await;
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
                        let c = routing.unframable_discarded;
                        c.set(c.get() + n as u64);
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
    // only stdin/stdout death or a closed source does (§16.1 park-don't-teardown).
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
                // Targetward remux output → the device, backpressured (§5). Gated on
                // the exec codec holding the serial lock (§6).
                // Re-read the live edge per event (§15.35), and — unlike the codec
                // and the map — **never park** on an unattached one. This route runs
                // inside the child's single stdout decode loop, so a parked mux event
                // stalls every *hostward* channel event queued behind it: one
                // detached device edge would stop delivery to local consumers that
                // have nothing to do with it. A shared pump counts; only a
                // per-endpoint pump can afford to backpressure.
                if let Some((tx, lock, id)) = mux_edge.origin() {
                    // The serial endpoint went away under us (its node was removed at
                    // runtime, or the graph was replaced): these device-bound bytes
                    // have nowhere left to go, exactly like the no-path case below.
                    if !reacquire_held(&lock, id).await {
                        mux_discarded_targetward.set(mux_discarded_targetward.get() + n);
                        return;
                    }
                    if tx.send(bytes).await.is_err() {
                        // The targetward channel closed between the grant and the
                        // send — same loss, same counter.
                        mux_discarded_targetward.set(mux_discarded_targetward.get() + n);
                    }
                } else {
                    // No targetward serial path (a read-only / hostward-only mux edge):
                    // the child's device-bound bytes have nowhere to go. Count the loss
                    // so it stays located and attributable, never silently dropped (§5).
                    mux_discarded_targetward.set(mux_discarded_targetward.get() + n);
                }
            } else {
                let n = bytes.len() as u64;
                let stat = stats.get(ev.channel.as_str());
                if let Some(s) = stat {
                    s.active.set(true);
                }
                // Mirror to this channel's tap hub for taps and the replay ring
                // (§17), independent of whether a graph consumer is bound — and
                // *outside* the fan-out below, so a spy never masks a real
                // consumer's absence (§5).
                if let Some(feed) = channel_feeds.get(ev.channel.as_str()) {
                    feed.mirror(&bytes);
                }
                // The one shared hostward fan-out (§5, F1): it charges a slow
                // consumer's full-buffer drop to that consumer, and an
                // all-`Closed`/empty sink set to this channel's unattached counter.
                if let Some(sinks) = channel_sinks.get(ev.channel.as_str()) {
                    // `channel_sinks` and `stats` are both keyed by the configured
                    // channel list, so a bound sink always has a stat; `scratch` is
                    // the defensive arm — its count is thrown away, but the bytes
                    // are still delivered.
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
        }
        EventKind::Open => {
            if let Some(s) = stats.get(ev.channel.as_str()) {
                s.active.set(true);
            }
        }
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
        use nexus_core::lock::{Arbitration, EndpointLock, OriginId, WriteMode};
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
}
