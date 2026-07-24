//! Phase 7 doctor probe P5, ported from `scripts/validate/phase7/p5.sh`
//! (design §13, §15.21 — rig discovery and certification without a bench).
//!
//! `nexus-doctor`'s P5 probe classifies each `--port` it is handed (dangling /
//! loopback / paired, verified in BOTH directions) and characterizes each as a real
//! UART or not. This test stands three all-software peers in front of it — a
//! `nexus-sim nullmodem` crossed pair, a `nexus-sim pty --stall` dangling port, and a
//! `nexus-sim pty --echo` loopback — and asserts P5 classifies every one correctly,
//! reporting `skipped (not a UART)` for the sim pts so the logic never waits for
//! hardware. On a real adapter bench the certificate populates fully; a `supported`
//! verdict on the sim rig (no adapter) is the CI outcome, a failing probe is not.
//!
//! The classes are exercised in two doctor runs, matching the bash: run 1 pairs
//! the nullmodem and flags the dangling port; run 2 exercises the echo loopback in
//! isolation, because a CPU-starved software echo peer sharing a run with other
//! active peers is timing-sensitive on a loaded box (a real TX↔RX jumper reflects
//! instantly in hardware with no process to schedule). Splitting keeps it
//! deterministic while validating every classification.
//!
//! P5 opens real serial ports through `serial2`, which a pty accepts only on Linux
//! (macOS → `ENOTTY`), so the whole test rides the Linux-only software-serial
//! doubles ([`serial_pair`]/[`serial_echo`]) and **skips** elsewhere — the doctor's
//! real-hardware P5 path is a separate, hardware-gated concern.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use nexus_itest::{Sim, TempRun, bin, serial_echo, serial_pair, wait_until};
use serde_json::{Value, json};

/// Run `nexus-doctor --json --port …` to completion and parse its JSON report.
/// The exit code is ignored (P5's own verdict is what we assert on, and an
/// unrelated probe going `unsupported` on some kernel must not fail this test —
/// the bash used `|| true` for the same reason).
fn run_doctor(ports: &[&str]) -> Value {
    let mut cmd = Command::new(bin("nexus-doctor"));
    cmd.arg("--json");
    for p in ports {
        cmd.arg("--port").arg(p);
    }
    let out = cmd.output().expect("run nexus-doctor");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse doctor json: {e}; stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The P5 probe object from a doctor report.
fn p5_probe(report: &Value) -> Value {
    report["probes"]
        .as_array()
        .expect("report has a probes array")
        .iter()
        .find(|p| p["id"] == json!("P5"))
        .cloned()
        .expect("report contains the P5 probe")
}

/// The string value of the P5 observation keyed by `key` (a port path or
/// `"<port> cert"`), panicking loudly if absent (the anti-tautology rule, §5).
fn obs(probe: &Value, key: &str) -> String {
    probe["observations"]
        .as_array()
        .expect("P5 has an observations array")
        .iter()
        .find(|o| o["key"].as_str() == Some(key))
        .and_then(|o| o["value"].as_str())
        .unwrap_or_else(|| {
            panic!(
                "no P5 observation keyed {key:?}; observations = {}",
                probe["observations"]
            )
        })
        .to_string()
}

#[test]
fn p5_classifies_paired_dangling_and_loopback_ports() {
    // Software-serial doubles are Linux-only (a pty is a serial device only there);
    // skip on macOS, where P5's real-hardware path is a separate concern.
    let Some(pair) = serial_pair() else {
        eprintln!(
            "SKIP: p7_p5 needs software serial doubles (Linux sim pty); \
             serial_pair() is None on this platform"
        );
        return;
    };
    let (a, b) = pair.ports();
    let (a, b) = (a.to_string(), b.to_string());

    // A dangling port: a `pty --stall` that stays present but never reflects a byte.
    let dangle_run = TempRun::new();
    let dangle_path = dangle_run.join("dangle");
    let dangle = dangle_path.to_string_lossy().into_owned();
    let _dangle = Sim::spawn(
        &[
            "pty",
            "--stall",
            "--link",
            &dangle,
            "--timeout-ms",
            "600000",
        ],
        Some(&dangle_path),
    );

    // `serial_pair` only waits for the first pts; make sure the second half of the
    // null modem (and thus the whole rig) is present before the doctor opens ports.
    let both_up = wait_until(Duration::from_secs(5), || Path::new(&b).exists());
    assert!(both_up, "null-modem second port never appeared at {b}");

    // ---- Run 1: paired (both directions) + dangling -------------------------
    let r1 = run_doctor(&[a.as_str(), b.as_str(), dangle.as_str()]);
    let p1 = p5_probe(&r1);
    assert_eq!(
        p1["status"],
        json!("supported"),
        "P5 run 1 not supported: {p1}"
    );

    // The pair must classify as paired, and cite each other — both directions, so a
    // half-crossed rig cannot slip through (§15.21).
    let va = obs(&p1, &a);
    assert!(
        va.contains(format!("paired with {b}").as_str()),
        "pair_a not paired with pair_b (got: {va})"
    );
    let vb = obs(&p1, &b);
    assert!(
        vb.contains(format!("paired with {a}").as_str()),
        "pair_b not paired with pair_a — asymmetric (got: {vb})"
    );

    // The dangling port hears nothing wired to it.
    let vd = obs(&p1, &dangle);
    assert!(
        vd.to_lowercase().contains("dangling"),
        "dangle not classified dangling (got: {vd})"
    );

    // Characterization skips the non-UART sim pts for every port.
    for port in [&a, &b, &dangle] {
        let cert = obs(&p1, &format!("{port} cert"));
        assert!(
            cert.to_lowercase().contains("not a uart"),
            "{port} characterization not skipped(not a UART) (got: {cert})"
        );
    }

    // ---- Run 2: loopback (in isolation, see the module note) ----------------
    let echo = serial_echo().expect("serial_echo() is Some on Linux (serial_pair was Some)");
    let loopdev = echo.device().to_string_lossy().into_owned();
    let r2 = run_doctor(&[loopdev.as_str()]);
    let p2 = p5_probe(&r2);
    assert_eq!(
        p2["status"],
        json!("supported"),
        "P5 run 2 not supported: {p2}"
    );
    let vl = obs(&p2, &loopdev);
    assert!(
        vl.to_lowercase().contains("loopback"),
        "echo device not classified loopback (got: {vl})"
    );
}
