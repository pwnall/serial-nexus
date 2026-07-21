//! The control plane: hand-rolled JSON-RPC 2.0 over newline-delimited JSON on a
//! Unix domain socket (design §10). One task per connection; mutations are
//! serialized by the current-thread runtime (plan §2). Batch arrays and parse
//! errors are refused cleanly with the spec-mandated null id.
//!
//! Dispatch is async because the arbitration verbs may *wait* (`lock --wait`,
//! `send`'s acquire-with-timeout, §15.20). A waiting verb is raced against a
//! disconnect on the same connection: if the client drops mid-wait, the dispatch
//! future is cancelled (dropped), which runs the waiter's cleanup guard and
//! removes it from the FIFO queue (§6 cancel-safe waiting). The race uses a
//! `biased` select so an immediately-ready fast verb is never pre-empted by a
//! spuriously-read next line.

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

                // Parse synchronously; a parse / invalid-request error replies with
                // the spec-mandated null id (§10, JSON-RPC 2.0 §5) and never reaches
                // dispatch.
                let req = match parse_incoming_request(&line) {
                    Ok(req) => req,
                    Err(err) => {
                        let resp = Response::error_without_id(err);
                        if write_half
                            .write_all(nexus_rpc::to_line(&resp).as_bytes())
                            .await
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                };

                let id = req.id.clone();
                let method = req.method;
                let params = req.params;
                let is_subscribe = method == "subscribe";

                // Dispatch may wait (`lock --wait`, `send`). Race it against a
                // disconnect so a dropped connection cancels the wait — dropping the
                // future runs the waiter's cleanup guard (§15.20). `biased` polls the
                // dispatch first, so a fast verb (ready on first poll) is taken
                // without ever reading — and possibly losing — a following request.
                let dispatch = daemon.dispatch(&method, params);
                tokio::pin!(dispatch);
                let response = tokio::select! {
                    biased;
                    result = &mut dispatch => match result {
                        Ok(value) => Response::success(id, value),
                        Err(err) => Response::error(id, err),
                    },
                    _ = lines.next_line() => {
                        // The client disconnected, or pipelined another request while
                        // this one was still waiting (unsupported). Either way abandon
                        // the in-flight verb: dropping `dispatch` cancels any wait and
                        // its guard leaves the FIFO queue.
                        break;
                    }
                };

                if write_half
                    .write_all(nexus_rpc::to_line(&response).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
                subscribed |= is_subscribe;
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
