//! `serial-nexus-devprep` — deauthorize and reauthorize one named USB serial
//! adapter, so §12's identity promise can be tested against a real kernel
//! re-enumeration (design §15.45).
//!
//! # Why this binary exists
//!
//! P4's `supported` arm asserts *"Resolver produces canonical identities; configs
//! survive replug and cold start"*, and §12 promises identity-to-path resolution
//! *"at every open and every faulted-and-wait recheck"*. Nothing in the tree
//! measured that against a real hotplug: `itest/tests/p7_replug.rs` re-links a
//! fixture sysfs tree under `--dev-root` and respawns a sim pty, which exercises
//! the daemon's healing but never the kernel's enumeration. Writing `0` then `1` to
//! a USB device's `authorized` attribute makes the kernel disconnect and
//! re-enumerate it for real — and that file is `root:root 0644`, which is why
//! privilege is involved at all.
//!
//! # What holds the privilege, and what bounds it
//!
//! This binary is blessed with the **two** file capabilities named once in
//! [`install::REQUIRED_CAPS`] — `cap_dac_override` for the `root:root 0644` sysfs
//! write, and `cap_fowner` for the POSIX ACL that hands the invoking user back the
//! tty the reauthorization recreated (§15.55) — on a copy installed at
//! `.snx-bin/<profile>/` (see [`install`]). The first is a large grant — it bypasses
//! every DAC check on the system — so what bounds them is this binary's narrowness,
//! asserted by construction:
//!
//! * **argv only.** It reads no environment variable, so there is no `LD_PRELOAD`
//!   or config-file path into a capability-holding process.
//! * **no `exec`.** It never spawns anything while privileged; `PR_SET_NO_NEW_PRIVS`
//!   makes that a kernel guarantee rather than a property of the code, and the one
//!   module that does spawn (`install`) is refused when the capability is held. The
//!   prctl is *established*, not attempted: [`main`] checks it, reads the bit back
//!   from the kernel, and a copy holding the capability that cannot get both answers
//!   refuses to run at all rather than continuing unhardened.
//! * **no path parameter.** The only device argument is a USB port *name* like
//!   `3-1`, validated against a digit/`-`/`.` alphabet and joined to a compile-time
//!   root, so no caller-supplied byte ever reaches `open(2)` as a path.
//! * **the kernel confirms the target filesystem.** `fstatfs` must report
//!   `SYSFS_MAGIC` before any write, which a `starts_with` on a canonicalized path
//!   cannot establish.
//! * **the device must be a non-hub serial adapter**, or the write is refused.
//!
//! Adding a verb that accepts a filesystem path would dissolve every one of those
//! bounds at once. That is a design amendment (§15.45), never a patch. Adding a
//! **capability** widens the blast radius the same way and is likewise an amendment:
//! [`install::REQUIRED_CAPS`] is the one place the set is written, so a third entry
//! is a visible, reviewable edit rather than a `setcap` line someone typed.
//!
//! §15.55 is that amendment executed the *other* way, and is worth reading as the
//! worked example: the `grant` verb was added without adding a path. It takes the
//! same kernel-verified port *name*, derives the device nodes itself, grants to
//! `getuid()` and never to an argv uid, and never opens the node — so each of the
//! five bounds above survives it intact.

mod install;
mod sysfs;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use serial_nexus_sys::caps::{self, CAP_DAC_OVERRIDE, CapState};
use sysfs::Port;

/// How often the hold loop checks for a termination signal. Short enough that a
/// `SIGTERM` does not visibly extend an outage, long enough not to spin.
const TERMINATE_POLL: Duration = Duration::from_millis(10);

/// Ceiling on how long a device may be held deauthorized, whatever `--hold-ms`
/// says. A helper that can be told to hold hardware down for an hour is a footgun
/// aimed at whoever runs the suite next.
const MAX_HOLD: Duration = Duration::from_secs(30);

/// How long re-enumeration is given to produce a tty again before the helper calls
/// it a failure. Measured budget on the dev box is well under a second; ten seconds
/// is a deadline, not an expectation.
const REENUMERATION_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(
    name = "serial-nexus-devprep",
    version,
    about = "USB re-enumeration helper for the serial_nexus rig (design §15.45)"
)]
struct Cli {
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Subcommand)]
enum Verb {
    /// Report which capabilities this process holds. Never privileged.
    Capabilities {
        /// Emit JSON instead of prose.
        #[arg(long)]
        json: bool,
    },
    /// Grant the **invoking user** read/write on every tty this port owns.
    ///
    /// The equivalent of `setfacl -m u:$USER:rw /dev/ttyUSB*` for one USB port, and
    /// the reason this binary carries `CAP_FOWNER` beside `CAP_DAC_OVERRIDE`
    /// (§15.55). It exists because a re-enumeration destroys the device node and
    /// udev builds a fresh one `root:dialout 0660`: any grant made before a cycle is
    /// gone with the inode that carried it, which is exactly how a rig lane that had
    /// device access at the start loses it half way through.
    ///
    /// **The bound §15.45 states is kept, not spent.** argv carries a USB *port
    /// name*, validated and checked against sysfs exactly as every other verb here
    /// does; the device nodes are then derived by this binary from the kernel's own
    /// view of that port. No caller-supplied path reaches `open`, and no
    /// caller-supplied uid reaches the ACL — the beneficiary is `getuid()`, so this
    /// can only ever hand access to whoever ran it.
    Grant {
        /// USB port name, repeatable. Not a path.
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Report a port's identity and authorization state. Reads only.
    Status {
        /// USB port name, e.g. `3-1`. Not a path.
        #[arg(long)]
        port: String,
        #[arg(long)]
        json: bool,
    },
    /// Deauthorize, hold, then reauthorize one or more ports — the replug.
    Cycle {
        /// USB port name, repeatable. Ports are deauthorized in the order given
        /// and reauthorized in reverse, which is what forces a `ttyUSBn`
        /// renumbering when two adapters are cycled together.
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        /// How long to hold the devices deauthorized, in milliseconds.
        #[arg(long, default_value_t = 1500)]
        hold_ms: u64,
        /// Run every check and every wait, but perform neither write. The positive
        /// control for the guard: a test body that passes under `--dry-run` is not
        /// measuring the replug.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Deauthorize, then hold the device down until **stdin reaches EOF**, then
    /// reauthorize. The verb tests use.
    ///
    /// `cycle` bakes the hold duration into this binary, which means every change
    /// to how an experiment is timed or sampled is a change to a binary that must
    /// then be re-blessed with a `sudo`. `hold` moves that decision out: the caller
    /// keeps the device down for exactly as long as it wants by keeping the pipe
    /// open, and does all of its own sampling — unprivileged, since reading sysfs
    /// needs nothing. What stays in here is only the pair of writes.
    ///
    /// The leash is the same mechanism §15.43 chose for the daemon and for the same
    /// reason: pipe EOF is POSIX, it fires on every way a caller can die including
    /// `SIGKILL`, and it needs no cooperation from the thing being watched.
    Hold {
        /// USB port name, repeatable. Deauthorized in the order given and
        /// reauthorized in reverse.
        #[arg(long = "port", required = true)]
        ports: Vec<String>,
        /// Ceiling on the hold, in milliseconds, in case the caller never closes
        /// stdin. Clamped to the same maximum `cycle` uses.
        #[arg(long, default_value_t = 30_000)]
        max_ms: u64,
        /// Do everything except the two writes. The caller's own discriminator
        /// must go quiet here, which is what makes it a discriminator.
        #[arg(long)]
        dry_run: bool,
    },
    /// Reauthorize a port. The repair verb for a run that died mid-cycle.
    Authorize {
        #[arg(long)]
        port: String,
        #[arg(long)]
        json: bool,
    },
    /// Install a copy at `.snx-bin/<profile>/` and print the one `sudo` command
    /// that blesses it. Refused when this process already holds the capability.
    Install {
        /// Cargo profile directory to install from and to.
        #[arg(long, default_value = "debug")]
        profile: String,
        /// Only report the state; change nothing.
        #[arg(long)]
        verify: bool,
        /// Print the exact `sudo setcap …` line for this profile and exit, changing
        /// nothing. `scripts/bless` reads it rather than spelling the capability
        /// list a second time, so the command an operator is shown, the command that
        /// runs, and the set `--verify` checks against cannot drift apart.
        #[arg(long = "print-setcap")]
        print_setcap: bool,
    },
    /// Is the replug capability usable here? Exits 0 ready, 2 blocked-on-bless,
    /// 1 a genuinely absent facility.
    Preflight {
        #[arg(long, default_value = "debug")]
        profile: String,
        #[arg(long)]
        json: bool,
    },
}

/// Exit codes. The three-way split is load-bearing: vmcell records that a two-way
/// answer made readers treat a fixable "not ready" as a missing facility and
/// downgrade themselves to a weaker check.
mod exit {
    /// Everything needed is present.
    pub const READY: i32 = 0;
    /// A genuinely absent facility — no adapter, not Linux. Skipping is honest.
    pub const NOT_READY: i32 = 1;
    /// Only the install/blessing is missing. **Not** a licence to skip.
    pub const BLOCKED_ON_BLESS: i32 = 2;
    /// The request was refused: bad port name, wrong device class, no capability.
    pub const REFUSED: i32 = 3;
    /// The operation started and failed — including a hold cut short by a signal.
    pub const FAILED: i32 = 70;
}

/// What it means that the no-new-privs bound could not be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unhardened {
    /// Say so and carry on. This copy holds no capability, so there is nothing left
    /// for the bound to protect — and `install`, `preflight` and the read-only verbs
    /// are exactly what an operator needs on a box whose kernel or sandbox answered
    /// this way. Reported on stderr, never silent.
    Report,
    /// Refuse to run at all. This copy is blessed and one of §15.45's five bounds is
    /// missing.
    Refuse,
}

/// The disposition, split out from [`main`] so the branch that can only occur on a
/// blessed copy — which no unprivileged suite can produce — is still exercised by a
/// test rather than reasoned about.
///
/// `permitted` counts and not only `effective`: a copy blessed `+p` can raise the
/// capability into its effective set itself at any moment, no further privilege
/// required, so it is a blessed process for this purpose. `install_verb` refuses on
/// exactly the same pair for exactly the same reason.
///
/// **`held` is the state of *any* required capability, not of one named bit**
/// (2026-08-12). This branch keyed on `CAP_DAC_OVERRIDE` alone, from before
/// §15.55 added `cap_fowner`, so a copy carrying only the second capability held
/// privilege and was *not* refused when the no-`exec` bound could not be
/// established. Unreachable through the sanctioned path — `field_grants_required_caps`
/// demands every name, so `install --verify` calls such a copy `Unblessed` and
/// `scripts/bless` always applies the derived full set — but §15.45's narrowness is
/// a standing tripwire and "unreachable today" is the argument that stops being true
/// when the set next grows. [`caps_held`] now folds the whole set.
fn unhardened_disposition(held: CapState) -> Unhardened {
    if held.permitted || held.effective {
        Unhardened::Refuse
    } else {
        Unhardened::Report
    }
}

/// The union of every capability in [`install::REQUIRED_CAPS`], as one [`CapState`]:
/// permitted if any is permitted, effective if any is effective.
///
/// A *union* rather than an intersection, deliberately. The question this answers is
/// "does this process hold privilege the bounds are there to contain", and one
/// capability is enough to make it yes — an intersection would let a half-blessed
/// copy through the very refusal that exists because a blessed copy must be bounded.
fn caps_held() -> CapState {
    fold_cap_states(
        install::REQUIRED_CAPS
            .iter()
            .map(|(_, bit)| caps::capability_state(*bit)),
    )
}

/// The pure half of [`caps_held`], split out for the same reason
/// [`unhardened_disposition`] is: the state it folds cannot be produced by an
/// unprivileged test, so the *fold* is asserted directly rather than through a proxy
/// that would pass for the wrong reason (AGENTS §9).
fn fold_cap_states(states: impl Iterator<Item = CapState>) -> CapState {
    states.fold(CapState::NONE, |acc, c| CapState {
        permitted: acc.permitted || c.permitted,
        effective: acc.effective || c.effective,
    })
}

pub fn main() {
    // Hardening first, before an argument is looked at and before anything but
    // `/proc/self/status` is read. Cheap, irreversible, and it means every path
    // below runs under the same restriction.
    //
    // **Checked, because §15.45 says this bound holds by construction.** The entry
    // lists "no `exec` while blessed — `PR_SET_NO_NEW_PRIVS`-guaranteed" among five
    // bounds on a process carrying `CAP_DAC_OVERRIDE`, and a guarantee whose return
    // value is discarded is an attempt: `prctl(2)` refuses `PR_SET_NO_NEW_PRIVS`
    // with `EINVAL` before Linux 3.5 and a seccomp policy can filter it, and until
    // 2026-08-12 this line was `let _ = …`, so a failure would have left a blessed
    // process running unhardened and said nothing. `establish_no_new_privs` checks
    // the setter and reads the bit back from the kernel; what is left here is the
    // decision about what an unestablished bound means.
    let hardening = caps::establish_no_new_privs();
    // Decided once, and true for this process's lifetime: file capabilities arrive
    // at `execve` and this binary never `exec`s, so nothing below can acquire the
    // capability after this branch.
    let held = caps::capability_state(CAP_DAC_OVERRIDE);
    // The *bounds* question is about privilege of any kind, so it folds the whole
    // required set; `held` above stays the state of `cap_dac_override` specifically,
    // because that is the capability the sysfs write needs and the one
    // `capabilities` reports by name. Two different questions, deliberately not one
    // variable (see [`caps_held`]).
    let any_held = caps_held();
    if let Err(reason) = &hardening {
        match unhardened_disposition(any_held) {
            Unhardened::Refuse => {
                // A blessed copy that cannot harden itself does not run. Not a
                // warning: the reason this binary may carry a root-equivalent
                // capability at all is that its bounds are established rather than
                // hoped for, and one of the five is missing. Stderr and an exit code
                // rather than the usual `--json` refusal, because argv has
                // deliberately not been parsed yet, and `itest`'s `replug()` helper
                // treats a non-zero exit as fatal and surfaces stderr with it.
                //
                // **The bound this branch keys on is capability, and capability is read
                // from `/proc`.** `capability_state` answers `NONE` when
                // `/proc/self/status` is unreadable, because every *other* caller wants
                // "cannot prove it is held" to behave like "not held" — the privileged
                // verbs refuse in both cases. Here that default runs the other way: a
                // genuinely blessed copy on a box with no readable `/proc` **and** a
                // filtered prctl takes the Report branch and keeps going unhardened. It
                // is stated rather than papered over, because the honest bound is "no
                // blessed copy this process can *see* runs unhardened", and no
                // unprivileged reading of a capability can promise more than that.
                // The names are *derived*, not typed: this branch now fires on any
                // required capability, so a hardcoded `cap_dac_override` would be
                // false on precisely the copy the union check was added to catch —
                // one blessed `cap_fowner` alone. Plan §3 rule 11's bar for a skip
                // message ("never false on the box printing it") applies at least as
                // strongly to a refusal.
                let held_names: Vec<&str> = install::REQUIRED_CAPS
                    .iter()
                    .filter(|(_, bit)| {
                        let c = caps::capability_state(*bit);
                        c.permitted || c.effective
                    })
                    .map(|(name, _)| *name)
                    .collect();
                eprintln!(
                    "serial-nexus-devprep: refusing to run: this copy holds {} \
                     but cannot establish PR_SET_NO_NEW_PRIVS — {reason}. §15.45 requires that \
                     bound on a blessed copy. Strip the capability (`sudo setcap -r <path>`) \
                     or run the unblessed copy from target/<profile>/.",
                    if held_names.is_empty() {
                        "a required capability".to_owned()
                    } else {
                        held_names.join(",")
                    }
                );
                std::process::exit(exit::REFUSED);
            }
            Unhardened::Report => eprintln!(
                "serial-nexus-devprep: {reason} — harmless in this unblessed copy, which holds \
                 no capability, but a blessed copy on this box will refuse to run (§15.45)."
            ),
        }
    }

    let cli = Cli::parse();
    let code = match cli.verb {
        Verb::Capabilities { json } => report_capabilities(held, hardening.is_ok(), json),
        Verb::Grant { ports, json } => grant_verb(&ports, json),
        Verb::Status { port, json } => status(&port, json),
        Verb::Cycle {
            ports,
            hold_ms,
            dry_run,
            json,
        } => cycle(&ports, hold_ms, dry_run, json, held),
        Verb::Hold {
            ports,
            max_ms,
            dry_run,
        } => hold_until_eof(&ports, max_ms, dry_run, held),
        Verb::Authorize { port, json } => authorize(&port, json, held),
        Verb::Install {
            profile,
            verify,
            print_setcap,
            // The union, not `held`: this refusal exists to keep `Command::new`
            // away from a process carrying *any* capability (§15.45's no-`exec`
            // bound), not only from one carrying `cap_dac_override`.
        } => install_verb(&profile, verify, print_setcap, any_held),
        Verb::Preflight { profile, json } => preflight(&profile, json, held),
    };
    std::process::exit(code);
}

/// Where the repository root is, derived from this binary's own location.
///
/// Both homes are two levels deep — `target/<profile>/` and `.snx-bin/<profile>/` —
/// so the same walk serves a freshly-built binary and a blessed one. Confirmed by
/// finding the workspace manifest, so a binary copied somewhere unexpected says so
/// instead of guessing.
fn repo_root() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot resolve own path: {e}"))?;
    let root = exe
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or_else(|| format!("{} is not two directories deep", exe.display()))?;
    if !root.join("Cargo.toml").is_file() {
        return Err(format!(
            "{} has no Cargo.toml — run this from target/<profile>/ or .snx-bin/<profile>/ \
             inside the checkout",
            root.display()
        ));
    }
    Ok(root.to_owned())
}

/// Report what this process holds and what it has established — the two halves of
/// §15.45's privilege story, from the process that is the subject of both.
///
/// `no_new_privs` is the read-back result from `main`, not a second measurement:
/// the bit cannot be unset, so one answer is the whole life of the process. It is
/// reported rather than merely acted on because a bound nothing ever observes is
/// indistinguishable from one that was never set — which is exactly the defect this
/// field closes.
fn report_capabilities(held: CapState, no_new_privs: bool, json: bool) -> i32 {
    // Reported separately from `cap_dac_override` because they fail differently and
    // the remedy differs: without the first there is no replug at all, without the
    // second the replug works and the device comes back unusable (§15.55). A single
    // `blessed` boolean cannot say which, and "the cycle succeeded but the node is
    // still Permission denied" is exactly the confusion this field removes.
    let fowner = caps::capability_state(caps::CAP_FOWNER);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "cap_dac_override_permitted": held.permitted,
                "cap_dac_override_effective": held.effective,
                "cap_fowner_effective": fowner.effective,
                "can_grant_device_access": fowner.effective,
                // **The whole set, not the first capability** (§15.55). This field is
                // what an external consumer of this JSON reaches for first, and it
                // used to answer `true` for a copy holding `cap_dac_override` alone —
                // while `install --verify` on the same file said `Unblessed` and
                // `preflight` exited BLOCKED_ON_BLESS. One binary must not carry two
                // definitions of "blessed", and the pre-§15.55 one is not the
                // survivor. The per-capability fields above stay exact, so nothing
                // that needs the finer answer loses it.
                "blessed": held.effective && fowner.effective,
                "no_new_privs": no_new_privs,
            })
        );
    } else if held.effective {
        println!("cap_dac_override: held (permitted and effective) — replug verbs will work");
        println!(
            "cap_fowner: {} — {}",
            if fowner.effective { "held" } else { "NOT held" },
            if fowner.effective {
                "device-access grants will work"
            } else {
                "replug works, but the node it brings back stays root:dialout and \
                 unusable; re-run the setcap `install --print-setcap` prints"
            }
        );
    } else if held.permitted {
        println!(
            "cap_dac_override: permitted but NOT effective — the file was blessed `+p` \
             instead of `+ep`; re-run setcap with `+ep`"
        );
    } else {
        println!("cap_dac_override: not held — this copy is not blessed");
    }
    if !json {
        println!(
            "no_new_privs: {} — {}",
            no_new_privs,
            if no_new_privs {
                "PR_SET_NO_NEW_PRIVS set and read back; no exec from here can gain privilege"
            } else {
                "the kernel would not confirm the bit; a blessed copy refuses to run (§15.45)"
            }
        );
    }
    exit::READY
}

/// Validate a port name and the device behind it, or explain the refusal.
fn open_port(name: &str) -> Result<Port, String> {
    let port = sysfs::validate_port_name(name)?;
    sysfs::check_is_a_replugable_serial_adapter(&port)?;
    Ok(port)
}

/// Everything the helper can say about a port without writing anything.
fn port_facts(port: &Port) -> serde_json::Value {
    serde_json::json!({
        "port": port.name(),
        "authorized": sysfs::authorized(port),
        "serial": sysfs::attr(port, "serial"),
        "id_vendor": sysfs::attr(port, "idVendor"),
        "id_product": sysfs::attr(port, "idProduct"),
        // `devnum` is the re-enumeration discriminator: the kernel hands out a new
        // one on every enumeration, so a changed devnum distinguishes a real
        // replug from a driver rebind. Compared for inequality, never ordering —
        // it wraps.
        "devnum": sysfs::attr(port, "devnum"),
        "ttys": sysfs::ttys(port),
    })
}

fn status(name: &str, json: bool) -> i32 {
    let port = match open_port(name) {
        Ok(p) => p,
        Err(e) => return refuse(&e, json),
    };
    let facts = port_facts(&port);
    if json {
        println!("{facts}");
    } else {
        println!(
            "port {} serial={:?} devnum={:?} authorized={:?} ttys={:?}",
            port,
            sysfs::attr(&port, "serial"),
            sysfs::attr(&port, "devnum"),
            sysfs::authorized(&port),
            sysfs::ttys(&port)
        );
    }
    exit::READY
}

fn refuse(message: &str, json: bool) -> i32 {
    if json {
        println!("{}", serde_json::json!({ "error": message }));
    } else {
        eprintln!("serial-nexus-devprep: {message}");
    }
    exit::REFUSED
}

/// The capability gate for the two writing verbs.
fn require_capability(held: CapState, json: bool) -> Option<i32> {
    if held.effective {
        return None;
    }
    let hint = if held.permitted {
        "the copy is blessed `+p` but not `+ep`"
    } else {
        "this copy is not blessed"
    };
    Some(refuse(
        &format!(
            "cap_dac_override is not effective ({hint}). Run \
             `cargo run -p serial-nexus-devprep -- install` and then the sudo command it prints."
        ),
        json,
    ))
}

/// Grant the invoking user read/write on every tty `port` currently owns.
///
/// Returns the nodes granted, or an error naming the one that failed. Silent about
/// a port with no tty: a device mid-enumeration legitimately has none yet, and the
/// callers below have already waited for the tty they care about.
fn grant_ports(ports: &[sysfs::Port]) -> Result<Vec<String>, String> {
    let uid = caps::real_uid();
    let mut granted = Vec::new();
    for port in ports {
        for tty in sysfs::ttys(port) {
            let node = PathBuf::from("/dev").join(&tty);
            serial_nexus_sys::grant_user_rw(&node, uid)
                .map_err(|e| format!("granting {uid} read/write on {}: {e}", node.display()))?;
            granted.push(node.display().to_string());
        }
    }
    Ok(granted)
}

/// What the post-reauthorization grant did — a **value**, not a printed side effect.
///
/// It used to be `Result<Vec<String>, i32>`, and the skip case was folded into the
/// success arm with a note printed from inside the helper *only when the caller was
/// not in JSON mode*. Two consequences, both real (plan §18 item 52 (b)): §15.55's
/// promise that a copy blessed before `cap_fowner` "skips the grant with a note"
/// was silent for every JSON caller, and `hold` — whose two report lines are always
/// JSON — therefore said nothing at all while quietly granting nothing. A caller
/// reading `"granted": []` could not tell "this port owns no tty right now" from
/// "this copy cannot grant and never will".
///
/// Three outcomes, deliberately not two:
///
/// * **Granted** — possibly of zero nodes, which is *not* a skip: a device
///   mid-enumeration legitimately owns no tty yet.
/// * **Skipped** — a stated reason, never an empty success. §15.55's whole point is
///   that a capability which arrived later must not turn every previously-working
///   `cycle` red, and the price of that is saying so out loud.
/// * **Failed** — the copy holds the capability and the grant still failed. A real
///   fault, raised, because the caller asked for a device it can use and would
///   otherwise meet `Permission denied` seconds later with nothing pointing here.
#[derive(Debug)]
enum GrantOutcome {
    Granted(Vec<String>),
    Skipped(&'static str),
    Failed(String),
}

/// Reasons a grant was skipped. Spelled once each, because they are wire-visible:
/// `grant_skipped` carries the reason verbatim.
mod grant_skip {
    /// The dry run's control property: it writes nothing, so nothing lost access.
    pub const DRY_RUN: &str =
        "--dry-run: nothing was deauthorized, so no node lost the caller's access";
    /// §15.55's second residual, in the field a JSON caller reads.
    pub const NO_FOWNER: &str = "this copy does not hold cap_fowner effectively — it was blessed before the \
         capability joined the set (§15.55). The replug worked; re-run `scripts/bless` \
         to get the grant back.";
    /// `hold` only: there is nothing to grant on when the device is still down.
    pub const REAUTHORIZE_FAILED: &str =
        "the reauthorization failed, so there is no returned node to grant access on";
}

impl GrantOutcome {
    /// The nodes granted, for the `granted` field every report already carried.
    fn granted(&self) -> &[String] {
        match self {
            Self::Granted(g) => g,
            _ => &[],
        }
    }

    /// The `grant_skipped` field: the **reason**, or `null`.
    ///
    /// A reason rather than a boolean, for two reasons that both matter. §15.55
    /// promises a *note*, and "true" is not a note — the three skips call for three
    /// different actions. And a JSON consumer writing the naive `if (r.grant_skipped)`
    /// still gets the right answer, because a non-empty string is truthy and `null`
    /// is not, so the strictly-more-informative shape costs a reader nothing.
    fn skipped(&self) -> Option<&'static str> {
        match self {
            Self::Skipped(why) => Some(why),
            _ => None,
        }
    }

    /// The fault, when the copy could grant and did not.
    fn failed(&self) -> Option<&str> {
        match self {
            Self::Failed(e) => Some(e),
            _ => None,
        }
    }

    /// The prose form of [`Self::skipped`], for the callers that print sentences
    /// instead of JSON. `None` when there is nothing to say.
    fn note(&self) -> Option<String> {
        self.skipped()
            .map(|why| format!("note: not granting device access — {why}"))
    }
}

/// Grant after a reauthorization, **best-effort and always announced**.
///
/// The announcing is the caller's now — this returns the announcement rather than
/// printing it — because there are three callers and one of them prints only JSON.
fn grant_after_reauthorize(ports: &[sysfs::Port], dry_run: bool) -> GrantOutcome {
    if dry_run {
        return GrantOutcome::Skipped(grant_skip::DRY_RUN);
    }
    if !caps::capability_state(caps::CAP_FOWNER).effective {
        return GrantOutcome::Skipped(grant_skip::NO_FOWNER);
    }
    match grant_ports(ports) {
        Ok(granted) => GrantOutcome::Granted(granted),
        Err(e) => GrantOutcome::Failed(e),
    }
}

fn grant_verb(names: &[String], json: bool) -> i32 {
    let mut ports = Vec::new();
    for name in names {
        match open_port(name) {
            Ok(p) => ports.push(p),
            Err(e) => return refuse(&e, json),
        }
    }
    if !caps::capability_state(caps::CAP_FOWNER).effective {
        return refuse(
            "cap_fowner is not effective, so no ACL can be set on a root-owned tty node. \
             Run `cargo run -p serial-nexus-devprep -- install` and then the sudo command it \
             prints (§15.55).",
            json,
        );
    }
    match grant_ports(&ports) {
        Ok(granted) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"granted": granted, "uid": caps::real_uid()})
                );
            } else if granted.is_empty() {
                println!("no tty nodes to grant (the ports own none right now)");
            } else {
                println!(
                    "granted uid {} rw on {}",
                    caps::real_uid(),
                    granted.join(", ")
                );
            }
            exit::READY
        }
        Err(e) => refuse(&e, json),
    }
}

fn authorize(name: &str, json: bool, held: CapState) -> i32 {
    let port = match open_port(name) {
        Ok(p) => p,
        Err(e) => return refuse(&e, json),
    };
    if sysfs::authorized(&port) == Some(true) {
        if json {
            println!(
                "{}",
                serde_json::json!({"port": port.name(), "already_authorized": true})
            );
        } else {
            println!("port {port} is already authorized");
        }
        return exit::READY;
    }
    if let Some(code) = require_capability(held, json) {
        return code;
    }
    match sysfs::set_authorized(&port, true) {
        Ok(()) => {
            // The reauthorization is what *destroyed* the caller's access: the node
            // it brings back is a new inode, `root:dialout 0660`, and any ACL on the
            // old one went with it. Restoring that access is part of putting the
            // device back, not a separate favour, so it happens here rather than
            // being left as a step every caller must remember (§15.55).
            let grant = grant_after_reauthorize(std::slice::from_ref(&port), false);
            if let Some(e) = grant.failed() {
                return refuse(e, json);
            }
            // Printed here rather than inside the grant so the ordering is the one it
            // has always had: the note precedes the verb's own answer.
            if !json && let Some(note) = grant.note() {
                eprintln!("{note}");
            }
            let granted = grant.granted();
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "port": port.name(),
                        "authorized": true,
                        "granted": granted,
                        "grant_skipped": grant.skipped(),
                    })
                );
            } else {
                println!("port {port} reauthorized");
                if !granted.is_empty() {
                    println!(
                        "granted uid {} rw on {}",
                        caps::real_uid(),
                        granted.join(", ")
                    );
                }
            }
            exit::READY
        }
        Err(e) => refuse(&format!("writing authorized=1 on {port}: {e}"), json),
    }
}

/// The replug envelope: everything `cycle` and `hold` do identically, written once.
///
/// The two verbs differ in exactly three places — how long the device stays down,
/// what a failed reauthorization means to the caller, and what the caller is told —
/// and until this factoring every *other* step was pasted twice: resolve the port
/// names, gate on the capability, arm crash safety, write `authorized=0` with a
/// rollback, write `authorized=1` in reverse order, hand the caller back access to
/// the node that came back. Roughly ninety duplicated lines, and the duplication had
/// already done what duplication does (plan §18 item 52 (c)): the grant step existed
/// at three sites, one of them had drifted into passing `json: true` at a call whose
/// only effect was to suppress §15.55's promised note, and the reauthorize loops had
/// drifted apart in what they do after the first refusal.
///
/// The rule this establishes: a difference between the two verbs is a **parameter**,
/// or a step the caller takes for itself around the envelope. It cannot go on being
/// an accident of which copy someone last edited.
struct Envelope {
    ports: Vec<Port>,
    dry_run: bool,
    /// Whether a refusal raised inside the envelope prints JSON on stdout instead of
    /// prose on stderr. `cycle` passes its `--json`; `hold` has no such flag and
    /// passes `false`, which is what it has always done.
    json: bool,
    /// The pid that started this process, sampled once before the first write. Both
    /// hold loops re-check it: a parent that dies is a caller that will never close
    /// the leash, and the device has to come back anyway.
    parent: u32,
}

impl Envelope {
    /// Resolve, gate, and arm crash safety — everything that must succeed *before*
    /// any write happens, so a failure here can never leave hardware down.
    ///
    /// `verb` appears in exactly one message and switches no behaviour.
    fn open(
        names: &[String],
        dry_run: bool,
        json: bool,
        held: CapState,
        verb: &str,
    ) -> Result<Self, i32> {
        let mut ports = Vec::new();
        for name in names {
            match open_port(name) {
                Ok(p) => ports.push(p),
                Err(e) => return Err(refuse(&e, json)),
            }
        }
        if !dry_run && let Some(code) = require_capability(held, json) {
            return Err(code);
        }
        // Crash safety, installed before the first write and not before: everything
        // above this point can fail without leaving hardware down.
        let parent = std::os::unix::process::parent_id();
        let _ = caps::catch_terminate();
        let _ = caps::set_pdeathsig_term();
        // The pdeathsig race: if the parent died between spawn and the call above,
        // the signal will never come, so check once explicitly.
        if !caps::parent_is(parent) {
            return Err(refuse(
                &format!("parent exited before the {verb} began; refusing to start"),
                json,
            ));
        }
        Ok(Self {
            ports,
            dry_run,
            json,
            parent,
        })
    }

    /// Deauthorize every port in the order given; the returned instant is the moment
    /// the last one went down.
    ///
    /// A refused write puts back whatever already went down before reporting: a
    /// helper that leaves half a rig deauthorized because the second port refused is
    /// exactly the state §15.45's crash-safety argument exists to prevent.
    fn down(&self) -> Result<Instant, i32> {
        if !self.dry_run {
            for port in &self.ports {
                if let Err(e) = sysfs::set_authorized(port, false) {
                    // Best effort to undo whatever already went down, then report.
                    for done in &self.ports {
                        let _ = sysfs::set_authorized(done, true);
                    }
                    return Err(refuse(
                        &format!("writing authorized=0 on {port}: {e}"),
                        self.json,
                    ));
                }
            }
        }
        Ok(Instant::now())
    }

    /// Reauthorize in **reverse** order, and report every port that refused as
    /// `(port name, error)`.
    ///
    /// Reverse because the device that returns first takes the lowest free minor,
    /// which is how two adapters cycled together come back renumbered — the effect
    /// `identity_survives_a_replug_that_renumbers_the_tty` is built on.
    ///
    /// **Every port is attempted, even after one fails**, and that is where the two
    /// pasted copies had drifted: `hold` collected failures and carried on, `cycle`
    /// returned at the first and left the remaining devices deauthorized with no
    /// further attempt. Neither shape was argued for anywhere. The envelope takes the
    /// one that puts more hardware back — a device left off the bus is the worst
    /// thing this helper can do (AGENTS §8's "a run that died mid-replug leaves a USB
    /// adapter deauthorized") — and each caller still decides what a failure *means*.
    fn up(&self) -> Vec<(String, String)> {
        let mut failures = Vec::new();
        if !self.dry_run {
            for port in self.ports.iter().rev() {
                if let Err(e) = sysfs::set_authorized(port, true) {
                    failures.push((port.name().to_owned(), e.to_string()));
                }
            }
        }
        failures
    }

    /// Hand the invoking user back read/write on every tty these ports own.
    ///
    /// The one grant site. Its outcome is a [`GrantOutcome`] rather than a printed
    /// note, because the three callers report in three different shapes and the
    /// previous arrangement — print from in here, only when the caller was not in
    /// JSON mode — is what made §15.55's promise silent for `hold` (item 52 (b)).
    fn grant(&self) -> GrantOutcome {
        grant_after_reauthorize(&self.ports, self.dry_run)
    }
}

/// The replug. Deauthorize every port, hold, reauthorize in reverse order, then
/// wait for each device's tty to come back.
fn cycle(names: &[String], hold_ms: u64, dry_run: bool, json: bool, held: CapState) -> i32 {
    let env = match Envelope::open(names, dry_run, json, held, "cycle") {
        Ok(e) => e,
        Err(code) => return code,
    };
    let ports = &env.ports;
    let hold = Duration::from_millis(hold_ms).min(MAX_HOLD);

    let before: Vec<serde_json::Value> = ports.iter().map(port_facts).collect();
    let started = Instant::now();

    let down_at = match env.down() {
        Ok(t) => t,
        Err(code) => return code,
    };

    // Hold, watching for a signal so a terminated helper still puts the hardware
    // back. This is the only sleep in the binary and it is the experiment's
    // variable, not a wait for something to become true — which is why it stays here
    // rather than moving into the envelope: it is one of the three things that make
    // `cycle` a different verb from `hold`.
    //
    // The hold is also the **only** window in which the disconnect is observable,
    // which is why the discriminator is sampled here rather than inferred
    // afterwards. `devnum` was the pre-registered discriminator and is **refuted**
    // (notes §3.54): measured on Linux 7.0.0-29, an `authorized` 0→1 cycle does not
    // change it, because deauthorization unbinds the configuration without
    // destroying the `usb_device` object that owns the address. What *does* happen,
    // measured in the same trace, is that the tty is destroyed and `/dev/ttyUSB*`
    // disappears — which is precisely the event the daemon under test experiences.
    // So the helper reports the disappearance it witnessed rather than a number it
    // hoped would move.
    let mut cut_short = false;
    let mut vanished: Vec<bool> = vec![false; ports.len()];
    let mut vanish_ms: Vec<Option<u64>> = vec![None; ports.len()];
    let hold_until = down_at + hold;
    while Instant::now() < hold_until {
        for (i, port) in ports.iter().enumerate() {
            if !vanished[i] && sysfs::ttys(port).is_empty() {
                vanished[i] = true;
                vanish_ms[i] = Some(down_at.elapsed().as_millis() as u64);
            }
        }
        if caps::terminate_requested() || !caps::parent_is(env.parent) {
            cut_short = true;
            break;
        }
        std::thread::sleep(TERMINATE_POLL.min(hold_until - Instant::now()));
    }

    if let Some((port, e)) = env.up().first() {
        return refuse(
            &format!(
                "writing authorized=1 on {port}: {e} — the device is still \
                 deauthorized; repair with `authorize --port {port}`"
            ),
            json,
        );
    }

    // Wait for the kernel, never sleep for it. Reported per port so a failure is
    // attributable to enumeration rather than to whatever the caller does next.
    // `cycle`'s post-condition — every tty is back, or this verb refuses — is the
    // second of the three things that separate it from `hold`, whose contract is
    // that the caller owns the timing; so the wait stays per-verb.
    let mut waits = Vec::new();
    for port in ports {
        let began = Instant::now();
        let mut back = false;
        while began.elapsed() < REENUMERATION_DEADLINE {
            if !sysfs::ttys(port).is_empty() {
                back = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let i = waits.len();
        waits.push(serde_json::json!({
            "port": port.name(),
            "tty_returned": back,
            "tty_wait_ms": began.elapsed().as_millis() as u64,
            // The discriminator. True means this helper *witnessed* the device's
            // tty go away — the disconnect the daemon under test experiences.
            // False under `--dry-run` by construction, which is what makes the
            // dry run a control rather than a claim.
            "tty_vanished_during_hold": vanished[i],
            "tty_vanished_after_ms": vanish_ms[i],
        }));
        if !back && !dry_run {
            return refuse(
                &format!(
                    "port {port} produced no tty within {}s of reauthorization",
                    REENUMERATION_DEADLINE.as_secs()
                ),
                json,
            );
        }
    }

    // Every tty is back by here (the wait loop above refuses otherwise), so this is
    // the moment the caller's access can be restored — and it must be, because the
    // node the cycle brought back is a new inode that inherited nothing (§15.55).
    let grant = env.grant();
    if let Some(e) = grant.failed() {
        return refuse(e, json);
    }
    if !json && let Some(note) = grant.note() {
        eprintln!("{note}");
    }

    let after: Vec<serde_json::Value> = ports.iter().map(port_facts).collect();
    let report = serde_json::json!({
        "dry_run": dry_run,
        "hold_ms": hold.as_millis() as u64,
        "hold_cut_short_by_signal": cut_short,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "before": before,
        "after": after,
        "waits": waits,
        "granted": grant.granted(),
        // §15.55's note, in the field a JSON caller can read. `null` when the grant
        // happened; a reason, never a bare `true`, when it did not (item 52 (b)).
        "grant_skipped": grant.skipped(),
    });
    if json {
        println!("{report}");
    } else {
        println!(
            "cycled {} port(s) in {} ms{}",
            ports.len(),
            started.elapsed().as_millis(),
            if dry_run { " (dry run: no writes)" } else { "" }
        );
    }
    if cut_short { exit::FAILED } else { exit::READY }
}

/// Down, hold until the caller closes stdin (or `max_ms`), up.
///
/// The whole privileged surface of a replug test, and deliberately the *entire*
/// contribution of this binary to one: no sampling, no measurement, no experiment
/// design. Those live in the caller, where changing them costs a rebuild and not a
/// `sudo`.
///
/// Prints one JSON line when the device is down and one when it is back up, both
/// flushed, so a caller can synchronise on them rather than sleeping.
fn hold_until_eof(names: &[String], max_ms: u64, dry_run: bool, held: CapState) -> i32 {
    use std::io::{BufRead, Write};

    // `json: false` — `hold` has no `--json` flag. Its two report lines are always
    // JSON; a *refusal* raised before the protocol starts is prose on stderr, which
    // is what a caller that never got a `down` line can actually read.
    let env = match Envelope::open(names, dry_run, false, held, "hold") {
        Ok(e) => e,
        Err(code) => return code,
    };
    let ceiling = Duration::from_millis(max_ms).min(MAX_HOLD);

    let down_at = match env.down() {
        Ok(t) => t,
        Err(code) => return code,
    };
    println!(
        "{}",
        serde_json::json!({
            "state": "down",
            "dry_run": dry_run,
            "ports": env.ports.iter().map(|p| p.name()).collect::<Vec<_>>(),
            "max_hold_ms": ceiling.as_millis() as u64,
        })
    );
    let _ = std::io::stdout().flush();

    // Wait for EOF on stdin. A caller that dies — for any reason, including
    // SIGKILL — closes its end, so this returns and the device comes back. The
    // ceiling covers a caller that lives but never closes. This is the first of the
    // three things that make `hold` a different verb from `cycle`: the outage's
    // length belongs to the caller, not to this binary.
    let released_by = {
        let stdin = std::io::stdin();
        let mut line = String::new();
        let mut reason = "stdin-eof";
        loop {
            if down_at.elapsed() >= ceiling {
                reason = "max-ms";
                break;
            }
            if caps::terminate_requested() || !caps::parent_is(env.parent) {
                reason = "signal";
                break;
            }
            // A short read with a deadline: `read_line` on a pipe blocks until the
            // writer writes or closes, so the loop is driven by the reader thread
            // below rather than by polling stdin directly.
            line.clear();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => break,    // EOF: the caller is done or gone
                Ok(_) => continue, // a caller may write to keep it alive
                Err(_) => break,
            }
        }
        reason
    };

    let failures = env.up();
    let grant = if failures.is_empty() {
        // The device is back; the node it came back on is new and inaccessible.
        // Wait briefly for the tty, then restore the caller's access to it (§15.55).
        // Bounded and non-fatal: `hold`'s contract is that the caller owns the
        // timing, so a tty that has not appeared yet is reported as nothing granted
        // rather than as a failure of the hold. That is the third difference from
        // `cycle`, whose wait refuses — and it is why the wait is here and not in
        // the envelope.
        if !dry_run {
            let began = Instant::now();
            while began.elapsed() < REENUMERATION_DEADLINE
                && env.ports.iter().any(|p| sysfs::ttys(p).is_empty())
            {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        env.grant()
    } else {
        GrantOutcome::Skipped(grant_skip::REAUTHORIZE_FAILED)
    };

    let (up_line, conclusion) = hold_conclusion(
        down_at.elapsed().as_millis() as u64,
        released_by,
        &failures,
        &grant,
    );
    println!("{up_line}");
    let _ = std::io::stdout().flush();
    match conclusion {
        HoldExit::Ready => exit::READY,
        HoldExit::ReauthorizeFailed => exit::FAILED,
        // The `up` line is already out, which is the whole of item 52 (b)'s second
        // half. What follows is the refusal object and the exit code this case has
        // always produced — now *after* the line the two-line protocol promises,
        // rather than instead of it.
        HoldExit::GrantFailed(e) => refuse(&e, true),
    }
}

/// What `hold` leaves with once the device is back.
#[derive(Debug, PartialEq, Eq)]
enum HoldExit {
    Ready,
    /// A port refused `authorized=1`. The device may still be down; the failures are
    /// in the `up` line.
    ReauthorizeFailed,
    /// The device came back and the caller's access did not.
    GrantFailed(String),
}

/// `hold`'s `up` line and its exit, produced **as one value**.
///
/// The defect plan §18 item 52 (b) names is a `return` in the middle of this tail: a
/// grant failure left the verb before the `up` line its two-line protocol promises,
/// so a caller reading line-by-line got a `down`, then a refusal object where the
/// `up` belonged, and no `held_ms`, `released_by` or `reauthorize_failures` at all —
/// on the one path where knowing whether the device came back matters most.
///
/// Expressing the conclusion as data rather than as control flow is what makes that
/// unrepresentable: the line and the exit are computed together, so there is no order
/// of statements in which the caller can reach the second without the first. It is
/// also what makes the property testable at all — no unprivileged test can make a
/// real grant fail, so the branch is asserted directly rather than through a proxy
/// (AGENTS §9), exactly as [`unhardened_disposition`] is.
///
/// Precedence is the one the verb has always had: a reauthorization failure outranks
/// a grant failure, because a device still deauthorized is the worse state and the
/// grant is skipped in that case rather than attempted.
fn hold_conclusion(
    held_ms: u64,
    released_by: &str,
    failures: &[(String, String)],
    grant: &GrantOutcome,
) -> (serde_json::Value, HoldExit) {
    let line = serde_json::json!({
        "state": "up",
        "held_ms": held_ms,
        "released_by": released_by,
        "reauthorize_failures": failures
            .iter()
            .map(|(port, e)| format!("{port}: {e}"))
            .collect::<Vec<_>>(),
        "granted": grant.granted(),
        // §15.55's promised note, in the only shape `hold` can carry one: it prints
        // no prose at all, so before this field the promise was silent here (item 52).
        "grant_skipped": grant.skipped(),
        "grant_failed": grant.failed(),
    });
    let conclusion = if !failures.is_empty() {
        HoldExit::ReauthorizeFailed
    } else if let Some(e) = grant.failed() {
        HoldExit::GrantFailed(e.to_owned())
    } else {
        HoldExit::Ready
    };
    (line, conclusion)
}

fn install_verb(profile: &str, verify: bool, print_setcap: bool, held: CapState) -> i32 {
    // Printing the command is a pure string derivation — no copy, no capability, no
    // spawn — so it is answered before the blessed-copy refusal below, which exists
    // to keep `Command::new` away from a process holding capabilities.
    if print_setcap {
        let root = match repo_root() {
            Ok(r) => r,
            Err(e) => return refuse(&e, false),
        };
        println!(
            "{}",
            install::setcap_command(&install::blessed_path(&root, profile))
        );
        return exit::READY;
    }
    // A blessed process must never spawn anything, and `install` shells out to
    // `getcap`. Refusing here is what keeps `CAP_DAC_OVERRIDE` and `Command::new`
    // from ever being live in the same process.
    if held.permitted || held.effective {
        return refuse(
            "refusing to run `install` from a blessed copy — run it from \
             `target/<profile>/serial-nexus-devprep` (or via `cargo run -p serial-nexus-devprep`)",
            false,
        );
    }
    let root = match repo_root() {
        Ok(r) => r,
        Err(e) => return refuse(&e, false),
    };
    if !verify {
        match install::install(&root, profile) {
            Ok(path) => {
                println!("installed {}", path.display());
                println!("now run, once:\n    {}", install::setcap_command(&path));
            }
            Err(e) => return refuse(&format!("{e}"), false),
        }
    }
    sweep_orphaned_capabilities(&root, profile);
    match install::inspect(&root, profile) {
        Ok(state) => {
            let path = install::blessed_path(&root, profile);
            match state {
                install::InstallState::Ready => {
                    // Derived, never typed: this line printed the pre-§15.55
                    // single-capability form until 2026-08-12 (plan §18 item 52 (e)),
                    // naming a smaller grant than the file it was describing carried.
                    println!(
                        "{}: ready ({})",
                        path.display(),
                        install::ready_description()
                    );
                    exit::READY
                }
                other => {
                    println!("{}: {other:?}", path.display());
                    println!("fix with:\n    {}", install::setcap_command(&path));
                    exit::BLOCKED_ON_BLESS
                }
            }
        }
        Err(e) => refuse(&format!("inspecting the install: {e}"), false),
    }
}

/// Remove any capability-carrying file left in `.snx-bin/<profile>/` that is not the
/// copy that belongs there, and say so on stderr.
///
/// **Why this runs on `--verify` too.** `--verify` promises to report the state and
/// change nothing — and what it promises that about is the *installed copy*, plus the
/// password it will not ask for. Unlinking a stray is neither: it needs no privilege
/// (write permission on a directory this tool created is enough — `setcap -r` would
/// need root, which is why the sweep deletes rather than strips), and the stray is
/// not part of the state being reported. Running it on both paths is what makes the
/// invariant hold rather than merely be available: **every** `scripts/bless` run
/// leaves exactly one capability-carrying file per profile directory, including the
/// already-blessed fast path, which calls nothing but `install --verify`. A `--verify`
/// that noticed a live root-equivalent capability nothing references and left it
/// there would have done the diagnosis and skipped the cure, on the one class of file
/// §15.45's narrowness argument is entirely about (notes §3.81 — the rename left
/// exactly such an orphan).
///
/// Reported on stderr rather than stdout so anything parsing this verb's answer is
/// unaffected, and **never fatal**: a directory that cannot be read is worth saying
/// out loud and is not a reason to fail an install that otherwise worked.
fn sweep_orphaned_capabilities(root: &std::path::Path, profile: &str) {
    match install::sweep_orphans(root, profile) {
        Ok(swept) => {
            for entry in &swept {
                let whose = if entry.orphan.ours {
                    "a capability this tool grants"
                } else {
                    "a capability this tool never grants — worth a look at where it came from"
                };
                match &entry.error {
                    None => eprintln!(
                        "serial-nexus-devprep: removed orphaned blessed copy {} \
                         (carried {}, {whose}); nothing references it and \
                         `scripts/bless` reproduces it",
                        entry.orphan.path.display(),
                        entry.orphan.field,
                    ),
                    Some(e) => eprintln!(
                        "serial-nexus-devprep: could NOT remove orphaned blessed copy {} \
                         (carries {}, {whose}): {e} — delete it by hand",
                        entry.orphan.path.display(),
                        entry.orphan.field,
                    ),
                }
            }
        }
        Err(e) => eprintln!(
            "serial-nexus-devprep: could not sweep {}/{profile} for orphaned \
             capabilities: {e}",
            install::BIN_DIR,
        ),
    }
}

/// Is the replug capability usable here?
///
/// Two buckets, and **environmental dominates**: on a box with no USB serial
/// adapter *and* no blessing the answer is `NOT_READY`, never `BLOCKED_ON_BLESS`,
/// because sending someone to run `sudo setcap` on a host that still cannot run the
/// test wastes the one privileged step this design asks of them.
fn preflight(profile: &str, json: bool, held: CapState) -> i32 {
    let mut environmental: Vec<String> = Vec::new();
    let mut bless: Vec<String> = Vec::new();

    if !cfg!(target_os = "linux") {
        environmental.push("not Linux: USB authorized(5) is a Linux interface".to_owned());
    }
    let adapters = discover_serial_ports();
    if adapters.is_empty() {
        environmental.push(format!(
            "no USB serial adapter found under {}",
            sysfs::USB_DEVICES
        ));
    }
    match repo_root().and_then(|root| {
        install::inspect(&root, profile).map_err(|e| format!("inspecting the install: {e}"))
    }) {
        Ok(install::InstallState::Ready) => {}
        Ok(state) => bless.push(format!("{state:?}")),
        Err(e) => bless.push(e),
    }
    if !held.effective {
        // Only meaningful when this process *is* the blessed copy; when it is not,
        // the install inspection above is the real signal. Recorded either way.
        bless.push("this process does not hold cap_dac_override effectively".to_owned());
    }

    let (code, sentinel) = if !environmental.is_empty() {
        (
            exit::NOT_READY,
            "REPLUG-PREFLIGHT: NOT READY — a genuinely absent facility remains",
        )
    } else if !bless.is_empty() {
        (
            exit::BLOCKED_ON_BLESS,
            "REPLUG-PREFLIGHT: BLOCKED-ON-BLESS — ask the maintainer for one `sudo setcap`",
        )
    } else {
        (exit::READY, "REPLUG-PREFLIGHT: READY")
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "verdict": sentinel,
                "exit": code,
                "environmental_problems": environmental,
                "bless_problems": bless,
                "adapters": adapters,
            })
        );
    } else {
        for problem in &environmental {
            println!("  environmental: {problem}");
        }
        for problem in &bless {
            let tag = if environmental.is_empty() {
                ""
            } else {
                " (also, but not the blocker)"
            };
            println!("  bless{tag}: {problem}");
        }
        println!("  adapters: {adapters:?}");
        println!("{sentinel}");
    }
    code
}

/// Every USB port under [`sysfs::USB_DEVICES`] that passes the serial-adapter
/// checks. Read-only; used by the preflight and by operator-facing messages.
fn discover_serial_ports() -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(sysfs::USB_DEVICES) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(port) = sysfs::validate_port_name(&name) else {
            continue;
        };
        if sysfs::check_is_a_replugable_serial_adapter(&port).is_ok() {
            found.push(name);
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unestablished `PR_SET_NO_NEW_PRIVS` is fatal exactly when this copy is
    /// blessed, and `+p`-only counts as blessed.
    ///
    /// This is the half of the fix a suite can reach. The other half — a `prctl`
    /// that actually fails — needs either a pre-3.5 kernel or a seccomp policy that
    /// filters the call, and the refusal it triggers needs a process holding
    /// `CAP_DAC_OVERRIDE`; neither is producible from an unprivileged test on this
    /// box, so the decision is factored out of `main` and asserted here rather than
    /// asserted through a proxy that would pass for the wrong reason (AGENTS §9).
    #[test]
    fn an_unestablished_bound_is_fatal_exactly_when_the_copy_is_blessed() {
        assert_eq!(
            unhardened_disposition(CapState::NONE),
            Unhardened::Report,
            "an unblessed copy holds nothing to protect and must stay usable"
        );
        assert_eq!(
            unhardened_disposition(CapState {
                permitted: true,
                effective: true
            }),
            Unhardened::Refuse
        );
        assert_eq!(
            unhardened_disposition(CapState {
                permitted: true,
                effective: false
            }),
            Unhardened::Refuse,
            "`+p` without `+e` is one capset(2) away from effective, and this process \
             could make that call itself"
        );
        assert_eq!(
            unhardened_disposition(CapState {
                permitted: false,
                effective: true
            }),
            Unhardened::Refuse,
            "the kernel does not produce effective-without-permitted, but if it ever \
             appeared it is privilege, and privilege refuses"
        );
    }

    /// The bounds question folds **every** required capability, so a copy carrying
    /// only the second one is still a blessed process (§15.45, §15.55).
    ///
    /// **Fail-first, recorded.** Before 2026-08-12 `main` read
    /// `capability_state(CAP_DAC_OVERRIDE)` alone, so this case — `cap_fowner` held,
    /// `cap_dac_override` not — folded to `NONE` and took the `Report` arm: a
    /// capability-carrying process continuing unhardened. Reverting `caps_held` to
    /// the single bit turns the second assertion below red, which is the fail-first
    /// proof; the first two assertions pass either way and are what pins the fold's
    /// *shape* rather than only its new case.
    #[test]
    fn the_bounds_question_folds_every_required_capability_not_just_the_first() {
        let none = CapState::NONE;
        let held = CapState {
            permitted: true,
            effective: true,
        };
        assert_eq!(
            fold_cap_states([none, none].into_iter()),
            CapState::NONE,
            "no capability anywhere is an unblessed process"
        );
        assert_eq!(
            fold_cap_states([none, held].into_iter()),
            held,
            "cap_fowner alone is still privilege: the union, never the first entry"
        );
        assert_eq!(
            fold_cap_states([held, none].into_iter()),
            held,
            "and so is cap_dac_override alone — an intersection would let both \
             half-blessed shapes through the refusal that exists to contain them"
        );
        // The set is what the fold is over, so a third entry inherits the *fold's*
        // rule without a line changing here — which is why this assertion is not
        // about the fold at all.
        //
        // **It is `assert_eq!`, not a `>=` floor, and that is the point.** AGENTS §4
        // and §15.45 make the helper's narrowness the whole safety argument, and say
        // in terms that "adding a capability is likewise an amendment" — a design
        // decision with a recorded entry, not a patch. A floor cannot tell an
        // amendment from a typo: `REQUIRED_CAPS.len() >= 2` passes just as happily
        // for a set carrying `cap_sys_admin`, and the tripwire that calls a third
        // capability an amendment would have been enforced by nothing at all. This
        // was a `>=` until 2026-08-12, found by an audit asking of each recorded
        // measure whether its guard could actually fail.
        //
        // So a third capability reddens here, deliberately and loudly. **The fix is
        // not to edit this number.** It is to write the §15 amendment first, then
        // change the constant and this assertion in the same commit as the entry
        // that authorises it — the amend-first order (AGENTS §5).
        assert_eq!(
            install::REQUIRED_CAPS.len(),
            2,
            "REQUIRED_CAPS carries {} capabilities; §15.45/§15.55 bound the helper at \
             TWO (`cap_dac_override` for the sysfs write, `cap_fowner` for the ACL \
             grant). Adding one is a design amendment, not a patch: write the §15 \
             entry that authorises it, then change the constant and this assertion \
             together with it (AGENTS §4, §5).",
            install::REQUIRED_CAPS.len()
        );
    }

    /// §15.55's skip-with-a-note is a **field**, not a prose side effect — which is
    /// what makes it visible to the JSON callers that are the majority of them
    /// (plan §18 item 52 (b)).
    ///
    /// **Fail-first, recorded.** Reverting [`GrantOutcome::skipped`] to `None` — the
    /// shape the tree had, where the skip was folded into an empty-success `Ok(vec![])`
    /// and the note printed only when the caller was *not* in JSON mode — turns the
    /// three `grant_skipped` assertions below red at once. It also, as the third
    /// assertion pins, makes "granted nothing because there is no tty right now"
    /// indistinguishable from "granted nothing because this copy never can".
    #[test]
    fn a_skipped_grant_says_why_in_the_json_and_not_only_in_prose() {
        let skipped = GrantOutcome::Skipped(grant_skip::NO_FOWNER);
        assert_eq!(skipped.skipped(), Some(grant_skip::NO_FOWNER));
        assert!(skipped.granted().is_empty());
        assert_eq!(skipped.failed(), None);
        assert!(
            skipped.note().is_some_and(|n| n.contains("cap_fowner")),
            "the prose callers still get the sentence §15.55 promises"
        );

        // The distinction the field exists to draw: a real grant of zero nodes.
        let nothing_to_do = GrantOutcome::Granted(Vec::new());
        assert_eq!(
            nothing_to_do.skipped(),
            None,
            "a port that owns no tty right now was granted, not skipped — and only \
             `grant_skipped` can tell a caller which of the two produced `granted: []`"
        );
        assert_eq!(nothing_to_do.note(), None);

        let failed = GrantOutcome::Failed("setxattr: EPERM".to_owned());
        assert_eq!(failed.failed(), Some("setxattr: EPERM"));
        assert_eq!(
            failed.skipped(),
            None,
            "a fault is raised, never reported as a skip: §15.55 makes exactly one of \
             the two survivable"
        );

        // A dry run grants nothing because nothing lost access — a third reason,
        // and the control property depends on saying so rather than on an empty list.
        assert_eq!(
            GrantOutcome::Skipped(grant_skip::DRY_RUN).skipped(),
            Some(grant_skip::DRY_RUN)
        );
    }

    /// `hold` emits its `up` line **before** it reports a grant failure, because the
    /// line and the exit are one value (plan §18 item 52 (b)).
    ///
    /// The branch is unreachable from an unprivileged suite — making a real grant
    /// fail needs a process holding `CAP_FOWNER` and a tty it cannot write — so the
    /// decision is factored out of the verb and asserted here rather than through a
    /// proxy that would pass for the wrong reason (AGENTS §9), exactly as
    /// [`unhardened_disposition`] is.
    ///
    /// **Fail-first, recorded.** The defect is the shape the tree had: a grant
    /// failure `return`ed `refuse(...)` from the middle of the tail, so no `up` line
    /// was printed at all. Planted here as a [`hold_conclusion`] that answers
    /// `(Value::Null, HoldExit::GrantFailed(..))` on that branch, the first assertion
    /// below goes red on `state`.
    #[test]
    fn hold_reports_up_even_when_the_grant_failed() {
        let (line, conclusion) = hold_conclusion(
            1500,
            "stdin-eof",
            &[],
            &GrantOutcome::Failed("granting 1000 read/write on /dev/ttyUSB0: EPERM".to_owned()),
        );
        assert_eq!(
            line["state"], "up",
            "the two-line protocol promises an `up` line; a caller reading it \
             line-by-line must not be handed a refusal where the line belongs"
        );
        assert_eq!(line["held_ms"], 1500);
        assert_eq!(line["released_by"], "stdin-eof");
        assert!(
            line["grant_failed"]
                .as_str()
                .is_some_and(|s| s.contains("EPERM")),
            "and the failure travels in the line, not only in the exit code: {line}"
        );
        assert!(
            matches!(conclusion, HoldExit::GrantFailed(ref e) if e.contains("EPERM")),
            "the exit still says the grant failed"
        );

        // Precedence, unchanged: a device still deauthorized outranks a lost ACL,
        // and the grant is skipped rather than attempted in that case.
        let (line, conclusion) = hold_conclusion(
            10,
            "signal",
            &[("3-1".to_owned(), "EIO".to_owned())],
            &GrantOutcome::Skipped(grant_skip::REAUTHORIZE_FAILED),
        );
        assert_eq!(line["state"], "up");
        assert_eq!(line["reauthorize_failures"][0], "3-1: EIO");
        assert_eq!(line["grant_skipped"], grant_skip::REAUTHORIZE_FAILED);
        assert_eq!(conclusion, HoldExit::ReauthorizeFailed);

        // And the ordinary path: an `up` line with a grant and no excuse on it.
        let (line, conclusion) = hold_conclusion(
            200,
            "max-ms",
            &[],
            &GrantOutcome::Granted(vec!["/dev/ttyUSB0".to_owned()]),
        );
        assert_eq!(line["granted"][0], "/dev/ttyUSB0");
        assert_eq!(line["grant_skipped"], serde_json::Value::Null);
        assert_eq!(line["grant_failed"], serde_json::Value::Null);
        assert_eq!(conclusion, HoldExit::Ready);
    }
}
