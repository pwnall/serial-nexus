#![forbid(unsafe_code)]

//! Phase 5 exec-codec conformance slice, ported from
//! `scripts/validate/phase5/exec-conformance.sh` (plan §10.5 / §15.26).
//!
//! The `serial-nexus-sim exec-conformance` battery drives an external codec child through
//! golden vectors, full-duplex liveness (the §15.22 deadlock class), fragmented-frame
//! reassembly, and kill-and-restart cleanliness. Three fixtures pin the behavior:
//!
//! 1. `passthrough.py` — a full-duplex passthrough — passes **every** check.
//! 2. `lag.py` — a *correct* bounded-lag codec (echoes one frame behind, flushes at
//!    EOF) — still passes liveness and restart; the check is not a lock-step ping-pong
//!    that would false-reject any legitimately buffering codec (§15.26).
//! 3. `half-duplex.py` — the read-all-before-writing antipattern — is **caught**:
//!    golden still passes (finite, closed input) but liveness fails, so the harness
//!    catches the §15.22 deadlock class rather than shipping it.
//!
//! This is an exec codec: it needs no serial device, only `python3` + `sh`, so it runs
//! on **every** platform. Ground truth is the sim's own structured JSON verdict
//! (`{pass, checks:{golden,liveness,fragmentation,restart}}`), never parsed CLI text.
//!
//! Deviations from the bash, and why (each preserves the original assertions):
//! * The bash's `cargo build -q -p serial-nexus-sim` precondition is dropped — `cargo test
//!   --workspace` already builds `serial-nexus-sim`, and [`serial_nexus_itest::bin`] asserts it exists.
//! * The bash `fail`s if `python3` is absent; a portable test instead **skips** (an
//!   environmental prerequisite, like a missing serial device). `sh -c` and `python3`
//!   are present on the Linux/macOS boxes this suite targets — and where they are
//!   provisioned on purpose, **`SNX_EXEC_CODEC=required`** turns the skip back into a
//!   failure ([`serial_nexus_itest::skip_no_exec_codec`], plan §3 rule 11 / §18 item
//!   49): ten of this file's tests self-skip on one missing interpreter, so an image
//!   that dropped it would have reported the whole battery green without running a
//!   byte of it.
//! * The three sub-checks become three self-contained `#[test]`s (each spawns its own
//!   sim), so a failure is attributable to one fixture.
//!
//! A fourth test guards the harness rather than a fixture: `--exec` reaches the child
//! through `sh -c`, so the fixture path is quoted and a spaced path is proved to run
//! (review 37, 37-EXTC-1).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use serial_nexus_itest::{TempRun, bin, skip_no_exec_codec};

/// Absolute path to a fixture under the workspace's `tests/ext-codec/`. Derived from
/// this crate's compile-time manifest dir (`itest/` — the directory, §15.40), so it is location- and
/// platform-independent — the portable replacement for the bash's `REPO_ROOT` dance.
fn ext_codec(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("serial-nexus-itest has a parent (the workspace root)")
        .join("tests")
        .join("ext-codec")
        .join(name)
}

/// The `--exec` string for a fixture, **single-quoted** (review 37, 37-EXTC-1).
///
/// The sim runs `--exec` through `sh -c` verbatim (`ExecChild::spawn`), so the path
/// has to survive word splitting: unquoted, a checkout directory containing a space
/// (`~/My Projects/serial-nexus`) split the command and failed all three conformance
/// tests below with the interpreter reporting a file it could not find — a harness
/// fault that reads as a codec fault. The sibling `p5_envelope.rs` quotes the same
/// path with a comment naming this exact hazard; this is that quoting, shared with
/// the guard that proves it.
fn exec_command(fixture: &Path) -> String {
    format!("python3 '{}'", fixture.display())
}

/// Whether `python3` is invocable — the fixtures are Python. Absent ⇒ the test skips
/// (an environmental prerequisite), mirroring how serial-device tests skip.
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Run `serial-nexus-sim exec-conformance --exec "python3 <fixture>" [extra…]` to completion
/// and return its JSON verdict. The sim exits non-zero when `pass == false` (e.g. the
/// half-duplex case), so we read stdout regardless of exit status and assert on the
/// structured verdict, never the exit code or any human text.
fn run_conformance(fixture: &str, extra: &[&str]) -> Value {
    run_conformance_at(&ext_codec(fixture), extra)
}

/// [`run_conformance`] against a fixture at an arbitrary path — the seam the spaced-path
/// guard needs, since the in-tree fixtures all live under a path this checkout chose.
fn run_conformance_at(fixture: &Path, extra: &[&str]) -> Value {
    let exec = exec_command(fixture);
    let out = Command::new(bin("serial-nexus-sim"))
        .arg("exec-conformance")
        .arg("--exec")
        .arg(&exec)
        .args(extra)
        .output()
        .expect("run serial-nexus-sim exec-conformance");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse exec-conformance verdict for {}: {e}; stdout={:?} stderr={:?}",
            fixture.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    })
}

/// A named boolean check from the verdict's `.checks` object; panics with the full
/// verdict if the field is missing (an errored/malformed run must fail loudly).
fn check(v: &Value, name: &str) -> bool {
    v.get("checks")
        .and_then(|c| c.get(name))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("verdict missing .checks.{name}: {v}"))
}

/// The `.error_paths` array entry for one arm, as `(pass, why)`. Panics if the arm
/// is absent: `--error-paths` promises one entry per injected fault, and a silently
/// missing arm is a check that did not run rather than one that passed.
fn error_arm(v: &Value, arm: &str) -> (bool, String) {
    let entry = v
        .get("error_paths")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("verdict has no .error_paths array: {v}"))
        .iter()
        .find(|e| e.get("arm").and_then(Value::as_str) == Some(arm))
        .unwrap_or_else(|| panic!("verdict has no .error_paths entry for {arm:?}: {v}"));
    (
        entry["pass"]
            .as_bool()
            .unwrap_or_else(|| panic!("arm {arm:?} has no boolean pass: {v}")),
        entry["why"].as_str().unwrap_or_default().to_owned(),
    )
}

// (1) The full-duplex passthrough passes every conformance check.
#[test]
fn passthrough_passes_every_conformance_check() {
    if !have_python3() {
        skip_no_exec_codec(
            "passthrough_passes_every_conformance_check",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("passthrough.py", &[]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "passthrough failed a conformance check: {v}"
    );
    assert!(check(&v, "golden"), "passthrough failed golden: {v}");
    assert!(check(&v, "liveness"), "passthrough failed liveness: {v}");
    assert!(
        check(&v, "fragmentation"),
        "passthrough failed fragmentation: {v}"
    );
    assert!(check(&v, "restart"), "passthrough failed restart: {v}");
}

// (2) A CORRECT bounded-lag codec (echoes one frame behind, flushes at EOF) still
// passes every check — the check is not a lock-step ping-pong that would reject any
// legitimately buffering codec (§15.26).
#[test]
fn bounded_lag_codec_is_not_wrongly_rejected() {
    if !have_python3() {
        skip_no_exec_codec(
            "bounded_lag_codec_is_not_wrongly_rejected",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("lag.py", &[]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "a valid bounded-lag codec was wrongly rejected: {v}"
    );
    assert!(
        check(&v, "liveness"),
        "bounded-lag codec wrongly failed liveness: {v}"
    );
    assert!(
        check(&v, "restart"),
        "bounded-lag codec wrongly failed restart: {v}"
    );
}

// (3) The deliberately half-duplex fixture is CAUGHT: golden still passes (finite,
// closed input), but liveness fails — the §15.22 deadlock class, made a test.
#[test]
fn half_duplex_fixture_caught_by_liveness() {
    if !have_python3() {
        skip_no_exec_codec(
            "half_duplex_fixture_caught_by_liveness",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("half-duplex.py", &["--frame-timeout-ms", "800"]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(false),
        "the half-duplex fixture was not caught (pass should be false): {v}"
    );
    assert!(
        !check(&v, "liveness"),
        "the half-duplex fixture was not caught by liveness: {v}"
    );
    assert!(
        check(&v, "golden"),
        "the half-duplex fixture should still pass golden (finite, closed input): {v}"
    );

    // …and the verdict says *why* it failed. §8 promises the battery names the
    // deadline when a check expires; plan §3's rule is that a deadline is never read
    // as a drop. A bare `liveness: false` cannot carry either, and for a while that is
    // all this verdict had: "your codec did not answer in time" and "your codec lost
    // frames" arrived as the same value, with an empty stderr.
    assert_eq!(
        v["timed_out"].as_array().map(|a| a
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()),
        Some(vec!["liveness"]),
        "the half-duplex failure is a deadline and must be reported as one: {v}"
    );
    let detail = v["details"]["liveness"].as_str().unwrap_or_default();
    assert!(
        detail.contains("of 64 frames") && detail.contains("ms"),
        "the liveness detail must say how many frames arrived and against what \
         deadline — that is what an author reads when CI goes red: {detail:?}"
    );
}

// (10) A passing run reports no deadline and no detail, so `timed_out` is a finding
// rather than a field that is always populated. Without this, (3)'s assertion above
// would pass just as well against a verdict that named a deadline unconditionally.
#[test]
fn a_passing_run_names_no_deadline() {
    if !have_python3() {
        skip_no_exec_codec("a_passing_run_names_no_deadline", "python3 not found");
        return;
    }
    let v = run_conformance("passthrough.py", &[]);
    assert_eq!(v["pass"].as_bool(), Some(true), "{v}");
    assert_eq!(
        v["timed_out"].as_array().map(Vec::len),
        Some(0),
        "a passing run must name no deadline: {v}"
    );
    assert_eq!(
        v["details"].as_object().map(serde_json::Map::len),
        Some(0),
        "a passing run must carry no failure detail: {v}"
    );
}

// (5) The demux shape, declared (plan §18 item 35). `passthrough-codec.py` is a real
// channel-swapping demux — the multiplexed side is the reserved empty channel
// identity, one real channel is its other face — which is the shape an exec codec
// author actually ships and the one the battery could not previously drive. With
// `--mux-to` naming the mapping, the *whole* battery runs against it: golden vectors
// through the swap plus liveness, fragmentation and restart driven on the multiplexed
// side.
#[test]
fn a_declared_demux_shape_runs_the_whole_battery() {
    if !have_python3() {
        skip_no_exec_codec(
            "a_declared_demux_shape_runs_the_whole_battery",
            "python3 not found",
        );
        return;
    }
    // The fixture takes its real channel as argv[1]; the flag must name the same one.
    let exec = format!(
        "python3 '{}' console",
        ext_codec("passthrough-codec.py").display()
    );
    let out = Command::new(bin("serial-nexus-sim"))
        .arg("exec-conformance")
        .arg("--exec")
        .arg(&exec)
        .args(["--mux-to", "console"])
        .output()
        .expect("run serial-nexus-sim exec-conformance --mux-to");
    let v: Value = serde_json::from_slice(&out.stdout).expect("parse the mapped verdict");

    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "the demux fixture failed the battery under its own declared mapping: {v}"
    );
    for name in ["golden", "liveness", "fragmentation", "restart"] {
        assert!(check(&v, name), "demux fixture failed {name}: {v}");
    }
    assert_eq!(
        v["mux_to"].as_str(),
        Some("console"),
        "the verdict must record the mapping it judged under: {v}"
    );
}

// (6) …and the mapping is load-bearing, not decorative: the same demux child fails
// the golden check when the battery is told nothing, because every `console` frame
// comes back on the empty channel. This is the fail-first proof for (5) — without it,
// (5) would pass just as well against a mapping the harness ignored.
#[test]
fn the_same_demux_fixture_fails_when_the_mapping_is_not_declared() {
    if !have_python3() {
        skip_no_exec_codec(
            "the_same_demux_fixture_fails_when_the_mapping_is_not_declared",
            "python3 not found",
        );
        return;
    }
    let exec = format!(
        "python3 '{}' console",
        ext_codec("passthrough-codec.py").display()
    );
    let out = Command::new(bin("serial-nexus-sim"))
        .arg("exec-conformance")
        .arg("--exec")
        .arg(&exec)
        .args(["--frame-timeout-ms", "800"])
        .output()
        .expect("run serial-nexus-sim exec-conformance without a mapping");
    let v: Value = serde_json::from_slice(&out.stdout).expect("parse the unmapped verdict");

    assert_eq!(
        v["pass"].as_bool(),
        Some(false),
        "an undeclared demux shape must not pass the identity battery: {v}"
    );
    assert!(
        !check(&v, "golden"),
        "the undeclared swap must be caught by golden — it relabels every frame: {v}"
    );
    assert!(
        v.get("mux_to").is_none(),
        "a run with no mapping declares none in its verdict: {v}"
    );
}

// (7) The error paths (plan §18 item 34). `strict.py` refuses each injected decode
// fault — unknown type byte, oversize length prefix, a channel length overrunning its
// body — and says so; the verdict names the arm and the byte offset of the fault.
#[test]
fn a_strict_codec_refuses_every_injected_decode_fault() {
    if !have_python3() {
        skip_no_exec_codec(
            "a_strict_codec_refuses_every_injected_decode_fault",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("strict.py", &["--error-paths"]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "the strict fixture failed a conformance check: {v}"
    );
    assert!(check(&v, "error_paths"), "strict failed error_paths: {v}");
    for (arm, offset) in [
        ("unknown_type", 4),
        ("oversize_length", 0),
        ("truncated_body", 5),
    ] {
        let (pass, why) = error_arm(&v, arm);
        assert!(pass, "strict failed the {arm} arm: {why}");
        let got = v["error_paths"]
            .as_array()
            .and_then(|a| a.iter().find(|e| e["arm"] == arm))
            .and_then(|e| e["offset"].as_u64());
        assert_eq!(
            got,
            Some(offset),
            "the {arm} arm must name the byte offset of the fault it injected: {v}"
        );
    }
}

// (8) …and the checks bite: `passthrough.py` re-encodes whatever it parses and
// validates nothing, so it fails all three arms — this battery's `Hoarder`. It is the
// in-tree fixture that emits garbage, so it is the fail-first proof for (7) and the
// standing evidence that a permissive relay is *visible* rather than merely allowed.
// It still passes every universal check (test 1 above), which is the honest statement:
// the error paths are opt-in because relaying is legal, not because it is invisible.
#[test]
fn a_permissive_codec_fails_every_error_path_arm() {
    if !have_python3() {
        skip_no_exec_codec(
            "a_permissive_codec_fails_every_error_path_arm",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("passthrough.py", &["--error-paths"]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(false),
        "a codec that relays malformed frames must not pass with --error-paths: {v}"
    );
    assert!(
        !check(&v, "error_paths"),
        "the permissive fixture was not caught by error_paths: {v}"
    );
    for arm in ["unknown_type", "oversize_length", "truncated_body"] {
        let (pass, _) = error_arm(&v, arm);
        assert!(
            !pass,
            "the permissive fixture wrongly passed the {arm} arm: {v}"
        );
    }
    // The two failure *shapes* are distinct and both must be reachable: relaying the
    // fault onward, and swallowing it silently. A check that only ever reported one
    // would be half a check.
    let (_, relayed) = error_arm(&v, "unknown_type");
    assert!(
        relayed.contains("echoed") || relayed.contains("malformed frame"),
        "the unknown-type arm should report the fault being passed onward: {relayed}"
    );
    let (_, swallowed) = error_arm(&v, "oversize_length");
    assert!(
        swallowed.contains("silently"),
        "the oversize arm should report the fault being swallowed: {swallowed}"
    );
}

// (9) Opt-in means absent, not false: a run without `--error-paths` carries neither
// the check nor the array, so every existing consumer of this verdict — and the four
// tests above it — sees the shape it always saw.
#[test]
fn error_paths_are_absent_from_a_run_that_did_not_ask_for_them() {
    if !have_python3() {
        skip_no_exec_codec(
            "error_paths_are_absent_from_a_run_that_did_not_ask_for_them",
            "python3 not found",
        );
        return;
    }
    let v = run_conformance("passthrough.py", &[]);
    assert_eq!(v["pass"].as_bool(), Some(true), "{v}");
    assert!(
        v["checks"].get("error_paths").is_none(),
        "an un-asked-for check must be absent, not false — false reads as a failure: {v}"
    );
    assert!(v.get("error_paths").is_none(), "{v}");
}

// (4) The exec string survives a fixture path containing a space (review 37,
// 37-EXTC-1). Proved end to end rather than by inspecting the string: the fixture is
// copied into a spaced directory and driven through the real sim, which is the only
// thing that establishes the quoting survives `sh -c`'s word splitting rather than
// merely Rust's formatting.
#[test]
fn a_fixture_path_containing_a_space_still_runs() {
    if !have_python3() {
        skip_no_exec_codec(
            "a_fixture_path_containing_a_space_still_runs",
            "python3 not found",
        );
        return;
    }
    let run = TempRun::new();
    let dir = run.join("ext codec");
    std::fs::create_dir_all(&dir).expect("create the spaced fixture directory");
    let fixture = dir.join("passthrough.py");
    std::fs::copy(ext_codec("passthrough.py"), &fixture).expect("copy the passthrough fixture");

    let v = run_conformance_at(&fixture, &[]);
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "the passthrough fixture failed from {} — the `--exec` string does not survive \
         `sh -c`'s word splitting, so every conformance test above fails on a checkout \
         whose path contains a space, blaming the codec for a harness fault: {v}",
        fixture.display()
    );
}
