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
//!   consumer receives at line rate (§15.19), fed by an async pump through a
//!   bounded bridge (full-buffer drops counted there, §5). It is **presence-
//!   gated** — written only while a client holds the slave, discarded-with-count
//!   otherwise (§7.2).

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
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
use nix::pty::{PtyMaster, grantpt, posix_openpt, unlockpt};
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

// The bounded bridge between the async pump and the blocking writer thread is the
// PTY boundary's buffer for a slow consumer (§5): bounded, so memory can't grow,
// and full-buffer drops are counted where they happen. Its depth is the node's
// configured `hostward_buffer` (§7.2), defaulting to 32 chunks (~2 MiB at
// [`READ_BUF`]-sized chunks) in `nexus_core::config`.

use nexus_core::lock::{Arbitration, OriginId, WriteMode};

use crate::cell::CriticalCell;
use crate::runtime::{ACTIVE_POLL, DropCounters, READ_BUF, SharedLock, back_off};
use nexus_sys as sys;

pub struct PtyNode {
    pub name: String,
    path: PathBuf,
    mode: u32,
    owner: Option<String>,
    group: Option<String>,
    advertised_baud: u32,
    /// Hostward drop policy (§5, §7.2): the depth (in chunks) of this PTY's writer
    /// bridge, past which a slow client's bytes are dropped-with-counters.
    hostward_buffer: usize,
    /// The master, shared between the read+presence and writer tasks. `None`
    /// once torn down (or if setup faulted).
    master: Option<Rc<PtyMaster>>,
    pts_path: Option<String>,
    symlink_installed: bool,
    /// Observed client presence. Shared between the async reader task (which sets
    /// it) and the blocking hostward writer thread (which gates on it), hence
    /// atomic.
    present: Arc<AtomicBool>,
    /// The blocking hostward writer thread (§15.19): a fast consumer receives at
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
    client_termios: Rc<CriticalCell<Option<Value>>>,
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
            hostward_buffer,
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
            hostward_buffer: *hostward_buffer,
            master: None,
            pts_path: None,
            symlink_installed: false,
            present: Arc::new(AtomicBool::new(false)),
            writer: None,
            writer_stop: Arc::new(AtomicBool::new(false)),
            counters: Arc::new(DropCounters::default()),
            client_termios: Rc::new(CriticalCell::new(None)),
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
        let pts = sys::ptsname(&master).map_err(|e| format!("ptsname: {e}"))?;

        apply_baseline(master.as_fd(), self.advertised_baud)?;
        sys::set_packet_mode(master.as_raw_fd(), true).map_err(|e| format!("TIOCPKT: {e}"))?;

        self.install_symlink(&pts)?;
        self.symlink_installed = true;
        self.apply_perms(&pts)?;
        prime_slave(&pts)?;

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
    /// hostward presence-gated. `lock` is this PTY's write-arbitration handle
    /// (§6) when it is a writing origin — the targetward drain is gated on it, so
    /// a non-holder is simply not read from. `None` for a `never` spy, which
    /// never writes.
    pub fn start(
        &mut self,
        hostward: Option<mpsc::Receiver<Chunk>>,
        targetward: Option<mpsc::Sender<Chunk>>,
        counters: Option<Arc<DropCounters>>,
        lock: Option<(SharedLock, OriginId)>,
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
            lock,
        )));

        // Writer: serial → client (hostward), presence-gated, on a dedicated
        // blocking thread so a fast consumer receives at line rate (§15.19). An
        // async pump bridges the hostward channel to a std channel the thread
        // blocks on; aborting the pump on teardown drops its sender, which unblocks
        // and ends the thread (std recv returns Err once every sender is gone).
        if let Some(mut rx) = hostward {
            // Bounded bridge: the pump moves chunks into it and drops-with-count
            // when it is full (a slow consumer shedding at its own boundary, §5),
            // so the writer thread's blocking recv can also observe the stop flag.
            // Depth is this PTY's configured hostward drop policy (§7.2).
            let (btx, brx) = std_mpsc::sync_channel::<Chunk>(self.hostward_buffer.max(1));
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
            match std::thread::Builder::new()
                .name(format!("pty-tx-{}", self.name))
                .spawn(move || writer_thread(fd, present, counters, stop, brx))
            {
                Ok(w) => self.writer = Some(w),
                Err(e) => {
                    // Thread/PID exhaustion (EAGAIN) is an environmental failure
                    // (§15.8): fault the node rather than panicking the runtime
                    // thread, matching setup()/apply_perms/the set_nonblocking path.
                    // The reader and pump tasks spawned above are aborted by teardown.
                    self.status = NodeStatus::Faulted {
                        reason: format!("spawn pty writer thread: {e}"),
                    };
                }
            }
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
            "client_termios": self.client_termios.with(|c| c.clone()),
        })
    }

    /// Drain and discard this origin's pre-grant targetward backlog from its
    /// master, returning the count of data bytes discarded (§6 purge-on-acquire).
    /// The control plane calls this synchronously at grant time — before the
    /// client can write anything post-grant — so a correct acquire-before-write
    /// client loses nothing and the counter reflects only pre-grant bytes.
    pub fn purge_origin(&self) -> u64 {
        let Some(master) = &self.master else {
            return 0;
        };
        let mut buf = vec![0u8; READ_BUF];
        drain_and_discard(master.as_raw_fd(), &mut buf)
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

/// Handle the origin's last close (§7.2, §6) on the present→absent transition,
/// from whichever path (POLLHUP, EOF, or EIO) observes it first. Resets the pair
/// to the baseline termios and forgets the client's settings so the next session
/// starts deterministic; then applies the write-lock lifecycle — the holder
/// releases (detach-release), while a non-holder's un-forwarded targetward backlog
/// is drained and discarded (purge-on-detach), counted, so a locked-out client's
/// buffered commands never fire when the lock later frees.
fn handle_last_close(
    fd: RawFd,
    buf: &mut [u8],
    master: &Rc<PtyMaster>,
    advertised_baud: u32,
    client_termios: &CriticalCell<Option<Value>>,
    lock: &Option<(SharedLock, OriginId)>,
) {
    let _ = apply_baseline(master.as_fd(), advertised_baud);
    client_termios.with_mut(|c| *c = None);
    if let Some((l, id)) = lock {
        let (held, exclusive, held_mode) = l.with(|g| {
            (
                g.holder() == Some(*id),
                g.arbitration() == Arbitration::Exclusive,
                g.write_mode(*id) == Some(WriteMode::Held),
            )
        });
        let mut changed = false;
        if held && !held_mode {
            // Detach-release: an on-demand holder's client left, so the lock
            // frees (§6). A `held` origin is held indefinitely and keeps the lock
            // across a client detach — only node removal releases it.
            let released = l.with_mut(|g| g.release(*id));
            if released {
                // Wake the FIFO head so a `lock --wait` waiter is granted on the
                // detach-release path (§6/§15.20); the borrow is already dropped.
                l.wake_waiters();
                changed = true;
            }
        } else if !held && exclusive {
            // Purge-on-detach: a locked-out writer's un-forwarded backlog is
            // dropped and counted, so its stale commands never fire (§6). A
            // free-for-all writer was drained above and has nothing to purge.
            let purged = drain_and_discard(fd, buf);
            if purged > 0 {
                l.with_mut(|g| g.record_purge(*id, purged));
                changed = true;
            }
        }
        // Emit an immediate lock-change notification for the transition (§10),
        // with no borrow outstanding.
        if changed {
            l.emit_change();
        }
    }
}

/// Read and discard everything currently buffered on the master, returning the
/// count of data-payload bytes discarded — the origin's un-forwarded targetward
/// backlog. Used by purge-on-acquire and purge-on-detach (§6); control packets
/// carry no data and are ignored. Stops at `WouldBlock`, EOF, or EIO.
fn drain_and_discard(fd: RawFd, buf: &mut [u8]) -> u64 {
    let mut discarded = 0u64;
    loop {
        match sys::read_fd(fd, buf) {
            Ok(0) => break,
            Ok(n) if n >= 1 => {
                if buf[0] == sys::TIOCPKT_DATA {
                    discarded += (n - 1) as u64;
                }
            }
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
            Err(_) => break,
        }
    }
    discarded
}

/// Reader + presence task. Polls the master (non-blocking) for readability and
/// hangup; drains data targetward stripping the packet-mode control byte; and on
/// last close re-asserts the baseline termios so the next session starts
/// deterministic (§7.2). Sleeps only when idle, so an active transfer streams.
async fn read_and_poll(
    master: Rc<PtyMaster>,
    present: Arc<AtomicBool>,
    client_termios: Rc<CriticalCell<Option<Value>>>,
    tx: Option<mpsc::Sender<Chunk>>,
    advertised_baud: u32,
    lock: Option<(SharedLock, OriginId)>,
) {
    let fd = master.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF];
    let mut last_reconcile = Instant::now();
    let mut wait = ACTIVE_POLL;
    loop {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let now = !re.contains(PollFlags::POLLHUP);

        // Write arbitration (§6): a writing origin drains targetward only while it
        // holds the lock; a non-holder is *not read from*, so its bytes stay in
        // the kernel buffer (backpressure to the client), never dropped. A spy
        // with no lock handle still reads — its termios and presence surface — but
        // has no `tx`, so its stray writes go nowhere. Each borrow below is dropped
        // before any await.
        let may_write = match &lock {
            Some((l, id)) => l.with(|g| g.may_write(*id)),
            None => true,
        };

        let mut did = false;
        let mut closed = false;
        // Drain available data for a writer that may write, *regardless of a
        // simultaneous POLLHUP*: a closing writer's residual must still be
        // forwarded (not purged) before the close is finalized. A non-holder
        // (may_write false) is not read from, so its backlog stays buffered for
        // purge-on-detach below.
        if may_write && re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        closed = true; // EOF: the slave closed
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
                        } else if buf[0] & sys::TIOCPKT_IOCTL != 0 && now {
                            // A present client called tcsetattr: reconcile its
                            // termios into state and re-assert EXTPROC if it was
                            // cleared (§7.2). TIOCPKT alone would miss baud changes.
                            // Gated on `now`: a lingering control packet from a
                            // client that already hung up must not re-populate the
                            // state handle_last_close clears — we still drain (and
                            // forward) data below regardless of POLLHUP.
                            reconcile_termios(&master, &client_termios);
                            last_reconcile = Instant::now();
                        }
                        // Other control packets (flush, flow-control) carry no
                        // data and need no action here.
                    }
                    Ok(_) => break, // zero-length non-EOF read: nothing to do
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    // A closed client surfaces as EIO once its buffer is drained.
                    Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                        closed = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
        }

        // Presence + last close (§7.2, §6), after any residual has been drained:
        // the client is gone if the slave hung up (POLLHUP) or a read hit EOF/EIO.
        // Handle the present→absent transition exactly once, from whichever path
        // observed it — reset the baseline and apply the write-lock lifecycle: the
        // holder releases (detach-release); an exclusive non-holder's buffered
        // backlog is purged-and-counted (purge-on-detach); a free-for-all writer,
        // already drained above, keeps its bytes.
        let present_now = now && !closed;
        let was = present.swap(present_now, Ordering::Relaxed);
        if was && !present_now {
            handle_last_close(
                fd,
                &mut buf,
                &master,
                advertised_baud,
                &client_termios,
                &lock,
            );
        }

        // Slow reconciliation backstop (§7.2): while a client is present, re-read
        // termios periodically so a missed packet-mode notification still lands.
        // Gate on live presence, not the top-of-loop `now`, so a close detected
        // mid-drain above cannot re-populate the client_termios it just cleared.
        if present.load(Ordering::Relaxed) && last_reconcile.elapsed() >= RECONCILE_INTERVAL {
            reconcile_termios(&master, &client_termios);
            last_reconcile = Instant::now();
        }

        // Active transfer rechecks promptly (ACTIVE_POLL); a truly idle master
        // backs off toward IDLE_POLL — well under the §7.2 sub-second presence
        // requirement (§15.19's adaptive active-to-idle backoff).
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
fn reconcile_termios(master: &PtyMaster, store: &CriticalCell<Option<Value>>) {
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
    store.with_mut(|s| {
        *s = Some(json!({
            "baud": format!("{:?}", cfgetospeed(&t)),
            "char_bits": char_bits,
            "parity": parity,
            "echo": t.local_flags.contains(LocalFlags::ECHO),
            "icanon": t.local_flags.contains(LocalFlags::ICANON),
            "extproc": t.local_flags.contains(LocalFlags::EXTPROC),
        }))
    });
}

/// Blocking hostward writer thread (§15.19): drain the bridged channel to the
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
        // Observe teardown before touching a buffered chunk, so a writer that just
        // broke out of `blocking_write_all` on the stop flag does not then drain the
        // remaining bridged backlog chunk-by-chunk — teardown ends within one poll
        // interval regardless of buffer depth (§15.19).
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                if present.load(Ordering::Relaxed) {
                    if blocking_write_all(fd, &chunk, &stop).is_err() {
                        // Peer hung up mid-write (or teardown set `stop` mid-write);
                        // presence will flip and the next chunks are discarded-and-
                        // counted until a client returns, or the loop-top stop check
                        // ends the thread on teardown.
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
/// fast consumer, no busy loop. `Err` means the peer hung up, or teardown set
/// `stop` while the write was blocked (§15.19): a present-but-stalled client keeps
/// the pts buffer full with no POLLHUP, so this observes `stop` within one poll
/// interval and returns instead of spinning on EAGAIN forever — the writer's join
/// then never wedges the single runtime thread.
fn blocking_write_all(
    fd: std::os::fd::RawFd,
    mut data: &[u8],
    stop: &AtomicBool,
) -> std::io::Result<()> {
    while !data.is_empty() {
        match sys::write_fd(fd, data) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let re = sys::poll_blocking(fd, PollFlags::POLLOUT | PollFlags::POLLHUP, 500);
                if re.contains(PollFlags::POLLHUP) && !re.contains(PollFlags::POLLOUT) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                // Teardown/removal asked us to stop while the peer stalled; bail
                // within the poll interval so the supervisor's join returns promptly.
                if stop.load(Ordering::Relaxed) {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Open then immediately close the slave once, priming the master's HUP state to
/// "absent" (nexus-doctor P2: a never-opened master does not report POLLHUP).
/// Priming is load-bearing for presence (§7.2): without it the master would never
/// enter the "absent" HUP state and presence would invert to phantom-present, so an
/// open failure faults the node (an environmental failure, §15.8) rather than being
/// swallowed.
fn prime_slave(pts: &str) -> Result<(), String> {
    let fd = open(pts, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())
        .map_err(|e| format!("prime open {pts}: {e}"))?;
    drop(fd);
    Ok(())
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
        // macOS termios caps standard speeds at B230400; the higher rates are
        // gated out of nix's `BaudRate` there. Advertised baud is cosmetic on a
        // PTY, so falling through to `None` off Linux/BSD is harmless (§7.2, §13).
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        460800 => BaudRate::B460800,
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        921600 => BaudRate::B921600,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::time::Instant;

    /// PTY-1: a present-but-stalled client (paused terminal / XOFF) leaves the
    /// kernel buffer full so `write_fd` spins on EAGAIN with no POLLHUP. A
    /// socketpair reproduces that — fill the send buffer while the peer never
    /// reads, so POLLOUT never arrives and the peer stays open (no hangup). The
    /// writer must observe the teardown `stop` flag and return `Interrupted` within
    /// a poll interval instead of looping forever.
    #[test]
    fn blocking_write_all_bails_when_stop_is_set_on_a_stalled_fd() {
        let (peer, sender) = UnixStream::pair().expect("socketpair");
        let wfd = sender.as_raw_fd();
        sys::set_nonblocking(wfd).expect("nonblocking");
        // Fill the send buffer until a write would block.
        let filler = vec![0u8; 4096];
        loop {
            match sys::write_fd(wfd, &filler) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("unexpected fill error: {e}"),
            }
        }
        let stop = AtomicBool::new(true);
        let start = Instant::now();
        let r = blocking_write_all(wfd, b"cannot be written while the peer stalls", &stop);
        assert!(
            matches!(&r, Err(e) if e.kind() == std::io::ErrorKind::Interrupted),
            "a stalled write with stop set must return Interrupted, got {r:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "must bail within one poll interval, took {:?}",
            start.elapsed()
        );
        drop(peer); // keep the peer open across the assertion (no POLLHUP)
    }
}
