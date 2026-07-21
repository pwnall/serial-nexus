//! Raw ioctls nix/serial2 don't wrap, isolated behind a localized unsafe
//! allowance — the single `unsafe`-bearing module in the daemon (plan §2).

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

/// Resolve a pty master's slave path. Linux/Android have the reentrant
/// `ptsname_r(3)` (a glibc extension nix exposes safely); elsewhere only the
/// static-buffer `ptsname(3)` exists, which nix marks `unsafe`. This wrapper hides
/// the split so the PTY node code is platform-agnostic. On the non-reentrant path
/// the returned `String` is copied out of the static buffer before this returns,
/// so no dangling reference escapes.
pub fn ptsname(master: &nix::pty::PtyMaster) -> nix::Result<String> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        nix::pty::ptsname_r(master)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        // Safety: single-threaded daemon; the `String` is cloned out of the
        // static buffer immediately, so a later `ptsname` call cannot corrupt it.
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
