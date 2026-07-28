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
//! tap is open — and charging whatever it does *not* mirror to the shared
//! `feed_dropped`, so the hole those bytes leave in the offset space is announced
//! rather than spliced across — [`TapFeed::mirror`]), a hub task drains it and
//! calls [`TapHub::ingest`], and the daemon
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
/// `from_offset` is the offset of the first *delivered* replay byte — the ring's oldest
/// retained byte, or, when the ring is deeper than the connection can be handed at once,
/// the oldest byte of the newest slice that fits (TAP-1); otherwise it is the current
/// live edge, so the next `tap.data` this tap sees carries exactly that offset. Either
/// way `from_offset + replay_bytes` lands exactly on the live edge.
pub struct Registered {
    pub replay_bytes: u64,
    /// Ring bytes `--replay` could **not** hand this tap: the snapshot's head, trimmed
    /// because the connection's bounded tap channel had room for fewer bytes than the
    /// ring holds (TAP-1). Zero in the ordinary case. What is delivered is always the
    /// *newest* end of the ring and always contiguous with the live stream, so this is
    /// information — "this endpoint's ring is deeper than one `tap.open` can deliver" —
    /// and not a hole in the offset space, which is why it is reported apart from
    /// `feed_dropped`.
    pub replay_truncated: u64,
    pub from_offset: u64,
    /// The offset space `from_offset` is expressed in (§11.8, §15.38). Stable for
    /// this hub's whole life and never reused, so a client that persists it beside
    /// its scrollback knows — rather than infers — when the space restarted.
    pub epoch: u64,
    /// Bytes this endpoint has lost at the producer→hub feed hop **and already charged
    /// to its taps** (§5) — the hub's reported-so-far watermark, not the raw counter
    /// `state` renders. The offset space counts only delivered bytes, so this is the
    /// client's baseline for the `gap_before` deltas that follow (TAP-1b).
    ///
    /// Loss recorded since the hub's last chunk is deliberately *excluded*: [`TapHub::ingest`]
    /// charges that same delta to every registered tap — this one included, it having
    /// just joined the fan-out list — as the next chunk's `gap_before`. Reporting the
    /// raw atomic here handed those bytes to a tap twice, once in its documented
    /// baseline and once as its first gap, so `baseline + Σgap_before` exceeded the
    /// endpoint's true loss (TAP-4). Advancing the watermark in `register` instead
    /// would be wrong in the other direction: it would swallow the gap for the taps
    /// that were already open.
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
/// hostward chunks. Mirroring happens only while `active`; an untapped, ring-less
/// endpoint costs one relaxed load plus one relaxed add per chunk, because the bytes
/// it does *not* mirror are still charged so the hole they leave in the offset space
/// is announced (TAP-2 — see [`TapFeed::mirror`]). Cloned into each producer at `start`.
#[derive(Clone)]
pub struct TapFeed {
    pub tx: mpsc::Sender<Chunk>,
    pub active: Arc<AtomicBool>,
    /// Hostward bytes this producer→hub feed hop did not carry: the feed was full (the
    /// hub fell behind across a scheduling gap under a firehose), **or** nothing was
    /// listening at all (no ring and no tap — the `replay_ring = 0` window between one
    /// `tap.close` and the next `tap.open`, TAP-2). Shared with the hub so `state`
    /// surfaces it (§5 all-loss-counted) and so [`TapHub::ingest`] can turn it into the
    /// next chunk's `gap_before`. Never backpressures the producer: the ring must not
    /// stall the device (§5).
    pub feed_dropped: Arc<AtomicU64>,
}

impl TapFeed {
    /// Mirror one hostward chunk to the hub if a tap or ring wants it, and charge it
    /// when it does not get there. Lossy by construction: the ring never backpressures
    /// the producer (§5), so a full feed drops — and that drop is counted
    /// (`feed_dropped`) so §5's "loss is always counted" holds even for this internal
    /// hop. A dropped chunk shows as a gap in the ring / every tap; the exact-splice
    /// guarantee is between the snapshot and the live stream, not a promise the ring
    /// never loses under a firehose. The counter is *shared* with the hub, which turns
    /// it into the per-chunk `gap_before` a tap client sees ([`TapHub::ingest`]) — so
    /// the hole is visible, not merely tallied.
    ///
    /// **Both ways a chunk fails to arrive are the same hole** (TAP-2). It can find the
    /// bounded feed full, or it can find nobody listening at all — no ring and no open
    /// tap, which on a `replay_ring = 0` endpoint is the entire window between one
    /// `tap.close` and the next `tap.open`. Charging only the first left the second as
    /// a third category invariant 10 has no room for: not delivered (so absent from the
    /// offset space, which counts delivered bytes only) *and* not counted (so absent
    /// from `gap_before`), which let a returning client splice its stored scrollback
    /// flush against a stream missing an hour, with nothing anywhere to say so. Both
    /// land on the one counter now, whose meaning is therefore "hostward bytes this
    /// endpoint's feed did not carry" rather than "bytes the feed overflowed on". That
    /// conflation is the deliberate price of announcing the hole through machinery
    /// every client already has, instead of inventing a second signal for it; the
    /// *size* of the hole, which is what a consumer acts on, is exact either way.
    ///
    /// The cost is one relaxed add per chunk on an untapped, ring-less endpoint. §5's
    /// "costs nothing when unset" was already a load plus a branch once §15.32 put a
    /// ring on every endpoint by default, and the add is negligible beside the read
    /// that produced the chunk.
    pub fn mirror(&self, chunk: &Chunk) {
        // Short-circuit order matters: `try_send` runs only when the feed is wanted.
        if !self.active.load(Ordering::Relaxed) || self.tx.try_send(chunk.clone()).is_err() {
            self.feed_dropped
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
    }

    /// Whether a tap or ring currently wants the hostward stream — the cheap check
    /// a producer makes before creating a chunk purely for the tap path.
    ///
    /// A producer that answers `false` here **and** has no graph sink either may skip
    /// building the [`Chunk`] altogether (`serial.rs`'s read-and-discard arm), in which
    /// case [`Self::mirror`] is never reached and the bytes must be charged with
    /// [`Self::skipped`] at the skip site instead — they are the same hole to a client
    /// splicing by offset as the ones an inactive feed refused (TAP-2).
    pub fn wanted(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Charge `n` hostward bytes that never became a [`Chunk`] at all — the skip-site
    /// twin of [`Self::mirror`]'s inactive-feed branch, and the second half of TAP-2.
    ///
    /// [`Self::wanted`] exists so a producer with nothing attached can avoid the
    /// allocation, and the *only* producer that takes it is the serial reader's
    /// read-and-discard arm (`hostward.is_empty() && !tap_wanted`). Because it never
    /// reaches [`Self::mirror`], the bytes of that window used to be charged to
    /// `discarded_unattached` and to nothing else: not delivered, so absent from the
    /// offset space (correctly — invariant 10 counts delivered bytes), and not on
    /// `feed_dropped`, so absent from `gap_before` too. A client returning to a
    /// `replay_ring = 0` endpoint therefore read `from_offset` at the previous
    /// frontier, the same `epoch`, `feed_dropped: 0` and a first chunk with
    /// `gap_before: 0`, and spliced its stored scrollback against a stream missing the
    /// whole dark window in silence.
    ///
    /// It is the *same* hole as a refused mirror and lands on the same counter, so the
    /// hub turns it into the next chunk's `gap_before` with no new field and no new
    /// client work. The two counters are independent and both are right: the bytes are
    /// unattached (no graph consumer took them) **and** they are a gap in the feed (no
    /// hub saw them) — the ring is a spy outside the graph, so its accounting never
    /// substitutes for the graph's (§5, invariant 9).
    ///
    /// Charge the bytes read, not a chunk length: at the skip site there is no chunk,
    /// which is the entire point of the entry point.
    pub fn skipped(&self, n: u64) {
        self.feed_dropped.fetch_add(n, Ordering::Relaxed);
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
    /// Hostward bytes the producer→hub feed did not carry — overflow, or nobody
    /// listening (TAP-2) — shared with the [`TapFeed`] that counts them (§5);
    /// surfaced in `state`.
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
    /// This hub's identity, and therefore its **offset space**'s (§11.8). Unique per
    /// hub instance within a daemon process, monotonic, never reused.
    ///
    /// `ingested` is monotonic only for as long as *this* hub lives, and a hub is
    /// rebuilt by `load --replace`, `add-node` and `remove-node` — after which
    /// offsets restart at 0 while the per-*boot* `info.instance` nonce, quite
    /// correctly, does not change. A client holding stored scrollback therefore had
    /// no way to tell "the ring is replaying bytes I already have" (the ordinary
    /// case, which its offsets already handle) from "the space restarted beneath me"
    /// (where its frontier is meaningless), and inferring one from the other is not
    /// possible: both look like `from_offset < my frontier`. Guessing sat in the
    /// browser client for two revisions and was wrong in both directions — it
    /// duplicated the ring into stored scrollback on every ordinary reload, and it
    /// would have frozen a console outright had it guessed the other way.
    ///
    /// So the daemon says which space it is talking about, and the question stops
    /// being a guess. Reported by `tap.open`; a client persists it beside its
    /// scrollback and re-anchors exactly when it changes (§15.38).
    epoch: u64,
}

/// Source of [`TapHub::epoch`]. Process-global and monotonic: uniqueness across the
/// daemon's whole life is the property clients depend on, so a per-endpoint counter
/// would be wrong — an endpoint removed and re-added would repeat an epoch and a
/// client would splice across the seam it exists to mark.
static NEXT_HUB_EPOCH: AtomicU64 = AtomicU64::new(1);

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
            epoch: NEXT_HUB_EPOCH.fetch_add(1, Ordering::Relaxed),
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
    /// is enough for a consumer to detect it, which is what §5 requires. The same
    /// mechanism now also carries the bytes an *inactive* feed never mirrored
    /// (TAP-2), whose position is exact — the feed is empty by definition while
    /// nothing is listening — and which reach the next tap to open as its first
    /// chunk's `gap_before`.
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
    ///
    /// **A snapshot too large for the connection is trimmed at its head, never its
    /// tail** (TAP-1). `out` is the connection's bounded tap channel and the only task
    /// that drains it is the one blocked in this very critical section, so what a tap
    /// can actually be handed is whatever `out` has free *right now*: [`TAP_QUEUE_CAP`]
    /// pieces of [`REPLAY_PIECE`] at the very most, and fewer on a connection already
    /// carrying another tap's backlog. A ring deeper than that budget therefore cannot
    /// be delivered whole — which is not itself a defect. Discarding the overflow *as
    /// it was reached* was: because the snapshot walks oldest-first, the bytes kept
    /// were the oldest and the bytes discarded were the newest — the seconds before the
    /// incident, which is the entire reason `--replay` exists — and `from_offset +
    /// replay_bytes` landed short of the live edge, so a client spliced stale
    /// scrollback straight onto the live stream with an unmarked hole between them.
    /// Taking the newest `budget` bytes instead keeps the splice exact and contiguous
    /// *by construction*: a shorter replay is precisely what a bounded delivery means.
    /// The shortfall is reported as [`Registered::replay_truncated`] rather than left
    /// for a client to infer.
    pub fn register(
        &mut self,
        id: u64,
        out: mpsc::Sender<TapMsg>,
        dropped: Rc<Cell<u64>>,
        replay: bool,
    ) -> Registered {
        let mut replay_bytes = 0u64;
        let mut replay_truncated = 0u64;
        // Where this tap's stream begins (§11.8). With a non-empty deliverable slice
        // under `--replay` it is the offset of that slice's oldest byte; otherwise it is
        // the live edge, so the tap's first live `tap.data` carries exactly this offset.
        let mut from_offset = self.ingested;
        if replay && let Some(ring) = &self.ring {
            let snap = ring.snapshot();
            // The budget is the connection's *free* channel space expressed in bytes:
            // each piece takes one slot and carries at most `REPLAY_PIECE`. Exact rather
            // than a guess — nothing else runs on this thread inside the critical
            // section, so no slot fills or frees beneath us.
            let budget = out.capacity().saturating_mul(REPLAY_PIECE);
            let deliver = snap.len().min(budget);
            replay_truncated = (snap.len() - deliver) as u64;
            // Keep the newest `deliver` bytes: the ring's head is what a bounded
            // delivery gives up, so that the tail meets the live stream exactly.
            let snap = &snap[snap.len() - deliver..];
            // `deliver <= ring.len() <= ingested` (the ring holds only ingested bytes —
            // invariant 10), so this cannot underflow.
            from_offset = self.ingested - deliver as u64;
            // Offset walks the stream position of each replay piece — advancing even
            // past a dropped piece, so a client that loses one still splices the rest
            // at the right offset (a gap it can see, never a silent shift). The budget
            // above makes `Full` unreachable; the arm stays because a truthful offset
            // costs less than trusting that across a refactor.
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
        let registered = Registered {
            replay_bytes,
            replay_truncated,
            from_offset,
            epoch: self.epoch,
            // The watermark the hub has already charged its taps, not the raw atomic
            // (TAP-4): the not-yet-charged delta reaches this tap as its first
            // `gap_before`, so reporting it here too would count it twice.
            feed_dropped: self.feed_dropped_seen,
        };
        if registered.replay_truncated > 0 {
            // Not an error — a bounded delivery doing what it says — but an operator
            // who set a 16 MiB ring and got 8 MiB deserves to be told where the other
            // half went rather than left to infer it from `replay_bytes`, which cannot
            // distinguish a short ring from a short channel.
            tracing::debug!(
                endpoint = %self.endpoint,
                replay_bytes = registered.replay_bytes,
                replay_truncated = registered.replay_truncated,
                "replay trimmed at its head to the connection's free tap-channel space"
            );
        }
        registered
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
    /// Hostward bytes the producer→hub feed did not carry for this endpoint (§5):
    /// overflow under a firehose, or nothing listening on a ring-less endpoint
    /// (TAP-2). The running total, unlike [`Registered::feed_dropped`]'s watermark.
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

        // TAP-4: a tap opening in the window between the drop and the next chunk must
        // *not* find those 7 bytes in its baseline, because `ingest` is about to charge
        // the very same delta to every registered tap — this one included — as
        // `gap_before`. Its baseline is the hub's reported-so-far watermark, so
        // `baseline + Σgap_before` is the endpoint's true loss and not twice it.
        let (tx2, mut rx2) = mpsc::channel(16);
        let late = hub.with_mut(|h| h.register(2, tx2, Rc::new(Cell::new(0)), false));
        assert_eq!(
            late.feed_dropped, 0,
            "the tap.open baseline double-counted a gap not yet charged (TAP-4)"
        );
        assert_eq!(late.from_offset, 4);

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
        // The late tap missed "aaaa" entirely and reads the hole as its first chunk's
        // signal: 0 (baseline) + 7 = 7, exactly what the endpoint lost.
        assert_eq!(
            drain_data(&mut rx2),
            vec![(4, 7, b"bb".to_vec()), (6, 0, b"c".to_vec())]
        );
    }

    // TAP-2: with no ring and no tap the feed is inactive, and the bytes it therefore
    // never carries are charged to the same shared counter an overflow is — so the hole
    // they leave in the offset space is announced as the next tap's first `gap_before`
    // instead of being spliced across in silence. The offset space itself must NOT
    // absorb them (invariant 10): `from_offset` stays on the previous frontier.
    #[test]
    fn bytes_an_inactive_feed_never_mirrored_are_counted_and_reach_the_next_tap() {
        let (hub, active, feed_dropped) = TapHub::new("usb0", 0);
        let (feed_tx, _feed_rx) = mpsc::channel(TAP_FEED_CAP);
        let feed = TapFeed {
            tx: feed_tx,
            active: active.clone(),
            feed_dropped: feed_dropped.clone(),
        };

        // One tap's worth of stream, then the tap goes away and the feed falls idle.
        let (tx, mut rx) = mpsc::channel(16);
        hub.with_mut(|h| h.register(1, tx, Rc::new(Cell::new(0)), false));
        assert!(feed.wanted());
        feed.mirror(&Chunk::from_static(b"seen"));
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"seen")));
        assert_eq!(drain_data(&mut rx), vec![(0, 0, b"seen".to_vec())]);
        hub.with_mut(|h| h.close(1));
        assert!(!feed.wanted()); // no ring, no tap

        // The `replay_ring = 0` window: nine bytes the hub never sees.
        feed.mirror(&Chunk::from_static(b"unwatched"));
        assert_eq!(
            feed_dropped.load(Ordering::Relaxed),
            9,
            "bytes an inactive feed dropped went uncounted (TAP-2)"
        );

        // The next tap resumes on the old frontier — the missing bytes are not folded
        // into the offset space — with a baseline that excludes the pending delta.
        let (tx2, mut rx2) = mpsc::channel(16);
        let reg = hub.with_mut(|h| h.register(2, tx2, Rc::new(Cell::new(0)), false));
        assert_eq!(reg.from_offset, 4);
        assert_eq!(reg.feed_dropped, 0);

        // …and the hole reaches it as the first live chunk's `gap_before`.
        feed.mirror(&Chunk::from_static(b"live"));
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"live")));
        assert_eq!(drain_data(&mut rx2), vec![(4, 9, b"live".to_vec())]);
    }

    // TAP-2's other half: the producer that answers `wanted() == false` and has no graph
    // sink either never builds the `Chunk` at all, so `mirror` is not on its path and
    // only `skipped` can charge the window. The hole must be indistinguishable from the
    // one an inactive feed refused — same counter, same `gap_before`, same untouched
    // offset space — because to a client splicing by offset it *is* the same hole.
    #[test]
    fn bytes_a_producer_never_built_a_chunk_for_are_the_same_hole() {
        let (hub, active, feed_dropped) = TapHub::new("usb0", 0);
        let (feed_tx, _feed_rx) = mpsc::channel(TAP_FEED_CAP);
        let feed = TapFeed {
            tx: feed_tx,
            active: active.clone(),
            feed_dropped: feed_dropped.clone(),
        };

        let (tx, mut rx) = mpsc::channel(16);
        hub.with_mut(|h| h.register(1, tx, Rc::new(Cell::new(0)), false));
        feed.mirror(&Chunk::from_static(b"seen"));
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"seen")));
        assert_eq!(drain_data(&mut rx), vec![(0, 0, b"seen".to_vec())]);
        hub.with_mut(|h| h.close(1));
        assert!(!feed.wanted(), "no ring, no tap: the producer may skip");

        // The read-and-discard arm: bytes off the device, no chunk, one charge.
        feed.skipped(9);
        assert_eq!(feed_dropped.load(Ordering::Relaxed), 9);

        let (tx2, mut rx2) = mpsc::channel(16);
        let reg = hub.with_mut(|h| h.register(2, tx2, Rc::new(Cell::new(0)), false));
        assert_eq!(reg.from_offset, 4, "the offset space did not absorb them");
        assert_eq!(
            reg.feed_dropped, 0,
            "the baseline excludes the pending delta"
        );

        feed.mirror(&Chunk::from_static(b"live"));
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"live")));
        assert_eq!(drain_data(&mut rx2), vec![(4, 9, b"live".to_vec())]);
    }

    // TAP-1: a ring deeper than the connection's free tap-channel space is trimmed at
    // its *head*, so the tap is handed the newest bytes and `from_offset +
    // replay_bytes` still lands exactly on the live edge. Discarding the overflow as it
    // was reached kept the oldest bytes and left an unmarked hole immediately before
    // the live edge — the part of the scrollback an operator opens `--replay` for.
    #[test]
    fn oversize_replay_delivers_the_newest_bytes_and_still_splices_exactly() {
        // A ring five pieces deep on a connection with room for two.
        let cap = 5 * REPLAY_PIECE;
        let (hub, _active, _fd) = TapHub::new("usb0", cap);
        let filler = vec![0u8; cap];
        hub.with_mut(|h| h.ingest(&Chunk::copy_from_slice(&filler)));
        // The newest piece is distinguishable from everything before it.
        let newest = vec![0xABu8; REPLAY_PIECE];
        hub.with_mut(|h| h.ingest(&Chunk::copy_from_slice(&newest)));
        let ingested = (cap + REPLAY_PIECE) as u64;

        let (tx, mut rx) = mpsc::channel(2);
        let dropped = Rc::new(Cell::new(0));
        let reg = hub.with_mut(|h| h.register(1, tx, dropped.clone(), true));

        assert_eq!(reg.replay_bytes, 2 * REPLAY_PIECE as u64);
        assert_eq!(reg.replay_truncated, 3 * REPLAY_PIECE as u64);
        assert_eq!(
            reg.from_offset + reg.replay_bytes,
            ingested,
            "a trimmed replay must still end on the live edge (TAP-1)"
        );
        assert_eq!(
            dropped.get(),
            0,
            "the head trim is a bounded delivery, not a tap drop"
        );

        let got = drain(&mut rx);
        assert_eq!(got.len(), 2 * REPLAY_PIECE);
        assert!(
            got[REPLAY_PIECE..].iter().all(|&b| b == 0xAB),
            "the trim kept the oldest bytes instead of the newest (TAP-1)"
        );

        // And the live stream continues from exactly where the replay stopped.
        hub.with_mut(|h| h.ingest(&Chunk::from_static(b"live")));
        assert_eq!(drain_data(&mut rx), vec![(ingested, 0, b"live".to_vec())]);
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
