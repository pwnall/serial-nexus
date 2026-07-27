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
//!   3. A present, draining client loses nothing: both PTY drop counters stay 0. Its
//!      console carries an explicit `hostward_buffer = 8192`, matching the consoles in
//!      `p3_log`. That is **prophylactic here**, not a reproduced failure: at 64 KiB
//!      this check survived 65 runs at the default depth under 4–8× CPU
//!      oversubscription, where `p3_log`'s 256 KiB sibling failed 14/40. The shape is
//!      the same, though — the pty pump sheds with a counter rather than blocking when
//!      its bridge fills (§5 "bounded buffering where configured, then counted drops",
//!      §15.19) — so a burst that outruns 32 chunks would fail this check with no
//!      defect behind it, and "a client that kept up" is exactly what the deep buffer
//!      states. Check 2 deliberately keeps the default: it is measuring the drop.
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
    //
    // The threshold is on the **sum** of the boundary's two counters, and that is not a
    // weakening — it is what makes the check reachable. `discarded_no_client` alone can
    // never reach 200_000 on a run where the pty pump also sheds: the two counters
    // partition the same 256 KiB, so every `dropped_slow_consumer` byte is one the
    // presence gate never got to see. Reproduced under CPU load at
    // `discarded_no_client=196604` + `dropped_slow_consumer=65540` = 262144 exactly —
    // an assertion failure on a daemon that had located and counted every single byte,
    // which is all §5 requires. The property "the *presence gate*, not buffer overflow,
    // is the mechanism" is asserted separately below, and that is the assertion a real
    // presence-gating regression would trip (it would move the bytes, not lose them).
    let reached = wait_until(Duration::from_secs(15), || {
        node_u64(rpc, "console", "discarded_no_client").unwrap_or(0)
            + node_u64(rpc, "console", "dropped_slow_consumer").unwrap_or(0)
            >= 200_000
    });
    assert!(
        reached,
        "the console's counted hostward loss did not reach the sourced bytes: {:?}",
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
    // `hostward_buffer = 8192` — do not "simplify" it away. This check asserts a
    // byte-exact 64 KiB echo *and* `dropped_slow_consumer == 0`, i.e. it deliberately
    // wants the no-loss case; but the pty pump→writer bridge sheds with a counter
    // rather than blocking when it fills (§5 "bounded buffering where configured, then
    // counted drops", §15.19), so at the 32-chunk default a burst that outruns the
    // bridge fails both assertions with the daemon behaving exactly as designed. The
    // deep buffer *is* the statement "this client kept up". Unlike `p3_log`'s 256 KiB
    // consoles — which failed 14/40 at the default under load — 64 KiB never actually
    // lost the race in 65 runs at 4–8× oversubscription, so this one is prophylactic.
    // Raising the *serial* node's depth instead would not work: the pty pump drops
    // rather than awaits, so it never backpressures upstream — the pty's own depth is
    // the only buffer in this path.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
hostward_buffer = 8192
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
    // Both assertions print the PTY's own drop counters, so a failure diagnoses itself:
    // `received + dropped_slow_consumer == sent` is a *counted* boundary shed (§5 — the
    // console's `hostward_buffer` was too shallow for the burst), while a short sum
    // means bytes vanished uncounted, which is a real defect.
    let drops = |field: &str| match node_u64(rpc, "console", field) {
        Some(v) => v.to_string(),
        None => "absent".to_owned(),
    };
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "echo round-trip failed with a present client: {verdict} \
         [console: dropped_slow_consumer={} discarded_no_client={}]",
        drops("dropped_slow_consumer"),
        drops("discarded_no_client")
    );
    assert_eq!(
        verdict["received"].as_u64(),
        Some(65536),
        "echo returned the wrong byte count: {verdict} \
         [console: dropped_slow_consumer={} discarded_no_client={}]",
        drops("dropped_slow_consumer"),
        drops("discarded_no_client")
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
