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
//! Six checks run here, in three classes.
//!
//! **Tool checks** — [`the_packaged_unit_verifies_clean_under_systemd_analyze`] and
//! [`the_packaged_udev_rules_verify_clean_under_udevadm`] — hand the shipped files
//! to systemd's own validators. They self-skip where those validators do not exist
//! (a Mac, a minimal container), through the one `required` mechanism, under
//! `SNX_PACKAGING`.
//!
//! **Text checks** — [`every_directive_the_readme_names_exists_in_the_packaged_unit`],
//! [`every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class`] and
//! [`the_root_probe_derives_its_sandbox_from_the_packaged_unit`] — need no tool and
//! never skip, so the drift class this project can actually cause is covered on
//! every platform on every push. Each derives *both* sides from the shipped files,
//! never from a list kept here; `meta_derive.rs` is the shape.
//!
//! And one root-gated measurement,
//! [`dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown`],
//! which is item 31's other half: the two claims in `packaging/README.md`'s evidence
//! table marked *man-page, and owed a measurement*. Every box this project has
//! measured on fails its precondition, so the part of its machinery that is pure
//! derivation is pulled out into the text check above rather than waiting with it —
//! a test that has never executed anywhere is a test nobody has debugged.
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
//! * **Whether any `DeviceAllow=` matches a device this machine has**, whether the
//!   `dialout` group is the right one for the distro, or whether the daemon can
//!   actually run under the sandbox. Nothing here starts the service.
//! * **Semantic correctness of a well-formed value.** `ProtectSystem=full` instead
//!   of `strict` is a valid unit and a weaker sandbox; only the evidence-table check
//!   would notice the *directive* vanishing, not its value changing.
//! * **The README's prose.** The text checks compare directive *names*. A paragraph
//!   that describes `ProtectSystem=strict` as doing the opposite of what it does is
//!   a review problem, not a gate problem.

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
/// privilege, and every box this project has measured on so far fails it — this one
/// cannot `systemctl start` (polkit refuses without a terminal) and cannot fall back
/// to a user namespace (`unshare -Ur` is refused by the kernel's `uid_map` policy).
/// Folding the two classes into one variable would mean an operator who set
/// `SNX_PACKAGING=required` for the cheap checks got a hard failure for a privilege
/// they never claimed to have.
///
/// **Deliberately not switched on anywhere yet.** Whether a CI runner gives us
/// systemd as PID 1 with passwordless root is unmeasured, and shipping `required` on
/// an assumption reddens a lane for someone else's runner image. The precedent is
/// `SNX_RIG_FLOW`, whose precondition was measured before it was demanded. Until a
/// green run proves it, this test *reports* what it found and the skip line is the
/// report.
fn skip_no_packaging_root(test: &str, reason: &str) {
    assert!(
        std::env::var("SNX_PACKAGING_ROOT").as_deref() != Ok("required"),
        "SNX_PACKAGING_ROOT=required, but {test} skipped: {reason}.\n\
         Required mode exists so a box that can start a transient systemd service \
         cannot report a green run for the two packaging claims that have never \
         been measured anywhere (plan §18 item 31, plan §3 rule 11). Run as root on \
         a systemd box, or unset SNX_PACKAGING_ROOT."
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
    let recorded = backticked_directives(section);
    assert!(
        active.len() >= 30,
        "the packaged unit parsed to {} active directive(s); the unit has not shrunk \
         that far, so the matcher has",
        active.len()
    );
    assert!(
        recorded.len() >= 30,
        "packaging/README.md's `{EVIDENCE_HEADING}` section records {} directive(s) \
         — it was gutted, and a gate comparing against a gutted table passes for the \
         wrong reason",
        recorded.len()
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
        !backticked_directives(&holed).contains(&victim),
        "`{victim}` survived {removed} deletion(s) from the evidence table — it is \
         recorded in a spelling the stripper misses, and the proof below would pass \
         on an unmutated table (plan §3 rule 17(b))"
    );
    assert!(
        !active.is_subset(&backticked_directives(&holed)),
        "removing `{victim}=` from the evidence table produced no drift: this gate \
         cannot notice a shipped directive whose evidence class nobody recorded, \
         which is the whole of plan §18 item 31"
    );

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
    // recorded evidence class is what the item was filed against.
    let unrecorded = drift_one_way(&active, &recorded);
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
    let phantoms = drift_one_way(&recorded, &mentioned);
    assert!(
        phantoms.is_empty(),
        "packaging/README.md's `{EVIDENCE_HEADING}` table records directive(s) that \
         appear nowhere in packaging/serial-nexus-daemon.service, not even in its \
         comments — the table is describing a unit that has moved on:\n  {}",
        phantoms.join("\n  ")
    );

    // The table's own vocabulary is closed: three class words and no others. A row
    // that invents a fourth is a claim nobody can weigh.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for class in ["measured", "man-page", "unverified"] {
        counts.insert(class, section.matches(class).count());
    }
    for (class, n) in &counts {
        assert!(
            *n > 0,
            "the evidence table uses the class `{class}` zero times — a record with \
             only one class in it is a record that stopped distinguishing"
        );
    }
    eprintln!(
        "MEASURED every_directive_the_packaged_unit_sets_has_a_recorded_evidence_class: \
         {} active directives, all recorded; class-word occurrences {counts:?}",
        active.len()
    );
}

// ---------------------------------------------------------------------------
// (5) The root-gated measurement — item 31's owed half
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
/// A test that has never executed anywhere is a test nobody has debugged, and this
/// tree's every box fails its precondition — so the part of its machinery that is
/// pure derivation runs on every push instead of waiting for a root box. What is
/// covered here: that the probe's `systemd-run` properties really are the packaged
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
    // **`-scratch`, and the suffix is load-bearing.** `service_properties` renames the
    // unit's three `*Directory=` values to `tag`, so `RuntimeDirectory=<tag>` makes
    // systemd create — and, under `DynamicUser=yes`, **chown** — `/run/<tag>` for the
    // service's dynamic user. Putting the probe directories inside that path handed
    // their ownership to the very user whose writes this test is trying to refuse, and
    // the ownership control died silently: the first CI run after the `/tmp` repair got
    // past `EXIT_NAMESPACE` and then reported `listed_write=ok` on a directory it had
    // just created root-owned 0755. Two different paths that must not be one, so they
    // are spelled differently rather than kept apart by a comment.
    let base = PathBuf::from("/run").join(format!("{tag}-scratch"));
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
    args.push(format!(
        r#"set +e
sd=/var/lib/{tag}
printf 'uid=%s\n' "$(id -u)"
printf 'user=%s\n' "$(id -un 2>/dev/null || echo unknown)"
if echo probe > "$sd/probe.txt" 2>/tmp/e1; then printf 'state_write=ok\n'
else printf 'state_write=fail:%s\n' "$(tr -d '\n' < /tmp/e1)"; fi
printf 'state_stat=%s\n' "$(stat -c '%U:%a' "$sd" 2>/dev/null || echo none)"
printf 'state_real=%s\n' "$(readlink -f "$sd" 2>/dev/null || echo none)"
if true > "{rw}/probe.txt" 2>/tmp/e2; then printf 'listed_write=ok\n'
else printf 'listed_write=fail:%s\n' "$(tr -d '\n' < /tmp/e2)"; fi
if true > "{ro}/probe.txt" 2>/tmp/e3; then printf 'unlisted_write=ok\n'
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
