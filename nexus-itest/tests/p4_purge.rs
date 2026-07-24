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
//! count + SHA-256 from a `nexus-sim pty --sink` standing in for the device — never
//! a judgement (§15.17). The serial node opens that sim pts as its device, which is
//! the software-loopback doctrine: a pty cannot stand in for a serial device on
//! macOS (serial2 → `ENOTTY`), so these tests self-skip off Linux (a skip is a
//! valid verdict, §5), the same discipline the bash hardware rig used.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Daemon, Rpc, Sim, bin, wait_until};
use serde_json::{Value, json};

/// A locked-out writer's backlog. Fits the PTY kernel buffer (so `write_all` never
/// blocks and the bytes are counted exactly), and doubles as the exact expected
/// purge count.
const SB: u64 = 2048;
/// Deterministic payload seed shared by sender and sink, so a checksum comparison —
/// not a judgement — decides "the same bytes arrived".
const SEED: &str = "13";

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

/// The device stand-in: a background `nexus-sim pty --sink` that publishes `link`
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
        let child = Command::new(bin("nexus-sim"))
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
            .expect("spawn nexus-sim pty --sink");
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

/// Spawn a locked-out client that writes `SB` seeded bytes and holds the slave open
/// (so it stays `present` while we detach or acquire around it).
fn spawn_holding_client(tty_b: &Path) -> Sim {
    Sim::spawn(
        &[
            "client",
            "--path",
            &tty_b.to_string_lossy(),
            "--send",
            &format!("seeded:{SB}"),
            "--seed",
            SEED,
            "--hold-ms",
            "5000",
            "--timeout-ms",
            "8000",
        ],
        None,
    )
}

/// Check 1 — the 3 a.m. hazard + purge-on-detach: a locked-out client's backlog is
/// purged-and-counted on detach and never fires, even though the lock was free.
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
    let client = spawn_holding_client(&tty_b);
    assert!(
        wait_until(Duration::from_secs(5), || client_present(rpc)),
        "locked-out client never became present: {:?}",
        rpc.node("ptyb")
    );

    // No holder, nothing purged yet: its bytes are simply buffered (§6).
    assert_eq!(
        holder(rpc),
        Value::Null,
        "endpoint has a holder it should not"
    );
    assert_eq!(
        purged(rpc, "ptyb"),
        Some(0),
        "purged should be 0 before detach"
    );

    // Detach the client: its backlog is purged-on-detach, counted exactly, and never
    // fires — the lock was free the whole time, but a non-holder's bytes never fire.
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
    // and holds the slave open.
    let client = spawn_holding_client(&tty_b);
    assert!(
        wait_until(Duration::from_secs(5), || client_present(rpc)),
        "client never became present: {:?}",
        rpc.node("ptyb")
    );
    assert_eq!(
        purged(rpc, "ptyb"),
        Some(0),
        "purged should be 0 before acquire"
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
    assert!(
        wait_until(Duration::from_secs(5), || purged(rpc, "ptyb") == Some(SB)),
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
    let v = sink.verdict();
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
