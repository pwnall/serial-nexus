//! `nexus-sys` — the one crate that carries `unsafe` (design §16.3).
//!
//! Every raw ioctl, `ptsname`, and `poll(2)` wrapper the daemon, the doctor, and
//! the sim need lives here — the wrappers previously existed three times (daemon,
//! doctor, sim), and the macOS port had to gate `TIOCGICOUNT`/`ptsname` per copy,
//! so a missed copy was a silent platform break. Collecting them into one internal
//! crate shrinks the audit surface for `unsafe` to a single file set with the
//! cfg-gating written once (§16.3); every other crate `#![forbid(unsafe_code)]`.

#![allow(unsafe_code)]

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
/// copy-out run under [`PTSNAME_LOCK`], so concurrent `nexus_sys::ptsname`
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
/// [`poll_ready`]). `posix_openpt` and `serial2::SerialPort::open` both leave the
/// fd blocking, so the data plane sets this itself.
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

// ---------------------------------------------------------------------------
// Capability-probe helpers (§15.17) — instruments, not data-plane primitives
//
// Everything from here down exists for `nexus-doctor`: it *measures* a kernel
// behaviour so the report can print the number that was observed. None of it is a
// readiness or I/O primitive for `nexus-daemon` — the data plane's readiness is
// `poll_ready`/`poll_blocking` above and nothing else (invariant 1, §15.18), and a
// helper in this section must never become the answer to "how do I wait for this
// fd" in a node.
//
// They live in this crate for the same reason the rest of the file does:
// `nexus-doctor` is `#![forbid(unsafe_code)]`, so a raw syscall it needs is a safe
// wrapper here or it does not exist (§16.3, invariant 4).
//
// Why measure at all: the production kernel is Linux 6.18 and the dev box is 7.0
// (§7), and several design premises here rest on behaviour confirmed only on 7.0.
// A premise like that has to be *re-observed* on 6.18, and "re-observed" means a
// JSON number an operator can diff between two `nexus-doctor --json` runs — not a
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
/// (7.0). `nexus-doctor` therefore measures it rather than remembering it: register
/// a pty master, ask epoll, then read, and report both answers as numbers. This is
/// the one sanctioned *call site class* for epoll in the tree — a measuring
/// instrument, never a readiness primitive, and nothing in `nexus-daemon` may use
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
/// surface, so emit it as an observation and never as a verdict.
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

#[cfg(test)]
mod tests {
    use super::*;

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
    /// thing `nexus-doctor` *measures* and reports, and a kernel that fixed it
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
}
