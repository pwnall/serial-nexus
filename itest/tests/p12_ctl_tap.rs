#![forbid(unsafe_code)]

//! Review 32, `CTL-1` — `serial-nexus-ctl tap` must **exit** on the terminal
//! `tap.closed` notification instead of blocking forever on a live connection.
//!
//! `tap.closed` (§17, `docs/rpc/observation.md`) exists precisely so a tap's client
//! learns that the graph dropped its endpoint out from under it: `teardown`,
//! `load --replace` and `remove-node --cascade` all reach it. It is terminal for the
//! *tap*, not for the connection — the daemon deliberately keeps the socket alive for
//! the connection's other taps and its subscription — so a client that ignores the
//! notification parks in `read_line` on a connection that will never carry another
//! byte. The CLI's notification loop `continue`d on every method that was not
//! `tap.data`, so `serial-nexus-ctl tap console > incident.bin` never returned after a
//! routine §11 reconfiguration: the bytes already written were intact and flushed, but
//! the *termination* and the exit status were lost, and any script or CI step waiting
//! on the process deadlocked. Permanently — the hub is gone from `state.taps` and a
//! re-`load` does not revive that tap id.
//!
//! Three tests. The first two are `CTL-1`'s two halves, one per platform class; the
//! third (37-TOOL-1) is the *other* way a tap's stream ends short — a bare connection
//! EOF, which used to exit 0 over a truncated capture:
//!
//! 1. [`ctl_tap_exits_on_tap_closed_rather_than_blocking_forever`] — the control-plane
//!    half, on **every** platform. Every host-facing endpoint gets a hub at load even
//!    while its serial node is `waiting` on an absent device (§15.8), so no serial
//!    device is needed. It also pins the documented exit status: an unbounded tap ends
//!    orderly (`0`), while a tap with an outstanding `--bytes` budget is a short read
//!    and errors, so a script cannot mistake a partial capture for the one it asked for.
//! 2. [`ctl_tap_exits_with_the_bytes_it_received_intact`] — the byte half, gated on
//!    [`serial_echo`] and therefore **Linux-only** (a pts cannot be a serial device on
//!    macOS, §5). A seeded `serial-nexus-sim` source feeds the endpoint, the CLI captures to a
//!    file, the graph is torn down, and the capture is asserted byte-exact by SHA-256
//!    against an independently computed source stream.
//! 3. [`ctl_tap_exits_nonzero_when_the_connection_dies_short_of_its_bytes_budget`] — the
//!    EOF arm, on **every** platform. Same documented rule, the route that never
//!    obeyed it.
//!
//! Fail-first: with the `tap.closed` arm removed from `tap_stream`, both children of
//! test 1 are still alive at the bound and both `wait_exit` calls return `None`; with
//! the EOF arm's budget check removed, test 3's capped child exits 0.
//!
//! `CTL-2` (the CLI discarding `gap_before` and the `offset` jump) is only half guarded
//! here. Producing either signal on demand needs a bounded queue to overflow — the
//! producer→hub feed for `gap_before`, this tap's own queue for the offset jump — and
//! a promptly-draining CLI cannot be made to cause either deterministically. The rule
//! lives in one small type instead (`TapContinuity` in `ctl/src/main.rs`)
//! and is pinned by that crate's unit tests, rather than in a timing race here.

use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serial_nexus_itest::{
    Daemon, Rpc, Sim, TempRun, bin, daemon_answers, seeded_bytes, serial_echo, sha256_hex,
    wait_until,
};

/// How long a `tap.closed` may take to reach the child and end it. Generous: the
/// failure this guards is *unbounded*, so a slow runner cannot make it a false pass.
const EXIT_BOUND: Duration = Duration::from_secs(15);

/// Number of open taps `state` currently reports.
fn tap_count(rpc: &Rpc) -> usize {
    rpc.state()["taps"].as_array().map(Vec::len).unwrap_or(0)
}

/// Spawn `serial-nexus-ctl --socket <sock> tap <args…>` with stdout and stderr each
/// redirected to a file, so the capture stays a clean byte stream and the notices can
/// be read back. Returns the child; the caller owns waiting for (or killing) it.
fn spawn_ctl_tap(socket: &Path, args: &[&str], out: &Path, err: &Path) -> Child {
    let stdout = std::fs::File::create(out).expect("create capture file");
    let stderr = std::fs::File::create(err).expect("create notice file");
    Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(socket)
        .arg("tap")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn serial-nexus-ctl tap")
}

/// Wait up to `bound` for `child` to exit, returning its status — or `None` after
/// killing it, so a regression is a bounded failure and never a leaked process.
fn wait_exit(child: &mut Child, bound: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + bound;
    loop {
        match child.try_wait().expect("try_wait on serial-nexus-ctl tap") {
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

/// A graph whose serial node's device does not exist: the node sits `waiting` (§7.1
/// faulted-and-wait) while its host-facing endpoint still owns a tap hub (§15.8), which
/// is all `tap.closed` needs. No serial device, so this runs everywhere.
fn absent_cfg(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[edge]]
a = "usb0"
b = "console"
"#,
        dev = run.join("absent-usb0").display(),
        console = run.join("console").display(),
    )
}

/// CTL-1 — `teardown` under a live CLI tap must end the process, not strand it.
///
/// Both exit-status arms are pinned here, because the status is the half the review
/// singled out as lost: an unbounded `tap` exits `0` after naming the endpoint and the
/// `reason` on stderr, and a `tap --bytes N` that never reached N exits non-zero.
#[test]
fn ctl_tap_exits_on_tap_closed_rather_than_blocking_forever() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&absent_cfg(d.run()), false)
        .expect("load graph");

    let (open_out, open_err) = (d.run().join("open.bin"), d.run().join("open.err"));
    let (cap_out, cap_err) = (d.run().join("capped.bin"), d.run().join("capped.err"));
    let sock = d.socket();
    let mut unbounded = spawn_ctl_tap(&sock, &["usb0"], &open_out, &open_err);
    let mut capped = spawn_ctl_tap(&sock, &["usb0", "--bytes", "4096"], &cap_out, &cap_err);

    // Both taps must be registered before the graph moves, or the test would prove
    // nothing about `tap.closed` (a tap that never opened exits for another reason).
    assert!(
        wait_until(Duration::from_secs(10), || tap_count(rpc) == 2),
        "the two CLI taps did not both register (taps={:?})",
        rpc.state()["taps"]
    );

    // Drop the endpoint out from under both taps — the routine §11 operation that
    // used to strand the CLI forever.
    rpc.teardown();

    let unbounded_status = wait_exit(&mut unbounded, EXIT_BOUND);
    let capped_status = wait_exit(&mut capped, EXIT_BOUND);
    assert!(
        unbounded_status.is_some(),
        "`serial-nexus-ctl tap usb0` never exited after teardown — it is still blocked in \
         read_line on a connection the daemon deliberately keeps alive (CTL-1)"
    );
    assert!(
        capped_status.is_some(),
        "`serial-nexus-ctl tap usb0 --bytes 4096` never exited after teardown (CTL-1)"
    );

    // The documented statuses (see `tap_closed` in ctl/src/main.rs).
    assert!(
        unbounded_status.expect("exited").success(),
        "an orderly end of stream must exit 0: {}",
        String::from_utf8_lossy(&std::fs::read(&open_err).expect("read notices"))
    );
    assert!(
        !capped_status.expect("exited").success(),
        "a `--bytes` budget the stream ended short of must exit non-zero"
    );

    // stderr names the endpoint and the stable reason token, beside the `tap opened:`
    // line; stdout stays the (here empty) byte stream.
    let notices = String::from_utf8(std::fs::read(&open_err).expect("read notices"))
        .expect("stderr is utf-8");
    assert!(
        notices.contains("tap closed"),
        "no termination notice on stderr: {notices:?}"
    );
    assert!(
        notices.contains("usb0"),
        "the notice does not name the endpoint: {notices:?}"
    );
    assert!(
        notices.contains("teardown"),
        "the notice does not name the reason: {notices:?}"
    );
    assert_eq!(
        std::fs::read(&open_out).expect("read capture").len(),
        0,
        "the notice leaked into the stdout byte stream"
    );

    // The short-read arm reports the shortfall rather than a bare errno.
    let capped_notices =
        String::from_utf8(std::fs::read(&cap_err).expect("read notices")).expect("stderr is utf-8");
    assert!(
        capped_notices.contains("0 of 4096"),
        "the short read does not name what was received against what was asked: \
         {capped_notices:?}"
    );
}

/// A daemon on `run`'s fixed socket and state file, hand-managed because this test
/// must **kill** it mid-stream. `Daemon` reaps its child only on drop, and dropping it
/// would take the CLI's socket away with the wrong ceremony (a graceful `shutdown`
/// first), which is the path the sibling tests already cover.
fn spawn_daemon(run: &TempRun) -> Child {
    Command::new(bin("serial-nexus-daemon"))
        .arg("--socket")
        .arg(run.socket())
        .arg("--state-file")
        .arg(run.state_file())
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serial-nexus-daemon")
}

/// 37-TOOL-1 — a clean connection EOF short of the `--bytes` budget is a **short read**,
/// and must exit non-zero like the `tap.closed` route it is indistinguishable from.
///
/// The two ways a tap's stream can end short were treated differently for no stated
/// reason: `tap.closed` — the graph dropping the endpoint — errored per the documented
/// rule, while the connection simply going away broke the read loop and fell through to
/// `Ok(())`. That is the one outcome `--bytes N` exists to prevent: a script that asked
/// for N bytes, got fewer because the daemon died mid-capture, and was told **0**. Both
/// sibling guards above exit through `tap.closed`, so this arm had no test at all.
///
/// `SIGKILL` on the daemon is the shape the review names (a crash, an abrupt close) and
/// the only one that reaches the arm: the graph-side routes all send `tap.closed` first.
/// A Unix-socket peer dying closes the fd, so the CLI reads a clean EOF, not an error —
/// which is exactly why the old code could mistake it for an orderly end of stream.
///
/// Both statuses are pinned, because "make every EOF an error" would be the wrong fix:
/// with no budget outstanding the bytes already written are intact and the stream ended,
/// which is still an orderly **0**.
#[test]
fn ctl_tap_exits_nonzero_when_the_connection_dies_short_of_its_bytes_budget() {
    let run = TempRun::new();
    let mut daemon = spawn_daemon(&run);
    let sock = run.socket();
    assert!(
        wait_until(Duration::from_secs(10), || sock.exists()
            && daemon_answers(&sock)),
        "daemon never answered `info` on {}",
        sock.display()
    );
    let rpc = Rpc::new(&sock);
    rpc.load_toml(&absent_cfg(&run), false).expect("load graph");

    let (open_out, open_err) = (run.join("open.bin"), run.join("open.err"));
    let (cap_out, cap_err) = (run.join("capped.bin"), run.join("capped.err"));
    let mut unbounded = spawn_ctl_tap(&sock, &["usb0"], &open_out, &open_err);
    let mut capped = spawn_ctl_tap(&sock, &["usb0", "--bytes", "4096"], &cap_out, &cap_err);
    assert!(
        wait_until(Duration::from_secs(10), || tap_count(&rpc) == 2),
        "the two CLI taps did not both register (taps={:?})",
        rpc.state()["taps"]
    );

    // The crash. Reaped here so the socket's peer is provably gone before the wait
    // below, rather than at some later drop.
    daemon.kill().expect("kill the daemon");
    daemon.wait().expect("reap the daemon");

    let capped_status = wait_exit(&mut capped, EXIT_BOUND);
    let unbounded_status = wait_exit(&mut unbounded, EXIT_BOUND);
    assert!(
        capped_status.is_some(),
        "`serial-nexus-ctl tap usb0 --bytes 4096` never exited after the daemon died"
    );
    assert!(
        unbounded_status.is_some(),
        "`serial-nexus-ctl tap usb0` never exited after the daemon died"
    );
    assert!(
        !capped_status.expect("exited").success(),
        "a capture truncated by the daemon's death exited 0 — a script cannot tell it \
         from the 4096 bytes it asked for (37-TOOL-1); stderr={:?}",
        String::from_utf8_lossy(&std::fs::read(&cap_err).expect("read notices"))
    );
    assert!(
        unbounded_status.expect("exited").success(),
        "an unbounded tap whose connection ended is still an orderly end of stream: {}",
        String::from_utf8_lossy(&std::fs::read(&open_err).expect("read notices"))
    );

    // And it says what was short, in the same sentence the `tap.closed` route uses —
    // an exit code alone would not tell an operator what happened.
    let notices =
        String::from_utf8(std::fs::read(&cap_err).expect("read notices")).expect("stderr is utf-8");
    assert!(
        notices.contains("0 of 4096"),
        "the short read does not name what was received against what was asked: \
         {notices:?}"
    );
    assert!(
        notices.contains("usb0"),
        "the short read does not name the endpoint: {notices:?}"
    );
}

/// CTL-1, the byte half — the process must exit *with the bytes it received intact*,
/// which is the property the verifier separated from the finder's "truncated capture"
/// wording: nothing is lost from the file, only the termination was. Linux-only (a sim
/// pty cannot be a serial device on macOS, §5).
#[test]
fn ctl_tap_exits_with_the_bytes_it_received_intact() {
    let Some(probe) = serial_echo() else {
        eprintln!("SKIP ctl_tap_exits_with_the_bytes_it_received_intact: no serial device here");
        return;
    };
    drop(probe); // the gated source double below is this test's own device

    const N: usize = 65536;
    const SEED: u64 = 23;

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");

    // A seeded source gated on GO so its payload cannot outrun the not-yet-open tap:
    // the capture then starts at offset 0 and is comparable to the source byte for
    // byte. `--hold-ms` keeps the device present until this test tears the graph down.
    let (n_str, seed_str) = (N.to_string(), SEED.to_string());
    let (dev_str, go_str) = (
        dev.to_string_lossy().into_owned(),
        go.to_string_lossy().into_owned(),
    );
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
            "40000",
            "--timeout-ms",
            "60000",
        ],
        Some(&dev),
    );

    // `hostward_buffer = 8192` per §5/§15.19: the console boundary is deliberately
    // lossy, and a byte-exactness assertion needs the depth that keeps it lossless at
    // this volume.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 8192
"#,
        dev = dev.display(),
    );
    rpc.load_toml(&cfg, false).expect("load source config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    let (out, err) = (d.run().join("incident.bin"), d.run().join("incident.err"));
    let mut child = spawn_ctl_tap(&d.socket(), &["usb0"], &out, &err);
    assert!(
        wait_until(Duration::from_secs(10), || tap_count(rpc) == 1),
        "the CLI tap did not register (taps={:?})",
        rpc.state()["taps"]
    );

    // Release the source, then wait for the whole stream to reach the capture file.
    std::fs::File::create(&go)
        .and_then(|mut f| f.flush())
        .expect("touch GO gate");
    let captured_len = || std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    assert!(
        wait_until(Duration::from_secs(40), || captured_len() == N as u64),
        "the capture never reached {N} bytes (got {})",
        captured_len()
    );

    rpc.teardown();
    let status = wait_exit(&mut child, EXIT_BOUND);
    assert!(
        status.is_some(),
        "`serial-nexus-ctl tap usb0 > capture` never exited after teardown (CTL-1)"
    );
    assert!(status.expect("exited").success(), "orderly end of stream");

    // Byte-exact against an independently computed source stream: exiting on
    // `tap.closed` must not cost, reorder or re-write a single captured byte.
    let captured = std::fs::read(&out).expect("read capture");
    assert_eq!(
        sha256_hex(&captured),
        sha256_hex(&seeded_bytes(SEED, N)),
        "the capture is not byte-exact with the source ({} bytes)",
        captured.len()
    );

    // CTL-2's silent half, which *is* deterministic: a contiguous stream — many chunks
    // across 64 KiB — must raise no discontinuity notice at all. A tracker that cried
    // gap on every chunk would be as useless as the one that cried none, and only a
    // real multi-chunk stream exercises the `offset + len` arithmetic end to end.
    let notices =
        String::from_utf8(std::fs::read(&err).expect("read notices")).expect("stderr is utf-8");
    assert!(
        !notices.contains("tap gap"),
        "a lossless capture raised a discontinuity notice: {notices:?}"
    );
}
