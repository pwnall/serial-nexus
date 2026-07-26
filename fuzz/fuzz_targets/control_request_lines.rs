#![no_main]
//! The daemon's control-socket **line framer** — `RequestLines` (review 26, SEC-7).
//!
//! This is the daemon's front door. It frames every byte a client writes to the 0600
//! control socket, *before* any JSON is parsed and before any verb is dispatched, and
//! it is the one place a hostile client controls both the content and the chunking.
//! `rpc_request_line` fuzzes what happens to a line once it has been framed; this
//! fuzzes the framing itself.
//!
//! Reached through `nexus_daemon::unstable_fuzz_api`, which exists for exactly this
//! and promises nothing (see that module's docs and implementation-notes §3.19).
//!
//! Two things make the framer worth a target rather than only unit tests. It carries a
//! hard per-line cap that must hold no matter how the bytes arrive, and it is
//! **cancel-safe**: §15.20's two-lane control plane drops the read future mid-line
//! whenever a waiting verb resolves first, and a partial line has to survive that or a
//! pipelined request is silently truncated. Both properties are about *arrival
//! patterns*, which is precisely what a fuzzer explores and a unit test enumerates.

use libfuzzer_sys::fuzz_target;
use nexus_daemon::unstable_fuzz_api::{LineRead, MAX_REQUEST_LINE, RequestLines};

/// A byte source that hands out at most `chunk` bytes per `poll_read`, so the fuzzer
/// controls *how the input is split across reads*, not just its content. A framer that
/// only works when a line arrives in one piece is a broken framer; the socket makes no
/// such promise.
struct Chunked<'a> {
    bytes: &'a [u8],
    chunk: usize,
}

impl tokio::io::AsyncRead for Chunked<'_> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let n = self.chunk.min(self.bytes.len()).min(buf.remaining());
        buf.put_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        std::task::Poll::Ready(Ok(()))
    }
}

fuzz_target!(|data: &[u8]| {
    // First byte picks the arrival granularity (1..=64 bytes per read); the rest is
    // the stream. A one-byte-at-a-time source is the adversarial case for any framer
    // that assumes a read returns a whole line.
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let chunk = (first % 64) as usize + 1;

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("current-thread runtime");
    rt.block_on(async {
        let mut lines = RequestLines::new(Chunked {
            bytes: rest,
            chunk,
        });
        let mut consumed = 0usize;
        loop {
            match lines.next_line().await {
                Ok(LineRead::Line(line)) => {
                    // The cap is the whole point of this type existing rather than
                    // `tokio::io::Lines`, whose accumulator grows until it sees a
                    // newline. One connection must not be able to grow the shared
                    // daemon's read buffer without bound.
                    assert!(
                        line.len() <= MAX_REQUEST_LINE,
                        "a framed line exceeded MAX_REQUEST_LINE: {} bytes",
                        line.len()
                    );
                    // Framing is by `\n`: a framed line may never contain one, or the
                    // daemon would dispatch two requests as one.
                    assert!(!line.contains('\n'), "a framed line kept its newline");
                    // NOT asserted: that the line is free of `\r`. `take_line` strips
                    // exactly one trailing CR (matching `tokio::io::Lines`), so an
                    // unterminated final line of `\r\r` before EOF comes out as `\r`.
                    // The first version of this target asserted otherwise and the
                    // fuzzer refuted it in seconds — correctly: CR is JSON whitespace,
                    // and `parse_incoming_request` below is the only consumer, so the
                    // residue is invisible. The assertion was wrong, not the framer.
                    //
                    // What is worth asserting is that the two stages *compose*: every
                    // line this framer emits is something the request parser can be
                    // handed without panicking, whatever the bytes were. Neither
                    // `rpc_request_line` (which starts from a whole line) nor the unit
                    // tests (which start from known-good framing) cover that seam.
                    let _ = nexus_rpc::parse_incoming_request(&line);
                    consumed += line.len();
                }
                // An over-cap line is refused, not truncated into a short line that
                // would then be parsed as if the client had sent it.
                Ok(LineRead::TooLong) => {}
                // EOF and invalid UTF-8 both terminate the connection.
                Ok(LineRead::Eof) | Err(_) => break,
            }
            // Termination: every iteration must make progress on a finite input, so
            // the loop cannot spin. `consumed` is a cheap witness that it does not
            // exceed what was fed in.
            assert!(
                consumed <= rest.len(),
                "framer produced more bytes than it was given"
            );
        }
    });
});
