//! Device identity resolution (§12) — the dependency-free (no libudev, §15.10)
//! translation between operator input, the canonical identity stored in
//! configuration, and the current `/dev` path that is observed state.
//!
//! This is the one module in `nexus-core` that touches the filesystem: the
//! resolver reads `/dev/serial/by-id`, `/dev/serial/by-path`, and the sysfs
//! `bInterfaceNumber`/`idVendor` tree directly. It runs in two directions
//! (§12):
//!
//! * **input → identity**, once, at add time ([`Resolver::resolve_input`]).
//!   A raw `/dev` path or bare serial number must have the device *present* so
//!   its identity can be captured; an already-canonical identity never does.
//! * **identity → current path**, at every open and every faulted-and-wait
//!   recheck ([`Resolver::resolve_current_path`]). A `usb:` identity resolves
//!   only to a device whose sysfs identity matches *exactly*, so a different
//!   adapter squatting the old path is never adopted (§7.1) — squatter refusal
//!   falls out of resolution by construction.
//!
//! Both roots are parameterized so tests point them at fixture trees (plan §3);
//! `sys_root` defaults to `dev_root/sys`, so a single `--dev-root` selects a
//! self-contained fixture (and the production `dev_root = "/"` yields
//! `sys_root = "/sys"`). The Linux backend is the only one implemented; a macOS
//! IOKit backend is deferred (§14), which is why the walk sits behind this API
//! rather than being inlined at the call sites.

use std::path::{Path, PathBuf};

/// Which resolver form a stored identity uses, in preference order (§12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    /// `usb:<vid>:<pid>:<serial>:<iface>` — the canonical, squatter-safe form.
    Usb,
    /// `by-path:<port>` — topology identity ("whatever occupies this physical
    /// port"); a degraded fallback for adapters without a usable serial number.
    ByPath,
    /// `raw:<path>` — a raw `/dev` path escape hatch with no identity guarantee.
    Raw,
}

impl DeviceKind {
    fn scheme(self) -> &'static str {
        match self {
            DeviceKind::Usb => "usb",
            DeviceKind::ByPath => "by-path",
            DeviceKind::Raw => "raw",
        }
    }
}

/// The outcome of resolving operator input at add time (§12): the canonical
/// identity to store in configuration, the current `/dev` path if the device is
/// present, a human-readable echo, and an instability warning for the fallback
/// forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The identity to persist in the `device` config field. `dump` emits this,
    /// so a configuration survives a cold start with the hardware unplugged.
    pub identity: String,
    /// The `/dev` path the device currently occupies, or `None` when it is
    /// absent (only reachable for identity-form input, which comes up waiting).
    pub path: Option<PathBuf>,
    pub kind: DeviceKind,
    /// Echo for the operator, e.g. `"FTDI FT232R USB UART, serial A6008isP,
    /// interface 00"` — so a wrong physical device answering is noticed (§12).
    pub description: String,
    /// A documented instability warning for the `by-path`/`raw` fallbacks (§12).
    pub warning: Option<String>,
}

/// Why add-time resolution failed. Both variants fail the `add-node` operation;
/// neither can occur for identity-form input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// A raw-path or serial-number add whose device is not plugged in — its
    /// identity cannot be captured now (§12). Identity-form adds never hit this.
    #[error(
        "device {input:?} is not present; adding by raw path or serial number requires the device plugged in so its identity can be captured (§12) — add by a usb:/by-path: identity to configure it while absent"
    )]
    NotPresent { input: String },
    /// Structurally malformed input (empty, or a `usb:`/`by-path:`/`raw:`
    /// identity that does not parse).
    #[error("malformed device input {input:?}: {reason}")]
    Malformed { input: String, reason: String },
}

/// A discovered `/dev/serial/by-id` entry and the identity its sysfs walk
/// yields (`None` → by-path fallback). Shared with `nexus-doctor`'s P4 probe.
#[derive(Debug, Clone)]
pub struct Adapter {
    pub by_id_name: String,
    pub dev_path: PathBuf,
    pub identity: Option<String>,
}

struct UsbInfo {
    identity: String,
    description: String,
}

/// Linux device-identity resolver rooted at a `/dev` and a `/sys` prefix (§12).
#[derive(Debug, Clone)]
pub struct Resolver {
    dev_root: PathBuf,
    sys_root: PathBuf,
}

impl Resolver {
    /// Resolver over `dev_root`, with `sys_root = dev_root/sys` — so a single
    /// root selects a self-contained fixture and `"/"` yields `"/sys"`.
    pub fn new(dev_root: impl Into<PathBuf>) -> Self {
        let dev_root = dev_root.into();
        let sys_root = dev_root.join("sys");
        Self { dev_root, sys_root }
    }

    /// Resolver with independently chosen roots (a fixture whose sysfs lives
    /// elsewhere; the doctor's historical `sys_root = "/sys"`).
    pub fn with_roots(dev_root: impl Into<PathBuf>, sys_root: impl Into<PathBuf>) -> Self {
        Self {
            dev_root: dev_root.into(),
            sys_root: sys_root.into(),
        }
    }

    /// Join an absolute `/dev`-style path under `dev_root` (a no-op for `"/"`).
    fn rooted(&self, abs: &str) -> PathBuf {
        self.dev_root.join(abs.trim_start_matches('/'))
    }

    // -- input → identity (add time) ---------------------------------------

    /// Resolve operator input to a canonical identity + current path + echo
    /// (§12). Capture forms (a raw `/dev` path, a bare serial number) require
    /// the device present; identity forms (`usb:`/`by-path:`/`raw:`) never do.
    pub fn resolve_input(&self, input: &str) -> Result<Resolved, ResolveError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason: "empty device string".into(),
            });
        }

        if let Some(rest) = input.strip_prefix("usb:") {
            return self.resolve_usb_identity(input, rest);
        }
        if let Some(rest) = input.strip_prefix("by-path:") {
            return self.resolve_bypath_identity(input, rest);
        }
        if let Some(rest) = input.strip_prefix("raw:") {
            return self.resolve_raw_identity(input, rest);
        }
        if input.starts_with('/') {
            return self.capture_from_path(input);
        }
        // A bare token is a serial number to capture from a present adapter.
        self.capture_from_serial(input)
    }

    /// Validate a `usb:` identity and locate its current path (absent is legal).
    fn resolve_usb_identity(&self, input: &str, rest: &str) -> Result<Resolved, ResolveError> {
        // `usb:vid:pid:serial:iface` — four `:`-separated fields after the scheme.
        let fields: Vec<&str> = rest.split(':').collect();
        if fields.len() != 4 {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason: "expected usb:<vid>:<pid>:<serial>:<iface>".into(),
            });
        }
        // A structurally meaningless identity — any empty *or whitespace-only*
        // field (`usb::::`, `usb:0403:6001::00`, `usb:0403:6001: :00`) — is rejected
        // at add time rather than stored and dumped as a canonical `device` (§11).
        // An absent serial/interface is spelled with the `-` marker, never empty and
        // never blank; a blank field would never match a real sysfs identity, so it
        // is malformed here for the same reason the empty form is (§12, §15.27).
        if fields.iter().any(|f| f.trim().is_empty()) {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason:
                    "usb identity fields must be non-empty (use - for an absent serial/interface)"
                        .into(),
            });
        }
        // Prefer a live sysfs description when the device is present; otherwise
        // describe from the identity fields alone.
        let (path, description) = match self.find_usb(input) {
            Some((dev_path, info)) => (Some(dev_path), info.description),
            None => (None, describe_usb_identity(rest)),
        };
        Ok(Resolved {
            identity: input.to_owned(),
            path,
            kind: DeviceKind::Usb,
            description,
            warning: None,
        })
    }

    fn resolve_bypath_identity(&self, input: &str, rest: &str) -> Result<Resolved, ResolveError> {
        if rest.is_empty() {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason: "empty by-path port".into(),
            });
        }
        let path = self.bypath_lookup(rest);
        Ok(Resolved {
            identity: input.to_owned(),
            path,
            kind: DeviceKind::ByPath,
            description: format!("topology port {rest}"),
            warning: Some(BYPATH_WARNING.into()),
        })
    }

    fn resolve_raw_identity(&self, input: &str, rest: &str) -> Result<Resolved, ResolveError> {
        // An empty (or all-slash) path is malformed — `rooted("")` would join to
        // the dev-root directory itself and report it "present" (§11 rejects
        // ill-formed resolver input up front), so reject it like the other forms.
        if rest.trim_start_matches('/').is_empty() {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason: "empty raw path".into(),
            });
        }
        let rooted = self.rooted(rest);
        Ok(Resolved {
            identity: input.to_owned(),
            path: rooted.exists().then_some(rooted),
            kind: DeviceKind::Raw,
            description: format!("raw path {rest}"),
            warning: Some(RAW_WARNING.into()),
        })
    }

    /// Capture an identity from a present raw `/dev` path: usb → by-path → raw.
    fn capture_from_path(&self, input: &str) -> Result<Resolved, ResolveError> {
        // An all-slash / empty-after-trim path is malformed — `rooted("/")` joins
        // to the dev-root directory itself, which always exists and would be
        // captured as `raw:/` bound to a directory (§11 rejects ill-formed
        // resolver input up front), so reject it as the `raw:` form does.
        if input.trim_start_matches('/').is_empty() {
            return Err(ResolveError::Malformed {
                input: input.to_owned(),
                reason: "empty path".into(),
            });
        }
        let rooted = self.rooted(input);
        if !rooted.exists() {
            return Err(ResolveError::NotPresent {
                input: input.to_owned(),
            });
        }
        let dev_name = rooted
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(self.capture_for_dev(&dev_name, rooted, input))
    }

    /// Capture an identity from a present adapter whose serial matches.
    fn capture_from_serial(&self, serial: &str) -> Result<Resolved, ResolveError> {
        for a in self.discover_adapters() {
            let matches = a
                .identity
                .as_deref()
                .and_then(usb_serial_field)
                .is_some_and(|s| s == serial);
            if matches {
                let dev_name = a
                    .dev_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let raw = format!("/dev/{dev_name}");
                return Ok(self.capture_for_dev(&dev_name, a.dev_path, &raw));
            }
        }
        Err(ResolveError::NotPresent {
            input: serial.to_owned(),
        })
    }

    /// The best identity for a present device node, applying the §12 fallback
    /// chain: a *unique* usb identity, else by-path, else the raw path. A serial
    /// that is absent (`-`) or duplicated across adapters cannot pin one device, so
    /// it degrades to by-path — the wrong-device-adoption guard (§15.10).
    fn capture_for_dev(&self, dev_name: &str, rooted: PathBuf, raw: &str) -> Resolved {
        if let Some(info) = self.sysfs_lookup(dev_name) {
            let absent = usb_serial_field(&info.identity) == Some("-");
            if !absent && !self.usb_identity_ambiguous(&info.identity) {
                return Resolved {
                    identity: info.identity,
                    path: Some(rooted),
                    kind: DeviceKind::Usb,
                    description: info.description,
                    warning: None,
                };
            }
        }
        if let Some(port) = self.bypath_of(dev_name) {
            return Resolved {
                identity: format!("by-path:{port}"),
                path: Some(rooted),
                kind: DeviceKind::ByPath,
                description: format!("topology port {port} ({dev_name})"),
                warning: Some(BYPATH_WARNING.into()),
            };
        }
        Resolved {
            identity: format!("raw:{raw}"),
            path: Some(rooted),
            kind: DeviceKind::Raw,
            description: format!("raw path {raw}"),
            warning: Some(RAW_WARNING.into()),
        }
    }

    /// Whether more than one present adapter reports this exact usb identity — a
    /// duplicated serial number, so the identity does not pin one device (§12).
    fn usb_identity_ambiguous(&self, identity: &str) -> bool {
        self.discover_adapters()
            .iter()
            .filter(|a| a.identity.as_deref() == Some(identity))
            .count()
            > 1
    }

    // -- identity → current path (open + recheck) --------------------------

    /// Resolve a stored `device` string to its current `/dev` path, or `None`
    /// when absent. For `usb:` and `by-path:` identities this is squatter-safe
    /// (only a device whose identity matches is returned); a raw `/dev` path or
    /// `raw:` identity resolves to the path literally (the documented instability
    /// of the escape hatch, §12). Never fails — absence is `None`.
    pub fn resolve_current_path(&self, device: &str) -> Option<PathBuf> {
        let device = device.trim();
        if device.starts_with("usb:") {
            self.find_usb(device).map(|(p, _)| p)
        } else if let Some(rest) = device.strip_prefix("by-path:") {
            self.bypath_lookup(rest)
        } else if let Some(rest) = device.strip_prefix("raw:") {
            let p = self.rooted(rest);
            p.exists().then_some(p)
        } else if device.starts_with('/') {
            let p = self.rooted(device);
            p.exists().then_some(p)
        } else {
            // A bare serial number left unresolved (uncaptured); best-effort.
            self.discover_adapters().into_iter().find_map(|a| {
                let matches = a
                    .identity
                    .as_deref()
                    .and_then(usb_serial_field)
                    .is_some_and(|s| s == device);
                matches.then_some(a.dev_path)
            })
        }
    }

    // -- Linux by-id / by-path / sysfs backend -----------------------------

    /// Enumerate `/dev/serial/by-id` and derive each entry's identity. Shared
    /// with the doctor's P4 probe (§12). `dev_path` is rooted under `dev_root`.
    pub fn discover_adapters(&self) -> Vec<Adapter> {
        let by_id = self.dev_root.join("dev/serial/by-id");
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&by_id) else {
            return out;
        };
        for entry in entries.flatten() {
            let by_id_name = entry.file_name().to_string_lossy().into_owned();
            let Ok(target) = std::fs::read_link(entry.path()) else {
                continue;
            };
            let dev_name = target
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let identity = self.sysfs_lookup(&dev_name).map(|i| i.identity);
            out.push(Adapter {
                by_id_name,
                dev_path: self.dev_root.join("dev").join(&dev_name),
                identity,
            });
        }
        out
    }

    /// The `(path, sysfs info)` of the adapter whose canonical identity equals
    /// `identity`, if present. This is the squatter-safe usb resolution.
    fn find_usb(&self, identity: &str) -> Option<(PathBuf, UsbInfo)> {
        let by_id = self.dev_root.join("dev/serial/by-id");
        let entries = std::fs::read_dir(&by_id).ok()?;
        for entry in entries.flatten() {
            let Ok(target) = std::fs::read_link(entry.path()) else {
                continue;
            };
            // Skip an odd entry (a target with no final component, e.g. `../..`)
            // rather than aborting the whole scan — a stray link must not hide a
            // present device sorting after it (§15.8: a present device is not absent).
            let Some(dev_name) = target.file_name().map(|s| s.to_string_lossy().into_owned())
            else {
                continue;
            };
            if let Some(info) = self.sysfs_lookup(&dev_name)
                && info.identity == identity
            {
                return Some((self.dev_root.join("dev").join(&dev_name), info));
            }
        }
        None
    }

    /// Read `/dev/serial/by-path/<port>` to its current device path, if present.
    fn bypath_lookup(&self, port: &str) -> Option<PathBuf> {
        let link = self.dev_root.join("dev/serial/by-path").join(port);
        let target = std::fs::read_link(&link).ok()?;
        let dev_name = target.file_name()?.to_string_lossy().into_owned();
        let p = self.dev_root.join("dev").join(dev_name);
        p.exists().then_some(p)
    }

    /// The `by-path` port name currently covering `dev_name`, if any.
    fn bypath_of(&self, dev_name: &str) -> Option<String> {
        let by_path = self.dev_root.join("dev/serial/by-path");
        let entries = std::fs::read_dir(&by_path).ok()?;
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path())
                && target.file_name().and_then(|s| s.to_str()) == Some(dev_name)
            {
                return Some(entry.file_name().to_string_lossy().into_owned());
            }
        }
        None
    }

    /// The canonical usb identity + description for a tty device name, via the
    /// dependency-free sysfs ancestor walk (§12): the nearest `bInterfaceNumber`
    /// is the interface; the first ancestor with `idVendor` is the USB device —
    /// stop there or the walk binds the root hub.
    fn sysfs_lookup(&self, dev_name: &str) -> Option<UsbInfo> {
        let device_link = self
            .sys_root
            .join("class/tty")
            .join(dev_name)
            .join("device");
        let start = std::fs::canonicalize(&device_link).ok()?;
        // Canonicalize the guard root too, so `starts_with` compares like paths
        // (fixture roots under `/tmp` are already real on Linux, but be exact).
        let guard = std::fs::canonicalize(&self.sys_root).unwrap_or_else(|_| self.sys_root.clone());
        let mut interface = None;
        let mut cur: &Path = &start;
        for _ in 0..12 {
            if interface.is_none() {
                interface = read_trimmed(&cur.join("bInterfaceNumber"));
            }
            if cur.join("idVendor").exists() {
                let vid = read_trimmed(&cur.join("idVendor"))?;
                let pid = read_trimmed(&cur.join("idProduct"))?;
                let serial = read_trimmed(&cur.join("serial"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "-".into());
                let iface = interface.unwrap_or_else(|| "-".into());
                let identity = format!("usb:{vid}:{pid}:{serial}:{iface}");
                let manufacturer = read_trimmed(&cur.join("manufacturer"));
                let product = read_trimmed(&cur.join("product"));
                let description = describe(manufacturer, product, &serial, &iface, &vid, &pid);
                return Some(UsbInfo {
                    identity,
                    description,
                });
            }
            match cur.parent() {
                Some(parent) if parent != cur && parent.starts_with(&guard) => cur = parent,
                _ => break,
            }
        }
        None
    }
}

const BYPATH_WARNING: &str = "bound by topology (by-path): this identity follows whatever adapter occupies the physical port, not a specific device (§12)";
const RAW_WARNING: &str = "bound by raw path: no device identity — a replugged or different adapter on this path is adopted blindly, and the path is not stable across reboots (§12)";

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}

/// The serial field of a `usb:vid:pid:serial:iface` identity, or `None`.
fn usb_serial_field(identity: &str) -> Option<&str> {
    let rest = identity.strip_prefix("usb:")?;
    rest.split(':').nth(2)
}

/// A human echo from live sysfs strings, e.g. "FTDI FT232R USB UART, serial
/// A6008isP, interface 00"; falls back to `vid:pid` when strings are absent.
fn describe(
    manufacturer: Option<String>,
    product: Option<String>,
    serial: &str,
    iface: &str,
    vid: &str,
    pid: &str,
) -> String {
    let mut head = match (manufacturer, product) {
        (Some(m), Some(p)) => format!("{m} {p}"),
        (Some(m), None) => m,
        (None, Some(p)) => p,
        (None, None) => format!("USB {vid}:{pid}"),
    };
    head = head.trim().to_owned();
    if serial != "-" {
        head.push_str(&format!(", serial {serial}"));
    }
    if iface != "-" {
        head.push_str(&format!(", interface {iface}"));
    }
    head
}

/// Describe a `usb:` identity from its fields alone (device absent).
fn describe_usb_identity(fields: &str) -> String {
    let parts: Vec<&str> = fields.split(':').collect();
    match parts.as_slice() {
        [vid, pid, serial, iface] => {
            let mut s = format!("USB {vid}:{pid}");
            if *serial != "-" {
                s.push_str(&format!(", serial {serial}"));
            }
            if *iface != "-" {
                s.push_str(&format!(", interface {iface}"));
            }
            s
        }
        _ => format!("usb:{fields}"),
    }
}

// Re-export the scheme helper for callers that classify a stored identity.
impl DeviceKind {
    /// Classify a stored `device` string by its scheme prefix. A bare path is
    /// [`DeviceKind::Raw`]; a bare token (uncaptured serial) is also `Raw`.
    pub fn of(device: &str) -> DeviceKind {
        let device = device.trim();
        if device.starts_with("usb:") {
            DeviceKind::Usb
        } else if device.starts_with("by-path:") {
            DeviceKind::ByPath
        } else {
            DeviceKind::Raw
        }
    }

    /// The scheme label (`"usb"`, `"by-path"`, `"raw"`) for state reporting.
    pub fn label(self) -> &'static str {
        self.scheme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A self-cleaning fixture tree under the system temp dir (no `tempfile`
    /// dependency — the licensing gate stays minimal, §13).
    struct TmpTree(PathBuf);

    impl TmpTree {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("snx-resolver-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TmpTree(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TmpTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(p: &Path, contents: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    /// Build a faithful by-id + sysfs fixture for one USB tty device. `serial`
    /// / `iface` may be `None`/`"-"` absent. Returns the rooted `/dev` device
    /// path (a plain file standing in for the tty node).
    #[allow(clippy::too_many_arguments)]
    fn add_usb_device(
        root: &Path,
        usbdir: &str,
        dev_name: &str,
        by_id_name: &str,
        vid: &str,
        pid: &str,
        serial: Option<&str>,
        iface: &str,
        strings: Option<(&str, &str)>,
    ) -> PathBuf {
        // The device node.
        let dev = root.join("dev").join(dev_name);
        write(&dev, "");
        // by-id/<name> -> ../../<dev_name>
        let by_id = root.join("dev/serial/by-id");
        std::fs::create_dir_all(&by_id).unwrap();
        std::os::unix::fs::symlink(format!("../../{dev_name}"), by_id.join(by_id_name)).unwrap();
        // sysfs: devices/<usbdir>/idVendor.. and <usbdir>/<iface-dir>/bInterfaceNumber.
        let usbdev = root.join("sys/bus/usb/devices").join(usbdir);
        write(&usbdev.join("idVendor"), vid);
        write(&usbdev.join("idProduct"), pid);
        if let Some(s) = serial {
            write(&usbdev.join("serial"), s);
        }
        if let Some((manu, prod)) = strings {
            write(&usbdev.join("manufacturer"), manu);
            write(&usbdev.join("product"), prod);
        }
        let iface_dir = usbdev.join(format!("{usbdir}:1.0"));
        write(&iface_dir.join("bInterfaceNumber"), iface);
        // class/tty/<dev>/device -> the interface dir (relative).
        let class = root.join("sys/class/tty").join(dev_name);
        std::fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink(
            format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.0"),
            class.join("device"),
        )
        .unwrap();
        dev
    }

    #[test]
    fn usb_capture_from_path_and_resolve_back() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0",
            "0403",
            "6001",
            Some("A6008isP"),
            "00",
            Some(("FTDI", "FT232R USB UART")),
        );
        // Add by raw path (present) → captures the usb identity + description.
        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.identity, "usb:0403:6001:A6008isP:00");
        assert_eq!(got.kind, DeviceKind::Usb);
        assert!(got.warning.is_none());
        assert!(got.description.contains("FTDI FT232R"));
        assert!(got.description.contains("A6008isP"));
        assert_eq!(got.path, Some(t.path().join("dev/ttyUSB0")));
        // identity → current path resolves back to the same device.
        assert_eq!(
            r.resolve_current_path("usb:0403:6001:A6008isP:00"),
            Some(t.path().join("dev/ttyUSB0"))
        );
    }

    #[test]
    fn identity_form_absent_is_ok_but_path_none() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        // No device present; a usb identity still resolves (comes up waiting).
        let got = r.resolve_input("usb:0403:6001:XYZ:00").unwrap();
        assert_eq!(got.identity, "usb:0403:6001:XYZ:00");
        assert_eq!(got.path, None);
        assert!(got.warning.is_none());
        assert_eq!(r.resolve_current_path("usb:0403:6001:XYZ:00"), None);
    }

    #[test]
    fn raw_path_add_absent_fails() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        assert_eq!(
            r.resolve_input("/dev/ttyUSB9"),
            Err(ResolveError::NotPresent {
                input: "/dev/ttyUSB9".into()
            })
        );
    }

    #[test]
    fn no_serial_clone_degrades_to_by_path_with_warning() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB1",
            "usb-1a86_USB_Serial-if00-port0",
            "1a86",
            "7523",
            None, // no serial number
            "00",
            None,
        );
        // by-path tree covering the same device node.
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB1",
            by_path.join("pci-0000:00:14.0-usb-0:1:1.0-port0"),
        )
        .unwrap();

        // A serial-less adapter degrades to by-path (an ambiguous `usb:…:-:…`
        // would be shared by identical clones, §12), carrying the instability
        // warning, and resolves back through the by-path tree.
        let got = r.resolve_input("/dev/ttyUSB1").unwrap();
        assert_eq!(got.identity, "by-path:pci-0000:00:14.0-usb-0:1:1.0-port0");
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert!(got.warning.is_some());
        assert_eq!(
            r.resolve_current_path(&got.identity),
            Some(t.path().join("dev/ttyUSB1"))
        );
    }

    #[test]
    fn squatter_on_same_path_is_not_adopted() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-FTDI_A6008isP-if00",
            "0403",
            "6001",
            Some("A6008isP"),
            "00",
            None,
        );
        let ours = "usb:0403:6001:A6008isP:00";
        assert!(r.resolve_current_path(ours).is_some());
        // Replace the device behind the same by-id/dev name with a different
        // identity (a squatter): resolution for OUR identity now fails.
        std::fs::write(t.path().join("sys/bus/usb/devices/1-1/serial"), "DIFFERENT").unwrap();
        assert_eq!(r.resolve_current_path(ours), None);
        // But the squatter's own identity does resolve.
        assert!(
            r.resolve_current_path("usb:0403:6001:DIFFERENT:00")
                .is_some()
        );
    }

    #[test]
    fn raw_identity_resolves_literally() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        write(&t.path().join("dev/ttyS9"), "");
        let got = r.resolve_input("raw:/dev/ttyS9").unwrap();
        assert_eq!(got.identity, "raw:/dev/ttyS9");
        assert_eq!(got.kind, DeviceKind::Raw);
        assert!(got.warning.is_some());
        assert_eq!(
            r.resolve_current_path("raw:/dev/ttyS9"),
            Some(t.path().join("dev/ttyS9"))
        );
    }

    #[test]
    fn empty_input_is_malformed() {
        let r = Resolver::new("/");
        assert!(matches!(
            r.resolve_input("  "),
            Err(ResolveError::Malformed { .. })
        ));
        assert!(matches!(
            r.resolve_input("usb:0403:6001"),
            Err(ResolveError::Malformed { .. })
        ));
        // An empty raw path must be rejected, not resolved to the dev-root dir.
        assert!(matches!(
            r.resolve_input("raw:"),
            Err(ResolveError::Malformed { .. })
        ));
        assert!(matches!(
            r.resolve_input("raw:/"),
            Err(ResolveError::Malformed { .. })
        ));
        // A bare all-slash path must be rejected, not captured as `raw:/` bound
        // to the dev-root directory.
        assert!(matches!(
            r.resolve_input("/"),
            Err(ResolveError::Malformed { .. })
        ));
        assert!(matches!(
            r.resolve_input("//"),
            Err(ResolveError::Malformed { .. })
        ));
    }

    #[test]
    fn duplicated_serial_degrades_to_by_path() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        // Two cheap clones hard-coding the SAME serial on different physical ports.
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Clone_DUP-a",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        add_usb_device(
            t.path(),
            "2-1",
            "ttyUSB1",
            "usb-Clone_DUP-b",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        // A by-path entry covering ttyUSB0 (the topology fallback).
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB0", by_path.join("pci-0:1:1.0-port0")).unwrap();

        // Adding by raw path must NOT capture the ambiguous usb:1a86:7523:DUP:00
        // (which would bind either clone) — it degrades to by-path (§12/§15.10).
        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert_eq!(got.identity, "by-path:pci-0:1:1.0-port0");
        assert!(got.warning.is_some());
    }

    #[test]
    fn device_kind_classifies_stored_strings() {
        assert_eq!(DeviceKind::of("usb:0403:6001:X:00"), DeviceKind::Usb);
        assert_eq!(DeviceKind::of("by-path:pci-0000"), DeviceKind::ByPath);
        assert_eq!(DeviceKind::of("raw:/dev/ttyUSB0"), DeviceKind::Raw);
        assert_eq!(DeviceKind::of("/dev/ttyUSB0"), DeviceKind::Raw);
    }

    #[test]
    fn empty_serial_string_degrades_to_by_path() {
        // A cheap adapter exposes an EMPTY iSerialNumber descriptor: the sysfs
        // `serial` file exists but is blank. It must be treated as absent
        // (§12/§15.10) — a concrete `usb:vid:pid::iface` would match a second
        // identical adapter on another port and reopen the wrong device.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-1a86_USB_Serial-if00-port0",
            "1a86",
            "7523",
            Some(""), // present-but-empty serial string
            "00",
            None,
        );
        // by-path tree covering the same device node.
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB0",
            by_path.join("pci-0000:00:14.0-usb-0:1:1.0-port0"),
        )
        .unwrap();

        // Empty serial → absent marker → degrades to by-path with the warning.
        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert_eq!(got.identity, "by-path:pci-0000:00:14.0-usb-0:1:1.0-port0");
        assert!(got.warning.is_some());
    }

    #[test]
    fn usb_identity_empty_field_is_malformed() {
        // A usb: identity with the right field COUNT but an empty field is
        // structurally meaningless and must be rejected at add time, not stored
        // and dumped as a canonical device (§11). An absent serial/interface is
        // spelled `-`, never empty.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        for input in [
            "usb::::",            // all empty
            "usb::6001:S:00",     // empty vid
            "usb:0403::S:00",     // empty pid
            "usb:0403:6001::00",  // empty serial
            "usb:0403:6001:S:",   // empty iface
            "usb:0403:6001: :00", // whitespace-only serial (§12, §15.27)
            "usb: :6001:S:00",    // whitespace-only vid
        ] {
            assert!(
                matches!(r.resolve_input(input), Err(ResolveError::Malformed { .. })),
                "expected {input:?} to be malformed"
            );
        }
        // The canonical absent-serial/iface form (with `-`) is still accepted
        // (device absent → path None, no by-id tree in the fixture).
        assert!(r.resolve_input("usb:0403:6001:-:-").is_ok());
    }
}
