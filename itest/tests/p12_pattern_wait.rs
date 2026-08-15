#![forbid(unsafe_code)]

//! The pattern wait — `tap.wait` (design §10 *The pattern wait*, §15.56; plan §18
//! item 47).
//!
//! Park until one of up to eight named byte patterns appears on a host-facing
//! endpoint's hostward stream, the deadline expires, or the graph drops the
//! endpoint. This file is the acceptance battery for that contract, one test per
//! clause, plus the interplay guard §15.20 demands of any new waiting verb.
//!
//! **Two shapes, deliberately.** The clauses about *outcomes* — a typed expiry, a
//! typed teardown, the maxima, `state` visibility, the pipelined-request refusal —
//! need a hub, not bytes, and every host-facing endpoint gets a hub at load even
//! while its serial node sits `waiting` on an absent device (§15.8). Those run on
//! **every platform**. The clauses about *matching* need real hostward bytes, which
//! off Linux means real hardware (a pts cannot be a serial device — `serial2` →
//! `ENOTTY`, §7), so those gate on [`serial_echo`] and self-skip.
//!
//! **How bytes are made, and why this shape.** A `serial-nexus-sim pty --echo`
//! stands in for the UART; a `send` writes a line targetward, the sim echoes it, and
//! it returns *hostward* — which is the direction a tap and a wait observe. That
//! gives exact bytes at times this test chooses, which a `--source` double (seeded,
//! self-paced) cannot. `send` appends `\n`, so the cross-frame pattern below spans
//! that newline on purpose: it is the byte the two frames meet at.
//!
//! **Fail-first, and where each guard's failure lives.** Every test here fails
//! against the pre-item-47 tree with `-32601 method not found`, which is the weak
//! form. The sharp form is per-guard, recorded so a later reader can re-run it by
//! reverting one thing:
//!
//! | Guard | Revert this and it reddens |
//! |---|---|
//! | [`a_pattern_split_across_two_data_frames_matches`] | feed the matcher per-chunk instead of into `ScanWindow`'s rolling buffer (`daemon/src/pattern.rs`, `ScanWindow::feed`) |
//! | [`a_pattern_already_in_the_ring_matches_only_when_replay_is_asked`] | drop the `replay` arm of `TapHub::arm_wait`; the no-replay half is the control and must keep timing out either way |
//! | [`an_expired_deadline_is_a_typed_result_not_an_error`] | answer expiry with an `RpcError` instead of `timed_out: true` |
//! | [`teardown_while_parked_is_a_typed_error_distinct_from_expiry`] | delete the armed-wait sweep in `TapHub::detach_all` — **measured**: the wait still ends `-32008`, because dropping the hub drops the `oneshot` sender and `tap_wait`'s `Err(_)` arm answers, but it loses the reason string and the accounting, so the guard reddens on those. The sweep is what makes the outcome *informative*, not what makes it typed |
//! | [`an_armed_wait_observes_a_ring_off_untapped_endpoint`] | drop `!self.waits.is_empty()` from `TapHub::refresh_active`; the endpoint stops mirroring and the wait times out on a talking console |
//! | [`an_armed_wait_is_visible_in_state_for_its_lifetime`] | drop the `waits` array from `Daemon::state` |
//! | [`a_verb_pipelined_behind_a_parked_wait_is_refused_and_both_survive`] | handle `tap.wait` in `control.rs` beside `tap.open` instead of in the dispatch lane; it then stops being a waiting verb and the pipelined request is answered normally |
//! | [`every_maximum_is_refused_before_anything_is_armed`] | move `parse_wait_params`/`Matcher::compile` after `arm_wait` |
//! | [`tap_data_keeps_flowing_on_the_connection_whose_wait_is_parked`] | revert `control.rs` to the pre-CTRLW-1 inner loop, which polls only the dispatch future and the next request line — the connection then writes no notification for the wait's whole duration |

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, Subscription, WaitConn, bin, serial_echo, wait_until};

/// How long a wait that is *expected* to match may take before the test calls it a
/// failure. Generous: this bounds a scheduling delay, never the daemon's own answer.
const MATCH_BOUND: Duration = Duration::from_secs(15);

/// A deadline short enough that a test asserting expiry does not pace the suite, and
/// long enough that a loaded box does not expire a wait that was about to match.
const EXPIRE_MS: u64 = 600;

/// Base64 of a pattern's bytes — the wire form for both kinds (§10 clause 2), because
/// console output is not guaranteed UTF-8 and neither is a pattern over it.
fn b64(bytes: &[u8]) -> String {
    serial_nexus_rpc::base64_encode(bytes)
}

fn literal(name: &str, bytes: &[u8]) -> Value {
    json!({ "name": name, "literal": b64(bytes) })
}

/// Whether `hay` contains `needle`. A helper rather than an inline
/// `windows(N).any(...)`: the literal `N` has to agree with the needle's length, and
/// when it did not this file's own replay guard failed for a reason that had nothing
/// to do with replay.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// A graph with one host-facing endpoint whose device is **absent**, so the endpoint
/// has a tap hub and no bytes will ever cross it — the quiet fixture every
/// outcome-shaped clause below wants. Runs on every platform (§15.8).
fn quiet_endpoint(d: &Daemon) -> &Rpc {
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
"#,
        dev = d.run().join("absent-device").display(),
    );
    rpc.load_toml(&cfg, false).expect("load the quiet endpoint");
    rpc
}

/// Park a `tap.wait` on `conn` and return its request id.
fn park(
    conn: &mut WaitConn,
    endpoint: &str,
    patterns: Value,
    timeout_ms: u64,
    replay: bool,
) -> i64 {
    conn.send(
        "tap.wait",
        json!({
            "endpoint": endpoint,
            "patterns": patterns,
            "timeout_ms": timeout_ms,
            "replay": replay,
        }),
    )
}

/// The armed waits `state` reports.
fn state_waits(rpc: &Rpc) -> Vec<Value> {
    rpc.state()["waits"].as_array().cloned().unwrap_or_default()
}

// --- Outcome clauses: every platform ----------------------------------------------

/// §10 clause 6 / §15.56's result-shaped-timeout decision: expiry is a **result**,
/// never an error, and it carries the counters that say how strong the "no" is.
#[test]
fn an_expired_deadline_is_a_typed_result_not_an_error() {
    let d = Daemon::start();
    let rpc = quiet_endpoint(&d);
    let mut conn = WaitConn::connect(&d.socket());
    let id = park(
        &mut conn,
        "usb0",
        json!([literal("never", b"never")]),
        EXPIRE_MS,
        false,
    );

    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("the wait must answer at its deadline");
    assert!(
        resp.get("error").is_none_or(Value::is_null),
        "a deadline is an answer, not an error: {resp}"
    );
    let r = &resp["result"];
    assert_eq!(r["timed_out"], json!(true), "{r}");
    assert_eq!(r["matched"], Value::Null, "{r}");
    // The counters that make a "no match" readable: this endpoint is silent, so the
    // scan covered nothing and nothing was lost — and both are *stated* rather than
    // left for the caller to assume (§10 clause 4).
    assert_eq!(r["bytes_scanned"], json!(0), "{r}");
    assert_eq!(r["gaps"], json!(0), "{r}");
    assert_eq!(r["gap_bytes"], json!(0), "{r}");
    assert!(
        r["epoch"].as_u64().is_some(),
        "the offset space is named: {r}"
    );

    // Expiry never consumes (§15.20 clause 2 / §10 clause 1): the wait left nothing
    // behind, so the endpoint is exactly as a never-issued wait would have left it.
    assert!(state_waits(rpc).is_empty(), "an expired wait is disarmed");
    let ep = &rpc.state()["endpoints"][0];
    assert_eq!(ep["waits"], json!(0), "{ep}");
    // What is deliberately NOT asserted here: "an armed wait charges no loss".
    // `quiet_endpoint`'s device is absent, so no hostward byte ever exists and
    // `feed_dropped` is structurally 0 whatever the wait does — an assertion whose
    // passing output is identical to its not-running output, which is plan §3 rule
    // 22's own tell. The property is real (§10 clause 7) and needs a fixture that
    // produces bytes; it is filed rather than faked.
}

/// §10 clause 6: teardown while parked is the **typed error** outcome, distinct from
/// expiry. The two must not collapse — a timeout claims the stream was watched
/// throughout and stayed silent, while this says the stream stopped existing.
#[test]
fn teardown_while_parked_is_a_typed_error_distinct_from_expiry() {
    let d = Daemon::start();
    let rpc = quiet_endpoint(&d);
    let mut conn = WaitConn::connect(&d.socket());
    // A deadline far past the teardown, so an expiry cannot be mistaken for this.
    let id = park(
        &mut conn,
        "usb0",
        json!([literal("never", b"never")]),
        60_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(rpc).is_empty()),
        "the wait must be armed before the teardown that is meant to end it"
    );

    rpc.teardown();

    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("teardown must end a parked wait, not strand it");
    let err = resp
        .get("error")
        .filter(|e| !e.is_null())
        .unwrap_or_else(|| panic!("teardown is an error outcome, got {resp}"));
    assert_eq!(
        err["code"],
        json!(-32008),
        "the endpoint-gone code, not the timeout result and not a generic failure: {err}"
    );
    // The accounting rides the error too, so an abandoned wait still says how much of
    // the stream it did cover.
    assert!(
        err["data"]["bytes_scanned"].as_u64().is_some(),
        "the counters ride the teardown error: {err}"
    );
    assert!(
        err["message"].as_str().unwrap_or("").contains("teardown"),
        "the reason is named, as `tap.closed`'s is: {err}"
    );
}

/// §10 clause 7: an armed wait is visible in `state` for its lifetime, as a tap is —
/// an observer an operator cannot see is one they cannot account for.
#[test]
fn an_armed_wait_is_visible_in_state_for_its_lifetime() {
    let d = Daemon::start();
    let rpc = quiet_endpoint(&d);
    let mut conn = WaitConn::connect(&d.socket());
    let _id = park(
        &mut conn,
        "usb0",
        json!([literal("alpha", b"alpha"), literal("beta", b"beta")]),
        60_000,
        false,
    );

    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(rpc).is_empty()),
        "an armed wait must appear in state"
    );
    let waits = state_waits(rpc);
    assert_eq!(waits.len(), 1, "{waits:?}");
    let w = &waits[0];
    assert_eq!(w["endpoint"], json!("usb0"), "{w}");
    assert!(w["wait"].as_u64().is_some(), "the wait carries an id: {w}");
    assert_eq!(
        w["patterns"],
        json!(["alpha", "beta"]),
        "the pattern *names* are reported, in the caller's order — never the bytes, \
         which are the caller's: {w}"
    );
    assert_eq!(w["bytes_scanned"], json!(0), "{w}");

    // Cancel-on-disconnect (§15.20 clause 4): closing the connection disarms it, so
    // the visibility really is "for its lifetime" and not "forever".
    conn.cancel();
    assert!(
        wait_until(Duration::from_secs(5), || state_waits(rpc).is_empty()),
        "a cancelled wait leaves state: {:?}",
        state_waits(rpc)
    );
}

/// §15.20 clause 5, applied to the new verb: a request pipelined behind a parked wait
/// is refused with `-32006`, and the wait, the connection and its taps all survive.
///
/// This is the `p12_control_streams` shape — the guard every waiting verb owes,
/// because the alternative the review found was silence: the connection died with no
/// reply to either request, taking a §17 browser's subscription and taps with it.
#[test]
fn a_verb_pipelined_behind_a_parked_wait_is_refused_and_both_survive() {
    let d = Daemon::start();
    let rpc = quiet_endpoint(&d);
    let mut conn = WaitConn::connect(&d.socket());
    let wait_id = park(
        &mut conn,
        "usb0",
        json!([literal("never", b"never")]),
        3_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(rpc).is_empty()),
        "the wait must be parked before anything is pipelined behind it"
    );

    // A second request on the *same* connection, while the first is parked.
    let pipelined = conn.send("info", Value::Null);
    let refusal = conn
        .response(pipelined, Duration::from_secs(5))
        .expect("the pipelined request is answered, not swallowed");
    assert_eq!(
        refusal["error"]["code"],
        json!(-32006),
        "one waiting verb per connection: {refusal}"
    );

    // And the wait survived the refusal — it answers its own deadline afterwards.
    let resp = conn
        .response(wait_id, MATCH_BOUND)
        .expect("the parked wait must survive the refusal of the request behind it");
    assert_eq!(resp["result"]["timed_out"], json!(true), "{resp}");

    // A *different* connection is unaffected throughout — the refusal is per
    // connection, and a client wanting concurrency opens a second one.
    assert!(rpc.info().get("daemon_version").is_some());
}

/// §10 clause 2 and §16.12: every dimension carries a stated maximum, checked
/// **before anything is armed** — so a refused request leaves the endpoint exactly as
/// it found it, mirror flag included.
#[test]
fn every_maximum_is_refused_before_anything_is_armed() {
    let d = Daemon::start();
    let rpc = quiet_endpoint(&d);

    let cases: Vec<(&str, Value)> = vec![
        (
            "nine patterns",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": (0..9).map(|i| literal(&format!("p{i}"), b"x")).collect::<Vec<_>>() }),
        ),
        (
            "zero patterns",
            json!({ "endpoint": "usb0", "timeout_ms": 100, "patterns": [] }),
        ),
        (
            "a pattern over the byte maximum",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [literal("big", &vec![b'x'; 1025])] }),
        ),
        (
            "a name over the identifier maximum",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [literal(&"n".repeat(257), b"x")] }),
        ),
        (
            "two patterns sharing a name",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [literal("same", b"a"), literal("same", b"b")] }),
        ),
        (
            "a lookback over its maximum",
            json!({ "endpoint": "usb0", "timeout_ms": 100, "lookback": 65_537,
                    "patterns": [literal("p", b"x")] }),
        ),
        (
            "a context over its maximum",
            json!({ "endpoint": "usb0", "timeout_ms": 100, "context": 4097,
                    "patterns": [literal("p", b"x")] }),
        ),
        (
            "a deadline over the timer maximum",
            json!({ "endpoint": "usb0", "timeout_ms": 3_600_001,
                    "patterns": [literal("p", b"x")] }),
        ),
        (
            "no deadline at all",
            json!({ "endpoint": "usb0", "patterns": [literal("p", b"x")] }),
        ),
        (
            "a pattern that is neither literal nor regex",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [{ "name": "p" }] }),
        ),
        (
            "a pattern that is both at once",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [{ "name": "p", "literal": b64(b"a"), "regex": b64(b"a") }] }),
        ),
        (
            "a regex whose source is not UTF-8",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [{ "name": "p", "regex": b64(&[0xff, 0xfe]) }] }),
        ),
        (
            "an unparseable regex",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [{ "name": "p", "regex": b64(b"a(") }] }),
        ),
        (
            // §15.56's decline made structural: a program over the compile ceiling is
            // refused at compile rather than paid for on the thread that runs every
            // console.
            "a counted-repetition bomb",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [{ "name": "bomb", "regex": b64(b"(?:a{1000}){1000}") }] }),
        ),
        (
            // The *floor*, not a maximum: a window shorter than the pattern cannot
            // hold it, so §10 clause 4's cross-frame guarantee would be silently
            // false rather than merely narrow. Refusing is the only answer that
            // does not lie about the scan.
            "a lookback under the longest literal",
            json!({ "endpoint": "usb0", "timeout_ms": 100, "lookback": 5,
                    "patterns": [literal("p", b"login:")] }),
        ),
        (
            "a zero lookback",
            json!({ "endpoint": "usb0", "timeout_ms": 100, "lookback": 0,
                    "patterns": [literal("p", b"x")] }),
        ),
        (
            "an empty pattern",
            json!({ "endpoint": "usb0", "timeout_ms": 100,
                    "patterns": [literal("p", b"")] }),
        ),
        (
            "an endpoint that does not exist",
            json!({ "endpoint": "nosuch", "timeout_ms": 100,
                    "patterns": [literal("p", b"x")] }),
        ),
    ];

    for (what, params) in cases {
        let err = rpc
            .call("tap.wait", params)
            .expect_err(&format!("{what} must be refused"));
        assert_eq!(err.code, -32602, "{what}: [{}] {}", err.code, err.message);
        // A refusal whose stated reason is false is worse than one with no reason:
        // the empty pattern used to share the over-length error and be refused for
        // being "0 bytes, over the 1024-byte maximum".
        if what == "an empty pattern" {
            assert!(
                err.message.contains("empty") && !err.message.contains("maximum"),
                "the empty-pattern refusal must not blame a maximum: {}",
                err.message
            );
        }
        if what == "a lookback under the longest literal" {
            assert!(
                err.message.contains("lookback") && err.message.contains('6'),
                "the floor refusal names the field and the floor it needs: {}",
                err.message
            );
        }
        // Refused *before* arming: nothing is watching, on any endpoint.
        assert!(
            state_waits(rpc).is_empty(),
            "{what}: a refused wait must arm nothing — {:?}",
            state_waits(rpc)
        );
    }
}

/// §10's version-skew rule, from the other side: the verb is reachable over RPC, and
/// `serial-nexus-ctl` reaches it too — with the exit status carrying the verdict.
///
/// Exit `1` is "the deadline expired with no match" (`grep(1)`'s scheme; see
/// `Cmd::TapWait`). Asserting the *number* rather than `!success()` is the point: `2`
/// is "the wait could not run", and a test that accepted any non-zero could not tell a
/// working timeout from a broken invocation.
#[test]
fn ctl_reaches_the_verb_and_its_exit_status_is_the_verdict() {
    let d = Daemon::start();
    let _rpc = quiet_endpoint(&d);

    let out = Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(d.socket())
        .args([
            "tap-wait",
            "usb0",
            "--pattern",
            "never=never",
            "--timeout-ms",
        ])
        .arg(EXPIRE_MS.to_string())
        .output()
        .expect("run serial-nexus-ctl tap-wait");
    assert_eq!(
        out.status.code(),
        Some(1),
        "a silent endpoint exits 1 (no match), never 0 and never 2 — stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no match"),
        "the human rendering states the verdict: {stdout:?}"
    );

    // A wait that could not run at all is 2, not 1: the two must be distinguishable
    // by a script, which is the whole reason this verb owns a multi-value exit.
    let out = Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(d.socket())
        .args([
            "tap-wait",
            "nosuch-endpoint",
            "--pattern",
            "never=never",
            "--timeout-ms",
            "500",
        ])
        .output()
        .expect("run serial-nexus-ctl tap-wait against a missing endpoint");
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown endpoint is a failure to run (2), not a no-match (1) — stderr={:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// --- Matching clauses: need real hostward bytes (Linux) ----------------------------

/// A `serial` node over a `sim pty --echo` double, plus the pty console that gives
/// the endpoint a targetward path for `send`. `None` where a pts cannot be a serial
/// device (§7), where every test using it self-skips.
struct Echo {
    daemon: Daemon,
    _echo: serial_nexus_itest::SerialEcho,
}

impl Echo {
    fn start(ring: Option<u64>) -> Option<Echo> {
        let echo = serial_echo()?;
        let d = Daemon::start();
        let console = d.run().join("console");
        let ring = match ring {
            Some(n) => format!("replay_ring = {n}"),
            None => String::new(),
        };
        let cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
arbitration = "free-for-all"
{ring}
[[node]]
type = "pty"
name = "console"
path = "{console}"
hostward_buffer = 8192
[[edge]]
a = "usb0"
b = "console"
"#,
            device = echo.device().display(),
            console = console.display(),
        );
        d.rpc().load_toml(&cfg, false).expect("load the echo graph");
        assert!(
            d.rpc()
                .wait_status("usb0", "active", Duration::from_secs(10)),
            "usb0 not active: {:?}",
            d.rpc().node("usb0")
        );
        Some(Echo {
            daemon: d,
            _echo: echo,
        })
    }

    fn rpc(&self) -> &Rpc {
        self.daemon.rpc()
    }

    /// Write one line targetward; the sim echoes it back hostward, which is the
    /// direction a tap and a wait observe. `send` appends `\n`.
    fn emit(&self, line: &str) {
        self.rpc()
            .send("usb0", line, false, 5_000)
            .unwrap_or_else(|e| panic!("send {line:?}: [{}] {}", e.code, e.message));
    }
}

/// Collect `tap.data` payloads from `sub` until `pred` is satisfied or `timeout`
/// elapses, returning one entry per **frame** — the frame boundaries are what the
/// cross-frame guard is about, so they must not be concatenated away.
fn frames_until(
    sub: &mut Subscription,
    timeout: Duration,
    mut pred: impl FnMut(&[Vec<u8>]) -> bool,
) -> Vec<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut frames: Vec<Vec<u8>> = Vec::new();
    while !pred(&frames) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match sub.next(deadline - now) {
            Some(v) if v.get("method").and_then(Value::as_str) == Some("tap.data") => {
                if let Some(data) = v["params"]["data"].as_str() {
                    frames.push(
                        serial_nexus_rpc::base64_decode(data)
                            .expect("a tap.data payload is valid base64"),
                    );
                }
            }
            Some(_) => continue,
            None => break,
        }
    }
    frames
}

/// §10 clause 4: "a pattern split across data-frame boundaries matches by
/// construction" — the property that makes the daemon-side matcher worth having,
/// since it is the one every hand-rolled client matcher gets wrong first.
///
/// The two halves are emitted as two separate `send`s, and the test **proves** they
/// arrived as two separate frames by watching a tap on another connection before
/// emitting the second half. Without that proof the guard could pass on a single
/// coalesced frame and assert nothing.
#[test]
fn a_pattern_split_across_two_data_frames_matches() {
    let Some(e) = Echo::start(None) else {
        eprintln!(
            "SKIP a_pattern_split_across_two_data_frames_matches: no serial device on \
             this platform (a pty is not a serial device off Linux)"
        );
        return;
    };
    // An independent observer of the frame boundaries.
    let mut tap = e
        .rpc()
        .stream("tap.open", json!({ "endpoint": "usb0", "replay": false }));

    let mut conn = WaitConn::connect(&e.daemon.socket());
    // `A\nB` spans the newline `send` appends to the first half: the two frames meet
    // exactly inside the pattern.
    let id = park(
        &mut conn,
        "usb0",
        json!([literal("seam", b"A\nB")]),
        // Inside MATCH_BOUND, so a per-chunk matcher answers `timed_out: true` and the
        // fail-first failure names the seam rather than reading "the wait never
        // answered" (measured: the 30 s this used to be produced exactly that).
        5_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(e.rpc()).is_empty()),
        "the wait must be armed before the first half is emitted"
    );

    e.emit("xxA");
    let first = frames_until(&mut tap, Duration::from_secs(10), |f| {
        f.iter().any(|p| p.ends_with(b"A\n"))
    });
    assert!(
        first.iter().any(|p| p.ends_with(b"A\n")),
        "the first half must arrive before the second is sent, or the two coalesce \
         into one frame and this guard asserts nothing: {first:?}"
    );

    e.emit("Byy");

    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("the wait must answer");
    let r = &resp["result"];
    assert_eq!(r["timed_out"], json!(false), "{r}");
    assert_eq!(r["matched"]["pattern"], json!("seam"), "{r}");

    // And the frames really were two: the pattern's bytes are not contained in any
    // single one. This is what turns "it matched" into "it matched across a seam".
    let all = frames_until(&mut tap, Duration::from_secs(5), |f| {
        f.iter().any(|p| p.starts_with(b"B"))
    });
    let frames: Vec<Vec<u8>> = first.into_iter().chain(all).collect();
    assert!(
        !frames.iter().any(|f| contains(f, b"A\nB")),
        "no single frame contained the pattern, so the match spanned them: {frames:?}"
    );
}

/// §10 clause 5: with replay, a pattern wholly emitted **before** the wait began
/// matches — and the no-replay half is the control that proves replay is what found
/// it rather than a live byte arriving late.
#[test]
fn a_pattern_already_in_the_ring_matches_only_when_replay_is_asked() {
    let Some(e) = Echo::start(Some(65_536)) else {
        eprintln!(
            "SKIP a_pattern_already_in_the_ring_matches_only_when_replay_is_asked: no \
             serial device on this platform"
        );
        return;
    };
    // Emit the marker and confirm it is in the endpoint's past before any wait exists.
    let mut tap = e
        .rpc()
        .stream("tap.open", json!({ "endpoint": "usb0", "replay": false }));
    e.emit("MARKER-9f2b");
    let seen = frames_until(&mut tap, Duration::from_secs(10), |f| {
        f.iter().any(|p| contains(p, b"MARKER-9f2b"))
    });
    assert!(
        seen.iter().any(|p| contains(p, b"MARKER-9f2b")),
        "the marker must have crossed the endpoint before either wait is armed: {seen:?}"
    );

    // The control first: no replay, so the marker is in the past and unreachable.
    let mut plain = WaitConn::connect(&e.daemon.socket());
    let plain_id = park(
        &mut plain,
        "usb0",
        json!([literal("marker", b"MARKER-9f2b")]),
        EXPIRE_MS,
        false,
    );
    let resp = plain
        .response(plain_id, MATCH_BOUND)
        .expect("the control wait must answer");
    assert_eq!(
        resp["result"]["timed_out"],
        json!(true),
        "without replay the marker is in the past and must NOT be found — otherwise \
         the replay half below proves nothing: {resp}"
    );
    assert_eq!(resp["result"]["replay_scanned"], json!(0), "{resp}");

    // Now the same wait with replay: the ring is scanned inside the arming critical
    // section, so it matches.
    let mut replayed = WaitConn::connect(&e.daemon.socket());
    let replay_id = park(
        &mut replayed,
        "usb0",
        json!([literal("marker", b"MARKER-9f2b")]),
        EXPIRE_MS,
        true,
    );
    let resp = replayed
        .response(replay_id, MATCH_BOUND)
        .expect("the replay wait must answer");
    let r = &resp["result"];
    assert_eq!(r["timed_out"], json!(false), "the ring holds it: {r}");
    assert_eq!(r["matched"]["pattern"], json!("marker"), "{r}");
    assert!(
        r["replay_scanned"].as_u64().unwrap_or(0) > 0,
        "the ring scan is reported, so a caller knows what the verdict covered: {r}"
    );
}

/// §10 clause 6: the result names its evidence — which pattern fired, at what offset,
/// in which epoch, with bounded context.
///
/// The offset is cross-checked against a **tap's** view of the same stream rather
/// than against the wait's own arithmetic: `tap.data`'s offset space is the one §10
/// says a match offset is expressed in, so comparing the two is what makes the claim
/// mean something. A test that recomputed the offset the same way the daemon does
/// would agree with a wrong daemon.
#[test]
fn the_result_names_the_pattern_its_offset_and_its_context() {
    let Some(e) = Echo::start(None) else {
        eprintln!(
            "SKIP the_result_names_the_pattern_its_offset_and_its_context: no serial \
             device on this platform"
        );
        return;
    };
    let mut tap = e
        .rpc()
        .stream("tap.open", json!({ "endpoint": "usb0", "replay": false }));
    // The tap's stream begins at the live edge; every frame carries its own offset,
    // so the marker's true offset is (frame offset + index within the frame).
    let mut conn = WaitConn::connect(&e.daemon.socket());
    let id = park(
        &mut conn,
        "usb0",
        json!([
            literal("decoy", b"NEVER-APPEARS"),
            literal("target", b"NEEDLE")
        ]),
        30_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(e.rpc()).is_empty()),
        "the wait must be armed first"
    );

    e.emit("padpadpadNEEDLEpad");

    // The tap's independent reading of where NEEDLE sits in the offset space.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut truth: Option<u64> = None;
    while truth.is_none() && Instant::now() < deadline {
        let Some(v) = tap.next(Duration::from_secs(2)) else {
            break;
        };
        if v.get("method").and_then(Value::as_str) != Some("tap.data") {
            continue;
        }
        let base = v["params"]["offset"].as_u64().expect("a frame offset");
        let bytes = serial_nexus_rpc::base64_decode(v["params"]["data"].as_str().unwrap())
            .expect("valid base64");
        if let Some(i) = bytes.windows(6).position(|w| w == b"NEEDLE") {
            truth = Some(base + i as u64);
        }
    }
    let truth = truth.expect("the tap must see NEEDLE so its offset can be cross-checked");

    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("the wait must answer");
    let m = &resp["result"]["matched"];
    assert_eq!(
        m["pattern"],
        json!("target"),
        "the pattern that fired is named, not merely 'something matched': {m}"
    );
    assert_eq!(
        m["offset"].as_u64(),
        Some(truth),
        "the match offset must be the same offset `tap.data` reports for those bytes: {m}"
    );
    assert_eq!(m["end_offset"].as_u64(), Some(truth + 6), "{m}");
    assert!(
        resp["result"]["epoch"].as_u64().is_some(),
        "the offset space the match offset counts in is named (§15.38): {resp}"
    );
    let ctx = serial_nexus_rpc::base64_decode(m["context"].as_str().expect("context is a string"))
        .expect("context is base64");
    assert!(
        contains(&ctx, b"NEEDLE"),
        "the context contains the match: {:?}",
        String::from_utf8_lossy(&ctx)
    );
    let ctx_off = m["context_offset"].as_u64().expect("context_offset");
    assert!(
        ctx_off <= truth && ctx_off + ctx.len() as u64 >= truth + 6,
        "context_offset places the context around the match: ctx_off={ctx_off} truth={truth}"
    );
}

/// §10 clause 7: an armed wait counts toward the endpoint's mirror activity exactly
/// as an open tap does — "an armed wait on a ring-off, untapped endpoint still
/// observes".
///
/// This is the clause with the sharpest failure mode: get it wrong and the wait is
/// armed on a feed the producer has been told nobody wants, so it times out on a
/// console that was talking the whole time. The endpoint here has `replay_ring = 0`
/// and no tap, which is the only shape in which the wait itself must flip the mirror.
#[test]
fn an_armed_wait_observes_a_ring_off_untapped_endpoint() {
    let Some(e) = Echo::start(Some(0)) else {
        eprintln!(
            "SKIP an_armed_wait_observes_a_ring_off_untapped_endpoint: no serial device \
             on this platform"
        );
        return;
    };
    // No tap is opened anywhere in this test, and the ring is off: before the wait is
    // armed, this endpoint's feed is inactive by construction.
    let mut conn = WaitConn::connect(&e.daemon.socket());
    let id = park(
        &mut conn,
        "usb0",
        json!([literal("quiet", b"HEARD-ME")]),
        // Short enough that a *broken* mirror answers `timed_out: true` inside
        // MATCH_BOUND, so the fail-first failure reads as the assertion below rather
        // than as "the wait never answered" (which is a weaker, ambiguous message).
        5_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(e.rpc()).is_empty()),
        "the wait must be armed before the bytes it is meant to hear"
    );
    let ep = &e.rpc().state()["endpoints"][0];
    assert_eq!(ep["taps"], json!(0), "no tap is open: {ep}");
    assert_eq!(ep["waits"], json!(1), "the wait is the only observer: {ep}");

    e.emit("HEARD-ME");

    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("the wait must answer");
    assert_eq!(
        resp["result"]["timed_out"],
        json!(false),
        "an armed wait observes even with no ring and no tap — otherwise the mirror \
         was never switched on for it: {resp}"
    );
    assert_eq!(
        resp["result"]["matched"]["pattern"],
        json!("quiet"),
        "{resp}"
    );
}

/// One node's `state` counter, or 0 if the node or the field is absent.
fn node_u64(rpc: &Rpc, node: &str, field: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| n.get(field).and_then(Value::as_u64))
        .unwrap_or(0)
}

/// A lone host-facing endpoint with **no graph consumer at all** — no edge, no pty,
/// ring off — so every hostward byte the reader takes off the device lands on
/// `discarded_unattached` (§5). `arbitration = "free-for-all"` so a control-plane
/// `send` can drive the echo double without holding a lock.
///
/// This is the fixture §10 clause 7's second half needs and the [`Echo`] fixture
/// cannot provide: `Echo` attaches a `console` pty, so `discarded_unattached` is
/// structurally 0 there whatever the mirror does, and an assertion over it would pass
/// just as happily with the counter deleted.
struct Unattached {
    daemon: Daemon,
    _echo: serial_nexus_itest::SerialEcho,
}

impl Unattached {
    fn start() -> Option<Unattached> {
        let echo = serial_echo()?;
        let d = Daemon::start();
        let cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
arbitration = "free-for-all"
replay_ring = 0
"#,
            device = echo.device().display(),
        );
        d.rpc()
            .load_toml(&cfg, false)
            .expect("load the unattached endpoint");
        assert!(
            d.rpc()
                .wait_status("usb0", "active", Duration::from_secs(10)),
            "usb0 not active: {:?}",
            d.rpc().node("usb0")
        );
        Some(Unattached {
            daemon: d,
            _echo: echo,
        })
    }

    fn rpc(&self) -> &Rpc {
        self.daemon.rpc()
    }

    /// Write one line targetward and wait for `discarded_unattached` to account for
    /// the echo coming back, returning how much it grew by.
    fn emit_and_measure(&self, line: &str) -> u64 {
        let before = node_u64(self.rpc(), "usb0", "discarded_unattached");
        self.rpc()
            .send("usb0", line, false, 5_000)
            .unwrap_or_else(|e| panic!("send {line:?}: [{}] {}", e.code, e.message));
        let want = before + line.len() as u64 + 1; // `send` appends \n
        assert!(
            wait_until(MATCH_BOUND, || {
                node_u64(self.rpc(), "usb0", "discarded_unattached") >= want
            }),
            "discarded_unattached never reached {want} (it is {}) — with no graph \
             consumer every echoed byte must land there (§5)",
            node_u64(self.rpc(), "usb0", "discarded_unattached")
        );
        node_u64(self.rpc(), "usb0", "discarded_unattached") - before
    }
}

/// §10 clause 7's other half: an armed wait **never affects `discarded_unattached`**
/// — the tap/ring mirror is a spy *outside* the graph, so a wait observing an
/// endpoint must not make its bytes look consumed (§15.32's tripwire, AGENTS §4's
/// purge invariant, plan §18 item 64(b)).
///
/// Asserted by nothing until 2026-08-15: a `grep` of the acceptance battery for
/// `discarded_unattached` returned zero. **And the remedy as filed — "two lines in the
/// existing ring-off guard" — would have asserted nothing**, because that guard runs
/// on the [`Echo`] fixture, which attaches a `console` pty; with a graph consumer
/// present the counter is 0 with the mirror working, broken, or deleted. Hence
/// [`Unattached`], where the counter is the *only* place the bytes can go.
///
/// **Two arms, because "never affects" is a comparison, not a value.** The same line
/// is echoed with no wait armed and then with one armed, and the two deltas must be
/// equal. A single-arm assertion (`delta == 9`) would pin the arithmetic of this
/// fixture rather than the independence the clause promises.
///
/// **What the fail-first proof covers, and what it does not.** Planting the defect the
/// `fan_out` doc warns about — the mirror handed the unattached charge instead of the
/// graph, so a spy masks a real consumer's absence — reddens this guard *and four
/// others* (`lone_serial_discards_unattached_bytes_with_counter`,
/// `a_mid_stream_connect_captures_from_the_join_point`,
/// `an_oversize_ring_replays_its_newest_bytes_and_lands_on_the_live_edge`,
/// `tap_close_stops_the_stream_while_bytes_keep_flowing`), measured. So the *class* is
/// not unguarded, and plan §18 item 64(b)'s premise is half right: what nothing else
/// covers is the **carrier**. All four of those mirror through a tap or a ring, and a
/// wait-specific regression — `arm_wait` attaching its matcher as a hostward
/// `AttachedSink`, which is the obvious wrong way to build it and would make
/// `FanOut::live` true — is invisible to every one of them. This guard is the only
/// place that would see it. It has no plant of its own, because building that one
/// means writing the wrong implementation; say that when quoting it.
#[test]
fn an_armed_wait_never_affects_discarded_unattached() {
    let Some(e) = Unattached::start() else {
        eprintln!(
            "SKIP an_armed_wait_never_affects_discarded_unattached: no serial device \
             on this platform"
        );
        return;
    };

    // Arm 1 — the control. No wait, no tap, no ring: the mirror is off entirely.
    let quiet = e.emit_and_measure("UNWATCHED");
    assert_eq!(
        node_u64(e.rpc(), "usb0", "delivered_hostward"),
        0,
        "the fixture must have no graph consumer, or this test is measuring nothing"
    );

    // Arm 2 — the same line, the same length, with a wait armed over it. Arming the
    // wait switches the endpoint's mirror ON (that is the clause's first half, which
    // `an_armed_wait_observes_a_ring_off_untapped_endpoint` covers), so this is
    // exactly the state in which a mirror that suppressed the counter would show.
    let mut conn = WaitConn::connect(&e.daemon.socket());
    let id = park(
        &mut conn,
        "usb0",
        json!([literal("watched", b"WATCHEDXX")]),
        5_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(e.rpc()).is_empty()),
        "the wait must be armed before the bytes it is meant to hear"
    );
    let watched = e.emit_and_measure("WATCHEDXX");

    // The wait heard them — so the mirror really was carrying this traffic, and the
    // equality below is about a live mirror rather than an inert one.
    let resp = conn
        .response(id, MATCH_BOUND)
        .expect("the wait must answer");
    assert_eq!(
        resp["result"]["timed_out"],
        json!(false),
        "the armed wait did not observe the bytes, so the comparison below would be \
         between two unmirrored runs: {resp}"
    );

    assert_eq!(
        watched, quiet,
        "an armed wait moved `discarded_unattached`: {quiet} B were charged for an \
         unwatched line and {watched} B for a watched one of the same length. The \
         tap/ring mirror is a spy OUTSIDE the graph (§5, §15.32, §10 clause 7): it \
         must neither suppress the unattached charge (a mirror counted as a consumer \
         — the bytes were still lost to the graph) nor add to it (the mirror is not \
         a second discard). `runtime::fan_out` is where that charge is made, and its \
         `sinks` argument must carry graph sinks only."
    );
}

/// The CLI's match path, end to end: exit `0`, and the verdict on stdout.
#[test]
fn ctl_exits_zero_when_the_pattern_matches() {
    let Some(e) = Echo::start(Some(65_536)) else {
        eprintln!(
            "SKIP ctl_exits_zero_when_the_pattern_matches: no serial device on this platform"
        );
        return;
    };
    e.emit("READY-TOKEN");
    // `--replay` so the CLI does not race the emit above: the marker is already in the
    // ring, which is exactly the case an operator uses `--replay` for.
    let out = Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(e.daemon.socket())
        .args([
            "tap-wait",
            "usb0",
            "--pattern",
            "ready=READY-TOKEN",
            "--replay",
            "--timeout-ms",
            "10000",
        ])
        .output()
        .expect("run serial-nexus-ctl tap-wait");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a match exits 0 — stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("matched") && stdout.contains("ready"),
        "the rendering names the pattern that fired: {stdout:?}"
    );
}

/// The other half of §15.20 clause 6 / CTRLW-1, which the refusal guard above does not
/// reach: while a wait is parked, this connection's **tap stream keeps flowing**.
///
/// The refusal guard proves the wait survives a pipelined request. This proves the
/// connection is not blacked out for the wait's duration — the failure CTRLW-1
/// recorded, where a parked verb left a §17 browser's console dark for as long as the
/// wait lasted and its bytes were really lost past the 128-slot tap queue, not merely
/// late. All three things ride **one** connection here, as a browser's do.
#[test]
fn tap_data_keeps_flowing_on_the_connection_whose_wait_is_parked() {
    let Some(e) = Echo::start(None) else {
        eprintln!(
            "SKIP tap_data_keeps_flowing_on_the_connection_whose_wait_is_parked: no serial \
             device on this platform"
        );
        return;
    };
    let mut conn = WaitConn::connect(&e.daemon.socket());

    // Subscribe and tap on the *same* connection the wait will park on.
    let sub_id = conn.send("subscribe", Value::Null);
    assert_eq!(
        conn.response(sub_id, Duration::from_secs(5))
            .expect("subscribe is answered")["result"]["subscribed"],
        json!(true)
    );
    let tap_id = conn.send("tap.open", json!({ "endpoint": "usb0", "replay": false }));
    let ack = conn
        .response(tap_id, Duration::from_secs(5))
        .expect("tap.open is answered");
    assert!(ack["result"]["tap"].as_u64().is_some(), "{ack}");

    // Park a wait for something that will never arrive, so the whole measurement
    // window happens while it is parked.
    let wait_id = park(
        &mut conn,
        "usb0",
        json!([literal("never", b"NEVER-EMITTED")]),
        6_000,
        false,
    );
    assert!(
        wait_until(Duration::from_secs(5), || !state_waits(e.rpc()).is_empty()),
        "the wait must be parked before the bytes that must flow past it"
    );
    // Everything read from here on was written by the daemon *while* the wait was
    // parked: `response` stashes non-matching lines rather than dropping them, and the
    // only request outstanding is the wait itself.
    conn.take_pending();

    e.emit("bytes-during-the-wait");

    // The wait rides its deadline out; the tap traffic that arrived meanwhile is in
    // `pending`.
    let resp = conn
        .response(wait_id, MATCH_BOUND)
        .expect("the wait must answer at its deadline");
    assert_eq!(resp["result"]["timed_out"], json!(true), "{resp}");

    let seen = conn.take_pending();
    let tap_bytes: Vec<u8> = seen
        .iter()
        .filter(|v| v.get("method").and_then(Value::as_str) == Some("tap.data"))
        .filter_map(|v| v["params"]["data"].as_str())
        .flat_map(|d| serial_nexus_rpc::base64_decode(d).expect("valid base64"))
        .collect();
    assert!(
        contains(&tap_bytes, b"bytes-during-the-wait"),
        "tap.data must keep arriving on the connection whose verb is parked — a parked \
         wait that blacks out its own connection is CTRLW-1 all over again. Saw {} \
         notification(s), {} tap byte(s): {:?}",
        seen.len(),
        tap_bytes.len(),
        String::from_utf8_lossy(&tap_bytes)
    );
    // And the subscription kept running too: state snapshots are the floor (§10).
    assert!(
        seen.iter()
            .any(|v| v.get("method").and_then(Value::as_str) == Some("state")),
        "the subscription must keep delivering past a parked wait as well: {seen:?}"
    );
}
