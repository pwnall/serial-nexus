//! Review-32 cluster 4: **§5's "loss is always visible and attributable" had holes
//! in the leg** (design §5, §7.4, §9; AGENTS.md invariant 3 clause 3 and invariant 9).
//!
//! One file per defect area, `p12_*` being this review's family as `p9_*` is
//! review 26's. Each test's doc comment names the finding it pins.
//!
//! * `LEG-1` — a `faces = target` leg threw away its channels' `DropCounters`, so
//!   everything it shed at its own intake was invisible in `state`. It was the one
//!   consuming node kind that never reported its target-facing endpoint's
//!   `dropped_full`.
//! * `LEGD-2` — `delivered_hostward` credited the whole chunk whenever `fan_out`
//!   reported `live`, and `fan_out` reports `live` for a *full* sink too, so a chunk
//!   no consumer received was counted as delivered *and* discarded at once.
//! * `LEG-2` (= `WIRE-2`) — the socket write half pops a chunk off its bounded
//!   receiver before framing it, and lost the untransmitted tail uncounted when the
//!   peer went away by **either** of two exits: `write_all` failing, or the read half
//!   returning `PeerGone` first and `select!` dropping the write future while it was
//!   suspended inside `write_all`. Both are exercised below.
//!
//! Everything here is a **leg** test: no serial device, so it runs on every platform.
//! The wire peers are hand-rolled on a raw `UnixStream` rather than driven through
//! `nexus-sim`, because these tests need to own the exact moment the peer stops
//! reading and the exact way it goes away — a stall the sim cannot express and a
//! half-close it does not offer. Ground truth is always a structured RPC counter
//! identity, never CLI text (§5).

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, bin, wait_until};
use serde_json::Value;

// ---- wire plumbing the harness deliberately does not carry ---------------------

/// A §9 `hello` frame in `codec-api`'s wire layout:
/// `u32 body_len | u32 magic | u16 version | u32 capabilities | u16 count |
/// count × (u16 len | UTF-8 identity)`, all big-endian.
///
/// Hand-rolled: `nexus-itest` does not depend on `codec-api`, and a test that
/// re-derives the bytes an operator's peer would put on the wire is a slightly
/// better witness than one that calls the daemon's own encoder.
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

/// A raw v1 wire peer that completes the §9 handshake and then behaves exactly as
/// the test tells it to — either never reading (so the daemon's socket send buffer
/// fills and its write half parks inside `write_all` with a chunk already popped) or
/// draining everything on a background thread.
struct WirePeer {
    stream: UnixStream,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl WirePeer {
    fn dial(address: &Path, channels: &[&str]) -> WirePeer {
        // Retry until the leg is *connectable*, not merely until its socket file
        // exists. The two are one syscall apart and the callers' readiness wait
        // (`sock.exists()`) can only see the first: `bind(2)` creates the node,
        // `listen(2)` is what stops `connect(2)` answering ECONNREFUSED, so a dial
        // landing between them fails on a daemon that is coming up perfectly
        // normally. Observed exactly there on macOS — `Connection refused (os error
        // 61)` at this line, in the full suite and never in isolation, which reads
        // as flakiness and gets chased as one (AGENTS §8).
        //
        // Bounded and condition-driven rather than slept on (§5): the loop ends on
        // the first success, and the deadline matches the callers' own 10 s
        // socket-appearance wait so a leg that never listens still fails loudly
        // instead of hanging the suite.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match UnixStream::connect(address) {
                Ok(s) => break s,
                Err(e) if std::time::Instant::now() < deadline => {
                    if e.kind() != std::io::ErrorKind::ConnectionRefused
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        panic!("dial the leg at {}: {e}", address.display());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!(
                    "dial the leg at {}: {e} (still not accepting after 10 s)",
                    address.display()
                ),
            }
        };
        stream
            .write_all(&hello_frame(channels))
            .expect("write hello");
        stream.flush().expect("flush hello");
        WirePeer {
            stream,
            stop: Arc::new(AtomicBool::new(false)),
            reader: None,
        }
    }

    /// Announce and then **never read**: the daemon's socket send buffer fills and
    /// its write half parks inside `write_all` holding a chunk it has already taken
    /// off its bounded receiver — LEG-2's precondition.
    fn stalled(address: &Path, channels: &[&str]) -> WirePeer {
        WirePeer::dial(address, channels)
    }

    /// Announce and drain: a background thread reads and discards everything, so the
    /// leg's buffered hostward backlog flushes all the way onto the wire.
    fn draining(address: &Path, channels: &[&str]) -> WirePeer {
        let mut peer = WirePeer::dial(address, channels);
        let mut reader = peer.stream.try_clone().expect("clone the peer socket");
        reader
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("set read timeout");
        let stop = peer.stop.clone();
        peer.reader = Some(std::thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            while !stop.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
            }
        }));
        peer
    }

    /// Half-close the peer's writing direction, leaving its receive window shut. The
    /// daemon's read half then sees a deterministic `Ok(0)` and returns `PeerGone`
    /// while its write half is *still* parked inside `write_all` — LEG-2's second
    /// exit path, the one a residual charged on the `write_all` error arm alone would
    /// miss entirely.
    fn half_close(&self) {
        self.stream
            .shutdown(Shutdown::Write)
            .expect("half-close the peer");
    }

    /// Take the peer away outright, so the daemon's parked `write_all` wakes with a
    /// broken pipe — LEG-2's first exit path.
    fn vanish(&self) {
        self.stream
            .shutdown(Shutdown::Both)
            .expect("close the peer");
    }
}

impl Drop for WirePeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}

/// Run `nexus-sim` with `args` to completion and return its JSON verdict. The
/// harness's `Sim::client` hardcodes the `client` subcommand; the streaming half of
/// the first test needs `wire --stall`, whose verdict reports the byte count that is
/// this file's ground truth.
fn sim_verdict(args: &[&str]) -> Value {
    let out = Command::new(bin("nexus-sim"))
        .args(args)
        .output()
        .expect("run nexus-sim");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse nexus-sim verdict: {e}; stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

// ---- state helpers -------------------------------------------------------------

/// One `u64` counter out of a leg channel's `state_extra` object (§7.4). A missing
/// field reads as 0 rather than panicking, so a test that loses a counter fails on
/// the arithmetic it is about rather than on an unwrap.
fn counter(rpc: &Rpc, node: &str, ch: &str, field: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/{field}"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

fn connection(rpc: &Rpc, node: &str) -> Option<String> {
    rpc.node(node)?
        .get("connection")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn bound(rpc: &Rpc, node: &str, ch: &str) -> bool {
    rpc.node(node)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/binding"))
                .and_then(Value::as_str)
                .map(|s| s == "bound")
        })
        .unwrap_or(false)
}

/// Wait until a counter has stopped growing across several consecutive polls
/// *while bytes are still owed*, and return its settled value. That plateau is the
/// observable proof the write half is parked inside `write_all` with a chunk in
/// hand — the state LEG-2 loses bytes from — because the only other place it parks
/// is `next_send`, which returns the instant any bound sender has a chunk. So the
/// wait is the condition itself, not a bare sleep (§5).
///
/// The predicate is "frozen **and** owing", never "frozen at a nonzero value". The
/// counter is credited per *completed* frame (`leg.rs` — only after `write_all`
/// returns), so on a kernel whose AF_UNIX buffer is narrower than one frame it
/// settles legitimately at 0: macOS's is 8192 bytes (`net.local.stream.sendspace`,
/// measured — exactly 8192 absorbed with no reader) against a 60 006-byte frame,
/// where a `now > 0` clause times out on a daemon whose ledger still closes to the
/// byte. `p6_fragmentation` guards the same finding with the same predicate for the
/// same reason, and "owing" is *stricter* than "nonzero" on Linux too: the old form
/// also accepted a plateau at `now == owed`, a fully drained peer, which is not the
/// park state at all.
fn wait_settled(rpc: &Rpc, node: &str, ch: &str, field: &str, owed: u64) -> u64 {
    let mut last = u64::MAX;
    let mut stable = 0u32;
    let settled = wait_until(Duration::from_secs(30), || {
        let now = counter(rpc, node, ch, field);
        if now == last && now < owed {
            stable += 1;
        } else {
            stable = 0;
            last = now;
        }
        stable >= 3
    });
    assert!(
        settled,
        "{node}/{ch}.{field} never froze below the {owed} bytes owed (last {last}): {:?}",
        rpc.node(node)
    );
    last
}

/// A `faces = host` leg (the peer dials it and streams hostward) wired straight into
/// a `faces = target` leg with no peer of its own, so the second leg's intake is a
/// consuming boundary that fills and then sheds. No serial device is involved.
fn two_leg_config(downlink: &Path, uplink: &Path) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{downlink}"
channels = ["c0"]
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "listen"
address = "{uplink}"
channels = ["c0"]
[[edge]]
a = "downlink/c0"
b = "uplink/c0"
"#,
        downlink = downlink.display(),
        uplink = uplink.display(),
    )
}

// ---- LEG-1 + LEGD-2 ------------------------------------------------------------

/// LEG-1 and LEGD-2 together, because they are the two halves of one arithmetic:
/// what a leg *sheds* at its intake must be reported, and what it claims to have
/// *delivered* must be no more than a consumer actually took.
///
/// A remote peer streams hostward into `downlink`; its only consumer is `uplink`, a
/// peerless `faces = target` leg whose per-channel relay and edge buffer fill and
/// then shed. Before the fix `uplink` reported nothing at all (its `DropCounters`
/// were dropped on the floor in `start`) while `downlink` reported the same bytes as
/// both `delivered_hostward` and `discarded_hostward`. Afterwards every streamed byte
/// is either on the far wire or named as shed, exactly once.
#[test]
fn every_hostward_byte_a_leg_sheds_is_reported_exactly_once() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let dn_sock = d.run().join("dn.sock");
    let up_sock = d.run().join("up.sock");
    rpc.load_toml(&two_leg_config(&dn_sock, &up_sock), false)
        .expect("load the two-leg graph");
    assert!(
        wait_until(Duration::from_secs(10), || dn_sock.exists()
            && up_sock.exists()),
        "the legs never bound their listen sockets"
    );

    // The producer: a conforming peer that pushes `FRAMES` hostward frames on `c0`.
    // The consuming side can hold at most one chunk per slot — 256 in the edge
    // channel, 256 in the leg's per-channel relay, one in the relay task's hand — so
    // a frame count several times that sheds by construction rather than by rate,
    // which keeps the test load-independent on a shared box.
    const FRAMES: usize = 2048;
    const FRAME_LEN: usize = 1024;
    let mut args: Vec<String> = vec![
        "wire".into(),
        "--transport".into(),
        "unix".into(),
        "--address".into(),
        dn_sock.to_string_lossy().into_owned(),
        "--announce".into(),
        "c0".into(),
    ];
    for _ in 0..FRAMES {
        args.push("--send".into());
        args.push(format!("c0={FRAME_LEN}"));
    }
    args.extend([
        "--hold-ms".into(),
        "1500".into(),
        "--timeout-ms".into(),
        "30000".into(),
    ]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let verdict = sim_verdict(&argv);
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "the streaming peer did not complete the handshake: {verdict}"
    );
    let streamed = verdict["sent"]
        .as_u64()
        .unwrap_or_else(|| panic!("no sent count in {verdict}"));
    assert_eq!(streamed, (FRAMES * FRAME_LEN) as u64, "{verdict}");

    // LEGD-2: with a single sink the receiving leg's two hostward counters partition
    // the stream exactly. Before the fix `delivered_hostward` was credited on
    // `fan_out`'s `live`, which is true for a *full* sink too, so this sum ran to
    // roughly twice `streamed` and never converged.
    let partitioned = wait_until(Duration::from_secs(20), || {
        counter(rpc, "downlink", "c0", "delivered_hostward")
            + counter(rpc, "downlink", "c0", "discarded_hostward")
            == streamed
    });
    assert!(
        partitioned,
        "downlink/c0 did not account for the stream exactly once \
         (delivered {} + discarded {} vs streamed {streamed}): {:?}",
        counter(rpc, "downlink", "c0", "delivered_hostward"),
        counter(rpc, "downlink", "c0", "discarded_hostward"),
        rpc.node("downlink")
    );
    let dn_discarded = counter(rpc, "downlink", "c0", "discarded_hostward");
    assert!(
        dn_discarded > 0,
        "the consuming boundary never filled, so nothing was shed: {:?}",
        rpc.node("downlink")
    );

    // LEG-1: the receiving leg reports what its own intake shed — the counter that
    // did not exist before, and the handle `start` used to discard.
    let up_shed = counter(rpc, "uplink", "c0", "dropped_slow_consumer");
    assert_eq!(
        up_shed,
        dn_discarded,
        "the same loss must read the same from both ends of the edge: {:?}",
        rpc.node("uplink")
    );
    assert!(up_shed > 0, "uplink shed nothing it could report");

    // …and the tail it did buffer is not lost either: give it a peer and everything
    // it took in leaves on the wire. Every streamed byte is now on one side or the
    // other of §5's ledger, with nothing counted twice.
    let _drain = WirePeer::draining(&up_sock, &["c0"]);
    assert!(
        wait_until(Duration::from_secs(20), || connection(rpc, "uplink")
            .as_deref()
            == Some("connected")
            && bound(rpc, "uplink", "c0")),
        "the draining peer never bound: {:?}",
        rpc.node("uplink")
    );
    let closed = wait_until(Duration::from_secs(30), || {
        counter(rpc, "uplink", "c0", "delivered_hostward")
            + counter(rpc, "uplink", "c0", "dropped_slow_consumer")
            == streamed
    });
    assert!(
        closed,
        "uplink did not account for every byte that reached it \
         (delivered {} + shed {} vs streamed {streamed}): {:?}",
        counter(rpc, "uplink", "c0", "delivered_hostward"),
        counter(rpc, "uplink", "c0", "dropped_slow_consumer"),
        rpc.node("uplink")
    );
    // The shedding is a full-buffer drop, not a framing failure or a purge.
    assert_eq!(counter(rpc, "uplink", "c0", "discarded_unframable"), 0);
    assert_eq!(counter(rpc, "uplink", "c0", "purged_on_reconnect"), 0);
}

// ---- LEG-2 (= WIRE-2) ----------------------------------------------------------

/// A `faces = host` leg whose send source is the arbitrated targetward stream of
/// local writers — the shape an operator's `send` lands in.
fn host_leg_config(address: &Path) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{address}"
channels = ["c0"]
"#,
        address = address.display(),
    )
}

/// The shared body of LEG-2's two guards: fill the leg's send queue against a peer
/// that has stopped reading, take the peer away by `kill`, and assert that every
/// byte the daemon accepted is accounted for exactly once.
///
/// `accepted_targetward` is what reached the wire, `discarded_peer_gone` is the tail
/// of the chunk the write half had already popped when the peer went, and
/// `purged_on_reconnect` is the outage-era backlog §7.4 discards at the next connect.
/// Before the fix the popped chunk was simply gone: no counter moved, and the pieces
/// written before the failure had *already* been credited, so one disconnect
/// overstated delivery and understated loss at the same time.
fn leg2_body(kill: impl FnOnce(&WirePeer)) {
    // 20 × 60 KB is far past any socket send buffer, so the write half is certain to
    // park mid-chunk, and comfortably inside the endpoint's 256-chunk targetward
    // queue, so every `send` is accepted without blocking the control plane. What
    // must *not* be inferred from that is a nonzero `accepted_targetward` while it
    // parks: one chunk is one 60 006-byte frame, which is wider than the whole of
    // macOS's 8 KiB AF_UNIX buffer, so the counter correctly stays 0 there — see
    // `wait_settled`, whose predicate is "frozen while still owing" for this reason.
    const LINES: usize = 20;
    const LINE_LEN: usize = 60_000;

    let d = Daemon::start();
    let rpc = d.rpc();
    let sock = d.run().join("dn.sock");
    rpc.load_toml(&host_leg_config(&sock), false)
        .expect("load the host-facing leg");
    assert!(
        wait_until(Duration::from_secs(10), || sock.exists()),
        "the leg never bound its listen socket"
    );

    let peer = WirePeer::stalled(&sock, &["c0"]);
    assert!(
        wait_until(Duration::from_secs(10), || connection(rpc, "downlink")
            .as_deref()
            == Some("connected")
            && bound(rpc, "downlink", "c0")),
        "the stalled peer never bound: {:?}",
        rpc.node("downlink")
    );

    let line = "x".repeat(LINE_LEN);
    let mut sent = 0u64;
    for _ in 0..LINES {
        let reply = rpc
            .send("downlink/c0", &line, false, 10_000)
            .expect("send targetward");
        assert_eq!(reply["delivered"].as_bool(), Some(true), "{reply}");
        sent += reply["sent"].as_u64().expect("sent byte count");
    }

    // The write half is now parked inside `write_all`, holding a chunk that is off
    // its receiver and not on the wire — the exact byte LEG-2 lost.
    let accepted_before = wait_settled(rpc, "downlink", "c0", "accepted_targetward", sent);
    assert!(accepted_before < sent, "the peer's buffer never filled");

    kill(&peer);
    assert!(
        wait_until(Duration::from_secs(15), || connection(rpc, "downlink")
            .as_deref()
            != Some("connected")),
        "the leg never registered the peer's departure: {:?}",
        rpc.node("downlink")
    );
    // Only now release the socket: dropping it inside `kill` would close both
    // directions at once and collapse the two exit paths into one.
    drop(peer);

    let stranded = counter(rpc, "downlink", "c0", "discarded_peer_gone");
    assert!(
        stranded > 0,
        "the in-flight chunk vanished uncounted (LEG-2): {:?}",
        rpc.node("downlink")
    );

    // The queued remainder is outage-era stale, so it is discarded — with a counter —
    // at the next connect (§7.4). That closes the ledger.
    let _next = WirePeer::draining(&sock, &["c0"]);
    let closed = wait_until(Duration::from_secs(20), || {
        counter(rpc, "downlink", "c0", "accepted_targetward")
            + counter(rpc, "downlink", "c0", "purged_on_reconnect")
            + counter(rpc, "downlink", "c0", "discarded_peer_gone")
            == sent
    });
    assert!(
        closed,
        "the leg did not account for every accepted byte \
         (accepted {} + purged {} + peer-gone {} vs sent {sent}): {:?}",
        counter(rpc, "downlink", "c0", "accepted_targetward"),
        counter(rpc, "downlink", "c0", "purged_on_reconnect"),
        counter(rpc, "downlink", "c0", "discarded_peer_gone"),
        rpc.node("downlink")
    );
    // The loss has its own name: folding it into `discarded_unframable` would have
    // destroyed the one thing that counter means ("this channel id is pathological").
    assert_eq!(counter(rpc, "downlink", "c0", "discarded_unframable"), 0);
}

/// LEG-2, exit path **one**: the peer's socket closes outright, so the write half's
/// own `write_all` fails and it returns `PeerGone` holding the popped chunk.
#[test]
fn a_peer_that_vanishes_mid_write_charges_the_stranded_chunk() {
    leg2_body(WirePeer::vanish);
}

/// LEG-2, exit path **two** — the one the finder missed and the verifier proved.
/// The peer half-closes (`shutdown(SHUT_WR)`) with its receive window still full, so
/// the daemon's *read* half returns `PeerGone` first and `select!` drops the write
/// future while it is suspended inside `write_all`, taking the popped chunk's stack
/// frame with it. A residual charged only on the `write_all` error arm is blind here,
/// which is why the chunk is held in the pump's own frame instead.
#[test]
fn a_half_closed_peer_charges_the_stranded_chunk_too() {
    leg2_body(WirePeer::half_close);
}
