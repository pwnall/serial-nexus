//! Phase 8 web-console slice, ported from `scripts/validate/phase8/web.sh`
//! (design §17 / §15.29, plan §11.3-6): the `serialnexusweb` HTTP + WebSocket
//! console, a pure loopback RPC client of the daemon. The properties, and how each is
//! expressed portably in Rust:
//!
//! 1. **The token gates every request** — no cookie → 401 (a raw `TcpStream`
//!    HTTP/1.1 request, status line parsed).
//! 2. **The Host header is validated** (DNS-rebinding defense) — a bad `Host` → 403,
//!    checked *before* the token.
//! 3. **The bootstrap URL** `?token=` sets the cookie (302); a wrong token → 401.
//! 4. **A valid cookie serves the app** (200) for `/`, `/app.js`, and every ES module
//!    `app.js` imports — the list read out of `app.js` itself, so a module added
//!    tomorrow cannot be forgotten here (§11.9).
//! 5. **The bind policy** (§15.29): a non-loopback plaintext bind without
//!    `--tls`/`--insecure-bind` exits non-zero with the documented reason; the TLS
//!    tier binds an `https://` listener, writes a 0600 key, and *permits* a
//!    non-loopback bind.
//! 6. **The WS bridge** relays `state` and enforces the §17 verb filter (a graph verb
//!    like `load` is refused at the bridge with `-32601`, never reaching the daemon),
//!    and the end-to-end WebSocket byte stream checksums byte-exact against the
//!    seeded source (headless `serialnexusweb wsclient` → server → daemon → device).
//! 7. **One frame is exactly one request** (review WEB-1/SEC-1, critical): a single
//!    text frame carrying two newline-separated JSON-RPC requests is refused with
//!    `-32600` and never reaches the daemon — the graph survives a smuggled
//!    `teardown` and the process survives a smuggled `shutdown`.
//! 8. **`Origin` is validated against the request's own `Host`, port-exactly** (review
//!    SEC-3/WEB-7): a sibling port on the same host is refused 403 on both a plain
//!    request and the WebSocket upgrade, this very authority is accepted, and an
//!    *absent* Origin is accepted (the shipped judgement — see the test's comment).
//! 9. **The pre-auth path is bounded** (review WEB-3): in-flight connections are
//!    capped, and a peer that connects and says nothing is released by the head
//!    deadline (408) rather than pinning a task and an fd forever.
//! 10. **The TLS tier round-trips** (plan §11.6, testing item T8): over a
//!     non-loopback-shaped bind, `curl --cacert` gets 200 **with** the token and 401
//!     **without** it, the untrusted (no-`--cacert`) client is rejected by cert
//!     validation, and the tier-2 cookie carries `Secure` (review WEB-2).
//!
//! Deviations from the bash, each preserving the original *assertions*:
//! * `curl -w '%{http_code}'` → a hand-rolled raw-`TcpStream` HTTP/1.1 client that
//!   reads the status line (portable, no `curl` flag divergence). The one place curl
//!   survives is the TLS tier, where a client must validate a certificate: plan
//!   §11.6's validation names `curl --cacert` by name, and `std` has no TLS. That
//!   test **self-skips** when curl is absent (a skip is a valid verdict, §5).
//! * `sed`/`grep` on the server's stdout for the bound port → a stdout reader thread
//!   plus a bounded scan for the printed `http(s)://…` URL.
//! * `sha256sum`/`stat -c %s` on the WS byte dump → [`sha256_hex`] + `Vec::len`; the
//!   source checksum comes from the `nexus-sim client` echo verdict's `sha256_sent`
//!   (the harness discards a `pty --source`'s stdout checksum, §5 / p3_log), driving
//!   the same "the browser byte path is lossless and byte-exact" property over the
//!   sanctioned echo helper.
//! * The frame-smuggling and Origin cases need a *raw* WebSocket — a crafted frame the
//!   `wsclient` subcommand cannot produce, since it serialises its own request — so
//!   this file carries a ~100-line RFC 6455 client ([`Ws`]): the handshake plus masked
//!   client frames and unmasked server frames. No new dependency; the whole point is
//!   to send bytes a well-behaved client never would.
//! * The plaintext HTTP gates, the WS bridge relay/filter, the frame-smuggling and
//!   Origin cases, the pre-auth bounds, the bind-policy refusal, and the TLS-tier
//!   bind/key-mode/non-loopback/round-trip checks need **no serial device**, so they
//!   run on every platform. The end-to-end WS byte stream needs a serial device
//!   ([`serial_echo`]) and so **skips** on macOS (§5).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nexus_itest::{Daemon, Sim, TempRun, bin, serial_echo, sha256_hex, wait_until};
use serde_json::Value;

/// The fixed per-session bearer token (the bash's `TOK`). Overriding the random
/// default keeps the test deterministic (`--token`, §15.29).
const TOKEN: &str = "testtoken0123456789abcdef";
/// End-to-end WS byte-stream size (the bash's `N`): 256 KiB.
const N: u64 = 262144;
/// Seed for the byte-stream source (the bash's `SEED`).
const SEED: u64 = 31;
/// Mirrors `serialnexusweb/src/server.rs`'s `MAX_CONNECTIONS`: in-flight connections
/// served at once, above which the newest is dropped rather than queued.
const MAX_CONNECTIONS: usize = 128;
/// Mirrors `serialnexusweb/src/server.rs`'s `HEAD_TIMEOUT`: how long a peer has to
/// deliver a complete request head before the connection is released.
const HEAD_TIMEOUT: Duration = Duration::from_secs(15);

/// A child killed and reaped on drop, so a panicking test never leaks a process.
struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A running `serialnexusweb` server whose stdout is drained into a shared buffer, so
/// the bound `http(s)://…` URL (printed once, right after binding) can be scanned for
/// the OS-chosen ephemeral port. Killed on drop.
struct WebServer {
    child: Child,
    lines: Arc<Mutex<Vec<String>>>,
}

impl WebServer {
    /// Spawn `serialnexusweb --bind <bind> --token <TOKEN> --socket <socket> <extra>`
    /// with `XDG_RUNTIME_DIR = xdg` and a stdout reader thread. `extra` carries any
    /// TLS flags.
    fn spawn(bind: &str, socket: &Path, xdg: &Path, extra: &[&str]) -> Self {
        let socket_str = socket.to_string_lossy().into_owned();
        let mut args: Vec<&str> = vec!["--bind", bind, "--token", TOKEN, "--socket", &socket_str];
        args.extend_from_slice(extra);
        let mut child = Command::new(bin("serialnexusweb"))
            .args(&args)
            .env("XDG_RUNTIME_DIR", xdg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serialnexusweb");
        let stdout = child.stdout.take().expect("piped serialnexusweb stdout");
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => sink.lock().unwrap().push(l),
                    Err(_) => break,
                }
            }
        });
        WebServer { child, lines }
    }

    /// Wait for the printed `scheme://…` URL line and return it (trimmed), or `None`.
    fn wait_url(&self, scheme: &str, timeout: Duration) -> Option<String> {
        let needle = format!("{scheme}://");
        let mut found = None;
        wait_until(timeout, || {
            let guard = self.lines.lock().unwrap();
            for l in guard.iter() {
                if let Some(i) = l.find(needle.as_str()) {
                    found = Some(l[i..].trim().to_string());
                    return true;
                }
            }
            false
        });
        found
    }

    /// The bound port parsed from the printed `scheme://host:port/…` URL.
    fn port(&self, scheme: &str, timeout: Duration) -> Option<u16> {
        parse_port(&self.wait_url(scheme, timeout)?)
    }

    /// Whether the server process has already exited — used to tell "this environment
    /// cannot bind what the test needs" (a skip) from "the server is broken" (a fail).
    fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Parse the port from a `scheme://host:port/rest` URL (loopback IPv4 forms only,
/// which is all this server prints).
fn parse_port(url: &str) -> Option<u16> {
    let after = url.split("://").nth(1)?;
    let authority = after.split('/').next()?;
    authority.rsplit_once(':')?.1.parse().ok()
}

/// A minimal raw HTTP/1.1 request over loopback returning the numeric status code, or
/// `None` when the server answered nothing at all (it dropped the connection) — the
/// portable replacement for `curl -s -o /dev/null -w '%{http_code}'`. The server
/// answers `Connection: close`, so a single read of the status line suffices.
fn try_http_status(
    port: u16,
    method: &str,
    target: &str,
    host: &str,
    headers: &[(&str, &str)],
) -> Option<u16> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut req = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).ok()?;
    // e.g. "HTTP/1.1 401 Unauthorized"
    status_line.split_whitespace().nth(1)?.parse().ok()
}

/// [`try_http_status`], panicking when the server never answered.
fn http_status(port: u16, method: &str, target: &str, host: &str, headers: &[(&str, &str)]) -> u16 {
    try_http_status(port, method, target, host, headers)
        .unwrap_or_else(|| panic!("no HTTP status for {method} {target} (Host: {host})"))
}

/// The `Cookie` header carrying a valid session token.
fn cookie_header() -> String {
    format!("nexus_session={TOKEN}")
}

/// Run a bounded `serialnexusweb <args>` to completion (kill on timeout) and return
/// its captured stdout. For small one-shot outputs (the `wsclient --rpc` JSON line)
/// whose bytes fit the pipe buffer, so reading after exit is safe.
fn run_web_bounded(args: &[&str], timeout: Duration) -> Option<Vec<u8>> {
    let mut child = Command::new(bin("serialnexusweb"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serialnexusweb");
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut buf = Vec::new();
    child.stdout.take()?.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Drive one JSON-RPC verb through the WebSocket bridge with the headless
/// `serialnexusweb wsclient --rpc`, returning the correlated JSON response (a `result`
/// on success, an `error` when the bridge refuses a denied verb, §17).
fn wsclient_rpc(port: u16, method: &str, timeout: Duration) -> Option<Value> {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let out = run_web_bounded(
        &["wsclient", "--url", &url, "--token", TOKEN, "--rpc", method],
        timeout,
    )?;
    serde_json::from_slice(&out).ok()
}

/// Sorted node names from a `state` result object (`{nodes:[{name,…}],…}`).
fn node_names(state: &Value) -> Vec<String> {
    let mut names: Vec<String> = state["nodes"]
        .as_array()
        .expect("state.nodes array")
        .iter()
        .map(|n| n["name"].as_str().expect("node name").to_string())
        .collect();
    names.sort();
    names
}

/// Whether the daemon still accepts control-socket connections. A killed daemon
/// leaves the socket *file* behind, so `exists()` is not liveness; a connect is.
fn daemon_alive(socket: &Path) -> bool {
    UnixStream::connect(socket).is_ok()
}

// ---- a raw RFC 6455 WebSocket client, for frames a real client would never send ----

/// A hand-rolled WebSocket client over loopback: enough of RFC 6455 to complete the
/// handshake, send *masked* client text frames of our exact choosing, and read
/// unmasked server frames back. It exists because the properties under test are about
/// bytes no well-formed client produces — a frame carrying two newline-separated
/// requests (WEB-1), and an upgrade carrying a foreign `Origin` (SEC-3/WEB-7) — which
/// `serialnexusweb wsclient` cannot express: it serialises its own single request.
struct Ws {
    stream: TcpStream,
}

impl Ws {
    /// Open `/ws`, returning the client on a `101`, or `Err(status)` for whatever the
    /// server answered instead (403 for a refused Origin, 401 without the cookie).
    fn connect(
        port: u16,
        host: &str,
        cookie: Option<&str>,
        origin: Option<&str>,
    ) -> Result<Ws, u16> {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        let mut ws = Ws { stream };
        // The key is not validated by the server (it only hashes it into the accept
        // digest), so RFC 6455 §1.3's own example nonce keeps this deterministic.
        let mut req = format!(
            "GET /ws HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n"
        );
        if let Some(c) = cookie {
            req.push_str(&format!("Cookie: {c}\r\n"));
        }
        if let Some(o) = origin {
            req.push_str(&format!("Origin: {o}\r\n"));
        }
        req.push_str("\r\n");
        ws.stream
            .write_all(req.as_bytes())
            .expect("write WS upgrade");
        ws.stream.flush().expect("flush WS upgrade");

        // Byte-at-a-time to the blank line: the bridge subscribes to the daemon the
        // instant it is built, so server frames follow the 101 immediately and a
        // buffered read would swallow them.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut head = Vec::new();
        loop {
            match ws.read_bytes(1, deadline) {
                Some(b) => head.push(b[0]),
                None => panic!(
                    "no complete HTTP response head from the WS upgrade; got {:?}",
                    String::from_utf8_lossy(&head)
                ),
            }
            if head.len() >= 4 && &head[head.len() - 4..] == b"\r\n\r\n" {
                break;
            }
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let status: u16 = text
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or_else(|| panic!("unparsable upgrade response: {text:?}"));
        if status == 101 { Ok(ws) } else { Err(status) }
    }

    /// Read exactly `n` bytes by `deadline`, or `None` (deadline, EOF, or error).
    fn read_bytes(&mut self, n: usize, deadline: Instant) -> Option<Vec<u8>> {
        let mut out = vec![0u8; n];
        let mut got = 0;
        while got < n {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            self.stream
                .set_read_timeout(Some((deadline - now).max(Duration::from_millis(1))))
                .ok()?;
            match self.stream.read(&mut out[got..]) {
                Ok(0) => return None,
                Ok(k) => got += k,
                Err(_) => return None, // WouldBlock at the deadline, or closed
            }
        }
        Some(out)
    }

    /// Send one text frame, masked as RFC 6455 requires of a client.
    fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mut frame = vec![0x81u8]; // FIN + opcode 1 (text)
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            assert!(payload.len() <= u16::MAX as usize, "test frame too large");
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream.write_all(&frame).expect("write WS frame");
        self.stream.flush().expect("flush WS frame");
    }

    /// The next complete server message's payload (continuation frames reassembled,
    /// control frames skipped, a close frame ending the stream), or `None`.
    fn recv_message(&mut self, deadline: Instant) -> Option<Vec<u8>> {
        let mut payload = Vec::new();
        loop {
            let hdr = self.read_bytes(2, deadline)?;
            let fin = hdr[0] & 0x80 != 0;
            let opcode = hdr[0] & 0x0f;
            let masked = hdr[1] & 0x80 != 0;
            assert!(!masked, "a server frame must not be masked (RFC 6455 §5.1)");
            let len = match hdr[1] & 0x7f {
                126 => {
                    let b = self.read_bytes(2, deadline)?;
                    u16::from_be_bytes([b[0], b[1]]) as usize
                }
                127 => {
                    let b = self.read_bytes(8, deadline)?;
                    let mut n = [0u8; 8];
                    n.copy_from_slice(&b);
                    u64::from_be_bytes(n) as usize
                }
                n => n as usize,
            };
            let body = if len == 0 {
                Vec::new()
            } else {
                self.read_bytes(len, deadline)?
            };
            if opcode == 0x8 {
                return None; // close
            }
            if opcode >= 0x8 {
                continue; // ping/pong — not part of a message
            }
            payload.extend_from_slice(&body);
            if fin {
                return Some(payload);
            }
        }
    }

    /// Collect the JSON-RPC *responses* (as opposed to the daemon's id-less
    /// notifications) the server sends back: everything carrying an `error`, or a
    /// non-zero `id`. Waits up to `first` for the first one, then a further `tail` for
    /// any that follow — so "exactly one reply, and it is the refusal" is a real
    /// assertion rather than a race the fast path wins.
    fn collect_replies(&mut self, first: Duration, tail: Duration) -> Vec<Value> {
        let mut out = Vec::new();
        let mut deadline = Instant::now() + first;
        while let Some(msg) = self.recv_message(deadline) {
            let Ok(v) = serde_json::from_slice::<Value>(&msg) else {
                continue;
            };
            let is_reply =
                v.get("error").is_some() || v.get("id").is_some_and(|id| !id.is_null() && *id != 0);
            if is_reply {
                out.push(v);
                deadline = Instant::now() + tail;
            }
        }
        out
    }
}

// ---- (5) bind policy: a non-loopback plaintext bind is refused (§15.29) ----------

#[test]
fn web_non_loopback_plaintext_bind_is_refused() {
    // No daemon needed: the tier check bails before any socket use. Runs everywhere.
    let run = TempRun::new();
    let socket_str = run.socket().to_string_lossy().into_owned();
    let out = Command::new(bin("serialnexusweb"))
        .args([
            "--bind",
            "0.0.0.0:0",
            "--token",
            TOKEN,
            "--socket",
            &socket_str,
        ])
        .env("XDG_RUNTIME_DIR", run.path())
        .output()
        .expect("run serialnexusweb");

    assert!(
        !out.status.success(),
        "a non-loopback --bind without --tls/--insecure-bind must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(
        stderr.contains("insecure-bind") || stderr.contains("loopback") || stderr.contains("15.29"),
        "the refusal must state the documented reason (§15.29); stderr was: {stderr}"
    );
}

// ---- (1)-(4) the HTTP security gates, on a no-hardware rig ------------------------

#[test]
fn web_http_security_gates() {
    // Pure HTTP: the token/Host gates and asset serving never touch the daemon, so
    // this runs on every platform. A live daemon still backs the socket for realism.
    let d = Daemon::start();
    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    // (1) no token → 401.
    assert_eq!(
        http_status(port, "GET", "/app.js", "127.0.0.1", &[]),
        401,
        "GET /app.js without a token should be 401"
    );
    // (2) bad Host → 403, checked before the token (even with the right token).
    assert_eq!(
        http_status(
            port,
            "GET",
            &format!("/?token={TOKEN}"),
            "evil.example",
            &[]
        ),
        403,
        "a bad Host should be 403"
    );
    // (3) bootstrap: right token → 302 (+cookie); wrong token → 401.
    assert_eq!(
        http_status(port, "GET", &format!("/?token={TOKEN}"), "127.0.0.1", &[]),
        302,
        "the bootstrap URL with the token should 302"
    );
    assert_eq!(
        http_status(port, "GET", "/?token=wrong", "127.0.0.1", &[]),
        401,
        "the bootstrap URL with a wrong token should 401"
    );
    // (4) a valid cookie → 200 for the app and the index.
    let cookie = cookie_header();
    let auth: &[(&str, &str)] = &[("Cookie", cookie.as_str())];
    assert_eq!(
        http_status(port, "GET", "/app.js", "127.0.0.1", auth),
        200,
        "GET /app.js with the cookie should be 200"
    );
    assert_eq!(
        http_status(port, "GET", "/", "127.0.0.1", auth),
        200,
        "GET / with the cookie should be 200"
    );

    // (5) every ES module `app.js` imports must serve too (§11.9) — a 404 here breaks
    // the module import chain in the browser and the console never boots. The list is
    // *derived from app.js's own import statements* rather than restated, so the next
    // module (as `saver.mjs` was) cannot be added to the chain and forgotten here.
    let app_js = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../serialnexusweb/src/assets/app.js"
    ))
    .expect("read serialnexusweb/src/assets/app.js");
    let mut checked = Vec::new();
    for stmt in app_js.split("from \"").skip(1) {
        let spec = stmt.split('"').next().unwrap_or("");
        if !spec.starts_with('/') {
            continue; // a bare/relative specifier is not something we serve
        }
        assert_eq!(
            http_status(port, "GET", spec, "127.0.0.1", auth),
            200,
            "app.js imports {spec}, which the server does not serve"
        );
        checked.push(spec.to_string());
    }
    assert!(
        checked.len() >= 3,
        "expected app.js's module imports to be found, got {checked:?} — has the \
         import syntax changed? (this check must not pass vacuously)"
    );
    // …and the check is not "everything is 200": an unserved path still 404s.
    assert_eq!(
        http_status(port, "GET", "/no-such-module.mjs", "127.0.0.1", auth),
        404,
        "an unknown asset path must 404 (otherwise the check above proves nothing)"
    );
}

// ---- (6a) the WS bridge relays state and enforces the §17 verb filter -------------

#[test]
fn web_ws_bridge_relays_state_and_enforces_denylist() {
    // A pty console needs no serial device, so this runs everywhere.
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
"#,
        console = console.display(),
    );
    rpc.load_toml(&cfg, false).expect("load pty config");
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );

    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    // `state` via the WS bridge lists the console, and the list matches the daemon's
    // directly (the bridge is a faithful relay, §17).
    let ws_state = wsclient_rpc(port, "state", Duration::from_secs(15))
        .expect("no state response via the WS bridge");
    let ws_names = node_names(&ws_state["result"]);
    assert!(
        ws_names.iter().any(|n| n == "console"),
        "state via the WS bridge did not list the console: {ws_state}"
    );
    let daemon_names = node_names(&rpc.state());
    assert_eq!(
        ws_names, daemon_names,
        "console list via the WS bridge != the daemon's directly"
    );

    // A graph-mutating verb is refused at the bridge, never reaching the daemon (§17).
    let ws_load = wsclient_rpc(port, "load", Duration::from_secs(15))
        .expect("no response for a bridged load");
    assert_eq!(
        ws_load["error"]["code"], -32601,
        "a load via the WS bridge should be refused with -32601 (§17): {ws_load}"
    );
    assert_eq!(
        node_names(&rpc.state()),
        daemon_names,
        "a refused load must not have reached the daemon"
    );
}

// ---- (7) WEB-1/SEC-1: one frame is exactly one request ---------------------------

/// The graph the smuggling tests run against: two pty consoles, so a `teardown` that
/// got through would be visible as an emptied node list (the reviewer saw
/// `{"torn_down":2}`).
fn two_console_graph(d: &Daemon) -> Vec<String> {
    let rpc = d.rpc();
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "c1"
path = "{a}"
[[node]]
type = "pty"
name = "c2"
path = "{b}"
"#,
        a = d.run().join("c1").display(),
        b = d.run().join("c2").display(),
    );
    rpc.load_toml(&cfg, false).expect("load two pty consoles");
    let names = node_names(&rpc.state());
    assert_eq!(names, vec!["c1".to_string(), "c2".to_string()]);
    names
}

#[test]
fn web_ws_frame_cannot_smuggle_a_second_request() {
    // Review WEB-1/SEC-1 (critical), reproduction §7.1: `screen()` returned `None`
    // both for "forward it" and "I could not parse it", so a frame holding two
    // newline-separated requests skipped screening entirely and the raw bytes were
    // written to the daemon's NDJSON socket, which split them into two dispatches —
    // the second being `teardown`. Needs no serial device; runs everywhere.
    let d = Daemon::start();
    let before = two_console_graph(&d);
    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    let cookie = cookie_header();
    let host = format!("127.0.0.1:{port}");
    let mut ws = Ws::connect(port, &host, Some(&cookie), None).expect("WS upgrade refused");

    // First, the property that always held and must not be lost: a denied verb alone
    // in its own frame is refused with -32601, id preserved for correlation.
    ws.send_text("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"teardown\"}");
    let replies = ws.collect_replies(Duration::from_secs(10), Duration::from_millis(500));
    assert_eq!(
        replies.len(),
        1,
        "one frame, one reply expected; got {replies:?}"
    );
    assert_eq!(replies[0]["id"], 7, "the refusal keeps the id: {replies:?}");
    assert_eq!(
        replies[0]["error"]["code"], -32601,
        "a lone denied verb must be refused with -32601: {replies:?}"
    );

    // Now the smuggled pair, byte-for-byte the reviewer's frame.
    ws.send_text(
        "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"info\"}\n\
         {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"teardown\"}",
    );
    let replies = ws.collect_replies(Duration::from_secs(10), Duration::from_secs(1));
    assert_eq!(
        replies.len(),
        1,
        "a two-request frame must produce exactly one refusal, never an `info` result \
         followed by a `teardown` result: {replies:?}"
    );
    assert_eq!(
        replies[0]["error"]["code"], -32600,
        "the multi-request frame must be refused as an invalid request: {replies:?}"
    );
    assert!(
        replies.iter().all(|r| r["id"] != 9),
        "nothing correlated to the smuggled request may come back: {replies:?}"
    );

    // The graph survives — the assertion the review asked for by name.
    assert!(
        daemon_alive(&d.socket()),
        "the daemon must still be running after a smuggled teardown"
    );
    assert_eq!(
        node_names(&d.rpc().state()),
        before,
        "the smuggled `teardown` reached the daemon and emptied the graph"
    );
}

#[test]
fn web_ws_frame_cannot_smuggle_a_shutdown() {
    // The reviewer's second run: the same frame with `shutdown` behind the newline
    // killed the daemon *process*. Split from the teardown case so the failure mode is
    // unambiguous (a dead daemon vs. an emptied graph).
    let d = Daemon::start();
    let before = two_console_graph(&d);
    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    let cookie = cookie_header();
    let host = format!("127.0.0.1:{port}");
    let mut ws = Ws::connect(port, &host, Some(&cookie), None).expect("WS upgrade refused");
    ws.send_text(
        "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"info\"}\n\
         {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"shutdown\"}",
    );
    let replies = ws.collect_replies(Duration::from_secs(10), Duration::from_secs(1));
    assert_eq!(
        replies.len(),
        1,
        "a two-request frame must produce exactly one refusal: {replies:?}"
    );
    assert_eq!(
        replies[0]["error"]["code"], -32600,
        "the multi-request frame must be refused as an invalid request: {replies:?}"
    );

    // The daemon process is still there, and still serving the same graph. A dead
    // daemon is the exact observation the review recorded, so it is asserted first and
    // separately from the graph read (which would panic against a dead socket).
    assert!(
        daemon_alive(&d.socket()),
        "the smuggled `shutdown` reached the daemon and killed the process"
    );
    assert_eq!(node_names(&d.rpc().state()), before);
}

// ---- (8) SEC-3/WEB-7: Origin is validated, port-exactly, against Host -------------

#[test]
fn web_origin_is_validated_against_the_requests_own_host() {
    // Review SEC-3/WEB-7 (high, upgraded by the blind pass): cookies are not
    // port-scoped, so a page served from another port on this same host is same-*site*
    // and `SameSite=Strict` still attaches our session cookie to its requests. Origin
    // is the header that carries the port, and it is compared against the request's own
    // Host (not the bound port) so SSH forwarding of the loopback default keeps working
    // (§17). Needs no serial device; runs everywhere.
    let d = Daemon::start();
    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");
    let host = format!("127.0.0.1:{port}");
    let sibling = format!("http://127.0.0.1:{}", port.wrapping_add(1));
    let ours = format!("http://127.0.0.1:{port}");
    let cookie = cookie_header();

    // A plain request from a sibling port on this very host: refused, cookie or not.
    assert_eq!(
        http_status(
            port,
            "GET",
            "/app.js",
            &host,
            &[("Cookie", cookie.as_str()), ("Origin", sibling.as_str())]
        ),
        403,
        "a sibling-port Origin must be refused (its page shares our cookie jar)"
    );
    // The same request from this very authority: served.
    assert_eq!(
        http_status(
            port,
            "GET",
            "/app.js",
            &host,
            &[("Cookie", cookie.as_str()), ("Origin", ours.as_str())]
        ),
        200,
        "an Origin naming this very authority must be served"
    );

    // The WebSocket upgrade — the console's actual door — carries the same gate.
    match Ws::connect(port, &host, Some(&cookie), Some(&sibling)) {
        Ok(_) => panic!("the WS upgrade accepted a sibling-port Origin"),
        Err(status) => assert_eq!(status, 403, "a refused upgrade should be 403"),
    }
    Ws::connect(port, &host, Some(&cookie), Some(&ours))
        .expect("the WS upgrade must accept an Origin naming this very authority");

    // An *absent* Origin is accepted. This is the shipped judgement, stated in
    // `server.rs`: browsers always send Origin on a WebSocket handshake and on
    // cross-origin fetches, so the browser-borne attack this closes cannot omit it,
    // while non-browser clients (`serialnexusweb wsclient`, curl, websocat) never send
    // one — refusing them would break the §17 headless client. It deliberately differs
    // from the Host arm, which refuses an absent header, because Host is mandatory in
    // HTTP/1.1 and Origin is not.
    Ws::connect(port, &host, Some(&cookie), None)
        .expect("an absent Origin must still be accepted (the §17 headless client)");
    assert_eq!(
        http_status(
            port,
            "GET",
            "/app.js",
            &host,
            &[("Cookie", cookie.as_str())]
        ),
        200,
        "an absent Origin must still be served"
    );

    // Origin is checked *after* Host and *before* the token: a foreign Origin is 403
    // even with no cookie at all, so an unauthenticated cross-origin probe learns
    // nothing about token validity.
    assert_eq!(
        http_status(
            port,
            "GET",
            "/app.js",
            &host,
            &[("Origin", sibling.as_str())]
        ),
        403,
        "a foreign Origin must be refused before the token decision"
    );
}

// ---- (9) WEB-3: the pre-auth path is bounded in connections and in time -----------

#[test]
fn web_pre_auth_connections_are_capped_and_time_out() {
    // Review SEC-3/CP-5/WEB-3: `read_request` was a byte-at-a-time loop with no
    // deadline and no cap on in-flight connections, so a peer that connected and said
    // nothing pinned a task and an fd forever — reachable by unauthenticated peers in
    // the sanctioned `--tls`/`--insecure-bind` tiers, both of which are pre-auth.
    //
    // Both bounds are proven in one pass, because they interlock: fill the cap with
    // silent peers (the newest connection is then dropped immediately), then show the
    // head deadline releases them (a 408, and the server serving again). The test costs
    // one HEAD_TIMEOUT of wall clock — deliberately not `#[ignore]`d, since an
    // unbounded pre-auth path is exactly the kind of regression that must not wait for
    // the nightly sweep.
    let run = TempRun::new();
    // No daemon: nothing here reaches `/ws`, and the gates run before the socket is
    // ever touched.
    let server = WebServer::spawn("127.0.0.1:0", &run.socket(), run.path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    // Fill the cap with peers that connect and send nothing.
    let mut silent: Vec<TcpStream> = Vec::with_capacity(MAX_CONNECTIONS);
    for i in 0..MAX_CONNECTIONS {
        let s = TcpStream::connect(("127.0.0.1", port))
            .unwrap_or_else(|e| panic!("connect silent peer {i}: {e}"));
        s.set_read_timeout(Some(Duration::from_millis(300))).ok();
        silent.push(s);
    }

    // The next connection is dropped rather than queued: it EOFs at once with no
    // response. `wait_until` absorbs the accept lag; each probe socket is closed as it
    // goes out of scope, so probing never accumulates connections of its own.
    let capped = wait_until(Duration::from_secs(10), || {
        let Ok(mut probe) = TcpStream::connect(("127.0.0.1", port)) else {
            return true; // the listener itself pushed back — also a bound
        };
        probe
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        let mut byte = [0u8; 1];
        matches!(probe.read(&mut byte), Ok(0))
    });
    assert!(
        capped,
        "with {MAX_CONNECTIONS} connections in flight the next one must be dropped, \
         not accepted and held (an unbounded pre-auth path)"
    );

    // The head deadline releases a silent peer with a 408 — the mechanism itself,
    // observed on one of the peers we are holding open.
    let started = Instant::now();
    let mut held = silent.remove(0);
    held.set_read_timeout(Some(HEAD_TIMEOUT + Duration::from_secs(20)))
        .ok();
    let mut status_line = String::new();
    // A read error here is the regression itself — the peer was still being held when
    // our own (longer) timeout expired — so it is reported as such rather than as a
    // harness panic.
    let read = BufReader::new(&mut held).read_line(&mut status_line);
    assert!(
        read.is_ok() && status_line.starts_with("HTTP/1.1 408"),
        "a silent peer must be released by the head deadline with a 408; after {:?} \
         the read gave {read:?} and the status line was {status_line:?}",
        started.elapsed()
    );

    // …and the permits it held come back, so the server serves again. Under a missing
    // deadline this never recovers, which is the failure the review described.
    let recovered = wait_until(Duration::from_secs(30), || {
        try_http_status(port, "GET", "/app.js", "127.0.0.1", &[]) == Some(401)
    });
    assert!(
        recovered,
        "the server never recovered after the silent peers' deadline elapsed"
    );
    drop(silent);
}

// ---- (6b) the WebSocket byte stream, end to end (needs a serial device) ----------

#[test]
fn web_ws_byte_stream_end_to_end() {
    // Needs a sim pty acting as a serial device (Linux); skip on macOS (§5).
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP web_ws_byte_stream_end_to_end: no serial device on this platform");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let console = d.run().join("console");

    // A free-for-all serial node over an echo device, fed targetward by a pty console:
    // the seeded batch written into the console rides device → serial and echoes back
    // hostward, where the web tap on `usb0` observes it byte-for-byte.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[edge]]
a = "usb0"
b = "console"
"#,
        console = console.display(),
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false).expect("load echo config");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("console", "active", Duration::from_secs(10)),
        "console not active: {:?}",
        rpc.node("console")
    );
    assert!(
        wait_until(Duration::from_secs(5), || console.exists()),
        "console pty symlink never appeared"
    );

    let server = WebServer::spawn("127.0.0.1:0", &d.socket(), d.run().path(), &[]);
    let port = server
        .port("http", Duration::from_secs(10))
        .expect("web server never printed its bound http URL");

    // Start the headless WS tap on usb0 first, capturing its decoded stdout in a
    // reader thread (256 KiB overflows the pipe buffer, so it must be drained live).
    let url = format!("ws://127.0.0.1:{port}/ws");
    let n_str = N.to_string();
    let mut ws_child = Command::new(bin("serialnexusweb"))
        .args([
            "wsclient",
            "--url",
            &url,
            "--token",
            TOKEN,
            "--endpoint",
            "usb0",
            "--bytes",
            &n_str,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serialnexusweb wsclient");
    let ws_stdout = ws_child.stdout.take().expect("piped wsclient stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = BufReader::new(ws_stdout).read_to_end(&mut buf);
        buf
    });
    let mut ws = Kill(ws_child);

    // The server's bridge opened a daemon tap on usb0; wait for it to register.
    assert!(
        wait_until(Duration::from_secs(10), || {
            rpc.state()["taps"]
                .as_array()
                .is_some_and(|t| t.iter().any(|x| x["endpoint"].as_str() == Some("usb0")))
        }),
        "the web tap did not register in the daemon (taps={:?})",
        rpc.state()["taps"]
    );

    // Release the source: N seeded bytes flow console → serial → echo → the web tap.
    // The echo verdict's `sha256_sent` is the byte-exact ground truth (§5).
    let console_str = console.to_string_lossy().into_owned();
    let seed = SEED.to_string();
    let verdict = Sim::client(&[
        "--path",
        &console_str,
        "--send",
        "seeded:256KiB",
        "--expect",
        "echo",
        "--seed",
        &seed,
        "--timeout-ms",
        "40000",
    ]);
    assert_eq!(
        verdict["pass"].as_bool(),
        Some(true),
        "256 KiB echo did not round-trip: {verdict}"
    );
    assert_eq!(
        verdict["received"].as_u64(),
        Some(N),
        "echo received != 256 KiB: {verdict}"
    );
    let src_sha = verdict["sha256_sent"]
        .as_str()
        .expect("client reported sha256_sent")
        .to_owned();

    // Wait for the wsclient to read its N bytes and exit, then join the reader thread.
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut exited = false;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = ws.0.try_wait() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        exited,
        "wsclient did not finish reading {N} bytes within the deadline"
    );
    assert!(
        ws.0.wait().is_ok_and(|s| s.success()),
        "wsclient exited unsuccessfully before delivering {N} bytes"
    );
    let ws_bytes = reader.join().expect("wsclient reader thread panicked");

    assert_eq!(
        ws_bytes.len() as u64,
        N,
        "the WS stream delivered {} bytes, expected {N}",
        ws_bytes.len()
    );
    assert_eq!(
        sha256_hex(&ws_bytes),
        src_sha,
        "the WS byte stream checksum != the source (browser path corrupted or dropped bytes)"
    );
}

// ---- (5b) the TLS tier binds, writes a 0600 key, and permits a non-loopback bind -

#[test]
fn web_tls_tier_binds_and_secures_key() {
    // rustls (ring) + rcgen are cross-platform, so the bind/key-mode checks run
    // everywhere. The HTTPS *request* path is `web_tls_round_trip` below.
    let run = TempRun::new();
    let cert = run.join("tls.crt");
    let key = run.join("tls.key");
    let cert_str = cert.to_string_lossy().into_owned();
    let key_str = key.to_string_lossy().into_owned();

    let server = WebServer::spawn(
        "127.0.0.1:0",
        &run.socket(),
        run.path(),
        &["--tls", "--tls-cert", &cert_str, "--tls-key", &key_str],
    );
    // The TLS server prints an https URL once it is listening (§15.29 tier 2).
    let url = server
        .wait_url("https", Duration::from_secs(15))
        .expect("TLS server never printed its https URL");
    assert!(
        parse_port(&url).is_some(),
        "could not parse the bound TLS port from {url:?}"
    );

    // The generated self-signed pair exists, and the private key is owner-only (0600).
    assert!(cert.exists(), "the TLS cert was not generated at {cert:?}");
    assert!(key.exists(), "the TLS key was not generated at {key:?}");
    let mode = std::fs::metadata(&key)
        .expect("stat tls.key")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "the generated TLS key is mode {mode:o}, want 600"
    );

    // A non-loopback bind is permitted WITH --tls (the same bind the plaintext policy
    // above refused): the server binds and prints an https URL rather than exiting.
    let nl_run = TempRun::new();
    let nl_cert = nl_run.join("nl.crt");
    let nl_key = nl_run.join("nl.key");
    let nl_cert_str = nl_cert.to_string_lossy().into_owned();
    let nl_key_str = nl_key.to_string_lossy().into_owned();
    let nl_server = WebServer::spawn(
        "0.0.0.0:0",
        &nl_run.socket(),
        nl_run.path(),
        &[
            "--tls",
            "--tls-cert",
            &nl_cert_str,
            "--tls-key",
            &nl_key_str,
        ],
    );
    assert!(
        nl_server
            .wait_url("https", Duration::from_secs(15))
            .is_some(),
        "--tls should permit a non-loopback bind (§15.29 tier 2)"
    );
}

// ---- (10) T8: the TLS tier's handshake, round-tripped (plan §11.6) ---------------

/// Run `curl` with the given arguments, returning `(success, stdout)`.
fn curl(args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("curl").args(args).output().ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

#[test]
fn web_tls_round_trip() {
    // Testing item T8: until now the TLS tier was only proven to *bind* — the
    // handshake itself, and therefore the whole tier-2 request path, was never
    // exercised, and the module docs' `// TODO(port)` deferred it to a bash rig that
    // §16.11 has since deleted. Plan §11.6's validation is literally "a `curl --cacert`
    // round-trip over a non-loopback bind passes with the token and fails without it",
    // and `std` ships no TLS client, so curl is the client here (the one surviving
    // curl dependency in this file) and the test **self-skips** without it.
    if curl(&["--version"]).map(|(ok, _)| ok) != Some(true) {
        eprintln!("SKIP web_tls_round_trip: curl not found (plan §11.6 names it as the client)");
        return;
    }

    let d = Daemon::start();
    let run = TempRun::new();
    let cert = run.join("tls.crt");
    let key = run.join("tls.key");
    let cert_str = cert.to_string_lossy().into_owned();
    let key_str = key.to_string_lossy().into_owned();
    // A non-loopback-shaped bind, as §11.6 specifies — the tier TLS exists for. The
    // listener still answers on 127.0.0.1, so no external interface is required.
    let mut server = WebServer::spawn(
        "0.0.0.0:0",
        &d.socket(),
        run.path(),
        &["--tls", "--tls-cert", &cert_str, "--tls-key", &key_str],
    );
    let Some(port) = server.port("https", Duration::from_secs(15)) else {
        assert!(
            server.exited(),
            "the TLS server neither printed an https URL nor exited — it is broken, \
             not merely unable to bind"
        );
        eprintln!("SKIP web_tls_round_trip: this environment cannot bind 0.0.0.0:0 with --tls");
        return;
    };

    // The generated cert's only SAN is `localhost` (see `tls.rs`), so that is the name
    // to request; `--ipv4` keeps curl on the IPv4 listener rather than trying ::1.
    let base = format!("https://localhost:{port}");
    let app = format!("{base}/app.js");
    let bootstrap = format!("{base}/?token={TOKEN}");
    let cookie = cookie_header();
    let max_time = "20";

    // With the token: 200 through a validated TLS handshake.
    let (ok, code) = curl(&[
        "--cacert",
        &cert_str,
        "--ipv4",
        "-sS",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "--max-time",
        max_time,
        "-H",
        &format!("Cookie: {cookie}"),
        &app,
    ])
    .expect("run curl");
    assert!(ok, "the TLS round trip failed at the transport level");
    assert_eq!(code, "200", "with the token, the TLS tier must serve 200");

    // Without it: 401, through the same validated handshake (so this is the token
    // gate answering, not a transport failure).
    let (ok, code) = curl(&[
        "--cacert",
        &cert_str,
        "--ipv4",
        "-sS",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "--max-time",
        max_time,
        &app,
    ])
    .expect("run curl");
    assert!(
        ok,
        "the tokenless TLS request failed at the transport level"
    );
    assert_eq!(code, "401", "without the token, the TLS tier must refuse");

    // A client that does not trust this cert is rejected by certificate validation —
    // the self-signed pair is a lab convenience, not an invitation to skip validation.
    let (ok, _) = curl(&[
        "--ipv4",
        "-sS",
        "-o",
        "/dev/null",
        "--max-time",
        max_time,
        &app,
    ])
    .expect("run curl");
    assert!(
        !ok,
        "an untrusted client must be rejected by cert validation (curl succeeded \
         without --cacert)"
    );

    // The tier-2 bootstrap cookie carries `Secure` (review WEB-2): without it the
    // token the cookie exists to keep out of URLs is still attached to any plaintext
    // request to this same host.
    let (ok, head) = curl(&[
        "--cacert",
        &cert_str,
        "--ipv4",
        "-sS",
        "-o",
        "/dev/null",
        "-D",
        "-",
        "--max-time",
        max_time,
        &bootstrap,
    ])
    .expect("run curl");
    assert!(
        ok,
        "the bootstrap TLS request failed at the transport level"
    );
    assert!(
        head.contains(" 302 "),
        "the bootstrap URL with the token should 302; head was:\n{head}"
    );
    let set_cookie = head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap_or_else(|| panic!("no Set-Cookie in the bootstrap response:\n{head}"));
    assert!(
        set_cookie.contains("Secure"),
        "the TLS tier's session cookie must carry Secure (review WEB-2): {set_cookie:?}"
    );
    assert!(
        set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Strict"),
        "the cookie's existing flags must survive: {set_cookie:?}"
    );
}
