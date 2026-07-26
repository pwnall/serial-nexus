//! A `faces = target` leg's **write arbitration** against the local graph (design
//! §6, §7.1 "release on idle *or* peer disconnect", §7.4) — the path review §2 item 4
//! (T1) found normative and untested.
//!
//! When wire data arrives for a channel, the leg is just another writer at the local
//! host-facing endpoint: it must take that endpoint's write lock **implicitly**, on
//! the first targetward byte (an operator never types `lock` for a remote peer), and
//! give it back both ways it can become stale —
//!
//! 1. [`the_leg_acquires_on_first_data_and_releases_when_idle`] — after
//!    `idle_release_ms` of quiet, so a remote console that stopped typing does not
//!    hold a local port's floor.
//! 2. [`the_leg_releases_the_local_lock_when_the_peer_disconnects`] — immediately on
//!    peer loss, so a local operator is not blocked behind a machine that vanished.
//!    Its `idle_release_ms` is a minute, so a release inside seconds can only have
//!    come from the disconnect path: the two mechanisms are told apart by
//!    construction rather than by timing luck.
//!
//! Two daemons over a loopback unix socket, modelled on `p6_reference`/`p6_outage`:
//!
//! ```text
//!   daemon B (operator side)                 daemon A (device side)
//!   send downlink/c0 ──▶ downlink (leg,      uplink (leg, faces=target,
//!                        faces=host,  ──────▶ role=connect) ──▶ usb0 (serial)
//!                        role=listen)          [origin "uplink/c0" on usb0's lock]
//! ```
//!
//! **No serial device is needed** — and that is not a shortcut: `usb0`'s device is
//! deliberately absent, so the node sits `waiting` while its write lock, the leg's
//! registered origin and its targetward receiver exist regardless (structural, §6,
//! the same footing `p4_waiting` stands on). So both tests run on **every** platform.

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, wait_until};
use serde_json::Value;

/// Daemon B: the operator side. A `faces = host` leg listening on `leg`, with one
/// free-for-all channel, so a plain `send` at `downlink/c0` puts bytes on the wire.
fn operator_cfg(leg: &Path) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
arbitration = "free-for-all"
channels = ["c0"]
"#,
        leg = leg.display(),
    )
}

/// Daemon A: the device side. A `faces = target` leg dialling `leg`, wired into a
/// device-absent `serial` node as an **on-demand** origin — the §7.4 shape, and the
/// one that makes the leg acquire and release rather than hold indefinitely.
fn device_cfg(leg: &Path, absent_device: &Path, idle_release_ms: u64) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "connect"
address = "{leg}"
reconnect_initial_ms = 100
reconnect_max_ms = 400
idle_release_ms = {idle_release_ms}
channels = ["c0"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
"#,
        leg = leg.display(),
        dev = absent_device.display(),
    )
}

/// The current holder of `usb0`'s write lock on the device-side daemon.
fn holder(rpc: &Rpc) -> Option<String> {
    rpc.node("usb0")?
        .pointer("/lock/holder")?
        .as_str()
        .map(str::to_owned)
}

/// One leg channel's `accepted_targetward` — bytes this leg handed to the local
/// graph, counted only once the write is accepted (§7.4).
fn accepted_targetward(rpc: &Rpc, node: &str, ch: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/accepted_targetward"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

/// Bring up the operator/device pair and wait until the leg is connected and `c0` is
/// bound on both sides. Returns the two daemons (kept alive by the caller) and the
/// line-length bookkeeping is the caller's.
fn connected_pair(idle_release_ms: u64) -> (Daemon, Daemon) {
    // The listener first, so the dialling side's first attempt lands.
    let db = Daemon::start();
    let leg = db.run().join("leg.sock");
    db.rpc()
        .load_toml(&operator_cfg(&leg), false)
        .expect("operator-side load");
    assert!(
        wait_until(Duration::from_secs(10), || leg.exists()),
        "the operator leg never bound its socket"
    );

    let da = Daemon::start();
    let absent = da.run().join("absent-device");
    da.rpc()
        .load_toml(&device_cfg(&leg, &absent, idle_release_ms), false)
        .expect("device-side load");

    assert!(
        wait_until(Duration::from_secs(15), || {
            da.rpc().node("uplink").and_then(|n| {
                n.get("connection")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }) == Some("connected".to_owned())
                && db.rpc().node("downlink").and_then(|n| {
                    n.pointer("/channels/c0/binding")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                }) == Some("bound".to_owned())
        }),
        "the leg pair never connected and bound c0: uplink={:?} downlink={:?}",
        da.rpc().node("uplink"),
        db.rpc().node("downlink")
    );
    // Nothing has crossed yet, so the local endpoint's floor is free (§6): the leg
    // must not take a lock it has no bytes for.
    assert_eq!(
        holder(da.rpc()),
        None,
        "the leg took the local write lock before any data arrived"
    );
    (da, db)
}

// ============================================================================
// 1 — implicit acquire on first data, then idle-release.
// ============================================================================
#[test]
fn the_leg_acquires_on_first_data_and_releases_when_idle() {
    const IDLE_MS: u64 = 400;
    let (da, db) = connected_pair(IDLE_MS);
    let (rpc_a, rpc_b) = (da.rpc(), db.rpc());

    // An operator types on the far side. `send` appends the newline, so six bytes
    // cross the wire.
    rpc_b
        .send("downlink/c0", "hello", false, 5000)
        .expect("send");
    const SENT: u64 = 6;

    // Implicit acquire (§7.1): the leg's origin — named for its endpoint address —
    // takes the local endpoint's write lock on the first targetward byte, with no
    // `lock` verb anywhere in this test.
    assert!(
        wait_until(Duration::from_secs(10), || holder(rpc_a).as_deref()
            == Some("uplink/c0")),
        "the leg did not acquire the local write lock on first data: {:?}",
        rpc_a.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || accepted_targetward(
            rpc_a, "uplink", "c0"
        ) == SENT),
        "the leg did not hand all {SENT} bytes to the local graph: {:?}",
        rpc_a.node("uplink")
    );

    // Idle-release (§7.1): quiet for longer than `idle_release_ms` frees the floor
    // for a local operator. Bounded wait, generously above the configured idle.
    assert!(
        wait_until(Duration::from_secs(10), || holder(rpc_a).is_none()),
        "the leg still held the local write lock after {IDLE_MS} ms of quiet: {:?}",
        rpc_a.node("usb0")
    );

    // And the release is not terminal: the next byte re-acquires, so an idle release
    // costs a remote operator nothing but the acquire (§6 FIFO).
    rpc_b
        .send("downlink/c0", "again", false, 5000)
        .expect("second send");
    assert!(
        wait_until(Duration::from_secs(10), || holder(rpc_a).as_deref()
            == Some("uplink/c0")),
        "the leg did not re-acquire after its idle release: {:?}",
        rpc_a.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || accepted_targetward(
            rpc_a, "uplink", "c0"
        ) == 2 * SENT),
        "the second line did not reach the local graph: {:?}",
        rpc_a.node("uplink")
    );
}

// ============================================================================
// 2 — disconnect-release, told apart from idle-release by construction.
// ============================================================================
#[test]
fn the_leg_releases_the_local_lock_when_the_peer_disconnects() {
    // A minute of idle allowance: nothing that happens inside this test's bounded
    // waits can be the idle timer, so a release here is the disconnect path or
    // nothing (§7.1, LEG-4).
    const IDLE_MS: u64 = 60_000;
    let (da, db) = connected_pair(IDLE_MS);
    let rpc_a = da.rpc();

    db.rpc()
        .send("downlink/c0", "hello", false, 5000)
        .expect("send");
    assert!(
        wait_until(Duration::from_secs(10), || holder(rpc_a).as_deref()
            == Some("uplink/c0")),
        "the leg did not acquire the local write lock on first data: {:?}",
        rpc_a.node("usb0")
    );
    // It is genuinely held across the quiet that follows — otherwise the release
    // below would prove nothing.
    assert_eq!(
        holder(rpc_a).as_deref(),
        Some("uplink/c0"),
        "the leg's hold should outlast a moment of quiet at a 60 s idle allowance"
    );

    // The peer's machine goes away.
    drop(db);

    assert!(
        wait_until(Duration::from_secs(10), || holder(rpc_a).is_none()),
        "the leg kept the local write lock after its peer disconnected — a local \
         operator is blocked behind a vanished remote (§7.1): {:?}",
        rpc_a.node("usb0")
    );
    // The node itself is unharmed and simply waiting for the peer to come back
    // (faulted-and-wait, §7.4) — the release is a lock event, not a teardown.
    assert!(
        rpc_a.node("uplink").is_some(),
        "the leg node disappeared with its peer"
    );
}
