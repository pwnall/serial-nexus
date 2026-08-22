//! Capability inspection and process hardening for the privileged replug helper
//! (design §15.45).
//!
//! This module exists here rather than in `devprep/` for the reason the whole crate
//! exists (§16.3): `prctl(2)` needs `unsafe` somewhere, and there is exactly one
//! crate allowed to hold it. The capability *reader* needs no `unsafe` at all — it
//! parses `/proc/self/status` — but it lives beside the hardening calls so that
//! everything this workspace knows about Linux capabilities is one file.
//!
//! Nothing here grants privilege. Both capabilities below reach the helper from the
//! *file* capabilities `setcap` applied to the installed copy; these functions only
//! observe that state and shrink what the process can do afterwards. The set itself
//! is written in exactly one place — `REQUIRED_CAPS` in
//! `devprep/src/linux/install.rs` — and this module names the bits, never the set.

/// Bit number of `CAP_DAC_OVERRIDE` in the capability bitmasks (`linux/capability.h`).
///
/// The **first of the two** capabilities the replug helper is blessed with (§15.55;
/// [`CAP_FOWNER`] is the other), and by far the larger grant: it bypasses every DAC
/// permission check on read and write, so a process holding it can write
/// `/etc/shadow`. The helper's narrowness — argv-only, no environment, no `exec`, one
/// kernel-verified sysfs write — is what bounds it, not the capability itself
/// (§15.45).
pub const CAP_DAC_OVERRIDE: u32 = 1;

/// Bit number of `CAP_FOWNER` in the capability bitmasks (`linux/capability.h`).
///
/// The second capability the replug helper carries, and the *only* reason it does:
/// setting a POSIX ACL on a file you do not own requires it. The tty nodes a USB
/// serial adapter produces are `root:dialout`, so granting the invoking user access
/// to one is a `setxattr` the kernel refuses without this bit (§15.55).
///
/// It is not `CAP_CHOWN` deliberately. Chown would also solve the problem and is a
/// strictly larger grant: it lets a process *give files away* as well as take them,
/// and it would destroy the node's original ownership rather than adding one entry
/// beside it. `CAP_FOWNER` plus an ACL leaves the node reading `root:dialout 0660`
/// with one extra `user:<uid>:rw-` line that `getfacl` shows and `setfacl -b`
/// removes.
pub const CAP_FOWNER: u32 = 3;

/// The **real** uid of the invoking user.
///
/// The helper is `setcap`'d, never `setuid`, so it runs as whoever launched it and
/// this is that user. It exists so the grant verb can name its beneficiary from the
/// kernel rather than from argv: a `--uid` flag on a capability-carrying binary
/// would let any caller hand privilege to any account, which is the one thing the
/// argv-only bound (§15.45) is for.
pub fn real_uid() -> u32 {
    // Safety: `getuid` takes no arguments, reads no memory, and cannot fail.
    unsafe { libc::getuid() }
}

/// Whether a capability sits in this process's permitted and effective sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapState {
    /// In the permitted set: the process *may* raise it into effective.
    pub permitted: bool,
    /// In the effective set: the kernel honours it on the next privileged check.
    pub effective: bool,
}

impl CapState {
    /// Neither permitted nor effective — an unblessed process.
    pub const NONE: Self = Self {
        permitted: false,
        effective: false,
    };
}

/// Read one capability's state for the calling process from `/proc/self/status`.
///
/// `/proc` rather than `capget(2)` on purpose: it needs no `unsafe`, it is stable
/// ABI, and it is the same source `getpcaps` reads, so an operator can check the
/// helper's own report against a stock tool.
///
/// Returns [`CapState::NONE`] when `/proc` is unreadable or the fields are absent,
/// because every caller treats "cannot prove the capability is held" and "not held"
/// identically — the privileged verbs refuse in both cases.
pub fn capability_state(bit: u32) -> CapState {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| parse_capability_state(&s, bit))
        .unwrap_or(CapState::NONE)
}

/// The pure half of [`capability_state`], split out so it is unit-testable against
/// real `/proc/self/status` text without needing the capability to be held.
///
/// `CapPrm`/`CapEff` are 16-digit lowercase hex bitmasks. Returns `None` when either
/// field is missing or unparseable — never a silent `false`, which would read as
/// "not held" and hide a format change.
pub fn parse_capability_state(status: &str, bit: u32) -> Option<CapState> {
    fn field(status: &str, name: &str) -> Option<u64> {
        status.lines().find_map(|line| {
            let rest = line.strip_prefix(name)?.strip_prefix(':')?;
            u64::from_str_radix(rest.trim(), 16).ok()
        })
    }
    let mask = 1u64.checked_shl(bit)?;
    Some(CapState {
        permitted: field(status, "CapPrm")? & mask != 0,
        effective: field(status, "CapEff")? & mask != 0,
    })
}

/// The capability set a **file** carries: what `setcap` writes into the
/// `security.capability` extended attribute and what `getcap` prints back.
///
/// Deliberately a different type from [`CapState`], because it answers a different
/// question. [`CapState`] is about *this process* and is read from `/proc`; this is
/// about a *file on disk*, and the two are connected only by `execve` — file
/// capabilities become process capabilities at exec and at no other moment. The
/// replug helper needs both: the process reading decides what it may do right now
/// (and whether it must refuse to run unhardened), the file reading is what
/// `install --verify` and `preflight` report about the installed copy.
///
/// The masks are bit `n` = capability `n`, the same numbering [`CAP_DAC_OVERRIDE`]
/// and [`CAP_FOWNER`] are written in. This module names the bits and never the set
/// (`REQUIRED_CAPS` in `devprep/src/linux/install.rs` is the set's one home), so
/// there is deliberately no name table here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileCaps {
    /// The permitted mask — what the process may hold after `execve`.
    pub permitted: u64,
    /// The inheritable mask. Almost always zero on a `setcap …+ep` blessing, and
    /// still read, because a capability sitting only here is a capability sitting on
    /// the file (see the orphan sweep in `devprep`).
    pub inheritable: u64,
    /// `VFS_CAP_FLAGS_EFFECTIVE`: **one flag for the whole file, not a mask.** When
    /// set, the kernel raises the permitted set into the effective set at `execve`;
    /// when clear the exec'd process starts with permitted-but-not-raised and must
    /// `capset(2)` for itself. This is the `e` in `getcap`'s `=ep`, and its absence
    /// is the `+p`-only state the helper must not read as blessed.
    pub effective: bool,
    /// The revision-3 (namespaced) blob's owning uid, `None` for revisions 1 and 2.
    ///
    /// Reported rather than acted on. A revision-3 capability is honoured only
    /// inside a user namespace whose root maps to this uid, so a report that printed
    /// it identically to a revision-2 blessing would describe a grant that does not
    /// exist on this box — and the authoritative answer to "am I blessed" is not
    /// this file reading at all but [`capability_state`], which asks the kernel
    /// about the running process.
    pub rootid: Option<u32>,
}

impl FileCaps {
    /// Is `bit` in the permitted mask — i.e. would an `execve` of this file hand the
    /// capability to the new process at all?
    pub fn permits(&self, bit: u32) -> bool {
        1u64.checked_shl(bit)
            .is_some_and(|mask| self.permitted & mask != 0)
    }

    /// Is `bit` anywhere on this file — permitted **or** inheritable?
    ///
    /// The looser question, and the one an orphan sweep asks: a capability sitting
    /// only in the inheritable mask is still a capability on a file, and a sweep
    /// that asked the strict question would walk past it.
    pub fn carries(&self, bit: u32) -> bool {
        1u64.checked_shl(bit)
            .is_some_and(|mask| (self.permitted | self.inheritable) & mask != 0)
    }

    /// Does this blob grant nothing at all? A file can carry the attribute with both
    /// masks empty; that is still an attribute, and still worth reporting.
    pub fn is_empty(&self) -> bool {
        self.permitted == 0 && self.inheritable == 0
    }
}

/// `VFS_CAP_REVISION_MASK` from `linux/capability.h` — the top byte of `magic_etc`.
const VFS_CAP_REVISION_MASK: u32 = 0xFF00_0000;
/// `VFS_CAP_FLAGS_EFFECTIVE` — the only flag defined in the low three bytes.
const VFS_CAP_FLAGS_EFFECTIVE: u32 = 0x0000_0001;

/// Decode a `security.capability` blob, the pure half of [`file_capabilities`].
///
/// Split out for the reason [`parse_capability_state`] is split out and
/// `unhardened_disposition` is split out in the helper: **no unprivileged test can
/// create a capability-carrying file** (that needs `CAP_SETFCAP`), so a decoder only
/// ever exercised through a real `getxattr` would be asserted by nothing on CI, on a
/// developer box, and on the rig alike. Here it is fed bytes directly, including the
/// exact 20 bytes this repository's own blessed copy carries.
///
/// # The format, from `linux/capability.h`
///
/// Little-endian throughout: a `__le32 magic_etc`, then `VFS_CAP_U32` pairs of
/// `{ __le32 permitted, __le32 inheritable }`, then — revision 3 only — a
/// `__le32 rootid`. `magic_etc`'s top byte is the revision and its low bits are
/// flags, of which exactly one is defined. Revision 1 carries **one** pair (32
/// capabilities), revisions 2 and 3 carry two (64), so the sizes are 12, 20 and 24
/// bytes and nothing else.
///
/// **The length is required to be exact, not merely sufficient.** A blob that is not
/// its revision's size is not a blob this decoder understands, and the direction to
/// be wrong in is loud: guessing at a truncated capability record and reporting the
/// bits that happened to parse is how a half-read grant gets described as a whole
/// one. A future revision 4 lands here as a named error rather than as a silent
/// misread, which is why the caller's buffer is larger than any defined revision —
/// an oversized answer must reach this function to be rejected, not be turned into
/// `ERANGE` by a buffer cut to today's maximum.
pub fn parse_file_capability_xattr(blob: &[u8]) -> Result<FileCaps, String> {
    fn le32(blob: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([blob[at], blob[at + 1], blob[at + 2], blob[at + 3]])
    }
    if blob.len() < 4 {
        return Err(format!(
            "security.capability is {} bytes, too short to hold even the magic word",
            blob.len()
        ));
    }
    let magic = le32(blob, 0);
    let revision = magic & VFS_CAP_REVISION_MASK;
    let effective = magic & VFS_CAP_FLAGS_EFFECTIVE != 0;
    // (revision, expected byte length, number of {permitted, inheritable} pairs)
    let (want_len, pairs) = match revision {
        0x0100_0000 => (12usize, 1usize),
        0x0200_0000 => (20, 2),
        0x0300_0000 => (24, 2),
        other => {
            return Err(format!(
                "security.capability carries revision {:#010x}, which this decoder does not \
                 know (it knows 1, 2 and 3). Refusing to guess at the layout of a capability \
                 record is deliberate: a misread here describes a grant that is not there",
                other >> 24
            ));
        }
    };
    if blob.len() != want_len {
        return Err(format!(
            "security.capability is {} bytes but revision {} is exactly {want_len}",
            blob.len(),
            revision >> 24
        ));
    }
    let mut permitted = 0u64;
    let mut inheritable = 0u64;
    for pair in 0..pairs {
        // data[pair] = { permitted, inheritable }, low 32 bits first.
        permitted |= u64::from(le32(blob, 4 + 8 * pair)) << (32 * pair);
        inheritable |= u64::from(le32(blob, 8 + 8 * pair)) << (32 * pair);
    }
    Ok(FileCaps {
        permitted,
        inheritable,
        effective,
        rootid: (revision == 0x0300_0000).then(|| le32(blob, 20)),
    })
}

/// What capabilities the file at `path` carries, read from the kernel rather than
/// from a subprocess. `Ok(None)` means the file carries none.
///
/// # Why this exists here rather than as a `getcap` invocation
///
/// The privileged replug helper's whole safety argument is its narrowness, and
/// §15.45 states five bounds — argv only, no environment read, **no `exec` while
/// blessed**, no path parameter, the kernel confirms the filesystem. The helper's
/// `install` module answered "what does this file carry" by spawning `getcap`, and
/// `preflight` reaches that code with **no** blessed-copy refusal in front of it, so
/// a copy holding `cap_dac_override,cap_fowner` execed a `PATH`-selected binary and
/// handed it the whole environment. Measured on the rig box: the spawned process's
/// parent reads `CapPrm`/`CapEff` `000000000000000a`, and a shim earlier on `PATH`
/// flips the helper's verdict between READY and BLOCKED-ON-BLESS — the environment
/// deciding the answer of a binary whose first stated bound is that it reads none.
///
/// The capability set of a file *is* the `security.capability` extended attribute,
/// so the honest answer is one `getxattr(2)`: no `PATH` lookup, no child, no
/// inherited environment, and no dependency on libcap being installed — which the
/// `docs/vmcell-requirements.md` survey had already noted breaks the sweep's own
/// test on a userland without `getcap`. `unsafe` lives only in this crate (§16.3),
/// which is why the syscall is here and the vocabulary — capability *names*, the
/// required set — stays in `devprep`.
///
/// # The two errno answers that are not errors
///
/// `ENODATA` is "no such attribute": the file carries nothing, which is
/// `Ok(None)`. `ENOTSUP` is "this filesystem does not do extended attributes", and a
/// filesystem that cannot store the attribute cannot be holding one, so it is
/// `Ok(None)` too — the same answer `getcap` gives, which matters because an
/// operator cross-checks this tool against the stock one. Every other errno is
/// raised with the path attached, because "cannot tell" must never render as
/// "carries nothing" on a question about privilege.
///
/// Follows a final symlink, exactly as `getcap` does. The one caller that walks a
/// directory filters on `file_type()` first, so a symlink never reaches here.
#[cfg(target_os = "linux")]
pub fn file_capabilities(path: &std::path::Path) -> std::io::Result<Option<FileCaps>> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("path contains a NUL byte"))?;
    let name = c"security.capability";
    // Larger than revision 3's 24 bytes on purpose: an unknown, longer blob must
    // reach the decoder to be *named*, rather than becoming an `ERANGE` that says
    // only that the buffer was small (see `parse_file_capability_xattr`).
    let mut buf = [0u8; 64];
    // Safety: `c_path` and `name` are valid NUL-terminated strings for the duration
    // of the call, and `buf` is valid for writes of `buf.len()` bytes, which is what
    // is passed as the size.
    let n = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n < 0 {
        let err = std::io::Error::last_os_error();
        return match err.raw_os_error() {
            Some(libc::ENODATA) | Some(libc::ENOTSUP) => Ok(None),
            _ => Err(std::io::Error::new(
                err.kind(),
                format!("reading security.capability of {}: {err}", path.display()),
            )),
        };
    }
    parse_file_capability_xattr(&buf[..n as usize])
        .map(Some)
        .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))
}

/// Set `PR_SET_NO_NEW_PRIVS` **and confirm with the kernel that it took**, so no
/// `execve` from this thread — or from anything it forks — can ever gain privilege,
/// including through a setuid binary or another file capability.
///
/// The helper never `exec`s anything while blessed, so this is belt-and-braces; it
/// is set anyway because it is one syscall and it converts "the helper does not
/// exec" from a property of the code into a property the kernel enforces. §15.45
/// lists that conversion among the five bounds that hold *by construction*, and a
/// bound holds by construction only if the construction is checked: `prctl(2)`
/// answers `EINVAL` for `PR_SET_NO_NEW_PRIVS` on kernels before 3.5, and a seccomp
/// policy can filter either call, so a caller that dropped the return value would
/// be asserting an intention. This function therefore checks the setter *and* reads
/// the bit back with `PR_GET_NO_NEW_PRIVS`, which is the kernel's own answer and
/// needs no `/proc` mount (unlike [`capability_state`], where the alternative to
/// `/proc` was `capget(2)` and more `unsafe`).
///
/// The bit is per-thread, inherited across `fork` and `execve`, and cannot be
/// unset; setting it on the helper's only thread — before it creates any — covers
/// every process that thread could become.
///
/// `Err` carries an operator-readable diagnosis rather than a bare `Errno`, and the
/// caller decides how loudly to fail: a copy that holds no capability has nothing
/// left to harden, while a blessed one must not proceed (see the refusal in
/// `serial-nexus-devprep`'s `main`).
#[cfg(target_os = "linux")]
pub fn establish_no_new_privs() -> Result<(), String> {
    nix::sys::prctl::set_no_new_privs()
        .map_err(|e| format!("prctl(PR_SET_NO_NEW_PRIVS) failed: {e}"))?;
    match nix::sys::prctl::get_no_new_privs() {
        Ok(true) => Ok(()),
        Ok(false) => Err(
            "prctl(PR_SET_NO_NEW_PRIVS) reported success but PR_GET_NO_NEW_PRIVS reads 0"
                .to_owned(),
        ),
        Err(e) => Err(format!(
            "prctl(PR_GET_NO_NEW_PRIVS) failed: {e} — the bit cannot be confirmed set"
        )),
    }
}

/// Ask the kernel to send `SIGTERM` to this process when its parent dies.
///
/// Crash safety (§15.45): the helper may be holding a USB device deauthorized when
/// the test that spawned it is killed. `PDEATHSIG` is what turns a SIGKILLed parent
/// into a signal the hold loop can act on, re-authorizing before it exits.
///
/// **Must be called by the helper itself, not by whoever spawns it**: `execve` of a
/// file carrying file capabilities clears the pending pdeathsig, so a parent that
/// set it before `exec` would have set nothing. The caller must also re-check
/// `getppid()` afterwards — see [`parent_is`] — because the parent can die in the
/// window between `fork` and this call.
#[cfg(target_os = "linux")]
pub fn set_pdeathsig_term() -> nix::Result<()> {
    nix::sys::prctl::set_pdeathsig(Some(nix::sys::signal::Signal::SIGTERM))
}

/// Whether this process's parent is still the pid it expects.
///
/// Closes the `PDEATHSIG` race: if the parent died before [`set_pdeathsig_term`]
/// ran, the signal will never arrive and the caller must exit on its own.
#[cfg(target_os = "linux")]
pub fn parent_is(expected: u32) -> bool {
    nix::unistd::getppid().as_raw() as u32 == expected
}

/// Set by the signal handler; polled by the replug helper's hold loop.
#[cfg(target_os = "linux")]
static TERMINATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The handler. Storing to an `AtomicBool` is async-signal-safe; nothing else
/// happens here, which is why this is correct rather than merely conventional.
#[cfg(target_os = "linux")]
extern "C" fn note_terminate(_signal: libc::c_int) {
    TERMINATE.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Catch `SIGINT`/`SIGTERM`/`SIGHUP` so a terminating helper can put the hardware
/// back before it exits (§15.45 crash safety).
///
/// Without this, a `SIGTERM` during the hold window leaves a USB device
/// deauthorized — invisible to the operator except as hardware that stopped
/// existing. With it, the hold loop notices within its poll interval and
/// re-authorizes on the way out.
#[cfg(target_os = "linux")]
pub fn catch_terminate() -> nix::Result<()> {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    let action = SigAction::new(
        SigHandler::Handler(note_terminate),
        SaFlags::empty(),
        SigSet::empty(),
    );
    for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: `note_terminate` only stores to an `AtomicBool`, which is
        // async-signal-safe; it allocates nothing and takes no lock.
        unsafe { sigaction(signal, &action)? };
    }
    Ok(())
}

/// Whether a termination signal has arrived since [`catch_terminate`].
#[cfg(target_os = "linux")]
pub fn terminate_requested() -> bool {
    TERMINATE.load(std::sync::atomic::Ordering::SeqCst)
}

/// Whether `dir` really is on `sysfs`, asked of the kernel rather than of the path
/// string.
///
/// This is the replug helper's confinement, and it is deliberately stronger than a
/// `starts_with("/sys/bus/usb/devices")` check on a canonicalized path. A string
/// test proves only that a path *spells* something; `fstatfs` proves the bytes the
/// helper is about to write land in a kernel-synthesised filesystem, which closes
/// the one remaining way to aim a `CAP_DAC_OVERRIDE` write at attacker-controlled
/// content — a tmpfs bind-mounted over `/sys/bus/usb/devices`.
///
/// Fails closed: an unreadable path or any other filesystem answers `false`.
#[cfg(target_os = "linux")]
pub fn is_sysfs(dir: &std::path::Path) -> bool {
    nix::sys::statfs::statfs(dir)
        .map(|s| s.filesystem_type() == nix::sys::statfs::SYSFS_MAGIC)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parser reads the real format, taken verbatim from this box.
    #[test]
    fn cap_fields_parse_from_real_proc_text() {
        let status = "Name:\tbash\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\n";
        assert_eq!(
            parse_capability_state(status, CAP_DAC_OVERRIDE),
            Some(CapState::NONE)
        );
        // CAP_DAC_OVERRIDE is bit 1, so the mask is 0x2.
        let blessed = "CapPrm:\t0000000000000002\nCapEff:\t0000000000000002\n";
        assert_eq!(
            parse_capability_state(blessed, CAP_DAC_OVERRIDE),
            Some(CapState {
                permitted: true,
                effective: true
            })
        );
        // `+p` without the effective bit: permitted but not honoured. The helper
        // must refuse here, which is why the two bits are reported separately.
        let permitted_only = "CapPrm:\t0000000000000002\nCapEff:\t0000000000000000\n";
        assert_eq!(
            parse_capability_state(permitted_only, CAP_DAC_OVERRIDE),
            Some(CapState {
                permitted: true,
                effective: false
            })
        );
    }

    /// A missing or malformed field is `None`, never a quiet `false` — a `false`
    /// would be indistinguishable from an honest "not held" and would hide a
    /// `/proc` format change behind a permanently-skipping test.
    #[test]
    fn absent_or_malformed_fields_are_none_rather_than_not_held() {
        assert_eq!(
            parse_capability_state("Name:\tbash\n", CAP_DAC_OVERRIDE),
            None
        );
        assert_eq!(
            parse_capability_state("CapPrm:\tnothex\nCapEff:\t0\n", CAP_DAC_OVERRIDE),
            None
        );
        // Present but only one of the pair.
        assert_eq!(
            parse_capability_state("CapPrm:\t0000000000000002\n", CAP_DAC_OVERRIDE),
            None
        );
    }

    /// A bit past the width of the mask cannot panic — `checked_shl` is the reason.
    #[test]
    fn an_out_of_range_capability_bit_is_none_rather_than_a_panic() {
        assert_eq!(parse_capability_state("CapPrm:\t0\nCapEff:\t0\n", 64), None);
    }

    /// The **exact twenty bytes this repository's own blessed copy carries**, read
    /// off the rig box with `os.getxattr` and pasted here verbatim (2026-08-21,
    /// `.snx-bin/debug/serial-nexus-devprep`, which `getcap` prints as
    /// `cap_dac_override,cap_fowner=ep`).
    ///
    /// A decoder tested only against blobs it invented would agree with itself. This
    /// is the one artifact that pins it to the kernel: revision 2, effective flag
    /// set, permitted `0x0a` — bits 1 and 3, which are exactly
    /// [`CAP_DAC_OVERRIDE`] and [`CAP_FOWNER`] — and nothing inheritable.
    #[test]
    fn the_blessing_this_repository_actually_carries_decodes_to_its_two_capabilities() {
        let blob: [u8; 20] = [
            0x01, 0x00, 0x00, 0x02, // magic_etc: revision 2 | VFS_CAP_FLAGS_EFFECTIVE
            0x0a, 0x00, 0x00, 0x00, // data[0].permitted   = CAP_DAC_OVERRIDE|CAP_FOWNER
            0x00, 0x00, 0x00, 0x00, // data[0].inheritable
            0x00, 0x00, 0x00, 0x00, // data[1].permitted   (capabilities 32..63)
            0x00, 0x00, 0x00, 0x00, // data[1].inheritable
        ];
        let caps = parse_file_capability_xattr(&blob).expect("the real blessing decodes");
        assert_eq!(
            caps,
            FileCaps {
                permitted: 0x0a,
                inheritable: 0,
                effective: true,
                rootid: None,
            }
        );
        assert!(caps.permits(CAP_DAC_OVERRIDE) && caps.permits(CAP_FOWNER));
        assert!(
            !caps.permits(0),
            "bit 0 is CAP_CHOWN and this file does not carry it"
        );
        assert!(!caps.is_empty());
    }

    /// A blessing without the effective flag is the `+p`-only state, and the decoder
    /// must separate it — that separation is the whole of the `/deps/`-substring
    /// lesson `devprep` recorded, moved to a place where it cannot be spelled wrong:
    /// there is no text to mis-match, only a bit.
    #[test]
    fn the_effective_flag_is_one_bit_of_the_magic_word_and_permitted_alone_does_not_set_it() {
        let mut blob: [u8; 20] = [0; 20];
        blob[..4].copy_from_slice(&0x0200_0000u32.to_le_bytes()); // revision 2, no flags
        blob[4..8].copy_from_slice(&0x0au32.to_le_bytes());
        let caps = parse_file_capability_xattr(&blob).expect("decodes");
        assert!(
            caps.permits(CAP_DAC_OVERRIDE),
            "permitted still carries the bit"
        );
        assert!(
            !caps.effective,
            "`+p` without `+e` must not read as effective — the process would have to \
             capset(2) for itself, and a report that called this blessed would be \
             describing a helper that then fails the sysfs write"
        );
    }

    /// Capabilities above 31 live in the **second** `{permitted, inheritable}` pair,
    /// and the shift that puts them there is the one arithmetic step in this decoder.
    ///
    /// Fail-first: dropping the `<< (32 * pair)` (or writing `data[1]` into the low
    /// word) turns this red and leaves every other test in this file green, because
    /// the two capabilities this tree grants are bits 1 and 3.
    #[test]
    fn a_capability_above_thirty_one_decodes_out_of_the_high_word() {
        let mut blob: [u8; 20] = [0; 20];
        blob[..4].copy_from_slice(&0x0200_0001u32.to_le_bytes());
        // CAP_CHECKPOINT_RESTORE is 40, i.e. bit 8 of the high word.
        blob[12..16].copy_from_slice(&(1u32 << 8).to_le_bytes());
        // ...and something inheritable-only in the low word, so the two masks are
        // proven not to be reading each other's bytes.
        blob[8..12].copy_from_slice(&(1u32 << 5).to_le_bytes());
        let caps = parse_file_capability_xattr(&blob).expect("decodes");
        assert_eq!(caps.permitted, 1u64 << 40);
        assert_eq!(caps.inheritable, 1u64 << 5);
        assert!(caps.permits(40) && !caps.permits(5));
        assert!(
            caps.carries(5) && caps.carries(40),
            "`carries` is the union: a capability sitting only in the inheritable mask \
             is still a capability on the file, which is what an orphan sweep asks"
        );
    }

    /// Revision 1 is one pair and twelve bytes; revision 3 adds the namespaced
    /// blob's owning uid, which is reported rather than swallowed.
    #[test]
    fn revisions_one_and_three_decode_at_their_own_lengths() {
        let mut v1: [u8; 12] = [0; 12];
        v1[..4].copy_from_slice(&0x0100_0001u32.to_le_bytes());
        v1[4..8].copy_from_slice(&0x0au32.to_le_bytes());
        assert_eq!(
            parse_file_capability_xattr(&v1).expect("revision 1 decodes"),
            FileCaps {
                permitted: 0x0a,
                inheritable: 0,
                effective: true,
                rootid: None,
            }
        );

        let mut v3: [u8; 24] = [0; 24];
        v3[..4].copy_from_slice(&0x0300_0001u32.to_le_bytes());
        v3[4..8].copy_from_slice(&0x0au32.to_le_bytes());
        v3[20..24].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(
            parse_file_capability_xattr(&v3)
                .expect("revision 3 decodes")
                .rootid,
            Some(1000),
            "a revision-3 capability is honoured only inside a user namespace whose \
             root maps to this uid, so the reader must surface it rather than print it \
             as if it were an ordinary blessing"
        );
    }

    /// Every shape the decoder must refuse rather than guess at, and each refusal
    /// names what it saw. A capability record that half-parses is worse than one that
    /// does not parse at all: the caller would report a grant.
    #[test]
    fn a_blob_that_is_not_exactly_its_revisions_size_is_refused_rather_than_guessed_at() {
        // Too short for the magic word at all.
        assert!(parse_file_capability_xattr(&[0x01, 0x00, 0x00]).is_err());
        // Revision 2 truncated to revision 1's length — the shape a partial read or a
        // hand-written attribute takes, and the one a `len() >= want` check would wave
        // through while silently reading zeros for the high word.
        let mut short: [u8; 12] = [0; 12];
        short[..4].copy_from_slice(&0x0200_0001u32.to_le_bytes());
        short[4..8].copy_from_slice(&0x0au32.to_le_bytes());
        let e = parse_file_capability_xattr(&short).expect_err("a 12-byte revision 2 is refused");
        assert!(
            e.contains("12") && e.contains("20"),
            "the refusal names both lengths: {e}"
        );
        // Revision 2 with a trailing byte.
        assert!(parse_file_capability_xattr(&[0u8; 21]).is_err());
        // An unknown revision, which is how a future format arrives.
        let mut v4: [u8; 20] = [0; 20];
        v4[..4].copy_from_slice(&0x0400_0001u32.to_le_bytes());
        let e = parse_file_capability_xattr(&v4).expect_err("revision 4 is refused");
        assert!(e.contains("revision"), "{e}");
    }

    /// A capability bit past the width of the mask answers `false` instead of
    /// panicking — the same `checked_shl` reasoning
    /// [`an_out_of_range_capability_bit_is_none_rather_than_a_panic`] carries one
    /// function over, and it is here because both accessors take a caller's `u32`.
    #[test]
    fn an_out_of_range_bit_queried_against_a_file_answers_false_rather_than_panicking() {
        let caps = FileCaps {
            permitted: u64::MAX,
            inheritable: u64::MAX,
            effective: true,
            rootid: None,
        };
        assert!(!caps.permits(64));
        assert!(!caps.carries(64));
        assert!(!caps.permits(u32::MAX));
    }

    /// The reader agrees with the decoder on a file that carries nothing, and says
    /// so as `Ok(None)` rather than as an error — this source file itself.
    ///
    /// The other half — a file that *does* carry a capability — cannot be produced by
    /// an unprivileged test (`setcap` needs `CAP_SETFCAP`, and this box's
    /// `unprivileged_userns_clone` route is closed: `unshare -Ur` answers
    /// `Operation not permitted` writing `uid_map`), which is precisely why
    /// [`parse_file_capability_xattr`] takes bytes and carries the rig's real blob as
    /// a fixture. What this test covers is the part the decoder cannot: that the
    /// syscall is reached, that `ENODATA` is not an error, and that the path makes it
    /// into the answer.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_with_no_capability_reads_as_none_and_a_missing_file_reads_as_an_error() {
        // This test binary: an *executable*, so it is the exact class of file that
        // could carry a capability — and it does not, because nothing in this
        // workspace's build blesses anything. A source file would prove less.
        let me = std::env::current_exe().expect("a test binary knows its own path");
        assert!(
            me.is_file(),
            "{} must exist for this test to mean anything",
            me.display()
        );
        assert_eq!(
            file_capabilities(&me).expect("reading an ordinary executable must not fail"),
            None,
            "a file carrying no capability is `Ok(None)`, never an error — the orphan \
             sweep walks a whole directory of them"
        );
        let missing = std::path::Path::new("/nonexistent-by-construction-snx");
        let err = file_capabilities(missing).expect_err("a missing file is an error, not None");
        assert!(
            err.to_string().contains("nonexistent-by-construction-snx"),
            "the error must name the path it was asked about: {err}"
        );
    }

    /// The reader agrees with the parser on this process, which holds nothing.
    #[test]
    fn this_unprivileged_test_process_holds_no_dac_override() {
        assert_eq!(capability_state(CAP_DAC_OVERRIDE), CapState::NONE);
    }

    /// The no-new-privs bound is *established*, not attempted: the same read that
    /// answers "set" afterwards answers "unset" before, so the instrument is shown
    /// able to fire in both directions (§15.46's discriminator-can-fire rule applied
    /// to a one-bit measurement). Without the "before" half this test would pass
    /// against a kernel that reported the bit set unconditionally.
    ///
    /// Safe to run inside the suite because `PR_SET_NO_NEW_PRIVS` is a *per-thread*
    /// attribute: it lands on the libtest thread executing this test and on nothing
    /// else, so no other test's `execve` is affected. It is irreversible on that
    /// thread, which is why the assertion is here and not in a shared helper.
    #[cfg(target_os = "linux")]
    #[test]
    fn no_new_privs_reads_unset_before_and_set_after_establishing_it() {
        assert_eq!(
            nix::sys::prctl::get_no_new_privs(),
            Ok(false),
            "this test thread must start unhardened, or the transition below measures nothing"
        );
        establish_no_new_privs().expect("PR_SET_NO_NEW_PRIVS on Linux 3.5 or later");
        assert_eq!(
            nix::sys::prctl::get_no_new_privs(),
            Ok(true),
            "the kernel must report the bit this process just set — the read-back is the \
             whole difference between establishing the bound and attempting it"
        );
    }

    /// The confinement check answers the kernel's question, not the path's: a real
    /// sysfs directory is accepted and an ordinary directory that merely *looks*
    /// like one is refused. The second half is the assertion that matters — it is
    /// what a `starts_with` on a path string cannot make.
    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_is_recognised_by_the_kernel_and_a_lookalike_path_is_not() {
        assert!(is_sysfs(std::path::Path::new("/sys/bus/usb/devices")));
        let tmp = std::env::temp_dir().join(format!("snx-sysfs-lookalike-{}", std::process::id()));
        let lookalike = tmp.join("sys/bus/usb/devices/3-1");
        std::fs::create_dir_all(&lookalike).expect("create lookalike tree");
        assert!(
            !is_sysfs(&lookalike),
            "a tmpfs/ext4 path spelled like a sysfs one must not pass the check"
        );
        assert!(!is_sysfs(std::path::Path::new(
            "/nonexistent-by-construction"
        )));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
