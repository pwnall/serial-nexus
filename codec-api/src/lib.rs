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

/// The codec conformance kit (§15.26 / plan §10.4): generic suites any [`Codec`]
/// implementation runs in its own tests. Behind the `test-support` feature so it
/// is compiled only where a consumer opts in (a dev-dependency), never into a
/// shipping build.
#[cfg(feature = "test-support")]
pub mod test_support;

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
    /// The channel this event belongs to.
    pub channel: ChannelId,
    /// What happened on the channel — one of the four §8 event kinds.
    pub kind: EventKind,
}

impl Event {
    /// A `data` event carrying opaque channel bytes.
    pub fn data(channel: impl Into<ChannelId>, bytes: impl Into<Bytes>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Data(bytes.into()),
        }
    }
    /// An `open` event announcing the channel is active.
    pub fn open(channel: impl Into<ChannelId>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Open,
        }
    }
    /// A `close` event marking the channel closed.
    pub fn close(channel: impl Into<ChannelId>) -> Self {
        Event {
            channel: channel.into(),
            kind: EventKind::Close,
        }
    }
    /// An `error` event carrying a human-readable, channel-scoped reason.
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

// ---------------------------------------------------------------------------
// The wire hello: the connection-opening handshake frame (§9).
// ---------------------------------------------------------------------------
//
// The daemon-to-daemon wire (§7.4, §9) is the envelope stream preceded by one
// `hello` frame that identifies the protocol and announces channels. The hello
// is a *distinct wire construct*, not a fifth [`EventKind`] — the envelope's
// four event kinds and golden vectors are frozen (the exec-codec contract,
// §15.15), and the exec child never sends a hello. The hello reuses the
// envelope's `u32` length-prefix discipline so a single reader frames both, but
// its body is a hello, distinguished by a leading magic number.
//
// Layout (all integers big-endian):
//
//   u32  body_len            length of everything after this field
//   ---- body ----
//   u32  magic               WIRE_MAGIC ("SNXL")
//   u16  wire_version        WIRE_VERSION
//   u32  capabilities        capability bitset (v1: 0)
//   u16  announcement_count  number of announced channel identities
//   ...  announcements       announcement_count × (u16 chan_len | chan UTF-8)
//
// `body_len` is bounded by MAX_FRAME_SIZE so a hello with many announcements
// stays bounded-memory (§9 clause 4).

/// The wire hello magic ("SNXL" — serial_nexus link), distinguishing a hello
/// frame from an envelope data frame at connection start.
pub const WIRE_MAGIC: u32 = 0x534E_584C;

/// The wire protocol version, versioned independently of [`ENVELOPE_VERSION`]
/// (§8, §15.15): wire evolution must never break the exec-codec envelope. A peer
/// announcing a different wire version is refused cleanly (§9 clause 6).
pub const WIRE_VERSION: u16 = 1;

/// Reserved capability bit for the deferred cross-machine lock request/grant
/// relay (§6, §9 clause 2, §14). Not negotiated in v1; defined so the bit is
/// claimed and a future daemon can light it up additively.
pub const CAP_LOCK_RELAY: u32 = 1 << 0;

/// The connection-opening handshake (§9 clause 3/6): protocol version,
/// negotiated capabilities, and the sender's channel announcements. Binding
/// announced identities to configured channels is the leg's job and never grows
/// the graph (§8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub version: u16,
    pub capabilities: u32,
    pub channels: Vec<ChannelId>,
}

/// A wire-handshake decode error (§9 clause 6: refuse a mismatch cleanly, with
/// the reason surfaced in leg state). Distinct from [`EnvelopeError`] because the
/// hello is a wire-only construct the shared envelope contract knows nothing of.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("bad wire magic {got:#010x} (expected {WIRE_MAGIC:#010x})")]
    BadMagic { got: u32 },
    #[error("unsupported wire protocol version {got} (this daemon speaks {WIRE_VERSION})")]
    UnsupportedVersion { got: u16 },
    #[error("hello frame body length {0} exceeds the maximum {MAX_FRAME_SIZE}")]
    FrameTooLarge(usize),
    #[error("truncated hello: {0}")]
    Truncated(&'static str),
    #[error("announced channel identity is not valid UTF-8")]
    BadChannelId,
}

/// Encode a [`Hello`] into the wire format, appending to `out`.
///
/// Returns [`WireError::FrameTooLarge`] if the body would exceed
/// [`MAX_FRAME_SIZE`] — the encoder never emits a hello the decoder must reject.
pub fn encode_hello(hello: &Hello, out: &mut Vec<u8>) -> Result<(), WireError> {
    // magic(4) + version(2) + capabilities(4) + count(2)
    let mut body_len = 12usize;
    for ch in &hello.channels {
        body_len += 2 + ch.0.len();
    }
    if body_len > MAX_FRAME_SIZE {
        return Err(WireError::FrameTooLarge(body_len));
    }
    let count =
        u16::try_from(hello.channels.len()).map_err(|_| WireError::FrameTooLarge(body_len))?;

    out.extend_from_slice(&(body_len as u32).to_be_bytes());
    out.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
    out.extend_from_slice(&hello.version.to_be_bytes());
    out.extend_from_slice(&hello.capabilities.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    for ch in &hello.channels {
        let ch_len = u16::try_from(ch.0.len()).map_err(|_| WireError::FrameTooLarge(ch.0.len()))?;
        out.extend_from_slice(&ch_len.to_be_bytes());
        out.extend_from_slice(ch.0.as_bytes());
    }
    Ok(())
}

/// Attempt to decode the opening [`Hello`] from the front of `buf`.
///
/// Mirrors [`try_decode`]'s partial-vs-error contract: `Ok(None)` means need
/// more bytes; `Err` means a malformed/oversize/mismatched hello to be refused
/// cleanly (§9 clause 6). Magic is checked first (a bad magic means "not our
/// protocol"), then the version: a version mismatch returns
/// [`WireError::UnsupportedVersion`] carrying the peer's value *without* parsing
/// the rest, since a different version may lay its body out differently.
pub fn try_decode_hello(buf: &[u8]) -> Result<Option<(Hello, usize)>, WireError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let body_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if body_len > MAX_FRAME_SIZE {
        return Err(WireError::FrameTooLarge(body_len));
    }
    let frame_end = 4 + body_len;
    if buf.len() < frame_end {
        return Ok(None);
    }
    let body = &buf[4..frame_end];
    // The magic(4) + version(2) prefix is version-stable by protocol design (it is
    // how negotiation works across differing future layouts), so validate it before
    // the v1-specific 12-byte header gate: a short-bodied hello with the right magic
    // and a wrong version is still reported as UnsupportedVersion, not a generic
    // truncation (§9 clause 6, and this function's contract).
    if body.len() < 6 {
        return Err(WireError::Truncated("header"));
    }
    let magic = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
    if magic != WIRE_MAGIC {
        return Err(WireError::BadMagic { got: magic });
    }
    let version = u16::from_be_bytes([body[4], body[5]]);
    if version != WIRE_VERSION {
        return Err(WireError::UnsupportedVersion { got: version });
    }
    // The rest of the v1 header (capabilities + announcement count) needs 12 bytes.
    if body.len() < 12 {
        return Err(WireError::Truncated("header"));
    }
    let capabilities = u32::from_be_bytes([body[6], body[7], body[8], body[9]]);
    let count = u16::from_be_bytes([body[10], body[11]]) as usize;
    // `count` is untrusted wire input: do not size the allocation from it (a
    // 16-byte hello can claim count=0xFFFF with zero announcement bytes and drive
    // a ~1.5 MB reservation — CODECAPI-1). Each real announcement needs >=2 bytes
    // (its u16 length prefix), so the body itself caps how many can follow; clamp
    // to that so growth stays proportional to bytes actually received. `body.len()
    // >= 12` is guaranteed by the header gate above, so the subtraction is safe.
    let mut channels = Vec::with_capacity(count.min((body.len() - 12) / 2));
    let mut off = 12;
    for _ in 0..count {
        if body.len() < off + 2 {
            return Err(WireError::Truncated("announcement length"));
        }
        let ch_len = u16::from_be_bytes([body[off], body[off + 1]]) as usize;
        off += 2;
        if body.len() < off + ch_len {
            return Err(WireError::Truncated("announcement identity"));
        }
        let ch = std::str::from_utf8(&body[off..off + ch_len])
            .map_err(|_| WireError::BadChannelId)?
            .to_owned();
        off += ch_len;
        channels.push(ChannelId(ch));
    }
    Ok(Some((
        Hello {
            version,
            capabilities,
            channels,
        },
        frame_end,
    )))
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

    // ---- hostile-decode clean-refusal paths (§9 clause 6) ----

    /// Frame a raw `body` behind a satisfied `u32` length prefix.
    fn frame_body(body: &[u8]) -> Vec<u8> {
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(body);
        buf
    }

    #[test]
    fn truncated_header_is_refused() {
        // body_len is satisfied but the body is shorter than the 3-byte header.
        assert_eq!(
            try_decode(&frame_body(&[0, 0])),
            Err(EnvelopeError::Truncated("header"))
        );
    }

    #[test]
    fn channel_len_overrunning_body_is_refused() {
        // type=0, chan_len=5, but zero channel bytes present in the body.
        assert_eq!(
            try_decode(&frame_body(&[0, 0, 5])),
            Err(EnvelopeError::Truncated("channel id"))
        );
    }

    #[test]
    fn non_utf8_channel_id_is_refused() {
        // type=0 (data), chan_len=2, channel bytes 0xFF 0xFE (invalid UTF-8).
        assert_eq!(
            try_decode(&frame_body(&[0, 0, 2, 0xFF, 0xFE])),
            Err(EnvelopeError::BadChannelId)
        );
    }

    #[test]
    fn non_utf8_error_message_is_refused() {
        // type=3 (error), chan_len=1, channel "x", payload 0xFF (invalid UTF-8).
        assert_eq!(
            try_decode(&frame_body(&[3, 0, 1, b'x', 0xFF])),
            Err(EnvelopeError::BadErrorMessage)
        );
    }

    #[test]
    fn encode_oversize_data_is_refused_and_appends_nothing() {
        // body = type(1) + chan_len(2) + "x"(1) + payload(MAX) = MAX + 4.
        let event = Event::data("x", vec![0u8; MAX_FRAME_SIZE]);
        let mut out = Vec::new();
        assert_eq!(
            encode(&event, &mut out),
            Err(EnvelopeError::FrameTooLarge(MAX_FRAME_SIZE + 4))
        );
        assert!(
            out.is_empty(),
            "encoder must append nothing when it refuses"
        );
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

    // ---- wire hello (§9) ----

    #[test]
    fn hello_round_trips_with_announcements() {
        let hello = Hello {
            version: WIRE_VERSION,
            capabilities: 0,
            channels: vec![ChannelId::new("console"), ChannelId::new("trace")],
        };
        let mut out = Vec::new();
        encode_hello(&hello, &mut out).unwrap();
        let (decoded, consumed) = try_decode_hello(&out).unwrap().unwrap();
        assert_eq!(decoded, hello);
        assert_eq!(consumed, out.len());
    }

    #[test]
    fn hello_with_no_channels_round_trips() {
        let hello = Hello {
            version: WIRE_VERSION,
            capabilities: CAP_LOCK_RELAY,
            channels: vec![],
        };
        let mut out = Vec::new();
        encode_hello(&hello, &mut out).unwrap();
        // body = 12 bytes (magic+version+caps+count), body_len prefix = 4.
        assert_eq!(out.len(), 16);
        let (decoded, consumed) = try_decode_hello(&out).unwrap().unwrap();
        assert_eq!(decoded, hello);
        assert_eq!(consumed, 16);
    }

    #[test]
    fn hello_partial_needs_more() {
        let hello = Hello {
            version: WIRE_VERSION,
            capabilities: 0,
            channels: vec![ChannelId::new("a"), ChannelId::new("bb")],
        };
        let mut out = Vec::new();
        encode_hello(&hello, &mut out).unwrap();
        for cut in 0..out.len() {
            assert_eq!(
                try_decode_hello(&out[..cut]).unwrap(),
                None,
                "prefix {cut} should need more"
            );
        }
        assert!(try_decode_hello(&out).unwrap().is_some());
    }

    #[test]
    fn hello_bad_magic_is_refused() {
        // A well-formed length prefix but a non-hello body (envelope-shaped).
        let mut buf = Vec::new();
        encode(&Event::data("console", Bytes::from_static(b"hi")), &mut buf).unwrap();
        // The envelope body starts with type byte 0, not the magic.
        match try_decode_hello(&buf) {
            Err(WireError::BadMagic { got }) => assert_ne!(got, WIRE_MAGIC),
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn hello_version_mismatch_is_refused_with_value() {
        // Hand-craft a hello with version 999.
        let mut body = Vec::new();
        body.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
        body.extend_from_slice(&999u16.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes()); // caps
        body.extend_from_slice(&0u16.to_be_bytes()); // count
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(&body);
        assert_eq!(
            try_decode_hello(&buf),
            Err(WireError::UnsupportedVersion { got: 999 })
        );
    }

    #[test]
    fn hello_short_body_still_reports_magic_and_version() {
        // A hello whose body carries only the version-stable magic(4)+version(2)
        // prefix (6 bytes, shorter than the v1 12-byte header) is still refused as a
        // version mismatch, not a generic truncation — the negotiation contract.
        let mut body = Vec::new();
        body.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
        body.extend_from_slice(&2u16.to_be_bytes());
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(&body);
        assert_eq!(
            try_decode_hello(&buf),
            Err(WireError::UnsupportedVersion { got: 2 })
        );
        // A short body with a *bad* magic reports BadMagic (not truncation).
        let mut body = Vec::new();
        body.extend_from_slice(&(!WIRE_MAGIC).to_be_bytes());
        body.extend_from_slice(&1u16.to_be_bytes());
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(&body);
        assert!(matches!(
            try_decode_hello(&buf),
            Err(WireError::BadMagic { .. })
        ));
    }

    #[test]
    fn hello_oversize_is_refused_before_buffering() {
        let mut buf = ((MAX_FRAME_SIZE + 1) as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(&[0, 0, 0]); // partial body only
        assert_eq!(
            try_decode_hello(&buf),
            Err(WireError::FrameTooLarge(MAX_FRAME_SIZE + 1))
        );
    }

    /// Frame a raw hello `body` behind a satisfied `u32` length prefix.
    fn frame_hello_body(body: &[u8]) -> Vec<u8> {
        let mut buf = (body.len() as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(body);
        buf
    }

    /// A v1 hello header (magic + WIRE_VERSION + caps=0) followed by `count`.
    fn hello_header(count: u16) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&WIRE_MAGIC.to_be_bytes());
        body.extend_from_slice(&WIRE_VERSION.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes()); // capabilities
        body.extend_from_slice(&count.to_be_bytes());
        body
    }

    #[test]
    fn hello_announcement_count_exceeding_body_is_refused() {
        // Header claims 2 announcements, but only one is present.
        let mut body = hello_header(2);
        body.extend_from_slice(&1u16.to_be_bytes()); // announcement 0 length = 1
        body.push(b'a'); // announcement 0 = "a"
        assert_eq!(
            try_decode_hello(&frame_hello_body(&body)),
            Err(WireError::Truncated("announcement length"))
        );
    }

    #[test]
    fn hello_announcement_identity_overrunning_body_is_refused() {
        // One announcement whose declared length runs past the body.
        let mut body = hello_header(1);
        body.extend_from_slice(&5u16.to_be_bytes()); // announcement length = 5
        body.extend_from_slice(b"ab"); // only 2 identity bytes, not 5
        assert_eq!(
            try_decode_hello(&frame_hello_body(&body)),
            Err(WireError::Truncated("announcement identity"))
        );
    }

    #[test]
    fn hello_non_utf8_announcement_is_refused() {
        // A well-framed announcement whose bytes are not valid UTF-8.
        let mut body = hello_header(1);
        body.extend_from_slice(&2u16.to_be_bytes()); // announcement length = 2
        body.extend_from_slice(&[0xFF, 0xFE]); // invalid UTF-8
        assert_eq!(
            try_decode_hello(&frame_hello_body(&body)),
            Err(WireError::BadChannelId)
        );
    }

    #[test]
    fn encode_hello_oversize_is_refused_and_appends_nothing() {
        // body = header(12) + announcement len(2) + identity(MAX) = MAX + 14.
        let hello = Hello {
            version: WIRE_VERSION,
            capabilities: 0,
            channels: vec![ChannelId::new("c".repeat(MAX_FRAME_SIZE))],
        };
        let mut out = Vec::new();
        assert_eq!(
            encode_hello(&hello, &mut out),
            Err(WireError::FrameTooLarge(MAX_FRAME_SIZE + 14))
        );
        assert!(
            out.is_empty(),
            "encoder must append nothing when it refuses"
        );
    }

    #[test]
    fn hello_does_not_disturb_the_envelope_golden_vectors() {
        // Sanity: adding the hello left the envelope encoding byte-identical.
        let mut out = Vec::new();
        encode(&Event::data("console", Bytes::from_static(b"hi")), &mut out).unwrap();
        assert_eq!(hex(&out), "0000000c000007636f6e736f6c656869");
    }
}
