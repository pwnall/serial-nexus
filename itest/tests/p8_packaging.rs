#![forbid(unsafe_code)]

//! **The packaging gate** (plan §18 item 31; plan §3 rules 10, 11, 17 and 22).
//!
//! `packaging/` ships three files an operator installs verbatim — a systemd unit, a
//! udev rules file, and a README that explains both — and until this file existed
//! **nothing in the tree read any of them**. A directive could be misspelled, a
//! section renamed, or a README paragraph left describing a knob the unit no longer
//! has, and every lane stayed green. That is plan §3 rule 22's tell in its plainest
//! form: the passing output and the not-running output were the same, because there
//! was no gate at all.
//!
//! Thirteen checks run here, in three classes.
//!
//! **Tool checks** — [`the_packaged_unit_verifies_clean_under_systemd_analyze`],
//! [`the_packaged_udev_rules_verify_clean_under_udevadm`] and
//! [`the_socket_group_recipe_verifies_clean_under_systemd_analyze`] — hand the
//! shipped files to systemd's own validators. They self-skip where those validators
//! do not exist (a Mac, a minimal container), through the one `required` mechanism,
//! under `SNX_PACKAGING`.
//!
//! **Text checks** — [`every_directive_the_readme_names_exists_in_the_packaged_unit`],
//! [`every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class`],
//! [`every_evidence_row_records_one_of_the_three_classes_and_nothing_else`],
//! [`the_root_probe_derives_its_sandbox_from_the_packaged_unit`] and
//! [`the_socket_group_recipe_agrees_with_itself`] — need no tool and never skip, so
//! the drift class this project can actually cause is covered on every platform on
//! every push. Each derives *both* sides from the shipped files, never from a list
//! kept here; `meta_derive.rs` is the shape.
//!
//! The third of those is the newest, and it is a **repair rather than an addition**
//! (plan §18 item 84): the evidence table's vocabulary used to be checked by
//! counting three substrings over the whole section, under a comment claiming every
//! row was held to them. It now reads the Class column row by row against a grammar
//! the README's own legend defines. The reasoning, and the line between a scope and
//! a hedge, are at that test and at [`restriction_names_a_part`].
//!
//! And **five root-gated measurements**, which are item 31's other half.
//! [`dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown`] is
//! the first: the two claims in `packaging/README.md`'s evidence table that read
//! *man-page, and owed a measurement* until CI's root arm measured them (2026-08-13,
//! item 68) and they became **measured**. The other four *execute the recipes* the
//! README's remaining `unverified` rows describe, which is the only thing that can
//! move those rows:
//!
//! * [`the_socket_group_recipe_hands_the_runtime_directory_to_the_operators_group`]
//!   runs the unit's own `groupadd`/`useradd` lines and then measures the premise and
//!   the recipe as a **pair** — the shipped unit with the operators' group in
//!   `SupplementaryGroups=` (which must *not* reach the runtime directory) against the
//!   static identity (which must produce the block's own predicted `stat` line). Either
//!   alone is consistent with the wrong story, which is the shape Claim 4 above already
//!   needed.
//! * [`the_packaged_sandbox_starts_the_daemon_and_it_serves`] starts the real daemon
//!   under the packaged `[Service]` directives and asks it for `state` over its own
//!   socket. Until it existed, "the daemon still starts under all of them" was a
//!   sentence no lane had ever tried.
//! * [`the_socket_group_recipe_widens_the_control_socket_to_the_operators_group`]
//!   completes that recipe end to end, including its `--socket-group`, against the
//!   *second* line the block predicts.
//! * [`the_upgrade_procedures_root_copy_carries_the_snapshot_across`] runs the
//!   README's upgrade procedure step by step: a start that seeds, a root `cp` into a
//!   `DynamicUser=` state directory, and a start that must come up on the copied
//!   snapshot.
//!
//! All five run under `SNX_PACKAGING_ROOT=required` in the `packaging` job's root
//! step and self-skip everywhere else, so the part of their machinery that is pure
//! derivation stays pulled out into the text checks above rather than riding with
//! them — a development box still fails the precondition, and a test one lane
//! executes is a test one lane debugs. The root step passes `--test-threads=1`; two
//! of these create the same system identity, and [`RECIPE_IDENTITY`] serialises them
//! for an operator who forgets it.
//!
//! # Using `systemd-analyze verify` correctly, which is not obvious
//!
//! Measured on Ubuntu, systemd 259 (259.5-0ubuntu3.4), kernel 7.0.0-29, 2026-08-12,
//! against the real `packaging/serial-nexus-daemon.service`:
//!
//! * The **unmodified** unit exits **1** on a box with no install, with
//!   `Command /usr/local/bin/serial-nexus-daemon is not executable: No such file or
//!   directory`. The exit status therefore conflates a unit defect with an
//!   environmental fact, and both naive uses are traps: trusting it reddens every
//!   box without an install, ignoring it asserts nothing.
//! * So the unit is **staged** — copied to a scratch tree with `ExecStart=`'s
//!   program replaced by a stub that exists and is executable — before it is
//!   verified. Staged, the real unit exits **0** with empty stderr. The staging is
//!   proven load-bearing by verifying the unstaged copy in the same test.
//! * With the ExecStart arm removed, **exit status alone catches far less than it
//!   looks like it does**. At *default* flags on systemd 259: a planted
//!   `NotADirective=yes` exits **0**; a planted `RestartSec=notanumber` exits **0**;
//!   a planted `[NotASection]` exits **0**. Only the refusal class (no `ExecStart=`
//!   at all) exits 1. Every one of those defects *is* printed on stderr, as
//!   `Unknown key …, ignoring.` / `Failed to parse …, ignoring: Invalid argument` /
//!   `Unknown section …. Ignoring.` — so **stderr, not the exit status, is the
//!   signal that survives**, and [`verdict`] requires both a zero status and an
//!   empty stderr.
//! * `--recursive-errors=no` (equivalently `=one`) *also* turns those three classes
//!   into exit 1 — the opposite of what the flag's name suggests. It is passed when
//!   the installed `systemd-analyze` advertises it, because a second independent
//!   signal is worth having; the stderr assertion is what makes the gate work on a
//!   systemd too old to have the flag (it landed in v250).
//!
//! # What this gate provably does **not** catch
//!
//! Stated here rather than left for a reader to assume, because a gate whose bounds
//! are unstated gets counted as coverage it does not have:
//!
//! * **A `SupplementaryGroups=` naming a group that does not exist.** Measured on
//!   the box above: `SupplementaryGroups=nosuchgroup12345` verifies with exit 0 and
//!   empty stderr. The gate reports what it sees each run rather than pinning the
//!   answer, so a systemd that grows the check is noticed instead of contradicted.
//! * **Whether any `DeviceAllow=` matches a device this machine has**, or whether the
//!   `dialout` group is the right one for the distro. Two of the unit's `DeviceAllow=`
//!   lines *are* exercised — section (9)'s seed configuration is a pty node, which
//!   needs `/dev/ptmx` and `char-pts` — but nothing here opens a serial node, and
//!   nothing here reads the group that owns one.
//! * **"Nothing here starts the service" — which stood in the entry above until
//!   2026-08-21 and is now false.** Section (9)'s probes *do* start the packaged
//!   daemon, ask it for `state` over its own control socket and shut it down. What
//!   bounds them is *where*: only on a box that passes
//!   [`packaging_root_precondition`] — PID 1 is systemd, effective uid 0,
//!   `systemd-run` on `PATH` — which is CI's `packaging` job and no development box.
//!   Everywhere else they skip, naming the precondition, and
//!   `SNX_PACKAGING_ROOT=required` turns that skip red on a lane claiming the
//!   capability. The comment is what a reviewer reads, so it is corrected in place
//!   rather than left to be read as a bound that no longer holds.
//! * **Semantic correctness of a well-formed value.** `ProtectSystem=full` instead
//!   of `strict` is a valid unit and a weaker sandbox; only the evidence-table check
//!   would notice the *directive* vanishing, not its value changing.
//! * **The README's prose.** The text checks compare directive *names*. A paragraph
//!   that describes `ProtectSystem=strict` as doing the opposite of what it does is
//!   a review problem, not a gate problem.
//! * **Whether an evidence class is the *right* one.** The vocabulary check decides
//!   that a row's Class column speaks the language the page says it speaks; it
//!   cannot decide that a row reading `measured` names anything that ran. Only a
//!   reader comparing the Evidence column against the tree can, and this file's
//!   directive correspondence is the part of that which can be derived.
//! * **An *installed* unit.** The root probes hand the packaged `[Service]`
//!   directives to `systemd-run` as transient properties; nothing here copies the
//!   unit into `/etc/systemd/system`, runs `systemctl daemon-reload`, or starts it by
//!   name. So `[Install]`, `Type=exec`, `Restart=`/`RestartSec=` and the
//!   `/usr/local/bin/` path in `ExecStart=` are outside every measurement below —
//!   deliberately, because a probe that installed a unit would overwrite a real
//!   deployment's, and the claims at stake are about the sandbox rather than about
//!   the path a binary is exec'd from. The four dropped directives are named at
//!   [`service_properties`] and asserted absent by
//!   [`the_root_probe_derives_its_sandbox_from_the_packaged_unit`].
//! * **A hedge given the shape of a scope.** `measured (partially)` is refused, and
//!   so is every other bare adverb in parentheses, because the grammar admits no such
//!   shape (see [`restriction_names_a_part`]). What it cannot refuse is a hedge
//!   wearing a determiner: `measured for the most part` opens with `the` and passes.
//!   The bound is stated rather than papered over — what is checked here is *shape*,
//!   and shape is a proxy for intent even when it is the best available one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The packaged unit, relative to the repository root.
const UNIT: &str = "packaging/serial-nexus-daemon.service";

/// The packaged udev rules, relative to the repository root.
const RULES: &str = "packaging/99-serial-nexus.rules";

/// The packaging README, relative to the repository root.
const README: &str = "packaging/README.md";

/// The heading that opens the README's evidence-class record.
///
/// Named here rather than discovered, so a rename of the section fails loudly at
/// [`evidence_section`] instead of silently reducing this gate to comparing an empty
/// set against an empty set.
const EVIDENCE_HEADING: &str = "## Evidence classes";

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/itest — the *directory*, which §15.40 kept short.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the itest crate has a parent directory")
        .to_path_buf()
}

/// Read a shipped packaging file, naming it if it is gone.
fn read_tree_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} — this gate cannot check a file it cannot read",
            path.display()
        )
    })
}

// ---------------------------------------------------------------------------
// The skip mechanism
// ---------------------------------------------------------------------------

/// Announce the packaging tool checks' self-skip — and refuse to skip when the
/// operator has said they must run.
///
/// **The systemd-validator class** (plan §3 rule 11). `systemd-analyze` and
/// `udevadm` do not exist on a Mac and need not exist in a minimal container, so
/// both tool checks here self-skip; without a `required` spelling a runner image
/// that stopped shipping systemd would report the whole packaging surface green
/// without validating a byte of it, and libtest captures a passing test's stderr, so
/// even the SKIP line is invisible without `--nocapture`.
///
/// **Why the variable names the capability and not the tool.** Every instance beside
/// it is named for the coverage at stake rather than the missing binary — `SNX_TLS`
/// gates a tier whose absent tool is `curl`, `SNX_WEB_UI` a suite whose absent tool
/// is `node`, `SNX_EXEC_CODEC` a battery whose absent tool is `python3`. The
/// coverage at stake here is the packaged deployment surface; `systemd-analyze` is
/// merely how this platform validates it, and a `SNX_SYSTEMD_ANALYZE` would name
/// nothing the day a second validator joined. `reason` carries what the provider
/// actually saw, which is where rule 11 puts the tool's name: in the message, on the
/// box printing it.
///
/// **This helper belongs in `itest/src/lib.rs`** beside [`skip_no_tls`-shaped]
/// siblings, and is defined here only because the session that wrote it did not own
/// that file. Moving it is filed as follow-up work; the shape, word and failure text
/// are already the shared ones, so the move is a cut and paste.
///
/// [`skip_no_tls`-shaped]: serial_nexus_itest
fn skip_no_packaging(test: &str, reason: &str) {
    assert!(
        std::env::var("SNX_PACKAGING").as_deref() != Ok("required"),
        "SNX_PACKAGING=required, but {test} skipped: {reason}.\n\
         Required mode exists so a box that has systemd's own validators cannot \
         report a green run for the packaging files nothing else in this tree \
         reads (plan §18 item 31, plan §3 rule 11). Install systemd, or unset \
         SNX_PACKAGING."
    );
    eprintln!("SKIP {test}: {reason}");
}

/// Announce the root-gated packaging measurement's self-skip.
///
/// **The root-box class** (plan §3 rule 11, plan §18 item 31). Separate from
/// [`skip_no_packaging`] on purpose: its precondition is not a package but a
/// privilege. Folding the two classes into one variable would mean an operator who
/// set `SNX_PACKAGING=required` for the cheap checks got a hard failure for a
/// privilege they never claimed to have.
///
/// **Demanded by CI's root step, and the demand followed the measurement.** The
/// `packaging` job's own "What this runner actually is" step reads `PID 1: systemd`
/// and `sudo: passwordless` on `ubuntu-latest`, and the root arm has since run the
/// measurement green — 6 passed, 0 failed, the probe under `DynamicUser=yes` at a
/// transient uid with `state_real=/var/lib/private/…` (item 68, CI run 31695823765,
/// 2026-08-13; reproduced at 31877969760, 2026-08-15). That order is the discipline
/// §15.52 set for `SNX_RIG_FLOW`: measure the precondition, then demand it, because
/// shipping `required` on an assumption reddens a lane for someone else's runner
/// image. **Until it was set, this variable was set by no lane at all**, which left
/// the root step's passing output identical to its self-skipping output — AGENTS
/// §3's tell, in the very step that had just been un-escaped from
/// `continue-on-error` in order to gate.
///
/// **A development box still fails the precondition — and the cause recorded here
/// was wrong.** *Refuted 2026-08-15* (AGENTS §9: a refuted diagnosis is recorded,
/// not quietly replaced). This comment used to say the rootless fallback was
/// "refused by the kernel's `uid_map` policy". Measured on this box, Ubuntu 26.04 /
/// kernel 7.0.0-29: `unshare -U true` **succeeds**, exit 0, and
/// `kernel.unprivileged_userns_clone` reads `1` — the kernel grants the namespace.
/// What the namespace does not carry is capability. `/proc/self/attr/current` reads
/// `unconfined` outside it and `unprivileged_userns (enforce)` inside, and `CapEff`
/// inside is `0000000000000000` where a fresh user namespace would otherwise hold a
/// full set. That is AppArmor — `kernel.apparmor_restrict_unprivileged_userns=1`,
/// with `/sys/kernel/security/apparmor` mounted — and it is why the *next* step,
/// the `/proc/self/uid_map` write that `unshare -Ur` performs, is the one that
/// returns `EPERM`. The observed stderr quoted elsewhere in this file is accurate
/// and stays as it is; only the cause attributed to it is corrected. `systemctl
/// start` remains closed for its own separate reason: polkit refuses without a
/// terminal to prompt on.
fn skip_no_packaging_root(test: &str, reason: &str) {
    assert!(
        std::env::var("SNX_PACKAGING_ROOT").as_deref() != Ok("required"),
        "SNX_PACKAGING_ROOT=required, but {test} skipped: {reason}.\n\
         Required mode exists so a box that can start a transient systemd service \
         cannot report a green run for the packaging claims only a root probe can \
         measure (plan §18 item 31, plan §3 rule 11) — CI's root step demands it \
         precisely so a runner image that stopped providing systemd-as-PID-1 or \
         passwordless root is noticed instead of skipped past. Run as root on a \
         systemd box, or unset SNX_PACKAGING_ROOT."
    );
    eprintln!("SKIP {test}: {reason}");
}

// ---------------------------------------------------------------------------
// A scratch tree
// ---------------------------------------------------------------------------

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "snx-p8-packaging-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch tree");
        Scratch(dir)
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create scratch subdirectory");
        }
        std::fs::write(&path, text).expect("write scratch file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// Running a validator
// ---------------------------------------------------------------------------

/// One validator run, reduced to the two things a verdict may read.
struct Run {
    /// `None` when the child was killed by a signal.
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn ok(&self) -> bool {
        self.status == Some(0)
    }
}

/// Run `program` with `args`, or `None` if it is not on `PATH`.
///
/// Distinguishing "absent" from "failed" is the whole point: an absent validator is
/// a self-skip naming what the provider saw, a failing one is a red gate.
fn run_tool(program: &str, args: &[String]) -> Option<Run> {
    match Command::new(program).args(args).output() {
        Ok(out) => Some(Run {
            status: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("spawning {program} failed for a reason other than absence: {e}"),
    }
}

/// The gate's judgement over one validator run.
///
/// **Both halves are load-bearing and neither is redundant.** The exit status alone
/// misses unknown keys, unknown sections and unparseable values on systemd 259 at
/// default flags (module doc); an empty-stderr check alone would miss nothing
/// measured, but would silently accept a validator that reported by status only.
/// Requiring both is what makes this gate's passing output — nothing — mean
/// something.
///
/// stderr is required *empty*, not merely free of known markers. A validator line
/// this tree has never seen is a finding to record, not a line to filter: the
/// message below prints it verbatim so the record can be made.
fn verdict(run: &Run) -> Result<(), String> {
    if !run.ok() {
        return Err(format!(
            "exit status {:?}, stderr:\n{}",
            run.status,
            run.stderr.trim_end()
        ));
    }
    if !run.stderr.trim().is_empty() {
        return Err(format!(
            "exit status 0 but stderr is not empty — the validator warned rather \
             than refused, which is exactly the class an exit-status-only gate \
             misses:\n{}",
            run.stderr.trim_end()
        ));
    }
    Ok(())
}

/// The flags this box's `systemd-analyze verify` understands, of the two we want.
///
/// Probed from `--help` rather than assumed, because `--recursive-errors=` landed in
/// systemd v250 and this gate must work on an older one — where the stderr half of
/// [`verdict`] is the whole gate.
fn verify_flags(help: &str) -> Vec<String> {
    let mut flags = Vec::new();
    if help.contains("--man") {
        // Keep the run hermetic: no man-page lookups for a unit whose
        // Documentation= is a URL.
        flags.push("--man=no".to_owned());
    }
    if help.contains("--recursive-errors") {
        flags.push("--recursive-errors=no".to_owned());
    }
    flags
}

// ---------------------------------------------------------------------------
// Parsing the two shipped files
// ---------------------------------------------------------------------------

/// Every directive `unit` *sets*: a `Name=` at the start of a line.
///
/// `ExecStart=`'s continuation lines begin with whitespace and are correctly not
/// directives.
fn active_directives(unit: &str) -> BTreeSet<String> {
    unit.lines().filter_map(|l| directive_at(l, 0)).collect()
}

/// Every directive `unit` *mentions*, active or inside a comment.
///
/// A comment mention is how the unit carries its alternatives — the static-identity
/// recipe spells `User=`, `Group=` and `DynamicUser=no`, and the extra-log-directory
/// example spells `ReadWritePaths=` — so a README that names one of those is naming
/// something the unit really does discuss. The two kinds are reported separately at
/// the call site; conflating them would let a README document a directive the unit
/// merely mentions in passing as though it were set.
fn mentioned_directives(unit: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in unit.lines() {
        let trimmed = line.trim_start();
        let body = match trimmed.strip_prefix('#') {
            Some(rest) => rest.trim_start(),
            None => trimmed,
        };
        if let Some(name) = directive_at(body, 0) {
            out.insert(name);
        }
    }
    out
}

/// `line[at..]` read as `Name=`, with a value following the `=`.
///
/// The trailing-value requirement is what separates a directive from prose about
/// one: the unit's own comments say "`DynamicUser= puts the real state directory
/// under /var/lib/private/`", and a matcher that took that for a setting would put
/// prose into a roster.
fn directive_at(line: &str, at: usize) -> Option<String> {
    let s = line.get(at..)?;
    let eq = s.find('=')?;
    let name = &s[..eq];
    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let value = s.get(eq + 1..)?;
    if value.starts_with(char::is_whitespace) || value.is_empty() {
        return None;
    }
    Some(name.to_owned())
}

/// Every systemd directive `md` names in backticks, as `` `Name=` `` or
/// `` `Name=value` ``.
///
/// Backticks rather than bare words for two reasons that are both about not
/// manufacturing a roster: the README's prose says "under `DynamicUser`" without the
/// `=` in places, and a bare-word scan would also lift `User` out of "the service's
/// user". A directive an author *marked up as code* is a directive the author is
/// making a claim about.
fn backticked_directives(md: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = md;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let token = &after[..close];
        rest = &after[close + 1..];
        // `Name=` and `Name=value` both count; the value is not read.
        let Some(eq) = token.find('=') else { continue };
        let name = &token[..eq];
        if name.is_empty()
            || !name.starts_with(|c: char| c.is_ascii_uppercase())
            || !name.chars().all(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        out.insert(name.to_owned());
    }
    out
}

/// The README's evidence-class section: from [`EVIDENCE_HEADING`] to the next `## `.
///
/// Scoped so a directive mentioned *elsewhere* on the page cannot satisfy the
/// evidence-table check. Without the scoping the gate would pass on a README that
/// discusses a directive in prose and never records what kind of claim it is, which
/// is the exact state item 31 was filed against.
fn evidence_section(md: &str) -> &str {
    let at = md.find(EVIDENCE_HEADING).unwrap_or_else(|| {
        panic!(
            "packaging/README.md has no `{EVIDENCE_HEADING}` section — the evidence \
             record this gate derives from is gone, and a gate that reads nothing \
             agrees with everything (plan §18 item 31)"
        )
    });
    let rest = &md[at + EVIDENCE_HEADING.len()..];
    match rest.find("\n## ") {
        Some(end) => &rest[..end],
        None => rest,
    }
}

// ---------------------------------------------------------------------------
// Reading the evidence table as rows rather than as a haystack
// ---------------------------------------------------------------------------
//
// **Why this section exists** (plan §18 item 84). The evidence table's vocabulary
// check used to be four lines: `section.matches(class).count() > 0` for each of
// `measured`, `man-page` and `unverified`, above a comment claiming "the table's own
// vocabulary is closed: three class words and no others". Those are not the same
// statement, and the gap is the whole of AGENTS §3's fifth register — *a gate whose
// assertion is strictly weaker than the comment above it claims*. Counting a
// substring over the whole section says only that each word appears **somewhere**;
// it says nothing at all about any row. A row whose Class column read `assumed`, or
// `probably`, or nothing whatsoever, passed untouched, and so did `measured
// (partially)` for as long as it stood — because it *contains* `measured`, and the
// prose in the Evidence column two cells over contains the other two words several
// times each. Three unrelated rows keep those counters positive forever.
//
// The repair is to read the column, which means reading the table, which means the
// row parser below. Everything here derives from `packaging/README.md`'s own bytes;
// nothing is a list kept in this file except [`CLASSES`], and that constant exists
// to be held *against* the page rather than to stand in for it.

/// The three evidence classes, spelled once.
///
/// **Not the authority — the tripwire.** The README's own legend table defines the
/// vocabulary and [`legend_classes`] derives it from the page; the vocabulary gate
/// then asserts that the derivation and this constant agree. Either half alone is
/// the tell AGENTS §3 names: a constant alone lets the page grow a fourth class in
/// silence, and a derivation alone lets the page *define* a fourth class and call
/// the gate green for reading it correctly. Holding one against the other is what
/// makes either mean anything.
const CLASSES: [&str; 3] = ["man-page", "measured", "unverified"];

/// One row of a GitHub-flavoured markdown table.
struct MdRow {
    /// The row's cells, trimmed, with the empty leading and trailing fields dropped.
    cells: Vec<String>,
    /// 0-based index into the *section's* lines, so a defect can name the row it is
    /// in and a plant can rewrite exactly one cell of exactly one row.
    line: usize,
}

/// One markdown table: its header, and its data rows.
struct MdTable {
    header: Vec<String>,
    /// 0-based index of the header line, for the header-rename plant.
    header_line: usize,
    rows: Vec<MdRow>,
}

/// A markdown table row split at its top-level `|`, or `None` if `line` is not one.
///
/// **Code-span aware, and that is not hypothetical here.** The evidence table's
/// cells carry code spans like `` `tty[A-Z]*[0-9]` `` and `` `state_real=/var/lib/…` ``;
/// a `|` inside one is content, and a splitter that took it for a column boundary
/// would shift every cell to its right — which for this gate means reading the
/// *Evidence* prose as the Class cell, where all three class words appear and
/// nothing ever reddens. Backslash escapes are passed through with their backslash
/// so `\|` cannot open a phantom column either.
///
/// Rows are written `| a | b |`, so the empty leading and trailing fields are
/// dropped — but only one of each, so a genuinely empty last cell survives to be
/// judged. That matters: "nothing at all" is one of the spellings item 84 names.
fn split_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_code = false;
    let mut escaped = false;
    for c in trimmed.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => {
                escaped = true;
                cur.push(c);
            }
            '`' => {
                in_code = !in_code;
                cur.push(c);
            }
            '|' if !in_code => {
                cells.push(cur.trim().to_owned());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    cells.push(cur.trim().to_owned());
    if cells.first().is_some_and(String::is_empty) {
        cells.remove(0);
    }
    if cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    Some(cells)
}

/// `| a | b |` rebuilt from its cells — the inverse of [`split_row`] for plants.
fn join_row(cells: &[String]) -> String {
    let mut out = String::from("|");
    for cell in cells {
        out.push(' ');
        out.push_str(cell);
        out.push_str(" |");
    }
    out
}

/// Whether `cells` is a table's `|---|---|` separator rather than data.
fn is_delimiter_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let bar = c.trim().trim_start_matches(':').trim_end_matches(':');
            !bar.is_empty() && bar.chars().all(|c| c == '-')
        })
}

/// Every markdown table in `section`, header first.
///
/// A table is a run of consecutive `|`-opening lines whose *second* line is a
/// separator. Requiring the separator is what keeps a stray prose line beginning
/// with `|` from being read as a one-row table with a header and no data — which
/// would be a table this gate then reported as carrying no Class column.
fn md_tables(section: &str) -> Vec<MdTable> {
    let lines: Vec<&str> = section.lines().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(header) = split_row(lines[i]) else {
            i += 1;
            continue;
        };
        let Some(delim) = lines.get(i + 1).and_then(|l| split_row(l)) else {
            i += 1;
            continue;
        };
        if !is_delimiter_row(&delim) {
            i += 1;
            continue;
        }
        let header_line = i;
        let mut rows = Vec::new();
        let mut j = i + 2;
        while let Some(cells) = lines.get(j).and_then(|l| split_row(l)) {
            rows.push(MdRow { cells, line: j });
            j += 1;
        }
        out.push(MdTable {
            header,
            header_line,
            rows,
        });
        i = j.max(i + 1);
    }
    out
}

/// `s` with markdown bold/italic asterisks removed outside code spans.
///
/// The table bolds a class word wherever the session that wrote the row wanted the
/// reader's eye — `**measured**`, `**unverified**`, `**measured for acceptance**` —
/// and a gate that read the asterisks as part of the word would report every bolded
/// row as a fourth class. Only `*` is stripped, and only outside code spans: `_` is
/// left alone because it rides inside identifiers this page writes without backticks
/// far more often than it marks emphasis, and `` `*Directory=` `` is content.
fn strip_emphasis(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for c in s.chars() {
        match c {
            '`' => {
                in_code = !in_code;
                out.push(c);
            }
            '*' if !in_code => {}
            _ => out.push(c),
        }
    }
    out
}

/// The index of the column `header` calls `name`, matched after emphasis-stripping
/// and case-folding, and by prefix so `Directive(s)` answers to `Directive`.
fn column_index(header: &[String], name: &str) -> Option<usize> {
    let want = name.to_ascii_lowercase();
    header.iter().position(|h| {
        strip_emphasis(h)
            .trim()
            .to_ascii_lowercase()
            .starts_with(&want)
    })
}

/// A Class cell split into its class terms at the top-level `;` and `,`.
///
/// Depth- and code-span-aware, because two real rows carry a whole evidence citation
/// inside their parentheses — `(CI root arm, run 31695823765, 2026-08-13; …)` — and
/// a flat split would tear those commas apart into segments beginning `run` and
/// `2026-08-13`, reddening the gate on the two best-evidenced rows in the table.
fn class_segments(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut in_code = false;
    for c in cell.chars() {
        match c {
            '`' => {
                in_code = !in_code;
                cur.push(c);
            }
            '(' if !in_code => {
                depth += 1;
                cur.push(c);
            }
            ')' if !in_code => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ';' | ',' if !in_code && depth == 0 => {
                out.push(cur.trim().to_owned());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_owned());
    out.retain(|s| !s.is_empty());
    out
}

/// Whether a class term's restriction **names the part of the claim it applies to**,
/// which is the entire difference between a scope and a hedge.
///
/// `measured (the mode)` says which half of that row's claim was measured, and a
/// reader can go and weigh the other half. `measured (partially)` states a fraction
/// and names no part: "partially measured" is a **fourth class wearing the third
/// one's word**, and it is one of the two spellings plan §18 item 84 names, because
/// the old substring gate saw the `measured` inside it and passed.
///
/// Three ways to name a part, and a restriction must take one:
///
/// * **a determiner opens it** — `the mode`, `the loopback default`, `the faulted
///   arm`, `the effect`, `the premise`, `the recipe`;
/// * **it cites** — any code span or ASCII digit, which is how the two root-arm rows
///   carry `run 31695823765, 2026-08-13` and `` `EACCES` ``-versus-`` `EROFS` ``
///   in the column itself;
/// * **every substantial word in it is a word the row itself uses elsewhere** —
///   `measured for acceptance` stands because that row's Evidence cell opens
///   "Acceptance — that systemd parses every one of these …". The row's *own Class
///   cell is excluded* from that vocabulary by the caller, without which the rule
///   would be satisfied by the restriction quoting itself and would assert nothing.
///
/// **This is an allowlist of shapes, not a blocklist of hedges, and the direction is
/// the point.** A blocklist passes every hedge nobody thought of; this rejects
/// `(mostly)`, `(roughly)`, `(broadly)` and `(probably)` without having been told
/// about any of them, because no bare adverb takes any of the three shapes. What it
/// costs is that a legitimate new restriction must be written in one of them — and
/// the failure message says which. A loud false red is the cheap direction of that
/// trade; the expensive direction is the one item 84 records.
///
/// **Stated bound, and it is a real one.** The determiner test is a test of *shape*,
/// and a hedge can be given that shape: `measured for the most part` and `measured
/// (the greater part)` both open with `the` and both pass, where `measured
/// (partially)`, `measured (mostly)` and `measured (roughly)` do not. That residue is
/// the price of an allowlist that refuses everything it was never told about, and the
/// alternative buys a worse one — a list of hedge words passes every hedge nobody
/// thought of, which is the failure direction item 84 records. The four-letter floor
/// on "substantial word" is there so the function words inside a restriction (`of`,
/// `and`, `its`) are not each required to appear elsewhere in the row; a side effect
/// is that `measured for now` is refused for the accidental reason that `now` is
/// three letters, rather than because anything here understood it.
fn restriction_names_a_part(restriction: &str, row_words: &BTreeSet<String>) -> Result<(), String> {
    const DETERMINERS: [&str; 8] = ["the", "a", "an", "its", "this", "these", "each", "both"];

    let inner = if let Some(rest) = restriction.strip_prefix("for ") {
        rest.trim()
    } else if let Some(rest) = restriction
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
    {
        rest.trim()
    } else {
        return Err(format!(
            "`{restriction}` is not a restriction this table's grammar admits. A \
             class word may be followed by `for <the part>` or by `(<the part>)` and \
             by nothing else"
        ));
    };
    if inner.is_empty() {
        return Err(format!(
            "`{restriction}` is an empty restriction — it narrows the class without \
             saying to what"
        ));
    }
    let first = inner
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if DETERMINERS.contains(&first.as_str()) {
        return Ok(());
    }
    if inner.contains('`') || inner.chars().any(|c| c.is_ascii_digit()) {
        return Ok(());
    }
    let words: Vec<String> = inner
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() >= 4)
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() {
        return Err(format!(
            "`{restriction}` names nothing a reader can weigh: no determiner opens \
             it, it cites nothing, and it carries no word long enough to be a subject"
        ));
    }
    let strangers: Vec<&String> = words.iter().filter(|w| !row_words.contains(*w)).collect();
    if strangers.is_empty() {
        return Ok(());
    }
    Err(format!(
        "`{restriction}` grades the class instead of naming a part of the claim: no \
         determiner opens it, it cites nothing, and {strangers:?} appear nowhere \
         else in this row. `(partially)`, `(mostly)` and their kin are a fourth \
         class wearing a real one's word — plan §18 item 84 is the record of a gate \
         that could not see one"
    ))
}

/// One Class cell, parsed into the class words it claims — or the reason it is not a
/// class cell at all.
///
/// `row_words` is every alphabetic word of the row's *other* cells, lower-cased; the
/// caller excludes the Class cell itself, and [`restriction_names_a_part`] explains
/// why that exclusion is load-bearing rather than tidy.
fn class_terms(cell: &str, row_words: &BTreeSet<String>) -> Result<Vec<String>, String> {
    let segments = class_segments(cell.trim());
    if segments.is_empty() {
        return Err(
            "its Class cell is empty. A row with no class is a claim nobody can \
             weigh, and it is the spelling the old substring gate was blindest to: \
             an empty cell moves no counter in either direction"
                .to_owned(),
        );
    }
    let mut heads = Vec::new();
    for segment in segments {
        let stripped = strip_emphasis(&segment);
        let seg = stripped.trim();
        let head_len = seg
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || *c == '-')
            .count();
        let head = &seg[..head_len];
        if !CLASSES.contains(&head) {
            let opener = seg.split_whitespace().next().unwrap_or_default();
            return Err(format!(
                "its Class cell `{segment}` opens with `{opener}`, which is not one \
                 of the three class words {CLASSES:?}. The vocabulary is closed — the \
                 README's own legend says so — and a fourth word is a claim with no \
                 agreed weight"
            ));
        }
        let restriction = seg[head_len..].trim();
        if !restriction.is_empty() {
            restriction_names_a_part(restriction, row_words)
                .map_err(|why| format!("its Class cell `{segment}` is qualified: {why}"))?;
        }
        heads.push(head.to_owned());
    }
    Ok(heads)
}

/// What the evidence section's tables say, read row by row.
struct VocabularyReading {
    /// One line per defect, each naming the row it is in.
    defects: Vec<String>,
    /// How many rows claimed each class word.
    heads: BTreeMap<String, usize>,
    tables: usize,
    rows: usize,
}

/// Judge every Class cell in `section`, row by row.
///
/// Three defect classes, and the second and third are why this reads tables rather
/// than text: a table with no Class column at all (a renamed header silently removes
/// its rows from every check below), and a row whose cell count does not match its
/// header's (a missing `|` shifts every cell right, so the Evidence prose — which
/// contains all three class words — arrives where the Class cell is read).
fn read_evidence_vocabulary(section: &str) -> VocabularyReading {
    let mut defects = Vec::new();
    let mut heads: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = 0usize;
    let tables = md_tables(section);
    for table in &tables {
        let Some(class_at) = column_index(&table.header, "Class") else {
            defects.push(format!(
                "the table headed {:?} has no Class column, so none of its rows is \
                 judged at all — a renamed header is how a gate loses a whole table \
                 without its output changing",
                table.header
            ));
            continue;
        };
        for row in &table.rows {
            rows += 1;
            let subject = row.cells.first().map_or("<no cells>", String::as_str);
            if row.cells.len() != table.header.len() {
                defects.push(format!(
                    "the row `{subject}` has {} cell(s) against the header's {} — \
                     every cell to the right of the missing `|` is read as the wrong \
                     column, and the Evidence prose names all three classes",
                    row.cells.len(),
                    table.header.len()
                ));
                continue;
            }
            let row_words: BTreeSet<String> = row
                .cells
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != class_at)
                .flat_map(|(_, c)| {
                    c.split(|c: char| !c.is_ascii_alphabetic())
                        .filter(|w| !w.is_empty())
                        .map(str::to_ascii_lowercase)
                        .collect::<Vec<_>>()
                })
                .collect();
            match class_terms(&row.cells[class_at], &row_words) {
                Ok(terms) => {
                    for term in terms {
                        *heads.entry(term).or_default() += 1;
                    }
                }
                Err(why) => defects.push(format!("the row `{subject}` — {why}")),
            }
        }
    }
    VocabularyReading {
        defects,
        heads,
        tables: tables.len(),
        rows,
    }
}

/// The class words the README's own legend table defines.
///
/// The legend is the table headed `| Class | Means |`, and its Class column *is* the
/// vocabulary. Deriving it here and asserting it against [`CLASSES`] is the same
/// both-sides-from-the-file discipline the directive checks use, one column over.
fn legend_classes(section: &str) -> BTreeSet<String> {
    for table in md_tables(section) {
        let header: Vec<String> = table
            .header
            .iter()
            .map(|h| strip_emphasis(h).trim().to_ascii_lowercase())
            .collect();
        if header == ["class", "means"] {
            return table
                .rows
                .iter()
                .filter_map(|r| r.cells.first())
                .map(|c| strip_emphasis(c).trim().to_owned())
                .collect();
        }
    }
    panic!(
        "packaging/README.md's `{EVIDENCE_HEADING}` section has no `| Class | Means |` \
         legend table. The legend is where the vocabulary is defined; without it this \
         gate would be holding its own constant against nothing"
    )
}

/// `section` with the Class cell of the row on `line` replaced by `new`.
///
/// The plants below rewrite exactly one cell of exactly one row, so a reddening is
/// attributable to the spelling planted and to nothing else.
fn plant_class_cell(section: &str, line: usize, class_at: usize, new: &str) -> String {
    section
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i != line {
                return l.to_owned();
            }
            let mut cells = split_row(l).expect("the planted line is a table row");
            cells[class_at] = new.to_owned();
            join_row(&cells)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `section` with the row on `line` shortened by one cell.
///
/// A row that has lost a `|` is the shape defect the row check exists for, and this
/// is how that check is proven to bite.
fn plant_ragged_row(section: &str, line: usize) -> String {
    section
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i != line {
                return l.to_owned();
            }
            let mut cells = split_row(l).expect("the planted line is a table row");
            cells.pop();
            join_row(&cells)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every directive the evidence table records **in a Directive column**, and every
/// one the rest of the section merely names.
///
/// **The two are not interchangeable, which is what plan §18 item 84's sweep for
/// siblings turned up.** Until 2026-08-21 the "every shipped directive has a
/// recorded evidence class" half of
/// [`every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class`] read a
/// single section-wide backtick scan, so a directive named *anywhere* in the
/// section satisfied it — including in another row's Evidence prose. The evidence
/// cell for the `PrivateDevices=` row quotes `` `PrivateDevices=` `` back while
/// citing `systemd.exec(5)`; delete the Directive-column mention and the old roster
/// still held the name, so the row could vanish out of the table with the gate
/// green. That is the same substring-standing-in-for-a-row shape item 84 names, one
/// assertion over.
///
/// The phantom half genuinely *is* section-wide and stays that way: the table
/// legitimately records `User=`, `Group=` and `ReadWritePaths=` in its prose and
/// evidence cells, and those must still be directives the unit at least mentions.
fn recorded_directives(section: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut in_column = BTreeSet::new();
    let mut blanked: Vec<String> = section.lines().map(str::to_owned).collect();
    for table in md_tables(section) {
        let Some(dir_at) = column_index(&table.header, "Directive") else {
            continue;
        };
        for row in &table.rows {
            let Some(cell) = row.cells.get(dir_at) else {
                continue;
            };
            in_column.extend(backticked_directives(cell));
            let mut cells = row.cells.clone();
            cells[dir_at] = "—".to_owned();
            blanked[row.line] = join_row(&cells);
        }
    }
    let outside = backticked_directives(&blanked.join("\n"));
    (in_column, outside)
}

/// `section` with every backticked mention of `name=` neutralised **in Directive
/// columns only**, and how many were neutralised.
///
/// The mutation [`recorded_directives`]'s doc comment describes: the row stops
/// recording the directive while every other cell keeps naming it, which is exactly
/// the state the section-wide roster could not see.
fn strip_directive_from_directive_column(section: &str, name: &str) -> (String, usize) {
    let mut lines: Vec<String> = section.lines().map(str::to_owned).collect();
    let mut removed = 0usize;
    for table in md_tables(section) {
        let Some(dir_at) = column_index(&table.header, "Directive") else {
            continue;
        };
        for row in &table.rows {
            let Some(cell) = row.cells.get(dir_at) else {
                continue;
            };
            let (stripped, n) = strip_directive(cell, name);
            if n > 0 {
                let mut cells = row.cells.clone();
                cells[dir_at] = stripped;
                lines[row.line] = join_row(&cells);
                removed += n;
            }
        }
    }
    (lines.join("\n"), removed)
}

/// `md` with **every** backticked mention of `name=` neutralised, and how many were
/// neutralised.
///
/// One `.replace("`After=`", …)` is not enough and the difference matters: the
/// evidence table records `After=` in its Directive column *and* `After=network.target`
/// in its Evidence column, so a stripper that only knew the bare spelling deleted one
/// of two and the planted-deletion proof then ran against a table that still named the
/// victim — a mutation that matched nothing, reading exactly like "the gate did not
/// redden" (plan §3 rule 17(b)). Measured: the first version of this gate failed
/// exactly that way on `After`.
fn strip_directive(md: &str, name: &str) -> (String, usize) {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    let mut removed = 0usize;
    while let Some(open) = rest.find('`') {
        out.push_str(&rest[..=open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            out.push_str(after);
            return (out, removed);
        };
        let token = &after[..close];
        match token.split_once('=') {
            Some((base, _)) if base == name => {
                out.push_str("Neutralised=x");
                removed += 1;
            }
            _ => out.push_str(token),
        }
        out.push('`');
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    (out, removed)
}

/// The one drift-report shape both text gates print.
///
/// Both directions, named item by item: a gate that fails with "these two differ"
/// makes the reader do the diff by hand, which is how a red gate becomes a deleted
/// gate.
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

/// What `left` has that `right` does not, one direction only.
///
/// Used where the containment is deliberately asymmetric and a symmetric report
/// would print a difference that is not a defect.
fn drift_one_way(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right)
        .map(|item| format!("`{item}=`"))
        .collect()
}

// ---------------------------------------------------------------------------
// (1) The unit, through systemd's own validator
// ---------------------------------------------------------------------------

/// Stage `unit` into `scratch` with `ExecStart=`'s program replaced by an executable
/// stub, and return the staged path beside the number of substitutions made.
///
/// The count is returned so callers can assert the staging *applied* (plan §3 rule
/// 17(b)): a substitution that matched nothing leaves the unstaged text in place,
/// the environmental arm fires, and a red gate looks exactly like a real unit defect.
fn stage_unit(scratch: &Scratch, unit: &str, name: &str) -> (PathBuf, usize) {
    let stub = scratch.write("bin/stub-daemon", "#!/bin/sh\nexit 0\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
            .expect("make the ExecStart stub executable");
    }
    let needle = "ExecStart=/usr/local/bin/serial-nexus-daemon";
    let count = unit.matches(needle).count();
    let replacement = format!("ExecStart={}", stub.display());
    let staged = unit.replace(needle, &replacement);
    (scratch.write(name, &staged), count)
}

#[test]
fn the_packaged_unit_verifies_clean_under_systemd_analyze() {
    let me = "the_packaged_unit_verifies_clean_under_systemd_analyze";
    let Some(help) = run_tool("systemd-analyze", &["verify".into(), "--help".into()]) else {
        skip_no_packaging(me, "systemd-analyze not found on PATH");
        return;
    };
    assert!(
        help.status.is_some(),
        "systemd-analyze was killed by a signal rather than answering --help"
    );
    let flags = verify_flags(&help.stdout);

    let unit = read_tree_file(UNIT);
    let scratch = Scratch::new("verify");
    let (staged, applied) = stage_unit(&scratch, &unit, "staged.service");
    assert_eq!(
        applied, 1,
        "the ExecStart stub substitution matched {applied} time(s), not once — the \
         staged unit is not the one this gate believes it verified. Did the unit's \
         ExecStart= path change?"
    );

    let verify = |path: &Path| -> Run {
        let mut args = vec!["verify".to_owned()];
        args.extend(flags.iter().cloned());
        args.push(path.display().to_string());
        run_tool("systemd-analyze", &args).expect("systemd-analyze was found a moment ago")
    };

    // 0. The staging is load-bearing, proven rather than asserted in prose: on a box
    //    with no install, the *unstaged* unit must fail — and its failure must be
    //    the environmental one, read at its text and not at its status, because two
    //    different defects share exit 1 (plan §3 rule 17(f)).
    let installed = Path::new("/usr/local/bin/serial-nexus-daemon").exists();
    let unstaged_path = scratch.write("unstaged.service", &unit);
    let unstaged = verify(&unstaged_path);
    if installed {
        eprintln!(
            "NOTE {me}: /usr/local/bin/serial-nexus-daemon exists on this box, so the \
             environmental arm the staging removes could not be exercised here"
        );
    } else {
        let err = verdict(&unstaged)
            .expect_err("the unstaged unit verified clean on a box with no install");
        assert!(
            err.contains("not executable") || err.contains("No such file"),
            "the unstaged unit failed for a reason other than the missing ExecStart \
             program, so this gate's staging is removing the wrong thing. Reported: \
             {err}"
        );
    }

    // 1. The real unit, staged, verifies clean.
    let clean = verify(&staged);
    verdict(&clean).unwrap_or_else(|e| {
        panic!(
            "packaging/serial-nexus-daemon.service does not verify clean under \
             `systemd-analyze verify {}`.\n{e}",
            flags.join(" ")
        )
    });

    // 2. …and the gate bites, in every defect class it claims (plan §3 rule 10).
    //    Each plant asserts its own application first, then requires BOTH the
    //    verdict to redden AND the diagnostic to reach stderr — the second is what
    //    keeps the gate honest on a systemd whose exit status stays 0, which is
    //    every systemd at default flags for three of these five classes.
    let staged_text = std::fs::read_to_string(&staged).expect("read back the staged unit");
    let plants: [(&str, String); 6] = [
        (
            "an unknown key in [Service]",
            staged_text.replace("DynamicUser=yes", "NotADirective=yes\nDynamicUser=yes"),
        ),
        (
            "an unknown key in [Install]",
            format!("{staged_text}\nNotAnInstallKey=yes\n"),
        ),
        (
            "an unknown section",
            format!("{staged_text}\n[NotASection]\nFoo=bar\n"),
        ),
        (
            "an unparseable value on a lifecycle directive",
            staged_text.replace("RestartSec=2", "RestartSec=notanumber"),
        ),
        (
            "an unparseable value on a security directive",
            staged_text.replace("ProtectSystem=strict", "ProtectSystem=notavalue"),
        ),
        (
            "no ExecStart= at all",
            staged_text
                .lines()
                .filter(|l| !l.starts_with("ExecStart=") && !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    ];
    for (what, planted) in &plants {
        assert_ne!(
            planted, &staged_text,
            "the plant for {what} changed nothing, so the run below verifies the \
             clean unit and reads exactly like `the gate did not redden` (plan §3 \
             rule 17(b))"
        );
        let path = scratch.write("planted.service", planted);
        let run = verify(&path);
        let err = verdict(&run).expect_err(&format!(
            "planting {what} into the packaged unit did not redden this gate — it is \
             counted as coverage it does not have"
        ));
        assert!(
            !run.stderr.trim().is_empty(),
            "planting {what} reddened only the exit status and printed nothing. This \
             gate's stderr half is what covers a systemd older than v250 (no \
             --recursive-errors), so a class that reaches only the status is a class \
             that goes unseen there. Reported: {err}"
        );
    }

    // 3. The measured blind spot, re-measured rather than pinned. A systemd that
    //    grows this check should be noticed; asserting either answer would either
    //    contradict such a systemd or freeze today's.
    let nogroup = staged_text.replace(
        "SupplementaryGroups=dialout",
        "SupplementaryGroups=nosuchgroup12345",
    );
    assert_ne!(
        nogroup, staged_text,
        "the nonexistent-group probe changed nothing, so its reading below describes \
         the clean unit"
    );
    let nogroup_path = scratch.write("nogroup.service", &nogroup);
    let nogroup_run = verify(&nogroup_path);
    eprintln!(
        "MEASURED {me}: a SupplementaryGroups= naming a group that does not exist is \
         {} by `systemd-analyze verify {}` on this box (exit {:?}). Not an assertion \
         — see this file's `What this gate provably does not catch`.",
        if verdict(&nogroup_run).is_err() {
            "CAUGHT"
        } else {
            "NOT caught"
        },
        flags.join(" "),
        nogroup_run.status
    );
}

// ---------------------------------------------------------------------------
// (2) The udev rules, through udevadm
// ---------------------------------------------------------------------------

#[test]
fn the_packaged_udev_rules_verify_clean_under_udevadm() {
    let me = "the_packaged_udev_rules_verify_clean_under_udevadm";
    let Some(help) = run_tool("udevadm", &["verify".into(), "--help".into()]) else {
        skip_no_packaging(me, "udevadm not found on PATH");
        return;
    };
    if !help.stdout.contains("--resolve-names") {
        // `udevadm verify` itself arrived after `systemd-analyze verify`; an older
        // udevadm answers this subcommand with usage text.
        skip_no_packaging(
            me,
            "this udevadm has no `verify --resolve-names`, so the environmental arm \
             cannot be staged away",
        );
        return;
    }

    let rules = read_tree_file(RULES);
    let scratch = Scratch::new("udev");
    let staged = scratch.write("staged.rules", &rules);

    let check = |path: &Path, resolve: bool| -> Run {
        let mut args = vec!["verify".to_owned()];
        if !resolve {
            // The staging: `GROUP="serialnexus"` names a group an operator creates in
            // step 4 of the README and that no CI box has. Resolving names turns that
            // deployment fact into a syntax verdict — the same conflation the unit's
            // ExecStart= arm makes, and staged away the same way.
            args.push("--resolve-names=never".to_owned());
        }
        args.push(path.display().to_string());
        run_tool("udevadm", &args).expect("udevadm was found a moment ago")
    };

    // 0. The staging is load-bearing where the group is absent — reported, not
    //    asserted, because a box that happens to have a `serialnexus` group is a
    //    legitimate box and must not redden here.
    let resolving = check(&staged, true);
    eprintln!(
        "MEASURED {me}: with name resolution ON the packaged rules verify {} on this \
         box (exit {:?}) — the staging removes exactly that arm.",
        if verdict(&resolving).is_err() {
            "RED"
        } else {
            "clean"
        },
        resolving.status
    );

    // 1. The real rules verify clean, staged.
    let clean = check(&staged, false);
    verdict(&clean).unwrap_or_else(|e| {
        panic!("packaging/99-serial-nexus.rules does not verify clean under `udevadm verify --resolve-names=never`.\n{e}")
    });

    // 2. …and it bites.
    let plants: [(&str, String); 2] = [
        (
            "a missing comma between tokens",
            rules.replace(
                "SUBSYSTEM==\"tty\", SUBSYSTEMS==\"usb\"",
                "SUBSYSTEM==\"tty\" SUBSYSTEMS==\"usb\"",
            ),
        ),
        (
            "an invalid key",
            rules.replace("GROUP=\"serialnexus\"", "NOTAKEY=\"x\""),
        ),
    ];
    for (what, planted) in &plants {
        assert_ne!(
            planted, &rules,
            "the plant for {what} changed nothing, so the run below checks the clean \
             rules file (plan §3 rule 17(b))"
        );
        let path = scratch.write("planted.rules", planted);
        let run = check(&path, false);
        verdict(&run).expect_err(&format!(
            "planting {what} into the packaged udev rules did not redden this gate"
        ));
    }
}

// ---------------------------------------------------------------------------
// (3) README -> unit: no phantom directives
// ---------------------------------------------------------------------------

#[test]
fn every_directive_the_readme_names_exists_in_the_packaged_unit() {
    let unit = read_tree_file(UNIT);
    let readme = read_tree_file(README);

    // 0. The matchers, in every spelling they claim and against the near-misses they
    //    must ignore (plan §3 rule 10).
    let mentioned = mentioned_directives(&unit);
    assert!(
        active_directives("Foo=bar\n#comment\n    --continuation\n").contains("Foo"),
        "the active-directive matcher does not see a plain `Name=value` line"
    );
    assert!(
        active_directives("# Foo=bar\n").is_empty(),
        "a commented-out directive counts as active, so the unit could `document` a \
         setting it does not apply"
    );
    assert!(
        active_directives("    --socket /run/x.sock\n").is_empty(),
        "an ExecStart= continuation line is read as a directive"
    );
    assert!(
        active_directives("DynamicUser= puts the state directory somewhere\n").is_empty(),
        "prose after a bare `Name=` is read as a value, so the unit's own comments \
         would manufacture directives"
    );
    assert!(
        mentioned_directives("#   User=serial-nexus\n").contains("User"),
        "the mention matcher misses the unit's commented alternative recipe, so the \
         README's `User=` would be reported as a phantom"
    );
    assert!(
        backticked_directives("under `DynamicUser` the directory belongs").is_empty(),
        "a backticked directive *without* its `=` is enumerated, so ordinary prose \
         about a concept becomes a claim about a setting"
    );
    assert!(
        backticked_directives("chowns every `*Directory=` to `User=`").contains("User")
            && !backticked_directives("chowns every `*Directory=` to `User=`")
                .iter()
                .any(|d| d.contains('*')),
        "the glob `*Directory=` is enumerated as a directive name, or `User=` beside \
         it is missed"
    );
    assert!(
        backticked_directives("`insecure_bind = true` and `write_mode = \"held\"`").is_empty(),
        "a lowercase TOML key is enumerated as a systemd directive"
    );
    assert!(
        backticked_directives("`ProtectSystem=strict`").contains("ProtectSystem"),
        "the `Name=value` spelling is missed, and most of the README uses it"
    );

    // 1. The real scan, with a floor on each side. A roster derived from a matcher
    //    that stopped matching agrees with any document at all.
    let active = active_directives(&unit);
    let named = backticked_directives(&readme);
    assert!(
        active.len() >= 30,
        "the packaged unit parsed to {} active directive(s) — it was reshaped, or \
         the matcher broke; either way this gate is now comparing the README against \
         nothing",
        active.len()
    );
    assert!(
        named.len() >= 10,
        "packaging/README.md names {} systemd directive(s) in backticks — the page \
         was rewritten past this gate's reach",
        named.len()
    );

    // 2. Planted against this tree's own bytes, both directions, before the clean
    //    verdict is trusted. Victims come from the parsed sets so neither proof can
    //    go stale when the unit or the page changes.
    let phantom = "NotADirectiveAnyoneShips";
    let planted_readme = format!("{readme}\n\nAnd set `{phantom}=yes` while you are there.\n");
    let planted_named = backticked_directives(&planted_readme);
    assert!(
        planted_named.contains(phantom),
        "a directive invented in the README is not enumerated, so this gate cannot \
         notice the page describing a knob the unit has never had"
    );
    assert!(
        !planted_named.is_subset(&mentioned),
        "a README naming `{phantom}=` still reports no drift against the unit"
    );

    let victim = named
        .iter()
        .find(|d| active.contains(*d))
        .expect("the README names at least one directive the unit actually sets")
        .clone();
    let unit_without: String = unit
        .lines()
        .filter(|l| directive_at(l, 0).as_deref() != Some(victim.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(
        unit_without, unit,
        "deleting `{victim}=` from the unit changed nothing (plan §3 rule 17(b))"
    );
    let still_mentioned = mentioned_directives(&unit_without).contains(&victim);
    assert!(
        !active_directives(&unit_without).contains(&victim),
        "deleting the `{victim}=` line left it in the active roster, so this gate \
         cannot notice a directive the unit loses"
    );
    if !still_mentioned {
        assert!(
            !named.is_subset(&mentioned_directives(&unit_without)),
            "a unit that lost `{victim}=` entirely still reports no drift against a \
             README that names it"
        );
    }

    // 3. The verdict. `mentioned` rather than `active` on the unit side: the unit
    //    carries its alternatives in comments (the static-identity recipe, the extra
    //    log directory), and the README is right to describe those.
    let phantoms: BTreeSet<String> = named.difference(&mentioned).cloned().collect();
    assert!(
        phantoms.is_empty(),
        "packaging/README.md names systemd directive(s) that appear nowhere in \
         packaging/serial-nexus-daemon.service — either the page describes a knob \
         the unit does not have, or the unit lost one the page still explains:\n  \
         {}\nUnit side (active): {:?}\nUnit side (mentioned in comments too): {:?}",
        drift("the README", &named, "the unit", &mentioned).join("\n  "),
        active,
        mentioned,
    );

    // 4. And the two kinds stay distinguishable: a README directive that is only
    //    *mentioned* is reported, because it is the one an operator has to add by
    //    hand rather than one that ships switched on.
    let only_commented: Vec<&String> = named.difference(&active).collect();
    eprintln!(
        "MEASURED every_directive_the_readme_names_exists_in_the_packaged_unit: {} of \
         the {} directives the README names are set by the shipped unit; {:?} appear \
         only in its comments (the alternatives an operator applies by hand).",
        named.len() - only_commented.len(),
        named.len(),
        only_commented
    );
}

// ---------------------------------------------------------------------------
// (4) unit -> README: every directive has a recorded evidence class
// ---------------------------------------------------------------------------

#[test]
fn every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class() {
    let unit = read_tree_file(UNIT);
    let readme = read_tree_file(README);
    let section = evidence_section(&readme);

    let active = active_directives(&unit);
    let mentioned = mentioned_directives(&unit);
    // **Two rosters, because the two halves of the verdict below are two different
    // questions.** `recorded_in_column` is what the table's Directive column actually
    // records, one row at a time; `recorded_anywhere` is every directive the section
    // names at all, prose and Evidence cells included. Reading one for both was the
    // sibling defect plan §18 item 84's sweep turned up — see [`recorded_directives`],
    // and see the plant a few lines down that proves the difference is real on this
    // tree's own bytes rather than in principle.
    let (recorded_in_column, recorded_outside_column) = recorded_directives(section);
    let recorded_anywhere = backticked_directives(section);
    assert!(
        active.len() >= 30,
        "the packaged unit parsed to {} active directive(s); the unit has not shrunk \
         that far, so the matcher has",
        active.len()
    );
    assert!(
        recorded_in_column.len() >= 30,
        "packaging/README.md's `{EVIDENCE_HEADING}` tables record {} directive(s) in \
         their Directive column — the table was gutted, and a gate comparing against \
         a gutted table passes for the wrong reason",
        recorded_in_column.len()
    );
    assert!(
        recorded_anywhere.len() >= recorded_in_column.len(),
        "the section names fewer directives in total ({}) than its own Directive \
         columns record ({}), which is arithmetic rather than a finding — the row \
         walker and the section scan have stopped reading the same bytes",
        recorded_anywhere.len(),
        recorded_in_column.len()
    );

    // Planted both directions against real bytes, before the clean verdict. The
    // victim comes from the parsed roster rather than being named here, so neither
    // proof goes stale when the unit changes.
    let victim = active
        .iter()
        .next()
        .expect("the unit sets at least one directive")
        .clone();
    let (holed, removed) = strip_directive(section, &victim);
    assert!(
        removed > 0,
        "the evidence table records `{victim}=` in some other spelling than the one \
         this gate reads, so the deletion below proves nothing (plan §3 rule 17(b))"
    );
    assert!(
        !recorded_directives(&holed).0.contains(&victim),
        "`{victim}` survived {removed} deletion(s) from the evidence table — it is \
         recorded in a spelling the stripper misses, and the proof below would pass \
         on an unmutated table (plan §3 rule 17(b))"
    );
    assert!(
        !active.is_subset(&recorded_directives(&holed).0),
        "removing `{victim}=` from the evidence table produced no drift: this gate \
         cannot notice a shipped directive whose evidence class nobody recorded, \
         which is the whole of plan §18 item 31"
    );

    // And the narrower mutation, which is the one the old roster could not see
    // (plan §18 item 84's sibling sweep). The victim is a directive the Directive
    // column records *and* some other cell names in passing — `After=` is recorded
    // as `` `After=` `` and quoted back as `` `After=network.target` `` in the same
    // row's Evidence cell, `PrivateDevices=` likewise. Delete it from the Directive
    // column alone and the row has stopped recording an evidence class for a
    // directive the unit still ships, which is item 31's defect exactly; the
    // section-wide scan carries on holding the name and reports nothing.
    //
    // Both arms are asserted, and the second is the fail-first proof kept in the
    // tree rather than in a commit message: the section-wide roster **must still be
    // green** on this mutation. The day that stops being true, this proof has
    // stopped demonstrating the difference it was written to demonstrate, and the
    // assertion says so instead of quietly passing.
    match active
        .iter()
        .find(|d| recorded_in_column.contains(*d) && recorded_outside_column.contains(*d))
    {
        Some(both) => {
            let (column_holed, n) = strip_directive_from_directive_column(section, both);
            assert!(
                n > 0,
                "`{both}=` is recorded in a Directive column and in some other cell, \
                 yet the column-scoped stripper matched nothing (plan §3 rule 17(b))"
            );
            let (holed_column, _) = recorded_directives(&column_holed);
            assert!(
                !holed_column.contains(both),
                "`{both}` survived {n} deletion(s) from the Directive column, so the \
                 mutation below proves nothing"
            );
            assert!(
                !active.is_subset(&holed_column),
                "a table that stopped recording `{both}=` in its Directive column \
                 reported no drift — the per-row roster is reading the section again"
            );
            assert!(
                active.is_subset(&backticked_directives(&column_holed)),
                "the section-wide scan also lost `{both}` under this mutation, so \
                 this arm no longer demonstrates why the roster had to be scoped to \
                 the column. Pick a directive some other cell still names, or retire \
                 this proof and say why in plan §18"
            );
        }
        None => eprintln!(
            "NOTE every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class: \
             no directive is both recorded in a Directive column and named elsewhere \
             in the section, so item 84's sibling mutation could not be planted on \
             this tree. The scoping still holds; it is simply unproven here today."
        ),
    }

    let phantom = "NeverShippedDirective";
    let padded = format!("{section}\n| `{phantom}=` | invented | measured | nothing |\n");
    assert!(
        backticked_directives(&padded).contains(phantom),
        "a row for a directive the unit does not set is not enumerated, so the table \
         could keep documenting a directive that was deleted years ago"
    );
    assert!(
        !backticked_directives(&padded).is_subset(&mentioned),
        "an evidence row naming a directive the unit does not even mention produced \
         no drift"
    );

    // The verdict, in two asymmetric halves, because they are two different defects
    // and the asymmetry is not an oversight.
    //
    // *Every active directive must be recorded.* This is item 31's rule in as many
    // words: a directive the unit ships is a deployment claim, and a claim with no
    // recorded evidence class is what the item was filed against. **Recorded means
    // in a Directive column**, not merely named somewhere on the page: the roster
    // that stands opposite `active` here is the per-row one, because "the section
    // mentions the word" and "a row records its class" are different statements and
    // only the second is what item 31 asked for.
    let unrecorded = drift_one_way(&active, &recorded_in_column);
    assert!(
        unrecorded.is_empty(),
        "packaging/serial-nexus-daemon.service ships directive(s) that \
         packaging/README.md's `{EVIDENCE_HEADING}` table does not record — a \
         deployment claim whose evidence class nobody wrote down is exactly what \
         plan §18 item 31 was filed against:\n  {}",
        unrecorded.join("\n  ")
    );
    // *Every recorded directive must at least be mentioned by the unit.* The table
    // may legitimately record `User=`, `Group=` and `ReadWritePaths=`, which the
    // unit carries as commented alternatives rather than as settings — an operator
    // applying the static-identity recipe needs their evidence class as much as any
    // shipped directive's. What it may not do is record a directive the unit has
    // stopped naming at all.
    //
    // **This half stays section-wide on purpose, and it is not the sibling defect
    // wearing a different hat.** Those three directives are named in the section's
    // prose and Evidence cells rather than in any Directive column, which is exactly
    // the right place for a knob the unit does not ship; narrowing this half to the
    // column would drop them out of the check and make it *weaker*. The asymmetry is
    // the finding: a per-row read is the stronger form for "did anyone record this",
    // and a section-wide read is the stronger form for "is anything here describing
    // a unit that has moved on".
    let phantoms = drift_one_way(&recorded_anywhere, &mentioned);
    assert!(
        phantoms.is_empty(),
        "packaging/README.md's `{EVIDENCE_HEADING}` table records directive(s) that \
         appear nowhere in packaging/serial-nexus-daemon.service, not even in its \
         comments — the table is describing a unit that has moved on:\n  {}",
        phantoms.join("\n  ")
    );

    // The vocabulary half of this test used to live here, four lines of
    // `section.matches(class).count() > 0` under a comment claiming the column was
    // closed. It is now
    // [`every_evidence_row_records_one_of_the_three_classes_and_nothing_else`], which
    // reads the column instead of the section — plan §18 item 84, and the reason for
    // the move is written at that test.
    eprintln!(
        "MEASURED every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class: \
         {} active directives, all recorded in a Directive column; the section names {} \
         directive(s) in all, {} of them only outside that column ({:?})",
        active.len(),
        recorded_anywhere.len(),
        recorded_outside_column
            .difference(&recorded_in_column)
            .count(),
        recorded_outside_column
            .difference(&recorded_in_column)
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// (5) The evidence table's vocabulary, read per row — plan §18 item 84
// ---------------------------------------------------------------------------

/// Every row of the evidence tables records one of the three classes and nothing
/// else.
///
/// **This test is the repair of a gate that asserted less than its comment claimed**
/// (plan §18 item 84, filed 2026-08-16 by the CDC-ACM bench session, executed
/// 2026-08-21). What stood here before was four lines above
/// `every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class`'s tail:
///
/// ```text
/// // The table's own vocabulary is closed: three class words and no others. A row
/// // that invents a fourth is a claim nobody can weigh.
/// for class in ["measured", "man-page", "unverified"] {
///     counts.insert(class, section.matches(class).count());
/// }
/// ```
///
/// The comment is a statement about **every row**. The code is a statement about
/// **the whole section**, and a weak one: it says each of three words occurs at
/// least once anywhere in ~100 lines that also contain thirty-odd rows of prose
/// using those same words in ordinary sentences. Three unrelated rows hold all three
/// counters positive for good. So a row whose Class column read `assumed`, or
/// `probably`, or nothing whatsoever, passed untouched — and so did `measured
/// (partially)` for as long as it stood, because the substring `measured` is inside
/// it. That is AGENTS §3's fifth register, *a gate whose assertion is strictly
/// weaker than the comment above it claims*, and it is the fifth recorded instance
/// of it. The comment is what a reviewer reads, which is why this register keeps
/// getting past review.
///
/// # What it asserts now
///
/// The Class column, one cell at a time, against a grammar the README's own legend
/// defines:
///
/// * every table in the section carries a Class column — a renamed header removes a
///   whole table from every check below without changing anybody's output;
/// * every row's cell count matches its header's — a row that has lost a `|` reads
///   its Evidence prose as its Class cell, and that prose contains all three class
///   words;
/// * every class term opens with one of the three words, exactly and in lower case;
/// * a term's restriction **names the part of the claim it applies to** rather than
///   grading the class — see [`restriction_names_a_part`], which is where the
///   `(the mode)`-versus-`(partially)` line is drawn and defended;
/// * all three classes are still in use, which is the one property the old check
///   really did have, now computed from parsed rows rather than from substrings;
/// * and the legend the grammar is derived from still defines exactly [`CLASSES`].
///
/// # Fail-first, in every spelling the old comment claimed
///
/// A scanning gate has to prove its matcher as well as its walker (AGENTS §3), so
/// each bad spelling is planted into **every row of every table**, one row at a time,
/// and each plant must redden. Planting into row one only would not separate a
/// walker that reaches every row from one that reaches the first and stops.
///
/// # What this check deliberately does not decide
///
/// Whether a class is the *right* one. `measured` on a row whose Evidence column
/// cites nothing that ran is a review problem and stays one; the directive
/// correspondence next door is the machinery for the part of that which can be
/// derived. This check decides only that the column speaks the language the page
/// says it speaks.
#[test]
fn every_evidence_row_records_one_of_the_three_classes_and_nothing_else() {
    let me = "every_evidence_row_records_one_of_the_three_classes_and_nothing_else";
    let readme = read_tree_file(README);
    let section = evidence_section(&readme);

    // 0. The matchers, in every spelling they claim and against the near-misses they
    //    must ignore (plan §3 rule 10). A grammar checked only against the document
    //    it was written for is a grammar that has agreed with one file once.
    let words = |s: &str| -> BTreeSet<String> {
        s.split(|c: char| !c.is_ascii_alphabetic())
            .filter(|w| !w.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    };
    let none = BTreeSet::new();

    assert_eq!(
        split_row("| a | b |"),
        Some(vec!["a".to_owned(), "b".to_owned()]),
        "the row splitter does not read an ordinary two-cell row"
    );
    assert_eq!(
        split_row("| a |  | c |"),
        Some(vec!["a".to_owned(), String::new(), "c".to_owned()]),
        "an empty middle cell is dropped rather than judged, and `nothing at all` is \
         one of the spellings item 84 names"
    );
    assert_eq!(
        split_row("| a | `x|y` | b |"),
        Some(vec!["a".to_owned(), "`x|y`".to_owned(), "b".to_owned()]),
        "a `|` inside a code span opens a column, which shifts every cell to its \
         right and makes this gate read the Evidence prose as the Class cell"
    );
    assert!(
        split_row("Three classes, and nothing else is allowed:").is_none(),
        "a prose line is read as a table row"
    );
    assert!(
        is_delimiter_row(&["---".to_owned(), ":--".to_owned()])
            && !is_delimiter_row(&["measured".to_owned(), "x".to_owned()]),
        "the separator test does not separate a `|---|` line from a data row, so a \
         data row would be taken for a header"
    );

    assert_eq!(
        class_terms("measured", &none).as_deref(),
        Ok(&["measured".to_owned()][..]),
        "a bare class word is refused"
    );
    assert_eq!(
        class_terms("**man-page**", &none).as_deref(),
        Ok(&["man-page".to_owned()][..]),
        "a bolded class word is read as a fourth class, and the table bolds wherever \
         the author wanted the reader's eye"
    );
    assert_eq!(
        class_terms("measured (the mode)", &none).as_deref(),
        Ok(&["measured".to_owned()][..]),
        "a restriction that names which half of the claim was measured is refused — \
         `measured (the mode)` is a real row and flattening it to `measured` would \
         make that row overclaim"
    );
    assert_eq!(
        class_terms("measured (CI root arm, run 31695823765)", &none).as_deref(),
        Ok(&["measured".to_owned()][..]),
        "a restriction that cites its evidence in the column itself is refused"
    );
    assert_eq!(
        class_terms(
            "man-page for the effect; **measured for acceptance**",
            &words("Acceptance — that systemd parses every one of these")
        )
        .as_deref(),
        Ok(&["man-page".to_owned(), "measured".to_owned()][..]),
        "a two-class cell is refused, or the `for <a word the row itself uses>` shape \
         is not admitted — both are real rows"
    );
    assert!(
        class_terms(
            "man-page for the effect; **measured for acceptance**",
            &words("nothing here says that word")
        )
        .is_err(),
        "the `every substantial word is one the row itself uses` arm accepts a word \
         the row does not use, so that arm asserts nothing and the whole rule \
         collapses to `any for-phrase passes`"
    );
    for (why, cell) in [
        ("a fourth class word", "assumed"),
        ("a second fourth class word", "probably"),
        ("nothing at all", ""),
        ("a hedge wearing a real class word", "measured (partially)"),
        ("a hedge nobody told this gate about", "measured (broadly)"),
        (
            "a third hedge, so the refusal is not a word list",
            "measured (roughly)",
        ),
        ("a hedge in front of the class word", "mostly measured"),
        ("a class word in the wrong case", "Measured"),
        (
            "a compound whose second term is invented",
            "measured; assumed",
        ),
        (
            "a connective this grammar does not admit",
            "man-page and measured",
        ),
        ("a class word buried in a parenthetical", "(measured)"),
        ("a restriction that narrows to nothing", "measured ()"),
        ("a class word with a suffix grown onto it", "measured-ish"),
    ] {
        assert!(
            class_terms(cell, &none).is_err(),
            "the class-cell grammar accepts `{cell}` ({why}), which the old \
             substring count also accepted — item 84 is not fixed"
        );
    }

    // 1. The real read, with floors on both the walker and the rows it reached. A
    //    grammar applied to nothing agrees with everything.
    let reading = read_evidence_vocabulary(section);
    assert!(
        reading.tables >= 3,
        "the `{EVIDENCE_HEADING}` section parsed to {} table(s) — it carries a legend \
         and two claim tables, so the row walker has stopped seeing markdown",
        reading.tables
    );
    assert!(
        reading.rows >= 24,
        "the `{EVIDENCE_HEADING}` section parsed to {} row(s); it carries about \
         thirty, so either the record was gutted or this gate is now judging a \
         handful of rows and calling the column closed",
        reading.rows
    );

    // 2. Planted against this tree's own bytes, in every spelling and in every row,
    //    before the clean verdict below is trusted (plan §3 rules 10 and 17(b)).
    let spellings = [
        ("a fourth class word", "assumed"),
        ("nothing at all", ""),
        ("a hedge wearing a real class word", "measured (partially)"),
        ("a hedge nobody told this gate about", "measured (broadly)"),
        ("a class word in the wrong case", "Measured"),
        (
            "a compound whose second term is invented",
            "measured; assumed",
        ),
    ];
    let mut planted_rows = 0usize;
    for table in md_tables(section) {
        let class_at = column_index(&table.header, "Class").unwrap_or_else(|| {
            panic!(
                "the table headed {:?} has no Class column — the verdict below would \
                 skip every one of its rows",
                table.header
            )
        });
        for row in &table.rows {
            planted_rows += 1;
            for (why, cell) in &spellings {
                let planted = plant_class_cell(section, row.line, class_at, cell);
                let landed = planted
                    .lines()
                    .nth(row.line)
                    .and_then(split_row)
                    .and_then(|c| c.get(class_at).cloned());
                assert_eq!(
                    landed.as_deref(),
                    Some(*cell),
                    "the plant for {why} did not land in row `{}`'s Class cell, so \
                     the run below judges the clean table and reads exactly like \
                     `the gate did not redden` (plan §3 rule 17(b))",
                    row.cells.first().map_or("?", String::as_str)
                );
                let after = read_evidence_vocabulary(&planted);
                assert!(
                    !after.defects.is_empty(),
                    "planting {why} (`{cell}`) into row `{}` of the table headed {:?} \
                     did not redden this gate. That row is counted as checked and is \
                     not — which is the whole of plan §18 item 84",
                    row.cells.first().map_or("?", String::as_str),
                    table.header
                );
            }
        }

        // The header rename, which is how a whole table leaves this gate's reach
        // without anybody's output changing.
        let renamed = plant_class_cell(section, table.header_line, class_at, "Kind");
        let after = read_evidence_vocabulary(&renamed);
        assert!(
            after.defects.iter().any(|d| d.contains("no Class column")),
            "renaming the Class header of the table headed {:?} was not reported — a \
             renamed column silently removes every one of its rows from the verdict",
            table.header
        );

        // The ragged row: one `|` short, so every cell to the right of it is read as
        // the wrong column and the Evidence prose lands where the class is judged.
        if let Some(row) = table.rows.first() {
            let ragged = plant_ragged_row(section, row.line);
            let after = read_evidence_vocabulary(&ragged);
            assert!(
                after
                    .defects
                    .iter()
                    .any(|d| d.contains("against the header's")),
                "a row one cell short of its header was not reported in the table \
                 headed {:?}; its Evidence prose names all three classes, so a \
                 shifted read is a silently green one",
                table.header
            );
        }
    }
    assert!(
        planted_rows >= 24,
        "the plant sweep reached {planted_rows} row(s), so most of the table was \
         never proven to be under this gate at all"
    );

    // The vocabulary collapsing to one class: the property the old check really did
    // have, kept, and now proven to bite from parsed rows rather than substrings.
    let mut collapsed = section.to_owned();
    for table in md_tables(section) {
        let Some(class_at) = column_index(&table.header, "Class") else {
            continue;
        };
        for row in &table.rows {
            collapsed = plant_class_cell(&collapsed, row.line, class_at, "measured");
        }
    }
    let collapsed_reading = read_evidence_vocabulary(&collapsed);
    assert!(
        collapsed_reading.defects.is_empty(),
        "collapsing every row to `measured` produced a grammar defect, so the arm \
         below is reddening for the wrong reason: {:?}",
        collapsed_reading.defects
    );
    assert!(
        !CLASSES
            .iter()
            .all(|c| collapsed_reading.heads.contains_key(*c)),
        "a table whose every row reads `measured` still reports all three classes in \
         use — a record that stopped distinguishing would go unnoticed"
    );

    // 3. The verdict.
    assert!(
        reading.defects.is_empty(),
        "packaging/README.md's `{EVIDENCE_HEADING}` tables carry {} row(s) whose \
         Class column is not one of the three classes the page's own legend defines. \
         The vocabulary is closed — a row that invents a fourth class, or hedges a \
         real one into a fourth, is a claim nobody can weigh (plan §18 item 84):\n  {}",
        reading.defects.len(),
        reading.defects.join("\n  ")
    );
    for class in CLASSES {
        assert!(
            reading.heads.contains_key(class),
            "no row in the evidence table records the class `{class}` — a record \
             with only one class in it is a record that stopped distinguishing"
        );
    }

    // 4. And the legend the grammar came from still says what this gate believes.
    //    Derived from the page, held against the constant: either half alone is the
    //    tell AGENTS §3 names, and [`CLASSES`] carries the argument.
    let legend = legend_classes(section);
    let expected: BTreeSet<String> = CLASSES.iter().map(|c| (*c).to_owned()).collect();
    assert_eq!(
        legend, expected,
        "packaging/README.md's `| Class | Means |` legend defines a different \
         vocabulary from the one this gate enforces. Whichever moved, the two must \
         be reconciled deliberately: the legend is what a reader is told the column \
         means, and the constant is what the column is held to"
    );
    let legend_row = md_tables(section)
        .into_iter()
        .find(|t| {
            t.header
                .iter()
                .map(|h| strip_emphasis(h).trim().to_ascii_lowercase())
                .collect::<Vec<_>>()
                == ["class", "means"]
        })
        .and_then(|t| t.rows.first().map(|r| r.line))
        .expect("the legend table has at least one row");
    let legend_planted = plant_class_cell(section, legend_row, 0, "**assumed**");
    assert_ne!(
        legend_classes(&legend_planted),
        expected,
        "a legend that defines a fourth class reads the same as one that does not, \
         so the derivation above is decorative and this gate is holding its constant \
         against nothing (plan §3 rule 17(b))"
    );

    eprintln!(
        "MEASURED {me}: {} table(s), {} row(s), all judged; classes in use {:?}; \
         legend {:?}",
        reading.tables, reading.rows, reading.heads, legend
    );
}

// ---------------------------------------------------------------------------
// (6) The root-gated measurement — item 31's owed half
// ---------------------------------------------------------------------------

/// This box's effective uid, read from `/proc/self/status` rather than `geteuid(2)`.
///
/// `serial_nexus_sys` is where `unsafe` lives (AGENTS §4) and the harness is
/// `#![forbid(unsafe_code)]`, so the uid arrives as text. `None` where there is no
/// procfs, which is itself the answer: no procfs, no systemd.
fn effective_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    // `Uid:\treal\teffective\tsaved\tfs`
    line.split_whitespace().nth(2)?.parse().ok()
}

/// What PID 1 is called, or `None` where there is no procfs.
fn pid1_comm() -> Option<String> {
    std::fs::read_to_string("/proc/1/comm")
        .ok()
        .map(|s| s.trim().to_owned())
}

/// The `[Service]` directives of `unit`, as `-p Name=value` arguments for
/// `systemd-run`, with the three `*Directory=` values renamed to `suffix`.
///
/// Derived from the shipped unit rather than hand-listed, because the point of the
/// measurement is what the *packaged* sandbox does — a hand-written approximation
/// would measure a unit nobody installs. The renames keep a probe from colliding
/// with a real install's state.
///
/// `ExecStart=`, `Type=`, `Restart=` and `RestartSec=` are dropped: `systemd-run`
/// supplies its own, and a restart policy on a one-shot probe would loop.
fn service_properties(unit: &str, suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_service = false;
    let dropped = ["ExecStart", "Type", "Restart", "RestartSec"];
    for line in unit.lines() {
        if line.starts_with('[') {
            in_service = line.starts_with("[Service]");
            continue;
        }
        if !in_service {
            continue;
        }
        let Some(name) = directive_at(line, 0) else {
            continue;
        };
        if dropped.contains(&name.as_str()) {
            continue;
        }
        let value = line[name.len() + 1..].trim();
        let value = match name.as_str() {
            "RuntimeDirectory" | "StateDirectory" | "LogsDirectory" => suffix,
            _ => value,
        };
        out.push("-p".to_owned());
        out.push(format!("{name}={value}"));
    }
    out
}

/// `key=value` lines from the probe's stdout, as a map.
fn probe_readings(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// The half of the root-gated measurement that can be checked without root.
///
/// Only CI's `packaging` job satisfies the root precondition; a development box does
/// not — so the part of the probe's machinery that is pure derivation runs on every
/// push, on every platform, instead of riding along with the arm that one lane
/// executes. What is covered here: that the probe's `systemd-run` properties really are the packaged
/// unit's own `[Service]` directives, that the four `systemd-run` supplies itself are
/// dropped, and that the three `*Directory=` values are renamed so a probe cannot
/// collide with a real install's state. What is not: anything that needs the service
/// to start.
#[test]
fn the_root_probe_derives_its_sandbox_from_the_packaged_unit() {
    let unit = read_tree_file(UNIT);
    let props = service_properties(&unit, "probe-tag");
    let pairs: Vec<&String> = props.iter().filter(|a| a.as_str() != "-p").collect();
    assert_eq!(
        props.len(),
        pairs.len() * 2,
        "every property must arrive as a `-p` flag and its value; systemd-run reads \
         no other spelling"
    );

    let names: BTreeSet<String> = pairs
        .iter()
        .filter_map(|p| directive_at(p, 0))
        .collect::<BTreeSet<_>>();
    let active = active_directives(&unit);
    let service_actives: BTreeSet<String> = {
        // The `[Service]` half of the unit's actives, derived the same way the
        // property builder derives it, so this floor tracks the unit.
        let mut in_service = false;
        let mut out = BTreeSet::new();
        for line in unit.lines() {
            if line.starts_with('[') {
                in_service = line.starts_with("[Service]");
                continue;
            }
            if in_service && let Some(n) = directive_at(line, 0) {
                out.insert(n);
            }
        }
        out
    };
    assert!(
        service_actives.len() >= 30,
        "the unit's [Service] section parsed to {} directive(s) — the probe would \
         measure a sandbox far weaker than the one that ships",
        service_actives.len()
    );

    for dropped in ["ExecStart", "Type", "Restart", "RestartSec"] {
        assert!(
            !names.contains(dropped),
            "`{dropped}=` was passed to systemd-run, which supplies its own — a \
             Restart= on a one-shot probe loops, and an ExecStart= collides with the \
             command"
        );
    }
    let expected: BTreeSet<String> = service_actives
        .difference(
            &["ExecStart", "Type", "Restart", "RestartSec"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        )
        .cloned()
        .collect();
    assert_eq!(
        names, expected,
        "the probe's sandbox is not the packaged unit's. Deriving it is the point: a \
         hand-written approximation measures a unit nobody installs"
    );
    assert!(
        active.is_superset(&names),
        "the probe passes a directive the unit does not set"
    );

    // The renames applied, and only to the three directory families.
    for family in ["RuntimeDirectory", "StateDirectory", "LogsDirectory"] {
        let value = pairs
            .iter()
            .find(|p| p.starts_with(&format!("{family}=")))
            .unwrap_or_else(|| panic!("the probe passes no `{family}=`"));
        assert_eq!(
            value.as_str(),
            &format!("{family}=probe-tag"),
            "the probe would write into the packaged unit's real {family}=, so a run \
             on a box with an install would destroy its state"
        );
    }
    let hardening = pairs
        .iter()
        .find(|p| p.starts_with("ProtectSystem="))
        .expect("the probe passes no ProtectSystem=");
    assert_eq!(
        hardening.as_str(),
        "ProtectSystem=strict",
        "a hardening value was rewritten on its way to the probe; the EROFS half of \
         the ReadWritePaths measurement depends on this one being verbatim"
    );

    // --- The socket the daemon-bearing payloads bind, derived not typed ----------
    //
    // The runtime *directory* is renamed above, so the probe cannot reuse the unit's
    // path — but its basename was a literal in `daemon_payload` until 2026-08-21,
    // which left the recipe's second `stat` prediction being compared against a
    // socket the payload had named for itself. This is the derivation, checked here
    // because it is pure text and needs no root.
    let sock_name = socket_file_name(&unit);
    let exec_socket =
        exec_start_flag(&unit, "--socket").expect("the unit's ExecStart= passes --socket");
    assert_eq!(
        exec_socket,
        format!(
            "/run/{}/{sock_name}",
            active_value(&unit, "RuntimeDirectory").unwrap_or_default()
        ),
        "the unit's `ExecStart=` binds `{exec_socket}`, which is not the socket name \
         `{sock_name}` inside its own `RuntimeDirectory=`. The daemon-bearing probes \
         bind `/run/<tag>/{sock_name}`, so the two would be measuring different files"
    );
    let payload = daemon_payload(
        "probe-tag",
        Path::new("/usr/local/lib/probe-tag"),
        Path::new("/etc/probe-tag/config.toml"),
        &sock_name,
        "",
    );
    assert!(
        payload.contains(&format!("sock=$rt/{sock_name}\n")),
        "the daemon-bearing payload does not bind `$rt/{sock_name}`, so the socket it \
         stats is not the one this unit's `ExecStart=` names:\n{payload}"
    );
    assert!(
        payload.contains("printf 'tmp_sentinel=%s\\n' \"$([ -e /tmp/probe-tag.sentinel ]"),
        "the daemon-bearing payload does not read the `PrivateTmp=` sentinel \
         `run_probe` plants, so `assert_daemon_served`'s only positive control on the \
         mount namespace would be missing from the run it judges:\n{payload}"
    );
}

#[test]
fn dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown() {
    let me = "dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown";

    // --- Preconditions, each naming what the provider actually saw ---------------
    let Some(comm) = pid1_comm() else {
        skip_no_packaging_root(me, "no /proc/1/comm on this platform, so no systemd");
        return;
    };
    if comm != "systemd" {
        skip_no_packaging_root(
            me,
            &format!("PID 1 is `{comm}`, not systemd — nothing here can start a unit"),
        );
        return;
    }
    let Some(uid) = effective_uid() else {
        skip_no_packaging_root(
            me,
            "could not read /proc/self/status to learn the effective uid",
        );
        return;
    };
    if uid != 0 {
        skip_no_packaging_root(
            me,
            &format!(
                "effective uid is {uid}, not 0. Without root, `systemd-run` answers \
                 `Access denied … requires interactive authentication` (polkit), and \
                 the rootless fallback is closed too: `unshare -Ur` is refused with \
                 `write failed /proc/self/uid_map: Operation not permitted`"
            ),
        );
        return;
    }
    if run_tool("systemd-run", &["--version".into()]).is_none() {
        skip_no_packaging_root(me, "systemd-run not found on PATH");
        return;
    }

    // --- Setup ------------------------------------------------------------------
    let tag = format!("snx-pkg-probe-{}", std::process::id());
    // **Not `std::env::temp_dir()`, and the reason is the unit under test.** The
    // packaged unit sets `PrivateTmp=yes`, which gives the service a *private*
    // `/tmp` and `/var/tmp`; a `ReadWritePaths=` naming a path under either one
    // therefore names a path that does not exist inside the namespace, and systemd
    // fails mount-namespace setup before `ExecStart` — `status=226`, `EXIT_NAMESPACE`.
    // That is exactly what CI's packaging job reported on every run from the arm's
    // landing (2026-08-13) until this line changed: the probe unit "did not run",
    // and the assertion below blamed the packaged sandbox for what was the probe's
    // own choice of scratch directory. `/run` is not privatised by `PrivateTmp=`,
    // exists in the namespace, and is the same tree `RuntimeDirectory=` already
    // writes into under this unit's `ProtectSystem=strict` — so `ReadWritePaths=`
    // has something real to re-mount read-write, which is the mechanism under test.
    //
    // **Where the probe directories live is the measurement**, and getting there took
    // four CI runs, each ruling one candidate out. The requirement is exact: a tree
    // that `ProtectSystem=strict` genuinely mounts **read-only**, that exists inside
    // the namespace, and that is **not** one of the unit's own `*Directory=` trees.
    //
    // * `/tmp` (and `/var/tmp`) — refused: `PrivateTmp=yes` replaces both, so a
    //   `ReadWritePaths=` naming a path under them names one that does not exist in
    //   the namespace, and systemd fails mount setup with `status=226` `EXIT_NAMESPACE`
    //   before `ExecStart`.
    // * `/run/<tag>` — refused: `service_properties` renames the three `*Directory=`
    //   values to `tag`, so `RuntimeDirectory=<tag>` is *this same path*, which systemd
    //   creates and, under `DynamicUser=yes`, **chowns** to the service user. That
    //   handed the probe directories to the very user whose writes the test refuses,
    //   and the ownership arm died silently reporting `listed_write=ok`.
    // * `/run/<tag>-scratch` — refused, and this one is subtler: it fixed the ownership
    //   arm (`listed_write` correctly read `fail:… Permission denied`) and broke the
    //   **control**, which reported `unlisted_write=ok`. `/run` is writable for
    //   services, so `ProtectSystem=strict` never refuses anything there and the
    //   control cannot produce the `EROFS` that separates "the mount flipped" from
    //   "the ownership changed".
    // * `/var/lib/<tag>-scratch` — this one. `/var` is read-only under
    //   `ProtectSystem=strict`, so the unlisted sibling gets `EROFS`; the listed one is
    //   remounted read-write by `ReadWritePaths=` and, being root-owned 0755, gets
    //   `EACCES`. That is the pair Claim 4 needs. It is a *sibling* of
    //   `StateDirectory`'s `/var/lib/<tag>`, never inside it, which is what the
    //   `-scratch` suffix keeps true.
    //
    // **Verified on a real box, unlike each of its predecessors** — the CI run after
    // this line changed read all nine readings with both halves of Claim 4 holding
    // (plan §18 item 68; run 31695823765, 2026-08-13, reproduced at 31877969760).
    let base = PathBuf::from("/var/lib").join(format!("{tag}-scratch"));
    let rw_dir = base.join("rw-listed");
    let ro_dir = base.join("rw-unlisted");
    std::fs::create_dir_all(&rw_dir).expect("create the ReadWritePaths probe directory");
    std::fs::create_dir_all(&ro_dir).expect("create the control probe directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 0755 root-owned: the ownership arm. Anyone may traverse; only root writes.
        std::fs::set_permissions(&rw_dir, std::fs::Permissions::from_mode(0o755))
            .expect("mode the ReadWritePaths probe directory");
        // 0777 root-owned: the *control*. Ownership cannot be what refuses a write
        // here, so a refusal isolates the mount — which is the whole measurement.
        std::fs::set_permissions(&ro_dir, std::fs::Permissions::from_mode(0o777))
            .expect("mode the control probe directory");
    }

    let unit = read_tree_file(UNIT);
    let mut args = vec![
        "--wait".to_owned(),
        "--pipe".to_owned(),
        "--collect".to_owned(),
        format!("--unit={tag}"),
    ];
    args.extend(service_properties(&unit, &tag));
    args.push("-p".to_owned());
    args.push(format!("ReadWritePaths={}", rw_dir.display()));
    args.push("/bin/sh".to_owned());
    args.push("-c".to_owned());
    // **`true >` and not `: >`, and dash is why** (plan §18 item 68). `:` is a POSIX
    // *special* built-in, and a redirection error on a special built-in "shall cause
    // the shell to exit" — so on Ubuntu, where `/bin/sh` is dash, the very refusal
    // this probe exists to observe killed the script mid-run and `set +e` could not
    // help: CI reported `status=2` with `cannot create …: Permission denied` and no
    // readings past that line. `true` is a regular built-in, so the redirection
    // failure is just a false exit status, which is what the `if` is there to read.
    // The write being *denied* is the expected result — it is the README's claim that
    // `ReadWritePaths` flips the mount without chowning — so the probe was dying
    // exactly on success.
    //
    // **And `2>/tmp/eN` comes BEFORE the failing redirection, which is the other half
    // of the same gotcha.** Redirections are applied left to right, so `> file
    // 2>/tmp/e2` fails on `> file` and never applies `2>`: the diagnostic goes to the
    // *inherited* stderr and the capture file stays empty. CI read exactly that —
    // `listed_write=fail:` with nothing after the colon, while the message appeared in
    // the unit's own stderr. Verified on dash rather than reasoned: with the old order
    // the reading is `fail:`, with this one it is `fail:/bin/dash: 2: cannot create
    // …: Permission denied`, which is what the assertion downstream needs to tell
    // EACCES from EROFS.
    args.push(format!(
        r#"set +e
sd=/var/lib/{tag}
printf 'uid=%s\n' "$(id -u)"
printf 'user=%s\n' "$(id -un 2>/dev/null || echo unknown)"
if echo probe > "$sd/probe.txt" 2>/tmp/e1; then printf 'state_write=ok\n'
else printf 'state_write=fail:%s\n' "$(tr -d '\n' < /tmp/e1)"; fi
printf 'state_stat=%s\n' "$(stat -c '%U:%a' "$sd" 2>/dev/null || echo none)"
printf 'state_real=%s\n' "$(readlink -f "$sd" 2>/dev/null || echo none)"
if true 2>/tmp/e2 > "{rw}/probe.txt"; then printf 'listed_write=ok\n'
else printf 'listed_write=fail:%s\n' "$(tr -d '\n' < /tmp/e2)"; fi
if true 2>/tmp/e3 > "{ro}/probe.txt"; then printf 'unlisted_write=ok\n'
else printf 'unlisted_write=fail:%s\n' "$(tr -d '\n' < /tmp/e3)"; fi
if ls /var/lib/private > /dev/null 2>/tmp/e4; then printf 'private_list=ok\n'
else printf 'private_list=fail:%s\n' "$(tr -d '\n' < /tmp/e4)"; fi
printf 'private_stat=%s\n' "$(stat -c '%U:%a' /var/lib/private 2>/dev/null || echo none)"
"#,
        tag = tag,
        rw = rw_dir.display(),
        ro = ro_dir.display(),
    ));

    let out = Command::new("systemd-run")
        .args(&args)
        .output()
        .expect("systemd-run answered --version a moment ago");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    // Clean up before asserting, so a red gate does not also leave state behind.
    let host_link = PathBuf::from(format!("/var/lib/{tag}"));
    let host_real = std::fs::read_link(&host_link).ok();
    let host_private = PathBuf::from(format!("/var/lib/private/{tag}"));
    let host_private_exists = host_private.exists();
    let _ = std::fs::remove_file(&host_link);
    let _ = std::fs::remove_dir_all(&host_private);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(format!("/var/log/{tag}"));
    let _ = std::fs::remove_dir_all(format!("/var/log/private/{tag}"));

    assert!(
        out.status.success(),
        "the probe unit — the packaged unit's own [Service] directives, with the \
         three *Directory= names changed — did not run.\n\
         **Read the systemd status word before attributing this.** The first version \
         of this message asserted it was 'a finding about the packaged sandbox, not \
         about the probe', and on its first CI run that was wrong: `status=226` is \
         `EXIT_NAMESPACE`, systemd failing to build the mount namespace, and the \
         cause was the probe putting its `ReadWritePaths=` directories under `/tmp` \
         while the unit sets `PrivateTmp=yes` (notes §3.93). A sandbox finding and a \
         probe defect land in the same place here, so the status word is what \
         separates them: 226 points at the namespace (paths, mounts — suspect the \
         probe first), 216/`EXIT_GROUP` at `SupplementaryGroups=`, usual exit codes \
         at the payload.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let r = probe_readings(&stdout);
    assert!(
        r.len() >= 8,
        "the probe printed {} reading(s); it is meant to print nine, so it died \
         partway and every assertion below would be about a shorter run.\n\
         stdout:\n{stdout}\nstderr:\n{stderr}",
        r.len()
    );
    let get = |k: &str| -> &str {
        r.get(k)
            .map(String::as_str)
            .unwrap_or_else(|| panic!("the probe printed no `{k}` reading:\n{stdout}"))
    };

    // --- Claim 1: DynamicUser= gives a transient, non-root identity --------------
    assert_ne!(
        get("uid"),
        "0",
        "the probe ran as root, so DynamicUser= did not take effect and nothing \
         below measures what the packaged unit promises"
    );

    // --- Claim 2: StateDirectory= is created, chowned, and writable ---------------
    assert_eq!(
        get("state_write"),
        "ok",
        "the transient identity could not write its own StateDirectory=, which is \
         the one thing the unit's `systemd creates AND chowns these` comment \
         promises"
    );

    // --- Claim 3: the real directory is under /var/lib/private/ ------------------
    // The unit's upgrade comment and the README's `sudo cp` procedure both rest on
    // exactly this, and neither had ever been measured.
    assert!(
        host_private_exists,
        "no /var/lib/private/{tag} on the host after the run: the unit's comment \
         (`DynamicUser= puts the real state directory under /var/lib/private/`) and \
         the README's upgrade procedure both rest on this indirection existing"
    );
    let real = host_real
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<not a symlink>".to_owned());
    assert!(
        real.contains("private"),
        "/var/lib/{tag} is `{real}`, not a symlink into private/ — the README's \
         upgrade step assumes the host path is an indirection"
    );

    // --- Claim 4: ReadWritePaths= flips the mount and does NOT chown --------------
    // Two errnos, one measurement. The listed directory is root-owned 0755: the
    // mount is read-write, so a refusal there can only be ownership. The control is
    // root-owned 0777 and NOT listed: ownership cannot refuse it, so a refusal there
    // can only be the mount. Getting `Permission denied` from the first and
    // `Read-only file system` from the second is the claim, stated as a pair rather
    // than as one assertion, because either alone is consistent with the wrong story.
    let listed = get("listed_write");
    let unlisted = get("unlisted_write");
    assert!(
        listed.starts_with("fail:") && listed.contains("Permission denied"),
        "writing into a root-owned 0755 directory listed in ReadWritePaths= gave \
         `{listed}`. The README tells operators to pre-chown such a directory \
         because `ReadWritePaths only flips the mount to read-write, it does not \
         chown` — if this write succeeded, that instruction is wrong and the page \
         must be corrected"
    );
    assert!(
        unlisted.starts_with("fail:") && unlisted.contains("Read-only file system"),
        "writing into a root-owned 0777 directory NOT listed in ReadWritePaths= gave \
         `{unlisted}`. Under ProtectSystem=strict it must be refused by the mount \
         (EROFS), and the contrast with the listed directory's EACCES is what \
         separates `the mount flipped` from `the ownership changed`"
    );

    // --- Reported, not asserted: is another unit's private state reachable? -------
    // The unit's upgrade comment says the daemon `cannot read the old file even
    // knowing where it is`. Our own StateDirectory= forces /var/lib/private into the
    // namespace, so whether a *sibling* directory under it is reachable is a real
    // question and the answer belongs in the record before it becomes an assertion.
    eprintln!(
        "MEASURED {me}: probe uid={} user={} state_stat={} state_real={} \
         private_list={} private_stat={} host_link={real}",
        get("uid"),
        get("user"),
        get("state_stat"),
        get("state_real"),
        get("private_list"),
        get("private_stat"),
    );
}

// ---------------------------------------------------------------------------
// (7) The socket-group recipe, read out of the unit's own comment block
// ---------------------------------------------------------------------------

/// The sentence that opens the unit's socket-group recipe.
///
/// Named here rather than discovered so a rewrite of that comment block fails loudly
/// at [`socket_group_recipe`] instead of silently reducing every check below to an
/// empty recipe agreeing with an empty probe.
const RECIPE_OPENS: &str = "Create it once:";

/// The static-identity recipe `packaging/serial-nexus-daemon.service` spells out in
/// its socket-group comment block, parsed out of the shipped file.
///
/// **Why parse a comment.** The block is a *recipe an operator runs*, and plan §18
/// item 31 files it as a claim with no evidence behind it. The probes below execute
/// it; if they executed a hand-typed copy they would measure a recipe nobody ships,
/// which is the failure `the_root_probe_derives_its_sandbox_from_the_packaged_unit`
/// already exists to prevent one section over.
struct SocketGroupRecipe {
    /// The `groupadd`/`useradd` argv, in the order written, with the leading `sudo`
    /// dropped: the probe already runs as root, and `sudo` is how an *operator* gets
    /// there. That is the one deviation from verbatim execution, and it is the only
    /// one — see [`create_recipe_identity`].
    commands: Vec<Vec<String>>,
    /// The replacement directives (`DynamicUser=no`, `User=`, …), in written order.
    directives: Vec<(String, String)>,
    /// The paths the block's `stat` line names, in written order.
    stat_paths: Vec<String>,
    /// The `%U %G %a` lines the block predicts, in the same order as `stat_paths`.
    predictions: Vec<String>,
    /// The daemon flag the block's `ExecStart=` line adds, e.g.
    /// `--socket-group console-operators`.
    socket_group_flag: Option<String>,
}

impl SocketGroupRecipe {
    fn directive(&self, name: &str) -> Option<&str> {
        self.directives
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// `line` with its leading `#` removed, or `None` if it is not a comment.
fn comment_body(line: &str) -> Option<&str> {
    line.strip_prefix('#').map(str::trim)
}

/// The unit's active `ExecStart=` as one argv line, systemd's trailing-backslash
/// continuations joined.
///
/// The shipped `ExecStart=` spans four lines, so a reader that took only the first
/// would see the program and none of its flags — which is the whole reason
/// [`check_recipe`] could not tie the recipe's second `stat` path to anything before.
fn exec_start_argv(unit: &str) -> Option<String> {
    let mut lines = unit.lines();
    let head = lines.find(|l| directive_at(l, 0).as_deref() == Some("ExecStart"))?;
    let mut argv = head["ExecStart=".len()..].trim().to_owned();
    while let Some(rest) = argv.strip_suffix('\\') {
        let Some(next) = lines.next() else { break };
        argv = format!("{} {}", rest.trim_end(), next.trim());
    }
    Some(argv)
}

/// The argument the unit's active `ExecStart=` passes to `flag`.
fn exec_start_flag(unit: &str, flag: &str) -> Option<String> {
    let argv = exec_start_argv(unit)?;
    let mut words = argv.split_whitespace();
    while let Some(word) = words.next() {
        if word == flag {
            return words.next().map(str::to_owned);
        }
    }
    None
}

/// Read the socket-group recipe out of `unit`.
///
/// The block runs from the line carrying [`RECIPE_OPENS`] to the first line that is
/// not a comment. Inside it, four shapes are recognised and everything else is prose
/// deliberately ignored — the block explains itself in sentences, and a parser that
/// tried to read those would be inventing a recipe:
///
/// * a line opening `sudo ` is a command (continuations joined at a trailing `\`);
/// * a line opening `stat ` is the verification command, read for its paths;
/// * a line that is *itself* a comment (`#   # serial-nexus …`) is the predicted
///   `stat` output;
/// * anything [`directive_at`] reads as `Name=value` is a replacement directive.
///
/// The block's `ExecStart=` line is deliberately **not** a directive: it is written
/// `ExecStart= ... --socket-group …`, with a space after the `=`, and `directive_at`
/// refuses exactly that shape. It is read for its flag instead, which is all a probe
/// can use — the ellipsis is not an argv.
fn socket_group_recipe(unit: &str) -> SocketGroupRecipe {
    let mut block: Vec<String> = Vec::new();
    let mut open = false;
    for line in unit.lines() {
        let Some(body) = comment_body(line) else {
            if open {
                break;
            }
            continue;
        };
        if !open {
            open = body.contains(RECIPE_OPENS);
            continue;
        }
        if let Some(prev) = block.last_mut()
            && let Some(head) = prev.strip_suffix('\\')
        {
            *prev = format!("{} {body}", head.trim_end());
            continue;
        }
        block.push(body.to_owned());
    }
    assert!(
        open,
        "packaging/serial-nexus-daemon.service has no `{RECIPE_OPENS}` line, so the \
         socket-group recipe this file parses, checks and executes is gone. An empty \
         recipe agrees with every probe, which is the tell AGENTS §3 names"
    );

    let mut out = SocketGroupRecipe {
        commands: Vec::new(),
        directives: Vec::new(),
        stat_paths: Vec::new(),
        predictions: Vec::new(),
        socket_group_flag: None,
    };
    for body in &block {
        if body.is_empty() {
            continue;
        }
        if let Some(pred) = body.strip_prefix('#') {
            out.predictions.push(pred.trim().to_owned());
            continue;
        }
        if let Some(rest) = body.strip_prefix("sudo ") {
            out.commands
                .push(rest.split_whitespace().map(str::to_owned).collect());
            continue;
        }
        if body.starts_with("stat ") {
            out.stat_paths.extend(
                body.split_whitespace()
                    .filter(|t| t.starts_with('/'))
                    .map(str::to_owned),
            );
            continue;
        }
        if let Some(name) = directive_at(body, 0) {
            let value = body[name.len() + 1..].trim().to_owned();
            out.directives.push((name, value));
            continue;
        }
        if let Some(at) = body.find("--socket-group") {
            let mut words = body[at..].split_whitespace();
            let flag = words.next().unwrap_or_default();
            if let Some(group) = words.next() {
                out.socket_group_flag = Some(format!("{flag} {group}"));
            }
        }
    }
    out
}

/// Shell metacharacters, refused in a recipe command.
///
/// The probes run these argv through [`Command`] with **no shell**, so a `&&` in the
/// block would be passed to `groupadd` as a literal argument rather than executed.
/// Refusing them is not defensive tidiness: it is the precondition that makes
/// "executed verbatim" true, and it is checked on every push rather than only where
/// root exists.
const SHELL_METACHARACTERS: &str = "&|;<>$`(){}[]*?!~\"'\\";

/// Read the recipe and hold it to what it says about itself, and to the unit around
/// it.
///
/// Most checks here are a *cross-check between two halves of the block* — the
/// directives against the commands, the predictions against the directives, the
/// widening flag against the group — and none of those pins a value, so the group's
/// name is the block's to choose.
///
/// **The exceptions are named rather than left to be discovered.** Three values are
/// pinned on purpose: `DynamicUser=no`, because any other value makes `User=`/`Group=`
/// inert and the whole block a no-op; and the two `stat` paths, which are held to
/// `RuntimeDirectory=` and to `ExecStart=`'s own `--socket` argument. Those two are
/// the repair of a real gap — the pair used to be checked only for *shape*, "a
/// directory and something inside it", which two invented paths satisfy exactly as
/// well as the real ones, so the recipe's verification step was tied to nothing
/// outside itself.
///
/// **What is still not checked, stated because the list above reads like coverage.**
/// Nothing here proves the daemon binds the socket at `stat_paths[1]`; that is
/// [`the_socket_group_recipe_widens_the_control_socket_to_the_operators_group`]'s
/// job, and it needs root. Off root this function reads two files' worth of text and
/// nothing else.
fn check_recipe(unit: &str) -> Result<SocketGroupRecipe, String> {
    let r = socket_group_recipe(unit);

    let Some(user) = r.directive("User") else {
        return Err("the recipe sets no `User=`, so it is not a static identity at all".into());
    };
    let Some(group) = r.directive("Group") else {
        return Err(
            "the recipe sets no `Group=`, so nothing gives the operators the \
                    runtime directory"
                .into(),
        );
    };
    let Some(mode) = r.directive("RuntimeDirectoryMode") else {
        return Err(
            "the recipe sets no `RuntimeDirectoryMode=`, so the directory it \
                    hands to the operators' group stays 0700 and the group still \
                    cannot traverse it"
                .into(),
        );
    };
    if r.directive("DynamicUser") != Some("no") {
        return Err(format!(
            "the recipe's `DynamicUser=` reads {:?}, not `no` — with the transient \
             identity still in force `User=`/`Group=` are ignored and the whole \
             recipe is a no-op",
            r.directive("DynamicUser")
        ));
    }
    if r.commands.len() < 2 {
        return Err(format!(
            "the recipe carries {} command(s); it must create both a group and a \
             user before the directives above can name them",
            r.commands.len()
        ));
    }
    for cmd in &r.commands {
        let Some(program) = cmd.first() else {
            return Err("the recipe carries an empty command".into());
        };
        for token in cmd {
            if let Some(bad) = token.chars().find(|c| SHELL_METACHARACTERS.contains(*c)) {
                return Err(format!(
                    "`{program}`'s argument `{token}` carries the shell \
                     metacharacter `{bad}`. The probe runs these argv with no shell, \
                     so that character would be passed as a literal argument rather \
                     than executed — the recipe would not be run verbatim, and the \
                     measurement would be of something else"
                ));
            }
        }
    }
    let tokens: BTreeSet<&str> = r.commands.iter().flatten().map(String::as_str).collect();
    for (what, name) in [("User=", user), ("Group=", group)] {
        if !tokens.contains(name) {
            return Err(format!(
                "the recipe's `{what}` names `{name}`, which none of its commands \
                 creates. The block would tell an operator to point the unit at an \
                 identity the block itself never makes"
            ));
        }
    }
    if r.predictions.len() != 2 || r.stat_paths.len() != 2 {
        return Err(format!(
            "the recipe's verification step names {} path(s) and predicts {} line(s); \
             it must predict exactly one line per path or a reader cannot tell which \
             prediction belongs to which",
            r.stat_paths.len(),
            r.predictions.len()
        ));
    }
    if !r.stat_paths[1].starts_with(&format!("{}/", r.stat_paths[0])) {
        return Err(format!(
            "the recipe stats `{}` and `{}`, which are not a directory and the socket \
             inside it. The whole point of the block is that the *directory* is the \
             half `SupplementaryGroups=` cannot buy, so the pair has to be that pair",
            r.stat_paths[0], r.stat_paths[1]
        ));
    }
    // …and the pair is held to the unit, not only to itself. Two paths invented out of
    // whole cloth pass the shape check above, so on its own it left the block's
    // verification step describing a directory systemd does not create and a socket the
    // daemon does not bind.
    let Some(runtime_dir) = active_value(unit, "RuntimeDirectory") else {
        return Err(
            "the unit sets no active `RuntimeDirectory=`, so nothing creates the \
             directory the recipe's `stat` reads and the block's premise — that \
             systemd chowns that directory to `User=`/`Group=` — has no subject"
                .to_owned(),
        );
    };
    let want_dir_path = format!("/run/{runtime_dir}");
    if r.stat_paths[0] != want_dir_path {
        return Err(format!(
            "the recipe verifies `{}` while the unit's `RuntimeDirectory={runtime_dir}` \
             makes systemd create `{want_dir_path}`. The block would tell an operator \
             to check the ownership of a directory this unit never creates",
            r.stat_paths[0]
        ));
    }
    let Some(exec_socket) = exec_start_flag(unit, "--socket") else {
        return Err(
            "the unit's active `ExecStart=` passes no `--socket`, so the path the \
             recipe's second `stat` reads is not the one this unit's daemon binds"
                .to_owned(),
        );
    };
    if r.stat_paths[1] != exec_socket {
        return Err(format!(
            "the recipe verifies the socket at `{}` while the unit's `ExecStart=` \
             binds `{exec_socket}`. The mode and group the block predicts are the \
             daemon's doing, so a path the daemon never binds is a prediction about \
             nothing",
            r.stat_paths[1]
        ));
    }
    let want_dir = format!("{user} {group} {}", mode.trim_start_matches('0'));
    if r.predictions[0] != want_dir {
        return Err(format!(
            "the recipe predicts `{}` for `{}` while its own directives say \
             `{want_dir}`. One of the two moved without the other",
            r.predictions[0], r.stat_paths[0]
        ));
    }
    let socket: Vec<&str> = r.predictions[1].split_whitespace().collect();
    if socket.len() != 3 || socket[0] != user || socket[1] != group {
        return Err(format!(
            "the recipe predicts `{}` for the socket while its identity is \
             `{user}`/`{group}` — the socket and the directory it lives in cannot \
             belong to different identities under one unit",
            r.predictions[1]
        ));
    }
    match r.socket_group_flag.as_deref() {
        Some(flag) if flag == format!("--socket-group {group}") => {}
        other => {
            return Err(format!(
                "the recipe's `ExecStart=` line widens the socket with {other:?}, not \
                 `--socket-group {group}`. Without that flag the daemon narrows its \
                 own socket back to 0600 and the predicted `{}` never happens, \
                 whatever the directory says",
                r.predictions[1]
            ));
        }
    }
    Ok(r)
}

/// `unit` with the socket-group recipe applied — the file an operator following the
/// block would end up with.
///
/// Derived rather than written: each recipe directive replaces the active directive
/// of the same name, the ones the shipped unit does not set are emitted where the
/// `DynamicUser=` line was, and the widening flag is appended to the last
/// continuation line of `ExecStart=`. A hand-written "after" file would verify a unit
/// no operator produces.
fn unit_with_recipe(unit: &str, recipe: &SocketGroupRecipe) -> String {
    let active = active_directives(unit);
    let mut out = String::with_capacity(unit.len() + 256);
    let mut in_exec = false;
    for line in unit.lines() {
        if in_exec && !line.trim_end().ends_with('\\') {
            out.push_str(line);
            if let Some(flag) = &recipe.socket_group_flag {
                out.push_str(" \\\n    ");
                out.push_str(flag);
            }
            out.push('\n');
            in_exec = false;
            continue;
        }
        if let Some(name) = directive_at(line, 0) {
            if name == "ExecStart" {
                in_exec = line.trim_end().ends_with('\\');
                if !in_exec && let Some(flag) = &recipe.socket_group_flag {
                    out.push_str(line);
                    out.push(' ');
                    out.push_str(flag);
                    out.push('\n');
                    continue;
                }
            } else if let Some(value) = recipe.directive(&name) {
                out.push_str(&format!("{name}={value}\n"));
                if name == "DynamicUser" {
                    for (n, v) in &recipe.directives {
                        if !active.contains(n) {
                            out.push_str(&format!("{n}={v}\n"));
                        }
                    }
                }
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The first *source line inside the socket-group recipe block* whose comment body
/// contains `needle`.
///
/// **Why the plants below need this.** Each plant rewrites one line of the shipped
/// block, and until 2026-08-21 each named that line as a literal — `console-operators`
/// and all. That made the group's name a value the gate pinned in eight places while
/// [`check_recipe`]'s own doc promised the opposite, and it was measured: renaming the
/// group *consistently* in the unit (all eight occurrences, nothing left disagreeing)
/// failed this test with `the plant … found no `#   Group=console-operators` to
/// rewrite`. A correct edit refused, by the machinery written to catch incorrect ones.
///
/// A bare needle against the whole file is not enough either: the prose above the
/// block names the group too, and a plant that lands in prose changes no reading and
/// reddens nothing — which reads exactly like a gate that does not bite. Anchoring to
/// a line *inside* the block, and rewriting that whole line, is what makes the plants
/// derivable and still load-bearing.
fn recipe_line<'a>(unit: &'a str, needle: &str) -> Option<&'a str> {
    let mut open = false;
    for line in unit.lines() {
        let Some(body) = comment_body(line) else {
            if open {
                break;
            }
            continue;
        };
        if !open {
            open = body.contains(RECIPE_OPENS);
            continue;
        }
        if body.contains(needle) {
            return Some(line);
        }
    }
    None
}

/// `unit` with the first occurrence of `from` rewritten to `to`, and the fact that it
/// landed.
///
/// Returned rather than asserted inside, because a plant that did not land is the
/// defect plan §3 rule 17(b) names: the run below then judges the clean file and
/// reads exactly like `the gate did not redden`.
fn plant_text(unit: &str, from: &str, to: &str) -> (String, bool) {
    match unit.find(from) {
        Some(at) => {
            let mut planted = String::with_capacity(unit.len());
            planted.push_str(&unit[..at]);
            planted.push_str(to);
            planted.push_str(&unit[at + from.len()..]);
            (planted, true)
        }
        None => (unit.to_owned(), false),
    }
}

#[test]
fn the_socket_group_recipe_agrees_with_itself() {
    let me = "the_socket_group_recipe_agrees_with_itself";
    let unit = read_tree_file(UNIT);

    let recipe = check_recipe(&unit).unwrap_or_else(|why| {
        panic!(
            "packaging/serial-nexus-daemon.service's socket-group recipe contradicts \
             itself: {why}.\nThe root probes below *execute* this block, so a recipe \
             that disagrees with its own predictions is a measurement of nothing"
        )
    });

    // The recipe is what the probes execute, so its floors are the probes' floors.
    assert!(
        recipe.commands.len() >= 2 && recipe.directives.len() >= 4 && recipe.predictions.len() == 2,
        "the recipe parsed to {} command(s), {} directive(s) and {} prediction(s) — \
         too thin to be the block that ships, so the parser has stopped seeing it",
        recipe.commands.len(),
        recipe.directives.len(),
        recipe.predictions.len()
    );

    // …and the checks bite, in every half they claim (plan §3 rule 10). Each plant
    // asserts its own landing first: a plant that changes nothing proves nothing,
    // which is a mistake this session made twice before it was written down here.
    //
    // **Every plant is derived from the recipe the parser just read**, and that is the
    // repair, not a tidy-up. Written as literals they pinned `console-operators` in
    // eight places, so a *consistent* rename of the group — the edit `check_recipe`'s
    // own doc comment promised stays cheap — was refused by this test with `the plant
    // … found no `#   Group=console-operators` to rewrite`, measured 2026-08-21 on the
    // unit with all eight occurrences renamed — eight, counted with
    // `grep -o console-operators packaging/serial-nexus-daemon.service | wc -l`, a
    // number both this comment and the one on [`recipe_line`] carried as seven until it
    // was counted — the same session's own record already said eight. See
    // [`recipe_line`] for why the needle is anchored inside the block rather than
    // matched against the whole file.
    // The three values the plants are built from, taken from the block the parser just
    // read rather than typed here. `check_recipe` has already refused a recipe missing
    // any of them, so the `expect`s below are that guarantee restated, not a hope.
    let group = recipe
        .directive("Group")
        .expect("check_recipe requires `Group=`");
    let mode = recipe
        .directive("RuntimeDirectoryMode")
        .expect("check_recipe requires `RuntimeDirectoryMode=`");
    let dynamic = format!(
        "DynamicUser={}",
        recipe
            .directive("DynamicUser")
            .expect("check_recipe requires `DynamicUser=no`")
    );
    let flag_text = recipe
        .socket_group_flag
        .clone()
        .expect("check_recipe requires the widening flag");
    // The unit's own `RuntimeDirectory=`, which is the *other* half of the tie the
    // recipe's first `stat` is held to. `check_recipe` refuses a unit that sets none,
    // so this `expect` is that refusal restated rather than a hope.
    let runtime_directive = format!(
        "RuntimeDirectory={}",
        active_value(&unit, "RuntimeDirectory")
            .expect("check_recipe requires an active `RuntimeDirectory=`")
    );
    let r = &recipe;
    let line = |needle: &str, what: &str| -> String {
        recipe_line(&unit, needle)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                panic!(
                    "the socket-group recipe block has no line carrying `{needle}` \
                 ({what}), so the plant below could not be built from the shipped \
                 file. Either the block moved or the parser above is reading \
                 something the plants are not"
                )
            })
    };
    // A mode the recipe does not use, so the two mode plants always change something.
    let other_mode = if mode.trim_start_matches('0') == "700" {
        "0750"
    } else {
        "0700"
    };
    let swap_last = |line: &str, to: &str| -> String {
        let trimmed = line.trim_end();
        let (head, _) = trimmed
            .rsplit_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        format!("{head} {to}")
    };
    let group_directive = line(&format!("Group={group}"), "the recipe's `Group=`");
    let groupadd = line("groupadd", "the recipe's group-creating command");
    let useradd_tail = line(&format!("--gid {group}"), "the recipe's `useradd` identity");
    let dir_prediction = line(&r.predictions[0], "the recipe's predicted directory stat");
    let mode_directive = line(
        &format!("RuntimeDirectoryMode={mode}"),
        "the recipe's `RuntimeDirectoryMode=`",
    );
    let dynamic_user = line(&dynamic, "the recipe's `DynamicUser=`");
    let exec_start = line(&flag_text, "the recipe's widening `ExecStart=`");
    let stat_dir = line(&r.stat_paths[0], "the recipe's verification `stat`");
    let stat_sock = line(&r.stat_paths[1], "the recipe's socket `stat` path");
    let plants: [(&str, String, String); 11] = [
        (
            "the directives naming a group the commands never create",
            group_directive.clone(),
            group_directive.replace(
                &format!("Group={group}"),
                &format!("Group={group}-snx-uncreated"),
            ),
        ),
        (
            "the group-creating command dropped from the recipe",
            groupadd.clone(),
            "#   (create the group however you like)".to_owned(),
        ),
        (
            "the commands creating a user the directives never name",
            useradd_tail.clone(),
            swap_last(&useradd_tail, "snx-other-user"),
        ),
        (
            "a predicted mode the directives do not produce",
            dir_prediction.clone(),
            swap_last(&dir_prediction, other_mode.trim_start_matches('0')),
        ),
        (
            "a directory mode the predictions do not expect",
            mode_directive.clone(),
            mode_directive.replace(
                &format!("RuntimeDirectoryMode={mode}"),
                &format!("RuntimeDirectoryMode={other_mode}"),
            ),
        ),
        (
            "the transient identity left in force",
            dynamic_user.clone(),
            dynamic_user.replace(&dynamic, "DynamicUser=yes"),
        ),
        (
            "the widening flag dropped from ExecStart=",
            exec_start.clone(),
            exec_start.replace(&flag_text, "(unchanged)"),
        ),
        (
            "a shell metacharacter in a command the probe runs without a shell",
            groupadd.clone(),
            format!("{groupadd} && id"),
        ),
        (
            "a verification stat naming a directory this unit never creates",
            stat_dir.clone(),
            stat_dir.replace(&r.stat_paths[0], "/run/snx-not-the-runtime-directory"),
        ),
        // **And the same tie from the unit's side, because the plant above does not
        // reach it.** Moving only the recipe's first `stat` path leaves its *second*
        // one — the socket — still spelled under the old directory, so `check_recipe`
        // refuses at the earlier shape check ("not a directory and the socket inside
        // it") and returns before the `RuntimeDirectory=` comparison is evaluated at
        // all. Measured 2026-08-21: with `if false &&` disabling that comparison, this
        // test stayed green and still printed `10 plants … each reddened`, which is a
        // new check arriving with no plant of its own. Moving the *unit's* directive
        // instead leaves the recipe untouched, so every check before the tie passes
        // and the tie is the one that answers.
        (
            "the unit's RuntimeDirectory= moved and the recipe's stat did not follow",
            runtime_directive.clone(),
            format!("{runtime_directive}-snx-elsewhere"),
        ),
        (
            "a verification stat naming a socket this unit's daemon never binds",
            stat_sock.clone(),
            stat_sock.replace(
                &r.stat_paths[1],
                &format!("{}/snx-not-the-socket.sock", r.stat_paths[0]),
            ),
        ),
    ];
    for (what, from, to) in &plants {
        let (from, to) = (from.as_str(), to.as_str());
        assert_ne!(
            from, to,
            "the plant for {what} rewrites its line to itself, so it changes nothing \
             and its green is indistinguishable from not running"
        );
        let (planted, landed) = plant_text(&unit, from, to);
        assert!(
            landed,
            "the plant for {what} found no `{from}` to rewrite, so the check below \
             reads the clean unit and its green is indistinguishable from not running \
             (plan §3 rule 17(b))"
        );
        let err = check_recipe(&planted).err().unwrap_or_else(|| {
            panic!(
                "planting {what} into the socket-group recipe did not redden this \
                 gate. The block is counted as checked and is not, and the root \
                 probes would execute the contradiction"
            )
        });
        assert!(
            !err.trim().is_empty(),
            "planting {what} reddened with an empty reason"
        );
    }

    // The recipe-applied unit, which section (8)'s validator check verifies and the
    // root probes' properties mirror. Derived here so a drift in either shows up on
    // every push rather than only where systemd exists.
    let applied = unit_with_recipe(&unit, &recipe);
    assert_ne!(
        applied, unit,
        "applying the recipe to the unit changed nothing, so the acceptance check \
         below verifies the shipped unit twice"
    );
    let applied_actives = active_directives(&applied);
    for (name, value) in &recipe.directives {
        if name == "ExecStart" {
            continue;
        }
        assert!(
            applied_actives.contains(name),
            "`{name}=` is in the recipe and not in the unit the recipe produces"
        );
        let want = format!("{name}={value}");
        assert!(
            applied.lines().any(|l| l == want),
            "the recipe-applied unit does not set `{want}` as an active directive, so \
             the operator following this block and the probe measuring it are looking \
             at different files"
        );
    }
    assert!(
        !applied.contains("\nDynamicUser=yes"),
        "the recipe-applied unit still sets `DynamicUser=yes`, so `User=`/`Group=` \
         are ignored in it and the acceptance check below verifies the wrong unit"
    );
    let flag = recipe
        .socket_group_flag
        .as_deref()
        .expect("check_recipe accepted a recipe with no widening flag");
    assert_eq!(
        applied.matches(flag).count(),
        2,
        "the recipe-applied unit carries `{flag}` {} time(s) — it belongs in exactly \
         two places, the comment block it came from and the ExecStart= it was \
         appended to",
        applied.matches(flag).count()
    );

    eprintln!(
        "MEASURED {me}: recipe = {} command(s), {} directive(s), predictions {:?}, \
         widening {flag:?}; {} plants, each derived from that recipe and each \
         reddened",
        recipe.commands.len(),
        recipe.directives.len(),
        recipe.predictions,
        plants.len()
    );
}

#[test]
fn the_socket_group_recipe_verifies_clean_under_systemd_analyze() {
    let me = "the_socket_group_recipe_verifies_clean_under_systemd_analyze";
    let Some(help) = run_tool("systemd-analyze", &["verify".into(), "--help".into()]) else {
        skip_no_packaging(me, "systemd-analyze not found on PATH");
        return;
    };
    let flags = verify_flags(&help.stdout);
    let unit = read_tree_file(UNIT);
    let recipe = check_recipe(&unit).expect("the recipe's own consistency is checked above");
    let applied = unit_with_recipe(&unit, &recipe);

    let scratch = Scratch::new("recipe");
    let (staged, count) = stage_unit(&scratch, &applied, "recipe.service");
    assert_eq!(
        count, 1,
        "the ExecStart stub substitution matched {count} time(s) in the \
         recipe-applied unit, not once — the staged file is not the one this gate \
         believes it verified"
    );

    let verify = |path: &Path| -> Run {
        let mut args = vec!["verify".to_owned()];
        args.extend(flags.iter().cloned());
        args.push(path.display().to_string());
        run_tool("systemd-analyze", &args).expect("systemd-analyze was found a moment ago")
    };
    let clean = verify(&staged);
    verdict(&clean).unwrap_or_else(|e| {
        panic!(
            "the unit an operator gets by following the socket-group recipe does not \
             verify clean under `systemd-analyze verify {}`. The recipe is what \
             packaging/README.md's evidence table calls its **acceptance**; a recipe \
             systemd will not parse is a recipe nobody can run.\n{e}",
            flags.join(" ")
        )
    });

    // The bound, measured rather than assumed and stated where a reader will look
    // for it: acceptance is not existence. `User=` and `Group=` name an identity
    // this box does not have, and the validator says nothing about that — the same
    // blind spot the shipped unit's `SupplementaryGroups=` probe records one section
    // up. Whether the identity can be *created*, and what the recipe then produces,
    // is the root arm's to answer.
    let plants: [(&str, String); 2] = [
        (
            "an unparseable value on the recipe's own mode",
            applied.replace("RuntimeDirectoryMode=0750", "RuntimeDirectoryMode=notamode"),
        ),
        (
            "an unknown directive beside the recipe's identity",
            applied.replace("DynamicUser=no", "DynamicUser=no\nStaticUser=yes"),
        ),
    ];
    for (what, planted) in &plants {
        assert_ne!(
            planted, &applied,
            "the plant for {what} changed nothing, so the run below verifies the \
             clean recipe unit (plan §3 rule 17(b))"
        );
        let (staged_plant, applied_count) = stage_unit(&scratch, planted, "recipe-planted.service");
        assert_eq!(
            applied_count, 1,
            "the planted recipe unit did not stage, so its verdict is the \
             environmental arm rather than the plant"
        );
        let run = verify(&staged_plant);
        verdict(&run).expect_err(&format!(
            "planting {what} into the recipe-applied unit did not redden this gate"
        ));
    }

    eprintln!(
        "MEASURED {me}: the recipe-applied unit verifies clean under \
         `systemd-analyze verify {}` (exit {:?}, empty stderr); 2 plants reddened. \
         Acceptance only — `User=`/`Group=` name an identity this box need not have.",
        flags.join(" "),
        clean.status
    );
}

// ---------------------------------------------------------------------------
// (8) Root probe machinery: staging, identities, payloads
// ---------------------------------------------------------------------------

/// The root arm's preconditions, or the reason this box fails one.
///
/// Extracted verbatim from
/// [`dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown`],
/// which had them inline when it was the only root-gated check here. Five checks now
/// share them, and five copies of a skip reason is five chances for one of them to
/// go stale while still reading like it was checked.
fn packaging_root_precondition() -> Result<(), String> {
    let Some(comm) = pid1_comm() else {
        return Err("no /proc/1/comm on this platform, so no systemd".to_owned());
    };
    if comm != "systemd" {
        return Err(format!(
            "PID 1 is `{comm}`, not systemd — nothing here can start a unit"
        ));
    }
    let Some(uid) = effective_uid() else {
        return Err("could not read /proc/self/status to learn the effective uid".to_owned());
    };
    if uid != 0 {
        return Err(format!(
            "effective uid is {uid}, not 0. Without root, `systemd-run` answers \
             `Access denied … requires interactive authentication` (polkit), and the \
             rootless fallback is closed too: `unshare -Ur` is refused with `write \
             failed /proc/self/uid_map: Operation not permitted`"
        ));
    }
    if run_tool("systemd-run", &["--version".into()]).is_none() {
        return Err("systemd-run not found on PATH".to_owned());
    }
    Ok(())
}

/// A workspace binary beside the running test executable, or `None`.
///
/// `cargo test` builds only the instrumented binaries under `target/<profile>/deps/`,
/// so the plain artifact exists exactly when something ran `cargo build` first — CI's
/// `packaging` job now does, for these probes. `None` is a *skip reason*, never a
/// panic: a box that has not built the workspace has failed a precondition, and a
/// precondition failure that reads like a defect is the thing plan §3 rule 11 exists
/// to stop.
fn workspace_bin(name: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_exe().ok()?;
    dir.pop();
    if dir.file_name().is_some_and(|n| n == "deps") {
        dir.pop();
    }
    let exe = dir.join(name);
    exe.exists().then_some(exe)
}

/// Everything a daemon-bearing probe puts on this box, removed however the test ends.
///
/// **`Drop`, not a cleanup block, and that is a deliberate change from the probe one
/// section up.** That one cleans up *before* asserting so a red gate leaves nothing
/// behind — the right instinct, written the only way it could be written for a single
/// linear test. With four probes it becomes four places to forget, and it still loses
/// the state when an `assert!` earlier in the test fires. Unwinding runs `Drop`.
///
/// The binaries go under `/usr/local/lib/<tag>/` rather than the README's
/// `/usr/local/bin/`, and the config under `/etc/<tag>/` rather than
/// `/etc/serial-nexus-daemon/`: a probe must never overwrite a real install, and the
/// claim under test is what the *sandbox* does, which no path in a read-only tree
/// changes. `ProtectSystem=strict` leaves both readable and executable — that is what
/// "read-only" means, and it is why the daemon can be exec'd from there at all.
struct RootStage {
    tag: String,
    bin_dir: PathBuf,
    etc_dir: PathBuf,
}

impl RootStage {
    fn new(tag: &str) -> Self {
        RootStage {
            tag: tag.to_owned(),
            bin_dir: PathBuf::from("/usr/local/lib").join(tag),
            etc_dir: PathBuf::from("/etc").join(tag),
        }
    }

    /// Copy the built daemon and CLI into the stage, or say which is missing.
    fn stage_binaries(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.bin_dir)
            .map_err(|e| format!("create {}: {e}", self.bin_dir.display()))?;
        std::fs::create_dir_all(&self.etc_dir)
            .map_err(|e| format!("create {}: {e}", self.etc_dir.display()))?;
        for name in ["serial-nexus-daemon", "serial-nexus-ctl"] {
            let src = workspace_bin(name).ok_or_else(|| {
                format!(
                    "no built `{name}` beside this test binary — run \
                     `cargo build --workspace` first (`cargo test` builds only the \
                     deps/ test binaries, never the plain artifact a service execs)"
                )
            })?;
            let dst = self.bin_dir.join(name);
            std::fs::copy(&src, &dst).map_err(|e| format!("copy {name} into the stage: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("make the staged {name} executable: {e}"))?;
            }
        }
        Ok(())
    }

    /// Write the probe's first-boot seed: one pty node, whose symlink lands in the
    /// unit's own `RuntimeDirectory=`.
    ///
    /// A pty node rather than the shipped example config on purpose. The example
    /// names serial identities no runner has and a log directory the probe renames,
    /// so every node in it would come up `faulted` — a daemon that starts and reports
    /// nothing but faults cannot distinguish "the sandbox let it work" from "the
    /// sandbox let it fail politely". A pty node needs `/dev/ptmx` and `char-pts`,
    /// which are two of the unit's own `DeviceAllow=` lines, so it exercises the
    /// device policy rather than dodging it.
    fn write_config(&self, node: &str) -> PathBuf {
        let path = self.etc_dir.join("config.toml");
        let text = format!(
            "[[node]]\ntype = \"pty\"\nname = \"{node}\"\npath = \"/run/{}/{node}\"\n",
            self.tag
        );
        std::fs::write(&path, text).expect("write the probe's seed configuration");
        path
    }
}

impl Drop for RootStage {
    fn drop(&mut self) {
        let tag = &self.tag;
        let _ = std::fs::remove_dir_all(&self.bin_dir);
        let _ = std::fs::remove_dir_all(&self.etc_dir);
        // `/var/lib/<tag>` is a symlink into private/ under DynamicUser= and a real
        // directory under the static recipe, so both removals are attempted and
        // neither is required to succeed.
        let _ = std::fs::remove_file(format!("/var/lib/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/var/lib/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/var/lib/private/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/var/lib/{tag}-old"));
        let _ = std::fs::remove_file(format!("/var/log/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/var/log/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/var/log/private/{tag}"));
        let _ = std::fs::remove_dir_all(format!("/run/{tag}"));
        // The `PrivateTmp=` sentinel, which [`run_probe`] plants on the *host* for
        // every probe it runs — the `stat`-only ones that never start a daemon
        // included, since the plant is in the one function they all go through. Both
        // clauses this comment used to carry were false: it named `daemon_payload` as
        // the planter (that function only *reads* the file, from inside the unit) and
        // said a probe that never started the daemon planted none. A stage whose test
        // skipped before reaching any probe still leaves nothing to remove, and
        // removing a file that is not there is the same no-op as the two
        // `/var/lib/<tag>` shapes above.
        let _ = std::fs::remove_file(sentinel_path(tag));
    }
}

/// Serialises the two probes that create the recipe's system identity.
///
/// CI's root step passes `--test-threads=1`, so this changes nothing there. It is for
/// the operator who runs the binary by hand without it: two probes creating one
/// `groupadd`ed group at the same time is a race whose loser reports a recipe defect.
static RECIPE_IDENTITY: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Why the recipe's identity could not be created — and the distinction is the whole
/// point of the type.
///
/// A box that already carries the name, or has no `groupadd`, has failed a
/// *precondition*: the probe skips, naming it, and `SNX_PACKAGING_ROOT=required` turns
/// that into a hard failure where a lane claims the capability. A `groupadd` that runs
/// and *fails* is the **recipe** failing, which is a finding about the shipped block
/// and must never be reported as a skip.
enum IdentityError {
    Precondition(String),
    Recipe(String),
}

/// The system identity the recipe creates, deleted however the test ends.
struct RecipeIdentity {
    user: String,
    group: String,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// Whether `name` is already a user or group on this box.
///
/// Read from `/etc/passwd` and `/etc/group` plus `id`, and the bound is worth stating:
/// an identity that exists only in NSS (LDAP, sssd) and in no file is not seen by the
/// first two, which is why `id` is asked as well. What this cannot do is prove
/// absence on a directory-backed box — and the consequence of being wrong is that the
/// probe *creates* a name that resolves elsewhere, so it errs by refusing rather than
/// by adopting.
fn identity_taken(user: &str, group: &str) -> Option<String> {
    let has_line = |path: &str, name: &str| {
        std::fs::read_to_string(path)
            .map(|t| t.lines().any(|l| l.starts_with(&format!("{name}:"))))
            .unwrap_or(false)
    };
    if has_line("/etc/passwd", user) {
        return Some(format!("`{user}` is already in /etc/passwd"));
    }
    if has_line("/etc/group", group) {
        return Some(format!("`{group}` is already in /etc/group"));
    }
    if run_tool("id", &["-u".into(), user.to_owned()]).is_some_and(|r| r.ok()) {
        return Some(format!("`{user}` already resolves through `id`"));
    }
    None
}

impl RecipeIdentity {
    /// Run the recipe's own commands, verbatim but for the `sudo` this process does
    /// not need, or return what went wrong.
    fn create(recipe: &SocketGroupRecipe, user: &str, group: &str) -> Result<Self, IdentityError> {
        let guard = RECIPE_IDENTITY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(taken) = identity_taken(user, group) {
            return Err(IdentityError::Precondition(format!(
                "{taken}. This probe creates and then deletes the identity the unit's \
                 recipe names; it will not adopt one it did not make, and it will \
                 certainly not delete one"
            )));
        }
        let me = RecipeIdentity {
            user: user.to_owned(),
            group: group.to_owned(),
            _guard: guard,
        };
        for cmd in &recipe.commands {
            let (program, args) = cmd
                .split_first()
                .expect("check_recipe refuses an empty command");
            let run = run_tool(program, args).ok_or_else(|| {
                IdentityError::Precondition(format!(
                    "`{program}` is not on PATH, so the recipe cannot be run here"
                ))
            })?;
            if !run.ok() {
                return Err(IdentityError::Recipe(format!(
                    "the recipe's own command `{}` exited {:?}. This is the recipe \
                     failing, not the probe: it is run verbatim but for the `sudo`.\n\
                     stdout: {}\nstderr: {}",
                    cmd.join(" "),
                    run.status,
                    run.stdout.trim(),
                    run.stderr.trim()
                )));
            }
        }
        Ok(me)
    }

    /// `id -gn` and `id -Gn` for the identity the recipe just made.
    fn readback(&self) -> Option<(String, String)> {
        let primary = run_tool("id", &["-gn".into(), self.user.clone()])?;
        let all = run_tool("id", &["-Gn".into(), self.user.clone()])?;
        (primary.ok() && all.ok()).then(|| {
            (
                primary.stdout.trim().to_owned(),
                all.stdout.trim().to_owned(),
            )
        })
    }
}

impl Drop for RecipeIdentity {
    fn drop(&mut self) {
        // The recipe spells no removal — it is a setup recipe — so the probe names
        // the counterparts of the two programs it ran. Both are best-effort: a
        // failure here must not turn a green measurement red, and it is reported
        // rather than swallowed so a box that accumulates probe identities says so.
        for (program, arg) in [("userdel", &self.user), ("groupdel", &self.group)] {
            let why = match run_tool(program, std::slice::from_ref(arg)) {
                None => "it is not on PATH".to_owned(),
                Some(run) if run.ok() => continue,
                Some(run) => format!("exit {:?}: {}", run.status, run.stderr.trim()),
            };
            eprintln!(
                "NOTE p8_packaging: `{program} {arg}` did not succeed ({why}); this \
                 box may be left carrying the probe's identity"
            );
        }
    }
}

/// Set `Name=value` among `props`, replacing an existing assignment of that name.
///
/// Only ever used for names the shipped unit sets *once*. `DeviceAllow=` is set four
/// times and is deliberately never passed here: replacing the first of four and
/// leaving three would build a sandbox no file describes.
fn set_property(props: &mut Vec<String>, name: &str, value: &str) {
    for i in 0..props.len().saturating_sub(1) {
        if props[i] == "-p" && directive_at(&props[i + 1], 0).as_deref() == Some(name) {
            props[i + 1] = format!("{name}={value}");
            return;
        }
    }
    props.push("-p".to_owned());
    props.push(format!("{name}={value}"));
}

/// The packaged `[Service]` properties with the socket-group recipe applied.
fn recipe_properties(unit: &str, recipe: &SocketGroupRecipe, tag: &str) -> Vec<String> {
    let mut props = service_properties(unit, tag);
    for (name, value) in &recipe.directives {
        // The recipe's `ExecStart=` line is an ellipsis, not an argv; its flag
        // reaches the probe through the command line instead.
        if name == "ExecStart" {
            continue;
        }
        set_property(&mut props, name, value);
    }
    props
}

/// `systemd-run` arguments for one probe of `props`, running `payload` under `/bin/sh`.
fn probe_args(tag: &str, props: &[String], payload: &str) -> Vec<String> {
    let mut args = vec![
        "--wait".to_owned(),
        "--pipe".to_owned(),
        "--collect".to_owned(),
        format!("--unit={tag}"),
    ];
    args.extend(props.iter().cloned());
    args.push("/bin/sh".to_owned());
    args.push("-c".to_owned());
    args.push(payload.to_owned());
    args
}

/// The host path of the `PrivateTmp=` positive control for `tag`.
///
/// One spelling, three readers: [`run_probe`] plants it, [`assert_daemon_served`]
/// requires it to still be there before it reads `hidden` as a mount namespace, and
/// [`RootStage`]'s `Drop` removes it.
fn sentinel_path(tag: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/{tag}.sentinel"))
}

/// Run one probe and return its readings beside the raw streams.
///
/// **It plants the `PrivateTmp=` sentinel first, and that placement is the point.**
/// The file has to exist on the host *before* the unit starts or the probe's `hidden`
/// means nothing, and a caller that forgot would get `hidden` for free — the vacuity,
/// one level up. Planting it in the one function every probe must go through is what
/// makes forgetting impossible; [`RootStage`]'s `Drop` removes it. Probes that do not
/// read it (the `stat`-only ones) simply ignore it.
fn run_probe(tag: &str, props: &[String], payload: &str) -> (Run, BTreeMap<String, String>) {
    let sentinel = sentinel_path(tag);
    std::fs::write(&sentinel, tag).unwrap_or_else(|e| {
        panic!(
            "could not write the `PrivateTmp=` sentinel at {}: {e}. Without the file \
             on the host a probe reads `hidden` whatever the sandbox did, which is \
             exactly the vacuity the reading exists to close",
            sentinel.display()
        )
    });
    assert!(
        sentinel.exists(),
        "the `PrivateTmp=` sentinel at {} is not there after being written, so the \
         `hidden` a probe is about to report would mean nothing",
        sentinel.display()
    );
    let args = probe_args(tag, props, payload);
    let out = Command::new("systemd-run")
        .args(&args)
        .output()
        .expect("systemd-run answered --version a moment ago");
    let run = Run {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    let readings = probe_readings(&run.stdout);
    (run, readings)
}

/// Fail with the status word read before anything is attributed to the sandbox.
///
/// The same separation the probe one section up had to learn on CI: `226`/`EXIT_NAMESPACE`
/// is systemd failing to build the mount namespace and points at the *probe's* paths;
/// `216`/`EXIT_GROUP` at an identity that does not exist; `203`/`EXIT_EXEC` at a
/// binary the sandbox cannot reach — which is what `ProtectHome=yes` does to anything
/// left in a checkout under `/home`, and the reason [`RootStage`] copies out of it.
fn assert_probe_ran(me: &str, run: &Run, what: &str) {
    assert!(
        run.ok(),
        "{me}: {what} did not run (exit {:?}).\n**Read the systemd status word before \
         attributing this to the packaged sandbox.** 226/EXIT_NAMESPACE points at the \
         probe's own paths and mounts, 216/EXIT_GROUP at an identity that does not \
         exist, 203/EXIT_EXEC at a binary the sandbox cannot reach (`/home` is one, \
         under ProtectHome=yes), and the usual exit codes at the payload.\n\
         stdout:\n{}\nstderr:\n{}",
        run.status,
        run.stdout,
        run.stderr
    );
}

/// One reading, or a panic naming the probe that did not print it.
fn reading<'a>(readings: &'a BTreeMap<String, String>, key: &str, stdout: &str) -> &'a str {
    readings
        .get(key)
        .map(String::as_str)
        .unwrap_or_else(|| panic!("the probe printed no `{key}` reading:\n{stdout}"))
}

/// The probe payload that starts the packaged daemon *inside* the sandbox, asks it
/// for state over its own control socket, and shuts it down.
///
/// Every line prints one `key=value` reading, because a probe that dies partway must
/// be distinguishable from one that answered — the reading count is asserted before
/// any reading is read.
///
/// **Two anti-vacuity readings, and only one of them can stand alone.**
///
/// They are here because of a measured near-miss. The first attempt at this
/// measurement ran the daemon under the same hardening block through a *user* manager
/// (`systemd-run --user`), on a developer box, and reported success: `Finished with
/// result: success`, the daemon serving, a pty allocated. It proved nothing.
/// `/proc/self/mountinfo` read **53 lines inside the unit and 53 outside**, `/tmp` was
/// the host's, and `/home` was visible — the manager had applied no mount namespace at
/// all and had said nothing about it, so `ProtectHome=`, `PrivateTmp=` and
/// `ProtectSystem=` were silently no-ops (measured on Ubuntu, kernel 7.0.0-30, systemd
/// 259, 2026-08-21; three of the nineteen — `ProtectKernelModules=`,
/// `ProtectKernelLogs=`, `ProtectClock=` — do fail loudly there, with
/// `218/CAPABILITIES`, which is what made the silence of the rest so easy to miss). A
/// sandbox that did not apply and a sandbox that applied cleanly produce the same
/// green: AGENTS §3's tell, wearing a systemd unit.
///
/// `tmp_sentinel` is the reading that closes it, and it is a **positive control** —
/// but only with both of its halves. [`run_probe`] writes `/tmp/<tag>.sentinel` on
/// the host immediately before starting the unit, and [`assert_daemon_served`]
/// requires that file to still be there before it reads `hidden` as a namespace. The
/// second half is the one that was missing until 2026-08-21, and the reading does not
/// carry the claim without it: an **absent** sentinel reads `hidden` too, so on its
/// own `hidden` cannot separate a private `/tmp` from a planter that never ran.
/// Measured on the box of record, 2026-08-21: a `-p PrivateTmp=yes` unit under this
/// box's *user* manager — which builds no mount namespace here at all, as the
/// paragraph below records — printed `tmp_sentinel=hidden` with nothing planted and
/// `tmp_sentinel=visible` with the same file planted. The plant is in the one
/// function every probe goes through rather than at the call sites, because a caller
/// that forgot would get `hidden` for free, which is the same vacuity one level up.
///
/// `home_entries` corroborates and **cannot** stand alone. `ls /home 2>/dev/null |
/// wc -l` prints `0` for an empty `/home` and `0` for no `/home` at all — both
/// measured on the box of record, 2026-08-21 — so on such a box it reads 0 with no
/// namespace applied. It is kept because it is the reading that caught the near-miss
/// (this box's `/home` has 2 entries) and because two readings disagreeing is worth
/// seeing, not because it proves the namespace on its own.
///
/// **The negative arm is measured; the positive arm runs only where root does.** On
/// the box of record a user-manager unit carrying `-p PrivateTmp=yes -p
/// ProtectHome=yes -p ProtectSystem=strict` read `home_entries=2`,
/// `tmp_sentinel=visible` and 53 mountinfo lines against the host's 53 — no namespace,
/// and both readings say so; a second run adding `-p PrivateUsers=yes` still read
/// `tmp_sentinel=visible` and 53. (The three directives that *do* fail loudly under
/// that manager were re-measured the same day, one per unit: `ProtectKernelModules=`,
/// `ProtectKernelLogs=` and `ProtectClock=` each exit `218/CAPABILITIES`, while
/// `ProtectKernelTunables=`, `ProtectControlGroups=`, `ProtectHostname=`,
/// `RestrictNamespaces=` and `ProtectProc=invisible` all report success.) The
/// *passing* arm cannot be reproduced off root here, because `unshare --mount
/// --map-root-user` is refused with `write failed /proc/self/uid_map: Operation not
/// permitted` — the same closed door [`packaging_root_precondition`] names. So what
/// this file proves without root is that the readings **discriminate**; that they read
/// `hidden`/`0` under the real system manager is CI's root arm's measurement, and no
/// sentence here should be read as claiming it was taken elsewhere.
fn daemon_payload(
    tag: &str,
    bin_dir: &Path,
    config: &Path,
    sock_name: &str,
    extra: &str,
) -> String {
    format!(
        r#"set +e
rt=/run/{tag}
sd=/var/lib/{tag}
sock=$rt/{sock_name}
printf 'uid=%s\n' "$(id -u)"
printf 'user=%s\n' "$(id -un 2>/dev/null || echo unknown)"
printf 'groups=%s\n' "$(id -Gn 2>/dev/null | tr ' ' ',')"
printf 'home_entries=%s\n' "$(ls /home 2>/dev/null | wc -l)"
printf 'tmp_sentinel=%s\n' "$([ -e /tmp/{tag}.sentinel ] && echo visible || echo hidden)"
printf 'dir_stat=%s\n' "$(stat -c '%U %G %a' $rt 2>/dev/null || echo none)"
printf 'state_real=%s\n' "$(readlink -f $sd 2>/dev/null || echo none)"
{bin}/serial-nexus-daemon --socket $sock --state-file $sd/state.toml --config {config} {extra} > /tmp/snx-daemon.out 2> /tmp/snx-daemon.err &
pid=$!
n=0
while [ $n -lt 300 ] && [ ! -S "$sock" ]; do sleep 0.05; n=$((n+1)); done
printf 'waited=%s\n' "$n"
printf 'socket=%s\n' "$([ -S "$sock" ] && echo present || echo absent)"
printf 'sock_stat=%s\n' "$(stat -c '%U %G %a' "$sock" 2>/dev/null || echo none)"
printf 'state_file_stat=%s\n' "$(stat -c '%U %a' $sd/state.toml 2>/dev/null || echo none)"
printf 'alive=%s\n' "$(kill -0 $pid 2>/dev/null && echo yes || echo no)"
{bin}/serial-nexus-ctl --socket $sock --json state > /tmp/snx-state.json 2> /tmp/snx-state.err
printf 'state_rc=%s\n' "$?"
printf 'nodes=%s\n' "$(tr -d ' \n' < /tmp/snx-state.json | grep -o '"name":"[^"]*"' | cut -d'"' -f4 | tr '\n' ',')"
{bin}/serial-nexus-ctl --socket $sock shutdown > /tmp/snx-shutdown.out 2>&1
printf 'shutdown_rc=%s\n' "$?"
wait $pid
printf 'daemon_rc=%s\n' "$?"
printf 'daemon_err=%s\n' "$(tr -d '\n' < /tmp/snx-daemon.err | cut -c1-300)"
printf 'ctl_err=%s\n' "$(tr -d '\n' < /tmp/snx-state.err | cut -c1-200)"
"#,
        tag = tag,
        bin = bin_dir.display(),
        config = config.display(),
        sock_name = sock_name,
        extra = extra,
    )
}

/// The file *name* of the socket the unit's own `ExecStart=` binds.
///
/// The probes rename the runtime *directory* so they cannot collide with a real
/// install, so the path itself is not reusable — but the basename was typed into
/// [`daemon_payload`] as a literal, which left
/// [`the_socket_group_recipe_widens_the_control_socket_to_the_operators_group`]
/// comparing `recipe.stat_paths[1]`'s prediction against a socket the payload had
/// named for itself. Derived here so the recipe's second `stat` and the socket the
/// probe actually measures are one claim rather than two that happen to agree.
///
/// A panic rather than a skip: `check_recipe` refuses a unit whose `ExecStart=` passes
/// no `--socket`, so reaching this is a defect in the tree and not a fact about the
/// box.
fn socket_file_name(unit: &str) -> String {
    let path = exec_start_flag(unit, "--socket").unwrap_or_else(|| {
        panic!(
            "the unit's active `ExecStart=` passes no `--socket`, so no probe here can \
             name the socket the packaged daemon binds"
        )
    });
    Path::new(&path)
        .file_name()
        .unwrap_or_else(|| panic!("the unit's `--socket {path}` has no file name"))
        .to_string_lossy()
        .into_owned()
}

/// The readings a daemon-bearing probe must produce for its claim to mean anything.
///
/// Shared by the three probes that start the daemon, so a new one cannot quietly
/// assert less than the others. `tmp_sentinel` is the sandbox's own fingerprint and
/// `home_entries` corroborates it — see [`daemon_payload`] for the run that made them
/// necessary and for what each one can and cannot carry alone.
///
/// `tag` is here for the sentinel: the positive control is only a control while the
/// file it names is on the host, and that is checked below rather than beside the
/// plant. See the comment on the check for what a single-site plant did to the
/// previous arrangement.
fn assert_daemon_served(
    me: &str,
    tag: &str,
    r: &BTreeMap<String, String>,
    stdout: &str,
    node: &str,
) {
    // The floor first, and it belongs *here* rather than at one call site: a probe
    // that died partway prints a prefix of its readings, and every assertion below
    // would then be about a shorter run than the one being judged. This assertion
    // lived in one of the three callers until the doc comment above — "so a new one
    // cannot quietly assert less than the others" — was read against the code and
    // found to be claiming more than the code did. That is the
    // assertion-weaker-than-its-comment register of AGENTS §3's tell, caught in the
    // helper written to prevent it.
    assert!(
        r.len() >= 18,
        "{me}: the probe printed {} reading(s) of the eighteen it prints, so it died \
         partway.\nstdout:\n{stdout}",
        r.len()
    );
    let get = |k: &str| reading(r, k, stdout);
    // **The positive control's own control, and it stands here rather than beside the
    // plant.** `hidden` is what a probe prints when the unit was given a private
    // `/tmp`, and it is equally what a probe prints when nothing was ever planted on
    // the host — so the reading alone cannot separate a working sandbox from a
    // planter that did not run. The assertion below it, standing on its own, is then
    // AGENTS §3's tell exactly — a passing output identical to a not-running one — in
    // the one place least excusable, the control that makes every other reading in
    // this function mean something. Measured on the box of record, 2026-08-21: a
    // `-p PrivateTmp=yes` unit under the user manager,
    // which builds no mount namespace on this box, read `tmp_sentinel=hidden` with
    // nothing planted and `visible` with the file planted.
    //
    // [`run_probe`] does assert the file the moment it writes it, and that assertion
    // stays — but it is one edit away from the write it guards, and an `if false { … }`
    // around the pair takes both. The check that makes `hidden` load-bearing therefore
    // lives in the function that *consumes* the reading, and runs after the unit did
    // rather than before it, which additionally catches a sentinel that vanished
    // between the plant and the probe's read.
    let sentinel = sentinel_path(tag);
    assert!(
        sentinel.exists(),
        "{me}: the `PrivateTmp=` positive control at {} is not on the host now that \
         the probe has run, so the `tmp_sentinel={}` it reported is evidence of \
         nothing — an absent file reads `hidden` whatever the sandbox did. Either \
         `run_probe` did not plant it or something removed it mid-run; until that is \
         answered, no sandboxing claim below this line has been measured",
        sentinel.display(),
        get("tmp_sentinel")
    );
    // …and now the reading, which with the file proven present on the host says what
    // it appears to say and nothing else.
    assert_eq!(
        get("tmp_sentinel"),
        "hidden",
        "{me}: the file `run_probe` planted in the host's `/tmp` is {} inside the \
         probe unit. `PrivateTmp=yes` gives the unit a fresh `/tmp`, so seeing the \
         host's means no mount namespace was built at all — every sandboxing directive \
         in this unit is then a silent no-op and everything asserted below is green \
         about nothing (see `daemon_payload` for the run that taught this)",
        get("tmp_sentinel")
    );
    // …and `home_entries` second, corroborating rather than proving. On a box whose
    // `/home` is empty, or which has none, this reads `0` with no namespace applied —
    // measured, not assumed: `ls <empty dir> | wc -l` and `ls <absent dir> 2>/dev/null
    // | wc -l` both print 0 (box of record, 2026-08-21). It is kept because it is the
    // reading that caught the near-miss `daemon_payload` describes, and because two
    // fingerprints disagreeing is a thing worth being told.
    assert_eq!(
        get("home_entries"),
        "0",
        "{me}: `/home` has {} visible entries inside the probe unit while the sentinel \
         above reported a real namespace. `ProtectHome=yes` replaces `/home` with an \
         empty directory, so the two readings contradict each other and one of the \
         directives took effect without the other",
        get("home_entries")
    );
    assert_eq!(
        get("socket"),
        "present",
        "{me}: the daemon never bound its control socket inside the packaged sandbox \
         (waited {} × 50 ms, alive={}, daemon_rc={}). daemon stderr: {}",
        get("waited"),
        get("alive"),
        get("daemon_rc"),
        get("daemon_err")
    );
    assert_eq!(
        get("state_rc"),
        "0",
        "{me}: `serial-nexus-ctl state` failed against a daemon running under the \
         packaged sandbox. ctl stderr: {}",
        get("ctl_err")
    );
    assert!(
        get("nodes").split(',').any(|n| n == node),
        "{me}: the daemon under the packaged sandbox reports nodes `{}`, which does \
         not include `{node}` from its seed configuration — it started, but the \
         sandbox stopped it doing the one thing the seed asked for",
        get("nodes")
    );
    assert_eq!(
        get("shutdown_rc"),
        "0",
        "{me}: the daemon refused `shutdown` under the packaged sandbox"
    );
    assert_eq!(
        get("daemon_rc"),
        "0",
        "{me}: the daemon exited {} rather than 0 after `shutdown`. stderr: {}",
        get("daemon_rc"),
        get("daemon_err")
    );
}

// ---------------------------------------------------------------------------
// (9) The recipes, executed — plan §18 item 31's four `unverified` rows
// ---------------------------------------------------------------------------

/// The value of an active directive in `unit`.
fn active_value(unit: &str, name: &str) -> Option<String> {
    unit.lines().find_map(|line| {
        (directive_at(line, 0)? == name).then(|| line[name.len() + 1..].trim().to_owned())
    })
}

/// `(uid, mode)` of `path`.
#[cfg(unix)]
fn owner_and_mode(path: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let md = std::fs::metadata(path).ok()?;
    Some((md.uid(), md.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn owner_and_mode(_path: &Path) -> Option<(u32, u32)> {
    None
}

/// A probe payload that reads an identity and a runtime directory and nothing else.
fn stat_payload(tag: &str) -> String {
    format!(
        r#"set +e
printf 'uid=%s\n' "$(id -u)"
printf 'user=%s\n' "$(id -un 2>/dev/null || echo unknown)"
printf 'groups=%s\n' "$(id -Gn 2>/dev/null | tr ' ' ',')"
printf 'dir_stat=%s\n' "$(stat -c '%U %G %a' /run/{tag} 2>/dev/null || echo none)"
printf 'state_real=%s\n' "$(readlink -f /var/lib/{tag} 2>/dev/null || echo none)"
"#
    )
}

#[test]
fn the_socket_group_recipe_hands_the_runtime_directory_to_the_operators_group() {
    let me = "the_socket_group_recipe_hands_the_runtime_directory_to_the_operators_group";
    if let Err(why) = packaging_root_precondition() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let unit = read_tree_file(UNIT);
    let recipe = check_recipe(&unit).expect("the recipe's consistency is checked without root");
    let user = recipe.directive("User").expect("checked above").to_owned();
    let group = recipe.directive("Group").expect("checked above").to_owned();

    // Step one of the recipe, run as the recipe writes it.
    let identity = match RecipeIdentity::create(&recipe, &user, &group) {
        Ok(id) => id,
        Err(IdentityError::Precondition(why)) => {
            skip_no_packaging_root(me, &why);
            return;
        }
        Err(IdentityError::Recipe(why)) => panic!("{me}: {why}"),
    };
    let readback = identity.readback();

    let base = format!("snx-pkg-sockgrp-{}", std::process::id());
    let shared_tag = format!("{base}-shared");
    let static_tag = format!("{base}-static");
    let _shared_stage = RootStage::new(&shared_tag);
    let _static_stage = RootStage::new(&static_tag);

    // The **premise**: the operators' group in `SupplementaryGroups=`, which the
    // unit's block and the README's operators paragraph both say cannot reach the
    // runtime directory. Derived from the unit's own value so a change there travels.
    let mut shared = service_properties(&unit, &shared_tag);
    let shipped = active_value(&unit, "SupplementaryGroups").unwrap_or_default();
    set_property(
        &mut shared,
        "SupplementaryGroups",
        &format!("{shipped} {group}"),
    );
    let (shared_run, shared_read) = run_probe(&shared_tag, &shared, &stat_payload(&shared_tag));

    // The **recipe**: the static identity the block replaces that block with.
    let statics = recipe_properties(&unit, &recipe, &static_tag);
    let (static_run, static_read) = run_probe(&static_tag, &statics, &stat_payload(&static_tag));

    assert_probe_ran(
        me,
        &shared_run,
        "the control probe (the shipped unit, with the operators' group added to \
         SupplementaryGroups=)",
    );
    assert_probe_ran(
        me,
        &static_run,
        "the recipe probe (the unit's own static-identity block)",
    );

    // --- The identity the recipe's own commands built ---------------------------
    let (primary, all) = readback.unwrap_or_else(|| {
        panic!(
            "{me}: `id` could not read back `{user}` after the recipe's own \
             `groupadd`/`useradd` reported success"
        )
    });
    assert_eq!(
        primary, group,
        "{me}: the recipe's `useradd` left `{user}`'s PRIMARY group as `{primary}`, \
         not `{group}`. The whole block turns on the primary group, because that is \
         the only one systemd chowns a *Directory= to"
    );
    if let Some(extra) = recipe
        .commands
        .iter()
        .flat_map(|c| c.windows(2))
        .find(|w| w[0] == "--groups")
        .map(|w| w[1].clone())
    {
        for name in extra.split(',') {
            assert!(
                all.split_whitespace().any(|g| g == name),
                "{me}: the recipe asks `useradd` for supplementary group `{name}` and \
                 `{user}` is in `{all}` — the device group the daemon needs is the \
                 one that went missing"
            );
        }
    }

    // --- The premise, measured -------------------------------------------------
    let sg = |k: &str| reading(&shared_read, k, &shared_run.stdout);
    assert_ne!(
        sg("uid"),
        "0",
        "{me}: the control probe ran as root, so `DynamicUser=` did not take effect \
         and nothing it reports is about a transient identity"
    );
    assert!(
        sg("groups").split(',').any(|g| g == group),
        "{me}: `{group}` is not in the control probe's group list `{}`, so \
         `SupplementaryGroups=` did not take effect and the reading below would show \
         a group that cannot reach the directory for the trivial reason that the \
         service is not in it",
        sg("groups")
    );
    let shared_cells: Vec<&str> = sg("dir_stat").split_whitespace().collect();
    assert_eq!(
        shared_cells.len(),
        3,
        "{me}: the control probe read `{}` for its runtime directory rather than a \
         `%U %G %a` triple",
        sg("dir_stat")
    );
    assert_ne!(
        shared_cells[1], group,
        "{me}: the runtime directory's GROUP is `{group}` under `SupplementaryGroups=` \
         alone. That would make the premise false — the unit's socket-group block, \
         `packaging/README.md`'s operators paragraph and plan §18 item 31 all rest on \
         systemd chowning `*Directory=` to `User=`/`Group=` and to nothing else, and \
         all three would have to change. Read this as a finding about systemd, not as \
         a probe defect"
    );
    let shipped_mode = active_value(&unit, "RuntimeDirectoryMode").unwrap_or_default();
    assert_eq!(
        shared_cells[2],
        shipped_mode.trim_start_matches('0'),
        "{me}: the runtime directory came up mode `{}` under the shipped unit, not \
         its own `RuntimeDirectoryMode={shipped_mode}`. The `EACCES` an operator gets \
         at `connect(2)` is that mode plus the wrong group; with a different mode the \
         paragraph's conclusion no longer follows from its premise",
        shared_cells[2]
    );

    // --- The recipe, measured ---------------------------------------------------
    let rg = |k: &str| reading(&static_read, k, &static_run.stdout);
    assert_eq!(
        rg("user"),
        user,
        "{me}: the recipe probe ran as `{}`, not `{user}` — `DynamicUser=no` plus \
         `User=` did not take effect, so the directory reading below is a transient \
         identity's",
        rg("user")
    );
    assert_eq!(
        rg("dir_stat"),
        recipe.predictions[0],
        "{me}: the unit's socket-group block predicts `{}` for `{}` and the recipe it \
         spells produced `{}`. The block's own `stat` is the check it tells an \
         operator to run, so this is the recipe being wrong about itself",
        recipe.predictions[0],
        recipe.stat_paths[0],
        rg("dir_stat")
    );
    assert!(
        !rg("state_real").contains("/private/"),
        "{me}: under the static identity the state directory still resolves to `{}`. \
         The block's own `Two consequences` paragraph tells an operator the \
         `/var/lib/private` indirection is a `DynamicUser=` behaviour they lose here — \
         and an operator who believes that and finds this would move a snapshot to the \
         wrong path",
        rg("state_real")
    );

    eprintln!(
        "MEASURED {me}: recipe identity `{user}`:`{primary}` groups `{all}`; \
         SupplementaryGroups= control read dir_stat={:?} groups={:?}; the recipe read \
         dir_stat={:?} (predicted {:?}) state_real={:?}",
        sg("dir_stat"),
        sg("groups"),
        rg("dir_stat"),
        recipe.predictions[0],
        rg("state_real"),
    );
}

#[test]
fn the_packaged_sandbox_starts_the_daemon_and_it_serves() {
    let me = "the_packaged_sandbox_starts_the_daemon_and_it_serves";
    if let Err(why) = packaging_root_precondition() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let unit = read_tree_file(UNIT);
    let tag = format!("snx-pkg-serves-{}", std::process::id());
    let stage = RootStage::new(&tag);
    if let Err(why) = stage.stage_binaries() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let config = stage.write_config("probe");
    let props = service_properties(&unit, &tag);
    let payload = daemon_payload(&tag, &stage.bin_dir, &config, &socket_file_name(&unit), "");
    let (run, read) = run_probe(&tag, &props, &payload);
    assert_probe_ran(me, &run, "the packaged sandbox with the daemon in it");

    let get = |k: &str| reading(&read, k, &run.stdout);
    assert_ne!(
        get("uid"),
        "0",
        "{me}: the daemon ran as root, so `DynamicUser=` did not take effect and this \
         is not the packaged identity"
    );
    assert_daemon_served(me, &tag, &read, &run.stdout, "probe");
    let sock: Vec<&str> = get("sock_stat").split_whitespace().collect();
    assert_eq!(
        sock.len(),
        3,
        "{me}: the control socket read `{}` rather than a `%U %G %a` triple",
        get("sock_stat")
    );
    assert_eq!(
        sock[2], "600",
        "{me}: the daemon's control socket came up mode `{}` under the packaged unit. \
         `packaging/README.md` tells an operator it is 0600 and that whoever can open \
         it owns every console; `p9_permissions.rs` measures that off systemd, and \
         this is the same promise inside the sandbox that ships",
        sock[2]
    );
    assert!(
        get("state_real").contains("/private/"),
        "{me}: the state directory resolves to `{}` under `DynamicUser=yes` — the \
         upgrade procedure's whole reason to exist is that it resolves into \
         `/var/lib/private/`",
        get("state_real")
    );

    eprintln!(
        "MEASURED {me}: the packaged unit's own [Service] sandbox started the daemon \
         as uid={} user={} (the host's /tmp sentinel read {} and home_entries={}, so \
         the mount namespace was built), it bound \
         {} and answered `state` in {} × 50 ms with nodes {:?}, socket {:?}, state \
         directory {:?}; `shutdown` returned {} and the daemon exited {}",
        get("uid"),
        get("user"),
        get("tmp_sentinel"),
        get("home_entries"),
        get("socket"),
        get("waited"),
        get("nodes"),
        get("sock_stat"),
        get("state_real"),
        get("shutdown_rc"),
        get("daemon_rc"),
    );
}

#[test]
fn the_socket_group_recipe_widens_the_control_socket_to_the_operators_group() {
    let me = "the_socket_group_recipe_widens_the_control_socket_to_the_operators_group";
    if let Err(why) = packaging_root_precondition() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let unit = read_tree_file(UNIT);
    let recipe = check_recipe(&unit).expect("the recipe's consistency is checked without root");
    let user = recipe.directive("User").expect("checked above").to_owned();
    let group = recipe.directive("Group").expect("checked above").to_owned();
    let flag = recipe
        .socket_group_flag
        .clone()
        .expect("check_recipe refuses a recipe with no widening flag");

    let tag = format!("snx-pkg-socket-{}", std::process::id());
    let stage = RootStage::new(&tag);
    if let Err(why) = stage.stage_binaries() {
        skip_no_packaging_root(me, &why);
        return;
    }
    // Bound, not dropped: its `Drop` deletes the identity however this test ends.
    let _identity = match RecipeIdentity::create(&recipe, &user, &group) {
        Ok(id) => id,
        Err(IdentityError::Precondition(why)) => {
            skip_no_packaging_root(me, &why);
            return;
        }
        Err(IdentityError::Recipe(why)) => panic!("{me}: {why}"),
    };

    let config = stage.write_config("probe");
    let props = recipe_properties(&unit, &recipe, &tag);
    let payload = daemon_payload(
        &tag,
        &stage.bin_dir,
        &config,
        &socket_file_name(&unit),
        &flag,
    );
    let (run, read) = run_probe(&tag, &props, &payload);
    assert_probe_ran(
        me,
        &run,
        "the socket-group recipe with the daemon in it (static identity, widened \
         socket)",
    );

    let get = |k: &str| reading(&read, k, &run.stdout);
    assert_eq!(
        get("user"),
        user,
        "{me}: the probe ran as `{}`, not the recipe's `{user}`",
        get("user")
    );
    assert_daemon_served(me, &tag, &read, &run.stdout, "probe");
    assert_eq!(
        get("dir_stat"),
        recipe.predictions[0],
        "{me}: the recipe predicts `{}` for `{}` and produced `{}`",
        recipe.predictions[0],
        recipe.stat_paths[0],
        get("dir_stat")
    );
    assert_eq!(
        get("sock_stat"),
        recipe.predictions[1],
        "{me}: the unit's socket-group block predicts `{}` for `{}` and the recipe it \
         spells — including its `{flag}` — produced `{}`. Both halves of the block's \
         own verification `stat` have to hold, because an operator who gets the \
         directory right and the socket wrong has a group that can traverse to a \
         socket it cannot open",
        recipe.predictions[1],
        recipe.stat_paths[1],
        get("sock_stat")
    );

    eprintln!(
        "MEASURED {me}: the unit's socket-group recipe, executed end to end — \
         `{}` produced dir_stat={:?} (predicted {:?}) and sock_stat={:?} (predicted \
         {:?}); the daemon served `state` and shut down cleanly under it",
        recipe
            .commands
            .iter()
            .map(|c| c.join(" "))
            .collect::<Vec<_>>()
            .join("` + `"),
        get("dir_stat"),
        recipe.predictions[0],
        get("sock_stat"),
        recipe.predictions[1],
    );
}

#[test]
fn the_upgrade_procedures_root_copy_carries_the_snapshot_across() {
    let me = "the_upgrade_procedures_root_copy_carries_the_snapshot_across";
    if let Err(why) = packaging_root_precondition() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let unit = read_tree_file(UNIT);
    let tag = format!("snx-pkg-upgrade-{}", std::process::id());
    let stage = RootStage::new(&tag);
    if let Err(why) = stage.stage_binaries() {
        skip_no_packaging_root(me, &why);
        return;
    }
    let config = stage.write_config("seeded");
    let props = service_properties(&unit, &tag);
    let payload = daemon_payload(&tag, &stage.bin_dir, &config, &socket_file_name(&unit), "");

    // Step 1 — "Let systemd create the new state directory (this start comes up on
    // the seed)". The unit name is the same on both starts on purpose: a transient
    // unit's `DynamicUser=` identity is allocated from that name, and two names would
    // measure two upgrades of two different services.
    let (seed_run, seed_read) = run_probe(&tag, &props, &payload);
    assert_probe_ran(
        me,
        &seed_run,
        "the upgrade procedure's first start (the seed)",
    );
    assert_daemon_served(me, &tag, &seed_read, &seed_run.stdout, "seeded");

    // Step 2 — "Copy the pre-rename snapshot over the seeded one", as root, with the
    // procedure's own `cp`. What the page promises about it is in its parenthetical:
    // that `cp` onto an existing file keeps that file's owner and its 0600 mode.
    let host_state = PathBuf::from(format!("/var/lib/{tag}/state.toml"));
    let before = owner_and_mode(&host_state).unwrap_or_else(|| {
        panic!(
            "{me}: no state file at {} after the seeding start. The procedure's step 2 \
             copies *over* a seeded file, and its parenthetical about `cp` keeping the \
             owner and mode is only true of an existing target — if the seed leaves \
             none, step 2 needs rewriting",
            host_state.display()
        )
    });
    assert_eq!(
        before.1, 0o600,
        "{me}: the seeded state file is mode {:o}, not 0600. The procedure's step 2 \
         tells an operator the copy keeps `that file's owner and its 0600 mode`",
        before.1
    );
    let old_dir = PathBuf::from(format!("/var/lib/{tag}-old"));
    std::fs::create_dir_all(&old_dir).expect("create the pre-rename state directory");
    let old_state = old_dir.join("state.toml");
    std::fs::write(
        &old_state,
        format!("[[node]]\ntype = \"pty\"\nname = \"carried\"\npath = \"/run/{tag}/carried\"\n"),
    )
    .expect("write the pre-rename snapshot");
    let cp = run_tool(
        "cp",
        &[
            old_state.display().to_string(),
            host_state.display().to_string(),
        ],
    )
    .expect("cp is not on PATH, which no box running systemd is");
    assert!(
        cp.ok(),
        "{me}: the procedure's own `cp {} {}` exited {:?}: {}. Root is supposed to be \
         able to write into a `DynamicUser=` state directory — that is the entire \
         reason the step exists",
        old_state.display(),
        host_state.display(),
        cp.status,
        cp.stderr.trim()
    );
    let after =
        owner_and_mode(&host_state).expect("the state file exists, it was just copied onto");
    assert_eq!(
        after, before,
        "{me}: `cp` onto the seeded state file changed it from (uid {}, mode {:o}) to \
         (uid {}, mode {:o}). The procedure's step 2 promises the opposite in so many \
         words, and an operator who believes it would leave a root-owned snapshot the \
         service cannot read",
        before.0, before.1, after.0, after.1
    );

    // Step 3 — "Start it for real, and confirm the graph came across."
    let (adopt_run, adopt_read) = run_probe(&tag, &props, &payload);
    assert_probe_ran(
        me,
        &adopt_run,
        "the upgrade procedure's second start (the adoption)",
    );
    assert_daemon_served(me, &tag, &adopt_read, &adopt_run.stdout, "carried");
    let nodes = reading(&adopt_read, "nodes", &adopt_run.stdout);
    assert!(
        !nodes.split(',').any(|n| n == "seeded"),
        "{me}: after the copy the daemon reports `{nodes}`, which still carries the \
         seed's node. The snapshot was copied and then not preferred, which is the \
         silent half of the failure the procedure exists to prevent"
    );

    eprintln!(
        "MEASURED {me}: the README's upgrade procedure, executed step by step under \
         the packaged unit's own [Service] properties. Seed start left \
         state_file_stat={:?} at {:?}; root `cp` kept (uid {}, mode {:o}); the second \
         start read state_file_stat={:?} and reported nodes {:?}",
        reading(&seed_read, "state_file_stat", &seed_run.stdout),
        reading(&seed_read, "state_real", &seed_run.stdout),
        after.0,
        after.1,
        reading(&adopt_read, "state_file_stat", &adopt_run.stdout),
        nodes,
    );
}
