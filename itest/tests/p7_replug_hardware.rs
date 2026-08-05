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
//! the fixture cannot: that the **devnum changes** — the discriminator that
//! separates a real re-enumeration from a driver rebind — while the **identity does
//! not**, and that the daemon comes back open on whatever `/dev` path the kernel
//! chose this time.
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

/// Run the blessed helper, returning its JSON report. Panics with the helper's own
/// diagnostics on failure — they name the port and the repair verb.
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
    let path_before = node_before["state_extra"]["resolved_path"].clone();
    assert!(
        path_before.is_string(),
        "no resolved_path before the replug: {node_before}"
    );

    // The replug itself. One invocation, one process: the device is never left
    // deauthorized by a test that dies, because the helper owns both writes.
    let report = replug(
        &rig.helper,
        &["cycle", "--port", &rig.port, "--hold-ms", "1500", "--json"],
    );
    assert_eq!(
        report["hold_cut_short_by_signal"],
        Value::Bool(false),
        "the hold was interrupted; the measurement is not the one intended: {report}"
    );

    // (D) The discriminator: the kernel really re-enumerated. Inequality only.
    let devnum_after = devnum(&rig.port).expect("devnum after");
    assert_ne!(
        devnum_before, devnum_after,
        "devnum did not change across the cycle ({devnum_before} -> {devnum_after}) — \
         this was not a re-enumeration, so nothing below measures a replug. \
         Helper report: {report}"
    );

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
    let extra = &healed["state_extra"];
    assert_eq!(
        extra["open"],
        Value::Bool(true),
        "healed but not open — `active` alone is the proxy the fixture test already \
         proves; §12 promises a resolved path at every open: {healed}"
    );
    let resolved = extra["resolved_path"]
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
        "replug measured: devnum {devnum_before} -> {devnum_after}; path {path_before} -> \
         {resolved:?}; tty wait {} ms",
        report["waits"][0]["tty_wait_ms"]
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

    let before = devnum(&rig.port).expect("devnum before");
    let report = replug(
        &rig.helper,
        &[
            "cycle",
            "--port",
            &rig.port,
            "--hold-ms",
            "250",
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(
        report["dry_run"],
        Value::Bool(true),
        "the control did not run as a dry run: {report}"
    );
    let after = devnum(&rig.port).expect("devnum after");
    assert_eq!(
        before, after,
        "devnum moved during a --dry-run cycle ({before} -> {after}): the helper \
         wrote when it promised not to, so --dry-run is not a control"
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
