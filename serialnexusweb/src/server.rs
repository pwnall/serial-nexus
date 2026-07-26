//! The loopback HTTP + WebSocket server (design §17, §15.29).
//!
//! HTTP request handling is hand-rolled on tokio, matching the daemon's hand-rolled
//! JSON-RPC (§15.13): a handful of routes, no framework. Every request is gated on
//! the per-session bearer token (a cookie after the bootstrap URL), a validated Host
//! header, and — when the client sends one — a validated `Origin`; the WebSocket
//! upgrade is completed by hand (the RFC 6455 §4.2.2 accept digest) and only the
//! post-handshake frame codec comes from tungstenite.
//!
//! The three gates answer three different questions and none substitutes for another:
//! the token is *who may act*, Host is *was this addressed to a name we serve* (DNS
//! rebinding), and Origin is *which page sent it* — the one cookies cannot answer,
//! because they are not port-scoped and `SameSite` is a policy rather than a check.
//! Everything before the token check is reachable by an unauthenticated peer in the
//! sanctioned non-loopback tiers (§15.29), so the pre-auth path is bounded in three
//! dimensions too: a deadline on the head (and the TLS handshake), a cap on the head
//! size, and a cap on in-flight connections.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

/// Immutable per-server configuration shared with every connection handler.
pub struct ServerConfig {
    /// The per-session bearer token every request must carry (§15.29).
    pub token: String,
    /// The daemon control socket this server proxies (§10).
    pub socket: PathBuf,
    /// Host header values accepted (DNS-rebinding defense, §15.29). Validated on
    /// every request, loopback or not: a malicious page rebinding DNS to 127.0.0.1
    /// still sends its own Host, so the check matters even on the loopback default.
    pub hosts: Vec<String>,
}

/// The largest HTTP request head (request line + headers) we will read, so a
/// hostile client cannot grow our buffer without bound — the §10 request-line cap,
/// applied to the browser surface.
pub const MAX_HEAD: usize = 16 * 1024;

/// How long a peer has to deliver a complete request head, and (with TLS) to finish
/// the handshake. Without it, a peer that connects and sends nothing pins a task and
/// an fd forever — reachable by unauthenticated peers in the sanctioned `--tls` and
/// `--insecure-bind` tiers (§15.29), since both bounds are pre-authentication.
const HEAD_TIMEOUT: Duration = Duration::from_secs(15);

/// In-flight connections served at once. A browser tab costs a handful (HTTP/1.1
/// `Connection: close` assets plus one long-lived WebSocket), so this leaves ample
/// headroom for a lab's tabs while bounding the fds and tasks an unauthenticated peer
/// can pin. Over the cap the newest connection is dropped, not queued: a queue would
/// just move the exhaustion.
const MAX_CONNECTIONS: usize = 128;

/// Post-handshake WebSocket frame limits. The browser→server direction carries
/// JSON-RPC requests only — `send` lines and tap control — so a cap far below
/// tungstenite's 64 MiB default costs nothing and bounds what one frame can make us
/// buffer. These limit *incoming* messages; the hostward `tap.data` firehose flows
/// the other way and is untouched.
const WS_MAX_MESSAGE: usize = 1 << 20; // 1 MiB
const WS_MAX_FRAME: usize = 256 * 1024;

/// The RFC 6455 §1.3 GUID appended to `Sec-WebSocket-Key` for the accept digest.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub async fn run(
    addr: SocketAddr,
    config: ServerConfig,
    tls: Option<Arc<rustls::ServerConfig>>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("binding {addr}: {e}"))?;
    // The actual bound address (an ephemeral `:0` request resolves here), so the
    // bootstrap URL is correct even when the OS chose the port.
    let bound = listener.local_addr().unwrap_or(addr);
    let scheme = if tls.is_some() { "https" } else { "http" };
    let shown_host = if bound.ip().is_loopback() {
        "127.0.0.1".to_string()
    } else {
        bound.ip().to_string()
    };
    // The bootstrap URL carries the token once; the browser stores it as a cookie and
    // drops it from the address bar (§15.29). Printed ready to open.
    println!(
        "serial_nexus web console — open:\n  {}://{}:{}/?token={}",
        scheme,
        shown_host,
        bound.port(),
        config.token
    );
    // Whether *we* terminate TLS, which decides the cookie's `Secure` attribute and
    // the default port an Origin is compared against (§15.29 tier 2).
    let secure = tls.is_some();
    let config = Arc::new(config);
    let acceptor = tls.map(TlsAcceptor::from);
    // Bound in-flight connections; the permit is held for the whole connection, WS
    // bridge included, and released on drop however the task ends.
    let conns = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tracing::info!("web console listening on {scheme}://{bound}");
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        let permit = match conns.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(
                    "refusing connection from {peer}: {MAX_CONNECTIONS} connections already in flight"
                );
                drop(stream); // closes immediately; the peer sees a reset, not a hang
                continue;
            }
        };
        let config = config.clone();
        let acceptor = acceptor.clone();
        tokio::task::spawn_local(async move {
            let _permit = permit;
            // TLS-terminate first if configured (§15.29 tier 2), then the plaintext
            // and encrypted paths are identical from here on. The handshake carries
            // the same deadline as the request head: it is equally pre-authentication.
            let result = match acceptor {
                Some(acc) => match timeout(HEAD_TIMEOUT, acc.accept(stream)).await {
                    Ok(Ok(tls_stream)) => handle_conn(tls_stream, config, secure).await,
                    Ok(Err(e)) => {
                        tracing::debug!("TLS handshake from {peer} failed: {e}");
                        Ok(())
                    }
                    Err(_) => {
                        tracing::debug!("TLS handshake from {peer} timed out");
                        Ok(())
                    }
                },
                None => handle_conn(stream, config, secure).await,
            };
            if let Err(e) = result {
                tracing::debug!("connection from {peer} ended: {e}");
            }
        });
    }
}

/// One parsed HTTP request head.
///
/// `pub` with `pub` fields, but this module is private to the crate: the only way in
/// from outside is the deliberate `unstable_fuzz_api` re-export in `lib.rs`, which
/// states its own terms (not semver'd, may vanish).
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The value of `cookie_name` from the `Cookie` header, if present.
    pub fn cookie(&self, cookie_name: &str) -> Option<&str> {
        let cookies = self.header("cookie")?;
        for pair in cookies.split(';') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=')
                && k == cookie_name
            {
                return Some(v);
            }
        }
        None
    }

    /// The value of `key` from the query string, if present (no percent-decoding
    /// beyond what a hex token needs — tokens are `[0-9a-f]`).
    fn query_param(&self, key: &str) -> Option<&str> {
        for pair in self.query.split('&') {
            if let Some((k, v)) = pair.split_once('=')
                && k == key
            {
                return Some(v);
            }
        }
        None
    }

    /// The host portion of the `Host` header (port stripped). Bracketed IPv6
    /// (`[::1]:8080`) keeps its brackets so [`bracketed_eq`] can match it against a
    /// bare `::1`; a `host:port` form drops the port.
    fn host(&self) -> Option<&str> {
        let h = self.header("host")?;
        Some(if h.starts_with('[') {
            match h.split_once(']') {
                Some((inner, _)) => &h[..inner.len() + 1], // include the closing ']'
                None => h,
            }
        } else {
            h.rsplit_once(':').map(|(host, _)| host).unwrap_or(h)
        })
    }
}

async fn handle_conn<S: AsyncRead + AsyncWrite + Unpin + 'static>(
    mut stream: S,
    config: Arc<ServerConfig>,
    secure: bool,
) -> anyhow::Result<()> {
    // A complete head, or the connection is dropped: an unauthenticated peer must not
    // be able to hold a task and an fd by staying silent.
    let req = match timeout(HEAD_TIMEOUT, read_request(&mut stream)).await {
        Ok(read) => match read? {
            Some(req) => req,
            None => return Ok(()), // empty/closed
        },
        Err(_) => {
            return write_simple(
                &mut stream,
                408,
                "Request Timeout",
                "no complete request head within the deadline",
            )
            .await;
        }
    };

    // Host validation first (DNS-rebinding defense, §15.29): a request whose Host is
    // not one we serve is refused before any token or content decision — always,
    // loopback or not.
    let host_ok = req
        .host()
        .map(|h| {
            config
                .hosts
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(h) || bracketed_eq(allowed, h))
        })
        .unwrap_or(false);
    if !host_ok {
        return write_simple(&mut stream, 403, "Forbidden", "unrecognized Host (§15.29)").await;
    }

    // Origin validation next, on every request including the WebSocket upgrade
    // (review SEC-3/WEB-7). Host answers "is this addressed to a name we serve" —
    // DNS rebinding. It cannot answer "which page sent it", because cookies are not
    // port-scoped: a page served from another port on this same host is same-*site*,
    // so `SameSite=Strict` still attaches our session cookie to its requests, and
    // SameSite is a cookie policy, not a check. Origin is the header that carries the
    // port.
    //
    // The judgement call, stated: a *present* Origin must designate this very server;
    // an *absent* one is accepted. Browsers always send Origin on a WebSocket
    // handshake and on cross-origin fetches, so the browser-borne attack this closes
    // cannot omit it, while non-browser clients (`serialnexusweb wsclient`, curl,
    // websocat) never send one — refusing them would break the §17 headless client —
    // and same-origin navigations and script loads legitimately omit it too. This
    // deliberately differs from the Host arm above, which refuses an absent header:
    // Host is mandatory in HTTP/1.1, Origin is not.
    if let Some(origin) = req.header("origin")
        && !origin_matches_host(origin, req.header("host").unwrap_or(""), secure)
    {
        return write_simple(
            &mut stream,
            403,
            "Forbidden",
            "cross-origin request refused (§15.29)",
        )
        .await;
    }

    // The bootstrap URL: `GET /?token=TOKEN`. If it matches, set the session cookie
    // and redirect to `/` so the token leaves the address bar (§15.29). This is the
    // one route the query param (not the cookie) authorizes.
    if req.method == "GET"
        && path_is(&req.path, "/")
        && let Some(tok) = req.query_param("token")
        && ct_eq(tok, &config.token)
    {
        let cookie = session_cookie(&config.token, secure);
        let resp = format!(
            "HTTP/1.1 302 Found\r\nLocation: /\r\nSet-Cookie: {cookie}\r\n\
             Content-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(resp.as_bytes()).await?;
        return Ok(());
    }

    // Every other request carries the token as the session cookie (§15.29). The
    // cookie doubles as CSRF protection via SameSite=Strict.
    let authorized = req
        .cookie("nexus_session")
        .map(|c| ct_eq(c, &config.token))
        .unwrap_or(false);
    if !authorized {
        return write_simple(
            &mut stream,
            401,
            "Unauthorized",
            "missing or invalid session token — open the bootstrap URL (§15.29)",
        )
        .await;
    }

    // Authorized. Route.
    if req.method == "GET" && path_is(&req.path, "/ws") {
        return upgrade_ws(stream, req, config).await;
    }
    if req.method == "GET"
        && let Some(asset) = crate::assets::lookup(&req.path)
    {
        return write_asset(&mut stream, asset).await;
    }
    write_simple(&mut stream, 404, "Not Found", "no such resource").await
}

/// Read the HTTP request head byte-by-byte up to the blank line terminating the
/// headers, so no request-body / WebSocket-frame byte is consumed past the head
/// (critical for the raw-socket WS handoff). Capped at [`MAX_HEAD`].
pub async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> anyhow::Result<Option<Request>> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Ok(None); // clean EOF before a complete request head
        }
        buf.push(byte[0]);
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        if buf.len() > MAX_HEAD {
            anyhow::bail!("request head exceeds {MAX_HEAD} bytes");
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(Some(Request {
        method,
        path,
        query,
        headers,
    }))
}

/// Complete the WebSocket handshake by hand (RFC 6455 §4.2.2), then hand the raw
/// socket to tungstenite for framing only, and bridge it to the daemon (§17).
async fn upgrade_ws<S: AsyncRead + AsyncWrite + Unpin + 'static>(
    mut stream: S,
    req: Request,
    config: Arc<ServerConfig>,
) -> anyhow::Result<()> {
    let key = match req.header("sec-websocket-key") {
        Some(k) => k,
        None => {
            return write_simple(&mut stream, 400, "Bad Request", "not a WebSocket upgrade").await;
        }
    };
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    let accept = nexus_rpc::base64_encode(&hasher.finalize());
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await?;

    // Framing only, with explicit incoming-size caps (review WEB-3): tungstenite's
    // defaults let one browser frame reach 64 MiB before we look at it.
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(WS_MAX_MESSAGE))
        .max_frame_size(Some(WS_MAX_FRAME));
    let ws = WebSocketStream::from_raw_socket(stream, Role::Server, Some(ws_config)).await;
    crate::bridge::bridge(ws, config.socket.clone()).await
}

async fn write_asset<S: AsyncWrite + Unpin>(
    stream: &mut S,
    asset: crate::assets::Asset,
) -> anyhow::Result<()> {
    // A restrictive CSP and `nosniff` on every asset (§17, §15.35). The console
    // renders daemon-supplied strings — node names, refusal messages, port
    // descriptions — and since the token holder can now edit the graph, a future
    // DOM-injection slip would be code execution *as the operator's session* rather
    // than a defaced page. Everything the page needs is same-origin and inline-free:
    // scripts and styles are served from here, the WebSocket is same-origin, and the
    // history export is a `blob:` URL. `frame-ancestors 'none'` keeps the console out
    // of a frame, which `SameSite=Strict` alone does not guarantee.
    const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
                       connect-src 'self' ws: wss:; img-src 'self' data:; \
                       base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Content-Security-Policy: {CSP}\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        asset.content_type,
        asset.body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.write_all(asset.body).await?;
    Ok(())
}

async fn write_simple<S: AsyncWrite + Unpin>(
    stream: &mut S,
    code: u16,
    reason: &str,
    body: &str,
) -> anyhow::Result<()> {
    let resp = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    Ok(())
}

/// The `Set-Cookie` value carrying the session token (§15.29).
///
/// `Secure` is added exactly when this listener terminates TLS. The cookie exists so
/// the token stops riding in URLs (§15.29); without `Secure`, the tier-2 cookie is
/// still attached to any plaintext request to the same host, which is the leak the
/// cookie was introduced to close (review WEB-2). It is *omitted* on the plaintext
/// tiers because browsers refuse to store a `Secure` cookie from a non-trustworthy
/// `http://` origin — setting it unconditionally would break the loopback default.
fn session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!("nexus_session={token}; Path=/; HttpOnly; SameSite=Strict");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Split an authority (`host`, `host:port`, `[::1]`, `[::1]:8080`) into its host and
/// explicit port. `None` means the authority is malformed (an unparsable port).
pub fn split_authority(authority: &str) -> Option<(&str, Option<u16>)> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (inner, tail) = rest.split_once(']')?;
        (inner, tail.strip_prefix(':'))
    } else if authority.matches(':').count() > 1 {
        // An unbracketed IPv6 literal (`::1`) — invalid in a Host header and never
        // produced by a browser, but our own allowed-host list carries that form.
        (authority, None)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        }
    };
    match port {
        None => Some((host, None)),
        Some("") => Some((host, None)),
        Some(p) => Some((host, Some(p.parse().ok()?))),
    }
}

/// Whether `origin` designates this very server, judged against the request's own
/// `Host` — which the caller has already confirmed is a name we serve.
///
/// Comparing against Host rather than against the bound port is deliberate: §17 names
/// SSH forwarding of the loopback default as a supported access mode, and there the
/// browser's port (`localhost:9999`) is not the port we bound. Host and Origin both
/// carry the *browser's* view of the authority, so they still agree under forwarding
/// — while a page served from a different port on the same host, the exact case
/// `SameSite=Strict` cannot see, no longer does. Port-exactness is therefore kept
/// without hard-coding a port.
///
/// The scheme is checked only for plausibility, not for equality: an attacker cannot
/// serve another scheme on the port we are listening on, so authority equality
/// already pins the origin to us, and not demanding scheme equality keeps a
/// TLS-terminating front end from breaking. `null` (a sandboxed iframe, a `file://`
/// page) has no `://` and is refused.
pub fn origin_matches_host(origin: &str, host_header: &str, secure: bool) -> bool {
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    // A serialized origin is scheme + authority, nothing else.
    if authority.contains('/') || authority.contains('?') || authority.contains('#') {
        return false;
    }
    let origin_default = if scheme.eq_ignore_ascii_case("https") {
        443
    } else if scheme.eq_ignore_ascii_case("http") {
        80
    } else {
        return false;
    };
    let Some((o_host, o_port)) = split_authority(authority) else {
        return false;
    };
    let Some((h_host, h_port)) = split_authority(host_header) else {
        return false;
    };
    let our_default = if secure { 443 } else { 80 };
    // `split_authority` has already stripped IPv6 brackets from both sides, so the
    // hosts compare directly.
    o_port.unwrap_or(origin_default) == h_port.unwrap_or(our_default)
        && o_host.eq_ignore_ascii_case(h_host)
}

/// Path comparison ignoring a trailing slash difference on `/`.
fn path_is(path: &str, want: &str) -> bool {
    path == want
}

/// Match a bracketed IPv6 host form (`[::1]`) against a bare one (`::1`) either way.
fn bracketed_eq(a: &str, b: &str) -> bool {
    let strip = |s: &str| s.trim_start_matches('[').trim_end_matches(']').to_string();
    strip(a) == strip(b)
}

/// Constant-time-ish string comparison for the token, so a timing side channel does
/// not leak it byte by byte. Compares full length regardless of the first mismatch.
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 8080-on-127.0.0.1 default, the shape every case below varies from.
    const HOST: &str = "127.0.0.1:8080";

    #[test]
    fn the_session_cookie_is_secure_only_under_tls() {
        // Plaintext tiers: a `Secure` cookie would not be stored by the browser at all
        // from an untrustworthy http origin, so it must stay off (§15.29 tier 1/3).
        let plain = session_cookie("abc", false);
        assert_eq!(
            plain,
            "nexus_session=abc; Path=/; HttpOnly; SameSite=Strict"
        );
        // TLS tier: `Secure` keeps the token off any plaintext same-host request
        // (review WEB-2).
        let tls = session_cookie("abc", true);
        assert!(tls.starts_with("nexus_session=abc; Path=/; HttpOnly; SameSite=Strict"));
        assert!(tls.ends_with("; Secure"), "{tls}");
        // The flags the cookie has always carried survive either way.
        for c in [&plain, &tls] {
            assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict"));
        }
    }

    #[test]
    fn an_origin_on_this_very_authority_is_accepted() {
        assert!(origin_matches_host("http://127.0.0.1:8080", HOST, false));
        assert!(origin_matches_host("https://127.0.0.1:8080", HOST, true));
        // Case-insensitive host, per RFC 3986.
        assert!(origin_matches_host(
            "http://LocalHost:8080",
            "localhost:8080",
            false
        ));
        // Bracketed IPv6 on both sides.
        assert!(origin_matches_host(
            "http://[::1]:8080",
            "[::1]:8080",
            false
        ));
    }

    #[test]
    fn a_sibling_port_on_the_same_host_is_refused() {
        // The whole point (review SEC-3/WEB-7): cookies are not port-scoped, so this
        // page's fetch/WebSocket carries our session cookie and `SameSite=Strict`
        // never sees it. Origin does.
        assert!(!origin_matches_host("http://127.0.0.1:9999", HOST, false));
        assert!(!origin_matches_host(
            "http://localhost:9999",
            "localhost:8080",
            false
        ));
        assert!(!origin_matches_host(
            "http://[::1]:9999",
            "[::1]:8080",
            false
        ));
        // A different host entirely, and the opaque `null` origin of a sandboxed
        // iframe or a file:// page.
        assert!(!origin_matches_host("http://evil.example", HOST, false));
        assert!(!origin_matches_host("null", HOST, false));
        assert!(!origin_matches_host("", HOST, false));
        // Schemes we could never have served the page under.
        assert!(!origin_matches_host("file://127.0.0.1:8080", HOST, false));
        assert!(!origin_matches_host("chrome-extension://abc", HOST, false));
        // A serialized origin has no path; anything with one is not one.
        assert!(!origin_matches_host(
            "http://127.0.0.1:8080/evil",
            HOST,
            false
        ));
    }

    #[test]
    fn default_ports_are_resolved_before_comparing() {
        // `http://localhost` is port 80, and a Host with no port means our scheme's
        // default — so the two agree only when both resolve to the same number.
        assert!(origin_matches_host("http://localhost", "localhost", false));
        assert!(origin_matches_host(
            "http://localhost:80",
            "localhost",
            false
        ));
        assert!(origin_matches_host("https://localhost", "localhost", true));
        assert!(origin_matches_host(
            "https://localhost:443",
            "localhost",
            true
        ));
        assert!(!origin_matches_host("http://localhost", "localhost", true));
        assert!(!origin_matches_host(
            "http://localhost:8080",
            "localhost",
            false
        ));
    }

    #[test]
    fn ssh_forwarding_keeps_working() {
        // §17 names SSH forwarding of the loopback default as a supported access mode.
        // The browser then sees a port we never bound — and Host and Origin agree on
        // it, which is exactly why the comparison is against Host, not the bound port.
        assert!(origin_matches_host(
            "http://localhost:9999",
            "localhost:9999",
            false
        ));
    }

    #[test]
    fn authorities_split_into_host_and_port() {
        assert_eq!(split_authority("localhost"), Some(("localhost", None)));
        assert_eq!(
            split_authority("localhost:8080"),
            Some(("localhost", Some(8080)))
        );
        assert_eq!(split_authority("[::1]"), Some(("::1", None)));
        assert_eq!(split_authority("[::1]:8080"), Some(("::1", Some(8080))));
        // A raw IPv6 literal (what our own allowed-host list carries) is not a
        // host:port split.
        assert_eq!(split_authority("::1"), Some(("::1", None)));
        // A port that is not a number is a malformed authority, refused rather than
        // silently treated as "no port".
        assert_eq!(split_authority("localhost:http"), None);
        assert_eq!(split_authority("localhost:99999"), None);
    }

    #[tokio::test]
    async fn a_request_head_parses_and_is_bounded() {
        let raw = b"GET /app.js?token=ab HTTP/1.1\r\nHost: 127.0.0.1:8080\r\n\
                    Origin: http://127.0.0.1:8080\r\nCookie: a=1; nexus_session=tok\r\n\r\n";
        let req = read_request(&mut &raw[..])
            .await
            .expect("parse")
            .expect("a head");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/app.js");
        assert_eq!(req.query_param("token"), Some("ab"));
        assert_eq!(req.cookie("nexus_session"), Some("tok"));
        assert_eq!(req.header("origin"), Some("http://127.0.0.1:8080"));
        assert_eq!(req.host(), Some("127.0.0.1"), "the port is stripped");

        // A head that never terminates is refused at MAX_HEAD, not buffered forever.
        let flood = vec![b'x'; MAX_HEAD + 64];
        assert!(read_request(&mut &flood[..]).await.is_err());
    }

    #[tokio::test]
    async fn a_silent_peer_hits_the_head_deadline() {
        // A peer that connects and sends nothing must not pin a task and an fd
        // forever (review WEB-3). The deadline is the same wrapper `handle_conn`
        // applies; a short one here keeps the test instant.
        let (mut ours, _theirs) = tokio::io::duplex(64);
        let elapsed = timeout(Duration::from_millis(50), read_request(&mut ours)).await;
        assert!(elapsed.is_err(), "a silent peer must time out, not hang");
    }
}
