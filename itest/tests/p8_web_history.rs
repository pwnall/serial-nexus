#![forbid(unsafe_code)]

//! Phase 8 browser-side console modules: the pure, DOM- and storage-free ES modules the
//! web console is built out of (plan §11.9 / design §15.32) — today the offset-splice +
//! retention core (`history.mjs`, spliced by the plan §11.8 tap offsets so a reload never
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
//! **Until 2026-08-21 the gate asserted the runner's exit status and nothing about
//! execution.** `node --test` counts a file that declares no tests as a *passing test* —
//! measured on node v24.19.0 on this box: replacing all three `*.test.mjs` with a single
//! comment line makes it print `tests 3 / suites 0 / pass 3 / fail 0` and exit **0**,
//! where the real files print `tests 56 / pass 56`. Fifty-six assertions could therefore
//! go to zero with this gate green, which is AGENTS §3's tell in its purest form: the
//! passing output is identical to the not-running output. Two of the six registers
//! recorded there apply at once — the gate asserted a proxy (a process's exit status) for
//! the property it promises (that the browser modules' tests *ran* and passed), and its
//! doc comment claimed a coverage its code did not enforce.
//!
//! What it asserts now, per file rather than over the batch: node runs it under
//! `--test-reporter=tap`, the summary block is parsed out of the TAP stream, and the file
//! is held to the number of top-level `test(` declarations **it itself contains**. The
//! floor is derived from the file rather than hand-kept, so adding a test never edits
//! this one (AGENTS §3: enumerate from tools, never from a hand-kept list).
//!
//! **A derived floor alone would not have caught the gutting plant, and saying why is the
//! useful half of this entry.** The gut removes a file's declarations *and* its
//! executions together, so the floor falls with the count it is guarding: `0 >= 0` is
//! green, and a gate that derives its floor from its own subject can be defeated by
//! deleting the subject. What catches it is the companion assertion that **every
//! discovered file declares at least one test**, so a file that declares none is a red
//! test naming that file. Node's own report cannot carry that assertion: an emptied file
//! run on its own reports `tests 1 / pass 1`, the single test point named after the file.
//! The same assertion is what keeps the *matcher* from going quietly blind — see
//! `declared_tests`.
//!
//! **The fail-first pass then found the same class of hole in this gate's own parser, and
//! that is the entry worth keeping.** `tap_summary` promised to read the counts only from
//! after the TAP plan line, because a test's stdout rides the same stream — and it took
//! the *last* value for each key, under which that promise asserts nothing: a test's
//! forged lines necessarily precede the reporter's block, so the real values overwrite
//! them and a whole-stream scan returns the identical answer. Planting exactly the defect
//! the comment named — the loop widened to `&lines[..]` — left the fixture green. The
//! parser now takes the *first* value per key, which costs nothing on real output and
//! turns that plant red (`tests: 999` against `tests: 2`). Sixth register, AGENTS §3: a
//! guard whose assertion is weaker than the comment above it, and the comment is what a
//! reviewer reads.
//!
//! **Three gates under `itest/` shell a foreign test runner and adopt its verdict, and
//! the other two were swept before this one was written.** `p8_web_ui.rs` had already
//! learned the lesson ("a suite that ran zero specs exits 0, so prove it actually executed
//! the specs") and holds `npx playwright test` to a passing-spec floor, with hand-kept
//! `SPECS_*` constants because a Playwright spec count cannot be read off the source.
//! `p8_external_codec.rs` runs `cargo test` over the external-consumer template and
//! carried this defect too at `800915b` — `.status()` and nothing else — which is a
//! separate change against a file this one does not own. Everything else that shells out
//! is a fixture or a tool rather than a runner: `python3` in
//! `p5_exec_conformance`/`p5_exec_orphans`/`p5_exec_crash`/`p5_envelope`/`p12_codec_signals`/
//! `p13_teardown_accounting`, `bash` and `sh` in `p12_log_queue`/`p5_exec_orphans`,
//! `stty`, `mkfifo`, `curl`, `id`, `kill`, `ps`, `systemd-run` — processes whose behaviour
//! the Rust test then measures itself. The one borderline case is `jq` in
//! `expectation_gates.rs`, whose verdict *is* adopted; it already passes `-e` (without
//! which jq prints `false` and exits 0, AGENTS §3's first recorded instance of this tell)
//! and that gate asserts the reject direction as well as the accept one.
//!
//! **What this gate still cannot see** (stated rather than implied): a file whose thirty
//! tests are rewritten down to one trivial passing `test()`. Declared and executed both
//! read 1 and it is green. The promise here is that every test a file declares is a test
//! node actually ran and passed — not that the declarations are worth anything, which is
//! review's job.
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

    // One node process per file. The batch form (`node --test a b c`) is cheaper — 0.116 s
    // against 0.327 s for these three, measured on this box — but its TAP stream is flat:
    // every test point is numbered in one sequence with no file attribution, so a batch
    // run can say "56 passed" and cannot say *which file* contributed none of them. That
    // binding is the whole assertion here, and 0.2 s buys it.
    for file in &tests {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<non-utf8>")
            .to_owned();
        let src = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let declared = declared_tests(&src);
        assert!(
            declared > 0,
            "{name} declares no top-level `test(` — either the file has been emptied of \
             its tests (which `node --test` reports as one *passing* test named after the \
             file, so the runner will never say so), or it is written in a spelling \
             `declared_tests` does not know. Both are holes and both are this failure. \
             Fix: restore the tests, or teach `declared_tests` the new spelling and plant \
             a violation in it — a scanning gate proves its matcher as well as its \
             walker (AGENTS §3).",
        );

        let out = Command::new("node")
            .arg("--test")
            .arg("--test-reporter=tap")
            .arg(file)
            .output()
            .expect("run node --test");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            out.status.success(),
            "node --test on {name} failed:\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        );

        // Exit status was the old gate in its entirety, so it is deliberately *not* what
        // decides the counts below: `fail` is read out of the reporter's own summary even
        // though a failing test also exits non-zero. A gate that reads one signal twice
        // is a gate that reads one signal.
        let Some(sum) = tap_summary(&stdout) else {
            panic!(
                "no TAP summary block in `node --test --test-reporter=tap {name}` — the \
                 reporter's output shape has changed and this gate is now measuring \
                 nothing. Fix: re-read `node --help` for the reporter set and update \
                 `tap_summary` (and its fixture test below, which carries the shape this \
                 was written against).\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        };
        assert_eq!(
            sum.fail, 0,
            "{name}: node reported {} failing of {} — --- stdout ---\n{stdout}\n\
             --- stderr ---\n{stderr}",
            sum.fail, sum.tests,
        );
        assert!(
            sum.pass >= declared,
            "{name} declares {declared} top-level tests but node reported only {} passing \
             (of {} run). A test that was skipped, cancelled, marked todo, or never \
             reached — a `return` above it, a throw at import time that node charged to \
             the file rather than to a test — reads exactly like this, and none of those \
             counts toward `pass`, which is why the floor is against `pass` and not \
             against `tests`.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            sum.pass,
            sum.tests,
        );
    }
}

/// The counts `node --test --test-reporter=tap` prints in its trailing summary block.
///
/// Only three of the seven are kept, because only three are load-bearing. `skipped`,
/// `todo` and `cancelled` need no field: none of them counts toward `pass`, so
/// `pass >= declared` already fails when a declared test takes any of those exits, and
/// the message says so. `suites` and `duration_ms` are not assertions.
#[derive(Debug, Default, PartialEq, Eq)]
struct TapSummary {
    tests: usize,
    pass: usize,
    fail: usize,
}

/// True for the reporter's own TAP plan line — `1..56` and nothing else on the line.
fn is_tap_plan(line: &str) -> bool {
    line.strip_prefix("1..")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Parse the summary block that follows the TAP plan line.
///
/// **Reading it from after the plan is load-bearing, and the escaping is not what makes
/// it so.** A test's own stdout rides the same TAP stream as comments. Node v24.19.0
/// escapes a *leading* `#` — a test that `console.log`s `# pass 999` comes out as
/// `# \# pass 999`, three tokens, which the shape check below rejects — but it escapes
/// nothing else, so a test that logs `tests 999` comes out as `# tests 999`, **byte for
/// byte the reporter's own summary line**. Measured on this box; there is no shape a
/// parser can use to tell those two apart. Position is the only thing that separates
/// them.
///
/// **First writer wins per key, which is what makes that rule observable.** The first
/// draft of this took the last, and under last-writer-wins the position rule asserts
/// nothing at all: a test's forged lines necessarily precede the reporter's block, so the
/// real values overwrite them and a whole-stream scan returns the same answer. Planting
/// exactly that defect — the loop widened to `&lines[..]` — left the fixture test below
/// green, which is AGENTS §3's "weaker than its comment" register, in this guard, found
/// by planting the defect the comment named and watching nothing happen. First-writer-
/// wins costs nothing on real output (the reporter prints each key once) and turns the
/// same plant red.
///
/// `rposition` rather than `position` because a subtest's own plan can appear indented
/// above it; only the last unindented one closes the run.
///
/// `None` means the summary was absent or incomplete — all three keys are required — and
/// the caller treats it as a hard failure rather than as zero counts. A parser that
/// silently returns zeroes when the format moves is the same vacuous gate this file
/// exists to have stopped being: `0 >= 0` is green.
fn tap_summary(stdout: &str) -> Option<TapSummary> {
    let lines: Vec<&str> = stdout.lines().collect();
    let plan = lines.iter().rposition(|l| is_tap_plan(l))?;
    let (mut tests, mut pass, mut fail) = (None, None, None);
    for line in &lines[plan + 1..] {
        let Some(rest) = line.strip_prefix("# ") else {
            continue;
        };
        let mut words = rest.split_whitespace();
        let (Some(key), Some(value), None) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        let Ok(n) = value.parse::<usize>() else {
            continue;
        };
        let slot = match key {
            "tests" => &mut tests,
            "pass" => &mut pass,
            "fail" => &mut fail,
            _ => continue,
        };
        slot.get_or_insert(n);
    }
    Some(TapSummary {
        tests: tests?,
        pass: pass?,
        fail: fail?,
    })
}

/// Count the top-level test declarations in one `*.test.mjs` — the floor the gate holds
/// `node` to having actually executed for that file.
///
/// The matcher is one spelling wide: a line beginning at column 0 with `test(`. That is
/// the only spelling under `web/src/assets/`, and it agrees with node exactly —
/// 16 (`ansi`) + 10 (`graph`) + 30 (`history`) against node's own `# pass 56`, measured
/// 2026-08-21. It does **not** know `it(`, `describe(`, `test.skip(`, or an indented
/// subtest, and the narrowness is deliberate twice over: a broader regex would start
/// counting occurrences inside comments and template literals, and `test.skip(` must not
/// raise the floor, because a skipped test is not a passing one.
///
/// A narrow matcher's failure mode is undercounting, and undercounting a *floor* is
/// silent — which would put the vacuity back one level down, in the matcher instead of in
/// the assertion. That is why the caller hard-fails on a zero: a file written entirely in
/// a spelling this does not know becomes a red test naming that file and asking for the
/// spelling to be added *and planted*, rather than a floor that quietly slid to zero.
fn declared_tests(src: &str) -> usize {
    src.lines().filter(|l| l.starts_with("test(")).count()
}

/// The matcher's own fail-first proof, kept in the tree rather than in a transcript.
///
/// Each spelling below was run through the real thing before it was written down here:
/// `^test(` is the one that counts, and the four near-misses are the ones that must not,
/// because counting them would either raise the floor above what node reports
/// (`test.skip`, which node counts under `skipped` and never under `pass`) or count text
/// that is not a declaration at all (the commented and template-literal forms).
#[test]
fn the_declared_test_matcher_counts_only_the_spelling_it_claims() {
    assert_eq!(declared_tests("test(\"a\", () => {});\n"), 1);
    assert_eq!(
        declared_tests("test(\"a\", () => {});\ntest('b', () => {});\n"),
        2
    );
    // Everything the matcher deliberately does not know. Each of these in a file of its
    // own yields 0, and 0 is a hard failure in the caller — loud, not silent.
    assert_eq!(
        declared_tests("  test(\"indented subtest\", () => {});\n"),
        0
    );
    assert_eq!(declared_tests("it(\"bdd spelling\", () => {});\n"), 0);
    assert_eq!(declared_tests("describe(\"a suite\", () => {});\n"), 0);
    assert_eq!(declared_tests("test.skip(\"not a pass\", () => {});\n"), 0);
    assert_eq!(declared_tests("// test(\"in a comment\")\n"), 0);
    assert_eq!(declared_tests(""), 0);
}

/// The TAP parser against the outputs that decide this gate, all captured verbatim from
/// node v24.19.0 on this box.
///
/// The first is the *gutted* shape — the plant that proved the old gate asserted nothing:
/// three emptied files, `pass 3`, exit 0. Note that it parses cleanly and reports three
/// passing tests; the parser is not what rejects that plant, `declared_tests` is.
///
/// The second is the forgery, and it is the *unescaped* one on purpose. Node escapes a
/// leading `#`, so `console.log("# pass 999")` arrives as `# \# pass 999` and dies on the
/// two-token shape check — which is why an earlier draft of this fixture, built from that
/// spelling, could not fail. `console.log("tests 999")` arrives as `# tests 999`, which no
/// shape check can tell from the reporter's own line. Widening the parser to `&lines[..]`
/// turns this case red, and that is the whole point of it being here.
#[test]
fn the_tap_summary_parser_reads_only_the_reporters_own_lines() {
    let gutted = "TAP version 13\n\
                  # Subtest: ansi.test.mjs\n\
                  ok 1 - ansi.test.mjs\n  ---\n  duration_ms: 23.6\n  type: 'test'\n  ...\n\
                  1..3\n# tests 3\n# suites 0\n# pass 3\n# fail 0\n# cancelled 0\n\
                  # skipped 0\n# todo 0\n# duration_ms 33.8\n";
    assert_eq!(
        tap_summary(gutted),
        Some(TapSummary {
            tests: 3,
            pass: 3,
            fail: 0
        })
    );

    let forged = "TAP version 13\n\
                  # tests 999\n# pass 999\n# fail 0\n\
                  # Subtest: prints a summary that is not escaped\n\
                  ok 1 - prints a summary that is not escaped\n\
                  # Subtest: a second real one\nok 2 - a second real one\n\
                  1..2\n# tests 2\n# suites 0\n# pass 2\n# fail 0\n# cancelled 0\n\
                  # skipped 0\n# todo 0\n# duration_ms 50.4\n";
    assert_eq!(
        tap_summary(forged),
        Some(TapSummary {
            tests: 2,
            pass: 2,
            fail: 0
        })
    );

    // No summary at all is `None`, never a default-zero struct — a node that died at
    // import time, or a reporter whose shape moved, must reach the caller's panic and not
    // its `pass >= declared` comparison, where `0 >= 0` would be green.
    assert_eq!(tap_summary(""), None);
    assert_eq!(tap_summary("TAP version 13\nok 1 - passes\n"), None);
    // A plan line with no summary after it is equally not a summary.
    assert_eq!(tap_summary("TAP version 13\nok 1 - passes\n1..1\n"), None);
    // And a *partial* block is not one either: a reporter that stopped printing `fail`
    // must not read as a run with no failures.
    assert_eq!(
        tap_summary("TAP version 13\nok 1 - passes\n1..1\n# tests 1\n# pass 1\n"),
        None
    );
}
