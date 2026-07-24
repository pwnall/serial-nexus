//! Phase 7 squatter slice, ported from `scripts/validate/phase7/squatter.sh`
//! (plan §Phase 7 item 3; design §7.1 faulted-and-wait, §12 squatter-safe device
//! identity). One property, proven end to end:
//!
//! After an **unplug**, a **different** adapter appearing on the same `/dev` path (a
//! different usb identity) must **not** be adopted. The resolver returns a path only
//! for the *same* identity (§12 `resolve_current_path` → `find_usb`), so the node
//! stays `waiting`, never opens the squatter, and the squatter receives **zero**
//! bytes. Wrong-device adoption is impossible by construction.
//!
//! Preserved assertions (pinned to structured RPC state + a byte-exact sim verdict,
//! never CLI text — §5):
//! 1. the serial node resolves its own usb identity (`usb:0403:6001:OURS:00`) through
//!    the fixture and comes up `active`;
//! 2. after the unplug the node reaches `waiting` (§7.1 faulted-and-wait);
//! 3. with a squatter (`usb:…:SQUATTER:00`) on the same `/dev/ttyUSB0` and several
//!    reconnect-poll cycles elapsed, the node **stays** `waiting` and never resolves
//!    the squatter (`resolved_path` stays `null`, `open == false`) — no wrong-device
//!    adoption;
//! 4. the squatter (a byte-counting `nexus-sim pty --sink`) received **0** bytes,
//!    because the daemon never opened it.
//!
//! Deviations from the bash, and why (each preserves the original *assertions*):
//! * The `scripts/lib/fixture-tree.sh` helpers (`make_usb_iface`/`unplug_usb`) are
//!   reimplemented with `std::os::unix::fs::symlink` + `std::fs` — the same by-id +
//!   sysfs symlink tree the resolver walks under `--dev-root` (§12), built
//!   unprivileged, identical to the `p7_unplug`/`p7_replug` ports.
//! * The daemon is hand-managed via `Command` (not `Daemon::start`) so it can carry
//!   `--dev-root <fixture>`; the "ours" device is a `nexus-sim pty --echo` double at
//!   the fixture's `/dev` path, killed to model the unplug.
//! * The bash's `jq -r '.received'` over the sink's captured stdout becomes a direct
//!   `serde_json` parse of the `nexus-sim pty --sink` verdict (the sim subprocess is
//!   spawned with a piped stdout so the byte count is ground truth, not CLI text).
//!
//! Needs a sim-pty serial device (the Linux software-loopback mechanism), so it skips
//! where that is unavailable (macOS: a pty cannot be a `serial2` device — `ENOTTY`).

use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Rpc, Sim, TempRun, bin, serial_echo, wait_until};
use serde_json::Value;

const USBDIR: &str = "1-1";
const DEVNAME: &str = "ttyUSB0";
const IFACE: &str = "00";

/// Our node's configured identity and its fixture entries (§12).
const OURS_IDENTITY: &str = "usb:0403:6001:OURS:00";
const OURS_SERIAL: &str = "OURS";
const OURS_BYID: &str = "usb-FTDI_OURS-if00";

/// The squatter: a *different* identity (a different usb serial) reusing the same
/// `/dev` name. Its identity (`usb:0403:6001:SQUATTER:00`) never matches ours.
const SQUATTER_SERIAL: &str = "SQUATTER";
const SQUATTER_BYID: &str = "usb-FTDI_SQUATTER-if00";

/// A child SIGKILLed and reaped on drop, so a panicking test never leaks it.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serialnexusd` on `run`'s socket + state file, rooted at the fixture
/// `dev_root` (§12: `sys_root` is `<dev-root>/sys`) — the seam that lets the resolver
/// walk fixture by-id/sysfs trees unprivileged. `Daemon::start` cannot pass
/// `--dev-root`, so the daemon is hand-managed (mirrors `p7_unplug`/`p7_replug`).
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

/// Build the fixture for one USB tty interface: the sysfs `idVendor`/`serial`/… tree,
/// the class/tty `device` link, and the `by-id` entry (§12). Faithful to
/// `fixture-tree.sh:make_usb_iface`. Reusing `usbdir`/`devname` across two identities
/// models a second adapter taking over the same `/dev` name (the squatter).
fn make_usb_iface(root: &Path, usbdir: &str, serial: &str, devname: &str, iface: &str, byid: &str) {
    let dev = root.join("sys/bus/usb/devices").join(usbdir);
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::write(dev.join("idVendor"), "0403").unwrap();
    std::fs::write(dev.join("idProduct"), "6001").unwrap();
    std::fs::write(dev.join("serial"), serial).unwrap();
    std::fs::write(dev.join("manufacturer"), "FTDI-ish").unwrap();
    std::fs::write(dev.join("product"), "Fixture Serial").unwrap();

    let ifdir = dev.join(format!("{usbdir}:1.{iface}"));
    std::fs::create_dir_all(&ifdir).unwrap();
    std::fs::write(ifdir.join("bInterfaceNumber"), iface).unwrap();

    // class/tty/<devname>/device -> the interface dir (relative, stays in-tree).
    let class = root.join("sys/class/tty").join(devname);
    std::fs::create_dir_all(&class).unwrap();
    let device_link = class.join("device");
    let _ = std::fs::remove_file(&device_link);
    std::os::unix::fs::symlink(
        format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.{iface}"),
        &device_link,
    )
    .unwrap();

    // by-id/<byid> -> ../../<devname>
    let by_id = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id).unwrap();
    let by_id_link = by_id.join(byid);
    let _ = std::fs::remove_file(&by_id_link);
    std::os::unix::fs::symlink(format!("../../{devname}"), &by_id_link).unwrap();
}

/// Remove the by-id entry and sysfs class link so the resolver sees the device as
/// absent — an unplug (§12). Faithful to `fixture-tree.sh:unplug_usb`.
fn unplug_usb(root: &Path, devname: &str, byid: &str) {
    let _ = std::fs::remove_file(root.join("dev/serial/by-id").join(byid));
    let _ = std::fs::remove_dir_all(root.join("sys/class/tty").join(devname));
}

/// Spawn the echoing "ours" serial device (a `nexus-sim pty --echo`) at the fixture
/// `/dev` path, waiting for its link to go live. Dropping the returned `Sim` SIGKILLs
/// it — the unplug — closing the master so the serial reader HUPs (§7.1).
fn spawn_ours_device(dev_node: &Path) -> Sim {
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

/// Spawn the squatter as a byte-counting `nexus-sim pty --sink` linked at the same
/// `/dev` path, with a piped stdout so its verdict (`received`) is ground truth. It
/// self-terminates at `--timeout-ms` (4 s here), which also spans several of the
/// daemon's 1 s reconnect-poll cycles — the window in which a buggy daemon would
/// wrongly adopt it.
fn spawn_squatter_sink(dev_node: &Path) -> Child {
    Command::new(bin("nexus-sim"))
        .args([
            "pty",
            "--sink",
            "--bytes",
            "4096",
            "--link",
            &dev_node.to_string_lossy(),
            "--timeout-ms",
            "4000",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn squatter sink")
}

#[test]
fn squatter_on_same_dev_path_is_refused_and_receives_nothing() {
    // Capability gate: needs a sim-pty serial device resolvable through a fixture
    // dev-root tree — Linux-only (a pty cannot be a `serial2` device on macOS,
    // `ENOTTY`). The probe is only the platform gate; we build our own device at the
    // dev-root path below, so drop it at once.
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP squatter_on_same_dev_path_is_refused_and_receives_nothing: \
             no sim-pty serial device on this platform"
        );
        return;
    };
    drop(probe);

    let run = TempRun::new();
    let root = run.join("root");
    std::fs::create_dir_all(root.join("dev")).expect("mkdir dev-root /dev");
    let dev_node = root.join("dev").join(DEVNAME);

    // ---- First plug: ours (usb:…:OURS:00) at ttyUSB0, then the daemon --------------
    make_usb_iface(&root, USBDIR, OURS_SERIAL, DEVNAME, IFACE, OURS_BYID);
    let ours = spawn_ours_device(&dev_node);

    let _daemon = KillOnDrop(spawn_daemon(&run, &root));
    assert!(
        wait_socket(&run.socket()),
        "daemon control socket never appeared"
    );
    let rpc = Rpc::new(run.socket());

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{identity}"
arbitration = "free-for-all"
"#,
        identity = OURS_IDENTITY,
    );
    rpc.load_toml(&cfg, false).expect("load squatter config");

    // (1) The node resolves its own usb identity through the fixture and comes up
    // active.
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(15)),
        "serial never became active on first plug: {:?}",
        rpc.node("usb0")
    );

    // ---- Unplug ours, then a squatter (different identity) takes the /dev path ------
    // Kill the device (the /dev symlink dangles) then drop ours' identity from the
    // fixture, so the resolver sees it absent. The reader's hangup faults the node to
    // waiting (§7.1 faulted-and-wait).
    drop(ours);
    unplug_usb(&root, DEVNAME, OURS_BYID);
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "serial did not fault-and-wait on unplug: {:?}",
        rpc.node("usb0")
    );

    // A DIFFERENT adapter squats the same /dev name (usb:…:SQUATTER:00). It is a byte
    // sink that must stay at zero — the daemon must never open it.
    make_usb_iface(
        &root,
        USBDIR,
        SQUATTER_SERIAL,
        DEVNAME,
        IFACE,
        SQUATTER_BYID,
    );
    let mut squatter = KillOnDrop(spawn_squatter_sink(&dev_node));
    assert!(
        wait_until(Duration::from_secs(5), || dev_node.exists()),
        "squatter device never appeared at {}",
        dev_node.display()
    );

    // Push targetward at the (waiting) node — it must park in the bounded channel,
    // never reach the squatter. Errors are tolerated exactly as the bash's `|| true`;
    // the assertions are the node state and the squatter's byte count, below.
    for i in 1..=5 {
        let _ = rpc.send(
            "usb0",
            &format!("SHOULD-NOT-REACH-SQUATTER-{i}"),
            false,
            1500,
        );
    }

    // Drain the squatter sink's verdict: `read_to_end` blocks until it self-terminates
    // at its 4 s timeout and closes stdout — a bounded wait that also spans several of
    // the daemon's 1 s reconnect-poll cycles, the window a buggy daemon would use to
    // adopt the wrong device.
    let mut out = Vec::new();
    squatter
        .0
        .stdout
        .take()
        .expect("piped squatter stdout")
        .read_to_end(&mut out)
        .expect("read squatter stdout");
    squatter.0.wait().expect("reap squatter sink");

    // (3) The node must NOT have adopted the squatter: still waiting, ours' identity
    // still unresolved (`resolved_path` null), no port open (§12 squatter-safe).
    let node = rpc.node("usb0").expect("usb0 node present");
    assert_eq!(
        node.get("status").and_then(Value::as_str),
        Some("waiting"),
        "node adopted a squatter (wrong-device adoption): {node}"
    );
    assert!(
        node.get("resolved_path")
            .map(Value::is_null)
            .unwrap_or(true),
        "our identity resolved to a foreign device's path (should be null): {node}"
    );
    assert_eq!(
        node.get("open").and_then(Value::as_bool),
        Some(false),
        "node opened a port while waiting for its own identity: {node}"
    );

    // (4) The squatter received nothing — the daemon never opened it.
    let verdict: Value = serde_json::from_slice(&out).unwrap_or_else(|e| {
        panic!(
            "parse squatter sink verdict: {e}; raw={:?}",
            String::from_utf8_lossy(&out)
        )
    });
    let received = verdict.get("received").and_then(Value::as_u64).unwrap_or(0);
    assert_eq!(
        received, 0,
        "squatter received {received} bytes (should be 0 — never opened): {verdict}"
    );
}
