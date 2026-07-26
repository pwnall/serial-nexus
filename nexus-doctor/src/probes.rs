//! The capability probes (design §15.17, plan §3): P1 EXTPROC/TIOCPKT, P2 PTY
//! presence, P3 serial-port fit, P4 by-id resolution — plus environment checks.
//! Each returns a self-judging [`Probe`]. The kernel probes (P1, P2) and the
//! resolver probe (P4) are passive and always safe to run; P3 opens a real
//! serial port and therefore runs only on an explicitly named `--port`.

use std::io::Read;
use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

use crate::report::{EnvCheck, Probe, Status};
use nexus_sys as sys;

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
            // functional). See nexus-daemon `nodes::pty::with_termios_fd`.
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
// P4 — by-id resolution ground truth (§12)
// ---------------------------------------------------------------------------

// The by-id + sysfs walk that produces `usb:vid:pid:serial:iface` lives in
// `nexus_core::resolver` (the daemon and the doctor share one implementation,
// §12); the doctor observes what that resolver reports.

pub fn p4_resolver(dev_root: &Path, sys_root: &Path) -> Probe {
    let p = Probe::new(
        "P4",
        "by-id resolution ground truth",
        "Does /dev/serial/by-id plus a dependency-free sysfs walk yield the canonical usb:vid:pid:serial:iface identity (§12)?",
    );
    let by_id = dev_root.join("dev/serial/by-id");
    if !by_id.is_dir() {
        return p.verdict(
            Status::skipped("no /dev/serial/by-id tree"),
            "No USB-serial adapter present; identity resolution untested here (run on an adapter-equipped box).",
        );
    }
    let adapters = nexus_core::Resolver::with_roots(dev_root, sys_root).discover_adapters();
    if adapters.is_empty() {
        return p.verdict(
            Status::skipped("by-id tree present but empty"),
            "No adapters to resolve.",
        );
    }
    let mut p = p.observe("count", adapters.len() as u64);
    let mut all_resolved = true;
    for a in &adapters {
        let val = a.identity.clone().unwrap_or_else(|| "by-path only".into());
        p = p.observe(&a.by_id_name, val);
        if a.identity.is_none() {
            all_resolved = false;
        }
    }
    if all_resolved {
        p.verdict(
            Status::Supported,
            "Resolver produces canonical identities; configs survive replug and cold start (§12).",
        )
    } else {
        p.verdict(
            Status::Degraded,
            "Some adapters resolve only by topology (no serial number) → by-path fallback with a documented instability warning (§12).",
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
// half-crossed pair is named); characterization certifies real UARTs and reports
// `skipped (not a UART)` for the sim pts used in CI. The doctor certifies the
// rig and stops — it never drives the daemon through it.
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

/// One read (up to the port's short read timeout): the bytes available now, or
/// empty on timeout.
fn p5_read_once(sp: &SerialPort) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    match sp.read(&mut buf) {
        Ok(n) => buf[..n].to_vec(),
        Err(_) => Vec::new(),
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
fn p5_name(port: &Path, resolver: &nexus_core::Resolver) -> String {
    for a in resolver.discover_adapters() {
        if a.dev_path == port
            && let Some(id) = a.identity
        {
            return id;
        }
    }
    port.display().to_string()
}

/// Whether a fd is a real UART: a pts (the CI sim) fails `TIOCGICOUNT`.
fn p5_is_uart(sp: &SerialPort) -> bool {
    sys::read_icounts(sp.as_raw_fd()).is_ok()
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
}

impl Certificate {
    /// A certificate line with no failures yet.
    fn new(line: impl Into<String>) -> Self {
        Certificate {
            line: line.into(),
            failures: Vec::new(),
        }
    }

    /// A certificate that could not be produced at all (the port would not
    /// reopen). The rig may be fine; it is simply uncharacterized → degrade.
    fn unavailable(line: impl Into<String>, item: &str) -> Self {
        Certificate {
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
    // The ladder is the integrity item: a rung that did not round-trip means the
    // rig itself corrupts or loses data, so no tier failure measured through it is
    // attributable to serial_nexus (§15.21) — a stop condition, not a footnote.
    cert.fail_if(!ladder_ok, "rate_ladder", true);
    // An unobserved deliberate mismatch means the error counters are not
    // observable on this rig: the data path is fine, the characterization is not.
    cert.fail_if(!mismatch_observed, "deliberate_mismatch", false);
    cert
}

pub fn p5_rig(ports: &[PathBuf], resolver: &nexus_core::Resolver) -> Probe {
    let mut p = Probe::new(
        "P5",
        "rig discovery and certification",
        "Classify each named port (dangling/loopback/paired, both directions) and certify the rig for a tiered checklist run (§13, §15.21).",
    );

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
        return p.verdict(
            Status::skipped(reason),
            "Grant access (udev GROUP=plugdev, or dialout) and re-run with the rig's --ports.",
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
    let deadline = Instant::now() + Duration::from_millis(4000);
    let mut next_send = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_send {
            for (i, sp) in sps.iter().enumerate() {
                if let Some(sp) = sp {
                    let _ = p5_write_all(sp, &p5_nonce(i));
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
            let ready = pfds[idx]
                .revents()
                .map(|r| r.intersects(PollFlags::POLLIN | PollFlags::POLLHUP))
                .unwrap_or(false);
            if ready {
                bufs[*i].extend_from_slice(&p5_read_once(sp));
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
    #[allow(clippy::needless_range_loop)]
    for i in 0..ports.len() {
        if sps[i].is_none() {
            continue;
        }
        let name = p5_name(&ports[i], resolver);
        let classification = if heard(i, i) {
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
                Certificate::skipped("not a UART")
            };
            p = p.observe(format!("{name} cert").as_str(), cert.line);
            failures.extend(cert.failures.into_iter().map(|f| f.qualified(&name)));
        }
    }
    // Paired UARTs get the independent-clock certificate (rate ladder + mismatch).
    for (i, j) in pairs {
        let (Ok(a_uart), Ok(b_uart)) = (
            p5_open(&ports[i], 115_200, Parity::None).map(|sp| p5_is_uart(&sp)),
            p5_open(&ports[j], 115_200, Parity::None).map(|sp| p5_is_uart(&sp)),
        ) else {
            continue;
        };
        if a_uart && b_uart {
            let subject = format!(
                "{} ↔ {}",
                p5_name(&ports[i], resolver),
                p5_name(&ports[j], resolver)
            );
            let cert = p5_certify_pair(&ports[i], &ports[j]);
            p = p.observe(format!("{subject} cert").as_str(), cert.line);
            failures.extend(cert.failures.into_iter().map(|f| f.qualified(&subject)));
        }
    }

    let (status, consequence) = p5_verdict(clean, any_uart, &failures);
    p.verdict(status, &consequence)
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
fn p5_verdict(clean: bool, any_uart: bool, failures: &[CertFailure]) -> (Status, String) {
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
        (
            Status::Supported,
            "Rig discovered and certified; every tiered checklist run starts from this certificate (§15.21).".to_owned(),
        )
    } else {
        (
            Status::Supported,
            "Rig discovered and classified (above); characterization skipped on non-UART sims — the certificate populates on real adapters (§13, no-target doctrine).".to_owned(),
        )
    }
}

// ---------------------------------------------------------------------------
// P3 — serial-port fit (§7.1, §13). Runs only on an explicitly named --port.
// ---------------------------------------------------------------------------

pub fn p3_serial(port: &Path) -> Probe {
    let p = Probe::new(
        "P3",
        &format!("serial-port fit ({})", port.display()),
        "Custom baud acceptance, TIOCEXCL exclusivity, modem-line set/read, and break toggling on a real port (§7.1).",
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
// Environment checks
// ---------------------------------------------------------------------------

pub fn environment(dev_root: &Path, sys_root: &Path, named_ports: &[PathBuf]) -> Vec<EnvCheck> {
    let mut checks = Vec::new();

    let kernel = read_trimmed(Path::new("/proc/sys/kernel/osrelease")).unwrap_or_default();
    checks.push(EnvCheck::new("kernel", kernel, Status::Supported));
    checks.push(EnvCheck::new("os", distro(), Status::Supported));

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

    // by-id tree.
    let by_id = dev_root.join("dev/serial/by-id");
    let adapters = nexus_core::Resolver::with_roots(dev_root, sys_root).discover_adapters();
    if by_id.is_dir() {
        checks.push(EnvCheck::new(
            "/dev/serial/by-id",
            format!("present ({} adapter(s))", adapters.len()),
            Status::Supported,
        ));
    } else {
        checks.push(EnvCheck::new(
            "/dev/serial/by-id",
            "absent (no USB-serial adapter)",
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

    // Access to each discovered or named serial device node.
    let mut ports: Vec<PathBuf> = adapters.iter().map(|a| a.dev_path.clone()).collect();
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

fn distro() -> String {
    if let Some(content) = read_trimmed(Path::new("/etc/os-release")) {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                return v.trim_matches('"').to_owned();
            }
        }
    }
    "unknown".into()
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

    /// The sim path (§15.21: "characterization reporting skipped on non-UARTs, so
    /// P5's logic never waits for a bench") and the fully certified rig both stay
    /// `supported`, with their consequence lines unchanged.
    #[test]
    fn a_clean_rig_is_supported_certified_or_skipped() {
        let (status, why) = p5_verdict(true, true, &[]);
        assert_eq!(status.label(), "supported");
        assert!(why.contains("certified"), "{why}");

        let (status, why) = p5_verdict(true, false, &[]);
        assert_eq!(status.label(), "supported");
        assert!(why.contains("skipped on non-UART sims"), "{why}");
    }

    /// A data-integrity failure is a stop condition: the certificate is the
    /// precondition every tiered run starts from, so a rig that loses bytes must
    /// not report `supported` with exit code 0 (review 26, DOC-1b).
    #[test]
    fn a_data_integrity_failure_is_unsupported_and_names_the_item() {
        let (status, why) = p5_verdict(true, true, &[fail("usb-A ↔ usb-B: rate_ladder", true)]);
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
        let (status, why) = p5_verdict(true, true, &[fail("usb-A: break", false)]);
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("usb-A: break"), "item not named: {why}");
    }

    /// Miswiring keeps its own (discovery-side) message, and still names any
    /// certificate item that also failed — both facts reach the operator.
    #[test]
    fn miswiring_degrades_and_still_reports_uncertified_items() {
        let (status, why) = p5_verdict(false, true, &[fail("usb-A: custom_baud", false)]);
        assert_eq!(status.label(), "degraded");
        assert!(why.contains("miswired"), "{why}");
        assert!(why.contains("usb-A: custom_baud"), "{why}");
    }

    /// Integrity outranks miswiring: a half-crossed rig that also corrupts data is
    /// a stop condition, not a warning.
    #[test]
    fn integrity_failure_outranks_miswiring() {
        let (status, _) = p5_verdict(
            false,
            true,
            &[fail("a: break", false), fail("a ↔ b: rate_ladder", true)],
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
            p5_verdict(true, false, &skipped.failures).0.label(),
            "supported"
        );

        let missing = Certificate::unavailable("pair reopen failed", "pair_reopen");
        assert_eq!(missing.failures, vec![fail("pair_reopen", false)]);
        assert_eq!(
            p5_verdict(true, true, &missing.failures).0.label(),
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

    /// `fail_if` only records failures — a passing item leaves the certificate
    /// clean, which is what keeps a good rig at `supported`.
    #[test]
    fn fail_if_records_only_failures() {
        let mut cert = Certificate::new("line");
        cert.fail_if(false, "custom_baud", false);
        cert.fail_if(true, "break", false);
        assert_eq!(cert.failures, vec![fail("break", false)]);
    }
}
