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
//! 4. **A valid cookie serves the app** (200) for `/app.js` and `/`.
//! 5. **The bind policy** (§15.29): a non-loopback plaintext bind without
//!    `--tls`/`--insecure-bind` exits non-zero with the documented reason; the TLS
//!    tier binds an `https://` listener, writes a 0600 key, and *permits* a
//!    non-loopback bind.
//! 6. **The WS bridge** relays `state` and enforces the §17 denylist (a graph verb
//!    like `load` is refused at the bridge, never reaching the daemon), and the
//!    end-to-end WebSocket byte stream checksums byte-exact against the seeded source
//!    (headless `serialnexusweb wsclient` → server → daemon → device).
//!
//! Deviations from the bash, each preserving the original *assertions*:
//! * `curl -w '%{http_code}'` → a hand-rolled raw-`TcpStream` HTTP/1.1 client that
//!   reads the status line (portable, no `curl` flag divergence).
//! * `sed`/`grep` on the server's stdout for the bound port → a stdout reader thread
//!   plus a bounded scan for the printed `http(s)://…` URL.
//! * `sha256sum`/`stat -c %s` on the WS byte dump → [`sha256_hex`] + `Vec::len`; the
//!   source checksum comes from the `nexus-sim client` echo verdict's `sha256_sent`
//!   (the harness discards a `pty --source`'s stdout checksum, §5 / p3_log), driving
//!   the same "the browser byte path is lossless and byte-exact" property over the
//!   sanctioned echo helper.
//! * The plaintext HTTP gates, the WS bridge relay/denylist, the bind-policy refusal,
//!   and the TLS-tier bind/key-mode/non-loopback checks need **no serial device**, so
//!   they run on every platform. The end-to-end WS byte stream needs a serial device
//!   ([`serial_echo`]) and so **skips** on macOS (§5).
//!
//! `// TODO(port)`: the TLS *HTTPS request* checks (a valid cookie → 200, a request
//! without the token → 401, and the untrusted-self-signed-cert rejection) need a TLS
//! client, which is not available from `std` without a new dependency. They are left
//! to the bash rig / a dedicated CI job (see `needs`/`skips`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
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

/// A minimal raw HTTP/1.1 request over loopback returning the numeric status code —
/// the portable replacement for `curl -s -o /dev/null -w '%{http_code}'`. The server
/// answers `Connection: close`, so a single read of the status line suffices.
fn http_status(port: u16, method: &str, target: &str, host: &str, cookie: Option<&str>) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut req = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(c) = cookie {
        req.push_str(&format!("Cookie: {c}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().expect("flush request");
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .expect("read status line");
    // e.g. "HTTP/1.1 401 Unauthorized"
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("could not parse status from {status_line:?}"))
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
        http_status(port, "GET", "/app.js", "127.0.0.1", None),
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
            None
        ),
        403,
        "a bad Host should be 403"
    );
    // (3) bootstrap: right token → 302 (+cookie); wrong token → 401.
    assert_eq!(
        http_status(port, "GET", &format!("/?token={TOKEN}"), "127.0.0.1", None),
        302,
        "the bootstrap URL with the token should 302"
    );
    assert_eq!(
        http_status(port, "GET", "/?token=wrong", "127.0.0.1", None),
        401,
        "the bootstrap URL with a wrong token should 401"
    );
    // (4) a valid cookie → 200 for the app and the index.
    let cookie = format!("nexus_session={TOKEN}");
    assert_eq!(
        http_status(port, "GET", "/app.js", "127.0.0.1", Some(cookie.as_str())),
        200,
        "GET /app.js with the cookie should be 200"
    );
    assert_eq!(
        http_status(port, "GET", "/", "127.0.0.1", Some(cookie.as_str())),
        200,
        "GET / with the cookie should be 200"
    );
}

// ---- (6a) the WS bridge relays state and enforces the §17 denylist ---------------

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
    assert!(
        ws_load.get("error").is_some(),
        "a load via the WS bridge should be refused (§17): {ws_load}"
    );
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
    // everywhere. The HTTPS request + cert-rejection checks need a TLS client and are
    // deferred (see the module docs' TODO(port) / `needs`).
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
