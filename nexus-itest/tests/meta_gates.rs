//! Meta-gates ported from phase 0: the checks that guard the codebase itself rather
//! than the daemon. From `scripts/validate/phase0/unsafe-gate.sh` (design §16.3 —
//! `unsafe` confined to `nexus-sys`) and `phase0/doctor.sh` (§15.17 — no probe reports
//! `unsupported`). Portable Rust: no `jq`, no shell `grep`.
//!
//! (The license gate is now `tests/p0_license_gate.rs`, which self-skips without
//! cargo-deny; §16.11 folded the last three shell scripts into Rust, so no bash remains.)
//!
//! ## The invariant tripwires (review 26)
//!
//! Two of AGENTS.md §6's invariants were upheld by review alone, and one of them —
//! invariant 5's clippy `RefCell` ban — **silently stopped working for a whole release**
//! when the daemon's state moved from `serialnexusd` to the sibling crate `nexus-daemon`
//! at the v8 library/binary split (INV5-CLIPPY-SCOPE: clippy resolves `clippy.toml`
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
//! Both, like [`unsafe_is_confined_to_nexus_sys`], carry a **planted-violation
//! self-proof**: a gate whose detector silently stopped detecting is the failure mode
//! being guarded against, so each proves it catches a synthetic offender (and, since
//! these two must read past legitimate prose, that it does *not* trip on a comment)
//! before it trusts its own clean verdict.

use std::path::{Path, PathBuf};
use std::process::Command;

use nexus_itest::bin;
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/nexus-itest.
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
/// [`unsafe_is_confined_to_nexus_sys`] applies inline).
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
/// they are banned (`nexus-sys/src/lib.rs` and `nodes/serial.rs` both discuss
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
/// (AGENTS.md §6 invariant 5 / design §16.2). Two, not one: `serialnexusd` owns the
/// thin binary and kept the original file, `nexus-daemon` owns every line of daemon
/// state since the v8 split (§15.26) and is where the ban actually bites.
const REFCELL_BAN_CRATES: &[&str] = &["nexus-daemon", "serialnexusd"];

/// The one sanctioned `RefCell` in the daemon: `CriticalCell`'s own internals, which
/// carry a localized `#[allow(clippy::disallowed_types)]` (§16.2).
const REFCELL_EXEMPT: &[&str] = &["cell.rs"];

#[test]
fn unsafe_is_confined_to_nexus_sys() {
    // 1. Prove the detector actually catches an `unsafe` usage. The sample is built by
    //    concatenation so this source file itself carries no literal match.
    let planted = format!("fn f() {{ {} {{ let _ = 1; }} }}", "unsafe");
    assert!(
        has_unsafe_usage(&planted),
        "the detector does not catch a planted unsafe usage"
    );

    // 2. No `.rs` outside `nexus-sys/` may contain an `unsafe` usage.
    let root = repo_root();
    let detector = Path::new(file!()).file_name().map(|n| n.to_os_string());
    let mut offenders = Vec::new();
    walk_rs(&root, &mut |path, src| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        if rel.starts_with("nexus-sys/") {
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
        "`unsafe` found outside nexus-sys/: {offenders:?}"
    );

    // 3. Sanity: nexus-sys genuinely carries the unsafe (else the split is a lie).
    let sys = std::fs::read_to_string(root.join("nexus-sys/src/lib.rs")).expect("read nexus-sys");
    assert!(
        has_unsafe_usage(&sys),
        "nexus-sys carries no unsafe — the extraction target is wrong"
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

    for krate in REFCELL_BAN_CRATES {
        let dir = root.join(krate);
        assert!(dir.is_dir(), "ban-list crate {krate} does not exist");

        // 1. No raw RefCell in the crate's sources, `cell.rs` excepted.
        let mut offenders = Vec::new();
        for (path, src) in sources_under(&dir.join("src")) {
            let file = path.file_name().unwrap_or_default().to_string_lossy();
            if REFCELL_EXEMPT.contains(&file.as_ref()) {
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
            "raw `{cell}` outside {}: {offenders:?} — daemon state belongs in \
             `CriticalCell` (§16.2)",
            REFCELL_EXEMPT.join("/"),
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

    // Sanity: the ban list is not vacuous — `nexus-daemon` genuinely carries the
    // sanctioned `CriticalCell`, so assertion 3 is scanning for something real.
    let cell_rs = std::fs::read_to_string(root.join("nexus-daemon/src/cell.rs"))
        .expect("read nexus-daemon/src/cell.rs");
    assert!(
        has_code_token(&cell_rs, &cell) && has_code_token(&cell_rs, &critical),
        "nexus-daemon/src/cell.rs no longer wraps a {cell} — the exemption is stale"
    );
}

/// **Invariant 1 (INV1-NO-GUARD).** `tokio::io::unix::AsyncFd`'s epoll readiness
/// fires spuriously and persistently on a pty master — `read(2)` returns EAGAIN
/// while epoll insists otherwise — and because the ready future completes
/// synchronously the loop never yields, starving the current-thread runtime and
/// freezing the control plane with it (§15.18/§15.19). Readiness for tty-family fds
/// is `nexus_sys::poll_ready`/`poll_blocking` instead.
///
/// That was upheld by review alone until this gate. **The allowlist is empty and must
/// stay empty**: every occurrence in the tree today is prose explaining the ban (in
/// `nexus-sys/src/lib.rs`, `runtime.rs`, `nodes/pty.rs`, `nodes/serial.rs`, AGENTS.md),
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
         (it busy-loops and starves the runtime, §15.18); use `nexus_sys::poll_ready` \
         / `poll_blocking`"
    );

    // 3. Sanity: the replacement the invariant names is really what the tree uses, so
    //    a clean verdict means "poll(2) everywhere", not "no readiness code left".
    let sys = std::fs::read_to_string(root.join("nexus-sys/src/lib.rs")).expect("read nexus-sys");
    assert!(
        has_code_token(&sys, "poll_ready") && has_code_token(&sys, "poll_blocking"),
        "nexus-sys no longer exposes poll_ready/poll_blocking — invariant 1's \
         replacement is gone and this gate is measuring nothing"
    );
}

#[test]
fn doctor_reports_no_unsupported_capability() {
    let out = Command::new(bin("nexus-doctor"))
        .arg("--json")
        .output()
        .expect("run nexus-doctor");
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
/// `nexus-daemon` and `serialnexusweb` each expose a `pub mod unstable_fuzz_api` that
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
    for krate in ["nexus-daemon", "serialnexusweb"] {
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
