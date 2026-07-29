//! Phase 8 browser-side console modules: the pure, DOM- and storage-free ES modules the
//! web console is built out of (§11.9 / design §15.32) — today the offset-splice +
//! retention core (`history.mjs`, spliced by the §11.8 tap offsets so a reload never
//! duplicates ring bytes) and the per-key write serializer (`saver.mjs`, which keeps two
//! overlapping full-buffer OPFS rewrites from truncating each other, review WEB-5). Both
//! are pure precisely so they are unit-testable outside a browser; their tests run here
//! under `node --test`.
//!
//! The runner discovers **every** `*.test.mjs` under `web/src/assets/` rather
//! than naming one file: this gate is the only place those tests are run in CI, so a
//! sibling test file added next to them must not be silently skipped (`saver.mjs`'s tests
//! were appended to `history.test.mjs` for exactly that reason — they no longer need to
//! be; `ansi.test.mjs` arrived later and was picked up by nothing more than existing).
//! The OPFS adapter itself is browser-only and rides the manual checklist (§16.7); this
//! gate covers the logic that must be correct.
//!
//! **The skip has an escape hatch, because it had to** (review 32 TESTR-6). A missing
//! `node` is still a skip — that concession is what lets the suite run on a machine
//! without a JS runtime (§5) — but the skip was not merely silent, it was *invisible*:
//! `eprintln!` from a **passing** test is captured by libtest, so a CI transcript showed
//! `... ok` and nothing else, on the one gate that runs `history.test.mjs` (the sole test
//! of `offsetSpaceChanged`, the splice arithmetic and `saver.mjs`'s write serialisation).
//! No job provisioned node for it and no job asserted it had run, so the whole thing
//! rested on the runner image happening to ship one. Setting **`SNX_WEB_UI=required`**
//! now turns the skip into a failure — the same knob and the same reasoning as
//! `p8_web_ui.rs`, and the `web-ui` CI job, which installs node anyway, sets it and runs
//! this test too. Optional on a laptop, mandatory where it is provisioned.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn browser_console_modules_pass_their_node_tests() {
    let required = std::env::var("SNX_WEB_UI").as_deref() == Ok("required");
    let have_node = Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_node {
        assert!(
            !required,
            "SNX_WEB_UI=required, but `node` was not found. Fix: install Node.js.\n\
             (This job is expected to run the browser console modules' unit tests; a \
             skip here would be a gate passing over a hole — plan §3 rule 7 — and it \
             would not even be visible, because libtest captures a passing test's \
             stderr.)"
        );
        eprintln!("SKIP browser_console_modules_pass_their_node_tests: node not found");
        return;
    }

    let assets = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/src/assets"));
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
