//! Phase 7 device-identity matrix, ported from `scripts/validate/phase7/matrix.sh`
//! (design §12, plan §Phase 7 item 4). The §12 identity↔path resolution is exercised
//! unprivileged against fixture `/dev/serial/by-id`, `/dev/serial/by-path`, and sysfs
//! trees rooted under a `--dev-root` prefix (the documented resolver seam) — no
//! hardware, no root. Four properties, over one shared daemon + fixture:
//!
//! 1. An FT4232-style multi-interface device (one USB device, four interfaces) yields
//!    four independently bound `serial` nodes that each reach `active` on a distinct
//!    resolved `/dev` path.
//! 2. A no-serial clone (a cheap CH340) added by raw path degrades to a `by-path`
//!    identity, carrying the documented instability `.warning` in the add-time RPC
//!    result, and `dump` round-trips the `by-path:` identity (not the raw path).
//! 3. A raw-path add with the device absent fails as designed (§12: capture forms need
//!    the device present) — the daemon returns the `DeviceAbsent` app error.
//! 4. An identity-form add with the device absent succeeds into `waiting` (§12: an
//!    identity add never needs the device plugged in).
//!
//! Ground truth is structured RPC only (`state`/`dump`/`add-node` results), never CLI
//! text (§5).
//!
//! Deviations from the bash, each preserving the original assertions:
//! * The whole matrix needs sim-pty serial devices at fixture `/dev` paths that a
//!   `serial` node can open *and* a sysfs fixture tree the resolver can walk. Both are
//!   Linux-only — a pty is not a serial device on macOS (`serial2` → `ENOTTY`), the
//!   same reason [`serial_echo`]/[`serial_pair`] return `None` there — so the test
//!   skips off Linux (`eprintln! SKIP; return`).
//! * The bash held its `nexus-sim pty --echo` devices open with `--hold-ms`; this uses
//!   `--timeout-ms` (the lifetime knob for a pure echo device, as [`serial_echo`] does
//!   in the harness) with a value far longer than the test.
//! * `Daemon::start` cannot pass `--dev-root`, so the daemon is hand-managed via
//!   `Command` + `bin("serialnexusd")` (the pattern in `p3_log.rs`), killed on drop.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Rpc, Sim, TempRun, bin, serial_echo, wait_until};
use serde_json::Value;

/// `nexus-rpc` `AppError::DeviceAbsent` — base `-32000` (`APP_ERROR_BASE`) offset by 5.
const DEVICE_ABSENT: i64 = -32005;

/// A daemon child SIGKILLed and reaped on drop, so a panicking test never leaks it.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn `serialnexusd` on `run`'s socket + state file with the resolver rooted at
/// `dev_root` (the §12 fixture seam). `Daemon::start` cannot express `--dev-root`.
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

/// Wait until a daemon is actually listening on `sock` (a bound listener accepts;
/// a stale socket file refuses). Bounded poll — the restart-safe `test -S` replacement.
fn wait_socket(sock: &Path) -> bool {
    wait_until(Duration::from_secs(10), || {
        std::os::unix::net::UnixStream::connect(sock).is_ok()
    })
}

/// Force-create a symlink at `link` pointing to `target` (removing any prior entry),
/// the portable `ln -sfn`. The fixture dir is fresh, so a prior entry never exists.
fn symlink_force(target: &str, link: &Path) {
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link).expect("create fixture symlink");
}

/// Build one USB interface under `root`: the sysfs USB device (shared `usbdir`, so
/// several interfaces share one `idVendor`/`serial`), its interface dir + class/tty
/// link, and a `by-id` entry — a faithful copy of `scripts/lib/fixture-tree.sh`'s
/// `make_usb_iface`. `serial == ""` means a no-serial clone (no `serial` file written,
/// so the sysfs walk yields `-` and the resolver degrades to by-path, §12).
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
    std::fs::create_dir_all(&dev).expect("mkdir usb device dir");
    std::fs::write(dev.join("idVendor"), vid).unwrap();
    std::fs::write(dev.join("idProduct"), pid).unwrap();
    if !serial.is_empty() {
        std::fs::write(dev.join("serial"), serial).unwrap();
    }
    std::fs::write(dev.join("manufacturer"), "FTDI-ish").unwrap();
    std::fs::write(dev.join("product"), "Fixture Serial").unwrap();
    let ifdir = dev.join(format!("{usbdir}:1.{iface}"));
    std::fs::create_dir_all(&ifdir).expect("mkdir interface dir");
    std::fs::write(ifdir.join("bInterfaceNumber"), iface).unwrap();
    // class/tty/<devname>/device -> the interface dir (relative, stays in-tree).
    let class = root.join("sys/class/tty").join(devname);
    std::fs::create_dir_all(&class).expect("mkdir class/tty dir");
    symlink_force(
        &format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.{iface}"),
        &class.join("device"),
    );
    // by-id/<byid> -> ../../<devname>
    let by_id = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id).expect("mkdir by-id");
    symlink_force(&format!("../../{devname}"), &by_id.join(byid));
}

/// Add a `/dev/serial/by-path` entry covering `devname` (the no-serial-clone
/// by-path fallback, §12) — the `make_bypath` fixture helper.
fn make_bypath(root: &Path, port: &str, devname: &str) {
    let by_path = root.join("dev/serial/by-path");
    std::fs::create_dir_all(&by_path).expect("mkdir by-path");
    symlink_force(&format!("../../{devname}"), &by_path.join(port));
}

/// A single-`serial`-node TOML block, added incrementally (§11). Mirrors the bash's
/// `addnode` heredoc (free-for-all so binding doesn't hinge on a lock).
fn node_toml(name: &str, device: &str) -> String {
    format!(
        "[[node]]\ntype = \"serial\"\nname = \"{name}\"\ndevice = \"{device}\"\narbitration = \"free-for-all\"\n"
    )
}

/// The resolved `/dev` paths of every node whose name starts with `ft`, in `state`.
fn ft_resolved_paths(rpc: &Rpc) -> Vec<String> {
    rpc.state()["nodes"]
        .as_array()
        .expect("state.nodes array")
        .iter()
        .filter(|n| {
            n["name"]
                .as_str()
                .map(|s| s.starts_with("ft"))
                .unwrap_or(false)
        })
        .filter_map(|n| n["resolved_path"].as_str().map(str::to_owned))
        .collect()
}

/// How many `ft*` nodes are currently `active` in `state`.
fn ft_active_count(rpc: &Rpc) -> usize {
    rpc.state()["nodes"]
        .as_array()
        .map(|ns| {
            ns.iter()
                .filter(|n| {
                    n["name"]
                        .as_str()
                        .map(|s| s.starts_with("ft"))
                        .unwrap_or(false)
                        && n["status"] == "active"
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn device_identity_matrix_over_fixture_trees() {
    // The matrix needs openable sim-pty serial devices at fixture `/dev` paths plus a
    // sysfs tree the resolver walks — both Linux-only, the same reason serial_echo
    // skips elsewhere. Probe that helper so the platform intent is explicit.
    if !cfg!(target_os = "linux") || serial_echo().is_none() {
        eprintln!(
            "SKIP device_identity_matrix_over_fixture_trees: fixture sysfs + sim-pty \
             serial devices are Linux-only (§12)"
        );
        return;
    }

    let run = TempRun::new();
    let root = run.join("root");
    std::fs::create_dir_all(root.join("dev")).expect("mkdir <root>/dev");

    // FT4232-style device: one USB device (serial FT4232) with four interfaces →
    // ttyUSB0..3, each a present sim echo device. Build the fixture tree first, then
    // bring up the backing device node (as the bash does).
    let mut sims: Vec<Sim> = Vec::new();
    for i in 0..4 {
        let devname = format!("ttyUSB{i}");
        let iface = format!("0{i}");
        let byid = format!("usb-FTDI_FT4232-if{iface}");
        make_usb_iface(
            &root, "2-1", "0403", "6011", "FT4232", &devname, &iface, &byid,
        );
        let devpath = root.join("dev").join(&devname);
        let devstr = devpath.to_string_lossy().into_owned();
        sims.push(Sim::spawn(
            &["pty", "--echo", "--link", &devstr, "--timeout-ms", "120000"],
            Some(&devpath),
        ));
    }

    // No-serial clone (a cheap CH340) with a by-path entry.
    make_usb_iface(
        &root,
        "3-1",
        "1a86",
        "7523",
        "", // no serial number -> sysfs walk yields `-` -> by-path fallback
        "ttyUSB9",
        "00",
        "usb-1a86_CH340-if00",
    );
    make_bypath(&root, "pci-0000:00:14.0-usb-0:3:1.0-port0", "ttyUSB9");
    let clone_dev = root.join("dev/ttyUSB9");
    let clone_devstr = clone_dev.to_string_lossy().into_owned();
    sims.push(Sim::spawn(
        &[
            "pty",
            "--echo",
            "--link",
            &clone_devstr,
            "--timeout-ms",
            "120000",
        ],
        Some(&clone_dev),
    ));

    // Bring up the daemon with the resolver rooted at the fixture tree.
    let _daemon = KillOnDrop(spawn_daemon(&run, &root));
    assert!(
        wait_socket(&run.socket()),
        "daemon control socket never appeared"
    );
    let rpc = Rpc::new(run.socket());

    // ---- 1. FT4232: four independently bound nodes with distinct resolved paths ----
    for i in 0..4 {
        let name = format!("ft{i}");
        let device = format!("usb:0403:6011:FT4232:0{i}");
        rpc.add_node_toml(&node_toml(&name, &device))
            .unwrap_or_else(|e| panic!("add {name} failed: [{}] {}", e.code, e.message));
    }
    assert!(
        wait_until(Duration::from_secs(20), || ft_active_count(&rpc) == 4),
        "not all four FT interfaces bound active (active={})",
        ft_active_count(&rpc)
    );
    let mut paths = ft_resolved_paths(&rpc);
    assert_eq!(
        paths.len(),
        4,
        "an FT interface reported no resolved_path: {paths:?}"
    );
    paths.sort();
    paths.dedup();
    assert_eq!(
        paths.len(),
        4,
        "FT interfaces did not resolve to 4 distinct paths: {paths:?}"
    );

    // ---- 2. No-serial clone → by-path identity + documented warning ----------------
    let clone = rpc
        .add_node_toml(&node_toml("clone", "/dev/ttyUSB9"))
        .expect("no-serial clone add should succeed (present device)");
    assert!(
        clone.get("warning").filter(|w| !w.is_null()).is_some(),
        "no-serial clone add carried no .warning: {clone}"
    );
    assert_eq!(
        clone["kind"],
        Value::from("by-path"),
        "no-serial clone did not bind by-path: {clone}"
    );
    // Its stored identity is the by-path form: dump round-trips it, not the raw path.
    let dumped = rpc.dump();
    let clone_device = dumped["node"]
        .as_array()
        .expect("dump.node array")
        .iter()
        .find(|n| n["name"] == "clone")
        .and_then(|n| n["device"].as_str())
        .expect("clone node with a device in dump");
    assert!(
        clone_device.starts_with("by-path:"),
        "clone's config identity is not by-path form: {clone_device}"
    );

    // ---- 3. Path-form add, device absent → fails as designed (§12) -----------------
    let err = rpc
        .add_node_toml(&node_toml("ghost_path", "/dev/ttyUSBX_absent"))
        .expect_err("path-form add of an absent device must fail");
    assert_eq!(
        err.code, DEVICE_ABSENT,
        "absent path-form add gave the wrong error code: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.to_lowercase().contains("not present")
            || err.message.to_lowercase().contains("absent"),
        "absent path-form add error should name the absence: {}",
        err.message
    );

    // ---- 4. Identity-form add, device absent → succeeds into waiting ----------------
    let ghost = rpc
        .add_node_toml(&node_toml("ghost_id", "usb:0403:9999:GHOST:00"))
        .expect("identity-form absent add should succeed");
    assert_eq!(
        ghost["added"],
        Value::from("ghost_id"),
        "identity-form absent add did not report the added node: {ghost}"
    );
    assert_eq!(
        rpc.node_status("ghost_id"),
        "waiting",
        "identity-form absent node is not waiting: {:?}",
        rpc.node("ghost_id")
    );

    // Keep the backing devices alive through every assertion above.
    drop(sims);
}
