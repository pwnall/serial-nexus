//! Serial port node (design §7.1). Faces host in the normal role.
//!
//! Slice 1 opened the device (raw path for now — the resolver lands in phase 7
//! without a config-format change), applied configured termios, and took
//! `TIOCEXCL` on the raw fd (serial2 sets `O_NOCTTY` but not `TIOCEXCL`, per the
//! nexus-doctor P3 finding). A missing device does not fail the load — the node
//! comes up `waiting` and heals later (§7.1 faulted-and-wait, phase 7).
//!
//! Slice 2 (this) drives byte flow: the blocking `serial2` fd is set
//! non-blocking and driven by poll-based readiness (`sys::poll_ready`), *not*
//! `serial2-tokio` (which hides the fd we need for `TIOCEXCL` and, later,
//! `TIOCGICOUNT`) and *not* `tokio::io::unix::AsyncFd` (whose epoll readiness
//! busy-loops on pty masters, implementation notes §3.10). A reader task
//! broadcasts hostward to every attached PTY (lossy `try_send`, §5); a writer
//! task drains the single targetward channel to the port (backpressure via the
//! bounded channel, §5).

use std::cell::Cell;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::rc::Rc;

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::{
    DataBits, FlowControl as CfgFlow, NodeConfig, Parity as CfgParity, StopBits as CfgStop,
};
use nix::poll::PollFlags;
use serde_json::json;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::task::JoinHandle;

use crate::runtime::{self, HostwardSink, IDLE_POLL, READ_BUF};
use crate::sys;

pub struct SerialNode {
    pub name: String,
    device: PathBuf,
    baud: u32,
    /// The open port, shared between the reader and writer tasks and retained
    /// for future control verbs (modem lines, break — phase 7). `None` while
    /// `waiting`/`faulted`.
    port: Option<Rc<SerialPort>>,
    /// Bytes read from the port and discarded because nothing was attached to
    /// consume them (§5 discard-when-unattached). Shared with the reader task.
    discarded_unattached: Rc<Cell<u64>>,
    tasks: Vec<JoinHandle<()>>,
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
            ..
        } = config
        else {
            unreachable!("SerialNode::create called with non-Serial config");
        };

        let mut node = SerialNode {
            name: name.clone(),
            device: PathBuf::from(device),
            baud: *baud,
            port: None,
            discarded_unattached: Rc::new(Cell::new(0)),
            tasks: Vec::new(),
            status: NodeStatus::Active,
        };

        match open_port(
            &node.device,
            *baud,
            *data_bits,
            *parity,
            *stop_bits,
            *flow_control,
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
    /// PTYs, drain targetward from them. A `waiting`/`faulted` port (none
    /// present) still drains its targetward channel so paused PTY writers don't
    /// spin on a dropped receiver.
    pub fn start(
        &mut self,
        hostward: Vec<HostwardSink>,
        targetward: Option<mpsc::Receiver<Chunk>>,
    ) {
        let Some(port) = self.port.clone() else {
            if let Some(mut rx) = targetward {
                self.tasks.push(tokio::task::spawn_local(async move {
                    while rx.recv().await.is_some() {}
                }));
            }
            return;
        };

        if let Err(e) = sys::set_nonblocking(port.as_raw_fd()) {
            self.status = NodeStatus::Faulted {
                reason: format!("set_nonblocking: {e}"),
            };
            self.port = None;
            return;
        }

        // Hostward: read the device continuously and broadcast to every attached
        // PTY (§5). The read happens whether or not a client is attached
        // downstream — a full PTY channel drops at ingest, never here.
        let port_r = port.clone();
        self.tasks.push(tokio::task::spawn_local(read_hostward(
            port_r,
            hostward,
            self.discarded_unattached.clone(),
        )));

        // Targetward: drain the bounded channel to the port at line rate; a full
        // channel backpressures to the origin (§5).
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
            "discarded_unattached": self.discarded_unattached.get(),
            "driver_counters": driver_counters,
        })
    }

    /// Stop the data-plane tasks and drop the port. Called on teardown/shutdown.
    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
        self.port = None;
    }
}

/// Hostward reader: poll the device for data, drain it fully, and broadcast each
/// chunk to every attached PTY (lossy `try_send`, §5). Exits on device close —
/// phase 7 restructures this into faulted-and-wait with re-open.
///
/// Loss is always counted at the boundary that drops it: with nothing attached,
/// the bytes are discarded against `discarded_unattached` (§5, so a first
/// consumer starts on fresh data rather than a stale kernel burst); with a
/// consumer attached but its bounded buffer full, the drop is counted against
/// that consumer's [`DropCounters`] (a slow spy costs only itself).
async fn read_hostward(
    port: Rc<SerialPort>,
    hostward: Vec<HostwardSink>,
    discarded_unattached: Rc<Cell<u64>>,
) {
    let fd = port.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF];
    loop {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let mut did = false;
        if re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => return, // device closed
                    Ok(n) => {
                        did = true;
                        if hostward.is_empty() {
                            // Nothing attached: read-and-discard with a counter.
                            discarded_unattached.set(discarded_unattached.get() + n as u64);
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
        if !did {
            tokio::time::sleep(IDLE_POLL).await;
        }
    }
}

impl Drop for SerialNode {
    fn drop(&mut self) {
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
