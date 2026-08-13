#![forbid(unsafe_code)]

//! The **reference framing codec** (design §7.5, §9): the v1 envelope format
//! ([`serial_nexus_codec_api::encode`] / [`serial_nexus_codec_api::try_decode`]) exposed as a
//! [`Codec`]. It does double duty — the first real demux/remux codec *and* the
//! core of the link codec (§8) — so its on-wire framing is exactly the shared
//! envelope, with no per-frame magic.
//!
//! **Resynchronization (§7.5 state: framing errors / resyncs).** A hardware mux
//! rides a lossy serial line, so `demux` must recover from a corrupted frame
//! rather than wedge. Recovery is exact and needs no sync marker **exactly when
//! the length prefix survives**: on any body-decode error whose `body_len` prefix
//! is intact, the decoder skips exactly that one frame (`4 + body_len` bytes),
//! counts one framing error, and stays aligned on the next frame boundary.
//!
//! A *mangled length prefix* is the unrecoverable class, and it has two shapes —
//! the second of which is worse than the first and is silent:
//!
//! - `body_len` **over** [`MAX_FRAME_SIZE`]: the boundary is unknown, so the
//!   decoder drops the 4-byte prefix, counts a framing error, and re-scans (best
//!   effort). Loss is bounded and visible in `framing_errors`.
//! - `body_len` **under** the maximum: [`try_decode`] succeeds on the *phantom*
//!   boundary, so `demux` emits a single `data` event on the corrupt frame's own
//!   (configured, legitimate) channel whose payload is the raw framing bytes of
//!   the frames that followed — and every real frame inside that span disappears
//!   with **no framing error attributable to the merge**. The daemon does not
//!   merely pass those bytes on: it mirrors them to the channel's replay ring,
//!   broadcasts them to that channel's sinks, and counts them as
//!   `delivered_hostward`. Detecting this needs a per-frame integrity field,
//!   which §9 deliberately does not specify for v1 — so it is a recorded trade,
//!   not an oversight, pinned by
//!   `merged_frames_when_the_length_prefix_is_mangled_under_max` so it stays a
//!   fact rather than a surprise.
//!
//! This keeps §8's "one shared frame format" — the link codec over a *reliable*
//! transport (phase 6) simply never hits the resync path, since TCP does not
//! corrupt.

use serial_nexus_codec_api::{Codec, CodecError, Event, MAX_FRAME_SIZE, encode, try_decode};

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
    use proptest::prelude::*;
    use serial_nexus_codec_api::EventKind;

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
    fn merged_frames_when_the_length_prefix_is_mangled_under_max() {
        // Review 32 WIRE-3. The over-max prefix above is loud: one dropped prefix,
        // one counted framing error. A prefix corrupted to a value that is still
        // <= MAX_FRAME_SIZE is the opposite — `try_decode` accepts the phantom
        // boundary, so the frames it swallows come back out as one frame's payload
        // on a legitimate channel and nothing counts the merge. This test does not
        // assert desirable behaviour; it pins the v1 trade §9 makes by specifying
        // no per-frame integrity field, so the module doc above stays true.
        let head = Event::data("c0", Bytes::from(vec![b'A'; 200]));
        let tail = Event::data("c2", Bytes::from_static(b"tail"));

        let mut wire = Vec::new();
        let mut enc = ReferenceCodec::new();
        enc.mux(&head, &mut wire).unwrap();
        let head_frame_len = wire.len();
        for i in 0..4u8 {
            enc.mux(
                &Event::data("c1", Bytes::from(vec![b'0' + i; 200])),
                &mut wire,
            )
            .unwrap();
        }
        enc.mux(&tail, &mut wire).unwrap();
        let tail_frame_len = mux_all(std::slice::from_ref(&tail)).len();

        // Grow the FIRST frame's length prefix so its phantom body ends exactly on
        // the tail frame's boundary — the realistic single-bit shape, and the one
        // that shows the re-alignment is *accidental* rather than earned.
        let swallowed = wire.len() - head_frame_len - tail_frame_len;
        let mangled = (head_frame_len - 4 + swallowed) as u32;
        assert!(mangled as usize <= MAX_FRAME_SIZE, "still a legal prefix");
        wire[0..4].copy_from_slice(&mangled.to_be_bytes());

        let mut codec = ReferenceCodec::new();
        let got = demux_all(&mut codec, &wire);

        assert_eq!(
            got.len(),
            2,
            "the merged frame and the tail: the four c1 frames vanished"
        );
        assert_eq!(got[0].channel.as_str(), "c0", "on a legitimate channel");
        let EventKind::Data(payload) = &got[0].kind else {
            panic!("expected a data event, got {:?}", got[0].kind);
        };
        assert_eq!(
            payload.len(),
            200 + swallowed,
            "the payload is the real 200 bytes plus the raw framing of what followed"
        );
        assert_eq!(&payload[..200], &[b'A'; 200][..]);
        assert_eq!(got[1], tail, "re-alignment here is luck, not recovery");
        assert_eq!(
            codec.framing_errors(),
            0,
            "nothing counts the merge — that is exactly what the module doc must say"
        );
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

    /// The reference codec satisfies the generic `serial-nexus-codec-api` conformance kit
    /// (§15.26 / plan §10.4) — the same suites an out-of-tree codec runs from the
    /// consumer position. This is the reference implementation exercising the kit
    /// it must be honest against; the bespoke resync/streaming tests above cover
    /// what the generic kit deliberately cannot (exact resync accounting).
    #[test]
    fn satisfies_the_conformance_kit() {
        use serial_nexus_codec_api::test_support as kit;
        let channels = ["console", "trace", "ctrl"];
        kit::round_trip_identity(ReferenceCodec::new, &channels);
        kit::fragmentation_tolerance(ReferenceCodec::new, "console");
        kit::handles_garbage(ReferenceCodec::new, "console");
        kit::bounded_parser_state(ReferenceCodec::new);
        // The reference codec exposes its accumulation buffer, so it can also prove
        // the property the trait-only suite cannot see: length-guided resync keeps
        // the buffer within one frame even on undecodable input (§5).
        kit::assert_buffer_bounded(ReferenceCodec::new, ReferenceCodec::buffered);
        // It resyncs by length guidance (§8 clause 9), so it also owes the opt-in
        // Err-then-Ok recovery suite: one refused frame is skipped whole and the
        // next decodes — clause 6's non-latching contract, codec side. A codec on a
        // reliable transport (the link codec) legitimately does not call this.
        kit::recovers_after_garbage(ReferenceCodec::new, "console");
        // …and, resyncing, it owes the accounting too: `resync_count` is what the
        // daemon reports as this node's `framing_errors` (§7.5). The malformed unit
        // is this codec's own — one frame whose type byte is not one of the four §8
        // kinds, with its length prefix intact, which length-guided resync skips
        // whole for exactly one count. `corrupt_type_byte_resyncs_exactly_and_counts`
        // above proves the *recovery*; the kit suite proves the counter is honest and
        // is not consumed by the read (`state` reads it once per poll).
        let mut malformed = mux_all(&[Event::data("console", Bytes::from_static(b"unknown kind"))]);
        malformed[4] = 0x7f; // the body's type byte
        kit::resync_is_counted(ReferenceCodec::new, &malformed, 1);
        // The interior fragments a targetward write on the shared boundary and hands
        // this codec one piece at a time; its own framing unit is that same envelope,
        // so the largest piece is exactly one maximal frame (§15.24/§15.27).
        kit::targetward_fragmentation_is_lossless(ReferenceCodec::new, "console");
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
