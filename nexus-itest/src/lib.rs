//! Cross-platform Rust integration-test harness for serial_nexus.
//!
//! This crate replaces the bash validation scripts under `scripts/validate/**` with
//! portable Rust (design §5). It boots `serialnexusd` as a subprocess, drives it over
//! the Unix control socket with a small JSON-RPC client, orchestrates `nexus-sim`
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

/// Locate a workspace binary (`serialnexusd`, `serialnexusctl`, `nexus-sim`,
/// `nexus-doctor`). Requires a prior `cargo build --workspace` (which `cargo test
/// --workspace` does as part of its compile phase); panics with guidance otherwise.
pub fn bin(name: &str) -> PathBuf {
    let exe = target_dir().join(name);
    assert!(
        exe.exists(),
        "binary `{name}` not found at {} — run `cargo build --workspace` first \
         (or invoke the suite as `cargo test --workspace`)",
        exe.display()
    );
    exe
}

/// SHA-256 of `bytes`, lowercase hex — the byte-exact ground truth for data-plane
/// assertions (matches `nexus-sim`'s `sha256_hex`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
        self.dir.join("serialnexusd.sock")
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
/// connection (as `serialnexusctl` does), NDJSON framing (§10). This is the Rust
/// replacement for `serialnexusctl --json … | jq`.
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
    /// `serialnexusctl` does before sending).
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
        // Consume the ack (a response carrying our id) before notifications flow.
        let _ = sub.read_line_until(Instant::now() + Duration::from_secs(10));
        sub
    }

    /// `subscribe` to the daemon's state-notification stream (§10).
    pub fn subscribe(&self) -> Subscription {
        self.stream("subscribe", Value::Null)
    }

    pub fn teardown(&self) {
        let _ = self.call("teardown", Value::Null);
    }

    pub fn shutdown(&self) {
        let _ = self.call("shutdown", Value::Null);
    }
}

/// A running `serialnexusd` subprocess with its own temp runtime dir and socket.
/// Killed and cleaned up on `Drop`.
pub struct Daemon {
    child: Child,
    rpc: Rpc,
    run: TempRun,
}

impl Daemon {
    /// Boot a fresh daemon on an empty graph and wait for its control socket.
    pub fn start() -> Self {
        let run = TempRun::new();
        let socket = run.socket();
        let child = Command::new(bin("serialnexusd"))
            .arg("--socket")
            .arg(&socket)
            .arg("--state-file")
            .arg(run.state_file())
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serialnexusd");
        let ready = wait_until(Duration::from_secs(10), || socket.exists());
        assert!(
            ready,
            "daemon control socket never appeared at {}",
            socket.display()
        );
        let rpc = Rpc::new(socket);
        Daemon { child, rpc, run }
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
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.rpc.shutdown();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A `nexus-sim` subprocess double (e.g. `pty --echo`), killed on `Drop`. Use
/// [`Sim::client`] for the one-shot `client` verdicts (which run to completion).
pub struct Sim {
    child: Child,
}

impl Sim {
    /// Spawn `nexus-sim` with `args` in the background (a long-lived double such as
    /// `pty --echo --link …`), waiting for `link` to appear if given.
    pub fn spawn(args: &[&str], link: Option<&Path>) -> Self {
        let child = Command::new(bin("nexus-sim"))
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn nexus-sim");
        if let Some(link) = link {
            let up = wait_until(Duration::from_secs(5), || link.exists());
            assert!(up, "sim link never appeared at {}", link.display());
        }
        Sim { child }
    }

    /// Run a one-shot `nexus-sim client …` to completion and return its JSON verdict.
    pub fn client(args: &[&str]) -> Value {
        let out = Command::new(bin("nexus-sim"))
            .arg("client")
            .args(args)
            .output()
            .expect("run nexus-sim client");
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

/// A single echoing serial device, or `None` to **skip**. Linux: a `nexus-sim pty
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

/// Detect a two-port crossover rig: `SNX_CROSSOVER_A`/`_B` if set, else exactly two
/// `/dev/cu.usbserial-*` (macOS) or two `/dev/serial/by-id/*` (Linux) adapters.
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
    pub fn next(&mut self, timeout: Duration) -> Option<Value> {
        let line = self.read_line_until(Instant::now() + timeout)?;
        serde_json::from_str(&line).ok()
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
/// doctrine). Backed by a `nexus-sim nullmodem` (two crossed pts), which is **lossless**
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
    _sim: Sim,
    _run: TempRun,
}

impl SerialPair {
    pub fn ports(&self) -> (&str, &str) {
        (&self.a, &self.b)
    }
}

/// A lossless cross-wired serial pair, or `None` to **skip**. Linux: a `nexus-sim
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
            _sim: sim,
            _run: run,
        });
    }
    #[allow(unreachable_code)]
    None
}
