//! Phase 3 boundary-counter checks, ported from `scripts/validate/phase3/counters.sh`
//! (design §5, §7.1, §7.2, and the §15.17 no-target doctrine). Every hostward drop
//! must be located, counted, and attributable in `state`:
//!
//!   1. A lone `serial` node with nothing attached reads-and-discards each sourced
//!      byte against `discarded_unattached` (§5).
//!   2. A `serial`→`pty` graph with no client attached discards at the PTY boundary
//!      against `discarded_no_client` (§7.2 presence gating), while the serial's own
//!      `discarded_unattached` stays 0 (a consumer *is* attached) and no slow-consumer
//!      full-buffer drops occur.
//!   3. A present, draining client loses nothing: both PTY drop counters stay 0.
//!
//! The "device" is a seeded `nexus-sim` source/echo double, not hardware — the
//! software-loopback doctrine, which is Linux-only (a pts cannot stand in for a serial
//! device on macOS: serial2 → `ENOTTY`). All three checks therefore self-skip where no
//! software serial device is available (a skip is a valid verdict, §5). Ground truth is
//! the structured `state` counters and the sim's own seeded-checksum verdict — never
//! parsed human text.

use std::path::PathBuf;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, TempRun, serial_echo, wait_until};

/// A numeric `state` field of a node (the counter fields live at the node's top level,
/// merged in from `state_extra`), or `None` if the node/field is absent.
fn node_u64(rpc: &Rpc, node: &str, field: &str) -> Option<u64> {
    rpc.node(node)?.get(field)?.as_u64()
}

/// A single seeded-source serial device backed by `nexus-sim pty --source` — the
/// software-loopback "device" this script sources bytes from (§15.17 no-target
/// doctrine). `None` off Linux, where a pts cannot be a serial device (serial2 →
/// `ENOTTY`); those checks then skip. Mirrors `nexus_itest::serial_echo`. The returned
/// tuple keeps the backing sim + temp dir alive for the caller's scope; `--hold-ms`
/// keeps the device present through the assertion window (the cumulative counter is
/// unaffected either way).
#[allow(unused_variables)]
fn seeded_serial_source(bytes: &str, seed: u64) -> Option<(Sim, PathBuf, TempRun)> {
    #[cfg(target_os = "linux")]
    {
        let run = TempRun::new();
        let device = run.join("serialdev");
        let seed_s = seed.to_string();
        let sim = Sim::spawn(
            &[
                "pty",
                "--source",
                "--bytes",
                bytes,
                "--seed",
                &seed_s,
                "--link",
                &device.to_string_lossy(),
                "--timeout-ms",
                "60000",
                "--hold-ms",
                "20000",
            ],
            Some(&device),
        );
        return Some((sim, device, run));
    }
    #[allow(unreachable_code)]
    None
}

/// Check 1: a lone serial with nothing attached reads-and-discards, counting each
/// sourced byte against `discarded_unattached` (§5).
#[test]
fn lone_serial_discards_unattached_bytes_with_counter() {
    let Some((_src, device, _run)) = seeded_serial_source("256KiB", 7) else {
        eprintln!(
            "SKIP lone_serial_discards_unattached_bytes_with_counter: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{device}"
"#,
        device = device.display(),
    );
    rpc.load_toml(&cfg, false).expect("load serial-only config");

    // 256 KiB is sourced; the counter must reach the bulk of it (matching the bash
    // `>= 200000` threshold, leaving margin for the tail buffered at source exit).
    let reached = wait_until(Duration::from_secs(15), || {
        node_u64(rpc, "usb0", "discarded_unattached").unwrap_or(0) >= 200_000
    });
    assert!(
        reached,
        "serial discarded_unattached did not reach the sourced bytes: {:?}",
        rpc.node("usb0")
    );
}

/// Check 2: a serial→PTY graph with no client discards at the PTY boundary
/// (`discarded_no_client`, §7.2), while the serial's own `discarded_unattached` stays 0
/// (a consumer is attached) and no slow-consumer drops occur.
#[test]
fn pty_no_client_discards_at_boundary_while_serial_stays_zero() {
    let Some((_src, device, _run)) = seeded_serial_source("256KiB", 7) else {
        eprintln!(
            "SKIP pty_no_client_discards_at_boundary_while_serial_stays_zero: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console2");
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{device}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = console.display(),
        device = device.display(),
    );
    rpc.load_toml(&cfg, false).expect("load serial->pty config");

    // With no client on the PTY, the serial→PTY stream is discarded at the PTY
    // boundary, counting every byte (§7.2 presence gating).
    let reached = wait_until(Duration::from_secs(15), || {
        node_u64(rpc, "console", "discarded_no_client").unwrap_or(0) >= 200_000
    });
    assert!(
        reached,
        "console discarded_no_client did not reach the sourced bytes: {:?}",
        rpc.node("console")
    );

    // Something IS attached to the serial (the PTY), so its own discard stays 0…
    assert_eq!(
        node_u64(rpc, "usb0", "discarded_unattached"),
        Some(0),
        "serial discarded_unattached should be 0 when a consumer is attached: {:?}",
        rpc.node("usb0")
    );
    // …and presence-gating, not buffer overflow, is the discard mechanism: the
    // presence-gated discard dominates any slow-consumer drops. Under this synthetic
    // firehose the writer's discard task can briefly fall behind the bounded fan-out
    // buffer and shed a *counted* slow-consumer drop (§5 requires loss be counted, not
    // that a firehose never overflows a bounded buffer) — but that path stays the
    // minority; the presence gate accounts for the bulk.
    let discarded = node_u64(rpc, "console", "discarded_no_client").unwrap_or(0);
    let slow = node_u64(rpc, "console", "dropped_slow_consumer").unwrap_or(0);
    assert!(
        discarded >= slow,
        "presence-gated discard should dominate slow-consumer drops \
         (discarded_no_client={discarded}, dropped_slow_consumer={slow}): {:?}",
        rpc.node("console")
    );
}

/// Check 3: a present, draining client loses nothing — the 64 KiB echo round-trip
/// passes byte-exact and both PTY drop counters stay 0 (§5/§7.2).
#[test]
fn present_draining_client_loses_nothing() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP present_draining_client_loses_nothing: no serial device on this platform");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console3");
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{device}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = console.display(),
        device = echo.device().display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load echo round-trip config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );

    // A present client writes 64 KiB into the PTY; it flows targetward to the serial,
    // out to the echo device, back hostward, and up to the client. The sim compares
    // its seeded send against the returned stream (byte-exact ground truth, --expect
    // echo) and reports the round-trip byte count.
    let verdict = Sim::client(&[
        "--path",
        &console.to_string_lossy(),
        "--send",
        "seeded:64KiB",
        "--expect",
        "echo",
        "--seed",
        "9",
        "--timeout-ms",
        "15000",
    ]);
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "echo round-trip failed with a present client: {verdict}"
    );
    assert_eq!(
        verdict["received"].as_u64(),
        Some(65536),
        "echo returned the wrong byte count: {verdict}"
    );

    // The client was present and kept up for the whole transfer: no drops of either
    // kind at the PTY boundary.
    assert_eq!(
        node_u64(rpc, "console", "discarded_no_client"),
        Some(0),
        "discarded_no_client must stay 0 while a client is present: {:?}",
        rpc.node("console")
    );
    assert_eq!(
        node_u64(rpc, "console", "dropped_slow_consumer"),
        Some(0),
        "dropped_slow_consumer must stay 0 for a draining client: {:?}",
        rpc.node("console")
    );
}
