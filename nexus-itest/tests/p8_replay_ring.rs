//! Phase 8 replay-ring slice, ported from `scripts/validate/phase8/replay-ring.sh`
//! (design §5 / §17, the web-console track). `replay_ring = <bytes>` on a
//! host-facing endpoint retains the most recent hostward bytes so a late
//! `tap.open --replay` sees what just happened. Three properties:
//!
//! 1. **Exact splice** — a replay tap opened mid-stream receives ring-then-live
//!    with no gap and no duplication, i.e. exactly a contiguous suffix of the
//!    hostward stream. Ground truth: the tap bytes are byte-exact (SHA-256) the
//!    *tail* of the `log` node of the same length (§5).
//! 2. **Empty-replay marker** — a ring-off endpoint, and an as-yet-empty ring,
//!    each answer `--replay` with `replay_bytes = 0` (§17).
//! 3. The `replay_ring` attribute **round-trips** through `dump`/`load` (§8/§11).
//!
//! Deviations from the bash, each preserving the original *assertions*:
//! * `stat -c %s` → [`file_len`]; `jq` on `serialnexusctl` text → structured RPC on
//!   `dump`/`state`; `sha256sum` + `tail -c` → [`sha256_hex`] over the in-memory tail;
//!   `timeout`/`sleep` → bounded [`wait_until`] and idle-drain polling.
//! * The bash read `replay_bytes` from a background `serialnexusctl tap` process's
//!   stderr banner. Rust hand-rolls the tap connection over a raw `UnixStream`
//!   ([`TapConn`]) so it captures **both** the `tap.open` ack (`replay_bytes`) and the
//!   `tap.data` notification stream on the *one* connection — the splice guarantee is
//!   per-tap, so a single connection must observe both halves.
//! * Properties 2 and 3 are pure config / tap-hub facts, independent of any live
//!   device: every host-facing endpoint gets a tap hub at load, ring or not, even
//!   while the serial node is `waiting` (its device absent, §15.8). So they are
//!   pulled into a test that runs on **every** platform (the identical empty-marker
//!   and dump-round-trip assertions over two ring / ring-off serial nodes with absent
//!   devices), per the harness rule that device-free properties must not skip.
//! * The exact-splice property needs a paced software serial *source* (a seeded
//!   `nexus-sim pty --source` standing in for a UART), which is the Linux
//!   software-loopback doctrine (`serial2` rejects a pty on macOS — `ENOTTY`), so
//!   that test **skips** off Linux, gated on [`serial_echo`] presence (a skip is a
//!   valid verdict, §5).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

const R: u64 = 65536; // ring depth: 64 KiB
const T: u64 = 524288; // total streamed: 512 KiB
const RATE: u64 = 262144; // 256 KiB/s → the stream lasts ~2s, tap opens mid-stream
const SEED: u64 = 23;

/// Current on-disk length of `p` (0 if absent) — the portable replacement for
/// `stat -c %s … || echo 0`.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// Standard base64 decode (alphabet `A-Za-z0-9+/`, `=` padding) — the inverse of the
/// daemon's `nexus_rpc::base64_encode` used for `tap.data`. Each `tap.data` payload is
/// its own padded string, so decoding per-message and concatenating is exact.
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
    let mut acc = 0u32;
    let mut nbits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue }; // skip any stray whitespace/newlines
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    out
}

/// The `replay_ring` attribute of the node named `name` in a `dump` config, or `None`.
fn dump_replay_ring(dump: &Value, name: &str) -> Option<u64> {
    dump.get("node")?
        .as_array()?
        .iter()
        .find(|n| n.get("name").and_then(Value::as_str) == Some(name))?
        .get("replay_ring")?
        .as_u64()
}

/// A hand-rolled tap connection over the raw control socket. Unlike the harness's
/// `Subscription` (which discards the request ack), this captures the `tap.open` ack —
/// carrying `replay_bytes` — *and* the following `tap.data` notification stream on the
/// same connection, which the per-tap exact-splice guarantee requires.
struct TapConn {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl TapConn {
    /// Open a `--replay` tap on `endpoint` and return `(replay_bytes, conn)`. The ack
    /// is the first line on the wire (written before any `tap.data` is drained), so the
    /// first `read_line` here is always the ack.
    fn open_replay(socket: &Path, endpoint: &str) -> (u64, Self) {
        let mut stream = UnixStream::connect(socket).expect("connect tap socket");
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tap.open",
            "params": { "endpoint": endpoint, "replay": true },
        });
        stream
            .write_all(format!("{req}\n").as_bytes())
            .expect("write tap.open");
        stream.flush().expect("flush tap.open");
        let mut conn = TapConn {
            stream,
            buf: Vec::new(),
        };
        let ack = conn
            .read_line(Duration::from_secs(10))
            .expect("tap.open ack line");
        let v: Value = serde_json::from_str(&ack).expect("parse tap.open ack");
        let replay_bytes = v["result"]["replay_bytes"]
            .as_u64()
            .unwrap_or_else(|| panic!("replay_bytes missing in ack: {ack}"));
        (replay_bytes, conn)
    }

    /// Read one `\n`-terminated line by `timeout`, buffering across reads, or `None`.
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
                Err(_) => return None, // WouldBlock/TimedOut/closed
            }
        }
    }

    /// Collect the decoded `tap.data` payload until the hostward stream is complete
    /// (`log` at full size) and the tap has been quiet long enough to be drained.
    /// Bounded by a hard cap — no unbounded wait. The 1200 ms quiet window
    /// comfortably exceeds the paced source's ~250 ms inter-block gap, so it never
    /// stops mid-stream.
    fn collect_until_drained(&mut self, log: &Path) -> Vec<u8> {
        let mut out = Vec::new();
        let hard = Instant::now() + Duration::from_secs(40);
        let mut last_data = Instant::now();
        loop {
            if Instant::now() >= hard {
                break;
            }
            if let Some(line) = self.read_line(Duration::from_millis(500))
                && let Ok(v) = serde_json::from_str::<Value>(&line)
                && v.get("method").and_then(Value::as_str) == Some("tap.data")
                && let Some(data) = v
                    .get("params")
                    .and_then(|p| p.get("data"))
                    .and_then(Value::as_str)
            {
                out.extend_from_slice(&base64_decode(data));
                last_data = Instant::now();
            }
            // Done once the source has fully written and the tap has quiesced.
            if file_len(log) >= T && last_data.elapsed() >= Duration::from_millis(1200) {
                break;
            }
        }
        out
    }
}

// ---- Properties 2 + 3: empty-replay marker + dump round-trip (every platform) ----

/// The `replay_ring` attribute round-trips through `dump`, and `--replay` on a
/// ring-off endpoint and on a configured-but-empty ring each report `replay_bytes = 0`
/// (§17's explicit empty-replay marker). Pure config / tap-hub facts: a tap hub exists
/// for every host-facing endpoint from load, even while the serial node is `waiting`
/// with an absent device (§15.8), so this needs no serial device and runs everywhere.
#[test]
fn replay_ring_attribute_round_trips_and_empty_marker() {
    let d = Daemon::start();
    let rpc = d.rpc();

    // usb0 carries a 64 KiB ring, usb1 none; both devices are absent, so both load
    // `waiting` (load never fails on a missing device, §15.8) — but their tap hubs
    // exist from the first instant.
    let absent0 = d.run().join("absent-usb0");
    let absent1 = d.run().join("absent-usb1");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev0}"
arbitration = "free-for-all"
replay_ring = {ring}
[[node]]
type = "serial"
name = "usb1"
device = "{dev1}"
arbitration = "free-for-all"
"#,
        dev0 = absent0.display(),
        dev1 = absent1.display(),
        ring = R,
    );
    rpc.load_toml(&cfg, false)
        .expect("load ring / ring-off config");

    // (3) dump/load round-trip: usb0 keeps its configured depth, usb1 reports 0.
    let dump = rpc.dump();
    assert_eq!(
        dump_replay_ring(&dump, "usb0"),
        Some(R),
        "replay_ring did not round-trip through dump: {dump}"
    );
    assert_eq!(
        dump_replay_ring(&dump, "usb1"),
        Some(0),
        "ring-off usb1 should report replay_ring = 0: {dump}"
    );

    // (2) empty-replay marker on a ring-off endpoint (usb1) and a configured but
    // still-empty ring (usb0). A one-shot `tap.open` call closes its connection on
    // return, detaching the tap it opened — enough to read the ack's `replay_bytes`.
    let off = rpc.ok("tap.open", json!({ "endpoint": "usb1", "replay": true }));
    assert_eq!(
        off["replay_bytes"].as_u64(),
        Some(0),
        "ring-off endpoint replay_bytes != 0: {off}"
    );
    let empty = rpc.ok("tap.open", json!({ "endpoint": "usb0", "replay": true }));
    assert_eq!(
        empty["replay_bytes"].as_u64(),
        Some(0),
        "empty (unfilled) ring replay_bytes != 0: {empty}"
    );
}

// ---- Property 1: exact ring-then-live splice (Linux serial device only) -----------

/// A replay tap opened mid-stream captures exactly a contiguous suffix of the
/// hostward stream — ring-then-live with no gap and no duplication — so its bytes
/// equal the last `tap_len` bytes of the byte-exact log. Needs a paced software
/// serial source (Linux only); skips where none exists.
#[test]
fn replay_tap_splices_ring_then_live_exactly() {
    // `serial_echo()` is `Some` exactly on a platform where a sim pty can stand in as a
    // serial device; we discard the echo (we spawn our own paced *source* below).
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP replay_tap_splices_ring_then_live_exactly: no serial device on this platform"
        );
        return;
    };
    drop(probe);

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let log = logdir.join("serial.log");

    // usb0's device: a paced seeded source, gated so it writes nothing until GO exists
    // (so usb0 comes up active with an empty ring), then holds the pts present.
    let t_str = T.to_string();
    let rate_str = RATE.to_string();
    let seed_str = SEED.to_string();
    let dev_str = dev.to_string_lossy().into_owned();
    let go_str = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &t_str,
            "--seed",
            &seed_str,
            "--rate",
            &rate_str,
            "--wait-file",
            &go_str,
            "--link",
            &dev_str,
            "--hold-ms",
            "5000",
            "--timeout-ms",
            "60000",
        ],
        Some(&dev),
    );

    // usb0 carries a 64 KiB replay ring and a byte-exact log anchor of its stream.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 16384
replay_ring = {ring}
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
        ring = R,
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load replay-ring config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Before releasing the gated source the ring is empty → the empty-replay marker.
    let gated = rpc.ok("tap.open", json!({ "endpoint": "usb0", "replay": true }));
    assert_eq!(
        gated["replay_bytes"].as_u64(),
        Some(0),
        "gated-source ring should be empty: {gated}"
    );

    // Release the paced source, then wait until the ring is full and live bytes still
    // remain (log has passed 2*R; the 512 KiB stream at 256 KiB/s lasts ~2s).
    std::fs::File::create(&go).expect("touch GO gate");
    assert!(
        wait_until(Duration::from_secs(30), || file_len(&log) >= 2 * R),
        "stream never reached 2*ring before the tap opened (log={} bytes)",
        file_len(&log)
    );

    // Open the replay tap on the one connection that observes both halves. The ring was
    // full at open, so the replay prefix is exactly R bytes (§5 full-ring marker).
    let (replay_bytes, mut tap) = TapConn::open_replay(&d.socket(), "usb0");
    assert_eq!(
        replay_bytes, R,
        "full-ring replay_bytes = {replay_bytes}, expected {R}"
    );
    // Exactly one open tap registers in `state` (§17), on usb0. Bounded poll: the
    // one-shot empty-marker tap above closes when its connection drops.
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.state()["taps"].as_array().is_some_and(|t| t.len() == 1)
        }),
        "replay tap did not register as the sole open tap (taps={})",
        rpc.state()["taps"]
    );

    // Collect ring+live until the source finishes and the tap drains its tail.
    let captured = tap.collect_until_drained(&log);
    assert!(
        wait_until(Duration::from_secs(30), || file_len(&log) >= T),
        "log never reached the full {T} bytes (got {})",
        file_len(&log)
    );

    let tap_len = captured.len() as u64;
    // At least the ring, at most the whole stream (more would be duplication).
    assert!(
        tap_len >= R,
        "replay tap captured {tap_len} bytes, expected at least the {R}-byte ring"
    );
    assert!(
        tap_len <= T,
        "replay tap captured {tap_len} bytes, more than the {T} streamed (duplication)"
    );

    // The captured replay+live must equal exactly the last `tap_len` bytes of the log
    // (no gap, no duplication at the ring/live seam) — the exact-splice guarantee.
    let logged = std::fs::read(&log).expect("read serial.log");
    assert_eq!(logged.len() as u64, T, "log length != {T}");
    let tail = &logged[logged.len() - tap_len as usize..];
    assert_eq!(
        sha256_hex(&captured),
        sha256_hex(tail),
        "replay+live != a contiguous suffix of the stream (gap or duplication at the splice)"
    );
}
