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
//! spuriously-read next line. The second lane **discriminates** what it read
//! (CTRL-1): only an EOF or a read error cancels, because only those are the
//! disconnect §15.20 sanctions; a *pipelined* request is answered with an error
//! carrying its own id and the wait continues.

use std::io;
use std::rc::Rc;

use nexus_rpc::{
    AppError, Notification, Response, RpcError, base64_encode, error_codes, parse_incoming_request,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
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
pub const MAX_REQUEST_LINE: usize = 1 << 20;

/// The refusal a *pipelined* request gets while a waiting verb is in flight on the
/// same connection (CTRL-1). Answering it — with its own id, so the client can
/// correlate — is what the design's §15.20 cancel-on-*disconnect* rule does not
/// cover: a second request line is not a disconnect, and killing the connection
/// there costs a §17 browser its whole session (one daemon connection carries
/// `subscribe`, every tap, and `send`).
///
/// The code is [`AppError::WaitInFlight`], a registered application code (§16.8):
/// the refusal is about the *connection's state*, not the request's contents, so a
/// standard framing code would misdescribe it and a client could not tell "retry
/// this once the wait resolves" from "this request is malformed".
const WAIT_IN_FLIGHT_MESSAGE: &str = "a waiting verb is already in flight on this connection; only one is supported \
     at a time — open a second connection, or retry once it resolves";

/// The outcome of reading one request line.
#[derive(Debug)]
pub enum LineRead {
    /// A complete `\n`-terminated line (trailing `\r` stripped).
    Line(String),
    /// A clean end-of-stream on the read half (the peer closed or half-closed it).
    Eof,
    /// A line that reached [`MAX_REQUEST_LINE`] with no newline; the caller must
    /// refuse it and close the connection.
    TooLong,
}

/// A newline-delimited request reader with a hard per-line length cap (§10).
///
/// `pub`, but this module is private: the only way in from outside the crate is the
/// deliberate `unstable_fuzz_api` re-export in `lib.rs`, which states its own terms
/// (not semver'd, may vanish). §15.26's embeddable surface is unaffected.
/// Unlike [`tokio::io::Lines`], whose accumulator grows without bound until it
/// sees a newline, this refuses a line once it passes [`MAX_REQUEST_LINE`] so one
/// connection cannot exhaust the shared daemon's memory (CTRL-1). The in-progress
/// line lives in `self.buf` (not on the future's stack), which — together with
/// `fill_buf`'s own cancel-safety — keeps `next_line` cancel-safe: a partially
/// read line survives the biased select dropping the read future mid-line, so a
/// pipelined request is never truncated (§15.20).
pub struct RequestLines<R> {
    reader: BufReader<R>,
    buf: Vec<u8>,
}

impl<R: tokio::io::AsyncRead + Unpin> RequestLines<R> {
    pub fn new(read_half: R) -> Self {
        RequestLines {
            reader: BufReader::new(read_half),
            buf: Vec::new(),
        }
    }

    /// Read the next `\n`-terminated line, capped at [`MAX_REQUEST_LINE`] bytes.
    /// Cancel-safe: dropping the returned future retains any partial line in
    /// `self.buf`. A trailing `\r` is stripped to match `Lines`; invalid UTF-8
    /// surfaces as an `InvalidData` error (closing the connection, as before).
    pub async fn next_line(&mut self) -> io::Result<LineRead> {
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
    // Counts this connection among the daemon's *subscribers* for as long as it is
    // subscribed (OBS-1). A broadcast receiver is taken at accept — before any
    // `subscribe` — so `receiver_count()` counts connections, not subscribers, and
    // the 5 Hz full-graph snapshot was built for anyone merely connected. The tally
    // decrements on drop, so a panicking connection task cannot leak a phantom
    // subscriber and keep the daemon serializing state forever.
    let mut subscribed = SubscriberTally {
        daemon: daemon.clone(),
        counted: false,
    };

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
                                let handle = open_taps.remove(pos);
                                // The graph may have dropped the endpoint out from
                                // under this tap (`teardown`/`load --replace`/
                                // `remove-node`, §17). The handle survives the hub's
                                // removal from the daemon's endpoint map, so without
                                // this check the close reports success while having
                                // closed nothing — the silent half of TAP-1.
                                if handle.is_orphaned() {
                                    let endpoint = handle.endpoint();
                                    Response::error(
                                        id,
                                        RpcError::invalid_params(format!(
                                            "tap {tap_id} was already closed by the daemon: its endpoint {endpoint:?} is gone"
                                        )),
                                    )
                                } else {
                                    Response::success(id, json!({ "closed": tap_id }))
                                }
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
                    // The second lane discriminates what it read (CTRL-1):
                    //
                    // * **EOF or a read error** abandons the in-flight verb and closes.
                    //   §15.20's cancel-on-disconnect is normative (a killed
                    //   `lock --wait` client must dequeue promptly) and a bare write-half
                    //   half-close is indistinguishable from a killed client at read
                    //   time, so it is treated as a disconnect. `serialnexusctl` keeps
                    //   both halves open across the read; a raw `socat` waiting-verb user
                    //   must too (CTRL-3: declined on purpose, and still declined).
                    // * **A pipelined request** is *not* a disconnect. It is answered
                    //   with [`WAIT_IN_FLIGHT_MESSAGE`] carrying its own id, and the wait
                    //   stays intact — a §17 browser drives `subscribe`, every tap and
                    //   `send` down one connection, so killing it here silently costs the
                    //   operator the whole session.
                    // * **An over-cap line** keeps the read-path bound (CTRL-1): refuse
                    //   with the null id and close, exactly as the first lane does.
                    //
                    // `next_line` keeps its partial line in `self.buf`, so a request that
                    // straddles a cancelled read is never truncated.
                    let dispatch = daemon.dispatch(&method, params);
                    tokio::pin!(dispatch);
                    let mut settled = None;
                    loop {
                        tokio::select! {
                            biased;
                            result = &mut dispatch => {
                                settled = Some(match result {
                                    Ok(value) => Response::success(id.clone(), value),
                                    Err(err) => Response::error(id.clone(), err),
                                });
                                break;
                            }
                            second = lines.next_line() => {
                                // `close_after` separates "refuse and keep waiting"
                                // (a pipelined or unparseable line) from "refuse and
                                // close" (the read-path bound).
                                let (refusal, close_after) = match second {
                                    Ok(LineRead::Eof) | Err(_) => break, // cancel-on-disconnect
                                    Ok(LineRead::TooLong) => (
                                        Response::error_without_id(RpcError::new(
                                            error_codes::INVALID_REQUEST,
                                            format!(
                                                "request line exceeds the {MAX_REQUEST_LINE}-byte limit"
                                            ),
                                        )),
                                        true,
                                    ),
                                    Ok(LineRead::Line(pipelined)) => {
                                        if pipelined.trim().is_empty() {
                                            continue;
                                        }
                                        // Echo the pipelined request's own id so the
                                        // client correlates the refusal with the request
                                        // it refuses; an unparseable line keeps the
                                        // spec's null id.
                                        let resp = match parse_incoming_request(&pipelined) {
                                            Ok(req) => Response::error(
                                                req.id,
                                                RpcError::new(
                                                    AppError::WaitInFlight.code(),
                                                    WAIT_IN_FLIGHT_MESSAGE,
                                                ),
                                            ),
                                            Err(err) => Response::error_without_id(err),
                                        };
                                        (resp, false)
                                    }
                                };
                                let wrote = write_half
                                    .write_all(nexus_rpc::to_line(&refusal).as_bytes())
                                    .await
                                    .is_ok();
                                if close_after || !wrote {
                                    break;
                                }
                            }
                        }
                    }
                    match settled {
                        Some(response) => response,
                        // The second lane ended the connection; dropping `dispatch`
                        // here runs the waiter's cleanup guard (§15.20).
                        None => break,
                    }
                };

                if write_half
                    .write_all(nexus_rpc::to_line(&response).as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
                if is_subscribe {
                    subscribed.subscribe();
                }
            }
            // Only drain notifications once subscribed; before that the receiver
            // just buffers (and drops-oldest on lag), which we never read.
            note = notes.recv(), if subscribed.counted => match note {
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
            // Tap deliveries for this connection (§17): base64-frame bytes into a
            // `tap.data` notification, or relay the terminal `tap.closed`. Both ride
            // this channel rather than the `subscribe` broadcast because §10 delivers
            // a tap's stream *on that connection*, subscribed or not. `recv` yields
            // `None` only when every sender is gone; the connection holds `tap_tx` for
            // its whole life, so this only pends rather than firing spuriously.
            tap = tap_rx.recv() => if let Some(msg) = tap {
                let note = match msg {
                    TapMsg::Data { tap_id, bytes, offset, gap_before } => Notification::new(
                        "tap.data",
                        Some(json!({
                            "tap": tap_id,
                            // The endpoint hostward offset of this chunk's first byte
                            // (§11.8), so a client splices replay and live exactly.
                            "offset": offset,
                            // Bytes lost at the endpoint's producer→hub feed hop just
                            // before these (§5, TAP-1b). Normally 0; non-zero means a
                            // real hole the contiguous offsets cannot show.
                            "gap_before": gap_before,
                            "data": base64_encode(&bytes),
                        })),
                    ),
                    // The graph dropped this tap's endpoint (TAP-1). Terminal for the
                    // tap, not for the connection: its other taps and its subscription
                    // keep running.
                    TapMsg::Closed { tap_id, endpoint, reason } => Notification::new(
                        "tap.closed",
                        Some(json!({
                            "tap": tap_id,
                            "endpoint": endpoint,
                            "reason": reason,
                        })),
                    ),
                };
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

/// Counts one control connection among the daemon's *subscribers* while it is
/// subscribed (OBS-1). Every connection takes a broadcast receiver at accept, so
/// `broadcast::Sender::receiver_count()` counts connections and the periodic
/// full-graph snapshot was serialized for clients that never asked for it. Drop —
/// including on a panicking connection task — decrements, so the count cannot drift
/// upward and pin the daemon into snapshotting forever.
struct SubscriberTally {
    daemon: Rc<Daemon>,
    counted: bool,
}

impl SubscriberTally {
    /// Count this connection once, however many `subscribe` verbs it issues.
    fn subscribe(&mut self) {
        if !self.counted {
            self.counted = true;
            self.daemon.add_subscriber();
        }
    }
}

impl Drop for SubscriberTally {
    fn drop(&mut self) {
        if self.counted {
            self.daemon.remove_subscriber();
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

    // A pipelined request line must be distinguishable from EOF at the read layer:
    // the cancel-on-disconnect rule (§15.20) applies to `Eof`, and only to `Eof`.
    // Both arrive on the same lane, so the discrimination the CTRL-1 fix rests on is
    // exactly this: `Line` before the half-close, `Eof` after it.
    #[tokio::test]
    async fn a_pipelined_line_is_not_an_eof() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let (read_half, _write_half) = server.into_split();
        let mut lines = RequestLines::new(read_half);

        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"state\"}\n")
            .await
            .unwrap();
        assert!(matches!(
            lines.next_line().await.unwrap(),
            LineRead::Line(_)
        ));
        client.shutdown().await.unwrap();
        assert!(matches!(lines.next_line().await.unwrap(), LineRead::Eof));
    }

    // CTRL-1/CP-1, end to end over a real connection: a request pipelined behind an
    // in-flight *waiting* verb must be answered — with its own id — while the wait
    // stays intact, and the connection must survive. Before the fix both requests
    // went unanswered and the connection was torn down, which for a §17 browser
    // (one daemon connection carrying `subscribe`, every tap and `send`) silently
    // cost the operator the whole session.
    #[tokio::test]
    async fn a_pipelined_request_is_refused_without_killing_the_waiting_verb() {
        use crate::daemon::Daemon;
        use crate::runtime::LockCell;
        use nexus_core::Chunk;
        use nexus_core::graph::WriteMode;
        use nexus_core::lock::{Arbitration, EndpointLock, OriginId};
        use nexus_rpc::{Id, Incoming};
        use tokio::io::AsyncBufReadExt;

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let daemon = Rc::new(Daemon::default());

                // A host endpoint whose lock is already held by another origin, so a
                // `send` against it parks in the FIFO queue (§6/§15.20).
                let (notifier, _keep) = tokio::sync::broadcast::channel(8);
                let lock = Rc::new(LockCell::new(
                    "usb0",
                    EndpointLock::new(Arbitration::Exclusive),
                    notifier,
                ));
                let holder = OriginId(1);
                lock.with_mut(|g| {
                    g.register(holder, "console-a", WriteMode::OnDemand);
                    let _ = g.acquire(holder);
                });
                let (tx, mut targetward_rx) = mpsc::channel::<Chunk>(4);
                daemon.advertise_endpoint_for_test("usb0", lock.clone(), tx);

                let (client, server) = UnixStream::pair().unwrap();
                tokio::task::spawn_local(serve_connection(daemon.clone(), server));

                let (client_read, mut client_write) = client.into_split();
                let mut replies = BufReader::new(client_read).lines();

                // A waiting verb (a generous deadline, so only the grant ends it),
                // immediately followed by a pipelined second request.
                client_write
                    .write_all(
                        b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"send\",\
                          \"params\":{\"endpoint\":\"usb0\",\"line\":\"go\",\"timeout_ms\":30000}}\n\
                          {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"state\"}\n",
                    )
                    .await
                    .unwrap();

                // The pipelined request is answered first, correlated to *its* id.
                let first = replies.next_line().await.unwrap().expect("a reply");
                let Ok(Incoming::Response(resp)) = serde_json::from_str::<Incoming>(&first) else {
                    panic!("expected a response, got {first}");
                };
                assert_eq!(
                    resp.id,
                    Id::Number(2),
                    "the refusal must echo the pipelined id"
                );
                let err = resp.error.expect("the pipelined request is refused");
                assert_eq!(err.code, AppError::WaitInFlight.code());
                assert!(err.message.contains("waiting verb"), "{}", err.message);

                // The wait survived: release the lock and the `send` completes on the
                // same connection.
                let _ = lock.with_mut(|g| g.release(holder));
                lock.wake_waiters();

                let second = replies.next_line().await.unwrap().expect("a second reply");
                let Ok(Incoming::Response(resp)) = serde_json::from_str::<Incoming>(&second) else {
                    panic!("expected a response, got {second}");
                };
                assert_eq!(resp.id, Id::Number(1), "the waiting verb still answers");
                assert!(resp.is_success(), "the send should have been granted");
                assert!(targetward_rx.try_recv().is_ok(), "the line was delivered");
            })
            .await;
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
