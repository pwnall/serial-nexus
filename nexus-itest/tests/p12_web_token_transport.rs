//! Phase 12 — **how the web console's credentials travel** (review
//! `docs/32-claude-opus-code-review.md`).
//!
//! One file per defect area, following the `p9_*`/`p10_*`/`p11_*` convention (§5).
//! This one pins **WEBS-1**: the per-session bearer token was delivered as a
//! `Path=/` cookie, and a cookie's jar key is (name, domain, path) — never the port
//! (RFC 6265 §8.5). The browser therefore replayed the token to *every* path of
//! *every* service on any port of the same host that the operator contacted, and the
//! token is shell-equivalent: whoever opens `/ws` reaches a bridge that can
//! `add-node` an exec codec running an arbitrary command line as the daemon's user
//! (`docs/security.md`). The finder's reproduction was an ordinary navigation to a
//! rogue sibling-port server, whose access log then held `nexus_session=<token>`.
//! The design's stated mitigation — the `Origin` check — guards only the *inbound*
//! direction and says nothing about what the browser *sends elsewhere*.
//!
//! What changed: the browser now holds two cookies rather than one. The token is
//! scoped to `Path=/ws`, the single path a browser needs it on, and a freshly minted
//! **asset credential** (`nexus_assets_<port>`, `Path=/`) carries the navigations and
//! subresource loads — which can present nothing else, since no browser attaches a
//! script-held credential to a `<script src>`. So the value a sibling-port service
//! harvests from a plain navigation opens this console's own JavaScript and is
//! refused at `/ws`.
//!
//! The residual is asserted here too, deliberately, rather than left to be
//! rediscovered: a page *served from* a sibling port is same-*site*, so `Path=/ws` is
//! matched by a request it makes to its own `/ws`, and `SameSite=Strict` never fires
//! between two ports of one host. Path scoping narrows the auto-replay from every
//! request to one path; it does not end it. Ending it would mean taking the token out
//! of cookies entirely, which the asset loads cannot survive.
//!
//! No serial device and no browser are involved, so this runs on **every** platform
//! (§5). The raw HTTP/RFC 6455 clients are local copies for the same reason
//! `p12_web_session.rs` keeps its own: what each asserts needs a different slice of
//! the response.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nexus_itest::{Daemon, bin, wait_until};

/// The fixed per-session token, so "this exact string leaked" is a byte-level fact.
const TOKEN: &str = "webs1token0123456789abcdef";

// ---------------------------------------------------------------- child process ----

/// A `serialnexusweb` child whose printed bootstrap URL is scanned for the OS-chosen
/// port. Killed on drop.
struct WebServer {
    child: Child,
    port: u16,
}

impl WebServer {
    fn spawn(token: &str, socket: &Path, xdg: &Path) -> WebServer {
        let socket_str = socket.to_string_lossy().into_owned();
        let mut child = Command::new(bin("serialnexusweb"))
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
            .expect("spawn serialnexusweb");
        // Drained for the child's whole life: the tracing subscriber writes to stdout
        // too, and a pipe whose read end we dropped would take the server down on its
        // next log line.
        let stdout = child.stdout.take().expect("piped serialnexusweb stdout");
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
            "serialnexusweb never printed its bound http URL; saw {:?}",
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

// ------------------------------------------------------------------ HTTP helpers ----

/// Read one complete HTTP response head from `stream`.
fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => buf.push(byte[0]),
            Err(_) => return None,
        }
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
    }
}

/// `GET <target>` with the given headers, returning `(status, response headers)`.
fn http_head(port: u16, target: &str, headers: &[(&str, &str)]) -> (u16, Vec<(String, String)>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    let mut req =
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).expect("write request");
    stream.flush().expect("flush request");
    let text =
        read_head(&mut stream).unwrap_or_else(|| panic!("no response head for GET {target}"));
    let mut lines = text.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no HTTP status in:\n{text}"));
    let headers = lines
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    (status, headers)
}

/// The status of `GET <target>` with the given `Cookie` header (none when empty).
fn status(port: u16, target: &str, cookie: &str) -> u16 {
    let headers: Vec<(&str, &str)> = if cookie.is_empty() {
        Vec::new()
    } else {
        vec![("Cookie", cookie)]
    };
    http_head(port, target, &headers).0
}

/// The status of a real RFC 6455 upgrade to `/ws` with the given `Cookie` header:
/// `101` when it is accepted, whatever was written instead when it is not.
fn ws_status(port: u16, cookie: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect web server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    // RFC 6455 §1.3's own example nonce: the server only hashes it into the accept
    // digest, so a fixed one keeps this deterministic.
    let mut req = format!(
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n"
    );
    if !cookie.is_empty() {
        req.push_str(&format!("Cookie: {cookie}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).expect("write WS upgrade");
    stream.flush().expect("flush WS upgrade");
    let text = read_head(&mut stream).expect("no response head from the WS upgrade");
    text.split("\r\n")
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no HTTP status in the upgrade response:\n{text}"))
}

// ------------------------------------------------------------ a minimal cookie jar --

/// One `Set-Cookie`, parsed the way a browser stores it.
struct StoredCookie {
    name: String,
    value: String,
    path: String,
    attributes: String,
}

impl StoredCookie {
    /// What the browser replays for this cookie: `name=value`, attributes stripped.
    fn pair(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// Parse a `Set-Cookie` value. The default path is irrelevant here — this server
/// always states one — so an absent `Path` is reported as such and asserted on.
fn parse_set_cookie(raw: &str) -> StoredCookie {
    let mut parts = raw.split(';');
    let (name, value) = parts
        .next()
        .and_then(|p| p.trim().split_once('='))
        .unwrap_or_else(|| panic!("a Set-Cookie has a name=value pair: {raw:?}"));
    let attributes = raw.split_once(';').map(|(_, a)| a.trim()).unwrap_or("");
    let path = attributes
        .split(';')
        .map(str::trim)
        .find_map(|a| {
            let (k, v) = a.split_once('=')?;
            k.eq_ignore_ascii_case("path").then(|| v.to_string())
        })
        .unwrap_or_else(|| panic!("this server always states a Path: {raw:?}"));
    StoredCookie {
        name: name.to_string(),
        value: value.to_string(),
        path,
        attributes: attributes.to_string(),
    }
}

/// RFC 6265 §5.1.4 path-match — the rule that decides, in the browser, whether a
/// stored cookie is attached to a request. Reimplemented here rather than inferred,
/// because the whole finding is about which requests carry the token.
fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    if cookie_path == request_path {
        return true;
    }
    if !request_path.starts_with(cookie_path) {
        return false;
    }
    cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/')
}

/// Open the bootstrap URL and return every cookie the server set, in header order.
fn bootstrap(port: u16) -> Vec<StoredCookie> {
    let (code, headers) = http_head(port, &format!("/?token={TOKEN}"), &[]);
    assert_eq!(code, 302, "the bootstrap URL redirects (§15.29)");
    headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| parse_set_cookie(v))
        .collect()
}

/// What a browser holding `jar` sends to **any** service on this host — ours or a
/// sibling port's — for a request to `request_path`. Ports are not part of a cookie's
/// jar key (RFC 6265 §8.5), so this function needs no port argument, which is the
/// finding in one sentence.
fn replayed_to(jar: &[StoredCookie], request_path: &str) -> Vec<String> {
    jar.iter()
        .filter(|c| path_matches(&c.path, request_path))
        .map(StoredCookie::pair)
        .collect()
}

// ------------------------------------------------------------------------ WEBS-1 ----

/// Review **WEBS-1**: an ordinary navigation to a sibling-port service must not hand
/// it the shell-equivalent token.
#[test]
fn the_session_token_is_not_replayed_to_paths_outside_the_websocket_upgrade() {
    let d = Daemon::start();
    let web = WebServer::spawn(TOKEN, &d.socket(), d.run().path());
    let jar = bootstrap(web.port);

    // Two cookies, and the session cookie first: `p8_web.rs` and `p12_web_session.rs`
    // both read *the* `Set-Cookie`, and the one that authorizes everything is the one
    // a single-value reader must see.
    assert_eq!(
        jar.len(),
        2,
        "the bootstrap sets the session cookie and the asset cookie; saw {:?}",
        jar.iter().map(StoredCookie::pair).collect::<Vec<_>>()
    );
    let session = &jar[0];
    let asset = &jar[1];
    assert_eq!(session.name, format!("nexus_session_{}", web.port));
    assert_eq!(session.value, TOKEN);
    assert_eq!(asset.name, format!("nexus_assets_{}", web.port));
    assert_ne!(
        asset.value, TOKEN,
        "the everywhere-replayed cookie must not carry the token"
    );
    assert!(
        !asset.value.is_empty() && asset.value.chars().all(|c| c.is_ascii_hexdigit()),
        "the asset credential is hex from the OS CSPRNG: {:?}",
        asset.value
    );

    // Neither cookie gives anything up that it used to carry: the split is about how
    // far a value travels, not about relaxing the gate (`SameSite=Strict` is what
    // `docs/security.md` calls the CSRF mechanism, and it stays on both).
    for c in [session, asset] {
        assert!(
            c.attributes.contains("HttpOnly") && c.attributes.contains("SameSite=Strict"),
            "{}: {:?}",
            c.name,
            c.attributes
        );
    }

    // The finding's own reproduction, replayed as arithmetic: the operator navigates
    // to a rogue server on a sibling port. Cookies are not port-scoped, so it receives
    // exactly what our own server would for the same path — and that no longer
    // includes the token, for any path a browsing user reaches by accident.
    for navigated in ["/", "/index.html", "/app.js", "/favicon.ico", "/wsx", "/w"] {
        let sent = replayed_to(&jar, navigated);
        assert!(
            !sent.iter().any(|c| c.contains(TOKEN)),
            "a navigation to {navigated} on a sibling port must not carry the token; sent {sent:?}"
        );
        assert_eq!(
            sent,
            vec![asset.pair()],
            "the asset credential is what such a request carries, and all it carries"
        );
    }

    // …and the residual, stated as a fact rather than left to be rediscovered: a page
    // served from a sibling port is same-*site*, so `SameSite=Strict` never fires, and
    // a request it makes to its own `/ws` still matches the token cookie's path. Path
    // scoping narrows the auto-replay to one path; only taking the token out of
    // cookies would end it, and the asset loads cannot present anything but a cookie.
    let upgrade = replayed_to(&jar, "/ws");
    assert!(
        upgrade.iter().any(|c| c.contains(TOKEN)),
        "the browser must still carry the token on the upgrade, or the console cannot \
         connect at all; sent {upgrade:?}"
    );

    // Which is exactly why the scope and the route are one constant on the server: a
    // cookie scoped anywhere else is a console that renders and never connects.
    assert!(path_matches(&session.path, "/ws"));
    assert_eq!(
        ws_status(web.port, &session.pair()),
        101,
        "the path-scoped session cookie must still open the WebSocket"
    );
}

/// Review **WEBS-1**, the other half: what leaks must not be enough to act. The asset
/// credential opens the static assets and is refused at `/ws`, where every capability
/// (`send`, the graph-editing verbs, an exec codec) lives.
#[test]
fn the_asset_credential_opens_the_assets_and_never_the_websocket() {
    let d = Daemon::start();
    let web = WebServer::spawn(TOKEN, &d.socket(), d.run().path());
    let jar = bootstrap(web.port);
    let session = jar[0].pair();
    let asset = jar[1].pair();

    // The credential a sibling port harvests: assets, yes — capability, no.
    for target in ["/", "/app.js", "/app.css"] {
        assert_eq!(
            status(web.port, target, &asset),
            200,
            "the asset credential must serve {target}, or the console cannot boot"
        );
    }
    assert_eq!(
        ws_status(web.port, &asset),
        401,
        "the asset credential must not open the WebSocket — the bridge behind it can \
         add an exec codec (docs/security.md)"
    );

    // The token still authorizes everything, under either name: a headless client
    // (`serialnexusweb wsclient`, curl) presents `nexus_session=<token>` by hand and
    // knows nothing about paths or the bound port (§17).
    let unscoped = format!("nexus_session={TOKEN}");
    for cookie in [&session, &unscoped] {
        assert_eq!(status(web.port, "/app.js", cookie), 200, "{cookie}");
        assert_eq!(ws_status(web.port, cookie), 101, "{cookie}");
    }

    // And neither route is open to a peer holding nothing, or holding a guess.
    let forged = format!("nexus_assets_{}=00000000000000000000000000000000", web.port);
    for cookie in ["", forged.as_str()] {
        assert_eq!(status(web.port, "/app.js", cookie), 401, "{cookie:?}");
        assert_eq!(ws_status(web.port, cookie), 401, "{cookie:?}");
    }
}

/// The path-match rule this file leans on, proven against RFC 6265 §5.1.4 rather than
/// assumed — a wrong implementation here would make the assertions above vacuous.
#[test]
fn the_path_match_rule_is_the_rfc_6265_one() {
    assert!(path_matches("/ws", "/ws"));
    assert!(path_matches("/ws", "/ws/echo"));
    assert!(path_matches("/", "/"));
    assert!(path_matches("/", "/anything/at/all"));
    // A prefix that does not end at a segment boundary is not a match — which is what
    // keeps `/ws` from covering `/wsx`, the shape a careless `starts_with` would leak.
    assert!(!path_matches("/ws", "/wsx"));
    assert!(!path_matches("/ws", "/"));
    assert!(!path_matches("/ws", "/app.js"));
}
