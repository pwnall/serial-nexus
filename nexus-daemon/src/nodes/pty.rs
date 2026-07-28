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
//!   last close it re-asserts the baseline termios **and discards the pair's
//!   undelivered hostward queue** ([`flush_hostward_queue`]), which is the other
//!   half of §7.2's "every client session starts deterministic" (§5: counted).
//! * Hostward delivery is a dedicated **blocking writer thread** so a fast
//!   consumer receives at line rate (§15.19), fed by an async pump through a
//!   bounded bridge (full-buffer drops counted there, §5). It is **presence-
//!   gated** — written only while a client holds the slave, discarded-with-count
//!   otherwise (§7.2).

use std::cell::Cell;
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
use nexus_core::config::NodeConfig;
use nexus_core::state::{NodeState, NodeStatus};
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

/// How often the reconciliation poll re-reads client termios as a backstop for
/// the packet-mode notification, catching the mechanism's obscure corners (§7.2).
/// A few seconds; one ioctl, effectively free.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(3);

// The bounded bridge between the async pump and the blocking writer thread is the
// PTY boundary's buffer for a slow consumer (§5): bounded, so memory can't grow,
// and full-buffer drops are counted where they happen. Its depth is the node's
// configured `hostward_buffer` (§7.2), defaulting to 32 chunks (~2 MiB at
// [`READ_BUF`]-sized chunks) in `nexus_core::config`.

use nexus_core::lock::{Arbitration, WriteMode};

use crate::boundary::TaskSet;
use crate::cell::CriticalCell;
use crate::runtime::{
    ACTIVE_POLL, DropCounters, EdgeInbox, Handoff, LossCounter, READ_BUF, SharedTargetEdge,
    WritableOrigin, back_off, try_forward_targetward,
};
use nexus_sys as sys;

/// The pty node's *single-threaded* loss counters (§5) — the ones the reader task
/// owns, as opposed to the [`DropCounters`] it shares with the serial reader
/// (which must be atomic, because the hostward writer runs on its own thread).
///
/// They travel as one `Rc` because they are read in one place ([`PtyNode::state_extra`])
/// and written in one task ([`read_and_poll`]); passing them individually is what
/// pushed that task's signature past clippy's argument bound, and a node that grows
/// a third counter should add a field here rather than another parameter.
#[derive(Default)]
struct Losses {
    /// Bytes read from the client and then lost because the host endpoint's
    /// targetward channel was gone — an edge disconnected, or its node removed,
    /// between the read and the send (§5: loss is counted where it happens).
    targetward: Cell<u64>,
    /// Hostward bytes the kernel still held for a departed client and that the
    /// last-close reset discarded, so the next session starts on an empty pair
    /// (§7.2, [`flush_hostward_queue`]).
    ///
    /// **Why its own counter** rather than either existing one. It is neither of
    /// the two losses they name: `discarded_no_client` is output the daemon had
    /// while *nobody held the slave* (presence gating), and these bytes were
    /// written for a client that was attached; `dropped_slow_consumer` is output
    /// the daemon never handed to the kernel at all, shed at its own bounded
    /// bridge while the session was live and the client could still catch up.
    /// These reached the kernel and were destroyed by the session *ending*, which
    /// is irrecoverable and is the direct cost of a console that attaches and does
    /// not read. Folding them into either number would tell an operator watching it
    /// move to look at the wrong mechanism — §5 asks for loss that is attributable,
    /// not merely non-zero.
    at_last_close: Cell<u64>,
}

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
    /// This node's own loss counters (§5), the ones that are not shared with the
    /// serial reader. Kept together so the reader task carries one handle rather
    /// than one per counter (see [`Losses`]).
    loss: Rc<Losses>,
    /// Hostward drop counters for this boundary (§5), shared with the serial
    /// reader. Reports presence-gated discards and slow-consumer full-buffer
    /// drops in state. Defaulted at creation, replaced from the wiring at start.
    counters: Arc<DropCounters>,
    /// Observed client termios (§7.2), updated by the reader on a packet-mode
    /// `tcsetattr` notification and by the slow reconciliation backstop; `None`
    /// while no client has touched the settings this session. The daemon only
    /// *observes* — propagation to hardware is deferred (§14).
    client_termios: Rc<CriticalCell<Option<Value>>>,
    tasks: TaskSet,
    /// The node's observed status *and the moment it entered it* (§7).
    status: NodeState,
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
            loss: Rc::new(Losses::default()),
            counters: Arc::new(DropCounters::default()),
            client_termios: Rc::new(CriticalCell::new(None)),
            tasks: TaskSet::default(),
            status: NodeState::new(NodeStatus::Active),
        };

        let setup = match node.setup() {
            Ok(()) => NodeStatus::Active,
            Err(reason) => NodeStatus::Faulted { reason },
        };
        node.status.set(setup);
        node
    }

    fn setup(&mut self) -> Result<(), String> {
        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)
            .map_err(|e| format!("posix_openpt: {e}"))?;
        grantpt(&master).map_err(|e| format!("grantpt: {e}"))?;
        unlockpt(&master).map_err(|e| format!("unlockpt: {e}"))?;
        let pts = sys::ptsname(&master).map_err(|e| format!("ptsname: {e}"))?;

        apply_baseline(&master, &pts, self.advertised_baud)?;
        sys::set_packet_mode(master.as_raw_fd(), true).map_err(|e| format!("TIOCPKT: {e}"))?;

        self.apply_perms(&pts)?;
        prime_slave(&pts)?;
        // The symlink goes in **last**, after every step that can fault (RV-1).
        // A failed `setup` returns before `self.master` is stored, so the master
        // fd drops here and the kernel is free to hand the same pts index to the
        // next pty node — while a symlink installed earlier would still be on
        // disk, no longer dangling but pointing at *another console's* live
        // device. `install_symlink`'s dangling-into-devpts recovery cannot catch
        // that, because the target exists. Reachable without any typo: a non-root
        // daemon with the `group =` setting `docs/security.md` endorses fails
        // `apply_perms` on a plain `chown` EPERM (§7.2, §15.8).
        self.install_symlink(&pts)?;
        self.symlink_installed = true;

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
        inbox: Option<EdgeInbox>,
        edge: Option<SharedTargetEdge>,
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
            self.status.set(NodeStatus::Faulted {
                reason: format!("set_nonblocking: {e}"),
            });
            return;
        }

        // Reader + presence poll: client → serial (targetward), plus the presence
        // check that also gates the writer below and the client-termios
        // reconciliation that feeds state (§7.2).
        // The slave path drives the BSD/macOS termios fallback (see
        // `with_termios_fd`); `master` is `Some`, so setup ran and `pts_path` is set.
        let pts: Rc<str> = Rc::from(self.pts_path.clone().unwrap_or_default());
        let edge = edge.unwrap_or_else(crate::runtime::TargetEdge::new);
        self.tasks.push(tokio::task::spawn_local(read_and_poll(
            master.clone(),
            pts,
            self.present.clone(),
            self.client_termios.clone(),
            edge,
            self.advertised_baud,
            self.loss.clone(),
        )));

        // Writer: serial → client (hostward), presence-gated, on a dedicated
        // blocking thread so a fast consumer receives at line rate (§15.19). An
        // async pump bridges each attached edge's hostward channel to a std channel
        // the thread blocks on; aborting the pump on teardown drops its sender,
        // which unblocks and ends the thread (std recv returns Err once every
        // sender is gone).
        //
        // Both halves are created unconditionally, even with no edge attached
        // (§15.35): the pump parks on the endpoint's inbox and the thread parks on
        // an empty bridge, so a later `connect` needs no thread spawn on the
        // runtime thread and no second failure path. An idle parked thread costs
        // nothing but its stack.
        if let Some(mut inbox) = inbox {
            // Bounded bridge: the pump moves chunks into it and drops-with-count
            // when it is full (a slow consumer shedding at its own boundary, §5),
            // so the writer thread's blocking recv can also observe the stop flag.
            // Depth is this PTY's configured hostward drop policy (§7.2).
            let (btx, brx) = std_mpsc::sync_channel::<Chunk>(self.hostward_buffer.max(1));
            let pump_counters = self.counters.clone();
            self.tasks.push(tokio::task::spawn_local(async move {
                while let Some(mut rx) = inbox.recv().await {
                    while let Some(chunk) = rx.recv().await {
                        let len = chunk.len() as u64;
                        match btx.try_send(chunk) {
                            Ok(()) => {}
                            Err(std_mpsc::TrySendError::Full(_)) => pump_counters.add_full(len),
                            // The writer thread is gone: no later edge can be
                            // served either, so stop rather than spin.
                            Err(std_mpsc::TrySendError::Disconnected(_)) => return,
                        }
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
                    self.status.set(NodeStatus::Faulted {
                        reason: format!("spawn pty writer thread: {e}"),
                    });
                }
            }
        }
    }

    pub fn status(&self) -> NodeState {
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
            // Client bytes lost because this pty's host endpoint went away between
            // the read and the send (§5, §15.35).
            "discarded_targetward": self.loss.targetward.get(),
            "dropped_slow_consumer": self.counters.dropped_full(),
            // Device bytes the kernel still held for a client that never read them
            // and that the last-close reset discarded, so the next session starts
            // on an empty pair (§7.2, §5).
            "discarded_at_last_close": self.loss.at_last_close.get(),
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

    /// Ask this node's workers to stop, without waiting (§16.1, BND-1): set the
    /// writer thread's stop flag and abort the async tasks. The writer is *not*
    /// joined here and the master is deliberately kept, since the fd must outlive
    /// the writer thread. Teardown calls this on every node first, so the daemon
    /// pays the join latency once rather than once per node.
    pub fn signal_stop(&mut self) {
        self.writer_stop.store(true, Ordering::Relaxed);
        self.tasks.abort_all();
    }

    pub fn teardown(&mut self) {
        // Signal the writer thread to stop, abort the async tasks, then join the
        // thread before dropping the master so its fd stays valid throughout. The
        // stop flag (not the pump's sender drop) is what ends the thread, since
        // the runtime can't run the pump to completion while teardown blocks here.
        // Idempotent after `signal_stop`.
        self.signal_stop();
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
        // Delegate rather than re-derive (SIMP-3, §16 one-rule-one-place): this
        // used to be a copy of `teardown`'s body — stop flag, task aborts, writer
        // join, symlink unlink — which is how a step added to the stop sequence
        // reaches three of the four call sites and silently skips a node dropped
        // without teardown (the `load --replace` rollback path, a panic unwind).
        // `teardown` is idempotent, and the two extra field writes it does
        // (`master = None`, `symlink_installed = false`) are harmless here.
        // `LogNode::drop` has the same shape.
        //
        // This node keeps its `Drop` after SIMPB-10 moved the *task* half into
        // [`TaskSet`], because the **order** here is load-bearing and field-drop order
        // is not a guarantee to rest it on: the async tasks must abort, then the writer
        // thread be joined, and only then may the master's fd drop (§7.1 fd-reuse).
        // `teardown` states that sequence; `TaskSet`'s own `Drop` then finds nothing.
        self.teardown();
    }
}

/// Run `op` against the fd that controls this pair's termios.
///
/// On Linux the pty **master** is itself a terminal, so `tc*attr` operate on it
/// directly — the original, unchanged behavior. On BSD/macOS the master is *not*
/// a terminal (`tcgetattr`/`tcsetattr` return `ENOTTY`), so we momentarily open
/// the **slave** and operate on it instead — the §13 platform arm of the §7.2
/// termios path. The brief slave open does not corrupt presence: reconciliation
/// only runs while a client already holds the slave (so it is already "present"),
/// and the last-close reset wants exactly the re-primed "absent" HUP state a
/// fresh open→close leaves behind (see [`prime_slave`]).
#[cfg(target_os = "linux")]
fn with_termios_fd<T>(
    master: &PtyMaster,
    _pts: &str,
    op: impl FnOnce(BorrowedFd) -> T,
) -> Result<T, String> {
    Ok(op(master.as_fd()))
}
#[cfg(not(target_os = "linux"))]
fn with_termios_fd<T>(
    _master: &PtyMaster,
    pts: &str,
    op: impl FnOnce(BorrowedFd) -> T,
) -> Result<T, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(pts)
        .map_err(|e| format!("open slave {pts}: {e}"))?;
    Ok(op(slave.as_fd()))
}

/// Baseline termios (§7.2): raw + echo off + EXTPROC on, plus the cosmetic
/// advertised baud. Applied through the master on Linux and a momentarily-opened
/// slave on BSD/macOS (see [`with_termios_fd`]). Free so both creation and the
/// last-close reset can call it.
fn apply_baseline(master: &PtyMaster, pts: &str, advertised_baud: u32) -> Result<(), String> {
    // Outer `?` propagates a slave-open failure (BSD path); the inner Result is the
    // tcget/tcsetattr outcome.
    with_termios_fd(master, pts, |fd| apply_baseline_fd(fd, advertised_baud))?
}

fn apply_baseline_fd(fd: BorrowedFd, advertised_baud: u32) -> Result<(), String> {
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
/// to the baseline termios, discards the hostward output the departing client
/// never read, and forgets the client's settings so the next session starts
/// deterministic; then applies the write-lock lifecycle — the holder releases
/// (detach-release), while a non-holder's un-forwarded targetward backlog is
/// drained and discarded (purge-on-detach), counted, so a locked-out client's
/// buffered commands never fire when the lock later frees.
///
/// The two discards here are in **opposite directions** and must not be conflated:
/// [`flush_hostward_queue`] drops device→client bytes the kernel was holding for a
/// client that has gone (§7.2, charged to `discarded_at_last_close`), while
/// [`drain_and_discard`] below drops client→device bytes (§6 purge-on-detach,
/// charged per origin). Neither is purge-on-reconnect, which is targetward and
/// lives in the `leg`/serial reconnect path (§6/§7.1).
///
/// The baseline reset here is itself observable: re-`tcsetattr`ing the pair re-arms
/// a `TIOCPKT_IOCTL` notification on the master, which the caller must consume so it
/// cannot be mistaken for a fresh client session (see the close block in
/// [`read_and_poll`]). The purge-on-detach branch below consumes it in passing; the
/// caller covers the branches that do not. The hostward flush adds nothing to that
/// obligation — it reads the *slave*, so it leaves the master with nothing readable
/// (measured; a `tcflush` would not, which is one of the reasons it is not used).
fn handle_last_close(
    buf: &mut [u8],
    master: &Rc<PtyMaster>,
    pts: &str,
    advertised_baud: u32,
    client_termios: &CriticalCell<Option<Value>>,
    origin: &Option<WritableOrigin>,
    loss: &Losses,
) {
    // The master's fd, rather than a parameter carrying the same number: the two
    // could only ever disagree by mistake.
    let fd = master.as_raw_fd();
    let _ = apply_baseline(master, pts, advertised_baud);
    // §7.2's reset is not only about how the next session's bytes are *framed*:
    // the pair also still holds whatever this session was sent and never read, and
    // the kernel hands that to the next opener. Discard it here, and charge it —
    // §5 allows a slow (or absent) spy to cost itself data, never to lose it
    // invisibly.
    loss.at_last_close.add(flush_hostward_queue(pts, buf));
    client_termios.with_mut(|c| *c = None);
    if let Some((_, l, id)) = origin {
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

/// The most times [`flush_hostward_queue`] re-reads the pair before giving up.
///
/// The loop's real terminator is a round that reads nothing; this cap only bounds
/// the one case that could keep feeding it — the hostward writer thread finishing
/// the chunk it was mid-write on when presence flipped (§15.19) — so a session
/// boundary can never become an unbounded loop on the single runtime thread. Each
/// round already moves a kernel queue's worth of bytes (measured: the whole 8 KiB
/// backlog in round one, 24 µs), so reaching the cap means something is writing
/// faster than the queue drains, and stopping there costs only the accuracy of the
/// count.
const MAX_FLUSH_ROUNDS: usize = 16;

/// Make the slave [`flush_hostward_queue`] just opened readable *byte by byte*
/// rather than line by line — the §13 arm of the flush.
///
/// **Linux: nothing to do, and doing something would cost.** The pair's baseline is
/// applied through the master and survives the client's close, so this fd is already
/// raw; a `tcsetattr` on the *slave* would additionally re-arm a `TIOCPKT_IOCTL`
/// packet on the master, which is precisely the session-evidence noise the flush is
/// careful not to create (see [`read_and_poll`]'s close block, §15.36).
///
/// **BSD/macOS: required.** The pair's termios resets to cooked when the last slave
/// fd closes (§7.2's platform arm — which is why the node re-asserts the baseline on
/// the next rising presence edge rather than trusting the one it set at last close),
/// and a canonical-mode read returns only *complete lines*: a half-written line
/// would survive the flush and reach the next session. Setting this fd raw first is
/// what makes the guarantee identical on both platforms. The extra `tcsetattr` joins
/// the one `apply_baseline` already performs through its own momentary slave open on
/// this platform, so it adds no new mechanism, only one more use of it.
#[cfg(target_os = "linux")]
fn make_drainable(_fd: BorrowedFd) {}
#[cfg(not(target_os = "linux"))]
fn make_drainable(fd: BorrowedFd) {
    if let Ok(mut t) = tcgetattr(fd) {
        cfmakeraw(&mut t);
        let _ = tcsetattr(fd, SetArg::TCSANOW, &t);
    }
}

/// Discard whatever the pair still holds **hostward** — device bytes written
/// toward a console whose client never read them — and return the exact count
/// (§7.2, §5).
///
/// This is the second half of §7.2's "on last close the daemon resets the pair's
/// termios so every client session starts deterministic". The termios half settles
/// how the next session's bytes are *framed*; this settles *which bytes it sees at
/// all*. Without it the kernel keeps the departed session's undelivered output
/// across the close — up to the pts input queue's depth, measured at ~13.8 KiB —
/// and hands it to the next opener, so a fresh `picocom` opens onto the previous
/// operator's scrollback. Worse for §5: nothing counted the loss when that data was
/// finally destroyed, so `state` reported `discarded_no_client: 0` on a boundary
/// that had shed kilobytes.
///
/// **Direction.** Hostward, device → client. It is *not* §6's purge, which is
/// targetward, drains the master, and is counted per origin ([`drain_and_discard`],
/// [`record_detach_purge`]); and it is not purge-on-reconnect (§6/§7.1), which is
/// also targetward and belongs to the reconnecting node. Different queue, different
/// direction, different counter.
///
/// **Why reading the slave dry rather than `tcflush`.** The queue belongs to the
/// *slave*, so a slave fd is the only handle that names all of it. Measured on 7.0
/// against an 8 KiB backlog: `TCOFLUSH` on the master clears the flip buffers but
/// leaves the line discipline's read buffer, and the next opener still read 4095
/// bytes; `TCIFLUSH` on the master is the other direction and changed nothing;
/// `TCIFLUSH` on the slave does clear it, but reports *how much* it discarded to
/// nobody — §5 needs the number — and leaves a `TIOCPKT_FLUSHREAD` packet readable
/// on the master, which at the next poll is indistinguishable from a client's own
/// session evidence. Reading the slave dry clears the same queue, yields the exact
/// count, and leaves the master with nothing readable (verified in
/// [`tests::a_sessions_undelivered_output_survives_last_close_and_the_flush_removes_it`]).
///
/// **Platform.** The open itself is one arm on purpose: it is the mechanism
/// §7.2/§15.30 already uses off Linux for the baseline termios (see
/// [`with_termios_fd`]) and that [`prime_slave`] uses everywhere, and it has to be
/// the slave on *both*, because the master cannot name the whole queue on either.
/// Only the *readability* of that fd needs the §13 split, in [`make_drainable`]. It
/// also leaves the pair in exactly the primed, hung-up state presence depends on:
/// verified that the master reports `POLLHUP` again the instant the fd closes, so
/// the §15.36 latch cannot be re-armed by our own open.
fn flush_hostward_queue(pts: &str, buf: &mut [u8]) -> u64 {
    // Read-only and non-blocking: this hand never writes, and must never park the
    // runtime thread. A pts the daemon can no longer open (a faulted setup leaves
    // `pts_path` empty; the device node could be gone) has no queue to charge for
    // either, so report none rather than inventing loss.
    let Ok(slave) = open(
        pts,
        OFlag::O_RDONLY | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) else {
        return 0;
    };
    make_drainable(slave.as_fd());
    let fd = slave.as_raw_fd();
    let mut discarded = 0u64;
    for _ in 0..MAX_FLUSH_ROUNDS {
        let mut round = 0u64;
        loop {
            match sys::read_fd(fd, buf) {
                Ok(0) => break, // EOF
                Ok(n) => round += n as u64,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                // EIO and friends: there is nothing left to take.
                Err(_) => break,
            }
        }
        discarded += round;
        // Re-check only after a productive round, and only with a *non-blocking*
        // poll: an empty round is the queue's true end, and re-polling after one
        // would spend the session boundary waiting for bytes nobody is sending.
        // The re-check exists because a byte can still be in the pair's flip
        // buffer when the line discipline's own buffer runs dry.
        if round == 0 || !sys::poll_ready(fd, PollFlags::POLLIN).contains(PollFlags::POLLIN) {
            break;
        }
    }
    discarded
}

/// Charge `n` targetward bytes that §6's **detach** instance drained — the
/// session's un-delivered backlog, whether it was still in the master's kernel
/// buffer or already parked in the reader's held slot.
///
/// It goes to the origin's *per-origin purge counter*, the one §6 requires state to
/// report, and **not** to `discarded_targetward`, because the two mean different
/// things and only one of them is a defect: `discarded_targetward` is loss — bytes
/// the endpoint could never take because it went away — while these were purged on
/// purpose, at the moment the floor question settled, exactly as
/// [`handle_last_close`]'s purge-on-detach branch counts a non-holder's backlog.
/// Reading `discarded_targetward > 0` on a console that merely detached from a
/// stalled endpoint would say §5 was violated when it was not.
///
/// With no origin to attribute to (a read-only spy edge, or one `disconnect`
/// cleared) there is no per-origin counter, so the node's own loss counter is the
/// only honest home; §5 forbids the silent version either way.
fn record_detach_purge(n: u64, origin: &Option<WritableOrigin>, lost: &Cell<u64>) {
    if n == 0 {
        return;
    }
    match origin {
        Some((_, l, id)) => {
            l.with_mut(|g| g.record_purge(*id, n));
            // The borrow is dropped; announce the transition like every other
            // purge does (§10).
            l.emit_change();
        }
        None => lost.add(n),
    }
}

/// Reader + presence task. Polls the master (non-blocking) for readability and
/// hangup; drains data targetward stripping the packet-mode control byte; and on
/// last close re-asserts the baseline termios so the next session starts
/// deterministic (§7.2). Sleeps only when idle, so an active transfer streams.
///
/// **The invariant this loop is shaped around (CONC-1): the presence /
/// last-close / detach-release block must run within a bounded time regardless of
/// targetward backpressure.** It is the node's whole lifecycle half — the presence
/// swap, [`handle_last_close`] (the §7.2 baseline reset, §6 detach-release and
/// purge-on-detach) and the [`RECONCILE_INTERVAL`] backstop — and it used to sit
/// *after* an unbounded `tx.send(payload).await`. A host endpoint whose targetward
/// channel is full (a serial node `waiting` for an absent device, or `active`
/// while the far end does not drain) parked that send for as long as the device
/// was gone, and everything below it stopped: the node reported
/// `client_present: true` forever after its client exited, and an on-demand write
/// lock stayed held by an origin whose client was gone.
///
/// So the data half never parks *unboundedly*. A payload the endpoint cannot take
/// right now is *held* in `pending` and this task stops reading the master, which
/// backpressures the client through the kernel buffer exactly as a locked-out
/// non-holder is backpressured — §5 forbids **dropping** targetward, not delaying
/// it, and the byte is never dropped for want of capacity. Every pass then runs the
/// lifecycle block once, with fresh `POLLHUP`.
///
/// **Three rules keep that hold from becoming a hiding place**, each of them a
/// defect the first version of this fix shipped:
///
/// 1. *The hold is registered on the endpoint, not on a timer.* While a payload is
///    held the tail of the loop parks in `tx.reserve()` under a
///    [`back_off`]-bounded timeout, so the task is a producer **suspended
///    mid-send** in the precise sense §6 uses: `boundary::drain_to_quiescence`
///    drains the channel, `yield_now`s, and this task resolves its reservation into
///    the freed slot in time for the next round to drain it. Parking on a bare
///    `sleep` instead put the payload outside the pipeline, where
///    purge-on-reconnect could not see it, and twenty minutes of pre-outage console
///    input fired into the device's boot prompt the moment it came back (§7.1) —
///    with the purge cheerfully reporting success.
/// 2. *A held payload never crosses a session boundary.* The close block below
///    offers it once more and then **purges** it (§6's detach instance). Carrying
///    it past `handle_last_close` would deliver it under whatever origin holds the
///    lock next, which is the interleave the exclusive floor exists to prevent; and
///    on an endpoint that is simply stalled it is the stale-command hazard itself.
/// 3. *Lifecycle observation does not depend on the data slot being empty.* Because
///    of (2), `pending` is only ever held while a client is still attached, so the
///    `pending.is_none()` gate on the master read can never outlive the session it
///    belongs to — and the close trigger is the presence edge **or** session
///    evidence, no longer `closed && saw_session`. The old conjunction needed an
///    EOF/EIO the drain never reached when it ended early on a full endpoint, so a
///    session collapsed into one poll gap leaked its lock for as long as the
///    endpoint stayed saturated.
async fn read_and_poll(
    master: Rc<PtyMaster>,
    pts: Rc<str>,
    present: Arc<AtomicBool>,
    client_termios: Rc<CriticalCell<Option<Value>>>,
    edge: SharedTargetEdge,
    advertised_baud: u32,
    loss: Rc<Losses>,
) {
    let fd = master.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF];
    let mut last_reconcile = Instant::now();
    let mut wait = ACTIVE_POLL;
    // Set when this session's client left *anything* readable on the master — a
    // `TIOCPKT_DATA` payload or a control packet such as the `TIOCPKT_IOCTL` a
    // client's `tcsetattr` provokes — and cleared by the close block below. It lets
    // a collapsed session, one whose whole attach→(traffic)→detach the task first
    // observes only at the closing EOF so `present` never latched true, still run
    // the last-close handler. It is deliberately *not* narrowed to data: a session
    // that only configures the line writes no byte and still has to release its
    // lock. See the close block below.
    let mut saw_session = false;
    // A payload already read off the master that the host endpoint's bounded
    // targetward channel could not take yet (CONC-1). Holding it here — rather
    // than parking the task inside the drain on `send().await` — is what keeps the
    // lifecycle block below reachable while an endpoint stays full indefinitely.
    // While it is held this task reads nothing, so the client backpressures
    // through the kernel buffer and no targetward byte is dropped (§5).
    //
    // Two invariants make the hold safe rather than a hole (see this function's
    // doc): the tail of the loop parks it on `tx.reserve()`, so a purge-to-
    // quiescence can still reach it, and the close block empties it, so it never
    // outlives the session that produced it.
    let mut pending: Option<Chunk> = None;
    loop {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let now = !re.contains(PollFlags::POLLHUP);

        // Re-read the live edge binding every pass (§15.35): `connect` and
        // `disconnect` fill and clear this slot on a running node, so a pty gains
        // and loses its write path without its reader ever restarting — which is
        // what keeps presence latching, termios reconciliation and detach-release
        // continuous across edge surgery. Cloning the binding out of the critical
        // section is what lets the send below `.await` with no borrow held (§15.20).
        let origin = edge.origin();

        // Write arbitration (§6): a writing origin drains targetward only while it
        // holds the lock; a non-holder is *not read from*, so its bytes stay in
        // the kernel buffer (backpressure to the client), never dropped. A spy
        // with no lock handle still reads — its termios and presence surface — but
        // has no `tx`, so its stray writes go nowhere. Each borrow below is dropped
        // before any await.
        // An endpoint with no edge at all is *not read from* (§6, §15.35): the
        // client's bytes stay in the kernel buffer as backpressure rather than being
        // read and thrown away. An attached but read-only edge (`write_mode =
        // "never"`) still reads — that is the spy shape, whose termios and presence
        // an operator does want surfaced — and its stray writes go nowhere.
        let may_write = match &origin {
            Some((_, l, id)) => l.with(|g| g.may_write(*id)),
            None => edge.with(|e| e.attached),
        };

        let mut did = false;
        let mut closed = false;

        // Hand off a payload held over from an earlier pass before reading
        // anything more (CONC-1). The retry is unconditional — not gated on
        // `may_write` — because these bytes are already out of the master and were
        // read while this origin *could* write; a steal landing in between must not
        // strand them. The one thing it must not outlive is its own session, and
        // that is the close block's job, not this one's.
        if let Some(chunk) = pending.take() {
            let len = chunk.len() as u64;
            match &origin {
                // The shared non-blocking handoff (SIMP-2): it never touches the
                // lock, so the unconditional-retry rule above still holds, and it
                // charges the endpoint-gone exit itself.
                Some(origin) => {
                    match try_forward_targetward(origin, chunk, &loss.targetward) {
                        Handoff::Taken => did = true,
                        // Still full: keep holding it and keep reading nothing.
                        Handoff::Full(chunk) => pending = Some(chunk),
                        // The host endpoint went away while the payload waited — the
                        // same loss the drain below charges, reached one pass later,
                        // counted inside the handoff.
                        Handoff::Lost => {}
                    }
                }
                // `disconnect` cleared the edge under us; §5 says loss is counted
                // where it happens.
                None => loss.targetward.add(len),
            }
        }

        // Drain available data for a writer that may write, *regardless of a
        // simultaneous POLLHUP*: a closing writer's residual must still be
        // forwarded (not purged) before the close is finalized. A non-holder
        // (may_write false) is not read from, so its backlog stays buffered for
        // purge-on-detach below. An origin still holding an un-delivered payload
        // reads nothing either, for the same reason and with the same effect: the
        // client stalls on a kernel buffer nobody is draining (CONC-1).
        //
        // That last gate used to be able to swallow a whole *session*: with a
        // payload held the loop never called `read_fd`, so `saw_session`/`closed`
        // could not be set and a session that opened and closed inside one poll gap
        // leaked its lock for as long as the endpoint stayed saturated. It is
        // bounded now by the close block emptying `pending`, which means the gate
        // can only be closed while a client is still attached — the pass that
        // observes the hangup always finds the slot free again.
        if pending.is_none() && may_write && re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        closed = true; // EOF: the slave closed
                        break;
                    }
                    Ok(n) if n >= 1 => {
                        did = true;
                        // Evidence of a client session, whatever the packet says: a
                        // read only succeeds while a client has been on the other
                        // end. It arms the close arm below so a session collapsed
                        // into one poll gap still releases — including the shape
                        // that carries no data at all (open → `tcsetattr` → close),
                        // which leaves exactly one `TIOCPKT_IOCTL` packet behind and
                        // used to be read straight past.
                        saw_session = true;
                        if buf[0] == sys::TIOCPKT_DATA {
                            // Forward the payload targetward — or, with no writable
                            // edge (a read-only spy edge, or one `disconnect`
                            // removed), count it. Either way the bytes are
                            // accounted: §5 forbids the silent version, and this
                            // path used to take it.
                            if n > 1 && origin.is_none() {
                                loss.targetward.add((n - 1) as u64);
                            }
                            if n > 1
                                && let Some(origin) = &origin
                            {
                                let payload = Chunk::copy_from_slice(&buf[1..n]);
                                // Targetward: backpressure to the origin — but
                                // never by parking *this task* (CONC-1). A full
                                // channel parks the payload instead and ends the
                                // drain; the master then goes unread, so the client
                                // stalls on the kernel buffer and the presence /
                                // last-close block below still runs this pass. The
                                // shared non-blocking handoff (SIMP-2) owns the
                                // three outcomes and charges the lost one itself.
                                match try_forward_targetward(origin, payload, &loss.targetward) {
                                    Handoff::Taken => {}
                                    Handoff::Full(payload) => {
                                        pending = Some(payload);
                                        break;
                                    }
                                    Handoff::Lost => {
                                        // The host endpoint is gone. Do *not*
                                        // return: that would end presence latching,
                                        // termios reconciliation and detach-release
                                        // with it (MAP-1's chain). Break out of the
                                        // drain and let the next pass re-read the
                                        // edge slot, which a `disconnect` will have
                                        // cleared.
                                        //
                                        // The payload is already out of the master,
                                        // so it is lost — and §5 says loss is
                                        // counted where it happens, which the
                                        // handoff did before returning.
                                        break;
                                    }
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
                            reconcile_termios(&master, &pts, &client_termios);
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
        //
        // The `saw_session` arm covers a *collapsed* session: one poll of a client
        // can observe the attach, all of its traffic, AND the closing EOF at once —
        // a scripted sub-poll-window probe, or a starved task — so `present` never
        // latches true and the `was` edge misses it, leaving an on-demand holder
        // that attached and detached in one poll holding its lock forever, the
        // endpoint dead to every other writer (§6).
        //
        // That arm is `saw_session && !present_now`, **not** `saw_session && closed`.
        // The conjunction with `closed` looked equivalent — a hung-up master answers
        // EIO once its buffer empties — but it silently required the drain to *reach*
        // that EIO, and the drain ends early whenever the endpoint refuses a payload.
        // A collapsed session against a saturated endpoint therefore armed
        // `saw_session`, parked its byte, never saw EIO, and leaked the lock; the
        // audit measured five of five leaking, and `send` from another origin failing
        // with the locked error afterwards. `!present_now` is the honest predicate:
        // POLLHUP alone means the session is over, whatever the drain got to.
        //
        // The latch is armed by **any** successful read, not by a `TIOCPKT_DATA`
        // payload alone. Narrowing it to data was a real leak: a session that opens,
        // calls `tcsetattr` and closes writes no byte, but the master still holds
        // its `TIOCPKT_IOCTL` packet past the hangup (`read = 1`, then `EIO`), and
        // both disjuncts read straight past that evidence. The one shape left
        // uncovered is a bare open→close touching nothing, which leaves *nothing*
        // readable to latch on; it is also the harmless one — a client that sent no
        // command has none to purge — and the next observed session re-arms the
        // handler.
        //
        // Two things together keep this an edge rather than a spin. The latch is
        // *cleared* the moment the close is handled, so with `was` already false
        // neither disjunct can fire until a new client leaves new evidence; and the
        // drain below consumes the packet-mode notification that `handle_last_close`
        // provokes itself — its `apply_baseline` re-`tcsetattr`s the pair (through
        // the master on Linux, which the kernel redirects to the slave; through a
        // momentary slave open on BSD), which re-arms `TIOCPKT_IOCTL` on the master.
        // That packet is indistinguishable *by type* from a client's, so leaving it
        // readable would re-arm the widened latch on the next poll and re-fire the
        // handler every pass (the 99%-CPU shape the `b8d8ed8` note records). Consume
        // it, and the anti-spin argument needs no assumption about POLLIN going
        // quiet after a hangup — which is a kernel-version-dependent claim (§13).
        let present_now = now && !closed;
        let was = present.swap(present_now, Ordering::Relaxed);
        if !present_now && (was || saw_session) {
            // The floor question is settling *now*, so the payload this task is
            // still holding has to settle with it (§6's detach instance). Offer it
            // once more first: a momentarily-full endpoint takes it here, and taking
            // it *before* `handle_last_close` puts it in the channel ahead of
            // anything the next holder queues, so two origins cannot interleave.
            //
            // If the endpoint still refuses it, it is purged rather than carried
            // across the release. Carrying it was the reachable defect: the retry at
            // the top of the loop does not consult the lock, so the chunk was written
            // by an origin that no longer held the floor — after detach-release, after
            // the next holder's purge-on-acquire, into the middle of the line that
            // holder had just queued. And on an endpoint that is merely stalled it is
            // the stale-command hazard verbatim: a client typed into a console that
            // answered nothing and walked away.
            let held = match pending.take() {
                None => 0,
                Some(chunk) => match &origin {
                    Some(o) => match try_forward_targetward(o, chunk, &loss.targetward) {
                        Handoff::Taken => 0,
                        Handoff::Full(chunk) => chunk.len() as u64,
                        // The endpoint is gone; the handoff charged it already.
                        Handoff::Lost => 0,
                    },
                    None => {
                        loss.targetward.add(chunk.len() as u64);
                        0
                    }
                },
            };
            handle_last_close(
                &mut buf,
                &master,
                &pts,
                advertised_baud,
                &client_termios,
                &origin,
                &loss,
            );
            saw_session = false;
            // Consume what the handler's own termios reset left behind (above) —
            // but only for an endpoint this pass is permitted to read from. One that
            // is *not* is either a locked-out non-holder, whose backlog
            // `handle_last_close` has just purged-and-counted itself (purge-on-
            // detach, §6), or an endpoint with no edge at all, whose client backlog
            // is deliberately **parked** and must never be dropped (invariant
            // 14/§5).
            //
            // On a `may_write` endpoint that kept up, the drain above forwarded
            // everything and all this finds is the control packet. On one that did
            // not — the case that used to be excluded here, because `pending` was
            // held and the master had gone unread — it finds the session's remaining
            // input, which is the same un-delivered backlog as the held payload and
            // settles the same way: §6 drains it at detach and counts it. Leaving it
            // there instead is not conservatism, it is a trap; the next pass would
            // read it as *new* session evidence and re-fire this handler forever.
            let residual = if may_write {
                drain_and_discard(fd, &mut buf)
            } else {
                0
            };
            record_detach_purge(held + residual, &origin, &loss.targetward);
        }
        // BSD/macOS only: the slave's termios resets to cooked when the last slave
        // fd closes (verified: a momentary daemon-side set does not survive to the
        // client's open), so the setup-time baseline is gone by the time a client
        // attaches. Re-assert it on the rising edge — the client now holds the slave,
        // so *its* fd keeps the raw/echo-off/EXTPROC baseline alive for the session.
        // The §13 platform arm of the §7.2 baseline; on Linux the master carries it.
        #[cfg(not(target_os = "linux"))]
        if !was && present_now {
            let _ = apply_baseline(&master, &pts, advertised_baud);
        }

        // Slow reconciliation backstop (§7.2): while a client is present, re-read
        // termios periodically so a missed packet-mode notification still lands.
        // Gate on live presence, not the top-of-loop `now`, so a close detected
        // mid-drain above cannot re-populate the client_termios it just cleared.
        if present.load(Ordering::Relaxed) && last_reconcile.elapsed() >= RECONCILE_INTERVAL {
            reconcile_termios(&master, &pts, &client_termios);
            last_reconcile = Instant::now();
        }

        // The one await of the pass. Active transfer rechecks promptly
        // (ACTIVE_POLL); a truly idle master backs off toward IDLE_POLL — well under
        // the §7.2 sub-second presence requirement (§15.19's adaptive
        // active-to-idle backoff).
        //
        // **While a payload is held, that wait is spent inside `tx.reserve()`, not
        // inside a timer**, and the difference is the whole of §6's reconnect
        // instance. A reservation is registered on the endpoint's channel, so this
        // task is a producer "suspended mid-send" in exactly the sense the purge
        // invariant names: `boundary::drain_to_quiescence` drains the parked
        // pipeline, `yield_now`s, and this task — woken by the permit that drain
        // freed — resolves its reservation into the channel before the next round
        // drains it. On a bare `sleep` the payload was invisible to that purge, and
        // it landed on the device *after* the reconnect: `purged_on_reconnect`
        // reported success and the operator's pre-outage input still hit the boot
        // prompt (measured at 5634 bytes on the audit's rig).
        //
        // The reservation is cancel-safe and the timeout keeps the lifecycle block
        // above on its bounded schedule (CONC-1), so nothing here re-parks the
        // presence half; and this loop's only other await is the plain sleep below,
        // which `pending` never reaches. Since the data plane is single-threaded,
        // that is the whole argument: whenever a purge can run at all, the held
        // payload is registered where the purge looks.
        match pending.take() {
            Some(chunk) => {
                let n = chunk.len() as u64;
                let Some((tx, _, _)) = &origin else {
                    // `disconnect` cleared the edge under us (§5: counted where it
                    // happens). Unreachable in practice — the retry at the top of
                    // the pass charges this case and no await separates the two —
                    // but the slot must not silently swallow a chunk either way.
                    loss.targetward.add(n);
                    continue;
                };
                match tokio::time::timeout(wait, tx.reserve()).await {
                    // Capacity: deliver through the permit. This is the send the
                    // purge above resolves.
                    Ok(Ok(permit)) => {
                        permit.send(chunk);
                        wait = ACTIVE_POLL;
                    }
                    // The endpoint went away while we waited — the `Handoff::Lost`
                    // case, one await later.
                    Ok(Err(_)) => loss.targetward.add(n),
                    // Still full: keep holding, keep the master unread, and let the
                    // lifecycle block run again.
                    Err(_) => {
                        pending = Some(chunk);
                        back_off(&mut wait);
                    }
                }
            }
            None if did => wait = ACTIVE_POLL,
            None => {
                tokio::time::sleep(wait).await;
                back_off(&mut wait);
            }
        }
    }
}

/// Read the client's current termios (through the master on Linux, a slave open
/// on BSD/macOS — see [`with_termios_fd`]) and store it in state; re-assert
/// EXTPROC if the client cleared it, so the daemon keeps observing subsequent
/// changes (§7.2). Observe-only — never propagated to hardware (§14).
fn reconcile_termios(master: &PtyMaster, pts: &str, store: &CriticalCell<Option<Value>>) {
    let _ = with_termios_fd(master, pts, |fd| reconcile_termios_fd(fd, store));
}

fn reconcile_termios_fd(fd: BorrowedFd, store: &CriticalCell<Option<Value>>) {
    let Ok(t) = tcgetattr(fd) else { return };
    if !t.local_flags.contains(LocalFlags::EXTPROC) {
        let mut re = t.clone();
        re.local_flags.insert(LocalFlags::EXTPROC);
        let _ = tcsetattr(fd, SetArg::TCSANOW, &re);
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
                    // Peer hung up mid-write (or teardown set `stop` mid-write):
                    // presence will flip and the next chunks are discarded-and-counted
                    // until a client returns, or the loop-top stop check ends the
                    // thread. The bytes of *this* chunk that never reached the master
                    // are lost too, so charge the remainder to the same
                    // no-client-took-them counter rather than dropping it silently
                    // (§5 all-loss-counted, PTY-3).
                    if let Err(short) = blocking_write_all(fd, &chunk, &stop) {
                        counters.add_absent(short.unwritten as u64);
                        tracing::debug!(
                            target: "pty",
                            unwritten = short.unwritten,
                            "hostward write ended early: {}",
                            short.error
                        );
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

/// A hostward write that ended before the chunk was fully delivered — the loss
/// [`writer_thread`] has to attribute (§5, PTY-3). Carrying the remainder in the
/// error is what makes the shortfall impossible to drop on the floor: the previous
/// bare `io::Result<()>` told the caller *that* the write failed but not *how much*
/// of the chunk had gone, so the unwritten tail vanished uncounted.
struct WriteShortfall {
    /// Bytes of the chunk that never reached the master.
    unwritten: usize,
    /// Why the write ended early: the peer hung up (`BrokenPipe`), teardown set the
    /// stop flag mid-write (`Interrupted`), or the write itself failed.
    error: std::io::Error,
}

/// Write every byte of `data` to the master, blocking the *thread* (not the async
/// runtime) on `poll(2)` for writability between partial writes — line rate for a
/// fast consumer, no busy loop. `Err` means the peer hung up, or teardown set
/// `stop` while the write was blocked (§15.19): a present-but-stalled client keeps
/// the pts buffer full with no POLLHUP, so this observes `stop` within one poll
/// interval and returns instead of spinning on EAGAIN forever — the writer's join
/// then never wedges the single runtime thread. The error carries the number of
/// bytes still unwritten so the caller can count them (§5).
fn blocking_write_all(
    fd: std::os::fd::RawFd,
    data: &[u8],
    stop: &AtomicBool,
) -> Result<(), WriteShortfall> {
    let mut rest = data;
    // Every early return goes through this, so no exit path can forget the
    // remainder — the shortfall is derived from `rest`, never re-counted by hand.
    let short = |rest: &[u8], error: std::io::Error| WriteShortfall {
        unwritten: rest.len(),
        error,
    };
    while !rest.is_empty() {
        match sys::write_fd(fd, rest) {
            Ok(0) => return Err(short(rest, std::io::ErrorKind::WriteZero.into())),
            Ok(n) => rest = &rest[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let re = sys::poll_blocking(fd, PollFlags::POLLOUT | PollFlags::POLLHUP, 500);
                if re.contains(PollFlags::POLLHUP) && !re.contains(PollFlags::POLLOUT) {
                    return Err(short(rest, std::io::ErrorKind::BrokenPipe.into()));
                }
                // Teardown/removal asked us to stop while the peer stalled; bail
                // within the poll interval so the supervisor's join returns promptly.
                if stop.load(Ordering::Relaxed) {
                    return Err(short(rest, std::io::ErrorKind::Interrupted.into()));
                }
            }
            Err(e) => return Err(short(rest, e)),
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
        let payload = b"cannot be written while the peer stalls";
        let start = Instant::now();
        let r = blocking_write_all(wfd, payload, &stop);
        let Err(short) = r else {
            panic!("a stalled write with stop set must fail");
        };
        assert_eq!(
            short.error.kind(),
            std::io::ErrorKind::Interrupted,
            "a stalled write with stop set must report Interrupted, got {:?}",
            short.error
        );
        // PTY-3: the bytes that never reached the master are reported, so the
        // caller can charge them (§5 all-loss-counted). Nothing was written here,
        // because the peer's buffer was already full.
        assert_eq!(short.unwritten, payload.len());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "must bail within one poll interval, took {:?}",
            start.elapsed()
        );
        drop(peer); // keep the peer open across the assertion (no POLLHUP)
    }

    /// RV-1: a `setup` that faults *after* allocating the pair must leave no
    /// symlink behind. The fd it never stored drops on the way out, the kernel is
    /// free to reissue that pts index to the next pty node, and a leftover symlink
    /// would then resolve — no longer dangling, so [`PtyNode::install_symlink`]'s
    /// devpts recovery cannot clean it up — onto *another console's* live device.
    /// `group` is the cheap deterministic trigger; the reachable one is a plain
    /// `chown` EPERM on a non-root daemon (§7.2, §15.8).
    #[test]
    fn a_setup_that_faults_leaves_no_symlink_behind() {
        let dir = std::env::temp_dir().join(format!("snx-pty-rv1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let link = dir.join("console");
        let cfg: NodeConfig = serde_json::from_value(json!({
            "type": "pty",
            "name": "conA",
            "path": link.display().to_string(),
            "group": "nosuchgroup-serial-nexus-rv1",
        }))
        .expect("pty node config");

        let node = PtyNode::create(&cfg);
        assert!(
            matches!(node.status().status(), NodeStatus::Faulted { .. }),
            "an unresolvable group must fault the node, got {:?}",
            node.status()
        );
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "a faulted setup left {} on disk — it now aliases whichever pty node \
             the kernel hands the freed pts index to (RV-1)",
            link.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One pty pair built the way [`PtyNode::setup`] builds it — baseline termios,
    /// packet mode on the master, slave primed once, master non-blocking — so the
    /// kernel behaviour measured below is the behaviour the node actually meets.
    fn pair() -> (PtyMaster, String) {
        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let pts = sys::ptsname(&master).expect("ptsname");
        apply_baseline(&master, &pts, 115200).expect("baseline termios");
        sys::set_packet_mode(master.as_raw_fd(), true).expect("TIOCPKT");
        prime_slave(&pts).expect("prime the slave");
        sys::set_nonblocking(master.as_raw_fd()).expect("nonblocking master");
        (master, pts)
    }

    /// One client session that attaches, never reads, and leaves: the "device"
    /// pushes up to `burst` bytes hostward while the client holds the slave, then
    /// the client detaches. Returns what the kernel accepted — the queue that is
    /// still there at last close.
    fn session_that_never_reads(master: &PtyMaster, pts: &str, burst: usize) -> u64 {
        let client = open(pts, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("attach");
        let block = vec![b'x'; 4096];
        let mut wrote = 0u64;
        while (wrote as usize) < burst {
            let want = (burst - wrote as usize).min(block.len());
            match sys::write_fd(master.as_raw_fd(), &block[..want]) {
                Ok(0) => break,
                Ok(n) => wrote += n as u64,
                // The pair is full: the client is not reading, which is the point.
                Err(_) => break,
            }
        }
        drop(client); // last close
        wrote
    }

    /// §7.2's "every client session starts deterministic", the half that was
    /// missing: the pair's *hostward* queue outlives the session that never read
    /// it, and [`flush_hostward_queue`] is what removes it — visibly (§5).
    ///
    /// The test carries its own control, which is what makes it a proof rather
    /// than a tautology: the first pair is closed and then measured, and the
    /// measurement is non-zero, so the kernel really does hand a departed
    /// session's undelivered output to the next opener (on 7.0: all 8192 bytes, in
    /// reads of 4095/4095/2 — the `N_TTY_BUF_SIZE - 1` geometry). The second pair
    /// runs the same session, flushes, and measures again: the flush must report
    /// exactly what the control found, and the next opener must find nothing.
    ///
    /// A kernel that already discarded the queue at last close would report a zero
    /// control, and the test says so instead of failing — the flush is then a
    /// belt-and-braces no-op there. That arm is deliberate: this is precisely the
    /// kind of claim that differs between 6.18 and 7.0 (§13), and it is checked
    /// rather than assumed on whichever kernel runs it.
    #[test]
    fn a_sessions_undelivered_output_survives_last_close_and_the_flush_removes_it() {
        const BURST: usize = 8192;
        let mut buf = vec![0u8; READ_BUF];

        // --- Control: no flush. What would the next session inherit? ---
        let (control_master, control_pts) = pair();
        let control_wrote = session_that_never_reads(&control_master, &control_pts, BURST);
        assert!(
            control_wrote > 0,
            "the kernel accepted nothing for an attached client, so this test has \
             no subject"
        );
        let survived = flush_hostward_queue(&control_pts, &mut buf);

        // --- Treatment: the same session, flushed at last close. ---
        let (master, pts) = pair();
        let wrote = session_that_never_reads(&master, &pts, BURST);
        let flushed = flush_hostward_queue(&pts, &mut buf);
        let after = flush_hostward_queue(&pts, &mut buf);

        if survived == 0 {
            eprintln!(
                "NOTE: this kernel discards a pts's undelivered hostward queue at \
                 last close by itself; the §7.2 flush is a no-op here"
            );
        } else {
            assert_eq!(
                survived, control_wrote,
                "the whole undelivered burst outlives last close on this kernel; if \
                 that ever becomes a partial figure, the §7.2 flush's accounting \
                 needs re-measuring, not adjusting"
            );
        }
        assert_eq!(
            flushed, survived,
            "the flush must discard exactly what the next opener would otherwise \
             have read ({survived} bytes queued by a {wrote}-byte session) — §5 \
             counts loss where it happens, so a short count is loss going unnamed"
        );
        assert_eq!(
            after, 0,
            "the pair still holds {after} bytes after the flush: a fresh client \
             session would open onto the previous session's output (§7.2)"
        );
        // The flush reads the *slave*, so it must leave no **session evidence** on
        // the master. A `tcflush` here would leave a TIOCPKT_FLUSHREAD packet — one
        // readable byte — which the reader's next pass cannot tell from a client's
        // own evidence and which would re-arm the §15.36 last-close latch it just
        // cleared; so would running `make_drainable`'s `tcsetattr` on Linux, which
        // is why that arm is a no-op there.
        //
        // Asserted on the **read**, not on POLLIN, because "session evidence" is
        // defined by what the reader latches on — `read_and_poll`'s
        // `Ok(n) if n >= 1` arm, which sets `saw_session` whatever the packet says —
        // and because POLLIN is a §13 kernel-version-dependent claim the product
        // deliberately does not depend on (see that close block's anti-spin note).
        // Measured on Darwin 24.6.0: a pty master with no slave open reports
        // `POLLIN | POLLHUP` unconditionally and answers `read` with 0/EOF, where
        // Linux reports the hangup and answers EIO. Both mean "nothing to latch
        // on"; only the read says so on both. On Linux this is the *stronger*
        // check — a readable packet byte implies POLLIN, so nothing the poll form
        // caught escapes it, while a spurious POLLIN over an empty queue no longer
        // fails a daemon that is behaving correctly.
        //
        // **Which device the claim needs** (§5). The packet-evidence regressions
        // this discriminates on — the `tcflush` alternative, and deleting
        // `make_drainable`'s Linux no-op — are observable on Linux and not on BSD,
        // where every variant leaves the master answering `read -> 0` (measured).
        // What it still catches *here* is a flush that leaks its slave fd, which
        // leaves the pair attached and the master answering `Ok(1)` (measured,
        // byte 0x13). So the assertion is non-vacuous on both platforms; it is the
        // tcflush class specifically that only Linux can fail.
        let master_fd = master.as_raw_fd();
        let re = sys::poll_ready(master_fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        match sys::read_fd(master_fd, &mut buf) {
            // Nothing to latch on: BSD answers EOF on a hung-up master, Linux EIO,
            // and either answers EAGAIN when POLLIN was not set at all.
            Ok(0) => {}
            Err(e)
                if e.raw_os_error() == Some(libc::EIO)
                    || e.kind() == std::io::ErrorKind::WouldBlock => {}
            Ok(n) => panic!(
                "the flush left {n} readable byte(s) on the master (first = {:#04x}, \
                 revents {re:?}) — the reader's next pass latches `saw_session` on \
                 any read of n >= 1 and would take it for a new client session \
                 (§15.36)",
                buf[0]
            ),
            Err(e) => panic!("unexpected master read after the flush: {e} (revents {re:?})"),
        }
        assert!(
            re.contains(PollFlags::POLLHUP),
            "the flush's momentary slave open left the pair looking attached \
             ({re:?}); presence latching depends on the master hanging up again \
             (§7.2, nexus-doctor P2)"
        );
    }

    /// PTY-3: a hostward write to a *gone* peer reports the whole chunk as
    /// unwritten, so `writer_thread` charges it to `discarded_no_client` instead of
    /// losing it silently (§5).
    #[test]
    fn blocking_write_all_reports_the_full_remainder_when_the_peer_is_gone() {
        let (peer, sender) = UnixStream::pair().expect("socketpair");
        let wfd = sender.as_raw_fd();
        sys::set_nonblocking(wfd).expect("nonblocking");
        drop(peer); // the client detached

        let stop = AtomicBool::new(false);
        let payload = b"bytes for a client that already left";
        // A socketpair write to a closed peer raises EPIPE; if some kernel accepted
        // it instead there is no loss to count, which is why `Ok` is not a failure.
        if let Err(short) = blocking_write_all(wfd, payload, &stop) {
            assert_eq!(
                short.unwritten,
                payload.len(),
                "a write to a hung-up peer delivers nothing, so all of it is loss"
            );
        }
    }
}
