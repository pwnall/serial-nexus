//! A headless WebSocket client (plan §11.3 validation): connect to a running web
//! server, tap one console through the browser-facing protocol, and write the
//! decoded hostward bytes to stdout — so an e2e script can checksum the stream end
//! to end (browser → server → daemon → device) without a browser.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Args;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::COOKIE;

#[derive(Args)]
pub struct WsclientArgs {
    /// The web server WebSocket URL, e.g. `ws://127.0.0.1:8080/ws`.
    #[arg(long)]
    url: String,
    /// The per-session bearer token (sent as the session cookie, §15.29). **This
    /// spelling puts the token in argv, where `/proc/<pid>/cmdline` exposes it to every
    /// local user for as long as the process runs** — the adversary the token exists to
    /// gate (§17). Prefer `--token-file`, or `SERIAL_NEXUS_WEB_TOKEN` in the
    /// environment.
    #[arg(long)]
    token: Option<String>,
    /// Read the token from this file (its first line, trimmed) instead of from argv —
    /// the recommended spelling. Keep the file mode 0600; a group- or world-readable
    /// one is served with a warning, since it hands the token to the same local users
    /// argv does.
    #[arg(long, conflicts_with = "token", value_name = "PATH")]
    token_file: Option<PathBuf>,
    /// The host-facing endpoint to tap (e.g. `usb0` or `mux/ch2`). Required unless
    /// `--rpc` is given.
    #[arg(long)]
    endpoint: Option<String>,
    /// Prefix the live stream with the endpoint's replay ring, if configured.
    #[arg(long)]
    replay: bool,
    /// Stop after this many decoded bytes (default: until the connection closes).
    #[arg(long)]
    bytes: Option<u64>,
    /// One-shot mode: send this RPC method through the bridge and print the JSON
    /// response (result or error) to stdout, then exit — for asserting the proxy
    /// relays a verb (or refuses a denied one, §17). Mutually exclusive with a tap.
    #[arg(long)]
    rpc: Option<String>,
    /// JSON params object for `--rpc` (default: none).
    #[arg(long)]
    params: Option<String>,
}

pub async fn run(args: WsclientArgs) -> anyhow::Result<()> {
    if let Some(method) = args.rpc.clone() {
        return run_rpc(args, method).await;
    }
    run_tap(args).await
}

/// One-shot: send `method` (with optional `--params`) through the WebSocket bridge
/// and print the correlated JSON response.
async fn run_rpc(args: WsclientArgs, method: String) -> anyhow::Result<()> {
    let mut ws = connect(&args).await?;
    let params: Value = match &args.params {
        Some(p) => serde_json::from_str(p).map_err(|e| anyhow::anyhow!("bad --params: {e}"))?,
        None => Value::Null,
    };
    ws.send(Message::Text(
        json!({ "jsonrpc": "2.0", "id": 42, "method": method, "params": params })
            .to_string()
            .into(),
    ))
    .await?;
    while let Some(Ok(msg)) = ws.next().await {
        if let Message::Text(text) = msg
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && v.get("id") == Some(&json!(42))
        {
            println!("{v}");
            return Ok(());
        }
    }
    anyhow::bail!("connection closed before a response to {method:?}")
}

/// The environment variable this client reads the session token from — the spelling a
/// script should prefer over `--token` (review 37-WEBS-1).
const TOKEN_ENV: &str = "SERIAL_NEXUS_WEB_TOKEN";

/// Resolve the session token from the three spellings this client accepts, in
/// precedence order: `--token-file`, then `--token`, then [`TOKEN_ENV`].
///
/// **Why there is more than one** (review 37-WEBS-1). The token is shell-equivalent —
/// whoever reaches `/ws` can add an exec codec — and §17's whole premise is that a
/// loopback port is reachable by *every local user*. A token on argv is published to
/// exactly those users through `/proc/<pid>/cmdline` for the process's lifetime, so the
/// one client the project sanctions for headless access must not be the one surface
/// that requires it. `--token` stays, because scripts and this project's own tests use
/// it and it is the right tool on a single-user box; it is no longer the only tool, and
/// its help says what it costs.
///
/// The environment is not free of exposure either — `/proc/<pid>/environ` carries it —
/// but that file is readable only by the process's own user, while `cmdline` is
/// world-readable by default on Linux. A file, mode 0600, is the spelling with no
/// process-table exposure at all.
fn resolve_token(
    token: Option<&str>,
    token_file: Option<&Path>,
    env: Option<String>,
) -> anyhow::Result<String> {
    if let Some(path) = token_file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading --token-file {}: {e}", path.display()))?;
        // First line, trimmed: `printf '%s\n' "$TOKEN" > tok` is how one gets written.
        let token = raw.lines().next().unwrap_or("").trim().to_string();
        if token.is_empty() {
            anyhow::bail!("--token-file {} is empty", path.display());
        }
        warn_if_readable_by_others(path);
        return Ok(token);
    }
    if let Some(token) = token {
        return Ok(token.to_string());
    }
    if let Some(token) = env.filter(|t| !t.is_empty()) {
        return Ok(token);
    }
    anyhow::bail!(
        "no session token: pass --token-file PATH (preferred), set {TOKEN_ENV}, or pass \
         --token (which publishes the token to every local user through \
         /proc/<pid>/cmdline)"
    )
}

/// Say so — once, on stderr — when a token file is readable beyond its owner.
///
/// stderr rather than `tracing`, because this binary's subscriber writes to stdout and
/// stdout is the tapped byte stream. A warning rather than a refusal: the file's mode is
/// the operator's call, and a hard failure here would strand a working setup, but a
/// group-readable token file hands the token to the same local users `--token` does,
/// which is the one thing this flag exists to avoid.
fn warn_if_readable_by_others(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let mode = meta.permissions().mode() & 0o077;
    if mode != 0 {
        eprintln!(
            "warning: --token-file {} is mode {:o}; the session token is readable \
             beyond its owner (chmod 600)",
            path.display(),
            meta.permissions().mode() & 0o777
        );
    }
}

/// Connect to the web server's WebSocket, presenting the session token as the
/// same-origin cookie (§15.29).
async fn connect(
    args: &WsclientArgs,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let token = resolve_token(
        args.token.as_deref(),
        args.token_file.as_deref(),
        std::env::var(TOKEN_ENV).ok(),
    )?;
    let mut req = args
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| anyhow::anyhow!("bad --url {:?}: {e}", args.url))?;
    // The *unscoped* cookie name, deliberately. Since review WEB-3 the server names the
    // cookie it sets after its own bound port (`nexus_session_<port>`), so two consoles
    // on one host stop evicting each other's jar entry — but a headless client is
    // handed a token, not a jar, and cannot know the bound port: under the SSH
    // forwarding §17 sanctions it is not even the port in `--url`. So the server also
    // accepts the base name, and that is the one this client presents.
    req.headers_mut().insert(
        COOKIE,
        format!("nexus_session={token}")
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid token for a Cookie header"))?,
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(req)
        .await
        .map_err(|e| anyhow::anyhow!("WebSocket connect failed: {e}"))?;
    Ok(ws)
}

/// The correlated answer to this client's own `tap.open` (id 1), turned into either the
/// operator's opening note or the refusal it exits on.
///
/// **The refusal reads exactly as `serial-nexus-ctl`'s does** (plan §18 item 59(c)).
/// This client and the CLI implement one rule — read the ack of a streaming verb before
/// the byte loop, and fail loudly naming what refused it (37-TOOL-2) — and they cannot
/// share the *code*, `read_ack` being shaped around a `BufRead` where this is shaped
/// around a WebSocket frame stream. They can share the *rendering*, which is the half an
/// operator sees, and this line used to print `{err}`: the whole JSON error object,
/// braces, `data` payload and all, where the CLI printed `<message> (<code>)`. Both now
/// call [`serial_nexus_rpc::describe_error`], the one place that rendering exists.
///
/// It is a free function rather than three lines inside the loop so that the rendering
/// is reachable from a test without a server, a bridge, or a daemon — which is what the
/// refusal arm had never had.
fn tap_open_ack(v: &Value) -> anyhow::Result<Option<String>> {
    if let Some(err) = v.get("error") {
        anyhow::bail!("tap.open failed: {}", serial_nexus_rpc::describe_error(err));
    }
    Ok(v.get("result")
        .and_then(|r| r.get("replay_bytes"))
        .map(|rb| format!("tap opened: replay_bytes={rb}")))
}

async fn run_tap(args: WsclientArgs) -> anyhow::Result<()> {
    let endpoint = args
        .endpoint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("--endpoint is required for a tap (or use --rpc)"))?;
    let mut ws = connect(&args).await?;

    // Open the tap over the proxied JSON-RPC surface.
    ws.send(Message::Text(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tap.open",
            "params": { "endpoint": endpoint, "replay": args.replay }
        })
        .to_string()
        .into(),
    ))
    .await?;

    let limit = args.bytes.unwrap_or(u64::MAX);
    let mut written = 0u64;
    let mut opened = false;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    while written < limit {
        let msg = match ws.next().await {
            Some(Ok(m)) => m,
            _ => break,
        };
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            Message::Ping(p) => {
                ws.send(Message::Pong(p)).await?;
                continue;
            }
            _ => continue,
        };
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // The tap.open response (id 1): fail loudly if the open was refused.
        if v.get("id") == Some(&json!(1)) {
            if let Some(note) = tap_open_ack(&v)? {
                eprintln!("{note}");
            }
            opened = true;
            continue;
        }
        // tap.data notifications carry the base64 hostward bytes.
        if v.get("method").and_then(Value::as_str) == Some("tap.data") {
            let data = v
                .get("params")
                .and_then(|p| p.get("data"))
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("tap.data missing 'data'"))?;
            let bytes = serial_nexus_rpc::base64_decode(data)
                .ok_or_else(|| anyhow::anyhow!("tap.data 'data' is not valid base64"))?;
            let take = ((limit - written) as usize).min(bytes.len());
            out.write_all(&bytes[..take])?;
            out.flush()?;
            written += take as u64;
        }
    }
    if !opened {
        anyhow::bail!("connection closed before the tap.open acknowledgement");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private temp directory, the shape `tls.rs`'s tests use: pid plus a monotonic
    /// counter is unique within a run, and nothing has to be added to build it.
    fn scratch_dir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("snx-ws-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Review **37-WEBS-1**: the shell-equivalent token has a spelling that does not
    /// ride argv, where every local user reads it out of `/proc/<pid>/cmdline`.
    ///
    /// The process environment is asserted through a parameter rather than by setting a
    /// variable, deliberately: `std::env::set_var` is process-global and this suite runs
    /// its tests in threads (§5, run under parallelism), so a test that mutated the
    /// environment would be reaching into its neighbours.
    #[test]
    fn a_token_can_be_supplied_without_putting_it_in_argv() {
        let dir = scratch_dir();
        let file = dir.join("token");
        std::fs::write(&file, "from-the-file\n").unwrap();

        // Each of the two argv-free spellings is sufficient on its own.
        assert_eq!(
            resolve_token(None, Some(&file), None).unwrap(),
            "from-the-file",
            "a token file is a complete credential source"
        );
        assert_eq!(
            resolve_token(None, None, Some("from-the-environment".into())).unwrap(),
            "from-the-environment"
        );
        // …and the file wins over the environment, being the narrower one.
        assert_eq!(
            resolve_token(None, Some(&file), Some("from-the-environment".into())).unwrap(),
            "from-the-file"
        );
        // `--token` still works — it is the right tool on a single-user box, and this
        // project's own tests pass it — and still beats an inherited variable, because
        // an explicit flag is the more specific statement of intent.
        assert_eq!(
            resolve_token(Some("from-argv"), None, Some("from-the-environment".into())).unwrap(),
            "from-argv"
        );

        // Nothing at all names all three spellings: a client that cannot authenticate
        // must say how to.
        let err = format!("{}", resolve_token(None, None, None).unwrap_err());
        for spelling in ["--token-file", TOKEN_ENV, "--token"] {
            assert!(err.contains(spelling), "{err}");
        }
        // An empty variable is "unset", not a token: `TOKEN=$(cat missing)` is one
        // typo away and would otherwise present an empty cookie and 401.
        assert!(resolve_token(None, None, Some(String::new())).is_err());

        // An empty file is named rather than silently presented as an empty token.
        let empty = dir.join("empty");
        std::fs::write(&empty, "").unwrap();
        let err = format!("{}", resolve_token(None, Some(&empty), None).unwrap_err());
        assert!(err.contains(&empty.display().to_string()), "{err}");
        // As is one that does not exist — the whole path, since that is what a script
        // has to fix.
        let missing = dir.join("nope");
        let err = format!("{}", resolve_token(None, Some(&missing), None).unwrap_err());
        assert!(err.contains(&missing.display().to_string()), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A refused `tap.open` prints what the CLI prints** (plan §18 item 59(c)).
    ///
    /// The expected string is written out in full here *and* in `serial-nexus-ctl`'s own
    /// guard, because these are two binaries in two crates and no test can watch both at
    /// once without a server, a bridge and a daemon — the arm this one covers had, until
    /// now, no test of any kind anywhere: the tree's only headless tap invocation is
    /// `p8_web`'s happy path, so the refusal line was written once, diverged, and stayed
    /// diverged. Two literal pins that agree by construction are the enforceable form:
    /// editing either client's rendering reddens that client's own suite.
    #[test]
    fn a_refused_tap_open_renders_the_error_the_way_the_cli_does() {
        let refused = json!({
            "jsonrpc": "2.0", "id": 1,
            "error": {"code": -32602, "message": "unknown endpoint: nope",
                      "data": {"available": ["usb0"]}}
        });
        let err = tap_open_ack(&refused).unwrap_err().to_string();
        assert_eq!(err, "tap.open failed: unknown endpoint: nope (-32602)");
        // The braces are the tell for the rendering this replaced: `{err}` on the raw
        // object printed the whole thing, `data` included.
        assert!(!err.contains('{'), "the raw error object is back: {err}");
        assert!(!err.contains("usb0"), "`data` rode the line: {err}");

        // The success arms: the replay note when the daemon reports a prefix, silence
        // when it does not. Neither is an error, and both mean the tap is open.
        assert_eq!(
            tap_open_ack(&json!({"id": 1, "result": {"replay_bytes": 4096}}))
                .expect("an accepted open is not an error"),
            Some("tap opened: replay_bytes=4096".to_string())
        );
        assert_eq!(
            tap_open_ack(&json!({"id": 1, "result": {}})).expect("still not an error"),
            None
        );

        // A bridge that answers something that is not an error object still fails, and
        // still says what arrived — `describe_error`'s fallback, exercised through the
        // one call site that can meet it.
        let odd = tap_open_ack(&json!({"id": 1, "error": "boom"}))
            .unwrap_err()
            .to_string();
        assert_eq!(err_tail(&odd), "\"boom\"");
    }

    /// The part of a refusal line after `tap.open failed: ` — the rendering itself.
    fn err_tail(line: &str) -> &str {
        line.strip_prefix("tap.open failed: ")
            .unwrap_or_else(|| panic!("a refusal must name the verb it refused: {line}"))
    }
}
