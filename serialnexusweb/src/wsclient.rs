//! A headless WebSocket client (plan §11.3 validation): connect to a running web
//! server, tap one console through the browser-facing protocol, and write the
//! decoded hostward bytes to stdout — so an e2e script can checksum the stream end
//! to end (browser → server → daemon → device) without a browser.

use std::io::Write;

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
    /// The per-session bearer token (sent as the session cookie, §15.29).
    #[arg(long)]
    token: String,
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

/// Connect to the web server's WebSocket, presenting the session token as the
/// same-origin cookie (§15.29).
async fn connect(
    args: &WsclientArgs,
) -> anyhow::Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    let mut req = args
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| anyhow::anyhow!("bad --url {:?}: {e}", args.url))?;
    req.headers_mut().insert(
        COOKIE,
        format!("nexus_session={}", args.token)
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid token for a Cookie header"))?,
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(req)
        .await
        .map_err(|e| anyhow::anyhow!("WebSocket connect failed: {e}"))?;
    Ok(ws)
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
            if let Some(err) = v.get("error") {
                anyhow::bail!("tap.open failed: {err}");
            }
            opened = true;
            if let Some(rb) = v.get("result").and_then(|r| r.get("replay_bytes")) {
                eprintln!("tap opened: replay_bytes={rb}");
            }
            continue;
        }
        // tap.data notifications carry the base64 hostward bytes.
        if v.get("method").and_then(Value::as_str) == Some("tap.data") {
            let data = v
                .get("params")
                .and_then(|p| p.get("data"))
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("tap.data missing 'data'"))?;
            let bytes = nexus_rpc::base64_decode(data)
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
