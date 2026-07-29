//! Phase 8 tap stream offsets + the per-boot instance nonce (§11.8 / design §10 /
//! §15.32). Two protocol facts the browser history of §17 rests on:
//!
//! 1. **Offset-exact resumption.** Every `tap.data` carries the endpoint's monotonic
//!    hostward byte `offset`, and `tap.open` reports the tap's `from_offset`. A client
//!    that disconnects mid-stream and reopens `--replay` can trim the overlapping ring
//!    bytes by offset and reconstruct the stream **exactly once** — no gap at the seam,
//!    no ring-depth duplication on reload. Ground truth is a byte-exact `log` of the
//!    same endpoint; the offset-trimmed reconstruction must equal a contiguous prefix
//!    of it (SHA-256), and its length must equal `frontier − start` (no duplication).
//!    Needs a paced software serial source (Linux only), so it skips elsewhere.
//! 2. **Reset detection.** `info` exposes a per-boot `instance` nonce that changes
//!    across a daemon restart (offsets reset to 0 then), so a client keyed on it starts
//!    a fresh history rather than splicing across the reset. Pure control-plane fact —
//!    runs on every platform.
//! 3. **A hole is visible, never silently spliced (TAP-1b).** The offset space counts
//!    *delivered* bytes, so bytes lost at the lossy producer→hub feed hop would leave the
//!    offsets contiguous across a real gap — a browser splicing by offset would
//!    concatenate a holed stream and never know. Of the two sanctioned fixes the daemon
//!    took the second: the offset space stays the delivered-bytes space (which is what
//!    keeps invariant 10 and the exact replay splice true) and the loss is signalled
//!    *beside* it — `tap.open` reports the endpoint's `feed_dropped` baseline and every
//!    `tap.data` carries `gap_before`, the bytes lost immediately before that chunk. This
//!    file guards the wire contract and the accounting law (a lossless run has
//!    `gap_before == 0` on every chunk and `feed_dropped == 0` on the endpoint, with
//!    strictly contiguous offsets); the synthetic-gap semantics themselves are pinned by
//!    `daemon/src/tap.rs`'s unit tests, which can inject a feed drop deterministically.
//! 4. **Invariant 10, on the wire.** `from_offset = ingested − ring.len()` never
//!    underflows and never lies: with a full ring `from_offset + replay_bytes` lands
//!    exactly on the live edge, so replay and live meet with no gap and no overlap.
//!    Also, the offset space resets to 0 when the *graph* is replaced while
//!    `info.instance` does **not** change — the daemon-side half of the OPFS console
//!    freeze, and the reason the nonce is not what a client can key on. What it keys on
//!    instead is the per-hub **`epoch`** `tap.open` reports (§15.38): a rebuilt hub gets
//!    a fresh one, the browser's `offsetSpaceChanged`/`reanchor` move its stored history
//!    onto the new space, and `tap.closed` (TAP-1) tells it the *stream* ended rather
//!    than triggering the re-anchor.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Sim, serial_echo, sha256_hex, wait_until};

const R: u64 = 65536; // ring depth: 64 KiB
const T: u64 = 524288; // total streamed: 512 KiB
const RATE: u64 = 262144; // 256 KiB/s → ~2s, giving a wide mid-stream reconnect window
const SEED: u64 = 41;

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Standard base64 decode — inverse of the daemon's `tap.data` encoding.
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

/// An offset-tracking tap connection over the raw control socket. It captures the
/// `tap.open` ack's `from_offset` and folds every `tap.data` into a shared contiguous
/// **frontier** — appending only bytes past `have_up_to`, so bytes re-sent by a second
/// replay (offset < `have_up_to`) are trimmed. This is exactly the browser-history
/// splice of §17, exercised here over the raw protocol.
struct OffsetTap {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl OffsetTap {
    /// Open a tap and return the whole `tap.open` ack beside the connection, so a test
    /// can assert the ack's full shape (`from_offset`, `replay_bytes`, `feed_dropped`).
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
        let mut conn = OffsetTap {
            stream,
            buf: Vec::new(),
        };
        let ack = conn.read_line(Duration::from_secs(10)).expect("ack line");
        let v: Value = serde_json::from_str(&ack).expect("parse ack");
        (v, conn)
    }

    fn open_replay(socket: &Path, endpoint: &str) -> (u64, Self) {
        let (ack, conn) = OffsetTap::open(socket, endpoint, true);
        let from_offset = ack["result"]["from_offset"]
            .as_u64()
            .unwrap_or_else(|| panic!("from_offset missing in ack: {ack}"));
        (from_offset, conn)
    }

    /// Drain `tap.data` frames as `(offset, gap_before, bytes)` until `want` bytes have
    /// arrived or the bounded deadline passes. Every frame's `offset` and `gap_before`
    /// are required to be present: their absence is the TAP-1b regression, not a
    /// tolerable omission.
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
                .unwrap_or_else(|| panic!("tap.data carries no `gap_before` (TAP-1b): {v}"));
            let bytes = base64_decode(p["data"].as_str().expect("tap.data data"));
            got += bytes.len() as u64;
            out.push((offset, gap, bytes));
        }
        out
    }

    /// Wait for this tap's terminal `tap.closed`, returning its `reason`.
    fn wait_closed(&mut self, timeout: Duration) -> Option<String> {
        let hard = Instant::now() + timeout;
        while Instant::now() < hard {
            let Some(line) = self.read_line(Duration::from_millis(500)) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if v.get("method").and_then(Value::as_str) == Some("tap.closed") {
                return v["params"]["reason"].as_str().map(str::to_owned);
            }
        }
        None
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

    /// Drain `tap.data` frames, folding each into `out` via the offset-trim splice, until
    /// `stop(frontier)` holds (checked with the frontier *before* each read) or a bounded
    /// timeout elapses. `have_up_to` is the next unseen offset; a chunk wholly at or
    /// before it is trimmed (already stored), one straddling it contributes only its
    /// fresh tail. A chunk starting *past* the frontier is a gap — the reconnect is
    /// immediate so it never happens, and asserting it turns a silent gap into a failure.
    fn fold(&mut self, have_up_to: &mut u64, out: &mut Vec<u8>, mut stop: impl FnMut(u64) -> bool) {
        let hard = Instant::now() + Duration::from_secs(40);
        while Instant::now() < hard {
            if stop(*have_up_to) {
                break;
            }
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
            let offset = p["offset"].as_u64().expect("tap.data offset");
            let bytes = base64_decode(p["data"].as_str().expect("tap.data data"));
            let end = offset + bytes.len() as u64;
            if end <= *have_up_to {
                continue; // wholly seen — trimmed
            }
            assert!(
                offset <= *have_up_to,
                "gap at splice: chunk offset {offset} > frontier {have_up_to}"
            );
            let skip = (*have_up_to - offset) as usize;
            out.extend_from_slice(&bytes[skip..]);
            *have_up_to = end;
        }
    }
}

/// §11.8 property 1: a mid-stream reconnect reconstructs the stream exactly once by
/// offset-trimming the second replay. Linux-only (paced pty-as-serial source).
#[test]
fn reconnecting_tap_reconstructs_stream_exactly_once_by_offset() {
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP reconnecting_tap_reconstructs_stream_exactly_once_by_offset: no serial dev"
        );
        return;
    };
    drop(probe);

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir logs");
    let log = logdir.join("serial.log");

    let (t, rate, seed) = (T.to_string(), RATE.to_string(), SEED.to_string());
    let dev_s = dev.to_string_lossy().into_owned();
    let go_s = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &t,
            "--seed",
            &seed,
            "--rate",
            &rate,
            "--wait-file",
            &go_s,
            "--link",
            &dev_s,
            "--hold-ms",
            "5000",
            "--timeout-ms",
            "60000",
        ],
        Some(&dev),
    );

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 16384
replay_ring = {R}
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
"#,
        dev = dev.display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load ring+log config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Release the paced source; wait until it is well underway (ring full, live remains).
    std::fs::File::create(&go).expect("touch GO");
    assert!(
        wait_until(Duration::from_secs(30), || file_len(&log) >= 2 * R),
        "stream never reached 2*ring (log={} bytes)",
        file_len(&log)
    );

    // First tap: replay + live. The reconstruction frontier starts at its from_offset;
    // read until it is 1.5 rings past the start, then disconnect mid-stream (so the
    // reconnect's replay genuinely overlaps bytes we already stored).
    let (start, mut tap1) = OffsetTap::open_replay(&d.socket(), "usb0");
    let mut have_up_to = start;
    let mut out: Vec<u8> = Vec::new();
    tap1.fold(&mut have_up_to, &mut out, |frontier| {
        frontier - start >= R + R / 2
    });
    drop(tap1); // disconnect mid-stream
    assert!(
        have_up_to - start >= R + R / 2,
        "first tap stopped early at frontier {} (start {start})",
        have_up_to
    );

    // Second tap: replay again. Its ring overlaps what we already have; the fold trims by
    // offset. Continue the SAME frontier — never reset — until the source finishes and
    // the tap has drained its tail.
    let (from2, mut tap2) = OffsetTap::open_replay(&d.socket(), "usb0");
    assert!(
        from2 <= have_up_to,
        "reconnect replay began at {from2}, past our frontier {have_up_to} — a gap"
    );
    tap2.fold(&mut have_up_to, &mut out, |frontier| {
        file_len(&log) >= T && frontier >= T
    });
    drop(tap2);

    // The reconstruction is exactly the log slice [start, have_up_to) — no duplication
    // (length equals the frontier span) and byte-exact (SHA-256 against the log).
    assert_eq!(
        out.len() as u64,
        have_up_to - start,
        "reconstruction length != frontier span (duplication at the reconnect seam)"
    );
    let logged = std::fs::read(&log).expect("read log");
    assert!(have_up_to <= logged.len() as u64, "frontier past the log");
    let slice = &logged[start as usize..have_up_to as usize];
    assert_eq!(
        sha256_hex(&out),
        sha256_hex(slice),
        "offset-trimmed reconstruction != a contiguous prefix of the stream"
    );
    // We genuinely crossed the ring boundary, so the reconnect trimmed real overlap.
    assert!(
        have_up_to - start > R,
        "did not stream past one ring ({} bytes); overlap trim untested",
        have_up_to - start
    );
}

/// §11.8 property 2: `info.instance` is stable within a boot and changes across a
/// restart, so a client detects the offset reset. Runs on every platform (no device).
#[test]
fn info_instance_nonce_is_stable_within_a_boot_and_changes_on_restart() {
    let first = {
        let d = Daemon::start();
        let rpc = d.rpc();
        let a = rpc.ok("info", Value::Null);
        let b = rpc.ok("info", Value::Null);
        let inst = a["instance"].as_u64().expect("info.instance present");
        assert_eq!(
            b["instance"].as_u64(),
            Some(inst),
            "instance changed within one boot: {a} vs {b}"
        );
        inst
        // daemon drops here
    };

    // A fresh boot is a fresh process: its per-boot nonce must differ, so a client keyed
    // on it detects that the endpoint offsets have reset rather than splicing across.
    let d2 = Daemon::start();
    let second = d2.rpc().ok("info", Value::Null)["instance"]
        .as_u64()
        .expect("info.instance present after restart");
    assert_ne!(
        first, second,
        "instance nonce did not change across a daemon restart"
    );
}

/// §11.8 properties 3 + 4 (TAP-1b, and invariant 10 on the wire): every `tap.data`
/// carries a `gap_before` beside its `offset`, the offset space is strictly contiguous,
/// and a lossless paced run leaves both the per-chunk gaps and the endpoint's
/// `feed_dropped` at zero — so a consumer that reads `gap_before` can tell a hole from
/// a clean stream instead of splicing across it. The closing half pins invariant 10:
/// with the ring full, `from_offset + replay_bytes` lands exactly on the live edge
/// (`from_offset = ingested − ring.len()`, which therefore cannot underflow or drift).
/// Needs a paced software serial source, so it skips off Linux (§5).
#[test]
fn tap_data_carries_a_gap_signal_and_a_contiguous_offset_space() {
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP tap_data_carries_a_gap_signal_and_a_contiguous_offset_space: no serial dev"
        );
        return;
    };
    drop(probe); // we spawn our own gated, paced source below

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir logs");
    let log = logdir.join("serial.log");

    let (t, rate, seed) = (T.to_string(), RATE.to_string(), SEED.to_string());
    let dev_s = dev.to_string_lossy().into_owned();
    let go_s = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &t,
            "--seed",
            &seed,
            "--rate",
            &rate,
            "--wait-file",
            &go_s,
            "--link",
            &dev_s,
            "--hold-ms",
            "5000",
            "--timeout-ms",
            "60000",
        ],
        Some(&dev),
    );

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 16384
replay_ring = {R}
[[node]]
type = "log"
name = "logx"
directory = "{logdir}"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
"#,
        dev = dev.display(),
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load ring+log config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Tap from the very first byte: an empty ring under `--replay` reports the explicit
    // empty marker and a zero baseline — `from_offset = ingested − ring.len()` = 0, the
    // no-underflow edge of invariant 10.
    let (ack, mut tap) = OffsetTap::open(&d.socket(), "usb0", true);
    assert_eq!(ack["result"]["replay_bytes"], json!(0), "ack: {ack}");
    assert_eq!(ack["result"]["from_offset"], json!(0), "ack: {ack}");
    assert_eq!(
        ack["result"]["feed_dropped"],
        json!(0),
        "tap.open must report the endpoint's feed-loss baseline (TAP-1b): {ack}"
    );

    std::fs::File::create(&go).expect("touch GO");
    let frames = tap.frames(T, Duration::from_secs(60));
    let received: u64 = frames.iter().map(|(_, _, b)| b.len() as u64).sum();
    assert_eq!(received, T, "the tap received {received} of {T} bytes");

    // The lossless preconditions, asserted before the contiguity law so a genuine drop
    // is diagnosed as a drop rather than as a mysterious offset jump.
    let state = rpc.state();
    assert_eq!(
        state["taps"][0]["dropped"],
        json!(0),
        "the tap's own queue dropped bytes: {state}"
    );
    assert_eq!(
        state["endpoints"][0]["feed_dropped"],
        json!(0),
        "the paced run lost bytes at the producer→hub feed hop: {state}"
    );

    // The law: offsets are strictly contiguous, and with no feed loss every chunk
    // reports `gap_before == 0`. A `gap_before` that vanished (or that started
    // double-counting) breaks this, as does an offset space that skipped.
    let mut expect_at = 0u64;
    for (i, (offset, gap, bytes)) in frames.iter().enumerate() {
        assert_eq!(
            *offset, expect_at,
            "frame {i}: offset {offset} is not contiguous with the frontier {expect_at}"
        );
        assert_eq!(
            *gap, 0,
            "frame {i}: a gap of {gap} bytes was signalled in a lossless run"
        );
        expect_at += bytes.len() as u64;
    }

    // Byte-exact against the co-attached log: the contiguity above describes the same
    // bytes the graph actually delivered (§5 ground truth, never a judgement).
    assert!(
        wait_until(Duration::from_secs(30), || file_len(&log) >= T),
        "log never reached {T} bytes (got {})",
        file_len(&log)
    );
    let logged = std::fs::read(&log).expect("read log");
    let captured: Vec<u8> = frames.iter().flat_map(|(_, _, b)| b.clone()).collect();
    assert_eq!(
        sha256_hex(&captured),
        sha256_hex(&logged[..T as usize]),
        "the tap's contiguous stream is not the endpoint's stream"
    );

    // Invariant 10 with a *full* ring: the replay begins exactly one ring behind the
    // live edge, and its pieces are contiguous and gap-free — so `ingested − ring.len()`
    // is the ring's true base offset, which is what makes the splice exact.
    let (ack2, _tap2) = OffsetTap::open(&d.socket(), "usb0", true);
    let from2 = ack2["result"]["from_offset"].as_u64().expect("from_offset");
    let replay2 = ack2["result"]["replay_bytes"]
        .as_u64()
        .expect("replay_bytes");
    assert_eq!(replay2, R, "the ring should be full: {ack2}");
    assert_eq!(
        from2 + replay2,
        expect_at,
        "from_offset + replay_bytes must land on the live edge (ack: {ack2})"
    );
}

/// The daemon-side half of the OPFS console freeze (T6/§11.8): `load --replace` resets
/// an endpoint's offset space to 0 while `info.instance` — the per-boot nonce — is
/// deliberately unchanged, because it tracks the daemon *process* and the process did
/// not restart. A client keyed only on the nonce would splice new bytes at stale
/// offsets. What it keys on instead is the per-hub **`epoch`** on `tap.open` (§15.38):
/// a rebuilt hub reports a new one, and the browser's `offsetSpaceChanged`/`reanchor`
/// carry the stored history onto the new space. The terminal `tap.closed` (TAP-1),
/// asserted below, is the separate signal that the *stream* ended. Uses an echo device
/// so the pre-replace offset is an exact, known value; skips off Linux (§5).
#[test]
fn tap_offsets_reset_on_load_replace_while_the_instance_nonce_does_not() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP tap_offsets_reset_on_load_replace_while_the_instance_nonce_does_not: no serial dev"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
"#,
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load echo config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Six bytes ("ping1\n") round-trip through the echo device, so the endpoint's
    // offset space has demonstrably advanced.
    let (ack1, mut tap1) = OffsetTap::open(&d.socket(), "usb0", false);
    assert_eq!(ack1["result"]["from_offset"], json!(0), "ack: {ack1}");
    rpc.send("usb0", "ping1", false, 5000).expect("send ping1");
    let frames = tap1.frames(6, Duration::from_secs(15));
    assert_eq!(
        frames.iter().map(|(_, _, b)| b.len()).sum::<usize>(),
        6,
        "the tap never saw the echoed line: {frames:?}"
    );
    let (ack_mid, _tap_mid) = OffsetTap::open(&d.socket(), "usb0", false);
    assert_eq!(
        ack_mid["result"]["from_offset"],
        json!(6),
        "the live edge did not advance with the delivered bytes: {ack_mid}"
    );

    let instance = rpc.info()["instance"].as_u64().expect("info.instance");

    // Replace the graph: every hub is dropped and rebuilt.
    rpc.load_toml(&cfg, true).expect("load --replace");
    assert_eq!(
        tap1.wait_closed(Duration::from_secs(5)).as_deref(),
        Some("graph replaced"),
        "the tap was orphaned instead of being told its endpoint was replaced (TAP-1)"
    );
    // The rebuilt node's *device* state is deliberately not asserted here: a tap hub
    // exists for every host-facing endpoint from load onward, whatever the device is
    // doing (§15.8), and this test is about the endpoint's offset space, not the port.

    // The nonce is unchanged — it tracks the daemon *process*, not the graph …
    assert_eq!(
        rpc.info()["instance"].as_u64(),
        Some(instance),
        "info.instance changed on a graph replace"
    );
    // … while the offset space restarted at 0. A client that spliced by offset across
    // this would corrupt its history; the rebuilt hub's fresh `epoch` is what tells it
    // not to (§15.38), and `tap.closed` above is what told it the stream had ended.
    let (ack2, _tap2) = OffsetTap::open(&d.socket(), "usb0", false);
    assert_eq!(
        ack2["result"]["from_offset"],
        json!(0),
        "the rebuilt endpoint did not restart its offset space: {ack2}"
    );
}
