#![forbid(unsafe_code)]

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
//! * [`notes_citations_in_code_resolve_to_a_real_section`] — every implementation-notes
//!   reference in a `.rs` file names a section that exists. Added after a commit whose
//!   own subject was prose truth wrote thirteen citations one section off (notes §3.60).
//!
//! Why they live together: they are the same mechanism (a whole-tree scan with a
//! justified scope) and they share the walker, so a bug in the walker fails all of them
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
    // The capability-blessed copy of `serial-nexus-devprep` (design §15.45):
    // gitignored build output, and a directory whose contents are a binary plus
    // whatever `install` put beside it. Excluded for the same reason as `target`.
    ".snx-bin",
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
    // §15.55's rename (notes §3.81). Unlike the v14 entries below, this one retires a
    // name that was *correct* until the helper outgrew it: the binary is invoked for
    // every rig test, and a name ending in the one operation it performs first reads
    // as a far narrower tool than it is. Banned tree-wide at the owner's request, so
    // it cannot drift back one merge at a time — the same reason the v14 names are here.
    "serial-nexus-replug",
    "serial_nexus_replug",
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
    // The entry that *states* the 2026-08-12 rename (notes §3.81) — a rule has to be
    // able to name what it bans, the same reason `BAN_STATEMENTS` exists below. Four:
    // the two tokens as they join the list, the before→after line, and the stale
    // blessed copy an operator must delete by its old path. A counted allowance rather
    // than a blanket pass because this file is append-only and grows: a fifth mention
    // means someone wrote the retired name into a *new* entry, which is exactly the
    // drift the ban is for.
    ("docs/implementation-notes.md", 4),
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
    ("docs/43-design-claude-fable-v17.md", 3),
    ("docs/44-implementation-plan-claude-fable-v17.md", 3),
    ("AGENTS.md", 3),
    // The archived operating manual and the superseded v14–v16 pairs carry the same rule
    // statements. They are under `docs/historical/`, which buys them nothing here —
    // §15.41 outranks the frozen-history rule — but a rule still has to be able to name
    // what it bans, in whatever generation states it.
    ("docs/historical/34-claude-fable-agents.md", 3),
    ("docs/historical/35-design-claude-fable-v14.md", 3),
    (
        "docs/historical/36-implementation-plan-claude-fable-v14.md",
        3,
    ),
    ("docs/historical/39-design-claude-fable-v15.md", 3),
    (
        "docs/historical/40-implementation-plan-claude-fable-v15.md",
        3,
    ),
    ("docs/historical/41-design-claude-fable-v16.md", 3),
    (
        "docs/historical/42-implementation-plan-claude-fable-v16.md",
        3,
    ),
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

// ---------------------------------------------------------------------------------
// implementation-notes citations — every §3.NN in code names a section that exists
// ---------------------------------------------------------------------------------

/// The notes file every `§3.NN` in this tree refers to.
const NOTES: &str = "docs/implementation-notes.md";

/// Every `§3.NN` citation in `text`, as `(1-based line, section number)`.
///
/// **Why `§3.NN` is unambiguously a notes reference here**, which is the whole scoping
/// question: the design's §3 (*Concepts and terminology*) and the plan's §3 (*Validation
/// toolkit*) have no numbered subsections, so neither document can be cited in this
/// shape. Plan §3's numbered rules are cited as `plan §3 rule 8` — a space, not a dot,
/// which this matcher does not touch. The premise is not left as a comment:
/// [`notes_citations_in_code_resolve_to_a_real_section`] asserts it against both
/// documents, so a future generation that grows a `### 3.1` heading in either one turns
/// this gate red rather than letting it resolve citations against the wrong file.
///
/// The scan is **text-level and deliberately so**: a stale citation inside a string
/// literal is worse than one in a comment, because `doctor` prints several of them to an
/// operator (`probes.rs` names notes §3.29 in a P13 status message). A scanner that only
/// understood comments would exempt exactly the citations a human reads.
fn notes_citations(text: &str) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("\u{a7}3.") {
        let rest = &text[at + "\u{a7}3.".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        // A maximal digit run, so `§3.1` and `§3.10` never collapse into each other.
        let Ok(n) = digits.parse::<u32>() else {
            continue;
        };
        out.push((text[..at].matches('\n').count() + 1, n));
    }
    out
}

/// The section numbers `docs/implementation-notes.md` actually defines, from its own
/// `### 3.NN` headings — the file is the authority, never a list kept beside it.
fn notes_sections(md: &str) -> std::collections::BTreeSet<u32> {
    let mut out = std::collections::BTreeSet::new();
    for line in md.lines() {
        let Some(rest) = line.strip_prefix("### 3.") else {
            continue;
        };
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<u32>() {
            out.insert(n);
        }
    }
    out
}

/// **Every implementation-notes citation in code names a section that exists.**
///
/// The defect this exists for is not hypothetical and not subtle: the commit that
/// converted seven guards to the read-after-close rule cited **notes §3.55** in thirteen
/// places, in six files, and §3.55 is a different entry — the teardown ledger. The
/// conversion is §3.56 and the prose sweep it also carried is inside §3.57. Zero `.rs`
/// files in the tree cited §3.56 at all. The commit's own subject was prose truth.
///
/// A citation is the one kind of comment a reader *follows*, so a wrong one costs more
/// than a vague one: it sends the next editor to a section that argues something else,
/// and it reads as confirmation. Nothing in the build could see it, because a number in
/// a comment compiles.
///
/// **What this gate covers, and what it does not — stated first, because the honest
/// answer is uncomfortable.** It catches a citation that resolves to *nothing*. The
/// thirteen above resolved to §3.55, which **exists**, so this gate would not have
/// caught them; a scanner cannot tell that a real section is the wrong real section
/// without reading both for meaning. What it does catch is the adjacent class, and that
/// class is not hypothetical either: a citation written *before* the section it names —
/// which is how both of this defect's cousins arrived, since the wrong number was picked
/// while the right one was still unwritten. It fired on exactly that during its own
/// first run, on this session's own §3.60 citations and (in a window of a few minutes)
/// on another session's §3.59 ones. The uncovered class is a reviewer's job and the
/// discipline of writing the notes entry in the same commit as the code that cites it;
/// this gate makes the second half mechanical.
///
/// Scope is `.rs` only, deliberately. The documents cite each other constantly and in
/// several shapes (`§3.45 E`, `§3.31/§3.55`, historical entries that must keep naming
/// sections a later generation renumbered), and a gate that swept them would either be
/// noisy or need an allowlist per generation. Code has one shape and one meaning, and it
/// is where the defect landed.
///
/// This file is **not** exempt from its own scan, unlike the two token gates above:
/// their planted literals have to live here, and this one's does not — it is assembled
/// at runtime from [`ABSENT`], so the scanner can walk this file like any other and a
/// stale citation in this very comment would fail the gate.
#[test]
fn notes_citations_in_code_resolve_to_a_real_section() {
    /// A section number the notes will never define, for planting. Kept as an integer
    /// so no literal citation of it appears anywhere in this file's text.
    const ABSENT: u32 = 99;

    let root = repo_root();
    let notes =
        std::fs::read_to_string(root.join(NOTES)).unwrap_or_else(|e| panic!("read {NOTES}: {e}"));
    let sections = notes_sections(&notes);

    // 0. The scoping premise, checked rather than asserted in prose: no other normative
    //    document defines a `3.N` subsection, so `§3.N` cannot mean anything but this
    //    file. AGENTS §2 names the current pair; both are read here.
    for doc in [
        "docs/43-design-claude-fable-v17.md",
        "docs/44-implementation-plan-claude-fable-v17.md",
    ] {
        let text =
            std::fs::read_to_string(root.join(doc)).unwrap_or_else(|e| panic!("read {doc}: {e}"));
        let clashes = notes_sections(&text);
        assert!(
            clashes.is_empty(),
            "{doc} now defines section(s) {clashes:?} in the `3.N` shape, so a bare \
             §3.N in code is ambiguous and this gate is resolving citations against \
             the wrong document. Either that heading is a mistake, or every citation \
             in the tree needs a document qualifier and this scan needs to read it."
        );
    }

    // 1. Fail-first on the matcher, in every spelling it claims to cover (AGENTS §3),
    //    and on the walker, through a nested directory. The absent section is formatted
    //    rather than written, so this file carries no literal of it.
    let scratch =
        std::env::temp_dir().join(format!("snx-meta-notes-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(scratch.join("deep/deeper")).expect("create scratch");
    let bogus = format!("\u{a7}3.{ABSENT}");
    let live = format!(
        "\u{a7}3.{}",
        sections.iter().next().expect("the notes have sections")
    );
    std::fs::write(
        scratch.join("deep/deeper/planted.rs"),
        format!(
            "//! Module doc citing {bogus}.\n\
             /// Doc comment citing {bogus}.\n\
             pub fn f() {{\n\
             \x20   // Line comment citing {bogus}.\n\
             \x20   panic!(\"an operator-visible message citing {bogus}\");\n\
             }}\n\
             /* block comment citing {bogus} */\n\
             // …and one that resolves, which must NOT be reported: {live}\n"
        ),
    )
    .expect("write planted file");
    // Three shapes that must stay silent: a non-Rust file (out of scope), a file under a
    // skipped build directory, and a file inside a nested checkout. A skip is a matcher
    // too, and gets planted in every spelling it claims to cover.
    std::fs::write(
        scratch.join("deep/notes.md"),
        format!("Prose citing {bogus}, which this gate does not scan.\n"),
    )
    .expect("write planted markdown");
    std::fs::create_dir_all(scratch.join("target/debug")).expect("create scratch target");
    std::fs::write(
        scratch.join("target/debug/generated.rs"),
        format!("// build output citing {bogus}\n"),
    )
    .expect("write planted build output");
    std::fs::create_dir_all(scratch.join("wt")).expect("create scratch worktree");
    std::fs::write(scratch.join("wt/.git"), "gitdir: /elsewhere\n").expect("write gitfile");
    std::fs::write(
        scratch.join("wt/copy.rs"),
        format!("// another checkout citing {bogus}\n"),
    )
    .expect("write planted worktree file");

    let mut planted: Vec<String> = Vec::new();
    walk_text(&scratch, &scratch, &mut |rel, text| {
        if !rel.ends_with(".rs") {
            return;
        }
        for (line, n) in notes_citations(text) {
            if !sections.contains(&n) {
                planted.push(format!("{rel}:{line}"));
            }
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        planted,
        vec![
            "deep/deeper/planted.rs:1".to_owned(),
            "deep/deeper/planted.rs:2".to_owned(),
            "deep/deeper/planted.rs:4".to_owned(),
            "deep/deeper/planted.rs:5".to_owned(),
            "deep/deeper/planted.rs:7".to_owned(),
        ],
        "the matcher or the walker missed a spelling of a dangling citation, reported \
         a resolvable one, or reached out of scope (a .md file, a build directory, or \
         a nested checkout)"
    );

    // 2. …and the shapes that are NOT notes citations must stay silent, or this gate
    //    becomes noise and gets deleted rather than fixed. Design and plan sections are
    //    spelled the same way with different numbers, and plan §3's rules are cited with
    //    a space where a notes section has a dot.
    for benign in [
        "design \u{a7}15.51 (P14)",
        "plan \u{a7}18 item 11",
        "AGENTS \u{a7}9's verification protocol",
        "plan \u{a7}3 rule 8, and \u{a7}3 rule 13",
        "\u{a7}7.2's baseline discipline",
        "\u{a7}16.13 names the artifact",
        "\u{a7}5's anti-tautology rule",
        "version 3.14 of something",
    ] {
        assert!(
            notes_citations(benign).is_empty(),
            "{benign:?} was read as an implementation-notes citation"
        );
    }

    // 3. The real scan.
    let mut rs_files = 0usize;
    let mut resolved = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    walk_text(&root, &root, &mut |rel, text| {
        if !rel.ends_with(".rs") {
            return;
        }
        rs_files += 1;
        for (line, n) in notes_citations(text) {
            if sections.contains(&n) {
                resolved += 1;
            } else {
                offenders.push(format!("{rel}:{line} -> \u{a7}3.{n}"));
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "citations of implementation-notes sections that do not exist: {offenders:#?}\n\
         {NOTES} defines 3.1..=3.{} today. A citation is followed, not skimmed: a wrong \
         one sends the next editor to a section that argues something else and reads as \
         confirmation. If the section is owed but unwritten, write it in the same commit \
         — that is the case this gate was added for.",
        sections.last().copied().unwrap_or(0)
    );

    // 4. Not vacuous, three ways: the notes were parsed, the tree was walked, and the
    //    scan is looking at content that really does cite (a matcher that silently
    //    stopped matching reports zero offenders just as loudly as a clean tree).
    assert!(
        sections.len() >= 50,
        "only {} sections parsed out of {NOTES} — the heading shape changed and every \
         citation below is being checked against almost nothing",
        sections.len()
    );
    assert!(
        rs_files >= 100,
        "only {rs_files} .rs files walked — the walker has stopped seeing the tree"
    );
    assert!(
        resolved >= 40,
        "only {resolved} resolvable notes citations found in {rs_files} .rs files — this \
         tree cites its notes constantly, so a number this low means the matcher stopped \
         matching rather than that the comments stopped citing"
    );
}
