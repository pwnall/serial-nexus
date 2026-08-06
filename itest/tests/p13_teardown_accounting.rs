//! §5's "all loss is visible" at the one moment the daemon used to break it: node
//! **teardown** (notes §3.31, design §5, §11, §15.35).
//!
//! `runtime.rs`'s `TargetwardLoss` states the rule these tests pin, verbatim:
//! *"Targetward is the direction §5 forbids dropping on, so **every** non-delivery
//! exit of **every** pump has to reach a counter."* The type exists because that
//! obligation "was a review convention rather than a compiler rule … and twice
//! shipped with an exit that charged nothing". This was the third instance, and it
//! evaded the type entirely: an interior node's targetward pump owned its
//! `mpsc::Receiver` inside the spawned future, so `TaskSet::abort_all` dropped the
//! receiver **and every chunk queued in it** without the pump taking any exit at all.
//! The pump bodies were scrupulous; they simply never got another turn.
//!
//! Measured on the shipped daemon before the fix, one `remove-node --cascade` on a
//! saturated map: **808 448 bytes in flight, 23 042 accounted, every node counter 0**.
//!
//! **Why these are device-free and deterministic.** Each graph here puts a node's
//! targetward pump in a state where §5 requires it to *stall* rather than drain:
//!
//! * a **map** whose *raw* (upstream) side is unattached parks inside `await_origin`
//!   — §5 forbids dropping targetward, so a detached edge must stall its writers
//!   rather than discard for them;
//! * a **serial** node whose device is not present is `waiting`, and §7.1's whole
//!   answer to an absent device is that its origins backpressure (the channel fills)
//!   instead of losing their commands;
//! * a **leg** whose peer has never connected has an unbound channel, and `next_send`
//!   skips an unbound channel by design ("waiting: open but deliberately not
//!   drained"), for the same reason.
//!
//! Nothing drains the queue in any of the three, so a client that `send`s N bytes
//! leaves exactly N bytes in flight, and the assertions are equalities rather than
//! thresholds. The bytes go in through `send`, which is RPC-acked, so "in flight" is a
//! fact the harness observed rather than a timing assumption. No serial device and no
//! pts client is involved, so these run on every platform.
//!
//! **The two boundary kinds arrived late, and that is the finding.** `serial` and
//! `leg` were named in §15.50 as owning queues of the same shape and reporting nothing
//! — deliberately, because a counter reading `0` while bytes are destroyed is worse
//! than silence — and closed in notes §3.55. The serial case is the sharper of the
//! two: the deepest backlog the daemon can accumulate is the one a `waiting` node
//! builds *by design*, and it was the one nothing counted.

use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::Daemon;

/// A map whose raw side is deliberately left unwired, so its targetward pump parks.
/// The pty gives the mapped endpoint a consumer edge (§4 rule 2) and is otherwise
/// inert here — the bytes are injected through `send`, not typed at the console.
fn parked_map_graph(p0: &std::path::Path) -> String {
    format!(
        r#"
[[node]]
type = "map"
name = "console"
targetward = ["lfcrlf"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "console"
b = "p0"
"#,
        p0 = p0.display(),
    )
}

/// A serial node whose device is not present, so it comes up `waiting` and its
/// targetward queue is never drained (§7.1: an absent device backpressures its origins,
/// it does not lose their commands). No edge and no device — `waiting` is a legal load
/// outcome (§15.8), which is what makes this device-free on every platform.
///
/// `purge_on_reconnect` is left at its default because there is no reconnect here: the
/// device never appears, so the one sanctioned targetward drain never runs and every
/// queued byte is teardown loss and nothing else.
fn waiting_serial_graph(device: &std::path::Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
"#,
        device = device.display(),
    )
}

/// A `faces = host` leg listening on a unix socket no peer ever dials, so its channel
/// stays unbound and `next_send` deliberately does not drain it (§7.4). One channel,
/// no edge, no device.
fn peerless_leg_graph(sock: &std::path::Path) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "unix"
role = "listen"
address = "{sock}"
channels = ["c0"]
"#,
        sock = sock.display(),
    )
}

/// Inject `lines` writes of `line` through a host-facing endpoint and return
/// the exact number of bytes now queued for its parked pump.
///
/// `send` is used rather than a pty client on purpose: it is **RPC-acked**, so when
/// it returns the daemon has already placed the bytes in the endpoint's targetward
/// channel. That turns "in flight" from a timing assumption into a fact the harness
/// observed, which is what lets the assertions below be equalities. A pty client
/// would leave the bytes in a kernel buffer the test cannot see, and the counter it
/// is checking would then be racing the reader's poll.
///
/// The daemon appends the newline `send` promises (§6, one line targetward), so the
/// queued length is measured from what the verb reports rather than assumed here.
fn inject(rpc: &serial_nexus_itest::Rpc, endpoint: &str, line: &str, lines: usize) -> u64 {
    let mut queued = 0u64;
    for i in 0..lines {
        let ack = rpc
            .send(endpoint, line, false, 5_000)
            .unwrap_or_else(|e| panic!("send #{i}: [{}] {}", e.code, e.message));
        queued += ack
            .get("sent")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("send #{i} did not report its byte count: {ack}"));
    }
    queued
}

/// **The defect.** `remove-node --cascade` on an interior node destroys whatever its
/// targetward pump still had queued, and must say so.
///
/// Fail-first: with `TeardownLoss::drain()` removed from `MapNode::signal_stop` the
/// reply reads `discarded_at_teardown: 0` against a graph that just destroyed 32 768
/// bytes, which is the shipped behaviour this pins.
#[test]
fn remove_node_reports_the_targetward_bytes_it_destroys() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let p0 = d.run().join("p0");
    rpc.load_toml(&parked_map_graph(&p0), false)
        .expect("load the parked-map graph");
    assert!(
        rpc.wait_status("p0", "active", Duration::from_secs(10)),
        "p0 not active: {:?}",
        rpc.node("p0")
    );
    // `waiting`, not `active`: the raw side is unwired, which is what parks the pump.
    assert_eq!(
        rpc.node("console")
            .and_then(|v| v["status"].as_str().map(str::to_owned)),
        Some("waiting".to_owned()),
        "the map's raw side must be unattached for its pump to park: {:?}",
        rpc.node("console")
    );
    let queued = inject(rpc, "console", &"z".repeat(999), 8);

    // Nothing has been discarded yet — the bytes are backlog, not loss, while the
    // node lives and could still be wired up (`connect`).
    let before = rpc.node("console").expect("map node");
    assert_eq!(
        before["targetward"]["discarded_at_teardown"].as_u64(),
        Some(0),
        "queued bytes are backlog until the node is torn down: {before}"
    );

    let reply = rpc
        .remove_node("console", true)
        .expect("remove-node --cascade");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "the removal must report every targetward byte it destroyed (§5): {reply}"
    );
}

/// §5's conservation law across the removal, which is the property the counter
/// exists to make checkable: every byte the client typed is either still accounted
/// somewhere or named as destroyed. A counter that merely moved the loss to a
/// different silence would pass the test above and fail this one.
#[test]
fn every_typed_byte_is_accounted_across_the_removal() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let p0 = d.run().join("p0");
    rpc.load_toml(&parked_map_graph(&p0), false).expect("load");
    assert!(
        rpc.wait_status("p0", "active", Duration::from_secs(10)),
        "p0 not active"
    );
    let queued = inject(rpc, "console", &"z".repeat(511), 5);

    let reply = rpc
        .remove_node("console", true)
        .expect("remove-node --cascade");
    let destroyed = reply["discarded_at_teardown"].as_u64().unwrap_or(0);
    let purged = reply["purged_bytes"].as_u64().unwrap_or(0);
    let pty = rpc
        .node("p0")
        .and_then(|v| v["discarded_targetward"].as_u64())
        .unwrap_or(0);

    assert_eq!(
        destroyed + purged + pty,
        queued,
        "conservation across the removal: destroyed {destroyed} + purged {purged} + \
         pty {pty} must equal the {queued} queued. reply={reply} p0={:?}",
        rpc.node("p0")
    );
}

/// The counter names *teardown* loss and must not be confused with the map's
/// running-time discard, which has its own name and its own meaning (§5's counters
/// name the loss they carry). A read-only map swallows bytes it *looked at*;
/// `discarded_at_teardown` names bytes it never got to look at.
#[test]
fn teardown_loss_is_not_folded_into_the_running_discard() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let p0 = d.run().join("p0");
    rpc.load_toml(&parked_map_graph(&p0), false).expect("load");
    assert!(
        rpc.wait_status("p0", "active", Duration::from_secs(10)),
        "p0 not active"
    );
    let queued = inject(rpc, "console", &"z".repeat(255), 3);

    let node = rpc.node("console").expect("map node");
    assert_eq!(
        node["targetward"]["discarded_no_raw_edge"].as_u64(),
        Some(0),
        "a parked pump has discarded nothing of its own: {node}"
    );
    let reply = rpc.remove_node("console", true).expect("remove");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "the whole backlog is teardown loss, not running discard: {reply}"
    );
}

// --- the two boundary kinds §15.50 named and notes §3.55 closed ------------------

/// **The serial half of the defect.** A `waiting` serial node's targetward queue is
/// the daemon's own answer to an absent device (§5/§7.1: backpressure the origins,
/// never drop) — so it is the deepest backlog the daemon can legally hold, and until
/// notes §3.55 `remove-node` destroyed it while answering `discarded_at_teardown: 0`.
///
/// Fail-first: with `TeardownLoss::drain()` removed from `SerialNode::signal_stop` the
/// reply reads `discarded_at_teardown: 0` against a node that just destroyed 8 016
/// bytes, which is the shipped silence this pins.
#[test]
fn remove_node_reports_the_targetward_bytes_a_waiting_serial_destroys() {
    let d = Daemon::start();
    let rpc = d.rpc();
    // A path inside the run dir that is never created: `resolve_current_path` finds
    // nothing, so the node loads `waiting` rather than faulting the load (§15.8).
    let absent = d.run().join("no-such-device");
    rpc.load_toml(&waiting_serial_graph(&absent), false)
        .expect("load the waiting-serial graph");
    let before = rpc.node("usb0").expect("serial node");
    assert_eq!(
        before["status"].as_str(),
        Some("waiting"),
        "the device must be absent for the queue to accumulate: {before}"
    );
    assert_eq!(
        before["open"].as_bool(),
        Some(false),
        "a node with an open port would drain the queue under the test: {before}"
    );

    let queued = inject(rpc, "usb0", &"z".repeat(1001), 8);

    // Backlog, not loss: the node still exists and the device may still appear, in
    // which case every one of these bytes is delivered (§7.1 faulted-and-wait).
    let before = rpc.node("usb0").expect("serial node");
    assert_eq!(
        before["discarded_at_teardown"].as_u64(),
        Some(0),
        "queued bytes are backlog until the node is torn down: {before}"
    );

    let reply = rpc.remove_node("usb0", true).expect("remove-node");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "the removal must report every targetward byte it destroyed (§5): {reply}"
    );
}

/// §5's conservation law for the serial node, as an equality over *reported* counters:
/// every byte the client sent is either delivered to the device, purged by §6's one
/// sanctioned drain, or named as destroyed.
///
/// Delivery is pinned to zero by a witness rather than by assumption — the node is
/// `waiting` with `open: false` right up to the removal, and a node with no port has
/// written nothing. That is what makes `accepted == delivered + purged + destroyed`
/// checkable here at all: a serial node has no "bytes written to the device" counter,
/// so the zero has to come from the port's own state.
#[test]
fn every_byte_sent_to_a_waiting_serial_is_accounted_across_the_removal() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let absent = d.run().join("no-such-device");
    rpc.load_toml(&waiting_serial_graph(&absent), false)
        .expect("load");
    let queued = inject(rpc, "usb0", &"z".repeat(511), 5);

    // The witness for "delivered == 0", taken while the node still exists.
    let before = rpc.node("usb0").expect("serial node");
    assert_eq!(
        before["open"].as_bool(),
        Some(false),
        "the port opened under the test, so `delivered == 0` is no longer witnessed: \
         {before}"
    );
    let purged_before = before["purged_on_reconnect"].as_u64().unwrap_or(0);
    assert_eq!(
        purged_before, 0,
        "nothing may be purged without a reconnect: {before}"
    );

    let reply = rpc.remove_node("usb0", true).expect("remove-node");
    let destroyed = reply["discarded_at_teardown"].as_u64().unwrap_or(0);
    let purged = reply["purged_bytes"].as_u64().unwrap_or(0);

    assert_eq!(
        destroyed + purged + purged_before,
        queued,
        "conservation across the removal: destroyed {destroyed} + purged {purged} + \
         purged-on-reconnect {purged_before} must equal the {queued} accepted, with \
         delivery witnessed at zero by `open: false`. reply={reply}"
    );
}

/// **The leg half of the defect.** A `faces = host` leg whose peer has never connected
/// holds its channel unbound and deliberately does not drain it (§7.4) — so a peerless
/// leg accumulates exactly the same shape of backlog a `waiting` serial does, and until
/// notes §3.55 destroyed it in the same silence.
///
/// The per-channel figure is asserted as well as the node-level one, because §5 asks
/// for loss that is *attributable*: a leg that reported one number for eight channels
/// would tell an operator what was lost and not where.
///
/// Fail-first: with the `channel_teardown` drain removed from `LegNode::signal_stop`
/// the reply reads `discarded_at_teardown: 0` against a leg that just destroyed 4 040
/// bytes.
#[test]
fn remove_node_reports_the_targetward_bytes_a_peerless_leg_destroys() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let sock = d.run().join("uplink.sock");
    rpc.load_toml(&peerless_leg_graph(&sock), false)
        .expect("load the peerless-leg graph");
    let before = rpc.node("uplink").expect("leg node");
    assert_eq!(
        before["channels"]["c0"]["binding"].as_str(),
        Some("waiting"),
        "the channel must be unbound for the queue to accumulate: {before}"
    );

    let queued = inject(rpc, "uplink/c0", &"z".repeat(504), 8);

    let before = rpc.node("uplink").expect("leg node");
    assert_eq!(
        before["channels"]["c0"]["discarded_at_teardown"].as_u64(),
        Some(0),
        "queued bytes are backlog until the node is torn down: {before}"
    );
    assert_eq!(
        before["discarded_at_teardown"].as_u64(),
        Some(0),
        "the node-level figure must agree with its channels: {before}"
    );

    let reply = rpc.remove_node("uplink", true).expect("remove-node");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "the removal must report every targetward byte it destroyed (§5): {reply}"
    );
}

/// §5's conservation law for the leg, as an equality over *reported* counters and with
/// no witness needed: a leg counts what it puts on the wire (`accepted_targetward`) and
/// what §6's purge took (`purged_on_reconnect`), so the whole law is readable from
/// `state` plus the reply.
///
/// This is the stricter of the two boundary conservation guards for exactly that
/// reason, and it is the shape §9 asks for — the portable form coming out stricter,
/// not weaker.
#[test]
fn every_byte_sent_to_a_peerless_leg_is_accounted_across_the_removal() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let sock = d.run().join("uplink.sock");
    rpc.load_toml(&peerless_leg_graph(&sock), false)
        .expect("load");
    let queued = inject(rpc, "uplink/c0", &"z".repeat(255), 6);

    let before = rpc.node("uplink").expect("leg node");
    let ch = &before["channels"]["c0"];
    let on_the_wire = ch["accepted_targetward"].as_u64().unwrap_or(0);
    let purged_before = ch["purged_on_reconnect"].as_u64().unwrap_or(0);

    let reply = rpc.remove_node("uplink", true).expect("remove-node");
    let destroyed = reply["discarded_at_teardown"].as_u64().unwrap_or(0);
    let purged = reply["purged_bytes"].as_u64().unwrap_or(0);

    assert_eq!(
        destroyed + purged + on_the_wire + purged_before,
        queued,
        "conservation across the removal: destroyed {destroyed} + purged {purged} + \
         on-the-wire {on_the_wire} + purged-on-reconnect {purged_before} must equal \
         the {queued} accepted. reply={reply} state={before}"
    );
}

// --- the third destroying verb: `load --replace` ---------------------------------

/// **`load --replace` is a teardown, and it must say what the teardown cost** (§5,
/// §15.50 as annotated by notes §3.59).
///
/// §15.50 named the `remove-node` and `teardown` replies because those were the two
/// verbs that destroyed nodes when it was written. `load --replace` is the third, and
/// it is the one whose loss is *largest* — it destroys the entire graph, not one node —
/// and the one an operator reaches for without the word "teardown" in front of them.
/// It called `teardown_with` as a statement, so the `{torn_down, discarded_at_teardown}`
/// it computed was dropped on the floor and the reply read `{"loaded": 1}`. A destroyed
/// node's last loss can land nowhere else: `state` has nobody left to ask.
///
/// The graph is the `waiting` serial one for the reason its own guard gives — an absent
/// device is §7.1's deepest legal backlog and the deterministic one — and the same
/// config is loaded back over itself, because what is under test is the reply, not the
/// difference between two configs.
///
/// Fail-first (the shipped shape restored — `self.teardown_with(…)` as a statement and
/// `json!({ "loaded": st.nodes.len() })` as the reply):
/// `load --replace destroyed 8008 targetward bytes and reported none of them:
/// {"loaded":1}`.
#[test]
fn load_replace_reports_the_targetward_bytes_it_destroys() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let absent = d.run().join("no-such-device");
    let cfg = waiting_serial_graph(&absent);

    // Kept for the always-present check at the bottom. The order is deliberate: the
    // headline assertion — the one the loss figure lives or dies on — is made first, so
    // a run against an unfixed daemon fails with *that* message rather than with the
    // secondary one it also happens to break.
    let first = rpc
        .load_toml(&cfg, false)
        .expect("load the waiting-serial graph");

    let before = rpc.node("usb0").expect("serial node");
    assert_eq!(
        before["status"].as_str(),
        Some("waiting"),
        "the device must be absent for the queue to accumulate: {before}"
    );
    let queued = inject(rpc, "usb0", &"z".repeat(1000), 8);

    let reply = rpc.load_toml(&cfg, true).expect("load --replace");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "load --replace destroyed {queued} targetward bytes and reported none of them: \
         {reply}"
    );
    assert_eq!(
        reply["torn_down"].as_u64(),
        Some(1),
        "the loss figure needs the node count beside it to be readable — a `0` with \
         nothing next to it is the defect notes §3.50 names: {reply}"
    );
    assert_eq!(
        reply["loaded"].as_u64(),
        Some(1),
        "the replacement graph did not load: {reply}"
    );

    // The successor is a fresh node, not the old one with its backlog: the figure above
    // is loss, and loss that was quietly carried over would be delivered later instead.
    let after = rpc.node("usb0").expect("the replacement serial node");
    assert_eq!(
        after["discarded_at_teardown"].as_u64(),
        Some(0),
        "the replacement node inherited its predecessor's ledger: {after}"
    );

    // The always-present rule, on the arm that tore nothing down: a `load` without
    // `replace` answers the honest `0`/`0` rather than omitting the fields, so a client
    // never learns to read absent as none (§15.50).
    assert_eq!(
        (
            first["torn_down"].as_u64(),
            first["discarded_at_teardown"].as_u64()
        ),
        (Some(0), Some(0)),
        "a non-replace load must still carry the pair, at zero: {first}"
    );
}
