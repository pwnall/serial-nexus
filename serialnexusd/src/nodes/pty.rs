//! PTY node (design §7.2). Faces target.
//!
//! Slice 1: creation only — allocate the master/slave pair, set the baseline
//! termios (raw, echo off, EXTPROC on), enable packet mode on the master,
//! install the configured symlink (with the stale-dangling-symlink recovery
//! rule), apply owner/mode to the slave device node, and *prime* the slave by
//! opening and closing it once so POLLHUP reports "absent" for the never-opened
//! case (nexus-doctor P2 finding). Presence detection and byte flow land in
//! slice 2.

use std::os::fd::AsRawFd;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use nexus_core::NodeStatus;
use nexus_core::config::NodeConfig;
use nix::fcntl::{OFlag, open};
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{
    BaudRate, LocalFlags, SetArg, cfmakeraw, cfsetspeed, tcgetattr, tcsetattr,
};
use serde_json::json;

use crate::sys;

pub struct PtyNode {
    pub name: String,
    path: PathBuf,
    mode: u32,
    group: Option<String>,
    advertised_baud: u32,
    master: Option<PtyMaster>,
    pts_path: Option<String>,
    symlink_installed: bool,
    status: NodeStatus,
}

impl PtyNode {
    pub fn create(config: &NodeConfig) -> PtyNode {
        let NodeConfig::Pty {
            name,
            path,
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
            group: group.clone(),
            advertised_baud: *advertised_baud,
            master: None,
            pts_path: None,
            symlink_installed: false,
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

        self.apply_baseline(&master)?;
        sys::set_packet_mode(master.as_raw_fd(), true).map_err(|e| format!("TIOCPKT: {e}"))?;

        self.install_symlink(&pts)?;
        self.symlink_installed = true;
        self.apply_perms(&pts)?;
        prime_slave(&pts);

        self.master = Some(master);
        self.pts_path = Some(pts);
        Ok(())
    }

    /// Baseline termios (§7.2): raw + echo off + EXTPROC on, applied through the
    /// master, plus the cosmetic advertised baud.
    fn apply_baseline(&self, master: &PtyMaster) -> Result<(), String> {
        let mut t = tcgetattr(master).map_err(|e| format!("tcgetattr: {e}"))?;
        cfmakeraw(&mut t);
        t.local_flags.remove(LocalFlags::ECHO);
        t.local_flags.insert(LocalFlags::EXTPROC);
        if let Some(baud) = standard_baud(self.advertised_baud) {
            let _ = cfsetspeed(&mut t, baud);
        }
        tcsetattr(master, SetArg::TCSANOW, &t).map_err(|e| format!("tcsetattr: {e}"))
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

    /// Apply mode (and group, if configured) to the slave device node — what
    /// gates open(2) (§7.2).
    fn apply_perms(&self, pts: &str) -> Result<(), String> {
        std::fs::set_permissions(pts, std::os::unix::fs::PermissionsExt::from_mode(self.mode))
            .map_err(|e| format!("chmod {pts}: {e}"))?;
        if let Some(group) = &self.group {
            let gid = nix::unistd::Group::from_name(group)
                .ok()
                .flatten()
                .map(|g| g.gid)
                .ok_or_else(|| format!("group {group} not found"))?;
            nix::unistd::chown(pts, None, Some(gid)).map_err(|e| format!("chown {pts}: {e}"))?;
        }
        Ok(())
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        json!({
            "pts_path": self.pts_path,
            "symlink": self.path.display().to_string(),
            "advertised_baud": self.advertised_baud,
            // Presence detection lands in slice 2; reported false until then.
            "client_present": false,
        })
    }

    pub fn teardown(&mut self) {
        if self.symlink_installed {
            let _ = std::fs::remove_file(&self.path);
            self.symlink_installed = false;
        }
        self.master = None;
    }
}

impl Drop for PtyNode {
    fn drop(&mut self) {
        // The symlink is our artifact; unlink it on removal / clean shutdown.
        if self.symlink_installed {
            let _ = std::fs::remove_file(&self.path);
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
