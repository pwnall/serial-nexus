#![forbid(unsafe_code)]

//! `tinymux-codec` — the **second** out-of-tree template codec (§8; plan §18 item 39).
//!
//! `acme-codec` next door is a single-channel passthrough: it demonstrates the
//! embedding pattern and nothing else. Three things a real codec has, it has not — it
//! carries no control events, holds no parser state, and takes no attributes — so
//! three of the conformance kit's suites had no in-template consumer at all. This
//! crate is the smallest codec that has all three:
//!
//! * **two channels**, selected by a tag byte, so `control_event_round_trip` has
//!   something to round-trip and misrouting is possible enough to be worth checking;
//! * **parser state**, so `assert_buffer_bounded` has a buffer to bound — and the
//!   resync that keeps it bounded is three lines, right here, where an author copying
//!   this can see it;
//! * **one attribute** (`channels`), validated in this crate rather than in the daemon
//!   binary, so `attributes_are_structural` can be run from the consumer position —
//!   which is the whole point of that suite (§8 clause 12).
//!
//! It is deliberately **not** an envelope codec. `acme` and the reference codec both
//! ride the shared envelope; a codec with a device's own framing is the more common
//! case, and this one shows what that costs: you write the framer and the resync, and
//! the kit still judges you.
//!
//! # The framing
//!
//! One record: `tag:u8 | kind:u8 | len:u16 big-endian | payload[len]`.
//!
//! * `tag` is the channel's index in the configured `channels` list — an index on the
//!   wire, a *name* everywhere else (§8 clause 3: identities are names; the mapping to
//!   an index is this codec's private business, established by configuration order).
//! * `kind` is `0` data, `1` open, `2` close, `3` error — the four §8 events, numbered
//!   as the envelope numbers them so a reader of both is not made to hold two tables.
//! * `len` bounds a record at [`MAX_RECORD`]. A `data` payload longer than that is
//!   **fragmented** across consecutive records, never dropped (§8 clause 4).

use serial_nexus_codec_api::{Codec, CodecError, Event, EventKind};

/// The largest payload one record carries. Longer `data` is fragmented across
/// consecutive records; the decoder rejects a length above this, which is what keeps
/// [`TinyMuxCodec::buffered`] inside one record plus its header.
pub const MAX_RECORD: usize = 1024;

/// The fixed record header: tag, kind, and the u16 length.
const HEADER: usize = 4;

/// The most channels one `tinymux` node serves — the tag is one byte, but a small
/// bound is a *stated, structurally checked* maximum rather than an implied one
/// (§15.34: every numeric attribute carries one).
pub const MAX_CHANNELS: usize = 8;

/// A two-channel (or up-to-[`MAX_CHANNELS`]) tag-framed codec.
pub struct TinyMuxCodec {
    /// Channel identities in configuration order; the index is the wire tag.
    channels: Vec<String>,
    /// Undecoded bytes carried between `demux` calls — at most one record's worth,
    /// which [`buffered`](Self::buffered) exposes so the kit can prove it.
    buf: Vec<u8>,
    resyncs: u64,
}

impl TinyMuxCodec {
    /// Build from an ordered channel list. Prefer
    /// [`from_attributes`](Self::from_attributes) at the registry boundary; this is
    /// the constructor a test (or an embedder with the list already in hand) uses.
    pub fn new<S: AsRef<str>>(channels: &[S]) -> Result<Self, String> {
        let channels: Vec<String> = channels.iter().map(|c| c.as_ref().to_owned()).collect();
        validate_channels(&channels)?;
        Ok(TinyMuxCodec {
            channels,
            buf: Vec::new(),
            resyncs: 0,
        })
    }

    /// **The attribute schema** (§8 clause 12), living in the codec crate so it can be
    /// proven from the consumer position with
    /// `serial_nexus_codec_api::test_support::attributes_are_structural`.
    ///
    /// One key, `channels`: required, an array of 1..=[`MAX_CHANNELS`] non-empty UTF-8
    /// identities containing no `/` (§8 clause 3) and no duplicates. Every failure
    /// **returns** — a schema failure is structural and aborts the load with nothing
    /// created (§11) — and every failure **names** what it refused, including the
    /// unknown key, because an operator cannot fix what the error will not point at.
    pub fn from_attributes(attributes: &toml::Table) -> Result<Self, String> {
        for key in attributes.keys() {
            if key != "channels" {
                return Err(format!(
                    "codec \"tinymux\" attributes: unknown key {key:?} (the only key is \
                     \"channels\")"
                ));
            }
        }
        let raw = attributes
            .get("channels")
            .ok_or_else(|| "codec \"tinymux\" attributes: channels is required".to_owned())?;
        let array = raw.as_array().ok_or_else(|| {
            "codec \"tinymux\" attributes: channels must be an array of identities".to_owned()
        })?;
        let mut channels = Vec::with_capacity(array.len());
        for (i, value) in array.iter().enumerate() {
            let s = value.as_str().ok_or_else(|| {
                format!("codec \"tinymux\" attributes: channels[{i}] is not a string")
            })?;
            channels.push(s.to_owned());
        }
        validate_channels(&channels)?;
        Ok(TinyMuxCodec {
            channels,
            buf: Vec::new(),
            resyncs: 0,
        })
    }

    /// Bytes held between calls — at most one record and its header. The accessor
    /// `assert_buffer_bounded` needs: without it the kit cannot see this field, and a
    /// hoarding decoder passes every trait-only suite (§8's kit-honesty rule).
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    fn tag_of(&self, channel: &str) -> Option<u8> {
        self.channels
            .iter()
            .position(|c| c == channel)
            .map(|i| i as u8)
    }

    fn push_record(&self, tag: u8, kind: u8, payload: &[u8], out: &mut Vec<u8>) {
        out.push(tag);
        out.push(kind);
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        out.extend_from_slice(payload);
    }
}

fn validate_channels(channels: &[String]) -> Result<(), String> {
    if channels.is_empty() {
        return Err("codec \"tinymux\" attributes: channels must be non-empty".to_owned());
    }
    if channels.len() > MAX_CHANNELS {
        return Err(format!(
            "codec \"tinymux\" attributes: channels has {} entries, above the maximum \
             {MAX_CHANNELS}",
            channels.len()
        ));
    }
    for (i, c) in channels.iter().enumerate() {
        if c.is_empty() {
            return Err(format!(
                "codec \"tinymux\" attributes: channels[{i}] is empty (the empty identity \
                 is reserved for the multiplexed side)"
            ));
        }
        if c.contains('/') {
            return Err(format!(
                "codec \"tinymux\" attributes: channels[{i}] = {c:?} contains '/', which a \
                 channel identity may not"
            ));
        }
        if channels[..i].contains(c) {
            return Err(format!(
                "codec \"tinymux\" attributes: channels[{i}] = {c:?} is a duplicate"
            ));
        }
    }
    Ok(())
}

impl Codec for TinyMuxCodec {
    fn name(&self) -> &str {
        "tinymux"
    }

    /// Decode whole records, holding at most one partial record across calls.
    ///
    /// The resync is the part worth copying: on an *undecodable* record — an unknown
    /// tag, an unknown kind, or a length past [`MAX_RECORD`] — it drops **one byte**
    /// and retries. That is what keeps `buf` bounded on a lossy line; the shape that
    /// looks simpler (`while let Ok(Some(..)) = decode { .. }`, exiting the loop on an
    /// error without draining) never drains and grows without bound. The kit's
    /// `Hoarder` is exactly that shape.
    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError> {
        self.buf.extend_from_slice(input);
        let mut consumed = 0usize;
        let mut refused: Option<String> = None;
        loop {
            let rest = &self.buf[consumed..];
            if rest.len() < HEADER {
                break; // need more — never an error (§8 clause 5)
            }
            let tag = rest[0];
            let kind = rest[1];
            let len = u16::from_be_bytes([rest[2], rest[3]]) as usize;
            let channel = self.channels.get(tag as usize);
            if channel.is_none() || kind > 3 || len > MAX_RECORD {
                refused.get_or_insert_with(|| {
                    format!("undecodable record (tag {tag}, kind {kind}, len {len})")
                });
                consumed += 1; // byte-wise resync: drain, then try again
                self.resyncs += 1;
                continue;
            }
            if rest.len() < HEADER + len {
                break; // a whole record has not arrived yet
            }
            let channel = channel.expect("checked above").clone();
            let payload = &rest[HEADER..HEADER + len];
            let event = match kind {
                0 => Event::data(channel.as_str(), payload.to_vec()),
                1 => Event::open(channel.as_str()),
                2 => Event::close(channel.as_str()),
                _ => Event::error(
                    channel.as_str(),
                    String::from_utf8_lossy(payload).into_owned(),
                ),
            };
            consumed += HEADER + len;
            emit(event);
        }
        self.buf.drain(..consumed);

        // Emit-before-error is supported and the salvaged events above are already
        // out (§8 clause 7): the refusal is reported after them, not instead of them.
        match refused {
            Some(reason) => Err(CodecError::Framing(reason)),
            None => Ok(()),
        }
    }

    /// Frame one event. A `data` payload above [`MAX_RECORD`] is **fragmented** across
    /// consecutive records — never dropped, never skipped on error (§8 clause 4).
    fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError> {
        let channel = event.channel.as_str();
        let tag = self.tag_of(channel).ok_or_else(|| {
            CodecError::Framing(format!(
                "channel {channel:?} is not one of this codec's configured channels \
                 {:?}",
                self.channels
            ))
        })?;
        match &event.kind {
            EventKind::Data(bytes) if bytes.is_empty() => self.push_record(tag, 0, &[], out),
            EventKind::Data(bytes) => {
                for fragment in bytes.chunks(MAX_RECORD) {
                    self.push_record(tag, 0, fragment, out);
                }
            }
            EventKind::Open => self.push_record(tag, 1, &[], out),
            EventKind::Close => self.push_record(tag, 2, &[], out),
            EventKind::Error(reason) => {
                // A reason longer than one record is truncated at a char boundary
                // rather than fragmented: an error reason is a diagnostic, not a
                // stream, and half a message on each of two records would be worse.
                let mut end = reason.len().min(MAX_RECORD);
                while end > 0 && !reason.is_char_boundary(end) {
                    end -= 1;
                }
                self.push_record(tag, 3, reason[..end].as_bytes(), out);
            }
        }
        Ok(())
    }

    fn resync_count(&self) -> u64 {
        self.resyncs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(src: &str) -> toml::Table {
        src.parse().expect("test attributes parse")
    }

    #[test]
    fn a_record_round_trips_on_the_channel_it_names() {
        let mut codec = TinyMuxCodec::new(&["console", "trace"]).expect("two channels");
        let mut wire = Vec::new();
        codec
            .mux(&Event::data("trace", b"hello".to_vec()), &mut wire)
            .expect("mux");
        let mut got = Vec::new();
        codec
            .demux(&wire, &mut |ev| got.push(ev))
            .expect("demux of our own framing");
        assert_eq!(got, vec![Event::data("trace", b"hello".to_vec())]);
    }

    #[test]
    fn an_oversize_payload_is_fragmented_not_dropped() {
        let mut codec = TinyMuxCodec::new(&["console"]).expect("one channel");
        let payload = vec![7u8; MAX_RECORD * 2 + 5];
        let mut wire = Vec::new();
        codec
            .mux(&Event::data("console", payload.clone()), &mut wire)
            .expect("mux");
        let mut got = Vec::new();
        codec
            .demux(&wire, &mut |ev| {
                if let EventKind::Data(b) = ev.kind {
                    got.extend_from_slice(&b);
                }
            })
            .expect("demux");
        assert_eq!(got, payload, "every byte survives fragmentation");
    }

    #[test]
    fn the_attribute_schema_refuses_what_it_should() {
        assert!(TinyMuxCodec::from_attributes(&table("channels = [\"a\", \"b\"]")).is_ok());
        for (src, must_name) in [
            ("", "channels"),
            ("channels = []", "channels"),
            ("channels = [\"a\", \"a\"]", "duplicate"),
            ("channels = [\"a/b\"]", "'/'"),
            ("channels = [\"a\"]\nwidgets = 1", "widgets"),
        ] {
            let err = TinyMuxCodec::from_attributes(&table(src))
                .err()
                .unwrap_or_else(|| panic!("{src:?} must be refused"));
            assert!(err.contains(must_name), "{err} does not name {must_name}");
        }
    }
}

// The out-of-tree codec proves conformance from its own tests with the shared kit
// (§8): `cargo test --features conformance`. This template exists because it runs the
// three suites `acme` cannot — the control-event round-trip, the buffer bound, and the
// attribute schema — so those three are the point of the block below.
#[cfg(all(test, feature = "conformance"))]
mod conformance {
    use super::*;
    use serial_nexus_codec_api::test_support as kit;

    const CHANNELS: [&str; 2] = ["console", "trace"];
    fn make() -> TinyMuxCodec {
        TinyMuxCodec::new(&CHANNELS).expect("the template's own channels are valid")
    }

    #[test]
    fn tinymux_conforms() {
        kit::round_trip_identity(make, &CHANNELS);
        kit::fragmentation_tolerance(make, "console");
        kit::handles_garbage(make, "console");
        kit::bounded_parser_state(make);
        // The three `acme` cannot run.
        kit::control_event_round_trip(make, &CHANNELS);
        kit::assert_buffer_bounded(make, TinyMuxCodec::buffered);
        kit::attributes_are_structural(
            TinyMuxCodec::from_attributes,
            &[
                "channels = [\"console\"]".parse().expect("parse"),
                "channels = [\"console\", \"trace\"]".parse().expect("parse"),
            ],
            &[
                ("".parse().expect("parse"), "channels"),
                ("channels = []".parse().expect("parse"), "channels"),
                (
                    "channels = [\"a\", \"a\"]".parse().expect("parse"),
                    "duplicate",
                ),
                (
                    "channels = [\"console\"]\nwidgets = 1".parse().expect("parse"),
                    "widgets",
                ),
            ],
        );
        // The interior fragments a targetward write on its own boundary and hands
        // this codec one piece at a time — up to ~64 KiB, sixty-four times this
        // codec's `MAX_RECORD`. A codec with a *smaller* framing unit must fragment
        // internally rather than refuse (§8 clause 4), which is what `mux`'s
        // `chunks(MAX_RECORD)` loop is for; this suite is what proves it, and the
        // template's own `an_oversize_payload_is_fragmented_not_dropped` only ever
        // asked for two records' worth.
        kit::targetward_fragmentation_is_lossless(make, "console");
        // The resync-accounting suite, parameterized by *this codec's own* malformed
        // unit — the same own-framing escape the note below describes, taken as a
        // parameter instead of a paragraph. Eight `0xFF` bytes are a record header
        // whose tag names no configured channel, so the byte-wise resync in `demux`
        // drops one byte and retries while at least a header's worth remains: 8, 7,
        // 6, 5 and 4 bytes left is five drops, and then three bytes wait for more.
        // Five is what this codec's policy costs, and it is the number an operator
        // reads as `framing_errors` on the node (§7.5).
        kit::resync_is_counted(make, &[0xFF; 8], 5);
        // NOT `recovers_after_garbage`: that suite injects an *envelope* frame, and
        // this codec has its own framing (§8's corruption recipe — replicate the
        // pattern with your own malformed unit, which `demux`'s own tests do, and
        // which `resync_is_counted` above takes as an argument).
    }
}
