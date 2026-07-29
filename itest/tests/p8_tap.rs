//! Phase 8 tap slice, ported from `scripts/validate/phase8/tap.sh` (design §5 / §17,
//! the web-console track). A **tap** is a connection-scoped, read-only dynamic
//! attachment on a host-facing endpoint — the `never` write mode in dynamic form.
//! On a no-hardware rig this asserts three properties:
//!
//! 1. **Faithful mirror** — the tap's hostward bytes are byte-exact (SHA-256) equal to
//!    both a co-attached `log` consumer of the same endpoint AND the seeded source that
//!    produced them; the tap neither drops nor corrupts a byte (§5).
//! 2. **Taps are state, never config** — `dump` is byte-identical while a tap is open,
//!    so a viewer's tap cannot leak into the operator-owned graph (§8/§11).
//! 3. **Detach on connection drop** — closing the tap's control connection detaches it
//!    (`state` shows zero taps), promptly even on an idle endpoint (§17).
//!
//! Deviations from the bash, each preserving the original *assertions*:
//! * `stat -c %s` → [`file_len`]; `jq` over `serial-nexus-ctl` text → structured RPC on
//!   `dump`/`state`; `sha256sum` → [`sha256_hex`]; `timeout`/`sleep` → bounded
//!   [`wait_until`] and a deadline-bounded collect loop.
//! * The bash read the source's checksum from the sim's stdout verdict. The harness
//!   nulls a sim's stdout, so the source's SHA is recomputed independently from the same
//!   seed ([`seeded_bytes`], the sim's SplitMix64) — a *stronger* ground truth than
//!   trusting the sim's self-report, and it means tap == log == seed are three
//!   independent computations, not one value compared to itself.
//! * The bash tap was a background `serial-nexus-ctl tap` process reading decoded bytes to
//!   a file; Rust opens the tap directly over the control socket (the harness
//!   [`Subscription`] from `tap.open`) and decodes each `tap.data` payload in-process.
//!
//! The first test needs a serial device (a `serial-nexus-sim pty --source` standing in for the
//! UART), so it **skips** where a sim pty cannot be a serial device (macOS) — gated on
//! [`serial_echo`] per the harness doctrine (§5).
//!
//! ## Tap lifecycle (added for the Opus review: TAP-1, T3, T6)
//!
//! The rest of the file covers what happens to a tap when the *graph* moves beneath it,
//! and what `tap.close` does — the web console calls it on every console switch, and a
//! repo-wide grep found it in no test (T3). A tap's hub is dropped by `teardown`,
//! `load --replace` and `remove-node`, while the connection-side handle survives; before
//! the fix the tap simply vanished from `state.taps` and its client sat on a live
//! connection receiving nothing, with no notification and no error (TAP-1 — the
//! daemon-side half of the known OPFS console freeze, T6). The daemon now sends a
//! terminal `tap.closed` naming the endpoint and a reason, and a later `tap.close` on
//! that id fails loudly instead of reporting a success that closed nothing.
//!
//! These are pure control-plane / tap-hub facts: **every** host-facing endpoint gets a
//! hub at load, even while its serial node is `waiting` with an absent device (§15.8),
//! so they need no serial device and run on every platform. Only the "the stream really
//! stops" half needs bytes, and it skips off Linux.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Rpc, Sim, Subscription, file_len, seeded_bytes, serial_echo, sha256_hex, wait_until,
};

const N: usize = 262144; // 256 KiB — well under the tap queue bound, so no drops here.
const SEED: u64 = 7;

/// Standard base64 decode of a `tap.data` payload — `serial_nexus_rpc`'s tested
/// decoder rather than a seventh hand-rolled copy (§16.5 one-rule-one-place, review
/// 37 37-TEST-5). Panics on a malformed payload: these bytes were produced by
/// `serial_nexus_rpc::base64_encode` on the other side of the socket, so anything
/// else is a defect, not an input.
fn base64_decode(s: &str) -> Vec<u8> {
    serial_nexus_rpc::base64_decode(s).expect("a tap.data payload must be valid base64")
}

/// Number of open taps `state` currently reports.
fn tap_count(rpc: &serial_nexus_itest::Rpc) -> usize {
    rpc.state()["taps"].as_array().map(Vec::len).unwrap_or(0)
}

/// Drain `tap.data` notifications from `sub`, decoding and concatenating their payloads
/// until `want` bytes are collected or `timeout` elapses. Ignores any non-`tap.data`
/// line. Bounded — no unbounded wait.
fn collect_tap(sub: &mut Subscription, want: usize, timeout: Duration) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut out = Vec::with_capacity(want);
    while out.len() < want {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match sub.next(deadline - now) {
            Some(v) if v.get("method").and_then(Value::as_str) == Some("tap.data") => {
                if let Some(data) = v
                    .get("params")
                    .and_then(|p| p.get("data"))
                    .and_then(Value::as_str)
                {
                    out.extend_from_slice(&base64_decode(data));
                }
            }
            Some(_) => continue, // a non-tap notification; keep waiting
            None => break,       // timeout / connection closed
        }
    }
    out
}

#[test]
fn tap_mirrors_stream_stays_out_of_config_and_detaches_on_drop() {
    // Needs a sim pty acting as a serial device (Linux); skip on macOS (§5).
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP tap_mirrors_stream_stays_out_of_config_and_detaches_on_drop: \
             no serial device on this platform"
        );
        return;
    };
    drop(probe); // we spawn our own gated source double below

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let log = logdir.join("serial.log");

    // usb0's device: a seeded source gated on GO so its payload cannot outrun a
    // not-yet-draining consumer (plan §3, presence != readiness). --hold-ms keeps the
    // device "present" after the write so the serial node does not see a mid-stream HUP.
    let n_str = N.to_string();
    let seed_str = SEED.to_string();
    let dev_str = dev.to_string_lossy().into_owned();
    let go_str = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &n_str,
            "--seed",
            &seed_str,
            "--wait-file",
            &go_str,
            "--link",
            &dev_str,
            "--hold-ms",
            "2000",
            "--timeout-ms",
            "40000",
        ],
        Some(&dev),
    );

    // serial usb0 → log logx: the log is an always-attached byte-exact co-consumer of
    // the hostward stream; the tap attaches to the same endpoint.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 8192
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
"#,
        dev = dev.display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load tap config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // dump BEFORE any tap: the baseline the open-tap dump must match exactly.
    let dump_pre = rpc.dump();

    // Open the tap on usb0 over the control socket. Holding the `Subscription` keeps the
    // connection open, so the tap stays registered until we drop it.
    let mut tap = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));

    // The tap is active once state lists exactly one tap on usb0.
    assert!(
        wait_until(Duration::from_secs(5), || {
            let taps = rpc.state()["taps"].clone();
            taps.as_array().map(Vec::len) == Some(1) && taps[0]["endpoint"].as_str() == Some("usb0")
        }),
        "tap did not register on usb0 (taps={:?})",
        rpc.state()["taps"]
    );

    // dump WHILE the tap is open must be byte-identical: a tap never touches config (§8).
    let dump_mid = rpc.dump();
    assert_eq!(
        dump_pre, dump_mid,
        "dump changed while a tap was open (a tap leaked into configuration)"
    );

    // Release the source: N seeded bytes flow device → serial → {log, tap}.
    std::fs::File::create(&go).expect("touch GO gate");

    // The tap receives exactly N hostward bytes (no drops at this size).
    let captured = collect_tap(&mut tap, N, Duration::from_secs(40));
    assert_eq!(
        captured.len(),
        N,
        "tap delivered {} bytes, expected {N} (dropped or truncated)",
        captured.len()
    );

    // The co-attached log reaches N bytes.
    assert!(
        wait_until(Duration::from_secs(20), || file_len(&log) == N as u64),
        "log file never reached {N} bytes (got {})",
        file_len(&log)
    );
    let logged = std::fs::read(&log).expect("read serial.log");

    // Byte-exact: tap == co-attached log == the seeded source (three independent
    // computations). Any drop or corruption on the tap path breaks one of these.
    let src_sha = sha256_hex(&seeded_bytes(SEED, N));
    let tap_sha = sha256_hex(&captured);
    let log_sha = sha256_hex(&logged);
    assert_eq!(tap_sha, log_sha, "tap checksum != co-attached log checksum");
    assert_eq!(
        tap_sha, src_sha,
        "tap checksum != source checksum (tap dropped or corrupted bytes)"
    );

    // Drop the tap's connection: state must show zero taps (detach-on-drop).
    drop(tap);
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 0),
        "tap did not detach after its connection dropped (taps={:?})",
        rpc.state()["taps"]
    );

    // Explicit connection-drop test: open a fresh tap, confirm it registers, drop its
    // connection, and confirm prompt detach even with an idle endpoint (the source is
    // done and the device may have gone away, but the endpoint's hub persists).
    let watch = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 1),
        "persistent tap did not register (taps={:?})",
        rpc.state()["taps"]
    );
    drop(watch);
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 0),
        "dropped tap did not detach (taps={:?})",
        rpc.state()["taps"]
    );
}

// ============================================================================
// Tap lifecycle: `tap.close` (T3) and graph reconfiguration beneath a live tap
// (TAP-1 / T6). Control-plane facts — they run on every platform.
// ============================================================================

/// A raw tap connection. Unlike the harness's [`Subscription`] (which discards the
/// request ack), this keeps the `tap.open` result *and* can issue further requests on
/// the same connection — which the §10/§17 contract requires of a real client: a tap's
/// stream, its `tap.close`, and its terminal `tap.closed` all live on **one**
/// connection. Notifications seen while waiting for a response are stashed, not
/// dropped, so a later `wait_note` cannot miss one.
struct TapConn {
    stream: UnixStream,
    buf: Vec<u8>,
    notes: Vec<Value>,
    next_id: i64,
}

impl TapConn {
    fn open(socket: &Path) -> TapConn {
        TapConn {
            stream: UnixStream::connect(socket).expect("connect control socket"),
            buf: Vec::new(),
            notes: Vec::new(),
            next_id: 1,
        }
    }

    /// Send `method`/`params` and return the whole JSON-RPC response object (success
    /// *or* error — both are assertable outcomes here). Notifications that arrive first
    /// are stashed in `self.notes`.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut req = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if !params.is_null() {
            req["params"] = params;
        }
        self.stream
            .write_all(format!("{req}\n").as_bytes())
            .expect("write request");
        self.stream.flush().expect("flush request");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let now = Instant::now();
            assert!(now < deadline, "no response to `{method}` within 10s");
            let Some(v) = self.next_object(deadline - now) else {
                panic!("the connection closed while waiting for `{method}`");
            };
            if v.get("id").and_then(Value::as_i64) == Some(id) {
                return v;
            }
            self.notes.push(v);
        }
    }

    /// `tap.open`, returning the response object.
    fn tap_open(&mut self, endpoint: &str, replay: bool) -> Value {
        self.call(
            "tap.open",
            json!({ "endpoint": endpoint, "replay": replay }),
        )
    }

    /// `tap.close`, returning the response object.
    fn tap_close(&mut self, tap: u64) -> Value {
        self.call("tap.close", json!({ "tap": tap }))
    }

    fn read_line(&mut self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
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

    fn next_object(&mut self, timeout: Duration) -> Option<Value> {
        let line = self.read_line(timeout)?;
        Some(
            serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON ({e}): {line}")),
        )
    }

    /// Wait for an id-less notification of `method` (checking anything already stashed
    /// first), or `None` within `timeout`.
    fn wait_note(&mut self, method: &str, timeout: Duration) -> Option<Value> {
        if let Some(pos) = self
            .notes
            .iter()
            .position(|n| n.get("method").and_then(Value::as_str) == Some(method))
        {
            return Some(self.notes.remove(pos));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let v = self.next_object(deadline - now)?;
            if v.get("method").and_then(Value::as_str) == Some(method) {
                return Some(v);
            }
            self.notes.push(v);
        }
    }

    /// Assert no notification of `method` arrives within `window` — the "nothing
    /// happened to this tap" half of T6, which a bare state check cannot express.
    fn expect_no_note(&mut self, method: &str, window: Duration, ctx: &str) {
        if let Some(v) = self.wait_note(method, window) {
            panic!("unexpected `{method}` {ctx}: {v}");
        }
    }
}

/// The `params` of a `tap.closed` notification, as `(tap, endpoint, reason)`.
fn closed_params(note: &Value) -> (u64, String, String) {
    let p = note.get("params").expect("tap.closed carries params");
    (
        p["tap"].as_u64().expect("tap.closed names the tap id"),
        p["endpoint"]
            .as_str()
            .expect("tap.closed names the endpoint")
            .to_owned(),
        p["reason"]
            .as_str()
            .expect("tap.closed carries a reason")
            .to_owned(),
    )
}

/// The `result.tap` id of a successful `tap.open`.
fn tap_id(ack: &Value) -> u64 {
    ack.pointer("/result/tap")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("tap.open did not return a tap id: {ack}"))
}

/// The `state.taps` entries, as `(tap, endpoint)` pairs.
fn open_taps(rpc: &Rpc) -> Vec<(u64, String)> {
    let mut taps: Vec<(u64, String)> = rpc.state()["taps"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| {
                    (
                        t["tap"].as_u64().expect("state tap id"),
                        t["endpoint"]
                            .as_str()
                            .expect("state tap endpoint")
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    // Sorted by tap id — i.e. by the order they were opened, since ids are allocated
    // monotonically. These assertions are about *which* taps are open; `state`'s own
    // endpoint-sorted rendering has its own guard (the DM-5 test in `p8_tap_drops.rs`).
    taps.sort();
    taps
}

/// Two host-facing serial endpoints whose devices are absent (so both load `waiting`
/// — their tap hubs exist from the first instant, §15.8) plus a pty attached to `usb0`,
/// so removing `usb0` needs `--cascade`.
fn absent_cfg(d: &Daemon) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev0}"
[[node]]
type = "serial"
name = "usb1"
device = "{dev1}"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[edge]]
a = "usb0"
b = "console"
"#,
        dev0 = d.run().join("absent-usb0").display(),
        dev1 = d.run().join("absent-usb1").display(),
        console = d.run().join("console").display(),
    )
}

/// T3 — `tap.close` had **zero** coverage, while the web console calls it on every
/// console switch. Both halves: the happy path detaches exactly this tap (and the
/// connection lives on), and a *stranger* connection cannot close a tap it did not
/// open — taps are connection-scoped (§17), so the id alone is not authority.
#[test]
fn tap_close_detaches_its_own_tap_and_a_stranger_connection_cannot() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(&d), false).expect("load graph");

    let mut owner = TapConn::open(&d.socket());
    let ack = owner.tap_open("usb0", true);
    let id = tap_id(&ack);
    // The `tap.open` result's full shape (§17/plan §11.8/TAP-1b): the id, the echoed
    // endpoint, the empty-replay marker, the offset the stream begins at, and the
    // endpoint's feed-loss baseline that `tap.data.gap_before` deltas are measured from.
    assert_eq!(ack.pointer("/result/endpoint"), Some(&json!("usb0")));
    assert_eq!(ack.pointer("/result/replay_bytes"), Some(&json!(0)));
    assert_eq!(ack.pointer("/result/from_offset"), Some(&json!(0)));
    assert_eq!(
        ack.pointer("/result/feed_dropped"),
        Some(&json!(0)),
        "tap.open must report the endpoint's feed-drop baseline (TAP-1b): {ack}"
    );
    assert_eq!(open_taps(rpc), vec![(id, "usb0".to_string())]);

    // A second connection knows the id but did not open it: refused, and the tap
    // survives. (Anything else would let any client close another's console stream.)
    let mut stranger = TapConn::open(&d.socket());
    let refused = stranger.tap_close(id);
    assert!(
        refused.get("result").is_none(),
        "a stranger connection closed someone else's tap: {refused}"
    );
    assert_eq!(
        refused.pointer("/error/code").and_then(Value::as_i64),
        Some(-32602),
        "wrong-connection tap.close: {refused}"
    );
    let msg = refused
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        msg.contains("no open tap"),
        "the refusal does not say the tap is not this connection's: {msg:?}"
    );
    assert_eq!(
        open_taps(rpc),
        vec![(id, "usb0".to_string())],
        "the stranger's refused close still detached the tap"
    );

    // The owner closes it: a defined success, and `state.taps` shrinks.
    let closed = owner.tap_close(id);
    assert_eq!(
        closed.pointer("/result/closed").and_then(Value::as_u64),
        Some(id),
        "tap.close did not report the closed id: {closed}"
    );
    assert!(
        wait_until(Duration::from_secs(5), || open_taps(rpc).is_empty()),
        "tap.close left the tap attached (taps={:?})",
        open_taps(rpc)
    );

    // Closing it twice is an error, not a hollow success.
    let again = owner.tap_close(id);
    assert!(
        again.get("result").is_none(),
        "closing an already-closed tap reported success: {again}"
    );

    // And the connection itself is unharmed — `tap.close` closes a tap, not a session.
    let after = owner.call("state", Value::Null);
    assert!(
        after.get("result").is_some(),
        "the connection died with its tap: {after}"
    );
}

/// TAP-1 — `load --replace` drops every hub. The open tap must be told, not silently
/// orphaned: a terminal `tap.closed` naming the endpoint and the reason, `state.taps`
/// emptied, and a later `tap.close` on that id refused rather than hollowly succeeding.
/// The connection survives (its other taps and its subscription are none of this tap's
/// business). Reproduction 6 of the review's log, inverted into a guard.
#[test]
fn load_replace_tells_open_taps_their_endpoint_is_gone() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(&d), false).expect("load graph");

    let mut conn = TapConn::open(&d.socket());
    let id = tap_id(&conn.tap_open("usb0", true));
    assert_eq!(open_taps(rpc), vec![(id, "usb0".to_string())]);

    // A different connection replaces the graph out from under the tap.
    rpc.load_toml(&absent_cfg(&d), true)
        .expect("load --replace");

    let note = conn
        .wait_note("tap.closed", Duration::from_secs(5))
        .expect("no `tap.closed` after load --replace — the tap was silently orphaned");
    assert_eq!(
        closed_params(&note),
        (id, "usb0".to_string(), "graph replaced".to_string()),
        "tap.closed did not name this tap, its endpoint and the reason: {note}"
    );
    assert!(
        wait_until(Duration::from_secs(5), || open_taps(rpc).is_empty()),
        "state still lists the orphaned tap: {:?}",
        open_taps(rpc)
    );

    // The connection-side handle outlived the hub; closing it must fail loudly.
    let closed = conn.tap_close(id);
    assert!(
        closed.get("result").is_none(),
        "tap.close on a daemon-closed tap reported success: {closed}"
    );
    let msg = closed
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        msg.contains("already closed") && msg.contains("usb0"),
        "the refusal does not explain that the endpoint is gone: {msg:?}"
    );

    // The connection is still usable — `tap.closed` is terminal for the tap only.
    assert!(
        conn.call("state", Value::Null).get("result").is_some(),
        "the connection died with its tap"
    );

    // The rebuilt endpoint is tappable again (a client's correct response to
    // `tap.closed` is to re-open, which is what the web console does).
    let fresh = tap_id(&conn.tap_open("usb0", true));
    assert_eq!(open_taps(rpc), vec![(fresh, "usb0".to_string())]);
}

/// TAP-1 — `teardown` reaches the same hub drop, with its own reason token.
#[test]
fn teardown_tells_open_taps_their_endpoint_is_gone() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(&d), false).expect("load graph");

    let mut conn = TapConn::open(&d.socket());
    let id = tap_id(&conn.tap_open("usb1", false));

    rpc.teardown();

    let note = conn
        .wait_note("tap.closed", Duration::from_secs(5))
        .expect("no `tap.closed` after teardown — the tap was silently orphaned");
    assert_eq!(
        closed_params(&note),
        (id, "usb1".to_string(), "teardown".to_string()),
        "tap.closed after teardown: {note}"
    );
    assert!(
        wait_until(Duration::from_secs(5), || open_taps(rpc).is_empty()),
        "state still lists the orphaned tap: {:?}",
        open_taps(rpc)
    );
    assert!(
        conn.tap_close(id).get("result").is_none(),
        "tap.close after teardown reported a success that closed nothing"
    );
}

/// TAP-1 + T6 — `remove-node --cascade` closes the taps on **that node's** endpoints
/// and no others. Both taps live on one connection, so this also pins that a
/// `tap.closed` is terminal for its tap and not for the connection or its siblings.
#[test]
fn remove_node_cascade_closes_only_the_removed_endpoints_taps() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(&d), false).expect("load graph");

    let mut conn = TapConn::open(&d.socket());
    let doomed = tap_id(&conn.tap_open("usb0", false));
    let survivor = tap_id(&conn.tap_open("usb1", false));
    assert_eq!(
        open_taps(rpc),
        vec![(doomed, "usb0".to_string()), (survivor, "usb1".to_string())]
    );

    // usb0 has an attached pty edge, so this is the cascade path.
    rpc.remove_node("usb0", true)
        .expect("remove-node usb0 --cascade");

    let note = conn
        .wait_note("tap.closed", Duration::from_secs(5))
        .expect("no `tap.closed` after remove-node --cascade");
    assert_eq!(
        closed_params(&note),
        (doomed, "usb0".to_string(), "endpoint removed".to_string()),
        "tap.closed after remove-node: {note}"
    );

    // The tap on the surviving endpoint is untouched — in state, and as a live tap
    // this connection can still close itself.
    assert!(
        wait_until(Duration::from_secs(5), || open_taps(rpc)
            == vec![(survivor, "usb1".to_string())]),
        "the surviving endpoint's tap was disturbed: {:?}",
        open_taps(rpc)
    );
    conn.expect_no_note(
        "tap.closed",
        Duration::from_millis(500),
        "for the surviving endpoint's tap",
    );
    assert_eq!(
        conn.tap_close(survivor)
            .pointer("/result/closed")
            .and_then(Value::as_u64),
        Some(survivor),
        "the surviving tap could not be closed by its own connection"
    );
}

/// T6 — the rest of the reconfigure-beneath-a-live-tap matrix: `add-node` and a
/// `remove-node` of an *unrelated* node must leave a live tap attached and silent,
/// while removing the tapped node closes it. The hub map is rebuilt on all three
/// paths, so "only the named endpoints go" is the property that keeps a console alive
/// through ordinary graph surgery.
#[test]
fn add_node_and_unrelated_remove_leave_a_live_tap_attached() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(&d), false).expect("load graph");

    let mut conn = TapConn::open(&d.socket());
    let id = tap_id(&conn.tap_open("usb1", false));

    // add-node: a log node needs no device, so this runs everywhere.
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    rpc.add_node_toml(&format!(
        r#"
[[node]]
type = "log"
name = "logx"
directory = "{dir}"
filename = "x.log"
"#,
        dir = logdir.display(),
    ))
    .expect("add-node logx");
    assert_eq!(
        open_taps(rpc),
        vec![(id, "usb1".to_string())],
        "add-node detached a live tap"
    );
    conn.expect_no_note("tap.closed", Duration::from_millis(500), "after add-node");

    // remove-node of an unrelated node (no edges, so no cascade needed).
    rpc.remove_node("logx", false).expect("remove-node logx");
    assert_eq!(
        open_taps(rpc),
        vec![(id, "usb1".to_string())],
        "removing an unrelated node detached a live tap"
    );
    conn.expect_no_note(
        "tap.closed",
        Duration::from_millis(500),
        "after removing an unrelated node",
    );

    // Removing the tapped node itself does close it — the tap is still live, not a
    // stale registration that had quietly stopped meaning anything.
    rpc.remove_node("usb1", false).expect("remove-node usb1");
    let note = conn
        .wait_note("tap.closed", Duration::from_secs(5))
        .expect("no `tap.closed` when the tapped node was removed");
    assert_eq!(
        closed_params(&note),
        (id, "usb1".to_string(), "endpoint removed".to_string()),
        "tap.closed for the removed tapped node: {note}"
    );
    assert!(open_taps(rpc).is_empty(), "state still lists the tap");
}

/// T3, the data half: after `tap.close` the stream really stops. Bytes keep flowing
/// hostward at the endpoint — proven by the serial node's own `discarded_unattached`
/// counter climbing (§5: with no consumer attached, every hostward byte is counted
/// where it is lost) — while the closed tap's connection receives nothing more.
/// Needs a serial device, so it skips off Linux (§5).
#[test]
fn tap_close_stops_the_stream_while_bytes_keep_flowing() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP tap_close_stops_the_stream_while_bytes_keep_flowing: no serial device");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();

    // A lone echo device: what `send` writes targetward comes straight back hostward.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
"#,
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load echo config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    let mut conn = TapConn::open(&d.socket());
    let id = tap_id(&conn.tap_open("usb0", false));

    // While the tap is open, an echoed line reaches it.
    rpc.send("usb0", "ping1", false, 5000).expect("send ping1");
    let data = conn
        .wait_note("tap.data", Duration::from_secs(10))
        .expect("the open tap never received the echoed line");
    let payload = base64_decode(
        data.pointer("/params/data")
            .and_then(Value::as_str)
            .expect("tap.data payload"),
    );
    assert_eq!(
        payload,
        b"ping1\n",
        "the tap received {:?}, not the echoed line",
        String::from_utf8_lossy(&payload)
    );

    // Close it, then keep the device talking.
    assert_eq!(
        conn.tap_close(id)
            .pointer("/result/closed")
            .and_then(Value::as_u64),
        Some(id)
    );
    let discarded = |rpc: &Rpc| {
        rpc.node("usb0")
            .and_then(|n| n["discarded_unattached"].as_u64())
            .unwrap_or(0)
    };
    let before = discarded(rpc);
    rpc.send("usb0", "ping2", false, 5000).expect("send ping2");
    assert!(
        wait_until(Duration::from_secs(10), || discarded(rpc) >= before + 6),
        "the echoed bytes never reached the endpoint (discarded_unattached {} → {})",
        before,
        discarded(rpc)
    );

    // The bytes flowed; the closed tap saw none of them.
    conn.expect_no_note("tap.data", Duration::from_millis(500), "after tap.close");
}

/// Bytes the endpoint's taps have shed because their connection stopped draining its
/// bounded queue (§5) — the observable proof that a queue is *full*, which is the only
/// state in which the terminal event has nowhere ordinary to go.
fn tap_dropped(rpc: &Rpc) -> u64 {
    rpc.state()["taps"]
        .as_array()
        .map(|taps| {
            taps.iter()
                .filter_map(|t| t.get("dropped").and_then(Value::as_u64))
                .sum()
        })
        .unwrap_or(0)
}

/// 37-CTRL-1 — the terminal `tap.closed` must reach a client whose tap queue is
/// **full**. Every guard above opens its tap on an idle or absent endpoint, where the
/// bounded queue has room and `try_send` always succeeds; the event used to be sent
/// with a `try_send` whose `Full` was discarded, and `detach_all` drops the senders one
/// line later, so on a saturated queue the loss was permanent, uncounted and invisible
/// from the channel. That queue is not an exotic state: it is the ordinary steady state
/// of a slow consumer on a firehose endpoint — precisely the client §10's "the tap does
/// not silently die" is about. When it happened, `ctl tap` re-entered the hang CTL-1
/// was fixed for and the console pane froze with no re-anchor.
///
/// Needs a real byte source to saturate a socket, so it skips off Linux (§7).
#[test]
fn tap_closed_reaches_a_client_whose_tap_queue_is_full() {
    if !cfg!(target_os = "linux") {
        eprintln!(
            "SKIP tap_closed_reaches_a_client_whose_tap_queue_is_full: a pts cannot be a serial device off Linux"
        );
        return;
    }
    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let dev_str = dev.to_string_lossy().into_owned();

    // A paced firehose: fast enough to fill a socket buffer and a 128-slot tap queue in
    // about a second, bounded in total so it cannot outlive the test.
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            "32MiB",
            "--rate",
            "2097152",
            "--link",
            &dev_str,
            "--hold-ms",
            "600000",
            "--timeout-ms",
            "600000",
        ],
        Some(&dev),
    );

    // A lone serial node: no consumer attached, so the hostward bytes are discarded
    // after the tap sees them (§5, §15.32 — the tap mirror is outside the graph).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
hostward_buffer = 8192
"#,
        dev = dev.display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load the firehose config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    let mut conn = TapConn::open(&d.socket());
    let id = tap_id(&conn.tap_open("usb0", false));

    // From here the client reads nothing: the socket fills, the connection blocks
    // mid-write, and its bounded tap queue saturates.
    assert!(
        wait_until(Duration::from_secs(30), || tap_dropped(rpc) > 0),
        "the fixture never saturated the tap queue, so nothing was observed on a full \
         queue (dropped={})",
        tap_dropped(rpc)
    );

    // The graph goes away beneath the saturated tap.
    rpc.teardown();

    // The client starts reading again and must find the terminal event in what it is
    // handed — behind the backlog the socket already held, ahead of whatever bytes are
    // still queued for an endpoint that no longer exists.
    let note = conn
        .wait_note("tap.closed", Duration::from_secs(30))
        .expect(
            "no `tap.closed` reached a client whose tap queue was full: it is left on a \
             live connection receiving nothing, with the loss uncounted (37-CTRL-1)",
        );
    assert_eq!(
        closed_params(&note),
        (id, "usb0".to_string(), "teardown".to_string()),
        "tap.closed did not name this tap, its endpoint and the reason: {note}"
    );

    // Terminal for the tap, not for the connection — the same contract the unsaturated
    // path keeps.
    assert!(
        conn.call("state", Value::Null).get("result").is_some(),
        "the connection died with its tap"
    );
}
