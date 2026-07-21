//! Raw ioctls nix/serial2 don't wrap, isolated behind a localized unsafe
//! allowance — the same discipline the daemon's `sys` module uses (plan §2).

#![allow(unsafe_code)]

use nix::libc;
use std::os::fd::RawFd;

nix::ioctl_write_ptr_bad!(tiocpkt, libc::TIOCPKT, libc::c_int);
nix::ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
nix::ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);

/// Packet-mode control-byte flag for a termios/ioctl change on the slave.
/// Stable Linux value; not exported by libc (only the `TIOCPKT` request code
/// is).
pub const TIOCPKT_IOCTL: u8 = 64;

/// Enable or disable packet mode on a pty master.
pub fn set_packet_mode(fd: RawFd, on: bool) -> nix::Result<()> {
    let v: libc::c_int = i32::from(on);
    // Safety: `v` outlives the call; TIOCPKT reads one int through the pointer.
    unsafe { tiocpkt(fd, &v) }?;
    Ok(())
}

/// Take or release exclusive access on a tty (`TIOCEXCL`/`TIOCNXCL`).
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

/// Linux serial error/edge counters (`TIOCGICOUNT`). Not defined by libc, so it
/// is declared here (§5, §7.1: overrun/framing counters surfaced in state).
#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct SerialIcounter {
    pub cts: libc::c_int,
    pub dsr: libc::c_int,
    pub rng: libc::c_int,
    pub dcd: libc::c_int,
    pub rx: libc::c_int,
    pub tx: libc::c_int,
    pub frame: libc::c_int,
    pub overrun: libc::c_int,
    pub parity: libc::c_int,
    pub brk: libc::c_int,
    pub buf_overrun: libc::c_int,
    pub reserved: [libc::c_int; 9],
}

// `TIOCGICOUNT` is Linux-only (libc exports the request code only there); gate the
// binding and read to Linux. Elsewhere P3/P5 already treat `Err` as "not a UART /
// unsupported" (§13 macOS best-effort), so a stub keeps the doctor building and its
// verdicts honest.
#[cfg(target_os = "linux")]
nix::ioctl_read_bad!(tiocgicount_raw, libc::TIOCGICOUNT, SerialIcounter);

/// Read the driver's serial counters, where supported.
#[cfg(target_os = "linux")]
pub fn read_icounter(fd: RawFd) -> nix::Result<SerialIcounter> {
    let mut c = SerialIcounter::default();
    // Safety: writes a fixed-size struct we own through the pointer.
    unsafe { tiocgicount_raw(fd, &mut c) }?;
    Ok(c)
}

/// Non-Linux stub: no `TIOCGICOUNT`, so report "unsupported" — the not-a-UART path.
#[cfg(not(target_os = "linux"))]
pub fn read_icounter(_fd: RawFd) -> nix::Result<SerialIcounter> {
    Err(nix::errno::Errno::ENOTSUP)
}

/// Resolve a pty master's slave path across platforms: reentrant `ptsname_r(3)` on
/// Linux/Android, the static-buffer `ptsname(3)` (nix marks it `unsafe`) elsewhere.
/// The returned `String` is copied out before this returns.
pub fn ptsname(master: &nix::pty::PtyMaster) -> nix::Result<String> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        nix::pty::ptsname_r(master)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        // Safety: single-threaded probe; the result is cloned out of the static
        // buffer immediately.
        unsafe { nix::pty::ptsname(master) }
    }
}
