#![no_main]
//! Fuzz the envelope frame decoder (`codec_api::try_decode`) — the exec-codec
//! stdin/stdout contract and the per-event unit on the wire (§8, §15.15). Arbitrary
//! bytes must never panic; a decoded frame must re-encode byte-identically and
//! round-trip, because the payload consumes all remaining body bytes (no slack).

use codec_api::{EventKind, encode, try_decode};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Ok(None) = a strict prefix (need more), Err = a clean refusal (oversize,
    // unknown type, bad UTF-8) — both must not panic. Only Ok(Some) has invariants.
    if let Ok(Some((event, consumed))) = try_decode(data) {
        assert!(
            consumed >= 4 && consumed <= data.len(),
            "consumed {consumed} out of range for {}-byte input",
            data.len()
        );
        let mut out = Vec::new();
        encode(&event, &mut out).expect("a decoded event must re-encode");
        // Byte-identity holds only for Data/Error, whose payload consumes ALL
        // remaining body bytes. Open/Close ignore any trailing body bytes within
        // body_len, so a decoded Open/Close legitimately re-encodes shorter — for
        // those we rely on the decode->encode->decode STABILITY check below (the
        // wire_hello pattern), not byte-identity.
        if matches!(&event.kind, EventKind::Data(_) | EventKind::Error(_)) {
            assert_eq!(
                out.as_slice(),
                &data[..consumed],
                "decode->encode was not byte-identical"
            );
        }
        // ...and decoding the re-encoding is stable.
        match try_decode(&out) {
            Ok(Some((event2, consumed2))) => {
                assert_eq!(consumed2, out.len(), "re-encode length drifted");
                assert_eq!(event2, event, "re-decode differed from the original event");
            }
            other => panic!("re-encoded frame did not decode cleanly: {other:?}"),
        }
    }
});
