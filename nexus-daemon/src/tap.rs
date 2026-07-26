//! Connection-scoped taps and the per-endpoint replay ring (design §5, §6, §17).
//!
//! A **tap** is the `never` write mode in dynamic form: a read-only observer
//! attached to a host-facing endpoint over the control plane, streaming that
//! endpoint's hostward bytes to one control connection as `tap.data`
//! notifications. It is a §5 boundary consumer in miniature — a bounded per-tap
//! queue with a drop counter, so a slow browser tab costs only its own tap's
//! counter and never its neighbors. Taps are *state*, scoped to a connection: they
//! never appear in configuration or `dump`, which is what keeps a viewer from
//! mutating the operator-owned graph (§8, §11).
//!
//! The **replay ring** is a per-host-facing-endpoint bounded ring of the most
//! recent hostward bytes (`replay_ring = <bytes>`, **default 65536** on every
//! host-facing endpoint since §15.32; `0` opts out). It is a feature buffer,
//! explicitly *not* flow control: it never backpressures the device. Its cost is
//! stated rather than hidden (§5) — bounded memory per endpoint plus one mirrored
//! copy of each hostward chunk into the tap feed, which is why the ring must stay
//! bulk-memcpy circular storage ([`ReplayRing`]) now that it sits on *every*
//! endpoint's hot path. A tap opened with `--replay` receives the ring snapshot and
//! then the live stream with an **exact splice** — no gap, no duplication.
//!
//! Both live in one per-endpoint [`TapHub`] behind `Rc<CriticalCell<TapHub>>` on
//! the runtime thread. The producer mirrors hostward bytes into a bounded feed
//! channel (only while [`TapHub::active`] — a ring is configured or at least one
//! tap is open), a hub task drains it and calls [`TapHub::ingest`], and the daemon
//! calls [`TapHub::register`]/[`TapHub::close`] on `tap.open`/`tap.close`. Because
//! ingest and register are both synchronous critical sections on the one thread,
//! a registration (ring snapshot + attach) can never interleave with a live chunk
//! — the exact-splice guarantee is a single-thread fact, not a lock (§15.20's
//! two-lane model doing double duty for §5's ring splice).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use nexus_core::Chunk;
use tokio::sync::mpsc;

use crate::cell::CriticalCell;

/// Depth (in chunks) of a control connection's outbound tap channel — the §5
/// bounded boundary for taps. A browser tab that stops reading fills this, after
/// which the hub drops-with-counter, so a slow tab costs only its own tap's
/// counter rather than buffering unbounded latency. Shared by all of one
/// connection's taps (the connection, i.e. the socket, is the honest shared
/// bottleneck); each tap counts its own drops. Kept modest so a stalled viewer
/// sheds promptly instead of hoarding tens of MiB of scrollback it will never show.
pub const TAP_QUEUE_CAP: usize = 128;

/// Depth (in chunks) of the producer→hub feed channel. The hub drains it on the
/// runtime thread with cheap per-chunk work (ring append + fan-out), so at console
/// rates it never fills; a firehose that outruns the hub drops here, lossy like any
/// hostward boundary (§5). Sized to absorb a reader-thread burst across a
/// runtime-scheduling gap.
pub const TAP_FEED_CAP: usize = 256;

/// Cap on the size of one replay-snapshot piece delivered on `tap.open --replay`,
/// so a large ring becomes a handful of `tap.data` notifications rather than one
/// giant line. Matches the data-plane read buffer.
const REPLAY_PIECE: usize = 64 * 1024;

/// One delivery from a hub to a control connection over that connection's tap
/// channel (§10/§17). Both variants become id-less notifications on the tap's own
/// connection — `tap.data` carries bytes, `tap.closed` is the stream's terminal
/// event — which is why they ride the tap channel and not the `subscribe`
/// broadcast: §10 delivers a tap's stream *on that connection*, whether or not the
/// client also subscribed.
pub enum TapMsg {
    /// Hostward bytes for one tap.
    Data {
        tap_id: u64,
        bytes: Chunk,
        /// The endpoint's monotonic hostward byte offset of `bytes[0]` (§10/§15.32):
        /// the count of hostward bytes ingested at this endpoint before these. Lets a
        /// reconnecting client (the browser history of §17) splice replay and live
        /// exactly, trimming any overlap instead of duplicating ring bytes on reload.
        offset: u64,
        /// Bytes lost at this endpoint's producer→hub feed hop that are *not*
        /// represented in the offset space — the hole the offsets alone cannot show
        /// (§5 all-loss-counted, TAP-1b). See [`TapHub::ingest`] for why the offset
        /// space deliberately stays the delivered-bytes space and the loss is
        /// signalled beside it instead.
        gap_before: u64,
    },
    /// The tap's endpoint went away beneath it — `teardown`, `load --replace`, or
    /// `remove-node` dropped the hub while this tap was open (§17, TAP-1). Without
    /// this the client sits on a live connection receiving nothing forever, with no
    /// notification and no error.
    Closed {
        tap_id: u64,
        endpoint: String,
        reason: &'static str,
    },
}

/// The outcome of registering a tap (§11.8): how many replay bytes were queued and the
/// endpoint offset the tap's stream begins at. With `--replay` and a non-empty ring,
/// `from_offset` is the offset of the ring's first byte; otherwise it is the current
/// live edge, so the next `tap.data` this tap sees carries exactly that offset.
pub struct Registered {
    pub replay_bytes: u64,
    pub from_offset: u64,
    /// Bytes this endpoint has lost at the producer→hub feed hop so far (§5). The
    /// offset space counts only delivered bytes, so this is the client's baseline
    /// for the `gap_before` deltas that follow (TAP-1b).
    pub feed_dropped: u64,
}

/// Why a hub was detached from beneath its open taps (§17 `tap.closed`). Static
/// strings: the reason is a stable, machine-readable token in the notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapCloseReason {
    /// `remove-node` dropped the endpoint's owning node.
    EndpointRemoved,
    /// `load --replace` rebuilt the whole graph.
    GraphReplaced,
    /// `teardown` (or clean shutdown) cleared the graph.
    Teardown,
}

impl TapCloseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            TapCloseReason::EndpointRemoved => "endpoint removed",
            TapCloseReason::GraphReplaced => "graph replaced",
            TapCloseReason::Teardown => "teardown",
        }
    }
}

/// The producer side of a tap feed: where a host-facing producer mirrors its
/// hostward chunks. Mirroring happens only while `active`, so an untapped,
/// ring-less endpoint costs one relaxed atomic load per chunk and nothing more
/// (§5 "costs nothing when unset"). Cloned into each producer at `start`.
#[derive(Clone)]
pub struct TapFeed {
    pub tx: mpsc::Sender<Chunk>,
    pub active: Arc<AtomicBool>,
    /// Bytes dropped at this producer→hub feed hop because the feed was full — the
    /// hub fell behind across a scheduling gap under a firehose. Shared with the hub
    /// so `state` surfaces it (§5 all-loss-counted). Never backpressures the
    /// producer: the ring must not stall the device (§5).
    pub feed_dropped: Arc<AtomicU64>,
}

impl TapFeed {
    /// Mirror one hostward chunk to the hub if a tap or ring wants it. Lossy by
    /// construction: the ring never backpressures the producer (§5), so a full feed
    /// drops — and that drop is counted (`feed_dropped`) so §5's "loss is always
    /// counted" holds even for this internal hop. A dropped chunk shows as a gap in
    /// the ring / every tap; the exact-splice guarantee is between the snapshot and
    /// the live stream, not a promise the ring never loses under a firehose. The
    /// counter is *shared* with the hub, which turns it into the per-chunk
    /// `gap_before` a tap client sees ([`TapHub::ingest`]) — so the hole is visible,
    /// not merely tallied.
    pub fn mirror(&self, chunk: &Chunk) {
        if self.active.load(Ordering::Relaxed) && self.tx.try_send(chunk.clone()).is_err() {
            self.feed_dropped
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
    }

    /// Whether a tap or ring currently wants the hostward stream — the cheap check
    /// a producer makes before creating a chunk purely for the tap path.
    pub fn wanted(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
}

/// A registered tap inside a [`TapHub`].
struct Tap {
    id: u64,
    /// The connection's outbound channel (the §5 bounded boundary). `Closed`
    /// signals the connection dropped; `Full` is a counted drop.
    out: mpsc::Sender<TapMsg>,
    /// Bytes dropped because `out` was full — shared with the daemon so `state`
    /// surfaces the tab's own drop counter (§5, §17).
    dropped: Rc<Cell<u64>>,
}

/// A bounded ring of the most recent hostward bytes on a host-facing endpoint
/// (design §5 replay ring). `cap == 0` means no ring (the default), and no bytes
/// are ever stored.
///
/// Backed by a fixed circular `Vec<u8>` (lazily allocated to `cap` on first use), so
/// `push` is at most two `copy_from_slice`s — O(bytes) with **bulk** memcpy, never the
/// byte-at-a-time churn a `VecDeque<u8>` drain+extend would impose. Since default-on
/// rings (§15.32) put this on the hostward hot path of *every* endpoint, and the hub
/// task drains it on the single runtime thread, a per-byte cost here starves the rest of
/// that thread and collapses firehose throughput (measured); bulk memcpy keeps it within
/// the §15.19 bound.
struct ReplayRing {
    cap: usize,
    /// Circular storage; empty until the first push, then exactly `cap` bytes long.
    buf: Vec<u8>,
    /// Index where the next byte will be written (`% cap`).
    pos: usize,
    /// Valid bytes currently retained, `<= cap`.
    len: usize,
}

impl ReplayRing {
    fn push(&mut self, bytes: &[u8]) {
        if self.cap == 0 {
            return;
        }
        // A single write larger than the ring keeps only its own tail.
        let bytes = if bytes.len() > self.cap {
            &bytes[bytes.len() - self.cap..]
        } else {
            bytes
        };
        if bytes.is_empty() {
            return;
        }
        if self.buf.is_empty() {
            self.buf = vec![0u8; self.cap];
        }
        // Write into the circular buffer at `pos`, wrapping once at the end.
        let n = bytes.len();
        let first = (self.cap - self.pos).min(n);
        self.buf[self.pos..self.pos + first].copy_from_slice(&bytes[..first]);
        if n > first {
            self.buf[..n - first].copy_from_slice(&bytes[first..]);
        }
        self.pos = (self.pos + n) % self.cap;
        self.len = (self.len + n).min(self.cap);
    }

    fn snapshot(&self) -> Vec<u8> {
        if self.len == 0 {
            return Vec::new();
        }
        // Oldest retained byte sits `len` behind the write cursor.
        let start = (self.pos + self.cap - self.len) % self.cap;
        let mut out = Vec::with_capacity(self.len);
        let first = (self.cap - start).min(self.len);
        out.extend_from_slice(&self.buf[start..start + first]);
        if self.len > first {
            out.extend_from_slice(&self.buf[..self.len - first]);
        }
        out
    }
}

/// A per-host-facing-endpoint tap hub (design §5 ring + §17 taps).
pub struct TapHub {
    taps: Vec<Tap>,
    ring: Option<ReplayRing>,
    /// Total hostward bytes ever ingested at this endpoint (§11.8): the monotonic
    /// offset stamped on each delivered chunk. Wraps only at u64 (petabytes), never in
    /// a session; a daemon restart resets it and the `info` instance nonce changes so a
    /// client detects the reset rather than splicing across it.
    ingested: u64,
    /// Producer mirrors hostward bytes only while this is set — a ring is
    /// configured (always active), or at least one tap is open. Shared with every
    /// [`TapFeed`] for this endpoint.
    active: Arc<AtomicBool>,
    /// Bytes lost at the producer→hub feed hop, shared with the [`TapFeed`] that
    /// counts them (§5); surfaced in `state`.
    feed_dropped: Arc<AtomicU64>,
    /// The value of `feed_dropped` this hub has already reported to its taps, so
    /// each [`Self::ingest`] can charge only the *new* loss as that chunk's
    /// `gap_before` (TAP-1b).
    feed_dropped_seen: u64,
    /// Set once the graph dropped this hub out from under its taps
    /// ([`Self::detach_all`]). Connection-side [`OpenTap`] handles outlive the
    /// graph, so this is how a later `tap.close` learns the tap is already gone and
    /// fails loudly instead of reporting a success that closed nothing (§17,
    /// TAP-1).
    orphaned: bool,
    /// The endpoint display: reported in `state`, and named in the `tap.closed`
    /// notification so a client learns *which* of its taps died.
    endpoint: String,
}

/// A shared, single-threaded handle to one endpoint's [`TapHub`].
pub type SharedTapHub = Rc<CriticalCell<TapHub>>;

impl TapHub {
    /// Build a hub for a host-facing endpoint with an optional ring (`ring_cap`
    /// bytes, 0 = off). Returns the shared hub plus the `active` flag to hand to
    /// the producer's [`TapFeed`]. A ring makes the hub active from the first
    /// instant, so scrollback fills from graph start (§5).
    pub fn new(
        endpoint: impl Into<String>,
        ring_cap: usize,
    ) -> (SharedTapHub, Arc<AtomicBool>, Arc<AtomicU64>) {
        let active = Arc::new(AtomicBool::new(ring_cap > 0));
        let feed_dropped = Arc::new(AtomicU64::new(0));
        let hub = TapHub {
            taps: Vec::new(),
            ring: (ring_cap > 0).then(|| ReplayRing {
                cap: ring_cap,
                buf: Vec::new(),
                pos: 0,
                len: 0,
            }),
            ingested: 0,
            active: active.clone(),
            feed_dropped: feed_dropped.clone(),
            feed_dropped_seen: 0,
            orphaned: false,
            endpoint: endpoint.into(),
        };
        (Rc::new(CriticalCell::new(hub)), active, feed_dropped)
    }

    /// Append one hostward chunk to the ring and fan it out to every registered
    /// tap (§5). A tap whose bounded `out` is full has this chunk's bytes counted
    /// against its own drop counter and stays live; a tap whose `out` is closed
    /// (its connection dropped) is removed. Synchronous — no `.await` — so it never
    /// interleaves with [`Self::register`] (the exact-splice guarantee).
    ///
    /// **The offset space stays the delivered-bytes space, and feed loss is
    /// signalled beside it** (TAP-1b, §11.8). `ingested` counts only bytes that
    /// reached this hub, which is what keeps invariant 10 true in both halves: the
    /// ring holds `≤ ingested` bytes by construction, `from_offset = ingested −
    /// ring.len()` cannot underflow, *and* the ring stays contiguous in offset
    /// space — the property [`Self::register`]'s exact splice rests on. Folding the
    /// lossy [`TapFeed::mirror`] hop's drops into `ingested` would break that second
    /// half: a hole inside the retained window makes `ingested − ring.len()` no
    /// longer the ring's true base offset, and a browser splicing by offset would
    /// then write replay bytes at offsets the live stream never uses — a silent
    /// corruption strictly worse than the invisible gap. The drop also cannot be
    /// *placed* exactly: it happens on the producer's (possibly blocking) thread
    /// while up to [`TAP_FEED_CAP`] chunks are still queued ahead of it, and the
    /// only cross-thread plumbing is the shared `feed_dropped` atomic. So the hole
    /// is reported as `gap_before` on the first chunk drained after it — exact in
    /// *size*, approximate in *position* (early by at most the feed depth) — which
    /// is enough for a consumer to detect it, which is what §5 requires.
    pub fn ingest(&mut self, chunk: &Chunk) {
        if let Some(ring) = &mut self.ring {
            ring.push(chunk);
        }
        let n = chunk.len() as u64;
        // New feed-hop loss since the last chunk: the gap this chunk follows.
        let observed = self.feed_dropped.load(Ordering::Relaxed);
        let gap_before = observed.wrapping_sub(self.feed_dropped_seen);
        self.feed_dropped_seen = observed;
        // Offset of this chunk's first byte in the endpoint's hostward stream (§11.8),
        // stamped before advancing the running total.
        let offset = self.ingested;
        self.taps.retain(|tap| {
            match tap.out.try_send(TapMsg::Data {
                tap_id: tap.id,
                bytes: chunk.clone(),
                offset,
                gap_before,
            }) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tap.dropped.set(tap.dropped.get() + n);
                    true
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false, // connection gone
            }
        });
        self.ingested = self.ingested.wrapping_add(n);
        self.refresh_active();
    }

    /// Register a new tap. With `replay` and a configured ring, the ring snapshot is
    /// queued into `out` *before* the tap joins the fan-out list, so — because this
    /// runs in one critical section that no [`Self::ingest`] can interrupt — the tap
    /// receives exactly `ring ++ live` with no gap and no duplication. Returns the
    /// number of replay bytes queued (0 when `replay` is false or no ring exists —
    /// the explicit empty-replay marker of §17).
    pub fn register(
        &mut self,
        id: u64,
        out: mpsc::Sender<TapMsg>,
        dropped: Rc<Cell<u64>>,
        replay: bool,
    ) -> Registered {
        let mut replay_bytes = 0u64;
        // Where this tap's stream begins (§11.8). With a non-empty ring under `--replay`
        // it is the offset of the ring's oldest retained byte; otherwise it is the live
        // edge, so the tap's first live `tap.data` carries exactly this offset.
        let mut from_offset = self.ingested;
        if replay && let Some(ring) = &self.ring {
            let snap = ring.snapshot();
            from_offset = self.ingested - snap.len() as u64;
            // Offset walks the stream position of each replay piece — advancing even
            // past a dropped piece, so a client that loses one still splices the rest
            // at the right offset (a gap it can see, never a silent shift).
            let mut piece_off = from_offset;
            for piece in snap.chunks(REPLAY_PIECE) {
                let bytes = Chunk::copy_from_slice(piece);
                let len = bytes.len() as u64;
                match out.try_send(TapMsg::Data {
                    tap_id: id,
                    bytes,
                    offset: piece_off,
                    // The ring is contiguous in offset space by construction, so a
                    // replay piece never straddles a hole (see `ingest`).
                    gap_before: 0,
                }) {
                    Ok(()) => replay_bytes += len,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        dropped.set(dropped.get() + len);
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => break,
                }
                piece_off += len;
            }
        }
        self.taps.push(Tap { id, out, dropped });
        self.refresh_active();
        Registered {
            replay_bytes,
            from_offset,
            feed_dropped: self.feed_dropped.load(Ordering::Relaxed),
        }
    }

    /// Remove a tap by id (explicit `tap.close` or connection drop). Idempotent: a
    /// not-found id is a no-op.
    pub fn close(&mut self, id: u64) {
        self.taps.retain(|t| t.id != id);
        self.refresh_active();
    }

    /// Detach every open tap because the graph dropped this hub's endpoint (§17,
    /// TAP-1): `teardown`, `load --replace`, or `remove-node`. Each tap's owning
    /// connection is told with a terminal [`TapMsg::Closed`] on its own tap channel
    /// — best effort, since a connection that has stopped draining its bounded
    /// queue is already counted as dropping — and the hub is marked `orphaned`, so
    /// a later `tap.close` on the surviving handle fails instead of reporting a
    /// success that closed nothing. Returns the ids that were live, for the caller's
    /// log line.
    ///
    /// The ring is released here: connection-side [`OpenTap`] handles keep the hub
    /// alive after the graph is gone, and a detached hub can never be tapped again
    /// (the daemon has already removed it from its endpoint map), so retaining
    /// scrollback nobody can reach is pure leak.
    pub fn detach_all(&mut self, reason: TapCloseReason) -> Vec<u64> {
        let ids: Vec<u64> = self.taps.iter().map(|t| t.id).collect();
        for tap in &self.taps {
            let _ = tap.out.try_send(TapMsg::Closed {
                tap_id: tap.id,
                endpoint: self.endpoint.clone(),
                reason: reason.as_str(),
            });
        }
        self.taps.clear();
        self.ring = None;
        self.orphaned = true;
        self.active.store(false, Ordering::Relaxed);
        ids
    }

    /// Whether the graph dropped this hub out from under its taps
    /// ([`Self::detach_all`]).
    pub fn is_orphaned(&self) -> bool {
        self.orphaned
    }

    /// The endpoint display this hub observes.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// The hub is active (producer should mirror) while a ring is configured or any
    /// tap is open — never once orphaned, since its endpoint no longer exists.
    fn refresh_active(&self) {
        let active = !self.orphaned && (self.ring.is_some() || !self.taps.is_empty());
        self.active.store(active, Ordering::Relaxed);
    }

    /// A `state` snapshot of this hub's open taps (§17): one `{tap_id, dropped}` per
    /// tap. Read-only. The ring's configured depth is *configuration* (via `dump`),
    /// not state; a `tap.open --replay` that returns `replay_bytes: 0` is the live
    /// signal that history is off for the endpoint.
    pub fn snapshot(&self) -> TapHubSnapshot {
        TapHubSnapshot {
            taps: self.taps.iter().map(|t| (t.id, t.dropped.get())).collect(),
            feed_dropped: self.feed_dropped.load(Ordering::Relaxed),
        }
    }
}

/// A control connection's handle to one open tap (§17). The connection keeps one
/// per tap it opened; dropping it — an explicit `tap.close` (removing it from the
/// connection's list) or the connection itself closing — detaches the tap from its
/// hub, a direct synchronous mutation since the hub lives on the same runtime
/// thread. Prompt detach-on-drop is what makes `state` show zero taps immediately
/// even on an idle endpoint the hub would not otherwise re-scan (§17).
pub struct OpenTap {
    pub tap_id: u64,
    pub hub: SharedTapHub,
}

impl OpenTap {
    /// Whether this tap's endpoint was dropped out from under it (§17, TAP-1) —
    /// what makes a later `tap.close` an error rather than a hollow success.
    pub fn is_orphaned(&self) -> bool {
        self.hub.with(TapHub::is_orphaned)
    }

    /// The endpoint this tap observed, for the `tap.close` diagnostic.
    pub fn endpoint(&self) -> String {
        self.hub.with(|h| h.endpoint().to_owned())
    }
}

impl Drop for OpenTap {
    fn drop(&mut self) {
        self.hub.with_mut(|h| h.close(self.tap_id));
    }
}

/// A read-only view of a hub's open taps for the `state` verb.
pub struct TapHubSnapshot {
    /// `(tap_id, bytes_dropped)` per open tap.
    pub taps: Vec<(u64, u64)>,
    /// Bytes lost at the producer→hub feed hop for this endpoint (§5).
    pub feed_dropped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut mpsc::Receiver<TapMsg>) -> Vec<u8> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let TapMsg::Data { bytes, .. } = msg {
                out.extend_from_slice(&bytes);
            }
        }
        out
    }

    /// `(offset, gap_before, bytes)` per delivered data message.
    fn drain_data(rx: &mut mpsc::Receiver<TapMsg>) -> Vec<(u64, u64, Vec<u8>)> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let TapMsg::Data {
                offset,
                gap_before,
                bytes,
                ..
            } = msg
            {
                out.push((offset, gap_before, bytes.to_vec()));
            }
        }
        out
    }

    #[test]
    fn tap_receives_live_bytes_and_counts_drops_when_full() {
        let (hub, active, _fd) = TapHub::new("usb0", 0);
        assert!(!active.load(Ordering::Relaxed)); // no ring, no tap → inactive
        let (tx, mut rx) = mpsc::channel(4);
        let dropped = Rc::new(Cell::new(0));
        hub.with_mut(|h| h.register(1, tx, dropped.clone(), false));
        assert!(active.load(Ordering::Relaxed)); // a tap makes it active

        // Deliver within the queue: all bytes arrive.
        for _ in 0..4 {
            hub.with_mut(|h| h.ingest(&Chunk::from_static(b"ab")));
        }
        assert_eq!(drain(&mut rx), b"abababab");
        assert_eq!(dropped.get(), 0);

        // Overfill the queue (cap 4, unread): the surplus is counted, not delivered.
        for _ in 0..6 {
            hub.with_mut(|h| h.ingest(&Chunk::from_static(b"XY")));
        }
        assert_eq!(dropped.get(), 4); // 2 chunks × 2 bytes dropped past the cap
    }

    #[test]
    fn replay_ring_splices_exactly() {
        let (hub, _active, _fd) = TapHub::new("usb0", 8);
        // Fill past the ring: it keeps the last 8 bytes.
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"0123456789")));
        let (tx, mut rx) = mpsc::channel(64);
        let dropped = Rc::new(Cell::new(0));
        let reg = hub.with_mut(|h| h.register(1, tx, dropped, true));
        assert_eq!(reg.replay_bytes, 8);
        // 10 bytes ingested, ring keeps the last 8 → replay begins at offset 2 (§11.8).
        assert_eq!(reg.from_offset, 2);
        // Live bytes after registration continue the stream.
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"abc")));
        // Ring tail (23456789) then live (abc): exact splice, no gap, no dup.
        assert_eq!(drain(&mut rx), b"23456789abc");
    }

    #[test]
    fn replay_off_yields_empty_marker() {
        let (hub, _active, _fd) = TapHub::new("usb0", 0);
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"data")));
        let (tx, _rx) = mpsc::channel(64);
        let dropped = Rc::new(Cell::new(0));
        let reg = hub.with_mut(|h| h.register(1, tx, dropped, true));
        assert_eq!(reg.replay_bytes, 0); // explicit empty-replay marker (§17)
        // No ring, but 4 bytes already flowed: the tap resumes at the live edge (§11.8).
        assert_eq!(reg.from_offset, 4);
    }

    #[test]
    fn close_removes_tap_and_clears_active() {
        let (hub, active, _fd) = TapHub::new("usb0", 0);
        let (tx, _rx) = mpsc::channel(4);
        let dropped = Rc::new(Cell::new(0));
        hub.with_mut(|h| h.register(7, tx, dropped, false));
        assert!(active.load(Ordering::Relaxed));
        hub.with_mut(|h| h.close(7));
        assert!(!active.load(Ordering::Relaxed));
        assert!(hub.with(|h| h.snapshot().taps.is_empty()));
    }

    // TAP-1b: bytes lost at the lossy producer→hub feed hop are invisible in the
    // offset space (which counts delivered bytes only, so invariant 10 and the
    // exact replay splice both survive), so the hub charges them to the *next*
    // chunk's `gap_before`. Without it a client splicing by offset concatenates a
    // holed stream and cannot tell.
    #[test]
    fn feed_loss_surfaces_as_gap_before_on_the_next_chunk() {
        let (hub, _active, feed_dropped) = TapHub::new("usb0", 0);
        let (tx, mut rx) = mpsc::channel(16);
        let dropped = Rc::new(Cell::new(0));
        hub.with_mut(|h| h.register(1, tx, dropped, false));

        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"aaaa")));
        // The producer's `mirror` found the feed full and dropped 7 bytes.
        feed_dropped.fetch_add(7, Ordering::Relaxed);
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"bb")));
        // No further loss: the gap is reported exactly once, not re-reported.
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"c")));

        assert_eq!(
            drain_data(&mut rx),
            vec![
                (0, 0, b"aaaa".to_vec()),
                (4, 7, b"bb".to_vec()),
                (6, 0, b"c".to_vec()),
            ]
        );
    }

    // Invariant 10, re-verified against the TAP-1b fix: feed-hop loss must not
    // enter `ingested`, or `from_offset = ingested − ring.len()` stops naming the
    // ring's true base offset (and could underflow). The ring stays contiguous in
    // offset space, so replay still splices exactly across a feed drop.
    #[test]
    fn feed_loss_does_not_shift_the_replay_offset_space() {
        let (hub, _active, feed_dropped) = TapHub::new("usb0", 8);
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"0123")));
        feed_dropped.fetch_add(1000, Ordering::Relaxed); // a firehose overrun
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"4567")));

        let (tx, mut rx) = mpsc::channel(64);
        let dropped = Rc::new(Cell::new(0));
        let reg = hub.with_mut(|h| h.register(1, tx, dropped, true));
        // 8 delivered bytes, an 8-byte ring: replay is the whole stream from 0.
        assert_eq!(reg.replay_bytes, 8);
        assert_eq!(reg.from_offset, 0);
        assert_eq!(reg.feed_dropped, 1000);
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"89")));
        assert_eq!(drain(&mut rx), b"0123456789");
    }

    // TAP-1: `teardown`/`load --replace`/`remove-node` drop the hub while taps are
    // open. The client's `OpenTap` handle survives, so the tap must be told —
    // terminal `tap.closed` on its own channel — and the hub must remember it was
    // orphaned so a later `tap.close` fails instead of reporting a hollow success.
    #[test]
    fn detach_all_notifies_open_taps_and_marks_the_hub_orphaned() {
        let (hub, active, _fd) = TapHub::new("mux/console", 64);
        let (tx, mut rx) = mpsc::channel(8);
        let dropped = Rc::new(Cell::new(0));
        hub.with_mut(|h| h.register(3, tx, dropped, false));
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"hi")));

        let ids = hub.with_mut(|h| h.detach_all(TapCloseReason::GraphReplaced));
        assert_eq!(ids, vec![3]);
        assert!(hub.with(TapHub::is_orphaned));
        // A ring alone used to keep the producer mirroring; an orphaned hub never does.
        assert!(!active.load(Ordering::Relaxed));
        assert!(hub.with(|h| h.snapshot().taps.is_empty()));

        // The data chunk, then the terminal close carrying endpoint and reason.
        assert!(matches!(rx.try_recv(), Ok(TapMsg::Data { .. })));
        match rx.try_recv() {
            Ok(TapMsg::Closed {
                tap_id,
                endpoint,
                reason,
            }) => {
                assert_eq!(tap_id, 3);
                assert_eq!(endpoint, "mux/console");
                assert_eq!(reason, "graph replaced");
            }
            _ => panic!("expected a terminal TapMsg::Closed"),
        }
    }
}
