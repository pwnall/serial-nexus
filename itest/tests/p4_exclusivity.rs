//! Phase 4 arbitration, ported from `scripts/validate/phase4/exclusivity.sh`
//! (design §6): the per-endpoint exclusive write lock. Two on-demand PTYs and a
//! `write=never` spy fan into one serial endpoint (a legal §4 fan-out). Only the
//! lock holder's bytes are read targetward; a non-holder is paused (its bytes
//! buffer, never fire) and a spy cannot contend at all.
//!
//! The bash test proved exclusivity with a byte-exact checksum against a
//! `serial-nexus-sim pty --sink` "device". Two things are split here:
//!
//! * **Arbitration state, contention, and detach-release** are a property of the
//!   host endpoint's lock, registered at wiring time regardless of whether the
//!   serial device ever opened (`daemon/src/runtime.rs` registers each edge
//!   as an origin before the node starts). So [`exclusive_lock_arbitration_and_detach_release`]
//!   uses an *absent* serial device and runs on every platform — no serial rig.
//! * **Byte-exact exclusivity** needs a real targetward path to observe what
//!   reached "hardware", so [`exclusive_write_lock_is_byte_exact`] takes a
//!   cross-wired [`serial_pair`] (Linux sim null-modem / macOS hardware) and reads
//!   the far end; it self-skips when no serial device is available (§5).
//!
//! Ground truth for the data-plane claim is a byte-exact SHA-256 computed by
//! `serial-nexus-sim` from the same seed on both ends — a match proves the device saw the
//! holder's stream and nothing else, never a judgement.

use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Sim, attach_slave, observed_while_open, serial_pair_or_rig, skip_no_pair,
};

/// `AppError::Locked` (serial-nexus-rpc): `APP_ERROR_BASE (-32000) - 3`. A contended,
/// un-waited `lock`/`send` is refused with this code (§6/§16.8).
const LOCKED_CODE: i64 = -32003;

/// The `.lock` snapshot object for `node` from `state` (§6), or panic — a missing
/// lock means the host endpoint was never wired, which is a harness bug.
fn lock_of(rpc: &serial_nexus_itest::Rpc, node: &str) -> Value {
    rpc.node(node)
        .unwrap_or_else(|| panic!("node {node} absent from state"))
        .get("lock")
        .cloned()
        .unwrap_or_else(|| panic!("node {node} reports no .lock endpoint"))
}

/// The sorted `origin` labels of a lock snapshot.
fn origin_labels(lock: &Value) -> Vec<String> {
    let mut v: Vec<String> = lock["origins"]
        .as_array()
        .expect("lock.origins is an array")
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect();
    v.sort();
    v
}

/// One origin object from a lock snapshot, by its label.
fn origin<'a>(lock: &'a Value, name: &str) -> &'a Value {
    lock["origins"]
        .as_array()
        .expect("lock.origins is an array")
        .iter()
        .find(|o| o.get("origin").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("origin {name} not present in lock"))
}

/// The graph: two on-demand PTYs and a `write=never` spy fan into `usb0` (§6). The
/// spy's edge is explicitly `write=never`. `device` is whatever serial path the
/// caller supplies (absent for the arbitration test, a live port for the data one).
fn fan_in_config(ta: &str, tb: &str, ts: &str, device: &str, baud: u32) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "ptya"
path = "{ta}"
[[node]]
type = "pty"
name = "ptyb"
path = "{tb}"
[[node]]
type = "pty"
name = "spy"
path = "{ts}"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
baud = {baud}
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
[[edge]]
a = "usb0"
b = "spy"
write_mode = "never"
"#
    )
}

/// Arbitration, contention, and detach-release (§6) — no serial device needed, so
/// this runs on every platform (the lock is a property of the host endpoint's
/// wiring, not of the device being open). Ports the state/lock/contention/
/// detach-release assertions of `exclusivity.sh`.
#[test]
fn exclusive_lock_arbitration_and_detach_release() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let ta = run.join("ttyA").to_string_lossy().into_owned();
    let tb = run.join("ttyB").to_string_lossy().into_owned();
    let ts = run.join("ttyS").to_string_lossy().into_owned();
    // An absent device: usb0 comes up `waiting`, but its endpoint lock and the
    // three fan-in origins are still registered (§6).
    let device = run.join("absent-usb0").to_string_lossy().into_owned();

    rpc.load_toml(&fan_in_config(&ta, &tb, &ts, &device, 115200), false)
        .expect("load fan-in config");

    // The three origins register at wiring time — wait for them to appear, then
    // assert the endpoint reports its lock: exclusive by default, no holder, three
    // origins with the right write modes (§6).
    let ready = serial_nexus_itest::wait_until(Duration::from_secs(5), || {
        origin_labels(&lock_of(rpc, "usb0")).len() == 3
    });
    assert!(ready, "the three fan-in origins never registered on usb0");

    let lock = lock_of(rpc, "usb0");
    assert_eq!(
        lock["arbitration"],
        json!("exclusive"),
        "arbitration should default to exclusive, got {lock:?}"
    );
    assert_eq!(
        lock["holder"],
        Value::Null,
        "no origin should hold the lock at start, got {:?}",
        lock["holder"]
    );
    assert_eq!(
        origin_labels(&lock),
        vec!["ptya", "ptyb", "spy"],
        "lock origins wrong"
    );
    assert_eq!(
        origin(&lock, "spy")["write_mode"],
        json!("never"),
        "spy edge should be write=never"
    );

    // Grab the lock for the holder: acquired and held (§6).
    let acq = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        acq["acquired"],
        json!(true),
        "lock ptya not acquired: {acq:?}"
    );
    assert_eq!(acq["held"], json!(true), "lock ptya not held: {acq:?}");

    let lock = lock_of(rpc, "usb0");
    assert_eq!(
        lock["holder"],
        json!("ptya"),
        "holder not ptya after acquire"
    );
    assert_eq!(
        origin(&lock, "ptya")["holds_lock"],
        json!(true),
        "ptya not marked holds_lock"
    );

    // A non-holder cannot acquire while ptya holds it (§6): refused fail-fast with
    // the LOCKED application error, its message naming the lock.
    let err = rpc
        .lock("ptyb", false, false, None)
        .expect_err("lock ptyb must be refused while ptya holds it");
    assert_eq!(
        err.code, LOCKED_CODE,
        "lock ptyb refused with {} ({}), want LOCKED {LOCKED_CODE}",
        err.code, err.message
    );
    assert!(
        err.message.to_lowercase().contains("lock"),
        "refusal message should name the lock: {:?}",
        err.message
    );

    // Detach-release (§6): the holder's client attaches ptya's slave, is observed
    // present, then detaches — so the lock releases automatically, no explicit
    // unlock. A brief hold guarantees the daemon's presence poll (5ms idle cadence)
    // observes the present→absent transition that triggers the release.
    let held_before = lock_of(rpc, "usb0")["holder"].clone();
    assert_eq!(
        held_before,
        json!("ptya"),
        "precondition: ptya holds the lock"
    );
    let _ = Sim::client(&[
        "--path",
        ta.as_str(),
        "--hold-ms",
        "500",
        "--timeout-ms",
        "5000",
    ]);
    let released = serial_nexus_itest::wait_until(Duration::from_secs(5), || {
        lock_of(rpc, "usb0")["holder"] == Value::Null
    });
    assert!(
        released,
        "holder not cleared by detach-release after the client detached: {:?}",
        lock_of(rpc, "usb0")["holder"]
    );
}

/// Byte-exact exclusivity (§6): while the holder streams targetward, a paused
/// non-holder's buffered bytes and a spy's stray write must never reach the device.
/// Needs a real targetward path, so it takes a cross-wired [`serial_pair`] and reads
/// the far end — `usb0` opens port A, the holder's bytes cross to port B where a
/// sim sink checksums exactly what reached "hardware". Self-skips with no serial rig.
#[test]
fn exclusive_write_lock_is_byte_exact() {
    let Some(pair) = serial_pair_or_rig() else {
        skip_no_pair("exclusive_write_lock_is_byte_exact");
        return;
    };
    let (port_a, port_b) = pair.ports();

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let ta = run.join("ttyA").to_string_lossy().into_owned();
    let tb = run.join("ttyB").to_string_lossy().into_owned();
    let ts = run.join("ttyS").to_string_lossy().into_owned();

    // usb0 opens port A; whatever it writes targetward crosses the wire to port B.
    rpc.load_toml(&fan_in_config(&ta, &tb, &ts, port_a, 115200), false)
        .expect("load fan-in config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Grab the lock for the holder, then let the locked-out writers attach and HOLD
    // their slaves open: a broken gate would drain their buffered bytes into the
    // device. ptyb is a paused non-holder (bytes must stay buffered); spy is
    // write=never (cannot write at all). Holding open — not send-then-close — is
    // what makes this exercise the lock gate rather than a close race.
    let acq = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        acq["acquired"],
        json!(true),
        "lock ptya not acquired: {acq:?}"
    );

    let mut pb = Sim::spawn(
        &[
            "client",
            "--path",
            tb.as_str(),
            "--send",
            "seeded:512",
            "--seed",
            "7",
            "--hold-ms",
            "20000",
            "--timeout-ms",
            "25000",
        ],
        None,
    );
    let mut ps = Sim::spawn(
        &[
            "client",
            "--path",
            ts.as_str(),
            "--send",
            "seeded:512",
            "--seed",
            "9",
            "--hold-ms",
            "20000",
            "--timeout-ms",
            "25000",
        ],
        None,
    );
    // Wait until the locked-out writer is present (its bytes are then buffered), so
    // the exclusivity check actually exercises the gate.
    let present = serial_nexus_itest::wait_until(Duration::from_secs(5), || {
        rpc.node("ptyb")
            .and_then(|n| n.get("client_present").and_then(Value::as_bool))
            .unwrap_or(false)
    });
    assert!(present, "locked-out writer never became present");

    // A sink on the far end (port B) records exactly what reached "hardware". Start
    // it draining before the holder sends so the crossover buffer never overflows.
    let port_b_owned = port_b.to_string();
    let sink = std::thread::spawn(move || {
        Sim::client(&[
            "--path",
            port_b_owned.as_str(),
            "--recv",
            "65536",
            "--set-baud",
            "115200",
            "--timeout-ms",
            "30000",
        ])
    });

    // The holder's pty session, opened by the harness and held across the sink's
    // count — the §3.29 witness (notes §3.56, plan §3 rule 8).
    //
    // The one-shot `Sim::client` below returns when the *kernel* accepted its last
    // byte, not when the daemon read it, so its close used to land while up to a pty
    // buffer's worth of the holder's 64 KiB was still inside the pair — and the
    // `received == 65536` assertion below then quietly asserted that the kernel had
    // retained those bytes across the slave's last close. Linux does (doctor P13
    // `retains`); Darwin does not (`waits-then-discards`). This fd makes the sim's exit
    // not be the last close, so the count is taken against a live session.
    let mut holder_session = attach_slave(std::path::Path::new(ta.as_str()));

    // The holder streams 64 KiB; only its bytes may flow to the device.
    let holder = Sim::client(&[
        "--path",
        ta.as_str(),
        "--send",
        "seeded:65536",
        "--seed",
        "42",
        "--timeout-ms",
        "30000",
    ]);
    let sha_holder = holder["sha256_sent"]
        .as_str()
        .expect("holder reported sha256_sent")
        .to_owned();

    // The sink's verdict is collected while three things are still open, and all three
    // are checked at the instant of the reading rather than assumed:
    //
    //  * `holder_session`, for the reason above;
    //  * `pb` and `ps`, the locked-out writers — **and that check is not decorative.**
    //    They are held by `--hold-ms 20000` against a `--timeout-ms 25000`, which is a
    //    timer, i.e. §9's proxy in time. If either expired before the holder finished
    //    streaming, its buffered bytes would be purged at its own detach and this
    //    test's whole claim — "a non-holder's buffered bytes never reach the device" —
    //    would pass because there was nothing left to leak. That is a vacuous pass no
    //    counter in the graph can distinguish from a real one, and it gets louder the
    //    slower the box. `Sim`'s witness answers `try_wait`, so an expired hold is now
    //    a named failure. (The timer itself stays: the sim's slave is held by a
    //    subprocess this harness cannot leash without a sim-side stdin-EOF hold, which
    //    is out of scope here and recorded in notes §3.56.)
    let sink = observed_while_open(
        &mut [&mut holder_session, &mut pb, &mut ps],
        "the device's byte count under an exclusive lock",
        || sink.join().expect("sink thread"),
    );
    let recv = sink["received"].as_u64().unwrap_or(0);
    let sha_sink = sink["sha256"].as_str().unwrap_or("");

    // Release the locked-out writers and end the holder's session, in that order. The
    // holder's close is the detach-release trigger asserted below, and it must be the
    // *last* close on ttyA or the daemon never sees a present→absent edge.
    drop(pb);
    drop(ps);
    drop(holder_session);

    // Byte-exact exclusivity: the device received exactly the holder's 64 KiB — a
    // non-holder or the spy would have interleaved bytes and changed the checksum.
    // Asserted below the closes, from values read above them (§3.29's ordering: the
    // observation is early, the message is late so the lifecycle failure reports
    // first).
    assert_eq!(
        recv, 65536,
        "device received {recv} bytes, expected 65536 (a non-holder leaked?): {sink:?}"
    );
    assert_eq!(
        sha_sink,
        sha_holder.as_str(),
        "device checksum != seeded-A (exclusivity broken)"
    );

    // Detach-release (§6): the holder's client sent and detached, so the lock frees
    // automatically — no explicit unlock.
    let released = serial_nexus_itest::wait_until(Duration::from_secs(10), || {
        lock_of(rpc, "usb0")["holder"] == Value::Null
    });
    assert!(
        released,
        "holder not cleared by detach-release after the client detached: {:?}",
        lock_of(rpc, "usb0")["holder"]
    );
}
