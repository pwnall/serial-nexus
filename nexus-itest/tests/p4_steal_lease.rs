//! Phase 4 arbitration: steal + lease, ported from
//! `scripts/validate/phase4/steal-lease.sh` (design §6 arbitration, §10
//! notifications, §15.20 two-lane control plane).
//!
//! Four properties of the write lock:
//!   1. `lock --steal` transfers the lock, records the theft in state, and emits an
//!      IMMEDIATE id-less `lock` notification (event-driven, distinct from the 200 ms
//!      periodic `state` snapshot).
//!   2. an expired `--lease-ms` auto-releases a silent holder within the bound.
//!   3. a stale lease timer NEVER fires across grants: unlock, re-lock (lease-free),
//!      let the old timer elapse — the new grant survives (generation-guarded, §6).
//!   4. re-arming a lease EXTENDS it: the earlier, shorter timer is invalidated.
//!
//! Needs no serial *device*: the lock is structural (created at graph-wire time from
//! config, independent of device readiness) and the script asserts no bytes. So where
//! the bash stood a `nexus-sim` sink behind the serial node, this uses an ABSENT
//! device path (the serial node parks in `waiting`) and the pty origins still attach
//! to its endpoint lock — the whole suite runs on every platform.

use std::time::Duration;

use nexus_itest::{Daemon, Rpc, wait_until};
use serde_json::{Value, json};

/// Boot a daemon and load the steal/lease graph: two pty origins (`ptya`, `ptyb`)
/// writing toward one serial endpoint (`usb0`, device absent → `waiting`). The lock
/// lives on `usb0`; `ptya`/`ptyb` are its two arbitration origins.
fn lock_graph_daemon() -> Daemon {
    let d = Daemon::start();
    {
        let run = d.run();
        let cfg = format!(
            r#"
[[node]]
type = "pty"
name = "ptya"
path = "{ptya}"
[[node]]
type = "pty"
name = "ptyb"
path = "{ptyb}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
"#,
            ptya = run.join("ttyA").display(),
            ptyb = run.join("ttyB").display(),
            dev = run.join("absent-device").display(),
        );
        d.rpc().load_toml(&cfg, false).expect("load lock graph");
    }
    d
}

/// The lock holder reported on `usb0` in `state` — a JSON string origin name, or
/// `Value::Null` when the lock is free.
fn holder(rpc: &Rpc) -> Value {
    rpc.node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("holder").cloned())
        .unwrap_or(Value::Null)
}

#[test]
fn steal_transfers_records_and_notifies_immediately() {
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    // ptya takes the lock.
    let acq = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(acq["acquired"], json!(true), "lock ptya did not acquire");

    // Subscribe, then prove the subscription is LIVE by waiting for a periodic
    // `state` snapshot (which only flows once a receiver is registered) — a bounded
    // liveness proof, not a bare sleep — before triggering the steal, so the
    // immediate `lock` notification cannot be missed.
    let mut sub = rpc.subscribe();
    let live = sub
        .wait_for(Duration::from_secs(5), |n| {
            n.get("method").and_then(Value::as_str) == Some("state")
        })
        .is_some();
    assert!(live, "subscription never registered (no `state` snapshot)");

    // ptyb steals: reports acquired + who it stole from.
    let steal = rpc.lock("ptyb", true, false, None).expect("steal for ptyb");
    assert_eq!(
        steal["acquired"],
        json!(true),
        "steal did not report acquired"
    );
    assert_eq!(
        steal["stole_from"],
        json!("ptya"),
        "steal did not report stole_from=ptya"
    );

    // The holder is now ptyb, and state records the steal so the ousted holder sees it.
    assert_eq!(holder(rpc), json!("ptyb"), "holder not ptyb after steal");
    let last_steal = rpc
        .node("usb0")
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("last_steal").cloned())
        .unwrap_or(Value::Null);
    assert_eq!(
        last_steal,
        json!({ "from": "ptya", "by": "ptyb" }),
        "state did not record the steal (from ptya, by ptyb)"
    );

    // An IMMEDIATE `lock` notification (method=="lock", not the "state" snapshot)
    // must arrive carrying holder=ptyb — the event-driven transition (§10).
    let note = sub.wait_for(Duration::from_secs(3), |n| {
        n.get("method").and_then(Value::as_str) == Some("lock")
            && n.pointer("/params/lock/holder").and_then(Value::as_str) == Some("ptyb")
    });
    assert!(
        note.is_some(),
        "no immediate `lock` notification carrying holder=ptyb after the steal"
    );

    rpc.unlock("ptyb").expect("unlock ptyb");
}

#[test]
fn expired_lease_releases_a_silent_holder_within_the_bound() {
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    let acq = rpc
        .lock("ptya", false, false, Some(300))
        .expect("lease-lock ptya");
    assert_eq!(acq["acquired"], json!(true), "lease-lock did not acquire");
    assert_eq!(
        holder(rpc),
        json!("ptya"),
        "ptya should hold immediately after a lease grant"
    );

    // Within a generous bound (the lease is 300 ms), the holder auto-releases.
    let released = wait_until(Duration::from_secs(3), || holder(rpc) == Value::Null);
    assert!(
        released,
        "lease did not auto-release the holder within the bound"
    );
}

#[test]
fn stale_lease_timer_never_fires_across_grants() {
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    // Arm a 400 ms lease, then release it before the lease fires, then re-lock plain
    // (lease-free). The stale 400 ms timer from the released grant must NOT release
    // this new grant (generation guard, §6).
    rpc.lock("ptya", false, false, Some(400))
        .expect("lease-lock (arm)");
    rpc.unlock("ptya").expect("unlock before lease fires");
    let relock = rpc.lock("ptya", false, false, None).expect("re-lock");
    assert_eq!(relock["acquired"], json!(true), "re-lock did not acquire");

    // Across a 700 ms window that outlives the old 400 ms lease, the holder must stay
    // ptya continuously: if the stale timer wrongly fired, holder would flip to null.
    // `wait_until` becoming true means a flip was observed — it must NOT.
    let flipped = wait_until(Duration::from_millis(700), || holder(rpc) != json!("ptya"));
    assert!(
        !flipped,
        "a stale lease timer released a later grant (holder became {:?})",
        holder(rpc)
    );

    rpc.unlock("ptya").expect("unlock ptya");
}

#[test]
fn re_arming_a_lease_extends_it() {
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    // Arm a 400 ms lease, then re-arm to a much longer 4000 ms lease well before the
    // original elapses. The renewal bumps the grant generation, invalidating the
    // first (400 ms) timer.
    rpc.lock("ptya", false, false, Some(400))
        .expect("lease-lock (arm)");
    let rearm = rpc
        .lock("ptya", false, false, Some(4000))
        .expect("lease re-arm");
    assert_eq!(
        rearm["held"],
        json!(true),
        "lease re-arm did not report held"
    );

    // Across the ORIGINAL 400 ms deadline (a 700 ms window) the holder must NOT be
    // released — the renewal won.
    let flipped = wait_until(Duration::from_millis(700), || holder(rpc) != json!("ptya"));
    assert!(
        !flipped,
        "lease renewal did not extend (holder released at the original deadline: {:?})",
        holder(rpc)
    );

    rpc.unlock("ptya").expect("unlock ptya");
}
