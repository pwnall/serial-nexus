//! Device identity resolution (§12) — the dependency-free (no libudev, §15.10)
//! translation between operator input, the canonical identity stored in
//! configuration, and the current `/dev` path that is observed state.
//!
//! This is the one module in `serial-nexus-core` that touches the filesystem: the
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
//!   falls out of resolution by construction — and only when *exactly one* device
//!   answers to it, so an identity two clones share binds nothing rather than
//!   whichever one sorts first (§15.10).
//!
//! **The two directions read the same source, and that is load-bearing.** They
//! used not to: capture walked sysfs while resolution, the duplicate-serial
//! guard and [`Resolver::enumerate_ports`] read only `/dev/serial/by-id`. Every
//! defect that follows from the split is the same defect — the resolver minting
//! an identity it cannot honour, or counting link *names* where the hazard is
//! duplicate *devices* (review 32 RES-1/RES-2). `/dev/serial/by-id` is now a
//! fast path over [`Resolver::sysfs_usb_devices`], never the only path: a
//! container handed a bare `--device=/dev/ttyUSB0`, or an image without udev's
//! `60-serial.rules`, has `/sys` and no by-id tree at all, and must still
//! resolve. The fallback is still an *exact* identity match, so squatter
//! refusal is unchanged.
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
/// yields (`None` → by-path fallback). Shared with `serial-nexus-doctor`'s P4 probe.
#[derive(Debug, Clone)]
pub struct Adapter {
    pub by_id_name: String,
    pub dev_path: PathBuf,
    pub identity: Option<String>,
}

/// One serial device the resolver can *see*, for the `ports` verb (§12, §15.35):
/// the identity an operator would put in a `device` field, where it currently
/// lives, and what it is.
///
/// Enumeration is strictly **passive** — every field comes from a readlink, a
/// directory listing, or a sysfs read. Nothing here calls `open(2)`, because
/// opening a USB-serial adapter to look at it asserts DTR and resets the board on
/// exactly the hardware people care about (§15.35). That is the whole reason this
/// is a separate face on the resolver rather than a probe in `serial-nexus-doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortCandidate {
    /// The canonical identity to store in configuration, in §12 preference
    /// order: `usb:…` when the sysfs walk yields one, else `by-path:…`, else the
    /// `raw:…` escape hatch.
    pub identity: String,
    pub kind: DeviceKind,
    /// The `/dev` path the device currently occupies. Always present — a
    /// candidate is by definition something the resolver just saw.
    pub path: PathBuf,
    /// Human echo (§12), so an operator recognizes the adapter before binding it.
    pub description: String,
    /// The `/dev/serial/by-id` entry name, when the device has one.
    pub by_id: Option<String>,
    /// The documented instability warning carried by the degraded identity forms
    /// (§12) — `None` for a `usb:` identity.
    pub warning: Option<String>,
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
        // The §15.10 ambiguity guard runs on **this** path too, not only on capture.
        // Capture degrades a duplicated serial to by-path, so a freshly captured
        // identity is never ambiguous — but three doors reach here carrying one that
        // is: a configuration persisted by an older daemon (`dump` wrote the
        // ambiguous `usb:` string), a hand-typed identity, and — the door no amount
        // of history fixes — an identity captured while only one clone was plugged
        // in, whose twin appears later. Left unguarded, `load`, daemon startup from
        // the state file, and `add-node device = "usb:…"` all accepted the identity
        // with no warning and bound whichever clone sorted first, so two nodes
        // carrying it both drove one adapter and the other was unreachable by any
        // node (review 32 RES-1).
        //
        // The identity itself is still *accepted*: §12's asymmetry is that
        // identity-form input never requires the device present, which is why `dump`
        // emits identities and why configurations survive cold starts. What is
        // refused is the **binding** — see [`Self::find_usb`] for why declining is
        // the arm §15.10 supports — and the operator hears about it here, in the
        // `warning` `add-node` echoes, naming both devices and the by-path fix.
        let ambiguous = self.usb_identity_matches(input);
        if ambiguous.len() > 1 {
            return Ok(Resolved {
                identity: input.to_owned(),
                path: None,
                kind: DeviceKind::Usb,
                description: describe_usb_identity(rest),
                warning: Some(ambiguous_warning(&ambiguous)),
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
        // Follow symlinks *before* deriving the device name. `/dev/serial/by-id/…`
        // and `/dev/serial/by-path/…` are the two most idiomatic spellings an
        // operator has, and a link name is not a sysfs device name: taking
        // `file_name()` of the literal input handed `sysfs_lookup` the string
        // `usb-FTDI_…-if00-port0`, which misses, so the single most canonical input
        // there is degraded all the way to `raw:` — carrying the "not stable across
        // reboots" warning, which is precisely backwards for a by-id path (RES-3).
        let (rooted, raw) = self.canonical_dev(rooted, input);
        let dev_name = rooted
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(self.capture_for_dev(&dev_name, rooted, &raw))
    }

    /// Follow symlinks on an operator-supplied `/dev` path, staying inside
    /// `dev_root`, and return the rooted canonical path together with its
    /// `/dev`-relative spelling. The spelling is what a `raw:` capture stores, so a
    /// genuinely identity-less device reached through a link records the device node
    /// rather than the link — the link may be gone next boot while the node is not.
    ///
    /// Two paths deliberately keep the operator's own spelling: one that does not
    /// canonicalize (a race with an unplug), and one whose target escapes
    /// `dev_root`. The resolver never binds a device outside the tree it was
    /// pointed at — that root is a test seam *and* the daemon's `--dev-root`.
    fn canonical_dev(&self, rooted: PathBuf, input: &str) -> (PathBuf, String) {
        let Ok(canon) = std::fs::canonicalize(&rooted) else {
            return (rooted, input.to_owned());
        };
        // Canonicalize the root too, so `strip_prefix` compares like paths (a
        // fixture root under a symlinked `/tmp` is the reachable case).
        let root = std::fs::canonicalize(&self.dev_root).unwrap_or_else(|_| self.dev_root.clone());
        let Ok(rel) = canon.strip_prefix(&root) else {
            return (rooted, input.to_owned());
        };
        if rel.as_os_str().is_empty() {
            return (rooted, input.to_owned());
        }
        let spelling = format!("/{}", rel.display());
        // Re-root the *spelling* rather than returning `canon`: every other path
        // this module yields is `dev_root`-joined and uncanonicalized, and a
        // resolved path that disagreed with `resolve_current_path`'s spelling would
        // read as two different devices to `ports`' bound-status comparison.
        (self.rooted(&spelling), spelling)
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
    ///
    /// **Counted over devices, never over `/dev/serial/by-id` entries.** Counting
    /// links could not fire for the exact hazard §15.10 names: udev derives the
    /// by-id name from `ID_SERIAL` + `ID_USB_INTERFACE_NUM` + port — every
    /// component a function of the same fields that make the identity ambiguous —
    /// so two clones sharing a serial number collide on *one* link name and udev
    /// publishes exactly one symlink for it. The count could never exceed 1, the
    /// ambiguous identity was captured with no warning, and both clones then
    /// resolved to whichever one owned the surviving link (RES-1).
    fn usb_identity_ambiguous(&self, identity: &str) -> bool {
        self.usb_identity_matches(identity).len() > 1
    }

    /// Every device *present right now* whose sysfs identity is exactly `identity`,
    /// sorted by device name. Zero means the identity names nothing here, one is the
    /// healthy case, and two or more is §15.10's duplicate-serial hazard.
    ///
    /// One helper, because both §12 directions have to agree about what "ambiguous"
    /// means: capture asks it to decide whether to degrade to by-path, and
    /// resolution asks it to decide whether the identity pins a device at all. When
    /// only capture consulted it, resolution happily bound the first match to an
    /// identity capture would have refused to mint (RES-1).
    fn usb_identity_matches(&self, identity: &str) -> Vec<(PathBuf, UsbInfo)> {
        self.sysfs_usb_devices()
            .into_iter()
            .filter(|(_, info)| info.identity == identity)
            .collect()
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

    /// Every serial device the resolver can see right now, as the identity an
    /// operator would bind it by (§12 preference order) — the `ports` verb's
    /// enumeration face (§15.35).
    ///
    /// Four passive sources, unioned and deduplicated by device node:
    /// `/dev/serial/by-id` (the canonical USB face), the `<sys_root>/class/tty`
    /// listing (which covers an adapter udev named *nowhere* — a container's bare
    /// `--device=/dev/ttyUSB0`, an image without `60-serial.rules`),
    /// `/dev/serial/by-path` (which still covers an adapter whose serial number is
    /// absent, so it has no by-id entry the resolver can use), and a scan of
    /// `<dev_root>/dev` for `cu.*` callout devices — the BSD/macOS face, where no
    /// by-id tree exists and a raw `cu.*` path is the interim identity (§12). The
    /// `cu.*` scan is not `cfg`-gated: the prefix simply matches nothing on Linux,
    /// and one code path keeps the macOS arm reachable from a Linux fixture instead
    /// of shipping untested.
    ///
    /// Each candidate's identity comes from the *same* [`Self::capture_for_dev`]
    /// fallback chain `add-node` uses, so what `ports` shows is exactly what
    /// binding that path would store — not a second opinion about it.
    ///
    /// **Passive by construction.** Readlinks, directory listings and sysfs reads
    /// only: no `open(2)`, because opening a USB adapter asserts DTR and resets
    /// the attached board (§15.35). Sorted by path, so two calls read the same.
    pub fn enumerate_ports(&self) -> Vec<PortCandidate> {
        // dev node name → its by-id entry name, when it has one.
        let mut seen: std::collections::BTreeMap<String, Option<String>> =
            std::collections::BTreeMap::new();

        for adapter in self.discover_adapters() {
            if let Some(name) = adapter.dev_path.file_name() {
                seen.insert(
                    name.to_string_lossy().into_owned(),
                    Some(adapter.by_id_name),
                );
            }
        }
        // sysfs itself covers the adapters udev never named at all: with no
        // `60-serial.rules` there is neither a by-id nor a by-path entry, and
        // `ports` answered `[]` on a tree where the adapter is present in sysfs and
        // at `/dev/ttyUSB0` — the enumeration face pointing away from the one fix
        // (RES-2). This is the same source capture reads, so what `ports` shows is
        // still exactly what binding the path would store.
        for (dev_path, _) in self.sysfs_usb_devices() {
            if let Some(name) = dev_path.file_name() {
                seen.entry(name.to_string_lossy().into_owned())
                    .or_insert(None);
            }
        }
        // by-path covers the adapters by-id cannot name.
        if let Ok(entries) = std::fs::read_dir(self.dev_root.join("dev/serial/by-path")) {
            for entry in entries.flatten() {
                let Ok(target) = std::fs::read_link(entry.path()) else {
                    continue;
                };
                let Some(dev_name) = target.file_name().map(|s| s.to_string_lossy().into_owned())
                else {
                    continue;
                };
                seen.entry(dev_name).or_insert(None);
            }
        }
        // The BSD/macOS callout devices. `cu.*`, never `tty.*`: the callout node is
        // the one that does not block waiting for carrier detect, and so the one a
        // serial tool opens.
        if let Ok(entries) = std::fs::read_dir(self.dev_root.join("dev")) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("cu.") {
                    seen.entry(name).or_insert(None);
                }
            }
        }

        let mut out: Vec<PortCandidate> = seen
            .into_iter()
            .filter_map(|(dev_name, by_id)| {
                let raw = format!("/dev/{dev_name}");
                let rooted = self.rooted(&raw);
                // A by-id link can outlive its device node; do not offer a path
                // that is no longer there.
                if !rooted.exists() {
                    return None;
                }
                let resolved = self.capture_for_dev(&dev_name, rooted.clone(), &raw);
                Some(PortCandidate {
                    identity: resolved.identity,
                    kind: resolved.kind,
                    path: rooted,
                    description: resolved.description,
                    by_id,
                    warning: resolved.warning,
                })
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Every present USB-serial device sysfs can see, as `(rooted `/dev` path,
    /// identity info)`, sorted by device name — the resolver's **device-level**
    /// source, and the one both §12 directions share (RES-1/RES-2).
    ///
    /// Passive by construction, exactly as [`Self::enumerate_ports`] is: a listing
    /// of `<sys_root>/class/tty`, one `sysfs_lookup` each, and a `stat` for the
    /// `/dev` node. Nothing is opened. A tty whose walk yields no usb identity
    /// (`ttyS0`, the virtual consoles, `ptmx`) is simply absent from the result,
    /// so this stays the *USB adapter* view and not a listing of every tty.
    fn sysfs_usb_devices(&self) -> Vec<(PathBuf, UsbInfo)> {
        let Ok(entries) = std::fs::read_dir(self.sys_root.join("class/tty")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        // `read_dir` order is arbitrary; sort so two calls read the same and a
        // hand-typed ambiguous identity resolves deterministically.
        names.sort();
        names
            .into_iter()
            .filter_map(|name| {
                let dev_path = self.dev_root.join("dev").join(&name);
                // A sysfs entry whose device node is gone is not a *present*
                // adapter — neither bindable nor countable as ambiguity.
                if !dev_path.exists() {
                    return None;
                }
                self.sysfs_lookup(&name).map(|info| (dev_path, info))
            })
            .collect()
    }

    /// The `(path, sysfs info)` of the adapter whose canonical identity equals
    /// `identity`, if present. This is the squatter-safe usb resolution.
    ///
    /// `/dev/serial/by-id` first (one readlink per adapter, the common case), then
    /// the sysfs device listing capture itself reads. Without that second source an
    /// identity minted in an environment with `/sys` and no by-id tree — a
    /// container given `--device=/dev/ttyUSB0`, a busybox-mdev image — could never
    /// be resolved back: `add-node` returned success with a populated
    /// `resolved_path` and the node then waited forever for a device that was
    /// right there (RES-2). The fallback matches the identity *exactly*, so
    /// squatter refusal is preserved by the same construction as the by-id arm.
    ///
    /// **An ambiguous identity binds nothing.** When two or more present devices
    /// answer to it, this declines instead of returning the first — the arm §15.10
    /// states as "wrong-device adoption is impossible by construction", and the only
    /// one available here: `resolve_current_path` returns a bare `Option<PathBuf>`,
    /// so "bind and warn" has nowhere to put the warning and would be indis-
    /// tinguishable, at every open and every faulted-and-wait recheck, from binding
    /// the right device. A node whose identity two adapters answer to therefore
    /// stays `waiting` rather than driving a coin-flip board (§12, review 32 RES-1);
    /// `add-node` names the ambiguity in its `warning`, and `ports` shows both
    /// devices under the by-path identities that do pin them.
    ///
    /// The device listing runs before the by-id readlinks, because a by-id *hit*
    /// says nothing about uniqueness: udev publishes exactly one link per colliding
    /// name, which is the whole of RES-1. That gives up the by-id fast path and
    /// costs one `<sys_root>/class/tty` listing per resolution — measured on this
    /// project's dev box (real `/sys`, ~110 tty entries) at **0.93 ms against
    /// 0.041 ms** for the readlink alone. Paid once per open and once per 1 Hz
    /// faulted-and-wait recheck per waiting node, which is under a tenth of a
    /// percent of a core; nothing here opens a device (§15.35), so the cost is
    /// `stat`s, not DTR.
    fn find_usb(&self, identity: &str) -> Option<(PathBuf, UsbInfo)> {
        let mut matches = self.usb_identity_matches(identity);
        if matches.len() > 1 {
            return None;
        }
        if let Some(found) = self.find_usb_by_id(identity) {
            return Some(found);
        }
        matches.pop()
    }

    /// The `/dev/serial/by-id` half of [`Self::find_usb`]. `None` when the tree is
    /// unreadable or names no device with this identity.
    fn find_usb_by_id(&self, identity: &str) -> Option<(PathBuf, UsbInfo)> {
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
    /// stop there or the walk binds the root hub. The §12 spelling rule is
    /// enforced *at the source*: a blank `serial` or `bInterfaceNumber` becomes
    /// the `-` absent marker, and a blank `idVendor`/`idProduct` yields no
    /// identity at all, so the caller degrades down the fallback chain.
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
                // §12 spelling rule, at the source: a present-but-blank
                // `bInterfaceNumber` normalizes to *absent* exactly as a blank
                // `serial` does (CP-6). Left empty it would mint the retired
                // `usb:vid:pid:serial:` form — malformed at add time, and a
                // stored one only ever comes up waiting (§15.27).
                interface = read_trimmed(&cur.join("bInterfaceNumber")).filter(|s| !s.is_empty());
            }
            if cur.join("idVendor").exists() {
                // vid/pid have no absent spelling — they *are* the identity — so a
                // blank one is not normalized but abandoned: yield no usb identity
                // and let the §12 fallback chain degrade to by-path, rather than
                // mint an unmatchable `usb::pid:…` (CP-6).
                let vid = read_trimmed(&cur.join("idVendor")).filter(|s| !s.is_empty())?;
                let pid = read_trimmed(&cur.join("idProduct")).filter(|s| !s.is_empty())?;
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

/// The warning an ambiguous `usb:` identity carries, naming every device that
/// answers to it (§12/§15.10). It says "nothing is bound" because that is what the
/// resolver does — declining is the only way an `Option<PathBuf>` can express "this
/// identity pins no device" — and it names the fix, since the by-path identities
/// `ports` lists for those same devices *do* pin them one each.
fn ambiguous_warning(matches: &[(PathBuf, UsbInfo)]) -> String {
    let devices: Vec<String> = matches
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect();
    format!(
        "ambiguous usb identity: {} present devices answer to it ({}) — a duplicated serial number pins no single adapter, so nothing is bound and the node stays waiting; bind each adapter by the by-path: identity `ports` lists for it instead (§12)",
        devices.len(),
        devices.join(", ")
    )
}

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
        let dev = add_usb_device_unlinked(root, usbdir, dev_name, vid, pid, serial, iface, strings);
        // by-id/<name> -> ../../<dev_name>
        let by_id = root.join("dev/serial/by-id");
        std::fs::create_dir_all(&by_id).unwrap();
        std::os::unix::fs::symlink(format!("../../{dev_name}"), by_id.join(by_id_name)).unwrap();
        dev
    }

    /// The same fixture with **no** `/dev/serial/by-id` entry. Two shapes need it,
    /// and both are shapes the by-id-only resolver could not see (RES-1/RES-2):
    /// the second of two clones sharing a serial number (udev derives the link name
    /// from those very fields, so it publishes exactly *one* link for the pair), and
    /// every device in an environment with `/sys` and no udev rules at all.
    #[allow(clippy::too_many_arguments)]
    fn add_usb_device_unlinked(
        root: &Path,
        usbdir: &str,
        dev_name: &str,
        vid: &str,
        pid: &str,
        serial: Option<&str>,
        iface: &str,
        strings: Option<(&str, &str)>,
    ) -> PathBuf {
        // The device node.
        let dev = root.join("dev").join(dev_name);
        write(&dev, "");
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
    fn duplicated_serial_degrades_to_by_path_with_the_single_udev_link_real_hardware_gets() {
        // RES-1. The fixture this test used to carry invented *two* by-id names for
        // two identical devices — a tree udev cannot produce, and the only reason
        // the by-id-counting guard passed. udev names a by-id entry
        // `usb-$ID_SERIAL-if$ID_USB_INTERFACE_NUM[-port$n]`, every component of
        // which is a function of the fields that make the identity ambiguous, so two
        // clones sharing a serial collide on ONE name and exactly one symlink
        // exists. Counted over links the guard could never fire for the hazard
        // §15.10 exists for; counted over devices it does.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        // Two cheap clones hard-coding the SAME serial on different physical ports…
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Clone_DUP-if00-port0",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        // …and the loser of the name collision, which udev leaves unlinked.
        add_usb_device_unlinked(
            t.path(),
            "2-1",
            "ttyUSB1",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        assert_eq!(
            std::fs::read_dir(t.path().join("dev/serial/by-id"))
                .unwrap()
                .count(),
            1,
            "the fixture must model udev's ONE link per colliding name"
        );
        // by-path entries covering both device nodes (the topology fallback).
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB0", by_path.join("pci-0:1:1.0-port0")).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB1", by_path.join("pci-0:2:1.0-port0")).unwrap();

        // Adding by raw path must NOT capture the ambiguous usb:1a86:7523:DUP:00
        // (which would bind either clone) — it degrades to by-path (§12/§15.10).
        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert_eq!(got.identity, "by-path:pci-0:1:1.0-port0");
        assert!(got.warning.is_some());

        // And so must the clone udev left unlinked — otherwise the two nodes carry
        // the same identity, both resolve to ttyUSB0, and the second adapter is
        // unreachable by any node while the first answers for both.
        let other = r.resolve_input("/dev/ttyUSB1").unwrap();
        assert_eq!(other.kind, DeviceKind::ByPath);
        assert_eq!(other.identity, "by-path:pci-0:2:1.0-port0");
        assert_ne!(other.identity, got.identity, "one identity per device");
        assert_eq!(
            r.resolve_current_path(&other.identity),
            Some(t.path().join("dev/ttyUSB1"))
        );
        // `ports` degrades identically — it reads the same capture chain, so it
        // cannot advertise a `usb:` identity `add-node` would refuse to mint.
        let ports = r.enumerate_ports();
        let identities: Vec<&str> = ports.iter().map(|p| p.identity.as_str()).collect();
        assert_eq!(
            identities,
            vec!["by-path:pci-0:1:1.0-port0", "by-path:pci-0:2:1.0-port0"],
            "{ports:#?}"
        );
    }

    #[test]
    fn an_ambiguous_usb_identity_binds_nothing_and_says_which_devices_answer() {
        // RES-1's other door. The capture guard degrades a duplicated serial to
        // by-path, so nothing the resolver mints today is ambiguous — but the
        // identity-form direction (`load`, daemon startup from the state file,
        // `add-node device = "usb:…"`) never consulted the guard at all, and that is
        // the direction an *upgrader* takes: a pre-fix daemon captured the ambiguous
        // string and `dump` persisted it. It is also where a clone plugged in later
        // lands, which no history rewrite can reach.
        //
        // Fail-first: before the guard moved onto this path, `resolve_input` of the
        // shared identity returned `path: Some(<...>/dev/ttyUSB0)` with
        // `warning: None`, and `resolve_current_path` returned the same — so two
        // nodes carrying it both drove ttyUSB0 and ttyUSB1 was unreachable.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Clone_DUP-if00-port0",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        add_usb_device_unlinked(
            t.path(),
            "2-1",
            "ttyUSB1",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        let shared = "usb:1a86:7523:DUP:00";

        // Accepted — §12's asymmetry is that identity-form input never requires the
        // device present, so refusing the *add* would break cold starts. What is
        // refused is the binding.
        let got = r.resolve_input(shared).expect("identity form is accepted");
        assert_eq!(got.identity, shared);
        assert_eq!(got.kind, DeviceKind::Usb);
        assert_eq!(
            got.path, None,
            "an identity two adapters answer to must bind neither"
        );
        let warning = got.warning.expect("the ambiguity must be operator-visible");
        for dev in ["ttyUSB0", "ttyUSB1"] {
            assert!(
                warning.contains(dev),
                "the warning must name every device that answers: {warning}"
            );
        }
        assert!(
            warning.contains("by-path"),
            "the warning must name the identity form that does pin one device: {warning}"
        );

        // …and every later open and faulted-and-wait recheck declines too, which is
        // the half `add-node`'s echo cannot cover: `load` never captures, so the
        // warning above is not even emitted on the path an upgrader takes.
        assert_eq!(r.resolve_current_path(shared), None);
    }

    #[test]
    fn an_identity_stops_being_ambiguous_when_the_clone_is_unplugged() {
        // The decline is scoped to the ambiguity, not a blanket refusal of shared
        // serials: unplug one clone and the survivor is pinned again, so a node that
        // came up `waiting` binds on the next reconnect poll with no operator action.
        // Without this the previous test is equally satisfied by a resolver that
        // never resolves a `usb:` identity at all.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Clone_DUP-if00-port0",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        add_usb_device_unlinked(
            t.path(),
            "2-1",
            "ttyUSB1",
            "1a86",
            "7523",
            Some("DUP"),
            "00",
            None,
        );
        let shared = "usb:1a86:7523:DUP:00";
        assert_eq!(r.resolve_current_path(shared), None, "ambiguous while both");

        // Unplug the clone: udev removes the device node and the class/tty entry.
        std::fs::remove_file(t.path().join("dev/ttyUSB1")).unwrap();
        std::fs::remove_dir_all(t.path().join("sys/class/tty/ttyUSB1")).unwrap();
        assert_eq!(
            r.resolve_current_path(shared),
            Some(t.path().join("dev/ttyUSB0")),
            "the surviving adapter is pinned again"
        );
        // And capture agrees: with one device answering, the identity is mintable.
        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.identity, shared);
        assert_eq!(got.kind, DeviceKind::Usb);
        assert!(got.warning.is_none());
    }

    #[test]
    fn one_identity_shared_by_two_ttys_on_one_interface_is_still_ambiguous() {
        // The shape the by-id count *could* see (a multi-port adapter whose ttys
        // share one USB interface, named `…-port0`/`…-port1`) must keep degrading
        // now that the count runs over sysfs devices — the device-level count
        // subsumes the link-level one rather than replacing it (RES-1).
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Multi_MP1-if00-port0",
            "0403",
            "6011",
            Some("MP1"),
            "00",
            None,
        );
        // A second tty hanging off the SAME interface directory: one identity, two
        // device nodes, two by-id links.
        write(&t.path().join("dev/ttyUSB1"), "");
        let class = t.path().join("sys/class/tty/ttyUSB1");
        std::fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink("../../../bus/usb/devices/1-1/1-1:1.0", class.join("device"))
            .unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB1",
            t.path().join("dev/serial/by-id/usb-Multi_MP1-if00-port1"),
        )
        .unwrap();
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB0", by_path.join("pci-0:1:1.0-port0")).unwrap();

        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert!(got.warning.is_some());
        // Resolution declines the shared identity here too, even though this is the
        // one shape the old by-id *link* count could see — the two directions have to
        // answer "does this pin a device?" the same way (RES-1).
        assert_eq!(r.resolve_current_path("usb:0403:6011:MP1:00"), None);
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
    fn blank_interface_number_normalizes_to_the_absent_marker() {
        // A sysfs node exposing a present-but-BLANK `bInterfaceNumber` (CP-6).
        // §12's spelling rule has one shape for every identity field: absent is
        // written `-`, never left empty. Left empty the walk would mint the
        // retired `usb:vid:pid:serial:` form — which `resolve_input` itself
        // rejects as malformed, so the captured identity could never be re-added
        // and a persisted node would come up waiting forever (§15.27).
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
            "  ", // present-but-whitespace-only interface number
            None,
        );

        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.identity, "usb:0403:6001:A6008isP:-");
        assert_eq!(got.kind, DeviceKind::Usb);
        // The captured identity is well-formed input in its own right (it
        // round-trips through dump/load and add), and resolves back to the device.
        assert!(r.resolve_input(&got.identity).is_ok());
        assert_eq!(
            r.resolve_current_path(&got.identity),
            Some(t.path().join("dev/ttyUSB0"))
        );
        // An absent interface is omitted from the operator echo, as an absent
        // serial is.
        assert!(!got.description.contains("interface"));
    }

    #[test]
    fn blank_vendor_id_yields_no_usb_identity_and_degrades_to_by_path() {
        // vid/pid have no absent spelling — they are the identity — so a blank one
        // cannot be normalized, only abandoned: the walk yields nothing and §12's
        // fallback chain takes over (CP-6). Minting `usb::6001:S:00` would store a
        // device string that `resolve_input` rejects as malformed.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-Broken_Adapter-if00",
            "", // blank idVendor
            "6001",
            Some("A6008isP"),
            "00",
            None,
        );
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB0", by_path.join("pci-0:1:1.0-port0")).unwrap();

        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.kind, DeviceKind::ByPath);
        assert_eq!(got.identity, "by-path:pci-0:1:1.0-port0");
        assert!(got.warning.is_some());
        // The discovery listing reports the same absence rather than a half-formed
        // identity (the doctor's P4 probe reads it, §12).
        let adapters = r.discover_adapters();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].identity, None);
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

    // -- enumerate_ports: the `ports` verb's passive face (§12, §15.35) ---------

    #[test]
    fn enumerate_ports_lists_each_source_with_its_identity_form() {
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        // 1. A well-behaved USB adapter: by-id entry + a serial number → `usb:`.
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
        // 2. An adapter with no serial number: by-id cannot name it usefully, so
        //    it reaches the enumeration through by-path and degrades (§12).
        write(&t.path().join("dev/ttyUSB1"), "");
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink("../../ttyUSB1", by_path.join("pci-0:1:1.1-port0")).unwrap();
        // 3. The BSD/macOS callout face: a bare `cu.*` node, no by-id, no sysfs.
        write(&t.path().join("dev/cu.usbserial-FT1234"), "");
        // 4. A `tty.*` twin, which must NOT be listed (it is not the callout node).
        write(&t.path().join("dev/tty.usbserial-FT1234"), "");

        let ports = r.enumerate_ports();
        let by_ident: Vec<&str> = ports.iter().map(|p| p.identity.as_str()).collect();
        assert_eq!(
            by_ident,
            vec![
                "raw:/dev/cu.usbserial-FT1234",
                "usb:0403:6001:A6008isP:00",
                "by-path:pci-0:1:1.1-port0",
            ],
            "sorted by path, one entry per device node: {ports:#?}"
        );

        let usb = ports.iter().find(|p| p.kind == DeviceKind::Usb).unwrap();
        assert_eq!(usb.path, t.path().join("dev/ttyUSB0"));
        assert!(usb.description.contains("FTDI FT232R"));
        assert_eq!(
            usb.by_id.as_deref(),
            Some("usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0")
        );
        assert!(usb.warning.is_none(), "a usb identity is not a fallback");

        let bypath = ports.iter().find(|p| p.kind == DeviceKind::ByPath).unwrap();
        assert_eq!(bypath.path, t.path().join("dev/ttyUSB1"));
        assert!(bypath.by_id.is_none());
        assert!(bypath.warning.is_some(), "the fallback forms warn (§12)");

        let cu = ports.iter().find(|p| p.kind == DeviceKind::Raw).unwrap();
        assert_eq!(cu.path, t.path().join("dev/cu.usbserial-FT1234"));
        assert!(cu.warning.is_some());
    }

    #[test]
    fn enumerate_ports_agrees_with_what_binding_the_path_would_store() {
        // The identity `ports` advertises must be byte-identical to the one
        // `add-node` captures from the same path — otherwise the verb invites an
        // operator to bind something other than what it showed them (§15.35).
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
        for candidate in r.enumerate_ports() {
            let raw = format!("/dev/{}", candidate.path.file_name().unwrap().display());
            let captured = r.resolve_input(&raw).expect("present device captures");
            assert_eq!(captured.identity, candidate.identity);
            assert_eq!(captured.kind, candidate.kind);
            assert_eq!(captured.description, candidate.description);
        }
    }

    #[test]
    fn enumerate_ports_skips_a_by_id_link_whose_device_node_is_gone() {
        // A stale by-id symlink (unplugged mid-scan) must not be offered as a
        // bindable candidate: it has no path, and `ports` reports present devices.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        let dev = add_usb_device(
            t.path(),
            "1-1",
            "ttyUSB0",
            "usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0",
            "0403",
            "6001",
            Some("A6008isP"),
            "00",
            None,
        );
        assert_eq!(r.enumerate_ports().len(), 1);
        std::fs::remove_file(&dev).unwrap();
        assert!(r.enumerate_ports().is_empty(), "stale link is not a port");
    }

    #[test]
    fn enumerate_ports_on_an_empty_tree_is_empty_not_an_error() {
        let t = TmpTree::new();
        assert!(Resolver::new(t.path()).enumerate_ports().is_empty());
    }

    // -- both §12 directions read one source (RES-1/RES-2/RES-3) ---------------

    #[test]
    fn a_usb_identity_captured_without_a_by_id_tree_resolves_back() {
        // RES-2. A container started with `--device=/dev/ttyUSB0` gets a fresh
        // `/dev` holding only that node while `/sys` is mounted; so does any
        // busybox-mdev image with no `60-serial.rules`. Capture read sysfs and
        // resolution read only by-id, so the daemon minted an identity it could
        // never honour: `add-node` returned success with a populated
        // `resolved_path` and the node then waited forever for a device that was
        // right there, with `ports` reporting `[]` to match.
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device_unlinked(
            t.path(),
            "1-1",
            "ttyUSB0",
            "0403",
            "6001",
            Some("UNIQ01"),
            "00",
            Some(("FTDI", "FT232R USB UART")),
        );
        assert!(
            !t.path().join("dev/serial/by-id").exists(),
            "the fixture must have no by-id tree at all"
        );

        let got = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(got.identity, "usb:0403:6001:UNIQ01:00");
        assert_eq!(got.kind, DeviceKind::Usb);
        assert!(got.warning.is_none());
        // The half that used to be impossible: the identity resolves back.
        assert_eq!(
            r.resolve_current_path(&got.identity),
            Some(t.path().join("dev/ttyUSB0"))
        );
        // And the enumeration face shows the device instead of an empty list.
        let ports = r.enumerate_ports();
        assert_eq!(ports.len(), 1, "{ports:#?}");
        assert_eq!(ports[0].identity, "usb:0403:6001:UNIQ01:00");
        assert_eq!(ports[0].path, t.path().join("dev/ttyUSB0"));
        assert_eq!(ports[0].by_id, None, "there is no by-id entry to report");
        assert!(ports[0].warning.is_none());
    }

    #[test]
    fn the_sysfs_fallback_still_refuses_a_squatter() {
        // The fallback is an EXACT identity match, so §12 squatter refusal is
        // preserved by the same construction as the by-id arm — widening the
        // source must not widen what matches (RES-2).
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        add_usb_device_unlinked(
            t.path(),
            "1-1",
            "ttyUSB0",
            "0403",
            "6001",
            Some("OURS"),
            "00",
            None,
        );
        let ours = "usb:0403:6001:OURS:00";
        assert!(r.resolve_current_path(ours).is_some());
        // A different adapter takes over the same `/dev` name.
        std::fs::write(t.path().join("sys/bus/usb/devices/1-1/serial"), "SQUATTER").unwrap();
        assert_eq!(r.resolve_current_path(ours), None, "squatter adopted");
        assert!(
            r.resolve_current_path("usb:0403:6001:SQUATTER:00")
                .is_some()
        );
        // A sysfs entry whose device node is gone is not a present adapter.
        std::fs::remove_file(t.path().join("dev/ttyUSB0")).unwrap();
        assert_eq!(r.resolve_current_path("usb:0403:6001:SQUATTER:00"), None);
    }

    #[test]
    fn by_id_and_by_path_input_capture_what_the_dev_path_captures() {
        // RES-3. `/dev/serial/by-id/usb-…` is the most idiomatic spelling an
        // operator has, and taking `file_name()` of it handed the sysfs walk a
        // *link* name — so the canonical input of all degraded to `raw:`, carrying
        // the "not stable across reboots" warning, which is exactly backwards for
        // the path whose entire purpose is reboot stability.
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
        let by_path = t.path().join("dev/serial/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::os::unix::fs::symlink(
            "../../ttyUSB0",
            by_path.join("pci-0000:00:14.0-usb-0:2:1.0-port0"),
        )
        .unwrap();

        let direct = r.resolve_input("/dev/ttyUSB0").unwrap();
        assert_eq!(direct.identity, "usb:0403:6001:A6008isP:00");
        for input in [
            "/dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A6008isP-if00-port0",
            "/dev/serial/by-path/pci-0000:00:14.0-usb-0:2:1.0-port0",
        ] {
            let got = r.resolve_input(input).unwrap();
            assert_eq!(got, direct, "{input} captured something else");
        }
    }

    #[test]
    fn a_symlinked_path_with_no_identity_captures_the_device_node_it_points_at() {
        // The `raw:` fallback spelling follows the link too: storing the link name
        // would persist a device string that only resolves while that link exists,
        // which is the one thing the raw escape hatch cannot promise (RES-3).
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        write(&t.path().join("dev/ttyFOO"), "");
        std::os::unix::fs::symlink("ttyFOO", t.path().join("dev/myboard")).unwrap();

        let got = r.resolve_input("/dev/myboard").unwrap();
        assert_eq!(got.identity, "raw:/dev/ttyFOO");
        assert_eq!(got.kind, DeviceKind::Raw);
        assert_eq!(got.path, Some(t.path().join("dev/ttyFOO")));
        assert!(got.warning.is_some(), "the raw escape hatch warns (§12)");
        assert_eq!(
            r.resolve_current_path(&got.identity),
            Some(t.path().join("dev/ttyFOO"))
        );
    }

    #[test]
    fn a_nested_device_path_that_is_not_a_symlink_keeps_its_spelling() {
        // Canonicalization must not flatten a real nested path to `/dev/<basename>`:
        // `/dev/pts/5` is `raw:/dev/pts/5`, never `raw:/dev/5` (RES-3).
        let t = TmpTree::new();
        let r = Resolver::new(t.path());
        write(&t.path().join("dev/pts/5"), "");
        let got = r.resolve_input("/dev/pts/5").unwrap();
        assert_eq!(got.identity, "raw:/dev/pts/5");
        assert_eq!(got.path, Some(t.path().join("dev/pts/5")));
        assert_eq!(
            r.resolve_current_path(&got.identity),
            Some(t.path().join("dev/pts/5"))
        );
        // A redundant spelling of the same path normalizes to it, rather than
        // storing a device string with `.` components in it.
        assert_eq!(r.resolve_input("/dev/./pts/5").unwrap(), got);
    }
}
