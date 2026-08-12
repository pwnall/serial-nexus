#![forbid(unsafe_code)]

//! Phase 4 arbitration: steal + lease, ported from
//! `scripts/validate/phase4/steal-lease.sh` (design §6 arbitration, §10
//! notifications, §15.20 two-lane control plane).
//!
//! Six properties of the write lock:
//!   1. `lock --steal` transfers the lock, records the theft in state, and emits an
//!      IMMEDIATE id-less `lock` notification (event-driven, distinct from the 200 ms
//!      periodic `state` snapshot).
//!   2. an expired `--lease-ms` auto-releases a silent holder within the bound.
//!   3. a stale lease timer NEVER fires across grants: unlock, re-lock (lease-free),
//!      let the old timer elapse — the new grant survives (generation-guarded, §6).
//!   4. re-arming a lease EXTENDS it: the earlier, shorter timer is invalidated.
//!   5. a `send --steal`'s theft is recorded too, and survives its transient origin's
//!      departure — §6's promise is about the *steal*, not about the stealer being
//!      still around to be asked (37-LOCK-3).
//!   6. `--lease-ms` is range-checked before the grant, like every other daemon-side
//!      timer input (§15.34/§16.12, 37-LOCK-2).
//!
//! Needs no serial *device*: the lock is structural (created at graph-wire time from
//! config, independent of device readiness) and the script asserts no bytes. So where
//! the bash stood a `serial-nexus-sim` sink behind the serial node, this uses an ABSENT
//! device path (the serial node parks in `waiting`) and the pty origins still attach
//! to its endpoint lock — the whole suite runs on every platform.

use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, wait_until};

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

/// The `lock.last_steal` record reported on `usb0` (§6), or `Value::Null` when no
/// steal is on record.
fn last_steal(rpc: &Rpc) -> Value {
    rpc.node("usb0")
        .and_then(|n| n.pointer("/lock/last_steal").cloned())
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
    assert_eq!(
        last_steal(rpc),
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

/// **37-LOCK-3.** A `send --steal` is recorded in state exactly as a `lock --steal`
/// is, and the record outlives the transient origin that made it.
///
/// `send` registers an origin labelled `send`, steals, writes and unregisters inside
/// one verb (§6) — so a record that resolved its parties' ids when `state` was asked
/// had already lost one of them by the time the reply was serialized. The asymmetry
/// was invisible: `lock --steal` (test 1 above) recorded fine, and §6's promise that a
/// steal "is recorded in state so the previous holder can see what happened" silently
/// covered one of its two spellings. The ousted holder is the party with no other way
/// to learn what happened, which is why this is a guard and not a nicety.
#[test]
fn a_send_steal_is_recorded_in_state_after_its_transient_origin_leaves() {
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        holder(rpc),
        json!("ptya"),
        "ptya should hold before the theft"
    );

    // `usb0`'s device is absent, so the node parks in `waiting` and its supervisor
    // holds the targetward receiver across the outage — the line lands in the
    // endpoint's channel rather than on a wire, which is all this needs (§7.1).
    let sent = rpc
        .send("usb0", "steal-me", true, 5_000)
        .expect("send --steal must take the lock and deliver");
    assert_eq!(
        sent["delivered"],
        json!(true),
        "the stealing send did not deliver: {sent}"
    );

    // The verb has returned, so its origin is already unregistered and the lock free.
    assert_eq!(
        holder(rpc),
        Value::Null,
        "the transient origin did not release on its way out"
    );
    assert_eq!(
        last_steal(rpc),
        json!({ "from": "ptya", "by": "send" }),
        "state lost the send-steal record when its transient origin left; ptya has no \
         other way to learn its lock was taken (§6)"
    );

    // And it stays readable after the *victim* leaves too — the moment an operator
    // investigating a wedged console actually asks.
    rpc.remove_node("ptya", true)
        .expect("remove-node ptya --cascade");
    assert_eq!(
        last_steal(rpc),
        json!({ "from": "ptya", "by": "send" }),
        "the record went with the victim's registration"
    );
}

/// **37-LOCK-2.** `--lease-ms` is range-checked at parse, against the same
/// `MAX_TIMER_MS` ceiling every other daemon-side timer input carries
/// (§15.34/§16.12, invariant 13).
///
/// It was the one input outside that rule: a raw `u64` handed straight to a sleep
/// task, so `--lease-ms 86400000` ("a day", the value the leg's own cap was written
/// against) armed a release a day out on an endpoint the operator believed leased,
/// and `u64::MAX` is the monotonic-clock overflow `send`'s deadline already clamps
/// for. The refusal must land **before** the grant: a lease refused after the lock
/// changed hands would be the worst of both.
#[test]
fn an_out_of_range_lease_is_refused_before_the_grant() {
    const MAX_TIMER_MS: u64 = 3_600_000; // serial_nexus_core::config::MAX_TIMER_MS
    let d = lock_graph_daemon();
    let rpc = d.rpc();

    for ms in [MAX_TIMER_MS + 1, u64::MAX] {
        let err = rpc
            .lock("ptya", false, false, Some(ms))
            .expect_err("an out-of-range lease must be refused");
        assert_eq!(err.code, -32602, "wrong code for {ms}: {}", err.message);
        assert!(
            err.message.contains("lease_ms") && err.message.contains(&MAX_TIMER_MS.to_string()),
            "the refusal must name the field and the ceiling: {}",
            err.message
        );
        assert_eq!(
            holder(rpc),
            Value::Null,
            "the refused lease granted the lock anyway (ms = {ms})"
        );
    }

    // The ceiling itself is legal — the check is a maximum, not an exclusive bound.
    rpc.lock("ptya", false, false, Some(MAX_TIMER_MS))
        .expect("a lease at exactly the maximum is accepted");
    assert_eq!(holder(rpc), json!("ptya"));
    rpc.unlock("ptya").expect("unlock ptya");
}
