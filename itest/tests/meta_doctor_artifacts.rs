//! The comparability rule, checked against the artifacts it is a rule about.
//!
//! `probe_set` equality was read as "these two reports are comparable field by
//! field". It is not: the digest covers each probe's question *string*, and
//! `docs/doctor/` holds four commits printing one `a131e1f4b46d6c83` across five
//! different observation sets (`df48bfc` makes a fifth commit and a sixth set, on
//! a box rather than in this directory). This gate freezes that counterexample,
//! and freezes the property that makes the second digest usable — it does not
//! move between sequential runs of one binary on one box.
//!
//! It drives the **shipped binary** rather than re-implementing the digest, so
//! what it proves is the execution of the real recompute path (§3), which is also
//! the path that computed the `field set` column of `docs/doctor/README.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use serial_nexus_itest::bin;

/// Repository root: this file is `<root>/itest/tests/meta_doctor_artifacts.rs`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("itest/ has a parent")
        .to_path_buf()
}

fn doctor_dir() -> PathBuf {
    repo_root().join("docs/doctor")
}

/// The digest as the shipped binary computes it. Panics rather than returning an
/// empty string when the recompute fails — a gate whose helper silently answers
/// `""` compares two empty strings and passes forever.
fn field_set(path: &Path) -> String {
    let out = Command::new(bin("serial-nexus-doctor"))
        .arg("--field-set")
        .arg(path)
        .output()
        .expect("run serial-nexus-doctor --field-set");
    assert!(
        out.status.success(),
        "--field-set {} failed: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let digest = String::from_utf8(out.stdout)
        .expect("utf8")
        .trim()
        .to_owned();
    assert_eq!(
        digest.len(),
        16,
        "--field-set {} printed {digest:?}",
        path.display()
    );
    digest
}

fn probe_set(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let v: Value = serde_json::from_str(&text).expect("artifact parses");
    v["build"]["probe_set"]
        .as_str()
        .unwrap_or_else(|| panic!("{} carries no build.probe_set", path.display()))
        .to_owned()
}

/// The committed counterexample to "equal `probe_set` ⇒ comparable field by
/// field", frozen so a future simplification of the digest cannot quietly erase
/// it.
#[test]
fn equal_probe_sets_do_not_mean_equal_field_sets_and_the_tree_proves_it() {
    let d = doctor_dir();
    let old = d.join("macos-24.6.0-2026-08-05-7ead470-tier3.json");
    let new = d.join("macos-24.6.0-2026-08-05-1a9a8fc-tier3.json");
    assert_eq!(
        probe_set(&old),
        probe_set(&new),
        "the counterexample needs these two to share a probe set"
    );
    assert_eq!(probe_set(&old), "a131e1f4b46d6c83");
    assert_ne!(
        field_set(&old),
        field_set(&new),
        "7ead470 and 1a9a8fc carry equal probe_set AND equal field_set — \
         the committed counterexample stopped being visible"
    );

    // The 4104x pair: six leaf paths separate them, and the digest must see it.
    let older = d.join("macos-24.6.0-2026-08-05-tier3.json");
    assert_eq!(probe_set(&older), probe_set(&old));
    assert_ne!(
        field_set(&older),
        field_set(&old),
        "the fa4b12d -> 7ead470 pair — one probe set, P10 hostward 4194304 -> 1022 — \
         reads as one shape"
    );

    // Pinned last, so the two discriminations above report themselves first. It
    // is the *field set* that has to be pinned rather than the probe set:
    // `probe_set()` reads a stored field out of a frozen file and so cannot see
    // the encoder move under it (measured — the length-delimiter mutation leaves
    // that assertion green), while `field_set()` recomputes and can.
    // `docs/doctor/README.md` publishes this exact value in its index, a column
    // no reader can check without re-running the tool.
    assert_eq!(
        field_set(&new),
        "0c303d4cb11e3893",
        "the recomputed field set of a frozen artifact moved — the `field set` \
         column of docs/doctor/README.md now publishes wrong digests"
    );
}

/// The other half of the discrimination: the digest is not run-to-run noise, or
/// it would redden every honest pair. Each group is three sequential runs of one
/// binary on one box.
#[test]
fn the_field_set_does_not_move_between_sequential_runs_of_one_binary() {
    let d = doctor_dir();
    for group in [
        [
            "macos-24.6.0-2026-08-05-1a9a8fc-tier3",
            "macos-24.6.0-2026-08-05-1a9a8fc-tier3-2",
            "macos-24.6.0-2026-08-05-1a9a8fc-tier3-3",
        ],
        [
            "macos-24.6.0-2026-08-05-7ead470-tier3",
            "macos-24.6.0-2026-08-05-7ead470-tier3-2",
            "macos-24.6.0-2026-08-05-7ead470-tier3-3",
        ],
        [
            "linux-7.0-2026-08-05b-tier3",
            "linux-7.0-2026-08-05b-tier3-2",
            "linux-7.0-2026-08-05b-tier3-3",
        ],
        [
            "linux-7.0-2026-08-05-tier3",
            "linux-7.0-2026-08-05-tier3-2",
            "linux-7.0-2026-08-05-tier3-3",
        ],
        [
            "linux-7.0-2026-07-29-passive-1",
            "linux-7.0-2026-07-29-passive-2",
            "linux-7.0-2026-07-29-passive-3",
        ],
    ] {
        let first = field_set(&d.join(format!("{}.json", group[0])));
        for name in &group[1..] {
            assert_eq!(
                first,
                field_set(&d.join(format!("{name}.json"))),
                "{} and {name} are sequential runs of one binary and their field \
                 sets differ — the digest is run-to-run noise, which would report \
                 every honest pair as incomparable",
                group[0]
            );
        }
    }
}

/// Emission path == recompute path, on real probe output rather than a fixture.
/// Passive (no `--port`): this opens no device.
#[test]
fn a_fresh_report_carries_the_digest_its_own_recompute_produces() {
    let out = Command::new(bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("run serial-nexus-doctor --json");
    assert!(out.status.success(), "doctor --json exited {}", out.status);
    let v: Value = serde_json::from_slice(&out.stdout).expect("doctor --json parses");
    let emitted = v["build"]["field_set"]
        .as_str()
        .expect("a fresh report carries build.field_set")
        .to_owned();

    let tmp = std::env::temp_dir().join(format!("snx-field-set-{}.json", std::process::id()));
    fs::write(&tmp, &out.stdout).expect("write temp report");
    let recomputed = field_set(&tmp);
    let _ = fs::remove_file(&tmp);

    assert_eq!(
        emitted, recomputed,
        "the digest the binary emitted and the digest it recomputes from its own \
         output disagree — the README index of docs/doctor/ is computed by the \
         recompute path"
    );
}

/// A gate that walks nothing passes forever: the tests above name their artifacts
/// by hand, so the floor is what proves the directory they name still exists.
#[test]
fn the_artifact_directory_still_holds_what_this_gate_indexes() {
    let n = fs::read_dir(doctor_dir())
        .expect("docs/doctor is readable")
        .filter(|e| {
            e.as_ref()
                .map(|e| e.path().extension() == Some("json".as_ref()))
                .unwrap_or(false)
        })
        .count();
    assert!(
        n >= 19,
        "only {n} JSON artifacts found in docs/doctor (floor 19)"
    );
}
