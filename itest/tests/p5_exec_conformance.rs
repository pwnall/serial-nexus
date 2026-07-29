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
//!   are present on the Linux/macOS boxes this suite targets.
//! * The three sub-checks become three self-contained `#[test]`s (each spawns its own
//!   sim), so a failure is attributable to one fixture.
//!
//! A fourth test guards the harness rather than a fixture: `--exec` reaches the child
//! through `sh -c`, so the fixture path is quoted and a spaced path is proved to run
//! (review 37, 37-EXTC-1).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use serial_nexus_itest::{TempRun, bin};

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

// (1) The full-duplex passthrough passes every conformance check.
#[test]
fn passthrough_passes_every_conformance_check() {
    if !have_python3() {
        eprintln!("SKIP passthrough_passes_every_conformance_check: python3 not found");
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
        eprintln!("SKIP bounded_lag_codec_is_not_wrongly_rejected: python3 not found");
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
        eprintln!("SKIP half_duplex_fixture_caught_by_liveness: python3 not found");
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
}

// (4) The exec string survives a fixture path containing a space (review 37,
// 37-EXTC-1). Proved end to end rather than by inspecting the string: the fixture is
// copied into a spaced directory and driven through the real sim, which is the only
// thing that establishes the quoting survives `sh -c`'s word splitting rather than
// merely Rust's formatting.
#[test]
fn a_fixture_path_containing_a_space_still_runs() {
    if !have_python3() {
        eprintln!("SKIP a_fixture_path_containing_a_space_still_runs: python3 not found");
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
