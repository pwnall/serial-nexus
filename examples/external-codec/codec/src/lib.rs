#![forbid(unsafe_code)]

//! `acme-codec` — a trivial out-of-tree codec, standing in for a closed-source
//! protocol crate (§8/§15.26).
//!
//! It depends only on `codec-api` — never on the daemon — so it could live in a
//! separate repository under a different license and pin the open core by version
//! tag. This one is a single-channel passthrough: every multiplexed byte is data
//! on the [`CHANNEL`] channel, and channel data is written straight through. That
//! is enough to demonstrate the embedding pattern (a real codec would frame N
//! channels with a device-specific protocol); correctness of a real protocol is
//! the codec author's job, tested with the `codec-api` conformance kit (§15.26).

use codec_api::{Codec, CodecError, Event, EventKind};

/// The one channel this demo codec exposes. A real mux codec would expose several.
pub const CHANNEL: &str = "console";

/// A single-channel passthrough codec named `acme`.
#[derive(Default)]
pub struct AcmeCodec;

impl AcmeCodec {
    pub fn new() -> Self {
        AcmeCodec
    }
}

impl Codec for AcmeCodec {
    fn name(&self) -> &str {
        "acme"
    }

    /// All multiplexed bytes are data on [`CHANNEL`]. Holds no parser state, so it
    /// trivially satisfies the §5 interior bound.
    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError> {
        if !input.is_empty() {
            emit(Event::data(CHANNEL, input.to_vec()));
        }
        Ok(())
    }

    /// Channel data is written straight through; non-data events (open/close/error)
    /// carry no bytes for a passthrough and are dropped.
    fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if let EventKind::Data(bytes) = &event.kind {
            out.extend_from_slice(bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_channel_data() {
        let mut codec = AcmeCodec::new();
        let mut framed = Vec::new();
        codec
            .mux(&Event::data(CHANNEL, b"hello".to_vec()), &mut framed)
            .unwrap();
        let mut got = Vec::new();
        codec
            .demux(&framed, &mut |ev| {
                if let EventKind::Data(b) = ev.kind {
                    got.extend_from_slice(&b);
                }
            })
            .unwrap();
        assert_eq!(got, b"hello");
    }
}

// A closed-source codec proves conformance from its own tests using the shared
// kit (§15.26 / plan §10.4): `cargo test --features conformance`. The passthrough
// serves exactly one channel, so the kit is instantiated with just that channel.
#[cfg(all(test, feature = "conformance"))]
mod conformance {
    use super::*;
    use codec_api::test_support as kit;

    #[test]
    fn acme_conforms_for_its_channel() {
        kit::round_trip_identity(AcmeCodec::new, &[CHANNEL]);
        kit::fragmentation_tolerance(AcmeCodec::new, CHANNEL);
        kit::handles_garbage(AcmeCodec::new, CHANNEL);
        kit::bounded_parser_state(AcmeCodec::new);
    }
}
