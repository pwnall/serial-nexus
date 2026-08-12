#![forbid(unsafe_code)]

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
//!
//! The second guard is the other half of "held indefinitely" —
//! [`a_held_pty_origin_reclaims_the_lock_a_steal_freed`], §6/§15.23's "reclaims the
//! moment it frees". It was proven for the codec and the map and simply unimplemented
//! for a pty (37-LOCK-1): the interior pumps drive their reclaim from
//! `runtime::reacquire_held`, a `held` **pty** edge has no interior pump, and its read
//! gate was a bare `may_write`. Any path that frees the endpoint while the pty is not
//! the holder — a completed `send --steal`, `unlock`, a lease expiry — therefore left
//! it free but *untakeable*: nobody reclaimed, and held priority inside
//! `EndpointLock::acquire` denies every other origin on a free lock while a `held`
//! origin is registered. The console's bytes backpressured until a manual `lock`,
//! another steal, or edge surgery.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};

/// The graph both guards load: a `held` edge from the serial node (which owns the
/// lock endpoint) to a PTY origin, matching held.sh's graph exactly.
fn held_graph(pty: &Path, dev: &Path) -> String {
    format!(
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
        pty = pty.display(),
        dev = dev.display(),
    )
}

/// `usb0`'s write-lock holder, as reported in `state` (§6): `.lock.holder` is the
/// holding origin's label, `.lock.origins[]` each carry `write_mode` + `holds_lock`.
fn holder(rpc: &Rpc) -> Option<String> {
    rpc.node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("holder")?.as_str().map(str::to_owned))
}

/// Open the PTY slave the way a client does — never adopting it as this process's
/// controlling terminal, and non-blocking so the round-trip read below can bound
/// itself instead of parking on a console that is (correctly or otherwise) silent.
fn attach(path: &Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(path)
        .unwrap_or_else(|e| panic!("open pty slave {}: {e}", path.display()))
}

/// Read from `client` until `marker` has been seen or `timeout` elapses, returning
/// everything read. Tolerant of extra bytes on purpose: the device echoes whatever
/// else the test sent through it, and the guard is about the marker arriving at all.
fn read_until(client: &mut std::fs::File, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        match client.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                seen.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&seen).contains(marker) {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("read pty slave: {e}"),
        }
    }
    String::from_utf8_lossy(&seen).into_owned()
}

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

    rpc.load_toml(&held_graph(&ptyh_path, echo.device()), false)
        .expect("load held-edge config");

    // (1) A `held` origin acquires the lock on attach (register), with no explicit
    // `lock` verb. Bounded wait so a just-finished `load` settling is tolerated.
    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc).as_deref()
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
        holder(rpc).as_deref(),
        Some("ptyh"),
        "held origin released its lock on client detach (must be held indefinitely, §6)"
    );
}

/// §6/§15.23: "a `held` origin that lost its lock to a steal reclaims it ahead of
/// every on-demand waiter **the moment it frees**". 37-LOCK-1 — for a pty origin
/// nothing drove that reclaim, so a completed `send --steal` left `usb0` free and
/// untakeable, with the console backpressured indefinitely.
///
/// Both halves are asserted, because only together do they distinguish the defect
/// from a lock that merely *reports* well: the holder returns to `ptyh`, **and** the
/// console's own bytes reach the device again (the echo device returns them, so the
/// round trip proves the read gate reopened rather than the label moving).
#[test]
fn a_held_pty_origin_reclaims_the_lock_a_steal_freed() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_held_pty_origin_reclaims_the_lock_a_steal_freed: \
             no serial device on this platform"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let ptyh_path = d.run().join("ttyH");
    rpc.load_toml(&held_graph(&ptyh_path, echo.device()), false)
        .expect("load held-edge config");
    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc).as_deref()
            == Some("ptyh")),
        "held origin did not acquire the lock on attach; usb0={:?}",
        rpc.node("usb0")
    );

    // A console client holds the slave for the whole test: hostward output is
    // presence-gated (§7.2), so without it the echoed bytes would be
    // discarded-and-counted rather than delivered.
    let mut client = attach(&ptyh_path);

    // The steal, and its release. `send --steal` takes the floor from `ptyh`, writes
    // its line, then unregisters its transient origin — which clears the holder (§6).
    // That is the exact sequence that used to wedge the endpoint.
    rpc.send("usb0", "stolen-line", true, 5_000)
        .expect("send --steal");

    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc).as_deref()
            == Some("ptyh")),
        "the held origin never reclaimed the lock a `send --steal` freed: usb0={:?} \
         (§6/§15.23 — the endpoint is free but untakeable, since held priority denies \
         every other origin too)",
        rpc.node("usb0")
    );

    // …and the reclaim is real, not just reported: the console writes, the echo
    // device returns it, and the daemon delivers it hostward. Before the fix the
    // read gate stayed shut and this line sat in the master's kernel buffer forever.
    client
        .write_all(b"reclaimed-marker\n")
        .expect("write into the console");
    let seen = read_until(&mut client, "reclaimed-marker", Duration::from_secs(10));
    assert!(
        seen.contains("reclaimed-marker"),
        "the console's bytes never reached the device after the steal released the \
         lock (read {seen:?}); usb0={:?}",
        rpc.node("usb0")
    );
}
