//! Review-32 guards for the two kind-dependent **configuration rules** that decide
//! what `load` accepts (design §6 arbitration, §7.2 pty, §11 load atomicity,
//! §15.26 "a structurally-invalid config never destroys a good running graph").
//!
//! | ID | The defect this pins |
//! |----|----------------------|
//! | CORE-2 | the codec-mux write-mode rule ignored arbitration while its sibling ten lines below exempted `free-for-all`, so a graph that *runs* was refused, citing a lock stall that cannot happen on a lock-less endpoint |
//! | RV-4 | a pty `mode` carried no range check, so `mode = 666` — the octal spelling read as decimal, 0o1232, owner `-w-` — loaded and then faulted the node with `prime open /dev/pts/N: EACCES`, a message that never mentions the mode |
//!
//! Both fixes live in `GraphConfig::validate` (`nexus-core/src/config.rs`), which is
//! also `connect`'s validator and `add-node`'s, so the guards here are deliberately
//! *end-to-end*: the unit tests beside the code pin the predicate, and these pin what
//! an operator actually meets — the refusal code, the offender named in it, and the
//! running graph surviving a refused `--replace`.
//!
//! **Platform.** Everything structural runs everywhere (§5 RULE 2). The one
//! behavioural test — CORE-2's positive half, which proves the newly-accepted graph
//! really moves bytes rather than parking on a lock — needs a serial device and
//! self-skips where a pty cannot be one (macOS: `serial2` → `ENOTTY`).

use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Rpc, Subscription, serial_echo, wait_until};
use serde_json::{Value, json};

/// `nexus_rpc::AppError::Structural` — every `GraphConfig::validate` failure surfaces
/// with this code (§16.8).
const STRUCTURAL: i64 = -32002;

/// The sorted node names in `state` — the cheap structural fingerprint of a running
/// graph, compared before and after a refused load.
fn node_names(rpc: &Rpc) -> Vec<String> {
    let mut names: Vec<String> = rpc
        .state()
        .get("nodes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|n| n.get("name").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// ===========================================================================
// CORE-2 — a `free-for-all` host endpoint exempts the multiplexed-edge rule
// ===========================================================================

/// The reported graph: a serial node whose endpoint has **no lock**, feeding a demux
/// codec's multiplexed endpoint with `write_mode` omitted (the generic `on-demand`
/// default). Refused before the fix; the refusal named a park-forever stall that
/// `EndpointLock::may_write` cannot produce under `free-for-all` (§6).
fn free_for_all_mux_graph(device: &str, arbitration: &str) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
arbitration = "{arbitration}"

[[node]]
type = "codec"
name = "mux"
codec = "reference"
channels = ["c0"]

[[edge]]
a = "usb0"
b = "mux"
"#
    )
}

#[test]
fn a_free_for_all_endpoint_accepts_an_on_demand_multiplexed_edge() {
    let d = Daemon::start();
    let rpc = d.rpc();
    // No device needed: the serial node comes up `waiting` with an absent path and the
    // whole assertion is structural (§15.8), so this half runs on every platform.
    let device = d.run().join("absent-device");
    let device = device.to_string_lossy().into_owned();

    let r = rpc
        .load_toml(&free_for_all_mux_graph(&device, "free-for-all"), false)
        .expect("a lock-less endpoint must accept an on-demand multiplexed edge");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(2),
        "the free-for-all graph did not load both nodes: {r}"
    );
    assert_eq!(node_names(rpc), ["mux", "usb0"]);

    // The exemption is about *arbitration*, nothing else: the identical graph under
    // the exclusive default is still refused, and the refusal still names both ends.
    rpc.teardown();
    let err = rpc
        .load_toml(&free_for_all_mux_graph(&device, "exclusive"), true)
        .expect_err("an arbitrated endpoint must still refuse an on-demand mux edge");
    assert_eq!(
        err.code, STRUCTURAL,
        "the mux-edge rule must stay a structural refusal: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("usb0") && err.message.contains("mux"),
        "the refusal must name both ends of the offending edge: {}",
        err.message
    );
}

/// CORE-2's positive half, in bytes: the configuration that used to be refused is not
/// merely *accepted*, it runs. The codec's multiplexed origin is `on-demand` on a
/// lock-less endpoint, so it must write targetward without ever being granted a lock
/// — the exact claim the old refusal denied.
///
/// The device is a `nexus-sim pty --echo` double, so a frame the codec writes comes
/// straight back: `send` on `mux/c0` → codec frames it → device echoes → codec
/// demuxes → `mux/c0`'s hostward mirror, where a tap reads it. Round-tripping the
/// payload proves both directions of the multiplexed edge in one assertion.
#[test]
fn a_free_for_all_multiplexed_origin_really_writes_targetward() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_free_for_all_multiplexed_origin_really_writes_targetward: \
             no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();

    let cfg = free_for_all_mux_graph(&echo.device().to_string_lossy(), "free-for-all");
    rpc.load_toml(&cfg, false).expect("load the echo graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "the serial node never opened the echo device: {}",
        rpc.state()
    );
    assert!(
        wait_until(Duration::from_secs(10), || rpc.node_status("mux")
            == "active"),
        "the codec node never came up: {}",
        rpc.state()
    );

    // Tap the channel endpoint *before* sending: the tap mirror is outside the graph
    // (invariant 9), so it sees the demuxed bytes even though no consumer is wired to
    // `mux/c0`.
    let mut tap = rpc.stream("tap.open", json!({ "endpoint": "mux/c0" }));

    // `send` self-acquires `mux/c0`'s lock (invariant 8) and appends a newline, so the
    // payload that crosses the wire is exactly this line plus `\n`.
    let ack = rpc
        .send("mux/c0", "core-2-round-trip", false, 5_000)
        .expect("send into the codec channel");
    assert_eq!(
        ack.get("delivered").and_then(Value::as_bool),
        Some(true),
        "send did not report delivery: {ack}"
    );
    let want = b"core-2-round-trip\n";
    let got = collect_tap(&mut tap, want.len(), Duration::from_secs(15));
    assert_eq!(
        got,
        want,
        "the on-demand multiplexed origin did not reach the device and return: got {:?}",
        String::from_utf8_lossy(&got)
    );
}

// ===========================================================================
// RV-4 — a pty `mode` is range-checked structurally
// ===========================================================================

/// A one-pty graph with the given `mode` line spliced in (empty for "omitted").
fn pty_graph(path: &str, mode_line: &str) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "con"
path = "{path}"
{mode_line}
"#
    )
}

#[test]
fn a_pty_mode_outside_its_range_is_refused_structurally_naming_the_field() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let tty = d.run().join("ttyC");
    let tty = tty.to_string_lossy().into_owned();

    // The reported typo: TOML rejects leading-zero integers, so `0o600` written as
    // `666` is decimal 666 = 0o1232 — owner `-w-`. It used to load, and the node then
    // faulted with an EACCES that never mentioned the mode.
    let err = rpc
        .load_toml(&pty_graph(&tty, "mode = 666"), false)
        .expect_err("an out-of-range pty mode must be a structural load error");
    assert_eq!(
        err.code, STRUCTURAL,
        "the pty mode was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("con") && err.message.contains("mode"),
        "the refusal must name the node and the field: {}",
        err.message
    );
    // §11: the whole file is judged before anything is created — including the pts and
    // its symlink, which a faulted-after-load node would already have left behind.
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );
    assert!(
        !std::path::Path::new(&tty).exists(),
        "a refused config allocated a pty anyway: {tty}"
    );

    // The other half of the window, which a bare `<= 0o777` cap would miss: 154 is
    // 0o232 — no owner read — and `prime_slave` opens the slave O_RDWR.
    let err = rpc
        .load_toml(&pty_graph(&tty, "mode = 154"), false)
        .expect_err("a mode denying the daemon owner read+write must be refused");
    assert_eq!(err.code, STRUCTURAL, "[{}] {}", err.code, err.message);

    // And the rule is a *bound*, not a blanket refusal: the octal spelling of the same
    // intent loads and the node comes up.
    rpc.load_toml(&pty_graph(&tty, "mode = 0o600"), false)
        .expect("a legal octal mode must load");
    assert!(
        rpc.wait_status("con", "active", Duration::from_secs(5)),
        "the pty node did not come up: {}",
        rpc.state()
    );
}

#[test]
fn a_refused_pty_mode_never_destroys_the_running_graph() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let tty = d.run().join("ttyGood");
    let tty = tty.to_string_lossy().into_owned();
    let bad = d.run().join("ttyBad");
    let bad = bad.to_string_lossy().into_owned();

    rpc.load_toml(&pty_graph(&tty, "mode = 0o660"), false)
        .expect("load the good graph");
    let before = node_names(rpc);
    assert_eq!(before, ["con"]);

    // `load --replace` composes teardown-then-load, so a numeric field validated late
    // would have torn the good graph down first (§11 atomicity, §15.26, invariant 13).
    let err = rpc
        .load_toml(&pty_graph(&bad, "mode = 666"), true)
        .expect_err("--replace with an out-of-range mode must be refused");
    assert_eq!(err.code, STRUCTURAL, "[{}] {}", err.code, err.message);
    assert_eq!(
        node_names(rpc),
        before,
        "a refused --replace changed the running graph"
    );
    assert!(
        rpc.wait_status("con", "active", Duration::from_secs(5)),
        "the surviving node is not active: {}",
        rpc.state()
    );
}

// ---------------------------------------------------------------------------
// Local helpers (`nexus-itest`'s shared lib is off-limits to this file, so the
// tap decoding lives here rather than in `src/lib.rs`).
// ---------------------------------------------------------------------------

/// Standard base64 decode — the inverse of the daemon's `nexus_rpc::base64_encode`
/// used for `tap.data`. Each payload is its own padded string, so decoding
/// per-message and concatenating is exact.
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

/// Drain `tap.data` notifications until `want` bytes are collected or `timeout`
/// elapses. Bounded — no bare sleeps, no unbounded wait (§5).
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
