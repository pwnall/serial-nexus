#![deny(unsafe_code)]

//! `serialnexusd` — the serial_nexus daemon.
//!
//! Unsafe is denied crate-wide and localized with `#[allow(unsafe_code)]` to
//! the `sys` module, which isolates the raw ioctls that nix/rustix don't wrap
//! (TIOCGICOUNT is the known candidate; §2).
//!
//! The walking skeleton lands in phase 2.

fn main() {
    eprintln!("serialnexusd: walking skeleton lands in phase 2");
    std::process::exit(2);
}
