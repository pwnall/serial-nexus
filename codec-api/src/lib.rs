#![forbid(unsafe_code)]

//! `codec-api` — the codec trait, per-channel event vocabulary, and the
//! envelope frame types (design §8, §9, §15.11, §15.15).
//!
//! A **codec** is a multi-channel framing transform: it converts between one
//! multiplexed byte stream and N channels, in both directions, emitting and
//! consuming per-channel [`Event`]s drawn from a small vocabulary — `data`,
//! `open`, `close`, `error`. One implementation serves both orientations; the
//! node's `faces` attribute selects which (§8).
//!
//! The **envelope** ([`encode`]/[`try_decode`]) is the v1 frame format shared by
//! the exec-codec child-process interface and the daemon-to-daemon wire (§8).
//! They are two *separately versioned contracts* that happen to share this one
//! implementation: [`ENVELOPE_VERSION`] governs the exec-codec envelope's public
//! stability promise (§15.15), and wire evolution must never break it. This
//! crate depends on nothing project-internal.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// The envelope frame format version. Bumping it is a breaking change to the
/// exec-codec child-process contract (§15.15) and must be deliberate.
pub const ENVELOPE_VERSION: u16 = 1;

/// The bounded maximum frame size (§9 clause 4). Keeps the interior one-frame
/// holdover (§5) and receive-side reassembly bounded-memory. v1: a fixed
/// constant; negotiable later.
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

/// A codec-scoped channel identity — the name that crosses the wire between
/// daemons (§3). Never derived from device paths; contains no `/` (§15.12).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChannelId(pub String);

impl ChannelId {
    pub fn new(s: impl Into<String>) -> Self {
        ChannelId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ChannelId {
    fn from(s: &str) -> Self {
        ChannelId(s.to_owned())
    }
}

/// The per-channel event vocabulary (§8). Deliberately small; the wire protocol
/// is evolvable to additional per-channel control events (§9 clause 2), but v1
/// is exactly these four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    /// Opaque channel data.
    Data(Bytes),
    /// The channel opened (announced/active).
    Open,
    /// The channel closed.
    Close,
    /// A channel-scoped error, with a human-readable reason.
    Error(String),
}

impl EventKind {
    /// The wire type byte for this event kind.
    fn type_byte(&self) -> u8 {
        match self {
            EventKind::Data(_) => 0,
            EventKind::Open => 1,
            EventKind::Close => 2,
            EventKind::Error(_) => 3,
        }
    }
}

/// A per-channel event: a channel identity and one [`EventKind`]. This is the
/// unit the envelope frames and the codec vocabulary carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub channel: ChannelId,
    pub kind: EventKind,
}

impl Event {
    pub fn data(channel: impl Into<ChannelId>, bytes: impl Into<Bytes>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Data(bytes.into()),
        }
    }
    pub fn open(channel: impl Into<ChannelId>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Open,
        }
    }
    pub fn close(channel: impl Into<ChannelId>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Close,
        }
    }
    pub fn error(channel: impl Into<ChannelId>, msg: impl Into<String>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Error(msg.into()),
        }
    }
}

/// A codec error. Schema/attribute failures are structural and fail the load
/// (§8, §11); framing errors during operation are counted in state (§7.5).
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("framing error: {0}")]
    Framing(String),
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
}

/// A multi-channel framing transform (§8). One implementation serves both
/// orientations. Edges always carry raw bytes; all framing knowledge is
/// internal to the codec, which keeps the §5 interior contract intact — a codec
/// may hold a partial frame (bounded by its frame size) and nothing else.
///
/// The reference framing codec and the exec codec implement this in phase 5;
/// codec-api defines only the contract.
pub trait Codec {
    /// The codec's registry name (the compiled-in match-on-name of §8).
    fn name(&self) -> &str;

    /// Consume multiplexed bytes arriving on the multiplexed side, invoking
    /// `emit` once per decoded per-channel event. Implementations may retain a
    /// partial frame across calls, bounded by the frame size.
    fn demux(&mut self, input: &[u8], emit: &mut dyn FnMut(Event)) -> Result<(), CodecError>;

    /// Encode a per-channel event into multiplexed bytes appended to `out`.
    fn mux(&mut self, event: &Event, out: &mut Vec<u8>) -> Result<(), CodecError>;

    /// Codec-specific resynchronization count surfaced as node state (§7.5:
    /// framing errors / resyncs). The default is `0` for codecs that never
    /// resynchronize (e.g. a codec over a reliable transport); a framing codec on
    /// a lossy line overrides it.
    fn resync_count(&self) -> u64 {
        0
    }
}

// ---------------------------------------------------------------------------
// The envelope: length-prefixed frames carrying a channel identity and a type.
// ---------------------------------------------------------------------------
//
// Layout (all integers big-endian):
//
//   u32  body_len            length of everything after this field
//   ---- body ----
//   u8   type                0=data, 1=open, 2=close, 3=error
//   u16  channel_id_len      length of the UTF-8 channel identity
//   ...  channel_id          channel_id_len bytes
//   ...  payload             data bytes / error message / empty
//
// `body_len` is bounded by MAX_FRAME_SIZE so reassembly is bounded-memory.

/// An envelope decode error (§9 clause 4/6: bounded frames, clean refusal).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    #[error("frame body length {0} exceeds the maximum {MAX_FRAME_SIZE}")]
    FrameTooLarge(usize),
    #[error("unknown frame type {0}")]
    UnknownType(u8),
    #[error("truncated frame: {0}")]
    Truncated(&'static str),
    #[error("channel identity is not valid UTF-8")]
    BadChannelId,
    #[error("error message is not valid UTF-8")]
    BadErrorMessage,
}

/// Encode one [`Event`] into the envelope format, appending to `out`.
///
/// Returns [`EnvelopeError::FrameTooLarge`] if the body would exceed
/// [`MAX_FRAME_SIZE`] — the encoder never emits a frame the decoder must reject.
pub fn encode(event: &Event, out: &mut Vec<u8>) -> Result<(), EnvelopeError> {
    let channel = event.channel.0.as_bytes();
    let payload: &[u8] = match &event.kind {
        EventKind::Data(b) => b,
        EventKind::Error(m) => m.as_bytes(),
        EventKind::Open | EventKind::Close => &[],
    };
    let body_len = 1 + 2 + channel.len() + payload.len();
    if body_len > MAX_FRAME_SIZE {
        return Err(EnvelopeError::FrameTooLarge(body_len));
    }
    // channel length must fit u16 (guaranteed by MAX_FRAME_SIZE < u16::MAX only
    // if channel dominates; check explicitly to be safe).
    let channel_len =
        u16::try_from(channel.len()).map_err(|_| EnvelopeError::FrameTooLarge(channel.len()))?;

    out.extend_from_slice(&(body_len as u32).to_be_bytes());
    out.push(event.kind.type_byte());
    out.extend_from_slice(&channel_len.to_be_bytes());
    out.extend_from_slice(channel);
    out.extend_from_slice(payload);
    Ok(())
}

/// Attempt to decode one [`Event`] from the front of `buf`.
///
/// * `Ok(Some((event, consumed)))` — a full frame was decoded; `consumed` bytes
///   should be discarded from the front of the buffer.
/// * `Ok(None)` — need more bytes (a partial frame).
/// * `Err(_)` — a malformed or oversize frame; the caller refuses the peer
///   cleanly (§9 clause 6).
pub fn try_decode(buf: &[u8]) -> Result<Option<(Event, usize)>, EnvelopeError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let body_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if body_len > MAX_FRAME_SIZE {
        return Err(EnvelopeError::FrameTooLarge(body_len));
    }
    let frame_end = 4 + body_len;
    if buf.len() < frame_end {
        return Ok(None);
    }
    let body = &buf[4..frame_end];
    if body.len() < 3 {
        return Err(EnvelopeError::Truncated("header"));
    }
    let type_byte = body[0];
    let channel_len = u16::from_be_bytes([body[1], body[2]]) as usize;
    let channel_end = 3 + channel_len;
    if body.len() < channel_end {
        return Err(EnvelopeError::Truncated("channel id"));
    }
    let channel_bytes = &body[3..channel_end];
    let payload = &body[channel_end..];
    let channel = ChannelId(
        std::str::from_utf8(channel_bytes)
            .map_err(|_| EnvelopeError::BadChannelId)?
            .to_owned(),
    );
    let kind = match type_byte {
        0 => EventKind::Data(Bytes::copy_from_slice(payload)),
        1 => EventKind::Open,
        2 => EventKind::Close,
        3 => EventKind::Error(
            std::str::from_utf8(payload)
                .map_err(|_| EnvelopeError::BadErrorMessage)?
                .to_owned(),
        ),
        other => return Err(EnvelopeError::UnknownType(other)),
    };
    Ok(Some((Event { channel, kind }, frame_end)))
}

/// A streaming envelope decoder: accumulates bytes and yields whole [`Event`]s,
/// holding at most one partial frame (bounded by [`MAX_FRAME_SIZE`]) — the §5
/// interior contract for a codec's parser state.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        FrameDecoder::default()
    }

    /// Append received bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Decode the next available event, or `None` if more bytes are needed.
    pub fn next_event(&mut self) -> Result<Option<Event>, EnvelopeError> {
        match try_decode(&self.buf)? {
            Some((event, consumed)) => {
                self.buf.drain(..consumed);
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    /// Bytes currently buffered (the bounded partial-frame state).
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn encode_hex(event: &Event) -> String {
        let mut out = Vec::new();
        encode(event, &mut out).unwrap();
        hex(&out)
    }

    /// Golden vectors: exact wire bytes for each event kind (§15.15). A drift
    /// here is a breaking envelope change and must be deliberate — regenerating
    /// these constants requires a written rationale in the commit.
    ///
    /// Layout reminder: `u32 body_len | u8 type | u16 chan_len | chan | payload`.
    #[test]
    fn golden_vectors() {
        // data "hi" on "console": body=00 0007 "console" "hi", body_len=0x0c.
        assert_eq!(
            encode_hex(&Event::data("console", Bytes::from_static(b"hi"))),
            "0000000c000007636f6e736f6c656869",
        );
        // open "trace": body=01 0005 "trace", body_len=0x08.
        assert_eq!(
            encode_hex(&Event::open("trace")),
            "000000080100057472616365"
        );
        // close "trace": body=02 0005 "trace".
        assert_eq!(
            encode_hex(&Event::close("trace")),
            "000000080200057472616365"
        );
        // error "c0"/"boom": body=03 0002 "c0" "boom", body_len=0x09.
        assert_eq!(
            encode_hex(&Event::error("c0", "boom")),
            "000000090300026330626f6f6d",
        );

        // Every golden vector decodes back to its event.
        for ev in [
            Event::data("console", Bytes::from_static(b"hi")),
            Event::open("trace"),
            Event::close("trace"),
            Event::error("c0", "boom"),
        ] {
            let mut out = Vec::new();
            encode(&ev, &mut out).unwrap();
            let (decoded, consumed) = try_decode(&out).unwrap().unwrap();
            assert_eq!(decoded, ev);
            assert_eq!(consumed, out.len());
        }
    }

    #[test]
    fn round_trips_all_kinds() {
        let events = [
            Event::data("console", Bytes::from_static(b"\x00\x01\xff data")),
            Event::open("coproc"),
            Event::close("coproc"),
            Event::error("console", "framing error: resync"),
        ];
        for ev in &events {
            let mut out = Vec::new();
            encode(ev, &mut out).unwrap();
            let (decoded, consumed) = try_decode(&out).unwrap().unwrap();
            assert_eq!(&decoded, ev);
            assert_eq!(consumed, out.len());
        }
    }

    #[test]
    fn partial_frame_needs_more() {
        let mut out = Vec::new();
        encode(&Event::data("x", Bytes::from_static(b"payload")), &mut out).unwrap();
        // Feeding any strict prefix yields None (need more), never an error.
        for cut in 0..out.len() {
            assert_eq!(
                try_decode(&out[..cut]).unwrap(),
                None,
                "prefix {cut} should need more"
            );
        }
        assert!(try_decode(&out).unwrap().is_some());
    }

    #[test]
    fn oversize_frame_is_refused() {
        // A length prefix over the max is rejected without buffering the body.
        let mut buf = ((MAX_FRAME_SIZE + 1) as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(&[0, 0, 0]); // partial body
        assert_eq!(
            try_decode(&buf),
            Err(EnvelopeError::FrameTooLarge(MAX_FRAME_SIZE + 1))
        );
    }

    #[test]
    fn unknown_type_is_refused() {
        // Hand-craft a frame with type byte 9.
        let mut buf = Vec::new();
        let body: &[u8] = &[9, 0, 1, b'x']; // type=9, chanlen=1, "x"
        buf.extend_from_slice(&(body.len() as u32).to_be_bytes());
        buf.extend_from_slice(body);
        assert_eq!(try_decode(&buf), Err(EnvelopeError::UnknownType(9)));
    }

    #[test]
    fn streaming_decoder_yields_multiple_frames() {
        let mut wire = Vec::new();
        encode(&Event::open("a"), &mut wire).unwrap();
        encode(&Event::data("a", Bytes::from_static(b"12345")), &mut wire).unwrap();
        encode(&Event::close("a"), &mut wire).unwrap();

        let mut dec = FrameDecoder::new();
        // Feed one byte at a time to exercise partial-frame buffering.
        let mut events = Vec::new();
        for b in &wire {
            dec.push(std::slice::from_ref(b));
            while let Some(ev) = dec.next_event().unwrap() {
                events.push(ev);
            }
        }
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], Event::open("a"));
        assert_eq!(events[1], Event::data("a", Bytes::from_static(b"12345")));
        assert_eq!(events[2], Event::close("a"));
        assert_eq!(dec.buffered(), 0);
    }
}
