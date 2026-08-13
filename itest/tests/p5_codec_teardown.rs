#![forbid(unsafe_code)]

//! **The codec node's half of §5's teardown ledger** (design §15.50, §5's teardown
//! ledger; plan §18 item 38).
//!
//! §15.50 clause 1 charges an interior node's queued targetward bytes at destruction
//! and names `codec` in the same breath as `map` and `exec`.
//! `p13_teardown_accounting.rs` proves it for `map`, and — since notes §3.55 — for the
//! two boundary kinds `serial` and `leg`. The kind an *author* extends the daemon
//! through had no guard at all: a codec node is where an out-of-tree transform runs
//! (§15.26), and the loss-accounting promise it inherits had nothing standing behind
//! it. Delete `TeardownLoss::drain()` from `CodecNode::signal_stop` and, until this
//! file, every test in the tree stayed green while `remove-node` destroyed a queue and
//! answered `discarded_at_teardown: 0`.
//!
//! **The shape, and why it is deterministic.** A `faces = "target"` codec whose
//! multiplexed side has no attached upstream parks each channel's targetward pump
//! inside `await_origin` — §5 forbids dropping targetward, so an unattached mux side
//! stalls its writers rather than discarding for them (the same rule that parks the
//! map's raw side). Nothing drains the queue, so a client that `send`s N bytes leaves
//! exactly N bytes in flight and every assertion below is an equality rather than a
//! threshold. `send` is RPC-acked, so "in flight" is a fact the harness observed
//! (plan §3). No device, no pts client, no codec but the built-in `reference` one:
//! this runs on every platform.
//!
//! **Delivery is witnessed, not assumed** (§9, and notes §3.55's rule for the serial
//! node). A codec node *does* count what it hands the device — `accepted_targetward`
//! per channel — so unlike the serial case the witness is a reported counter rather
//! than a state field, and the conservation law reads entirely off `state` plus the
//! reply. That makes it the stricter form, the same way the leg's is stricter than the
//! serial's.
//!
//! **Two fates, one counter each.** The last test is the discriminator: the *same*
//! bytes sent into the *same* node take the other legal path when the multiplexed edge
//! is attached-but-read-only (`write_mode = "never"`, validation's documented read-only
//! demux), where the pump drains-and-counts at run time into `discarded_targetward`
//! and the teardown ledger is correctly `0`. A counter that answered "everything I ever
//! had" rather than "what I destroyed" would pass the first two tests and fail that one
//! (§15.50 clause 2: the counter reads `0` for a node's whole working life and moves
//! exactly once, at stop).
//!
//! **Scope, stated because "the codec node is covered" would be too broad.** This is
//! the *in-process* codec node, whose whole targetward exposure is the host-facing
//! queue §15.50 clause 9 charges. The `exec` codec — the other kind an author extends
//! the daemon through — has a second internal merge stage between its per-channel
//! forwarders and the child, so its accounting is a stage-by-stage question rather than
//! this one; it belongs to plan §18 item 21 and its guards live beside the other
//! kinds' in `p13_teardown_accounting.rs`. The review-37 codec guards item 38 names
//! are asserted where they already stood rather than re-asserted here, and are listed
//! so the codec surface has one place that says where each lives:
//!
//! * *mid-chunk refusal with `accepted + discarded == total`* (37-CODEC-3) — the node
//!   half is `nodes::codec::signal_tests::a_mux_refusal_mid_chunk_charges_the_residual_and_stops_framing`;
//!   the codec half, which no suite had, is now the kit's
//!   `targetward_fragmentation_is_lossless` (a codec whose own framing unit is smaller
//!   than the interior's must fragment, never refuse).
//! * *unknown-key naming* (37-CODEC-2) — `p5_bad_attributes.rs` for the built-in exec
//!   schema, the kit's `attributes_are_structural` from the consumer position, and
//!   `p8_external_codec.rs` end to end.
//! * *reserved-identity lifecycle* (37-CODEC-4) —
//!   `nodes::exec::tests::lifecycle_events_on_the_reserved_identity_are_not_unconfigured_channels`.
//! * *quoted exec paths* (37-EXTC-1) — `p5_exec_conformance.rs`'s
//!   `a_fixture_path_containing_a_space_still_runs`; a harness property, not a codec
//!   one.
//! * *teardown-tolerant partial frame* (37-EXTC-2) — a codec holding a partial unit
//!   when the stream ends must treat "need more" as a non-error, which the kit's
//!   `fragmentation_tolerance` asserts byte for byte. Those bytes are hostward parser
//!   state and never entered the targetward queue, so §15.50 charges nothing for them:
//!   the ledger's scope is the node's targetward inbox and its one held chunk.

use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, wait_until};

/// A `faces = "target"` codec whose multiplexed side is deliberately left unwired, so
/// each channel's targetward pump parks in `await_origin` and its queue accumulates.
///
/// No edge at all — not even a consumer on the channel — because a codec's channel
/// endpoints exist from configuration (§15.35 builds the endpoint machinery per
/// endpoint, not per edge) and the bytes are injected through `send`, not typed at a
/// console.
const PARKED_CODEC_GRAPH: &str = r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
"#;

/// The same codec with its multiplexed side **attached but never writable** — the
/// read-only demux validation documents. `await_origin` answers `None` for it, so the
/// channel pump drains-and-counts at run time instead of parking: inert rather than
/// destructive, and never wedging a writer on a configuration that will never become
/// writable (§5, MAP-1).
///
/// The upstream is a serial node for a device that is not there, which keeps this
/// device-free on every platform: `write_mode = "never"` decides the pump's fate, not
/// the device's presence, so the node being `waiting` is incidental — and the read-only
/// demux is the shape `p5_resync.rs` wires with `held` instead.
fn read_only_demux_graph(device: &std::path::Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
[[edge]]
a = "usb0"
b = "mux"
write_mode = "never"
"#,
        device = device.display(),
    )
}

/// Inject `lines` writes of `line` into a host-facing endpoint and return the exact
/// number of bytes now queued for its pump.
///
/// `send` rather than a pty client for the reason `p13_teardown_accounting.rs` gives:
/// it is **RPC-acked**, so when it returns the daemon has already placed the bytes in
/// the endpoint's targetward channel, and "in flight" is observed rather than timed.
/// The byte count comes from what the verb reports, since the daemon appends the
/// newline `send` promises (§6).
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

/// The witness the whole file rests on: the node is parked, so nothing it was sent has
/// been delivered or discarded while it lived. Taken before the removal, because
/// afterwards there is nobody left to ask.
fn assert_parked_and_undelivered(node: &Value) {
    assert_eq!(
        node["status"].as_str(),
        Some("waiting"),
        "the multiplexed side must be unattached for the channel pump to park: {node}"
    );
    assert_eq!(
        node["channels"]["c0"]["accepted_targetward"].as_u64(),
        Some(0),
        "the codec handed bytes to a device it has no edge to, so `delivered == 0` is \
         no longer witnessed: {node}"
    );
    assert_eq!(
        node["channels"]["c0"]["discarded_targetward"].as_u64(),
        Some(0),
        "a parked pump has discarded nothing of its own — that is the read-only \
         demux's counter, not this one: {node}"
    );
}

/// **The ledger figure.** `remove-node --cascade` on a codec node destroys whatever
/// its channels' targetward pumps still had queued, and must say so (§15.50 clause 1).
///
/// Fail-first: with `self.teardown_loss.drain()` removed from `CodecNode::signal_stop`
/// the reply reads `discarded_at_teardown: 0` against a node that just destroyed 8 008
/// bytes, which is the silence this pins.
#[test]
fn remove_node_reports_the_targetward_bytes_a_parked_codec_destroys() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(PARKED_CODEC_GRAPH, false)
        .expect("load the parked-codec graph");

    let queued = inject(rpc, "mux/c0", &"z".repeat(1000), 8);

    // Backlog, not loss: the node still exists, and a `connect` giving it an upstream
    // would deliver every one of these bytes (§15.50 clause 2, §15.35).
    let before = rpc.node("mux").expect("codec node");
    assert_parked_and_undelivered(&before);
    assert_eq!(
        before["discarded_at_teardown"].as_u64(),
        Some(0),
        "queued bytes are backlog until the node is torn down: {before}"
    );

    let reply = rpc.remove_node("mux", true).expect("remove-node --cascade");
    assert_eq!(
        reply["discarded_at_teardown"].as_u64(),
        Some(queued),
        "the removal must report every targetward byte it destroyed (§5, §15.50): \
         {reply}"
    );
}

/// §5's conservation law across the removal, which is the property the counter exists
/// to make checkable: every byte the client sent is delivered, discarded at run time,
/// purged, or named as destroyed — and a counter that merely moved the loss to a
/// different silence would pass the test above and fail this one.
///
/// Read entirely off reported counters, with no state field standing in for a number:
/// a codec node counts what it hands the device per channel, so the equality closes
/// without the witness-by-`open: false` the serial node needs (notes §3.55).
#[test]
fn every_byte_sent_to_a_codec_channel_is_accounted_across_the_removal() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(PARKED_CODEC_GRAPH, false).expect("load");

    let queued = inject(rpc, "mux/c0", &"z".repeat(511), 6);

    let before = rpc.node("mux").expect("codec node");
    let delivered = before["channels"]["c0"]["accepted_targetward"]
        .as_u64()
        .unwrap_or(0);
    let discarded_running = before["channels"]["c0"]["discarded_targetward"]
        .as_u64()
        .unwrap_or(0);

    let reply = rpc.remove_node("mux", true).expect("remove-node --cascade");
    let destroyed = reply["discarded_at_teardown"].as_u64().unwrap_or(0);
    let purged = reply["purged_bytes"].as_u64().unwrap_or(0);

    assert_eq!(
        destroyed + purged + delivered + discarded_running,
        queued,
        "conservation across the removal: destroyed {destroyed} + purged {purged} + \
         delivered {delivered} + discarded {discarded_running} must equal the {queued} \
         sent. reply={reply} state={before}"
    );
}

/// **The discriminator: teardown loss is not the running discard.** The same bytes
/// into the same node take the other legal fate when the multiplexed edge is attached
/// but read-only: the pump drains and counts them *while the node lives*
/// (`discarded_targetward`), and the ledger correctly charges nothing at stop.
///
/// This is what makes the two guards above say something. A `discarded_at_teardown`
/// that reported "every byte this node ever had" rather than "the bytes I destroyed"
/// passes both of them and fails here — §15.50 clause 2's counter reads `0` for a
/// node's entire working life and moves exactly once.
#[test]
fn a_read_only_demux_charges_the_running_discard_and_not_the_teardown_ledger() {
    let d = Daemon::start();
    let rpc = d.rpc();
    // A path inside the run dir that is never created, so the upstream loads
    // `waiting` rather than faulting the load (§15.8) and no device is needed.
    let absent = d.run().join("no-such-device");
    rpc.load_toml(&read_only_demux_graph(&absent), false)
        .expect("load the read-only-demux graph");
    let mux = rpc.node("mux").expect("codec node");
    assert_eq!(
        mux["status"].as_str(),
        Some("active"),
        "the multiplexed edge must be *attached* for this to be the read-only demux \
         rather than the parked case above: {mux}"
    );

    let queued = inject(rpc, "mux/c0", &"z".repeat(255), 4);

    // The drain is a run-time act on the pump's own task, not an RPC-acked one, so it
    // is polled for rather than assumed — bounded, and the assertion after it is still
    // an equality.
    let drained = wait_until(Duration::from_secs(10), || {
        rpc.node("mux")
            .and_then(|n| n["channels"]["c0"]["discarded_targetward"].as_u64())
            == Some(queued)
    });
    let before = rpc.node("mux").expect("codec node");
    assert!(
        drained,
        "a read-only multiplexed edge must drain-and-count the channel's targetward \
         queue rather than park on it (§5, MAP-1): {before}"
    );
    assert_eq!(
        before["discarded_at_teardown"].as_u64(),
        Some(0),
        "the teardown ledger moves at stop and nowhere else (§15.50 clause 2): {before}"
    );

    let reply = rpc.remove_node("mux", true).expect("remove-node --cascade");
    let destroyed = reply["discarded_at_teardown"].as_u64().unwrap_or(0);
    assert_eq!(
        destroyed, 0,
        "the queue was already drained and counted at run time, so the teardown \
         destroyed nothing — charging it again would report one loss under two names \
         (§5's never-sum rule): {reply}"
    );
    // …and the same conservation law still closes, with the whole figure sitting in
    // the other counter: one loss, named once (§5's never-sum rule).
    let purged = reply["purged_bytes"].as_u64().unwrap_or(0);
    let discarded_running = before["channels"]["c0"]["discarded_targetward"]
        .as_u64()
        .unwrap_or(0);
    assert_eq!(
        destroyed + purged + discarded_running,
        queued,
        "conservation across the removal of a read-only demux: destroyed {destroyed} \
         + purged {purged} + discarded-at-run-time {discarded_running} must equal the \
         {queued} sent. reply={reply} state={before}"
    );
}
