//! PTY node (design §7.2). Faces target.
//!
//! Slice 1 built the pair: allocate the master/slave, set the baseline termios
//! (raw, echo off, EXTPROC on), enable packet mode on the master, install the
//! configured symlink (with the stale-dangling-symlink recovery rule), apply
//! owner/mode to the slave device node, and *prime* the slave by opening and
//! closing it once so POLLHUP reports "absent" for the never-opened case
//! (nexus-doctor P2 finding).
//!
//! Byte flow and presence (never `AsyncFd`, whose epoll readiness busy-loops on
//! pty masters — §15.18):
//!
//! * A read+presence **async task** polls the master (`POLLIN | POLLHUP`,
//!   non-blocking) with an ACTIVE_POLL→IDLE_POLL backoff: `POLLHUP` set ⇒ no
//!   client; `POLLIN` ⇒ drain, strip the packet-mode control byte, forward only
//!   `TIOCPKT_DATA` payloads targetward, and reconcile client termios on a
//!   `TIOCPKT_IOCTL` packet (§7.2). This side is human-scale command entry. On
//!   last close it re-asserts the baseline termios (§7.2).
//! * Hostward delivery is a dedicated **blocking writer thread** so a fast
//!   consumer receives at line rate (§15.18), fed by an async pump through a
//!   bounded bridge (full-buffer drops counted there, §5). It is **presence-
//!   gated** — written only while a client holds the slave, discarded-with-count
//!   otherwise (§7.2).

use std::cell::RefCell;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle as ThreadHandle;
use std::time::{Duration, Instant};

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nix::fcntl::{OFlag, open};
use nix::libc;
use nix::poll::PollFlags;
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{
    BaudRate, ControlFlags, LocalFlags, SetArg, cfgetospeed, cfmakeraw, cfsetspeed, tcgetattr,
    tcsetattr,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// How often the reconciliation poll re-reads client termios as a backstop for
/// the packet-mode notification, catching the mechanism's obscure corners (§7.2).
/// A few seconds; one ioctl, effectively free.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(3);

/// Depth (in chunks) of the bounded bridge between the async pump and the blocking
/// writer thread. This is the PTY boundary's buffer for a slow consumer (§5):
/// bounded, so memory can't grow, and full-buffer drops are counted where they
/// happen. At [`READ_BUF`]-sized chunks this is ~2 MiB.
const WRITER_QUEUE: usize = 32;

use crate::runtime::{ACTIVE_POLL, DropCounters, READ_BUF, back_off};
use crate::sys;

pub struct PtyNode {
    pub name: String,
    path: PathBuf,
    mode: u32,
    owner: Option<String>,
    group: Option<String>,
    advertised_baud: u32,
    /// The master, shared between the read+presence and writer tasks. `None`
    /// once torn down (or if setup faulted).
    master: Option<Rc<PtyMaster>>,
    pts_path: Option<String>,
    symlink_installed: bool,
    /// Observed client presence. Shared between the async reader task (which sets
    /// it) and the blocking hostward writer thread (which gates on it), hence
    /// atomic.
    present: Arc<AtomicBool>,
    /// The blocking hostward writer thread (§15.18): a fast consumer receives at
    /// line rate rather than the poll path's ~1 MB/s.
    writer: Option<ThreadHandle<()>>,
    /// Set on teardown to stop the writer thread at its next recv timeout — the
    /// sync teardown can't rely on the aborted async pump dropping its sender,
    /// since the runtime isn't running while teardown blocks on the join.
    writer_stop: Arc<AtomicBool>,
    /// Hostward drop counters for this boundary (§5), shared with the serial
    /// reader. Reports presence-gated discards and slow-consumer full-buffer
    /// drops in state. Defaulted at creation, replaced from the wiring at start.
    counters: Arc<DropCounters>,
    /// Observed client termios (§7.2), updated by the reader on a packet-mode
    /// `tcsetattr` notification and by the slow reconciliation backstop; `None`
    /// while no client has touched the settings this session. The daemon only
    /// *observes* — propagation to hardware is deferred (§14).
    client_termios: Rc<RefCell<Option<Value>>>,
    tasks: Vec<JoinHandle<()>>,
    status: NodeStatus,
}

impl PtyNode {
    pub fn create(config: &NodeConfig) -> PtyNode {
        let NodeConfig::Pty {
            name,
            path,
            owner,
            group,
            mode,
            advertised_baud,
            ..
        } = config
        else {
            unreachable!("PtyNode::create called with non-Pty config");
        };

        // Default 0600; 0660 when a group is configured (§7.2).
        let default_mode = if group.is_some() { 0o660 } else { 0o600 };
        let mut node = PtyNode {
            name: name.clone(),
            path: PathBuf::from(path),
            mode: mode.unwrap_or(default_mode),
            owner: owner.clone(),
            group: group.clone(),
            advertised_baud: *advertised_baud,
            master: None,
            pts_path: None,
            symlink_installed: false,
            present: Arc::new(AtomicBool::new(false)),
            writer: None,
            writer_stop: Arc::new(AtomicBool::new(false)),
            counters: Arc::new(DropCounters::default()),
            client_termios: Rc::new(RefCell::new(None)),
            tasks: Vec::new(),
            status: NodeStatus::Active,
        };

        node.status = match node.setup() {
            Ok(()) => NodeStatus::Active,
            Err(reason) => NodeStatus::Faulted { reason },
        };
        node
    }

    fn setup(&mut self) -> Result<(), String> {
        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)
            .map_err(|e| format!("posix_openpt: {e}"))?;
        grantpt(&master).map_err(|e| format!("grantpt: {e}"))?;
        unlockpt(&master).map_err(|e| format!("unlockpt: {e}"))?;
        let pts = ptsname_r(&master).map_err(|e| format!("ptsname: {e}"))?;

        apply_baseline(master.as_fd(), self.advertised_baud)?;
        sys::set_packet_mode(master.as_raw_fd(), true).map_err(|e| format!("TIOCPKT: {e}"))?;

        self.install_symlink(&pts)?;
        self.symlink_installed = true;
        self.apply_perms(&pts)?;
        prime_slave(&pts);

        self.master = Some(Rc::new(master));
        self.pts_path = Some(pts);
        Ok(())
    }

    /// Install the configured symlink to the pts node. A pre-existing path
    /// faults the node — except a symlink dangling into devpts, presumed our
    /// stale artifact from a crash, which is silently replaced (§7.2).
    fn install_symlink(&self, pts: &str) -> Result<(), String> {
        if let Ok(meta) = std::fs::symlink_metadata(&self.path) {
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&self.path).unwrap_or_default();
                let dangling_into_devpts = target.starts_with("/dev/pts") && !target.exists();
                if dangling_into_devpts {
                    let _ = std::fs::remove_file(&self.path);
                } else {
                    return Err(format!(
                        "symlink path {} already exists",
                        self.path.display()
                    ));
                }
            } else {
                return Err(format!("path {} already exists", self.path.display()));
            }
        }
        symlink(pts, &self.path).map_err(|e| format!("symlink {}: {e}", self.path.display()))
    }

    /// Apply mode, owner, and group to the slave device node — what gates
    /// open(2) (§7.2). A configured owner/group that cannot be resolved or
    /// applied faults the node (an environmental failure, §15.8) rather than
    /// being silently dropped; leaving either unset keeps the daemon-user
    /// default the freshly allocated pts already has.
    fn apply_perms(&self, pts: &str) -> Result<(), String> {
        std::fs::set_permissions(pts, std::os::unix::fs::PermissionsExt::from_mode(self.mode))
            .map_err(|e| format!("chmod {pts}: {e}"))?;

        let uid = match &self.owner {
            Some(owner) => Some(
                nix::unistd::User::from_name(owner)
                    .ok()
                    .flatten()
                    .map(|u| u.uid)
                    .ok_or_else(|| format!("user {owner} not found"))?,
            ),
            None => None,
        };
        let gid = match &self.group {
            Some(group) => Some(
                nix::unistd::Group::from_name(group)
                    .ok()
                    .flatten()
                    .map(|g| g.gid)
                    .ok_or_else(|| format!("group {group} not found"))?,
            ),
            None => None,
        };
        if uid.is_some() || gid.is_some() {
            nix::unistd::chown(pts, uid, gid).map_err(|e| format!("chown {pts}: {e}"))?;
        }
        Ok(())
    }

    /// Start the data plane: forward targetward, poll presence, and drain
    /// hostward presence-gated.
    pub fn start(
        &mut self,
        hostward: Option<mpsc::Receiver<Chunk>>,
        targetward: Option<mpsc::Sender<Chunk>>,
        counters: Option<Arc<DropCounters>>,
    ) {
        // Adopt the wiring's shared counters (the same Rc the serial reader
        // increments) so presence-gated discards and full-buffer drops land on
        // one instance this node reports from (§5).
        if let Some(counters) = counters {
            self.counters = counters;
        }
        let Some(master) = self.master.clone() else {
            return; // setup faulted; nothing to drive
        };
        if let Err(e) = sys::set_nonblocking(master.as_raw_fd()) {
            self.status = NodeStatus::Faulted {
                reason: format!("set_nonblocking: {e}"),
            };
            return;
        }

        // Reader + presence poll: client → serial (targetward), plus the presence
        // check that also gates the writer below and the client-termios
        // reconciliation that feeds state (§7.2).
        self.tasks.push(tokio::task::spawn_local(read_and_poll(
            master.clone(),
            self.present.clone(),
            self.client_termios.clone(),
            targetward,
            self.advertised_baud,
        )));

        // Writer: serial → client (hostward), presence-gated, on a dedicated
        // blocking thread so a fast consumer receives at line rate (§15.18). An
        // async pump bridges the hostward channel to a std channel the thread
        // blocks on; aborting the pump on teardown drops its sender, which unblocks
        // and ends the thread (std recv returns Err once every sender is gone).
        if let Some(mut rx) = hostward {
            // Bounded bridge: the pump moves chunks into it and drops-with-count
            // when it is full (a slow consumer shedding at its own boundary, §5),
            // so the writer thread's blocking recv can also observe the stop flag.
            let (btx, brx) = std_mpsc::sync_channel::<Chunk>(WRITER_QUEUE);
            let pump_counters = self.counters.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(chunk) = rx.recv().await {
                    let len = chunk.len() as u64;
                    match btx.try_send(chunk) {
                        Ok(()) => {}
                        Err(std_mpsc::TrySendError::Full(_)) => pump_counters.add_full(len),
                        Err(std_mpsc::TrySendError::Disconnected(_)) => break,
                    }
                }
            }));
            let fd = master.as_raw_fd();
            let present = self.present.clone();
            let counters = self.counters.clone();
            let stop = self.writer_stop.clone();
            self.writer = Some(
                std::thread::Builder::new()
                    .name(format!("pty-tx-{}", self.name))
                    .spawn(move || writer_thread(fd, present, counters, stop, brx))
                    .expect("spawn pty writer thread"),
            );
        }
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        json!({
            "pts_path": self.pts_path,
            "symlink": self.path.display().to_string(),
            "advertised_baud": self.advertised_baud,
            "client_present": self.present.load(Ordering::Relaxed),
            // Hostward drops at this boundary (§5): bytes discarded while no
            // client held the slave, and bytes dropped because the client was
            // too slow to drain its bounded buffer.
            "discarded_no_client": self.counters.discarded_absent(),
            "dropped_slow_consumer": self.counters.dropped_full(),
            // Observed client termios (§7.2), null until a client touches it.
            "client_termios": self.client_termios.borrow().clone(),
        })
    }

    pub fn teardown(&mut self) {
        // Signal the writer thread to stop, abort the async tasks, then join the
        // thread before dropping the master so its fd stays valid throughout. The
        // stop flag (not the pump's sender drop) is what ends the thread, since
        // the runtime can't run the pump to completion while teardown blocks here.
        self.writer_stop.store(true, Ordering::Relaxed);
        for t in self.tasks.drain(..) {
            t.abort();
        }
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        self.master = None;
        if self.symlink_installed {
            let _ = std::fs::remove_file(&self.path);
            self.symlink_installed = false;
        }
    }
}

impl Drop for PtyNode {
    fn drop(&mut self) {
        self.writer_stop.store(true, Ordering::Relaxed);
        for t in self.tasks.drain(..) {
            t.abort();
        }
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        // The symlink is our artifact; unlink it on removal / clean shutdown.
        if self.symlink_installed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Baseline termios (§7.2): raw + echo off + EXTPROC on, applied through the
/// master, plus the cosmetic advertised baud. Free so both creation and the
/// last-close reset can call it against a borrowed master fd.
fn apply_baseline(fd: BorrowedFd, advertised_baud: u32) -> Result<(), String> {
    let mut t = tcgetattr(fd).map_err(|e| format!("tcgetattr: {e}"))?;
    cfmakeraw(&mut t);
    t.local_flags.remove(LocalFlags::ECHO);
    t.local_flags.insert(LocalFlags::EXTPROC);
    if let Some(baud) = standard_baud(advertised_baud) {
        let _ = cfsetspeed(&mut t, baud);
    }
    tcsetattr(fd, SetArg::TCSANOW, &t).map_err(|e| format!("tcsetattr: {e}"))
}

/// Reader + presence task. Polls the master (non-blocking) for readability and
/// hangup; drains data targetward stripping the packet-mode control byte; and on
/// last close re-asserts the baseline termios so the next session starts
/// deterministic (§7.2). Sleeps only when idle, so an active transfer streams.
async fn read_and_poll(
    master: Rc<PtyMaster>,
    present: Arc<AtomicBool>,
    client_termios: Rc<RefCell<Option<Value>>>,
    tx: Option<mpsc::Sender<Chunk>>,
    advertised_baud: u32,
) {
    let fd = master.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF];
    let mut last_reconcile = Instant::now();
    let mut wait = ACTIVE_POLL;
    loop {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let now = !re.contains(PollFlags::POLLHUP);
        let was = present.swap(now, Ordering::Relaxed);
        if was && !now {
            // Last close: reset to baseline for a deterministic next session and
            // forget the departed client's settings (§7.2). No client is open.
            let _ = apply_baseline(master.as_fd(), advertised_baud);
            *client_termios.borrow_mut() = None;
        }

        let mut did = false;
        if now && re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        present.store(false, Ordering::Relaxed);
                        break;
                    }
                    Ok(n) if n >= 1 => {
                        did = true;
                        if buf[0] == sys::TIOCPKT_DATA {
                            // Data packet: forward the payload targetward.
                            if n > 1
                                && let Some(tx) = &tx
                            {
                                let payload = Chunk::copy_from_slice(&buf[1..n]);
                                // Targetward: backpressure to the origin (await).
                                if tx.send(payload).await.is_err() {
                                    return; // serial gone
                                }
                            }
                        } else if buf[0] & sys::TIOCPKT_IOCTL != 0 {
                            // A client called tcsetattr: reconcile its termios
                            // into state and re-assert EXTPROC if it was cleared
                            // (§7.2). TIOCPKT alone would miss baud changes.
                            reconcile_termios(&master, &client_termios);
                            last_reconcile = Instant::now();
                        }
                        // Other control packets (flush, flow-control) carry no
                        // data and need no action here.
                    }
                    Ok(_) => break, // zero-length non-EOF read: nothing to do
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    // A client that closed surfaces as EIO; mark absent.
                    Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                        present.store(false, Ordering::Relaxed);
                        break;
                    }
                    Err(_) => break,
                }
            }
        }

        // Slow reconciliation backstop (§7.2): while a client is present, re-read
        // termios periodically so a missed packet-mode notification still lands.
        if now && last_reconcile.elapsed() >= RECONCILE_INTERVAL {
            reconcile_termios(&master, &client_termios);
            last_reconcile = Instant::now();
        }

        // Active transfer rechecks promptly (ACTIVE_POLL); a truly idle master
        // backs off toward IDLE_POLL — well under the §7.2 sub-second presence
        // requirement (§15.18).
        if did {
            wait = ACTIVE_POLL;
        } else {
            tokio::time::sleep(wait).await;
            back_off(&mut wait);
        }
    }
}

/// Read the client's current termios through the master and store it in state;
/// re-assert EXTPROC if the client cleared it, so the daemon keeps observing
/// subsequent changes (§7.2). Observe-only — never propagated to hardware (§14).
fn reconcile_termios(master: &PtyMaster, store: &RefCell<Option<Value>>) {
    let Ok(t) = tcgetattr(master) else { return };
    if !t.local_flags.contains(LocalFlags::EXTPROC) {
        let mut re = t.clone();
        re.local_flags.insert(LocalFlags::EXTPROC);
        let _ = tcsetattr(master, SetArg::TCSANOW, &re);
    }
    let char_bits = match t.control_flags & ControlFlags::CSIZE {
        cs if cs == ControlFlags::CS8 => 8,
        cs if cs == ControlFlags::CS7 => 7,
        cs if cs == ControlFlags::CS6 => 6,
        _ => 5,
    };
    let parity = if !t.control_flags.contains(ControlFlags::PARENB) {
        "none"
    } else if t.control_flags.contains(ControlFlags::PARODD) {
        "odd"
    } else {
        "even"
    };
    *store.borrow_mut() = Some(json!({
        "baud": format!("{:?}", cfgetospeed(&t)),
        "char_bits": char_bits,
        "parity": parity,
        "echo": t.local_flags.contains(LocalFlags::ECHO),
        "icanon": t.local_flags.contains(LocalFlags::ICANON),
        "extproc": t.local_flags.contains(LocalFlags::EXTPROC),
    }));
}

/// Blocking hostward writer thread (§15.18): drain the bridged channel to the
/// master while a client is present — a blocking write delivers at line rate,
/// where the poll path capped a fast consumer at ~1 MB/s — and discard otherwise
/// (§7.2 presence-gated output; the discard is counted so loss stays visible, §5).
/// Exits when the stop flag is set or the bridge sender is gone.
fn writer_thread(
    fd: std::os::fd::RawFd,
    present: Arc<AtomicBool>,
    counters: Arc<DropCounters>,
    stop: Arc<AtomicBool>,
    rx: std_mpsc::Receiver<Chunk>,
) {
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                if present.load(Ordering::Relaxed) {
                    if blocking_write_all(fd, &chunk).is_err() {
                        // Peer hung up mid-write; presence will flip and the next
                        // chunks are discarded-and-counted until a client returns.
                    }
                } else {
                    counters.add_absent(chunk.len() as u64);
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Write every byte of `data` to the master, blocking the *thread* (not the async
/// runtime) on `poll(2)` for writability between partial writes — line rate for a
/// fast consumer, no busy loop. `Err` means the peer hung up.
fn blocking_write_all(fd: std::os::fd::RawFd, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        match sys::write_fd(fd, data) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let re = sys::poll_blocking(fd, PollFlags::POLLOUT | PollFlags::POLLHUP, 500);
                if re.contains(PollFlags::POLLHUP) && !re.contains(PollFlags::POLLOUT) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Open then immediately close the slave once, priming the master's HUP state to
/// "absent" (nexus-doctor P2: a never-opened master does not report POLLHUP).
fn prime_slave(pts: &str) {
    if let Ok(fd) = open(pts, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()) {
        drop(fd);
    }
}

/// Map a baud to the nearest standard `BaudRate` for the cosmetic advertised
/// speed; `None` if not a standard rate (advertised baud is cosmetic on a PTY).
fn standard_baud(baud: u32) -> Option<BaudRate> {
    Some(match baud {
        9600 => BaudRate::B9600,
        19200 => BaudRate::B19200,
        38400 => BaudRate::B38400,
        57600 => BaudRate::B57600,
        115200 => BaudRate::B115200,
        230400 => BaudRate::B230400,
        460800 => BaudRate::B460800,
        921600 => BaudRate::B921600,
        _ => return None,
    })
}
