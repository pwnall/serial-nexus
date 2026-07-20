//! Raw ioctls nix/serial2 don't wrap, isolated behind a localized unsafe
//! allowance — the single `unsafe`-bearing module in the daemon (plan §2).

#![allow(unsafe_code)]

use nix::libc;
use std::os::fd::RawFd;

nix::ioctl_write_ptr_bad!(tiocpkt, libc::TIOCPKT, libc::c_int);
nix::ioctl_none_bad!(tiocexcl, libc::TIOCEXCL);
nix::ioctl_none_bad!(tiocnxcl, libc::TIOCNXCL);

/// Packet-mode control-byte flag for a termios/ioctl change on the slave
/// (stable Linux value; libc exports only the `TIOCPKT` request code). Consumed
/// by presence/termios observation in slice 2.
#[allow(dead_code)]
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
