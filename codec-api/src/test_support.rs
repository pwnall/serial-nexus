//! The codec conformance kit (§15.26 / plan §10.4).
//!
//! Generic suites any [`Codec`] implementation can run in its own crate's tests —
//! including a *closed-source* one, which is the point: the codec author proves
//! conformance from the consumer position without forking serial_nexus. Enable the
//! `test-support` feature (a dev-dependency) and call these from `#[cfg(test)]`:
//!
//! ```ignore
//! use codec_api::test_support as kit;
//! #[test]
//! fn conforms() {
//!     kit::round_trip_identity(MyCodec::new, &["console", "trace"]);
//!     kit::fragmentation_tolerance(MyCodec::new, "console");
//!     kit::handles_garbage(MyCodec::new, "console");
//!     kit::bounded_parser_state(MyCodec::new);
//! }
//! ```
//!
//! Each suite takes a **factory** (`Fn() -> C`) because `demux`/`mux` are stateful
//! (`&mut self`); the suite builds fresh instances. A suite that fails **panics**
//! (an `assert!`), so a broken codec fails its `#[test]` — that is the mechanism
//! the negative self-tests below rely on. The suites are deliberately dependency-
//! free (a small LCG stands in for a PRNG) so a consumer inherits no extra crates.

use crate::{Codec, Event, EventKind, MAX_FRAME_SIZE};

/// Deterministic pseudo-random bytes (a splitmix/LCG hybrid; no `rand` dependency),
/// so a conformance failure reproduces exactly. Covers the full `0..=255` range,
/// including the `0x00`/`0xFF` bytes that trip naive length/type parsing.
pub fn seeded_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        // LCG (Knuth MMIX constants), take a high byte for better bit mixing.
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        out.push((state >> 33) as u8);
    }
    out
}

/// Fold a per-event `(channel, bytes)` stream into channel → concatenated bytes,
/// so a codec that legitimately splits one payload across several `data` events
/// still compares equal.
fn fold_channels(pairs: Vec<(String, Vec<u8>)>) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut map: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    for (channel, bytes) in pairs {
        map.entry(channel).or_default().extend_from_slice(&bytes);
    }
    map
}

/// Drain every `data` event `demux` emits for `input`, as `(channel, bytes)`. The
/// call must return (`Ok`/`Err`); a suite decides whether an `Err` is acceptable.
fn demux_data<C: Codec>(codec: &mut C, input: &[u8]) -> (Vec<(String, Vec<u8>)>, bool) {
    let mut out = Vec::new();
    let ok = codec
        .demux(input, &mut |event| {
            if let EventKind::Data(bytes) = event.kind {
                out.push((event.channel.to_string(), bytes.to_vec()));
            }
        })
        .is_ok();
    (out, ok)
}

/// **Round-trip identity.** `mux` one `data` event per channel, `demux` the framed
/// bytes back, and assert the reconstructed per-channel bytes are identical. A
/// codec that drops, reorders, misroutes, or corrupts bytes fails here.
///
/// `channels` is the set of identities the codec claims to serve — a codec that
/// only serves one channel (a passthrough) is tested with that one channel.
pub fn round_trip_identity<C: Codec>(make: impl Fn() -> C, channels: &[&str]) {
    assert!(!channels.is_empty(), "give the kit at least one channel");
    let mut enc = make();
    let mut wire = Vec::new();
    let mut expected: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, channel) in channels.iter().enumerate() {
        let payload = seeded_bytes(i as u64 + 1, 300);
        enc.mux(&Event::data(*channel, payload.clone()), &mut wire)
            .expect("mux of a data event must succeed");
        expected.push(((*channel).to_owned(), payload));
    }
    let mut dec = make();
    let (got, ok) = demux_data(&mut dec, &wire);
    assert!(ok, "demux of the codec's own framing returned an error");
    assert_eq!(
        fold_channels(got),
        fold_channels(expected),
        "round-trip identity failed: demux did not reconstruct what mux framed"
    );
}

/// **Control-event round-trip.** `mux` an `open → data → close → error` sequence
/// per channel, `demux` the framed bytes, and assert every event survives with its
/// channel and kind intact — the *full* §8 vocabulary, not just `data`. Runs of
/// `data` events are concatenated so a codec that legitimately splits one payload
/// still compares equal, but a codec that drops, reorders, misroutes, or corrupts
/// an `open`/`close`/`error` fails here — the gap [`round_trip_identity`] cannot see.
///
/// **Opt-in, unlike the four universal suites.** Only a codec whose `demux`
/// *surfaces* control events runs this: a passthrough codec that carries opaque
/// bytes and drops control events legitimately cannot, and must not, be tested with
/// it. Call it from `#[cfg(test)]` only for a codec that transports the vocabulary:
///
/// ```ignore
/// kit::control_event_round_trip(MyCodec::new, &["console", "trace"]);
/// ```
pub fn control_event_round_trip<C: Codec>(make: impl Fn() -> C, channels: &[&str]) {
    assert!(!channels.is_empty(), "give the kit at least one channel");
    let mut enc = make();
    let mut wire = Vec::new();
    let mut expected: std::collections::BTreeMap<String, Vec<EventKind>> =
        std::collections::BTreeMap::new();
    for (i, channel) in channels.iter().enumerate() {
        let payload = seeded_bytes(i as u64 + 1, 64);
        let reason = format!("reason-{i}");
        for ev in [
            Event::open(*channel),
            Event::data(*channel, payload.clone()),
            Event::close(*channel),
            Event::error(*channel, reason.clone()),
        ] {
            enc.mux(&ev, &mut wire)
                .expect("mux of a control event must succeed");
        }
        expected.insert(
            (*channel).to_owned(),
            vec![
                EventKind::Open,
                EventKind::Data(payload.into()),
                EventKind::Close,
                EventKind::Error(reason),
            ],
        );
    }

    let mut dec = make();
    let mut got: std::collections::BTreeMap<String, Vec<EventKind>> =
        std::collections::BTreeMap::new();
    dec.demux(&wire, &mut |event| {
        got.entry(event.channel.to_string())
            .or_default()
            .push(event.kind);
    })
    .expect("demux of the codec's own framing must succeed");

    // Fold consecutive `data` events so a codec that splits one payload across
    // several frames still matches; control events are left in order.
    for kinds in got.values_mut() {
        let mut folded: Vec<EventKind> = Vec::new();
        for kind in kinds.drain(..) {
            if let (Some(EventKind::Data(acc)), EventKind::Data(bytes)) = (folded.last_mut(), &kind)
            {
                let mut merged = acc.to_vec();
                merged.extend_from_slice(bytes);
                *acc = merged.into();
            } else {
                folded.push(kind);
            }
        }
        *kinds = folded;
    }

    assert_eq!(
        got, expected,
        "control-event round-trip failed: an open/data/close/error event was \
         dropped, misrouted, or corrupted"
    );
}

/// **Fragmentation tolerance.** Frame a payload, then feed the framed bytes to
/// `demux` one byte at a time, asserting the reassembled channel data still
/// matches. A codec that assumes a whole frame arrives per `demux` call fails here
/// — exactly the §5/§9 fragmentation the interior must tolerate.
pub fn fragmentation_tolerance<C: Codec>(make: impl Fn() -> C, channel: &str) {
    let payload = seeded_bytes(42, 5000);
    let mut enc = make();
    let mut wire = Vec::new();
    enc.mux(&Event::data(channel, payload.clone()), &mut wire)
        .expect("mux of a data event must succeed");

    let mut dec = make();
    let mut got: Vec<u8> = Vec::new();
    for byte in &wire {
        dec.demux(std::slice::from_ref(byte), &mut |event| {
            if event.channel.as_str() == channel {
                if let EventKind::Data(bytes) = event.kind {
                    got.extend_from_slice(&bytes);
                }
            }
        })
        .expect("demux of a single byte must succeed");
    }
    assert_eq!(
        got, payload,
        "byte-at-a-time reassembly lost or corrupted data"
    );
}

/// **Garbage tolerance / resync termination.** Feed pseudo-random garbage and
/// assert `demux` *returns* (no panic, no hang) — the codec must resynchronize or
/// cleanly error, never crash or loop. Then feed a valid frame and assert the
/// codec is still callable. A codec that panics on unexpected bytes fails here.
///
/// This intentionally does *not* assert recovery of the post-garbage frame's
/// content: a resyncing codec recovers, but a codec over a reliable transport
/// legitimately never resyncs — the kit serves both.
pub fn handles_garbage<C: Codec>(make: impl Fn() -> C, channel: &str) {
    let garbage = seeded_bytes(7, 4096);
    let mut dec = make();
    // Must return without panicking; Err is acceptable (unframed noise).
    let _ = demux_data(&mut dec, &garbage);

    // The codec must remain callable afterward.
    let mut enc = make();
    let mut frame = Vec::new();
    enc.mux(&Event::data(channel, b"recover".to_vec()), &mut frame)
        .expect("mux must succeed");
    let _ = demux_data(&mut dec, &frame);
}

/// **Bounded work and no output amplification.** Feed a blob larger than
/// [`MAX_FRAME_SIZE`] in chunks and assert two invariants: `demux` returns each
/// time (bounded work, no hang), and the codec never *amplifies* — the total
/// `data` bytes it emits cannot exceed the total bytes fed, because framing only
/// adds overhead (§5). A codec that duplicates input or hangs fails here.
///
/// **This suite cannot see a codec's *internal* buffer through the [`Codec`]
/// trait**, so it does *not* catch a codec that silently hoards undecodable input
/// (emitting nothing while its parser state grows without bound) — a real §5
/// violation on a lossy or hostile line. A codec that can report its buffered byte
/// count should additionally run [`assert_buffer_bounded`], which does catch it;
/// the reference codec does.
pub fn bounded_parser_state<C: Codec>(make: impl Fn() -> C) {
    let mut dec = make();
    let blob = seeded_bytes(99, MAX_FRAME_SIZE * 2 + 7);
    let mut emitted: usize = 0;
    for chunk in blob.chunks(1000) {
        // Err is fine (unframed garbage); the point is that the call returns.
        let _ = dec.demux(chunk, &mut |event| {
            if let EventKind::Data(bytes) = event.kind {
                emitted += bytes.len();
            }
        });
    }
    assert!(
        emitted <= blob.len(),
        "codec amplified {} input bytes into {emitted} emitted bytes (unbounded state?)",
        blob.len()
    );
}

/// **Bounded internal buffer** — the property [`bounded_parser_state`] cannot see.
/// A framing codec must retain at most one partial frame (§5), even when fed
/// *undecodable* input; a naive non-resyncing decoder (`self.buf.extend(input);
/// while let Ok(Some(..)) = try_decode(&self.buf) { drain; emit }`) never drains on
/// a decode error and grows without bound — a memory-exhaustion bug on a lossy or
/// hostile line. A codec that can report its buffered byte count passes a
/// `buffered` accessor; this feeds a deterministic oversize-length blob (`0xFF`
/// bytes, so every framing codec reads an oversize `body_len`) and asserts the
/// buffer never exceeds one whole frame — [`MAX_FRAME_SIZE`] plus the 4-byte length
/// prefix (a legitimately near-max partial frame retains up to that). A resyncing
/// codec (which drains) passes; a hoarding one fails.
///
/// ```ignore
/// kit::assert_buffer_bounded(MyCodec::new, MyCodec::buffered);
/// ```
pub fn assert_buffer_bounded<C: Codec>(make: impl Fn() -> C, buffered: impl Fn(&C) -> usize) {
    let mut codec = make();
    let chunk = [0xFFu8; 4096];
    let rounds = (MAX_FRAME_SIZE * 2) / chunk.len() + 1;
    for _ in 0..rounds {
        let _ = codec.demux(&chunk, &mut |_| {});
        let held = buffered(&codec);
        assert!(
            held <= MAX_FRAME_SIZE + 4,
            "codec retained {held} buffered bytes, past the one-frame bound \
             MAX_FRAME_SIZE + 4 (a MAX_FRAME_SIZE body plus its 4-byte length \
             prefix), MAX_FRAME_SIZE={MAX_FRAME_SIZE} (§5): it hoards undecodable \
             input"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A genuinely-conformant codec built on codec-api's own envelope — the same
    // framing the reference codec exposes — so the suites are proven to *pass* for
    // a correct codec, independent of codec-reference (which cannot be a dependency
    // here without a cycle).
    #[derive(Default)]
    struct GoodFraming {
        buf: Vec<u8>,
    }
    impl Codec for GoodFraming {
        fn name(&self) -> &str {
            "good-framing"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            self.buf.extend_from_slice(input);
            loop {
                match crate::try_decode(&self.buf) {
                    Ok(Some((event, consumed))) => {
                        self.buf.drain(..consumed);
                        emit(event);
                    }
                    Ok(None) => break,
                    // Length-prefix-guided resync (same policy as the reference codec).
                    Err(_) => {
                        if self.buf.len() < 4 {
                            break;
                        }
                        let body_len = u32::from_be_bytes([
                            self.buf[0],
                            self.buf[1],
                            self.buf[2],
                            self.buf[3],
                        ]) as usize;
                        let skip = if body_len <= MAX_FRAME_SIZE && self.buf.len() >= 4 + body_len {
                            4 + body_len
                        } else {
                            4
                        };
                        self.buf.drain(..skip);
                    }
                }
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            crate::encode(event, out)?;
            Ok(())
        }
    }

    #[test]
    fn good_framing_codec_passes_every_suite() {
        round_trip_identity(GoodFraming::default, &["console", "trace", "ctrl"]);
        // GoodFraming carries the full §8 vocabulary (it rides the envelope), so it
        // also passes the opt-in control-event round-trip.
        control_event_round_trip(GoodFraming::default, &["console", "trace", "ctrl"]);
        fragmentation_tolerance(GoodFraming::default, "console");
        handles_garbage(GoodFraming::default, "console");
        bounded_parser_state(GoodFraming::default);
        // The resyncing framing codec drains on an oversize prefix, so its buffer
        // stays within one frame.
        assert_buffer_bounded(GoodFraming::default, |c| c.buf.len());
    }

    // A single-channel passthrough (like the external template's codec) also
    // conforms — for its one channel.
    #[derive(Default)]
    struct Passthrough;
    impl Codec for Passthrough {
        fn name(&self) -> &str {
            "passthrough"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            if !input.is_empty() {
                emit(Event::data("console", input.to_vec()));
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            if let EventKind::Data(bytes) = &event.kind {
                out.extend_from_slice(bytes);
            }
            Ok(())
        }
    }

    #[test]
    fn single_channel_passthrough_conforms_for_its_channel() {
        round_trip_identity(Passthrough::default, &["console"]);
        fragmentation_tolerance(Passthrough::default, "console");
        handles_garbage(Passthrough::default, "console");
        bounded_parser_state(Passthrough::default);
    }

    // --- Deliberately broken codecs: each must FAIL exactly the suite that tests
    //     the property it violates (plan §10.4). `#[should_panic]` captures the
    //     suite's `assert!` firing.

    /// Drops the last byte of every payload it muxes → breaks round-trip identity.
    #[derive(Default)]
    struct DropsLastByte {
        buf: Vec<u8>,
    }
    impl Codec for DropsLastByte {
        fn name(&self) -> &str {
            "drops-last-byte"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            self.buf.extend_from_slice(input);
            while let Ok(Some((event, consumed))) = crate::try_decode(&self.buf) {
                self.buf.drain(..consumed);
                emit(event);
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            let corrupt = match &event.kind {
                EventKind::Data(bytes) if !bytes.is_empty() => {
                    Event::data(event.channel.clone(), bytes[..bytes.len() - 1].to_vec())
                }
                _ => event.clone(),
            };
            crate::encode(&corrupt, out)?;
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "round-trip identity failed")]
    fn a_lossy_codec_fails_round_trip() {
        round_trip_identity(DropsLastByte::default, &["console"]);
    }

    /// Silently drops every `open` event it muxes → the far side never sees the
    /// open, so it breaks the control-event round-trip (but not the data-only one).
    #[derive(Default)]
    struct DropsOpen {
        buf: Vec<u8>,
    }
    impl Codec for DropsOpen {
        fn name(&self) -> &str {
            "drops-open"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            self.buf.extend_from_slice(input);
            while let Ok(Some((event, consumed))) = crate::try_decode(&self.buf) {
                self.buf.drain(..consumed);
                emit(event);
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            if matches!(event.kind, EventKind::Open) {
                return Ok(()); // drop the open — never framed
            }
            crate::encode(event, out)?;
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "control-event round-trip failed")]
    fn a_codec_dropping_open_fails_control_round_trip() {
        control_event_round_trip(DropsOpen::default, &["console"]);
    }

    /// Panics when it sees a `0xFF` byte → breaks garbage tolerance.
    #[derive(Default)]
    struct PanicsOnGarbage;
    impl Codec for PanicsOnGarbage {
        fn name(&self) -> &str {
            "panics-on-garbage"
        }
        fn demux(
            &mut self,
            input: &[u8],
            _emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            assert!(!input.contains(&0xFF), "unexpected byte");
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            crate::encode(event, out)?;
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "unexpected byte")]
    fn a_panicking_codec_fails_garbage_tolerance() {
        handles_garbage(PanicsOnGarbage::default, "console");
    }

    /// Emits every input byte twice → breaks the bounded-state / no-amplification
    /// invariant.
    #[derive(Default)]
    struct Amplifier;
    impl Codec for Amplifier {
        fn name(&self) -> &str {
            "amplifier"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            if !input.is_empty() {
                let doubled: Vec<u8> = input.iter().flat_map(|b| [*b, *b]).collect();
                emit(Event::data("console", doubled));
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            if let EventKind::Data(bytes) = &event.kind {
                out.extend_from_slice(bytes);
            }
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "amplified")]
    fn an_amplifying_codec_fails_bounded_state() {
        bounded_parser_state(Amplifier::default);
    }

    /// Only decodes when a whole frame arrives in one `demux` call (clears its
    /// buffer between calls) → breaks fragmentation tolerance.
    #[derive(Default)]
    struct WholeFrameOnly;
    impl Codec for WholeFrameOnly {
        fn name(&self) -> &str {
            "whole-frame-only"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            // No accumulation across calls: a fragmented frame is silently lost.
            let mut scratch = input.to_vec();
            while let Ok(Some((event, consumed))) = crate::try_decode(&scratch) {
                scratch.drain(..consumed);
                emit(event);
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            crate::encode(event, out)?;
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "reassembly")]
    fn a_whole_frame_only_codec_fails_fragmentation() {
        fragmentation_tolerance(WholeFrameOnly::default, "console");
    }

    /// The classic non-resyncing accumulating decoder: it never drains on a decode
    /// error, so it hoards undecodable input without bound. It passes all four
    /// trait-only suites (it emits nothing, so no amplification) but fails
    /// [`assert_buffer_bounded`] — the whole reason that check exists.
    #[derive(Default)]
    struct Hoarder {
        buf: Vec<u8>,
    }
    impl Codec for Hoarder {
        fn name(&self) -> &str {
            "hoarder"
        }
        fn demux(
            &mut self,
            input: &[u8],
            emit: &mut dyn FnMut(Event),
        ) -> Result<(), crate::CodecError> {
            self.buf.extend_from_slice(input);
            // No resync: on a decode error the loop exits and the buffer is never
            // drained, so undecodable bytes accumulate forever.
            while let Ok(Some((event, consumed))) = crate::try_decode(&self.buf) {
                self.buf.drain(..consumed);
                emit(event);
            }
            Ok(())
        }
        fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), crate::CodecError> {
            crate::encode(event, out)?;
            Ok(())
        }
    }

    #[test]
    fn a_hoarding_codec_passes_the_trait_only_suites() {
        // Demonstrates the gap the buffer-bound check closes: the trait-only suites
        // cannot see the unbounded internal state.
        round_trip_identity(Hoarder::default, &["console"]);
        fragmentation_tolerance(Hoarder::default, "console");
        handles_garbage(Hoarder::default, "console");
        bounded_parser_state(Hoarder::default);
    }

    #[test]
    #[should_panic(expected = "hoards undecodable input")]
    fn a_hoarding_codec_fails_the_buffer_bound() {
        assert_buffer_bounded(Hoarder::default, |c| c.buf.len());
    }
}
