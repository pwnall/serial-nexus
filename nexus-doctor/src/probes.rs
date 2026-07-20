//! The capability probes (design §15.17, plan §3): P1 EXTPROC/TIOCPKT, P2 PTY
//! presence, P3 serial-port fit, P4 by-id resolution — plus environment checks.
//! Each returns a self-judging [`Probe`]. The kernel probes (P1, P2) and the
//! resolver probe (P4) are passive and always safe to run; P3 opens a real
//! serial port and therefore runs only on an explicitly named `--port`.

use std::io::Read;
use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::time::Instant;

use nix::fcntl::{OFlag, open};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::{PtyMaster, grantpt, posix_openpt, ptsname_r, unlockpt};
use nix::sys::stat::Mode;
use nix::sys::termios::{LocalFlags, SetArg, cfmakeraw, tcgetattr, tcsetattr};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

use crate::report::{EnvCheck, Probe, Status};
use crate::sys;

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
    let pts = ptsname_r(&master)?;
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
            if o.after_close && !o.while_open && !o.after_reopen && o.termios_settable {
                p.verdict(
                    Status::Supported,
                    "POLLHUP presence detection works; prime the slave (open+close at creation) for the never-opened case.",
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
    let pts = ptsname_r(&master)?;

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

pub struct Adapter {
    pub by_id_name: String,
    pub dev_path: String,
    pub identity: Option<String>,
}

pub fn discover_adapters(dev_root: &Path, sys_root: &Path) -> Vec<Adapter> {
    let by_id = dev_root.join("dev/serial/by-id");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&by_id) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let dev_name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let identity = sysfs_identity(sys_root, &dev_name);
        out.push(Adapter {
            by_id_name: name,
            dev_path: format!("/dev/{dev_name}"),
            identity,
        });
    }
    out
}

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
    let adapters = discover_adapters(dev_root, sys_root);
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

fn sysfs_identity(sys_root: &Path, dev_name: &str) -> Option<String> {
    let device_link = sys_root.join("class/tty").join(dev_name).join("device");
    let start = std::fs::canonicalize(&device_link).ok()?;
    let mut interface = None;
    let mut cur: &Path = &start;
    for _ in 0..12 {
        if interface.is_none() {
            interface = read_trimmed(&cur.join("bInterfaceNumber"));
        }
        if cur.join("idVendor").exists() {
            let vid = read_trimmed(&cur.join("idVendor"))?;
            let pid = read_trimmed(&cur.join("idProduct"))?;
            let serial = read_trimmed(&cur.join("serial")).unwrap_or_else(|| "-".into());
            let iface = interface.unwrap_or_else(|| "-".into());
            return Some(format!("usb:{vid}:{pid}:{serial}:{iface}"));
        }
        match cur.parent() {
            Some(parent) if parent != cur && parent.starts_with(sys_root) => cur = parent,
            _ => break,
        }
    }
    None
}

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
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
    let icounter_supported = sys::read_icounter(sp.as_raw_fd()).is_ok();

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
    let adapters = discover_adapters(dev_root, sys_root);
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
    let mut ports: Vec<PathBuf> = adapters
        .iter()
        .map(|a| PathBuf::from(&a.dev_path))
        .collect();
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
    let groups = nix::unistd::getgroups().ok()?;
    Some(groups.contains(&target))
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
