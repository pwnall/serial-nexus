//! Codec and exec-codec **signals**: what a demux node tells an operator when its
//! transform, or the wire, does something the configuration did not anticipate
//! (`docs/32-claude-opus-code-review.md` — WIRE-1, CODEC-1, CORE-3/EXEC-2, the
//! codec/exec half of LEGD-2; design §5 "loss is always visible and attributable",
//! §7.5/§7.6, §11 structural validation).
//!
//! The `p12_*` family is this review's regression home, as `p9_*` is review 26's.
//!
//! Three properties, each a defect that shipped:
//!
//! 1. **An unconfigured channel identity is counted *and named*** (CODEC-1). Bytes
//!    decoded onto an identity the node has no channel for are still dropped — §8's
//!    rule that an announcement never grows the graph, which `docs/codec-authors.md`
//!    states outright — but they used to vanish with no counter, no state field and
//!    not even a log line. The symptom was never the invisible part: a mis-spelled
//!    configured channel sits at `waiting` / `delivered_hostward: 0` and always did.
//!    The *diagnosis* was, because nothing anywhere named the identity actually on
//!    the wire, which is the only thing that separates a typo from a device
//!    multiplexing a stream the operator never enumerated.
//! 2. **An out-of-range `restart_backoff_ms` is refused structurally** (CORE-3),
//!    before anything is created and before a `--replace` teardown — the §11
//!    atomicity guarantee, which comes for free because `exec::parse_attributes` is
//!    pure and runs first. It was the one millisecond timer in the schema with no
//!    cap, so `86400000` ("a day", three slipped digits) loaded clean and then never
//!    respawned a crashed child again, with the node reporting `retrying` throughout.
//! 3. **The codec node reports demux health at all** (WIRE-1): `demux_errors`,
//!    `last_demux_error` and `multiplexed.discarded_hostward` exist in `state`, so a
//!    §7.5 "never resyncs" transform refusing the stream is visible rather than a
//!    `tracing::warn!` nobody is reading.
//!
//! **Why WIRE-1's *behaviour* is pinned in a unit test and not here.** Reaching the
//! `Codec::demux` → `Err` arm needs a codec that returns `Err`, and the shipped
//! registry has exactly one in-process codec — `reference` — which resyncs by length
//! guidance and returns `Ok` on every path. The arm is reachable only through the
//! §15.26 out-of-tree surface, i.e. a custom daemon, which is not a thing this
//! harness boots. The counters/fault behaviour is therefore pinned against a stub
//! transform in `daemon/src/nodes/codec.rs`'s `signal_tests`; what is pinned
//! *here* is the part an operator and the RPC docs depend on — that the fields exist
//! on the wire with the documented names. The same boundary applies to LEGD-2's
//! arithmetic and to `discarded_hostward`'s: both need a fan-out or a transform this
//! harness cannot construct through the shipped registry, so both are pinned in unit
//! tests (`runtime::tests` for the shared credit rule, `codec::signal_tests` for the
//! emit-then-refuse charge) and only their *names* are pinned here.
//!
//! Device-free throughout, so it runs on every platform; the exec case self-skips
//! without `python3`, exactly as `p5_exec_crash` does.

use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, wait_until};

/// Whether `python3` is invocable — the exec child is a Python script. Absent ⇒ skip
/// (an environmental prerequisite, like a missing serial device). Local to this file
/// on purpose: `itest/src/lib.rs` is shared and must not grow per-test helpers.
fn have_python3() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// A one-shot exec child that announces and writes on the channel identity `gps` —
/// which the node under test is deliberately *not* configured for — and then parks
/// on stdin so it never exits and never triggers a restart (which would double the
/// counters and make the assertion timing-dependent).
///
/// It speaks the shared envelope by hand (`serial-nexus-codec-api`'s v1 framing: a big-endian
/// u32 body length, one type byte, a big-endian u16 channel length, the channel,
/// then the payload) so the test needs no dependency beyond the stdlib.
const GHOST_CHILD: &str = r#"import struct, sys

def encode(channel, type_byte, payload):
    chan = channel.encode("utf-8")
    body = bytes([type_byte]) + struct.pack(">H", len(chan)) + chan + payload
    return struct.pack(">I", len(body)) + body

out = sys.stdout.buffer
out.write(encode("gps", 1, b""))             # open on an identity the node lacks
out.write(encode("gps", 0, b"$GPGGA,1234"))  # 11 bytes of data on it
out.write(encode("gps", 0, b"$"))            # + 1, so the total is a round 12
out.flush()
sys.stdin.buffer.read()                      # park until the daemon closes stdin
"#;

/// A node object's field from `state` (the daemon flattens `state_extra` onto the
/// node), or `Value::Null` when the node or the field is absent.
fn field(d: &Daemon, node: &str, key: &str) -> Value {
    d.rpc()
        .node(node)
        .and_then(|n| n.get(key).cloned())
        .unwrap_or(Value::Null)
}

/// CODEC-1: an exec child emitting on an identity the node has no channel for is
/// counted in `discarded_unconfigured_channel` and named in `unconfigured_channels`.
///
/// The multiplexed side has no upstream here — the node is `waiting` — which is
/// deliberate: it isolates the child's hostward output as the only thing moving, and
/// it also exercises the case a serial device would otherwise be needed for.
#[test]
fn an_unconfigured_channel_identity_is_counted_and_named() {
    if !have_python3() {
        eprintln!("SKIP an_unconfigured_channel_identity_is_counted_and_named: python3 not found");
        return;
    }
    let d = Daemon::start();
    let rpc = d.rpc();

    let child = d.run().join("ghost-child.py");
    std::fs::write(&child, GHOST_CHILD).expect("write the exec child");

    let cfg = format!(
        r#"
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["c0"]
[node.attributes]
argv = ["python3", "{child}"]
"#,
        child = child.display(),
    );
    rpc.load_toml(&cfg, false).expect("load the exec graph");

    // The child writes as soon as it starts; poll rather than sleep (§5).
    let counted = wait_until(Duration::from_secs(10), || {
        field(&d, "mux", "discarded_unconfigured_channel").as_u64() == Some(12)
    });
    assert!(
        counted,
        "the bytes on an unconfigured identity must be counted where they are lost \
         (§5); state said {:?}",
        field(&d, "mux", "discarded_unconfigured_channel")
    );
    assert_eq!(
        field(&d, "mux", "unconfigured_channels"),
        serde_json::json!(["gps"]),
        "the identity actually on the wire is named — the diagnosis CODEC-1 was about"
    );
    assert_eq!(
        field(&d, "mux", "unconfigured_overflow"),
        serde_json::json!(0),
        "one identity is far below the bound"
    );
    // The configured channel is untouched by its neighbour's noise, and still shows
    // §8's configured-but-unannounced signal.
    let c0 = d
        .rpc()
        .node("mux")
        .and_then(|n| n.get("channels").and_then(|c| c.get("c0")).cloned())
        .expect("the configured channel is reported");
    assert_eq!(
        c0.get("delivered_hostward").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(c0.get("status").and_then(Value::as_str), Some("waiting"));
}

/// CORE-3 (= EXEC-2): an over-range `restart_backoff_ms` is refused structurally,
/// naming the field — and, because `exec::parse_attributes` is pure and runs before
/// anything is created, a `load --replace` carrying it does **not** tear down the
/// running graph first (§11 atomicity, §15.26). Both intake paths are covered:
/// `load` and `add-node`.
///
/// Device-free and python-free: nothing is ever spawned, which is the point.
#[test]
fn an_over_range_restart_backoff_is_refused_before_anything_is_created() {
    let d = Daemon::start();
    let rpc = d.rpc();

    // A good graph first, so the `--replace` attempt below has something to destroy.
    let good = format!(
        r#"
[[node]]
type = "pty"
name = "keep"
path = "{p}"
"#,
        p = d.run().join("keep").display(),
    );
    rpc.load_toml(&good, false)
        .expect("load the graph to preserve");
    assert!(rpc.wait_status("keep", "active", Duration::from_secs(5)));

    // "a day" — three slipped digits, the exact shape the leg's `reconnect_initial_ms`
    // cap was written against.
    let bad = format!(
        r#"
{good}
[[node]]
type = "codec"
name = "mux"
codec = "exec"
faces = "target"
channels = ["c0"]
[node.attributes]
argv = ["/bin/true"]
restart_backoff_ms = 86400000
"#
    );
    let err = rpc
        .load_toml(&bad, true)
        .expect_err("an out-of-range timer is structural");
    assert_eq!(err.code, -32002, "structural error code (§16.8): {err:?}");
    assert!(
        err.message.contains("restart_backoff_ms"),
        "the refusal names the offending field: {}",
        err.message
    );
    assert!(
        err.message.contains("86400000"),
        "and the offending value: {}",
        err.message
    );

    // Nothing was torn down and nothing was created: the §11 guarantee `--replace`
    // composes teardown-then-load into, which only holds because the check is
    // structural and runs first.
    assert_eq!(
        d.rpc().node_status("keep"),
        "active",
        "a refused --replace must not destroy the running graph"
    );
    assert!(
        d.rpc().node("mux").is_none(),
        "the refused node must not exist"
    );

    // The `add-node` intake refuses it identically (it runs the same pure parse).
    let err = rpc
        .add_node_toml(
            r#"
[[node]]
type = "codec"
name = "mux2"
codec = "exec"
faces = "target"
channels = ["c0"]
[node.attributes]
argv = ["/bin/true"]
restart_backoff_ms = 86400000
"#,
        )
        .expect_err("add-node runs the same structural check");
    assert!(
        err.message.contains("restart_backoff_ms"),
        "add-node names the field too: {}",
        err.message
    );
    assert!(d.rpc().node("mux2").is_none());

    // The cap is a cap, not a ban: one hour exactly still loads.
    let ok = format!(
        r#"
{good}
[[node]]
type = "codec"
name = "mux3"
codec = "exec"
faces = "target"
channels = ["c0"]
[node.attributes]
argv = ["/bin/true"]
restart_backoff_ms = 3600000
"#
    );
    rpc.load_toml(&ok, true)
        .expect("the range end is legal, as it is for every other timer");
}

/// WIRE-1's reporting contract: a codec node's `state` carries `demux_errors`,
/// `last_demux_error` and `multiplexed.discarded_hostward`, so a transform refusing
/// the stream has somewhere to be seen. The shipped `reference` codec never refuses
/// anything, so the values are the healthy ones — the assertion is that the fields
/// exist with the documented names and shapes (`docs/rpc/observation.md`), which is
/// what an operator and every `--json` consumer depend on.
#[test]
fn a_codec_node_reports_its_demux_health() {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(
        r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
"#,
        false,
    )
    .expect("load the reference demux");

    let node = rpc.node("mux").expect("the codec node is reported");
    assert_eq!(
        node.get("demux_errors").and_then(Value::as_u64),
        Some(0),
        "the daemon's own count of Codec::demux refusals, distinct from the \
         transform's resync_count (`framing_errors`)"
    );
    assert!(
        node.get("last_demux_error").is_some_and(Value::is_null),
        "present and null while nothing has been refused"
    );
    assert_eq!(
        node.get("multiplexed")
            .and_then(|m| m.get("discarded_hostward"))
            .and_then(Value::as_u64),
        Some(0),
        "multiplexed bytes a refusing demux did not turn into payload (§5) — the \
         *arithmetic* of that counter (a partial decode charges only the tail it \
         refused, not the frames it emitted first) is pinned in `signal_tests`, \
         where a stub transform can produce the emit-then-error shape this one \
         cannot"
    );
    // CODEC-1's fields ride on the in-process codec too, under the same names the
    // exec codec uses — one reporter, so the two kinds cannot drift.
    assert_eq!(
        node.get("discarded_unconfigured_channel")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        node.get("unconfigured_channels"),
        Some(&serde_json::json!([]))
    );
}
