//! Phase 8 tap slice, ported from `scripts/validate/phase8/tap.sh` (design §5 / §17,
//! the web-console track). A **tap** is a connection-scoped, read-only dynamic
//! attachment on a host-facing endpoint — the `never` write mode in dynamic form.
//! On a no-hardware rig this asserts three properties:
//!
//! 1. **Faithful mirror** — the tap's hostward bytes are byte-exact (SHA-256) equal to
//!    both a co-attached `log` consumer of the same endpoint AND the seeded source that
//!    produced them; the tap neither drops nor corrupts a byte (§5).
//! 2. **Taps are state, never config** — `dump` is byte-identical while a tap is open,
//!    so a viewer's tap cannot leak into the operator-owned graph (§8/§11).
//! 3. **Detach on connection drop** — closing the tap's control connection detaches it
//!    (`state` shows zero taps), promptly even on an idle endpoint (§17).
//!
//! Deviations from the bash, each preserving the original *assertions*:
//! * `stat -c %s` → [`file_len`]; `jq` over `serialnexusctl` text → structured RPC on
//!   `dump`/`state`; `sha256sum` → [`sha256_hex`]; `timeout`/`sleep` → bounded
//!   [`wait_until`] and a deadline-bounded collect loop.
//! * The bash read the source's checksum from the sim's stdout verdict. The harness
//!   nulls a sim's stdout, so the source's SHA is recomputed independently from the same
//!   seed ([`seeded_bytes`], the sim's SplitMix64) — a *stronger* ground truth than
//!   trusting the sim's self-report, and it means tap == log == seed are three
//!   independent computations, not one value compared to itself.
//! * The bash tap was a background `serialnexusctl tap` process reading decoded bytes to
//!   a file; Rust opens the tap directly over the control socket (the harness
//!   [`Subscription`] from `tap.open`) and decodes each `tap.data` payload in-process.
//!
//! Needs a serial device (a `nexus-sim pty --source` standing in for the UART), so it
//! **skips** where a sim pty cannot be a serial device (macOS) — gated on [`serial_echo`]
//! per the harness doctrine (§5).

use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, Subscription, serial_echo, sha256_hex, wait_until};
use serde_json::{Value, json};

const N: usize = 262144; // 256 KiB — well under the tap queue bound, so no drops here.
const SEED: u64 = 7;

/// Current on-disk length of `p` (0 if absent) — the portable replacement for
/// `stat -c %s … || echo 0`.
fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// The sim's deterministic payload generator (SplitMix64), reimplemented so the test
/// owns the source's ground-truth checksum without parsing the sim's (nulled) stdout.
/// `N` is a multiple of 8, so this is byte-identical to `nexus-sim`'s output.
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

/// Number of open taps `state` currently reports.
fn tap_count(rpc: &nexus_itest::Rpc) -> usize {
    rpc.state()["taps"].as_array().map(Vec::len).unwrap_or(0)
}

/// Drain `tap.data` notifications from `sub`, decoding and concatenating their payloads
/// until `want` bytes are collected or `timeout` elapses. Ignores any non-`tap.data`
/// line. Bounded — no unbounded wait.
fn collect_tap(sub: &mut Subscription, want: usize, timeout: Duration) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut out = Vec::with_capacity(want);
    while out.len() < want {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match sub.next(deadline - now) {
            Some(v) if v.get("method").and_then(Value::as_str) == Some("tap.data") => {
                if let Some(data) = v
                    .get("params")
                    .and_then(|p| p.get("data"))
                    .and_then(Value::as_str)
                {
                    out.extend_from_slice(&base64_decode(data));
                }
            }
            Some(_) => continue, // a non-tap notification; keep waiting
            None => break,       // timeout / connection closed
        }
    }
    out
}

#[test]
fn tap_mirrors_stream_stays_out_of_config_and_detaches_on_drop() {
    // Needs a sim pty acting as a serial device (Linux); skip on macOS (§5).
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP tap_mirrors_stream_stays_out_of_config_and_detaches_on_drop: \
             no serial device on this platform"
        );
        return;
    };
    drop(probe); // we spawn our own gated source double below

    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("dev");
    let go = d.run().join("go");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let log = logdir.join("serial.log");

    // usb0's device: a seeded source gated on GO so its payload cannot outrun a
    // not-yet-draining consumer (plan §3, presence != readiness). --hold-ms keeps the
    // device "present" after the write so the serial node does not see a mid-stream HUP.
    let n_str = N.to_string();
    let seed_str = SEED.to_string();
    let dev_str = dev.to_string_lossy().into_owned();
    let go_str = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &n_str,
            "--seed",
            &seed_str,
            "--wait-file",
            &go_str,
            "--link",
            &dev_str,
            "--hold-ms",
            "2000",
            "--timeout-ms",
            "40000",
        ],
        Some(&dev),
    );

    // serial usb0 → log logx: the log is an always-attached byte-exact co-consumer of
    // the hostward stream; the tap attaches to the same endpoint.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 8192
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
    rpc.load_toml(&cfg, false).expect("load tap config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // dump BEFORE any tap: the baseline the open-tap dump must match exactly.
    let dump_pre = rpc.dump();

    // Open the tap on usb0 over the control socket. Holding the `Subscription` keeps the
    // connection open, so the tap stays registered until we drop it.
    let mut tap = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));

    // The tap is active once state lists exactly one tap on usb0.
    assert!(
        wait_until(Duration::from_secs(5), || {
            let taps = rpc.state()["taps"].clone();
            taps.as_array().map(Vec::len) == Some(1) && taps[0]["endpoint"].as_str() == Some("usb0")
        }),
        "tap did not register on usb0 (taps={:?})",
        rpc.state()["taps"]
    );

    // dump WHILE the tap is open must be byte-identical: a tap never touches config (§8).
    let dump_mid = rpc.dump();
    assert_eq!(
        dump_pre, dump_mid,
        "dump changed while a tap was open (a tap leaked into configuration)"
    );

    // Release the source: N seeded bytes flow device → serial → {log, tap}.
    std::fs::File::create(&go).expect("touch GO gate");

    // The tap receives exactly N hostward bytes (no drops at this size).
    let captured = collect_tap(&mut tap, N, Duration::from_secs(40));
    assert_eq!(
        captured.len(),
        N,
        "tap delivered {} bytes, expected {N} (dropped or truncated)",
        captured.len()
    );

    // The co-attached log reaches N bytes.
    assert!(
        wait_until(Duration::from_secs(20), || file_len(&log) == N as u64),
        "log file never reached {N} bytes (got {})",
        file_len(&log)
    );
    let logged = std::fs::read(&log).expect("read serial.log");

    // Byte-exact: tap == co-attached log == the seeded source (three independent
    // computations). Any drop or corruption on the tap path breaks one of these.
    let src_sha = sha256_hex(&seeded_bytes(SEED, N));
    let tap_sha = sha256_hex(&captured);
    let log_sha = sha256_hex(&logged);
    assert_eq!(tap_sha, log_sha, "tap checksum != co-attached log checksum");
    assert_eq!(
        tap_sha, src_sha,
        "tap checksum != source checksum (tap dropped or corrupted bytes)"
    );

    // Drop the tap's connection: state must show zero taps (detach-on-drop).
    drop(tap);
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 0),
        "tap did not detach after its connection dropped (taps={:?})",
        rpc.state()["taps"]
    );

    // Explicit connection-drop test: open a fresh tap, confirm it registers, drop its
    // connection, and confirm prompt detach even with an idle endpoint (the source is
    // done and the device may have gone away, but the endpoint's hub persists).
    let watch = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 1),
        "persistent tap did not register (taps={:?})",
        rpc.state()["taps"]
    );
    drop(watch);
    assert!(
        wait_until(Duration::from_secs(5), || tap_count(rpc) == 0),
        "dropped tap did not detach (taps={:?})",
        rpc.state()["taps"]
    );
}
