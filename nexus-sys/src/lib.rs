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
/// `nexus-doctor` P7 reports as `latch_covers_termios_only_session: true` on both
/// kernels of record (6.18 and 7.0) — so there the latch is a no-op that costs one
/// never-taken branch per poll. Other BSDs have `kqueue` and would very likely
/// behave as Darwin does, but nobody has measured them, and a latch that silently
/// claimed to work there would be exactly the unmeasured claim §15.17 exists to
/// prevent. `nexus-doctor`'s P12 reports what this mechanism does on whatever
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
/// surface, so emit it as an observation and never as a verdict. **6.18 has since
/// answered it and answered the same** — doctor P10 reads `pending_output_bytes:
/// 0` in both directions there (2026-07-27, `docs/nexus-doctor.md`). The rule
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

#[cfg(test)]
mod tests {
    use super::*;

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
    /// `TIOCPKT_IOCTL` packet is the mechanism there (`nexus-doctor` P7
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
