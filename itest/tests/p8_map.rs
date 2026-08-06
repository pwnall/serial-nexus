//! Console map track (plan §12.1; design §7.8, §15.33): the per-console character
//! map node, driven end-to-end through the daemon.
//!
//! Three properties, each pinned against an **independent in-test oracle** (never
//! the daemon's own `serial_nexus_core::map`, so the test is a genuine cross-check, not a
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
//!
//! The audit (`docs/historical/26-claude-opus-code-review.md`) added three more, all
//! device-free and therefore running on **every** platform. They source their
//! hostward bytes from a `leg` channel driven by a `serial-nexus-sim wire` peer rather than
//! from a UART — the map neither knows nor cares which host-facing endpoint feeds
//! its raw side:
//!
//! 4. **`spchex` is picocom's control class, end to end** (MAP-1) — the rule shipped
//!    as `b == 0x20` (SPACE) through v11, so the one rule an operator reaches for to
//!    reveal stray control bytes rewrote every space instead, and `0x00..=0x1f`/`0x7f`
//!    were unreachable by *any* rule in the vocabulary.
//! 5. **The map counts consumer absence** (DM-3) — mapped bytes that reach no graph
//!    consumer are counted even though a tap and the default ring saw every one of
//!    them: a ring is a spy outside the graph and may never suppress a loss count
//!    (§5, AGENTS.md invariant 9).
//! 6. **A read-only map is inert, not destructive** (MAP-1 runtime) — the reviewer's
//!    controlled A/B, one attribute apart: with `write_mode = "never"` on the raw
//!    edge the map used to drop its mapped endpoint's targetward receiver while its
//!    senders stayed live, which killed a writing PTY's reader task outright and
//!    froze `client_present` at `true`.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Sim, Subscription, seeded_bytes, serial_echo, serial_pair_or_rig, sha256_hex,
    skip_no_pair, wait_until,
};

// ---- Independent oracles (reimplemented here, never serial_nexus_core::map) --------------

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

/// picocom's `M_SPCHEX` class, **enumerated** rather than restated as a predicate:
/// DEL plus every C0 control except TAB/LF/CR, which have rules of their own. Written
/// this way on purpose — an oracle that repeated `serial_nexus_core::map`'s range test would
/// agree with a wrong implementation, which is exactly how the SPACE-for-control
/// defect survived a 256-byte sweep (MAP-1).
fn is_picocom_special(b: u8) -> bool {
    matches!(b, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f)
}

/// The hostward oracle for `["spchex"]`: every special byte → `[xx]`, everything
/// else — SPACE emphatically included — verbatim.
fn oracle_spchex(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in input {
        if is_picocom_special(b) {
            out.extend_from_slice(&hex4(b));
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

/// Standard base64 decode of a `tap.data` payload — `serial_nexus_rpc`'s tested
/// decoder rather than a seventh hand-rolled copy (§16.5 one-rule-one-place, review
/// 37 37-TEST-5). Panics on a malformed payload: these bytes were produced by
/// `serial_nexus_rpc::base64_encode` on the other side of the socket, so anything
/// else is a defect, not an input.
fn base64_decode(s: &str) -> Vec<u8> {
    serial_nexus_rpc::base64_decode(s).expect("a tap.data payload must be valid base64")
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
        .join("packaging/serial-nexus-daemon.example.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let cfg: serial_nexus_core::GraphConfig =
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
        serial_nexus_core::config::NodeConfig::Map {
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

    // DM-3 (§5, AGENTS.md invariant 9): nothing in the *graph* consumes the mapped
    // endpoint here — the two taps are spies outside it — so every mapped byte is
    // consumer-absent loss and must be counted as such, even though the tap above
    // received all of it byte-exactly a few assertions ago. The reviewer's
    // reproduction 20 found `bytes_in: 35`, `bytes_out: 40` and no such counter at
    // all on the node.
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.node("console")
                .and_then(|n| n["hostward"]["discarded_unattached"].as_u64())
                == Some(mapped.len() as u64)
        }),
        "mapped bytes that reached no graph consumer must be counted (DM-3), and the \
         tap/ring mirror must not suppress the count: {:?}",
        rpc.node("console").map(|n| n["hostward"].clone())
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
    let Some(pair) = serial_pair_or_rig() else {
        skip_no_pair("map_steal_to_bypass_speaks_mapped_then_raw");
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
    let Some(pair) = serial_pair_or_rig() else {
        skip_no_pair("map_raw_edge_defaults_to_held_and_maps_targetward_at_volume");
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
    let Some(pair) = serial_pair_or_rig() else {
        skip_no_pair("map_deletion_emits_nothing_for_a_fully_deleted_chunk");
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

// ---- 4/5: spchex over the control range, and the map's unattached accounting ----

#[test]
fn spchex_hexes_the_control_class_never_space_and_counts_unattached_loss() {
    // MAP-1 + DM-3, end to end through a live map node, with **no serial device**:
    // the hostward bytes come from a `leg` channel a `serial-nexus-sim wire` peer drives,
    // so this runs on every platform (a map does not care which host-facing
    // endpoint feeds its raw side).
    //
    // The payload is the sim's seeded stream — uniform over 0x00..=0xff, so one 4 KiB
    // batch carries hundreds of C0 controls, several DELs and a few dozen spaces:
    // the exact byte classes the defect confused. The expectation is computed by
    // `oracle_spchex`, which enumerates picocom's class instead of restating the
    // implementation's predicate.
    const N: usize = 4096;
    const SEED: u64 = 5;
    let seeded = seeded_bytes(SEED, N);
    let expected = oracle_spchex(&seeded);

    // The defect's two halves, stated as preconditions so the test can never pass
    // vacuously: the batch must actually contain the bytes an operator is hunting,
    // and it must contain spaces for the rule to wrongly rewrite.
    let specials = seeded.iter().filter(|&&b| is_picocom_special(b)).count();
    let spaces = seeded.iter().filter(|&&b| b == 0x20).count();
    assert!(
        specials > 0 && spaces > 0,
        "degenerate batch: the seeded source must carry both control bytes and spaces"
    );
    assert!(
        seeded.contains(&0x00) && seeded.contains(&0x1b),
        "the batch must carry the 0x00/0x1b an operator reaches for spchex to find"
    );

    let d = Daemon::start();
    let rpc = d.rpc();
    let leg = d.run().join("leg.sock");

    // downlink/c0 (leg, host-facing) --held--> console/raw (map) --> [no consumer].
    // The raw edge omits write_mode deliberately (promoted to `held`, §7.8/§3.17).
    let cfg = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
arbitration = "free-for-all"
channels = ["c0"]
[[node]]
type = "map"
name = "console"
hostward = ["spchex"]
[[edge]]
a = "downlink/c0"
b = "console/raw"
"#,
        leg = leg.display(),
    );
    rpc.load_toml(&cfg, false).expect("load spchex map graph");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "map node not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(10), || leg.exists()),
        "the leg's listen socket never appeared"
    );

    // Tap the mapped endpoint before the peer speaks, so no mapped byte predates it.
    let mut tap = rpc.stream("tap.open", json!({ "endpoint": "console" }));
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.state()["taps"].as_array().map(Vec::len) == Some(1)
        }),
        "the tap did not register: {:?}",
        rpc.state()["taps"]
    );

    let leg_str = leg.to_string_lossy().into_owned();
    let _peer = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &leg_str,
            "--announce",
            "c0",
            "--send",
            &format!("c0={N}"),
            "--seed",
            &SEED.to_string(),
            "--hold-ms",
            "8000",
            "--timeout-ms",
            "20000",
        ],
        None,
    );

    let got = collect_tap(&mut tap, expected.len(), Duration::from_secs(20));

    // The whole batch, byte-for-byte against the independent oracle.
    assert_eq!(
        got.len(),
        expected.len(),
        "mapped tap delivered {} bytes, expected {}",
        got.len(),
        expected.len()
    );
    assert_eq!(
        sha256_hex(&got),
        sha256_hex(&expected),
        "the spchex stream did not match picocom's control-byte class"
    );

    // The two halves of the defect, each pinned on its own so a failure names which
    // one regressed rather than just "the checksum differs".
    assert_eq!(
        got.iter().filter(|&&b| b == 0x20).count(),
        spaces,
        "spchex rewrote SPACE — hexing a space is nrmhex's job (MAP-1)"
    );
    for probe in [&b"[00]"[..], &b"[1b]"[..]] {
        assert!(
            got.windows(4).any(|w| w == probe),
            "no rule rendered {:?}: the hex family must reach the control range (§7.8)",
            String::from_utf8_lossy(probe)
        );
    }

    // Per-rule counters cross-check the oracle's tally, and the byte totals show the
    // 4× expansion the class actually took.
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.node("console")
                .and_then(|n| n["hostward"]["bytes_in"].as_u64())
                == Some(N as u64)
        }),
        "hostward counters did not settle: {:?}",
        rpc.node("console")
    );
    let node = rpc.node("console").expect("map node in state");
    assert_eq!(
        node["hostward"]["rules"]["spchex"].as_u64(),
        Some(specials as u64),
        "spchex substitution count must match the oracle's control-class tally: {node}"
    );
    assert_eq!(
        node["hostward"]["bytes_out"].as_u64(),
        Some(expected.len() as u64),
        "hostward bytes_out must equal the mapped length: {node}"
    );

    // DM-3, on a graph with no consumer at all on the mapped side (reproduction 20):
    // every mapped byte is consumer-absent loss and is counted, while the tap above
    // received every one of them — the ring/tap mirror is a spy outside the graph and
    // never suppresses the count (§5, AGENTS.md invariant 9).
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.node("console")
                .and_then(|n| n["hostward"]["discarded_unattached"].as_u64())
                == Some(expected.len() as u64)
        }),
        "the map must count mapped bytes that reached no graph consumer (DM-3): {:?}",
        rpc.node("console").map(|n| n["hostward"].clone())
    );
}

// ---- 6: the read-only map is inert, not destructive (the controlled A/B) --------

/// Wait for the map's targetward accounting to settle, **while the pts client that
/// produced the bytes is still open** (notes §3.29).
///
/// The rule, the borrow mechanism, the withdrawn `tcflush(TCOFLUSH)` referee and the
/// full "precisely what is and is not stronger" argument now live on
/// [`serial_nexus_itest::settled_while_open`], because six further guards in this
/// suite needed the same enforcement and a rule that lives in one test file is a rule
/// the next test file re-derives (notes §3.56). Read it there before editing either
/// side of this ordering.
///
/// What stays local is the map's own predicate: the `never` arm counts
/// `discarded_no_raw_edge` (the read-only map swallows and names the bytes) and the
/// `held` arm counts `bytes_in` (a writable map transforms them). The two arms are the
/// controlled A/B, so the counter is chosen by the arm rather than by a flag inside
/// the daemon.
fn settled_while_open(
    rpc: &serial_nexus_itest::Rpc,
    client: &mut serial_nexus_itest::SlaveWitness,
    write_mode: &str,
    typed: u64,
) -> bool {
    serial_nexus_itest::settled_while_open(
        &mut [client],
        &format!("[{write_mode}] the map's targetward accounting"),
        Duration::from_secs(10),
        || {
            let n = rpc.node("console").unwrap_or(Value::Null);
            match write_mode {
                "never" => n["targetward"]["discarded_no_raw_edge"].as_u64() == Some(typed),
                _ => n["targetward"]["bytes_in"].as_u64() == Some(typed),
            }
        },
    )
}

#[test]
fn a_read_only_map_leaves_its_writers_pty_alive() {
    // MAP-1 (runtime), as the reviewer's reproduction 21 isolated it: one graph, one
    // attribute changed. `write_mode = "never"` on the raw edge is the documented
    // read-only/display map (§7.8). The map used to drop the mapped endpoint's
    // targetward receiver in that arm while its senders stayed live, so the first
    // byte a PTY writer sent hit a closed channel and ended `read_and_poll` — taking
    // presence latching, last-close handling, termios reconciliation and
    // detach-release with it. `client_present` then froze at `true` forever.
    //
    // Both arms run, so the A/B is in the suite rather than in a review appendix.
    // No serial device: `usb0`'s device is absent (`waiting`), which is enough for
    // the lock, the origin and the targetward receiver that make this test go.
    const TYPED: u64 = 64;
    for write_mode in ["never", "held"] {
        let d = Daemon::start();
        let rpc = d.rpc();
        let p0 = d.run().join("p0");
        let cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "map"
name = "console"
targetward = ["lfcrlf"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "usb0"
b = "console/raw"
write_mode = "{write_mode}"
[[edge]]
a = "console"
b = "p0"
"#,
            dev = d.run().join("absent-device").display(),
            p0 = p0.display(),
        );
        rpc.load_toml(&cfg, false)
            .unwrap_or_else(|e| panic!("[{write_mode}] load: {e:?}"));
        assert!(
            rpc.wait_status("p0", "active", Duration::from_secs(10)),
            "[{write_mode}] p0 not active: {:?}",
            rpc.node("p0")
        );
        assert!(
            wait_until(Duration::from_secs(5), || p0.exists()),
            "[{write_mode}] p0 symlink never appeared"
        );

        // The PTY takes the mapped endpoint's write lock, then a client attaches and
        // is *observed* before it types — which is what makes the frozen-presence
        // failure the reviewer saw the one this reproduces (rather than a reader that
        // died before ever latching). The client is driven from this process so the
        // ordering is ours, not a subprocess's.
        rpc.lock("p0", false, false, None)
            .unwrap_or_else(|e| panic!("[{write_mode}] lock p0: {e:?}"));
        // Opened here rather than through `attach_slave` because this fd is the client,
        // not only the witness — the two are the same handle on purpose (a second fd on
        // the same slave would change what `client_present` and the last close mean).
        // `adopt_slave` records the path so the witness can prove the *far end* is still
        // there and not merely that the descriptor is valid (notes §3.60).
        let mut client = serial_nexus_itest::adopt_slave(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&p0)
                .unwrap_or_else(|e| panic!("[{write_mode}] open pty slave: {e}")),
            &p0,
        );
        assert!(
            wait_until(Duration::from_secs(10), || {
                rpc.node("p0")
                    .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                    == Some(true)
            }),
            "[{write_mode}] the client never became present"
        );
        client
            .write_all(&vec![b'x'; TYPED as usize])
            .expect("type at the console");
        client.flush().expect("flush");

        // Where the typed bytes landed is observed *while the client is still open* —
        // and the borrow in `settled_while_open`'s signature is what enforces that,
        // not this comment. Only the observation is taken here; the assertion is
        // deferred below so the more specific lifecycle failures report first.
        //
        // That ordering is presentational, and deliberately not sold as more. It does
        // NOT guarantee "a MAP-1 regression fails with MAP-1's message": measured, a
        // revert of the map half alone (drop the targetward `rx` instead of spawning
        // the pump) leaves `pty.rs`'s reader alive — `Handoff::Lost` breaks rather
        // than returning, by design — so both lifecycle assertions pass and the
        // accounting one below is the *only* detector, in this order or the old one.
        // Only the two-part revert (map half + `Handoff::Lost => return`) produces
        // MAP-1's message, and it did so before this change too.
        let settled = settled_while_open(rpc, &mut client, write_mode, TYPED);

        // The property: the client exits, and the PTY's reader must still be alive to
        // notice. With the dropped receiver, the first typed byte hit a closed channel
        // and ended `read_and_poll`, so this stayed `true` indefinitely.
        drop(client);
        assert!(
            wait_until(Duration::from_secs(10), || {
                rpc.node("p0")
                    .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                    == Some(false)
            }),
            "[{write_mode}] client_present never went false after the client exited — \
             the PTY's reader task died with its targetward channel (MAP-1): {:?}",
            rpc.node("p0")
        );
        // And the detach released the on-demand holder's lock, which is the same
        // reader task's job (§6) — proof the task survived rather than merely that
        // presence happened to flip.
        assert!(
            wait_until(Duration::from_secs(5), || {
                rpc.node("console")
                    .and_then(|n| n.pointer("/lock/holder").cloned())
                    == Some(Value::Null)
            }),
            "[{write_mode}] the mapped endpoint's lock was not detach-released: {:?}",
            rpc.node("console").map(|n| n["lock"].clone())
        );

        // The typed bytes went somewhere defensible, and *which* somewhere is the
        // whole difference between the two arms (observed above, before the close).
        //
        // Both nodes are dumped, not just the map. The 2026-08-03 macOS failure
        // printed `console` alone, and "every counter on the map is 0" cannot on its
        // own separate *the bytes never reached the daemon* from *the pty read them
        // and accounted them somewhere else* — `p0` is where `discarded_targetward`
        // and `discarded_at_last_close` live, so a dump without it sends the next
        // reader to re-derive by hand what the assertion could simply have said.
        let node = rpc.node("console").expect("map node in state");
        assert!(
            settled,
            "[{write_mode}] targetward accounting did not settle: console={node} \
             p0={:?}",
            rpc.node("p0")
        );
        if write_mode == "never" {
            // Inert: the bytes are swallowed and counted (§5), and the transform is
            // deliberately not run, so no rule claims a substitution it never made.
            assert_eq!(
                node["targetward"]["bytes_in"].as_u64(),
                Some(0),
                "a read-only map must not claim to have transformed anything: {node}"
            );
        } else {
            assert_eq!(
                node["targetward"]["discarded_no_raw_edge"].as_u64(),
                Some(0),
                "a writable map must discard nothing: {node}"
            );
        }
    }
}

// ---- 7: the residual-forward promise, kept where it is observable ---------------

/// A closing writer's residual is **forwarded, not purged** — `nodes/pty.rs`'s
/// stated promise: *"Drain available data for a writer that may write, regardless of
/// a simultaneous POLLHUP: a closing writer's residual must still be forwarded (not
/// purged) before the close is finalized."*
///
/// **Why this test exists, and why it is the one place in the suite that closes
/// first.** Its sibling above deliberately observes the counter *before* the close
/// (notes §3.29) — which, as a side effect, guarantees the daemon drained the master
/// in an earlier poll pass, so the close there is always observed in a data-free
/// pass and this promise is never exercised. Adversarial verification of that change
/// found the gap by planting the natural regression of the promise:
///
/// ```text
/// - if pending.is_none() && may_write && re.contains(PollFlags::POLLIN) {
/// + if pending.is_none() && may_write && now && re.contains(PollFlags::POLLIN) {
/// ```
///
/// a plausible "consistency" edit, since the sibling `TIOCPKT_IOCTL` branch really is
/// gated on `now`. With it, the residual is purged at detach instead of forwarded and
/// the dump shows `purged: 64` where `discarded_no_raw_edge: 64` belongs. The
/// reordered sibling passes that regression; this test fails it. Coverage restored
/// rather than merely mourned in a note.
///
/// **Why it is Linux-gated, which is the interesting part.** Closing first is exactly
/// the shape §3.29 forbids in general, and the reason is that it silently assumes the
/// kernel keeps unread bytes across the slave's last close. That assumption is not
/// portable — and it is now *measured* rather than assumed: doctor **P13** reports
/// this kernel's policy directly (`retains`, `close(2)` in ~7 µs, 64/64 recovered on
/// Linux 7.0.0-29). So the gate here is not "macOS is awkward"; it is "this guard is
/// only meaningful where P13 says `retains`, and Linux is where that is measured".
/// Run the doctor before extending it to another platform, and read P13 first.
#[cfg(target_os = "linux")]
#[test]
fn a_closing_writers_residual_is_forwarded_not_purged() {
    const TYPED: u64 = 64;
    let d = Daemon::start();
    let rpc = d.rpc();
    let p0 = d.run().join("p0");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "map"
name = "console"
targetward = ["lfcrlf"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "usb0"
b = "console/raw"
write_mode = "never"
[[edge]]
a = "console"
b = "p0"
"#,
        dev = d.run().join("absent-device").display(),
        p0 = p0.display(),
    );
    rpc.load_toml(&cfg, false).expect("load");
    assert!(
        rpc.wait_status("p0", "active", Duration::from_secs(10)),
        "p0 not active: {:?}",
        rpc.node("p0")
    );
    assert!(
        wait_until(Duration::from_secs(5), || p0.exists()),
        "p0 symlink never appeared"
    );
    rpc.lock("p0", false, false, None).expect("lock p0");

    let mut client = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(&p0)
        .unwrap_or_else(|e| panic!("open pty slave: {e}"));
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.node("p0")
                .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                == Some(true)
        }),
        "the client never became present"
    );

    // Type and close with nothing in between, so the daemon's next poll finds the
    // data and the hangup together — the pass the promise is about. On a kernel that
    // retains (P13), those bytes are still there for that pass to drain.
    client
        .write_all(&vec![b'x'; TYPED as usize])
        .expect("type at the console");
    client.flush().expect("flush");
    drop(client);

    // Forwarded: the map counts them as its own consumer-absent loss. Purged: they
    // are charged to the origin's `purged` tally instead, and the map counts nothing
    // — which is the regression's signature, so both halves are asserted.
    let settled = wait_until(Duration::from_secs(10), || {
        rpc.node("console")
            .and_then(|n| n["targetward"]["discarded_no_raw_edge"].as_u64())
            == Some(TYPED)
    });
    let node = rpc.node("console").expect("map node in state");
    assert!(
        settled,
        "the closing writer's residual was not forwarded to the map — pty.rs drained \
         it into the detach purge instead of forwarding it (look for `purged: {TYPED}` \
         below): console={node} p0={:?}",
        rpc.node("p0")
    );
    assert_eq!(
        node.pointer("/lock/origins/0/purged")
            .and_then(Value::as_u64),
        Some(0),
        "the residual must be forwarded, never purged-at-detach: {node}"
    );
}
