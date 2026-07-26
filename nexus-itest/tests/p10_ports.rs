//! The `ports` verb — the resolver's passive device enumeration (design §12,
//! §15.35; plan §14.1).
//!
//! `ports` is the verb an operator runs *to look*: it answers "what serial
//! hardware is on this machine, what identity would bind it, and is anything
//! already holding it" without touching a single device. Four properties:
//!
//! 1. **Every candidate is listed, in its §12 identity form.** A USB adapter with
//!    a serial number appears as `usb:…`; one without appears as `by-path:…` with
//!    the documented instability warning; a BSD-style `cu.*` callout node appears
//!    as `raw:…`. What `ports` advertises is what an `add-node` on that path would
//!    store, because both go through the same fallback chain.
//! 2. **Bound status is live.** A free candidate reports `bound_to: null`; after a
//!    node binds it, the very next `ports` names that node — and only for the port
//!    it took.
//! 3. **Binding by any identity form is recognized.** `bound_to` resolves each
//!    node's stored identity to a path and compares paths, so a device held by a
//!    `by-path:` identity reports bound even though its own identity spelling is
//!    different.
//! 4. **Nothing is opened.** The fixture's device nodes are FIFOs with no writer.
//!    A blocking `open(2)` of one would wedge the daemon's single runtime thread
//!    and the RPC would time out; `ports` answers promptly instead. That is the
//!    property §15.35 cares about — probing a port toggles DTR and resets the
//!    board behind it — with the structural half proved by
//!    `meta_gates::port_enumeration_cannot_open_a_device`.
//!
//! Device-free: the fixture is a tree of symlinks, text files and FIFOs, and no
//! test here needs a real UART or a pts. It runs on every platform.

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, TempRun};
use serde_json::Value;

/// Write `contents` to `p`, creating parents.
fn write(p: &Path, contents: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir -p");
    std::fs::write(p, contents).expect("write fixture file");
}

/// Create a device node that **cannot be read without blocking**: a FIFO with no
/// writer. `open(2)` for reading on one blocks until a writer arrives, so any code
/// that tried to probe this "device" would hang rather than return — which is the
/// runtime half of the passivity proof.
fn fifo(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir -p");
    let status = std::process::Command::new("mkfifo")
        .arg(p)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo {} failed", p.display());
}

/// A faithful by-id + sysfs fixture for one USB tty device, mirroring the
/// resolver's own unit-test fixture (`nexus-core/src/resolver.rs`). `serial =
/// None` models an adapter with no serial number, which is what makes the §12
/// fallback to by-path reachable.
#[allow(clippy::too_many_arguments)]
fn add_usb_device(
    root: &Path,
    usbdir: &str,
    dev_name: &str,
    by_id_name: Option<&str>,
    by_path_name: Option<&str>,
    vid: &str,
    pid: &str,
    serial: Option<&str>,
    iface: &str,
    strings: (&str, &str),
) {
    fifo(&root.join("dev").join(dev_name));
    if let Some(by_id_name) = by_id_name {
        let by_id = root.join("dev/serial/by-id");
        std::fs::create_dir_all(&by_id).expect("mkdir by-id");
        std::os::unix::fs::symlink(format!("../../{dev_name}"), by_id.join(by_id_name))
            .expect("symlink by-id");
    }
    if let Some(by_path_name) = by_path_name {
        let by_path = root.join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).expect("mkdir by-path");
        std::os::unix::fs::symlink(format!("../../{dev_name}"), by_path.join(by_path_name))
            .expect("symlink by-path");
    }
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

/// The `ports` entry whose `path` ends with `dev_name`.
fn port_for<'a>(ports: &'a Value, dev_name: &str) -> &'a Value {
    ports
        .get("ports")
        .and_then(Value::as_array)
        .expect("ports array")
        .iter()
        .find(|p| {
            p.get("path")
                .and_then(Value::as_str)
                .is_some_and(|s| s.ends_with(dev_name))
        })
        .unwrap_or_else(|| panic!("no port for {dev_name} in {ports:#}"))
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Build the fixture dev-root: two USB adapters (one with a serial number, one
/// without) plus a BSD-style callout node.
fn fixture(root: &Path) {
    // A well-behaved adapter: by-id entry, serial number → a `usb:` identity.
    add_usb_device(
        root,
        "1-1",
        "ttyUSB0",
        Some("usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0"),
        Some("pci-0000:00:14.0-usb-0:1:1.0-port0"),
        "0403",
        "6001",
        Some("A6008isP"),
        "00",
        ("FTDI", "FT232R USB UART"),
    );
    // An adapter with no serial number: the §12 chain degrades it to by-path.
    add_usb_device(
        root,
        "1-2",
        "ttyUSB1",
        Some("usb-Prolific_USB-Serial_Controller-if00-port0"),
        Some("pci-0000:00:14.0-usb-0:2:1.0-port0"),
        "067b",
        "2303",
        None,
        "00",
        ("Prolific", "USB-Serial Controller"),
    );
    // The BSD/macOS face: a bare callout node, no by-id tree entry, no sysfs.
    fifo(&root.join("dev/cu.usbserial-FT9999"));
    // Its `tty.*` twin, which must never be offered: `cu` is the callout device.
    fifo(&root.join("dev/tty.usbserial-FT9999"));
}

#[test]
fn ports_enumerates_every_candidate_in_its_identity_form() {
    let run = TempRun::new();
    let root = run.join("devroot");
    fixture(&root);

    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let ports = d.rpc().ports();
    let list = ports.get("ports").and_then(Value::as_array).expect("array");

    // Exactly the three candidates — the `tty.*` twin is not one of them.
    let paths: Vec<&str> = list.iter().filter_map(|p| str_at(p, "path")).collect();
    assert_eq!(
        paths.len(),
        3,
        "expected 3 candidates (cu.*, ttyUSB0, ttyUSB1), got {ports:#}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("tty.usbserial")),
        "the tty.* twin of a cu.* callout node must not be listed: {ports:#}"
    );

    // 1. The adapter with a serial number is a canonical `usb:` identity, no warning.
    let usb0 = port_for(&ports, "ttyUSB0");
    assert_eq!(str_at(usb0, "identity"), Some("usb:0403:6001:A6008isP:00"));
    assert_eq!(str_at(usb0, "kind"), Some("usb"));
    assert_eq!(
        str_at(usb0, "by_id"),
        Some("usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0")
    );
    assert!(
        str_at(usb0, "description").is_some_and(|d| d.contains("FTDI FT232R")),
        "the human echo names the adapter: {usb0:#}"
    );
    assert_eq!(
        usb0.get("warning"),
        Some(&Value::Null),
        "a usb identity is not a fallback and carries no instability warning"
    );

    // 2. The serial-less adapter degrades to by-path, warning and all (§12).
    let usb1 = port_for(&ports, "ttyUSB1");
    assert_eq!(
        str_at(usb1, "identity"),
        Some("by-path:pci-0000:00:14.0-usb-0:2:1.0-port0")
    );
    assert_eq!(str_at(usb1, "kind"), Some("by-path"));
    assert!(
        str_at(usb1, "warning").is_some_and(|w| w.contains("topology")),
        "the by-path fallback states its instability: {usb1:#}"
    );

    // 3. The callout node is the raw escape hatch.
    let cu = port_for(&ports, "cu.usbserial-FT9999");
    assert_eq!(str_at(cu, "identity"), Some("raw:/dev/cu.usbserial-FT9999"));
    assert_eq!(str_at(cu, "kind"), Some("raw"));

    // 4. Nothing is bound yet — every candidate is free.
    for p in list {
        assert_eq!(
            p.get("bound_to"),
            Some(&Value::Null),
            "nothing is configured, so nothing is bound: {p:#}"
        );
    }
}

#[test]
fn binding_a_listed_port_flips_exactly_that_ports_status() {
    let run = TempRun::new();
    let root = run.join("devroot");
    fixture(&root);
    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    // Take the identity straight out of `ports` — the workflow the verb exists
    // for: see what is plugged in, then bind it without knowing a device path.
    let identity = str_at(port_for(&rpc.ports(), "ttyUSB0"), "identity")
        .expect("identity")
        .to_owned();
    rpc.add_node_toml(&format!(
        r#"
        [[node]]
        type = "serial"
        name = "usb0"
        device = "{identity}"
        baud = 115200
        "#
    ))
    .expect("add-node by the identity ports advertised");

    let after = rpc.ports();
    assert_eq!(
        str_at(port_for(&after, "ttyUSB0"), "bound_to"),
        Some("usb0"),
        "the bound port names its node: {after:#}"
    );
    assert_eq!(
        port_for(&after, "ttyUSB1").get("bound_to"),
        Some(&Value::Null),
        "a neighbour is not bound by association: {after:#}"
    );
    assert_eq!(
        port_for(&after, "cu.usbserial-FT9999").get("bound_to"),
        Some(&Value::Null)
    );

    // Removing the node hands the port back.
    rpc.remove_node("usb0", false).expect("remove-node");
    assert_eq!(
        port_for(&rpc.ports(), "ttyUSB0").get("bound_to"),
        Some(&Value::Null),
        "the port is free again once nothing binds it"
    );
}

#[test]
fn bound_status_follows_the_device_not_the_identity_spelling() {
    // A node may hold a device by a `by-path:` or raw identity whose *spelling*
    // differs from the `usb:` identity `ports` advertises for the same device.
    // Comparing strings would report it free and invite an operator to bind it
    // twice; `bound_to` resolves both sides to a path instead.
    let run = TempRun::new();
    let root = run.join("devroot");
    fixture(&root);
    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    rpc.add_node_toml(
        r#"
        [[node]]
        type = "serial"
        name = "topo"
        device = "by-path:pci-0000:00:14.0-usb-0:1:1.0-port0"
        baud = 115200
        "#,
    )
    .expect("add-node by topology identity");

    let ports = rpc.ports();
    let usb0 = port_for(&ports, "ttyUSB0");
    assert_eq!(
        str_at(usb0, "identity"),
        Some("usb:0403:6001:A6008isP:00"),
        "the advertised identity is still the preferred usb form"
    );
    assert_eq!(
        str_at(usb0, "bound_to"),
        Some("topo"),
        "a differently-spelled identity for the same device still reports bound: {ports:#}"
    );
}

#[test]
fn bound_status_follows_a_symlinked_raw_path_to_the_device_it_names() {
    // A `raw:` identity resolves *literally* — that is the escape hatch's documented
    // behaviour (§12) — so a node bound through `/dev/serial/by-id/…` holds a path
    // that is textually unlike the `/dev/ttyUSB0` `ports` advertises. Comparing the
    // strings would report the device free and invite an operator to bind it twice,
    // where the second node just faults on TIOCEXCL. Canonicalize both sides.
    let run = TempRun::new();
    let root = run.join("devroot");
    fixture(&root);
    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);
    let rpc = d.rpc();

    rpc.add_node_toml(
        r#"
        type = "serial"
        name = "byid"
        device = "raw:/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0"
        baud = 115200
        "#
        .replace(
            "        type =",
            "        [[node]]
        type =",
        )
        .as_str(),
    )
    .expect("add-node by a symlinked raw path");

    let ports = rpc.ports();
    assert_eq!(
        str_at(port_for(&ports, "ttyUSB0"), "bound_to"),
        Some("byid"),
        "a raw path that is a symlink to the device still reports bound: {ports:#}"
    );
}

#[test]
fn enumeration_never_opens_a_candidate_device() {
    // Every device node in the fixture is a writer-less FIFO. A blocking
    // `open(2)` of one never returns, so a daemon that probed a candidate would
    // wedge its single runtime thread and this call would hit the RPC read
    // timeout instead of answering. Repeated, because a one-shot cache would hide
    // a probe that only happens on the first call.
    let run = TempRun::new();
    let root = run.join("devroot");
    fixture(&root);
    let d = Daemon::start_with_args(&["--dev-root", root.to_str().expect("utf-8 fixture path")]);

    let started = std::time::Instant::now();
    for _ in 0..3 {
        let ports = d.rpc().ports();
        assert_eq!(
            ports
                .get("ports")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            3,
            "enumeration is stable across calls: {ports:#}"
        );
    }
    // Generous: the point is "returned at all", not a latency budget. Three
    // blocking opens would have burned the 30s RPC timeout three times over.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "three `ports` calls took {:?} — something is probing the devices",
        started.elapsed()
    );
    // The daemon is still answering afterwards, so nothing is parked on a fd.
    assert!(d.rpc().info().get("daemon_version").is_some());
}
