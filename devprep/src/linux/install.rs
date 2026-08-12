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
//! unreachable from a blessed process**: `main` refuses `install` when the
//! capability is held, so `CAP_DAC_OVERRIDE` and `Command::new` never coexist.

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
    /// Installed, byte-identical to the build, mode `0700`, and carrying
    /// `cap_dac_override` with the effective bit.
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

/// The capabilities a blessed copy must carry, and why each is there.
///
/// `cap_dac_override` writes `authorized` in sysfs (`root:root 0644`);
/// `cap_fowner` sets a POSIX ACL on a tty node the invoking user does not own
/// (§15.55). Both are required: a copy holding only the first can replug but
/// cannot hand back access to the node it just recreated, which is the shape the
/// rig lane failed on.
pub const REQUIRED_CAPS: &[&str] = &["cap_dac_override", "cap_fowner"];

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
    let all_named = REQUIRED_CAPS.iter().all(|want| present.contains(want));
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
        REQUIRED_CAPS.join(","),
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
