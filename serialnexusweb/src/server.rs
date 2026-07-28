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
//!
//! **The credential the browser holds is split in two** (review WEBS-1). A cookie's jar
//! key is (name, domain, path) and never the port, so a value stored under `Path=/` is
//! replayed to every path of every service on this host that the operator's browser
//! contacts — and the session token is shell-equivalent, because whoever opens `/ws`
//! can add an exec codec (`docs/security.md`). So the token cookie is scoped
//! `Path=/ws`, the one path a browser needs it on, and a second, freshly minted
//! *asset credential* (`nexus_assets_<port>`, `Path=/`) carries the static assets.
//! What a sibling-port service harvests from an ordinary navigation now authorizes
//! reading this console's own JavaScript and nothing else. The residual is stated
//! rather than implied: a page *served from* a sibling port is same-*site*, so it can
//! still fetch its own `/ws` with credentials and log what the browser attaches. Path
//! scoping narrows the auto-replay from every request to one path; ending it entirely
//! would mean taking the token out of cookies altogether, which no browser can do for
//! the navigations and subresource loads that must also be gated.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, oneshot};
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
///
/// Five seconds, not the fifteen it was (review WEB-5): a complete request head is one
/// round trip on a link the operator already reached, so the only thing the extra ten
/// seconds bought was a longer hold time for a peer that sends one byte and waits. It is
/// how long a peer that is *not* evicted may sit in the pre-auth population, so it is
/// also the ceiling on how long a stalled peer keeps a slot the operator might want. The
/// slack that remains is for the TLS handshake, which shares this deadline and costs a
/// couple of round trips.
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Connections **past the token gate** served at once. A browser tab costs a handful
/// (HTTP/1.1 `Connection: close` assets plus one long-lived WebSocket), so this leaves
/// ample headroom for a lab's tabs while bounding the fds and tasks one credential can
/// pin. Over the cap a request is answered `503`, not queued: a queue would just move
/// the exhaustion.
///
/// The permit is taken **at the token gate and not at accept** (review WEB-5): a peer
/// that has shown no credential must not be able to occupy a slot the operator needs,
/// and the only moment we can tell the two apart is after the head is read. What bounds
/// the population *before* that moment is [`MAX_PRE_AUTH_CONNECTIONS`], and it bounds it
/// by eviction rather than by refusal.
const MAX_CONNECTIONS: usize = 128;

/// How many connections may sit *before* the token gate at once (review WEB-5).
///
/// **This cap is enforced by eviction, never by refusing an accept**, and that
/// distinction is the whole finding. The first attempt at the reserve was a second
/// semaphore acquired in the accept loop, which made the denial four times cheaper
/// instead of closing it: the cookie cannot be known before the head is read, so a full
/// pre-auth pool refused *every* new connection — including the operator's, carrying a
/// valid session cookie. Measured against the shipped binary, 32 silent sockets reset
/// every subsequent connection on `/app.js` and `/ws` alike, where the single pool had
/// cost 128.
///
/// So: every accepted connection joins the pre-auth population immediately and
/// unconditionally; when that pushes the population over this cap, the **oldest** member
/// is evicted — its task cancelled, its socket closed — and the newcomer takes its place.
/// A silent peer can therefore no longer deny anyone; it can only be the thing that gets
/// evicted. The operator's browser is always the newest arrival, which is the last
/// candidate for eviction, and it leaves the population entirely the moment its cookie is
/// read a round trip later. To make the console so much as retry, a flood would have to
/// turn this whole pool over inside that round trip — tens of thousands of connections
/// per second on loopback, against 6.4/s for the semaphore it replaces.
///
/// The residual, stated: an evicted connection's socket closes when its task is next
/// polled, so a burst can leave evicted-but-unreaped tasks briefly alive. tokio's
/// cooperative budget forces the accept loop to yield, which bounds that backlog at the
/// same order as the old total cap, and every one of those tasks is on its way out rather
/// than holding a slot for [`HEAD_TIMEOUT`].
///
/// A per-peer-IP cap was considered and deliberately not added: on the loopback
/// default every local user — the adversary §15.28 names — shares 127.0.0.1 with the
/// operator, so a per-IP bound would ration the operator's own browser without
/// separating it from the attacker.
const MAX_PRE_AUTH_CONNECTIONS: usize = 32;

/// The connections that have not yet shown a credential.
///
/// Registration never fails — see [`MAX_PRE_AUTH_CONNECTIONS`]. `live` is in arrival
/// order, so its front is the connection that has had the longest to deliver a request
/// head and has therefore earned its eviction; a ticket deregisters by id, never by
/// position, because the pool is reordered under it by every eviction.
struct PreAuthPool {
    inner: Mutex<PreAuthInner>,
}

#[derive(Default)]
struct PreAuthInner {
    next_id: u64,
    live: VecDeque<(u64, oneshot::Sender<()>)>,
}

impl PreAuthPool {
    fn new() -> Arc<PreAuthPool> {
        Arc::new(PreAuthPool {
            inner: Mutex::new(PreAuthInner::default()),
        })
    }

    /// Register a freshly accepted connection, evicting the oldest members until the
    /// population fits. Returns the connection's membership ticket and the signal that
    /// fires if *this* connection is later the one evicted.
    ///
    /// Called synchronously from the accept loop, so the population is back within its
    /// cap before the next `accept()` — no burst can register more than the cap at once.
    fn admit(self: &Arc<Self>) -> (PreAuthTicket, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let id = inner.next_id;
        inner.next_id = inner.next_id.wrapping_add(1);
        inner.live.push_back((id, tx));
        while inner.live.len() > MAX_PRE_AUTH_CONNECTIONS {
            if let Some((_, victim)) = inner.live.pop_front() {
                // `Err` only means the victim's task already ended — nothing to cancel.
                let _ = victim.send(());
            }
        }
        drop(inner);
        (
            PreAuthTicket {
                pool: self.clone(),
                id,
            },
            rx,
        )
    }

    /// Remove `id` from the population, if it is still in it (an evicted connection was
    /// removed at eviction time and this is then a no-op).
    fn leave(&self, id: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.live.retain(|(other, _)| *other != id);
    }

    #[cfg(test)]
    fn live(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .live
            .len()
    }
}

/// A connection's membership in the pre-authentication population. Dropping it — at the
/// token gate, or when the connection ends for any other reason — leaves the pool, which
/// both frees the slot and makes the connection un-evictable.
struct PreAuthTicket {
    pool: Arc<PreAuthPool>,
    id: u64,
}

impl Drop for PreAuthTicket {
    fn drop(&mut self) {
        self.pool.leave(self.id);
    }
}

/// Resolves when this connection is **evicted**, and never otherwise.
///
/// A dropped sender — the ticket leaving the pool at the token gate, or the connection
/// simply ending — parks forever instead of resolving. That asymmetry is load-bearing:
/// the caller races this future against the connection itself, so one that fired on
/// ticket drop would cancel the session one instruction after it authenticated.
async fn evicted(signal: oneshot::Receiver<()>) {
    if signal.await.is_err() {
        std::future::pending::<()>().await
    }
}

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
    // The cookie name is the listener's, not a fixed one (review WEB-3): cookies are
    // keyed by (name, domain, path) and never by port, so a fixed name makes two
    // consoles on one host evict each other's session.
    let port = bound.port();
    // The asset-only credential the browser carries under `Path=/` (review WEBS-1).
    // Minted fresh per run from the OS CSPRNG and unrelated to the token by
    // construction — *not* derived from it, so nothing about a hash has to be trusted
    // to keep the token unrecoverable from the value that does get replayed everywhere.
    let assets: Arc<str> = Arc::from(new_asset_credential());
    let config = Arc::new(config);
    let acceptor = tls.map(TlsAcceptor::from);
    // Bound the connections that have shown a credential; the permit is taken at the
    // token gate, held for the rest of the connection (WS bridge included), and released
    // on drop however the task ends.
    let conns = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    // …and, separately, the population that has *not* shown one yet, bounded by evicting
    // its oldest member rather than by refusing the newest arrival (review WEB-5).
    let pre_auth = PreAuthPool::new();
    tracing::info!("web console listening on {scheme}://{bound}");
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                continue;
            }
        };
        // Join the pre-auth population here, synchronously, before the next accept:
        // registration cannot fail — which is exactly how an authenticated client stops
        // being deniable by silent peers — and the eviction it may trigger is what keeps
        // the population bounded.
        let (ticket, evict) = pre_auth.admit();
        let config = config.clone();
        let assets = assets.clone();
        let acceptor = acceptor.clone();
        let conns = conns.clone();
        tokio::task::spawn_local(async move {
            // TLS-terminate first if configured (§15.29 tier 2), then the plaintext
            // and encrypted paths are identical from here on. The handshake carries
            // the same deadline as the request head: it is equally pre-authentication,
            // and it runs under the pre-auth ticket for the same reason.
            let served = async move {
                match acceptor {
                    Some(acc) => match timeout(HEAD_TIMEOUT, acc.accept(stream)).await {
                        Ok(Ok(tls_stream)) => {
                            handle_conn(tls_stream, config, assets, secure, port, ticket, conns)
                                .await
                        }
                        Ok(Err(e)) => {
                            tracing::debug!("TLS handshake from {peer} failed: {e}");
                            Ok(())
                        }
                        Err(_) => {
                            tracing::debug!("TLS handshake from {peer} timed out");
                            Ok(())
                        }
                    },
                    None => handle_conn(stream, config, assets, secure, port, ticket, conns).await,
                }
            };
            // Cancelling `served` drops the stream, which is the eviction. Past the token
            // gate `evicted` parks forever (the ticket is gone), so an authenticated
            // session — a WebSocket bridge above all — can never be taken down this way.
            let result = tokio::select! {
                biased;
                () = evicted(evict) => {
                    tracing::debug!(
                        "evicting {peer}: more than {MAX_PRE_AUTH_CONNECTIONS} connections \
                         are waiting at the token gate"
                    );
                    Ok(())
                }
                r = served => r,
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

    /// **Every** value carried under `cookie_name` in the `Cookie` header, in the
    /// order the header lists them.
    ///
    /// Returning all of them rather than the first is the point (review WEB-3). A
    /// `Cookie` header is a flat list with no path or domain attached, so a page on
    /// another port of this same host — same *site*, which is why `SameSite=Strict`
    /// never fires — can plant `nexus_session…=x; path=/ws`, and RFC 6265 §5.4 orders
    /// the longer path first. A first-value-wins reader then sees only the plant: the
    /// assets (path `/`) still load, so the console renders normally and simply never
    /// connects, and a reload does not clear it. Offering every value turns that from
    /// a denial into a no-op — the real cookie is still in the header, and the caller
    /// accepts the request if *any* value matches.
    pub fn cookies<'a>(&'a self, cookie_name: &'a str) -> impl Iterator<Item = &'a str> {
        self.header("cookie")
            .into_iter()
            .flat_map(|cookies| cookies.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .filter(move |(k, _)| *k == cookie_name)
            .map(|(_, v)| v)
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
    assets: Arc<str>,
    secure: bool,
    port: u16,
    pre_auth: PreAuthTicket,
    conns: Arc<Semaphore>,
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

    // The bootstrap URL: `GET /?token=TOKEN`. If it matches, set the two session
    // cookies and redirect to `/` so the token leaves the address bar (§15.29). This
    // is the one route the query param (not a cookie) authorizes.
    //
    // The token cookie is written first, because it is *the* session cookie: a client
    // reading one `Set-Cookie` is reading the one that authorizes everything.
    if req.method == "GET"
        && req.path == "/"
        && let Some(tok) = req.query_param("token")
        && ct_eq(tok, &config.token)
    {
        let session = session_cookie(&config.token, secure, port);
        let asset = asset_cookie(&assets, secure, port);
        let resp = format!(
            "HTTP/1.1 302 Found\r\nLocation: /\r\nSet-Cookie: {session}\r\n\
             Set-Cookie: {asset}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(resp.as_bytes()).await?;
        return Ok(());
    }

    // Every other request carries a credential as a cookie (§15.29), and *which*
    // credential suffices depends on the route (review WEBS-1). Both cookies keep
    // `SameSite=Strict`, which is what the CSRF story rests on; the split is about how
    // far each value travels, not about how a request is judged once it arrives.
    //
    // Two names are accepted for the token, and every value under each is checked
    // (review WEB-3). The listener-scoped name is the one *we* set, so it is the one a
    // browser replays; the unscoped base name is for a headless client (`serialnexusweb
    // wsclient`, curl, websocat), which is handed a token rather than a cookie jar and
    // has no way to know the bound port — under the SSH forwarding §17 sanctions it is
    // not even the port in the client's own URL. Reading a name we never set costs
    // nothing: the value still has to equal the token, and no one can plant a cookie
    // that authorizes them. Neither name is path-scoped when a client sets it by hand,
    // which is why `Path=/ws` on our own `Set-Cookie` costs the headless client nothing.
    let cookie_name = session_cookie_name(port);
    let token_ok = req
        .cookies(&cookie_name)
        .chain(req.cookies(SESSION_COOKIE))
        .any(|c| ct_eq(c, &config.token));
    // The asset credential opens the static assets and nothing else. `/ws` is the whole
    // capability — the bridge behind it commands every console and can add an exec
    // codec — so a credential that survives being replayed to a sibling port must not
    // reach it.
    let asset_ok = req
        .cookies(&asset_cookie_name(port))
        .any(|c| ct_eq(c, &assets));
    let ws_route = req.path == WS_PATH;
    if !token_ok && !(asset_ok && !ws_route) {
        let why = if asset_ok {
            "the console cookie does not authorize the WebSocket — open the bootstrap URL (§15.29)"
        } else {
            "missing or invalid session token — open the bootstrap URL (§15.29)"
        };
        return write_simple(&mut stream, 401, "Unauthorized", why).await;
    }
    // Past the token gate, so this connection changes populations (review WEB-5): it
    // leaves the pre-auth pool — freeing that slot and, just as importantly, becoming
    // un-evictable — and only now takes a permit from the pool that nothing without a
    // credential can reach. In that order, so an authenticated client never competes
    // with the noise below it for either resource.
    drop(pre_auth);
    let _served = match conns.try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(
                "refusing an authenticated request: {MAX_CONNECTIONS} connections already in flight"
            );
            return write_simple(
                &mut stream,
                503,
                "Service Unavailable",
                "too many connections in flight — close a console tab and retry",
            )
            .await;
        }
    };

    // Authorized. Route.
    if req.method == "GET" && ws_route {
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

/// A restrictive CSP, sent with `nosniff` on every asset (§17, §15.35). The console
/// renders daemon-supplied strings — node names, refusal messages, port descriptions —
/// and since the token holder can now edit the graph, a future DOM-injection slip
/// would be code execution *as the operator's session* rather than a defaced page.
/// Everything the page needs is same-origin and inline-free: scripts and styles are
/// served from here, the WebSocket is same-origin, and the history export is a `blob:`
/// URL. `frame-ancestors 'none'` keeps the console out of a frame, which
/// `SameSite=Strict` alone does not guarantee.
///
/// `connect-src` is `'self'` alone (review WEBS-2). It used to read `'self' ws: wss:`,
/// and the bare scheme sources let a script open a WebSocket to *any* host — the exact
/// silent, bidirectional, page-preserving exfiltration channel the paragraph above
/// claims is closed. CSP3 makes `'self'` match a same-origin `ws:`/`wss:` connection,
/// verified against this project's own pinned Chromium on both deployment tiers
/// (`ws://` from an `http://` page, `wss://` from an `https://` page), so dropping the
/// schemes costs the console nothing.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
                   connect-src 'self'; img-src 'self' data:; \
                   base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

async fn write_asset<S: AsyncWrite + Unpin>(
    stream: &mut S,
    asset: crate::assets::Asset,
) -> anyhow::Result<()> {
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

/// The base name of the session cookie: the name a *headless* client presents, and the
/// stem [`session_cookie_name`] scopes to a listener.
const SESSION_COOKIE: &str = "nexus_session";

/// The base name of the asset cookie — the credential that opens the static assets and
/// nothing else (review WEBS-1), scoped to a listener by [`asset_cookie_name`].
const ASSET_COOKIE: &str = "nexus_assets";

/// The one path the session token is needed on, and therefore the only path its cookie
/// is scoped to (review WEBS-1). Also the route match, so the two cannot drift: a
/// cookie scoped to a path the upgrade does not live at is a console that never
/// connects, and a route the cookie's scope does not cover is the same bug.
const WS_PATH: &str = "/ws";

/// The session cookie's name for a listener bound to `port` (review WEB-3).
///
/// A cookie's jar key is (name, domain, path) — never the port (RFC 6265 §8.5). With
/// one fixed name, the two-console arrangement §17 explicitly prescribes ("it is
/// single-daemon per instance in v1 — run two for two daemons") collided on one jar
/// entry: opening the second bootstrap URL replaced the first console's cookie, and
/// the first console's only recovery path is a reload, which 401s. Naming the cookie
/// after the listener gives each console its own entry, so both survive.
///
/// The *bound* port is used rather than the browser's view of it, because the name has
/// to match between the response that sets the cookie and every request that reads it,
/// and only the bound port is a constant of this server. The residual that leaves: two
/// consoles reached through SSH forwarding (§17) from different machines that happen to
/// share a bound port still collide, and forwarding to distinct local ports cannot be
/// seen from here.
fn session_cookie_name(port: u16) -> String {
    format!("{SESSION_COOKIE}_{port}")
}

/// The `Set-Cookie` value carrying the session token (§15.29).
///
/// `Secure` is added exactly when this listener terminates TLS. The cookie exists so
/// the token stops riding in URLs (§15.29); without `Secure`, the tier-2 cookie is
/// still attached to any plaintext request to the same host, which is the leak the
/// cookie was introduced to close (review WEB-2). It is *omitted* on the plaintext
/// tiers because browsers refuse to store a `Secure` cookie from a non-trustworthy
/// `http://` origin — setting it unconditionally would break the loopback default.
///
/// `Path` is [`WS_PATH`], not `/` (review WEBS-1). A cookie is never port-scoped, so a
/// `Path=/` token is attached to *every* request the operator's browser makes to any
/// service on this host — the finding's demonstration was a rogue sibling-port server
/// reading the token out of one ordinary navigation's access log. The browser needs the
/// token on exactly one path, the WebSocket upgrade, so that is the only path it is
/// offered on, and a sibling port sees it only if the operator visits a page there that
/// deliberately requests its own `/ws`. The assets keep their own credential
/// ([`asset_cookie`]) rather than sharing this one, which is what makes the narrow
/// scope survivable.
fn session_cookie(token: &str, secure: bool, port: u16) -> String {
    let name = session_cookie_name(port);
    let mut cookie = format!("{name}={token}; Path={WS_PATH}; HttpOnly; SameSite=Strict");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// The asset cookie's name for a listener bound to `port`, scoped for the same reason
/// [`session_cookie_name`] is: two consoles on one host must not collide on one jar
/// entry.
fn asset_cookie_name(port: u16) -> String {
    format!("{ASSET_COOKIE}_{port}")
}

/// The `Set-Cookie` value carrying the asset credential (review WEBS-1).
///
/// This is the value that *is* replayed to every path of every same-host service the
/// browser contacts, because a navigation and a `<script src>` can present nothing
/// else. It is therefore deliberately worth nothing: it opens the console's own
/// static assets — HTML, CSS and JavaScript that ship in the public source tree — and
/// is refused on `/ws`, which is where every capability lives. It carries the same
/// `HttpOnly`, `SameSite=Strict` and TLS-conditional `Secure` flags as the session
/// cookie: nothing about the gate is relaxed, only what leaks when it is replayed.
fn asset_cookie(credential: &str, secure: bool, port: u16) -> String {
    let name = asset_cookie_name(port);
    let mut cookie = format!("{name}={credential}; Path=/; HttpOnly; SameSite=Strict");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// A fresh 128-bit asset credential, hex-encoded, from the OS CSPRNG.
///
/// Independent of the token by construction rather than derived from it: a derivation
/// would make the token's secrecy depend on a hash's preimage resistance for no gain,
/// when the value can simply be unrelated. 128 bits is ample for a credential whose
/// only capability is reading files that ship in the repository.
fn new_asset_credential() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS RNG");
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
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
        let plain = session_cookie("abc", false, 8080);
        assert_eq!(
            plain,
            "nexus_session_8080=abc; Path=/ws; HttpOnly; SameSite=Strict"
        );
        // TLS tier: `Secure` keeps the token off any plaintext same-host request
        // (review WEB-2).
        let tls = session_cookie("abc", true, 8080);
        assert!(tls.starts_with("nexus_session_8080=abc; Path=/ws; HttpOnly; SameSite=Strict"));
        assert!(tls.ends_with("; Secure"), "{tls}");
        // The flags the cookie has always carried survive either way — and the asset
        // cookie carries every one of them too (review WEBS-1): the split narrows what
        // leaks, it does not relax the gate.
        let plain_asset = asset_cookie("cred", false, 8080);
        let tls_asset = asset_cookie("cred", true, 8080);
        assert_eq!(
            plain_asset,
            "nexus_assets_8080=cred; Path=/; HttpOnly; SameSite=Strict"
        );
        assert!(tls_asset.ends_with("; Secure"), "{tls_asset}");
        for c in [&plain, &tls, &plain_asset, &tls_asset] {
            assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict"));
        }
    }

    /// Review **WEBS-1**: the shell-equivalent token must not sit in a cookie the
    /// browser replays to every path of every same-host service it contacts.
    #[test]
    fn only_the_asset_credential_is_scoped_to_every_path() {
        let session = session_cookie("shell-equivalent", false, 8080);
        let asset = asset_cookie("worthless", false, 8080);
        // The token's cookie reaches one path — the upgrade — and that path is the
        // route constant, so the two cannot drift apart.
        assert!(session.contains("Path=/ws;"), "{session}");
        assert_eq!(WS_PATH, "/ws");
        // The everywhere-replayed cookie carries a value that is not the token.
        assert!(asset.contains("Path=/;"), "{asset}");
        assert!(!asset.contains("shell-equivalent"), "{asset}");
        // Two consoles on one host stay separate here too (review WEB-3's rule,
        // applied to the second cookie rather than rediscovered later).
        assert_ne!(asset_cookie_name(8080), asset_cookie_name(8081));
        assert_ne!(asset_cookie_name(8080), session_cookie_name(8080));
        // A fresh credential per run, from the OS CSPRNG, unrelated to any token.
        let (a, b) = (new_asset_credential(), new_asset_credential());
        assert_ne!(a, b, "the credential must not be a constant");
        assert_eq!(a.len(), 32, "128 bits, hex-encoded: {a}");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "{a}");
    }

    /// Review WEB-3: the cookie's jar key is (name, domain, path) and never the port,
    /// so two consoles on one host must not write the same name.
    #[test]
    fn the_session_cookie_is_named_for_its_listener() {
        let a = session_cookie("tok-a", false, 8080);
        let b = session_cookie("tok-b", false, 8081);
        assert!(a.starts_with("nexus_session_8080="), "{a}");
        assert!(b.starts_with("nexus_session_8081="), "{b}");
        // Same domain, same path — so the *name* is the only thing keeping the two
        // entries apart, and it does.
        assert!(a.contains("Path=/ws") && b.contains("Path=/ws"));
        assert_ne!(
            session_cookie_name(8080),
            session_cookie_name(8081),
            "two consoles must not collide on one jar entry (§17: run two for two daemons)"
        );
        // The unscoped base name is never *set* by anyone; it exists only as the name a
        // headless client may present.
        assert!(!a.starts_with("nexus_session="));
    }

    /// Review WEB-3, the shadowing half: a sibling-port page can plant a cookie of the
    /// same name under a longer path, and RFC 6265 §5.4 puts it first in the header.
    /// A first-value-wins reader saw only the plant.
    #[tokio::test]
    async fn every_cookie_value_for_a_name_is_offered_not_just_the_first() {
        let raw = b"GET /ws HTTP/1.1\r\nHost: 127.0.0.1:8080\r\n\
                    Cookie: nexus_session_8080=planted; a=1; nexus_session_8080=real; \
                    nexus_session=headless\r\n\r\n";
        let req = read_request(&mut &raw[..])
            .await
            .expect("parse")
            .expect("a head");
        let scoped: Vec<&str> = req.cookies("nexus_session_8080").collect();
        assert_eq!(
            scoped,
            vec!["planted", "real"],
            "both values must reach the caller, in header order"
        );
        // Which is what makes the gate survive the plant: the real token is accepted
        // even though a bogus value sorts ahead of it.
        assert!(
            req.cookies("nexus_session_8080").any(|c| ct_eq(c, "real")),
            "a planted cookie must not shadow the session"
        );
        // And the unscoped headless name is read independently of the scoped one.
        assert_eq!(
            req.cookies(SESSION_COOKIE).collect::<Vec<_>>(),
            vec!["headless"]
        );
        // A name that is a prefix of ours is not ours.
        assert_eq!(req.cookies("nexus_session_808").count(), 0);
    }

    /// Review WEBS-2: the bare `ws:`/`wss:` sources let a script open a WebSocket to
    /// any host, contradicting this file's own same-origin claim. `'self'` matches a
    /// same-origin WebSocket on both tiers in the pinned Chromium the UI suite uses.
    #[test]
    fn the_csp_confines_connections_to_this_origin() {
        assert!(CSP.contains("connect-src 'self';"), "{CSP}");
        for scheme in [" ws:", " wss:"] {
            assert!(
                !CSP.contains(scheme),
                "a bare scheme source reaches every host:{CSP}"
            );
        }
        // The rest of the policy is unchanged.
        assert!(CSP.starts_with("default-src 'none';"));
        assert!(CSP.contains("frame-ancestors 'none'"));
    }

    /// Review WEB-5: the unauthenticated population stays a small fraction of what
    /// credentialed work gets, and the head deadline stays short.
    #[test]
    fn the_pre_auth_population_is_a_fraction_of_the_connection_pool() {
        // Const-asserted, so widening the pre-auth pool fails to *compile* rather than
        // failing a test someone can skip. All the operands are compile-time constants;
        // there is nothing here a run-time check could see that the compiler cannot.
        const _: () = {
            assert!(
                MAX_PRE_AUTH_CONNECTIONS < MAX_CONNECTIONS,
                "peers that have shown nothing must not outnumber the ones that have"
            );
            assert!(
                MAX_CONNECTIONS >= 64,
                "the pool credentialed connections draw on must fit a lab's tabs"
            );
            // Comfortably more than a browser's parallel subresource loads, so a single
            // tab opening the console never evicts its own connections.
            assert!(
                MAX_PRE_AUTH_CONNECTIONS >= 16,
                "a browser's own parallel loads must not evict each other"
            );
            // A complete request head is one round trip; the deadline is only what a
            // peer that escapes eviction gets to hold a slot for.
            assert!(
                HEAD_TIMEOUT.as_secs() <= 5,
                "a shorter pre-auth deadline is what caps a stalled peer's hold time"
            );
        };
    }

    /// Review WEB-5, the regression the first remediation shipped: the cap must be
    /// enforced by evicting the **oldest** member, never by refusing the newcomer.
    ///
    /// Fail-first is direct here — the shipped behaviour was `try_acquire_owned()` on a
    /// 32-permit semaphore, whose observable is "the 33rd arrival gets nothing". This
    /// asserts the opposite: the 33rd arrival is admitted and the 1st pays for it. A
    /// pool that refused the newcomer could not produce an eviction signal on `[0]` at
    /// all.
    #[tokio::test]
    async fn the_pre_auth_pool_evicts_its_oldest_member_and_never_refuses_a_newcomer() {
        use tokio::sync::oneshot::error::TryRecvError;

        let pool = PreAuthPool::new();
        let mut members: Vec<(PreAuthTicket, oneshot::Receiver<()>)> = (0
            ..MAX_PRE_AUTH_CONNECTIONS)
            .map(|_| pool.admit())
            .collect();
        assert_eq!(pool.live(), MAX_PRE_AUTH_CONNECTIONS);
        for (i, (_, signal)) in members.iter_mut().enumerate() {
            assert!(
                matches!(signal.try_recv(), Err(TryRecvError::Empty)),
                "a full pool evicts nobody until someone new arrives (member {i})"
            );
        }

        // The newcomer — the operator's browser, in the scenario the finding names.
        let (newest, mut newest_signal) = pool.admit();
        assert!(
            matches!(newest_signal.try_recv(), Err(TryRecvError::Empty)),
            "the newest arrival must be admitted, not refused: refusing it is the \
             lockout, because the connection carrying the session cookie is always the \
             newest one"
        );
        assert!(
            matches!(members[0].1.try_recv(), Ok(())),
            "the oldest member is the one evicted — it has had the longest to send a head"
        );
        for (i, (_, signal)) in members.iter_mut().enumerate().skip(1) {
            assert!(
                matches!(signal.try_recv(), Err(TryRecvError::Empty)),
                "only as many members are evicted as the newcomer displaces (member {i})"
            );
        }
        // The population is back at the cap, not above it.
        assert_eq!(pool.live(), MAX_PRE_AUTH_CONNECTIONS);

        // Passing the token gate leaves the pool, so the slot comes back and the
        // connection stops being evictable.
        drop(newest);
        assert_eq!(pool.live(), MAX_PRE_AUTH_CONNECTIONS - 1);
    }

    /// Review WEB-5: leaving the pool must never *look* like an eviction.
    ///
    /// `evicted` races the connection itself, so if a dropped sender resolved it — the
    /// shape a plain `let _ = signal.await;` would have — every connection would be
    /// cancelled the instant it authenticated, killing each WebSocket bridge at birth.
    #[tokio::test]
    async fn leaving_the_pre_auth_pool_never_looks_like_an_eviction() {
        let pool = PreAuthPool::new();
        let (ticket, signal) = pool.admit();
        drop(ticket); // what the token gate does
        assert!(
            timeout(Duration::from_millis(50), evicted(signal))
                .await
                .is_err(),
            "a ticket that left the pool must never resolve its eviction signal"
        );
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
        assert_eq!(
            req.cookies("nexus_session").collect::<Vec<_>>(),
            vec!["tok"]
        );
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
