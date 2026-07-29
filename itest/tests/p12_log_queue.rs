//! Review 32, log-queue slice (design §5 "loss is always visible and attributable",
//! §7.3 log state). One property, over both of the ways the log's writer thread can
//! stop draining:
//!
//! **LOGQ-1** — once `writer_loop` has returned, nothing will ever drain the node's
//! queue again, so every byte the pump is still handed is *loss*, not backlog. Its own
//! `return // stop draining; the pump drops-and-counts` comment claimed exactly that,
//! but `enqueue` only shed once the queue hit `QUEUE_CAP_BYTES`, so the first **16 MiB**
//! of provably-unwritable console data was reported as `queued_bytes` while
//! `dropped_bytes` stayed flat — and retained in RAM until the node was removed. An
//! operator sizing an outage from the loss counter saw nothing at all for that first
//! 16 MiB.
//!
//! The two return paths get one test each, because they are reached by different
//! failures and only one of them abandons a batch on the way out:
//!
//! * [`a_write_failure_under_fault_starts_shedding_immediately`] — a `write(2)` failure
//!   under `overflow = "fault"`, over `/dev/full`.
//! * [`a_failed_rotation_starts_shedding_immediately`] — a failed rotation under the
//!   *default* drop-oldest policy, over a directory made read-only mid-run. This one
//!   also pins the byte identity in full: `written + dropped == produced`, with
//!   `written` read off the file on disk.
//!
//! Ground truth is structured RPC state plus the on-disk file length, never CLI text,
//! and the produced total is pinned by asserting each probe's own `received` (§5).
//!
//! That identity is pinned here only at **offset 0**: `/dev/full` refuses every write
//! with nothing stored, and a rename fails before any byte moves, so no guard in this
//! file can see a *partial* write — `write(2)` storing what fits and the retry getting
//! ENOSPC, which is the ordinary shape of a disk filling up rather than a disk already
//! full. Charging the whole chunk there made `written + dropped` exceed `produced` for a
//! prefix that was durably in the file (LOG-1). Reproducing it needs a `write(2)` target
//! that partial-writes on demand, which nothing reachable from a daemon fixture here
//! provides, so that half is pinned beside the code:
//! `daemon/src/nodes/log.rs`'s `a_partial_write_charges_only_the_unwritten_remainder`.
//!
//! **CONC-3** — a refused writer-thread `spawn` must fault the node, not panic the daemon
//! out of `startup_load` — is pinned twice, and this file carries the end-to-end half:
//! [`a_daemon_that_cannot_spawn_the_log_writer_starts_and_faults_the_node`] boots
//! `serial-nexus-daemon` under `ulimit -u 1` and asks it about the node.
//!
//! This module doc previously said such a test was impossible ("provoking `EAGAIN` from
//! `pthread_create` needs an `RLIMIT_NPROC` the harness cannot impose on a daemon it
//! shares a process tree with"). That was simply wrong, and the review-32 audit said so:
//! `RLIMIT_NPROC` is set per *process*, `bash -c 'ulimit -u 1; exec …'` applies it to the
//! daemon alone, and the harness — a different process — never notices. The unit-level
//! guard beside it (`daemon/src/nodes/log.rs`'s
//! `a_refused_writer_thread_faults_the_node_instead_of_panicking`) now drives the real
//! `LogNode::create` through an injected spawn, so it too fails against the unfixed code;
//! this one additionally proves the *daemon* survives, which is the sentence §15.8 is
//! actually making.
//!
//! Both tests need a serial *device* to source a hostward stream, so they **self-skip**
//! off Linux (§5, `serial_echo` returns `None` there); the first also needs a writable
//! `/dev/full`, and the second a directory whose mode the process actually obeys.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{
    Daemon, Rpc, Sim, TempRun, bin, daemon_answers, file_len, serial_echo, wait_until,
};

/// One 8 KiB echo round-trip through the console pty: client → console → serial →
/// echo device → back hostward → {console, log}.
fn echo_8k(tty: &Path, seed: u64) -> Value {
    let path = tty.to_string_lossy().into_owned();
    let seed = seed.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        "seeded:8KiB",
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "30000",
    ])
}

/// Assert the probe delivered its whole 8 KiB, which is what pins the number of bytes
/// the *log* was handed: the console and the log are fed by the same fan-out, so a
/// console that received all 8192 proves the serial node produced all 8192.
fn assert_full_round_trip(probe: &Value, which: &str) -> u64 {
    assert_eq!(
        probe["pass"].as_bool(),
        Some(true),
        "{which} echo probe failed: {probe}"
    );
    assert_eq!(
        probe["received"].as_u64(),
        Some(8192),
        "{which} echo probe was short, so the produced total is unknown: {probe}"
    );
    8192
}

fn log_counters(rpc: &Rpc, node: &str) -> (u64, u64) {
    let n = rpc.node(node).unwrap_or(Value::Null);
    (
        n["dropped_bytes"].as_u64().unwrap_or(0),
        n["queued_bytes"].as_u64().unwrap_or(0),
    )
}

#[test]
fn a_write_failure_under_fault_starts_shedding_immediately() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_write_failure_under_fault_starts_shedding_immediately: \
             no serial device on this platform"
        );
        return;
    };
    // `/dev/full` refuses every write with ENOSPC and needs no privilege (the same
    // device `p3_log_enospc` uses); absent on macOS.
    let full = Path::new("/dev/full");
    if !full.exists() || std::fs::OpenOptions::new().write(true).open(full).is_err() {
        eprintln!(
            "SKIP a_write_failure_under_fault_starts_shedding_immediately: \
             no writable /dev/full on this system"
        );
        return;
    }

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    std::os::unix::fs::symlink("/dev/full", logdir.join("full")).expect("symlink -> /dev/full");
    let console = d.run().join("console");

    // `hostward_buffer = 8192` on the console for the §5/§15.19 reason: the pty
    // hostward bridge sheds with a counter at its default depth of 32 chunks, and this
    // test reads the produced total off the console's own `received`.
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
arbitration = "free-for-all"
device = "{dev}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "full"
overflow = "fault"
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
    rpc.load_toml(&cfg, false).expect("load config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("console", "active", Duration::from_secs(10))
            && rpc.wait_status("cap", "active", Duration::from_secs(10)),
        "nodes not active: usb0={:?} console={:?} cap={:?}",
        rpc.node("usb0"),
        rpc.node("console"),
        rpc.node("cap")
    );

    // Batch 1 forces the first write(2), which ENOSPCs; under `fault` the writer
    // counts the batch it was holding, faults the node, and returns for good.
    let mut produced = assert_full_round_trip(&echo_8k(&console, 1), "first");
    assert!(
        rpc.wait_status("cap", "faulted", Duration::from_secs(20)),
        "the log did not fault on the refused write: {:?}",
        rpc.node("cap")
    );

    // Two more batches after the writer is provably gone. Every one of their bytes is
    // unwritable by construction — /dev/full accepts nothing and no thread is draining
    // the queue any more — so all of them must land in `dropped_bytes`.
    produced += assert_full_round_trip(&echo_8k(&console, 2), "second");
    produced += assert_full_round_trip(&echo_8k(&console, 3), "third");

    assert!(
        wait_until(Duration::from_secs(20), || {
            log_counters(rpc, "cap") == (produced, 0)
        }),
        "post-return bytes were reported as backlog rather than loss (LOGQ-1): \
         produced={produced} node={:?}",
        rpc.node("cap")
    );

    // Fault isolation still holds: the port and its live console keep flowing.
    assert_eq!(
        rpc.node_status("console"),
        "active",
        "the console must stay active while the log sheds"
    );
}

#[test]
fn a_failed_rotation_starts_shedding_immediately() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_failed_rotation_starts_shedding_immediately: \
             no serial device on this platform"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let live = logdir.join("cap.log");
    let console = d.run().join("console");

    // Default overflow (drop-oldest): §7.3 ends the writer on a rotation failure under
    // *either* policy, so this arm needs no `overflow` attribute — which also makes it
    // the shape every operator gets who never wrote one.
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
arbitration = "free-for-all"
device = "{dev}"
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
    rpc.load_toml(&cfg, false).expect("load config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("console", "active", Duration::from_secs(10))
            && rpc.wait_status("cap", "active", Duration::from_secs(10)),
        "nodes not active: usb0={:?} console={:?} cap={:?}",
        rpc.node("usb0"),
        rpc.node("console"),
        rpc.node("cap")
    );

    // Batch 1 is written normally, and waiting for it on disk is what puts the
    // rotation strictly *after* it in the byte stream (§7.3) — no bare sleep.
    let mut produced = assert_full_round_trip(&echo_8k(&console, 11), "first");
    assert!(
        wait_until(Duration::from_secs(20), || file_len(&live) == 8192),
        "the pre-rotation batch never reached the file: {} bytes, node={:?}",
        file_len(&live),
        rpc.node("cap")
    );

    // Take the rename away from under the writer. The daemon runs as this process's
    // user, so the process's own inability to write here is a sound proxy — where it
    // is not (root, or a filesystem that ignores modes) the test cannot run at all.
    std::fs::set_permissions(&logdir, std::fs::Permissions::from_mode(0o500))
        .expect("chmod the log directory read-only");
    if std::fs::write(logdir.join("probe"), b"x").is_ok() {
        eprintln!(
            "SKIP a_failed_rotation_starts_shedding_immediately: \
             a read-only directory is still writable here"
        );
        let _ = std::fs::set_permissions(&logdir, std::fs::Permissions::from_mode(0o700));
        return;
    }

    rpc.rotate("cap").expect("rotate accepted while active");
    assert!(
        rpc.wait_status("cap", "faulted", Duration::from_secs(20)),
        "the log did not fault on the refused rename: {:?}",
        rpc.node("cap")
    );
    // The writer is gone, so nothing will rotate or write again; give the directory
    // its permissions back so the run's temp tree can be cleaned up.
    std::fs::set_permissions(&logdir, std::fs::Permissions::from_mode(0o700))
        .expect("restore the log directory");

    // Two more batches after the writer returned.
    produced += assert_full_round_trip(&echo_8k(&console, 22), "second");
    produced += assert_full_round_trip(&echo_8k(&console, 33), "third");

    // The whole point, as one identity: every produced byte is either on disk or
    // counted as lost, and nothing is held as backlog by a queue with no consumer.
    assert!(
        wait_until(Duration::from_secs(20), || {
            let (dropped, queued) = log_counters(rpc, "cap");
            file_len(&live) + dropped == produced && queued == 0
        }),
        "written + dropped != produced, or bytes are still reported as queued \
         (LOGQ-1): produced={produced} written={} node={:?}",
        file_len(&live),
        rpc.node("cap")
    );
    assert_eq!(
        file_len(&live),
        8192,
        "only the pre-rotation batch may be on disk; the rest is loss"
    );
    assert!(
        !logdir.join("cap.log.000").exists(),
        "a failed rename must leave no rotated file behind"
    );
    assert_eq!(
        rpc.node_status("console"),
        "active",
        "the console must stay active while the log sheds"
    );
}

/// A hand-managed `serial-nexus-daemon`, SIGKILLed and reaped on drop. `Daemon` cannot be used
/// here: this test must wrap the binary in a shell that lowers `RLIMIT_NPROC` first, and
/// `Daemon::start_with_args` (rightly) owns the command line.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// CONC-3, end to end: a daemon that **cannot spawn** the log's writer thread still comes
/// up, and reports the refusal as that node's state (§15.8 — environmental failure changes
/// a node's state, never the graph, and never the daemon's life).
///
/// Before the fix this was `.spawn(…).expect(…)` inline in `LogNode::create`: the panic
/// unwound out of `startup_load` → `serve` → `run`, straight past the `if let Err(e)` arm
/// that exists so a bad state file cannot cost the daemon its life. Because the graph is
/// persisted, one transient thread limit turned a log node that had loaded fine into a
/// **crash loop across restarts** — the daemon died at boot, every boot.
///
/// The mechanism (the audit's, reproduced here before it was written down): `RLIMIT_NPROC`
/// is per-process and per-uid, so `ulimit -u 1` in a subshell that then `exec`s the daemon
/// refuses every `clone(2)` the daemon attempts while leaving this test process — which
/// already exists — untouched. Observed by hand on 2026-07-27: socket up, `state` reports
/// `cap` `faulted (spawn log writer thread: Resource temporarily unavailable (os error
/// 11))`, and the daemon keeps answering.
///
/// A daemon that fails to answer here is a **failure, not a skip**: "the daemon did not
/// come up" is precisely the regression. If a future daemon legitimately needs a thread at
/// startup for something else, this test will say so loudly and the fix is to give it a
/// graph that reaches the log node some other way — not to soften the assertion.
///
/// Linux-only (`/proc`-style `ulimit -u` semantics and a POSIX shell); self-skips without
/// `bash`.
#[test]
fn a_daemon_that_cannot_spawn_the_log_writer_starts_and_faults_the_node() {
    if !cfg!(target_os = "linux") {
        eprintln!(
            "SKIP a_daemon_that_cannot_spawn_the_log_writer_starts_and_faults_the_node: \
             RLIMIT_NPROC-refuses-every-thread is asserted on Linux only"
        );
        return;
    }
    if Command::new("bash")
        .arg("-c")
        .arg("exit 0")
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!(
            "SKIP a_daemon_that_cannot_spawn_the_log_writer_starts_and_faults_the_node: \
             no bash to lower RLIMIT_NPROC with"
        );
        return;
    }

    let run = TempRun::new();
    let socket = run.socket();
    let logdir = run.join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    // The graph is written by *this* process, so the daemon's only unusual act is the
    // thread it cannot start.
    let cfg_path = run.join("graph.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "[[node]]\ntype = \"log\"\nname = \"cap\"\ndirectory = \"{}\"\n\
             filename = \"cap.log\"\n",
            logdir.display()
        ),
    )
    .expect("write the startup config");
    let stderr_path = run.join("daemon.err");
    let stderr = std::fs::File::create(&stderr_path).expect("create the daemon log");

    let script = format!(
        "ulimit -u 1; exec '{daemon}' --socket '{socket}' --state-file '{state}' --config '{cfg}'",
        daemon = bin("serial-nexus-daemon").display(),
        socket = socket.display(),
        state = run.join("d.state.toml").display(),
        cfg = cfg_path.display(),
    );
    let child = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn serial-nexus-daemon under a thread limit");
    let _daemon = KillOnDrop(child);

    let ready = wait_until(Duration::from_secs(15), || {
        socket.exists() && daemon_answers(&socket)
    });
    let logged = || std::fs::read_to_string(&stderr_path).unwrap_or_default();
    assert!(
        ready,
        "the daemon never answered with its writer thread refused — a refused `spawn` \
         must fault the node, not end the process (CONC-3, §15.8). Its output:\n{}",
        logged()
    );

    let rpc = Rpc::new(&socket);
    assert_eq!(
        rpc.node_status("cap"),
        "faulted",
        "a log whose writer thread was refused must be faulted: {:?}",
        rpc.node("cap")
    );
    let reason = rpc
        .node("cap")
        .and_then(|n| n["reason"].as_str().map(str::to_owned))
        .unwrap_or_default();
    assert!(
        reason.starts_with("spawn log writer thread:"),
        "the fault must name the refused spawn rather than something incidental: \
         {reason:?}"
    );
    // And the daemon is still alive and serving after reporting it.
    assert!(
        daemon_answers(&socket),
        "the daemon stopped answering after reporting the fault:\n{}",
        logged()
    );
}
