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
/// buffer, bounded by [`MAX_FRAME_SIZE`] — the §5 interior contract (parser state
/// plus, at the boundaries, one holdover; a codec holds only parser state).
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

    /// On a decode error, skip one frame and count a resync. Returns `true` if it
    /// advanced (retry decoding), `false` if it needs more bytes first.
    ///
    /// A valid `body_len` prefix (`<= MAX_FRAME_SIZE`) means the whole frame is
    /// buffered — [`try_decode`] returns `Ok(None)`, not `Err`, when it is not —
    /// so the frame can be skipped exactly, keeping alignment. An oversize prefix
    /// is itself corrupt with an unknown boundary: drop the 4-byte prefix and
    /// re-scan.
    fn resync(&mut self) -> bool {
        if self.buf.len() < 4 {
            return false; // need the length prefix before we can skip a frame
        }
        let body_len =
            u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        let skip = if body_len <= MAX_FRAME_SIZE {
            let frame_end = 4 + body_len;
            if self.buf.len() < frame_end {
                // Unreachable on the error path (try_decode returns None here), but
                // defend against skipping past the buffer.
                return false;
            }
            frame_end
        } else {
            4 // corrupt length prefix: boundary unknown, drop it and re-scan
        };
        self.buf.drain(..skip);
        self.framing_errors += 1;
        true
    }
}

impl Codec for ReferenceCodec {
    fn name(&self) -> &str {
        NAME
    }

    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError> {
        self.buf.extend_from_slice(input);
        loop {
            match try_decode(&self.buf) {
                Ok(Some((event, consumed))) => {
                    self.buf.drain(..consumed);
                    emit(event);
                }
                Ok(None) => break, // partial frame: wait for more bytes
                // A malformed frame: resync past it rather than wedge (§7.5).
                Err(_) => {
                    if !self.resync() {
                        break;
                    }
                }
            }
        }
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
