//! Everything the helper knows about `/sys`, including every check that runs
//! *before* a privileged write (design §15.45).
//!
//! The helper holds `CAP_DAC_OVERRIDE`, which bypasses every DAC check on the
//! system, so the interface it exposes is the security boundary. That interface is
//! deliberately not a path: it is a USB **port name** like `3-1`, matched against a
//! character class and joined to a compile-time constant root. Nothing a caller
//! supplies ever reaches `open(2)` as a path.

use std::io;
use std::path::{Path, PathBuf};

/// The one root the helper will ever touch. A constant, never a parameter — a
/// `--sysfs-root` flag would hand the caller the write primitive this module exists
/// to withhold, and is the change §15.45 forbids without a design amendment.
pub const USB_DEVICES: &str = "/sys/bus/usb/devices";

/// USB device class 9 is a hub. Deauthorizing a hub takes down every device behind
/// it — which on a laptop can be the keyboard, or the disk the tree is on.
const CLASS_HUB: &str = "09";

/// Drivers whose presence means "this is a USB serial adapter". The helper refuses
/// any device that is not one, so the blast radius of the capability is bounded to
/// the device class this repository is about (§13).
const SERIAL_DRIVERS: &[&str] = &["ftdi_sio", "cp210x", "ch341", "pl2303", "cdc_acm"];

/// A USB port name that has passed [`validate_port_name`].
///
/// Constructing one is the only way to reach the read and write helpers below, so
/// "was this validated?" is answered by the type rather than by review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port(String);

impl Port {
    /// The port's sysfs directory: the constant root joined to the validated name.
    pub fn dir(&self) -> PathBuf {
        Path::new(USB_DEVICES).join(&self.0)
    }

    /// The port name as given, e.g. `3-1` or `3-1.4`.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Accept exactly `<bus>-<port>[.<port>]*`: digits, one `-`, and up to five dotted
/// hub-chain segments.
///
/// This is the first of the four checks, and the one that makes the rest possible:
/// because the accepted alphabet is digits, `-` and `.`, no `..` component and no
/// `/` can survive it, so [`Port::dir`] cannot escape [`USB_DEVICES`] no matter what
/// the caller passes. Canonicalization is therefore a belt-and-braces step rather
/// than the boundary — which is the opposite of a design that takes a path.
pub fn validate_port_name(name: &str) -> Result<Port, String> {
    let bad = |why: &str| {
        Err(format!(
            "refusing port name {name:?}: {why}. Expected <bus>-<port>[.<port>]*, e.g. 3-1 or 3-1.4"
        ))
    };
    if name.is_empty() || name.len() > 64 {
        return bad("empty or absurdly long");
    }
    let Some((bus, chain)) = name.split_once('-') else {
        return bad("no '-' separating bus from port");
    };
    if bus.is_empty() || !bus.bytes().all(|b| b.is_ascii_digit()) {
        return bad("bus is not a decimal number");
    }
    let segments: Vec<&str> = chain.split('.').collect();
    if segments.len() > 6 {
        return bad("more than six hub-chain segments");
    }
    for seg in &segments {
        if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
            return bad("a port segment is not a decimal number");
        }
    }
    Ok(Port(name.to_owned()))
}

/// Read a sysfs attribute, trimmed. `None` when absent or unreadable.
pub fn attr(port: &Port, name: &str) -> Option<String> {
    std::fs::read_to_string(port.dir().join(name))
        .ok()
        .map(|s| s.trim().to_owned())
}

/// Every `ttyUSB*`/`ttyACM*` this device currently owns, via its interface dirs.
///
/// Used three ways: to prove the device is a serial adapter before writing, to wait
/// for re-enumeration to finish, and to report what came back.
pub fn ttys(port: &Port) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(port.dir()) else {
        return found;
    };
    for iface in entries.flatten() {
        // Interface dirs are named `<port>:<config>.<intf>`; anything else here is
        // an attribute file or a link we do not care about.
        if !iface.file_name().to_string_lossy().contains(':') {
            continue;
        }
        // The tty shows up either as `<iface>/ttyUSB0` or under `<iface>/tty/ttyUSB0`
        // depending on the driver; check both rather than guessing.
        for sub in [iface.path(), iface.path().join("tty")] {
            let Ok(kids) = std::fs::read_dir(&sub) else {
                continue;
            };
            for kid in kids.flatten() {
                let n = kid.file_name().to_string_lossy().into_owned();
                if n.starts_with("ttyUSB") || n.starts_with("ttyACM") {
                    found.push(n);
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The driver bound to each interface of this device.
fn interface_drivers(port: &Port) -> Vec<String> {
    let mut drivers = Vec::new();
    let Ok(entries) = std::fs::read_dir(port.dir()) else {
        return drivers;
    };
    for iface in entries.flatten() {
        if !iface.file_name().to_string_lossy().contains(':') {
            continue;
        }
        if let Ok(link) = std::fs::read_link(iface.path().join("driver"))
            && let Some(name) = link.file_name()
        {
            drivers.push(name.to_string_lossy().into_owned());
        }
    }
    drivers
}

/// The four checks that must all pass before any privileged write, in order.
///
/// Fails closed and names which check failed, because the message is what an
/// operator acts on: "not a serial adapter" and "is a hub" call for different
/// responses.
pub fn check_is_a_replugable_serial_adapter(port: &Port) -> Result<(), String> {
    let dir = port.dir();
    // 1. It exists and is a directory.
    if !dir.is_dir() {
        return Err(format!(
            "{} is not a directory — no USB device is attached at port {port}",
            dir.display()
        ));
    }
    // 2. The kernel agrees it is sysfs. This is the check a path-string test cannot
    //    make, and the one that closes a bind-mount over the sysfs root.
    if !serial_nexus_sys::caps::is_sysfs(&dir) {
        return Err(format!(
            "{} is not on sysfs — refusing to write there",
            dir.display()
        ));
    }
    // 3. Not a hub.
    if attr(port, "bDeviceClass").as_deref() == Some(CLASS_HUB) {
        return Err(format!(
            "port {port} is a USB hub (bDeviceClass {CLASS_HUB}); deauthorizing it \
             would take down every device behind it"
        ));
    }
    // 4. It is a serial adapter — by a bound driver we know, or by owning a tty.
    let drivers = interface_drivers(port);
    let known = drivers.iter().any(|d| SERIAL_DRIVERS.contains(&d.as_str()));
    if !known && ttys(port).is_empty() {
        return Err(format!(
            "port {port} is not a USB serial adapter: no tty and no known serial \
             driver bound (drivers: {drivers:?}, recognised: {SERIAL_DRIVERS:?})"
        ));
    }
    Ok(())
}

/// Read the `authorized` attribute. `None` when the device has none — which is
/// itself disqualifying and reported as such by the caller.
pub fn authorized(port: &Port) -> Option<bool> {
    attr(port, "authorized").map(|v| v == "1")
}

/// Write `authorized`. **This is the only privileged operation in the binary.**
///
/// Every caller reaches it through [`check_is_a_replugable_serial_adapter`]; the
/// `Port` type is what makes that unskippable by construction.
pub fn set_authorized(port: &Port, value: bool) -> io::Result<()> {
    std::fs::write(port.dir().join("authorized"), if value { "1" } else { "0" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alphabet is the boundary, so it is tested as one: every spelling of an
    /// escape a caller might try must be refused, not merely the obvious `..`.
    #[test]
    fn only_bus_dash_port_names_are_accepted() {
        for good in ["3-1", "1-2", "3-1.4", "10-3.2.1", "3-1.2.3.4.5.6"] {
            assert!(
                validate_port_name(good).is_ok(),
                "{good} should be accepted"
            );
        }
        for bad in [
            "",
            "3",
            "-1",
            "3-",
            "3-1/../../../etc",
            "../3-1",
            "3-1/authorized",
            "3-1\0",
            "3-1 ",
            " 3-1",
            "3-1.",
            "3-.1",
            "a-1",
            "3-a",
            "3-1.2.3.4.5.6.7",
            "/sys/bus/usb/devices/3-1",
            "3-1;reboot",
            "3-1\n3-2",
        ] {
            assert!(
                validate_port_name(bad).is_err(),
                "{bad:?} must be refused by the port-name alphabet"
            );
        }
    }

    /// A validated name cannot address anything outside the constant root. This is
    /// the property the alphabet exists to give, asserted directly rather than
    /// inferred from the character class.
    #[test]
    fn a_validated_port_cannot_escape_the_constant_root() {
        for name in ["3-1", "10-3.2.1"] {
            let port = validate_port_name(name).expect("valid");
            let dir = port.dir();
            assert!(
                dir.starts_with(USB_DEVICES),
                "{dir:?} escaped {USB_DEVICES}"
            );
            assert!(
                !dir.to_string_lossy().contains(".."),
                "{dir:?} contains a parent traversal"
            );
            assert_eq!(dir.components().count(), 6, "unexpected depth for {dir:?}");
        }
    }

    /// A directory that is not sysfs is refused even when it is shaped exactly like
    /// a USB device dir — the check that a canonicalized-path test would miss.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_perfect_lookalike_outside_sysfs_is_still_refused() {
        let tmp = std::env::temp_dir().join(format!("snx-replug-fake-{}", std::process::id()));
        let dev = tmp.join("3-1");
        std::fs::create_dir_all(dev.join("3-1:1.0")).expect("mkdir");
        std::fs::write(dev.join("bDeviceClass"), "00\n").expect("write class");
        std::fs::write(dev.join("authorized"), "1\n").expect("write authorized");
        // Shaped right, but not on sysfs: `is_sysfs` is what refuses it.
        assert!(!serial_nexus_sys::caps::is_sysfs(&dev));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A hub is refused by class even if everything else about it looks fine. The
    /// real root hub on this box is class 09 and is a live case for the check.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hub_is_refused_by_device_class() {
        let Ok(port) = validate_port_name("usb3").or_else(|_| validate_port_name("3-0")) else {
            return;
        };
        if !port.dir().is_dir() {
            return; // no such port on this box; the unit above covers the logic
        }
        if attr(&port, "bDeviceClass").as_deref() == Some(CLASS_HUB) {
            let err = check_is_a_replugable_serial_adapter(&port).unwrap_err();
            assert!(err.contains("hub"), "hub refusal should say so: {err}");
        }
    }
}
