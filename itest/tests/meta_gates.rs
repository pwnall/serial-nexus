//! Meta-gates ported from phase 0: the checks that guard the codebase itself rather
//! than the daemon. From `scripts/validate/phase0/unsafe-gate.sh` (design §16.3 —
//! `unsafe` confined to `serial-nexus-sys`) and `phase0/doctor.sh` (§15.17 — no probe reports
//! `unsupported`). Portable Rust: no `jq`, no shell `grep`.
//!
//! (The license gate is now `tests/p0_license_gate.rs`, which self-skips without
//! cargo-deny; §16.11 folded the last three shell scripts into Rust, so no bash remains.)
//!
//! ## The invariant tripwires (review 26)
//!
//! Two of AGENTS.md §4's invariants were upheld by review alone, and one of them —
//! invariant 5's clippy `RefCell` ban — **silently stopped working for a whole release**
//! when the daemon's state moved out of the thin binary crate (`daemon-bin/`) into the
//! sibling library crate `serial-nexus-daemon` (`daemon/`) at the v8 library/binary split (INV5-CLIPPY-SCOPE: clippy resolves `clippy.toml`
//! upward through *ancestors*, and a sibling is not one). Nothing noticed, because
//! nothing was watching. So the gates below watch:
//!
//! * [`refcell_ban_covers_every_crate_that_holds_daemon_state`] — invariant 5. Greps for
//!   a raw `RefCell` in the ban crates' sources, checks each ban crate really carries a
//!   `clippy.toml` next to its manifest, and — the part that would have caught the
//!   original break — asserts that **every** crate whose sources reach for
//!   `CriticalCell` is in the ban list. A future crate move now fails a test instead of
//!   quietly disarming the lint.
//! * [`no_asyncfd_is_used_anywhere_in_the_workspace`] — invariant 1 (INV1-NO-GUARD),
//!   which had no gate at all. `AsyncFd`'s epoll readiness busy-loops on a pty master
//!   and starves the current-thread runtime (§15.18/§15.19); the only occurrences in the
//!   tree are prose explaining that, so the gate looks at *code* and the allowlist is
//!   empty.
//!
//! Both, like [`unsafe_is_confined_to_serial_nexus_sys`], carry a **planted-violation
//! self-proof**: a gate whose detector silently stopped detecting is the failure mode
//! being guarded against, so each proves it catches a synthetic offender (and, since
//! these two must read past legitimate prose, that it does *not* trip on a comment)
//! before it trusts its own clean verdict.
//!
//! That proof used to stop at the *matcher* (review 37, 37-TEST-1): the planted
//! offender was a string, never a file, so nothing exercised [`walk_rs`],
//! [`sources_under`] or [`crate_dirs`] — the walkers that have to find it. Both
//! swallowed `read_dir` failure, so a walk that had shrunk to one directory, or to
//! none, reported the same green as a clean tree, for the two gates enforcing
//! invariants 1 and 5. Each scanning gate now takes the shape `meta_names.rs`
//! already had: **plant a file in a scratch tree and require the walker to surface
//! it**, then assert a floor on what the real scan visited and that no directory went
//! unread. Non-vacuity witnesses (`sys/` carries the `unsafe`; `daemon/src/cell.rs`
//! wraps the sanctioned cell; `poll_ready`/`poll_blocking` exist) are collected
//! *through the same walk* rather than by reading a known path behind its back —
//! "the property holds" and "the scan reached the file" are one claim, and reading
//! the file separately answered only the first.
//!
//! ## The entry-point doc links (review 32)
//!
//! [`entry_point_doc_links_resolve`] is the newest member and the one with the longest
//! rap sheet: README's documentation index has pointed at a *deleted* design/plan pair
//! in three consecutive review generations (review 19 DOC-5, review 26 DOC-5, review 32
//! DOCR-1/DOCR-5). Each was patched by hand and each broke again the next time the pair
//! was version-bumped, because "the newest pair lives in `docs/`, superseded ones move
//! to `docs/historical/`" (§9) is a *rename* every generation and nothing checked the
//! two files that are a newcomer's first stop. That is a gate's job, not a reviewer's.
//!
//! Its first version checked markdown *links* only, which covered README's index and
//! missed the other half: AGENTS §2 names the current pair in
//! **backticks**, and a backtick is not a link. So the shape with the longest rap sheet
//! was the shape still unguarded (review-32 audit).
//! [`entry_point_design_and_plan_names_resolve`] closes it — any backticked
//! `NN-design-…md` / `NN-implementation-plan-…md` in either entry point must name a file
//! that exists, and one written without a directory must exist directly under `docs/`,
//! which is precisely what stops being true the moment a generation is superseded. An
//! explicitly-pathed `docs/historical/…` reference still resolves, because naming a
//! superseded pair *as* historical is legitimate.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use serial_nexus_itest::bin;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/itest — the *directory*, which §15.40 kept short.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Does `src` contain a real `unsafe` *usage* — the keyword as a whole word followed
/// by a block/fn/impl/trait/extern? Mirrors the bash gate's `\bunsafe\b\s*(\{|fn|impl|
/// trait|extern)`, so `#![forbid(unsafe_code)]` (word `unsafe_code`) never trips it.
fn has_unsafe_usage(src: &str) -> bool {
    let b = src.as_bytes();
    let is_word = |c: u8| c == b'_' || c.is_ascii_alphanumeric();
    let mut i = 0;
    while let Some(pos) = src[i..].find("unsafe") {
        let start = i + pos;
        let end = start + "unsafe".len();
        let before_ok = start == 0 || !is_word(b[start - 1]);
        let after_ok = b.get(end).map(|&c| !is_word(c)).unwrap_or(true);
        if before_ok && after_ok {
            let rest = src[end..].trim_start();
            if rest.starts_with('{')
                || rest.starts_with("fn")
                || rest.starts_with("impl")
                || rest.starts_with("trait")
                || rest.starts_with("extern")
            {
                return true;
            }
        }
        i = end;
    }
    false
}

/// This detector file, as a **repo-relative path** (review 37, 37-TEST-2).
///
/// Every gate below names the tokens it scans for, so each excludes this file from
/// its own scan. The exclusion used to be matched on `file_name()`, which is
/// depth-independent: a future `meta_gates.rs` *anywhere* in the workspace inherited
/// a blanket exemption from the `unsafe`, `AsyncFd` and `CriticalCell` scans that
/// nobody had stated — and for `AsyncFd` this file is invariant 1's only automated
/// enforcement. That is the same TESTR-7 correction [`REFCELL_EXEMPT`] already
/// carries, one instance over; [`self_exclusion_is_a_path_not_a_name`] proves it.
const THIS_FILE: &str = "itest/tests/meta_gates.rs";

/// The floor on `.rs` files a whole-tree [`walk_rs`] pass must reach.
///
/// The tree carries ~130 outside `fuzz/`. The floor exists because a walker that
/// quietly stopped walking reports the same green as a clean tree (review 37,
/// 37-TEST-1): `read_dir` failures were swallowed, so a scan over a shrunken file
/// set — one crate, or none — passed the ban gates for invariants 1 and 5. Set well
/// below the real count so ordinary deletions do not trip it, and far above the
/// single-crate degradation it exists to catch.
const MIN_RS_FILES: usize = 100;

/// The floor on crate roots [`crate_dirs`] must find. Fifteen today (twelve workspace
/// members plus the out-of-tree consumer template's three). Same reasoning as
/// [`MIN_RS_FILES`]: the completeness half of the `RefCell` ban is only as good as the
/// list of crates it ranges over.
const MIN_CRATE_DIRS: usize = 12;

/// `path` relative to `root`, with separators normalised, so a comparison against a
/// repo-relative literal holds off Unix.
fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// What one [`walk_rs`] pass actually did — the evidence a clean verdict needs.
///
/// `files` is the visited count a gate asserts a floor on; `unreadable` names the
/// directories `read_dir` refused for a reason other than "gone". A vanished
/// directory is a benign mid-walk race (a concurrent build removing a scratch tree);
/// a permission error is the silent degradation 37-TEST-1 named, and a gate that
/// walked half the tree must say so rather than report green.
#[derive(Default)]
struct WalkStats {
    files: usize,
    unreadable: Vec<String>,
}

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
                // Skip build output, VCS, the excluded fuzz crate, and vendored trees.
                if matches!(name.as_ref(), "target" | ".git" | "fuzz" | "node_modules") {
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

/// Is `path` this detector file, matched on its **whole** path below `root`?
fn is_this_file(root: &Path, path: &Path) -> bool {
    rel_path(root, path) == THIS_FILE
}

/// A scratch tree that removes itself on drop.
///
/// The planted-violation walks below assert *after* writing, so a failing assertion
/// unwinds past any hand-rolled cleanup; a guard keeps a red gate from leaving
/// litter under `/tmp` on every run. Named by pid + call site, so parallel test
/// binaries never share one.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("snx-meta-gates-{tag}-{}", std::process::id()));
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

/// Does `hay` contain `needle` as a whole word (Rust-identifier boundaries)? So
/// `RefCell` matches in `std::cell::RefCell::new` but not inside `MyRefCellish`.
fn contains_word(hay: &str, needle: &str) -> bool {
    let b = hay.as_bytes();
    let is_word = |c: u8| c == b'_' || c.is_ascii_alphanumeric();
    let mut i = 0;
    while let Some(pos) = hay[i..].find(needle) {
        let start = i + pos;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_word(b[start - 1]);
        let after_ok = b.get(end).map(|&c| !is_word(c)).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        i = end;
    }
    false
}

/// Does `src` use `token` as a whole word in **code** — i.e. in at least one line
/// outside a `//`-style comment?
///
/// Both banned tokens below are *named in prose* by the very files that explain why
/// they are banned (`sys/src/lib.rs` and `nodes/serial.rs` both discuss
/// `AsyncFd`; `daemon.rs` says raw `RefCell` is banned), so a bare substring grep
/// would be permanently red and would be deleted rather than fixed. Comments are
/// therefore skipped.
///
/// Known limits, both accepted deliberately: a `/* … */` block comment is not
/// recognised (none in this tree mentions either token, and a *false positive* there
/// is a loud test failure, not a silent hole), and a `//` inside a string literal
/// truncates the rest of that line. Neither can turn a real, idiomatic use of these
/// types — a `use` line, a type annotation, a `::new` call — invisible.
fn has_code_token(src: &str, token: &str) -> bool {
    src.lines().any(|line| {
        let code = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        contains_word(code, token)
    })
}

/// Every crate root in the tree (a directory holding a `Cargo.toml`), relative to
/// `root`, excluding the workspace manifest itself. Used to prove the `RefCell` ban
/// list is *complete* rather than merely non-empty.
///
/// `unreadable` collects the directories `read_dir` refused for a reason other than
/// "gone", for the same reason [`WalkStats`] carries the field: this walker feeds the
/// completeness half of invariant 5, and a crate it never reached is a crate whose
/// missing `clippy.toml` reads as compliance.
fn crate_dirs(root: &Path, unreadable: &mut Vec<String>) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, unreadable: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    unreadable.push(format!("{}: {e}", dir.display()));
                }
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Build output, VCS, vendored trees, and the workspace-excluded fuzz
            // crate (its own toolchain, no daemon state).
            if matches!(name.as_ref(), "target" | ".git" | "fuzz" | "node_modules") {
                continue;
            }
            if path.join("Cargo.toml").is_file() {
                out.push(path.clone());
            }
            walk(&path, out, unreadable);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, unreadable);
    out
}

/// Every `.rs` under `dir`, as (path, source), plus the walk's own evidence. The
/// tuple is deliberate: a caller that wants the sources has to hold the proof that
/// the walk producing them happened.
fn sources_under(dir: &Path) -> (Vec<(PathBuf, String)>, WalkStats) {
    let mut out = Vec::new();
    let stats = walk_rs(dir, &mut |p, src| {
        out.push((p.to_path_buf(), src.to_owned()))
    });
    (out, stats)
}

/// **The self-exclusion is a path, not a base name** (review 37, 37-TEST-2).
///
/// Called by each of the three scanning gates rather than written once as a test of
/// its own, because each of them *relies* on it: a widened exclusion is invisible
/// from inside the gate it silences. Synthetic paths only — nothing is written to
/// the tree, since the property under test is the matcher.
fn assert_self_exclusion_is_a_path(root: &Path) {
    assert!(
        is_this_file(root, &root.join(THIS_FILE)),
        "the detector no longer excludes itself ({THIS_FILE}) — it names every token \
         it scans for, so the gates would be red forever"
    );
    for impostor in [
        "daemon/src/meta_gates.rs",
        "web/src/server/meta_gates.rs",
        "sim/meta_gates.rs",
    ] {
        assert!(
            !is_this_file(root, &root.join(impostor)),
            "{impostor} is excluded from the unsafe / AsyncFd / CriticalCell scans by \
             nothing but its base name. The exemption covers exactly {THIS_FILE}: a \
             same-named file elsewhere would carry a blanket pass on invariants 1 and \
             5, and for AsyncFd this gate is the only automated enforcement there is"
        );
    }
}

/// The crates that must carry the `disallowed-types` ban on `std::cell::RefCell`
/// (AGENTS.md §4 invariant 5 / design §16.2). Two, not one: `serial-nexus-daemon` owns the
/// thin binary and kept the original file, `serial-nexus-daemon` owns every line of daemon
/// state since the v8 split (§15.26) and is where the ban actually bites.
const REFCELL_BAN_CRATES: &[&str] = &["daemon", "daemon-bin"];

/// The one sanctioned `RefCell` in the daemon: `CriticalCell`'s own internals, which
/// carry a localized `#[allow(clippy::disallowed_types)]` (§16.2).
///
/// **A repo-relative path, not a bare file name** (review 32 TESTR-7). The exemption
/// used to be matched against `path.file_name()`, which is depth-independent: any
/// future `cell.rs` anywhere under a ban crate's `src` — `daemon/src/nodes/
/// cell.rs` is the obvious one — inherited an exemption nobody had stated. That does
/// not lose the *clippy* half of invariant 5 (a plain raw `RefCell` in such a file is
/// still a lint error), but it loses this gate's independent half, which is precisely
/// its coverage of the one case clippy cannot see: a raw `RefCell` carrying a local
/// `#[allow(clippy::disallowed_types)]`. Two such allows exist today, both in the file
/// named below, and nothing bounds where a third might appear.
const REFCELL_EXEMPT: &[&str] = &["daemon/src/cell.rs"];

/// Is `path` the one file [`REFCELL_EXEMPT`] names, matched on its **whole** path
/// below `root`? Separators are normalised so the comparison holds off Unix.
fn is_refcell_exempt(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let rel = rel.to_string_lossy().replace('\\', "/");
    REFCELL_EXEMPT.contains(&rel.as_str())
}

#[test]
fn unsafe_is_confined_to_serial_nexus_sys() {
    let root = repo_root();
    assert_self_exclusion_is_a_path(&root);

    // 1. Prove the detector actually catches an `unsafe` usage. The sample is built by
    //    concatenation so this source file itself carries no literal match.
    let planted = format!("fn f() {{ {} {{ let _ = 1; }} }}", "unsafe");
    assert!(
        has_unsafe_usage(&planted),
        "the detector does not catch a planted unsafe usage"
    );

    // 1b. …and prove the **walker** reaches a file carrying it (review 37, 37-TEST-1).
    //     A matcher proved against a string says nothing about the walk that has to
    //     find the offender: `read_dir` failures were swallowed, so a walk that
    //     covered one directory — or none — reported the same green as a clean tree.
    //     Planted nested, beside a non-`.rs` decoy and a copy under `target/`, so this
    //     also pins the extension filter and the skip list the real scan depends on.
    let scratch = Scratch::new("unsafe");
    scratch.write("nested/deep/offender.rs", &planted);
    scratch.write("nested/deep/offender.txt", &planted);
    scratch.write("target/debug/build/offender.rs", &planted);
    let mut planted_hits = Vec::new();
    let stats = walk_rs(scratch.path(), &mut |path, src| {
        if has_unsafe_usage(src) {
            planted_hits.push(rel_path(scratch.path(), path));
        }
    });
    assert_eq!(
        planted_hits,
        vec!["nested/deep/offender.rs".to_owned()],
        "the walker missed a planted `unsafe` (or surfaced a file it must skip) — \
         this gate would pass over anything"
    );
    assert_eq!(
        stats.files, 1,
        "the walker visited {} files in a scratch tree holding exactly one `.rs` \
         outside target/ — the extension filter or the skip list has moved",
        stats.files
    );
    drop(scratch);

    // 2. No `.rs` outside `serial-nexus-sys` may contain an `unsafe` usage. The
    //    directory is `sys/`, not the crate name: §15.40 renamed the packages and
    //    deliberately left the tree layout short, so a gate that scans the
    //    filesystem must spell the *directory*.
    let mut offenders = Vec::new();
    let mut sys_files = 0usize;
    let mut sys_unsafe = 0usize;
    let stats = walk_rs(&root, &mut |path, src| {
        let rel = rel_path(&root, path);
        if rel.starts_with("sys/") {
            // Counted, not skipped: step 3 needs the extraction target proved through
            // this same walk rather than by reading a known file behind its back.
            sys_files += 1;
            if has_unsafe_usage(src) {
                sys_unsafe += 1;
            }
            return;
        }
        // Self-exclude this detector file (it names the keywords it scans for).
        if is_this_file(&root, path) {
            return;
        }
        if has_unsafe_usage(src) {
            offenders.push(rel);
        }
    });
    assert!(
        offenders.is_empty(),
        "`unsafe` found outside serial-nexus-sys/: {offenders:?}"
    );

    // 2b. …and the scan was real: the whole tree, not a shrunken slice of it.
    assert!(
        stats.unreadable.is_empty(),
        "directories the scan could not read: {:?} — every `.rs` under them went \
         unchecked, and this gate would still have reported green",
        stats.unreadable
    );
    assert!(
        stats.files >= MIN_RS_FILES,
        "only {} `.rs` files walked (floor {MIN_RS_FILES}) — the walker has stopped \
         seeing the tree, and a gate that checks nothing passes forever",
        stats.files
    );

    // 3. Sanity: serial-nexus-sys genuinely carries the unsafe (else the split is a
    //    lie) — established from the walk above, so "no offenders" cannot mean "the
    //    walker never got there".
    assert!(
        sys_files > 0 && sys_unsafe > 0,
        "the walk found {sys_unsafe} file(s) carrying `unsafe` among {sys_files} \
         under sys/ — serial-nexus-sys is the extraction target and must carry it, or \
         a clean verdict above means nothing was scanned"
    );
}

/// **Invariant 5 (INV5-CLIPPY-SCOPE).** Daemon state lives in `CriticalCell`, whose
/// contents are reachable only inside a synchronous `with`/`with_mut` closure, so a
/// borrow physically cannot cross an `.await` (§16.2). The clippy `disallowed-types`
/// ban is what makes that a compile-shape fact rather than a review item — and it
/// stopped covering the daemon for a whole release when the code moved to a sibling
/// crate, because clippy resolves `clippy.toml` upward through *ancestors* only.
///
/// Three assertions, in the order they would have caught that:
///
/// 1. no raw `RefCell` in a ban crate's sources (outside `CriticalCell`'s own file) —
///    the property the lint enforces, now also enforced where a crate move cannot
///    reach it;
/// 2. each ban crate really has a `clippy.toml` beside its manifest carrying the ban;
/// 3. **every crate whose sources use `CriticalCell` is in the ban list** — so moving
///    daemon state into a new crate fails this test until that crate gets its own
///    `clippy.toml`, instead of silently disarming the lint.
#[test]
fn refcell_ban_covers_every_crate_that_holds_daemon_state() {
    // 0. Prove the detector both fires and discriminates. Tokens are concatenated so
    //    this file carries no literal code occurrence of its own.
    let cell = format!("Ref{}", "Cell");
    let critical = format!("Critical{}", "Cell");
    let planted = format!("    let s = std::cell::{cell}::new(GraphState::new());");
    assert!(
        has_code_token(&planted, &cell),
        "the detector does not catch a planted `{cell}` usage"
    );
    // …and does NOT fire on the prose that legitimately names it (`daemon.rs:19`
    // says raw `RefCell` is banned). A gate that must be `#[allow]`ed away is a gate
    // that gets deleted.
    assert!(
        !has_code_token(
            &format!("//! banned in this crate: std::cell::{cell}"),
            &cell
        ),
        "the detector trips on a comment mentioning `{cell}` — it would be red forever"
    );

    let root = repo_root();
    assert_self_exclusion_is_a_path(&root);

    // 0b. …and prove the *exemption* is a path, not a name (review 32 TESTR-7). The
    //     sanctioned file is exempt; a same-named file at another path is not. Both
    //     probes are synthetic paths — nothing is written to the tree — because the
    //     property under test is the matcher, and a matcher that silently widened is
    //     this file's whole reason to exist.
    assert!(
        is_refcell_exempt(&root, &root.join("daemon/src/cell.rs")),
        "the sanctioned {critical_hint} file is no longer exempt — the ban list would \
         be red forever",
        critical_hint = REFCELL_EXEMPT[0]
    );
    for impostor in [
        "daemon/src/nodes/cell.rs",
        "daemon-bin/src/cell.rs",
        "daemon/src/state/cell.rs",
    ] {
        assert!(
            !is_refcell_exempt(&root, &root.join(impostor)),
            "{impostor} is exempt from the {cell} scan by nothing but its base name. \
             The exemption covers exactly {:?}: a second `#[allow]`ed {cell} anywhere \
             else is invisible to clippy by construction and would be invisible here \
             too (§16.2, invariant 5)",
            REFCELL_EXEMPT
        );
    }

    // 0c. …and prove both **walkers** this gate rides on actually walk (review 37,
    //     37-TEST-1). `sources_under` must surface a planted offender nested under a
    //     scratch crate, and `crate_dirs` must find that crate's root — the two
    //     mechanisms behind assertions 1 and 3, neither of which had ever been driven
    //     over a file. Both tokens are planted in both spellings the matchers claim to
    //     cover: a code line (a hit) and a comment (not one).
    //     The planted crate sits one level down, because a crate root the walker never
    //     descends to is exactly the "new crate holds daemon state" case assertion 3
    //     exists for.
    let scratch = Scratch::new("refcell");
    scratch.write(
        "workspace/acrate/Cargo.toml",
        "[package]\nname = \"planted\"\n",
    );
    scratch.write(
        "workspace/acrate/src/nodes/holder.rs",
        &format!("{planted}\n    let state = {critical}::new(GraphState::new());\n"),
    );
    scratch.write(
        "workspace/acrate/src/prose.rs",
        &format!("//! this crate uses neither std::cell::{cell} nor {critical}\n"),
    );
    let (sources, stats) = sources_under(&scratch.path().join("workspace/acrate/src"));
    let hits = |token: &str| -> Vec<String> {
        let mut hits: Vec<String> = sources
            .iter()
            .filter(|(_, src)| has_code_token(src, token))
            .map(|(p, _)| rel_path(scratch.path(), p))
            .collect();
        hits.sort();
        hits
    };
    let want = vec!["workspace/acrate/src/nodes/holder.rs".to_owned()];
    assert_eq!(
        hits(&cell),
        want,
        "the source walker missed a planted `{cell}` (or tripped on the comment that \
         merely names it) — assertion 1 below would pass over anything"
    );
    assert_eq!(
        hits(&critical),
        want,
        "the source walker missed a planted `{critical}` — assertion 3's completeness \
         claim rests on this exact scan"
    );
    assert_eq!(
        stats.files, 2,
        "the source walker visited {} files where two were planted",
        stats.files
    );
    let mut scratch_unreadable = Vec::new();
    let scratch_crates = crate_dirs(scratch.path(), &mut scratch_unreadable);
    assert_eq!(
        scratch_crates
            .iter()
            .map(|d| rel_path(scratch.path(), d))
            .collect::<Vec<_>>(),
        vec!["workspace/acrate".to_owned()],
        "the crate walker missed a planted crate root — assertion 3's completeness \
         claim is only as wide as the crate list it ranges over"
    );
    assert!(
        scratch_unreadable.is_empty(),
        "the crate walker could not read {scratch_unreadable:?} in a tree it just created"
    );
    drop(scratch);

    // The sanctioned `CriticalCell` file, as the ban-crate walk below finds it — the
    // non-vacuity witness for assertions 1 and 3, collected *through the walker*
    // rather than read behind its back at the end.
    let mut exempt_witness: Option<String> = None;

    for krate in REFCELL_BAN_CRATES {
        let dir = root.join(krate);

        // 1. No raw RefCell in the crate's sources, the one exempt path excepted.
        let mut offenders = Vec::new();
        let (sources, stats) = sources_under(&dir.join("src"));
        // Execution, not existence (AGENTS §3): `dir.is_dir()` said the ban-list crate
        // was there and nothing said its sources had been read. A ban crate with no
        // reachable sources is the shrunken-scan failure, wearing a green tick.
        assert!(
            stats.unreadable.is_empty(),
            "directories under {krate}/src the scan could not read: {:?}",
            stats.unreadable
        );
        assert!(
            stats.files > 0,
            "no sources walked under {krate}/src — ban-list crate {krate} is gone, \
             moved, or unreadable, and assertion 1 just passed over nothing"
        );
        for (path, src) in sources {
            if is_refcell_exempt(&root, &path) {
                if has_code_token(&src, &cell) && has_code_token(&src, &critical) {
                    exempt_witness = Some(rel_path(&root, &path));
                }
                continue;
            }
            if has_code_token(&src, &cell) {
                offenders.push(rel_path(&root, &path));
            }
        }
        assert!(
            offenders.is_empty(),
            "raw `{cell}` outside {REFCELL_EXEMPT:?}: {offenders:?} — daemon state \
             belongs in `CriticalCell` (§16.2)",
        );

        // 2. The lint file exists beside the manifest and carries the ban. This is
        //    the file whose absence was the original defect.
        let clippy_toml = dir.join("clippy.toml");
        let text = std::fs::read_to_string(&clippy_toml).unwrap_or_else(|e| {
            panic!(
                "{krate} bans `{cell}` but has no clippy.toml at {} ({e}) — clippy \
                 resolves clippy.toml from CARGO_MANIFEST_DIR upward through \
                 ANCESTORS, so a sibling crate's copy does not apply",
                clippy_toml.display()
            )
        });
        assert!(
            text.contains("disallowed-types") && text.contains(&format!("std::cell::{cell}")),
            "{} does not ban std::cell::{cell} via disallowed-types",
            clippy_toml.display()
        );
    }

    // 3. Completeness: `CriticalCell` is the daemon-state wrapper the ban exists to
    //    force. Any crate reaching for it is a crate the ban must cover.
    let mut unreadable = Vec::new();
    let crates = crate_dirs(&root, &mut unreadable);
    assert!(
        unreadable.is_empty(),
        "directories the crate walk could not read: {unreadable:?} — a crate root \
         under them is a crate this completeness claim never ranged over"
    );
    assert!(
        crates.len() >= MIN_CRATE_DIRS,
        "only {} crate roots found (floor {MIN_CRATE_DIRS}) — the crate walker has \
         stopped seeing the tree, and assertion 3 is only as complete as this list",
        crates.len()
    );
    let mut uncovered = Vec::new();
    for dir in crates {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if REFCELL_BAN_CRATES.contains(&name.as_str()) {
            continue;
        }
        let uses_it = sources_under(&dir.join("src"))
            .0
            .iter()
            .any(|(p, src)| !is_this_file(&root, p) && has_code_token(src, &critical));
        if uses_it {
            uncovered.push(rel_path(&root, &dir));
        }
    }
    assert!(
        uncovered.is_empty(),
        "these crates hold daemon state ({critical}) but are not in REFCELL_BAN_CRATES: \
         {uncovered:?} — add a clippy.toml beside each manifest and list it here \
         (this is exactly how the ban broke at the v8 library/binary split)"
    );

    // Sanity: the ban list is not vacuous — `serial-nexus-daemon` genuinely carries the
    // sanctioned `CriticalCell`, so assertions 1 and 3 are scanning for something real.
    // The witness comes from the ban-crate walk above, not from a direct read of a
    // known path: "the exemption is live" and "the walk reached the exempt file" are
    // the same claim, and reading the file separately answered only the first.
    assert_eq!(
        exempt_witness.as_deref(),
        Some(REFCELL_EXEMPT[0]),
        "the ban-crate walk did not find a {cell} wrapped in a {critical} at \
         {:?} — either the exemption is stale or the walk never reached it",
        REFCELL_EXEMPT[0]
    );
}

/// **Invariant 1 (INV1-NO-GUARD).** `tokio::io::unix::AsyncFd`'s epoll readiness
/// fires spuriously and persistently on a pty master — `read(2)` returns EAGAIN
/// while epoll insists otherwise — and because the ready future completes
/// synchronously the loop never yields, starving the current-thread runtime and
/// freezing the control plane with it (§15.18/§15.19). Readiness for tty-family fds
/// is `serial_nexus_sys::poll_ready`/`poll_blocking` instead.
///
/// That was upheld by review alone until this gate. **The allowlist is empty and must
/// stay empty**: every occurrence in the tree today is prose explaining the ban (in
/// `sys/src/lib.rs`, `runtime.rs`, `nodes/pty.rs`, `nodes/serial.rs`, AGENTS.md),
/// which [`has_code_token`] skips. A legitimate future use would have to be on a
/// non-tty fd and would belong here with its justification — not silently in a node.
#[test]
fn no_asyncfd_is_used_anywhere_in_the_workspace() {
    let token = format!("Async{}", "Fd");
    let root = repo_root();
    assert_self_exclusion_is_a_path(&root);

    // 1. Prove the detector catches a real use, and does not trip on the doc comments
    //    that explain why there are none.
    for planted in [
        format!("use tokio::io::unix::{token};"),
        format!("    let afd = {token}::new(fd)?;"),
        format!("    let guard: {token}<OwnedFd> = registered;"),
    ] {
        assert!(
            has_code_token(&planted, &token),
            "the detector does not catch a planted usage: {planted}"
        );
    }
    assert!(
        !has_code_token(
            &format!("//! never `{token}`, whose epoll readiness spins"),
            &token
        ),
        "the detector trips on the prose that documents the ban"
    );

    // 1b. …and prove the **walker** reaches the file carrying one (review 37,
    //     37-TEST-1). Both spellings the matcher claims to cover are planted, in
    //     separate nested files, so this pins matcher *and* walk in one pass: this
    //     gate is invariant 1's only automated enforcement, and a walk that had
    //     quietly shrunk to nothing reported the same green as a clean tree.
    let scratch = Scratch::new("asyncfd");
    scratch.write(
        "krate/src/nodes/user.rs",
        &format!("use tokio::io::unix::{token};\n"),
    );
    scratch.write(
        "krate/src/nodes/prose.rs",
        &format!("//! readiness is never `{token}` here — it spins on a pty master\n"),
    );
    let mut planted_hits = Vec::new();
    let stats = walk_rs(scratch.path(), &mut |path, src| {
        if has_code_token(src, &token) {
            planted_hits.push(rel_path(scratch.path(), path));
        }
    });
    assert_eq!(
        planted_hits,
        vec!["krate/src/nodes/user.rs".to_owned()],
        "the walker missed a planted `{token}` use (or tripped on the comment beside \
         it) — invariant 1's only gate would pass over anything"
    );
    assert_eq!(
        stats.files, 2,
        "the walker visited {} files where two were planted",
        stats.files
    );
    drop(scratch);

    // 2. No `.rs` in the workspace may use it. No allowlist (see the doc comment).
    let mut offenders = Vec::new();
    let mut replacement_seen = 0usize;
    let stats = walk_rs(&root, &mut |path, src| {
        let rel = rel_path(&root, path);
        // Step 3's witness, collected through this same walk: `sys/` is where the
        // sanctioned replacement lives, and "nobody uses AsyncFd" only means
        // "poll(2) everywhere" if the walk actually reached it.
        if rel.starts_with("sys/")
            && has_code_token(src, "poll_ready")
            && has_code_token(src, "poll_blocking")
        {
            replacement_seen += 1;
        }
        if is_this_file(&root, path) {
            return;
        }
        if has_code_token(src, &token) {
            offenders.push(rel);
        }
    });
    assert!(
        offenders.is_empty(),
        "`{token}` is used in {offenders:?} — invariant 1 forbids it on pty/tty fds \
         (it busy-loops and starves the runtime, §15.18); use `serial_nexus_sys::poll_ready` \
         / `poll_blocking`"
    );

    // 2b. …over the whole tree, not a shrunken slice of it.
    assert!(
        stats.unreadable.is_empty(),
        "directories the scan could not read: {:?} — every `.rs` under them went \
         unchecked, and this gate would still have reported green",
        stats.unreadable
    );
    assert!(
        stats.files >= MIN_RS_FILES,
        "only {} `.rs` files walked (floor {MIN_RS_FILES}) — the walker has stopped \
         seeing the tree, and a gate that checks nothing passes forever",
        stats.files
    );

    // 3. Sanity: the replacement the invariant names is really what the tree uses, so
    //    a clean verdict means "poll(2) everywhere", not "no readiness code left".
    assert!(
        replacement_seen > 0,
        "the walk found no file under sys/ exposing poll_ready and poll_blocking — \
         invariant 1's replacement is gone (or was never walked) and this gate is \
         measuring nothing"
    );
}

#[test]
fn doctor_reports_no_unsupported_capability() {
    let out = Command::new(bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("run serial-nexus-doctor");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse doctor json: {e}; stdout={:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });

    // No probe may contradict the design (§15.17). `skipped`/`degraded` are fine.
    assert_eq!(
        v["summary"]["unsupported"],
        json!(0),
        "a capability is unsupported: {}",
        v["summary"]
    );

    let status = |id: &str| -> String {
        v["probes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == json!(id))
            .unwrap_or_else(|| panic!("probe {id} missing"))["status"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // P2 (POLLHUP presence): `supported` on Linux; `supported` or `degraded` elsewhere
    // (the §7.2 platform arm on BSD/macOS). P1 (EXTPROC) may always degrade to poll-only.
    let p2 = status("P2");
    #[cfg(target_os = "linux")]
    assert_eq!(p2, "supported", "P2 must be supported on Linux, was {p2}");
    #[cfg(not(target_os = "linux"))]
    assert!(p2 == "supported" || p2 == "degraded", "P2 was {p2}");

    let p1 = status("P1");
    assert!(p1 == "supported" || p1 == "degraded", "P1 was {p1}");
}

/// The floor on fuzz target sources the `unstable_fuzz_api` gate must read. Nine
/// today; the rule below is only as strong as the corpus it searches.
const MIN_FUZZ_TARGETS: usize = 5;

/// `src` with every `//`-style comment removed, line structure preserved.
///
/// The same conservative rule [`has_code_token`] documents, with the same accepted
/// limits (no `/* … */`, and a `//` inside a string literal truncates its line), and
/// for the same reason: prose about a token is not a use of it.
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The body of the block `header` introduces in `src`, delimited by **brace
/// counting**. `None` if the header is absent or its braces never balance.
///
/// Call it on comment-stripped source ([`strip_line_comments`]) so a brace inside a
/// comment cannot move the boundary.
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

/// Every identifier `body`'s `pub use` statements re-export — the last `::` segment
/// of each item, brace groups flattened.
fn reexported_items(body: &str) -> Vec<String> {
    body.split("pub use")
        .skip(1)
        .flat_map(|u| u[..u.find(';').unwrap_or(u.len())].split(['{', '}', ',']))
        .filter_map(|t| t.rsplit("::").next())
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .map(str::to_owned)
        .collect()
}

/// The `unstable_fuzz_api` bargain, enforced (design §15.26 amendment, notes §3.19).
///
/// `serial-nexus-daemon` and `serial-nexus-web` each expose a `pub mod unstable_fuzz_api` that
/// re-exports internals the fuzz harness drives — a deliberate, named exception to
/// §15.26's "everything else stays private", taken because the alternative (extracting
/// two parsers into crates of their own) costs more than it buys. The exception is
/// bounded by one rule: **an item re-exported there must have a fuzz target driving
/// it.** Without that, the module is just a hole in the API boundary that grows
/// whenever something is inconvenient to reach.
///
/// That is a rule someone has to remember, which is exactly the shape of the bug this
/// file exists because of (INV5-CLIPPY-SCOPE: a configuration gate that disarmed
/// silently and went unnoticed for a release). So it is a test.
///
/// Also asserts each module says what it is: the doc comment must disclaim stability,
/// because the whole justification for the exception is that no embedder could depend
/// on it by accident.
///
/// Two matcher corrections (review 37, 37-TEST-6), both latent when they were filed:
/// the search for a driving target ran over **raw** target sources, so a re-export
/// merely *named in a comment* satisfied the rule; and the module body was delimited
/// by the first line-start `}`, so the first nested block inside either module would
/// have exempted every re-export after it. Both are proved by the planted cases below.
#[test]
fn every_unstable_fuzz_api_export_has_a_fuzz_target() {
    const HEADER: &str = "pub mod unstable_fuzz_api";
    let root = repo_root();
    let fuzz_dir = root.join("fuzz/fuzz_targets");
    let target_sources: Vec<String> = std::fs::read_dir(&fuzz_dir)
        .unwrap_or_else(|e| {
            panic!(
                "read {}: {e} — the §15.26 exception below has no consumer",
                fuzz_dir.display()
            )
        })
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect();
    assert!(
        target_sources.len() >= MIN_FUZZ_TARGETS,
        "only {} fuzz target(s) read (floor {MIN_FUZZ_TARGETS}) — a rule whose corpus \
         stopped being read passes forever",
        target_sources.len()
    );
    // Comments stripped: a target that only *mentions* a re-export in prose is not a
    // target driving it, and the whole exception is bounded by "the fuzzer drives it".
    let targets = strip_line_comments(&target_sources.join("\n"));

    // 0. Self-proof, both matchers, both directions. The token is assembled so this
    //    file carries no literal of its own.
    let planted = format!("Planted{}Parser", "Fuzz");
    assert!(
        contains_word(
            &strip_line_comments(&format!("use serial_nexus_daemon::x::{planted};")),
            &planted
        ),
        "the comment stripper eats a real `use` line"
    );
    assert!(
        !contains_word(
            &strip_line_comments(&format!("// also worth fuzzing one day: {planted}")),
            &planted
        ),
        "a re-export named only in a fuzz target's comment counts as driven — the one \
         rule bounding the §15.26 exception is satisfied by prose"
    );
    // …and the body delimiter counts braces instead of stopping at the first
    // line-start `}`. The planted module puts a nested block *before* its re-export,
    // which is precisely the shape that truncated the old slice.
    let nested = [
        "pub mod unstable_fuzz_api {",
        "    pub fn helper() -> u8 {",
        "        1",
        "}",
        &format!("    pub use crate::control::{planted};"),
        "}",
    ]
    .join("\n");
    let nested_body = braced_body(&nested, HEADER).expect("the planted module is brace-delimited");
    assert!(
        reexported_items(nested_body).contains(&planted),
        "the module-body delimiter stops at the first line-start `}}` — every \
         re-export past the first nested block is silently exempt from the rule this \
         gate exists to enforce"
    );

    let mut checked = 0usize;
    for krate in ["daemon", "web"] {
        let lib = root.join(krate).join("src/lib.rs");
        let src =
            std::fs::read_to_string(&lib).unwrap_or_else(|e| panic!("read {}: {e}", lib.display()));
        let Some(at) = src.find(HEADER) else {
            continue; // the module is allowed to disappear — that is its promise
        };

        // The disclaimer is load-bearing: it is what makes "no embedder can depend on
        // this by accident" true rather than hopeful. Read from the raw source — the
        // disclaimer *is* a comment.
        let doc = &src[..at];
        assert!(
            doc.contains("Not part of") && (doc.contains("fuzz harness") || doc.contains("fuzz")),
            "{krate}'s unstable_fuzz_api must document that it is unsupported"
        );

        // Every identifier in the module's `pub use` list must appear in some target.
        let code = strip_line_comments(&src);
        let body = braced_body(&code, HEADER)
            .unwrap_or_else(|| panic!("{krate}'s unstable_fuzz_api body is not brace-balanced"));
        for item in reexported_items(body) {
            assert!(
                contains_word(&targets, &item),
                "{krate}::unstable_fuzz_api re-exports `{item}`, but no fuzz target \
                 under fuzz/fuzz_targets/ drives it (a mention in a comment does not \
                 count). The exception to §15.26 is bounded by exactly this rule: \
                 re-export what the fuzzer drives, nothing else. Add the target, or \
                 drop the re-export."
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 2,
        "expected to check at least the two parsers SEC-7 named; checked {checked}"
    );
}

/// The crates a `serial-nexus-core` dependency on would give it the ability to open a
/// device node. `serial2` opens serial ports; `serial-nexus-sys` is the workspace's only
/// `unsafe` crate and owns every ioctl; `nix` and `libc` are the raw syscall
/// surfaces underneath both.
const DEVICE_OPENING_CRATES: &[&str] = &["serial2", "serial-nexus-sys", "nix", "libc"];

/// Does `manifest` declare a dependency on `krate`?
///
/// Every spelling cargo accepts, because a detector that misses one is worse than
/// no detector: `krate = "1.0"`, `krate = { … }`, the **dotted** `krate.workspace =
/// true` (which is the only form this workspace actually uses, so an earlier
/// version of this function that omitted it could not have caught a single real
/// violation), a `[dependencies.krate]` table — including under a
/// `[target.'cfg(…)'.dependencies]` prefix — and a *renamed* dependency
/// `alias = { package = "krate" }`. Matched at the start of a trimmed line, so a
/// crate merely named in a comment does not trip it.
fn declares_dependency(manifest: &str, krate: &str) -> bool {
    manifest.lines().any(|line| {
        let line = line.trim();
        // A table header ending in `.<krate>` under any dependency table.
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let is_dep_table = inner.contains("dependencies.");
            if is_dep_table && inner.ends_with(&format!(".{krate}")) {
                return true;
            }
        }
        // A renamed dependency pulling the same crate in under another name.
        if line.contains(&format!("package = \"{krate}\""))
            || line.contains(&format!("package=\"{krate}\""))
        {
            return true;
        }
        // `krate = …` or `krate.<key> = …` (the workspace-inheritance spelling).
        let Some(rest) = line.strip_prefix(krate) else {
            return false;
        };
        let rest = rest.trim_start();
        rest.starts_with('=') || (rest.starts_with('.') && rest.contains('='))
    })
}

/// `ports` cannot reach for the tools that open a device (design §12, §15.35).
///
/// Passivity matters because opening a USB-serial adapter asserts DTR and resets
/// the board behind it, so an operator running `ports` *to look* must not disturb
/// anything. **The behavioural proof is `p10_ports`**, whose fixture device nodes
/// are writer-less FIFOs: any read of one would block forever and the RPC would
/// time out. This gate is the narrower structural half, and its claim is bounded
/// deliberately — it does *not* prove nothing is opened, because `std::fs` can open
/// a path with no dependency and no `unsafe`. What it proves is that `serial-nexus-core`
/// carries neither the crates that *drive* a serial port (`serial2`, `serial-nexus-sys`,
/// `nix`, `libc`) nor an explicit file-opening API, so the enumeration path stays
/// what §15.35 specifies: readlinks, directory listings, and sysfs attribute reads.
///
/// Written as a gate rather than a comment because the failure mode is a future
/// convenience — "just add `nix` to serial-nexus-core for one `stat`" — that no reviewer
/// would connect to a board resetting on the bench three months later.
#[test]
fn port_enumeration_cannot_open_a_device() {
    let root = repo_root();
    let manifest_path = root.join("core/Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));

    // Prove the detector fires before trusting its clean verdict — a gate whose
    // scanner silently stopped scanning is this file's whole reason to exist. Every
    // spelling is planted, and **the dotted one especially**: this workspace writes
    // every dependency as `krate.workspace = true`, so a detector blind to that form
    // would pass forever while catching nothing.
    for krate in DEVICE_OPENING_CRATES {
        for planted in [
            format!("{krate} = \"0.1\""),
            format!("{krate} = {{ version = \"0.1\" }}"),
            format!("{krate}.workspace = true"),
            format!("{krate}.version = \"0.1\""),
            format!("[dependencies.{krate}]\nversion = \"0.1\""),
            format!("[dev-dependencies.{krate}]\nversion = \"0.1\""),
            format!("[target.'cfg(unix)'.dependencies.{krate}]\nversion = \"0.1\""),
            format!("renamed = {{ package = \"{krate}\", version = \"0.1\" }}"),
        ] {
            let doc = format!("{manifest}\n{planted}\n");
            assert!(
                declares_dependency(&doc, krate),
                "the dependency detector missed a planted `{planted}`"
            );
        }
    }
    // …and that it does not trip on the crate merely being discussed, or on an
    // unrelated crate whose name merely starts the same way. `serial-nexus-core` explains in
    // prose why it resolves devices without libudev (§15.10).
    for benign in [
        "# serial2 is deliberately not a dependency here",
        "nix-compat.workspace = true",
        "libcst = \"1.0\"",
    ] {
        assert!(
            !DEVICE_OPENING_CRATES
                .iter()
                .any(|k| declares_dependency(&format!("{benign}\n"), k)),
            "the detector must not read {benign:?} as a dependency"
        );
    }

    for krate in DEVICE_OPENING_CRATES {
        assert!(
            !declares_dependency(&manifest, krate),
            "serial-nexus-core declares a dependency on `{krate}`, which can open a device \
             node. The resolver's enumeration face (`ports`, §15.35) is passive by \
             construction: readlinks and sysfs reads only, because probing a port \
             toggles DTR and resets the board behind it. If this dependency is \
             genuinely needed, move the code that needs it out of serial-nexus-core."
        );
    }

    let lib = std::fs::read_to_string(root.join("core/src/lib.rs")).expect("read lib.rs");
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "serial-nexus-core must forbid unsafe: without it, a hand-rolled `open` syscall \
         could reach a device with no dependency to notice"
    );

    // No explicit file-opening API anywhere in serial-nexus-core. The resolver needs only
    // `read_dir`, `read_link`, `exists`, `canonicalize` and `read_to_string` on
    // sysfs attributes; reaching for `File::open` or `OpenOptions` is the shape a
    // device probe would take, and it needs no dependency to write.
    const OPENERS: &[&str] = &["OpenOptions", "File"];
    let (sources, stats) = sources_under(&root.join("core/src"));
    assert!(
        stats.unreadable.is_empty(),
        "directories under core/src the scan could not read: {:?}",
        stats.unreadable
    );
    assert!(
        !sources.is_empty(),
        "found no serial-nexus-core sources — this gate would pass vacuously"
    );
    // Self-proof first, on the same scanner.
    for opener in OPENERS {
        assert!(
            has_code_token(&format!("let f = std::fs::{opener}::open(p);"), opener),
            "the opener detector missed a planted `{opener}`"
        );
    }
    assert!(
        !has_code_token("// File::open would be wrong here\n", "File"),
        "the opener detector must not trip on a comment"
    );
    for (path, src) in &sources {
        for opener in OPENERS {
            assert!(
                !has_code_token(src, opener),
                "{} reaches for `{opener}`. serial-nexus-core resolves device identity by \
                 readlink and sysfs read only (§12, §15.35): opening a device node \
                 asserts DTR and resets the board behind it, and `ports` is the verb \
                 an operator runs to *look*. If a file genuinely must be opened here, \
                 that is a design change, not a refactor.",
                path.display()
            );
        }
    }
}

/// Every markdown link target in `text`, as (line number, raw target). Inline links
/// and images alike — the syntax is `](target)` for both.
///
/// Deliberately a small hand-rolled extractor rather than a markdown crate: the whole
/// point of this gate is that it has no dependency that could go stale, and the shapes
/// it must handle are few and fixed — a bare path, a path with a `#fragment`, an
/// `<angle-bracketed>` path, and a `path "title"` pair. Anything it cannot parse it
/// reports as a link, so the failure mode is a loud false positive rather than a quiet
/// miss (the same bias `has_unsafe_usage` takes).
fn markdown_links(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(close) = after.find(')') else {
                break;
            };
            let target = after[..close].trim();
            // `path "title"` → `path`; `<path>` → `path`.
            let target = target.split_whitespace().next().unwrap_or("");
            let target = target.trim_start_matches('<').trim_end_matches('>');
            if !target.is_empty() {
                out.push((n + 1, target.to_owned()));
            }
            rest = &after[close + 1..];
        }
    }
    out
}

/// Does `target` point at something inside the repository (as opposed to the web, an
/// in-page anchor, or a mail address)?
fn is_relative_link(target: &str) -> bool {
    !(target.starts_with('#')
        || target.starts_with('/')
        || target.contains("://")
        || target.starts_with("mailto:"))
}

/// The path part of a link target, with any `#fragment` and `?query` removed. `None`
/// when nothing but a fragment is left.
fn link_path(target: &str) -> Option<&str> {
    let path = target.split(['#', '?']).next().unwrap_or("");
    (!path.is_empty()).then_some(path)
}

/// **Every relative doc link in the two files a newcomer opens first must resolve.**
///
/// `README.md` and `AGENTS.md` are the tree's entry points — the first is what a
/// consumer reads, the second is what an agent (or a human picking this repo up cold)
/// is told to read first. A dead link in either sends the reader to a file that was
/// renamed a generation ago, which is exactly the failure §9's monotonic
/// design/plan versioning manufactures: the newest pair lives in `docs/`, the previous
/// one moves to `docs/historical/`, and the index in README has to be edited by hand
/// each time. It was not, three reviews running.
///
/// Fragments are not checked — only the file or directory a link points at. That is
/// the part that rots on a rename, and checking anchors would need a markdown parser
/// and a slug algorithm, which is a dependency this gate is better off without.
#[test]
fn entry_point_doc_links_resolve() {
    let root = repo_root();

    // 1. Prove the extractor and the resolver both fire on a planted dead link, and
    //    that neither trips on the shapes that are legitimately not repository paths.
    //    Built by concatenation so this file carries no literal markdown link of its
    //    own for the gate to find later.
    let planted = format!(
        "see [the design]{}\nand [the plan]{}\n",
        "(docs/00-a-design-that-was-deleted.md)", "(docs/rpc/)"
    );
    let found = markdown_links(&planted);
    assert_eq!(
        found.len(),
        2,
        "the link extractor missed a planted link: {found:?}"
    );
    assert!(
        !root.join(link_path(&found[0].1).unwrap()).exists(),
        "the planted dead link unexpectedly exists — pick another name"
    );
    assert!(
        root.join(link_path(&found[1].1).unwrap()).exists(),
        "the resolver cannot see a directory target that really is there"
    );
    for skipped in [
        "https://example.invalid/x",
        "#an-in-page-anchor",
        "mailto:nobody@example.invalid",
    ] {
        assert!(
            !is_relative_link(skipped),
            "{skipped} must not be treated as a repository path"
        );
    }
    // A `path "title"` pair and an `<angled>` path both reduce to the path.
    for (raw, want) in [
        ("[x](docs/macos.md \"macOS\")", "docs/macos.md"),
        ("[x](<docs/macos.md>)", "docs/macos.md"),
        ("[x](docs/macos.md#gotchas)", "docs/macos.md"),
    ] {
        let got = markdown_links(raw);
        assert_eq!(got.len(), 1, "extractor failed on {raw:?}: {got:?}");
        assert_eq!(link_path(&got[0].1), Some(want), "on {raw:?}");
    }

    // 2. The real scan.
    let mut checked = 0usize;
    let mut dead: Vec<String> = Vec::new();
    for file in ["README.md", "AGENTS.md"] {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (line, target) in markdown_links(&text) {
            if !is_relative_link(&target) {
                continue;
            }
            let Some(rel) = link_path(&target) else {
                continue;
            };
            checked += 1;
            // Relative to the linking file's own directory, which for these two is the
            // repo root — written the general way so a moved entry point still works.
            let base = path.parent().unwrap_or(&root);
            if !base.join(rel).exists() {
                dead.push(format!("{file}:{line} -> {target}"));
            }
        }
    }
    assert!(
        dead.is_empty(),
        "dead relative links in the tree's entry-point docs: {dead:?}\n\
         These are the first files a newcomer opens. README's documentation index has \
         pointed at a deleted design/plan pair in three consecutive reviews (19 DOC-5, \
         26 DOC-5, 32 DOCR-1/DOCR-5) because §9's monotonic versioning renames that \
         pair every generation; this gate is what replaces patching it by hand."
    );
    // 3. And the scan was not vacuous: an extractor that silently stopped extracting
    //    is the failure invariant 5 suffered, so a floor rather than a bare `> 0`.
    assert!(
        checked >= 10,
        "only {checked} relative links found across README.md and AGENTS.md — the \
         extractor has stopped seeing them, and a gate that checks nothing passes \
         forever"
    );
}

/// The content of every single-backtick code span in `text`, as (line number, span).
///
/// Line-local and deliberately naive — split each line on the backtick and keep the odd
/// pieces. A fenced code block yields nothing interesting (its lines carry no backticks,
/// and the ``` fences themselves reduce to empty strings or a language tag), and none of
/// those can look like the filenames below.
fn backtick_spans(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        for (i, piece) in line.split('`').enumerate() {
            if i % 2 == 1 && !piece.is_empty() {
                out.push((n + 1, piece.to_owned()));
            }
        }
    }
    out
}

/// Is `token` a §9 generation-numbered design or implementation-plan filename —
/// `30-design-claude-fable-v13.md`, `docs/31-implementation-plan-claude-fable-v13.md`?
///
/// The leading number is required, because that is what §9 bumps; the middle is left
/// open, because the author name in it has already changed twice.
fn is_generation_doc(token: &str) -> bool {
    let base = token.rsplit('/').next().unwrap_or(token);
    base.ends_with(".md")
        && base
            .split('-')
            .next()
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        && (base.contains("-design-") || base.contains("-implementation-plan-"))
}

/// Where a [`is_generation_doc`] token must be found, relative to the repository root:
/// as written when it carries a directory, else directly under `docs/`.
fn generation_doc_path(token: &str) -> PathBuf {
    if token.contains('/') {
        PathBuf::from(token)
    } else {
        Path::new("docs").join(token)
    }
}

/// **Every backticked design/plan filename in the entry-point docs must exist — and the
/// un-pathed ones must be the *current* pair.**
///
/// The sibling of [`entry_point_doc_links_resolve`], for the half a link checker cannot
/// see. `AGENTS.md` §9 names the current pair in prose, in backticks
/// (`NN-design-…` + `NN-implementation-plan-…`), and that parenthetical is the exact site
/// that went stale in three consecutive review generations. A stale one is not cosmetic:
/// §9 says `§N` always means the *current* normative design, so a newcomer sent to a
/// superseded file reads rules the code no longer implements — which AGENTS' own opening
/// paragraph records as having cost this project two rounds of deleted working code.
///
/// The resolution rule is what gives the gate its teeth. A bare filename must exist
/// directly under `docs/`, where §9 puts only the newest pair; the moment a generation is
/// superseded the file moves to `docs/historical/` and this fails. A token written *with*
/// a directory resolves as written, so prose that deliberately cites a historical
/// generation (`docs/historical/28-design-…`) stays legal.
///
/// Proved fail-first end to end, not only through the planted-violation block below: with
/// a stale name spliced into the scanned text the gate fails naming it, and the pathed
/// historical name spliced beside it is accepted in the same run.
#[test]
fn entry_point_design_and_plan_names_resolve() {
    let root = repo_root();

    // 1. Planted-violation self-proof, in both directions. Assembled from pieces so this
    //    file contains no literal offender of its own.
    let stale = format!("{}{}{}", "29-", "implementation-plan-", "gone-v12.md");
    // The live half is *discovered*, never spelled. README's index and AGENTS.md §2 are
    // the two places that name the current pair by filename, and a literal here would
    // quietly become a third — one that goes stale on the same generation bump this gate
    // exists to catch, and fails with "the resolver cannot see the current design" while
    // the entry points it guards are perfectly correct. That is exactly what happened
    // when the v14 pair landed.
    let live = std::fs::read_dir(root.join("docs"))
        .expect("read docs/")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| is_generation_doc(n))
        .expect("docs/ holds no design/plan generation file at all — §9's pair is gone");
    let planted = format!("§9 (currently `{live}` + `{stale}`), superseded ones move\n");
    let spans = backtick_spans(&planted);
    assert_eq!(
        spans.len(),
        2,
        "the backtick extractor missed a planted span: {spans:?}"
    );
    assert!(
        spans.iter().all(|(_, s)| is_generation_doc(s)),
        "the filename matcher no longer recognises a §9 generation doc: {spans:?}"
    );
    assert!(
        !root.join(generation_doc_path(&stale)).exists(),
        "the planted stale name unexpectedly exists — pick another"
    );
    assert!(
        root.join(generation_doc_path(&live)).exists(),
        "the resolver cannot see the current design, which is right there in docs/"
    );
    // …and the shapes that must NOT be treated as generation docs: ordinary prose, an
    // un-numbered doc, and a review file that happens to be numbered.
    for benign in [
        "cargo test --workspace",
        "docs/macos.md",
        "docs/historical/26-claude-opus-code-review.md",
        "§15.37",
    ] {
        assert!(
            !is_generation_doc(benign),
            "{benign:?} must not be read as a design/plan filename"
        );
    }
    // A historical citation written *with* its directory stays legal.
    let historical = "docs/historical/28-design-claude-fable-v12.md";
    assert!(is_generation_doc(historical));
    assert_eq!(generation_doc_path(historical), PathBuf::from(historical));

    // 2. The real scan.
    let mut checked = 0usize;
    let mut stale_names: Vec<String> = Vec::new();
    for file in ["README.md", "AGENTS.md"] {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (line, span) in backtick_spans(&text) {
            for token in span.split_whitespace() {
                let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/');
                if !is_generation_doc(token) {
                    continue;
                }
                checked += 1;
                if !root.join(generation_doc_path(token)).exists() {
                    stale_names.push(format!("{file}:{line} -> {token}"));
                }
            }
        }
    }
    assert!(
        stale_names.is_empty(),
        "backticked design/plan filenames in the entry-point docs that do not exist \
         under docs/: {stale_names:?}\n\
         §9 bumps this pair every generation and moves the old one to docs/historical/. \
         A bare filename here must be the *current* pair; cite a superseded one with its \
         directory (docs/historical/…) if that is what you meant."
    );
    // 3. Not vacuous: the two entry points must still be naming the pair somewhere. If
    //    this floor ever wants lowering, the pair stopped being named by filename at all
    //    — which is also a change worth noticing.
    assert!(
        checked >= 3,
        "only {checked} backticked design/plan filenames found across README.md and \
         AGENTS.md — the extractor has stopped seeing them, and a gate that checks \
         nothing passes forever"
    );
}
