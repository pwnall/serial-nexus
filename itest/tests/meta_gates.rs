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

fn walk_rs(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
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
            walk_rs(&path, visit);
        } else if name.ends_with(".rs")
            && let Ok(src) = std::fs::read_to_string(&path)
        {
            visit(&path, &src);
        }
    }
}

/// Is `path` this detector file? It names the very tokens it scans for, so every
/// gate here excludes itself (the same self-exclusion
/// [`unsafe_is_confined_to_serial_nexus_sys`] applies inline).
fn is_this_file(path: &Path) -> bool {
    path.file_name() == Path::new(file!()).file_name()
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
fn crate_dirs(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
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
            walk(&path, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// Every `.rs` under `dir`, as (path, source).
fn sources_under(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    walk_rs(dir, &mut |p, src| {
        out.push((p.to_path_buf(), src.to_owned()))
    });
    out
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
    // 1. Prove the detector actually catches an `unsafe` usage. The sample is built by
    //    concatenation so this source file itself carries no literal match.
    let planted = format!("fn f() {{ {} {{ let _ = 1; }} }}", "unsafe");
    assert!(
        has_unsafe_usage(&planted),
        "the detector does not catch a planted unsafe usage"
    );

    // 2. No `.rs` outside `serial-nexus-sys` may contain an `unsafe` usage. The
    //    directory is `sys/`, not the crate name: §15.40 renamed the packages and
    //    deliberately left the tree layout short, so a gate that scans the
    //    filesystem must spell the *directory*.
    let root = repo_root();
    let detector = Path::new(file!()).file_name().map(|n| n.to_os_string());
    let mut offenders = Vec::new();
    walk_rs(&root, &mut |path, src| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        if rel.starts_with("sys/") {
            return;
        }
        // Self-exclude this detector file (it names the keywords it scans for).
        if path.file_name().map(|n| n.to_os_string()) == detector {
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

    // 3. Sanity: serial-nexus-sys genuinely carries the unsafe (else the split is a lie).
    let sys = std::fs::read_to_string(root.join("sys/src/lib.rs")).expect("read sys/src/lib.rs");
    assert!(
        has_unsafe_usage(&sys),
        "serial-nexus-sys carries no unsafe — the extraction target is wrong"
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

    for krate in REFCELL_BAN_CRATES {
        let dir = root.join(krate);
        assert!(dir.is_dir(), "ban-list crate {krate} does not exist");

        // 1. No raw RefCell in the crate's sources, the one exempt path excepted.
        let mut offenders = Vec::new();
        for (path, src) in sources_under(&dir.join("src")) {
            if is_refcell_exempt(&root, &path) {
                continue;
            }
            if has_code_token(&src, &cell) {
                offenders.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned(),
                );
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
    let critical = format!("Critical{}", "Cell");
    let mut uncovered = Vec::new();
    for dir in crate_dirs(&root) {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if REFCELL_BAN_CRATES.contains(&name.as_str()) {
            continue;
        }
        let uses_it = sources_under(&dir.join("src"))
            .iter()
            .any(|(p, src)| !is_this_file(p) && has_code_token(src, &critical));
        if uses_it {
            uncovered.push(
                dir.strip_prefix(&root)
                    .unwrap_or(&dir)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert!(
        uncovered.is_empty(),
        "these crates hold daemon state ({critical}) but are not in REFCELL_BAN_CRATES: \
         {uncovered:?} — add a clippy.toml beside each manifest and list it here \
         (this is exactly how the ban broke at the v8 library/binary split)"
    );

    // Sanity: the ban list is not vacuous — `serial-nexus-daemon` genuinely carries the
    // sanctioned `CriticalCell`, so assertion 3 is scanning for something real.
    let cell_rs =
        std::fs::read_to_string(root.join("daemon/src/cell.rs")).expect("read daemon/src/cell.rs");
    assert!(
        has_code_token(&cell_rs, &cell) && has_code_token(&cell_rs, &critical),
        "daemon/src/cell.rs no longer wraps a {cell} — the exemption is stale"
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

    // 2. No `.rs` in the workspace may use it. No allowlist (see the doc comment).
    let root = repo_root();
    let mut offenders = Vec::new();
    walk_rs(&root, &mut |path, src| {
        if is_this_file(path) {
            return;
        }
        if has_code_token(src, &token) {
            offenders.push(
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    });
    assert!(
        offenders.is_empty(),
        "`{token}` is used in {offenders:?} — invariant 1 forbids it on pty/tty fds \
         (it busy-loops and starves the runtime, §15.18); use `serial_nexus_sys::poll_ready` \
         / `poll_blocking`"
    );

    // 3. Sanity: the replacement the invariant names is really what the tree uses, so
    //    a clean verdict means "poll(2) everywhere", not "no readiness code left".
    let sys = std::fs::read_to_string(root.join("sys/src/lib.rs")).expect("read sys/src/lib.rs");
    assert!(
        has_code_token(&sys, "poll_ready") && has_code_token(&sys, "poll_blocking"),
        "serial-nexus-sys no longer exposes poll_ready/poll_blocking — invariant 1's \
         replacement is gone and this gate is measuring nothing"
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
#[test]
fn every_unstable_fuzz_api_export_has_a_fuzz_target() {
    let root = repo_root();
    let fuzz_dir = root.join("fuzz/fuzz_targets");
    assert!(
        fuzz_dir.is_dir(),
        "fuzz/fuzz_targets is missing; the exception below has no consumer"
    );
    let targets: String = std::fs::read_dir(&fuzz_dir)
        .expect("read fuzz_targets")
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect();

    let mut checked = 0usize;
    for krate in ["daemon", "web"] {
        let lib = root.join(krate).join("src/lib.rs");
        let src =
            std::fs::read_to_string(&lib).unwrap_or_else(|e| panic!("read {}: {e}", lib.display()));
        let Some(at) = src.find("pub mod unstable_fuzz_api") else {
            continue; // the module is allowed to disappear — that is its promise
        };

        // The disclaimer is load-bearing: it is what makes "no embedder can depend on
        // this by accident" true rather than hopeful.
        let doc = &src[..at];
        assert!(
            doc.contains("Not part of") && (doc.contains("fuzz harness") || doc.contains("fuzz")),
            "{krate}'s unstable_fuzz_api must document that it is unsupported"
        );

        // Every identifier in the module's `pub use` list must appear in some target.
        let body = &src[at..];
        let body = &body[..body.find("\n}").expect("module body is brace-delimited")];
        for item in body
            .split("pub use")
            .skip(1)
            .flat_map(|u| u[..u.find(';').unwrap_or(u.len())].split(['{', '}', ',']))
            .filter_map(|t| t.rsplit("::").next())
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_'))
        {
            assert!(
                contains_word(&targets, item),
                "{krate}::unstable_fuzz_api re-exports `{item}`, but no fuzz target \
                 under fuzz/fuzz_targets/ mentions it. The exception to §15.26 is \
                 bounded by exactly this rule: re-export what the fuzzer drives, \
                 nothing else. Add the target, or drop the re-export."
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
    let sources = sources_under(&root.join("core/src"));
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
