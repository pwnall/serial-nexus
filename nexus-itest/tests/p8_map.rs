//! Console map track (plan §12.1; design §7.8, §15.33): the per-console character
//! map node, driven end-to-end through the daemon.
//!
//! Three properties, each pinned against an **independent in-test oracle** (never
//! the daemon's own `nexus_core::map`, so the test is a genuine cross-check, not a
//! tautology — the same discipline p8_tap uses for its seeded source):
//!
//! 1. **Unknown mapping is structural** — a `load` naming a mapping outside picocom's
//!    vocabulary is refused with the offending name, nothing created (cross-platform;
//!    no serial device).
//! 2. **Hostward transform is byte-exact, raw and mapped views coexist** — a seeded
//!    source through a `map` node: a tap on the map's mapped endpoint equals the
//!    oracle-mapped stream byte-for-byte (SHA-256), a tap on the *upstream* endpoint
//!    equals the raw seeded stream, the per-rule/byte counters match the oracle's
//!    tallies, and the map's default replay ring holds the mapped tail (§7.8, §15.32).
//! 3. **Steal-to-bypass speaks mapped, then raw, then mapped again** — `send` at the
//!    map's endpoint reaches the device mapped; `send --steal` at the upstream reaches
//!    it raw (verbatim); a subsequent `send` at the map is mapped again, proving the
//!    map reclaims its held edge (§6 held priority, §7.8 steal-to-bypass).
//!
//! Properties 2–3 need a serial *device*, so they **skip** where a sim pty cannot be
//! one (macOS: `serial2` → `ENOTTY`), per the harness doctrine (§5). Property 1 runs
//! everywhere — a map is an interior transform with no device of its own.

use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, Subscription, serial_echo, serial_pair, sha256_hex, wait_until};
use serde_json::{Value, json};

// ---- Independent oracles (reimplemented here, never nexus_core::map) --------------

/// picocom's hex form for one byte: `[` + two lowercase hex digits + `]` (§7.8).
fn hex4(b: u8) -> Vec<u8> {
    format!("[{b:02x}]").into_bytes()
}

/// The hostward oracle for `["8bithex", "crlf"]`: any 8-bit byte (≥0x80) → `[xx]`,
/// CR (0x0d) → LF (0x0a), everything else verbatim — first match, in that order.
fn oracle_hostward(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in input {
        if b >= 0x80 {
            out.extend_from_slice(&hex4(b));
        } else if b == 0x0d {
            out.push(0x0a);
        } else {
            out.push(b);
        }
    }
    out
}

/// The targetward oracle for `["lfcrlf"]`: LF (0x0a) → CR LF, everything else verbatim.
fn oracle_targetward(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in input {
        if b == 0x0a {
            out.extend_from_slice(b"\r\n");
        } else {
            out.push(b);
        }
    }
    out
}

/// The sim's deterministic SplitMix64 payload — reimplemented so the test owns the
/// source's ground truth (identical to `nexus-sim`; `len` a multiple of 8).
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
    let mut acc = 0u32;
    let mut nbits = 0u32;
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

/// Drain `tap.data` notifications, concatenating decoded payloads until `want` bytes
/// or `timeout`. Bounded — no unbounded wait.
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
            Some(_) => continue,
            None => break,
        }
    }
    out
}

fn file_bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_default()
}

#[test]
fn packaging_example_config_validates_with_the_map_present() {
    // Plan §12.2: the shipped example config load-verifies with the map present.
    // Validated purely against the real graph validator — no daemon, no filesystem
    // side effects, every platform.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("packaging/serialnexusd.example.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let cfg: nexus_core::GraphConfig =
        toml::from_str(&text).expect("example config parses as a GraphConfig");
    let errors = cfg.validate();
    assert!(
        errors.is_empty(),
        "the packaging example config must be structurally valid: {errors:?}"
    );
    // The mapped quirky console is present, with both mapping directions.
    let map = cfg
        .nodes
        .iter()
        .find(|n| n.name() == "qcon")
        .expect("example must contain the `qcon` map node");
    match map {
        nexus_core::config::NodeConfig::Map {
            hostward,
            targetward,
            ..
        } => {
            assert_eq!(hostward, &["lfcrlf"], "hostward normalizes bare LF (§7.8)");
            assert_eq!(targetward, &["lfcr"], "targetward satisfies CR (§7.8)");
        }
        other => panic!("`qcon` should be a map node, got {other:?}"),
    }
}

#[test]
fn unknown_mapping_name_is_a_structural_load_error() {
    // A map naming a mapping outside picocom's vocabulary is structural (§7.8): the
    // load is refused, the offending name is in the error, and nothing is created.
    // No serial device needed — runs on every platform.
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = r#"
[[node]]
type = "map"
name = "console"
hostward = ["crlf", "bogus-mapping"]
targetward = ["lfcr"]
"#;
    let err = rpc
        .load_toml(cfg, false)
        .expect_err("a bogus mapping name must fail the load structurally");
    assert_eq!(err.code, -32002, "structural error code (§16.8): {err:?}");
    assert!(
        err.message.contains("bogus-mapping"),
        "the error must name the offending mapping, got: {}",
        err.message
    );
    // Nothing was created — the graph is still empty.
    assert!(
        rpc.state()["nodes"].as_array().map(Vec::is_empty) == Some(true),
        "a structural error must create nothing: {:?}",
        rpc.state()["nodes"]
    );
}

#[test]
fn map_hostward_transforms_byte_exact_with_raw_and_mapped_views() {
    // Needs a sim pty acting as a serial device (Linux); skip on macOS (§5). Use the
    // provider only as a platform gate, then spawn our own gated source in the daemon's
    // run dir (the p8_tap pattern) — the provider's own temp dir is removed on drop.
    let Some(probe) = serial_echo() else {
        eprintln!(
            "SKIP map_hostward_transforms_byte_exact_with_raw_and_mapped_views: \
             no serial device on this platform"
        );
        return;
    };
    drop(probe);

    const N: usize = 131072; // 128 KiB seeded → mapped ~320 KiB (8bithex on ~half)
    const SEED: u64 = 11;
    let seeded = seeded_bytes(SEED, N);
    let mapped = oracle_hostward(&seeded);

    let d = Daemon::start();
    let rpc = d.rpc();
    let go = d.run().join("go");
    let dev_path = d.run().join("dev");
    let dev = dev_path.to_string_lossy().into_owned();

    // A seeded, GO-gated source so its payload cannot outrun a not-yet-draining tap
    // (plan §3, presence != readiness); --hold-ms keeps the device present after the
    // write so the serial node sees no mid-stream HUP.
    let go_str = go.to_string_lossy().into_owned();
    let _source = Sim::spawn(
        &[
            "pty",
            "--source",
            "--bytes",
            &N.to_string(),
            "--seed",
            &SEED.to_string(),
            "--wait-file",
            &go_str,
            "--link",
            &dev,
            "--hold-ms",
            "3000",
            "--timeout-ms",
            "40000",
        ],
        Some(&dev_path),
    );

    // usb0 (host) --held--> console/raw (map) --> [no graph consumer; taps observe].
    // hostward maps 8bithex then crlf; the mapped endpoint is `console`, the raw
    // upstream endpoint is `usb0`. hostward_buffer high so the map's intake never
    // sheds at this size.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
hostward_buffer = 8192
[[node]]
type = "map"
name = "console"
hostward = ["8bithex", "crlf"]
[[edge]]
a = "usb0"
b = "console/raw"
write_mode = "held"
"#,
    );
    rpc.load_toml(&cfg, false).expect("load map graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "map node not active: {:?}",
        rpc.node("console")
    );

    // Tap both views before releasing the source: `console` = the mapped stream,
    // `usb0` = the raw stream. Both are host-facing endpoints with taps + rings (§7.8).
    let mut tap_mapped = rpc.stream("tap.open", json!({ "endpoint": "console" }));
    let mut tap_raw = rpc.stream("tap.open", json!({ "endpoint": "usb0" }));
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.state()["taps"].as_array().map(Vec::len) == Some(2)
        }),
        "both taps did not register: {:?}",
        rpc.state()["taps"]
    );

    // Release the source: N seeded bytes flow device → usb0 → {raw tap, map → mapped tap}.
    std::fs::File::create(&go).expect("touch GO gate");

    let got_mapped = collect_tap(&mut tap_mapped, mapped.len(), Duration::from_secs(40));
    let got_raw = collect_tap(&mut tap_raw, N, Duration::from_secs(40));

    // The mapped tap equals the oracle-mapped stream, byte-for-byte.
    assert_eq!(
        got_mapped.len(),
        mapped.len(),
        "mapped tap delivered {} bytes, expected {}",
        got_mapped.len(),
        mapped.len()
    );
    assert_eq!(
        sha256_hex(&got_mapped),
        sha256_hex(&mapped),
        "mapped stream did not match the independent hostward oracle"
    );
    // The raw tap equals the seeded source — raw and mapped views coexist (§7.8).
    assert_eq!(
        sha256_hex(&got_raw),
        sha256_hex(&seeded),
        "raw upstream tap did not match the seeded source (raw view corrupted)"
    );

    // Per-rule + per-direction counters match the oracle's tallies (§7.8).
    let eight_bit = seeded.iter().filter(|&&b| b >= 0x80).count() as u64;
    let cr = seeded.iter().filter(|&&b| b == 0x0d).count() as u64;
    let settled = wait_until(Duration::from_secs(10), || {
        rpc.node("console")
            .and_then(|n| n["hostward"]["bytes_in"].as_u64())
            == Some(N as u64)
    });
    let node = rpc.node("console").expect("map node in state");
    assert!(settled, "map hostward counters did not settle: {node}");
    assert_eq!(
        node["hostward"]["bytes_in"].as_u64(),
        Some(N as u64),
        "hostward bytes_in: {node}"
    );
    assert_eq!(
        node["hostward"]["bytes_out"].as_u64(),
        Some(mapped.len() as u64),
        "hostward bytes_out must equal the mapped length: {node}"
    );
    assert_eq!(
        node["hostward"]["rules"]["8bithex"].as_u64(),
        Some(eight_bit),
        "8bithex substitution count must match the oracle: {node}"
    );
    assert_eq!(
        node["hostward"]["rules"]["crlf"].as_u64(),
        Some(cr),
        "crlf substitution count must match the oracle: {node}"
    );

    // The map's default replay ring holds the mapped tail: a fresh replay tap opened
    // after the stream drains delivers the last 64 KiB of the mapped stream, exactly
    // (§15.32 splice, on the map's mapped endpoint like any host endpoint). Race-free
    // because the source is done — the replay tap sees only the ring, no live bytes.
    const RING: usize = 65536;
    // Drop the live taps so the fresh replay tap is the only one and the source is idle.
    drop(tap_mapped);
    drop(tap_raw);
    let mut replay = rpc.stream("tap.open", json!({ "endpoint": "console", "replay": true }));
    let want_tail = &mapped[mapped.len() - RING..];
    let got_tail = collect_tap(&mut replay, RING, Duration::from_secs(10));
    assert_eq!(
        got_tail.len(),
        RING,
        "replay tap delivered {} ring bytes, expected {RING}",
        got_tail.len()
    );
    assert_eq!(
        sha256_hex(&got_tail),
        sha256_hex(want_tail),
        "the map's replay ring did not hold the exact mapped tail (§15.32 splice)"
    );
}

#[test]
fn map_steal_to_bypass_speaks_mapped_then_raw() {
    // Needs a lossless serial null modem (Linux); skip elsewhere (§5).
    let Some(pair) = serial_pair() else {
        eprintln!(
            "SKIP map_steal_to_bypass_speaks_mapped_then_raw: no serial device on this platform"
        );
        return;
    };
    let (end_a, end_b) = pair.ports();
    let (end_a, end_b) = (end_a.to_owned(), end_b.to_owned());

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log dir");
    let devlog = logdir.join("dev.log");

    // usb0 (end A, host) --held--> console/raw (map, targetward=lfcrlf) --> console.
    // devsink opens end B and logs whatever crosses the null modem — the device's view
    // of the bytes usb0 wrote targetward. `send console` speaks mapped; `send usb0
    // --steal` speaks raw (§7.8 steal-to-bypass).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{end_a}"
[[node]]
type = "map"
name = "console"
targetward = ["lfcrlf"]
[[node]]
type = "serial"
name = "devsink"
device = "{end_b}"
arbitration = "free-for-all"
hostward_buffer = 4096
[[node]]
type = "log"
name = "devlog"
directory = "{logdir}"
filename = "dev.log"
[[edge]]
a = "usb0"
b = "console/raw"
write_mode = "held"
[[edge]]
a = "devsink"
b = "devlog"
write_mode = "never"
"#,
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load steal-bypass graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("devsink", "active", Duration::from_secs(20)),
        "serial ends not active: usb0={:?} devsink={:?}",
        rpc.node("usb0"),
        rpc.node("devsink")
    );

    // The map holds usb0's write lock via its held raw edge (§6): holder = "console/raw".
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("usb0")
                .and_then(|n| n["lock"]["holder"].as_str().map(str::to_owned))
                == Some("console/raw".to_owned())
        }),
        "the map should hold usb0's lock (holder=console/raw): {:?}",
        rpc.node("usb0")
    );

    // Waits until devlog's on-disk bytes exactly equal `want` (the log flushes as its
    // writer drains; a bounded poll absorbs the lag).
    let wait_devlog = |want: &[u8]| -> bool {
        let want = want.to_vec();
        wait_until(Duration::from_secs(10), || file_bytes(&devlog) == want)
    };

    // (1) send at the map's endpoint: "map" + the send's trailing \n → lfcrlf → "map\r\n".
    rpc.send("console", "map", false, 5000)
        .expect("send mapped");
    assert!(
        wait_devlog(&oracle_targetward(b"map\n")),
        "device did not receive the mapped bytes; devlog={:?}",
        file_bytes(&devlog)
    );

    // (2) steal the upstream and send raw: "raw\n" reaches the device verbatim (no
    // lfcrlf), appended after the mapped bytes.
    rpc.send("usb0", "raw", true, 5000)
        .expect("send raw (steal)");
    let after_raw = [oracle_targetward(b"map\n"), b"raw\n".to_vec()].concat();
    assert!(
        wait_devlog(&after_raw),
        "device did not receive the raw (verbatim) bytes after the steal; devlog={:?}",
        file_bytes(&devlog)
    );

    // (3) send at the map again: the map reclaims its held edge (§6 held priority) and
    // resumes mapping — "back\n" → "back\r\n".
    rpc.send("console", "back", false, 5000)
        .expect("send mapped again after steal");
    let after_back = [after_raw.clone(), oracle_targetward(b"back\n")].concat();
    assert!(
        wait_devlog(&after_back),
        "the map did not resume mapping after the steal; devlog={:?}",
        file_bytes(&devlog)
    );

    // Targetward counters cross-check (plan §12.1, §7.8): the map processed "map\n"
    // and "back\n" (the stolen "raw\n" bypassed the map, so it is NOT counted here).
    // bytes_out reflects the lfcrlf expansion; lfcrlf fired once per mapped send.
    let want_in = (b"map\n".len() + b"back\n".len()) as u64; // 9
    let want_out = (oracle_targetward(b"map\n").len() + oracle_targetward(b"back\n").len()) as u64; // 11
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("console")
                .and_then(|n| n["targetward"]["bytes_in"].as_u64())
                == Some(want_in)
        }),
        "targetward bytes_in did not settle to {want_in}: {:?}",
        rpc.node("console")
    );
    let node = rpc.node("console").expect("map node in state");
    assert_eq!(
        node["targetward"]["bytes_out"].as_u64(),
        Some(want_out),
        "targetward bytes_out must reflect the lfcrlf expansion: {node}"
    );
    assert_eq!(
        node["targetward"]["rules"]["lfcrlf"].as_u64(),
        Some(2),
        "lfcrlf must have fired once per mapped send: {node}"
    );
}

#[test]
fn map_raw_edge_defaults_to_held_and_maps_targetward_at_volume() {
    // Regression for the audit's one correctness finding: a map's raw edge that OMITS
    // write_mode must default to `held` (§7.8), not the generic on-demand — otherwise
    // the held-origin targetward pump parks forever. Also the plan §12.1 targetward
    // byte-exactness at volume, cross-checked against an independent oracle + counters.
    let Some(pair) = serial_pair() else {
        eprintln!(
            "SKIP map_raw_edge_defaults_to_held_and_maps_targetward_at_volume: no serial device"
        );
        return;
    };
    let (end_a, end_b) = pair.ports();
    let (end_a, end_b) = (end_a.to_owned(), end_b.to_owned());

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log dir");
    let devlog = logdir.join("dev.log");

    // NOTE: the `usb0 -> console/raw` edge deliberately OMITS write_mode. The fix
    // promotes it to held; without the fix the map never acquires usb0's lock and
    // every send below parks forever (the test would time out).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{end_a}"
[[node]]
type = "map"
name = "console"
targetward = ["lfcrlf"]
[[node]]
type = "serial"
name = "devsink"
device = "{end_b}"
arbitration = "free-for-all"
hostward_buffer = 4096
[[node]]
type = "log"
name = "devlog"
directory = "{logdir}"
filename = "dev.log"
[[edge]]
a = "usb0"
b = "console/raw"
[[edge]]
a = "devsink"
b = "devlog"
write_mode = "never"
"#,
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load default-held map graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("devsink", "active", Duration::from_secs(20)),
        "serial ends not active"
    );

    // The fix in one assertion: an omitted write_mode on the map's raw edge yields a
    // HELD origin that acquires usb0's lock on attach (holder = "console/raw").
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("usb0")
                .and_then(|n| n["lock"]["holder"].as_str().map(str::to_owned))
                == Some("console/raw".to_owned())
        }),
        "an omitted map raw-edge write_mode must default to held (§7.8): {:?}",
        rpc.node("usb0")
    );

    // Drive many targetward chunks through the map — a mix of plain and LF-dense lines
    // so the pump processes multiple chunks and the lfcrlf expansion is exercised at
    // volume. Accumulate the independent oracle in lockstep.
    let mut expected: Vec<u8> = Vec::new();
    let mut lf_count: u64 = 0;
    let mut bytes_in: u64 = 0;
    for i in 0..40u32 {
        // Alternate: a plain line, and an LF-dense line (embedded newlines the map
        // must each expand). `send` appends one trailing '\n'.
        let line = if i % 2 == 0 {
            format!("data-row-{i}")
        } else {
            "a\nb\nc\nd\ne".to_owned()
        };
        rpc.send("console", &line, false, 5000)
            .unwrap_or_else(|e| panic!("send #{i} (would hang without the held default): {e:?}"));
        let sent = format!("{line}\n");
        bytes_in += sent.len() as u64;
        lf_count += sent.bytes().filter(|&b| b == b'\n').count() as u64;
        expected.extend_from_slice(&oracle_targetward(sent.as_bytes()));
    }

    // The device receives every mapped byte, in order, byte-exact against the oracle.
    assert!(
        wait_until(Duration::from_secs(15), || file_bytes(&devlog) == expected),
        "device log did not match the targetward oracle at volume (len got={} want={})",
        file_bytes(&devlog).len(),
        expected.len()
    );

    // Counters: bytes_in = every input byte; bytes_out = the mapped length; lfcrlf
    // fired once per LF. And the raw-side intake drop counter is surfaced and zero
    // (no hostward data flows here, so nothing sheds).
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("console")
                .and_then(|n| n["targetward"]["bytes_out"].as_u64())
                == Some(expected.len() as u64)
        }),
        "targetward bytes_out did not settle: {:?}",
        rpc.node("console")
    );
    let node = rpc.node("console").expect("map node");
    assert_eq!(
        node["targetward"]["bytes_in"].as_u64(),
        Some(bytes_in),
        "targetward bytes_in: {node}"
    );
    assert_eq!(
        node["targetward"]["rules"]["lfcrlf"].as_u64(),
        Some(lf_count),
        "lfcrlf substitution count must match the LF tally: {node}"
    );
    assert_eq!(
        node["raw"]["dropped_slow_consumer"].as_u64(),
        Some(0),
        "raw-side intake drop counter must be surfaced and zero here: {node}"
    );
}

#[test]
fn map_deletion_emits_nothing_for_a_fully_deleted_chunk() {
    // Finding #2 (deletion path): a mapping that deletes every byte of a chunk (ignlf
    // on a lone LF) must emit NOTHING downstream — no device write — while still
    // counting the input (bytes_in advances, the rule fires, bytes_out stays 0), per
    // §7.8 "deletion is intent, not loss". Verified deterministically: a fully-deleted
    // send followed by a surviving send leaves the device with ONLY the survivor's
    // bytes (an errant empty-chunk write would corrupt this exact comparison).
    let Some(pair) = serial_pair() else {
        eprintln!("SKIP map_deletion_emits_nothing_for_a_fully_deleted_chunk: no serial device");
        return;
    };
    let (end_a, end_b) = pair.ports();
    let (end_a, end_b) = (end_a.to_owned(), end_b.to_owned());

    let d = Daemon::start();
    let rpc = d.rpc();
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log dir");
    let devlog = logdir.join("dev.log");

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{end_a}"
[[node]]
type = "map"
name = "console"
targetward = ["ignlf"]
[[node]]
type = "serial"
name = "devsink"
device = "{end_b}"
arbitration = "free-for-all"
hostward_buffer = 4096
[[node]]
type = "log"
name = "devlog"
directory = "{logdir}"
filename = "dev.log"
[[edge]]
a = "usb0"
b = "console/raw"
[[edge]]
a = "devsink"
b = "devlog"
write_mode = "never"
"#,
        logdir = logdir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load ignlf map graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20))
            && rpc.wait_status("devsink", "active", Duration::from_secs(20)),
        "serial ends not active"
    );

    // (1) send an empty line: `send` makes it a lone "\n", which ignlf deletes to an
    // empty chunk — nothing must reach the device.
    rpc.send("console", "", false, 5000).expect("send deleted");
    // (2) send a surviving line: "hi\n" → ignlf drops the \n → "hi" reaches the device.
    rpc.send("console", "hi", false, 5000)
        .expect("send survivor");

    // The device sees ONLY "hi": if the fully-deleted chunk had emitted anything, this
    // exact-equality wait would never be satisfied (it would carry stray bytes).
    assert!(
        wait_until(Duration::from_secs(10), || file_bytes(&devlog) == b"hi"),
        "device must receive only the survivor's bytes; a deleted chunk leaked: {:?}",
        file_bytes(&devlog)
    );

    // Counters: both LFs were seen (bytes_in = 1 + 3), both deleted by ignlf, and only
    // "hi" survived (bytes_out = 2) — the deletion is counted, never a silent drop.
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("console")
                .and_then(|n| n["targetward"]["bytes_out"].as_u64())
                == Some(2)
        }),
        "targetward bytes_out did not settle to 2: {:?}",
        rpc.node("console")
    );
    let node = rpc.node("console").expect("map node");
    assert_eq!(
        node["targetward"]["bytes_in"].as_u64(),
        Some(4),
        "bytes_in must count both inputs (1 + 3): {node}"
    );
    assert_eq!(
        node["targetward"]["rules"]["ignlf"].as_u64(),
        Some(2),
        "ignlf must have deleted both LFs: {node}"
    );
}
