#![forbid(unsafe_code)]

//! Structural configuration validation, end-to-end through a live daemon
//! (design §4 the graph rules, §11 load atomicity, §15.26 "a structurally-invalid
//! config never destroys a good running graph").
//!
//! Every case here is a **regression guard for a defect the Opus review reproduced
//! on a live daemon** (`docs/historical/26-claude-opus-code-review.md` §1/§7); the fixes moved
//! each one into `GraphConfig::validate` (or into serde's `deny_unknown_fields`),
//! so the guard's shape is always the same: hand the daemon the offending config,
//! assert the *structural* refusal names the offender, and assert the daemon and its
//! running graph came through untouched. The last part is the half that matters —
//! two of these defects were process-killers, and one of them killed the graph
//! *before* it killed the process:
//!
//! | ID | The defect this pins |
//! |----|----------------------|
//! | RV-11a | `replay_ring = <huge>` loaded, then SIGABRTed on the first hostward byte — from a config `load` had already persisted, so the daemon crash-looped on restart |
//! | LOAD-1/RV-11b | `hostward_buffer = <huge>` panicked inside `Wiring::build` *after* `load --replace` had torn the good graph down, leaving an empty graph |
//! | CODEC-1/WIRE-1 | a codec's multiplexed edge with `write_mode` omitted parked targetward forever while `send` answered `{"delivered": true}` |
//! | RV-4 | two effectively-`held` origins on one endpoint loaded happily; the loser could never write and appeared nowhere in `state` |
//! | DM-1 | a `faces = "target"` serial node loaded, seized the port with `TIOCEXCL`, and was wired to nothing |
//! | CP-2/CFG-3 | unknown keys were ignored, and a mis-typed *table* name plus `--replace` destroyed the running graph while reporting success |
//! | CFG-1 | normative §7.1 spells the flow-control values `xonxoff`/`rtscts`, which failed to deserialize — rejecting the entire file |
//! | AGENTS-INV7 | a whitespace-only name is the empty name wearing a costume (§3/§11/§12) |
//!
//! One family here is not a review defect and takes the same shape for a different
//! reason: §14's **refused-at-load** entries, whose deferral state is defined as "the
//! refusal is live, tested behavior — never a silent no-op". A refusal nobody exercises
//! is a claim, not a behavior, so each such entry gets a guard that hands **the daemon**
//! a configuration naming the deferred capability over RPC and asserts the refusal, its
//! text, and the untouched graph. §14 entry 14 (the serial output leg, §7.1) is DM-1's
//! test below; §14 entry 15 (the existing-terminal node, §7.7) is the pair after it.
//!
//! **What that pair does not cover, said rather than implied:** `serial-nexus-ctl load
//! <file.toml>` parses the file *client-side*, so an operator using the CLI meets a
//! `ctl`-side deserialization failure and the config never reaches the daemon at all.
//! These guards assert the daemon's refusal on the RPC surface — which is the one the
//! web console, the harness and any other client reach, and the one §14's "live, tested
//! behavior" is a claim about. The CLI's own refusal is a separate surface with a
//! separate error path, and neither guard here speaks for it.
//!
//! **Platform.** Structural validation needs no serial device, so all but one test
//! run everywhere (RULE 2). The single exception is CODEC-1's *positive* half, which
//! proves the accepted shape actually moves bytes: it needs a serial device, so it
//! self-skips where a pty cannot be one (macOS: `serial2` → `ENOTTY`).
//!
//! Assertions pin to structured RPC results and error codes, never CLI text (RULE 1)
//! — the one subprocess here is `serial-nexus-ctl`, and even there the assertion is its
//! *exit status* plus the surviving graph read back over RPC, never its prose.

use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, bin, serial_echo, wait_until};

/// `serial_nexus_rpc::AppError::Structural` — `APP_ERROR_BASE (-32000)` minus its ordinal
/// `2`. Every `GraphConfig::validate` failure surfaces with this code (§16.8).
const STRUCTURAL: i64 = -32002;

/// The JSON-RPC standard `invalid params` code. A config the daemon cannot even
/// *deserialize* — an unknown key, now that `deny_unknown_fields` is on — fails here
/// rather than in `validate`, and it fails in `parse_config_param`, which runs before
/// `--replace` touches the running graph.
const INVALID_PARAMS: i64 = -32602;

/// The reviewer's `replay_ring` value (§7 reproduction 2): 2^60, which allocated
/// out of the allocator's reach and aborted the process on the first hostward byte.
const ABSURD_REPLAY_RING: u64 = 1_152_921_504_606_846_976;

/// The reviewer's `hostward_buffer` value (§7 reproduction 3): `u64::MAX`, above
/// tokio's `MAX_PERMITS`, which panicked inside `mpsc::channel`. Deliberately sent as
/// JSON rather than TOML — TOML integers are *signed* 64-bit, so this value cannot be
/// written in a TOML file at all, while the `load` RPC takes it happily.
const ABSURD_HOSTWARD_BUFFER: u64 = u64::MAX;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

/// The daemon's per-boot nonce (`info.instance`, plan §11.8). Unchanged across a test
/// means *this* daemon process served both calls — the liveness assertion RV-11a
/// needs, since its defect was a SIGABRT followed by a crash loop.
fn instance(rpc: &Rpc) -> Value {
    rpc.info()
        .get("instance")
        .cloned()
        .expect("info must carry an instance nonce")
}

/// A two-node graph with no device requirement: a `waiting` serial (its device path
/// is absent) plus a pty. Everything asserted about it is structural, so it runs on
/// every platform.
fn good_graph(d: &Daemon) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "pty"
name = "con"
path = "{tty}"
[[edge]]
a = "usb0"
b = "con"
"#,
        dev = d.run().join("absent-device").display(),
        tty = d.run().join("ttyC").display(),
    )
}

/// Load [`good_graph`] and return its node-name fingerprint.
fn load_good_graph(d: &Daemon) -> Vec<String> {
    let rpc = d.rpc();
    let r = rpc
        .load_toml(&good_graph(d), false)
        .expect("load the good graph");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(2),
        "the baseline graph did not load two nodes: {r}"
    );
    let names = node_names(rpc);
    assert_eq!(
        names,
        ["con", "usb0"],
        "unexpected baseline graph: {names:?}"
    );
    names
}

/// The persisted configuration snapshot as text (`""` when the file does not exist).
/// `load` is a config-mutating verb, so a value that reaches this file is a value the
/// daemon will re-load — and re-die on — at the next start (§11/§15.9).
fn state_file_text(d: &Daemon) -> String {
    std::fs::read_to_string(d.run().state_file()).unwrap_or_default()
}

// ===========================================================================
// RV-11a (critical) — an absurd `replay_ring` is refused structurally, the
// daemon survives, and the value never reaches the persisted snapshot.
// ===========================================================================

/// The config that used to load and then abort the process on the first hostward
/// byte. `replay_ring` is a `usize` and 2^60 fits a TOML signed integer, so this is
/// exactly the file the reviewer wrote.
fn absurd_ring_graph(d: &Daemon) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
replay_ring = {ring}
"#,
        dev = d.run().join("absent-device").display(),
        ring = ABSURD_REPLAY_RING,
    )
}

#[test]
fn absurd_replay_ring_is_refused_structurally_naming_node_and_field() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let err = rpc
        .load_toml(&absurd_ring_graph(&d), false)
        .expect_err("an absurd replay_ring must be a structural load error");
    assert_eq!(
        err.code, STRUCTURAL,
        "replay_ring was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    // The operator must be able to find the offender without a bisect: the message
    // names the node *and* the configuration key.
    assert!(
        err.message.contains("usb0"),
        "the error must name the offending node: {}",
        err.message
    );
    assert!(
        err.message.contains("replay_ring"),
        "the error must name the offending field: {}",
        err.message
    );

    // Nothing was created (§11: the whole file is judged before anything exists).
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );

    // The cap is the shipped one, and the value one below it loads — the check is a
    // bound, not a blanket refusal of large rings.
    let legal = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
replay_ring = {ring}
"#,
        dev = d.run().join("absent-device").display(),
        ring = serial_nexus_core::config::MAX_REPLAY_RING,
    );
    rpc.load_toml(&legal, false)
        .expect("replay_ring at exactly the documented maximum must load");
}

#[test]
fn absurd_replay_ring_over_replace_spares_the_running_graph_and_the_state_file() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let before = load_good_graph(&d);
    let boot = instance(rpc);

    // The snapshot exists and describes the good graph.
    assert!(
        wait_until(Duration::from_secs(5), || !state_file_text(&d).is_empty()),
        "the good load did not persist a configuration snapshot"
    );
    assert!(
        state_file_text(&d).contains("usb0"),
        "the snapshot does not describe the good graph: {}",
        state_file_text(&d)
    );

    // `--replace` composes teardown-then-load, so a structural error must be caught
    // *before* the teardown (§15.26) — this is the half that made the original defect
    // unrecoverable: the value was persisted, so the daemon re-loaded it and re-died.
    let err = rpc
        .load_config(
            json!({ "node": [ {
                "type": "serial",
                "name": "boom",
                "device": d.run().join("absent-device").to_string_lossy(),
                "replay_ring": ABSURD_REPLAY_RING,
            } ] }),
            true,
        )
        .expect_err("an absurd replay_ring must be refused even under --replace");
    assert_eq!(
        err.code, STRUCTURAL,
        "not a structural refusal: [{}] {}",
        err.code, err.message
    );

    // The daemon is the same process it was (no SIGABRT, no crash loop), still
    // serving, and the running graph is untouched.
    assert_eq!(
        instance(rpc),
        boot,
        "the daemon restarted — the refused load did not spare the process"
    );
    assert_eq!(
        node_names(rpc),
        before,
        "the refused load disturbed the running graph"
    );

    // And the poison never reached the snapshot the next start would replay.
    let snapshot = state_file_text(&d);
    assert!(
        !snapshot.contains(&ABSURD_REPLAY_RING.to_string()),
        "the refused replay_ring reached the persisted state file:\n{snapshot}"
    );
    assert!(
        snapshot.contains("usb0"),
        "the snapshot no longer describes the surviving graph:\n{snapshot}"
    );
}

// ===========================================================================
// LOAD-1/RV-11b (high) — an absurd `hostward_buffer` under `--replace` is
// refused with the running graph intact.
// ===========================================================================

#[test]
fn absurd_hostward_buffer_under_replace_leaves_the_running_graph_intact() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let before = load_good_graph(&d);
    let boot = instance(rpc);

    // `u64::MAX` is unwritable in TOML (its integers are signed), so this goes over
    // the RPC as JSON — the same path `serial-nexus-ctl` uses once it has parsed a file,
    // and the path the panic used to be reached through.
    let err = rpc
        .load_config(
            json!({ "node": [ {
                "type": "pty",
                "name": "con",
                "path": d.run().join("ttyX").to_string_lossy(),
                "hostward_buffer": ABSURD_HOSTWARD_BUFFER,
            } ] }),
            true,
        )
        .expect_err("an absurd hostward_buffer must be refused, not panicked on");
    assert_eq!(
        err.code, STRUCTURAL,
        "not a structural refusal: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("hostward_buffer"),
        "the error must name the offending field: {}",
        err.message
    );

    // The whole point: the teardown never ran. Same daemon, same graph.
    assert_eq!(
        instance(rpc),
        boot,
        "the daemon restarted during a refused --replace"
    );
    assert_eq!(
        node_names(rpc),
        before,
        "the refused --replace tore the running graph down (the LOAD-1 defect)"
    );

    // The floor is still enforced too — 0 was the one bound that already existed, and
    // adding the ceiling must not have displaced it.
    let zero = rpc
        .load_config(
            json!({ "node": [ {
                "type": "pty",
                "name": "con2",
                "path": d.run().join("ttyY").to_string_lossy(),
                "hostward_buffer": 0,
            } ] }),
            true,
        )
        .expect_err("hostward_buffer = 0 must still be refused");
    assert_eq!(
        zero.code, STRUCTURAL,
        "hostward_buffer = 0 was refused, but not structurally: [{}] {}",
        zero.code, zero.message
    );
    assert_eq!(
        node_names(rpc),
        before,
        "the refused zero-buffer --replace tore the running graph down"
    );
}

// ===========================================================================
// CODEC-1/WIRE-1 (high) — an edge into a codec's multiplexed endpoint must
// declare `held` (or `never`); the accepted shape really does move bytes.
// ===========================================================================

/// serial → codec, with the edge's `write_mode` written exactly as
/// `packaging/serial-nexus-daemon.example.toml` used to show it: omitted.
fn mux_graph(device: &str, write_mode: Option<&str>) -> String {
    let mode = match write_mode {
        Some(m) => format!("write_mode = \"{m}\"\n"),
        None => String::new(),
    };
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
[[edge]]
a = "usb0"
b = "mux"
{mode}"#,
    )
}

#[test]
fn codec_mux_edge_without_write_mode_is_a_structural_load_error() {
    // No device needed: the edge rule is judged from configuration alone, so this
    // runs on every platform.
    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("absent-device").to_string_lossy().into_owned();

    let err = rpc
        .load_toml(&mux_graph(&dev, None), false)
        .expect_err("a mux edge with the default write_mode must fail the load");
    assert_eq!(
        err.code, STRUCTURAL,
        "the mux edge was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    // The offender is an *edge*, so the message names it by index and by both ends.
    assert!(
        err.message.contains("edge 0"),
        "the error must name the offending edge: {}",
        err.message
    );
    assert!(
        err.message.contains("usb0") && err.message.contains("mux"),
        "the error must name both ends of the edge: {}",
        err.message
    );
    assert!(
        err.message.contains("held"),
        "the error must say which mode works: {}",
        err.message
    );
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );

    // `never` is the deliberate read-only arm and stays legal — the rule refuses the
    // silent-stall modes, not every mode.
    rpc.load_toml(&mux_graph(&dev, Some("never")), false)
        .expect("write_mode = \"never\" on a mux edge must still load");
}

#[test]
fn held_mux_edge_actually_advances_accepted_targetward() {
    // The positive half of CODEC-1: the shape the rule steers operators toward must
    // genuinely carry bytes to the device. The original defect answered
    // `{"delivered": true}` while `accepted_targetward` stayed 0 forever, so the
    // counter — not the `send` reply — is the oracle.
    let Some(dev) = serial_echo() else {
        eprintln!(
            "SKIP held_mux_edge_actually_advances_accepted_targetward: needs a Linux \
             pty-as-serial device (serial2 rejects a pty on macOS: ENOTTY)"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();

    let cfg = mux_graph(&dev.device().to_string_lossy(), Some("held"));
    rpc.load_toml(&cfg, false).expect("load the held mux graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active over the sim device: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("mux", "active", Duration::from_secs(10)),
        "the codec did not come up active: {:?}",
        rpc.node("mux")
    );
    // The held edge owns the serial's write lock (§6) — the precondition the
    // multiplexed pump needs, and the thing an `on-demand` edge could never get.
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("usb0")
                .and_then(|n| n.pointer("/lock/holder").cloned())
                == Some(json!("mux"))
        }),
        "the held mux edge does not hold the serial lock: {:?}",
        rpc.node("usb0")
    );

    let sent = rpc
        .send("mux/c0", "hello", false, 5000)
        .expect("send through the codec channel");
    assert_eq!(
        sent.get("delivered").and_then(Value::as_bool),
        Some(true),
        "send did not report delivery: {sent}"
    );
    let n = sent
        .get("sent")
        .and_then(Value::as_u64)
        .expect("send must report a byte count");

    // The counter is the claim `send`'s reply cannot make: the bytes were handed to
    // the serial's targetward path, not parked in a pump that will never be granted.
    let accepted = || {
        rpc.node("mux")
            .as_ref()
            .and_then(|m| m.pointer("/channels/c0/accepted_targetward"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    assert!(
        wait_until(Duration::from_secs(10), || accepted() >= n),
        "accepted_targetward stalled at {} (expected >= {n}) — the mux pump never got \
         the lock: {:?}",
        accepted(),
        rpc.node("mux")
    );
}

// ===========================================================================
// RV-4 (medium) — at most one effectively-`held` origin per host endpoint.
// ===========================================================================

/// Two maps hanging off one serial's host endpoint, with `write_mode` written
/// *nowhere* — the reachable shape (§7 reproduction 7). Both raw edges are promoted
/// to `held` by the map rule, so both origins claim "held indefinitely" and one of
/// them silently loses forever.
fn two_maps_graph(d: &Daemon, arbitration: Option<&str>) -> String {
    let arb = match arbitration {
        Some(a) => format!("arbitration = \"{a}\"\n"),
        None => String::new(),
    };
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
{arb}[[node]]
type = "map"
name = "m1"
hostward = ["crlf"]
[[node]]
type = "map"
name = "m2"
hostward = ["crlf"]
[[edge]]
a = "usb0"
b = "m1/raw"
[[edge]]
a = "usb0"
b = "m2/raw"
"#,
        dev = d.run().join("absent-device").display(),
    )
}

#[test]
fn two_promoted_held_edges_on_one_endpoint_are_refused_naming_both_offenders() {
    let d = Daemon::start();
    let rpc = d.rpc();

    let err = rpc
        .load_toml(&two_maps_graph(&d, None), false)
        .expect_err("two effectively-held origins on one endpoint must be refused");
    assert_eq!(
        err.code, STRUCTURAL,
        "the held collision was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("usb0"),
        "the error must name the contested endpoint: {}",
        err.message
    );
    // Both offenders, because naming one leaves the operator guessing which map to
    // change — and the loser is the one the lock silently starves.
    assert!(
        err.message.contains("m1/raw"),
        "the error must name the first held origin: {}",
        err.message
    );
    assert!(
        err.message.contains("m2/raw"),
        "the error must name the second held origin: {}",
        err.message
    );
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );
}

#[test]
fn free_for_all_endpoint_still_accepts_two_held_edges() {
    // §6: a free-for-all endpoint has no lock at all, so two "held" origins there are
    // the operator's explicit, working choice — the rule must not over-reject it.
    let d = Daemon::start();
    let rpc = d.rpc();

    let r = rpc
        .load_toml(&two_maps_graph(&d, Some("free-for-all")), false)
        .expect("two held edges on a free-for-all endpoint must still load");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(3),
        "the free-for-all graph did not load all three nodes: {r}"
    );
    assert_eq!(
        node_names(rpc),
        ["m1", "m2", "usb0"],
        "unexpected graph after the free-for-all load"
    );
}

// ===========================================================================
// DM-1 (medium) — `faces = "target"` is refused on a serial node only.
// ===========================================================================

#[test]
fn serial_faces_target_is_refused_as_not_implemented() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "outport"
device = "{dev}"
faces = "target"
"#,
        dev = d.run().join("absent-device").display(),
    );

    let err = rpc
        .load_toml(&cfg, false)
        .expect_err("a target-facing serial node must be refused, not silently held open");
    assert_eq!(
        err.code, STRUCTURAL,
        "the target-facing serial was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("outport"),
        "the error must name the offending node: {}",
        err.message
    );
    // The refusal has to say *why* it is refused — the role is designed (§7.1) but
    // deferred (§14), which is a different answer from "that is not a thing".
    assert!(
        err.message.contains("not implemented"),
        "the error must say the role is not implemented: {}",
        err.message
    );
    assert!(
        err.message.contains("§14"),
        "the error must point at the deferred-work section: {}",
        err.message
    );
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );
}

#[test]
fn codec_and_leg_may_still_face_target() {
    // The refusal above is serial-specific: a demultiplexing codec and a sending leg
    // are *defined* by facing target (§7.5/§7.4), and both must keep loading.
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "connect"
address = "{sock}"
channels = ["c0"]
"#,
        sock = d.run().join("peer.sock").display(),
    );

    let r = rpc
        .load_toml(&cfg, false)
        .expect("a target-facing codec and leg must still load");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(2),
        "the target-facing codec/leg graph did not load: {r}"
    );
    assert_eq!(
        node_names(rpc),
        ["mux", "uplink"],
        "unexpected graph after the target-facing load"
    );
}

// ===========================================================================
// §14.15 (medium) — the existing-terminal node (§7.7) is *refused-at-load*, and
// the refusal names the node kinds that do exist.
// ===========================================================================

/// The shipped node kinds, spelled as `type` accepts them — `NodeConfig`'s variants
/// under `rename_all = "kebab-case"` (§7.1–§7.6, §7.8). §7.7's existing-terminal node
/// is deliberately absent: it is design-specified and unimplemented (§14 entry 15).
///
/// Hand-kept, and that is the point — this list is the *design's* claim about which
/// kinds exist, checked against the refusal the daemon actually emits. A seventh kind
/// landing reddens `existing_terminal_is_refused_at_load_listing_the_shipped_kinds`
/// until someone re-reads §7.7 and §14 and adds it here, which is the only moment at
/// which "listing the node kinds that do exist" can go quietly stale.
const SHIPPED_NODE_KINDS: [&str; 6] = ["serial", "pty", "log", "codec", "leg", "map"];

/// The §7.7 configuration an operator would actually write — a QEMU serial console
/// reached by path, `faces = "host"` because the far side acts as the target. Both
/// keys are §7.7's own vocabulary and neither is ever read: the node kind is refused
/// before any field of it is deserialized.
fn existing_terminal_graph(d: &Daemon) -> String {
    format!(
        r#"
[[node]]
type = "existing-terminal"
name = "qemu"
path = "{tty}"
faces = "host"
"#,
        tty = d.run().join("qemu-console").display(),
    )
}

/// The backtick-quoted kind names serde lists after `expected one of`, in order.
/// Returns an empty vector when the message has no such clause at all, so the
/// assertion that consumes it fails naming the whole message rather than panicking.
fn kinds_listed_in(message: &str) -> Vec<String> {
    let Some((_, tail)) = message.split_once("expected one of ") else {
        return Vec::new();
    };
    tail.split(',')
        .filter_map(|s| s.trim().strip_prefix('`')?.strip_suffix('`'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn existing_terminal_is_refused_at_load_listing_the_shipped_kinds() {
    // §14's deferral vocabulary calls this state *refused-at-load*: "the model
    // specifies it and the schema admits the words, but the implementation does not
    // exist: a configuration naming it is refused ... listing what does exist. The
    // refusal is live, tested behavior — never a silent no-op." This test is the
    // "tested" half; before it existed the refusal was real but unexercised, falling
    // out incidentally of serde's internally-tagged enum rather than being anybody's
    // asserted promise (finding 12).
    //
    // **What the daemon actually answers, which is not what §14's vocabulary says.**
    // §7.7 promises "the same treatment §7.1 gives the serial output leg", and §7.1's
    // is the DM-1 test above: `GraphConfig::validate` refuses it with a STRUCTURAL
    // error whose text says "not implemented" and cites §14. §7.7's node never reaches
    // `validate`. `type = "existing-terminal"` is not a `NodeConfig` variant, so it
    // dies one stage earlier, in `parse_config_param`'s `serde_json::from_value`, as
    // INVALID_PARAMS carrying serde's unknown-variant text. That text does list the
    // shipped kinds — the substance §7.7 and §14 promise — but it is not a structural
    // error and it names neither the deferral nor §14. Asserted here as it is: a guard
    // written to §14's vocabulary instead of to the daemon would be a guard for
    // behavior that does not exist, and the divergence belongs in the design's hands,
    // not hidden behind a laxer assertion here.
    let d = Daemon::start();
    let rpc = d.rpc();

    let err = rpc
        .load_toml(&existing_terminal_graph(&d), false)
        .expect_err("an existing-terminal node must be refused at load, not created");
    assert_eq!(
        err.code, INVALID_PARAMS,
        "the existing-terminal node was refused, but not while parsing params: [{}] {}",
        err.code, err.message
    );
    // The operator has to learn *which word* the daemon rejected, or a refusal of a
    // five-node file is a bisect.
    assert!(
        err.message.contains("existing-terminal"),
        "the refusal must name the node kind it refused: {}",
        err.message
    );

    // The promise §7.7 and §14 both spell out: the refusal lists the node kinds that
    // do exist. Checked as a set equality, not as "contains serial" — a message that
    // listed four of six would satisfy every containment check while sending the
    // operator looking for a kind it never mentioned.
    let mut listed = kinds_listed_in(&err.message);
    let mut expected: Vec<String> = SHIPPED_NODE_KINDS.iter().map(|s| (*s).to_owned()).collect();
    listed.sort();
    expected.sort();
    assert_eq!(
        listed, expected,
        "the refusal must list exactly the shipped node kinds (§7.7): {}",
        err.message
    );

    // Refused *at load* means nothing was created — §11 judges the whole file before
    // anything exists, and a deferred kind must not be the exception that leaves a
    // half-graph behind.
    assert!(
        node_names(rpc).is_empty(),
        "a refused existing-terminal load created nodes: {:?}",
        node_names(rpc)
    );
    assert!(
        state_file_text(&d).is_empty(),
        "a refused existing-terminal load reached the persisted snapshot: {}",
        state_file_text(&d)
    );

    // The control: the same file with a shipped kind loads. Without it, this guard
    // would pass just as happily against a daemon that refused every config it was
    // handed, and would say nothing about the *kind* being the reason.
    let shipped = format!(
        r#"
[[node]]
type = "pty"
name = "qemu"
path = "{tty}"
"#,
        tty = d.run().join("qemu-console").display(),
    );
    let r = rpc
        .load_toml(&shipped, false)
        .expect("a shipped node kind at the same path must still load");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(1),
        "the control graph did not load: {r}"
    );
}

#[test]
fn a_refused_existing_terminal_disturbs_neither_the_running_graph_nor_the_daemon() {
    // The other half of "never a silent no-op" (§14): the refusal must also not be a
    // silent *demolition*. `load --replace` of an unknown node kind is exactly the
    // CP-2/CFG-3 shape below — a file the daemon cannot deserialize, arriving with a
    // verb that tears the good graph down first — so the deferred kind gets the same
    // check the mis-typed table name gets, and `add-node` gets it too, since that is
    // the surface an operator reaches for when a graph is already running.
    let d = Daemon::start();
    let rpc = d.rpc();
    let before = load_good_graph(&d);
    let boot = instance(rpc);
    // `Daemon::dispatch` snapshots a successful config mutation **before** it writes the
    // reply, so `load` having answered already means the file is on disk. The wait is a
    // cheap belt on the file *appearing* (a fresh run starts with no state file at all),
    // not a race with the write — and it still fails loudly rather than comparing
    // against an empty baseline, which would assert nothing.
    assert!(
        wait_until(Duration::from_secs(5), || !state_file_text(&d).is_empty()),
        "the good load did not persist a configuration snapshot"
    );
    let persisted = state_file_text(&d);

    let err = rpc
        .load_toml(&existing_terminal_graph(&d), true)
        .expect_err("an existing-terminal node must be refused under --replace too");
    assert_eq!(
        err.code, INVALID_PARAMS,
        "the --replace refusal changed code: [{}] {}",
        err.code, err.message
    );

    let err = rpc
        .add_node_toml(&existing_terminal_graph(&d))
        .expect_err("`add-node` of an existing-terminal node must be refused");
    assert_eq!(
        err.code, INVALID_PARAMS,
        "the add-node refusal was not an invalid-params error: [{}] {}. This pins TODAY's \
         shape deliberately — §14 entry 15's refusal is serde's unknown-variant error, not \
         entry 14's structural one, and plan §18 item 45 is the decision about upgrading it. \
         If item 45 landed, this guard is what tells you,",
        err.code, err.message
    );
    assert!(
        err.message.contains("existing-terminal"),
        "the add-node refusal must name the node kind it refused: {}",
        err.message
    );
    // Set equality, not a count: six *wrong* names would satisfy a length check, and the
    // helper already returns the names. The asymmetry with the `load` assertion above was
    // free to remove and is exactly the shape that makes one of two sibling guards weaker
    // than it reads.
    let mut listed = kinds_listed_in(&err.message);
    listed.sort();
    let mut shipped: Vec<String> = SHIPPED_NODE_KINDS.iter().map(|k| (*k).to_owned()).collect();
    shipped.sort();
    assert_eq!(
        listed, shipped,
        "the add-node refusal must list exactly the shipped kinds too: {}",
        err.message
    );

    assert_eq!(instance(rpc), boot, "the daemon restarted");
    assert_eq!(
        node_names(rpc),
        before,
        "a refused existing-terminal config disturbed the running graph"
    );
    assert_eq!(
        state_file_text(&d),
        persisted,
        "a refused existing-terminal config rewrote the persisted snapshot"
    );
}

// ===========================================================================
// CP-2/CFG-3 (medium) — unknown keys are named; a mis-typed table name cannot
// destroy a running graph.
// ===========================================================================

#[test]
fn misspelled_config_key_is_rejected_naming_the_key() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let before = load_good_graph(&d);
    let boot = instance(rpc);

    // The reviewer's typo (§7 reproduction 8): accepted in silence, while `dump`
    // showed the untouched default — a validation that never happened.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "con"
path = "{tty}"
advertized_baud = 9600
"#,
        tty = d.run().join("ttyZ").display(),
    );

    let err = rpc
        .load_toml(&cfg, true)
        .expect_err("an unknown configuration key must be rejected");
    assert_eq!(
        err.code, INVALID_PARAMS,
        "the unknown key was rejected, but not as invalid params: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("advertized_baud"),
        "the error must name the unrecognised key: {}",
        err.message
    );

    // The rejection happens while parsing params — before `--replace` gets anywhere
    // near the running graph.
    assert_eq!(instance(rpc), boot, "the daemon restarted");
    assert_eq!(
        node_names(rpc),
        before,
        "the rejected --replace disturbed the running graph"
    );
}

/// Run `serial-nexus-ctl --socket <sock> load <file> --replace` and return its exit
/// status. Nothing about its *output* is asserted anywhere — only the status, and
/// the graph read back over RPC (RULE 1).
fn ctl_load_replace(d: &Daemon, file: &std::path::Path) -> std::process::ExitStatus {
    Command::new(bin("serial-nexus-ctl"))
        .arg("--socket")
        .arg(d.socket())
        .arg("load")
        .arg(file)
        .arg("--replace")
        .output()
        .expect("run serial-nexus-ctl load --replace")
        .status
}

#[test]
fn a_file_that_parses_to_nothing_cannot_destroy_a_running_graph() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let before = load_good_graph(&d);

    // (a) The reviewer's mis-typed table name (§7 reproduction 8). It used to parse
    // to an empty graph, which `--replace` executed as an unannounced `teardown`
    // reporting exit 0.
    let typo = d.run().join("typo.toml");
    std::fs::write(
        &typo,
        "[[nodez]]\ntype = \"log\"\nname = \"x\"\ndirectory = \"/tmp\"\nfilename = \"x.log\"\n",
    )
    .expect("write the mis-typed config");
    let status = ctl_load_replace(&d, &typo);
    assert!(
        !status.success(),
        "`load --replace` of a mis-typed table name reported success: {status:?}"
    );
    assert_eq!(
        node_names(rpc),
        before,
        "a mis-typed table name destroyed the running graph"
    );

    // (b) The general shape behind it: any non-empty file that parses to nothing.
    // A comment-only file has no unknown key for serde to catch, so this is the
    // backstop rather than the same check twice.
    let comments = d.run().join("comments.toml");
    std::fs::write(&comments, "# a config that declares nothing at all\n")
        .expect("write the comment-only config");
    let status = ctl_load_replace(&d, &comments);
    assert!(
        !status.success(),
        "`load --replace` of a comment-only file reported success: {status:?}"
    );
    assert_eq!(
        node_names(rpc),
        before,
        "a comment-only file destroyed the running graph"
    );

    // The control: a real config still replaces the graph, so the guard above is a
    // check on *emptiness*, not a broken `--replace`.
    let real = d.run().join("real.toml");
    std::fs::write(
        &real,
        format!(
            "[[node]]\ntype = \"log\"\nname = \"sink\"\ndirectory = \"{dir}\"\nfilename = \"s.log\"\n",
            dir = d.run().path().display(),
        ),
    )
    .expect("write the replacement config");
    let status = ctl_load_replace(&d, &real);
    assert!(
        status.success(),
        "a valid `load --replace` failed: {status:?}"
    );
    assert_eq!(
        node_names(rpc),
        ["sink"],
        "the valid --replace did not take effect"
    );
}

// ===========================================================================
// CFG-1 (low) — every documented flow-control spelling loads; `dump` is kebab.
// ===========================================================================

#[test]
fn every_flow_control_spelling_loads_and_dumps_kebab_case() {
    // §7.1 spells these `xonxoff`/`rtscts`; the serialized form is kebab-case. Both
    // must parse — the design's own spellings used to fail deserialization, which is
    // a *parse* error and therefore rejects the entire file, not just the field.
    let d = Daemon::start();
    let rpc = d.rpc();
    let cases = [
        ("f_none", "none", "none"),
        ("f_xon_kebab", "xon-xoff", "xon-xoff"),
        ("f_xon_flat", "xonxoff", "xon-xoff"),
        ("f_rts_kebab", "rts-cts", "rts-cts"),
        ("f_rts_flat", "rtscts", "rts-cts"),
    ];

    let mut cfg = String::new();
    for (name, spelling, _) in cases {
        cfg.push_str(&format!(
            "[[node]]\ntype = \"serial\"\nname = \"{name}\"\ndevice = \"{dev}\"\nflow_control = \"{spelling}\"\n",
            dev = d.run().join(&format!("absent-{name}")).display(),
        ));
    }
    let r = rpc
        .load_toml(&cfg, false)
        .expect("every documented flow-control spelling must load");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(cases.len() as u64),
        "not every flow-control node loaded: {r}"
    );

    // `dump` is the round-trip surface: whichever spelling was written, the canonical
    // kebab-case form comes back, so a dump→load cycle is stable.
    let dumped = rpc.dump();
    let nodes = dumped
        .get("node")
        .and_then(Value::as_array)
        .expect("dump must carry a node array")
        .clone();
    for (name, spelling, canonical) in cases {
        let node = nodes
            .iter()
            .find(|n| n.get("name").and_then(Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("dump is missing node {name}: {dumped}"));
        assert_eq!(
            node.get("flow_control").and_then(Value::as_str),
            Some(canonical),
            "{spelling:?} did not round-trip to the canonical {canonical:?}: {node}"
        );
    }
}

// ===========================================================================
// AGENTS-INV7 (low) — a whitespace-only name is as unusable as an empty one,
// while the *reserved* empty default-endpoint name stays legal.
// ===========================================================================

#[test]
fn whitespace_only_node_name_is_refused() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "log"
name = "   "
directory = "{dir}"
filename = "blank.log"
"#,
        dir = d.run().path().display(),
    );

    let err = rpc
        .load_toml(&cfg, false)
        .expect_err("a whitespace-only node name must be refused");
    assert_eq!(
        err.code, STRUCTURAL,
        "the blank node name was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("whitespace-only"),
        "the error must say what is wrong with the name: {}",
        err.message
    );
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );
}

#[test]
fn whitespace_only_channel_identity_is_refused() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = [" "]
"#;

    let err = rpc
        .load_toml(cfg, false)
        .expect_err("a whitespace-only channel identity must be refused");
    assert_eq!(
        err.code, STRUCTURAL,
        "the blank channel identity was refused, but not structurally: [{}] {}",
        err.code, err.message
    );
    assert!(
        err.message.contains("whitespace-only"),
        "the error must say what is wrong with the identity: {}",
        err.message
    );
    assert!(
        err.message.contains("mux"),
        "the error must name the declaring node: {}",
        err.message
    );
    assert!(
        node_names(rpc).is_empty(),
        "a structural error created nodes: {:?}",
        node_names(rpc)
    );
}

#[test]
fn the_reserved_empty_default_endpoint_name_still_works() {
    // The blank-name rule must not catch the *reserved* empty local name every
    // default endpoint carries: `b = "mux"` addresses the codec's multiplexed
    // endpoint, whose local name is the empty string (§3, `DEFAULT_ENDPOINT`).
    let d = Daemon::start();
    let rpc = d.rpc();
    let dev = d.run().join("absent-device").to_string_lossy().into_owned();

    let r = rpc
        .load_toml(&mux_graph(&dev, Some("held")), false)
        .expect("addressing a default endpoint by bare node name must load");
    assert_eq!(
        r.get("loaded").and_then(Value::as_u64),
        Some(2),
        "the default-endpoint graph did not load: {r}"
    );
    // And the edge really did land on the default endpoint: the held origin took the
    // serial's write lock, which only a wired edge can do.
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("usb0")
                .and_then(|n| n.pointer("/lock/holder").cloned())
                == Some(json!("mux"))
        }),
        "the default-endpoint edge was not wired: {:?}",
        rpc.node("usb0")
    );
}
