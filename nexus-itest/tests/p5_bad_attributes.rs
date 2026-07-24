//! Phase 5 bad-codec-attributes slice, ported from
//! `scripts/validate/phase5/bad-attributes.sh` (design §8, §11, §15.26).
//!
//! Bad codec configuration is **structural**: a codec whose attribute table the
//! codec itself rejects — or an unknown codec name — fails the `load` with the
//! codec's own error, and because the load is atomic, **nothing is created** (the
//! graph stays empty). A valid `reference` codec (which takes no attributes) still
//! loads, proving the gate is the bad attribute, not codec nodes in general.
//!
//! Needs no serial *device* (pure structural validation of the compiled-in codec
//! registry), so it runs on every platform — the portable replacement for the
//! `jq`/`grep`-on-stderr bash. Assertions pin to the structured RPC error
//! (code + message) and the `state` snapshot, never human CLI text (§5).

use nexus_itest::Daemon;
use serde_json::Value;

/// The `structural` app-error code (`AppError::Structural`, base −32000 − 2). Both a
/// bad attribute table and an unknown codec name surface as this — the RPC-level
/// proof that the rejection is structural, matching the bash's "structural" claim.
const STRUCTURAL: i64 = -32002;

/// The number of nodes in the daemon's current graph — the portable replacement for
/// the bash `jq -e '.nodes==[]'` empty check.
fn node_count(rpc: &nexus_itest::Rpc) -> usize {
    rpc.state()["nodes"].as_array().map(Vec::len).unwrap_or(0)
}

// ---- (1) A bad attribute for the reference codec is rejected; nothing created ----

#[test]
fn bad_codec_attribute_is_rejected_atomically() {
    let d = Daemon::start();
    let rpc = d.rpc();
    assert_eq!(node_count(rpc), 0, "graph not empty at start");

    // The reference codec takes NO attributes; a config bearing one is a structural
    // schema failure the codec's own factory raises (§8/§11).
    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
attributes = { misspelled_option = true }
"#;
    let err = rpc
        .load_toml(cfg, false)
        .expect_err("load with a bad codec attribute should have failed");

    // The rejection must be structural and must carry the codec's OWN error — for the
    // reference codec that message names "reference" (the bash's `grep -qi 'reference'`).
    assert_eq!(
        err.code, STRUCTURAL,
        "bad-attribute rejection was not a structural error: {err:?}"
    );
    assert!(
        err.message.to_lowercase().contains("reference"),
        "rejection did not mention the codec's own error: {:?}",
        err.message
    );

    // Atomic: a rejected load creates nothing (§11).
    assert_eq!(
        node_count(rpc),
        0,
        "a rejected load created nodes (must be atomic, nothing created): {:?}",
        rpc.state()
    );
}

// ---- (2) An unknown codec name is likewise structural; nothing created ----

#[test]
fn unknown_codec_name_is_rejected_atomically() {
    let d = Daemon::start();
    let rpc = d.rpc();
    assert_eq!(node_count(rpc), 0, "graph not empty at start");

    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "does-not-exist"
faces = "target"
channels = ["c0"]
"#;
    let err = rpc
        .load_toml(cfg, false)
        .expect_err("load with an unknown codec should have failed");

    assert_eq!(
        err.code, STRUCTURAL,
        "unknown-codec rejection was not a structural error: {err:?}"
    );
    assert!(
        err.message.to_lowercase().contains("unknown codec"),
        "unknown-codec rejection wrong: {:?}",
        err.message
    );

    // (The daemon also carries the registered codec list in `data.available` for
    // capability discovery, §8/§15.26; the harness `RpcError` surfaces only
    // `{code, message}`, and the code + message already prove the structural
    // rejection the bash asserted.)
    assert_eq!(
        node_count(rpc),
        0,
        "a rejected load created nodes: {:?}",
        rpc.state()
    );
}

// ---- (3) A valid reference codec (no attributes) still loads ----

#[test]
fn valid_reference_codec_loads() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
"#;
    rpc.load_toml(cfg, false)
        .expect("a valid codec config failed to load");

    // The gate is the bad attribute, not codec nodes in general: the node exists and
    // its (flattened `state_extra`) `codec` field reads "reference" — the bash's
    // `.nodes[]|select(.name=="mux")|.codec=="reference"`.
    let mux = rpc
        .node("mux")
        .expect("valid codec did not load (no `mux` node)");
    assert_eq!(
        mux.get("codec").and_then(Value::as_str),
        Some("reference"),
        "loaded codec node is not the reference codec: {mux:?}"
    );
}
