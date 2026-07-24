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

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

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
    fn open_replay(socket: &Path, endpoint: &str) -> (u64, Self) {
        let mut stream = UnixStream::connect(socket).expect("connect tap socket");
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tap.open",
            "params": { "endpoint": endpoint, "replay": true },
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
        let from_offset = v["result"]["from_offset"]
            .as_u64()
            .unwrap_or_else(|| panic!("from_offset missing in ack: {ack}"));
        (from_offset, conn)
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
