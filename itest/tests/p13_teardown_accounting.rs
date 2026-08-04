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
//! **Why these are device-free and deterministic.** A map whose *raw* (upstream) side
//! is unattached parks its targetward pump inside `await_origin` — §5 forbids
//! dropping targetward, so a detached edge must stall its writers rather than
//! discard for them. Nothing drains the queue while it is parked, so a client that
//! `send`s N bytes leaves exactly N bytes in flight, and the assertions are equalities
//! rather than thresholds. The bytes go in through `send`, which is RPC-acked, so
//! "in flight" is a fact the harness observed rather than a timing assumption. No
//! serial device and no pts client is involved, so these run on every platform.

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

/// Inject `lines` writes of `line` through the map's host-facing endpoint and return
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
