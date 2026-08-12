#![forbid(unsafe_code)]

//! `serial-nexus-ctl subscribe` must **read its acknowledgement** (37-TOOL-2, design
//! §10 / §15.16).
//!
//! `subscribe_stream` discarded every Response frame — including the error reply to the
//! subscribe request itself — and then blocked in `read_line` on a connection the
//! daemon deliberately keeps open for the stream. Against a daemon that does not answer
//! the verb (a version skew across §15.16's graceful-degradation boundary), the CLI
//! therefore printed nothing, exited never, and said nothing about why: an operator or
//! a CI step waiting on it deadlocked with no diagnosis. Its sibling `tap` reads and
//! examines its `tap.open` ack for exactly this reason.
//!
//! Two tests, and the negative one needs no daemon:
//!
//! 1. [`ctl_subscribe_exits_naming_the_code_when_the_daemon_refuses_the_verb`] — a
//!    twenty-line **stand-in daemon** on a Unix socket answers the subscribe request
//!    with a JSON-RPC error and then holds the connection open forever, writing
//!    nothing. That is the version-skew shape exactly, and it is deterministic: no
//!    timing, no real daemon, no way to reach it with a shipped one (this daemon
//!    implements `subscribe`). The CLI must exit non-zero, naming the code.
//! 2. [`ctl_subscribe_still_streams_notifications_from_a_daemon_that_answers`] — the
//!    positive control against the real daemon, because "bail on the first Response"
//!    would pass test 1 and break the verb. The daemon emits a full state snapshot
//!    roughly every 200 ms while a subscriber exists (§10), so `--count 1` terminates
//!    on the stream rather than on a timer.
//!
//! Runs on every platform: a control socket and a JSON line, no device.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, TempRun, bin};

/// How long a refusal may take to reach the child and end it. Generous: the failure
/// this guards is *unbounded*, so a slow runner cannot make it a false pass.
const EXIT_BOUND: Duration = Duration::from_secs(15);

/// Wait up to `bound` for `child` to exit, returning its status — or `None` after
/// killing it, so a regression is a bounded failure and never a leaked process.
fn wait_exit(child: &mut Child, bound: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + bound;
    loop {
        match child
            .try_wait()
            .expect("try_wait on serial-nexus-ctl subscribe")
        {
            Some(status) => return Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Spawn `serial-nexus-ctl --socket <sock> subscribe <args…>` with both streams
/// redirected to files, so the notification stream stays parseable and the diagnosis
/// can be read back.
fn spawn_ctl_subscribe(socket: &Path, args: &[&str], out: &Path, err: &Path) -> Child {
    let stdout = std::fs::File::create(out).expect("create stream file");
    let stderr = std::fs::File::create(err).expect("create diagnosis file");
    Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(socket)
        .arg("subscribe")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn serial-nexus-ctl subscribe")
}

/// A child SIGKILLed and reaped on drop, so a panicking test never leaks one.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// 37-TOOL-2 — a refused `subscribe` must exit, naming the code.
#[test]
fn ctl_subscribe_exits_naming_the_code_when_the_daemon_refuses_the_verb() {
    let run = TempRun::new();
    let sock = run.join("skew.sock");

    // The stand-in: answer the one request with `-32601 method not found` and then keep
    // the connection open, writing nothing — which is what the real control plane does
    // with a subscribe connection, and what made the CLI's swallowed ack a hang rather
    // than an EOF. The accept loop runs on its own thread and dies with the test.
    let listener = UnixListener::bind(&sock).expect("bind the stand-in socket");
    let held = std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut writer = stream.try_clone().expect("clone the stand-in connection");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let id = serde_json::from_str::<Value>(line.trim())
            .ok()
            .and_then(|v| v.get("id").cloned())
            .unwrap_or(Value::Null);
        let refusal = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":\
             {{\"code\":-32601,\"message\":\"method not found: subscribe\"}}}}\n"
        );
        let _ = writer.write_all(refusal.as_bytes());
        let _ = writer.flush();
        // Hold the connection open exactly as the daemon would. The CLI must not be
        // waiting on it; if it is, `wait_exit` below kills it and the test fails.
        let mut sink = String::new();
        let _ = reader.read_line(&mut sink);
    });

    let (out, err) = (run.join("stream.ndjson"), run.join("stream.err"));
    let mut child = spawn_ctl_subscribe(&sock, &[], &out, &err);
    let status = wait_exit(&mut child, EXIT_BOUND);
    assert!(
        status.is_some(),
        "`serial-nexus-ctl subscribe` never exited against a daemon that refused the \
         verb — it is still blocked in read_line on a connection that will never carry \
         another byte (37-TOOL-2)"
    );
    assert!(
        !status.expect("exited").success(),
        "a refused `subscribe` exited 0: a script cannot tell it from an empty stream"
    );

    let diagnosis =
        String::from_utf8(std::fs::read(&err).expect("read diagnosis")).expect("stderr is utf-8");
    assert!(
        diagnosis.contains("-32601"),
        "the refusal does not name the code the daemon returned: {diagnosis:?}"
    );
    assert_eq!(
        std::fs::read(&out).expect("read stream").len(),
        0,
        "a refused subscribe emitted notification lines"
    );

    // Let the stand-in's connection go with the CLI's exit.
    drop(held);
}

/// The positive control: the verb still works against a daemon that answers it.
#[test]
fn ctl_subscribe_still_streams_notifications_from_a_daemon_that_answers() {
    let d = Daemon::start();
    let run = d.run();
    let (out, err) = (run.join("live.ndjson"), run.join("live.err"));
    let mut child = KillOnDrop(spawn_ctl_subscribe(
        &d.socket(),
        &["--count", "1"],
        &out,
        &err,
    ));

    let status = wait_exit(&mut child.0, EXIT_BOUND);
    assert!(
        status.is_some(),
        "`serial-nexus-ctl subscribe --count 1` never exited against a live daemon"
    );
    assert!(
        status.expect("exited").success(),
        "a served `subscribe` exited non-zero: {}",
        String::from_utf8_lossy(&std::fs::read(&err).expect("read diagnosis"))
    );

    // One line, and it is the notification — not the ack, which stays off stdout so the
    // stream is clean for `jq`.
    let emitted = std::fs::read_to_string(&out).expect("read stream");
    let lines: Vec<&str> = emitted.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one notification: {lines:?}"
    );
    let note: Value = serde_json::from_str(lines[0]).expect("the emitted line is JSON");
    assert_eq!(
        note.get("method").and_then(Value::as_str),
        Some("state"),
        "the emitted line is not a state notification: {note}"
    );
    assert!(
        note.get("id").is_none(),
        "a response frame reached the notification stream: {note}"
    );
}
