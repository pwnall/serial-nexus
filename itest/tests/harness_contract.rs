#![forbid(unsafe_code)]

//! Harness self-checks that need a live daemon (design §5's anti-tautology rule: a
//! broken harness must fail loudly, never pass vacuously).
//!
//! The three cases the harness itself can get wrong live in `serial_nexus_itest`'s own
//! unit tests, over a socket pair, because they need a peer that misbehaves on purpose.
//! This one needs the opposite — a *correct* daemon, answering a stream request it is
//! right to refuse — so it lives here.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::json;
use serial_nexus_itest::{
    Daemon, KillOnDrop, RawDaemon, Sim, TempRun, WebServer, bin, daemon_answers, pid_alive,
    wait_until,
};

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

/// The [`serial_pair_or_rig`] provider table (notes §3.43 repair (b)).
///
/// The seam it encodes is "software wins whenever it exists; the rig is a fallback for
/// the platform where it does not" — and that is exactly the sentence a future
/// simplification inverts. Checking it through the pure decision function is what makes
/// the guard portable: the table is identical on a box with no hardware, where the
/// provider itself can only ever return one of its arms.
///
/// Fail-first (2026-08-05): swapping the first two arms of `choose_pair_source` so a
/// visible rig outranks the software double turns the third assertion red with
/// `a visible rig must not displace the software double, Rig != Software`.
#[test]
fn the_pair_provider_prefers_software_and_falls_back_to_the_rig() {
    use serial_nexus_itest::{PairChoice, choose_pair_source};

    // No provider at all: the caller self-skips.
    assert_eq!(
        choose_pair_source(false, false, false),
        PairChoice::Skip,
        "no software double and no rig must be a skip"
    );
    // Off Linux with a rig attached — the case this repair exists for.
    assert_eq!(
        choose_pair_source(false, true, false),
        PairChoice::Rig,
        "with no software double, a visible rig must be used"
    );
    // On a box that has both — which is any Linux box with SNX_CROSSOVER_A/_B exported
    // to run the rig suite. The rig must NOT be commandeered as a side effect.
    assert_eq!(
        choose_pair_source(true, true, false),
        PairChoice::Software,
        "a visible rig must not displace the software double"
    );
    // The forcing knob, so the fallback arm is exercisable where it is not the default.
    assert_eq!(
        choose_pair_source(true, true, true),
        PairChoice::Rig,
        "SNX_SERIAL_PAIR=rig must force the rig arm even where software exists"
    );
    // Forced with nothing to force onto: a hard failure, never a silent fallback.
    assert_eq!(
        choose_pair_source(true, false, true),
        PairChoice::ForcedRigMissing,
        "SNX_SERIAL_PAIR=rig with no rig must fail, not quietly run the software double"
    );
}

/// The child half of [`a_sigkilled_test_process_leaves_no_daemon`]. libtest runs every
/// `#[test]` in a binary, so this is a no-op unless it was re-invoked as the fixture;
/// `SNX_ORPHAN_FIXTURE` is both the trigger and the channel for what it reports.
///
/// It starts **one child per leashed spawn path in the harness** — `Daemon::start`,
/// `RawDaemon` (the eight `spawn_daemon` copies and four one-off wrappers now behind
/// it, plan §18 items 50 and 24), and `Sim::spawn` — plus one deliberately *unleashed*
/// `Sim`, which is the control: §15.43's leash is opt-in, and a mechanism that fired
/// without being asked would be a different, worse defect. The report is one line per
/// field, in the order the parent reads them.
#[test]
fn orphan_leash_fixture() {
    let Ok(report) = std::env::var("SNX_ORPHAN_FIXTURE") else {
        return; // an ordinary suite run: nothing to do
    };
    let d = Daemon::start();
    let raw_run = TempRun::new();
    let raw = RawDaemon::start(&raw_run);
    // Long-lived doubles: nothing here may self-terminate before the parent has looked,
    // so the timeout is far past the parent's own deadlines.
    let leashed_sim = Sim::spawn(&["pty", "--echo", "--timeout-ms", "600000"], None);
    let unleashed_sim = Sim::spawn_unleashed(&["pty", "--echo", "--timeout-ms", "600000"], None);
    std::fs::write(
        &report,
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n",
            d.pid(),
            d.socket().display(),
            raw.pid(),
            raw.socket().display(),
            leashed_sim.pid(),
            unleashed_sim.pid(),
        ),
    )
    .expect("report the children this fixture started");
    // Block until the parent kills us. Nothing here may run `Drop` — a parent that dies
    // without unwinding is the entire point. The `raw_run` binding is held so its temp
    // directory is not swept out from under the daemon named in the report.
    let _held = (&raw_run, &leashed_sim, &unleashed_sim);
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
    let fields: Vec<&str> = reported.lines().collect();
    assert_eq!(
        fields.len(),
        6,
        "the fixture report is not the six lines this test reads: {reported:?}"
    );
    let pid_at = |i: usize, what: &str| -> u32 {
        fields[i]
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("fixture report has no {what} pid: {reported:?}"))
    };
    let pid = pid_at(0, "Daemon::start");
    let socket = PathBuf::from(fields[1]);
    let raw_pid = pid_at(2, "RawDaemon");
    let raw_socket = PathBuf::from(fields[3]);
    let sim_pid = pid_at(4, "leashed Sim");
    let unleashed_pid = pid_at(5, "unleashed Sim");
    for (what, p) in [
        ("daemon", pid),
        ("raw daemon", raw_pid),
        ("sim double", sim_pid),
        ("unleashed sim double", unleashed_pid),
    ] {
        assert!(
            pid_alive(p),
            "the fixture's {what} {p} was already gone before the kill; this test would \
             prove nothing"
        );
    }

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
    // The same claim for the *other* two leashed spawn paths, which had no leash at all
    // until plan §18 items 50 and 24: every `spawn_daemon` copy and every one-off wrapper
    // booted a daemon without one, and `Sim` was uncovered outright.
    assert!(
        wait_until(Duration::from_secs(30), || !pid_alive(raw_pid)),
        "the RawDaemon {raw_pid} outlived the SIGKILLed test process that spawned it; \
         it still holds {}",
        raw_socket.display()
    );
    assert!(
        wait_until(Duration::from_secs(30), || !pid_alive(sim_pid)),
        "the Sim double {sim_pid} outlived the SIGKILLed test process that spawned it; \
         it still holds a pty master, and its --timeout-ms is ten minutes away"
    );

    // **The control, and it is not decorative.** §15.43's leash is opt-in; a leash that
    // fired without being asked would stop a double the moment anything handed it a null
    // stdin, which is what `Command::output()` does and why `Sim::client` must never pass
    // the flag. So the unleashed double must still be running here — and this test kills
    // it itself, because nothing else will.
    assert!(
        pid_alive(unleashed_pid),
        "the *unleashed* Sim double {unleashed_pid} died with its parent: the leash is \
         firing without being opted into, which would end any double spawned with a \
         closed or null stdin (§15.43)"
    );
    let _ = Command::new("kill")
        .arg("-9")
        .arg(unleashed_pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    // The socket check is the half that pid reuse cannot fool, and it is also evidence
    // the daemon took its *clean* teardown path (§10 unlinks the socket) rather than
    // merely dying.
    for socket in [&socket, &raw_socket] {
        assert!(
            wait_until(Duration::from_secs(5), || !socket.exists()),
            "the daemon's control socket {} was never unlinked: it did not take the \
             clean teardown path",
            socket.display()
        );
    }

    // The fixture died before its own `TempRun`s could sweep — that is the point; this
    // test owns the sweeping.
    for socket in [&socket, &raw_socket] {
        if let Some(dir) = socket.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

// ---------------------------------------------------------------------------
// The shared child scaffolding, self-tested (design §16.5, "assertion helpers are
// shared and self-tested"; plan §18 item 50).
//
// Each of these pins the one property whose quiet loss would look exactly like the
// helper still working — which is the whole reason the item asks for self-tests
// rather than for the consolidation alone.
// ---------------------------------------------------------------------------

/// **[`KillOnDrop`] really kills.**
///
/// A `std::process::Child`'s own `Drop` neither signals nor reaps, so this guard is the
/// only thing standing between a panicking assertion and a leaked subprocess — in
/// fifteen files before it was shared. Gut its `Drop` body and nothing else in the
/// suite changes colour: every test that uses it still passes, and the leak shows up in
/// the *next* run as a device already held.
///
/// Fail-first: replacing the `Drop` body with `{}` fails here with
/// `the KillOnDrop guard's child <pid> is still alive after the guard was dropped`.
#[test]
fn a_killondrop_guard_really_kills_its_child() {
    // A double with a ten-minute timeout, so "it is gone" can only mean the guard
    // killed it — never that it finished on its own while this test looked away.
    let guard = KillOnDrop(
        Command::new(bin("serial-nexus-sim"))
            .args(["pty", "--echo", "--timeout-ms", "600000"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the sim double"),
    );
    let pid = guard.id();
    assert!(
        wait_until(Duration::from_secs(10), || pid_alive(pid)),
        "the sim double {pid} never came up, so this test would prove nothing"
    );

    drop(guard);

    assert!(
        wait_until(Duration::from_secs(10), || !pid_alive(pid)),
        "the KillOnDrop guard's child {pid} is still alive after the guard was dropped"
    );
}

/// **[`RawDaemon::start`] returns a daemon that is already answering.**
///
/// The seven `wait_socket` copies it replaced stopped at `UnixStream::connect(..).is_ok()`,
/// which a bound listener satisfies before the daemon is serving. Every caller's next
/// line is an RPC, so the readiness wait is load-bearing and its failure mode is a
/// timing-dependent error from nowhere — the shape that reads as a flake rather than as
/// a defect in the harness.
///
/// The assertion is deliberately *un*retried: one `info` call, immediately, with no
/// `wait_until` around it. That is exactly the promise `start` makes.
///
/// Fail-first: weakening `wait_daemon_ready` to `socket.exists()` makes this fail with
/// the RPC error from the un-served socket. (Weakening it to a bare connect is the
/// *other* half of the pair and is pinned deterministically by
/// `a_listener_that_accepts_but_never_answers_is_not_ready` in the library's own unit
/// tests, where a silent listener can be arranged on purpose.)
#[test]
fn a_raw_daemon_answers_rpc_the_instant_start_returns() {
    let run = TempRun::new();
    let d = RawDaemon::start(&run);
    let info = d.rpc().ok("info", json!({}));
    assert!(
        info.get("daemon_version").is_some(),
        "RawDaemon::start returned before the daemon could answer `info`: {info}"
    );
    assert!(
        daemon_answers(d.socket()),
        "the socket RawDaemon::start reports is not the one the daemon answers on"
    );
    assert!(
        pid_alive(d.pid()),
        "RawDaemon::pid does not name a live process"
    );
}

/// **A restart on the same socket and state file works through the shared spawner** —
/// the capability the eight private `spawn_daemon` copies existed for, and the one a
/// consolidation onto `Daemon::start` (fresh temp dir per call) would have silently
/// dropped.
#[test]
fn a_raw_daemon_can_be_killed_and_restarted_on_the_same_socket() {
    let run = TempRun::new();
    let mut first = RawDaemon::start(&run);
    let first_pid = first.pid();
    first.kill();
    assert!(
        wait_until(Duration::from_secs(10), || !pid_alive(first_pid)),
        "RawDaemon::kill left {first_pid} running"
    );

    // The hard kill leaves a stale socket file behind, which is precisely the state
    // `test -S` would have called "ready" and the connect probe correctly refuses.
    let second = RawDaemon::start(&run);
    assert_ne!(second.pid(), first_pid, "the restart reused the dead pid?");
    assert!(
        second
            .rpc()
            .ok("info", json!({}))
            .get("daemon_version")
            .is_some(),
        "the restarted daemon does not answer on the reclaimed socket"
    );
}

/// **[`WebServer::start`] returns only once the port it reports is accepting.**
///
/// The seven copies it replaced all scraped the bound URL out of the child's stdout,
/// and the value of doing that — rather than sleeping — is that the port is live when
/// the scan succeeds. So the assertion is an un-retried TCP connect: a `start` that
/// returned early, or reported a port it read from the wrong line, fails here rather
/// than three HTTP helpers downstream with `Connection refused`.
///
/// No daemon: the console answers its own assets long before anything reaches the
/// control socket, so this is a pure test of the boot scaffolding.
///
/// Fail-first: returning `port + 1` from `WebServer::start` fails here with
/// `the port WebServer::start reported is not accepting`.
#[test]
fn a_web_server_start_returns_a_port_that_already_accepts() {
    let run = TempRun::new();
    let server = WebServer::start(
        "127.0.0.1:0",
        "harnesstoken0123456789abcdef",
        &run.socket(),
        run.path(),
        &[],
    );
    let port = server.port();
    assert!(port != 0, "WebServer::start reported port 0");
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok(),
        "the port WebServer::start reported is not accepting: 127.0.0.1:{port}"
    );
}
