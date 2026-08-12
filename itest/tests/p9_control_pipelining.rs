#![forbid(unsafe_code)]

//! Control-plane pipelining behind an in-flight *waiting* verb (design §15.20;
//! review CTRL-1/CP-1, reproduction 5).
//!
//! `serve_connection` races a waiting verb (`lock --wait`, a contended `send`)
//! against a second read on the same connection. That second lane must
//! **discriminate** what it read:
//!
//! * **EOF / read error** — a disconnect. §15.20 makes cancel-on-disconnect
//!   normative: the parked waiter is dequeued and the connection closes. This half is
//!   design-correct and is guarded here too ([`eof_on_the_request_half_cancels_the_parked_wait`]),
//!   so a future "fix" for the half below cannot quietly take it with it
//!   (`implementation-notes §3.14`, CTRL-3 — declined on purpose).
//! * **A pipelined request line** — *not* a disconnect. The reviewer reproduced the
//!   old behavior on a raw socket: `lock --wait` for `ptyb` while `ptya` holds, then
//!   `state` on the same connection → the connection was torn down with **no reply to
//!   either request** and `ptyb` never appeared in `waiters`. Through the §17 web
//!   console — where one daemon connection carries `subscribe`, every tap and every
//!   `send` — that silently cost the operator their whole session. The shipped fix
//!   answers the pipelined request with `AppError::WaitInFlight` carrying *its own*
//!   id and leaves the wait intact.
//!
//! Every test here drives the daemon over a **raw** control connection (the harness's
//! `Rpc` is one request per connection by construction, so it cannot express
//! pipelining) and asserts on structured JSON-RPC objects and `state`, never CLI text
//! (§5). No serial *device* is needed — a write lock, its origins and its FIFO queue
//! are structural properties of the graph edges, and the serial supervisor holds the
//! targetward receiver while `waiting` (the p4 doctrine) — so these run on **every**
//! platform.

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use serial_nexus_itest::{Daemon, Rpc, TempRun, wait_until};

/// `serial_nexus_rpc::AppError::WaitInFlight` — the application code (base `-32000` minus its
/// ordinal `6`) a request pipelined behind a parked waiting verb is refused with; see
/// the error-code registry in `docs/rpc/README.md` (§16.8). Asserting the *code* is the
/// stable contract; the message is only spot-checked.
const WAIT_IN_FLIGHT: i64 = -32006;

/// JSON-RPC's parse error (`id: null`), for the unparseable-pipelined-line case.
const PARSE_ERROR: i64 = -32700;

/// Two PTY writers fanning targetward into one host-facing serial endpoint whose
/// device is absent (so it loads `waiting` — its lock, origins and queue exist
/// regardless, §15.8/§6). `ptya` takes the lock; `ptyb` is the origin whose
/// `lock --wait` parks.
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
        ta = run.join("ttyA").display(),
        tb = run.join("ttyB").display(),
        dev = run.join("absent-device").display(),
    )
}

/// A raw control connection that can hold several requests in flight — what the
/// harness's one-request-per-connection [`Rpc`] deliberately cannot model, and what
/// a §17 browser's single daemon connection actually is.
struct Conn {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Conn {
    fn open(socket: &Path) -> Conn {
        let stream = UnixStream::connect(socket).expect("connect control socket");
        Conn {
            stream,
            buf: Vec::new(),
        }
    }

    /// Write one request line. Never waits for its reply — the point of the file.
    fn request(&mut self, id: i64, method: &str, params: Value) {
        let mut req = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if !params.is_null() {
            req["params"] = params;
        }
        self.write_line(&format!("{req}"));
    }

    /// Write one raw line (used for the deliberately unparseable case).
    fn write_line(&mut self, line: &str) {
        self.stream
            .write_all(format!("{line}\n").as_bytes())
            .expect("write request line");
        self.stream.flush().expect("flush request line");
    }

    /// Read one `\n`-terminated line by `timeout`, buffering across reads, or `None`
    /// on timeout / close.
    fn read_line(&mut self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                return Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            self.stream.set_read_timeout(Some(deadline - now)).ok();
            let mut tmp = [0u8; 8192];
            match self.stream.read(&mut tmp) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(_) => return None, // WouldBlock/TimedOut/closed
            }
        }
    }

    /// The next JSON object on the connection, whatever its id.
    fn next_object(&mut self, timeout: Duration) -> Option<Value> {
        let line = self.read_line(timeout)?;
        Some(
            serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON ({e}): {line}")),
        )
    }

    /// The response carrying `id`, skipping anything else (a notification, or a reply
    /// to a different request), or `None` if it never arrives.
    fn response(&mut self, id: i64, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let v = self.next_object(deadline - now)?;
            if v.get("id").and_then(Value::as_i64) == Some(id) {
                return Some(v);
            }
        }
    }

    /// Half-close the *request* half — the §15.20 disconnect signal, and the one thing
    /// that is allowed to cancel a parked waiting verb.
    fn half_close(&mut self) {
        self.stream
            .shutdown(Shutdown::Write)
            .expect("shutdown the write half");
    }

    /// Whether the daemon closed its side within `timeout` (a read returning EOF).
    /// Any bytes that arrive first are reported, since an unexpected reply is a
    /// materially different failure than a silent close.
    fn closed_by_daemon(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err("still open".into());
            }
            self.stream.set_read_timeout(Some(deadline - now)).ok();
            let mut tmp = [0u8; 8192];
            match self.stream.read(&mut tmp) {
                Ok(0) => return Ok(()), // EOF: the daemon closed, as §15.20 says
                Ok(n) => {
                    return Err(format!(
                        "daemon sent {:?} instead of closing",
                        String::from_utf8_lossy(&tmp[..n])
                    ));
                }
                Err(_) => continue, // timed out on this read; keep waiting to the deadline
            }
        }
    }
}

/// The FIFO waiter queue of `node`'s endpoint lock (`.lock.waiters`), front first.
fn waiters(rpc: &Rpc, node: &str) -> Vec<String> {
    rpc.node(node)
        .and_then(|n| n.get("lock").cloned())
        .and_then(|l| l.get("waiters").cloned())
        .and_then(|w| w.as_array().cloned())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The current holder of `node`'s endpoint lock, if any.
fn holder(rpc: &Rpc, node: &str) -> Option<String> {
    rpc.node(node)?
        .get("lock")?
        .get("holder")?
        .as_str()
        .map(str::to_owned)
}

/// Boot a daemon on the two-writer graph and take `usb0`'s lock for `ptya`, so any
/// further acquisition parks.
fn daemon_with_a_held_lock() -> Daemon {
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&cfg(d.run()), false).expect("load graph");
    for p in ["ptya", "ptyb"] {
        assert!(
            rpc.wait_status(p, "active", Duration::from_secs(10)),
            "{p} did not come up active: {:?}",
            rpc.node(p)
        );
    }
    let acquired = rpc.lock("ptya", false, false, None).expect("lock ptya");
    assert_eq!(
        acquired.get("acquired").and_then(Value::as_bool),
        Some(true),
        "ptya did not acquire the lock: {acquired}"
    );
    d
}

/// Park `conn` on `lock ptyb --wait` (request id 1) and confirm the daemon queued it.
fn park_lock_wait(conn: &mut Conn, rpc: &Rpc) {
    conn.request(
        1,
        "lock",
        json!({ "origin": "ptyb", "steal": false, "wait": true, "lease_ms": null }),
    );
    assert!(
        wait_until(Duration::from_secs(5), || waiters(rpc, "usb0")
            == vec!["ptyb".to_string()]),
        "ptyb never parked in the FIFO queue (waiters={:?})",
        waiters(rpc, "usb0")
    );
}

// ============================================================================
// CTRL-1/CP-1 — a pipelined request is refused; the wait, and the connection,
// both survive.
// ============================================================================
#[test]
fn pipelined_request_is_refused_while_the_waiting_verb_survives() {
    let d = daemon_with_a_held_lock();
    let rpc = d.rpc();

    let mut conn = Conn::open(&d.socket());
    park_lock_wait(&mut conn, rpc);

    // Reproduction 5, on the same connection: a second request while the first is
    // parked. It must be *answered*, with its own id and the registered code — not
    // swallowed, and not fatal to the connection.
    conn.request(2, "state", Value::Null);
    let refusal = conn
        .response(2, Duration::from_secs(5))
        .expect("the pipelined request went unanswered (the connection was torn down)");
    assert!(
        refusal.get("result").is_none(),
        "the pipelined request was served, not refused: {refusal}"
    );
    let err = refusal.get("error").expect("a JSON-RPC error object");
    assert_eq!(
        err.get("code").and_then(Value::as_i64),
        Some(WAIT_IN_FLIGHT),
        "pipelined request refused with the wrong code: {refusal}"
    );
    let msg = err.get("message").and_then(Value::as_str).unwrap_or("");
    assert!(
        msg.contains("waiting verb"),
        "the refusal does not name the cause: {msg:?}"
    );

    // The wait is intact: ptyb is still queued, ptya still holds. (The defect left
    // `waiters` empty — the parked verb had been cancelled along with the connection.)
    assert_eq!(
        waiters(rpc, "usb0"),
        vec!["ptyb".to_string()],
        "the pipelined request disturbed the FIFO queue"
    );
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptya"),
        "the holder changed while a request was pipelined"
    );

    // A second pipelined request is refused the same way — the refusal loop keeps
    // serving the connection rather than falling out of it after one.
    conn.request(3, "dump", Value::Null);
    let refusal2 = conn
        .response(3, Duration::from_secs(5))
        .expect("the second pipelined request went unanswered");
    assert_eq!(
        refusal2.pointer("/error/code").and_then(Value::as_i64),
        Some(WAIT_IN_FLIGHT),
        "second pipelined request: {refusal2}"
    );

    // The waiting verb still resolves normally when the holder releases.
    rpc.unlock("ptya").expect("unlock ptya");
    let grant = conn
        .response(1, Duration::from_secs(10))
        .expect("the parked `lock --wait` never answered after the unlock");
    assert_eq!(
        grant.pointer("/result/acquired").and_then(Value::as_bool),
        Some(true),
        "the parked wait did not resolve into a grant: {grant}"
    );
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptyb"),
        "ptyb was granted but is not the holder"
    );

    // And the connection is still a working control connection afterwards: with no
    // waiting verb in flight, a request is served normally.
    conn.request(4, "state", Value::Null);
    let after = conn
        .response(4, Duration::from_secs(5))
        .expect("the connection died after the wait resolved");
    assert!(
        after.get("result").is_some(),
        "a post-wait request was not served: {after}"
    );

    rpc.unlock("ptyb").expect("unlock ptyb");
}

// ============================================================================
// The web-console shape (reproduction 5, second half): the waiting verb is a
// contended `send`, and the pipelined request is the console's `state`.
// ============================================================================
#[test]
fn pipelined_request_during_a_contended_send_is_refused_and_the_send_completes() {
    let d = daemon_with_a_held_lock();
    let rpc = d.rpc();

    // `send` self-acquires the write lock (§6); ptya holds it, so this parks with a
    // generous deadline — exactly what an operator typing into a locked console does.
    let mut conn = Conn::open(&d.socket());
    conn.request(
        1,
        "send",
        json!({ "endpoint": "usb0", "line": "hello", "timeout_ms": 30000, "steal": false }),
    );
    assert!(
        wait_until(Duration::from_secs(5), || waiters(rpc, "usb0")
            == vec!["send".to_string()]),
        "the contended send never parked (waiters={:?})",
        waiters(rpc, "usb0")
    );

    // The console then clicks another view: a `state` on the same WS-backed connection.
    conn.request(2, "state", Value::Null);
    let refusal = conn
        .response(2, Duration::from_secs(5))
        .expect("the pipelined `state` went unanswered — the session went silent");
    assert_eq!(
        refusal.pointer("/error/code").and_then(Value::as_i64),
        Some(WAIT_IN_FLIGHT),
        "pipelined `state` during a send: {refusal}"
    );

    // Releasing the lock lets the parked send complete on that same connection.
    rpc.unlock("ptya").expect("unlock ptya");
    let sent = conn
        .response(1, Duration::from_secs(10))
        .expect("the parked `send` never answered after the unlock");
    assert!(
        sent.get("result").is_some(),
        "the parked send did not complete: {sent}"
    );
}

// ============================================================================
// The sanctioned half, unchanged: EOF on the request half still cancels.
// ============================================================================
#[test]
fn eof_on_the_request_half_cancels_the_parked_wait() {
    let d = daemon_with_a_held_lock();
    let rpc = d.rpc();

    let mut conn = Conn::open(&d.socket());
    park_lock_wait(&mut conn, rpc);

    // §15.20 cancel-on-disconnect: a half-close is indistinguishable from a killed
    // client at read time, so it dequeues the waiter and ends the connection. This is
    // normative (CTRL-3, declined on purpose) — the CTRL-1 discrimination must not
    // have widened into "keep serving after EOF".
    conn.half_close();
    assert!(
        wait_until(Duration::from_secs(5), || waiters(rpc, "usb0").is_empty()),
        "the parked waiter was not dequeued on EOF (waiters={:?})",
        waiters(rpc, "usb0")
    );
    if let Err(why) = conn.closed_by_daemon(Duration::from_secs(5)) {
        panic!("the connection was not closed after EOF on the request half: {why}");
    }

    // No spurious grant: ptya still holds, and the queue stays empty.
    assert_eq!(
        holder(rpc, "usb0").as_deref(),
        Some("ptya"),
        "the cancelled waiter took the lock"
    );
    assert!(waiters(rpc, "usb0").is_empty(), "the queue refilled itself");
}

// ============================================================================
// An unparseable pipelined line: refused with the spec's null id, wait intact.
// ============================================================================
#[test]
fn unparseable_pipelined_line_is_refused_with_a_null_id_and_the_wait_survives() {
    let d = daemon_with_a_held_lock();
    let rpc = d.rpc();

    let mut conn = Conn::open(&d.socket());
    park_lock_wait(&mut conn, rpc);

    // Not JSON at all. The second lane cannot echo an id it never parsed, so the
    // spec's null id applies — and, like a pipelined request, it is a refusal and not
    // a disconnect.
    conn.write_line("this is not json");
    let refusal = conn
        .next_object(Duration::from_secs(5))
        .expect("the unparseable line went unanswered (the connection was torn down)");
    assert_eq!(
        refusal.get("id"),
        Some(&Value::Null),
        "an unparseable request must be refused with the null id: {refusal}"
    );
    assert_eq!(
        refusal.pointer("/error/code").and_then(Value::as_i64),
        Some(PARSE_ERROR),
        "unparseable pipelined line: {refusal}"
    );
    assert_eq!(
        waiters(rpc, "usb0"),
        vec!["ptyb".to_string()],
        "the unparseable line disturbed the parked wait"
    );

    // The wait still resolves on the same connection.
    rpc.unlock("ptya").expect("unlock ptya");
    let grant = conn
        .response(1, Duration::from_secs(10))
        .expect("the parked `lock --wait` never answered after the unlock");
    assert_eq!(
        grant.pointer("/result/acquired").and_then(Value::as_bool),
        Some(true),
        "the parked wait did not resolve into a grant: {grant}"
    );
    rpc.unlock("ptyb").expect("unlock ptyb");
}
