//! The WebSocket ↔ daemon bridge (design §17). Each browser WebSocket gets one
//! daemon control-socket connection; the server relays JSON-RPC both ways — a
//! *screening* proxy, not an interpreter.
//!
//! What the screen draws a line around changed with §15.35, and the reasoning is
//! worth keeping rather than just the outcome. The original non-goal was "the web
//! client never mutates the graph", enforced here so that even a compromised page
//! could not reconfigure the daemon. That was the right default until the operator
//! decided otherwise, and the argument that retired it is a fair one: a token holder
//! already commands every configured console on the machine — every device, every
//! `send` — so withholding graph edits protected little while costing real workflow.
//! So the allowlist now includes the **graph-editing** verbs (`add-node`,
//! `remove-node`, `connect`, `disconnect`, and the passive `ports`), and graph
//! editing is stated plainly in `docs/security.md` as daemon-user capability: a log
//! node writes files and an exec codec runs commands as the daemon's user, and
//! whoever holds the token is trusted with exactly that.
//!
//! **Lifecycle verbs stay off the browser wire.** `load` (which replaces the whole
//! graph), `teardown` and `shutdown` are refused here, because the ask was graph
//! editing and a browser page turning the daemon off serves no one.
//!
//! Everything else — `state`/`subscribe`/`info`/`dump`/`ports`, `tap.open`/
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
//!    here until someone deliberately admits it. A denylist made the boundary depend
//!    on remembering to extend it. Widening the list (as §15.35 did) is a deliberate
//!    act with a line of evidence behind it; that is exactly the property an
//!    allowlist buys and an inverted list would give away.

use std::path::PathBuf;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use serial_nexus_rpc::error_codes::{INVALID_REQUEST, METHOD_NOT_FOUND};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

/// The complete set of verbs the browser may invoke (§17, §15.35): §10's
/// observation, tap, arbitration, rotation, serial-signal **and graph-editing**
/// surface. Everything not named here is refused, which is what makes the boundary
/// fail safe — an unrecognized verb, a future one or a smuggled one, is refused by
/// default rather than admitted by oversight.
///
/// What is deliberately absent, and why:
///
/// * `load` — it replaces the entire graph in one call, which is a different act
///   from editing it. The editor page composes the incremental verbs instead.
/// * `teardown`, `shutdown` — daemon lifecycle. The ask was graph editing.
///
/// `set-attribute` is absent because it does not exist (§14). If it lands, admitting
/// it here is a deliberate act, exactly as admitting these was.
const ALLOWED: &[&str] = &[
    // Observation.
    "state",
    "subscribe",
    "info",
    "ports",
    "dump",
    "tap.open",
    "tap.close",
    // Arbitration, logging, serial signals. (A page that may `send` arbitrary bytes
    // to a device is not meaningfully restrained by withholding `send-break`.)
    "send",
    "lock",
    "unlock",
    "rotate",
    "send-break",
    "set-modem",
    "pulse-dtr",
    // Graph editing (§15.35). Daemon-user capability, stated in docs/security.md.
    "add-node",
    "remove-node",
    "connect",
    "disconnect",
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

    // Daemon → browser: forward each JSON line verbatim as a text frame. On the way
    // out it fires `daemon_eof`, which is how the loop below learns the daemon
    // connection ended (review WEB-4).
    //
    // A oneshot rather than the task's own `JoinHandle`: the handle is awaited once at
    // the end of this function, and a handle that had already been polled to completion
    // inside the `select!` would panic there.
    let (daemon_eof_tx, mut daemon_eof) = tokio::sync::oneshot::channel::<()>();
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
            let _ = daemon_eof_tx.send(());
        })
    };

    // Browser → daemon: filter, then forward. A denied or malformed request is
    // rejected locally with a JSON-RPC error, never reaching the daemon.
    //
    // Selected against the daemon reader, because the two directions have to be
    // symmetric (review WEB-4). Waiting only on `ws_stream.next()` handled "the browser
    // went away" and nothing else: when the *daemon* went away, the reader task exited
    // and dropped only its clone of `to_browser` while this loop kept the original, so
    // the writer never saw the channel close, `ws_sink.close()` was never called, and
    // the socket stayed open with no Close frame for as long as the tab was idle.
    // There is no keepalive and no watchdog anywhere in this binary, so nothing else
    // would have noticed: the page kept rendering "connected" over a dead daemon and
    // its own "disconnected — reload to reconnect" signal never fired.
    let daemon_gone = loop {
        let msg = tokio::select! {
            // `biased` so a daemon that has already ended is observed before another
            // browser frame is taken off the wire — there is nowhere left to send it.
            biased;
            // Ready exactly once, and this arm breaks, so it is never polled again.
            _ = &mut daemon_eof => break true,
            next = ws_stream.next() => match next {
                Some(Ok(m)) => m,
                // `None` (browser closed) or a frame error: the browser is gone.
                _ => break false,
            },
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
                // The daemon socket is AF_UNIX, so the first write after the peer
                // closes fails with EPIPE. That is the *other* way the daemon's
                // departure surfaces, and it is a departure, not a browser error.
                if d_write.write_all(line.as_bytes()).await.is_err() {
                    break true;
                }
            }
            Message::Binary(_) => {} // the protocol is JSON text only
            Message::Ping(p) => {
                let _ = to_browser.send(Message::Pong(p)).await;
            }
            Message::Close(_) => break false,
            _ => {}
        }
    };

    // Say *why* the console is losing its connection (WEB-4). A Close frame carrying a
    // reason is what lets the page tell "the daemon went away" — restart it and reload
    // — from a network drop, and §17's own principle is that a dead pane must never
    // look live. `Away` (1001) is the RFC 6455 code for an endpoint going away.
    if daemon_gone {
        let _ = to_browser
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Away,
                reason: "daemon connection closed".into(),
            })))
            .await;
    }

    // Either side gone: dropping d_write closes the daemon connection, which ends the
    // daemon reader (if it has not ended already), which drops the last `to_browser`,
    // which ends the writer — after it has drained the Close frame above. Await them
    // so nothing leaks.
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
                INVALID_REQUEST,
                "invalid request: a frame must be exactly one JSON-RPC request object",
            ));
        }
    };
    if !v.is_object() {
        return Err(rpc_error(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: a frame must be exactly one JSON-RPC request object",
        ));
    }
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    match v.get("method").and_then(Value::as_str) {
        None => Err(rpc_error(id, INVALID_REQUEST, "invalid request: no method")),
        // Fail safe: anything not named in ALLOWED is refused, including verbs §10
        // grows later (§17/§15.35: the console edits the graph, but daemon lifecycle
        // stays off the browser wire).
        Some(m) if !ALLOWED.contains(&m) => Err(rpc_error(
            id,
            METHOD_NOT_FOUND,
            &format!(
                "method {m:?} is not available from the web console (§17: lifecycle verbs stay off the browser wire)"
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

/// One JSON-RPC error object, rendered as the single line the browser gets back.
///
/// `code` comes from `serial_nexus_rpc::error_codes`, never a literal (review SIMPB-8): this
/// is the only JSON-RPC-error-emitting surface in the workspace, and it is the one a
/// compromised page hits, so `grep METHOD_NOT_FOUND` — the way a security reader
/// enumerates where a refusal can come from — has to find the browser boundary too.
/// The numbers are unchanged; the tests below still assert the literals on purpose,
/// because the wire contract is what they are pinning.
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
    fn lifecycle_verbs_are_refused_others_pass() {
        // §15.35 settled the posture: the console *edits* the graph, so the
        // graph-editing verbs are forwarded (asserted below, via ALLOWED). What stays
        // refused is daemon lifecycle plus whole-graph replacement — and
        // `set-attribute`, which does not exist and must not be admitted by
        // anticipation (§14). These are the ones a compromised page must not reach.
        for m in ["load", "teardown", "shutdown", "set-attribute"] {
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
        // The graph and editor pages (§15.35) drive these; the same fail-safe
        // direction, pinned so a future tightening of the list breaks a test rather
        // than the page.
        for m in [
            "dump",
            "ports",
            "add-node",
            "remove-node",
            "connect",
            "disconnect",
        ] {
            assert!(ALLOWED.contains(&m), "the editor page needs {m}");
        }
    }

    /// The posture change is bounded: widening the allowlist to graph editing must
    /// not have admitted lifecycle by accident. Stated as its own test because
    /// "these four are absent" is the property, and a list that grows silently is
    /// exactly what §17's allowlist shape exists to prevent.
    #[test]
    fn the_allowlist_admits_graph_editing_and_no_lifecycle_verb() {
        for m in ["load", "teardown", "shutdown", "set-attribute"] {
            assert!(
                !ALLOWED.contains(&m),
                "{m} must stay off the browser wire (§17/§15.35)"
            );
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

    /// Review SIMPB-8: the bridge's refusal codes come from `serial_nexus_rpc::error_codes`,
    /// the names the rest of the workspace uses, not from literals only this file
    /// knows. Asserted from both ends — the constant *and* the number it renders as —
    /// because the substitution had to leave the wire contract byte-identical.
    #[test]
    fn the_refusal_codes_are_the_registrys_and_the_wire_is_unchanged() {
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(rejection("not json")["error"]["code"], INVALID_REQUEST);
        assert_eq!(
            rejection("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"teardown\"}")["error"]["code"],
            METHOD_NOT_FOUND
        );
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
