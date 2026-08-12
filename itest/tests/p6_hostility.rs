#![forbid(unsafe_code)]

//! Phase 6 wire hostility, ported from `scripts/validate/phase6/hostility.sh`
//! (design §9 clause-4/6 clean-refusal contract, §7.4 leg self-heal).
//!
//! A receiving leg (`faces = host`, `role = listen`, unix transport, one channel
//! `console`) is driven by `serial-nexus-sim wire` acting as a hostile-or-conforming peer.
//! Each of four hostilities must leave the leg **faulted** with the reason surfaced
//! in state (never a panic, never a silent drop), the daemon closing the connection
//! (`peer_closed`):
//!   1. version mismatch (`--hello-version 999`) — the version appears in the reason;
//!   2. bad magic (`--bad-magic`) — a not-our-protocol peer;
//!   3. oversize frame (`--oversize-frame`, §9 clause 4) — refused on the length prefix;
//!   4. unknown frame type (`--unknown-type`, §9 clause 6).
//!
//! Then the leg **heals**: after all that hostility a conforming peer binds cleanly
//! (`status == active`, `channels.console.binding == bound`) — faulted-and-wait
//! self-heals (§7.4).
//!
//! A fifth hostility gets two tests of its own, because it is the one that is not a
//! *frame* at all: a peer that connects and never finishes saying hello, either mute or
//! trickling ([`handshake_deadline_case`], §15.24). All four above complete or actively
//! terminate their hello, so the leg's overall handshake deadline — the promise that a
//! trickling peer cannot wedge a listener — had no test in the tree (37-LEG-4).
//!
//! A leg needs no serial *device* (the transport is a loopback unix socket), so this
//! test runs on every platform. Ground truth is the structured `state` snapshot
//! (`.status` / `.reason` / `.channels.console.binding`) and the sim's own JSON
//! verdict (`.pass` / `.peer_closed`) — never parsed CLI text.

use std::process::Command;
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, Rpc, Sim, bin, wait_until};

/// Run `serial-nexus-sim wire` in the foreground to completion against `address`,
/// announcing `console`, with the hostility flags in `extra`. Returns its parsed
/// JSON verdict (the sim prints the verdict to stdout, §Phase 6 wire mode).
fn run_wire(address: &str, extra: &[&str]) -> Value {
    let out = Command::new(bin("serial-nexus-sim"))
        .arg("wire")
        .args(["--transport", "unix"])
        .args(["--address", address])
        .args(["--announce", "console"])
        .args(["--hold-ms", "500"])
        .args(["--timeout-ms", "4000"])
        .args(extra)
        .output()
        .expect("run serial-nexus-sim wire");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse wire verdict: {e}; stdout={:?} stderr={:?}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Bounded wait for the `downlink` leg to be `faulted` with a reason containing
/// `substr` (§9 clause 6: the refusal reason is surfaced in state).
fn faulted_with(rpc: &Rpc, substr: &str) -> bool {
    wait_until(Duration::from_secs(5), || {
        let Some(n) = rpc.node("downlink") else {
            return false;
        };
        n.get("status").and_then(Value::as_str) == Some("faulted")
            && n.get("reason")
                .and_then(Value::as_str)
                .map(|r| r.contains(substr))
                .unwrap_or(false)
    })
}

/// The `downlink` leg's config: `faces = host`, listen role, unix transport, one
/// channel. A leg needs no serial device, so this shape runs everywhere.
fn leg_config(address: &str) -> String {
    format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{address}"
channels = ["console"]
"#,
    )
}

/// Bounded wait for a conforming peer to have bound the leg (§7.4 self-heal).
fn healed(rpc: &Rpc) -> bool {
    wait_until(Duration::from_secs(5), || {
        let Some(n) = rpc.node("downlink") else {
            return false;
        };
        n.get("status").and_then(Value::as_str) == Some("active")
            && n.pointer("/channels/console/binding")
                .and_then(Value::as_str)
                == Some("bound")
    })
}

/// Drive one hostile case: the sim elicits a clean refusal (`peer_closed`), and the
/// leg's status must go `faulted` with the given reason substring.
fn hostile_case(rpc: &Rpc, address: &str, reason_sub: &str, extra: &[&str]) {
    let verdict = run_wire(address, extra);
    assert_eq!(
        verdict.get("pass").and_then(Value::as_bool),
        Some(true),
        "sim did not report a clean refusal for {extra:?}: {verdict}"
    );
    assert_eq!(
        verdict.get("peer_closed").and_then(Value::as_bool),
        Some(true),
        "daemon did not close the connection for {extra:?}: {verdict}"
    );
    assert!(
        faulted_with(rpc, reason_sub),
        "leg not faulted with reason matching /{reason_sub}/ for {extra:?}: {:?}",
        rpc.node("downlink")
    );
}

#[test]
fn wire_hostility_faults_cleanly_then_leg_heals() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let leg_sock = d.run().join("leg.sock");
    let leg = leg_sock.to_string_lossy().into_owned();

    rpc.load_toml(&leg_config(&leg), false)
        .expect("load leg graph");

    // 1. Version mismatch: the version must appear in the fault reason.
    hostile_case(rpc, &leg, "999", &["--hello-version", "999"]);
    // 2. Bad magic: a not-our-protocol peer.
    hostile_case(rpc, &leg, "magic", &["--bad-magic"]);
    // 3. Oversize frame (§9 clause 4): refused on the length prefix.
    hostile_case(rpc, &leg, "exceeds", &["--oversize-frame"]);
    // 4. Unknown frame type (§9 clause 6).
    hostile_case(rpc, &leg, "unknown frame type", &["--unknown-type"]);

    // Heal: after all that hostility, a conforming peer binds cleanly
    // (faulted-and-wait self-heals, §7.4). Hold the connection open while we inspect.
    let _good = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &leg,
            "--announce",
            "console",
            "--hold-ms",
            "2500",
            "--timeout-ms",
            "3500",
        ],
        None,
    );
    assert!(
        healed(rpc),
        "leg did not heal for a conforming peer: {:?}",
        rpc.node("downlink")
    );
}

/// How long the mute/trickling peer is willing to hold the connection open.
///
/// The daemon's overall handshake deadline is 5 s (`leg.rs`'s `HANDSHAKE_TIMEOUT`), and
/// this window has to outlast it by enough that a loaded runner cannot make the peer
/// the one who gave up first — which would prove nothing. It is an upper bound, not a
/// cost: the sim returns the instant the daemon hangs up, so the test runs in about the
/// deadline, not this.
const MUTE_MS: &str = "12000";

/// §15.24 — **a peer that never finishes saying hello cannot wedge a listener**, and
/// the listener heals.
///
/// The four hostilities above are all *frames*: the leg reads a complete hello, judges
/// it, and refuses. This is the case where nothing ever completes, and `extra` picks
/// which shape of it — total silence, or a hello dribbled a byte at a time with the
/// last one withheld forever. The trickle is the harder one: every individual read
/// stays productive, so only an *overall* deadline can end it.
///
/// Nothing in the tree exercised that arm (37-LEG-4) — the sim had no way to be quiet —
/// so the timeout branch could have been deleted with the suite green, and the listener
/// would then have hung on the first such peer. Permanently, and not only for that
/// peer: the reject-extras arm runs only *after* the handshake, so everyone behind it
/// waits too. That last part is what the heal at the end actually proves.
///
/// Ground truth is the structured `state` snapshot plus the sim's own verdict, whose
/// `peer_closed` says the daemon hung up inside the window rather than the sim giving
/// up on the daemon.
fn handshake_deadline_case(extra: &[&str]) {
    let d = Daemon::start();
    let rpc = d.rpc();
    let leg_sock = d.run().join("leg.sock");
    let leg = leg_sock.to_string_lossy().into_owned();
    rpc.load_toml(&leg_config(&leg), false)
        .expect("load leg graph");

    let verdict = run_wire(&leg, extra);
    assert_eq!(
        verdict.get("peer_closed").and_then(Value::as_bool),
        Some(true),
        "the leg never hung up on a peer that never finished its hello — the overall \
         handshake deadline did not fire (§15.24) for {extra:?}: {verdict}"
    );
    assert!(
        verdict
            .get("hello_bytes_withheld")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0,
        "the peer completed its hello, so this proves nothing about the deadline: \
         {verdict}"
    );
    assert!(
        faulted_with(rpc, "handshake deadline"),
        "leg not faulted with the handshake-deadline reason for {extra:?}: {:?}",
        rpc.node("downlink")
    );

    // The half that matters operationally: the listener is still a listener.
    let _good = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &leg,
            "--announce",
            "console",
            "--hold-ms",
            "2500",
            "--timeout-ms",
            "3500",
        ],
        None,
    );
    assert!(
        healed(rpc),
        "leg did not accept a conforming peer after the handshake deadline fired for \
         {extra:?}: {:?}",
        rpc.node("downlink")
    );
}

/// The silent shape: the connection opens and no hello ever arrives.
#[test]
fn a_mute_peer_trips_the_handshake_deadline_and_the_leg_heals() {
    handshake_deadline_case(&["--mute-ms", MUTE_MS]);
}

/// The trickling shape §15.24 names by name: a hello that arrives forever.
#[test]
fn a_trickling_peer_trips_the_handshake_deadline_and_the_leg_heals() {
    handshake_deadline_case(&["--mute-ms", MUTE_MS, "--trickle-hello"]);
}
