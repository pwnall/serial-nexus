#![forbid(unsafe_code)]

//! Phase 7 unplug slice, ported from `scripts/validate/phase7/unplug.sh`
//! (plan §Phase 7 item 1; design §7.1 faulted-and-wait, §12 device identity).
//!
//! A serial node addressed by a usb identity (`usb:0403:6001:UNPLUG1:00`),
//! resolved through a fixture `/dev/serial/by-id` + sysfs tree under `--dev-root`
//! (§12), fans out to a PTY with an attached client. Killing the serial device
//! and removing its fixture symlink faults-and-waits the serial node (§7.1)
//! *within the poll interval*, while the PTY's client stays attached — the unplug
//! of one boundary never disturbs another (no HUP propagated to the consumer).
//!
//! Preserved assertions (identical to the bash, pinned to structured RPC state,
//! never CLI text — §5):
//! 1. the serial node resolves its usb identity through the fixture and comes up
//!    `active`;
//! 2. a client attaching to the PTY makes `client_present == true`;
//! 3. after the unplug the serial node reaches `waiting`;
//! 4. the PTY's client is undisturbed (`client_present` stays `true`).
//!
//! Deviations from the bash, and why (each preserves the original assertions):
//! * The bash `fixture-tree.sh` helpers (`make_usb_iface`/`unplug_usb`) are
//!   reimplemented here with `std::os::unix::fs::symlink` + `std::fs` — the same
//!   by-id/sysfs symlink tree the resolver's own unit tests build (`add_usb_device`
//!   in `core/src/resolver.rs`), so the `usb:` identity walk (§12) is
//!   exercised unprivileged with no `ln`/`mkdir` shelling.
//! * The device node the bash backs with `serial-nexus-sim pty --echo --link .../ttyUSB0`
//!   is here the sanctioned [`serial_echo`] pts, symlinked into the fixture at
//!   `<dev-root>/dev/ttyUSB0`; `open(2)` follows the chain to the pts. Dropping the
//!   [`SerialEcho`] kills its backing sim — the "kill the device" half of the
//!   unplug — closing the master so the serial reader HUPs (§7.1).
//! * Needs a software serial device (a sim pts), so the test is Linux-only and
//!   SKIPs on macOS (`serial_echo()` -> `None`), per the harness serial doctrine.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{RawDaemon, Rpc, Sim, TempRun, serial_echo, wait_until};

/// Build the fixture for one USB tty interface: the sysfs `idVendor`/`serial`/…
/// tree, the class/tty `device` link, and the `by-id` entry (§12). Faithful to
/// `fixture-tree.sh:make_usb_iface`; `serial` may be empty for a no-serial clone.
#[allow(clippy::too_many_arguments)]
fn make_usb_iface(
    root: &Path,
    usbdir: &str,
    vid: &str,
    pid: &str,
    serial: &str,
    devname: &str,
    iface: &str,
    byid: &str,
) {
    let dev = root.join("sys/bus/usb/devices").join(usbdir);
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::write(dev.join("idVendor"), vid).unwrap();
    std::fs::write(dev.join("idProduct"), pid).unwrap();
    if !serial.is_empty() {
        std::fs::write(dev.join("serial"), serial).unwrap();
    }
    std::fs::write(dev.join("manufacturer"), "FTDI-ish").unwrap();
    std::fs::write(dev.join("product"), "Fixture Serial").unwrap();
    let ifdir = dev.join(format!("{usbdir}:1.{iface}"));
    std::fs::create_dir_all(&ifdir).unwrap();
    std::fs::write(ifdir.join("bInterfaceNumber"), iface).unwrap();
    // class/tty/<devname>/device -> the interface dir (relative, stays in-tree).
    let class = root.join("sys/class/tty").join(devname);
    std::fs::create_dir_all(&class).unwrap();
    std::os::unix::fs::symlink(
        format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.{iface}"),
        class.join("device"),
    )
    .unwrap();
    // by-id/<byid> -> ../../<devname>
    let by_id = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id).unwrap();
    std::os::unix::fs::symlink(format!("../../{devname}"), by_id.join(byid)).unwrap();
}

/// Remove the by-id entry and sysfs device so the resolver sees the device as
/// absent — an unplug (§12). Faithful to `fixture-tree.sh:unplug_usb`.
fn unplug_usb(root: &Path, devname: &str, byid: &str) {
    let _ = std::fs::remove_file(root.join("dev/serial/by-id").join(byid));
    let _ = std::fs::remove_dir_all(root.join("sys/class/tty").join(devname));
}

/// A PTY node's `client_present` observed state (§7.2), `false` if the node is
/// absent or the field is missing.
fn client_present(rpc: &Rpc, node: &str) -> bool {
    rpc.node(node)
        .and_then(|n| n.get("client_present").and_then(Value::as_bool))
        .unwrap_or(false)
}

#[test]
fn unplug_faults_serial_and_leaves_pty_client_attached() {
    // The device is a sim pts; skip where none exists (macOS), per the serial
    // doctrine — the codec/pty/control paths run everywhere, but this one needs a
    // real serial-capable fd (§12 identity resolution over a live device).
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP unplug_faults_serial_and_leaves_pty_client_attached: \
             no software serial device on this platform"
        );
        return;
    };
    let device = echo.device().to_path_buf();

    // Fixture: an FTDI-like device at usb:0403:6001:UNPLUG1:00 behind ttyUSB0.
    let run = TempRun::new();
    let root = run.join("root");
    make_usb_iface(
        &root,
        "1-1",
        "0403",
        "6001",
        "UNPLUG1",
        "ttyUSB0",
        "00",
        "usb-FTDI_UNPLUG1-if00",
    );
    // The device node the resolver hands the daemon (`<dev-root>/dev/ttyUSB0`),
    // pointed at the echo pts; open(2) follows the chain to the pts.
    let dev_node = root.join("dev/ttyUSB0");
    std::fs::create_dir_all(dev_node.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&device, &dev_node).unwrap();

    let _daemon = RawDaemon::builder(&run)
        .args(&["--dev-root", &root.to_string_lossy()])
        .start();
    let rpc = Rpc::new(run.socket());

    let con = run.join("con");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "usb:0403:6001:UNPLUG1:00"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con"
path = "{con}"
[[edge]]
a = "usb0"
b = "con"
"#,
        con = con.display(),
    );
    rpc.load_toml(&cfg, false).expect("load unplug config");

    // (1) The serial node resolves the usb identity through the fixture and comes
    // up active.
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "serial never became active (identity did not resolve through the fixture): {:?}",
        rpc.node("usb0")
    );

    // The PTY must be up (slave openable, symlink published) before the client
    // races to open it, or the attach fails spuriously.
    assert!(
        rpc.wait_status("con", "active", Duration::from_secs(10)),
        "pty con never became active: {:?}",
        rpc.node("con")
    );
    assert!(
        wait_until(Duration::from_secs(5), || con.exists()),
        "pty symlink never appeared at {}",
        con.display()
    );

    // (2) Attach a client to the PTY and confirm presence (the slave held open).
    let con_str = con.to_string_lossy().into_owned();
    let _client = Sim::spawn(
        &[
            "client",
            "--path",
            &con_str,
            "--set-baud",
            "115200",
            // Caller-owned hold (plan §18 item 25): the client holds the slave for as
            // long as this test does, not for a timer's worth.
            "--hold-stdin-eof",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(10), || client_present(&rpc, "con")),
        "PTY client never attached: {:?}",
        rpc.node("con")
    );

    // ---- Unplug: kill the device sim and remove its fixture entry ------------
    // Dropping the echo kills its backing sim, closing the pts master so the
    // serial reader HUPs; removing the fixture makes the identity unresolvable, so
    // the reconnect poll cannot heal it (§7.1/§12).
    drop(echo);
    unplug_usb(&root, "ttyUSB0", "usb-FTDI_UNPLUG1-if00");

    // (3) The serial node reaches `waiting` within the poll interval (§7.1).
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "serial did not reach waiting after unplug: {:?}",
        rpc.node("usb0")
    );

    // (4) The PTY's client is undisturbed (fd still open, no HUP propagated).
    assert!(
        client_present(&rpc, "con"),
        "PTY client HUP'd by an unrelated serial unplug (should be undisturbed): {:?}",
        rpc.node("con")
    );
}
