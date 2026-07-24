//! Phase 8 browser-side history: the pure offset-splice + retention module (§11.9 /
//! design §15.32). The web console persists per-console scrollback in the browser's
//! Origin Private File System, spliced by the §11.8 tap offsets so a reload never
//! duplicates ring bytes. The splice/retention core lives in a DOM- and storage-free ES
//! module (`serialnexusweb/src/assets/history.mjs`) precisely so it is unit-testable; its
//! tests are `history.test.mjs`, run here under `node --test`.
//!
//! The OPFS adapter itself is browser-only and rides the manual checklist (§16.7); this
//! gate covers the logic that must be correct. It **self-skips** when `node` is absent (a
//! skip is a valid verdict, §5), so CI runs it wherever a Node runtime exists.

use std::process::Command;

#[test]
fn browser_history_offset_splice_module_passes_node_tests() {
    let have_node = Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_node {
        eprintln!("SKIP browser_history_offset_splice_module_passes_node_tests: node not found");
        return;
    }

    let test_file = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../serialnexusweb/src/assets/history.test.mjs"
    );
    let out = Command::new("node")
        .arg("--test")
        .arg(test_file)
        .output()
        .expect("run node --test");
    assert!(
        out.status.success(),
        "node --test on the history module failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
