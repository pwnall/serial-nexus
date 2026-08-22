//! The bless recipe: install a copy of this binary at a stable, mode-restricted
//! path so `setcap` has something durable to bless (design §15.45).
//!
//! Why a copy at all, rather than blessing `target/<profile>/serial-nexus-devprep`
//! in place: the kernel clears `security.capability` on every write to a file, and
//! `cargo build` rewrites that path constantly, so an in-place blessing would be
//! destroyed by the next build and would have to be re-applied — with a `sudo` —
//! on every rebuild. The stable copy is touched only by `install`.
//!
//! **Nothing in this crate spawns a process, and as of 2026-08-21 that is enforced
//! rather than argued** (design §15.71, plan §18 items 103 and 104).
//!
//! This module used to answer "what does this file carry" by running `getcap`, and
//! the argument that made that safe was that `main` refuses `install` while any
//! capability in [`REQUIRED_CAPS`] is held — so the spawn and the capability could
//! never coexist. The argument was true of `install` and false of the module:
//! [`preflight`](super::preflight) calls [`inspect`] with no such refusal in front of
//! it, so a blessed copy execed a `PATH`-selected binary while holding
//! `cap_dac_override,cap_fowner`. Measured on the rig box twice by two agents, with a
//! shim that reads its *parent's* `/proc/<ppid>/status`: `CapPrm`/`CapEff`
//! `000000000000000a`, which is exactly those two bits — and a shim that answers
//! "carries nothing" flips the helper's verdict from `READY` to `BLOCKED-ON-BLESS`,
//! which is the environment deciding the answer of a binary whose first stated bound
//! is that it reads none.
//!
//! The capability set of a file *is* its `security.capability` extended attribute, so
//! the answer is one `getxattr(2)`, in `serial_nexus_sys::caps` because §16.3 puts
//! every syscall there. No `PATH` lookup, no child, no inherited environment, and no
//! dependency on libcap being installed — which `docs/vmcell-requirements.md` had
//! already recorded as breaking this module's own sweep test on a userland without
//! `getcap`. `itest/tests/meta_gates.rs`'s
//! `the_privileged_helper_neither_spawns_a_process_nor_reads_the_environment` is what
//! keeps it that way; before it, this was the one AGENTS §4 tripwire with no gate.
//!
//! **The blessed-copy refusal on `install` stays**, with the reason it should always
//! have carried: `install` is the one verb that writes files outside sysfs, and a
//! process holding `CAP_DAC_OVERRIDE` bypasses every permission check on that write.
//! The copy that places and mode-restricts the blessed binary should be the unblessed
//! one.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serial_nexus_sys::caps::FileCaps;

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
    /// Not `cap_dac_override` alone: [`grants_required_caps`] requires the
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

/// Render a file's capability set in `getcap`'s vocabulary, for a report that names
/// the grant rather than asserting one.
///
/// **Rendered, never re-read.** Until 2026-08-21 this module took `getcap`'s stdout
/// and every question below was a string test over it, which is why it carried a
/// careful field extractor and a regression test for a real defect vmcell hit:
/// `getcap` prints `<path> <caps>`, so a whole-line test for `ep` is satisfied by the
/// *path* (`target/debug/deps/…` contains those letters, and so does any user named
/// `steph`), and an un-raised `+p`-only binary read as blessed. Reading the xattr
/// deletes that whole class rather than guarding it: there is no line and no path to
/// mis-match, the two questions below are bit tests, and this function is the only
/// place a string is produced at all — printed by its caller and parsed by nobody.
///
/// The vocabulary is libcap's on purpose, so an operator can hold this tool's report
/// next to `getcap`'s and read the same words. Capabilities are grouped by the flags
/// they carry; the effective *flag* raises the whole set, so it prints as `e` on
/// every listed name exactly as libcap renders it; the flag letters go in libcap's
/// `e`,`i`,`p` order; and an attribute that grants nothing prints as libcap's bare
/// `=` rather than as an empty string, because "carries the attribute and grants
/// nothing" and "carries no attribute" are different answers and the sweep acts on
/// the difference.
///
/// **A capability this tool does not name prints as its kernel bit number** —
/// `cap_12` where `getcap` says `cap_net_admin`. Deliberate, and stated rather than
/// implied: the alternative is a forty-odd-entry name table in a tree whose recurring
/// defect is a hand-kept list going stale, the bit number is the kernel's own
/// identifier, and `capsh --decode=0x…` resolves it. [`REQUIRED_CAPS`] is the one
/// place a capability name is written here and it names exactly the two this tool
/// grants — which is the *right* two to name, because an unrecognised name in this
/// report is precisely the case a human is being asked to look at.
pub fn describe(caps: &FileCaps) -> String {
    fn name(bit: u32) -> String {
        REQUIRED_CAPS
            .iter()
            .find(|(_, b)| *b == bit)
            .map_or_else(|| format!("cap_{bit}"), |(n, _)| (*n).to_owned())
    }
    let rootid = match caps.rootid {
        // A revision-3 blob is honoured only inside a user namespace whose root maps
        // to this uid. Printing it like an ordinary blessing would describe a grant
        // that does not exist on the box reading it.
        Some(uid) => format!(" [namespaced, rootid {uid}]"),
        None => String::new(),
    };
    // Grouped by the (inheritable, permitted) pair, in bit order, which is both
    // libcap's grouping and a stable order for a string humans compare across runs.
    let mut groups: Vec<((bool, bool), Vec<String>)> = Vec::new();
    for bit in 0..64u32 {
        let mask = 1u64 << bit;
        let key = (caps.inheritable & mask != 0, caps.permitted & mask != 0);
        if key == (false, false) {
            continue;
        }
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, names)) => names.push(name(bit)),
            None => groups.push((key, vec![name(bit)])),
        }
    }
    if groups.is_empty() {
        return format!("={rootid}");
    }
    let clauses: Vec<String> = groups
        .iter()
        .map(|((inheritable, permitted), names)| {
            let mut flags = String::new();
            if caps.effective {
                flags.push('e');
            }
            if *inheritable {
                flags.push('i');
            }
            if *permitted {
                flags.push('p');
            }
            format!("{}={flags}", names.join(","))
        })
        .collect();
    format!("{}{rootid}", clauses.join(" "))
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
/// key on `cap_dac_override` alone while `grants_required_caps` required
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

/// Does this file carry any capability in [`REQUIRED_CAPS`] at all, in either mask
/// and whatever the effective flag says?
///
/// A deliberately looser question than [`grants_required_caps`], because it is a
/// different question. That one asks "is this copy blessed" and must be strict: every
/// capability, permitted, and the effective flag, or the helper would report itself
/// ready and then fail the write. This one asks "is this file one of *ours*", and the
/// two answers diverge in exactly the cases orphan hygiene is about. A copy blessed
/// before `cap_fowner` joined the set carries `cap_dac_override=ep`, is **not**
/// blessed by the strict test, and is still a root-equivalent capability sitting on a
/// file nothing looks for (notes §3.81). A `+p`-only copy is one `capset(2)` — a call
/// the process can make itself — away from effective, which is the same reasoning
/// `unhardened_disposition` uses to treat `+p` as privilege. And a capability sitting
/// only in the *inheritable* mask is still a capability on a file, which is why this
/// asks [`FileCaps::carries`] rather than [`FileCaps::permits`].
///
/// Used only to *describe* what the sweep found. What the sweep removes is decided by
/// [`orphans_in`], on the broader "carries the attribute at all" test, for the reason
/// recorded there.
pub fn carries_a_required_cap(caps: &FileCaps) -> bool {
    REQUIRED_CAPS.iter().any(|(_, bit)| caps.carries(*bit))
}

/// A capability-carrying file in the blessed directory that is not the copy
/// `install` put there.
#[derive(Debug, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    /// What the file's `security.capability` attribute decodes to, as bits.
    pub caps: FileCaps,
    /// [`caps`](Self::caps) in `getcap`'s vocabulary, so the report names the grant
    /// rather than asserting one. Rendered by [`describe`] from the bits above, which
    /// is why the two can never disagree.
    pub grant: String,
    /// Whether [`caps`](Self::caps) includes a capability this tool grants
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
/// without root: no unprivileged test can create a file carrying a capability
/// (`setcap` needs `CAP_SETFCAP`, and this box's unprivileged-userns route is closed),
/// so a sweep that only ever ran against the real reader would have its directory walk
/// and its keep-exclusion asserted by nothing (AGENTS §3 — a scanning gate proves its
/// matcher *and* its walker). The injection also survived the reader changing
/// underneath it: it was shaped around `getcap`'s stdout and takes the xattr decoding
/// unchanged, because what it abstracts is "what does this file carry", not "what did
/// that subprocess print".
///
/// Symlinks are skipped: `read_dir`'s file type does not follow them, and a symlink
/// carries no capability of its own — only its target does, and if that target is in
/// this directory it is visited on its own account.
pub fn orphans_in(
    dir: &Path,
    keep: &Path,
    read: &dyn Fn(&Path) -> io::Result<Option<FileCaps>>,
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
        if let Some(caps) = read(&path)? {
            found.push(Orphan {
                ours: carries_a_required_cap(&caps),
                grant: describe(&caps),
                caps,
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

/// Whether this file grants **every** capability in [`REQUIRED_CAPS`] *effectively*.
///
/// Permitted without the effective flag is permitted-but-not-raised: the exec'd
/// helper would have to `capset(2)` for itself and, as written, would fail the sysfs
/// write instead. So both halves are required — every bit in the permitted mask, and
/// `VFS_CAP_FLAGS_EFFECTIVE` set — and each is asked of the mask rather than of a
/// rendering, so the order libcap happens to print names in, and any extra capability
/// beyond the two asked for, are both irrelevant by construction rather than by a
/// parser that remembered to allow for them.
///
/// Revision-agnostic on purpose: a namespaced (revision 3) blob answers here on its
/// bits like any other, and [`describe`] is what tells the operator it is namespaced.
/// The authoritative answer to "does the process hold this" is not a file reading at
/// all — it is `serial_nexus_sys::caps::capability_state`, which `main` already takes
/// from `/proc` and which is what every refusal keys on.
pub fn grants_required_caps(caps: &FileCaps) -> bool {
    caps.effective && REQUIRED_CAPS.iter().all(|(_, bit)| caps.permits(*bit))
}

/// Ask the **kernel** what the file carries. `Ok(None)` when it carries nothing.
///
/// One `getxattr(2)` on `security.capability`, which is where a file capability
/// actually lives — see the module doc for what this replaced and what it cost while
/// it stood. Kept as a named function rather than inlined at its two call sites
/// because it is the default the sweep's injection point substitutes for, and because
/// naming it is what lets the module doc point at one place.
fn read_caps(path: &Path) -> io::Result<Option<FileCaps>> {
    serial_nexus_sys::caps::file_capabilities(path)
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
        Some(caps) if grants_required_caps(&caps) => Ok(InstallState::Ready),
        Some(caps) => Ok(InstallState::Unblessed(describe(&caps))),
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

/// The remedy for a state, **keyed on the state** rather than assumed to be the
/// privileged one.
///
/// Until 2026-08-21 every non-`Ready` state printed [`setcap_command`], which is
/// the right answer for exactly one of the four. For `Stale` it is actively wrong:
/// the installed copy differs from the build, so re-applying a capability it
/// already carries changes nothing, and the operator is sent to `sudo` for a
/// repair that needs no privilege at all. That misreading cost a session its
/// replug lane — it read `Stale`, saw a `sudo` in the remedy, found the box wanted
/// a password, and recorded the lane as blocked while the helper was correctly
/// blessed the whole time (plan §18 item 101).
///
/// Only `Unblessed` is a privileged repair. `Stale`, `Absent` and `WrongMode` are
/// all fixed by the unprivileged copy `scripts/bless` performs — and when a
/// `setcap` genuinely follows it, `scripts/bless` is what runs it, so naming the
/// script is never an under-answer.
pub fn remedy_for(state: &InstallState, profile: &str, blessed: &Path) -> String {
    let bless = if profile == "release" {
        "scripts/bless --release"
    } else {
        "scripts/bless"
    };
    match state {
        // Already correct; callers do not ask, but a total function has no hole.
        InstallState::Ready => format!("nothing to do ({})", ready_description()),
        InstallState::Unblessed(_) => setcap_command(blessed),
        InstallState::Stale => format!(
            "{bless}   (the installed copy is not this build — it needs replacing, \
             not re-capping, and the copy itself needs no privilege)"
        ),
        InstallState::Absent => format!("{bless}   (nothing is installed yet)"),
        InstallState::WrongMode(_) => {
            format!("{bless}   (the copy is installed but its mode is not 0700)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_nexus_sys::caps::{CAP_DAC_OVERRIDE, CAP_FOWNER};

    /// **Only one of the four repairs is privileged, and the report used to claim
    /// all of them were.** Fail-first: with `remedy_for` replaced by
    /// `setcap_command` — which is what every arm printed until 2026-08-21 — the
    /// three unprivileged rows below fail, naming the state that was mis-advised.
    ///
    /// The row that matters is `Stale`. It is reachable on a correctly blessed tree
    /// for a reason that has nothing to do with capabilities: `inspect` compares the
    /// installed copy against `target/<profile>/serial-nexus-devprep`, a hardlink
    /// cargo re-points at whichever `deps/` artifact matches the last build's unit
    /// graph, and a workspace-scoped build produces a different devprep binary than
    /// `-p serial-nexus-devprep` does. Sending that operator to `sudo` is how a
    /// session came to record its replug lane as blocked while the helper was
    /// blessed (plan §18 item 101).
    #[test]
    fn only_the_unblessed_state_is_advised_to_reach_for_privilege() {
        let path = Path::new("/repo/.snx-bin/debug/serial-nexus-devprep");
        let privileged = [InstallState::Unblessed("(none)".to_owned())];
        let unprivileged = [
            InstallState::Stale,
            InstallState::Absent,
            InstallState::WrongMode(0o755),
        ];

        for state in &privileged {
            let r = remedy_for(state, "debug", path);
            assert!(
                r.contains("sudo") && r.contains("setcap"),
                "{state:?} is the one repair only root can make, so its remedy must \
                 name the privileged command: {r}"
            );
        }
        for state in &unprivileged {
            let r = remedy_for(state, "debug", path);
            assert!(
                !r.contains("sudo") && !r.contains("setcap"),
                "{state:?} is repaired by an unprivileged copy, so its remedy must \
                 not send the operator to root — that is the misreading item 101 \
                 records: {r}"
            );
            assert!(
                r.contains("scripts/bless"),
                "{state:?}'s remedy must name the command that actually repairs it: {r}"
            );
        }

        // The profile reaches the remedy, so a release-profile operator is not told
        // to run the debug repair.
        assert!(
            remedy_for(&InstallState::Stale, "release", path).contains("scripts/bless --release"),
            "the release profile's remedy must name --release"
        );
        assert!(
            !remedy_for(&InstallState::Stale, "debug", path).contains("--release"),
            "the debug profile's remedy must not name --release"
        );

        // A total function: `Ready` has an answer rather than a panic or a hole.
        assert!(remedy_for(&InstallState::Ready, "debug", path).contains("nothing to do"));
    }

    /// A capability set in the terms `setcap` is given them, for the tests below.
    ///
    /// They build `FileCaps` directly because **no unprivileged test can create a
    /// capability-carrying file** — `setcap` needs `CAP_SETFCAP`, and this box's
    /// unprivileged-userns route is closed (`unshare -Ur` answers `Operation not
    /// permitted` writing `uid_map`). That is the same reason
    /// `serial_nexus_sys::caps::parse_file_capability_xattr` takes bytes: the
    /// decoding is pinned there against the rig's real twenty-byte blob, and the
    /// *decisions* are pinned here against the bits it produces.
    fn caps_of(permitted: &[u32], inheritable: &[u32], effective: bool) -> FileCaps {
        let mask = |bits: &[u32]| bits.iter().fold(0u64, |m, b| m | (1u64 << b));
        FileCaps {
            permitted: mask(permitted),
            inheritable: mask(inheritable),
            effective,
            rootid: None,
        }
    }

    /// Permitted without the effective flag is not blessed — **and the trap that used
    /// to make this hard is now unrepresentable.**
    ///
    /// The test this replaces was called
    /// `a_path_containing_the_flag_letters_does_not_read_as_blessed`, and it existed
    /// because the answer arrived as a `getcap` *line*: `getcap` prints
    /// `<path> <caps>`, so a whole-line test for `ep` was satisfied by
    /// `/home/steph/repo/target/debug/deps/serial-nexus-devprep cap_dac_override=p` —
    /// a `+p`-only binary reading as blessed off the letters in its own path. The
    /// input is now a 20-byte kernel record with no path in it and the effective flag
    /// is one bit, so there is no line to mis-match and no spelling of that defect
    /// left. What survives, and is what the old test was actually protecting, is the
    /// decision: `+p` is not blessed (§15.71, plan §18 item 103).
    #[test]
    fn a_permitted_only_blessing_is_not_blessed_however_it_is_spelled() {
        let plus_p = caps_of(&[CAP_DAC_OVERRIDE, CAP_FOWNER], &[], false);
        assert!(
            !grants_required_caps(&plus_p),
            "the helper would have to capset(2) for itself and, as written, fails the \
             sysfs write instead — a report of `ready` here is a skip that reads as a pass"
        );
        assert!(
            carries_a_required_cap(&plus_p),
            "...and it is still privilege sitting on a file, which is the orphan sweep's \
             question, not this one"
        );
        // The rendering says `p` and not `ep`, so the operator-facing line and the
        // decision cannot disagree.
        assert_eq!(describe(&plus_p), "cap_dac_override,cap_fowner=p");
    }

    /// The required pair reads as blessed, with or without capabilities beyond it,
    /// and in no particular order — because there **is** no order.
    ///
    /// The test this replaces walked four textual spellings
    /// (`cap_dac_override,cap_fowner=ep`, `cap_fowner,cap_dac_override=pe`, and two
    /// with a third capability mixed in) because libcap is free to print names in any
    /// order. Asking the mask removes the axis: the same four cases are the same two
    /// bits, and only the extra-capability case still says anything, which it does.
    #[test]
    fn the_required_pair_reads_as_blessed_even_beside_capabilities_nobody_asked_for() {
        assert!(grants_required_caps(&caps_of(
            &[CAP_DAC_OVERRIDE, CAP_FOWNER],
            &[],
            true
        )));
        // A third capability does not un-bless the file. It *is* worth a human's eye,
        // and the report is where that happens — `describe` names it, by number,
        // because `REQUIRED_CAPS` is the only place a name is written.
        let extra = caps_of(&[CAP_DAC_OVERRIDE, CAP_FOWNER, 12], &[], true);
        assert!(grants_required_caps(&extra));
        assert_eq!(describe(&extra), "cap_dac_override,cap_fowner,cap_12=ep");
    }

    /// Refusals: a capability that is not ours, an attribute that grants nothing, and
    /// **half the set**.
    #[test]
    fn a_foreign_capability_an_empty_attribute_and_half_the_set_do_not_read_as_blessed() {
        // cap_net_raw is 13. Not ours; not blessed.
        assert!(!grants_required_caps(&caps_of(&[13], &[], true)));
        // The attribute is present and grants nothing. `getcap` spells that `=`, and
        // so does this — because it is a different answer from "no attribute", which
        // the reader returns as `None` and never reaches here.
        let nothing = caps_of(&[], &[], true);
        assert!(!grants_required_caps(&nothing));
        assert_eq!(describe(&nothing), "=");
        assert!(nothing.is_empty());
        // **Half the set is not the set.** A copy blessed before `cap_fowner` joined
        // the requirement replugs fine and then cannot grant, which is precisely the
        // failure §15.55 exists to remove — so it must not read as blessed.
        assert!(
            !grants_required_caps(&caps_of(&[CAP_DAC_OVERRIDE], &[], true)),
            "cap_dac_override alone must not read as blessed once cap_fowner is required"
        );
        assert!(
            !grants_required_caps(&caps_of(&[CAP_FOWNER], &[], true)),
            "and neither must the other half"
        );
    }

    /// The orphan matcher is looser than the blessed matcher, and the two cases where
    /// they disagree are the whole reason it exists (plan §18 item 52 (d)).
    ///
    /// **Fail-first, recorded.** Reimplementing [`carries_a_required_cap`] as
    /// [`grants_required_caps`] — the obvious "reuse what is there" mistake — turns
    /// the first two assertions below red: a pre-§15.55 blessing
    /// (`cap_dac_override=ep`) and a `+p`-only copy both stop counting as
    /// capability-carrying, which is precisely the orphan notes §3.81 left behind.
    #[test]
    fn the_orphan_matcher_counts_a_capability_the_blessed_matcher_rejects() {
        // The actual orphan §3.81 describes, in the two shapes it can take.
        let pre_55 = caps_of(&[CAP_DAC_OVERRIDE], &[], true);
        assert!(
            carries_a_required_cap(&pre_55),
            "a pre-§15.55 blessing is not a blessed copy, and is still a live capability"
        );
        assert!(
            !grants_required_caps(&pre_55),
            "the two matchers must disagree here — that disagreement is the point"
        );
        assert!(
            carries_a_required_cap(&caps_of(&[CAP_DAC_OVERRIDE], &[], false)),
            "`+p` is one capset(2) away from effective and the process can make that \
             call itself"
        );
        assert!(carries_a_required_cap(&caps_of(
            &[CAP_DAC_OVERRIDE, CAP_FOWNER],
            &[],
            true
        )));
        assert!(
            carries_a_required_cap(&caps_of(&[CAP_FOWNER], &[], true)),
            "the second capability alone is still one of ours — the hand-kept-list \
             failure §15.55 already cost this tree once"
        );
        // **Inheritable-only**, which the text era could see only if libcap happened
        // to print the name, and which the bits make unmissable: a capability in the
        // inheritable mask is a capability on the file.
        assert!(
            carries_a_required_cap(&caps_of(&[], &[CAP_DAC_OVERRIDE], false)),
            "`carries` is the union of both masks; `permits` is not"
        );
        assert!(!grants_required_caps(&caps_of(
            &[],
            &[CAP_DAC_OVERRIDE, CAP_FOWNER],
            true
        )));
        // Not ours. Still swept (see `orphans_in`), but the report says so.
        assert!(!carries_a_required_cap(&caps_of(&[13], &[], true)));
        assert!(!carries_a_required_cap(&caps_of(&[12, 19], &[], true)));
    }

    /// The report speaks `getcap`'s dialect, checked against `getcap`'s **actual
    /// output on this repository's own blessed copy**.
    ///
    /// Measured 2026-08-21 on the rig box:
    ///
    /// ```text
    /// $ getcap .snx-bin/debug/serial-nexus-devprep
    /// .snx-bin/debug/serial-nexus-devprep cap_dac_override,cap_fowner=ep
    /// $ python3 -c 'import os;print(os.getxattr("...","security.capability").hex())'
    /// 010000020a000000000000000000000000000000
    /// ```
    ///
    /// The right-hand side of that pair is the fixture in
    /// `serial_nexus_sys::caps`'s decoder test; the left-hand side is the string
    /// below. Between them the whole path from twenty kernel bytes to the sentence an
    /// operator reads is pinned to a stock tool's answer on real hardware — which is
    /// what makes "the vocabulary is libcap's" a claim rather than an intention.
    #[test]
    fn the_rendered_grant_matches_what_getcap_prints_for_the_real_blessing() {
        assert_eq!(
            describe(&caps_of(&[CAP_DAC_OVERRIDE, CAP_FOWNER], &[], true)),
            "cap_dac_override,cap_fowner=ep"
        );
    }

    /// The renderer's remaining shapes: two flag groups, a capability with no name
    /// here, and a namespaced blob.
    ///
    /// None of these is what `setcap` writes for this tool, which is exactly why they
    /// are asserted — the report exists for the file *nobody expected*, and a renderer
    /// that only ever produced the expected string would be a report that cannot
    /// describe a surprise.
    #[test]
    fn the_renderer_groups_by_flags_numbers_what_it_cannot_name_and_marks_a_namespaced_blob() {
        // Two groups: permitted-and-effective, then inheritable-and-effective. libcap
        // groups the same way and prints the groups in capability order.
        assert_eq!(
            describe(&caps_of(&[CAP_DAC_OVERRIDE], &[CAP_FOWNER], true)),
            "cap_dac_override=ep cap_fowner=ei"
        );
        // The effective flag is one bit for the whole file: clear it and no group
        // carries `e`.
        assert_eq!(
            describe(&caps_of(&[CAP_DAC_OVERRIDE], &[CAP_FOWNER], false)),
            "cap_dac_override=p cap_fowner=i"
        );
        // A capability this tool does not name prints as the kernel's own number.
        // `capsh --decode` resolves it; a hand-kept table of forty-odd names would not
        // stay true, and this report's whole job is to be true about a surprise.
        assert_eq!(describe(&caps_of(&[21], &[], true)), "cap_21=ep");
        // A high bit, which is the half of the decoder that lives in the second
        // 32-bit pair.
        assert_eq!(describe(&caps_of(&[40], &[], true)), "cap_40=ep");
        // Revision 3: honoured only inside a user namespace whose root maps to this
        // uid, so the report must not print it as an ordinary blessing.
        let namespaced = FileCaps {
            rootid: Some(1000),
            ..caps_of(&[CAP_DAC_OVERRIDE, CAP_FOWNER], &[], true)
        };
        assert_eq!(
            describe(&namespaced),
            "cap_dac_override,cap_fowner=ep [namespaced, rootid 1000]"
        );
        assert!(
            grants_required_caps(&namespaced),
            "the bits are the bits; whether this box's namespace honours them is what \
             `capability_state` answers at run time, and the report is what says the \
             question exists"
        );
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
        // The injected reader, now answering in bits rather than in `getcap`'s
        // stdout. The shape of the injection did not have to change, because what it
        // abstracts is "what does this file carry" and not "what did that subprocess
        // print" — which is the test this change had to survive and did.
        let fake_reader = |p: &Path| -> io::Result<Option<FileCaps>> {
            Ok(match p.file_name().and_then(|n| n.to_str()) {
                Some("serial-nexus-devprep") => {
                    Some(caps_of(&[CAP_DAC_OVERRIDE, CAP_FOWNER], &[], true))
                }
                Some("serial-nexus-devprep.superseded") => {
                    Some(caps_of(&[CAP_DAC_OVERRIDE], &[], true))
                }
                _ => None,
            })
        };

        let found = orphans_in(&dir, &keep, &fake_reader).expect("walk");
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
        assert_eq!(
            found[0].grant, "cap_dac_override=ep",
            "the report names the grant it found, rendered from the same bits `ours` \
             was decided on, so the sentence and the decision cannot drift apart"
        );

        // And the unlink half, over the same tree.
        let swept = sweep_orphans(&tmp, "debug").expect("sweep");
        assert_eq!(
            swept.len(),
            0,
            "the real reader finds no capability on a text file — and it now reaches \
             that answer through `getxattr(2)`, so this line no longer needs libcap to \
             be installed for the test to run at all (it needed it, and \
             `docs/vmcell-requirements.md` recorded this exact test failing on a \
             userland that ships no `getcap`)"
        );
        // Prove the deleting walk against the injected reader instead, since the
        // real one cannot see a capability this test is able to create.
        for orphan in orphans_in(&dir, &keep, &fake_reader).expect("walk") {
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
