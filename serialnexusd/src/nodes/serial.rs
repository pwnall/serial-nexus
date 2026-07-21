//! Serial port node (design §7.1). Faces host in the normal role.
//!
//! Slice 1 opened the device (raw path for now — the resolver lands in phase 7
//! without a config-format change), applied configured termios, and took
//! `TIOCEXCL` on the raw fd (serial2 sets `O_NOCTTY` but not `TIOCEXCL`, per the
//! nexus-doctor P3 finding). A missing device does not fail the load — the node
//! comes up `waiting` and heals later (§7.1 faulted-and-wait, phase 7).
//!
//! Byte flow: the blocking `serial2` fd is set non-blocking (for the drain loop)
//! and *not* driven by `serial2-tokio` (which hides the fd we need for
//! `TIOCEXCL`/`TIOCGICOUNT`) nor `tokio::io::unix::AsyncFd` (whose epoll readiness
//! busy-loops on pty masters, §15.18). The **hostward reader runs on a dedicated
//! blocking thread** — a `poll(2)` blocking wait wakes it the instant the device
//! has data, so it drains at line rate (a non-blocking poll-plus-sleep on the
//! runtime thread capped hostward throughput at ~1 MB/s) and costs zero CPU while
//! parked. This is §15.18's reserved "spawn_blocking reader threads" hatch, which
//! the phase-3 benchmark cashed and §15.19 made normative (the hybrid data plane).
//! It broadcasts to every attached consumer (lossy `try_send`, §5).
//! The low-rate **targetward writer** stays an async task draining the bounded
//! channel to the port (backpressure via the channel, §5); read and write on the
//! shared fd are independent directions.

use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle as ThreadHandle;

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::{
    DataBits, FlowControl as CfgFlow, ModemLines, NodeConfig, Parity as CfgParity,
    StopBits as CfgStop,
};
use nix::poll::PollFlags;
use serde_json::json;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::runtime::{self, HostwardSink, READ_BUF};
use crate::sys;

/// Re-arm interval (ms) for the serial reader thread's blocking readiness poll.
/// A live device wakes it far sooner; on teardown the thread observes its stop
/// flag within this bound, so `join` returns promptly.
const READ_POLL_TIMEOUT_MS: u16 = 200;

pub struct SerialNode {
    pub name: String,
    device: PathBuf,
    baud: u32,
    /// Initial modem-line assertions applied at open (§7.1), so line states are
    /// deterministic against auto-reset adapters. Retained so phase 7's reopen
    /// ritual can restore the configured lines after a replug.
    modem: ModemLines,
    /// The open port, retained for the targetward writer and future control verbs
    /// (modem lines, break — phase 7). `None` while `waiting`/`faulted`. The
    /// reader thread borrows only its raw fd, so the port must outlive the thread
    /// (teardown joins the thread before dropping it).
    port: Option<Rc<SerialPort>>,
    /// Bytes read from the port and discarded because nothing was attached to
    /// consume them (§5). Shared with the reader thread, hence atomic.
    discarded_unattached: Arc<AtomicU64>,
    /// Set on teardown to stop the reader thread at its next poll timeout.
    reader_stop: Arc<AtomicBool>,
    /// The dedicated hostward reader thread (§15.19).
    reader: Option<ThreadHandle<()>>,
    /// The async targetward writer task, if any.
    tasks: Vec<JoinHandle<()>>,
    /// Targetward receiver held **unread** while the port is `waiting`/`faulted`
    /// (none present). Retaining it — rather than draining it — lets the bounded
    /// channel fill and backpressure the origin (its `send().await` suspends, the
    /// client's kernel buffers), so a locked writer's commands are *delayed, never
    /// dropped* (§5). Draining-and-discarding here would silently vaporize them,
    /// violating the never-drop-targetward invariant; dropping the receiver would
    /// error the origin's send and exit its reader. Phase 7's reopen ritual hands
    /// this receiver to a fresh writer.
    parked_targetward: Option<mpsc::Receiver<Chunk>>,
    status: NodeStatus,
}

impl SerialNode {
    pub fn create(config: &NodeConfig) -> SerialNode {
        let NodeConfig::Serial {
            name,
            device,
            baud,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            modem,
            ..
        } = config
        else {
            unreachable!("SerialNode::create called with non-Serial config");
        };

        let mut node = SerialNode {
            name: name.clone(),
            device: PathBuf::from(device),
            baud: *baud,
            modem: *modem,
            port: None,
            discarded_unattached: Arc::new(AtomicU64::new(0)),
            reader_stop: Arc::new(AtomicBool::new(false)),
            reader: None,
            tasks: Vec::new(),
            parked_targetward: None,
            status: NodeStatus::Active,
        };

        match open_port(
            &node.device,
            *baud,
            *data_bits,
            *parity,
            *stop_bits,
            *flow_control,
            node.modem,
        ) {
            Ok(port) => {
                node.port = Some(Rc::new(port));
                node.status = NodeStatus::Active;
            }
            // A device that isn't present yet is `waiting` (it will heal when it
            // reappears, §7.1); any other open error is `faulted`.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                node.status = NodeStatus::Waiting {
                    reason: format!("device {} not present", node.device.display()),
                };
            }
            Err(e) => {
                node.status = NodeStatus::Faulted {
                    reason: format!("open {}: {e}", node.device.display()),
                };
            }
        }
        node
    }

    /// Start the data plane for this port: broadcast hostward to the attached
    /// PTYs, drain targetward from them. A `waiting`/`faulted` port (none present)
    /// *parks* its targetward receiver unread, so the bounded channel fills and
    /// backpressures the origin (§5: commands are delayed, never dropped) rather
    /// than draining-and-discarding them.
    pub fn start(
        &mut self,
        hostward: Vec<HostwardSink>,
        targetward: Option<mpsc::Receiver<Chunk>>,
    ) {
        let Some(port) = self.port.clone() else {
            // Hold the receiver without reading it: a full channel suspends the
            // origin's send().await (§5 backpressure). Dropping it instead would
            // error the origin's send and exit its reader; draining it would
            // silently discard commands. Phase 7's reopen hands this on.
            self.parked_targetward = targetward;
            return;
        };

        if let Err(e) = sys::set_nonblocking(port.as_raw_fd()) {
            self.status = NodeStatus::Faulted {
                reason: format!("set_nonblocking: {e}"),
            };
            self.port = None;
            return;
        }

        // Hostward: a dedicated thread reads the device continuously (blocking
        // poll → line rate, §15.19) and broadcasts to every attached consumer.
        // The read happens whether or not a client is attached downstream — a
        // full consumer channel drops at ingest, never here (§5).
        let fd = port.as_raw_fd();
        let stop = self.reader_stop.clone();
        let discarded = self.discarded_unattached.clone();
        self.reader = Some(
            std::thread::Builder::new()
                .name(format!("serial-rx-{}", self.name))
                .spawn(move || reader_thread(fd, hostward, discarded, stop))
                .expect("spawn serial reader thread"),
        );

        // Targetward: drain the bounded channel to the port at line rate; a full
        // channel backpressures to the origin (§5). Human-scale command entry, so
        // it stays an async task.
        if let Some(mut rx) = targetward {
            let port_w = port;
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(chunk) = rx.recv().await {
                    if runtime::write_all(port_w.as_raw_fd(), &chunk)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }

    /// Stop the reader thread and join it within the poll-timeout bound.
    fn stop_reader(&mut self) {
        self.reader_stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        // Driver input counters (TIOCGICOUNT) surface framing/parity/overrun loss
        // "where supported" (§5, §7.1); a pts (test device) returns an error, so
        // report null rather than faulting or fabricating zeros.
        let driver_counters = self
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
        json!({
            "resolved_path": self.device.display().to_string(),
            "baud": self.baud,
            "open": self.port.is_some(),
            "discarded_unattached": self.discarded_unattached.load(Ordering::Relaxed),
            "driver_counters": driver_counters,
        })
    }

    /// Stop the reader thread and the targetward task, then drop the port. The
    /// reader is joined *before* the port drops, so its fd stays valid throughout
    /// (avoiding an fd-reuse race). Called on teardown/shutdown.
    pub fn teardown(&mut self) {
        self.stop_reader();
        for t in self.tasks.drain(..) {
            t.abort();
        }
        // Drop any parked targetward receiver so its channel closes and a
        // backpressured origin's send() unblocks (with an error) rather than
        // hanging past teardown.
        drop(self.parked_targetward.take());
        self.port = None;
    }
}

/// Hostward reader thread (§15.19): a blocking `poll(2)` waits for readability —
/// waking the instant the device has data (line rate) and parking at zero CPU
/// otherwise — then the loop drains fully and broadcasts each chunk to every
/// attached consumer. Exits on device close, stop flag, or a fatal error (phase 7
/// restructures this into faulted-and-wait with re-open).
///
/// Loss is always counted at the boundary that drops it: with nothing attached,
/// bytes are discarded against `discarded_unattached` (§5, so a first consumer
/// starts on fresh data, not a stale kernel burst); with a consumer attached but
/// its bounded buffer full, the drop is counted against that consumer's
/// [`DropCounters`] (a slow spy costs only itself).
fn reader_thread(
    fd: std::os::fd::RawFd,
    hostward: Vec<HostwardSink>,
    discarded_unattached: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
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
                    Ok(0) => return, // device closed
                    Ok(n) => {
                        if hostward.is_empty() {
                            // Nothing attached: read-and-discard with a counter.
                            discarded_unattached.fetch_add(n as u64, Ordering::Relaxed);
                            continue;
                        }
                        let chunk = Chunk::copy_from_slice(&buf[..n]);
                        for (tx, counters) in &hostward {
                            match tx.try_send(chunk.clone()) {
                                Ok(()) => {}
                                // Slow consumer: its bounded buffer is full.
                                Err(TrySendError::Full(_)) => counters.add_full(n as u64),
                                // Receiver gone (teardown): not a boundary drop.
                                Err(TrySendError::Closed(_)) => {}
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => return,
                }
            }
        } else if re.contains(PollFlags::POLLHUP) {
            return; // device gone
        }
        // Bare timeout (empty revents): re-check the stop flag and re-arm.
    }
}

impl Drop for SerialNode {
    fn drop(&mut self) {
        self.stop_reader();
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }
}

fn open_port(
    device: &std::path::Path,
    baud: u32,
    data_bits: DataBits,
    parity: CfgParity,
    stop_bits: CfgStop,
    flow: CfgFlow,
    modem: ModemLines,
) -> std::io::Result<SerialPort> {
    let port = SerialPort::open(device, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(baud)?;
        s.set_char_size(char_size(data_bits));
        s.set_parity(map_parity(parity));
        s.set_stop_bits(map_stop(stop_bits));
        s.set_flow_control(map_flow(flow));
        Ok(s)
    })?;
    // serial2 does not take TIOCEXCL; the daemon does, so stray processes cannot
    // share the port (§7.1, P3 finding).
    sys::set_exclusive(port.as_raw_fd(), true)
        .map_err(|e| std::io::Error::other(format!("TIOCEXCL: {e}")))?;
    // Initial modem-line assertions (§7.1): a line left unset (None) keeps the
    // driver's power-on state; an asserted/deasserted line is made deterministic
    // here (and reapplied by phase 7's reopen ritual against auto-reset adapters).
    if let Some(dtr) = modem.dtr {
        port.set_dtr(dtr)
            .map_err(|e| std::io::Error::other(format!("set DTR: {e}")))?;
    }
    if let Some(rts) = modem.rts {
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
