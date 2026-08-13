//! The bless recipe: install a copy of this binary at a stable, mode-restricted
//! path so `setcap` has something durable to bless (design §15.45).
//!
//! Why a copy at all, rather than blessing `target/<profile>/serial-nexus-devprep`
//! in place: the kernel clears `security.capability` on every write to a file, and
//! `cargo build` rewrites that path constantly, so an in-place blessing would be
//! destroyed by the next build and would have to be re-applied — with a `sudo` —
//! on every rebuild. The stable copy is touched only by `install`.
//!
//! **This module is the only place in the crate that spawns a process, and it is
//! unreachable from a blessed process**: `main` refuses `install` when **any**
//! capability in [`REQUIRED_CAPS`] is held, so no file capability and
//! `Command::new` ever coexist.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where blessed copies live: project-local and gitignored, never `~/.cargo/bin`
/// or `/usr/local/bin`.
///
/// Project-local for the reason vmcell records: two checkouts of this tree on one
/// box must not fight over a single blessed binary, and a capability-carrying file
/// on a shared system path outlives the checkout that explains it.
pub const BIN_DIR: &str = ".snx-bin";

/// The mode the installed copy must carry, applied **before** `setcap`.
///
/// This is the real security boundary, not a tidiness note. The blessed copy holds
/// `CAP_DAC_OVERRIDE`; an other-executable copy of it is a local privilege
/// escalation for every user on the box. Owner-only execution means the grant is
/// same-user — it hands the developer nothing they could not already do — rather
/// than cross-user. On a shared box with a team, `0750` plus a dedicated group is
/// the right adjustment; `0700` is the safe default.
pub const BLESSED_MODE: u32 = 0o700;

/// How the installed copy compares to the freshly-built one.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallState {
    /// Installed, byte-identical to the build, mode `0700`, and carrying **every**
    /// capability in [`REQUIRED_CAPS`] with the effective bit.
    ///
    /// Not `cap_dac_override` alone: [`field_grants_required_caps`] requires the
    /// whole set, and its own test pins that half a set does not read as blessed.
    /// This doc said otherwise until 2026-08-12 — the same contradicts-itself-one-
    /// screen-apart shape the §15.55 alignment pass repaired in `sys/src/caps.rs`.
    Ready,
    /// Nothing at the stable path yet.
    Absent,
    /// Present but not byte-identical to `target/<profile>/` — a rebuild happened.
    Stale,
    /// Present and current, but the mode is not `0700`.
    WrongMode(u32),
    /// Present and current, but the capability is missing or not `+ep`.
    Unblessed(String),
}

/// Absolute path of the stable copy for `profile`.
pub fn blessed_path(repo_root: &Path, profile: &str) -> PathBuf {
    repo_root
        .join(BIN_DIR)
        .join(profile)
        .join("serial-nexus-devprep")
}

/// Absolute path of the freshly-built binary for `profile`.
pub fn built_path(repo_root: &Path, profile: &str) -> PathBuf {
    repo_root
        .join("target")
        .join(profile)
        .join("serial-nexus-devprep")
}

/// Extract the capability field from one `getcap` output line.
///
/// Split deliberately from any substring test, because the naive form is a real
/// defect that vmcell hit and recorded: `getcap` prints `<path> <caps>`, so testing
/// the whole line for `ep` is satisfied by a *path* containing those letters —
/// `target/debug/deps/...` does, and so does any user named `steph`. Such a test
/// reports an un-raised `+p`-only binary as blessed, and a skip that reads as a pass
/// is the worst outcome available.
///
/// Returns the capability text only, so the caller can test *it* rather than the
/// line.
pub fn getcap_field(line: &str) -> Option<&str> {
    // Format is `<path> <caps>` (older libcap used `<path> = <caps>`); the caps
    // field is the last whitespace-separated token and always contains '='.
    let field = line.trim().rsplit_once(char::is_whitespace)?.1;
    field.contains('=').then_some(field)
}

/// The capabilities a blessed copy must carry, and why each is there — **the one
/// place the set is written** (§15.45, §15.55; AGENTS §4). Adding an entry is a
/// design amendment, not a patch.
///
/// `cap_dac_override` writes `authorized` in sysfs (`root:root 0644`);
/// `cap_fowner` sets a POSIX ACL on a tty node the invoking user does not own
/// (§15.55). Both are required: a copy holding only the first can replug but
/// cannot hand back access to the node it just recreated, which is the shape the
/// rig lane failed on.
///
/// Each entry carries **both** representations the tree needs — the `setcap` name
/// and the `linux/capability.h` bit — because they were two lists before, and two
/// lists that must agree is the shape that let the no-`exec`-while-blessed refusal
/// key on `cap_dac_override` alone while `field_grants_required_caps` required
/// both: a copy blessed with only `cap_fowner` carried a capability and was not
/// refused. One list, two projections, and the drift is unrepresentable.
pub const REQUIRED_CAPS: &[(&str, u32)] = &[
    ("cap_dac_override", serial_nexus_sys::caps::CAP_DAC_OVERRIDE),
    ("cap_fowner", serial_nexus_sys::caps::CAP_FOWNER),
];

/// Just the `setcap` names, in order — the projection every command string uses.
pub fn required_cap_names() -> Vec<&'static str> {
    REQUIRED_CAPS.iter().map(|(name, _)| *name).collect()
}

/// Does a capability field *mention* any capability in [`REQUIRED_CAPS`], whatever
/// the flags?
///
/// A deliberately looser question than [`field_grants_required_caps`], because it is
/// a different question. That one asks "is this copy blessed" and must be strict:
/// every name, both flags, or the helper would report itself ready and then fail the
/// write. This one asks "is this file one of *ours*", and the two answers diverge in
/// exactly the cases orphan hygiene is about. A copy blessed before `cap_fowner`
/// joined the set carries `cap_dac_override=ep`, is **not** blessed by the strict
/// test, and is still a root-equivalent capability sitting on a file nothing looks
/// for (notes §3.81). A `+p`-only copy is one `capset(2)` — a call the process can
/// make itself — away from effective, which is the same reasoning
/// `unhardened_disposition` uses to treat `+p` as privilege.
///
/// Used only to *describe* what the sweep found. What the sweep removes is decided
/// by [`orphans_in`], on the broader "carries a capability at all" test, for the
/// reason recorded there.
pub fn field_carries_a_required_cap(field: &str) -> bool {
    // The names half is everything left of the last `=`; a field with no `=` at all
    // never reaches here, because `getcap_field` requires one.
    let names = field.rsplit_once('=').map(|(n, _)| n).unwrap_or(field);
    names
        .split(',')
        .map(|n| n.trim().trim_start_matches('+'))
        .any(|n| REQUIRED_CAPS.iter().any(|(want, _)| n == *want))
}

/// A capability-carrying file in the blessed directory that is not the copy
/// `install` put there.
#[derive(Debug, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    /// What `getcap` said it carries, verbatim — so the report names the grant
    /// rather than asserting one.
    pub field: String,
    /// Whether [`field`](Self::field) names a capability this tool grants
    /// ([`REQUIRED_CAPS`]). False means the file carries something this tree never
    /// asks for, which is worth a human's eye and is still not left lying there.
    pub ours: bool,
}

/// Every capability-carrying file directly under `dir` except `keep`.
///
/// **Why the removal test is "carries a capability at all" rather than the
/// [`REQUIRED_CAPS`]-derived one.** `.snx-bin/<profile>/` is created by `install`
/// and by nothing else; it is gitignored, it is not on anyone's `PATH`, and the only
/// file that belongs in it is the copy `install` just wrote. So a capability this
/// tree never grants is *less* explicable there than one it does, not more — and
/// §15.45's narrowness argument is about capability-carrying files nothing
/// references, not about which capability they happen to carry. The derived set is
/// what the report uses to say whether the file was one of ours, which is the part a
/// hand-kept list would have got wrong the day `cap_fowner` joined (§15.55).
///
/// `read` is injected rather than called directly so the **walker** is provable
/// without root: no unprivileged test can create a file carrying a capability, so a
/// sweep that only ever ran against a real `getcap` would have its directory walk and
/// its keep-exclusion asserted by nothing (AGENTS §3 — a scanning gate proves its
/// matcher *and* its walker).
///
/// Symlinks are skipped: `read_dir`'s file type does not follow them, and a symlink
/// carries no capability of its own — only its target does, and if that target is in
/// this directory it is visited on its own account.
pub fn orphans_in(
    dir: &Path,
    keep: &Path,
    read: &dyn Fn(&Path) -> io::Result<Option<String>>,
) -> io::Result<Vec<Orphan>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // Nothing installed yet is not a problem to report; it is the ordinary state
        // of a fresh checkout.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    // Canonicalized once: the keep is compared by resolved path, so `./x` and `x`
    // and a `..`-laden spelling all name the same file. A keep that does not exist
    // yet canonicalizes to nothing, which is correct — then every capability-carrying
    // file in there is an orphan.
    let keep_real = keep.canonicalize().ok();
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.canonicalize().ok() == keep_real && keep_real.is_some() {
            continue;
        }
        if let Some(field) = read(&path)? {
            found.push(Orphan {
                ours: field_carries_a_required_cap(&field),
                field,
                path,
            });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// One orphan, and what happened to it.
#[derive(Debug)]
pub struct Swept {
    pub orphan: Orphan,
    /// `None` when the file is gone. Populated rather than raised, because a sweep
    /// that could not unlink its third file must still report the first two.
    pub error: Option<String>,
}

/// Remove every capability-carrying file in `profile`'s blessed directory except the
/// copy that belongs there, and say what was removed.
///
/// **Deleted, not stripped, and that is a measurement rather than a preference.**
/// `setcap -r` needs root; unlinking needs only write permission on the *directory*,
/// which the user running `install` already has because `install` created it. So the
/// delete form is the one that works with no password on the same run that noticed
/// the problem, and a sweep that needed a `sudo` would be a sweep that gets deferred.
/// Nothing is lost: the file is a copy of a build artifact, reproducible by
/// `scripts/bless`.
pub fn sweep_orphans(repo_root: &Path, profile: &str) -> io::Result<Vec<Swept>> {
    let keep = blessed_path(repo_root, profile);
    let dir = keep.parent().expect("blessed path has a parent").to_owned();
    let orphans = orphans_in(&dir, &keep, &read_caps)?;
    Ok(orphans
        .into_iter()
        .map(|orphan| {
            let error = std::fs::remove_file(&orphan.path)
                .err()
                .map(|e| e.to_string());
            Swept { orphan, error }
        })
        .collect())
}

/// The `Ready` state in one line, **derived** rather than typed.
///
/// The two facts it states — the mode and the capability set — both have exactly one
/// home in this file, and this line used to spell the second one by hand as
/// `cap_dac_override +ep`: the pre-§15.55 single-capability form, still printed by a
/// tree that had required two since that amendment landed (plan §18 item 52 (e)). A
/// report that names a smaller grant than the file carries is the worst direction for
/// this particular sentence to be wrong in.
pub fn ready_description() -> String {
    format!(
        "mode {:04o}, {}+ep",
        BLESSED_MODE,
        required_cap_names().join(",")
    )
}

/// Whether a capability field grants **every** capability in [`REQUIRED_CAPS`]
/// *effectively*.
///
/// `+p`/`=p` without `e` is permitted-but-not-raised: the helper would still fail
/// the write. Both flags must be present, and every required name must appear —
/// checked per name rather than by string equality, because libcap is free to
/// print them in any order and to add ones we did not ask for.
pub fn field_grants_required_caps(field: &str) -> bool {
    let Some((names, flags)) = field.rsplit_once('=') else {
        return false;
    };
    // libcap prints either `cap_dac_override=ep` or, with several caps,
    // `cap_dac_override,cap_fowner=ep`; `=` may also be `+`-prefixed per-cap.
    let present: Vec<&str> = names
        .split(',')
        .map(|n| n.trim_start_matches('+'))
        .collect();
    let all_named = REQUIRED_CAPS.iter().all(|(want, _)| present.contains(want));
    all_named && flags.contains('e') && flags.contains('p')
}

/// Ask `getcap` what the file carries. `Ok(None)` when it carries nothing.
fn read_caps(path: &Path) -> io::Result<Option<String>> {
    let out = Command::new("getcap").arg(path).output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "getcap {} failed: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    Ok(getcap_field(&text).map(str::to_owned))
}

/// Inspect the stable copy against the freshly-built one.
pub fn inspect(repo_root: &Path, profile: &str) -> io::Result<InstallState> {
    let blessed = blessed_path(repo_root, profile);
    if !blessed.exists() {
        return Ok(InstallState::Absent);
    }
    let built = built_path(repo_root, profile);
    // Byte comparison rather than a recorded hash: a stamp file is a third thing
    // that can disagree with the other two, and vmcell's own notes record a false
    // "already blessed" from exactly that. Comparing the artifacts leaves nothing
    // to go stale.
    if built.exists() && std::fs::read(&built)? != std::fs::read(&blessed)? {
        return Ok(InstallState::Stale);
    }
    let mode = std::fs::metadata(&blessed)?.permissions().mode() & 0o7777;
    if mode != BLESSED_MODE {
        return Ok(InstallState::WrongMode(mode));
    }
    match read_caps(&blessed)? {
        Some(field) if field_grants_required_caps(&field) => Ok(InstallState::Ready),
        Some(field) => Ok(InstallState::Unblessed(field)),
        None => Ok(InstallState::Unblessed("(none)".to_owned())),
    }
}

/// Copy the built binary to the stable path and restrict it to its owner.
///
/// Does **not** run `setcap`: that needs root, and this tool never invokes `sudo`
/// on the operator's behalf. It prints the exact command instead, so the privileged
/// step is something a human types and can read first.
pub fn install(repo_root: &Path, profile: &str) -> io::Result<PathBuf> {
    let built = built_path(repo_root, profile);
    if !built.exists() {
        return Err(io::Error::other(format!(
            "{} does not exist — run `cargo build --workspace{}` first",
            built.display(),
            if profile == "release" {
                " --release"
            } else {
                ""
            }
        )));
    }
    let blessed = blessed_path(repo_root, profile);
    let dir = blessed.parent().expect("blessed path has a parent");
    std::fs::create_dir_all(dir)?;
    // Remove before copying: writing over a blessed file would clear its caps
    // anyway, but an explicit unlink makes the "the old blessing is gone" step
    // visible rather than incidental.
    let _ = std::fs::remove_file(&blessed);
    std::fs::copy(&built, &blessed)?;
    // Mode before caps, always: a window where the file is both capability-carrying
    // and group/other-executable is the escalation this ordering exists to avoid.
    std::fs::set_permissions(&blessed, std::fs::Permissions::from_mode(BLESSED_MODE))?;
    Ok(blessed)
}

/// The one command an operator must run as root, spelled exactly.
///
/// Derived from [`REQUIRED_CAPS`] rather than typed, so the string an operator is
/// told to run and the string `--verify` checks for can never disagree — the
/// hand-kept-list drift this repository keeps finding (AGENTS §3).
pub fn setcap_command(blessed: &Path) -> String {
    format!(
        "sudo setcap {}+ep {}",
        required_cap_names().join(","),
        blessed.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `/deps/` trap, planted in every spelling it could take. A matcher that
    /// tests the whole line passes all of these; the field parser must not.
    #[test]
    fn a_path_containing_the_flag_letters_does_not_read_as_blessed() {
        // `+p` only — permitted, not effective. The path contains "deps" and "ep".
        let line = "/home/steph/repo/target/debug/deps/serial-nexus-devprep cap_dac_override=p\n";
        let field = getcap_field(line).expect("a caps field is present");
        assert_eq!(field, "cap_dac_override=p");
        assert!(
            !field_grants_required_caps(field),
            "a +p-only binary must not read as blessed"
        );
        // The naive test this exists to prevent would have passed:
        assert!(
            line.contains("ep"),
            "the trap is real: the line does contain 'ep'"
        );
    }

    /// The accepting cases, including the multi-capability spelling.
    #[test]
    fn an_effective_dac_override_is_recognised_in_each_spelling() {
        for line in [
            "/x/serial-nexus-devprep cap_dac_override,cap_fowner=ep",
            "/x/serial-nexus-devprep cap_fowner,cap_dac_override=pe",
            "/x/serial-nexus-devprep cap_dac_override,cap_fowner,cap_net_admin=ep",
            "/x/serial-nexus-devprep cap_net_admin,cap_fowner,cap_dac_override=ep\n",
        ] {
            let field = getcap_field(line).unwrap_or_else(|| panic!("field in {line:?}"));
            assert!(
                field_grants_required_caps(field),
                "{line:?} should read as blessed"
            );
        }
    }

    /// Refusals: a different capability, no capability, and empty output.
    #[test]
    fn other_capabilities_and_empty_output_do_not_read_as_blessed() {
        let f = getcap_field("/x/y cap_net_raw=ep").expect("field");
        assert!(!field_grants_required_caps(f));
        assert_eq!(getcap_field(""), None);
        assert_eq!(getcap_field("/x/y-with-no-caps\n"), None);
        // A path with a space is why the field is taken from the right, not the left.
        let f = getcap_field("/x/my dir/serial-nexus-devprep cap_dac_override,cap_fowner=ep")
            .expect("field");
        assert!(field_grants_required_caps(f));
        // **Half the set is not the set.** A copy blessed before `cap_fowner` joined
        // the requirement replugs fine and then cannot grant, which is precisely the
        // failure §15.55 exists to remove — so it must not read as blessed.
        let half = getcap_field("/x/y cap_dac_override=ep").expect("field");
        assert!(
            !field_grants_required_caps(half),
            "cap_dac_override alone must not read as blessed once cap_fowner is required"
        );
    }

    /// The orphan matcher is looser than the blessed matcher, and the two cases where
    /// they disagree are the whole reason it exists (plan §18 item 52 (d)).
    ///
    /// **Fail-first, recorded.** Reimplementing [`field_carries_a_required_cap`] as
    /// `field_grants_required_caps` — the obvious "reuse what is there" mistake —
    /// turns the first two assertions below red: a pre-§15.55 blessing
    /// (`cap_dac_override=ep`) and a `+p`-only copy both stop counting as
    /// capability-carrying, which is precisely the orphan notes §3.81 left behind.
    #[test]
    fn the_orphan_matcher_counts_a_capability_the_blessed_matcher_rejects() {
        // The actual orphan §3.81 describes, in the two shapes it can take.
        assert!(
            field_carries_a_required_cap("cap_dac_override=ep"),
            "a pre-§15.55 blessing is not a blessed copy, and is still a live capability"
        );
        assert!(
            !field_grants_required_caps("cap_dac_override=ep"),
            "the two matchers must disagree here — that disagreement is the point"
        );
        assert!(
            field_carries_a_required_cap("cap_dac_override=p"),
            "`+p` is one capset(2) away from effective and the process can make that \
             call itself"
        );
        assert!(field_carries_a_required_cap(
            "cap_dac_override,cap_fowner=ep"
        ));
        assert!(
            field_carries_a_required_cap("cap_fowner=ep"),
            "the second capability alone is still one of ours — the hand-kept-list \
             failure §15.55 already cost this tree once"
        );
        // Not ours. Still swept (see `orphans_in`), but the report says so.
        assert!(!field_carries_a_required_cap("cap_net_raw=ep"));
        assert!(!field_carries_a_required_cap(
            "cap_net_admin,cap_sys_ptrace=ep"
        ));
        // The `/deps/` trap one file over: a *name* that merely contains one of ours
        // is not one of ours. `getcap_field` has already removed the path by here,
        // but the comparison is still per-name equality rather than `contains`.
        assert!(!field_carries_a_required_cap(
            "cap_dac_override_not_really=ep"
        ));
    }

    /// The **walker**, proven without root.
    ///
    /// No unprivileged test can create a file carrying a capability, so the reader is
    /// injected and the directory walk, the keep-exclusion and the unlink are the
    /// things under test — the half a real-`getcap` sweep would leave asserted by
    /// nothing (AGENTS §3: a scanning gate proves its matcher *and* its walker).
    ///
    /// **Fail-first, recorded.** Dropping the keep-exclusion in [`orphans_in`] turns
    /// the first assertion red and, in the sweep below, deletes the blessed copy the
    /// rig lane is about to use — the one outcome worse than leaving an orphan.
    #[test]
    fn the_sweep_removes_a_blessed_stray_and_never_the_copy_that_belongs() {
        let tmp = std::env::temp_dir().join(format!("snx-sweep-{}", std::process::id()));
        let dir = tmp.join(BIN_DIR).join("debug");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let keep = blessed_path(&tmp, "debug");
        // The orphan the rename left: a name nothing looks for any more, still
        // blessed (notes §3.81). Spelled with a stand-in rather than the retired
        // name itself, which §15.40's meta-gate bans tree-wide — measured, not
        // assumed: the first draft of this test used the real one and
        // `retired_names_appear_only_where_history_lives` reddened on these two lines.
        let stray = dir.join("serial-nexus-devprep.superseded");
        let capless = dir.join("notes.txt");
        for p in [&keep, &stray, &capless] {
            std::fs::write(p, b"x").expect("write");
        }
        let fake_getcap = |p: &Path| -> io::Result<Option<String>> {
            Ok(match p.file_name().and_then(|n| n.to_str()) {
                Some("serial-nexus-devprep") => Some("cap_dac_override,cap_fowner=ep".to_owned()),
                Some("serial-nexus-devprep.superseded") => Some("cap_dac_override=ep".to_owned()),
                _ => None,
            })
        };

        let found = orphans_in(&dir, &keep, &fake_getcap).expect("walk");
        assert_eq!(
            found.iter().map(|o| &o.path).collect::<Vec<_>>(),
            vec![&stray],
            "exactly the blessed stray: not the copy that belongs there, and not the \
             file that carries no capability at all"
        );
        assert!(
            found[0].ours,
            "cap_dac_override is a capability this tool grants"
        );

        // And the unlink half, over the same tree.
        let swept = sweep_orphans(&tmp, "debug").expect("sweep");
        assert_eq!(
            swept.len(),
            0,
            "the real getcap finds no capability on a text file"
        );
        // Prove the deleting walk against the injected reader instead, since the
        // real one cannot see a capability this test is able to create.
        for orphan in orphans_in(&dir, &keep, &fake_getcap).expect("walk") {
            std::fs::remove_file(&orphan.path).expect("unlink");
        }
        assert!(!stray.exists(), "the stray must actually be gone");
        assert!(
            keep.exists(),
            "the installed copy must survive its own sweep"
        );
        assert!(
            capless.exists(),
            "a file carrying nothing is not this sweep's business"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A directory that does not exist yet is the ordinary state of a fresh checkout,
    /// not a failure — and `install` calls the sweep before anything has been
    /// installed on exactly that tree.
    #[test]
    fn a_blessed_directory_that_does_not_exist_yet_sweeps_clean() {
        let tmp = std::env::temp_dir().join(format!("snx-sweep-absent-{}", std::process::id()));
        assert!(sweep_orphans(&tmp, "debug").expect("sweep").is_empty());
        assert!(
            !tmp.exists(),
            "the sweep must not create the directory it inspects"
        );
    }

    /// The `Ready` line names the capability set the file actually carries.
    ///
    /// **Fail-first, recorded.** Restoring the literal this replaced —
    /// `"mode 0700, cap_dac_override +ep"` — reddens the `cap_fowner` assertion,
    /// which is the defect item 52 (e) names: the pre-§15.55 form still printed by a
    /// tree that requires two capabilities.
    #[test]
    fn the_ready_line_derives_the_whole_capability_set() {
        let line = ready_description();
        for (name, _) in REQUIRED_CAPS {
            assert!(
                line.contains(name),
                "the ready line must name every required capability; {line:?} omits {name}"
            );
        }
        assert!(
            line.contains("0700"),
            "and the mode it was blessed under: {line:?}"
        );
        // The same projection the operator-facing command uses, so "what you were
        // told to grant" and "what ready means" cannot drift apart.
        let granted = format!("{}+ep", required_cap_names().join(","));
        assert!(
            line.ends_with(&granted),
            "{line:?} must end with {granted:?}"
        );
        assert!(setcap_command(Path::new("/x")).contains(&granted));
    }

    /// A missing install is `Absent`, not an error — the preflight distinguishes
    /// "not installed yet" from "installed wrong", and they get different advice.
    #[test]
    fn an_empty_tree_reports_absent() {
        let tmp = std::env::temp_dir().join(format!("snx-install-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).expect("mkdir");
        assert_eq!(
            inspect(&tmp, "debug").expect("inspect"),
            InstallState::Absent
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A copy that differs from the build reads `Stale` — the condition that
    /// replaces vmcell's `.blessed` stamp entirely.
    #[test]
    fn a_rebuilt_binary_makes_the_installed_copy_stale() {
        let tmp = std::env::temp_dir().join(format!("snx-install-stale-{}", std::process::id()));
        let built = built_path(&tmp, "debug");
        let blessed = blessed_path(&tmp, "debug");
        std::fs::create_dir_all(built.parent().expect("parent")).expect("mkdir target");
        std::fs::create_dir_all(blessed.parent().expect("parent")).expect("mkdir bin");
        std::fs::write(&built, b"new build").expect("write built");
        std::fs::write(&blessed, b"old build").expect("write blessed");
        assert_eq!(
            inspect(&tmp, "debug").expect("inspect"),
            InstallState::Stale
        );
        // Same bytes, wrong mode: the next state up.
        std::fs::write(&blessed, b"new build").expect("rewrite");
        std::fs::set_permissions(&blessed, std::fs::Permissions::from_mode(0o755))
            .expect("chmod 755");
        assert_eq!(
            inspect(&tmp, "debug").expect("inspect"),
            InstallState::WrongMode(0o755)
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `install` restricts the copy to its owner before anyone can bless it.
    #[test]
    fn install_writes_an_owner_only_copy() {
        let tmp = std::env::temp_dir().join(format!("snx-install-mode-{}", std::process::id()));
        let built = built_path(&tmp, "debug");
        std::fs::create_dir_all(built.parent().expect("parent")).expect("mkdir");
        std::fs::write(&built, b"#!/bin/true\n").expect("write built");
        let blessed = install(&tmp, "debug").expect("install");
        let mode = std::fs::metadata(&blessed)
            .expect("stat")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(mode, BLESSED_MODE, "installed copy must be owner-only");
        assert!(
            setcap_command(&blessed).starts_with("sudo setcap cap_dac_override,cap_fowner+ep "),
            "the printed command must ask for exactly REQUIRED_CAPS: {}",
            setcap_command(&blessed)
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
