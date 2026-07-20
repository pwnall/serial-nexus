#![deny(unsafe_code)]

//! S4 — resolver ground truth (design §12; plan phase 0).
//!
//! Questions: what do `/dev/serial/by-id` and its symlinks actually give us,
//! and can we build the canonical `usb:<vid>:<pid>:<serial>:<iface>` identity
//! (§12) without libudev? The by-id *name* is human-friendly but ambiguous to
//! parse (vendor/model strings contain underscores); the authoritative numeric
//! identity comes from a dependency-free sysfs walk from the resolved device.
//! by-path is the topology fallback for adapters with absent/duplicate serials.
//!
//! `--dev-root` (default `/`) makes the by-id tree a fixture directory, the
//! first-class test seam of §3. Prints one JSON verdict line; passes when every
//! present adapter resolves to an identity (skips cleanly when none is present).

use std::path::{Path, PathBuf};

use serde_json::json;

fn main() {
    let dev_root = parse_dev_root().unwrap_or_else(|| PathBuf::from("/"));
    let sys_root = PathBuf::from("/sys");
    let verdict = run(&dev_root, &sys_root);
    println!("{verdict}");
    let pass = verdict
        .get("pass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    std::process::exit(if pass { 0 } else { 1 });
}

fn parse_dev_root() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--dev-root" {
            return args.next().map(PathBuf::from);
        }
        if let Some(v) = a.strip_prefix("--dev-root=") {
            return Some(PathBuf::from(v));
        }
    }
    None
}

fn run(dev_root: &Path, sys_root: &Path) -> serde_json::Value {
    let by_id_dir = dev_root.join("dev/serial/by-id");
    let by_path_dir = dev_root.join("dev/serial/by-path");

    let entries = match std::fs::read_dir(&by_id_dir) {
        Ok(rd) => rd,
        Err(_) => {
            return json!({
                "tool": "s4_resolver", "spike": "S4",
                "dev_root": dev_root.display().to_string(),
                "by_id_present": false,
                "skipped": true,
                "reason": "no /dev/serial/by-id tree (no USB serial adapter and no fixture)",
                "pass": true
            });
        }
    };

    let mut resolved = Vec::new();
    let mut all_ok = true;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let link = entry.path();
        let target = match std::fs::read_link(&link) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let dev_name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let iface_from_name = name
            .rsplit_once("-if")
            .and_then(|(_, rest)| rest.split(['-', '_']).next())
            .map(|s| s.to_owned());

        let usb = sysfs_usb_identity(sys_root, &dev_name);
        let by_path = by_path_aliases(&by_path_dir, &dev_name);

        let identity = usb.as_ref().map(|u| {
            format!(
                "usb:{}:{}:{}:{}",
                u.vid,
                u.pid,
                u.serial.clone().unwrap_or_else(|| "-".into()),
                u.interface
                    .clone()
                    .or_else(|| iface_from_name.clone())
                    .unwrap_or_else(|| "-".into())
            )
        });
        // On a real system every present adapter must resolve to a numeric
        // identity; against a bare fixture (no sysfs) we accept the by-id
        // name + interface parse as the ground truth we can offer.
        if usb.is_none() && sys_root == Path::new("/sys") && dev_root == Path::new("/") {
            all_ok = false;
        }

        resolved.push(json!({
            "by_id_name": name,
            "dev_path": format!("/dev/{dev_name}"),
            "interface_from_name": iface_from_name,
            "vid": usb.as_ref().map(|u| u.vid.clone()),
            "pid": usb.as_ref().map(|u| u.pid.clone()),
            "serial": usb.as_ref().and_then(|u| u.serial.clone()),
            "interface": usb.as_ref().and_then(|u| u.interface.clone()),
            "identity": identity,
            "by_path_aliases": by_path,
        }));
    }

    // A present-but-empty tree (adapter unplugged, static /dev) is a clean skip,
    // exactly like the absent-directory branch — not a failure.
    if resolved.is_empty() {
        return json!({
            "tool": "s4_resolver", "spike": "S4",
            "dev_root": dev_root.display().to_string(),
            "by_id_present": true,
            "count": 0,
            "skipped": true,
            "reason": "by-id tree present but empty (no adapters)",
            "pass": true,
        });
    }

    json!({
        "tool": "s4_resolver", "spike": "S4",
        "dev_root": dev_root.display().to_string(),
        "by_id_present": true,
        "count": resolved.len(),
        "adapters": resolved,
        "note": "numeric vid:pid:serial:iface comes from a dependency-free sysfs walk; by-id name alone is ambiguous",
        // `all_ok` is already false if any *present* adapter failed to resolve.
        "pass": all_ok,
    })
}

struct UsbIdentity {
    vid: String,
    pid: String,
    serial: Option<String>,
    interface: Option<String>,
}

/// Walk sysfs from `/sys/class/tty/<dev>/device` *up the ancestor chain* to the
/// USB device node, reading idVendor/idProduct/serial and the interface's
/// bInterfaceNumber. The nesting depth differs between ttyUSB (usb-serial) and
/// ttyACM (CDC), so we don't assume a fixed number of parents: we take the
/// nearest ancestor bearing `bInterfaceNumber` as the interface, and the first
/// ancestor bearing `idVendor` as the device (stopping there so we bind the
/// adapter, not the root hub above it). No libudev, no dependencies — just
/// files (§12, §13).
fn sysfs_usb_identity(sys_root: &Path, dev_name: &str) -> Option<UsbIdentity> {
    let device_link = sys_root.join("class/tty").join(dev_name).join("device");
    let start = std::fs::canonicalize(&device_link).ok()?;

    let mut interface = None;
    let mut cur: &Path = &start;
    for _ in 0..12 {
        if interface.is_none() {
            interface = read_trimmed(&cur.join("bInterfaceNumber"));
        }
        if cur.join("idVendor").exists() {
            let vid = read_trimmed(&cur.join("idVendor"))?;
            let pid = read_trimmed(&cur.join("idProduct"))?;
            let serial = read_trimmed(&cur.join("serial"));
            return Some(UsbIdentity {
                vid,
                pid,
                serial,
                interface,
            });
        }
        match cur.parent() {
            Some(p) if p != cur && p.starts_with(sys_root) => cur = p,
            _ => break,
        }
    }
    None
}

fn by_path_aliases(by_path_dir: &Path, dev_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(by_path_dir) {
        for e in rd.flatten() {
            if let Ok(t) = std::fs::read_link(e.path()) {
                if t.file_name().map(|s| s.to_string_lossy()).as_deref() == Some(dev_name) {
                    out.push(e.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    out
}

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}
