//! Review-32 regression guards for the resolver's two §12 directions (design §12,
//! §15.10, §15.35). One file per defect area, per §5; `p12_*` is this review's
//! family as `p9_*` is review 26's.
//!
//! The cluster these pin is a single structural fault: identity **capture** walked
//! sysfs while identity **resolution**, the duplicate-serial guard and
//! `enumerate_ports` read only `/dev/serial/by-id`. Two directions over two
//! sources disagree, and every finding here is one shape of that disagreement.
//!
//! * `RES-1` — the duplicate-serial guard counted by-id *entries*. udev derives a
//!   by-id name from `ID_SERIAL` + `ID_USB_INTERFACE_NUM` + port, every component
//!   of which is a function of the fields that make an identity ambiguous, so two
//!   clones sharing a serial number collide on ONE name and udev publishes exactly
//!   one symlink. The count could never exceed 1 for the hazard §15.10 exists for:
//!   both clones captured the same `usb:` identity with no warning, both resolved
//!   to whichever one owned the surviving link, and the second adapter became
//!   unreachable by any node. Counting over *devices* fixed capture; the guard was
//!   still absent from the **identity-form** direction — `load`, startup from the
//!   persisted state file, `add-node device = "usb:…"` — which is precisely the
//!   direction an upgrader takes, since a pre-fix daemon persisted the ambiguous
//!   string. Both halves are pinned here.
//! * `RES-2` — a `usb:` identity captured in an environment with `/sys` and no
//!   by-id tree (a container handed `--device=/dev/ttyUSB0`, a busybox-mdev image)
//!   could never be resolved back. `add-node` returned success with a populated
//!   `resolved_path`, and the node then waited forever for a device that was right
//!   there; `ports` answered `[]` in the same tree.
//! * `RES-3` — `add-node` by the canonical `/dev/serial/by-id/usb-…` path took the
//!   *link* name as the device name, missed every lookup, and degraded to `raw:`
//!   carrying the "not stable across reboots" warning — backwards for the one path
//!   whose purpose is reboot stability.
//!
//! Ground truth is structured RPC only (`add-node`/`state`/`ports`/`dump`), never
//! CLI text (§5). Three of the four tests are device-free — the fixture is
//! symlinks, text files and FIFOs, so they run on every platform. The fourth needs
//! an openable sim-pty serial device to prove the node actually *comes up*, which
//! is Linux-only (a pts is not a `serial2` device on macOS, `ENOTTY`), so it
//! self-skips.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, Sim, TempRun, serial_echo};

/// Write `contents` to `p`, creating parents.
fn write(p: &Path, contents: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir -p");
    std::fs::write(p, contents).expect("write fixture file");
}

/// A device node that **cannot be read without blocking**: a FIFO with no writer.
/// Any code that probed it would hang rather than return, so the tests below double
/// as a runtime check that resolution and enumeration stay passive (§15.35).
fn fifo(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir -p");
    let status = std::process::Command::new("mkfifo")
        .arg(p)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo {} failed", p.display());
}

/// The sysfs half of a USB tty device: the USB device dir, its interface dir, and
/// the `class/tty/<dev_name>/device` link the resolver's ancestor walk starts from.
/// Deliberately separate from the `/dev` node and from the udev symlinks, because
/// what these tests vary is exactly *which* of those three exist.
#[allow(clippy::too_many_arguments)]
fn sysfs_device(
    root: &Path,
    usbdir: &str,
    dev_name: &str,
    vid: &str,
    pid: &str,
    serial: Option<&str>,
    iface: &str,
    strings: (&str, &str),
) {
    let usbdev = root.join("sys/bus/usb/devices").join(usbdir);
    write(&usbdev.join("idVendor"), vid);
    write(&usbdev.join("idProduct"), pid);
    if let Some(s) = serial {
        write(&usbdev.join("serial"), s);
    }
    write(&usbdev.join("manufacturer"), strings.0);
    write(&usbdev.join("product"), strings.1);
    write(
        &usbdev.join(format!("{usbdir}:1.0/bInterfaceNumber")),
        iface,
    );
    let class = root.join("sys/class/tty").join(dev_name);
    std::fs::create_dir_all(&class).expect("mkdir class/tty");
    std::os::unix::fs::symlink(
        format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.0"),
        class.join("device"),
    )
    .expect("symlink class/tty device");
}

/// `/dev/serial/by-id/<name> -> ../../<dev_name>`.
fn by_id_link(root: &Path, name: &str, dev_name: &str) {
    let by_id = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id).expect("mkdir by-id");
    std::os::unix::fs::symlink(format!("../../{dev_name}"), by_id.join(name))
        .expect("symlink by-id");
}

/// `/dev/serial/by-path/<port> -> ../../<dev_name>`.
fn by_path_link(root: &Path, port: &str, dev_name: &str) {
    let by_path = root.join("dev/serial/by-path");
    std::fs::create_dir_all(&by_path).expect("mkdir by-path");
    std::os::unix::fs::symlink(format!("../../{dev_name}"), by_path.join(port))
        .expect("symlink by-path");
}

/// A single-`serial`-node TOML block for `add-node` (free-for-all so nothing here
/// hinges on a write lock).
fn node_toml(name: &str, device: &str) -> String {
    format!(
        "[[node]]\ntype = \"serial\"\nname = \"{name}\"\ndevice = \"{device}\"\narbitration = \"free-for-all\"\n"
    )
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// The `ports` entry whose `path` ends with `dev_name`, or `None` when the verb did
/// not list that device at all.
fn port_for<'a>(ports: &'a Value, dev_name: &str) -> Option<&'a Value> {
    ports
        .get("ports")
        .and_then(Value::as_array)
        .expect("ports array")
        .iter()
        .find(|p| str_at(p, "path").is_some_and(|s| s.ends_with(dev_name)))
}

#[test]
fn two_clones_sharing_one_serial_get_one_identity_each() {
    // RES-1. The fixture is what udev actually produces for two adapters with the
    // same hard-coded serial: ONE by-id link (the name collides, so the second
    // device gets none) and a by-path entry for each. Counting links, the guard saw
    // one entry and captured the ambiguous `usb:0403:6001:DUP:00` for both.
    let run = TempRun::new();
    let root = run.join("devroot");
    for (usbdir, dev_name, port) in [
        ("1-1", "ttyUSB0", "pci-0000:00:14.0-usb-0:1:1.0-port0"),
        ("2-1", "ttyUSB1", "pci-0000:00:14.0-usb-0:2:1.0-port0"),
    ] {
        fifo(&root.join("dev").join(dev_name));
        sysfs_device(
            &root,
            usbdir,
            dev_name,
            "0403",
            "6001",
            Some("DUP"),
            "00",
            ("FTDI", "FT232R USB UART"),
        );
        by_path_link(&root, port, dev_name);
    }
    // udev's single link, for the name both clones would claim.
    by_id_link(&root, "usb-FTDI_FT232R_USB_UART_DUP-if00-port0", "ttyUSB0");

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    // `ports` must not advertise an identity that binds either clone.
    let ports = rpc.ports();
    for dev_name in ["ttyUSB0", "ttyUSB1"] {
        let p =
            port_for(&ports, dev_name).unwrap_or_else(|| panic!("{dev_name} unlisted: {ports:#}"));
        assert_eq!(
            str_at(p, "kind"),
            Some("by-path"),
            "an ambiguous serial must degrade to topology identity (§12/§15.10): {p:#}"
        );
        assert!(
            str_at(p, "warning").is_some_and(|w| w.contains("topology")),
            "the degraded form must state its instability: {p:#}"
        );
    }

    // Adding each clone by its own raw path captures a *different* identity.
    let a = rpc
        .add_node_toml(&node_toml("portA", "/dev/ttyUSB0"))
        .expect("add portA");
    let b = rpc
        .add_node_toml(&node_toml("portB", "/dev/ttyUSB1"))
        .expect("add portB");
    for (echo, name) in [(&a, "portA"), (&b, "portB")] {
        assert_eq!(
            str_at(echo, "kind"),
            Some("by-path"),
            "{name} captured a serial two devices answer to: {echo:#}"
        );
        assert!(
            echo.get("warning").filter(|w| !w.is_null()).is_some(),
            "{name}'s degraded capture carried no .warning: {echo:#}"
        );
    }
    assert_ne!(
        str_at(&a, "identity"),
        str_at(&b, "identity"),
        "two physical adapters must not share one configured identity: {a:#} {b:#}"
    );

    // The operator-visible half: each node resolves to its OWN device. Before the
    // fix both nodes reported `.../ttyUSB0` — everything typed at portB went to the
    // other board, and the second adapter was unreachable by any node.
    let pa = rpc.node("portA").expect("portA in state");
    let pb = rpc.node("portB").expect("portB in state");
    let path_a = str_at(&pa, "resolved_path").expect("portA resolved_path");
    let path_b = str_at(&pb, "resolved_path").expect("portB resolved_path");
    assert!(path_a.ends_with("/dev/ttyUSB0"), "portA -> {path_a}");
    assert!(path_b.ends_with("/dev/ttyUSB1"), "portB -> {path_b}");

    // …and `ports` agrees: one node per device, not one node holding both.
    let after = rpc.ports();
    assert_eq!(
        str_at(port_for(&after, "ttyUSB0").expect("ttyUSB0"), "bound_to"),
        Some("portA"),
        "{after:#}"
    );
    assert_eq!(
        str_at(port_for(&after, "ttyUSB1").expect("ttyUSB1"), "bound_to"),
        Some("portB"),
        "{after:#}"
    );
}

#[test]
fn an_ambiguous_usb_identity_reloaded_from_config_binds_nothing() {
    // RES-1's other half, and the one the capture guard cannot reach. Capture
    // degrades a duplicated serial to by-path, so nothing this daemon *mints* is
    // ambiguous — but `load`, startup from the persisted state file, and `add-node
    // device = "usb:…"` all take the identity-form path, which never consulted the
    // ambiguity guard at all. That is exactly the path an upgrader takes: a pre-fix
    // daemon captured `usb:0403:6001:DUP:00` for both clones and `dump` persisted it,
    // so restarting on an existing state file reproduced the finding with no operator
    // action. It is also where a clone plugged in *after* a clean capture lands, which
    // no history rewrite reaches.
    //
    // Fail-first: both adds were accepted with `kind: usb`, no `warning`, and BOTH
    // reported `resolved_path: …/dev/ttyUSB0` — one board answering for two nodes
    // while the second adapter was unreachable by any node.
    let run = TempRun::new();
    let root = run.join("devroot");
    for (usbdir, dev_name, port) in [
        ("1-1", "ttyUSB0", "pci-0000:00:14.0-usb-0:1:1.0-port0"),
        ("2-1", "ttyUSB1", "pci-0000:00:14.0-usb-0:2:1.0-port0"),
    ] {
        fifo(&root.join("dev").join(dev_name));
        sysfs_device(
            &root,
            usbdir,
            dev_name,
            "0403",
            "6001",
            Some("DUP"),
            "00",
            ("FTDI", "FT232R USB UART"),
        );
        by_path_link(&root, port, dev_name);
    }
    by_id_link(&root, "usb-FTDI_FT232R_USB_UART_DUP-if00-port0", "ttyUSB0");

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    // The add itself still succeeds — §12's asymmetry is that identity-form input
    // never requires the device present, which is why `dump` emits identities and why
    // configurations survive cold starts. Refusing here would refuse every config
    // written while the hardware was unplugged.
    for name in ["portA", "portB"] {
        let echo = rpc
            .add_node_toml(&node_toml(name, "usb:0403:6001:DUP:00"))
            .unwrap_or_else(|e| panic!("identity-form add of {name}: [{}] {}", e.code, e.message));
        assert_eq!(
            echo.get("resolved_path"),
            Some(&Value::Null),
            "{name} bound a device an identity two adapters answer to cannot pin: {echo:#}"
        );
        let warning = echo
            .get("warning")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{name}'s add carried no .warning: {echo:#}"));
        assert!(
            warning.contains("ttyUSB0") && warning.contains("ttyUSB1"),
            "the warning must name both devices that answer: {warning}"
        );
    }

    // …and `state` agrees at every recheck, not just at add time: `resolved_path` is
    // recomputed live from the stored identity, and `load`/startup never emit the echo
    // above at all, so this is the assertion that covers the upgrade path.
    for name in ["portA", "portB"] {
        let node = rpc.node(name).unwrap_or_else(|| panic!("{name} in state"));
        assert_eq!(
            node.get("resolved_path"),
            Some(&Value::Null),
            "{name} resolved an ambiguous identity to a device: {node:#}"
        );
    }

    // The operator's way out is in front of them: `ports` still lists both adapters,
    // each under the by-path identity that *does* pin exactly one of them.
    let ports = rpc.ports();
    for dev_name in ["ttyUSB0", "ttyUSB1"] {
        let p =
            port_for(&ports, dev_name).unwrap_or_else(|| panic!("{dev_name} unlisted: {ports:#}"));
        assert_eq!(str_at(p, "kind"), Some("by-path"), "{p:#}");
        assert_eq!(
            p.get("bound_to"),
            Some(&Value::Null),
            "a device no node can pin must not report itself bound: {p:#}"
        );
    }
}

#[test]
fn an_adapter_udev_never_named_is_enumerable_and_bindable() {
    // RES-2. `/sys` mounted, no `/dev/serial` at all — a container started with
    // `--device=/dev/ttyUSB0`, or any image without udev's `60-serial.rules`.
    // Capture minted a `usb:` identity from sysfs that resolution, reading only
    // by-id, could never honour.
    let run = TempRun::new();
    let root = run.join("devroot");
    fifo(&root.join("dev/ttyUSB0"));
    sysfs_device(
        &root,
        "1-1",
        "ttyUSB0",
        "0403",
        "6001",
        Some("UNIQ01"),
        "00",
        ("FTDI", "FT232R USB UART"),
    );
    assert!(
        !root.join("dev/serial").exists(),
        "the fixture must model a tree with no udev serial links at all"
    );

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    // The enumeration face used to answer `[]` here, pointing the operator away
    // from the one device that is present.
    let ports = rpc.ports();
    let p = port_for(&ports, "ttyUSB0").unwrap_or_else(|| panic!("ttyUSB0 unlisted: {ports:#}"));
    assert_eq!(str_at(p, "identity"), Some("usb:0403:6001:UNIQ01:00"));
    assert_eq!(str_at(p, "kind"), Some("usb"));
    assert_eq!(
        p.get("by_id"),
        Some(&Value::Null),
        "there is no by-id entry"
    );
    assert_eq!(
        p.get("warning"),
        Some(&Value::Null),
        "a usb identity is not a fallback"
    );

    let echo = rpc
        .add_node_toml(&node_toml("console", "/dev/ttyUSB0"))
        .expect("add by a present raw path");
    assert_eq!(str_at(&echo, "identity"), Some("usb:0403:6001:UNIQ01:00"));
    assert_eq!(str_at(&echo, "kind"), Some("usb"));

    // The half that used to be impossible. `resolved_path` is recomputed live from
    // the stored identity on every `state`, so a null here is the daemon saying the
    // device is absent — which is what it said, forever, about a device it had just
    // described in the reply above.
    let node = rpc.node("console").expect("console in state");
    assert_eq!(str_at(&node, "identity"), Some("usb:0403:6001:UNIQ01:00"));
    let resolved = str_at(&node, "resolved_path").unwrap_or_else(|| {
        panic!("the daemon minted an identity it cannot resolve back (RES-2): {node:#}")
    });
    assert!(resolved.ends_with("/dev/ttyUSB0"), "resolved to {resolved}");
    assert_eq!(
        str_at(
            port_for(&rpc.ports(), "ttyUSB0").expect("ttyUSB0"),
            "bound_to"
        ),
        Some("console")
    );
}

#[test]
fn adding_by_the_canonical_by_id_path_captures_the_usb_identity() {
    // RES-3. `/dev/serial/by-id/usb-…` is the spelling every serial-tooling doc
    // hands an operator; taking `file_name()` of it fed the sysfs walk a link name,
    // so the most canonical input there is stored `raw:` — squatter refusal off,
    // human echo degraded, and a reboot-instability warning on the one path that is
    // stable across reboots.
    let run = TempRun::new();
    let root = run.join("devroot");
    fifo(&root.join("dev/ttyUSB0"));
    sysfs_device(
        &root,
        "1-1",
        "ttyUSB0",
        "0403",
        "6001",
        Some("A6008isP"),
        "00",
        ("FTDI", "FT232R USB UART"),
    );
    by_id_link(
        &root,
        "usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0",
        "ttyUSB0",
    );
    by_path_link(&root, "pci-0000:00:14.0-usb-0:2:1.0-port0", "ttyUSB0");

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    // Every idiomatic spelling of the same device captures the same identity. Each
    // node is removed before the next add so the assertions never depend on the
    // daemon tolerating two nodes on one device.
    for (name, input) in [
        ("direct", "/dev/ttyUSB0"),
        (
            "byid",
            "/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0",
        ),
        (
            "bypath",
            "/dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0",
        ),
    ] {
        let echo = rpc
            .add_node_toml(&node_toml(name, input))
            .unwrap_or_else(|e| panic!("add {name} by {input}: [{}] {}", e.code, e.message));
        assert_eq!(
            str_at(&echo, "identity"),
            Some("usb:0403:6001:A6008isP:00"),
            "{input} did not capture the canonical identity: {echo:#}"
        );
        assert_eq!(str_at(&echo, "kind"), Some("usb"), "{echo:#}");
        assert_eq!(
            echo.get("warning").unwrap_or(&Value::Null),
            &Value::Null,
            "a usb capture must not carry a fallback's instability warning: {echo:#}"
        );
        assert!(
            str_at(&echo, "description").is_some_and(|d| d.contains("FTDI FT232R")),
            "the operator echo must name the adapter, so a wrong device is noticed \
             (§12): {echo:#}"
        );
        // What `dump` round-trips is the canonical identity, not the input path —
        // otherwise the stored config outlives neither a reboot nor a replug.
        let dumped = rpc.dump();
        let device = dumped["node"]
            .as_array()
            .expect("dump.node array")
            .iter()
            .find(|n| n["name"] == name)
            .and_then(|n| n["device"].as_str())
            .unwrap_or_else(|| panic!("{name} missing from dump: {dumped:#}"));
        assert_eq!(device, "usb:0403:6001:A6008isP:00", "{dumped:#}");
        rpc.remove_node(name, false).expect("remove-node");
    }
}

#[test]
fn a_node_bound_without_a_by_id_tree_actually_opens_its_device() {
    // RES-2, end to end on a device that can really be opened: the reported symptom
    // was `status: waiting` forever, for hardware that was present. Needs a sim-pty
    // serial device at a fixture `/dev` path (Linux-only — a pts is not a `serial2`
    // device on macOS, `ENOTTY`), so it self-skips like the other `p7_*`/`p12_*`
    // device tests.
    if !cfg!(target_os = "linux") || serial_echo().is_none() {
        eprintln!(
            "SKIP a_node_bound_without_a_by_id_tree_actually_opens_its_device: \
             fixture sysfs + sim-pty serial devices are Linux-only (§12)"
        );
        return;
    }

    let run = TempRun::new();
    let root = run.join("devroot");
    std::fs::create_dir_all(root.join("dev")).expect("mkdir <root>/dev");
    sysfs_device(
        &root,
        "1-1",
        "ttyUSB0",
        "0403",
        "6001",
        Some("UNIQ01"),
        "00",
        ("FTDI", "FT232R USB UART"),
    );
    let dev_node = root.join("dev/ttyUSB0");
    let _device = Sim::spawn(
        &[
            "pty",
            "--echo",
            "--link",
            &dev_node.to_string_lossy(),
            "--timeout-ms",
            "120000",
        ],
        Some(&dev_node),
    );

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();
    let echo = rpc
        .add_node_toml(&node_toml("console", "/dev/ttyUSB0"))
        .expect("add by a present raw path");
    assert_eq!(str_at(&echo, "identity"), Some("usb:0403:6001:UNIQ01:00"));

    // The captured identity is honoured: the node opens the device rather than
    // waiting on it forever.
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(15)),
        "a device present in sysfs but unnamed by udev never came up: {:?}",
        rpc.node("console")
    );

    // And an identity-form add of the same string — the shape a persisted config
    // reloads as — resolves identically, which is the property that survives a
    // restart.
    rpc.add_node_toml(&node_toml("again", "usb:0403:6001:UNIQ01:00"))
        .expect("add by the captured identity");
    let node = rpc.node("again").expect("again in state");
    assert!(
        str_at(&node, "resolved_path").is_some_and(|p| p.ends_with("/dev/ttyUSB0")),
        "the stored identity did not resolve on reload: {node:#}"
    );
}
