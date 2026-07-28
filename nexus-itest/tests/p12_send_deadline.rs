//! Review 32 arbitration guards: `send`'s deadline covers the *write*, and a waiter
//! that loses its edge is told so (design §6, §15.20, §15.35).
//!
//! Two defects, one theme — a verb that cannot complete must leave the endpoint
//! exactly as it found it, and must say something true on the way out.
//!
//! * `CONC-2` — `send` is specified as "acquire-with-timeout, write, and release as
//!   one atomic daemon-side operation … failing with the locked error at its
//!   deadline", but `timeout_ms` bounded only the acquire. The write into the
//!   endpoint's 256-deep targetward channel was unbounded, so a target that stopped
//!   draining parked the verb *forever* while its transient origin still held the
//!   exclusive write lock: `state` showed a phantom `holder: "send"` and every other
//!   origin was refused `-32003`, with nothing daemon-side to end it.
//! * `CTRL-2` — `EndpointLock::acquire` answered two different states with one word,
//!   so a parked `lock --wait` whose edge had just been `disconnect`ed was told its
//!   origin was `write=never`. That is a claim about configuration, and it was false;
//!   it sent whoever debugged it looking for a value that had never existed.
//!
//! Both are device-free and run on every platform: a serial node with an absent
//! device comes up `waiting` and parks its targetward receiver unread (§15.8), which
//! is a saturated channel with no UART in sight, and the `map`/pty shapes need no
//! device at all. Assertions are on structured RPC results only (§5).
//!
//! The one exception is CONC-2's *second* half —
//! [`a_send_that_times_out_delivers_none_of_its_bytes`], added after the review-32 audit
//! observed that nothing here ever checked the claim the refusal makes ("nothing was
//! sent"). Watching the bytes not arrive needs a target that resumes draining, so that
//! one is Linux-only and self-skips (§5), and its ground truth is a `log` file rather
//! than a counter.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Rpc, serial_echo, wait_until};
use serde_json::Value;

/// `AppError::Locked` — `APP_ERROR_BASE (-32000) - 3` (nexus-rpc `AppError`).
const LOCKED: i64 = -32003;
/// The JSON-RPC standard invalid-params code, which `lock`'s non-contention
/// refusals carry (docs/rpc/arbitration.md).
const INVALID_PARAMS: i64 = -32602;

/// One pty origin writing toward one serial endpoint whose device is absent, so the
/// node comes up `waiting` and never drains what is sent at it — the flow-control
/// stall of the finding, reachable without hardware.
fn stalled_endpoint_daemon() -> Daemon {
    let d = Daemon::start();
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "con"
path = "{con}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "con"
"#,
        con = d.run().join("ttyCon").display(),
        dev = d.run().join("absent-device").display(),
    );
    d.rpc().load_toml(&cfg, false).expect("load stalled graph");
    d
}

/// The lock holder reported on a host-facing endpoint in `state`: the origin's name,
/// or `Value::Null` when the lock is free.
fn holder(rpc: &Rpc, endpoint: &str) -> Value {
    rpc.node(endpoint)
        .and_then(|n| n.pointer("/lock/holder").cloned())
        .unwrap_or(Value::Null)
}

/// The origin names currently queued for an endpoint's lock, in FIFO order.
fn waiters(rpc: &Rpc, endpoint: &str) -> Vec<String> {
    rpc.node(endpoint)
        .and_then(|n| n.pointer("/lock/waiters").cloned())
        .and_then(|w| w.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|w| w.as_str().map(str::to_owned))
        .collect()
}

/// CONC-2: a `send` whose delivery cannot land must fail at *its own* deadline and
/// leave no holder behind.
///
/// The shape matters as much as the timing. The endpoint is `exclusive`, so while a
/// `send` holds it nobody else may write; the old code held it for as long as the
/// target stayed stalled, which is unbounded. The guard therefore checks three
/// things together: the verb returns, it returns at roughly the deadline the
/// operator named (not instantly — that would be a different failure — and not much
/// later), and the endpoint is free the moment it does.
#[test]
fn a_send_into_a_saturated_endpoint_fails_at_its_deadline_and_leaves_no_holder() {
    let d = stalled_endpoint_daemon();
    let rpc = d.rpc();
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "usb0 should be waiting on an absent device: {:?}",
        rpc.node("usb0")
    );

    // Fill the endpoint's targetward channel (`runtime::CHANNEL_CAP` = 256 chunks).
    // Each `send` registers a transient origin, takes the lock, delivers, and leaves,
    // so these all succeed until the channel is full — the sends land in a queue the
    // parked serial node never drains.
    const TIMEOUT_MS: u64 = 700;
    let mut accepted = 0usize;
    let mut refusal = None;
    let mut elapsed = Duration::ZERO;
    for i in 0..400 {
        let started = Instant::now();
        match rpc.send("usb0", &format!("line-{i}"), false, TIMEOUT_MS) {
            Ok(_) => accepted += 1,
            Err(e) => {
                elapsed = started.elapsed();
                refusal = Some(e);
                break;
            }
        }
    }

    let err = refusal.unwrap_or_else(|| {
        panic!("400 sends all succeeded into a stalled endpoint; the channel never filled")
    });
    // The channel really is what filled: a failure in the first handful of sends
    // would mean something else refused us, and the timing assertion below would then
    // be measuring the wrong thing.
    assert!(
        accepted >= 200,
        "only {accepted} sends landed before the refusal; the 256-deep targetward \
         channel is not what filled ({}: {})",
        err.code,
        err.message
    );
    assert_eq!(
        err.code, LOCKED,
        "a backpressured send must fail with the locked error (§6): [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("backpressure"),
        "the refusal must name the cause an operator can act on: {:?}",
        err.message
    );
    // It waited for its deadline rather than failing fast, and it did not blow past
    // it. The upper bound is deliberately loose — this box is shared and can be
    // loaded — but the pre-fix behaviour was *unbounded*, so any finite bound fails
    // it. The lower bound proves the deadline is the thing that ended the wait.
    assert!(
        elapsed >= Duration::from_millis(TIMEOUT_MS / 2),
        "the send failed after {elapsed:?}, far short of its {TIMEOUT_MS}ms deadline — \
         it did not wait for the write at all"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "the send took {elapsed:?} to honour a {TIMEOUT_MS}ms deadline"
    );

    // The whole point (§6): the transient origin is gone and the endpoint is free.
    // Before the fix `state` reported `holder: "send"` for as long as the target
    // stayed stalled, and every other origin was locked out behind a phantom.
    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc, "usb0")
            == Value::Null),
        "the endpoint is still held after the send gave up: {:?}",
        rpc.node("usb0")
    );
    let origins: Vec<String> = rpc
        .node("usb0")
        .and_then(|n| n.pointer("/lock/origins").cloned())
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|o| o.get("origin").and_then(Value::as_str).map(str::to_owned))
        .collect();
    assert!(
        !origins.iter().any(|o| o == "send"),
        "a timed-out send left its transient origin registered: {origins:?}"
    );
    // And the endpoint is genuinely usable again, not merely reported free: the
    // configured origin can take the lock it was locked out of.
    rpc.lock("con", false, false, None)
        .expect("the surviving origin can take the freed lock");
    assert_eq!(holder(rpc, "usb0"), Value::String("con".into()));
}

/// CONC-2, the half the timing guard above cannot see: a `send` that fails at its
/// deadline **delivers nothing**.
///
/// The refusal says so in as many words ("nothing was sent"), and the review pinned the
/// byte count as exact — but nothing in the suite observed the wire. The property holds
/// because `mpsc::Sender::send` reserves a permit and writes synchronously, so a future
/// dropped at the deadline enqueued nothing; that is tokio's implementation, not this
/// project's, and a later switch to `send_timeout`, a `PollSender`, or a hand-rolled
/// reserve-then-send would break it in silence. So look.
///
/// Observing it needs a target that **stops draining and then starts again**, which the
/// device-free shape above cannot do: its serial node is `waiting` on a device that never
/// appears, so the backlog is never written and there is nothing to inspect. Here the
/// device is a symlink created *after* the deadline has passed — the port an operator
/// plugs back in — and `purge_on_reconnect = false` keeps §7.1 from discarding the very
/// backlog this test is about. Everything the node then writes comes back off the echo
/// double and into a `log`, which is the suite's lossless ground truth (§5).
///
/// The ordering argument that makes the absence mean something: the endpoint's targetward
/// channel is FIFO, so a chunk the timed-out `send` had managed to enqueue would sit
/// *ahead* of the marker line sent after the reconnect. Waiting for the marker to reach
/// the file therefore gives the queued bytes their full chance to arrive first. And the
/// backlog is asserted present, so "absent" cannot be satisfied by a path that dropped
/// everything.
///
/// Measured on 2026-07-27: 256 lines accepted (`line-0000`…`line-0255`, exactly
/// `runtime::CHANNEL_CAP`), `line-0256` refused, and the captured file reads
/// `line-0000 … line-0255 MARKER-AFTER-DRAIN` — the refused line absent between the two.
/// The detector was proved by pointing the final assertion at a line that *is* present
/// and watching it fail.
///
/// Linux-only: it needs a software serial device (§5, `serial_echo` is `None` elsewhere).
#[test]
fn a_send_that_times_out_delivers_none_of_its_bytes() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_send_that_times_out_delivers_none_of_its_bytes: \
             no serial device on this platform"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let capture = logdir.join("cap.log");
    // Absent for now; a symlink to the echo device appears here later.
    let late_dev = d.run().join("late-dev");

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
purge_on_reconnect = false
[[node]]
type = "pty"
name = "con"
path = "{con}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "con"
[[edge]]
a = "usb0"
b = "cap"
"#,
        dev = late_dev.display(),
        con = d.run().join("ttyCon").display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load the late-device graph");
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "usb0 should be waiting on an absent device: {:?}",
        rpc.node("usb0")
    );

    // Fill the 256-deep targetward channel, then keep going until one send hits its
    // deadline. Zero-padded so no payload is a substring of another — the whole test
    // rests on searching the captured stream for one exact line.
    const TIMEOUT_MS: u64 = 700;
    let mut last_accepted = None;
    let mut timed_out = None;
    for i in 0..400 {
        let line = format!("line-{i:04}");
        match rpc.send("usb0", &line, false, TIMEOUT_MS) {
            Ok(_) => last_accepted = Some(line),
            Err(e) => {
                assert_eq!(
                    e.code, LOCKED,
                    "a backpressured send must fail with the locked error (§6): [{}] {}",
                    e.code, e.message
                );
                assert!(
                    e.message.contains("nothing was sent"),
                    "the refusal claims nothing was sent; that claim is what this test \
                     checks, so it must be the claim being made: {:?}",
                    e.message
                );
                timed_out = Some(line);
                break;
            }
        }
    }
    let timed_out = timed_out
        .expect("400 sends all succeeded into a stalled endpoint; the channel never filled");
    let last_accepted = last_accepted.expect("no send was ever accepted");

    // Plug the port back in. The backlog is kept (`purge_on_reconnect = false`) and
    // written to the echo double, which reflects it into the log.
    std::os::unix::fs::symlink(echo.device(), &late_dev).expect("symlink the echo device");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(30)),
        "usb0 never opened the device that appeared: {:?}",
        rpc.node("usb0")
    );

    // One more line, accepted this time, behind everything already queued.
    const MARKER: &str = "MARKER-AFTER-DRAIN";
    rpc.send("usb0", MARKER, false, 10_000)
        .expect("a send into a draining endpoint is accepted");

    let captured =
        || String::from_utf8_lossy(&std::fs::read(&capture).unwrap_or_default()).into_owned();
    assert!(
        wait_until(Duration::from_secs(30), || captured().contains(MARKER)),
        "the marker never reached the log, so the queue's fate is unknown: node={:?} \
         captured={:?}",
        rpc.node("usb0"),
        captured()
    );

    let text = captured();
    // The backlog really was delivered — so an absent line is a line that was never
    // sent, not one that a purge or a dropped channel swallowed along with the rest.
    assert!(
        text.contains("line-0000") && text.contains(&last_accepted),
        "the accepted backlog did not reach the device, so this test cannot tell \
         'never sent' from 'lost': captured={text:?}"
    );
    assert!(
        !text.contains(&timed_out),
        "the send that failed at its deadline delivered {timed_out:?} anyway — its \
         refusal said 'nothing was sent' and the byte count it reported was a lie \
         (CONC-2, §6)\ncaptured={text:?}"
    );
}

/// A graph of one map node (host-facing `m`, target-facing `m/raw`) and two pty
/// origins, wired to `m`. No device anywhere, so this runs on every platform.
fn two_origin_graph(d: &Daemon) -> String {
    format!(
        r#"
[[node]]
type = "map"
name = "m"
hostward = []
targetward = []
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[node]]
type = "pty"
name = "p1"
path = "{p1}"
[[edge]]
a = "m"
b = "p0"
[[edge]]
a = "m"
b = "p1"
"#,
        p0 = d.run().join("p0").display(),
        p1 = d.run().join("p1").display(),
    )
}

/// Run `lock --wait` on `origin` from its own connection and thread, returning a
/// receiver for its eventual outcome. `Rpc` holds an `Rc`, so it cannot cross a
/// thread boundary; the socket path can, and one request per connection is what the
/// client does anyway (§10).
fn park_a_waiter(
    socket: std::path::PathBuf,
    origin: &str,
) -> mpsc::Receiver<Result<Value, nexus_itest::RpcError>> {
    let (tx, rx) = mpsc::channel();
    let origin = origin.to_owned();
    std::thread::spawn(move || {
        let outcome = Rpc::new(socket).lock(&origin, false, true, None);
        let _ = tx.send(outcome);
    });
    rx
}

/// CTRL-2: a parked `lock --wait` whose edge is cut must be told it was *detached*,
/// not that its origin is `write=never`.
///
/// Both trigger paths are checked, because the daemon reaches them through different
/// doors: `disconnect` unregisters the origin directly, and `remove-node --cascade`
/// gets there via the node's edges. The old message was identical on both, and it
/// named a configuration value (`write_mode = "never"`) that neither graph has ever
/// contained.
#[test]
fn a_waiter_whose_edge_is_disconnected_is_told_it_was_detached() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&two_origin_graph(&d), false)
        .expect("load the two-origin graph");

    // p0 takes the lock; p1 parks behind it in the FIFO queue.
    rpc.lock("p0", false, false, None).expect("p0 takes it");
    let parked = park_a_waiter(d.socket(), "p1");
    assert!(
        wait_until(Duration::from_secs(10), || waiters(rpc, "m")
            .iter()
            .any(|w| w == "p1")),
        "p1 never appeared in the endpoint's waiter queue: {:?}",
        rpc.node("m")
    );

    // Cut p1's edge out from under it. The wait must end — that half always worked —
    // with a diagnosis that is true.
    rpc.disconnect("m", "p1").expect("disconnect the waiter");
    let outcome = parked
        .recv_timeout(Duration::from_secs(15))
        .expect("the parked lock --wait never returned after its edge was disconnected");
    let err = outcome.expect_err("a detached origin cannot be granted the lock");
    assert_eq!(
        err.code, INVALID_PARAMS,
        "the refusal code is unchanged by the split (§6): [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("detached"),
        "the waiter must be told its edge went away: {:?}",
        err.message
    );
    assert!(
        !err.message.contains("write=never"),
        "the waiter was told its origin is write=never — a claim about a \
         configuration value this graph has never contained: {:?}",
        err.message
    );

    // The endpoint is otherwise untouched: p0 still holds it and the queue is empty.
    assert_eq!(holder(rpc, "m"), Value::String("p0".into()));
    assert!(
        waiters(rpc, "m").is_empty(),
        "the detached waiter is still queued: {:?}",
        rpc.node("m")
    );
}

/// CTRL-2, the second door: `remove-node --cascade` on the waiting origin's own node.
/// `docs/rpc/configuration.md` promises removal wakes parked waiters "with the
/// defined error"; the defined error must not be a false statement about write mode.
#[test]
fn a_waiter_whose_node_is_removed_is_told_it_was_detached() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&two_origin_graph(&d), false)
        .expect("load the two-origin graph");

    rpc.lock("p0", false, false, None).expect("p0 takes it");
    let parked = park_a_waiter(d.socket(), "p1");
    assert!(
        wait_until(Duration::from_secs(10), || waiters(rpc, "m")
            .iter()
            .any(|w| w == "p1")),
        "p1 never appeared in the endpoint's waiter queue: {:?}",
        rpc.node("m")
    );

    rpc.remove_node("p1", true)
        .expect("remove the waiter's node");
    let outcome = parked
        .recv_timeout(Duration::from_secs(15))
        .expect("the parked lock --wait never returned after its node was removed");
    let err = outcome.expect_err("a removed origin cannot be granted the lock");
    assert_eq!(err.code, INVALID_PARAMS, "[{}] {}", err.code, err.message);
    assert!(
        err.message.contains("detached"),
        "the waiter must be told it left the endpoint: {:?}",
        err.message
    );
    assert!(
        !err.message.contains("write=never"),
        "the waiter was told its origin is write=never, which was never true: {:?}",
        err.message
    );
    assert_eq!(holder(rpc, "m"), Value::String("p0".into()));
}

/// CTRL-2, the other half of the split: a declared `write = never` origin is refused
/// *before* it ever reaches the lock state machine, so the two states really were
/// disjoint in practice — which is what made the old shared message not merely
/// ambiguous but always wrong.
///
/// A `never` edge gets no `lock`/`unlock` address at all (`runtime`'s wiring and
/// `Daemon::connect` both withhold `origin_locks` for it), so `resolve_origin`
/// refuses first with its own accurate sentence. `WaitOutcome::ReadOnly` is
/// therefore unreachable from the `lock` verb today, and *every* `write=never`
/// message an operator could actually have seen from it was a detached origin being
/// misdescribed. The guard pins that the read-only path still refuses, and refuses
/// on its own terms.
#[test]
fn a_declared_read_only_origin_is_refused_before_it_reaches_the_lock() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "map"
name = "m"
hostward = []
targetward = []
[[node]]
type = "pty"
name = "spy"
path = "{spy}"
[[edge]]
a = "m"
b = "spy"
write_mode = "never"
"#,
        spy = d.run().join("spy").display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load the read-only graph");

    let err = rpc
        .lock("spy", false, false, None)
        .expect_err("a write=never origin cannot hold the lock");
    assert_eq!(err.code, INVALID_PARAMS, "[{}] {}", err.code, err.message);
    assert!(
        err.message.contains("not a writable origin"),
        "a declared read-only origin is refused by name resolution, on its own \
         terms: {:?}",
        err.message
    );
    assert_eq!(
        holder(rpc, "m"),
        Value::Null,
        "the refusal must not have granted anything: {:?}",
        rpc.node("m")
    );
}
