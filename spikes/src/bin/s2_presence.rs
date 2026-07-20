#![deny(unsafe_code)]

//! S2 — PTY presence / POLLHUP semantics (design §7.2; plan phase 0).
//!
//! Presence detection is load-bearing: the PTY node emits hostward data only
//! while a client holds the slave open, and detects that via the master's HUP
//! condition. Questions:
//!
//! * Does the master report `POLLHUP` when no one holds the slave open — both
//!   after a close and (design claim) when the slave was never opened?
//! * Does `POLLHUP` clear when the slave is (re)opened, so a *polling* check
//!   detects attach (there is no un-HUP wakeup event)?
//! * Can the daemon reset termios through the **master** with no slave open
//!   (the last-close baseline reset of §7.2)?
//! * How cheap is a zero-timeout HUP-status poll (it runs on a sub-second
//!   cadence to catch silent openers like `cat`)?
//!
//! Self-judging: one JSON verdict line; exits nonzero if the detection
//! mechanisms the design relies on do not hold.

use std::os::fd::AsFd;
use std::time::Instant;

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serde_json::json;

fn main() {
    let verdict = run();
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

/// Poll the master for HUP with a zero timeout; return whether POLLHUP is set.
fn hup(master: &PtyMaster) -> anyhow::Result<bool> {
    let mut fds = [PollFd::new(master.as_fd(), PollFlags::POLLHUP)];
    poll(&mut fds, PollTimeout::ZERO)?;
    Ok(fds[0]
        .revents()
        .unwrap_or(PollFlags::empty())
        .contains(PollFlags::POLLHUP))
}

fn new_master() -> anyhow::Result<PtyMaster> {
    let m = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&m)?;
    unlockpt(&m)?;
    Ok(m)
}

fn run() -> serde_json::Value {
    match exercise() {
        Ok(o) => {
            // The detection the design actually depends on: HUP after close,
            // clear HUP while open, HUP clears again on reopen, and termios
            // settable with no slave. `never_opened` is reported for the record.
            let pass = o.hup_after_close
                && !o.hup_while_open
                && !o.hup_after_reopen
                && o.termios_settable_without_slave;
            json!({
                "tool": "s2_presence",
                "spike": "S2",
                "kernel": std::fs::read_to_string("/proc/sys/kernel/osrelease")
                    .map(|s| s.trim().to_owned()).unwrap_or_default(),
                "hup_when_never_opened": o.hup_when_never_opened,
                "hup_while_open": o.hup_while_open,
                "hup_after_close": o.hup_after_close,
                "hup_after_reopen": o.hup_after_reopen,
                "termios_settable_without_slave": o.termios_settable_without_slave,
                "zero_timeout_poll_ns_median": o.zero_timeout_poll_ns_median,
                "designed": {
                    "hup_while_open": false,
                    "hup_after_close": true,
                    "hup_after_reopen": false
                },
                "pass": pass
            })
        }
        Err(e) => {
            json!({"tool": "s2_presence", "spike": "S2", "error": e.to_string(), "pass": false})
        }
    }
}

struct Observations {
    hup_when_never_opened: bool,
    hup_while_open: bool,
    hup_after_close: bool,
    hup_after_reopen: bool,
    termios_settable_without_slave: bool,
    zero_timeout_poll_ns_median: u128,
}

fn exercise() -> anyhow::Result<Observations> {
    // Case: a master whose slave was never opened.
    let never = new_master()?;
    let hup_when_never_opened = hup(&never)?;
    drop(never);

    // The main pair, whose slave we open, close, and reopen.
    let master = new_master()?;
    let pts = ptsname_r(&master)?;

    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let hup_while_open = hup(&master)?;

    drop(slave);
    let hup_after_close = hup(&master)?;

    let slave2 = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let hup_after_reopen = hup(&master)?;
    drop(slave2);

    // Last-close baseline reset (§7.2): set termios through the master while no
    // slave is open, and confirm it takes.
    let termios_settable_without_slave = {
        let mut t = tcgetattr(&master)?;
        cfmakeraw(&mut t);
        t.local_flags.remove(LocalFlags::ECHO);
        t.local_flags.insert(LocalFlags::EXTPROC);
        tcsetattr(&master, SetArg::TCSANOW, &t).is_ok()
            && tcgetattr(&master)?
                .local_flags
                .contains(LocalFlags::EXTPROC)
    };

    // Cost of the zero-timeout HUP check. Median over many iterations.
    let iters = 4096;
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = hup(&master)?;
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let zero_timeout_poll_ns_median = samples[samples.len() / 2];

    Ok(Observations {
        hup_when_never_opened,
        hup_while_open,
        hup_after_close,
        hup_after_reopen,
        termios_settable_without_slave,
        zero_timeout_poll_ns_median,
    })
}
