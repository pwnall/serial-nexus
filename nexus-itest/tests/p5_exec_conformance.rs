//! Phase 5 exec-codec conformance slice, ported from
//! `scripts/validate/phase5/exec-conformance.sh` (plan §10.5 / §15.26).
//!
//! The `nexus-sim exec-conformance` battery drives an external codec child through
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
//! * The bash's `cargo build -q -p nexus-sim` precondition is dropped — `cargo test
//!   --workspace` already builds `nexus-sim`, and [`nexus_itest::bin`] asserts it exists.
//! * The bash `fail`s if `python3` is absent; a portable test instead **skips** (an
//!   environmental prerequisite, like a missing serial device). `sh -c` and `python3`
//!   are present on the Linux/macOS boxes this suite targets.
//! * The three sub-checks become three self-contained `#[test]`s (each spawns its own
//!   sim), so a failure is attributable to one fixture.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use nexus_itest::bin;
use serde_json::Value;

/// Absolute path to a fixture under the workspace's `tests/ext-codec/`. Derived from
/// this crate's compile-time manifest dir (`nexus-itest/`), so it is location- and
/// platform-independent — the portable replacement for the bash's `REPO_ROOT` dance.
fn ext_codec(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nexus-itest has a parent (the workspace root)")
        .join("tests")
        .join("ext-codec")
        .join(name)
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

/// Run `nexus-sim exec-conformance --exec "python3 <fixture>" [extra…]` to completion
/// and return its JSON verdict. The sim exits non-zero when `pass == false` (e.g. the
/// half-duplex case), so we read stdout regardless of exit status and assert on the
/// structured verdict, never the exit code or any human text.
fn run_conformance(fixture: &str, extra: &[&str]) -> Value {
    let exec = format!("python3 {}", ext_codec(fixture).display());
    let out = Command::new(bin("nexus-sim"))
        .arg("exec-conformance")
        .arg("--exec")
        .arg(&exec)
        .args(extra)
        .output()
        .expect("run nexus-sim exec-conformance");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse exec-conformance verdict for {fixture}: {e}; stdout={:?} stderr={:?}",
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
