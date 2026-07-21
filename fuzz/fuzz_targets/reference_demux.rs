#![no_main]
//! Fuzz the reference codec's demultiplexer (`codec_reference::ReferenceCodec`,
//! §7.5) — the length-guided resync path. Arbitrary bytes, fed in arbitrary chunks,
//! must always RETURN (resync terminates — no infinite loop), never panic, keep the
//! parser state bounded to one frame (§5), and only ever *increase* the framing-error
//! counter. `demux` resyncs rather than erroring, so it never hard-fails on garbage.

use codec_api::{Codec, MAX_FRAME_SIZE};
use codec_reference::ReferenceCodec;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut codec = ReferenceCodec::new();
    let mut prev_errors = codec.framing_errors();
    let mut rest = data;
    while !rest.is_empty() {
        let n = (rest[0] as usize % rest.len()) + 1;
        let (head, tail) = rest.split_at(n);
        // demux resyncs internally and always returns Ok; if it ever hard-errors,
        // that is itself a finding.
        codec
            .demux(head, &mut |_ev| {})
            .expect("demux must not hard-error on arbitrary bytes");
        let now = codec.framing_errors();
        assert!(now >= prev_errors, "framing_errors must be monotonic");
        prev_errors = now;
        assert!(
            codec.buffered() <= MAX_FRAME_SIZE + 4,
            "buffered {} exceeded one frame (interior contract §5)",
            codec.buffered()
        );
        rest = tail;
    }

    // The whole slice in one shot must also terminate and stay bounded.
    let mut whole = ReferenceCodec::new();
    whole
        .demux(data, &mut |_ev| {})
        .expect("demux must not hard-error on arbitrary bytes");
    assert!(whole.buffered() <= MAX_FRAME_SIZE + 4);
});
