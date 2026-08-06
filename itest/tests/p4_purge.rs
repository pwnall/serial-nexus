//! Phase 4 arbitration purge rules, ported from `scripts/validate/phase4/purge.sh`
//! (design §6). Three properties of the write-lock's purge machinery:
//!
//!  1. **The 3 a.m. hazard + purge-on-detach.** A locked-out client types into its
//!     kernel buffer, never acquires, and detaches: its stale backlog is purged and
//!     counted exactly — and never fires, even though the lock was free the whole
//!     time (a non-holder's bytes never reach the device).
//!  2. **Purge-on-acquire.** Pre-grant bytes written *before* acquiring are drained
//!     and discarded on the grant, counted exactly, and never reach the device.
//!  3. **The grant purge is synchronous.** With no client attached at acquire time
//!     there is nothing to purge, so a correct acquire-*before*-write client loses
//!     nothing: its post-grant command reaches the device byte-for-byte.
//!
//! Ground truth for "the device received nothing / everything" is an exact byte
//! count + SHA-256 from a `serial-nexus-sim pty --sink` standing in for the device — never
//! a judgement (§15.17). The serial node opens that sim pts as its device, which is
//! the software-loopback doctrine: a pty cannot stand in for a serial device on
//! macOS (serial2 → `ENOTTY`), so these tests self-skip off Linux (a skip is a
//! valid verdict, §5), the same discipline the bash hardware rig used.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Rpc, Sim, SlaveWitness, attach_slave, bin, observed_while_open, seeded_bytes,
    settled_while_open, wait_until,
};

/// A locked-out writer's backlog. Fits the PTY kernel buffer (so `write_all` never
/// blocks and the bytes are counted exactly), and doubles as the exact expected
/// purge count.
const SB: u64 = 2048;
/// Deterministic payload seed shared by sender and sink, so a checksum comparison —
/// not a judgement — decides "the same bytes arrived". [`SEED_N`] is the same number
/// for the calls that generate the stream in-process rather than through the sim's
/// `--seed` argv; `seeded_bytes_matches_the_sim` is what keeps the two generators one
/// generator.
const SEED: &str = "13";
const SEED_N: u64 = 13;

/// One ptyb origin on one serial endpoint (exclusive by default, §6): the client
/// writes into `ptyb`, whose backlog flows targetward to `usb0`'s device. A fresh
/// device (a `pty --sink`) per check makes "the device received nothing" exact.
fn purge_config(tty_b: &Path, device: &Path) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "ptyb"
path = "{ttyb}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[edge]]
a = "usb0"
b = "ptyb"
"#,
        ttyb = tty_b.display(),
        dev = device.display(),
    )
}

/// Self-skip off Linux, where a pty cannot back a serial device (see the module
/// note). Returns `true` (and prints the skip line) when the test must not run.
fn skip_off_linux(fn_name: &str) -> bool {
    if cfg!(target_os = "linux") {
        false
    } else {
        eprintln!(
            "SKIP {fn_name}: software-loopback serial sink is Linux-only (serial2 → ENOTTY on a pty)"
        );
        true
    }
}

/// The device stand-in: a background `serial-nexus-sim pty --sink` that publishes `link`
/// (the serial node's device path) and counts + checksums the bytes it receives.
/// Its stdout is piped so [`Sink::verdict`] can read the machine-readable result —
/// unlike [`Sim::spawn`], which nulls stdout. Killed and reaped on `Drop`, so a
/// panicking test never leaks the sim.
struct Sink {
    child: Child,
}

impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Sink {
    /// Spawn the sink and wait for its device link to appear before returning.
    /// `bytes` is the sink's capacity (it exits once that many arrive, else on
    /// `--timeout-ms`); pass a value ≥ the payload for the pass-through check and a
    /// large one for the "received nothing" checks (whose bound is just liveness).
    fn spawn(link: &Path, bytes: &str, timeout_ms: &str) -> Sink {
        let child = Command::new(bin("serial-nexus-sim"))
            .args([
                "pty",
                "--sink",
                "--bytes",
                bytes,
                "--timeout-ms",
                timeout_ms,
                "--link",
            ])
            .arg(link)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-sim pty --sink");
        assert!(
            wait_until(Duration::from_secs(5), || link.exists()),
            "sink device link never appeared at {}",
            link.display()
        );
        Sink { child }
    }

    /// Read the sink's stdout to EOF — which blocks until the sim exits (it prints
    /// its single JSON verdict line only on exit) — then parse the verdict. `self`
    /// drops at the end, so [`Drop`] reaps the child.
    fn verdict(mut self) -> Value {
        let mut out = Vec::new();
        if let Some(mut stdout) = self.child.stdout.take() {
            stdout.read_to_end(&mut out).expect("read sink stdout");
        }
        serde_json::from_slice(&out).unwrap_or_else(|e| {
            panic!(
                "parse sink verdict: {e}; stdout={:?}",
                String::from_utf8_lossy(&out)
            )
        })
    }
}

/// The `usb0` endpoint's write-lock holder (§6), or `Value::Null` if unheld.
fn holder(rpc: &Rpc) -> Value {
    rpc.node("usb0")
        .and_then(|n| n.get("lock").and_then(|l| l.get("holder").cloned()))
        .unwrap_or(Value::Null)
}

/// Bytes purged from `origin`'s targetward backlog on the `usb0` endpoint lock
/// (§6), or `None` if the origin has no lock entry yet.
fn purged(rpc: &Rpc, origin: &str) -> Option<u64> {
    rpc.node("usb0")?
        .get("lock")?
        .get("origins")?
        .as_array()?
        .iter()
        .find(|o| o.get("origin").and_then(Value::as_str) == Some(origin))
        .and_then(|o| o.get("purged").and_then(Value::as_u64))
}

/// Whether a client holds `ptyb`'s slave (`client_present`, §7.2).
fn client_present(rpc: &Rpc) -> bool {
    rpc.node("ptyb")
        .and_then(|n| n.get("client_present").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Load the graph and wait for both nodes to come up. The device link already
/// exists (the sink is spawned first), so the serial node opens it at create time
/// and reports `active`.
fn load_and_activate(rpc: &Rpc, tty_b: &Path, device: &Path) {
    rpc.load_toml(&purge_config(tty_b, device), false)
        .expect("load purge graph");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(10)),
        "usb0 (serial) not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("ptyb", "active", Duration::from_secs(5)),
        "ptyb (pty) not active: {:?}",
        rpc.node("ptyb")
    );
}

/// Attach a locked-out client **from this process**, write `SB` seeded bytes into its
/// kernel buffer, and hand the still-open slave back so the caller owns the detach.
///
/// **Why the harness writes rather than a `serial-nexus-sim client --hold-ms`
/// subprocess** (notes §3.56): the two checks that use this both need a *positive*
/// pre-close witness that `SB` bytes really are queued, and the only party that can
/// give one is the writer. `write_all` returning `Ok(())` for a `SB`-byte slice is
/// exactly that statement — the kernel accepted every one of them — and the sim
/// reports the same fact only in a verdict it prints on exit, which is the close these
/// checks are measuring. `SB` is sized to fit the pty buffer on both kernels, so the
/// call never blocks and the count is exact rather than "as much as drained".
///
/// Two lesser properties come with it. The `--hold-ms 5000` these calls used to carry
/// was a timer, §9's proxy in time: on a loaded box a hold can expire before the
/// detach the test means to observe, and nothing in the graph distinguishes that from
/// a correct run. And an owned `File` makes the detach a `drop` the compiler can see,
/// which is what [`serial_nexus_itest::settled_while_open`] is built on.
///
/// The payload is the same seeded stream the sim would have sent, and it is
/// deliberately not filtered for control bytes: §7.2 promises the daemon has already
/// put the pair in raw mode, so a byte-for-byte `SB` reaching the purge counter is a
/// *check* of that promise. A pair left in cooked mode would expand `0x0a` under
/// `OPOST`/`ONLCR` and redden the exact-count assertion, which is the direction a
/// guard is supposed to fail in.
fn hold_a_locked_out_backlog(tty_b: &Path) -> SlaveWitness {
    let mut slave = attach_slave(tty_b);
    let payload = seeded_bytes(SEED_N, SB as usize);
    assert_eq!(
        payload.len() as u64,
        SB,
        "the harness oracle sized the payload wrong"
    );
    slave
        .write_all(&payload)
        .unwrap_or_else(|e| panic!("write {SB} locked-out bytes into {}: {e}", tty_b.display()));
    slave
        .flush()
        .unwrap_or_else(|e| panic!("flush the locked-out backlog: {e}"));
    slave
}

/// Check 1 — the 3 a.m. hazard + purge-on-detach: a locked-out client's backlog is
/// purged-and-counted on detach and never fires, even though the lock was free.
///
/// **This is one of the two deliberate exceptions to notes §3.29's rule**, and it is
/// worth saying why in full, because the rule's whole point is that exceptions are
/// argued rather than assumed (§3.56).
///
/// The rule is *a byte counter is read while the client that fed it is still open*,
/// because reading it afterwards asserts that the kernel retained the bytes across the
/// slave's last close — which doctor P13 measures as `retains` on Linux 7.0.0-29
/// (`docs/doctor/linux-7.0-2026-08-05-tier3.json`) and `waits-then-discards` on Darwin
/// 24.6.0 (`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`), so it is a kernel
/// property and not a promise. Here it is **this increment's trigger** that is
/// close-only: purge-on-detach fires *because of* the detach, so the one reading that
/// names it cannot be taken before it. That is the exception, and it is narrower than
/// the sentence that used to stand here.
///
/// **Corrected 2026-08-05 (notes §3.60), because the old wording was falsified twelve
/// lines away.** It said the counter "comes into existence because of the close and
/// reads 0 before it". The counter is `purged`, and the very next test in this file —
/// [`pre_grant_backlog_is_purged_on_acquire_and_never_reaches_device`] — watches that
/// same field reach `SB` **while its client is still open**, inside
/// `settled_while_open`, because purge-on-*acquire* bumps it too. The field exists and
/// moves mid-session; what is close-only is the *edge* this test is about. A guard that
/// misstates which of the two it depends on is the kind of claim §5 says to argue
/// rather than assume, and this one would have licensed the wrong simplification: that
/// `purged` cannot be read early, when it demonstrably can.
///
/// **And "unconvertible" belongs to one assertion, not to the guard.** Everything below
/// up to the detach is already taken with the session proven open — the three readings
/// listed at the end of this comment. What cannot move above the close is the single
/// post-close `purged == SB` assertion, because the increment it names has not happened
/// yet. Same shape as `p8_map`'s `a_closing_writers_residual_is_forwarded_not_purged`,
/// and treated the same way — the test is Linux-gated (via [`skip_off_linux`], for the
/// software serial sink) on a kernel P13 has measured, and it does not move to a
/// kernel it has not.
///
/// What the exception owes in exchange is that the post-close number must not be the
/// *only* evidence, and it no longer is. Before the detach this test now establishes,
/// with the session proven open at the instant of each reading:
///
/// * a **positive** witness that `SB` bytes are queued — [`hold_a_locked_out_backlog`]
///   wrote them from this process and `write_all` returned, so the kernel accepted
///   every one. The previous witness was `purged == 0`, a zero, which is equally true
///   of a client that wrote nothing at all;
/// * that the daemon saw the client attach (`client_present`), so the origin exists;
/// * that nothing has been purged yet, which is now a *second* fact about a backlog
///   already known to exist rather than the only one.
#[test]
fn non_holder_backlog_is_purged_on_detach_and_never_reaches_device() {
    if skip_off_linux("non_holder_backlog_is_purged_on_detach_and_never_reaches_device") {
        return;
    }
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let tty_b = run.join("ttyB");
    let device = run.join("dev1");

    // Fresh sink device so "the device received nothing" is an exact byte count. The
    // 6 s timeout is a liveness bound only — no byte ever flows to the device here.
    let sink = Sink::spawn(&device, "1048576", "6000");
    load_and_activate(rpc, &tty_b, &device);

    // A locked-out client types SB bytes into its kernel buffer and holds the slave.
    // It never acquired, so under the exclusive default the daemon does not read it.
    // `write_all` has already returned inside the helper, which is the positive
    // witness: exactly SB bytes are in that buffer, right now, unread.
    let mut client = hold_a_locked_out_backlog(&tty_b);
    let present = settled_while_open(
        &mut [&mut client],
        "the locked-out client's presence",
        Duration::from_secs(5),
        || client_present(rpc),
    );
    assert!(
        present,
        "locked-out client never became present: {:?}",
        rpc.node("ptyb")
    );

    // No holder, nothing purged yet: its bytes are simply buffered (§6). Both readings
    // are taken with the session proven open, so "nothing purged" is a statement about
    // a backlog that demonstrably exists rather than about an empty pty.
    let quiet = settled_while_open(
        &mut [&mut client],
        "the pre-detach state of a locked-out backlog",
        Duration::from_secs(5),
        || holder(rpc) == Value::Null && purged(rpc, "ptyb") == Some(0),
    );
    assert!(
        quiet,
        "before the detach the endpoint must be holderless with nothing purged, \
         got holder={:?} purged={:?}",
        holder(rpc),
        purged(rpc, "ptyb")
    );

    // Detach the client: its backlog is purged-on-detach, counted exactly, and never
    // fires — the lock was free the whole time, but a non-holder's bytes never fire.
    // This close is the exception (see the doc comment): the counter below is the
    // close's own product, so it cannot be read before it.
    drop(client);
    assert!(
        wait_until(Duration::from_secs(5), || purged(rpc, "ptyb") == Some(SB)),
        "purge-on-detach did not count exactly {SB}, got {:?}",
        purged(rpc, "ptyb")
    );

    // The device saw none of it (the 3 a.m. command never fired).
    let v = sink.verdict();
    assert_eq!(
        v.get("received").and_then(Value::as_u64),
        Some(0),
        "device received bytes from a non-holder (the 3 a.m. command fired): {v}"
    );

    rpc.teardown();
}

/// Check 2 — purge-on-acquire: bytes written *before* acquiring are drained and
/// discarded on the grant, counted exactly, and never reach the device.
#[test]
fn pre_grant_backlog_is_purged_on_acquire_and_never_reaches_device() {
    if skip_off_linux("pre_grant_backlog_is_purged_on_acquire_and_never_reaches_device") {
        return;
    }
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let tty_b = run.join("ttyB");
    let device = run.join("dev2");

    let sink = Sink::spawn(&device, "1048576", "6000");
    load_and_activate(rpc, &tty_b, &device);

    // The client writes SB bytes BEFORE acquiring (the incorrect-but-guarded case)
    // and holds the slave open. This check was never in §3.29's class — every counter
    // it reads is read while the client is open — but it shares the writer, and the
    // writer's positive witness (`write_all` returned for SB bytes) is what makes
    // "purged exactly SB" below a statement about a known backlog.
    let mut client = hold_a_locked_out_backlog(&tty_b);
    let present = settled_while_open(
        &mut [&mut client],
        "the pre-grant client's presence",
        Duration::from_secs(5),
        || client_present(rpc) && purged(rpc, "ptyb") == Some(0),
    );
    assert!(
        present,
        "client never became present with an unpurged backlog: {:?} purged={:?}",
        rpc.node("ptyb"),
        purged(rpc, "ptyb")
    );

    // Acquire: purge-on-acquire drains and discards the pre-grant backlog, counted.
    let ack = rpc.lock("ptyb", false, false, None).expect("lock ptyb");
    assert_eq!(
        ack.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock ptyb was not acquired: {ack}"
    );
    assert_eq!(
        holder(rpc),
        json!("ptyb"),
        "ptyb should hold the lock after acquire"
    );
    let counted = settled_while_open(
        &mut [&mut client],
        "purge-on-acquire's count",
        Duration::from_secs(5),
        || purged(rpc, "ptyb") == Some(SB),
    );
    assert!(
        counted,
        "purge-on-acquire did not discard+count exactly {SB}, got {:?}",
        purged(rpc, "ptyb")
    );

    // The purged pre-grant bytes never reached the device.
    drop(client);
    let v = sink.verdict();
    assert_eq!(
        v.get("received").and_then(Value::as_u64),
        Some(0),
        "device received pre-grant bytes (purge-on-acquire leaked): {v}"
    );

    rpc.teardown();
}

/// Check 3 — the grant purge is synchronous at grant time, so a correct
/// acquire-BEFORE-write client loses nothing: the daemon drains at the moment of
/// the grant (nothing is buffered, no client attached), and the client's later
/// command reaches the device intact, byte-for-byte, with nothing purged.
#[test]
fn synchronous_grant_lets_a_post_grant_command_through_intact() {
    if skip_off_linux("synchronous_grant_lets_a_post_grant_command_through_intact") {
        return;
    }
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let tty_b = run.join("ttyB");
    let device = run.join("dev3");

    // A sink sized to exactly the payload: it exits the instant SB bytes arrive.
    let sink = Sink::spawn(&device, &SB.to_string(), "15000");
    load_and_activate(rpc, &tty_b, &device);

    // Acquire FIRST (no client attached, so nothing to purge), THEN write.
    let ack = rpc.lock("ptyb", false, false, None).expect("lock ptyb");
    assert_eq!(
        ack.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock ptyb was not acquired: {ack}"
    );

    // The post-grant session, attached **after** the grant so the check's premise
    // ("no client attached at acquire time, so nothing to purge") is untouched, and
    // held across the device's byte count (notes §3.29 / §3.56, plan §3 rule 8). The
    // one-shot `Sim::client` below closes the moment its `write_all` returns, i.e.
    // when the kernel accepted the last byte rather than when the daemon read it, so
    // `received == SB` afterwards used to assert that the kernel had retained the tail
    // across the slave's last close — true on Linux (doctor P13 `retains`), false on
    // Darwin (`waits-then-discards`). With this fd open the sim's exit is not the last
    // close, so the count is taken against a live session.
    let mut session = attach_slave(&tty_b);

    // The post-grant command: a one-shot client that sends SB seeded bytes and exits.
    let client = Sim::client(&[
        "--path",
        &tty_b.to_string_lossy(),
        "--send",
        &format!("seeded:{SB}"),
        "--seed",
        SEED,
        "--timeout-ms",
        "15000",
    ]);
    assert_eq!(
        client.get("pass").and_then(Value::as_bool),
        Some(true),
        "post-grant client failed: {client}"
    );
    let sent_sha = client
        .get("sha256_sent")
        .and_then(Value::as_str)
        .expect("client reported sha256_sent");

    // The post-grant command reaches the device intact, byte-for-byte, with nothing
    // purged — a racy (lazy-drain) purge would have discarded or corrupted it.
    let v = observed_while_open(
        &mut [&mut session],
        "the device's count of a post-grant command",
        || sink.verdict(),
    );
    // The session ends here, and nowhere earlier: moving the observation below this
    // line is `E0382`.
    drop(session);
    assert_eq!(
        v.get("received").and_then(Value::as_u64),
        Some(SB),
        "post-grant command did not reach the device (a racy purge discarded it): {v}"
    );
    assert_eq!(
        v.get("sha256").and_then(Value::as_str),
        Some(sent_sha),
        "post-grant command corrupted en route: {v}"
    );
    assert_eq!(
        purged(rpc, "ptyb"),
        Some(0),
        "purge-on-acquire wrongly counted post-grant bytes: {:?}",
        purged(rpc, "ptyb")
    );

    rpc.shutdown();
}
