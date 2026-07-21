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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use codec_api::{Event, EventKind, FrameDecoder, encode};
use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nexus_core::graph::{EndpointAddr, Facing};
use nexus_core::lock::OriginId;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::runtime::{CHANNEL_CAP, DropCounters, HostwardSink, READ_BUF, SharedLock, Wiring};

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
    /// Times the child has been (re)started after a crash — observable state (§7.6).
    restart_count: Rc<Cell<u64>>,
    /// Shared with the supervisor task, which flips it to faulted on a crash and
    /// back to active once a child is running.
    status: Rc<RefCell<NodeStatus>>,
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
            restart_count: Rc::new(Cell::new(0)),
            status: Rc::new(RefCell::new(NodeStatus::Active)),
            tasks: Vec::new(),
        }
    }

    pub fn start(&mut self, wiring: &mut Wiring) {
        if self.faces != Facing::Target {
            *self.status.borrow_mut() = NodeStatus::Faulted {
                reason: "exec re-multiplexer orientation (faces=host) lands in phase 6".to_owned(),
            };
            return;
        }
        let mux = EndpointAddr::node(&self.name);
        let Some(mux_hostward_rx) = wiring.target_hostward_rx.remove(&mux) else {
            *self.status.borrow_mut() = NodeStatus::Waiting {
                reason: "multiplexed side has no attached upstream".to_owned(),
            };
            return;
        };
        let mux_targetward_tx = wiring.target_targetward_tx.remove(&mux);
        let serial_lock = wiring.origin_locks.remove(&mux);
        self.mux_counters = wiring.target_counters.remove(&mux);

        let mut channel_sinks: HashMap<String, Vec<HostwardSink>> = HashMap::new();
        let mut channel_rxs: Vec<(String, mpsc::Receiver<Chunk>)> = Vec::new();
        for ch in &self.channels {
            let addr = EndpointAddr::channel(&self.name, ch);
            if let Some(sinks) = wiring.host_sinks.remove(&addr) {
                channel_sinks.insert(ch.clone(), sinks);
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
        {
            let src_tx = src_tx.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                let mut rx = mux_hostward_rx;
                while let Some(chunk) = rx.recv().await {
                    if src_tx.send((MUX_CHANNEL.to_owned(), chunk)).await.is_err() {
                        break;
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
                mux_targetward_tx,
                serial_lock,
                channel_sinks,
                stats: self.stats.clone(),
                restart_count: self.restart_count.clone(),
                status: self.status.clone(),
            })));
    }

    pub fn status(&self) -> NodeStatus {
        self.status.borrow().clone()
    }

    pub fn state_extra(&self) -> Value {
        let channels: serde_json::Map<String, Value> = self
            .channels
            .iter()
            .map(|ch| {
                let stat = self.stats.get(ch);
                let obj = json!({
                    "status": if stat.is_some_and(|s| s.active.get()) { "active" } else { "waiting" },
                    "delivered_hostward": stat.map_or(0, |s| s.delivered_hostward.get()),
                    "discarded_unattached": stat.map_or(0, |s| s.discarded_unattached.get()),
                });
                (ch.clone(), obj)
            })
            .collect();
        json!({
            "codec": "exec",
            "faces": self.faces.to_string(),
            "restart_count": self.restart_count.get(),
            "multiplexed": {
                "dropped_slow_consumer": self.mux_counters.as_ref().map_or(0, |c| c.dropped_full()),
            },
            "channels": channels,
        })
    }

    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
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
    mux_targetward_tx: Option<mpsc::Sender<Chunk>>,
    serial_lock: Option<(SharedLock, OriginId)>,
    channel_sinks: HashMap<String, Vec<HostwardSink>>,
    stats: Rc<HashMap<String, Rc<ChannelStat>>>,
    restart_count: Rc<Cell<u64>>,
    status: Rc<RefCell<NodeStatus>>,
}

/// Supervise the child: (re)spawn it, pump envelope frames both ways until it
/// dies, then fault, back off, and restart (§7.6). The merged source and routing
/// outputs persist across restarts, so a restarted child resumes cleanly.
async fn supervise(mut a: SuperviseArgs) {
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
                *a.status.borrow_mut() = NodeStatus::Faulted {
                    reason: format!("spawn {:?}: {e}", a.argv[0]),
                };
                tokio::time::sleep(std::time::Duration::from_millis(a.backoff_ms)).await;
                a.restart_count.set(a.restart_count.get() + 1);
                continue;
            }
        };
        *a.status.borrow_mut() = NodeStatus::Active;

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        // Pump both directions concurrently until the child dies or the source
        // closes. The outcome distinguishes the two so teardown does not respawn.
        let routing = Routing {
            mux_targetward_tx: &a.mux_targetward_tx,
            serial_lock: &a.serial_lock,
            channel_sinks: &a.channel_sinks,
            stats: &a.stats,
        };
        let end = pump_child(stdin, stdout, stderr, &mut a.src_rx, &routing).await;

        let _ = child.kill().await;
        match end {
            // The merged source closed: the node was torn down (its forwarders
            // dropped their senders) or its upstream is gone. Stop; do not respawn.
            PumpEnd::SourceClosed => return,
            PumpEnd::ChildDied => {
                a.restart_count.set(a.restart_count.get() + 1);
                *a.status.borrow_mut() = NodeStatus::Faulted {
                    reason: format!("child exited; restarting (count {})", a.restart_count.get()),
                };
                tokio::time::sleep(std::time::Duration::from_millis(a.backoff_ms)).await;
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
    mux_targetward_tx: &'a Option<mpsc::Sender<Chunk>>,
    serial_lock: &'a Option<(SharedLock, OriginId)>,
    channel_sinks: &'a HashMap<String, Vec<HostwardSink>>,
    stats: &'a Rc<HashMap<String, Rc<ChannelStat>>>,
}

/// Pump one child instance. The stdin-feeding and stdout-reading loops run as
/// **concurrently-polled** futures in one `select!` — not two branches of a
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
    stderr: tokio::process::ChildStderr,
    src_rx: &mut mpsc::Receiver<(String, Chunk)>,
    routing: &Routing<'_>,
) -> PumpEnd {
    tokio::select! {
        // stdin: frame each tagged chunk and write it to the child.
        end = async {
            while let Some((channel, bytes)) = src_rx.recv().await {
                let mut frame = Vec::new();
                if encode(&Event::data(channel.as_str(), bytes), &mut frame).is_err() {
                    continue; // an oversize frame is dropped rather than desyncing
                }
                if stdin.write_all(&frame).await.is_err() || stdin.flush().await.is_err() {
                    return PumpEnd::ChildDied; // child stdin broke
                }
            }
            PumpEnd::SourceClosed
        } => end,
        // stdout: decode envelope frames and route them.
        end = async {
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
        } => end,
        // stderr → diagnostics; drains until EOF, then parks so it never ends the
        // pump on its own (only stdin/stdout death or a closed source does).
        end = async {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "exec-codec", "child stderr: {line}");
            }
            std::future::pending::<PumpEnd>().await
        } => end,
    }
}

/// Route one event the child emitted: a frame on the reserved multiplexed channel
/// goes targetward to the device (gated on holding the serial's lock); a frame on
/// a real channel is fanned out hostward to that channel's consumers.
async fn route_event(ev: Event, routing: &Routing<'_>) {
    let Routing {
        mux_targetward_tx,
        serial_lock,
        channel_sinks,
        stats,
    } = routing;
    match ev.kind {
        EventKind::Data(bytes) => {
            if ev.channel.as_str() == MUX_CHANNEL {
                // Targetward remux output → the device, backpressured (§5). Gated on
                // the exec codec holding the serial lock (§6).
                if let (Some(tx), Some((lock, id))) = (mux_targetward_tx, serial_lock) {
                    if !ensure_holds(lock, *id).await {
                        return;
                    }
                    let _ = tx.send(bytes).await;
                }
            } else {
                let n = bytes.len() as u64;
                let stat = stats.get(ev.channel.as_str());
                if let Some(s) = stat {
                    s.active.set(true);
                }
                match channel_sinks.get(ev.channel.as_str()) {
                    Some(sinks) => {
                        if let Some(s) = stat {
                            s.delivered_hostward.set(s.delivered_hostward.get() + n);
                        }
                        for (tx, counters) in sinks {
                            match tx.try_send(bytes.clone()) {
                                Ok(()) => {}
                                Err(TrySendError::Full(_)) => counters.add_full(n),
                                Err(TrySendError::Closed(_)) => {}
                            }
                        }
                    }
                    None => {
                        if let Some(s) = stat {
                            s.discarded_unattached.set(s.discarded_unattached.get() + n);
                        }
                    }
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

/// Ensure the codec holds the serial's write lock, re-acquiring FIFO after a steal
/// (§6). Mirrors the in-process codec's gate; returns false if the endpoint is gone.
async fn ensure_holds(lock: &SharedLock, id: OriginId) -> bool {
    if lock.borrow().may_write(id) {
        return true;
    }
    loop {
        if lock.is_closed() {
            return false;
        }
        let notified = lock.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        // Already holds, or reclaim as a held origin ahead of on-demand waiters (§6).
        let outcome = {
            let mut g = lock.borrow_mut();
            if g.may_write(id) {
                Some(false)
            } else if g.reclaim_held(id) {
                Some(true)
            } else {
                None
            }
        };
        match outcome {
            Some(fresh) => {
                if fresh {
                    lock.emit_change();
                }
                return true;
            }
            None => notified.await,
        }
    }
}
