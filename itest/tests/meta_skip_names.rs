#![forbid(unsafe_code)]

//! **The SKIP-message naming gate** (plan §18 item 49 clause (b); plan §3 rules 10, 11
//! and 22).
//!
//! Every self-skip in this tree prints one line naming the test that skipped, and
//! forty-plus of them **free-type that name**. Two spellings carry it:
//!
//! * a message literal — the skip line written inline, whose first word is the test's
//!   own function name;
//! * a call to a *skip helper* — `skip_no_rig`, `skip_no_replug`, `skip_no_tls`,
//!   `skip_no_rig_flow`, `skip_no_exec_codec`, and the file-local ones
//!   (`skip_off_linux`, `premise_holds`, `ringless_window_gap`) — whose first argument
//!   is that same name as a string literal, because the helper cannot see its caller.
//!
//! Nothing kept those names true. A rename moves the `fn` and leaves the message
//! behind, and the result is a skip line that is **false on the box printing it** —
//! plan §3 rule 11's bar in as many words ("a skip message names what the provider
//! actually saw and must never be false on the box printing it"). The failure is quiet
//! by construction: an operator greps the suite output for the test they are chasing,
//! finds nothing, and concludes it ran.
//!
//! **Measured before this gate was written: zero live mismatches.** So this is
//! prevention, not repair, and it is written the way a preventive gate has to be — it
//! plants its own violation in every spelling it claims to cover before it trusts a
//! clean verdict (plan §3 rule 10; AGENTS §3's "a scanning gate proves its **matcher**
//! as well as its walker"), in three layers:
//!
//! 1. *the matcher*, against synthetic sources in each spelling — the one-line message,
//!    the backslash-continued multi-line message, the helper call on one line, the
//!    helper call split across lines, a path-qualified call
//!    (`serial_nexus_itest::skip_no_tls(…)`), a helper whose name parameter is not the
//!    first — and against the near-misses that must **not** trip it: an *interpolated*
//!    name (`{test}`, which is how the helpers themselves print), a non-literal
//!    argument (`&why`), a helper's own definition, a look-alike callee, and a message
//!    whose first word is ordinary prose rather than an identifier;
//! 2. *the walker*, by planting a file two directories down a scratch tree and
//!    requiring the enumeration to surface what it holds — the half review 37 found
//!    missing from `meta_gates.rs`, whose planted offender was always a string and
//!    never a file;
//! 3. *the comparison*, by corrupting **this tree's own bytes** — one real subject
//!    renamed in memory — so the failure message this gate would print is exercised on
//!    real source rather than on a fixture that resembles it.
//!
//! **The helper roster is derived, never listed** (plan §3 rule 11: "the roster's
//! authority is the code, not any prose list"). A skip helper is recognised by what it
//! does — it interpolates one of its own parameters into a skip line — and its
//! name-carrying argument *index* comes from its signature, so a helper added anywhere
//! in the tree is covered the moment it lands, and one whose name parameter moves is
//! still read correctly.
//!
//! **Why this file carries a real scanner instead of `meta_derive.rs`'s line-stripper.**
//! That stripper truncates a line at its first `//` and says so; its matchers are local,
//! so a truncated line costs it one missed match. This gate's question is *structural* —
//! which function's braces contain this offset — and there a truncated line is fatal:
//! `p8_web.rs` writes `format!("ws://127.0.0.1:{port}/ws")`, which a line-stripper cuts
//! mid-literal into an unterminated quote, flipping the string parity of everything
//! after it and running one test's body over the next one's. That was measured here, as
//! a false report against `web_tls_round_trip`. So [`scan`] walks the source properly —
//! line and block comments, normal, raw and byte strings, character literals against
//! lifetimes — and produces two byte-aligned views: `text`, which keeps string contents
//! (a skip line lives inside one), and `skel`, which blanks them (so a brace inside a
//! TOML fixture cannot close a function).
//!
//! **What this gate deliberately does not assert.** It does not require every skip line
//! to name a test: several name a variable instead (`{test}`, `{fn_name}`), which is the
//! *stronger* form and the one this gate pushes authors toward, since a name that
//! arrives as an argument cannot go stale. The rule is conditional — *if* a skip line
//! names a test, that name is its enclosing function's — and the floors below are what
//! keep a conditional rule from becoming a vacuous one (plan §3 rule 22).
//!
//! **This file is its own worst case**, exactly as `meta_derive.rs` is: the walk below
//! reads it like any other source, and every fixture in it is a deliberate violation.
//! So no fixture is written as a literal — each is assembled at runtime from a `q`
//! holding the quote character — and this file's own bytes therefore contain no string
//! a matcher here can see. A fixture that regressed to a literal would fail this gate
//! against itself, loudly, which is the right failure.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/itest — the *directory*, which §15.40 kept short.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the itest crate has a parent directory")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// The source scanner
// ---------------------------------------------------------------------------

/// Two byte-aligned views of one `.rs` source.
///
/// Both are exactly as long as the original and blank rather than delete, so **every
/// offset means the same thing in all three** — a line number computed here points at
/// the line a reader will open.
struct Source {
    /// Comments and character literals blanked; string literals kept intact, because a
    /// skip line lives inside one.
    text: String,
    /// `text` with string *contents* blanked as well — the structural skeleton. Brace
    /// and parenthesis counting runs on this and needs no string state of its own,
    /// which is the whole point: the TOML and Python fixtures embedded all over this
    /// suite are full of braces, quotes and parentheses that are not code.
    skel: String,
}

/// Blank `orig[range]` in `v`, preserving newlines so line numbers survive.
fn blank(v: &mut [u8], orig: &[u8], from: usize, to: usize) {
    for k in from..to.min(orig.len()) {
        if orig[k] != b'\n' {
            v[k] = b' ';
        }
    }
}

/// Scan `src` into its [`Source`] views.
///
/// Handles what this tree actually contains: `//` and nested `/* */` comments, normal
/// strings with escapes, raw strings with any number of hashes (`r#"…"#`, and the `b`
/// prefix), and character literals — the last distinguished from **lifetimes**, which
/// open a quote and never close it. Getting that distinction wrong swallows everything
/// from a `<'a>` to the next apostrophe, which is a whole function body in this tree.
fn scan(src: &str) -> Source {
    let b = src.as_bytes();
    let n = b.len();
    let mut text = b.to_vec();
    let mut skel = b.to_vec();

    let mut i = 0usize;
    while i < n {
        match b[i] {
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                let mut j = i;
                while j < n && b[j] != b'\n' {
                    j += 1;
                }
                blank(&mut text, b, i, j);
                blank(&mut skel, b, i, j);
                i = j;
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                let mut depth = 0usize;
                let mut j = i;
                while j < n {
                    if b[j] == b'/' && j + 1 < n && b[j + 1] == b'*' {
                        depth += 1;
                        j += 2;
                        continue;
                    }
                    if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
                        depth -= 1;
                        j += 2;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    j += 1;
                }
                blank(&mut text, b, i, j);
                blank(&mut skel, b, i, j);
                i = j;
            }
            b'"' => {
                // A raw string announces itself *behind* the quote: `r"`, `r#"`,
                // `br##"`. Counting the hashes backwards is how the closing delimiter
                // is known.
                let mut hashes = 0usize;
                let mut k = i;
                while k > 0 && b[k - 1] == b'#' {
                    hashes += 1;
                    k -= 1;
                }
                let raw = k > 0 && b[k - 1] == b'r';
                let mut j = i + 1;
                let close = loop {
                    if j >= n {
                        break n;
                    }
                    if raw {
                        if b[j] == b'"'
                            && j + hashes < n
                            && b[j + 1..j + 1 + hashes].iter().all(|c| *c == b'#')
                        {
                            break j;
                        }
                        j += 1;
                    } else {
                        match b[j] {
                            b'\\' => j += 2,
                            b'"' => break j,
                            _ => j += 1,
                        }
                    }
                };
                blank(&mut skel, b, i + 1, close);
                i = close + 1;
            }
            b'\'' => {
                let mut j = i + 1;
                if j < n && b[j] == b'\\' {
                    // An escape: `'\n'`, `'\''`, `'\u{1F600}'`.
                    j += 2;
                    while j < n && b[j] != b'\'' {
                        j += 1;
                    }
                } else if j < n {
                    // One whole character — `§` is three bytes and must not be split.
                    j += 1;
                    while j < n && !src.is_char_boundary(j) {
                        j += 1;
                    }
                }
                if j < n && b[j] == b'\'' {
                    blank(&mut text, b, i, j + 1);
                    blank(&mut skel, b, i, j + 1);
                    i = j + 1;
                } else {
                    // A lifetime, which never closes. Step over the quote alone.
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    Source {
        text: String::from_utf8(text).expect("blanking preserves UTF-8 boundaries"),
        skel: String::from_utf8(skel).expect("blanking preserves UTF-8 boundaries"),
    }
}

// ---------------------------------------------------------------------------
// Structural helpers — all of these read the skeleton
// ---------------------------------------------------------------------------

fn is_word(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// The identifier beginning at byte `at`, or `""` if there is none there.
fn ident_at(code: &str, at: usize) -> &str {
    let bytes = code.as_bytes();
    let mut end = at;
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    &code[at..end]
}

/// Does `s` have the shape of a Rust test function's name?
///
/// The discriminator between "this skip line names a test" and "this skip line begins
/// with an ordinary word": snake_case, lowercase-initial, nothing else. A message that
/// opens with prose fails it and is left unchecked rather than reported — the
/// conditional rule stated in the module doc.
fn is_fn_name_shaped(s: &str) -> bool {
    !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The 1-based line number of byte offset `at`.
fn line_of(code: &str, at: usize) -> usize {
    code[..at].bytes().filter(|b| *b == b'\n').count() + 1
}

/// The span *inside* the group opened at `open_at` by `open`/`close`, by depth
/// counting. Runs on the skeleton, where every delimiter left is code.
fn group_inner(skel: &str, open_at: usize, open: u8, close: u8) -> Option<Range<usize>> {
    let bytes = skel.as_bytes();
    if bytes.get(open_at) != Some(&open) {
        return None;
    }
    let mut depth = 0usize;
    for (i, c) in bytes.iter().enumerate().skip(open_at) {
        if *c == open {
            depth += 1;
        } else if *c == close {
            depth -= 1;
            if depth == 0 {
                return Some(open_at + 1..i);
            }
        }
    }
    None
}

/// Split an argument (or parameter) list on its **top-level** commas, returning
/// absolute ranges into the source so the caller can read the same span out of `text`.
fn split_args(skel: &str, span: Range<usize>) -> Vec<Range<usize>> {
    let bytes = skel.as_bytes();
    let mut out = Vec::new();
    let mut start = span.start;
    let mut depth = 0i32;
    for i in span.clone() {
        match bytes[i] {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                out.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(start..span.end);
    out
}

/// The content of `arg` when it is a plain one-piece string literal, else `None`.
fn string_literal(arg: &str) -> Option<&str> {
    let t = arg.trim();
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    if inner.contains('"') {
        None
    } else {
        Some(inner)
    }
}

/// One function definition: its name, its `fn` keyword's offset, and its body's span.
#[derive(Debug, Clone)]
struct FnDef {
    name: String,
    fn_at: usize,
    body: Range<usize>,
}

/// Every function definition in the source, with the **span of its body**.
///
/// Spans rather than "the nearest preceding `fn`", and the difference is not academic:
/// `p8_soak.rs`'s `soak_endurance` defines a nested `fn env_u64` and *then* prints its
/// skip line, so a nearest-preceding resolver reports a false mismatch against a helper
/// the skip line is not inside. That was this gate's first finding, and it was the
/// gate's own defect rather than the tree's — the class plan §3 rule 10 calls a matcher
/// that has not been proved.
///
/// Declarations without a body (a trait's `fn f(&self);`) are skipped: the scan from the
/// closing parenthesis stops at whichever of `{` and `;` comes first.
fn fn_defs(skel: &str) -> Vec<FnDef> {
    let bytes = skel.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(pos) = skel[i..].find("fn ") {
        let at = i + pos;
        i = at + 3;
        // A keyword, not the tail of an identifier.
        if at > 0 && is_word(bytes[at - 1]) {
            continue;
        }
        let name = ident_at(skel, at + 3);
        if name.is_empty() {
            continue;
        }
        // Step over a generic parameter list to reach the argument list.
        let after_name = at + 3 + name.len();
        let Some(rel) = skel[after_name..].find('(') else {
            continue;
        };
        let Some(args) = group_inner(skel, after_name + rel, b'(', b')') else {
            continue;
        };
        // The body opens at the first `{` after the signature; a `;` first means this
        // is a declaration and has no body.
        let mut j = args.end + 1;
        let body_open = loop {
            match bytes.get(j) {
                None | Some(b';') => break None,
                Some(b'{') => break Some(j),
                _ => j += 1,
            }
        };
        let Some(body_open) = body_open else { continue };
        let Some(body) = group_inner(skel, body_open, b'{', b'}') else {
            continue;
        };
        out.push(FnDef {
            name: name.to_owned(),
            fn_at: at,
            body,
        });
    }
    out
}

/// The **innermost** definition whose body contains `at`.
///
/// A skip line inside a closure (`p8_web_ui`'s `let skip = |…|` shape) resolves to the
/// `#[test]` fn the closure lives in, because a closure is not a `fn`. A skip line
/// inside a *nested* `fn` resolves to that nested fn, which is the right answer rather
/// than a limitation: a helper cannot honestly hard-code a caller's name, and the fix is
/// the one the shared helpers already model — pass the name in.
fn enclosing_fn(defs: &[FnDef], at: usize) -> Option<&FnDef> {
    defs.iter()
        .filter(|d| d.body.contains(&at))
        .max_by_key(|d| d.body.start)
}

/// Does the item whose `fn` keyword sits at `fn_at` carry `#[test]`?
///
/// Walked line by line backwards over the attribute block — blank lines included,
/// because a blanked doc comment leaves one — and stopped by the first line that is
/// neither blank nor an attribute, which is the previous item's `}` or `;`. Used only
/// for the non-vacuity floors below, never as a per-site requirement: the rule this gate
/// enforces is name equality, and a multi-line attribute (none in this tree today) would
/// end the walk early. A floor tolerates that; a per-site assertion would turn it into a
/// red gate for a reason that is not the tree's, and a red gate that is wrong gets
/// deleted.
fn carries_test_attribute(text: &str, fn_at: usize) -> bool {
    let mut line_start = text[..fn_at].rfind('\n').map(|n| n + 1).unwrap_or(0);
    while line_start > 0 {
        let prev_end = line_start - 1;
        let prev_start = text[..prev_end].rfind('\n').map(|n| n + 1).unwrap_or(0);
        let line = text[prev_start..prev_end].trim();
        if line == "#[test]" {
            return true;
        }
        if line.is_empty() || line.starts_with("#[") {
            line_start = prev_start;
            continue;
        }
        return false;
    }
    false
}

// ---------------------------------------------------------------------------
// The two matchers
// ---------------------------------------------------------------------------

/// One place a skip line names a test: where it is, and the name it typed.
#[derive(Debug, Clone)]
struct Subject {
    at: usize,
    named: String,
    /// How the name was written, for the failure message: the spelling that has to be
    /// edited when a rename lands.
    spelling: String,
}

/// Every skip **message literal** whose first word is a test name.
///
/// The needle is the opening quote plus the marker word, so the name being read is
/// always the message's first token; `{test}`-style interpolation stops at the brace and
/// is correctly enumerated as naming nothing.
fn message_subjects(src: &Source) -> Vec<Subject> {
    let q = '"';
    let needle = format!("{q}SKIP ");
    let code = &src.text;
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(pos) = code[i..].find(&needle) {
        let at = i + pos;
        i = at + needle.len();
        let name = ident_at(code, i);
        if !is_fn_name_shaped(name) {
            continue;
        }
        // The separator every skip line uses between the test and the reason. Without
        // it, `SKIP the run: …` would read `the` as a test name.
        if !code[i + name.len()..].starts_with(':') {
            continue;
        }
        out.push(Subject {
            at,
            named: name.to_owned(),
            spelling: "a skip message literal".to_owned(),
        });
    }
    out
}

/// Every **skip helper** defined in the source: its name, and the position of the
/// parameter it interpolates into its skip line.
///
/// Derived from behavior rather than from a list of names (plan §3 rule 11). A helper is
/// a function that prints a skip line naming one of its own parameters — which is
/// exactly the shape that forces its callers to type a test name they cannot verify, and
/// so exactly the shape whose call sites this gate has to check.
fn skip_helpers(src: &Source) -> BTreeMap<String, usize> {
    let q = '"';
    let needle = format!("{q}SKIP {{");
    let defs = fn_defs(&src.skel);
    let mut out = BTreeMap::new();
    let mut i = 0usize;
    while let Some(pos) = src.text[i..].find(&needle) {
        let at = i + pos;
        i = at + needle.len();
        let param = ident_at(&src.text, i);
        if param.is_empty() {
            continue;
        }
        // `{param}` or `{param:…}` — anything else is not a plain interpolation.
        match src.text[i + param.len()..].chars().next() {
            Some('}') | Some(':') => {}
            _ => continue,
        }
        let Some(def) = enclosing_fn(&defs, at) else {
            continue;
        };
        let sig = def.fn_at + 3 + def.name.len();
        let Some(rel) = src.skel[sig..].find('(') else {
            continue;
        };
        let Some(params) = group_inner(&src.skel, sig + rel, b'(', b')') else {
            continue;
        };
        let index = split_args(&src.skel, params).iter().position(|r| {
            src.skel[r.clone()]
                .split(':')
                .next()
                .map(|n| n.trim().trim_start_matches("mut ").trim() == param)
                .unwrap_or(false)
        });
        if let Some(index) = index {
            out.insert(def.name.clone(), index);
        }
    }
    out
}

/// Every call to one of `helpers` whose name argument is a **literal** test name.
fn helper_call_subjects(src: &Source, helpers: &BTreeMap<String, usize>) -> Vec<Subject> {
    let bytes = src.skel.as_bytes();
    let mut out = Vec::new();
    for (name, index) in helpers {
        let needle = format!("{name}(");
        let mut i = 0usize;
        while let Some(pos) = src.skel[i..].find(&needle) {
            let at = i + pos;
            i = at + name.len();
            // A whole identifier — a path-qualified call (`itest::skip_no_tls(`) is one
            // of these, since `:` is not a word byte, while `my_skip_no_tls(` is not.
            if at > 0 && is_word(bytes[at - 1]) {
                continue;
            }
            // The definition itself is not a call site.
            if src.skel[..at].trim_end().ends_with("fn") {
                continue;
            }
            let Some(args) = group_inner(&src.skel, at + name.len(), b'(', b')') else {
                continue;
            };
            let args = split_args(&src.skel, args);
            let Some(arg) = args.get(*index) else {
                continue;
            };
            // Read the *text* view here: the skeleton has the literal's content blanked.
            let Some(lit) = string_literal(&src.text[arg.clone()]) else {
                continue;
            };
            if !is_fn_name_shaped(lit) {
                continue;
            }
            out.push(Subject {
                at,
                named: lit.to_owned(),
                spelling: format!("argument {index} of `{name}`"),
            });
        }
    }
    out
}

/// A skip line that names a test other than the one it is inside.
#[derive(Debug)]
struct Mismatch {
    file: String,
    line: usize,
    named: String,
    enclosing: String,
    spelling: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} — {} says `{}`, but the enclosing fn is `{}`",
            self.file, self.line, self.spelling, self.named, self.enclosing
        )
    }
}

/// Resolve every subject against its enclosing function, returning the ones that lie and
/// the count of the ones that sat in a `#[test]` fn (the non-vacuity evidence).
fn judge(rel: &str, src: &Source, subjects: &[Subject]) -> (Vec<Mismatch>, usize) {
    let defs = fn_defs(&src.skel);
    let mut bad = Vec::new();
    let mut in_tests = 0usize;
    for s in subjects {
        let Some(def) = enclosing_fn(&defs, s.at) else {
            bad.push(Mismatch {
                file: rel.to_owned(),
                line: line_of(&src.text, s.at),
                named: s.named.clone(),
                enclosing: "<no enclosing fn>".to_owned(),
                spelling: s.spelling.clone(),
            });
            continue;
        };
        if carries_test_attribute(&src.text, def.fn_at) {
            in_tests += 1;
        }
        if def.name != s.named {
            bad.push(Mismatch {
                file: rel.to_owned(),
                line: line_of(&src.text, s.at),
                named: s.named.clone(),
                enclosing: def.name.clone(),
                spelling: s.spelling.clone(),
            });
        }
    }
    (bad, in_tests)
}

// ---------------------------------------------------------------------------
// The walker
// ---------------------------------------------------------------------------

/// What one [`walk_rs`] pass did — the evidence a clean verdict needs.
#[derive(Default)]
struct WalkStats {
    files: usize,
    unreadable: Vec<String>,
}

/// Is `dir` the root of a nested checkout? A worktree carries a `.git` file, a clone a
/// `.git` directory — the predicate rather than a name list (plan §3 rule 10).
fn is_nested_checkout(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Visit every `.rs` file below `dir`, skipping build output, VCS, vendored trees and
/// nested checkouts. `fuzz/` is walked rather than skipped: it is outside the workspace,
/// but a skip line landing there would have to be true too.
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
                if matches!(name.as_ref(), "target" | ".git" | "node_modules")
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

/// A scratch tree that removes itself on drop. Named by pid and call site so parallel
/// test binaries never share one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("snx-meta-skips-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch tree");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

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

/// A file's path relative to the repo root, for a failure message a reader can open.
fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

// ---------------------------------------------------------------------------
// (0) The scanner's own guards
// ---------------------------------------------------------------------------

/// The scanner is the layer under both gates, and every one of these cases was either
/// measured against this tree or is a Rust construct one line away from it. A scanner
/// defect does not weaken the gates below — it makes them report *the wrong file*, which
/// is worse than silence, and it did exactly that once (see the module doc).
#[test]
fn the_source_scanner_survives_the_constructs_this_tree_is_written_in() {
    let q = '"';

    // The measured one: a `//` inside a string literal. A line-stripper cuts here and
    // leaves an unterminated quote, and every brace after it lands in the wrong body.
    let url = format!(
        "fn a() {{\n    let u = format!({q}ws://host:{{p}}/ws{q});\n}}\nfn b() {{\n    let _ = 1;\n}}\n"
    );
    let s = scan(&url);
    let defs = fn_defs(&s.skel);
    assert_eq!(
        defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"],
        "a `//` inside a string literal ran one function's body into the next: {defs:?}"
    );

    // A raw string full of the punctuation the skeleton counts — the TOML fixtures this
    // suite is built out of.
    let toml = format!(
        "fn a() {{\n    let c = r#{q}\n[[node]]\ntype = {q}pty{q}\npath = {q}{{p}}{q}\n{q}#;\n}}\nfn b() {{\n    let _ = 1;\n}}\n"
    );
    let s = scan(&toml);
    assert_eq!(
        fn_defs(&s.skel)
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"],
        "a raw string containing quotes and braces broke the body spans"
    );

    // Character literals that *are* the delimiters, beside a lifetime that opens a quote
    // and never closes it. Both live in this tree's meta-gates.
    let chars = format!(
        "fn a<{}>(x: &{} str) {{\n    let _ = ({}, {}, {}, {});\n}}\nfn b() {{\n    let _ = 1;\n}}\n",
        "'l", "'l", "'{'", "'}'", "'('", "')'"
    );
    let s = scan(&chars);
    assert_eq!(
        fn_defs(&s.skel)
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"],
        "a delimiter character literal or a lifetime broke the body spans"
    );

    // A block comment, and a skip line inside a comment: prose about a skip line is not
    // a skip line, and this tree's doc comments quote them constantly.
    let commented = "/* fn ghost() { */\nfn a() {\n    \n}\n"
        .replace("    \n", &format!("    // eprintln!({q}SKIP a: x{q});\n"));
    let s = scan(&commented);
    assert!(
        message_subjects(&s).is_empty(),
        "a skip line inside a comment was enumerated as a real one"
    );
    assert_eq!(
        fn_defs(&s.skel)
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a"],
        "a `fn` inside a block comment was read as a definition"
    );

    // Offsets are preserved, which is what makes a reported line number openable.
    let s = scan(&url);
    assert_eq!(s.text.len(), url.len(), "the text view changed length");
    assert_eq!(s.skel.len(), url.len(), "the skeleton changed length");
}

// ---------------------------------------------------------------------------
// (1) Skip message literals
// ---------------------------------------------------------------------------

#[test]
fn a_skip_message_naming_a_test_names_its_enclosing_fn() {
    let root = repo_root();
    let q = '"';

    // 0. The matcher, in every spelling it claims to cover and every near miss it must
    //    ignore. Assembled rather than written out, so this file's own bytes carry no
    //    string the walk in step 1 could read as a skip line.
    let one_line = format!(
        "#[test]\nfn planted_test() {{\n    eprintln!({q}SKIP planted_test: no device{q});\n}}\n"
    );
    let found = message_subjects(&scan(&one_line));
    assert_eq!(
        found.len(),
        1,
        "the matcher does not see the one-line skip message, which is how most of this \
         tree's forty-odd skip lines are written; it enumerated {found:?}"
    );
    assert_eq!(found[0].named, "planted_test");

    // The continued form: the name is on the first physical line, the reason wraps.
    let wrapped = format!(
        "#[test]\nfn planted_test() {{\n    eprintln!(\n        {q}SKIP planted_test: needs a \\\n         Linux pty{q}\n    );\n}}\n"
    );
    assert_eq!(
        message_subjects(&scan(&wrapped)).len(),
        1,
        "the matcher misses the backslash-continued message — a third of this tree's \
         skip lines wrap, and they would all go unchecked"
    );

    // An *interpolated* name is the stronger form and must not be enumerated: the
    // helpers in itest/src/lib.rs all print exactly this, and reading it as a subject
    // would attribute the helper's own fn name to every caller's skip.
    let interpolated =
        format!("fn skip_it(test: &str) {{\n    eprintln!({q}SKIP {{test}}: gone{q});\n}}\n");
    assert!(
        message_subjects(&scan(&interpolated)).is_empty(),
        "an interpolated skip name is read as a literal test name — every shared skip \
         helper would then be reported as lying about its own caller"
    );

    // Prose, not a name: `the` is identifier-shaped but is not followed by the separator
    // every skip line uses.
    let prose = format!("fn f() {{\n    eprintln!({q}SKIP the run: unsupported{q});\n}}\n");
    assert!(
        message_subjects(&scan(&prose)).is_empty(),
        "a skip line opening with ordinary prose is read as naming a test, so the gate \
         would demand a fn named after an English word"
    );

    // The enclosing-fn resolution itself, including the two shapes that decide it: a
    // second definition after the first, and a skip line inside a closure (p8_web_ui's
    // `let skip = |…|` shape) which must still resolve to the test it lives in.
    let two = format!(
        "#[test]\nfn first_test() {{\n    eprintln!({q}SKIP first_test: a{q});\n}}\n\n#[test]\nfn second_test() {{\n    let skip = |why: &str| eprintln!({q}SKIP second_test: {{why}}{q});\n    skip({q}b{q});\n}}\n"
    );
    let two_src = scan(&two);
    let (bad, in_tests) = judge("planted.rs", &two_src, &message_subjects(&two_src));
    assert!(
        bad.is_empty(),
        "two correct skip lines — one of them inside a closure — were reported as \
         mismatches: {bad:?}"
    );
    assert_eq!(
        in_tests, 2,
        "the #[test] attribute walk did not recognise both planted tests, so the \
         non-vacuity floor below counts nothing"
    );

    // A nested `fn` defined *before* the skip line must not capture it — `p8_soak`'s
    // `soak_endurance` defines a local `env_u64` and then prints its skip, and a
    // nearest-preceding resolver reports that as a mismatch against a helper the skip
    // line is not inside. This gate's first run made exactly that false report; the
    // fixture is that finding, kept.
    let nested = format!(
        "#[test]\nfn planted_test() {{\n    fn helper(k: &str) -> u64 {{\n        k.len() as u64\n    }}\n    let _ = helper({q}x{q});\n    eprintln!({q}SKIP planted_test: no device{q});\n}}\n"
    );
    let nested_src = scan(&nested);
    let (bad, _) = judge("planted.rs", &nested_src, &message_subjects(&nested_src));
    assert!(
        bad.is_empty(),
        "a skip line following a nested `fn` was attributed to that nested fn rather \
         than to the test it is inside: {bad:?}"
    );
    // …while a skip line genuinely *inside* the nested fn is still attributed to it,
    // which is the right answer: a helper cannot honestly hard-code a caller's name.
    let inside = nested.replace(
        "        k.len() as u64\n",
        &format!(
            "        eprintln!({q}SKIP planted_test: no device{q});\n        k.len() as u64\n"
        ),
    );
    assert_ne!(inside, nested, "the planted inner skip was never inserted");
    let inside_src = scan(&inside);
    let (bad, _) = judge("planted.rs", &inside_src, &message_subjects(&inside_src));
    assert_eq!(
        bad.len(),
        1,
        "a hard-coded test name inside a nested helper was accepted; the resolver is \
         not scoping by body at all. Reported: {bad:?}"
    );

    // …and the violation, in the same shape: the name of the *other* test. Assembled
    // rather than written out for the reason the module doc gives — a literal here is a
    // skip line in this file's own bytes, and the walk in step 2 reads them.
    let wrong = two.replace(
        &format!("{q}SKIP second_test:"),
        &format!("{q}SKIP first_test:"),
    );
    assert_ne!(wrong, two, "the planted rename was never applied");
    let wrong_src = scan(&wrong);
    let (bad, _) = judge("planted.rs", &wrong_src, &message_subjects(&wrong_src));
    assert_eq!(
        bad.len(),
        1,
        "a skip line naming the wrong test was not reported — the exact defect this gate \
         exists for. Reported: {bad:?}"
    );
    assert!(
        bad[0].to_string().contains("second_test"),
        "the report does not name the enclosing fn, so a reader cannot see which of the \
         two spellings to fix: {}",
        bad[0]
    );

    // The `#[test]` walk's own near miss: a plain fn must not inherit the attribute of
    // the item above it.
    let after = format!(
        "#[test]\nfn planted_test() {{\n    let _ = 1;\n}}\n\nfn planted_helper() {{\n    eprintln!({q}SKIP planted_helper: x{q});\n}}\n"
    );
    let after_src = scan(&after);
    let (_, in_tests) = judge("planted.rs", &after_src, &message_subjects(&after_src));
    assert_eq!(
        in_tests, 0,
        "the attribute walk stepped over a previous item's body and credited a plain \
         helper with `#[test]`; the floors below would then be satisfied by non-tests"
    );

    // 1. The walker: plant a file two directories down and require the walk to surface
    //    what it holds, mismatch included.
    let scratch = Scratch::new("messages");
    scratch.write("deep/nested/planted.rs", &wrong);
    let mut planted: Vec<Mismatch> = Vec::new();
    let planted_stats = walk_rs(scratch.path(), &mut |p, raw| {
        let src = scan(raw);
        let (bad, _) = judge(&p.display().to_string(), &src, &message_subjects(&src));
        planted.extend(bad);
    });
    assert_eq!(
        planted_stats.files, 1,
        "the planted walk visited an unexpected file count"
    );
    assert_eq!(
        planted.len(),
        1,
        "the walk did not reach a planted .rs file two directories down — a walk that \
         stops walking reports the same green as a clean tree. Found: {planted:?}"
    );

    // 2. The real scan.
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut subjects = 0usize;
    let mut in_test_fns = 0usize;
    let mut files_with_subjects: BTreeSet<String> = BTreeSet::new();
    let mut first_real: Option<(String, String, String)> = None; // (rel, raw source, name)
    let stats = walk_rs(&root, &mut |p, raw| {
        let rel = rel_of(&root, p);
        let src = scan(raw);
        let found = message_subjects(&src);
        if found.is_empty() {
            return;
        }
        subjects += found.len();
        files_with_subjects.insert(rel.clone());
        if first_real.is_none() {
            first_real = Some((rel.clone(), raw.to_owned(), found[0].named.clone()));
        }
        let (bad, in_tests) = judge(&rel, &src, &found);
        in_test_fns += in_tests;
        mismatches.extend(bad);
    });
    assert!(
        stats.unreadable.is_empty(),
        "directories went unread, so this verdict covers part of the tree: {:?}",
        stats.unreadable
    );
    assert!(
        stats.files >= 100,
        "only {} .rs file(s) visited — the walk shrank, and a scan over a shrunken walk \
         agrees with anything",
        stats.files
    );
    assert!(
        subjects >= 80,
        "only {subjects} skip message(s) name a test — 93 were measured across 49 files \
         when this gate was written, so the matcher has stopped matching and this gate's \
         passing output is now identical to its not-running output (plan §3 rule 22)"
    );
    assert!(
        files_with_subjects.len() >= 40,
        "skip messages were found in only {} file(s); 49 carried one when this gate was \
         written, so the walk is reaching only part of the tree",
        files_with_subjects.len()
    );
    assert!(
        in_test_fns >= 80,
        "only {in_test_fns} of {subjects} skip messages sit in a `#[test]` fn — the \
         enclosing-fn resolution is landing somewhere other than the tests, which would \
         make the comparison above meaningless"
    );

    // 3. The comparison, exercised on this tree's own bytes: rename one real subject and
    //    require the report to name it. Taken from the scan rather than typed, so this
    //    proof cannot go stale when a test is renamed.
    let (rel, raw, named) = first_real.expect("the scan found at least one skip message");
    let corrupt = raw.replacen(
        &format!("{q}SKIP {named}:"),
        &format!("{q}SKIP {named}_typo:"),
        1,
    );
    assert_ne!(
        corrupt, raw,
        "the planted rename never applied to {rel}, so the proof below asserts nothing"
    );
    let corrupt = scan(&corrupt);
    let (planted, _) = judge(&rel, &corrupt, &message_subjects(&corrupt));
    assert!(
        planted.iter().any(|m| m.named == format!("{named}_typo")),
        "renaming a real skip line's subject in {rel} produced no report naming it: this \
         gate cannot notice the rename it exists for. Reported instead: {planted:?}"
    );

    // 4. The verdict.
    assert!(
        mismatches.is_empty(),
        "a skip message names a test other than the one printing it — the message is \
         false on the box that prints it (plan §3 rule 11), and an operator grepping the \
         suite output for that test finds nothing and concludes it ran:\n  {}\n\
         Fix the message, or pass the name in the way `skip_no_rig` and friends do — a \
         name that arrives as an argument cannot go stale.",
        mismatches
            .iter()
            .map(Mismatch::to_string)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// (2) Skip helper call sites
// ---------------------------------------------------------------------------

#[test]
fn a_skip_helper_call_naming_a_test_names_its_enclosing_fn() {
    let root = repo_root();
    let q = '"';

    // 0. The matcher, both halves — the roster derivation and the call-site read.
    let helper_src = format!(
        "pub fn skip_planted(test: &str, why: &str) {{\n    assert!(ok, {q}{{test}} {{why}}{q});\n    eprintln!({q}SKIP {{test}}: {{why}}{q});\n}}\n"
    );
    let helpers = skip_helpers(&scan(&helper_src));
    assert_eq!(
        helpers.get("skip_planted"),
        Some(&0),
        "the roster derivation misses the shape every shared skip helper in \
         itest/src/lib.rs is written in; it derived {helpers:?}"
    );

    // The name parameter is not always first, and the index is what a call site is read
    // with — pinning it at 0 would silently leave unchecked every helper that moves it.
    let second = format!(
        "fn skip_second(flag: bool, name: &str) {{\n    eprintln!({q}SKIP {{name}}: {{flag}}{q});\n}}\n"
    );
    assert_eq!(
        skip_helpers(&scan(&second)).get("skip_second"),
        Some(&1),
        "the roster derivation assumes the name parameter is the first one"
    );

    // A function that prints a skip line naming a *literal* is not a helper: its callers
    // type nothing, and enumerating it would make every call to it a subject.
    let not_a_helper =
        format!("fn plain() {{\n    eprintln!({q}SKIP plain: nothing to do{q});\n}}\n");
    assert!(
        skip_helpers(&scan(&not_a_helper)).is_empty(),
        "a plain skip line was read as a helper signature"
    );

    // The call sites, in the spellings the tree uses.
    let calls = format!(
        "{helper_src}\n#[test]\nfn planted_test() {{\n    skip_planted({q}planted_test{q}, {q}a, with a comma{q});\n}}\n\n#[test]\nfn wrapped_test() {{\n    serial_nexus_itest::skip_planted(\n        {q}wrapped_test{q},\n        &why,\n    );\n}}\n\n#[test]\nfn dynamic_test() {{\n    skip_planted(&name, {q}x{q});\n}}\n"
    );
    let calls_src = scan(&calls);
    let helpers = skip_helpers(&calls_src);
    let found = helper_call_subjects(&calls_src, &helpers);
    assert_eq!(
        found.iter().map(|s| s.named.as_str()).collect::<Vec<_>>(),
        vec!["planted_test", "wrapped_test"],
        "the call-site reader must see the one-line call and the path-qualified \
         multi-line one, must split an argument list whose second argument contains a \
         comma inside its string, and must see neither the helper's own definition nor \
         the call whose name is a variable; it found {found:?}"
    );
    let (bad, in_tests) = judge("planted.rs", &calls_src, &found);
    assert!(bad.is_empty(), "correct call sites were reported: {bad:?}");
    assert_eq!(
        in_tests, 2,
        "the #[test] walk did not recognise the planted tests around the call sites"
    );

    // A call whose callee merely *ends* with a helper's name is not that helper.
    let lookalike = format!(
        "{helper_src}\n#[test]\nfn planted_test() {{\n    my_skip_planted({q}other_test{q}, {q}x{q});\n}}\n"
    );
    let lookalike_src = scan(&lookalike);
    assert!(
        helper_call_subjects(&lookalike_src, &skip_helpers(&lookalike_src)).is_empty(),
        "a call to `my_skip_planted` was attributed to `skip_planted`, so an unrelated \
         function could redden this gate"
    );

    // …and the violation.
    let wrong = calls.replace(
        &format!("{q}wrapped_test{q},"),
        &format!("{q}planted_test{q},"),
    );
    assert_ne!(wrong, calls, "the planted rename was never applied");
    let wrong_src = scan(&wrong);
    let (bad, _) = judge(
        "planted.rs",
        &wrong_src,
        &helper_call_subjects(&wrong_src, &skip_helpers(&wrong_src)),
    );
    assert_eq!(
        bad.len(),
        1,
        "a helper call naming the wrong test was not reported. Reported: {bad:?}"
    );
    assert!(
        bad[0].to_string().contains("wrapped_test"),
        "the report does not name the enclosing fn: {}",
        bad[0]
    );

    // 1. The real roster, derived from the whole tree, because helpers and their callers
    //    live in different crates: `skip_no_tls` is defined in itest/src/lib.rs and
    //    called from itest/tests/.
    let mut roster: BTreeMap<String, usize> = BTreeMap::new();
    let stats = walk_rs(&root, &mut |_p, raw| {
        roster.extend(skip_helpers(&scan(raw)));
    });
    assert!(
        stats.unreadable.is_empty(),
        "directories went unread while deriving the helper roster: {:?}",
        stats.unreadable
    );
    assert!(
        stats.files >= 100,
        "only {} .rs file(s) visited while deriving the roster",
        stats.files
    );
    assert!(
        roster.len() >= 8,
        "the tree defines {} skip helper(s); itest/src/lib.rs alone carries six and \
         three more are file-local, so the derivation has stopped deriving and every \
         call site below goes unchecked: {roster:?}",
        roster.len()
    );

    // 2. The walker, against a locally-derived roster.
    let scratch = Scratch::new("calls");
    scratch.write("deep/nested/planted.rs", &wrong);
    let mut planted: Vec<Mismatch> = Vec::new();
    let planted_stats = walk_rs(scratch.path(), &mut |p, raw| {
        let src = scan(raw);
        let local = skip_helpers(&src);
        let (bad, _) = judge(
            &p.display().to_string(),
            &src,
            &helper_call_subjects(&src, &local),
        );
        planted.extend(bad);
    });
    assert_eq!(
        planted_stats.files, 1,
        "the planted walk visited an unexpected file count"
    );
    assert_eq!(
        planted.len(),
        1,
        "the walk did not reach a planted .rs file two directories down. Found: {planted:?}"
    );

    // 3. The real scan.
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut subjects = 0usize;
    let mut in_test_fns = 0usize;
    let mut first_real: Option<(String, String, String)> = None;
    walk_rs(&root, &mut |p, raw| {
        let rel = rel_of(&root, p);
        let src = scan(raw);
        let found = helper_call_subjects(&src, &roster);
        if found.is_empty() {
            return;
        }
        subjects += found.len();
        if first_real.is_none() {
            first_real = Some((rel.clone(), raw.to_owned(), found[0].named.clone()));
        }
        let (bad, in_tests) = judge(&rel, &src, &found);
        in_test_fns += in_tests;
        mismatches.extend(bad);
    });
    assert!(
        subjects >= 35,
        "only {subjects} skip helper call(s) pass a literal test name — 41 were measured \
         when this gate was written, so the call-site reader has stopped \
         reading (plan §3 rule 22)"
    );
    assert!(
        in_test_fns >= 35,
        "only {in_test_fns} of {subjects} helper calls sit in a `#[test]` fn — the \
         enclosing-fn resolution is landing somewhere other than the tests"
    );

    // 4. The comparison, on this tree's own bytes.
    let (rel, raw, named) = first_real.expect("the scan found at least one helper call");
    let corrupt = raw.replacen(&format!("{q}{named}{q}"), &format!("{q}{named}_typo{q}"), 1);
    assert_ne!(
        corrupt, raw,
        "the planted rename never applied to {rel}, so the proof below asserts nothing"
    );
    let corrupt = scan(&corrupt);
    let (planted, _) = judge(&rel, &corrupt, &helper_call_subjects(&corrupt, &roster));
    assert!(
        planted.iter().any(|m| m.named == format!("{named}_typo")),
        "renaming a real helper call's test argument in {rel} produced no report naming \
         it. Reported instead: {planted:?}"
    );

    // 5. The verdict.
    assert!(
        mismatches.is_empty(),
        "a skip helper is called with a test name other than its caller's — the helper \
         prints that name verbatim, so the skip line is false on the box printing it \
         (plan §3 rule 11):\n  {}",
        mismatches
            .iter()
            .map(Mismatch::to_string)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
