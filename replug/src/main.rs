//! `serial-nexus-replug` — the platform dispatcher.
//!
//! The helper's substance is in [`linux`], and it stays Linux-only for now. What
//! this file exists for is the *build*: until 2026-08-05 the crate was an
//! unconditional workspace member whose first `use` reached
//! `serial_nexus_sys::caps`, which is `#[cfg(target_os = "linux")]`. So
//! `cargo build --workspace --locked` — the **first** step of the macOS CI lane, and
//! the first thing anyone runs on a Mac — failed outright, and every check behind it
//! reported nothing. `sys` already said the helper "does not build elsewhere"; the
//! decision was written down and never enforced (notes §3.65 A).
//!
//! # Why this is a dispatcher and not a `#[cfg]` sprinkled over the Linux code
//!
//! A macOS equivalent of the Linux mechanism **exists and was measured** this
//! session, so the shape here anticipates it rather than closing it off. Linux
//! deauthorizes and reauthorizes through sysfs (`authorized` 0 then 1);
//! macOS has IOUSBLib's `USBDeviceReEnumerate`, reachable through the
//! `IOCFPlugInTypes` handle the FT232R already advertises. Measured on Darwin 24.6.0
//! / macOS 15.7.8 against an FT232R: `USBDeviceOpen` then `USBDeviceReEnumerate`
//! both return `kIOReturnSuccess`, the target's `sessionID` changes
//! (26843088327954 → 26857977258534) while an untouched second adapter's does not,
//! and `/dev/cu.usbserial-*` returns at the same path. So a real kernel
//! re-enumeration of exactly the named device is reachable.
//!
//! **Two differences make it a partial equivalent, and both are measurements, not
//! guesses.** First, it needs **no privilege**: the open above succeeded at
//! `euid=501`, where the Linux path exists precisely to carry `CAP_DAC_OVERRIDE`,
//! so `capabilities`, `install` and `preflight` are Linux concepts with nothing to
//! port. Second, and the reason a macOS backend is not a drop-in: the Linux
//! mechanism is **two-step and holdable** — deauthorize, wait `--hold-ms`, authorize
//! — while `USBDeviceReEnumerate` is **atomic**. The outage it produces measured
//! **40, 41 and 42 ms** across three trials at 2 ms sampling, and nothing in the API
//! lengthens it. So `cycle --hold-ms` cannot be honoured off Linux and `hold` cannot
//! be implemented at all; what a macOS backend could offer is the replug *event*,
//! not a controllable outage.
//!
//! Landing that backend is a design amendment (§15.45 is written around a
//! privileged Linux helper), so this file stops at the boundary: the crate builds
//! everywhere, and off Linux every verb refuses in one place with the reason and the
//! measurement attached, rather than the workspace failing to compile.

#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform;

#[cfg(target_os = "linux")]
fn main() {
    platform::main();
}

/// Off Linux: refuse, in the one shape the rest of the tree already reads.
///
/// Exit code **2**, matching the Linux binary's "refused" code rather than a
/// success or a panic, so a harness that shells out here gets a verdict instead of
/// an ambiguity. `itest`'s `blessed_replug` already reports *why* the helper is
/// unusable rather than skipping silently; this keeps that contract on a platform
/// where the answer is "not this kernel" instead of "not installed".
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "serial-nexus-replug is Linux-only (design §15.45, notes §3.65 A).\n\
         \n\
         It re-enumerates a USB serial adapter by writing `authorized` 0 then 1 on \
         sysfs, which is a Linux interface, and it is built to carry the Linux file \
         capability CAP_DAC_OVERRIDE that write needs.\n\
         \n\
         A macOS equivalent exists and is NOT yet implemented: IOUSBLib's \
         USBDeviceReEnumerate re-enumerates the named device unprivileged (measured \
         on Darwin 24.6.0: the target's sessionID changes, an untouched adapter's \
         does not, and the /dev/cu.usbserial-* node returns after 40-42 ms). It is a \
         partial equivalent — the outage is atomic and fixed, so `--hold-ms` has no \
         meaning and `hold` has no implementation — and landing it is a design \
         amendment, not a patch.\n\
         \n\
         The crate builds here so that `cargo build --workspace` does not fail on \
         this platform; it does not run here."
    );
    std::process::exit(2);
}
