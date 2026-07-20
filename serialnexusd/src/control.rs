//! The control plane: hand-rolled JSON-RPC 2.0 over newline-delimited JSON on a
//! Unix domain socket (design §10). One task per connection; mutations are
//! serialized by the current-thread runtime (plan §2). Batch arrays and parse
//! errors are refused cleanly with the spec-mandated null id.

use std::rc::Rc;

use nexus_rpc::{Response, parse_incoming_request};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::daemon::Daemon;

/// Serve one client connection until it closes: read newline-delimited
/// requests, dispatch each, and write back one response line.
pub async fn serve_connection(daemon: Rc<Daemon>, stream: UnixStream) {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => break, // client closed or read error
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&daemon, &line);
        if write_half
            .write_all(nexus_rpc::to_line(&response).as_bytes())
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Parse and dispatch one request line into a response. A parse or invalid
/// request yields an error response with `id: null` (§10, JSON-RPC 2.0 §5).
fn handle_line(daemon: &Daemon, line: &str) -> Response {
    match parse_incoming_request(line) {
        Ok(req) => match daemon.dispatch(&req.method, req.params) {
            Ok(result) => Response::success(req.id, result),
            Err(err) => Response::error(req.id, err),
        },
        Err(err) => Response::error_without_id(err),
    }
}
