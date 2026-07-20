//! The control plane: hand-rolled JSON-RPC 2.0 over newline-delimited JSON on a
//! Unix domain socket (design §10). One task per connection; mutations are
//! serialized by the current-thread runtime (plan §2). Batch arrays and parse
//! errors are refused cleanly with the spec-mandated null id.

use std::rc::Rc;

use nexus_rpc::{Response, parse_incoming_request};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::broadcast::error::RecvError;

use crate::daemon::Daemon;

/// Serve one client connection until it closes: read newline-delimited requests,
/// dispatch each, and write back one response line. Once the client issues
/// `subscribe`, this also forwards the daemon's id-less notifications to it (§10)
/// while still handling further requests on the same connection.
pub async fn serve_connection(daemon: Rc<Daemon>, stream: UnixStream) {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let mut notes = daemon.subscribe();
    let mut subscribed = false;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(line)) => line,
                    Ok(None) | Err(_) => break, // client closed or read error
                };
                if line.trim().is_empty() {
                    continue;
                }
                let (response, now_subscribed) = handle_line(&daemon, &line);
                if write_half
                    .write_all(nexus_rpc::to_line(&response).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
                subscribed |= now_subscribed;
            }
            // Only drain notifications once subscribed; before that the receiver
            // just buffers (and drops-oldest on lag), which we never read.
            note = notes.recv(), if subscribed => match note {
                Ok(note) => {
                    if write_half
                        .write_all(nexus_rpc::to_line(&note).as_bytes())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => {} // skipped snapshots; the next is current
                Err(RecvError::Closed) => break,
            },
        }
    }
}

/// Parse and dispatch one request line into a response, reporting whether it was
/// a successful `subscribe` (so the caller starts forwarding notifications). A
/// parse or invalid request yields an error response with `id: null` (§10,
/// JSON-RPC 2.0 §5).
fn handle_line(daemon: &Daemon, line: &str) -> (Response, bool) {
    match parse_incoming_request(line) {
        Ok(req) => {
            let is_subscribe = req.method == "subscribe";
            match daemon.dispatch(&req.method, req.params) {
                Ok(result) => (Response::success(req.id, result), is_subscribe),
                Err(err) => (Response::error(req.id, err), false),
            }
        }
        Err(err) => (Response::error_without_id(err), false),
    }
}
