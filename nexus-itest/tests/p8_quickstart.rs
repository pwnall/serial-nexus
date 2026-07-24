//! Phase 8 validation (item 1): the five-minute quickstart, ported from
//! `scripts/validate/phase8/quickstart.sh` (design §2, plan §Phase 8 item 1). The
//! README happy path, exactly:
//!
//! ```text
//!   nexus-sim client  ─▶  [ pty "console" ]──edge──[ serial "usb0" ]  ─▶  nexus-sim pty --echo
//!      (operator)            $TTY symlink          free-for-all             (the "device")
//! ```
//!
//! `free-for-all` skips the write lock, so an operator just types (§6). The success
//! condition is a 64 KiB seeded round-trip that returns byte-identical, plus the
//! no-terminal `send` path (an atomic acquire-write-release).
//!
//! Assertions preserved from the bash, in order:
//! 1. the control socket is mode 0600 — the socket IS the authorization model (§10);
//! 2. `usb0` (serial) and `console` (pty) both reach `active`, and the pty symlink is
//!    created;
//! 3. a 64 KiB seeded echo round-trip is byte-identical (`pass && sent==65536 &&
//!    received==65536` — the sim's `pass` folds in `sha256_sent==sha256_received`);
//! 4. `send usb0 --line "hello"` delivers, reporting `delivered==true && sent==6`.
//!
//! This drives a `serial` node over a software-loopback echo device
//! ([`serial_echo`]), which is Linux-only (a pty cannot be a serial device on macOS —
//! `serial2` → `ENOTTY`), so it **skips** where no such device exists.
//!
//! Deviations from the bash, and why:
//! * The build step is dropped — `cargo test --workspace` compiles the binaries the
//!   harness needs before the test body runs.
//! * The 300 s wall-clock budget in the bash timed a clean checkout *including the
//!   build*; here the build is already done, so the budget is asserted only over the
//!   runtime portion (a much looser, still-meaningful bound).

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, serial_echo};

/// The five-minute wall-clock budget (plan §Phase 8 item 1). Asserted over the
/// runtime portion only (the build is already done under `cargo test`).
const BUDGET: Duration = Duration::from_secs(300);

#[test]
fn quickstart_echo_round_trip_under_budget() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP quickstart_echo_round_trip_under_budget: no serial device on this platform"
        );
        return;
    };
    let start = Instant::now();

    // The daemon, in a short-path runtime dir (SUN_LEN, §10).
    let d = Daemon::start();
    let rpc = d.rpc();

    // 1. The socket IS the authorization model — mode 0600 (§10).
    let mode = std::fs::metadata(d.socket()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "control socket perms are {mode:o}, want 600");

    // 2. The demo config: serial (host-facing, free-for-all) -> pty (target-facing).
    let console = d.run().join("console");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[edge]]
a = "usb0"
b = "console"
"#,
        dev = echo.device().display(),
        console = console.display(),
    );
    rpc.load_toml(&cfg, false).expect("load demo config");

    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 never reached active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );
    // The pty exposes a stable symlink at its configured path (§7.2).
    assert!(
        std::fs::symlink_metadata(&console)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "pty symlink not created at {}",
        console.display()
    );

    // 3. The echo verdict: 64 KiB out, byte-identical back — the whole round trip. The
    //    sim's `pass` for `--expect echo` folds in `sha256_sent == sha256_received`, so
    //    a passing verdict is the byte-exact ground truth (§5).
    let tty = console.to_string_lossy().into_owned();
    let v = Sim::client(&[
        "--path",
        &tty,
        "--send",
        "seeded:64KiB",
        "--expect",
        "echo",
        "--seed",
        "42",
        "--timeout-ms",
        "15000",
    ]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "64KiB echo round-trip did not pass: {v}"
    );
    assert_eq!(v["sent"].as_u64(), Some(65536), "sent != 65536: {v}");
    assert_eq!(
        v["received"].as_u64(),
        Some(65536),
        "received != 65536: {v}"
    );

    // 4. The no-terminal path: `send` is an atomic acquire-write-release. "hello\n" is
    //    6 bytes on the wire.
    let sent = rpc
        .send("usb0", "hello", false, 15000)
        .expect("send usb0 --line");
    assert_eq!(
        sent["delivered"].as_bool(),
        Some(true),
        "send usb0 --line did not deliver: {sent}"
    );
    assert_eq!(
        sent["sent"].as_u64(),
        Some(6),
        "send reported wrong byte count: {sent}"
    );

    // The runtime portion stays well under the five-minute budget.
    let elapsed = start.elapsed();
    assert!(
        elapsed < BUDGET,
        "quickstart took {elapsed:?}, over the {BUDGET:?} budget"
    );

    // `echo` (the backing sim device), `d` (the daemon), and `d`'s temp dir all drop
    // here — the shutdown + kill + cleanup the bash's EXIT trap did by hand.
    drop(echo);
}
