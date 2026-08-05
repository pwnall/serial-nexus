//! Harness self-checks that need a live daemon (design §5's anti-tautology rule: a
//! broken harness must fail loudly, never pass vacuously).
//!
//! The three cases the harness itself can get wrong live in `serial_nexus_itest`'s own
//! unit tests, over a socket pair, because they need a peer that misbehaves on purpose.
//! This one needs the opposite — a *correct* daemon, answering a stream request it is
//! right to refuse — so it lives here.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::json;
use serial_nexus_itest::{Daemon, TempRun, wait_until};

/// **Review 37, 37-TEST-4.** [`serial_nexus_itest::Rpc::stream`] used to swallow the
/// stream verb's ack with `let _ =`, so a daemon that *refused* the subscribe or the
/// tap handed back a live-looking [`serial_nexus_itest::Subscription`] that then yielded
/// nothing. The test downstream reported "timed out", which sends diagnosis to the tap
/// pipeline, the poll loop, the runtime — everywhere except the refusal that was
/// already sitting in the discarded line.
///
/// A refusal is easy to arrange honestly: no graph is loaded, so there is no
/// host-facing endpoint to tap and `tap.open` answers with an error, exactly as §10
/// says it should.
#[test]
#[should_panic(expected = "refused by the daemon")]
fn a_stream_the_daemon_refuses_fails_loudly_rather_than_timing_out_later() {
    let d = Daemon::start();
    let _sub = d
        .rpc()
        .stream("tap.open", json!({ "endpoint": "no-such-endpoint" }));
}

/// The positive control for the guard above: an accepted stream still works, so the
/// ack parsing rejects refusals rather than everything.
#[test]
fn an_accepted_stream_still_opens() {
    let d = Daemon::start();
    let mut sub = d.rpc().subscribe();
    // The daemon publishes a state snapshot on a tick, and `subscribe` is what turns
    // that tick from a no-op into traffic (§10), so a notification arriving at all is
    // proof the stream is live rather than a `Subscription` over a dead ack.
    let note = sub
        .wait_for(std::time::Duration::from_secs(10), |v| {
            v.get("method").and_then(|m| m.as_str()) == Some("state")
        })
        .expect("subscribe yielded no state notification within 10s");
    assert!(
        note.get("params").is_some(),
        "a state notification must carry params: {note}"
    );
}

/// A child killed and reaped on drop, so a panicking assertion in the parent test does
/// not itself leak the fixture this file is about.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Is `pid` a live process? `kill -0` rather than a raw `kill(2)`: everything outside
/// `serial_nexus_sys` is `unsafe`-free (§16.3) and the harness already borrows `kill(1)`
/// for signals.
fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The child half of [`a_sigkilled_test_process_leaves_no_daemon`]. libtest runs every
/// `#[test]` in a binary, so this is a no-op unless it was re-invoked as the fixture;
/// `SNX_ORPHAN_FIXTURE` is both the trigger and the channel for what it reports.
#[test]
fn orphan_leash_fixture() {
    let Ok(report) = std::env::var("SNX_ORPHAN_FIXTURE") else {
        return; // an ordinary suite run: nothing to do
    };
    let d = Daemon::start();
    std::fs::write(&report, format!("{}\n{}\n", d.pid(), d.socket().display()))
        .expect("report the daemon's pid and socket");
    // Block until the parent kills us. Nothing here may run `Drop` — a parent that dies
    // without unwinding is the entire point.
    std::thread::sleep(Duration::from_secs(300));
    unreachable!("the orphan fixture was supposed to be killed long before this");
}

/// **A daemon never outlives the test process that spawned it, even when that process
/// dies without unwinding** — the orphan leash (design §15.43).
///
/// `Daemon`'s `Drop` covers the ordinary exits: a passing test, a panicking one. It
/// covers none of the others — a SIGKILL, an `abort`, a runner killing the process group
/// on a timeout. What survives one of those is a daemon holding a control socket nothing
/// will dial again and, if its graph opened any, real devices under `TIOCEXCL`; the
/// *next* run of every test wanting those devices then fails, in a place that says
/// nothing about the run that leaked. One whole-gate Mac run lost all five rig tests to
/// exactly this.
///
/// The fixture is this same binary re-invoked, so the daemon under test comes up through
/// exactly the `Daemon::start` path every other test uses, rather than a hand-rolled
/// imitation that could be handed a leash the real one lacks.
#[test]
fn a_sigkilled_test_process_leaves_no_daemon() {
    let run = TempRun::new();
    let report = run.join("fixture.report");
    let exe = std::env::current_exe().expect("current_exe");

    let mut fixture = KillOnDrop(
        Command::new(&exe)
            .arg("orphan_leash_fixture")
            .arg("--exact")
            .arg("--test-threads=1")
            .env("SNX_ORPHAN_FIXTURE", &report)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("re-invoke this test binary as the orphan fixture"),
    );

    assert!(
        wait_until(Duration::from_secs(60), || report.exists()),
        "the orphan fixture never reported a daemon: it may not have started one"
    );
    let reported = std::fs::read_to_string(&report).expect("read the fixture's report");
    let mut lines = reported.lines();
    let pid: u32 = lines
        .next()
        .and_then(|l| l.trim().parse().ok())
        .unwrap_or_else(|| panic!("fixture report has no pid: {reported:?}"));
    let socket = PathBuf::from(lines.next().expect("fixture report has no socket path"));
    assert!(
        pid_alive(pid),
        "the fixture's daemon {pid} was already gone before the kill; this test would \
         prove nothing"
    );

    // The mechanism, exactly: the fixture dies without unwinding, so none of its `Drop`
    // impls, `atexit` handlers, or signal arms run.
    let killed = Command::new("kill")
        .arg("-9")
        .arg(fixture.0.id().to_string())
        .status()
        .expect("run kill(1)");
    assert!(killed.success(), "`kill -9` on the orphan fixture failed");
    let _ = fixture.0.wait();

    assert!(
        wait_until(Duration::from_secs(30), || !pid_alive(pid)),
        "daemon {pid} outlived the SIGKILLed test process that spawned it; it still \
         holds {} and every device its graph had opened",
        socket.display()
    );
    // The socket check is the half that pid reuse cannot fool, and it is also evidence
    // the daemon took its *clean* teardown path (§10 unlinks the socket) rather than
    // merely dying.
    assert!(
        wait_until(Duration::from_secs(5), || !socket.exists()),
        "the daemon's control socket {} was never unlinked: it did not take the clean \
         teardown path",
        socket.display()
    );

    // The fixture died before its own `TempRun` could sweep — that is the point; this
    // test owns the sweeping.
    if let Some(dir) = socket.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}
