//! The **leg's** oversize-chunk fragmentation, end to end (design §15.24, §9 clause 5,
//! AGENTS.md invariant 3). Review 32 WIRER-3.
//!
//! §15.24 has said since v6 that fragmentation "is contract text now, with a 100 001-byte
//! `send` round-trip as its regression guard". That guard did not exist. Invariant 3 is
//! not test-free — `runtime.rs`'s `data_frames_*` unit tests pin the shared helper and
//! `codec.rs`'s `targetward_oversize_chunk_is_fragmented_never_dropped` pins the
//! in-process codec's use of it — but the *leg's* use of it (`next_send` →
//! [`data_frames`] → `write_all` → the peer's `FrameDecoder` reassembly → `route_recv` →
//! local delivery) had never been crossed by a test: the whole `p6_*` family moves 32 KiB
//! per channel, comfortably under `frame_payload_cap`, so no leg test had ever produced a
//! second frame. Both of the clause-3 defects this release fixed (LEG-2, WIRE-2) live on
//! exactly that path.
//!
//! Two tests, for the two halves of invariant 3:
//!
//! 1. [`a_hundred_thousand_byte_send_crosses_the_leg_wire_byte_exactly`] — "fragment,
//!    never skip". One `send` of 100 001 bytes over a loopback **unix** leg between two
//!    daemons, with the receiving side attached to a `log` — the one *lossless* sink
//!    (§7.3), so the arrival oracle is a byte-exact SHA-256 of a file and never a
//!    judgement (§5). The payload cycles a 64-character alphabet, so a reordered or
//!    duplicated fragment changes the digest; an all-one-byte filler would not.
//! 2. [`the_untransmitted_tail_is_charged_when_the_peer_dies_mid_chunk`] — "count any
//!    residual", the quieter half and the one LEG-2 was. The peer is a `nexus-sim wire
//!    --stall` double: it completes the handshake, announces the channel and then **never
//!    reads**, so the daemon's write half backs up into the socket buffer and parks inside
//!    `write_all` holding a chunk it has taken out of its bounded receiver. Killing it
//!    there is the exact shape LEG-2 lost bytes in, and `leg.rs`'s `discarded_peer_gone`
//!    is what must now hold the untransmitted tail.
//!
//! **Device-free on purpose.** Everything here is a leg, a log and a unix socket, so —
//! unlike every other `p6_*` file, which needs a software serial echo device — this one
//! runs on macOS too (§5).
//!
//! **The topology is host↔host, which is deliberate and is not the reference topology.**
//! §2's reference shape pairs a `faces = "host"` leg with a `faces = "target"` one, and
//! `p6_reference.rs` covers that. Here the far side must land its arrival in a `log`, and
//! a log is a host-side consumer: it can only attach *below* a host-facing endpoint. So
//! the far leg faces `host` as well, which makes its channel a host-facing endpoint the
//! log hangs off, and the wire — which carries no direction, only channel identities and
//! bytes (§9) — joins the near side's targetward to the far side's hostward.

use std::path::Path;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, TempRun, sha256_hex, wait_until};
use serde_json::Value;

/// Design §15.24's number, split into its two parts: 100 000 payload characters, plus
/// the newline the `send` verb appends.
const PAYLOAD: usize = 100_000;
const CHUNK: u64 = PAYLOAD as u64 + 1;

/// `codec_api::MAX_FRAME_SIZE`, restated as a literal rather than depended on: this
/// crate drives the daemon over its RPC surface only, and a dependency edge from the
/// harness into the wire crate costs more than one number does.
///
/// **What keeps it honest is not this file.** If the wire's frame size ever grew past
/// 100 001, this test would stop fragmenting and would still pass — so the guard against
/// that is elsewhere and is real: §9's golden vectors are byte-frozen across wire
/// evolution, and `runtime.rs`'s `frame_payload_cap_reserves_the_envelope_header` pins
/// the cap against `MAX_FRAME_SIZE` directly. Anyone raising it has to walk past both,
/// and should raise the number here with them.
const MAX_FRAME_SIZE: u64 = 64 * 1024;

/// One `send` must be bigger than one frame, or this whole file measures nothing.
/// Checked at compile time, so it cannot be a test that quietly stops fragmenting.
const _: () = assert!(
    CHUNK > MAX_FRAME_SIZE,
    "a 100 001-byte send fits in one frame — §15.24's guard would never fragment"
);

/// A position-sensitive ASCII payload: byte *i* is `ALPHABET[i % 64]`, so a fragment
/// delivered twice, out of order, or short changes the SHA-256. A run of one repeated
/// byte would hide all three.
fn payload() -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    (0..PAYLOAD)
        .map(|i| ALPHABET[i % ALPHABET.len()] as char)
        .collect()
}

/// The near (sending) daemon's graph: one `faces = "host"` leg listening on a loopback
/// unix socket. `send` on a host-facing endpoint writes targetward, which for this
/// facing is the direction that goes onto the wire.
fn near_config(sock: &Path) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{sock}"
arbitration = "free-for-all"
channels = ["c0"]
"#,
        sock = sock.display(),
    )
}

/// A channel counter out of `state` (`leg.rs`'s `ChannelStat`), or 0 when the node or
/// the field is absent.
fn stat(rpc: &Rpc, node: &str, ch: &str, field: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/{field}"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

/// Whether the leg has a live peer that has announced `ch` (§8/§9).
fn bound(rpc: &Rpc, node: &str, ch: &str) -> bool {
    rpc.node(node)
        .map(|n| {
            n.get("connection").and_then(Value::as_str) == Some("connected")
                && n.pointer(&format!("/channels/{ch}/binding"))
                    .and_then(Value::as_str)
                    == Some("bound")
        })
        .unwrap_or(false)
}

#[test]
fn a_hundred_thousand_byte_send_crosses_the_leg_wire_byte_exactly() {
    // --- near: the sender. Its leg listens; `send` puts bytes on the wire.
    let near = Daemon::start();
    let rpc_n = near.rpc();
    let sock = near.run().join("leg.sock");
    rpc_n
        .load_toml(&near_config(&sock), false)
        .expect("near daemon load");
    assert!(
        rpc_n.wait_status("downlink", "waiting", Duration::from_secs(10))
            || rpc_n.wait_status("downlink", "active", Duration::from_secs(10)),
        "the listening leg never came up: {:?}",
        rpc_n.node("downlink")
    );

    // --- far: the receiver. Its leg dials the same socket and hands what arrives to a
    //     `log`, the only sink the design promises is lossless (§7.3) — which is what
    //     lets the arrival assertion be a digest rather than a judgement.
    let far = Daemon::start();
    let rpc_f = far.rpc();
    let logs = TempRun::new();
    let far_cfg = format!(
        r#"
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "unix"
role = "connect"
address = "{sock}"
arbitration = "free-for-all"
reconnect_initial_ms = 150
reconnect_max_ms = 600
channels = ["c0"]

[[node]]
type = "log"
name = "cap"
directory = "{dir}"
filename = "cap.log"

[[edge]]
a = "uplink/c0"
b = "cap"
"#,
        sock = sock.display(),
        dir = logs.path().display(),
    );
    rpc_f.load_toml(&far_cfg, false).expect("far daemon load");

    assert!(
        wait_until(Duration::from_secs(15), || bound(rpc_n, "downlink", "c0")
            && bound(rpc_f, "uplink", "c0")),
        "the leg never connected and bound: near={:?} far={:?}",
        rpc_n.node("downlink"),
        rpc_f.node("uplink")
    );

    // The one send §15.24 names. Two frames on the wire (65 531 + 34 470 for a
    // two-character channel identity), reassembled by the peer's `FrameDecoder`.
    let line = payload();
    let ack = rpc_n
        .send("downlink/c0", &line, false, 10_000)
        .expect("send 100 001 bytes across the leg");
    assert_eq!(
        ack["sent"].as_u64(),
        Some(CHUNK),
        "the verb did not accept the whole line: {ack}"
    );

    // (a) The far side receives exactly 100 001 bytes, byte-exactly.
    let file = logs.join("cap.log");
    let want: Vec<u8> = line.bytes().chain(std::iter::once(b'\n')).collect();
    assert!(
        wait_until(Duration::from_secs(20), || {
            std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0) == CHUNK
        }),
        "the far side's log never reached {CHUNK} bytes (got {}): near={:?} far={:?}",
        std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
        rpc_n.node("downlink"),
        rpc_f.node("uplink")
    );
    let got = std::fs::read(&file).expect("read the far side's log");
    assert_eq!(
        sha256_hex(&got),
        sha256_hex(&want),
        "the reassembled stream is not the stream that was sent (len {} vs {CHUNK})",
        got.len()
    );

    // (b) The near side charged every byte to `accepted_targetward`, once — and skipped
    //     nothing on the way (invariant 3's first two clauses, on the leg's own path).
    assert_eq!(
        stat(rpc_n, "downlink", "c0", "accepted_targetward"),
        CHUNK,
        "near-side accounting: {:?}",
        rpc_n.node("downlink")
    );
    for field in [
        "discarded_unframable",
        "discarded_peer_gone",
        "discarded_targetward",
    ] {
        assert_eq!(
            stat(rpc_n, "downlink", "c0", field),
            0,
            "near side charged {field} on a clean fragmenting send: {:?}",
            rpc_n.node("downlink")
        );
    }
    // …and the far side accounted the same bytes on the way into its local graph.
    assert_eq!(
        stat(rpc_f, "uplink", "c0", "delivered_hostward"),
        CHUNK,
        "far-side accounting: {:?}",
        rpc_f.node("uplink")
    );
    assert_eq!(
        stat(rpc_f, "uplink", "c0", "discarded_hostward"),
        0,
        "the far side shed part of the reassembled chunk: {:?}",
        rpc_f.node("uplink")
    );
}

/// Invariant 3's third clause on the leg's sending half (LEG-2): a chunk the write half
/// has taken out of its bounded receiver but not yet put on the wire is **charged**, in
/// full, when the connection dies underneath it. Before the custody slot existed those
/// bytes simply vanished — accepted by `send`, never framed, never counted, which is the
/// silent-truncation shape §5 forbids.
///
/// The peer is `nexus-sim wire --stall`: a conforming handshake, an announced channel,
/// and then no reads at all, so the daemon's targetward direction fills the socket
/// buffer and the write half parks inside `write_all` mid-chunk (§9 head-of-line).
#[test]
fn the_untransmitted_tail_is_charged_when_the_peer_dies_mid_chunk() {
    let near = Daemon::start();
    let rpc = near.rpc();
    let sock = near.run().join("leg.sock");
    rpc.load_toml(&near_config(&sock), false)
        .expect("near daemon load");

    let peer = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &sock.to_string_lossy(),
            "--announce",
            "c0",
            "--stall",
            "--hold-ms",
            "60000",
            "--timeout-ms",
            "90000",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(15), || bound(rpc, "downlink", "c0")),
        "the stalling peer never bound the channel: {:?}",
        rpc.node("downlink")
    );

    // Queue chunks until the write half provably stops draining them. "Provably" is
    // the point: a plateau in `accepted_targetward` *while a backlog is outstanding*
    // can only mean the write half is suspended inside `write_all`, because the only
    // other place it parks is `next_send`, which returns immediately when any sender
    // has a chunk. So at that instant it is holding one — which is exactly the state
    // LEG-2 lost bytes from. Sending in rounds keeps this honest on a box whose socket
    // buffers are larger than this one's (~200 KiB), where the first rounds are simply
    // absorbed.
    let line = payload();
    let mut queued = 0u64;
    let mut blocked = false;
    for _round in 0..8 {
        for _ in 0..4 {
            // A refused `send` is *also* the signal (the 256-chunk receiver filled), so
            // a failure here is data, not an error.
            if rpc.send("downlink/c0", &line, false, 2_000).is_ok() {
                queued += CHUNK;
            }
        }
        let mut last = u64::MAX;
        let mut still = 0u32;
        if wait_until(Duration::from_secs(3), || {
            let now = stat(rpc, "downlink", "c0", "accepted_targetward");
            if now == last && now < queued {
                still += 1;
            } else {
                still = 0;
            }
            last = now;
            // Ten consecutive samples (~250 ms with the RPC round trips) with the
            // counter frozen and bytes still owed.
            still >= 10
        }) {
            blocked = true;
            break;
        }
    }
    assert!(
        blocked,
        "the write half never backed up against the stalled peer, so this test never \
         reached the state LEG-2 lost bytes in (queued {queued}): {:?}",
        rpc.node("downlink")
    );
    let accepted_before = stat(rpc, "downlink", "c0", "accepted_targetward");

    // Kill the peer out from under the suspended `write_all`.
    drop(peer);

    assert!(
        wait_until(Duration::from_secs(15), || {
            stat(rpc, "downlink", "c0", "discarded_peer_gone") > 0
        }),
        "the chunk the write half was holding when the peer died was never charged — \
         it was accepted by `send` and then vanished (LEG-2). node={:?}",
        rpc.node("downlink")
    );

    let node = rpc.node("downlink").expect("downlink after the peer died");
    let accepted = stat(rpc, "downlink", "c0", "accepted_targetward");
    let tail = stat(rpc, "downlink", "c0", "discarded_peer_gone");
    let unframable = stat(rpc, "downlink", "c0", "discarded_unframable");
    let purged = stat(rpc, "downlink", "c0", "purged_on_reconnect");

    // The tail is one chunk's worth at most: only one chunk is ever in custody, and
    // whatever of it reached the wire was discharged as it went.
    assert!(
        tail > 0 && tail <= CHUNK,
        "the untransmitted tail must be a slice of exactly one {CHUNK}-byte chunk, was \
         {tail}: {node}"
    );
    // Nothing was refused by the framer on a two-character channel identity.
    assert_eq!(unframable, 0, "the framer refused a fragment: {node}");
    // Conservation: every chunk that left the receiver is accounted for down to the
    // byte, so the accounted total is a whole number of chunks. A chunk still sitting
    // in the receiver contributes nothing (it is parked, not lost — §7.4's
    // faulted-and-wait), and one split across the wire and the tail contributes both
    // halves. Byte-for-byte conservation is the property invariant 3's third clause
    // buys; the modulo is how it is visible from outside the daemon.
    let accounted = accepted + tail + unframable + purged;
    assert_eq!(
        accounted % CHUNK,
        0,
        "the leg's accounting lost {} bytes of a chunk: accepted={accepted} \
         tail={tail} unframable={unframable} purged={purged} (queued={queued}, \
         accepted before the kill={accepted_before}): {node}",
        accounted % CHUNK
    );
    assert!(
        accounted >= CHUNK && accounted <= queued,
        "accounted {accounted} bytes against {queued} queued: {node}"
    );
}
