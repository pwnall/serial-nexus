//! Phase 6 leg-outage slice, ported from `scripts/validate/phase6/outage.sh`
//! (design §7.4 leg lifecycle, §9 wire, §6 arbitration). A `nexus-sim tcp-proxy`
//! between two daemons severs the link mid-stream (`--drop-after`) and restores it
//! (`--restore-after-ms`). During the gap the leg is faulted-and-wait — targetward
//! writers pause (backpressure, no drop). After restore the connect-role leg
//! reconnects, purge-on-reconnect discards the outage-era targetward backlog with a
//! counter (§7.4), and a fresh round-trip is byte-clean.
//!
//! Reconnect *also* releases the outage-era **hostward** backlog, which the purge
//! deliberately does not touch (§6's purge is targetward-only) — so the final
//! round-trip is gated on the console going quiet first. See step 6a: the barrier is
//! load-bearing, and the failure it removes is one CPU load hides rather than causes.
//!
//! Topology (two daemons, one leg each, a serial echo device behind daemon A):
//!
//! ```text
//!   client → pty p0 ──(targetward)──▶ downlink/c0 (leg, faces=host, listen)
//!                                          │
//!                                    tcp-proxy (severs after 8KiB of A's
//!                                          │      hostward echo, restores at 2.5s)
//!                                     uplink/c0 (leg, faces=target, connect)
//!                                          │
//!                             serial usb0 ─┴─▶ echo device ──(hostward echo)──┘
//! ```
//!
//! Needs a serial *device* (the echo device that bounces the burst back hostward to
//! trip the outage), so it **skips** where none exists (macOS): [`serial_echo`]
//! returns `None`. Data-plane ground truth is the sim client's byte-exact
//! `sha256_sent`/`sha256_received`, never a judgement (§5). The two legs themselves
//! run everywhere; only the echo round-trip needs the device.

use std::net::TcpListener;
use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Rpc, Sim, serial_echo, wait_until};
use serde_json::Value;

/// Two distinct free ephemeral TCP ports on loopback (the portable replacement for
/// the bash `free_port` python one-liner). Both listeners are held simultaneously
/// then dropped, so the two ports are guaranteed distinct.
fn two_free_ports() -> (u16, u16) {
    let a = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral a");
    let b = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral b");
    let pa = a.local_addr().expect("local_addr a").port();
    let pb = b.local_addr().expect("local_addr b").port();
    (pa, pb)
}

/// A leg node's flattened `connection` field (`connected`/`waiting`/`faulted`), or
/// `None` when the node is absent (leg.rs `state_extra`, §7.4).
fn leg_connection(rpc: &Rpc, name: &str) -> Option<String> {
    rpc.node(name)?
        .get("connection")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Whether the leg's channel `ch` is `bound` (the peer announced it, §8).
fn channel_bound(rpc: &Rpc, name: &str, ch: &str) -> bool {
    rpc.node(name)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/binding"))
                .and_then(Value::as_str)
                .map(|s| s == "bound")
        })
        .unwrap_or(false)
}

/// The leg's node-level `reconnect_count` (§7.4).
fn reconnect_count(rpc: &Rpc, name: &str) -> u64 {
    rpc.node(name)
        .and_then(|n| n.get("reconnect_count").and_then(Value::as_u64))
        .unwrap_or(0)
}

/// The leg channel's `purged_on_reconnect` counter — outage-era targetward backlog
/// discarded on reconnect (§7.4 purge-on-reconnect).
fn purged_on_reconnect(rpc: &Rpc, name: &str, ch: &str) -> u64 {
    rpc.node(name)
        .and_then(|n| {
            n.pointer(&format!("/channels/{ch}/purged_on_reconnect"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

/// Every counter on the receiving daemon that moves while a hostward byte is still
/// travelling toward the console: what the receiving leg forwarded to local
/// consumers, what it shed at its own hostward boundary, and what the `p0` console
/// then did with what it got (discarded for want of a client, or dropped because the
/// client was too slow). A byte in flight hostward has to land in one of these, so a
/// tuple that stops changing is the observable form of "the console is quiet".
///
/// Taken from a **single** `state` snapshot, so the four numbers describe one instant
/// rather than four (a byte can move between two `state` calls and be missed by both).
///
/// A missing pointer **panics** rather than reading as `0`. The tuple is only ever
/// compared with its predecessor, so an `unwrap_or(0)` here would turn a renamed or
/// relocated counter into a constant — and a constant tuple is "quiet" forever, which
/// would silently degrade step 6a's second gate into a fixed 500 ms wait with nobody
/// the wiser. Same reasoning as the meta-gates' planted-violation self-proofs (§5): a
/// gate that stops gating has to say so.
fn hostward_progress(rpc: &Rpc) -> (u64, u64, u64, u64) {
    let state = rpc.state();
    let node = |name: &str| -> Value {
        state
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|ns| {
                ns.iter()
                    .find(|n| n.get("name").and_then(Value::as_str) == Some(name))
            })
            .cloned()
            .unwrap_or_else(|| panic!("node `{name}` absent from `state`: {state}"))
    };
    let num = |v: &Value, ptr: &str| {
        v.pointer(ptr)
            .and_then(Value::as_u64)
            .unwrap_or_else(|| panic!("`state` has no numeric `{ptr}`: {v}"))
    };
    let leg = node("downlink");
    let pty = node("p0");
    (
        num(&leg, "/channels/c0/delivered_hostward"),
        num(&leg, "/channels/c0/discarded_hostward"),
        num(&pty, "/discarded_no_client"),
        num(&pty, "/dropped_slow_consumer"),
    )
}

/// Drive one seeded echo round-trip through daemon B's pty `p0` and return the sim
/// client verdict (whose `sha256_sent`/`sha256_received` are the byte-exact ground
/// truth). Blocks to completion, as the bash's foreground `nexus-sim client` did.
fn echo_roundtrip(p0: &Path, send_spec: &str, seed: u64, timeout_ms: u64) -> Value {
    let path = p0.to_string_lossy().into_owned();
    let seed = seed.to_string();
    let timeout = timeout_ms.to_string();
    Sim::client(&[
        "--path",
        &path,
        "--send",
        send_spec,
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        &timeout,
    ])
}

#[test]
fn outage_faults_then_purges_then_recovers_byte_clean() {
    // The echo device that bounces the burst back hostward (tripping the outage) is a
    // serial *device*; skip where the platform has no software serial device (macOS).
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP outage_faults_then_purges_then_recovers_byte_clean: no serial device on this platform"
        );
        return;
    };

    let (port_b, port_p) = two_free_ports();

    // --- Daemon B: the receiver. Its leg listens on PORT_B; a pty p0 is the console.
    let db = Daemon::start();
    let rpc_b = db.rpc();
    let p0 = db.run().join("p0");
    let cfg_b = format!(
        r#"
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "tcp"
role = "listen"
address = "127.0.0.1:{port_b}"
arbitration = "free-for-all"
channels = ["c0"]
[[node]]
type = "pty"
name = "p0"
path = "{p0}"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
"#,
        p0 = p0.display(),
    );
    rpc_b.load_toml(&cfg_b, false).expect("daemon B load");

    // --- The proxy: daemon A dials PORT_P; the proxy forwards to PORT_B, severing
    //     after 8KiB of A's outward (hostward echo) flow, then restoring after 2.5s.
    let proxy_listen = format!("127.0.0.1:{port_p}");
    let proxy_connect = format!("127.0.0.1:{port_b}");
    let _proxy = Sim::spawn(
        &[
            "tcp-proxy",
            "--listen",
            &proxy_listen,
            "--connect",
            &proxy_connect,
            "--drop-after",
            "8KiB",
            "--restore-after-ms",
            "2500",
            "--timeout-ms",
            "40000",
        ],
        None,
    );

    // --- Daemon A: the sender. Its serial owns the echo device; its leg connects to
    //     PORT_P through the proxy.
    let da = Daemon::start();
    let rpc_a = da.rpc();
    let cfg_a = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "tcp"
role = "connect"
address = "127.0.0.1:{port_p}"
reconnect_initial_ms = 150
reconnect_max_ms = 600
channels = ["c0"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
"#,
        dev = echo.device().display(),
    );
    rpc_a.load_toml(&cfg_a, false).expect("daemon A load");

    // The serial node must open its echo device, and the pty console must materialize.
    assert!(
        rpc_a.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 serial not active: {:?}",
        rpc_a.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || p0.exists()),
        "pty p0 symlink never appeared"
    );

    // Both legs connect and bind.
    assert!(
        wait_until(Duration::from_secs(8), || {
            leg_connection(rpc_b, "downlink").as_deref() == Some("connected")
                && channel_bound(rpc_b, "downlink", "c0")
        }),
        "receiver leg never bound: {:?}",
        rpc_b.node("downlink")
    );
    assert!(
        wait_until(Duration::from_secs(8), || {
            leg_connection(rpc_a, "uplink").as_deref() == Some("connected")
        }),
        "sender leg never connected: {:?}",
        rpc_a.node("uplink")
    );

    // 1. Pre-outage: a small round-trip is clean (well under the 8KiB drop threshold).
    let pre = echo_roundtrip(&p0, "seeded:4KiB", 11, 8000);
    assert_eq!(
        pre["pass"].as_bool(),
        Some(true),
        "pre-outage round-trip failed: {pre}"
    );
    assert!(
        pre["sha256_sent"].is_string() && pre["sha256_sent"] == pre["sha256_received"],
        "pre-outage checksum mismatch: {pre}"
    );

    // 2. A burst whose echo crosses the 8KiB threshold trips the outage (its own
    //    round-trip is interrupted; not asserted). Run it in the background so its
    //    sustained hostward echo keeps flowing until the proxy severs.
    let p0_str = p0.to_string_lossy().into_owned();
    let burst = Sim::spawn(
        &[
            "client",
            "--path",
            &p0_str,
            "--send",
            "seeded:64KiB",
            "--expect",
            "echo",
            "--seed",
            "22",
            "--timeout-ms",
            "3000",
        ],
        None,
    );

    // 3. The receiver leg detects the outage: it stops being connected while the link
    //    is down (faulted-and-wait). During this window a writer's bytes back up,
    //    paused not dropped.
    assert!(
        wait_until(Duration::from_secs(15), || {
            matches!(leg_connection(rpc_b, "downlink").as_deref(), Some(c) if c != "connected")
        }),
        "receiver leg never registered the outage: {:?}",
        rpc_b.node("downlink")
    );
    drop(burst); // stop the interrupted burst, as the bash killed its background client.

    // 4. An operator types targetward *during* the outage. With the leg disconnected,
    //    these bytes back up at the receiver (paused, not dropped) — exactly the stale
    //    command hazard purge-on-reconnect exists to defuse (§6/§7.4). The send-only
    //    client (no `--expect`) writes and returns; its verdict is intentionally
    //    ignored (the bash's `|| true`).
    let _ = Sim::client(&[
        "--path",
        &p0_str,
        "--send",
        "seeded:12KiB",
        "--seed",
        "99",
        "--timeout-ms",
        "2000",
    ]);

    // 5. After restore the connect-role leg reconnects (reconnect_count rises,
    //    connection returns to connected, channel rebinds).
    assert!(
        wait_until(Duration::from_secs(20), || {
            leg_connection(rpc_b, "downlink").as_deref() == Some("connected")
                && channel_bound(rpc_b, "downlink", "c0")
                && reconnect_count(rpc_b, "downlink") >= 1
        }),
        "receiver leg never reconnected after restore: {:?}",
        rpc_b.node("downlink")
    );

    // 6. Purge-on-reconnect: the outage-era targetward backlog was discarded with a
    //    counter (§7.4), so stale commands never fire post-restore.
    let purged = purged_on_reconnect(rpc_b, "downlink", "c0");
    assert!(
        purged > 0,
        "purge-on-reconnect counter did not record outage-era backlog (got {purged}): {:?}",
        rpc_b.node("downlink")
    );

    // 6a. The reconnect releases a SECOND thing, and step 7 has to let it land first.
    //
    //     Reconnect purges the outage-era *targetward* backlog (step 6) — and flushes
    //     the outage-era **hostward** one. Step 2's abandoned 64 KiB burst (seed 22)
    //     was echoed by the device while the link was down, so it sat in daemon A's
    //     `uplink/c0` per-channel hostward queue for the whole outage. Purge-on-
    //     reconnect does not touch it and MUST NOT: §6 calls the purge "the one
    //     sanctioned *targetward* drain", and `leg.rs` gates it on
    //     `faces == Facing::Host`. So on reconnect that backlog crosses the restored
    //     link and is written to this very console — specified behaviour (§5, §7.4),
    //     not a defect, and not something to "fix" in the product.
    //
    //     It made step 7 ambiguous. Its client used to attach immediately after the
    //     `wait_until` above, i.e. inside the ~20-30 ms window in which the flood is
    //     landing, and then read the flood as though it were its own echo. Measured at
    //     ~1 failure in 10 unloaded runs. It reported `received: 8190, sent: 4096` and
    //     read as a *doubling*, which it never was: a pts hands out at most 4095 bytes
    //     per read (`N_TTY_BUF_SIZE - 1`) and `nexus-sim`'s `read_until` had no cap at
    //     `n`, so any contaminated stream rendered as 4095 + 4095. With that cap now in
    //     place the same contamination reports `received: 4096` with a non-zero
    //     `overshoot` and a checksum mismatch — a clearer verdict of the same failure,
    //     which this barrier is what actually prevents.
    //
    //     **Counter-intuitively, CPU load SUPPRESSES this**: load widens client-spawn
    //     latency past the flood. So a green loaded re-run is not evidence of anything,
    //     and this barrier must not be removed on the strength of one. Nor should it be
    //     "simplified" into a sleep — AGENTS §5 forbids bare sleeps, and both gates
    //     below end on an observable.
    //
    //     Step 7's property is "the data plane recovered", which can only be measured
    //     from a known-quiet console. Two independent gates, because each covers the
    //     other's hole:
    //       (a) drain the console to quiet. Only a reader can clear bytes the flood
    //           already deposited in the pts buffer; no RPC counter can see those.
    //       (b) then require the receiving daemon's hostward accounting to stop moving.
    //           This catches the converse — a drain that reached its quiet window
    //           before the flood ever started arriving, and so drained nothing.
    //
    //     Do not delete (a) on the grounds that it usually reads zero. It does: in the
    //     common case the flood is already discarded to `p0.discarded_no_client` before
    //     the drain subprocess finishes spawning (measured: ~40-55 KiB charged there by
    //     the time step 6 returns, drain `received: 0`). Its job is the *uncommon* case —
    //     the same ~1-in-10 window in which step 7's client used to attach mid-flood —
    //     where it attaches first and absorbs the flood, including whatever already sits
    //     in the pts buffer, which no counter can observe and no counter can clear.
    let flood = Sim::client(&[
        "--path",
        &p0_str,
        "--drain",
        // Quiet window an order of magnitude past the measured 20-30 ms flood latency.
        "--quiet-ms",
        "750",
        "--timeout-ms",
        "20000",
    ]);
    // `pass` is deliberately not asserted: a `--drain` that reads zero bytes reports
    // `pass: false` by design (`nexus-sim`'s `recv_pass`), and an empty flood is a
    // legal outcome here — it is in fact the common one. What must hold is that the
    // console reached *quiet* rather than the wall clock, and `timed_out` is true for
    // exactly one break, the deadline.
    assert_ne!(
        flood["timed_out"].as_bool(),
        Some(true),
        "the console never went quiet after reconnect (the outage-era hostward backlog \
         is still arriving), so step 7 cannot tell its echo from the flood: {flood}"
    );
    // (b) Hostward accounting stable for half a second, on top of the drain's 750 ms of
    //     real silence. `wait_until` polls every 20 ms, so this is ~25 samples.
    const QUIET: Duration = Duration::from_millis(500);
    let mut last = hostward_progress(rpc_b);
    let mut since = Instant::now();
    assert!(
        wait_until(Duration::from_secs(20), || {
            let now = hostward_progress(rpc_b);
            if now != last {
                last = now;
                since = Instant::now();
            }
            since.elapsed() >= QUIET
        }),
        "the receiving daemon's hostward accounting never went quiet after reconnect \
         (leg {:?}, console {:?})",
        rpc_b.node("downlink"),
        rpc_b.node("p0")
    );

    // 7. Post-restore: a fresh round-trip is byte-clean (the data plane recovered).
    //    The console is now known-quiet (6a), so what this client reads can only be its
    //    own echo.
    let post = echo_roundtrip(&p0, "seeded:4KiB", 33, 8000);
    assert_eq!(
        post["pass"].as_bool(),
        Some(true),
        "post-restore round-trip failed: {post}"
    );
    assert!(
        post["sha256_sent"].is_string() && post["sha256_sent"] == post["sha256_received"],
        "post-restore checksum mismatch: {post}"
    );
}

// ---- LEG-3 / DM-2: the *receiving* side purges its own backlog too -------------

/// `usb0`'s current lock holder / FIFO waiter queue on the receiving daemon (§6).
fn holder(rpc: &Rpc) -> Option<String> {
    rpc.node("usb0")?
        .pointer("/lock/holder")?
        .as_str()
        .map(str::to_owned)
}
fn waiters(rpc: &Rpc) -> Vec<String> {
    rpc.node("usb0")
        .and_then(|n| n.pointer("/lock/waiters").cloned())
        .and_then(|w| w.as_array().cloned())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_receiving_leg_purges_its_own_targetward_backlog_on_peer_disconnect() {
    // §6/§7.4's purge-on-reconnect guarantee — "twenty minutes of buffered commands
    // must not fire into its boot prompt" — held on the *sending* side (asserted by
    // the test above) and, until LEG-3, silently did not on the receiving one: a
    // `faces = target` leg owns a bounded queue of wire-arriving chunks plus the one
    // it is currently trying to write, and nothing purged them when the peer went
    // away. The code comment even denied the backlog existed.
    //
    // The backlog is built deterministically rather than by racing a proxy: a second
    // writer (`hog`) holds the local endpoint's write lock, so the leg's channel task
    // takes its first chunk, parks in the FIFO queue behind `hog` — visible in
    // `usb0.lock.waiters` — and everything after it stacks up in the channel's
    // inbound queue. That parked chunk *is* §6's "chunk held by a producer suspended
    // mid-send", the case a single non-yielding `try_recv` pass misses.
    //
    // No serial device: `usb0`'s device is absent, so it is `waiting` while its lock
    // and origins exist structurally — this runs on every platform.
    const FRAMES: usize = 4;
    const FRAME_LEN: u64 = 64;
    const TOTAL: u64 = FRAMES as u64 * FRAME_LEN;

    let d = Daemon::start();
    let rpc = d.rpc();
    let leg = d.run().join("leg.sock");
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "pty"
name = "hog"
path = "{hog}"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "listen"
address = "{leg}"
idle_release_ms = 60000
channels = ["c0"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
[[edge]]
a = "usb0"
b = "hog"
"#,
        dev = d.run().join("absent-device").display(),
        hog = d.run().join("hog").display(),
        leg = leg.display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load receiving-leg graph");
    assert!(
        rpc.wait_status("hog", "active", Duration::from_secs(10)),
        "hog pty not active: {:?}",
        rpc.node("hog")
    );
    assert!(
        wait_until(Duration::from_secs(10), || leg.exists()),
        "the leg never bound its listen socket"
    );

    // A local writer holds the floor, so nothing the peer sends can be written.
    rpc.lock("hog", false, false, None).expect("lock hog");
    assert_eq!(holder(rpc).as_deref(), Some("hog"), "hog should hold usb0");

    // The remote operator types four commands into the outage.
    let leg_str = leg.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec![
        "wire".into(),
        "--transport".into(),
        "unix".into(),
        "--address".into(),
        leg_str,
        "--announce".into(),
        "c0".into(),
    ];
    for _ in 0..FRAMES {
        args.push("--send".into());
        args.push(format!("c0={FRAME_LEN}"));
    }
    args.extend([
        "--hold-ms".into(),
        "30000".into(),
        "--timeout-ms".into(),
        "40000".into(),
    ]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let peer = Sim::spawn(&argv, None);

    // The leg's channel task took the first chunk and is parked in the FIFO queue
    // behind `hog`; the rest are queued behind it. That is the backlog under test.
    assert!(
        wait_until(Duration::from_secs(10), || waiters(rpc)
            == vec!["uplink/c0".to_string()]),
        "the leg's channel task never queued for the local write lock: {:?}",
        rpc.node("usb0")
    );

    // The peer's machine vanishes mid-outage.
    drop(peer);
    assert!(
        wait_until(Duration::from_secs(10), || {
            leg_connection(rpc, "uplink").as_deref() != Some("connected")
        }),
        "the leg never registered the peer's disconnect: {:?}",
        rpc.node("uplink")
    );

    // The local floor frees. The leg is granted the lock it was queued for — and
    // must then discard everything the dead connection left behind rather than fire
    // it at a device that has moved on (§6).
    rpc.unlock("hog").expect("unlock hog");
    assert!(
        wait_until(Duration::from_secs(10), || {
            purged_on_reconnect(rpc, "uplink", "c0") == TOTAL
        }),
        "the receiving side did not purge its outage-era backlog to quiescence \
         (want {TOTAL} bytes, LEG-3/DM-2): {:?}",
        rpc.node("uplink")
    );
    let node = rpc.node("uplink").expect("uplink node");
    assert_eq!(
        node.pointer("/channels/c0/accepted_targetward")
            .and_then(Value::as_u64),
        Some(0),
        "an outage-era command reached the local graph: {node}"
    );
    // The purge is not a loss report in disguise: nothing is charged to the ordinary
    // targetward discard counter (§5 keeps the two causes apart, LEG-4).
    assert_eq!(
        node.pointer("/channels/c0/discarded_targetward")
            .and_then(Value::as_u64),
        Some(0),
        "purged bytes were also counted as an ordinary discard: {node}"
    );
    // …and the leg gave the local endpoint's floor back on the way out (§7.1).
    assert!(
        wait_until(Duration::from_secs(5), || holder(rpc).is_none()),
        "the leg kept the local write lock after purging: {:?}",
        rpc.node("usb0")
    );
}
