#![deny(unsafe_code)]

//! S1 — EXTPROC / TIOCPKT (design §7.2, §15.14; plan phase 0).
//!
//! The design's single most-flagged mechanism. Questions, answered against the
//! running kernel:
//!
//! 1. With `EXTPROC` set on the slave and packet mode (`TIOCPKT`) on the master,
//!    does a client `tcsetattr` on the slave surface as a `TIOCPKT_IOCTL`
//!    control packet on the master? (This is how the daemon *observes* client
//!    termios changes — §7.2.)
//! 2. Does *clearing* `EXTPROC` (a client rebuilding termios from scratch)
//!    produce a final control packet?
//! 3. Can the daemon re-assert `EXTPROC` through the **master** fd afterward?
//!
//! Self-judging: prints one JSON verdict line; exits nonzero if the observed
//! behavior contradicts the design (a stop condition — the fallback is §7.2's
//! reconciliation poll, recorded as a §15 amendment).

use std::io::Read;
use std::os::fd::{AsFd, AsRawFd};

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serde_json::json;

/// Raw ioctls nix does not wrap. Isolated with a localized unsafe allowance,
/// exactly as the daemon's `sys` module will be (§2).
mod sys {
    #![allow(unsafe_code)]
    use nix::libc;
    use std::os::fd::RawFd;

    nix::ioctl_write_ptr_bad!(tiocpkt, libc::TIOCPKT, libc::c_int);

    /// Enable or disable packet mode on a pty master.
    pub fn set_packet_mode(fd: RawFd, on: bool) -> nix::Result<()> {
        let v: libc::c_int = i32::from(on);
        // Safety: `v` outlives the call; TIOCPKT reads one int through the ptr.
        unsafe { tiocpkt(fd, &v) }?;
        Ok(())
    }

    /// Packet-mode control-byte flag for a termios/ioctl change on the slave.
    /// Stable Linux value; not exported by libc 0.2.186 (only the `TIOCPKT`
    /// request code is), so it is spelled out here.
    pub const TIOCPKT_IOCTL: u8 = 64;
}

fn main() {
    let verdict = run();
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

fn kernel() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| "unknown".into())
}

fn run() -> serde_json::Value {
    match exercise() {
        Ok(obs) => {
            let pass = obs.ioctl_packet_on_tcsetattr && obs.reassert_extproc_via_master;
            json!({
                "tool": "s1_extproc",
                "spike": "S1",
                "kernel": kernel(),
                "question": "EXTPROC+TIOCPKT: does slave tcsetattr surface as a master ioctl packet, and can the master re-assert EXTPROC?",
                "ioctl_packet_on_tcsetattr": obs.ioctl_packet_on_tcsetattr,
                "clear_extproc_produces_packet": obs.clear_extproc_produces_packet,
                "reassert_extproc_via_master": obs.reassert_extproc_via_master,
                "designed": {
                    "ioctl_packet_on_tcsetattr": true,
                    "reassert_extproc_via_master": true
                },
                "pass": pass,
                "note": if pass { "EXTPROC packet-mode observation works as designed" }
                        else { "EXTPROC mechanism deviates — fall back to §7.2 reconciliation poll" }
            })
        }
        Err(e) => json!({
            "tool": "s1_extproc",
            "spike": "S1",
            "kernel": kernel(),
            "error": e.to_string(),
            "pass": false
        }),
    }
}

struct Observations {
    ioctl_packet_on_tcsetattr: bool,
    clear_extproc_produces_packet: bool,
    reassert_extproc_via_master: bool,
}

/// Drain any pending master packets within a short budget, returning the
/// control byte (`buf[0]`) of each packet read. In packet mode every master
/// read begins with a `TIOCPKT_*` control byte (§7.2, verified in research).
fn drain(master: &mut nix::pty::PtyMaster, budget_ms: u16) -> anyhow::Result<Vec<u8>> {
    let mut seen = Vec::new();
    loop {
        let mut fds = [PollFd::new(
            master.as_fd(),
            PollFlags::POLLIN | PollFlags::POLLPRI,
        )];
        let n = poll(&mut fds, PollTimeout::from(budget_ms))?;
        if n == 0 {
            break;
        }
        let revents = fds[0].revents().unwrap_or(PollFlags::empty());
        if !revents.intersects(PollFlags::POLLIN | PollFlags::POLLPRI) {
            break;
        }
        let mut buf = [0u8; 256];
        let got = master.read(&mut buf)?;
        if got == 0 {
            break;
        }
        seen.push(buf[0]);
    }
    Ok(seen)
}

fn exercise() -> anyhow::Result<Observations> {
    // Allocate a master/slave pair the POSIX way, so we hold both fds (§7.2).
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let slave_path = ptsname_r(&master)?;
    let slave = open(
        slave_path.as_str(),
        OFlag::O_RDWR | OFlag::O_NOCTTY,
        Mode::empty(),
    )?;

    let mut master = master;

    // Baseline termios the daemon applies: raw + echo off + EXTPROC on (§7.2).
    let mut base = tcgetattr(&slave)?;
    cfmakeraw(&mut base);
    base.local_flags.remove(LocalFlags::ECHO);
    base.local_flags.insert(LocalFlags::EXTPROC);
    tcsetattr(&slave, SetArg::TCSANOW, &base)?;

    // Turn on packet mode and clear whatever the baseline set generated.
    sys::set_packet_mode(master.as_raw_fd(), true)?;
    let _ = drain(&mut master, 100)?;

    // (1) A "client" tcsetattr on the slave. Changing a benign control char is
    // enough to trigger the slave-side TCSETS the daemon wants to observe.
    let mut client = tcgetattr(&slave)?;
    client.control_chars[nix::libc::VMIN] = 4;
    tcsetattr(&slave, SetArg::TCSANOW, &client)?;
    let packets = drain(&mut master, 500)?;
    let ioctl_packet_on_tcsetattr = packets
        .iter()
        .any(|b| b & sys::TIOCPKT_IOCTL == sys::TIOCPKT_IOCTL);

    // (2) The client rebuilds termios from scratch, clearing EXTPROC.
    let mut cleared = tcgetattr(&slave)?;
    cleared.local_flags.remove(LocalFlags::EXTPROC);
    tcsetattr(&slave, SetArg::TCSANOW, &cleared)?;
    let clear_packets = drain(&mut master, 500)?;
    let clear_extproc_produces_packet = clear_packets
        .iter()
        .any(|b| b & sys::TIOCPKT_IOCTL == sys::TIOCPKT_IOCTL);

    // (3) The daemon re-asserts EXTPROC through the master fd.
    let mut viamaster = tcgetattr(&master)?;
    viamaster.local_flags.insert(LocalFlags::EXTPROC);
    tcsetattr(&master, SetArg::TCSANOW, &viamaster)?;
    let after = tcgetattr(&slave)?;
    let reassert_extproc_via_master = after.local_flags.contains(LocalFlags::EXTPROC);

    Ok(Observations {
        ioctl_packet_on_tcsetattr,
        clear_extproc_produces_packet,
        reassert_extproc_via_master,
    })
}
