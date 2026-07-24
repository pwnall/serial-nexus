//! The loopback HTTP + WebSocket server (design §17, §15.29).
//!
//! HTTP request handling is hand-rolled on tokio, matching the daemon's hand-rolled
//! JSON-RPC (§15.13): a handful of routes, no framework. Every request is gated on
//! the per-session bearer token (a cookie after the bootstrap URL) and a validated
//! Host header; the WebSocket upgrade is completed by hand (the RFC 6455 §4.2.2
//! accept digest) and only the post-handshake frame codec comes from tungstenite.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Role;

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
const MAX_HEAD: usize = 16 * 1024;

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
    let config = Arc::new(config);
    let acceptor = tls.map(TlsAcceptor::from);
    tracing::info!("web console listening on {scheme}://{bound}");
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        let config = config.clone();
        let acceptor = acceptor.clone();
        tokio::task::spawn_local(async move {
            // TLS-terminate first if configured (§15.29 tier 2), then the plaintext
            // and encrypted paths are identical from here on.
            let result = match acceptor {
                Some(acc) => match acc.accept(stream).await {
                    Ok(tls_stream) => handle_conn(tls_stream, config).await,
                    Err(e) => {
                        tracing::debug!("TLS handshake from {peer} failed: {e}");
                        Ok(())
                    }
                },
                None => handle_conn(stream, config).await,
            };
            if let Err(e) = result {
                tracing::debug!("connection from {peer} ended: {e}");
            }
        });
    }
}

/// One parsed HTTP request head.
struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The value of `cookie_name` from the `Cookie` header, if present.
    fn cookie(&self, cookie_name: &str) -> Option<&str> {
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
) -> anyhow::Result<()> {
    let req = match read_request(&mut stream).await? {
        Some(req) => req,
        None => return Ok(()), // empty/closed
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

    // The bootstrap URL: `GET /?token=TOKEN`. If it matches, set the session cookie
    // and redirect to `/` so the token leaves the address bar (§15.29). This is the
    // one route the query param (not the cookie) authorizes.
    if req.method == "GET"
        && path_is(&req.path, "/")
        && let Some(tok) = req.query_param("token")
        && ct_eq(tok, &config.token)
    {
        let cookie = format!(
            "nexus_session={}; Path=/; HttpOnly; SameSite=Strict",
            config.token
        );
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
async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> anyhow::Result<Option<Request>> {
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

    let ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
    crate::bridge::bridge(ws, config.socket.clone()).await
}

async fn write_asset<S: AsyncWrite + Unpin>(
    stream: &mut S,
    asset: crate::assets::Asset,
) -> anyhow::Result<()> {
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
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
