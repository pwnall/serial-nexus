//! Phase 7 doctor probe P5, ported from `scripts/validate/phase7/p5.sh`
//! (design §13, §15.21 — rig discovery and certification without a bench).
//!
//! `serial-nexus-doctor`'s P5 probe classifies each `--port` it is handed (dangling /
//! loopback / paired, verified in BOTH directions) and characterizes each as a real
//! UART or not. This test stands three all-software peers in front of it — a
//! `serial-nexus-sim nullmodem` crossed pair, a `serial-nexus-sim pty --stall` dangling port, and a
//! `serial-nexus-sim pty --echo` loopback — and asserts P5 classifies every one correctly,
//! reporting `skipped (not a UART)` for the sim pts so the logic never waits for
//! hardware. On a real adapter bench the certificate populates fully; a `supported`
//! verdict on the sim rig (no adapter) is the CI outcome, a failing probe is not.
//!
//! All four classes are exercised in **one** doctor run, over one four-port rig.
//! Until this file was rewritten they were split across two runs, with the echo
//! loopback probed alone; the comment here blamed CPU starvation ("a software echo
//! peer sharing a run with other active peers is timing-sensitive on a loaded
//! box"). That was a misdiagnosis, and the split was a workaround for a real defect
//! in the double rather than a scheduling concession: `serial-nexus-sim`'s `pty --echo`
//! treated a *slave close* as terminal (a bare `POLLHUP`/`EIO` on the pty master
//! broke its loop), so the first probe that opened and closed the port killed the
//! double for good. A jumper is stateless — it does not stop reflecting because
//! something unplugged from it once — and P5 itself opens every port twice
//! (discovery, then characterization), so an echo double could never survive a
//! multi-port run. It is fixed in the double; the split is gone with it.
//!
//! [`a_probe_that_opens_and_closes_the_port_leaves_the_loopback_alive`] is the
//! guard for that defect, and it is deliberately harsher than P5's own reopen: it
//! runs the whole doctor **twice** against one double.
//!
//! P5 opens real serial ports through `serial2`, which a pty accepts only on Linux
//! (macOS → `ENOTTY`), so the whole test rides the Linux-only software-serial
//! doubles ([`serial_pair`]/[`serial_echo`]) and **skips** elsewhere — the doctor's
//! real-hardware P5 path is a separate, hardware-gated concern.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{Sim, TempRun, bin, serial_echo, serial_pair, wait_until};

/// `serial-nexus-doctor --json --port …`, not yet started.
fn doctor_cmd(ports: &[&str]) -> Command {
    let mut cmd = Command::new(bin("serial-nexus-doctor"));
    cmd.arg("--json");
    for p in ports {
        cmd.arg("--port").arg(p);
    }
    cmd.stdout(Stdio::piped());
    cmd
}

/// Parse a finished doctor run's JSON report.
fn doctor_report(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse doctor json: {e}; stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Run `serial-nexus-doctor --json --port …` to completion and parse its JSON report.
/// The exit code is ignored (P5's own verdict is what we assert on, and an
/// unrelated probe going `unsupported` on some kernel must not fail this test —
/// the bash used `|| true` for the same reason).
fn run_doctor(ports: &[&str]) -> Value {
    let out = doctor_cmd(ports).output().expect("run serial-nexus-doctor");
    doctor_report(&out)
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

/// A `pty --stall` double: present, opens cleanly, never reflects a byte — the
/// dangling-port stand-in. Returns the sim (kill it by dropping it) and its path.
fn stall_double(run: &TempRun, name: &str) -> (Sim, String) {
    let path = run.join(name);
    let s = path.to_string_lossy().into_owned();
    let sim = Sim::spawn(
        &["pty", "--stall", "--link", &s, "--timeout-ms", "600000"],
        Some(&path),
    );
    (sim, s)
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
    let (_dangle_sim, dangle) = stall_double(&dangle_run, "dangle");

    // A loopback port: a `pty --echo`, the software TX↔RX jumper. It shares this run
    // with the other three doubles on purpose — see the module note; the split that
    // used to isolate it was a workaround for a defect in the double, not a real
    // scheduling limit.
    let echo = serial_echo().expect("serial_echo() is Some on Linux (serial_pair was Some)");
    let loopdev = echo.device().to_string_lossy().into_owned();

    // `serial_pair` only waits for the first pts; make sure the second half of the
    // null modem (and thus the whole rig) is present before the doctor opens ports.
    let both_up = wait_until(Duration::from_secs(5), || Path::new(&b).exists());
    assert!(both_up, "null-modem second port never appeared at {b}");

    // ---- One run over the whole rig: paired (both directions) + dangling + loopback
    let r1 = run_doctor(&[a.as_str(), b.as_str(), dangle.as_str(), loopdev.as_str()]);
    let p1 = p5_probe(&r1);
    assert_eq!(p1["status"], json!("supported"), "P5 not supported: {p1}");

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

    // The dangling port hears nothing wired to it — and is *dangling*, not hung up:
    // its double stays present the whole time, which is the distinction the hung-up
    // class must not blur (see the hung-up guard below).
    let vd = obs(&p1, &dangle);
    assert!(
        vd.to_lowercase().contains("dangling"),
        "dangle not classified dangling (got: {vd})"
    );

    // The echo double is the jumper.
    let vl = obs(&p1, &loopdev);
    assert!(
        vl.to_lowercase().contains("loopback"),
        "echo device not classified loopback (got: {vl})"
    );

    // Characterization skips the non-UART sim pts for every port.
    for port in [&a, &b, &dangle, &loopdev] {
        let cert = obs(&p1, &format!("{port} cert"));
        assert!(
            cert.to_lowercase().contains("not a uart"),
            "{port} characterization not skipped(not a UART) (got: {cert})"
        );
    }
}

/// A probe that opens a port and closes it again must leave a loopback double
/// exactly as it found it — a TX↔RX jumper is stateless.
///
/// This is the regression guard for the `serial-nexus-sim pty --echo` defect described in
/// the module note (a bare `POLLHUP`/`EIO` on the pty master ended the echo loop, so
/// the *first* close killed the double). It is written against the doctor rather
/// than against a hand-rolled open/close because the doctor is where the defect
/// surfaced: P3 opens and closes every named port before P5 opens it, and P5 itself
/// closes its discovery opens before reopening each port to characterize it.
///
/// Two whole doctor runs, not two opens: an in-process reopen lands sub-microsecond
/// after the close, and at a 0 µs gap the broken double survived 25/25 — the window
/// has to be *deliberately* in the fatal region (measured against the unfixed
/// double: 0 µs → 0% fatal, 20 µs → 36%, 30 µs → 92%, 50 ms → 100%). A separate
/// process launch is milliseconds wide, and the second run then finds the corpse:
/// against the unfixed double this test failed 3/3 with the port reported
/// `open error: No such file or directory` (the dead sim removes its own symlink),
/// and passes 10/10 with it fixed. Do not "simplify" the second run into a second
/// open in this process — that is precisely the version that cannot fail.
#[test]
fn a_probe_that_opens_and_closes_the_port_leaves_the_loopback_alive() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP: p7_p5 needs software serial doubles (Linux sim pty); \
             serial_echo() is None on this platform"
        );
        return;
    };
    let loopdev = echo.device().to_string_lossy().into_owned();

    // Run 1 opens the port (P3, then P5 discovery, then P5 characterization) and
    // closes it three times over, then exits.
    let p1 = p5_probe(&run_doctor(&[loopdev.as_str()]));
    let v1 = obs(&p1, &loopdev);
    assert!(
        v1.to_lowercase().contains("loopback"),
        "echo device not classified loopback on the first run (got: {v1})"
    );

    // Run 2: same rig, same double, nothing has been unplugged. The jumper is still
    // a jumper.
    let p2 = p5_probe(&run_doctor(&[loopdev.as_str()]));
    assert_eq!(
        p2["status"],
        json!("supported"),
        "P5 not supported on the second run — the echo double did not survive the \
         first probe's close: {p2}"
    );
    let v2 = obs(&p2, &loopdev);
    assert!(
        v2.to_lowercase().contains("loopback"),
        "echo device not classified loopback on the second run — a close killed the \
         jumper (got: {v2})"
    );
}

/// A port whose peer hangs up *during* P5 is reported as hung up, not as dangling.
///
/// The two are opposite operator instructions: `dangling (nothing wired to it)`
/// says "this port has no partner — wire it up", while the truth here is "the
/// partner was there and went away mid-probe — re-seat it and re-run". Every write
/// fails `EIO` and the port polls `POLLIN|POLLHUP` with nothing to read, which the
/// classifier used to fold into its no-partner branch — the same misreport a real
/// hot-unplug during P5 produces, in the binary §3 tells users to attach to bug
/// reports.
///
/// The double is killed 1.2 s into the run, which is inside P5's 4 s discovery
/// window (P3 on a single port finishes in ~0.1 s) and cannot be waited for with a
/// condition: the induced hangup *is* the stimulus, not a readiness gate, so the
/// sleep is the subject of the test rather than a violation of §5's no-bare-sleeps
/// rule. Do not replace it with a `wait_until`.
#[test]
fn p5_reports_a_peer_that_hangs_up_mid_probe_as_hung_up_not_dangling() {
    if serial_echo().is_none() {
        eprintln!(
            "SKIP: p7_p5 needs software serial doubles (Linux sim pty); \
             serial_echo() is None on this platform"
        );
        return;
    }
    let run = TempRun::new();
    let (sim, port) = stall_double(&run, "vanish");

    let child = doctor_cmd(&[port.as_str()])
        .spawn()
        .expect("spawn serial-nexus-doctor");
    std::thread::sleep(Duration::from_millis(1200));
    drop(sim); // the peer hangs up, mid-discovery
    let out = child
        .wait_with_output()
        .expect("wait for serial-nexus-doctor");

    let p = p5_probe(&doctor_report(&out));
    let v = obs(&p, &port);
    assert!(
        v.to_lowercase().contains("hung up"),
        "a peer that hung up mid-probe was not reported as hung up (got: {v})"
    );
    assert!(
        !v.to_lowercase().contains("dangling"),
        "a hung-up peer is still being reported as dangling — the operator is told \
         to wire up a port that was wired (got: {v})"
    );
    // An unclassifiable port must not certify the rig as clean: the certificate is
    // the precondition every tiered run starts from (§15.21, DOC-1b).
    assert_eq!(
        p["status"],
        json!("degraded"),
        "P5 certified a rig it could not classify: {p}"
    );
}

/// The P14 probe object from a doctor report.
fn p14_probe(report: &Value) -> Value {
    report["probes"]
        .as_array()
        .expect("report has a probes array")
        .iter()
        .find(|p| p["id"] == json!("P14"))
        .cloned()
        .expect("report contains the P14 probe")
}

/// **P14's plumbing runs against the software null modem, and its claim does not**
/// (design §15.51, plan §18 item 11).
///
/// This is the guard for the one way a ceiling search can be catastrophically
/// wrong: a pts pair carries bytes perfectly at every rate, because nothing clocks
/// it. `serial-nexus-sim nullmodem` reads `discovered_pairs: 1` in P5, so a P14 that
/// gated on the pair *count* would climb the whole ladder, pass every rung
/// including `u32::MAX`, and report `structural-cap` — a confident wire number
/// with no wire, on the platform CI actually runs. The gate is [`p5_is_uart`],
/// which a pts fails on both kernels, and the assertion below is that P14 says so
/// **positively** rather than merely declining to answer.
///
/// The skip is asserted, never inferred from silence: a report with no P14 block
/// at all would satisfy "did not claim a ceiling" while proving the opposite of
/// what this test is for, so `p14_probe` panics on absence and the reason string is
/// pinned. That, and not the verdict word, is what makes the matcher provable —
/// `p5_classifies_paired_dangling_and_loopback_ports` above pins the same
/// `not a UART` spelling for P5's certificate, so the two cannot drift apart.
#[test]
fn p14_reports_skipped_not_a_uart_against_the_software_null_modem() {
    let Some(pair) = serial_pair() else {
        eprintln!(
            "SKIP: p7_p5 needs software serial doubles (Linux sim pty); \
             serial_pair() is None on this platform"
        );
        return;
    };
    let (a, b) = pair.ports();
    let (a, b) = (a.to_string(), b.to_string());
    let both_up = wait_until(Duration::from_secs(5), || Path::new(&b).exists());
    assert!(both_up, "null-modem second port never appeared at {b}");

    let report = run_doctor(&[a.as_str(), b.as_str()]);

    // The precondition this test would be vacuous without: P5 really did pair the
    // two halves in both directions. If discovery failed, P14 would skip for the
    // *wrong* reason ("no verified cross-paired rig") and the UART gate below would
    // never have been reached.
    let p5 = p5_probe(&report);
    assert!(
        obs(&p5, &a).starts_with("paired with"),
        "the null modem was not discovered as a pair, so this test cannot reach \
         P14's UART gate: {}",
        obs(&p5, &a)
    );

    let p14 = p14_probe(&report);
    assert_eq!(
        p14["status"],
        json!("skipped"),
        "P14 must not answer over a software null modem: {p14}"
    );
    let reason = p14["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("not a UART") || reason.contains("not characterizable"),
        "P14 skipped for the wrong reason — the pair WAS discovered, so the skip \
         must name the missing clock, not the missing rig: {reason:?}"
    );

    // The plumbing ran: the probe is present, it saw the pair, and it said so.
    // Without these the assertions above are satisfied by a probe that never
    // executed at all.
    let pairs_seen = p14["observations"]
        .as_array()
        .expect("P14 has an observations array")
        .iter()
        .find(|o| o["key"] == json!("pairs_discovered"))
        .and_then(|o| o["value"].as_u64())
        .expect("P14 reports how many pairs it was handed");
    assert_eq!(
        pairs_seen, 1,
        "P14 was not handed the discovered pair, so its skip is about the rig \
         being absent rather than about the pts having no clock"
    );

    // And the claim itself is absent. A `max_reliable_baud` **value** on a pts is
    // the exact failure this guard exists to catch, whatever the verdict word says.
    //
    // The assertion is on the *value*, not on the key, and the difference is a
    // collision between two correct requirements. `expectations/*.jq` needs the
    // cell **present** on every non-skipped path — a `degraded` report that omits
    // it is refused, which is how a marginal cable used to redden the lane — and
    // `field_set` wants the cell set stable between a passive run and a rig-less
    // one. So the probe stamps the three cells on every path and fills them in
    // only when the search runs. This guard originally asserted the key was
    // absent, and that assertion started failing the moment the cells were
    // stamped: correct guard, superseded shape. What it must refuse is a
    // *ceiling*, and a `null` is the probe saying it has none.
    let ceiling = p14["observations"]
        .as_array()
        .expect("P14 has an observations array")
        .iter()
        .find(|o| o["key"] == json!("max_reliable_baud"))
        .map(|o| o["value"].clone())
        .expect("P14 carries the cell on every path, so the gate admits its degrades");
    assert_eq!(
        ceiling,
        Value::Null,
        "P14 reported a ceiling for a software null modem: {}",
        p14["observations"]
    );
}
