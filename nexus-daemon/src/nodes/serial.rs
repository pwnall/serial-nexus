//! Serial port node (design §7.1). Faces host in the normal role.
//!
//! The device is named by a resolver identity (`usb:vid:pid:serial:iface`, a
//! `by-path:` topology port, or a `raw:`/`/dev` path) — [`nexus_core::Resolver`]
//! turns that into the current `/dev` path at every open and every reconnect
//! recheck (§12). A missing device does not fail the load: the node comes up
//! `waiting` and heals when its device reappears (§7.1 faulted-and-wait).
//!
//! Byte flow: the blocking `serial2` fd is set non-blocking (for the drain loop)
//! and *not* driven by `serial2-tokio` (which hides the fd we need for
//! `TIOCEXCL`/`TIOCGICOUNT`) nor `tokio::io::unix::AsyncFd` (whose epoll readiness
//! busy-loops on pty masters, §15.18). The **hostward reader runs on a dedicated
//! blocking thread** — a `poll(2)` blocking wait wakes it the instant the device
//! has data, so it drains at line rate and costs zero CPU while parked (§15.19).
//! It broadcasts to every attached consumer (lossy `try_send`, §5). The low-rate
//! **targetward writer** and the **reconnect supervisor** share one async task on
//! the runtime thread; read and write on the shared fd are independent directions.
//!
//! **Faulted-and-wait (§7.1).** The reader thread signals device loss (an exit on
//! `POLLHUP`/EOF/error) by pulsing a `Notify`; the supervisor then joins it, drops
//! the port, transitions to `waiting`, and polls the resolver for the device's
//! reappearance *by the same identity* (a squatter on the old `/dev` path resolves
//! to a different identity and is refused, §12). On reappearance it runs the
//! **reopen ritual** — reapply termios, retake `TIOCEXCL`, restore the configured
//! modem lines against auto-reset adapters, set non-blocking — then purges the
//! outage-era targetward backlog (`purge_on_reconnect`, the one sanctioned drain
//! of the never-drop targetward path, counted) and re-arms the reader and writer.

use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::config::{
    DataBits, FlowControl as CfgFlow, ModemLines, NodeConfig, Parity as CfgParity,
    StopBits as CfgStop,
};
use nexus_core::resolver::{DeviceKind, Resolver};
use nexus_core::state::NodeStatus;
use nix::poll::PollFlags;
use serde_json::json;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc::error::TryRecvError};
use tokio::task::JoinHandle;

use crate::boundary::BlockingReader;
use crate::cell::CriticalCell;
use crate::runtime::{self, HostwardSink, READ_BUF};
use nexus_sys as sys;

/// Re-arm interval (ms) for the serial reader thread's blocking readiness poll.
/// A live device wakes it far sooner; on teardown the thread observes its stop
/// flag within this bound, so `join` returns promptly.
const READ_POLL_TIMEOUT_MS: u16 = 200;

/// How often the supervisor rechecks the resolver for a `waiting`/`faulted`
/// device's reappearance (§7.1 "a stat every second or two"). Cheap and portable
/// (by-id readlink + sysfs walk), free of libudev/LGPL linkage (§15.10).
const RECONNECT_POLL: Duration = Duration::from_millis(1000);

/// The termios + modem parameters an open (initial or reopen) applies (§7.1).
#[derive(Clone, Copy)]
struct OpenParams {
    baud: u32,
    data_bits: DataBits,
    parity: CfgParity,
    stop_bits: CfgStop,
    flow: CfgFlow,
    /// Initial/retained modem-line assertions, restored on every (re)open so line
    /// states stay deterministic against auto-reset adapters (§7.1).
    modem: ModemLines,
    /// Discard the outage-era targetward backlog on reconnect (default on, §7.1).
    purge_on_reconnect: bool,
}

/// State the reconnect supervisor mutates and the node's `&self` methods read,
/// shared on the single runtime thread (Rc/[`CriticalCell`] — the reader thread
/// touches only atomics, never this).
struct SerialShared {
    status: NodeStatus,
    /// The currently-open port, or `None` while `waiting`/`faulted`. Retained for
    /// `state_extra` (driver counters) and the serial-signal verbs.
    port: Option<Rc<SerialPort>>,
}

pub struct SerialNode {
    pub name: String,
    /// The resolver identity this node is configured for (config, round-trips).
    device: String,
    resolver: Resolver,
    params: OpenParams,
    shared: Rc<CriticalCell<SerialShared>>,
    reader_slot: Rc<CriticalCell<BlockingReader>>,
    /// Bytes read from the port and discarded because nothing was attached to
    /// consume them (§5). Shared with the reader thread, hence atomic.
    discarded_unattached: Arc<AtomicU64>,
    /// Bytes drained from the targetward backlog and discarded on reconnect
    /// (§7.1 purge-on-reconnect). The one sanctioned targetward drop, counted.
    purged_reconnect: Arc<AtomicU64>,
    /// The supervisor task (drives the targetward writer and the reconnect poll).
    tasks: Vec<JoinHandle<()>>,
}

impl SerialNode {
    pub fn create(config: &NodeConfig, resolver: &Resolver) -> SerialNode {
        let NodeConfig::Serial {
            name,
            device,
            baud,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            purge_on_reconnect,
            modem,
            ..
        } = config
        else {
            unreachable!("SerialNode::create called with non-Serial config");
        };

        let params = OpenParams {
            baud: *baud,
            data_bits: *data_bits,
            parity: *parity,
            stop_bits: *stop_bits,
            flow: *flow_control,
            modem: *modem,
            purge_on_reconnect: *purge_on_reconnect,
        };

        // Initial open, synchronous so `state` is accurate the instant `load`
        // returns. Identity → current path (squatter-safe for identity forms,
        // §12); absence is `waiting`, any other open error is `faulted` — neither
        // fails the load (§15.8).
        let (status, port) = match resolver.resolve_current_path(device) {
            Some(path) => match open_port(&path, &params) {
                Ok(p) => (NodeStatus::Active, Some(Rc::new(p))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                    NodeStatus::Waiting {
                        reason: format!("device {device} vanished during open"),
                    },
                    None,
                ),
                Err(e) => (
                    NodeStatus::Faulted {
                        reason: format!("open {device}: {e}"),
                    },
                    None,
                ),
            },
            None => (
                NodeStatus::Waiting {
                    reason: format!("device {device} not present"),
                },
                None,
            ),
        };

        SerialNode {
            name: name.clone(),
            device: device.clone(),
            resolver: resolver.clone(),
            params,
            shared: Rc::new(CriticalCell::new(SerialShared { status, port })),
            reader_slot: Rc::new(CriticalCell::new(BlockingReader::default())),
            discarded_unattached: Arc::new(AtomicU64::new(0)),
            purged_reconnect: Arc::new(AtomicU64::new(0)),
            tasks: Vec::new(),
        }
    }

    /// Start the data plane: broadcast hostward to the attached consumers, drain
    /// targetward from them, and supervise faulted-and-wait reconnect (§7.1). A
    /// single async supervisor owns the targetward receiver across outages, so a
    /// `waiting` port backpressures its origins (channel fills, §5) rather than
    /// dropping their commands; only the sanctioned purge-on-reconnect drains it.
    pub fn start(
        &mut self,
        hostward: Vec<HostwardSink>,
        targetward: Option<mpsc::Receiver<Chunk>>,
    ) {
        let ctx = SuperviseCtx {
            name: self.name.clone(),
            device: self.device.clone(),
            resolver: self.resolver.clone(),
            params: self.params,
            hostward,
            targetward,
            shared: self.shared.clone(),
            reader_slot: self.reader_slot.clone(),
            discarded: self.discarded_unattached.clone(),
            purged: self.purged_reconnect.clone(),
        };
        self.tasks.push(tokio::task::spawn_local(supervise(ctx)));
    }

    pub fn status(&self) -> NodeStatus {
        self.shared.with(|sh| sh.status.clone())
    }

    /// The open port, for the serial-signal verbs (§7.1). `None` while the node is
    /// `waiting`/`faulted`. Borrow-clone-drop before any `.await` (§15.20).
    pub(crate) fn port(&self) -> Option<Rc<SerialPort>> {
        self.shared.with(|sh| sh.port.clone())
    }

    pub fn state_extra(&self) -> serde_json::Value {
        self.shared.with(|sh| {
            // Driver input counters (TIOCGICOUNT) surface framing/parity/overrun loss
            // "where supported" (§5, §7.1); a pts (test device) returns an error, so
            // report null rather than faulting or fabricating zeros.
            let driver_counters = sh
                .port
                .as_ref()
                .and_then(|p| sys::read_icounts(p.as_raw_fd()).ok())
                .map(|c| {
                    json!({
                        "rx": c.rx,
                        "tx": c.tx,
                        "frame": c.frame,
                        "overrun": c.overrun,
                        "parity": c.parity,
                        "brk": c.brk,
                        "buf_overrun": c.buf_overrun,
                    })
                });
            // The current resolved /dev path is state (§12); the configured identity is
            // config. Report both so an operator sees which device answered.
            let resolved_path = self
                .resolver
                .resolve_current_path(&self.device)
                .map(|p| p.display().to_string());
            let modem_lines = sh.port.as_ref().map(|p| read_modem_lines(p.as_raw_fd()));
            json!({
                "identity": self.device,
                "identity_kind": DeviceKind::of(&self.device).label(),
                "resolved_path": resolved_path,
                "baud": self.params.baud,
                "open": sh.port.is_some(),
                "discarded_unattached": self.discarded_unattached.load(Ordering::Relaxed),
                "purged_on_reconnect": self.purged_reconnect.load(Ordering::Relaxed),
                "modem_lines": modem_lines,
                "driver_counters": driver_counters,
            })
        })
    }

    /// Stop the supervisor and the reader thread, then drop the port. The reader is
    /// joined *before* the port drops so its fd stays valid throughout (fd-reuse
    /// race). Called on teardown/shutdown.
    pub fn teardown(&mut self) {
        // Abort the supervisor first so it cannot re-arm a reader after we join.
        // Sync teardown holds the runtime thread, so the aborted task will not run
        // again before we finish; its dropped future releases its port clone after
        // the reader is already joined.
        for t in self.tasks.drain(..) {
            t.abort();
        }
        stop_join_reader(&self.reader_slot);
        self.shared.with_mut(|sh| sh.port = None);
    }
}

/// Everything the reconnect supervisor owns for one node's lifetime.
struct SuperviseCtx {
    name: String,
    device: String,
    resolver: Resolver,
    params: OpenParams,
    hostward: Vec<HostwardSink>,
    targetward: Option<mpsc::Receiver<Chunk>>,
    shared: Rc<CriticalCell<SerialShared>>,
    reader_slot: Rc<CriticalCell<BlockingReader>>,
    discarded: Arc<AtomicU64>,
    purged: Arc<AtomicU64>,
}

/// One step of the Active loop: drive the targetward writer and watch for loss.
enum Step {
    /// Keep running Active (a chunk was written, or a quiescent wait elapsed).
    Continue,
    /// The device was lost (reader exited, or a write failed) — reconnect.
    Lost,
    /// The targetward channel closed (origins gone / teardown); keep hostward
    /// alive but stop driving the writer.
    WriterClosed,
}

/// The reconnect supervisor (§7.1): adopts the create-opened port, drives the
/// targetward writer, and on device loss polls the resolver until the *same*
/// identity reappears, then runs the reopen ritual.
async fn supervise(mut ctx: SuperviseCtx) {
    // Adopt the port `create` opened, if any: arm a reader on it.
    let mut lost: Option<Arc<Notify>> = None;
    if let Some(port) = ctx.shared.with(|sh| sh.port.clone()) {
        match arm_reader(&port, &ctx) {
            Ok(notify) => lost = Some(notify),
            Err(e) => fault(&ctx, format!("arm reader: {e}")),
        }
    }

    loop {
        match &lost {
            // Active: a live port with an armed reader.
            Some(notify) => {
                let port = match ctx.shared.with(|sh| sh.port.clone()) {
                    Some(p) => p,
                    None => {
                        lost = None;
                        continue;
                    }
                };
                match active_step(&port, notify, &mut ctx.targetward).await {
                    Step::Continue => {}
                    Step::WriterClosed => {
                        // No more targetward; keep the reader running (hostward
                        // survives, §15.24) and wait for loss only.
                        ctx.targetward = None;
                    }
                    Step::Lost => {
                        stop_join_reader(&ctx.reader_slot);
                        set_waiting(&ctx, format!("device {} lost", ctx.device));
                        lost = None;
                    }
                }
            }
            // Waiting/faulted: poll for the device's reappearance.
            None => {
                tokio::time::sleep(RECONNECT_POLL).await;
                if let Some(path) = ctx.resolver.resolve_current_path(&ctx.device) {
                    match open_port(&path, &ctx.params) {
                        Ok(port) => {
                            let port = Rc::new(port);
                            purge_on_reconnect(&mut ctx).await;
                            match arm_reader(&port, &ctx) {
                                Ok(notify) => {
                                    set_active(&ctx, port);
                                    lost = Some(notify);
                                }
                                Err(e) => fault(&ctx, format!("arm reader: {e}")),
                            }
                        }
                        // Vanished again between the resolve and the open: stay
                        // waiting and retry. Any other error faults but keeps polling.
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => fault(&ctx, format!("reopen {}: {e}", ctx.device)),
                    }
                }
            }
        }
    }
}

/// Drive one targetward chunk to the port, or wait for loss (§7.1/§5). Returns as
/// soon as either fires so the supervisor can react promptly.
async fn active_step(
    port: &Rc<SerialPort>,
    lost: &Notify,
    targetward: &mut Option<mpsc::Receiver<Chunk>>,
) -> Step {
    match targetward {
        Some(rx) => {
            tokio::select! {
                biased;
                _ = lost.notified() => Step::Lost,
                got = rx.recv() => match got {
                    Some(chunk) => {
                        if runtime::write_all(port.as_raw_fd(), &chunk).await.is_err() {
                            Step::Lost
                        } else {
                            Step::Continue
                        }
                    }
                    None => Step::WriterClosed,
                }
            }
        }
        // No writer: wait only for device loss, at zero CPU.
        None => {
            lost.notified().await;
            Step::Lost
        }
    }
}

/// Purge the outage-era targetward backlog on reconnect (§7.1). The parked
/// receiver's buffered chunks — plus any chunk an origin is suspended mid-`send`
/// behind that full channel (§5 backpressure) — are the daemon-side backlog that
/// accumulated while the node was `waiting`; draining them with a counter is the
/// one sanctioned targetward drop. Post-reconnect commands are kept.
///
/// A backpressured origin is parked inside `tx.send(chunk).await` holding one
/// already-read, outage-era chunk. A synchronous `try_recv` loop frees channel
/// permits but never yields, so that blocked send would resolve only on the
/// supervisor's next await — after `set_active` — and fire its stale chunk into
/// the just-reopened (likely power-cycled) device. So drain the channel, then
/// `yield_now` to let every freed-permit sender resolve, and drain again — a
/// *bounded* few rounds, enough to flush the finite in-flight chunks (one per
/// suspended sender) without unboundedly draining a continuously-producing origin,
/// whose genuinely-post-reconnect bytes are kept. Runs while the node is still
/// `waiting` (no reader/writer armed), so nothing reaches the device during the
/// drain.
async fn purge_on_reconnect(ctx: &mut SuperviseCtx) {
    if !ctx.params.purge_on_reconnect {
        return;
    }
    let Some(rx) = ctx.targetward.as_mut() else {
        return;
    };
    let mut purged = 0u64;
    // Drain the currently-buffered backlog, then give any origin suspended inside
    // `tx.send().await` (backpressured, §5, holding one already-read outage-era
    // chunk) a *bounded* chance to resolve and be drained+counted. A freed channel
    // permit wakes a blocked sender, and one `yield_now` runs every currently-
    // runnable one, so a couple of drain+yield rounds flush the finite in-flight
    // chunks (one per suspended sender) without unboundedly draining a
    // continuously-producing origin — a streaming leg's genuinely-post-reconnect
    // bytes are kept, not purged. Termination is by *whether a chunk was drained*,
    // never a byte-count delta: a round that drains only zero-length chunks still
    // made progress and must yield, or a backpressured non-empty chunk behind those
    // empties would be stranded. The node is still `waiting` here (no reader/writer
    // armed), so nothing reaches the device during the drain.
    for _ in 0..3 {
        let mut drained_any = false;
        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    purged += chunk.len() as u64;
                    drained_any = true;
                }
                Err(TryRecvError::Empty) => break,
                // The senders are gone; the writer will observe close next.
                Err(TryRecvError::Disconnected) => break,
            }
        }
        // Nothing drained this pass: no origin was blocked behind the channel, so
        // the pipeline is quiescent.
        if !drained_any {
            break;
        }
        // Let any origin blocked in `tx.send().await` behind the (now-drained) full
        // channel resolve its in-flight chunk so the next pass drains it, before the
        // node goes Active and the writer starts.
        tokio::task::yield_now().await;
    }
    if purged > 0 {
        ctx.purged.fetch_add(purged, Ordering::Relaxed);
    }
}

/// Set the port non-blocking and spawn a fresh reader thread, recording its
/// stop flag and handle in the shared slot (any previous reader must already be
/// joined). Returns the `Notify` the reader pulses on device loss.
fn arm_reader(port: &Rc<SerialPort>, ctx: &SuperviseCtx) -> std::io::Result<Arc<Notify>> {
    sys::set_nonblocking(port.as_raw_fd())?;
    let fd = port.as_raw_fd();
    let hostward = ctx.hostward.clone();
    let discarded = ctx.discarded.clone();
    // The boundary-supervisor library owns the stop flag, the loss `Notify`, and
    // the join handle (§16.1 loss-notify + join-then-transition); we supply the
    // node-specific reader body.
    ctx.reader_slot.with_mut(|slot| {
        slot.arm(format!("serial-rx-{}", ctx.name), move |stop, lost| {
            reader_thread(fd, hostward, discarded, stop, lost)
        })?;
        Ok(slot.lost())
    })
}

/// Stop the current reader thread and join it within the poll-timeout bound. On
/// the loss path the thread has already exited, so this returns at once; on
/// teardown of a live device it costs at most one poll interval.
fn stop_join_reader(reader_slot: &Rc<CriticalCell<BlockingReader>>) {
    reader_slot.with_mut(|slot| slot.stop_join());
}

fn set_active(ctx: &SuperviseCtx, port: Rc<SerialPort>) {
    ctx.shared.with_mut(|sh| {
        sh.port = Some(port);
        sh.status = NodeStatus::Active;
    });
}

fn set_waiting(ctx: &SuperviseCtx, reason: String) {
    ctx.shared.with_mut(|sh| {
        sh.port = None;
        sh.status = NodeStatus::Waiting { reason };
    });
}

fn fault(ctx: &SuperviseCtx, reason: String) {
    ctx.shared.with_mut(|sh| {
        sh.port = None;
        sh.status = NodeStatus::Faulted { reason };
    });
}

/// Hostward reader thread (§15.19): a blocking `poll(2)` waits for readability —
/// waking the instant the device has data (line rate) and parking at zero CPU
/// otherwise — then the loop drains fully and broadcasts each chunk to every
/// attached consumer. On device loss (`POLLHUP`, EOF, or a read error) it pulses
/// `lost` so the supervisor enters faulted-and-wait; a clean stop (teardown)
/// exits without pulsing.
///
/// Loss is always counted at the boundary that drops it: with nothing attached,
/// bytes are discarded against `discarded_unattached` (§5); with a consumer
/// attached but its bounded buffer full, the drop is counted against that
/// consumer's [`DropCounters`] (a slow spy costs only itself).
fn reader_thread(
    fd: std::os::fd::RawFd,
    hostward: Vec<HostwardSink>,
    discarded_unattached: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    lost: Arc<Notify>,
) {
    let mut buf = vec![0u8; READ_BUF];
    while !stop.load(Ordering::Relaxed) {
        let re = sys::poll_blocking(
            fd,
            PollFlags::POLLIN | PollFlags::POLLHUP,
            READ_POLL_TIMEOUT_MS,
        );
        if re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        lost.notify_one(); // device closed
                        return;
                    }
                    Ok(n) => {
                        if hostward.is_empty() {
                            // Nothing attached: read-and-discard with a counter.
                            discarded_unattached.fetch_add(n as u64, Ordering::Relaxed);
                            continue;
                        }
                        let chunk = Chunk::copy_from_slice(&buf[..n]);
                        // Whether the chunk reached any live boundary. A consumer
                        // cascade-removed while this node survives leaves a
                        // permanently-Closed sink in this snapshot (never rebuilt);
                        // if every sink is Closed the chunk would vanish uncounted,
                        // so track liveness and attribute it below (§5).
                        let mut any_live = false;
                        for (tx, counters) in &hostward {
                            match tx.try_send(chunk.clone()) {
                                // Delivered to a live consumer.
                                Ok(()) => any_live = true,
                                // Slow consumer: its bounded buffer is full — the
                                // drop is counted against it, and it is still live.
                                Err(TrySendError::Full(_)) => {
                                    counters.add_full(n as u64);
                                    any_live = true;
                                }
                                // Receiver gone: whole-node teardown, or a consumer
                                // cascade-removed while this node lives. Counted
                                // below only if no sink took the chunk.
                                Err(TrySendError::Closed(_)) => {}
                            }
                        }
                        // Every sink was Closed: the chunk reached no live boundary
                        // (e.g. this node's only consumer was removed with no reader
                        // re-arm). Count it as unattached loss so §5's "loss is
                        // always visible and attributable" holds.
                        if !any_live {
                            discarded_unattached.fetch_add(n as u64, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        lost.notify_one();
                        return;
                    }
                }
            }
        } else if re.contains(PollFlags::POLLHUP) {
            lost.notify_one(); // device gone
            return;
        }
        // Bare timeout (empty revents): re-check the stop flag and re-arm.
    }
}

impl Drop for SerialNode {
    fn drop(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
        stop_join_reader(&self.reader_slot);
    }
}

// ---------------------------------------------------------------------------
// Serial-signal operations (§7.1) — driven on the runtime thread from the
// control plane, operating on the retained `Rc<SerialPort>`. `send-break` and
// `pulse-dtr` hold a line for a bounded interval; a restore guard deasserts even
// if the dispatch future is cancelled (a dropped control connection, §15.20), so
// a line is never left stuck. Each returns the driver error unchanged, so a fd
// that does not support the operation (a pts) surfaces cleanly rather than
// pretending success.
// ---------------------------------------------------------------------------

/// Restores a break/modem line on drop (including cancellation), so a bounded
/// assertion never leaks past the verb.
struct RestoreGuard<'a> {
    port: &'a SerialPort,
    op: RestoreOp,
}

enum RestoreOp {
    ClearBreak,
    SetDtr(bool),
}

impl Drop for RestoreGuard<'_> {
    fn drop(&mut self) {
        let _ = match self.op {
            RestoreOp::ClearBreak => self.port.set_break(false),
            RestoreOp::SetDtr(level) => self.port.set_dtr(level),
        };
    }
}

/// Assert a serial break for `ms` milliseconds, then clear it (§7.1). The clear
/// runs even on cancellation.
pub(crate) async fn send_break(port: &SerialPort, ms: u64) -> std::io::Result<()> {
    port.set_break(true)?;
    let _guard = RestoreGuard {
        port,
        op: RestoreOp::ClearBreak,
    };
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(())
}

/// Pulse DTR: drive it to `assert` for `ms` milliseconds, then to `!assert` — the
/// classic auto-reset toggle (§7.1). The final level is set even on cancellation.
pub(crate) async fn pulse_dtr(port: &SerialPort, ms: u64, assert: bool) -> std::io::Result<()> {
    port.set_dtr(assert)?;
    let _guard = RestoreGuard {
        port,
        op: RestoreOp::SetDtr(!assert),
    };
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(())
}

/// Set DTR and/or RTS on the live port (§7.1). A `None` line is left untouched.
/// This acts on the live port only; the configuration's initial modem lines are
/// what a reopen restores (§15.8), so a live change does not survive a replug.
pub(crate) fn set_modem(
    port: &SerialPort,
    dtr: Option<bool>,
    rts: Option<bool>,
) -> std::io::Result<()> {
    if let Some(dtr) = dtr {
        port.set_dtr(dtr)?;
    }
    if let Some(rts) = rts {
        port.set_rts(rts)?;
    }
    Ok(())
}

/// Read the current modem-line levels for state (§7.1), or `null` where the fd
/// does not support `TIOCMGET` (a pts). DTR/RTS are outputs, the rest inputs.
fn read_modem_lines(fd: std::os::fd::RawFd) -> serde_json::Value {
    match sys::read_modem_bits(fd) {
        Ok(bits) => json!({
            "dtr": bits & libc::TIOCM_DTR != 0,
            "rts": bits & libc::TIOCM_RTS != 0,
            "cts": bits & libc::TIOCM_CTS != 0,
            "dsr": bits & libc::TIOCM_DSR != 0,
            "dcd": bits & libc::TIOCM_CAR != 0,
            "ri": bits & libc::TIOCM_RI != 0,
        }),
        Err(_) => serde_json::Value::Null,
    }
}

fn open_port(device: &std::path::Path, params: &OpenParams) -> std::io::Result<SerialPort> {
    let port = SerialPort::open(device, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(params.baud)?;
        s.set_char_size(char_size(params.data_bits));
        s.set_parity(map_parity(params.parity));
        s.set_stop_bits(map_stop(params.stop_bits));
        s.set_flow_control(map_flow(params.flow));
        Ok(s)
    })?;
    // serial2 does not take TIOCEXCL; the daemon does, so stray processes cannot
    // share the port (§7.1, P3 finding). Re-taken on every reopen.
    sys::set_exclusive(port.as_raw_fd(), true)
        .map_err(|e| std::io::Error::other(format!("TIOCEXCL: {e}")))?;
    // Modem-line assertions (§7.1): a line left unset (None) keeps the driver's
    // power-on state; an asserted/deasserted line is made deterministic here and
    // reapplied by the reopen ritual against auto-reset adapters.
    if let Some(dtr) = params.modem.dtr {
        port.set_dtr(dtr)
            .map_err(|e| std::io::Error::other(format!("set DTR: {e}")))?;
    }
    if let Some(rts) = params.modem.rts {
        port.set_rts(rts)
            .map_err(|e| std::io::Error::other(format!("set RTS: {e}")))?;
    }
    Ok(port)
}

fn char_size(d: DataBits) -> CharSize {
    match d {
        DataBits::Five => CharSize::Bits5,
        DataBits::Six => CharSize::Bits6,
        DataBits::Seven => CharSize::Bits7,
        DataBits::Eight => CharSize::Bits8,
    }
}

fn map_parity(p: CfgParity) -> Parity {
    match p {
        CfgParity::None => Parity::None,
        CfgParity::Odd => Parity::Odd,
        CfgParity::Even => Parity::Even,
    }
}

fn map_stop(s: CfgStop) -> StopBits {
    match s {
        CfgStop::One => StopBits::One,
        CfgStop::Two => StopBits::Two,
    }
}

fn map_flow(f: CfgFlow) -> FlowControl {
    match f {
        CfgFlow::None => FlowControl::None,
        CfgFlow::XonXoff => FlowControl::XonXoff,
        CfgFlow::RtsCts => FlowControl::RtsCts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A supervisor context whose only meaningful fields for `purge_on_reconnect`
    /// are `params`, `targetward`, and `purged`; the rest are inert placeholders.
    fn test_ctx(targetward: Option<mpsc::Receiver<Chunk>>) -> SuperviseCtx {
        SuperviseCtx {
            name: "test".into(),
            device: "raw:/dev/null".into(),
            resolver: Resolver::new("/"),
            params: OpenParams {
                baud: 115_200,
                data_bits: DataBits::Eight,
                parity: CfgParity::None,
                stop_bits: CfgStop::One,
                flow: CfgFlow::None,
                modem: ModemLines {
                    dtr: None,
                    rts: None,
                },
                purge_on_reconnect: true,
            },
            hostward: Vec::new(),
            targetward,
            shared: Rc::new(CriticalCell::new(SerialShared {
                status: NodeStatus::Waiting {
                    reason: "test".into(),
                },
                port: None,
            })),
            reader_slot: Rc::new(CriticalCell::new(BlockingReader::default())),
            discarded: Arc::new(AtomicU64::new(0)),
            purged: Arc::new(AtomicU64::new(0)),
        }
    }

    /// XC-PURGE-1: purge-on-reconnect must also drain a chunk an origin is blocked
    /// mid-`send` behind the full channel — otherwise it fires into the reopened
    /// device on the first post-reconnect `recv`. The count must stay exact.
    #[test]
    fn purge_on_reconnect_drains_backpressured_in_flight_chunk() {
        // A current-thread runtime + LocalSet mirrors the daemon (single-threaded,
        // cooperative): a producer blocked in `send().await` resolves only when the
        // purge yields, which is exactly what the fix must do.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            // A capacity-2 targetward channel, filled so a third send backpressures.
            let (tx, rx) = mpsc::channel::<Chunk>(2);
            tx.send(Chunk::copy_from_slice(b"AAA")).await.unwrap(); // 3 bytes
            tx.send(Chunk::copy_from_slice(b"BBBB")).await.unwrap(); // 4 bytes
            // A backpressured origin: suspended inside `send().await` holding one
            // already-read, outage-era chunk (§5). Keep `tx` alive so the channel
            // stays connected, as real origins persist across a reopen.
            let tx2 = tx.clone();
            let producer = tokio::task::spawn_local(async move {
                tx2.send(Chunk::copy_from_slice(b"CCCCC")).await.unwrap(); // 5 bytes
            });
            tokio::task::yield_now().await; // let the producer reach its blocked send
            assert!(!producer.is_finished(), "producer must be blocked in send");

            let mut ctx = test_ctx(Some(rx));
            purge_on_reconnect(&mut ctx).await;

            // The blocked send resolved and was drained+counted — not left to fire
            // into the reopened device — and the count is exact.
            assert!(producer.is_finished(), "blocked send must have resolved");
            assert_eq!(ctx.purged.load(Ordering::Relaxed), 3 + 4 + 5);
            // Nothing outage-era remains for the writer to send.
            let mut rx = ctx.targetward.take().unwrap();
            assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
            drop(tx);
        });
    }

    /// XC-PURGE-1 (empty-chunk guard): the drain loop must terminate on whether a
    /// chunk was *drained*, not on a byte-count delta — otherwise a round that
    /// drains only zero-length chunks reads as "no progress", breaks without
    /// yielding, and strands a backpressured non-empty chunk queued behind the
    /// empties (which then fires into the reopened device). A capacity-2 channel is
    /// filled with two 0-byte chunks; a 3rd non-empty send backpressures.
    #[test]
    fn purge_on_reconnect_drains_past_empty_chunks() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let (tx, rx) = mpsc::channel::<Chunk>(2);
            tx.send(Chunk::new()).await.unwrap(); // 0 bytes
            tx.send(Chunk::new()).await.unwrap(); // 0 bytes — channel now full
            let tx2 = tx.clone();
            let producer = tokio::task::spawn_local(async move {
                tx2.send(Chunk::copy_from_slice(b"CCCCC")).await.unwrap(); // 5 bytes
            });
            tokio::task::yield_now().await;
            assert!(!producer.is_finished(), "producer must be blocked in send");

            let mut ctx = test_ctx(Some(rx));
            purge_on_reconnect(&mut ctx).await;

            // The non-empty chunk behind the two empties resolved and was drained —
            // not stranded to fire into the reopened device — and only its 5 bytes
            // count (the empties contribute 0).
            assert!(
                producer.is_finished(),
                "the send behind the empty chunks must have resolved"
            );
            assert_eq!(ctx.purged.load(Ordering::Relaxed), 5);
            let mut rx = ctx.targetward.take().unwrap();
            assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
            drop(tx);
        });
    }

    /// SERIAL-3: when every hostward sink is permanently Closed (a consumer
    /// cascade-removed while this node lives), the bytes must be counted as
    /// unattached loss (§5), not dropped silently.
    #[test]
    fn all_sinks_closed_counts_discarded_on_surviving_producer() {
        use std::os::fd::AsRawFd;
        let (read_fd, write_fd) = nix::unistd::pipe().unwrap();
        let (tx, rx) = mpsc::channel::<Chunk>(4);
        drop(rx); // consumer gone: the sink is now permanently Closed.
        let counters = Arc::new(crate::runtime::DropCounters::default());
        let hostward: Vec<HostwardSink> = vec![(tx, counters.clone())];
        let discarded = Arc::new(AtomicU64::new(0));

        // Feed 5 bytes, then close the write end: the reader drains them, hits EOF,
        // and returns — deterministic, no polling race.
        nix::unistd::write(&write_fd, b"hello").unwrap();
        drop(write_fd);

        let rfd = read_fd.as_raw_fd();
        let discarded_thread = discarded.clone();
        std::thread::spawn(move || {
            reader_thread(
                rfd,
                hostward,
                discarded_thread,
                Arc::new(AtomicBool::new(false)),
                Arc::new(Notify::new()),
            );
        })
        .join()
        .unwrap();
        drop(read_fd); // the fd stays valid until the reader thread has joined.

        // The 5 bytes reached no live boundary → counted as unattached loss, and
        // the dead consumer is not mis-charged a full-buffer drop.
        assert_eq!(discarded.load(Ordering::Relaxed), 5);
        assert_eq!(counters.dropped_full(), 0);
    }
}
