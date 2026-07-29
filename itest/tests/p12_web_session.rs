//! Phase 12 — the web console's **session and connection lifetime** (review
//! `docs/32-claude-opus-code-review.md`).
//!
//! One file per defect area, following the `p9_*`/`p10_*`/`p11_*` convention (§5).
//! This one pins the two review findings whose subject is a *session* rather than a
//! request:
//!
//! * **WEB-3** — the session cookie was named `nexus_session` on every listener, and a
//!   cookie's jar key is (name, domain, path) and never the port (RFC 6265 §8.5). Two
//!   consoles on one host — the arrangement §17 explicitly prescribes ("it is
//!   single-daemon per instance in v1 — run two for two daemons") — therefore collided
//!   on one jar entry, and opening the second bootstrap URL silently logged the
//!   operator out of the first, unrecoverably: the client's only recovery is a reload,
//!   which 401s. `Request::cookie` compounded it by reading only the *first* value for
//!   the name, so a sibling-port page planting a longer-path cookie broke `/ws` while
//!   `/app.js` still returned 200 — a console that renders normally and never connects.
//! * **WEB-4** — the bridge handled only the browser-gone direction. When the *daemon*
//!   connection ended, the reader task dropped only its clone of the funnel sender
//!   while the main loop parked on the WebSocket still holding the original, so the
//!   writer never saw the channel close and `ws_sink.close()` was never called. With no
//!   keepalive, no timer and no watchdog anywhere in the binary, the browser's socket
//!   stayed open with no Close frame for as long as the tab was idle: the page kept
//!   rendering "connected" over a dead daemon and its own "disconnected — reload to
//!   reconnect" signal never fired.
//!
//! Both need no serial device, so they run on **every** platform (§5). The raw HTTP and
//! RFC 6455 clients below are deliberately local copies rather than shared with
//! `p8_web.rs`: the close-frame assertion needs a reader that hands back the *opcode*,
//! which `p8_web`'s message-level client folds away.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{TempRun, bin, daemon_answers, wait_until};

/// Distinct per-server tokens, so "the right console answered" is a byte-level fact
/// rather than an inference. Same length, since the token compare is length-sensitive.
const TOKEN_A: &str = "aaaatoken0123456789abcdef";
const TOKEN_B: &str = "bbbbtoken0123456789abcdef";

// ---------------------------------------------------------------- child processes --

/// A `serial-nexus-web` child whose printed bootstrap URL is scanned for the OS-chosen
/// port. Killed on drop.
struct WebServer {
    child: Child,
    port: u16,
}

impl WebServer {
    /// Spawn `serial-nexus-web --bind 127.0.0.1:0 --token <token> --socket <socket>` and
    /// wait for the `http://…` line it prints right after binding.
    fn spawn(token: &str, socket: &Path, xdg: &Path) -> WebServer {
        let socket_str = socket.to_string_lossy().into_owned();
        let mut child = Command::new(bin("serial-nexus-web"))
            .args([
                "--bind",
                "127.0.0.1:0",
                "--token",
                token,
                "--socket",
                &socket_str,
            ])
            .env("XDG_RUNTIME_DIR", xdg)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-web");
        // Drained for the child's whole life, not just until the URL appears: the
        // tracing subscriber writes to stdout too, and a pipe whose read end we dropped
        // would take the server down on its next log line.
        let stdout = child.stdout.take().expect("piped serial-nexus-web stdout");
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => sink.lock().unwrap().push(l),
                    Err(_) => break,
                }
            }
        });
        let mut port = None;
        let found = wait_until(Duration::from_secs(10), || {
            for l in lines.lock().unwrap().iter() {
                if let Some(rest) = l.split("http://").nth(1)
                    && let Some(authority) = rest.split('/').next()
                    && let Some((_, p)) = authority.rsplit_once(':')
                    && let Ok(n) = p.trim().parse::<u16>()
                {
                    port = Some(n);
                    return true;
                }
            }
            false
        });
        assert!(
            found,
            "serial-nexus-web never printed its bound http URL; saw {:?}",
            lines.lock().unwrap()
        );
        WebServer {
            child,
            port: port.expect("a port once the URL line was found"),
        }
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A `serial-nexus-daemon` this test owns outright, so it can be **killed mid-session** — the
/// event WEB-4 is about. [`serial_nexus_itest::Daemon`] deliberately keeps its child private
/// and shuts down cleanly on drop, which is the one thing this must not do.
struct OwnedDaemon {
    child: Child,
    run: TempRun,
}

impl OwnedDaemon {
    fn start() -> OwnedDaemon {
        let run = TempRun::new();
        let socket = run.socket();
        let child = Command::new(bin("serial-nexus-daemon"))
            .arg("--socket")
            .arg(&socket)
            .arg("--state-file")
            .arg(run.state_file())
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-daemon");
        let ready = wait_until(Duration::from_secs(10), || {
            socket.exists() && daemon_answers(&socket)
        });
        assert!(ready, "daemon never answered `info` within 10s");
        OwnedDaemon { child, run }
    }

    /// SIGKILL and reap, so the web server's daemon connection dies without a
    /// courtesy close — the routine "the operator restarted serial-nexus-daemon" event.
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        self.kill();
    }
}

// ------------------------------------------------------------------- HTTP helpers --

/// One HTTP/1.1 request, returning `(status, response headers)`. The server answers
/// `Connection: close`, so reading to the blank line is a complete head.
fn http_head(
    port: u16,
    target: &str,
    headers: &[(&str, &str)],
) -> Option<(u16, Vec<(String, String)>)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut req =
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return None, // closed before a complete head
            Ok(_) => buf.push(byte[0]),
            Err(_) => return None,
        }
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Some((status, headers))
}

/// The status of `GET <target>` with the given headers.
fn status(port: u16, target: &str, headers: &[(&str, &str)]) -> u16 {
    http_head(port, target, headers)
        .unwrap_or_else(|| panic!("no HTTP status for GET {target} on :{port}"))
        .0
}

/// Open the bootstrap URL and return the `name=value` pair the server set, exactly as
/// a browser would store and later replay it (attributes stripped).
fn bootstrap_cookie(port: u16, token: &str) -> String {
    let (code, headers) = http_head(port, &format!("/?token={token}"), &[])
        .unwrap_or_else(|| panic!("no response to the bootstrap URL on :{port}"));
    assert_eq!(code, 302, "the bootstrap URL redirects (§15.29)");
    let set = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("the bootstrap response set no cookie: {headers:?}"));
    set.split(';')
        .next()
        .expect("a Set-Cookie value has a name=value pair")
        .trim()
        .to_string()
}

// ------------------------------------------------------- a raw RFC 6455 client ----

/// A raw WebSocket client that hands back **frames**, opcode included, so a Close is
/// distinguishable from a timeout and from an EOF.
///
/// Fill-then-commit, like `p8_web.rs`'s client and for the same reason (§5): `pending`
/// is only ever appended to, and a frame leaves it in one act once it is whole, so a
/// deadline landing inside a frame costs no bytes and does not shift the client's
/// phase.
struct Ws {
    stream: TcpStream,
    pending: Vec<u8>,
}

/// One received frame: the opcode and its payload.
struct Frame {
    opcode: u8,
    payload: Vec<u8>,
}

impl Ws {
    /// Complete the upgrade against `/ws`, or panic naming what came back instead.
    fn connect(port: u16, cookie: &str) -> Ws {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        let mut ws = Ws {
            stream,
            pending: Vec::new(),
        };
        // RFC 6455 §1.3's own example nonce: the server only hashes it into the accept
        // digest, so a fixed one keeps this deterministic.
        let req = format!(
            "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\nCookie: {cookie}\r\n\r\n"
        );
        ws.stream
            .write_all(req.as_bytes())
            .expect("write WS upgrade");
        ws.stream.flush().expect("flush WS upgrade");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut head = Vec::new();
        loop {
            // One byte at a time: anything read past the blank line stays in `pending`
            // for `recv_frame`, so the first server frame is never swallowed.
            assert!(
                ws.fill(1, deadline),
                "no complete HTTP response head from the WS upgrade; got {:?}",
                String::from_utf8_lossy(&head)
            );
            head.push(ws.pending.remove(0));
            if head.len() >= 4 && &head[head.len() - 4..] == b"\r\n\r\n" {
                break;
            }
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let status = text
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("?");
        assert_eq!(status, "101", "the upgrade was refused: {text:?}");
        ws
    }

    /// Append to `pending` until it holds `n` bytes or the deadline passes. Never
    /// consumes, so a failure costs the caller nothing.
    fn fill(&mut self, n: usize, deadline: Instant) -> bool {
        let mut buf = [0u8; 4096];
        while self.pending.len() < n {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            if self
                .stream
                .set_read_timeout(Some((deadline - now).max(Duration::from_millis(1))))
                .is_err()
            {
                return false;
            }
            match self.stream.read(&mut buf) {
                Ok(0) => return false, // EOF
                Ok(k) => self.pending.extend_from_slice(&buf[..k]),
                Err(_) => return false,
            }
        }
        true
    }

    /// Send one masked text frame, as RFC 6455 requires of a client.
    fn send_text(&mut self, text: &str) {
        let payload = text.as_bytes();
        assert!(payload.len() < 126, "test frames stay in the short form");
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let mut frame = vec![0x81u8, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream.write_all(&frame).expect("write WS frame");
        self.stream.flush().expect("flush WS frame");
    }

    /// The next whole frame by `deadline`, or `None` (deadline, EOF, or error).
    fn recv_frame(&mut self, deadline: Instant) -> Option<Frame> {
        if !self.fill(2, deadline) {
            return None;
        }
        let (b0, b1) = (self.pending[0], self.pending[1]);
        assert!(
            b1 & 0x80 == 0,
            "a server frame must not be masked (RFC 6455 §5.1)"
        );
        let (header_len, len) = match b1 & 0x7f {
            126 => {
                if !self.fill(4, deadline) {
                    return None;
                }
                (
                    4,
                    u16::from_be_bytes([self.pending[2], self.pending[3]]) as usize,
                )
            }
            127 => {
                if !self.fill(10, deadline) {
                    return None;
                }
                let mut n = [0u8; 8];
                n.copy_from_slice(&self.pending[2..10]);
                (10, u64::from_be_bytes(n) as usize)
            }
            n => (2, n as usize),
        };
        if !self.fill(header_len + len, deadline) {
            return None;
        }
        // The one commit point: the frame is whole, so take all of it at once.
        let frame: Vec<u8> = self.pending.drain(..header_len + len).collect();
        Some(Frame {
            opcode: b0 & 0x0f,
            payload: frame[header_len..].to_vec(),
        })
    }
}

// ------------------------------------------------------------------------ WEB-3 ----

/// Review **WEB-3**: two consoles on one host, one cookie jar, both still authorized.
///
/// The daemon is deliberately absent — every gate this exercises runs before the
/// control socket is touched, and leaving it out keeps the test about the session.
#[test]
fn two_web_consoles_keep_their_own_sessions_in_one_cookie_jar() {
    let run = TempRun::new();
    let a = WebServer::spawn(TOKEN_A, &run.socket(), run.path());
    let b = WebServer::spawn(TOKEN_B, &run.socket(), run.path());
    assert_ne!(a.port, b.port, "the OS gave two ephemeral ports");

    // Each bootstrap URL sets a cookie named for *its* listener, so the two occupy
    // different (name, domain, path) entries. Before the fix both were `nexus_session`
    // on domain 127.0.0.1 path /, and the second store overwrote the first.
    let cookie_a = bootstrap_cookie(a.port, TOKEN_A);
    let cookie_b = bootstrap_cookie(b.port, TOKEN_B);
    assert_eq!(
        cookie_a,
        format!("nexus_session_{}={TOKEN_A}", a.port),
        "the cookie is named for the listener that set it"
    );
    assert_eq!(cookie_b, format!("nexus_session_{}={TOKEN_B}", b.port));
    assert_ne!(
        cookie_a.split('=').next(),
        cookie_b.split('=').next(),
        "two consoles must not collide on one jar entry (§17: run two for two daemons)"
    );

    // A browser holds both and replays both to either port, because cookies are not
    // port-scoped — which is the whole problem, and now costs nothing. Order is not
    // guaranteed by RFC 6265 §5.4 for equal paths, so both orders are asserted.
    let jar = format!("{cookie_a}; {cookie_b}");
    let jar_reversed = format!("{cookie_b}; {cookie_a}");
    for (name, port) in [("A", a.port), ("B", b.port)] {
        for header in [jar.as_str(), jar_reversed.as_str()] {
            assert_eq!(
                status(port, "/app.js", &[("Cookie", header)]),
                200,
                "console {name} must stay authorized while the other console's cookie \
                 sits beside its own in the jar"
            );
        }
    }

    // And the eviction symptom itself is gone: it used to be that after bootstrapping
    // B, console A saw only B's value under the one shared name. Holding B's cookie
    // *alone* is still (correctly) unauthorized on A — the point is that a real jar
    // never has to be in that state.
    assert_eq!(
        status(a.port, "/app.js", &[("Cookie", cookie_b.as_str())]),
        401,
        "B's token must not authorize A — the isolation is by name, not by luck"
    );

    // The shadowing half of WEB-3: a sibling-port page can plant a cookie of the same
    // name under a longer path (`path=/ws`), and RFC 6265 §5.4 orders the longer path
    // first. A first-value-wins reader saw only the plant, so `/ws` 401'd while
    // `/app.js` still returned 200 — a console that renders and never connects.
    let planted = format!("{}=bogus; {cookie_a}", cookie_a.split('=').next().unwrap());
    for target in ["/app.js", "/ws"] {
        let code = status(a.port, target, &[("Cookie", planted.as_str())]);
        assert_ne!(
            code, 401,
            "a planted cookie must not shadow the session on {target} (got {code})"
        );
    }
}

// ------------------------------------------------------------------------ WEB-4 ----

/// Review **WEB-4**: the browser's WebSocket is closed — with a reason — when the
/// daemon connection ends, instead of being left open over a dead daemon.
#[test]
fn a_bridged_websocket_closes_when_the_daemon_goes_away() {
    let mut daemon = OwnedDaemon::start();
    let web = WebServer::spawn(TOKEN_A, &daemon.run.socket(), daemon.run.path());
    let cookie = bootstrap_cookie(web.port, TOKEN_A);
    let mut ws = Ws::connect(web.port, &cookie);

    // Prove the bridge is live before killing anything, so a Close frame afterwards is
    // unambiguously the daemon's departure and not a failed setup.
    assert!(
        info_round_trips(&mut ws, 7),
        "the bridge never relayed the `info` response"
    );

    daemon.kill();

    // The defect: nothing in this binary polls, pings or times out, so without the
    // bridge noticing the daemon's EOF the socket below simply stays open and silent —
    // and the page goes on rendering "connected". A bounded wait is therefore the whole
    // assertion.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut close: Option<Frame> = None;
    while Instant::now() < deadline {
        let Some(frame) = ws.recv_frame(deadline) else {
            break; // EOF or the deadline: `close` stays whatever we have
        };
        if frame.opcode == 0x8 {
            close = Some(frame);
            break;
        }
    }
    let close = close.expect(
        "the WebSocket must be closed when the daemon goes away — an open socket with \
         no Close frame is a console that reports itself connected to a dead daemon",
    );
    // A reason, not a bare close: `app.js`'s onclose cannot otherwise tell "the daemon
    // went away, restart it and reload" from an ordinary network drop.
    assert!(
        close.payload.len() >= 2,
        "the Close frame carries a code: {:?}",
        close.payload
    );
    let code = u16::from_be_bytes([close.payload[0], close.payload[1]]);
    assert_eq!(code, 1001, "RFC 6455 `Away`: an endpoint is going away");
    let reason = String::from_utf8_lossy(&close.payload[2..]).into_owned();
    assert!(
        reason.contains("daemon"),
        "the close reason must name the cause: {reason:?}"
    );
}

// ------------------------------------------------------------------------ WEB-5 ----

/// Mirrors `web/src/server.rs`'s `MAX_PRE_AUTH_CONNECTIONS`: how many
/// connections may sit before the token gate at once. Enforced by evicting the oldest
/// member, **never** by refusing a newcomer — which is the whole subject below.
const MAX_PRE_AUTH_CONNECTIONS: usize = 32;

/// Three times the cap, so "the pool is saturated" is not a boundary coincidence and the
/// eviction count below is far outside the noise.
const FLOOD: usize = 3 * MAX_PRE_AUTH_CONNECTIONS;

/// Mirrors `web/src/server.rs`'s `HEAD_TIMEOUT`. Nothing waits on it here; it
/// is the alternative explanation the timing check below has to rule out.
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Review **WEB-5**: **an unauthenticated flood cannot deny an authenticated client a
/// _new_ connection.**
///
/// That sentence is the finding, and the first two attempts at a guard both pinned
/// something else. One 128-slot pool served both populations, so any local user — the
/// adversary §15.28 names — pinned every slot at one byte per connection and the
/// operator's browser was reset. The first remediation split off a 32-permit pre-auth
/// semaphore acquired *in the accept loop*, which made the denial four times cheaper:
/// the cookie cannot be known before the head is read, so a full pre-auth pool refused
/// every new connection, the operator's included. Its guard asserted that the 33rd
/// pre-auth connection was dropped — i.e. it asserted the lockout — and that an
/// **already established** WebSocket kept working, which had been true before the split
/// too (32 silent peers left 96 of 128 slots free).
///
/// So this pins the connection the operator actually needs: a brand-new one, made while
/// the pre-auth pool is saturated, carrying a valid session cookie. Fail-first is
/// measured, not assumed (§15.36): against the shipped binary before the fix, 31 silent
/// peers still gave `HTTP/1.1 200 OK` on `/app.js` and 32 gave a connection reset, on
/// `/app.js` and `/ws` alike; after it, 300 held peers and a ~18 000 conn/s reconnect
/// flood both leave every authenticated request served.
///
/// Bounded-ness is asserted beside it, because a "fix" that simply removed the cap would
/// otherwise pass: at least `FLOOD - MAX_PRE_AUTH_CONNECTIONS` of the silent peers must
/// have been closed by the server, while enough are still open to prove the pool really
/// was full when the authenticated probes ran.
#[test]
fn a_pre_authentication_flood_cannot_deny_an_authenticated_client_a_new_connection() {
    let daemon = OwnedDaemon::start();
    let web = WebServer::spawn(TOKEN_A, &daemon.run.socket(), daemon.run.path());
    let cookie = bootstrap_cookie(web.port, TOKEN_A);

    // An authenticated session, established and proven live before the flood starts.
    let mut ws = Ws::connect(web.port, &cookie);
    assert!(
        info_round_trips(&mut ws, 11),
        "the bridge must answer before the flood, or nothing after it means anything"
    );

    // Saturate the pre-auth population several times over with peers that connect and
    // say nothing. The read timeout is short because the only question ever asked of
    // these sockets is "did the server close you", which a closed socket answers at once.
    let started = Instant::now();
    let mut silent: Vec<TcpStream> = Vec::with_capacity(FLOOD);
    for i in 0..FLOOD {
        let s = TcpStream::connect(("127.0.0.1", web.port))
            .unwrap_or_else(|e| panic!("connect silent peer {i}: {e}"));
        s.set_read_timeout(Some(Duration::from_millis(20))).ok();
        silent.push(s);
    }

    // ---- the headline: a *new* authenticated connection is served anyway ----
    // Repeated, so a single lucky accept cannot carry the assertion. Under the defect
    // every one of these was a connection reset before a byte of the head was read.
    for attempt in 0..5 {
        let code = http_head(web.port, "/app.js", &[("Cookie", cookie.as_str())]).map(|(c, _)| c);
        assert_eq!(
            code,
            Some(200),
            "attempt {attempt}: a client presenting a valid session cookie must still be \
             able to open a NEW connection while {FLOOD} unauthenticated peers are \
             holding the pre-auth pool full — got {code:?}. A pre-auth pool that gates \
             the *accept* cannot serve this request, because the cookie is not readable \
             until after the head is read"
        );
    }
    // …including a new WebSocket, which is the connection a reload actually depends on
    // (`app.js` has no reconnect, so a dropped bridge means reloading the page).
    let mut fresh = Ws::connect(web.port, &cookie);
    assert!(
        info_round_trips(&mut fresh, 13),
        "a NEW bridge opened during the flood must work, not just an old one"
    );

    // ---- and the flood is still bounded, by eviction rather than by refusal ----
    let mut closed = 0usize;
    for s in silent.iter_mut() {
        let mut byte = [0u8; 1];
        if matches!(s.read(&mut byte), Ok(0)) {
            closed += 1;
        }
    }
    let open = FLOOD - closed;
    let elapsed = started.elapsed();
    assert!(
        elapsed < HEAD_TIMEOUT,
        "inconclusive rather than wrong: the flood and the probes took {elapsed:?}, past \
         the {HEAD_TIMEOUT:?} head deadline, so a closed peer no longer distinguishes \
         eviction from the deadline. Check the box's load (AGENTS §8) and re-run"
    );
    assert!(
        closed >= FLOOD - MAX_PRE_AUTH_CONNECTIONS,
        "the unauthenticated population must stay bounded: of {FLOOD} silent peers only \
         {open} should survive the {MAX_PRE_AUTH_CONNECTIONS}-slot cap, but {closed} were \
         closed. Serving every one of them would be an unbounded pre-auth path"
    );
    assert!(
        open >= MAX_PRE_AUTH_CONNECTIONS / 2,
        "only {open} of the flood were still open when the probes above ran, so the pool \
         was not actually saturated and the headline assertion proved nothing"
    );

    // …and the operator's established console is entirely unaffected by all of it.
    assert!(
        info_round_trips(&mut ws, 12),
        "an authenticated session must survive a pre-authentication flood"
    );
    drop(silent);
}

/// Send `info` with the given id over an established bridge and report whether the
/// correlated result comes back within a bounded time.
fn info_round_trips(ws: &mut Ws, id: u64) -> bool {
    ws.send_text(&format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"info\"}}"
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let Some(frame) = ws.recv_frame(deadline) else {
            return false;
        };
        if frame.opcode == 0x8 {
            return false; // closed
        }
        if let Ok(v) = serde_json::from_slice::<Value>(&frame.payload)
            && v.get("id") == Some(&Value::from(id))
            && v.get("result").is_some()
        {
            return true;
        }
    }
    false
}
