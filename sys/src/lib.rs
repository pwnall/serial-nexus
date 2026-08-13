//! `serial-nexus-sys` — the one crate that carries `unsafe` (design §16.3).
//!
//! Every raw ioctl, `ptsname`, and `poll(2)` wrapper the daemon, the doctor, and
//! the sim need lives here — the wrappers previously existed three times (daemon,
//! doctor, sim), and the macOS port had to gate `TIOCGICOUNT`/`ptsname` per copy,
//! so a missed copy was a silent platform break. Collecting them into one internal
//! crate shrinks the audit surface for `unsafe` to a single file set with the
//! cfg-gating written once (§16.3); every other crate `#![forbid(unsafe_code)]`.

#![allow(unsafe_code)]

/// Capability inspection and process hardening for the privileged replug helper
/// (design §15.45). Linux-only: file capabilities and `prctl(2)` are Linux
/// concepts, and the helper the module supports does not build elsewhere.
#[cfg(target_os = "linux")]
pub mod caps;

/// USB re-enumeration on Darwin — the macOS half of the replug helper (§15.45).
/// macOS-only for the mirror-image reason `caps` is Linux-only: it is IOKit, and
/// the mechanism it wraps (`USBDeviceReEnumerate`) has no counterpart elsewhere.
/// Unlike the Linux path it needs no privilege, and unlike the Linux path its
/// outage is atomic — see the module docs (notes §3.66).
#[cfg(target_os = "macos")]
pub mod usb_macos;

use nix::libc;
use std::os::fd::RawFd;

nix::ioctl_write_ptr_bad!(tiocpkt, libc::TIOCPKT, libc::c_int);
nix::ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
nix::ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);
// `TIOCGICOUNT` is a Linux-only ioctl (libc exports the request code only under
// target_os = linux/android), so the binding — and only the binding — is gated to
// Linux. On other platforms `read_icounts` is a stub returning `ENOTSUP`, which the
// caller already maps to "driver counters unsupported → omit them" (§5, §13 macOS
// best-effort): the same graceful path a pts takes on Linux.
#[cfg(target_os = "linux")]
nix::ioctl_read_bad!(tiocgicount, libc::TIOCGICOUNT, SerialIcounts);
nix::ioctl_read_bad!(tiocmget, libc::TIOCMGET, libc::c_int);
// The two queue-depth ioctls are *probe instruments* (see the capability-probe
// section at the foot of this file), but their bindings are declared up here with
// every other ioctl on purpose: the argument for this crate existing is that one
// file lists every raw ioctl the workspace issues, and that argument survives only
// if the list is not split by who happens to call each entry.
//
// `FIONREAD` is POSIX-ubiquitous (Linux and the BSDs both export it, with different
// request encodings that `ioctl_read_bad!` casts for us). `TIOCOUTQ` is exported by
// libc only for Linux, so — exactly like `TIOCGICOUNT` above — the *binding* is
// gated and the wrapper keeps a non-Linux `ENOTSUP` stub.
nix::ioctl_read_bad!(fionread, libc::FIONREAD, libc::c_int);
#[cfg(target_os = "linux")]
nix::ioctl_read_bad!(tiocoutq, libc::TIOCOUTQ, libc::c_int);

/// The kernel's `serial_icounter_struct` (TIOCGICOUNT): driver-maintained input
/// counters the design surfaces in serial state *where supported* (§5, §7.1) —
/// framing/parity/overrun errors are otherwise invisible loss. The layout (and
/// the trailing `reserved[9]`) must match the kernel exactly, because the ioctl
/// writes `sizeof(serial_icounter_struct)` bytes through the pointer. Not every
/// driver implements it — a pts returns an error — so callers treat `Err` as
/// "unsupported" and omit the counts rather than faulting.
#[repr(C)]
#[derive(Default, Clone, Copy)]
#[allow(dead_code)] // cts/dsr/rng/dcd/reserved are read by the kernel, not us.
pub struct SerialIcounts {
    pub cts: i32,
    pub dsr: i32,
    pub rng: i32,
    pub dcd: i32,
    pub rx: i32,
    pub tx: i32,
    pub frame: i32,
    pub overrun: i32,
    pub parity: i32,
    pub brk: i32,
    pub buf_overrun: i32,
    reserved: [i32; 9],
}

/// Read the driver input counters (`TIOCGICOUNT`). `Err` means the fd's driver
/// does not implement it (e.g. a pts standing in for a device in tests), which
/// the caller surfaces as "unsupported" — never a fault (§5).
#[cfg(target_os = "linux")]
pub fn read_icounts(fd: RawFd) -> nix::Result<SerialIcounts> {
    let mut counts = SerialIcounts::default();
    // Safety: the ioctl writes exactly `sizeof(SerialIcounts)` bytes into
    // `counts`, whose layout mirrors the kernel struct.
    unsafe { tiocgicount(fd, &mut counts) }?;
    Ok(counts)
}

/// Non-Linux stub: no `TIOCGICOUNT`, so report "unsupported" exactly as a pts does
/// on Linux (§5, §13). The caller (`nodes/serial.rs`) maps `Err` to omitted driver
/// counters — a runtime degradation, never a fault.
#[cfg(not(target_os = "linux"))]
pub fn read_icounts(_fd: RawFd) -> nix::Result<SerialIcounts> {
    Err(nix::errno::Errno::ENOTSUP)
}

/// Whether this build has a real `TIOCGICOUNT` behind [`read_icounts`].
///
/// It lives beside the two arms so it cannot drift from them, and it exists
/// because `Err` does not mean the same thing on both. On Linux the ioctl exists
/// and an `Err` is a *measurement* — a pts answers `ENOTTY` because that driver
/// does not implement it, which is exactly what makes "not a UART" true there.
/// Everywhere else the arm above is a compile-time `ENOTSUP` stub, so every fd
/// answers `Err`, a genuine FTDI adapter included, and no rig, cable or re-seat
/// can change it. A caller reporting an item as uncharacterized has to tell those
/// two apart: design §7 asks it to name the observation rather than send an
/// operator after a fault the kernel cannot produce.
pub const ICOUNTS_SUPPORTED: bool = cfg!(target_os = "linux");

/// Serializes the non-reentrant `ptsname(3)` arm below (see [`ptsname`]'s
/// *Safety* section). Held only across the call and the copy-out — never across
/// an `.await`, and never by the data plane's hot paths, which do not resolve
/// slave paths.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
static PTSNAME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Resolve a pty master's slave path. Linux/Android have the reentrant
/// `ptsname_r(3)` (a glibc extension nix exposes safely); elsewhere only the
/// static-buffer `ptsname(3)` exists, which nix marks `unsafe`. This wrapper hides
/// the split so the PTY node code is platform-agnostic. On the non-reentrant path
/// the returned `String` is copied out of the static buffer before this returns,
/// so no dangling reference escapes.
///
/// # Safety (the non-Linux arm's precondition, made explicit)
///
/// `ptsname(3)` returns a pointer into a *process-wide static buffer* that the
/// next call overwrites, so two concurrent callers can hand each other another
/// pty's path — the wrong slave symlink for a `pty` node (§7.2), which is a
/// silent misrouting, not a crash. The precondition was previously only
/// "single-threaded callers", true of every caller today but written in no
/// document and unenforced (review 26, SYS-1: latent, not live — this arm
/// compiles only off Linux, and Linux takes the reentrant path).
///
/// It is now enforced for callers *inside this process*: the call and the
/// copy-out run under [`PTSNAME_LOCK`], so concurrent `serial_nexus_sys::ptsname`
/// callers serialize and each observes its own result. The residual precondition
/// is the part no library can enforce — **no other code in the process may call
/// `ptsname(3)` concurrently** (a third-party crate or C library calling it
/// directly bypasses this lock). serial_nexus itself calls it only here.
pub fn ptsname(master: &nix::pty::PtyMaster) -> nix::Result<String> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        nix::pty::ptsname_r(master)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        // A poisoned lock carries no state worth refusing over — the guarded
        // region is one FFI call and a copy — so recover the guard rather than
        // turning another thread's panic into a pty node that cannot start.
        let _guard = PTSNAME_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Safety: the static buffer is read (and cloned into an owned `String`)
        // before `_guard` drops, so no other caller of this function can
        // overwrite it in between, and no reference to it escapes.
        unsafe { nix::pty::ptsname(master) }
    }
}

/// Read the modem-line bitmask (`TIOCMGET`): DTR/RTS outputs and CTS/DSR/DCD/RI
/// inputs, masked with `libc::TIOCM_*` (§7.1 "current modem-line readings"). `Err`
/// means the fd's driver does not implement it (a pts in tests), which callers
/// surface as `null` rather than a fault.
pub fn read_modem_bits(fd: RawFd) -> nix::Result<libc::c_int> {
    let mut bits: libc::c_int = 0;
    // Safety: TIOCMGET writes one int through the pointer; `bits` outlives it.
    unsafe { tiocmget(fd, &mut bits) }?;
    Ok(bits)
}

/// Packet-mode data marker: a master read whose leading control byte is
/// `TIOCPKT_DATA` (0) carries slave-written data in the remaining bytes; any
/// other leading byte is a control packet with no data (§7.2). The data plane
/// strips this byte and forwards only data payloads.
pub const TIOCPKT_DATA: u8 = 0;

/// Packet-mode control-byte flag for a termios/ioctl change on the slave
/// (stable Linux value; libc exports only the `TIOCPKT` request code). A read
/// whose leading control byte has this bit set means a client called
/// `tcsetattr`; the PTY reader reconciles client termios into state (§7.2) and
/// forwards no payload for it.
pub const TIOCPKT_IOCTL: u8 = 64;

/// Enable or disable packet mode on a pty master (§7.2).
pub fn set_packet_mode(fd: RawFd, on: bool) -> nix::Result<()> {
    let v: libc::c_int = i32::from(on);
    // Safety: `v` outlives the call; TIOCPKT reads one int through the pointer.
    unsafe { tiocpkt(fd, &v) }?;
    Ok(())
}

/// What a port's driver did with a `CRTSCTS` request — **three outcomes, because
/// two of them are indistinguishable from the read-back alone and only one of them
/// is the defect** (design §7.1 clause 7, §11, §15.53).
///
/// §7.1 makes a three-way call: *honour*, *honest refusal*, *accept-then-drop*.
/// Only the last is a defect — a driver that returns success from `tcsetattr` and
/// then reads the flag back clear leaves the link running without the flow control
/// the config asked for, silently, under exactly the conditions it was configured
/// to survive. A driver that fails the `tcsetattr` outright is **honest**: nothing
/// silent happens, the node's own open fails loudly with the driver's own error
/// (`serial2`'s `set_on_file` propagates the `tcsetattr`/`TCSETSW2` errno straight
/// out of `SerialPort::open`, verified in the pinned 0.2.37 source), and §7.1 does
/// not refuse such a config at `load`.
///
/// [`honours_flow_control`] returned `nix::Result<bool>` until 2026-08-12, which could
/// not express that: an honest refusal and an accept-then-drop both came back
/// `Ok(false)`, so the daemon refused both while doctor P15 — which *did* keep
/// `tcsetattr_ok` — called the same port `supported`. That is the split §7.1 clause
/// 2 names as worse than either verdict alone, and `shipped_predicate_agrees` could
/// not catch it because both sides were reading the same half of the measurement.
///
/// Deliberately **no `bool` accessor and no two-valued shim.** The two-valued shape
/// is what let two callers collapse different facts into one answer; a caller that
/// legitimately only cares about one arm spells that arm out (`== Ok(Honoured)`),
/// which is one comparison at the call site and says which question it is asking.
/// Which flow-control mode a [`honours_flow_control`] interrogation is about.
///
/// The two arms differ in exactly one thing — which flag in which termios word is
/// set and read back — which is why they are a parameter rather than two functions
/// (§15.61, §16.5). Everything else about the measurement, including
/// [`FlowOutcome::classify`]'s three-way rule, is mode-independent and is not
/// duplicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowMode {
    /// Hardware handshaking: `CRTSCTS` in `c_cflag`.
    RtsCts,
    /// Software handshaking: `IXON | IXOFF` in `c_iflag`. `serial2`'s
    /// `FlowControl::XonXoff` sets both and clears `CRTSCTS`, and verifies the
    /// **whole** `c_iflag` word by read-back — which is why a driver that drops
    /// them faults the node's own open exactly as a dropped `CRTSCTS` does.
    XonXoff,
}

impl FlowMode {
    /// The spelling `core::config`'s `flow_control` key uses, for messages that an
    /// operator has to be able to paste back into a config.
    pub fn config_spelling(self) -> &'static str {
        match self {
            FlowMode::RtsCts => "rts-cts",
            FlowMode::XonXoff => "xon-xoff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowOutcome {
    /// The flag reads back set: the driver implements the mode and agrees it did.
    Honoured,
    /// `tcsetattr` **failed** and the flag reads back clear. Honest, and not a
    /// refusal at `load` (§7.1 clause 7) — the failure this produces later is loud.
    Refused,
    /// `tcsetattr` reported **success** and the flag still reads back clear. The
    /// defect: neither honoured nor refused. Measured on Darwin 24.6.0 with an
    /// FT232R on Apple's `IOSerialFamily` (the `acb5162` macOS triple, 2026-08-05).
    AcceptedThenDropped,
}

impl FlowOutcome {
    /// The three-way rule, as a pure function of the two readings — so it can be
    /// tested against *injected* readings on a box whose driver can only ever
    /// produce one of the arms (§9: the arm that does not run here is exactly the
    /// one a single-kernel, single-adapter session cannot exercise).
    ///
    /// **The read-back is asked first, and that ordering is load-bearing.** A port
    /// whose termios already carried `CRTSCTS` before the interrogation reads the
    /// flag back set no matter what the set returned, and POSIX lets `tcsetattr`
    /// return 0 having applied only *some* of the requested changes — so the set's
    /// status is evidence about the arms only once the flag is known to be clear.
    /// With the flag clear, `tcsetattr` having failed means the driver said no
    /// (POSIX: -1 means none of the changes were made), and `tcsetattr` having
    /// succeeded means it said yes and did nothing.
    ///
    /// Shared with doctor P15, which takes its **own** readings by hand and then
    /// classifies them through this one function: the measurement stays independent
    /// — that is what `shipped_predicate_agrees` cross-checks — while the *meaning*
    /// of the three arms exists once, so the report and `load` cannot drift about
    /// which arm a pair of readings is (§7.1 clause 2, §15.53).
    pub fn classify(set_ok: bool, honoured_on_readback: bool) -> FlowOutcome {
        if honoured_on_readback {
            FlowOutcome::Honoured
        } else if set_ok {
            FlowOutcome::AcceptedThenDropped
        } else {
            FlowOutcome::Refused
        }
    }
}

/// Does this port's driver *honour* a `CRTSCTS` request, refuse it outright, or
/// accept it and discard it?
///
/// **One implementation, because two callers must not be able to disagree.** The
/// daemon refuses a `flow = "rts-cts"` config at load time on a port that answers
/// [`FlowOutcome::AcceptedThenDropped`] here, and the doctor's P15 reports the
/// same reading; if those two measured it separately, a report could say the port
/// is fine while `load` rejects it, or worse the reverse (§7.1 clause 2, §15.53).
///
/// The question is not answerable any other way. `tcsetattr` returns **success** on
/// a driver that drops the flag — measured on Darwin 24.6.0 with an FT232R on
/// Apple's `IOSerialFamily`, where the request is accepted and the read-back is
/// clear — so only reading the termios back distinguishes honouring from
/// discarding, and only the `tcsetattr` status distinguishes discarding from an
/// honest refusal. `serial2` relies on the same read-back, which is why an
/// accept-then-drop port faults a node with `failed to apply some or all settings`
/// (notes §3.65 E), and it propagates the errno for the refusing one.
///
/// Restores the termios it found before returning, on every path. `Err` means the
/// port could not be opened or interrogated at all — which is **not** any of the
/// three answers and callers must not collapse it into one: an absent or busy
/// device is *unmeasured*, which §7.1 makes a wait, never a refusal.
/// **Both modes, one implementation** (§7.1 clause 2, §15.61). The mode is a
/// parameter because the only thing that differs between them is which flag in which
/// termios word is set and read back; the open, the restore, the "a failed set is
/// half the measurement" rule and [`FlowOutcome::classify`]'s three-way meaning are
/// mode-independent, and copying them per mode is the two-copies-that-must-agree
/// shape §16.5 bans — the one that let the daemon and P15 answer differently about a
/// port in the first place.
///
/// `XonXoff` mirrors what `serial2`'s `FlowControl::XonXoff` does — set `IXON|IXOFF`,
/// clear `CRTSCTS` — because the question this predicate answers is whether *the
/// node's own open* would fault, and `serial2` compares the whole `c_iflag` word by
/// read-back. Asking a different question than the one the product asks would make
/// this a proxy (AGENTS §9).
pub fn honours_flow_control(path: &std::path::Path, mode: FlowMode) -> nix::Result<FlowOutcome> {
    use nix::sys::termios::{ControlFlags, InputFlags, SetArg, tcgetattr, tcsetattr};
    // O_NONBLOCK so a port asserting flow control against us cannot block the open,
    // and O_NOCTTY so interrogating a port never makes it this process's terminal.
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDWR | nix::fcntl::OFlag::O_NOCTTY | nix::fcntl::OFlag::O_NONBLOCK,
        nix::sys::stat::Mode::empty(),
    )?;
    let before = tcgetattr(&fd)?;
    let mut want = before.clone();
    match mode {
        FlowMode::RtsCts => want.control_flags |= ControlFlags::CRTSCTS,
        FlowMode::XonXoff => {
            want.input_flags |= InputFlags::IXON | InputFlags::IXOFF;
            want.control_flags &= !ControlFlags::CRTSCTS;
        }
    }
    // A failed set is not an early return: it is half the measurement. It is what
    // separates the honest refusal from the silent drop, and the read-back is
    // needed either way, so both readings are taken and classified together below.
    let set = tcsetattr(&fd, SetArg::TCSANOW, &want);
    let after = tcgetattr(&fd);
    // Restore before reporting, so an error on the read-back cannot leave a real
    // adapter reconfigured behind us.
    let _ = tcsetattr(&fd, SetArg::TCSANOW, &before);
    let after = after?;
    // Both flags are required for `XonXoff`: a driver honouring one and dropping the
    // other still fails `serial2`'s whole-word comparison, so a predicate reading
    // either alone would call a port fine that the node's open refuses.
    let honoured = match mode {
        FlowMode::RtsCts => after.control_flags.contains(ControlFlags::CRTSCTS),
        FlowMode::XonXoff => after
            .input_flags
            .contains(InputFlags::IXON | InputFlags::IXOFF),
    };
    Ok(FlowOutcome::classify(set.is_ok(), honoured))
}

/// Take or release exclusive access on a tty — `TIOCEXCL` so stray processes
/// cannot share the serial port (§7.1); serial2 does not do this for us.
pub fn set_exclusive(fd: RawFd, on: bool) -> nix::Result<()> {
    // Safety: no-argument legacy ioctls on a valid fd.
    unsafe {
        if on {
            tiocexcl(fd)?;
        } else {
            tiocnxcl(fd)?;
        }
    }
    Ok(())
}

/// Put a fd into non-blocking mode (`O_NONBLOCK`) so the data plane's
/// `poll(2)` + `read(2)`/`write(2)` readiness loop ([`poll_ready`], [`read_fd`],
/// [`write_fd`]) never blocks the current-thread runtime. `tokio::io::unix::AsyncFd`
/// is deliberately *not* used for tty-family fds — it is prohibited for pty
/// masters (§15.18), whose epoll readiness busy-loops the runtime (see
/// [`poll_ready`]). `posix_openpt` leaves the fd blocking, so the data plane sets
/// this itself; the pinned `serial2` opens with `O_NONBLOCK | O_NOCTTY` already and
/// never clears it, so that call is a documented redundancy rather than the thing
/// that makes the serial fd non-blocking. Setting it there anyway is deliberate: the
/// readiness loop's correctness must not rest on a dependency's open flags, which are
/// not part of its API (37-SER-3 corrected the prose, not the call).
pub fn set_nonblocking(fd: RawFd) -> std::io::Result<()> {
    // Safety: F_GETFL/F_SETFL take no memory arguments; fd is a valid open fd.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Mark a fd close-on-exec (`FD_CLOEXEC`), so no child the daemon spawns inherits
/// a copy of it (§7.2, §16.3).
///
/// The daemon does spawn children — the exec codec's (§7.6), which `fork`+`exec`
/// with no fd cleanup of their own — and a fd that survives that exec is not merely
/// untidy: a pty **master** in a codec child holds the console's pair open, so
/// `remove-node`/`load --replace` no longer hangs up the slave and the pts index is
/// never reclaimed, and it is a live handle on the console's targetward bytes in both
/// directions. `posix_openpt` is specified to take only `O_RDWR | O_NOCTTY`
/// portably — `O_CLOEXEC` there is a Linux extension — so the flag is set as a
/// separate step, which is also the only shape that composes with `grantpt`.
///
/// `F_SETFD` carries exactly one flag, so this is a plain set rather than the
/// read-modify-write [`set_nonblocking`] needs on `F_SETFL`.
pub fn set_cloexec(fd: RawFd) -> std::io::Result<()> {
    // Safety: F_SETFD takes an int argument by value; fd is a valid open fd.
    let rc = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// A single non-blocking `read(2)`. `EAGAIN` surfaces as
/// [`std::io::ErrorKind::WouldBlock`], the signal to stop draining and wait for
/// the next readiness poll — the read primitive the data-plane tasks drive.
pub fn read_fd(fd: RawFd, buf: &mut [u8]) -> std::io::Result<usize> {
    // Safety: `buf` is valid for writes of `buf.len()` bytes; read writes at
    // most that many and returns the count (or -1 with errno set).
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
    if n < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

/// A single non-blocking `write(2)`. Like [`read_fd`], `EAGAIN` surfaces as
/// [`std::io::ErrorKind::WouldBlock`] for the readiness retry loop.
pub fn write_fd(fd: RawFd, buf: &[u8]) -> std::io::Result<usize> {
    // Safety: `buf` is valid for reads of `buf.len()` bytes; write reads at most
    // that many and returns the count (or -1 with errno set).
    let n = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
    if n < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

/// A single non-blocking readiness check (`poll(2)` with a zero timeout). Returns
/// the reported events immediately without ever blocking the thread — so it is
/// safe to call from the current-thread data plane.
///
/// This is the data plane's readiness primitive *instead of*
/// `tokio::io::unix::AsyncFd`: on a pty master, `AsyncFd`'s epoll-based readiness
/// spuriously and *persistently* reports "readable" (epoll disagrees with
/// `poll(2)`, which reports the true empty state), which busy-loops and starves
/// the whole single-threaded runtime. A direct `poll(2)` reports the truth.
pub fn poll_ready(fd: RawFd, interest: nix::poll::PollFlags) -> nix::poll::PollFlags {
    use nix::poll::{PollFd, PollTimeout, poll};
    // Safety: `fd` is a valid open fd kept alive by the caller across this call.
    let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
    let mut fds = [PollFd::new(borrowed, interest)];
    let _ = poll(&mut fds, PollTimeout::ZERO);
    fds[0].revents().unwrap_or_else(nix::poll::PollFlags::empty)
}

/// A *blocking* readiness poll with a bounded timeout — for use only off the
/// async runtime thread (via `spawn_blocking` or a dedicated thread). The kernel
/// wakes it the instant the fd is ready, so a high-throughput reader drains at
/// line rate and a *quiescent* one costs zero CPU while parked — the hybrid data
/// plane's hot-path hatch (§15.19; a blocking helper off the runtime thread,
/// never epoll, which misreports pty-master readiness, §15.18). Returns the
/// reported events (empty on timeout, so the caller can re-arm and observe a
/// stop flag).
pub fn poll_blocking(
    fd: RawFd,
    interest: nix::poll::PollFlags,
    timeout_ms: u16,
) -> nix::poll::PollFlags {
    use nix::poll::{PollFd, PollTimeout, poll};
    // Safety: `fd` is a valid open fd kept alive by the caller across this call.
    let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
    let mut fds = [PollFd::new(borrowed, interest)];
    let _ = poll(&mut fds, PollTimeout::from(timeout_ms));
    fds[0].revents().unwrap_or_else(nix::poll::PollFlags::empty)
}

/// Has the far end hung up on this tty-family fd? A zero-timeout `poll(2)` asking
/// exactly that and nothing else.
///
/// **Why this exists as a named question rather than a `poll_ready` call at the
/// caller: the mask is load-bearing, and it is load-bearing on one kernel only.**
/// POSIX says `POLLHUP` is reported in `revents` whatever the caller requested, and
/// Linux does that — but **Darwin gates it on the requested mask**, measured rather
/// than read: P9 ships the cell `hangup_delivered_to_a_mask_that_requested_nothing`,
/// which reads `true` on Linux 7.0.0-29 and **`false`** on Darwin 24.6.0 in the
/// committed captures of the `4317ea5ac187f506` era, and an independent poll of a
/// `posix_openpt` pair on the Darwin rig box reproduces it exactly: after the
/// master's close, an empty-mask poll of the slave answers `none` while a
/// `POLLHUP`-requesting poll of the same fd answers `POLLHUP` (notes §3.93).
///
/// So a helper written and self-tested on Linux with an empty mask passes there and
/// is **silently dead on Darwin** — the one platform whose persistent devfs nodes
/// make this question necessary at all (§15.59, plan §18 item 66). Spelling the mask
/// once, here, is what keeps that trap out of every caller.
///
/// **What a `true` does and does not license.** It means this descriptor's peer has
/// gone: for a pts slave, the master closed. It says nothing about whether any
/// *process* on the other side is alive, and **nothing at all about a serial fd**,
/// whose peer is a wire and cannot hang up this way — measured on the rig box, where
/// a real FT232R port answers `none` both while the far end is open and after it
/// closes, with the mask requested either way. A serial witness therefore never sees
/// this fire, which is why adding it to a shared witness is safe rather than merely
/// convenient.
///
/// Takes `AsFd` rather than a `RawFd` deliberately: the borrow keeps the descriptor
/// alive across the call as a compile-time fact instead of the promise
/// [`poll_ready`]'s safety comment has to make (§15.59 records the same correction
/// being made inside P16).
pub fn peer_hungup<F: std::os::fd::AsFd>(fd: &F) -> bool {
    use nix::poll::PollFlags;
    use std::os::fd::AsRawFd;
    poll_ready(fd.as_fd().as_raw_fd(), PollFlags::POLLHUP).contains(PollFlags::POLLHUP)
}

/// Total CPU time (user + system) charged to `pid` so far, in **nanoseconds**.
///
/// **The portable analogue the tree said did not exist** (plan §18 items 12 and 13).
/// `p3_idle_cost` stated in-tree that "there is no portable analogue" to
/// `/proc/<pid>/stat`, and on that sentence three CPU guards were gated to Linux —
/// `p9_pty_collapse`'s anti-spin assertion, `p12_sim_idle_cpu`'s §15.36 idle-CPU
/// tripwire, and `p3_idle_cost` itself. The sentence was true of *`/proc`*, not of
/// the question: Darwin answers it with `proc_pid_rusage`, whose `ri_user_time` and
/// `ri_system_time` are already nanoseconds. Nanoseconds are therefore the shared
/// unit, and the Linux arm converts up rather than the Darwin arm converting down —
/// clock ticks are the coarser instrument (10 ms on a `USER_HZ` of 100) and rounding
/// a measurement to a resolution neither kernel imposes would throw away the finer
/// answer to make the two look alike.
///
/// **What it measures is the *process*, not this one.** `getrusage(RUSAGE_SELF)` is
/// the wrong instrument for every caller here — they sample a *daemon they spawned* —
/// and `RUSAGE_CHILDREN` counts only children already reaped, so it reads zero for a
/// child that is still running, which is exactly the case under test. Both were
/// evaluated and rejected before this was written.
///
/// Same-uid is sufficient on both kernels; no privilege is needed and none is taken.
pub fn process_cpu_nanos(pid: u32) -> std::io::Result<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
        // The `comm` field is parenthesised and may itself contain spaces, so the
        // split starts after the LAST ')' — the same parse `itest::cpu_ticks` used,
        // kept byte-for-byte so the Linux numbers this replaces do not move.
        let tail = &stat[stat.rfind(')').ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "no comm field")
        })? + 1..];
        let fields: Vec<&str> = tail.split_whitespace().collect();
        // `tail` starts at field 3 (state), so utime (14) and stime (15) are at 11, 12.
        let parse = |i: usize| -> std::io::Result<u64> {
            fields
                .get(i)
                .and_then(|f| f.parse::<u64>().ok())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("/proc/{pid}/stat has no field {i}"),
                    )
                })
        };
        let ticks = parse(11)? + parse(12)?;
        // Safety: `sysconf` takes an int and returns a long; no pointers involved.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        let hz = if hz > 0 { hz as u64 } else { 100 };
        Ok(ticks.saturating_mul(1_000_000_000 / hz))
    }
    #[cfg(target_os = "macos")]
    {
        // `rusage_info_v0` opens with a 16-byte uuid, then `ri_user_time` and
        // `ri_system_time` as `u64`. Read through a sized buffer rather than a declared
        // struct because the `rusage_info_v*` family grows with the OS version and only
        // the leading, stable prefix is wanted here.
        let mut buf = [0u8; 512];
        // Safety: `buf` is 512 bytes and outlives the call; `RUSAGE_INFO_V0` (0) asks
        // the kernel to write `rusage_info_v0`, which is far smaller.
        let rc = unsafe { libc::proc_pid_rusage(pid as i32, 0, buf.as_mut_ptr().cast()) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let at = |o: usize| u64::from_ne_bytes(buf[o..o + 8].try_into().expect("8 bytes"));
        let ticks = at(16).saturating_add(at(24));

        // **Those fields are mach absolute time, not nanoseconds, and the difference is
        // invisible on Intel.** `mach_timebase_info` reads `numer=1 denom=1` on x86_64,
        // so ticks *are* nanoseconds there and a helper written and measured on an
        // Intel Mac looks perfect. On Apple Silicon the timebase is 125/3 — a tick is
        // ~41.67 ns — so the same code under-reports by ~24x. Measured, not read: this
        // function shipped without the conversion and CI's arm64 macOS runner failed
        // its self-test on the arm that asserts a deliberate CPU burn exceeds one Linux
        // clock tick, which a 24x under-count puts right at the boundary (notes §3.100).
        // One more instance of the session's own theme — correct on the box that wrote
        // it, wrong on the machine that runs it.
        // The `#[allow(deprecated)]` is scoped to this one function rather than to the
        // statements that need it, because the deprecation lands on the struct's
        // *fields* as well as its name. It is the `libc` crate's own policy ("use the
        // `mach2` crate"), not an Apple removal — the symbol is stable — and taking a
        // new dependency into `serial_nexus_sys` to read two `u32`s would move the
        // workspace's dependency graph, `cargo deny`, and a second lockfile (item 58's
        // standing consequence) for no behaviour.
        #[allow(deprecated)]
        fn mach_timebase() -> (u64, u64) {
            let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
            // Safety: writes the two `u32`s of a stack-owned struct; no pointers escape.
            let rc = unsafe { libc::mach_timebase_info(&mut info) };
            // A failure here would make every reading silently wrong, so fall back to
            // the identity rather than to a guess: on the platform where the identity
            // is wrong (arm64) the call does not fail, and on the one where it is right
            // (x86_64) the fallback is exact.
            if rc == 0 && info.numer > 0 && info.denom > 0 {
                (u64::from(info.numer), u64::from(info.denom))
            } else {
                (1, 1)
            }
        }
        static TIMEBASE: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
        let (numer, denom) = *TIMEBASE.get_or_init(mach_timebase);
        // u128 so the multiply cannot overflow before the divide; a process would need
        // ~584 years of CPU to trouble the u64 the result goes back into.
        Ok(
            u64::try_from(u128::from(ticks) * u128::from(numer) / u128::from(denom))
                .unwrap_or(u64::MAX),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    {
        let _ = pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no per-process CPU source is implemented for this platform",
        ))
    }
}

// ---------------------------------------------------------------------------
// SessionLatch — a session *edge*, which is not a readiness source
// ---------------------------------------------------------------------------

/// Did a pty slave open and close since the last time this was asked?
///
/// **This is not a readiness mechanism and must never become one** (invariant 1,
/// §15.18). The ban there is on `AsyncFd`/epoll as the answer to *"is this fd
/// readable"* on a tty-family fd, because epoll persistently reports a pty master
/// readable while `read` gives `EAGAIN` and busy-loops the single-threaded
/// runtime. Readiness is still [`poll_ready`]/[`poll_blocking`] and nothing else.
/// What this answers is a different question — *"did a session boundary happen"* —
/// which is a level-state question nowhere and an **edge** question everywhere, and
/// the reason it needs its own mechanism is that on Darwin no level state records
/// it at all.
///
/// # Why it exists
///
/// §6's detach-release frees an on-demand write lock when a pty client leaves, and
/// `nodes/pty.rs`'s reader recognises "a client was here" from evidence the client
/// left on the master (invariant 16): a `TIOCPKT_DATA` payload, or the
/// `TIOCPKT_IOCTL` packet a `tcsetattr` provokes. Linux keeps that packet readable
/// *past* the hangup, so a session collapsed inside one poll gap is still seen.
/// **Darwin does not**: `ptsclose` → `ttyclose` flushes both tty queues at the
/// slave's last close, and every level-triggered observable afterwards — poll
/// revents, `FIONREAD`, `TIOCOUTQ`, `TIOCGPGRP`, `TIOCMGET`, `TIOCGWINSZ`, the pts
/// inode's timestamps — is byte-identical to no session having happened. A client
/// that opened, called `tcsetattr` and closed inside one 5 ms poll gap therefore
/// kept its write lock forever, with the endpoint dead to every other writer
/// (measured 20/20 against the shipped daemon before this existed).
///
/// A `kqueue` knote registered `EVFILT_READ | EV_CLEAR` does carry that edge.
/// Measured on macOS 15.7.8 / Darwin 24.6.0 against a pair built the way
/// `PtyNode::setup` builds one: a collapsed termios-only session posts `EV_EOF`
/// 5 times out of 5, a bare open→close posts it too, and an idle hung-up master
/// posts **nothing across 200 daemon-shaped poll+read passes** — which is the
/// property that keeps this from re-firing the last-close handler forever, the
/// 99%-CPU shape `b8d8ed8` records.
///
/// # Two rules for the caller, and both are measured forge sites
///
/// 1. **The edge must not feed the backoff.** Set the caller's session latch and
///    nothing else; an edge is not data, so it must not mark the pass productive,
///    or an idle console stops backing off toward `IDLE_POLL`.
/// 2. **The daemon forges these edges itself.** Any momentary slave open the node
///    performs — §7.2's baseline re-assert, the last-close hostward flush, the
///    termios reconciliation backstop — posts an `EV_EOF` indistinguishable from a
///    client's (measured 3/3). So [`discard`](Self::discard) after any block that
///    opens the slave, and note that [`watch`](Self::watch) swallows the
///    registration edge for the same reason: registering on a master that is
///    *already* hung up posts one immediately, and every pty node starts there,
///    because setup primes the slave and closes it.
///
/// # Platform
///
/// Real on macOS, inert everywhere else, and the split is `macos` rather than this
/// crate's usual `linux`/not-`linux` because the claim is a *measurement* and macOS
/// is where it was taken. Linux needs nothing — it retains the packet, which
/// `serial-nexus-doctor` P7 reports as `latch_covers_termios_only_session: true` on both
/// kernels of record (6.18 and 7.0) — so there the latch is a no-op that costs one
/// never-taken branch per poll. Other BSDs have `kqueue` and would very likely
/// behave as Darwin does, but nobody has measured them, and a latch that silently
/// claimed to work there would be exactly the unmeasured claim §15.17 exists to
/// prevent. `serial-nexus-doctor`'s P12 reports what this mechanism does on whatever
/// kernel it runs on.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct SessionLatch {
    kq: RawFd,
}

#[cfg(target_os = "macos")]
impl SessionLatch {
    /// Register an edge watch on `fd` (a pty master), consuming whatever the
    /// registration itself posts. See the type's rule 2.
    pub fn watch(fd: RawFd) -> std::io::Result<Self> {
        // Safety: `kqueue()` takes no arguments and returns a fresh fd or -1.
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // Constructed before the registration so an early return still closes the
        // descriptor through `Drop` rather than leaking it.
        let latch = SessionLatch { kq };
        let change = libc::kevent {
            ident: fd as libc::uintptr_t,
            filter: libc::EVFILT_READ,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // Safety: `changelist` points at one initialized `kevent` the kernel reads
        // and does not retain; `nevents` is 0, so it writes nothing through the
        // null `eventlist`. `fd` need only be a valid descriptor at this instant —
        // the knote holds the kernel's own reference to the file, so the caller may
        // close `fd` afterwards without invalidating anything here.
        let rc = unsafe {
            libc::kevent(
                latch.kq,
                &change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        latch.discard();
        Ok(latch)
    }

    /// Has a session boundary been posted since the last call? Consumes it.
    pub fn took_edge(&self) -> bool {
        self.drain_flags() & libc::EV_EOF != 0
    }

    /// Throw away anything pending — for use after the caller has itself opened
    /// and closed the slave (the type's rule 2).
    pub fn discard(&self) {
        let _ = self.drain_flags();
    }

    /// Non-blocking drain of the whole queue, returning the OR of every event's
    /// flags. Never blocks the runtime thread: the timeout is zero.
    fn drain_flags(&self) -> u16 {
        const BATCH: usize = 8;
        let blank = libc::kevent {
            ident: 0,
            filter: 0,
            flags: 0,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let mut flags: u16 = 0;
        loop {
            let mut evs = [blank; BATCH];
            // Safety: `eventlist` points at `BATCH` initialized `kevent`s the
            // kernel may overwrite, and it is told exactly that many; the zero
            // `timeout` makes the call non-blocking.
            let n = unsafe {
                libc::kevent(
                    self.kq,
                    std::ptr::null(),
                    0,
                    evs.as_mut_ptr(),
                    BATCH as libc::c_int,
                    &zero,
                )
            };
            if n <= 0 {
                break;
            }
            let got = n as usize;
            for ev in &evs[..got] {
                flags |= ev.flags;
            }
            if got < BATCH {
                break;
            }
        }
        flags
    }
}

#[cfg(target_os = "macos")]
impl Drop for SessionLatch {
    fn drop(&mut self) {
        // Safety: `self.kq` came from `watch`, is never handed out, and this type
        // is not `Clone`, so nothing else can be using or closing it. The result is
        // ignored because `Drop` has nowhere to report it.
        unsafe { libc::close(self.kq) };
    }
}

/// Inert arm — see [`SessionLatch`]'s *Platform* section. Every method is the
/// "nothing happened" answer, so a caller needs no `cfg` of its own.
#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub struct SessionLatch;

#[cfg(not(target_os = "macos"))]
impl SessionLatch {
    /// Always succeeds and watches nothing.
    pub fn watch(_fd: RawFd) -> std::io::Result<Self> {
        Ok(SessionLatch)
    }

    /// Always `false`: on Linux the retained packet is the mechanism (doctor P7).
    pub fn took_edge(&self) -> bool {
        false
    }

    /// Nothing to discard.
    pub fn discard(&self) {}
}

// ---------------------------------------------------------------------------
// Capability-probe helpers (§15.17) — instruments, not data-plane primitives
//
// Everything from here down exists for `serial-nexus-doctor`: it *measures* a kernel
// behaviour so the report can print the number that was observed. None of it is a
// readiness or I/O primitive for `serial-nexus-daemon` — the data plane's readiness is
// `poll_ready`/`poll_blocking` above and nothing else (invariant 1, §15.18), and a
// helper in this section must never become the answer to "how do I wait for this
// fd" in a node.
//
// They live in this crate for the same reason the rest of the file does:
// `serial-nexus-doctor` is `#![forbid(unsafe_code)]`, so a raw syscall it needs is a safe
// wrapper here or it does not exist (§16.3, invariant 4).
//
// Why measure at all: the production kernel is Linux 6.18 and the dev box is 7.0
// (§7), and several design premises here rest on behaviour confirmed only on 7.0.
// A premise like that has to be *re-observed* on 6.18, and "re-observed" means a
// JSON number an operator can diff between two `serial-nexus-doctor --json` runs — not a
// recollection, and not a status word with the finding buried in prose.
// ---------------------------------------------------------------------------

/// `epoll_wait(2)` event bits, spelled as the kernel spells them (`EPOLLIN`,
/// `EPOLLPRI`, `EPOLLOUT`, `EPOLLERR`, `EPOLLHUP`, `EPOLLRDHUP`).
///
/// Written as literals rather than re-exported from `libc` for one reason:
/// [`EpollReady`] and [`EpollReady::flag_names`] must compile on macOS, where
/// `libc` exports no `EPOLL*` at all (§13 best-effort). The values are Linux ABI
/// and identical on every architecture, and the unit test
/// `epoll_flag_constants_match_the_kernel_abi` proves each one against `libc`
/// where `libc` has it, so this is a compile-time convenience and not a second
/// source of truth.
///
/// `EPOLLERR` and `EPOLLHUP` are reported by the kernel whether or not they were
/// requested, which is why a probe that registers only `EPOLLIN` can still observe
/// a hangup.
pub const EPOLLIN: u32 = 0x001;
/// See [`EPOLLIN`].
pub const EPOLLPRI: u32 = 0x002;
/// See [`EPOLLIN`].
pub const EPOLLOUT: u32 = 0x004;
/// See [`EPOLLIN`].
pub const EPOLLERR: u32 = 0x008;
/// See [`EPOLLIN`].
pub const EPOLLHUP: u32 = 0x010;
/// See [`EPOLLIN`].
pub const EPOLLRDHUP: u32 = 0x2000;

/// One fd's readiness exactly as `epoll_wait(2)` reported it: the fd (recovered
/// from the event's `u64` data word, which [`Epoll::add_level_readable`] stamps
/// with it) and the raw event bitmask, **undecoded**.
///
/// The bitmask is carried out whole rather than reduced to a bool because a probe
/// reports what it *observed*: a kernel that answers `EPOLLIN|EPOLLHUP` where
/// another answers `EPOLLIN` is precisely the 6.18-vs-7.0 delta these instruments
/// exist to make visible, and a bool would have thrown it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpollReady {
    /// The fd the event is about (this crate stamps the event's data word with it).
    pub fd: RawFd,
    /// The raw `events` bitmask, exactly as the kernel returned it.
    pub events: u32,
}

impl EpollReady {
    /// Whether `flag` (one of the `EPOLL*` constants above) is set.
    pub fn contains(&self, flag: u32) -> bool {
        self.events & flag != 0
    }

    /// The set bits by name, for a report a human diffs across two kernels — at
    /// 6.18-vs-7.0 review time nobody should have to decode `0x11` by hand. Bits
    /// outside the six named constants are deliberately *not* listed here;
    /// [`EpollReady::events`] keeps the raw truth beside this, so a caller should
    /// emit both.
    pub fn flag_names(&self) -> Vec<&'static str> {
        [
            (EPOLLIN, "EPOLLIN"),
            (EPOLLPRI, "EPOLLPRI"),
            (EPOLLOUT, "EPOLLOUT"),
            (EPOLLERR, "EPOLLERR"),
            (EPOLLHUP, "EPOLLHUP"),
            (EPOLLRDHUP, "EPOLLRDHUP"),
        ]
        .into_iter()
        .filter(|&(bit, _)| self.contains(bit))
        .map(|(_, name)| name)
        .collect()
    }
}

/// An owned `epoll(7)` set: the epoll fd is created by [`Epoll::new`] and closed
/// on `Drop`, so a probe that panics or returns early cannot leak it.
///
/// # Why epoll exists here at all, when invariant 1 bans it
///
/// Invariant 1 bans epoll-based readiness *in the data plane*: on a pty master,
/// tokio's readiness guard (epoll underneath) reports "readable" persistently
/// while `read(2)` answers `EAGAIN`, so the ready future completes synchronously
/// forever, the loop never yields, and the current-thread runtime starves with the
/// control plane inside it (§15.18/§15.19). The tree's replacement is
/// [`poll_ready`]/[`poll_blocking`], and `meta_gates` greps the whole workspace to
/// keep it that way.
///
/// But that ban is a **claim about a kernel**, and a claim about a kernel is
/// exactly what may differ between the production target (6.18) and the dev box
/// (7.0). `serial-nexus-doctor` therefore measures it rather than remembering it: register
/// a pty master, ask epoll, then read, and report both answers as numbers. This is
/// the one sanctioned *call site class* for epoll in the tree — a measuring
/// instrument, never a readiness primitive, and nothing in `serial-nexus-daemon` may use
/// it. (The banned tokio identifier is not named in code here; the discussion of
/// the ban is prose, which is this tree's existing convention and which
/// `meta_gates::no_asyncfd_is_used_anywhere_in_the_workspace` skips by design.)
///
/// Registration is **level**-triggered on purpose — see
/// [`Epoll::add_level_readable`].
///
/// # What this instrument measured on the dev box, so a caller does not overclaim
///
/// Measured with these wrappers on Linux 7.0.0-28-generic (2026-07-26): a bare
/// level-triggered `EPOLLIN` registration on a pty master **agrees with
/// `poll(2)`** in the obvious shapes — idle with the slave open, epoll returns 0
/// events, `poll(2)` returns nothing, `read(2)` returns `EAGAIN`, `FIONREAD` 0;
/// five bytes queued, epoll returns `EPOLLIN`, `FIONREAD` 5; slave closed, epoll
/// returns `EPOLLHUP` and `read(2)` returns `EIO`.
///
/// That is not a refutation of invariant 1 and must not be reported as one: the
/// starvation §15.18 records is a property of tokio's readiness *guard* (its
/// registration lifecycle and its synchronously-completing ready future), not of
/// `epoll_ctl` in isolation. The consequence for a probe is concrete — **a probe
/// built on this must report the counts it saw, not degrade a box for failing to
/// reproduce a busy-loop.** Numbers that match the line above are the healthy
/// answer on 7.0, and the point of printing them is that 6.18 gets to answer for
/// itself.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct Epoll {
    fd: RawFd,
}

#[cfg(target_os = "linux")]
impl Epoll {
    /// Create an epoll set (`epoll_create1(EPOLL_CLOEXEC)`). `CLOEXEC` because the
    /// doctor spawns nothing, and an instrument should not be the reason a future
    /// child inherits a stray descriptor.
    pub fn new() -> std::io::Result<Self> {
        // Safety: `epoll_create1` takes no memory arguments; it returns a new fd or
        // -1 with errno set.
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Epoll { fd })
    }

    /// Register `fd` for readability, **level**-triggered (`EPOLLIN`, no
    /// `EPOLLET`), stamping the event's data word with `fd` so [`Epoll::wait`] can
    /// name it.
    ///
    /// Level-triggering is not a default that happened; it is the measurement.
    /// tokio's readiness guard registers level-triggered, so that is the mode
    /// invariant 1 is a statement about — an edge-triggered registration would
    /// answer a *different* question (it fires once per transition and so cannot
    /// exhibit the persistent-ready loop at all), and would quietly report "no
    /// problem here" on a kernel that still has one. There is deliberately no
    /// edge-triggered variant on this type.
    ///
    /// `EPOLLERR`/`EPOLLHUP` arrive whether or not they are requested, so a caller
    /// that registers this way still sees a hangup — worth reporting beside the
    /// readable bit rather than masking off.
    pub fn add_level_readable(&self, fd: RawFd) -> std::io::Result<()> {
        let mut ev = libc::epoll_event {
            events: EPOLLIN,
            u64: fd as u64,
        };
        // Safety: EPOLL_CTL_ADD reads one `epoll_event` through the pointer, which
        // lives across the call; `self.fd` is this type's own epoll fd and `fd` is
        // owned and kept alive by the caller.
        let rc = unsafe { libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, fd, &mut ev) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Wait up to `timeout_ms` for up to `max_events` registered fds to become
    /// ready, and return one [`EpollReady`] per event the kernel reported — so the
    /// caller gets both *how many* fired and *which flags* each carried.
    ///
    /// The timeout is a `u16` for the same reason [`poll_blocking`]'s is: it makes
    /// "block forever" unrepresentable. A diagnostic that can hang is a diagnostic
    /// nobody runs on the machine they were trying to diagnose.
    ///
    /// An empty vector means the timeout expired with nothing ready — which, on a
    /// pty master that `read(2)` also refuses, is the *good* answer and the one
    /// worth printing.
    pub fn wait(&self, timeout_ms: u16, max_events: usize) -> std::io::Result<Vec<EpollReady>> {
        let capacity = max_events.max(1);
        let mut events = vec![libc::epoll_event { events: 0, u64: 0 }; capacity];
        // Safety: `events` is valid for writes of `capacity` `epoll_event`s and
        // outlives the call; `epoll_wait` writes at most `capacity` of them and
        // returns how many (or -1 with errno set).
        let n = unsafe {
            libc::epoll_wait(
                self.fd,
                events.as_mut_ptr(),
                capacity as libc::c_int,
                i32::from(timeout_ms),
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(events[..n as usize]
            .iter()
            // `epoll_event` is `repr(packed)` on x86_64: these are field *reads*
            // (both fields are `Copy`), never references into the packed struct.
            .map(|e| EpollReady {
                fd: e.u64 as RawFd,
                events: e.events,
            })
            .collect())
    }
}

#[cfg(target_os = "linux")]
impl Drop for Epoll {
    fn drop(&mut self) {
        // Safety: `self.fd` was created by `Epoll::new`, is never handed out, and
        // this type is not `Clone`, so nothing else can be using or closing it.
        // The result is ignored deliberately: `close` can only fail here in ways a
        // probe cannot act on, and `Drop` has nowhere to report it.
        unsafe { libc::close(self.fd) };
    }
}

/// Non-Linux stub. `epoll(7)` is a Linux interface with no portable equivalent —
/// kqueue is a different mechanism, and the behaviour under measurement is
/// specifically epoll's — so every method reports `ENOTSUP`, exactly as
/// [`read_icounts`] does off Linux.
///
/// The contract for the caller is the same one `read_icounts` established: a
/// probe turns this error into **`skipped`**, never `unsupported`. `unsupported`
/// means a design premise was contradicted with no fallback; "this kernel has no
/// epoll" contradicts nothing, since the data plane is forbidden from using epoll
/// anyway (invariant 1) and macOS is best-effort (§13).
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct Epoll;

#[cfg(not(target_os = "linux"))]
impl Epoll {
    /// Always `ENOTSUP` off Linux — see the type's documentation.
    pub fn new() -> std::io::Result<Self> {
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    }

    /// Always `ENOTSUP` off Linux — see the type's documentation.
    pub fn add_level_readable(&self, _fd: RawFd) -> std::io::Result<()> {
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    }

    /// Always `ENOTSUP` off Linux — see the type's documentation.
    pub fn wait(&self, _timeout_ms: u16, _max_events: usize) -> std::io::Result<Vec<EpollReady>> {
        Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
    }
}

/// Bytes sitting in `fd`'s **input** queue right now (`FIONREAD`) — what a
/// `read(2)` could return without blocking.
///
/// A probe instrument, not a data-plane call: the data plane reads until
/// [`std::io::ErrorKind::WouldBlock`] and never needs to ask. It exists because
/// invariant 1's claim deserves a third witness. If epoll ever says *readable*
/// while [`read_fd`] answers `EAGAIN`, the two disagree and someone has to be
/// believed; `FIONREAD` is the kernel stating the queue depth outright, which
/// turns "do epoll and `read(2)` disagree about this pty master" from an
/// inference into a number an operator can read off two `--json` runs.
///
/// It is meaningful on a pty master, which is not obvious and was worth checking:
/// measured on Linux 7.0, a master with five bytes written from the slave reports
/// `FIONREAD == 5`, and `0` when idle.
///
/// `Err` means the fd's driver does not implement it. Callers omit the number
/// rather than faulting — the same treatment [`read_icounts`] gets (§5).
pub fn pending_input_bytes(fd: RawFd) -> nix::Result<usize> {
    let mut n: libc::c_int = 0;
    // Safety: FIONREAD writes one `int` through the pointer; `n` outlives the call.
    unsafe { fionread(fd, &mut n) }?;
    Ok(n.max(0) as usize)
}

/// Bytes still queued for transmission on `fd` (`TIOCOUTQ`) — the write-side twin
/// of [`pending_input_bytes`], for a probe that fills a buffer to discover its
/// depth (fill with the non-blocking [`write_fd`] until `WouldBlock`, then ask
/// where the bytes went).
///
/// **Report this number, do not judge it.** On a real UART it is the driver's
/// untransmitted count. On a pty it is 0: a pty has no transmitter to drain, and
/// the tty layer reports 0 when the line discipline offers no `chars_in_buffer`.
/// That was *measured*, not assumed — Linux 7.0 answers 0 on a pty master in all
/// three states (idle, five bytes queued, hung up). A probe that treated 0 as a
/// failure would redden a healthy box; whether a given kernel accounts for a pty
/// here at all is exactly the quiet 6.18-vs-7.0 difference this section exists to
/// surface, so emit it as an observation and never as a verdict. **6.18 has since
/// answered it and answered the same** — doctor P10 reads `pending_output_bytes:
/// 0` in both directions there (2026-07-27, `docs/serial-nexus-doctor.md`). The rule
/// stands unchanged for the next kernel: report the number, do not judge it.
///
/// Linux-only (libc exports the request code only there); elsewhere this is the
/// same `ENOTSUP` stub [`read_icounts`] uses, which a probe reports as `skipped`.
#[cfg(target_os = "linux")]
pub fn pending_output_bytes(fd: RawFd) -> nix::Result<usize> {
    let mut n: libc::c_int = 0;
    // Safety: TIOCOUTQ writes one `int` through the pointer; `n` outlives the call.
    unsafe { tiocoutq(fd, &mut n) }?;
    Ok(n.max(0) as usize)
}

/// Non-Linux stub for [`pending_output_bytes`]: no `TIOCOUTQ` request code is
/// exported off Linux, so report "unsupported" the way [`read_icounts`] does and
/// let the probe skip (§13 macOS best-effort).
#[cfg(not(target_os = "linux"))]
pub fn pending_output_bytes(_fd: RawFd) -> nix::Result<usize> {
    Err(nix::errno::Errno::ENOTSUP)
}

// ---------------------------------------------------------------------------
// POSIX ACL: granting one user read/write on a device node (§15.55)
// ---------------------------------------------------------------------------
//
// **Linux-only, and gated at the module rather than inside each function.** Two
// things here have no Darwin equivalent: `O_PATH`, which does not exist, and
// `setxattr`, whose Darwin signature carries an extra `position` argument. Neither
// is a portability gap worth papering over — the blessed helper this serves is
// itself Linux-only (§15.45, notes §3.66), because macOS re-enumerates through
// IOUSBLib unprivileged and needs no capability and no grant.
//
// The gate is here, on the block, rather than on the `pub fn` alone, because the
// constants and the encoder are only ever reached through it — and a `cfg` that
// covers less than the code it protects is the shape notes §3.71 records, where an
// insertion stole an attribute and the break was invisible to `cargo build`.

/// POSIX ACL entry tags (`linux/posix_acl_xattr.h`), in the order the kernel
/// requires them to appear.
#[cfg(target_os = "linux")]
const ACL_USER_OBJ: u16 = 0x01;
#[cfg(target_os = "linux")]
const ACL_USER: u16 = 0x02;
#[cfg(target_os = "linux")]
const ACL_GROUP_OBJ: u16 = 0x04;
#[cfg(target_os = "linux")]
const ACL_MASK: u16 = 0x10;
#[cfg(target_os = "linux")]
const ACL_OTHER: u16 = 0x20;
/// The id field of an entry that does not name one.
#[cfg(target_os = "linux")]
const ACL_UNDEFINED_ID: u32 = 0xffff_ffff;
/// `POSIX_ACL_XATTR_VERSION`.
#[cfg(target_os = "linux")]
const ACL_XATTR_VERSION: u32 = 2;

/// Encode the minimal POSIX access ACL for `mode` **plus** `u:<uid>:rw-`.
///
/// Pure, so the wire format is unit-testable without a privileged syscall — which
/// matters because getting it wrong is silent: the kernel rejects a malformed blob
/// with `EINVAL`, but a *well-formed* blob with the wrong permission bits would be
/// applied exactly as written.
///
/// The base entries are derived from the file's own mode rather than assumed, so
/// this never widens `other` — a node the operator has locked down stays locked
/// down apart from the one entry being added. The mask is the union of the named
/// entry and the group-object bits, which is what `setfacl -m u:<uid>:rw` computes.
///
/// **Idempotent by construction.** Once an ACL exists, a file's group bits *are*
/// its mask, so re-deriving from the mode on a second application yields the same
/// blob rather than drifting wider each time.
#[cfg(target_os = "linux")]
fn encode_posix_acl_rw(mode: u32, uid: u32) -> Vec<u8> {
    const RW: u16 = 6;
    let user_obj = ((mode >> 6) & 7) as u16;
    let group_obj = ((mode >> 3) & 7) as u16;
    let other = (mode & 7) as u16;
    let mask = group_obj | RW;

    let mut out = Vec::with_capacity(4 + 5 * 8);
    out.extend_from_slice(&ACL_XATTR_VERSION.to_le_bytes());
    for (tag, perm, id) in [
        (ACL_USER_OBJ, user_obj, ACL_UNDEFINED_ID),
        (ACL_USER, RW, uid),
        (ACL_GROUP_OBJ, group_obj, ACL_UNDEFINED_ID),
        (ACL_MASK, mask, ACL_UNDEFINED_ID),
        (ACL_OTHER, other, ACL_UNDEFINED_ID),
    ] {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&perm.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
    }
    out
}

/// Add `u:<uid>:rw-` to the POSIX access ACL of the **character device** at `path`.
///
/// The equivalent of `setfacl -m u:<uid>:rw <path>`, and the reason the replug
/// helper carries `CAP_FOWNER` (§15.55). Used to hand a test lane access to a tty a
/// USB re-enumeration has just recreated: udev builds the fresh node `root:dialout
/// 0660`, so any grant made before the cycle is gone with the inode that carried it.
///
/// # Two hazards, both closed here rather than in the caller
///
/// **The node is never opened.** Opening a serial port asserts DTR and can reset the
/// board behind it — the reason `serial-nexus-doctor` is passive until a port is
/// named (§15.17) — so a privileged helper must not open one merely to label it.
/// The reference is taken with `O_PATH`, which resolves the inode *without* calling
/// the driver's open at all, and the `setxattr` is aimed at it through
/// `/proc/self/fd/<n>`, the standard way to operate on an `O_PATH` reference.
///
/// **The path cannot be swapped underneath.** `O_NOFOLLOW` refuses a symlink
/// outright, and the type check runs on the **fd**, not on the name — so the thing
/// verified and the thing modified are the same inode, and there is no window
/// between them. A plain `lstat`-then-`setxattr(path)` would have one, and on a
/// binary holding `CAP_FOWNER` that window is the whole attack.
///
/// Refuses anything that is not a character device, because that is the only shape
/// this exists to grant and a capability-carrying binary should decline everything
/// it was not built for.
#[cfg(target_os = "linux")]
pub fn grant_user_rw(path: &std::path::Path, uid: u32) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("device path contains a NUL byte"))?;
    // Safety: `c_path` is a valid NUL-terminated string for the duration of the
    // call. O_PATH|O_NOFOLLOW resolves the inode without opening the driver and
    // without following a final symlink.
    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Closed on every path below.
    let guard = OwnedPathFd(fd);

    // Safety: `st` is written by the kernel; `fd` is a valid O_PATH descriptor, on
    // which `fstat` is explicitly permitted.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstat(guard.0, &mut st) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if st.st_mode & libc::S_IFMT != libc::S_IFCHR {
        return Err(std::io::Error::other(format!(
            "{} is not a character device (mode {:#o}) — this grant exists for tty \
             nodes and refuses everything else",
            path.display(),
            st.st_mode
        )));
    }

    let acl = encode_posix_acl_rw(st.st_mode & 0o7777, uid);
    let proc_path = std::ffi::CString::new(format!("/proc/self/fd/{}", guard.0))
        .expect("a decimal fd number contains no NUL");
    let name = c"system.posix_acl_access";
    // Safety: `proc_path` and `name` are valid NUL-terminated strings; `acl` is
    // valid for reads of its own length, which is what is passed.
    let rc = unsafe {
        libc::setxattr(
            proc_path.as_ptr(),
            name.as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Closes a raw fd on drop, so every early return above releases it.
#[cfg(target_os = "linux")]
struct OwnedPathFd(RawFd);
#[cfg(target_os = "linux")]
impl Drop for OwnedPathFd {
    fn drop(&mut self) {
        // Safety: we own this descriptor and close it exactly once.
        unsafe { libc::close(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constant and the arm this build compiled must agree, or P5 would
    /// explain a Linux measurement with a Darwin mechanism, or the reverse.
    #[test]
    fn the_icounts_constant_matches_the_arm_this_build_compiled() {
        assert_eq!(ICOUNTS_SUPPORTED, cfg!(target_os = "linux"));
        #[cfg(not(target_os = "linux"))]
        {
            // The stub never touches the fd, so an invalid one proves the arm:
            // what is asserted is that *no* fd can succeed here, which is the
            // claim P5's consequence line makes to the operator.
            assert!(matches!(read_icounts(-1), Err(nix::errno::Errno::ENOTSUP)));
        }
    }

    /// **The three-way rule, against injected readings** (design §7.1 clause 7,
    /// §15.53; §9).
    ///
    /// Every arm ships and at most one of them can be reached by hardware on any
    /// given box: the rig here is two FT232R adapters whose Linux driver honours
    /// `CRTSCTS`, Darwin's `IOSerialFamily` accepts-then-drops it on the same
    /// adapters, and **no driver in reach of this project has been measured
    /// refusing it outright** — so the refusal arm has no device that can produce
    /// it and a test driving the real predicate would exercise one third of this
    /// function and read as covered. The readings are therefore injected, which is
    /// the same shape `p15_verdict` and `open_failure_text` are split for.
    ///
    /// The fourth cell — set failed, flag reads back set — is not a contradiction
    /// and is asserted rather than left to chance: a port whose termios *already*
    /// carried `CRTSCTS` reads it back set whatever the set returned, and the flag
    /// being set on the port is what "honoured" means.
    #[test]
    fn the_three_crtscts_outcomes_are_separated_by_the_set_status_not_the_readback() {
        use FlowOutcome::{AcceptedThenDropped, Honoured, Refused};

        // The flag is set on the port: honoured, whatever the set reported.
        assert_eq!(FlowOutcome::classify(true, true), Honoured);
        assert_eq!(
            FlowOutcome::classify(false, true),
            Honoured,
            "a port that already carried CRTSCTS honours the mode; a set that failed \
             for an unrelated reason does not unset a flag that reads back set"
        );

        // Flag clear. Now — and only now — the set's own status is the discriminator,
        // and it is the whole of §7.1's three-way call.
        assert_eq!(
            FlowOutcome::classify(true, false),
            AcceptedThenDropped,
            "success plus a cleared read-back is the defect: the driver neither \
             honours the mode nor refuses it, so the link would run without the flow \
             control the config asked for"
        );
        assert_eq!(
            FlowOutcome::classify(false, false),
            Refused,
            "a failed tcsetattr is an HONEST refusal (§7.1 clause 7) and must not be \
             reported as the silent drop — collapsing the two is what made `load` \
             refuse a config the design does not refuse"
        );
    }

    /// **The predicate answers about the real port on this box**, and it puts the
    /// termios back — the half of [`honours_flow_control`] the pure test above cannot
    /// reach (the open, the two sets, the restore), measured on a pty because a pty
    /// is the one tty every box has (§9: the portable form, not the rig's form).
    ///
    /// Linux honours `CRTSCTS` on a pts, so the *answer* asserted here is this
    /// kernel's and is deliberately not the point; what is asserted is that some
    /// arm is reached without error and that the port is left as it was found. A
    /// probe that reconfigures a tty and cannot say it restored it is P15's own
    /// first-ranked `degraded` arm, and this function is what P15 calls last.
    #[cfg(target_os = "linux")]
    #[test]
    fn interrogating_a_real_tty_answers_and_restores_what_it_found() {
        use nix::sys::termios::tcgetattr;
        use std::os::unix::fs::OpenOptionsExt;
        let master =
            nix::pty::posix_openpt(nix::fcntl::OFlag::O_RDWR | nix::fcntl::OFlag::O_NOCTTY)
                .expect("posix_openpt");
        nix::pty::grantpt(&master).expect("grantpt");
        nix::pty::unlockpt(&master).expect("unlockpt");
        let pts = ptsname(&master).expect("ptsname");
        let path = std::path::Path::new(&pts);

        let slave = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)
            .expect("open the pts");
        let before = tcgetattr(&slave).expect("tcgetattr before");

        let answer =
            honours_flow_control(path, FlowMode::RtsCts).expect("a readable pts is measurable");
        assert_eq!(
            answer,
            FlowOutcome::Honoured,
            "a Linux pts honours CRTSCTS — an unexpected arm here means the \
             interrogation, not the kernel, changed"
        );

        let after = tcgetattr(&slave).expect("tcgetattr after");
        assert_eq!(
            before.control_flags, after.control_flags,
            "the predicate left the tty reconfigured: it must restore the termios it \
             found on every path, because the daemon and the doctor both call it on \
             ports an operator owns"
        );
    }

    /// **The software mode's arm on a real tty, and it is portable where the
    /// hardware one is not** (§15.61, plan §18 item 67).
    ///
    /// A pts honours `IXON|IXOFF` on both kernels of record — measured on Darwin
    /// 24.6.0 (`c_iflag` `0x2b02` → `0x2f02`, the delta being exactly `IXOFF` over an
    /// already-set `IXON`) and expected on Linux, where a pts is the same
    /// software-flow-capable tty. **That makes this a `Honoured` arm on a box whose
    /// real serial port is `AcceptedThenDropped`, which is the whole point**: the two
    /// live side by side on the Darwin rig box, so the predicate is shown to
    /// *discriminate* rather than to refuse everything. A refusal rule proven only
    /// against ports it refuses is not proven.
    ///
    /// If this ever reads something else on Linux, the reading is new information
    /// about that kernel and belongs in the record — not a reason to relax the
    /// assertion.
    ///
    /// The restore check reads **both** flag words, which is item 14's recorded
    /// lesson: a check reading `c_cflag` alone would certify a port left with `IXON`
    /// asserted, and this predicate is the one thing that writes to an operator's
    /// port on every `load`.
    #[test]
    fn the_software_arm_answers_on_a_real_tty_and_restores_both_flag_words() {
        use nix::sys::termios::tcgetattr;
        use std::os::unix::fs::OpenOptionsExt;
        let master =
            nix::pty::posix_openpt(nix::fcntl::OFlag::O_RDWR | nix::fcntl::OFlag::O_NOCTTY)
                .expect("posix_openpt");
        nix::pty::grantpt(&master).expect("grantpt");
        nix::pty::unlockpt(&master).expect("unlockpt");
        let pts = ptsname(&master).expect("ptsname");
        let path = std::path::Path::new(&pts);

        let slave = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(path)
            .expect("open the pts");
        let before = tcgetattr(&slave).expect("tcgetattr before");

        let answer =
            honours_flow_control(path, FlowMode::XonXoff).expect("a readable pts is measurable");
        assert_eq!(
            answer,
            FlowOutcome::Honoured,
            "a pts honoured IXON|IXOFF on both kernels of record when this was \
             written; a different arm here is a kernel reading worth recording, not \
             an assertion to loosen"
        );

        let after = tcgetattr(&slave).expect("tcgetattr after");
        assert_eq!(
            (before.input_flags, before.control_flags),
            (after.input_flags, after.control_flags),
            "the predicate left the tty reconfigured. Both words are compared \
             deliberately: the software arm writes `c_iflag` and clears CRTSCTS in \
             `c_cflag`, so a restore check reading either alone would certify a port \
             this call had left changed"
        );
    }

    /// A pty pair built the way `PtyNode::setup` builds one — baseline termios
    /// through a momentary slave open, packet mode on the master, slave primed
    /// once, master non-blocking — so [`SessionLatch`] is measured against the
    /// pair it actually meets. Priming is load-bearing: a master whose slave has
    /// *never* been opened does not hang up, so a test that skips it measures a
    /// quiescent fd and passes whatever the latch does.
    #[cfg(target_os = "macos")]
    fn latch_pair() -> (nix::pty::PtyMaster, String) {
        use nix::fcntl::OFlag;
        use nix::pty::{grantpt, posix_openpt, unlockpt};
        use std::os::fd::AsRawFd;

        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let pts = ptsname(&master).expect("ptsname");
        {
            // Baseline through the slave (the BSD arm of §7.2), then prime.
            let slave = open_slave(&pts);
            let mut t = nix::sys::termios::tcgetattr(&slave).expect("tcgetattr");
            nix::sys::termios::cfmakeraw(&mut t);
            nix::sys::termios::tcsetattr(&slave, nix::sys::termios::SetArg::TCSANOW, &t)
                .expect("tcsetattr");
        }
        set_packet_mode(master.as_raw_fd(), true).expect("TIOCPKT");
        drop(open_slave(&pts)); // prime
        set_nonblocking(master.as_raw_fd()).expect("nonblocking");
        (master, pts)
    }

    /// Open the slave the way a client does — never adopting it as this process's
    /// controlling terminal.
    #[cfg(target_os = "macos")]
    fn open_slave(pts: &str) -> std::fs::File {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(pts)
            .unwrap_or_else(|e| panic!("open pty slave {pts}: {e}"))
    }

    /// One collapsed *termios-only* session: open, `tcsetattr`, close, with no
    /// data byte anywhere — the shape that leaves nothing on a Darwin master and
    /// so leaked its write lock before the latch existed.
    #[cfg(target_os = "macos")]
    fn termios_only_session(pts: &str) {
        let slave = open_slave(pts);
        let mut t = nix::sys::termios::tcgetattr(&slave).expect("tcgetattr");
        t.local_flags.toggle(nix::sys::termios::LocalFlags::ECHO);
        nix::sys::termios::tcsetattr(&slave, nix::sys::termios::SetArg::TCSANOW, &t)
            .expect("tcsetattr");
    }

    /// The property the whole type exists for: a session carrying **no data byte**
    /// is still visible afterwards, on a kernel where no level state records it.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_collapsed_termios_only_session_posts_a_session_edge() {
        use std::os::fd::AsRawFd;
        let (master, pts) = latch_pair();
        let latch = SessionLatch::watch(master.as_raw_fd()).expect("watch the master");

        for trial in 0..5 {
            termios_only_session(&pts);
            assert!(
                latch.took_edge(),
                "trial {trial}: a collapsed termios-only session left no edge — \
                 `nodes/pty.rs`'s last-close handler has nothing to fire on, so an \
                 on-demand holder that attached and left inside one poll gap keeps \
                 its write lock (§6 detach-release, invariant 16)"
            );
        }
    }

    /// Rule 1 of the type's contract, and the anti-spin property: once consumed,
    /// an idle hung-up master posts nothing. Without this the last-close handler
    /// re-fires every pass — the 99%-CPU shape `b8d8ed8` records — and, worse, it
    /// would release a write lock an operator took with *no client attached*.
    ///
    /// Shaped like the reader's own pass (poll, then read, then ask) because that
    /// is the sequence whose side effects could re-arm the knote.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_idle_hung_up_master_posts_no_edge_across_many_passes() {
        use std::os::fd::AsRawFd;
        let (master, _pts) = latch_pair();
        let fd = master.as_raw_fd();
        let latch = SessionLatch::watch(fd).expect("watch the master");

        let mut buf = [0u8; 256];
        let mut spurious = 0;
        for _ in 0..200 {
            let _ = poll_ready(fd, nix::poll::PollFlags::POLLIN);
            let _ = read_fd(fd, &mut buf);
            if latch.took_edge() {
                spurious += 1;
            }
        }
        assert_eq!(
            spurious, 0,
            "an idle master posted {spurious} phantom session edges in 200 passes; \
             each one re-fires the last-close handler and releases a lock nobody's \
             client took"
        );
    }

    /// Rule 2, the forge site closest to hand: registering on a master that is
    /// *already* hung up posts an immediate `EV_EOF`, and every pty node starts
    /// exactly there because `setup` primes the slave and closes it. `watch` must
    /// swallow it, or the node's first pass invents a session that never happened.
    #[cfg(target_os = "macos")]
    #[test]
    fn watch_swallows_the_edge_its_own_registration_posts() {
        use std::os::fd::AsRawFd;
        let (master, _pts) = latch_pair(); // leaves the pair primed and hung up
        let latch = SessionLatch::watch(master.as_raw_fd()).expect("watch the master");
        assert!(
            !latch.took_edge(),
            "registering on an already-hung-up master handed its own EV_EOF to the \
             caller — the pty reader would take it for a client session on its very \
             first pass (SessionLatch rule 2)"
        );
    }

    /// The daemon opens the slave itself — §7.2's baseline re-assert, the
    /// last-close hostward flush, the reconciliation backstop — and each of those
    /// forges an edge indistinguishable from a client's. This pins that they *do*
    /// forge one, so the caller's obligation to [`SessionLatch::discard`] after
    /// such a block is a measured requirement rather than defensive habit.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_daemon_side_momentary_slave_open_forges_an_edge_that_discard_clears() {
        use std::os::fd::AsRawFd;
        let (master, pts) = latch_pair();
        let latch = SessionLatch::watch(master.as_raw_fd()).expect("watch the master");

        drop(open_slave(&pts)); // what `apply_baseline` / `flush_hostward_queue` do
        assert!(
            latch.took_edge(),
            "a momentary slave open posted no edge, so this test can no longer \
             prove why `discard` is required after the last-close block"
        );
        // And `discard` really clears it, which is the half the caller depends on.
        drop(open_slave(&pts));
        latch.discard();
        assert!(
            !latch.took_edge(),
            "`discard` left an edge behind — the last-close handler's own slave \
             opens would re-fire it on the next pass"
        );
    }

    /// The inert arm is still a *type* the daemon compiles against, so keep one
    /// assertion on it: it must answer "no session" rather than panicking or
    /// failing to construct. On Linux that is the correct answer — the retained
    /// `TIOCPKT_IOCTL` packet is the mechanism there (`serial-nexus-doctor` P7
    /// `latch_covers_termios_only_session: true` on both 6.18 and 7.0).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_inert_latch_constructs_and_reports_no_session() {
        use nix::fcntl::OFlag;
        use nix::pty::posix_openpt;
        use std::os::fd::AsRawFd;

        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        let latch = SessionLatch::watch(master.as_raw_fd()).expect("watch is infallible here");
        assert!(!latch.took_edge());
        latch.discard();
        assert!(!latch.took_edge());
    }

    /// Two masters resolved concurrently must each keep their *own* slave path
    /// (§7.2: the path becomes a `pty` node's stable symlink, so a swap is a
    /// silent misroute, not a crash). On Linux this rides the reentrant
    /// `ptsname_r` arm; off Linux it is the regression guard for `PTSNAME_LOCK`,
    /// the static-buffer precondition documented on [`ptsname`] (review 26,
    /// SYS-1). Portable either way, which is the point.
    #[test]
    fn concurrent_ptsname_keeps_each_master_s_own_path() {
        use nix::fcntl::OFlag;
        use nix::pty::posix_openpt;

        let a = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("open pty master a");
        let b = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("open pty master b");
        let want_a = ptsname(&a).expect("resolve a");
        let want_b = ptsname(&b).expect("resolve b");
        assert_ne!(want_a, want_b, "two masters must have distinct slaves");

        std::thread::scope(|s| {
            for (master, want) in [(&a, &want_a), (&b, &want_b)] {
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(&ptsname(master).expect("resolve under contention"), want);
                    }
                });
            }
        });
    }

    /// The `EPOLL*` constants are written as literals so [`EpollReady`] compiles
    /// on macOS (where libc exports none of them). That convenience is only
    /// allowed to exist if it is *proven* equal to the ABI rather than trusted, so
    /// this checks each one against libc on the platform that has them. A silently
    /// wrong bit would make a probe report the wrong kernel behaviour, which is
    /// worse than no probe.
    #[test]
    #[cfg(target_os = "linux")]
    fn epoll_flag_constants_match_the_kernel_abi() {
        for (ours, theirs, name) in [
            (EPOLLIN, libc::EPOLLIN, "EPOLLIN"),
            (EPOLLPRI, libc::EPOLLPRI, "EPOLLPRI"),
            (EPOLLOUT, libc::EPOLLOUT, "EPOLLOUT"),
            (EPOLLERR, libc::EPOLLERR, "EPOLLERR"),
            (EPOLLHUP, libc::EPOLLHUP, "EPOLLHUP"),
            (EPOLLRDHUP, libc::EPOLLRDHUP, "EPOLLRDHUP"),
        ] {
            assert_eq!(ours, theirs as u32, "{name} does not match libc");
        }
    }

    /// The epoll wrapper mechanically works: a registered fd with a byte waiting
    /// comes back once, identified by fd, carrying `EPOLLIN`, with `FIONREAD`
    /// agreeing on the depth.
    ///
    /// Note what this deliberately does **not** assert: the pty-master behaviour
    /// behind invariant 1 (epoll ready while `read` says `EAGAIN`). That is the
    /// thing `serial-nexus-doctor` *measures* and reports, and a kernel that fixed it
    /// must not turn this crate's test suite red — a probe reports what it
    /// observed, a unit test pins what we implement. A socketpair is used because
    /// its readiness is unambiguous on every Linux and needs no pty.
    #[test]
    #[cfg(target_os = "linux")]
    fn epoll_reports_a_readable_fd_and_names_the_flag() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (mut tx, rx) = UnixStream::pair().expect("socketpair");
        tx.write_all(b"x").expect("write one byte");

        let ep = Epoll::new().expect("epoll_create1");
        ep.add_level_readable(rx.as_raw_fd())
            .expect("EPOLL_CTL_ADD");
        let ready = ep.wait(500, 4).expect("epoll_wait");

        assert_eq!(ready.len(), 1, "one registered fd, one event");
        assert_eq!(ready[0].fd, rx.as_raw_fd(), "the event names its own fd");
        assert!(ready[0].contains(EPOLLIN), "events={:#x}", ready[0].events);
        assert!(ready[0].flag_names().contains(&"EPOLLIN"));
        assert_eq!(
            pending_input_bytes(rx.as_raw_fd()).expect("FIONREAD"),
            1,
            "FIONREAD must agree with the byte actually queued"
        );
    }

    /// An idle registered fd yields an empty vector rather than an error — the
    /// answer a probe prints when epoll and `read(2)` finally agree. Also pins the
    /// `u16` timeout: this call must return, and it must return quickly.
    #[test]
    #[cfg(target_os = "linux")]
    fn epoll_wait_returns_empty_on_timeout() {
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (_tx, rx) = UnixStream::pair().expect("socketpair");
        let ep = Epoll::new().expect("epoll_create1");
        ep.add_level_readable(rx.as_raw_fd())
            .expect("EPOLL_CTL_ADD");

        let start = std::time::Instant::now();
        let ready = ep.wait(20, 4).expect("epoll_wait");
        assert!(ready.is_empty(), "nothing was written: {ready:?}");
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    /// `Drop` really closes the epoll fd. A leak here would be invisible in a
    /// single probe run and fatal in a probe that samples in a loop, so the test
    /// creates far more sets than a typical 1024-fd soft limit allows: with a leak
    /// this hits EMFILE long before the end.
    #[test]
    #[cfg(target_os = "linux")]
    fn dropping_an_epoll_set_closes_its_fd() {
        for i in 0..4096 {
            Epoll::new().unwrap_or_else(|e| panic!("epoll_create1 failed at iteration {i}: {e}"));
        }
    }

    /// `FIONREAD` reports the queue depth (and zero on an idle fd) on every
    /// platform this crate builds for — it is the one probe instrument here that
    /// is not Linux-only.
    #[test]
    fn pending_input_bytes_counts_what_is_queued() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (mut tx, rx) = UnixStream::pair().expect("socketpair");
        assert_eq!(pending_input_bytes(rx.as_raw_fd()).expect("FIONREAD"), 0);
        tx.write_all(b"hello").expect("write");
        // The write is to a socketpair buffer, so it is readable on return.
        assert_eq!(pending_input_bytes(rx.as_raw_fd()).expect("FIONREAD"), 5);
    }

    /// [`set_cloexec`] actually sets the flag the exec-inheritance guard rests on.
    /// A fd is *not* close-on-exec by default, so the before-assertion is what makes
    /// the after-assertion mean something (37-PTY-1).
    #[test]
    fn set_cloexec_marks_a_fd_close_on_exec() {
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (tx, _rx) = UnixStream::pair().expect("socketpair");
        // `dup(2)` is specified to clear FD_CLOEXEC on the new descriptor, which is
        // the only reliable way to get an inheritable fd here: std marks everything
        // it opens close-on-exec, so duplicating is what reproduces the state a raw
        // `posix_openpt` leaves behind.
        // Safety: `tx` is open for the whole test; the returned fd is closed below.
        let fd = unsafe { libc::dup(tx.as_raw_fd()) };
        assert!(fd >= 0, "dup: {}", std::io::Error::last_os_error());

        // Safety: F_GETFD takes no argument and reads no memory; `fd` is open.
        let before = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_eq!(
            before & libc::FD_CLOEXEC,
            0,
            "a dup'd fd is already close-on-exec, so this test proves nothing"
        );

        set_cloexec(fd).expect("set_cloexec");

        // Safety: as above.
        let after = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        // Safety: `fd` came from `dup` above and is not owned by anything else.
        unsafe { libc::close(fd) };
        assert_ne!(
            after & libc::FD_CLOEXEC,
            0,
            "set_cloexec left FD_CLOEXEC clear; every exec-codec child would still \
             inherit the fd (§7.2, §7.6)"
        );
    }

    /// The ACL blob is checked field by field, because the failure mode it guards is
    /// **silent**: a malformed blob is refused with `EINVAL` and noticed, while a
    /// well-formed one carrying the wrong permission bits is applied exactly as
    /// written and grants whatever it says (§15.55).
    #[test]
    #[cfg(target_os = "linux")]
    fn the_acl_blob_grants_the_named_user_and_nothing_else() {
        // A udev-fresh tty node: root:dialout 0660.
        let blob = encode_posix_acl_rw(0o660, 1000);
        assert_eq!(blob.len(), 4 + 5 * 8, "version word plus five entries");
        assert_eq!(&blob[0..4], &2u32.to_le_bytes(), "POSIX_ACL_XATTR_VERSION");

        let entry = |i: usize| -> (u16, u16, u32) {
            let b = &blob[4 + i * 8..4 + i * 8 + 8];
            (
                u16::from_le_bytes([b[0], b[1]]),
                u16::from_le_bytes([b[2], b[3]]),
                u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            )
        };
        // Tag order is not cosmetic: the kernel requires ascending tags.
        assert_eq!(entry(0), (ACL_USER_OBJ, 6, ACL_UNDEFINED_ID));
        assert_eq!(entry(1), (ACL_USER, 6, 1000), "the one granted user, rw");
        assert_eq!(entry(2), (ACL_GROUP_OBJ, 6, ACL_UNDEFINED_ID));
        assert_eq!(entry(3), (ACL_MASK, 6, ACL_UNDEFINED_ID));
        assert_eq!(
            entry(4),
            (ACL_OTHER, 0, ACL_UNDEFINED_ID),
            "`other` must stay exactly what the mode said — a grant that opened a \
             device node to the world would be the whole of this feature's risk"
        );
    }

    /// The base entries come from the file's own mode, so a node the operator has
    /// locked down further is not quietly widened back to the common case.
    #[test]
    #[cfg(target_os = "linux")]
    fn the_acl_blob_never_widens_the_mode_it_found() {
        // 0600: owner rw, group nothing, other nothing.
        let blob = encode_posix_acl_rw(0o600, 4242);
        let entry = |i: usize| -> (u16, u16, u32) {
            let b = &blob[4 + i * 8..4 + i * 8 + 8];
            (
                u16::from_le_bytes([b[0], b[1]]),
                u16::from_le_bytes([b[2], b[3]]),
                u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            )
        };
        assert_eq!(
            entry(2),
            (ACL_GROUP_OBJ, 0, ACL_UNDEFINED_ID),
            "group stays 0"
        );
        assert_eq!(entry(4), (ACL_OTHER, 0, ACL_UNDEFINED_ID), "other stays 0");
        // The mask must still admit the named entry, or the grant would be inert.
        assert_eq!(entry(3), (ACL_MASK, 6, ACL_UNDEFINED_ID));
        assert_eq!(entry(1), (ACL_USER, 6, 4242));
    }

    /// Re-applying must not drift: once an ACL exists the group bits *are* the mask,
    /// so the second derivation has to reproduce the first.
    #[test]
    #[cfg(target_os = "linux")]
    fn the_acl_blob_is_idempotent_under_reapplication() {
        let first = encode_posix_acl_rw(0o660, 1000);
        // After the first application the node reads 0660 still (mask 6 in the group
        // bits); a second pass must produce the identical blob.
        let second = encode_posix_acl_rw(0o660, 1000);
        assert_eq!(first, second);
    }

    /// It refuses anything that is not a character device — proved against a regular
    /// file, which is the shape a swapped path would most likely have.
    #[test]
    #[cfg(target_os = "linux")]
    fn granting_on_a_regular_file_is_refused() {
        let tmp = std::env::temp_dir().join(format!("snx-acl-{}", std::process::id()));
        std::fs::write(&tmp, b"not a tty").expect("write the fixture");
        let err = grant_user_rw(&tmp, 1000).expect_err("a regular file must be refused");
        assert!(
            err.to_string().contains("not a character device"),
            "the refusal must say what it refused and why: {err}"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    /// **[`process_cpu_nanos`] moves when a process burns CPU and not otherwise**
    /// (§16.5; plan §18 items 12 and 13).
    ///
    /// The burn is sized to dwarf both kernels' resolution — Linux's tick is 10 ms —
    /// so this is not a timing assertion, only a claim that the counter is wired to
    /// the right field and rises with real work.
    ///
    /// **The obvious second arm is deliberately absent, and the reason is a defect
    /// this test shipped with for one CI run.** It slept 200 ms and asserted the
    /// counter barely moved — "a sleep costs wall-clock, not CPU" — which passed on a
    /// quiet developer box and **failed on CI's macOS runner**. The assertion is
    /// unsound by construction here: `process_cpu_nanos` measures the **process**, and
    /// `cargo test` runs this unit beside dozens of others in that same process, so a
    /// neighbour's work lands inside this test's "idle" window. It was measuring the
    /// harness, not the counter.
    ///
    /// The property is real and is asserted where it can be — against a **dedicated
    /// child** that is genuinely idle: `p9_pty_collapse`'s post-hangup daemon and
    /// `p12_sim_idle_cpu`'s peerless doubles. Sampling a child is what every caller
    /// that matters does; this unit covers the one arm a box can run without spawning
    /// anything, and claims nothing more.
    #[test]
    fn process_cpu_nanos_rises_with_work() {
        let pid = std::process::id();
        let t0 = process_cpu_nanos(pid).expect("this process's own CPU time is readable");

        let mut acc = 0u64;
        for i in 0..60_000_000u64 {
            acc = acc.wrapping_add(i ^ acc);
        }
        assert_ne!(acc, 1, "keep the loop from being optimised away");
        let t1 = process_cpu_nanos(pid).expect("still readable");
        assert!(
            t1 > t0 + 10_000_000,
            "burning CPU moved the counter by {} ns, which is under one Linux clock \
             tick — the field being read is not the process's CPU time",
            t1 - t0
        );
    }

    /// **[`peer_hungup`] on a real pts pair, both arms, each the other's control**
    /// (§16.5: a shared helper is self-tested; plan §18 item 66).
    ///
    /// The negative arm is not decoration. A helper that answered `true`
    /// unconditionally would satisfy the positive arm alone, and a helper that
    /// answered `false` unconditionally would satisfy the negative one — so neither
    /// is evidence without the other, which is the shape P16 uses for the same
    /// question in the doctor.
    ///
    /// **This test is portable in form and bites on Darwin in particular.** Both
    /// assertions hold on Linux whatever mask the helper requests, because Linux
    /// reports `POLLHUP` unrequested; on Darwin the positive arm fails outright if
    /// the mask stops naming `POLLHUP`. That asymmetry is the point rather than a
    /// weakness — it is the regression this helper exists to make impossible, and it
    /// is stated here so a future reader does not "simplify" the mask on the platform
    /// where doing so is free.
    #[test]
    fn peer_hungup_is_absent_while_the_master_is_open_and_present_after_it_closes() {
        use nix::fcntl::{OFlag, open};
        use nix::pty::{grantpt, posix_openpt, unlockpt};
        use nix::sys::stat::Mode;

        let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let pts = ptsname(&master).expect("ptsname");
        // Blocking, as `itest`'s `attach_slave` opens it — the fd shape the witness
        // this helper serves actually holds.
        let slave = std::fs::File::from(
            open(pts.as_str(), OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("open pts"),
        );

        assert!(
            !peer_hungup(&slave),
            "a slave whose master is still open reported a hangup — the negative arm \
             is what makes the positive one mean anything"
        );

        drop(master);
        // The hangup is delivered in single-digit microseconds on both kernels of
        // record (P16: 1 µs on Linux 7.0.0-29, 6–16 µs on Darwin 24.6.0), so a short
        // bounded wait is generous rather than tuned; it exists so a loaded box
        // reports the real answer instead of a scheduling artifact.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !peer_hungup(&slave) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            peer_hungup(&slave),
            "the master closed and the held slave never reported POLLHUP. On Darwin \
             the first thing to check is the requested mask: this kernel gates the \
             hangup on it (P9's `hangup_delivered_to_a_mask_that_requested_nothing` \
             reads false there), so an empty mask makes this helper silently useless"
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod acl_kernel_probe {
    /// The kernel's own opinion of the blob, without privilege: `/dev/null` is a
    /// character device this process does not own, so a **well-formed** ACL is
    /// refused `EPERM` (missing `CAP_FOWNER`) while a malformed one is refused
    /// `EINVAL`. Distinguishing those two is the only unprivileged way to prove the
    /// wire format is right, and it is worth proving: an `EINVAL` here would mean
    /// the blessed binary's grant silently never worked.
    #[test]
    fn the_blob_is_well_formed_enough_for_the_kernel_to_reach_the_permission_check() {
        let err = super::grant_user_rw(std::path::Path::new("/dev/null"), 1000)
            .expect_err("an unprivileged process cannot set an ACL on a root-owned node");
        assert_eq!(
            err.raw_os_error(),
            Some(libc::EPERM),
            "expected EPERM (well-formed, unprivileged); EINVAL would mean the ACL \
             encoding itself is wrong and the grant would never work even blessed: {err}"
        );
    }
}
