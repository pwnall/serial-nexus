//! Phase 3 `subscribe` + client-termios slice, ported from
//! `scripts/validate/phase3/subscribe.sh` (design §10 / §7.2).
//!
//! Two properties, one per test:
//!   1. The `subscribe` stream carries node status and counter snapshots — every
//!      notification is a `state` notification, and across the stream the serial
//!      node's `active` status, the no-client PTY's `discarded_no_client`, and the
//!      log node's `dropped_bytes` counter all surface (§10).
//!   2. A client that changes its termios surfaces in state as `client_present` +
//!      `client_termios`, and last-close resets both — presence returns false,
//!      `client_termios` clears to null, and a fresh probe reads the daemon's
//!      baseline (echo off, EXTPROC on) rather than the departed client's B9600
//!      (§7.2).
//!
//! No hardware: the "device" is a `nexus-sim` PTY double standing in as a serial
//! port. That software-loopback works only on Linux (a pts cannot be a serial port
//! on macOS — serial2 → `ENOTTY`, see the harness module note), so both tests gate
//! on [`serial_echo`] and self-skip where no such device exists (a skip is a valid
//! verdict, §5).

use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, Subscription, serial_echo, wait_until};
use serde_json::Value;

/// The node object named `name` inside a `state` notification's `params.nodes`,
/// or `None`. Mirrors the bash `node_of` selector.
fn node_in_snapshot<'a>(note: &'a Value, name: &str) -> Option<&'a Value> {
    note.get("params")?
        .get("nodes")?
        .as_array()?
        .iter()
        .find(|n| n.get("name").and_then(Value::as_str) == Some(name))
}

/// Collect up to `want` notifications from the stream (or until `budget` elapses).
/// The Rust replacement for `timeout N serialnexusctl subscribe --count K`.
fn collect_snapshots(sub: &mut Subscription, want: usize, budget: Duration) -> Vec<Value> {
    let deadline = Instant::now() + budget;
    let mut out = Vec::new();
    while out.len() < want {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match sub.next(remaining) {
            Some(n) => out.push(n),
            None => break,
        }
    }
    out
}

/// Check 1: the `subscribe` stream carries node status and counter snapshots.
///
/// A finite `pty --source` feeds a `free-for-all` serial node whose hostward bytes
/// fan out to a no-client PTY (which discards-with-count) and a log. Once a discard
/// has accrued, the subscribe stream must report the serial node `active`, the PTY's
/// `discarded_no_client > 0`, and the log's `dropped_bytes` as a number — every line
/// being a well-formed `state` notification.
#[test]
fn subscribe_stream_carries_status_and_counter_snapshots() {
    // The source below is a sim pts standing in as a serial device; that only works
    // on Linux. `serial_echo()` is `Some` exactly on such a platform (we discard the
    // echo it hands back — we need a *source*, spawned separately below).
    if serial_echo().is_none() {
        eprintln!(
            "SKIP subscribe_stream_carries_status_and_counter_snapshots: \
             no serial device on this platform"
        );
        return;
    }

    let d = Daemon::start();
    let rpc = d.rpc();

    // --hold-ms keeps the device "plugged in" after the finite source completes, so
    // usb0 stays active while we observe the stream (a closed device faults-and-waits,
    // §7.1); this test observes the subscribe stream, not reconnect.
    let dev1 = d.run().join("dev1");
    let dev1_s = dev1.to_string_lossy().into_owned();
    let _src = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            "256KiB",
            "--seed",
            "7",
            "--hold-ms",
            "30000",
            "--link",
            dev1_s.as_str(),
        ],
        Some(&dev1),
    );

    let console1 = d.run().join("console1");
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
device = "{dev}"
[[node]]
type = "log"
name = "cap"
directory = "{dir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
        console = console1.display(),
        dev = dev1.display(),
        dir = d.run().path().display(),
    );
    rpc.load_toml(&cfg, false).expect("load c1");

    // Data has flowed and the no-client PTY has discarded some (a counter to observe).
    let discarded = wait_until(Duration::from_secs(15), || {
        rpc.node("console")
            .and_then(|c| c.get("discarded_no_client").and_then(Value::as_u64))
            .unwrap_or(0)
            > 0
    });
    assert!(
        discarded,
        "no discard accrued to observe (source -> console never flowed): {:?}",
        rpc.node("console")
    );

    // Observe the stream (the daemon emits a full snapshot every ~200ms while a
    // subscriber exists, §10).
    let mut sub = rpc.subscribe();
    let notes = collect_snapshots(&mut sub, 4, Duration::from_secs(10));
    assert!(!notes.is_empty(), "subscribe produced no notifications");

    // Every line is a well-formed state notification.
    for n in &notes {
        assert_eq!(
            n.get("method").and_then(Value::as_str),
            Some("state"),
            "subscribe emitted a non-state notification: {n}"
        );
        assert!(
            n.get("params")
                .and_then(|p| p.get("nodes"))
                .map(Value::is_array)
                .unwrap_or(false),
            "state notification missing a nodes array: {n}"
        );
    }

    // Status is streamed...
    assert!(
        notes.iter().any(|n| node_in_snapshot(n, "usb0")
            .and_then(|u| u.get("status").and_then(Value::as_str))
            == Some("active")),
        "usb0 active status not in the subscribe stream"
    );
    // ...and counters are streamed (the no-client discard the source produced)...
    assert!(
        notes.iter().any(|n| node_in_snapshot(n, "console")
            .and_then(|c| c.get("discarded_no_client").and_then(Value::as_u64))
            .unwrap_or(0)
            > 0),
        "console discard counter not in the subscribe stream"
    );
    // ...including the log node's dropped_bytes counter.
    assert!(
        notes.iter().any(|n| node_in_snapshot(n, "cap")
            .and_then(|c| c.get("dropped_bytes"))
            .map(Value::is_number)
            .unwrap_or(false)),
        "log dropped_bytes counter not in the subscribe stream"
    );
}

/// Check 2: a client's termios change surfaces in state, and last close resets it.
///
/// A client attaches to the console PTY, sets a distinctive baud (B9600), and holds
/// the slave open. While present the subscribe stream must report `client_present`
/// true and `client_termios.baud == "B9600"`. When it departs, the daemon re-asserts
/// the baseline termios and forgets the client's settings (§7.2): presence returns
/// false, `client_termios` clears to null, and a fresh probe reads the baseline
/// (echo off, EXTPROC on) rather than the departed client's B9600.
#[test]
fn client_termios_change_surfaces_in_stream_and_resets_on_last_close() {
    // The serial "device" is a sim echo pts — Linux-only; skip elsewhere.
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP client_termios_change_surfaces_in_stream_and_resets_on_last_close: \
             no serial device on this platform"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();

    let console2 = d.run().join("console2");
    let console2_s = console2.to_string_lossy().into_owned();
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
device = "{dev}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = console2.display(),
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load c2");

    // Subscribe first; the daemon's periodic snapshot no-ops until a subscriber
    // exists, so the first snapshot landing proves the stream is live before the
    // client attaches (bounded, not a bare sleep — plan §3).
    let mut sub = rpc.subscribe();
    assert!(
        sub.next(Duration::from_secs(5)).is_some(),
        "subscription never produced its first snapshot"
    );

    // Attach a client that sets a distinctive baud and holds the slave open long
    // enough for several snapshots to capture it. It runs to completion here; the
    // snapshots emitted while it was present are buffered on the stream socket and
    // read below (the daemon keeps emitting even while this test thread blocks).
    let verdict = Sim::client(&[
        "--path",
        console2_s.as_str(),
        "--set-baud",
        "9600",
        "--hold-ms",
        "1800",
        "--seed",
        "1",
        "--timeout-ms",
        "15000",
    ]);
    assert_eq!(
        verdict.get("pass").and_then(Value::as_bool),
        Some(true),
        "termios-setting client failed: {verdict}"
    );

    // A snapshot taken while the client was present must report its baud, and while
    // it was attached client_present must have been observed true.
    let mut present_seen = false;
    let mut baud_seen = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while !(present_seen && baud_seen) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Some(n) = sub.next(remaining) else { break };
        if let Some(console) = node_in_snapshot(&n, "console") {
            if console.get("client_present").and_then(Value::as_bool) == Some(true) {
                present_seen = true;
            }
            if console
                .get("client_termios")
                .and_then(|t| t.get("baud"))
                .and_then(Value::as_str)
                == Some("B9600")
            {
                baud_seen = true;
            }
        }
    }
    assert!(
        baud_seen,
        "client_termios baud change (B9600) never surfaced in the stream"
    );
    assert!(
        present_seen,
        "client_present never observed true in the stream"
    );

    // Last-close reset (§7.2): once the B9600 client departs, the daemon re-asserts
    // the baseline termios and forgets the client's settings. Verify the invariant
    // directly: presence returns false, state clears client_termios to null, and a
    // fresh probe reads the baseline (EXTPROC on, echo off) rather than B9600.
    let gone = wait_until(Duration::from_secs(5), || {
        rpc.node("console")
            .and_then(|c| c.get("client_present").and_then(Value::as_bool))
            == Some(false)
    });
    assert!(
        gone,
        "client_present never returned false after the B9600 client exited: {:?}",
        rpc.node("console")
    );

    let console = rpc.node("console").expect("console node present");
    assert_eq!(
        console.get("client_termios"),
        Some(&Value::Null),
        "client_termios not cleared to null on last close: {:?}",
        console.get("client_termios")
    );

    // A fresh probe (observe-only) must read the daemon's re-asserted baseline, not
    // the departed client's B9600 raw/echo-off — echo off, EXTPROC on.
    let baseline = Sim::client(&["--path", console2_s.as_str(), "--report-termios"]);
    assert_eq!(
        baseline.get("echo").and_then(Value::as_bool),
        Some(false),
        "baseline termios (echo) not restored after the B9600 client closed: {baseline}"
    );
    assert_eq!(
        baseline.get("extproc").and_then(Value::as_bool),
        Some(true),
        "baseline termios (extproc) not restored after the B9600 client closed: {baseline}"
    );
}
