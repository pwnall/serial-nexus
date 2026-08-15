#![forbid(unsafe_code)]

//! **The exec-child orphan class, asserted over the whole fixture set** (plan §18 item
//! 65(c); design §15.43, §7.6, §15.22).
//!
//! An `exec` codec child is spawned by the *daemon*, so when the daemon is SIGKILLed
//! nothing in this process — and nothing in the daemon — is left to stop it. The child's
//! only leash is the EOF that arrives on its stdin when the daemon's write end is closed
//! by the kernel (§15.43), and a child that never reads its stdin never sees it. That is
//! not hypothetical: `p8_daemon_transcript` leaked **three processes per run** for about
//! eighty runs with the suite green every time, until item 50's agent counted 260 of them
//! on the box (notes §3.91).
//!
//! [`Daemon::drop`]'s process-group sweep catches the class from the harness side, and it
//! is only as good as the tests that happen to route through it. This file makes the
//! coverage deliberate: it enumerates **every** fixture under `tests/ext-codec/`, runs
//! each one as a live codec child, kills the daemon the way a runner or a crash kills it,
//! and requires the child to be gone. Enumerated from the directory rather than from a
//! hand-kept list (AGENTS §3), with a floor so a walker that stopped walking cannot
//! report the same green as a clean tree.
//!
//! **Fail-first, measured.** Against the unfixed `deaf.py` this guard reddens naming the
//! fixture and the surviving pid — `deaf.py` never reads a byte of its stdin *by design*
//! (it is the fixture for the merge-stage backlog item 21 measures), so the leash it was
//! documented to honour could never reach it. Its docstring claimed "nothing here
//! outlives the node that started it", which is true only where the daemon gets to run
//! code. The other five fixtures read to EOF and pass unchanged, which is what makes this
//! a guard over the class rather than one file's regression test.
//!
//! **Why the daemon is SIGKILLed rather than shut down.** On `teardown`, on `shutdown`
//! and on SIGTERM the daemon reaps its children itself, so those paths cannot observe the
//! property this file exists to hold: they prove the *supervisor* works, not that the
//! child is survivable-proof. SIGKILL is the one path where the child's own behaviour is
//! the only thing left — and it is also the path [`Daemon::drop`] takes for every test in
//! this suite.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{Daemon, skip_no_exec_codec, wait_until};

/// The workspace's `tests/ext-codec/` directory, from this crate's compile-time
/// manifest dir (the same derivation `p5_exec_crash` and `p13_teardown_accounting`
/// carry).
fn ext_codec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("serial-nexus-itest has a parent (the workspace root)")
        .join("tests")
        .join("ext-codec")
}

/// Whether `python3` is invocable — every fixture here is a Python script.
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// The floor on fixtures this walk must find. Six today
/// (`deaf`, `half-duplex`, `lag`, `passthrough`, `passthrough-codec`, `strict`); set
/// below that so an ordinary deletion does not trip it, and far above the zero a
/// walker that quietly stopped walking would report — which is the vacuous-gate shape
/// AGENTS §3 names, since a loop over nothing passes exactly like a clean tree.
const MIN_FIXTURES: usize = 4;

/// A standalone `faces = "target"` exec codec running `argv`, with one channel whose
/// endpoint faces host so `send` can feed it. No edges and no device: the multiplexed
/// side has no attached upstream, a legal load outcome (§7.5/§15.8) and what keeps this
/// device-free on every platform.
///
/// `restart_backoff_ms` is the schema maximum (one hour), so a fixture that *does* exit
/// early is not silently replaced by a fresh child mid-measurement — the survivor count
/// then means what it says.
fn exec_graph(argv: &str) -> String {
    format!(
        r#"
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["c0"]
[node.attributes]
argv = {argv}
restart_backoff_ms = 3600000
"#
    )
}

/// **No fixture under `tests/ext-codec/` outlives the daemon that spawned it, on the
/// one path where only the child can honour the promise** (plan §18 item 65(c)).
#[test]
fn no_exec_codec_fixture_outlives_a_sigkilled_daemon() {
    if !have_python3() {
        skip_no_exec_codec(
            "no_exec_codec_fixture_outlives_a_sigkilled_daemon",
            "python3 not found",
        );
        return;
    }
    let dir = ext_codec_dir();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    fixtures.sort();
    assert!(
        fixtures.len() >= MIN_FIXTURES,
        "the fixture walk found {} script(s) under {} — below the floor of \
         {MIN_FIXTURES}. A walk that finds nothing passes this test exactly like a \
         clean tree, so the count is asserted rather than trusted.",
        fixtures.len(),
        dir.display()
    );

    for fixture in fixtures {
        let name = fixture
            .file_name()
            .expect("a fixture path has a file name")
            .to_string_lossy()
            .into_owned();
        let mut d = Daemon::start();
        let argv = format!("[\"python3\", {:?}]", fixture.display().to_string());
        d.rpc()
            .load_toml(&exec_graph(&argv), false)
            .unwrap_or_else(|e| panic!("load the {name} exec graph: [{}] {}", e.code, e.message));

        // The child has to be *there* before its absence can mean anything: a guard that
        // kills a daemon which never spawned anything reports a clean group and proves
        // nothing (AGENTS §3). This is also the plant's liveness half — the sweep's
        // matcher and walker, exercised against a real codec child.
        let mut live = Vec::new();
        assert!(
            wait_until(Duration::from_secs(20), || {
                live = d.survivors();
                !live.is_empty()
            }),
            "{name}: the daemon never had a codec child in its process group, so this \
             fixture's orphan behaviour was never exercised"
        );
        assert_eq!(
            live.len(),
            1,
            "{name}: the graph has exactly one codec child: {live:?}"
        );

        // Feed it, so the child is a *working* one rather than one asleep at its first
        // read — and, for the fixture that never reads, so the daemon is blocked in
        // `write_all` on a full pipe, the state item 21 measures. The leak does not
        // depend on this; the realism does.
        let _ = d.rpc().send("mux/c0", &"z".repeat(999), false, 5_000);

        // The stimulus: the supervisor dies without unwinding. Nothing in the daemon
        // runs after this — no teardown, no `kill_on_drop` — so whatever is still alive
        // is alive because the child itself did not stop (§15.43).
        let status = d.signal_and_wait("KILL", Duration::from_secs(10));
        assert!(
            status.is_some(),
            "{name}: the daemon was still running 10 s after SIGKILL"
        );

        let mut survivors = d.survivors();
        let gone = wait_until(Duration::from_secs(15), || {
            survivors = d.survivors();
            survivors.is_empty()
        });
        assert!(
            gone,
            "{name}: the daemon is dead and this fixture is still running:\n{}\nAn exec \
             codec child's only leash is the EOF on its stdin (§15.43, §7.6). A fixture \
             that blocks before reading stdin — or that treats EOF as anything but the \
             end — outlives every run that spawns it, invisibly, with the suite green.",
            survivors
                .iter()
                .map(|(pid, argv)| format!("  pid {pid}: {argv}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        // …and the drop's sweep asserts the same thing from the harness side, which is
        // what keeps the two implementations of this property honest about each other.
    }
}

/// **The deaf fixture's own watch, both arms, with no daemon in the way** (plan §18
/// item 65(c)).
///
/// The guard above is the class property and it is the *slower* of the two; this one
/// names the mechanism, at the boundary that owns it, and its failure message points at
/// the fixture. It exists because the mechanism has two arms and the guard above
/// exercises whichever one the scheduler picks:
///
/// * **spawner dies after the fixture is up** — `getppid()` changes, which is the arm
///   anyone would write;
/// * **spawner dies before the fixture is up** — the arm that was *missing* from the
///   first draft of the fix, and that the class guard found by failing. CPython takes
///   tens of milliseconds to reach its first bytecode, the whole stimulus above is over
///   in about that long, and a fixture that samples `getppid()` after its parent is
///   already gone compares the adopter's pid with itself forever. Measured: with only
///   the first arm the fixture leaked on the second run of this file and not the third.
///
/// A shell is the supervisor here rather than a daemon, because the property is about
/// the fixture and nothing about the daemon is load-bearing: `sh` backgrounds the
/// fixture, prints its pid and exits. The fixture's stdout goes to `/dev/null` inside
/// the shell so reading `sh`'s own output cannot block on a pipe the fixture is holding
/// open.
///
/// **The two orderings are forced, not hoped for**, which is the difference between a
/// test of two arms and a test of whichever arm the box happened to run. The
/// before-it-starts arm sleeps *inside* the background subshell and then `exec`s the
/// fixture into the same pid, so the supervisor is provably gone half a second before
/// the fixture's first bytecode. The after-it-is-up arm sleeps in the supervisor
/// instead; its stated assumption is that CPython reaches `main` inside that window
/// (tens of milliseconds against two seconds), and a box slow enough to break it
/// degrades this arm into the other one — weaker coverage, never a false green.
///
/// **The liveness half, because an absence proves nothing until a presence is
/// established.** The class guard above already says so and applies it; this test
/// shipped without it and was measured vacuous: a `deaf.py` whose first statement is
/// `raise SystemExit` passed **both** arms green, in 0.1 s and 0.2 s, since "the pid
/// is gone within 15 s" is trivially true of a fixture that never ran. The fixture
/// therefore announces itself through `SNX_DEAF_READY_FILE` (see `announce_ready` in
/// `deaf.py`), written before its watch is armed and containing its own pid.
///
/// **Why a marker and not `assert!(pid_alive(pid))` at the moment the supervisor
/// returns.** That works only in the after-it-is-up arm. In the before-it-starts arm
/// the pid belongs to a subshell still inside `sleep 0.5`, so the assertion would
/// hold with the fixture yet to exist, and it would keep holding against a fixture
/// that then dies at its first bytecode — AGENTS §9's proxy, passing on the shape it
/// was written for and vacuous on the other. Nor can the fixture's presence be
/// *polled* for in that arm: once `exec`ed it is already an orphan and exits within
/// one 0.25 s watch tick, so a sampler races a window it is not guaranteed to see.
/// The marker is a latch — it survives the exit it is evidence of — which makes the
/// same assertion deterministic in both arms.
///
/// It also closes a second hole nobody had named: the pid read back from the marker
/// is required to be the pid the supervisor printed, so this test can no longer sit
/// waiting on a pid that was never the fixture's.
#[test]
fn the_deaf_fixture_stops_when_its_spawner_dies_before_or_after_it_starts() {
    if !have_python3() {
        skip_no_exec_codec(
            "the_deaf_fixture_stops_when_its_spawner_dies_before_or_after_it_starts",
            "python3 not found",
        );
        return;
    }
    let deaf = ext_codec_dir().join("deaf.py");
    let run = serial_nexus_itest::TempRun::new();
    for (i, (arm, script)) in [
        (
            "the spawner dies before the fixture has started",
            format!(
                "( sleep 0.5; exec python3 {} >/dev/null 2>&1 ) & echo $!",
                deaf.display()
            ),
        ),
        (
            "the spawner dies once the fixture is up",
            format!(
                "python3 {} >/dev/null 2>&1 & echo $!; sleep 2",
                deaf.display()
            ),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // One marker per arm: a shared path would let the first arm's evidence stand
        // in for the second's, which is the same vacuity one level up.
        let ready = run.join(&format!("deaf-ready-{i}"));
        let out = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .env("SNX_DEAF_READY_FILE", &ready)
            .stdin(Stdio::null())
            .output()
            .expect("run the throwaway supervisor");
        assert!(out.status.success(), "{arm}: the supervisor shell failed");
        let pid: u32 = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or_else(|e| {
                panic!("{arm}: the supervisor did not print the fixture's pid: {e}")
            });

        // **Presence, before the absence below can mean anything.** Ten seconds is
        // twenty times the before-it-starts arm's own 0.5 s delay, and the marker is
        // a latch rather than a sample, so this waits for an event that has either
        // happened or never will.
        assert!(
            wait_until(Duration::from_secs(10), || ready.exists()),
            "{arm}: `deaf.py` never reached `main` — no {} appeared, so the fixture \
             this test is named for never ran and its disappearance below would prove \
             only that nothing was there to disappear",
            ready.display()
        );
        let announced: u32 = std::fs::read_to_string(&ready)
            .unwrap_or_else(|e| panic!("{arm}: read {}: {e}", ready.display()))
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("{arm}: the fixture's readiness marker is not a pid: {e}"));
        assert_eq!(
            announced, pid,
            "{arm}: the fixture reports pid {announced} but the supervisor printed \
             {pid} — the wait below would be watching a process that is not the fixture"
        );

        let stopped = wait_until(Duration::from_secs(15), || {
            !serial_nexus_itest::pid_alive(pid)
        });
        if !stopped {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            panic!(
                "{arm}: `deaf.py` (pid {pid}, killed) was still running 15 s after the \
                 process that spawned it exited. It never reads its stdin by design, so \
                 §15.43's leash cannot reach it and its parent watch is the only thing \
                 that stops it outliving every run."
            );
        }
    }
}

// --- the latent sibling: the refusal that keeps it unreachable ----------------

/// **`--exit-on-stdin-eof` is refused with `transcript`, and the refusal says why**
/// (plan §18 item 65(e); design §15.43, §7.6).
///
/// The sim's leash watch parks a background thread inside `read` on stdin, holding
/// `std::io::Stdin`'s lock for the process's whole life. `std::io::Stdin` is a plain,
/// **non-reentrant** `Mutex` — only `Stdout` and `Stderr` are behind a `ReentrantLock`
/// — so any sim mode that both arms the leash and reads stdin deadlocks against that
/// parked thread *before its first read*.
///
/// **That collision is notes §3.91's *latent sibling*, and it has never happened.**
/// The distinction is worth keeping straight, because this comment previously fused
/// the two: the 260 orphaned transcript children on the development box came from the
/// same non-reentrant `Mutex` and a different pair of holders — `park_transcript`
/// called `std::io::stdin().lock()` while its own caller's `StdinLock` was still
/// alive, a **same-thread self-deadlock** in one function's own call chain. The leash
/// watch was not involved and could not have been: the daemon spawns a `transcript`
/// child with no `--exit-on-stdin-eof` (see `recorder_argv`/`replayer_argv` in
/// `p8_daemon_transcript`), and the flag is refused for that mode anyway — which is
/// the refusal below. §3.91 files the sibling separately and calls it "unreachable
/// today", one refusal away.
///
/// So this guard has never yet caught a live instance, and claiming the 260 for it
/// would credit it with exactly the thing it is here to keep from ever happening.
/// Today exactly one mode reads stdin — `transcript`, whose stdin **is** the daemon's
/// envelope pipe — and it is unreachable only because that combination is refused at
/// startup. Item 65(e) filed the refusal as untested, and it was: this is the guard.
///
/// **Why the exit status alone would assert nothing.** `2` is also clap's usage-error
/// status, so a test that checked it and nothing else would pass just as green against
/// an argv the parser rejected — the same test, the same output, a different mechanism.
/// So the message is asserted too, and a control arm runs the identical argv *without*
/// the flag and requires no refusal from it: whatever `2` means in the first arm, it is
/// caused by the flag.
#[test]
fn the_sim_refuses_the_leash_for_the_one_mode_that_reads_stdin() {
    let run = serial_nexus_itest::TempRun::new();
    let script = run.join("script.transcript");
    std::fs::write(&script, "> H:aa\n").expect("write the script");
    let record = run.join("observed.transcript");
    let argv = [
        "transcript",
        "--transcript",
        &script.display().to_string(),
        "--record",
        &record.display().to_string(),
    ]
    .map(str::to_owned);

    let refused = Command::new(serial_nexus_itest::bin("serial-nexus-sim"))
        .arg("--exit-on-stdin-eof")
        .args(&argv)
        .stdin(Stdio::null())
        .output()
        .expect("run serial-nexus-sim transcript under the leash");
    assert_eq!(
        refused.status.code(),
        Some(2),
        "arming the leash on `transcript` must be refused: status={:?} stderr={:?}",
        refused.status,
        String::from_utf8_lossy(&refused.stderr)
    );
    let stderr = String::from_utf8_lossy(&refused.stderr).into_owned();
    assert!(
        stderr.contains("--exit-on-stdin-eof is refused with `transcript`"),
        "the refusal must name itself rather than exiting 2 like a usage error; \
         stderr={stderr:?}"
    );

    // The control arm: the same argv, no flag. It must not print that refusal — so the
    // status above is the leash's answer and not the parser's.
    let mut allowed = serial_nexus_itest::KillOnDrop::new(
        Command::new(serial_nexus_itest::bin("serial-nexus-sim"))
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run serial-nexus-sim transcript without the leash"),
    );
    let exit = allowed.wait_exit(Duration::from_secs(10));
    assert_ne!(
        exit.and_then(|s| s.code()),
        Some(2),
        "the same argv without the leash must not be refused; the refusal has to be \
         attributable to the flag"
    );
}
