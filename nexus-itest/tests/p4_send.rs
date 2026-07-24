//! Phase 4 arbitration (design §6), ported from `scripts/validate/phase4/send.sh`.
//!
//! The atomic `send` verb: `send` names the ENDPOINT, and the CLI is a transient
//! origin that acquires the write lock (with a timeout), writes one line, and
//! releases — one daemon-side operation. While another origin holds the lock, a
//! plain `send` fails with the `locked` error at its deadline and delivers nothing;
//! `send --steal` takes the lock and delivers the line **exactly once**.
//!
//! Ground truth for "delivered exactly once" is a byte-exact SHA-256 of the bytes
//! that reached the far end, never a judgement (§5). Where the bash test used a
//! `nexus-sim pty --sink` as the serial "device", this opens `usb0` on port A of a
//! cross-wired [`serial_pair`] (Linux sim null-modem / macOS hardware) and reads the
//! stolen line off port B with a sim sink **sized to exactly that line** — so a stray
//! leak from the denied plain `send` would corrupt the count or checksum. It
//! self-skips when no serial device is available (the same self-skip discipline the
//! bash hardware rig used).

use std::time::Duration;

use nexus_itest::{Daemon, Sim, serial_pair, sha256_hex, wait_until};
use serde_json::{Value, json};

/// `AppError::Locked` (nexus-rpc): `APP_ERROR_BASE (-32000) - 3`. A contended,
/// un-waited `lock`/`send` is refused with this code (§6/§16.8).
const LOCKED_CODE: i64 = -32003;

/// The `.lock` snapshot object for `node` from `state` (§6), or panic — a missing
/// lock means the host endpoint was never wired, which is a harness bug.
fn lock_of(rpc: &nexus_itest::Rpc, node: &str) -> Value {
    rpc.node(node)
        .unwrap_or_else(|| panic!("node {node} absent from state"))
        .get("lock")
        .cloned()
        .unwrap_or_else(|| panic!("node {node} reports no .lock endpoint"))
}

/// The `origin` labels registered on a lock snapshot (§6), sorted for a stable
/// compare.
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

#[test]
fn send_is_atomic_locked_denies_then_steal_delivers_line_exactly_once() {
    let Some(pair) = serial_pair() else {
        eprintln!(
            "SKIP send_is_atomic_locked_denies_then_steal_delivers_line_exactly_once: \
             no serial device on this platform \
             (attach a crossover rig, or run on Linux for the sim null-modem)"
        );
        return;
    };
    let (port_a, port_b) = pair.ports();

    // The line the steal delivers; `send` appends '\n', so the wire form is
    // "steal-me\n" (9 bytes) — the sink is sized to exactly this.
    const LINE: &str = "steal-me";
    let explen: u64 = (LINE.len() + 1) as u64;
    let exp_sha = sha256_hex(format!("{LINE}\n").as_bytes());

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let ptya_path = run.join("ttyA").to_string_lossy().into_owned();

    // One on-demand PTY holder (`ptya`) plus the CLI `send` origin fan into one
    // exclusive serial endpoint (`usb0`, opening port A). Default arbitration is
    // exclusive (§6, invariant 8). A matching standard baud makes the byte-exact
    // read correct on real hardware; it is cosmetic on the sim null-modem (§7.2).
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "ptya"
path = "{ptya}"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
baud = 115200
[[edge]]
a = "usb0"
b = "ptya"
"#,
        ptya = ptya_path,
        device = port_a,
    );
    rpc.load_toml(&cfg, false).expect("load fan-in config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // ptya grabs the lock, so the endpoint is held by another origin (§6).
    let acq = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        acq["acquired"],
        json!(true),
        "lock ptya not acquired: {acq:?}"
    );

    // A sink on the far end (port B), sized to exactly the stolen line, records what
    // reaches "hardware". Start it draining before the steal so the crossover buffer
    // never overflows and (on hardware) the receiving UART is open when bytes arrive.
    // A leak from the denied plain `send` below would land here first and corrupt the
    // count/checksum — exactly what "delivered exactly once" must exclude.
    let port_b_owned = port_b.to_string();
    let explen_arg = explen.to_string();
    let sink = std::thread::spawn(move || {
        Sim::client(&[
            "--path",
            port_b_owned.as_str(),
            "--recv",
            explen_arg.as_str(),
            "--set-baud",
            "115200",
            "--timeout-ms",
            "20000",
        ])
    });

    // A plain `send` joins the queue with its deadline and fails with the `locked`
    // error when the deadline elapses (§6); nothing is delivered. The ~400 ms it
    // parks also gives the sink time to open port B before the steal.
    let err = rpc
        .send("usb0", "should-not-arrive", false, 400)
        .expect_err("plain send must fail while ptya holds the lock");
    assert_eq!(
        err.code, LOCKED_CODE,
        "plain send refused with {} ({}), want LOCKED {LOCKED_CODE}",
        err.code, err.message
    );
    assert!(
        err.message.to_lowercase().contains("lock"),
        "send failed, but not with a locked error: {:?}",
        err.message
    );

    // The queue is intact after the deadline: the transient origin was cleaned up
    // (waiters empty) and ptya still holds the lock (§6).
    let lock = lock_of(rpc, "usb0");
    assert!(
        lock["waiters"]
            .as_array()
            .expect("waiters is an array")
            .is_empty(),
        "waiter queue not empty after a timed-out send: {:?}",
        lock["waiters"]
    );
    assert_eq!(
        lock["holder"],
        json!("ptya"),
        "ptya should still hold the lock after the failed send",
    );

    // `send --steal` takes the lock and delivers the line exactly once (§6). The
    // timeout is irrelevant to a steal (it takes the lock immediately).
    let res = rpc.send("usb0", LINE, true, 5000).expect("send --steal");
    assert_eq!(
        res["delivered"],
        json!(true),
        "send --steal did not report delivery: {res:?}"
    );
    assert_eq!(
        res["sent"],
        json!(explen),
        "send --steal reported wrong byte count: {res:?}"
    );

    // The device received exactly the stolen line, once — byte-exact ground truth
    // (§5). A wrong count means "delivered != once"; a wrong checksum means the wrong
    // bytes reached the wire (e.g. a leak from the denied plain send).
    let verdict = sink.join().expect("sink thread");
    assert_eq!(
        verdict["received"],
        json!(explen),
        "device received {:?} bytes, expected {explen} (delivered != once): {verdict:?}",
        verdict["received"]
    );
    assert_eq!(
        verdict["sha256"].as_str(),
        Some(exp_sha.as_str()),
        "device checksum != the stolen line (wrong bytes delivered): {verdict:?}"
    );
    assert_eq!(
        verdict["pass"],
        json!(true),
        "sink did not pass (short/over-read): {verdict:?}"
    );

    // After the atomic send, the transient origin released and unregistered: the
    // holder is clear and no phantom "send" origin lingers — only ptya remains (§6).
    let released = wait_until(Duration::from_secs(3), || {
        lock_of(rpc, "usb0")["holder"].is_null()
    });
    assert!(
        released,
        "send did not release the lock: holder = {:?}",
        lock_of(rpc, "usb0")["holder"]
    );
    assert_eq!(
        origin_labels(&lock_of(rpc, "usb0")),
        vec!["ptya"],
        "a transient send origin lingered on the endpoint",
    );
}
