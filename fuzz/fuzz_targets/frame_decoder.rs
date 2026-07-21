#![no_main]
//! Fuzz the streaming envelope decoder (`codec_api::FrameDecoder`) — the
//! partial-frame reassembly path a single `try_decode` call does not exercise.
//! Feeding arbitrary bytes in arbitrary chunks must never panic and the drain loop
//! must always terminate. `FrameDecoder` is the link codec: it never resyncs, so an
//! `Err` is terminal (a real leg drops the connection); we end the run there.

use codec_api::{FrameDecoder, MAX_FRAME_SIZE};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut dec = FrameDecoder::new();
    let mut rest = data;
    while !rest.is_empty() {
        // Use the leading byte to pick a chunk size in 1..=rest.len().
        let n = (rest[0] as usize % rest.len()) + 1;
        let (head, tail) = rest.split_at(n);
        dec.push(head);
        // Drain every whole frame; stop on a partial (Ok(None)) or, terminally, on
        // a clean refusal (Err — an oversize/malformed prefix the link never resyncs).
        let mut drained = 0usize;
        loop {
            match dec.next_event() {
                Ok(Some(_)) => {
                    drained += 1;
                    assert!(drained < 8_000_000, "drain loop failed to terminate");
                }
                Ok(None) => break,
                // Terminal: further pushes cannot make progress, so end the run.
                // Before the terminal error, only an incomplete frame is buffered.
                Err(_) => return,
            }
        }
        // On the no-error path a partial trailing frame is bounded by §5's contract.
        assert!(
            dec.buffered() <= MAX_FRAME_SIZE + 4,
            "buffered {} exceeded one frame",
            dec.buffered()
        );
        rest = tail;
    }
});
