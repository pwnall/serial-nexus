//! The data-plane contracts (design §5, §15.5).
//!
//! The interior is queue-free and policy-free; all buffering, dropping, and
//! flow control lives at the boundary types — serial ports, PTY masters,
//! sockets, files, and the exec codec's child stdio pipes (§3/§5/§15.22; the
//! last arrives with the phase-5 exec node). This module encodes the two
//! delivery contracts and the single-chunk holdover slot as pure, testable
//! types, with mock boundaries standing in for the real kernel objects that
//! arrive in later phases.
//!
//! * **Hostward** ([`HostFanout`]) is infallible and immediate: a host-facing
//!   endpoint broadcasts to every attached consumer, and each consuming
//!   boundary applies its own drop policy. A slow consumer costs only itself.
//! * **Targetward** ([`Origin`], [`TargetwardSink`]) returns [`Delivery`] and
//!   never blocks: `Busy` propagates back to the origin, which pauses (the
//!   kernel buffers on the client side; nothing is dropped). A transform that
//!   has already emitted output parks it in its [`Holdover`], capping interior
//!   memory at one chunk per direction.

use std::collections::VecDeque;

/// A unit of data moving through the graph. `Bytes` is cheap to clone, which is
/// what makes hostward broadcast to N consumers inexpensive.
pub type Chunk = bytes::Bytes;

/// The result of a targetward [`TargetwardSink::deliver_targetward`]: the byte
/// stream was accepted, or the path is full and the caller must pause. Never a
/// third state — targetward flow is delayed, never dropped (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Accepted,
    Busy,
}

impl Delivery {
    pub fn is_accepted(self) -> bool {
        matches!(self, Delivery::Accepted)
    }
    pub fn is_busy(self) -> bool {
        matches!(self, Delivery::Busy)
    }
}

// ---------------------------------------------------------------------------
// Hostward: infallible broadcast with per-consumer loss isolation (§5).
// ---------------------------------------------------------------------------

/// A hostward consumer boundary. `deliver_hostward` is infallible: the consumer
/// applies its own policy (bounded buffering, then counted drops) and never
/// signals back, so one slow consumer cannot stall its neighbors.
pub trait HostwardConsumer {
    fn deliver_hostward(&mut self, chunk: &Chunk);
}

/// A host-facing endpoint: broadcasts hostward data to every attached consumer
/// (§4 rule 2, fan-out is implicit). Generic over the consumer type so the same
/// fan-out serves mocks in tests and real boundaries in the daemon.
pub struct HostFanout<C: HostwardConsumer> {
    consumers: Vec<C>,
}

impl<C: HostwardConsumer> HostFanout<C> {
    pub fn new() -> Self {
        HostFanout {
            consumers: Vec::new(),
        }
    }

    pub fn attach(&mut self, consumer: C) -> usize {
        self.consumers.push(consumer);
        self.consumers.len() - 1
    }

    pub fn consumers(&self) -> &[C] {
        &self.consumers
    }

    pub fn consumer(&self, i: usize) -> &C {
        &self.consumers[i]
    }

    /// Broadcast a chunk to every attached consumer, synchronously and
    /// infallibly (§5).
    pub fn broadcast(&mut self, chunk: &Chunk) {
        for c in &mut self.consumers {
            c.deliver_hostward(chunk);
        }
    }
}

impl<C: HostwardConsumer> Default for HostFanout<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// A mock consuming boundary: a byte-bounded buffer that drops (with a counter)
/// once full, exactly as a real slow PTY/socket boundary would (§5). Used to
/// prove per-consumer loss isolation.
#[derive(Debug, Clone)]
pub struct MockConsumer {
    capacity: usize,
    buffered: Vec<u8>,
    dropped: u64,
}

impl MockConsumer {
    /// A consumer that can hold `capacity` bytes before dropping. `capacity == usize::MAX`
    /// is an effectively unbounded (fast) consumer.
    pub fn with_capacity(capacity: usize) -> Self {
        MockConsumer {
            capacity,
            buffered: Vec::new(),
            dropped: 0,
        }
    }

    pub fn unbounded() -> Self {
        Self::with_capacity(usize::MAX)
    }

    pub fn received(&self) -> &[u8] {
        &self.buffered
    }

    pub fn received_len(&self) -> usize {
        self.buffered.len()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

impl HostwardConsumer for MockConsumer {
    fn deliver_hostward(&mut self, chunk: &Chunk) {
        // Whole-chunk admission: buffer it if it fits, else count the whole
        // chunk as dropped. Loss is located and counted, never silent (§5).
        if self.buffered.len().saturating_add(chunk.len()) <= self.capacity {
            self.buffered.extend_from_slice(chunk);
        } else {
            self.dropped += chunk.len() as u64;
        }
    }
}

// ---------------------------------------------------------------------------
// Targetward: Accepted/Busy backpressure with a one-chunk holdover (§5).
// ---------------------------------------------------------------------------

/// A targetward sink: the next hop toward the target. Returns [`Delivery`] and
/// never blocks.
pub trait TargetwardSink {
    fn deliver_targetward(&mut self, chunk: Chunk) -> Delivery;

    /// Attempt to drain any internally-parked holdover, e.g. after a downstream
    /// boundary signals it became writable. The runtime calls this on resume
    /// *independent of new input* — otherwise a chunk parked on the last offer
    /// would sit forever when the origin has nothing more to push. Boundaries
    /// with no holdover need not override it.
    fn flush(&mut self) {}
}

/// The single-chunk holdover slot (§5): an interior transform that has already
/// emitted output when downstream refuses parks exactly one chunk here, capping
/// interior memory at one frame per direction. It is *not* a queue.
#[derive(Debug, Default)]
pub struct Holdover {
    slot: Option<Chunk>,
}

impl Holdover {
    pub fn is_parked(&self) -> bool {
        self.slot.is_some()
    }

    pub fn held_bytes(&self) -> usize {
        self.slot.as_ref().map_or(0, Chunk::len)
    }

    fn park(&mut self, chunk: Chunk) {
        debug_assert!(self.slot.is_none(), "holdover holds at most one chunk");
        self.slot = Some(chunk);
    }

    fn take(&mut self) -> Option<Chunk> {
        self.slot.take()
    }
}

/// A mock target boundary that can be toggled Busy (standing in for a full
/// kernel object). Records every byte that reaches it, in order, so tests can
/// assert no loss / duplication / reordering.
#[derive(Debug, Default)]
pub struct BusyBoundary {
    busy: bool,
    received: Vec<u8>,
}

impl BusyBoundary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    pub fn received(&self) -> &[u8] {
        &self.received
    }
}

impl TargetwardSink for BusyBoundary {
    fn deliver_targetward(&mut self, chunk: Chunk) -> Delivery {
        if self.busy {
            Delivery::Busy
        } else {
            self.received.extend_from_slice(&chunk);
            Delivery::Accepted
        }
    }
}

/// An interior transform forwarding targetward through a [`Holdover`]. It
/// consumes input, produces output (identity here — a real codec reframes), and
/// forwards downstream. When downstream refuses, it parks the *output* and
/// still reports `Accepted` (the input was consumed); it reports `Busy` only
/// when the holdover is full and cannot drain, refusing new input. Thus it
/// holds at most one emitted chunk — never a queue.
pub struct InteriorTargetward<S: TargetwardSink> {
    downstream: S,
    holdover: Holdover,
}

impl<S: TargetwardSink> InteriorTargetward<S> {
    pub fn new(downstream: S) -> Self {
        InteriorTargetward {
            downstream,
            holdover: Holdover::default(),
        }
    }

    pub fn downstream(&self) -> &S {
        &self.downstream
    }

    pub fn held_bytes(&self) -> usize {
        self.holdover.held_bytes()
    }

    /// Try to push the parked chunk downstream. Returns true if the holdover is
    /// now empty (nothing parked, or it drained), false if it is still stuck.
    fn flush_holdover(&mut self) -> bool {
        if let Some(chunk) = self.holdover.take() {
            match self.downstream.deliver_targetward(chunk.clone()) {
                Delivery::Accepted => true,
                Delivery::Busy => {
                    self.holdover.park(chunk);
                    false
                }
            }
        } else {
            true
        }
    }
}

impl<S: TargetwardSink> TargetwardSink for InteriorTargetward<S> {
    fn deliver_targetward(&mut self, chunk: Chunk) -> Delivery {
        // Drain any parked output first (preserving order). If it won't go, we
        // cannot accept new input — report Busy and hold nothing new.
        if self.holdover.is_parked() && !self.flush_holdover() {
            return Delivery::Busy;
        }
        // Identity transform: output == input. Consume the input; if downstream
        // refuses the output, park it but still acknowledge the input.
        match self.downstream.deliver_targetward(chunk.clone()) {
            Delivery::Accepted => Delivery::Accepted,
            Delivery::Busy => {
                self.holdover.park(chunk);
                Delivery::Accepted
            }
        }
    }

    fn flush(&mut self) {
        // Propagate downstream first (its holdover may gate ours), then drain
        // our own parked chunk.
        self.downstream.flush();
        let _ = self.flush_holdover();
    }
}

/// An origin: a hostward boundary through which bytes enter the graph traveling
/// targetward (§3). It reads its kernel object into `pending` (modeling bytes
/// buffered on the client side of the fence) and offers them to the path; on
/// `Busy` it pauses and stops draining until resumed — nothing is dropped (§5).
pub struct Origin<S: TargetwardSink> {
    sink: S,
    pending: VecDeque<Chunk>,
    paused: bool,
    offered: usize,
}

impl<S: TargetwardSink> Origin<S> {
    pub fn new(sink: S) -> Self {
        Origin {
            sink,
            pending: VecDeque::new(),
            paused: false,
            offered: 0,
        }
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Bytes still buffered on the client side (not yet accepted by the path).
    pub fn pending_bytes(&self) -> usize {
        self.pending.iter().map(Chunk::len).sum()
    }

    pub fn total_offered(&self) -> usize {
        self.offered
    }

    /// The client produced a chunk; buffer it and try to drain toward the path.
    pub fn offer(&mut self, chunk: Chunk) {
        self.offered += chunk.len();
        self.pending.push_back(chunk);
        self.pump();
    }

    /// Retry draining pending bytes (e.g. after downstream reports it drained).
    pub fn resume(&mut self) {
        self.pump();
    }

    fn pump(&mut self) {
        // First drain any holdover parked along the path (a chunk emitted on an
        // earlier offer while downstream was busy); only then push new input, so
        // ordering is preserved and nothing is stranded when `pending` is empty.
        self.sink.flush();
        while let Some(front) = self.pending.front().cloned() {
            match self.sink.deliver_targetward(front) {
                Delivery::Accepted => {
                    self.pending.pop_front();
                    self.paused = false;
                }
                Delivery::Busy => {
                    self.paused = true;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn chunk(bytes: &[u8]) -> Chunk {
        Chunk::copy_from_slice(bytes)
    }

    // --- Hostward -----------------------------------------------------------

    #[test]
    fn hostward_broadcast_reaches_every_consumer() {
        let mut fanout = HostFanout::new();
        let a = fanout.attach(MockConsumer::unbounded());
        let b = fanout.attach(MockConsumer::unbounded());
        let c = fanout.attach(MockConsumer::unbounded());
        fanout.broadcast(&chunk(b"hello"));
        for i in [a, b, c] {
            assert_eq!(fanout.consumer(i).received(), b"hello");
        }
    }

    #[test]
    fn slow_consumer_costs_only_itself() {
        // One zero-capacity (slow) consumer alongside a fast one: the slow one
        // drops everything; the fast one loses nothing (§5 isolation).
        let mut fanout = HostFanout::new();
        let slow = fanout.attach(MockConsumer::with_capacity(0));
        let fast = fanout.attach(MockConsumer::unbounded());
        for _ in 0..100 {
            fanout.broadcast(&chunk(b"0123456789"));
        }
        assert_eq!(fanout.consumer(slow).received_len(), 0);
        assert_eq!(fanout.consumer(slow).dropped(), 1000);
        assert_eq!(fanout.consumer(fast).received_len(), 1000);
        assert_eq!(fanout.consumer(fast).dropped(), 0);
    }

    // --- Targetward ---------------------------------------------------------

    #[test]
    fn targetward_busy_pauses_the_offering_origin_only() {
        // Two independent origin→boundary paths. Make one boundary busy; only
        // that origin pauses.
        let mut busy_boundary = BusyBoundary::new();
        busy_boundary.set_busy(true);
        let mut stalled = Origin::new(busy_boundary);
        let mut flowing = Origin::new(BusyBoundary::new());

        stalled.offer(chunk(b"cmd"));
        flowing.offer(chunk(b"cmd"));

        assert!(stalled.is_paused(), "origin into a busy path must pause");
        assert!(
            !flowing.is_paused(),
            "independent origin must be unaffected"
        );
        assert_eq!(flowing.sink().received(), b"cmd");
        assert_eq!(
            stalled.sink().received(),
            b"",
            "nothing reaches a busy target"
        );
        assert_eq!(
            stalled.pending_bytes(),
            3,
            "paused bytes are held, not dropped"
        );
    }

    #[test]
    fn paused_origin_drains_in_order_on_resume() {
        let mut origin = Origin::new(InteriorTargetward::new(BusyBoundary::new()));
        origin.set_boundary_busy(true);

        origin.offer(chunk(b"AAA"));
        origin.offer(chunk(b"BBB"));
        origin.offer(chunk(b"CCC"));
        assert!(origin.is_paused());
        assert_eq!(
            origin.sink().downstream().received(),
            b"",
            "nothing reaches a busy target"
        );

        // The kernel drains: clear busy and resume. Everything arrives in order.
        origin.set_boundary_busy(false);
        origin.resume();
        assert!(!origin.is_paused());
        assert_eq!(origin.pending_bytes(), 0);
        assert_eq!(origin.sink().downstream().received(), b"AAABBBCCC");
    }

    #[test]
    fn interior_holds_at_most_one_chunk() {
        let mut boundary = BusyBoundary::new();
        boundary.set_busy(true);
        let mut interior = InteriorTargetward::new(boundary);
        // First chunk: consumed, output parked (Accepted, holdover holds 3).
        assert_eq!(
            interior.deliver_targetward(chunk(b"AAA")),
            Delivery::Accepted
        );
        assert_eq!(interior.held_bytes(), 3);
        // Second chunk: holdover full and stuck → Busy, nothing new held.
        assert_eq!(interior.deliver_targetward(chunk(b"BBBBB")), Delivery::Busy);
        assert_eq!(
            interior.held_bytes(),
            3,
            "still only the first output parked"
        );
    }

    // --- Property tests -----------------------------------------------------

    proptest! {
        /// A fast consumer receives the full concatenated stream regardless of
        /// how many slow neighbors share the fan-out (loss isolation, §5).
        #[test]
        fn prop_fast_consumer_never_loses(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..64), 0..64),
            slow_caps in prop::collection::vec(0usize..8, 0..5),
        ) {
            let mut fanout = HostFanout::new();
            let fast = fanout.attach(MockConsumer::unbounded());
            let slow: Vec<usize> = slow_caps.iter().map(|c| fanout.attach(MockConsumer::with_capacity(*c))).collect();

            let mut expected = Vec::new();
            for c in &chunks {
                expected.extend_from_slice(c);
                fanout.broadcast(&Chunk::copy_from_slice(c));
            }
            prop_assert_eq!(fanout.consumer(fast).received(), expected.as_slice());
            // Every slow consumer's received + dropped accounts for the whole
            // stream — loss is located and counted, never silent.
            for &s in &slow {
                let cons = fanout.consumer(s);
                prop_assert_eq!(cons.received_len() as u64 + cons.dropped(), expected.len() as u64);
            }
        }

        /// Under any busy/resume schedule, a targetward path loses, duplicates,
        /// and reorders nothing once fully drained, and the interior never holds
        /// more than one chunk.
        #[test]
        fn prop_targetward_no_loss_bounded_interior(
            chunks in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..32), 1..48),
            busy_schedule in prop::collection::vec(any::<bool>(), 1..48),
        ) {
            let mut path = Origin::new(InteriorTargetward::new(BusyBoundary::new()));
            let mut expected = Vec::new();
            let mut max_held = 0usize;

            for (i, c) in chunks.iter().enumerate() {
                // Toggle boundary busy per the schedule before each offer.
                let busy = busy_schedule[i % busy_schedule.len()];
                path.set_boundary_busy(busy);
                if !busy { path.resume(); }
                expected.extend_from_slice(c);
                path.offer(Chunk::copy_from_slice(c));
                max_held = max_held.max(path.sink().held_bytes());
            }
            // Finally un-busy and drain everything.
            path.set_boundary_busy(false);
            path.resume();

            prop_assert_eq!(path.sink().downstream().received(), expected.as_slice());
            prop_assert!(path.pending_bytes() == 0, "all bytes drained");
            // The interior holdover holds at most one input chunk (<= 31 bytes here).
            prop_assert!(max_held <= 31, "interior held {} bytes (> one chunk)", max_held);
        }
    }

    // Test-only controls to reach the innermost boundary through the chain.
    impl Origin<InteriorTargetward<BusyBoundary>> {
        fn set_boundary_busy(&mut self, busy: bool) {
            self.sink.downstream_boundary_mut().set_busy(busy);
        }
    }

    impl InteriorTargetward<BusyBoundary> {
        fn downstream_boundary_mut(&mut self) -> &mut BusyBoundary {
            &mut self.downstream
        }
    }
}
