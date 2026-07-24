//! Phase 4 arbitration (design §6), ported from `scripts/validate/phase4/held.sh`.
//!
//! A `write_mode = "held"` origin acquires the write lock **on attach** and holds it
//! **indefinitely**: a client detach must NOT release it — only node removal does.
//! This is the demux codec's permanent hold in miniature; here a PTY edge stands in
//! for the codec. The test is a regression guard against detach-release (§6) wrongly
//! firing on a `held` holder.
//!
//! The lock lives on the `serial` node's host-facing endpoint (`usb0`), so this needs
//! a serial *device*: it obtains an echo device from [`serial_echo`] and self-skips
//! where none exists (macOS — a pts cannot be a serial device, §13/§5), the same
//! self-skip discipline the bash hardware rig used. The PTY, lock, and client legs
//! carry the actual behavior under test.

use std::time::Duration;

use nexus_itest::{Daemon, Sim, serial_echo, wait_until};
use serde_json::Value;

#[test]
fn held_origin_acquires_lock_on_attach_and_survives_client_detach() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP held_origin_acquires_lock_on_attach_and_survives_client_detach: \
             no serial device on this platform"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let ptyh_path = run.join("ttyH");

    // A `held` edge from the serial node (which owns the lock endpoint) to a PTY
    // origin, matching held.sh's graph exactly.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "ptyh"
path = "{pty}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "ptyh"
write_mode = "held"
"#,
        pty = ptyh_path.display(),
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load held-edge config");

    // usb0's write lock, as reported in `state` (§6): `.lock.holder` is the holding
    // origin's label, `.lock.origins[]` each carry `write_mode` + `holds_lock`.
    let holder = || -> Option<String> {
        rpc.node("usb0")
            .and_then(|n| n.get("lock").cloned())
            .and_then(|l| l.get("holder")?.as_str().map(str::to_owned))
    };

    // (1) A `held` origin acquires the lock on attach (register), with no explicit
    // `lock` verb. Bounded wait so a just-finished `load` settling is tolerated.
    assert!(
        wait_until(Duration::from_secs(5), || holder().as_deref()
            == Some("ptyh")),
        "held origin did not acquire the lock on attach; usb0={:?}",
        rpc.node("usb0")
    );

    // (2) The origin is reported as a held writer that holds the lock.
    let origin = rpc
        .node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("origins").and_then(|o| o.as_array()).cloned())
        .and_then(|arr| {
            arr.into_iter()
                .find(|o| o.get("origin").and_then(Value::as_str) == Some("ptyh"))
        })
        .expect("usb0 lock reports a `ptyh` origin");
    assert_eq!(
        origin.get("write_mode").and_then(Value::as_str),
        Some("held"),
        "held origin not reported with write_mode=held: {origin:?}"
    );
    assert_eq!(
        origin.get("holds_lock").and_then(Value::as_bool),
        Some(true),
        "held origin not reported as holding the lock: {origin:?}"
    );

    // (3) A client attaches to the PTY, writes, and detaches. The verdict is ignored
    // (held.sh runs this with `|| true`); the point is the attach→write→detach cycle,
    // not the payload. `Sim::client` runs to completion, so the client has exited
    // (detached) by the time it returns.
    let _ = Sim::client(&[
        "--path",
        &ptyh_path.to_string_lossy(),
        "--send",
        "seeded:256",
        "--seed",
        "5",
        "--timeout-ms",
        "8000",
    ]);

    // Wait for the daemon to observe the detach (presence flips back to absent). The
    // PTY presence poll may lag the client's exit by one interval — bound it.
    let detached = wait_until(Duration::from_secs(5), || {
        rpc.node("ptyh")
            .and_then(|n| n.get("client_present").and_then(Value::as_bool))
            == Some(false)
    });
    assert!(
        detached,
        "pty client never detached; ptyh={:?}",
        rpc.node("ptyh")
    );

    // (4) The held lock must SURVIVE the client detach — held indefinitely (§6). A
    // detach-release firing here would be the regression this guard catches.
    assert_eq!(
        holder().as_deref(),
        Some("ptyh"),
        "held origin released its lock on client detach (must be held indefinitely, §6)"
    );
}
