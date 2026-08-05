//! Cross-platform Rust integration-test harness for serial_nexus.
//!
//! This crate replaces the bash validation scripts under `scripts/validate/**` with
//! portable Rust (design §5). It boots `serial-nexus-daemon` as a subprocess, drives it over
//! the Unix control socket with a small JSON-RPC client, orchestrates `serial-nexus-sim`
//! doubles, and asserts on structured results — with none of the `stat -c` / `jq` /
//! `nc` / `sha256sum` / `timeout` shelling whose flags diverge across Linux and macOS.
//! Portability lives in `std` plus a couple of documented crates (`serde_json`,
//! `sha2`), not in whichever coreutils a given box happens to ship.
//!
//! ## Platform note (macOS)
//!
//! The software-loopback doctrine — a pty standing in for a serial device — does not
//! work on macOS: `serial2` configures a serial port with an ioctl a pty rejects
//! (`ENOTTY`). So tests that need a serial *device* obtain a **lossless** one from
//! [`serial_pair`] (a cross-wired null modem) or [`serial_echo`] (a single echo device)
//! — both Linux-only (a sim pty), returning `None` so the test **skips** elsewhere. The
//! macOS real-hardware serial path is covered by the dedicated `serial_hardware` test
//! (via [`crossover_ports`]), which reads through the daemon's own fast, lossless reader
//! rather than a raw client (a flow-control-less UART drops bytes under a raw high-volume
//! read). The daemon itself is proven on
//! real macOS serial hardware; everything that does not need a serial device (control
//! plane, config, pty, codecs, legs) runs on every platform.
//!
//! ## Conventions
//!
//! * Every helper that can fail in setup panics with a clear message — a broken
//!   harness must fail loudly, never pass vacuously (the anti-tautology rule, §5).
//! * [`Daemon`], [`Sim`], and [`TempRun`] clean up on `Drop` (kill children, remove
//!   the temp dir), so a panicking test never leaks a daemon or a socket.
//! * Ground truth for data-plane claims is a byte-exact SHA-256 ([`sha256_hex`]) or a
//!   sim-reported checksum — never a judgement.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The workspace `target/<profile>/` directory, derived from the running test
/// executable (which lives in `target/<profile>/deps/`).
fn target_dir() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop(); // the test binary's file name
    if p.file_name().map(|n| n == "deps").unwrap_or(false) {
        p.pop(); // out of deps/
    }
    p
}

/// Locate a workspace binary (`serial-nexus-daemon`, `serial-nexus-ctl`, `serial-nexus-sim`,
/// `serial-nexus-doctor`). Requires a prior `cargo build --workspace`: `cargo test` only
/// builds the test-instrumented bins under `target/debug/deps/`, never the plain
/// `target/debug/<name>` artifact this boots, so the workspace must be built first
/// (CI does exactly that — see `.github/workflows/ci.yml`). Panics with guidance otherwise.
pub fn bin(name: &str) -> PathBuf {
    let exe = target_dir().join(name);
    assert!(
        exe.exists(),
        "binary `{name}` not found at {} — run `cargo build --workspace` first \
         (`cargo test` builds only the deps/ test bins, not this plain artifact)",
        exe.display()
    );
    exe
}

/// SHA-256 of `bytes`, lowercase hex — the byte-exact ground truth for data-plane
/// assertions (matches `serial-nexus-sim`'s `sha256_hex`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The `serial-nexus-sim` deterministic byte stream (splitmix64) — the generator behind
/// every `seeded:<size>` payload the sim sends.
///
/// The ground truth a data-plane assertion needs is frequently on a stdout the harness
/// discards (`Sim::spawn` nulls it), so a test that must know *which bytes* were sent
/// reconstructs them from `(seed, len)` and hashes them through [`sha256_hex`]. Eight
/// test files carried a byte-identical copy of this (review 37, 37-TEST-5); one copy
/// with the sim-pinned vectors beside it (`seeded_bytes_matches_the_sim`) is what makes
/// "matches the sim" a checked claim rather than a comment.
pub fn seeded_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut s = seed;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        out.extend_from_slice(&z.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// Current on-disk length of `p`, or 0 if it is not there — the portable replacement
/// for `stat -c %s` / `cat | wc -c`.
///
/// Absent reads as 0 on purpose: every caller polls this inside a [`wait_until`] while
/// a log file is still being created, and "not yet" and "empty" are the same answer to
/// the question being asked.
pub fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// `utime + stime` of `pid` in clock ticks, read from `/proc/<pid>/stat` (Linux).
///
/// The two fields are 14 and 15 (1-based) and the parenthesised `comm` before them may
/// itself contain spaces — including `)` — so the split starts past the **last** `)`.
/// That detail is the reason this is shared rather than re-derived: it is invisible
/// until a process is named something awkward, and it was hand-copied into two test
/// files before this (review 37, 37-TEST-5).
///
/// Panics if the file cannot be read or parsed: a CPU budget measured against an
/// unreadable counter is the vacuous pass §5 forbids.
#[cfg(target_os = "linux")]
pub fn cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .unwrap_or_else(|e| panic!("read /proc/{pid}/stat: {e}"));
    let tail = &stat[stat.rfind(')').expect("comm field is parenthesised") + 1..];
    let fields: Vec<&str> = tail.split_whitespace().collect();
    // `tail` starts at field 3 (state), so utime (14) and stime (15) are at 11 and 12.
    let utime: u64 = fields[11].parse().expect("utime");
    let stime: u64 = fields[12].parse().expect("stime");
    utime + stime
}

/// Poll `cond` until it returns true or `timeout` elapses. Returns whether it became
/// true. The harness's only wait primitive — no bare sleeps (§5).
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A short-lived temp directory used as `XDG_RUNTIME_DIR`. Deliberately under `/tmp`
/// with a short name so the control socket path stays under the `sockaddr_un` limit
/// (~104 bytes on macOS / 108 on Linux, §7). Removed on `Drop`.
pub struct TempRun {
    dir: PathBuf,
}

impl TempRun {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        // No `Math.random`/timestamp needed: pid + a monotonic counter is unique
        // within a run, and each test process gets its own pid.
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("snx-it-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp run dir");
        TempRun { dir }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.join("serial-nexus-daemon.sock")
    }

    pub fn state_file(&self) -> PathBuf {
        self.dir.join("state.toml")
    }
}

impl Default for TempRun {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempRun {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// An RPC-level error returned by the daemon (`{code, message}` from the JSON-RPC
/// `error` object).
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// A tiny JSON-RPC 2.0 client over the daemon's Unix control socket: one request per
/// connection (as `serial-nexus-ctl` does), NDJSON framing (§10). This is the Rust
/// replacement for `serial-nexus-ctl --json … | jq`.
#[derive(Clone)]
pub struct Rpc {
    socket: PathBuf,
    next_id: std::rc::Rc<std::cell::Cell<i64>>,
}

impl Rpc {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Rpc {
            socket: socket.into(),
            next_id: std::rc::Rc::new(std::cell::Cell::new(1)),
        }
    }

    /// Send `method`/`params`, returning the `result` value or the daemon's
    /// `RpcError`. Panics only on a transport failure (socket gone, malformed line) —
    /// a protocol-level error is a normal `Err` a test can assert on.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".into(), json!("2.0"));
        req.insert("id".into(), json!(id));
        req.insert("method".into(), json!(method));
        if !params.is_null() {
            req.insert("params".into(), params);
        }
        let line = format!("{}\n", Value::Object(req));

        let mut stream = UnixStream::connect(&self.socket)
            .unwrap_or_else(|e| panic!("connect {}: {e}", self.socket.display()));
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream.write_all(line.as_bytes()).expect("write request");
        stream.flush().expect("flush request");

        // Read one NDJSON response line.
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    buf.push(byte[0]);
                }
                Err(e) => panic!("read response for `{method}`: {e}"),
            }
        }
        let resp: Value = serde_json::from_slice(&buf).unwrap_or_else(|e| {
            panic!(
                "parse response for `{method}`: {e}; raw={:?}",
                String::from_utf8_lossy(&buf)
            )
        });
        if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
            return Err(RpcError {
                code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// `call` that panics on an RPC error — for the common "this must succeed" path.
    pub fn ok(&self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|e| panic!("`{method}` failed: [{}] {}", e.code, e.message))
    }

    /// The `state` snapshot.
    pub fn state(&self) -> Value {
        self.ok("state", Value::Null)
    }

    /// The node object named `name` from `state`, or `None`.
    pub fn node(&self, name: &str) -> Option<Value> {
        self.state()
            .get("nodes")?
            .as_array()?
            .iter()
            .find(|n| n.get("name").and_then(Value::as_str) == Some(name))
            .cloned()
    }

    /// A node's `.status` string (`""` if the node is absent).
    pub fn node_status(&self, name: &str) -> String {
        self.node(name)
            .and_then(|n| n.get("status").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default()
    }

    /// Wait for a node to reach `status` (through the reconnect transient), returning
    /// whether it did within `timeout`.
    pub fn wait_status(&self, name: &str, status: &str, timeout: Duration) -> bool {
        wait_until(timeout, || self.node_status(name) == status)
    }

    /// `load` a graph config (the JSON shape `dump` returns and `load` accepts, §11).
    pub fn load_config(&self, config: Value, replace: bool) -> Result<Value, RpcError> {
        self.call("load", json!({ "config": config, "replace": replace }))
    }

    /// `load` a config authored as TOML (parsed to the `load` JSON shape here, the way
    /// `serial-nexus-ctl` does before sending).
    pub fn load_toml(&self, toml_cfg: &str, replace: bool) -> Result<Value, RpcError> {
        let v: toml::Value = toml::from_str(toml_cfg).expect("parse test TOML config");
        self.load_config(serde_json::to_value(&v).expect("toml->json"), replace)
    }

    /// The current graph config as JSON (what `load` round-trips).
    pub fn dump(&self) -> Value {
        self.ok("dump", Value::Null)
    }

    /// The `info` result (registry / codec info, §10).
    pub fn info(&self) -> Value {
        self.ok("info", Value::Null)
    }

    /// `add-node` a single node authored as a `[[node]]` TOML block.
    pub fn add_node_toml(&self, node_toml: &str) -> Result<Value, RpcError> {
        let v: toml::Value = toml::from_str(node_toml).expect("parse add-node TOML");
        let node = v
            .get("node")
            .and_then(|n| n.as_array())
            .and_then(|a| a.first())
            .cloned()
            .expect("add_node_toml needs a [[node]] block");
        self.call(
            "add-node",
            json!({ "node": serde_json::to_value(&node).unwrap() }),
        )
    }

    pub fn remove_node(&self, node: &str, cascade: bool) -> Result<Value, RpcError> {
        self.call("remove-node", json!({ "node": node, "cascade": cascade }))
    }

    /// The `ports` result — the resolver's passive device enumeration (§12/§15.35).
    pub fn ports(&self) -> Value {
        self.ok("ports", Value::Null)
    }

    /// `connect` one edge onto the running graph (§15.35). `write_mode` is the
    /// declared mode; `None` leaves it to the config default (`on-demand`).
    pub fn connect(&self, a: &str, b: &str, write_mode: Option<&str>) -> Result<Value, RpcError> {
        self.call(
            "connect",
            json!({ "a": a, "b": b, "write_mode": write_mode }),
        )
    }

    /// `disconnect` the edge between two endpoints (§15.35).
    pub fn disconnect(&self, a: &str, b: &str) -> Result<Value, RpcError> {
        self.call("disconnect", json!({ "a": a, "b": b }))
    }

    /// `send` one line targetward through an endpoint (§6). `steal` takes the lock.
    pub fn send(
        &self,
        endpoint: &str,
        line: &str,
        steal: bool,
        timeout_ms: u64,
    ) -> Result<Value, RpcError> {
        self.call(
            "send",
            json!({ "endpoint": endpoint, "line": line, "timeout_ms": timeout_ms, "steal": steal }),
        )
    }

    pub fn lock(
        &self,
        origin: &str,
        steal: bool,
        wait: bool,
        lease_ms: Option<u64>,
    ) -> Result<Value, RpcError> {
        self.call(
            "lock",
            json!({ "origin": origin, "steal": steal, "wait": wait, "lease_ms": lease_ms }),
        )
    }

    pub fn unlock(&self, origin: &str) -> Result<Value, RpcError> {
        self.call("unlock", json!({ "origin": origin }))
    }

    pub fn send_break(&self, node: &str, ms: u64) -> Result<Value, RpcError> {
        self.call("send-break", json!({ "node": node, "ms": ms }))
    }

    pub fn rotate(&self, node: &str) -> Result<Value, RpcError> {
        self.call("rotate", json!({ "node": node }))
    }

    /// Open a streaming connection (`subscribe` for state notifications, or
    /// `tap.open`/other) and return a [`Subscription`] that yields the id-less
    /// notification lines. The request ack is consumed here (§10).
    pub fn stream(&self, method: &str, params: Value) -> Subscription {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".into(), json!("2.0"));
        req.insert("id".into(), json!(id));
        req.insert("method".into(), json!(method));
        if !params.is_null() {
            req.insert("params".into(), params);
        }
        let line = format!("{}\n", Value::Object(req));
        let stream = UnixStream::connect(&self.socket)
            .unwrap_or_else(|e| panic!("connect {}: {e}", self.socket.display()));
        let mut sub = Subscription {
            stream,
            buf: Vec::new(),
        };
        sub.stream
            .write_all(line.as_bytes())
            .expect("write stream request");
        sub.stream.flush().expect("flush");

        // Consume the ack (a response carrying our id) before notifications flow —
        // **parsed**, not discarded (review 37, 37-TEST-4). `let _ =` threw away three
        // different failures, and all three surfaced later as "timed out", which points
        // diagnosis at the wrong half of the system: an *error* ack (the daemon refusing
        // the subscribe or the tap) read as a healthy stream that then yields nothing; no
        // ack at all read the same way; and a first line that is not the ack — a
        // notification racing it — silently eaten, leaving the stream one message short
        // of what the test is waiting for.
        //
        // The id check is meaningful because §10 orders these: the control loop writes a
        // stream verb's response before it starts draining that connection's
        // notification lane, so anything else arriving first is a real protocol change.
        let ack = sub
            .read_line_until(Instant::now() + Duration::from_secs(10))
            .unwrap_or_else(|| {
                panic!(
                    "`{method}` stream: the daemon sent no ack within 10s (§10 answers a \
                     stream request before any notification flows)"
                )
            });
        let ack: Value = serde_json::from_str(&ack)
            .unwrap_or_else(|e| panic!("`{method}` stream: ack is not JSON: {e}; raw={ack:?}"));
        if let Some(err) = ack.get("error").filter(|e| !e.is_null()) {
            panic!(
                "`{method}` stream refused by the daemon: [{}] {}",
                err.get("code").and_then(Value::as_i64).unwrap_or(0),
                err.get("message").and_then(Value::as_str).unwrap_or("")
            );
        }
        assert_eq!(
            ack.get("id").and_then(Value::as_i64),
            Some(id),
            "`{method}` stream: the first line carries id {:?}, not this request's {id} \
             — a notification was consumed as the ack and the stream starts a message \
             short",
            ack.get("id")
        );
        sub
    }

    /// `subscribe` to the daemon's state-notification stream (§10).
    pub fn subscribe(&self) -> Subscription {
        self.stream("subscribe", Value::Null)
    }

    pub fn teardown(&self) {
        let _ = self.call("teardown", Value::Null);
    }

    /// Ask the daemon to shut down. Best-effort by nature: `shutdown` makes the daemon
    /// exit, and it may close (RST) the connection before flushing a response — a
    /// legitimate outcome, since the process is going away. Whether the response flush
    /// wins the race against teardown is environment-timing-dependent, so unlike
    /// [`Self::call`] (which panics on a read error) this treats a reset/EOF as success,
    /// keeping the shutdown deterministic across environments. The response, if it does
    /// arrive first, is simply drained.
    pub fn shutdown(&self) {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let line = format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"shutdown\"}}\n");
        if let Ok(mut stream) = UnixStream::connect(&self.socket) {
            let _ = stream.write_all(line.as_bytes());
            let _ = stream.flush();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut sink = [0u8; 64];
            let _ = stream.read(&mut sink); // drain a response if any; ignore reset/EOF
        }
    }
}

/// One `info` round trip on `socket`, returning whether the daemon **answered** —
/// the readiness probe behind [`Daemon::start`] (T7).
///
/// Readiness used to be `socket.exists()`, which is true from the instant
/// `UnixListener::bind` creates the inode — before the accept loop runs, and before
/// `startup_load` has finished bringing a persisted graph up. Every test then raced
/// that window with its first RPC. `Rpc::call` *panics* on a transport failure (by
/// design: a broken harness must fail loudly), so the race surfaced as a hard,
/// confusing panic rather than a retry — exactly the flake class `b8d8ed8` had just
/// fixed in the product.
///
/// This is deliberately total: every failure is a `false` (retry), never a panic, so
/// the caller's bounded [`wait_until`] owns the deadline and the error message.
/// `info` is the cheapest verb that proves the whole path — accept, read a line,
/// dispatch, write a response — and it touches no graph state, so probing is free of
/// side effects (the connection is closed immediately after).
///
/// Public so a test that boots its own `serial-nexus-daemon` with extra flags (e.g.
/// `--socket-group`, `p9_permissions.rs`) waits on the same definition of "up" that
/// [`Daemon::start`] does, instead of inventing a weaker one.
pub fn daemon_answers(socket: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return false;
    };
    if stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .is_err()
    {
        return false;
    }
    // id 0 is outside `Rpc`'s own sequence (which starts at 1), so a probe can never
    // be confused with a test's request.
    let line = b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"info\"}\n";
    if stream.write_all(line).is_err() || stream.flush().is_err() {
        return false;
    }
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return false, // closed before answering
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buf.push(byte[0]);
                if buf.len() > 1 << 20 {
                    return false;
                }
            }
            Err(_) => return false, // timeout / reset
        }
    }
    // Ready means *answered our request*, not merely "wrote something".
    serde_json::from_slice::<Value>(&buf)
        .ok()
        .and_then(|v| v.get("id").and_then(Value::as_i64))
        == Some(0)
}

/// A running `serial-nexus-daemon` subprocess with its own temp runtime dir and socket.
/// Killed and cleaned up on `Drop`.
pub struct Daemon {
    child: Child,
    /// The orphan leash (design §15.43): the write end of the daemon's stdin pipe,
    /// held for this `Daemon`'s whole life and never written to.
    ///
    /// [`Drop`] is the *happy* path; this covers every unhappy one. A test process
    /// killed by a signal, aborted, or killed as a process group by a runner executes
    /// no `Drop` at all — but the kernel still closes this fd, the daemon reads EOF and
    /// stops through its normal teardown. Without it such a process leaves a daemon
    /// holding its control socket and every device its graph opened, and the *next*
    /// run of any test wanting those devices fails on a `TIOCEXCL` whose cause is
    /// nowhere in that run's output.
    _leash: std::process::ChildStdin,
    rpc: Rpc,
    run: TempRun,
}

impl Daemon {
    /// Boot a fresh daemon on an empty graph and wait until it **answers RPC** —
    /// not merely until its socket inode appears ([`daemon_answers`], T7).
    pub fn start() -> Self {
        Self::start_with_args(&[])
    }

    /// [`Self::start`] plus extra `serial-nexus-daemon` flags — the seam a test needs to
    /// point the resolver at a fixture tree (`--dev-root`, §12). Kept here rather
    /// than hand-rolled per test so the readiness wait, the temp `XDG_RUNTIME_DIR`
    /// and the kill-on-drop stay in one place; a test that must *restart* the
    /// daemon mid-run still manages its own `Child` (the `p7_*` pattern).
    pub fn start_with_args(extra: &[&str]) -> Self {
        let run = TempRun::new();
        let socket = run.socket();
        let mut child = Command::new(bin("serial-nexus-daemon"))
            .arg("--socket")
            .arg(&socket)
            .arg("--state-file")
            .arg(run.state_file())
            // The leash: this daemon stops when the pipe below is closed, which the
            // kernel does when this test process dies, however it dies (§15.43).
            .arg("--exit-on-stdin-eof")
            .args(extra)
            .env("XDG_RUNTIME_DIR", run.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-daemon");
        let leash = child
            .stdin
            .take()
            .expect("the daemon was spawned with a piped stdin");

        // The guard is constructed *before* the readiness assertion, not after. A bare
        // `std::process::Child` has no kill-on-drop — its `Drop` neither signals nor
        // reaps — so the old order unwound past a running daemon on a readiness timeout
        // and left it behind with nobody holding its pid.
        let daemon = Daemon {
            child,
            _leash: leash,
            rpc: Rpc::new(socket.clone()),
            run,
        };
        let ready = wait_until(Duration::from_secs(10), || {
            socket.exists() && daemon_answers(&socket)
        });
        assert!(
            ready,
            "daemon never answered `info` on {} within 10s (socket present: {})",
            socket.display(),
            socket.exists()
        );
        daemon
    }

    pub fn rpc(&self) -> &Rpc {
        &self.rpc
    }

    pub fn run(&self) -> &TempRun {
        &self.run
    }

    pub fn socket(&self) -> PathBuf {
        self.run.socket()
    }

    /// The daemon subprocess's pid.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Send `signal` (a `kill(1)` name — `"TERM"`, `"INT"`) and wait up to `timeout` for
    /// the daemon to exit, returning its status (`None` if it was still running at the
    /// deadline).
    ///
    /// The seam a clean-exit test needs (review 37, 37-SEAM-1). [`Drop`] issues
    /// `shutdown` and then SIGKILLs — right for a test that is finished, but it means
    /// nothing ever *observed* the post-loop teardown in `serve`: control socket
    /// unlinked, pty symlinks removed, state file left intact. Deleting the signal arms
    /// of that loop passed the whole suite.
    ///
    /// `kill(1)` rather than a raw `kill(2)`: everything outside `serial_nexus_sys` is
    /// `unsafe`-free (invariant 3 / §16.3) and the harness carries no safe signal
    /// wrapper, so it borrows the tool `p5_exec_crash` already uses.
    pub fn signal_and_wait(
        &mut self,
        signal: &str,
        timeout: Duration,
    ) -> Option<std::process::ExitStatus> {
        let pid = self.child.id();
        let sent = Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(pid.to_string())
            .status()
            .expect("run kill(1)");
        assert!(sent.success(), "`kill -{signal} {pid}` failed: {sent}");
        self.wait_for_exit(timeout)
    }

    /// Wait up to `timeout` for the daemon to exit on its own — after a signal, or after
    /// the `shutdown` verb. `None` means it was still running at the deadline.
    pub fn wait_for_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("try_wait on the daemon") {
                Some(exit) => return Some(exit),
                None if Instant::now() >= deadline => return None,
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.rpc.shutdown();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A `serial-nexus-sim` subprocess double (e.g. `pty --echo`), killed on `Drop`. Use
/// [`Sim::client`] for the one-shot `client` verdicts (which run to completion).
pub struct Sim {
    child: Child,
}

impl Sim {
    /// Spawn `serial-nexus-sim` with `args` in the background (a long-lived double such as
    /// `pty --echo --link …`), waiting for `link` to appear if given.
    pub fn spawn(args: &[&str], link: Option<&Path>) -> Self {
        let child = Command::new(bin("serial-nexus-sim"))
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-sim");
        if let Some(link) = link {
            let up = wait_until(Duration::from_secs(5), || link.exists());
            assert!(up, "sim link never appeared at {}", link.display());
        }
        Sim { child }
    }

    /// Run a one-shot `serial-nexus-sim client …` to completion and return its JSON verdict.
    pub fn client(args: &[&str]) -> Value {
        let out = Command::new(bin("serial-nexus-sim"))
            .arg("client")
            .args(args)
            .output()
            .expect("run serial-nexus-sim client");
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "parse sim client verdict: {e}; stdout={:?} stderr={:?}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A single software serial device that echoes what is written to it — the Linux
/// software-loopback "device" for echo round-trip tests. **Not available on macOS** (a
/// pty cannot be a serial device there — `serial2` → `ENOTTY`); those tests skip. Keeps
/// its backing sim + dir alive for its lifetime.
pub struct SerialEcho {
    device: PathBuf,
    _sim: Sim,
    _run: TempRun,
}

impl SerialEcho {
    /// The `/dev`-like path a `serial` node should open as its `device`.
    pub fn device(&self) -> &Path {
        &self.device
    }
}

/// A single echoing serial device, or `None` to **skip**. Linux: a `serial-nexus-sim pty
/// --echo` double. macOS: `None` (no single-port echo hardware; use [`serial_pair`] +
/// real hardware for the crossover path instead).
pub fn serial_echo() -> Option<SerialEcho> {
    #[cfg(target_os = "linux")]
    {
        let run = TempRun::new();
        let device = run.join("serialdev");
        let sim = Sim::spawn(
            &[
                "pty",
                "--echo",
                "--link",
                &device.to_string_lossy(),
                "--timeout-ms",
                "600000",
            ],
            Some(&device),
        );
        return Some(SerialEcho {
            device,
            _sim: sim,
            _run: run,
        });
    }
    #[allow(unreachable_code)]
    None
}

/// Detect a two-port crossover rig: `SNX_CROSSOVER_A`/`_B` if both are set, on any
/// platform; else — **macOS only** — exactly two `/dev/cu.usbserial-*` nodes.
///
/// There is deliberately no Linux by-id arm, and the asymmetry is worth stating where
/// it bites (review 37 `37-DOC-3`): on Linux a physically cross-wired pair is invisible
/// here until both variables are exported, so every rig-gated test self-skips on a box
/// whose rig is attached and working, and a green run reads as hardware coverage that
/// never executed. A doctor P5 reporting Tier 3 says nothing about whether these tests
/// ran — it certifies the rig, not this detection (§15.21).
pub fn crossover_ports() -> Option<(String, String)> {
    if let (Ok(a), Ok(b)) = (
        std::env::var("SNX_CROSSOVER_A"),
        std::env::var("SNX_CROSSOVER_B"),
    ) {
        return Some((a, b));
    }
    #[cfg(target_os = "macos")]
    {
        let mut ports: Vec<String> = std::fs::read_dir("/dev")
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("cu.usbserial"))
                    .unwrap_or(false)
            })
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        ports.sort();
        if ports.len() == 2 {
            return Some((ports[0].clone(), ports[1].clone()));
        }
    }
    None
}

/// Announce a rig-gated test's self-skip — and refuse to skip at all when the
/// operator has said the rig must be exercised.
///
/// **Why this exists.** Measured on a Linux box with the FT232R crossover physically
/// attached and working: `cargo test --test serial_hardware` reported `4 passed` in
/// **0.00s** with every test printing SKIP, against `4 passed` in **10.39s** with
/// `SNX_CROSSOVER_A`/`_B` exported and every test genuinely driving the wire. A green
/// run was hardware coverage that never executed, and nothing in the output
/// distinguished the two — the exact failure mode a self-skip is otherwise safe
/// against, and §9's theme in its plainest form.
///
/// `SNX_CROSSOVER=required` turns every rig self-skip into a hard failure, so a box
/// that has the rig can *prove* its coverage ran rather than assert it. This mirrors
/// `SNX_WEB_UI=required` exactly (plan §3 rule 7) rather than inventing a second
/// mechanism for the same problem.
///
/// The message names the ports it can see, because the one box where the skip matters
/// is the one with the hardware attached: an operator staring at two adapters should
/// not have to already know which variables to export.
pub fn skip_no_rig(test: &str) {
    let seen = rig_seen();
    assert!(
        std::env::var("SNX_CROSSOVER").as_deref() != Ok("required"),
        "SNX_CROSSOVER=required, but {test} found no rig ({seen}).\n\
         Required mode exists so a box with the hardware attached cannot report a \
         green run for coverage that never executed."
    );
    eprintln!("SKIP {test}: no crossover rig ({seen})");
}

/// Serial-adapter device nodes visible on this box, for the skip message only.
///
/// **Reported, never auto-selected.** Two adapters being present is not two adapters
/// being cross-wired, and a harness that opened whatever it found would transmit at
/// 250000 baud and pulse DTR on equipment it never verified — which is the same reason
/// `serial-nexus-doctor` is passive until a port is named with `--port`. Naming the
/// candidates is help; choosing them is the operator's.
fn rig_candidates() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    // Linux: udev's stable names. Falls back to nothing rather than guessing at
    // /dev/ttyUSB*, which enumerates non-adapter ttys on some boxes.
    if let Ok(entries) = std::fs::read_dir("/dev/serial/by-id") {
        found.extend(
            entries
                .flatten()
                .map(|e| e.path().to_string_lossy().into_owned()),
        );
    }
    // macOS: the call-out nodes.
    if let Ok(entries) = std::fs::read_dir("/dev") {
        found.extend(
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("cu.usbserial"))
                        .unwrap_or(false)
                })
                .map(|p| p.to_string_lossy().into_owned()),
        );
    }
    found.sort();
    found
}

/// A streaming connection to the daemon (`subscribe`/`tap.open`), yielding id-less
/// notification lines. Buffers across reads so a timeout never splits a line.
pub struct Subscription {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Subscription {
    /// Read one complete `\n`-terminated line by `deadline`, or `None`.
    fn read_line_until(&mut self, deadline: Instant) -> Option<String> {
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

    /// The next notification JSON within `timeout`, or `None` on timeout/close.
    ///
    /// A complete line that is **not** JSON panics rather than reading as `None`
    /// (review 37, 37-TEST-4). `None` is how a timeout is spelled and [`Self::wait_for`]
    /// treats it as terminal, so mapping a malformed line to `None` made the harness's
    /// loudest possible failure — a daemon writing garbage onto a notification stream —
    /// indistinguishable from its quietest, a daemon writing nothing. A partial line at
    /// EOF is still `None`: [`Self::read_line_until`] only yields `\n`-terminated lines,
    /// so nothing here can fire on a stream that was merely cut short.
    pub fn next(&mut self, timeout: Duration) -> Option<Value> {
        let line = self.read_line_until(Instant::now() + timeout)?;
        Some(
            serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("notification line is not JSON: {e}; raw={line:?}")),
        )
    }

    /// Wait for a notification matching `pred` within `timeout`.
    pub fn wait_for(
        &mut self,
        timeout: Duration,
        mut pred: impl FnMut(&Value) -> bool,
    ) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            match self.next(deadline - now) {
                Some(n) if pred(&n) => return Some(n),
                Some(_) => continue,
                None => return None,
            }
        }
    }
}

/// A cross-wired serial pair — the two ends are each other's target (the no-target
/// doctrine). Backed by a `serial-nexus-sim nullmodem` (two crossed pts), which is **lossless**
/// — byte-exact behavior tests require that. It is deliberately Linux-only:
///
/// * A pty cannot be a serial device on macOS (`serial2` → `ENOTTY`), so there is no
///   software null modem there.
/// * Real macOS crossover *hardware* works, but a flow-control-less UART drops bytes
///   under a *raw* high-volume reader, which would flake a byte-exact assertion. The
///   macOS real-hardware serial path is instead proven by the dedicated
///   `serial_hardware` test, whose reader is the daemon's own (fast, lossless) reader
///   into a `log` node ([`crossover_ports`]).
///
/// Keeps its backing sim + dir alive for its lifetime.
pub struct SerialPair {
    a: String,
    b: String,
    source: PairSource,
    /// `Some` for the software double (the sim and its dir die with the pair);
    /// `None` for the rig, whose ports outlive every test.
    _sim: Option<Sim>,
    _run: Option<TempRun>,
    /// `Some` for the rig: the process-wide claim on the two physical ports, released
    /// when the pair drops. `None` for the software double, which is per-call.
    _claim: Option<MutexGuard<'static, ()>>,
}

impl SerialPair {
    pub fn ports(&self) -> (&str, &str) {
        (&self.a, &self.b)
    }

    /// Which provider backed this pair. A test that reports what it exercised is the
    /// difference between coverage and a claim about coverage (§9).
    pub fn source(&self) -> PairSource {
        self.source
    }
}

/// Which provider backed a [`SerialPair`]. The two are **not** interchangeable — see
/// [`serial_pair_or_rig`] for the contract a caller must satisfy to accept either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSource {
    /// A `serial-nexus-sim nullmodem`: two crossed pts, deterministic, no hardware,
    /// runs in CI, and characterizes as "not a UART".
    Software,
    /// The operator's cross-wired adapters ([`crossover_ports`]): real UARTs, with
    /// device identity, a baud rate that costs wall-clock, and no flow control.
    Rig,
}

/// The provider decision, as a pure function of the three facts that decide it — so
/// the table is checked without hardware (`itest/tests/harness_contract.rs`) rather
/// than only on the one box that has a rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairChoice {
    Software,
    Rig,
    /// Neither provider exists: the caller self-skips (via [`skip_no_pair`]).
    Skip,
    /// `SNX_SERIAL_PAIR=rig` was exported but no rig is visible. Running the software
    /// double here would be §3.35's defect in a new place — an operator instruction
    /// that silently does nothing — so this is a hard failure, not a fallback.
    ForcedRigMissing,
}

/// Pick the provider. **Software wins whenever it exists**, and the rig is a fallback
/// for the platform where the software double does not, never a preference:
///
/// * the sim null modem is deterministic, needs no hardware, and runs in CI;
/// * on a box that has a rig, the rig is already claimed by `serial_hardware.rs` and
///   `p12_serial_exclusivity.rs`, and those tests *need* it — these ones only need two
///   cross-wired ports;
/// * `SNX_CROSSOVER_A`/`_B` are exported on such a box precisely to run those tests, so
///   preferring the rig here would silently move six tests onto hardware (and onto a
///   ~15x slower wire) as a side effect of an unrelated export.
///
/// `SNX_SERIAL_PAIR=rig` forces the fallback arm on any platform. It exists so the arm
/// can be exercised on the platform of record instead of shipping code that only ever
/// runs elsewhere — §9's proxy-in-space rule applied to a provider.
pub fn choose_pair_source(software: bool, rig: bool, force_rig: bool) -> PairChoice {
    match (software, rig, force_rig) {
        (_, true, true) => PairChoice::Rig,
        (_, false, true) => PairChoice::ForcedRigMissing,
        (true, _, false) => PairChoice::Software,
        (false, true, false) => PairChoice::Rig,
        (false, false, false) => PairChoice::Skip,
    }
}

/// Whether the software null modem exists on this platform.
///
/// The one place the platform is spelled for provider selection; [`serial_pair`]'s own
/// `#[cfg(target_os = "linux")]` is the other, and [`serial_pair_or_rig`] fails loudly
/// rather than silently if the two ever disagree.
pub fn software_pair_available() -> bool {
    cfg!(target_os = "linux")
}

/// Serializes the physical ports across the tests of one binary. Process-local is
/// enough and the reason is measured, not assumed: **cargo runs test binaries strictly
/// sequentially** (sampled 12 times during a whole-gate run — `docs/macos.md`), so no
/// two binaries are ever concurrent. Within one binary they are not: three `p8_map`
/// rig tests running on the default thread pool failed 2 of 3 with "serial ends not
/// active" until this existed (2026-08-05, `/dev/ttyUSB0`↔`/dev/ttyUSB1`).
static RIG_CLAIM: Mutex<()> = Mutex::new(());

/// Take the rig claim, recovering from a poisoned mutex exactly as `serial_hardware.rs`
/// does: a panicking rig test must not cascade the rest into poison-panics.
fn rig_claim() -> MutexGuard<'static, ()> {
    RIG_CLAIM
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Discard anything still in flight on a physical port before handing it to a test.
///
/// The software double is built fresh per call and starts empty; the rig is shared
/// state that the previous claimant may have left mid-stream, and every caller here
/// asserts on an *exact* byte count or an exact checksum, where a stale byte is a
/// failure with a misleading name. Non-blocking open, so a port with no carrier cannot
/// hang the harness; best-effort by construction — a port that cannot be opened for
/// draining is the test's problem to report, not this helper's.
fn drain_stale(port: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(mut f) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(port)
    else {
        return;
    };
    let mut buf = [0u8; 4096];
    for _ in 0..64 {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(_) => break, // WouldBlock: the port is quiet
        }
    }
}

/// A lossless cross-wired serial pair, or `None` to **skip**. Linux: a `serial-nexus-sim
/// nullmodem`. Non-Linux: `None` (see the [`SerialPair`] note; the macOS hardware path
/// lives in the `serial_hardware` test via [`crossover_ports`]).
pub fn serial_pair() -> Option<SerialPair> {
    #[cfg(target_os = "linux")]
    {
        let run = TempRun::new();
        let a = run.join("nm-a");
        let b = run.join("nm-b");
        let sim = Sim::spawn(
            &[
                "nullmodem",
                "--link-a",
                &a.to_string_lossy(),
                "--link-b",
                &b.to_string_lossy(),
                "--timeout-ms",
                "600000",
            ],
            Some(&a),
        );
        return Some(SerialPair {
            a: a.to_string_lossy().into_owned(),
            b: b.to_string_lossy().into_owned(),
            source: PairSource::Software,
            _sim: Some(sim),
            _run: Some(run),
            _claim: None,
        });
    }
    #[allow(unreachable_code)]
    None
}

/// A cross-wired pair from **either** provider — the software null modem where it
/// exists, else the operator's rig — or `None` to **skip** (announce it with
/// [`skip_no_pair`]).
///
/// **The contract a caller accepts by using this instead of [`serial_pair`].** The two
/// ports may be real UARTs, so the test must hold only for what "two cross-wired ports"
/// promises, and nothing a pts happens to also be:
///
/// * *no assumption that the ports are ptys.* `serial-nexus-doctor` keys a real port by
///   its canonical `usb:vid:pid:serial:iface` identity, not by path, and characterizes
///   it as a UART rather than "skipped (not a UART)". `p7_p5` asserts both of those the
///   pts way and therefore stays on [`serial_pair`] — measured 2026-08-05: it fails on
///   the rig with `no P5 observation keyed "/dev/ttyUSB0"`.
/// * *volume within the measured envelope.* A UART has no flow control, so a raw
///   high-volume reader can lose bytes. Measured on this rig (FT232R, Linux 7.0.0-29,
///   load 0.5): a raw `serial-nexus-sim --recv` took 64 KiB at 115200 byte-exact 5 of 5
///   and at 230400 3 of 3, and `p4_exclusivity` (64 KiB) and `p4_free_for_all` (32 KiB)
///   pass over the wire. That is the envelope this contract covers; above it, read
///   through the daemon's own reader into a `log` node, as `serial_hardware.rs` does.
/// * *wall clock.* Bytes cost time at a baud rate: `p4_exclusivity` runs 0.08 s on the
///   software double and 5.76 s on the rig. Timeouts must be generous enough for the
///   wire. The three `p4_*` callers pin `baud = 115200` explicitly; the three in
///   `p8_map` name no baud at all and inherit the same rate from `default_baud()`,
///   so the effective rate is uniform but only half the callers state it — do not
///   read this as "every caller pins it", which is what this line used to say.
/// * *exclusivity.* The two ports are one shared resource: the returned pair holds a
///   process-wide claim for its lifetime, so rig-backed tests in one binary serialize.
///
/// Software wins whenever it exists; see [`choose_pair_source`] for why, and for
/// `SNX_SERIAL_PAIR=rig`, which forces the rig arm so it can be exercised on a platform
/// where the software double also exists.
pub fn serial_pair_or_rig() -> Option<SerialPair> {
    let rig = crossover_ports();
    let force_rig = std::env::var("SNX_SERIAL_PAIR").as_deref() == Ok("rig");
    // Decide before building: the software double costs a subprocess, and under
    // `SNX_SERIAL_PAIR=rig` it must not be spawned only to be discarded.
    match choose_pair_source(software_pair_available(), rig.is_some(), force_rig) {
        PairChoice::Software => Some(serial_pair().expect(
            "software_pair_available() says this platform has a null modem but \
             serial_pair() returned None — the two spellings of the platform have \
             drifted (a harness that disagrees with itself must fail loudly, §5)",
        )),
        PairChoice::Rig => {
            let (a, b) = rig.expect("PairChoice::Rig is only chosen when a rig is visible");
            let claim = rig_claim();
            drain_stale(&a);
            drain_stale(&b);
            // Say which provider ran, for the same reason `skip_no_rig` names the ports:
            // a green run that cannot be told from a different green run is §3.35's
            // defect. Hardware and the software double have very different failure
            // modes, and the first question about a red one is which was under it.
            eprintln!(
                "RIG: this test is running on the crossover rig ({a} <-> {b}), not the sim null modem"
            );
            Some(SerialPair {
                a,
                b,
                source: PairSource::Rig,
                _sim: None,
                _run: None,
                _claim: Some(claim),
            })
        }
        PairChoice::Skip => None,
        PairChoice::ForcedRigMissing => panic!(
            "SNX_SERIAL_PAIR=rig, but no crossover rig is visible ({}).\n\
             Falling back to the software null modem would be an operator instruction \
             that silently does nothing — the defect SNX_CROSSOVER=required exists to \
             prevent (§3.35).",
            rig_seen()
        ),
    }
}

/// Announce a [`serial_pair_or_rig`] self-skip — and refuse to skip when the operator
/// has said the rig must be exercised.
///
/// The message this replaces said "no serial device on this platform", which is false
/// on a box with two working adapters, and told the operator to attach a crossover rig,
/// which did nothing because [`serial_pair`] had no rig arm (notes §3.37). Both halves
/// are now true: the remedy named is the one that works, and reaching this line means
/// neither provider exists.
///
/// `SNX_CROSSOVER=required` covers this skip for the same reason it covers
/// [`skip_no_rig`]'s: where this skip is reachable at all — a platform with no software
/// double — the rig is the only provider, so a green skip on a box with the hardware
/// attached is exactly the unexecuted coverage required-mode exists to catch. On Linux
/// the skip is unreachable (the software double never fails), so required-mode neither
/// fires nor needs to.
pub fn skip_no_pair(test: &str) {
    assert!(
        std::env::var("SNX_CROSSOVER").as_deref() != Ok("required"),
        "SNX_CROSSOVER=required, but {test} has no cross-wired pair: this platform has \
         no software null modem and no rig is visible ({}).\n\
         Required mode exists so a box with the hardware attached cannot report a green \
         run for coverage that never executed.",
        rig_seen()
    );
    eprintln!(
        "SKIP {test}: no cross-wired serial pair — no software null modem on this \
         platform (a pty is not a serial device off Linux) and no rig ({}).",
        rig_seen()
    );
}

/// The "what this box can see" clause shared by every rig-related message.
fn rig_seen() -> String {
    let candidates = rig_candidates();
    if candidates.is_empty() {
        "no USB-serial adapters visible either".to_owned()
    } else {
        format!(
            "visible now: {} — export SNX_CROSSOVER_A and SNX_CROSSOVER_B to two \
             cross-wired ones",
            candidates.join(", ")
        )
    }
}

// ---------------------------------------------------------------------------
// The USB replug capability (design §15.45).
//
// `serial-nexus-replug` is the one binary in this workspace meant to carry a Linux
// file capability. Tests never hold `CAP_DAC_OVERRIDE` themselves: they shell out
// to the blessed copy, which validates its own arguments against sysfs and performs
// the two writes. These helpers locate that copy, prove it is actually blessed, and
// translate a stable `/dev/serial/by-id` name into the sysfs port the helper takes.
// ---------------------------------------------------------------------------

/// The blessed-binary directory for the running profile: `<workspace>/.snx-bin/<profile>`.
///
/// Derived from `target/<profile>/` so a `--release` test finds the release copy,
/// and gitignored so it is never committed.
fn blessed_dir() -> PathBuf {
    let target = target_dir();
    let profile = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "debug".to_owned());
    let root = target.parent().and_then(|p| p.parent()).unwrap_or(&target);
    root.join(".snx-bin").join(profile)
}

/// The blessed `serial-nexus-replug`, or why it cannot be used.
///
/// **Proves the blessing rather than assuming it**: the file existing says nothing,
/// because the kernel strips `security.capability` on every rewrite, so a copy left
/// over from before a rebuild is present and powerless. This asks the binary itself
/// — `capabilities --json` reports the effective bit it reads from its own
/// `/proc/self/status` — which is the only answer that cannot be stale.
pub fn blessed_replug_helper() -> Result<PathBuf, String> {
    let path = blessed_dir().join("serial-nexus-replug");
    if !path.exists() {
        return Err(format!(
            "{} is not installed — run `cargo build --workspace && \
             ./target/debug/serial-nexus-replug install`, then the sudo command it prints",
            path.display()
        ));
    }
    let out = std::process::Command::new(&path)
        .args(["capabilities", "--json"])
        .output()
        .map_err(|e| format!("running {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(text.trim()).map_err(|e| {
        format!(
            "{} printed unparseable JSON ({e}): {text:?}",
            path.display()
        )
    })?;
    if json["cap_dac_override_effective"] == serde_json::Value::Bool(true) {
        return Ok(path);
    }
    Err(format!(
        "{} is installed but not blessed ({}). Run:\n    sudo setcap cap_dac_override+ep {}",
        path.display(),
        text.trim(),
        path.display()
    ))
}

/// Announce a replug-gated test's self-skip, and refuse to skip when the operator
/// has said the capability must be exercised.
///
/// Mirrors [`skip_no_rig`] deliberately rather than inventing a second mechanism:
/// `SNX_REPLUG=required` is to this capability what `SNX_CROSSOVER=required` is to
/// the wire. The reason is the same one §3.35 records — a green run for coverage
/// that never executed is worse than a red one.
pub fn skip_no_replug(test: &str, why: &str) {
    assert!(
        std::env::var("SNX_REPLUG").as_deref() != Ok("required"),
        "SNX_REPLUG=required, but {test} cannot replug: {why}"
    );
    eprintln!("SKIP {test}: {why}");
}

/// The sysfs USB port name (`3-1`) backing a `/dev/serial/by-id` link, or `None`.
///
/// Unprivileged throughout, and it is the *test* that does this translation rather
/// than the helper: keeping `/dev` names out of the capability-carrying binary is
/// what lets its only device argument be a validated port name instead of a path.
///
/// The by-id link is the right input for a replug test for a reason the operation
/// itself demonstrates: re-enumeration can hand the adapter a different `ttyUSBn`,
/// so a `/dev/ttyUSB0` argument names something that may not survive the test.
pub fn usb_port_of(by_id_link: &Path) -> Option<String> {
    let tty = by_id_link.canonicalize().ok()?;
    let tty_name = tty.file_name()?.to_string_lossy().into_owned();
    // Resolve `/sys/class/tty/<tty>/device` **fully** and walk up, rather than
    // reading the one link and taking its name. Measured on 7.0.0-29: that link
    // points at `../../../ttyUSB0` — the usb-serial port device, not the USB
    // interface — so the obvious one-step form finds no colon and answers `None`
    // for a perfectly healthy adapter. The canonical path is
    // `/sys/devices/…/usb3/3-1/3-1:1.0/ttyUSB0`, and the first ancestor whose name
    // carries a colon is the interface `3-1:1.0`, whose prefix is the port.
    let resolved = std::fs::canonicalize(format!("/sys/class/tty/{tty_name}/device")).ok()?;
    for ancestor in resolved.ancestors() {
        if let Some(name) = ancestor.file_name()
            && let Some((port, _)) = name.to_string_lossy().split_once(':')
        {
            return Some(port.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`Subscription`] over one end of a socket pair, with the other end handed back
    /// so a test can play a badly-behaved daemon. The harness's own failure modes need a
    /// peer that misbehaves on purpose, which no real daemon does — and the point of
    /// these three is exactly that the harness must not translate misbehaviour into
    /// silence.
    fn paired_subscription() -> (Subscription, UnixStream) {
        let (client, server) = UnixStream::pair().expect("socketpair");
        (
            Subscription {
                stream: client,
                buf: Vec::new(),
            },
            server,
        )
    }

    #[test]
    fn a_well_formed_notification_line_parses() {
        let (mut sub, mut server) = paired_subscription();
        server
            .write_all(b"{\"method\":\"state\",\"params\":{}}\n")
            .expect("write the notification");
        let note = sub.next(Duration::from_secs(5)).expect("a notification");
        assert_eq!(note.get("method").and_then(Value::as_str), Some("state"));
    }

    /// The legitimate `None`s must stay `None`, or the guard below has replaced one
    /// misdiagnosis with another: a closed stream is how every `wait_for` loop ends.
    #[test]
    fn a_closed_stream_still_reads_as_none() {
        let (mut sub, server) = paired_subscription();
        drop(server);
        assert!(
            sub.next(Duration::from_secs(5)).is_none(),
            "a closed stream must read as `None`, not panic"
        );
    }

    /// The sim-pinned vectors behind [`seeded_bytes`] (review 37, 37-TEST-5).
    ///
    /// `seeded_bytes` claims to reproduce what `serial-nexus-sim` sends, and eight test
    /// files rested byte-exact assertions on that claim while nothing checked it. These
    /// two digests are the sim's own `sha256_sent`, captured from
    ///
    /// ```text
    /// serial-nexus-sim pty --echo --link $D/dev --timeout-ms 30000 &
    /// serial-nexus-sim client --path $D/dev --send seeded:1KiB  --expect echo --seed 7
    /// serial-nexus-sim client --path $D/dev --send seeded:64KiB --expect echo --seed 7
    /// ```
    ///
    /// so a change to either generator now fails here instead of turning every
    /// byte-exact data-plane assertion in the suite into a comparison of two identical
    /// mistakes. Regenerate with the commands above if the sim's stream ever changes on
    /// purpose.
    const SIM_SEEDED_VECTORS: &[(u64, usize, &str)] = &[
        (
            7,
            1024,
            "901f39cd35b7f9ba4b44bbad8d9afc718b5c30ba6eed0e17595918d53d66fbc7",
        ),
        (
            7,
            65536,
            "ccf5943c6c514320eb6a681df9e22cf3b6cd7dd90eb1385df4ac01244dc310a7",
        ),
    ];

    #[test]
    fn seeded_bytes_matches_the_sim() {
        for &(seed, len, want) in SIM_SEEDED_VECTORS {
            let got = seeded_bytes(seed, len);
            assert_eq!(
                got.len(),
                len,
                "seeded_bytes({seed}, {len}) is the wrong length"
            );
            assert_eq!(
                sha256_hex(&got),
                want,
                "seeded_bytes({seed}, {len}) no longer reproduces `serial-nexus-sim`'s \
                 stream — every byte-exact assertion that reconstructs ground truth \
                 from a seed is now comparing the harness against itself"
            );
        }
    }

    /// A length that is not a multiple of the generator's 8-byte word is where a
    /// re-derivation goes wrong (`truncate`, not "round up"), and several callers ask
    /// for exactly such a length.
    #[test]
    fn seeded_bytes_is_a_prefix_at_any_length() {
        let full = seeded_bytes(7, 1024);
        for len in [0usize, 1, 7, 8, 9, 1000] {
            assert_eq!(
                seeded_bytes(7, len),
                full[..len],
                "seeded_bytes(7, {len}) is not the {len}-byte prefix of the same stream"
            );
        }
    }

    #[test]
    fn file_len_reads_zero_for_an_absent_file() {
        let run = TempRun::new();
        let p = run.join("nothing-here");
        assert_eq!(file_len(&p), 0);
        std::fs::write(&p, b"0123456789").expect("write");
        assert_eq!(file_len(&p), 10);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cpu_ticks_reads_this_process_and_never_goes_backwards() {
        let pid = std::process::id();
        let before = cpu_ticks(pid);
        // Burn a little, so the counter has something to move on. Not a timing
        // assertion: the claim is only that the field is found and is monotone.
        let mut acc = 0u64;
        for i in 0..5_000_000u64 {
            acc = acc.wrapping_add(i);
        }
        assert!(acc > 0);
        assert!(
            cpu_ticks(pid) >= before,
            "utime+stime went backwards — the field offsets past the comm field are wrong"
        );
    }

    /// **Review 37, 37-TEST-4.** A complete line that is not JSON used to map to `None`
    /// — the same value a timeout produces, and [`Subscription::wait_for`] treats `None`
    /// as terminal. A daemon emitting garbage therefore read as a daemon emitting
    /// nothing, and every diagnosis started at the wrong end.
    #[test]
    #[should_panic(expected = "notification line is not JSON")]
    fn a_malformed_notification_line_panics_instead_of_reading_as_a_timeout() {
        let (mut sub, mut server) = paired_subscription();
        server
            .write_all(b"<html>503 Service Unavailable</html>\n")
            .expect("write the garbage line");
        let _ = sub.next(Duration::from_secs(5));
    }
}
