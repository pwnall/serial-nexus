//! Phase 3 log fault-isolation slice, ported from
//! `scripts/validate/phase3/log-enospc.sh` (design §5 / §7.3): a `log` node whose
//! file lives on a full disk faults with an ENOSPC write reason, while the port and
//! its other consumers keep flowing — loss is faulted and isolated, never a wedged
//! data plane.
//!
//! Both overflow policies are covered, because §7.3 sanctions **two** answers to a
//! refusing filesystem and the bash rig only ever pinned one:
//!
//! * `overflow = "fault"` — [`log_faults_on_enospc_while_data_plane_stays_live`], the
//!   ported script: the node faults with the write error as its reason.
//! * the **default** `drop-oldest` — [`the_default_policy_stays_active_and_separates_write_refusals`],
//!   new with review LOG-2. Here the node deliberately stays `active` while
//!   `dropped_bytes` climbs (§5 names the full disk as the case the policy exists
//!   for, so a fault would be *wrong* here), and the added `write_errors` /
//!   `last_write_error` are what let an operator tell "the filesystem is refusing
//!   every write" from "the consumer is slow" — the surviving, observability half of
//!   a finding whose defect claim was refuted on blind re-verification.
//!
//! Uses `/dev/full` (always ENOSPC on write, no privilege) reached through a symlink,
//! and a `nexus-sim pty --echo` standing in for the serial device. Both are
//! **Linux-only** — `/dev/full` does not exist on macOS and a pts cannot be a serial
//! device there ([`serial_echo`] returns `None`) — so these tests self-skip off Linux
//! (a skip is a valid verdict, §5).

use std::time::Duration;

use nexus_itest::{Daemon, Sim, TempRun, serial_echo, wait_until};

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

#[test]
fn the_default_policy_stays_active_and_separates_write_refusals() {
    // Review LOG-2. `p3_log_enospc` pinned `overflow = "fault"`, so the **default**
    // arm — the one every operator gets who never writes the attribute — had no test
    // either way. What §5/§7.3 specify for it is *not* a fault: drop-oldest with
    // exact counters is a sanctioned answer to a full disk, and the blind
    // re-verification refuted the claim that staying `active` here is a defect. So
    // this test asserts the specified behavior (still `active`, `dropped_bytes`
    // climbing) **and** the observability the review's surviving half added:
    // `write_errors` / `last_write_error`, which are the only way to tell a
    // filesystem refusing every write from a consumer that is merely slow.
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP the_default_policy_stays_active_and_separates_write_refusals: \
             no serial device on this platform"
        );
        return;
    };
    let full = std::path::Path::new("/dev/full");
    if !full.exists() || std::fs::OpenOptions::new().write(true).open(full).is_err() {
        eprintln!(
            "SKIP the_default_policy_stays_active_and_separates_write_refusals: \
             no writable /dev/full on this system"
        );
        return;
    }

    let dir = TempRun::new();
    std::os::unix::fs::symlink("/dev/full", dir.join("full")).expect("symlink full -> /dev/full");
    let console = dir.join("console");

    let d = Daemon::start();
    let rpc = d.rpc();

    // Identical to the fault graph above but for the one omitted attribute: the
    // `diskfull` log node leaves `overflow` at its default (drop-oldest).
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

    // Hostward bytes: client → console → usb0 → echo device → back hostward →
    // {console, diskfull}. Every write the log attempts is refused with ENOSPC.
    let probe = Sim::client(&[
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
        probe.get("pass").and_then(|p| p.as_bool()).unwrap_or(false),
        "echo probe failed: {probe}"
    );

    // The loss is counted, and — the part `dropped_bytes` alone cannot say — the
    // refusals are counted separately, with the most recent message kept.
    assert!(
        wait_until(Duration::from_secs(15), || {
            let n = rpc.node("diskfull").unwrap_or(serde_json::Value::Null);
            n["dropped_bytes"].as_u64().unwrap_or(0) > 0
                && n["write_errors"].as_u64().unwrap_or(0) > 0
        }),
        "the default policy did not record the refused writes (LOG-2): {:?}",
        rpc.node("diskfull")
    );
    let node = rpc.node("diskfull").expect("diskfull node present");

    // §5's sanctioned arm: drop-oldest keeps the node ACTIVE. A fault here would be
    // the wrong behavior, so it is asserted as such rather than merely tolerated.
    assert_eq!(
        rpc.node_status("diskfull"),
        "active",
        "drop-oldest is a sanctioned policy arm for a full disk (§5); the node must \
         not fault: {node}"
    );
    assert!(
        node.get("reason").is_none_or(serde_json::Value::is_null),
        "an active node must carry no fault reason: {node}"
    );

    // The message names the write and the file, which is what makes it actionable.
    let last = node["last_write_error"].as_str().unwrap_or("").to_owned();
    assert!(
        last.contains("write") && last.contains("full"),
        "last_write_error must name the failing write and its file, got {last:?}"
    );
    assert!(
        last.contains("space") || last.contains("os error 28"),
        "last_write_error must carry the OS error (ENOSPC), got {last:?}"
    );

    // Fault isolation still holds: the port and its live PTY consumer keep flowing.
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
        "echo probe failed while the log was shedding (data plane wedged): {probe2}"
    );
    assert_eq!(
        rpc.node_status("console"),
        "active",
        "console should stay active while the log sheds"
    );
}
