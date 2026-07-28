//! Review-32 regression guards for the tap hub's replay and gap accounting
//! (`nexus-daemon/src/tap.rs`; design §5 replay ring, §11.8 offsets, invariant 10).
//! Two protocol facts a client splicing by offset depends on, each broken in a
//! different direction before this file existed:
//!
//! 1. **TAP-1 — a replay too large to deliver loses its head, never its tail.** The
//!    snapshot is queued into the connection's bounded tap channel from inside the
//!    critical section the only drainer is blocked in, so what a tap can be handed is
//!    the channel's free space — 128 pieces of 64 KiB, i.e. 8 MiB — while
//!    `MAX_REPLAY_RING` is 16 MiB and the validator accepts every value up to it.
//!    Trimming as the budget was reached kept the **oldest** bytes and discarded the
//!    newest, and left `from_offset + replay_bytes` short of the live edge: the client
//!    spliced stale scrollback flush onto the live stream across an unmarked hole. The
//!    guard asserts the arithmetic (`from_offset + replay_bytes == ingested`) *and*
//!    the identity of the bytes (SHA-256 against the newest tail of the deterministic
//!    source stream), because the arithmetic alone would also hold for a tail trim
//!    that reported a smaller `ingested`.
//! 2. **TAP-2 — the `replay_ring = 0` window between two taps is a hole, and a hole is
//!    announced.** With no ring and no open tap the producer's feed is inactive, so
//!    the hostward bytes of that window reach no hub: `ingested` does not advance
//!    (correctly — the offset space is the *delivered*-bytes space, invariant 10, and
//!    folding them in would make `ingested − ring.len()` stop naming the ring's base
//!    offset) and they used to go uncounted as well, which made them a third category
//!    that was neither delivered nor signalled. The next `tap.open` reported the
//!    previous tap's frontier with the same `epoch` and `feed_dropped: 0`, and a
//!    browser spliced across an arbitrary gap in silence. They are now charged to the
//!    same shared `feed_dropped` atomic an overflow is, so the hole arrives as the
//!    first live chunk's `gap_before` with no new field and no new client work.
//!    TAP-4 rides along: the `tap.open` baseline is the hub's already-charged
//!    watermark, so `baseline + Σgap_before` is the true loss and not twice it.
//!    **There are two charge sites and this file exercises both.** A producer with a
//!    graph consumer builds the `Chunk` and the inactive feed refuses it
//!    (`TapFeed::mirror`); a producer with *no* consumer and an unwanted feed never
//!    builds one at all and must charge at the skip site (`TapFeed::skipped`). The
//!    first fix wrote only the mirror half, and the guard — which attached a `log`
//!    sink for exactly that reason — could not see it: the sink-less shape, which is
//!    how an operator watches a lone device with `ctl tap`, still spliced across the
//!    window in silence. One test per site now, sharing one body, because a client
//!    cannot tell the two shapes apart and neither may the accounting.
//!
//! Both need a software serial device (a `nexus-sim` pty double), so both **skip** off
//! Linux (§5): a pts cannot be a serial device on macOS.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

/// The ring under test: 10 MiB, deeper than one `tap.open` can deliver and well inside
/// the 16 MiB `MAX_REPLAY_RING` the validator accepts.
const RING: u64 = 10 * 1024 * 1024;
/// Bytes streamed, chosen so the ring is full *and* the live edge is past it — the only
/// shape in which a head trim and a tail trim disagree about `from_offset`.
const TOTAL: u64 = 12 * 1024 * 1024;
/// Paced well under the §15.19 bound so the producer→hub feed never overruns; the test
/// asserts `feed_dropped == 0` before reading anything into the offset space.
const RATE: u64 = 6 * 1024 * 1024;
const SEED: u64 = 91;
/// `TAP_QUEUE_CAP * REPLAY_PIECE` — the deliverable budget on a fresh connection whose
/// tap channel is empty. Duplicated from `nexus-daemon/src/tap.rs` deliberately: an
/// itest is a black-box client, and a change to either constant should show up here.
const BUDGET: u64 = 128 * 64 * 1024;

/// The `nexus-sim` deterministic byte stream (splitmix64), copied verbatim from
/// `nexus-sim`'s `seeded_bytes` as `p3_firehose` does — the source's own checksum is on
/// a stdout `Sim::spawn` discards, and the stream is deterministic in `(seed, size)`.
fn seeded_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut s = seed;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.extend_from_slice(&z.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Standard base64 decode — the inverse of the daemon's `tap.data` encoding.
fn base64_decode(s: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let (mut acc, mut nbits) = (0u32, 0u32);
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue };
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    out
}

/// One tap on its own raw control connection: the `tap.open` ack plus the `tap.data`
/// stream on the same socket, which is where the splice guarantee lives (it is per-tap,
/// so both halves must be observed on the one connection). Local to this file — the
/// shared harness is off limits to a review guard.
struct TapConn {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl TapConn {
    fn open(socket: &Path, endpoint: &str, replay: bool) -> (Value, Self) {
        let mut stream = UnixStream::connect(socket).expect("connect tap socket");
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tap.open",
            "params": { "endpoint": endpoint, "replay": replay },
        });
        stream
            .write_all(format!("{req}\n").as_bytes())
            .expect("write tap.open");
        stream.flush().expect("flush tap.open");
        let mut conn = TapConn {
            stream,
            buf: Vec::new(),
        };
        let line = conn
            .read_line(Duration::from_secs(10))
            .expect("tap.open ack");
        let ack: Value = serde_json::from_str(&line).expect("parse tap.open ack");
        (ack["result"].clone(), conn)
    }

    /// Drain `tap.data` as `(offset, gap_before, bytes)` until `want` bytes have arrived
    /// or the bounded deadline passes. Both fields are required: their absence is the
    /// regression, not a tolerable omission.
    fn frames(&mut self, want: u64, timeout: Duration) -> Vec<(u64, u64, Vec<u8>)> {
        let hard = Instant::now() + timeout;
        let mut out: Vec<(u64, u64, Vec<u8>)> = Vec::new();
        let mut got = 0u64;
        while got < want && Instant::now() < hard {
            let Some(line) = self.read_line(Duration::from_millis(500)) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("method").and_then(Value::as_str) != Some("tap.data") {
                continue;
            }
            let p = v.get("params").expect("tap.data params");
            let offset = p["offset"].as_u64().expect("tap.data carries an offset");
            let gap = p["gap_before"]
                .as_u64()
                .unwrap_or_else(|| panic!("tap.data carries no `gap_before`: {v}"));
            let bytes = base64_decode(p["data"].as_str().expect("tap.data data"));
            got += bytes.len() as u64;
            out.push((offset, gap, bytes));
        }
        out
    }

    fn read_line(&mut self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                return Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            self.stream.set_read_timeout(Some(deadline - now)).ok();
            let mut tmp = [0u8; 65536];
            match self.stream.read(&mut tmp) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(_) => return None,
            }
        }
    }
}

/// A node counter out of `state`, 0 if absent.
fn node_u64(rpc: &nexus_itest::Rpc, node: &str, field: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| n.get(field).and_then(Value::as_u64))
        .unwrap_or(0)
}

/// A field of a named endpoint in `state`'s top-level `endpoints` array (§5/DM-5),
/// looked up by name rather than by index so an added node cannot silently reorder it.
fn endpoint_u64(rpc: &nexus_itest::Rpc, endpoint: &str, field: &str) -> u64 {
    rpc.state()["endpoints"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|e| e["endpoint"].as_str() == Some(endpoint))
                .and_then(|e| e[field].as_u64())
        })
        .unwrap_or(0)
}

/// TAP-1 — a ring deeper than one `tap.open` can deliver replays its **newest** bytes,
/// and the replay still ends exactly on the live edge so the splice with the live
/// stream is gapless. Under the old tail trim the same run replayed the oldest 8 MiB of
/// a 10 MiB ring and stopped 2 MiB short of the live edge, silently.
///
/// The graph is a lone serial node: with no consumer, `discarded_unattached` counts
/// every byte the reader took off the device, which is the cheapest exact "the source
/// has finished" signal there is (the tap/ring mirror is a spy *outside* the graph, so
/// it does not disturb that counter — invariant 9).
#[test]
fn an_oversize_ring_replays_its_newest_bytes_and_lands_on_the_live_edge() {
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP an_oversize_ring_replays_its_newest_bytes_and_lands_on_the_live_edge: no serial dev"
        );
        return;
    };
    drop(probe); // we spawn our own gated, paced source below

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");

    let (total, rate, seed) = (TOTAL.to_string(), RATE.to_string(), SEED.to_string());
    let dev_s = dev.to_string_lossy().into_owned();
    let go_s = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &total,
            "--seed",
            &seed,
            "--rate",
            &rate,
            "--wait-file",
            &go_s,
            "--link",
            &dev_s,
            "--hold-ms",
            "60000",
            "--timeout-ms",
            "120000",
        ],
        Some(&dev),
    );

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
hostward_buffer = 16384
replay_ring = {ring}
"#,
        dev = dev.display(),
        ring = RING,
    );
    rpc.load_toml(&cfg, false)
        .expect("load oversize-ring config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Release the paced source and let it finish: with no graph consumer, every byte
    // read lands on `discarded_unattached`, so that counter reaching TOTAL is the exact
    // "the whole stream is in" signal.
    std::fs::File::create(&go).expect("touch GO");
    assert!(
        wait_until(Duration::from_secs(90), || {
            node_u64(rpc, "usb0", "discarded_unattached") >= TOTAL
        }),
        "the source never delivered {TOTAL} bytes (read {})",
        node_u64(rpc, "usb0", "discarded_unattached")
    );
    assert_eq!(
        node_u64(rpc, "usb0", "discarded_unattached"),
        TOTAL,
        "the reader took more than the source produced"
    );
    // The lossless precondition, asserted before anything is read *out* of the offset
    // space, so a genuine feed overrun is diagnosed as one rather than as a bad splice.
    assert_eq!(
        endpoint_u64(rpc, "usb0", "feed_dropped"),
        0,
        "the paced run lost bytes at the producer→hub feed hop; the offset space is \
         then shorter than the source and this test cannot distinguish the two"
    );

    // A fresh connection: its 128-slot tap channel is empty, so the deliverable budget
    // is exactly BUDGET and the trim is entirely the ring's excess depth.
    let (ack, mut tap) = TapConn::open(&d.socket(), "usb0", true);
    let from_offset = ack["from_offset"].as_u64().expect("from_offset in ack");
    let replay_bytes = ack["replay_bytes"].as_u64().expect("replay_bytes in ack");
    assert_eq!(
        replay_bytes, BUDGET,
        "a {RING}-byte ring should replay the connection's whole free budget: {ack}"
    );
    assert_eq!(
        from_offset + replay_bytes,
        TOTAL,
        "a trimmed replay must still end on the live edge — from_offset {from_offset} \
         + replay_bytes {replay_bytes} != ingested {TOTAL} (TAP-1): {ack}"
    );

    // The identity of the bytes, which is the half the arithmetic cannot show: they are
    // the newest tail of the source stream, not the oldest retained window.
    let frames = tap.frames(replay_bytes, Duration::from_secs(60));
    let received: Vec<u8> = frames.iter().flat_map(|(_, _, b)| b.clone()).collect();
    assert_eq!(
        received.len() as u64,
        replay_bytes,
        "the ack promised {replay_bytes} replay bytes, {} arrived",
        received.len()
    );
    let source = seeded_bytes(SEED, TOTAL as usize);
    assert_eq!(
        sha256_hex(&received),
        sha256_hex(&source[(TOTAL - replay_bytes) as usize..]),
        "the replay is not the newest {replay_bytes} bytes of the stream (TAP-1)"
    );

    // The pieces themselves are contiguous and hole-free: the ring is contiguous in
    // offset space by construction, so a `gap_before` here would be a fabricated hole.
    let mut expect_at = from_offset;
    for (i, (offset, gap, bytes)) in frames.iter().enumerate() {
        assert_eq!(*offset, expect_at, "replay piece {i} is not contiguous");
        assert_eq!(*gap, 0, "replay piece {i} reported a gap of {gap} bytes");
        expect_at += bytes.len() as u64;
    }
}

/// TAP-2 (+ TAP-4) — on a `replay_ring = 0` endpoint the bytes produced while no tap is
/// open leave a hole, and the hole is announced. The offset space must **not** absorb
/// them (invariant 10: `from_offset` resumes on the previous tap's frontier), and the
/// count must reach the next tap as its first chunk's `gap_before`, on top of a
/// `tap.open` baseline that does not already contain it.
///
/// **With a graph consumer attached.** The producer builds the `Chunk` (the `log` needs
/// it) and the inactive feed is the only thing declining it, so the charge happens in
/// `TapFeed::mirror`. The sink-less shape below reaches the *other* charge site, and the
/// two are different code paths: this one alone was the half the original guard covered.
#[test]
fn the_ringless_window_between_two_taps_is_announced_as_a_gap() {
    ringless_window_gap(
        "the_ringless_window_between_two_taps_is_announced_as_a_gap",
        true,
    );
}

/// TAP-2's other half, and the one that stayed broken after the first fix: the same
/// endpoint with **no graph consumer at all**.
///
/// A serial reader whose hostward fan-out is empty and whose feed is unwanted skips
/// building the `Chunk` entirely (`hostward.is_empty() && !tap_wanted`), so
/// `TapFeed::mirror` is never called and the window can only be charged at the skip site
/// — `TapFeed::skipped`. Until that call existed, a lone serial node observed only
/// through `tap.open` (which is exactly how an operator watches a device with `ctl tap`,
/// and the shape TAP-1 above loads) reported the dark window as `discarded_unattached`
/// and nowhere else: the next `tap.open` returned the previous frontier, the same
/// `epoch`, `feed_dropped: 0`, and a first chunk with `gap_before: 0`. The client
/// spliced across it in silence.
///
/// Fail-first: proved against a tree with `TapFeed::skipped` in place but its serial
/// call site removed — `feed_dropped` stayed 0 and the first chunk's `gap_before` was 0
/// while `discarded_unattached` grew by the window.
#[test]
fn the_ringless_window_is_announced_with_no_graph_consumer_at_all() {
    ringless_window_gap(
        "the_ringless_window_is_announced_with_no_graph_consumer_at_all",
        false,
    );
}

/// The body both TAP-2 guards share. `with_sink` selects which of the two charge sites
/// the producer takes: a `log` consumer forces the chunk to be built (`TapFeed::mirror`'s
/// inactive branch), no consumer at all takes the read-and-discard arm
/// (`TapFeed::skipped`). Everything a client observes must be identical either way,
/// because to a client splicing by offset it is the same hole.
///
/// An echo device makes the byte counts exact and known: each `send` of a 5-character
/// line comes back as 6 bytes.
fn ringless_window_gap(test_name: &str, with_sink: bool) {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP {test_name}: no serial dev");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir logs");
    let sink = if with_sink {
        format!(
            r#"
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
"#,
            logdir = logdir.display(),
        )
    } else {
        String::new()
    };
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
replay_ring = 0
{sink}"#,
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load ring-less config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Session 1: six delivered bytes, so the endpoint's frontier is a known 6.
    let (ack1, mut tap1) = TapConn::open(&d.socket(), "usb0", false);
    assert_eq!(ack1["from_offset"], json!(0), "ack: {ack1}");
    assert_eq!(ack1["feed_dropped"], json!(0), "ack: {ack1}");
    rpc.send("usb0", "line1", false, 5000).expect("send line1");
    let seen: usize = tap1
        .frames(6, Duration::from_secs(15))
        .iter()
        .map(|(_, _, b)| b.len())
        .sum();
    assert_eq!(seen, 6, "the first tap never saw its echoed line");

    // Session 1 ends. With no ring and no tap the feed goes inactive — the window this
    // test is about. Wait for the daemon to observe the detach before producing into it.
    drop(tap1);
    assert!(
        wait_until(Duration::from_secs(5), || {
            endpoint_u64(rpc, "usb0", "taps") == 0
        }),
        "the tap is still attached: {}",
        rpc.state()["endpoints"]
    );

    // Three lines echoed into nobody: 18 bytes that reach no hub. They are counted at
    // the feed hop, which is the only place that can see them.
    for i in 0..3 {
        rpc.send("usb0", &format!("gap{i}!"), false, 5000)
            .unwrap_or_else(|e| panic!("send gap{i}: {e:?}"));
    }
    let site = if with_sink {
        "TapFeed::mirror's inactive branch"
    } else {
        "TapFeed::skipped, at the serial reader's read-and-discard arm"
    };
    assert!(
        wait_until(Duration::from_secs(15), || {
            endpoint_u64(rpc, "usb0", "feed_dropped") == 18
        }),
        "the un-mirrored window went uncounted (TAP-2); the charge site for this shape \
         is {site}: feed_dropped = {}, discarded_unattached = {}",
        endpoint_u64(rpc, "usb0", "feed_dropped"),
        node_u64(rpc, "usb0", "discarded_unattached")
    );

    // Session 2: the frontier is unmoved — the missing bytes were NOT folded into the
    // offset space, which would break `from_offset = ingested − ring.len()` — and the
    // baseline excludes the delta the next chunk is about to carry (TAP-4).
    let (ack2, mut tap2) = TapConn::open(&d.socket(), "usb0", false);
    assert_eq!(
        ack2["from_offset"],
        json!(6),
        "the offset space absorbed un-delivered bytes (invariant 10): {ack2}"
    );
    assert_eq!(
        ack2["epoch"], ack1["epoch"],
        "the hub was rebuilt between sessions; this test's premise is gone"
    );
    assert_eq!(
        ack2["feed_dropped"],
        json!(0),
        "the tap.open baseline already contains the gap the first chunk will carry \
         (TAP-4): {ack2}"
    );

    // …and the hole arrives as the first live chunk's `gap_before`: baseline 0 + 18 is
    // exactly what the endpoint lost, and the client can mark the seam.
    rpc.send("usb0", "line2", false, 5000).expect("send line2");
    let frames = tap2.frames(6, Duration::from_secs(15));
    assert_eq!(
        frames.first().map(|(o, g, b)| (*o, *g, b.len())),
        Some((6, 18, 6)),
        "the ring-less window was spliced across in silence (TAP-2): {frames:?}"
    );
}
