//! Phase 6 leg-outage slice, ported from `scripts/validate/phase6/outage.sh`
//! (design §7.4 leg lifecycle, §9 wire, §6 arbitration). A `nexus-sim tcp-proxy`
//! between two daemons severs the link mid-stream (`--drop-after`) and restores it
//! (`--restore-after-ms`). During the gap the leg is faulted-and-wait — targetward
//! writers pause (backpressure, no drop). After restore the connect-role leg
//! reconnects, purge-on-reconnect discards the outage-era targetward backlog with a
//! counter (§7.4), and a fresh round-trip is byte-clean.
//!
//! Topology (two daemons, one leg each, a serial echo device behind daemon A):
//!
//! ```text
//!   client → pty p0 ──(targetward)──▶ downlink/c0 (leg, faces=host, listen)
//!                                          │
//!                                    tcp-proxy (severs after 8KiB of A's
//!                                          │      hostward echo, restores at 2.5s)
//!                                     uplink/c0 (leg, faces=target, connect)
//!                                          │
//!                             serial usb0 ─┴─▶ echo device ──(hostward echo)──┘
//! ```
//!
//! Needs a serial *device* (the echo device that bounces the burst back hostward to
//! trip the outage), so it **skips** where none exists (macOS): [`serial_echo`]
//! returns `None`. Data-plane ground truth is the sim client's byte-exact
//! `sha256_sent`/`sha256_received`, never a judgement (§5). The two legs themselves
//! run everywhere; only the echo round-trip needs the device.

use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};
use serde_json::Value;

/// Two distinct free ephemeral TCP ports on loopback (the portable replacement for
/// the bash `free_port` python one-liner). Both listeners are held simultaneously
/// then dropped, so the two ports are guaranteed distinct.
fn two_free_ports() -> (u16, u16) {
    let a = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral a");
    let b = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral b");
    let pa = a.local_addr().expect("local_addr a").port();
    let pb = b.local_addr().expect("local_addr b").port();
    (pa, pb)
}

/// A leg node's flattened `connection` field (`connected`/`waiting`/`faulted`), or
/// `None` when the node is absent (leg.rs `state_extra`, §7.4).
fn leg_connection(rpc: &Rpc, name: &str) -> Option<String> {
    rpc.node(name)?
        .get("connection")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Whether the leg's channel `ch` is `bound` (the peer announced it, §8).
fn channel_bound(rpc: &Rpc, name: &str, ch: &str) -> bool {
    rpc.node(name)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/binding"))
                .and_then(Value::as_str)
                .map(|s| s == "bound")
        })
        .unwrap_or(false)
}

/// The leg's node-level `reconnect_count` (§7.4).
fn reconnect_count(rpc: &Rpc, name: &str) -> u64 {
    rpc.node(name)
        .and_then(|n| n.get("reconnect_count").and_then(Value::as_u64))
        .unwrap_or(0)
}

/// The leg channel's `purged_on_reconnect` counter — outage-era targetward backlog
/// discarded on reconnect (§7.4 purge-on-reconnect).
fn purged_on_reconnect(rpc: &Rpc, name: &str, ch: &str) -> u64 {
    rpc.node(name)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/purged_on_reconnect"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

/// Drive one seeded echo round-trip through daemon B's pty `p0` and return the sim
/// client verdict (whose `sha256_sent`/`sha256_received` are the byte-exact ground
/// truth). Blocks to completion, as the bash's foreground `nexus-sim client` did.
fn echo_roundtrip(p0: &Path, send_spec: &str, seed: u64, timeout_ms: u64) -> Value {
    let path = p0.to_string_lossy().into_owned();
    let seed = seed.to_string();
    let timeout = timeout_ms.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        send_spec,
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        &timeout,
    ])
}

#[test]
fn outage_faults_then_purges_then_recovers_byte_clean() {
    // The echo device that bounces the burst back hostward (tripping the outage) is a
    // serial *device*; skip where the platform has no software serial device (macOS).
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP outage_faults_then_purges_then_recovers_byte_clean: no serial device on this platform"
        );
        return;
    };

    let (port_b, port_p) = two_free_ports();

    // --- Daemon B: the receiver. Its leg listens on PORT_B; a pty p0 is the console.
    let db = Daemon::start();
    let rpc_b = db.rpc();
    let p0 = db.run().join("p0");
    let cfg_b = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "tcp"
role = "listen"
address = "127.0.0.1:{port_b}"
arbitration = "free-for-all"
channels = ["c0"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
"#,
        p0 = p0.display(),
    );
    rpc_b.load_toml(&cfg_b, false).expect("daemon B load");

    // --- The proxy: daemon A dials PORT_P; the proxy forwards to PORT_B, severing
    //     after 8KiB of A's outward (hostward echo) flow, then restoring after 2.5s.
    let proxy_listen = format!("127.0.0.1:{port_p}");
    let proxy_connect = format!("127.0.0.1:{port_b}");
    let _proxy = Sim::spawn(
        &[
            "tcp-proxy",
            "--listen",
            &proxy_listen,
            "--connect",
            &proxy_connect,
            "--drop-after",
            "8KiB",
            "--restore-after-ms",
            "2500",
            "--timeout-ms",
            "40000",
        ],
        None,
    );

    // --- Daemon A: the sender. Its serial owns the echo device; its leg connects to
    //     PORT_P through the proxy.
    let da = Daemon::start();
    let rpc_a = da.rpc();
    let cfg_a = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "tcp"
role = "connect"
address = "127.0.0.1:{port_p}"
reconnect_initial_ms = 150
reconnect_max_ms = 600
channels = ["c0"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
"#,
        dev = echo.device().display(),
    );
    rpc_a.load_toml(&cfg_a, false).expect("daemon A load");

    // The serial node must open its echo device, and the pty console must materialize.
    assert!(
        rpc_a.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 serial not active: {:?}",
        rpc_a.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || p0.exists()),
        "pty p0 symlink never appeared"
    );

    // Both legs connect and bind.
    assert!(
        wait_until(Duration::from_secs(8), || {
            leg_connection(rpc_b, "downlink").as_deref() == Some("connected")
                && channel_bound(rpc_b, "downlink", "c0")
        }),
        "receiver leg never bound: {:?}",
        rpc_b.node("downlink")
    );
    assert!(
        wait_until(Duration::from_secs(8), || {
            leg_connection(rpc_a, "uplink").as_deref() == Some("connected")
        }),
        "sender leg never connected: {:?}",
        rpc_a.node("uplink")
    );

    // 1. Pre-outage: a small round-trip is clean (well under the 8KiB drop threshold).
    let pre = echo_roundtrip(&p0, "seeded:4KiB", 11, 8000);
    assert_eq!(
        pre["pass"].as_bool(),
        Some(true),
        "pre-outage round-trip failed: {pre}"
    );
    assert!(
        pre["sha256_sent"].is_string() && pre["sha256_sent"] == pre["sha256_received"],
        "pre-outage checksum mismatch: {pre}"
    );

    // 2. A burst whose echo crosses the 8KiB threshold trips the outage (its own
    //    round-trip is interrupted; not asserted). Run it in the background so its
    //    sustained hostward echo keeps flowing until the proxy severs.
    let p0_str = p0.to_string_lossy().into_owned();
    let burst = Sim::spawn(
        &[
            "client",
            "--path",
            &p0_str,
            "--send",
            "seeded:64KiB",
            "--expect",
            "echo",
            "--seed",
            "22",
            "--timeout-ms",
            "3000",
        ],
        None,
    );

    // 3. The receiver leg detects the outage: it stops being connected while the link
    //    is down (faulted-and-wait). During this window a writer's bytes back up,
    //    paused not dropped.
    assert!(
        wait_until(Duration::from_secs(15), || {
            matches!(leg_connection(rpc_b, "downlink").as_deref(), Some(c) if c != "connected")
        }),
        "receiver leg never registered the outage: {:?}",
        rpc_b.node("downlink")
    );
    drop(burst); // stop the interrupted burst, as the bash killed its background client.

    // 4. An operator types targetward *during* the outage. With the leg disconnected,
    //    these bytes back up at the receiver (paused, not dropped) — exactly the stale
    //    command hazard purge-on-reconnect exists to defuse (§6/§7.4). The send-only
    //    client (no `--expect`) writes and returns; its verdict is intentionally
    //    ignored (the bash's `|| true`).
    let _ = Sim::client(&[
        "--path",
        &p0_str,
        "--send",
        "seeded:12KiB",
        "--seed",
        "99",
        "--timeout-ms",
        "2000",
    ]);

    // 5. After restore the connect-role leg reconnects (reconnect_count rises,
    //    connection returns to connected, channel rebinds).
    assert!(
        wait_until(Duration::from_secs(20), || {
            leg_connection(rpc_b, "downlink").as_deref() == Some("connected")
                && channel_bound(rpc_b, "downlink", "c0")
                && reconnect_count(rpc_b, "downlink") >= 1
        }),
        "receiver leg never reconnected after restore: {:?}",
        rpc_b.node("downlink")
    );

    // 6. Purge-on-reconnect: the outage-era targetward backlog was discarded with a
    //    counter (§7.4), so stale commands never fire post-restore.
    let purged = purged_on_reconnect(rpc_b, "downlink", "c0");
    assert!(
        purged > 0,
        "purge-on-reconnect counter did not record outage-era backlog (got {purged}): {:?}",
        rpc_b.node("downlink")
    );

    // 7. Post-restore: a fresh round-trip is byte-clean (the data plane recovered).
    let post = echo_roundtrip(&p0, "seeded:4KiB", 33, 8000);
    assert_eq!(
        post["pass"].as_bool(),
        Some(true),
        "post-restore round-trip failed: {post}"
    );
    assert!(
        post["sha256_sent"].is_string() && post["sha256_sent"] == post["sha256_received"],
        "post-restore checksum mismatch: {post}"
    );
}
