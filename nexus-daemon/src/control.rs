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

use std::io;
use std::rc::Rc;

use nexus_rpc::{
    Notification, Response, RpcError, base64_encode, error_codes, parse_incoming_request,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedReadHalf;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use crate::daemon::Daemon;
use crate::tap::{OpenTap, TAP_QUEUE_CAP, TapMsg};

/// Hard cap on the length, in bytes, of a single control-plane request line
/// (§10). [`RequestLines::next_line`] refuses a longer line rather than letting
/// one connection grow the shared daemon's read buffer without bound (CTRL-1) —
/// the read-path analogue of the `SUN_LEN` socket-path bound (implementation-notes
/// §7). One MiB sits far above any real control verb, including a `load`'s inline
/// graph JSON, and far below memory-pressure territory.
const MAX_REQUEST_LINE: usize = 1 << 20;

/// The outcome of reading one request line.
#[derive(Debug)]
enum LineRead {
    /// A complete `\n`-terminated line (trailing `\r` stripped).
    Line(String),
    /// A clean end-of-stream on the read half (the peer closed or half-closed it).
    Eof,
    /// A line that reached [`MAX_REQUEST_LINE`] with no newline; the caller must
    /// refuse it and close the connection.
    TooLong,
}

/// A newline-delimited request reader with a hard per-line length cap (§10).
/// Unlike [`tokio::io::Lines`], whose accumulator grows without bound until it
/// sees a newline, this refuses a line once it passes [`MAX_REQUEST_LINE`] so one
/// connection cannot exhaust the shared daemon's memory (CTRL-1). The in-progress
/// line lives in `self.buf` (not on the future's stack), which — together with
/// `fill_buf`'s own cancel-safety — keeps `next_line` cancel-safe: a partially
/// read line survives the biased select dropping the read future mid-line, so a
/// pipelined request is never truncated (§15.20).
struct RequestLines {
    reader: BufReader<OwnedReadHalf>,
    buf: Vec<u8>,
}

impl RequestLines {
    fn new(read_half: OwnedReadHalf) -> Self {
        RequestLines {
            reader: BufReader::new(read_half),
            buf: Vec::new(),
        }
    }

    /// Read the next `\n`-terminated line, capped at [`MAX_REQUEST_LINE`] bytes.
    /// Cancel-safe: dropping the returned future retains any partial line in
    /// `self.buf`. A trailing `\r` is stripped to match `Lines`; invalid UTF-8
    /// surfaces as an `InvalidData` error (closing the connection, as before).
    async fn next_line(&mut self) -> io::Result<LineRead> {
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                // Clean EOF. Any buffered bytes without a trailing newline are the
                // final line (matching `Lines`); an empty buffer is end-of-stream.
                if self.buf.is_empty() {
                    return Ok(LineRead::Eof);
                }
                return self.take_line();
            }
            if let Some(pos) = available.iter().position(|&b| b == b'\n') {
                if self.buf.len() + pos > MAX_REQUEST_LINE {
                    self.reader.consume(pos + 1);
                    self.buf.clear();
                    return Ok(LineRead::TooLong);
                }
                self.buf.extend_from_slice(&available[..pos]);
                self.reader.consume(pos + 1);
                return self.take_line();
            }
            // No newline in this chunk. Stop the moment the line would pass the cap
            // — refusing before the buffer can grow unbounded.
            let chunk = available.len();
            if self.buf.len() + chunk > MAX_REQUEST_LINE {
                self.reader.consume(chunk);
                self.buf.clear();
                return Ok(LineRead::TooLong);
            }
            self.buf.extend_from_slice(available);
            self.reader.consume(chunk);
        }
    }

    /// Move the accumulated bytes out as a `String`, stripping one trailing `\r`.
    fn take_line(&mut self) -> io::Result<LineRead> {
        let mut bytes = std::mem::take(&mut self.buf);
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        String::from_utf8(bytes)
            .map(LineRead::Line)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.utf8_error()))
    }
}

/// Serve one client connection until it closes: read newline-delimited requests,
/// dispatch each, and write back one response line. Once the client issues
/// `subscribe`, this also forwards the daemon's id-less notifications to it (§10)
/// while still handling further requests on the same connection.
pub async fn serve_connection(daemon: Rc<Daemon>, stream: UnixStream) {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = RequestLines::new(read_half);
    let mut notes = daemon.subscribe();
    let mut subscribed = false;

    // Per-connection tap plumbing (§17): the hubs of every tapped endpoint deliver
    // this connection's tap bytes into one bounded channel (the §5 boundary — a slow
    // tab fills it and its taps count drops), which the loop drains into `tap.data`
    // notifications. `open_taps` tracks the taps this connection opened so they close
    // on `tap.close` and, via `OpenTap`'s drop, when the connection ends.
    let (tap_tx, mut tap_rx) = mpsc::channel::<TapMsg>(TAP_QUEUE_CAP);
    let mut open_taps: Vec<OpenTap> = Vec::new();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line {
                    Ok(LineRead::Line(line)) => line,
                    Ok(LineRead::Eof) | Err(_) => break, // client closed or read error
                    Ok(LineRead::TooLong) => {
                        // One connection streaming a line with no newline would
                        // otherwise grow the shared daemon's read buffer without
                        // bound and OOM every console it serves (CTRL-1). Refuse the
                        // over-cap line with the null id the spec mandates for an
                        // unparseable request, then close.
                        let err = RpcError::new(
                            error_codes::INVALID_REQUEST,
                            format!("request line exceeds the {MAX_REQUEST_LINE}-byte limit"),
                        );
                        let _ = write_half
                            .write_all(
                                nexus_rpc::to_line(&Response::error_without_id(err)).as_bytes(),
                            )
                            .await;
                        break;
                    }
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

                // Taps are connection-scoped (§17): handle `tap.open`/`tap.close`
                // here, where the connection's outbound tap channel and open-tap
                // list live, rather than in the shared dispatch. Both complete
                // synchronously, with no waiting lane.
                let response = if method == "tap.open" {
                    match daemon.tap_open(params, tap_tx.clone()) {
                        Ok((value, handle)) => {
                            open_taps.push(handle);
                            Response::success(id, value)
                        }
                        Err(err) => Response::error(id, err),
                    }
                } else if method == "tap.close" {
                    match params
                        .as_ref()
                        .and_then(|p| p.get("tap"))
                        .and_then(Value::as_u64)
                    {
                        Some(tap_id) => match open_taps.iter().position(|t| t.tap_id == tap_id) {
                            // Dropping the removed `OpenTap` detaches it from its hub.
                            Some(pos) => {
                                drop(open_taps.remove(pos));
                                Response::success(id, json!({ "closed": tap_id }))
                            }
                            None => Response::error(
                                id,
                                RpcError::invalid_params(format!(
                                    "no open tap {tap_id} on this connection"
                                )),
                            ),
                        },
                        None => Response::error(
                            id,
                            RpcError::invalid_params("missing 'tap' in params"),
                        ),
                    }
                } else {
                    // Dispatch may wait (`lock --wait`, `send`). Race it against a
                    // disconnect so a dropped connection cancels the wait — dropping the
                    // future runs the waiter's cleanup guard (§15.20). `biased` polls the
                    // dispatch first, so a fast verb (ready on first poll) is taken
                    // without ever reading — and possibly losing — a following request.
                    //
                    // Any resolution of the second lane — a dropped/half-closed connection
                    // (EOF), a pipelined request (unsupported while a verb waits), an
                    // over-cap line, or a read error — abandons the in-flight verb and
                    // closes: the design's §15.20 cancel-on-disconnect is normative (a
                    // killed `lock --wait` client must dequeue promptly), so a bare
                    // write-half half-close is treated as a disconnect. `serialnexusctl`
                    // keeps both halves open across the read, so its waiting verbs are
                    // unaffected; a raw `socat` waiting-verb user must likewise keep the
                    // write half open (CTRL-3: current behavior is design-correct).
                    let dispatch = daemon.dispatch(&method, params);
                    tokio::pin!(dispatch);
                    tokio::select! {
                        biased;
                        result = &mut dispatch => match result {
                            Ok(value) => Response::success(id, value),
                            Err(err) => Response::error(id, err),
                        },
                        _ = lines.next_line() => break,
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
            // Tap bytes for this connection (§17): base64-frame each into a `tap.data`
            // notification and write it. `recv` yields `None` only when every sender
            // is gone; the connection holds `tap_tx` for its whole life, so this only
            // pends (no tap data) rather than firing spuriously.
            tap = tap_rx.recv() => if let Some(msg) = tap {
                let note = Notification::new(
                    "tap.data",
                    Some(json!({
                        "tap": msg.tap_id,
                        // The endpoint hostward offset of this chunk's first byte
                        // (§11.8), so a client splices replay and live exactly.
                        "offset": msg.offset,
                        "data": base64_encode(&msg.bytes),
                    })),
                );
                if write_half
                    .write_all(nexus_rpc::to_line(&note).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    // Two CR/LF- and LF-terminated requests then a half-close: each line comes back
    // with its trailing CR stripped (matching `tokio::io::Lines`), and the write-half
    // EOF surfaces as the distinct `LineRead::Eof` the dispatch race relies on to keep
    // a waiting verb alive (CTRL-3).
    #[tokio::test]
    async fn reads_delimited_lines_then_reports_clean_eof() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let (read_half, _write_half) = server.into_split();
        let mut lines = RequestLines::new(read_half);

        client.write_all(b"{\"a\":1}\r\n{\"b\":2}\n").await.unwrap();
        client.shutdown().await.unwrap();

        match lines.next_line().await.unwrap() {
            LineRead::Line(l) => assert_eq!(l, "{\"a\":1}"),
            _ => panic!("expected the first line"),
        }
        match lines.next_line().await.unwrap() {
            LineRead::Line(l) => assert_eq!(l, "{\"b\":2}"),
            _ => panic!("expected the second line"),
        }
        assert!(matches!(lines.next_line().await.unwrap(), LineRead::Eof));
    }

    // A connection streaming past the cap with no newline must not grow the read
    // buffer without bound: `next_line` stops and reports `TooLong` (CTRL-1) rather
    // than accumulating the whole stream.
    #[tokio::test]
    async fn over_cap_line_is_refused_not_buffered() {
        let (client, server) = UnixStream::pair().unwrap();
        let (read_half, _write_half) = server.into_split();
        let mut lines = RequestLines::new(read_half);

        // Feed the over-cap run from a separate task so writer and draining reader
        // both make progress on the single-threaded runtime; tolerate the read half
        // dropping once we bail out.
        tokio::spawn(async move {
            let mut client = client;
            let blob = vec![b'x'; MAX_REQUEST_LINE + 16];
            let _ = client.write_all(&blob).await;
            let _ = client.write_all(b"\n").await;
        });

        assert!(matches!(
            lines.next_line().await.unwrap(),
            LineRead::TooLong
        ));
    }
}
