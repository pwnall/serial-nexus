//! PTY setup and lifecycle guards for review 32 (`docs/32-claude-opus-code-review.md`),
//! the `p12_*` family — design §7.2 (presence, the pts symlink), §6
//! (detach-release), §5 (targetward is the direction loss is forbidden on).
//!
//! Two findings, both in `daemon/src/nodes/pty.rs`, both of which leave a
//! *live* graph quietly wrong rather than failing loudly:
//!
//! 1. [`a_faulted_pty_setup_leaves_no_symlink_aliasing_a_live_console`] — **RV-1**.
//!    `PtyNode::setup` used to install the configured symlink *before*
//!    `apply_perms` and `prime_slave`. Either of those failing returns `Err`
//!    without ever storing the master, so the fd drops, the kernel reclaims the
//!    pts index — and the symlink stays on disk, now resolving onto whichever pty
//!    node receives that index next. `install_symlink`'s dangling-into-devpts
//!    recovery cannot clean it up, because it no longer dangles. The trigger here
//!    is an unresolvable `group` (deterministic and cheap); the *reachable* one is
//!    a plain `chown` EPERM on a non-root daemon using the `group =` setting
//!    `docs/security.md` endorses.
//!
//! 2. [`a_saturated_targetward_channel_does_not_freeze_presence_or_the_lock`] —
//!    **CONC-1**. `read_and_poll` forwarded a client's bytes with
//!    `tx.send(payload).await` inside its drain loop. When the host endpoint's
//!    bounded targetward channel is full — the ordinary state of a serial node
//!    that is `waiting` for an absent device, or `active` while the far end does
//!    not drain — that await parks for as long as the device is gone, and
//!    *everything after it in the loop stops*: the presence swap,
//!    `handle_last_close` (the §7.2 baseline-termios reset, §6 detach-release and
//!    purge-on-detach) and the reconciliation backstop. The node then reports
//!    `client_present: true` forever after its client exits, and an on-demand write
//!    lock stays held by an origin whose client is gone.
//!
//! Neither needs a serial *device*: the `usb0` node's device is deliberately
//! absent, so it comes up `waiting` while its write lock, origins and targetward
//! receiver exist regardless — structural, exactly as `p4_waiting` and
//! `p9_pty_collapse` establish. Both therefore run on **every** platform.
//!
//! Two more tests pin what the *fix* for CONC-1 cost, which the independent audit
//! of the remediation found and measured. Parking the refused payload in a
//! `pending` slot took the pty reader off `tx.send().await` — and off everything
//! that knew how to find a producer suspended there:
//!
//! 3. [`a_collapsed_session_against_a_saturated_endpoint_still_releases_the_lock`]
//!    — the arm the original CONC-1 guard below cannot reach, because it asserts
//!    `client_present == Some(true)` before saturating and so only ever exercises
//!    the path where the presence latch fired. A session whose whole attach → write
//!    → close falls inside one `IDLE_POLL` gap latches nothing, and against a
//!    saturated endpoint the drain that *would* have armed `saw_session` either
//!    never ran (the `pending.is_none()` gate) or ended early on the full channel
//!    without ever reaching the EOF that `closed && saw_session` also required.
//!    Measured by the audit at five leaked locks out of five, with another origin's
//!    `send` failing `-32003` behind them.
//! 4. [`a_stalled_consoles_backlog_never_fires_into_the_device_that_comes_back`] —
//!    §6's purge invariant, both of the instances a held payload escaped. The
//!    *reconnect* instance is specified to drain the parked pipeline to quiescence
//!    "including a chunk held by a producer suspended mid-send", and a payload
//!    parked behind a bare `sleep` is not in the pipeline at all:
//!    `boundary::drain_to_quiescence` drains, `yield_now`s — and reaches nothing.
//!    The *detach* instance settles the floor question, and a payload carried
//!    across it is delivered by an origin that no longer holds the floor. The audit
//!    measured both: `purged_on_reconnect: 898558` reported on a graph that then
//!    wrote 5634 bytes of pre-outage console input into the device that had just
//!    come back.
//!
//! A fifth, found later while chasing a `p6_outage` failure and the second latent
//! path to that symptom:
//!
//! 5. [`a_fresh_console_session_does_not_inherit_the_previous_sessions_bytes`] —
//!    §7.2 promises the daemon resets the pair on last close "so **every client
//!    session starts deterministic**". It reset the *termios* and left the pair's
//!    undelivered **hostward** queue sitting in the kernel, which handed it to the
//!    next opener: a fresh client read the previous session's data, at the pts
//!    queue's full depth (~13.8 KiB), in the `4095/4095/…` reads that geometry
//!    produces. The §5 half was the worse half — nothing counted those bytes when
//!    they were eventually destroyed, so `state` reported `discarded_no_client: 0`
//!    on a boundary that had silently shed kilobytes. The fix drains the pair at
//!    last close and charges it to `discarded_at_last_close`.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, Rpc, TempRun, serial_echo, wait_until};

/// Open a pty slave the way a client does — never adopting it as this process's
/// controlling terminal. `nonblocking` is what lets the CONC-1 test *detect* that
/// the daemon has stopped draining the master instead of deadlocking on it.
fn attach(path: &Path, nonblocking: bool) -> std::fs::File {
    let mut flags = libc::O_NOCTTY;
    if nonblocking {
        flags |= libc::O_NONBLOCK;
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(flags)
        .open(path)
        .unwrap_or_else(|e| panic!("open pty slave {}: {e}", path.display()))
}

/// A node's `client_present` from `state` (§7.2), or `None` if the node is absent.
fn client_present(rpc: &Rpc, node: &str) -> Option<bool> {
    rpc.node(node)?.get("client_present")?.as_bool()
}

// ============================================================================
// RV-1 — a faulted setup must leave no symlink behind.
// ============================================================================

/// `conA` faults inside `apply_perms`; `conB` is created straight afterwards and
/// is the node most likely to receive the pts index `conA` just freed.
const TWO_CONSOLES: &str = r#"
[[node]]
type = "pty"
name = "conA"
path = "CON_A"
group = "no-such-group-serial-nexus-rv1"

[[node]]
type = "pty"
name = "conB"
path = "CON_B"
"#;

fn two_consoles(run: &TempRun) -> String {
    TWO_CONSOLES
        .replace("CON_A", &run.join("conA").display().to_string())
        .replace("CON_B", &run.join("conB").display().to_string())
}

#[test]
fn a_faulted_pty_setup_leaves_no_symlink_aliasing_a_live_console() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let path_a = d.run().join("conA");
    let path_b = d.run().join("conB");

    rpc.load_toml(&two_consoles(d.run()), false)
        .expect("load two pty nodes");

    // The premise: one node faults on the unresolvable group, the other comes up.
    assert!(
        rpc.wait_status("conB", "active", Duration::from_secs(10)),
        "conB never became active: {:?}",
        rpc.node("conB")
    );
    assert_eq!(
        rpc.node_status("conA"),
        "faulted",
        "conA should have faulted on its group: {:?}",
        rpc.node("conA")
    );
    let reason = rpc
        .node("conA")
        .and_then(|n| n.get("reason").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    assert!(
        reason.contains("no-such-group-serial-nexus-rv1"),
        "conA faulted for the wrong reason ({reason:?}) — this test only pins RV-1 \
         if the fault comes from the post-symlink permission step"
    );

    // RV-1 proper. The faulted node must own nothing on disk: a leftover symlink
    // resolves to the pts index the kernel reclaimed and handed to conB, so
    // `readlink conA` and `readlink conB` both name conB's device and anything
    // opening the faulted console's path attaches to a *different* node's console.
    let leftover = std::fs::symlink_metadata(&path_a);
    assert!(
        leftover.is_err(),
        "the faulted node left {} on disk (readlink -> {:?}) — it now aliases \
         whichever pty node received the freed pts index (RV-1)",
        path_a.display(),
        std::fs::read_link(&path_a),
    );

    // ... and the healthy node's symlink still resolves to its own pts.
    let pts_b = rpc
        .node("conB")
        .and_then(|n| n.get("pts_path").and_then(Value::as_str).map(str::to_owned))
        .expect("conB reports a pts_path");
    assert_eq!(
        std::fs::read_link(&path_b)
            .expect("conB symlink")
            .display()
            .to_string(),
        pts_b,
        "conB's symlink does not resolve to its own pts"
    );

    // The consequence the finding reproduced, asserted from the operator's side:
    // opening the faulted console's path must not reach conB. The positive control
    // below is what makes this meaningful — presence *does* latch when conB's own
    // path is opened, so "stayed false" is a property, not a dead assertion.
    assert_eq!(
        client_present(rpc, "conB"),
        Some(false),
        "conB reports a client before anything opened it"
    );
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&path_a)
            .is_err(),
        "the faulted console's path is openable — it resolves to some live device"
    );
    assert_eq!(
        client_present(rpc, "conB"),
        Some(false),
        "opening the faulted node's path flipped conB's client_present (RV-1)"
    );

    let live = attach(&path_b, false);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "conB")
            == Some(true)),
        "positive control failed: conB never saw a client on its own path, so the \
         assertions above prove nothing"
    );
    drop(live);
}

// ============================================================================
// CONC-1 — targetward backpressure must not freeze the lifecycle half.
// ============================================================================

/// One PTY writer into one device-absent serial host endpoint, the `p9_pty_collapse`
/// shape: the serial node never opens anything (`waiting`), so nothing ever drains
/// its targetward channel, while its lock and the PTY's on-demand origin are built
/// by the wiring regardless (§6).
fn stalled_endpoint(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = run.join("console").display(),
        dev = run.join("absent-device").display(),
    )
}

/// The current holder of `usb0`'s write lock, if any (§6).
fn holder(rpc: &Rpc) -> Option<String> {
    rpc.node("usb0")?
        .pointer("/lock/holder")?
        .as_str()
        .map(str::to_owned)
}

/// Bytes purged from `origin`'s targetward backlog on the `usb0` endpoint lock —
/// §6's per-origin purge counter, the one state is required to report. This is
/// where a detach purge lands, and deliberately *not* `discarded_targetward`: the
/// two say different things and only the second is a §5 violation.
fn purged(rpc: &Rpc, origin: &str) -> u64 {
    rpc.node("usb0")
        .and_then(|n| {
            n.pointer("/lock/origins")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
        .iter()
        .find(|o| o.get("origin").and_then(Value::as_str) == Some(origin))
        .and_then(|o| o.get("purged").and_then(Value::as_u64))
        .unwrap_or(0)
}

/// A pty node's `discarded_targetward` (§5 loss — bytes the endpoint could never
/// take because it went away), 0 if the node or the field is absent.
fn discarded_targetward(rpc: &Rpc, node: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| n.get("discarded_targetward").and_then(Value::as_u64))
        .unwrap_or(0)
}

/// One of a node's `state` counters, 0 if the node or the field is absent. A
/// missing field reads as 0 on purpose: that is what an *unfixed* daemon reports
/// for `discarded_at_last_close`, so the guard below fails on the defect rather
/// than on a JSON pointer.
fn counter(rpc: &Rpc, node: &str, field: &str) -> u64 {
    rpc.node(node)
        .and_then(|n| n.get(field).and_then(Value::as_u64))
        .unwrap_or(0)
}

/// Open the PTY slave, write `data`, and close — as fast as the kernel allows,
/// with no intervening syscall the daemon could interleave with, so the whole
/// session lands inside one `IDLE_POLL` (5 ms) window of the reader task and the
/// daemon meets attach, data and hangup at once. The same fixture
/// `p9_pty_collapse` uses; it is repeated here because the property under a
/// *saturated* endpoint is a different one.
fn collapsed_session(path: &Path) {
    let mut slave = attach(path, false);
    slave.write_all(b"c\n").expect("write to the pty slave");
    slave.flush().expect("flush the pty slave");
    drop(slave);
}

/// Write to `slave` until the daemon stops draining the master, returning the
/// byte count that took.
///
/// The fd is non-blocking, so a write that cannot land reports `WouldBlock`
/// instead of wedging this thread. A *persistent* `WouldBlock` means the kernel's
/// pts buffer is full and nobody is reading the master — which, given the caller
/// has verified that `console` holds the write lock (so §6 permits the reader to
/// drain) and that the edge is attached, can only be the host endpoint's bounded
/// targetward channel having filled. That is the saturation the finding needs, and
/// it is detected rather than assumed: no byte count is hard-coded, because how
/// many `read(2)`s a given number of bytes becomes is a kernel scheduling detail
/// (the reported threshold, CHANNEL_CAP + 1 = 257 chunks, is a *chunk* count).
fn saturate(slave: &mut std::fs::File) -> u64 {
    /// How long `WouldBlock` must persist before the endpoint counts as saturated:
    /// long enough that a momentary gap between two of the reader's polls cannot
    /// masquerade as a stall.
    const STALL: Duration = Duration::from_millis(500);
    /// Bounds: the channel holds 256 chunks of at most 64 KiB, so 40 MiB is well
    /// past any reachable fill, and a run that never stalls is a broken premise
    /// rather than a slow one.
    const MAX_BYTES: u64 = 40 * 1024 * 1024;
    const DEADLINE: Duration = Duration::from_secs(60);

    let block = vec![b'x'; 8192];
    let started = Instant::now();
    let mut written = 0u64;
    let mut blocked_since: Option<Instant> = None;
    loop {
        match slave.write(&block) {
            Ok(n) => {
                written += n as u64;
                blocked_since = None;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => match blocked_since {
                Some(since) if since.elapsed() >= STALL => return written,
                Some(_) => std::thread::sleep(Duration::from_millis(20)),
                None => blocked_since = Some(Instant::now()),
            },
            Err(e) => panic!("write to the pty slave failed: {e}"),
        }
        assert!(
            written < MAX_BYTES && started.elapsed() < DEADLINE,
            "the daemon never stopped draining the master after {written} bytes in \
             {:?} — the targetward channel was never saturated, so this test would \
             prove nothing",
            started.elapsed()
        );
    }
}

/// CONC-1: with the host endpoint's targetward channel saturated, a client that
/// exits must still be observed as gone and its on-demand write lock must still be
/// released — within a bounded time, not "whenever the device comes back".
///
/// **Why this fails against the unfixed code.** The old drain loop forwarded with
/// `tx.send(payload).await`; once the 256-deep channel is full that await parks,
/// and the presence swap, `handle_last_close` and the reconciliation backstop all
/// live *after* it in the same loop body. The verifier of CONC-1 measured exactly
/// this shape on a graph identical to [`stalled_endpoint`]: 200 client chunks left
/// presence and detach-release working, 260 froze both, with
/// `client_present: true` and `holder: "console"` still reported at T+20 s, and the
/// freeze lifting only when something finally drained the channel. This test does
/// not count chunks — it drives the client until the daemon demonstrably stops
/// reading the master ([`saturate`]), which is the same state reached by a strictly
/// stronger route, and then asserts the lifecycle half recovers.
///
/// It also pins the half of the fix that is easy to get wrong in the other
/// direction: `discarded_targetward` must stay **0**. §5 forbids dropping
/// targetward, so the cure for a full channel is to park the *byte* (and stop
/// reading, which backpressures the client through the kernel buffer) — never to
/// shed it in order to keep the loop moving.
#[test]
fn a_saturated_targetward_channel_does_not_freeze_presence_or_the_lock() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    rpc.load_toml(&stalled_endpoint(d.run()), false)
        .expect("load graph");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console pty not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // The PTY takes the lock as an on-demand origin, the way an operator's
    // `lock console` (or a `send`) does before typing. Without it §6 would keep the
    // reader from draining at all and the stall below would prove nothing — hence
    // the holder assertion after saturating.
    let r = rpc
        .lock("console", false, false, None)
        .unwrap_or_else(|e| panic!("lock console: [{}] {}", e.code, e.message));
    assert_eq!(
        r.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock console did not acquire: {r}"
    );

    let mut slave = attach(&console, true);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the client attach"
    );
    let written = saturate(&mut slave);
    // The saturation is only meaningful while the reader is *permitted* to drain:
    // a lost lock would stall the master for an entirely different reason (§6).
    assert_eq!(
        holder(rpc).as_deref(),
        Some("console"),
        "console stopped holding the lock while saturating, so the stall after \
         {written} bytes is not the full-channel stall this test needs"
    );

    drop(slave); // the client exits, leaving the endpoint full

    // The whole finding, in two assertions. Both used to hang until the absent
    // device came back — i.e. never.
    assert!(
        wait_until(Duration::from_secs(15), || client_present(rpc, "console")
            == Some(false)),
        "client_present stayed true after the client exited with the targetward \
         channel saturated ({written} bytes) — the presence half of read_and_poll \
         is parked behind the data half (CONC-1)"
    );
    assert!(
        wait_until(Duration::from_secs(15), || holder(rpc).is_none()),
        "the on-demand write lock was NOT released after its client exited with the \
         targetward channel saturated — detach-release is parked behind the send \
         (CONC-1, §6). Holder still {:?}",
        holder(rpc)
    );

    // §5: the byte that could not be handed over is parked, never shed.
    assert_eq!(
        discarded_targetward(rpc, "console"),
        0,
        "targetward bytes were discarded to keep the loop moving — §5 forbids \
         dropping targetward; the payload must wait and the client must \
         backpressure through the unread kernel buffer"
    );

    // ... and the other half of "parked, never shed": what the endpoint could not
    // take by the time the client left is *purged and counted* at the detach, not
    // carried past the release. Both numbers together are the property — a zero
    // here beside a zero above would mean the payload is still being held by an
    // origin that no longer holds the floor, which is what the audit measured
    // being written to the device minutes later (§6's detach instance).
    assert!(
        purged(rpc, "console") > 0,
        "the console detached from a saturated endpoint holding an un-delivered \
         payload, and nothing was purged: §6 settles the floor question at detach \
         by draining that backlog and counting it, and a payload carried across \
         the release is delivered by an origin that no longer holds the lock. \
         lock={:?}",
        rpc.node("usb0").and_then(|n| n.get("lock").cloned())
    );
}

// ============================================================================
// The regressions the CONC-1 fix introduced (audit of the review-32 remediation).
// ============================================================================

/// How many collapsed sessions to run against the saturated endpoint. The collapse
/// is a race this test wins nearly every time (a handful of syscalls against a 5 ms
/// poll gap) — one unlucky trial proves nothing, eight prove it. Same reasoning,
/// same number, as `p9_pty_collapse`.
const SATURATED_COLLAPSES: usize = 8;

/// How long a detach-release may take before it counts as leaked. Generous next to
/// the 5 ms poll it actually costs.
const RELEASE_WAIT: Duration = Duration::from_secs(5);

/// CONC-1's collapsed-session arm, which the `pending` slot made unreachable: with
/// the endpoint saturated, a session whose whole attach → write → close falls
/// inside one poll gap must *still* release its on-demand write lock.
///
/// **Why this fails against the shipped CONC-1 fix.** Two clauses had to hold at
/// once and neither did. The master read is gated on `pending.is_none()`, so while
/// a payload is held the loop never calls `read_fd` and the `saw_session` latch
/// cannot arm; and the close trigger was `closed && saw_session`, whose `closed`
/// half comes only from an EOF/EIO the drain never reaches when it ends early on a
/// full channel. So the session left no trace the lifecycle block could act on, the
/// lock stayed with a console whose client was gone, and the endpoint was dead to
/// every other writer for as long as the device stayed away. The audit measured
/// exactly this, control against treatment: unsaturated, five of five collapsed
/// sessions released; saturated, five of five kept `holder: "con"` past +3 s, and
/// another origin's `send` came back `-32003`.
///
/// The premise is established rather than assumed — [`saturate`] returns only once
/// the daemon has demonstrably stopped draining the master, and the endpoint stays
/// that way for the whole test because `usb0`'s device never appears.
///
/// Needs no serial device (`usb0` is `waiting` forever), so it runs everywhere.
#[test]
fn a_collapsed_session_against_a_saturated_endpoint_still_releases_the_lock() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    rpc.load_toml(&stalled_endpoint(d.run()), false)
        .expect("load graph");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console pty not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    // Saturate the endpoint with one ordinary session, then let it go. Everything
    // after this runs against a channel that is full and stays full.
    let r = rpc
        .lock("console", false, false, None)
        .unwrap_or_else(|e| panic!("lock console: [{}] {}", e.code, e.message));
    assert_eq!(
        r.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock console did not acquire: {r}"
    );
    let mut slave = attach(&console, true);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the saturating client attach"
    );
    let written = saturate(&mut slave);
    assert_eq!(
        holder(rpc).as_deref(),
        Some("console"),
        "console stopped holding the lock while saturating, so the stall after \
         {written} bytes is not the full-channel stall this test needs"
    );
    drop(slave);
    assert!(
        wait_until(RELEASE_WAIT, || holder(rpc).is_none()),
        "the saturating client's own detach did not release the lock, so the \
         collapsed sessions below would prove nothing (that is CONC-1 itself, \
         pinned by the test above)"
    );

    let mut released = 0usize;
    for i in 0..SATURATED_COLLAPSES {
        // Re-arm: the operator's `lock console` before a scripted probe runs.
        if holder(rpc).as_deref() != Some("console") {
            let r = rpc
                .lock("console", false, false, None)
                .unwrap_or_else(|e| panic!("lock console #{i}: [{}] {}", e.code, e.message));
            assert_eq!(
                r.get("acquired").and_then(Value::as_bool),
                Some(true),
                "lock console #{i} did not acquire: {r}"
            );
        }

        collapsed_session(&console);

        if wait_until(RELEASE_WAIT, || holder(rpc).is_none()) {
            released += 1;
        } else {
            // Leave the graph usable so the count at the end reports every trial,
            // not just the first failure.
            let _ = rpc.unlock("console");
        }
    }
    assert_eq!(
        released, SATURATED_COLLAPSES,
        "only {released}/{SATURATED_COLLAPSES} collapsed sessions released the write \
         lock against a saturated endpoint. A session that opens, types and closes \
         inside one poll gap leaves its evidence on the master and nowhere else; if \
         a held payload keeps the loop from reading it, or the close trigger insists \
         on an EOF the drain never reached, the endpoint stays locked to a client \
         that is gone (§6, §7.2)"
    );

    // The releases were real releases, not the loop shedding bytes to get moving:
    // §5's counter stays at zero and §6's purge counter carries what the stalled
    // endpoint could not take.
    assert_eq!(
        discarded_targetward(rpc, "console"),
        0,
        "targetward bytes were counted as *lost* rather than purged — §5 loss and \
         §6 purge are different claims and only the first is a defect"
    );
    assert!(
        purged(rpc, "console") > 0,
        "no backlog was purged across {SATURATED_COLLAPSES} detaches from a \
         saturated endpoint, so those sessions' bytes are still somewhere: \
         lock={:?}",
        rpc.node("usb0").and_then(|n| n.get("lock").cloned())
    );
}

/// The graph for the purge test: a serial node whose device appears *later*, a
/// console on it, and a `log` recording everything that comes back off the device.
///
/// The echo double reflects every byte the daemon writes, so the log is a
/// byte-exact record of what reached the port (§5's lossless sink; the same shape
/// `p12_send_deadline` uses). `purge_on_reconnect` is left at its default — this
/// test is about a purge that reports success while missing something, so turning
/// it off would remove the subject.
fn late_device_graph(run: &TempRun, late_dev: &Path, logdir: &Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
        dev = late_dev.display(),
        console = run.join("console").display(),
        logdir = logdir.display(),
    )
}

/// §6's purge invariant, against the two instances a payload held in the pty
/// reader's `pending` slot escaped: **nothing a console typed into a stalled
/// endpoint may reach the device that comes back**.
///
/// Two consoles, two escapes, one assertion each on the same captured stream:
///
/// * `x`s — the **detach** instance. A console saturates the endpoint and walks
///   away. Its un-delivered payload is out of the master and outside the channel,
///   so neither the drain the close handler runs nor the reconnect purge can see
///   it; the retry at the top of the reader's loop does not consult the lock, so it
///   is written by an origin that released the floor long before. The audit
///   measured 5634 bytes of it landing on a device that had just come back —
///   beside a `purged_on_reconnect: 898558` that reported success.
/// * `STALE-CONSOLE-INPUT` — the **reconnect** instance, isolated. The second
///   console types one short line into the still-full channel and *stays attached*,
///   so no detach can cover for the purge: this line is held by the reader and
///   nothing else, and §6 says the reconnect drain reaches "a chunk held by a
///   producer suspended mid-send". Held behind a bare timer it is not suspended in
///   any sense `boundary::drain_to_quiescence` can observe — it drains, `yield_now`s
///   and finds nothing — and the line lands on the device the instant the purge
///   finishes. That is the boot-prompt scenario §7.1 names, in one line instead of
///   twenty minutes of them.
///
/// The marker is what makes the two absences mean something: the endpoint's
/// targetward channel is FIFO, so anything the reader had queued sits *ahead* of a
/// line sent after the reconnect, and waiting for the marker to reach the log gives
/// the stale bytes their full chance to arrive first. Neither payload contains an
/// `x` or the other's text, so each assertion names one escape.
///
/// The second console's line is read into the held slot long before the reconnect:
/// the reader polls the master every `IDLE_POLL` (5 ms) at worst, while the serial
/// node's resolver poll and open take the better part of a second (the test waits
/// for `active`) — three orders of magnitude of slack.
///
/// Linux-only: it needs a software serial device to come back (§5, `serial_echo`
/// is `None` elsewhere).
#[test]
fn a_stalled_consoles_backlog_never_fires_into_the_device_that_comes_back() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_stalled_consoles_backlog_never_fires_into_the_device_that_comes_back: \
             no serial device on this platform"
        );
        return;
    };
    const STALE: &str = "STALE-CONSOLE-INPUT";
    const MARKER: &str = "MARKER-AFTER-RECONNECT";

    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let capture = logdir.join("cap.log");
    let late_dev = d.run().join("late-dev");

    rpc.load_toml(&late_device_graph(d.run(), &late_dev, &logdir), false)
        .expect("load the late-device graph");
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "usb0 should be waiting on an absent device: {:?}",
        rpc.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || console.exists()),
        "console pty symlink never appeared"
    );

    // --- Console 1: type into the stall, then walk away (the detach instance) ---
    let r = rpc
        .lock("console", false, false, None)
        .unwrap_or_else(|e| panic!("lock console: [{}] {}", e.code, e.message));
    assert_eq!(
        r.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock console did not acquire: {r}"
    );
    let mut slave = attach(&console, true);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the first client attach"
    );
    let written = saturate(&mut slave);
    assert_eq!(
        holder(rpc).as_deref(),
        Some("console"),
        "console stopped holding the lock while saturating ({written} bytes), so \
         the stall is not the full-channel stall this test needs"
    );
    drop(slave);
    assert!(
        wait_until(RELEASE_WAIT, || holder(rpc).is_none()),
        "the first console's detach did not release the lock (CONC-1)"
    );

    // --- Console 2: one line into the still-full channel, and stay (reconnect) ---
    let r = rpc
        .lock("console", false, false, None)
        .unwrap_or_else(|e| panic!("re-lock console: [{}] {}", e.code, e.message));
    assert_eq!(
        r.get("acquired").and_then(Value::as_bool),
        Some(true),
        "the second console did not acquire the freed lock: {r}"
    );
    let mut held = attach(&console, false);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the second client attach"
    );
    held.write_all(format!("{STALE}\n").as_bytes())
        .expect("write the stale line");
    held.flush().expect("flush the stale line");

    // --- The port comes back ---
    std::os::unix::fs::symlink(echo.device(), &late_dev).expect("symlink the echo device");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(30)),
        "usb0 never opened the device that appeared: {:?}",
        rpc.node("usb0")
    );
    let purged_on_reconnect = rpc
        .node("usb0")
        .and_then(|n| n.get("purged_on_reconnect").and_then(Value::as_u64))
        .unwrap_or(0);
    assert!(
        purged_on_reconnect > 0,
        "purge-on-reconnect discarded nothing, so this test cannot tell a purge \
         that missed the held payload from a purge that never ran: {:?}",
        rpc.node("usb0")
    );

    // --- Hand the floor back and put a marker behind everything ---
    drop(held);
    assert!(
        wait_until(RELEASE_WAIT, || holder(rpc).is_none()),
        "the second console's detach did not release the lock"
    );
    rpc.send("usb0", MARKER, false, 10_000)
        .expect("a send into a live endpoint is accepted");
    let captured =
        || String::from_utf8_lossy(&std::fs::read(&capture).unwrap_or_default()).into_owned();
    assert!(
        wait_until(Duration::from_secs(30), || captured().contains(MARKER)),
        "the marker never came back off the device, so the fate of the console's \
         backlog is unknown: node={:?} captured={:?}",
        rpc.node("usb0"),
        captured()
    );

    let text = captured();
    assert!(
        !text.contains(STALE),
        "a line typed into a stalled endpoint and still held by the pty reader was \
         written to the device the moment it came back. §6: the reconnect purge \
         drains the parked pipeline to quiescence *including a chunk held by a \
         producer suspended mid-send* — a payload parked on a bare timer is not in \
         the pipeline, and `drain_to_quiescence` reports success without ever \
         seeing it (§7.1: twenty minutes of buffered commands must not fire into \
         the boot prompt)\ncaptured={text:?}"
    );
    assert!(
        !text.contains('x'),
        "the first console saturated this endpoint, walked away, and its backlog \
         fired into the device that came back afterwards — from an origin that had \
         released the write lock several steps earlier. §6 settles that backlog at \
         the detach: drained, counted, never delivered\ncaptured={text:?}"
    );
    // Whatever was purged was purged *and counted* — §5 never allows the silent
    // version, and the counters are how an operator sees a stalled console cost.
    assert!(
        purged(rpc, "console") > 0,
        "two consoles detached from a stalled endpoint and §6's per-origin purge \
         counter never moved: lock={:?}",
        rpc.node("usb0").and_then(|n| n.get("lock").cloned())
    );
    assert_eq!(
        discarded_targetward(rpc, "console"),
        0,
        "targetward bytes were counted as §5 *loss*; a detach purge is not loss"
    );
}

// ============================================================================
// §7.2 last close — a fresh session must not inherit the previous one's bytes.
// ============================================================================

/// The graph for the last-close flush: an echo device, a console on it, and a log
/// recording the same hostward stream.
///
/// The log is not decoration. It is the **lossless** sink of the same fan-out
/// (§5), so "the log holds every byte" is a deterministic statement that the device
/// has finished echoing and the serial node has finished fanning out — which is
/// what lets the test know that anything a later client reads came out of a queue
/// rather than off the wire.
fn echo_console_and_log(run: &TempRun, dev: &Path, logdir: &Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
        dev = dev.display(),
        console = run.join("console").display(),
        logdir = logdir.display(),
    )
}

/// Read whatever is available from a non-blocking pty slave, appending to `sink`.
fn drain_into(slave: &mut std::fs::File, sink: &mut Vec<u8>) {
    let mut buf = [0u8; 8192];
    loop {
        match slave.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => sink.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock => return,
            Err(_) => return,
        }
    }
}

/// §7.2: **every client session starts deterministic** — including in the bytes it
/// is shown. A console whose client attaches and never reads leaves the pair's
/// hostward queue full; when that client detaches, those bytes must be gone, and
/// gone *visibly* (§5).
///
/// **Why this fails against the unfixed daemon.** `handle_last_close` re-applied the
/// baseline termios with `TCSANOW` and nothing else — there was no `tcflush`, and no
/// drain, anywhere in the tree. The kernel accepts ~13.8 KiB of master writes for a
/// client that never reads and keeps them across the client's last close, so the
/// *next* opener receives the previous operator's output: measured at 8192 bytes
/// delivered in reads of `[4095, 4095, 2]`, the `N_TTY_BUF_SIZE - 1` geometry.
/// Reproduced on this graph, and again on the pre-remediation commit, so it is a
/// standing product defect and not a regression.
///
/// Three properties, in the order they matter:
///
/// * The fresh client reads the marker sent *after* it attached and **no `x`** —
///   the previous session's payload is a single repeated byte that appears nowhere
///   in the marker, so one `x` is proof and no `x` is proof of absence rather than
///   of a race. Waiting for the marker is what gives any stale byte its full chance
///   to arrive first: the pty writer is FIFO, so anything queued ahead of the marker
///   would land ahead of it.
/// * Every byte is accounted (§5). The console was fanned out exactly what the log
///   recorded, and after the dust settles that total must equal
///   `discarded_no_client + dropped_slow_consumer + discarded_at_last_close`. This
///   is also the test's quiescence signal: while it does *not* hold, bytes are still
///   somewhere inside the daemon, and attaching the next client would be a race
///   rather than a measurement.
/// * `discarded_at_last_close > 0` — the premise (something really was queued) and
///   the §5 requirement (it was named) in one number. An unfixed daemon reports no
///   such field, which reads as 0.
///
/// Linux-only: it needs a software serial device to emit (`serial_echo` is `None`
/// elsewhere; §5's platform rule).
#[test]
fn a_fresh_console_session_does_not_inherit_the_previous_sessions_bytes() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_fresh_console_session_does_not_inherit_the_previous_sessions_bytes: \
             no serial device on this platform"
        );
        return;
    };
    // A single repeated byte for the stale payload, and a marker that contains
    // none of it — so each assertion below names exactly one stream.
    const STALE_BYTE: u8 = b'x';
    const STALE_LEN: usize = 64 * 1024;
    const MARKER: &str = "MARKER-AFTER-REATTACH";

    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    let logdir = d.run().join("logs");
    std::fs::create_dir_all(&logdir).expect("mkdir log directory");
    let capture = logdir.join("cap.log");

    rpc.load_toml(
        &echo_console_and_log(d.run(), echo.device(), &logdir),
        false,
    )
    .expect("load the echo/console/log graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "usb0 never opened the echo device: {:?}",
        rpc.node("usb0")
    );
    assert!(
        wait_until(Duration::from_secs(10), || console.exists()),
        "console pty symlink never appeared"
    );

    // --- Session A: attach, never read, and let the device fill the pair. ---
    let a = attach(&console, true);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the first client attach"
    );
    let stale = String::from_utf8(vec![STALE_BYTE; STALE_LEN]).expect("ascii payload");
    rpc.send("usb0", &stale, false, 20_000)
        .expect("a send into a live endpoint is accepted");
    // `send` appends the newline (§10), so this is what the device echoes back and
    // what the fan-out hands to *both* consumers.
    let total = STALE_LEN as u64 + 1;
    let logged = || std::fs::metadata(&capture).map(|m| m.len()).unwrap_or(0);
    assert!(
        wait_until(Duration::from_secs(30), || logged() >= total),
        "the echo never came all the way back ({} of {total} bytes logged), so the \
         console's queue state is unknown: {:?}",
        logged(),
        rpc.node("usb0")
    );

    drop(a); // the client detaches without ever having read a byte
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(false)),
        "the daemon never observed the first client leave, so the last-close \
         handler never ran (§7.2)"
    );

    // Quiescence *and* §5's conservation law, asserted after the stale-byte check
    // below so an unfixed daemon fails on the defect rather than on this wait.
    let accounted = || {
        counter(rpc, "console", "discarded_no_client")
            + counter(rpc, "console", "dropped_slow_consumer")
            + counter(rpc, "console", "discarded_at_last_close")
    };
    let settled = wait_until(Duration::from_secs(15), || accounted() >= total);

    // --- Session B: a fresh client, which must see only what arrives from now on ---
    let mut b = attach(&console, true);
    assert!(
        wait_until(Duration::from_secs(10), || client_present(rpc, "console")
            == Some(true)),
        "the daemon never saw the second client attach"
    );
    let mut seen: Vec<u8> = Vec::new();
    rpc.send("usb0", MARKER, false, 10_000)
        .expect("a send into a live endpoint is accepted");
    let got_marker = wait_until(Duration::from_secs(20), || {
        drain_into(&mut b, &mut seen);
        seen.windows(MARKER.len()).any(|w| w == MARKER.as_bytes())
    });
    drain_into(&mut b, &mut seen);
    drop(b);
    assert!(
        got_marker,
        "the fresh client never received the marker sent after it attached, so the \
         absence of stale bytes below would prove nothing: read {} bytes {:?}",
        seen.len(),
        String::from_utf8_lossy(&seen[..seen.len().min(80)])
    );

    // The defect itself.
    let stale_bytes = seen.iter().filter(|b| **b == STALE_BYTE).count();
    assert_eq!(
        stale_bytes,
        0,
        "a fresh console session was handed {stale_bytes} bytes of the *previous* \
         session's output. §7.2: on last close the daemon resets the pair so every \
         client session starts deterministic — resetting termios is only half of \
         that, because the kernel keeps up to a pts queue's worth of undelivered \
         hostward bytes across the close and gives them to the next opener. The \
         client read {} bytes in total",
        seen.len()
    );

    // §5, both halves: the bytes are gone *and* they were named. An unfixed daemon
    // reports no `discarded_at_last_close` at all, which is the silent-loss shape
    // the rule exists to forbid.
    assert!(
        counter(rpc, "console", "discarded_at_last_close") > 0,
        "nothing was charged to `discarded_at_last_close` after a client that never \
         read detached from a full pair: either the queue was never filled (this \
         test then proves nothing) or kilobytes were discarded with no counter \
         moving, which §5 forbids. console={:?}",
        rpc.node("console")
    );
    assert!(
        settled,
        "the console was fanned out {total} bytes and only {} are accounted for \
         ({:?}) — §5 asks every lost byte to be counted where it happens, and the \
         hostward path has three places it can happen: the bounded bridge \
         (`dropped_slow_consumer`), the presence gate (`discarded_no_client`) and \
         the last-close flush (`discarded_at_last_close`)",
        accounted(),
        rpc.node("console")
    );
}
