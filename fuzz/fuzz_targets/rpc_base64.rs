#![no_main]
//! Fuzz `serial_nexus_rpc::base64_decode`/`base64_encode` — the dependency-free codec that
//! carries arbitrary console bytes inside a JSON string (`tap.data`, §10/§17).
//!
//! Added for review 26 **SEC-7**. It is a hand-rolled parser (§13's permissive-only
//! policy is why it is hand-rolled rather than a crate), it is fed *attacker-influenced
//! text* on the decode side — anything a client sends, and anything a browser or a
//! `serial-nexus-ctl tap` reads back — and its arithmetic has the exact shape that hides
//! off-by-ones: 6-bit accumulation, a `4 - group.len()` shift, and a padding rule.
//!
//! Two invariants, one per direction:
//!
//! * **encode is lossless** — `decode(encode(x)) == x` for arbitrary bytes, so console
//!   bytes survive the trip through JSON. This is the property `tap.data`'s byte-exact
//!   SHA-256 assertions in `serial-nexus-itest` depend on.
//! * **decode is total and canonical** — arbitrary text either refuses (`None`) or
//!   yields bytes that re-encode to a form decoding to the same bytes. No panic, no
//!   slice out of range, and no decoded output longer than its input.

use libfuzzer_sys::fuzz_target;
use serial_nexus_rpc::{base64_decode, base64_encode};

fuzz_target!(|data: &[u8]| {
    // Direction 1: every byte string survives the round trip exactly.
    let encoded = base64_encode(data);
    assert!(
        encoded.is_ascii(),
        "base64_encode emitted non-ASCII, which cannot ride in a JSON string"
    );
    assert_eq!(
        base64_decode(&encoded).as_deref(),
        Some(data),
        "encode->decode was not the identity"
    );

    // Direction 2: arbitrary text is decoded or refused, never mishandled. (Nested
    // rather than a let-chain: this crate is edition 2021, unlike the workspace.)
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(bytes) = base64_decode(text) {
        assert!(
            bytes.len() <= text.len(),
            "decoded {} bytes from a {}-byte input",
            bytes.len(),
            text.len()
        );
        // A decode's output is canonical: re-encoding it and decoding again is the
        // same bytes (the input may have carried whitespace or padding slack).
        let canonical = base64_encode(&bytes);
        assert_eq!(
            base64_decode(&canonical).as_deref(),
            Some(bytes.as_slice()),
            "a decoded payload did not survive re-encoding"
        );
    }
});
