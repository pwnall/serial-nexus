//! `serial-nexus-replug` — deauthorize and reauthorize one named USB serial
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
//! This binary is blessed with **one** file capability, `CAP_DAC_OVERRIDE`, on a
//! copy installed at `.snx-bin/<profile>/` (see [`install`]). That capability is a
//! large grant — it bypasses every DAC check on the system — so what bounds it is
//! this binary's narrowness, asserted by construction:
//!
//! * **argv only.** It reads no environment variable, so there is no `LD_PRELOAD`
//!   or config-file path into a capability-holding process.
//! * **no `exec`.** It never spawns anything while privileged; `PR_SET_NO_NEW_PRIVS`
//!   makes that a kernel guarantee rather than a property of the code, and the one
//!   module that does spawn (`install`) is refused when the capability is held.
//! * **no path parameter.** The only device argument is a USB port *name* like
//!   `3-1`, validated against a digit/`-`/`.` alphabet and joined to a compile-time
//!   root, so no caller-supplied byte ever reaches `open(2)` as a path.
//! * **the kernel confirms the target filesystem.** `fstatfs` must report
//!   `SYSFS_MAGIC` before any write, which a `starts_with` on a canonicalized path
//!   cannot establish.
//! * **the device must be a non-hub serial adapter**, or the write is refused.
//!
//! Adding a verb that accepts a filesystem path would dissolve every one of those
//! bounds at once. That is a design amendment (§15.45), never a patch.

#![forbid(unsafe_code)]

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
    name = "serial-nexus-replug",
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

fn main() {
    let cli = Cli::parse();
    let held = caps::capability_state(CAP_DAC_OVERRIDE);

    // Hardening first, before anything reads a file or looks at an argument. Cheap,
    // and it means every path below runs under the same restrictions.
    let _ = caps::set_no_new_privs();

    let code = match cli.verb {
        Verb::Capabilities { json } => report_capabilities(held, json),
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
        Verb::Install { profile, verify } => install_verb(&profile, verify, held),
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

fn report_capabilities(held: CapState, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "cap_dac_override_permitted": held.permitted,
                "cap_dac_override_effective": held.effective,
                "blessed": held.effective,
            })
        );
    } else if held.effective {
        println!("cap_dac_override: held (permitted and effective) — replug verbs will work");
    } else if held.permitted {
        println!(
            "cap_dac_override: permitted but NOT effective — the file was blessed `+p` \
             instead of `+ep`; re-run setcap with `+ep`"
        );
    } else {
        println!("cap_dac_override: not held — this copy is not blessed");
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
        eprintln!("serial-nexus-replug: {message}");
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
             `cargo run -p serial-nexus-replug -- install` and then the sudo command it prints."
        ),
        json,
    ))
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
            if json {
                println!(
                    "{}",
                    serde_json::json!({"port": port.name(), "authorized": true})
                );
            } else {
                println!("port {port} reauthorized");
            }
            exit::READY
        }
        Err(e) => refuse(&format!("writing authorized=1 on {port}: {e}"), json),
    }
}

/// The replug. Deauthorize every port, hold, reauthorize in reverse order, then
/// wait for each device's tty to come back.
fn cycle(names: &[String], hold_ms: u64, dry_run: bool, json: bool, held: CapState) -> i32 {
    let mut ports = Vec::new();
    for name in names {
        match open_port(name) {
            Ok(p) => ports.push(p),
            Err(e) => return refuse(&e, json),
        }
    }
    if !dry_run && let Some(code) = require_capability(held, json) {
        return code;
    }
    let hold = Duration::from_millis(hold_ms).min(MAX_HOLD);

    // Crash safety, installed before the first write and not before: everything
    // above this point can fail without leaving hardware down.
    let parent = std::os::unix::process::parent_id();
    let _ = caps::catch_terminate();
    let _ = caps::set_pdeathsig_term();
    // The pdeathsig race: if the parent died between spawn and the call above, the
    // signal will never come, so check once explicitly.
    if !caps::parent_is(parent) {
        return refuse(
            "parent exited before the cycle began; refusing to start",
            json,
        );
    }

    let before: Vec<serde_json::Value> = ports.iter().map(port_facts).collect();
    let started = Instant::now();

    // Down, in the order given.
    if !dry_run {
        for port in &ports {
            if let Err(e) = sysfs::set_authorized(port, false) {
                // Best effort to undo whatever already went down, then report.
                for done in &ports {
                    let _ = sysfs::set_authorized(done, true);
                }
                return refuse(&format!("writing authorized=0 on {port}: {e}"), json);
            }
        }
    }

    // Hold, watching for a signal so a terminated helper still puts the hardware
    // back. This is the only sleep in the binary and it is the experiment's
    // variable, not a wait for something to become true.
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
    let down_at = Instant::now();
    let hold_until = down_at + hold;
    while Instant::now() < hold_until {
        for (i, port) in ports.iter().enumerate() {
            if !vanished[i] && sysfs::ttys(port).is_empty() {
                vanished[i] = true;
                vanish_ms[i] = Some(down_at.elapsed().as_millis() as u64);
            }
        }
        if caps::terminate_requested() || !caps::parent_is(parent) {
            cut_short = true;
            break;
        }
        std::thread::sleep(TERMINATE_POLL.min(hold_until - Instant::now()));
    }

    // Up, in reverse: the device that returns first takes the lowest free minor,
    // which is how two adapters cycled together come back renumbered.
    if !dry_run {
        for port in ports.iter().rev() {
            if let Err(e) = sysfs::set_authorized(port, true) {
                return refuse(
                    &format!(
                        "writing authorized=1 on {port}: {e} — the device is still \
                         deauthorized; repair with `authorize --port {port}`"
                    ),
                    json,
                );
            }
        }
    }

    // Wait for the kernel, never sleep for it. Reported per port so a failure is
    // attributable to enumeration rather than to whatever the caller does next.
    let mut waits = Vec::new();
    for port in &ports {
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

    let after: Vec<serde_json::Value> = ports.iter().map(port_facts).collect();
    let report = serde_json::json!({
        "dry_run": dry_run,
        "hold_ms": hold.as_millis() as u64,
        "hold_cut_short_by_signal": cut_short,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "before": before,
        "after": after,
        "waits": waits,
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

    let mut ports = Vec::new();
    for name in names {
        match open_port(name) {
            Ok(p) => ports.push(p),
            Err(e) => return refuse(&e, false),
        }
    }
    if !dry_run && let Some(code) = require_capability(held, false) {
        return code;
    }
    let ceiling = Duration::from_millis(max_ms).min(MAX_HOLD);

    let parent = std::os::unix::process::parent_id();
    let _ = caps::catch_terminate();
    let _ = caps::set_pdeathsig_term();
    if !caps::parent_is(parent) {
        return refuse(
            "parent exited before the hold began; refusing to start",
            false,
        );
    }

    if !dry_run {
        for port in &ports {
            if let Err(e) = sysfs::set_authorized(port, false) {
                for done in &ports {
                    let _ = sysfs::set_authorized(done, true);
                }
                return refuse(&format!("writing authorized=0 on {port}: {e}"), false);
            }
        }
    }
    let down_at = Instant::now();
    println!(
        "{}",
        serde_json::json!({
            "state": "down",
            "dry_run": dry_run,
            "ports": ports.iter().map(|p| p.name()).collect::<Vec<_>>(),
            "max_hold_ms": ceiling.as_millis() as u64,
        })
    );
    let _ = std::io::stdout().flush();

    // Wait for EOF on stdin. A caller that dies — for any reason, including
    // SIGKILL — closes its end, so this returns and the device comes back. The
    // ceiling covers a caller that lives but never closes.
    let released_by = {
        let stdin = std::io::stdin();
        let mut line = String::new();
        let mut reason = "stdin-eof";
        loop {
            if down_at.elapsed() >= ceiling {
                reason = "max-ms";
                break;
            }
            if caps::terminate_requested() || !caps::parent_is(parent) {
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

    let mut failures = Vec::new();
    if !dry_run {
        for port in ports.iter().rev() {
            if let Err(e) = sysfs::set_authorized(port, true) {
                failures.push(format!("{port}: {e}"));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "state": "up",
            "held_ms": down_at.elapsed().as_millis() as u64,
            "released_by": released_by,
            "reauthorize_failures": failures,
        })
    );
    let _ = std::io::stdout().flush();
    if failures.is_empty() {
        exit::READY
    } else {
        exit::FAILED
    }
}

fn install_verb(profile: &str, verify: bool, held: CapState) -> i32 {
    // A blessed process must never spawn anything, and `install` shells out to
    // `getcap`. Refusing here is what keeps `CAP_DAC_OVERRIDE` and `Command::new`
    // from ever being live in the same process.
    if held.permitted || held.effective {
        return refuse(
            "refusing to run `install` from a blessed copy — run it from \
             `target/<profile>/serial-nexus-replug` (or via `cargo run -p serial-nexus-replug`)",
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
    match install::inspect(&root, profile) {
        Ok(state) => {
            let path = install::blessed_path(&root, profile);
            match state {
                install::InstallState::Ready => {
                    println!(
                        "{}: ready (mode 0700, cap_dac_override +ep)",
                        path.display()
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
