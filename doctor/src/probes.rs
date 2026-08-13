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
//! **"The probe itself failed" is `degraded` too, never `skipped`** ([`measurement_failed`]):
//! `skipped` is the one word every conditional clause in `expectations/*.jq`
//! exempts, so an error path wearing it exempts itself from exactly the clauses
//! that exist to notice the measurement is missing (§13).

use std::collections::BTreeMap;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{ControlFlags, LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

use crate::report::{EnvCheck, Probe, Status};
use serial_nexus_sys as sys;
use serial_nexus_sys::RtsCtsOutcome;

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
/// That is not a detail. Measured on Linux 7.0.0-29, filling a pty hostward with
/// the same bytes, the two modes differ by an **order of magnitude in what the peer
/// can recover**, and in opposite directions: raw accepts less and returns all of
/// it, cooked accepts more and returns none. Same kernel, same probe, opposite
/// answers — so a depth reported without its mode is not a cross-kernel
/// measurement, it is two measurements wearing one name.
///
/// The relation is stated without figures on purpose. The pair this sentence used
/// to quote (~13.8 KiB raw / ~23.5 KiB cooked) came from a session scratchpad and
/// is backed by **no committed `docs/doctor/` artifact** — the raw half does not
/// even agree with the committed Linux capture, which reads 13824–15360 bytes per
/// direction (notes §3.34's filing; the artifact-backed figures live in design
/// §15.46). `expectations/linux.jq`'s P10 paragraph is the model form: the
/// relation, the kernel, no numbers. What is *proven* here is the relation itself,
/// by `p10_recoverability_separates_a_deep_buffer_from_a_black_hole`, which
/// asserts it numberlessly.
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
                        "POLLIN goes quiet after the last close on this kernel ({} passes, {} with POLLIN, none readable-with-nothing-to-read): an ungated `closed`-only last-close arm would NOT spin on the hangup alone here, so pty.rs's `saw_session` latch is not what holds the anti-spin argument up on this kernel.{rearm} This is a per-kernel reading — §13 forbids acting on it until the other kernel of record (6.18 or 7.0, whichever this run is not) reports the same numbers, so diff this block before simplifying anything.{discipline}",
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
                "A collapsed termios-only session leaves {} byte(s) readable past the hangup (leading {}, ioctl bit {}): pty.rs's widened last-close latch arms on it, so an `stty`/health-check/scripted client that opens, reconfigures and closes inside one poll gap still runs detach-release (§6). Diff this against the other kernel of record (6.18 or 7.0, whichever this run is not) before trusting the coverage there.{discipline}",
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

/// The daemon's own idle pty cadence (`serial_nexus_daemon::runtime::IDLE_POLL` is
/// 5 ms; `ACTIVE_POLL` is 200 µs). The paced windows below run at exactly this
/// interval, because "an idle master posts no edge" is a claim about the loop
/// `nodes/pty.rs` actually runs, and a claim about 200 back-to-back syscalls is a
/// different one. Both are kept, and the pair discriminates: an edge that appears
/// only in the paced window is time-driven, one that appears only in the tight
/// window is syscall-driven, and until now nothing here could tell those apart.
const P12_PACED_PAUSE: Duration = Duration::from_millis(5);

/// Passes per paced window — P6's count, so the two probes' windows are the same
/// size and directly comparable (P6: 64 passes at 2 ms, 163 ms of wall clock).
/// 64 × 5 ms measures 324816/324914/324476 µs on an idle Linux box, and there are
/// two of them: ~0.65 s per doctor run, named because it is the whole cost of
/// this fix.
const P12_PACED_PASSES: u32 = 64;

/// The historical tight window, unchanged in count and shape. It is what every
/// committed `docs/doctor/` report's `idle_edges_in_200_passes` was measured with,
/// and re-pacing it would silently redefine a field six artifacts already carry
/// (§16.13). What it lacked was a wall clock: 200 passes over a hung-up master
/// cost **98 µs** on Linux 7.0.0-29, which is why the witness below is reported in
/// microseconds and not in P6's milliseconds — `elapsed_ms` here would print 0.
const P12_TIGHT_PASSES: u32 = 200;

/// How many paced passes the positive control gives the boundary this probe
/// produced on purpose. Generous by design: on the kernel this mechanism exists
/// for, the edge is posted by `close(2)` itself, so anything past the first pass
/// is already a finding, and 16 × 5 ms = 80 ms is the outer bound on saying
/// "the latch is deaf" rather than "the latch was slow".
const P12_CONTROL_PASSES: u32 = 16;

/// One window of reader-shaped passes and what the latch did during it.
#[derive(Default)]
struct EdgeWindow {
    passes: u32,
    edges: u64,
    elapsed_us: u64,
    pause_us: u64,
    /// Passes where `poll(2)` reported anything at all, and how each pass's
    /// `read(2)` ended. **The witness that the loop ran.** A window reporting zero
    /// edges because it never executed is the vacuous verdict §9 names, and the
    /// shipped probe could not tell that from a quiet kernel.
    poll_event_passes: u32,
    reads: BTreeMap<String, u32>,
}

impl EdgeWindow {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "passes": self.passes,
            "edges": self.edges,
            "elapsed_us": self.elapsed_us,
            "pass_pause_us": self.pause_us,
            "poll_event_passes": self.poll_event_passes,
            "read_outcomes": self.reads,
        })
    }
}

/// Run `passes` reader-shaped passes over `fd` and count what the latch posted.
/// The pass body is `nodes/pty.rs`'s in its order — poll `POLLIN|POLLHUP`, read,
/// then ask the latch — because that order's side effects are what could re-arm
/// the knote, and a probe that asked the latch first would be measuring a
/// sequence the daemon never performs.
fn p12_window(fd: RawFd, latch: &sys::SessionLatch, passes: u32, pause: Duration) -> EdgeWindow {
    let mut w = EdgeWindow {
        pause_us: pause.as_micros() as u64,
        ..EdgeWindow::default()
    };
    let mut buf = [0u8; 256];
    let start = Instant::now();
    for _ in 0..passes {
        let re = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        w.passes += 1;
        if !re.is_empty() {
            w.poll_event_passes += 1;
        }
        let class = match sys::read_fd(fd, &mut buf) {
            Ok(0) => "eof".to_owned(),
            Ok(_) => "bytes".to_owned(),
            Err(e) => read_class(&e),
        };
        *w.reads.entry(class).or_default() += 1;
        if latch.took_edge() {
            w.edges += 1;
        }
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    w.elapsed_us = start.elapsed().as_micros() as u64;
    w
}

/// The anti-spin half, and the two controls that make its zero mean something.
///
/// All four run on **one** master through **one** latch, and that is the whole
/// point: the three shape trials above each build their own pair and their own
/// `SessionLatch`, so a latch that went deaf on *this* pair is invisible to them.
/// `watch()` deliberately swallows the registration edge on a master that is
/// already hung up, which is exactly the state the idle windows then measure — so
/// "0 edges from an already-EOF knote" is a reading that a broken instrument and a
/// quiet kernel produce identically, and only a boundary produced on purpose,
/// afterwards, on the same latch, separates them.
struct P12Idle {
    /// The historical window, unchanged: 200 back-to-back passes.
    tight: EdgeWindow,
    /// The same question at the daemon's own idle cadence.
    paced: EdgeWindow,
    /// **Negative control.** The same window with a slave *open* — a client
    /// present, no boundary. An edge here fires §6 detach-release mid-session and
    /// hands away the write lock of a client that never left: the mirror image of
    /// the spin, and nothing measured it.
    live: EdgeWindow,
    /// **Positive control, on the latch instance that produced the zeros above.**
    control_edge: bool,
    /// Which control pass took the edge (1 = the first), `None` if none did.
    control_pass: Option<u32>,
    /// Wall clock from the close to the edge (or to giving up).
    control_us: u64,
}

/// See [`P12Idle`]. Sequencing is load-bearing and is stated here so an editor
/// cannot reorder it silently: both idle windows run while nothing is attached,
/// the live window runs with a slave open (and rule 2's `discard` after this
/// process's own open), and the control runs last, because it ends the session
/// and cannot be undone.
fn p12_idle() -> anyhow::Result<P12Idle> {
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

    let tight = p12_window(fd, &latch, P12_TIGHT_PASSES, Duration::ZERO);
    let paced = p12_window(fd, &latch, P12_PACED_PASSES, P12_PACED_PAUSE);

    // A client attaches. The open is this process's own doing, so `SessionLatch`
    // rule 2 applies before anything is counted.
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    std::thread::sleep(PTY_SETTLE);
    latch.discard();
    let live = p12_window(fd, &latch, P12_PACED_PASSES, P12_PACED_PAUSE);

    // ...and leaves. This is the control: the same latch, the same master, a
    // boundary this probe knows happened.
    let closed_at = Instant::now();
    drop(slave);
    let mut buf = [0u8; 256];
    let mut control_pass = None;
    for i in 1..=P12_CONTROL_PASSES {
        let _ = sys::poll_ready(fd, PollFlags::POLLIN | PollFlags::POLLHUP);
        let _ = sys::read_fd(fd, &mut buf);
        if latch.took_edge() {
            control_pass = Some(i);
            break;
        }
        std::thread::sleep(P12_PACED_PAUSE);
    }

    Ok(P12Idle {
        tight,
        paced,
        live,
        control_edge: control_pass.is_some(),
        control_pass,
        control_us: closed_at.elapsed().as_micros() as u64,
    })
}

/// The facts P12's verdict turns on, extracted so the decision is a pure function
/// of numbers. This is the answer to a real constraint: the mechanism is inert on
/// Linux, so the platform of record cannot produce a single row of this table —
/// but it can, and now does, regression-test the decision made from it (§9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct P12Facts {
    /// Did the termios-only shape post an edge? `None` = that trial errored.
    termios_edge: Option<bool>,
    /// Edges from a hung-up, unattached master. Anything but 0 is the spin.
    idle_edges: u64,
    /// Edges while a client was attached and idle. Anything but 0 releases a live
    /// client's write lock.
    live_session_edges: u64,
    /// Did the same latch post the boundary this probe produced on purpose?
    control_edge: bool,
    /// Passes and wall clock the two idle windows actually covered.
    idle_passes: u32,
    idle_elapsed_us: u64,
    /// `false` where the windows did not run at all.
    measured: bool,
}

/// P12's verdict. Pure; touches no fd.
fn p12_verdict(f: P12Facts) -> (Status, String) {
    if !f.measured {
        return (
            Status::Degraded,
            "The session-edge measurement did not complete, so which mechanism carries detach-release on this kernel is unknown. Read P7: if it reports readable evidence, the packet route is intact regardless (§6, §15.39).".to_owned(),
        );
    }
    // The dangerous direction still gets said first: it releases a lock nobody's
    // client took, on every pass.
    if f.idle_edges > 0 {
        return (
            Status::Degraded,
            format!(
                "An idle, hung-up master posted {} session edge(s) across {} reader-shaped passes covering {} us of wall clock. That re-fires `pty.rs`'s last-close handler on a pair no client has touched — releasing a write lock the operator took, and burning the runtime thread doing it. `SessionLatch`'s discard sites (§15.39) or this kernel's `EV_CLEAR` semantics have changed; re-check both before trusting detach-release here.",
                f.idle_edges, f.idle_passes, f.idle_elapsed_us
            ),
        );
    }
    if f.live_session_edges > 0 {
        return (
            Status::Degraded,
            format!(
                "A master with a client ATTACHED and idle posted {} session edge(s). `pty.rs` reads that edge as \"a client left\", so §6 detach-release fires mid-session and hands the write lock away under a client that never went anywhere. The idle-hangup count is 0, so this is not the spin shape but its mirror image, and nothing measured it before.",
                f.live_session_edges
            ),
        );
    }
    if f.idle_passes == 0 || f.idle_elapsed_us == 0 {
        return (
            Status::Degraded,
            "The anti-spin windows reported 0 edges over 0 passes, or across 0 us of wall clock. That is not evidence of anything: a loop that did not execute cannot observe an edge (§9). Read the window blocks above before reading any count here.".to_owned(),
        );
    }
    if !f.control_edge {
        return (
            Status::Degraded,
            format!(
                "The anti-spin windows are silent AND SO IS THE INSTRUMENT. After {} passes over {} us reporting 0 edges, this probe closed a slave on the same master through the same latch, and the latch reported nothing for that boundary either. A zero from a latch that cannot post an edge says nothing about spin and nothing about §6 detach-release: read it as `unmeasured`, not as `quiet`. Read P7 beside this — if it is `supported`, the retained packet is carrying detach-release here regardless.",
                f.idle_passes, f.idle_elapsed_us
            ),
        );
    }
    match f.termios_edge {
        Some(true) => (Status::Supported, format!(
            "A collapsed termios-only session posts a session-boundary edge; an idle hung-up master posts NONE across {} reader-shaped passes covering {} us of wall clock, one window of them paced at the daemon's own 5 ms `IDLE_POLL`; a master with a client attached and idle posts none either; and the same latch DID post the boundary this probe then produced on purpose, which is what makes those zeros a measurement rather than an inert instrument. `pty.rs`'s `saw_session` latch is armed by the edge where this kernel keeps no readable evidence, so detach-release covers the `stty`/health-check/scripted shape (§6, §15.39). This is the mechanism `p9_pty_collapse` asserts end to end here.",
            f.idle_passes, f.idle_elapsed_us)),
        Some(false) => (
            Status::Degraded,
            "A collapsed termios-only session posts NO session-boundary edge on this kernel, and the failure is specific rather than instrumental: the same mechanism did post the control boundary this probe produced on purpose. With P7 also reporting nothing readable, `pty.rs`'s last-close latch has neither mechanism to arm on, so a client that opens, calls tcsetattr and closes inside one poll gap keeps its write lock until the node is removed or another writer steals it (§6) — silently. Read P7 beside this: if *it* is `supported`, the packet is carrying detach-release here and this is only an unused second route.".to_owned(),
        ),
        None => (
            Status::Degraded,
            "The termios-only trial errored, so the shape §6 detach-release depends on is unmeasured here even though the anti-spin windows and the control both ran. Read P7: if it reports readable evidence, the packet route is intact regardless (§6, §15.39).".to_owned(),
        ),
    }
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
/// **Five things are reported and the last two are what make the first three
/// readable.** Whether the **termios-only** shape posts an edge is the property
/// `p9_pty_collapse` asserts end to end. Whether an **idle** master posts one is
/// the anti-spin property: a non-zero count there means the last-close handler
/// re-fires forever *and* releases a lock no client ever took, which is worse than
/// the leak it fixes — and it is now asked twice, once back to back and once paced
/// at the daemon's own 5 ms `IDLE_POLL`, each with the wall clock it covered. The
/// **bare open/close** shape is reported because Darwin covers it where Linux does
/// not — an asymmetry worth seeing in a diff rather than discovering. Then the two
/// that were missing: a **negative control** (the same idle window with a client
/// attached, where an edge would fire §6 detach-release mid-session), and a
/// **positive control** — a slave opened and closed on the *same* master through
/// the *same* latch, after the zeros above. Without it, "0 edges" is a reading a
/// broken instrument and a quiet kernel produce identically, and `supported` off
/// it is the vacuous verdict §9 names.
pub fn p12_session_edge() -> Probe {
    let mut p = Probe::new(
        "P12",
        "session-boundary edge on a pty master",
        "Does an edge latch report a collapsed client session that left nothing readable on the master, and does it stay silent while idle?",
    );

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

    let idle = p12_idle();
    let facts = match &idle {
        Ok(i) => {
            p = p
                // Unchanged key, unchanged loop, unchanged meaning: six committed
                // artifacts carry it and a diff against them must stay lawful.
                .observe("idle_edges_in_200_passes", i.tight.edges)
                .observe("idle_window_tight", i.tight.observations())
                .observe("idle_window_paced", i.paced.observations())
                .observe("live_session_window", i.live.observations())
                .observe("control_session_edge", i.control_edge)
                .observe(
                    "control_session_edge_pass",
                    serde_json::json!(i.control_pass),
                )
                .observe("control_session_edge_us", i.control_us);
            P12Facts {
                termios_edge,
                idle_edges: i.tight.edges + i.paced.edges,
                live_session_edges: i.live.edges,
                control_edge: i.control_edge,
                idle_passes: i.tight.passes + i.paced.passes,
                idle_elapsed_us: i.tight.elapsed_us + i.paced.elapsed_us,
                measured: true,
            }
        }
        Err(e) => {
            p = p.observe("idle_windows", format!("probe error: {e}"));
            P12Facts {
                termios_edge,
                idle_edges: 0,
                live_session_edges: 0,
                control_edge: false,
                idle_passes: 0,
                idle_elapsed_us: 0,
                measured: false,
            }
        }
    };

    // The inert arm is not a failure and its wording does not move — but it now
    // reports the windows it ran, and that is deliberate. Where `SessionLatch` is
    // inert, `control_session_edge: false` beside a full set of executed passes is
    // the arm demonstrating itself inert on the one platform that can do so
    // cheaply, which is the negative control the kernel that DEPENDS on this
    // mechanism cannot provide for itself (§9). It also gives the Linux artifact a
    // P12 subtree where it had a hole, which is what §7 asks of a cross-kernel
    // instrument.
    if !cfg!(target_os = "macos") {
        return p.verdict(
            Status::skipped("serial-nexus-sys's SessionLatch is inert on this platform"),
            &format!(
                "The session boundary is carried by the retained `TIOCPKT_IOCTL` packet here, which P7 measures — nothing is untested, only unmeasurable by this route (§15.39, §13). The windows above ran anyway and are reported: `control_session_edge: {}` beside {} executed passes over {} us is this platform's inert arm proving itself inert, and a Linux report where that field read `true` would mean the latch had grown a second implementation nobody measured.",
                facts.control_edge, facts.idle_passes, facts.idle_elapsed_us
            ),
        );
    }

    let (status, consequence) = p12_verdict(facts);
    p.verdict(status, &consequence)
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
    /// As `NoReader`, but **a second fd on the same slave is held open across the
    /// writer's close**, and released only after the master has drained.
    ///
    /// **This shape measures the premise an entire test-harness architecture rests
    /// on, and nothing measured it before.** notes §3.56 converted seven guards to
    /// hold a harness-opened slave fd across the observation, on the argument that
    /// *every kernel attaches its close-time work to the **last** close of the
    /// pty* — XNU runs `ptsclose` → `ttylclose` → `ttywflush` at reference count
    /// zero, Linux charges `discarded_at_last_close` at the same edge — so a
    /// witness fd is exactly as strong as holding the writing client open. That
    /// argument was read out of two kernels' source and never measured: P13's
    /// other three shapes all use a **single** slave fd, so none of them can see a
    /// reference count at all.
    ///
    /// Read it against `a_no_reader_blocking_slave`, which is the same session
    /// without the witness. On a kernel that **retains** (Linux) the two agree and
    /// the shape is a control proving itself inert — the same role P12's windows
    /// play there. On a kernel that **discards at last close** (Darwin) they must
    /// differ: this one recovers the payload and that one does not. A kernel where
    /// they *do not* differ is one where a held fd buys nothing, and seven guards
    /// in `itest` would need their argument rewritten rather than their code.
    NoReaderSecondFdHeld,
    /// As `NoReader`, but **a reader arrives while the kernel is inside its
    /// close-wait** — after `close(2)` has been entered on the slave and before it
    /// has returned.
    ///
    /// **This is the shape the failing macOS test inhabits, and no other shape
    /// covers it** (plan §18 item 22; notes §3.29). The four above all fix the
    /// reader's state *before* the close: `b` drains and then closes, `a`/`c`/`d`
    /// never read at all. On a kernel that returns from the close immediately
    /// (Linux `retains`, or any kernel that discards promptly) that distinction is
    /// invisible, because there is no window to arrive in. On one that **waits**
    /// — XNU's `ptsclose` → `ttylclose` → `ttywflush` → `ttywait` parks up to
    /// `t_timeout`, measured at 600104 µs on Darwin 24.6.0 with no reader — the
    /// window is ~0.6 s wide and the daemon's reader cadence (200 µs–5 ms) lands
    /// inside it on every healthy run. What that arrival buys is precisely what
    /// notes §3.29 could only *predict*: whether the close returns early with the
    /// bytes recovered, or whether it still pays the whole timeout.
    ///
    /// **The instrument states its own applicability**, because a reader that
    /// arrives after the close has already returned measured nothing. The reader
    /// is a thread already spinning on an `AtomicBool` when the close is entered,
    /// so the arrival is a cache-line and a syscall away rather than a thread
    /// spawn away, and both timestamps come off one `Instant` epoch:
    /// `arrived_before_close_returned` says whether the race was won, and
    /// `reading` says what the row licenses when it was not. A kernel that never
    /// waits will win it or lose it by microseconds and either answer is honest —
    /// what the row must never do is report an arrival it did not make.
    ReaderArrivesDuringCloseWait,
}

impl CloseShape {
    fn key(self) -> &'static str {
        match self {
            CloseShape::NoReader => "a_no_reader_blocking_slave",
            CloseShape::ReaderDrains => "b_reader_drains_before_close",
            CloseShape::NoReaderNonblocking => "c_no_reader_nonblocking_slave",
            CloseShape::NoReaderSecondFdHeld => "d_no_reader_second_fd_held",
            CloseShape::ReaderArrivesDuringCloseWait => "e_reader_arrives_during_close_wait",
        }
    }
}

/// When the arriving reader of [`CloseShape::ReaderArrivesDuringCloseWait`] got
/// there, relative to the close it was racing.
///
/// Every field is measured against one `Instant` epoch taken before the reader
/// thread is armed, so the two threads' readings are comparable without assuming
/// anything about clock domains.
struct ArrivalTiming {
    /// µs from the instant the close was **entered** to the reader's first
    /// `read(2)` on the master. Not the read's completion: the question is when
    /// the reader showed up, and a kernel that hands the bytes over instantly and
    /// one that parks are both "arrived".
    first_read_offset_us: u64,
    /// µs from the same instant to the close's **return**. Equal to the shape's
    /// `close_microseconds`; carried inside the block too so the ordering can be
    /// re-derived from the arrival cells alone.
    close_returned_us: u64,
    /// Did the reader's first read enter before the close returned? The
    /// discriminator, and the whole reason the block carries a `reading`.
    arrived_before_close_returned: bool,
    /// What the arriving reader recovered, packet-mode control bytes already
    /// subtracted.
    bytes_recovered: u64,
    /// How that drain ended (`EAGAIN` / `EIO` / `eof` / …).
    terminal: String,
    /// µs the arming handshake cost — the reader signalling that it is spinning,
    /// before the close is entered. A witness on the instrument rather than on
    /// the kernel: an arming cost near the close's own duration would mean the
    /// race was decided by thread scheduling and not by the kernel's policy.
    arm_us: u64,
}

impl ArrivalTiming {
    /// What this row licenses, in the instrument's own words (§13's
    /// self-testimony rule 3: an instrument states what its reading does *not*
    /// license).
    fn reading(&self) -> &'static str {
        if self.arrived_before_close_returned {
            "arrived-inside-the-close-window"
        } else {
            "lost-the-race"
        }
    }

    fn does_not_license(&self) -> &'static str {
        if self.arrived_before_close_returned {
            "the bytes and the close time here belong to a reader that arrived INSIDE the close; they say nothing about a reader that never arrives — that is shape `a`"
        } else {
            "the close returned before the reader's first read, so this row observed no arrival inside the close window and licenses NOTHING about one. On a kernel whose close does not wait there is no window to arrive in, which is a legitimate reason to read this and not a defect."
        }
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "arrived_before_close_returned": self.arrived_before_close_returned,
            "reading": self.reading(),
            "does_not_license": self.does_not_license(),
            "reader_first_read_offset_us": self.first_read_offset_us,
            "close_returned_us": self.close_returned_us,
            "reader_terminal_read": self.terminal,
            "reader_arm_microseconds": self.arm_us,
        })
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
    /// Bytes the *arriving* reader recovered — 0 for every shape but `e`, the only
    /// one that has a second reader at all.
    ///
    /// Deliberately **not** called "recovered during the close": that would be a
    /// wrong cell on exactly the run where the row is least trustworthy, since a
    /// reader that lost the race recovered its bytes *after* the close returned.
    /// `reader_arrival.arrived_before_close_returned` is the cell that says which
    /// of the two happened, and it is the one to read first.
    bytes_during: u64,
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
    /// Present only for [`CloseShape::ReaderArrivesDuringCloseWait`]: when the
    /// arriving reader got there, and what it recovered.
    arrival: Option<ArrivalTiming>,
}

impl CloseResult {
    /// Everything the master got back, wherever in the close it got it.
    ///
    /// Split out rather than spelled at each site because shape `e` recovers its
    /// payload in a *third* place — during the close — and a total that summed
    /// only before and after would publish `bytes_lost: 64` beside a reader that
    /// recovered all 64. A wrong cell is worse than a missing one (§13).
    fn recovered(&self) -> u64 {
        self.bytes_before + self.bytes_during + self.bytes_after
    }

    fn observations(&self, written: u64) -> serde_json::Value {
        let recovered = self.recovered();
        let mut v = serde_json::json!({
            "bytes_written_by_slave": written,
            "bytes_recovered_before_close": self.bytes_before,
            "bytes_recovered_after_close": self.bytes_after,
            "bytes_recovered_total": recovered,
            "bytes_lost": written.saturating_sub(recovered),
            "close_microseconds": self.close_us,
            "terminal_read": self.terminal,
            "slave_termios_mode": self.slave_mode,
            "baseline_packet_bytes": self.baseline_packet_bytes,
        });
        // The arrival cells exist only on the shape that has an arriving reader.
        // Stamping `bytes_recovered_by_arriving_reader: 0` on the other four would add a
        // leaf path to every one of them for a number that is structurally zero —
        // cost paid by every cross-era intersection, information bought: none.
        if let Some(a) = &self.arrival
            && let Some(o) = v.as_object_mut()
        {
            o.insert(
                "bytes_recovered_by_arriving_reader".to_owned(),
                serde_json::json!(self.bytes_during),
            );
            o.insert("reader_arrival".to_owned(), a.observations());
        }
        v
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
/// How long either side of shape `e`'s arming handshake will spin before giving
/// up and letting the timestamps report the loss. Two orders of magnitude above
/// Darwin's measured 600 ms close-wait, so a legitimate wait is never cut short,
/// and finite so a wedged close cannot turn this diagnostic into a spinner.
const P13_ARRIVAL_SPIN_CAP: Duration = Duration::from_secs(60);

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
/// only where the answer is interesting. Shape `e` pays *nothing extra there* and
/// that is its point: a reader inside the window is what a waiting kernel is
/// waiting for.
///
/// **The five shapes, and why the fifth was added** (plan §18 item 22). Four of
/// them fix the reader's state before the close — `b` drains first, `a`/`c`/`d`
/// never read — so between them they cannot produce a reader that arrives *while*
/// the kernel is inside its close-wait. That is the shape notes §3.29's
/// unexplained macOS red inhabits: the daemon's reader runs a 200 µs–5 ms cadence
/// against a ~600 ms Darwin close-wait, so on every healthy run it arrives inside,
/// and the question the record could only answer by prediction is whether the
/// arrival ends the wait with the bytes recovered. `e` measures it, and reports
/// whether it managed to arrive at all rather than assuming it did.
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
        CloseShape::NoReaderSecondFdHeld,
        CloseShape::ReaderArrivesDuringCloseWait,
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

    let recovered = bare.recovered();
    // **The reference-count reading, and it is the comparison a reader would not
    // otherwise make.** Shape `d` is shape `a` with a second fd on the same pts
    // held across the writer's close, so the pair isolates the *last*-close edge
    // that notes §3.56's harness architecture rests on. Which cell moves is the
    // kernel's to decide and is exactly what a cross-kernel diff wants: on a
    // kernel that retains, the byte counts agree and the **terminal read** is the
    // discriminator (Linux 7.0.0-29 reads `EIO` with one fd and `EAGAIN` with the
    // witness held — the hangup itself is deferred); on a kernel that discards at
    // last close, the byte counts must differ, and a kernel where *neither* moves
    // is one where a held fd buys nothing and seven `itest` guards need a new
    // argument rather than new code.
    let held = results
        .iter()
        .find(|(s, _)| matches!(s, CloseShape::NoReaderSecondFdHeld))
        .map(|(_, r)| r);
    let ref_count = match held {
        Some(h) => format!(
            " **The last-close reference count** is measured too, by holding a second fd on the same pts across the writer's close (`d_no_reader_second_fd_held` against `a_no_reader_blocking_slave`): {} of {P13_PAYLOAD} byte(s) survive with the witness held against {} without it, and the terminal read is `{}` against `{}`. Compare the two rows rather than reading either alone — that pair is the whole measurement, and if *neither* the bytes nor the terminal move on some kernel, a held fd buys nothing there and the harness rule that depends on it (notes §3.56) is resting on nothing.",
            h.recovered(),
            bare.recovered(),
            h.terminal,
            bare.terminal,
        ),
        None => String::new(),
    };

    // **The arriving-reader reading** (plan §18 item 22). Shape `e` is shape `a`
    // with a reader that shows up *inside* the close rather than before it or
    // never — the shape notes §3.29's unexplained macOS red actually inhabits, and
    // the one none of the four above can produce. Its sentence leads with whether
    // the arrival happened at all, because a row that lost the race licenses
    // nothing and must not be read as an answer (§13, §15.49).
    let arriving = results
        .iter()
        .find(|(s, _)| matches!(s, CloseShape::ReaderArrivesDuringCloseWait))
        .and_then(|(_, r)| r.arrival.as_ref().map(|a| (r, a)));
    let arrival_note = match arriving {
        Some((r, a)) if a.arrived_before_close_returned => format!(
            " **A reader that arrives *during* the close** (`e_reader_arrives_during_close_wait`) got its first `read(2)` in {} µs after the close was entered and before it returned, recovered {} of {P13_PAYLOAD} byte(s) there, and the close then took {} µs against {} µs with no reader at all. On a kernel whose close waits for a reader, that difference is the whole finding: it is the daemon's own reader cadence (200 µs–5 ms) landing inside the wait, and a run where the close still pays its full timeout means the reader stalled — a daemon-side event, not a kernel one (notes §3.29).",
            a.first_read_offset_us, a.bytes_recovered, r.close_us, bare.close_us,
        ),
        Some((_, a)) => format!(
            " **The arriving-reader shape did not observe an arrival** (`e_reader_arrives_during_close_wait`): the close returned {} µs after it was entered and the reader's first `read(2)` came {} µs after that, so nothing in that row licenses a claim about a reader inside the close window. That is the expected reading on a kernel whose close does not wait — there is no window to arrive in — and it is reported rather than suppressed, because a shape that silently stopped asking its question is the failure this probe set exists to avoid (§13).",
            a.close_returned_us,
            a.first_read_offset_us.saturating_sub(a.close_returned_us),
        ),
        None => String::new(),
    };
    let waited = bare.close_us >= P13_WAIT_THRESHOLD_US;
    let policy = p13_policy(bare.close_us, recovered);
    let drained_note = match drained {
        Some(d) => format!(
            " With a master that drains before the close, {} of {P13_PAYLOAD} byte(s) are recovered and the close takes {} µs — the healthy-reader case, and the one the daemon is in.",
            d.recovered(),
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
                "This kernel **{policy}** bytes a pts client wrote but the master never read: with no reader, {recovered} of {P13_PAYLOAD} byte(s) survive the last close and `close(2)` takes {} µs (terminal read `{}`).{drained_note} Numbers, not a verdict — every policy is legitimate and the daemon is correct under each (§7.2 drains before finalizing a close, §5 accounts what it reads). Read it for two things: a cross-kernel diff, and the reason a harness reads a byte counter while its client is still open rather than after (notes §3.29). A `waits-then-*` kernel additionally means a lost byte implies a reader stalled for the whole timeout, not a lost microsecond race.{ref_count}{arrival_note}",
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

    // The witness: a *second* fd on the same pts, opened before the writer closes
    // and released after the master has drained. It is opened after the baseline
    // re-assert so it cannot perturb it, and it is deliberately not written
    // through — the claim under test is about the reference count, not about a
    // second writer.
    let witness = match shape {
        CloseShape::NoReaderSecondFdHeld => Some(open(
            pts.as_str(),
            OFlag::O_RDWR | OFlag::O_NOCTTY,
            Mode::empty(),
        )?),
        _ => None,
    };

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
    let (close_us, bytes_during, arrival) = match shape {
        CloseShape::ReaderArrivesDuringCloseWait => p13_close_with_arriving_reader(slave, fd),
        _ => {
            let t0 = Instant::now();
            drop(slave);
            (t0.elapsed().as_micros() as u64, 0, None)
        }
    };

    std::thread::sleep(PTY_SETTLE);
    let (raw_after, reads_after, _, terminal) = read_available(fd, &mut buf, 64);
    // Same control-byte correction as above.
    let bytes_after = raw_after.saturating_sub(reads_after);

    // Release the witness only now — after the drain — so what was recovered above
    // was recovered while the reference count was still above zero. Dropping it
    // earlier would make this shape a slower spelling of `NoReader`.
    drop(witness);

    Ok(CloseResult {
        close_us,
        bytes_before,
        bytes_during,
        bytes_after,
        terminal,
        slave_mode,
        baseline_packet_bytes,
        arrival,
    })
}

/// Close the slave with a reader arriving **inside** the close, and time both.
///
/// Returns the close's own duration, what the arriving reader recovered, and the
/// arrival block.
///
/// **Why a spinning thread rather than a sleeping one.** The window this is trying
/// to land inside is the kernel's, and it is 600104 µs wide on Darwin and ~7–20 µs
/// wide on Linux. A reader that sleeps, or that is *spawned* at the close, cannot
/// land inside the second of those at all — so the shape would be structurally
/// inert on the platform of record and could only ever report the other kernel's
/// answer, which is §13's vacuity taxonomy 2 (a discriminator that cannot fire).
/// The reader is therefore already running and spinning on an `AtomicBool` before
/// the close is entered, and the arming handshake's own cost is reported
/// (`reader_arm_microseconds`) so a reader that was *not* ready in time is visible
/// rather than inferred.
///
/// **Both spins are bounded.** A `close(2)` that never returns would otherwise
/// leave the reader spinning a core forever, and the doctor is a diagnostic that
/// must not become the fault it is diagnosing (§15.36's never-busy-wait rule
/// applied to this binary). Each side gives up after [`P13_ARRIVAL_SPIN_CAP`] and
/// proceeds; the timestamps then say plainly that the race was lost.
fn p13_close_with_arriving_reader(
    slave: std::os::fd::OwnedFd,
    fd: RawFd,
) -> (u64, u64, Option<ArrivalTiming>) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let epoch = Instant::now();
    let armed = AtomicBool::new(false);
    let closing = AtomicBool::new(false);

    std::thread::scope(|s| {
        let reader = s.spawn(|| {
            let mut rbuf = [0u8; 4096];
            armed.store(true, Ordering::Release);
            let give_up = Instant::now() + P13_ARRIVAL_SPIN_CAP;
            while !closing.load(Ordering::Acquire) && Instant::now() < give_up {
                std::hint::spin_loop();
            }
            let at = epoch.elapsed().as_micros() as u64;
            let (bytes, reads, _, terminal) = read_available(fd, &mut rbuf, 64);
            // One packet-mode control byte per read is not client data — the same
            // correction every other drain in this probe applies.
            (at, bytes.saturating_sub(reads), terminal)
        });

        let give_up = Instant::now() + P13_ARRIVAL_SPIN_CAP;
        while !armed.load(Ordering::Acquire) && Instant::now() < give_up {
            std::hint::spin_loop();
        }
        let armed_at = epoch.elapsed().as_micros() as u64;

        let close_entered = epoch.elapsed().as_micros() as u64;
        closing.store(true, Ordering::Release);
        drop(slave);
        let close_returned = epoch.elapsed().as_micros() as u64;

        // **A reader that never ran must not read as one that arrived.** The
        // fallback timestamp is `u64::MAX`, not `0`: a zero here would be *earlier*
        // than the close's return and would publish `arrived_before_close_returned:
        // true` beside a byte count of zero — an instrument reporting an arrival it
        // did not observe, which is the one failure this shape exists to avoid.
        let (first_read_at, bytes_recovered, terminal) =
            reader
                .join()
                .unwrap_or((u64::MAX, 0, "reader thread panicked".to_owned()));

        let timing = ArrivalTiming {
            first_read_offset_us: first_read_at.saturating_sub(close_entered),
            close_returned_us: close_returned.saturating_sub(close_entered),
            arrived_before_close_returned: first_read_at < close_returned,
            bytes_recovered,
            terminal,
            arm_us: armed_at,
        };
        (
            close_returned.saturating_sub(close_entered),
            bytes_recovered,
            Some(timing),
        )
    })
}

// ---------------------------------------------------------------------------
// P16 — can a held pts slave fd tell that its master has gone? (§15.59)
// ---------------------------------------------------------------------------

/// Back-to-back zero-timeout passes in the quiet window. The same count P12's
/// tight window uses, for the same reason: it is a number six committed artifacts
/// already carry, so a reader comparing the two windows is comparing like with
/// like.
const P16_TIGHT_PASSES: u32 = 200;
/// Passes in the paced window, at the PTY node's own `IDLE_POLL`. "No hangup in
/// back-to-back syscalls" and "no hangup at the mechanism's cadence" are different
/// claims (§15.49 clause 3), and the second is the one the harness's usage
/// resembles.
const P16_PACED_PASSES: u32 = 64;
const P16_PACE: Duration = Duration::from_millis(5);
/// How long the post-close arm will wait for `POLLHUP`. Generous by two orders of
/// magnitude against the microseconds Linux takes, because a kernel that delivers
/// the hangup late is a *finding* and a probe that gave up early would report it
/// as an absence.
const P16_HANGUP_WAIT: Duration = Duration::from_millis(500);

/// One sampling window on the held slave fd: how many passes, over what wall
/// clock, and how many of them saw `POLLHUP`.
///
/// The wall clock is in **microseconds** because that is the unit of the loop's
/// true cost — a millisecond field over a 200-pass tight window prints `0` and
/// witnesses nothing (§15.49 clause 1).
struct P16Window {
    passes: u32,
    hangups: u32,
    elapsed_us: u64,
    /// Every distinct `revents` label the window saw, so a kernel that answers
    /// with some *other* bit is visible rather than folded into "not POLLHUP".
    revents: BTreeMap<String, u64>,
}

impl P16Window {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "passes": self.passes,
            "hangup_passes": self.hangups,
            "elapsed_us": self.elapsed_us,
            "revents_seen": self.revents,
        })
    }
}

/// Poll the held slave for `POLLHUP` `passes` times, pausing `pause` between them.
///
/// Takes the fd as a borrow rather than a `RawFd`: `BorrowedFd::borrow_raw` is an
/// `unsafe` fn and `unsafe` lives only in `serial_nexus_sys` (§16.3), which is the
/// same wall that sent this question to the doctor in the first place. `AsFd`
/// costs nothing and keeps the lifetime real instead of promised.
fn p16_window<Fd: AsFd>(fd: &Fd, passes: u32, pause: Duration) -> P16Window {
    let borrowed = fd.as_fd();
    let mut w = P16Window {
        passes: 0,
        hangups: 0,
        elapsed_us: 0,
        revents: BTreeMap::new(),
    };
    let t0 = Instant::now();
    for _ in 0..passes {
        let mut fds = [PollFd::new(borrowed, PollFlags::POLLHUP)];
        let revents = match poll(&mut fds, PollTimeout::ZERO) {
            Ok(_) => fds[0].revents().unwrap_or_else(PollFlags::empty),
            Err(_) => PollFlags::empty(),
        };
        w.passes += 1;
        if revents.contains(PollFlags::POLLHUP) {
            w.hangups += 1;
        }
        *w.revents.entry(revents_label(revents)).or_insert(0) += 1;
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    w.elapsed_us = t0.elapsed().as_micros() as u64;
    w
}

/// What the harness's three-step `prove_open` comparison can see, measured rather
/// than reasoned about.
///
/// The three steps are mirrored exactly (`itest/src/lib.rs`'s `SlaveWitness`):
/// `fstat` on the held fd, `stat` on the path it was opened through, and the
/// `(st_dev, st_ino, st_rdev)` triple compared between them. They are reported
/// separately because they fail independently and because step 1 is the one the
/// record already measured to be a tautology (notes §3.60).
struct P16StatReading {
    fstat_answers: bool,
    path_resolves: bool,
    /// `None` when the path did not resolve — there is nothing to compare against,
    /// and a `false` there would read as "a different device" rather than "no
    /// device".
    identity_matches: Option<bool>,
}

impl P16StatReading {
    fn take(file: &std::fs::File, path: &str) -> Self {
        use std::os::unix::fs::MetadataExt;
        let held = file.metadata().ok();
        let live = std::fs::metadata(path).ok();
        let triple = |m: &std::fs::Metadata| (m.dev(), m.ino(), m.rdev());
        P16StatReading {
            fstat_answers: held.is_some(),
            path_resolves: live.is_some(),
            identity_matches: match (&held, &live) {
                (Some(h), Some(l)) => Some(triple(h) == triple(l)),
                _ => None,
            },
        }
    }

    /// Would the shipped `prove_open` **refuse** this witness? That is the whole
    /// question the harness asks, expressed as the disjunction its code computes:
    /// the fd's own `fstat` failed, or the path stopped resolving, or the path
    /// resolves to some other device.
    fn prove_open_would_refuse(&self) -> bool {
        !self.fstat_answers || !self.path_resolves || self.identity_matches == Some(false)
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "fstat_on_the_held_fd_answers": self.fstat_answers,
            "path_still_resolves": self.path_resolves,
            "device_identity_matches": self.identity_matches,
            "shipped_prove_open_would_refuse": self.prove_open_would_refuse(),
        })
    }
}

/// Everything one run of P16 measured.
struct P16Result {
    tight: P16Window,
    paced: P16Window,
    stat_while_open: P16StatReading,
    /// The post-close arm: did `POLLHUP` arrive, and how long after the master's
    /// close.
    hangup_after_close: bool,
    hangup_after_us: u64,
    hangup_revents: String,
    stat_after_close: P16StatReading,
    /// What a `read(2)` on the slave answered once the master was gone — a second
    /// witness on the same event, in the vocabulary P6/P13 already report.
    read_after_close: String,
}

impl P16Result {
    /// Can `poll(POLLHUP)` tell a live pair from a dead one **here**? Both arms,
    /// because either alone is uninformative: a fd that reports `POLLHUP` always
    /// answers "dead" correctly and "alive" wrongly.
    fn poll_can_tell(&self) -> bool {
        self.hangup_after_close && self.tight.hangups == 0 && self.paced.hangups == 0
    }

    /// Can the shipped `stat`-comparison tell? Same two-armed shape: it must
    /// **not** refuse a witness whose master is open, and it must refuse one whose
    /// master has gone.
    fn stat_can_tell(&self) -> bool {
        !self.stat_while_open.prove_open_would_refuse()
            && self.stat_after_close.prove_open_would_refuse()
    }
}

/// **P16 — can a held pts slave fd tell that its master has gone?** (§15.59)
///
/// **Why this is a probe and not a paragraph.** `itest`'s `SlaveWitness::prove_open`
/// is the enforcement behind notes §3.56's seven converted guards: it establishes
/// that a held slave is still live by comparing `(st_dev, st_ino, st_rdev)` against
/// a fresh `stat` of the path the fd was opened through. That works **on Linux**
/// because the kernel unlinks `/dev/pts/N` at the *master's* close — measured
/// (notes §3.60), `fstat(fd)` `Ok(020600)` beside `stat(path)` `ENOENT` on one
/// closed pair. On Darwin, whose `/dev/ttysNNN` are persistent devfs nodes, the
/// same comparison is **expected to degrade** to step 1's tautology plus the
/// compile-time borrow. Expected, never measured: §7 forbids exactly that shape of
/// one-way claim, and notes §3.60 named this probe as the instrument that would
/// close it. `poll(POLLHUP)` is the portable candidate, and `itest` may not issue
/// it — `unsafe` lives only in `serial_nexus_sys` (§16.3) — so the doctor is its
/// home.
///
/// **Two arms, and each is the other's control** (§15.49). The quiet arm polls the
/// held slave while the master is **open**, where `POLLHUP` must be absent; the
/// firing arm polls the same fd through the same mask after the master closes,
/// where it must arrive. A probe with only the second would report a hangup that
/// was never absent — the reading is "this fd says dead", which is worthless
/// without "and it said alive a moment ago" — and one with only the first would be
/// a zero with no witness. Both windows are wall-clocked in microseconds, and the
/// quiet arm runs twice: back-to-back and at the PTY node's own 5 ms cadence,
/// because those are different claims.
///
/// **It reports both instruments side by side and judges neither kernel.** The
/// `stat` comparison is mirrored step for step from the harness, so a report says
/// what the shipped check *would have done* on this kernel rather than what this
/// probe thinks of it. `poll_can_tell` and `stat_can_tell` are the two answers; a
/// kernel where the second is `false` is `degraded` with the observation named,
/// which is §7's rule and not a complaint about Darwin.
///
/// Passive: it needs no `--port` and opens no device but a pty. Cost ~0.33 s, the
/// paced window's 64 × 5 ms.
pub fn p16_slave_witness_liveness() -> Probe {
    let p = Probe::new(
        "P16",
        "pts slave-witness liveness",
        "Does a held pts slave fd report POLLHUP once the master closes, and stay quiet while it is open (§15.59)?",
    );
    match p16_inner() {
        Ok(r) => p16_verdict(p, &r),
        Err(e) => measurement_failed(
            p,
            &e,
            "Whether a held pts slave fd can tell that its master has gone is unmeasured on this kernel, so `itest`'s witness-fd argument (notes §3.56) stays resting on the `stat` comparison alone.",
        ),
    }
}

fn p16_inner() -> anyhow::Result<P16Result> {
    let master = new_master()?;
    let pts = sys::ptsname(&master)?;
    apply_pty_baseline(&master, &pts)?;

    // The witness, opened exactly as `itest::attach_slave` opens it: blocking and
    // `O_NOCTTY`. Blocking deliberately — P13 measures an `O_NONBLOCK` slave losing
    // its queued bytes unconditionally on Darwin, so a witness opened non-blocking
    // would arm the hazard it is held to disarm (notes §3.56), and a probe that
    // measured some *other* fd's behaviour would answer a question nobody asked.
    let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    let slave = std::fs::File::from(slave);

    // Arm A, the negative control: the master is open, so the hangup must be
    // absent. Taken before anything closes, and both windows run on the same fd
    // through the same mask the firing arm will use.
    let tight = p16_window(&slave, P16_TIGHT_PASSES, Duration::ZERO);
    let paced = p16_window(&slave, P16_PACED_PASSES, P16_PACE);
    let stat_while_open = P16StatReading::take(&slave, &pts);

    // The edge under test.
    drop(master);

    // Arm B: the same fd, the same mask. Bounded rather than one-shot, because a
    // kernel that delivers the hangup a millisecond later is a finding and a
    // one-shot poll would file it as an absence.
    let t0 = Instant::now();
    let mut fds = [PollFd::new(slave.as_fd(), PollFlags::POLLHUP)];
    let timeout = PollTimeout::try_from(P16_HANGUP_WAIT).unwrap_or(PollTimeout::NONE);
    let revents = match poll(&mut fds, timeout) {
        Ok(_) => fds[0].revents().unwrap_or_else(PollFlags::empty),
        Err(_) => PollFlags::empty(),
    };
    let hangup_after_us = t0.elapsed().as_micros() as u64;

    let stat_after_close = P16StatReading::take(&slave, &pts);
    let mut buf = [0u8; 256];
    let (_, _, _, read_after_close) = read_available(slave.as_raw_fd(), &mut buf, 4);

    Ok(P16Result {
        tight,
        paced,
        stat_while_open,
        hangup_after_close: revents.contains(PollFlags::POLLHUP),
        hangup_after_us,
        hangup_revents: revents_label(revents),
        stat_after_close,
        read_after_close,
    })
}

/// Fold P16's measurement into a verdict and a consequence.
///
/// Split out and pure over the result so both arms are testable on a box that can
/// only produce one of them (§9): the interesting `degraded` shapes are a kernel
/// where the hangup never arrives and one where the quiet arm was not quiet, and
/// neither is reachable on the platform of record.
fn p16_verdict(p: Probe, r: &P16Result) -> Probe {
    let p = p
        .observe("quiet_window_tight", r.tight.observations())
        .observe("quiet_window_paced", r.paced.observations())
        .observe("stat_comparison_while_master_open", r.stat_while_open.observations())
        .observe(
            "hangup_after_master_closed",
            serde_json::json!({
                "hangup_delivered": r.hangup_after_close,
                "microseconds_to_hangup": r.hangup_after_us,
                "revents": r.hangup_revents,
                "read_after_close": r.read_after_close,
            }),
        )
        .observe("stat_comparison_after_master_closed", r.stat_after_close.observations())
        .observe("poll_can_tell_a_live_pair_from_a_dead_one", r.poll_can_tell())
        .observe("stat_comparison_can_tell", r.stat_can_tell())
        .observe(
            "does_not_license",
            "this measures whether a held slave fd can observe its MASTER going away. It says nothing about whether the process on the other side is alive, and nothing about a serial fd, whose peer is a wire and cannot hang up this way (§15.59).",
        );

    // The control first, because a quiet arm that was not quiet makes the firing
    // arm unreadable: "this fd reports a hangup" is not a measurement if it
    // reported one while the master was open (§15.49's control rule).
    if r.tight.hangups > 0 || r.paced.hangups > 0 {
        return p.verdict(
            Status::Degraded,
            &format!(
                "**The negative control fired**: this kernel reported `POLLHUP` on a held pts slave in {} of {} back-to-back passes and {} of {} paced passes **while the master was still open**, so the post-close reading below cannot be read as a liveness signal — an fd that always says `hangup` answers \"dead\" correctly and \"alive\" wrongly. `itest`'s witness argument (notes §3.56) must keep resting on the `stat` comparison here, whose own two-armed reading is `stat_comparison_can_tell: {}`.",
                r.tight.hangups, r.tight.passes, r.paced.hangups, r.paced.passes, r.stat_can_tell()
            ),
        );
    }

    let stat_note = if r.stat_can_tell() {
        format!(
            " The **shipped `stat` comparison also tells them apart here**: `itest`'s `SlaveWitness::prove_open` would accept the witness while the master is open and refuse it afterwards — the path stops resolving ({}) or resolves to a different device. That is the Linux-shaped answer, and it is why the harness's seven converted guards (notes §3.56) are enforced rather than merely borrowed on this kernel.",
            if r.stat_after_close.path_resolves {
                "no — the node persisted"
            } else {
                "yes"
            }
        )
    } else {
        format!(
            " **The shipped `stat` comparison cannot tell them apart here**, and that is this probe's whole reason for existing (§15.59). After the master closed, `fstat` on the held fd answers `{}`, the path still resolves `{}`, and the identity triple matches `{:?}` — so `SlaveWitness::prove_open` would return `Ok` on a witness whose pair is gone, degrading to the compile-time borrow it also carries. `poll(POLLHUP)` is the instrument that does tell, and this row is the measurement that says so rather than the prediction notes §3.60 had to leave standing.",
            r.stat_after_close.fstat_answers,
            r.stat_after_close.path_resolves,
            r.stat_after_close.identity_matches
        )
    };

    if !r.hangup_after_close {
        return p.verdict(
            Status::Degraded,
            &format!(
                "**`POLLHUP` did not arrive on a held pts slave after its master closed** — {} µs of waiting, `revents` `{}`, and a following `read(2)` answering `{}`. So the portable liveness instrument §15.59 proposes does not work on this kernel, and a witness fd cannot learn here that its pair has gone by polling. That is an observation about this kernel and not a failure (§7); what it costs is the portable half of the argument.{stat_note}",
                r.hangup_after_us, r.hangup_revents, r.read_after_close
            ),
        );
    }

    p.verdict(
        Status::Supported,
        &format!(
            "A held pts slave fd **can** tell that its master has gone, on this kernel, through `poll(POLLHUP)`: quiet in all {} back-to-back passes over {} µs and all {} paced passes over {} µs while the master was open, then `POLLHUP` {} µs after it closed (`revents` `{}`, a following `read(2)` answering `{}`). Both arms are the other's control, which is what makes either readable — a zero with no witness is not a measurement, and a hangup that was never absent is not a signal (§15.49).{stat_note} Read it beside `itest`'s `SlaveWitness::prove_open`, which is the check this measures rather than judges.",
            r.tight.passes, r.tight.elapsed_us, r.paced.passes, r.paced.elapsed_us,
            r.hangup_after_us, r.hangup_revents, r.read_after_close
        ),
    )
}

// ---------------------------------------------------------------------------
// The verdict P8/P9/P10 take when the measurement itself failed (§13)
// ---------------------------------------------------------------------------

/// The verdict a kernel-diff probe takes when its measurement did not run.
///
/// **`skipped` is not available here, and the reason is mechanical rather than
/// stylistic.** It is the one word every conditional clause in
/// `expectations/{linux,macos}.jq` exempts — the exemption exists so a `--port`-gated
/// probe does not redden a passive lane — so an error path spelling itself `skipped`
/// exempts itself from exactly the clauses that exist to notice a measurement is
/// missing. Measured, not reasoned: with these three arms spelling `skipped`, a
/// report in which P8, P9 and P10 all errored passed `jq -e -f expectations/linux.jq`
/// at exit **0**, taking P9's discriminator clause and both of P10's content clauses
/// green with it; the identical failure spelled `degraded` exits 1. That is §13's
/// rule verbatim ("`skipped` is never an error path's output") and it is why the
/// three arms route here instead of each spelling their own word.
///
/// `degraded` is what §13 gives a run that could not ask its question, and it costs
/// no lane its exit code on its own account: the summary clause fails on
/// `unsupported` alone, and `unsupported` would be wrong — none of these three
/// probes is a premise the daemon depends on, so an unmeasured one contradicts
/// nothing. What it does do is redden the clauses that were about to certify a
/// measurement nobody took, which is the whole point.
///
/// The error is named twice, deliberately. `probe_error` is the structured cell a
/// gate or a cross-kernel diff can read — a `degraded` with no observation is a
/// verdict word, and §13 forbids diffing those — and the consequence sentence is
/// what an operator reads, keeping the old wording's correct half: what the failure
/// does *not* put at risk.
fn measurement_failed(p: Probe, e: &anyhow::Error, consequence: &str) -> Probe {
    p.observe("probe_error", e.to_string()).verdict(
        Status::Degraded,
        &format!(
            "{consequence} The measurement did not run: {e}. Reported `degraded` rather than \
             `skipped` because a probe that errored still owes the gate the answer it did not \
             give, and `skipped` is the word that would excuse it (§13)."
        ),
    )
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
    p8_verdict(p8_inner())
}

/// P8's verdict, split from its measurement so that every arm is reachable from a
/// unit test on a box that can produce none of its input (§13: verdicts are pure
/// functions of measured facts, which is how `p5_verdict` and `p12_verdict` are
/// tested where the rig cannot exist). The **error** arm is why the split was
/// worth taking: a box that can open a pty cannot be asked to fail one on demand,
/// so the arm §13's `skipped` rule governs had no executable test at all, and it
/// was wrong for exactly as long.
fn p8_verdict(measured: anyhow::Result<(EpollSpin, EpollSpin)>) -> Probe {
    let p = Probe::new(
        "P8",
        "epoll vs read(2) on a pty master",
        "Does epoll report a pty master readable while read(2) returns EAGAIN — the busy-loop shape that made the data plane use poll(2) instead (invariant 1, §15.18)?",
    );
    match measured {
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
                    "{finding} After the last slave closed, the level-triggered set reported an event on {} of {} waits ({} of them with no bytes to read) — persistent readiness on a hung-up fd is expected and is why the PTY reader branches on POLLHUP rather than looping on readability. Diff both blocks against the other kernel of record (6.18 or 7.0, whichever this run is not) before drawing any conclusion from either.",
                    hungup.ready_waits, hungup.waits, hungup.ready_then_no_data
                ),
            )
        }
        // Two error arms, and the split between them is §13's rule rather than a
        // stylistic preference. A platform with no epoll (macOS) offers no
        // mechanism to measure, so the question is *unmeasurable here*: a genuine
        // skip, and the only one this probe is entitled to. A probe that **errored**
        // is not that — it is a run that could not ask its own question, which §13
        // gives `degraded` with the observation named. Both arms stay clear of
        // `unsupported`, because the design is *justified* by P8's answer rather
        // than dependent on it, so neither outcome contradicts anything.
        //
        // The second arm spelled itself `skipped` until 2026-08-12 and that was a
        // hole, not a nicety: this probe's own status clause admits `supported` or
        // `skipped`, so a report whose epoll/read comparison never ran satisfied it
        // (see [`measurement_failed`], and `the_kernel_diff_clauses_bite_on_a_probe_that_errored`
        // in `itest/tests/expectation_gates.rs`, which runs both spellings through
        // the real gate).
        Err(e) if is_unsupported_errno(&e) => p.verdict(
            Status::skipped("epoll is Linux-only"),
            "epoll(7) has no portable equivalent, and the data plane is forbidden from using it anyway (invariant 1) — nothing here is untested, only unmeasurable on this platform (§13).",
        ),
        Err(e) => measurement_failed(
            p,
            &e,
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
/// `read_icounts`).
///
/// A probe turns *this* error into `skipped` with a platform reason, and **any other
/// error into `degraded`** with the error named in a `probe_error` observation
/// ([`measurement_failed`]) — so a genuine failure never hides behind "Linux-only",
/// and never behind `skipped` either. Until 2026-08-12 the second arm was also
/// `skipped`, which is the one word every jq clause exempts, so an errored probe
/// exempted itself from the clauses that read its measurements (§13; notes §3.75).
///
/// It matches **both** error carriers on purpose, and the distinction is not
/// academic: the `serial-nexus-sys` epoll stub returns a
/// `std::io::Error::from_raw_os_error(ENOTSUP)`, while a real `nix` call returns a
/// `nix::Error` — so a guard that exercises only one branch proves nothing about the
/// path the other platform actually takes (AGENTS §9's proxy rule).
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

/// Samples per reference cell, matched to P2's count so sample size is not one of
/// the variables. See [`ZeroTimeoutRefs`].
const P9_REF_SAMPLES: usize = 4096;

/// The masks every reference cell is taken at, at both fd states.
///
/// **The empty mask is the point of this table.** A `poll` that requests
/// *nothing* still receives POLLHUP on a hung-up fd, which is what makes "the mask
/// is not a factor" a measurement rather than a citation of POSIX — and citing
/// POSIX is exactly what §7 forbids when the observation is this cheap to take.
/// The cell can see it because `serial_nexus_sys::poll_blocking` returns `revents`
/// unmasked; a wrapper that intersected them with the requested `events` would
/// have made this table vacuous, and [`an_unrequested_hangup_is_still_delivered`]
/// is the guard that keeps it from becoming so.
///
/// The empty cell is also a **within-group order control**. It runs last at each
/// fd state, so a monotone warmup would make it the cheapest cell in its group;
/// notes §3.45 excluded warmup across probes, and this excludes it within one.
const P9_REF_MASKS: [(&str, PollFlags); 3] = [
    ("pollin", PollFlags::POLLIN),
    ("pollhup", PollFlags::POLLHUP),
    ("empty_mask", PollFlags::empty()),
];

/// What one requested timeout actually cost. Sampled in **nanoseconds** and
/// reported in microseconds, plus `median_ns`: the 0 ms row is the cost of
/// asking, which is sub-microsecond and would diff as a constant `0 µs` on every
/// kernel — a number that cannot differ is not worth printing (P2 reports its
/// zero-timeout poll in ns for the same reason).
/// The **1x2** that decomposes "the cost of a zero-timeout poll" — a phrase two
/// probes use for numbers that differ by an order of magnitude on one kernel.
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
/// **This was built as a 2x2 and it is a 1x2, because the mask cannot vary
/// anything.** POSIX delivers POLLHUP in `revents` whether or not it was requested,
/// so at a fixed fd state both mask cells observe one kernel state. The 2026-08-05
/// Darwin triple shows it: across fd state 7.46-10.12x, across mask 0.968-1.314x
/// with the sign flipping between runs (notes §3.45 B). The isolated variable is
/// **ready versus not-ready**, and "fd state" names how this probe *achieves* that
/// rather than an independently confirmed cause.
///
/// The mask cells stay, and a third — [`P9_REF_MASKS`]'s empty one — joins them,
/// because "the mask does not matter" is a **result**, not a reason to stop
/// measuring. Publishing it as a measured control is also the only form of it that
/// survives a kernel which disagrees: a citation of POSIX would not (§7).
///
/// Two replica cells sit alongside, and they are why this probe no longer has to
/// *name* the P2/P9 instrument offset: they reproduce P2's helper here, on one
/// clock, varying the wrapper and then `O_NONBLOCK` one at a time.
///
/// **Cost** is six times [`P9_REF_SAMPLES`] zero-timeout polls plus two replica
/// cells of the same size — a few milliseconds on a kernel where the poll is cheap,
/// and bounded by the very number being measured on one where it is not (~0.4 s at
/// Darwin's captured 16-22 µs unready cells, the worst case).
struct ZeroTimeoutRefs {
    /// Every cell, in execution order: the unready group, then the hung-up group.
    cells: Vec<ZeroCell>,
    /// **P2's instrument on P9's own fd.** The local [`hup`] helper — a `nix`
    /// `PollFd` over a `BorrowedFd` with `nix`-typed `revents` parsing — on the same
    /// hung-up `O_NONBLOCK` master the `ready_hungup_master_pollhup_ns` cell used.
    /// Exactly one thing differs between the two numbers: the wrapper. That is the
    /// residual notes §3.45 B could only name.
    p2_instrument_same_fd_ns: u64,
    /// The same helper on a fresh hung-up **blocking** master carrying the baseline
    /// termios — P2's shape. Against the line above it isolates `O_NONBLOCK`, and
    /// against `P2.zero_timeout_poll_ns_median` the only remaining differences are
    /// the moment in the process at which it was taken and P2's `?` where this
    /// collapses the `Result`.
    p2_instrument_blocking_fd_ns: u64,
    /// The contamination detector for the replica cells: a hung-up master must
    /// report HUP on every pass, or they measured something else.
    p2_instrument_ready_passes: u64,
}

/// One reference cell: [`P9_REF_SAMPLES`] zero-timeout polls at one fd state and one
/// requested mask, plus what those polls actually saw.
///
/// `revents_seen` is the union across the cell, not a sample. It is what turns the
/// mask from an asserted non-factor into a measured one: the `empty_mask` cell on a
/// hung-up master reads `POLLHUP` here having requested nothing.
struct ZeroCell {
    /// `unready` or `ready_hungup`.
    state: &'static str,
    /// A key from [`P9_REF_MASKS`].
    mask: &'static str,
    median_ns: u64,
    ready_passes: u64,
    revents_seen: PollFlags,
}

impl ZeroCell {
    /// The observation key this cell publishes under. **The four names this produces
    /// for the POLLIN/POLLHUP cells are byte-identical to the ones the 2026-08-05
    /// triples carry**, which is the whole reason the cells are named by formula
    /// rather than restructured into a nested object (§16.13).
    fn key(&self) -> String {
        format!("{}_master_{}_ns", self.state, self.mask)
    }
}

impl ZeroTimeoutRefs {
    fn median(&self, state: &str, mask: &str) -> u64 {
        self.cells
            .iter()
            .find(|c| c.state == state && c.mask == mask)
            .map_or(0, |c| c.median_ns)
    }

    fn medians(&self, state: &str) -> Vec<u64> {
        self.cells
            .iter()
            .filter(|c| c.state == state)
            .map(|c| c.median_ns)
            .collect()
    }

    /// The same medians with the empty-mask cell dropped. On a kernel that gates the
    /// hangup on the requested mask, that cell is not a replicate of the others — it
    /// observes a *different* kernel answer (`none` where they see `POLLHUP`), so
    /// including it in an fd-state contrast compares two states that were never both
    /// reached. Darwin: keeping it reads 1.01x, dropping it reads ~11x (notes §3.53).
    fn medians_requesting(&self, state: &str) -> Vec<u64> {
        self.cells
            .iter()
            .filter(|c| c.state == state && c.mask != "empty_mask")
            .map(|c| c.median_ns)
            .collect()
    }

    fn keys(&self, state: &str) -> Vec<String> {
        self.cells
            .iter()
            .filter(|c| c.state == state)
            .map(ZeroCell::key)
            .collect()
    }

    /// Passes on the *unready* group that reported an event. Must be 0 or the unready
    /// cells measured a ready fd — the same detector the single-cell version carried,
    /// summed over the group.
    fn ready_passes_on_unready_fd(&self) -> u64 {
        self.cells
            .iter()
            .filter(|c| c.state == "unready")
            .map(|c| c.ready_passes)
            .sum()
    }
}

/// One [`ZeroCell`]: [`P9_REF_SAMPLES`] zero-timeout polls through the shipped
/// `poll_blocking` wrapper, for the reason [`p9_poll_granularity`] gives — a
/// difference the wrapper introduces is as real as one the kernel does.
///
/// `revents_seen` accumulates unconditionally rather than only on ready passes, so a
/// cell that saw a bit once still reports it.
fn p9_zero_cell(
    fd: RawFd,
    state: &'static str,
    mask: &'static str,
    interest: PollFlags,
) -> ZeroCell {
    let mut samples = Vec::with_capacity(P9_REF_SAMPLES);
    let mut ready_passes = 0u64;
    let mut revents_seen = PollFlags::empty();
    for _ in 0..P9_REF_SAMPLES {
        let start = Instant::now();
        let revents = sys::poll_blocking(fd, interest, 0);
        samples.push(start.elapsed().as_nanos() as u64);
        revents_seen |= revents;
        if !revents.is_empty() {
            ready_passes += 1;
        }
    }
    samples.sort_unstable();
    ZeroCell {
        state,
        mask,
        median_ns: samples[samples.len() / 2],
        ready_passes,
        revents_seen,
    }
}

/// [`p9_zero_cell`]'s counterpart through **P2's instrument** — the local [`hup`]
/// helper — so the offset between the two probes is measured here instead of argued
/// about across them.
///
/// One difference against P2 remains and is not measurable from inside P9: P2's loop
/// propagates the `Result` with `?` where this one collapses it with `unwrap_or`.
/// Both branch on a `Result` per pass; neither allocates.
fn p9_p2_instrument_median(master: &PtyMaster) -> (u64, u64) {
    let mut samples = Vec::with_capacity(P9_REF_SAMPLES);
    let mut ready = 0u64;
    for _ in 0..P9_REF_SAMPLES {
        let start = Instant::now();
        let hungup = hup(master).unwrap_or(false);
        samples.push(start.elapsed().as_nanos() as u64);
        if hungup {
            ready += 1;
        }
    }
    samples.sort_unstable();
    (samples[samples.len() / 2], ready)
}

/// A ratio of two nanosecond medians in hundredths. Integer on purpose: a frozen
/// artifact (§16.13) must not carry a float whose last digits diff as noise. A zero
/// denominator reads 0 rather than panicking — an unmeasurable cell must not take
/// the report down.
fn ratio_x100(num: u64, den: u64) -> u64 {
    num.saturating_mul(100).checked_div(den).unwrap_or(0)
}

/// The resolution below which this probe declines to read an order effect, in
/// hundredths. **A fitted constant, and its bounds are stated because they are the
/// whole basis for it** (notes §3.73): across the 36 committed artifacts carrying
/// the empty-mask cell, the widest intra-group spread attributable to noise is
/// 1.279x (Darwin's not-ready group) and 1.142x on Linux 7.0, while the one reading
/// the control exists to catch — Linux 6.18's not-ready group at `883/418/418` — is
/// 2.112x. 150 sits above every corpus reading and below that one. It is published
/// as a field so a later capture can re-derive the threshold rather than re-argue
/// it.
const P9_ORDER_TOLERANCE_X100: u64 = 150;

/// What the within-group order control says, per group.
///
/// [`P9_REF_MASKS`]'s empty-mask cell runs **last** at each fd state, so a monotone
/// warmup would leave it the cheapest cell in its group. The probe has documented
/// that control since it was written and never published its outcome — a control
/// asserted and not reported, which is the same defect class as notes §3.50's "a `0`
/// printed with nothing beside it that says what the `0` means" (notes §3.73).
///
/// Deliberately **not** rank-based. A bare "is the last cell the minimum?" reading
/// fires on 20 of 27 committed Linux 7.0 reports, on deltas of 0–3 ns out of ~260 —
/// it would be a false-alarm generator on the platform of record. The reading is
/// magnitude-gated by [`P9_ORDER_TOLERANCE_X100`] and per-group, and a group whose
/// cells are not comparable says so rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum P9OrderGroup {
    /// The cells do not observe one kernel state, so their order says nothing about
    /// warmup. On Darwin the hung-up group is this: an empty mask reads
    /// `revents: none` where the requesting cells read `POLLHUP`, so the spread is
    /// the mask being a *level* and reading it as instrument drift would report a
    /// kernel difference as a warmup artifact.
    NotComparable,
    /// No spread above the stated resolution. **Not a strong pass**: it means there
    /// was nothing to see, not that the control discriminated.
    Flat,
    /// Spread above tolerance, declining in execution order with the last-run cell
    /// cheapest — the signature a monotone warmup would leave.
    ConsistentWithWarmup,
    /// Spread above tolerance but *not* declining in execution order, so whatever
    /// moved these cells is not a monotone warmup.
    WarmupRefuted,
}

impl P9OrderGroup {
    fn label(self) -> &'static str {
        match self {
            P9OrderGroup::NotComparable => "not-comparable",
            P9OrderGroup::Flat => "flat",
            P9OrderGroup::ConsistentWithWarmup => "consistent-with-warmup",
            P9OrderGroup::WarmupRefuted => "warmup-refuted",
        }
    }
}

/// Classify one group's medians, given in **execution order**.
fn p9_order_group(medians: &[u64], comparable: bool) -> P9OrderGroup {
    if !comparable || medians.len() < 2 {
        return P9OrderGroup::NotComparable;
    }
    let (min, max) = match (medians.iter().copied().min(), medians.iter().copied().max()) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ => return P9OrderGroup::NotComparable,
    };
    // An unmeasurable cell must not take the report down, exactly as `ratio_x100`
    // already ensures.
    if min == 0 {
        return P9OrderGroup::NotComparable;
    }
    if ratio_x100(max, min) <= P9_ORDER_TOLERANCE_X100 {
        return P9OrderGroup::Flat;
    }
    let monotone = medians.windows(2).all(|w| w[0] >= w[1]);
    if monotone && medians.last() == Some(&min) {
        P9OrderGroup::ConsistentWithWarmup
    } else {
        P9OrderGroup::WarmupRefuted
    }
}

/// The control's combined reading, and what it does not license.
struct P9OrderControl {
    not_ready: P9OrderGroup,
    ready: P9OrderGroup,
    says: &'static str,
}

fn p9_order_control(
    not_ready: &[u64],
    ready: &[u64],
    hangup_delivered_unrequested: bool,
    ready_passes_on_unready_fd: u64,
) -> P9OrderControl {
    // A not-ready group is comparable only if it really was not ready; a hung-up
    // group only where the empty mask observed the same kernel state as its
    // siblings, which is exactly what `hangup_delivered_unrequested` measures.
    let nr = p9_order_group(not_ready, ready_passes_on_unready_fd == 0);
    let rd = p9_order_group(ready, hangup_delivered_unrequested);
    let says = match (nr, rd) {
        (P9OrderGroup::NotComparable, P9OrderGroup::NotComparable) => "unmeasured",
        (P9OrderGroup::ConsistentWithWarmup, _) | (_, P9OrderGroup::ConsistentWithWarmup) => {
            "warmup-not-excluded"
        }
        (P9OrderGroup::WarmupRefuted, _) | (_, P9OrderGroup::WarmupRefuted) => {
            "spread-above-tolerance-not-in-execution-order"
        }
        _ => "excludes-warmup-above-tolerance",
    };
    P9OrderControl {
        not_ready: nr,
        ready: rd,
        says,
    }
}

/// How the mask column must be read on *this* kernel — derived from the
/// measurement, never asserted.
///
/// §3.52 collapsed this table to a 1x2 on the premise that POSIX delivers `POLLHUP`
/// in `revents` whether or not it was requested, so at a fixed fd state the mask
/// cells observe one kernel state and are replicates. That premise held on the only
/// kernel it was ever run against. **Darwin 24.6.0 refutes it**: an empty mask on a
/// hung-up master returns `revents: none` while `POLLHUP`-requesting cells on the
/// same fd through the same wrapper return `POLLHUP` — so the mask is a real level
/// there and the collapse is invalid (notes §3.53).
///
/// Emitting a fixed `shape` and a fixed rationale beside a field that contradicts
/// both is the defect this exists to prevent, and it is the same defect §3.52
/// repaired on P10's drain axis: one level of a parameter, mistaken for the
/// parameter not mattering.
struct P9MaskAxis {
    shape: &'static str,
    role: &'static str,
    /// Which separation figure the survival criterion must be read against. Where
    /// the mask gates the hangup, the empty-mask "ready" cell is not ready at all,
    /// so folding it into the fd-state contrast understates it by an order of
    /// magnitude — ~1.0x against ~10x on Darwin, where the requesting-mask cells
    /// separate cleanly.
    separation_field: &'static str,
    /// The mask-spread figures the separation must be read against. They must come
    /// from the same cell set as `separation_field`: comparing a cleaned separation
    /// against an uncleaned spread is how the Darwin reading first looked like a
    /// disagreement with Linux when it is not one.
    spread_fields: &'static str,
}

/// Pure, so the two readings are tested against each other rather than against
/// whichever kernel the test box happens to be (§9).
fn p9_mask_axis(hangup_delivered_unrequested: bool) -> P9MaskAxis {
    if hangup_delivered_unrequested {
        P9MaskAxis {
            shape: "1x2",
            role: "measured: a poll requesting nothing still received POLLHUP on this kernel, so at \
                   a fixed fd state every mask cell observed one kernel state — the cells are \
                   replicates, not levels, and the table is a 1x2. The empty-mask cell measures \
                   that rather than citing POSIX, and it runs last in each group, so it doubles \
                   as a within-group warmup control.",
            separation_field: "worst_case_separation_x100",
            spread_fields: "mask_spread_not_ready_x100 / mask_spread_ready_x100",
        }
    } else {
        P9MaskAxis {
            shape: "2x3",
            role: "measured: a poll requesting nothing received NOTHING on a master that a \
                   POLLHUP-requesting poll on the same fd reported hung up, so this kernel gates \
                   the hangup on the requested mask and the mask cells are LEVELS, not \
                   replicates. The table does not collapse here. **This is not a disagreement \
                   with the kernels that do deliver it — it is a third state, and reading it as \
                   a mask level is what makes it look like one.** Compare only the \
                   `*_requesting_*` figures: the empty-mask cell is never made ready here, so \
                   folding it into the ready group collapses the fd-state contrast to ~1x, and \
                   dropping it recovers the same shape a delivering kernel reports — fd state \
                   dominating (~10x) with the mask not mattering (~1.1x) among the cells that \
                   asked for something.",
            separation_field: "worst_case_separation_requesting_masks_x100",
            spread_fields: "mask_spread_not_ready_requesting_x100 / mask_spread_ready_requesting_x100",
        }
    }
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
    p9_verdict(p9_inner())
}

/// P9's verdict, split from its measurement for the reason [`p8_verdict`] gives:
/// the arms become pure functions of measured facts (§13), so the error arm — the
/// one no box can be asked to produce on demand — is reachable from a unit test.
fn p9_verdict(measured: anyhow::Result<(Vec<Granularity>, ZeroTimeoutRefs)>) -> Probe {
    let p = Probe::new(
        "P9",
        "poll(2) timeout granularity",
        "For a never-ready tty fd, what does a requested poll(2) timeout of 0/1/5/10 ms actually cost (min/median/max, µs)?",
    );
    match measured {
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
            // The 1x2 that decomposes the phrase "zero-timeout poll", which P2 and P9
            // both use for numbers that differ 8-11x on Darwin and agree on Linux.
            // Built as a 2x2 and published as a 1x2: the mask cannot vary anything
            // (see `ZeroTimeoutRefs`), so it is reported as a control. The four cell
            // keys the 2026-08-05 triples carry are produced verbatim by
            // `ZeroCell::key`; only the parent key changed.
            let not_ready = refs.medians("unready");
            let ready = refs.medians("ready_hungup");
            let nr_min = not_ready.iter().copied().min().unwrap_or(0);
            let nr_max = not_ready.iter().copied().max().unwrap_or(0);
            let rd_min = ready.iter().copied().min().unwrap_or(0);
            let rd_max = ready.iter().copied().max().unwrap_or(0);
            // The same contrast over the cells that actually requested something. Equal
            // to the above on a kernel where the empty mask is a replicate; the only
            // honest figure on one where it is a level.
            let nr_req = refs.medians_requesting("unready");
            let rd_req = refs.medians_requesting("ready_hungup");
            let nr_req_min = nr_req.iter().copied().min().unwrap_or(0);
            let nr_req_max = nr_req.iter().copied().max().unwrap_or(0);
            let rd_req_min = rd_req.iter().copied().min().unwrap_or(0);
            let rd_req_max = rd_req.iter().copied().max().unwrap_or(0);
            let mut cells = serde_json::Map::new();
            for c in &refs.cells {
                let stem = c.key();
                let stem = stem.trim_end_matches("_ns").to_owned();
                cells.insert(c.key(), serde_json::json!(c.median_ns));
                cells.insert(
                    format!("{stem}_revents"),
                    serde_json::json!(revents_label(c.revents_seen)),
                );
                cells.insert(
                    format!("{stem}_ready_passes"),
                    serde_json::json!(c.ready_passes),
                );
            }
            let hangup_unrequested = refs
                .cells
                .iter()
                .find(|c| c.state == "ready_hungup" && c.mask == "empty_mask")
                .is_some_and(|c| {
                    c.revents_seen.contains(PollFlags::POLLHUP)
                        && c.ready_passes as usize == P9_REF_SAMPLES
                });
            // Derived from the measurement immediately above, never asserted: on a
            // kernel that gates the hangup on the requested mask this is a 2x3 and the
            // empty-mask cell is a level (notes §3.53).
            let axis = p9_mask_axis(hangup_unrequested);
            // `medians()` yields cells in `P9_REF_MASKS` order, which is the order
            // they were measured in — the control reads nothing without that.
            let order = p9_order_control(&not_ready, &ready, hangup_unrequested, contaminated);
            let mut zero_by_state = serde_json::json!({
                "shape": axis.shape,
                "isolated_variable": "ready-vs-not-ready",
                "samples_each": P9_REF_SAMPLES,
                "not_ready_cells": refs.keys("unready"),
                "ready_cells": refs.keys("ready_hungup"),
                "worst_case_separation_x100": ratio_x100(nr_min, rd_max),
                "worst_case_separation_requesting_masks_x100": ratio_x100(nr_req_min, rd_req_max),
                "read_the_separation_from": axis.separation_field,
                "read_the_mask_spread_from": axis.spread_fields,
                // Whether the mask is a control or a level is now the report's answer
                // rather than its premise. Kept under a name that does not assert one:
                // "it does not matter" is a result, and so is "it does".
                "mask_role": axis.role,
                "hangup_delivered_to_a_mask_that_requested_nothing": hangup_unrequested,
                // **The within-group order control's outcome, published rather than
                // asserted** (notes §3.73). `mask_role` has always told the reader
                // the empty-mask cell runs last and so doubles as a warmup control;
                // nothing said whether it passed. `flat` is the common answer and is
                // NOT a strong pass — it means there was nothing above the
                // resolution to see.
                "order_control_says": order.says,
                "order_control_not_ready": order.not_ready.label(),
                "order_control_ready_hungup": order.ready.label(),
                "order_control_tolerance_x100": P9_ORDER_TOLERANCE_X100,
                "order_control_does_not_license": "a within-group ordering, not a mechanism. `consistent-with-warmup` says the cells decline in the order they ran and does NOT name what declined — a cold cache, a frequency ramp and a genuine kernel cost are indistinguishable here. `flat` says only that nothing exceeded `order_control_tolerance_x100`, which is a fitted resolution and not a noise floor. `not-comparable` means the group's cells did not observe one kernel state, which on a hangup-gating kernel is the mask being a level rather than any instrument fault.",
                "mask_spread_not_ready_x100": ratio_x100(nr_max, nr_min),
                "mask_spread_ready_x100": ratio_x100(rd_max, rd_min),
                // The same spreads with the empty mask dropped. On a kernel where it is
                // a replicate these equal the two above; where it is a level, the pair
                // completes the decomposition — and on Darwin it is what recovers the
                // Linux conclusion. There, `mask_spread_ready_x100` reads ~8x purely
                // because the empty-mask cell is in it, while among masks that actually
                // requested something the mask does not matter (~1x) and the fd state
                // does (~7x): the same shape Linux reports, once the empty mask is read
                // as its own kernel state rather than as a third level of the mask
                // (notes §3.53).
                "mask_spread_not_ready_requesting_x100": ratio_x100(nr_req_max, nr_req_min),
                "mask_spread_ready_requesting_x100": ratio_x100(rd_req_max, rd_req_min),
                // The P2/P9 offset, measured rather than named (notes §3.45 B named it).
                "p2_instrument_same_fd_ns": refs.p2_instrument_same_fd_ns,
                "p2_instrument_blocking_fd_ns": refs.p2_instrument_blocking_fd_ns,
                "p2_instrument_ready_passes": refs.p2_instrument_ready_passes,
                "wrapper_offset_x100": ratio_x100(
                    refs.p2_instrument_same_fd_ns,
                    refs.median("ready_hungup", "pollhup"),
                ),
                "nonblocking_offset_x100": ratio_x100(
                    refs.p2_instrument_blocking_fd_ns,
                    refs.p2_instrument_same_fd_ns,
                ),
                "headline_over_matched_cell_x100": ratio_x100(
                    zero,
                    refs.median("unready", "pollin"),
                ),
                "p2_reports_the_shape": "ready_hungup_master_pollhup_ns",
                "p2_instrument_verbatim_is": "p2_instrument_blocking_fd_ns",
                "headline_offset_is": "sample count and warmup only: median_ns_for_0ms_request is \
                     n=16 taken cold, unready_master_pollin_ns is n=4096 on the same fd, same \
                     mask, same wrapper",
                "the_data_plane_parks_on": "unready_master_pollin_ns",
                "ready_passes_on_unready_fd": refs.ready_passes_on_unready_fd(),
            });
            if let Some(obj) = zero_by_state.as_object_mut() {
                obj.extend(cells);
            }
            p = p.observe("zero_timeout_by_fd_state", zero_by_state);
            p.verdict(
                Status::Supported,
                &format!(
                    "A zero timeout costs {zero} ns median (the cost of asking) and a requested 1 ms costs {one_ms} µs median on this kernel — that is the floor §15.19's hybrid data plane was built around and the floor poll_ready's idle backoff steps against. 16 samples per timeout: enough to see the floor, not enough to characterize a tail. `zero_timeout_by_fd_state` is a **{shape}** here: the isolated variable is ready-versus-not-ready, and whether the mask column is a control or a second axis is decided by `hangup_delivered_to_a_mask_that_requested_nothing`, which this kernel answers **{hangup_unrequested}** — see `mask_role`. Read `{separation}` against `{spread}`: the finding survives only where the first exceeds the second, and the two must come from the SAME cell set — `read_the_separation_from` and `read_the_mask_spread_from` name the matching pair, because comparing a figure that drops the empty-mask cell against one that keeps it is not a comparison. `median_ns_for_0ms_request` above is n=16 taken cold and is NOT comparable to P2's headline — `p2_instrument_blocking_fd_ns` is, and `wrapper_offset_x100` / `nonblocking_offset_x100` say how much of any residual is this probe's instrument rather than the kernel's. The empty-mask cell runs last at each fd state, so it doubles as a within-group warmup control, and **`order_control_says` is that control's outcome** — `flat` is the usual answer and is not a strong pass, only a statement that nothing exceeded `order_control_tolerance_x100`; read `order_control_does_not_license` before drawing a mechanism from it. Diff these against the other kernel of record (6.18 or 7.0, whichever this run is not) before tuning any backoff step or timer against them.{}",
                    if contaminated > 0 {
                        format!(" NOTE: {contaminated} pass(es) returned early because the fd reported an event, so those samples measure readiness rather than the timeout — treat the affected rows as suspect.")
                    } else {
                        String::new()
                    },
                    shape = axis.shape,
                    separation = axis.separation_field,
                    spread = axis.spread_fields,
                ),
            )
        }
        // Nothing in the design rests on a *measurement* being available — the
        // floor is whatever it is, and the code copes with any value — so this arm
        // is never `unsupported`. It is not `skipped` either, and that half was
        // wrong until 2026-08-12: `skipped` is the exemption both expectation files
        // grant the clause that reads `zero_timeout_by_fd_state`, the clause whose
        // whole job is to check that P9's mask column measured its own framing
        // (notes §3.65). A P9 that errored into `skipped` therefore carried that
        // clause green, on precisely the run where the framing was never measured.
        // `degraded` with the error named is §13's word for a run that could not
        // ask its question — see [`measurement_failed`].
        Err(e) => measurement_failed(
            p,
            &e,
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
    // The reference cells, after the timeout rows so the slave is still open for
    // them: a hung-up master returns instantly from every poll, which would measure
    // nothing. Unready group first, then the same masks once the session is gone —
    // the fd state is the variable, and it moves exactly once here.
    let mut cells = Vec::with_capacity(P9_REF_MASKS.len() * 2);
    for (mask, interest) in P9_REF_MASKS {
        cells.push(p9_zero_cell(fd, "unready", mask, interest));
    }
    drop(slave);
    std::thread::sleep(PTY_SETTLE);
    for (mask, interest) in P9_REF_MASKS {
        cells.push(p9_zero_cell(fd, "ready_hungup", mask, interest));
    }

    // P2's instrument, twice, one variable at a time. First on **this** fd, which is
    // `O_NONBLOCK`: against `ready_hungup_master_pollhup_ns` the only thing that
    // differs is the wrapper.
    let (p2_instrument_same_fd_ns, p2_instrument_ready_passes) = p9_p2_instrument_median(&master);
    // Then on a fresh **blocking** master, which is P2's shape. A second pair is the
    // only way to vary the file-status flag without disturbing the cells above, and
    // it is cheap: one pty and one hangup.
    let blocking = new_master()?;
    let blocking_pts = sys::ptsname(&blocking)?;
    apply_pty_baseline(&blocking, &blocking_pts)?;
    {
        let primed = open(
            blocking_pts.as_str(),
            OFlag::O_RDWR | OFlag::O_NOCTTY,
            Mode::empty(),
        )?;
        drop(primed);
    }
    std::thread::sleep(PTY_SETTLE);
    let (p2_instrument_blocking_fd_ns, _) = p9_p2_instrument_median(&blocking);

    Ok((
        rows,
        ZeroTimeoutRefs {
            cells,
            p2_instrument_same_fd_ns,
            p2_instrument_blocking_fd_ns,
            p2_instrument_ready_passes,
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

/// The recheck's **drain ladder** (notes §3.52). Each rung refills the pair from
/// empty, hands back exactly this many bytes, and asks the kernel for room again one
/// byte at a time.
///
/// **One drain size is one bit of resolution, and 2026-08-05 proved it.** Under the
/// model "writable iff occupancy < T, then accept up to capacity C", draining D from
/// a full queue leaves occupancy C−D and the top-up accepts D iff `T > C−D`. Darwin's
/// C is 1024/1022 and D was a hardcoded 512, so the single rung excluded only
/// `T <= 512` and every larger threshold predicted the number that was observed. A
/// ladder brackets T instead of assuming it: a rung that tops up puts a floor under
/// T, a rung that refuses puts a ceiling on it.
///
/// **512 is first and must stay first.** It sees the pair in exactly the state the
/// single-rung recheck saw — same call sequence, same history — so the flat
/// `recheck` fields a committed artifact carries are still produced by the same
/// experiment and stay diffable (§16.13). Everything after it runs against a pair
/// with more fill/drain history, which is why each rung publishes its **own** refill
/// and every bound is stated against that rather than a global capacity.
///
/// **1 is the sharp end.** Any watermark strictly below capacity predicts
/// `topped_up == 0` after a one-byte drain, so this rung is the only one that can
/// close the band to `T ∈ (C−1, C]` — and `T = C` *is* the capacity model, so there
/// is nothing left below it to exclude.
const P10_RECHECK_DRAINS: [u64; 4] = [512, 1, 128, 900];

/// The rung the flat `recheck` fields are defined as. Named rather than spelled,
/// because those fields' meaning is "the {`P10_RECHECK_DRAIN`}-byte rung" and a
/// reader must be able to follow that from the constant to the ladder.
///
/// Smaller than the smallest depth either kernel of record has ever reported
/// (Darwin's 1022 is the floor), so this rung's drain never empties the queue and
/// its top-up measures *republished* room rather than a fresh fill.
const P10_RECHECK_DRAIN: u64 = P10_RECHECK_DRAINS[0];

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
/// One rung of [`p10_recheck`]'s ladder.
#[derive(Default)]
struct Rung {
    /// What this rung asked its drain for. `None` is the **from-empty** rung: drain
    /// everything, so the top-up starts at occupancy 0.
    ///
    /// That rung exists because the single-rung recheck could not produce it, and the
    /// gap was structural rather than a matter of sample size: the top-up always
    /// began at occupancy C−512 and never at 0, so a reservation charged at the
    /// empty→nonempty transition was invisible **by construction** and the falsifier
    /// attached to `room_republished_minus_room_freed` could not fire on that shape
    /// at all. Here the transition is inside the measured number. It carries a second
    /// finding for free: this rung fills byte-granularly where
    /// `refilled_from_empty_bytes` fills in 4 KiB chunks, so the two together say
    /// whether write size changes the accounting.
    drain_requested: Option<u64>,
    /// What the same pair accepts when refilled from empty. Equal to `total()` on a
    /// kernel whose bound is a fixed queue capacity; **not** equal on one whose bound
    /// is a snapshot of asynchronous work.
    refilled: u64,
    refill_writes: u64,
    refill_terminal: String,
    /// What the drain actually took. Never assumed equal to `drain_requested`: a
    /// queue shallower than the request gives less, and a cooked pty gives nothing at
    /// all. A rung whose drain came up short is not the rung that was asked for, and
    /// [`p10_ladder_reading`] must not read a bound out of it.
    drained: u64,
    /// What the kernel then accepted, one byte at a time.
    topped_up: u64,
    topup_writes: u64,
    topup_terminal: String,
    topup_ceiling_hit: bool,
}

impl Rung {
    /// The occupancy the top-up started from — this rung's own refill less its own
    /// drain. Every watermark bound is stated against this and never against a global
    /// capacity: on a kernel whose depth moves rung to rung (Linux measures 9728,
    /// 11776 and 13824 refills within one ladder) the two are not the same number,
    /// and using the wrong one would silently shift the bracket.
    fn occupancy_after_drain(&self) -> i64 {
        self.refilled as i64 - self.drained as i64
    }

    fn topped_up_minus_drained(&self) -> i64 {
        self.topped_up as i64 - self.drained as i64
    }

    /// Does this rung constrain a watermark? Only a rung that asked for a partial
    /// drain **and** actually got bytes back freed room whose republication says
    /// anything. The from-empty rung is excluded because `T > 0` is vacuous, and a
    /// zero-byte drain is excluded because it freed nothing — a bound read out of
    /// either would be a verdict computed from a loop that never executed (§9), which
    /// is the exact defect notes §3.48 filed against P4.
    fn carries_a_watermark_bound(&self) -> bool {
        self.drain_requested.is_some() && self.drained > 0
    }

    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "drain_requested_bytes": self.drain_requested,
            "refilled_from_empty_bytes": self.refilled,
            "refilled_from_empty_writes": self.refill_writes,
            "refill_terminal_write": self.refill_terminal,
            "drained_bytes": self.drained,
            "drain_came_up_short": self
                .drain_requested
                .is_some_and(|d| self.drained < d),
            "occupancy_after_drain_bytes": self.occupancy_after_drain(),
            "topped_up_bytes": self.topped_up,
            "topped_up_writes": self.topup_writes,
            "topup_terminal_write": self.topup_terminal,
            "topup_ceiling_hit": self.topup_ceiling_hit,
            "topped_up_minus_drained": self.topped_up_minus_drained(),
            "carries_a_watermark_bound": self.carries_a_watermark_bound(),
        })
    }
}

#[derive(Default)]
struct Recheck {
    /// [`P10_RECHECK_DRAINS`] in order, then the from-empty rung.
    rungs: Vec<Rung>,
}

impl Recheck {
    /// The rung the flat `recheck` fields publish. `None` only if the ladder did not
    /// run, which [`p10_ladder_is_a_ladder`] forbids.
    fn legacy(&self) -> Option<&Rung> {
        self.rungs
            .iter()
            .find(|r| r.drain_requested == Some(P10_RECHECK_DRAIN))
    }
}

/// What the ladder **excludes**, stated as a bracket rather than as a verdict.
///
/// Model under test: "writable iff occupancy < T, then accept up to capacity". A rung
/// that topped up proves the kernel called its `occupancy_after_drain` writable, so
/// `T >` that occupancy; a rung that topped up nothing proves it called it
/// unwritable, so `T <=` that occupancy. The pure-capacity reading is the special
/// case `T = capacity`, which is what an empty refusing set leaves standing — and it
/// leaves it standing only down to the *smallest drain actually probed*, which is why
/// the ladder's smallest rung is one byte.
///
/// **The bounds are model-relative, and on a pipeline kernel `threshold_gt` is not an
/// occupancy anyone should read on its own.** Where every rung tops up — Linux —
/// `threshold_gt` is just the largest `refilled − drained` the ladder saw, an
/// occupancy the top-up demonstrably did *not* meet, because the pipeline moved under
/// it during the settle. It is a bound within the stated model, not a measurement of
/// the queue.
#[derive(Default)]
struct LadderReading {
    topping_up: usize,
    refusing: usize,
    /// `T >` this. `None` when no rung topped up, in which case nothing here is a
    /// bound and the ladder met a kernel that refused every rung.
    threshold_gt: Option<i64>,
    /// `T <=` this. `None` when no rung refused: **no hysteresis was observed at any
    /// drain size this ladder probed**, which is a bounded statement and not "there is
    /// no watermark".
    threshold_le: Option<i64>,
    /// The common `drained − topped_up` shortfall, when every bound-carrying rung
    /// shows the same positive one. That is the signature of a reservation charged per
    /// fill episode, and it is a shape a single rung cannot display: at one drain size
    /// a constant shortfall and a smaller capacity are the same number.
    uniform_shortfall: Option<i64>,
    /// How many rungs the two bounds were computed from. Exported so a reader can see
    /// that a bracket came from rungs that ran, rather than having to trust it.
    rungs_carrying_a_bound: usize,
}

impl LadderReading {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "rungs_carrying_a_bound": self.rungs_carrying_a_bound,
            "rungs_topping_up": self.topping_up,
            "rungs_refusing": self.refusing,
            "watermark_threshold_gt": self.threshold_gt,
            "watermark_threshold_le": self.threshold_le,
            "uniform_shortfall_bytes": self.uniform_shortfall,
            "reading": "T is the watermark in `writable iff occupancy < T, then accept up to \
                 capacity`. A null `watermark_threshold_le` means no rung refused, so no \
                 hysteresis was seen at any drain size probed — bounded by the smallest rung, \
                 not proof of a pure capacity. A null `watermark_threshold_gt` means no rung \
                 topped up at all. Both are null when no rung freed any bytes, which is what a \
                 cooked pty does; nothing is inferred from a rung that freed nothing. Where \
                 `rungs_refusing` is 0 the `_gt` bound is the largest occupancy the ladder \
                 happened to reach and NOT an occupancy the kernel was observed to accept a \
                 write at — on a pipeline kernel it moved under the top-up.",
        })
    }
}

/// Fold the ladder into its bracket. Pure, so it is tested against synthetic rungs
/// rather than against whatever kernel the test box happens to be (§9).
fn p10_ladder_reading(rungs: &[Rung]) -> LadderReading {
    let bounded: Vec<&Rung> = rungs
        .iter()
        .filter(|r| r.carries_a_watermark_bound())
        .collect();
    let mut out = LadderReading {
        rungs_carrying_a_bound: bounded.len(),
        ..LadderReading::default()
    };
    for r in &bounded {
        let occ = r.occupancy_after_drain();
        if r.topped_up > 0 {
            out.topping_up += 1;
            out.threshold_gt = Some(out.threshold_gt.map_or(occ, |m: i64| m.max(occ)));
        } else {
            out.refusing += 1;
            out.threshold_le = Some(out.threshold_le.map_or(occ, |m: i64| m.min(occ)));
        }
    }
    let shortfalls: Vec<i64> = bounded
        .iter()
        .map(|r| -r.topped_up_minus_drained())
        .collect();
    out.uniform_shortfall = match shortfalls.first() {
        Some(&first) if first > 0 && shortfalls.iter().all(|&s| s == first) => Some(first),
        _ => None,
    };
    out
}

/// What a `FIONREAD` reading is worth once a drain has said what was really there.
///
/// [`FillResult::peer_pending_input`] is the one field in this probe a reader can
/// mistake for an *answer* — `0` looks like "the queue was empty" — and on a
/// kernel of record it is wrong in exactly that direction: `docs/doctor/`'s six
/// Darwin captures (`…-7ead470-tier3{,-2,-3}` and `…-1a9a8fc-tier3{,-2,-3}`) read
/// `peer_pending_input_bytes: 0` beside `bytes_recovered_by_peer: 1024`
/// targetward, byte-identical, across two binaries. So the report carries the
/// classification instead of leaving it to be inferred (§7: name the observation),
/// and it is **not** enough to compare against the depth: Linux answers correctly
/// and still reads *less* than `recovered`, saturating at 4095 (the n_tty read
/// buffer) with the whole 13824–15360 recoverable. Undercounting is a documented
/// cap; claiming empty is a fault.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FionreadTrust {
    /// The ioctl failed or is unimplemented: nothing to trust or distrust.
    Unavailable,
    /// The drain recovered nothing, so there is no reading to check against.
    NothingToCheck,
    /// `FIONREAD` matched what the drain then recovered, byte for byte.
    Agrees,
    /// `FIONREAD` reported some of what the drain recovered — a staging cap, not a
    /// fault (Linux 7.0.0-29: 4095).
    Undercounts,
    /// `FIONREAD` reported MORE than the drain could recover.
    Overcounts,
    /// **It said empty and the very next `read(2)` returned bytes.** Taken with
    /// both fills finished in `EAGAIN`, on this thread, with the drain as the next
    /// statement and no writer anywhere — so no arrival-timing story explains it.
    ContradictedEmpty,
}

impl FionreadTrust {
    fn key(self) -> &'static str {
        match self {
            FionreadTrust::Unavailable => "unavailable",
            FionreadTrust::NothingToCheck => "nothing-to-check",
            FionreadTrust::Agrees => "agrees",
            FionreadTrust::Undercounts => "undercounts",
            FionreadTrust::Overcounts => "overcounts",
            FionreadTrust::ContradictedEmpty => "contradicted-empty",
        }
    }

    /// Where a reader must not take `peer_pending_input_bytes` at face value.
    fn is_wrong(self) -> bool {
        matches!(
            self,
            FionreadTrust::ContradictedEmpty | FionreadTrust::Overcounts
        )
    }
}

/// Pure and total, so the decision is testable on a kernel that cannot produce
/// every row (§9). `Some(0)` with nothing recovered is `NothingToCheck` rather
/// than `Agrees` on purpose: an empty queue agreeing with an empty reading is not
/// evidence that the instrument works, and a gate reading `agrees` off it would
/// pass vacuously.
fn fionread_trust(at_drain: Option<u64>, recovered: u64) -> FionreadTrust {
    match at_drain {
        None => FionreadTrust::Unavailable,
        Some(0) if recovered == 0 => FionreadTrust::NothingToCheck,
        Some(0) => FionreadTrust::ContradictedEmpty,
        Some(n) if n == recovered => FionreadTrust::Agrees,
        Some(n) if n < recovered => FionreadTrust::Undercounts,
        Some(_) => FionreadTrust::Overcounts,
    }
}

/// One direction's instrument check, for [`p10_fionread_note`].
struct FionreadCheck<'a> {
    direction: &'a str,
    at_drain: Option<u64>,
    recovered: u64,
    writer: Option<u64>,
}

/// The sentence P10 owes a reader when its own `FIONREAD` is provably wrong.
/// Empty when every direction is sound, so a healthy report gains no prose.
fn p10_fionread_note(checks: &[FionreadCheck<'_>]) -> String {
    let mut out = String::new();
    for c in checks {
        let trust = fionread_trust(c.at_drain, c.recovered);
        if !trust.is_wrong() {
            continue;
        }
        out.push_str(&format!(
            " **INSTRUMENT WARNING — `peer_pending_input_bytes` is provably wrong in the `{}` direction on this kernel and must not be read as an answer.** `FIONREAD` on the reading end reported **{}** byte(s) immediately before a drain that then recovered **{}**, with both fills finished in `EAGAIN` and no writer in between — so a `0` here does not mean \"the queue was empty\", it means the ioctl does not describe this fd's readable queue (`peer_pending_input_trust: {}`). `bytes_recovered_by_peer` is unaffected: it is a drain, not an ioctl, and it is the field to size anything against. The reading is reported rather than dropped because which kernels answer this ioctl correctly on a pty master, and in which direction, is itself the cross-kernel observation (§7) — Linux 7.0.0-29 answers exactly and saturates at 4095 (the n_tty read buffer) with the remainder still recoverable, and reads 0 on the *writing* fd in both directions. **Pre-registered so the next run settles the mechanism:** if a master's `FIONREAD` answers with the tty's *input* queue rather than its readable one, then `writer_pending_input_bytes` in the hostward direction — the master, which has nothing to read — comes back non-zero and equal to that direction's depth; it reads **{}** here, and a 0 there refutes that reading and leaves the mechanism open. Interpretable only with `slave_termios_mode: raw`, which this run reports: with ECHO on, a master legitimately has echoed bytes to read.",
            c.direction,
            c.at_drain.map(|n| n.to_string()).unwrap_or_else(|| "unavailable".to_owned()),
            c.recovered,
            trust.key(),
            c.writer.map(|n| n.to_string()).unwrap_or_else(|| "unavailable".to_owned()),
        ));
    }
    out
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
    /// The **second** `FIONREAD` on the reading end, taken immediately before the
    /// drain. The field above it is sampled mid-measurement, before the second
    /// fill pass, so a disagreement there always has an innocent reading — bytes
    /// arrived later. This one has none, and that is the entire reason it exists:
    /// it is the sample a contradiction can be *proved* against.
    peer_pending_input_at_drain: Option<u64>,
    /// `FIONREAD` on the **written** fd, where a correct implementation reports 0
    /// — measured `Some(0)` in both directions on Linux 7.0.0-29. The
    /// discriminator for *why* a peer-side reading can be wrong; see
    /// [`p10_fionread_note`]'s pre-registration.
    writer_pending_input: Option<u64>,
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
        // The flat `recheck` fields below are **the 512-byte rung** and nothing else.
        // Unchanged in name, in value and in production order, because that rung runs
        // first and sees the pair state the single-rung recheck saw — so a diff
        // against the 2026-08-05 triples still compares one experiment to the same
        // experiment (§16.13). Everything the ladder adds is a sibling.
        let fallback = Rung::default();
        let legacy = self.recheck.legacy().unwrap_or(&fallback);
        let reading = p10_ladder_reading(&self.recheck.rungs);
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
            "peer_pending_input_bytes_at_drain": self.peer_pending_input_at_drain,
            "peer_pending_input_trust":
                fionread_trust(self.peer_pending_input_at_drain, self.recovered).key(),
            "writer_pending_input_bytes": self.writer_pending_input,
            "pending_output_bytes": self.pending_output,
            "slave_termios_mode": self.slave_mode,
            "bytes_recovered_by_peer": self.recovered,
            "bytes_unrecoverable": self.total().saturating_sub(self.recovered),
            "recheck": {
                "refilled_from_empty_bytes": legacy.refilled,
                "refilled_from_empty_writes": legacy.refill_writes,
                "refill_terminal_write": legacy.refill_terminal,
                "refill_reproduced_total": legacy.refilled == self.total(),
                "drained_again_bytes": legacy.drained,
                "topped_up_bytes": legacy.topped_up,
                "topped_up_writes": legacy.topup_writes,
                "topup_terminal_write": legacy.topup_terminal,
                "topup_chunk_bytes": 1,
                "topup_ceiling_bytes": P10_RECHECK_CEILING,
                "topup_ceiling_hit": legacy.topup_ceiling_hit,
                // Zero means the kernel republished exactly the room the reader
                // freed; positive means an asynchronous pipeline advanced during the
                // settle; negative means a reservation. **At one drain size this
                // field cannot tell a capacity from a watermark above that size, and
                // cannot see a reservation charged at the empty→nonempty transition
                // at all** — that is what the ladder below is for, and this field is
                // now a summary of one rung rather than the answer.
                "room_republished_minus_room_freed": legacy.topped_up_minus_drained(),
                "flat_fields_are_the_rung_draining": P10_RECHECK_DRAIN,
                "ladder": self
                    .recheck
                    .rungs
                    .iter()
                    .map(Rung::observations)
                    .collect::<Vec<_>>(),
                "ladder_reading": reading.observations(),
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
    p10_verdict(p10_fill_direction(true), p10_fill_direction(false))
}

/// P10's verdict, split from its two fills for the reason [`p8_verdict`] gives:
/// the arms become pure functions of measured facts (§13), so the error arm is
/// reachable from a unit test on a box whose ptys all work.
fn p10_verdict(
    targetward: anyhow::Result<FillResult>,
    hostward: anyhow::Result<FillResult>,
) -> Probe {
    let p = Probe::new(
        "P10",
        "pty buffer depth",
        "How many bytes does a pty accept in each direction before it would block, with nothing draining the other end?",
    );
    match (targetward, hostward) {
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
                    "This kernel's pty accepted {} byte(s) slave→master (**targetward** — a client typing, travelling toward the device, first pass ending in `{}`) and {} byte(s) master→slave (**hostward** — the node delivering device output to its client, ending in `{}`), reaching {} and {} in total once a short pause has let the tty's asynchronous flip work run. **Of those, {} and {} byte(s) were actually recoverable by the peer** ({} / {}): acceptance is not delivery, and the two are the same number only on a kernel that queues everything it takes. Read the daemon's `hostward_buffer` defaults against the SCALE of these, not their last byte: the pty default is 32 chunks, and a queue far larger than the kernel pipe below it only defers the same backpressure. Both figures move by a chunk or two run to run depending on when that flip work lands, so a one-chunk difference across kernels is noise; only an order-of-magnitude one is signal, **and only between runs whose `slave_termios_mode` agrees** — a cooked pty and a raw one give different depths on one kernel, and in opposite directions — raw accepts less and returns all of it, cooked accepts more and returns none (measured on Linux 7.0.0-29) — so a mode mismatch explains a gap before any kernel difference does, and the `slave_termios_mode` cell beside each direction is what settles it. The `recheck` block under each direction asks the second question the first cannot, at four drain sizes rather than one: after the peer is drained the pair is refilled from empty and handed back 512, 1, 128 and 900 bytes in turn, and then once from empty entirely. `ladder_reading.watermark_threshold_gt` and `_le` bracket the watermark in \"writable iff occupancy < T, then accept up to capacity\" — a rung that tops up floors T at its `occupancy_after_drain`, a rung that refuses caps it there. A null `_le` means no rung refused, which bounds T below capacity only down to the smallest rung probed and is **not** proof of a pure capacity; read `_gt` on such a run as the largest occupancy the ladder reached rather than as one the kernel accepted a write at, because on a pipeline kernel it moved under the top-up. `uniform_shortfall_bytes` names a reservation charged per fill episode; the from-empty rung (`drain_requested_bytes: null`) is the one whose top-up starts at occupancy 0, so a reservation charged at the empty→nonempty transition lands inside its number instead of behind it, and comparing its `topped_up_bytes` against the 4 KiB-chunked `refilled_from_empty_bytes` on the same rung says whether write size changes the accounting. The flat fields beside the ladder are the 512-byte rung alone, kept so older reports still diff, and `room_republished_minus_room_freed` there says whether the kernel gave back exactly the room a reader freed (a fixed queue capacity), or more (an asynchronous pipeline that advanced during the settle — Linux 7.0.0-29 reads +2048 or +9216, bimodal, never 0 across 20 samples), or less. `refill_reproduced_total` says whether the depth above is reproducible on the same pair at all; on Linux it usually is not. Numbers, not a verdict — diff them against the other kernel of record (6.18 or 7.0, whichever this run is not) before changing a default.{}{}{}",
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
                    },
                    p10_fionread_note(&[
                        FionreadCheck {
                            direction: "slave_to_master_targetward",
                            at_drain: targetward.peer_pending_input_at_drain,
                            recovered: targetward.recovered,
                            writer: hostward.writer_pending_input,
                        },
                        FionreadCheck {
                            direction: "master_to_slave_hostward",
                            at_drain: hostward.peer_pending_input_at_drain,
                            recovered: hostward.recovered,
                            writer: hostward.writer_pending_input,
                        },
                    ]),
                ),
            )
        }
        // Nothing is contradicted by an unmeasured buffer depth: the defaults are
        // configuration, and every one of them works at any depth (backpressure is
        // the mechanism either way, §5). So never `unsupported` — and, since
        // 2026-08-12, never `skipped` either. This probe is the one where that word
        // cost the most: its status clause already admits `degraded` (a non-raw
        // line discipline degrades by design), so the two clauses carrying P10's
        // content — the FIONREAD cross-check, and the recheck *ladder*, which
        // exists because a one-rung recheck passed both gates while bracketing
        // nothing — hang off `$p.status == "skipped"` alone. An errored P10
        // spelling itself `skipped` took both of them green with it, on the one run
        // where neither direction was measured at all. See [`measurement_failed`].
        (Err(e), _) | (_, Err(e)) => measurement_failed(
            p,
            &e,
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
    // Same instant as the two above, and worth its syscall because it is the
    // DISCRIMINATOR for a wrong peer-side reading: on a correct implementation the
    // writer's own readable queue is empty in both directions (Linux 7.0.0-29,
    // `Some(0)` both ways). A kernel answering `FIONREAD` on a pty master out of
    // the tty's *input* queue instead of its readable one reads that direction's
    // depth here — on the fd that has nothing to read at all.
    let writer_pending_input = sys::pending_input_bytes(write_to).ok().map(|n| n as u64);
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
    //
    // The reading that can be CHECKED — see `FillResult::peer_pending_input_at_drain`.
    // Deliberately the statement immediately before the drain: nothing runs between
    // the ioctl and the first `read(2)` it can be contradicted by.
    let peer_pending_input_at_drain = sys::pending_input_bytes(peer).ok().map(|n| n as u64);
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
        peer_pending_input_at_drain,
        writer_pending_input,
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

/// See [`Recheck`]. **Precondition: the peer is already drained** — every rung's
/// refill measures a fill from empty, and starting one against a full queue would
/// report 0 and say nothing.
///
/// Rungs run in [`P10_RECHECK_DRAINS`] order, then the from-empty rung. Each leaves
/// the peer empty, so the next rung's refill is a refill from empty and not from
/// whatever the last one left behind.
fn p10_recheck(write_to: RawFd, peer: RawFd) -> Recheck {
    let mut rungs = Vec::with_capacity(P10_RECHECK_DRAINS.len() + 1);
    for drain in P10_RECHECK_DRAINS {
        rungs.push(p10_recheck_rung(write_to, peer, Some(drain)));
    }
    rungs.push(p10_recheck_rung(write_to, peer, None));
    Recheck { rungs }
}

/// One rung: refill from empty, hand back `drain_requested` bytes (or everything, for
/// the from-empty rung), let the tty's asynchronous work run, then write **one byte
/// at a time** until it blocks.
///
/// The call sequence is the single-rung recheck's, unchanged and in the same order —
/// refill, drain, settle, byte-granular top-up, full drain. It is unchanged on
/// purpose: the 512 rung runs first and must reproduce the committed artifacts'
/// `recheck` block exactly, and an extra settle before the drain (which would be
/// defensible on its own terms) would have moved every one of those numbers.
fn p10_recheck_rung(write_to: RawFd, peer: RawFd, drain_requested: Option<u64>) -> Rung {
    let (refilled, refill_writes, refill_terminal, _) = p10_fill_pass(write_to, 0);
    let drained = p10_drain_at_most(peer, drain_requested.unwrap_or(P10_CEILING));
    // The same settle the second fill pass uses. On a kernel that moves bytes on an
    // asynchronous work item, room appears only after it runs, so measuring before it
    // does would report that kernel's scheduling rather than its bookkeeping.
    std::thread::sleep(P10_SETTLE);
    let (topped_up, topup_writes, topup_terminal, topup_ceiling_hit) =
        p10_fill_pass_with(write_to, 0, 1, P10_RECHECK_CEILING);
    // Leave the pair as this rung found it.
    let _ = p10_drain(peer);
    Rung {
        drain_requested,
        refilled,
        refill_writes,
        refill_terminal,
        drained,
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
    // node. Counted, never judged — neither is a failure of identity resolution *on
    // its own*, and that decline stands unchanged — review 32's RES-2 recorded it
    // in as many words ("It stays `supported` in a no-udev environment **by
    // design**", `docs/implementation-notes.md`, "The resolver's second door, and
    // the diagnostic that pointed away from it"), and §5 forbids silently
    // re-fixing a recorded decline. What is narrowed below is only the case that
    // decision never contemplated: a tree where nothing at all resolved.
    let other = candidates.iter().filter(|c| c.by_id.is_none()).count() - unnamed.len();
    // **The population the verdict is actually computed over.** `for a in &adapters`
    // is not one. Where udev published no by-id links it runs zero times, leaves
    // `all_resolved` at its initialised `true`, and P4 then asserted "Resolver
    // produces canonical identities; configs survive replug and cold start" off a
    // loop that never executed — §9's vacuous pass. Darwin is exactly that tree:
    // four `cu.*` candidates and `count: 0`, byte-identical across
    // `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json` (notes
    // §3.45 (ii)). So is a Linux box whose only adapter is a serial-number-less
    // clone reachable through `/dev/serial/by-path` alone — the plan's "no-serial"
    // P4 case, which this probe could not report while it looked only at by-id.
    // `canonical` counts devices for which the resolver *did* produce
    // `usb:vid:pid:serial:iface`, over the deduplicated candidate union rather than
    // over the by-id listing, and no arm may claim identity resolution works
    // without at least one.
    let of_kind =
        |k: serial_nexus_core::DeviceKind| candidates.iter().filter(|c| c.kind == k).count();
    let canonical = of_kind(serial_nexus_core::DeviceKind::Usb);
    let topology_only = of_kind(serial_nexus_core::DeviceKind::ByPath);
    let unidentified = of_kind(serial_nexus_core::DeviceKind::Raw);
    // The resolver's *other* source, reported because it is what separates the two
    // `canonical == 0` worlds — a kernel with no sysfs at all from a Linux box whose
    // adapter simply has no serial number. §7 wants the observation named.
    let sysfs_tty_listing = sys_root.join("class/tty").is_dir();

    let p = p
        .observe(
            "by_id_tree",
            if by_id_present { "present" } else { "absent" },
        )
        .observe(
            "sysfs_tty_listing",
            if sysfs_tty_listing {
                "present"
            } else {
                "absent"
            },
        )
        .observe("count", adapters.len() as u64)
        .observe("sysfs_only", unnamed.len() as u64)
        .observe("other_candidates", other as u64)
        .observe("canonical", canonical as u64)
        .observe("topology_only", topology_only as u64)
        .observe("unidentified", unidentified as u64);

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
    // Name the devices that resolved to something weaker than canonical, not just
    // how many. The report is the diff artifact (§13), and an operator needs to know
    // *which* node is the one whose identity will not survive a replug.
    for c in candidates
        .iter()
        .filter(|c| c.kind != serial_nexus_core::DeviceKind::Usb)
    {
        p = p.observe(&c.path.display().to_string(), c.identity.clone());
    }

    // The by-id tree's *absence* is reported on the environment check as `degraded`
    // with the observation named (§13), not here: this probe answers "does identity
    // resolution work", and in a no-udev Linux tree it does — through the sysfs
    // listing, which is the same source capture reads. Naming it in the consequence
    // keeps the operator informed without reddening a box the daemon is fine on.
    //
    // The sentence is now conditioned on `unnamed` as well as on the tree, because
    // it asserts a *provenance*: it read "identities came from the <sys>/class/tty
    // listing" on Darwin, which has no such listing and reported `sysfs_only: 0` —
    // prose written for a Linux container, asserted about a BSD box (notes §3.45
    // (ii)). Its precondition is a device that actually came from that listing.
    let where_from = if by_id_present || unnamed.is_empty() {
        ""
    } else {
        " No /dev/serial/by-id tree here (no udev 60-serial.rules — a container's bare --device=…, a busybox-mdev image): identities came from the <sys>/class/tty listing, the same source capture reads (§12)."
    };
    if canonical == 0 {
        // Devices are visible and not one of them resolved to a canonical identity.
        // §7 verbatim: an environment that differs is `degraded` with the observation
        // named. Not `unsupported` — the `by-path:`/`raw:` forms are a working
        // fallback the daemon binds through (§12), so no design premise is
        // contradicted. Not `skipped` either, and that is the load-bearing half: the
        // resolver *ran*, over all four of its passive sources, and answered; the
        // question was asked and came back negative, so "untested here" would replace
        // one false statement with another. The report already says `degraded` for
        // this same fact one section away, on the `/dev/serial/by-id` environment
        // check — one report must not answer one question two ways.
        //
        // A by-id link can outlive its device node, and `enumerate_ports` drops those,
        // so the count of devices *seen* is the larger of the two lists.
        let visible = candidates.len().max(adapters.len());
        let why = if sysfs_tty_listing || by_id_present {
            " The resolver's sources are readable here, so this is a device with no usable serial number — §12's by-path fallback — not a missing mechanism."
        } else {
            " Neither /dev/serial/by-id nor <sys>/class/tty exists here, so the resolver's Linux backend has no source at all: the BSD/macOS shape, where a cu.* raw path is the interim identity and a node configured with a usb: or by-path: identity resolves to nothing and stays `waiting` (§12, §13; the IOKit backend that would supply canonical identities off Linux is deferred, §14)."
        };
        return p.verdict(
            Status::Degraded,
            &format!(
                "0 of {visible} visible serial device(s) resolve to a canonical usb:vid:pid:serial:iface identity ({topology_only} by-path only, {unidentified} raw path only): the identity a config would store here does NOT survive replug or renumbering, and carries the documented instability warning (§12).{why}"
            ),
        );
    }
    // `all_resolved` can still be vacuously `true` — a tree with sysfs-only devices
    // has no by-id adapters to iterate — and that is now harmless: it can only
    // *downgrade*. The `supported` claim rests on `canonical >= 1`, counted over the
    // population above.
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

/// The observation **key** for a pair, ordered so it does not depend on which port
/// discovery reached first.
///
/// `field_set` (§15.44) digests the sorted set of observation leaf paths, so a key
/// built as `format!("{a} ↔ {b}")` straight from `ports[i]`/`ports[j]` makes the
/// digest a function of discovery order rather than of the cells. That is not
/// theoretical: two Linux Tier-3 runs at the same commit, with `git diff -- doctor/`
/// empty, produced different `field_set` values whose *entire* delta was this key
/// spelled both ways — 478 leaf paths each, every cell present in both. A reviewer
/// following §15.44's rule would have been told to "diff only the intersection" of two
/// runs that carry identical cells. `ports[i]`/`ports[j]` follow argv, and the identity
/// standing at a given argv position moves across a replug (§12's own test renumbers
/// `ttyUSB0`/`ttyUSB1` deliberately), so the flip needs no code change to appear.
///
/// This is the same failure mode `probe_set`'s choice 3 (`report.rs`) was written to
/// avoid, reappearing in the second digest. Sorting is the whole fix, and it is a
/// **key**-ordering change only: the `a`/`b` roles below keep argv order, because they
/// carry measured direction (`rts_a_to_cts_b` against `rts_b_to_cts_a`, P14's `ab`
/// against `ba`) and reordering them would relabel measurements to stabilize a digest
/// that does not read them. §15.44 already says neither digest can see a value change.
fn p5_pair_subject(a_name: &str, b_name: &str) -> String {
    if a_name <= b_name {
        format!("{a_name} ↔ {b_name}")
    } else {
        format!("{b_name} ↔ {a_name}")
    }
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

/// Why [`p5_certify_port`] was not run for a port [`p5_is_uart`] rejected.
///
/// **One sentence on both platforms, and that is the repair.** The predicate used
/// to be `TIOCGICOUNT` alone, which is Linux-only, so off Linux it answered
/// `false` for every port — a genuine FTDI adapter included — and this constant
/// carried a second, kernel-shaped spelling to keep the report from calling the
/// operator's hardware a non-UART. §15.47 replaced the predicate with the
/// disjunction `TIOCMGET || TIOCGICOUNT`, and a pts fails **both** on both
/// kernels (checked), so the port-shaped answer is now true everywhere and the
/// platform-shaped one is not: a Darwin adapter reaching this constant answered
/// neither ioctl, which is a fact about that port and not about Darwin.
///
/// The dated measurement that motivated the old wording is kept as history and
/// labelled as such: on 2026-07-28, macOS 15.7.8 with two real FTDI adapters
/// cross-wired, P5 discovered the pair in both directions and then skipped both
/// ports' characterization. That was the pre-widening predicate. The committed
/// `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json` (`1a9a8fca1c36`) is
/// the same rig after it, and both ports certify there — `custom_baud=true
/// break=true` — with the pair reporting `rate_ladder=true`.
///
/// A capability report may only claim what it measured (§15.17); what is measured
/// here is that this port answered neither `TIOCMGET` nor `TIOCGICOUNT`.
const P5_UNCHARACTERIZED: &str = "not a UART (answered neither TIOCMGET nor TIOCGICOUNT)";

/// Why a failed `icounter` item is not the rig's fault where `TIOCGICOUNT` does
/// not exist — the mechanism P5's verdict owes the operator (§7).
const P5_WHY_NO_ICOUNTER: &str = "the driver's input counters are read with TIOCGICOUNT, which exists only on Linux (`serial_nexus_sys::read_icounts` is a compile-time ENOTSUP stub elsewhere), so every port answers Err here however real it is";

/// The same absence reaching the *pair* item, stated separately because the two
/// halves of the deliberate mismatch do not fail together. The bulk pattern is
/// still transmitted and may well be corrupted on the wire; it is the witness
/// that is missing, so `after > before` reduces to `0 > 0` and the item can never
/// certify here — which is not the same claim as "the mismatch did not run", and
/// the tier sentence in the same verdict says it did.
const P5_WHY_NO_MISMATCH: &str = "its second half is the receiver's frame-error counter, read with that same Linux-only TIOCGICOUNT — the mismatched traffic was transmitted, but nothing on this kernel can witness the counter, so the item reads false whatever the wire did";

/// `Some(why)` when this build cannot measure the driver input counters at all,
/// so the two certificate items that read them are excused from the rig's tally
/// by the platform rather than by the cable. One helper, so the two call sites
/// cannot disagree about which platform that is.
fn p5_icounts_unmeasurable(why: &'static str) -> Option<&'static str> {
    (!sys::ICOUNTS_SUPPORTED).then_some(why)
}

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
    /// `Some(mechanism)` when this kernel gives no way to measure the item at
    /// all, so it did not *fail*: no cable, adapter or re-seat can make it pass
    /// here. The reason rides on the failure rather than being matched out of
    /// `item` in the verdict, so the excuse can never widen to an item that
    /// genuinely failed on a path this kernel does measure (§7, notes §3.45 E).
    unmeasurable_here: Option<&'static str>,
}

impl CertFailure {
    /// Prefix the item with the port (or pair) it was measured on, so the verdict
    /// line names *which* rig element failed — the whole point of naming the
    /// asymmetry in the discovery half (§15.21).
    fn qualified(self, subject: &str) -> CertFailure {
        CertFailure {
            item: format!("{subject}: {}", self.item),
            ..self
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
                // A port that would not reopen is a rig state every kernel can
                // observe: it is uncharacterized, not unmeasurable.
                unmeasurable_here: None,
            }],
        }
    }

    /// A characterization deliberately not attempted — the non-UART (CI sim) arm.
    /// Records **no** failure: §15.21 specifies characterization reporting skipped
    /// on non-UARTs precisely so P5's logic never waits for a bench.
    fn skipped(reason: &str) -> Self {
        Certificate::new(format!("skipped ({reason})"))
    }

    /// Record `item` as failed when `failed`, with its consequence class and —
    /// the fourth argument, which every site must answer rather than default —
    /// whether this kernel could have measured it at all (`None` = it could, so
    /// the failure is the rig's).
    fn fail_if(
        &mut self,
        failed: bool,
        item: &str,
        integrity: bool,
        unmeasurable_here: Option<&'static str>,
    ) {
        if failed {
            self.failures.push(CertFailure {
                item: item.to_owned(),
                integrity,
                unmeasurable_here,
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
    p5_port_certificate(custom_baud_ok, break_ok, &modem, icounter)
}

/// Fold one port's raw measurements into its certificate. Pure, split out for the
/// reason [`p5_verdict`] was: the part that must be tested is the *classification*
/// — which failures are the rig's and which are the kernel's — and it cannot be
/// reached through [`p5_certify_port`] without a bench. A pts is not a substitute:
/// [`p5_is_uart`] rejects one on both kernels (`TIOCMGET` and `TIOCGICOUNT` both
/// answer `ENOTTY` there — measured over a socat pair on Linux 7.0, where P5
/// reports `skipped (not a UART)`), so the caller never runs and a pts-driven
/// guard would pass vacuously everywhere (§9).
fn p5_port_certificate(
    custom_baud_ok: bool,
    break_ok: bool,
    modem: &str,
    icounter: bool,
) -> Certificate {
    let mut cert = Certificate::new(format!(
        "custom_baud={custom_baud_ok} break={break_ok} modem[{modem}] icounter={icounter}"
    ));
    // None of these is a data-integrity failure: the port carries bytes, but a
    // checklist tier that leans on a nonstandard rate, on break reception, or on
    // the driver counters would be running uncertified (§15.21) → degrade, named.
    cert.fail_if(!custom_baud_ok, "custom_baud", false, None);
    cert.fail_if(!break_ok, "break", false, None);
    // The counters are the one item whose *measurability* is a platform fact.
    cert.fail_if(
        !icounter,
        "icounter",
        false,
        p5_icounts_unmeasurable(P5_WHY_NO_ICOUNTER),
    );
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
///
/// The second return value is the **rate ladder's** own verdict, hoisted out of
/// the certificate because a second reader needs it and the certificate line is
/// prose. P14 (§15.51) refuses to search for a ceiling over a pair whose ladder
/// did not round-trip at 9600: a ceiling measured through a rig that corrupts
/// bytes at the bottom of the range is a measurement of the wiring, not of the
/// clocks. It is `false` on both early returns, where the ladder did not
/// complete at all — "did not pass" and "did not run" are the same instruction
/// to a probe that needs it to have passed.
fn p5_certify_pair(port_a: &Path, port_b: &Path) -> (Certificate, bool) {
    // Rate ladder: reconfigure both ports to each rate and exchange a nonce.
    let rates = [9600u32, 115_200, CUSTOM_BAUD];
    let mut ladder_ok = true;
    for &baud in &rates {
        let (Ok(a), Ok(b)) = (
            p5_open(port_a, baud, Parity::None),
            p5_open(port_b, baud, Parity::None),
        ) else {
            return (
                Certificate::unavailable("pair reopen failed", "pair_reopen"),
                false,
            );
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
            cert.fail_if(!ladder_ok, "rate_ladder", true, None);
            return (cert, false);
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
    let mut cert = p5_pair_certificate(ladder_ok, mismatch_observed);
    // Reaching here means the bulk mismatch pattern was written to the wire —
    // which is the claim P11 needs, independent of whether it was *observed* to
    // corrupt anything. The two early returns above never get here, and that is
    // the distinction: a certificate that bailed on a reopen transmitted nothing.
    cert.mismatch_transmitted = true;
    (cert, ladder_ok)
}

/// How long a driven modem line is given to reach the far port before it is read.
/// A USB adapter's control transfer and the peer's `TIOCMGET` are separate round
/// trips over the bus; without a pause the read races the write and a wired line
/// reads unwired.
const P5_MODEM_SETTLE: Duration = Duration::from_millis(60);

/// **Handshake continuity across a verified pair — reported, never judged**
/// (§15.52).
///
/// Every modem read in the certificate above happens with the peer port *closed*,
/// so it cannot answer what the wire carries; this one holds both ports open and
/// drives one end while reading the other. It exists because a hardware session
/// measured the bench rig directly and found it to be a **5-wire crossover** —
/// RTS↔CTS cross-wired in both directions, DTR moving nothing — where the tree had
/// assumed the 3-wire link §5 names as the common case (notes §3.53 i). That was a
/// capability the operator's own rig had and no report mentioned.
///
/// **The DTR arm is the in-probe negative control, not decoration.** On a rig where
/// RTS→CTS crosses and DTR→DSR does not, a `read_cts` that returned a constant, and
/// a rig with every line bridged to every other, both fail it — so the CTS reading
/// cannot be satisfied by an instrument that is not looking. Both polarities are
/// driven for the same reason at the level below: a line stuck high passes a
/// one-polarity test.
///
/// It adds no certificate item and reaches no verdict. §15.21's rule about the
/// modem map applies unchanged: **not wired is a valid answer**, a 3-wire rig is
/// the design's own stated assumption, and an item that degraded every honest one
/// would report the operator's cabling as a fault.
fn p5_handshake(port_a: &Path, port_b: &Path) -> String {
    let (Ok(a), Ok(b)) = (
        p5_open(port_a, 115_200, Parity::None),
        p5_open(port_b, 115_200, Parity::None),
    ) else {
        return "unavailable (pair would not reopen for the handshake read)".to_owned();
    };
    // Both ends at rest before anything is driven, so a level left over from the
    // certificate's own opens cannot be read as a crossing.
    for p in [&a, &b] {
        let _ = p.set_rts(false);
        let _ = p.set_dtr(false);
    }
    std::thread::sleep(P5_MODEM_SETTLE);

    /// Drive `line` on `tx` to both levels and report whether `read` on `rx`
    /// followed. `false` for a line that does not move, `?` for a read that
    /// errored — which is a third answer and must not print as either of the
    /// other two.
    fn crosses(
        set: impl Fn(bool) -> std::io::Result<()>,
        read: impl Fn() -> std::io::Result<bool>,
    ) -> String {
        let mut seen = Vec::new();
        for level in [true, false] {
            if set(level).is_err() {
                return "?".to_owned();
            }
            std::thread::sleep(P5_MODEM_SETTLE);
            match read() {
                Ok(v) => seen.push(v),
                Err(_) => return "?".to_owned(),
            }
        }
        let _ = set(false);
        // Followed both levels, or followed neither. A line that reads `true` at
        // both is stuck, not wired, and prints as `stuck-high`.
        match (seen[0], seen[1]) {
            (true, false) => "true".to_owned(),
            (false, false) => "false".to_owned(),
            (true, true) => "stuck-high".to_owned(),
            (false, true) => "inverted".to_owned(),
        }
    }

    p5_handshake_line(&HandshakeCells {
        rts_ab: crosses(|v| a.set_rts(v), || b.read_cts()),
        rts_ba: crosses(|v| b.set_rts(v), || a.read_cts()),
        dtr_ab_dsr: crosses(|v| a.set_dtr(v), || b.read_dsr()),
        dtr_ab_dcd: crosses(|v| a.set_dtr(v), || b.read_cd()),
        dtr_ab_ri: crosses(|v| a.set_dtr(v), || b.read_ri()),
        dtr_ba_dsr: crosses(|v| b.set_dtr(v), || a.read_dsr()),
        // **B→A's DCD and RI, the two crossings the verdict used to assert
        // without measuring** (notes §3.73). Until 2026-08-07 the line printed
        // "DTR moves nothing" from four cells — A→B against DSR/DCD/RI but B→A
        // against DSR alone — so a rig that wired B's DTR to A's DCD read
        // `false` on every cell it published and the sentence claimed a
        // negative it had never asked about in that direction. Character-for-
        // character mirrors of the two A→B lines above with `a`/`b` swapped.
        dtr_ba_dcd: crosses(|v| b.set_dtr(v), || a.read_cd()),
        dtr_ba_ri: crosses(|v| b.set_dtr(v), || a.read_ri()),
    })
}

/// The eight crossings the handshake line is computed from.
///
/// A struct rather than eight positional `&str`: the fold is the place a
/// transposed argument would be invisible, and `clippy::too_many_arguments`
/// fires at eight anyway. Every field is one `crosses()` reading —
/// `"true"`, `"false"`, `"stuck-high"` or `"inverted"`.
struct HandshakeCells {
    rts_ab: String,
    rts_ba: String,
    dtr_ab_dsr: String,
    dtr_ab_dcd: String,
    dtr_ab_ri: String,
    dtr_ba_dsr: String,
    dtr_ba_dcd: String,
    dtr_ba_ri: String,
}

/// The handshake line's shape, split out pure so the wiring vocabulary is
/// testable without a bench — the reason [`p5_pair_certificate`] is split the same
/// way. It classifies, it does not judge: every value it can produce is a
/// legitimate rig.
fn p5_handshake_line(c: &HandshakeCells) -> String {
    let HandshakeCells {
        rts_ab,
        rts_ba,
        dtr_ab_dsr,
        dtr_ab_dcd,
        dtr_ab_ri,
        dtr_ba_dsr,
        dtr_ba_dcd,
        dtr_ba_ri,
    } = c;
    let both_rts = rts_ab == "true" && rts_ba == "true";
    // **All six DTR crossings, not four.** "DTR moves nothing" is a claim about
    // both directions against all three inputs; computing it from a subset made
    // the verdict stronger than its measurement (notes §3.73).
    let any_dtr = [
        dtr_ab_dsr, dtr_ab_dcd, dtr_ab_ri, dtr_ba_dsr, dtr_ba_dcd, dtr_ba_ri,
    ]
    .iter()
    .any(|v| v.as_str() == "true");
    // The name a reader wants first, then the cells it is computed from — so a
    // half-crossed handshake is named rather than left to be spotted in six
    // fields, exactly as discovery names a half-crossed data pair.
    let shape = match (both_rts, rts_ab == "true" || rts_ba == "true", any_dtr) {
        (true, _, true) => "wired: RTS/CTS both ways and at least one DTR line",
        (true, _, false) => "5-wire crossover: RTS/CTS both ways, DTR moves nothing",
        (false, true, _) => "HALF-CROSSED handshake: RTS/CTS carries one way only",
        (false, false, true) => "DTR wired, RTS/CTS not",
        (false, false, false) => "3-wire: no handshake lines carried",
    };
    format!(
        "{shape} [rts_a_to_cts_b={rts_ab} rts_b_to_cts_a={rts_ba} \
         dtr_a_to_dsr_b={dtr_ab_dsr} dtr_a_to_dcd_b={dtr_ab_dcd} \
         dtr_a_to_ri_b={dtr_ab_ri} dtr_b_to_dsr_a={dtr_ba_dsr} \
         dtr_b_to_dcd_a={dtr_ba_dcd} dtr_b_to_ri_a={dtr_ba_ri}]"
    )
}

/// The pair half of [`p5_port_certificate`], pure for the same reason.
fn p5_pair_certificate(ladder_ok: bool, mismatch_observed: bool) -> Certificate {
    let mut cert = Certificate::new(format!(
        "rate_ladder={ladder_ok} deliberate_mismatch_observed={mismatch_observed}"
    ));
    // The ladder is the integrity item: a rung that did not round-trip means the
    // rig itself corrupts or loses data, so no tier failure measured through it is
    // attributable to serial_nexus (§15.21) — a stop condition, not a footnote.
    // It is measurable on every kernel: it reads bytes, not counters.
    cert.fail_if(!ladder_ok, "rate_ladder", true, None);
    // An unobserved deliberate mismatch means the error counters are not
    // observable on this rig: the data path is fine, the characterization is not.
    // Where the counters cannot be read at all, that reading is the platform's,
    // not the rig's — `false` there is `0 > 0`, not a result.
    cert.fail_if(
        !mismatch_observed,
        "deliberate_mismatch",
        false,
        p5_icounts_unmeasurable(P5_WHY_NO_MISMATCH),
    );
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

/// One cross-paired rig element, carried out of P5 by **path** rather than by
/// count — the thing [`RigFacts`] deliberately does not hold.
///
/// `RigFacts` answers "what kind of rig is this" for a *verdict*; P14 (§15.51)
/// has to actually transmit on the pair, so it needs the two ports themselves.
/// Splitting it out rather than growing `RigFacts` keeps that type `Copy` and
/// keeps the by-value threading through `p5_tier_scope`/`p5_verdict`/
/// `p11_line_state` untouched — a `Vec` field there would have rewritten
/// twenty-five call sites to buy nothing those readers use.
///
/// Both `both_uart` and `baseline_ok` are carried rather than recomputed,
/// because recomputing either means reopening the ports and asking again, and
/// the answer P14 must act on is the one *P5 measured* — the same rule
/// [`RigFacts::mismatch_pairs`] exists for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPair {
    pub a: PathBuf,
    pub b: PathBuf,
    /// Resolver identities (or raw paths), so a P14 row survives a renumbering
    /// exactly as P5's observation rows do.
    pub a_name: String,
    pub b_name: String,
    /// Whether **both** ports answered [`p5_is_uart`]. Discovery pairs a
    /// software null modem perfectly well — `serial-nexus-sim nullmodem` reads
    /// `discovered_pairs: 1` — so a probe that gates on the pair count alone
    /// runs its ceiling search against a pts, where every rate "passes" because
    /// there is no clock to miss, and reports a confident wire number with no
    /// wire. This flag is what makes that arm skip instead.
    pub both_uart: bool,
    /// Whether the pair's rate ladder round-tripped in both directions at all
    /// three of P5's rates — §15.51's "after the certificate's baseline
    /// integrity has passed".
    pub baseline_ok: bool,
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

pub fn p5_rig(
    ports: &[PathBuf],
    resolver: &serial_nexus_core::Resolver,
) -> (Probe, RigFacts, Vec<VerifiedPair>) {
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
            Vec::new(),
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
    // Every pair discovery verified, carried out by path for P14 (§15.51) —
    // *including* the ones that fail the UART gate or the ladder below. A probe
    // that must skip needs to know the pair exists in order to say why it
    // skipped; handing it only the pairs that already qualify would turn "this
    // rig is a software null modem" into an indistinguishable silence.
    let mut verified: Vec<VerifiedPair> = Vec::new();
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
            let subject =
                p5_pair_subject(&p5_name(&ports[i], resolver), &p5_name(&ports[j], resolver));
            p = p.observe(
                format!("{subject} cert").as_str(),
                "unavailable (pair would not reopen for characterization)",
            );
            failures.push(
                CertFailure {
                    item: "pair_reopen".to_owned(),
                    integrity: false,
                    // A pair that would not reopen is a rig state every kernel
                    // can observe: uncharacterized, not unmeasurable.
                    unmeasurable_here: None,
                }
                .qualified(&subject),
            );
            continue;
        };
        let mut baseline_ok = false;
        if a_uart && b_uart {
            let subject =
                p5_pair_subject(&p5_name(&ports[i], resolver), &p5_name(&ports[j], resolver));
            let (cert, ladder_ok) = p5_certify_pair(&ports[i], &ports[j]);
            baseline_ok = ladder_ok;
            if cert.mismatch_transmitted {
                mismatch_pairs += 1;
            }
            p = p.observe(format!("{subject} cert").as_str(), cert.line);
            failures.extend(cert.failures.into_iter().map(|f| f.qualified(&subject)));
            // §15.52 — handshake continuity, on its own key and folded into no
            // verdict. It runs after the certificate so a pair that could not be
            // characterized is not asked a second question it cannot answer
            // either, and it is the only modem read in this probe taken with the
            // peer port *open*, which is what makes it about the wire.
            p = p.observe(
                format!("{subject} handshake").as_str(),
                p5_handshake(&ports[i], &ports[j]).as_str(),
            );
        }
        verified.push(VerifiedPair {
            a: ports[i].clone(),
            b: ports[j].clone(),
            a_name: p5_name(&ports[i], resolver),
            b_name: p5_name(&ports[j], resolver),
            both_uart: a_uart && b_uart,
            baseline_ok,
        });
    }

    let facts = RigFacts {
        discovered_pairs,
        mismatch_pairs,
        loopbacks,
    };
    let (status, consequence) = p5_verdict(clean, any_uart, &failures, &hung_up, facts);
    (p.verdict(status, &consequence), facts, verified)
}

/// The tier sentence: **what the certificate covers** — the topology discovery
/// found, plus whether the pair items reached the wire.
///
/// Extracted from [`p5_verdict`]'s certified arm because it is not a property of
/// the verdict at all. Sequencing the only site that named a tier behind "and no
/// certificate item failed" is what kept the word Tier out of a Darwin report
/// whose cross-wired FT232R pair had just certified `rate_ladder=true` over
/// physical silicon: the tier is a discovery fact and the certificate is a
/// separate one, which is exactly the distinction [`RigFacts`]'s two counts were
/// added to preserve (§15.21, notes §3.42/§3.45 E).
fn p5_tier_scope(facts: RigFacts) -> String {
    match facts.tier() {
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
    }
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
///
/// The **tier** is not part of that precedence: it is what discovery saw, so the
/// uncertified arm names it too. It stays out of the miswired and hung-up arms,
/// where discovery is exactly what is in doubt.
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
    // §7 wants the observation named, and "uncertified" is not one. An item this
    // kernel cannot measure did not *fail* — no cable, adapter or re-seat makes it
    // pass here — so listing it beside items that really did fail sends a Darwin
    // operator chasing a rig fault no Darwin box can produce (notes §3.45 E (i);
    // the pre-widening text said "TIOCGICOUNT, which is Linux-only" and the
    // widened one said nothing). Grouped by mechanism in first-seen order, so a
    // second mechanism gets its own sentence without touching this fold and the
    // clauses follow the item list. Read off the failure rather than matched out
    // of the item name, so the excuse can never widen to an item that failed on a
    // path this kernel does measure.
    let mut unmeasurable: Vec<(&str, Vec<&str>)> = Vec::new();
    for f in failures.iter().filter(|f| !f.integrity) {
        if let Some(why) = f.unmeasurable_here {
            match unmeasurable.iter_mut().find(|(w, _)| *w == why) {
                Some((_, items)) => items.push(f.item.as_str()),
                None => unmeasurable.push((why, vec![f.item.as_str()])),
            }
        }
    }
    let structural = if unmeasurable.is_empty() {
        String::new()
    } else {
        let mut s = String::new();
        for (why, items) in &unmeasurable {
            let items = items.join(", ");
            s.push_str(&format!(
                " {items} cannot be measured on this kernel at all: {why}."
            ));
        }
        s.push_str(
            " That is the platform, not the rig: re-seating a cable cannot change it, and those items certify only on a Linux box (§13's best-effort tier).",
        );
        s
    };
    // Built once. Two arms below print this clause and they must keep printing the
    // same one as it grows — it grew today, and duplicated text is how the two
    // halves of a consequence line drift apart.
    let also = if uncertified.is_empty() {
        String::new()
    } else {
        format!(" Also uncertified: {}.{structural}", uncertified.join(", "))
    };

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
        // No tier is named here or in the `hung_up` arm below, deliberately: the
        // tier is what *discovery* saw, and in these two arms discovery is exactly
        // what is in doubt. Naming one would be the §9 proxy in space — a topology
        // word standing in for a topology nobody established.
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
        return (
            Status::Degraded,
            format!(
                "A port's peer hung up while P5 was probing it ({}) — it could not be classified, so the rig is not certified. Re-seat or re-open the peer and re-run P5 before any tier (§15.21).{also}",
                hung_up.join(", ")
            ),
        );
    }
    if !uncertified.is_empty() {
        // **The tier belongs here too, and this is where it went missing.** Both
        // discovery gates returned above, so the topology at this point is exactly
        // as established as it is in the certified arm below — it was never the
        // certificate that made it knowable, which is the whole reason `RigFacts`
        // carries `discovered_pairs` and `mismatch_pairs` as two counts. Leaving
        // the only tier-naming site behind this early return meant a rig could
        // certify `rate_ladder=true` over a physical crossover and never have its
        // tier printed: `grep -c "Tier [0-9]"` is 0 over all three 2026-08-05
        // Darwin captures, whose sole differing input against the Linux triple of
        // the same binary is this list (§3.42's pre-registration, §3.45 E).
        let topology = if any_uart {
            format!(" Topology: {}", p5_tier_scope(facts))
        } else {
            // Nothing certified, so there is no certificate to scope and no tier
            // is claimed — the `else` arm below owns that case's wording.
            String::new()
        };
        return (
            Status::Degraded,
            format!(
                "The rig carries data, but is not fully characterized ({items}) — a tier leaning on that item would be running uncertified (§15.21).{structural} Everything else above is certified.{topology}",
                items = uncertified.join(", ")
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
        let scope = p5_tier_scope(facts);
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
        // **One sentence on both kernels, and the arm's subject is the port.**
        // This used to be a `cfg` pair whose non-Linux half said "the UART
        // predicate is TIOCGICOUNT, which is Linux-only, so no port certifies
        // here however real it is … run the certificate on a Linux box". That
        // was true of the predicate §15.47 replaced and is false of the one this
        // build ships: `p5_is_uart` is `TIOCMGET || TIOCGICOUNT`, a pts fails
        // both on both kernels, and the committed
        // `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json` shows the same
        // FTDI pair certifying `rate_ladder=true` on Darwin. Reaching this arm
        // means every named port answered *neither* ioctl, which is a statement
        // about those ports and not about the kernel — so the advice is the same
        // everywhere, and a Darwin operator is no longer told to go and find real
        // adapters they are already holding.
        let why = "characterization skipped — no named port answered the UART predicate (TIOCMGET or TIOCGICOUNT), which is what a pts sim looks like on every kernel. Discovery and pairing above are still measured, and the certificate populates on real adapters (§13, no-target doctrine; §15.47 for the portable predicate)";
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
// P14 — the maximum-rate search (§15.51). Opt-in behind --port exactly like
// P3/P5/P11, and additionally gated on a cross-paired rig, because it transmits
// for tens of seconds at rates a live target would not survive being handed.
// ---------------------------------------------------------------------------

/// P14's question, verbatim. One site today — P14 reports its own skip rather
/// than needing a placeholder in `main` — but the const is here for the reason
/// [`P3_QUESTION`]'s doc comment gives: the string feeds `probe_set`, so a
/// second site that ever appears must be unable to word it differently.
const P14_QUESTION: &str = "On a P5-verified cross-paired rig, what is the highest baud rate at which a seeded payload still round-trips byte-exact in both directions, and what stopped the search (§15.51)?";

/// The ladder's **fixed body**: the standard rates, then the divisor-friendly
/// family where real adapter clocks actually land.
///
/// Plain bisection is ruled out by two hardware facts (§15.51): achievable rates
/// are quantized by each adapter's divisor model, so most of the integer range
/// between two rungs consists of rates the hardware rounds to something else;
/// and reliability is not guaranteed monotone in the requested rate. A ladder
/// walks the points that exist.
const P14_LADDER_BODY: [u32; 16] = [
    9_600, 19_200, 38_400, 57_600, 115_200, 230_400, 460_800, 921_600, 1_000_000, 1_500_000,
    2_000_000, 3_000_000, 4_000_000, 6_000_000, 8_000_000, 12_000_000,
];

/// The structural cap: the largest rate the configuration field can spell.
///
/// It is not a number invented here. `core/src/config.rs` range-checks a serial
/// node's `baud` against `1 ..= u32::MAX`, so this is §15.34's stated maximum
/// for the very attribute the operator sets — which is what makes a
/// `structural-cap` answer mean "the instrument ran out", not "the wire did".
const P14_MAX_BAUD: u32 = u32::MAX;

/// Three byte-exact round-trips per direction, per §15.51's reliability bar.
const P14_TRIALS_PER_DIRECTION: u32 = 3;

/// At most four requested midpoints between the last reliable and the first
/// unreliable rate. Bounded because refinement buys precision on a quantized
/// axis, where past a few steps every midpoint lands on the same divisor.
const P14_MAX_REFINEMENTS: u32 = 4;

/// Target airtime per payload, so the reliability bar is the same at 9600 as at
/// 3 Mbaud rather than shrinking with the rate as a constant byte count would.
const P14_AIRTIME_MS: u64 = 250;

/// 8N1 — one start bit, eight data, one stop — so a byte costs ten bit times.
/// The divisor that turns a rate into a payload size.
const P14_BITS_PER_BYTE: u64 = 10;

/// Floor and cap on the payload. The floor keeps a very low rate from being
/// judged on a handful of bytes; the cap bounds memory and the per-trial
/// deadline, and above the rate where it binds the airtime shrinks — which the
/// report says, rather than continuing to claim a constant.
const P14_PAYLOAD_FLOOR: usize = 64;
const P14_PAYLOAD_CAP: usize = 64 * 1024;

/// The rate the search starts from, restores to, and re-proves on the way out —
/// P5's own ladder rate, so the rig is left exactly as the certificate needs it.
const P14_BASELINE_BAUD: u32 = 115_200;

/// How many payload-lengths of slack the reader accepts before calling a trial
/// corrupt rather than continuing to wait. A garbled leading byte shifts the
/// payload without losing it, which is why the judgement is `contains_sub` over
/// a slack window rather than a prefix comparison.
const P14_READ_SLACK: usize = 64;

/// A hard wall-clock stop on the whole search. The ladder is finite by
/// construction, so this fires only on pathology — and when it does the search
/// is **incomplete**, which the fold refuses to dress up as a measured ceiling.
const P14_BUDGET: Duration = Duration::from_secs(180);

/// What one rung's trials came to.
///
/// `Corrupt` and `TimedOut` are separated because a stall and a loss are
/// different facts and §6's deadline discipline forbids reading one as the
/// other — the same rule that makes the sim stamp `timed_out`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RungOutcome {
    Passed,
    Corrupt,
    TimedOut,
    /// The set call succeeded and the driver landed on a different rate. The
    /// adapter's divisor model answered.
    AdapterRefused,
    /// The set call itself failed, with an errno. The platform's ask surface
    /// answered, before any byte moved.
    PlatformRefused,
    /// The peer went away mid-search (EIO/ENXIO/ENODEV). Not a ceiling.
    HungUp,
}

impl RungOutcome {
    fn passed(self) -> bool {
        matches!(self, RungOutcome::Passed)
    }

    fn label(self) -> &'static str {
        match self {
            RungOutcome::Passed => "passed",
            RungOutcome::Corrupt => "corrupt",
            RungOutcome::TimedOut => "timed-out",
            RungOutcome::AdapterRefused => "adapter-refused",
            RungOutcome::PlatformRefused => "platform-refused",
            RungOutcome::HungUp => "hung-up",
        }
    }
}

/// One entry of the search history: what was asked for, and what came back.
///
/// This is the whole input to both pure functions below, which is why it holds
/// no port, no handle and no clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RateTrial {
    requested: u32,
    outcome: RungOutcome,
}

/// Why the search stopped — a reason, never a grade (§15.51).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CeilingKind {
    /// A higher rate failed its trials, with the failure separated.
    UnreliableCorrupt,
    UnreliableTimedOut,
    AdapterRefused,
    PlatformRefused,
    HungUp,
    /// Every rate the 32-bit field can spell passed. The instrument's own limit,
    /// and it says so instead of implying the wire's.
    StructuralCap,
}

impl CeilingKind {
    fn label(self) -> &'static str {
        match self {
            CeilingKind::UnreliableCorrupt => "unreliable-corrupt",
            CeilingKind::UnreliableTimedOut => "unreliable-timed-out",
            CeilingKind::AdapterRefused => "adapter-refused",
            CeilingKind::PlatformRefused => "platform-refused",
            CeilingKind::HungUp => "hung-up",
            CeilingKind::StructuralCap => "structural-cap",
        }
    }

    fn from_outcome(o: RungOutcome) -> Option<CeilingKind> {
        match o {
            RungOutcome::Passed => None,
            RungOutcome::Corrupt => Some(CeilingKind::UnreliableCorrupt),
            RungOutcome::TimedOut => Some(CeilingKind::UnreliableTimedOut),
            RungOutcome::AdapterRefused => Some(CeilingKind::AdapterRefused),
            RungOutcome::PlatformRefused => Some(CeilingKind::PlatformRefused),
            RungOutcome::HungUp => Some(CeilingKind::HungUp),
        }
    }
}

/// Whether `rate` is a rung of the ladder itself — the fixed body, or a rung of
/// the open end above it.
///
/// The open end is *computed*, not listed, which is what makes "the probe's own
/// list is never the ceiling" true: it is `12_000_000 << k` for every `k` that
/// fits, plus [`P14_MAX_BAUD`] itself as the final clamped step. Refinement
/// midpoints are exactly the rates this returns `false` for, which is how
/// [`p14_next_rate`] counts them without being told.
fn p14_is_ladder_rate(rate: u32) -> bool {
    if rate == P14_MAX_BAUD || P14_LADDER_BODY.contains(&rate) {
        return true;
    }
    let top = P14_LADDER_BODY[P14_LADDER_BODY.len() - 1] as u64;
    let mut r = top;
    while r <= P14_MAX_BAUD as u64 {
        if r == rate as u64 {
            return true;
        }
        r *= 2;
    }
    false
}

/// The highest rate this history has seen round-trip byte-exact.
///
/// A `max` over **every** pass rather than a walk from the end, which is what
/// makes a non-monotone history safe: a rung that failed below a passing higher
/// one lowers nothing.
fn p14_highest_pass(history: &[RateTrial]) -> Option<u32> {
    history
        .iter()
        .filter(|t| t.outcome.passed())
        .map(|t| t.requested)
        .max()
}

/// The lowest failure that bounds `floor` from above — the other half of the
/// bracket, and `None` when nothing above the floor has failed.
fn p14_lowest_failure_above(history: &[RateTrial], floor: Option<u32>) -> Option<RateTrial> {
    history
        .iter()
        .filter(|t| !t.outcome.passed())
        .filter(|t| floor.is_none_or(|f| t.requested > f))
        .min_by_key(|t| t.requested)
        .copied()
}

/// **The next-rate decision — pure, and the half of P14 a bench cannot test.**
///
/// Phase 1 climbs while nothing above the floor has failed: the first
/// unattempted body rung, then the open end, doubling from the body's top. The
/// open end terminates *by construction* rather than by hope — the final
/// doubling is clamped to [`P14_MAX_BAUD`], and once that has been attempted
/// there is nothing left to propose, so this function can never return a rate a
/// `u32` cannot hold.
///
/// Phase 2 refines: once some failure bounds the highest pass from above, it
/// bisects that bracket, at most [`P14_MAX_REFINEMENTS`] times, stopping when
/// the bracket is too narrow to hold a new rate. Refinements are counted as the
/// attempted rates that are not ladder rungs, so the caller keeps no state the
/// history does not already carry.
///
/// `None` means the search is over.
fn p14_next_rate(history: &[RateTrial]) -> Option<u32> {
    let floor = p14_highest_pass(history);
    let bound = p14_lowest_failure_above(history, floor);

    let Some(bound) = bound else {
        // Phase 1 — climb.
        for &rung in &P14_LADDER_BODY {
            if !history.iter().any(|t| t.requested == rung) {
                return Some(rung);
            }
        }
        // The open end. `max` rather than `last`, so the proposal cannot go
        // backwards if a history ever arrives out of order.
        let highest = history.iter().map(|t| t.requested).max()?;
        if highest == P14_MAX_BAUD {
            return None;
        }
        let doubled = (highest as u64) * 2;
        return Some(if doubled >= P14_MAX_BAUD as u64 {
            P14_MAX_BAUD
        } else {
            doubled as u32
        });
    };

    // Phase 2 — refine between the highest pass and the lowest failure above it.
    let floor = floor?;
    let refinements = history
        .iter()
        .filter(|t| !p14_is_ladder_rate(t.requested))
        .count() as u32;
    if refinements >= P14_MAX_REFINEMENTS {
        return None;
    }
    if bound.requested.saturating_sub(floor) <= 1 {
        return None;
    }
    let mid = floor + (bound.requested - floor) / 2;
    if mid <= floor || mid >= bound.requested || history.iter().any(|t| t.requested == mid) {
        return None;
    }
    Some(mid)
}

/// **The fold — the number, and its reason for stopping.**
///
/// `max_reliable_baud` is the highest rate that passed, over the whole history.
/// The kind is read off the failure that bounds it from above; when no failure
/// bounds it, the search either exhausted the ladder — which is exactly
/// `structural-cap` — or stopped early, and the second case returns `None`
/// rather than borrowing the first case's name. **The absence of a reason is
/// not a fifth reason**: a truncated search must not be able to print the most
/// impressive answer in the taxonomy, so the verdict degrades on `None`.
fn p14_ceiling(history: &[RateTrial]) -> (Option<u32>, Option<CeilingKind>) {
    let floor = p14_highest_pass(history);
    match p14_lowest_failure_above(history, floor) {
        Some(bound) => (floor, CeilingKind::from_outcome(bound.outcome)),
        None if floor == Some(P14_MAX_BAUD) => (floor, Some(CeilingKind::StructuralCap)),
        None => (floor, None),
    }
}

/// The measured inputs P14's verdict is folded from. Copy, small, and holding
/// no handle — the [`P12Facts`] shape, for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct P14Facts {
    /// A port was named at all.
    ports_named: bool,
    /// Discovery verified a cross-paired rig.
    pair_present: bool,
    /// Both of that pair's ports answered [`p5_is_uart`].
    both_uart: bool,
    /// P5's rate ladder round-tripped on that pair — §15.51's precondition.
    baseline_ok: bool,
    max_reliable_baud: Option<u32>,
    ceiling_kind: Option<CeilingKind>,
    /// The way out: the baseline rate re-applied, and one round-trip re-proven
    /// at it.
    baseline_restored: bool,
    baseline_reproved: bool,
    /// Whether the wall-clock stop fired. **It reaches the verdict, and that was
    /// a repair**: `P14_BUDGET`'s own comment promised "the fold refuses to dress
    /// up an incomplete search as a measured ceiling", and the fold could not
    /// see it. A budget that expired straight after a *failing* rung — with up
    /// to four refinements still owed — left a bounded floor and a named kind, so
    /// `p14_ceiling` answered and the verdict read `supported` over a search that
    /// had stopped early. The `ceiling_kind.is_none()` arm only ever covered the
    /// case where *no* failure bounds the floor.
    search_budget_exhausted: bool,
}

/// **The verdict — and it never grades the number.**
///
/// `supported` whenever the measurement completed, whatever the ceiling: a rig
/// that tops out at 115200 is slow, not broken, which is P13's rule that a probe
/// reports rather than judges. `skipped` without an opted-in verified UART pair.
/// `degraded` only where the question could not be *asked* — baseline integrity
/// failed under it, the search did not complete, or the closing restore did not
/// round-trip. Never `unsupported`: no answer here contradicts a design premise
/// with no fallback, and the gate refuses that word (`expectations/*.jq`).
fn p14_verdict(f: P14Facts) -> (Status, String) {
    if !f.ports_named {
        return (
            Status::skipped("no --port named"),
            "Re-run with both of the rig's --ports. P14 transmits at every rate it tries, so it is opt-in exactly as P3/P5/P11 are (§13, §15.51).".to_owned(),
        );
    }
    if !f.pair_present {
        return (
            Status::skipped("no verified cross-paired rig"),
            "The ceiling is a property of two independently clocked UARTs talking to each other; a dangling converter and a TX↔RX jumper share one clock and cannot answer it. Cross-wire two adapters and name both with --port (§13's Tier 3).".to_owned(),
        );
    }
    if !f.both_uart {
        return (
            Status::skipped(P5_UNCHARACTERIZED),
            "The pair discovery found carries bytes, but neither port answers the UART predicate, so there is no line rate to search for — a software null modem passes every rate because nothing clocks it. The plumbing above ran; the claim did not (§15.51).".to_owned(),
        );
    }
    if !f.baseline_ok {
        return (
            Status::Degraded,
            "P5's rate ladder did not round-trip on this pair, so a ceiling measured through it would be measuring the wiring rather than the clocks — the search was not run. Fix the rig until P5 certifies `rate_ladder=true`, then re-run (§15.21, §15.51).".to_owned(),
        );
    }
    if f.search_budget_exhausted {
        return (
            Status::Degraded,
            "The search hit its wall-clock budget before the ladder was exhausted, so whatever floor the rungs above establish is a floor over an *interrupted* set of rates and not a ceiling. Re-run on a quieter box, or read the rungs directly (§15.51).".to_owned(),
        );
    }
    if f.max_reliable_baud.is_none() || f.ceiling_kind.is_none() {
        return (
            Status::Degraded,
            "The search did not complete: no rate was established as reliable, or it stopped without a recorded reason (the rungs above say which). An incomplete search has no ceiling to report, and reporting one anyway is the failure this arm exists to prevent (§15.51).".to_owned(),
        );
    }
    if f.ceiling_kind == Some(CeilingKind::HungUp) {
        // **`RungOutcome::HungUp`'s own doc says "Not a ceiling", and this arm is
        // what makes that true of the code.** It used to fold to a `supported`
        // ceiling like any other stop, so an adapter that vanished mid-search
        // produced a confident number bounded by the rig leaving the bench.
        // A vanished peer means the question could not be finished being asked,
        // which is precisely what this probe's `degraded` is for.
        return (
            Status::Degraded,
            "A port's peer went away mid-search (EIO/ENXIO/ENODEV), so the search was bounded by the rig disappearing rather than by a rate — whatever passed below is a floor over the rungs that ran before the disconnection, and not a ceiling. Re-seat the adapter and re-run (§15.51).".to_owned(),
        );
    }
    if !f.baseline_restored || !f.baseline_reproved {
        return (
            Status::Degraded,
            format!(
                "The ceiling was measured ({}), but the rig was not returned to its baseline rate with a proven round-trip (restored={}, re-proved={}) — so the number stands and the rig's state does not. Re-seat or re-open the pair and re-run P5 before any tiered item (§15.51).",
                f.max_reliable_baud
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "none".into()),
                f.baseline_restored,
                f.baseline_reproved,
            ),
        );
    }
    let baud = f.max_reliable_baud.unwrap_or(0);
    let kind = f.ceiling_kind.map(CeilingKind::label).unwrap_or("unknown");
    let why = match f.ceiling_kind {
        Some(CeilingKind::UnreliableCorrupt) => {
            "the next rate transmitted and the bytes came back wrong — a loss, separated from a stall because they are different facts"
        }
        Some(CeilingKind::UnreliableTimedOut) => {
            "the next rate transmitted and the bytes did not come back inside the deadline — a stall, separated from a loss because they are different facts"
        }
        Some(CeilingKind::AdapterRefused) => {
            "the next rate was accepted by the ask and the driver landed somewhere else, which the requested-versus-actual cells above name; the adapter's divisor model is the limit"
        }
        Some(CeilingKind::PlatformRefused) => {
            "the set call itself failed with an errno, so this is a fact about the platform's ask surface and **not** about the wire — a different kernel may ask for more over the same cable (§15.47)"
        }
        Some(CeilingKind::HungUp) => {
            "a port's peer went away mid-search, so the search was bounded by the rig disappearing rather than by a rate"
        }
        Some(CeilingKind::StructuralCap) => {
            "every rate the 32-bit configuration field can spell round-tripped, so this names the instrument's own limit and not the wire's"
        }
        None => "no reason was recorded",
    };
    (
        Status::Supported,
        format!(
            "Maximum reliable rate {baud} baud on this pair; the search stopped because {why} (`ceiling_kind={kind}`). Configure a serial node above that rate and you are past what was measured here. Read the number as a **floor over the probed set** under the stated trial policy — {P14_TRIALS_PER_DIRECTION} byte-exact constant-airtime round-trips per direction — never as a promise about rates the ladder skipped, sustained throughput, longer cables, or other temperatures."
        ),
    )
}

/// The payload size for a rate: [`P14_AIRTIME_MS`] of airtime at 8N1, clamped.
fn p14_payload_len(baud: u32) -> usize {
    let bytes = (baud as u64 * P14_AIRTIME_MS) / (1000 * P14_BITS_PER_BYTE);
    (bytes as usize).clamp(P14_PAYLOAD_FLOOR, P14_PAYLOAD_CAP)
}

/// A payload nobody can satisfy with a leftover. The head names the rate, the
/// direction and the trial, so a stale buffer from the previous rung cannot
/// match, and the tail is a cheap LCG so the bytes exercise every bit position
/// rather than repeating a short cycle a divisor error might survive.
fn p14_payload(baud: u32, dir: &str, trial: u32, len: usize) -> Vec<u8> {
    let head = format!("\x02SNX-P14-{baud}-{dir}-{trial}\x03");
    let mut v: Vec<u8> = Vec::with_capacity(len.max(head.len()));
    v.extend_from_slice(head.as_bytes());
    let mut x: u32 = baud ^ (trial.wrapping_mul(0x9E37_79B9)) ^ (dir.as_bytes()[0] as u32);
    while v.len() < len {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        v.push((x >> 24) as u8);
    }
    v
}

/// Which surface refused, read off the error rather than off its message.
///
/// serial2 collapses two very different failures into one `Err`: a `tcsetattr`
/// that the kernel rejected, and its own post-set verification finding the
/// driver landed more than 2.5% from the ask. They are separable without
/// matching on the crate's error *string* — which would be a promise the crate
/// never made — because only the first carries an errno: the syscall wrapper
/// builds its error from `last_os_error`, the verifier builds a bare
/// `ErrorKind::Other`. So `raw_os_error().is_some()` means *the ask was refused
/// before any byte moved* (the platform), and its absence means *the ask was
/// accepted and the clock landed elsewhere* (the adapter).
///
/// **What the read-back does and does not corroborate, measured rather than
/// assumed.** An earlier version of this comment said the read-back corroborates
/// the classification "by a number rather than resting on this rule alone". On
/// the platform of record it does not: `ftdi_sio` reports back the rate it was
/// *asked* for, so every rung from 115200 to 3000000 on the bench rig reads back
/// exactly the request — including 2500000 and 2750000, which the adapter
/// actually runs at ~1.9 Mbaud. The read-back therefore corroborates the
/// `adapter-refused` arm, where the driver *does* answer with a different number
/// (4000000 reads back 9600), and says nothing at all about a rate it accepted.
/// The cell that speaks to the accepted rates is `achieved_baud_floor`, timed
/// from the trials themselves.
fn p14_refusal(e: &std::io::Error) -> RungOutcome {
    // **A vanished peer is not a ceiling, and it used to print as one.** An
    // adapter unplugged during a rate change answers `EIO`, which carries an
    // errno, so the rule below would have called it `platform-refused` — and the
    // verdict would then have told the operator that *this kernel's ask surface*
    // stops here and a different kernel might ask for more over the same cable.
    // The cable was gone. `is_hangup` is the predicate P5 already uses for
    // exactly this distinction; consulting it first is the whole repair.
    if is_hangup(e) {
        return RungOutcome::HungUp;
    }
    match e.raw_os_error() {
        Some(_) => RungOutcome::PlatformRefused,
        // **Errno-less does not mean the adapter answered.** serial2's own
        // post-set verification builds a bare `ErrorKind::Other`, which is the
        // adapter case this rule was written for — but its `set_baud_rate`
        // *fallback* arm, selected on every unix target that is neither
        // Apple/BSD nor Linux, refuses an unlisted rate with an errno-less
        // `InvalidInput` before any syscall is made. Calling that "the adapter's
        // divisor model is the limit" would blame silicon that was never told
        // the number. Neither of this project's two platforms takes that arm, so
        // this clause is unreachable here and is written from serial2's source
        // rather than from a measurement — which is why it is a *narrow* match on
        // the kind rather than a broadening of the rule.
        None if e.kind() == std::io::ErrorKind::InvalidInput => RungOutcome::PlatformRefused,
        None => RungOutcome::AdapterRefused,
    }
}

/// Ask one port for a rate and report what the driver **says** it is running.
///
/// The read-back happens on **both** paths, success and refusal alike: a refusal
/// whose actual rate nobody read is a refusal nobody can explain, and §15.46
/// makes the instrument testify to its own configuration. But "says" is the
/// operative word and the doc used to read "is actually running" — measured on
/// the bench rig, `ftdi_sio` echoes the requested number for every rate it
/// accepts, whether or not its divisor can produce it. The read-back is the
/// driver's answer, which is a different thing from the wire's.
fn p14_apply_rate(
    sp: &mut SerialPort,
    baud: u32,
) -> (Option<RungOutcome>, Option<i32>, Option<u32>) {
    let readback = |sp: &SerialPort| sp.get_configuration().and_then(|c| c.get_baud_rate()).ok();
    let mut settings = match sp.get_configuration() {
        Ok(s) => s,
        Err(e) => return (Some(p14_refusal(&e)), e.raw_os_error(), None),
    };
    if let Err(e) = settings.set_baud_rate(baud) {
        let (o, n) = (p14_refusal(&e), e.raw_os_error());
        return (Some(o), n, readback(sp));
    }
    match sp.set_configuration(&settings) {
        Ok(()) => (None, None, readback(sp)),
        Err(e) => {
            let (o, n) = (p14_refusal(&e), e.raw_os_error());
            (Some(o), n, readback(sp))
        }
    }
}

/// Read whatever is already buffered and throw it away, so a rung's trial is
/// judged on its own bytes. Bounded: a port that never goes quiet stops at the
/// window rather than holding the search.
fn p14_flush(sp: &SerialPort, window: Duration) {
    let until = Instant::now() + window;
    while Instant::now() < until {
        match p5_read_result(sp) {
            Ok(got) if got.is_empty() => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

/// What one direction's one trial came to.
#[derive(Debug, Clone, Copy, Default)]
struct TrialResult {
    written: u64,
    received: u64,
    byte_exact: bool,
    hung_up: bool,
    /// How long this trial's payload took to arrive, from the first write to the
    /// moment the whole payload was matched. Only meaningful when `byte_exact`.
    elapsed_us: u64,
}

impl TrialResult {
    /// **Why this trial failed, decided here rather than inferred from sums.**
    ///
    /// The classification used to live in the caller and compare *totals over up
    /// to three trials* against a *single* payload length — and because a
    /// direction short-circuits on its first failure, an intermittent rung
    /// failing on trial 2 or 3 (the shape a marginal rate actually produces) had
    /// already banked one or two clean payloads. A total stall on trial 3 then
    /// read `bytes_received == 2 x payload`, which is neither short nor starved,
    /// and folded to `Corrupt`. The identical failure on trial 1 classified
    /// correctly. §6 forbids reading a stall as a loss, and this is the shape
    /// that was doing it.
    fn failure(&self, payload_len: u64) -> Option<RungOutcome> {
        if self.hung_up {
            return Some(RungOutcome::HungUp);
        }
        if self.byte_exact {
            return None;
        }
        // A short write is a transmit-side stall; too few bytes back is a
        // receive-side one. Enough bytes, wrong bytes, is the only loss.
        if self.written < payload_len || self.received < payload_len {
            Some(RungOutcome::TimedOut)
        } else {
            Some(RungOutcome::Corrupt)
        }
    }
}

/// One round trip, **writing and reading concurrently**.
///
/// The concurrency is not a refinement, it is the measurement. A
/// write-the-whole-payload-then-read shape was tried first on the crossover rig
/// and reported a ceiling of 250000 baud on hardware that is byte-exact to
/// 3000000: at and above 460800 the payload outruns the receiver's buffer while
/// the sender is still writing, so the loss is the harness's and the number is
/// the harness's too. Polling both directions is what the daemon does (§5), and
/// it is the only shape whose failures belong to the wire.
fn p14_trial(tx: &SerialPort, rx: &SerialPort, payload: &[u8], deadline: Duration) -> TrialResult {
    let start = Instant::now();
    let mut sent = 0usize;
    let mut got: Vec<u8> = Vec::with_capacity(payload.len() + P14_READ_SLACK);
    let mut hung_up = false;
    let ceiling = payload.len() + P14_READ_SLACK;
    while start.elapsed() < deadline {
        let mut pfds = Vec::with_capacity(2);
        pfds.push(PollFd::new(rx.as_fd(), PollFlags::POLLIN));
        if sent < payload.len() {
            pfds.push(PollFd::new(tx.as_fd(), PollFlags::POLLOUT));
        }
        let _ = poll(&mut pfds, PollTimeout::from(20u16));
        let readable = pfds[0].revents().unwrap_or(PollFlags::empty());
        if readable.intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR) {
            match p5_read_result(rx) {
                Ok(chunk) => got.extend_from_slice(&chunk),
                Err(e) => {
                    if is_hangup(&e) {
                        hung_up = true;
                    }
                    break;
                }
            }
        }
        if sent < payload.len()
            && pfds
                .get(1)
                .and_then(|f| f.revents())
                .is_some_and(|r| r.contains(PollFlags::POLLOUT))
        {
            match tx.write(&payload[sent..]) {
                Ok(0) => {}
                Ok(n) => sent += n,
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    if is_hangup(&e) {
                        hung_up = true;
                    }
                    break;
                }
            }
        }
        if got.len() >= payload.len() && contains_sub(&got, payload) {
            break;
        }
        if got.len() >= ceiling {
            break;
        }
    }
    let byte_exact = contains_sub(&got, payload);
    TrialResult {
        written: sent as u64,
        received: got.len() as u64,
        byte_exact,
        hung_up,
        elapsed_us: start.elapsed().as_micros() as u64,
    }
}

/// One direction of one rung: up to [`P14_TRIALS_PER_DIRECTION`] trials, short-
/// circuiting on the first failure.
///
/// Three clean trials are what makes a rung reliable; one failure is already
/// enough to make it not, so the remaining trials would buy nothing but time at
/// the one rung that always costs the most. `trials_run` is reported beside
/// `trials_passed` so the short circuit is visible rather than inferred.
#[derive(Debug, Clone, Copy, Default)]
struct DirectionResult {
    measured: bool,
    trials_run: u32,
    trials_passed: u32,
    bytes_sent: u64,
    bytes_received: u64,
    byte_exact: bool,
    hung_up: bool,
    elapsed_us: u64,
    /// The failing trial's own classification, `None` while every trial passed.
    failure: Option<RungOutcome>,
    /// **What the wire actually ran at, as a floor** — payload bits divided by
    /// the fastest clean trial's wall clock, so poll and syscall overhead can
    /// only push it *down*. `None` when no trial completed.
    ///
    /// It is here because the driver's read-back turned out not to answer this
    /// question. Measured on an FT232R over the bench crossover: every rate from
    /// 115200 to 3000000 reads back **exactly** what was asked, and the achieved
    /// rate tracks it at a steady ~0.94 of the request (this instrument's
    /// overhead) — **except at 2500000 and 2750000, where it collapses to 0.76
    /// and 0.70, both landing at ~1.9 Mbaud**. The adapter is rounding to its
    /// nearest supported divisor and `tcgetattr` is reporting the *requested*
    /// number back. The bytes are still byte-exact, because both ends are
    /// mis-set identically and therefore agree with each other.
    ///
    /// So `max_reliable_baud` is the highest rate that **round-trips when both
    /// ends are asked for it**, which is what an operator configuring a port
    /// gets — and it is not necessarily the rate on the wire. This cell is what
    /// lets a reader see the difference instead of being told a number that
    /// hides it (§15.46).
    achieved_baud_floor: Option<u64>,
}

impl DirectionResult {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            // A `0` that says what it means: an unmeasured direction and a
            // direction that carried nothing are different facts (§15.49).
            "measured": self.measured,
            "trials_run": self.trials_run,
            "trials_passed": self.trials_passed,
            "bytes_sent": self.bytes_sent,
            "bytes_received": self.bytes_received,
            "byte_exact": self.byte_exact,
            "hung_up": self.hung_up,
            "elapsed_us": self.elapsed_us,
            "failure": self.failure.map(RungOutcome::label),
            "achieved_baud_floor": self.achieved_baud_floor,
        })
    }
}

fn p14_direction(
    tx: &SerialPort,
    rx: &SerialPort,
    baud: u32,
    dir: &str,
    payload_len: usize,
    deadline: Duration,
) -> DirectionResult {
    let mut d = DirectionResult {
        measured: true,
        byte_exact: true,
        ..Default::default()
    };
    let started = Instant::now();
    let mut fastest_clean_us: Option<u64> = None;
    for trial in 0..P14_TRIALS_PER_DIRECTION {
        p14_flush(rx, Duration::from_millis(30));
        let payload = p14_payload(baud, dir, trial, payload_len);
        let r = p14_trial(tx, rx, &payload, deadline);
        d.trials_run += 1;
        d.bytes_sent += r.written;
        d.bytes_received += r.received;
        d.hung_up |= r.hung_up;
        if r.byte_exact {
            d.trials_passed += 1;
            fastest_clean_us = Some(match fastest_clean_us {
                Some(best) => best.min(r.elapsed_us),
                None => r.elapsed_us,
            });
        } else {
            d.byte_exact = false;
            // The failing trial classifies itself, while the payload it failed
            // on is still in scope. The caller no longer has to reconstruct it
            // from sums that a short circuit has already made ambiguous.
            d.failure = r.failure(payload_len as u64);
            break;
        }
    }
    d.elapsed_us = started.elapsed().as_micros() as u64;
    // A floor, not a rate: the fastest clean trial's wall clock includes this
    // probe's own poll loop, so overhead can only make the number smaller than
    // the wire's. Read a *large* gap from the request, not a small one — on the
    // bench rig a healthy rung sits at ~0.94 of the ask and a rounded-down one at
    // 0.70.
    d.achieved_baud_floor = fastest_clean_us
        .filter(|us| *us > 0)
        .map(|us| (payload_len as u64 * P14_BITS_PER_BYTE * 1_000_000) / us);
    d
}

/// One measured rung, with everything the report prints about it.
struct Rung14 {
    requested: u32,
    actual_a: Option<u32>,
    actual_b: Option<u32>,
    refusal_errno: Option<i32>,
    phase: &'static str,
    outcome: RungOutcome,
    ab: DirectionResult,
    ba: DirectionResult,
    frame_delta: Option<u64>,
    overrun_delta: Option<u64>,
    parity_delta: Option<u64>,
}

impl Rung14 {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "requested_baud": self.requested,
            // `null`, never a sentinel: a rate nobody could read back is not a
            // rate of zero, and the two must not print the same.
            "actual_baud_a": self.actual_a,
            "actual_baud_b": self.actual_b,
            "refusal_errno": self.refusal_errno,
            "phase": self.phase,
            "outcome": self.outcome.label(),
            "ab": self.ab.observations(),
            "ba": self.ba.observations(),
            // Linux-only (TIOCGICOUNT). `null` off Linux, with the mechanism
            // named once at the probe level rather than repeated per rung — the
            // §15.47 treatment: unmeasurable is data, not a bare zero.
            "frame_delta": self.frame_delta,
            "overrun_delta": self.overrun_delta,
            "parity_delta": self.parity_delta,
        })
    }
}

/// Read the driver's input-error counters, where the kernel has them.
fn p14_icounts(sp: &SerialPort) -> Option<(u64, u64, u64)> {
    sys::read_icounts(sp.as_raw_fd())
        .ok()
        .map(|c| (c.frame as u64, c.overrun as u64, c.parity as u64))
}

/// Measure one rung: apply the rate to both ports, settle, and run both
/// directions. A refused rate never reaches the trials — there is nothing to
/// measure at a rate the hardware is not running.
fn p14_measure_rung(
    a: &mut SerialPort,
    b: &mut SerialPort,
    rate: u32,
    phase: &'static str,
) -> Rung14 {
    let (refusal_a, errno_a, actual_a) = p14_apply_rate(a, rate);
    let (refusal_b, errno_b, actual_b) = p14_apply_rate(b, rate);
    // **The outcome and its errno are taken from the same port.** They used to be
    // two independent `or`s, so a rig whose port A was `adapter-refused` (no
    // errno) and whose port B was `platform-refused` (errno) printed
    // `outcome: "adapter-refused"` beside a non-null `refusal_errno` — precisely
    // the pairing the discriminator declares impossible, in the artifact, for a
    // reader to trip over. Plausible on a mixed-adapter rig.
    let (refusal, refusal_errno) = match (refusal_a, refusal_b) {
        (Some(o), _) => (Some(o), errno_a),
        (None, Some(o)) => (Some(o), errno_b),
        (None, None) => (None, None),
    };
    let mut rung = Rung14 {
        requested: rate,
        actual_a,
        actual_b,
        refusal_errno,
        phase,
        outcome: RungOutcome::Passed,
        ab: DirectionResult::default(),
        ba: DirectionResult::default(),
        frame_delta: None,
        overrun_delta: None,
        parity_delta: None,
    };
    if let Some(o) = refusal {
        rung.outcome = o;
        return rung;
    }
    // §15.25's post-set settle, on both ends. Its absence was this project's one
    // genuine hardware bug: an FTDI transmits the first bytes after a rate change
    // at a transitional rate.
    std::thread::sleep(P5_OPEN_SETTLE);

    let payload_len = p14_payload_len(rate);
    let airtime_us = (payload_len as u64 * P14_BITS_PER_BYTE * 1_000_000) / (rate.max(1) as u64);
    let deadline = Duration::from_micros(airtime_us * 4) + Duration::from_millis(500);

    let before = (p14_icounts(a), p14_icounts(b));
    rung.ab = p14_direction(a, b, rate, "AB", payload_len, deadline);
    rung.ba = p14_direction(b, a, rate, "BA", payload_len, deadline);
    let after = (p14_icounts(a), p14_icounts(b));
    if let (Some(a0), Some(b0), Some(a1), Some(b1)) = (before.0, before.1, after.0, after.1) {
        // Saturating, because the kernel's counters are `i32` and a driver that
        // wrapped or reset one between the two reads would otherwise underflow a
        // `u64` — a debug panic, or ~1.8e19 printed as a frame-error count in
        // release. A counter that went backwards is not a negative number of
        // errors; it is a measurement this rung cannot make, and `0` beside the
        // byte counts is the honest floor.
        rung.frame_delta = Some(a1.0.saturating_sub(a0.0) + b1.0.saturating_sub(b0.0));
        rung.overrun_delta = Some(a1.1.saturating_sub(a0.1) + b1.1.saturating_sub(b0.1));
        rung.parity_delta = Some(a1.2.saturating_sub(a0.2) + b1.2.saturating_sub(b0.2));
    }

    // Each direction's failing trial has already classified itself against its
    // own payload (see `TrialResult::failure`); the rung is the worse of the two.
    // A hangup outranks everything — it means the rig left, not that a rate was
    // reached.
    rung.outcome = match (rung.ab.failure, rung.ba.failure) {
        (Some(RungOutcome::HungUp), _) | (_, Some(RungOutcome::HungUp)) => RungOutcome::HungUp,
        (Some(o), _) => o,
        (None, Some(o)) => o,
        (None, None) => RungOutcome::Passed,
    };
    rung
}

/// One port's answer to "did the flow-control mode I asked for actually take?".
struct FlowReadback {
    port: String,
    /// Did `tcsetattr` report success? A driver that *refuses* is not the
    /// interesting case — it is honest. The interesting one accepts and drops.
    /// **This cell is what separates them**, and it is why the probe can make
    /// §7.1's three-way call from the two cells here: with the flag reading back
    /// clear, a failed set is the honest refusal and a successful one is the
    /// defect (`silently_dropped`).
    tcsetattr_ok: bool,
    tcsetattr_error: Option<String>,
    cflag_before: u64,
    cflag_after: u64,
    /// The whole point: `CRTSCTS` present in the read-back.
    honoured: bool,
    /// Does `serial_nexus_sys::honours_rtscts` — the predicate the daemon's `load`
    /// consults — agree with the reading beside it, **as an arm of §7.1's three-way
    /// call rather than as a read-back**? `None` when that predicate could not run
    /// (an unreadable port is not a disagreement). A `false` here means the report
    /// and the daemon would answer differently about the same port, which is worse
    /// than either answer.
    shipped_predicate_agrees: Option<bool>,
    /// Set back to what it was, and checked — **both flag words**, by the probe's
    /// last read of the port. A probe that reconfigures a real port and cannot say
    /// it restored it is a probe nobody should run twice.
    restored: bool,
    /// The **software** half of the same question, taken through the same open and
    /// the same restore (plan §18 item 14). `Err` carries why it could not be
    /// taken: unmeasurable is data, not absence (§15.47).
    soft: Result<SoftFlowReadback, String>,
}

/// One port's answer to "did the *software* flow-control mode I asked for take?".
///
/// **Why this rides on P15 rather than taking a probe id of its own.** It is the
/// same open, the same restore and one more flag pair (plan §18 item 14): a
/// second probe would have to re-open a port P11 has just promised to leave
/// alone, and would double the reconfiguration this one already performs.
///
/// It shipped one commit ahead of the `question` string that describes it, and
/// that gap was deliberate and is now closed (§15.59). While it stood, the block's
/// own `asks` cell carried the sub-question so a reader of the JSON never had to
/// infer it from the header — kept, because a self-describing block is worth
/// having whether or not the header agrees with it. The reasoning for the delay is
/// worth keeping too: an era boundary refuses every cross-era diff, and spending
/// one on a wording change while P16's real instrument change was already owed
/// would have closed two eras where one would do.
struct SoftFlowReadback {
    /// Did `tcsetattr` report success? Same discriminator as the hardware half:
    /// with the flags reading back clear, a failed set is an honest refusal and a
    /// successful one is the accept-then-drop defect.
    tcsetattr_ok: bool,
    tcsetattr_error: Option<String>,
    iflag_before: u64,
    iflag_after: u64,
    /// The two flags, read back **separately**, because a driver may take one and
    /// drop the other and `serial2`'s `FlowControl::XonXoff` sets both.
    ixon: bool,
    ixoff: bool,
    /// The property the *product* promises, not a proxy for it (AGENTS §9):
    /// `serial2` compares the whole `c_iflag` word it asked for against the one it
    /// reads back (`Settings::matches_requested`), so this — not the two flags
    /// alone — is what decides whether the serial node's open turns into
    /// `failed to apply some or all settings` on a `faulted` node.
    iflag_matches_request: bool,
}

impl SoftFlowReadback {
    fn honoured(&self) -> bool {
        self.ixon && self.ixoff
    }

    fn silently_dropped(&self) -> bool {
        self.tcsetattr_ok && !self.honoured()
    }
}

/// The sub-question the software block answers, in the report itself.
const P15_SOFT_ASKS: &str = "Does this port honour a requested SOFTWARE flow-control mode (IXON|IXOFF) on read-back, or accept the request and silently drop it?";

/// What a software-flow-control reading does **not** license, printed beside it.
///
/// §7.1 clause 7 and plan §18 item 14: `xon-xoff` has no `load`-time pre-check,
/// and the ledger item declines adding one until a dropping driver is actually
/// found. So this reading moves no verdict and refuses no config; it exists so
/// that "unmeasured, not known-good" stops being the honest statement.
const P15_SOFT_DOES_NOT_LICENSE: &str = "reported, never judged: nothing in the daemon consults this reading, and a `flow_control = \"xon-xoff\"` config is refused at neither `load` nor `add-node` whatever it says. §15.53's refusal covers `rts-cts` only, and extending it to a mode no artifact had measured would be policy without evidence (plan §18 item 14). What a `silently_dropped: true` here would mean is that such a node faults late, at its own open, with `serial2`'s bare `failed to apply some or all settings` — the outcome the `rts-cts` refusal exists to prevent.";

impl FlowReadback {
    fn observations(&self) -> serde_json::Value {
        serde_json::json!({
            "requested": "rts-cts (CRTSCTS)",
            "tcsetattr_ok": self.tcsetattr_ok,
            "tcsetattr_error": self.tcsetattr_error,
            "cflag_before_hex": format!("{:#x}", self.cflag_before),
            "cflag_after_hex": format!("{:#x}", self.cflag_after),
            "honoured_on_readback": self.honoured,
            "shipped_predicate_agrees": self.shipped_predicate_agrees,
            "silently_dropped": self.tcsetattr_ok && !self.honoured,
            "baseline_restored": self.restored,
            "software_flow_control": match &self.soft {
                Ok(s) => serde_json::json!({
                    "asks": P15_SOFT_ASKS,
                    "measured": true,
                    "requested": "xon-xoff (IXON|IXOFF set, CRTSCTS cleared — serial2's FlowControl::XonXoff transform)",
                    "tcsetattr_ok": s.tcsetattr_ok,
                    "tcsetattr_error": s.tcsetattr_error,
                    "iflag_before_hex": format!("{:#x}", s.iflag_before),
                    "iflag_after_hex": format!("{:#x}", s.iflag_after),
                    "ixon_on_readback": s.ixon,
                    "ixoff_on_readback": s.ixoff,
                    "honoured_on_readback": s.honoured(),
                    "silently_dropped": s.silently_dropped(),
                    "serial2_readback_would_fault": !s.iflag_matches_request,
                    "does_not_license": P15_SOFT_DOES_NOT_LICENSE,
                }),
                Err(e) => serde_json::json!({
                    "asks": P15_SOFT_ASKS,
                    "measured": false,
                    "unmeasurable_here": e,
                    "does_not_license": P15_SOFT_DOES_NOT_LICENSE,
                }),
            },
        })
    }
}

/// Widen a termios flag word to `u64` for the report, portably.
///
/// **`tcflag_t` is `u32` on Linux and `u64` on Darwin**, so a cast is *required* on
/// one platform and *redundant* on the other — and clippy is correct both times:
/// `as u64` trips `unnecessary_cast` on Darwin, `u64::from` trips
/// `useless_conversion` there, and dropping the widening breaks Linux. A generic
/// bound is the one spelling that is right everywhere and needs no `#[allow]`,
/// because the conversion is resolved per target rather than written down.
///
/// Found by the Darwin lint cross-check (plan §18 item 54) within hours of that gate
/// landing — the *second* live instance of the class it was added for, and one no
/// Linux lane and no `cargo check --target` could have seen.
fn flag_bits<T: Into<u64>>(bits: T) -> u64 {
    bits.into()
}

/// Ask one port for `CRTSCTS` and read it back, then put it back as it was.
fn p15_readback(path: &PathBuf) -> Result<FlowReadback, String> {
    let fd = open(
        path,
        OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|e| format!("open: {e}"))?;
    let before = tcgetattr(&fd).map_err(|e| format!("tcgetattr: {e}"))?;
    let mut want = before.clone();
    want.control_flags |= ControlFlags::CRTSCTS;

    let set = tcsetattr(&fd, SetArg::TCSANOW, &want);
    let after = tcgetattr(&fd);

    // **Restore before inspecting the read-back, not after** (notes §3.68). The
    // read-back used to carry a `?`, which returns *between* the set and the
    // restore: on a kernel that honours the flag that leaves a real adapter with
    // `CRTSCTS` asserted, and the probe then reports `skipped (no port could be
    // opened)` — no `baseline_restored` cell in the JSON at all, so both gate
    // files' restore clause is exempted by the skip. The one error path that could
    // strand a port was the one path that emitted nothing about it.
    let restored_after_own_write = tcsetattr(&fd, SetArg::TCSANOW, &before).is_ok()
        && tcgetattr(&fd)
            .map(|t| t.control_flags == before.control_flags)
            .unwrap_or(false);

    let after = after.map_err(|e| format!("tcgetattr after: {e}"))?;

    // **The daemon's refusal and this report must never disagree.** The daemon
    // rejects a `flow_control = "rts-cts"` config at load time on a port that answers
    // `AcceptedThenDropped` to `sys::honours_rtscts`; if P15 measured the same thing
    // by its own code path, a drift between the two would let a report call a port
    // fine while `load` refuses it. So the shipped predicate is called here and its
    // answer is required to match the one this probe just read by hand — an operator
    // reading `honoured_on_readback` and `tcsetattr_ok` is reading the exact function
    // `load` consults (notes §3.67).
    //
    // **The comparison is over the three-way answer, not over the read-back**
    // (§7.1 clause 7, §15.53). It compared `honoured` alone until 2026-08-12, which
    // agreed by construction on the half that could not differ: an honest refusal
    // and an accept-then-drop both read the flag back clear, so the two sides could
    // classify the same port into different arms — the arm that decides whether
    // `load` refuses — and this field would still say `true`. The readings stay
    // independently taken (that is what this cross-check is for); only the rule
    // that turns two readings into an arm is shared, so a disagreement here is
    // about the *port*, never about the meaning of the arms.
    let honoured = after.control_flags.contains(ControlFlags::CRTSCTS);
    let by_hand = RtsCtsOutcome::classify(set.is_ok(), honoured);
    let shipped = serial_nexus_sys::honours_rtscts(path);
    let agrees = match &shipped {
        Ok(v) => Some(*v == by_hand),
        Err(_) => None,
    };

    // **The software half, on the same open** (plan §18 item 14). It runs *after*
    // `honours_rtscts` for the same reason the restore check does — that call is
    // an external write to this port — and before the final baseline verification,
    // so the last read below covers this write too.
    //
    // The transform is `serial2::FlowControl::XonXoff` applied to the baseline
    // this probe found, spelled out rather than delegated: `c_iflag |=
    // IXON|IXOFF` and `c_cflag &= !CRTSCTS`, which is exactly what the serial
    // node's open performs for `flow_control = "xon-xoff"`. Asking for anything
    // else would measure a request the daemon never makes.
    let soft = p15_soft_readback(&fd, &before);

    // **`honours_rtscts` and the software pass are the probe's last writes to this
    // port, so the baseline has to be re-read after them** (notes §3.68).
    // `honours_rtscts` opens the same tty a second time, sets `CRTSCTS`, and
    // restores — and it cannot verify its own restore, because it closes the fd it
    // would need to re-read through. Deciding `baseline_restored` before that call
    // published a `true` about a port whose final reconfiguration nothing had
    // checked, on the one kernel where the write takes effect. This fd is still
    // open, so the re-read costs one `tcgetattr` and makes the field describe the
    // port as this probe actually leaves it.
    //
    // **Both flag words, since 2026-08-12**: the software pass writes `c_iflag`,
    // and a restore check that read `c_cflag` alone would certify a port this
    // probe had left with `IXON` asserted. The strengthening is deliberate and
    // costs no extra syscall.
    let restored = restored_after_own_write
        && tcgetattr(&fd)
            .map(|t| t.control_flags == before.control_flags && t.input_flags == before.input_flags)
            .unwrap_or(false);

    Ok(FlowReadback {
        port: path.display().to_string(),
        tcsetattr_ok: set.is_ok(),
        tcsetattr_error: set.err().map(|e| e.to_string()),
        cflag_before: flag_bits(before.control_flags.bits()),
        cflag_after: flag_bits(after.control_flags.bits()),
        honoured,
        shipped_predicate_agrees: agrees,
        restored,
        soft,
    })
}

/// Ask one already-open port for `IXON|IXOFF` and read it back, then put the
/// termios back as it was.
///
/// Takes the baseline the caller already read rather than re-reading it, so both
/// halves of P15 restore to the *same* witness and a single final `tcgetattr`
/// verifies both (§13: the restore claim is verified by the probe's last read).
///
/// **The restore runs before any error is inspected**, exactly as the hardware
/// half's does (notes §3.68): a `?` between the set and the restore is how a
/// probe leaves a real adapter reconfigured and then reports `skipped`, which is
/// the word that exempts every conditional gate clause.
fn p15_soft_readback<Fd: AsFd>(
    fd: &Fd,
    before: &nix::sys::termios::Termios,
) -> Result<SoftFlowReadback, String> {
    use nix::sys::termios::InputFlags;

    let mut want = before.clone();
    want.input_flags |= InputFlags::IXON | InputFlags::IXOFF;
    want.control_flags &= !ControlFlags::CRTSCTS;

    let set = tcsetattr(fd, SetArg::TCSANOW, &want);
    let after = tcgetattr(fd);
    let _ = tcsetattr(fd, SetArg::TCSANOW, before);

    let after = after.map_err(|e| format!("tcgetattr after xon-xoff: {e}"))?;
    Ok(SoftFlowReadback {
        tcsetattr_ok: set.is_ok(),
        tcsetattr_error: set.err().map(|e| e.to_string()),
        iflag_before: flag_bits(before.input_flags.bits()),
        iflag_after: flag_bits(after.input_flags.bits()),
        ixon: after.input_flags.contains(InputFlags::IXON),
        ixoff: after.input_flags.contains(InputFlags::IXOFF),
        iflag_matches_request: after.input_flags == want.input_flags,
    })
}

/// The software-flow-control sentence appended to every arm that measured a port.
///
/// **The verdict now answers for this reading, and that changed at P16's landing**
/// (§15.59). A probe's verdict speaks for the question its `question` string asks;
/// P15's named `CRTSCTS` alone while the probe reported both, so between plan §18
/// item 14 and this commit the software reading was carried, stated and
/// deliberately *not* judged — the only honest arrangement while the header asked
/// a narrower question than the body answered. §15.59 folded the two moves into
/// one `probe_set` boundary: the `question` now names both kinds, so a `supported`
/// verdict over a silently-dropped software request would be the probe answering
/// `supported` to a question it answered *no* to. [`p15_verdict`] therefore
/// degrades on it — ranked below the hardware drop, because that one has a shipped
/// consequence (`load` refuses, §15.53) and this one deliberately has none.
///
/// **What did not change is the daemon.** Plan §18 item 14's decline stands: no
/// `xon-xoff` pre-check, no refusal at `load`, nothing in the product consults
/// this cell. A `degraded` here reports a driver difference (§7's rule) and does
/// not enact a policy, and every arm below says so in as many words.
///
/// The sentence is not optional decoration: `expectations/*.jq` reads
/// observations and never `.consequence`, and the digests read `(id, question)`
/// and leaf paths — so an operator-facing finding that lives only in a cell is a
/// finding nobody reads (§13's gate-blind-spot rule). The drop arm therefore leads
/// with the defect and names the ports.
fn p15_soft_note(rows: &[FlowReadback]) -> String {
    let dropped: Vec<&str> = rows
        .iter()
        .filter(|r| r.soft.as_ref().is_ok_and(|s| s.silently_dropped()))
        .map(|r| r.port.as_str())
        .collect();
    let refused: Vec<&str> = rows
        .iter()
        .filter(|r| {
            r.soft
                .as_ref()
                .is_ok_and(|s| !s.tcsetattr_ok && !s.honoured())
        })
        .map(|r| r.port.as_str())
        .collect();
    let unmeasured: Vec<&str> = rows
        .iter()
        .filter(|r| r.soft.is_err())
        .map(|r| r.port.as_str())
        .collect();
    let honoured = rows.len() - dropped.len() - refused.len() - unmeasured.len();

    let mut out = String::new();
    if !dropped.is_empty() {
        out.push_str(&format!(
            " **SOFTWARE flow control: {} named port(s) ACCEPTED an `IXON|IXOFF` request and then reported it clear** ({}) — measured on the same open (plan §18 item 14). `serial2` verifies `c_iflag` by read-back exactly as it verifies `c_cflag`, so a `flow_control = \"xon-xoff\"` node on such a port does **not** get §15.53's refusal at `load` — that refusal covers `rts-cts` only — and instead faults at its own open with the bare `failed to apply some or all settings` the refusal exists to prevent. **This degrades the verdict and refuses nothing**: since §15.59 widened this probe's question to name both flow-control kinds, a `supported` here would be answering `supported` to a question this port answered no to — but the daemon is unchanged, no pre-check consults this cell, and plan §18 item 14's decline stands until a design decision is taken on the evidence this row now supplies (`serial2_readback_would_fault` is the cell that decides the node's fate, because it compares the whole `c_iflag` word rather than the two flags alone).",
            dropped.len(),
            dropped.join(", ")
        ));
    }
    if !refused.is_empty() {
        out.push_str(&format!(
            " **SOFTWARE flow control: {} named port(s) REFUSED `IXON|IXOFF` outright** ({}) — `tcsetattr` failed rather than reporting success over a clear flag, which is the honest answer and the one nothing needs to act on.",
            refused.len(),
            refused.join(", ")
        ));
    }
    if honoured > 0 {
        out.push_str(&format!(
            " Software flow control (`xon-xoff`, `IXON|IXOFF`) was measured on the same open and {honoured} of {} named port(s) honoured it on read-back. The verdict answers for this half too since §15.59 widened the question — but the *daemon* does not: no pre-check consults it and no config is refused on it (plan §18 item 14's decline, unchanged).",
            rows.len()
        ));
    }
    if !unmeasured.is_empty() {
        out.push_str(&format!(
            " Software flow control could not be read back on {} — the cell says so rather than reading as an answer (unmeasurable is data, §15.47).",
            unmeasured.join(", ")
        ));
    }
    out
}

/// Fold the per-port readings into a verdict. Pure, so both readings are tested
/// against each other rather than against whichever kernel is in front of you
/// (§9) — the arm that does not run here is the one a single-kernel session
/// cannot exercise.
fn p15_verdict(named: usize, rows: &[FlowReadback], errors: &[String]) -> (Status, String) {
    if named == 0 {
        return (
            Status::skipped("no --port named"),
            "Re-run with `--port /dev/ttyUSB0` to ask whether this driver honours a requested flow-control mode (a dangling converter is enough — no target device, §13).".to_owned(),
        );
    }
    if rows.is_empty() {
        return (
            Status::skipped("no port could be opened"),
            format!(
                "No named port could be opened or configured ({}). Grant access and re-run with `--port`.",
                errors.join("; ")
            ),
        );
    }
    let dropped: Vec<&FlowReadback> = rows
        .iter()
        .filter(|r| r.tcsetattr_ok && !r.honoured)
        .collect();
    let unrestored: Vec<&FlowReadback> = rows.iter().filter(|r| !r.restored).collect();
    if !unrestored.is_empty() {
        return (
            Status::Degraded,
            format!(
                "This probe could not restore the pre-existing termios on {}. Nothing below should be trusted and the port should be reopened before use — a reconfigured adapter is a worse outcome than an unanswered question.",
                unrestored
                    .iter()
                    .map(|r| r.port.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    let disagreeing: Vec<&FlowReadback> = rows
        .iter()
        .filter(|r| r.shipped_predicate_agrees == Some(false))
        .collect();
    if !disagreeing.is_empty() {
        return (
            Status::Degraded,
            format!(
                "**This report and the daemon would answer differently about {}.**                  `serial_nexus_sys::honours_rtscts` is the predicate `load` consults to                  refuse a `flow_control = \"rts-cts\"` config, and it disagreed with the read-back                  this probe took by hand on the same port. Neither reading can be trusted                  until they are reconciled: a report that calls a port fine while `load`                  refuses it — or the reverse — is worse than either verdict on its own.{}",
                disagreeing
                    .iter()
                    .map(|r| r.port.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                p15_soft_note(rows)
            ),
        );
    }
    // **A port that REFUSED the request is not a port that honoured it, and the
    // verdict may not say it was** (§7.1 clause 7, §15.53). Both readings are
    // `supported` — an honest refusal is a legitimate driver answer and §7.1
    // deliberately does not refuse such a config at `load` — but they are different
    // facts, and this arm printed "Every named port honoured `CRTSCTS` on read-back"
    // over both until 2026-08-12. On a refusing port that sentence is simply false:
    // `honoured_on_readback` is `false` in the cell right beside it, which is the
    // shape §13 exists to prevent (a verdict a reader would act on, contradicted by
    // the observation under it). The probe already had the data — `tcsetattr_ok`
    // separates the two — and only the prose could not tell them apart.
    let refused: Vec<&FlowReadback> = rows
        .iter()
        .filter(|r| !r.tcsetattr_ok && !r.honoured)
        .collect();

    // **The software half now moves this verdict, ranked below the hardware one**
    // (§15.59). Until P16's landing the `question` string named `CRTSCTS` alone, so
    // the software reading was carried and stated and deliberately not judged —
    // the only honest arrangement while the header asked less than the body
    // answered. The widened question changes exactly that: `supported` over a
    // silently-dropped `IXON|IXOFF` request would be answering `supported` to a
    // question this port answered no to.
    //
    // Ranked *below* the `CRTSCTS` drop because that finding has a shipped
    // consequence an operator acts on — §15.53 refuses the config at `load` — and
    // this one has none by decision (plan §18 item 14's decline). The more
    // actionable finding leads, which is the same ordering rule the restore and
    // daemon-disagreement arms above follow.
    let soft_dropped: Vec<&FlowReadback> = rows
        .iter()
        .filter(|r| r.soft.as_ref().is_ok_and(|s| s.silently_dropped()))
        .collect();
    if dropped.is_empty() && !soft_dropped.is_empty() {
        return (
            Status::Degraded,
            format!(
                "**The hardware half of this question is fine here and the software half is not.** `CRTSCTS` is honoured or honestly refused on every named port, and {} of them accepted an `IXON|IXOFF` request and then reported it clear. The detail, and the bound on what it licenses, follow.{}",
                soft_dropped.len(),
                p15_soft_note(rows)
            ),
        );
    }

    if dropped.is_empty() && refused.is_empty() {
        return (
            Status::Supported,
            format!(
                "Every named port ({}) honoured `CRTSCTS` on read-back, so a `flow_control = \"rts-cts\"` edge configures here and the driver agrees it did. `serial2` verifies settings by reading them back, so this is exactly the check the serial node's open performs.{}",
                rows.len(),
                p15_soft_note(rows)
            ),
        );
    }
    if dropped.is_empty() {
        return (
            Status::Supported,
            format!(
                "**{} of {} named port(s) REFUSED the `CRTSCTS` request outright** ({}); the rest honoured it on read-back. A refusal is `tcsetattr` *failing* rather than reporting success and leaving the flag clear, and it is the honest answer: nothing about the request was silently discarded. **A `flow_control = \"rts-cts\"` config on such a port is therefore NOT refused at `load`/`add-node` (§7.1, §15.53)** — only the accept-then-drop driver is, because only that one would leave a link running without the flow control it asked for. The node's own open fails instead, loudly, carrying the driver's own error plus the flag, the port and both remedies: `flow_control = \"none\"` (or `xon-xoff`) for this port, or an adapter whose driver implements RTS/CTS. Read this beside `honoured_on_readback` and `tcsetattr_ok` in the cells below: those two together are the three-way discrimination this probe exists to make, and `silently_dropped` is `false` here.{}",
                refused.len(),
                rows.len(),
                refused
                    .iter()
                    .map(|r| r.port.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                p15_soft_note(rows)
            ),
        );
    }
    (
        Status::Degraded,
        format!(
            "**{} named port(s) ACCEPTED a `CRTSCTS` request and then reported it clear** ({}). `tcsetattr` returned success and `tcgetattr` read the flag back unset, so the driver neither honours the mode nor refuses it. **What the daemon does about it is decided and shipped (§15.53, notes §3.67): a node configured with `flow_control = \"rts-cts\"` on such a port is REFUSED at `load`/`add-node`, before anything is created** — not faulted later, and not silently run without the flow control it asked for. The refusal names the node, the device, the resolved path and the read-back, and offers two remedies: `flow_control = \"none\"` (or `xon-xoff`) for this port, or an adapter whose driver implements RTS/CTS. `serial-nexus-doctor --port <this port>` reports the same reading the daemon consults, and `shipped_predicate_agrees` below says whether the two agree on this box. The refusal is structural and creates **nothing** — §11's load is atomic, so a five-node file with one such port creates zero nodes, not four; what survives is the *running* graph, because the pre-check runs before any teardown, so a refused `load --replace` leaves what is already up untouched. Measured on Darwin 24.6.0 / macOS 15.7.8 with an FT232R on Apple's IOSerialFamily driver: `CCTS_OFLOW` alone, `CRTS_IFLOW` alone, both together, a blocking open and the `/dev/tty.*` node all behave identically, while a **pty on the same box honours the flag** — so it is the serial driver and not the tty layer. The wire is not the suspect either: RTS↔CTS crossing is independently proven on that rig. **Two paths still reach a `faulted` node rather than the refusal**, both because the pre-check could not open the port to measure it: a `load --replace` on a port the running graph already holds (filed, notes §3.68 (5a)), and an adapter that arrives *after* the config loads. On those the node's own open fails, and the fault now carries this same reading and remedy instead of `serial2`'s bare `failed to apply some or all settings`.{}",
            dropped.len(),
            dropped
                .iter()
                .map(|r| r.port.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            p15_soft_note(rows)
        ),
    )
}

/// **P15 — does a named port honour a requested flow-control mode, or accept the
/// request and drop it?**
///
/// §7.1 lets an edge ask for `rts-cts`, and the serial node applies it through
/// `serial2`, which **verifies by read-back**: it sets the termios, reads it back,
/// and errors when the two disagree. So a driver that silently ignores the request
/// does not produce a degraded link — it produced `failed to apply some or all
/// settings` and a `faulted` node, some time after `load` had already returned
/// success. That failure mode was found by a harness test on Darwin (notes §3.65 E)
/// and had no instrument behind it; this probe is the instrument, and it did its job:
/// the decision was taken against a committed artifact rather than a red test (§7).
///
/// **The decision is made, and this doc comment used to outlive it.** §15.53 / notes
/// §3.67: the daemon consults `sys::honours_rtscts` — the one predicate, which this
/// probe also calls and cross-checks as `shipped_predicate_agrees` — and **refuses**
/// an *accept-then-drop* config at `load`/`add-node`, before anything is created. Not
/// degrade: the thing degraded would be the transport's contract rather than an
/// observation, since an `rts-cts` edge exists because the far end needs the line
/// held. What §7 forbids is the operator learning nothing, and that was the *old*
/// behaviour. This probe now reports the shipped answer instead of describing the
/// question as open; the guard below pins that, because a stale consequence string is
/// invisible to every other gate and this one is read by operators on the platform it
/// is wrong about.
///
/// **Three answers, two of them `supported`, and the verdict must say which**
/// (§7.1 clause 7). A driver that *refuses* `CRTSCTS` outright is honest — §7.1 does
/// not refuse its config at `load`, because nothing runs silently without the flow
/// control it asked for; the node's own open fails loudly instead. It reads
/// `supported` here for that reason, and it gets its own named arm, because the arm
/// it used to share said "every named port honoured `CRTSCTS`" over a cell reading
/// `honoured_on_readback: false`. The probe always had the data — `tcsetattr_ok` is
/// the discriminator — so this cost no new observation key and moved neither digest.
///
/// **Why not an observation on P11.** P11 states its own contract — it opens with
/// the port's current settings unchanged and "inspects, it does not configure" —
/// and this question cannot be asked without configuring. A new question takes a
/// new id (the append-only rule above P13), which moves `probe_set` into a new era
/// deliberately, exactly as P14 did.
///
/// **The software half rides here** (plan §18 item 14). `xon-xoff` had no
/// pre-check and no probe: `serial2` verifies `c_iflag` by read-back exactly as it
/// verifies `c_cflag`, so a driver that accepted `IXON`/`IXOFF` and read them back
/// clear would fault a node with the same bare error §15.53's refusal exists to
/// prevent — and no artifact on any kernel said whether one does. It does now. The
/// reading is taken on **this** open, with **this** restore, and one more flag
/// pair, which is what makes it an observation rather than a second probe.
///
/// **It landed in two steps, and the second is the one to read** (§15.59). At
/// 2026-08-13 the reading shipped under a `question` string still naming `CRTSCTS`
/// alone, so it moved `field_set` and not `probe_set` and it deliberately did not
/// move the verdict — the only honest arrangement while the header asked less than
/// the body answered, and a wording change is not worth an era boundary of its own.
/// At P16's landing the two were folded into **one** `probe_set` move: the
/// `question` now names both flow-control kinds and [`p15_verdict`] degrades on a
/// silently-dropped software request, ranked below the `CRTSCTS` drop because that
/// one has a shipped consequence and this one has none. **The daemon is
/// unchanged** — plan §18 item 14's decline stands, no pre-check consults the cell,
/// and no config is refused on it; the verdict reports a driver difference (§7)
/// and enacts nothing. The other route by which the software pass moves the
/// verdict is `baseline_restored`, which covers both flag words: a port left with
/// `IXON` asserted is worse than any unanswered question.
///
/// Opt-in behind `--port` like P3/P5/P11/P14, and it restores the termios it
/// found, checked rather than assumed — **by the probe's last read of the port,
/// after both writes and after `honours_rtscts`'s own**.
pub fn p15_flow_control_readback(ports: &[PathBuf]) -> Probe {
    let mut p = Probe::new(
        "P15",
        "real-port flow-control honouring",
        "Does a named port honour a requested flow-control mode — hardware (CRTSCTS) or software (IXON/IXOFF) — on read-back, or accept the request and silently drop it (§7.1, §15.53)?",
    );
    let mut rows: Vec<FlowReadback> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for port in ports {
        match p15_readback(port) {
            Ok(r) => rows.push(r),
            Err(e) => errors.push(format!("{}: {e}", port.display())),
        }
    }
    for r in &rows {
        p = p.observe(&r.port, r.observations());
    }
    if !errors.is_empty() {
        p = p.observe("unreadable_ports", serde_json::json!(errors));
    }
    let (status, consequence) = p15_verdict(ports.len(), &rows, &errors);
    p.verdict(status, &consequence)
}

pub fn p14_max_rate(ports: &[PathBuf], pairs: &[VerifiedPair]) -> Probe {
    let mut p = Probe::new("P14", "maximum reliable rate", P14_QUESTION);
    let mut facts = P14Facts {
        ports_named: !ports.is_empty(),
        pair_present: !pairs.is_empty(),
        both_uart: false,
        baseline_ok: false,
        max_reliable_baud: None,
        ceiling_kind: None,
        baseline_restored: false,
        baseline_reproved: false,
        search_budget_exhausted: false,
    };
    // **The three cells the gate requires, stamped up front on every path.**
    //
    // They used to be emitted only after the search ran, and two `degraded` arms
    // return before that — a pair whose P5 ladder did not round-trip, and a pair
    // that would not reopen. `expectations/*.jq` exempts only `skipped`, so those
    // two arms produced a `degraded` report with no `max_reliable_baud` and the
    // gate went **red for a reason that is not a defect** — exactly what the
    // `degraded` arm exists to prevent. A rig whose cable seated a millimetre
    // differently would have reddened the lane.
    //
    // `null` is admissible by design: the clause tests `has`, never a type and
    // never a value, precisely so an incomplete search can say so. The search
    // overwrites these with real values when it runs; `Probe::observe` appends, so
    // a reader sees the later, truer pair — and a report that never got there
    // still carries the keys with an honest `null`.
    p = p
        .observe("max_reliable_baud", serde_json::Value::Null)
        .observe("ceiling_kind", serde_json::Value::Null)
        .observe(
            "ceiling_is_a_floor_over",
            "nothing yet — the search did not run on this path; the verdict says why.",
        );
    p = p.observe("pairs_discovered", pairs.len() as u64);
    // Stated on every path, including the ones that never measure a rate: a
    // report has to say what the instrument's own limit is before its answer can
    // be read against it (§15.34).
    p = p.observe("structural_max_baud", P14_MAX_BAUD as u64);
    p = p.observe("baseline_baud", P14_BASELINE_BAUD as u64);

    let Some(pair) = pairs.iter().find(|p| p.both_uart) else {
        let (status, consequence) = p14_verdict(facts);
        if let Some(first) = pairs.first() {
            p = p.observe(
                "pair",
                format!("{} ↔ {}", first.a_name, first.b_name).as_str(),
            );
        }
        return p.verdict(status, &consequence);
    };
    facts.both_uart = true;
    facts.baseline_ok = pair.baseline_ok;
    p = p.observe(
        "pair",
        format!("{} ↔ {}", pair.a_name, pair.b_name).as_str(),
    );
    p = p.observe("baseline_integrity_from_p5", pair.baseline_ok);
    p = p.observe("icounts_measurable", sys::ICOUNTS_SUPPORTED);
    if !sys::ICOUNTS_SUPPORTED {
        p = p.observe("icounts_unmeasurable_because", P5_WHY_NO_ICOUNTER);
    }
    if !pair.baseline_ok {
        let (status, consequence) = p14_verdict(facts);
        return p.verdict(status, &consequence);
    }

    // Open both ports once and change the rate in place. Reopening per rung
    // would toggle DTR on every step — an auto-reset pulse per rate on the very
    // boards §7.1 holds the port open to protect.
    let (Ok(mut a), Ok(mut b)) = (
        p5_open(&pair.a, P14_BASELINE_BAUD, Parity::None),
        p5_open(&pair.b, P14_BASELINE_BAUD, Parity::None),
    ) else {
        p = p.observe("pair_open", "failed");
        let (status, consequence) = p14_verdict(facts);
        return p.verdict(status, &consequence);
    };

    let started = Instant::now();
    let mut history: Vec<RateTrial> = Vec::new();
    let mut rungs: Vec<serde_json::Value> = Vec::new();
    let mut budget_exhausted = false;
    while let Some(rate) = p14_next_rate(&history) {
        let phase = if !p14_is_ladder_rate(rate) {
            "refinement"
        } else if P14_LADDER_BODY.contains(&rate) {
            "body"
        } else {
            "open-end"
        };
        let rung = p14_measure_rung(&mut a, &mut b, rate, phase);
        history.push(RateTrial {
            requested: rate,
            outcome: rung.outcome,
        });
        rungs.push(rung.observations());
        if started.elapsed() > P14_BUDGET {
            budget_exhausted = true;
            break;
        }
    }
    let search_elapsed_ms = started.elapsed().as_millis() as u64;

    // The way out: restore the baseline rate and re-prove one round-trip at it,
    // so the rig is left the way its own certificate needs it (§15.51).
    let restored = p14_apply_rate(&mut a, P14_BASELINE_BAUD).0.is_none()
        && p14_apply_rate(&mut b, P14_BASELINE_BAUD).0.is_none();
    std::thread::sleep(P5_OPEN_SETTLE);
    let reproved = restored && {
        let len = p14_payload_len(P14_BASELINE_BAUD);
        let deadline = Duration::from_millis(2000);
        p14_flush(&b, Duration::from_millis(50));
        let ab = p14_trial(
            &a,
            &b,
            &p14_payload(P14_BASELINE_BAUD, "AB", 99, len),
            deadline,
        );
        p14_flush(&a, Duration::from_millis(50));
        let ba = p14_trial(
            &b,
            &a,
            &p14_payload(P14_BASELINE_BAUD, "BA", 99, len),
            deadline,
        );
        ab.byte_exact && ba.byte_exact
    };

    let (max_reliable, kind) = p14_ceiling(&history);
    facts.max_reliable_baud = max_reliable;
    facts.ceiling_kind = kind;
    facts.baseline_restored = restored;
    facts.baseline_reproved = reproved;
    facts.search_budget_exhausted = budget_exhausted;

    let bound = p14_lowest_failure_above(&history, max_reliable);
    p = p
        .observe(
            "max_reliable_baud",
            max_reliable.map(|b| serde_json::Value::from(b as u64)).unwrap_or(serde_json::Value::Null),
        )
        .observe(
            "ceiling_kind",
            kind.map(|k| serde_json::Value::from(k.label()))
                .unwrap_or(serde_json::Value::Null),
        )
        .observe(
            "first_unreliable_baud",
            bound
                .map(|t| serde_json::Value::from(t.requested as u64))
                .unwrap_or(serde_json::Value::Null),
        )
        .observe(
            "ceiling_is_a_floor_over",
            "the rates this ladder probed, under this trial policy — three byte-exact constant-airtime round-trips per direction. It is not a promise about rates between rungs, about sustained throughput, about longer cables, or about other temperatures. **It is a REQUESTED rate, not necessarily the wire's**: an adapter may round the ask to its nearest divisor and report the request back unchanged, and the bytes still round-trip because both ends are mis-set identically. Read `achieved_baud_floor` under each direction beside it — a large gap there (the bench rig shows ~0.94 of the ask on a clean rung and 0.70 on a rounded one) means the number above is the number you configure, not the number on the wire.",
        )
        .observe(
            "search_stops_at",
            "the FIRST rung that fails, not the exhaustion of the ladder — so a rate above the reported ceiling was never tried unless refinement reached it, and this number is a floor for that reason too.",
        )
        .observe("ladder_body_rungs", P14_LADDER_BODY.len() as u64)
        .observe(
            "rungs_attempted",
            history.len() as u64,
        )
        .observe(
            "refinements_used",
            history
                .iter()
                .filter(|t| !p14_is_ladder_rate(t.requested))
                .count() as u64,
        )
        .observe("refinements_max", P14_MAX_REFINEMENTS as u64)
        .observe("trials_per_direction", P14_TRIALS_PER_DIRECTION as u64)
        .observe("airtime_ms", P14_AIRTIME_MS)
        .observe("payload_floor_bytes", P14_PAYLOAD_FLOOR as u64)
        .observe("payload_cap_bytes", P14_PAYLOAD_CAP as u64)
        .observe("search_elapsed_ms", search_elapsed_ms)
        .observe("search_budget_exhausted", budget_exhausted)
        .observe("baseline_restored", restored)
        .observe("baseline_reproved", reproved)
        .observe("rungs", serde_json::Value::Array(rungs));

    let (status, consequence) = p14_verdict(facts);
    p.verdict(status, &consequence)
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
        Err(_) => checks.push(EnvCheck::new("XDG_RUNTIME_DIR", "unset", Status::Degraded)),
    }

    // **The socket path is COMPUTED, not described** (notes §3.72).
    //
    // This line used to be a prose clause on the `XDG_RUNTIME_DIR` check reading
    // "unset — daemon falls back to /run or a --socket override". That named the arm
    // only a *root* process reaches and omitted the one an unprivileged user actually
    // gets, so on macOS — no `XDG_RUNTIME_DIR`, no `/run` — it was wrong for every
    // reader, in the first field they look at when the socket is not where they
    // expected. Nothing could catch it: `expectations/*.jq` assert over `.probes[]`,
    // `.summary` and `.build`, and the environment block is none of those.
    //
    // A described policy drifts from the implemented one; a computed path cannot. This
    // calls the same function the daemon binds through and `ctl` connects to, so the
    // operator can compare it against `ls` directly. `Supported` on every arm: all
    // three are correct outcomes of §10, and the reason this row exists is to answer
    // "where is it?", not to grade the answer.
    let (sock, origin) = serial_nexus_rpc::default_socket_path(serial_nexus_rpc::DAEMON_NAME);
    checks.push(EnvCheck::new(
        "daemon socket (default)",
        format!("{} — {}", sock.display(), origin.describe()),
        Status::Supported,
    ));

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
            unmeasurable_here: None,
        }
    }

    /// An uncertified item this kernel had no way to measure — the shape both
    /// counter-reading items take off Linux.
    fn unmeasurable(item: &str, why: &'static str) -> CertFailure {
        CertFailure {
            item: item.to_owned(),
            integrity: false,
            unmeasurable_here: Some(why),
        }
    }

    /// A TX↔RX jumper: Tier 2.
    fn jumpered() -> RigFacts {
        RigFacts {
            discovered_pairs: 0,
            mismatch_pairs: 0,
            loopbacks: 1,
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

    /// The defect notes §3.45 (ii) filed: a tree with serial devices the resolver
    /// cannot name reported `supported` — "Resolver produces canonical identities;
    /// configs survive replug and cold start" — from a `for a in &adapters` loop
    /// that ran **zero** times, because the `skipped` early return needs
    /// `adapters.is_empty() && candidates.is_empty()` and this tree has candidates.
    /// §9 calls a verdict computed from a loop that never executed vacuous.
    ///
    /// The fixture is Darwin's shape reproduced on Linux through the `--dev-root`
    /// seam (plan §3): four `cu.*` callout nodes, no by-id tree, no by-path tree, no
    /// sysfs. It reproduces
    /// `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`'s P4 block
    /// observation for observation — `by_id_tree: absent`, `count: 0`,
    /// `sysfs_only: 0`, `other_candidates: 4` — which is what makes it a test of the
    /// platform's shape rather than of a Linux stand-in for it (§9, no proxy in
    /// space).
    ///
    /// **Fail-first, against the unfixed tree:** status `supported`, and the
    /// consequence carrying both false sentences. Four assertions below fail on it —
    /// `canonical` is absent, the status is wrong, the certification claim is
    /// present, and so is the `<sys>/class/tty` provenance on a tree that has no
    /// such listing.
    #[test]
    fn a_tree_where_nothing_resolved_is_degraded_rather_than_certified() {
        let t = TmpTree::new("noidentity");
        for n in [
            "cu.usbserial-BH00L4KU",
            "cu.usbserial-BH00LL8O",
            "cu.Bluetooth-Incoming-Port",
            "cu.BLTH",
        ] {
            write_file(&t.path().join("dev").join(n), "");
        }
        let sys_root = t.path().join("sys");

        let p = p4_resolver(t.path(), &sys_root);
        assert_eq!(
            p.status.label(),
            "degraded",
            "P4 certified a resolver that produced no identity at all — a verdict \
             computed from a loop that never executed (§9): {p:#?}"
        );
        assert!(
            !p.consequence
                .contains("Resolver produces canonical identities"),
            "the consequence claims the property the probe just failed to observe: {}",
            p.consequence
        );
        assert!(
            !p.consequence
                .contains("identities came from the <sys>/class/tty listing"),
            "false provenance: this tree has no <sys>/class/tty listing and \
             sysfs_only reads 0: {}",
            p.consequence
        );
        // §7 wants the observation named, and the operator-visible consequence is
        // the one docs/macos.md records: a node configured with a usb: identity
        // never resolves.
        assert!(
            p.consequence.contains("stays `waiting`") && p.consequence.contains("<sys>/class/tty"),
            "the differing environment must be named, not implied: {}",
            p.consequence
        );
        // The captured Darwin shape, observation for observation.
        assert_eq!(observed(&p, "by_id_tree"), Some("absent".into()));
        assert_eq!(observed(&p, "count"), Some(0.into()));
        assert_eq!(observed(&p, "sysfs_only"), Some(0.into()));
        assert_eq!(observed(&p, "other_candidates"), Some(4.into()));
        // …and the population the verdict is computed over, which had no name.
        assert_eq!(
            observed(&p, "canonical"),
            Some(0.into()),
            "the verdict's population must be counted, never assumed: {p:#?}"
        );
        assert_eq!(observed(&p, "unidentified"), Some(4.into()));
        assert_eq!(observed(&p, "sysfs_tty_listing"), Some("absent".into()));

        // Which nodes, not just how many — the report is the diff artifact (§13).
        assert_eq!(
            observed(
                &p,
                &t.path()
                    .join("dev/cu.usbserial-BH00L4KU")
                    .display()
                    .to_string()
            ),
            Some("raw:/dev/cu.usbserial-BH00L4KU".into()),
            "{p:#?}"
        );
    }

    /// The same defect on Linux, and the reason this is not a macOS-only fix: a USB
    /// adapter whose EEPROM carries no serial number gets **no** `/dev/serial/by-id`
    /// link, so `adapters` is empty, the loop runs zero times, and P4 reported
    /// `supported` for a box whose only identity is `by-path:` — the plan's
    /// "no-serial" P4 case, which the probe could not report while it looked only at
    /// by-id. This is the `degraded` arm §12 always intended, finally reachable.
    ///
    /// **Fail-first:** `supported` against the unfixed tree.
    #[test]
    fn a_serial_numberless_adapter_reachable_only_by_path_degrades() {
        let t = TmpTree::new("bypathonly");
        write_file(&t.path().join("dev/ttyUSB0"), "");
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB0",
            by_path.join("pci-0000:00:14.0-usb-0:1:1.0"),
        )
        .unwrap();
        // sysfs exists and is readable — this box is not macOS; it is an adapter
        // with nothing to make a canonical identity out of.
        std::fs::create_dir_all(t.path().join("sys/class/tty")).unwrap();
        let sys_root = t.path().join("sys");

        let p = p4_resolver(t.path(), &sys_root);
        assert_eq!(
            p.status.label(),
            "degraded",
            "P4 certified a box whose only identity is by-path — §12's documented \
             instability warning, reported as if configs survived replug: {p:#?}"
        );
        assert_eq!(observed(&p, "canonical"), Some(0.into()));
        assert_eq!(observed(&p, "topology_only"), Some(1.into()));
        assert_eq!(observed(&p, "sysfs_tty_listing"), Some("present".into()));
        assert!(
            p.consequence.contains("no usable serial number")
                && !p.consequence.contains("stays `waiting`"),
            "a Linux box with a serial-numberless clone must not be told its kernel \
             lacks the mechanism: {}",
            p.consequence
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
        // **Portable, and that is the whole point of this block.** The pair this
        // replaced was a `cfg` fork: the Linux half pinned "skipped on non-UART
        // sims" and the non-Linux half pinned "TIOCGICOUNT, which is Linux-only".
        // The second one guarded the sentence that had gone false when §15.47
        // widened the predicate to `TIOCMGET || TIOCGICOUNT` — and it *could not
        // compile* on the platform of record, so the defect was unreachable from
        // the box every developer sits at, and repairing the string reddened
        // nothing here. That is AGENTS §9's proxy in space, sitting inside the
        // guard for a defect that was itself a proxy in space.
        //
        // The portable form is stricter on Linux, which §9 names as the tell:
        // the arm is now one sentence about the *ports* (they answered neither
        // ioctl), so both of the false claims are refusable on both kernels.
        assert!(
            !why.contains("the UART predicate is TIOCGICOUNT"),
            "the uncharacterized arm still calls the predicate Linux-only; \
             `p5_is_uart` has been `TIOCMGET || TIOCGICOUNT` since §15.47, and a \
             real FTDI pair certifies on Darwin — \
             docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json: {why}"
        );
        assert!(
            !why.contains("run the certificate on a Linux box"),
            "the uncharacterized arm still sends a Darwin operator to another \
             kernel for a certificate their own box now produces: {why}"
        );
        assert!(
            why.contains("TIOCMGET"),
            "the arm must name the predicate it actually applied, or the next \
             widening drifts the same way: {why}"
        );
        assert!(why.contains("skipped"), "{why}");
        // Whichever arm ran, an uncertified rig must not borrow the certified
        // arm's opening. That sentence is what a tiered checklist run reads to
        // decide it may start (§15.21), and P5's prose has now over-claimed three
        // times (AGENTS §2's 6.18 entry has the other two).
        assert!(!why.contains("Rig discovered and certified"), "{why}");
    }

    /// The skip *reason* the arm above explains must name the same predicate the
    /// arm does, on every kernel — one constant now, where there used to be two.
    ///
    /// This compiles and runs on both platforms, which the pair it replaces did
    /// not: `P5_UNCHARACTERIZED` was `#[cfg]`-forked, so the false half was
    /// invisible to every Linux run. Naming `TIOCMGET` is the greppable token
    /// plan §18 item 1 asks each replacement sentence to carry.
    #[test]
    fn the_uncharacterized_reason_names_the_disjunction_not_one_ioctl() {
        assert!(
            P5_UNCHARACTERIZED.contains("TIOCMGET"),
            "{P5_UNCHARACTERIZED}"
        );
        assert!(
            P5_UNCHARACTERIZED.contains("TIOCGICOUNT"),
            "both members of the disjunction, or the reason names half a \
             predicate: {P5_UNCHARACTERIZED}"
        );
        assert!(
            !P5_UNCHARACTERIZED.contains("Linux-only"),
            "the reason a port was not characterized is a fact about the port, \
             not about the kernel — a pts fails both ioctls on both kernels, and \
             a real adapter answers TIOCMGET on both: {P5_UNCHARACTERIZED}"
        );
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
        assert_eq!(
            (dangling.tier(), jumpered().tier(), paired().tier()),
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

        let (_, why) = p5_verdict(true, true, &[], &[], jumpered());
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
        let lines: Vec<String> = [dangling, jumpered(), paired(), paired_uncertified()]
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

    /// **A degraded certificate still names the tier discovery found.** §3.42
    /// pre-registered "P5 reports Tier 3 with `rate_ladder=true`" for the first
    /// Darwin rig run. The rig delivered exactly that — ports paired both ways,
    /// `rate_ladder=true` over the physical crossover — and the word Tier never
    /// appeared: `grep -c "Tier [0-9]"` is **0** over all three 2026-08-05 Darwin
    /// captures, against `**Tier 3**` in all three Linux captures of the *same
    /// binary*. The single differing input is the `failures` list, and the only
    /// site that named a tier sat after the early return it triggers.
    ///
    /// Driven here by a *measurable* failure, so what is under test is the
    /// ordering on every platform rather than the platform — the Darwin-shaped
    /// input gets its own guard below.
    #[test]
    fn an_uncertified_rig_still_names_the_tier_discovery_found() {
        let (status, why) = p5_verdict(true, true, &[fail("usb-A: break", false)], &[], paired());
        assert_eq!(status.label(), "degraded", "{why}");
        assert!(
            why.contains("Tier 3"),
            "a degraded certificate did not name its tier: {why}"
        );
        assert!(why.contains("usb-A: break"), "item dropped: {why}");
        // Naming the tier must not promote the sentence: the certified arm's
        // opening is what a tiered run reads to decide it may start (§15.21).
        assert!(!why.contains("Rig discovered and certified"), "{why}");

        // The sentence tracks the topology rather than a constant that happens to
        // read Tier 3 on the box the guard was written on.
        let (_, why) = p5_verdict(true, true, &[fail("usb-A: break", false)], &[], jumpered());
        assert!(why.contains("Tier 2"), "{why}");

        // Nothing certified (a pair that would not reopen, `any_uart` false):
        // there is no certificate to scope, so no tier may be claimed.
        let (_, why) = p5_verdict(
            true,
            false,
            &[fail("a ↔ b: pair_reopen", false)],
            &[],
            paired(),
        );
        assert!(
            !why.contains("Tier"),
            "a tier was claimed with nothing certified: {why}"
        );
    }

    /// **An item this kernel cannot measure is named as such — and no other item
    /// is.** The old consequence said the skipped items were skipped because
    /// "TIOCGICOUNT, which is Linux-only"; the widened predicate replaced it with
    /// a bare list, so a Darwin operator could re-seat cables chasing something no
    /// Darwin box can produce (§7 wants the observation named; notes §3.45 E (i)).
    ///
    /// **A pair's observation key may not depend on which port discovery reached
    /// first** (§15.44, notes §3.71).
    ///
    /// `field_set` is computed from the sorted set of observation leaf paths, so an
    /// order-dependent key makes the digest a function of discovery order. Two Linux
    /// Tier-3 runs at one commit produced different digests over identical cells for
    /// exactly this reason. Asserted as the property — *any* two names, both
    /// orders, one key — rather than against the one pair the bench happens to have,
    /// because the bug is about ordering and a fixture with a single pair cannot see
    /// it (§9: the guard must assert the promise, not a proxy for it).
    ///
    /// The third case is the anti-vacuity control: a function returning a constant
    /// would satisfy the first two and is not what was asked for.
    #[test]
    fn a_pairs_observation_key_is_the_same_whichever_port_was_discovered_first() {
        for (a, b) in [
            ("usb:0403:6001:BH00L4KU:00", "usb:0403:6001:BH00LL8O:00"),
            ("/dev/cu.usbserial-BH00L4KU", "/dev/cu.usbserial-BH00LL8O"),
            ("/dev/ttyUSB0", "/dev/ttyUSB1"),
            // Not lexicographic in path order: `ttyUSB10` sorts before `ttyUSB9`,
            // so a "sort the paths numerically" reading of this would be wrong and
            // the key still has to be stable under it.
            ("/dev/ttyUSB10", "/dev/ttyUSB9"),
        ] {
            assert_eq!(
                p5_pair_subject(a, b),
                p5_pair_subject(b, a),
                "key for ({a}, {b}) depends on discovery order"
            );
        }
        // Distinct pairs must still get distinct keys — the fix is an ordering, not
        // a collapse.
        assert_ne!(
            p5_pair_subject("/dev/ttyUSB0", "/dev/ttyUSB1"),
            p5_pair_subject("/dev/ttyUSB2", "/dev/ttyUSB3"),
        );
        // And the key still names both ports, so the report stays readable.
        let k = p5_pair_subject("/dev/ttyUSB1", "/dev/ttyUSB0");
        assert!(
            k.contains("/dev/ttyUSB0") && k.contains("/dev/ttyUSB1"),
            "{k}"
        );
    }

    /// The negative half is the one with teeth, and it is what proves the
    /// *matcher* rather than the walker: an excuse that fires on every uncertified
    /// item reads as passing here while telling the operator to stop looking at a
    /// cable that really is loose.
    #[test]
    fn only_the_items_this_kernel_cannot_measure_are_excused_by_the_platform() {
        // The Darwin shape, from the three 2026-08-05 captures (two per-port
        // `icounter`, one per-pair `deliberate_mismatch`), plus a `break` that
        // Darwin measures perfectly well — it reads true there — and that here
        // failed.
        let darwin = [
            unmeasurable("A: icounter", P5_WHY_NO_ICOUNTER),
            unmeasurable("B: icounter", P5_WHY_NO_ICOUNTER),
            unmeasurable("A ↔ B: deliberate_mismatch", P5_WHY_NO_MISMATCH),
            fail("A: break", false),
        ];
        let (status, why) = p5_verdict(true, true, &darwin, &[], paired());
        assert_eq!(status.label(), "degraded", "{why}");
        for item in [
            "A: icounter",
            "B: icounter",
            "A ↔ B: deliberate_mismatch",
            "A: break",
        ] {
            assert!(why.contains(item), "item dropped: {why}");
        }
        assert!(
            why.contains("A: icounter, B: icounter cannot be measured on this kernel at all"),
            "{why}"
        );
        assert!(
            why.contains("A ↔ B: deliberate_mismatch cannot be measured on this kernel at all"),
            "{why}"
        );
        assert!(why.contains("TIOCGICOUNT"), "mechanism not named: {why}");
        assert!(why.contains("Tier 3"), "{why}");
        // The transmitted-but-unwitnessed distinction survives: the tier sentence
        // in the same line says the mismatch ran, so the excuse must not read as
        // "it did not".
        assert!(why.contains("deliberate baud mismatch ran"), "{why}");
        assert!(why.contains("was transmitted"), "{why}");
        assert!(
            !why.contains("A: break cannot be measured"),
            "a measurable failure was excused as the platform's: {why}"
        );

        // On a rig whose failures are all measurable the clause is absent
        // entirely — which is what every Linux run must read.
        let (_, why) = p5_verdict(true, true, &[fail("A: break", false)], &[], paired());
        assert!(!why.contains("cannot be measured"), "{why}");
        assert!(!why.contains("TIOCGICOUNT"), "{why}");

        // The miswiring arm prints the same list, so it owes the same mechanism.
        let (_, why) = p5_verdict(false, true, &darwin, &[], paired());
        assert!(why.contains("miswired"), "{why}");
        assert!(why.contains("TIOCGICOUNT"), "{why}");
    }

    /// **The two counter items are excused exactly where the ioctl is absent.**
    /// The fold above is only as good as what the certificate records, and that
    /// binding cannot be reached through `p5_certify_port` without a bench: a pts
    /// is rejected by `p5_is_uart` on both kernels, so a pts-driven guard would
    /// pass vacuously (§9). Asserted on the pure builders instead, in the form
    /// that comes out **stricter on the platform of record**: off Linux both items
    /// must carry the mechanism, on Linux neither may — there `icounter=false` is
    /// a measurement (a pts answering `ENOTTY`) and a platform excuse would be
    /// false.
    #[test]
    fn the_counter_items_are_platform_excused_exactly_where_the_ioctl_is_absent() {
        let excused = |c: &Certificate, item: &str| -> Option<&'static str> {
            c.failures
                .iter()
                .find(|f| f.item == item)
                .unwrap_or_else(|| panic!("{item} not recorded: {:?}", c.failures))
                .unmeasurable_here
        };
        let counters_absent = p5_port_certificate(true, true, "cts=false", false);
        let mismatch_unobserved = p5_pair_certificate(true, false);
        assert_eq!(
            excused(&counters_absent, "icounter").is_some(),
            !sys::ICOUNTS_SUPPORTED,
            "icounter excused on the wrong platform"
        );
        assert_eq!(
            excused(&mismatch_unobserved, "deliberate_mismatch").is_some(),
            !sys::ICOUNTS_SUPPORTED,
            "deliberate_mismatch excused on the wrong platform"
        );
        // Never the items that do not read a counter, on any platform.
        let rig_faults = p5_port_certificate(false, false, "cts=false", true);
        assert_eq!(excused(&rig_faults, "custom_baud"), None);
        assert_eq!(excused(&rig_faults, "break"), None);
        assert_eq!(
            excused(&p5_pair_certificate(false, true), "rate_ladder"),
            None
        );
        // A passing item records nothing, so no excuse can leak from one.
        assert!(
            p5_port_certificate(true, true, "cts=false", true)
                .failures
                .is_empty()
        );
        #[cfg(not(target_os = "linux"))]
        {
            assert!(
                excused(&counters_absent, "icounter")
                    .expect("the stub platform must excuse it")
                    .contains("TIOCGICOUNT"),
                "the mechanism must be the ioctl's absence"
            );
        }
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
    /// **The handshake vocabulary names every shape, and judges none of them**
    /// (§15.52).
    ///
    /// Two properties, and the second is the one that keeps the item honest. The
    /// classifier must distinguish a 5-wire crossover from a 3-wire link from a
    /// half-crossed handshake — otherwise the six cells beneath it are all a
    /// reader has, and a half-crossed handshake is exactly the state discovery
    /// already refuses to leave unnamed on the data pair. And **no shape may
    /// produce a certificate failure**: a 3-wire rig is §5's own stated
    /// assumption, so an item that degraded one would report the operator's
    /// cabling choice as a fault and would move the verdict on every committed
    /// artifact whose rig nobody has re-inspected.
    #[test]
    fn the_handshake_line_names_the_wiring_and_grades_none_of_it() {
        // Eight cells, named, so a transposed argument cannot hide in a
        // positional call — the shape that let the four-cell verdict ship
        // (notes §3.73).
        fn cells(v: [&str; 8]) -> HandshakeCells {
            HandshakeCells {
                rts_ab: v[0].into(),
                rts_ba: v[1].into(),
                dtr_ab_dsr: v[2].into(),
                dtr_ab_dcd: v[3].into(),
                dtr_ab_ri: v[4].into(),
                dtr_ba_dsr: v[5].into(),
                dtr_ba_dcd: v[6].into(),
                dtr_ba_ri: v[7].into(),
            }
        }
        let line = |v: [&str; 8]| p5_handshake_line(&cells(v));
        const NONE: [&str; 8] = ["false"; 8];

        // The bench rig, measured (notes §3.53 i, re-measured §3.73): RTS/CTS
        // both ways, DTR nothing — now across all six DTR crossings.
        let five_wire = line([
            "true", "true", "false", "false", "false", "false", "false", "false",
        ]);
        assert!(five_wire.starts_with("5-wire crossover"), "{five_wire}");
        // The design's stated common case.
        let three_wire = line(NONE);
        assert!(three_wire.starts_with("3-wire"), "{three_wire}");
        // The state worth naming: it carries one way, which is a wiring fault a
        // reader would otherwise have to spot across eight cells.
        let half = line([
            "true", "false", "false", "false", "false", "false", "false", "false",
        ]);
        assert!(half.contains("HALF-CROSSED"), "{half}");
        assert!(
            line([
                "false", "true", "false", "false", "false", "false", "false", "false"
            ])
            .contains("HALF-CROSSED"),
            "the mirror direction must be named too"
        );
        // A rig that carries DTR as well, and one that carries only DTR.
        let wired = line([
            "true", "true", "true", "false", "false", "false", "false", "false",
        ]);
        assert!(
            wired.starts_with("wired:"),
            "a fully wired rig must not be called a 5-wire crossover"
        );
        let dtr_only = line([
            "false", "false", "true", "false", "false", "false", "false", "false",
        ]);
        assert!(dtr_only.starts_with("DTR wired"));

        // **Every one of the six DTR crossings must be able to move the
        // verdict** — the defect was that two of them could not (notes §3.73).
        // Each is raised alone, so a cell dropped from the fold reddens here
        // and nowhere else. Fail-first: against the four-cell `any_dtr` the two
        // B→A cases below fail.
        for (i, which) in [
            (2, "dtr_a_to_dsr_b"),
            (3, "dtr_a_to_dcd_b"),
            (4, "dtr_a_to_ri_b"),
            (5, "dtr_b_to_dsr_a"),
            (6, "dtr_b_to_dcd_a"),
            (7, "dtr_b_to_ri_a"),
        ] {
            let mut v = NONE;
            v[i] = "true";
            assert!(
                line(v).starts_with("DTR wired"),
                "{which} alone did not register as a DTR line — the verdict is \
                 computed from a subset of the crossings it prints"
            );
            let mut v = [
                "true", "true", "false", "false", "false", "false", "false", "false",
            ];
            v[i] = "true";
            let with_rts = line(v);
            assert!(
                with_rts.starts_with("wired:"),
                "{which} did not lift a 5-wire crossover to `wired:` — a rig \
                 whose DTR IS carried would be reported as carrying nothing: {with_rts}"
            );
            assert!(
                !with_rts.starts_with("5-wire crossover"),
                "{which}: the sentence claims DTR moves nothing while the cell \
                 beside it says otherwise: {with_rts}"
            );
        }

        // Every shape must be distinguishable, or the classifier is a constant.
        let shapes: std::collections::BTreeSet<String> =
            [&five_wire, &three_wire, &half, &wired, &dtr_only]
                .iter()
                .map(|s| s.split(" [").next().unwrap_or_default().to_owned())
                .collect();
        assert_eq!(
            shapes.len(),
            5,
            "two wiring shapes share a name: {shapes:?}"
        );
        // The eight cells travel with the name, always — the name is a reading
        // of them and a reader must be able to check it.
        for l in [&five_wire, &three_wire, &half, &wired, &dtr_only] {
            for key in [
                "rts_a_to_cts_b=",
                "rts_b_to_cts_a=",
                "dtr_a_to_dsr_b=",
                "dtr_a_to_dcd_b=",
                "dtr_a_to_ri_b=",
                "dtr_b_to_dsr_a=",
                "dtr_b_to_dcd_a=",
                "dtr_b_to_ri_a=",
            ] {
                assert!(l.contains(key), "{key} missing from {l}");
            }
        }
        // A non-boolean reading still reaches the reader rather than being
        // folded away: `stuck-high` is not `true`, so it must not claim wiring.
        let stuck = line([
            "true",
            "true",
            "stuck-high",
            "false",
            "false",
            "false",
            "false",
            "false",
        ]);
        assert!(
            stuck.starts_with("5-wire crossover") && stuck.contains("dtr_a_to_dsr_b=stuck-high"),
            "a stuck line was read as wired, or was dropped from the cells: {stuck}"
        );
        // **Reported, never judged**: the handshake reaches no `CertFailure`, so
        // no shape can move P5's verdict. Asserted over the verdict itself rather
        // than by inspecting the call site, because that is the property.
        for l in [&five_wire, &three_wire, &half] {
            let (status, _) = p5_verdict(true, true, &[], &[], paired());
            assert_eq!(
                status.label(),
                "supported",
                "a handshake reading moved the verdict: {l}"
            );
        }
    }

    #[test]
    fn skipped_certificates_carry_no_failure_but_unavailable_ones_do() {
        let skipped = Certificate::skipped(P5_UNCHARACTERIZED);
        assert_eq!(skipped.line, format!("skipped ({P5_UNCHARACTERIZED})"));
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
        cert.fail_if(false, "custom_baud", false, None);
        cert.fail_if(true, "break", false, None);
        assert_eq!(cert.failures, vec![fail("break", false)]);
    }

    /// P13's whole reason for existing is that a byte count alone cannot separate
    /// a kernel that discarded promptly from one that waited for a reader first —
    /// both end with nothing recovered. Pin all four quadrants, and pin the pair
    /// that differs *only* in the close duration, or a later simplification that
    /// drops `close_us` from the classifier would pass every other test here.
    /// **P13's fourth shape must not be inert on the platform of record.**
    ///
    /// `d_no_reader_second_fd_held` is `a_no_reader_blocking_slave` with a second
    /// fd held across the writer's close. If the two rows are identical in every
    /// cell, the witness bought nothing here and the shape is a decoration — and
    /// that matters beyond this probe, because notes §3.56 converted seven `itest`
    /// guards to hold exactly such a witness on exactly that argument.
    ///
    /// **Which cell moves is the kernel's to decide, and this guard does not pin
    /// it.** Linux `retains`, so the byte counts agree and the *terminal read* is
    /// the discriminator — measured 7.0.0-29: `EIO` when the writer's close is the
    /// last one, `EAGAIN` while the witness is held, because the hangup itself is
    /// deferred to the reference-count edge. A kernel that discards at last close
    /// moves the byte counts instead. Asserting "something differs" rather than
    /// "the terminal differs" is what keeps this portable, and it is still a real
    /// assertion: an implementation that opened no second fd would move nothing at
    /// all.
    ///
    /// **The precondition below used to be Linux's answer, asserted as everyone's**
    /// (notes §3.65). `bare` recovering the whole payload is what a *retaining*
    /// kernel does; Darwin `waits-then-discards` and recovers **0 of 64**, so the
    /// guard failed on the one platform whose §3.56 conversions the witness was
    /// bought for. It now reads the disposition off the bare shape and asserts the
    /// consequence that belongs to it — and on a discarding kernel that consequence
    /// is **stricter**, not weaker: the witness must recover what would otherwise
    /// be lost, all of it. Measured on Darwin 24.6.0: `bare` 0 of 64 with a 600060
    /// us close, `held` **64 of 64** with a 12 us close and `EAGAIN`. So §3.56's
    /// argument is not merely intact off Linux, it is load-bearing there and does
    /// the work — on Linux the witness only moves the terminal, because the bytes
    /// were never at risk.
    #[test]
    fn the_last_close_reference_count_shape_is_not_inert_here() {
        let bare = p13_shape(CloseShape::NoReader).expect("the no-reader shape runs");
        let held = p13_shape(CloseShape::NoReaderSecondFdHeld).expect("the held-fd shape runs");
        let bare_bytes = bare.bytes_before + bare.bytes_after;
        let held_bytes = held.bytes_before + held.bytes_after;
        if bare_bytes < P13_PAYLOAD as u64 {
            // A discarding kernel. The witness exists precisely to stop this, so
            // "something differs" is far too weak here: the bytes must come back.
            assert_eq!(
                held_bytes, P13_PAYLOAD as u64,
                "this kernel discards a departed writer's bytes at last close \
                 (bare recovered {bare_bytes} of {P13_PAYLOAD}), and holding a second fd \
                 across that close recovered {held_bytes} rather than all {P13_PAYLOAD}. \
                 A witness fd is then NOT the last-close edge here, and the seven guards \
                 notes §3.56 converted are resting on an argument this box does not support"
            );
        }
        let bytes_differ =
            (bare.bytes_before + bare.bytes_after) != (held.bytes_before + held.bytes_after);
        let terminal_differs = bare.terminal != held.terminal;
        assert!(
            bytes_differ || terminal_differs,
            "holding a second fd on the same pts across the writer's close changed \
             nothing at all — same bytes ({} vs {}), same terminal ({} vs {}). Then \
             a witness fd is not the last-close edge on this kernel, and the seven \
             guards notes §3.56 converted are resting on an argument this box does \
             not support",
            bare.bytes_before + bare.bytes_after,
            held.bytes_before + held.bytes_after,
            bare.terminal,
            held.terminal
        );
    }

    /// **P13's fifth shape reports whether it arrived, rather than assuming it
    /// did** (plan §18 item 22).
    ///
    /// Executed on whatever box runs this, and deliberately *not* pinning the race:
    /// `arrived_before_close_returned` is the kernel's answer, and on one whose
    /// close does not wait there may be no window to arrive in at all. What is
    /// asserted is that the row is internally coherent — the word matches the
    /// boolean, the two timestamps order the way the boolean says, and the byte
    /// accounting adds up — because a row that contradicted itself would be the
    /// §13 failure the shape exists to avoid, and no gate reads prose.
    ///
    /// Measured on Linux 7.0.0-29 over five consecutive passive runs: the arriving
    /// reader wins **5 of 5**, its first `read(2)` entering 0–1 µs after the close
    /// against a close that returns in 2–6 µs, recovering 64 of 64 with a terminal
    /// `EIO`. So the discriminator fires on the platform of record rather than only
    /// on the kernel the shape was written for, which is what keeps it out of §13's
    /// vacuity taxonomy 2.
    #[test]
    fn p13_arrival_shape_says_whether_it_arrived_rather_than_assuming_it_did() {
        let r = p13_shape(CloseShape::ReaderArrivesDuringCloseWait)
            .expect("the arriving-reader shape runs");
        let a = r
            .arrival
            .as_ref()
            .expect("the arriving-reader shape must carry its timing block");

        assert_eq!(
            a.arrived_before_close_returned,
            a.reading() == "arrived-inside-the-close-window",
            "the word and the boolean disagree — a reader of the JSON would take \
             the word: {} vs {}",
            a.reading(),
            a.arrived_before_close_returned
        );
        if a.arrived_before_close_returned {
            assert!(
                a.first_read_offset_us <= a.close_returned_us,
                "the row says the reader arrived before the close returned, and the \
                 timestamps say the opposite: first read at {} us, close returned at {} us",
                a.first_read_offset_us,
                a.close_returned_us
            );
            assert_eq!(
                a.bytes_recovered, r.bytes_during,
                "the arriving reader's bytes must be the ones the shape accounts as \
                 recovered during the close"
            );
        }
        assert_eq!(
            r.recovered(),
            r.bytes_before + r.bytes_during + r.bytes_after,
            "the total must count every place the payload can come back from"
        );
        assert!(
            !a.does_not_license().is_empty(),
            "a race result with no statement of what it licenses is the reading §13 refuses"
        );
    }

    /// The two readings of the arrival, checked against each other rather than
    /// against whichever kernel is in front of you (§9). The `lost-the-race` arm is
    /// the one a box whose close returns in microseconds may never produce, and it
    /// is exactly the arm a Darwin-shaped kernel would make routine.
    #[test]
    fn the_arrival_row_names_both_outcomes_and_licenses_neither_by_default() {
        let row = |arrived: bool| ArrivalTiming {
            first_read_offset_us: if arrived { 1 } else { 900 },
            close_returned_us: if arrived { 600_000 } else { 7 },
            arrived_before_close_returned: arrived,
            bytes_recovered: if arrived { 64 } else { 0 },
            terminal: "EAGAIN".to_owned(),
            arm_us: 250,
        };
        assert_ne!(row(true).reading(), row(false).reading());
        assert_ne!(row(true).does_not_license(), row(false).does_not_license());
        assert!(
            row(false).does_not_license().contains("NOTHING"),
            "a lost race must say so in the strongest available terms, because the \
             cells beside it look exactly like an answer: {}",
            row(false).does_not_license()
        );
        assert!(
            row(true).does_not_license().contains("shape `a`"),
            "an arrival must point at the shape that answers the question it does \
             not: {}",
            row(true).does_not_license()
        );
    }

    /// **The arrival cells exist on the shape that has an arriving reader, and on
    /// no other** — and the total counts them.
    ///
    /// Two properties in one guard because they are one decision. Stamping
    /// `bytes_recovered_by_arriving_reader: 0` on the other four shapes would add a leaf
    /// path to each for a structurally-zero number, moving `field_set` further than
    /// the change earns; and a total that summed only before and after would publish
    /// `bytes_lost: 64` beside a reader that recovered all 64 — a *wrong* cell,
    /// which §13 ranks below a missing one.
    #[test]
    fn only_the_arriving_reader_shape_carries_arrival_cells_and_the_total_counts_them() {
        let base = |during: u64, arrival: Option<ArrivalTiming>| CloseResult {
            close_us: 9,
            bytes_before: 0,
            bytes_during: during,
            bytes_after: 0,
            terminal: "EIO".to_owned(),
            slave_mode: "raw",
            baseline_packet_bytes: 1,
            arrival,
        };
        let plain = base(0, None).observations(64);
        assert!(
            plain.get("reader_arrival").is_none()
                && plain.get("bytes_recovered_by_arriving_reader").is_none(),
            "a shape with no arriving reader grew arrival cells: {plain}"
        );
        assert_eq!(plain["bytes_lost"], serde_json::json!(64));

        let arrived = base(
            64,
            Some(ArrivalTiming {
                first_read_offset_us: 1,
                close_returned_us: 9,
                arrived_before_close_returned: true,
                bytes_recovered: 64,
                terminal: "EIO".to_owned(),
                arm_us: 250,
            }),
        )
        .observations(64);
        assert_eq!(
            arrived["bytes_recovered_by_arriving_reader"],
            serde_json::json!(64)
        );
        assert_eq!(arrived["bytes_recovered_total"], serde_json::json!(64));
        assert_eq!(
            arrived["bytes_lost"],
            serde_json::json!(0),
            "the payload came back through the arriving reader, and the row said it \
             was lost: {arrived}"
        );
        assert!(arrived["reader_arrival"]["reading"].is_string());
    }

    /// A P16 result with the two arms set independently, so every quadrant of the
    /// verdict is constructible on a box that can only produce one of them (§9).
    fn p16_result(quiet: bool, hangup: bool, path_persists: bool) -> P16Result {
        let window = |hangups: u32, passes: u32| P16Window {
            passes,
            hangups,
            elapsed_us: 120,
            revents: BTreeMap::from([("none".to_owned(), passes as u64)]),
        };
        P16Result {
            tight: window(if quiet { 0 } else { 7 }, P16_TIGHT_PASSES),
            paced: window(0, P16_PACED_PASSES),
            stat_while_open: P16StatReading {
                fstat_answers: true,
                path_resolves: true,
                identity_matches: Some(true),
            },
            hangup_after_close: hangup,
            hangup_after_us: if hangup { 1 } else { 500_000 },
            hangup_revents: if hangup {
                "POLLHUP".to_owned()
            } else {
                "none".to_owned()
            },
            stat_after_close: P16StatReading {
                fstat_answers: true,
                path_resolves: path_persists,
                identity_matches: path_persists.then_some(true),
            },
            read_after_close: "eof".to_owned(),
        }
    }

    fn p16_probe(r: &P16Result) -> Probe {
        p16_verdict(Probe::new("P16", "pts slave-witness liveness", "q"), r)
    }

    /// **P16's two arms are each other's control, and both must be able to fire**
    /// (§15.59, §15.49, §9).
    ///
    /// The `degraded` shapes are the interesting ones and neither is reachable on
    /// the platform of record — Linux is quiet while the master is open and
    /// delivers `POLLHUP` in ~1 µs after it closes — so they are tested as a pure
    /// fold rather than against whatever kernel is in front of you. Three
    /// properties, in the order a reader needs them:
    ///
    /// 1. A quiet arm that was **not** quiet outranks everything. An fd that
    ///    reports a hangup while the master is open answers "dead" correctly and
    ///    "alive" wrongly, so the post-close reading below it is not a signal — and
    ///    the guard pins that ranking against a row that would otherwise select the
    ///    `supported` arm.
    /// 2. No hangup after the close is `degraded` with the observation named,
    ///    never `unsupported`: a kernel without a portable liveness signal is a
    ///    fact to carry into the argument (§7), not a contradicted premise.
    /// 3. `supported` requires both arms, which is the whole shape of the
    ///    instrument.
    #[test]
    fn p16s_arms_are_each_others_control_and_every_verdict_is_reachable() {
        // 3 — the Linux shape.
        let p = p16_probe(&p16_result(true, true, false));
        assert_eq!(p.status.label(), "supported");
        assert!(p.consequence.contains("can** tell"), "{}", p.consequence);

        // 2 — the hangup never arrives. Degraded, and the consequence must say what
        // is lost rather than blaming the kernel.
        let p = p16_probe(&p16_result(true, false, true));
        assert_eq!(p.status.label(), "degraded");
        assert!(
            p.consequence.contains("did not arrive"),
            "the missing hangup must be the headline: {}",
            p.consequence
        );
        assert!(
            !p.status.is_unsupported(),
            "a kernel without this signal is an observation, never a contradicted \
             design premise (§7)"
        );

        // 1 — the control fired, and it outranks the hangup arm. This row ALSO has
        // a healthy post-close hangup, so a wrong ranking would print the
        // `supported` sentence over a control that was not quiet.
        let noisy = p16_result(false, true, false);
        let p = p16_probe(&noisy);
        assert_eq!(p.status.label(), "degraded");
        assert!(
            p.consequence.contains("negative control fired"),
            "a control that fired must lead over both other findings: {}",
            p.consequence
        );
        // **And the published answer must agree with the verdict.** The status is
        // decided by its own arm, so a `poll_can_tell` that read the firing arm
        // alone would print `true` in the cell beside a `degraded` saying the
        // reading is unusable — a verdict contradicted by its own observation, in
        // the shape §13 exists to prevent. Asserted here because the status
        // assertion above cannot see it.
        assert!(
            !noisy.poll_can_tell(),
            "an fd that reported a hangup while its master was open cannot tell a \
             live pair from a dead one — it answers \"dead\" always, which is right \
             once and wrong the rest of the time"
        );
        assert!(
            !p16_result(true, false, false).poll_can_tell(),
            "a hangup that never arrived cannot tell them apart either"
        );
        assert!(p16_result(true, true, false).poll_can_tell());
    }

    /// **The two instruments are reported side by side, and the `stat` one is
    /// mirrored from the harness rather than re-imagined** (§15.59).
    ///
    /// `shipped_prove_open_would_refuse` is the disjunction `SlaveWitness::prove_open`
    /// computes — `fstat` failed, or the path stopped resolving, or the path names a
    /// different device — so a report says what the *shipped* check would have done
    /// on this kernel rather than what this probe thinks of it. The Darwin-shaped
    /// row (a path that persists past the master's close) is the one that matters and
    /// is unreachable here.
    #[test]
    fn p16_says_whether_the_shipped_stat_comparison_could_tell() {
        // Linux: the node is unlinked at the master's close, so the comparison
        // refuses the dead witness and accepts the live one.
        let linux = p16_result(true, true, false);
        assert!(linux.stat_can_tell());
        assert!(linux.poll_can_tell());
        let c = p16_probe(&linux).consequence;
        assert!(c.contains("also tells them apart here"), "{c}");

        // Darwin's expected shape: a persistent devfs node. `prove_open` would
        // return `Ok` on a witness whose pair is gone — the residual notes §3.60
        // could only predict — while `poll` still tells.
        let darwinish = p16_result(true, true, true);
        assert!(
            !darwinish.stat_can_tell(),
            "a path that still resolves to the same device after the master closed \
             is exactly the case the shipped comparison cannot see"
        );
        assert!(darwinish.poll_can_tell());
        let c = p16_probe(&darwinish).consequence;
        assert!(
            c.contains("cannot tell them apart here"),
            "the whole reason §15.59 exists is this row, and it must say so: {c}"
        );

        // **The other direction, which is the one an `and` is easy to lose.** An
        // instrument that refuses a witness whose master is *open* tells nothing
        // either — it answers "dead" always, exactly as a poll that always reports
        // `POLLHUP` does — and `stat_can_tell` must be `false` there even though the
        // post-close half of it reads correctly. Without this row the master-open
        // conjunct is unexecuted and could be deleted silently.
        let refuses_everything = P16Result {
            stat_while_open: P16StatReading {
                fstat_answers: true,
                path_resolves: false,
                identity_matches: None,
            },
            ..p16_result(true, true, false)
        };
        assert!(
            !refuses_everything.stat_can_tell(),
            "a comparison that refuses a live witness is not a discriminator, and \
             reporting it as one would credit the harness with an enforcement it \
             does not have"
        );

        // **And the identity cell must not read `false` where there is nothing to
        // compare**: a vanished path is an absence, not a different device, and the
        // two are different findings for a reader chasing a witness failure. Driven
        // through the real `take` rather than a hand-built struct — the hand-built
        // one cannot see the constructor deciding this, which is the half that can
        // get it wrong.
        let f = std::fs::File::open("/dev/null").expect("/dev/null opens");
        let gone = P16StatReading::take(&f, "/dev/pts/this-path-does-not-exist");
        assert!(gone.fstat_answers, "the fd is real, so step 1 must answer");
        assert!(!gone.path_resolves);
        assert_eq!(
            gone.identity_matches, None,
            "a path that does not resolve has no identity to disagree with, and a \
             `false` there reads as 'some other device is at this path'"
        );
        assert!(gone.prove_open_would_refuse());
        assert_eq!(
            gone.observations()["device_identity_matches"],
            serde_json::Value::Null
        );
    }

    /// **P16's quiet window carries its own wall clock, in the unit of its true
    /// cost** (§15.49 clause 1), and the paced arm is a different claim from the
    /// tight one (clause 3).
    ///
    /// Executed on whichever box runs this. It does not pin either answer — a
    /// kernel that hangs up early is a finding — only that the windows ran, that
    /// they are distinguishable, and that the paced one really paced.
    #[test]
    fn p16_windows_report_their_own_cost_and_the_paced_one_paces() {
        let r = p16_inner().expect("the pty arms run");
        assert_eq!(r.tight.passes, P16_TIGHT_PASSES);
        assert_eq!(r.paced.passes, P16_PACED_PASSES);
        let floor = (P16_PACED_PASSES as u64) * (P16_PACE.as_micros() as u64);
        assert!(
            r.paced.elapsed_us >= floor,
            "the paced window took {} µs for {} passes at a {:?} pace — it did not \
             pace, so it is a second copy of the tight window wearing a different \
             name (§15.49's replicates-are-not-levels)",
            r.paced.elapsed_us,
            r.paced.passes,
            P16_PACE
        );
        assert!(
            r.tight.elapsed_us < floor,
            "the tight window is supposed to be back-to-back and took {} µs",
            r.tight.elapsed_us
        );
        assert_eq!(
            r.tight.revents.values().sum::<u64>(),
            P16_TIGHT_PASSES as u64,
            "every pass must be accounted in the revents histogram, or a kernel \
             answering with some other bit would vanish into the gap"
        );
    }

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

    /// **A current binary always states P4's population**, whatever the verdict.
    ///
    /// This is the half of the §3.45 (ii) fix that the expectation files deliberately
    /// cannot carry. They must *abstain* when `canonical` is absent, because every
    /// archived report predates the field and a gate that failed on absence would be
    /// reporting the instrument's age rather than the resolver's behaviour. So the
    /// requirement lives here, where it is about the probe rather than about a file on
    /// disk, and where no archived report can satisfy or violate it.
    ///
    /// Fail-first: deleting the `canonical` observation from `p4_resolver` fails this
    /// with `P4 reported no population for the supported verdict`, on both fixtures —
    /// including the resolving one, so the guard cannot be satisfied by the degraded
    /// path alone.
    #[test]
    fn p4_always_reports_its_population() {
        // A tree where nothing resolves — Darwin's shape, and the degraded verdict.
        let nothing = TmpTree::new("pop-none");
        for n in ["cu.usbserial-A", "cu.usbserial-B"] {
            std::fs::create_dir_all(nothing.path().join("dev")).unwrap();
            std::fs::write(nothing.path().join("dev").join(n), b"").unwrap();
        }
        // …and one where they do, so the guard cannot be satisfied by the degraded
        // path alone.
        let resolving = TmpTree::new("pop-some");
        unlinked_usb_device(resolving.path(), "ttyUSB0");
        let by_id = resolving.path().join("dev/serial/by-id");
        std::fs::create_dir_all(&by_id).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB0",
            by_id.join("usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0"),
        )
        .unwrap();

        for (name, root) in [
            ("a tree where nothing resolves", nothing.path()),
            ("a tree where devices resolve", resolving.path()),
        ] {
            let p = p4_resolver(root, &root.join("sys"));
            assert!(
                observed(&p, "canonical").is_some(),
                "P4 reported no population for the `{}` verdict on {name}; the \
                 expectation files abstain when this key is absent, so its absence \
                 here is not caught anywhere else",
                p.status.label()
            );
        }
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
    /// Asserted against the **512-byte rung**, which is what the flat `recheck` fields
    /// publish — the same experiment this test guarded before the ladder existed, and
    /// still the first thing the ladder runs.
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
        let legacy = f
            .recheck
            .legacy()
            .expect("no rung drained P10_RECHECK_DRAIN, so the flat `recheck` fields are zeros");
        assert!(
            legacy.refilled > 0,
            "the recheck refilled 0 bytes into a peer that `recovered`'s drain had just \
             emptied — it ran before that drain, or not at all"
        );
        let expected = legacy.refilled.min(P10_RECHECK_DRAIN);
        assert_eq!(
            legacy.drained, expected,
            "the recheck's partial drain took {} bytes where {expected} were available \
             to take, so the room it then measures was never freed",
            legacy.drained
        );
        assert!(
            legacy.topped_up > 0,
            "{} bytes were handed back to the peer and the kernel then accepted none of \
             them — the top-up pass did not run",
            legacy.drained
        );
    }

    /// **The ladder must be a ladder.** One drain size is one bit of resolution, which
    /// is the defect this replaced; a ladder that collapses to a single size passes
    /// every other test in this file while measuring exactly what the single rung did.
    #[test]
    fn p10_ladder_is_a_ladder() {
        let (m, s) = pty_pair_in_mode(true);
        let f = p10_fill(m.as_raw_fd(), s.as_raw_fd(), "raw");
        assert_eq!(
            f.recheck.rungs.len(),
            P10_RECHECK_DRAINS.len() + 1,
            "the ladder ran {} rung(s) where {} partial drains plus one from-empty rung \
             were configured",
            f.recheck.rungs.len(),
            P10_RECHECK_DRAINS.len()
        );
        assert!(
            f.recheck.legacy().is_some(),
            "no rung drained {P10_RECHECK_DRAIN} bytes, so the flat `recheck` fields the \
             committed artifacts carry are published from a default and every one of them \
             reads 0"
        );
        let drained: std::collections::BTreeSet<u64> = f
            .recheck
            .rungs
            .iter()
            .filter(|r| r.drain_requested.is_some())
            .map(|r| r.drained)
            .collect();
        assert!(
            drained.len() >= 2,
            "the ladder's partial rungs drained {drained:?} — {} distinct size(s), so it has \
             the single-rung recheck's one bit of resolution and cannot bracket a watermark \
             at all",
            drained.len()
        );
        assert!(
            f.recheck.rungs.iter().all(|r| r.refilled > 0),
            "a rung refilled 0 bytes, so it ran against a peer the previous rung left full \
             and its drain freed room that was never measured"
        );
    }

    /// **A bracket must come from rungs that ran, and must point the right way.** The
    /// arithmetic is easy to invert and the vacuity is easy to reintroduce, so both are
    /// pinned against synthetic rungs rather than against whatever kernel this box is —
    /// the direction of an inequality is not a kernel property and must not be tested
    /// like one (§9).
    ///
    /// The bracket fixture carries **two** topping-up rungs and **two** refusing ones
    /// on purpose. With one of each, `map_or(occ, max)` and `map_or(occ, min)` are
    /// indistinguishable and an inverted bound passes — which is exactly how this
    /// guard's first draft claimed a fail-first proof it did not have.
    #[test]
    fn p10_ladder_reading_brackets_the_watermark_and_never_reads_one_from_a_rung_that_froze() {
        let rung = |drain: Option<u64>, refilled: u64, drained: u64, topped_up: u64| Rung {
            drain_requested: drain,
            refilled,
            drained,
            topped_up,
            ..Rung::default()
        };

        // A watermark at some T with 896 < T <= 1022: the 512 and 128 rungs top up
        // (occupancy 512 and 896), the 2 and 1 rungs do not (occupancy 1022, 1023).
        let r = p10_ladder_reading(&[
            rung(Some(512), 1024, 512, 512),
            rung(Some(128), 1024, 128, 128),
            rung(Some(2), 1024, 2, 0),
            rung(Some(1), 1024, 1, 0),
        ]);
        assert_eq!(r.rungs_carrying_a_bound, 4);
        assert_eq!((r.topping_up, r.refusing), (2, 2));
        assert_eq!(
            (r.threshold_gt, r.threshold_le),
            (Some(896), Some(1022)),
            "the floor under T is the LARGEST occupancy that topped up and the ceiling is \
             the SMALLEST that refused; got gt={:?} le={:?}, which is a wider bracket than \
             the rungs support and would read as 'T > 512' on a kernel that said more",
            r.threshold_gt,
            r.threshold_le
        );

        // A cooked pty: every drain takes nothing, so nothing is bounded. This is the
        // shape notes §3.48 filed against P4 — a verdict from a loop that never ran.
        let cooked = p10_ladder_reading(&[
            rung(Some(512), 24064, 0, 0),
            rung(Some(1), 24064, 0, 0),
            rung(None, 24064, 0, 0),
        ]);
        assert_eq!(
            (
                cooked.rungs_carrying_a_bound,
                cooked.threshold_gt,
                cooked.threshold_le,
                cooked.uniform_shortfall
            ),
            (0, None, None, None),
            "a bracket was read out of rungs whose drains freed 0 bytes"
        );

        // The from-empty rung carries no watermark information: T > 0 is vacuous.
        let from_empty_only = p10_ladder_reading(&[rung(None, 1024, 1024, 1024)]);
        assert_eq!(from_empty_only.rungs_carrying_a_bound, 0);

        // A uniform per-episode reservation is a shape one rung cannot show.
        let reserved = p10_ladder_reading(&[
            rung(Some(512), 1024, 512, 508),
            rung(Some(128), 1024, 128, 124),
        ]);
        assert_eq!(reserved.uniform_shortfall, Some(4));
        assert_eq!(
            p10_ladder_reading(&[rung(Some(512), 1024, 512, 512)]).uniform_shortfall,
            None
        );
    }

    /// **"The mask does not matter" must be measured, not cited.** A control that
    /// requests nothing is the only cell that can carry that claim; two cells that both
    /// request a real bit cannot, which is how a degenerate axis was published as a
    /// measured one.
    #[test]
    fn p9_reference_masks_include_one_that_requests_nothing() {
        assert!(
            P9_REF_MASKS.iter().any(|(_, m)| m.is_empty()),
            "the reference masks are {:?} — with no mask that requests nothing, \
             `hangup_delivered_to_a_mask_that_requested_nothing` is a citation of POSIX \
             and not an observation of this kernel",
            P9_REF_MASKS.map(|(n, _)| n)
        );
        let labels: std::collections::BTreeSet<&str> =
            P9_REF_MASKS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            labels.len(),
            P9_REF_MASKS.len(),
            "two reference masks share a label, so their cells collide in the report"
        );
    }

    /// The premise the 1x2 rests on, taken as a measurement on whatever kernel runs
    /// this. `assert_ne!` pins the discrimination itself, so a cell stubbed to a
    /// constant fails here even if both spellings drift.
    ///
    /// It also guards the wrapper: `poll_blocking` returns `revents` unmasked, and a
    /// wrapper that intersected them with the requested `events` would make the
    /// empty-mask cell read `none` on a hung-up fd and the whole control vacuous.
    #[test]
    fn an_unrequested_hangup_is_measured_and_the_framing_follows_it() {
        let master = new_master().expect("openpt");
        let pts = sys::ptsname(&master).expect("ptsname");
        let fd = master.as_raw_fd();
        sys::set_nonblocking(fd).expect("nonblocking");
        let slave =
            open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("slave");
        std::thread::sleep(PTY_SETTLE);
        let live = sys::poll_blocking(fd, PollFlags::empty(), 0);
        drop(slave);
        std::thread::sleep(PTY_SETTLE);
        let hungup_empty = sys::poll_blocking(fd, PollFlags::empty(), 0);
        // The control that makes the empty-mask reading interpretable: whatever the
        // empty mask says, a mask that ASKED must see the hangup. Without this, an
        // empty `hungup_empty` is equally explained by "the mask gated it" and by
        // "the fd never hung up", and the probe would attribute a setup failure to
        // the kernel.
        let hungup_asked = sys::poll_blocking(fd, PollFlags::POLLHUP, 0);
        assert!(
            live.is_empty(),
            "a live master answered {} to a poll requesting nothing",
            revents_label(live)
        );
        assert!(
            hungup_asked.contains(PollFlags::POLLHUP),
            "the slave was closed and a POLLHUP-requesting poll still answered {} — the fd did \
             not hang up, so nothing below measures a mask",
            revents_label(hungup_asked)
        );

        // This is the portable property, and it is the one the report actually
        // promises: the framing is DERIVED from the measurement. Asserting POSIX
        // here instead is what made this guard red on Darwin while the report went
        // on printing `shape: "1x2"` beside a field reading false (notes §3.53).
        let delivered_unrequested = hungup_empty.contains(PollFlags::POLLHUP);
        let axis = p9_mask_axis(delivered_unrequested);
        if delivered_unrequested {
            assert_eq!(axis.shape, "1x2");
            assert_eq!(axis.separation_field, "worst_case_separation_x100");
            assert_ne!(
                live, hungup_empty,
                "the empty mask is a replicate here, so it must still discriminate fd state"
            );
        } else {
            assert_eq!(
                axis.shape, "2x3",
                "this kernel gates the hangup on the requested mask, so the mask cells are \
                 levels and the table must not be published as a 1x2"
            );
            assert_eq!(
                axis.separation_field, "worst_case_separation_requesting_masks_x100",
                "with the empty mask a level, the fd-state contrast must be read from the \
                 cells that requested something"
            );
        }
    }

    fn flow_row(port: &str, ok: bool, honoured: bool, restored: bool) -> FlowReadback {
        FlowReadback {
            port: port.to_owned(),
            tcsetattr_ok: ok,
            tcsetattr_error: None,
            cflag_before: 0x4b00,
            cflag_after: if honoured { 0x4b00 | 0x30000 } else { 0x4b00 },
            honoured,
            shipped_predicate_agrees: Some(true),
            restored,
            soft: Ok(soft_row(true, true)),
        }
    }

    /// The software half of a row, built from the two answers that classify it:
    /// did `tcsetattr` succeed, and did the flags read back.
    fn soft_row(ok: bool, honoured: bool) -> SoftFlowReadback {
        let ixon_ixoff = 0x400 | 0x1000;
        SoftFlowReadback {
            tcsetattr_ok: ok,
            tcsetattr_error: None,
            iflag_before: 0,
            iflag_after: if honoured { ixon_ixoff } else { 0 },
            ixon: honoured,
            ixoff: honoured,
            iflag_matches_request: honoured,
        }
    }

    /// **Both arms ship; only one runs per kernel.** Linux honours `CRTSCTS` and
    /// Darwin drops it, so on either box alone half of this function is unreachable
    /// — which is exactly the shape §9 says to test purely rather than against
    /// whatever is plugged in. The silently-dropped arm is the one the probe exists
    /// for, and it must be `degraded` and never `unsupported`: a differing kernel is
    /// an observation, not a contradiction of the design (§7).
    #[test]
    fn p15_separates_a_dropped_request_from_an_honoured_one() {
        let honoured = [flow_row("/dev/ttyUSB0", true, true, true)];
        let (s, c) = p15_verdict(1, &honoured, &[]);
        assert!(matches!(s, Status::Supported));
        assert!(c.contains("honoured"));

        let dropped = [flow_row("/dev/cu.usbserial-A", true, false, true)];
        let (s, c) = p15_verdict(1, &dropped, &[]);
        assert!(
            matches!(s, Status::Degraded),
            "a dropped request must degrade"
        );
        assert!(
            c.contains("/dev/cu.usbserial-A"),
            "the consequence must name the port that dropped it: {c}"
        );

        // A driver that REFUSES is honest, and must not be confused with one that
        // accepts and drops: `silently_dropped` is false, so the fleet is clean.
        let refused = [flow_row("/dev/ttyUSB0", false, false, true)];
        assert!(matches!(p15_verdict(1, &refused, &[]).0, Status::Supported));
    }

    /// **`supported` is one status over two different facts, and the sentence must
    /// say which one it measured** (§7.1 clause 7, §13, §15.53).
    ///
    /// An honest refusal reads `supported` here on purpose — the driver said no, the
    /// operator learns it, and §7.1 does not refuse such a config at `load`. But
    /// until 2026-08-12 the arm above it printed **"Every named port honoured
    /// `CRTSCTS` on read-back"** over that reading, with `honoured_on_readback:
    /// false` in the cell directly beneath. A verdict contradicted by its own
    /// observation is the failure §13's reported-never-judged discipline exists to
    /// prevent, and this is the operator-facing half of the same collapse that made
    /// `load` refuse a config §7.1 does not refuse.
    ///
    /// Nothing else in the gate set can see it: `expectations/*.jq` assert over
    /// `.probes[].observations` and never over `.consequence`, and the two digests
    /// cover `(id, question)` and observation leaf paths. Asserted as properties —
    /// the arm is named, the honour claim is *absent*, the port is named, the
    /// non-refusal at `load` is stated — so rewording stays free (the same shape as
    /// the dropped arm's guard below).
    #[test]
    fn p15s_supported_verdict_never_claims_an_honour_a_refusing_port_did_not_give() {
        let refused = [flow_row("/dev/ttyS9", false, false, true)];
        let (s, c) = p15_verdict(1, &refused, &[]);
        assert!(
            matches!(s, Status::Supported),
            "an honest refusal is a legitimate driver answer, not a degradation (§7)"
        );
        assert!(
            !c.contains("Every named port"),
            "the verdict claims every port honoured CRTSCTS while `honoured_on_readback` \
             reads false in the cell beside it — that sentence is FALSE on this fleet: {c}"
        );
        assert!(
            c.contains("REFUSED") && c.contains("/dev/ttyS9"),
            "the refusing port must get its own named arm, and be named: {c}"
        );
        assert!(
            c.contains("NOT refused at `load`/`add-node`"),
            "the operator's next question is what the daemon will do with this port, \
             and §7.1's answer is that it loads: {c}"
        );

        // A mixed fleet is the shape that hides best: one port honours, one refuses,
        // and the old sentence was true of the first and false of the second.
        let mixed = [
            flow_row("/dev/ttyUSB0", true, true, true),
            flow_row("/dev/ttyS9", false, false, true),
        ];
        let (s, c) = p15_verdict(2, &mixed, &[]);
        assert!(matches!(s, Status::Supported));
        assert!(
            !c.contains("Every named port"),
            "one of the two ports did not honour it: {c}"
        );
        assert!(
            c.contains("/dev/ttyS9") && !c.contains("/dev/ttyUSB0"),
            "the arm must name the ports it is about — the refusing one — and not \
             the ones it is not: {c}"
        );

        // And the accept-then-drop finding still outranks it: a fleet carrying both
        // must lead with the defect, not with the honest answer.
        let both = [
            flow_row("/dev/ttyS9", false, false, true),
            flow_row("/dev/cu.usbserial-A", true, false, true),
        ];
        let (s, c) = p15_verdict(2, &both, &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.contains("ACCEPTED a `CRTSCTS` request"),
            "the defect must lead over the honest refusal: {c}"
        );
    }

    /// **The dropped arm must describe the disposition the daemon SHIPS, and this is
    /// the one consequence string in the tree that an operator acts on** (§15.53,
    /// notes §3.67/§3.72).
    ///
    /// Between §3.67 and this guard, P15's consequence told every reader that such a
    /// node "goes `faulted`, not `degraded`" and that whether to degrade or keep
    /// faulting "is a design question this probe exists to inform, not to answer".
    /// Both had been false since §3.67 shipped the refusal at `load`. A field report
    /// from an arm64 Darwin box arrived carrying that text verbatim (§3.72): the
    /// probe's *measurements* were all correct, and the sentence a reader would act
    /// on described the behaviour of a tree several commits old, on the one platform
    /// where it is the only actionable finding in the report.
    ///
    /// **Nothing else in the gate set can see this.** `expectations/*.jq` assert over
    /// `.probes[].observations`, never over `.consequence`; the digests cover
    /// `(id, question)` and the observation leaf paths, and a consequence is neither.
    /// A stale consequence is invisible to every gate and visible to every operator.
    ///
    /// Asserted as *properties* — the shipped verb, the refusal, both remedies, and
    /// the bound — rather than as a golden string, so rewording stays free and only a
    /// change of meaning fails. The two negative clauses name the specific retracted
    /// claims, because those are what a revert would restore.
    #[test]
    fn p15s_dropped_arm_states_the_shipped_refusal_and_not_the_retracted_open_question() {
        let (_, c) = p15_verdict(
            1,
            &[flow_row("/dev/cu.usbserial-A", true, false, true)],
            &[],
        );

        // The disposition, and where it happens. `load`/`add-node` and "before
        // anything is created" are the operator-visible half of §15.53.
        assert!(
            c.contains("REFUSED at `load`/`add-node`"),
            "must state the shipped refusal and the verbs it fires on: {c}"
        );
        assert!(
            c.contains("before anything is created"),
            "must say the refusal precedes creation, which is why --replace is safe: {c}"
        );
        // Both remedies, because a refusal with no way forward is §7's complaint in
        // a new costume.
        assert!(
            c.contains("flow_control = \\\"none\\\"") || c.contains("flow_control = \"none\""),
            "must offer the config remedy: {c}"
        );
        assert!(
            c.contains("adapter whose driver implements RTS/CTS"),
            "must offer the hardware remedy: {c}"
        );
        // The bound. §15.53 refuses only where it can measure, and two paths still
        // fault; a consequence claiming a total refusal would be the mirror defect.
        assert!(
            c.contains("--replace") && c.contains("the config loads"),
            "must name both paths that still reach a faulted node: {c}"
        );

        // The retracted claims, named. These are exactly the sentences that were
        // true before §3.67 and shipped for several commits after it.
        assert!(
            !c.contains("a design question this probe exists to inform"),
            "the fault-versus-degrade question is DECIDED (§15.53); consequence still calls it open: {c}"
        );
        assert!(
            !c.contains("the node goes `faulted`, not `degraded`"),
            "a configured rts-cts node is refused at load, not faulted: {c}"
        );
    }

    /// A probe that reconfigures a real adapter and cannot say it put it back must
    /// say *that* before it says anything else — the unrestored arm outranks even
    /// the finding the probe exists to report.
    #[test]
    fn p15_reports_a_failed_restore_ahead_of_its_own_finding() {
        let rows = [flow_row("/dev/cu.usbserial-A", true, false, false)];
        let (s, c) = p15_verdict(1, &rows, &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.contains("could not restore"),
            "the restore failure must lead, not the CRTSCTS finding: {c}"
        );
    }

    /// **The arm ranked above the probe's own finding, executed** (notes §3.68).
    ///
    /// `shipped_predicate_agrees: Some(false)` says this report and the daemon
    /// would answer differently about one port — a report that calls a port fine
    /// while `load` refuses it is worse than either verdict alone, which is why it
    /// outranks the dropped-request finding. Nothing constructed that value before,
    /// so the branch and *both* of its rank relationships were unexecuted: a future
    /// edit reordering the three arms was caught by nothing. The two orderings are
    /// asserted here in the only way that pins them — against rows that would
    /// otherwise select the neighbouring arm.
    #[test]
    fn p15_ranks_a_daemon_disagreement_below_a_failed_restore_and_above_its_own_finding() {
        let disagreeing = |port: &str, honoured: bool, restored: bool| FlowReadback {
            shipped_predicate_agrees: Some(false),
            ..flow_row(port, true, honoured, restored)
        };

        // Above the dropped-request finding: this row is *also* silently dropping,
        // so if the ranking were the other way round the CRTSCTS text would win.
        let (s, c) = p15_verdict(1, &[disagreeing("/dev/ttyUSB0", false, true)], &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.contains("answer differently") && c.contains("/dev/ttyUSB0"),
            "a daemon disagreement must lead over the dropped-request finding, and \
             must name the port: {c}"
        );

        // Below a failed restore: this row disagrees *and* was left reconfigured,
        // and a reconfigured adapter is the worse outcome of the two.
        let (s, c) = p15_verdict(1, &[disagreeing("/dev/ttyUSB0", false, false)], &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.contains("could not restore"),
            "a failed restore must still outrank a daemon disagreement: {c}"
        );

        // And agreement on a clean port is not a finding at all — the arm must not
        // fire on `None` (the predicate could not run), which is not a disagreement.
        let unmeasured = FlowReadback {
            shipped_predicate_agrees: None,
            ..flow_row("/dev/ttyUSB0", true, true, true)
        };
        assert!(matches!(
            p15_verdict(1, &[unmeasured], &[]).0,
            Status::Supported
        ));
    }

    /// **The software reading moves the verdict, ranked below the hardware one,
    /// and moves nothing in the daemon** (plan §18 item 14, §15.59, §9).
    ///
    /// The measured answer on the rig of record is that `ftdi_sio` **honours**
    /// `IXON|IXOFF` (`c_iflag` `0x5` → `0x1405`, a delta of exactly the two flags,
    /// on both ports of the FT232R crossover, Linux 7.0.0-29) — so the interesting
    /// arm, a driver that accepts and drops, is unreachable on this box and is
    /// tested here as a pure fold instead of against whatever is plugged in.
    ///
    /// **The judgement half is the point of the guard, and it inverted at P16's
    /// landing.** Between plan §18 item 14 and §15.59 this function asserted
    /// `Supported` on a dropping software row, because P15's `question` named
    /// `CRTSCTS` alone and a verdict may only answer for the question its header
    /// asks. The widened question is what changes it: `supported` over a silently
    /// dropped `IXON|IXOFF` would now be answering `supported` to a question this
    /// port answered no to. What did **not** change is asserted here too, because
    /// that is the clause a future reader will doubt: §15.53's refusal still covers
    /// `rts-cts` only, and item 14's decline still stands.
    #[test]
    fn p15s_software_finding_degrades_the_verdict_and_refuses_nothing() {
        // A clean hardware fleet whose driver silently drops the software mode.
        let dropping = [FlowReadback {
            soft: Ok(soft_row(true, false)),
            ..flow_row("/dev/ttyUSB0", true, true, true)
        }];
        let (s, c) = p15_verdict(1, &dropping, &[]);
        assert!(
            matches!(s, Status::Degraded),
            "the question names both flow-control kinds since §15.59, so a silently \
             dropped IXON|IXOFF cannot leave this probe reporting `supported` — that \
             is a verdict answering `supported` to a question it answered no to"
        );
        assert!(
            c.contains("SOFTWARE") && c.contains("/dev/ttyUSB0"),
            "a dropped software request must be named and its port named, or the \
             finding reaches nobody: {c}"
        );
        assert!(
            c.contains("faults at its own open") || c.contains("fault"),
            "the consequence must say what such a node actually does — fault late, \
             with the bare error the rts-cts refusal exists to prevent: {c}"
        );
        assert!(
            c.contains("not refused at `load`") || c.contains("refuses nothing"),
            "the verdict must say the daemon is unchanged: item 14 declines the \
             refusal, and a `degraded` that read as a shipped policy would be that \
             decline reversed by implication: {c}"
        );

        // **The ranking, both directions.** A fleet carrying both defects must lead
        // with the `CRTSCTS` one, because that is the finding with a shipped
        // consequence an operator acts on (§15.53's refusal at `load`).
        let both = [FlowReadback {
            soft: Ok(soft_row(true, false)),
            ..flow_row("/dev/cu.usbserial-A", true, false, true)
        }];
        let (s, c) = p15_verdict(1, &both, &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.starts_with("**1 named port(s) ACCEPTED a `CRTSCTS` request"),
            "the hardware drop must lead over the software one — it is the half with \
             a refusal behind it: {c}"
        );

        // An honest refusal: `tcsetattr` failed rather than reporting success over a
        // clear flag. Nothing to act on, and it must not be reported as a drop.
        let refused = [FlowReadback {
            soft: Ok(soft_row(false, false)),
            ..flow_row("/dev/ttyS9", true, true, true)
        }];
        let (s, c) = p15_verdict(1, &refused, &[]);
        assert!(matches!(s, Status::Supported));
        assert!(
            c.contains("REFUSED `IXON|IXOFF`"),
            "a refusal must get its own arm, as the hardware half's does: {c}"
        );
        assert!(
            !c.contains("ACCEPTED an `IXON|IXOFF` request"),
            "an honest refusal reported as a silent drop: {c}"
        );

        // Unmeasurable is data, not absence (§15.47): the port is named and the
        // sentence never reads as an answer.
        let unmeasured = [FlowReadback {
            soft: Err("tcgetattr after xon-xoff: ENOTTY".to_owned()),
            ..flow_row("/dev/ttyUSB1", true, true, true)
        }];
        let (s, c) = p15_verdict(1, &unmeasured, &[]);
        assert!(matches!(s, Status::Supported));
        assert!(
            c.contains("could not be read back") && c.contains("/dev/ttyUSB1"),
            "an unmeasured port must say so and be named: {c}"
        );

        // And the ordinary answer this rig gives, so the honoured arm is exercised
        // too and the three above are known to be distinguishable.
        let (_, c) = p15_verdict(1, &[flow_row("/dev/ttyUSB0", true, true, true)], &[]);
        assert!(
            c.contains("honoured it on read-back") && c.contains("no config is refused on it"),
            "the honoured arm must state both the reading and its own bound — the \
             bound being that the daemon consults none of this (item 14's decline): {c}"
        );
    }

    /// **A failed restore of the *software* write degrades, through the same arm
    /// the hardware write's does.**
    ///
    /// `baseline_restored` now covers both flag words, and the reason is not
    /// symmetry: the software pass writes `c_iflag`, so a restore check that read
    /// `c_cflag` alone would certify a port this probe had left with `IXON`
    /// asserted. This is the one route by which the software reading moves the
    /// verdict, and it must be the *leading* arm — a reconfigured adapter is a
    /// worse outcome than any unanswered question (notes §3.68).
    #[test]
    fn p15_ranks_an_unrestored_port_above_the_software_reading_too() {
        let rows = [FlowReadback {
            soft: Ok(soft_row(true, false)),
            ..flow_row("/dev/ttyUSB0", true, true, false)
        }];
        let (s, c) = p15_verdict(1, &rows, &[]);
        assert!(matches!(s, Status::Degraded));
        assert!(
            c.contains("could not restore"),
            "the restore failure must lead over the software finding: {c}"
        );
        assert!(
            !c.contains("SOFTWARE"),
            "nothing below an unrestored port should be offered as trustworthy: {c}"
        );
    }

    /// **The software read-back's error arm, executed rather than argued.**
    ///
    /// A probe that cannot take a reading must say so; the failure this guards
    /// against is the arm returning a confident `false` — indistinguishable in the
    /// JSON from a driver that dropped the request. Driven through a descriptor
    /// that is not a terminal at all, with a real `Termios` taken from a pty, so
    /// both syscalls fail the way they would on a port that vanished mid-probe.
    ///
    /// **The baseline comes off the *slave*, and that is a portability fact rather
    /// than a style choice.** This test took it off the `PtyMaster` until
    /// 2026-08-13 and so was Linux-only by accident: Linux answers `tcgetattr` on a
    /// `/dev/ptmx` master, Darwin answers **ENOTTY**, and the test died in its own
    /// setup on the platform of record — `panicked at … the master has a termios:
    /// ENOTTY`, in CI's macOS lane and on the x86_64 rig box alike. The slave is a
    /// terminal on both kernels, which is why every other pty test in this module
    /// already reaches for `ptsname` (notes §3.93). Strictly wider: the arm under
    /// test is unchanged and still runs on Linux exactly as before.
    #[test]
    fn the_software_readback_reports_unmeasurable_rather_than_answering() {
        let master = new_master().expect("a pty master opens");
        let pts = sys::ptsname(&master).expect("the master names its slave");
        let slave = open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())
            .expect("the pts slave opens");
        let baseline = tcgetattr(&slave).expect("the slave has a termios");
        let not_a_tty = std::fs::File::open("/dev/null").expect("/dev/null opens");

        let r = p15_soft_readback(&not_a_tty, &baseline);
        let e = r.err().expect(
            "a descriptor with no termios answered the software flow-control question \
             — the arm would then publish a reading nobody took",
        );
        assert!(
            e.contains("tcgetattr"),
            "the unmeasurable cell must name the mechanism that could not answer \
             (§15.47), not merely that something failed: {e}"
        );
    }

    /// No `--port` is a skip, not a verdict — the same opt-in shape as P3/P5/P11,
    /// and a loop that did not run may not produce a status (§9, notes §3.48).
    #[test]
    fn p15_skips_rather_than_certifying_a_port_list_it_never_had() {
        assert!(matches!(p15_verdict(0, &[], &[]).0, Status::Skipped { .. }));
        assert!(matches!(
            p15_verdict(2, &[], &["boom".to_owned()]).0,
            Status::Skipped { .. }
        ));
    }

    /// The two readings of the mask column, checked against each other rather than
    /// against this box (§9). Both arms ship; only one runs per kernel, and the one
    /// that does not run is exactly the one a single-kernel session cannot test.
    /// **The within-group order control reports an outcome, and all four of its
    /// per-group values are reachable** (notes §3.73).
    ///
    /// The probe documented this control from the day it was written and published
    /// nothing about whether it passed. Three of the four values below appear in no
    /// committed artifact — `warmup-refuted` in none at all — so a pure test is the
    /// only thing that covers them, which is the shape §3.65 caught three times in
    /// one session (a guard pinning the platform of record's answer instead of the
    /// property).
    ///
    /// Cells are given in **execution order**, last-run cell last.
    #[test]
    fn the_order_control_reports_its_outcome_and_every_arm_is_reachable() {
        // Flat: the whole committed Linux 7.0 corpus. 258/259/260 is 1.00x.
        assert_eq!(p9_order_group(&[258, 259, 260], true), P9OrderGroup::Flat);
        // Still flat at the widest noise the corpus shows (Darwin not-ready,
        // 1.279x) — the tolerance exists so this does not fire.
        assert_eq!(
            p9_order_group(&[1279, 1000, 1100], true),
            P9OrderGroup::Flat
        );

        // Linux 6.18's not-ready group, verbatim: 883/418/418 is 2.112x, declining,
        // last-run cell tied cheapest. This is the reading the control was built to
        // catch and the corpus does not contain.
        assert_eq!(
            p9_order_group(&[883, 418, 418], true),
            P9OrderGroup::ConsistentWithWarmup
        );

        // Above tolerance but NOT in execution order: whatever moved these is not a
        // monotone warmup. No committed artifact reads this.
        assert_eq!(
            p9_order_group(&[418, 418, 883], true),
            P9OrderGroup::WarmupRefuted
        );
        assert_eq!(
            p9_order_group(&[418, 883, 418], true),
            P9OrderGroup::WarmupRefuted
        );

        // Not comparable: the caller says the cells did not observe one state, and
        // an unmeasurable cell must not take the report down.
        assert_eq!(
            p9_order_group(&[883, 418, 418], false),
            P9OrderGroup::NotComparable
        );
        assert_eq!(
            p9_order_group(&[0, 418, 418], true),
            P9OrderGroup::NotComparable
        );
        assert_eq!(p9_order_group(&[418], true), P9OrderGroup::NotComparable);

        // All four labels are distinct, or the cell is a constant wearing four names.
        let labels: std::collections::BTreeSet<&str> = [
            P9OrderGroup::NotComparable,
            P9OrderGroup::Flat,
            P9OrderGroup::ConsistentWithWarmup,
            P9OrderGroup::WarmupRefuted,
        ]
        .iter()
        .map(|g| g.label())
        .collect();
        assert_eq!(labels.len(), 4, "two arms share a label: {labels:?}");

        // The combined reading. Linux 7.0: both groups flat, hangup delivered
        // unrequested, nothing ready on the unready fd.
        let linux = p9_order_control(&[258, 259, 260], &[262, 261, 263], true, 0);
        assert_eq!(linux.says, "excludes-warmup-above-tolerance");
        assert_eq!(linux.not_ready, P9OrderGroup::Flat);

        // Linux 6.18: the not-ready group fires, so the combined reading must not
        // claim warmup was excluded.
        let six_eighteen = p9_order_control(&[883, 418, 418], &[463, 462, 474], true, 0);
        assert_eq!(six_eighteen.says, "warmup-not-excluded");
        assert_eq!(six_eighteen.ready, P9OrderGroup::Flat);

        // **Darwin: the hung-up group must be `not-comparable`, never a warmup
        // reading.** Its ~11x spread there is the mask being a real level
        // (`revents: none` against `POLLHUP` on the same fd), and reporting a
        // kernel difference as instrument drift is the specific error this arm
        // exists to prevent.
        let darwin = p9_order_control(&[1279, 1000, 1100], &[1000, 1100, 10990], false, 0);
        assert_eq!(darwin.ready, P9OrderGroup::NotComparable);
        assert_eq!(darwin.not_ready, P9OrderGroup::Flat);
        assert_eq!(darwin.says, "excludes-warmup-above-tolerance");

        // A contaminated not-ready group (something was ready on an fd that should
        // never be) makes that group say so rather than reading its order.
        let contaminated = p9_order_control(&[883, 418, 418], &[463, 462, 474], true, 7);
        assert_eq!(contaminated.not_ready, P9OrderGroup::NotComparable);

        // Both groups unreadable reads `unmeasured`, not a pass.
        let neither = p9_order_control(&[883, 418, 418], &[1, 2, 3], false, 7);
        assert_eq!(neither.says, "unmeasured");

        // The tolerance is published so a later capture can re-derive it. It must
        // sit above the corpus and below the one firing reading, or the constant is
        // fitted to nothing.
        assert!(
            (128..=211).contains(&P9_ORDER_TOLERANCE_X100),
            "the tolerance no longer separates the committed corpus (max 1.279x) \
             from Linux 6.18's 2.112x: {P9_ORDER_TOLERANCE_X100}"
        );
    }

    #[test]
    fn the_mask_axis_framing_is_derived_from_the_measurement_both_ways() {
        let replicate = p9_mask_axis(true);
        let level = p9_mask_axis(false);
        assert_eq!(replicate.shape, "1x2");
        assert_eq!(level.shape, "2x3");
        assert_ne!(
            replicate.separation_field, level.separation_field,
            "if both readings name the same separation figure, the 2x3 arm silently \
             reports a contrast contaminated by a cell the kernel never made ready"
        );
        // The pairing is the whole point: a cleaned separation read against an
        // uncleaned spread is what made Darwin's 9.92x-vs-1.12x first read as a
        // failure of the survival criterion (notes §3.53).
        assert_ne!(replicate.spread_fields, level.spread_fields);
        for a in [&replicate, &level] {
            let cleaned = a.separation_field.contains("requesting");
            assert_eq!(
                cleaned,
                a.spread_fields.contains("requesting"),
                "separation `{}` and spread `{}` come from different cell sets",
                a.separation_field,
                a.spread_fields
            );
        }
        assert!(
            replicate.role.contains("replicates") && level.role.contains("LEVELS"),
            "each arm must SAY which reading it is; a reader diffing two kernels' \
             reports needs the prose to move with the number"
        );
        for a in [&replicate, &level] {
            assert!(
                a.role.starts_with("measured:"),
                "the rationale must present itself as an observation, never as a \
                 citation of POSIX — that premise is what notes §3.53 caught"
            );
        }
    }

    /// **Acceptance is not delivery, and P10 could not see the difference.**
    ///
    /// Filling hostward (master→slave) against a slave nobody reads: raw hands
    /// every accepted byte back, cooked hands back none of them — measured on
    /// Linux 7.0.0-29, where cooked also *accepts* the larger number, so the two
    /// modes disagree in both directions at once. Before `bytes_recovered_by_peer`
    /// existed the two were indistinguishable in the report, which is how a
    /// cooked-pty measurement could be read as this kernel's buffer depth. The
    /// figures that used to sit in this sentence are withdrawn rather than
    /// updated: they were a scratchpad pair no committed artifact backs (notes
    /// §3.34), and this test is the reason dropping them costs nothing — it
    /// proves the relation without them.
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

    /// **The classification is the fix, so it is tested where the measurement
    /// cannot be taken.** Every row below is a reading a committed
    /// `docs/doctor/` artifact carries, including two this kernel cannot produce:
    /// Darwin's targetward `0` beside 1024 recovered (6 of 6 runs across two
    /// binaries) and its hostward exact 1022. §9's proxy-in-space rule cuts both
    /// ways — a decision may not be *asserted* on a kernel it was not measured on,
    /// but a pure function of measured numbers may and must be.
    #[test]
    fn fionread_trust_separates_a_documented_cap_from_a_contradicted_zero() {
        // Linux 7.0.0-29, both directions, `linux-7.0-2026-08-05b-tier3*.json`.
        assert_eq!(
            fionread_trust(Some(4095), 15360),
            FionreadTrust::Undercounts
        );
        assert_eq!(
            fionread_trust(Some(4095), 13824),
            FionreadTrust::Undercounts
        );
        // Darwin 24.6.0, `macos-24.6.0-2026-08-05-1a9a8fc-tier3*.json`.
        assert_eq!(
            fionread_trust(Some(0), 1024),
            FionreadTrust::ContradictedEmpty
        );
        assert_eq!(fionread_trust(Some(1022), 1022), FionreadTrust::Agrees);
        // The shapes no artifact carries yet, pinned so they cannot drift.
        assert_eq!(fionread_trust(None, 1024), FionreadTrust::Unavailable);
        assert_eq!(fionread_trust(Some(0), 0), FionreadTrust::NothingToCheck);
        assert_eq!(fionread_trust(Some(5), 0), FionreadTrust::Overcounts);
        assert!(FionreadTrust::ContradictedEmpty.is_wrong());
        assert!(!FionreadTrust::Undercounts.is_wrong());
    }

    /// **The shipped P10 sentence may not quote a figure no artifact backs**
    /// (plan §18 item 1, notes §3.34's filing).
    ///
    /// The pair this refuses — `~13.8 KiB` raw against `~23.5 KiB` cooked — came
    /// from a session scratchpad, and the raw half does not agree with the
    /// committed Linux capture, which reads 13824–15360 bytes per direction. It
    /// rode in the **consequence string of every report this binary emits, on
    /// every kernel**, which is the reason it was filed rather than swept during a
    /// documentation pass: changing what a shipped report says is a decision, not
    /// an edit.
    ///
    /// This guard exists because *nothing else asserts that string*. Neither
    /// expectation file inspects consequence text, and neither digest can see it —
    /// `probe_set` covers `(id, question)` and `field_set` covers observation leaf
    /// paths, so a sentence rewrite moves neither (§15.44's named residual). So
    /// dropping the figures would otherwise have reddened nothing, and a green
    /// suite would have proven nothing.
    ///
    /// It asserts the *relation* survives, not just that the numbers went: a
    /// repair that deleted the whole clause would leave a reader with no reason to
    /// check `slave_termios_mode` before blaming a kernel, which is the sentence's
    /// actual job.
    #[test]
    fn the_shipped_p10_consequence_quotes_no_uncommitted_figure() {
        // The real probe, not a reconstruction: the string under test is the one
        // the binary emits, so the guard runs the emitter. P10 is pty-only and
        // passive, so it needs no port and no rig.
        let probe = p10_pty_buffer_depth();
        let why = &probe.consequence;
        assert!(
            !why.is_empty(),
            "P10 emitted no consequence at all, so this guard is reading nothing"
        );
        for figure in ["13.8", "23.5"] {
            assert!(
                !why.contains(figure),
                "the shipped P10 consequence still quotes {figure}, a scratchpad \
                 number no `docs/doctor/` artifact backs (notes §3.34): {why}"
            );
        }
        assert!(
            why.contains("slave_termios_mode"),
            "the consequence must still send the reader to the cell that settles \
             a cross-kernel gap before they blame the kernel: {why}"
        );
        assert!(
            why.contains("raw accepts less and returns all of it"),
            "the mode relation itself must survive the figures being withdrawn — \
             it is what the sentence is for, and it is proven numberlessly by \
             `p10_recoverability_separates_a_deep_buffer_from_a_black_hole`: {why}"
        );
    }

    /// The warning must fire on the Darwin reading, name the direction it applies
    /// to, and stay silent on a healthy one — a note that fired on Linux's 4095
    /// would be an operator learning to ignore it.
    #[test]
    fn p10_fionread_note_fires_only_where_the_reading_is_contradicted() {
        let healthy = [
            FionreadCheck {
                direction: "slave_to_master_targetward",
                at_drain: Some(4095),
                recovered: 15360,
                writer: Some(0),
            },
            FionreadCheck {
                direction: "master_to_slave_hostward",
                at_drain: Some(4095),
                recovered: 15360,
                writer: Some(0),
            },
        ];
        assert!(
            p10_fionread_note(&healthy).is_empty(),
            "Linux's 4095-of-15360 is the n_tty read-buffer cap, not a fault, and a \
             warning there trains the reader to skip the one that matters"
        );
        let darwin = [
            FionreadCheck {
                direction: "slave_to_master_targetward",
                at_drain: Some(0),
                recovered: 1024,
                writer: Some(0),
            },
            FionreadCheck {
                direction: "master_to_slave_hostward",
                at_drain: Some(1022),
                recovered: 1022,
                writer: Some(1022),
            },
        ];
        let note = p10_fionread_note(&darwin);
        assert!(
            note.contains("slave_to_master_targetward"),
            "the note must name the direction"
        );
        assert!(note.contains("1024") && note.contains("contradicted-empty"));
        assert!(
            note.contains("writer_pending_input_bytes"),
            "the discriminator must be quoted"
        );
    }

    /// **The calibration the Darwin finding rests on.** Linux-gated because it
    /// asserts a kernel behaviour measured on Linux; running it where it was not
    /// measured is the proxy §9 forbids, and on Darwin it would assert the very
    /// defect being reported.
    ///
    /// **If this is red on 6.18, it is a finding about the production kernel and
    /// not a regression — read it, do not "fix" it.** The name says *platform of
    /// record*, and the design's platform of record is 6.18 (§13), while every
    /// figure behind this assertion was measured on 7.0.0-29. So a red here on
    /// the production kernel says the two Linuxes disagree about `FIONREAD` on a
    /// pty, which invalidates exactly one thing: notes §3.45 (iv)'s reading of
    /// Darwin's `contradicted-empty` as a *Darwin* fault rather than a general
    /// one. It invalidates no shipped behaviour — P10 reports
    /// `peer_pending_input_trust` as data and the daemon never consumes it — and
    /// §7's rule that a differing kernel is `degraded` with the observation named
    /// is discharged by the probe, which does exactly that. This is a test, and a
    /// test's job is to be loud. Capture the numbers, record them beside §3.45,
    /// and re-scope the Darwin claim; do not relax the assertion to make a suite
    /// green on a kernel nobody has measured.
    #[test]
    #[cfg(target_os = "linux")]
    fn p10_fionread_is_trustworthy_on_the_platform_of_record() {
        for targetward in [true, false] {
            let f = p10_fill_direction(targetward).expect("fill one direction");
            let trust = fionread_trust(f.peer_pending_input_at_drain, f.recovered);
            assert!(
                matches!(trust, FionreadTrust::Agrees | FionreadTrust::Undercounts),
                "FIONREAD on this Linux pty classified `{}` ({:?} readable at the drain, \
                 {} recovered): either the at-drain sample moved after the drain, or the \
                 platform of record has stopped answering this ioctl — and the Darwin \
                 finding in notes §3.45 (iv) rests on Linux answering it correctly",
                trust.key(),
                f.peer_pending_input_at_drain,
                f.recovered
            );
            assert_eq!(
                f.writer_pending_input,
                Some(0),
                "the writing fd reported {:?} bytes readable on a raw pair; the \
                 hostward-master pre-registration in the P10 consequence reads a \
                 non-zero here as \"this kernel answers FIONREAD out of the tty input \
                 queue\", so a non-zero on LINUX would invalidate that discriminator",
                f.writer_pending_input
            );
        }
    }

    /// **The defect, as a table.** `supported` off a latch that never proved it
    /// could post an edge is the vacuous verdict §9 names, and six committed
    /// artifacts carry exactly that shape.
    #[test]
    fn p12_verdict_refuses_supported_from_an_unproven_latch() {
        let good = P12Facts {
            termios_edge: Some(true),
            idle_edges: 0,
            live_session_edges: 0,
            control_edge: true,
            idle_passes: 264,
            idle_elapsed_us: 325_000,
            measured: true,
        };
        assert!(matches!(p12_verdict(good).0, Status::Supported));
        let (s, c) = p12_verdict(good);
        assert!(
            c.contains("325000 us"),
            "the supported arm must quote its wall clock: {c}"
        );
        let _ = s;

        let vacuous = P12Facts {
            control_edge: false,
            ..good
        };
        let (s, c) = p12_verdict(vacuous);
        assert!(
            matches!(s, Status::Degraded),
            "P12 reported `supported` from a latch that posted no edge for a boundary \
             this probe produced on purpose — that zero is an inert instrument, not a \
             quiet kernel (notes §3.45 iii)"
        );
        assert!(c.contains("SO IS THE INSTRUMENT"));

        let no_window = P12Facts {
            idle_passes: 0,
            idle_elapsed_us: 0,
            ..good
        };
        assert!(matches!(p12_verdict(no_window).0, Status::Degraded));

        let spin = P12Facts {
            idle_edges: 3,
            ..good
        };
        assert!(p12_verdict(spin).1.contains("re-fires"));

        let mid_session = P12Facts {
            live_session_edges: 1,
            ..good
        };
        assert!(
            p12_verdict(mid_session).1.contains("ATTACHED"),
            "an edge with a client attached releases a live write lock and must be \
             said in its own words, not folded into the idle count"
        );

        let unmeasured = P12Facts {
            measured: false,
            ..good
        };
        assert!(matches!(p12_verdict(unmeasured).0, Status::Degraded));
    }

    /// The windows must execute where the latch is inert — that is what makes a
    /// Linux report's `control_session_edge: false` a negative control rather than
    /// a missing field.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn p12_windows_execute_where_the_latch_is_inert() {
        let i = p12_idle().expect("the windows run on any platform with a pty");
        assert_eq!(i.tight.passes, P12_TIGHT_PASSES);
        assert_eq!(i.paced.passes, P12_PACED_PASSES);
        assert!(
            i.paced.elapsed_us >= 200_000,
            "the paced window covered {} us for {} passes at a {} us cadence — the \
             pause is not being taken, and the wall-clock witness is the fix",
            i.paced.elapsed_us,
            i.paced.passes,
            i.paced.pause_us
        );
        assert!(!i.tight.reads.is_empty() && !i.live.reads.is_empty());
        assert!(
            !i.control_edge,
            "`SessionLatch` posted a session edge on a platform whose arm is \
             documented inert (§15.39). That is a finding, not a test failure: \
             P12's Linux `skipped` verdict and its consequence both say this \
             cannot happen."
        );
    }

    // -- P14: the maximum-rate search (§15.51) -------------------------------
    //
    // The whole point of splitting the decision and the fold out as pure
    // functions is that a bench cannot test them: the rig answers one history,
    // and every history that matters — a non-monotone ladder, a run that passes
    // all the way to the 32-bit cap, a Darwin ask-ceiling — has to be
    // *constructed*. That is the §15.47 pattern P5's verdict already uses, and
    // it is what makes a guard here regression-proof against an edit made on a
    // box whose hardware happens to stop at 3 Mbaud.

    fn t(requested: u32, outcome: RungOutcome) -> RateTrial {
        RateTrial { requested, outcome }
    }

    /// A `P14Facts` whose measurement completed cleanly, so each test can mutate
    /// exactly the field it is about.
    fn p14_good() -> P14Facts {
        P14Facts {
            ports_named: true,
            pair_present: true,
            both_uart: true,
            baseline_ok: true,
            max_reliable_baud: Some(3_000_000),
            ceiling_kind: Some(CeilingKind::AdapterRefused),
            baseline_restored: true,
            baseline_reproved: true,
            search_budget_exhausted: false,
        }
    }

    /// Every body rung passing, as the prefix of a longer constructed history.
    fn p14_body_all_passed() -> Vec<RateTrial> {
        P14_LADDER_BODY
            .iter()
            .map(|&r| t(r, RungOutcome::Passed))
            .collect()
    }

    #[test]
    fn p14_ladder_is_a_ladder_and_its_open_end_is_computed_not_listed() {
        // The analogue of `p10_ladder_is_a_ladder`: a collapsed ladder — one
        // rung, or a body whose entries are equal — satisfies every other test
        // in this block while measuring nothing.
        let mut sorted = P14_LADDER_BODY.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            P14_LADDER_BODY.len(),
            "the fixed body has duplicate rungs"
        );
        assert!(
            P14_LADDER_BODY.windows(2).all(|w| w[0] < w[1]),
            "the fixed body must be strictly ascending, or the climb re-asks a rate"
        );
        // The open end is a *rule*, not a list: rungs the body never names must
        // still be recognised as ladder rates, or every one of them would be
        // counted as a refinement and the budget would close after four.
        let top = P14_LADDER_BODY[P14_LADDER_BODY.len() - 1];
        assert!(p14_is_ladder_rate(top));
        assert!(p14_is_ladder_rate(top * 2));
        assert!(p14_is_ladder_rate(top * 4));
        assert!(p14_is_ladder_rate(P14_MAX_BAUD));
        // And a midpoint is not one, which is what makes refinement countable.
        assert!(!p14_is_ladder_rate(3_500_000));
        assert!(!p14_is_ladder_rate(top + 1));
    }

    #[test]
    fn p14_next_rate_climbs_the_body_then_doubles_and_terminates_at_the_structural_cap() {
        // Walk an all-pass history to exhaustion. Three properties, and the
        // third is the one plan §18 item 11 words as "the open end's termination
        // proven by construction": the walk is finite, every proposal fits the
        // field the operator configures, and the last one is that field's
        // maximum rather than something below it.
        let mut history: Vec<RateTrial> = Vec::new();
        let mut proposed: Vec<u32> = Vec::new();
        // A bound far above the real count (16 body + 9 open-end), so a
        // non-terminating decision function fails this test instead of hanging
        // the suite — a hang is not a red test, it is an unread one.
        for _ in 0..1_000 {
            match p14_next_rate(&history) {
                Some(r) => {
                    proposed.push(r);
                    history.push(t(r, RungOutcome::Passed));
                }
                None => break,
            }
        }
        assert!(
            p14_next_rate(&history).is_none(),
            "the climb did not terminate within 1000 proposals"
        );
        // **Strictly increasing, and that is the assertion — not `r <= P14_MAX_BAUD`,
        // which clippy is right to call absurd.** The cap *is* `u32::MAX`, so a
        // comparison against it in a `u32` is true by the type and proves nothing;
        // writing it would have been a guard that could never fail, in the test whose
        // whole subject is a bound. What an unclamped doubling actually produces is a
        // **wrap**: `3_072_000_000 * 2` truncates to `1_849_032_704`, which is
        // *smaller* than the rung before it. Monotonicity is therefore the property
        // that catches it, and it is checkable in the type the field really has.
        assert!(
            proposed.windows(2).all(|w| w[0] < w[1]),
            "the climb went backwards, which is what an unclamped doubling looks \
             like after it wraps a u32: {proposed:?}"
        );
        assert_eq!(
            proposed[..P14_LADDER_BODY.len()],
            P14_LADDER_BODY,
            "the fixed body must be climbed in order before the open end starts"
        );
        assert_eq!(
            proposed.last().copied(),
            Some(P14_MAX_BAUD),
            "the final clamped step is what makes `structural-cap` mean \
             'every rate the field can spell passed' rather than 'the doubling \
             overflowed'"
        );
        // The open end really opened: more rungs than the body names, so the
        // probe's own list is not the ceiling.
        assert!(
            proposed.len() > P14_LADDER_BODY.len(),
            "the open end proposed nothing; the body would then be the ceiling"
        );
        // And the fold reads that history as the instrument's limit, not the
        // wire's.
        assert_eq!(
            p14_ceiling(&history),
            (Some(P14_MAX_BAUD), Some(CeilingKind::StructuralCap))
        );
    }

    #[test]
    fn p14_brackets_the_highest_pass_and_the_lowest_failure_above_it_even_when_the_ladder_is_not_monotone()
     {
        // §15.51's stated reason for a ladder rather than a bisection:
        // reliability is not guaranteed monotone in the requested rate, because
        // a rate with a poor divisor fit can fail below a cleaner-fitting higher
        // one. A decision function that reads only the last two entries turns
        // that into a ceiling four rungs too low.
        // The body climbed as far as 3 M, with 1 M failing on the way — the poor
        // divisor fit §15.51 names. The climb must not have stopped there, and
        // the bracket must not remember it.
        let top = P14_LADDER_BODY
            .iter()
            .position(|&r| r == 3_000_000)
            .unwrap();
        let mut history: Vec<RateTrial> = p14_body_all_passed()[..=top].to_vec();
        let poor_fit = history
            .iter_mut()
            .find(|x| x.requested == 1_000_000)
            .expect("1 M is a body rung");
        poor_fit.outcome = RungOutcome::Corrupt;
        // Nothing above the highest pass has failed, so the climb continues —
        // the failure *below* 3 M bounds nothing.
        assert_eq!(p14_next_rate(&history), Some(4_000_000));
        assert_eq!(p14_ceiling(&history), (Some(3_000_000), None));

        history.push(t(4_000_000, RungOutcome::AdapterRefused));
        // Now there is a bracket, and it is (3 M, 4 M) — not (1 M, 2 M), which
        // is what a walk from the end would have produced.
        assert_eq!(p14_next_rate(&history), Some(3_500_000));
        assert_eq!(
            p14_ceiling(&history),
            (Some(3_000_000), Some(CeilingKind::AdapterRefused)),
            "the ceiling is the highest rate that passed, over the whole history"
        );

        // Bisection, and it stops at the budget rather than at a converged
        // bracket — four midpoints on a quantized axis is where precision stops
        // being bought.
        for expected in [3_500_000u32, 3_250_000, 3_125_000, 3_062_500] {
            let next = p14_next_rate(&history).expect("refinement stopped early");
            assert_eq!(next, expected);
            history.push(t(next, RungOutcome::AdapterRefused));
        }
        assert_eq!(
            history
                .iter()
                .filter(|x| !p14_is_ladder_rate(x.requested))
                .count() as u32,
            P14_MAX_REFINEMENTS
        );
        assert_eq!(
            p14_next_rate(&history),
            None,
            "refinement must stop at P14_MAX_REFINEMENTS midpoints"
        );
        assert_eq!(
            p14_ceiling(&history),
            (Some(3_000_000), Some(CeilingKind::AdapterRefused))
        );
    }

    #[test]
    fn p14_refinement_stops_when_the_bracket_can_hold_no_new_rate() {
        // A bracket one apart has no interior. Without this the loop proposes
        // the floor forever, which the budget would eventually stop — but by
        // then the report carries four rungs that measured nothing.
        let history = vec![
            t(115_200, RungOutcome::Passed),
            t(115_201, RungOutcome::Corrupt),
        ];
        assert_eq!(p14_next_rate(&history), None);
        assert_eq!(
            p14_ceiling(&history),
            (Some(115_200), Some(CeilingKind::UnreliableCorrupt))
        );
    }

    #[test]
    fn p14_separates_a_stall_from_a_loss_and_an_ask_from_a_wire() {
        // Four ceilings that must never be spelled the same way. `corrupt` and
        // `timed_out` are §6's deadline discipline — a stall and a loss are
        // different facts. `adapter-refused` and `platform-refused` are §15.47's
        // — one is the silicon, the other is the ask.
        for (outcome, kind) in [
            (RungOutcome::Corrupt, CeilingKind::UnreliableCorrupt),
            (RungOutcome::TimedOut, CeilingKind::UnreliableTimedOut),
            (RungOutcome::AdapterRefused, CeilingKind::AdapterRefused),
            (RungOutcome::PlatformRefused, CeilingKind::PlatformRefused),
            (RungOutcome::HungUp, CeilingKind::HungUp),
        ] {
            let history = vec![t(115_200, RungOutcome::Passed), t(230_400, outcome)];
            assert_eq!(
                p14_ceiling(&history),
                (Some(115_200), Some(kind)),
                "{} must fold to {}",
                outcome.label(),
                kind.label()
            );
        }
        let labels: std::collections::BTreeSet<&str> = [
            CeilingKind::UnreliableCorrupt,
            CeilingKind::UnreliableTimedOut,
            CeilingKind::AdapterRefused,
            CeilingKind::PlatformRefused,
            CeilingKind::HungUp,
            CeilingKind::StructuralCap,
        ]
        .iter()
        .map(|k| k.label())
        .collect();
        assert_eq!(labels.len(), 6, "two ceiling kinds share a spelling");
    }

    #[test]
    fn p14_a_darwin_ask_ceiling_is_a_fact_about_the_ask_and_never_about_the_wire() {
        // The shape plan §18 item 11 names: a `platform-refused` at 230400,
        // which is the rate Darwin's *named* termios constants stop at. This is
        // the one ceiling kind whose consequence must not be readable as a
        // statement about the cable — the same cable answers a higher number on
        // a kernel that will ask for it, which is exactly what this tree's
        // cross-kernel record is for (§7).
        //
        // It is a constructed-history test rather than a platform assertion on
        // purpose: serial2 reaches Darwin's rate through IOSSIOSPEED rather than
        // through the named constants, so hard-coding 230400 as a Darwin cap
        // would print a platform fact this instrument never measured (§9's proxy
        // in space).
        let history = vec![
            t(115_200, RungOutcome::Passed),
            t(230_400, RungOutcome::PlatformRefused),
        ];
        let (max, kind) = p14_ceiling(&history);
        assert_eq!(
            (max, kind),
            (Some(115_200), Some(CeilingKind::PlatformRefused))
        );
        let facts = P14Facts {
            max_reliable_baud: max,
            ceiling_kind: kind,
            ..p14_good()
        };
        let (status, why) = p14_verdict(facts);
        assert!(matches!(status, Status::Supported), "{}", status.label());
        assert!(
            why.contains("platform's ask surface"),
            "the consequence must name the surface that refused: {why}"
        );
        assert!(
            why.contains("**not** about the wire"),
            "the consequence must deny the wire reading explicitly: {why}"
        );
        assert!(
            why.contains("a different kernel may ask for more over the same cable"),
            "the consequence must say what would change the answer: {why}"
        );
    }

    #[test]
    fn p14_reports_the_number_and_never_grades_it() {
        // §15.51: `supported` whenever the measurement completes, whatever the
        // number — a rig that tops out at 115200 is slow, not broken. The two
        // ends of the plausible range must differ in *prose* and agree in
        // *status*, which is the property a grading verdict breaks.
        let slow = P14Facts {
            max_reliable_baud: Some(115_200),
            ..p14_good()
        };
        let fast = P14Facts {
            max_reliable_baud: Some(12_000_000),
            ..p14_good()
        };
        let (s_slow, why_slow) = p14_verdict(slow);
        let (s_fast, why_fast) = p14_verdict(fast);
        assert!(matches!(s_slow, Status::Supported), "{}", s_slow.label());
        assert!(matches!(s_fast, Status::Supported), "{}", s_fast.label());
        assert!(why_slow.contains("115200"), "{why_slow}");
        assert!(why_fast.contains("12000000"), "{why_fast}");
        assert_ne!(why_slow, why_fast, "the number must reach the prose");
        // And the bound the report owes the reader travels with the number.
        for why in [&why_slow, &why_fast] {
            assert!(
                why.contains("floor over the probed set"),
                "the consequence must bound what the number licenses: {why}"
            );
        }
    }

    #[test]
    fn p14_degrades_only_where_the_question_could_not_be_asked_and_never_reports_unsupported() {
        // Skips first: not opted in, no rig, or a rig with no clock. The last is
        // the software null modem, and it must skip rather than answer — every
        // rate "passes" on a pts, which would print a confident wire number with
        // no wire.
        //
        // **The two gates are not independent, and calling them so overstates
        // the defence by one gate.** `p5_rig` computes `baseline_ok` only
        // *inside* the `a_uart && b_uart` branch, so a pts fails the baseline
        // gate **because** it failed the UART gate — one predicate read twice. A
        // single false positive from `p5_is_uart` defeats both at once, and a
        // pts would then sail through the rate ladder because nothing clocks it.
        // The real protection is `p5_is_uart`, measured on Linux (a pts answers
        // `ENOTTY` to both `TIOCMGET` and `TIOCGICOUNT`, on the slave and on the
        // master) and unverified from this box on Darwin — which is the single
        // point of failure for the claim, and is named rather than assumed.
        let cases: [(P14Facts, &str); 3] = [
            (
                P14Facts {
                    ports_named: false,
                    ..p14_good()
                },
                "no --port named",
            ),
            (
                P14Facts {
                    pair_present: false,
                    ..p14_good()
                },
                "no verified cross-paired rig",
            ),
            (
                P14Facts {
                    both_uart: false,
                    ..p14_good()
                },
                P5_UNCHARACTERIZED,
            ),
        ];
        for (facts, reason) in cases {
            let (status, _) = p14_verdict(facts);
            match status {
                Status::Skipped { reason: r } => assert_eq!(r, reason),
                other => panic!("expected skipped({reason}), got {}", other.label()),
            }
        }

        // Degrades: the question could not be asked, or the search left no
        // answer, or the rig was not put back.
        let degrades: [(P14Facts, &str); 4] = [
            (
                P14Facts {
                    baseline_ok: false,
                    ..p14_good()
                },
                "rate ladder did not round-trip",
            ),
            (
                P14Facts {
                    max_reliable_baud: None,
                    ..p14_good()
                },
                "did not complete",
            ),
            (
                // The truncated search. **The absence of a reason is not a
                // fifth reason**: without this arm an incomplete search prints
                // the most impressive answer in the taxonomy by default.
                P14Facts {
                    ceiling_kind: None,
                    ..p14_good()
                },
                "did not complete",
            ),
            (
                P14Facts {
                    baseline_reproved: false,
                    ..p14_good()
                },
                "not returned to its baseline rate",
            ),
        ];
        for (facts, needle) in degrades {
            let (status, why) = p14_verdict(facts);
            assert!(
                matches!(status, Status::Degraded),
                "expected degraded, got {} — {why}",
                status.label()
            );
            assert!(why.contains(needle), "{why}");
        }

        // And no input reaches `unsupported`. That word means a design premise
        // is contradicted with no fallback, it is a live gate
        // (`expectations/*.jq` require `.summary.unsupported == 0`), and no
        // answer P14 can produce qualifies: a slow rig is slow.
        for ports_named in [false, true] {
            for pair_present in [false, true] {
                for both_uart in [false, true] {
                    for baseline_ok in [false, true] {
                        for max in [None, Some(9_600u32), Some(P14_MAX_BAUD)] {
                            for kind in [None, Some(CeilingKind::StructuralCap)] {
                                for restored in [false, true] {
                                    let f = P14Facts {
                                        ports_named,
                                        pair_present,
                                        both_uart,
                                        baseline_ok,
                                        max_reliable_baud: max,
                                        ceiling_kind: kind,
                                        baseline_restored: restored,
                                        baseline_reproved: restored,
                                        search_budget_exhausted: false,
                                    };
                                    assert!(
                                        !p14_verdict(f).0.is_unsupported(),
                                        "P14 reported unsupported for {f:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// **A stall on trial 2 or 3 is a stall, not a loss** (§6's deadline
    /// discipline; the defect an adversarial pass measured).
    ///
    /// The classification used to live in the rung fold and compare *totals over
    /// up to three trials* against a *single* payload length. Because a direction
    /// short-circuits on its first failure, an intermittent rung failing on trial
    /// 2 or 3 — which is the shape a marginal rate actually produces — had
    /// already banked one or two clean payloads, so a total stall on trial 3 read
    /// `received == 2 x payload`: neither short nor starved, therefore `Corrupt`.
    /// The identical failure on trial 1 classified correctly. The bug was a
    /// *sum* compared against a *unit*.
    #[test]
    fn a_trial_classifies_itself_and_a_late_stall_is_not_a_loss() {
        const L: u64 = 1000;
        let clean = TrialResult {
            written: L,
            received: L,
            byte_exact: true,
            hung_up: false,
            elapsed_us: 1000,
        };
        assert_eq!(clean.failure(L), None);

        // A total receive stall: everything written, nothing came back. This is
        // the trial-3 shape, and it must read as a stall whatever earlier trials
        // banked — which is exactly what a per-trial classification guarantees
        // and a summed one could not.
        let starved = TrialResult {
            written: L,
            received: 0,
            byte_exact: false,
            ..clean
        };
        assert_eq!(starved.failure(L), Some(RungOutcome::TimedOut));
        // A transmit-side stall.
        let short_write = TrialResult {
            written: L / 2,
            received: L / 2,
            ..starved
        };
        assert_eq!(short_write.failure(L), Some(RungOutcome::TimedOut));
        // Enough bytes, wrong bytes: the only shape that is a loss.
        let corrupt = TrialResult {
            written: L,
            received: L + 8,
            byte_exact: false,
            hung_up: false,
            elapsed_us: 1000,
        };
        assert_eq!(corrupt.failure(L), Some(RungOutcome::Corrupt));
        // A hangup outranks both: the rig left, no rate was reached.
        let gone = TrialResult {
            hung_up: true,
            ..corrupt
        };
        assert_eq!(gone.failure(L), Some(RungOutcome::HungUp));
        // And the discriminator has teeth: the two failures that differ only in
        // whether the bytes arrived must not fold to the same word.
        assert_ne!(starved.failure(L), corrupt.failure(L));
    }

    /// **A vanished peer and an exhausted budget are not ceilings** — the two
    /// arms an adversarial pass found reporting `supported` over a search that
    /// had not finished asking its question.
    #[test]
    fn a_hangup_and_an_exhausted_budget_both_refuse_to_report_a_ceiling() {
        // A rate change against an unplugged adapter answers EIO, which carries
        // an errno — so the errno rule alone called it `platform-refused` and the
        // verdict then told the operator that *this kernel's ask surface* stops
        // here and another kernel might ask for more over the same cable. The
        // cable was gone.
        let eio = std::io::Error::from_raw_os_error(libc::EIO);
        assert_eq!(p14_refusal(&eio), RungOutcome::HungUp);
        for errno in [libc::ENXIO, libc::ENODEV] {
            assert_eq!(
                p14_refusal(&std::io::Error::from_raw_os_error(errno)),
                RungOutcome::HungUp
            );
        }
        // An errno-less `InvalidInput` is serial2's own pre-syscall refusal of an
        // unlisted rate on the unix targets that are neither Apple/BSD nor Linux.
        // Blaming the adapter's divisor model there names silicon that was never
        // told the number.
        assert_eq!(
            p14_refusal(&std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsupported baud rate"
            )),
            RungOutcome::PlatformRefused
        );

        // The two verdict arms.
        let (status, why) = p14_verdict(P14Facts {
            ceiling_kind: Some(CeilingKind::HungUp),
            ..p14_good()
        });
        assert!(matches!(status, Status::Degraded), "{}", status.label());
        assert!(why.contains("went away mid-search"), "{why}");
        assert!(why.contains("not a ceiling"), "{why}");

        let (status, why) = p14_verdict(P14Facts {
            search_budget_exhausted: true,
            ..p14_good()
        });
        assert!(matches!(status, Status::Degraded), "{}", status.label());
        assert!(why.contains("wall-clock budget"), "{why}");
        // And the control: with the budget intact the same facts are supported,
        // so the arm above is about the budget and not about something else.
        assert!(matches!(p14_verdict(p14_good()).0, Status::Supported));
    }

    /// **Every P14 path emits the three cells the gate requires, including the
    /// ones that return before the search runs.**
    ///
    /// `expectations/*.jq` exempts `skipped` and requires `max_reliable_baud`,
    /// `ceiling_kind` and `ceiling_is_a_floor_over` otherwise. Two `degraded` arms
    /// returned before the observation block — a pair whose P5 ladder did not
    /// round-trip, and a pair that would not reopen — so a real rig with a
    /// marginally seated cable produced a `degraded` report the gate **rejected**,
    /// which is the opposite of what the `degraded` arm is for.
    ///
    /// The companion guard in `itest/tests/expectation_gates.rs` proves the
    /// *clause* admits the shape; this one proves the *probe* produces it, which
    /// is the half that reddens when the stamp is removed. It reaches the arm with
    /// no hardware: a verified pair whose `baseline_ok` is false returns before any
    /// port is opened.
    #[test]
    fn every_p14_path_carries_the_cells_the_gate_requires() {
        let pair = VerifiedPair {
            a: PathBuf::from("/dev/does-not-exist-a"),
            b: PathBuf::from("/dev/does-not-exist-b"),
            a_name: "a".into(),
            b_name: "b".into(),
            both_uart: true,
            // The arm under test: discovery paired them, the certificate's rate
            // ladder did not round-trip, so the question cannot be asked.
            baseline_ok: false,
        };
        let ports = vec![pair.a.clone(), pair.b.clone()];
        let p = p14_max_rate(&ports, std::slice::from_ref(&pair));
        assert_eq!(
            p.status.label(),
            "degraded",
            "a pair whose baseline failed must degrade, not answer"
        );
        for key in [
            "max_reliable_baud",
            "ceiling_kind",
            "ceiling_is_a_floor_over",
        ] {
            assert!(
                p.observations.iter().any(|o| o.key == key),
                "the {key} cell is missing from a `degraded` P14, so                  `expectations/*.jq` rejects a report whose only fault is that the                  rig's cable was marginal: {:?}",
                p.observations.iter().map(|o| &o.key).collect::<Vec<_>>()
            );
        }
        // And the skip paths carry them too — the gate exempts `skipped`, but a
        // reader diffing cell sets across kernels should not see them appear and
        // disappear with the rig.
        let none = p14_max_rate(&[], &[]);
        assert_eq!(none.status.label(), "skipped");
        assert!(
            none.observations
                .iter()
                .any(|o| o.key == "max_reliable_baud"),
            "even a skipped P14 should carry the cell, so `field_set` does not              move between a passive run and a rig-less one"
        );
    }

    #[test]
    fn p14_payload_is_constant_airtime_until_the_cap_binds_and_never_aliases() {
        // The reliability bar must be the same at 9600 as at 3 Mbaud, or a
        // higher rung is judged on a shorter test than a lower one and the
        // ceiling is an artefact of the payload size.
        for baud in [9_600u32, 115_200, 921_600, 2_000_000] {
            let len = p14_payload_len(baud);
            let airtime_ms = (len as u64 * P14_BITS_PER_BYTE * 1000) / baud as u64;
            assert!(
                airtime_ms.abs_diff(P14_AIRTIME_MS) <= 2,
                "{baud} baud sizes {len} bytes = {airtime_ms} ms, not {P14_AIRTIME_MS} ms"
            );
        }
        // Below the floor and above the cap the airtime is *not* constant, and
        // that is deliberate and bounded — stated here so a later reader does
        // not repair a property this test never claimed.
        assert_eq!(p14_payload_len(1), P14_PAYLOAD_FLOOR);
        assert_eq!(p14_payload_len(P14_MAX_BAUD), P14_PAYLOAD_CAP);

        // A payload must not be satisfiable by the previous trial's leftovers:
        // the rate, the direction and the trial index all reach the bytes.
        let a = p14_payload(115_200, "AB", 0, 256);
        assert_ne!(a, p14_payload(115_200, "AB", 1, 256), "trial index");
        assert_ne!(a, p14_payload(115_200, "BA", 0, 256), "direction");
        assert_ne!(a, p14_payload(230_400, "AB", 0, 256), "rate");
        assert_eq!(a.len(), 256);
    }

    #[test]
    fn p14_reads_the_refusing_surface_off_the_error_rather_than_its_message() {
        // serial2 collapses a rejected `tcsetattr` and its own post-set
        // verification into one `Err`. Only the first carries an errno, and that
        // — not the crate's error string, which is not a contract — is what
        // separates "the platform refused the ask" from "the adapter landed
        // elsewhere". Measured on the crossover rig 2026-08-05: asking an FT232R
        // for 4000000 on Linux 7.0.0-29 succeeds at the syscall and reads back
        // **9600**, so the errno arm never fires there and the adapter arm is
        // the one that must be right.
        let syscall = std::io::Error::from_raw_os_error(libc::EINVAL);
        assert_eq!(p14_refusal(&syscall), RungOutcome::PlatformRefused);
        let verification = std::io::Error::other("failed to apply some or all settings");
        assert_eq!(p14_refusal(&verification), RungOutcome::AdapterRefused);
    }

    // -- P8/P9/P10: an error is not a skip (§13) ------------------------------

    /// **`skipped` is never an error path's output** (§13), asserted on the three
    /// probes that broke the rule until 2026-08-12.
    ///
    /// The consequence was not cosmetic. `skipped` is the word every conditional
    /// clause in `expectations/{linux,macos}.jq` exempts, so each of these three
    /// error paths exempted itself from the clauses written to notice a missing
    /// measurement: P8's and P9's own status clauses (`supported` or `skipped`),
    /// P9's `zero_timeout_by_fd_state` discriminator, and both of P10's content
    /// clauses. A report in which all three errored passed `jq -e -f
    /// expectations/linux.jq` at exit 0; spelled `degraded` it exits 1. The gate
    /// half of that pair is proven against the real expectation file in
    /// `itest/tests/expectation_gates.rs`; this is the probe half — which the
    /// error arms had no way to have at all until [`p8_verdict`], [`p9_verdict`]
    /// and [`p10_verdict`] were split from their measurements (§13: verdicts are
    /// pure functions of measured facts), because no box that can open a pty can
    /// be asked to fail one.
    ///
    /// Fail-first, run against the unfixed tree: all three assert `degraded` and
    /// read back `skipped`.
    #[test]
    fn a_kernel_diff_probe_that_errored_degrades_and_names_the_error() {
        // The shape a container with no `/dev/ptmx` produces — the realistic way
        // these three fail, and the one place all three converge.
        let boom = || anyhow::anyhow!("posix_openpt: ENOENT: No such file or directory");
        for p in [
            p8_verdict(Err(boom())),
            p9_verdict(Err(boom())),
            p10_verdict(Err(boom()), Err(boom())),
        ] {
            assert_eq!(
                p.status.label(),
                "degraded",
                "{} spelled a probe error `{}`, which is the one word that exempts \
                 every conditional gate clause — the measurement is missing and the \
                 gate would certify it anyway (§13)",
                p.id,
                p.status.label()
            );
            // A verdict word cannot be diffed across kernels (§13), so the reason
            // has to ride in a cell as well as in the prose. `degraded` carries no
            // `reason` field of its own — that is `skipped`'s — which is exactly
            // why this observation exists.
            assert_eq!(
                observed(&p, "probe_error"),
                Some("posix_openpt: ENOENT: No such file or directory".into()),
                "{}'s degraded verdict names no error: {:#?}",
                p.id,
                p.observations
            );
            assert!(
                p.consequence.contains("posix_openpt: ENOENT"),
                "{}'s consequence sentence does not say what failed, so the report \
                 an operator pastes into a thread reads as a shrug: {}",
                p.id,
                p.consequence
            );
        }
    }

    /// The other half of the same rule, and the one a repair can quietly destroy:
    /// **a genuine skip must still skip.**
    ///
    /// P8 off Linux has no mechanism to measure — `serial_nexus_sys::Epoll` keeps a
    /// stub whose every method answers `ENOTSUP` rather than `#[cfg]`-gating the
    /// probe away — and "unmeasurable here" is not "the probe failed" (§13, and
    /// §15.47's unmeasurable-as-data rule). Routing it to `degraded` with the rest
    /// would redden the macOS lane on every run, which is how a rule against
    /// over-skipping turns into a rule against reporting.
    #[test]
    fn p8_still_skips_where_epoll_does_not_exist() {
        // **Both carriers, because only one of them is the Darwin path.**
        // `sys::Epoll::new()` off Linux returns
        // `std::io::Error::from_raw_os_error(libc::ENOTSUP)` — the *io* branch of
        // `is_unsupported_errno` — while a `nix` call returns a `nix::Error`. A guard
        // that fed only the `nix` shape would assert this skip on a code path Darwin
        // never takes, which is AGENTS §9's proxy in space: passing on the box it was
        // written on and saying nothing about the platform it protects.
        let carriers: [(&str, anyhow::Error); 2] = [
            (
                "the io::Error the serial-nexus-sys epoll stub really returns",
                std::io::Error::from_raw_os_error(libc::ENOTSUP).into(),
            ),
            ("a nix::Error ENOTSUP", nix::Error::ENOTSUP.into()),
        ];
        for (what, err) in carriers {
            let p = p8_verdict(Err(err));
            assert_eq!(
                p.status.label(),
                "skipped",
                "{what}: the epoll stub's ENOTSUP is a mechanism that does not exist \
                 here, not a failed measurement: {:?}",
                p.status
            );
            assert!(
                p.status.badge_label().contains("epoll is Linux-only"),
                "{what}: the skip does not say why: {}",
                p.status.badge_label()
            );
            assert_eq!(
                observed(&p, "probe_error"),
                None,
                "{what}: an unmeasurable-here skip must not claim an error it did not have"
            );
        }
    }
}
