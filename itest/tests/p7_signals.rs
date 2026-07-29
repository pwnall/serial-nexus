//! Phase 7 serial-signal + lifecycle slice, ported from
//! `scripts/validate/phase7/signals.sh` (design §7.1 signal verbs, §6/§15.20 lock
//! detach-release, §13 no-target doctrine). Five properties:
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
//! 4. The line-holding verbs (`send-break`, `pulse-dtr`) are mutually exclusive per
//!    port: a second one issued from a second control connection is refused, so the
//!    shorter verb's restore cannot clear the line under the longer one (37-SER-1).
//! 5. A present-but-mistyped `ms`/`assert` is a named refusal, not a silent fall back
//!    to the verb's default (37-SER-2).
//!
//! Properties 4 and 5 have no bash ancestor: they are review findings 37-SER-1 and
//! 37-SER-2, and they follow the same self-skip discipline as the rest.
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * All of them drive a `serial` node, so they obtain a lossless software device
//!   from `serial_echo` (a `serial-nexus-sim pty --echo` pts) and **skip** where none
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

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, Sim, file_len, serial_echo, sha256_hex, wait_until};

const SIZE_256K: u64 = 256 * 1024;

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
    //
    // `hostward_buffer = 8192` on the console is load-bearing, not decoration — do not
    // "simplify" it away. The measured subject here is the **log** (does `--cascade`
    // flush its queue whole?); the console is only the instrument that returns the
    // batch, and the assertion below insists its 256 KiB echo came back complete. But
    // hostward flow is lossy at boundaries by design (§5, §15.19: "a slow spy costs
    // itself data, never its neighbors") — the pty pump→writer bridge sheds with
    // `dropped_slow_consumer` rather than blocking, so at the 32-chunk default depth
    // (`default_pty_hostward_buffer`) a descheduled drain client legally loses part of
    // the burst and this test fails on a daemon that did nothing wrong. That is the
    // same 256 KiB-through-a-default-console shape `p3_log` measured at 14/40 failures
    // under sustained CPU load, 0/40 at 8192, with `received + dropped_slow_consumer ==
    // 262144` to the byte in every failure — loss that was located and counted, not
    // loss that escaped. Raising the *serial* node's depth does not help: the pty pump
    // drops rather than awaits, so it never backpressures upstream and the pty node's
    // own depth is the only buffer in the path.
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

// ---- Checks 4 & 5: the signal verbs' own refusals (§7.1) -------------------------

/// Boot a daemon owning one free-for-all serial node (`usb0`) on `device`, active.
/// The minimum graph the signal verbs need: no edge, no consumer, just a live port.
fn signal_daemon(device: &Path) -> Daemon {
    let d = Daemon::start();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
"#,
        dev = device.display(),
    );
    d.rpc().load_toml(&cfg, false).expect("load signal config");
    assert!(
        d.rpc()
            .wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        d.rpc().node("usb0")
    );
    d
}

/// **37-SER-1.** Two line-holding verbs cannot overlap on one port.
///
/// Both verbs' restore guards gate on the port *generation*, which is unchanged
/// while the port is simply open — so a `send-break --ms 3000` overlapped by a
/// `send-break --ms 20` had the short verb's guard clear the break at 20 ms while the
/// long one went on reporting success for its full 3 s. The overlap needs two control
/// connections, which costs nothing: the one-waiting-verb rule is per connection
/// (§10), and the design says outright that a client wanting concurrency opens a
/// second one.
///
/// A physical line has one state, so the slot is per port; the refusal names the rule
/// rather than queueing, because the caller asked for a *bounded* assertion and a
/// queued one would be a different duration than the one it was told succeeded.
#[test]
fn a_second_line_holding_verb_is_refused_while_one_is_in_flight() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_second_line_holding_verb_is_refused_while_one_is_in_flight: \
             no serial device on this platform"
        );
        return;
    };
    let d = signal_daemon(echo.device());
    let rpc = d.rpc();

    // The long break runs on its own connection, on its own thread: `Rpc` is one
    // request per connection and blocks until the reply, so a second connection is
    // exactly how a second operator reaches this endpoint.
    const HOLD_MS: u64 = 3_000;
    let socket = d.socket();
    let long = std::thread::spawn(move || Rpc::new(socket).send_break("usb0", HOLD_MS));

    // Wait until the long verb is observably holding the line — with `pulse-dtr` as
    // the probe, for a reason worth stating: on a pts its `TIOCMSET` fails ENOTTY
    // *after* the slot is claimed and released, and nothing in that dispatch awaits,
    // so the probe never holds the line for a scheduler-visible instant and cannot
    // starve the verb it is waiting for. Probing with `send-break` instead has the
    // probe win the startup race, hold the line for its own 20 ms, and refuse the
    // very verb under test — which is what the first draft of this test did.
    // The probe therefore doubles as the proof that the two verbs share one slot:
    // "already in flight" here is a statement about the interlock, "ENOTTY" about
    // the device, and they are not confusable.
    let mut probe = String::new();
    let holding = wait_until(Duration::from_millis(2_000), || {
        match rpc.call("pulse-dtr", json!({ "node": "usb0", "ms": 20 })) {
            Err(e) => {
                probe = e.message.clone();
                e.message.contains("already in flight")
            }
            Ok(_) => false,
        }
    });
    assert!(
        holding,
        "`pulse-dtr` never met the in-flight refusal while a 3 s `send-break` held the \
         line — the two line-holding verbs do not share a slot; last probe said: {probe}"
    );

    // Same-verb overlap, the shape the finding names: the short break's restore guard
    // passes the generation check and clears the long verb's line at 20 ms, while the
    // long verb goes on reporting success for its full duration.
    let same = rpc
        .send_break("usb0", 20)
        .expect_err("a second `send-break` must be refused while one holds the line");
    assert!(
        same.message.contains("already in flight"),
        "the refusal must name the rule it enforces, got: {}",
        same.message
    );

    // The long verb ran its full duration and says so — nobody cleared its line.
    let held = long
        .join()
        .expect("the long send-break thread panicked")
        .expect("the long send-break must succeed");
    assert_eq!(
        held["break_ms"],
        json!(HOLD_MS),
        "the long verb did not report its own duration: {held}"
    );

    // The slot is released with the verb, not leaked: the next one is accepted.
    rpc.send_break("usb0", 20)
        .expect("the line slot is free once the holding verb returns");
    assert_eq!(
        rpc.node_status("usb0"),
        "active",
        "the overlap disturbed the node: {:?}",
        rpc.node("usb0")
    );
}

/// **37-SER-2.** A present-but-mistyped `ms`/`assert` is named, never replaced by the
/// verb's default.
///
/// `as_u64`/`as_bool` answer `None` for a mistyped value exactly as they do for an
/// absent key, so `"ms": "70000"` ran the range check against the *substituted*
/// default and reported nothing wrong, and `"assert": "false"` (a string, which is
/// truthy nowhere) drove DTR in the direction opposite to what the caller wrote.
/// §11's "a typo cannot silently become a default" rule, applied to the RPC surface.
///
/// JSON `null` is the control: `serial-nexus-ctl` builds its params from `Option`
/// fields and sends every optional key unconditionally, so `null` must keep reading
/// as "unset" — a refusal there would refuse the shipped client's ordinary requests.
#[test]
fn a_mistyped_signal_param_is_named_rather_than_silently_defaulted() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_mistyped_signal_param_is_named_rather_than_silently_defaulted: \
             no serial device on this platform"
        );
        return;
    };
    let d = signal_daemon(echo.device());
    let rpc = d.rpc();

    // (verb, params, the field the refusal must name)
    let cases: &[(&str, Value, &str)] = &[
        // The exact value the review names: an operator's shell quoted the number, so
        // the 70 s the daemon would have refused became the 250 ms default.
        ("send-break", json!({ "node": "usb0", "ms": "70000" }), "ms"),
        ("send-break", json!({ "node": "usb0", "ms": -5 }), "ms"),
        ("send-break", json!({ "node": "usb0", "ms": 1.5 }), "ms"),
        ("pulse-dtr", json!({ "node": "usb0", "ms": "20" }), "ms"),
        // The sharper half: this one does not merely lose the caller's number, it
        // inverts the caller's intent.
        (
            "pulse-dtr",
            json!({ "node": "usb0", "ms": 20, "assert": "false" }),
            "assert",
        ),
    ];
    for (verb, params, field) in cases {
        let Err(err) = rpc.call(verb, params.clone()) else {
            panic!("`{verb}` {params} ran on the substituted default instead of refusing");
        };
        assert_eq!(
            err.code, -32602,
            "`{verb}` {params} must be invalid-params, got [{}] {}",
            err.code, err.message
        );
        assert!(
            err.message.contains(&format!("'{field}'")),
            "the refusal must name the field, got: {}",
            err.message
        );
    }

    // `null` still means "unset": the verb runs on its default.
    let ok = rpc
        .call("send-break", json!({ "node": "usb0", "ms": null }))
        .expect("an explicit null `ms` means unset, as every Option-built client sends");
    assert_eq!(
        ok["break_ms"],
        json!(250),
        "a null `ms` did not fall back to the verb default: {ok}"
    );

    assert_eq!(
        rpc.node_status("usb0"),
        "active",
        "a refused param disturbed the node: {:?}",
        rpc.node("usb0")
    );
}
