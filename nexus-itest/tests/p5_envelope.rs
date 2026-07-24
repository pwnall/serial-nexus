//! Phase 5 any-language envelope conformance, ported from
//! `scripts/validate/phase5/envelope.sh` (design §8, plan §Phase 5 item 3).
//!
//! `nexus-sim envelope` drives an external codec child — here the stdlib-only Python
//! passthrough (`tests/ext-codec/passthrough.py`) — through the golden-vector battery:
//! every event kind (open/data/close/error) plus edge cases (empty payload, binary
//! payload, a long channel id, back-to-back frames on one channel). Each vector is
//! encoded to the child's stdin and decoded back from its stdout, then compared frame
//! for frame. A conforming child re-emits the exact 10-frame sequence (§8).
//!
//! Needs NO serial *device* — the child speaks the envelope over a plain pipe — so this
//! runs on every platform. It does require an external `python3` (the any-language
//! escape hatch's demonstration child); absent it, the test skips with a note rather
//! than flaking an environmental failure.
//!
//! Assertions preserved from the bash's single `jq -e` gate over the verdict:
//!   * `.pass == true`
//!   * `.sent_frames == .received_frames`
//!   * `.received_frames == 10`
//!   * `.trailing_bytes == 0`

use std::path::PathBuf;
use std::process::Command;

use nexus_itest::bin;
use serde_json::Value;

/// The workspace root — the parent of this crate's manifest directory. The passthrough
/// child lives at `<repo>/tests/ext-codec/passthrough.py` (as the bash's `$REPO_ROOT`
/// resolved it).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nexus-itest crate has a parent (the workspace root)")
        .to_path_buf()
}

/// Whether an external `python3` is on PATH (the child interpreter `nexus-sim` will
/// `sh -c`). The bash treated its absence as a hard failure; here it is a portable skip
/// so a box without python3 does not flake a false regression.
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn envelope_passthrough_child_reemits_golden_battery() {
    if !have_python3() {
        eprintln!("SKIP envelope_passthrough_child_reemits_golden_battery: no python3 on PATH");
        return;
    }
    let passthrough = repo_root().join("tests/ext-codec/passthrough.py");
    assert!(
        passthrough.exists(),
        "envelope child not found at {} — the passthrough codec is part of the repo",
        passthrough.display()
    );

    // `nexus-sim envelope --exec "python3 <passthrough.py>"`: the child reads envelope
    // frames on stdin and re-emits them on stdout; the sim decodes and diffs against the
    // golden battery. Single-quote the path so a spaced repo path survives `sh -c`.
    let exec = format!("python3 '{}'", passthrough.display());
    let out = Command::new(bin("nexus-sim"))
        .arg("envelope")
        .arg("--exec")
        .arg(&exec)
        .output()
        .expect("run nexus-sim envelope");
    let verdict: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse nexus-sim envelope verdict: {e}; stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });

    // Faithful port of the bash's `jq -e` conjunction over the verdict.
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "envelope battery did not pass: {verdict}"
    );
    let sent = verdict["sent_frames"].as_u64();
    let received = verdict["received_frames"].as_u64();
    assert_eq!(
        sent, received,
        "sent_frames != received_frames (child dropped/added frames): {verdict}"
    );
    assert_eq!(
        received,
        Some(10),
        "received_frames != 10 (the golden battery is 10 frames): {verdict}"
    );
    assert_eq!(
        verdict["trailing_bytes"].as_u64(),
        Some(0),
        "child left trailing (undecodable) bytes: {verdict}"
    );
}
