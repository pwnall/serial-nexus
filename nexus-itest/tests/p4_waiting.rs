//! Phase 4 arbitration — the FIFO waiter queue is fair and cancel-safe, ported from
//! `scripts/validate/phase4/waiting.sh` (design §6 arbitration / §15.20 two-lane
//! control plane).
//!
//! The original stands a `nexus-sim pty --sink` in for the device so the serial host
//! endpoint is *active*. That device is unnecessary here: the write lock, its origins,
//! and its FIFO queue are a **structural** property of the graph edges (built at wiring
//! time in `runtime.rs`, independent of device liveness), and the serial supervisor
//! holds the targetward receiver across the `waiting` state, so `send`'s serviceability
//! check still passes. So `usb0` is a device-absent `serial` node (`waiting`) and every
//! assertion — arrival-order grants, per-grant purge-on-acquire, cancel-on-disconnect,
//! the contended-`send` LOCKED error, and the teardown / cascade-remove wakeups — runs
//! on **every platform with no serial device** (RULE 2: pty/control nodes run
//! everywhere). Only the purge case needs a PTY client (a `nexus-sim client` on `ptyb`),
//! which works on Linux and macOS alike.
//!
//! Each section of the source script becomes one self-contained `#[test]` with its own
//! daemon (RULE 4). Assertions pin to structured RPC state and byte-exact purge counts,
//! never CLI text (RULE 1).

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Rpc, Sim, TempRun, wait_until};
use serde_json::{Value, json};

/// `nexus-rpc` `AppError::Locked` (base `-32000` minus its ordinal `3`). A contended
/// `lock`/`send` fails with this code (design §6/§10). Asserting the code is the
/// portable replacement for the script's `grep -qi 'lock'` on stderr.
const LOCKED_CODE: i64 = -32003;

/// ptyb's pre-grant backlog: the client types this many bytes while ptyb is *not* the
/// holder (so they buffer), then the queued grant purges exactly them (§6).
const PRE: u64 = 64;

/// The graph the script loads: three PTY writers fanning targetward into one serial
/// host endpoint. `usb0`'s device is absent, so it comes up `waiting` — its write lock,
/// origins, and queue exist regardless (structural, §6).
fn cfg(run: &TempRun) -> String {
    format!(
        r#"
[[node]]
type = "pty"
name = "ptya"
path = "{ta}"
[[node]]
type = "pty"
name = "ptyb"
path = "{tb}"
[[node]]
type = "pty"
name = "ptyc"
path = "{tc}"
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
[[edge]]
a = "usb0"
b = "ptyc"
"#,
        ta = run.join("ttyA").display(),
        tb = run.join("ttyB").display(),
        tc = run.join("ttyC").display(),
        dev = run.join("absent-device").display(),
    )
}

/// A parked `lock --wait` control connection — the Rust stand-in for a background
/// `serialnexusctl lock <origin> --wait`. It sends the request and keeps the socket
/// open so the daemon parks it in the FIFO queue (`Rpc::call` can't model this: it
/// reads the one response and returns). The test later reads the eventual grant/deny
/// with [`Self::response`], or dequeues it by dropping the connection
/// ([`Self::cancel`]), which the daemon treats as a disconnect (§15.20).
struct LockWaiter {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl LockWaiter {
    /// Connect, send `lock {origin, wait:true}`, and leave it parked.
    fn spawn(socket: &Path, origin: &str) -> LockWaiter {
        let mut stream = UnixStream::connect(socket).expect("connect lock waiter");
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "lock",
            "params": { "origin": origin, "steal": false, "wait": true, "lease_ms": null },
        });
        let line = format!("{req}\n");
        stream
            .write_all(line.as_bytes())
            .expect("write lock --wait request");
        stream.flush().expect("flush lock --wait request");
        LockWaiter {
            stream,
            buf: Vec::new(),
        }
    }

    /// Read the JSON-RPC response object (a grant or a defined error) within `timeout`,
    /// or `None` if still parked / the connection closed. The waiter never subscribes,
    /// so its only inbound line is its own response.
    fn response(&mut self, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                return serde_json::from_slice(&line[..line.len() - 1]).ok();
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            self.stream.set_read_timeout(Some(deadline - now)).ok();
            let mut tmp = [0u8; 4096];
            match self.stream.read(&mut tmp) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(_) => return None, // WouldBlock/TimedOut/closed
            }
        }
    }

    /// Cancel the parked wait by closing the connection — the §15.20 cancel-on-disconnect
    /// the daemon must observe to dequeue a killed waiter.
    fn cancel(self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// The `.lock` object of the named node from a `state` snapshot.
fn lock_of<'a>(state: &'a Value, node: &str) -> Option<&'a Value> {
    state
        .get("nodes")?
        .as_array()?
        .iter()
        .find(|n| n.get("name").and_then(Value::as_str) == Some(node))?
        .get("lock")
}

/// The current lock holder of `node` (`.lock.holder`), if any.
fn holder(rpc: &Rpc, node: &str) -> Option<String> {
    let st = rpc.state();
    lock_of(&st, node)?
        .get("holder")?
        .as_str()
        .map(str::to_owned)
}

/// The FIFO waiter queue of `node` (`.lock.waiters`), front = next to be granted.
fn waiters(rpc: &Rpc, node: &str) -> Vec<String> {
    let st = rpc.state();
    lock_of(&st, node)
        .and_then(|l| l.get("waiters"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

/// The purge-on-acquire byte count recorded for `origin` on `node`'s lock
/// (`.lock.origins[origin].purged`).
fn purged(rpc: &Rpc, node: &str, origin: &str) -> u64 {
    let st = rpc.state();
    lock_of(&st, node)
        .and_then(|l| l.get("origins"))
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|o| o.get("origin").and_then(Value::as_str) == Some(origin))
        })
        .and_then(|o| o.get("purged"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// Poll until `node`'s waiter queue equals `want`, within `timeout`.
fn wait_waiters(rpc: &Rpc, node: &str, want: &[&str], timeout: Duration) -> bool {
    let want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
    wait_until(timeout, || waiters(rpc, node) == want)
}

/// Load the graph and wait for the three PTY writers to come up active.
fn load_graph(rpc: &Rpc, run: &TempRun) {
    rpc.load_toml(&cfg(run), false).expect("load graph");
    for p in ["ptya", "ptyb", "ptyc"] {
        assert!(
            rpc.wait_status(p, "active", Duration::from_secs(5)),
            "{p} did not come up active: {:?}",
            rpc.node(p)
        );
    }
}

/// A plain `lock <origin>` must acquire, returning `acquired: true`.
fn lock_acquired(rpc: &Rpc, origin: &str) {
    let r = rpc
        .lock(origin, false, false, None)
        .unwrap_or_else(|e| panic!("lock {origin} failed: [{}] {}", e.code, e.message));
    assert_eq!(
        r.get("acquired").and_then(Value::as_bool),
        Some(true),
        "lock {origin} did not acquire: {r}"
    );
}

/// A `lock --wait` response must be a fresh grant (`.result.acquired == true`).
fn assert_granted(resp: &Value, who: &str, ctx: &str) {
    assert_eq!(
        resp.pointer("/result/acquired").and_then(Value::as_bool),
        Some(true),
        "{who}'s --wait was not granted {ctx}: {resp}"
    );
}

/// A `lock --wait` response must be a defined error, not a spurious grant.
fn assert_errored(resp: &Value, ctx: &str) {
    assert!(
        resp.get("error").is_some() && resp.get("result").is_none(),
        "the --wait wrongly succeeded {ctx}: {resp}"
    );
}

// ============================================================================
// A — FIFO across an unlock and a detach-release; purge-on-acquire per grant.
// ============================================================================
#[test]
fn fifo_grants_in_arrival_order_and_purges_on_each_acquire() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    load_graph(rpc, run);

    // ptyb gets a client that types PRE bytes (buffered, since ptyb is not the holder)
    // and holds the slave open, so it can later detach-release. Kept in an `Option` so
    // the test can kill it (drop) at the detach point.
    let tb = run.join("ttyB");
    let mut clb = Some(Sim::spawn(
        &[
            "client",
            "--path",
            &tb.to_string_lossy(),
            "--send",
            &format!("seeded:{PRE}"),
            "--seed",
            "5",
            "--hold-ms",
            "30000",
            "--timeout-ms",
            "35000",
        ],
        None,
    ));
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("ptyb")
                .and_then(|n| n.get("client_present").and_then(Value::as_bool))
                == Some(true)
        }),
        "ptyb client never became present"
    );

    // ptya takes the lock; two --wait waiters enqueue in arrival order.
    lock_acquired(rpc, "ptya");
    let mut wb = LockWaiter::spawn(&d.socket(), "ptyb");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb"], Duration::from_secs(5)),
        "ptyb did not enqueue (waiters={:?})",
        waiters(rpc, "usb0")
    );
    let mut wc = LockWaiter::spawn(&d.socket(), "ptyc");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb", "ptyc"], Duration::from_secs(5)),
        "queue not [ptyb,ptyc] (got {:?})",
        waiters(rpc, "usb0")
    );

    // Unlock ptya: the head (ptyb) is granted — arrival order — and its queued grant
    // runs purge-on-acquire, discarding its PRE pre-grant bytes exactly.
    rpc.unlock("ptya").expect("unlock ptya");
    let rb = wb
        .response(Duration::from_secs(10))
        .expect("ptyb's --wait did not return after unlock");
    assert_granted(&rb, "ptyb", "after unlock");
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptyb"),
        "holder not ptyb after unlock (got {:?})",
        holder(rpc, "usb0")
    );
    assert!(
        wait_waiters(rpc, "usb0", &["ptyc"], Duration::from_secs(5)),
        "queue not [ptyc] after ptyb granted (got {:?})",
        waiters(rpc, "usb0")
    );
    assert!(
        wait_until(Duration::from_secs(3), || purged(rpc, "usb0", "ptyb")
            == PRE),
        "purge-on-acquire on the queued grant did not count {PRE} (got {})",
        purged(rpc, "usb0", "ptyb")
    );

    // Detach ptyb's client: detach-release frees the lock and the head (ptyc) is
    // granted next — the second grant path.
    drop(clb.take()); // kill the sim client → slave closes → detach-release
    let rc = wc
        .response(Duration::from_secs(10))
        .expect("ptyc's --wait did not return after ptyb's detach-release");
    assert_granted(&rc, "ptyc", "after ptyb's detach-release");
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptyc"),
        "holder not ptyc after detach-release (got {:?})",
        holder(rpc, "usb0")
    );
    assert!(
        wait_waiters(rpc, "usb0", &[], Duration::from_secs(5)),
        "queue not empty after ptyc granted (got {:?})",
        waiters(rpc, "usb0")
    );
    rpc.unlock("ptyc").expect("unlock ptyc");
}

// ============================================================================
// B — cancel-safety: killing the first waiter dequeues it; the second is granted.
// ============================================================================
#[test]
fn cancelled_head_waiter_is_dequeued_and_the_next_is_granted() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    load_graph(rpc, run);

    lock_acquired(rpc, "ptya");
    let wb = LockWaiter::spawn(&d.socket(), "ptyb");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb"], Duration::from_secs(5)),
        "ptyb did not enqueue"
    );
    let mut wc = LockWaiter::spawn(&d.socket(), "ptyc");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb", "ptyc"], Duration::from_secs(5)),
        "queue not [ptyb,ptyc] (got {:?})",
        waiters(rpc, "usb0")
    );

    // Kill the first waiter's control connection: the daemon dequeues it (§15.20) and
    // the queue shrinks to [ptyc].
    wb.cancel();
    assert!(
        wait_waiters(rpc, "usb0", &["ptyc"], Duration::from_secs(5)),
        "cancelled waiter not dequeued (got {:?})",
        waiters(rpc, "usb0")
    );

    // Unlock ptya: ptyc (not the cancelled ptyb) is granted next.
    rpc.unlock("ptya").expect("unlock ptya");
    let rc = wc
        .response(Duration::from_secs(10))
        .expect("ptyc's --wait did not return after the cancelled ptyb");
    assert_granted(&rc, "ptyc", "after the cancelled ptyb");
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptyc"),
        "holder not ptyc after cancel (got {:?})",
        holder(rpc, "usb0")
    );
    rpc.unlock("ptyc").expect("unlock ptyc");
}

// ============================================================================
// C — a deadline `send` against a stubborn holder returns LOCKED, queue intact.
// ============================================================================
#[test]
fn deadline_send_against_a_held_lock_returns_locked_and_leaves_the_queue_intact() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    load_graph(rpc, run);

    lock_acquired(rpc, "ptya");
    let wb = LockWaiter::spawn(&d.socket(), "ptyb");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb"], Duration::from_secs(5)),
        "ptyb did not enqueue"
    );

    // A deadline send against the held lock must fail with the LOCKED app error.
    let err = rpc
        .send("usb0", "nope", false, 400)
        .expect_err("deadline send should have failed against a held lock");
    assert_eq!(
        err.code, LOCKED_CODE,
        "deadline send failed, but not with the LOCKED error: [{}] {}",
        err.code, err.message
    );

    // The pre-existing waiter queue is untouched by the transient send origin.
    assert_eq!(
        waiters(rpc, "usb0"),
        vec!["ptyb".to_string()],
        "the send disturbed the queue (got {:?})",
        waiters(rpc, "usb0")
    );

    wb.cancel();
    rpc.unlock("ptya").expect("unlock ptya");
}

// ============================================================================
// E — remove-node --cascade of a WAITING writer's node wakes its parked --wait
// (§6/§15.20, DLC-1): unregistering a queued waiter from a SURVIVING host lock
// must wake it so it leaves with a defined error, not park until an unrelated wake.
// ============================================================================
#[test]
fn cascade_removing_a_waiting_writers_node_wakes_its_parked_wait_with_an_error() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    load_graph(rpc, run);

    lock_acquired(rpc, "ptya");
    let mut wc = LockWaiter::spawn(&d.socket(), "ptyc");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyc"], Duration::from_secs(5)),
        "ptyc did not enqueue"
    );

    // Remove the *waiting* writer's node while ptya still holds usb0's lock. usb0
    // survives; ptyc's origin is unregistered from usb0's lock and its parked --wait
    // must return promptly rather than hang until its own timeout.
    rpc.remove_node("ptyc", true)
        .expect("remove-node ptyc --cascade failed");
    let resp = wc.response(Duration::from_secs(4)).expect(
        "the --wait did not return after the waiter's node was cascade-removed (stuck waiter)",
    );
    assert_errored(&resp, "after its origin was cascade-removed");

    // usb0 is unharmed: ptya still holds it and the queue is empty.
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptya"),
        "usb0 holder not ptya after cascade-remove of waiter (got {:?})",
        holder(rpc, "usb0")
    );
    assert!(
        wait_waiters(rpc, "usb0", &[], Duration::from_secs(5)),
        "usb0 queue not empty after cascade-remove of waiter (got {:?})",
        waiters(rpc, "usb0")
    );
    rpc.unlock("ptya").expect("unlock ptya");
}

// ============================================================================
// D — teardown wakes a parked waiter with a defined error (§6/§15.20): a
// deadline-less `lock --wait` must not hang forever when its endpoint vanishes.
// ============================================================================
#[test]
fn teardown_wakes_a_parked_deadline_less_waiter_with_a_defined_error() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    load_graph(rpc, run);

    lock_acquired(rpc, "ptya");
    let mut wb = LockWaiter::spawn(&d.socket(), "ptyb");
    assert!(
        wait_waiters(rpc, "usb0", &["ptyb"], Duration::from_secs(5)),
        "ptyb did not enqueue"
    );

    rpc.teardown();
    // The parked --wait must RETURN promptly (not hang until its own timeout), and with
    // an error — not a spurious grant.
    let resp = wb
        .response(Duration::from_secs(4))
        .expect("the --wait did not return after teardown (stuck waiter)");
    assert_errored(&resp, "after teardown");
}
