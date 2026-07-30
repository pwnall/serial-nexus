//! Phase 12 — the web console's **post-handshake WebSocket bounds** (review
//! `docs/37-claude-fable-code-review.md`, 37-WEBS-4).
//!
//! One file per defect area, following the `p9_*`/`p10_*`/`p11_*` convention (§5).
//!
//! §17 and `docs/security.md` both promise that incoming WebSocket messages are capped
//! at 1 MiB and frames at 256 KiB — "the browser→server direction carries JSON-RPC
//! requests only, so the cap costs nothing and bounds what one frame can make the
//! server buffer". The caps were wired (two `WebSocketConfig` calls in `upgrade_ws`)
//! and never asserted: deleting both restored tungstenite's 64 MiB default with the
//! whole suite green, which is the same as not having them, because nothing would ever
//! notice them going away.
//!
//! What the two tests below pin is the property the promise is about — a frame past the
//! cap is refused at the framing layer, the connection ends, and **the request it
//! carried never reaches the daemon** — with a just-under-cap control in the same
//! session, so a "fix" that simply refused large frames wholesale would fail too. The
//! payload is a graph mutation rather than a read, so "it never arrived" is a fact
//! about the daemon's own configuration afterwards rather than an inference from a
//! missing reply. The two caps are exercised separately, one frame over the frame cap
//! and one *fragmented message* over the message cap, so deleting either config call
//! fails a test of its own.
//!
//! Note what is and is not a *promise* here. §17 and `docs/security.md` say the cap
//! "bounds what one frame can make the server buffer"; neither says the connection ends,
//! and 37-WEBS-4 asked for the caps to be **asserted at all**. So the load-bearing
//! assertions are the two that pin the documented property — no reply, and no mutation in
//! the daemon's own configuration — and they run first. `closed()` is kept behind them
//! because a server that quietly discarded the frame and carried on would satisfy both
//! (silence is not refusal), but it is the weaker claim and it must never be what takes a
//! run down.
//!
//! No serial device is involved, so this runs on **every** platform (§5). The raw RFC
//! 6455 client is a local copy, as in `p12_web_session.rs` and `p12_web_token_transport.rs`:
//! this one has to emit frames those cannot — payloads past the 16-bit length form, and
//! a continuation sequence.
//!
//! ## The closure oracle, and why it is not `read() == Ok(0)`
//!
//! This file was green on Linux and red on macOS for one run and amber the next, against
//! a server measured correct on both. The caps were never the problem: `closed()` was.
//! The refusal is enforced at frame-header parse, so the over-cap payload is never
//! drained and the server's `close(2)` runs with unread bytes queued — which is an **RST**,
//! not a FIN, on every mainstream stack. Two consequences the old oracle got wrong, and
//! one Darwin detail that decided which way each run fell:
//!
//! 1. the Close frame the server does send is consumed by `response_arrives` on the line
//!    above (see [`Ws::close_seen`]);
//! 2. the reset surfaces as an `ErrorKind`, and `matches!(read, Ok(0))` scored it the same
//!    as a live-but-quiet peer (see [`ends_a_connection`]);
//! 3. Darwin's `setsockopt(SO_RCVTIMEO)` returns `EINVAL` on a socket carrying a pending
//!    RST, so `fill` returned *without reading* and left the error for the bare probe,
//!    where Linux consumed it in `fill` and left a plain EOF behind. That is the whole of
//!    the platform split, and it is a coin flip either way — Linux was lucky, not right.
//!
//! The predicate is now derived from what the server promises (the session is over)
//! rather than from which of the two shutdown paths a kernel happened to take — the same
//! correction `docs/macos.md` delta 4 states for socket-buffer-derived assertions. It is
//! *stricter* on Linux than what it replaces: a Close frame is now positive evidence the
//! server ended the session deliberately, where an EOF alone is merely consistent with it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use serial_nexus_itest::{Daemon, bin, wait_until};

/// The fixed per-session token, so the cookie below is a byte-level fact.
const TOKEN: &str = "webs4token0123456789abcdef";

/// Mirrors `web/src/server.rs`'s `WS_MAX_MESSAGE` and `WS_MAX_FRAME` — the two limits
/// §17 and `docs/security.md` state. Mirrored rather than imported because the web
/// crate exports no such surface (and should not): what is under test is the shipped
/// binary's behaviour at these numbers, not the constants agreeing with themselves.
const WS_MAX_MESSAGE: usize = 1 << 20;
const WS_MAX_FRAME: usize = 256 * 1024;

// ---------------------------------------------------------------- child process ----

/// A `serial-nexus-web` child whose printed bootstrap URL is scanned for the OS-chosen
/// port. Killed on drop.
struct WebServer {
    child: Child,
    port: u16,
    /// Everything the child has written to stdout, where its tracing subscriber writes.
    ///
    /// Kept rather than dropped after the port is scraped, because the operator-facing
    /// half of a size cap is a **log line**: a bound nobody can see being enforced is a
    /// bound nobody can tell is still there. Asserting on it is what stops the `warn!`
    /// being deleted with this suite green — the same argument 37-WEBS-4 made about the
    /// caps themselves.
    logs: Arc<Mutex<Vec<String>>>,
}

impl WebServer {
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
        // Drained for the child's whole life: the tracing subscriber writes to stdout
        // too, and a pipe whose read end we dropped would take the server down on its
        // next log line.
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
            logs: lines,
        }
    }

    /// Wait, bounded, for a stdout line containing `needle`.
    ///
    /// Polled rather than read once: the refusal is logged by the server on its own
    /// schedule, and the client observes the closed socket first — so a single look is a
    /// race the server usually loses.
    fn logged(&self, needle: &str) -> bool {
        wait_until(Duration::from_secs(10), || {
            self.logs.lock().unwrap().iter().any(|l| l.contains(needle))
        })
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Open the bootstrap URL and return the `name=value` pair a browser would replay for
/// the session cookie — the first `Set-Cookie`, which is the one that authorizes `/ws`.
fn bootstrap_cookie(port: u16) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let req = format!(
        "GET /?token={TOKEN} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().expect("flush request");
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => buf.push(byte[0]),
        }
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let set = text
        .split("\r\n")
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| panic!("the bootstrap response set no cookie:\n{text}"));
    set.split(';')
        .next()
        .expect("a Set-Cookie has a name=value pair")
        .trim()
        .to_string()
}

// ------------------------------------------------------- a raw RFC 6455 client ----

/// One received frame: its opcode and payload.
struct Frame {
    opcode: u8,
    payload: Vec<u8>,
}

/// Why [`Ws::fill`] stopped short of the byte count it was asked for.
///
/// A `bool` here is what made this file fail on macOS and pass on Linux for the same
/// server behaviour. The three ways a read can end — *nothing yet*, a clean FIN, an
/// abortive reset — are three different answers to "is this session over", and
/// collapsing them into one `false` threw away the only copy: a pending socket error is
/// a **one-shot** on every stack (`so_error`/`sk_err` is cleared by the read that
/// reports it), so a later probe cannot ask again. [`Ws::closed`] is the caller that
/// needs the distinction, and it sits one frame above the code that used to discard it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The deadline passed with the request still unmet. The peer is **silent**, which
    /// is emphatically not the same as gone — telling those two apart is the whole job
    /// of [`Ws::closed`].
    Deadline,
    /// `read` returned 0: the peer sent FIN.
    Eof,
    /// `read` failed with this kind.
    Io(std::io::ErrorKind),
}

/// The [`std::io::ErrorKind`]s that mean *this connection is over*, as against "nothing
/// to read yet".
///
/// An RST is a **stronger** ending than a FIN, not a weaker one, and these tests
/// deliberately engineer one: tungstenite enforces the frame cap while parsing the frame
/// *header*, so the server never drains the over-cap payload, and its `close(2)`
/// therefore runs with unread bytes still queued — which every mainstream stack answers
/// with an RST instead of a FIN (RFC 1122 §4.2.2.13). Reading that reset as "not closed"
/// is backwards, and it is what the `matches!(read, Ok(0))` this replaces did.
fn ends_a_connection(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::BrokenPipe
    )
}

/// A raw WebSocket client that can emit payloads past the 16-bit length form and
/// multi-frame (continuation) messages — the two shapes the caps under test are about.
///
/// Fill-then-commit on the receive side, like every other client in this suite (§5):
/// `pending` is only appended to, and a frame leaves it in one act once it is whole, so
/// a deadline landing inside a frame costs no bytes and does not shift the phase.
struct Ws {
    stream: TcpStream,
    pending: Vec<u8>,
    /// The payload of the first Close frame this session saw, latched the moment it is
    /// drained.
    ///
    /// Without the latch the two helpers that read this socket **compete for one frame**:
    /// the server's Close lands inside the ten-second window `response_arrives` is
    /// sitting in, `response_arrives` correctly reports "no reply, the peer closed" — and
    /// drops the frame on the floor. `closed`, called on the very next line, is then
    /// asked to prove the thing that was just discarded. That is not a corner case, it is
    /// the path a *promptly correct* server takes, so the assertion was most likely to
    /// fail against the best-behaved server. Latching in [`Ws::recv_frame`], the one
    /// commit point, means no caller can forget.
    close_seen: Option<Vec<u8>>,
}

impl Ws {
    fn connect(port: u16, cookie: &str) -> Ws {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        // A write deadline as well: the over-cap frames below are deliberately larger
        // than a socket buffer, and a server that neither reads nor closes must fail the
        // test rather than park it.
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .expect("set write timeout");
        let mut ws = Ws {
            stream,
            pending: Vec::new(),
            close_seen: None,
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
            // One byte at a time: anything past the blank line stays in `pending` for
            // `recv_frame`, so the first server frame is never swallowed.
            assert!(
                ws.fill(1, deadline).is_ok(),
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
    /// consumes, so a failure costs the caller nothing — and reports *why* it stopped,
    /// which is the part [`Ws::closed`] cannot reconstruct afterwards (see [`Stop`]).
    fn fill(&mut self, n: usize, deadline: Instant) -> Result<(), Stop> {
        let mut buf = [0u8; 4096];
        while self.pending.len() < n {
            let now = Instant::now();
            if now >= deadline {
                return Err(Stop::Deadline);
            }
            // A failure here is deliberately **not** a reason to stop. Darwin rejects
            // `SO_RCVTIMEO` with `EINVAL` once the socket is carrying a pending RST, so
            // bailing out on it is what let the reset go unobserved on macOS while Linux —
            // where the same call succeeds — consumed it here and left a plain EOF behind
            // for the next read. That divergence, not the caps, is the whole of the macOS
            // failure this file used to have. The read below is the call that reports the
            // reset, and it is the answer `closed` needs; a deadline from a previous
            // iteration (or from `connect`) is still armed, so this cannot block forever.
            let _ = self
                .stream
                .set_read_timeout(Some((deadline - now).max(Duration::from_millis(1))));
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(Stop::Eof),
                Ok(k) => self.pending.extend_from_slice(&buf[..k]),
                // `Read::read` does not retry `EINTR`, and one signal must not be read as
                // a closed connection.
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Stop::Io(e.kind())),
            }
        }
        Ok(())
    }

    /// Emit one frame with an explicit opcode and FIN bit, masked as RFC 6455 §5.3
    /// requires of a client. Errors are handed back rather than asserted: a server that
    /// refuses an over-cap frame may reset the connection part-way through the write,
    /// and that is a legitimate way to observe the refusal.
    fn send_frame(&mut self, opcode: u8, fin: bool, payload: &[u8]) -> std::io::Result<()> {
        let mask = [0x37u8, 0xfa, 0x21, 0x3d];
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(if fin { 0x80 | opcode } else { opcode });
        match payload.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n if n <= u16::MAX as usize => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&frame)?;
        self.stream.flush()
    }

    /// One whole text message in one frame; the ordinary case.
    fn send_text(&mut self, text: &str) -> std::io::Result<()> {
        self.send_frame(0x1, true, text.as_bytes())
    }

    /// One text message split into `parts` fragments — a text frame with FIN clear
    /// followed by continuation frames (RFC 6455 §5.4), which is how a message can
    /// exceed the *message* cap while every frame stays under the frame cap.
    fn send_fragmented_text(&mut self, text: &str, parts: usize) -> std::io::Result<()> {
        let bytes = text.as_bytes();
        let chunk = bytes.len().div_ceil(parts);
        for (i, part) in bytes.chunks(chunk).enumerate() {
            let last = (i + 1) * chunk >= bytes.len();
            let opcode = if i == 0 { 0x1 } else { 0x0 };
            self.send_frame(opcode, last, part)?;
        }
        Ok(())
    }

    /// The next whole frame by `deadline`, or why the wait ended without one.
    fn recv_frame(&mut self, deadline: Instant) -> Result<Frame, Stop> {
        self.fill(2, deadline)?;
        let (b0, b1) = (self.pending[0], self.pending[1]);
        assert!(
            b1 & 0x80 == 0,
            "a server frame must not be masked (RFC 6455 §5.1)"
        );
        let (header_len, len) = match b1 & 0x7f {
            126 => {
                self.fill(4, deadline)?;
                (
                    4,
                    u16::from_be_bytes([self.pending[2], self.pending[3]]) as usize,
                )
            }
            127 => {
                self.fill(10, deadline)?;
                let mut n = [0u8; 8];
                n.copy_from_slice(&self.pending[2..10]);
                (10, u64::from_be_bytes(n) as usize)
            }
            n => (2, n as usize),
        };
        self.fill(header_len + len, deadline)?;
        // The one commit point: the frame is whole, so take all of it at once.
        let frame: Vec<u8> = self.pending.drain(..header_len + len).collect();
        let opcode = b0 & 0x0f;
        let payload = frame[header_len..].to_vec();
        // Latched here, at the single commit point, so that no caller can consume a Close
        // frame without the session recording that it happened — see `Ws::close_seen`.
        if opcode == 0x8 && self.close_seen.is_none() {
            self.close_seen = Some(payload.clone());
        }
        Ok(Frame { opcode, payload })
    }

    /// The status code of the Close frame this session saw, if it saw one carrying a
    /// code. RFC 6455 §5.5.1: `[u16 status code][UTF-8 reason]`, both optional — an empty
    /// payload is a close with *no* code, which a browser reports as 1005.
    fn close_code(&self) -> Option<u16> {
        let payload = self.close_seen.as_ref()?;
        (payload.len() >= 2).then(|| u16::from_be_bytes([payload[0], payload[1]]))
    }

    /// Wait for the correlated response to `id`, reporting whether it arrives before the
    /// connection closes or the deadline passes.
    fn response_arrives(&mut self, id: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Ok(frame) = self.recv_frame(deadline) else {
                return false;
            };
            if frame.opcode == 0x8 {
                return false; // closed — and `recv_frame` has latched it for `closed()`
            }
            if let Ok(v) = serde_json::from_slice::<Value>(&frame.payload)
                && v.get("id") == Some(&Value::from(id))
            {
                return true;
            }
        }
        false
    }

    /// Whether the server has **ended** this session, within a bounded wait — as against
    /// having merely gone quiet.
    ///
    /// A connection that is only *silent* is deliberately not reported as closed: that
    /// distinction is what stops a server which quietly discarded the frame from passing
    /// for one that refused it, and it is the only thing this helper is for.
    ///
    /// Three endings count, and the previous version recognised one of them:
    ///
    /// * a **Close frame**, wherever in the session it was seen — [`Ws::close_seen`]
    ///   explains why it has to be a latch rather than a fresh read;
    /// * **EOF**, a graceful FIN;
    /// * an **abortive close**, which arrives as an error kind, not as `Ok(0)` — see
    ///   [`ends_a_connection`] for why these tests guarantee one.
    ///
    /// Only [`Stop::Deadline`] — nothing at all within the window — is "still open". The
    /// old form asked the socket for a bare `Ok(0)` after `fill` had already consumed and
    /// discarded the reset, so on macOS it reported a session that had demonstrably ended
    /// as open. It was never right on Linux either; it was lucky there.
    fn closed(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.close_seen.is_some() {
                return true;
            }
            match self.recv_frame(deadline) {
                // Ordinary traffic (the daemon's `state` notification arrives unbidden);
                // keep reading until the session ends.
                Ok(_) => continue,
                Err(Stop::Eof) => return true,
                Err(Stop::Io(kind)) => return ends_a_connection(kind),
                Err(Stop::Deadline) => return false,
            }
        }
    }
}

/// RFC 6455's `1009` ("Message Too Big") — the code the bridge closes a refused session
/// with, and the code that says the *cap* is what ended it.
///
/// Checked so that an unrelated ending cannot be mistaken for a cap refusal: `closed()` is
/// satisfied by any ending at all, including the `1001` the WEB-4 daemon-gone path sends,
/// so without this a mid-test daemon departure would let the file go green with both
/// `WebSocketConfig` calls deleted.
///
/// **Conditional on a Close frame arriving**, and that is not a weakness that can be
/// engineered away: the refusal leaves the over-cap payload undrained, so the server's
/// `close(2)` emits an RST that may destroy its own Close frame in flight (`docs/macos.md`
/// delta 6). The frame is best-effort on the wire, so the assertion is best-effort too —
/// it convicts when the evidence arrives and abstains when it does not, which is the
/// honest shape. `closed()` is what holds unconditionally.
const CLOSE_MESSAGE_TOO_BIG: u16 = 1009;

/// The distinguishing fragment of the `warn!` the bridge emits when the framing layer
/// refuses a browser's bytes.
///
/// Matched on a fragment rather than the whole line so that rewording the message does
/// not fail this gate, while deleting it does — the assertion is about an operator being
/// told at all, not about the exact words.
const REFUSAL_LOG: &str = "refusing a browser frame";

// ------------------------------------------------------------------ the payloads ----

/// One `add-node` request adding a log node called `name`, padded with leading
/// whitespace to exactly `total` bytes.
///
/// Whitespace is the padding because JSON ignores it *between* tokens: the screen parses
/// the same request either way, and `bridge` forwards the re-serialised value, so a
/// frame that gets through arrives at the daemon as an ordinary small line. That keeps
/// the daemon's own request-line cap out of the experiment — what is under test is the
/// WebSocket layer, and only it.
///
/// A graph mutation rather than a read, because its absence afterwards is a fact about
/// the daemon rather than an inference from a reply that did not come.
fn padded_add_node(name: &str, directory: &Path, id: u64, total: usize) -> String {
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"add-node","params":{{"node":{{"type":"log","name":"{name}","directory":"{}","filename":"{name}.log"}}}}}}"#,
        directory.display()
    );
    assert!(request.len() < total, "the padding must not be negative");
    let mut padded = " ".repeat(total - request.len());
    padded.push_str(&request);
    padded
}

/// Whether the daemon's configuration holds a node called `name`. An empty graph dumps
/// without a `node` key at all, which is "no such node" rather than a malformed dump.
fn daemon_has_node(dump: &Value, name: &str) -> bool {
    dump.get("node")
        .and_then(Value::as_array)
        .is_some_and(|nodes| nodes.iter().any(|n| n["name"] == name))
}

// ---------------------------------------------------------------------- the tests ----

/// Review **37-WEBS-4**, the frame cap: a single frame past 256 KiB is refused at the
/// framing layer and its request never reaches the daemon — while a frame just under
/// the cap goes through in the same session.
#[test]
fn a_frame_over_the_cap_is_refused_and_its_request_never_reaches_the_daemon() {
    let d = Daemon::start();
    let logs = d.run().join("weblogs");
    std::fs::create_dir_all(&logs).expect("mkdir");
    let web = WebServer::spawn(TOKEN, &d.socket(), d.run().path());
    let cookie = bootstrap_cookie(web.port);
    let mut ws = Ws::connect(web.port, &cookie);

    // Live first, or nothing after it means anything.
    ws.send_text(r#"{"jsonrpc":"2.0","id":1,"method":"info"}"#)
        .expect("write the liveness probe");
    assert!(ws.response_arrives(1), "the bridge never answered `info`");

    // The control: just under the cap, and it works end to end. This is what stops the
    // assertion below being satisfied by a server that refuses everything large.
    let under = padded_add_node("webs4_under_cap", &logs, 2, WS_MAX_FRAME - 1024);
    ws.send_text(&under).expect("write the under-cap frame");
    assert!(
        ws.response_arrives(2),
        "a {}-byte frame is under the {WS_MAX_FRAME}-byte cap and must be served",
        under.len()
    );
    assert!(
        daemon_has_node(&d.rpc().dump(), "webs4_under_cap"),
        "the under-cap request must have reached the daemon, or the over-cap assertion \
         below proves nothing about the cap"
    );

    // The subject: one frame past the cap.
    let over = padded_add_node("webs4_over_frame", &logs, 3, WS_MAX_FRAME + 1024);
    // A write error is one of the ways the refusal shows up, so it is not a failure.
    let _ = ws.send_text(&over);
    assert!(
        !ws.response_arrives(3),
        "an over-cap frame was served: the {WS_MAX_FRAME}-byte frame cap §17 and \
         docs/security.md promise is not in force"
    );
    // Before the closure assertion, not after it: this is the strongest and the most
    // OS-independent evidence in the file — the one the module doc is built around — and
    // ordering it behind `closed()` meant that a closure oracle which misjudged *how* the
    // session ended took the cap assertion down with it, unrun.
    assert!(
        !daemon_has_node(&d.rpc().dump(), "webs4_over_frame"),
        "the request inside an over-cap frame reached the daemon and mutated the graph"
    );
    assert!(
        ws.closed(),
        "an over-cap frame must end the connection, not be silently skipped"
    );
    if let Some(code) = ws.close_code() {
        assert_eq!(
            code, CLOSE_MESSAGE_TOO_BIG,
            "the session ended with close code {code}, not {CLOSE_MESSAGE_TOO_BIG} — \
             something other than the frame cap ended it, so this run proves nothing \
             about the cap"
        );
    }
    assert!(
        web.logged(REFUSAL_LOG),
        "the server refused an over-cap frame and said nothing about it: an operator \
         watching this daemon cannot tell a tripped security cap from an idle browser \
         disconnect"
    );
}

/// Review **37-WEBS-4**, the message cap: a *fragmented* message past 1 MiB is refused
/// even though every one of its frames is under the frame cap.
///
/// Its own test because the two caps are two configuration calls, and one of them
/// standing alone must not make the suite green: without the message cap this message
/// accumulates to 1.2 MiB inside the server and is forwarded, and no frame in it ever
/// touches the frame cap.
#[test]
fn a_fragmented_message_over_the_cap_is_refused_and_never_reaches_the_daemon() {
    let d = Daemon::start();
    let logs = d.run().join("weblogs");
    std::fs::create_dir_all(&logs).expect("mkdir");
    let web = WebServer::spawn(TOKEN, &d.socket(), d.run().path());
    let cookie = bootstrap_cookie(web.port);
    let mut ws = Ws::connect(web.port, &cookie);

    ws.send_text(r#"{"jsonrpc":"2.0","id":1,"method":"info"}"#)
        .expect("write the liveness probe");
    assert!(ws.response_arrives(1), "the bridge never answered `info`");

    // The control, fragmented too: a multi-frame message under the cap must be
    // assembled and served, so what the assertion below catches is the *size* and not
    // continuation frames as such.
    let parts = 6;
    let under = padded_add_node("webs4_under_message", &logs, 2, WS_MAX_MESSAGE / 2);
    ws.send_fragmented_text(&under, parts)
        .expect("write the under-cap fragments");
    assert!(
        ws.response_arrives(2),
        "a {}-byte fragmented message is under the {WS_MAX_MESSAGE}-byte cap and must \
         be served",
        under.len()
    );
    assert!(
        daemon_has_node(&d.rpc().dump(), "webs4_under_message"),
        "the under-cap fragmented request must have reached the daemon, or the over-cap \
         assertion below proves nothing about the cap"
    );

    // Six fragments of ~200 KiB: every frame comfortably under the 256 KiB frame cap,
    // the assembled message comfortably over the 1 MiB message cap.
    let over = padded_add_node(
        "webs4_over_message",
        &logs,
        3,
        WS_MAX_MESSAGE + WS_MAX_FRAME,
    );
    assert!(
        over.len().div_ceil(parts) < WS_MAX_FRAME,
        "each fragment must stay under the frame cap, or this tests the wrong cap"
    );
    let _ = ws.send_fragmented_text(&over, parts);
    assert!(
        !ws.response_arrives(3),
        "a {}-byte message was assembled and served: the {WS_MAX_MESSAGE}-byte message \
         cap §17 and docs/security.md promise is not in force",
        over.len()
    );
    // Ordered ahead of the closure assertion for the reason given in the frame-cap test.
    assert!(
        !daemon_has_node(&d.rpc().dump(), "webs4_over_message"),
        "the request inside an over-cap message reached the daemon and mutated the graph"
    );
    assert!(
        ws.closed(),
        "an over-cap message must end the connection, not be silently skipped"
    );
    if let Some(code) = ws.close_code() {
        assert_eq!(
            code, CLOSE_MESSAGE_TOO_BIG,
            "the session ended with close code {code}, not {CLOSE_MESSAGE_TOO_BIG} — \
             something other than the message cap ended it, so this run proves nothing \
             about the cap"
        );
    }
    assert!(
        web.logged(REFUSAL_LOG),
        "the server refused an over-cap message and said nothing about it: an operator \
         watching this daemon cannot tell a tripped security cap from an idle browser \
         disconnect"
    );
}
