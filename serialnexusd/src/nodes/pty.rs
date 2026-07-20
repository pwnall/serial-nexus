//! PTY node (design §7.2). Faces target.
//!
//! Slice 1 built the pair: allocate the master/slave, set the baseline termios
//! (raw, echo off, EXTPROC on), enable packet mode on the master, install the
//! configured symlink (with the stale-dangling-symlink recovery rule), apply
//! owner/mode to the slave device node, and *prime* the slave by opening and
//! closing it once so POLLHUP reports "absent" for the never-opened case
//! (nexus-doctor P2 finding).
//!
//! Slice 2 (this) drives byte flow and presence, poll-based (not `AsyncFd`,
//! whose epoll readiness busy-loops on pty masters — implementation notes §3.10):
//!
//! * A read+presence task polls the master (`POLLIN | POLLHUP`, non-blocking):
//!   `POLLHUP` set ⇒ no client; `POLLIN` ⇒ drain, strip the packet-mode control
//!   byte, and forward only `TIOCPKT_DATA` payloads targetward (§7.2). It sleeps
//!   [`IDLE_POLL`] only when idle, so an active transfer streams at full rate. On
//!   last close it re-asserts the baseline termios (§7.2).
//! * A writer task drains the hostward channel to the master, **presence-gated**
//!   — written only while a client holds the slave, discarded otherwise (§7.2).

use std::cell::Cell;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::rc::Rc;

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nix::fcntl::{OFlag, open};
use nix::libc;
use nix::poll::PollFlags;
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{
    BaudRate, LocalFlags, SetArg, cfmakeraw, cfsetspeed, tcgetattr, tcsetattr,
};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::runtime::{self, DropCounters, IDLE_POLL, READ_BUF};
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
    /// Observed client presence, shared with the reader task. Single-threaded
    /// runtime, so a `Cell` (no atomics) suffices; `state` reads it at an await
    /// boundary where no task is mid-update.
    present: Rc<Cell<bool>>,
    /// Hostward drop counters for this boundary (§5), shared with the serial
    /// reader. Reports presence-gated discards and slow-consumer full-buffer
    /// drops in state. Defaulted at creation, replaced from the wiring at start.
    counters: Rc<DropCounters>,
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
            present: Rc::new(Cell::new(false)),
            counters: Rc::new(DropCounters::default()),
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
        counters: Option<Rc<DropCounters>>,
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
        // check that also gates the writer below.
        self.tasks.push(tokio::task::spawn_local(read_and_poll(
            master.clone(),
            self.present.clone(),
            targetward,
            self.advertised_baud,
        )));

        // Writer: serial → client (hostward), presence-gated.
        if let Some(rx) = hostward {
            self.tasks.push(tokio::task::spawn_local(write_hostward(
                master,
                self.present.clone(),
                self.counters.clone(),
                rx,
            )));
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
            "client_present": self.present.get(),
            // Hostward drops at this boundary (§5): bytes discarded while no
            // client held the slave, and bytes dropped because the client was
            // too slow to drain its bounded buffer.
            "discarded_no_client": self.counters.discarded_absent(),
            "dropped_slow_consumer": self.counters.dropped_full(),
        })
    }

    pub fn teardown(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
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
        for t in self.tasks.drain(..) {
            t.abort();
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
    present: Rc<Cell<bool>>,
    tx: Option<mpsc::Sender<Chunk>>,
    advertised_baud: u32,
) {
    let fd = master.as_raw_fd();
    let mut buf = vec![0u8; READ_BUF];
    loop {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let now = !re.contains(PollFlags::POLLHUP);
        let was = present.replace(now);
        if was && !now {
            // Last close: reset to baseline for a deterministic next session
            // (§7.2). No client is open, so this is safe.
            let _ = apply_baseline(master.as_fd(), advertised_baud);
        }

        let mut did = false;
        if now && re.contains(PollFlags::POLLIN) {
            loop {
                match sys::read_fd(fd, &mut buf) {
                    Ok(0) => {
                        present.set(false);
                        break;
                    }
                    Ok(n) => {
                        did = true;
                        // Packet mode: forward only TIOCPKT_DATA payloads; other
                        // leading bytes are control packets with no data (§7.2).
                        if n > 1 && buf[0] == sys::TIOCPKT_DATA {
                            if let Some(tx) = &tx {
                                let payload = Chunk::copy_from_slice(&buf[1..n]);
                                // Targetward: backpressure to the origin (await).
                                if tx.send(payload).await.is_err() {
                                    return; // serial gone
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    // A client that closed surfaces as EIO; mark absent.
                    Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                        present.set(false);
                        break;
                    }
                    Err(_) => break,
                }
            }
        }

        if !did {
            tokio::time::sleep(IDLE_POLL).await;
        }
    }
}

/// Writer task: drain the hostward channel to the master while a client is
/// present; discard otherwise (§7.2 presence-gated output). A full master buffer
/// backpressures the write within `write_all`; the feeding channel drops at
/// ingest (§5).
async fn write_hostward(
    master: Rc<PtyMaster>,
    present: Rc<Cell<bool>>,
    counters: Rc<DropCounters>,
    mut rx: mpsc::Receiver<Chunk>,
) {
    let fd = master.as_raw_fd();
    while let Some(chunk) = rx.recv().await {
        if present.get() {
            let _ = runtime::write_all(fd, &chunk).await;
        } else {
            // No client holds the slave — discard, counted at this boundary so
            // loss is visible and attributable (§5, §7.2 presence gating).
            counters.add_absent(chunk.len() as u64);
        }
    }
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
