#![forbid(unsafe_code)]

//! **Derive-from-tools meta-gates** (plan §18 item 40; AGENTS §3's closing rule).
//!
//! Four rosters in this tree exist twice: once as the code, manifest or registry
//! that *is* the thing, and once as a list a human typed — a Markdown table, a
//! module-doc parenthetical, a `[[bin]]` block. A list typed once is correct once.
//! AGENTS §3 states the doctrine for the one instance that already bit ("CI
//! enumerates loops from tools (`cargo fuzz list`), never hand-kept lists"), and
//! plan §18 item 40 names the four pairs still kept by hand:
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
//!   **Two ungated enumerations remain, said rather than implied:**
//!   `expectations/{linux,macos}.jq` each carry a per-probe `.id == "PN"` clause, so
//!   a probe with no clause is simply ungated there. That is a gate-coverage
//!   question rather than a roster-drift one, and it is not this gate's.
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
