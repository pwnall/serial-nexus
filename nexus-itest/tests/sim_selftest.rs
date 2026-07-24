//! Phase 1 sim self-test, ported from `scripts/validate/phase1/sim-selftest.sh`
//! (plan §4): calibrate the judges before they judge. A `nexus-sim pty --echo`
//! double against a `nexus-sim client --send seeded:1MiB --expect echo` must
//! round-trip with matching checksums. Pure sim (no daemon, no serial device) —
//! runs on every platform (the macOS pty-double fix makes this hold there too).

use std::time::Duration;

use nexus_itest::{Sim, TempRun, wait_until};
use serde_json::json;

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
