#![forbid(unsafe_code)]

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

// ---------------------------------------------------------------------------
// The read-after-close rule: notes §3.29 / design §15.46 / plan §3 rule 8.
// ---------------------------------------------------------------------------

/// Something whose being **open** is a precondition of a byte-counter reading, and
/// which can prove it at the instant that counter is read.
///
/// The two implementations answer the same question with different syscalls, and both
/// are cheap enough to run on every observation:
///
/// * [`SlaveWitness`] — a held pty (or serial) slave, proven by comparing the fd's
///   device identity against a **fresh stat of the path it was opened through**. An
///   `fstat` on the fd alone proves nothing here and used to be all this did; see
///   [`SlaveWitness::prove_open`] for the measurement that retired it.
/// * [`Sim`] — a subprocess is open while it has not exited, which
///   `Child::try_wait` answers without reaping a live child. This one is **not**
///   decorative: every `Sim` held open across an observation is held by a
///   `--hold-ms` timer, and a timer is §9's proxy in time. The check converts "the
///   hold might have expired under load" from a silent vacuous pass into a named
///   failure.
///
/// [`std::fs::File`] deliberately does **not** implement this trait. It cannot: a
/// `File` does not remember the path it came from, so it cannot answer the only
/// question worth asking, and an impl on it would let any of this suite's plain
/// client fds be handed in as a witness that proves nothing (notes §3.60).
pub trait OpenWitness {
    /// How this witness names itself in a failure message.
    fn label(&self) -> String;
    /// `Ok(())` if this witness is demonstrably still open *now*, else why not.
    fn prove_open(&mut self) -> Result<(), String>;
}

/// A pty or serial slave held open as a witness, **carrying the path it was opened
/// through** — because an fd on its own cannot answer whether the far end is still
/// there.
///
/// Constructed by [`attach_slave`] (which opens it) or [`adopt_slave`] (which takes an
/// fd the caller opened with its own flags). Both record the device identity at
/// adoption so a failure can say what changed.
///
/// It forwards [`std::io::Read`] and [`std::io::Write`] to the underlying fd, because
/// several callers *are* the client — `p4_purge` types a locked-out backlog through
/// exactly this handle — and splitting "the writer" from "the witness" would put two
/// fds on one slave and change what the tests measure.
pub struct SlaveWitness {
    file: std::fs::File,
    path: PathBuf,
    /// `(st_dev, st_ino, st_rdev)` at adoption — reported by a failure so the message
    /// names the device that went away rather than only the one that is there now.
    adopted_as: DeviceIdentity,
}

/// The identity of a device node: the filesystem it lives on, its inode there, and the
/// `(major, minor)` of the device it names. All three come out of one `stat`, and all
/// three are compared, because they fail independently — a repointed symlink usually
/// moves `st_rdev`, while a devtmpfs node destroyed and recreated at the same
/// `(major, minor)` (what a USB replug does, §12) moves only `st_ino`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceIdentity {
    dev: u64,
    ino: u64,
    rdev: u64,
}

impl DeviceIdentity {
    fn of(md: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            dev: md.dev(),
            ino: md.ino(),
            rdev: md.rdev(),
        }
    }
}

impl std::fmt::Display for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `rdev` raw rather than decoded to `major:minor`. The decode is not the same
        // arithmetic on Linux and Darwin, and a witness failure that printed a
        // plausible-looking wrong pair on one of them would cost a reader more than the
        // raw number does.
        write!(f, "dev={} ino={} rdev={}", self.dev, self.ino, self.rdev)
    }
}

impl OpenWitness for SlaveWitness {
    fn label(&self) -> String {
        format!("the held slave {}", self.path.display())
    }

    /// Prove the fd still names a device that is still there — and say precisely what
    /// that does and does not establish, because the previous version of this function
    /// established **nothing**.
    ///
    /// It was `self.metadata().map(|_| ())`, and that is a tautology. Measured on this
    /// box (Linux 7.0.0-29) on a pty whose **master had already been closed**: `fstat`
    /// on the slave fd returns `Ok` with `mode=020600` while the pair is gone. In safe
    /// Rust the descriptor is guaranteed valid for the `File`'s lifetime, so the call
    /// had no reachable failure at all; "proven live" reduced entirely to the
    /// compile-time borrow, which is a real property (see [`settled_while_open`]) but
    /// not the one the name claims.
    ///
    /// What is checked now, in the order a failure wants to read:
    ///
    /// 1. `fstat` on the held fd. Kept, so the witness parameter costs a syscall rather
    ///    than a name — and stated plainly as the weak arm: it cannot fail here.
    /// 2. `stat` on the **path the fd was opened through**. This is the load-bearing
    ///    one, and it is where §9's tell shows up: it comes out *stricter* on the
    ///    platform of record. Linux unlinks a pty slave's `/dev/pts/N` entry at the
    ///    master's close, so a witness whose peer has gone away fails here with
    ///    `ENOENT` — measured directly, same box: `fstat(fd)` `Ok(020600)` and
    ///    `stat("/dev/pts/3")` `ENOENT`, simultaneously, on one closed pair.
    /// 3. The two identities must agree. A `/dev/serial/by-id/…` or a node symlink that
    ///    now resolves somewhere else is a *different device* behind the same name —
    ///    exactly what §12's replug renumbering does to `ttyUSB0`/`ttyUSB1` — and the
    ///    old check could not see it, since the fd is still perfectly valid.
    ///
    /// **The residual, stated rather than papered over.** This proves the path still
    /// resolves to the device the fd holds. It does **not** prove the pty's *master* is
    /// open on a kernel whose slave nodes are not per-allocation directory entries:
    /// Darwin's `/dev/ttysNNN` are persistent devfs nodes, so step 2 is expected to stay
    /// `Ok` there after a master close and the check degrades to step 1's tautology plus
    /// the borrow. That is unmeasured here — this box is Linux — and it is why the
    /// sentence says "expected" (§7: no one-way claim on single-kernel evidence). The
    /// instrument that would close it portably is `poll(POLLHUP)` on the fd, and this
    /// crate cannot issue it: `poll(2)` needs an `unsafe` call and `unsafe` lives only
    /// in `serial_nexus_sys` (AGENTS.md invariant 3 / §16.3) — the same wall
    /// `p12_pty_setup` records for `FIONREAD`. A doctor probe is the right home for it
    /// if it is ever wanted (§7's P6–P11 pattern), not a `libc` call smuggled into the
    /// harness.
    fn prove_open(&mut self) -> Result<(), String> {
        let held = self
            .file
            .metadata()
            .map_err(|e| format!("fstat on the held fd failed: {e}"))?;
        let held = DeviceIdentity::of(&held);
        let live = std::fs::metadata(&self.path).map_err(|e| {
            format!(
                "the fd is still open but {} no longer resolves to a device ({e}); \
                 it was {held} when this witness was taken. On Linux the kernel \
                 unlinks a pty slave's /dev/pts entry at the *master's* close, so this \
                 is the far end having gone away underneath a descriptor that fstat \
                 still answers",
                self.path.display()
            )
        })?;
        let live = DeviceIdentity::of(&live);
        if live != held {
            return Err(format!(
                "{} no longer names the device this witness holds: the fd is {held} \
                 (adopted as {}), the path now resolves to {live}. The descriptor is \
                 valid and useless — something replaced the node underneath it (a \
                 replaced pty node, or a replug renumbering a tty, §12)",
                self.path.display(),
                self.adopted_as
            ));
        }
        Ok(())
    }
}

impl std::io::Read for SlaveWitness {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl std::io::Write for SlaveWitness {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl OpenWitness for Sim {
    fn label(&self) -> String {
        "the held serial-nexus-sim child".to_owned()
    }

    /// A subprocess is open while it has not exited — **an approximation of the
    /// property, not the property**, and the gap is measured rather than hoped at.
    ///
    /// What is wanted is "the sim's slave fd is open at the instant of this reading".
    /// What is answered is "the sim's *process* has not been reaped yet". The sim closes
    /// its port and *then* serializes and prints its verdict before exiting
    /// (`sim/src/main.rs`: the mode function owns the port and returns a `Value`, which
    /// `main` prints afterwards), so there is a window in which `try_wait` says `Ok(None)`
    /// while the slave is already gone.
    ///
    /// Measured on this box (Linux 7.0.0-29, debug profile, load average 0.4), against
    /// the shipped `serial-nexus-sim client`: the master saw the last slave close
    /// **0.658–0.843 ms** before `waitpid(WNOHANG)` reported the child, 6 of 6, with the
    /// instrument's own busy-poll granularity inside that figure — so it is an upper
    /// bound on the window, not the window. Sub-millisecond, and the thing it replaced
    /// was a bare `--hold-ms 20000` timer with no check at all, so this is four orders
    /// of magnitude better than nothing and still not an identity. Do not write "proven
    /// live at the instant of the read" about this witness; write what it is.
    ///
    /// It stays load-bearing anyway, and for a reason the approximation does not touch:
    /// the failure it exists to catch is a `--hold-ms` that expired *seconds* ago under
    /// load, turning a vacuous pass into a named failure. A 0.7 ms blind spot at the tail
    /// does not blunt that.
    fn prove_open(&mut self) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(st)) => Err(format!(
                "the sim already exited ({st}) — its --hold-ms elapsed before the \
                 observation finished, so this reading was taken against a closed slave"
            )),
            Err(e) => Err(format!("try_wait on the sim failed: {e}")),
        }
    }
}

/// Open a pty node's slave (through its symlink) as a blocking, non-controlling fd —
/// the witness half of [`settled_while_open`].
///
/// `O_NOCTTY` because a test process must never acquire a controlling terminal from a
/// node it is only observing. Blocking rather than `O_NONBLOCK` deliberately: doctor
/// P13 measures an `O_NONBLOCK` slave losing its queued bytes **unconditionally** in
/// 29 µs on Darwin, against ~600 ms of drain-wait for a blocking one, so a witness
/// opened non-blocking would arm the very hazard it is held to disarm.
///
/// Panics on failure — a witness that could not be taken must fail loudly, never
/// degrade the observation to the unwitnessed one it replaced (§5's anti-tautology
/// rule).
pub fn attach_slave(path: &Path) -> SlaveWitness {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(path)
        .unwrap_or_else(|e| panic!("open pty slave {} as a witness: {e}", path.display()));
    adopt_slave(file, path)
}

/// Adopt an fd the caller opened itself as a [`SlaveWitness`], recording `path` as the
/// name it was opened through.
///
/// Exists because two callers need flags [`attach_slave`] deliberately does not offer —
/// `p12_pty_setup` opens `O_NONBLOCK` so its CONC-1 arm can *detect* a daemon that
/// stopped draining instead of deadlocking on it, and `p8_map` drives its own client
/// through the same fd it witnesses.
///
/// The `path` argument is verified rather than trusted: the fd and the path must name
/// the same device at adoption, or this panics. A caller that hands in the wrong path
/// would otherwise get a witness that fails (or, worse, passes) for a reason having
/// nothing to do with the test — the anti-tautology rule (§5) applied to the
/// constructor, since every later check is a comparison against what is recorded here.
pub fn adopt_slave(file: std::fs::File, path: &Path) -> SlaveWitness {
    let held = file
        .metadata()
        .unwrap_or_else(|e| panic!("fstat the fd being adopted as a witness: {e}"));
    let held = DeviceIdentity::of(&held);
    let live = std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {} while adopting a witness: {e}", path.display()));
    let live = DeviceIdentity::of(&live);
    assert_eq!(
        held,
        live,
        "adopt_slave was handed a path that does not name the fd's device: the fd is \
         {held}, {} is {live}. Every later proof compares against this pair, so a \
         mismatched path here would make the witness answer about the wrong device",
        path.display()
    );
    SlaveWitness {
        file,
        path: path.to_path_buf(),
        adopted_as: held,
    }
}

/// Wait for a byte counter to settle **while the client that produced those bytes is
/// still open** (notes §3.29, design §15.46, plan §3 rule 8).
///
/// The witness borrow is the enforcement, and it is the whole reason this is a
/// function rather than an inline [`wait_until`]. Its caller closes the client with
/// `drop(client)`, which *moves* it; so moving this call below that line does not
/// merely weaken the test, it **fails to compile** (`borrow of moved value:
/// client`). The ordering is a property the compiler checks, not a comment a future
/// editor can step over — which matters because the previous shape of the first of
/// these tests made it a comment, and the comment lost.
///
/// **What the compiler covers is *relocation*, not *rewriting*** — say the bound
/// exactly, because two rewrites that defeat it both compile: re-calling
/// [`attach_slave`] below the close and handing in the fresh handle (a witness on a
/// pair the observation's bytes never went through), and moving the call down while
/// dropping the dead witness from the slice and keeping a live sibling (the borrow is
/// satisfied by whichever witness is still there, not by the one that matters). Neither
/// is an accident; both are what an editor does when the borrow checker is in the way,
/// which is precisely when this argument is worth reading. The compile error catches the
/// cheap mistake — a moved line — and the only thing that catches the deliberate one is
/// this paragraph and a reviewer.
///
/// Why the ordering is load-bearing at all: reading the counter *after* the close
/// asserts that the kernel retained bytes across the slave's last close, which no
/// kernel promises. Linux retains them (doctor P13: `retains`, `close(2)` in tens of
/// µs, 64/64 recovered, `docs/doctor/linux-7.0-2026-08-05-tier3.json`), so the old
/// order could not fail there and never had. Darwin does not — `waits-then-discards`,
/// 600104 µs and 0 of 64 with no reader, 29 µs and 0 of 64 for an `O_NONBLOCK` slave
/// (`docs/doctor/macos-24.6.0-2026-08-05-tier3.json`) — and that is sufficient: the
/// assertion was wrong wherever it ran.
///
/// **What the witness actually proves, stated precisely because the loose version of
/// this sentence is wrong.** Every kernel attaches its flush to the *last* close of
/// the pty — XNU runs `ptsclose` → `ttylclose` → `ttywflush` when the reference count
/// reaches zero, and Linux charges `discarded_at_last_close` at the same edge. So a
/// witness fd the harness holds on the same slave is exactly as strong as holding the
/// writing client open: while any fd remains, no kernel reaches its flush. That is why
/// several callers open the slave themselves and let a one-shot
/// `serial-nexus-sim client` do the writing — the sim's own exit is then not the last
/// close, and the harness keeps a compile-checked handle on the session instead of a
/// deadline on a subprocess.
///
/// **Precisely what is and is not stronger.** The *assertion* is logically stronger:
/// the counters this waits on are monotone `Cell<u64>` bumped only by `.add`, so
/// "flowed during the live session" implies "reads N afterwards" while the converse
/// does not hold. The *test* is not uniformly stronger, and cannot be — the two
/// observations cannot both be taken on one run. Waiting for the counter before the
/// close inserts an RPC round-trip there, which guarantees the daemon has drained the
/// master *before* the close happens; so a converted test no longer traverses the
/// write-then-immediately-close residual path that `pty.rs`'s "a closing writer's
/// residual must still be forwarded (not purged) before the close is finalized"
/// describes. It never covered that promise deliberately — it traversed it by
/// accident, and only on kernels that retain.
///
/// That gap is **closed, not merely noted**: `p8_map`'s
/// `a_closing_writers_residual_is_forwarded_not_purged` is the deliberate guard for
/// it, and was proved fail-first against a daemon with the drain gated on `now` — a
/// regression the converted test passes and that one catches, with `purged: 64` where
/// `discarded_no_raw_edge: 64` belongs. Do not "simplify" the two into one: they need
/// opposite orderings, and that is the whole point. Two further guards keep the
/// closes-first shape on purpose — `p4_purge`'s purge-on-detach check and
/// `p12_pty_setup`'s `discarded_at_last_close` check — because the counters they
/// assert come into existence *because* of the close and read 0 before it. Each says
/// so in its own doc comment and each carries a positive pre-close witness so the
/// post-close number is not its only evidence.
///
/// This helper is **not** advertised as the root cause of the 2026-08-03 macOS red.
/// XNU's `ptsclose` waits up to `t_timeout` (≈0.6 s) for the master to drain before
/// any flush, so the old ordering's expected Darwin outcome was green — and a red
/// means the reader did not drain in that window, which is a stall, not a kernel coin
/// flip. `docs/macos.md` (2026-08-04) carries the source excerpt and the probe that
/// separates the two.
///
/// One thing this deliberately does **not** do is emulate Darwin's close so a Linux
/// run can referee the ordering dynamically. That was tried and withdrawn:
/// `tcflush(TCOFLUSH)` on a Linux pts slave is not XNU's `ttyflush`. It empties the
/// peer's flip buffer only, never the master's ldisc `read_buf`, so it is a race
/// against the flip-to-ldisc push rather than a queue flush — measured on this box at
/// 300 trials per row, it destroys the bytes 100% of the time at a 0 µs delay and
/// **0%** at 20 µs and beyond. In this function's position it would run an RPC
/// round-trip after the write and so destroy nothing at all: dead code that reads like
/// a guard. The compile-time borrow is strictly stronger and costs nothing.
///
/// The openness is proven **twice**: once before the wait, so an observation that
/// began against an already-closed client is never scored, and once at the moment the
/// condition became true (or the deadline expired), so a witness that closed *during*
/// the wait is named rather than tolerated. The return value is deliberately a `bool`
/// rather than an assertion, so a caller can defer its message below the lifecycle
/// checks that give a more specific diagnosis first.
pub fn settled_while_open(
    witnesses: &mut [&mut dyn OpenWitness],
    what: &str,
    timeout: Duration,
    cond: impl FnMut() -> bool,
) -> bool {
    assert!(
        !witnesses.is_empty(),
        "{what}: settled_while_open with no witness is the defect it exists to \
         prevent — the observation would be unordered against every close in the test"
    );
    prove_all(witnesses, what, "when the observation began");
    let settled = wait_until(timeout, cond);
    prove_all(
        witnesses,
        what,
        if settled {
            "when its byte counter was read"
        } else {
            "while its byte counter was being waited on"
        },
    );
    settled
}

/// [`settled_while_open`] for an observation that **blocks** rather than polls: a sim
/// subprocess's verdict, read to EOF, is a byte count as much as a `state` counter is.
///
/// Same contract and same enforcement — the witnesses are borrowed, so moving this
/// call below the close that ends the session is `E0382`, and they are proven open on
/// both sides of the blocking call. The value the closure returns is handed straight
/// back, so the caller can defer its assertions below the lifecycle checks exactly as
/// with the polling form. Read [`settled_while_open`]'s doc comment for the rule; this
/// is the same rule with `FnOnce` in place of `wait_until`.
pub fn observed_while_open<T>(
    witnesses: &mut [&mut dyn OpenWitness],
    what: &str,
    observe: impl FnOnce() -> T,
) -> T {
    assert!(
        !witnesses.is_empty(),
        "{what}: observed_while_open with no witness is the defect it exists to \
         prevent — the observation would be unordered against every close in the test"
    );
    prove_all(witnesses, what, "when the observation began");
    let out = observe();
    prove_all(witnesses, what, "when its byte counter was read");
    out
}

/// Panic unless every witness is still open, naming the one that is not and the rule
/// it broke. Split out so both the pre- and post-checks report identically.
fn prove_all(witnesses: &mut [&mut dyn OpenWitness], what: &str, when: &str) {
    for w in witnesses.iter_mut() {
        if let Err(why) = w.prove_open() {
            panic!(
                "{what}: {} was no longer open {when} — {why}.\n\
                 A byte counter read after its producer closed asserts that the \
                 *kernel* retained the bytes across the slave's last close, which \
                 Linux does and Darwin does not (notes §3.29, plan §3 rule 8; doctor \
                 P13 measures both).",
                w.label()
            );
        }
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
/// platform; else — **macOS only, and only when the operator has opted in** —
/// exactly two `/dev/cu.usbserial-*` nodes.
///
/// **The doctrine is now the same on both platforms, and that is plan §18 item 5
/// discharged** (design §13's no-target doctrine). [`rig_candidates`] states the
/// rule this function used to break on one platform: *reported, never
/// auto-selected* — two adapters being present is not two adapters being
/// cross-wired, and a harness that opened whatever it found would transmit
/// 32 KiB at 250000 baud and pulse DTR on equipment nobody verified. That is the
/// same reason `serial-nexus-doctor` is passive until a port is named. The macOS
/// arm did exactly what the rule forbids, silently, on a plain
/// `cargo test --workspace`.
///
/// **What was decided, and what it cost.** Deleting the arm outright was the
/// obvious transplant and is *not* what happened, because it is not free: the
/// software serial doubles are Linux-only — a pty is not a serial device on
/// Darwin, where `serial2` answers `ENOTTY` — so this scan is macOS's **only**
/// serial provider, and removing it would make eleven tests self-skip on a
/// working box. That is notes §3.35's defect wearing the other hat, and this tree
/// has already paid once for a green run that was coverage never executed. So the
/// arm survives behind a **named opt-in**: set `SNX_CROSSOVER` to anything at all
/// (`scan`, `required`, `1`) and the scan runs; leave it unset and the pair is
/// reported in the skip message and never chosen. An operator gets the coverage by
/// saying one word, and nobody transmits on unknown hardware by accident.
///
/// Three consequences worth stating rather than discovering. The macOS platform
/// record taken before this change (`docs/macos.md`, the `fa4b12d` run) was
/// produced by the unconditional scan and needs `SNX_CROSSOVER` exported to
/// reproduce. `SNX_CROSSOVER=required` becomes **reachable** on a Mac for the first
/// time — it previously could not fire there, because required-mode only triggers
/// on a skip and the scan prevented skips, so the one knob that exists to stop a
/// vacuous green run was structurally dead on the platform that most needed it.
/// And the selection is now **announced**: a run that transmits on scanned
/// hardware says which two nodes it picked, in its own output, so the choice is
/// auditable after the fact instead of being inferred from a filename.
///
/// The Linux side is unchanged and the asymmetry it used to carry is gone: on both
/// platforms a physically cross-wired pair is invisible here until the operator
/// says so. A doctor P5 reporting Tier 3 still says nothing about whether these
/// tests ran — it certifies the rig, not this detection (§15.21).
/// Grant this user read/write on every rig device node the environment names,
/// **once per test binary**, using the blessed helper (§15.55).
///
/// # Why this exists at all
///
/// A grant lives on an inode, and a USB re-enumeration destroys the inode: udev
/// builds a fresh node `root:dialout 0660`, so anything an operator arranged before
/// the run — `setfacl`, a chown — is gone the first time a replug test cycles an
/// adapter. That is not hypothetical; it is the whole of the 2026-08-12 rig lane's
/// eight `Permission denied (os error 13)` failures (notes §3.79). The helper
/// re-grants after every reauthorization it performs, which repairs the *middle* of
/// a run; this repairs the *start* of one, so a rig box needs nothing but the single
/// `setcap` — no group membership, no re-login, and nothing to remember when the
/// project moves to a different machine.
///
/// # What it will not do
///
/// It grants only for ports the environment has **already named** — the same
/// `SNX_CROSSOVER_A`/`_B` and `SNX_REPLUG_DEV`/`_DEV_B` that opt this run into
/// touching hardware at all. Naming a port is the operator's statement that it is
/// wired to the rig and not to live equipment (§15.17), and this adds no new way to
/// point the suite at a device nobody named.
///
/// Failure is **reported, never fatal**. Without a blessed helper there is nothing
/// to do and the tests skip or fail on their own terms with their own messages; a
/// panic here would replace a specific diagnostic with a generic one.
#[cfg(target_os = "linux")]
pub fn ensure_rig_access() {
    use std::sync::OnceLock;
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // Every device the operator named, deduplicated by the USB port behind it —
        // two by-id links and two tty paths routinely name the same two adapters.
        let mut ports: Vec<String> = Vec::new();
        for var in [
            "SNX_CROSSOVER_A",
            "SNX_CROSSOVER_B",
            "SNX_REPLUG_DEV",
            "SNX_REPLUG_DEV_B",
        ] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            match usb_port_of(Path::new(&path)) {
                Some(port) if !ports.contains(&port) => ports.push(port),
                Some(_) => {}
                None => eprintln!(
                    "rig access: {var}={path} resolves to no sysfs USB port; not granting for it"
                ),
            }
        }
        if ports.is_empty() {
            return;
        }
        let helper = match blessed_devprep_helper() {
            Ok(h) => h,
            Err(why) => {
                eprintln!(
                    "rig access: not granting device access — {why}. Tests needing these \
                     nodes will report their own reason."
                );
                return;
            }
        };
        let mut args: Vec<String> = vec!["grant".to_owned()];
        for port in &ports {
            args.push("--port".to_owned());
            args.push(port.clone());
        }
        args.push("--json".to_owned());
        match std::process::Command::new(&helper).args(&args).output() {
            Ok(out) if out.status.success() => {
                eprintln!(
                    "rig access: granted on ports {} — {}",
                    ports.join(", "),
                    String::from_utf8_lossy(&out.stdout).trim()
                );
            }
            Ok(out) => eprintln!(
                "rig access: `grant` refused for ports {}: {}{}",
                ports.join(", "),
                String::from_utf8_lossy(&out.stdout).trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => eprintln!("rig access: running {}: {e}", helper.display()),
        }
    });
}

/// Non-Linux: the blessed helper is a Linux mechanism (§15.45, notes §3.66), so
/// there is nothing to grant and nothing to say about it.
#[cfg(not(target_os = "linux"))]
pub fn ensure_rig_access() {}

pub fn crossover_ports() -> Option<(String, String)> {
    if let (Ok(a), Ok(b)) = (
        std::env::var("SNX_CROSSOVER_A"),
        std::env::var("SNX_CROSSOVER_B"),
    ) {
        // The one funnel every crossover test passes through, so it is where the
        // run acquires access to the nodes it is about to open (§15.55, notes
        // §3.79). Once per binary; a no-op without a blessed helper.
        ensure_rig_access();
        return Some((a, b));
    }
    #[cfg(target_os = "macos")]
    {
        // The opt-in. Any value — this is a switch, not a vocabulary, and
        // `required` (the spelling an operator already knows) must turn the scan
        // *on* as well as making its absence fatal, or required-mode would demand
        // a rig it had just refused to look for.
        if std::env::var("SNX_CROSSOVER").is_err() {
            return None;
        }
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
            // Say what was chosen. A scan that selects silently is indistinguishable
            // in the output from a pair the operator named, and the two carry very
            // different warranties.
            eprintln!(
                "RIG: SNX_CROSSOVER is set and no ports were named, so the macOS scan \
                 selected {} and {} — verify they are the cross-wired pair, because \
                 this harness will transmit and pulse DTR on both.",
                ports[0], ports[1]
            );
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

/// A control connection that can **park** — the harness's client for waiting verbs
/// (`tap.wait`, `lock --wait`, a contended `send`).
///
/// [`Rpc::call`] cannot model one, for two independent reasons: it sets a 30 s read
/// timeout and *panics* on expiry (by design — a transport failure must be loud), and
/// it closes the connection after one exchange. A parked verb needs the opposite of
/// both: an unbounded, caller-chosen deadline, and the write half held open for the
/// whole wait, because §15.20 cancels a waiting verb on connection EOF and a
/// half-close is indistinguishable from a killed client at read time.
///
/// It also keeps the connection *usable* while a verb is parked, which is what makes
/// it the client for the two things a parked wait must be tested against: pipelining
/// a second request behind it (refused with `-32006`, both surviving) and watching
/// `tap.data` keep flowing past it (CTRLW-1). Lines that are not the response being
/// waited for are stashed rather than dropped — see [`Self::take_pending`] — because
/// a helper that silently ate notifications would make "the stream kept flowing" and
/// "the stream stopped" look identical, which is the shape plan §3 rule 22 is about.
///
/// One helper rather than the per-file `LockWaiter` copies that preceded it (plan §18
/// item 50's class): §16.5 makes assertion helpers shared and self-tested.
pub struct WaitConn {
    stream: UnixStream,
    buf: Vec<u8>,
    next_id: i64,
    pending: Vec<Value>,
}

impl WaitConn {
    /// Open a control connection and hold both halves.
    pub fn connect(socket: &Path) -> WaitConn {
        let stream = UnixStream::connect(socket)
            .unwrap_or_else(|e| panic!("connect {}: {e}", socket.display()));
        WaitConn {
            stream,
            buf: Vec::new(),
            next_id: 1,
            pending: Vec::new(),
        }
    }

    /// Send one request and return its id. Does **not** wait for the reply, which is
    /// the whole point: the caller decides when, and how long, to wait.
    pub fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".into(), json!("2.0"));
        req.insert("id".into(), json!(id));
        req.insert("method".into(), json!(method));
        if !params.is_null() {
            req.insert("params".into(), params);
        }
        let line = format!("{}\n", Value::Object(req));
        self.stream
            .write_all(line.as_bytes())
            .unwrap_or_else(|e| panic!("write `{method}`: {e}"));
        self.stream.flush().expect("flush request");
        id
    }

    /// Read lines until the response carrying `id` arrives, or `timeout` elapses.
    ///
    /// Returns the whole response object — `result` *or* `error` — because a parked
    /// verb's interesting outcomes live on both sides and a helper that unwrapped one
    /// would force every error assertion through a second path. Everything read on the
    /// way (notifications, another request's reply) is stashed for
    /// [`Self::take_pending`].
    pub fn response(&mut self, id: i64, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let line = self.read_line_until(deadline)?;
            let v: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON: {e}; raw={line:?}"));
            if v.get("id").and_then(Value::as_i64) == Some(id) {
                return Some(v);
            }
            self.pending.push(v);
        }
    }

    /// Read whatever arrives next within `timeout`, response or notification.
    pub fn next_line(&mut self, timeout: Duration) -> Option<Value> {
        if !self.pending.is_empty() {
            return Some(self.pending.remove(0));
        }
        let line = self.read_line_until(Instant::now() + timeout)?;
        Some(
            serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("control line is not JSON: {e}; raw={line:?}")),
        )
    }

    /// Everything read while waiting for some other response — the notifications a
    /// test asserts kept flowing past a parked verb.
    pub fn take_pending(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.pending)
    }

    /// Cancel every verb parked on this connection by closing it — §15.20's
    /// cancel-on-disconnect, which the daemon must observe.
    pub fn cancel(self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

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
// `serial-nexus-devprep` is the one binary in this workspace meant to carry a Linux
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

/// The blessed `serial-nexus-devprep`, or why it cannot be used.
///
/// **Proves the blessing rather than assuming it**: the file existing says nothing,
/// because the kernel strips `security.capability` on every rewrite, so a copy left
/// over from before a rebuild is present and powerless. This asks the binary itself
/// — `capabilities --json` reports the effective bit it reads from its own
/// `/proc/self/status` — which is the only answer that cannot be stale.
pub fn blessed_devprep_helper() -> Result<PathBuf, String> {
    // **The blessing is a Linux mechanism, and the caller must be told that rather
    // than left to infer it from a missing file** (notes §3.72). Off Linux this used
    // to report the helper "not installed" and tell the operator to run `install` and
    // "then the sudo command it prints" — but macOS `install` deliberately installs
    // nothing and prints no sudo command, because its re-enumeration path is
    // unprivileged (§3.66). So under `SNX_REPLUG=required` a Mac user got a hard
    // failure whose remedy was a no-op they could run forever. The non-portability is
    // deliberate and unchanged; only the explanation is new.
    if !cfg!(target_os = "linux") {
        return Err(
            "the blessed replug helper is Linux-only: it exists to carry \
             CAP_DAC_OVERRIDE for one sysfs `authorized` write, and macOS re-enumerates \
             through IOUSBLib unprivileged, so `install` has nothing to install and \
             prints no sudo command (§15.45, notes §3.66). Wiring the hardware replug \
             test to the macOS backend is a change to the test's contract — it takes a \
             /dev/serial/by-id path where the macOS arm is addressed by USB serial \
             number — and is filed, not done (plan §18)"
                .to_owned(),
        );
    }
    let path = blessed_dir().join("serial-nexus-devprep");
    if !path.exists() {
        return Err(format!(
            "{} is not installed — run `cargo build --workspace && \
             ./target/debug/serial-nexus-devprep install`, then the sudo command it prints",
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
    // **The whole capability set, not the first bit** (§15.55). This read
    // `cap_dac_override_effective` alone until 2026-08-12, so a copy blessed with
    // only that capability — a pre-§15.55 blessing, or a hand-typed `setcap` —
    // passed as blessed here, and the rig lane then proceeded to `cycle`, whose
    // grant step silently did nothing (it prints its note only outside `--json`,
    // plan §18 item 52), and every test that opened the recreated node failed
    // `Permission denied`: notes §3.79's eight-failure signature, which §15.55
    // exists to remove. `can_grant_device_access` is the field that answers the
    // question the caller is really asking, and nothing read it.
    let can_replug = json["cap_dac_override_effective"] == serde_json::Value::Bool(true);
    let can_grant = json["can_grant_device_access"] == serde_json::Value::Bool(true);
    if can_replug && !can_grant {
        return Err(format!(
            "{} is blessed for the replug but not for the access grant \
             (`cap_dac_override` yes, `cap_fowner` no — §15.55). A cycle would succeed \
             and leave every recreated node root:dialout, so the tests that follow would \
             fail `Permission denied` for a reason that has nothing to do with them. \
             Re-run `scripts/bless`, which applies the derived full set.",
            path.display()
        ));
    }
    if can_replug {
        // Blessed — but is it the *current* helper? The capability check cannot
        // tell: a copy blessed before an edit is fully functional and runs the old
        // code, so a test would silently measure a helper that no longer exists in
        // the tree. Warn rather than fail, because the comparison is over bytes and
        // an ordinary relink changes them (measured: a `cargo test --workspace`
        // that touched nothing in `devprep/` still produced a different build id at
        // byte 25) — failing on that would red the suite for a no-op. A warning
        // puts the hazard in front of whoever edited the helper without costing
        // anyone else a `sudo`.
        let built = target_dir().join("serial-nexus-devprep");
        if let (Ok(a), Ok(b)) = (std::fs::read(&built), std::fs::read(&path))
            && a != b
        {
            eprintln!(
                "NOTE: {} differs from {} — if you edited the helper, re-run \
                 `scripts/bless` or these tests are measuring the previously blessed \
                 build. (An ordinary relink also changes these bytes, so this is a \
                 warning, not a failure.)",
                path.display(),
                built.display()
            );
        }
        return Ok(path);
    }
    Err(format!(
        "{} is installed but not blessed ({}). Run scripts/bless — it derives the full \
         capability set from the helper itself, so this message never has to spell it.",
        path.display(),
        text.trim(),
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

/// Announce a hardware-flow-control test's self-skip — and refuse to skip when the
/// operator has said the capability must be exercised.
///
/// **A fourth instance of one mechanism, and the first whose precondition is
/// *measured* rather than declared** (plan §3 rule 11, §18 item 7, design §15.52).
/// The other three ask whether a *thing* is present: a crossover rig, a blessed
/// replug helper, `curl`. This one asks whether the rig that is present carries
/// RTS↔CTS, which is a property of the operator's cabling and not of their
/// checkout — and §5's stated design assumption is the **3-wire** link, so a rig
/// that answers no is a legitimate rig and not a fault. The measurement is taken
/// first and printed in the skip, so a reader never has to guess which of the two
/// they have; the doctor reports the same fact independently in P5's handshake
/// block.
///
/// `SNX_CROSSOVER=required` deliberately does **not** redden this: the rig is
/// present, the capability is not, and conflating the two would make an honest
/// 3-wire bench unable to run the rig lane at all. `SNX_RIG_FLOW=required` is the
/// separate word for "this bench is 5-wire and I want that proven".
pub fn skip_no_rig_flow(test: &str, measured: &str) {
    assert!(
        std::env::var("SNX_RIG_FLOW").as_deref() != Ok("required"),
        "SNX_RIG_FLOW=required, but {test} found no hardware handshake: {measured}.\n\
         Required mode exists so a 5-wire bench cannot report a green run for the \
         one capability that only a 5-wire bench can exercise (§15.52, plan §3 \
         rule 11). Cross-wire RTS↔CTS, or unset SNX_RIG_FLOW."
    );
    eprintln!("SKIP {test}: the rig carries no RTS/CTS handshake — {measured}");
}

/// Announce the TLS round-trip's self-skip — and refuse to skip when the operator
/// has said the tier must be exercised.
///
/// **The one skip class that had no `required` spelling** (plan §3 rule 11, plan §18
/// item 10). `web_tls_round_trip` is the only proof that the TLS tier's *handshake*
/// works rather than merely that it binds, and it carries **two** silent skips: no
/// `curl` on `PATH` (plan §11.6 names curl as the client, and `std` ships no TLS
/// client), and an environment that cannot bind `0.0.0.0:0` with `--tls`. Either one
/// turns the tier's only end-to-end guard into a green line, and nothing in the
/// output distinguishes that from a run that exercised it — the shape notes §3.35
/// measured and §3.43 measured again, both times on a box where the capability was
/// present and the harness never used it.
///
/// `SNX_TLS=required` turns both into hard failures, exactly as `SNX_CROSSOVER` and
/// `SNX_REPLUG` do for their classes. It is a third instance of one mechanism rather
/// than a third mechanism: same variable shape, same word, same failure text — a skip
/// class that invented its own spelling would be a fourth thing to remember.
///
/// `reason` names *which* of the two skips fired, because they call for different
/// actions: one is a missing tool, the other is a sandbox that will not bind.
pub fn skip_no_tls(test: &str, reason: &str) {
    assert!(
        std::env::var("SNX_TLS").as_deref() != Ok("required"),
        "SNX_TLS=required, but {test} skipped: {reason}.\n\
         Required mode exists so a box that can run the TLS round-trip cannot report \
         a green run for the one guard that proves the handshake rather than the bind \
         (plan §11.6, §3 rule 11)."
    );
    eprintln!("SKIP {test}: {reason}");
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

    // -----------------------------------------------------------------------------
    // The witness's own guards (notes §3.60). `prove_open` was
    // `self.metadata().map(|_| ())`, which in safe Rust has no reachable failure —
    // these three are what make it a check instead of a name.
    // -----------------------------------------------------------------------------

    /// Two temp-dir symlinks and two character devices every Unix ships, so this runs
    /// on every platform the suite does. `/dev/null` and `/dev/zero` are distinct
    /// devices with distinct `st_rdev`, which is the whole discriminator.
    fn link_to(dir: &Path, name: &str, target: &str) -> PathBuf {
        let link = dir.join(name);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(target, &link).expect("symlink");
        link
    }

    /// **A witness whose path is repointed at a different device is refused**, though
    /// its descriptor stays perfectly valid.
    ///
    /// This is the case an `fstat` on the fd cannot see and the reason the path is
    /// carried at all: a node symlink that now resolves elsewhere is a *different
    /// device* under the same name — what §12's replug renumbering does to
    /// `ttyUSB0`/`ttyUSB1`, and what a replaced pty node does to a run-directory link.
    /// The fd keeps answering; it just no longer refers to what the test named.
    #[test]
    fn a_witness_is_refused_once_its_path_names_a_different_device() {
        let run = TempRun::new();
        let link = link_to(run.path(), "dev", "/dev/null");
        let mut w = attach_slave(&link);
        w.prove_open()
            .expect("a witness on an intact path proves open");

        // The control that keeps this test honest: the fd is *still fine*. If this ever
        // fails, the guard below is passing for the wrong reason and the finding this
        // test encodes has changed.
        assert!(
            w.file.metadata().is_ok(),
            "fstat on the held fd failed, so the assertion below would no longer be \
             about the path at all"
        );

        link_to(run.path(), "dev", "/dev/zero");
        let why = w
            .prove_open()
            .expect_err("a witness whose path now names /dev/zero must be refused");
        assert!(
            why.contains("no longer names the device this witness holds"),
            "the refusal did not name the mechanism: {why}"
        );

        // …and the path disappearing entirely is the other arm, reported separately
        // because a reader chasing it wants to know which of the two happened.
        std::fs::remove_file(&link).expect("unlink");
        let why = w
            .prove_open()
            .expect_err("a witness whose path is gone must be refused");
        assert!(
            why.contains("no longer resolves to a device"),
            "the refusal did not name the mechanism: {why}"
        );
    }

    /// **The measurement that retired the old `prove_open`, as a guard.**
    ///
    /// On a pty whose master has closed, `fstat` on the slave fd returns `Ok` — this
    /// test asserts that, so the tautology is *recorded* rather than described — while
    /// the kernel has unlinked the `/dev/pts` entry, so a fresh `stat` of the path
    /// fails. The old witness was green on a dead pair; the new one is red.
    ///
    /// Linux-only, and that is §9's tell rather than a gap: this is the arm that comes
    /// out **stricter** on the platform of record. Darwin's `/dev/ttysNNN` are
    /// persistent devfs nodes, so the same construction is expected to leave the path
    /// resolvable there — unmeasured, and stated as an expectation in
    /// [`SlaveWitness::prove_open`]'s residual paragraph rather than asserted here.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_witness_on_a_pty_whose_master_closed_is_refused_though_fstat_still_answers() {
        let Some(echo) = serial_echo() else {
            panic!("serial_echo() is Some on Linux; a None here is a harness defect");
        };
        // Keep the run directory (and so the symlink) alive; drop only the sim, which
        // is what closes the master. Dropping the whole `SerialEcho` would delete the
        // link too, and the test would then prove that a deleted symlink is a deleted
        // symlink instead of that a closed master destroys the device.
        let SerialEcho {
            device,
            _sim: sim,
            _run: run,
        } = echo;
        let target = std::fs::read_link(&device).expect("the sim's link points somewhere");
        let mut w = attach_slave(&device);
        w.prove_open().expect("a live sim pty proves open");

        drop(sim);
        assert!(
            wait_until(Duration::from_secs(10), || !target.exists()),
            "{} still exists after the sim died — the kernel did not unlink the pty \
             slave at the master's close, which is the premise of this whole check",
            target.display()
        );

        // The tautology, measured: the descriptor the old `prove_open` interrogated is
        // still answering, on a pair that no longer exists.
        use std::os::unix::fs::FileTypeExt;
        let md = w
            .file
            .metadata()
            .expect("fstat still answers on a slave whose master closed");
        assert!(
            md.file_type().is_char_device(),
            "the held fd stopped being a character device, which the old check would \
             also have missed"
        );

        let why = w
            .prove_open()
            .expect_err("a witness on a pty whose master closed must be refused");
        assert!(
            why.contains("no longer resolves to a device"),
            "the refusal did not name the mechanism: {why}"
        );
        drop(run);
    }

    /// A path that does not name the fd's device is a harness defect, and it dies at
    /// the constructor rather than three assertions later against the wrong device.
    #[test]
    #[should_panic(expected = "does not name the fd's device")]
    fn adopting_a_witness_with_the_wrong_path_panics_at_the_constructor() {
        let null = std::fs::File::open("/dev/null").expect("open /dev/null");
        let _ = adopt_slave(null, Path::new("/dev/zero"));
    }
}
