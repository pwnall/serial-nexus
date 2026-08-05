//! §12's identity promise, measured against a **real** USB re-enumeration
//! (design §15.45, notes §3.54).
//!
//! # What this adds over `p7_replug.rs`
//!
//! `p7_replug.rs` proves the daemon heals: it re-links a fixture sysfs tree under
//! `--dev-root` and respawns a sim pty at a fixed path. That exercises the waiting
//! → active transition and the reopen ritual, but the kernel never enumerates
//! anything — the device object never goes away, `ftdi_sio` never unbinds, udev
//! never rebuilds `/dev/serial/by-id`, and no `/dev` name ever changes. So the one
//! sentence P4 prints on success — *"Resolver produces canonical identities;
//! configs survive replug and cold start"* — and §12's *"identity-to-current-path
//! at every open and every faulted-and-wait recheck"* were, until this file, claims
//! with no measurement behind them on any kernel.
//!
//! These tests drive a genuine deauthorize/reauthorize of a real FTDI adapter
//! through `serial-nexus-replug` (the one blessed binary, §15.45) and assert what
//! the fixture cannot: that the helper **witnessed the tty be destroyed** while it
//! held the device down, while the **identity did not move**, and that the daemon
//! comes back open on whatever `/dev` path the kernel chose this time.
//!
//! # The discriminator, and the one that was refuted
//!
//! `devnum` was pre-registered as the discriminator, on the reasoning that a driver
//! rebind leaves it alone while a real re-enumeration always changes it. **Measured
//! on Linux 7.0.0-29, it does not change across an `authorized` 0→1 cycle** — the
//! `usb_device` object that owns the address survives deauthorization, so only the
//! configuration is unbound and rebound. It is recorded here, never asserted.
//!
//! What the same trace *did* show, sampled every 50 ms: the tty count under the
//! interface goes 1 → 0 → 1 and `/dev/ttyUSB0` disappears and returns. That is
//! precisely the event the daemon under test experiences, so the helper now reports
//! the disappearance it witnessed during its own hold window
//! (`tty_vanished_during_hold`), and that is what these tests assert on. Notes
//! §3.54 carries the refutation.
//!
//! # Why the rig variable here is a by-id path
//!
//! Re-enumeration may hand the adapter a different `ttyUSBn`. That is the premise
//! §12 is built on (design §1: *"the same adapter does not always return as the
//! same `/dev` path"*), and it means a `/dev/ttyUSB0` argument names something this
//! very test can invalidate. `SNX_REPLUG_DEV` therefore takes a
//! `/dev/serial/by-id/...` link, and the translation to the sysfs port the helper
//! wants happens here, unprivileged — which is also what keeps `/dev` names out of
//! the capability-carrying binary entirely.
//!
//! # Serialization
//!
//! These tests own the physical adapters for their duration, exactly as
//! `serial_hardware.rs` and `p12_serial_exclusivity.rs` do. Within this binary a
//! file-local mutex serializes them; across binaries the guarantee is the measured
//! one recorded on `itest::RIG_CLAIM` — cargo runs test binaries strictly
//! sequentially. A third claimant now exists; that doc names all three.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{
    Daemon, TempRun, blessed_replug_helper, skip_no_replug, usb_port_of, wait_until,
};

/// Serializes the adapters across the tests in this binary. Poison-recovering for
/// the reason `serial_hardware.rs` gives: one panicking replug test must not
/// cascade the rest into poison-panics that hide the original failure.
static RIG: Mutex<()> = Mutex::new(());

fn rig_guard() -> MutexGuard<'static, ()> {
    RIG.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Everything a replug test needs, or the reason it cannot run.
struct Rig {
    helper: PathBuf,
    by_id: PathBuf,
    port: String,
    identity: String,
}

/// Resolve the rig from the environment, or explain what is missing.
///
/// Deliberately strict about `SNX_REPLUG_DEV` in the same way `serial_pair_or_rig`
/// is strict about a forced rig: a variable that names something unusable is a
/// **hard failure**, never a silent skip, because an operator instruction that
/// quietly does nothing is the defect §3.35 exists to kill.
fn rig() -> Result<Rig, String> {
    let helper = blessed_replug_helper()?;
    let Ok(dev) = std::env::var("SNX_REPLUG_DEV") else {
        return Err("SNX_REPLUG_DEV is not set (name a /dev/serial/by-id/... link)".to_owned());
    };
    let by_id = PathBuf::from(&dev);
    assert!(
        dev.starts_with("/dev/serial/by-id/"),
        "SNX_REPLUG_DEV={dev} is not a /dev/serial/by-id path. Re-enumeration can \
         renumber ttyUSBn, so this test refuses a /dev/ttyUSB* argument: it names a \
         node this very test may invalidate."
    );
    assert!(
        by_id.exists(),
        "SNX_REPLUG_DEV={dev} does not exist. Visible now: {:?}",
        by_id_links()
    );
    let port = usb_port_of(&by_id)
        .unwrap_or_else(|| panic!("SNX_REPLUG_DEV={dev} resolves to no sysfs USB port"));
    let identity = identity_of(&port)
        .unwrap_or_else(|| panic!("port {port} publishes no usb:vid:pid:serial:iface identity"));
    Ok(Rig {
        helper,
        by_id,
        port,
        identity,
    })
}

fn by_id_links() -> Vec<String> {
    std::fs::read_dir("/dev/serial/by-id")
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn sysfs_attr(port: &str, name: &str) -> Option<String> {
    std::fs::read_to_string(format!("/sys/bus/usb/devices/{port}/{name}"))
        .ok()
        .map(|s| s.trim().to_owned())
}

/// The canonical identity the daemon's own resolver would mint for this device.
fn identity_of(port: &str) -> Option<String> {
    Some(format!(
        "usb:{}:{}:{}:00",
        sysfs_attr(port, "idVendor")?,
        sysfs_attr(port, "idProduct")?,
        sysfs_attr(port, "serial")?
    ))
}

/// `devnum` — the kernel's enumeration counter for this device.
///
/// **The discriminator this whole file turns on.** A driver unbind/bind leaves it
/// alone; a real disconnect and re-enumeration always changes it. Compared for
/// inequality only, never ordering: it wraps at 127 per bus.
fn devnum(port: &str) -> Option<String> {
    sysfs_attr(port, "devnum")
}

/// Run the blessed helper for a one-shot verb (`status`, `authorize`), returning
/// its JSON report.
fn replug(helper: &Path, args: &[&str]) -> Value {
    let out = std::process::Command::new(helper)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("running {} {args:?}: {e}", helper.display()));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{} {args:?} exited {:?}\nstdout: {stdout}\nstderr: {}",
        helper.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("helper printed unparseable JSON ({e}): {stdout:?}"))
}

/// Hold `port` deauthorized for as long as `while_down` runs, then bring it back.
///
/// **This is the shape that keeps the blessed binary still.** The helper's entire
/// contribution is the two writes; the hold length, the sampling and the whole
/// experiment live in `while_down`, here, unprivileged — so iterating on what a
/// replug test measures costs a rebuild of the test and never a `sudo`. The device
/// comes back when this function drops the child's stdin, and also if the test
/// process dies at any point, because pipe EOF fires either way (§15.43's leash,
/// same mechanism and same reason).
///
/// Returns the helper's `down` and `up` reports and whatever `while_down` produced.
fn while_deauthorized<T>(
    helper: &Path,
    port: &str,
    dry_run: bool,
    while_down: impl FnOnce() -> T,
) -> (Value, Value, T) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let mut args = vec!["hold", "--port", port, "--max-ms", "20000"];
    if dry_run {
        args.push("--dry-run");
    }
    let mut child = Command::new(helper)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawning {} {args:?}: {e}", helper.display()));

    let mut out = BufReader::new(child.stdout.take().expect("helper stdout is piped"));
    let mut line = String::new();
    out.read_line(&mut line)
        .expect("read the helper's down line");
    let down: Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| {
        let _ = child.kill();
        panic!("helper's first line was not JSON ({e}): {line:?}")
    });
    assert_eq!(
        down["state"], "down",
        "helper did not report the device down: {down}"
    );

    let observed = while_down();

    // EOF: the device comes back. Dropping stdin is the whole protocol.
    drop(child.stdin.take());
    line.clear();
    out.read_line(&mut line).expect("read the helper's up line");
    let up: Value = serde_json::from_str(line.trim())
        .unwrap_or_else(|e| panic!("helper's second line was not JSON ({e}): {line:?}"));
    let status = child.wait().expect("wait for the helper");
    assert!(status.success(), "helper exited {status:?}: up report {up}");
    assert_eq!(
        up["state"], "up",
        "helper did not report the device up: {up}"
    );
    (down, up, observed)
}

/// Whether the port currently owns any tty, read straight from sysfs.
///
/// Unprivileged, which is the point: the discriminator belongs to the test.
fn has_tty(port: &str) -> bool {
    std::fs::read_dir(format!("/sys/bus/usb/devices/{port}"))
        .map(|entries| {
            entries.flatten().any(|iface| {
                iface.file_name().to_string_lossy().contains(':')
                    && std::fs::read_dir(iface.path()).is_ok_and(|kids| {
                        kids.flatten()
                            .any(|k| k.file_name().to_string_lossy().starts_with("ttyUSB"))
                    })
            })
        })
        .unwrap_or(false)
}

/// A daemon holding one free-for-all serial node addressed **by identity**, cross-wired
/// to a console pty — the `p7_replug.rs` shape against the real `/dev` and `/sys`.
fn config(identity: &str, console: &Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{identity}"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con"
path = "{console}"
[[edge]]
a = "usb0"
b = "con"
"#,
        console = console.display(),
    )
}

/// The property, end to end: a real re-enumeration, and a node that comes back at
/// the same identity on whatever path the kernel chose.
#[test]
fn a_real_usb_reenumeration_heals_the_node_at_its_canonical_identity() {
    let _claim = rig_guard();
    let rig = match rig() {
        Ok(r) => r,
        Err(why) => {
            skip_no_replug(
                "a_real_usb_reenumeration_heals_the_node_at_its_canonical_identity",
                &why,
            );
            return;
        }
    };

    // Self-repair: a previous run that died mid-cycle would have left the adapter
    // deauthorized, and the next failure would be attributed to whatever ran next.
    // Say so loudly rather than silently fixing it.
    let status = replug(&rig.helper, &["status", "--port", &rig.port, "--json"]);
    if status["authorized"] == Value::Bool(false) {
        eprintln!(
            "NOTE: port {} was left deauthorized by an earlier run; repairing",
            rig.port
        );
        replug(&rig.helper, &["authorize", "--port", &rig.port, "--json"]);
    }

    let run = TempRun::new();
    let console = run.join("con");
    let daemon = Daemon::start();
    let rpc = daemon.rpc();
    rpc.load_toml(&config(&rig.identity, &console), false)
        .expect("load the replug config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "serial node never came up on the real adapter: {:?}",
        rpc.node("usb0")
    );

    let devnum_before = devnum(&rig.port).expect("devnum before");
    let node_before = rpc
        .node("usb0")
        .expect("node usb0 present before the replug");
    let path_before = node_before["resolved_path"].clone();
    assert!(
        path_before.is_string(),
        "no resolved_path before the replug: {node_before}"
    );

    // The replug itself. The helper holds the device down only while the closure
    // below runs, and everything measured in it is measured here, unprivileged.
    let (down, up, (saw_tty_go, daemon_noticed, statuses)) =
        while_deauthorized(&rig.helper, &rig.port, false, || {
            // (D) The discriminator, sampled by the test: the tty really went away.
            // `devnum` was the pre-registered discriminator and is refuted — it
            // does not move across an `authorized` cycle on this kernel (module
            // docs, notes §3.54) — so what is asserted is the disappearance, which
            // is the event the daemon under test actually experiences.
            let tty_gone = wait_until(Duration::from_secs(5), || !has_tty(&rig.port));

            // (A) **And the daemon has to notice.** Without this the test is
            // vacuous in the most ordinary way: the helper can take the device
            // down and put it back inside a millisecond, the node never leaves
            // `active`, and asserting that it is `active` afterwards proves
            // nothing at all. Measured budget: the reader notices loss within
            // ~200 ms (`READ_POLL_TIMEOUT_MS`) and rediscovery polls at ~1 s
            // (`RECONNECT_POLL`), so the hold lasts exactly as long as it takes —
            // which is the reason the hold length is decided here rather than
            // baked into the blessed binary.
            let mut seen: Vec<String> = Vec::new();
            let noticed = wait_until(Duration::from_secs(10), || {
                let status = rpc.node_status("usb0");
                if !seen.contains(&status) {
                    seen.push(status.clone());
                }
                status != "active"
            });
            (tty_gone, noticed, seen)
        });
    assert!(
        saw_tty_go,
        "the tty never disappeared while the device was deauthorized, so nothing \
         below measures a replug. down={down} up={up}"
    );
    assert!(
        daemon_noticed,
        "the daemon never left `active` during a real outage, so `active` afterwards \
         proves nothing — the node was never observed to fault. Statuses seen: \
         {statuses:?}. down={down} up={up}"
    );
    assert!(
        up["reauthorize_failures"]
            .as_array()
            .is_some_and(|f| f.is_empty()),
        "the helper failed to reauthorize: {up}"
    );
    assert_eq!(
        up["released_by"], "stdin-eof",
        "the hold ended for a reason other than the test finishing: {up}"
    );

    // Recorded, never asserted: `devnum` is stable across this operation on Linux
    // 7.0.0-29 (the refuted pre-registered discriminator — see the module docs).
    let devnum_after = devnum(&rig.port).expect("devnum after");

    // The by-id link must come back before the daemon can possibly resolve it. Wait
    // on the link, never sleep for it: `metadata` follows the link, so a dangling
    // one reads false.
    assert!(
        wait_until(Duration::from_secs(10), || std::fs::metadata(&rig.by_id)
            .is_ok()),
        "the by-id link {} never returned after re-enumeration",
        rig.by_id.display()
    );

    // (B) Healed, and actually open on a real device node.
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(30)),
        "node never healed after a real replug: {:?}",
        rpc.node("usb0")
    );
    let healed = rpc.node("usb0").expect("node usb0 present after the heal");
    assert_eq!(
        healed["open"],
        Value::Bool(true),
        "healed but not open — `active` alone is the proxy the fixture test already \
         proves; §12 promises a resolved path at every open: {healed}"
    );
    let resolved = healed["resolved_path"]
        .as_str()
        .unwrap_or_else(|| panic!("no resolved_path after heal: {healed}"));
    assert!(
        Path::new(resolved).exists(),
        "resolved_path {resolved} does not exist after heal: {healed}"
    );

    // (C) The literal P4 sentence: the identity did not move.
    assert_eq!(
        healed["identity"].as_str(),
        Some(rig.identity.as_str()),
        "identity changed across a replug: {healed}"
    );
    assert_eq!(
        identity_of(&rig.port).as_deref(),
        Some(rig.identity.as_str()),
        "the device came back with a different canonical identity"
    );

    // (E) Recorded, never asserted either way: Linux reuses the lowest free minor,
    // so an unchanged ttyUSBn here is a legitimate outcome. The property is that
    // the daemon does not care, which is B and C above.
    eprintln!(
        "replug measured: devnum {devnum_before} -> {devnum_after} (stable by design on \
         this kernel); path {path_before} -> {resolved:?}; statuses seen while down \
         {statuses:?}; held {} ms, released by {}",
        up["held_ms"], up["released_by"]
    );
}

/// The positive control, and the reason it runs in the suite rather than being
/// asserted in prose: under `--dry-run` the helper performs every check and every
/// wait but neither write, so a test body that still "passes" is not measuring the
/// replug. The discriminator must go quiet exactly when the writes do.
///
/// This is §3.50's pattern — force the mechanism to zero and watch the guard red —
/// executed rather than predicted.
#[test]
fn the_replug_discriminator_goes_quiet_when_no_write_happens() {
    let _claim = rig_guard();
    let rig = match rig() {
        Ok(r) => r,
        Err(why) => {
            skip_no_replug(
                "the_replug_discriminator_goes_quiet_when_no_write_happens",
                &why,
            );
            return;
        }
    };

    let (down, up, saw_tty_go) = while_deauthorized(&rig.helper, &rig.port, true, || {
        wait_until(Duration::from_secs(2), || !has_tty(&rig.port))
    });
    assert_eq!(
        down["dry_run"],
        Value::Bool(true),
        "the control did not run as a dry run: {down}"
    );
    // The whole point: the discriminator the real test asserts on must stay quiet
    // here. If a dry run can make it fire, it is not measuring the write.
    assert!(
        !saw_tty_go,
        "the tty vanished during a --dry-run hold: the helper wrote when it \
         promised not to, so the control is not one. down={down} up={up}"
    );

    // And the adapter is still usable — a control that damages the rig is not one.
    assert_eq!(
        replug(&rig.helper, &["status", "--port", &rig.port, "--json"])["authorized"],
        Value::Bool(true),
        "the dry run left the device deauthorized"
    );
}

/// The capability is not ambient: an ordinary test process cannot perform the write
/// itself, which is the whole reason the helper exists as a separate blessed binary.
///
/// Runs everywhere, needs no rig and no blessing — it asserts an absence.
#[test]
fn the_test_process_itself_holds_no_capability_to_write_sysfs() {
    let target = Path::new("/sys/bus/usb/devices");
    if !target.is_dir() {
        eprintln!("SKIP the_test_process_itself_holds_no_capability_to_write_sysfs: no sysfs USB");
        return;
    }
    // Pick any USB device and try to write its `authorized` directly. This must
    // fail: if it ever succeeds, the suite is running privileged and every result
    // in this file is about a root daemon rather than the shipped one.
    let Some(any) = std::fs::read_dir(target)
        .expect("read sysfs usb devices")
        .flatten()
        .map(|e| e.path().join("authorized"))
        .find(|p| p.exists())
    else {
        eprintln!("SKIP the_test_process_itself_holds_no_capability_to_write_sysfs: no device");
        return;
    };
    let err = std::fs::write(&any, "1").expect_err(
        "this test process could write a root-owned sysfs attribute — the suite is \
         running with privilege it must not have, and every replug result here would \
         be measuring a privileged daemon",
    );
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied,
        "unexpected error writing {}: {err}",
        any.display()
    );
}
