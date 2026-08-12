#![forbid(unsafe_code)]

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
//! this test runs on every platform. The peer is a background `serial-nexus-sim wire` double
//! that holds the connection open while the daemon state is inspected.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, Sim, wait_until};

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

// ---- 37-LEG-3: §7.4's concurrent second connection is refused, and only it -------

/// A §9 `hello` frame in the shared envelope's wire layout: `u32 body_len | u32 magic
/// | u16 version | u32 capabilities | u16 count | count × (u16 len | UTF-8 identity)`,
/// big-endian throughout. Hand-rolled, as `p12_leg_accounting` does: the harness does
/// not depend on `serial-nexus-codec-api`, and the first peer below has to send data
/// at a moment of the test's choosing — *after* the second peer has been refused —
/// which `serial-nexus-sim wire` cannot express.
fn hello_frame(channels: &[&str]) -> Vec<u8> {
    const WIRE_MAGIC: u32 = 0x534E_584C; // "SNXL"
    const WIRE_VERSION: u16 = 1;
    let mut body = Vec::new();
    body.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
    body.extend_from_slice(&WIRE_VERSION.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes()); // capabilities
    body.extend_from_slice(&(channels.len() as u16).to_be_bytes());
    for ch in channels {
        body.extend_from_slice(&(ch.len() as u16).to_be_bytes());
        body.extend_from_slice(ch.as_bytes());
    }
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

/// A §9 envelope data frame: `u32 body_len | u8 type=0 | u16 channel_len | channel |
/// payload`, big-endian.
fn data_frame(channel: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = vec![0u8]; // type 0 = data
    body.extend_from_slice(&(channel.len() as u16).to_be_bytes());
    body.extend_from_slice(channel.as_bytes());
    body.extend_from_slice(payload);
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

/// Dial a `listen`+`unix` leg, retrying until it is *connectable* rather than merely
/// until its socket file exists: `bind(2)` creates the inode and `listen(2)` is what
/// stops `connect(2)` answering `ECONNREFUSED`, so a dial landing between them fails
/// against a daemon that is coming up perfectly normally. Bounded and condition-driven
/// rather than slept on (§5).
fn dial_leg(address: &Path) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match UnixStream::connect(address) {
            Ok(s) => return s,
            Err(e) if Instant::now() < deadline => {
                assert!(
                    matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ),
                    "dial the leg at {}: {e}",
                    address.display()
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!(
                "dial the leg at {}: {e} (still not accepting after 10 s)",
                address.display()
            ),
        }
    }
}

/// The leg's `discarded_hostward` for `console` — where wire data for a configured
/// channel with no local consumer lands (§5 counts the loss where it happens), and so
/// the observable that the first peer's stream is still being decoded and routed.
fn discarded_hostward(d: &Daemon) -> u64 {
    downlink(d)
        .pointer("/channels/console/discarded_hostward")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

#[test]
fn a_concurrent_second_peer_is_refused_while_the_first_session_keeps_flowing() {
    // §7.4: "one active peer per leg; concurrent second connections are refused."
    // The refusal arm was unit-tested only through an injected always-failing accept
    // (its backoff), never against a live listener with a real second peer and the
    // first session's data flow asserted to survive it (37-LEG-3) — and "refused"
    // has two halves that a test of the arm alone cannot separate: the newcomer is
    // closed, *and* the incumbent is untouched. A leg that tore its session down and
    // adopted the newcomer would satisfy the first half perfectly.
    //
    // Ground truth is a structured counter, never CLI text (§5): with no local
    // consumer bound, `console`'s wire data is counted as `discarded_hostward`, so
    // that counter moving is the observable form of "the first session is still being
    // decoded and routed".
    const FRAME_LEN: usize = 64;

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
channels = ["console"]
"#,
        leg = leg_sock.display(),
    );
    rpc.load_toml(&cfg, false).expect("load leg graph");
    assert!(
        wait_until(Duration::from_secs(10), || leg_sock.exists()),
        "the leg never bound its listen socket"
    );

    // The incumbent: announce, then send one command's worth of data.
    let mut first = dial_leg(&leg_sock);
    first
        .write_all(&hello_frame(&["console"]))
        .expect("write the first peer's hello");
    first
        .write_all(&data_frame("console", &[b'a'; FRAME_LEN]))
        .expect("write the first peer's data");
    first.flush().expect("flush the first peer");
    assert!(
        wait_until(Duration::from_secs(10), || {
            downlink(&d).get("connection").and_then(Value::as_str) == Some("connected")
                && discarded_hostward(&d) == FRAME_LEN as u64
        }),
        "the first peer's session never carried data: {:?}",
        downlink(&d)
    );
    let peer_address = downlink(&d).get("peer_address").cloned();

    // The newcomer. It is accepted at the socket layer — the refusal is the daemon's,
    // not the kernel's — and then closed without a handshake, so a read sees EOF. The
    // timeout is what distinguishes "refused" from "adopted as a second session": a
    // leg that kept the connection open would block here instead.
    let mut second = dial_leg(&leg_sock);
    second
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set the second peer's read timeout");
    let mut buf = [0u8; 64];
    let refused = second.read(&mut buf);
    assert!(
        matches!(&refused, Ok(0)),
        "the leg did not refuse a concurrent second peer (§7.4): {refused:?}"
    );

    // The incumbent is untouched: same connection, same peer, same binding, and no
    // reconnect was recorded — the refusal is not a session bounce.
    let dl = downlink(&d);
    assert_eq!(
        dl.get("connection").and_then(Value::as_str),
        Some("connected"),
        "the refusal disturbed the live session: {dl}"
    );
    assert_eq!(
        dl.get("peer_address").cloned(),
        peer_address,
        "the leg swapped its peer for the newcomer: {dl}"
    );
    assert_eq!(binding(&d, "console"), "bound", "{dl}");
    assert_eq!(
        dl.get("reconnect_count").and_then(Value::as_u64),
        Some(0),
        "the refusal cost the incumbent its connection: {dl}"
    );

    // …and the first session still flows *after* the refusal, which is the half a
    // test of the reject arm alone cannot see.
    first
        .write_all(&data_frame("console", &[b'b'; FRAME_LEN]))
        .expect("write the first peer's post-refusal data");
    first.flush().expect("flush the first peer again");
    assert!(
        wait_until(Duration::from_secs(10), || discarded_hostward(&d)
            == 2 * FRAME_LEN as u64),
        "the first peer's stream stopped after a second peer was refused: {:?}",
        downlink(&d)
    );
    drop(second);
    drop(first);
}
