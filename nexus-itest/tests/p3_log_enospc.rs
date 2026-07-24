//! Phase 3 log fault-isolation slice, ported from
//! `scripts/validate/phase3/log-enospc.sh` (design §5 / §7.3): a `log` node whose
//! file lives on a full disk faults with an ENOSPC write reason, while the port and
//! its other consumers keep flowing — loss is faulted and isolated, never a wedged
//! data plane.
//!
//! Uses `/dev/full` (always ENOSPC on write, no privilege) reached through a symlink,
//! and a `nexus-sim pty --echo` standing in for the serial device. Both are
//! **Linux-only** — `/dev/full` does not exist on macOS and a pts cannot be a serial
//! device there ([`serial_echo`] returns `None`) — so this test self-skips off Linux
//! (a skip is a valid verdict, §5).

use std::time::Duration;

use nexus_itest::{Daemon, Sim, TempRun, serial_echo};

#[test]
fn log_faults_on_enospc_while_data_plane_stays_live() {
    // Needs a serial *device* (the echo double); `None` off Linux → skip.
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP log_faults_on_enospc_while_data_plane_stays_live: no serial device on this platform"
        );
        return;
    };
    // Needs a writable `/dev/full` to force ENOSPC (the bash `[ -w /dev/full ]` gate).
    let full = std::path::Path::new("/dev/full");
    if !full.exists() || std::fs::OpenOptions::new().write(true).open(full).is_err() {
        eprintln!(
            "SKIP log_faults_on_enospc_while_data_plane_stays_live: no writable /dev/full on this system"
        );
        return;
    }

    // A small dir holds the console symlink and the log's `full` symlink → /dev/full,
    // so the §7.3 rotation-counter directory scan stays cheap.
    let dir = TempRun::new();
    std::os::unix::fs::symlink("/dev/full", dir.join("full")).expect("symlink full -> /dev/full");
    let console = dir.join("console");

    let d = Daemon::start();
    let rpc = d.rpc();

    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{device}"
[[node]]
type = "log"
name = "diskfull"
directory = "{dir}"
filename = "full"
overflow = "fault"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "diskfull"
"#,
        console = console.display(),
        device = echo.device().display(),
        dir = dir.path().display(),
    );
    rpc.load_toml(&cfg, false).expect("load config");

    // Bounded readiness: the port must open and the console pty must be live before
    // bytes can flow (the bash relied on the sim client's own device-wait).
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "usb0 (serial) never became active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console (pty) never became active: {:?}",
        rpc.node("console")
    );

    // Drive hostward bytes: client → console → usb0 → echo device → back hostward →
    // {console, diskfull}. That forces the log's write to /dev/full and, under
    // overflow=fault, faults the node.
    let probe1 = Sim::client(&[
        "--path",
        &console.to_string_lossy(),
        "--send",
        "seeded:8KiB",
        "--expect",
        "echo",
        "--seed",
        "1",
        "--timeout-ms",
        "15000",
    ]);
    assert!(
        probe1
            .get("pass")
            .and_then(|p| p.as_bool())
            .unwrap_or(false),
        "first echo probe failed: {probe1}"
    );

    // The log faults on the ENOSPC write, isolated from the port.
    assert!(
        rpc.wait_status("diskfull", "faulted", Duration::from_secs(10)),
        "log node did not fault on ENOSPC: {:?}",
        rpc.node("diskfull")
    );

    // The fault reason names the write failure (§7.3): the log's writer reports
    // `write <path>: <err>`; on Linux ENOSPC is "No space left on device (os error 28)".
    let node = rpc.node("diskfull").expect("diskfull node present");
    let reason = node
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_owned();
    assert!(
        reason.contains("write") || reason.contains("space") || reason.contains("os error 28"),
        "fault reason does not mention the write failure: {reason:?}"
    );

    // The data plane is not wedged: the port and its live PTY consumer keep flowing,
    // byte-for-byte (received == the full 8 KiB), after the log faulted.
    let probe2 = Sim::client(&[
        "--path",
        &console.to_string_lossy(),
        "--send",
        "seeded:8KiB",
        "--expect",
        "echo",
        "--seed",
        "2",
        "--timeout-ms",
        "15000",
    ]);
    assert!(
        probe2
            .get("pass")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
            && probe2.get("received").and_then(|r| r.as_u64()) == Some(8192),
        "echo probe failed after the log faulted (data plane wedged): {probe2}"
    );

    // The console must stay active while the log is faulted (fault isolation, §7.3).
    assert_eq!(
        rpc.node_status("console"),
        "active",
        "console should stay active while the log is faulted"
    );
}
