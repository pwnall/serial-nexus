//! Phase 6 leg head-of-line slice, ported from
//! `scripts/validate/phase6/head-of-line.sh` (design §9 whole-connection flow
//! control, §15.22 direction independence).
//!
//! The v1 wire has whole-connection (not per-channel) targetward flow control: when
//! the peer stops reading, *every* channel's targetward freezes together. A
//! `nexus-sim wire --stall` peer streams sustained hostward on all channels but never
//! reads its socket, so the daemon leg's targetward backs up into the socket buffer.
//! This test pins two properties:
//!
//! * (a) §15.22 direction independence — a c2 hostward drain keeps advancing while c0
//!   and c1 targetward are wedged, because the leg's two socket directions are
//!   concurrently polled.
//! * (b) a fully-stalled peer freezes every targetward channel together — the
//!   connection's targetward flowed (sum > 0), then froze with neither channel
//!   completing its 2 MiB. We assert the SUM (not each channel), because under a
//!   fully-stalled peer whichever channel wins the race wedges the shared socket, so
//!   the other can legitimately sit at 0 — itself a head-of-line manifestation.
//!
//! Needs no serial *device* (a unix-socket leg fanning out to local PTYs), so it runs
//! on every platform — the portable replacement for the `jq`/`sleep`-riddled bash.
//!
//! Faithful-port deviations (each preserves the original assertions):
//! * The bash's implicit "load returned, so the leg is bound" race is closed by an
//!   explicit bounded wait for the leg's listen socket to appear before the single-shot
//!   `nexus-sim wire` peer dials it (its `SimStream::connect` does not retry).
//! * The two 1.5 s samples are fixed dwells (`thread::sleep`): proving a counter is
//!   *frozen* is asserting non-progress over a window, which has no condition to poll —
//!   this is the one place the "no bare sleeps" convention cannot apply.

use std::time::Duration;

use nexus_itest::{Daemon, Sim, wait_until};
use serde_json::Value;

/// The full targetward burst each writer attempts; the stall must block both below it.
const MIB2: u64 = 2 * 1024 * 1024;

/// A leg channel's observed counter (`accepted_targetward` / `delivered_hostward`)
/// from the `downlink` node object, or 0 if absent. Leg `state_extra` nests these
/// under `.channels.<id>.<field>` (see `nexus-daemon/src/nodes/leg.rs`).
fn chan(node: &Value, ch: &str, field: &str) -> u64 {
    node.get("channels")
        .and_then(|c| c.get(ch))
        .and_then(|c| c.get(field))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Whether the `downlink` leg reports `connection == "connected"`.
fn connected(node: &Value) -> bool {
    node.get("connection").and_then(Value::as_str) == Some("connected")
}

#[test]
fn whole_connection_head_of_line_freezes_targetward_hostward_flows() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();

    let leg = run.join("leg.sock");
    let p0 = run.join("p0");
    let p1 = run.join("p1");
    let p2 = run.join("p2");
    let (leg_s, p0_s, p1_s, p2_s) = (
        leg.to_string_lossy().into_owned(),
        p0.to_string_lossy().into_owned(),
        p1.to_string_lossy().into_owned(),
        p2.to_string_lossy().into_owned(),
    );

    // A receiving leg (faces=host, listen) with three channels fanning out to local
    // PTYs; each edge writes on-demand.
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
channels = ["c0", "c1", "c2"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[node]]
type = "pty"
name = "p1"
path = "{p1}"
[[node]]
type = "pty"
name = "p2"
path = "{p2}"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
[[edge]]
a = "downlink/c1"
b = "p1"
write_mode = "on-demand"
[[edge]]
a = "downlink/c2"
b = "p2"
write_mode = "on-demand"
"#,
        leg = leg.display(),
        p0 = p0.display(),
        p1 = p1.display(),
        p2 = p2.display(),
    );
    rpc.load_toml(&cfg, false).expect("load head-of-line graph");

    // The PTY fan-out symlinks must exist before a `client` can open them.
    for p in [&p0, &p1, &p2] {
        assert!(
            wait_until(Duration::from_secs(5), || p.exists()),
            "pty symlink {} never appeared",
            p.display()
        );
    }
    // The listen leg binds its unix socket asynchronously; wait for it so the
    // single-shot wire peer's connect (no retry) lands.
    assert!(
        wait_until(Duration::from_secs(5), || leg.exists()),
        "leg listen socket never appeared at {}",
        leg.display()
    );

    // The stalled peer: streams sustained hostward on all channels, never reads.
    let _wire = Sim::spawn(
        &[
            "wire",
            "--transport",
            "unix",
            "--address",
            &leg_s,
            "--announce",
            "c0",
            "--announce",
            "c1",
            "--announce",
            "c2",
            "--stall",
            "--hold-ms",
            "8000",
            "--timeout-ms",
            "10000",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(5), || rpc
            .node("downlink")
            .map(|n| connected(&n))
            .unwrap_or(false)),
        "leg never connected: {:?}",
        rpc.node("downlink")
    );

    // Drain c2's hostward, and confirm it flows despite the peer stalling reads.
    let _drain = Sim::spawn(
        &[
            "client",
            "--path",
            &p2_s,
            "--drain",
            "--quiet-ms",
            "20000",
            "--timeout-ms",
            "30000",
        ],
        None,
    );
    assert!(
        wait_until(Duration::from_secs(8), || rpc
            .node("downlink")
            .map(|n| chan(&n, "c2", "delivered_hostward") > 32768)
            .unwrap_or(false)),
        "c2 hostward never started flowing while the peer stalled reads: {:?}",
        rpc.node("downlink")
    );

    // Two operators write a large targetward burst on c0 and c1. The peer never reads,
    // so the socket send buffer fills and the leg's SEND wedges — both channels'
    // targetward freeze together.
    let _w0 = Sim::spawn(
        &[
            "client",
            "--path",
            &p0_s,
            "--send",
            "seeded:2MiB",
            "--seed",
            "10",
            "--timeout-ms",
            "20000",
        ],
        None,
    );
    let _w1 = Sim::spawn(
        &[
            "client",
            "--path",
            &p1_s,
            "--send",
            "seeded:2MiB",
            "--seed",
            "20",
            "--timeout-ms",
            "20000",
        ],
        None,
    );

    // Sample the frozen targetward and the still-advancing hostward, 1.5 s apart. A
    // fixed dwell: "frozen" is a non-progress assertion over a window (see module doc).
    std::thread::sleep(Duration::from_millis(1500));
    let a = rpc
        .node("downlink")
        .expect("downlink node present (sample a)");
    let s0a = chan(&a, "c0", "accepted_targetward");
    let s1a = chan(&a, "c1", "accepted_targetward");
    let h2a = chan(&a, "c2", "delivered_hostward");

    std::thread::sleep(Duration::from_millis(1500));
    let b = rpc
        .node("downlink")
        .expect("downlink node present (sample b)");
    let s0b = chan(&b, "c0", "accepted_targetward");
    let s1b = chan(&b, "c1", "accepted_targetward");
    let h2b = chan(&b, "c2", "delivered_hostward");

    // Targetward flowed at the connection level (a positive lower bound on the total,
    // so "frozen" cannot be satisfied by a totally-broken targetward path that moved
    // zero bytes). Assert the SUM, not each channel: whichever channel wins the race
    // wedges the shared socket, so the other can legitimately sit at 0.
    assert!(
        s0a + s1a > 0,
        "targetward never flowed at all (path broken, not wedged): c0={s0a} c1={s1a}"
    );

    // Both channels' targetward froze together (no progress over the interval), and
    // neither reached its full 2 MiB — the whole-connection head-of-line stall.
    assert_eq!(
        s0a, s0b,
        "c0 targetward did not freeze (still advancing): {s0a} -> {s0b}"
    );
    assert_eq!(
        s1a, s1b,
        "c1 targetward did not freeze (still advancing): {s1a} -> {s1b}"
    );
    assert!(
        s0b < MIB2 && s1b < MIB2,
        "targetward should be blocked below the full 2 MiB: c0={s0b} c1={s1b}"
    );

    // Hostward kept advancing across the same interval — the two socket directions are
    // independent (the §9/§15.22 property this test pins).
    assert!(
        h2b > h2a,
        "hostward did not keep advancing during the targetward freeze: {h2a} -> {h2b}"
    );

    // It is a stall, not a disconnect: the leg is still connected.
    let fin = rpc.node("downlink").expect("downlink node present (final)");
    assert!(
        connected(&fin),
        "the leg should stay connected during head-of-line blocking: {fin:?}"
    );
}
