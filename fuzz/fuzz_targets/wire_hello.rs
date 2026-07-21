#![no_main]
//! Fuzz the wire handshake decoder (`codec_api::try_decode_hello`, §9). Arbitrary
//! bytes must refuse cleanly (bad magic, unsupported version, oversize, truncated,
//! bad channel id) and never panic. Byte-identity does NOT hold (the decoder ignores
//! trailing bytes within `body_len` beyond the announced count), so we assert
//! decode -> encode -> decode STABILITY instead.

use codec_api::{WIRE_VERSION, encode_hello, try_decode_hello};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(Some((hello, consumed))) = try_decode_hello(data) {
        assert!(
            consumed >= 4 && consumed <= data.len(),
            "consumed {consumed} out of range for {}-byte input",
            data.len()
        );
        // A decoded hello always speaks this daemon's version and its announced
        // channel count matches the vector length (a mismatch would be a parse bug).
        assert_eq!(hello.version, WIRE_VERSION, "decoded a foreign wire version");
        let mut out = Vec::new();
        encode_hello(&hello, &mut out).expect("a decoded hello must re-encode");
        match try_decode_hello(&out) {
            Ok(Some((hello2, consumed2))) => {
                assert_eq!(consumed2, out.len(), "re-encode length drifted");
                assert_eq!(hello2, hello, "re-decode differed from the original hello");
            }
            other => panic!("re-encoded hello did not decode cleanly: {other:?}"),
        }
    }
});
