//! Phase 6 leg loopback-only security gate, ported from
//! `scripts/validate/phase6/insecure-bind.sh` (design §7.4/§9, footgun surfaced per
//! §15.12).
//!
//! A leg bound/dialed to a non-loopback address *without* `insecure_bind` is a
//! **structural** load error (`-32002`): the load fails naming the offender and the
//! flag, and nothing is created (§4/§11 structural atomicity). *With* the flag the
//! config loads — the node then faults on the bind, an environmental state, so the
//! load itself succeeds and the node is present, carrying a visible `insecure_bind`
//! confession in `state` (§15.12). A loopback bind is the default safe case and
//! loads without the flag.
//!
//! This is config-plane only — no sockets are opened by the assertions — so a leg
//! needs no serial *device* and every case runs on every platform (the macOS
//! replacement for the `jq`/`grep`-driven bash). Ground truth is the structured RPC
//! error object and the `state` snapshot, never CLI text.

use nexus_itest::Daemon;

/// The structural validation error code (`nexus_rpc::AppError::Structural`, §16.8:
/// `APP_ERROR_BASE = -32000`, offset -2). Hardcoded as a literal because `nexus-itest`
/// does not depend on `nexus-rpc`; this mirrors the bash `grep -q -- "-32002"`.
const STRUCTURAL: i64 = -32002;

/// A non-loopback tcp `listen` leg without `insecure_bind`.
const BAD: &str = r#"
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "10.0.0.5:9999"
channels = ["console"]
"#;

/// The same address, opted into with the deliberately-ugly flag.
const OK: &str = r#"
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "10.0.0.5:9999"
insecure_bind = true
channels = ["console"]
"#;

/// A loopback bind (the default, safe case).
const LOOPBACK: &str = r#"
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "127.0.0.1:0"
channels = ["console"]
"#;

/// A non-loopback tcp bind WITHOUT `insecure_bind`: structural refusal (§7.4).
/// The error names the structural code, the offending node, and the flag, and the
/// graph is left empty (a refused load creates nothing).
#[test]
fn non_loopback_bind_without_flag_is_structural_refusal() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let err = rpc
        .load_toml(BAD, false)
        .expect_err("non-loopback bind should have failed to load");

    // The structural error code (§16.8 registry; base -32000, Structural = -32002).
    assert_eq!(
        err.code, STRUCTURAL,
        "expected structural error code {STRUCTURAL} (-32002), got {}: {}",
        err.code, err.message,
    );
    // The message must name the offending node …
    assert!(
        err.message.contains("uplink"),
        "structural error must name the offending node `uplink`: {}",
        err.message,
    );
    // … and the flag that would opt into it.
    assert!(
        err.message.contains("insecure_bind"),
        "structural error must name `insecure_bind`: {}",
        err.message,
    );

    // Nothing created: the graph is still empty (structural atomicity).
    let nodes = rpc.state()["nodes"]
        .as_array()
        .expect("state.nodes is an array")
        .len();
    assert_eq!(nodes, 0, "a refused load must create nothing (empty graph)");
}

/// The same address WITH `insecure_bind` loads: the config is valid, so the load
/// succeeds and the node is present (it then faults on the bind, an environmental
/// state, not a load failure). The §9 named footgun is a visible, greppable
/// confession in `state` (§15.12).
#[test]
fn insecure_bind_true_loads_and_marks_state() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let res = rpc
        .load_toml(OK, false)
        .expect("insecure_bind=true must load");
    assert_eq!(
        res.get("loaded").and_then(|v| v.as_u64()),
        Some(1),
        "insecure_bind=true must let the leg load (loaded==1): {res}",
    );

    // The leg node is present after the insecure load.
    let node = rpc
        .node("uplink")
        .expect("the leg node must be present after an insecure load");

    // An insecure leg must carry the visible `insecure_bind` marker in state.
    assert_eq!(
        node.get("insecure_bind").and_then(|v| v.as_bool()),
        Some(true),
        "an insecure leg must carry the insecure_bind marker in state: {node}",
    );
}

/// A loopback bind loads without the flag (the default, safe case).
#[test]
fn loopback_bind_loads_without_flag() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let res = rpc
        .load_toml(LOOPBACK, false)
        .expect("a loopback leg must load without insecure_bind");
    assert_eq!(
        res.get("loaded").and_then(|v| v.as_u64()),
        Some(1),
        "a loopback leg must load without insecure_bind (loaded==1): {res}",
    );
}
