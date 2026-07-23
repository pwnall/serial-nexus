#![forbid(unsafe_code)]

//! The **reference framing codec** (design §7.5, §9): the v1 envelope format
//! ([`codec_api::encode`] / [`codec_api::try_decode`]) exposed as a
//! [`Codec`]. It does double duty — the first real demux/remux codec *and* the
//! core of the link codec (§8) — so its on-wire framing is exactly the shared
//! envelope, with no per-frame magic.
//!
//! **Resynchronization (§7.5 state: framing errors / resyncs).** A hardware mux
//! rides a lossy serial line, so `demux` must recover from a corrupted frame
//! rather than wedge. Because the envelope is length-prefixed, recovery is exact
//! and needs no sync marker: on any body-decode error whose `body_len` prefix is
//! intact, the decoder skips exactly that one frame (`4 + body_len` bytes),
//! counts one framing error, and stays aligned on the next frame boundary. The
//! only unrecoverable corruption is a mangled length prefix (`body_len` over the
//! maximum), where the boundary is unknown; the decoder drops the 4-byte prefix
//! and re-scans (best effort). This keeps §8's "one shared frame format" — the
//! link codec over a *reliable* transport (phase 6) simply never hits the resync
//! path, since TCP does not corrupt.

use codec_api::{Codec, CodecError, Event, MAX_FRAME_SIZE, encode, try_decode};

/// This codec's registry name (§8 match-on-name).
pub const NAME: &str = "reference";

/// The v1 framing codec. Holds at most one partial frame in its accumulation
/// buffer, bounded by [`MAX_FRAME_SIZE`] + 4 — one body plus its 4-byte length
/// prefix, which is not counted in `body_len` — the §5 interior contract (parser
/// state plus, at the boundaries, one holdover; a codec holds only parser state).
#[derive(Debug, Default)]
pub struct ReferenceCodec {
    /// Accumulated multiplexed-side bytes awaiting a whole frame.
    buf: Vec<u8>,
    /// Count of frames skipped by resynchronization — surfaced in node state as
    /// framing errors / resyncs (§7.5).
    framing_errors: u64,
}

impl ReferenceCodec {
    pub fn new() -> Self {
        ReferenceCodec::default()
    }

    /// Frames skipped by resynchronization so far (§7.5 counter).
    pub fn framing_errors(&self) -> u64 {
        self.framing_errors
    }

    /// Bytes currently buffered — the bounded partial-frame parser state (§5).
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// On a decode error at the front of `rest`, return how many bytes to skip to
    /// resync past one frame — `Some(skip)` to advance (retry decoding), `None` if
    /// it needs more bytes first. Nothing is drained here: the caller advances a
    /// cursor and counts the resync, so a run of undecodable frames costs one
    /// front-drain per `demux` call, not one per frame (O(n), not O(n^2)).
    ///
    /// A valid `body_len` prefix (`<= MAX_FRAME_SIZE`) means the whole frame is
    /// buffered — [`try_decode`] returns `Ok(None)`, not `Err`, when it is not —
    /// so the frame can be skipped exactly, keeping alignment. An oversize prefix
    /// is itself corrupt with an unknown boundary: drop the 4-byte prefix and
    /// re-scan.
    fn resync_skip(rest: &[u8]) -> Option<usize> {
        if rest.len() < 4 {
            return None; // need the length prefix before we can skip a frame
        }
        let body_len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        if body_len <= MAX_FRAME_SIZE {
            let frame_end = 4 + body_len;
            if rest.len() < frame_end {
                // Unreachable on the error path (try_decode returns None here), but
                // defend against skipping past the buffer.
                return None;
            }
            Some(frame_end)
        } else {
            Some(4) // corrupt length prefix: boundary unknown, drop it and re-scan
        }
    }
}

impl Codec for ReferenceCodec {
    fn name(&self) -> &str {
        NAME
    }

    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError> {
        self.buf.extend_from_slice(input);
        // Advance a cursor over consumed/skipped frames and front-drain the whole
        // consumed prefix once at the end, so a run of undecodable frames costs one
        // O(n) drain rather than one O(remaining) `drain(..)` per frame (§7.5).
        let mut pos = 0;
        loop {
            match try_decode(&self.buf[pos..]) {
                Ok(Some((event, consumed))) => {
                    pos += consumed;
                    emit(event);
                }
                Ok(None) => break, // partial frame: wait for more bytes
                // A malformed frame: resync past it rather than wedge (§7.5).
                Err(_) => {
                    let Some(skip) = Self::resync_skip(&self.buf[pos..]) else {
                        break;
                    };
                    pos += skip;
                    self.framing_errors += 1;
                }
            }
        }
        self.buf.drain(..pos);
        Ok(())
    }

    fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError> {
        encode(event, out)?;
        Ok(())
    }

    fn resync_count(&self) -> u64 {
        self.framing_errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use codec_api::EventKind;
    use proptest::prelude::*;

    /// Collect every event `demux` emits for `input`.
    fn demux_all(codec: &mut ReferenceCodec, input: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        codec.demux(input, &mut |e| out.push(e)).unwrap();
        out
    }

    fn mux_all(events: &[Event]) -> Vec<u8> {
        let mut codec = ReferenceCodec::new();
        let mut wire = Vec::new();
        for e in events {
            codec.mux(e, &mut wire).unwrap();
        }
        wire
    }

    #[test]
    fn mux_then_demux_round_trips() {
        let events = vec![
            Event::open("console"),
            Event::data("console", Bytes::from_static(b"hello \x00\xff world")),
            Event::data("trace", Bytes::from_static(b"trace bytes")),
            Event::error("console", "resync"),
            Event::close("console"),
        ];
        let wire = mux_all(&events);
        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);
        assert_eq!(got, events);
        assert_eq!(codec.framing_errors(), 0);
        assert_eq!(codec.buffered(), 0);
    }

    #[test]
    fn streaming_byte_at_a_time_reassembles() {
        let events = vec![
            Event::data("a", Bytes::from_static(b"12345")),
            Event::data("b", Bytes::from_static(b"67890")),
        ];
        let wire = mux_all(&events);
        let mut codec = ReferenceCodec::new();
        let mut got = Vec::new();
        for b in &wire {
            codec
                .demux(std::slice::from_ref(b), &mut |e| got.push(e))
                .unwrap();
        }
        assert_eq!(got, events);
        assert_eq!(codec.buffered(), 0);
    }

    #[test]
    fn corrupt_type_byte_resyncs_exactly_and_counts() {
        // Three frames; corrupt the middle frame's type byte to an unknown value,
        // keeping its length prefix intact. The decoder must skip exactly that
        // frame (length-prefix-guided) and recover the other two, counting one
        // framing error — the exact, provable recovery the resync test relies on.
        let f0 = Event::data("c0", Bytes::from_static(b"AAAA"));
        let f1 = Event::data("c1", Bytes::from_static(b"BBBB"));
        let f2 = Event::data("c2", Bytes::from_static(b"CCCC"));

        let mut wire = Vec::new();
        let mut enc = ReferenceCodec::new();
        enc.mux(&f0, &mut wire).unwrap();
        let f1_start = wire.len();
        enc.mux(&f1, &mut wire).unwrap();
        enc.mux(&f2, &mut wire).unwrap();

        // The type byte is the first byte of the body, at offset f1_start + 4.
        wire[f1_start + 4] = 0xFF;

        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);
        assert_eq!(
            got,
            vec![f0, f2],
            "the corrupt frame is skipped, others survive"
        );
        assert_eq!(codec.framing_errors(), 1, "exactly one resync counted");
        assert_eq!(codec.buffered(), 0);
    }

    #[test]
    fn corrupt_channel_id_utf8_resyncs() {
        // A channel-id byte flipped to invalid UTF-8 is likewise a body-decode
        // error the length prefix lets us skip exactly.
        let f0 = Event::data("ok", Bytes::from_static(b"1111"));
        let f1 = Event::data("id", Bytes::from_static(b"2222"));
        let mut wire = Vec::new();
        let mut enc = ReferenceCodec::new();
        enc.mux(&f0, &mut wire).unwrap();
        let f1_start = wire.len();
        enc.mux(&f1, &mut wire).unwrap();
        // body: [type u8][chan_len u16][chan bytes...]. Channel id starts at
        // body offset 3 → wire offset f1_start + 4 + 3.
        wire[f1_start + 4 + 3] = 0xFF; // invalid UTF-8 lead byte
        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);
        assert_eq!(got, vec![f0]);
        assert_eq!(codec.framing_errors(), 1);
    }

    #[test]
    fn oversize_length_prefix_resyncs_and_recovers_following_frame() {
        // A mangled length prefix (body_len > MAX_FRAME_SIZE) has an unknown frame
        // boundary, so resync drops only the 4-byte prefix and re-scans (the
        // oversize `else { 4 }` branch). Prepend one such prefix ahead of a valid
        // frame: demux must drop exactly those 4 bytes, count one framing error,
        // and re-align on the following frame.
        let good = Event::data("c0", Bytes::from_static(b"payload"));
        let valid = mux_all(std::slice::from_ref(&good));

        let mut wire = ((MAX_FRAME_SIZE + 1) as u32).to_be_bytes().to_vec();
        wire.extend_from_slice(&valid);

        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);
        assert_eq!(
            got,
            vec![good],
            "the frame after the mangled prefix survives"
        );
        assert_eq!(codec.framing_errors(), 1, "one 4-byte-prefix drop counted");
        assert_eq!(codec.buffered(), 0);
    }

    #[test]
    fn truncated_header_length_prefix_resyncs_and_recovers() {
        // A body_len that is <= MAX_FRAME_SIZE but structurally impossible (a whole
        // frame needs a 3-byte header: type + u16 channel_len) makes try_decode
        // return Err(Truncated) once 4 + body_len bytes are present. resync skips
        // exactly 4 + body_len and re-aligns. Use body_len = 2: a 6-byte runt frame
        // ahead of a valid one.
        let good = Event::data("c0", Bytes::from_static(b"payload"));
        let valid = mux_all(std::slice::from_ref(&good));

        // 4-byte length prefix declaring body_len = 2, then 2 filler body bytes.
        let mut wire = 2u32.to_be_bytes().to_vec();
        wire.extend_from_slice(&[0x00, 0x00]);
        wire.extend_from_slice(&valid);

        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);
        assert_eq!(got, vec![good], "the frame after the runt header survives");
        assert_eq!(
            codec.framing_errors(),
            1,
            "one runt frame (4 + body_len) skipped"
        );
        assert_eq!(codec.buffered(), 0);
    }

    /// The reference codec satisfies the generic `codec-api` conformance kit
    /// (§15.26 / plan §10.4) — the same suites an out-of-tree codec runs from the
    /// consumer position. This is the reference implementation exercising the kit
    /// it must be honest against; the bespoke resync/streaming tests above cover
    /// what the generic kit deliberately cannot (exact resync accounting).
    #[test]
    fn satisfies_the_conformance_kit() {
        use codec_api::test_support as kit;
        let channels = ["console", "trace", "ctrl"];
        kit::round_trip_identity(ReferenceCodec::new, &channels);
        kit::fragmentation_tolerance(ReferenceCodec::new, "console");
        kit::handles_garbage(ReferenceCodec::new, "console");
        kit::bounded_parser_state(ReferenceCodec::new);
        // The reference codec exposes its accumulation buffer, so it can also prove
        // the property the trait-only suite cannot see: length-guided resync keeps
        // the buffer within one frame even on undecodable input (§5).
        kit::assert_buffer_bounded(ReferenceCodec::new, ReferenceCodec::buffered);
    }

    proptest! {
        /// Any sequence of events survives mux→demux unchanged, with no spurious
        /// framing errors and nothing left buffered.
        #[test]
        fn prop_mux_demux_identity(
            payloads in prop::collection::vec(
                (proptest::string::string_regex("[a-z0-9]{1,6}").unwrap(),
                 prop::collection::vec(any::<u8>(), 0..40)),
                0..32),
        ) {
            let events: Vec<Event> = payloads
                .into_iter()
                .map(|(chan, bytes)| Event {
                    channel: chan.as_str().into(),
                    kind: EventKind::Data(Bytes::from(bytes)),
                })
                .collect();
            let wire = mux_all(&events);
            let mut codec = ReferenceCodec::new();
            let got = demux_all(&mut codec, &wire);
            prop_assert_eq!(got, events);
            prop_assert_eq!(codec.framing_errors(), 0);
            prop_assert_eq!(codec.buffered(), 0);
        }
    }
}
