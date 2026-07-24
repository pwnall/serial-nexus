//! Phase 5 held-lock slice, ported from `scripts/validate/phase5/held.sh`
//! (design §6 arbitration / held origins, §7.5 the demux codec).
//!
//! A demultiplexer codec holds its serial's write lock **permanently** (its edge is
//! `write_mode = "held"`), because any other writer would corrupt the mux framing.
//! Five properties follow:
//!
//!   1. the codec (origin `mux`) is the serial's lock holder immediately after load;
//!   2. a raw `lock`/`send` at the serial is refused with the `LOCKED` app error
//!      while the codec holds it;
//!   3. `lock --steal` transfers the lock to a raw writer (`rawpty`) and records the
//!      theft in state (`from = mux`, `by = rawpty`);
//!   4. while the codec is ousted, a channel writer's targetward bytes **park** in
//!      the codec — its `accepted_targetward` counter freezes at 0 (the §6 stall);
//!   5. once the steal is released the codec re-acquires (FIFO) and forwards every
//!      parked byte to completion — `accepted_targetward` resumes to the full burst
//!      (commands delayed, never dropped, §6).
//!
//! ## Portability — no serial *device* required (runs on every platform)
//!
//! The serial node comes up `waiting` on an absent device path, but everything this
//! test asserts is wired from *config* at graph-wire time, independent of device
//! readiness: the `held` origin acquires the serial's write lock the instant it is
//! registered, and the codec's targetward path (its clone of the serial's targetward
//! sender, feeding the `CHANNEL_CAP`-deep channel) exists regardless. Crucially, the
//! serial's reconnect supervisor **owns that targetward receiver for the node's whole
//! life** (`nodes/serial.rs`: the `waiting` branch keeps `ctx.targetward` alive
//! without draining it), so the channel never closes and the codec's
//! `accepted_targetward` handoff reaches the full 4 KiB burst whether or not a device
//! is draining the frames. A 4 KiB write is one 64 KiB-read chunk → one reference
//! frame, far under the 256-chunk channel, so parking it needs no drain. The codec +
//! lock arbitration is what is under test, so this uses an absent device and runs
//! everywhere — the same reasoning `p4_steal_lease` used for the lock graph. (The
//! bash stood a `nexus-sim pty --sink` behind the serial to drain the frames; that
//! is a device the demultiplexer's data path does not need for these assertions.)

use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, wait_until};
use serde_json::{Value, json};

/// The channel writer's bounded burst, in bytes (the bash's `NBYTES`).
const NBYTES: u64 = 4096;

/// The `LOCKED` application error code (`nexus_rpc::AppError::Locked`):
/// `APP_ERROR_BASE (-32000) - 3`. A contended `lock`/`send` is refused with it (§6).
const LOCKED: i64 = -32003;

/// The lock's `holder` origin reported on `usb0` in `state` — a JSON string origin
/// name, or `Value::Null` when the lock is free (the serial is a single host-facing
/// endpoint, so its lock is at `.lock`, §6).
fn holder(rpc: &Rpc) -> Value {
    rpc.node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("holder").cloned())
        .unwrap_or(Value::Null)
}

/// The demux channel `c0`'s targetward acceptance counter — the device-write handoff
/// count that freezes while the codec does not hold the serial lock (§6/§7.5).
fn accepted(rpc: &Rpc) -> u64 {
    rpc.node("mux")
        .as_ref()
        .and_then(|n| n.pointer("/channels/c0/accepted_targetward"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Boot a daemon and load the held-lock graph: a `waiting` serial `usb0` (device
/// absent) with two origins — the demux codec `mux` (edge `held`) and a raw on-demand
/// writer `rawpty` — plus a pty console `con-c0` on the codec's free-for-all channel
/// `c0`. Returns once the demux is confirmed to hold the serial's write lock.
fn held_graph_daemon() -> Daemon {
    let d = Daemon::start();
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con-c0"
path = "{con}"
[[node]]
type = "pty"
name = "rawpty"
path = "{raw}"
[[edge]]
a = "usb0"
b = "mux"
write_mode = "held"
[[edge]]
a = "usb0"
b = "rawpty"
[[edge]]
a = "mux/c0"
b = "con-c0"
"#,
        dev = d.run().join("absent-device").display(),
        con = d.run().join("tty-c0").display(),
        raw = d.run().join("ttyW").display(),
    );
    d.rpc()
        .load_toml(&cfg, false)
        .expect("load held-lock graph");

    // The demux holds the serial's write lock (§6 held origin: it acquires at
    // register time, independent of the device being present).
    assert!(
        wait_until(Duration::from_secs(5), || holder(d.rpc()) == json!("mux")),
        "demux did not hold the serial lock: {:?}",
        d.rpc().node("usb0")
    );
    d
}

#[test]
fn held_lock_holds_and_refuses_raw_contention() {
    let d = held_graph_daemon();
    let rpc = d.rpc();

    // The holder is the codec's mux origin (§6 held origin).
    assert_eq!(
        holder(rpc),
        json!("mux"),
        "the demux must hold the serial lock"
    );

    // A plain (un-stolen) `lock` at the serial is refused while the demux holds it.
    let err = rpc
        .lock("rawpty", false, false, None)
        .expect_err("plain lock rawpty should be refused while the demux holds usb0");
    assert_eq!(
        err.code, LOCKED,
        "lock rawpty was refused, but not with the LOCKED error: [{}] {}",
        err.code, err.message
    );

    // A plain `send` at the serial is refused the same way: it self-acquires the
    // held lock, cannot, and times out with LOCKED (§6).
    let err = rpc
        .send("usb0", "raw", false, 500)
        .expect_err("plain send usb0 should be refused while the demux holds it");
    assert_eq!(
        err.code, LOCKED,
        "send usb0 was refused, but not with the LOCKED error: [{}] {}",
        err.code, err.message
    );
}

#[test]
fn stealing_the_held_lock_stalls_the_channel_then_resumes() {
    let d = held_graph_daemon();
    let rpc = d.rpc();

    // Steal the serial lock durably for the raw writer — the demux is ousted (§6).
    let steal = rpc
        .lock("rawpty", true, false, None)
        .expect("lock rawpty --steal");
    assert_eq!(
        steal["acquired"],
        json!(true),
        "lock rawpty --steal did not report acquired: {steal}"
    );
    assert_eq!(holder(rpc), json!("rawpty"), "rawpty did not take the lock");
    let last_steal = rpc
        .node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("last_steal").cloned())
        .unwrap_or(Value::Null);
    assert_eq!(
        last_steal,
        json!({ "from": "mux", "by": "rawpty" }),
        "state did not record the steal (from mux, by rawpty)"
    );

    // Before opening the pty slave, the con-c0 console must be active and its symlink
    // installed (the client below opens it as its `--path`).
    assert!(
        rpc.wait_status("con-c0", "active", Duration::from_secs(5)),
        "con-c0 pty not active: {:?}",
        rpc.node("con-c0")
    );
    let ttyc0 = d.run().join("tty-c0");
    assert!(
        wait_until(Duration::from_secs(5), || ttyc0.exists()),
        "con-c0 pty symlink never appeared"
    );

    // A bounded burst on the free-for-all channel, held open across the stall. The
    // client runs in the background (killed on drop) — with the lock stolen its bytes
    // must park in the codec, not flow to the device.
    let path = ttyc0.to_string_lossy().into_owned();
    let send = format!("seeded:{NBYTES}");
    let _client = Sim::spawn(
        &[
            "client",
            "--path",
            &path,
            "--send",
            &send,
            "--seed",
            "3",
            "--hold-ms",
            "30000",
            "--timeout-ms",
            "40000",
        ],
        None,
    );

    // The channel writer becomes present (its client holds the pty slave open).
    let present = wait_until(Duration::from_secs(8), || {
        rpc.node("con-c0")
            .and_then(|n| n.get("client_present").and_then(Value::as_bool))
            == Some(true)
    });
    assert!(
        present,
        "channel writer never became present: {:?}",
        rpc.node("con-c0")
    );

    // With the demux ousted, the channel writer's bytes park in the codec:
    // accepted_targetward must stay frozen at 0 (the §6 stall). Over the window it
    // must never advance (the final resume assertion confirms the bytes did arrive
    // and were parked, i.e. delayed rather than dropped).
    let advanced = wait_until(Duration::from_millis(1500), || accepted(rpc) > 0);
    assert!(
        !advanced,
        "accepted advanced while the lock was stolen (the stall was not observed): got {}",
        accepted(rpc)
    );

    // Release the theft: the demux re-acquires (FIFO) and forwards every parked byte
    // to completion — delayed, never dropped.
    rpc.unlock("rawpty").expect("unlock rawpty");
    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc) == json!("mux")),
        "demux did not re-acquire the lock after the theft: {:?}",
        rpc.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || accepted(rpc) == NBYTES),
        "accepted did not resume to {NBYTES} after the theft ended (data wedged or lost): got {}",
        accepted(rpc)
    );
}
