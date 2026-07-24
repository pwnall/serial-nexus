//! Phase 0 licensing gate, folded from `scripts/validate/phase0/license-gate.sh` into
//! the Rust harness (§16.11). Proves the §13 permissive-only policy actually *rejects* a
//! banned crate rather than merely being configured to (plan §2: "the gate is proven,
//! not assumed"): the clean workspace passes `cargo deny check bans`, and a scratch crate
//! that pulls in the banned `serialport` fails it.
//!
//! `cargo-deny` is the precondition, not the subject — the dedicated CI gate installs it.
//! Where it is absent the test **self-skips** with a valid verdict (§13/§15.17, the same
//! skip discipline as the doctor's `skipped(no adapter)`), so it runs wherever the tool
//! exists and never blocks a machine without it.

use std::process::Command;

use nexus_itest::TempRun;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

fn have_cargo_deny() -> bool {
    Command::new("cargo")
        .args(["deny", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn cargo_deny_ban_list_rejects_a_banned_crate() {
    if !have_cargo_deny() {
        eprintln!("SKIP cargo_deny_ban_list_rejects_a_banned_crate: cargo-deny not installed");
        return;
    }

    // 1. The clean tree passes the ban check (offline: the workspace Cargo.lock is
    //    committed, so no index fetch is needed).
    let clean = Command::new("cargo")
        .args(["deny", "--manifest-path"])
        .arg(format!("{REPO}/Cargo.toml"))
        .args(["check", "bans"])
        .output()
        .expect("run cargo deny on the clean tree");
    assert!(
        clean.status.success(),
        "the clean tree unexpectedly fails the ban check:\n{}",
        String::from_utf8_lossy(&clean.stderr)
    );

    // 2. A scratch crate pulling in the banned `serialport` must fail the ban check.
    let scratch = TempRun::new();
    let proj = scratch.join("banned");
    let created = Command::new("cargo")
        .args(["new", "--quiet", "--bin"])
        .arg(&proj)
        .output()
        .expect("cargo new");
    assert!(
        created.status.success(),
        "cargo new failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let cargo_toml = proj.join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&cargo_toml).expect("read scratch Cargo.toml");
    manifest.push_str("\nserialport = \"*\"\n");
    std::fs::write(&cargo_toml, manifest).expect("write scratch Cargo.toml");
    std::fs::copy(format!("{REPO}/deny.toml"), proj.join("deny.toml")).expect("copy deny.toml");

    let banned = Command::new("cargo")
        .args(["deny", "--manifest-path"])
        .arg(proj.join("Cargo.toml"))
        .args(["check", "bans"])
        .output()
        .expect("run cargo deny on the banned crate");
    assert!(
        !banned.status.success(),
        "the ban list did NOT reject `serialport` — the §13 gate is a no-op"
    );
}
