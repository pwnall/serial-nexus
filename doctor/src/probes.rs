//! The capability probes (design §15.17, plan §3): P1 EXTPROC/TIOCPKT, P2 PTY
//! presence, P3 serial-port fit, P4 by-id resolution, P5 rig certification,
//! P6/P7 — the two pty last-close measurements the §6/§7.2 write-lock lifecycle
//! rests on — and P8..P11, the kernel-behaviour measurements the *data plane's*
//! shape rests on: P8 epoll-vs-`read(2)` on a pty master (invariant 1), P9
//! `poll(2)` timeout granularity (§15.19's timer floor), P10 pty buffer depth
//! (the `hostward_buffer` defaults), P11 real-port line-state ioctls (§5/§7.1),
//! and P12, P7's sibling — the session-boundary *edge* (§15.39), which carries
//! §6's detach-release on Darwin and is inert on Linux, where the retained packet
//! P7 measures carries it instead.
//! Each returns a self-judging [`Probe`]. The kernel probes (P1, P2, P6, P7, P8,
//! P9, P10, P12) and the resolver probe (P4) are passive and always safe to run;
//! P3, P5 and P11 open a real serial port and therefore run only on an explicitly
//! named `--port`.
//!
//! **P6..P12 exist to be diffed across kernels.** Production runs Linux 6.18;
//! this tree was developed on 7.0, and §13 forbids a one-way decision resting on
//! 7.0-only evidence. Each probe therefore emits its *raw measurements* — pass
//! counts, poll flags, read outcomes, packet bytes in hex, microseconds, byte
//! depths — not just a verdict word, so `serial-nexus-doctor --json` from a 6.18 box and
//! a 7.0 box can be diffed and the disagreement, if any, read straight off the
//! numbers. **Prose in the Markdown arm is a bonus; the JSON is the artifact.**
//!
//! None of them can report `unsupported`, and the rule behind that is worth
//! stating once: `unsupported` means a design premise is contradicted *with no
//! fallback*, and it is a live gate (`expectations/linux.jq` requires
//! `.summary.unsupported == 0`, and `itest/tests/meta_gates.rs` asserts the
//! same), so a probe that reddens a healthy box is a bug and not a finding. Every
//! answer these probes can return is legitimate kernel behaviour the shipped code
//! is already correct under — what varies is whether a *pending simplification* is
//! safe (P6), whether a latch covers a realistic session shape (P7), whether a
//! design's justification reproduces here (P8), and what numbers a tuning
//! decision should be made against (P9, P10, P11). "This kernel behaves
//! differently from the dev box" is `degraded` with the observation named, or
//! `skipped` where the mechanism does not exist at all — never `unsupported`.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

use crate::report::{EnvCheck, Probe, Status};
use serial_nexus_sys as sys;

const CUSTOM_BAUD: u32 = 250_000;

// ---------------------------------------------------------------------------
// P1 — EXTPROC / TIOCPKT signaling (§7.2, §15.14)
// ---------------------------------------------------------------------------

pub fn p1_extproc() -> Probe {
    let p = Probe::new(
        "P1",
        "EXTPROC / TIOCPKT signaling",
        "Does a client tcsetattr surface as a TIOCPKT_IOCTL packet on the master; does clearing EXTPROC emit a final packet; can the master re-assert EXTPROC?",
    );
    match p1_inner() {
        Ok((ioctl_packet, clear_packet, reassert)) => {
            let p = p
                .observe("ioctl_packet_on_tcsetattr", ioctl_packet)
                .observe("clear_extproc_produces_packet", clear_packet)
                .observe("reassert_extproc_via_master", reassert);
            if ioctl_packet && reassert {
                p.verdict(
                    Status::Supported,
                    "EXTPROC packet-mode observation is primary; the §7.2 reconciliation poll is only a backstop.",
                )
            } else {
                p.verdict(
                    Status::Degraded,
                    "EXTPROC notification incomplete → §7.2 runs poll-only; client-termios observation latency degrades, nothing else.",
                )
            }
        }
        Err(e) => p.verdict(
            Status::Degraded,
            &format!("probe error ({e}) → assume poll-only observation (§7.2)."),
        ),
    }
}

fn p1_inner() -> anyhow::Result<(bool, bool, bool)> {
    let mut master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;

    let mut base = tcgetattr(&slave)?;
    cfmakeraw(&mut base);
    base.local_flags.remove(LocalFlags::ECHO);
    base.local_flags.insert(LocalFlags::EXTPROC);
    tcsetattr(&slave, SetArg::TCSANOW, &base)?;

    sys::set_packet_mode(master.as_raw_fd(), true)?;
    let _ = drain(&mut master, 100)?;

    let mut client = tcgetattr(&slave)?;
    client.control_chars[nix::libc::VMIN] = 4;
    tcsetattr(&slave, SetArg::TCSANOW, &client)?;
    let ioctl_packet = drain(&mut master, 500)?
        .iter()
        .any(|b| b & sys::TIOCPKT_IOCTL == sys::TIOCPKT_IOCTL);

    let mut cleared = tcgetattr(&slave)?;
    cleared.local_flags.remove(LocalFlags::EXTPROC);
    tcsetattr(&slave, SetArg::TCSANOW, &cleared)?;
    let clear_packet = drain(&mut master, 500)?
        .iter()
        .any(|b| b & sys::TIOCPKT_IOCTL == sys::TIOCPKT_IOCTL);

    let mut viamaster = tcgetattr(&master)?;
    viamaster.local_flags.insert(LocalFlags::EXTPROC);
    tcsetattr(&master, SetArg::TCSANOW, &viamaster)?;
    let reassert = tcgetattr(&slave)?.local_flags.contains(LocalFlags::EXTPROC);

    Ok((ioctl_packet, clear_packet, reassert))
}

fn drain(master: &mut PtyMaster, budget_ms: u16) -> anyhow::Result<Vec<u8>> {
    let mut seen = Vec::new();
    loop {
        let mut fds = [PollFd::new(
            master.as_fd(),
            PollFlags::POLLIN | PollFlags::POLLPRI,
        )];
        if poll(&mut fds, PollTimeout::from(budget_ms))? == 0 {
            break;
        }
        let revents = fds[0].revents().unwrap_or_else(PollFlags::empty);
        if !revents.intersects(PollFlags::POLLIN | PollFlags::POLLPRI) {
            break;
        }
        let mut buf = [0u8; 256];
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => seen.push(buf[0]),
            Err(_) => break,
        }
    }
    Ok(seen)
}

// ---------------------------------------------------------------------------
// P2 — PTY presence / POLLHUP semantics (§7.2)
// ---------------------------------------------------------------------------

pub fn p2_presence() -> Probe {
    let p = Probe::new(
        "P2",
        "PTY presence / POLLHUP semantics",
        "Does the master report POLLHUP only when no client holds the slave; does HUP clear on reopen; is termios settable with no slave open?",
    );
    match p2_inner() {
        Ok(o) => {
            let p = p
                .observe("hup_when_never_opened", o.never_opened)
                .observe("hup_while_open", o.while_open)
                .observe("hup_after_close", o.after_close)
                .observe("hup_after_reopen", o.after_reopen)
                .observe("termios_settable_without_slave", o.termios_settable)
                .observe("zero_timeout_poll_ns_median", o.poll_ns);
            // Core presence: POLLHUP must be absent while a slave is open, present
            // after it closes, and clear again on reopen. These are the signals the
            // data plane gates on.
            //
            // `never_opened` is recorded but does NOT gate the verdict: NO mainstream
            // kernel — Linux included (§3.2, and verified on 7.0: a never-opened master
            // does not HUP) — reliably HUPs a never-opened master, so the node primes
            // the slave (open+close at creation) to seed "absent" on every platform.
            // Priming is a universal refinement, not a platform arm. (An earlier fix
            // wrongly gated Supported on `never_opened` being true, which no Linux
            // satisfies, so it demoted native Linux to Degraded — the §15.30 "predicted
            // ≠ verified" trap, caught by running on real Linux hardware.)
            //
            // The one genuine platform split is `termios_settable`: the Linux master is
            // a terminal, so the baseline is applied through it (Supported); a BSD/macOS
            // master is not (ENOTTY), so the node applies the baseline via the slave and
            // re-asserts it on the client's rising edge (Degraded, still fully
            // functional). See serial-nexus-daemon `nodes::pty::with_termios_fd`.
            let core_presence = o.after_close && !o.while_open && !o.after_reopen;
            if core_presence && o.termios_settable {
                p.verdict(
                    Status::Supported,
                    "POLLHUP presence detection works; the master is a terminal (baseline applied natively), and the node primes the slave (open+close at creation) for the never-opened case.",
                )
            } else if core_presence {
                p.verdict(
                    Status::Degraded,
                    "POLLHUP presence works once the slave is primed, but this kernel's master is not a terminal (ENOTTY): the baseline is applied via the slave and re-asserted on the client's rising edge (§7.2). The PTY node handles it (§13); presence-gated output is available.",
                )
            } else {
                p.verdict(
                    Status::Unsupported,
                    "PTY presence via POLLHUP does not behave as the design assumes — presence-gated output is unavailable on this kernel.",
                )
            }
        }
        Err(e) => p.verdict(Status::Unsupported, &format!("probe error: {e}")),
    }
}

struct Presence {
    never_opened: bool,
    while_open: bool,
    after_close: bool,
    after_reopen: bool,
    termios_settable: bool,
    poll_ns: u64,
}

fn p2_inner() -> anyhow::Result<Presence> {
    let never = new_master()?;
    let never_opened = hup(&never)?;
    drop(never);

    let master = new_master()?;
    let pts = sys::ptsname(&master)?;

    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let while_open = hup(&master)?;
    drop(slave);
    let after_close = hup(&master)?;
    let slave2 = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let after_reopen = hup(&master)?;
    drop(slave2);

    let termios_settable = {
        let mut t = tcgetattr(&master)?;
        cfmakeraw(&mut t);
        t.local_flags.insert(LocalFlags::EXTPROC);
        tcsetattr(&master, SetArg::TCSANOW, &t).is_ok()
            && tcgetattr(&master)?
                .local_flags
                .contains(LocalFlags::EXTPROC)
    };

    let iters = 4096;
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = hup(&master)?;
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();

    Ok(Presence {
        never_opened,
        while_open,
        after_close,
        after_reopen,
        termios_settable,
        poll_ns: samples[samples.len() / 2] as u64,
    })
}

fn hup(master: &PtyMaster) -> anyhow::Result<bool> {
    let mut fds = [PollFd::new(master.as_fd(), PollFlags::POLLHUP)];
    poll(&mut fds, PollTimeout::ZERO)?;
    Ok(fds[0]
        .revents()
        .unwrap_or_else(PollFlags::empty)
        .contains(PollFlags::POLLHUP))
}

fn new_master() -> anyhow::Result<PtyMaster> {
    let m = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&m)?;
    unlockpt(&m)?;
    Ok(m)
}

// ---------------------------------------------------------------------------
// Shared pty machinery for P6/P7 (both measure a packet-mode master across a
// client session's collapse, so they share the setup, the sampler and the
// labels — a probe's numbers are only diffable across kernels if both kernels
// were measured by the same code).
// ---------------------------------------------------------------------------

/// The §7.2 baseline termios the PTY node applies to every pair it owns — raw,
/// echo off, **EXTPROC set** — mirroring `serial-nexus-daemon::nodes::pty`'s
/// `apply_baseline_fd`. Applied through whichever fd this platform lets us: the
/// Linux master is a terminal and carries it; a BSD/macOS master answers
/// `ENOTTY`, so the node applies it through a momentarily-opened slave (see
/// `with_termios_fd`, and P2's `termios_settable`).
///
/// **EXTPROC is not decoration here, it is the mechanism under test.** Linux's
/// `pty_set_termios` raises `TIOCPKT_IOCTL` on the master only when EXTPROC is
/// set in the old or the new termios (or when the IXON flow-control state
/// changes) — which is why the design's baseline sets it (§7.2) and why P1 probes
/// it. Measuring P7 against a baseline *without* EXTPROC reports "a collapsed
/// termios-only session leaves nothing readable" on a kernel where it plainly
/// does: a false `degraded`, and precisely the wrong answer to carry to 6.18.
fn set_baseline<Fd: AsFd>(fd: &Fd) -> nix::Result<()> {
    let mut t = tcgetattr(fd)?;
    cfmakeraw(&mut t);
    t.local_flags.remove(LocalFlags::ECHO);
    t.local_flags.insert(LocalFlags::EXTPROC);
    tcsetattr(fd, SetArg::TCSANOW, &t)
}

/// Apply the baseline the way the node does at pty *creation*: through the
/// master, falling back to a momentary slave open. **Never call this after the
/// last close when the hangup itself is what is being measured** — the fallback
/// opens a slave, which clears the hangup. Use [`set_baseline`] on the master
/// directly there (that is what `handle_last_close` does).
/// Returns **which arm ran**: `true` if the master carried the baseline, `false` if
/// the momentary-slave fallback was needed. Callers report it, because it is the
/// measured form of a fact this file used to take from a `cfg` — and because the two
/// facts it is easily confused with are independent of it and of each other. A master
/// that accepts the baseline does not imply the pair still carries it when the client
/// opens (a kernel that re-initialises pts termios at open resets it either way), and
/// a `tcsetattr` that returns `Ok` does not imply EXTPROC was retained. So: report
/// this, key nothing on it. See [`arm_client_baseline`].
fn apply_pty_baseline(master: &PtyMaster, pts: &str) -> anyhow::Result<bool> {
    if set_baseline(&master).is_ok() {
        return Ok(true);
    }
    let slave = open(pts, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    set_baseline(&slave)?;
    Ok(false)
}

/// Is this fd's line discipline the raw baseline the daemon runs, or a cooked one?
///
/// **Why a probe has to report this rather than assume it.** [`apply_pty_baseline`]
/// applies the baseline through the master where the master is a terminal, and
/// otherwise through a slave it opens *and immediately closes*. On a kernel where
/// the master is not a terminal — Darwin, measured by P2's
/// `termios_settable_without_slave: false` — that momentary set does not survive to
/// the next open: `nodes/pty.rs` says so in its own words at the non-Linux re-assert
/// ("the slave's termios resets to cooked when the last slave fd closes … a momentary
/// daemon-side set does not survive to the client's open"), which is why the *node*
/// re-asserts on the client's rising presence edge. Every probe here that opens a
/// fresh slave after the baseline therefore measures whatever the kernel reset it to,
/// and nothing in the report said which that was.
///
/// That is not a detail. Measured on Linux 7.0.0-29, filling a pty hostward with the
/// same bytes: **raw** accepts ~13.8 KiB and every byte is recoverable by the peer;
/// **cooked** accepts ~23.5 KiB and *none of it* is recoverable. Same kernel, same
/// probe, opposite answers — so a depth reported without its mode is not a
/// cross-kernel measurement, it is two measurements wearing one name.
fn termios_mode<Fd: AsFd>(fd: &Fd) -> &'static str {
    match tcgetattr(fd) {
        Ok(t) => {
            let cooked = t.local_flags.contains(LocalFlags::ICANON)
                || t.local_flags.contains(LocalFlags::ECHO);
            if cooked { "cooked" } else { "raw" }
        }
        Err(_) => "unknown",
    }
}

/// The pair's actual configuration at the moment a probe's measured session ran,
/// and what putting it there cost on the master.
struct ClientBaseline {
    via_master: bool,
    reasserted: bool,
    mode: &'static str,
    extproc: bool,
    footprint_bytes: u64,
}

impl ClientBaseline {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "baseline_via_master": self.via_master,
            "reasserted_on_client_slave": self.reasserted,
            "slave_termios_mode": self.mode,
            "extproc_set_at_shape": self.extproc,
            "baseline_packet_bytes_drained": self.footprint_bytes,
        })
    }
}

/// Put the pair into the daemon's §7.2 baseline **on the client slave this probe is
/// about to measure**, then consume what that re-assert made readable on the master.
///
/// This is the P6/P7 analogue of the repair P10 carries, and it needs one thing P10
/// did not: a drain. P10's re-assert is invisible in P10's numbers because P10 counts
/// bytes it writes itself. P6 and P7 count *whatever is readable on the master*, so on
/// a kernel that emits `TIOCPKT_IOCTL` for an EXTPROC `tcsetattr` the re-assert is
/// itself readable, and re-asserting without draining makes the probe count its own
/// footprint as the session's evidence. Measured on Linux 7.0.0-29 (byte-identical
/// across 5 runs): re-asserting without a drain moves P7's `a_open_close` from **0
/// bytes to 1** (`0x40`) and `c_open_write_close` from **2 to 3** — the first of those
/// inverting the one shape the §6 argument relies on leaving *nothing* behind. With
/// this drain all three shapes read exactly what they read before the repair (0/1/2).
/// The footprint is *reported* rather than merely discarded, so that invariance is
/// auditable in the JSON instead of asserted in a comment.
///
/// It is the same obligation `nodes/pty.rs` discharges after `handle_last_close` —
/// consume what the handler's own termios reset left behind — arriving here for the
/// same reason: a `tcsetattr` on this pair is loud, and a reader that does not account
/// for its own noise reads it as the peer's.
///
/// Applied on every platform, never behind a `cfg` and never keyed on `via_master`.
/// A repair that only ever executes off the platform of record is a §9 proxy in space,
/// exercised nowhere it can be observed failing — and keying on the master arm would
/// additionally be *wrong*, because a master that accepts the baseline does not imply
/// the pair still carries it when the client opens. Those are two facts, not one.
fn arm_client_baseline<Fd: AsFd>(
    master_fd: RawFd,
    slave: &Fd,
    via_master: bool,
    buf: &mut [u8],
) -> ClientBaseline {
    let reasserted = set_baseline(slave).is_ok();
    std::thread::sleep(PTY_SETTLE);
    let (footprint_bytes, _, _, _) = read_available(master_fd, buf, 64);
    ClientBaseline {
        via_master,
        reasserted,
        mode: termios_mode(slave),
        extproc: extproc_set(slave),
        footprint_bytes,
    }
}

/// Did the kernel **retain** EXTPROC on this fd? A different question from "did
/// `tcsetattr` succeed": a kernel may accept the flag and drop it, and that is exactly
/// the shape that makes a `TIOCPKT_IOCTL`-based measurement silently unanswerable
/// while every syscall in sight returns `Ok` (P1's `reassert_extproc_via_master`).
fn extproc_set<Fd: AsFd>(fd: &Fd) -> bool {
    tcgetattr(fd)
        .map(|t| t.local_flags.contains(LocalFlags::EXTPROC))
        .unwrap_or(false)
}

/// Why a P7 shape was silent — the two causes are different findings and only the
/// first is repairable by a termios call. Measured discriminator (Linux 7.0.0-29):
/// planting a baseline with no EXTPROC silences the termios shape and leaves the
/// write shape at 2 bytes; a fully cooked pair also leaves it at 2. So a write shape
/// that is *also* silent is not a line-discipline finding at all.
fn p7_silence_cause(termios_bytes: u64, write_bytes: u64, extproc: bool) -> &'static str {
    if termios_bytes > 0 {
        "covered"
    } else if write_bytes == 0 {
        "hangup-destroys-evidence"
    } else if !extproc {
        "extproc-unavailable"
    } else {
        "latch-uncovered"
    }
}

/// Name a `revents` bitmask stably (`POLLIN|POLLHUP`), and name any bit this
/// table does not know rather than dropping it — an unknown bit on the
/// production kernel is exactly the kind of thing this report exists to surface.
fn revents_label(f: PollFlags) -> String {
    const KNOWN: [(PollFlags, &str); 6] = [
        (PollFlags::POLLIN, "POLLIN"),
        (PollFlags::POLLPRI, "POLLPRI"),
        (PollFlags::POLLOUT, "POLLOUT"),
        (PollFlags::POLLERR, "POLLERR"),
        (PollFlags::POLLHUP, "POLLHUP"),
        (PollFlags::POLLNVAL, "POLLNVAL"),
    ];
    let mut parts: Vec<String> = Vec::new();
    let mut named = PollFlags::empty();
    for (flag, name) in KNOWN {
        if f.contains(flag) {
            parts.push((*name).to_owned());
            named |= flag;
        }
    }
    let residual = f.bits() & !named.bits();
    if residual != 0 {
        parts.push(format!("0x{:x}", residual as u16));
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join("|")
    }
}

/// Classify one `read(2)` on the master: the outcome *class* (what a poll loop
/// would branch on) and the bytes it produced.
fn read_class(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::WouldBlock {
        return "EAGAIN".to_owned();
    }
    match e.raw_os_error() {
        Some(libc::EIO) => "EIO".to_owned(),
        Some(libc::EAGAIN) => "EAGAIN".to_owned(),
        Some(n) => format!("errno:{n}"),
        None => "error".to_owned(),
    }
}

/// Read everything the master will hand over right now (the fd is non-blocking),
/// bounded by `max_reads`. Returns the bytes, the read count, the **leading byte
/// of every read** in hex — the packet-mode control byte, which is the whole
/// finding in P7 — and the label of the read that ended the drain.
fn read_available(fd: RawFd, buf: &mut [u8], max_reads: u32) -> (u64, u64, Vec<String>, String) {
    let mut bytes = 0u64;
    let mut reads = 0u64;
    let mut leading = Vec::new();
    for _ in 0..max_reads {
        match sys::read_fd(fd, buf) {
            Ok(0) => return (bytes, reads, leading, "eof".to_owned()),
            Ok(n) => {
                bytes += n as u64;
                reads += 1;
                leading.push(format!("0x{:02x}", buf[0]));
            }
            Err(e) => return (bytes, reads, leading, read_class(&e)),
        }
    }
    (bytes, reads, leading, "capped".to_owned())
}

/// What repeated readiness passes on the master observed. Every field is
/// reported: the verdict reads one of them, a kernel diff reads them all.
#[derive(Default)]
struct Readiness {
    passes: u64,
    pollin: u64,
    pollhup: u64,
    /// The spin signature: a pass where `poll(2)` said POLLIN and the following
    /// `read(2)` produced **no bytes**. A poll loop that treats readable-with-EOF
    /// as an event re-fires on every one of these.
    pollin_no_data: u64,
    bytes: u64,
    elapsed_ms: u64,
    /// Read-outcome class → count (`bytes` / `eof` / `EAGAIN` / `EIO` / …).
    reads: BTreeMap<String, u64>,
    /// `revents` label → count.
    revents: BTreeMap<String, u64>,
}

impl Readiness {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "passes": self.passes,
            "elapsed_ms": self.elapsed_ms,
            "pollin_passes": self.pollin,
            "pollhup_passes": self.pollhup,
            "pollin_with_no_data_passes": self.pollin_no_data,
            "bytes_read": self.bytes,
            "read_outcomes": self.reads,
            "revents_seen": self.revents,
        })
    }
}

/// P6's sampling budget: bounded passes over a bounded wall clock, with a pause
/// between passes near the PTY node's idle poll period (5 ms) so the sample
/// resembles the loop whose behaviour is in question — and so the doctor itself
/// never busy-spins while asking whether the kernel would make the daemon do so.
const P6_PASSES: u32 = 64;
const P6_WINDOW: Duration = Duration::from_millis(200);
const P6_PASS_PAUSE: Duration = Duration::from_millis(2);
/// Let a pty pair's traffic land before it is read/measured. The kernel delivers
/// synchronously; this is insurance, and the doctor is a diagnostic, not a data
/// path, so the milliseconds cost nothing.
const PTY_SETTLE: Duration = Duration::from_millis(20);

/// Poll the master repeatedly and record, per pass, the `revents` and what a
/// `read(2)` then returned. Polls **exactly the interest the PTY node polls**
/// (`POLLIN | POLLHUP`, `nodes/pty.rs`), because the question is what that loop
/// would see. Reads on every pass regardless of POLLIN — the fd is non-blocking,
/// so this cannot hang, and "what does read say when poll says nothing" is half
/// the diff.
fn sample_readiness(fd: RawFd, max_passes: u32, window: Duration) -> Readiness {
    let mut o = Readiness::default();
    let mut buf = [0u8; 4096];
    let start = Instant::now();
    let deadline = start + window;
    for _ in 0..max_passes {
        if Instant::now() >= deadline {
            break;
        }
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        o.passes += 1;
        *o.revents.entry(revents_label(re)).or_default() += 1;
        if re.contains(PollFlags::POLLIN) {
            o.pollin += 1;
        }
        if re.contains(PollFlags::POLLHUP) {
            o.pollhup += 1;
        }
        let (class, n) = match sys::read_fd(fd, &mut buf) {
            Ok(0) => ("eof".to_owned(), 0usize),
            Ok(n) => ("bytes".to_owned(), n),
            Err(e) => (read_class(&e), 0),
        };
        *o.reads.entry(class).or_default() += 1;
        o.bytes += n as u64;
        if re.contains(PollFlags::POLLIN) && n == 0 {
            o.pollin_no_data += 1;
        }
        std::thread::sleep(P6_PASS_PAUSE);
    }
    o.elapsed_ms = start.elapsed().as_millis() as u64;
    o
}

// ---------------------------------------------------------------------------
// P6 — pty-master readiness after the last slave closes (§6, §7.2, §13)
// ---------------------------------------------------------------------------

/// Does the master keep asserting `POLLIN` once its last slave fd is gone?
///
/// The record disagrees with itself. `b8d8ed8` reports that an *ungated*
/// `closed`-only last-close arm in `nodes/pty.rs` spun at **99 % CPU** and
/// starved the data plane; `itest/tests/p9_pty_collapse.rs` reports that
/// planting that same arm did **not** raise the daemon's CPU on Linux 7.0,
/// because the master stops reporting `POLLIN` after the last close so `closed`
/// is set only in the poll that drains the final bytes. Both observations are
/// credible; they were made on different kernels, and production is 6.18. The
/// shipped code is safe either way — it consumes the packet its own reset
/// provokes, so it needs no assumption about POLLIN at all — but nobody can
/// *simplify* it until this is measured where it has to run.
pub fn p6_last_close_readiness() -> Probe {
    let p = Probe::new(
        "P6",
        "pty-master readiness after the last slave closes",
        "Once a pty's last slave fd closes, does the master keep asserting POLLIN with nothing to read (the shape that spins a close-triggered poll loop)?",
    );
    match p6_inner() {
        Ok(r) => {
            let spun = r.after_close.pollin_no_data;
            let rearm = if r.reset_applied && r.reset_extproc_retained {
                format!(
                    " The node's own last-close termios reset then re-armed readability {} time(s) ({} byte(s)), so the drain in `pty.rs` that consumes that packet stays load-bearing regardless: without it the handler re-arms itself and the runaway returns by that route rather than through a stuck POLLIN.",
                    r.after_reset.pollin, r.after_reset.bytes
                )
            } else if r.reset_applied {
                format!(
                    " The node's own last-close termios reset was ACCEPTED through the master here, but this kernel did not retain EXTPROC afterwards (`handler_reset_extproc_retained: false`) — so the EXTPROC-gated `TIOCPKT_IOCTL` re-arm cannot fire at all, and the {} byte(s) read below say nothing about whether `pty.rs`'s drain is load-bearing. P1 is the probe for that mechanism, and §7.2 already runs poll-only where it is degraded. Read `handler_reset_applied: true` as \"the syscall was accepted\", never as \"the reset took effect\".",
                    r.after_reset.bytes
                )
            } else {
                " The node's own last-close termios reset could not be applied through the master here (the §7.2 BSD arm), so its re-arm effect is unmeasured.".to_owned()
            };
            // Which fd the *node* resets through is a code-path fact, not a kernel
            // measurement, so it is reported as one. Where the two differ, this second
            // block is measuring a reset the node does not perform on this platform:
            // `handle_last_close` goes through `with_termios_fd`, which off Linux opens
            // a momentary slave — and that open *clears the hangup*, which is why this
            // probe must not imitate it (see `apply_pty_baseline`) and therefore why
            // this block stays Linux-shaped wherever it is not Linux. `pty.rs` accounts
            // for that open separately, discarding the session edge it posts.
            let node_reset_path = if cfg!(target_os = "linux") {
                "master"
            } else {
                "momentary-slave"
            };
            let discipline = format!(
                " The client session was measured with the pair in `{}` and EXTPROC {} — the §7.2 baseline re-asserted on the client's own slave, having reached the pair {} at setup — so this reading is of the daemon's pty and not of whatever discipline the kernel left behind.",
                r.client.mode,
                if r.client.extproc { "set" } else { "**NOT set — this kernel does not retain it** (P1)" },
                if r.client.via_master { "through the master" } else { "through the momentary-slave fallback" },
            );
            let p = p
                .observe("after_last_close", r.after_close.observations())
                .observe("client_session_baseline", r.client.observations())
                .observe("handler_reset_applied", r.reset_applied)
                .observe("handler_reset_extproc_retained", r.reset_extproc_retained)
                .observe("handler_reset_path_probe", "master")
                .observe("handler_reset_path_node", node_reset_path)
                .observe("handler_reset_readable_bytes", r.after_reset.bytes)
                .observe("after_handler_termios_reset", r.after_reset.observations());
            if spun == 0 {
                p.verdict(
                    Status::Supported,
                    &format!(
                        "POLLIN goes quiet after the last close on this kernel ({} passes, {} with POLLIN, none readable-with-nothing-to-read): an ungated `closed`-only last-close arm would NOT spin on the hangup alone here, so pty.rs's `saw_session` latch is not what holds the anti-spin argument up on this kernel.{rearm} This is a per-kernel reading — §13 forbids acting on it until the production kernel (6.18) reports the same numbers, so diff this block before simplifying anything.{discipline}",
                        r.after_close.passes, r.after_close.pollin
                    ),
                )
            } else {
                p.verdict(
                    Status::Degraded,
                    &format!(
                        "The master keeps asserting POLLIN with nothing to read after the last close ({spun} of {} passes): an ungated `closed`-only arm WOULD re-fire every pass here — the 99%-CPU shape `b8d8ed8` records. pty.rs's `saw_session` latch (and the drain that consumes the handler's own control packet) is load-bearing on this kernel: do not simplify it.{rearm} The shipped code is correct as it stands; this is a warning about a pending simplification, not a fault.{discipline}",
                        r.after_close.passes
                    ),
                )
            }
        }
        // Never `unsupported`: this probe measures which of two legitimate kernel
        // behaviours applies, and the daemon copes with both. A probe that could
        // not run leaves the question open — which means "do not simplify".
        Err(e) => p.verdict(
            Status::Degraded,
            &format!(
                "probe error ({e}) → post-hangup readiness unmeasured on this kernel; treat pty.rs's last-close latch as load-bearing."
            ),
        ),
    }
}

struct P6Result {
    after_close: Readiness,
    client: ClientBaseline,
    reset_applied: bool,
    reset_extproc_retained: bool,
    after_reset: Readiness,
}

fn p6_inner() -> anyhow::Result<P6Result> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    // Non-blocking, packet mode, §7.2 baseline: the master exactly as the PTY node
    // holds it (`nodes/pty.rs`), so the readings describe that loop's fd.
    sys::set_nonblocking(fd)?;
    let via_master = apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;

    let mut buf = [0u8; 4096];
    // A client attaches. Re-assert the §7.2 baseline on the fd that client holds — the
    // daemon's rising-presence-edge re-assert — so the hangup sampled below is a hangup
    // of the daemon's pty and not of whatever discipline the kernel left the pair in.
    // Free here in a way it is not in P7: the drain that follows already exists for
    // exactly this reason, so the packet the re-assert provokes is consumed by
    // machinery that was already there. Measured on Linux 7.0.0-29: `after_last_close`
    // is byte-identical with and without it (pollin 0, pollin-with-no-data 0, POLLHUP
    // 64/64, terminal EIO), and so is `handler_reset_readable_bytes` (1).
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let client = arm_client_baseline(fd, &slave, via_master, &mut buf);
    // Drain everything setup, the attach and the re-assert left, so the sampling below
    // measures the hangup and nothing else.
    std::thread::sleep(PTY_SETTLE);
    let _ = read_available(fd, &mut buf, 64);

    // The last close — the §7.2 "client detached" edge, with no client data behind
    // it (the bare-hangup shape `p9_pty_collapse` samples the daemon's CPU across).
    drop(slave);
    std::thread::sleep(PTY_SETTLE);
    let after_close = sample_readiness(fd, P6_PASSES, P6_WINDOW);

    // The other half of the same mechanism, recorded but not judged: the node's
    // `handle_last_close` re-applies the baseline termios to the now-hung-up pair,
    // which re-arms TIOCPKT_IOCTL on the master. pty.rs consumes that packet
    // deliberately; if this kernel leaves POLLIN asserted afterwards, that drain is
    // the load-bearing part. Applied through the master only — the slave fallback
    // would clear the very hangup being measured.
    let reset_applied = set_baseline(&master).is_ok();
    // …and whether it *took*. `set_baseline` answering `Ok(())` says the syscall was
    // accepted, not that the pair now carries EXTPROC — and EXTPROC is the whole
    // mechanism this block is about, since a kernel that drops the flag emits no
    // `TIOCPKT_IOCTL` at all. Reading it back is what stops `handler_reset_applied:
    // true` sitting next to `handler_reset_readable_bytes: 0` as an unexplained
    // contradiction (it does, on Darwin, in every committed artifact).
    let reset_extproc_retained = extproc_set(&master);
    std::thread::sleep(PTY_SETTLE);
    let after_reset = sample_readiness(fd, P6_PASSES, P6_WINDOW);
    Ok(P6Result {
        after_close,
        client,
        reset_applied,
        reset_extproc_retained,
        after_reset,
    })
}

// ---------------------------------------------------------------------------
// P7 — what a collapsed client session leaves readable on the master (§6, §13)
// ---------------------------------------------------------------------------

/// The three client-session shapes, run against a packet-mode master and read
/// *after* the hangup.
#[derive(Clone, Copy)]
enum SessionShape {
    /// Open the slave and close it, touching nothing.
    OpenClose,
    /// Open, `tcsetattr` (the `stty`/health-check/scripted-probe shape), close.
    Termios,
    /// Open, write one byte, close.
    Write,
}

impl SessionShape {
    fn key(self) -> &'static str {
        match self {
            SessionShape::OpenClose => "a_open_close",
            SessionShape::Termios => "b_open_tcsetattr_close",
            SessionShape::Write => "c_open_write_close",
        }
    }
}

/// What one collapsed session left on the master after its hangup.
struct ShapeResult {
    bytes: u64,
    reads: u64,
    leading_hex: Vec<String>,
    terminal: String,
    baseline: ClientBaseline,
}

impl ShapeResult {
    fn ioctl_bit(&self) -> bool {
        self.leading_hex
            .iter()
            .filter_map(|h| u8::from_str_radix(h.trim_start_matches("0x"), 16).ok())
            .any(|b| b & sys::TIOCPKT_IOCTL != 0)
    }

    fn data_packet(&self) -> bool {
        self.leading_hex
            .iter()
            .filter_map(|h| u8::from_str_radix(h.trim_start_matches("0x"), 16).ok())
            .any(|b| b == sys::TIOCPKT_DATA)
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "bytes_readable_after_close": self.bytes,
            "reads": self.reads,
            "leading_bytes_hex": self.leading_hex,
            "terminal_read": self.terminal,
            "ioctl_bit_set": self.ioctl_bit(),
            "data_packet_seen": self.data_packet(),
            "baseline_via_master": self.baseline.via_master,
            "reasserted_on_client_slave": self.baseline.reasserted,
            "slave_termios_mode": self.baseline.mode,
            "extproc_set_at_shape": self.baseline.extproc,
            "baseline_packet_bytes_drained": self.baseline.footprint_bytes,
        })
    }
}

/// Which collapsed client sessions leave evidence on a packet-mode master?
///
/// `nodes/pty.rs` releases a detached client's write lock on
/// `(was && !present_now) || (closed && saw_session)`. Both disjuncts require the
/// reader to have *observed* something, so a session that opens, calls
/// `tcsetattr` and closes inside one poll gap used to satisfy neither and leaked
/// its write lock forever (§6 detach-release). The fix widened the latch from
/// "saw a `TIOCPKT_DATA` payload" to "saw **any** readable packet", and it rests
/// on one measured premise: that a client's `tcsetattr` leaves a `TIOCPKT_IOCTL`
/// byte readable on the master *past the hangup* — observed on 7.0 at syscall
/// level as `read(11, "A", 65536) = 1` (`0x41` = `TIOCPKT_IOCTL|TIOCPKT_FLUSHREAD`).
/// If 6.18 leaves nothing there, the widened latch silently fails to cover the
/// realistic collapsed session and the write-lock leak persists on the production
/// kernel **with no signal at all**. That is what this probe catches.
///
/// The *value* of the leading byte is reported, never judged: `stty`'s
/// `TCSETSW2`/`TCSETSF2` flushes, so it reads `0x41`
/// (`TIOCPKT_IOCTL|TIOCPKT_FLUSHREAD`), while this probe's `tcsetattr(TCSANOW)`
/// leaves a bare `0x40`. The latch arms on *any* successful read, so only
/// "something was readable" gates the verdict — but the byte is in the JSON,
/// because a kernel that changed which packet it emits is exactly the surprise a
/// cross-kernel diff is for.
pub fn p7_collapsed_session() -> Probe {
    let p = Probe::new(
        "P7",
        "evidence a collapsed client session leaves on the master",
        "After a pty client hangs up, which session shapes (bare open/close, tcsetattr-only, one byte written) leave a readable packet on the packet-mode master?",
    );
    let shapes = [
        SessionShape::OpenClose,
        SessionShape::Termios,
        SessionShape::Write,
    ];
    let mut p = p;
    let mut results: Vec<(SessionShape, ShapeResult)> = Vec::new();
    for shape in shapes {
        match p7_shape(shape) {
            Ok(r) => {
                p = p.observe(shape.key(), r.observations());
                results.push((shape, r));
            }
            Err(e) => {
                p = p.observe(shape.key(), format!("probe error: {e}"));
            }
        }
    }

    let termios = results
        .iter()
        .find(|(s, _)| matches!(s, SessionShape::Termios))
        .map(|(_, r)| r);
    let wrote = results
        .iter()
        .find(|(s, _)| matches!(s, SessionShape::Write))
        .map(|(_, r)| r);
    let covered = termios.map(|r| r.bytes > 0).unwrap_or(false);
    let data_covered = wrote.map(|r| r.bytes > 0).unwrap_or(false);
    // What the measurement ran *in*, promoted out of the per-shape blocks so a jq
    // one-liner can gate on it the way P10's `slave_termios_mode` is gated on.
    let extproc_at_shape = termios.map(|r| r.baseline.extproc).unwrap_or(false);
    let raw_at_shape = termios.map(|r| r.baseline.mode == "raw").unwrap_or(false);
    let cause = p7_silence_cause(
        termios.map(|r| r.bytes).unwrap_or(0),
        wrote.map(|r| r.bytes).unwrap_or(0),
        extproc_at_shape,
    );
    let discipline = termios
        .map(|r| {
            format!(
                " Measured with the pair in `{}`, EXTPROC {}, the §7.2 baseline re-asserted on the client's own slave ({}) and reaching the pair {} at setup; the re-assert's own {} byte(s) were drained before the session ran, so nothing below is the probe's own footprint.",
                r.baseline.mode,
                if r.baseline.extproc { "set" } else { "**NOT set — this kernel did not retain it**" },
                if r.baseline.reasserted { "applied" } else { "REFUSED" },
                if r.baseline.via_master { "through the master" } else { "through the momentary-slave fallback" },
                r.baseline.footprint_bytes,
            )
        })
        .unwrap_or_default();
    p = p
        .observe("latch_covers_termios_only_session", covered)
        .observe("latch_covers_data_session", data_covered)
        .observe("extproc_retained_at_shape", extproc_at_shape)
        .observe(
            "measured_in_daemon_baseline",
            raw_at_shape && extproc_at_shape,
        )
        .observe("silence_cause", cause);

    match cause {
        "covered" => p.verdict(
            Status::Supported,
            &format!(
                "A collapsed termios-only session leaves {} byte(s) readable past the hangup (leading {}, ioctl bit {}): pty.rs's widened last-close latch arms on it, so an `stty`/health-check/scripted client that opens, reconfigures and closes inside one poll gap still runs detach-release (§6). Diff this against the production kernel (6.18) before trusting the coverage there.{discipline}",
                termios.map(|r| r.bytes).unwrap_or(0),
                termios.map(|r| r.leading_hex.join(" ")).unwrap_or_default(),
                termios.map(|r| r.ioctl_bit()).unwrap_or(false),
            ),
        ),
        // Both shapes silent. NOT a latch-coverage finding, and emphatically not a
        // lost line discipline: a written byte is not EXTPROC-gated, and measured on
        // Linux 7.0.0-29 it is not ICANON-gated either (a fully cooked pair still
        // delivers its 2 bytes across the hangup). Both silent means the pair's queues
        // do not survive the last close here.
        "hangup-destroys-evidence" => p.verdict(
            Status::Degraded,
            &format!(
                "NOTHING is readable on the master after the hangup for ANY of the three shapes on this kernel — not the termios-only session, and not the one that wrote a byte. That second fact is what makes this a different finding from a lost baseline: a written byte is not EXTPROC-gated (measured on Linux 7.0.0-29: planting a baseline with no EXTPROC silences the termios shape and leaves the write shape at 2 bytes; a fully cooked pair also leaves it at 2). Both silent means this kernel destroys the pair's queues at the last close — the disposition **P13** measures — so no termios repair can move these numbers and this is **not** evidence that §6 detach-release is broken here. On such a kernel the session boundary is carried as an *edge* rather than as readable state: read **P12** in this same report, which measures exactly that, before drawing any conclusion about the write lock (§15.39).{discipline}"
            ),
        ),
        // The measurement could not ask its question: no EXTPROC means no
        // TIOCPKT_IOCTL is possible at all, whatever the latch does.
        "extproc-unavailable" => p.verdict(
            Status::Degraded,
            &format!(
                "The termios-only session left nothing readable, but this kernel did not retain EXTPROC on the pair even after the baseline was re-asserted on the client's own slave — so the EXTPROC-gated `TIOCPKT_IOCTL` notification P1 probes cannot fire here at all. This is not evidence about the latch's coverage; it is evidence that this *mechanism* is absent, and §7.2 already runs poll-only wherever P1 is degraded. A session that wrote a byte does leave evidence here. Read **P12** for the mechanism that carries §6 detach-release on such a kernel.{discipline}"
            ),
        ),
        // The original finding, now reached only when the premises actually hold.
        _ => p.verdict(
            Status::Degraded,
            &format!(
                "A collapsed termios-only session leaves NOTHING readable on the master after the hangup on this kernel, though a session that wrote a byte does — and the pair demonstrably carried the daemon's EXTPROC baseline when it ran, so this kernel retains data evidence past the hangup but not the ioctl packet. pty.rs's last-close latch has nothing to arm on for that shape via *this* mechanism; check **P12** in this report for whether the session-boundary edge covers it here (§15.39), and if it does not, a client that opens, calls tcsetattr and closes inside one poll gap keeps its write lock until the node is removed or another writer steals it (§6 detach-release), silently.{discipline}"
            ),
        ),
    }
}

fn p7_shape(shape: SessionShape) -> anyhow::Result<ShapeResult> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    sys::set_nonblocking(fd)?;
    let via_master = apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;

    let mut buf = [0u8; 4096];
    // Prime and quiesce exactly as the node does (open+close at creation, §7.2),
    // then drain twice: once with the primer open, once after its hangup. What the
    // measured session leaves behind must be its own, not the setup's.
    {
        let prime = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
        std::thread::sleep(PTY_SETTLE);
        let _ = read_available(fd, &mut buf, 64);
        drop(prime);
    }
    std::thread::sleep(PTY_SETTLE);
    let _ = read_available(fd, &mut buf, 64);

    // The measured session.
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    // The §7.2 baseline, re-asserted on the fd the client itself holds — the daemon's
    // rising-presence-edge re-assert, and what makes an `stty`-shaped `tcsetattr` emit
    // the packet this probe counts. `apply_pty_baseline` above set the baseline before
    // any slave existed; on a kernel that re-initialises pts termios at open, that set
    // is gone by the time this session runs, and shape `b` then measures a pair with no
    // EXTPROC and reports "nothing readable" on a kernel that would plainly have left
    // something — a false `degraded`, reproduced on Linux 7.0.0-29 by planting the loss
    // (`b` falls 1 → 0 while `c` stays at 2, and this call restores it to 1).
    //
    // That asymmetry is also this report's diagnostic, and it is why the verdict below
    // splits on it: `b == 0` with `c > 0` is a lost line discipline; `b == 0` with
    // `c == 0` is a kernel that destroys the evidence at the hangup, which no termios
    // call can repair and which P13 — not this probe — is the instrument for.
    let baseline = arm_client_baseline(fd, &slave, via_master, &mut buf);
    match shape {
        SessionShape::OpenClose => {}
        SessionShape::Termios => {
            // A real client change, through the same safe termios path P1/P2 use:
            // re-enable ECHO, which the baseline cleared. This is the syscall
            // sequence `stty -F <path> echo` performs (openat → TCGETS2 → TCSETSW2
            // → close) and the shape `p9_pty_collapse` drives end to end.
            let mut t = tcgetattr(&slave)?;
            t.local_flags.insert(LocalFlags::ECHO);
            tcsetattr(&slave, SetArg::TCSANOW, &t)?;
        }
        SessionShape::Write => {
            nix::unistd::write(&slave, b"x")?;
        }
    }
    std::thread::sleep(PTY_SETTLE);
    // The collapse: the client is gone before the reader ever polled.
    drop(slave);
    std::thread::sleep(PTY_SETTLE);

    let (bytes, reads, leading_hex, terminal) = read_available(fd, &mut buf, 64);
    Ok(ShapeResult {
        bytes,
        reads,
        leading_hex,
        terminal,
        baseline,
    })
}

// ---------------------------------------------------------------------------
// P12 — session-boundary EDGE evidence (§15.39, §6 detach-release)
//
// P7's sibling, and deliberately the same question asked of the *other*
// mechanism. P7 measures what a collapsed session leaves **readable** on the
// master; this measures whether the session boundary is reported as an **edge**
// where it leaves nothing readable at all. Between them a reader can always tell
// which of the two carries detach-release on the kernel in front of them — which
// matters because the answer is different on Linux and Darwin and the failure
// they guard against (a leaked write lock) looks identical either way.
//
// Not `#[cfg]`-gated for the reason P8 states: `serial_nexus_sys::SessionLatch` has an
// inert arm off macOS, so this file has one code path and the probe reports
// `skipped` where the mechanism is not the one in use.
// ---------------------------------------------------------------------------

/// One trial's answer: did the latch report an edge for this session shape?
fn p12_shape(shape: SessionShape) -> anyhow::Result<bool> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    sys::set_nonblocking(fd)?;
    apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;
    // Prime and quiesce exactly as `PtyNode::setup` does, then watch. `watch`
    // swallows the registration edge, which is itself part of the contract being
    // measured — a latch that handed that one back would report an edge here for
    // every shape, including "nothing happened".
    {
        let prime = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
        std::thread::sleep(PTY_SETTLE);
        drop(prime);
    }
    std::thread::sleep(PTY_SETTLE);
    let latch = sys::SessionLatch::watch(fd)?;

    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    match shape {
        SessionShape::OpenClose => {}
        SessionShape::Termios => {
            let mut t = tcgetattr(&slave)?;
            t.local_flags.insert(LocalFlags::ECHO);
            tcsetattr(&slave, SetArg::TCSANOW, &t)?;
        }
        SessionShape::Write => {
            let _ = sys::write_fd(slave.as_raw_fd(), b"x");
        }
    }
    drop(slave);
    std::thread::sleep(PTY_SETTLE);
    Ok(latch.took_edge())
}

/// The anti-spin half: how many edges does an **idle**, hung-up master post
/// across `passes` reader-shaped passes? Anything but zero re-fires the last-close
/// handler forever.
fn p12_idle_edges(passes: u32) -> anyhow::Result<u64> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    sys::set_nonblocking(fd)?;
    apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;
    {
        let prime = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
        std::thread::sleep(PTY_SETTLE);
        drop(prime);
    }
    std::thread::sleep(PTY_SETTLE);
    let latch = sys::SessionLatch::watch(fd)?;
    let mut buf = [0u8; 256];
    let mut edges = 0u64;
    for _ in 0..passes {
        // The reader's own shape: poll, read, then ask the latch — that sequence's
        // side effects are what could re-arm the knote.
        let _ = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let _ = sys::read_fd(fd, &mut buf);
        if latch.took_edge() {
            edges += 1;
        }
    }
    Ok(edges)
}

/// Does an edge latch on a pty master report a session that left nothing readable?
///
/// `nodes/pty.rs` releases a detached client's write lock on
/// `!present_now && (was || saw_session)`, and `saw_session` is armed two ways: by
/// a readable packet (P7's subject, the Linux mechanism) or by a session-boundary
/// edge (`serial_nexus_sys::SessionLatch`, §15.39, the Darwin one). Darwin's `ttyclose`
/// flushes both tty queues at the slave's last close, so the packet is gone and
/// **the edge is the only mechanism left**; a kernel where neither works leaks the
/// write lock on every collapsed session, silently, which is precisely the failure
/// P7's `degraded` arm describes and this probe's `degraded` arm inherits.
///
/// Three things are reported and all three are load-bearing. Whether the
/// **termios-only** shape posts an edge is the property `p9_pty_collapse` asserts
/// end to end. Whether an **idle** master posts one is the anti-spin property: a
/// non-zero count there means the last-close handler re-fires forever *and*
/// releases a lock no client ever took, which is worse than the leak it fixes. And
/// the **bare open/close** shape is reported because Darwin covers it where Linux
/// does not — an asymmetry worth seeing in a diff rather than discovering.
pub fn p12_session_edge() -> Probe {
    let p = Probe::new(
        "P12",
        "session-boundary edge on a pty master",
        "Does an edge latch report a collapsed client session that left nothing readable on the master, and does it stay silent while idle?",
    );
    // The inert arm is not a failure: on Linux the retained packet is the
    // mechanism and P7 measures it, so there is nothing here to be wrong.
    if !cfg!(target_os = "macos") {
        return p.verdict(
            Status::skipped("serial-nexus-sys's SessionLatch is inert on this platform"),
            "The session boundary is carried by the retained `TIOCPKT_IOCTL` packet here, which P7 measures — nothing is untested, only unmeasurable by this route (§15.39, §13).",
        );
    }

    let mut p = p;
    let mut termios_edge = None;
    for shape in [
        SessionShape::OpenClose,
        SessionShape::Termios,
        SessionShape::Write,
    ] {
        match p12_shape(shape) {
            Ok(edge) => {
                p = p.observe(&format!("{}_edge", shape.key()), edge);
                if matches!(shape, SessionShape::Termios) {
                    termios_edge = Some(edge);
                }
            }
            Err(e) => {
                p = p.observe(
                    &format!("{}_edge", shape.key()),
                    format!("probe error: {e}"),
                )
            }
        }
    }

    let idle = p12_idle_edges(200);
    p = p.observe(
        "idle_edges_in_200_passes",
        match &idle {
            Ok(n) => serde_json::json!(n),
            Err(e) => serde_json::json!(format!("probe error: {e}")),
        },
    );

    match (termios_edge, idle) {
        (Some(true), Ok(0)) => p.verdict(
            Status::Supported,
            "A collapsed termios-only session posts a session-boundary edge and an idle hung-up master posts none in 200 reader-shaped passes: `pty.rs`'s `saw_session` latch is armed by the edge where this kernel keeps no readable evidence, so detach-release covers the `stty`/health-check/scripted shape (§6, §15.39). This is the mechanism `p9_pty_collapse` asserts end to end here.",
        ),
        // Any nonzero idle count is the dangerous direction and gets said first:
        // it releases a lock nobody's client took, on every pass.
        (_, Ok(n)) if n > 0 => p.verdict(
            Status::Degraded,
            &format!(
                "An idle, hung-up master posted {n} session edge(s) in 200 passes. That re-fires `pty.rs`'s last-close handler on a pair no client has touched — releasing a write lock the operator took, and burning the runtime thread doing it. `SessionLatch`'s discard sites (§15.39) or this kernel's `EV_CLEAR` semantics have changed; re-check both before trusting detach-release here."
            ),
        ),
        (Some(false), _) => p.verdict(
            Status::Degraded,
            "A collapsed termios-only session posts NO session-boundary edge on this kernel. With P7 also reporting nothing readable, `pty.rs`'s last-close latch has neither mechanism to arm on, so a client that opens, calls tcsetattr and closes inside one poll gap keeps its write lock until the node is removed or another writer steals it (§6) — silently. Read P7 beside this: if *it* is `supported`, the packet is carrying detach-release here and this is only an unused second route.",
        ),
        _ => p.verdict(
            Status::Degraded,
            "The session-edge measurement did not complete, so which mechanism carries detach-release on this kernel is unknown. Read P7: if it reports readable evidence, the packet route is intact regardless (§6, §15.39).",
        ),
    }
}

// ---------------------------------------------------------------------------
// P13 — what a pts slave's last close does to bytes the master has not read
// (§5's accounting, §7.2's drain-before-close ordering, harness rule notes §3.29)
// ---------------------------------------------------------------------------

/// How the master behaved during the session whose close is being timed.
#[derive(Clone, Copy)]
enum CloseShape {
    /// The master never reads while the client is attached — the shape a poll
    /// loop produces when the whole session falls inside one poll gap.
    NoReader,
    /// The master drains before the close, the way a healthy reader does.
    ReaderDrains,
    /// As `NoReader`, but the slave carries `O_NONBLOCK`. XNU's `ttylclose`
    /// branches on exactly this flag, so the pair is a controlled A/B on the
    /// branch rather than an inference about it.
    NoReaderNonblocking,
}

impl CloseShape {
    fn key(self) -> &'static str {
        match self {
            CloseShape::NoReader => "a_no_reader_blocking_slave",
            CloseShape::ReaderDrains => "b_reader_drains_before_close",
            CloseShape::NoReaderNonblocking => "c_no_reader_nonblocking_slave",
        }
    }
}

/// One timed last-close and what survived it.
struct CloseResult {
    /// How long `close(2)` on the slave took. The three-way discriminator: a
    /// kernel that *waits* for the reader spends milliseconds here, one that
    /// flushes or retains returns in microseconds.
    close_us: u64,
    /// Bytes the master recovered *before* the close (0 for the no-reader shapes).
    bytes_before: u64,
    /// Bytes the master recovered *after* the close.
    bytes_after: u64,
    /// How the master's post-close drain ended (`eof` / `EIO` / …).
    terminal: String,
    /// The line discipline the measured slave was actually in. A close policy read
    /// in some other discipline is another configuration's number wearing this
    /// one's name (notes §3.34).
    slave_mode: &'static str,
    /// Bytes the baseline re-assert itself put on the master, consumed before the
    /// measurement window opened. Reported, not assumed: Linux queues one
    /// `TIOCPKT_IOCTL` control byte and a BSD may queue a termios struct behind it.
    baseline_packet_bytes: u64,
}

impl CloseResult {
    fn observations(&self, written: u64) -> serde_json::Value {
        serde_json::json!({
            "bytes_written_by_slave": written,
            "bytes_recovered_before_close": self.bytes_before,
            "bytes_recovered_after_close": self.bytes_after,
            "bytes_recovered_total": self.bytes_before + self.bytes_after,
            "bytes_lost": written.saturating_sub(self.bytes_before + self.bytes_after),
            "close_microseconds": self.close_us,
            "terminal_read": self.terminal,
            "slave_termios_mode": self.slave_mode,
            "baseline_packet_bytes": self.baseline_packet_bytes,
        })
    }
}

/// How many bytes the slave writes before the close. Small on purpose: this
/// measures a *policy*, not a capacity, and P10 already measures the depth.
const P13_PAYLOAD: usize = 64;
/// A close is "waiting for the reader" rather than returning promptly beyond
/// this. Two orders of magnitude above the microsecond-scale prompt return
/// measured on Linux, and two below Darwin's `t_timeout` (60 ticks), so neither
/// kernel lands near the boundary.
const P13_WAIT_THRESHOLD_US: u64 = 1_000;

/// What does this kernel do with bytes a pts client wrote and the master has not
/// read, when the client closes?
///
/// **Why this exists.** The three answers are behaviourally different and, until
/// this probe, indistinguishable from anything the tree recorded:
///
/// * **retain** — the bytes stay readable past the hangup (Linux: `close(2)`
///   returns in ~1 µs and the master still reads all of them, then `EIO`);
/// * **flush** — the close discards them promptly;
/// * **wait, then flush** — the close *blocks*, nudging the master awake, and
///   discards only if the reader never comes. XNU's `ptsclose` sets
///   `t_timeout = 60` ticks and calls `ttylclose` → `ttywflush` → `ttywait`
///   before any flush, which is this third answer.
///
/// `close_microseconds` separates them, which no observation in the set did.
/// P7 asks what a collapsed session leaves *readable* against a master nobody
/// drains — a yes/no that "flush" and "wait, then flush" answer identically,
/// because a master that never reads makes the wait time out either way. The
/// distinction is not academic: under "flush" a harness that closes before
/// checking a counter is racing microseconds and should almost always lose;
/// under "wait, then flush" it should almost always *win*, and a loss means the
/// reader stalled for the whole timeout — a daemon-side event wearing a kernel
/// costume. `docs/macos.md` (2026-08-04) records a macOS CI failure whose two
/// readings differ exactly this way, and names this probe as the separator.
///
/// **What it does not do.** It never judges the answer. All three are legitimate
/// and the shipped daemon is correct under each — §7.2 drains before finalizing a
/// close, and §5 accounts what it reads. The verdict is `Supported` whenever the
/// measurement completed, and the *policy* is stated in the consequence line so a
/// cross-platform diff reads it directly. What it does bind is a **harness** rule
/// (notes §3.29): a test reads a byte counter while the client that fed it is
/// still open, on every platform, whatever this probe says — because "retain" is
/// a property of the kernel under the test, never of the product under test.
///
/// **Cost.** Microseconds on a kernel that retains or flushes. On one that waits,
/// shapes `a` and `c` each pay their timeout once — up to ~0.6 s apiece on
/// Darwin — which is the price of measuring the thing that matters and is paid
/// only where the answer is interesting.
pub fn p13_last_close_disposition() -> Probe {
    let p = Probe::new(
        "P13",
        "disposition of unread client bytes at a pts last close",
        "When a pty client writes bytes the master has not read and then closes, does this kernel retain them, discard them, or block the close waiting for the reader?",
    );
    let shapes = [
        CloseShape::NoReader,
        CloseShape::ReaderDrains,
        CloseShape::NoReaderNonblocking,
    ];
    let mut p = p;
    let mut results: Vec<(CloseShape, CloseResult)> = Vec::new();
    for shape in shapes {
        match p13_shape(shape) {
            Ok(r) => {
                p = p.observe(shape.key(), r.observations(P13_PAYLOAD as u64));
                results.push((shape, r));
            }
            Err(e) => p = p.observe(shape.key(), format!("probe error: {e}")),
        }
    }

    let no_reader = results
        .iter()
        .find(|(s, _)| matches!(s, CloseShape::NoReader))
        .map(|(_, r)| r);
    let drained = results
        .iter()
        .find(|(s, _)| matches!(s, CloseShape::ReaderDrains))
        .map(|(_, r)| r);

    let Some(bare) = no_reader else {
        return p.verdict(
            Status::Degraded,
            "The last-close disposition did not measure, so which of retain / discard / wait-then-discard this kernel implements is unknown. A harness must read a byte counter while its client is still open regardless (notes §3.29) — that rule does not depend on this answer.",
        );
    };

    let recovered = bare.bytes_before + bare.bytes_after;
    let waited = bare.close_us >= P13_WAIT_THRESHOLD_US;
    let policy = p13_policy(bare.close_us, recovered);
    let drained_note = match drained {
        Some(d) => format!(
            " With a master that drains before the close, {} of {P13_PAYLOAD} byte(s) are recovered and the close takes {} µs — the healthy-reader case, and the one the daemon is in.",
            d.bytes_before + d.bytes_after,
            d.close_us
        ),
        None => String::new(),
    };

    // §7.2's baseline is the discipline the daemon's pty runs. Idempotent where
    // the master carries the baseline and load-bearing where it does not, so this
    // should never fire on either kernel of record — which is the shape a tripwire
    // is supposed to have (notes §3.34, P10's `slave_termios_mode`).
    let cooked: Vec<&str> = results
        .iter()
        .filter(|(_, r)| r.slave_mode != "raw")
        .map(|(s, _)| s.key())
        .collect();
    let p = p
        .observe("policy", policy)
        .observe("close_waits_for_reader", waited);
    if !cooked.is_empty() {
        return p.verdict(
            Status::Degraded,
            &format!(
                "The last-close policy read **{policy}**, but the measured slave was not in §7.2's baseline discipline for shape(s) {} — these are some other configuration's numbers and must not be diffed against a run reporting `slave_termios_mode: \"raw\"`. The re-assert on the measured slave failed or was undone; re-check it before reading any figure here (notes §3.34).",
                cooked.join(", ")
            ),
        );
    }
    p.verdict(
            Status::Supported,
            &format!(
                "This kernel **{policy}** bytes a pts client wrote but the master never read: with no reader, {recovered} of {P13_PAYLOAD} byte(s) survive the last close and `close(2)` takes {} µs (terminal read `{}`).{drained_note} Numbers, not a verdict — every policy is legitimate and the daemon is correct under each (§7.2 drains before finalizing a close, §5 accounts what it reads). Read it for two things: a cross-kernel diff, and the reason a harness reads a byte counter while its client is still open rather than after (notes §3.29). A `waits-then-*` kernel additionally means a lost byte implies a reader stalled for the whole timeout, not a lost microsecond race.",
                bare.close_us, bare.terminal
            ),
        )
}

/// Name the kernel's last-close policy from the two numbers that determine it.
///
/// Split out from the probe so it can be tested against all four quadrants: the
/// interesting pair (`discards` vs `waits-then-discards`) differs *only* in
/// `close_us`, and a classifier that read the byte count alone would collapse them
/// into one word — which is precisely the conflation P13 exists to undo.
fn p13_policy(close_us: u64, recovered: u64) -> &'static str {
    match (close_us >= P13_WAIT_THRESHOLD_US, recovered > 0) {
        (false, true) => "retains",
        (false, false) => "discards",
        (true, true) => "waits-then-retains",
        (true, false) => "waits-then-discards",
    }
}

/// Put the slave P13 measures into §7.2's baseline and clear the cost of doing so
/// off the master, returning the resulting discipline and the bytes the re-assert
/// queued.
///
/// **Two separate jobs, and the second is the one that matters.** The re-assert is
/// P10's repair (notes §3.34) applied to P13's slave: `apply_pty_baseline` reaches
/// the master where the master is a terminal and otherwise a slave it opens and
/// immediately drops, and a BSD pty resets slave termios at that last close — so
/// without this the probe measures a cooked pty the daemon never runs, and says
/// nothing about which it measured. Applied on every platform: a repair that only
/// executes off the platform of record is a §9 proxy in space.
///
/// The drain exists because the re-assert is not free on the wire. With the
/// master in packet mode, a slave-side `tcsetattr` raises `TIOCPKT_IOCTL` —
/// measured on Linux 7.0.0-29 as exactly one byte, `0x40`. One bare control byte is
/// absorbed by the `bytes - reads` correction the caller already applies, so on
/// Linux this drain changes no reported figure. It is here for the kernel this tree
/// cannot interrogate: a BSD `ptcread` may copy the whole `struct termios` after that
/// control byte, which the per-read correction cannot subtract, and ~72 uncounted
/// bytes landing in `bytes_after` would flip Darwin's measured `waits-then-discards`
/// to `waits-then-retains` — an inverted headline caused by the instrument. Rather
/// than assume which BSD does what, the bytes are consumed before the measurement
/// window opens and **reported**, so the next Darwin capture answers it with a
/// number (§7).
fn p13_arm_slave<Fd: AsFd>(slave: &Fd, master_fd: RawFd, buf: &mut [u8]) -> (&'static str, u64) {
    let _ = set_baseline(slave);
    let mode = termios_mode(slave);
    std::thread::sleep(PTY_SETTLE);
    let (queued, ..) = read_available(master_fd, buf, 64);
    (mode, queued)
}

fn p13_shape(shape: CloseShape) -> anyhow::Result<CloseResult> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    sys::set_nonblocking(fd)?;
    apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;

    let mut buf = [0u8; 4096];
    // Prime and quiesce exactly as P6/P7 do, so what the measured session leaves
    // behind is its own and not the setup's.
    {
        let prime = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
        std::thread::sleep(PTY_SETTLE);
        let _ = read_available(fd, &mut buf, 64);
        drop(prime);
    }
    std::thread::sleep(PTY_SETTLE);
    let _ = read_available(fd, &mut buf, 64);

    let mut flags = OFlag::O_RDWR | OFlag::O_NOCTTY;
    if matches!(shape, CloseShape::NoReaderNonblocking) {
        flags |= OFlag::O_NONBLOCK;
    }
    let slave = open(pts.as_str(), flags, Mode::empty())?;
    let (slave_mode, baseline_packet_bytes) = p13_arm_slave(&slave, fd, &mut buf);

    // The payload is 64 `x`. That it survives a cooked discipline unchanged is a
    // property of the *byte*, not an accident to rely on silently: OPOST/ONLCR
    // expands `\n` and leaves `x` alone, measured on 7.0.0-29 as 64 recovered raw
    // and cooked alike where a `\n` payload came back 128. The re-assert above is
    // what makes that irrelevant rather than load-bearing.
    nix::unistd::write(&slave, &[b'x'; P13_PAYLOAD])?;
    std::thread::sleep(PTY_SETTLE);

    // The one variable: whether anything drained the master during the session.
    let bytes_before = match shape {
        CloseShape::ReaderDrains => {
            let (bytes, reads, ..) = read_available(fd, &mut buf, 64);
            // One packet-mode control byte rides in front of each read's payload and
            // is not client data, so the payload is `bytes - reads`. Subtracting a
            // constant 1 would under-count a drain that took several reads.
            bytes.saturating_sub(reads)
        }
        _ => 0,
    };

    // The measurement. `drop` is not timed reliably enough to trust here, so the
    // close is explicit and bracketed.
    let t0 = Instant::now();
    drop(slave);
    let close_us = t0.elapsed().as_micros() as u64;

    std::thread::sleep(PTY_SETTLE);
    let (raw_after, reads_after, _, terminal) = read_available(fd, &mut buf, 64);
    // Same control-byte correction as above.
    let bytes_after = raw_after.saturating_sub(reads_after);

    Ok(CloseResult {
        close_us,
        bytes_before,
        bytes_after,
        terminal,
        slave_mode,
        baseline_packet_bytes,
    })
}

// ---------------------------------------------------------------------------
// P8 — epoll vs read(2) on a pty master (invariant 1, §15.18, §13)
//
// Linux-only in substance, but NOT `#[cfg]`-gated: `serial_nexus_sys::Epoll` keeps a
// stub off Linux whose every method answers `ENOTSUP`, so this file has one code
// path on every platform and the probe reports `skipped` where epoll does not
// exist. A cfg arm here would be a second thing to keep in sync for no gain.
// ---------------------------------------------------------------------------

/// A ratio rounded to three decimals — enough to read off a report, few enough
/// digits that two runs of the same kernel produce the *same* number and a diff
/// shows only real differences.
fn ratio3(num: u64, den: u64) -> f64 {
    if den == 0 {
        return 0.0;
    }
    ((num as f64 / den as f64) * 1000.0).round() / 1000.0
}

/// Name an epoll event bitmask stably (`EPOLLIN|EPOLLHUP`), keeping any bit
/// `serial-nexus-sys` does not name — same discipline as [`revents_label`]: an
/// unfamiliar bit on the production kernel is the surprise this report exists to
/// surface, so it is printed in hex rather than dropped.
fn epoll_label(r: &sys::EpollReady) -> String {
    const KNOWN: [u32; 6] = [
        sys::EPOLLIN,
        sys::EPOLLPRI,
        sys::EPOLLOUT,
        sys::EPOLLERR,
        sys::EPOLLHUP,
        sys::EPOLLRDHUP,
    ];
    let mut parts: Vec<String> = r.flag_names().into_iter().map(str::to_owned).collect();
    let residual = r.events & !KNOWN.iter().fold(0u32, |a, b| a | b);
    if residual != 0 {
        parts.push(format!("0x{residual:x}"));
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join("|")
    }
}

/// One epoll-vs-`read(2)` sampling phase on a pty master. Every field is emitted;
/// the consequence line reads one of them, a cross-kernel diff reads them all.
#[derive(Default)]
struct EpollSpin {
    /// `epoll_wait` calls made, and how many returned at least one event.
    waits: u64,
    ready_waits: u64,
    events: u64,
    /// epoll event-mask label → count.
    flags: BTreeMap<String, u64>,
    /// Read-outcome class → count (`bytes` / `eof` / `EAGAIN` / `EIO` / …).
    reads: BTreeMap<String, u64>,
    bytes: u64,
    /// **The invariant-1 signature**: epoll reported the fd ready and the very
    /// next `read(2)` answered `EAGAIN`. A readiness guard that treats "ready" as
    /// an event and completes synchronously spins once per one of these.
    ready_then_eagain: u64,
    /// The broader form: epoll said ready and the read produced no bytes for any
    /// reason (`EAGAIN` **or** `EIO` after a hangup). Reported separately because
    /// a hung-up fd is level-ready forever by design, which is a different fact
    /// from a spurious readable.
    ready_then_no_data: u64,
    /// `poll(2)`'s answer to the same question, sampled in the same pass — the
    /// two disagreeing is the whole content of §15.18's claim.
    poll2_pollin: u64,
    /// The deepest `FIONREAD` seen: the kernel stating the queue depth outright,
    /// so "epoll says ready, read says EAGAIN" is a measurement and not an
    /// inference about who is lying.
    fionread_max: u64,
    elapsed_ms: u64,
}

impl EpollSpin {
    fn observations(&self, timeout_ms: u16) -> serde_json::Value {
        serde_json::json!({
            "registration": "level-triggered EPOLLIN",
            "epoll_wait_timeout_ms": timeout_ms,
            "epoll_waits": self.waits,
            "epoll_ready_waits": self.ready_waits,
            "epoll_events": self.events,
            "epoll_flags_seen": self.flags,
            "poll2_pollin_passes": self.poll2_pollin,
            "read_outcomes": self.reads,
            "bytes_read": self.bytes,
            "ready_then_eagain": self.ready_then_eagain,
            "ready_then_no_data": self.ready_then_no_data,
            "spin_ratio": ratio3(self.ready_then_eagain, self.waits),
            "fionread_max": self.fionread_max,
            "elapsed_ms": self.elapsed_ms,
        })
    }
}

/// P8's budget: 64 passes with a 1 ms epoll timeout and a 1 ms pause, twice —
/// ~256 ms worst case. The pause matters in the hung-up phase, where a
/// level-triggered set returns instantly on every call: without it the *doctor*
/// would busy-spin while asking whether the kernel makes a daemon do so.
const P8_PASSES: u32 = 64;
const P8_TIMEOUT_MS: u16 = 1;
const P8_PASS_PAUSE: Duration = Duration::from_millis(1);

/// Does epoll report a pty master readable while `read(2)` returns `EAGAIN`?
///
/// This is the kernel behaviour behind **invariant 1**. The daemon polls every
/// tty-family fd with non-blocking `poll(2)` plus an adaptive idle backoff
/// (`serial_nexus_sys::poll_ready`/`poll_blocking`) because tokio's readiness guard —
/// epoll underneath — reported a pty master persistently readable while `read(2)`
/// answered `EAGAIN`, so its ready future completed synchronously forever, the
/// loop never yielded, and the current-thread runtime starved with the control
/// plane inside it (§15.18/§15.19). That premise was never measured on the
/// production kernel.
///
/// **This probe is informational and the shipped design does not depend on its
/// answer** — the design is *justified* by it, and the justification survives
/// either result, because `poll(2)` is correct whether or not epoll agrees. Two
/// things follow, and both are deliberate:
///
/// * A box on which the busy-loop does **not** reproduce is `supported`, not
///   degraded. The starvation §15.18 records is a property of a readiness
///   *guard* (registration lifecycle plus a synchronously-completing ready
///   future), not of `epoll_ctl` in isolation, so a bare level-triggered
///   registration agreeing with `poll(2)` refutes nothing. The finding lives in
///   the numbers; the verdict only says the numbers exist.
/// * Both phases are measured, because they are different questions. With a
///   slave open and the master drained, "ready" would be spurious. After the last
///   slave closes, a level-triggered set reports `EPOLLHUP` on *every* call by
///   design and `read(2)` answers `EIO` — genuinely persistent readiness, counted
///   under `ready_then_no_data` rather than `ready_then_eagain` so the two are
///   never conflated.
pub fn p8_epoll_readiness() -> Probe {
    let p = Probe::new(
        "P8",
        "epoll vs read(2) on a pty master",
        "Does epoll report a pty master readable while read(2) returns EAGAIN — the busy-loop shape that made the data plane use poll(2) instead (invariant 1, §15.18)?",
    );
    match p8_inner() {
        Ok((idle, hungup)) => {
            let p = p
                .observe("slave_open_idle", idle.observations(P8_TIMEOUT_MS))
                .observe("after_slave_close", hungup.observations(P8_TIMEOUT_MS))
                .observe("busy_loop_reproduced", idle.ready_then_eagain > 0)
                .observe("epoll_agrees_with_poll2", idle.ready_waits == idle.poll2_pollin);
            let finding = if idle.ready_then_eagain > 0 {
                format!(
                    "REPRODUCED: with a slave open and nothing to read, epoll reported the master ready on {} of {} waits and the following read(2) answered EAGAIN {} time(s) (poll(2) said POLLIN on {} passes, FIONREAD peaked at {}). This is §15.18's shape verbatim — a readiness guard built on it spins, and the daemon's poll(2)-plus-backoff readiness is load-bearing here.",
                    idle.ready_waits, idle.waits, idle.ready_then_eagain, idle.poll2_pollin, idle.fionread_max
                )
            } else {
                format!(
                    "NOT reproduced at this layer: a bare level-triggered EPOLLIN registration agreed with poll(2) on this kernel ({} of {} waits ready, poll(2) POLLIN on {} passes, 0 reads answering EAGAIN after a ready report). Read that as scoped, not as a refutation — the starvation §15.18 records is a property of tokio's readiness guard (registration lifecycle + a synchronously-completing ready future), not of epoll_ctl alone, so invariant 1 stands and nothing here licenses putting epoll back in the data plane.",
                    idle.ready_waits, idle.waits, idle.poll2_pollin
                )
            };
            p.verdict(
                Status::Supported,
                &format!(
                    "{finding} After the last slave closed, the level-triggered set reported an event on {} of {} waits ({} of them with no bytes to read) — persistent readiness on a hung-up fd is expected and is why the PTY reader branches on POLLHUP rather than looping on readability. Diff both blocks against the production kernel (6.18) before drawing any conclusion from either.",
                    hungup.ready_waits, hungup.waits, hungup.ready_then_no_data
                ),
            )
        }
        // Never `unsupported`, and never `degraded`: the design does not rest on
        // this answer, so a kernel without epoll (macOS, §13) or a probe that could
        // not run leaves nothing at risk — it leaves a question unmeasured, which is
        // what `skipped` says. The reason carries the errno so a failure is visible
        // rather than silent.
        Err(e) if is_unsupported_errno(&e) => p.verdict(
            Status::skipped("epoll is Linux-only"),
            "epoll(7) has no portable equivalent, and the data plane is forbidden from using it anyway (invariant 1) — nothing here is untested, only unmeasurable on this platform (§13).",
        ),
        Err(e) => p.verdict(
            Status::skipped(format!("probe error: {e}")),
            "The epoll/read comparison did not run; invariant 1's justification is unmeasured on this box, and the data plane's poll(2) readiness is unaffected either way (§15.18).",
        ),
    }
}

fn p8_inner() -> anyhow::Result<(EpollSpin, EpollSpin)> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    // The master exactly as the PTY node holds it: non-blocking, §7.2 baseline,
    // packet mode. Measuring some other fd's readiness would answer some other
    // question (§15.18 is a claim about this fd in this configuration).
    sys::set_nonblocking(fd)?;
    apply_pty_baseline(&master, &pts)?;
    sys::set_packet_mode(fd, true)?;

    let mut buf = [0u8; 4096];
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    std::thread::sleep(PTY_SETTLE);
    // Drained first: "epoll says ready" is only interesting when there is provably
    // nothing to read, so anything setup and attach left behind is consumed here.
    let _ = read_available(fd, &mut buf, 64);
    let idle = p8_phase(fd)?;

    drop(slave);
    std::thread::sleep(PTY_SETTLE);
    let hungup = p8_phase(fd)?;
    Ok((idle, hungup))
}

/// One sampling phase: a fresh epoll set (registering the same fd twice in one
/// set is `EEXIST`), then `epoll_wait` → `poll(2)` → `FIONREAD` → `read(2)` per
/// pass, so all four answers about the same fd come from the same instant.
fn p8_phase(fd: RawFd) -> anyhow::Result<EpollSpin> {
    let ep = sys::Epoll::new()?;
    ep.add_level_readable(fd)?;

    let mut o = EpollSpin::default();
    let mut buf = [0u8; 4096];
    let start = Instant::now();
    for _ in 0..P8_PASSES {
        let ready = ep.wait(P8_TIMEOUT_MS, 8)?;
        o.waits += 1;
        if !ready.is_empty() {
            o.ready_waits += 1;
        }
        for r in &ready {
            o.events += 1;
            *o.flags.entry(epoll_label(r)).or_default() += 1;
        }
        if sys::poll_ready(fd, PollFlags::POLLIN).contains(PollFlags::POLLIN) {
            o.poll2_pollin += 1;
        }
        if let Ok(n) = sys::pending_input_bytes(fd) {
            o.fionread_max = o.fionread_max.max(n as u64);
        }
        // Read on every pass, not only the ready ones: "what does read(2) say when
        // epoll said nothing" is the other half of the comparison, and the fd is
        // non-blocking so it cannot hang.
        let (class, n) = match sys::read_fd(fd, &mut buf) {
            Ok(0) => ("eof".to_owned(), 0usize),
            Ok(n) => ("bytes".to_owned(), n),
            Err(e) => (read_class(&e), 0),
        };
        if !ready.is_empty() {
            if n == 0 {
                o.ready_then_no_data += 1;
            }
            if class == "EAGAIN" {
                o.ready_then_eagain += 1;
            }
        }
        *o.reads.entry(class).or_default() += 1;
        o.bytes += n as u64;
        std::thread::sleep(P8_PASS_PAUSE);
    }
    o.elapsed_ms = start.elapsed().as_millis() as u64;
    Ok(o)
}

/// Whether an error is "this platform does not implement it" — the `ENOTSUP` the
/// `serial-nexus-sys` stubs answer off Linux (`Epoll`, `pending_output_bytes`,
/// `read_icounts`). A probe turns *this* error into `skipped` with a platform
/// reason, and any other error into `skipped` with the error text, so a genuine
/// failure never hides behind "Linux-only".
fn is_unsupported_errno(e: &anyhow::Error) -> bool {
    if let Some(io) = e.downcast_ref::<std::io::Error>() {
        return io.raw_os_error() == Some(libc::ENOTSUP);
    }
    if let Some(errno) = e.downcast_ref::<nix::Error>() {
        return *errno == nix::errno::Errno::ENOTSUP;
    }
    false
}

// ---------------------------------------------------------------------------
// P9 — poll(2) timeout granularity (§15.19's timer floor, §13)
// ---------------------------------------------------------------------------

/// The requested timeouts, and how many samples each gets. Four timeouts × 16
/// samples ≈ 0 + 16 + 80 + 160 ms ≈ **0.26 s** added to a doctor run — the
/// doctor is run interactively, so the sample count is chosen to keep it under a
/// third of a second rather than to please a statistician. Min/median/max over 16
/// samples is enough to see a timer floor; it is not enough to characterize a
/// tail, and the report says so.
const P9_TIMEOUTS_MS: [u16; 4] = [0, 1, 5, 10];
const P9_SAMPLES: usize = 16;

/// Samples per reference shape, matched to P2's count so sample size is not one of
/// the variables. See [`ZeroTimeoutRefs`].
const P9_REF_SAMPLES: usize = 4096;

/// What one requested timeout actually cost. Sampled in **nanoseconds** and
/// reported in microseconds, plus `median_ns`: the 0 ms row is the cost of
/// asking, which is sub-microsecond and would diff as a constant `0 µs` on every
/// kernel — a number that cannot differ is not worth printing (P2 reports its
/// zero-timeout poll in ns for the same reason).
/// The 2x2 that decomposes "the cost of a zero-timeout poll" — a phrase two probes
/// use for numbers that differ by an order of magnitude on one kernel.
///
/// P2 reports `zero_timeout_poll_ns_median` and P9 reports `median_ns_for_0ms_request`,
/// and they are **not the same measurement**: P2's fd is *hung up* and its mask is
/// *POLLHUP only*, so every pass returns ready; P9's fd has a slave open and asks
/// about *POLLIN*, so no pass ever does. On Linux 7.0.0-29 that distinction costs
/// nothing (263 vs 195 ns in the committed captures). On Darwin 24.6.0 the two
/// disagree **8-11x** across three captures — 2091/2102/2086 ns against
/// 22832/22980/16098 ns — with tight per-sample distributions, so it is neither an
/// outlier nor a cold-start artifact, and sample count does not explain it either
/// (n=16 and n=4096 medians agree on Linux).
///
/// An order-of-magnitude disagreement between two probes naming the same operation
/// is either a kernel property or an instrument error, and nothing in the set could
/// tell which. So P9 reproduces P2's shape itself, in one pty, on one clock, varying
/// one thing at a time. The next capture decomposes the gap instead of posing it.
///
/// **Cost** is four times [`P9_REF_SAMPLES`] zero-timeout polls — microseconds on a
/// kernel where the poll is cheap, and bounded by the very number being measured on
/// one where it is not (~0.4 s at Darwin's captured 22 µs, the worst case).
struct ZeroTimeoutRefs {
    /// P9's own shape at P2's sample count: slave open, POLLIN, never ready.
    unready_pollin_ns: u64,
    /// Isolates the mask with the fd state held unready.
    unready_pollhup_ns: u64,
    /// Isolates the fd state with the mask held at POLLIN.
    ready_pollin_ns: u64,
    /// **P2's shape verbatim**: hung-up master, POLLHUP, ready every pass.
    ready_pollhup_ns: u64,
    /// The contamination detector again, at the reference sample count.
    ready_passes_on_unready_fd: u64,
}

/// Median nanoseconds for [`P9_REF_SAMPLES`] zero-timeout polls, and how many
/// reported an event. Uses the shipped `poll_blocking` wrapper for the reason
/// [`p9_poll_granularity`] gives: a difference the wrapper introduces is as real
/// as one the kernel does.
fn p9_zero_median(fd: RawFd, interest: PollFlags) -> (u64, u64) {
    let mut samples = Vec::with_capacity(P9_REF_SAMPLES);
    let mut ready = 0u64;
    for _ in 0..P9_REF_SAMPLES {
        let start = Instant::now();
        let revents = sys::poll_blocking(fd, interest, 0);
        samples.push(start.elapsed().as_nanos() as u64);
        if !revents.is_empty() {
            ready += 1;
        }
    }
    samples.sort_unstable();
    (samples[samples.len() / 2], ready)
}

struct Granularity {
    requested_ms: u16,
    min_ns: u64,
    median_ns: u64,
    max_ns: u64,
    /// Passes where the fd reported something instead of timing out. Must be 0
    /// for the numbers to mean what they say — reported so a contaminated sample
    /// is visible rather than averaged in silently.
    ready_passes: u64,
}

impl Granularity {
    fn median_us(&self) -> u64 {
        self.median_ns / 1000
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "requested_ms": self.requested_ms,
            "requested_us": u64::from(self.requested_ms) * 1000,
            "samples": P9_SAMPLES,
            "min_us": self.min_ns / 1000,
            "median_us": self.median_us(),
            "max_us": self.max_ns / 1000,
            "median_ns": self.median_ns,
            "overshoot_median_us": self.median_us() as i64 - i64::from(self.requested_ms) * 1000,
            "ready_passes": self.ready_passes,
        })
    }
}

/// What does a requested `poll(2)` timeout actually cost?
///
/// Two shipped decisions read this number. §15.19's hybrid data plane exists
/// because the async poll loop's per-iteration timer floor capped a pty writer at
/// ~1 MB/s while a blocking `poll(2)` on a dedicated thread reached ~185 MiB/s —
/// a conclusion about *timer* cost, not about `read(2)`. And
/// `serial_nexus_sys::poll_ready`'s adaptive idle backoff picks its sleep steps against
/// the same floor: a backoff below the floor buys latency it cannot deliver and
/// spends CPU pretending.
///
/// Measured through `serial_nexus_sys::poll_blocking` — the shipped wrapper, not a raw
/// syscall — because a difference introduced by the wrapper is as real as one
/// introduced by the kernel, and on a pty master with an open slave, which is the
/// fd class the daemon actually parks on.
pub fn p9_poll_granularity() -> Probe {
    let p = Probe::new(
        "P9",
        "poll(2) timeout granularity",
        "For a never-ready tty fd, what does a requested poll(2) timeout of 0/1/5/10 ms actually cost (min/median/max, µs)?",
    );
    match p9_inner() {
        Ok((rows, refs)) => {
            let mut p = p;
            let mut one_ms = 0u64;
            let mut contaminated = 0u64;
            for row in &rows {
                p = p.observe(
                    &format!("poll_timeout_{}ms", row.requested_ms),
                    row.observations(),
                );
                if row.requested_ms == 1 {
                    one_ms = row.median_us();
                }
                contaminated += row.ready_passes;
            }
            let zero = rows.first().map(|r| r.median_ns).unwrap_or(0);
            p = p
                .observe("median_us_for_1ms_request", one_ms)
                .observe("median_ns_for_0ms_request", zero)
                .observe("ready_passes_total", contaminated);
            // The 2x2 that decomposes the phrase "zero-timeout poll", which P2 and P9
            // both use for numbers that differ 8-11x on Darwin and agree on Linux.
            p = p.observe(
                "zero_timeout_by_fd_state_and_mask",
                serde_json::json!({
                    "samples_each": P9_REF_SAMPLES,
                    "unready_master_pollin_ns": refs.unready_pollin_ns,
                    "unready_master_pollhup_ns": refs.unready_pollhup_ns,
                    "ready_hungup_master_pollin_ns": refs.ready_pollin_ns,
                    "ready_hungup_master_pollhup_ns": refs.ready_pollhup_ns,
                    "p2_reports_the_shape": "ready_hungup_master_pollhup_ns",
                    "the_data_plane_parks_on": "unready_master_pollin_ns",
                    "ready_passes_on_unready_fd": refs.ready_passes_on_unready_fd,
                }),
            );
            p.verdict(
                Status::Supported,
                &format!(
                    "A zero timeout costs {zero} ns median (the cost of asking) and a requested 1 ms costs {one_ms} µs median on this kernel — that is the floor §15.19's hybrid data plane was built around and the floor poll_ready's idle backoff steps against. 16 samples per timeout: enough to see the floor, not enough to characterize a tail. Diff these against the production kernel (6.18) before tuning any backoff step or timer against them.{}",
                    if contaminated > 0 {
                        format!(" NOTE: {contaminated} pass(es) returned early because the fd reported an event, so those samples measure readiness rather than the timeout — treat the affected rows as suspect.")
                    } else {
                        String::new()
                    }
                ),
            )
        }
        // Nothing in the design rests on a *measurement* being available — the
        // floor is whatever it is, and the code copes with any value — so an
        // unmeasured probe is `skipped`, never `unsupported` or `degraded`.
        Err(e) => p.verdict(
            Status::skipped(format!("probe error: {e}")),
            "Timeout granularity unmeasured on this box; §15.19's blocking-thread hot path and poll_ready's backoff are unaffected (both are correct at any floor).",
        ),
    }
}

fn p9_inner() -> anyhow::Result<(Vec<Granularity>, ZeroTimeoutRefs)> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let fd = master.as_raw_fd();
    sys::set_nonblocking(fd)?;
    apply_pty_baseline(&master, &pts)?;
    // The slave stays open for the whole measurement: a hung-up master reports
    // POLLHUP and every poll returns instantly, which would measure nothing.
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    std::thread::sleep(PTY_SETTLE);
    let mut buf = [0u8; 4096];
    let _ = read_available(fd, &mut buf, 64);

    let mut rows = Vec::new();
    for requested_ms in P9_TIMEOUTS_MS {
        let mut samples = Vec::with_capacity(P9_SAMPLES);
        let mut ready_passes = 0u64;
        for _ in 0..P9_SAMPLES {
            let start = Instant::now();
            let revents = sys::poll_blocking(fd, PollFlags::POLLIN, requested_ms);
            samples.push(start.elapsed().as_nanos() as u64);
            if !revents.is_empty() {
                ready_passes += 1;
            }
        }
        samples.sort_unstable();
        rows.push(Granularity {
            requested_ms,
            min_ns: samples[0],
            median_ns: samples[samples.len() / 2],
            max_ns: samples[samples.len() - 1],
            ready_passes,
        });
    }
    // The reference 2x2, after the timeout rows so the slave is still open for
    // them: a hung-up master returns instantly from every poll, which would
    // measure nothing. Unready cells first, then the same two masks once the
    // session is gone.
    let (unready_pollin_ns, ready_passes_on_unready_fd) = p9_zero_median(fd, PollFlags::POLLIN);
    let (unready_pollhup_ns, _) = p9_zero_median(fd, PollFlags::POLLHUP);
    drop(slave);
    std::thread::sleep(PTY_SETTLE);
    let (ready_pollin_ns, _) = p9_zero_median(fd, PollFlags::POLLIN);
    let (ready_pollhup_ns, _) = p9_zero_median(fd, PollFlags::POLLHUP);

    Ok((
        rows,
        ZeroTimeoutRefs {
            unready_pollin_ns,
            unready_pollhup_ns,
            ready_pollin_ns,
            ready_pollhup_ns,
            ready_passes_on_unready_fd,
        },
    ))
}

// ---------------------------------------------------------------------------
// P10 — pty buffer depth (§5 hostward_buffer defaults, §7.2)
// ---------------------------------------------------------------------------

/// Fill in 4 KiB chunks up to a hard 4 MiB ceiling. The ceiling is a backstop,
/// not an expectation: a pty that never says `EAGAIN` (a kernel that grows the
/// buffer, or a peer draining it) must end this probe by ceiling rather than by
/// running until someone notices — **it must never block or hang**, which is why
/// both fds are non-blocking and why the loop is bounded twice.
const P10_CHUNK: usize = 4096;
const P10_CEILING: u64 = 4 * 1024 * 1024;

/// The recheck's partial drain (notes §3.44): how many bytes to hand back to the
/// peer before asking the kernel for room again. Smaller than the smallest depth
/// either kernel of record has ever reported (Darwin's 1022 is the floor), so the
/// drain never empties the queue and the top-up measures *republished* room rather
/// than a fresh fill.
const P10_RECHECK_DRAIN: u64 = 512;

/// The recheck's top-up is byte-granular — the whole point is the exact blocking
/// point, not a 4096-quantized one — so it needs its own, much smaller backstop:
/// 4 MiB of one-byte writes is four million syscalls.
const P10_RECHECK_CEILING: u64 = 64 * 1024;

/// How long to let the tty's asynchronous flip-buffer work run before the second
/// fill pass. See [`FillResult::settled_bytes`] for why there is a second pass at
/// all.
const P10_SETTLE: Duration = PTY_SETTLE;

/// What one direction of a pty accepted before it would have blocked.
/// P10's second question, asked of the same pair once the first is answered:
/// **is the number above a queue capacity, or a per-fill allowance?**
///
/// The two are indistinguishable in a single fill, and the difference is exactly
/// what the 2026-08-05 Darwin capture left open — 1024 bytes targetward against
/// 1022 hostward, byte-identical across three runs, with no probe asking why
/// (`docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`). A capacity
/// bound republishes exactly the room a reader frees; a per-fill allowance charges
/// its reservation again. So: drain the peer completely, refill from empty, hand
/// back [`P10_RECHECK_DRAIN`] bytes, let the tty's asynchronous work run, and write
/// **one byte at a time** until it blocks again.
///
/// **Calibrated on Linux before it was pre-registered for anywhere else**
/// (7.0.0-29): `refilled` reproduces `total()` only about half the time — the
/// flip-scheduling spread [`FillResult::settled_bytes`] documents — `drained_again`
/// is reliably [`P10_RECHECK_DRAIN`], and `topped_up` exceeds it. Linux answers
/// "neither, exactly": its bound is a moving snapshot of an asynchronous pipeline,
/// and it hands back *more* room than was freed because the pipeline advanced
/// during the settle. Read a Darwin `room_republished_minus_room_freed` of 0
/// against that, not against zero expectation.
#[derive(Default)]
struct Recheck {
    /// What the same pair accepts when refilled from empty. Equal to `total()` on a
    /// kernel whose bound is a fixed queue capacity; **not** equal on one whose
    /// bound is a snapshot of asynchronous work.
    refilled: u64,
    refill_writes: u64,
    refill_terminal: String,
    /// What the partial drain actually took back — never assumed to be
    /// [`P10_RECHECK_DRAIN`], because a kernel shallower than that would give less.
    drained_again: u64,
    /// What the kernel then accepted, one byte at a time.
    topped_up: u64,
    topup_writes: u64,
    topup_terminal: String,
    topup_ceiling_hit: bool,
}

struct FillResult {
    bytes: u64,
    writes: u64,
    /// Bytes the *same* fd accepted after a short pause, with still nothing
    /// draining the far end.
    ///
    /// This is not padding, it is the measurement's honesty. A tty moves bytes
    /// from the driver buffer into the line discipline's read buffer on an
    /// **asynchronous** work item, so a single-pass fill measures "how much fits
    /// before EAGAIN *at this instant*" and lands 11776, 13824 or 15360 bytes on
    /// the same kernel depending on whether that work ran mid-fill. Reporting one
    /// number would have handed the 6.18 diff a scheduling race dressed as a
    /// cross-kernel difference. So both are reported, and the pair says which
    /// happened: a first pass short by a chunk with a matching `settled_extra`
    /// **is** the late-flip case.
    ///
    /// That it is a race and not a kernel property is now measured rather than
    /// argued: three *sequential* 7.0 runs on an idle box produced all three
    /// first-pass values, and the two 6.18 runs produced two of them — the two
    /// kernels having swapped shapes between 2026-07-27 and 2026-07-29
    /// (`docs/doctor/`). An earlier version of this comment guessed the 13824 case
    /// needed "several doctors running concurrently"; run 2 of three, alone on an
    /// idle box, says otherwise.
    ///
    /// Neither figure is exact, and no arrangement of passes would make it so —
    /// on 7.0 the first pass measured 11776–15360 and the total 13824–15360. Read
    /// a one-chunk difference across kernels as noise and only an
    /// order-of-magnitude one as signal; the number this probe exists to give is
    /// the *scale* of the pipe under a `hostward_buffer`, not its last byte.
    settled_bytes: u64,
    settled_writes: u64,
    ceiling_hit: bool,
    /// Why the first fill stopped: `EAGAIN` (the answer being measured),
    /// `ceiling`, or an errno. Classified by the same errno classifier the read
    /// paths use — a write's `EAGAIN` is the same errno and means the same thing
    /// here.
    terminal: String,
    /// Why the second (post-settle) fill stopped.
    settled_terminal: String,
    /// `FIONREAD` on the *reading* end after the settle: where the bytes went. A
    /// depth short of `bytes` means the rest sits in a buffer the reader cannot
    /// see yet (on Linux this caps at the ldisc read buffer, ~4 KiB, which is the
    /// number that explains the second pass).
    peer_pending_input: Option<u64>,
    /// `TIOCOUTQ` on the written fd. **Reported, never judged** — a pty has no
    /// transmitter to drain and answers 0 on Linux 7.0 in every state; whether a
    /// given kernel accounts for a pty here at all is exactly the quiet
    /// cross-kernel difference worth printing.
    pending_output: Option<u64>,
    /// The line discipline the measured slave was actually in — `raw` (the daemon's
    /// baseline), `cooked`, or `unknown`. A depth means nothing without it (see
    /// [`termios_mode`]), and a report that omitted it could not say whether a
    /// cross-kernel gap was the kernel or the probe's own configuration.
    slave_mode: &'static str,
    /// Bytes the peer could actually be given back, against `total()` accepted.
    /// The field that tells a deep buffer from a black hole.
    recovered: u64,
    /// The capacity-versus-reservation recheck (notes §3.44). Runs **after** every
    /// field above is final and after `recovered`'s drain has emptied the peer, so
    /// no existing observation can move: an artifact taken before this landed and
    /// one taken after stay field-by-field diffable (§16.13).
    recheck: Recheck,
}

impl FillResult {
    fn total(&self) -> u64 {
        self.bytes + self.settled_bytes
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "bytes_accepted_before_eagain": self.bytes,
            "writes": self.writes,
            "settled_extra_bytes": self.settled_bytes,
            "settled_extra_writes": self.settled_writes,
            "settle_ms": P10_SETTLE.as_millis() as u64,
            "total_bytes_accepted": self.total(),
            "chunk_bytes": P10_CHUNK,
            "ceiling_hit": self.ceiling_hit,
            "ceiling_bytes": P10_CEILING,
            "terminal_write": self.terminal,
            "terminal_write_after_settle": self.settled_terminal,
            "peer_pending_input_bytes": self.peer_pending_input,
            "pending_output_bytes": self.pending_output,
            "slave_termios_mode": self.slave_mode,
            "bytes_recovered_by_peer": self.recovered,
            "bytes_unrecoverable": self.total().saturating_sub(self.recovered),
            "recheck": {
                "refilled_from_empty_bytes": self.recheck.refilled,
                "refilled_from_empty_writes": self.recheck.refill_writes,
                "refill_terminal_write": self.recheck.refill_terminal,
                "refill_reproduced_total": self.recheck.refilled == self.total(),
                "drained_again_bytes": self.recheck.drained_again,
                "topped_up_bytes": self.recheck.topped_up,
                "topped_up_writes": self.recheck.topup_writes,
                "topup_terminal_write": self.recheck.topup_terminal,
                "topup_chunk_bytes": 1,
                "topup_ceiling_bytes": P10_RECHECK_CEILING,
                "topup_ceiling_hit": self.recheck.topup_ceiling_hit,
                // The field the Darwin question turns on. Zero means the kernel
                // republished exactly the room the reader freed, which is what a
                // fixed queue capacity does; a negative number means a reservation
                // is being charged per fill; Linux reads positive, because its
                // asynchronous pipeline advanced during the settle.
                "room_republished_minus_room_freed":
                    self.recheck.topped_up as i64 - self.recheck.drained_again as i64,
            },
        })
    }

    /// Did every accepted byte come back? The one-line read of the two fields above.
    fn fully_recoverable(&self) -> bool {
        self.recovered >= self.total()
    }
}

/// How many bytes will a pty accept with nothing draining the other end?
///
/// This sizes the queues that sit *above* it. `serial-nexus-core`'s `hostward_buffer`
/// defaults (256 chunks for a serial node, **32 for a pty**) bound the daemon's
/// own mpsc depth per consumer; the kernel's pty buffer is the pipe underneath,
/// and a daemon queue much larger than the pipe below it only defers the same
/// backpressure while a much smaller one throws away headroom the kernel would
/// have given for free. The 32-chunk pty default produced a CI flake this
/// session, which is precisely the situation where knowing the real depth beats
/// guessing at it.
///
/// Both directions are measured because they are different buffers, and which is
/// which is the opposite of the intuitive reading: a `pty` node holds the **master**
/// and its client holds the slave, so **slave→master is *targetward*** — the client
/// typing, travelling toward the device, which `nodes/pty.rs` reads off the master
/// and hands to `try_forward_targetward` — while master→slave is *hostward*, the
/// node delivering device output to its client. Each runs on its own pair so neither
/// fill perturbs the other.
///
/// Both labels were inverted here through the 2026-07-30 artifacts, in the keys and
/// in the consequence prose alike. The cost was not cosmetic: the number an operator
/// reads when sizing `hostward_buffer` was the *other* direction's depth. Corrected
/// 2026-08-04. The `probe_set` fingerprint covers ids and questions, so it does not
/// move; a diff against an older report shows the two keys renamed with their
/// numbers unchanged, which is exactly the swap.
pub fn p10_pty_buffer_depth() -> Probe {
    let p = Probe::new(
        "P10",
        "pty buffer depth",
        "How many bytes does a pty accept in each direction before it would block, with nothing draining the other end?",
    );
    match (p10_fill_direction(true), p10_fill_direction(false)) {
        (Ok(targetward), Ok(hostward)) => {
            let p = p
                .observe("slave_to_master_targetward", targetward.observations())
                .observe("master_to_slave_hostward", hostward.observations());
            // A depth measured in the wrong line discipline is not this kernel's
            // depth (see `termios_mode`), so the mode decides the verdict: §7 says
            // a run that could not ask the intended question reports `degraded`
            // with the observation named, never a confident number.
            let raw_both = targetward.slave_mode == "raw" && hostward.slave_mode == "raw";
            let status = if raw_both {
                Status::Supported
            } else {
                Status::Degraded
            };
            p.verdict(
                status,
                &format!(
                    "This kernel's pty accepted {} byte(s) slave→master (**targetward** — a client typing, travelling toward the device, first pass ending in `{}`) and {} byte(s) master→slave (**hostward** — the node delivering device output to its client, ending in `{}`), reaching {} and {} in total once a short pause has let the tty's asynchronous flip work run. **Of those, {} and {} byte(s) were actually recoverable by the peer** ({} / {}): acceptance is not delivery, and the two are the same number only on a kernel that queues everything it takes. Read the daemon's `hostward_buffer` defaults against the SCALE of these, not their last byte: the pty default is 32 chunks, and a queue far larger than the kernel pipe below it only defers the same backpressure. Both figures move by a chunk or two run to run depending on when that flip work lands, so a one-chunk difference across kernels is noise; only an order-of-magnitude one is signal, **and only between runs whose `slave_termios_mode` agrees** — a cooked pty and a raw one give different depths on one kernel (measured on Linux 7.0.0-29: raw ~13.8 KiB fully recoverable, cooked ~23.5 KiB with nothing recoverable), so a mode mismatch explains a gap before any kernel difference does. The `recheck` block under each direction asks the second question the first cannot: after the peer is drained, the pair is refilled from empty and then handed back 512 bytes, and `room_republished_minus_room_freed` says whether the kernel gave back exactly the room a reader freed (a fixed queue capacity), or more (an asynchronous pipeline that advanced during the settle — Linux 7.0.0-29 reads +2048 or +9216, bimodal, never 0 across 20 samples), or less (a reservation charged per fill). `refill_reproduced_total` says whether the depth above is reproducible on the same pair at all; on Linux it usually is not. Numbers, not a verdict — diff them against the production kernel (6.18) before changing a default.{}{}",
                    targetward.bytes,
                    targetward.terminal,
                    hostward.bytes,
                    hostward.terminal,
                    targetward.total(),
                    hostward.total(),
                    targetward.recovered,
                    hostward.recovered,
                    if targetward.fully_recoverable() { "all of it" } else { "short" },
                    if hostward.fully_recoverable() { "all of it" } else { "short" },
                    if targetward.ceiling_hit || hostward.ceiling_hit {
                        format!(" NOTE: a direction hit the {P10_CEILING}-byte ceiling rather than ever answering EAGAIN, so its depth is a lower bound and this kernel's blocking point was not observed at all.")
                    } else {
                        String::new()
                    },
                    if raw_both {
                        String::new()
                    } else {
                        format!(" DEGRADED: the measured slave was `{}` targetward and `{}` hostward, not the raw baseline the daemon runs (§7.2), so these depths are not the daemon's configuration and must not be diffed against a run that was raw.", targetward.slave_mode, hostward.slave_mode)
                    }
                ),
            )
        }
        // Nothing is contradicted by an unmeasured buffer depth: the defaults are
        // configuration, and every one of them works at any depth (backpressure is
        // the mechanism either way, §5). So `skipped`, never `unsupported`.
        (Err(e), _) | (_, Err(e)) => p.verdict(
            Status::skipped(format!("probe error: {e}")),
            "Pty buffer depth unmeasured on this box; the hostward_buffer defaults are unaffected (they bound the daemon's own queue, and backpressure holds at any kernel depth, §5).",
        ),
    }
}

/// Fill one direction of a fresh pty pair. `targetward` selects master→slave
/// (what the node writes to its client); otherwise slave→master.
fn p10_fill_direction(targetward: bool) -> anyhow::Result<FillResult> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    let master_fd = master.as_raw_fd();
    sys::set_nonblocking(master_fd)?;
    apply_pty_baseline(&master, &pts)?;

    let slave = open(
        pts.as_str(),
        OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?;
    let slave_fd = slave.as_raw_fd();
    // Belt and braces: the open flag above already asks for it, and a blocking fd
    // here is the one way this probe could hang.
    sys::set_nonblocking(slave_fd)?;

    // Re-assert the baseline on **the slave this probe actually measures**, not on
    // the one `apply_pty_baseline` opened and closed above. Where the master is a
    // terminal this is idempotent — the pair is already raw and the Linux figures do
    // not move — and where it is not, this is the difference between measuring the
    // daemon's pty and measuring a cooked one the daemon never runs (see
    // [`termios_mode`]). Applied on every platform rather than behind a
    // `cfg(not(linux))`: a repair that only ever executes off the platform of record
    // is a §9 proxy in space, exercised nowhere it can be observed failing.
    let _ = set_baseline(&slave);
    let mode = termios_mode(&slave);
    std::thread::sleep(PTY_SETTLE);

    // Which fd is written decides which *graph* direction is being filled, and the
    // mapping is the opposite of what it looks like. A pty node holds the **master**
    // and its client holds the slave; `nodes/pty.rs` reads the master and calls
    // `try_forward_targetward` on what it finds. So **slave → master is targetward**
    // (the client typing, travelling toward the device) and master → slave is
    // hostward (the node delivering device output to its client). This was inverted
    // here through the 2026-07-30 artifacts: both keys and the consequence prose
    // named each direction as the other, so an operator sizing `hostward_buffer`
    // against the printed number was reading the wrong direction's depth. Corrected
    // 2026-08-04; a diff against an older `docs/doctor/` report shows the keys
    // renamed, and the *numbers* under them swapped meaning rather than changing.
    let (write_to, peer) = if targetward {
        (slave_fd, master_fd)
    } else {
        (master_fd, slave_fd)
    };
    Ok(p10_fill(write_to, peer, mode))
}

fn p10_fill(write_to: RawFd, peer: RawFd, slave_mode: &'static str) -> FillResult {
    // Pass one: what a writer hits right now.
    let (bytes, writes, terminal, hit_a) = p10_fill_pass(write_to, 0);
    // Let the tty's asynchronous flip work run, then sample where the bytes went
    // and take pass two: the steady-state depth. Two passes, both bounded by the
    // same ceiling — this cannot loop and cannot block (the fd is non-blocking).
    std::thread::sleep(P10_SETTLE);
    let peer_pending_input = sys::pending_input_bytes(peer).ok().map(|n| n as u64);
    let pending_output = sys::pending_output_bytes(write_to).ok().map(|n| n as u64);
    let (settled_bytes, settled_writes, settled_terminal, hit_b) = p10_fill_pass(write_to, bytes);

    // **The measurement this probe was missing.** Everything above counts what
    // `write(2)` *accepted*, which is not what a reader can *get*: a kernel that
    // takes 4 MiB and hands back nothing scores identically to one holding 4 MiB
    // ready. Draining the peer separates them, and it is the only field here that
    // can — `peer_pending_input` is a `FIONREAD` best-effort and reads 0 for bytes
    // sitting in a canonical queue that a reader will never be given.
    //
    // Runs last, after every other observation, because it is the one step that
    // changes the state it measures. Bounded by the same ceiling and reading a
    // non-blocking fd, so it can neither loop nor block.
    let recovered = p10_drain(peer);

    // Everything above is final and the peer is now empty, which is both the
    // precondition the recheck needs and the reason it runs last: an observation
    // that moved `bytes` or `recovered` would break the cross-report diff (§16.13).
    let recheck = p10_recheck(write_to, peer);

    FillResult {
        bytes,
        writes,
        settled_bytes,
        settled_writes,
        ceiling_hit: hit_a || hit_b,
        terminal,
        settled_terminal,
        peer_pending_input,
        pending_output,
        slave_mode,
        recovered,
        recheck,
    }
}

/// Read the peer dry and count what came back. Non-blocking, so `EAGAIN` is the
/// ordinary end of the drain rather than an error.
fn p10_drain(peer: RawFd) -> u64 {
    p10_drain_at_most(peer, P10_CEILING)
}

/// [`p10_drain`] with the cap named. The recheck needs a *partial* drain: handing
/// back a known amount of room and asking for it again is the measurement.
fn p10_drain_at_most(peer: RawFd, cap: u64) -> u64 {
    let mut buf = [0u8; 65536];
    let mut recovered = 0u64;
    while recovered < cap {
        let want = usize::try_from(cap - recovered)
            .unwrap_or(buf.len())
            .min(buf.len());
        match sys::read_fd(peer, &mut buf[..want]) {
            Ok(0) => break,
            Ok(n) => recovered += n as u64,
            Err(_) => break,
        }
    }
    recovered
}

/// See [`Recheck`]. **Precondition: the peer is already drained** — the refill
/// measures a fill from empty, and starting it against a full queue would report 0
/// and say nothing.
fn p10_recheck(write_to: RawFd, peer: RawFd) -> Recheck {
    let (refilled, refill_writes, refill_terminal, _) = p10_fill_pass(write_to, 0);
    let drained_again = p10_drain_at_most(peer, P10_RECHECK_DRAIN);
    // The same settle the second pass uses. On a kernel that moves bytes on an
    // asynchronous work item, room appears only after it runs, so measuring before
    // it does would report that kernel's scheduling rather than its bookkeeping.
    std::thread::sleep(P10_SETTLE);
    let (topped_up, topup_writes, topup_terminal, topup_ceiling_hit) =
        p10_fill_pass_with(write_to, 0, 1, P10_RECHECK_CEILING);
    // Leave the pair as this function found it.
    let _ = p10_drain(peer);
    Recheck {
        refilled,
        refill_writes,
        refill_terminal,
        drained_again,
        topped_up,
        topup_writes,
        topup_terminal,
        topup_ceiling_hit,
    }
}

/// One bounded fill pass. `already` is what earlier passes wrote, so the 4 MiB
/// ceiling bounds the *total* rather than each pass — the backstop has to hold
/// across the whole probe or it is not a backstop.
fn p10_fill_pass(write_to: RawFd, already: u64) -> (u64, u64, String, bool) {
    p10_fill_pass_with(write_to, already, P10_CHUNK, P10_CEILING)
}

/// [`p10_fill_pass`] with the write size and the backstop named. The recheck's
/// top-up writes one byte at a time — it is measuring the exact blocking point, not
/// a 4096-quantized one — and so needs its own, much smaller ceiling.
fn p10_fill_pass_with(
    write_to: RawFd,
    already: u64,
    chunk: usize,
    ceiling: u64,
) -> (u64, u64, String, bool) {
    let buf = [b'A'; P10_CHUNK];
    let chunk = &buf[..chunk.min(P10_CHUNK)];
    let mut bytes = 0u64;
    let mut writes = 0u64;
    let mut ceiling_hit = false;
    let terminal = loop {
        if already + bytes >= ceiling {
            ceiling_hit = true;
            break "ceiling".to_owned();
        }
        match sys::write_fd(write_to, chunk) {
            // A short write is fine and expected at the boundary; a zero-length
            // one would loop forever, so it ends the fill and is named.
            Ok(0) => break "wrote_zero".to_owned(),
            Ok(n) => {
                bytes += n as u64;
                writes += 1;
            }
            Err(e) => break read_class(&e),
        }
    };
    (bytes, writes, terminal, ceiling_hit)
}

// ---------------------------------------------------------------------------
// P4 — device identity resolution ground truth (§12)
// ---------------------------------------------------------------------------

// The `<sys>/class/tty` listing + sysfs walk that produces
// `usb:vid:pid:serial:iface`, with `/dev/serial/by-id` as a fast path over it,
// lives in `serial_nexus_core::resolver` (the daemon and the doctor share one
// implementation, §12); the doctor observes what that resolver reports — and
// therefore has to ask it about *devices*, not about the by-id directory, or it
// contradicts the daemon in the one environment §12 grew a fallback for (RES-2).

pub fn p4_resolver(dev_root: &Path, sys_root: &Path) -> Probe {
    let p = Probe::new(
        "P4",
        "device identity resolution",
        "Does the resolver's one source — the <sys>/class/tty listing plus a dependency-free sysfs walk, with /dev/serial/by-id as a fast path over it — yield the canonical usb:vid:pid:serial:iface identity (§12)?",
    );
    // **Gate on devices, not on the by-id directory.** Gating on `by_id.is_dir()`
    // made this probe skip with "no USB-serial adapter present" in exactly the
    // environment §12 now handles — `/sys` mounted, no udev `60-serial.rules`, the
    // adapter sitting at `/dev/ttyUSB0` — so the report the README tells operators to
    // attach to every bug report contradicted the daemon that was working beside it
    // (review 32 RES-2). `enumerate_ports` is the resolver's own enumeration face and
    // reads the same source capture does, so what P4 reports is what `add-node`
    // would store; it is passive by construction (readlinks, listings, sysfs reads —
    // never `open(2)`, §15.35), which is what lets a *diagnostic* run it unattended.
    let resolver = serial_nexus_core::Resolver::with_roots(dev_root, sys_root);
    let by_id_present = dev_root.join("dev/serial/by-id").is_dir();
    let adapters = resolver.discover_adapters();
    let candidates = resolver.enumerate_ports();
    // Devices sysfs can identify that udev never named — the RES-2 population.
    let unnamed: Vec<_> = candidates
        .iter()
        .filter(|c| c.by_id.is_none() && c.kind == serial_nexus_core::DeviceKind::Usb)
        .collect();
    // Everything else with no by-id entry: a by-path-only adapter, or a BSD `cu.*`
    // node. Counted, never judged — neither is a failure of identity resolution.
    let other = candidates.iter().filter(|c| c.by_id.is_none()).count() - unnamed.len();

    let p = p
        .observe(
            "by_id_tree",
            if by_id_present { "present" } else { "absent" },
        )
        .observe("count", adapters.len() as u64)
        .observe("sysfs_only", unnamed.len() as u64)
        .observe("other_candidates", other as u64);

    if adapters.is_empty() && candidates.is_empty() {
        return p.verdict(
            Status::skipped("no serial device visible"),
            "No serial device visible through /dev/serial/by-id, the sysfs tty listing, /dev/serial/by-path or cu.*; identity resolution untested here (run on an adapter-equipped box).",
        );
    }

    let mut p = p;
    let mut all_resolved = true;
    for a in &adapters {
        let val = a.identity.clone().unwrap_or_else(|| "by-path only".into());
        p = p.observe(&a.by_id_name, val);
        if a.identity.is_none() {
            all_resolved = false;
        }
    }
    for c in &unnamed {
        p = p.observe(&c.path.display().to_string(), c.identity.clone());
    }

    // The by-id tree's *absence* is reported on the environment check as `degraded`
    // with the observation named (§13), not here: this probe answers "does identity
    // resolution work", and in that environment it does — through the sysfs listing,
    // which is the same source capture reads. Naming it in the consequence keeps the
    // operator informed without reddening a box the daemon is fine on; a `degraded`
    // verdict here would also fail `expectations/linux.jq`, which admits only
    // `supported` or `skipped` for P4.
    let where_from = if by_id_present {
        ""
    } else {
        " No /dev/serial/by-id tree here (no udev 60-serial.rules — a container's bare --device=…, a busybox-mdev image): identities came from the <sys>/class/tty listing, the same source capture reads (§12)."
    };
    if all_resolved {
        p.verdict(
            Status::Supported,
            &format!(
                "Resolver produces canonical identities; configs survive replug and cold start (§12).{where_from}"
            ),
        )
    } else {
        p.verdict(
            Status::Degraded,
            &format!(
                "Some adapters resolve only by topology (no serial number) → by-path fallback with a documented instability warning (§12).{where_from}"
            ),
        )
    }
}

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}

// ---------------------------------------------------------------------------
// P5 — rig discovery and certification (§13, §15.21). Opt-in like every
// TX-emitting probe: it transmits a nonce, so it runs only on explicitly named
// --ports (a listed port could be wired to live equipment). Discovery classifies
// each named port (dangling / loopback / paired, both directions, so a
// half-crossed pair is named); characterization certifies real UARTs and skips
// with a reason otherwise — `not a UART` for the sim pts used in CI on Linux, and
// a different sentence off Linux, where the UART predicate cannot answer at all
// (see `P5_UNCHARACTERIZED`). The doctor certifies the rig and stops — it never
// drives the daemon through it.
// ---------------------------------------------------------------------------

/// A unique, distinctive nonce for the port at index `i` — the index makes it
/// unique across ports without any RNG (the doctor is deterministic).
fn p5_nonce(i: usize) -> Vec<u8> {
    format!("\x02SNX-P5-RIG-{i:03}\x03").into_bytes()
}

fn contains_sub(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= hay.len()
        && hay.windows(needle.len()).any(|w| w == needle)
}

/// Open a port for P5 (raw, 8N1 at a standard baud) with short read/write timeouts
/// so the continuous discovery scan neither blocks on a stalled/dangling port nor
/// misses a reply.
fn p5_open(port: &Path, baud: u32, parity: Parity) -> std::io::Result<SerialPort> {
    let mut sp = SerialPort::open(port, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(baud)?;
        s.set_char_size(CharSize::Bits8);
        s.set_stop_bits(StopBits::One);
        s.set_parity(parity);
        s.set_flow_control(FlowControl::None);
        Ok(s)
    })?;
    sp.set_read_timeout(Duration::from_millis(20))?;
    // A write timeout keeps a dangling/stalled port (buffer never drained) from
    // blocking the whole exchange — it times out and is classified dangling.
    sp.set_write_timeout(Duration::from_millis(200))?;
    Ok(sp)
}

/// Best-effort write of the whole nonce. A timeout/would-block (a stalled port)
/// stops rather than blocking the exchange.
fn p5_write_all(sp: &SerialPort, mut data: &[u8]) -> std::io::Result<()> {
    while !data.is_empty() {
        match sp.write(data) {
            Ok(0) => break,
            Ok(n) => data = &data[n..],
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                break;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Whether an errno means *the peer is gone*, as opposed to merely silent.
///
/// A pts whose master closed, and a USB adapter yanked mid-probe, both answer
/// `EIO` to every read and write; `ENXIO`/`ENODEV` are the same fact from a driver
/// that has already torn the device down. None of them is a timeout, and that
/// distinction is the whole point: a silent port is *dangling*, a gone port is
/// *hung up*, and the two carry opposite instructions for the operator.
fn is_hangup(e: &std::io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EIO) | Some(libc::ENXIO) | Some(libc::ENODEV)
    )
}

/// One read (up to the port's short read timeout): the bytes available now,
/// `Ok(empty)` on a timeout/would-block, or the error for a real failure.
///
/// The error used to be swallowed here (`Err(_) => Vec::new()`), which is how a
/// hung-up peer read as a quiet one all the way up to the classifier.
fn p5_read_result(sp: &SerialPort) -> std::io::Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    match sp.read(&mut buf) {
        Ok(n) => Ok(buf[..n].to_vec()),
        Err(e)
            if e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::WouldBlock =>
        {
            Ok(Vec::new())
        }
        Err(e) => Err(e),
    }
}

/// One read, with any failure flattened to "nothing readable" — for the
/// characterization paths, which judge on the bytes they got.
fn p5_read_once(sp: &SerialPort) -> Vec<u8> {
    p5_read_result(sp).unwrap_or_default()
}

/// What a port did during P5 discovery, beyond the bytes it carried.
///
/// Discovery answers "who heard whom"; this answers the question that used to be
/// invisible beside it — whether the port was *able* to speak at all. A port that
/// heard nothing is only dangling if it was otherwise healthy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PortHealth {
    /// Nonce writes attempted, and how many died of a hangup errno.
    writes: u32,
    write_hangups: u32,
    /// Reads that failed with a hangup errno.
    read_hangups: u32,
    /// Poll passes that reported anything, and how many of those were a bare
    /// `POLLHUP`/`POLLERR`/`POLLNVAL` (level-triggered, so a hung-up port sets it
    /// on every pass for the rest of the window).
    poll_wakes: u32,
    hup_wakes: u32,
    /// Bytes actually read off the port.
    bytes_read: u64,
}

impl PortHealth {
    /// Whether the peer hung up rather than never existing.
    ///
    /// Three independent proofs, any one of which is enough — a hot-unplug that
    /// lands between two poll passes shows up in the writes, one that lands
    /// mid-read shows up in the read errno, and one that happened before P5 even
    /// started shows up as a `POLLHUP` on every pass. All of them require the port
    /// to have carried **no** bytes: a port that spoke and then vanished was
    /// already classified by what it said.
    fn hung_up(&self) -> bool {
        self.bytes_read == 0
            && ((self.writes > 0 && self.write_hangups == self.writes)
                || self.read_hangups > 0
                || (self.poll_wakes > 0 && self.hup_wakes == self.poll_wakes))
    }
}

/// Drain everything readable from `sp` within `window` (raw non-blocking reads,
/// sleeping briefly when idle so it does not busy-spin).
fn p5_drain(sp: &SerialPort, window: Duration) -> Vec<u8> {
    let deadline = Instant::now() + window;
    let mut out = Vec::new();
    while Instant::now() < deadline {
        let got = p5_read_once(sp);
        if got.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        } else {
            out.extend_from_slice(&got);
        }
    }
    out
}

/// A rig-certificate port name: the resolver identity where the port resolves
/// (so the certificate survives renumbering, §15.21), else the raw path.
fn p5_name(port: &Path, resolver: &serial_nexus_core::Resolver) -> String {
    for a in resolver.discover_adapters() {
        if a.dev_path == port
            && let Some(id) = a.identity
        {
            return id;
        }
    }
    port.display().to_string()
}

/// Whether this fd is a real UART — measured with an ioctl **every platform this
/// project supports implements**, not with one only Linux does.
///
/// The property the certificate needs is "characterizing this port means
/// something": a driver with a line, not the pts the CI doubles stand up (§15.21).
/// The predicate used to be `TIOCGICOUNT` alone, which is that property *on Linux*
/// and nothing at all anywhere else — `serial_nexus_sys`'s non-Linux arm is a hard
/// `ENOTSUP` stub — so on Darwin it answered "not a UART" for two genuine FT232R
/// adapters and the entire certificate, rate ladder included, never ran
/// (`docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3.json`: both ports
/// `cert: skipped`). That is §9's proxy in space exactly: a Linux-only observable
/// standing in for a portable property, passing on the box it was written on.
///
/// `TIOCMGET` is the portable member, and **it is the only item in P3's whole
/// vector that discriminates.** Measured 2026-08-05 on Linux 7.0.0-29 with an
/// FT232R crossover attached: a pts accepts a custom baud *and reports 250000
/// back*, accepts `TIOCEXCL`, and accepts `TIOCSBRK`/`TIOCCBRK` — so custom baud,
/// exclusivity and break cannot be the predicate however UART-ish they read. It
/// refuses `TIOCMGET` with `ENOTTY`, and both FT232R ports answer it. Off Linux the
/// other half is already committed: P3 reports `modem_calls_ok: true` for both
/// FT232R ports in all three 2026-08-05 macOS captures.
///
/// The Linux-only member stays in the **disjunction** rather than being replaced.
/// That is what makes this non-regressive by construction: every port that
/// certifies today answers `TIOCGICOUNT`, so it still certifies and no committed
/// Linux artifact can move. A widening cannot lose a port; a replacement could, and
/// §7 forbids that one-way decision on single-kernel evidence.
///
/// Read-only, deliberately: `TIOCMGET` asserts nothing on the line, where the
/// `TIOCMSET` P3 also performs would drive an output, and this predicate runs over
/// every port the operator named, which may be live equipment. The **values** are
/// worthless here and are not consulted — both FT232R ports read
/// `cts=false dsr=false dcd=false ri=false` on both kernels — only whether the
/// driver answers at all.
fn p5_is_uart(sp: &SerialPort) -> bool {
    let fd = sp.as_raw_fd();
    sys::read_modem_bits(fd).is_ok() || sys::read_icounts(fd).is_ok()
}

/// Why [`p5_certify_port`] was not run for a port [`p5_is_uart`] rejected — and it
/// is **not** the same sentence on both platforms, because the predicate does not
/// mean the same thing on both.
///
/// The predicate is `TIOCGICOUNT`, which is Linux-only (`serial-nexus-sys`'s non-Linux arm
/// is a hard `ENOTSUP` stub). On Linux a port that answers it is a real driver and
/// one that does not is the CI pts sim, so "not a UART" is exactly right. Off Linux
/// it answers `false` for **every** port — a genuine FTDI adapter included — so the
/// same words become a false statement about the operator's hardware. Measured
/// 2026-07-28 on macOS 15.7.8 against two real FTDI adapters cross-wired as a null
/// modem: P5 discovered the pair correctly in both directions and then reported both
/// ports `skipped (not a UART)`.
///
/// A capability report may only claim what it measured (§15.17), and this is the
/// third time that rule has had to be applied to P5's own prose — the other two are
/// in AGENTS §2's 6.18 entry. What was measured here is "this kernel gives me no way
/// to tell", so that is what it says.
#[cfg(target_os = "linux")]
const P5_UNCHARACTERIZED: &str = "not a UART";
#[cfg(not(target_os = "linux"))]
const P5_UNCHARACTERIZED: &str = "not characterizable here (TIOCGICOUNT is Linux-only)";

/// One failed certificate item, named so the verdict can cite it (§15.21).
///
/// `integrity` separates the two consequences the certificate can carry: a *data*
/// failure — the rig did not deliver the bytes it was handed — makes the
/// certificate unusable as the precondition every tiered run starts from, so it
/// is a stop condition (`Unsupported`); an uncharacterized item (break, custom
/// baud, observable error counters) leaves a rig that works but is not fully
/// characterized (`Degraded`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct CertFailure {
    item: String,
    integrity: bool,
}

impl CertFailure {
    /// Prefix the item with the port (or pair) it was measured on, so the verdict
    /// line names *which* rig element failed — the whole point of naming the
    /// asymmetry in the discovery half (§15.21).
    fn qualified(self, subject: &str) -> CertFailure {
        CertFailure {
            item: format!("{subject}: {}", self.item),
            integrity: self.integrity,
        }
    }
}

/// The outcome of certifying one port or one pair: the report line (the
/// observation value, unchanged in shape) plus the structured failures the
/// verdict folds over.
///
/// Before this split the certification functions returned only the line, so every
/// certificate failure was report *text* the verdict never saw: a rate-ladder
/// mismatch, an unobserved deliberate mismatch, or a dead break all left P5
/// `supported` and the process exit code 0 (review 26, DOC-1b). §15.21 makes the
/// certificate the precondition every tiered checklist run starts from — and a
/// precondition that cannot fail is not one.
#[derive(Debug, Clone)]
struct Certificate {
    line: String,
    failures: Vec<CertFailure>,
    /// Whether the deliberate baud-mismatch traffic was actually **transmitted**
    /// on these ports — not whether it was observed to corrupt anything.
    ///
    /// P11 reads this (through [`RigFacts::mismatch_pairs`]) to decide whether it
    /// may offer the mismatch as the explanation for a nonzero `frame` count, and
    /// "the certificate ran" is not the same claim: [`p5_certify_pair`] returns
    /// early *before* the mismatch block whenever a port will not reopen, so
    /// counting certificate attempts would let P11 blame an item that never put a
    /// byte on the wire — the exact defect the `RigFacts` thread exists to fix.
    mismatch_transmitted: bool,
}

impl Certificate {
    /// A certificate line with no failures yet.
    fn new(line: impl Into<String>) -> Self {
        Certificate {
            line: line.into(),
            failures: Vec::new(),
            mismatch_transmitted: false,
        }
    }

    /// A certificate that could not be produced at all (the port would not
    /// reopen). The rig may be fine; it is simply uncharacterized → degrade.
    fn unavailable(line: impl Into<String>, item: &str) -> Self {
        Certificate {
            mismatch_transmitted: false,
            line: line.into(),
            failures: vec![CertFailure {
                item: item.to_owned(),
                integrity: false,
            }],
        }
    }

    /// A characterization deliberately not attempted — the non-UART (CI sim) arm.
    /// Records **no** failure: §15.21 specifies characterization reporting skipped
    /// on non-UARTs precisely so P5's logic never waits for a bench.
    fn skipped(reason: &str) -> Self {
        Certificate::new(format!("skipped ({reason})"))
    }

    /// Record `item` as failed when `failed`, with its consequence class.
    fn fail_if(&mut self, failed: bool, item: &str, integrity: bool) {
        if failed {
            self.failures.push(CertFailure {
                item: item.to_owned(),
                integrity,
            });
        }
    }
}

/// A single-port certificate for a real UART: break capability, the modem-line
/// map (input levels), custom-baud acceptance, and counter support. The modem map
/// is reported, never judged — "not wired" is a valid answer (§15.21).
fn p5_certify_port(port: &Path) -> Certificate {
    let Ok(sp) = p5_open(port, CUSTOM_BAUD, Parity::None) else {
        return Certificate::unavailable("unavailable for characterization", "reopen");
    };
    let baud = sp.get_configuration().and_then(|c| c.get_baud_rate()).ok();
    let custom_baud_ok = baud
        .map(|b| {
            (b as i64 - CUSTOM_BAUD as i64).unsigned_abs() as f64 / CUSTOM_BAUD as f64 <= 0.025
        })
        .unwrap_or(false);
    let break_ok = sp.set_break(true).is_ok() && sp.set_break(false).is_ok();
    let modem = format!(
        "cts={} dsr={} dcd={} ri={}",
        sp.read_cts().map(|b| b.to_string()).unwrap_or("?".into()),
        sp.read_dsr().map(|b| b.to_string()).unwrap_or("?".into()),
        sp.read_cd().map(|b| b.to_string()).unwrap_or("?".into()),
        sp.read_ri().map(|b| b.to_string()).unwrap_or("?".into()),
    );
    let icounter = sys::read_icounts(sp.as_raw_fd()).is_ok();
    let mut cert = Certificate::new(format!(
        "custom_baud={custom_baud_ok} break={break_ok} modem[{modem}] icounter={icounter}"
    ));
    // None of these is a data-integrity failure: the port carries bytes, but a
    // checklist tier that leans on a nonstandard rate, on break reception, or on
    // the driver counters would be running uncertified (§15.21) → degrade, named.
    cert.fail_if(!custom_baud_ok, "custom_baud", false);
    cert.fail_if(!break_ok, "break", false);
    cert.fail_if(!icounter, "icounter", false);
    cert
}

/// After (re)opening a port at a new baud, wait for the adapter to apply the new
/// line rate before the first byte. Real-hardware finding (first live P5 pair-cert
/// run, §15.21 "recalibrate the doctor against real adapters"): an FTDI transmits
/// or samples the very first bytes after `open`+`set_baud_rate` at a transitional
/// rate, so a single-shot exchange with no settle sees GARBLED bytes at 115200 and
/// above (9600 is forgiving). Discovery is immune only by accident — it opens once
/// and re-sends every 500 ms, so later sends land after the line settles; the
/// single-shot certificate has no such cushion and must settle explicitly. The
/// doctor is a diagnostic, not a data path, so the milliseconds cost nothing.
const P5_OPEN_SETTLE: Duration = Duration::from_millis(150);

/// The paired-rig certificate (§15.21), only meaningful on independently clocked
/// UARTs: a rate ladder including a nonstandard rate (all must round-trip), and a
/// deliberate baud mismatch that must corrupt the nonce and raise the frame-error
/// counter — proving the error counters are observable. Returns the summary line
/// plus the failures the verdict folds over (DOC-1b).
fn p5_certify_pair(port_a: &Path, port_b: &Path) -> Certificate {
    // Rate ladder: reconfigure both ports to each rate and exchange a nonce.
    let rates = [9600u32, 115_200, CUSTOM_BAUD];
    let mut ladder_ok = true;
    for &baud in &rates {
        let (Ok(a), Ok(b)) = (
            p5_open(port_a, baud, Parity::None),
            p5_open(port_b, baud, Parity::None),
        ) else {
            return Certificate::unavailable("pair reopen failed", "pair_reopen");
        };
        std::thread::sleep(P5_OPEN_SETTLE); // let both adapters apply the new baud
        // §15.21 "all must round-trip": certify BOTH directions at each rate, not
        // just a→b. A one-way ladder leaves 9600/nonstandard uncertified b→a (and
        // discovery runs only at 115200), so a half-working reverse path would pass.
        for (tx, rx, dir) in [(&a, &b, "AB"), (&b, &a, "BA")] {
            let nonce = format!("\x02LADDER-{baud}-{dir}\x03").into_bytes();
            let _ = p5_write_all(tx, &nonce);
            std::thread::sleep(Duration::from_millis(120));
            let got = p5_drain(rx, Duration::from_millis(300));
            if !contains_sub(&got, &nonce) {
                ladder_ok = false;
            }
        }
    }
    // Deliberate baud mismatch: TX at 115200, RX at 9600 — the nonce must NOT
    // arrive intact, and the frame-error counter must rise (observable, §15.21).
    let mismatch_observed = {
        let (Ok(a), Ok(b)) = (
            p5_open(port_a, 115_200, Parity::None),
            p5_open(port_b, 9600, Parity::None),
        ) else {
            let mut cert = Certificate::unavailable(
                format!("rate_ladder={ladder_ok} mismatch=reopen-failed"),
                "mismatch_reopen",
            );
            cert.fail_if(!ladder_ok, "rate_ladder", true);
            return cert;
        };
        std::thread::sleep(P5_OPEN_SETTLE); // settle both ends before the mismatch probe
        let before = sys::read_icounts(b.as_raw_fd())
            .map(|c| c.frame)
            .unwrap_or(0);
        // A single ~24-byte nonce raises the frame counter only probabilistically
        // (few mismatched frames land in the window), which made this observation
        // flaky on real hardware. Send the pattern repeated to ~768 bytes so many
        // mismatched frames reach the 9600 receiver and the counter reliably rises.
        let unit = b"\x02MISMATCH-PROBE-PATTERN\x03";
        let bulk: Vec<u8> = unit.iter().cycle().take(unit.len() * 32).copied().collect();
        let _ = p5_write_all(&a, &bulk);
        std::thread::sleep(Duration::from_millis(150));
        let got = p5_drain(&b, Duration::from_millis(300));
        let after = sys::read_icounts(b.as_raw_fd())
            .map(|c| c.frame)
            .unwrap_or(before);
        !contains_sub(&got, unit) && after > before
    };
    let mut cert = Certificate::new(format!(
        "rate_ladder={ladder_ok} deliberate_mismatch_observed={mismatch_observed}"
    ));
    // Reaching here means the bulk mismatch pattern was written to the wire —
    // which is the claim P11 needs, independent of whether it was *observed* to
    // corrupt anything. The two early returns above never get here, and that is
    // the distinction: a certificate that bailed on a reopen transmitted nothing.
    cert.mismatch_transmitted = true;
    // The ladder is the integrity item: a rung that did not round-trip means the
    // rig itself corrupts or loses data, so no tier failure measured through it is
    // attributable to serial_nexus (§15.21) — a stop condition, not a footnote.
    cert.fail_if(!ladder_ok, "rate_ladder", true);
    // An unobserved deliberate mismatch means the error counters are not
    // observable on this rig: the data path is fine, the characterization is not.
    cert.fail_if(!mismatch_observed, "deliberate_mismatch", false);
    cert
}

/// What P5 established about the rig, carried to the probes that read the same
/// ports afterwards.
///
/// It exists because two report sentences were explaining the operator's numbers
/// with mechanisms that had not run. The rate ladder and the **deliberate baud
/// mismatch** — the item that corrupts a nonce on purpose to prove the driver's
/// error counters observable (§15.21) — transmit *only* inside
/// [`p5_certify_pair`], which runs only over pairs P5 verified in **both**
/// directions. `named >= 2` is necessary but not sufficient: two *dangling* ports
/// are two named ports and no pair. So the counts are taken where the
/// certification happens rather than inferred from the command line, and a
/// probe that wants to say "that is the mismatch item" has to ask.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RigFacts {
    /// Pairs **discovery** verified in both directions — what is physically
    /// wired, which is what a *tier* is. Separate from `mismatch_pairs` because
    /// certification can fail on a rig that is genuinely cross-wired, and
    /// collapsing the two let P5 print "Tier 1 — a dangling converter" directly
    /// above its own observation line reading `paired with …`.
    pub discovered_pairs: usize,
    /// Pairs whose deliberate baud-mismatch traffic was actually **transmitted**
    /// ([`Certificate::mismatch_transmitted`]). This, not the number of
    /// certificate *attempts*, is what P11 may cite as the cause of a nonzero
    /// `frame` count: [`p5_certify_pair`] returns early before the mismatch block
    /// when a port will not reopen, so an attempt count would reintroduce exactly
    /// the defect this thread was added to fix.
    pub mismatch_pairs: usize,
    /// Ports discovered with TX↔RX jumpered.
    pub loopbacks: usize,
}

impl RigFacts {
    /// §13's hardware tier, from what discovery found — **the topology, not the
    /// certificate**: **3** a cross-wired pair (independent clocks), **2** a
    /// TX↔RX jumper (a real driver data path on one clock), **1** a dangling
    /// converter. Tier 1 is this project's *baseline* rig, not an exotic case,
    /// and it certifies strictly less than the bare word "certified" suggests —
    /// which is why [`p5_verdict`] names the tier instead of implying the whole
    /// certificate, and why it reports separately whether that tier's items ran.
    pub fn tier(self) -> u8 {
        if self.discovered_pairs > 0 {
            3
        } else if self.loopbacks > 0 {
            2
        } else {
            1
        }
    }
}

pub fn p5_rig(ports: &[PathBuf], resolver: &serial_nexus_core::Resolver) -> (Probe, RigFacts) {
    let mut p = Probe::new("P5", "rig discovery and certification", P5_QUESTION);

    // Open every port for discovery.
    let mut sps: Vec<Option<SerialPort>> = Vec::new();
    let mut perm_denied = false;
    for port in ports {
        match p5_open(port, 115_200, Parity::None) {
            Ok(sp) => sps.push(Some(sp)),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                perm_denied = true;
                sps.push(None);
            }
            Err(e) => {
                p = p.observe(&port.display().to_string(), format!("open error: {e}"));
                sps.push(None);
            }
        }
    }
    if sps.iter().all(Option::is_none) {
        let reason = if perm_denied {
            "permission denied"
        } else {
            "no port opened"
        };
        return (
            p.verdict(
                Status::skipped(reason),
                "Grant access (udev GROUP=plugdev, or dialout) and re-run with the rig's --ports.",
            ),
            RigFacts::default(),
        );
    }

    // Discovery: transmit each port's nonce and CONTINUOUSLY scan every port for a
    // few seconds, re-sending the nonce periodically. A gapped write-then-drain
    // races a software echo/bridge peer that is CPU-starved on a loaded box (it may
    // echo only after the drain window closes); a continuous scan instead catches
    // the echo whenever it lands, and the re-sends give a slow peer repeated
    // triggers — while a truly dangling port hears nothing across the whole window.
    // The doctor is a diagnostic, not a data path, so the seconds cost nothing.
    let mut bufs: Vec<Vec<u8>> = vec![Vec::new(); ports.len()];
    // Beside the bytes, record whether each port was *able* to carry them: a write
    // that dies EIO and a poll stuck at POLLHUP are the signature of a peer that
    // hung up, and folding that into "heard nothing" told the operator to wire up a
    // port that was wired (§15.21).
    let mut health: Vec<PortHealth> = vec![PortHealth::default(); ports.len()];
    let deadline = Instant::now() + Duration::from_millis(4000);
    let mut next_send = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_send {
            for (i, sp) in sps.iter().enumerate() {
                if let Some(sp) = sp {
                    health[i].writes += 1;
                    if let Err(e) = p5_write_all(sp, &p5_nonce(i))
                        && is_hangup(&e)
                    {
                        health[i].write_hangups += 1;
                    }
                }
            }
            next_send = Instant::now() + Duration::from_millis(500);
        }
        // Block on poll for readability across all live ports (like the daemon's
        // reader and the nullmodem bridge) — a short-timeout read scan races a
        // CPU-starved echo peer and misses the reply; poll wakes the instant any
        // port has data. Read every ready port.
        let live: Vec<(usize, &SerialPort)> = sps
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|sp| (i, sp)))
            .collect();
        let mut pfds: Vec<PollFd> = live
            .iter()
            .map(|(_, sp)| PollFd::new(sp.as_fd(), PollFlags::POLLIN))
            .collect();
        let _ = poll(&mut pfds, PollTimeout::from(200u16));
        for (idx, (i, sp)) in live.iter().enumerate() {
            let revents = pfds[idx].revents().unwrap_or(PollFlags::empty());
            // A hangup is still read (buffered bytes survive the peer's close), but
            // it is also *counted*: on a hung-up port POLLHUP is level-triggered, so
            // it repeats on every pass for the rest of the window.
            let gone = PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL;
            if !revents.is_empty() {
                health[*i].poll_wakes += 1;
                if revents.intersects(gone) {
                    health[*i].hup_wakes += 1;
                }
            }
            if revents.intersects(PollFlags::POLLIN | gone) {
                match p5_read_result(sp) {
                    Ok(got) => {
                        health[*i].bytes_read += got.len() as u64;
                        bufs[*i].extend_from_slice(&got);
                    }
                    Err(e) if is_hangup(&e) => health[*i].read_hangups += 1,
                    Err(_) => {}
                }
            }
        }
        // Yield the CPU each pass. A port stuck poll-ready (e.g. a `POLLHUP` on a
        // stalled/half-open peer) would otherwise busy-spin this loop and starve a
        // software echo/bridge peer of the CPU it needs to reply (the bug this
        // guards against — without it the loopback reply is never captured).
        std::thread::sleep(Duration::from_millis(5));
    }
    let heard = |listener: usize, sender: usize| contains_sub(&bufs[listener], &p5_nonce(sender));

    // Classify, and remember verified UART pairs (i<j) for characterization. The
    // index loops are the who-heard-whom matrix — `heard(i, j)` needs both indices,
    // so an iterator loop does not fit.
    let mut clean = true;
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut hung_up: Vec<String> = Vec::new();
    let mut loopbacks = 0usize;
    #[allow(clippy::needless_range_loop)]
    for i in 0..ports.len() {
        if sps[i].is_none() {
            continue;
        }
        let name = p5_name(&ports[i], resolver);
        let classification = if heard(i, i) {
            loopbacks += 1;
            "loopback (TX↔RX jumpered)".to_string()
        } else {
            let mut partner = None;
            let mut asym = None;
            for j in 0..ports.len() {
                if j == i || sps[j].is_none() {
                    continue;
                }
                match (heard(i, j), heard(j, i)) {
                    (true, true) => partner = Some(j),
                    (a, b) if a != b => asym = Some(j),
                    _ => {}
                }
            }
            if let Some(j) = partner {
                if i < j {
                    pairs.push((i, j));
                }
                format!("paired with {}", p5_name(&ports[j], resolver))
            } else if let Some(j) = asym {
                clean = false;
                format!(
                    "HALF-CROSSED with {} (asymmetric — check TX/RX wiring)",
                    p5_name(&ports[j], resolver)
                )
            } else if health[i].hung_up() {
                // Not "nothing wired to it": something *was* wired to it and went
                // away while P5 was talking to it (a pts whose master closed, an
                // adapter unplugged mid-probe). Saying `dangling` here hands the
                // operator the wrong instruction — wire up a port that is wired —
                // and P5 cannot classify a port whose peer is gone, so it says so.
                hung_up.push(name.clone());
                "hung up (peer closed) — not classifiable".to_string()
            } else {
                "dangling (nothing wired to it)".to_string()
            }
        };
        p = p.observe(&name, classification);
    }

    // Release the discovery opens before characterization reopens the ports.
    drop(sps);

    // Characterize each port; a non-UART (the CI sim) skips cleanly. Every
    // certificate's failures are collected here — they decide the verdict below,
    // rather than being report text nobody folds in (DOC-1b).
    let mut any_uart = false;
    let mut failures: Vec<CertFailure> = Vec::new();
    for port in ports {
        if let Ok(sp) = p5_open(port, 115_200, Parity::None) {
            let name = p5_name(port, resolver);
            let cert = if p5_is_uart(&sp) {
                any_uart = true;
                drop(sp);
                p5_certify_port(port)
            } else {
                Certificate::skipped(P5_UNCHARACTERIZED)
            };
            p = p.observe(format!("{name} cert").as_str(), cert.line);
            failures.extend(cert.failures.into_iter().map(|f| f.qualified(&name)));
        }
    }
    // Paired UARTs get the independent-clock certificate (rate ladder + mismatch).
    //
    // Two counts, because they are two facts. `discovered_pairs` is the topology
    // and drives the tier; `mismatch_pairs` counts only the pairs whose mismatch
    // traffic actually reached the wire, which is the narrower claim P11 is
    // allowed to make. Counting certificate *attempts* would put back the defect
    // this thread exists to remove — `p5_certify_pair` bails before the mismatch
    // block whenever a port will not reopen.
    let discovered_pairs = pairs.len();
    let mut mismatch_pairs = 0usize;
    for (i, j) in pairs {
        let (Ok(a_uart), Ok(b_uart)) = (
            p5_open(&ports[i], 115_200, Parity::None).map(|sp| p5_is_uart(&sp)),
            p5_open(&ports[j], 115_200, Parity::None).map(|sp| p5_is_uart(&sp)),
        ) else {
            // A pair discovery verified in both directions, whose ports then
            // would not reopen, is *uncharacterized* — §15.21's degrade case. It
            // used to `continue` in silence, leaving no observation, no failure
            // and no fact, so the verdict read `supported` over a pair nothing
            // had certified. Not an integrity failure: the rig carried data
            // during discovery, it simply could not be characterized.
            let subject = format!(
                "{} ↔ {}",
                p5_name(&ports[i], resolver),
                p5_name(&ports[j], resolver)
            );
            p = p.observe(
                format!("{subject} cert").as_str(),
                "unavailable (pair would not reopen for characterization)",
            );
            failures.push(
                CertFailure {
                    item: "pair_reopen".to_owned(),
                    integrity: false,
                }
                .qualified(&subject),
            );
            continue;
        };
        if a_uart && b_uart {
            let subject = format!(
                "{} ↔ {}",
                p5_name(&ports[i], resolver),
                p5_name(&ports[j], resolver)
            );
            let cert = p5_certify_pair(&ports[i], &ports[j]);
            if cert.mismatch_transmitted {
                mismatch_pairs += 1;
            }
            p = p.observe(format!("{subject} cert").as_str(), cert.line);
            failures.extend(cert.failures.into_iter().map(|f| f.qualified(&subject)));
        }
    }

    let facts = RigFacts {
        discovered_pairs,
        mismatch_pairs,
        loopbacks,
    };
    let (status, consequence) = p5_verdict(clean, any_uart, &failures, &hung_up, facts);
    (p.verdict(status, &consequence), facts)
}

/// Fold discovery and every certificate into P5's one verdict (§15.21, DOC-1b).
///
/// Pure, so the fold is unit-testable without a bench — the rest of P5 needs
/// hardware, which is exactly how the old two-input verdict (`clean` and
/// `any_uart`, both from *discovery*) went unnoticed while the certification
/// results were computed and then discarded into report text.
///
/// Precedence, worst first: a data-integrity failure is a stop condition
/// (`Unsupported`); miswiring and uncharacterized items are `Degraded`; anything
/// else is `Supported`, with the two pre-existing consequence lines preserved
/// verbatim so the certified and the skipped-on-a-sim paths read as they always
/// have.
fn p5_verdict(
    clean: bool,
    any_uart: bool,
    failures: &[CertFailure],
    hung_up: &[String],
    facts: RigFacts,
) -> (Status, String) {
    let named = |integrity: bool| -> Vec<&str> {
        failures
            .iter()
            .filter(|f| f.integrity == integrity)
            .map(|f| f.item.as_str())
            .collect()
    };
    let integrity = named(true);
    let uncertified = named(false);

    if !integrity.is_empty() {
        return (
            Status::Unsupported,
            format!(
                "The rig FAILED data integrity ({}) — it did not deliver the bytes it was handed, so a tiered run started from this certificate would misattribute the rig's loss to serial_nexus (§15.21). Re-wire or replace the adapters and re-run P5 before any tier.",
                integrity.join(", ")
            ),
        );
    }
    if !clean {
        let also = if uncertified.is_empty() {
            String::new()
        } else {
            format!(" Also uncertified: {}.", uncertified.join(", "))
        };
        return (
            Status::Degraded,
            format!(
                "A rig is miswired (asymmetric/half-crossed) — named above; fix it before a tiered run so a tier failure is attributable to serial_nexus, not a loose wire (§15.21).{also}"
            ),
        );
    }
    if !hung_up.is_empty() {
        // A port whose peer went away mid-probe is a rig fault, not a clean rig: P5
        // could not classify it at all, so a tiered run started from this
        // certificate would be leaning on a port nobody has established the wiring
        // of. Folding it in here is the same rule §15.21 applies to miswiring and to
        // an uncharacterized item — an observation the operator must act on cannot
        // leave the verdict `supported` (DOC-1b). It stays `degraded`, never
        // `unsupported`: the rig may well be fine once the peer is back, and
        // `unsupported` is reserved for a rig that demonstrably ate bytes.
        let also = if uncertified.is_empty() {
            String::new()
        } else {
            format!(" Also uncertified: {}.", uncertified.join(", "))
        };
        return (
            Status::Degraded,
            format!(
                "A port's peer hung up while P5 was probing it ({}) — it could not be classified, so the rig is not certified. Re-seat or re-open the peer and re-run P5 before any tier (§15.21).{also}",
                hung_up.join(", ")
            ),
        );
    }
    if !uncertified.is_empty() {
        return (
            Status::Degraded,
            format!(
                "The rig carries data, but is not fully characterized ({}) — a tier leaning on that item would be running uncertified (§15.21). Everything else above is certified.",
                uncertified.join(", ")
            ),
        );
    }
    if any_uart {
        // **Name the tier.** The bare sentence this replaced — "Rig discovered and
        // certified; every tiered checklist run starts from this certificate" —
        // was emitted for any UART rig, including a Tier-1 dangling converter
        // where `pairs` was empty, so `p5_certify_pair` never ran and neither
        // `integrity` failure site could fire. §15.21 makes this certificate the
        // precondition a tiered run starts from, so an unqualified "certified"
        // over a rig that certified only per-port items tells the operator a
        // Tier-2/3 run may start from it. Tier 1 is the *baseline* rig here
        // (§13's no-target doctrine), not a corner case, so this is the common
        // reading, and the report never named the tier anywhere else either.
        // The tier is the *topology* discovery found; whether that tier's items
        // executed is a second fact. Deriving the sentence from one number let
        // P5 print "Tier 1 — a dangling converter" directly above its own
        // observation line reading `paired with …`, whenever a discovered pair
        // failed to reopen for characterization.
        let tier = facts.tier();
        let scope = match tier {
            3 if facts.mismatch_pairs > 0 => format!(
                "**Tier 3** — {n} cross-wired {pair}, independent clocks, so the rate ladder and the deliberate baud mismatch ran.",
                n = facts.mismatch_pairs,
                pair = if facts.mismatch_pairs == 1 {
                    "pair"
                } else {
                    "pairs"
                },
            ),
            3 => "**Tier 3 wiring, uncertified** — a cross-wired pair was discovered, but its independent-clock certificate did not complete, so the rate ladder and the deliberate baud mismatch did **not** run. The pair's certificate line above says why."
                .to_owned(),
            2 => "**Tier 2** — a TX↔RX jumper: a real driver data path, but on one clock, so the rate ladder and the deliberate baud mismatch did **not** run (both need a cross-wired pair)."
                .to_owned(),
            // Deliberately does *not* say "and no break was received by anything":
            // true of Tier 1 and equally true of Tier 3, because no probe here
            // drives a break into an open, counting peer at any tier — so a
            // tier-scoped sentence for a binary-scoped fact read as a promise that
            // upgrading the rig would get you break reception. It would not; that
            // is a job for the suite's `crossover_ports()`-gated guards.
            _ => "**Tier 1** — a dangling converter: per-port items only. The rate ladder and the deliberate baud mismatch did **not** run."
                .to_owned(),
        };
        // The ceiling clause is only meaningful below a complete top-tier
        // certificate — appending "may not start above Tier 3" would be vacuous
        // advice in the one case where the operator has the whole thing.
        let ceiling = if tier < 3 || facts.mismatch_pairs == 0 {
            " and may not start *above* the tier named here, because the items a higher tier leans on did not execute"
        } else {
            ""
        };
        (
            Status::Supported,
            format!(
                "Rig discovered and certified at {scope} A tiered checklist run starts from this certificate (§15.21){ceiling}."
            ),
        )
    } else {
        // Two different reasons land here and they need different advice, for the
        // reason `P5_UNCHARACTERIZED` documents: on Linux nothing answered
        // TIOCGICOUNT, so these really are sims and a real adapter would certify;
        // off Linux nothing *can* answer it, so telling the operator to attach real
        // adapters is advice they may already have followed — measured 2026-07-28
        // against two genuine FTDI adapters on macOS, which this arm then invited to
        // be replaced with real ones.
        #[cfg(target_os = "linux")]
        let why = "characterization skipped on non-UART sims — the certificate populates on real adapters (§13, no-target doctrine)";
        #[cfg(not(target_os = "linux"))]
        let why = "characterization does not run on this platform at all — the UART predicate is TIOCGICOUNT, which is Linux-only, so no port certifies here however real it is. Discovery and pairing above are still measured; run the certificate on a Linux box (§13, best-effort tier)";
        (
            Status::Supported,
            format!("Rig discovered and classified (above); {why}."),
        )
    }
}

// ---------------------------------------------------------------------------
// P3 — serial-port fit (§7.1, §13). Runs only on an explicitly named --port.
// ---------------------------------------------------------------------------

/// P3's question, verbatim, shared by the real probe and by `main`'s
/// no-`--port` placeholder.
///
/// **The duplication these consts remove was not cosmetic.** `Build::probe_set`
/// digests each probe's question so two reports can be checked for
/// comparability, and a placeholder whose wording drifted from the real probe's
/// made a passive run and a rig run of the *same binary* fingerprint differently
/// — which would have told the operator the 6.18-vs-7.0 diff was invalid.
pub const P3_QUESTION: &str = "Custom baud acceptance, TIOCEXCL exclusivity, modem-line set/read, and break toggling on a real port (§7.1).";

/// P5's question, verbatim. Shared for the reason [`P3_QUESTION`] gives.
pub const P5_QUESTION: &str = "Classify each named port (dangling/loopback/paired, both directions) and certify the rig for a tiered checklist run (§13, §15.21).";

pub fn p3_serial(port: &Path) -> Probe {
    // The title carries the device path so a two-port run's entries are
    // distinguishable; the *question* is the same on every port, which is what
    // keeps the probe-set fingerprint a property of the binary rather than of
    // how many ports were named.
    let p = Probe::new(
        "P3",
        &format!("serial-port fit ({})", port.display()),
        P3_QUESTION,
    );
    match p3_inner(port) {
        Ok(o) => {
            let p = p
                .observe("requested_baud", CUSTOM_BAUD)
                .observe(
                    "baud_readback",
                    o.baud_readback.map(|b| b as i64).unwrap_or(-1),
                )
                .observe("custom_baud_ok", o.custom_baud_ok)
                .observe("tiocexcl_refuses_second_open", o.exclusivity_ok)
                .observe("modem_calls_ok", o.modem_ok)
                .observe("break_ok", o.break_ok)
                .observe("tiocgicount_supported", o.icounter_supported);
            if o.custom_baud_ok && o.exclusivity_ok {
                p.verdict(
                    Status::Supported,
                    "serial2 fit confirmed; the daemon issues TIOCEXCL on the raw fd (serial2 sets O_NOCTTY only).",
                )
            } else {
                p.verdict(
                    Status::Degraded,
                    "A serial control did not behave as designed → apply it via the sys module on serial2's raw fd (§13).",
                )
            }
        }
        Err(e) if is_permission_denied(&e) => p.verdict(
            Status::skipped("permission denied"),
            "Grant access (udev GROUP=plugdev, or the dialout group) and re-run with --port.",
        ),
        Err(e) => p.verdict(Status::Degraded, &format!("probe error: {e}")),
    }
}

struct SerialFit {
    baud_readback: Option<u32>,
    custom_baud_ok: bool,
    exclusivity_ok: bool,
    modem_ok: bool,
    break_ok: bool,
    icounter_supported: bool,
}

fn p3_inner(port: &Path) -> anyhow::Result<SerialFit> {
    let sp = SerialPort::open(port, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(CUSTOM_BAUD)?;
        s.set_char_size(CharSize::Bits8);
        s.set_stop_bits(StopBits::One);
        s.set_parity(Parity::None);
        s.set_flow_control(FlowControl::None);
        Ok(s)
    })?;

    let baud_readback = sp.get_configuration().and_then(|c| c.get_baud_rate()).ok();
    let custom_baud_ok = baud_readback
        .map(|b| {
            (b as i64 - CUSTOM_BAUD as i64).unsigned_abs() as f64 / CUSTOM_BAUD as f64 <= 0.025
        })
        .unwrap_or(false);

    let modem_ok = sp.set_dtr(true).is_ok()
        && sp.set_dtr(false).is_ok()
        && sp.set_rts(true).is_ok()
        && sp.set_rts(false).is_ok()
        && sp.read_cts().is_ok()
        && sp.read_dsr().is_ok();
    let break_ok = sp.set_break(true).is_ok() && sp.set_break(false).is_ok();

    // Driver error/edge counters (§5, §7.1: surfaced in state where supported).
    let icounter_supported = sys::read_icounts(sp.as_raw_fd()).is_ok();

    let excl_set = sys::set_exclusive(sp.as_raw_fd(), true).is_ok();
    let second = SerialPort::open(port, 9600);
    let exclusivity_ok =
        excl_set && second.as_ref().err().and_then(|e| e.raw_os_error()) == Some(nix::libc::EBUSY);
    drop(second);
    let _ = sys::set_exclusive(sp.as_raw_fd(), false);

    Ok(SerialFit {
        baud_readback,
        custom_baud_ok,
        exclusivity_ok,
        modem_ok,
        break_ok,
        icounter_supported,
    })
}

fn is_permission_denied(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>()
        .map(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// P11 — real-port line-state counters (§5, §7.1). OPT-IN behind --port, exactly
// like P3 and P5: it opens a real device, and opening a port toggles DTR on
// equipment that may be live. The default (passive) run reports `skipped`.
// ---------------------------------------------------------------------------

/// The modem lines `TIOCMGET` reports, named. DTR/RTS are outputs the daemon
/// drives; CTS/DSR/DCD/RI are inputs §7.1 surfaces as presence and flow signals.
const MODEM_LINES: [(libc::c_int, &str); 6] = [
    (libc::TIOCM_DTR, "DTR"),
    (libc::TIOCM_RTS, "RTS"),
    (libc::TIOCM_CTS, "CTS"),
    (libc::TIOCM_DSR, "DSR"),
    (libc::TIOCM_CAR, "DCD"),
    (libc::TIOCM_RNG, "RI"),
];

/// One port's line state: each ioctl's availability and, when it answers, its
/// values. `Err` carries the errno text — "this driver does not implement it" is
/// itself an observation worth diffing, not an absence.
struct LineState {
    icounts: Result<sys::SerialIcounts, String>,
    modem: Result<libc::c_int, String>,
}

impl LineState {
    fn observations(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "tiocgicount_available": self.icounts.is_ok(),
            "tiocmget_available": self.modem.is_ok(),
        });
        let map = v.as_object_mut().expect("object literal");
        match &self.icounts {
            Ok(c) => {
                map.insert(
                    "counters".to_owned(),
                    serde_json::json!({
                        "rx": c.rx, "tx": c.tx,
                        "frame": c.frame, "overrun": c.overrun, "parity": c.parity,
                        "brk": c.brk, "buf_overrun": c.buf_overrun,
                        "cts": c.cts, "dsr": c.dsr, "rng": c.rng, "dcd": c.dcd,
                    }),
                );
            }
            Err(e) => {
                map.insert("tiocgicount_error".to_owned(), e.as_str().into());
            }
        }
        match &self.modem {
            Ok(bits) => {
                map.insert("modem_bits_hex".to_owned(), format!("0x{bits:04x}").into());
                map.insert(
                    "modem_lines_asserted".to_owned(),
                    MODEM_LINES
                        .iter()
                        .filter(|(bit, _)| bits & bit != 0)
                        .map(|(_, name)| serde_json::Value::from(*name))
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            Err(e) => {
                map.insert("tiocmget_error".to_owned(), e.as_str().into());
            }
        }
        v
    }

    /// The ioctls that did not answer, named for the verdict.
    fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.icounts.is_err() {
            out.push("TIOCGICOUNT");
        }
        if self.modem.is_err() {
            out.push("TIOCMGET");
        }
        out
    }
}

/// Do this box's real ports answer the two line-state ioctls the serial node's
/// accounting is built on?
///
/// `TIOCGICOUNT` (`serial_nexus_sys::read_icounts`) carries the driver's framing,
/// parity, overrun and break counts — loss that is otherwise **invisible**,
/// which is why §5/§7.1 surface them in `state` where supported — and
/// `TIOCMGET` (`serial_nexus_sys::read_modem_bits`) carries the modem lines §7.1's
/// presence handling reads. Both are legitimately absent on some drivers (a pts
/// answers neither `TIOCGICOUNT`; macOS has no `TIOCGICOUNT` at all), and the
/// node already treats `Err` as "omit the counters" rather than as a fault — so
/// an absent ioctl is `degraded` with the port and ioctl named, **never**
/// `unsupported`.
///
/// Opt-in behind `--port` like P3/P5, and gentler than either: it opens the port
/// with its **current settings unchanged** (no baud change, no raw mode) and
/// transmits nothing. The open itself still toggles DTR, which is the whole
/// reason the doctor never opens a port it was not handed.
pub fn p11_line_state(ports: &[PathBuf], rig: RigFacts) -> Probe {
    let mut p = Probe::new(
        "P11",
        "real-port line-state counters",
        "Do TIOCGICOUNT (driver error/edge counters) and TIOCMGET (modem lines) answer on a real port, and what do they currently read (§5, §7.1)?",
    );

    let mut opened = 0usize;
    let mut perm_denied = false;
    let mut missing: Vec<String> = Vec::new();
    for port in ports {
        // Settings returned unchanged: serial2 hands the closure the port's
        // current configuration, so this is the least invasive open the crate
        // allows — P11 inspects, it does not configure.
        match SerialPort::open(port, |s: Settings| Ok(s)) {
            Ok(sp) => {
                opened += 1;
                let state = LineState {
                    icounts: sys::read_icounts(sp.as_raw_fd()).map_err(|e| e.to_string()),
                    modem: sys::read_modem_bits(sp.as_raw_fd()).map_err(|e| e.to_string()),
                };
                for ioctl in state.missing() {
                    missing.push(format!("{}: {ioctl}", port.display()));
                }
                p = p.observe(&port.display().to_string(), state.observations());
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    perm_denied = true;
                }
                p = p.observe(&port.display().to_string(), format!("open error: {e}"));
            }
        }
    }

    let (status, consequence) = p11_verdict(ports.len(), opened, perm_denied, &missing, rig);
    p.verdict(status, &consequence)
}

/// Fold P11's per-port results into one verdict (the `p5_verdict` lesson: keep
/// the judgement pure so it is testable without a bench, since the rest of the
/// probe needs hardware).
///
/// `unsupported` is unreachable by construction and that is deliberate — an
/// absent `TIOCGICOUNT` contradicts nothing: it is Linux-only, absent on several
/// drivers, and the serial node omits the counters instead of faulting (§5). It
/// is also a live gate (`expectations/linux.jq` requires
/// `.summary.unsupported == 0`), so a probe that reddened on a healthy box with a
/// plain adapter would be a bug, not a finding.
fn p11_verdict(
    named: usize,
    opened: usize,
    perm_denied: bool,
    missing: &[String],
    rig: RigFacts,
) -> (Status, String) {
    if named == 0 {
        return (
            Status::skipped("no --port named"),
            "Re-run with --port /dev/ttyUSB0 to read the driver counters and modem lines a serial node's accounting depends on (a dangling converter is enough — no target device, §13).".to_owned(),
        );
    }
    if opened == 0 {
        let reason = if perm_denied {
            "permission denied"
        } else {
            "no port opened"
        };
        return (
            Status::skipped(reason),
            "Grant access (udev GROUP=plugdev, or the dialout group) and re-run with --port."
                .to_owned(),
        );
    }
    if !missing.is_empty() {
        return (
            Status::Degraded,
            format!(
                "A named port does not implement every line-state ioctl ({}). The serial node omits what it cannot read rather than faulting (§5), so the port still works — what is lost is the error/overrun accounting that makes silent loss visible, and §7.1's modem-line presence where TIOCMGET is the missing one. Expected on a pts and on macOS (no TIOCGICOUNT); on a real Linux UART it is worth investigating.",
                missing.join(", ")
            ),
        );
    }
    // **Only blame the mismatch item when the mismatch item ran.** The deliberate
    // baud mismatch transmits inside `p5_certify_pair` and nowhere else, so on a
    // rig P5 found no pair on — a dangling converter, §13's baseline — it never
    // executed, and naming it as the usual cause of a nonzero `frame` sends the
    // operator to a mechanism that was not present. Measured on the 2026-07-27
    // 6.18 run: one dangling FT232R reporting `frame=4`, explained by this
    // sentence as the mismatch item, on a box where no pair existed to mismatch.
    //
    // Three-valued, because the rig is: the mismatch either transmitted, or it
    // did not and the port is jumpered, or it did not and nothing is wired to it.
    // A two-valued version told a Tier-2 operator to reason about "a dangling
    // port" that was not theirs.
    let counters = if rig.mismatch_pairs > 0 {
        "and P3/P5 transmit on these same ports earlier in the same invocation (a nonzero `frame` here is usually P5's deliberate baud-mismatch item, not a fault)".to_owned()
    } else {
        let where_from = match rig.tier() {
            3 => {
                "This rig is cross-wired, but its pair certificate did not complete, so the mismatch pattern never reached the wire; re-run once the pair certifies before reading anything into `frame`"
            }
            2 => {
                "On a TX↔RX jumpered port, framing errors are your own transmit looping back — check the baud both ends of the jumper were set to"
            }
            _ => {
                "On a dangling port that is either history since the driver bound the device or crosstalk into a floating RX; replug the adapter, which rebinds the driver and zeroes the counters, and re-run to tell the two apart"
            }
        };
        format!(
            "and P3/P5 transmit on these same ports earlier in the same invocation — but P5 transmitted **no deliberate baud mismatch** this run, so that item cannot account for a nonzero `frame` here. {where_from}"
        )
    };
    (
        Status::Supported,
        format!(
            "Both line-state ioctls answer on {opened} of {named} named port(s): the driver counters (§5, §7.1) and the modem lines are readable, so serial state carries real error/overrun accounting rather than omitting it. Read the counts as a snapshot of a cumulative total, not as a measurement of this run — they count since the driver bound the device, {counters}. Across kernels, diff the ioctl *availability* and the field set; the absolute counts differ by construction."
        ),
    )
}

// ---------------------------------------------------------------------------
// Environment checks
// ---------------------------------------------------------------------------

pub fn environment(dev_root: &Path, sys_root: &Path, named_ports: &[PathBuf]) -> Vec<EnvCheck> {
    let mut checks = Vec::new();

    // **A report has to be able to name its own kernel.** §16.13 says provenance is
    // *recorded, never asserted*, and this pair is the provenance every cross-kernel
    // claim in the tree rests on. Read from `/proc/sys/kernel/osrelease` and
    // `/etc/os-release`, both of which exist only on Linux, it recorded `kernel: ""`
    // and `os: "unknown"` on every macOS run — and marked them `Supported` — so the
    // Darwin version had to be typed into `docs/doctor/README.md` by hand beside the
    // file. A hand-recorded field in an index is exactly the assertion committing
    // artifacts exists to replace.
    //
    // `uname(2)` is POSIX and answers on both. `nodename` is deliberately **not**
    // read: it is the machine's hostname, and nothing here needs it.
    let uts = nix::sys::utsname::uname().ok();
    let (kernel, kernel_status) = match &uts {
        // `release` alone, which is what the Linux side has always printed
        // (`7.0.0-29-generic`), so the field keeps its shape across the archive and a
        // diff against a pre-2026-08-05 report reads normally. On Darwin it is the
        // number the file names were carrying by hand: `24.6.0`.
        Some(u) => (
            u.release().to_string_lossy().into_owned(),
            Status::Supported,
        ),
        None => (
            "unknown — uname(2) did not answer".to_owned(),
            Status::Degraded,
        ),
    };
    checks.push(EnvCheck::new("kernel", kernel, kernel_status));
    checks.push(EnvCheck::new("os", distro(uts.as_ref()), Status::Supported));

    // $XDG_RUNTIME_DIR — the non-root control-socket home (§10).
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if Path::new(&dir).is_dir() => {
            checks.push(EnvCheck::new("XDG_RUNTIME_DIR", dir, Status::Supported));
        }
        Ok(dir) => checks.push(EnvCheck::new(
            "XDG_RUNTIME_DIR",
            format!("{dir} (missing)"),
            Status::Degraded,
        )),
        Err(_) => checks.push(EnvCheck::new(
            "XDG_RUNTIME_DIR",
            "unset — daemon falls back to /run or a --socket override",
            Status::Degraded,
        )),
    }

    // by-id tree. **The tree's absence is not the adapter's absence**, and reporting
    // it as one is how this check came to contradict a daemon working beside it: with
    // `/sys` mounted and no udev `60-serial.rules` (a container handed a bare
    // `--device=/dev/ttyUSB0`, a busybox-mdev image) it said "absent (no USB-serial
    // adapter)" and skipped, about a tree where the adapter is present in sysfs and
    // at `/dev/ttyUSB0` (review 32 RES-2). The resolver learned to resolve there; the
    // diagnostic has to learn to *say* so, because AGENTS §3 makes this report the
    // first attachment on every bug report.
    //
    // A differing environment is `degraded` with the observation named, never
    // `unsupported` (§13): the daemon's fallback applies and it works — what the
    // operator loses is udev's stable naming, which is worth a line in the report.
    let by_id = dev_root.join("dev/serial/by-id");
    let resolver = serial_nexus_core::Resolver::with_roots(dev_root, sys_root);
    let adapters = resolver.discover_adapters();
    let candidates = resolver.enumerate_ports();
    if by_id.is_dir() {
        checks.push(EnvCheck::new(
            "/dev/serial/by-id",
            format!("present ({} adapter(s))", adapters.len()),
            Status::Supported,
        ));
    } else if !candidates.is_empty() {
        checks.push(EnvCheck::new(
            "/dev/serial/by-id",
            format!(
                "absent — {} serial device(s) visible another way (sysfs / by-path / cu.*); identities come from those instead of udev's stable names (§12). No 60-serial.rules here — a container's bare --device=…, a busybox-mdev image; macOS has no by-id tree at all (§13)",
                candidates.len()
            ),
            Status::Degraded,
        ));
    } else {
        checks.push(EnvCheck::new(
            "/dev/serial/by-id",
            "absent, and no serial device visible through sysfs, by-path or cu.* either",
            Status::skipped("no adapter"),
        ));
    }

    // User and group membership relevant to serial access.
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|u| u.name)
        .unwrap_or_else(|| nix::unistd::getuid().to_string());
    checks.push(EnvCheck::new("user", user, Status::Supported));
    for grp in ["dialout", "plugdev"] {
        checks.push(group_membership_check(grp));
    }

    // Access to each discovered or named serial device node. Enumerated candidates
    // are in the set too, not just by-id adapters: in the no-udev environment above
    // the by-id list is empty, and reporting no access line at all for a device the
    // operator is staring at is the same misdirection in a quieter form (RES-2).
    let mut ports: Vec<PathBuf> = adapters.iter().map(|a| a.dev_path.clone()).collect();
    for c in &candidates {
        if !ports.contains(&c.path) {
            ports.push(c.path.clone());
        }
    }
    for p in named_ports {
        if !ports.contains(p) {
            ports.push(p.clone());
        }
    }
    for dev in ports {
        checks.push(device_access_check(&dev));
    }

    checks
}

/// The OS name a human reads first. `/etc/os-release`'s `PRETTY_NAME` where a
/// distribution publishes one; otherwise what `uname(2)` can say, which on Darwin is
/// `Darwin 24.6.0 (x86_64)` rather than the `unknown` this printed for four
/// generations. Only `"unknown"` when neither source answers — a genuine gap rather
/// than the platform's normal state.
fn distro(uts: Option<&nix::sys::utsname::UtsName>) -> String {
    distro_from(read_trimmed(Path::new("/etc/os-release")), uts)
}

/// The decision itself, with `/etc/os-release`'s content passed in rather than read.
///
/// Split out **so the off-Linux arm is testable from Linux**. A guard that only
/// asserted "the kernel field is non-empty" would pass on the box it was written on
/// and prove nothing about the platform the field was empty on for four generations —
/// §9's proxy in space, in the exact place it did the damage. Handing the file's
/// content in makes "no `/etc/os-release`" an ordinary input, so the Darwin path runs
/// in the Linux suite on every push.
fn distro_from(os_release: Option<String>, uts: Option<&nix::sys::utsname::UtsName>) -> String {
    if let Some(content) = os_release {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                return v.trim_matches('"').to_owned();
            }
        }
    }
    match uts {
        Some(u) => format!(
            "{} {} ({})",
            u.sysname().to_string_lossy(),
            u.release().to_string_lossy(),
            u.machine().to_string_lossy()
        ),
        None => "unknown".into(),
    }
}

fn group_membership_check(group: &str) -> EnvCheck {
    let member = is_group_member(group);
    match member {
        Some(true) => EnvCheck::new(&format!("group:{group}"), "member", Status::Supported),
        Some(false) => EnvCheck::new(&format!("group:{group}"), "not a member", Status::Degraded),
        None => EnvCheck::new(
            &format!("group:{group}"),
            "group not present on system",
            Status::skipped("no such group"),
        ),
    }
}

fn is_group_member(group: &str) -> Option<bool> {
    let target = nix::unistd::Group::from_name(group).ok().flatten()?.gid;
    if nix::unistd::getgid() == target || nix::unistd::getegid() == target {
        return Some(true);
    }
    // `getgroups` is unavailable on Apple platforms in nix (Apple's semantics
    // differ). Supplementary-group membership is simply unknown there → reported as
    // skipped, matching §13's macOS best-effort environment checks.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let groups = nix::unistd::getgroups().ok()?;
        Some(groups.contains(&target))
    }
}

fn device_access_check(dev: &Path) -> EnvCheck {
    use nix::unistd::{AccessFlags, access};
    let name = format!("access:{}", dev.display());
    match access(dev, AccessFlags::R_OK | AccessFlags::W_OK) {
        Ok(()) => EnvCheck::new(&name, "read+write", Status::Supported),
        Err(_) => EnvCheck::new(
            &name,
            "no access — grant via udev (GROUP=plugdev) or dialout",
            Status::Degraded,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fail(item: &str, integrity: bool) -> CertFailure {
        CertFailure {
            item: item.to_owned(),
            integrity,
        }
    }

    /// A Tier-3 rig: one cross-wired pair that reached `p5_certify_pair`, so the
    /// rate ladder and the deliberate mismatch really ran. The failure-precedence
    /// folds below are tier-independent and use this as their neutral input;
    /// `RigFacts::default()` is the Tier-1 shape (no pair, no jumper).
    fn paired() -> RigFacts {
        RigFacts {
            discovered_pairs: 1,
            mismatch_pairs: 1,
            loopbacks: 0,
        }
    }

    /// A cross-wired pair discovery verified, whose characterization then failed:
    /// Tier-3 *wiring*, no mismatch transmitted. The state that used to print
    /// "Tier 1 — a dangling converter" over an observation reading `paired with`.
    fn paired_uncertified() -> RigFacts {
        RigFacts {
            discovered_pairs: 1,
            mismatch_pairs: 0,
            loopbacks: 0,
        }
    }

    // -- P4 / environment: the diagnostic must not contradict the daemon (RES-2) ---

    /// A self-cleaning fixture tree under the system temp dir. No `tempfile`
    /// dependency — the doctor's dependency list is part of the licensing gate
    /// (§13), and the resolver's own tests do the same.
    struct TmpTree(PathBuf);

    impl TmpTree {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("snx-doctor-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TmpTree(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TmpTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(p: &Path, contents: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    /// One USB tty device in sysfs with a `/dev` node and **no udev symlinks at
    /// all** — the RES-2 environment: a container handed `--device=/dev/ttyUSB0`, a
    /// busybox-mdev image. Mirrors `serial-nexus-core`'s `add_usb_device_unlinked`.
    fn unlinked_usb_device(root: &Path, dev_name: &str) {
        write_file(&root.join("dev").join(dev_name), "");
        let usbdev = root.join("sys/bus/usb/devices/1-1");
        write_file(&usbdev.join("idVendor"), "0403");
        write_file(&usbdev.join("idProduct"), "6001");
        write_file(&usbdev.join("serial"), "UNIQ01");
        write_file(&usbdev.join("manufacturer"), "FTDI");
        write_file(&usbdev.join("product"), "FT232R USB UART");
        write_file(&usbdev.join("1-1:1.0/bInterfaceNumber"), "00");
        let class = root.join("sys/class/tty").join(dev_name);
        std::fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink("../../../bus/usb/devices/1-1/1-1:1.0", class.join("device"))
            .unwrap();
    }

    fn env_check<'a>(checks: &'a [EnvCheck], name: &str) -> &'a EnvCheck {
        checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no {name} check in {checks:#?}"))
    }

    fn observed(p: &Probe, key: &str) -> Option<serde_json::Value> {
        p.observations
            .iter()
            .find(|o| o.key == key)
            .map(|o| o.value.clone())
    }

    /// RES-2, the doctor's half. AGENTS §3 makes this report the first attachment on
    /// every bug report, and in the one environment the finding is about it said the
    /// opposite of the truth: gated on `dev/serial/by-id.is_dir()`, P4 skipped with
    /// "No USB-serial adapter present" and the environment check said "absent (no
    /// USB-serial adapter)" — about a tree where the adapter is in sysfs and at
    /// `/dev/ttyUSB0`, and where the daemon now binds it.
    ///
    /// Fail-first, run against the pre-fix code: P4 `skipped (no /dev/serial/by-id
    /// tree)` with zero observations, and the by-id check `skipped (no adapter)`.
    #[test]
    fn p4_and_the_environment_see_an_adapter_udev_never_named() {
        let t = TmpTree::new("res2");
        unlinked_usb_device(t.path(), "ttyUSB0");
        let sys_root = t.path().join("sys");
        assert!(
            !t.path().join("dev/serial").exists(),
            "the fixture must model a tree with no udev serial links at all"
        );

        let p = p4_resolver(t.path(), &sys_root);
        assert_eq!(
            p.status.label(),
            "supported",
            "P4 skipped a resolvable adapter: {:?}",
            p.status
        );
        assert_eq!(observed(&p, "by_id_tree"), Some("absent".into()));
        assert_eq!(observed(&p, "sysfs_only"), Some(1.into()));
        assert_eq!(
            observed(&p, &t.path().join("dev/ttyUSB0").display().to_string()),
            Some("usb:0403:6001:UNIQ01:00".into()),
            "the identity the daemon would store must be in the report: {p:#?}"
        );
        assert!(
            p.consequence.contains("no udev 60-serial.rules"),
            "the environment difference must be named, not implied: {}",
            p.consequence
        );

        let checks = environment(t.path(), &sys_root, &[]);
        let by_id = env_check(&checks, "/dev/serial/by-id");
        assert_eq!(
            by_id.status.label(),
            "degraded",
            "a differing environment is degraded with the observation named, never \
             skipped-as-absent (§13): {by_id:#?}"
        );
        assert!(
            by_id.value.contains("1 serial device"),
            "the check must count the devices that ARE visible: {}",
            by_id.value
        );
        // …and the device gets an access line, which it had none of before: an
        // operator debugging a permissions problem here saw no row at all.
        let access = format!("access:{}", t.path().join("dev/ttyUSB0").display());
        env_check(&checks, &access);
    }

    /// The other side of the same coin, and the reason the fix cannot simply stop
    /// skipping: a box with **no** serial device must still read `skipped`, because
    /// that is what CI runs and `expectations/linux.jq` admits only `supported` or
    /// `skipped` for P4. A verdict that reddened an adapter-less runner would be a
    /// bug, not a finding (probes.rs module doc).
    #[test]
    fn an_adapterless_tree_still_skips_rather_than_reddening() {
        let t = TmpTree::new("empty");
        std::fs::create_dir_all(t.path().join("dev")).unwrap();
        let sys_root = t.path().join("sys");

        let p = p4_resolver(t.path(), &sys_root);
        assert_eq!(p.status.label(), "skipped", "{:?}", p.status);
        let checks = environment(t.path(), &sys_root, &[]);
        assert_eq!(
            env_check(&checks, "/dev/serial/by-id").status.label(),
            "skipped"
        );
    }

    /// The udev-equipped box — this project's dev box and the 6.18 production target
    /// — must read exactly as it did: `supported`, one observation per by-id entry,
    /// and no by-id-absence sentence in the consequence. The RES-2 fix widened where
    /// P4 looks; it must not have moved the verdict anywhere it already worked.
    #[test]
    fn a_by_id_equipped_tree_reads_exactly_as_before() {
        let t = TmpTree::new("byid");
        unlinked_usb_device(t.path(), "ttyUSB0");
        let by_id = t.path().join("dev/serial/by-id");
        std::fs::create_dir_all(&by_id).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB0",
            by_id.join("usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0"),
        )
        .unwrap();
        let sys_root = t.path().join("sys");

        let p = p4_resolver(t.path(), &sys_root);
        assert_eq!(p.status.label(), "supported");
        assert_eq!(observed(&p, "by_id_tree"), Some("present".into()));
        assert_eq!(observed(&p, "count"), Some(1.into()));
        assert_eq!(
            observed(&p, "sysfs_only"),
            Some(0.into()),
            "a device udev named is not a sysfs-only device: {p:#?}"
        );
        assert_eq!(
            observed(&p, "usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0"),
            Some("usb:0403:6001:UNIQ01:00".into())
        );
        assert!(
            !p.consequence.contains("No /dev/serial/by-id tree"),
            "{}",
            p.consequence
        );
        assert_eq!(
            env_check(&environment(t.path(), &sys_root, &[]), "/dev/serial/by-id")
                .status
                .label(),
            "supported"
        );
    }

    /// The sim path (§15.21: "characterization reporting skipped on non-UARTs, so
    /// P5's logic never waits for a bench") and the fully certified rig both stay
    /// `supported`, with their consequence lines explaining which one happened.
    #[test]
    fn a_clean_rig_is_supported_certified_or_skipped() {
        let (status, why) = p5_verdict(true, true, &[], &[], paired());
        assert_eq!(status.label(), "supported");
        assert!(why.contains("certified"), "{why}");

        let (status, why) = p5_verdict(true, false, &[], &[], RigFacts::default());
        assert_eq!(status.label(), "supported");
        // *Why* nothing certified is platform-specific, because the UART predicate
        // is (see `P5_UNCHARACTERIZED`): on Linux the ports really were sims; off
        // Linux `TIOCGICOUNT` cannot answer for any port, real adapters included.
        // Assert the arm this build ships — pinning the Linux wording everywhere is
        // what failed this test on a Mac against code that had just become *more*
        // accurate there.
        #[cfg(target_os = "linux")]
        assert!(why.contains("skipped on non-UART sims"), "{why}");
        #[cfg(not(target_os = "linux"))]
        assert!(why.contains("TIOCGICOUNT, which is Linux-only"), "{why}");
        // Portable and the clause with teeth: whichever arm ran, an uncertified rig
        // must not borrow the certified arm's opening. That sentence is what a
        // tiered checklist run reads to decide it may start (§15.21), and P5's prose
        // has now over-claimed three times (AGENTS §2's 6.18 entry has the other two).
        assert!(!why.contains("Rig discovered and certified"), "{why}");
    }

    /// **A Tier-1 certificate must say Tier 1.** §15.21 makes P5's certificate the
    /// precondition every tiered checklist run starts from, which only means
    /// something if the certificate says what it covers. The unqualified sentence
    /// this guards ("Rig discovered and certified; every tiered checklist run
    /// starts from this certificate") was emitted for *any* UART rig — including a
    /// dangling converter, §13's baseline, where `pairs` is empty, so
    /// `p5_certify_pair` never runs, the rate ladder and the deliberate mismatch
    /// never transmit, and neither integrity-failure site can fire. An operator
    /// reading it starts a Tier-2/3 run from a Tier-1 certificate. Observed
    /// verbatim on the 2026-07-27 6.18 report.
    ///
    /// Each tier must therefore name itself *and* name what did not run, so the
    /// three lines are not interchangeable.
    #[test]
    fn the_certificate_names_its_tier_and_what_that_tier_did_not_run() {
        let dangling = RigFacts::default();
        let jumpered = RigFacts {
            discovered_pairs: 0,
            mismatch_pairs: 0,
            loopbacks: 1,
        };
        assert_eq!(
            (dangling.tier(), jumpered.tier(), paired().tier()),
            (1, 2, 3)
        );

        let (status, why) = p5_verdict(true, true, &[], &[], dangling);
        assert_eq!(status.label(), "supported");
        assert!(why.contains("Tier 1"), "tier not named: {why}");
        // The claim that matters: the mismatch is reported as *not* run.
        assert!(
            why.contains("deliberate baud mismatch did **not** run"),
            "a Tier-1 certificate implied the pair items ran: {why}"
        );

        let (_, why) = p5_verdict(true, true, &[], &[], jumpered);
        assert!(why.contains("Tier 2"), "tier not named: {why}");
        assert!(
            why.contains("did **not** run"),
            "a Tier-2 certificate implied the pair items ran: {why}"
        );

        let (_, why) = p5_verdict(true, true, &[], &[], paired());
        assert!(why.contains("Tier 3"), "tier not named: {why}");
        assert!(
            why.contains("deliberate baud mismatch ran"),
            "a Tier-3 certificate did not claim the pair items: {why}"
        );

        // **A discovered pair is never described as a dangling converter.** When
        // characterization fails, `mismatch_pairs` is 0 while `discovered_pairs`
        // is not; deriving the sentence from a single count printed "Tier 1 — a
        // dangling converter: per-port items only" directly above P5's own
        // observation line reading `paired with …`.
        assert_eq!(paired_uncertified().tier(), 3, "wiring is the tier");
        let (_, why) = p5_verdict(true, true, &[], &[], paired_uncertified());
        assert!(
            !why.contains("dangling converter"),
            "a cross-wired pair was described as dangling: {why}"
        );
        assert!(
            why.contains("Tier 3 wiring, uncertified"),
            "the uncertified-pair state was not named: {why}"
        );
        assert!(
            why.contains("did **not** run"),
            "an uncertified pair implied its items ran: {why}"
        );

        // And no state's line may be mistaken for another's.
        let lines: Vec<String> = [dangling, jumpered, paired(), paired_uncertified()]
            .iter()
            .map(|f| p5_verdict(true, true, &[], &[], *f).1)
            .collect();
        assert_eq!(
            lines
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4,
            "two rig states produced the same certificate line"
        );
    }

    /// A data-integrity failure is a stop condition: the certificate is the
    /// precondition every tiered run starts from, so a rig that loses bytes must
    /// not report `supported` with exit code 0 (review 26, DOC-1b).
    #[test]
    fn a_data_integrity_failure_is_unsupported_and_names_the_item() {
        let (status, why) = p5_verdict(
            true,
            true,
            &[fail("usb-A ↔ usb-B: rate_ladder", true)],
            &[],
            paired(),
        );
        assert!(status.is_unsupported(), "verdict was {}", status.label());
        assert!(
            why.contains("usb-A ↔ usb-B: rate_ladder"),
            "failing item not named: {why}"
        );
    }

    /// An uncharacterized item degrades — the rig works, but a tier leaning on
    /// that item would be running uncertified.
    #[test]
    fn an_uncharacterized_item_degrades_and_names_the_item() {
        let (status, why) = p5_verdict(true, true, &[fail("usb-A: break", false)], &[], paired());
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("usb-A: break"), "item not named: {why}");
    }

    /// Miswiring keeps its own (discovery-side) message, and still names any
    /// certificate item that also failed — both facts reach the operator.
    #[test]
    fn miswiring_degrades_and_still_reports_uncertified_items() {
        let (status, why) = p5_verdict(
            false,
            true,
            &[fail("usb-A: custom_baud", false)],
            &[],
            paired(),
        );
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("miswired"), "{why}");
        assert!(why.contains("usb-A: custom_baud"), "{why}");
    }

    /// A port whose peer hung up mid-probe degrades the verdict and names the port.
    /// It is `degraded`, never `unsupported`: P5 could not classify the port, which
    /// is a rig fault the operator must clear before a tier, but it is not the
    /// demonstrated byte loss that `unsupported` is reserved for. Without this fold
    /// the observation printed "hung up (peer closed) — not classifiable" beside a
    /// `supported` verdict, which is the DOC-1b shape: an observation nobody acts on.
    #[test]
    fn a_hung_up_peer_degrades_and_names_the_port() {
        let (status, why) = p5_verdict(
            true,
            false,
            &[],
            &["usb-A".to_string()],
            RigFacts::default(),
        );
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("usb-A"), "port not named: {why}");
        assert!(why.contains("hung up"), "{why}");
    }

    /// Integrity outranks miswiring: a half-crossed rig that also corrupts data is
    /// a stop condition, not a warning.
    #[test]
    fn integrity_failure_outranks_miswiring() {
        let (status, _) = p5_verdict(
            false,
            true,
            &[fail("a: break", false), fail("a ↔ b: rate_ladder", true)],
            &[],
            paired(),
        );
        assert!(status.is_unsupported());
    }

    /// The two constructors that feed the fold: a skipped characterization records
    /// no failure (the CI sim must stay green), while an unavailable one degrades.
    #[test]
    fn skipped_certificates_carry_no_failure_but_unavailable_ones_do() {
        let skipped = Certificate::skipped("not a UART");
        assert_eq!(skipped.line, "skipped (not a UART)");
        assert!(skipped.failures.is_empty());
        assert_eq!(
            p5_verdict(true, false, &skipped.failures, &[], RigFacts::default())
                .0
                .label(),
            "supported"
        );

        let missing = Certificate::unavailable("pair reopen failed", "pair_reopen");
        assert_eq!(missing.failures, vec![fail("pair_reopen", false)]);
        assert_eq!(
            p5_verdict(true, true, &missing.failures, &[], paired())
                .0
                .label(),
            "degraded"
        );
    }

    /// A failure is reported against the port (or pair) it was measured on.
    #[test]
    fn failures_are_qualified_by_their_subject() {
        assert_eq!(
            fail("rate_ladder", true).qualified("usb-A ↔ usb-B"),
            fail("usb-A ↔ usb-B: rate_ladder", true)
        );
    }

    /// P6..P13 may never report `unsupported`, on any box.
    ///
    /// Each measures *which* of several legitimate kernel behaviours applies, and
    /// the shipped daemon is correct under all of them — so none can contradict a
    /// design premise, which is what `unsupported` means. It is also a live gate:
    /// `expectations/linux.jq` requires `.summary.unsupported == 0` and
    /// `itest/tests/meta_gates.rs` asserts the doctor reports no unsupported
    /// capability, so a probe that reddened on a healthy box would fail CI. This
    /// runs the real probes (about a second of ptys and polls on Linux, and roughly
    /// 1.2 s more on Darwin, where P13's two no-reader shapes each pay `ttywait`'s
    /// ~0.6 s `t_timeout`) rather than reasoning
    /// about the code, because the arm that would break the rule is one someone adds
    /// later. P11 is included with **no** ports, which is its default shape: a
    /// passive run must stay `skipped` and must never open anything.
    #[test]
    fn the_kernel_diff_probes_never_report_unsupported() {
        for p in [
            p6_last_close_readiness(),
            p7_collapsed_session(),
            p8_epoll_readiness(),
            p9_poll_granularity(),
            p10_pty_buffer_depth(),
            p11_line_state(&[], RigFacts::default()),
            // P12 belongs here because `expectations/linux.jq` and
            // `expectations/macos.jq` both gate it, and a guard that names the
            // gate has to cover what the gate covers. It is `skipped` on Linux
            // (the latch is inert there, §15.39) and carries measurements where
            // it runs, so both assertions below apply to it unchanged.
            p12_session_edge(),
            // P13 belongs here for the same reason as P12, and was missed when it
            // landed: both expectation files gate it by name, so an `unsupported`
            // P13 reddens both lanes. The measurements assertion matters more here
            // than anywhere else — P13 never judges the policy (every policy is
            // legitimate), so its verdict word carries no information at all and the
            // observations are the entire content of the probe.
            p13_last_close_disposition(),
        ] {
            assert!(
                !p.status.is_unsupported(),
                "{} reported unsupported, which reddens expectations/linux.jq and \
                 meta_gates: {}",
                p.id,
                p.consequence
            );
            // A probe that RAN must carry numbers: the report is the diff artifact,
            // so a verdict word alone is not enough (§13). A `skipped` one is exempt
            // — it measured nothing by definition, and its reason says why (P11 with
            // no --port, or P8 off Linux).
            let skipped = p.status.label() == "skipped";
            assert!(
                skipped || !p.observations.is_empty(),
                "{} emitted no measurements — the report is the diff artifact, so a \
                 verdict word alone is not enough (§13)",
                p.id
            );
        }
    }

    /// A passive run (no `--port`) must not open anything: P11 skips, names the
    /// reason, and observes nothing. This is the doctor's standing rule — a listed
    /// port may be wired to live equipment and opening it toggles DTR — and P11 is
    /// the newest probe that could break it.
    #[test]
    fn p11_is_opt_in_and_reports_nothing_without_a_port() {
        let p = p11_line_state(&[], RigFacts::default());
        assert_eq!(p.status.label(), "skipped");
        assert!(p.observations.is_empty(), "a passive run opened a port");
        let (status, why) = p11_verdict(0, 0, false, &[], RigFacts::default());
        assert_eq!(status.label(), "skipped");
        assert!(why.contains("--port"), "{why}");
    }

    /// P11's fold: an unanswered ioctl degrades and **names the port and the
    /// ioctl**, and every ioctl answering is `supported`. Neither is
    /// `unsupported` — an absent `TIOCGICOUNT` is Linux-only-and-driver-specific,
    /// and the serial node omits the counters rather than faulting (§5), so
    /// reddening `.summary.unsupported` over it would be a false alarm on a pts
    /// and on every macOS box.
    #[test]
    fn p11_degrades_on_a_missing_ioctl_and_never_reports_unsupported() {
        let (status, why) = p11_verdict(
            1,
            1,
            false,
            &["/dev/pts/9: TIOCGICOUNT".to_owned()],
            RigFacts::default(),
        );
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("/dev/pts/9: TIOCGICOUNT"), "{why}");

        let (status, why) = p11_verdict(2, 2, false, &[], paired());
        assert_eq!(status.label(), "supported");
        assert!(why.contains("2 of 2"), "{why}");
    }

    /// **P11 may only blame the deliberate baud mismatch when it ran.** That item
    /// transmits inside `p5_certify_pair` and nowhere else, so on a rig P5 found
    /// no pair on — a dangling converter, §13's *baseline* — it never executed and
    /// cannot account for a nonzero `frame` count. The unconditional sentence this
    /// guards did exactly that on the 2026-07-27 6.18 report: one dangling FT232R
    /// reading `frame=4`, explained as P5's mismatch item, on a box where no pair
    /// existed to mismatch. `named >= 2` would not have fixed it — two dangling
    /// ports are two named ports and no pair — which is why the input is P5's own
    /// certified-pair count.
    #[test]
    fn p11_blames_the_baud_mismatch_only_when_a_pair_was_certified() {
        let (_, blamed) = p11_verdict(2, 2, false, &[], paired());
        assert!(
            blamed.contains("usually P5's deliberate baud-mismatch item"),
            "a certified pair did not get the mismatch explanation: {blamed}"
        );

        // **Every rig state where the mismatch did not transmit must refuse to
        // blame it** — including a genuinely cross-wired pair whose certificate
        // did not complete, which `certified_pairs` (an *attempt* count) got
        // wrong: `p5_certify_pair` returns early before the mismatch block when a
        // port will not reopen, so an attempt count reinstated the whole defect.
        for rig in [RigFacts::default(), paired_uncertified()] {
            let (status, why) = p11_verdict(1, 1, false, &[], rig);
            assert_eq!(status.label(), "supported");
            assert!(
                !why.contains("usually P5's deliberate baud-mismatch item"),
                "P11 blamed an item that never transmitted ({rig:?}): {why}"
            );
            // Not merely silent — it says the item did not run.
            assert!(
                why.contains("no deliberate baud mismatch"),
                "the absence was dropped rather than reported ({rig:?}): {why}"
            );
        }

        // …and the guidance is scoped to the rig in front of the operator, not to
        // a dangling port they may not have. Three states, three explanations.
        let dangling = p11_verdict(1, 1, false, &[], RigFacts::default()).1;
        assert!(dangling.contains("replug"), "{dangling}");
        let jumpered = p11_verdict(
            1,
            1,
            false,
            &[],
            RigFacts {
                loopbacks: 1,
                ..RigFacts::default()
            },
        )
        .1;
        assert!(
            jumpered.contains("jumpered") && !jumpered.contains("dangling"),
            "a jumpered rig was told to reason about a dangling port: {jumpered}"
        );
        let uncertified = p11_verdict(1, 1, false, &[], paired_uncertified()).1;
        assert!(
            uncertified.contains("cross-wired") && !uncertified.contains("dangling"),
            "a cross-wired rig was told to reason about a dangling port: {uncertified}"
        );

        for (named, opened, denied, missing) in [
            (0usize, 0usize, false, vec![]),
            (1, 0, true, vec![]),
            (1, 1, false, vec!["p: TIOCMGET".to_owned()]),
            (1, 1, false, vec![]),
        ] {
            for rig in [RigFacts::default(), paired(), paired_uncertified()] {
                assert!(
                    !p11_verdict(named, opened, denied, &missing, rig)
                        .0
                        .is_unsupported()
                );
            }
        }
    }

    /// The epoll label is the raw observation P8's cross-kernel diff is read off,
    /// so — exactly like [`revents_label`] — it must name the known bits and must
    /// **not** drop one it has never seen. `flag_names` deliberately lists only
    /// the six it knows; the residual is this function's job.
    #[test]
    fn epoll_labels_name_known_flags_and_surface_unknown_bits() {
        let ready = |events: u32| sys::EpollReady { fd: 7, events };
        assert_eq!(epoll_label(&ready(0)), "none");
        assert_eq!(epoll_label(&ready(sys::EPOLLIN)), "EPOLLIN");
        assert_eq!(
            epoll_label(&ready(sys::EPOLLIN | sys::EPOLLHUP)),
            "EPOLLIN|EPOLLHUP"
        );
        // A bit outside the six named constants still appears, in hex.
        let label = epoll_label(&ready(sys::EPOLLIN | 0x4000_0000));
        assert!(
            label.starts_with("EPOLLIN|0x"),
            "unknown epoll bit dropped: {label}"
        );
    }

    /// P8's spin ratio is a reported number, so it must be stable across two runs
    /// of the same kernel (three decimals) and must not divide by zero when a
    /// phase made no passes at all.
    #[test]
    fn the_spin_ratio_is_stable_and_safe_at_zero() {
        assert_eq!(ratio3(0, 0), 0.0);
        assert_eq!(ratio3(0, 64), 0.0);
        assert_eq!(ratio3(64, 64), 1.0);
        assert_eq!(ratio3(1, 3), 0.333);
        assert_eq!(ratio3(2, 3), 0.667);
    }

    /// P10 must terminate on a bounded fill even if the fd never says `EAGAIN`.
    /// The ceiling is the backstop that makes "it can never hang" a property of
    /// the code rather than a hope about the kernel — proved here against a
    /// bottomless sink (`/dev/null`), which accepts every byte forever.
    #[test]
    fn the_buffer_fill_stops_at_the_ceiling_on_a_bottomless_sink() {
        let sink = open(
            "/dev/null",
            OFlag::O_WRONLY | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .expect("open /dev/null");
        let r = p10_fill(sink.as_raw_fd(), sink.as_raw_fd(), "raw");
        assert!(r.ceiling_hit, "fill did not stop: {}", r.terminal);
        assert_eq!(r.terminal, "ceiling");
        assert!(r.bytes >= P10_CEILING);
        // A bottomless sink is also the limit case the recoverability field exists
        // to name: /dev/null accepts everything and returns none of it, which is
        // precisely the shape an acceptance-only measurement cannot tell from a
        // 4 MiB buffer.
        assert_eq!(r.recovered, 0);
        assert!(r.total() > r.recovered);
        // The ceiling bounds the TOTAL, not each pass: the second pass must find
        // the budget already spent and stop immediately, or a two-pass fill would
        // write twice what the backstop promises.
        assert_eq!(r.settled_bytes, 0, "the ceiling did not bound pass two");
        assert_eq!(r.settled_terminal, "ceiling");
        assert_eq!(r.total(), r.bytes);
    }

    /// The `revents` label is the raw poll observation a cross-kernel diff reads.
    /// It must name every known flag and must **not** silently drop a bit this
    /// table has never seen — an unfamiliar bit on the production kernel is
    /// precisely the surprise the report exists to surface.
    #[test]
    fn revents_labels_name_known_flags_and_surface_unknown_bits() {
        assert_eq!(revents_label(PollFlags::empty()), "none");
        assert_eq!(
            revents_label(PollFlags::POLLIN | PollFlags::POLLHUP),
            "POLLIN|POLLHUP"
        );
        // A bit outside the table (POLLRDBAND here) still appears, in hex.
        let exotic = PollFlags::from_bits_retain(PollFlags::POLLIN.bits() | 0x0080);
        let label = revents_label(exotic);
        assert!(
            label.starts_with("POLLIN|0x"),
            "unknown bit dropped: {label}"
        );
    }

    /// Read outcomes are classified by errno, because "poll said readable and read
    /// said EIO" is a different fact from "poll said readable and read said
    /// EAGAIN" — the first is a hangup, the second a spurious wake.
    #[test]
    fn read_outcomes_are_classified_by_errno() {
        assert_eq!(
            read_class(&std::io::Error::from_raw_os_error(libc::EIO)),
            "EIO"
        );
        assert_eq!(
            read_class(&std::io::Error::from_raw_os_error(libc::EAGAIN)),
            "EAGAIN"
        );
        assert_eq!(
            read_class(&std::io::Error::from_raw_os_error(libc::ENOTTY)),
            format!("errno:{}", libc::ENOTTY)
        );
    }

    /// P7's packet classification decodes the same hex strings P7 emits. The
    /// round-trip is the point: `ioctl_bit` parses text this file formatted, so a
    /// mismatch between the two would report `ioctl_bit_set=false` on a kernel that
    /// plainly sets it — a silent wrong answer in the artifact the 6.18 decision
    /// is made from.
    #[test]
    fn packet_classification_decodes_the_hex_the_probe_emits() {
        let shape = |bytes: &[u8]| ShapeResult {
            bytes: bytes.len() as u64,
            reads: bytes.len() as u64,
            leading_hex: bytes.iter().map(|b| format!("0x{b:02x}")).collect(),
            terminal: "EIO".to_owned(),
            // This test is about the hex classifier alone; the baseline block is
            // inert here and set to what a healthy Linux pair reports.
            baseline: ClientBaseline {
                via_master: true,
                reasserted: true,
                mode: "raw",
                extproc: true,
                footprint_bytes: 0,
            },
        };
        // 0x40 = TIOCPKT_IOCTL (a bare tcsetattr); 0x41 adds TIOCPKT_FLUSHREAD,
        // which is what `stty`'s flushing TCSETSW2/TCSETSF2 leaves.
        let termios = shape(&[sys::TIOCPKT_IOCTL]);
        assert!(termios.ioctl_bit() && !termios.data_packet());
        assert!(shape(&[0x41]).ioctl_bit());
        let data = shape(&[sys::TIOCPKT_DATA]);
        assert!(data.data_packet() && !data.ioctl_bit());
        let nothing = shape(&[]);
        assert!(!nothing.ioctl_bit() && !nothing.data_packet());
    }

    /// `fail_if` only records failures — a passing item leaves the certificate
    /// clean, which is what keeps a good rig at `supported`.
    #[test]
    fn fail_if_records_only_failures() {
        let mut cert = Certificate::new("line");
        cert.fail_if(false, "custom_baud", false);
        cert.fail_if(true, "break", false);
        assert_eq!(cert.failures, vec![fail("break", false)]);
    }

    /// P13's whole reason for existing is that a byte count alone cannot separate
    /// a kernel that discarded promptly from one that waited for a reader first —
    /// both end with nothing recovered. Pin all four quadrants, and pin the pair
    /// that differs *only* in the close duration, or a later simplification that
    /// drops `close_us` from the classifier would pass every other test here.
    #[test]
    fn p13_policy_reads_the_close_duration_not_only_the_byte_count() {
        let slow = P13_WAIT_THRESHOLD_US;
        let fast = P13_WAIT_THRESHOLD_US - 1;
        assert_eq!(p13_policy(fast, 64), "retains");
        assert_eq!(p13_policy(fast, 0), "discards");
        assert_eq!(p13_policy(slow, 64), "waits-then-retains");
        assert_eq!(p13_policy(slow, 0), "waits-then-discards");
        // The conflation the probe undoes: same bytes, different verdict.
        assert_ne!(p13_policy(fast, 0), p13_policy(slow, 0));
        // And the Linux answer this tree measures, so a threshold edit that
        // reclassified the platform of record cannot land silently.
        assert_eq!(p13_policy(7, 64), "retains");
    }

    /// A pty pair whose slave is left in `mode`, returned as the two raw fds P10
    /// fills. The `OwnedFd`s are returned alongside so the caller keeps them alive:
    /// dropping either mid-measurement would close the pair under the fill.
    fn pty_pair_in_mode(raw: bool) -> (PtyMaster, std::os::fd::OwnedFd) {
        let master = new_master().expect("openpt");
        let pts = sys::ptsname(&master).expect("ptsname");
        sys::set_nonblocking(master.as_raw_fd()).expect("master nonblocking");
        let slave = open(
            pts.as_str(),
            OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .expect("open pts");
        sys::set_nonblocking(slave.as_raw_fd()).expect("slave nonblocking");
        if raw {
            set_baseline(&slave).expect("raw baseline");
        } else {
            // The shape a BSD pty lands in when the baseline's momentary slave
            // closes: canonical, echoing — what the daemon never runs.
            let mut t = tcgetattr(&slave).expect("tcgetattr");
            t.local_flags.insert(LocalFlags::ICANON | LocalFlags::ECHO);
            tcsetattr(&slave, SetArg::TCSANOW, &t).expect("cooked");
        }
        (master, slave)
    }

    /// A report must be able to name its own kernel **on every platform it runs on**.
    ///
    /// The three arms are asserted separately because the one that was broken is the
    /// one a Linux box never reaches: `/proc/sys/kernel/osrelease` and
    /// `/etc/os-release` both exist here, so every Linux run looked healthy while
    /// every macOS artifact in `docs/doctor/` recorded `kernel: ""` and
    /// `os: "unknown"` — and marked them `Supported`. Passing the file content in is
    /// what lets the no-`os-release` arm (Darwin's) run *in this suite*, rather than
    /// being trusted until someone next opens a Mac.
    #[test]
    fn the_os_name_survives_a_box_with_no_os_release_file() {
        let uts = nix::sys::utsname::uname().expect("uname(2) must answer on any POSIX box");

        // uname's release is what the `kernel` field now reports. Non-empty here and
        // non-empty on Darwin — the portable property, not a Linux observable.
        assert!(
            !uts.release().is_empty(),
            "uname(2) gave an empty release, so `kernel` would be blank again"
        );

        // The Darwin arm, exercised on Linux: no os-release, so uname carries it.
        let fallback = distro_from(None, Some(&uts));
        assert_ne!(
            fallback, "unknown",
            "with no /etc/os-release the OS name fell back to `unknown` — the exact \
             value every macOS artifact recorded"
        );
        assert!(
            fallback.contains(&*uts.sysname().to_string_lossy())
                && fallback.contains(&*uts.release().to_string_lossy()),
            "the fallback must name the system and its release, got {fallback:?}"
        );

        // A distribution that publishes PRETTY_NAME still wins, so the Linux archive
        // keeps reading exactly as it always has.
        assert_eq!(
            distro_from(
                Some("ID=ubuntu\nPRETTY_NAME=\"Ubuntu 26.04 LTS\"\n".to_owned()),
                Some(&uts)
            ),
            "Ubuntu 26.04 LTS"
        );

        // And `unknown` remains reachable, so it still means "neither source
        // answered" rather than "this is a Mac".
        assert_eq!(distro_from(None, None), "unknown");
    }

    /// The re-assert must leave the master exactly as it found it.
    ///
    /// Fail-first: delete the `read_available` inside `arm_client_baseline` and this
    /// fails on Linux 7.0.0-29 with `arming the client baseline left 1 byte(s) readable
    /// on the master` — precisely the byte that would then be miscounted as the
    /// collapsed session's evidence, turning P7's `a_open_close` from 0 into 1 and
    /// inverting the one shape the §6 detach-release argument relies on leaving nothing
    /// behind.
    ///
    /// Portable by construction: on a kernel that emits no packet for a `tcsetattr` the
    /// footprint is 0 and this still asserts the property (nothing left over), so it is
    /// not a Linux observable standing in for a portable one (§9).
    #[test]
    fn arming_the_client_baseline_leaves_no_footprint_on_the_master() {
        let master = new_master().expect("openpt");
        let pts = sys::ptsname(&master).expect("ptsname");
        let fd = master.as_raw_fd();
        sys::set_nonblocking(fd).expect("nonblocking");
        let via_master = apply_pty_baseline(&master, &pts).expect("baseline");
        sys::set_packet_mode(fd, true).expect("packet mode");
        let mut buf = [0u8; 4096];
        {
            let prime =
                open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("prime");
            std::thread::sleep(PTY_SETTLE);
            let _ = read_available(fd, &mut buf, 64);
            drop(prime);
        }
        std::thread::sleep(PTY_SETTLE);
        let _ = read_available(fd, &mut buf, 64);

        let slave =
            open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("client");
        let armed = arm_client_baseline(fd, &slave, via_master, &mut buf);
        let (left, _, lead, _) = read_available(fd, &mut buf, 64);
        assert_eq!(
            left,
            0,
            "arming the client baseline left {left} byte(s) readable on the master \
             ({}); P6/P7 would count the probe's own tcsetattr as the session's \
             evidence (it drained {} byte(s) of its own)",
            lead.join(" "),
            armed.footprint_bytes
        );
    }

    /// **The two causes of a silent P7 shape are different findings, and only one of
    /// them is a line-discipline problem.**
    ///
    /// This classifier is the whole reason P7's Darwin `degraded` can be read correctly.
    /// Measured on Linux 7.0.0-29 by planting each condition in turn: a baseline with no
    /// EXTPROC silences the *termios* shape (1 → 0) and leaves the *write* shape at 2,
    /// and a fully cooked pair also leaves it at 2 — so a write shape that is **also**
    /// silent cannot be a lost discipline, and no termios repair can move it.
    #[test]
    fn a_silent_write_shape_is_not_a_lost_line_discipline() {
        // Lost baseline: the termios shape goes quiet, the write shape does not.
        assert_eq!(p7_silence_cause(0, 2, false), "extproc-unavailable");
        // The hangup destroyed everything — no termios call can repair this, and P13,
        // not P7, is the instrument. This is the Darwin reading.
        assert_eq!(p7_silence_cause(0, 0, false), "hangup-destroys-evidence");
        assert_eq!(p7_silence_cause(0, 0, true), "hangup-destroys-evidence");
        // The genuine latch-coverage finding: baseline intact, data survives, packet
        // does not.
        assert_eq!(p7_silence_cause(0, 2, true), "latch-uncovered");
        assert_eq!(p7_silence_cause(1, 2, true), "covered");
        // And the discrimination itself, so a classifier collapsed to one answer fails
        // here rather than passing every single-valued assertion above.
        assert_ne!(
            p7_silence_cause(0, 2, false),
            p7_silence_cause(0, 0, false),
            "a lost discipline and a destructive hangup must not classify alike"
        );
    }

    /// The detector behind P10's `slave_termios_mode`, planted in both spellings.
    ///
    /// Both directions are asserted because a `termios_mode` stubbed to a constant
    /// — the exact simplification this guards against — passes any single-valued
    /// test. `assert_ne!` pins the discrimination itself, so a mutation that
    /// collapses the two answers fails here even if both spellings drift.
    #[test]
    fn termios_mode_tells_the_daemons_baseline_from_a_cooked_pty() {
        let (_m_raw, s_raw) = pty_pair_in_mode(true);
        let (_m_cooked, s_cooked) = pty_pair_in_mode(false);
        assert_eq!(termios_mode(&s_raw), "raw");
        assert_eq!(termios_mode(&s_cooked), "cooked");
        assert_ne!(termios_mode(&s_raw), termios_mode(&s_cooked));
    }

    /// **The recheck must actually recheck.** Its three fields are only meaningful in
    /// order: the refill has to start from an *empty* peer, the partial drain has to
    /// take the amount it asked for, and the top-up has to find the room that drain
    /// freed. A recheck that ran in the wrong order reports plausible zeros.
    ///
    /// Deliberately does **not** assert the sign of
    /// `room_republished_minus_room_freed`. Linux measures a positive number and the
    /// Darwin pre-registration in notes §3.44 is 0; pinning either here would make the
    /// guard assert a prediction about a kernel this box is not — §9's proxy in space
    /// in its purest form. The prediction lives in the notes, where being wrong is a
    /// record.
    #[test]
    fn p10_recheck_measures_republished_room_rather_than_nothing() {
        let (m, s) = pty_pair_in_mode(true);
        let f = p10_fill(m.as_raw_fd(), s.as_raw_fd(), "raw");
        assert!(
            f.recheck.refilled > 0,
            "the recheck refilled 0 bytes into a peer that `recovered`'s drain had just \
             emptied — it ran before that drain, or not at all"
        );
        let expected = f.recheck.refilled.min(P10_RECHECK_DRAIN);
        assert_eq!(
            f.recheck.drained_again, expected,
            "the recheck's partial drain took {} bytes where {expected} were available \
             to take, so the room it then measures was never freed",
            f.recheck.drained_again
        );
        assert!(
            f.recheck.topped_up > 0,
            "{} bytes were handed back to the peer and the kernel then accepted none of \
             them — the top-up pass did not run",
            f.recheck.drained_again
        );
    }

    /// **Acceptance is not delivery, and P10 could not see the difference.**
    ///
    /// Filling hostward (master→slave) against a slave nobody reads: raw hands
    /// every accepted byte back, cooked hands back none of them — measured on
    /// Linux 7.0.0-29 at ~13.8 KiB fully recoverable against ~23.5 KiB with
    /// nothing recoverable. Before `bytes_recovered_by_peer` existed the two were
    /// indistinguishable in the report, which is how a cooked-pty measurement
    /// could be read as this kernel's buffer depth.
    ///
    /// Asserted as a *relation* between the two modes rather than against either
    /// figure: the depths move by a chunk run to run and differ by kernel, but a
    /// raw pty conserving what it took while a cooked one does not is the property
    /// the field exists to report, and it holds wherever a pty has a line
    /// discipline at all.
    #[test]
    fn p10_recoverability_separates_a_deep_buffer_from_a_black_hole() {
        let (m_raw, s_raw) = pty_pair_in_mode(true);
        let raw = p10_fill(m_raw.as_raw_fd(), s_raw.as_raw_fd(), "raw");
        assert!(
            raw.total() > 0,
            "a raw pty accepted nothing hostward — the fill never ran"
        );
        assert_eq!(
            raw.total() - raw.recovered,
            0,
            "a raw pty lost bytes it accepted: {} accepted, {} recovered",
            raw.total(),
            raw.recovered
        );

        let (m_cooked, s_cooked) = pty_pair_in_mode(false);
        let cooked = p10_fill(m_cooked.as_raw_fd(), s_cooked.as_raw_fd(), "cooked");
        assert!(
            cooked.total() > 0,
            "a cooked pty accepted nothing hostward — the fill never ran"
        );
        assert!(
            cooked.total() > cooked.recovered,
            "a cooked pty returned everything it accepted ({} of {}), so this guard \
             no longer discriminates and the recoverability field is untested",
            cooked.recovered,
            cooked.total()
        );
    }
}
