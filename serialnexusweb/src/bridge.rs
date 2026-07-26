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
//!
//! Two properties make the filter binding rather than decorative, both settled by a
//! reproduced bypass (review WEB-1/SEC-1):
//!
//! 1. **The screen decides on a parsed value, and the forwarder sends that value
//!    re-serialised** — never the browser's raw frame. The daemon's control socket is
//!    NDJSON, so a frame carrying `{…"method":"info"}\n{…"method":"teardown"}` used to
//!    split into two requests on the far side, of which only the first had been
//!    screened. Re-serialising one parsed object emits exactly one line (JSON escapes
//!    every interior newline), so what was screened is exactly what is sent.
//! 2. **The verb set is an allowlist**, so a verb added to §10 tomorrow is refused
//!    here until someone deliberately admits it. A denylist made the §17 non-goal
//!    depend on remembering to extend it.

use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

/// The complete set of verbs the browser may invoke: §10's observation, tap,
/// arbitration, rotation and serial-signal surface — everything that is *not* graph
/// mutation (`load`, `add-node`, `remove-node`, and the §14-deferred `connect` /
/// `disconnect` / `set-attribute`) or daemon lifecycle (`teardown`, `shutdown`).
/// That boundary is §17's; the allowlist shape is what makes it fail safe, since an
/// unrecognized verb — a future one, or a smuggled one — is refused by default.
/// (`dump` is here because it is read-only: it reports configuration, never writes
/// it. The serial signals are here because §17 draws its line at graph and lifecycle,
/// and a page that may `send` arbitrary bytes to a device is not meaningfully
/// restrained by withholding `send-break`; the UI does not drive them today.)
const ALLOWED: &[&str] = &[
    "state",
    "subscribe",
    "info",
    "dump",
    "tap.open",
    "tap.close",
    "send",
    "lock",
    "unlock",
    "rotate",
    "send-break",
    "set-modem",
    "pulse-dtr",
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
                // Screen the *parsed* request, then forward that same value
                // re-serialised. The raw frame is never written to the daemon, so no
                // second request can ride behind a newline past the screen (WEB-1).
                let line = match screen(&text) {
                    Ok(value) => forward_line(&value),
                    Err(reject) => {
                        let _ = to_browser.send(Message::Text(reject.into())).await;
                        continue;
                    }
                };
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

/// Screen one browser frame. `Ok(value)` is the request to forward — the *parsed*
/// value, so the caller cannot forward anything the screen did not see; `Err(json)`
/// is the JSON-RPC error to return instead. Keeps the request's id for correlation
/// when the frame is well-formed enough to have one.
///
/// The result type is deliberately not `Option`: the previous shape returned `None`
/// both for "forward it" and for "I could not parse it", and `serde_json::from_str`
/// rejecting a two-request frame therefore *forwarded* it unscreened (WEB-1). There
/// is now no value of the return type that means "unscreened".
fn screen(text: &str) -> Result<Value, String> {
    // Exactly one JSON value, and it must be an object: a trailing second request
    // (`{…}\n{…}`) fails to parse here, and a batch array — which the daemon rejects
    // anyway (§10) — is not an object.
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            return Err(rpc_error(
                Value::Null,
                -32600,
                "invalid request: a frame must be exactly one JSON-RPC request object",
            ));
        }
    };
    if !v.is_object() {
        return Err(rpc_error(
            Value::Null,
            -32600,
            "invalid request: a frame must be exactly one JSON-RPC request object",
        ));
    }
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    match v.get("method").and_then(Value::as_str) {
        None => Err(rpc_error(id, -32600, "invalid request: no method")),
        // Fail safe: anything not named in ALLOWED is refused, including verbs §10
        // grows later (§17: the web client never mutates the graph).
        Some(m) if !ALLOWED.contains(&m) => Err(rpc_error(
            id,
            -32601,
            &format!(
                "method {m:?} is not available from the web console (§17: it never mutates the graph)"
            ),
        )),
        Some(_) => Ok(v),
    }
}

/// Render a screened request as exactly one NDJSON line for the daemon socket.
/// `serde_json` escapes every control character inside strings, so the only newline
/// in the result is the terminator — the property that makes the screen binding
/// (WEB-1).
fn forward_line(v: &Value) -> String {
    // A `Value` that came out of `from_str` always re-serialises (no NaN, no
    // non-string keys), so this cannot realistically fail; `to_string` on a
    // hypothetical failure would drop the request, and an empty line the daemon
    // skips is the safe reading of "do nothing".
    let mut line = serde_json::to_string(v).unwrap_or_default();
    line.push('\n');
    line
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

    /// The error object of a rejection, for assertions.
    fn rejection(text: &str) -> Value {
        let msg = screen(text).expect_err("this frame must be refused");
        serde_json::from_str(&msg).expect("a rejection is JSON")
    }

    #[test]
    fn graph_mutating_verbs_are_refused_others_pass() {
        // Graph and lifecycle verbs are rejected locally (§17 non-goal), including the
        // §14-deferred surgery verbs the daemon does not implement yet.
        for m in [
            "load",
            "add-node",
            "remove-node",
            "teardown",
            "shutdown",
            "connect",
            "disconnect",
            "set-attribute",
        ] {
            let req = format!("{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"{m}\"}}");
            let v = rejection(&req);
            assert_eq!(v["id"], 3, "the rejection keeps the id for correlation");
            assert_eq!(v["error"]["code"], -32601, "refusing {m}");
            assert!(
                v["error"]["message"]
                    .as_str()
                    .is_some_and(|s| s.contains("§17")),
                "the rejection cites the design section for {m}: {v}"
            );
        }
        // Every allowlisted verb is forwarded, unchanged.
        for m in ALLOWED {
            let req = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{m}\"}}");
            let fwd = screen(&req).unwrap_or_else(|e| panic!("{m} should be forwarded: {e}"));
            assert_eq!(fwd["method"], *m);
            assert_eq!(fwd["id"], 1);
        }
        // A request with no method is refused.
        assert_eq!(
            rejection("{\"jsonrpc\":\"2.0\",\"id\":1}")["error"]["code"],
            -32600
        );
    }

    #[test]
    fn the_verbs_the_console_uses_are_allowed() {
        // app.js drives exactly these; if one is ever dropped from ALLOWED the console
        // breaks silently, so the allowlist's fail-safe direction is pinned both ways.
        for m in [
            "subscribe",
            "info",
            "state",
            "tap.open",
            "tap.close",
            "send",
        ] {
            assert!(ALLOWED.contains(&m), "app.js needs {m}");
        }
    }

    #[test]
    fn a_second_request_behind_a_newline_is_refused_never_forwarded() {
        // WEB-1/SEC-1, reproduced: one frame, two NDJSON lines. The daemon's socket
        // would have split it into two dispatches, of which only the first was ever
        // screened — so `teardown` (and `shutdown`) executed.
        let smuggled = "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"info\"}\n\
                        {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"teardown\"}";
        let outcome = screen(smuggled);
        assert!(
            outcome.is_err(),
            "a multi-request frame must be refused, not forwarded: {outcome:?}"
        );
        assert_eq!(rejection(smuggled)["error"]["code"], -32600);
        // The same shape with an allowed second verb is refused too: the rule is
        // "exactly one request per frame", not "screen every line".
        assert!(
            screen("{\"id\":1,\"method\":\"state\"}\n{\"id\":2,\"method\":\"state\"}").is_err()
        );
    }

    #[test]
    fn batches_and_scalars_are_refused() {
        // A batch array is not one request object (§10 rejects batches anyway).
        assert_eq!(
            rejection("[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"state\"}]")["error"]["code"],
            -32600
        );
        // Nor is a bare scalar or a string.
        for text in ["7", "\"state\"", "null", "true", ""] {
            assert!(screen(text).is_err(), "{text:?} must be refused");
        }
    }

    #[test]
    fn an_unknown_future_verb_is_refused_by_the_allowlist() {
        // The point of the allowlist: a verb §10 grows tomorrow is denied here until
        // someone deliberately admits it, rather than permitted by omission.
        for m in ["reload", "graph.patch", "load-v2", "TEARDOWN"] {
            let req = format!("{{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"{m}\"}}");
            assert_eq!(rejection(&req)["error"]["code"], -32601, "refusing {m}");
        }
    }

    #[test]
    fn a_forwarded_line_is_exactly_one_ndjson_line() {
        // Even a payload full of newlines re-serialises to one line: JSON escapes
        // control characters inside strings, which is why forwarding the parsed value
        // (not the raw frame) closes the WEB-1 class rather than one instance.
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "send",
            "params": { "endpoint": "usb0", "line": "a\n{\"method\":\"shutdown\"}\n" }
        })
        .to_string();
        let line = forward_line(&screen(&req).expect("send is allowed"));
        assert!(line.ends_with('\n'));
        assert_eq!(
            line.matches('\n').count(),
            1,
            "exactly one newline — the terminator: {line:?}"
        );
        // Round-trips to the same request the screen approved.
        let back: Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(back["method"], "send");
        assert_eq!(back["params"]["line"], "a\n{\"method\":\"shutdown\"}\n");
    }
}
