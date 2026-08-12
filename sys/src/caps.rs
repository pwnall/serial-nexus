//! Capability inspection and process hardening for the privileged replug helper
//! (design §15.45).
//!
//! This module exists here rather than in `devprep/` for the reason the whole crate
//! exists (§16.3): `prctl(2)` needs `unsafe` somewhere, and there is exactly one
//! crate allowed to hold it. The capability *reader* needs no `unsafe` at all — it
//! parses `/proc/self/status` — but it lives beside the hardening calls so that
//! everything this workspace knows about Linux capabilities is one file.
//!
//! Nothing here grants privilege. `CAP_DAC_OVERRIDE` reaches the helper from the
//! *file* capability `setcap` applied to the installed copy; these functions only
//! observe that state and shrink what the process can do afterwards.

/// Bit number of `CAP_DAC_OVERRIDE` in the capability bitmasks (`linux/capability.h`).
///
/// This is the one capability the replug helper is blessed with, and it is a large
/// grant: it bypasses every DAC permission check on read and write, so a process
/// holding it can write `/etc/shadow`. The helper's narrowness — argv-only, no
/// environment, no `exec`, one kernel-verified sysfs write — is what bounds it, not
/// the capability itself (§15.45).
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
