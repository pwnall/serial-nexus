//! The WebSocket ↔ daemon bridge (design §17). Each browser WebSocket gets one
//! daemon control-socket connection; the server relays JSON-RPC both ways — a
//! filtering proxy, not an interpreter. Filtering enforces §17's hard non-goal: the
//! web client never mutates the graph, so graph and lifecycle verbs are refused at
//! the server (defence in depth — even a compromised page cannot `load` or
//! `teardown`). Everything else — `state`/`subscribe`/`info`/`dump`, `tap.open`/
//! `tap.close`, `send`, `lock`/`unlock`, `rotate`, the serial signals — passes
//! through, and the daemon's notifications (`state`, `lock`, `tap.data`) stream
//! back. Taps and `subscribe` are connection-scoped, so one daemon connection per
//! browser carries all of it (§10).

use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

/// Verbs the browser may NOT invoke: anything that mutates the operator-owned graph
/// or the daemon lifecycle (§17 non-goals). Everything else is proxied.
const DENIED: &[&str] = &[
    "load",
    "add-node",
    "remove-node",
    "teardown",
    "shutdown",
    "connect",
    "disconnect",
    "set-attribute",
];

pub async fn bridge<S: AsyncRead + AsyncWrite + Unpin + 'static>(
    ws: WebSocketStream<S>,
    socket: PathBuf,
) -> anyhow::Result<()> {
    let daemon = UnixStream::connect(&socket)
        .await
        .map_err(|e| anyhow::anyhow!("connecting to daemon {}: {e}", socket.display()))?;
    let (d_read, mut d_write) = daemon.into_split();
    let (mut ws_sink, mut ws_stream) = ws.split();

    // One channel funnels everything bound for the browser (relayed daemon lines and
    // locally-generated rejections) into a single writer, so no two tasks contend
    // for the sink.
    let (to_browser, mut to_browser_rx) = mpsc::channel::<Message>(256);

    // Subscribe up front so status, lock, and tap notifications flow (§10).
    d_write
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"subscribe\"}\n")
        .await?;

    // Writer: drain the funnel into the WebSocket sink.
    let writer = tokio::task::spawn_local(async move {
        while let Some(msg) = to_browser_rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
        let _ = ws_sink.close().await;
    });

    // Daemon → browser: forward each JSON line verbatim as a text frame.
    let daemon_to_browser = {
        let to_browser = to_browser.clone();
        tokio::task::spawn_local(async move {
            let mut lines = BufReader::new(d_read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                if to_browser.send(Message::Text(line.into())).await.is_err() {
                    break;
                }
            }
        })
    };

    // Browser → daemon: filter, then forward. A denied or malformed request is
    // rejected locally with a JSON-RPC error, never reaching the daemon.
    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };
        match msg {
            Message::Text(text) => {
                if let Some(reject) = screen(&text) {
                    let _ = to_browser.send(Message::Text(reject.into())).await;
                    continue;
                }
                let mut line = text.to_string();
                line.push('\n');
                if d_write.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
            Message::Binary(_) => {} // the protocol is JSON text only
            Message::Ping(p) => {
                let _ = to_browser.send(Message::Pong(p)).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Browser gone (or errored): dropping d_write closes the daemon connection, which
    // ends the daemon reader, which drops the last `to_browser`, which ends the
    // writer. Await them so nothing leaks.
    drop(d_write);
    drop(to_browser);
    let _ = daemon_to_browser.await;
    let _ = writer.await;
    Ok(())
}

/// Return `Some(error_json)` if this browser request must be refused (a denied verb,
/// or a structurally invalid request), else `None` to forward it. Keeps the daemon's
/// id for correlation when possible.
fn screen(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let method = v.get("method").and_then(Value::as_str);
    match method {
        None => Some(rpc_error(id, -32600, "invalid request: no method")),
        Some(m) if DENIED.contains(&m) => Some(rpc_error(
            id,
            -32601,
            &format!(
                "method {m:?} is not available from the web console (§17: it never mutates the graph)"
            ),
        )),
        Some(_) => None,
    }
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_mutating_verbs_are_refused_others_pass() {
        // Denied graph/lifecycle verbs are rejected locally (§17 non-goal).
        for m in DENIED {
            let req = format!("{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"{m}\"}}");
            let reject = screen(&req).expect("denied verb must be screened out");
            let v: Value = serde_json::from_str(&reject).unwrap();
            assert_eq!(v["id"], 3);
            assert!(
                v.get("error").is_some(),
                "rejection carries an error for {m}"
            );
        }
        // Operational verbs pass through untouched.
        for m in [
            "state", "tap.open", "send", "lock", "rotate", "dump", "info",
        ] {
            let req = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{m}\"}}");
            assert!(screen(&req).is_none(), "{m} should be forwarded");
        }
        // A request with no method is refused.
        assert!(screen("{\"jsonrpc\":\"2.0\",\"id\":1}").is_some());
    }
}
