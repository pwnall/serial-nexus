#![forbid(unsafe_code)]

//! **Derive-from-tools meta-gates** (plan §18 item 40; AGENTS §3's closing rule).
//!
//! Six rosters in this tree exist twice: once as the code, manifest or registry
//! that *is* the thing, and once as a list a human typed — a Markdown table, a
//! module-doc parenthetical, a `[[bin]]` block. A list typed once is correct once.
//! AGENTS §3 states the doctrine for the one instance that already bit ("CI
//! enumerates loops from tools (`cargo fuzz list`), never hand-kept lists"), and
//! plan §18 item 40 names the first four pairs still kept by hand; items 62 and 63
//! added (e) and (f), which are the same doctrine applied to two *invariants*
//! rather than to two lists:
//!
//! * **(a)** [`every_required_mode_variable_the_harness_reads_appears_in_plan_3s_table`]
//!   — the `SNX_*=required` lattice. Plan §3's table says outright that it "is the
//!   reader's map; the authority is the harness code that reads these variables",
//!   which is a promise nothing was keeping: `SNX_TLS` arrived as the third
//!   instance of the one `required` mechanism and `SNX_RIG_FLOW` as the fourth
//!   (AGENTS §3), and the next one to land without a row is invisible to the
//!   operator who has to type the rig lane from the documentation. The gate reads
//!   the *comparisons* — an `SNX_*` variable measured against the literal
//!   `required` — so the roster is the code's, not a copy of it.
//! * **(b)** [`every_documented_probe_roster_matches_the_doctor_registry`] — the
//!   doctor's probe roster, which appears **twice** outside `probes.rs` — as
//!   `docs/serial-nexus-doctor.md`'s registry table and the design's §13 glance
//!   table — plus §13 rule 1's prose count of it, which this gate also reads.
//!   §16.13 makes probe ids the citation key for every kernel claim in the tree, so
//!   a roster that has quietly lost a probe is a reader looking up a citation that
//!   is not there.
//!
//!   It was three until 2026-08-12: a parenthetical in `doctor/src/main.rs`'s module
//!   doc, which this gate reddened on arrival three probes stale and which the item
//!   sanctioned deleting rather than repairing (notes §3.75). The step that read it
//!   is kept — it fires only if the enumeration comes back — because the cheap way
//!   for a copy to return is for someone to find the absence surprising.
//!
//!   **The two enumerations this gate declined are now held elsewhere.**
//!   `expectations/{linux,macos}.jq` each carry a per-probe `.id == "PN"` clause, so
//!   a probe with no clause was simply ungated there. That is a gate-coverage
//!   question rather than a roster-drift one, and it is still not this gate's — it
//!   is `expectation_gates.rs`'s
//!   `every_probe_the_doctor_emits_is_named_by_a_clause_in_both_expectation_files`,
//!   which reads its roster from a live report rather than from this file's registry
//!   because what a CI gate sees is a report (plan §18 item 64(d)). Measured before
//!   it landed, with a seventeenth probe planted in `doctor/src/probes.rs`: the
//!   doctor exited 0 and `jq -e -f expectations/linux.jq` exited 0, while the gate
//!   below reddened — for the *documentation* roster, which is the half it owns.
//! * **(c)** [`every_fuzz_target_file_is_registered_and_every_registration_has_a_file`]
//!   — the `[[bin]]` table of `fuzz/Cargo.toml` against the sources under
//!   `fuzz/fuzz_targets/`. This is the missing link under two claims already made
//!   elsewhere: CI's fuzz loop enumerates `cargo fuzz list`, whose comment promises
//!   "a target added under `fuzz/fuzz_targets/` is now fuzzed the night it lands",
//!   and `meta_gates.rs`'s `every_unstable_fuzz_api_export_has_a_fuzz_target`
//!   satisfies §15.26's re-export bargain from the **directory listing**. Neither
//!   is true of an unregistered file: `cargo fuzz list` prints `[[bin]]` names, so
//!   a `.rs` file nobody registered is built never and fuzzed never while still
//!   answering "yes, a target drives it" to the re-export gate. Bijection here
//!   composes with that gate to give §8's actual rule — an item re-exported through
//!   `unstable_fuzz_api` is driven by a target that is *built and run*.
//! * **(d)** [`the_documented_rpc_verb_table_matches_the_daemon_and_ctl`] — the
//!   method index in `docs/rpc/README.md` against the verbs the daemon dispatches
//!   and the verbs `serial-nexus-ctl` sends. `docs/rpc/` is the schema authority
//!   and states the contract in those terms ("Only the methods documented on the
//!   pages above are live"), which is a claim about a table nobody was checking.
//!   `serial-nexus-rpc`'s own `docs_rpc_table_matches_the_registry` already does
//!   this for the **error-code** table in the same file; the verb index is the half
//!   that had no gate.
//! * **(e)** [`every_numeric_configuration_attribute_is_bounded_and_says_so`] —
//!   §16.12/§11 invariant 13: "every numeric attribute … carries a stated,
//!   structurally checked maximum". That promise was enforced by seven hand-written
//!   `range_error(…)` sites, each behind an `if let NodeConfig::X { …, .. }` whose
//!   `..` means a new field triggers no compiler complaint, plus a property test
//!   over a fixed list of four fields — so the *exhaustiveness* was per-field, and
//!   was already violated: `Pty { advertised_baud }` had no bound at all (plan §18
//!   item 62). This gate derives the field roster from the schema instead, so the
//!   invariant is checked over whatever `NodeConfig` currently declares rather than
//!   over what someone remembered to list. It reads **three** things, because the
//!   promise has three parts: *stated* against `docs/rpc/configuration.md`'s range
//!   table, *structurally checked* against the validator, and — since 2026-08-15 —
//!   that the check **constrains**. The third was missing and the omission was the
//!   same class this file exists for: the gate read a field's *name* out of a
//!   `range_error(…)` call and never its window, so replacing `baud`'s
//!   `1, u32::MAX as u64` with `0, u64::MAX` left it green over a call that can never
//!   fire. It now resolves both ends — literals in any base, `uN::MAX`, the four
//!   `MAX_*` constants, `as` casts — and reports any window spanning the whole of the
//!   field's declared type. **Stated residual**: it does not judge whether a window is
//!   the *right* one. `1 ..= u32::MAX` passes because it refuses `0`; whether
//!   `u32::MAX` is a sensible ceiling for a baud rate is §7.1's deliberate answer, and
//!   what covers it is `core/src/config.rs`'s per-field tests and its `validate`
//!   property test, not a structural scan.
//! * **(f)** [`every_state_key_the_daemon_emits_is_documented`] — design §5 makes
//!   `docs/rpc/observation.md` "the authoritative per-kind enumeration and stays
//!   so", which was a claim about a document nothing read: the error-code table and
//!   the verb index are gated two ways each while the largest surface on the wire
//!   was checked only by hand-written per-field tests that *cite* the page without
//!   reading it. Measured on arrival: twenty-nine keys the daemon emits appeared
//!   nowhere on that page, `delivered_hostward` — which the design itself names at
//!   §5 as the counter `p6_head_of_line.rs` reads — among them (plan §18 item 63).
//!
//!   Neither (e) nor (f) is a roster-drift gate wearing a new hat. A missing row in
//!   (a)–(d) is a reader misled; a missing bound in (e) is an invariant that reads
//!   as upheld because six of its seven instances are, and a missing key in (f) is a
//!   wire surface with no schema.
//!
//! **What makes each of these a gate rather than a decoration** (AGENTS §3: "a
//! scanning gate proves its **matcher** as well as its walker — plant the violation
//! in every spelling it claims to cover"). Every test below plants its own
//! violation before it trusts a clean verdict, in each layer it has:
//!
//! 1. *the matcher*, against synthetic text in every spelling the rule covers, and
//!    against the near-misses that must **not** trip it (a mention in a comment, a
//!    variable named inside a skip message, a parameter variable that is not a
//!    required mode);
//! 2. *the walker or listing*, wherever a gate has one — (a) walks the tree and (c)
//!    lists a directory, and each plants a file in a scratch tree and requires the
//!    enumeration to surface it. That is the half `meta_gates.rs` had to add after
//!    review 37 found its planted offender was always a string and never a file;
//!    (b) and (d) read named files, so a file that went missing fails at
//!    [`read_tree_file`] instead;
//! 3. *the comparison*, by deriving the drift report from the **real** document
//!    with one real entry deleted and one phantom entry inserted, so the failure
//!    message this gate would print is exercised on this tree's own bytes rather
//!    than on a fixture that resembles them.
//!
//! And each side asserts a floor. A gate that compares two sets it enumerated to
//! zero passes forever, which is the same silent-disarm shape as the clippy
//! `RefCell` ban that stopped working for a release (INV5-CLIPPY-SCOPE).
//!
//! **Why the fuzz manifest is parsed rather than `cargo fuzz list` shelled out
//! to.** `cargo fuzz list` reads exactly the `[[bin]]` table this gate reads, but
//! cargo-fuzz needs a nightly toolchain and libFuzzer and is installed only in the
//! scheduled `fuzz-nightly` job — so a gate that shelled out would self-skip on
//! every developer box and in every push lane, which is precisely the vacuous-green
//! shape the required-mode lattice exists to prevent. Reading the manifest is the
//! same enumeration with no toolchain and no skip.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The current normative plan, whose §3 carries the required-mode lattice.
///
/// Named as a path rather than discovered, because AGENTS §2 makes the pair's
/// filenames the thing a generation bump has to update in the same commit; a stale
/// name here fails loudly at [`read_tree_file`] instead of silently reading
/// nothing. `meta_gates.rs`'s entry-point gates already hold the *other* end of
/// that rename (README and AGENTS must name a pair that exists).
const PLAN: &str = "docs/44-implementation-plan-claude-fable-v17.md";

/// The current normative design, whose §13 carries the probe roster at a glance.
const DESIGN: &str = "docs/43-design-claude-fable-v17.md";

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/itest — the *directory*, which §15.40 kept short.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the itest crate has a parent directory")
        .to_path_buf()
}

/// Read a tracked file below `root`, naming it if it is gone.
///
/// Every source these gates read is required to exist: a rename that leaves one of
/// them unreadable must fail the gate rather than let it compare an empty set to an
/// empty set.
fn read_tree_file(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} — a gate cannot derive a roster it cannot read",
            path.display()
        )
    })
}

/// `src` with every `//`-introduced comment removed, line by line.
///
/// Same shape and same accepted limits as `meta_gates.rs`'s stripper: no `/* … */`
/// handling, and a `//` inside a string literal truncates its line. Both are
/// tolerable here for the same reason — prose *about* a token is not a use of it,
/// and the tree's comments discuss the tokens these matchers hunt for. **This file
/// is its own worst case**: the comments below spell out the required-mode idiom,
/// including a `var("SNX_X")` that names no real variable, and gate (a) walks this
/// file like any other. A stripper that stopped stripping would therefore red that
/// gate with `SNX_X` rather than degrade quietly.
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `len` bytes of `s` ending at `at`, snapped forward to a char boundary.
///
/// Returned with its absolute start offset so a caller can turn a hit inside the
/// window back into an offset in `s`. Byte-window slicing is how these matchers
/// bound "near enough to be the same expression"; snapping keeps a multi-byte
/// character (this tree's prose is full of `§`, `—` and `→`) from panicking a gate.
fn back_window(s: &str, at: usize, len: usize) -> (usize, &str) {
    let mut start = at.saturating_sub(len);
    while start < at && !s.is_char_boundary(start) {
        start += 1;
    }
    (start, &s[start..at])
}

/// The body of the block `header` introduces in `src`, delimited by **brace
/// counting**; `None` if the header is absent or the braces never balance.
///
/// Brace counting rather than "the first line-start `}`" for the reason review 37
/// filed against the older version of that trick (37-TEST-6): the first nested
/// block inside the region silently truncates the slice, and everything after it
/// becomes invisible to the gate. Call it on comment-stripped source so a brace
/// inside a comment cannot move the boundary.
fn braced_body<'a>(src: &'a str, header: &str) -> Option<&'a str> {
    let at = src.find(header)?;
    let rest = &src[at..];
    let open = rest.find('{')?;
    let mut depth = 0usize;
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every top-level parenthesised span in `text`, with **balanced** counting.
///
/// Depth counting is load-bearing rather than tidy: the enumeration this is used on
/// (`doctor/src/main.rs`'s module doc) names "P8 epoll vs `read(2)`" and "P9
/// `poll(2)` timeout granularity" *inside* the parenthesis, so a scan that stopped
/// at the first `)` would read the roster as ending at P8 and would then report a
/// drift that is an artefact of its own parser.
fn paren_spans(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(&text[start..i]);
                }
            }
            _ => {}
        }
    }
    out
}

/// The span between the first `open` at or after `at` and its matching `close`,
/// counted for nesting; `None` if either end is missing.
///
/// The generic form of [`braced_body`], used by gate (e) for a call's argument list
/// and for an array literal. Nesting matters in both: `range_error(name, "mode",
/// *mode as u64, 0o600, 0o777)` has no inner parenthesis today, but
/// `range_error(name, field, value, 0, MAX_TIMER_MS as u64)` is one edit away, and a
/// scan that stopped at the first `)` would read half an argument list and silently
/// under-report what the validator checks — a gate failing *open*.
fn balanced_span(s: &str, at: usize, open: char, close: char) -> Option<&str> {
    let rest = &s[at..];
    let start = rest.find(open)?;
    let mut depth = 0usize;
    for (i, c) in rest[start..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&rest[start + 1..start + i]);
            }
        }
    }
    None
}

/// Every double-quoted span in `text`, in order.
///
/// No escape handling: every literal these gates read is a field name or a JSON
/// key, none of which contains a quote. A literal that did would split into two
/// tokens, neither of which looks like a field name, so the failure direction is a
/// *missing* entry — the loud one — rather than a phantom match.
fn string_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        match after.find('"') {
            Some(close) => {
                out.push(after[..close].to_owned());
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

/// Every backticked span in `text`, in order.
fn backticked(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        match after.find('`') {
            Some(close) => {
                out.push(after[..close].to_owned());
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

/// The data rows of the Markdown table whose header line contains `header_needle`,
/// each row split into its cells (outer pipes dropped, cells trimmed).
///
/// The needle is a **locator**, not a roster: if a table is retitled the parse
/// returns nothing and the caller's non-vacuity floor reds, which is the failure
/// this whole file is about. Rows run from the delimiter line to the first line
/// that does not start with `|`.
fn table_rows(md: &str, header_needle: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = md.lines().collect();
    let Some(header) = lines.iter().position(|l| l.contains(header_needle)) else {
        return Vec::new();
    };
    // The `|---|` separator sits immediately below the header; rows follow it.
    let mut rows = Vec::new();
    for line in lines.iter().skip(header + 2) {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            break;
        }
        rows.push(
            trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_owned())
                .collect::<Vec<_>>(),
        );
    }
    rows
}

/// What one [`walk_rs`] pass actually did — the evidence a clean verdict needs.
///
/// `files` is the visited count a gate asserts a floor on; `unreadable` names the
/// directories `read_dir` refused for a reason other than "gone". A vanished
/// directory is a benign mid-walk race (a concurrent build removing a scratch
/// tree); a permission error is a walk that shrank in silence, and a gate that
/// visited half the tree must say so rather than report green.
#[derive(Default)]
struct WalkStats {
    files: usize,
    unreadable: Vec<String>,
}

/// Is `dir` the root of a **nested checkout** — another worktree or clone inside
/// this one? A worktree carries a `.git` file, a clone a `.git` directory.
///
/// Skipped for the reason `meta_gates.rs` records: a nested checkout is a second
/// copy of this tree, and every roster read out of it is a fact about the copy.
fn is_nested_checkout(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Visit every `.rs` file below `dir`, skipping build output, VCS, vendored trees
/// and nested checkouts.
///
/// `fuzz/` is skipped deliberately: it is excluded from the workspace and its
/// targets are enumerated by gate (c) from the manifest rather than by walking.
fn walk_rs(dir: &Path, visit: &mut impl FnMut(&Path, &str)) -> WalkStats {
    fn inner<F: FnMut(&Path, &str)>(dir: &Path, visit: &mut F, stats: &mut WalkStats) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    stats.unreadable.push(format!("{}: {e}", dir.display()));
                }
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if matches!(name.as_ref(), "target" | ".git" | "fuzz" | "node_modules")
                    || is_nested_checkout(&path)
                {
                    continue;
                }
                inner(&path, visit, stats);
            } else if name.ends_with(".rs")
                && let Ok(src) = std::fs::read_to_string(&path)
            {
                stats.files += 1;
                visit(&path, &src);
            }
        }
    }
    let mut stats = WalkStats::default();
    inner(dir, visit, &mut stats);
    stats
}

/// A scratch tree that removes itself on drop.
///
/// The planted-walk proofs below assert *after* writing, so a failing assertion
/// unwinds past any hand-rolled cleanup; the guard keeps a red gate from leaving
/// litter behind on every run. Named by pid and call site so parallel test binaries
/// never share one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("snx-meta-derive-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch tree");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Write `text` at `rel` below the scratch root, creating parents.
    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a planted file has a parent"))
            .expect("create scratch subdirectory");
        std::fs::write(&path, text).expect("write planted file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The one drift-report shape every gate here prints: what each side has that the
/// other does not, named item by item.
///
/// A gate that fails with "the two rosters differ" makes the reader do the diff by
/// hand, which is how a red gate becomes a deleted gate. Both directions are
/// reported because they are different defects: an entry only in the code is a
/// roster that lost something, an entry only in the list is a roster naming
/// something that no longer exists.
fn drift(
    left_name: &str,
    left: &BTreeSet<String>,
    right_name: &str,
    right: &BTreeSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for item in left.difference(right) {
        out.push(format!(
            "`{item}` is in {left_name} but not in {right_name}"
        ));
    }
    for item in right.difference(left) {
        out.push(format!(
            "`{item}` is in {right_name} but not in {left_name}"
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// (a) The `SNX_*=required` roster, enumerated from the comparisons
// ---------------------------------------------------------------------------

/// Every `SNX_*` variable `src` compares against the literal `required`.
///
/// The rule this enumerates is plan §3 rule 11's: `required` turns a legitimate
/// self-skip into a hard failure, and the *authority* for which variables have that
/// power is the code that reads them. So the matcher looks for the comparison
/// rather than for the name: each `required` literal is attributed to the nearest
/// preceding `var("SNX_…")` within one expression's reach.
///
/// Deliberately not matched, each a spelling that appears in this tree and would
/// make the roster wrong in a different direction:
///
/// * a variable named only in a **skip message** (`"SNX_TLS=required, but …"`) —
///   the literal there is `=required` inside a longer string, never the standalone
///   `"required"` this looks for, so a message can be reworded without moving the
///   roster;
/// * a variable named only in a **comment** — comments are stripped first, because
///   `itest/src/lib.rs` explains required-mode at length beside the code;
/// * a **parameter** variable (`SNX_REPLUG_DEV`, `SNX_CROSSOVER_A`) — plan §3 says
///   in as many words that those four are "parameters, not gates", and they are
///   read with `.ok()`/`.is_err()`, never measured against `required`. The
///   `SNX_CROSSOVER` read that *enables* the macOS `cu.usbserial` scan is
///   `.is_err()` too, and is correctly not a second entry for the same variable.
fn required_mode_vars_in(src: &str) -> BTreeSet<String> {
    // Assembled rather than written out, so this file's own source text carries no
    // string a walk over the tree could read as a required-mode comparison.
    let q = '"';
    let literal = format!("{q}required{q}");
    let opener = format!("var({q}SNX_");

    let code = strip_line_comments(src);
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while let Some(pos) = code[i..].find(&literal) {
        let at = i + pos;
        i = at + literal.len();
        // A comparison, not a value: `== Ok("required")`, `!= Ok("required")`, or a
        // bare `== "required"`. A `"required"` used as an argument or a map key is
        // not a claim about a variable.
        let (_, lead) = back_window(&code, at, 24);
        if !(lead.contains("Ok(") || lead.contains("==") || lead.contains("!=")) {
            continue;
        }
        // …attributed to the env read it is comparing. The window is one
        // expression wide: every instance in this tree reads
        // `std::env::var("SNX_X").as_deref() == Ok("required")`, roughly forty
        // bytes, and a window this size cannot reach past an intervening
        // statement to borrow an unrelated variable's name.
        let (start, back) = back_window(&code, at, 200);
        if let Some(hit) = back.rfind(&opener) {
            let name_at = start + hit + opener.len() - "SNX_".len();
            if let Some(end) = code[name_at..].find(q) {
                out.insert(code[name_at..name_at + end].to_owned());
            }
        }
    }
    out
}

/// Plan §3's required-mode lattice, split into the rows that claim `required`
/// power and the rows that explicitly disclaim it.
///
/// The table's own Variable column carries the distinction structurally: a
/// required-mode row spells a bare variable (`` `SNX_TLS` ``), while the one
/// parameter row spells a value with it (`` `SNX_SERIAL_PAIR=rig` ``) and its
/// Platform-notes cell says "not a required mode". Keying on the `=` keeps that
/// judgement in the table where the author made it, instead of in a list here.
fn required_mode_table(plan: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut required = BTreeSet::new();
    let mut parameters = BTreeSet::new();
    for row in table_rows(plan, "| Variable | Gates what |") {
        let Some(cell) = row.first() else { continue };
        let Some(token) = backticked(cell).into_iter().find(|t| t.starts_with("SNX_")) else {
            continue;
        };
        match token.split_once('=') {
            Some((name, _value)) => parameters.insert(name.to_owned()),
            None => required.insert(token),
        };
    }
    (required, parameters)
}

/// Plan §3's table minus the row for `victim` — the planted deletion the gate's
/// own failure path is proved against.
fn plan_without_row(plan: &str, victim: &str) -> String {
    plan.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with('|')
                && backticked(trimmed.trim_matches('|').split('|').next().unwrap_or(""))
                    .iter()
                    .any(|t| t == victim))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_required_mode_variable_the_harness_reads_appears_in_plan_3s_table() {
    let root = repo_root();
    let q = '"';
    // Assembled at runtime so this file's bytes contain no plantable violation of
    // the rule it enforces — the walk below reads this file like any other.
    let fixture = format!("SNX_{}_FIXTURE", "PLANTED");

    // 0. The matcher, in every spelling it claims to cover and every near miss it
    //    must ignore.
    let read =
        format!("let ready = std::env::var({q}{fixture}{q}).as_deref() == Ok({q}required{q});");
    assert!(
        required_mode_vars_in(&read).contains(&fixture),
        "the matcher does not see the one idiom every required-mode read in this \
         tree uses; the roster it derives is empty of everything"
    );
    let negated = format!(
        "assert!(std::env::var({q}{fixture}{q}).as_deref() != Ok({q}required{q}), {q}…{q});"
    );
    assert!(
        required_mode_vars_in(&negated).contains(&fixture),
        "the matcher sees only the `==` spelling — but the assert-form in \
         itest/src/lib.rs is `!=`, so every variable in the harness's own \
         required-mode helpers would go unenumerated"
    );
    let commented = format!(
        "// worth adding: std::env::var({q}{fixture}{q}).as_deref() == Ok({q}required{q})\nlet x = 1;"
    );
    assert!(
        required_mode_vars_in(&commented).is_empty(),
        "a required mode merely *proposed in a comment* is enumerated as if it \
         existed, so plan §3's table would be required to document a variable \
         nothing reads"
    );
    let message = format!("panic!({q}{fixture}=required, but the fixture is absent{q});");
    assert!(
        required_mode_vars_in(&message).is_empty(),
        "a variable named inside a skip *message* counts as a required mode — \
         rewording a message would then move the roster"
    );
    let parameter = format!("let dev = std::env::var({q}SNX_REPLUG_DEV{q}).ok();");
    assert!(
        required_mode_vars_in(&parameter).is_empty(),
        "a plain parameter read is enumerated as a required mode; plan §3's four \
         parameter variables would each be demanded a table row they must not have"
    );

    // 1. The walker: plant a file, require the walk to surface what it holds.
    let scratch = Scratch::new("required");
    scratch.write("deep/nested/harness.rs", &read);
    let mut planted = BTreeSet::new();
    let planted_stats = walk_rs(scratch.path(), &mut |_p, src| {
        planted.extend(required_mode_vars_in(src));
    });
    assert!(
        planted.contains(&fixture),
        "the walk did not reach a planted .rs file two directories down — a walk \
         that stops walking reports the same green as a clean tree"
    );
    assert_eq!(
        planted_stats.files, 1,
        "the planted walk visited an unexpected file count"
    );

    // 2. The real scan.
    let mut code_vars = BTreeSet::new();
    let stats = walk_rs(&root, &mut |_p, src| {
        code_vars.extend(required_mode_vars_in(src));
    });
    assert!(
        stats.unreadable.is_empty(),
        "directories went unread, so this roster is derived from part of the tree: {:?}",
        stats.unreadable
    );
    assert!(
        stats.files >= 100,
        "only {} .rs file(s) visited — the walk shrank, and a roster derived from a \
         shrunken walk agrees with any table at all",
        stats.files
    );
    assert!(
        code_vars.len() >= 5,
        "the harness reads {} required-mode variable(s); §3's rig lane alone names \
         five, so the matcher has stopped matching",
        code_vars.len()
    );

    // 3. The table.
    let plan = read_tree_file(&root, PLAN);
    let (table_required, table_parameters) = required_mode_table(&plan);
    assert!(
        table_required.len() >= 5,
        "plan §3's required-mode table parsed to {} row(s) — it was retitled or \
         reshaped, and this gate is now comparing the code against nothing",
        table_required.len()
    );
    assert!(
        !table_parameters.is_empty(),
        "plan §3's table no longer carries the parameter row that proves the two \
         kinds are distinguishable; the `=value` spelling is what separates them"
    );

    // 4. …planted against the real document, both directions, before the clean
    //    verdict is trusted. The victim is taken from the table itself rather than
    //    named here, so this proof cannot go stale when the lattice grows.
    let victim = table_required
        .iter()
        .next()
        .expect("the table has at least one required-mode row")
        .clone();
    let (stale_required, _) = required_mode_table(&plan_without_row(&plan, &victim));
    let missing = drift(
        "the harness code",
        &code_vars,
        "plan §3's table",
        &stale_required,
    );
    assert!(
        missing.iter().any(|m| m.contains(&victim)),
        "deleting the `{victim}` row from plan §3's table produced no drift naming \
         it: this gate cannot notice a required mode that loses its documentation. \
         Reported instead: {missing:?}"
    );
    // Inserted ahead of a row the table itself supplied, so this proof follows the
    // lattice rather than pinning a variable name that may be renamed.
    let anchor = format!("| `{victim}` |");
    let phantom_row = format!("| `{fixture}` | nothing at all | — | — | — |");
    let phantom_plan = plan.replace(&anchor, &format!("{phantom_row}\n{anchor}"));
    assert_ne!(
        phantom_plan, plan,
        "the phantom row was never inserted, so the proof below asserts nothing"
    );
    let (phantom_required, _) = required_mode_table(&phantom_plan);
    let extra = drift(
        "the harness code",
        &code_vars,
        "plan §3's table",
        &phantom_required,
    );
    assert!(
        extra.iter().any(|m| m.contains(&fixture)),
        "a table row for a variable nothing reads produced no drift: plan §3 could \
         document a required mode the harness has deleted. Reported instead: {extra:?}"
    );

    // 5. The verdict.
    let d = drift(
        "the harness code",
        &code_vars,
        "plan §3's table",
        &table_required,
    );
    assert!(
        d.is_empty(),
        "plan §3's required-mode lattice has drifted from the harness that reads \
         it. The table is the reader's map and the code is the authority (plan §3 \
         rule 11), so the fix is the table unless the variable itself is wrong:\n  \
         {}\nCode side: {:?}\nTable side: {:?}",
        d.join("\n  "),
        code_vars,
        table_required
    );

    // 6. And the table's own disclaimer holds: a row that spells a *value* says
    //    that variable is not a required mode, which is a claim about the code.
    for name in &table_parameters {
        assert!(
            !code_vars.contains(name),
            "plan §3's table lists `{name}` as a parameter rather than a required \
             mode, but the harness compares it against `required` — one of the two \
             is wrong, and an operator reading the table would not set it"
        );
    }
}

// ---------------------------------------------------------------------------
// (b) The probe rosters against the registry in `doctor/src/probes.rs`
// ---------------------------------------------------------------------------

/// Every `P<n>` token in `text`, bounded like an identifier.
///
/// Bounded on both sides so `P12` is one id and never two, and so a word ending in
/// `P` followed by a digit does not manufacture one.
fn probe_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let is_word = |c: u8| c == b'_' || c.is_ascii_alphanumeric();
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while let Some(pos) = text[i..].find('P') {
        let start = i + pos;
        i = start + 1;
        if start > 0 && is_word(bytes[start - 1]) {
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == start + 1 {
            continue;
        }
        if end < bytes.len() && is_word(bytes[end]) {
            continue;
        }
        out.insert(text[start..end].to_owned());
    }
    out
}

/// The probe registry: the id every `Probe::new` in `probes.rs` is constructed
/// with.
///
/// This is the authority §16.13 leans on — a kernel claim in prose cites a probe id
/// and a committed artifact, and the id has to mean the same thing in both. The
/// first string literal after each constructor is the id (the constructor's own
/// signature puts it first), and comments are stripped so a probe discussed in
/// prose is not counted as a probe that exists.
fn probe_registry(probes_rs: &str) -> BTreeSet<String> {
    let code = strip_line_comments(probes_rs);
    let mut out = BTreeSet::new();
    let mut i = 0usize;
    while let Some(pos) = code[i..].find("Probe::new(") {
        let at = i + pos + "Probe::new(".len();
        i = at;
        let Some(open) = code[at..].find('"') else {
            break;
        };
        let value_at = at + open + 1;
        let Some(close) = code[value_at..].find('"') else {
            break;
        };
        let id = &code[value_at..value_at + close];
        if probe_ids(id).contains(id) {
            out.insert(id.to_owned());
        }
    }
    out
}

/// The ids in the first cell of every row of the Markdown table `header_needle`
/// locates.
fn probe_roster_table(md: &str, header_needle: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for row in table_rows(md, header_needle) {
        if let Some(cell) = row.first() {
            out.extend(probe_ids(cell));
        }
    }
    out
}

/// The probe enumeration inside `doctor/src/main.rs`'s module doc, if it still
/// carries one.
///
/// `None` is a legitimate answer, and plan §18 item 40 says so outright: the second
/// enumeration may be *matched* against the registry "or the enumeration deleted
/// from `doctor/src/main.rs`". So the gate accepts deletion and refuses staleness,
/// which are the only two states in which the file tells the truth.
///
/// The enumeration is found by shape rather than by position: a parenthesised span
/// inside the `//!` block naming three or more probes is a roster; the module doc's
/// other parentheses name none.
fn main_rs_probe_enumeration(main_rs: &str) -> Option<BTreeSet<String>> {
    let module_doc: String = main_rs
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found: Option<BTreeSet<String>> = None;
    for span in paren_spans(&module_doc) {
        let ids = probe_ids(span);
        if ids.len() >= 3 {
            let acc = found.get_or_insert_with(BTreeSet::new);
            acc.extend(ids);
        }
    }
    found
}

#[test]
fn every_documented_probe_roster_matches_the_doctor_registry() {
    let root = repo_root();
    let q = '"';

    // 0. The matcher, both sides.
    let planted_probe =
        format!("let p = Probe::new(\n    {q}P99{q},\n    {q}a planted probe{q},\n);");
    assert!(
        probe_registry(&planted_probe).contains("P99"),
        "the registry matcher misses a constructor whose id sits on the next line \
         — which is how thirteen of the fifteen in probes.rs are written"
    );
    let commented_probe = format!("// one day: Probe::new({q}P99{q}, {q}…{q})");
    assert!(
        probe_registry(&commented_probe).is_empty(),
        "a probe merely proposed in a comment enters the registry, so every doc \
         roster would be required to document a probe that does not exist"
    );
    assert_eq!(
        probe_ids("P8 epoll vs read(2), P12 the edge"),
        BTreeSet::from(["P8".to_owned(), "P12".to_owned()]),
        "the id scanner does not read a two-digit id as one token"
    );
    assert!(
        probe_ids("PORT4 and TCPKT9").is_empty(),
        "the id scanner manufactures probe ids out of ordinary words"
    );
    assert_eq!(
        main_rs_probe_enumeration("//! runs (P1 a, P2 b via read(2), P3 c) plus checks\n"),
        Some(BTreeSet::from([
            "P1".to_owned(),
            "P2".to_owned(),
            "P3".to_owned()
        ])),
        "the prose-enumeration reader stops at the first `)` — the real \
         enumeration contains `read(2)` and `poll(2)`, so it would be read as \
         ending three probes early and would report drift that is its own"
    );
    assert_eq!(
        main_rs_probe_enumeration("//! runs every probe (see the roster) and reports\n"),
        None,
        "a module doc with no enumeration is read as carrying an empty one, which \
         would make item 40's sanctioned deletion fail the gate it satisfies"
    );

    // 1. The registry, and its own non-vacuity.
    let registry = probe_registry(&read_tree_file(&root, "doctor/src/probes.rs"));
    assert!(
        registry.len() >= 12,
        "the probe registry enumerated to {} — doctor/src/probes.rs was reshaped \
         and every comparison below is now against nothing",
        registry.len()
    );
    let highest = registry
        .iter()
        .filter_map(|id| id[1..].parse::<u32>().ok())
        .max()
        .expect("every registry id is P followed by a number");
    let contiguous: BTreeSet<String> = (1..=highest).map(|n| format!("P{n}")).collect();
    assert_eq!(
        registry, contiguous,
        "the registry is not P1..P{highest} without gaps. §13 makes a probe id a \
         permanent diff key across kernels — a retired probe keeps its number and a \
         new question takes a new one — so a hole here is either a lost probe or a \
         renumbering that invalidates every committed artifact"
    );

    // 2. Each documented roster, planted against the real document first.
    let sources: [(&str, &str); 2] = [
        ("docs/serial-nexus-doctor.md", "| ID | What it checks |"),
        (DESIGN, "| Probe | Question |"),
    ];
    for (rel, header) in sources {
        let md = read_tree_file(&root, rel);
        let rostered = probe_roster_table(&md, header);
        assert!(
            rostered.len() >= 12,
            "{rel}'s probe roster parsed to {} row(s) — the table was retitled or \
             reshaped and this gate is comparing the registry against nothing",
            rostered.len()
        );

        // The planted deletion: strike the highest-numbered row and require the
        // drift report to name it. Taken from the registry rather than typed, so
        // the proof follows the roster as it grows.
        let victim = format!("P{highest}");
        let stale: String = md
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !(trimmed.starts_with('|')
                    && probe_ids(trimmed.trim_matches('|').split('|').next().unwrap_or(""))
                        .contains(&victim))
            })
            .collect::<Vec<_>>()
            .join("\n");
        let planted = drift(
            "doctor/src/probes.rs",
            &registry,
            &format!("{rel}'s roster"),
            &probe_roster_table(&stale, header),
        );
        assert!(
            planted.iter().any(|m| m.contains(&victim)),
            "deleting the {victim} row from {rel} produced no drift naming it — \
             this gate cannot notice a probe that loses its documentation. \
             Reported instead: {planted:?}"
        );

        let d = drift(
            "doctor/src/probes.rs",
            &registry,
            &format!("{rel}'s roster"),
            &rostered,
        );
        assert!(
            d.is_empty(),
            "{rel}'s probe roster has drifted from the registry in \
             doctor/src/probes.rs:\n  {}\nThe registry is the authority (§15.17, \
             §16.13: a kernel claim cites a probe id, so an id with no row is a \
             citation a reader cannot follow). Add the row, or retire the probe.",
            d.join("\n  ")
        );
    }

    // 3. §13 rule 1's prose **count** of the roster: "The probe roster is P1-P15".
    //     A range in a sentence is a hand-kept list wearing two numbers, and it is the
    //     enumeration most likely to survive a P16 unnoticed — a new row lands in the
    //     tables because the author is looking at them, and this sentence is a page
    //     away. Checked against the same `highest` the registry produced, so the gate
    //     has no third opinion about what the roster is.
    let design = read_tree_file(&root, DESIGN);
    let prose_range = format!("The probe roster is P1–P{highest}");
    assert!(
        design.contains(&prose_range),
        "{DESIGN}'s §13 rule 1 does not say {prose_range:?}. The registry in \
         doctor/src/probes.rs holds P1..P{highest}, and that sentence is the roster's \
         third documented form — the one a reader meets before either table. If a \
         probe was added, the sentence moves with it; if the wording changed, this \
         gate has to learn the new spelling rather than be deleted, or nothing checks \
         it again (plan §18 item 40; notes §3.75)"
    );

    // 4. `doctor/src/main.rs`'s module-doc enumeration: deleted, or exact.
    let main_rs = read_tree_file(&root, "doctor/src/main.rs");
    if let Some(enumerated) = main_rs_probe_enumeration(&main_rs) {
        let d = drift(
            "doctor/src/probes.rs",
            &registry,
            "doctor/src/main.rs's module-doc enumeration",
            &enumerated,
        );
        assert!(
            d.is_empty(),
            "doctor/src/main.rs's module doc enumerates the probes it runs, and the \
             list has drifted from the registry beside it:\n  {}\nPlan §18 item 40 \
             sanctions either repair: match the enumeration to the registry, or \
             **delete it** — `docs/serial-nexus-doctor.md` is the probe registry of \
             record (design §13) and a second copy in a module doc buys nothing it \
             can keep true.",
            d.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// (c) `fuzz/Cargo.toml`'s `[[bin]]` table against `fuzz/fuzz_targets/`
// ---------------------------------------------------------------------------

/// The `[[bin]]` entries of a cargo manifest, as `name -> path`.
///
/// Hand-parsed, deliberately and narrowly: this reads the exact subset of TOML the
/// fuzz manifest is written in — an array-of-tables header followed by
/// `key = "value"` lines. The narrowness is safe in the direction that matters,
/// because the caller asserts a **bijection**: a `[[bin]]` spelled in some form
/// this misses leaves its file unregistered, which is a loud failure, not a silent
/// pass. A parser that guessed at inline tables could fail the other way.
fn fuzz_bin_table(manifest: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut in_bin = false;
    let mut name: Option<String> = None;
    let mut path: Option<String> = None;
    // A `[[bin]]` block ends at the next table header or at end of file; both
    // endings have to commit the entry, or the last registration in the manifest
    // would be enumerated by nobody.
    fn flush(
        name: &mut Option<String>,
        path: &mut Option<String>,
        out: &mut BTreeMap<String, String>,
    ) {
        if let (Some(n), Some(p)) = (name.take(), path.take()) {
            out.insert(n, p);
        }
    }
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            flush(&mut name, &mut path, &mut out);
            in_bin = trimmed == "[[bin]]";
            continue;
        }
        if !in_bin {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        match key.trim() {
            "name" => name = Some(value),
            "path" => path = Some(value),
            _ => {}
        }
    }
    flush(&mut name, &mut path, &mut out);
    out
}

#[test]
fn every_fuzz_target_file_is_registered_and_every_registration_has_a_file() {
    let root = repo_root();
    let fuzz = root.join("fuzz");

    // 0. The matcher: a registration this parser cannot see is the one failure it
    //    could hide, so prove it sees the shape the manifest is written in — and
    //    that a `[[bin]]`-shaped block under another table header is not read as a
    //    registration.
    let planted = "[[bin]]\nname = \"planted\"\npath = \"fuzz_targets/planted.rs\"\ntest = false\n";
    assert_eq!(
        fuzz_bin_table(planted).get("planted").map(String::as_str),
        Some("fuzz_targets/planted.rs"),
        "the manifest parser cannot read the registration shape every target in \
         fuzz/Cargo.toml uses"
    );
    assert!(
        fuzz_bin_table("[dependencies]\nname = \"planted\"\npath = \"x.rs\"\n").is_empty(),
        "the manifest parser reads keys outside `[[bin]]` as a registration, so a \
         dependency table could satisfy the bijection below"
    );
    assert_eq!(
        fuzz_bin_table(&format!(
            "{planted}\n[[bin]]\nname = \"second\"\npath = \"fuzz_targets/second.rs\"\n"
        ))
        .len(),
        2,
        "the manifest parser collapses consecutive registrations into one"
    );

    // 1. Both sides, from their real sources.
    let manifest = read_tree_file(&root, "fuzz/Cargo.toml");
    let registered = fuzz_bin_table(&manifest);
    let targets_dir = fuzz.join("fuzz_targets");
    let mut files: BTreeSet<String> = BTreeSet::new();
    let entries = std::fs::read_dir(&targets_dir).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} — the fuzz corpus this gate is about is unreadable",
            targets_dir.display()
        )
    });
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".rs") {
            files.insert(name);
        }
    }
    assert!(
        registered.len() >= 5,
        "fuzz/Cargo.toml registers {} target(s); review 26's SEC-7 alone added five \
         on top of the original four, so the manifest side has stopped enumerating",
        registered.len()
    );
    assert!(
        files.len() >= 5,
        "only {} source(s) under fuzz/fuzz_targets/ — the listing side has stopped \
         enumerating",
        files.len()
    );

    // 2. The bijection, named in both directions.
    let registered_files: BTreeSet<String> = registered
        .values()
        .filter_map(|p| p.rsplit('/').next().map(str::to_owned))
        .collect();
    let d = drift(
        "fuzz/fuzz_targets/",
        &files,
        "fuzz/Cargo.toml's [[bin]] table",
        &registered_files,
    );
    assert!(
        d.is_empty(),
        "the fuzz corpus and its registrations have drifted:\n  {}\nThis is the \
         hinge under two claims made elsewhere. CI's nightly loop enumerates \
         `cargo fuzz list`, which prints `[[bin]]` **names** while its comment \
         promises that \"a target added under fuzz/fuzz_targets/ is now fuzzed the \
         night it lands\"; and meta_gates.rs's \
         `every_unstable_fuzz_api_export_has_a_fuzz_target` satisfies §15.26's \
         re-export bargain from the **directory listing**. An unregistered file \
         makes the first false and the second vacuous at once: it answers \"a \
         target drives this re-export\" while never being built and never being \
         run.",
        d.join("\n  ")
    );

    // 3. Each registration names a file that exists, is named after it, and is a
    //    fuzz target rather than an ordinary source.
    for (name, path) in &registered {
        let full = fuzz.join(path);
        assert!(
            full.is_file(),
            "fuzz/Cargo.toml registers `{name}` at `{path}`, which does not exist — \
             `cargo fuzz build` fails on this manifest, so the whole nightly loop \
             is red for one missing file"
        );
        let stem = path
            .rsplit('/')
            .next()
            .and_then(|f| f.strip_suffix(".rs"))
            .unwrap_or_default();
        assert_eq!(
            name, stem,
            "fuzz target `{name}` is registered at `{path}`: `cargo fuzz run` takes \
             the **name**, so a name that is not its file's stem makes the CI loop's \
             `for t in $targets` name something no reader can find"
        );
        let src = std::fs::read_to_string(&full)
            .unwrap_or_else(|e| panic!("read {}: {e}", full.display()));
        assert!(
            src.contains("fuzz_target!"),
            "`{name}` is registered as a fuzz target but its source drives no \
             `fuzz_target!` — it would build, be listed, be \"fuzzed\" for its sixty \
             seconds, and exercise nothing"
        );
    }

    // 4. The planted violation, run through the same comparison the verdict uses:
    //    a file nobody registered must surface by name.
    let scratch = Scratch::new("fuzz");
    scratch.write(
        "fuzz_targets/unregistered.rs",
        "fuzz_target!(|d: &[u8]| { let _ = d; });\n",
    );
    let mut planted_files: BTreeSet<String> = files.clone();
    for entry in std::fs::read_dir(scratch.path().join("fuzz_targets"))
        .expect("read the planted corpus")
        .flatten()
    {
        planted_files.insert(entry.file_name().to_string_lossy().into_owned());
    }
    let planted_drift = drift(
        "fuzz/fuzz_targets/",
        &planted_files,
        "fuzz/Cargo.toml's [[bin]] table",
        &registered_files,
    );
    assert!(
        planted_drift.iter().any(|m| m.contains("unregistered.rs")),
        "a planted, unregistered target file produced no drift: the exact defect \
         this gate exists for — a source that is fuzzed never — passes it. \
         Reported instead: {planted_drift:?}"
    );
}

// ---------------------------------------------------------------------------
// (d) The documented RPC verb index against the daemon and `ctl`
// ---------------------------------------------------------------------------

/// Does `token` have the shape of an RPC method name?
///
/// Lowercase, digits, `-` and `.` only, and short. Used to keep the source-side
/// scanners from mistaking an ordinary string literal — a format string, an error
/// message, a JSON key with spaces in it — for a verb.
fn looks_like_a_verb(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 24
        && token.starts_with(|c: char| c.is_ascii_lowercase())
        && token
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// The method index of `docs/rpc/README.md`: which page documents which verbs.
///
/// The Methods cell of each row is truncated at its first `(`, because
/// `observation.md`'s cell continues "(+ the `state` / `lock` / `tap.data` /
/// `tap.closed` notifications and `LockSnapshot`)" — notifications are daemon →
/// client messages and `LockSnapshot` is a payload type, neither of which is a verb
/// a client may call.
fn documented_verb_index(readme: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for row in table_rows(readme, "| Page | Methods |") {
        let (Some(page_cell), Some(methods_cell)) = (row.first(), row.get(1)) else {
            continue;
        };
        // `[configuration.md](configuration.md)` — the link target is the file.
        let page = page_cell
            .rsplit_once('(')
            .map(|(_, t)| t.trim_end_matches(')').to_owned())
            .unwrap_or_else(|| page_cell.clone());
        let methods: BTreeSet<String> = backticked(match methods_cell.split_once('(') {
            Some((before, _)) => before,
            None => methods_cell,
        })
        .into_iter()
        .filter(|t| looks_like_a_verb(t))
        .collect();
        if !methods.is_empty() {
            out.insert(page, methods);
        }
    }
    out
}

/// Every method the daemon answers: the `match method` arms of `Daemon::dispatch`,
/// plus the two verbs `control.rs` handles on the connection task itself.
///
/// Both halves are needed and neither is optional: `tap.open`/`tap.close` are
/// connection-scoped (§17) and deliberately never reach `dispatch`, so a gate that
/// read only the dispatcher would report them as documented-but-unimplemented and
/// be deleted for crying wolf.
fn implemented_verbs(daemon_rs: &str, control_rs: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let code = strip_line_comments(daemon_rs);
    if let Some(dispatch) = braced_body(&code, "pub async fn dispatch")
        && let Some(arms) = braced_body(dispatch, "match method")
    {
        let mut rest = arms;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let token = &after[..close];
            let tail = after[close + 1..].trim_start();
            if tail.starts_with("=>") && looks_like_a_verb(token) {
                out.insert(token.to_owned());
            }
            rest = &after[close + 1..];
        }
    }
    let control = strip_line_comments(control_rs);
    let mut rest = control.as_str();
    while let Some(at) = rest.find("method == \"") {
        let after = &rest[at + "method == \"".len()..];
        let Some(close) = after.find('"') else { break };
        let token = &after[..close];
        if looks_like_a_verb(token) {
            out.insert(token.to_owned());
        }
        rest = &after[close + 1..];
    }
    out
}

/// Every method `serial-nexus-ctl` puts on the wire: the verb half of each
/// `build_request` arm, plus the two requests the streaming paths write directly.
///
/// `subscribe` and `tap.open` never pass through `build_request` — they are
/// answered by a stream rather than a single response, so `ctl` writes them itself
/// — and a scanner that read only the dispatch table would miss exactly the two
/// verbs whose transport is unusual.
fn ctl_verbs(ctl_main_rs: &str) -> BTreeSet<String> {
    let code = strip_line_comments(ctl_main_rs);
    let mut out = BTreeSet::new();
    if let Some(body) = braced_body(&code, "fn build_request") {
        let mut rest = body;
        while let Some(open) = rest.find('"') {
            let before = rest[..open].trim_end();
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let token = &after[..close];
            let tail = after[close + 1..].trim_start();
            // The verb of an arm is the first element of a `("verb", params)`
            // tuple: opened by `(` and followed by `,`. A JSON key sits behind a
            // `{`, and a `panic!`-style literal is followed by `)`.
            if before.ends_with('(') && tail.starts_with(',') && looks_like_a_verb(token) {
                out.insert(token.to_owned());
            }
            rest = &after[close + 1..];
        }
    }
    let mut rest = code.as_str();
    while let Some(at) = rest.find("Request::new(") {
        let after = &rest[at + "Request::new(".len()..];
        let Some(open) = after.find('"') else { break };
        let value = &after[open + 1..];
        let Some(close) = value.find('"') else { break };
        let token = &value[..close];
        if looks_like_a_verb(token) {
            out.insert(token.to_owned());
        }
        rest = &value[close + 1..];
    }
    out
}

#[test]
fn the_documented_rpc_verb_table_matches_the_daemon_and_ctl() {
    let root = repo_root();

    // 0. The matchers, in the spellings each side is really written in.
    assert_eq!(
        documented_verb_index(
            "| Page | Methods |\n|---|---|\n| [p.md](p.md) | `a`, `b` (+ the `c` notification) |\n"
        )
        .get("p.md"),
        Some(&BTreeSet::from(["a".to_owned(), "b".to_owned()])),
        "the index reader folds the parenthetical notification list into the verb \
         set — `tap.data` and `tap.closed` are pushed by the daemon and can never \
         be dispatched, so the documented set would name two methods no `match \
         method` will ever carry"
    );
    let planted_dispatch = "pub async fn dispatch(&self, m: &str) -> R {\n let r = match method {\n \"alpha\" => self.alpha(),\n \"beta\" => Ok(json!({ \"key\": 1 })),\n other => Err(nf(other)),\n };\n r\n}";
    assert_eq!(
        implemented_verbs(planted_dispatch, ""),
        BTreeSet::from(["alpha".to_owned(), "beta".to_owned()]),
        "the dispatch reader either misses a match arm or counts a JSON key as a \
         verb; both make the comparison below meaningless"
    );
    assert_eq!(
        implemented_verbs(
            "",
            "if method == \"tap.open\" { } else if method == \"tap.close\" { }"
        ),
        BTreeSet::from(["tap.open".to_owned(), "tap.close".to_owned()]),
        "the connection-scoped reader misses §17's taps, which never reach the \
         dispatcher — the gate would report both as documented-but-unimplemented"
    );
    let planted_ctl = "fn build_request(cmd: &Cmd) -> R {\n Ok(match cmd {\n Cmd::Dump => (\"dump\", None),\n Cmd::Rotate { node } => (\"rotate\", Some(json!({ \"node\": node }))),\n Cmd::Tap { .. } => unreachable!(\"tap is handled before dispatch\"),\n })\n}\nlet _ = Request::new(1, \"tap.open\", Some(p));";
    assert_eq!(
        ctl_verbs(planted_ctl),
        BTreeSet::from([
            "dump".to_owned(),
            "rotate".to_owned(),
            "tap.open".to_owned()
        ]),
        "the ctl reader mis-enumerates: it must take the verb of each `(\"verb\", \
         params)` arm and the directly-written streaming requests, while ignoring \
         JSON keys and the `unreachable!` text that names a subcommand"
    );

    // 1. Both sides, from their real sources.
    let readme = read_tree_file(&root, "docs/rpc/README.md");
    let index = documented_verb_index(&readme);
    let documented: BTreeSet<String> = index.values().flatten().cloned().collect();
    let implemented = implemented_verbs(
        &read_tree_file(&root, "daemon/src/daemon.rs"),
        &read_tree_file(&root, "daemon/src/control.rs"),
    );
    let from_ctl = ctl_verbs(&read_tree_file(&root, "ctl/src/main.rs"));

    assert!(
        index.len() >= 5,
        "docs/rpc/README.md's page index parsed to {} row(s) — the table moved and \
         this gate is comparing the daemon against nothing",
        index.len()
    );
    assert!(
        documented.len() >= 15,
        "only {} documented method(s) — §10's surface is larger than that, so the \
         index reader has stopped reading",
        documented.len()
    );
    assert!(
        implemented.len() >= 15,
        "only {} implemented method(s) — the dispatch reader has stopped reading",
        implemented.len()
    );
    assert!(
        from_ctl.len() >= 15,
        "only {} method(s) enumerated from ctl — the ctl reader has stopped reading",
        from_ctl.len()
    );

    // 2. Planted against the real document before the clean verdict is trusted.
    let victim = documented
        .iter()
        .next()
        .expect("the index documents at least one method")
        .clone();
    let stale = readme.replacen(&format!("`{victim}`, "), "", 1);
    let planted = drift(
        "the daemon",
        &implemented,
        "docs/rpc/README.md's page index",
        &documented_verb_index(&stale)
            .values()
            .flatten()
            .cloned()
            .collect(),
    );
    assert!(
        planted.iter().any(|m| m.contains(&victim)),
        "removing `{victim}` from docs/rpc/README.md's index produced no drift \
         naming it — a verb could drop out of the documented contract silently. \
         Reported instead: {planted:?}"
    );

    // 3. The verdict: the documented surface is exactly the implemented one.
    let d = drift(
        "the daemon (dispatch + the connection-scoped tap verbs)",
        &implemented,
        "docs/rpc/README.md's page index",
        &documented,
    );
    assert!(
        d.is_empty(),
        "the documented RPC method index has drifted from the daemon:\n  {}\n\
         docs/rpc/ is the schema authority and says so in the terms this checks — \
         \"Only the methods documented on the pages above are live\" — and §15.16 \
         makes an undocumented method a `-32601` a client discovers at runtime.",
        d.join("\n  ")
    );

    // 4. Each documented method is documented on the page the index sends the
    //    reader to. An index entry pointing at a page that never mentions the verb
    //    is the same broken promise as a missing row, one click later.
    for (page, methods) in &index {
        let text = read_tree_file(&root, &format!("docs/rpc/{page}"));
        for method in methods {
            let heading = format!("`{method}`");
            assert!(
                text.lines()
                    .any(|l| l.starts_with('#') && l.trim_end().ends_with(&heading)),
                "docs/rpc/README.md sends a reader to {page} for `{method}`, but \
                 that page carries no heading for it"
            );
        }
    }

    // 5. `ctl` may be narrower than the contract — it is "a thin presentation layer
    //    over these methods" (docs/rpc/README.md), and its `tap` subcommand ends a
    //    tap "until the tap or the connection ends" (docs/rpc/observation.md) rather
    //    than by calling `tap.close`, so it sends one verb fewer — but it may never
    //    be *wider*: a verb it sends that the daemon does not answer is a `-32601`
    //    an operator finds at the prompt, and one the docs do not carry is a
    //    surface with no schema.
    for verb in &from_ctl {
        assert!(
            implemented.contains(verb),
            "serial-nexus-ctl sends `{verb}`, which the daemon does not dispatch: \
             every invocation of it answers -32601 (§15.16's version-skew signal) \
             against the daemon it ships beside"
        );
        assert!(
            documented.contains(verb),
            "serial-nexus-ctl sends `{verb}`, which docs/rpc/README.md's index does \
             not document — the CLI is a presentation layer over the documented \
             methods, so a verb it can send has a schema page or it has no contract"
        );
    }
}

// ---------------------------------------------------------------------------
// (e) §16.12's numeric maxima, over the schema rather than over a list
// ---------------------------------------------------------------------------

/// The configuration schemas whose numeric fields §16.12 governs, as
/// `(file, declaration header)`.
///
/// Two, and the second is not an afterthought: a codec's `attributes` table is
/// opaque to `GraphConfig::validate` by design (§8), so the exec codec's own
/// numerics are checked in `parse_attributes` instead. §16.12 says *every* numeric
/// attribute, not every numeric attribute in one file, and a gate that read only
/// the node schema would report green while the other door stood open — which is
/// the shape of the defect this gate exists for, one file over.
const NUMERIC_SCHEMAS: [(&str, &str); 2] = [
    ("core/src/config.rs", "pub enum NodeConfig"),
    ("daemon/src/nodes/exec.rs", "struct ExecAttributes"),
];

/// Is `ty` a numeric attribute's type?
///
/// `Option<u32>` counts. An omitted value is not a number and cannot be out of
/// range, but a *present* one is, and `Pty { mode: Option<u32> }` is exactly that
/// shape — the field review 32 found unchecked and fixed. Unwrapping the `Option`
/// here is what keeps the next one from hiding behind it.
fn is_numeric_type(ty: &str) -> bool {
    let mut t = ty.trim();
    if let Some(inner) = t.strip_prefix("Option<") {
        t = inner.trim().trim_end_matches('>').trim();
    }
    matches!(
        t,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

/// The `name: Type` fields declared anywhere inside `body`, as `name -> Type`.
///
/// Line-oriented, because both schemas are written one field per line, and
/// deliberately blind to which variant a field belongs to: §16.12's unit is the
/// attribute, and `hostward_buffer` is one rule whether it is declared on `Serial`
/// or on `Pty`. Attribute lines (`#[serde(…)]`) are skipped and comments are
/// stripped, so a `default = "default_baud"` cannot be read as a field and a field
/// discussed in prose cannot be read as declared.
fn declared_fields(body: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in strip_line_comments(body).lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            continue;
        }
        let ty = rest.trim().trim_end_matches(',').trim();
        if !ty.is_empty() {
            out.insert(name.to_owned(), ty.to_owned());
        }
    }
    out
}

/// Every field name `validate_body` range-checks, in both spellings the validator
/// uses.
///
/// 1. **The literal call** — `range_error(name, "baud", …)`, six of the seven sites.
/// 2. **The loop table** — `for (field, value) in [("reconnect_initial_ms", …), …]
///    { … range_error(name, field, value, …) }`, where the field names are in an
///    array literal and the call itself names none. A matcher that read only
///    spelling 1 would report the leg's three timers as unchecked and would be
///    deleted for crying wolf, which is why the loop is read as well — and read
///    *conditionally*, on its body actually reaching `range_error`, so an unrelated
///    table of string pairs cannot vouch for a field nothing checks.
fn range_checked_fields(validate_body: &str) -> BTreeSet<String> {
    range_windows(validate_body).into_keys().collect()
}

/// `args` — the inside of a call's parentheses — split at **top-level** commas.
///
/// Nesting is counted on `(` and `[` only. Angle brackets are deliberately not
/// counted: no argument any of these calls takes is generic, and `>=` inside one
/// would unbalance a counter that tried.
fn top_level_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in args.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur).trim().to_owned()),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_owned());
    }
    out
}

/// Every field `validate_body` range-checks, mapped to the **window** it is checked
/// against as `(low, high)` source expressions.
///
/// The two spellings [`range_checked_fields`] documents, now read for their bounds as
/// well as their names: `range_error(node, field, value, low, high)` is five
/// arguments, the field being the second when it is a literal and coming from the
/// enclosing `for` table when it is not.
///
/// Reading the window is what turns "a bound is *written*" into "a bound
/// *constrains*". Until 2026-08-15 this gate read only the name, so replacing
/// `baud`'s `1, u32::MAX as u64` with `0, u64::MAX` left it green — a field with a
/// range check that refuses nothing passed a gate whose whole subject is §16.12's
/// "stated, structurally checked maximum". Only a hand-written unit test noticed,
/// which is the layer plan §18 item 62 filed as the unreliable one.
fn range_windows(validate_body: &str) -> BTreeMap<String, (String, String)> {
    let code = strip_line_comments(validate_body);
    let mut out = BTreeMap::new();
    let mut record = |args: &str, names: Option<&[String]>| {
        let parts = top_level_args(args);
        if parts.len() != 5 {
            return;
        }
        let (low, high) = (parts[3].clone(), parts[4].clone());
        let fields: Vec<String> = match names {
            Some(n) => n.to_vec(),
            None => string_literals(&parts[1]),
        };
        for f in fields {
            out.insert(f, (low.clone(), high.clone()));
        }
    };
    let opener = "range_error(";
    let mut i = 0usize;
    while let Some(pos) = code[i..].find(opener) {
        let at = i + pos + opener.len() - 1;
        i = at + 1;
        if let Some(args) = balanced_span(&code, at, '(', ')') {
            record(args, None);
        }
    }
    let mut i = 0usize;
    while let Some(pos) = code[i..].find("for ") {
        let at = i + pos;
        i = at + "for ".len();
        let Some(brace_rel) = code[at..].find('{') else {
            break;
        };
        let Some(open_rel) = code[at..at + brace_rel].find('[') else {
            continue;
        };
        let Some(table) = balanced_span(&code, at + open_rel, '[', ']') else {
            continue;
        };
        let Some(body) = braced_body(&code[at..], "for ") else {
            continue;
        };
        let names = string_literals(table);
        if names.is_empty() {
            continue;
        }
        if let Some(rel) = body.find(opener) {
            let anchor = rel + opener.len() - 1;
            if let Some(args) = balanced_span(body, anchor, '(', ')') {
                record(args, Some(&names));
            }
        }
    }
    out
}

/// Every `const NAME: T = <literal expression>;` in `src`, resolved to a number.
///
/// The validator's ceilings are named constants (`MAX_REPLAY_RING`,
/// `MAX_HOSTWARD_BUFFER`, `MAX_TIMER_MS`, `MAX_ROTATION_PADDING`), so a gate that
/// wants to know whether a bound constrains has to know what they are. Products are
/// resolved because one of them is written `16 * 1024 * 1024`.
fn file_consts(src: &str) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for line in strip_line_comments(src).lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("pub const ")
            .or(line.strip_prefix("const "))
        else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(':') else {
            continue;
        };
        let Some((_, value)) = tail.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name
            .chars()
            .all(|c| c == '_' || c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            continue;
        }
        if let Some(v) = resolve_u64(value.trim().trim_end_matches(';'), &BTreeMap::new()) {
            out.insert(name.to_owned(), v);
        }
    }
    out
}

/// `expr` as a number, if it is one this gate can read: an integer literal in any of
/// Rust's bases, a `uN::MAX`, a product of those, a named constant from `consts`, or
/// any of them under an `as` cast.
fn resolve_u64(expr: &str, consts: &BTreeMap<String, u64>) -> Option<u64> {
    let mut e = expr.trim();
    // Strip `as <type>` casts, which every ceiling in the validator wears.
    while let Some(at) = e.rfind(" as ") {
        e = e[..at].trim();
    }
    if let Some((a, b)) = e.split_once('*') {
        return resolve_u64(a, consts)?.checked_mul(resolve_u64(b, consts)?);
    }
    if let Some(ty) = e.strip_suffix("::MAX") {
        return type_max(ty.trim());
    }
    if let Some(v) = consts.get(e) {
        return Some(*v);
    }
    let digits = e.replace('_', "");
    if let Some(h) = digits.strip_prefix("0x") {
        return u64::from_str_radix(h, 16).ok();
    }
    if let Some(o) = digits.strip_prefix("0o") {
        return u64::from_str_radix(o, 8).ok();
    }
    if let Some(b) = digits.strip_prefix("0b") {
        return u64::from_str_radix(b, 2).ok();
    }
    digits.parse::<u64>().ok()
}

/// The largest value a field of type `ty` can hold, as a `u64`.
///
/// `usize` is read as 64-bit rather than as the host's width, so this gate answers
/// the same on every box: the *window* it is comparing against is a source
/// expression, which does not change with the target either. Signed and floating
/// types return `None` — the tree declares none today, and a gate that guessed at
/// one would be reporting a verdict about a domain it never worked out.
fn type_max(ty: &str) -> Option<u64> {
    let mut t = ty.trim();
    if let Some(inner) = t.strip_prefix("Option<") {
        t = inner.trim().trim_end_matches('>').trim();
    }
    match t {
        "u8" => Some(u64::from(u8::MAX)),
        "u16" => Some(u64::from(u16::MAX)),
        "u32" => Some(u64::from(u32::MAX)),
        "u64" | "usize" => Some(u64::MAX),
        _ => None,
    }
}

/// The fields whose declared window refuses **no value their type can hold**, each
/// with the reason — the honest reading of "structurally checked" in §16.12.
///
/// A bound constrains when at least one value of the field's own type falls outside
/// it, which is `low > 0 || high < type_max`. That admits `baud`'s `1 ..= u32::MAX`,
/// where the ceiling is the type's and §7.1 buys nonstandard rates on purpose but
/// `0` is refused, and refuses `0 ..= u64::MAX`, which is a `range_error` call that
/// can never produce an error.
///
/// A window this gate cannot *read* is reported too, and deliberately not skipped:
/// an unreadable bound is a bound nobody checked, and failing loudly with the
/// expression in hand is what keeps the resolver honest as the validator grows.
fn unconstraining_bounds(
    numeric: &BTreeMap<String, String>,
    windows: &BTreeMap<String, (String, String)>,
    consts: &BTreeMap<String, u64>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (field, ty) in numeric {
        let Some((low, high)) = windows.get(field) else {
            continue; // the missing-bound verdict is the other half of this gate
        };
        let (Some(low_v), Some(high_v), Some(max)) = (
            resolve_u64(low, consts),
            resolve_u64(high, consts),
            type_max(ty),
        ) else {
            out.push(format!(
                "`{field}: {ty}` is bounded by `{low} ..= {high}`, which this gate \
                 cannot evaluate — extend `resolve_u64`/`type_max` rather than let an \
                 unread bound count as a checked one"
            ));
            continue;
        };
        if low_v == 0 && high_v >= max {
            out.push(format!(
                "`{field}: {ty}` is bounded by `{low} ..= {high}`, which spans the \
                 whole of `{ty}` — the `range_error` call can never fire"
            ));
        }
    }
    out
}

/// Every field name `body` compares against a `MAX_*` constant — the exec codec's
/// spelling of the same rule, written by hand because a codec's attribute table is
/// opaque to the shared helper.
///
/// The comparison operator is required, and required to sit *between* the field and
/// the constant: `restart_backoff_ms` also appears in the error message two lines
/// down, beside a `{MAX_TIMER_MS}` interpolation, and a matcher that accepted mere
/// proximity would treat the message as the check — passing on a tree where someone
/// deleted the `if` and kept the sentence.
fn fields_compared_against_a_maximum(body: &str) -> BTreeSet<String> {
    maximum_comparison_windows(body).into_keys().collect()
}

/// The same comparisons, mapped to the window they impose: `field > MAX_X` bounds the
/// field to `0 ..= MAX_X`, which is the shape [`unconstraining_bounds`] reads.
fn maximum_comparison_windows(body: &str) -> BTreeMap<String, (String, String)> {
    let code = strip_line_comments(body);
    let bytes = code.as_bytes();
    let mut out = BTreeMap::new();
    let mut i = 0usize;
    while let Some(pos) = code[i..].find("MAX_") {
        let at = i + pos;
        i = at + 1;
        let mut name_end = at;
        while name_end < bytes.len()
            && (bytes[name_end] == b'_' || bytes[name_end].is_ascii_alphanumeric())
        {
            name_end += 1;
        }
        let constant = code[at..name_end].to_owned();
        // Back over the whitespace, then the operator, then more whitespace.
        let mut j = at;
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
            j -= 1;
        }
        if j > 0 && (bytes[j - 1] == b'=' || bytes[j - 1] == b'\'') {
            j -= 1; // the `=` of `>=` / `<=`
        }
        if j == 0 || !(bytes[j - 1] == b'>' || bytes[j - 1] == b'<') {
            continue;
        }
        j -= 1;
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
            j -= 1;
        }
        let end = j;
        while j > 0 && (bytes[j - 1] == b'_' || bytes[j - 1].is_ascii_alphanumeric()) {
            j -= 1;
        }
        if j < end {
            out.insert(code[j..end].to_owned(), ("0".to_owned(), constant));
        }
    }
    out
}

/// The field names stated in `docs/rpc/configuration.md`'s range table — the
/// *stated* half of §16.12, which is a separate claim from the checked half and
/// fails separately.
fn stated_range_table(configuration_md: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for row in table_rows(configuration_md, "| Field | Range | Why bounded |") {
        let Some(cell) = row.first() else { continue };
        out.extend(backticked(cell));
    }
    out
}

#[test]
fn every_numeric_configuration_attribute_is_bounded_and_says_so() {
    let root = repo_root();

    // 0. The matchers, in every spelling each side is written in — and in the near
    //    misses that must not vouch for a field.
    let schema = "Serial {\n    name: String,\n    #[serde(default = \"default_baud\")]\n    baud: u32,\n    /// prose about hostward_buffer: usize\n    hostward_buffer: usize,\n    mode: Option<u32>,\n    modem: ModemLines,\n    channels: Vec<String>,\n}";
    let fields = declared_fields(schema);
    let numeric: BTreeSet<String> = fields
        .iter()
        .filter(|(_, ty)| is_numeric_type(ty))
        .map(|(n, _)| n.clone())
        .collect();
    assert_eq!(
        numeric,
        BTreeSet::from([
            "baud".to_owned(),
            "hostward_buffer".to_owned(),
            "mode".to_owned()
        ]),
        "the schema reader mis-enumerates: it must take a plain numeric field, an \
         `Option<numeric>` (which is how `mode` is declared, the field review 32 \
         found unchecked), and neither the serde attribute above one nor a field \
         merely named in a doc comment"
    );
    assert!(
        !is_numeric_type("String") && !is_numeric_type("ModemLines"),
        "a non-numeric type is read as numeric, so this gate would demand a range \
         check for every string in the schema and be deleted within the day"
    );
    assert_eq!(
        range_checked_fields(
            "errors.extend(range_error(name, \"baud\", *baud as u64, 1, u32::MAX as u64));"
        ),
        BTreeSet::from(["baud".to_owned()]),
        "the literal `range_error` spelling — six of the validator's seven sites — \
         is not read"
    );
    assert_eq!(
        range_checked_fields(
            "for (field, value) in [(\"a_ms\", *a), (\"b_ms\", *b)] {\n errors.extend(range_error(name, field, value, 0, MAX_TIMER_MS));\n }"
        ),
        BTreeSet::from(["a_ms".to_owned(), "b_ms".to_owned()]),
        "the loop-table spelling is not read — the leg's three timers name their \
         fields in an array literal and the `range_error` call itself names none, so \
         all three would be reported as unchecked"
    );
    assert!(
        range_checked_fields(
            "for (field, label) in [(\"a_ms\", \"A\"), (\"b_ms\", \"B\")] {\n log(field, label);\n }"
        )
        .is_empty(),
        "any table of string pairs is read as a range check, so a logging loop could \
         vouch for a field nothing bounds"
    );
    assert_eq!(
        fields_compared_against_a_maximum("if attrs.restart_backoff_ms > MAX_TIMER_MS {"),
        BTreeSet::from(["restart_backoff_ms".to_owned()]),
        "the exec codec's hand-written comparison is not read, so its one timer \
         would be reported unchecked forever"
    );
    assert!(
        fields_compared_against_a_maximum(
            "format!(\"restart_backoff_ms = {}, above the maximum {MAX_TIMER_MS}\")"
        )
        .is_empty(),
        "the *message* naming the field beside the constant is read as the check — a \
         tree with the `if` deleted and the sentence kept would pass"
    );
    assert_eq!(
        range_windows(
            "errors.extend(range_error(name, \"baud\", *baud as u64, 1, u32::MAX as u64));"
        )
        .get("baud")
        .cloned(),
        Some(("1".to_owned(), "u32::MAX as u64".to_owned())),
        "the window reader does not recover a `range_error` call's low and high — \
         without them this gate reads that a bound was *written* and never that it \
         constrains anything"
    );
    assert_eq!(
        range_windows(
            "for (field, value) in [(\"a_ms\", *a), (\"b_ms\", *b)] {\n errors.extend(range_error(name, field, value, 0, MAX_TIMER_MS));\n }"
        )
        .get("b_ms")
        .cloned(),
        Some(("0".to_owned(), "MAX_TIMER_MS".to_owned())),
        "the loop-table spelling's window is not recovered, so the leg's three timers \
         would be exempt from the vacuity check below"
    );
    assert_eq!(
        maximum_comparison_windows("if attrs.restart_backoff_ms > MAX_TIMER_MS {")
            .get("restart_backoff_ms")
            .cloned(),
        Some(("0".to_owned(), "MAX_TIMER_MS".to_owned())),
        "the exec codec's comparison yields no window, so the one numeric attribute \
         outside the node schema is exempt from the vacuity check"
    );
    {
        // The arithmetic the vacuity verdict rests on, in every spelling the tree
        // writes a bound in.
        let consts = BTreeMap::from([("MAX_TIMER_MS".to_owned(), 3_600_000u64)]);
        for (expr, want) in [
            ("1", Some(1)),
            ("0o777", Some(511)),
            ("65_536", Some(65_536)),
            ("16 * 1024 * 1024", Some(16 * 1024 * 1024)),
            ("u32::MAX as u64", Some(u64::from(u32::MAX))),
            ("u64::MAX", Some(u64::MAX)),
            ("MAX_TIMER_MS", Some(3_600_000)),
            ("MAX_ROTATION_PADDING as u64", None),
        ] {
            assert_eq!(
                resolve_u64(expr, &consts),
                want,
                "`{expr}` does not resolve as expected — a bound this gate misreads is \
                 a bound it either exempts or falsely accuses"
            );
        }
        assert_eq!(
            (type_max("u32"), type_max("Option<u32>"), type_max("i64")),
            (Some(u64::from(u32::MAX)), Some(u64::from(u32::MAX)), None),
            "the type-domain reader must unwrap `Option` (that is how `mode` is \
             declared) and must refuse a type it has not worked out rather than guess"
        );
        // The verdict function itself: `1 ..= u32::MAX` on a `u32` constrains (it
        // refuses `0`), `0 ..= u64::MAX` on a `u32` refuses nothing at all, and an
        // unreadable window is reported rather than waved through.
        let numeric = BTreeMap::from([("baud".to_owned(), "u32".to_owned())]);
        let real = BTreeMap::from([(
            "baud".to_owned(),
            ("1".to_owned(), "u32::MAX as u64".to_owned()),
        )]);
        let vacuous =
            BTreeMap::from([("baud".to_owned(), ("0".to_owned(), "u64::MAX".to_owned()))]);
        let unreadable = BTreeMap::from([(
            "baud".to_owned(),
            ("0".to_owned(), "something_new()".to_owned()),
        )]);
        assert!(unconstraining_bounds(&numeric, &real, &consts).is_empty());
        assert_eq!(unconstraining_bounds(&numeric, &vacuous, &consts).len(), 1);
        assert_eq!(
            unconstraining_bounds(&numeric, &unreadable, &consts).len(),
            1,
            "an unreadable bound is silently treated as a checked one, which is the \
             failing-open shape this whole file exists to prevent"
        );
    }
    assert_eq!(
        stated_range_table(
            "| Field | Range | Why bounded |\n| --- | --- |\n| `a_ms`, `b_ms` | `0 ..= 1` | because |\n| `mode` (pty) | `0 ..= 2` | because |\n"
        ),
        BTreeSet::from(["a_ms".to_owned(), "b_ms".to_owned(), "mode".to_owned()]),
        "the range-table reader must take every field a row names (three rows carry \
         more than one) and must not be confused by the kind in parentheses"
    );

    // 1. Both sides, from their real sources.
    let mut declared: BTreeMap<String, String> = BTreeMap::new();
    for (rel, header) in NUMERIC_SCHEMAS {
        let src = strip_line_comments(&read_tree_file(&root, rel));
        let body = braced_body(&src, header).unwrap_or_else(|| {
            panic!(
                "{rel} no longer declares `{header}` — the schema this gate derives \
                 §16.12's roster from is gone, and an empty roster agrees with any \
                 validator at all"
            )
        });
        declared.extend(declared_fields(body));
    }
    let numeric: BTreeSet<String> = declared
        .iter()
        .filter(|(_, ty)| is_numeric_type(ty))
        .map(|(n, _)| n.clone())
        .collect();

    let config_rs = strip_line_comments(&read_tree_file(&root, "core/src/config.rs"));
    let validate = braced_body(&config_rs, "pub fn validate")
        .expect("core/src/config.rs declares `pub fn validate`");
    let exec_rs = strip_line_comments(&read_tree_file(&root, "daemon/src/nodes/exec.rs"));
    let parse_attributes = braced_body(&exec_rs, "pub fn parse_attributes")
        .expect("daemon/src/nodes/exec.rs declares `pub fn parse_attributes`");
    let mut checked = range_checked_fields(validate);
    checked.extend(fields_compared_against_a_maximum(parse_attributes));
    let stated = stated_range_table(&read_tree_file(&root, "docs/rpc/configuration.md"));

    // The windows those checks impose, and the constants they are written in terms
    // of, for the vacuity half below.
    let numeric_typed: BTreeMap<String, String> = declared
        .iter()
        .filter(|(_, ty)| is_numeric_type(ty))
        .map(|(n, ty)| (n.clone(), ty.clone()))
        .collect();
    let mut consts = file_consts(&config_rs);
    consts.extend(file_consts(&exec_rs));
    let mut windows = range_windows(validate);
    windows.extend(maximum_comparison_windows(parse_attributes));
    assert!(
        consts.len() >= 4 && windows.len() >= 9,
        "{} constant(s) and {} window(s) were read — the four `MAX_*` ceilings and the \
         nine-plus checked fields are both floors, and a vacuity verdict over an empty \
         window set is silence, not a pass",
        consts.len(),
        windows.len()
    );

    assert!(
        numeric.len() >= 9,
        "the schemas declare {} numeric field(s) — the node schema alone carries \
         nine, so the field reader has stopped reading and this gate is comparing \
         nothing against everything",
        numeric.len()
    );
    assert!(
        checked.len() >= 9,
        "only {} field(s) are structurally range-checked — the validator was \
         reshaped and the check reader no longer sees it",
        checked.len()
    );
    assert!(
        stated.len() >= 9,
        "docs/rpc/configuration.md's range table parsed to {} field(s) — it was \
         retitled or reshaped, and the *stated* half of §16.12 is now unchecked",
        stated.len()
    );

    // 2. Planted against the real sources, both halves, before a clean verdict is
    //    trusted. The victim is taken from the tree rather than named here, so
    //    neither proof goes stale when the schema grows.
    // Drawn from the *intersection*, not from the schema: a victim that is already
    // unchecked would make the deletion below a no-op, and the gate would then fail
    // at its own scaffolding with a message about the proof instead of at the
    // verdict with a message about the field. (Measured: with `advertised_baud`'s
    // check removed — the plan §18 item 62 defect, replayed — a schema-order victim
    // reddened this test on the wrong sentence.)
    let victim = numeric
        .iter()
        .find(|f| checked.contains(*f))
        .expect(
            "no numeric field is range-checked at all — the validator or the check \
             reader is gone, and the verdicts below would name every field at once",
        )
        .clone();
    let unchecked = config_rs.replace(&format!("\"{victim}\""), "\"planted_no_such_field\"");
    let mut planted = range_checked_fields(
        braced_body(&unchecked, "pub fn validate").expect("the planted source still validates"),
    );
    planted.extend(fields_compared_against_a_maximum(parse_attributes));
    let unchecked_exec = exec_rs.replace("> MAX_TIMER_MS", "> u64::MAX");
    let exec_planted = fields_compared_against_a_maximum(
        braced_body(&unchecked_exec, "pub fn parse_attributes")
            .expect("the planted source still parses attributes"),
    );
    assert!(
        checked.contains(&victim),
        "`{victim}` is not range-checked on this tree, so the deletion below deletes \
         nothing and proves nothing"
    );
    assert!(
        !planted.contains(&victim),
        "renaming `{victim}`'s range check in core/src/config.rs left it reported as \
         checked: the gate's whole subject — a numeric field with no bound — passes it"
    );
    let victim_drift = drift(
        "the configuration schema",
        &numeric,
        "the validator",
        &planted,
    );
    assert!(
        victim_drift.iter().any(|m| m.contains(&victim)),
        "a real field with its real range check removed produced no drift naming it. \
         Reported instead: {victim_drift:?}"
    );
    assert!(
        !exec_planted.contains("restart_backoff_ms"),
        "replacing the exec codec's `> MAX_TIMER_MS` comparison left the field \
         reported as checked, so the one numeric attribute outside the node schema \
         is ungated"
    );
    // …and the same removal must reach the *verdict*, not merely the matcher. Run
    // the real comparison with the exec half of `checked` withheld, which is the
    // state the tree would be in if that check were deleted. (Done on the values
    // rather than on the file: `daemon/src/nodes/exec.rs` is not this gate's to
    // edit, and a proof that needs to write a file it does not own is a proof that
    // will be skipped.)
    let node_schema_only = range_checked_fields(validate);
    let exec_drift = drift(
        "the configuration schema",
        &numeric,
        "the validator",
        &node_schema_only,
    );
    assert!(
        exec_drift.iter().any(|m| m.contains("restart_backoff_ms")),
        "with the exec codec's own range check withheld, the verdict does not name \
         the field it bounds — so this gate covers the node schema and quietly not \
         the codec attribute table beside it. Reported instead: {exec_drift:?}"
    );
    // The anchor is code rather than prose: `config_rs` is comment-stripped above,
    // so a doc-comment anchor would silently match nothing and this proof would
    // assert that an unchanged file is unchanged.
    let phantom = "planted_unbounded_ms";
    let anchor = "\n    Pty {";
    assert_eq!(
        config_rs.matches(anchor).count(),
        1,
        "the planted-field anchor is no longer unique in core/src/config.rs"
    );
    let phantom_schema =
        config_rs.replacen(anchor, &format!("{anchor}\n        {phantom}: u64,"), 1);
    assert_ne!(
        phantom_schema, config_rs,
        "the phantom field was never inserted, so the proof below asserts nothing"
    );
    let phantom_fields = declared_fields(
        braced_body(&phantom_schema, "pub enum NodeConfig").expect("the planted schema parses"),
    );
    assert!(
        phantom_fields
            .get(phantom)
            .is_some_and(|ty| is_numeric_type(ty)),
        "a numeric field planted into the real `NodeConfig` is not enumerated — a \
         field added tomorrow is invisible to this gate, which is exactly the \
         per-field exhaustiveness plan §18 item 62 filed"
    );
    let phantom_drift = drift(
        "the configuration schema",
        &phantom_fields
            .iter()
            .filter(|(_, ty)| is_numeric_type(ty))
            .map(|(n, _)| n.clone())
            .collect(),
        "the validator",
        &checked,
    );
    assert!(
        phantom_drift.iter().any(|m| m.contains(phantom)),
        "a numeric field with no range check produced no drift naming it. Reported \
         instead: {phantom_drift:?}"
    );
    // …and the *stated* half against the real table, which fails separately and so
    // has to be proved separately: strike the row that names the victim and require
    // the second verdict to notice. Rows are matched on their first cell, because
    // three of them name more than one field and several name a field in prose
    // further along the row.
    let configuration_md = read_tree_file(&root, "docs/rpc/configuration.md");
    let stale_table: String = configuration_md
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with('|')
                && backticked(trimmed.trim_matches('|').split('|').next().unwrap_or(""))
                    .contains(&victim))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let stale_stated = stated_range_table(&stale_table);
    assert!(
        !stale_stated.contains(&victim),
        "striking `{victim}`'s row from docs/rpc/configuration.md's range table left \
         it parsed as stated — the planted deletion below asserts nothing"
    );
    let unstated_drift = drift(
        "the range-checked fields",
        &checked,
        "docs/rpc/configuration.md's range table",
        &stale_stated,
    );
    assert!(
        unstated_drift.iter().any(|m| m.contains(&victim)),
        "a bound that loses its table row produced no drift naming it: §16.12's \
         *stated* half could go unwritten while the check stayed. Reported instead: \
         {unstated_drift:?}"
    );

    // …and the **vacuity** plant, against the real validator: widen a real window to
    // its type's whole domain — the reviewer's own plant, `baud`'s
    // `1, u32::MAX as u64` becoming `0, u64::MAX` — and require the verdict to name
    // it. Until 2026-08-15 that edit left this gate green, because it read that a
    // bound was *written* and never that it constrains: a `range_error` call that can
    // never fire satisfied a gate whose subject is §16.12's "structurally checked
    // maximum". Only `core/src/config.rs`'s own hand-written unit test noticed, which
    // is the layer plan §18 item 62 filed as unreliable.
    //
    // Done on the *values* rather than on the file, for the reason the exec plant
    // above gives and one more: if the tree ever really carries this defect, a
    // text-anchored plant finds nothing to widen and reds on its own scaffolding —
    // the gate failing with a message about the proof instead of about the field,
    // which is the mistake the victim-selection comment above records. The victim is
    // taken from the windows that currently *do* constrain, so the proof cannot go
    // stale as the schema grows, and the reader's half is proved separately in step 0
    // against a `range_error` call written out in full.
    let widened_victim = numeric_typed
        .iter()
        .find(|(f, _)| {
            windows.get(*f).is_some_and(|w| {
                unconstraining_bounds(
                    &numeric_typed,
                    &BTreeMap::from([((*f).clone(), w.clone())]),
                    &consts,
                )
                .is_empty()
            })
        })
        .map(|(f, _)| f.clone())
        .expect(
            "no numeric field carries a window this gate can both read and call \
             constraining — the verdict below would be about the resolver, not the tree",
        );
    let mut widened_windows = windows.clone();
    widened_windows.insert(
        widened_victim.clone(),
        ("0".to_owned(), "u64::MAX".to_owned()),
    );
    let vacuity_drift = unconstraining_bounds(&numeric_typed, &widened_windows, &consts);
    assert!(
        vacuity_drift.iter().any(|m| m.contains(&widened_victim)),
        "`{widened_victim}`'s range check widened to `0 ..= u64::MAX` produced no \
         verdict naming it: a bound that refuses nothing still reads as a bound here. \
         Reported instead: {vacuity_drift:?}"
    );

    // 3. The verdict, both halves of the promise. **No exemption list**, and that is
    //    a decision rather than an omission: §16.12 admits no numeric attribute
    //    without a maximum, so an exemption here would be the invariant's own escape
    //    hatch. A field that genuinely cannot carry one is a design amendment
    //    (AGENTS §5), not a row in a list in a test.
    let unbounded = drift(
        "the configuration schema",
        &numeric,
        "the validator",
        &checked,
    );
    let unbounded: Vec<String> = unbounded
        .into_iter()
        .filter(|m| m.contains("but not in the validator"))
        .collect();
    assert!(
        unbounded.is_empty(),
        "a numeric configuration attribute has no structural range check, which is \
         §16.12/§11 invariant 13 — \"every numeric attribute … carries a stated, \
         structurally checked maximum\":\n  {}\nThe fix is a `range_error(…)` site \
         in `GraphConfig::validate` (or, for a codec attribute, the same comparison \
         in its own `parse_attributes`) plus a row in docs/rpc/configuration.md's \
         range table. The invariant admits no exemption: a value an operator can \
         type is a value that has to be bounded before anything is created (§11).",
        unbounded.join("\n  ")
    );
    let unstated: Vec<String> = drift(
        "the range-checked fields",
        &checked,
        "docs/rpc/configuration.md's range table",
        &stated,
    );
    assert!(
        unstated.is_empty(),
        "§16.12 asks for a *stated* maximum as well as a checked one, and the two \
         have drifted:\n  {}\nA field checked but not tabled is a bound an operator \
         meets only by tripping it; a field tabled but not checked is a documented \
         promise nothing keeps (§16.14: editing either side alone is a test failure).",
        unstated.join("\n  ")
    );
    let vacuous = unconstraining_bounds(&numeric_typed, &windows, &consts);
    assert!(
        vacuous.is_empty(),
        "a numeric configuration attribute carries a range check that refuses nothing \
         its type can hold:\n  {}\nA `range_error(…)` call is not the promise; \
         §16.12/§11 invariant 13 asks for a *maximum*, and a window spanning the whole \
         type is the check present and the bound absent — which reads, to every gate \
         and every reviewer, exactly like a field that is bounded. Narrow the window, \
         or amend the design (AGENTS §5) if the value genuinely has no maximum.\n\
         **What this half does not claim**: that each window is the *right* one. \
         `1 ..= u32::MAX` passes because it refuses `0`, and whether `u32::MAX` is a \
         sensible ceiling for a baud rate is a design question §7.1 answers on \
         purpose, not one a structural gate can. The residual is stated rather than \
         implied, and what covers it is `core/src/config.rs`'s own per-field tests \
         and its `validate` property test.",
        vacuous.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// (f) The observation surface against `docs/rpc/observation.md`
// ---------------------------------------------------------------------------

/// The directory whose modules build the `state` verb's node objects.
///
/// Listed rather than named file by file: a node kind added tomorrow lands here,
/// and a gate that carried its own list of kinds would be exactly the hand-kept
/// roster this file exists to abolish. §7's node kinds are the directory.
const NODE_MODULES: &str = "daemon/src/nodes";

/// The one page design §5 makes authoritative for this surface.
///
/// Deliberately *not* "anywhere under `docs/rpc/`". Half of these keys share a name
/// with a configuration field — `baud`, `faces`, `role`, `transport`, `codec` — so a
/// gate that accepted any page would let `configuration.md` vouch for a *state* key
/// it never describes, and would report green while the enumeration §5 promises had
/// a hole in it. The narrower target is the stricter one, which is the tell that it
/// is the right one (AGENTS §9).
const OBSERVATION_DOC: &str = "docs/rpc/observation.md";

/// State keys emitted by `src` that this gate does not require `observation.md` to
/// name, each with the reason.
///
/// Empty, and expected to stay that way — a key on the wire with no entry on the
/// schema page is the defect, not the exception. It exists because the *walker*
/// here is a whole module rather than one function: a future `json!` in a node
/// module that is not part of `state` would be a false positive, and the honest
/// answer to one is a named exemption rather than a weakened matcher. Two-sided
/// below: an entry naming a key that is not emitted, or one that *is* documented,
/// fails the gate.
const UNDOCUMENTED_STATE_KEYS: &[(&str, &str)] = &[];

/// Every JSON object key `src` writes, in the three spellings the node modules use.
///
/// 1. `"key": value` inside a `json!` literal — the great majority.
/// 2. `obj.insert("key".to_owned(), …)` — how the shared unconfigured-channel
///    reporter and the map node write into an object they were handed.
/// 3. `obj["key"] = …` — one instance today, the leg's `insecure_bind` confession,
///    which is written this way precisely because it is *conditional*, and a
///    conditional key is the one most likely to be missed by a human reader.
///
/// Comments are stripped and the `mod tests` block is removed: prose about a key is
/// not an emission of it, and a key asserted in a unit test is a key that already
/// has to come from somewhere real.
fn emitted_state_keys(src: &str) -> BTreeSet<String> {
    let code = strip_tests_module(&strip_line_comments(src));
    let mut out = BTreeSet::new();
    let mut rest = code.as_str();
    let mut base = 0usize;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let token = &after[..close];
        let token_start = base + open + 1;
        let tail = after[close + 1..].trim_start();
        let looks_like_a_key = !token.is_empty()
            && token.starts_with(|c: char| c.is_ascii_lowercase())
            && token
                .chars()
                .all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit());
        if looks_like_a_key {
            // Spelling 1 and 2 are told apart by what follows; spelling 3 by what
            // precedes. `"key":` must not be confused with `key: "value"` — the
            // `tracing` macros are full of the latter — so the colon has to come
            // *after* the literal.
            let lead = code[..token_start.saturating_sub(1)].trim_end();
            if tail.starts_with(':')
                || (lead.ends_with("insert(") && tail.starts_with(&[',', '.'][..]))
                || (lead.ends_with('[')
                    && tail.starts_with(']')
                    && tail[1..].trim_start().starts_with('='))
            {
                out.insert(token.to_owned());
            }
        }
        base += open + 1 + close + 1;
        rest = &after[close + 1..];
    }
    out
}

/// `src` with a trailing `mod tests { … }` block removed, brace-counted.
fn strip_tests_module(src: &str) -> String {
    let Some(at) = src.find("mod tests") else {
        return src.to_owned();
    };
    let Some(body) = braced_body(&src[at..], "mod tests") else {
        return src[..at].to_owned();
    };
    // `body` is a subslice of `src[at..]`; everything after its closing brace stays.
    let body_end = (body.as_ptr() as usize - src.as_ptr() as usize) + body.len() + 1;
    format!("{}{}", &src[..at], &src[body_end.min(src.len())..])
}

/// Does `doc` **document** `key` — name it in backticks inside a Markdown table
/// row, which is the form every entry on this page takes?
///
/// **A mention is not documentation, and the first version of this asked only for a
/// mention.** It was a word-boundary substring search over the whole file, so a
/// single appended line — measured, with the exact bytes
/// `TODO(nobody): <all twenty-nine key names>` — turned the gate green over a page
/// that documented none of them. The gate's name, this file's module doc, the
/// verdict message and the item all said *documented*; the code said *appears
/// somewhere*, which is AGENTS §3's second new register: an assertion strictly
/// weaker than the comment above it, and the comment is what a reviewer reads.
///
/// Rows rather than first cells: `observation.md` documents nested keys in the
/// Description cell of the object that carries them (`modem_lines`'s six booleans,
/// `driver_counters`' seven, `client_termios`' six, and the per-channel keys under
/// `channels`), and those are genuine entries. Requiring the *first* cell would
/// demand a top-level row for a key that has no top-level existence. The backticks
/// and the row together are what a mention cannot fake: prose, a heading and a TODO
/// list are all excluded, and `approx` still does not vouch for `rx`.
fn doc_documents_key(doc: &str, key: &str) -> bool {
    doc.lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .any(|row| backticked(row).iter().any(|cell| cell == key))
}

/// The keys `doc` documents as **rows of the per-kind extras section** — the
/// reverse of [`doc_documents_key`], and the side that catches a row for a key
/// nothing emits.
///
/// Scoped to that one section, and to the *first* cell of each row, because both
/// widenings would make this side meaningless: the page's other tables describe
/// `state`'s top-level shape (`nodes`, `taps`, `endpoints`, `waits` and the tap and
/// wait objects), which `daemon/src/daemon.rs` builds and no node module emits, and
/// a Description cell names configuration fields, sibling counters and prose
/// identifiers that are not that row's subject.
///
/// The `**map**` line is prose pointing at the map node's own section rather than a
/// table, so its three keys are not read here. That is a floor, stated: this side
/// asserts that every *row* is real, not that the section is exhaustively tabular.
fn documented_extra_keys(doc: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut inside = false;
    for line in doc.lines() {
        if line.starts_with("### ") {
            inside = line.contains("node-type extras");
            continue;
        }
        if line.starts_with("## ") || line.starts_with("# ") {
            inside = false;
            continue;
        }
        if !inside || !line.trim_start().starts_with('|') {
            continue;
        }
        let first = line
            .trim()
            .trim_matches('|')
            .split('|')
            .next()
            .unwrap_or("");
        out.extend(backticked(first));
    }
    out
}

/// The keys `emitted` that `doc` does not document and no exemption covers.
fn undocumented_keys(
    emitted: &BTreeMap<String, BTreeSet<String>>,
    doc: &str,
    exempt: &[(&str, &str)],
) -> Vec<String> {
    emitted
        .iter()
        .filter(|(key, _)| !doc_documents_key(doc, key))
        .filter(|(key, _)| !exempt.iter().any(|(e, _)| *e == key.as_str()))
        .map(|(key, from)| {
            format!(
                "`{key}` is emitted by {} but has no row in {OBSERVATION_DOC}",
                from.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        })
        .collect()
}

/// The keys `doc`'s per-kind extras section gives a row that nothing under
/// `daemon/src/nodes/` emits.
fn undocumented_in_reverse(
    emitted: &BTreeMap<String, BTreeSet<String>>,
    documented: &BTreeSet<String>,
) -> Vec<String> {
    documented
        .iter()
        .filter(|key| !emitted.contains_key(*key))
        .map(|key| {
            format!(
                "`{key}` has a row in {OBSERVATION_DOC} but nothing under {NODE_MODULES}/ emits it"
            )
        })
        .collect()
}

#[test]
fn every_state_key_the_daemon_emits_is_documented() {
    let root = repo_root();

    // 0. The matcher, in all three spellings it claims to cover and in the near
    //    misses that must not manufacture a key.
    let planted = "json!({ \"alpha\": 1, \"beta\": x });\n\
                   obj.insert(\"gamma\".to_owned(), json!(2));\n\
                   obj[\"delta\"] = json!(true);\n";
    assert_eq!(
        emitted_state_keys(planted),
        BTreeSet::from([
            "alpha".to_owned(),
            "beta".to_owned(),
            "gamma".to_owned(),
            "delta".to_owned()
        ]),
        "the key matcher misses one of the three spellings the node modules use. \
         The `json!` literal is the common one; `insert` is how the shared \
         unconfigured-channel reporter writes; and `obj[\"k\"] = …` is the leg's \
         conditional `insecure_bind`, which is exactly the kind of key a human \
         reader skips"
    );
    assert!(
        emitted_state_keys("tracing::warn!(target: \"codec\", channel = %id, \"dropped\");")
            .is_empty(),
        "a `key: \"value\"` pair is read as an emitted key — the tracing macros are \
         written that way throughout the node modules, so every log target would be \
         demanded a row on the schema page"
    );
    assert!(
        emitted_state_keys("assert_eq!(state[\"planted\"], json!(1));").is_empty(),
        "an object *read* by subscript counts as one written — every field any unit \
         test inspects would become a documentation obligation"
    );
    assert!(
        emitted_state_keys("// one day: json!({ \"planted\": 1 })\nlet x = 1;").is_empty(),
        "a key proposed in a comment is enumerated as emitted"
    );
    assert_eq!(
        strip_tests_module(
            "fn a() {}\n#[cfg(test)]\nmod tests {\n fn b() { let _ = 1; }\n}\nfn c() {}\n"
        )
        .trim(),
        "fn a() {}\n#[cfg(test)]\n\nfn c() {}".trim(),
        "the test-module stripper either leaves the tests in or eats the code after \
         them — the first makes a fixture key a documentation obligation, the second \
         hides real emissions from the walk"
    );
    // The doc matcher, in the form the page really uses and against every near miss
    // that must not pass for documentation. The TODO line is the reviewer's own
    // plant, kept verbatim: appended to the page it turned the *previous* matcher —
    // a word-boundary substring search — green over twenty-nine undocumented keys.
    assert!(
        doc_documents_key("| `rx` | integer | characters the driver saw |", "rx"),
        "a real per-kind row is not read as documentation, so this gate demands rows \
         the page already has and would be deleted within the day"
    );
    assert!(
        doc_documents_key(
            "| `modem_lines` | object | six booleans: `dtr`, `rts`, `cts` |",
            "dtr"
        ),
        "a key documented inside the Description cell of the object that carries it \
         is not read — `modem_lines`, `driver_counters`, `client_termios` and the \
         per-channel keys all document their members that way, and demanding a \
         top-level row would demand rows for keys with no top-level existence"
    );
    for near_miss in [
        "the `rx` counter is described in prose",
        "TODO(nobody): rx tx feed_dropped delivered_hostward",
        "### `rx`",
        "| `approx` | integer | not it |",
        "| rx | integer | unbackticked, so not an entry |",
    ] {
        assert!(
            !doc_documents_key(near_miss, "rx"),
            "{near_miss:?} is accepted as documentation of `rx`. A mention is not an \
             entry: the previous matcher was a word-boundary substring search over \
             the whole file, and one appended `TODO(nobody): …` line naming every key \
             made this gate green over a page that documented none of them"
        );
    }
    // The exemption mechanism, exercised rather than merely declared: with the entry
    // it is silent, without it the key is reported. An empty `const` proves neither.
    let synthetic: BTreeMap<String, BTreeSet<String>> =
        BTreeMap::from([("planted".to_owned(), BTreeSet::from(["x.rs".to_owned()]))]);
    assert!(
        undocumented_keys(&synthetic, "nothing here", &[("planted", "a reason")]).is_empty(),
        "a named exemption does not suppress its key, so the mechanism cannot be used"
    );
    assert_eq!(
        undocumented_keys(&synthetic, "nothing here", &[]).len(),
        1,
        "an unexempted, undocumented key is not reported — the gate's whole subject"
    );

    // 1. The walker: list the node modules, and require the listing to reach a file
    //    planted beside them. A gate that carried its own list of node kinds would
    //    go stale the day a kind is added, which is the failure mode of every other
    //    roster in this file.
    let dir = root.join(NODE_MODULES);
    let mut modules: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    modules.sort();
    assert!(
        modules.len() >= 7,
        "only {} module(s) under {NODE_MODULES}/ — §7 ships six node kinds plus the \
         exec codec, so the listing has stopped listing and this gate would compare \
         an empty surface against a full page",
        modules.len()
    );

    // 2. The emitted surface, from those modules.
    let mut emitted: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in &modules {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for key in emitted_state_keys(&src) {
            emitted.entry(key).or_default().insert(name.clone());
        }
    }
    assert!(
        emitted.len() >= 50,
        "only {} state key(s) enumerated from {NODE_MODULES}/ — the seven kinds \
         carry far more than that between them, so the matcher has stopped matching",
        emitted.len()
    );

    let doc = read_tree_file(&root, OBSERVATION_DOC);
    assert!(
        doc.len() > 10_000,
        "{OBSERVATION_DOC} is {} bytes — it was truncated or replaced, and a gate \
         comparing a full surface against a stub reports every key as undocumented \
         (or, if the stub is empty of nothing, reports nothing at all)",
        doc.len()
    );

    // 3. Planted against the real page and the real sources, both directions.
    let victim = emitted
        .keys()
        .find(|k| k.len() > 6 && doc_documents_key(&doc, k))
        .expect("some documented key")
        .clone();
    let stale: String = doc
        .lines()
        .filter(|l| !doc_documents_key(l, &victim))
        .collect::<Vec<_>>()
        .join("\n");
    let planted_drift = undocumented_keys(&emitted, &stale, UNDOCUMENTED_STATE_KEYS);
    assert!(
        planted_drift.iter().any(|m| m.contains(&victim)),
        "striking every line naming `{victim}` from {OBSERVATION_DOC} produced no \
         drift naming it: this gate cannot notice a documented key losing its \
         documentation. Reported instead: {planted_drift:?}"
    );
    // …and a key that arrives in the code with no page entry, in each of the three
    // spellings, against the real page rather than a fixture.
    for (spelling, planted_src) in [
        ("a json! literal", "json!({ \"planted_state_key\": 1 })"),
        (
            "an insert call",
            "obj.insert(\"planted_state_key\".to_owned(), json!(1));",
        ),
        ("a subscript assignment", "obj[\"planted_state_key\"] = j;"),
    ] {
        let mut with_planted = emitted.clone();
        for key in emitted_state_keys(planted_src) {
            with_planted
                .entry(key)
                .or_default()
                .insert("planted.rs".to_owned());
        }
        let d = undocumented_keys(&with_planted, &doc, UNDOCUMENTED_STATE_KEYS);
        assert!(
            d.iter().any(|m| m.contains("planted_state_key")),
            "a new state key written as {spelling} produced no drift: a counter can \
             reach the wire with no schema entry, which is the drift plan §18 item \
             63 measured at seventeen keys and this gate at twenty-nine. Reported \
             instead: {d:?}"
        );
    }

    // 4. The exemption list is two-sided: an entry must name a key that is really
    //    emitted and really absent from the page, or it is a licence nobody needs.
    for (key, reason) in UNDOCUMENTED_STATE_KEYS {
        assert!(
            emitted.contains_key(*key),
            "`{key}` is exempted from {OBSERVATION_DOC} (\"{reason}\") but nothing \
             under {NODE_MODULES}/ emits it — an exemption for a key that no longer \
             exists is a hole waiting for the name to be reused"
        );
        assert!(
            !doc_documents_key(&doc, key),
            "`{key}` is exempted from {OBSERVATION_DOC} (\"{reason}\") but the page \
             documents it: delete the exemption rather than leave a licence standing \
             over a key that does not need one"
        );
    }

    // 5. **The other direction** — a row for a key nothing emits. Every sibling
    //    roster gate in this file is two-sided, and this one was not: a
    //    `| `phantom_key_nothing_emits` |` row added to the page passed, so the
    //    "authoritative per-kind enumeration" could document a counter that was
    //    renamed, moved to another kind, or never existed, and an operator would go
    //    looking for it in `state`. Stale documentation reads exactly like current
    //    documentation, which is what makes this half worth the same plant as the
    //    other.
    let documented = documented_extra_keys(&doc);
    assert!(
        documented.len() >= 30,
        "the per-kind extras section parsed to {} row key(s) — the six kind tables \
         carry far more than that, so the section was retitled or reshaped and this \
         direction is comparing an empty set against everything",
        documented.len()
    );
    // The matcher: rows inside the section are read, the header and separator rows
    // are not, and rows in the page's *other* tables are out of scope (they describe
    // `state`'s top-level shape, which `daemon/src/daemon.rs` builds).
    let synthetic_doc = "## `state`\n\n| Field | Type | Description |\n| --- | --- |\n\
                         | `nodes` | array | top-level, not a node extra |\n\n\
                         ### The node-type extras, per kind\n\n\
                         **serial** (§7.1):\n\n| Field | Type | Description |\n\
                         | --- | --- | --- |\n\
                         | `alpha` | integer | with `beta` named in the description |\n\
                         | `gamma`, `delta` | integer | two keys in one first cell |\n\n\
                         ### Loss counters\n\n| `epsilon` | integer | past the section |\n";
    assert_eq!(
        documented_extra_keys(synthetic_doc),
        BTreeSet::from(["alpha".to_owned(), "gamma".to_owned(), "delta".to_owned()]),
        "the reverse-direction reader must take every key a row's first cell names \
         (several rows name more than one), and must take neither a key named only in \
         a Description cell, nor a row from a table outside the per-kind section, nor \
         the `Field`/`---` scaffolding"
    );
    // Planted against the real page: a row for a key nothing emits must be reported.
    let phantom_key = "phantom_key_nothing_emits";
    let anchor = "\n**pty** (§7.2):";
    assert_eq!(
        doc.matches(anchor).count(),
        1,
        "the phantom-row anchor is no longer unique in {OBSERVATION_DOC}"
    );
    let phantom_doc = doc.replacen(
        anchor,
        &format!("\n| `{phantom_key}` | integer | planted |\n{anchor}"),
        1,
    );
    assert!(
        documented_extra_keys(&phantom_doc).contains(phantom_key),
        "a row planted into the real per-kind section is not enumerated, so the \
         verdict below could not see one"
    );
    let phantom_reverse = undocumented_in_reverse(&emitted, &documented_extra_keys(&phantom_doc));
    assert!(
        phantom_reverse.iter().any(|m| m.contains(phantom_key)),
        "a documented key that nothing emits produced no drift naming it. Reported \
         instead: {phantom_reverse:?}"
    );
    // …and the mirror plant, so this side is proved to *discriminate* rather than to
    // report everything: with the real page and the real sources it must be silent,
    // which is the assertion the verdict below makes anyway — but with a real key's
    // emission withheld it must name that key too, which is what says the comparison
    // reads `emitted` rather than a constant.
    // The victim is drawn from the *rows*, not from the forward half's victim: the
    // forward side accepts a key documented in a Description cell (`accepted_targetward`
    // is one), and withholding such a key would prove nothing here because this side
    // never claimed it.
    let reverse_victim = documented
        .iter()
        .find(|k| emitted.contains_key(*k))
        .expect(
            "no row key in the per-kind extras section is emitted at all — the two \
             sides have no overlap, and the verdict below would name every row",
        )
        .clone();
    let mut short_emitted = emitted.clone();
    short_emitted.remove(&reverse_victim);
    let withheld_reverse = undocumented_in_reverse(&short_emitted, &documented);
    assert!(
        withheld_reverse.iter().any(|m| m.contains(&reverse_victim)),
        "with `{reverse_victim}`'s emission withheld the reverse verdict does not name \
         it, so this side is not reading the emitted surface at all. Reported instead: \
         {withheld_reverse:?}"
    );

    // 6. The verdict, both directions.
    let d = undocumented_keys(&emitted, &doc, UNDOCUMENTED_STATE_KEYS);
    assert!(
        d.is_empty(),
        "the observation surface has drifted from its schema page:\n  {}\n\
         Design §5 makes {OBSERVATION_DOC} \"the authoritative per-kind enumeration \
         and stays so\", so a key the daemon puts in `state` with no entry there is a \
         wire surface with no schema — the drift class that produced §15.54, where \
         the loss taxonomy had to be corrected because four shipped counters \
         falsified it. Document the key, or stop emitting it.",
        d.join("\n  ")
    );
    let r = undocumented_in_reverse(&emitted, &documented);
    assert!(
        r.is_empty(),
        "{OBSERVATION_DOC}'s per-kind extras section gives a row to a key nothing \
         under {NODE_MODULES}/ emits:\n  {}\n\
         An authoritative enumeration is authoritative in both directions: a row for a \
         counter that was renamed, moved to another kind or never shipped sends an \
         operator looking in `state` for something that is not there, and reads exactly \
         like a row that is current. Delete the row, or emit the key.",
        r.join("\n  ")
    );
}
