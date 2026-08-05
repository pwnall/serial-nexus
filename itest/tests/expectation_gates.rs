//! The expectation files are gates, and a gate proves its **matcher** (AGENTS §3).
//!
//! `expectations/linux.jq` and `expectations/macos.jq` are the per-platform gates CI
//! runs against live `serial-nexus-doctor --json` output. Every clause in them was
//! previously asserted only by CI passing on a healthy box — which is precisely the
//! shape that let the P4 defect through: the probe reported `supported` with a
//! population of zero, and both files admitted it, on Darwin for three consecutive
//! captures (`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`, notes
//! §3.45 (ii)). A gate nobody has ever seen fail is not known to be a gate.
//!
//! So this file plants the violation, in every spelling the clause claims to cover —
//! a `supported` P4 whose `canonical` count is zero, and one whose `canonical` key is
//! missing entirely — and requires the platform's own expectation file to **reject**
//! both, having first accepted the unmutated report from the same binary.
//!
//! It self-skips without `jq`, the same way the Playwright gate self-skips without
//! node (AGENTS §3); CI has jq, because CI runs these files.
//!
//! The fixture is Darwin's shape reproduced on Linux through the `--dev-root` seam
//! (plan §3) — four `cu.*` callout nodes, no by-id tree, no by-path tree, no sysfs —
//! so the report under test is the one the defect was found in rather than a
//! stand-in for it (§9, no proxy in space).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("doctor/ has a parent")
        .to_path_buf()
}

/// The expectation file this box's CI lane actually gates on. Running the *other*
/// platform's file here would be a proxy in space (§9).
fn platform_expectation() -> PathBuf {
    let name = if cfg!(target_os = "macos") {
        "expectations/macos.jq"
    } else {
        "expectations/linux.jq"
    };
    repo_root().join(name)
}

fn have_jq() -> bool {
    Command::new("jq")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// `jq -e -f <file>` over `json`, returning whether the gate accepted it.
fn gate_accepts(expectation: &Path, json: &str) -> bool {
    let out = Command::new("jq")
        .arg("-e")
        .arg("-f")
        .arg(expectation)
        .arg("/dev/stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .as_mut()
                .expect("stdin piped")
                .write_all(json.as_bytes())?;
            c.wait_with_output()
        })
        .expect("jq runs");
    out.status.success()
}

/// One `jq` filter over `json`, as a string.
fn jq_filter(filter: &str, json: &str) -> String {
    let out = Command::new("jq")
        .arg(filter)
        .arg("/dev/stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .as_mut()
                .expect("stdin piped")
                .write_all(json.as_bytes())?;
            c.wait_with_output()
        })
        .expect("jq runs");
    assert!(out.status.success(), "jq filter failed: {filter}");
    String::from_utf8(out.stdout).expect("jq emits utf-8")
}

struct TmpTree(PathBuf);

impl TmpTree {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("snx-expect-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dev")).unwrap();
        TmpTree(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_p4_clause_rejects_a_certified_resolver_that_resolved_nothing() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let t = TmpTree::new("p4");
    for n in [
        "cu.usbserial-BH00L4KU",
        "cu.usbserial-BH00LL8O",
        "cu.Bluetooth-Incoming-Port",
        "cu.BLTH",
    ] {
        std::fs::write(t.path().join("dev").join(n), b"").unwrap();
    }
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--dev-root")
        .arg(t.path())
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // The honest report from this fixture must PASS. A gate that rejects the truth is
    // not a stricter gate, it is a broken one.
    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest report: {}",
        expectation.display(),
        jq_filter(r#".probes[] | select(.id=="P4")"#, &report)
    );

    // Spelling 1 — the defect exactly as captured on Darwin: `supported` over a
    // population of zero.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P4") | .status) = "supported""#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P4 whose canonical count is 0 — the vacuous pass \
         it exists to refuse (§9, notes §3.45 (ii))",
        expectation.display()
    );

    // Spelling 2 — the same claim from a report that does not carry the population at
    // all, which is what every pre-2026-08-05 binary emitted. Also rejected, and that is
    // the deliberate half: this gate is only ever run against a LIVE report (CI pipes
    // `--json` straight into it; nothing runs it over `docs/doctor/*.json`), so a report
    // that cannot state its population is one no operator will meet — a capture just
    // taken comes from the current binary and carries the key. Requiring it therefore
    // costs nothing real and keeps the clause saying what it means.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P4") | .observations) |= map(select(.key != "canonical"))
           | (.probes[] | select(.id=="P4") | .status) = "supported""#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P4 with no `canonical` observation at all",
        expectation.display()
    );
}

/// The proof above runs against one platform's file per box. This is what carries it
/// to the other: the two gates must hold the *same* clause, byte for byte, so an edit
/// to one that forgets the other fails here instead of silently reopening the hole on
/// the lane nobody is standing on.
#[test]
fn both_expectation_files_carry_the_same_p4_population_clause() {
    let clause = r#"and (all(.probes[]; . as $p
      | ($p.id != "P4")
      or ($p.status != "supported")
      or (((([$p.observations[] | select(.key == "canonical") | .value] | first) // -1)) > 0)))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        assert!(
            text.contains(clause),
            "{name} does not carry the P4 population clause verbatim — the behavioural \
             proof in this file runs on one platform's gate and reaches the other only \
             through this identity"
        );
    }
}
