#![forbid(unsafe_code)]

//! `serialnexusctl` — the serial_nexus CLI.
//!
//! A JSON-RPC client plus a rendering layer, nothing else (§15.16). `--json`
//! (raw result pass-through) exists from the first commit, making the CLI
//! agent-usable immediately. Lands in phase 2.

fn main() {
    eprintln!("serialnexusctl: lands in phase 2");
    std::process::exit(2);
}
