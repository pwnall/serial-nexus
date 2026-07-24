//! Phase 7 replug slice, ported from `scripts/validate/phase7/replug.sh`
//! (design §7.1 faulted-and-wait / reopen ritual, §12 device identity, §5 never-drop
//! targetward). One property, proven end to end:
//!
//! After an **unplug**, targetward writes buffer during the outage (parked in the
//! serial's bounded channel, backpressured, never dropped, §5). Recreating the device
//! at the **same identity** (§12) heals the node `waiting -> active`; the reopen ritual
//! reapplies raw termios + `TIOCEXCL` + modem lines (§7.1); and purge-on-reconnect
//! discards the outage-era backlog (`purged_on_reconnect > 0`) so stale commands never
//! fire into the booting device. A fresh echo round-trip being byte-clean proves the
//! raw termios was reapplied — a non-raw reopen would corrupt it via echo/newline
//! translation, and any surviving stale byte would leak into the stream.
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * The bash's `stat`/`jq`/`nc`/`cargo build` scaffolding is replaced by structured
//!   RPC over the control socket and `nexus-sim` verdicts (the harness doctrine, §5).
//! * `serialnexusctl … | jq` state polling becomes `Rpc::wait_status` / `Rpc::node`
//!   on the structured `state` snapshot; the echo checks assert on the sim `client`
//!   verdict (`pass` + byte-exact `received`), never on free text.
//! * The `scripts/lib/fixture-tree.sh` `make_usb_iface`/`unplug_usb` shell helpers are
//!   reimplemented in Rust (`make_fixture`/`unplug`) — the same by-id + sysfs symlink
//!   tree the resolver walks under `--dev-root` (§12), built unprivileged.
//! * The daemon is hand-managed via `Command` (not `Daemon::start`) so it can carry
//!   `--dev-root <fixture>`; the device is a `nexus-sim pty --echo` double at the
//!   fixture's `/dev` path, killed and respawned to model the unplug/replug.
//!
//! Needs a sim-pty serial device (the Linux software-loopback mechanism), so it skips
//! where that is unavailable (macOS: a pty cannot be a `serial2` device — `ENOTTY`).

use std::os::unix::fs::symlink;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Rpc, Sim, TempRun, bin, serial_echo, wait_until};
use serde_json::Value;

/// The canonical usb identity the serial node is configured for (§12). The fixture
/// tree below is what makes this resolve to `<dev-root>/dev/ttyUSB0`.
const IDENTITY: &str = "usb:0403:6001:REPLUG1:00";
const DEVNAME: &str = "ttyUSB0";
const BYID: &str = "usb-FTDI_REPLUG1-if00";
const USBDIR: &str = "1-1";
const IFACE: &str = "00";

/// A daemon child SIGKILLed and reaped on drop, so a panicking test never leaks it.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serialnexusd` on `run`'s socket + state file, rooted at the fixture
/// `dev_root` (§12: `sys_root` is `<dev-root>/sys`) — the seam that lets the resolver
/// walk fixture by-id/sysfs trees unprivileged.
fn spawn_daemon(run: &TempRun, dev_root: &Path) -> Child {
    Command::new(bin("serialnexusd"))
        .arg("--socket")
        .arg(run.socket())
        .arg("--state-file")
        .arg(run.state_file())
        .arg("--dev-root")
        .arg(dev_root)
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serialnexusd")
}

/// Wait until a daemon is actually accepting on `sock` (bounded poll, no bare sleep).
fn wait_socket(sock: &Path) -> bool {
    wait_until(Duration::from_secs(10), || {
        UnixStream::connect(sock).is_ok()
    })
}

/// Replace `link` with a fresh symlink to `target` — the `ln -sfn` the shell fixture
/// used, so the fixture builder is idempotent and doubles as the replug.
fn relink(target: &str, link: &Path) {
    let _ = std::fs::remove_file(link);
    symlink(target, link).unwrap_or_else(|e| panic!("symlink {}: {e}", link.display()));
}

/// Build the by-id + sysfs fixture for one USB tty device (the Rust port of
/// `fixture-tree.sh::make_usb_iface`, §12). Idempotent: symlinks are replaced, so it
/// doubles as the "replug" that restores the identity the unplug removed.
fn make_fixture(root: &Path) {
    let dev = root.join("sys/bus/usb/devices").join(USBDIR);
    std::fs::create_dir_all(&dev).expect("mkdir sysfs usb device");
    std::fs::write(dev.join("idVendor"), "0403").unwrap();
    std::fs::write(dev.join("idProduct"), "6001").unwrap();
    std::fs::write(dev.join("serial"), "REPLUG1").unwrap();
    std::fs::write(dev.join("manufacturer"), "FTDI-ish").unwrap();
    std::fs::write(dev.join("product"), "Fixture Serial").unwrap();

    // The interface dir carries bInterfaceNumber; the class/tty link points at it, and
    // the resolver walks up from there to the idVendor-bearing device.
    let ifdir = dev.join(format!("{USBDIR}:1.{IFACE}"));
    std::fs::create_dir_all(&ifdir).expect("mkdir sysfs interface");
    std::fs::write(ifdir.join("bInterfaceNumber"), IFACE).unwrap();

    let class = root.join("sys/class/tty").join(DEVNAME);
    std::fs::create_dir_all(&class).expect("mkdir class/tty");
    relink(
        &format!("../../../bus/usb/devices/{USBDIR}/{USBDIR}:1.{IFACE}"),
        &class.join("device"),
    );

    let by_id_dir = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id_dir).expect("mkdir by-id");
    relink(&format!("../../{DEVNAME}"), &by_id_dir.join(BYID));
}

/// Remove the by-id entry and sysfs class link so the resolver sees the device as
/// absent (the Rust port of `fixture-tree.sh::unplug_usb`, §12).
fn unplug(root: &Path) {
    let _ = std::fs::remove_file(root.join("dev/serial/by-id").join(BYID));
    let _ = std::fs::remove_dir_all(root.join("sys/class/tty").join(DEVNAME));
}

/// Spawn the echoing serial device (a `nexus-sim pty --echo`) at the fixture `/dev`
/// path, waiting for its link to go live. Dropping the returned `Sim` SIGKILLs it —
/// the unplug — leaving the `/dev/ttyUSB0` symlink dangling as a real one would.
fn spawn_device(dev_node: &Path) -> Sim {
    Sim::spawn(
        &[
            "pty",
            "--echo",
            "--link",
            &dev_node.to_string_lossy(),
            "--timeout-ms",
            "600000",
        ],
        Some(dev_node),
    )
}

/// Drive one seeded batch through the console pty and verify the echo round-trip: the
/// verdict's `pass` and byte-exact `received` are ground truth (§5). Byte-cleanliness
/// here is what proves the raw termios survived the reopen (and no stale byte leaked).
fn echo(console: &Path, spec: &str, seed: u64) -> Value {
    Sim::client(&[
        "--path",
        &console.to_string_lossy(),
        "--send",
        spec,
        "--expect",
        "echo",
        "--seed",
        &seed.to_string(),
        "--timeout-ms",
        "8000",
    ])
}

#[test]
fn replug_heals_and_reapplies_the_open_ritual() {
    // Capability gate: this test needs a sim-pty serial device resolvable through a
    // fixture dev-root tree — the same Linux-only mechanism `serial_echo` provides (a
    // pty cannot be a `serial2` device on macOS). We build our own device at the
    // dev-root path below, so the probe is only the platform gate; drop it at once.
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP replug_heals_and_reapplies_the_open_ritual: no sim-pty serial device on this platform"
        );
        return;
    };
    drop(probe);

    let run = TempRun::new();
    let root = run.join("root");
    std::fs::create_dir_all(root.join("dev")).expect("mkdir dev-root /dev");
    let dev_node = root.join("dev").join(DEVNAME);
    let console = run.join("con");

    // First plug: the fixture identity + a live echo device, then the daemon.
    make_fixture(&root);
    let device = spawn_device(&dev_node);

    let _daemon = KillOnDrop(spawn_daemon(&run, &root));
    assert!(
        wait_socket(&run.socket()),
        "daemon control socket never appeared"
    );
    let rpc = Rpc::new(run.socket());

    // A free-for-all serial (so the console's targetward writes flow without a lock)
    // cross-wired to an interactive console pty (§7.1/§7.2).
    let cfg = format!(
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
        identity = IDENTITY,
        console = console.display(),
    );
    rpc.load_toml(&cfg, false).expect("load replug config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(15)),
        "serial never active on first plug: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("con", "active", Duration::from_secs(10)),
        "console pty never active: {:?}",
        rpc.node("con")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // Baseline echo: device present, data plane healthy.
    let base = echo(&console, "seeded:1KiB", 1);
    assert_eq!(
        base["pass"].as_bool(),
        Some(true),
        "baseline echo failed: {base}"
    );
    assert_eq!(
        base["received"].as_u64(),
        Some(1024),
        "baseline echo short: {base}"
    );

    // ---- Unplug -------------------------------------------------------------------
    // Kill the device (its /dev symlink dangles) then drop its identity from the
    // fixture, so the resolver sees it absent. The reader's hangup faults the node to
    // waiting (§7.1 faulted-and-wait). Bash order: kill the sim, then remove fixture.
    drop(device);
    unplug(&root);
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "serial did not fault-and-wait on unplug: {:?}",
        rpc.node("usb0")
    );

    // Buffer stale targetward commands during the outage: they park in the serial's
    // bounded channel (backpressured, never dropped, §5). Errors are tolerated exactly
    // as the bash's `|| true` — the assertion is the purge counter, below.
    for i in 1..=8 {
        let _ = rpc.send("usb0", &format!("STALE-COMMAND-{i}"), false, 5000);
    }

    // ---- Replug at the SAME identity ----------------------------------------------
    make_fixture(&root);
    let device = spawn_device(&dev_node);
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(15)),
        "serial did not heal on replug: {:?}",
        rpc.node("usb0")
    );

    // purge-on-reconnect discarded the outage backlog (the stale commands never fired).
    let purged = rpc.node("usb0").expect("usb0 node")["purged_on_reconnect"]
        .as_u64()
        .expect("purged_on_reconnect present");
    assert!(
        purged > 0,
        "purge-on-reconnect counter not set (got {purged}); stale commands may have fired"
    );

    // The reopen ritual reapplied raw termios: a fresh echo round-trips byte-clean (a
    // non-raw reopen would echo/translate and corrupt it), and no stale byte leaks.
    let heal = echo(&console, "seeded:2KiB", 9);
    assert_eq!(
        heal["pass"].as_bool(),
        Some(true),
        "post-replug echo not byte-clean (termios not reapplied, or stale leak): {heal}"
    );
    assert_eq!(
        heal["received"].as_u64(),
        Some(2048),
        "post-replug echo short: {heal}"
    );

    // Keep the healed device alive until the assertions are done.
    drop(device);
}
