//! Phase 8 browser-side console modules: the pure, DOM- and storage-free ES modules the
//! web console is built out of (§11.9 / design §15.32) — today the offset-splice +
//! retention core (`history.mjs`, spliced by the §11.8 tap offsets so a reload never
//! duplicates ring bytes) and the per-key write serializer (`saver.mjs`, which keeps two
//! overlapping full-buffer OPFS rewrites from truncating each other, review WEB-5). Both
//! are pure precisely so they are unit-testable outside a browser; their tests run here
//! under `node --test`.
//!
//! The runner discovers **every** `*.test.mjs` under `serialnexusweb/src/assets/` rather
//! than naming one file: this gate is the only place those tests are run in CI, so a
//! sibling test file added next to them must not be silently skipped (`saver.mjs`'s tests
//! were appended to `history.test.mjs` for exactly that reason — they no longer need to
//! be). The OPFS adapter itself is browser-only and rides the manual checklist (§16.7);
//! this gate covers the logic that must be correct. It **self-skips** when `node` is
//! absent (a skip is a valid verdict, §5), so CI runs it wherever a Node runtime exists.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn browser_console_modules_pass_their_node_tests() {
    let have_node = Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_node {
        eprintln!("SKIP browser_console_modules_pass_their_node_tests: node not found");
        return;
    }

    let assets = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../serialnexusweb/src/assets"
    ));
    // Every `*.test.mjs` in the assets dir, sorted so the command line is stable.
    let mut tests: Vec<PathBuf> = std::fs::read_dir(&assets)
        .unwrap_or_else(|e| panic!("read {}: {e}", assets.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".test.mjs"))
        })
        .collect();
    tests.sort();
    assert!(
        !tests.is_empty(),
        "no *.test.mjs found under {} — the browser modules' only CI gate would pass \
         vacuously",
        assets.display()
    );

    let out = Command::new("node")
        .arg("--test")
        .args(&tests)
        .output()
        .expect("run node --test");
    assert!(
        out.status.success(),
        "node --test on the browser console modules failed ({tests:?}):\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
