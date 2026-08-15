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
/// The tree carries ~165, the nine `fuzz/fuzz_targets/` sources among them since the
/// walk stopped skipping `fuzz/` (see [`is_fuzz_scratch`]). The floor exists because a
/// walker that quietly stopped walking reports the same green as a clean tree (review
/// 37, 37-TEST-1): `read_dir` failures were swallowed, so a scan over a shrunken file
/// set — one crate, or none — passed the ban gates for invariants 1 and 5. Set well
/// below the real count so ordinary deletions do not trip it, and far above the
/// single-crate degradation it exists to catch.
const MIN_RS_FILES: usize = 100;

/// The floor on crate roots [`crate_dirs`] must find. Eighteen today (the workspace
/// members, the out-of-tree consumer template's four manifests, and the
/// workspace-*excluded* `fuzz/`). Same reasoning as [`MIN_RS_FILES`]: the completeness
/// half of the `RefCell` ban is only as good as the list of crates it ranges over.
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

/// Is `dir` the root of a **nested checkout** — another worktree or clone living inside
/// this one?
///
/// Skipped by every walker here, because a nested checkout is a second copy of this tree
/// and scanning it reports each of its own files as a violation *of this tree*. Measured:
/// a single `git worktree add` under `.claude/worktrees/` turns four of these gates red
/// at once, naming `sys/src/lib.rs` and this very file — findings that are true of the
/// copy and vacuous about the repository under test.
///
/// Keyed on "is a checkout" rather than on the directory's *name* on purpose. A name list
/// would have to guess where someone puts a worktree, and `.claude` in particular must
/// **not** be skipped wholesale: `.claude/settings.json` is tracked project configuration
/// and the §15.41 privacy rule is tree-wide, so it has to stay in the scan. A worktree has
/// a `.git` **file** (a gitfile pointing at the parent) and a clone has a `.git`
/// directory; `exists()` covers both. The repository root itself is never tested, because
/// these walkers examine entries *below* the root they are given.
fn is_nested_checkout(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Is `dir` cargo-fuzz's **generated** state — a corpus, a crash-artifact directory or
/// a coverage dump — rather than tracked source?
///
/// `fuzz/` itself is walked. It was skipped by name until 2026-08-12, and that skip was
/// a hole in §16.3's containment rather than a saving: the nine `fuzz/fuzz_targets/`
/// sources are each their own crate root, they link `serial-nexus-daemon` and
/// `serial-nexus-web` through their `unstable_fuzz_api` modules, and an `unsafe` block
/// landing in one was invisible to both halves of the invariant's enforcement — the
/// whole-tree scan in [`unsafe_is_confined_to_serial_nexus_sys`] and the enumeration in
/// [`every_crate_root_forbids_unsafe_except_the_one_that_may_not`] skipped the same
/// directory by the same name. Being excluded from the workspace makes a crate
/// *unbuilt by `cargo build --workspace`*, which is an argument for scanning it harder,
/// not for scanning it not at all.
///
/// What genuinely must not be walked is what the fuzzer *writes*. `/fuzz/corpus` and
/// `/fuzz/artifacts` are gitignored and hold whatever libFuzzer produced — tens of
/// thousands of input files after one nightly loop, none of them source, all of them
/// attacker-shaped bytes that a token scan has no business reading. `fuzz/coverage` is
/// the same for `cargo fuzz coverage`. `fuzz/target` is already covered by the `target`
/// skip.
///
/// Keyed on the **path** (`fuzz/<name>`), never on the bare directory name, for the
/// reason [`THIS_FILE`] and [`REFCELL_EXEMPT`] are both paths (review 32 TESTR-7,
/// review 37 37-TEST-2): a name list would hand a blanket exemption to any future
/// directory called `corpus` anywhere in the tree, and an exemption nobody stated is
/// exactly the shape these gates keep being written to catch.
fn is_fuzz_scratch(dir: &Path) -> bool {
    let name = dir.file_name().and_then(|n| n.to_str());
    let parent = dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    matches!(name, Some("corpus" | "artifacts" | "coverage")) && parent == Some("fuzz")
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
                // Skip build output, VCS, vendored trees, the fuzzer's generated
                // corpus/artifact/coverage trees (see `is_fuzz_scratch` — the fuzz
                // *sources* are walked), and any nested worktree/clone (see
                // `is_nested_checkout`).
                if matches!(name.as_ref(), "target" | ".git" | "node_modules")
                    || is_fuzz_scratch(&path)
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
            // Build output, VCS, vendored trees, the fuzzer's generated corpus and
            // artifact trees, and any nested worktree/clone — whose crates are this
            // workspace's crates counted twice (see `is_nested_checkout`).
            //
            // `fuzz/` itself is **not** skipped any more: it holds a real crate root,
            // and the gate that made this walk's list of crates matter — every crate
            // root forbidding `unsafe` — was blind to nine of them for exactly as long
            // as the name sat here (see `is_fuzz_scratch`).
            if matches!(name.as_ref(), "target" | ".git" | "node_modules")
                || is_fuzz_scratch(&path)
                || is_nested_checkout(&path)
            {
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

/// The `path` of every `[[bin]]` entry in a cargo manifest, in manifest order.
///
/// The **third shape a crate root comes in**, after `src/lib.rs`/`src/main.rs` and the
/// per-file roots under `tests/`: a `[[bin]]` with an explicit `path` puts a crate root
/// anywhere the author likes, and `fuzz/Cargo.toml` puts nine of them under
/// `fuzz/fuzz_targets/`. A gate that enumerates only the first two shapes reports green
/// over every one of them.
///
/// Hand-parsed, deliberately and narrowly, over the exact subset of TOML these
/// manifests are written in — an array-of-tables header followed by `key = "value"`
/// lines. `meta_derive.rs`'s `fuzz_bin_table` reads the same table the same way for the
/// registration bijection, and records the reason neither shells out to `cargo fuzz
/// list`: cargo-fuzz needs a nightly toolchain and libFuzzer and is installed only in
/// the scheduled fuzz job, so a gate built on it would self-skip on every ordinary run
/// — which is every run where the attribute actually goes missing. The narrowness is
/// safe in the direction that matters: a registration spelled in some form this misses
/// drops a root out of the enumeration, and the fuzz floor in
/// [`every_crate_root_forbids_unsafe_except_the_one_that_may_not`] turns that into a
/// loud failure rather than a silent pass.
fn manifest_bin_paths(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_bin = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_bin = trimmed == "[[bin]]";
            continue;
        }
        if !in_bin {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() == "path" {
            out.push(value.trim().trim_matches('"').to_owned());
        }
    }
    out
}

/// The crate roots `dir`'s own manifest declares explicitly, resolved against `dir`.
///
/// Missing paths are **kept**, not filtered: a `[[bin]]` registered at a file that does
/// not exist is a manifest `cargo fuzz build` fails on, and it surfaces here as a root
/// carrying no attribute (`meta_derive.rs`'s registration bijection says it more
/// precisely). Dropping it would be the silent half.
fn manifest_bin_roots(dir: &Path) -> Vec<PathBuf> {
    let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    manifest_bin_paths(&manifest)
        .into_iter()
        .map(|p| dir.join(p))
        .collect()
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
    // The fuzz crate, in both of its halves. A fuzz **target** is tracked source and a
    // crate root, so it must be surfaced — this directory was skipped by name until
    // 2026-08-12 and an `unsafe` block in one was invisible to this gate. What the
    // fuzzer *writes* must not be: a corpus entry is attacker-shaped bytes and a crash
    // artifact is a reproducer, neither is source, and after one nightly loop there are
    // tens of thousands of them. Planted with an `.rs` extension precisely because the
    // extension filter would otherwise hide whether the skip works at all — a skip is a
    // matcher too (AGENTS §3), so it is planted in every spelling it claims to cover.
    scratch.write("fuzz/fuzz_targets/offender.rs", &planted);
    scratch.write("fuzz/corpus/rpc_base64/seed.rs", &planted);
    scratch.write("fuzz/artifacts/rpc_base64/crash-dead.rs", &planted);
    scratch.write("fuzz/coverage/rpc_base64/coverage.rs", &planted);
    scratch.write("fuzz/target/debug/build/offender.rs", &planted);
    // …and the name-keyed spelling of that skip, which must NOT swallow anything: a
    // `corpus/` that is not the fuzzer's is ordinary source and stays in the scan.
    scratch.write("krate/src/corpus/offender.rs", &planted);
    // A nested **worktree** (a `.git` gitfile, which is what `git worktree add` leaves)
    // and a nested **clone** (a `.git` directory), each carrying the offender. Both must
    // be skipped whole: they are copies of a tree, not part of this one, and before this
    // was true a single worktree under `.claude/worktrees/` turned this gate red against
    // its own duplicate. Planted in both spellings because a skip is a matcher too
    // (AGENTS §3) — keying on the directory's name instead of on "is a checkout" passes
    // one of these two and fails the other.
    scratch.write("wt/.git", "gitdir: /elsewhere/.git/worktrees/wt\n");
    scratch.write("wt/src/offender.rs", &planted);
    scratch.write("clone/.git/HEAD", "ref: refs/heads/main\n");
    scratch.write("clone/src/offender.rs", &planted);
    let mut planted_hits = Vec::new();
    let stats = walk_rs(scratch.path(), &mut |path, src| {
        if has_unsafe_usage(src) {
            planted_hits.push(rel_path(scratch.path(), path));
        }
    });
    // Sorted: `read_dir` order is the filesystem's, and this list stopped being one
    // entry long when the fuzz sources joined the walk.
    planted_hits.sort();
    assert_eq!(
        planted_hits,
        vec![
            "fuzz/fuzz_targets/offender.rs".to_owned(),
            "krate/src/corpus/offender.rs".to_owned(),
            "nested/deep/offender.rs".to_owned(),
        ],
        "the walker missed a planted `unsafe`, or surfaced one inside a directory it must \
         skip (`target/`, the fuzzer's corpus/artifacts/coverage trees, a nested worktree, \
         a nested clone) — this gate would either pass over anything or convict the \
         repository of a copy's or a corpus entry's contents"
    );
    assert_eq!(
        stats.files, 3,
        "the walker visited {} files in a scratch tree holding exactly three `.rs` \
         outside target/, the fuzzer's generated trees and the two nested checkouts — \
         the extension filter or the skip list has moved",
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
        // `src/` plus whatever the manifest puts elsewhere. A crate whose sources do
        // not live under `src/` is a crate this completeness claim silently never
        // ranged over, and the tree has one: `fuzz/`'s roots are `[[bin]] path`
        // entries under `fuzz_targets/`, in a crate that links `serial-nexus-daemon`
        // through `unstable_fuzz_api`. Each fuzz target is a single file with no
        // modules, so its registration *is* its whole source (§16.2, invariant 5).
        let mut sources = sources_under(&dir.join("src")).0;
        for path in manifest_bin_roots(&dir) {
            if let Ok(src) = std::fs::read_to_string(&path) {
                sources.push((path, src));
            }
        }
        let uses_it = sources
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

/// **Every crate root forbids `unsafe`, and exactly one does not** (§16.3; §5's
/// tripwire table; plan §2).
///
/// The invariant is stated absolutely — "`unsafe` lives only in `serial_nexus_sys`;
/// everything else is `#![forbid(unsafe_code)]`" — and until 2026-08-12 the harness
/// crate satisfied *neither* half of it: `serial-nexus-itest` carried the attribute on
/// **none** of its 95 crate roots (`src/lib.rs` plus 94 files under `tests/`, each its
/// own crate root and therefore its own place the attribute has to appear), while two
/// of its files told a reader checking the invariant that it was present. What held
/// the line was [`unsafe_is_confined_to_serial_nexus_sys`] above — a grep, and a grep
/// is a *detector*, not a compiler: it cannot see an edition-2024 `unsafe(…)`
/// attribute, and it cannot see a macro that expands to an `unsafe` block.
///
/// This gate is the structural half, and it is deliberately a **two-sided** check.
/// Forgetting the attribute on a new test file is the drift it exists to catch; the
/// other side matters as much — the exception set must stay at exactly one file, so
/// the sanctioned inner `allow` cannot quietly spread to a second crate that found it
/// convenient. §16.3's whole value is that the answer to "where is the `unsafe`" is
/// one directory.
///
/// Crate roots are enumerated rather than listed: `src/lib.rs`, `src/main.rs`, every
/// `tests/*.rs`, and every `[[bin]] path` the manifest declares, for every crate the
/// [`crate_dirs`] walk finds. A list typed here would be the hand-kept roster this
/// repository keeps learning not to keep (AGENTS §3).
///
/// The `[[bin]]` shape is the newest of the three and was the second hole of the same
/// kind: `fuzz/fuzz_targets/*.rs` are nine crate roots that live under neither `src/`
/// nor `tests/`, and until 2026-08-12 both this gate and
/// [`unsafe_is_confined_to_serial_nexus_sys`] skipped the directory holding them by
/// name. None of the nine carried the attribute, and an `unsafe` block in any of them
/// would have been seen by neither half of §16.3's enforcement — in a crate that links
/// `serial-nexus-daemon` and `serial-nexus-web` through their `unstable_fuzz_api`
/// modules, and that `cargo build --workspace` does not compile, so the compiler was
/// not watching either. Derived from the manifest ([`manifest_bin_roots`]) rather than
/// globbed from the directory, because the registration is what decides whether a file
/// is built at all — that is item 40's doctrine and the same rule `meta_derive.rs`'s
/// bijection gate enforces from the other side.
#[test]
fn every_crate_root_forbids_unsafe_except_the_one_that_may_not() {
    const FORBID: &str = "#![forbid(unsafe_code)]";
    // Built by concatenation, exactly like the planted `unsafe` sample above and for
    // the same reason: a literal here would make **this file** an exemption, since it
    // is a crate root the scan below reads. Caught on the first run — the gate reported
    // `["itest/tests/meta_gates.rs", "sys/src/lib.rs"]`, which is a detector matching
    // itself rather than a second crate going soft. The `forbid` needle needs no such
    // dodge: this file carries that attribute for real, as every crate root must.
    let allow = format!("#![{}(unsafe_code)]", "allow");
    let allow = allow.as_str();
    // The one crate §16.3 names. Spelled as a repo-relative path, not a base name:
    // `lib.rs` is the commonest file name in the tree.
    const EXCEPTION: &str = "sys/src/lib.rs";

    let root = repo_root();

    // 0. The manifest matcher first, on a planted registration: the `[[bin]]` shape is
    //    the only one of the three that is *parsed* rather than found on disk, so a
    //    parser that stopped parsing would drop nine roots and report the same green as
    //    a compliant tree. Both directions, and both endings a block has — the last
    //    registration in a manifest is terminated by end-of-file, not by a header.
    let planted_manifest = "[[bin]]\nname = \"planted\"\npath = \"fuzz_targets/planted.rs\"\n\
         test = false\n\n[[bin]]\nname = \"second\"\npath = \"fuzz_targets/second.rs\"\n";
    assert_eq!(
        manifest_bin_paths(planted_manifest),
        vec![
            "fuzz_targets/planted.rs".to_owned(),
            "fuzz_targets/second.rs".to_owned()
        ],
        "the manifest parser cannot read the `[[bin]]` shape every fuzz target is \
         registered in — the nine roots under fuzz/fuzz_targets/ would drop out of the \
         enumeration silently"
    );
    assert!(
        manifest_bin_paths("[dependencies]\nname = \"x\"\npath = \"src/not-a-bin.rs\"\n")
            .is_empty(),
        "a `path` key under another table header is read as a crate root — the parser \
         would invent roots that cargo never builds"
    );

    let mut unreadable = Vec::new();
    let crates = crate_dirs(&root, &mut unreadable);
    assert!(
        unreadable.is_empty(),
        "the crate walk could not read {unreadable:?} — a crate it never reached is a \
         crate whose missing attribute reads as compliance"
    );

    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in &crates {
        for candidate in ["src/lib.rs", "src/main.rs"] {
            let p = dir.join(candidate);
            if p.is_file() {
                roots.push(p);
            }
        }
        // Integration tests: each file directly under `tests/` is its own crate root,
        // which is exactly why the attribute has to be repeated per file and exactly
        // why 94 of them were missed.
        if let Ok(entries) = std::fs::read_dir(dir.join("tests")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|e| e == "rs") {
                    roots.push(p);
                }
            }
        }
        // …and the third shape: a `[[bin]]` with an explicit `path`, which puts a crate
        // root wherever the manifest says. Nine of the tree's live under
        // `fuzz/fuzz_targets/`; the workspace binaries' registrations name their own
        // `src/main.rs`, hence the dedup below.
        roots.extend(manifest_bin_roots(dir));
    }
    roots.sort();
    roots.dedup();

    // Non-vacuity, both directions: this tree has well over a hundred crate roots, and
    // a walk that found a handful has stopped walking rather than found compliance.
    assert!(
        roots.len() > 100,
        "only {} crate roots found; the enumeration has stopped seeing them and a gate \
         that checks nothing passes forever",
        roots.len()
    );

    // …and specifically the manifest-declared ones, which the count above cannot speak
    // for: the other 150-odd roots would carry that assertion on their own while the
    // `[[bin]]` shape silently contributed nothing, which is the state this gate was in
    // until 2026-08-12. `MIN_FUZZ_TARGETS` is the same floor the `unstable_fuzz_api`
    // gate holds the corpus to.
    let fuzz_roots = roots
        .iter()
        .filter(|p| rel_path(&root, p).starts_with("fuzz/fuzz_targets/"))
        .count();
    assert!(
        fuzz_roots >= MIN_FUZZ_TARGETS,
        "only {fuzz_roots} crate root(s) enumerated under fuzz/fuzz_targets/ (floor \
         {MIN_FUZZ_TARGETS}) — either fuzz/Cargo.toml's [[bin]] table stopped being \
         read or the crate walk stopped reaching fuzz/. Each of those files is a crate \
         root §16.3 covers, in a crate `cargo build --workspace` never compiles"
    );

    let mut missing = Vec::new();
    let mut exempted = Vec::new();
    for path in &roots {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(path).unwrap_or_default();
        if src.contains(allow) {
            exempted.push(rel);
        } else if !src.contains(FORBID) {
            missing.push(rel);
        }
    }

    assert!(
        missing.is_empty(),
        "these crate roots carry neither `{FORBID}` nor the one sanctioned \
         `{allow}`: {missing:?}. §16.3 makes the attribute the invariant and the \
         `unsafe` grep only its detector — a file without it is a file where the \
         compiler is not enforcing anything"
    );
    assert_eq!(
        exempted,
        vec![EXCEPTION.to_owned()],
        "the `{allow}` exception must be exactly `{EXCEPTION}` and nothing else \
         (§16.3): `unsafe` lives in one crate so that the answer to \"where is the \
         unsafe\" is one directory, and a second exemption ends that whatever its \
         reason"
    );
}

// ---------------------------------------------------------------------------
// The devprep platform arms answer the same verbs (plan §18 item 52 (a)).
// ---------------------------------------------------------------------------

/// The verb names of a `#[derive(Subcommand)] enum Verb { … }`, in source order.
///
/// A parser rather than a grep, because the thing being compared is the *set of
/// verbs clap will accept* and nothing else in either file may be mistaken for one.
/// Line comments — including doc comments, which in these two files talk about the
/// other arm's verbs constantly — are stripped before anything is matched, and
/// candidates are only taken at the enum's own brace depth, so a variant's fields
/// and its `#[arg(...)]` attributes cannot be read as verbs.
fn verb_variants(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: isize = 0;
    let mut started = false;
    for raw in src.lines() {
        // Everything from `//` on is prose: `///`, `//!` and ordinary comments alike.
        let line = raw.split("//").next().unwrap_or("");
        let trimmed = line.trim();
        if !started {
            if trimmed.starts_with("enum Verb") && trimmed.ends_with('{') {
                started = true;
                depth = 1;
            }
            continue;
        }
        // A variant is an identifier at the head of a line at the enum's own depth.
        if depth == 1
            && let Some(name) = trimmed
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
            && !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_uppercase())
        {
            out.push(name.to_owned());
        }
        depth += line.matches('{').count() as isize;
        depth -= line.matches('}').count() as isize;
        if depth <= 0 {
            break;
        }
    }
    out
}

/// What each arm accepts that the other does not, as `(missing on macOS, missing on
/// Linux)`.
fn verb_parity(linux: &str, macos: &str) -> (Vec<String>, Vec<String>) {
    let (l, m) = (verb_variants(linux), verb_variants(macos));
    let missing_on_macos: Vec<String> = l.iter().filter(|v| !m.contains(v)).cloned().collect();
    let missing_on_linux: Vec<String> = m.iter().filter(|v| !l.contains(v)).cloned().collect();
    (missing_on_macos, missing_on_linux)
}

/// Every verb `serial-nexus-devprep` accepts on one platform is accepted on the
/// other, even where the honest answer is "nothing to do".
///
/// **The contract is the macOS file's own**, stated in its module doc about
/// `capabilities`, `install` and `preflight`: *"They stay, because a caller that
/// shells out to them must get a clean answer rather than an unknown verb, and they
/// report ready with the reason."* An unknown-subcommand error is precisely what that
/// bans — and `grant` had been left out of the enum since §15.55 added it, so the one
/// verb that arm's own amendment introduced was the one it refused (plan §18 item 52
/// (a)). Concretely: §15.55 makes `grant` the step `authorize`, `cycle` and `hold`
/// take automatically after a reauthorization, so a caller performing that step by
/// hand met clap's usage error on a platform whose answer is "nothing to do".
///
/// Checked **both ways**. A verb macOS accepts and Linux does not is the same defect
/// with the platforms swapped, and a one-directional gate would treat the reference
/// arm as privileged for no stated reason.
///
/// Neither list is typed here. The gate parses both enums, so a ninth verb inherits
/// the rule the day it is added — the hand-kept-roster failure this repository keeps
/// finding (AGENTS §3), and the same failure §15.55 already paid for once with two
/// capability lists that had to agree.
#[test]
fn every_devprep_verb_answers_on_both_platform_arms() {
    let root = repo_root();
    // The *directory* is what a filesystem-scanning gate spells (AGENTS §11); the
    // crate name carries the family prefix and this path must not.
    let linux_path = root.join("devprep/src/linux/mod.rs");
    let macos_path = root.join("devprep/src/macos/mod.rs");
    let linux = std::fs::read_to_string(&linux_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", linux_path.display()));
    let macos = std::fs::read_to_string(&macos_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", macos_path.display()));

    // Parity first, non-vacuity second, and that order is deliberate: the floors
    // below are keyed to today's verb count, so putting them first would let *any*
    // parity break redden on "only parsed seven" instead of on the sentence that
    // names the defect (measured — the first fail-first run of this gate did exactly
    // that). A parse that collapses to nothing still cannot slip through, because
    // parity over two empty sets is vacuously clean and the floors catch it.
    let (missing_on_macos, missing_on_linux) = verb_parity(&linux, &macos);
    assert!(
        missing_on_macos.is_empty(),
        "{} accepts {missing_on_macos:?} and {} does not — that is clap's \
         unknown-subcommand error, which the macOS arm's own module doc bans: a caller \
         that shells out must get a clean answer with the reason, not a usage error",
        linux_path.display(),
        macos_path.display()
    );
    assert!(
        missing_on_linux.is_empty(),
        "{} accepts {missing_on_linux:?} and {} does not",
        macos_path.display(),
        linux_path.display()
    );

    let linux_verbs = verb_variants(&linux);
    let macos_verbs = verb_variants(&macos);
    // Non-vacuity: a parser that found nothing reports the same clean parity as two
    // identical enums, which is the tell AGENTS §3 names — a passing gate whose
    // output is identical to its not-running output.
    assert!(
        linux_verbs.len() >= 8,
        "only parsed {linux_verbs:?} from {} — the enum moved or the parser stopped \
         seeing it, and either way this gate is asserting nothing",
        linux_path.display()
    );
    assert!(
        macos_verbs.len() >= 8,
        "only parsed {macos_verbs:?} from {}",
        macos_path.display()
    );
    // The verb §15.55 introduced, named explicitly: it is the one this gate was
    // written for, and a parser that quietly stopped recognising it would leave the
    // set comparison green for the wrong reason.
    for verbs in [&linux_verbs, &macos_verbs] {
        assert!(
            verbs.contains(&"Grant".to_owned()),
            "`Grant` is §15.55's verb and must be in {verbs:?}"
        );
    }

    // The **matcher and the parser**, proven against planted sources rather than
    // trusted (AGENTS §3). Three plants: the real defect, a doc comment that names
    // the missing verb (a grep would call this parity), and a variant *field* whose
    // name looks like a verb.
    let planted_linux = "#[derive(Subcommand)]\nenum Verb {\n    Cycle {\n        #[arg(long)]\n        json: bool,\n    },\n    Grant {\n        Port: String,\n    },\n}\n";
    let planted_macos_missing = "#[derive(Subcommand)]\nenum Verb {\n    /// Grant is not implementable here.\n    Cycle {\n        #[arg(long)]\n        json: bool,\n    },\n}\n";
    assert_eq!(
        verb_parity(planted_linux, planted_macos_missing).0,
        vec!["Grant".to_owned()],
        "the planted omission must surface, and the planted doc comment naming it \
         must not paper over it"
    );
    assert_eq!(
        verb_variants(planted_linux),
        vec!["Cycle".to_owned(), "Grant".to_owned()],
        "a variant's fields sit one brace deeper and are not verbs"
    );
    // And the parser must not run past the enum into whatever follows it.
    let after = format!("{planted_linux}\nenum Other {{\n    Nope,\n}}\n");
    assert_eq!(
        verb_variants(&after),
        vec!["Cycle".to_owned(), "Grant".to_owned()]
    );
}

// ---------------------------------------------------------------------------
// The "one shared helper" rules, forbidden a second instance rather than merely
// having their first one tested (plan §18 item 64(g)).
//
// §16.1's boundary supervisor, §16.4's purge, the one fragmenter (§15.27) and
// §16.11's retired shell suite are each stated as "there is exactly one of these".
// Every *instance* was covered — `boundary.rs`'s property tests, the three purge
// instances' guards, `data_frames`' byte-exact fragmentation tests — and nothing
// forbade a fourth hand-rolled one, which is the failure §16.1 was written after:
// three of the five worst audit findings were the same lifecycle rules re-derived
// by hand, per node.
//
// **What is gated here and what is not, said rather than implied.** Three of the
// four rules have an enumerable spelling a second implementation cannot avoid:
// a second framer must call `encode`, a second boundary must start a thread, a
// returning shell suite must be a file. §16.4's purge has none — a hand-rolled
// drain is a `try_recv` loop, and banning those in the daemon would redden on the
// next legitimate control-channel drain rather than on a purge, which is a
// nuisance gate rather than an invariant. It is left ungated deliberately and its
// instances stay covered by their own guards (`runtime.rs`'s
// `purge_to_quiescence_*` family, `serial.rs`'s and `leg.rs`'s per-instance
// tests). Do not read the three gates below as covering it.
// ---------------------------------------------------------------------------

/// `src` with every `#[cfg(test)] mod … { … }` block **blanked** — every character
/// inside replaced by a space, newlines kept — so line numbers survive the strip.
///
/// The gates below are about **product** code: a test that calls `encode` to build
/// a fixture frame, or spawns a thread to bound a join, is not a second framer or a
/// second boundary — `boundary.rs`'s own `within` helper is exactly that, and a
/// scan that counted it would be red on arrival and deleted rather than fixed.
/// Blanking rather than deleting is what lets the failure message name a line number
/// the reader can open; a gate that names a line nobody can find is a gate nobody
/// acts on.
///
/// Naive about braces inside string literals and about `#[cfg(test)]` on a
/// non-module item; both make the stripper cut *less* than intended, which leaves
/// more code under the ban rather than less. The direction matters: a stripper that
/// over-cut would hide product code from every gate below.
fn blank_test_modules(src: &str) -> String {
    let mut out: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    while let Some(at) = src[i..].find("#[cfg(test)]") {
        let at = i + at;
        let Some(open) = src[at..].find('{').map(|o| at + o) else {
            break;
        };
        let mut depth = 0usize;
        let mut end = src.len();
        for (o, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + o + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        // Char indices, since `out` is a Vec<char>: this tree is ASCII in code and
        // has non-ASCII in comments and strings, so byte offsets would misalign.
        let start_c = src[..at].chars().count();
        let end_c = src[..end].chars().count();
        for c in out.iter_mut().take(end_c).skip(start_c) {
            if *c != '\n' {
                *c = ' ';
            }
        }
        i = end;
    }
    out.into_iter().collect()
}

/// Every product-code line in `src` (test modules blanked, `//`-comments ignored)
/// that **calls** `name` — the identifier as a whole word, immediately followed by
/// `(` — as `(line number, line)`.
///
/// "Immediately followed by `(`" is what separates a call from a mention, and both
/// near-misses are real lines in this tree: `use …::{Event, MAX_FRAME_SIZE, encode};`
/// imports the fragmenter's encoder without calling it, and
/// `format!("encode hello: {e}")` names it in an error message on a line whose
/// *other* half is a call to something else entirely.
fn code_call_sites(src: &str, name: &str) -> Vec<(usize, String)> {
    let product = blank_test_modules(src);
    let mut out = Vec::new();
    for (n, line) in product.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(head, _)| head);
        if contains_word(code, &format!("{name}(")) {
            out.push((n + 1, code.trim().to_owned()));
        }
    }
    out
}

/// Every product-code line in `src` containing `needle` verbatim, as
/// `(line number, line)`. For rules whose spelling is a path rather than a call —
/// `thread::spawn(`, `thread::Builder` — where a whole-word match on the last
/// segment would also match an unrelated `.spawn(` on a `Command`.
fn code_lines_containing(src: &str, needle: &str) -> Vec<(usize, String)> {
    let product = blank_test_modules(src);
    let mut out = Vec::new();
    for (n, line) in product.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(head, _)| head);
        if code.contains(needle) {
            out.push((n + 1, code.trim().to_owned()));
        }
    }
    out
}

/// **The one fragmenter has one call site** (§15.27, §5, invariant 3 — plan §18
/// item 64(g)).
///
/// Invariant 3 says every targetward framer fragments oversize chunks through the
/// one shared helper and never skips on an encode error. Both halves are tested at
/// the helper — `data_frames_fragments_byte_exactly_with_no_residual` and its
/// siblings in `runtime.rs` — and a *second* framer would satisfy neither while
/// failing nothing, because those tests call `data_frames` by name.
///
/// The spelling a second framer cannot avoid is `serial_nexus_codec_api::encode`:
/// the envelope is what makes a frame a frame, and hand-rolling the envelope too
/// would be a §15.27 violation of a larger kind, caught by the codec conformance
/// suite. So the daemon's product code may call `encode` in exactly one place, and
/// this names it.
#[test]
fn the_daemon_builds_a_targetward_frame_in_exactly_one_place() {
    // 0. The matcher, in the spellings it claims to cover and against the four
    //    near-misses that must not trip it — every one of them a real line in this
    //    tree, not an invented one.
    assert_eq!(
        code_call_sites("fn f() {\n    encode(&ev, &mut out)?;\n}", "encode").len(),
        1,
        "the scanner misses a bare `encode(` call"
    );
    for (what, near_miss) in [
        (
            "`base64_encode` read as the envelope encoder — a whole-word match is \
             what separates them, and without it this gate would be permanently red \
             and deleted rather than fixed",
            "fn f() {\n    let s = serial_nexus_rpc::base64_encode(&b);\n}",
        ),
        (
            "the import line read as a call site (`runtime.rs:50`)",
            "use serial_nexus_codec_api::{Event, MAX_FRAME_SIZE, encode};\n",
        ),
        (
            "the encoder named in an error message read as a call — the real line is \
             `leg.rs`'s `encode_hello(…).map_err(|e| format!(\"encode hello: {e}\"))`, \
             where a token match sees `encode` beside a `(` belonging to something else",
            "fn f() {\n    encode_hello(&o, &mut f).map_err(|e| format!(\"encode hello: {e}\"))?;\n}",
        ),
        (
            "a test fixture counted as a second framer",
            "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn t() { encode(&ev, &mut o); }\n}",
        ),
    ] {
        assert!(
            code_call_sites(near_miss, "encode").is_empty(),
            "the scanner trips on {what}"
        );
    }

    let root = repo_root();
    let mut sites: Vec<String> = Vec::new();
    let stats = walk_rs(&root.join("daemon/src"), &mut |path, src| {
        for (n, line) in code_call_sites(src, "encode") {
            sites.push(format!("{}:{n}: {line}", rel_path(&root, path)));
        }
    });
    // The walker's own floor: `daemon/src` is tens of files, and a walk that reached
    // none of them reports the same green as a compliant tree (37-TEST-1).
    assert!(
        stats.files >= 10,
        "the walk over daemon/src reached {} .rs files — it stopped walking, and the \
         comparison below is against nothing",
        stats.files
    );
    assert!(
        stats.unreadable.is_empty(),
        "daemon/src has unreadable directories, so this scan is not complete: {:?}",
        stats.unreadable
    );

    assert_eq!(
        sites.len(),
        1,
        "the daemon's product code calls the envelope encoder in {} places. Invariant \
         3 gives it one: `runtime.rs`'s `data_frames`, which fragments at \
         `frame_payload_cap` and reports a residual rather than skipping on an encode \
         error (§15.27). A second call site is a second framer, and the helper's own \
         tests cannot see it — they call `data_frames` by name. If this is deliberate, \
         it is a design amendment, not a patch:\n  {}",
        sites.len(),
        sites.join("\n  ")
    );
    assert!(
        sites[0].starts_with("daemon/src/runtime.rs:"),
        "the daemon's one envelope-encode call moved out of `runtime.rs`, where \
         `data_frames` and its fragmentation tests live: {}",
        sites[0]
    );
}

/// **The boundary supervisor is the only thing that starts a thread** (§16.1 — plan
/// §18 item 64(g)).
///
/// §16.1 exists because three of the five worst audit findings were instance-level
/// violations of the same hand-rolled lifecycle rules — concurrent halves,
/// park-don't-teardown, loss notification, join-then-transition — re-derived per
/// node. One supervisor encodes them once and is property-tested once; serial, exec
/// and leg are rebased onto it. Nothing forbade a fourth node from spawning its own
/// pair, and a fourth would pass every test the supervisor has, because those tests
/// drive the supervisor.
///
/// A hand-rolled boundary must start an OS thread, and the daemon's product code
/// does that in exactly two places, both named here. The second is not a boundary
/// and is why this gate is an allowlist rather than a count of one: `watch_stdin_eof`
/// is the leash watcher — deliberately detached, no stop flag, no join, reclaimed by
/// process exit — and its module doc argues that shape at length. Adding a third
/// means either rebasing onto [`BlockingWorker`] or amending §16.1.
#[test]
fn the_daemon_starts_an_os_thread_only_in_the_supervisor_and_the_leash() {
    // 0. The matcher, in both spellings an OS-thread start can take, and against the
    //    three near-misses — all real lines in this tree.
    const SPELLINGS: [&str; 2] = ["thread::spawn(", "thread::Builder"];
    let scan = |src: &str| -> Vec<(usize, String)> {
        let mut v: Vec<(usize, String)> = SPELLINGS
            .iter()
            .flat_map(|n| code_lines_containing(src, n))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    assert_eq!(
        scan("fn f() { std::thread::spawn(move || {}); }").len(),
        1,
        "the scanner misses the bare `std::thread::spawn` spelling"
    );
    assert_eq!(
        scan("fn f() { std::thread::Builder::new().name(n).spawn(body) }").len(),
        1,
        "the scanner misses the named-builder spelling, which is the one the \
         supervisor uses and therefore the one a copy of it would use"
    );
    for (what, near_miss) in [
        (
            "a subprocess spawn (`exec.rs`'s `cmd.spawn()`) read as an OS thread",
            "fn f() { let mut child = match cmd.spawn() { Ok(c) => c, Err(e) => return }; }",
        ),
        (
            "a thread named in an ERROR MESSAGE read as a thread start — the real \
             lines are `pty.rs`'s and `log.rs`'s `format!(\"spawn pty writer thread: \
             {e}\")`, which a token match on `spawn` plus `thread` reads as two \
             hand-rolled boundaries",
            "fn f() { Fault { reason: format!(\"spawn pty writer thread: {e}\") } }",
        ),
        (
            "a test's helper thread counted as a second boundary — `boundary.rs`'s own \
             bounded-join helper is exactly that, so this gate would be red on arrival",
            "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn t() { std::thread::spawn(|| {}); }\n}",
        ),
    ] {
        assert!(scan(near_miss).is_empty(), "the scanner trips on {what}");
    }

    let root = repo_root();
    // The two sanctioned starts, by file rather than by line: a line number rots on
    // the next edit, and a gate that rots is a gate that gets relaxed.
    const SANCTIONED: [&str; 2] = ["daemon/src/boundary.rs", "daemon/src/lib.rs"];
    let mut sites: Vec<String> = Vec::new();
    let stats = walk_rs(&root.join("daemon/src"), &mut |path, src| {
        let rel = rel_path(&root, path);
        for (n, line) in scan(src) {
            sites.push(format!("{rel}:{n}: {line}"));
        }
    });
    assert!(
        stats.files >= 10,
        "the walk over daemon/src reached {} .rs files — it stopped walking",
        stats.files
    );

    let strays: Vec<&String> = sites
        .iter()
        .filter(|s| !SANCTIONED.iter().any(|ok| s.starts_with(&format!("{ok}:"))))
        .collect();
    assert!(
        strays.is_empty(),
        "the daemon's product code starts an OS thread outside `boundary.rs`'s \
         `spawn_named` and `lib.rs`'s stdin leash. §16.1 gives the tree one boundary \
         supervisor precisely because the lifecycle rules — concurrent halves, \
         park-don't-teardown, loss notification, join-then-transition — were being \
         re-derived by hand per node, and a hand-rolled pair passes every test \
         `BlockingWorker` has. Rebase onto `BlockingWorker::arm`/`arm_quiet`, or amend \
         §16.1:\n  {}",
        strays
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    // ...and the allowlist must still be *reached*, or this gate passes because the
    // scan found nothing at all rather than because it found the right things.
    for ok in SANCTIONED {
        assert!(
            sites.iter().any(|s| s.starts_with(&format!("{ok}:"))),
            "the scan found no OS-thread start in {ok} — either the supervisor's \
             `spawn_named` moved, or the matcher stopped matching, and in both cases \
             a stray somewhere else would now go unnoticed. Found: {sites:?}"
        );
    }
}

/// **The bash validation suite stays retired** (§16.11 — plan §18 item 64(g)).
///
/// §16.11 is EXECUTED: the shell suite folded into the harness, and `scripts/` holds
/// only `bless` (§15.45). What held it retired was that nobody had written one back.
/// The three surviving wrappers — the license gate, the external-consumer build, the
/// wait helper — each live in the harness now, and each has a test that would keep
/// passing beside a `.sh` that quietly did the same job again.
///
/// Two assertions, because "retired" has two halves: no `.sh` anywhere in the tree,
/// and `scripts/` holding exactly what §16.11 says it holds. Both are the design's
/// own words; a script that earns its place earns an amendment first.
#[test]
fn no_shell_script_has_come_back() {
    let root = repo_root();

    // 0. The walker, against a planted violation in a scratch tree — the half review
    //    37 found missing, where the planted offender was always a string and never
    //    a file.
    let scratch = Scratch::new("shell");
    scratch.write(
        "scripts/validate/phase3/firehose.sh",
        "#!/usr/bin/env bash\n",
    );
    scratch.write("scripts/bless", "#!/usr/bin/env bash\n");
    let (planted, _) = shell_scripts_under(scratch.path());
    assert_eq!(
        planted,
        vec!["scripts/validate/phase3/firehose.sh".to_owned()],
        "the walker does not surface a `.sh` planted in a nested directory — which is \
         where the retired suite lived, so a walker that only reads the top level \
         proves nothing"
    );

    // 1. The tree, with the walker's own floor.
    let (found, files) = shell_scripts_under(&root);
    assert!(
        files >= 100,
        "the walk over the tree reached {files} files — it stopped walking, and a \
         walker that stopped reports the same green as a clean tree (37-TEST-1)"
    );
    assert!(
        found.is_empty(),
        "shell scripts are back in the tree. §16.11 retired the bash validation suite \
         into the harness, and plan §5 states the canonical form; a script here is a \
         validation path CI does not run, cannot lint, and no meta-gate reads. If it \
         earns its place, amend §16.11 first:\n  {}",
        found.join("\n  ")
    );

    // 2. And `scripts/` holds exactly what §16.11 says it holds. A `.sh` is the
    //    spelling the suite wore; an extensionless file with a `#!` line is the same
    //    thing wearing `bless`'s clothes, and the sentence in §16.11 is what refuses
    //    it without this gate having to guess at shebangs.
    let mut entries: Vec<String> = std::fs::read_dir(root.join("scripts"))
        .expect("scripts/ is readable")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["bless".to_owned()],
        "§16.11 says `scripts/` holds only `bless` (§15.45's build + install + setcap). \
         Anything else there is a validation or tooling path outside the harness — the \
         state §16.11 ended — and belongs in the harness or in an amendment"
    );
}

/// Every `.sh` file below `dir`, repo-relative, with the number of files walked.
///
/// Walks everything rather than only `.rs`, since the subject is a file that is not
/// Rust; skips the same generated and vendored trees [`walk_rs`] does.
fn shell_scripts_under(dir: &Path) -> (Vec<String>, usize) {
    fn inner(root: &Path, dir: &Path, out: &mut Vec<String>, files: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if matches!(name.as_ref(), "target" | ".git" | "node_modules")
                    || is_fuzz_scratch(&path)
                    || is_nested_checkout(&path)
                {
                    continue;
                }
                inner(root, &path, out, files);
            } else {
                *files += 1;
                if name.ends_with(".sh") {
                    out.push(rel_path(root, &path));
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut files = 0usize;
    inner(dir, dir, &mut out, &mut files);
    out.sort();
    (out, files)
}
