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

// ---- LEG-2: a hostile peer cannot grow the `unbound` list without bound ---------

/// The leg's `unbound_overflow` — occurrences the cap refused to record (§8, LEG-2).
fn unbound_overflow(d: &Daemon) -> u64 {
    downlink(d)
        .get("unbound_overflow")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Every channel identity `state` reports for the leg, with its binding.
fn channel_bindings(d: &Daemon) -> Vec<(String, String)> {
    downlink(d)
        .get("channels")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.get("binding")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_peer_streaming_fresh_channel_identities_cannot_grow_the_unbound_list() {
    // LEG-2. `unbound` is an operator prompt ("the peer offers `console-c`, you have
    // not configured it"), and it used to be an uncapped `Vec<String>` appended to —
    // after a linear dedup scan — for **every data frame on an unconfigured
    // channel**. A peer inventing identities therefore drove unbounded memory growth
    // and O(n²) CPU on the single runtime thread. Hostile peers are in scope by this
    // repo's own standard (`p6_hostility`), and a `listen`+`unix` leg is dialable by
    // anyone who can reach its path.
    //
    // The cap and the per-identity length limit are the daemon's; this test asserts
    // only what `state` promises an operator, so it pins the behavior rather than the
    // constants: the list stops growing, the refusals are *visible* rather than
    // silent, and no stored identity is longer than the truncation marker allows.
    // Needs no serial device — runs on every platform.
    const CAP: usize = 256; // MAX_UNBOUND (leg.rs)
    const ID_CAP: usize = 64; // MAX_UNBOUND_ID_LEN
    const MARKER: &str = "…(truncated)";
    const FLOOD: usize = 300;

    let d = Daemon::start();
    let rpc = d.rpc();
    let leg_sock = d.run().join("leg.sock");
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
    assert!(
        wait_until(Duration::from_secs(10), || leg_sock.exists()),
        "the leg never bound its listen socket"
    );

    // One over-long identity first (so it lands inside the cap and its storage can be
    // inspected), then a flood of short fresh ones — 301 unconfigured identities in
    // total against a 256-entry list.
    let long_id = "z".repeat(200);
    let mut args: Vec<String> = vec![
        "wire".into(),
        "--transport".into(),
        "unix".into(),
        "--address".into(),
        leg_sock.to_string_lossy().into_owned(),
        "--announce".into(),
        "console".into(),
        "--send".into(),
        format!("{long_id}=4"),
    ];
    for i in 0..FLOOD {
        args.push("--send".into());
        args.push(format!("flood-{i:04}=4"));
    }
    // The peer holds the connection open far longer than the bounded waits below
    // need: binding state is per-connection (§8), so `unbound` is cleared the moment
    // the peer leaves and every assertion here has to run while it is still there.
    args.extend([
        "--hold-ms".into(),
        "25000".into(),
        "--timeout-ms".into(),
        "35000".into(),
    ]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let _peer = Sim::spawn(&argv, None);

    // Every identity past the cap is refused *and counted*: 1 + 300 offered, 256
    // recorded. The count is what keeps "we stopped remembering" from being silent.
    let extra = (1 + FLOOD - CAP) as u64;
    assert!(
        wait_until(Duration::from_secs(15), || unbound_overflow(&d) == extra),
        "the leg did not report the refused identities (want {extra}): overflow={} \
         channels={}",
        unbound_overflow(&d),
        channel_bindings(&d).len()
    );

    // The list itself stopped at the cap, and the two configured channels are still
    // reported alongside it (announcements never grow the graph, §8).
    let bindings = channel_bindings(&d);
    let unbound: Vec<&(String, String)> = bindings.iter().filter(|(_, b)| b == "unbound").collect();
    assert_eq!(
        unbound.len(),
        CAP,
        "the unbound list grew past its cap ({} entries)",
        unbound.len()
    );
    assert_eq!(
        bindings.len(),
        CAP + 2,
        "state reports something other than the cap plus the two configured channels: {}",
        bindings.len()
    );
    assert_eq!(
        bindings
            .iter()
            .find(|(k, _)| k == "console")
            .map(|(_, b)| b.as_str()),
        Some("bound"),
        "the flood disturbed the configured `console` binding"
    );
    assert_eq!(
        bindings
            .iter()
            .find(|(k, _)| k == "trace")
            .map(|(_, b)| b.as_str()),
        Some("waiting"),
        "the flood disturbed the configured `trace` binding"
    );

    // No stored identity is unbounded in length either — the over-long one is kept
    // truncated *and marked*, so `state` never implies the peer sent the short name.
    let longest = unbound.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    assert!(
        longest <= ID_CAP + MARKER.len(),
        "a stored identity is {longest} bytes, past the {ID_CAP}+marker bound"
    );
    assert!(
        !bindings.iter().any(|(k, _)| *k == long_id),
        "the leg stored the peer's 200-byte identity verbatim"
    );
    let truncated = unbound
        .iter()
        .find(|(k, _)| k.contains(MARKER))
        .unwrap_or_else(|| panic!("no truncated identity in state: {:?}", unbound.len()));
    assert!(
        truncated.0.starts_with("zzzz") && truncated.0.ends_with(MARKER),
        "the truncated identity is neither recognisable nor marked: {:?}",
        truncated.0
    );
}
