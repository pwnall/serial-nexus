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
//! * **(g)** [`every_status_table_row_has_the_headers_cell_count`] — the plan's
//!   Status table, checked for being a *table* rather than for what it says. Plan §3
//!   rule 19 makes it the single home of every current-era measured figure and every
//!   other page cites it, so its columns are load-bearing: a figure whose Scope has
//!   slid one column left is a figure quoted with the wrong scope. It landed on a row
//!   carrying **six** pipe-delimited cells against a five-column header — a landing
//!   replaced the row and left the previous row's Caveat behind — and that row had
//!   been there unseen because GitHub-flavoured Markdown drops cells past the header
//!   count without a mark of any kind. The rendered table looked right (plan §18
//!   item 94).
//!
//!   This one reads no second roster, which is the difference: (a)–(f) compare a
//!   document against the code that is the authority, while (g) has no other side to
//!   compare against — the table *is* the authority. What it checks instead is that
//!   the authority parses as what it claims to be, and it is the same doctrine one
//!   layer down: a malformed row is a figure the reader cannot see, in a table whose
//!   entire purpose is that figures are quotable only from it.
//!
//!   **Its own first pass was three of AGENTS §3's registers at once**, and the
//!   parser is where all three lived. The row walk ended at the first line that did
//!   not start with `|`, which is not where GFM ends a table: leading pipes are
//!   optional, so a row that dropped one is still a row and so is every row below it
//!   — a de-piped row in the last third hid eleven of thirty-three rows, and a
//!   surplus cell planted below it drew nothing. The delimiter check compared *cell
//!   counts*, a test any data row passes, while its message claimed GFM "renders no
//!   table at all" — so deleting the `|---|` row outright left it green. And the
//!   evidence that the walk had covered the table was a hand-set floor of twenty
//!   against thirty-odd rows, which is thirteen rows of slack in exactly the
//!   direction that hides a truncated walk. All three now read the document instead:
//!   the extent is GFM's ([`ends_gfm_table`]), the delimiter row must *be* a
//!   delimiter row ([`is_delimiter_cell`]), and the row count is derived from the
//!   document rather than floored ([`non_blank_run_below_delimiter`]). The first two
//!   are differential against two reference renderers — `marked@15` and `micromark`
//!   with `micromark-extension-gfm-table` — over 116 row-extent inputs and 12
//!   delimiter spellings, because a parser gate that guesses at the parser is just
//!   the hand-kept list again, one level down.
//!
//!   **Its second pass carried two more of the same registers, and a mutation on
//!   this document proved each.** The first: a blank — or whitespace-only — line
//!   anywhere inside the table hid every row below it, silently. [`ends_gfm_table`]
//!   stops at a blank and so did the derived expectation
//!   ([`non_blank_run_below_delimiter`]), so the walk and its own yardstick agreed
//!   *by construction* and the extent check could not fire for that shape at all: a
//!   blank planted before line 50 printed `ok`, and so did the same blank with a
//!   surplus cell planted below it — while that surplus cell on its own reddens.
//!   Measured on this very table with both reference renderers, which agree: a blank
//!   after body row 17 renders as **one table of 17 rows plus a paragraph**, so
//!   sixteen of the thirty-three authority rows stop being in a table at all. That
//!   makes the interruption *itself* the defect rather than a reason to check fewer
//!   rows — plan §3 rule 19's authority surface is one table — so the expectation is
//!   now derived from the run of **row-shaped** lines below the delimiter
//!   ([`row_shaped_surface`]), which steps over exactly the interruptions the walk
//!   stops at and reports every one it steps over. Two expectations now bracket the
//!   walk: the non-blank run is blind to a blank line — it stops on the very one the
//!   walk stops on — while the surface is blind to neither shape, so the surface is
//!   the one that reports an interruption. The run is kept because it stops for a
//!   *different* reason than the walk does, and two rules that stop for different
//!   reasons are evidence where one rule is a restatement.
//!
//!   The second: [`locate_table`] takes the **first** line carrying the needle, so a
//!   second table under the same header aimed the entire gate at the decoy and left
//!   the real table unread — a nine-row decoy above line 30 plus a surplus cell on
//!   line 60 printed `ok`. The needle must now occur exactly once
//!   ([`lines_containing`]). **Why (g) alone needs that**: (a)–(f) compare a document
//!   against a roster the code owns, so a decoy that is not the real table drifts and
//!   reddens on its own contents; (g) has nothing to disagree with, and a well-formed
//!   decoy is *ideal* by every check it makes. The only defence is that there be one
//!   table. Both renderers were asked what a decoy is, too: with a blank line between
//!   them it is two tables (9 rows and 33), and with no blank it is **one** table of
//!   44 rows — which is why the surface walk reads a header-plus-delimiter pair as
//!   the next table only when a block boundary precedes it ([`table_starts_at`]).
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

/// The length of `text`'s run of trailing backslashes.
///
/// The parity of that run is what decides whether the character after it is escaped:
/// an odd run ends in a backslash that escapes, an even one in a backslash that was
/// itself escaped. [`split_cells`] needs it to tell a row's closing pipe from a pipe
/// that is content.
fn trailing_backslashes(text: &str) -> usize {
    text.chars().rev().take_while(|c| *c == '\\').count()
}

/// One Markdown table row split into its cells, **the way GitHub splits it**:
/// outer pipes dropped, cells trimmed, and an escaped `\|` kept as content.
///
/// GFM's table extension divides a row on the pipe *before* any inline parsing
/// runs, so a pipe inside a code span opens a new cell exactly like a bare one; the
/// only pipe that is content is an escaped one ("include a pipe in a cell's content
/// by escaping it, including inside other inline spans"). This tree's tables are
/// written both ways — `` `POLLIN\|POLLHUP` `` escapes, `` `IXON|IXOFF` `` does not
/// — and the difference is a rendering difference, not a taste one. Counting GFM's
/// way rather than "protecting" code spans is what keeps
/// [`every_status_table_row_has_the_headers_cell_count`] able to see the defect it
/// exists for: a cell GitHub silently drops.
fn split_cells(row: &str) -> Vec<String> {
    let trimmed = row.trim();
    let body = trimmed.strip_prefix('|').unwrap_or(trimmed);
    // Only an *unescaped* trailing pipe is the row's closing delimiter, and what
    // decides that is the **parity** of the backslash run in front of it, never the
    // presence of one: `b\|` ends in an escaped pipe, which is content, while
    // `b\\|` ends in an escaped *backslash* followed by the closing delimiter.
    // Testing `ends_with('\\')` cannot tell those apart and counts `| a | b\\|` as
    // three cells where GitHub renders two — a false alarm on a correct row, which
    // is the one direction a gate may not fail in (measured against `marked@15`).
    let body = match body.strip_suffix('|') {
        Some(inner) if trailing_backslashes(inner) % 2 == 1 => body,
        Some(inner) => inner,
        None => body,
    };
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in body.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == '|' {
            cells.push(std::mem::take(&mut cur).trim().to_owned());
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_owned());
    cells
}

/// Does `line` end a GFM table's run of rows?
///
/// GFM's table extension keeps taking rows until "the first empty line or beginning
/// of another block-level structure" — **not** until the first line without a
/// leading pipe. Leading and trailing pipes are optional on every row, so a line
/// that dropped one is still a row, and so is every row after it. Reading the extent
/// as "lines that start with `|`" is how a de-piped row can hide every row below
/// itself from [`every_status_table_row_has_the_headers_cell_count`]: measured on
/// this tree's own Status table, a row de-piped in the last third left the walk
/// covering 22 rows of 33, and a surplus cell planted below it drew no offence at
/// all.
///
/// Ground truth is two reference renderers, run differentially: `marked@15` — the
/// stand-in for GitHub's cmark-gfm that a developer can run in one command — and
/// `micromark` with `micromark-extension-gfm-table`, the spec-faithful one. Both were
/// asked the same question for 116 values of `X` in
/// `| abc | def |\n| --- | --- |\n| r1 | x |\nX\n| r3 | z |`: is `| r3 | z |` still a
/// row? The 116 are this table's 33 rows, each of them again with its leading pipe
/// dropped, and 50 adversarial shapes. **This function agrees with micromark on all
/// 116.** The renderers disagree with each other on 7, and `marked` is the outlier on
/// every one — it keeps as a row a tab-only line, a bare `-`, `+` or `*`, an ordered
/// marker that does not start at 1, and an HTML type-7 open tag, each of which
/// CommonMark says begins a block. None of the seven can occur in a Status row, and
/// either direction of a disagreement reddens this gate rather than silencing it: a
/// walk that stops early fails the derived extent check, and a walk that runs long
/// reports a one-cell "row".
///
/// What the agreed arms say: a blank line, an ATX heading, a blockquote, a list
/// marker, a fence, a thematic break, an HTML block and a four-space indent each end
/// the table — the lines below them render as a paragraph of pipes. A pipe-less line,
/// a prose line and a setext underline (`===`, which needs a paragraph to underline)
/// are rows.
fn ends_gfm_table(line: &str) -> bool {
    let mut indent = 0usize;
    for c in line.chars() {
        match c {
            ' ' => indent += 1,
            '\t' => indent += 4,
            _ => break,
        }
    }
    // Four columns of indent opens an indented code block, which ends the table.
    if indent >= 4 {
        return true;
    }
    // Trimmed at both ends: the leading run is already accounted for by `indent`, and
    // a trailing `\r` from a CRLF document must not make a thematic break look like
    // prose.
    let rest = line.trim();
    let bytes = rest.as_bytes();
    let Some(first) = bytes.first().copied() else {
        // The blank line: the one terminator every table in this tree actually uses.
        return true;
    };
    // A run of one character, three or more of it, and nothing else but spaces.
    let thematic_break = |c: u8| {
        rest.bytes().filter(|b| *b == c).count() >= 3
            && rest.bytes().all(|b| b == c || b == b' ' || b == b'\t')
    };
    // `- `, `* `, `+ `, `1. `, `1) ` — the marker, then a space or the line's end.
    let after_marker_is_space = |at: usize| bytes.get(at).is_none_or(|b| *b == b' ' || *b == b'\t');
    let ordered_marker = || {
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        (1..=9).contains(&digits)
            && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
            && after_marker_is_space(digits + 1)
    };
    match first {
        b'>' => true,
        b'#' => {
            let hashes = rest.bytes().take_while(|b| *b == b'#').count();
            (1..=6).contains(&hashes) && after_marker_is_space(hashes)
        }
        b'`' => rest.starts_with("```"),
        b'~' => rest.starts_with("~~~"),
        // An HTML block start, approximated by its opening delimiter: a `<` at the
        // head of a Status row would be a defect on its own terms.
        b'<' => {
            matches!(bytes.get(1), Some(b) if b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?'))
        }
        b'-' | b'*' | b'_' => thematic_break(first) || (first != b'_' && after_marker_is_space(1)),
        b'+' => after_marker_is_space(1),
        b'0'..=b'9' => ordered_marker(),
        _ => false,
    }
}

/// A Markdown table located by a needle in its header line, with the **line
/// numbers** a failure message needs to send a reader to the offending row.
///
/// Line numbers are 1-based, matching what an editor and `sed -n 'Np'` say. The
/// raw row text is carried beside the cells for the same reason: a gate that
/// reports "six cells against five" without the row costs the next reader the hour
/// it took to find it.
struct MarkdownTable {
    /// The 1-based line the header sits on.
    header_line: usize,
    /// The header row's own cells.
    header: Vec<String>,
    /// The `|---|` row's cells. GFM renders a table only when this row is a
    /// *delimiter* row — every cell a run of dashes with optional colons — and its
    /// cell count matches the header's, so it is part of the structure and not
    /// decoration ([`is_delimiter_cell`]).
    delimiter: Vec<String>,
    /// `(1-based line number, raw line, cells)` for each body row.
    rows: Vec<(usize, String, Vec<String>)>,
    /// The 1-based line that ended the run of rows, or one past the last line of the
    /// document if the table ran to the end of it. Carried so a gate can say *where*
    /// its walk stopped, and check that it stopped where the table does.
    end_line: usize,
    /// The line at [`MarkdownTable::end_line`], or `None` at end of document. A walk
    /// that stopped on a line worth printing is a walk worth doubting.
    terminator: Option<String>,
}

/// The Markdown table whose header line contains `header_needle`; `None` if no line
/// does.
///
/// The needle is a **locator**, not a roster: if a table is retitled the lookup
/// answers `None` and the caller must fail on the missing table rather than compare
/// against an empty set, which is the failure this whole file is about. Rows run
/// from the delimiter line to the line that ends the table block — a blank line or
/// the start of another block, which is GFM's rule and not "the first line without a
/// pipe" ([`ends_gfm_table`]).
fn locate_table(md: &str, header_needle: &str) -> Option<MarkdownTable> {
    let lines: Vec<&str> = md.lines().collect();
    let header = lines.iter().position(|l| l.contains(header_needle))?;
    let delimiter = lines
        .get(header + 1)
        .map(|l| split_cells(l))
        .unwrap_or_default();
    // The `|---|` separator sits immediately below the header; rows follow it.
    let mut rows = Vec::new();
    let mut end_line = lines.len() + 1;
    let mut terminator = None;
    for (index, line) in lines.iter().enumerate().skip(header + 2) {
        if ends_gfm_table(line) {
            end_line = index + 1;
            terminator = Some((*line).to_owned());
            break;
        }
        rows.push((index + 1, (*line).to_owned(), split_cells(line)));
    }
    Some(MarkdownTable {
        header_line: header + 1,
        header: split_cells(lines[header]),
        delimiter,
        rows,
        end_line,
        terminator,
    })
}

/// The data rows of the Markdown table whose header line contains `header_needle`,
/// each row split into its cells.
///
/// The cell view of [`locate_table`], for the gates that want a roster rather than a
/// position. One walker, two views: a table that parses differently for one gate
/// than for another is a second copy of the parser, which is the thing this file is
/// about.
fn table_rows(md: &str, header_needle: &str) -> Vec<Vec<String>> {
    locate_table(md, header_needle)
        .map(|t| t.rows.into_iter().map(|(_, _, cells)| cells).collect())
        .unwrap_or_default()
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

// ---------------------------------------------------------------------------
// (g) The plan's Status table is structurally a table
// ---------------------------------------------------------------------------

/// The header line of the plan's Status table, verbatim.
///
/// A **locator**, not a roster (see [`locate_table`]): if §3 rule 19's table is ever
/// retitled this needle stops matching, [`locate_table`] answers `None`, and the
/// test below fails on the missing table rather than passing over an empty row set.
/// That is AGENTS §3's named tell — "a gate whose passing output is identical to its
/// not-running output" — and this gate's whole subject is a defect that survived
/// because it was invisible, so it may not fail invisibly itself.
const STATUS_TABLE_HEADER: &str = "| Figure | Scope | Date | Commit / record | Caveat |";

/// `text`'s first `chars` characters, with an ellipsis if it was cut.
///
/// Counted in `char`s rather than bytes: the Status table's cells are full of `§`,
/// `·` and `—`, and a byte slice through one panics the gate it is trying to
/// explain.
fn preview(text: &str, chars: usize) -> String {
    let mut out: String = text.chars().take(chars).collect();
    if text.chars().count() > chars {
        out.push('…');
    }
    out
}

/// `md` with line `line_no` (1-based) replaced by `replacement`.
///
/// Plants land by line number rather than through `str::replace`, because two Status
/// rows can share a long prefix and a row's text can appear again in the prose
/// around the table: a plant that landed on a line other than the one it names
/// proves something about a line nobody chose.
fn replace_line(md: &str, line_no: usize, replacement: &str) -> String {
    let mut lines: Vec<&str> = md.lines().collect();
    assert!(
        (1..=lines.len()).contains(&line_no),
        "line {line_no} is outside the {} line(s) of the document being planted in",
        lines.len()
    );
    lines[line_no - 1] = replacement;
    let mut out = lines.join("\n");
    if md.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// `md` with `text` inserted **before** line `line_no` (1-based), shifting every
/// line from there down by one. `text` may itself carry newlines, which is how a
/// whole decoy table is planted in one call.
///
/// The insertion plants are the ones the line-number arithmetic has to be right for:
/// a row that sat on line `L` sits on `L + 1` afterwards, and a plant that landed one
/// row off would prove something about a row nobody chose (see [`replace_line`]).
fn insert_before(md: &str, line_no: usize, text: &str) -> String {
    let mut lines: Vec<&str> = md.lines().collect();
    assert!(
        (1..=lines.len() + 1).contains(&line_no),
        "line {line_no} is outside the {} line(s) of the document being planted in",
        lines.len()
    );
    lines.insert(line_no - 1, text);
    let mut out = lines.join("\n");
    if md.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// `md` with line `line_no` (1-based) deleted.
fn delete_line(md: &str, line_no: usize) -> String {
    let mut lines: Vec<&str> = md.lines().collect();
    assert!(
        (1..=lines.len()).contains(&line_no),
        "line {line_no} is outside the {} line(s) of the document being planted in",
        lines.len()
    );
    lines.remove(line_no - 1);
    let mut out = lines.join("\n");
    if md.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Is `cell` a GFM **delimiter** cell — a run of one or more dashes, with an
/// optional colon at either end?
///
/// This shape is what makes a table a table, and the cell *count* of that row is not
/// it. Measured against both `marked@15` and `micromark` + `micromark-extension-gfm-table`,
/// which agree on all twelve spellings the gate below checks:
/// `| a | b |\n| - - | --- |\n| r1 | x |` renders as one paragraph of pipes and no
/// table at all, and so does the same document with the `|---|` line deleted outright
/// — in which case the first *data* row is what sits under the header, and a check
/// that only counts cells sees nothing wrong, because a data row has exactly the
/// header's count. `-`, `---`, `:--`, `--:` and `:-:` are the accepted spellings; a
/// bare `:`, an empty cell and anything with a character other than a dash in it are
/// not.
fn is_delimiter_cell(cell: &str) -> bool {
    let body = cell.trim();
    let body = body.strip_prefix(':').unwrap_or(body);
    let body = body.strip_suffix(':').unwrap_or(body);
    !body.is_empty() && body.bytes().all(|b| b == b'-')
}

/// Everything wrong with `table`'s delimiter row: a cell count that does not match
/// the header's, and any cell that is not a delimiter cell.
///
/// Both are the same defect — GFM renders no table — and both are invisible in a
/// rendered document, because what the reader sees is the paragraph of pipes that
/// replaced the table, and a paragraph of pipes is exactly what a reader skims past.
fn delimiter_offences(table: &MarkdownTable) -> Vec<String> {
    let mut out = Vec::new();
    if table.delimiter.len() != table.header.len() {
        out.push(format!(
            "{} cell(s) against the header's {}",
            table.delimiter.len(),
            table.header.len()
        ));
    }
    for (index, cell) in table.delimiter.iter().enumerate() {
        if !is_delimiter_cell(cell) {
            out.push(format!(
                "cell {}: {:?} is not a delimiter cell (`-`, `---`, `:--`, `--:`, `:-:`)",
                index + 1,
                preview(cell, 72)
            ));
        }
    }
    out
}

/// How many non-blank lines sit directly below `table`'s delimiter row in `md`.
///
/// The **derived** expectation for the row walk, and deliberately computed by a
/// simpler rule than the walk's own: [`ends_gfm_table`] stops at a blank line *or* at
/// a new block, this stops only at a blank line. The two agreeing is therefore
/// evidence — the walk reached the table's real end — rather than a restatement of
/// the walk in its own terms. A hand-set floor could not say that: against thirty-odd
/// rows, a floor of twenty let thirteen of them leave the gate's scope with a passing
/// output identical to a clean run.
fn non_blank_run_below_delimiter(md: &str, table: &MarkdownTable) -> usize {
    md.lines()
        .skip(table.header_line + 1)
        .take_while(|line| !line.trim().is_empty())
        .count()
}

/// The 1-based line numbers of every line in `md` containing `needle`.
///
/// [`locate_table`] takes the **first** of them, which is the right answer only when
/// there is one. A second table under the same header is not a drifted roster and not
/// a malformed row — it is a perfectly-formed table that the locator prefers, and
/// every check downstream then reports on a table nobody cites. Measured on this
/// document: a nine-row decoy carrying this header above line 30, plus a surplus cell
/// on line 60 of the real table, printed `ok`.
///
/// Gates (a)–(f) do not need this: each compares a document against a roster the code
/// owns, so a decoy that is not the real table disagrees with the code and reddens.
/// (g) has no second side — a well-formed decoy satisfies every check it makes — so
/// uniqueness of the needle *is* the check.
fn lines_containing(md: &str, needle: &str) -> Vec<usize> {
    md.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(index, _)| index + 1)
        .collect()
}

/// Is `line` a GFM **delimiter row** — two or more cells, every one of them a
/// delimiter cell ([`is_delimiter_cell`])?
///
/// A row of the table's *frame* rather than its body. Used to tell the next table's
/// frame from this table's rows; a thematic break (`---`, one cell) is not one.
fn is_delimiter_row(line: &str) -> bool {
    let cells = split_cells(line);
    cells.len() >= 2 && cells.iter().all(|cell| is_delimiter_cell(cell))
}

/// Is `line` shaped like a body row of the Status table — three or more cells, and
/// not a line GFM would treat as ending the table ([`ends_gfm_table`])?
///
/// **Three cells, not two**, and the floor is what keeps the surface walk from
/// running past the table: the paragraph under this table is prose, and prose in this
/// tree carries stray pipes (`` `IXON|IXOFF` ``) often enough that one pipe cannot
/// mean "row". A Status row carries five, and even the short-row defect leaves four,
/// so nothing real sits between the two thresholds. A row that dropped its leading
/// pipe is still row-shaped, which matters: it is a row to GFM, and a surface walk
/// that called it prose would report the rows below it as unreachable.
fn is_row_shaped(line: &str) -> bool {
    !ends_gfm_table(line) && split_cells(line).len() >= 3
}

/// Does a table **begin** at `lines[index]` — a row-shaped line with a delimiter row
/// directly beneath it, which is GFM's own two-line signature for a table's frame?
///
/// Only ever asked after a block boundary, because that is the only place the answer
/// is yes. Measured on both reference renderers with this document's table: a nine-row
/// table, a blank line, then this table renders as **two** tables (9 rows and 33),
/// while the same pair with the blank line removed renders as **one** table of 44 rows
/// — the second header and its `|---|` become ordinary body rows. So a header-plus-
/// delimiter pair inside an uninterrupted run of rows is not a new table, and the
/// surface walk must not treat it as one.
fn table_starts_at(lines: &[&str], index: usize) -> bool {
    lines.get(index).is_some_and(|line| is_row_shaped(line))
        && lines
            .get(index + 1)
            .is_some_and(|line| is_delimiter_row(line))
}

/// Is `line` an **ATX heading** — up to three columns of indent, then one to six
/// `#`, then a space or the line's end?
///
/// [`ends_gfm_table`] already answers the wider question — does this line open a
/// block — which every heading does and so does a blank line, a list marker and a
/// fence. This asks the narrow one, because only a heading bounds a *section*, and
/// the section is what [`row_shaped_surface`] measures its extent against.
fn is_atx_heading(line: &str) -> bool {
    let rest = line.trim_start();
    if line.len() - rest.len() >= 4 {
        return false;
    }
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes)
        && rest
            .as_bytes()
            .get(hashes)
            .is_none_or(|b| *b == b' ' || *b == b'\t')
}

/// The 1-based line the section holding the table headed at `header_line` ends on:
/// the first ATX heading below it that does **not** have row-shaped lines between it
/// and the heading after it, or one past the last line of the document.
///
/// Why not simply "the next heading". A heading is a terminator, so a heading
/// planted *inside* a table is byte-for-byte the shape of the heading that ends the
/// section — and taking the first one would let the very interruption this walk
/// exists to report choose the window it is reported in. That is AGENTS §3's tell
/// one level down: the yardstick moves with the defect, so a passing output is
/// identical to a not-running one. Measured on this document: `""`, `"#### Retired
/// figures"`, `""`, a sentence and `""` planted before line 50 printed `ok` against
/// a surface bounded by the first heading below the table, with seventeen of the
/// thirty-three authority rows outside it. A heading with rows below it is therefore
/// read as an interruption; the first one without them ends the section.
///
/// The limit, stated because it is the obvious over-read: `#` inside a fenced code
/// block reads as a heading here. Nothing between this table and its section's end
/// is fenced, and the direction of that error is a section that ends *early*, which
/// this rule then rejects for the rows below it.
fn section_end_below(lines: &[&str], header_line: usize) -> usize {
    let headings: Vec<usize> = (header_line..lines.len())
        .filter(|index| is_atx_heading(lines[*index]))
        .collect();
    for (nth, heading) in headings.iter().enumerate() {
        let next = headings.get(nth + 1).copied().unwrap_or(lines.len());
        if !lines[heading + 1..next].iter().any(|l| is_row_shaped(l)) {
            return heading + 1;
        }
    }
    lines.len() + 1
}

/// Is any line row-shaped in `from ..` up to the end of the section that ends on the
/// 1-based line `section_end`?
///
/// The question that tells the table's real end from an interruption: both are a
/// line the table cannot continue through, and only "do the rows come back"
/// separates them.
fn row_shaped_below(lines: &[&str], from: usize, section_end: usize) -> bool {
    let stop = (section_end - 1).min(lines.len());
    from < stop && lines[from..stop].iter().any(|l| is_row_shaped(l))
}

/// The run of **row-shaped** lines below `table`'s delimiter, and every interruption
/// stepped over on the way.
///
/// The expectation an interruption cannot move, and the reason it is needed:
/// [`non_blank_run_below_delimiter`] stops at a blank line and so does the walk
/// ([`ends_gfm_table`]), so for a *blank* interruption the derived expectation and the
/// walk agree by construction and the extent check cannot fire — a passing output
/// identical to a clean run, which is AGENTS §3's own tell. This walk instead steps
/// over every line the table cannot continue through and keeps counting, so a
/// truncation moves the walk and leaves this count where it was.
///
/// **Where it stops is derived from the enclosing section, not from the first line
/// that is not a row** ([`section_end_below`]). Taking the first such line was a
/// third truncator and it hid the same defect in a third way: a terminator run
/// followed by anything the walk also stops on — ordinary prose, a planted heading —
/// ended the surface *inside* the table, the pending run was dropped, and all three
/// counters then agreed on the truncated count. Measured on this document, each
/// printing `ok` with seventeen authority rows outside every check below: a blank
/// plus a sentence plus a blank before line 50; the same without the trailing blank;
/// and the same with a `####` heading in it. A non-row line ends the surface only
/// when no row-shaped line follows it before the section does, so the pending run at
/// the stop is provably empty — every terminator with a row under it has already
/// been reported.
///
/// It still stops at the frame of the *next* table ([`table_starts_at`]), because
/// both reference renderers read a header-plus-delimiter pair below a block boundary
/// as a second table and this file does not get to disagree with them. That stop is
/// **reported** rather than silent ([`RowSurface::stopped_at_frame`]): GFM is right
/// that those are two tables, and for the authority surface that is the defect
/// rather than the answer.
struct RowSurface {
    /// Row-shaped lines below the delimiter, interruptions ignored.
    rows: usize,
    /// `(1-based line number, raw line)` for each interruption inside the run.
    interruptions: Vec<(usize, String)>,
    /// The 1-based line the surface walk stopped on, or one past the document.
    end_line: usize,
    /// The line at [`RowSurface::end_line`], or `None` at end of document.
    terminator: Option<String>,
    /// The walk stopped on the frame of another table rather than on the end of the
    /// table's own run — so the rows below it are in a second table.
    stopped_at_frame: bool,
}

fn row_shaped_surface(md: &str, table: &MarkdownTable) -> RowSurface {
    let lines: Vec<&str> = md.lines().collect();
    let section_end = section_end_below(&lines, table.header_line);
    let mut rows = 0usize;
    let mut pending: Vec<(usize, String)> = Vec::new();
    let mut interruptions: Vec<(usize, String)> = Vec::new();
    let mut end_line = lines.len() + 1;
    let mut terminator = None;
    let mut stopped_at_frame = false;
    for (index, line) in lines.iter().enumerate().skip(table.header_line + 1) {
        // Past the enclosing section the rows belong to another surface, whatever
        // they look like.
        if index + 1 >= section_end {
            end_line = index + 1;
            terminator = Some((*line).to_owned());
            break;
        }
        // A table frame below a block boundary is the next table, not this one's
        // continuation.
        if !pending.is_empty() && table_starts_at(&lines, index) {
            end_line = index + 1;
            terminator = Some((*line).to_owned());
            stopped_at_frame = true;
            break;
        }
        if is_row_shaped(line) {
            rows += 1;
            interruptions.append(&mut pending);
            continue;
        }
        // The table's real end: a line it cannot continue through, with no row-shaped
        // line under it before the section ends. Anything else is an interruption,
        // and is held until the rows that prove it was one arrive.
        if !row_shaped_below(&lines, index + 1, section_end) {
            end_line = index + 1;
            terminator = Some((*line).to_owned());
            break;
        }
        pending.push((index + 1, (*line).to_owned()));
    }
    RowSurface {
        rows,
        interruptions,
        end_line,
        terminator,
        stopped_at_frame,
    }
}

/// Every body row of `table` whose cell count differs from the header's, reported
/// with its line number, both counts, its cells and the row itself.
///
/// Both directions are defects and they are different ones. A row with **more**
/// cells than the header is silently truncated by GitHub — the surplus cell renders
/// nowhere, which is exactly why a stray sixth cell can sit in the authority table
/// for a week unseen. A row with **fewer** shifts every column after the gap, so a
/// scope is read as a date and a caveat as a commit.
fn cell_count_offences(table: &MarkdownTable) -> Vec<String> {
    let expected = table.header.len();
    let mut out = Vec::new();
    for (line, raw, cells) in &table.rows {
        if cells.len() == expected {
            continue;
        }
        let cells_seen = cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("      cell {}: {}", i + 1, preview(c, 72)))
            .collect::<Vec<_>>()
            .join("\n");
        out.push(format!(
            "line {line}: {found} cell(s) against the header's {expected}\n\
             {cells_seen}\n      row: {raw}",
            found = cells.len(),
        ));
    }
    out
}

#[test]
fn every_status_table_row_has_the_headers_cell_count() {
    let root = repo_root();

    // 0. The matchers — the cell splitter, the row-extent rule and the delimiter-cell
    //    shape — in each spelling this table is written in and in the GFM edge cases
    //    that decide what a cell, a row and a table *are*. Ground truth throughout is
    //    `marked@15` over the fixture each message names; GitHub renders with
    //    cmark-gfm, and this is the stand-in for it a developer can run.
    assert_eq!(
        split_cells("| a | b | c | d | e |").len(),
        5,
        "the splitter cannot count a well-formed five-column row, so every verdict \
         below is about its own arithmetic"
    );
    assert_eq!(
        split_cells("| a | b | c | d | e | f |").len(),
        6,
        "the splitter does not see a surplus cell — the exact defect this gate \
         exists for, and the one GitHub renders as if it were not there"
    );
    assert_eq!(
        split_cells("| a | b | c | d |").len(),
        4,
        "the splitter does not see a missing cell — the mirror defect, which shifts \
         every column after the gap"
    );
    assert_eq!(
        split_cells("|  spaced  |  cells  |"),
        vec!["spaced".to_owned(), "cells".to_owned()],
        "the splitter does not trim cells, so a row's content would never compare \
         equal to anything"
    );
    assert_eq!(
        split_cells(r"| a | TIOCMGET \|\| TIOCGICOUNT | c |"),
        vec![
            "a".to_owned(),
            r"TIOCMGET \|\| TIOCGICOUNT".to_owned(),
            "c".to_owned()
        ],
        "an **escaped** pipe is read as a cell boundary. GFM says the escape is the \
         one way to put a pipe in a cell, so this row renders as three cells and \
         this gate would redden a table that is correct — a false alarm is how a \
         gate gets deleted"
    );
    assert_eq!(
        split_cells("| a | `IXON|IXOFF` | c |").len(),
        4,
        "a **bare** pipe inside a code span is read as content. GFM divides a row \
         before any inline parsing runs, so GitHub opens a fourth cell here and \
         drops it against a three-column header — a splitter that 'protected' code \
         spans would be blind to precisely the silently-dropped cell this gate is \
         about"
    );
    assert_eq!(
        split_cells(r"| a | b\|").len(),
        2,
        "an escaped pipe at the end of a row was taken for the closing delimiter. \
         The backslash run before it is odd, so the pipe is content: `marked@15` \
         renders `b|` as the second of two cells, and the row simply carries no \
         trailing delimiter"
    );
    assert_eq!(
        split_cells(r"| a | b\\|").len(),
        2,
        "a closing pipe behind an **escaped backslash** was taken for content. The \
         run before it is even, so the backslash is the escaped thing and the pipe \
         closes the row: `marked@15` renders two cells, and answering three reddens \
         a correct row. Presence of a backslash is not the question; parity is"
    );

    for (line, ends, why) in [
        (
            "",
            true,
            "a blank line is the terminator every table here uses",
        ),
        ("   ", true, "a whitespace-only line is a blank line"),
        (
            "\t",
            true,
            "a tab-only line is blank too, so CommonMark and `micromark` end the \
             table here — `marked@15` is the outlier that keeps it as an empty row, \
             and this rule follows the spec",
        ),
        ("## §3 Gates", true, "an ATX heading opens a new block"),
        ("> quoted", true, "a blockquote opens a new block"),
        ("- item", true, "a bullet list marker opens a new block"),
        ("1. item", true, "an ordered list marker opens a new block"),
        ("```", true, "a fence opens a new block"),
        ("***", true, "a thematic break opens a new block"),
        (
            "---",
            true,
            "a thematic break spelled in dashes still opens one",
        ),
        ("<div>", true, "an HTML block opens a new block"),
        (
            "    indented four",
            true,
            "four columns of indent opens code",
        ),
        (
            "| **1004 passing · 0 failed · 7 ignored** | Linux | 2026-08-14 | `x` | — |",
            false,
            "an ordinary Status row is a row",
        ),
        (
            "**967 passing · 0 failed · 7 ignored**, seven self-skips | Linux | d | c | — |",
            false,
            "a row that dropped its leading pipe is still a row, and so is every row \
             below it — the defect this rewrite exists for: reading the extent as \
             'lines starting with a pipe' hid 11 of this table's 33 rows, and a \
             surplus cell planted below the de-piped row drew no offence",
        ),
        (
            "===",
            false,
            "a setext underline needs a paragraph to underline; inside a table it is \
             a row",
        ),
        (
            "plain prose",
            false,
            "a pipe-less prose line is a row with one cell",
        ),
        (
            "**A busy console's cost: 78 % → 36 %** | Linux | d | c | — |",
            false,
            "a row opening in bold is not a thematic break and not a list marker",
        ),
    ] {
        assert_eq!(
            ends_gfm_table(line),
            ends,
            "the row-extent rule disagrees with GFM's renderers on {line:?}: {why}"
        );
    }

    for (cell, is_delim) in [
        ("---", true),
        ("-", true),
        (":--", true),
        ("--:", true),
        (":-:", true),
        ("  ---  ", true),
        ("- -", false),
        ("", false),
        (":", false),
        ("--=", false),
        ("—", false),
        ("**1007 passing · 0 failed · 7 ignored**", false),
    ] {
        assert_eq!(
            is_delimiter_cell(cell),
            is_delim,
            "the delimiter-cell shape disagrees with GFM's renderers on {cell:?}, \
             which decides whether GitHub renders a table here at all — not how many \
             columns it has"
        );
    }

    for (line, shaped, why) in [
        (
            "| **1004 passing · 0 failed · 7 ignored** | Linux | 2026-08-14 | `x` | — |",
            true,
            "an ordinary Status row is row-shaped, so the surface walk can count it",
        ),
        (
            "**967 passing · 0 failed · 7 ignored**, seven self-skips | Linux | d | c | — |",
            true,
            "a row that dropped its leading pipe is a row to GFM, and a surface walk \
             that read it as prose would stop there and call every row below it \
             unreachable — the same truncation one layer over",
        ),
        ("| a | b | c |", true, "three cells is the shape's floor"),
        ("", false, "a blank line is an interruption, never a row"),
        (
            "   ",
            false,
            "and so is a whitespace-only one — both reference renderers end the \
             table on it, which is exactly the interruption that used to be honoured \
             in silence",
        ),
        (
            "## a heading in the table",
            false,
            "a heading opens a block, so it interrupts rather than continues",
        ),
        (
            "a sentence with one | pipe in it",
            false,
            "two cells is below the floor: a stray pipe in the paragraph under the \
             table must end the surface rather than extend it into the prose",
        ),
        (
            "Current measured figures live in the table below and nowhere else",
            false,
            "the prose that actually follows this table ends the surface",
        ),
    ] {
        assert_eq!(
            is_row_shaped(line),
            shaped,
            "the row-shape rule disagrees on {line:?}: {why}"
        );
    }
    assert!(
        is_delimiter_row("|---|---|---|---|---|"),
        "a delimiter row is not recognised as one, so the next table's frame reads \
         as this table's rows and the surface walk runs into it"
    );
    assert!(
        !is_delimiter_row("| a | b | c | d | e |"),
        "a data row is read as a delimiter row — the surface walk would stop at the \
         first row of the table it is measuring"
    );
    assert!(
        !is_delimiter_row("---"),
        "a thematic break is read as a delimiter row; it is one cell and no table's \
         frame"
    );

    // …and the surface walk itself, on synthetic documents small enough to read.
    // The first is the defect's own shape: an interrupted table, where the walk sees
    // one row and the surface sees both.
    let interrupted_fixture =
        "| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n\n| 4 | 5 | 6 |\n\nprose\n";
    let fixture_table = locate_table(interrupted_fixture, "| a | b | c |")
        .expect("the fixture carries a table under its own header");
    let fixture_surface = row_shaped_surface(interrupted_fixture, &fixture_table);
    assert_eq!(
        (
            fixture_table.rows.len(),
            fixture_surface.rows,
            fixture_surface.interruptions.len()
        ),
        (1, 2, 1),
        "on a table with one blank line inside it the walk must see 1 row, the \
         surface 2, and the interruption must be reported. Anything else and the \
         expectation moves with the truncation it exists to detect — which is the \
         state this gate shipped in"
    );
    // The second: two tables one blank line apart are two tables, not one
    // interrupted one. Both reference renderers agree, and a surface walk that
    // disagreed would redden every document that stacks tables.
    let stacked_fixture = "| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n\n| a | b | c |\n|---|---|---|\n| 4 | 5 | 6 |\n";
    let stacked_table = locate_table(stacked_fixture, "| a | b | c |")
        .expect("the fixture carries a table under its own header");
    let stacked_surface = row_shaped_surface(stacked_fixture, &stacked_table);
    assert_eq!(
        (stacked_surface.rows, stacked_surface.interruptions.len()),
        (1, 0),
        "the surface walk read the *next* table's rows as this one's, so a document \
         with two tables a blank line apart reddens on a defect it does not have — \
         and a gate that cries wolf on a correct document is a gate that gets deleted"
    );
    assert_eq!(
        lines_containing(stacked_fixture, "| a | b | c |"),
        vec![1, 5],
        "the needle counter cannot see two tables under one header, which is the \
         whole of the second repair"
    );

    // 1. The table, from the real document. A needle that stopped matching fails
    //    here, loudly, rather than yielding an empty row set to compare.
    let plan = read_tree_file(&root, PLAN);
    let header_lines = lines_containing(&plan, STATUS_TABLE_HEADER);
    assert_eq!(
        header_lines.len(),
        1,
        "{PLAN} carries {n} line(s) spelling the Status table's header, at {header_lines:?}. \
         [`locate_table`] takes the first of them, so with a second one every check \
         below reports on whichever table is higher up the page and the other goes \
         wholly unread — measured: a nine-row decoy under this header, planted above \
         the real table with a surplus cell in it, printed `ok`. Plan §3 rule 19 makes \
         *one* table the home of every current-era measured figure, so two tables under \
         one header is a defect in the document before it is one in this gate: give the \
         other table its own header, fold its rows into this one, or — if the line is \
         prose quoting the header rather than a second table — reword the quotation so \
         it is not the header line verbatim.",
        n = header_lines.len(),
    );
    let table = locate_table(&plan, STATUS_TABLE_HEADER).unwrap_or_else(|| {
        panic!(
            "{PLAN} carries no line containing {STATUS_TABLE_HEADER:?}. Plan §3 rule \
             19 makes the Status table the single home of every current-era measured \
             figure; if its header moved, this gate has to learn the new spelling \
             rather than be deleted, or nothing checks the authority surface again"
        )
    });

    // 2. The structure: the header the needle actually landed on, the delimiter row
    //    that makes the thing a table, and the extent of the walk. Each of the three
    //    is a way for every comparison below to be against nothing.
    assert_eq!(
        table.header,
        split_cells(STATUS_TABLE_HEADER),
        "the line at {} contains the header needle but does not split into the \
         needle's own cells — it carries {} cell(s), so the needle matched a longer \
         line and every column position below is off by whatever precedes it",
        table.header_line,
        table.header.len()
    );
    let delimiter_faults = delimiter_offences(&table);
    assert!(
        delimiter_faults.is_empty(),
        "the row under the Status table's header (line {}) is not a delimiter row:\n  \
         {}\n\
         GFM renders **no table at all** in that state — header, delimiter and every \
         figure below collapse into one paragraph of pipes. Counting its cells does \
         not detect this: a data row promoted into the delimiter's place has exactly \
         the header's count, and one cell corrupted to `- -` keeps it",
        table.header_line + 1,
        delimiter_faults.join("\n  "),
    );
    let expected_rows = non_blank_run_below_delimiter(&plan, &table);
    assert!(
        !table.rows.is_empty(),
        "the Status table at line {} parsed to no body rows at all, and a gate that \
         checks nothing prints what a clean table prints (AGENTS §3). Plan §3 rule 19 \
         makes this table the home of every current-era measured figure, so an empty \
         one is a defect in the document even before it is one in this gate",
        table.header_line
    );
    assert_eq!(
        table.rows.len(),
        expected_rows,
        "the row walk covered {} of the {} non-blank line(s) below the Status table's \
         delimiter: it stopped at line {} on {:?}. GFM ends a table at a blank line or \
         a new block, so a walk that stopped anywhere inside that run either read the \
         extent wrongly — a row without its leading pipe is still a row — or the table \
         really is interrupted, in which case the figures below the interruption are \
         not in a table at all and are not the quotable surface plan §3 rule 19 says \
         they are. This is the derived expectation that replaced a hand-set row floor, \
         which at 20 against 33 rows let a truncated walk print `ok`",
        table.rows.len(),
        expected_rows,
        table.end_line,
        table.terminator.as_deref().unwrap_or("end of document"),
    );

    // 2b. …and the same walk against an expectation the interruption **cannot
    //     move**. The check above cannot see a blank line planted inside the table,
    //     and not because it is written wrongly: [`ends_gfm_table`] stops at a blank
    //     and so does the non-blank run, so the walk and its yardstick agree by
    //     construction for that one shape. The surface walk steps over the same
    //     terminators and keeps counting — but only because its *end* is derived
    //     from the enclosing section rather than from the first line that is not a
    //     row ([`section_end_below`]). That distinction is the whole of this check:
    //     a terminator run followed by anything the walk also stops on — prose, a
    //     planted heading, the frame of another table — used to end the surface
    //     inside the table, and then all three counters agreed on the truncated
    //     count and this assert restated the walk instead of checking it. Now a
    //     non-row line ends the surface only when the rows do not come back before
    //     the section does, so a blank moves the walk and leaves this count where
    //     it was, whatever follows the blank.
    let surface = row_shaped_surface(&plan, &table);
    assert!(
        surface.interruptions.is_empty(),
        "the Status table is interrupted at line(s) {lines:?} — the run of rows below \
         its delimiter is broken by:\n  {report}\n\
         GFM ends a table at a blank line or a new block, so the rows *below* an \
         interruption are not in a table at all: measured on this table with \
         `marked@15` and `micromark` + `micromark-extension-gfm-table`, which agree, a \
         blank line after body row 17 renders as one table of 17 rows and a paragraph \
         of pipes — the other sixteen figures lose their columns entirely and nothing \
         about the page says so. Plan §3 rule 19 makes this table the single home of \
         every current-era measured figure, so the interruption is itself the defect \
         and not a reason to check fewer rows: close the gap, or move what is below it \
         under its own header.",
        lines = surface
            .interruptions
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        report = surface
            .interruptions
            .iter()
            .map(|(line, raw)| format!("line {line}: {:?}", preview(raw, 72)))
            .collect::<Vec<_>>()
            .join("\n  "),
    );
    assert!(
        !surface.stopped_at_frame,
        "the run of rows below the Status table's delimiter stops at line {} on {:?} \
         — the frame of a *second* table, one block boundary below the first. Both \
         reference renderers agree that is two tables and not one interrupted one, \
         and that is exactly the defect: the {} row(s) this walk reached are the \
         whole authority surface, and every figure under the second header is \
         outside it. Nothing else here can see this shape — the needle counter reads \
         the header line, so a second frame spelled differently is invisible to it, \
         and the walk and both its expectations agree on the truncated count. Plan \
         §3 rule 19 makes one table the home of every current-era measured figure: \
         fold the rows back in, or move them out of this section under their own \
         header",
        surface.end_line,
        surface.terminator.as_deref().unwrap_or("end of document"),
        surface.rows,
    );
    assert_eq!(
        table.rows.len(),
        surface.rows,
        "the row walk covered {} rows against {} row-shaped line(s) below the Status \
         table's delimiter: the walk stopped at line {} on {:?}, the surface at line \
         {} on {:?}. The two are derived by rules blind to different things — the walk \
         stops where GFM ends the table, the surface steps over every terminator — so \
         they differ only when the authority surface is broken into more than one \
         table, or when this file's parser has drifted from GFM's. Either way the \
         figures past the break are not the quotable surface plan §3 rule 19 says they \
         are",
        table.rows.len(),
        surface.rows,
        table.end_line,
        table.terminator.as_deref().unwrap_or("end of document"),
        surface.end_line,
        surface.terminator.as_deref().unwrap_or("end of document"),
    );

    // 3. Planted against the real document's own bytes, in every direction, before a
    //    clean verdict is trusted. The victims are well-formed rows taken from the
    //    table rather than named here, so these proofs cannot go stale as it grows —
    //    and they outlive the defect that motivated the gate: once the malformed row
    //    is repaired, these are what still prove the gate can see one.
    let (victim_line, victim_raw, victim_cells) = table
        .rows
        .iter()
        .find(|(_, _, cells)| cells.len() == table.header.len())
        .cloned()
        .expect("the Status table has at least one well-formed row to plant against");
    let named = |offences: &[String], line: usize| {
        offences
            .iter()
            .any(|m| m.starts_with(&format!("line {line}:")))
    };

    let short_row = format!("| {} |", victim_cells[..victim_cells.len() - 1].join(" | "));
    let short_plan = replace_line(&plan, victim_line, &short_row);
    assert_ne!(
        short_plan, plan,
        "the short row was never planted, so the proof below asserts nothing"
    );
    let short_table = locate_table(&short_plan, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    let short_offences = cell_count_offences(&short_table);
    assert!(
        named(&short_offences, victim_line),
        "a row with one cell **missing** produced no offence naming line \
         {victim_line} — the mirror defect, where every column after the gap is read \
         as the wrong column, passes this gate. Reported instead: {short_offences:?}"
    );

    let long_row = format!("{} an orphan cell |", victim_raw.trim_end());
    let long_plan = replace_line(&plan, victim_line, &long_row);
    assert_ne!(
        long_plan, plan,
        "the surplus cell was never planted, so the proof below asserts nothing"
    );
    let long_table = locate_table(&long_plan, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    let long_offences = cell_count_offences(&long_table);
    assert!(
        named(&long_offences, victim_line),
        "a row with one **surplus** cell produced no offence naming line \
         {victim_line} — which is the defect class plan §18 item 94 filed, and the \
         one GitHub hides by rendering the surplus nowhere. Reported instead: \
         {long_offences:?}"
    );

    // 3b. The plant that rewrote the walk, on this document's own bytes and in the
    //     shape that defeated the old one: a row in the table's **last third** loses
    //     its leading "| " — which GFM renders identically, so it is not itself an
    //     offence — and a row *below* it gains a surplus cell. Measured against the
    //     walk this replaced: 22 rows of 33 covered, zero offences reported, the row
    //     floor of 20 cleared, and the output identical to a clean run.
    let drop_leading_pipe = |raw: &str| {
        let trimmed = raw.trim_start();
        trimmed
            .strip_prefix('|')
            .unwrap_or(trimmed)
            .trim_start()
            .to_owned()
    };
    let rows = table.rows.len();
    let depipe_at = (rows * 2 / 3..rows - 1)
        .find(|i| {
            let (_, raw, cells) = &table.rows[*i];
            cells.len() == table.header.len() && !ends_gfm_table(&drop_leading_pipe(raw))
        })
        .expect("the Status table's last third holds a well-formed row above its last");
    let surplus_at = (depipe_at + 1..rows)
        .rev()
        .find(|i| table.rows[*i].2.len() == table.header.len())
        .expect("the Status table holds a well-formed row below the de-piped one");
    let (depipe_line, depipe_raw, _) = table.rows[depipe_at].clone();
    let (surplus_line, surplus_raw, _) = table.rows[surplus_at].clone();
    let truncating_plan = replace_line(&plan, depipe_line, &drop_leading_pipe(&depipe_raw));
    let truncating_plan = replace_line(
        &truncating_plan,
        surplus_line,
        &format!("{} an orphan cell |", surplus_raw.trim_end()),
    );
    let truncated = locate_table(&truncating_plan, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    assert_eq!(
        truncated.rows.len(),
        table.rows.len(),
        "line {depipe_line} lost its leading pipe (body row {} of {rows}) and the walk \
         shrank from {} rows to {} — GitHub renders that row exactly as before and \
         keeps every row below it, so a walk that stops there hands the last third of \
         the authority table to nobody",
        depipe_at + 1,
        table.rows.len(),
        truncated.rows.len(),
    );
    let truncated_offences = cell_count_offences(&truncated);
    assert!(
        named(&truncated_offences, surplus_line),
        "with line {depipe_line} de-piped, the surplus cell planted **below** it on \
         line {surplus_line} drew no offence: {truncated_offences:?}. That pair is \
         the mutation this gate failed before the walk was rewritten, and it failed \
         it silently — a passing output identical to a clean run"
    );
    assert!(
        !named(&truncated_offences, depipe_line),
        "the de-piped row on line {depipe_line} was itself reported as an offence. \
         Leading and trailing pipes are optional in GFM and `marked@15` renders that \
         row unchanged, so reddening on it is a false alarm on a correct document — \
         and a gate that cries wolf on the real table is a gate that gets deleted"
    );

    // 3c. The plant the walk's own terminator hides, and the reason step 2b exists:
    //     a blank line inside the table. GFM ends the table there, so the walk stops
    //     — and the non-blank run stops on the same line, for the same reason, which
    //     is why the two agreed and the extent check could not fire. Measured before
    //     the surface walk landed: a blank planted before line 50 of this document
    //     printed `ok`.
    let blank_at = table.rows[table.rows.len() / 2].0;
    let blank_plan = insert_before(&plan, blank_at, "");
    assert_ne!(
        blank_plan, plan,
        "the blank line was never planted, so the proofs below assert nothing"
    );
    let blank_table = locate_table(&blank_plan, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    assert!(
        blank_table.rows.len() < table.rows.len(),
        "a blank planted at line {blank_at} did not truncate the walk ({} rows \
         against {}), so this plant is not the interruption it is meant to be",
        blank_table.rows.len(),
        table.rows.len(),
    );
    let blank_surface = row_shaped_surface(&blank_plan, &blank_table);
    let retired_extent_passes =
        blank_table.rows.len() == non_blank_run_below_delimiter(&blank_plan, &blank_table);
    let current_extent_passes =
        blank_surface.interruptions.is_empty() && blank_surface.rows == blank_table.rows.len();
    assert!(
        retired_extent_passes && !current_extent_passes,
        "with a blank line planted at line {blank_at}, the non-blank run {retired} and \
         the surface walk {current}. Those two differing here is the whole of the \
         repair: the non-blank run stops at a blank and so does the walk, so for this \
         one shape the expectation and the walk agree *by construction* and step 2's \
         check cannot fire — {} of the {} authority rows leave the gate's scope with a \
         passing output identical to a clean run. If the surface walk also passes, \
         nothing detects an interrupted table again",
        table.rows.len() - blank_table.rows.len(),
        table.rows.len(),
        retired = if retired_extent_passes {
            "agrees with the truncated walk"
        } else {
            "disagrees"
        },
        current = if current_extent_passes {
            "agrees with it too"
        } else {
            "does not"
        },
    );
    assert!(
        blank_surface
            .interruptions
            .iter()
            .any(|(line, _)| *line == blank_at),
        "the interruption at line {blank_at} is not named in the report ({:?}), so \
         the gate would redden without saying where — and a red gate nobody can act \
         on is a deleted gate",
        blank_surface.interruptions,
    );
    assert_eq!(
        blank_surface.rows,
        table.rows.len(),
        "the surface walk lost rows to the interruption it is supposed to step over: \
         {} against the {} this table has. The expectation has to be derived from \
         something the truncation cannot also move, or it is the walk restated",
        blank_surface.rows,
        table.rows.len(),
    );

    // 3d. …and the pair that made the silence dangerous rather than merely wrong: the
    //     same blank, plus a surplus cell on a row **below** it. Both printed `ok`
    //     together, and the surplus cell alone reddens — so the interruption was not
    //     just undetected, it was disarming the check that was working.
    let (below_line, below_raw, _) = table
        .rows
        .iter()
        .find(|(line, _, cells)| *line > blank_at && cells.len() == table.header.len())
        .cloned()
        .expect("the Status table has a well-formed row below its midpoint");
    // The planted blank pushed every row below it down one line.
    let pair_plan = replace_line(
        &blank_plan,
        below_line + 1,
        &format!("{} an orphan cell |", below_raw.trim_end()),
    );
    let pair_table = locate_table(&pair_plan, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    let pair_surface = row_shaped_surface(&pair_plan, &pair_table);
    let cell_check_sees_it = named(&cell_count_offences(&pair_table), below_line + 1);
    let interruption_check_fires = !pair_surface.interruptions.is_empty();
    assert!(
        !cell_check_sees_it && interruption_check_fires,
        "a surplus cell on line {} — below a blank planted at line {blank_at} — was \
         seen by the cell-count check ({cell_check_sees_it}) and the interruption \
         check fired ({interruption_check_fires}). GFM puts that row outside the \
         table, so the cell-count check is *right* not to reach it and the \
         interruption is what has to be reported; this pair is exactly the mutation \
         that used to print `ok`",
        below_line + 1,
    );

    // 4. …and each structural check above, in the direction that would disarm it. A
    //    check that cannot fail prints exactly what a check that is not running
    //    prints (AGENTS §3), which is the register this gate's own row floor was in.
    let typo = STATUS_TABLE_HEADER.replace("Commit / record", "Commit / records");
    assert!(
        locate_table(&plan, &typo).is_none(),
        "a mistyped header needle still located a table — the locator is matching \
         something looser than the header line, so the gate cannot tell 'the table \
         moved' from 'the table is clean'"
    );

    let widened = replace_line(
        &plan,
        table.header_line,
        &format!("| a stray column {STATUS_TABLE_HEADER}"),
    );
    let widened_table = locate_table(&widened, STATUS_TABLE_HEADER)
        .expect("a line carrying the needle plus a prefix still matches the needle");
    assert_eq!(
        widened_table.header.len(),
        table.header.len() + 1,
        "the stray column never landed on the header line, so the contrast below is \
         drawn against the unplanted document"
    );
    // The two checks side by side on the same planted document: the retired one
    // accepts it, the current one must not, and that difference *is* the repair. The
    // retired predicate was `header.len() >= 5`, which [`locate_table`] guarantees by
    // construction — it only ever returns a line carrying the six-pipe needle, and 48
    // needle-carrying variants brute-force to a minimum of exactly 5 — so it could
    // not fail, and printed what not running prints.
    let retired_check_passes = widened_table.header.len() >= 5;
    let current_check_passes = widened_table.header == split_cells(STATUS_TABLE_HEADER);
    assert!(
        retired_check_passes && !current_check_passes,
        "on a header line carrying a stray cell *before* the needle, the retired \
         check {retired} and the current one {current}. Those two differing here is \
         the whole of the repair: if the current check also passes, step 2 cannot \
         fail and the column positions every figure below is read at are unpinned \
         again; if the retired one fails, this plant is not the document that \
         motivated the change",
        retired = if retired_check_passes {
            "passes"
        } else {
            "fails"
        },
        current = if current_check_passes {
            "passes"
        } else {
            "fails"
        },
    );

    let no_delimiter = delete_line(&plan, table.header_line + 1);
    let no_delimiter_table = locate_table(&no_delimiter, STATUS_TABLE_HEADER)
        .expect("deleting the delimiter row leaves the header in place");
    assert!(
        !delimiter_offences(&no_delimiter_table).is_empty(),
        "with the `|---|` row deleted, the first **data** row was accepted in its \
         place and the delimiter check stayed green — while `marked@15` renders the \
         whole thing as a paragraph. This is what a cell-count check cannot see: the \
         promoted row has exactly the header's count"
    );
    let corrupt_delimiter = replace_line(
        &plan,
        table.header_line + 1,
        &format!("|- -|{}", "---|".repeat(table.header.len() - 1)),
    );
    let corrupt_table = locate_table(&corrupt_delimiter, STATUS_TABLE_HEADER)
        .expect("the header is untouched by a corrupted delimiter row");
    assert!(
        !delimiter_offences(&corrupt_table).is_empty(),
        "one delimiter cell corrupted to `- -` left the delimiter check green, and \
         `marked@15` renders no table at all for that document — the count matched, \
         which is all the check this replaced ever asked"
    );

    let interrupted_at = table.rows[table.rows.len() / 2].0;
    let interrupted = replace_line(&plan, interrupted_at, "## a heading in the table");
    let interrupted_table = locate_table(&interrupted, STATUS_TABLE_HEADER)
        .expect("the planted document still carries the Status table");
    assert!(
        interrupted_table.rows.len()
            < non_blank_run_below_delimiter(&interrupted, &interrupted_table),
        "a heading planted mid-table (line {interrupted_at}) left the walk covering \
         every non-blank line below the delimiter, so the extent check in step 2 \
         cannot fail and is a restatement of the walk rather than a check on it"
    );

    // 4b. The misdirection plant: a second, **perfectly formed** table under the same
    //     header, above the real one. [`locate_table`] takes the first line carrying
    //     the needle, so the whole gate re-aims at the decoy — and every check it
    //     makes is ideal there, because a decoy nobody wrote in anger has no defects.
    //     Measured: a nine-row decoy above line 30 plus a surplus cell on line 60 of
    //     the real table printed `ok`. Both reference renderers read that document as
    //     two tables, 9 rows and 33, so it is a second table by GFM's rules and not an
    //     artefact of this file's parser.
    let decoy_row = |n: usize| {
        let mut cells = vec![format!("decoy row {n}")];
        cells.extend((1..table.header.len()).map(|_| "—".to_owned()));
        format!("| {} |", cells.join(" | "))
    };
    let decoy_block = format!(
        "{STATUS_TABLE_HEADER}\n|{}\n{}\n",
        "---|".repeat(table.header.len()),
        (1..=9).map(decoy_row).collect::<Vec<_>>().join("\n"),
    );
    let decoy_plan = insert_before(&plan, table.header_line, &decoy_block);
    let shift = decoy_plan.lines().count() - plan.lines().count();
    let decoy_plan = replace_line(
        &decoy_plan,
        below_line + shift,
        &format!("{} an orphan cell |", below_raw.trim_end()),
    );
    let decoy_headers = lines_containing(&decoy_plan, STATUS_TABLE_HEADER);
    assert_eq!(
        decoy_headers.len(),
        2,
        "the decoy table was never planted, so the proof below asserts nothing"
    );
    let decoy_table = locate_table(&decoy_plan, STATUS_TABLE_HEADER)
        .expect("the planted document carries two tables under this header");
    assert_eq!(
        (decoy_table.header_line, decoy_table.rows.len()),
        (decoy_headers[0], 9),
        "the locator did not land on the first of the two headers and walk its nine \
         rows, so this plant is not the misdirection it is meant to prove"
    );
    // The real table is still there, below the decoy, and it now carries the surplus
    // cell — which is what "goes wholly unchecked" means.
    let below_the_decoy = decoy_plan
        .lines()
        .skip(decoy_headers[1] - 1)
        .collect::<Vec<_>>()
        .join("\n");
    let real_table = locate_table(&below_the_decoy, STATUS_TABLE_HEADER)
        .expect("the real Status table survives below the decoy");
    assert!(
        !cell_count_offences(&real_table).is_empty(),
        "the surplus cell never landed in the real table, so this plant proves \
         nothing about what the decoy hides"
    );
    let decoy_surface = row_shaped_surface(&decoy_plan, &decoy_table);
    let retired_checks_pass = decoy_table.header == split_cells(STATUS_TABLE_HEADER)
        && delimiter_offences(&decoy_table).is_empty()
        && decoy_table.rows.len() == non_blank_run_below_delimiter(&decoy_plan, &decoy_table)
        && decoy_table.rows.len() == decoy_surface.rows
        && decoy_surface.interruptions.is_empty()
        && cell_count_offences(&decoy_table).is_empty();
    let current_check_passes = lines_containing(&decoy_plan, STATUS_TABLE_HEADER).len() == 1;
    assert!(
        retired_checks_pass && !current_check_passes,
        "against a document carrying a decoy table under this header, every check but \
         one {retired}, and the needle-count check {current}. That difference is the \
         second repair: the decoy satisfies the header shape, the delimiter row, both \
         extents and the cell counts — being well formed is *all* it has to be — while \
         the real table below it, surplus cell and all, is never read. If the \
         needle-count check also passes, a second table silently re-aims the gate again",
        retired = if retired_checks_pass {
            "passes on the decoy"
        } else {
            "fails on the decoy"
        },
        current = if current_check_passes {
            "passes too"
        } else {
            "fails"
        },
    );

    // 5. The verdict.
    let offences = cell_count_offences(&table);
    assert!(
        offences.is_empty(),
        "the plan's Status table has {n} row(s) whose cell count differs from the \
         header's {expected} (header at line {header_line}):\n  {report}\n\
         GitHub-flavoured Markdown silently ignores cells past the header count and \
         leaves a short row's later columns shifted, so neither shape is visible in \
         the rendered table — which is how one survives a landing. Plan §3 rule 19 \
         makes this table the only authoritative home of every current-era measured \
         figure, and a figure whose Scope or Caveat has slid into the wrong column is \
         a misquotable figure (plan §18 item 94).",
        n = offences.len(),
        expected = table.header.len(),
        header_line = table.header_line,
        report = offences.join("\n  "),
    );
}
