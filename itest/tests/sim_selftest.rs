//! Phase 1 sim self-test, ported from `scripts/validate/phase1/sim-selftest.sh`
//! (plan §4): calibrate the judges before they judge. A `serial-nexus-sim pty --echo`
//! double against a `serial-nexus-sim client --send seeded:1MiB --expect echo` must
//! round-trip with matching checksums. Pure sim (no daemon, no serial device) —
//! runs on every platform (the macOS pty-double fix makes this hold there too).
//!
//! The second test calibrates a different judge, and the calibration is the same idea:
//! `wire --stall` is the §9 conformance driver's head-of-line probe, and every mode's
//! contract is "print a single JSON verdict line on exit". It could not keep that one
//! against the very peer it exists to characterize (37-TOOL-4) — see
//! [`wire_stall_still_reaches_its_verdict_against_a_peer_that_never_reads`].

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Sim, TempRun, bin, wait_until};

#[test]
fn sim_pty_echo_round_trips_1mib() {
    let run = TempRun::new();
    let link = run.join("dut");
    let _pty = Sim::spawn(
        &[
            "pty",
            "--echo",
            "--link",
            &link.to_string_lossy(),
            "--timeout-ms",
            "20000",
        ],
        Some(&link),
    );
    // Presence-before-send: the double publishes the link before it is draining, but
    // the client opens+holds the slave, so a brief settle is enough for the echo loop.
    assert!(
        wait_until(Duration::from_secs(2), || link.exists()),
        "pty link never appeared"
    );

    let v = Sim::client(&[
        "--path",
        &link.to_string_lossy(),
        "--send",
        "seeded:1MiB",
        "--expect",
        "echo",
        "--seed",
        "42",
        "--timeout-ms",
        "20000",
    ]);
    assert_eq!(
        v["pass"],
        json!(true),
        "sim echo round-trip did not pass: {v}"
    );
    assert_eq!(v["sent"], json!(1_048_576), "unexpected sent size: {v}");
    assert_eq!(v["sent"], v["received"], "sent != received: {v}");
    assert_eq!(
        v["sha256_sent"], v["sha256_received"],
        "echo checksum mismatch (bytes mangled): {v}"
    );
}

// ---- `wire --stall` must reach its verdict -------------------------------------

/// A §9 `hello` frame in the v1 wire layout:
/// `u32 body_len | u32 magic | u16 version | u32 capabilities | u16 count |
/// count × (u16 len | UTF-8 identity)`, all big-endian.
///
/// Hand-rolled for the reason `p12_leg_accounting.rs`'s twin states: `serial-nexus-itest`
/// does not depend on `serial-nexus-codec-api`, and a stand-in that re-derives the bytes is a
/// better witness than one calling the encoder under test. The shared home for the two
/// copies would be the harness crate, which this file does not own.
fn hello_frame(channels: &[&str]) -> Vec<u8> {
    const WIRE_MAGIC: u32 = 0x534E_584C; // "SNXL"
    const WIRE_VERSION: u16 = 1;
    let mut body = Vec::new();
    body.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
    body.extend_from_slice(&WIRE_VERSION.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes()); // capabilities
    body.extend_from_slice(&(channels.len() as u16).to_be_bytes());
    for ch in channels {
        body.extend_from_slice(&(ch.len() as u16).to_be_bytes());
        body.extend_from_slice(ch.as_bytes());
    }
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

/// How long the stalled peer holds, and how long the sim may then take to say so.
/// The bound is many times the hold because the failure it guards is *unbounded*: a
/// slow runner must not be able to make a hang look like a pass.
const STALL_HOLD_MS: &str = "3000";
const VERDICT_BOUND: Duration = Duration::from_secs(20);

/// 37-TOOL-4 — `wire --stall` against a peer that stops reading must still exit with
/// its verdict line.
///
/// The mode's whole purpose is to *be* the peer that never reads, so that the daemon's
/// targetward direction backs up (§9's head-of-line property). Its own hostward writes
/// were issued with a blocking `write_all` and the hold deadline was checked only
/// between rounds — so the instant its counterpart also stopped reading, the sim parked
/// in `write_all` with no deadline at all: no verdict line, no exit, and a §9
/// conformance run that hangs instead of reporting. SIM-1's shape recurring.
///
/// The counterpart here is a twenty-line stand-in rather than a daemon, deliberately:
/// the point is a peer that *provably* never reads a byte, and a daemon leg reads
/// hostward by design. It completes the handshake so the sim's own `pass` reflects the
/// handshake (which succeeded) rather than the stall.
///
/// What is asserted is the contract, not the mechanism: an exit inside the bound, a
/// parseable verdict, and the stall named in it — with `framed_hostward` above
/// `streamed_hostward`, which is the divergence that only exists when the writes really
/// were cut short. The bounded wait itself is a `poll(POLLOUT)`, not a retry loop, so
/// the mode stays inside plan §3 rule 2.
#[test]
fn wire_stall_still_reaches_its_verdict_against_a_peer_that_never_reads() {
    let run = TempRun::new();
    let sock = run.join("stall.sock");
    let listener = UnixListener::bind(&sock).expect("bind the stand-in peer socket");

    // The stand-in holds the accepted connection open (so the sim's writes fill the
    // socket buffer rather than failing) and never reads it. `release` is what ends it:
    // dropping the sender at the end of the test wakes the thread, which then drops the
    // stream. Parking on a channel is neither a sleep nor a spin.
    let (release, wait_for_release) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        // Speak first, exactly as a leg does on accept, so the sim's handshake half
        // completes and the stall is the only thing under test.
        let _ = stream.write_all(&hello_frame(&["c0"]));
        let _ = stream.flush();
        let _ = wait_for_release.recv();
    });

    let mut child = Command::new(bin("serial-nexus-sim"))
        .args(["wire", "--transport", "unix"])
        .arg("--address")
        .arg(&sock)
        .args(["--announce", "c0"])
        .args(["--stall", "--hold-ms", STALL_HOLD_MS])
        .args(["--timeout-ms", "10000"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serial-nexus-sim wire --stall");

    let exited = wait_exit(&mut child, VERDICT_BOUND);
    assert!(
        exited,
        "`serial-nexus-sim wire --stall` never exited against a peer that stopped \
         reading — it is parked in a blocking write with no deadline, so the §9 \
         conformance driver's verdict-on-exit contract is broken (37-TOOL-4)"
    );

    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_to_string(&mut out)
        .expect("read the verdict line");
    let verdict: Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("parse the wire verdict: {e}; stdout={out:?}"));

    assert_eq!(
        verdict.get("behavior").and_then(Value::as_str),
        Some("stall"),
        "not the stall verdict: {verdict}"
    );
    assert_eq!(
        verdict.get("write_stalled").and_then(Value::as_bool),
        Some(true),
        "the peer never read a byte, so the writes must be reported as cut short: \
         {verdict}"
    );
    let framed = verdict
        .get("framed_hostward")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("no framed_hostward in {verdict}"));
    let streamed = verdict
        .get("streamed_hostward")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("no streamed_hostward in {verdict}"));
    assert!(
        framed > streamed,
        "the two byte counts agree, so nothing was actually held back — the stall was \
         not reached: framed={framed} streamed={streamed} ({verdict})"
    );

    drop(release);
}

/// Wait up to `bound` for `child` to exit, killing and reaping it otherwise so a
/// regression is a bounded failure and never a leaked process.
fn wait_exit(child: &mut Child, bound: Duration) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        match child.try_wait().expect("try_wait on serial-nexus-sim") {
            Some(_) => return true,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}
