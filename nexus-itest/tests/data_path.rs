//! Phase 2 data-plane slice, ported from `scripts/validate/phase2/data-path.sh`
//! (design §5, §7.1, §7.2): real bytes flow client → daemon → device through a
//! `serial → pty` graph, the §7.2 baseline termios is confirmed end to end, and
//! client presence tracks the operator opening/closing the PTY slave.
//!
//! Topology (software-loopback, the no-target doctrine §15.17):
//!
//!   nexus-sim client ─▶ [ pty "console" ]──edge──[ serial "usb0" ] ─▶ nexus-sim pty --echo
//!      (operator)          $tty symlink              device=$dev          (the "device")
//!
//! The device echoes; a 64 KiB seeded round-trip that returns byte-identical
//! proves client → PTY → serial → device → serial → PTY → client with nothing
//! lost — ground truth is the sim's own SHA-256 of what it sent vs. received, not
//! a judgement (§5).
//!
//! The data-path test needs a serial *device*, so it uses [`serial_echo`] and
//! self-skips where none exists (macOS: a pty cannot stand in for a serial port,
//! serial2 → `ENOTTY`). The presence-transition property is a PTY-local one that
//! needs no serial device, so it is also exercised on a lone pty node that runs on
//! every platform.

use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};

/// A node's `client_present` state field (`None` if absent / not a pty).
fn client_present(rpc: &Rpc, node: &str) -> Option<bool> {
    rpc.node(node)?.get("client_present")?.as_bool()
}

/// Full end-to-end port of `data-path.sh`: the `serial → pty` graph goes active,
/// installs its symlink, shows the §7.2 baseline termios, and carries a 64 KiB
/// seeded echo round-trip byte-exact; then presence tracks a client holding and
/// releasing the slave. Skips where no serial device is available (§5).
#[test]
fn serial_pty_data_path_and_presence() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP serial_pty_data_path_and_presence: no serial device on this platform");
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let tty = d.run().join("console");
    let tty_s = tty.to_string_lossy().into_owned();

    // serial(usb0, free-for-all so the PTY may write without a lock) → pty(console).
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{tty}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[edge]]
a = "usb0"
b = "console"
"#,
        tty = tty.display(),
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load serial->pty graph");

    // Both nodes active: the device is present, so the serial node opened it.
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "usb0 not active (serial did not open the device): {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );

    // The PTY installed its stable symlink.
    assert!(
        std::fs::symlink_metadata(&tty)
            .unwrap()
            .file_type()
            .is_symlink(),
        "pty symlink not created at {}",
        tty.display()
    );

    // Presence starts false — no client has attached yet (asserted before any
    // client opens the slave, so there is no close-detection race).
    assert_eq!(
        client_present(rpc, "console"),
        Some(false),
        "client_present should be false with no client"
    );

    // Baseline termios, observed from the client's side of the slave (§7.2):
    // raw (no OPOST, no ICANON), echo off, EXTPROC on.
    let t = Sim::client(&["--path", tty_s.as_str(), "--report-termios"]);
    assert_eq!(
        t["echo"].as_bool(),
        Some(false),
        "baseline echo must be off: {t}"
    );
    assert_eq!(
        t["icanon"].as_bool(),
        Some(false),
        "baseline must be non-canonical (ICANON off): {t}"
    );
    assert_eq!(
        t["extproc"].as_bool(),
        Some(true),
        "baseline EXTPROC must be on: {t}"
    );
    assert_eq!(
        t["opost"].as_bool(),
        Some(false),
        "baseline OPOST must be off (raw): {t}"
    );

    // THE DATA PATH: 64 KiB seeded out, the device echoes it back byte-identical.
    // Pass is the sim's own SHA-256(sent) == SHA-256(received) — byte-exact truth.
    let v = Sim::client(&[
        "--path",
        tty_s.as_str(),
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
        "64KiB echo round-trip failed (bytes lost/mangled): {v}"
    );
    assert_eq!(v["sent"].as_u64(), Some(65536), "sent byte count: {v}");
    assert_eq!(
        v["received"].as_u64(),
        Some(65536),
        "received byte count: {v}"
    );
    assert_eq!(
        v["sha256_sent"].as_str(),
        v["sha256_received"].as_str(),
        "echoed bytes were not identical to what was sent: {v}"
    );

    // Presence transitions: the round-trip client has exited, so let presence
    // settle false, then hold the slave open (a sim client parked on --hold-ms),
    // watch it go true, then false again on close.
    assert!(
        wait_until(Duration::from_secs(2), || client_present(rpc, "console")
            == Some(false)),
        "client_present did not settle false after the round-trip client exited"
    );
    let holder = Sim::spawn(
        &[
            "client",
            "--path",
            tty_s.as_str(),
            "--hold-ms",
            "60000",
            "--timeout-ms",
            "65000",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(3), || client_present(rpc, "console")
            == Some(true)),
        "client_present never went true while a client held the slave"
    );
    drop(holder); // kill the holder → its slave fd closes
    assert!(
        wait_until(Duration::from_secs(2), || client_present(rpc, "console")
            == Some(false)),
        "client_present never returned false after the client released the slave"
    );
}

/// The presence-tracking half of `data-path.sh`, on a lone pty node so it runs on
/// every platform (a pty needs no serial device, §15.17): presence starts false,
/// goes true while a client holds the slave, and returns false on close (§7.2).
#[test]
fn pty_client_presence_transitions() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let tty = d.run().join("console");
    let tty_s = tty.to_string_lossy().into_owned();

    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{tty}"
"#,
        tty = tty.display(),
    );
    rpc.load_toml(&cfg, false).expect("load lone pty node");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(5)),
        "console not active: {:?}",
        rpc.node("console")
    );
    assert!(
        std::fs::symlink_metadata(&tty)
            .unwrap()
            .file_type()
            .is_symlink(),
        "pty symlink not created at {}",
        tty.display()
    );

    // Starts false — no client attached.
    assert_eq!(
        client_present(rpc, "console"),
        Some(false),
        "client_present should start false with no client"
    );

    // Hold the slave open (a parked sim client), watch presence rise.
    let holder = Sim::spawn(
        &[
            "client",
            "--path",
            tty_s.as_str(),
            "--hold-ms",
            "60000",
            "--timeout-ms",
            "65000",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(3), || client_present(rpc, "console")
            == Some(true)),
        "client_present never went true while a client held the slave"
    );

    // Close it, watch presence fall within a second.
    drop(holder);
    assert!(
        wait_until(Duration::from_secs(2), || client_present(rpc, "console")
            == Some(false)),
        "client_present never returned false after the client released the slave"
    );
}
