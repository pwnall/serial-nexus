//! Naming and context meta-gates (design §15.40 and §15.41; plan §17.2 and §17.4).
//!
//! Two scans over the whole tree, each with the shape review 32 item 7 demanded of a
//! scanning meta-gate: **plant the violation, prove the detector fires**, then assert a
//! floor on what was actually visited, so a walker that quietly stopped walking cannot
//! report green.
//!
//! * [`retired_names_appear_only_where_history_lives`] — §15.40's rename is
//!   un-reintroducible: the retired vocabulary (`serialnexusd`, `serialnexusctl`,
//!   `serialnexusweb`, and the bare `nexus-*`/`nexus_*` crate tokens) survives only in
//!   `docs/historical/`, in the captured doctor artifacts, and in the one compatibility
//!   shim that must *read* a pre-rename default for a release.
//! * [`consumer_context_terms_appear_only_where_the_ban_is_stated`] — §15.41's privacy
//!   rule, which explicitly outranks the frozen-history rule, so this one has **no**
//!   historical exemption.
//!
//! Why the two live together: they are the same mechanism (a whole-tree token scan with
//! a justified allowlist) and they share the walker, so a bug in the walker fails both
//! rather than being fixed once and forgotten once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Repository root: this file is `<root>/itest/tests/meta_names.rs`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("itest/ has a parent")
        .to_path_buf()
}

/// Directories never worth scanning: build output, installed dependency trees, and
/// captured browser-test debris. None of them are tracked content.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "test-results",
    "playwright-report",
];

/// Is `dir` the root of a **nested checkout** — another worktree or clone living inside
/// this one?
///
/// Skipped, because a nested checkout is a second copy of this tree: every retired name
/// and every consumer-context term in it is reported as a violation *of this tree*, at a
/// path that is not this tree's. Measured: one `git worktree add` under
/// `.claude/worktrees/` turns both gates in this file red, plus two in `meta_gates.rs`.
///
/// Keyed on "is a checkout" rather than on the directory's *name*, because a name list
/// has to guess where worktrees get put — and `.claude` specifically must **not** be
/// skipped wholesale, since `.claude/settings.json` is tracked and §15.41's privacy rule
/// is tree-wide. `git worktree add` leaves a `.git` **file**; a clone leaves a `.git`
/// **directory**; `exists()` covers both, and the root handed to [`walk_text`] is itself
/// never tested.
fn is_nested_checkout(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Walk every readable UTF-8 file under `dir`, handing `(path relative to root, text)`
/// to `visit`. Non-UTF-8 files are skipped, which is exactly right: a token scan has
/// nothing to say about a binary, and `read_to_string` is the cheapest way to tell.
fn walk_text(root: &Path, dir: &Path, visit: &mut impl FnMut(&str, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || is_nested_checkout(&path) {
                continue;
            }
            walk_text(root, &path, visit);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            visit(&rel, &text);
        }
    }
}

/// Is the byte at `i` (or the one before `hay[i..]`) part of a longer identifier, so
/// that a hit is a coincidence rather than a use of the token?
///
/// Identifier characters only — `-` deliberately does **not** count, because
/// `serial-nexus-core` really does contain `nexus-core` and the whole point of the
/// [`RETIRED`] scan is that the *new* name must not be reported as the old one. That
/// case is handled by [`preceded_by_family_prefix`] instead, which is narrower and
/// says what it means.
fn glued(hay: &str, at: usize, len: usize) -> bool {
    let before = hay[..at].chars().next_back();
    let after = hay[at + len..].chars().next();
    let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    ident(before) || ident(after)
}

/// Does the occurrence at `at` sit immediately after the family prefix — i.e. is this
/// `nexus-core` inside `serial-nexus-core`, or `nexus_core` inside `serial_nexus_core`?
/// Those are the *converted* names and must not be reported.
fn preceded_by_family_prefix(hay: &str, at: usize) -> bool {
    hay[..at].ends_with("serial-") || hay[..at].ends_with("serial_")
}

/// Every offending occurrence of `token` in `text`, as 1-based line numbers.
fn hits(text: &str, token: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices(token) {
        if glued(text, at, token.len()) || preceded_by_family_prefix(text, at) {
            continue;
        }
        out.push(text[..at].matches('\n').count() + 1);
    }
    out
}

/// Case-insensitive, hyphen-blind, suffix-tolerant matcher for the prose terms of
/// §15.41. Three deliberate differences from [`hits`], each one a hole this gate had
/// on its first run:
///
/// * **case** — the terms open sentences and sit inside code spans in the same file;
/// * **hyphens folded to spaces** — `closed-repo` and `closed repo` are the same
///   phrase, and the first slipped past a scan that only knew the second (it was
///   sitting in `--help` output at the time);
/// * **no trailing boundary** — `closed repository`, `closed repositories` and
///   `closed-sourced` are the same assertion with a suffix, and requiring a word
///   boundary *after* the token silently exempted every one of them.
///
/// The leading boundary stays, so `unclosed source` is not a hit. Folding `-` to a
/// space preserves byte length, so reported line numbers stay true.
fn hits_ci(text: &str, token: &str) -> Vec<usize> {
    let hay = text.to_lowercase().replace('-', " ");
    let mut out = Vec::new();
    for (at, _) in hay.match_indices(token) {
        let before = hay[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        out.push(hay[..at].matches('\n').count() + 1);
    }
    out
}

// ---------------------------------------------------------------------------------
// §15.40 — the retired vocabulary
// ---------------------------------------------------------------------------------

/// The names §15.40 retired. `codec-api` and `codec-reference` are deliberately absent:
/// the first is still a *directory* name and the second is still a Cargo *feature*
/// name, both legitimately, and plan §17.2 names exactly this set.
const RETIRED: &[&str] = &[
    "serialnexusd",
    "serialnexusctl",
    "serialnexusweb",
    "nexus-core",
    "nexus_core",
    "nexus-rpc",
    "nexus_rpc",
    "nexus-sys",
    "nexus_sys",
    "nexus-daemon",
    "nexus_daemon",
    "nexus-sim",
    "nexus_sim",
    "nexus-doctor",
    "nexus_doctor",
    "nexus-itest",
    "nexus_itest",
];

/// Path prefixes where the retired vocabulary is *supposed* to survive.
///
/// `docs/historical/` is design §15.40's own carve-out — superseded generations are
/// records of what was decided when, and rewriting them would make them lie. The
/// `docs/doctor/` entries are captured tool output under §16.13, whose whole value is
/// that they are the bytes a run actually emitted: editing one to say a nicer name
/// falsifies a dated measurement, which is the opposite of what committing it was for.
/// `docs/doctor/README.md` is prose about those artifacts and is **not** exempt.
const HISTORY_PREFIXES: &[&str] = &[
    "docs/historical/",
    "docs/doctor/linux-6.18-2026-07-29-tier3.md",
    "docs/doctor/linux-7.0-2026-07-29-passive-",
];

/// The one class of *live* file that may still spell a retired name, with the exact
/// number of occurrences each is allowed.
///
/// Plan §17.3 keeps the pre-rename socket and state-file defaults readable for one
/// release, which cannot be done without naming them once. The allowance is a count
/// rather than a blanket pass on purpose: adding a *second* retired name to one of
/// these files still fails this gate, and so does removing the compatibility shim
/// without removing its entry here — an allowlist nobody has to maintain is an
/// allowlist that silently grows.
///
const COMPAT_ALLOWANCE: &[(&str, usize)] = &[
    // The single definition: `LEGACY_DAEMON_NAME`'s value, and nothing else. Every
    // other site — the daemon's adoption branch, both clients' fallback, and the guard
    // in `p13_legacy_defaults.rs` — derives from that constant rather than repeating it,
    // which is why this list is two entries long and not a dozen.
    ("rpc/src/lib.rs", 1),
    // The operator-facing upgrade note. Someone upgrading has these files sitting on
    // disk under their old names and has to be able to recognise them: the two
    // fallbacks, the retired daemon name, and the retired CLI name.
    ("packaging/README.md", 4),
];

/// **§15.40's rename is un-reintroducible.**
///
/// The rename is only worth its churn if the old vocabulary cannot drift back in one
/// merge at a time, and prose is where it drifts: a comment naming `serialnexusd`
/// compiles perfectly.
#[test]
fn retired_names_appear_only_where_history_lives() {
    let root = repo_root();

    // 1. Fail-first, on the real mechanism rather than only on the matcher. Plant a
    //    file carrying a retired name in a scratch tree and prove the *walker* surfaces
    //    it; the token is assembled from pieces so this file carries no literal of its
    //    own for a later scan to trip over.
    let scratch =
        std::env::temp_dir().join(format!("snx-meta-names-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("nested")).expect("create scratch");
    let planted = format!("{}{}{}", "serial", "nexus", "d");
    std::fs::write(
        scratch.join("nested/thing.rs"),
        format!("// the {planted} used to live here\n"),
    )
    .expect("write planted file");
    // The same name inside a nested **worktree** and a nested **clone**, neither of which
    // is part of this tree and neither of which may be reported. A skip is a matcher and
    // gets planted in every spelling it claims to cover (AGENTS §3): keying on the
    // directory's name rather than on "is a checkout" catches one of these and misses the
    // other, and the assertion below is what says which.
    std::fs::create_dir_all(scratch.join("wt")).expect("create scratch worktree");
    std::fs::write(
        scratch.join("wt/.git"),
        "gitdir: /elsewhere/.git/worktrees/wt\n",
    )
    .expect("write gitfile");
    std::fs::write(
        scratch.join("wt/thing.rs"),
        format!("// the {planted} used to live here\n"),
    )
    .expect("write planted file in the worktree");
    std::fs::create_dir_all(scratch.join("clone/.git")).expect("create scratch clone");
    std::fs::write(
        scratch.join("clone/thing.rs"),
        format!("// the {planted} used to live here\n"),
    )
    .expect("write planted file in the clone");
    let mut planted_hits = Vec::new();
    walk_text(&scratch, &scratch, &mut |rel, text| {
        for token in RETIRED {
            if !hits(text, token).is_empty() {
                planted_hits.push(format!("{rel}:{token}"));
            }
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        planted_hits,
        vec![format!("nested/thing.rs:{planted}")],
        "the walker or the matcher missed a planted retired name, or surfaced one from a \
         nested worktree/clone — this gate would either pass over anything or convict the \
         repository of a copy's contents"
    );

    // 2. …and the converted names must NOT fire, or the gate is unusable and gets
    //    deleted rather than fixed. This is the case a naive substring scan gets wrong:
    //    `serial-nexus-core` contains `nexus-core`.
    for benign in [
        "serial-nexus-core",
        "serial_nexus_core",
        "serial-nexus-daemon-bin",
        "serial_nexus_daemon::run",
        "use serial_nexus_sys::poll_ready;",
        "cargo test -p serial-nexus-itest",
    ] {
        for token in RETIRED {
            assert!(
                hits(benign, token).is_empty(),
                "{benign:?} was reported as containing the retired name {token:?}"
            );
        }
    }

    // 3. The real scan.
    let this_file = "itest/tests/meta_names.rs";
    let allowance: BTreeMap<&str, usize> = COMPAT_ALLOWANCE.iter().copied().collect();
    let mut visited = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    let mut allowed_seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut history_seen = 0usize;
    walk_text(&root, &root, &mut |rel, text| {
        visited += 1;
        if rel == this_file {
            return;
        }
        let found: usize = RETIRED.iter().map(|t| hits(text, t).len()).sum();
        if found == 0 {
            return;
        }
        if HISTORY_PREFIXES.iter().any(|p| rel.starts_with(p)) {
            history_seen += 1;
            return;
        }
        if let Some((&key, &budget)) = allowance.get_key_value(rel) {
            *allowed_seen.entry(key).or_default() += found;
            if found > budget {
                offenders.push(format!(
                    "{rel}: {found} retired-name occurrences, but only {budget} are \
                     allowed for the plan §17.3 compatibility window"
                ));
            }
            return;
        }
        for token in RETIRED {
            for line in hits(text, token) {
                offenders.push(format!("{rel}:{line} -> {token}"));
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "retired names outside docs/historical/ and the §17.3 compatibility window: \
         {offenders:#?}\n\
         Design §15.40 renamed these; the old vocabulary lives in docs/historical/ and \
         nowhere else. Directories are short stems (core/, rpc/, sys/, daemon/, \
         daemon-bin/, ctl/, web/, sim/, doctor/, itest/) — if this fired on a \
         filesystem path, spell the DIRECTORY, not the crate."
    );

    // 4. Not vacuous, three ways: the walk covered the tree, the history exemption is
    //    still load-bearing, and every compatibility allowance is still being used. An
    //    allowance whose file stopped needing it is a stale exemption, and stale
    //    exemptions are how a gate stops being one.
    assert!(
        visited >= 200,
        "only {visited} files walked — the walker has stopped seeing the tree"
    );
    assert!(
        history_seen > 0,
        "no file under docs/historical/ carries a retired name — either history was \
         rewritten or the exemption is now dead and should be deleted"
    );
    for (path, budget) in COMPAT_ALLOWANCE {
        let seen = allowed_seen.get(path).copied().unwrap_or(0);
        assert_eq!(
            seen, *budget,
            "{path} is allowed {budget} retired-name occurrence(s) for the §17.3 \
             compatibility window but carries {seen}. If the window has closed, delete \
             the entry; if the shim moved, move the entry."
        );
    }
}

// ---------------------------------------------------------------------------------
// §15.41 — capabilities, never consumers
// ---------------------------------------------------------------------------------

/// The three phrasings §15.40 bans outright. They have no legitimate use in this
/// document set: each asserts something about *who* consumes the extension surface
/// rather than what the surface supports.
/// The phrasings §15.41 bans outright, written in the folded form [`hits_ci`] scans:
/// lowercase, hyphens already collapsed to spaces, and truncated to the stem so every
/// inflection is covered (`closed repo` catches `closed repository` and
/// `closed repositories`; `known repositor` catches both numbers). They have no
/// legitimate use in this document set — each asserts something about *who* consumes
/// the extension surface rather than what the surface supports.
const BANNED_CONTEXT: &[&str] = &["closed source", "closed repo", "known repositor"];

/// The files that may name the banned terms, with the exact number of occurrences
/// each is allowed — because a rule has to be able to state what it bans.
///
/// Counted rather than blanket-exempted for the same reason as [`COMPAT_ALLOWANCE`]:
/// these three files are precisely the ones a well-meaning edit would add a fourth
/// mention to.
const BAN_STATEMENTS: &[(&str, usize)] = &[
    ("docs/35-design-claude-fable-v14.md", 3),
    ("docs/36-implementation-plan-claude-fable-v14.md", 3),
    ("AGENTS.md", 3),
    // The archived operating manual carries the same §10 rule statement. It is under
    // `docs/historical/`, which buys it nothing here — §15.41 outranks the frozen-history
    // rule — but a rule still has to be able to name what it bans, in whatever generation
    // states it.
    ("docs/historical/34-claude-fable-agents.md", 3),
];

/// **§15.41's privacy rule, with no historical exemption.**
///
/// This is the one norm the design explicitly ranks above the frozen-history rule:
/// leaked business context is not decision history, so `docs/historical/` is scrubbed
/// too and gets no pass here.
#[test]
fn consumer_context_terms_appear_only_where_the_ban_is_stated() {
    let root = repo_root();

    // 1. Fail-first on the walker, exactly as above.
    let scratch =
        std::env::temp_dir().join(format!("snx-meta-ctx-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("docs/historical")).expect("create scratch");
    let planted = format!("{}-{}", "Closed", "Source");
    // Deliberately hyphenated and capitalised: the fold and the case-insensitivity are
    // both load-bearing, and a plant that exercised neither proved neither.
    std::fs::write(
        scratch.join("docs/historical/old.md"),
        format!("A {planted} consumer pins this by tag.\n"),
    )
    .expect("write planted file");
    let mut planted_hits = Vec::new();
    walk_text(&scratch, &scratch, &mut |rel, text| {
        for token in BANNED_CONTEXT {
            if !hits_ci(text, token).is_empty() {
                planted_hits.push(rel.to_owned());
            }
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        planted_hits,
        vec!["docs/historical/old.md".to_owned()],
        "the planted term was not caught under docs/historical/ — §15.41 ranks the \
         privacy rule ABOVE the frozen-history rule, so history gets no exemption here"
    );

    // 2. The real scan.
    let this_file = "itest/tests/meta_names.rs";
    let allowance: BTreeMap<&str, usize> = BAN_STATEMENTS.iter().copied().collect();
    let mut visited = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    let mut allowed_seen: BTreeMap<&str, usize> = BTreeMap::new();
    walk_text(&root, &root, &mut |rel, text| {
        visited += 1;
        if rel == this_file {
            return;
        }
        let found: usize = BANNED_CONTEXT.iter().map(|t| hits_ci(text, t).len()).sum();
        if found == 0 {
            return;
        }
        if let Some((&key, &budget)) = allowance.get_key_value(rel) {
            *allowed_seen.entry(key).or_default() += found;
            if found > budget {
                offenders.push(format!(
                    "{rel}: {found} banned-term occurrences, but the rule statement \
                     needs only {budget}"
                ));
            }
            return;
        }
        for token in BANNED_CONTEXT {
            for line in hits_ci(text, token) {
                offenders.push(format!("{rel}:{line} -> {token}"));
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "consumer-flavoured context terms outside the three files that state the ban: \
         {offenders:#?}\n\
         Design §15.41: the documents describe capabilities and never assert the \
         existence, count, or nature of external consumers. Say \"out-of-tree\"; \
         paraphrase rather than quote when older material violates this."
    );
    assert!(
        visited >= 200,
        "only {visited} files walked — the walker has stopped seeing the tree"
    );
    for (path, budget) in BAN_STATEMENTS {
        let seen = allowed_seen.get(path).copied().unwrap_or(0);
        assert_eq!(
            seen, *budget,
            "{path} is allowed {budget} banned-term occurrence(s) because it states the \
             rule, but carries {seen}. Update the count deliberately or fix the prose."
        );
    }
}
