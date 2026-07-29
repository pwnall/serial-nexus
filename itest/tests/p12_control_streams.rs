//! The control connection keeps streaming while one of its verbs is parked
//! (design §10 "a tap streams its hostward bytes as id-less `tap.data` notifications
//! on that connection"; §17 one daemon connection per browser; review **CTRLW-1**).
//!
//! `serve_connection` used to leave its outer `select!` and enter an inner two-arm
//! loop the moment a verb was not ready on its first poll — `lock --wait`, a
//! contended `send`, anything. That inner loop polled only the dispatch future and
//! the next request line, so for the whole duration of the wait the connection wrote
//! **no** notifications: `subscribe` traffic merely late, but `tap.data` bytes really
//! lost past the 128-slot per-connection tap queue. For the §17 console — one daemon
//! connection carrying `subscribe`, every tap and every `send` — that is a full
//! terminal blackout for as long as the wait lasts, which on an ordinary contended
//! `send` is `DEFAULT_SEND_TIMEOUT_MS` = 2 s and on a `lock --wait` is unbounded.
//!
//! Both tests here open **one** connection, `subscribe` + `tap.open` a live endpoint
//! on it, park a verb on that same connection, and then measure the `tap.data` bytes
//! that arrive *while it is parked*. They assert on byte counts drawn from the
//! notifications' own base64 payloads, never on text (§5).
//!
//! **Fail-first, observed.** Both tests were run against the *unfixed* daemon — a
//! throwaway `git worktree` at `5a92ca5` with its own target dir, so the shared tree
//! never moved (AGENTS §8) — and both failed with **zero** `tap.data` and zero other
//! notifications in the window, then passed with only `control.rs` replaced. That is
//! the structural prediction: the old inner loop (`control.rs:300-355`) has no
//! `notes.recv()` and no `tap_rx.recv()` arm at all, so no notification line *can* be
//! written between the moment a verb parks and the moment it settles, and the review's
//! verifier measured the same hole from the other side — 11,188,225 bytes charged to
//! the tap's `dropped` counter during one 6 s `lock --wait`. The [`DISCARD`] phase
//! below is what makes the zero honest: bytes the daemon had already written into the
//! socket *before* the verb parked are drained and thrown away, so they cannot be
//! mistaken for bytes delivered during the wait.
//!
//! **Linux only**: the live endpoint is a `serial` node over a `serial-nexus-sim` pty double,
//! and a pts cannot be a serial device on macOS (`serial2` → `ENOTTY`, §7), so these
//! self-skip there.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, Sim, wait_until};

/// Bytes/second the sim paces the device at. High enough that a measurement window
/// of [`WINDOW`] carries hundreds of KiB through several `tap.data` notifications,
/// low enough to stay polite on a shared, loaded box.
const SOURCE_RATE: &str = "262144";

/// Total payload the sim emits — far more than any test here consumes, so the device
/// never falls silent mid-measurement. The sim is killed on `Drop`.
const SOURCE_BYTES: &str = "32MiB";

/// How long to read and *discard* after a verb is confirmed parked, before the real
/// measurement starts. Drains anything the daemon wrote into the socket before the
/// park, so the window that follows contains only bytes produced during the wait.
const DISCARD: Duration = Duration::from_millis(500);

/// The measurement window: how long the verb is observed to be parked while the tap
/// is expected to keep flowing.
const WINDOW: Duration = Duration::from_millis(1500);

/// Exact decoded length of one padded standard-base64 `tap.data` payload
/// (`serial_nexus_rpc::base64_encode` always pads). Counting bytes rather than decoding them
/// keeps the assertion on the quantity the design actually promises — delivery — with
/// no byte-exactness claim at the lossy console boundary (§5).
fn base64_len(s: &str) -> u64 {
    let n = s.len() as u64;
    if n < 4 {
        return 0;
    }
    let pad = s.bytes().rev().take_while(|&b| b == b'=').count() as u64;
    n / 4 * 3 - pad
}

/// A raw control connection that can hold a request in flight while reading whatever
/// else the daemon sends — what the harness's one-request-per-connection [`Rpc`]
/// deliberately cannot model, and what a §17 browser's single daemon connection is.
/// Deliberately local to this file (`p9_control_pipelining.rs` carries its own): the
/// shared harness stays out of the streaming-plus-pipelining business.
struct Conn {
    stream: UnixStream,
    buf: Vec<u8>,
}

/// What one drain of a connection saw.
#[derive(Default)]
struct Drained {
    /// Decoded `tap.data` bytes delivered in the window.
    tap_bytes: u64,
    /// How many separate `tap.data` notifications carried them.
    tap_messages: u32,
    /// Any other id-less notification (the `subscribe` lane) seen in the window.
    other_notes: u32,
    /// The response to the awaited id, if it settled during the window.
    response: Option<Value>,
}

impl Conn {
    fn open(socket: &Path) -> Conn {
        let stream = UnixStream::connect(socket).expect("connect control socket");
        Conn {
            stream,
            buf: Vec::new(),
        }
    }

    /// Write one request line. Never waits for its reply — the point of the file.
    fn request(&mut self, id: i64, method: &str, params: Value) {
        let mut req = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if !params.is_null() {
            req["params"] = params;
        }
        self.stream
            .write_all(format!("{req}\n").as_bytes())
            .expect("write request line");
        self.stream.flush().expect("flush request line");
    }

    /// Read one `\n`-terminated line by `deadline`, buffering across reads, or `None`
    /// on timeout / close.
    fn read_line(&mut self, deadline: Instant) -> Option<String> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                return Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            self.stream.set_read_timeout(Some(deadline - now)).ok();
            let mut tmp = [0u8; 65536];
            match self.stream.read(&mut tmp) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(_) => return None, // WouldBlock/TimedOut/closed
            }
        }
    }

    /// The response carrying `id`, skipping notifications, or `None` if it never
    /// arrives within `timeout`.
    fn response(&mut self, id: i64, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let line = self.read_line(deadline)?;
            let v: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON ({e}): {line}"));
            if v.get("id").and_then(Value::as_i64) == Some(id) {
                return Some(v);
            }
        }
    }

    /// Read everything the daemon sends for `window`, tallying `tap.data` bytes and
    /// watching for the response to `awaited`. Reads to the end of the window even
    /// after the response arrives, so a test can tell "still parked" from "settled".
    fn drain_for(&mut self, window: Duration, awaited: i64) -> Drained {
        let deadline = Instant::now() + window;
        let mut seen = Drained::default();
        while let Some(line) = self.read_line(deadline) {
            let v: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON ({e}): {line}"));
            match v.get("method").and_then(Value::as_str) {
                Some("tap.data") => {
                    let data = v
                        .pointer("/params/data")
                        .and_then(Value::as_str)
                        .expect("tap.data carries base64 `data`");
                    seen.tap_bytes += base64_len(data);
                    seen.tap_messages += 1;
                }
                Some(_) => seen.other_notes += 1,
                None => {
                    if v.get("id").and_then(Value::as_i64) == Some(awaited) {
                        seen.response = Some(v);
                    }
                }
            }
        }
        seen
    }
}

/// The FIFO waiter queue of `node`'s endpoint lock, front first.
fn waiters(rpc: &Rpc, node: &str) -> Vec<String> {
    rpc.node(node)
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("waiters").cloned())
        .and_then(|w| w.as_array().cloned())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A daemon plus the sim device feeding it. Field order is the drop order: the
/// daemon goes down (releasing the pts) before the sim that owns the master.
struct Rig {
    daemon: Daemon,
    _source: Sim,
}

/// One live host-facing endpoint (`usb0`, fed by a paced sim source) fanned out to two
/// pty consoles, so the endpoint's write lock has two origins to contend over. Returns
/// `None` off Linux, where a pts cannot be a serial device (§7) — the caller skips.
fn rig() -> Option<Rig> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let daemon = Daemon::start();
    let dev = daemon.run().join("dev");
    let dev_str = dev.to_string_lossy().into_owned();
    let source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            SOURCE_BYTES,
            "--rate",
            SOURCE_RATE,
            "--link",
            &dev_str,
            "--hold-ms",
            "600000",
            "--timeout-ms",
            "600000",
        ],
        Some(&dev),
    );

    // The two pty consoles have nobody attached to their slave side, so their hostward
    // bridges shed (§5/§15.19: a slow spy costs itself data, never its neighbors) —
    // which is exactly why nothing here asserts byte-exactness through them. The
    // explicit `hostward_buffer` keeps that shedding away from the tap's own timing.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
hostward_buffer = 8192
[[node]]
type = "pty"
name = "ptya"
path = "{ta}"
hostward_buffer = 8192
[[node]]
type = "pty"
name = "ptyb"
path = "{tb}"
hostward_buffer = 8192
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
"#,
        dev = dev.display(),
        ta = daemon.run().join("ttyA").display(),
        tb = daemon.run().join("ttyB").display(),
    );
    let rpc = daemon.rpc();
    rpc.load_toml(&cfg, false).expect("load the graph");
    for node in ["usb0", "ptya", "ptyb"] {
        assert!(
            rpc.wait_status(node, "active", Duration::from_secs(10)),
            "{node} never came up active: {:?}",
            rpc.node(node)
        );
    }
    Some(Rig {
        daemon,
        _source: source,
    })
}

/// Open one connection, `subscribe` on it, and `tap.open` the live `usb0` endpoint —
/// the §17 console's connection in miniature. Returns it plus the `tap.data` byte
/// count observed over a [`WINDOW`]-long baseline, which proves the stream is live
/// before anything is parked and gives the later windows something to be compared to.
fn subscribed_tap(rig: &Rig) -> (Conn, u64) {
    let mut conn = Conn::open(&rig.daemon.socket());
    conn.request(1, "subscribe", Value::Null);
    assert!(
        conn.response(1, Duration::from_secs(5))
            .and_then(|r| r.get("result").cloned())
            .is_some(),
        "subscribe was not acknowledged"
    );
    conn.request(2, "tap.open", json!({ "endpoint": "usb0" }));
    let opened = conn
        .response(2, Duration::from_secs(5))
        .expect("tap.open was not answered");
    assert!(
        opened.get("result").is_some(),
        "tap.open on the live endpoint failed: {opened}"
    );

    // Baseline: nothing is parked, so this is the ordinary streaming rate. `awaited`
    // is an id no request uses.
    let base = conn.drain_for(WINDOW, -1);
    assert!(
        base.tap_bytes > 0,
        "the endpoint delivered no tap.data even with nothing parked — the fixture, \
         not the daemon, is broken (messages={}, other notes={})",
        base.tap_messages,
        base.other_notes
    );
    (conn, base.tap_bytes)
}

/// Assert that `seen` shows a live stream during the window, against `baseline`.
/// The floor is a quarter of the baseline rather than equality: the sim paces the
/// device, but a loaded runner can still shift a block or two across a window edge.
fn assert_still_streaming(seen: &Drained, baseline: u64, what: &str) {
    assert!(
        seen.tap_bytes > 0,
        "no tap.data arrived during the {what} — the connection's notification lanes \
         were frozen for the whole wait (CTRLW-1). other notifications={}",
        seen.other_notes
    );
    assert!(
        seen.tap_bytes * 4 >= baseline,
        "the tap stream nearly stopped during the {what}: {} bytes in {} messages \
         against a {baseline}-byte baseline over the same window",
        seen.tap_bytes,
        seen.tap_messages,
    );
}

// ============================================================================
// CTRLW-1 — a parked `lock --wait` must not black out the connection's tap.
// ============================================================================
#[test]
fn tap_data_keeps_arriving_while_a_lock_wait_is_parked_on_the_same_connection() {
    let Some(rig) = rig() else {
        eprintln!("SKIP: a pts cannot be a serial device off Linux (§7)");
        return;
    };
    let rpc = rig.daemon.rpc();
    let (mut conn, baseline) = subscribed_tap(&rig);

    // Another origin takes the write lock (from its own connection), so the console's
    // own `lock --wait` parks in the FIFO queue.
    let held = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        held.get("acquired").and_then(Value::as_bool),
        Some(true),
        "ptya did not acquire the lock: {held}"
    );

    conn.request(
        3,
        "lock",
        json!({ "origin": "ptyb", "steal": false, "wait": true, "lease_ms": null }),
    );
    assert!(
        wait_until(Duration::from_secs(5), || waiters(rpc, "usb0")
            == vec!["ptyb".to_string()]),
        "ptyb never parked in the FIFO queue (waiters={:?})",
        waiters(rpc, "usb0")
    );

    // Discard whatever the daemon wrote before the verb parked, then measure.
    let _ = conn.drain_for(DISCARD, 3);
    let seen = conn.drain_for(WINDOW, 3);
    assert!(
        seen.response.is_none(),
        "the `lock --wait` settled during the measurement window, so nothing was \
         actually observed under a parked verb: {:?}",
        seen.response
    );
    assert_still_streaming(&seen, baseline, "parked `lock --wait`");

    // And the parked verb is unharmed by all that streaming: it still resolves on the
    // same connection when the holder releases.
    rpc.unlock("ptya").expect("unlock ptya");
    let grant = conn
        .response(3, Duration::from_secs(10))
        .expect("the parked `lock --wait` never answered after the unlock");
    assert_eq!(
        grant.pointer("/result/acquired").and_then(Value::as_bool),
        Some(true),
        "the parked wait did not resolve into a grant: {grant}"
    );
    rpc.unlock("ptyb").expect("unlock ptyb");
}

// ============================================================================
// CTRLW-1, the shape the shipped console actually hits: the parked verb is a
// contended `send` — every keystroke into a console whose endpoint another origin
// holds. The verifier's correction to the finding is that the freeze was never
// specific to `lock --wait`; *any* dispatch future pending on its first poll took
// the notification lanes down with it, and `send` is the one a §17 browser issues
// on every Enter.
// ============================================================================
#[test]
fn tap_data_keeps_arriving_while_a_contended_send_is_parked_on_the_same_connection() {
    let Some(rig) = rig() else {
        eprintln!("SKIP: a pts cannot be a serial device off Linux (§7)");
        return;
    };
    let rpc = rig.daemon.rpc();
    let (mut conn, baseline) = subscribed_tap(&rig);

    rpc.lock("ptya", false, false, None).expect("lock ptya");

    // `send` self-acquires the write lock (§6/invariant 8), so with ptya holding it
    // parks. The deadline is generous only so the wait certainly outlives the
    // measurement on a loaded runner; the shipped console's default is 2 s, which is
    // the length of the blackout this pins.
    conn.request(
        3,
        "send",
        json!({ "endpoint": "usb0", "line": "hello", "timeout_ms": 20000, "steal": false }),
    );
    assert!(
        wait_until(Duration::from_secs(5), || waiters(rpc, "usb0")
            == vec!["send".to_string()]),
        "the contended send never parked (waiters={:?})",
        waiters(rpc, "usb0")
    );

    let _ = conn.drain_for(DISCARD, 3);
    let seen = conn.drain_for(WINDOW, 3);
    assert!(
        seen.response.is_none(),
        "the `send` settled during the measurement window, so nothing was actually \
         observed under a parked verb: {:?}",
        seen.response
    );
    assert_still_streaming(&seen, baseline, "parked contended `send`");

    // The send still completes once the holder releases, and the connection is still a
    // working control connection afterwards.
    rpc.unlock("ptya").expect("unlock ptya");
    let sent = conn
        .response(3, Duration::from_secs(10))
        .expect("the parked `send` never answered after the unlock");
    assert!(
        sent.get("result").is_some(),
        "the parked send did not complete: {sent}"
    );
    conn.request(4, "state", Value::Null);
    let after = conn
        .response(4, Duration::from_secs(5))
        .expect("the connection died after the wait resolved");
    assert!(
        after.get("result").is_some(),
        "a post-wait request was not served: {after}"
    );
}
