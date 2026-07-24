//! Phase 6 wire hostility, ported from `scripts/validate/phase6/hostility.sh`
//! (design §9 clause-4/6 clean-refusal contract, §7.4 leg self-heal).
//!
//! A receiving leg (`faces = host`, `role = listen`, unix transport, one channel
//! `console`) is driven by `nexus-sim wire` acting as a hostile-or-conforming peer.
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
//! A leg needs no serial *device* (the transport is a loopback unix socket), so this
//! test runs on every platform. Ground truth is the structured `state` snapshot
//! (`.status` / `.reason` / `.channels.console.binding`) and the sim's own JSON
//! verdict (`.pass` / `.peer_closed`) — never parsed CLI text.

use std::process::Command;
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, bin, wait_until};
use serde_json::Value;

/// Run `nexus-sim wire` in the foreground to completion against `address`,
/// announcing `console`, with the hostility flags in `extra`. Returns its parsed
/// JSON verdict (the sim prints the verdict to stdout, §Phase 6 wire mode).
fn run_wire(address: &str, extra: &[&str]) -> Value {
    let out = Command::new(bin("nexus-sim"))
        .arg("wire")
        .args(["--transport", "unix"])
        .args(["--address", address])
        .args(["--announce", "console"])
        .args(["--hold-ms", "500"])
        .args(["--timeout-ms", "4000"])
        .args(extra)
        .output()
        .expect("run nexus-sim wire");
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

    // A receiving leg: faces=host, listen role, unix transport, one channel.
    let cfg = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "{leg}"
channels = ["console"]
"#,
    );
    rpc.load_toml(&cfg, false).expect("load leg graph");

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
    let healed = wait_until(Duration::from_secs(5), || {
        let Some(n) = rpc.node("downlink") else {
            return false;
        };
        n.get("status").and_then(Value::as_str) == Some("active")
            && n.pointer("/channels/console/binding")
                .and_then(Value::as_str)
                == Some("bound")
    });
    assert!(
        healed,
        "leg did not heal for a conforming peer: {:?}",
        rpc.node("downlink")
    );
}
