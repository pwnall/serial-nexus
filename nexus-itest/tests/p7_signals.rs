//! Phase 7 serial-signal + lifecycle slice, ported from
//! `scripts/validate/phase7/signals.sh` (design §7.1 signal verbs, §6/§15.20 lock
//! detach-release, §13 no-target doctrine). Three properties:
//!
//! 1. The serial-signal verbs REACH the live port: `send-break` latches on a pts
//!    (succeeds), while `set-modem`/`pulse-dtr` reach the driver and are cleanly
//!    rejected by the pts (ENOTTY — the exact Tier-3 hardware boundary a real UART
//!    would honor), and the node stays healthy throughout. True master-side
//!    observation of a break/DTR pulse is a Tier-3 hardware checklist item (a real
//!    null modem, §13); unprivileged, we prove only that the verb reached the port.
//! 2. `remove-node --cascade` flushes a log's queue fully before the node
//!    disappears: the captured file is byte-complete (flushed, not truncated), and
//!    the node + its edge are gone from both state and config.
//! 3. `remove-node --cascade` of a lock-HOLDING writer releases the surviving host
//!    endpoint's lock cleanly — no phantom holder / origin, so the endpoint does not
//!    wedge permanently locked by a departed writer (§6/§15.20).
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * All three drive a `serial` node, so they obtain a lossless software device
//!   from `serial_echo` (a `nexus-sim pty --echo` pts) and **skip** where none
//!   exists (macOS): the signal verbs and a serial endpoint's write-lock are
//!   inherently serial-device operations. The pts behaves identically to the bash's
//!   `pty --source` slave for the signal verbs (break latches, modem ioctls ENOTTY).
//! * The bash sourced the log stream with a `pty --source` device whose checksum
//!   goes only to discarded stdout; check 2 instead drives a seeded `client` batch
//!   through a console pty that the echo device returns hostward, using the client
//!   verdict's `sha256_sent` as byte-exact ground truth (the p3_log pattern) — the
//!   identical "cascade flushes the whole stream, complete not truncated" property,
//!   strengthened from a bare size compare to a checksum.
//! * Signal-verb rejection is asserted structurally on the daemon's RpcError message
//!   (`"set-modem on …"` / `"pulse-dtr on …"`, the ioctl-dispatch path) in place of
//!   the bash's `grep -iE 'ioctl|set-modem on'` on CLI stderr — a device-level error,
//!   not a routing error, proving the verb reached the port and issued the ioctl (§5).

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Sim, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

const SIZE_256K: u64 = 256 * 1024;

/// Current on-disk length of `p` (0 if absent) — the portable replacement for
/// `stat -c %s … || echo 0`.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Drive one seeded batch through an echo device: write `send_spec` (e.g.
/// `seeded:256KiB`) into `tty`, read the echo back, and return the `client` verdict
/// (whose `sha256_sent` is the batch's byte-exact ground truth).
fn echo_send(tty: &Path, send_spec: &str, seed: u64) -> Value {
    let path = tty.to_string_lossy().into_owned();
    let seed = seed.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        send_spec,
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "30000",
    ])
}

// ---- Check 1: the serial-signal verbs reach the live port (§7.1) ----------------

#[test]
fn signal_verbs_reach_the_live_port_and_leave_the_node_healthy() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP signal_verbs_reach_the_live_port_and_leave_the_node_healthy: \
             no serial device on this platform"
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
device = "{dev}"
arbitration = "free-for-all"
"#,
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load signal-verb config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // send-break: reaches the live port and latches on a pts (succeeds).
    rpc.send_break("usb0", 30)
        .expect("send-break must reach the port and latch on a pts");

    // set-modem: reaches the driver; a pts has no modem lines and rejects the ioctl
    // (ENOTTY). A device-level error carrying "set-modem on <node>" proves the verb
    // reached the port (past `serial_port`), rather than a routing error.
    let err = rpc
        .call("set-modem", json!({ "node": "usb0", "dtr": true }))
        .expect_err("set-modem must fail on a pts (a pts has no modem lines)");
    assert!(
        err.message.contains("set-modem on"),
        "set-modem did not reach the live port (unexpected error): {}",
        err.message
    );

    // pulse-dtr: same — reaches the driver, cleanly rejected by the pts.
    let err = rpc
        .call("pulse-dtr", json!({ "node": "usb0", "ms": 20 }))
        .expect_err("pulse-dtr must fail on a pts");
    assert!(
        err.message.contains("pulse-dtr on"),
        "pulse-dtr did not reach the live port (unexpected error): {}",
        err.message
    );

    // The node is undisturbed by the signal verbs.
    assert_eq!(
        rpc.node_status("usb0"),
        "active",
        "signal verbs disturbed the serial node: {:?}",
        rpc.node("usb0")
    );
}

// ---- Check 2: remove-node --cascade flushes the log queue before removal (§7.3) --

#[test]
fn remove_node_cascade_flushes_the_log_fully() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP remove_node_cascade_flushes_the_log_fully: no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let console = d.run().join("console");

    // A free-for-all serial feeds every hostward byte to a capturing log; a console
    // pty injects a 256 KiB seeded batch the echo device returns hostward.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
        console = console.display(),
        dev = echo.device().display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load capture config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    let v = echo_send(&console, "seeded:256KiB", 5);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "256 KiB echo did not round-trip: {v}"
    );
    assert_eq!(
        v["received"].as_u64(),
        Some(SIZE_256K),
        "echo received != 256 KiB: {v}"
    );
    let sent_sha = v["sha256_sent"]
        .as_str()
        .expect("client reported sha256_sent")
        .to_owned();

    // Wait until the log has captured the full sourced stream, then cascade-remove it.
    let cap = logdir.join("cap.log");
    assert!(
        wait_until(Duration::from_secs(15), || file_len(&cap) >= SIZE_256K),
        "log never captured the full stream (queued={:?})",
        rpc.node("cap").map(|n| n["queued_bytes"].clone())
    );
    rpc.remove_node("cap", true)
        .expect("remove-node cap --cascade failed");

    // The node is gone from state, its edge removed, and it is gone from config.
    assert!(
        rpc.node("cap").is_none(),
        "cap still present in state after removal"
    );
    let dump = rpc.dump();
    let cap_in_config = dump
        .get("node")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|n| n.get("name").and_then(Value::as_str) == Some("cap"));
    assert!(!cap_in_config, "cap still in config after removal: {dump}");

    // The file is complete (flushed on cascade, never truncated) — byte-exact.
    let data = std::fs::read(&cap).expect("read cap.log");
    assert_eq!(
        data.len() as u64,
        SIZE_256K,
        "log file not complete after cascade flush (captured {} bytes)",
        data.len()
    );
    assert_eq!(
        sha256_hex(&data),
        sent_sha,
        "cap.log != sent stream (lossy or truncated cascade flush)"
    );
}

// ---- Check 3: cascade of a lock-HOLDING writer releases the host lock (§6/§15.20) --

#[test]
fn remove_node_cascade_of_lock_holder_releases_the_host_lock() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP remove_node_cascade_of_lock_holder_releases_the_host_lock: \
             no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let ptya = d.run().join("ptya");

    // An exclusive serial host with a single pty writer that will hold its lock.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "exclusive"
[[node]]
type = "pty"
name = "ptya"
path = "{ptya}"
[[edge]]
a = "usb0"
b = "ptya"
"#,
        dev = echo.device().display(),
        ptya = ptya.display(),
    );
    rpc.load_toml(&cfg, false).expect("load lock-holder config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // ptya acquires usb0's exclusive write lock.
    rpc.lock("ptya", false, false, None).expect("lock ptya");
    let holder = rpc.node("usb0").expect("usb0 present")["lock"]["holder"]
        .as_str()
        .map(str::to_owned);
    assert_eq!(
        holder.as_deref(),
        Some("ptya"),
        "ptya did not hold usb0's lock: {:?}",
        rpc.node("usb0").map(|n| n["lock"].clone())
    );

    // Cascade-remove the lock-holding writer. The surviving serial's lock must be
    // free — no phantom holder, no phantom origin — recoverable by a later writer.
    rpc.remove_node("ptya", true)
        .expect("remove-node ptya --cascade failed");
    let released = wait_until(Duration::from_secs(5), || {
        let Some(n) = rpc.node("usb0") else {
            return false;
        };
        let lock = &n["lock"];
        let holder_free = lock["holder"].is_null();
        // A surviving endpoint keeps its (now-empty) lock; an absent `origins` array
        // reads as empty too (jq `null|length == 0`), so accept either shape.
        let no_origins = lock["origins"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true);
        holder_free && no_origins
    });
    assert!(
        released,
        "cascade left a phantom lock holder/origin on usb0: {:?}",
        rpc.node("usb0").map(|n| n["lock"].clone())
    );
}
