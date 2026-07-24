//! Phase 6 reference topology, ported from `scripts/validate/phase6/reference.sh`
//! (design §2 reference topology, §7.4 leg transport, §9 wire/handshake).
//!
//! Two daemons in separate runtime dirs, joined over a loopback (unix) leg:
//!   * **Daemon A** (sender, `faces = target`): two echo "devices" behind `serial`
//!     nodes, each feeding one channel of a **connect**-role leg (`uplink`).
//!   * **Daemon B** (receiver, `faces = host`): a **listen**-role leg (`downlink`)
//!     whose channels fan out to local PTYs where operators sit.
//!
//! An operator on each B-side PTY sends a distinct seeded 32 KiB stream targetward;
//! it crosses B → wire → A → device (which echoes) → A → wire → B and returns. The
//! per-channel checksums must reconcile exactly — bytes traverse device ↔
//! remote-client end to end. Ground truth is the sim client's byte-exact
//! `sha256_sent`/`sha256_received`, never a judgement (§5).
//!
//! Needs two software echo *devices* for daemon A's serial nodes, so it uses
//! [`serial_echo`] twice and **skips** where no serial device exists (macOS). The leg
//! transport itself is a loopback unix socket in daemon B's runtime dir.

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};
use serde_json::Value;

/// The per-channel seeded batch size each operator drives (bash: `seeded:32KiB`).
const SIZE_32K: u64 = 32 * 1024;

/// Whether a leg node reports a live peer with **both** channels `bound` (§8/§9).
/// Mirrors the bash `.connection=="connected" and .channels.c0.binding=="bound" and
/// .channels.c1.binding=="bound"`.
fn leg_bound_both(rpc: &Rpc, name: &str) -> bool {
    let Some(node) = rpc.node(name) else {
        return false;
    };
    node.get("connection").and_then(Value::as_str) == Some("connected")
        && node.pointer("/channels/c0/binding").and_then(Value::as_str) == Some("bound")
        && node.pointer("/channels/c1/binding").and_then(Value::as_str) == Some("bound")
}

/// Drive one operator on a B-side PTY: write a seeded 32 KiB stream targetward and
/// read the device's echo back, returning the sim `client` verdict (whose
/// `sha256_sent`/`sha256_received` are the byte-exact ground truth). The full device
/// ↔ remote-client round trip for one channel.
fn operator_roundtrip(pty: &Path, seed: u64) -> Value {
    let path = pty.to_string_lossy().into_owned();
    let seed = seed.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        "seeded:32KiB",
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "15000",
    ])
}

/// Assert one channel's round-trip verdict: the batch went out whole, came back
/// whole, and the sent/received checksums reconciled end to end (§2/§9).
fn assert_roundtrip(label: &str, v: &Value) {
    assert_eq!(
        v["pass"].as_bool(),
        Some(true),
        "{label}: per-channel echo round-trip did not pass: {v}"
    );
    assert_eq!(
        v["sent"].as_u64(),
        Some(SIZE_32K),
        "{label}: did not send 32 KiB: {v}"
    );
    assert_eq!(
        v["received"].as_u64(),
        Some(SIZE_32K),
        "{label}: did not receive 32 KiB back: {v}"
    );
    assert_eq!(
        v["sha256_sent"], v["sha256_received"],
        "{label}: checksums did not reconcile end to end: {v}"
    );
}

#[test]
fn reference_topology_per_channel_roundtrip_end_to_end() {
    // Daemon A's two serial nodes each need a real echo device standing where a
    // `/dev/ttyUSB*` would; two independent software echo doubles supply them.
    let (Some(dev0), Some(dev1)) = (serial_echo(), serial_echo()) else {
        eprintln!(
            "SKIP reference_topology_per_channel_roundtrip_end_to_end: no serial device \
             on this platform"
        );
        return;
    };

    // Daemon B (receiver) first, so its listen leg is bound before A dials in.
    let d_b = Daemon::start();
    let rpc_b = d_b.rpc();
    let leg = d_b.run().join("leg.sock");
    let p0 = d_b.run().join("p0");
    let p1 = d_b.run().join("p1");
    let cfg_b = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
arbitration = "free-for-all"
channels = ["c0", "c1"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[node]]
type = "pty"
name = "p1"
path = "{p1}"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
[[edge]]
a = "downlink/c1"
b = "p1"
write_mode = "on-demand"
"#,
        leg = leg.display(),
        p0 = p0.display(),
        p1 = p1.display(),
    );
    rpc_b.load_toml(&cfg_b, false).expect("daemon B load");

    // Daemon A (sender): two echo devices behind serials, feeding a connect-role leg.
    let d_a = Daemon::start();
    let rpc_a = d_a.rpc();
    let cfg_a = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev0}"
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "usb1"
device = "{dev1}"
arbitration = "free-for-all"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "connect"
address = "{leg}"
channels = ["c0", "c1"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
[[edge]]
a = "usb1"
b = "uplink/c1"
write_mode = "on-demand"
"#,
        dev0 = dev0.device().display(),
        dev1 = dev1.device().display(),
        leg = leg.display(),
    );
    rpc_a.load_toml(&cfg_a, false).expect("daemon A load");

    // Both legs connect and bind both channels (§8/§9). Bounded polls on structured
    // state, no bare sleeps.
    assert!(
        wait_until(Duration::from_secs(10), || leg_bound_both(
            rpc_b, "downlink"
        )),
        "receiver leg never bound both channels: {:?}",
        rpc_b.node("downlink")
    );
    assert!(
        wait_until(Duration::from_secs(10), || leg_bound_both(rpc_a, "uplink")),
        "sender leg never bound both channels: {:?}",
        rpc_a.node("uplink")
    );

    // The echo devices must be open on A, else the round trip has no far end. (The
    // bash relied on the round-trip timeout; asserting active makes a failure
    // attributable to the daemon rather than a race.)
    assert!(
        rpc_a.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc_a.node("usb0")
    );
    assert!(
        rpc_a.wait_status("usb1", "active", Duration::from_secs(20)),
        "usb1 not active: {:?}",
        rpc_a.node("usb1")
    );

    // The B-side PTY symlinks must exist before an operator can attach to them.
    assert!(
        wait_until(Duration::from_secs(5), || p0.exists() && p1.exists()),
        "B-side PTY symlinks never appeared"
    );

    // An operator on each B-side PTY sends a distinct seeded stream and expects the
    // device's echo back — the full device ↔ remote-client round trip, per channel,
    // both channels concurrently (mirrors the bash's two background clients).
    let (v0, v1) = std::thread::scope(|s| {
        let h1 = s.spawn(|| operator_roundtrip(&p1, 202));
        let v0 = operator_roundtrip(&p0, 101);
        let v1 = h1.join().expect("p1 operator thread");
        (v0, v1)
    });
    assert_roundtrip("c0", &v0);
    assert_roundtrip("c1", &v1);

    // Both directions advanced on the receiver leg's channel (device data hostward,
    // commands targetward), corroborating the checksums (§7.4 counters). Bounded to
    // absorb any counter-visibility lag after the client saw its last echoed byte.
    let advanced = wait_until(Duration::from_secs(5), || {
        let Some(node) = rpc_b.node("downlink") else {
            return false;
        };
        let acc = node
            .pointer("/channels/c0/accepted_targetward")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let del = node
            .pointer("/channels/c0/delivered_hostward")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        acc >= SIZE_32K && del >= SIZE_32K
    });
    assert!(
        advanced,
        "receiver leg counters did not advance in both directions: {:?}",
        rpc_b.node("downlink")
    );
}
