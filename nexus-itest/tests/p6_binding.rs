//! Phase 6 binding, ported from `scripts/validate/phase6/binding.sh`
//! (design §8 binding, §7.4 leg lifecycle, §9 the wire hello): announcements never
//! mutate the graph.
//!
//! A receiving leg (`faces = host`, `role = listen`, unix transport) is configured
//! with two channels (`console`, `trace`). A peer dials it and announces
//! `{console, extra}` in its hello. The leg reconciles the announcement against its
//! configured channels into three bindings (§8):
//!   * `console` — configured AND announced → **bound**.
//!   * `trace`   — configured, NOT announced → **waiting** (faulted-and-wait).
//!   * `extra`   — announced, NOT configured → **unbound** (visible state, no endpoint).
//!
//! And the graph never grows from an announcement: the node count is unchanged, the
//! unbound channel carries no endpoint (no `lock`), while a configured channel does.
//!
//! A leg needs no serial *device* (the transport here is a loopback unix socket), so
//! this test runs on every platform. The peer is a background `nexus-sim wire` double
//! that holds the connection open while the daemon state is inspected.

use std::time::Duration;

use nexus_itest::{Daemon, Sim, wait_until};
use serde_json::Value;

/// The `downlink` leg node from `state`, or `Value::Null` if absent.
fn downlink(d: &Daemon) -> Value {
    d.rpc().node("downlink").unwrap_or(Value::Null)
}

/// A channel object's `binding` string on the `downlink` leg (`""` if missing).
fn binding(d: &Daemon, channel: &str) -> String {
    downlink(d)
        .pointer(&format!("/channels/{channel}/binding"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

#[test]
fn peer_announcement_reconciles_bindings_without_growing_the_graph() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let leg_sock = d.run().join("leg.sock");

    // A receiving leg configured with two channels (console, trace) over a loopback
    // unix socket in the listen role.
    let cfg = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
arbitration = "free-for-all"
channels = ["console", "trace"]
"#,
        leg = leg_sock.display(),
    );
    rpc.load_toml(&cfg, false).expect("load leg graph");

    // Node count before the peer connects.
    let before = rpc.state()["nodes"].as_array().unwrap().len();

    // The peer announces {console, extra}: console is configured (→ bound), trace is
    // configured-but-unannounced (→ waiting), extra is announced-but-unconfigured
    // (→ unbound). Hold the connection open while we inspect (§9 hold-ms).
    let leg_str = leg_sock.to_string_lossy().into_owned();
    let _wire = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &leg_str,
            "--announce",
            "console",
            "--announce",
            "extra",
            "--hold-ms",
            "4000",
            "--timeout-ms",
            "5000",
        ],
        None,
    );

    // Bounded wait for the leg to accept the peer and complete the handshake — the
    // binding reconciliation runs synchronously with reaching `connected`.
    let connected = wait_until(Duration::from_secs(5), || {
        downlink(&d).get("connection").and_then(Value::as_str) == Some("connected")
    });
    assert!(
        connected,
        "leg never connected: {:?}",
        downlink(&d).get("connection")
    );

    // Binding reconciliation (§8).
    assert_eq!(
        binding(&d, "console"),
        "bound",
        "console should be bound (configured + announced)"
    );
    assert_eq!(
        binding(&d, "trace"),
        "waiting",
        "trace should be waiting (configured, not announced)"
    );
    assert_eq!(
        binding(&d, "extra"),
        "unbound",
        "extra should be unbound (announced, not configured)"
    );

    // Announcements never grow the graph: node count unchanged.
    let after = rpc.state()["nodes"].as_array().unwrap().len();
    assert_eq!(
        before, after,
        "node count changed from announcements ({before} -> {after})"
    );

    // The unbound channel exists only as state — no endpoint, hence no lock (§8).
    let dl = downlink(&d);
    assert!(
        dl.pointer("/channels/extra/lock").is_none(),
        "an unbound channel must have no endpoint/lock (§8): {:?}",
        dl.pointer("/channels/extra")
    );
    // A configured (bound/waiting) channel DOES carry its host-facing lock.
    assert!(
        dl.pointer("/channels/console/lock").is_some(),
        "a configured channel must carry its host-facing lock: {:?}",
        dl.pointer("/channels/console")
    );
}
