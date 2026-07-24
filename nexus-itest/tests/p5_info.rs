//! Phase 5 `info`-verb slice, ported from `scripts/validate/phase5/info.sh`
//! (design §10 / §15.26 / §8). Two properties, neither needing a serial *device*,
//! so both run on **every platform** — the macOS-portable replacement for the
//! `jq`/`grep`-over-CLI-stderr bash:
//!
//! 1. The `info` verb reports the daemon's full capability surface — its
//!    `daemon_version` (string), the numeric `wire_version` and `envelope_version`
//!    protocol versions, and the registered `codecs` (including `reference`) — so
//!    tools discover what a possibly-custom daemon supports rather than assume it.
//! 2. An unknown codec in configuration fails **structurally** (nothing created —
//!    the load is atomic), and the error names the **available** codec list
//!    (including `reference`) so tools can discover the supported set.
//!
//! Assertions are on the structured RPC `info` result and the structured
//! `RpcError` (code + message), never parsed human CLI text (§5).
//!
//! Deviation from the bash, and why (it preserves the original assertion): the bash
//! grep-checked the CLI's rendered `error.data` JSON on stderr for the tokens
//! `"available"` and `reference`. The harness's `RpcError` surfaces the JSON-RPC
//! `code` and `message` (not the `data` object). The daemon builds that `message`
//! to embed the available list verbatim — `... unknown codec "…"; available:
//! ["reference"]` (see `nexus-daemon/src/daemon.rs`) — so this port asserts the same
//! two tokens against `RpcError::message` plus the structural error `code`, a
//! faithful equivalent of the original "available list is present, with reference".

use nexus_itest::Daemon;

/// `AppError::Structural` — `APP_ERROR_BASE (-32000) - 2` (nexus-rpc `AppError`).
const STRUCTURAL: i64 = -32002;

// ---- (1) info reports the full capability surface (§10/§15.26) ------------------

#[test]
fn info_reports_full_capability_surface() {
    let d = Daemon::start();
    let info = d.rpc().info();

    // .codecs | index("reference") — the reference framing codec is registered.
    let codecs = info["codecs"]
        .as_array()
        .unwrap_or_else(|| panic!("info.codecs is not an array: {info}"));
    assert!(
        codecs.iter().any(|c| c.as_str() == Some("reference")),
        "info.codecs does not contain \"reference\": {info}"
    );

    // .wire_version | numbers — the wire protocol version is a number.
    assert!(
        info["wire_version"].is_number(),
        "info.wire_version is not a number: {info}"
    );
    // .envelope_version | numbers — the envelope protocol version is a number.
    assert!(
        info["envelope_version"].is_number(),
        "info.envelope_version is not a number: {info}"
    );
    // .daemon_version | type == "string".
    assert!(
        info["daemon_version"].is_string(),
        "info.daemon_version is not a string: {info}"
    );
    // .instance | number — the per-boot nonce for tap offset-reset detection (§11.8).
    assert!(
        info["instance"].is_number(),
        "info.instance is not a number: {info}"
    );
}

// ---- (2) an unknown codec is structural, atomic, and names the available list ---

#[test]
fn unknown_codec_is_rejected_atomically_with_available_list() {
    let d = Daemon::start();
    let rpc = d.rpc();

    // A codec node naming a codec that does not exist (faithful to the bash TOML).
    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "does-not-exist"
faces = "target"
channels = ["c0"]
"#;

    // The load must fail — an unknown codec is a STRUCTURAL error (§8/§15.26).
    let err = rpc
        .load_toml(cfg, false)
        .expect_err("load with an unknown codec should have failed");
    assert_eq!(
        err.code, STRUCTURAL,
        "unknown-codec load returned code {} (want STRUCTURAL {STRUCTURAL}); message={:?}",
        err.code, err.message
    );

    // The error names the available codec list, including `reference` — the same two
    // tokens the bash grepped for, here embedded in the daemon-built message.
    assert!(
        err.message.contains("available"),
        "unknown-codec error message missing the available list: {:?}",
        err.message
    );
    assert!(
        err.message.contains("reference"),
        "unknown-codec error message missing `reference` in the available list: {:?}",
        err.message
    );

    // A rejected load must create nothing (structural atomicity, §11).
    let nodes = rpc.state()["nodes"]
        .as_array()
        .expect("state.nodes is an array")
        .len();
    assert_eq!(
        nodes, 0,
        "a rejected load created {nodes} node(s); the load must be atomic"
    );
}
