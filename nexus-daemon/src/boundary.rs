//! Boundary-supervisor library (design §16.1, plan §9.1).
//!
//! Every boundary node whose environment can come and go — the serial port
//! (§7.1), the exec child (§7.6), the leg socket (§7.4) — hand-rolled the same
//! lifecycle: spawn/connect, pump two independent directions as concurrently-polled
//! halves, park a half whose producer is exhausted instead of tearing the whole
//! node down, notify the supervisor on loss, join the worker before transitioning
//! status, back off, and retry. Three of the project's worst audit findings — the
//! exec-pump deadlock (§15.22), the leg stale-status wedge (§15.24), and the
//! waiting-serial targetward drain — were per-node re-derivations of exactly these
//! rules gone wrong (design §16). This module states them once, property-tested
//! once, so the next boundary node inherits them by construction rather than
//! rediscovering them by hand.
//!
//! The primitives, mapped to the four invariants §16.1 names:
//! * **park-don't-teardown** → [`park`]: a direction whose producer is exhausted
//!   awaits this instead of returning, so its independent sibling stays live.
//! * **concurrent halves** → [`race3`]: run the directions as
//!   concurrently-polled futures and return the first *session-ending* outcome —
//!   deadlock-free by construction, since a parked half never blocks its sibling.
//! * **loss notification + join-then-transition** → [`BlockingReader`]: a hostward
//!   reader on a dedicated blocking thread (§15.19) that pulses a loss [`Notify`]
//!   on device loss and is joined before the supervisor transitions (fd-reuse-safe).
//! * **back off, retry** → [`Backoff`]: the reconnect/restart backoff, reset on a
//!   good connection.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::Notify;

/// Park the calling half until the task is aborted on teardown (§16.1
/// park-don't-teardown). A direction whose producer is exhausted awaits this
/// instead of returning its session outcome, so the pump's *other* direction — an
/// independent read/write half sharing the same connection — stays live. This is
/// the rule the leg stale-status wedge violated (§15.24): a producer-closed write
/// half must park, never tear down the still-live read half. Its output type is
/// generic, so it drops into a half of any outcome type unchanged.
pub async fn park<T>() -> T {
    std::future::pending::<T>().await
}

/// Run three boundary halves as concurrently-polled futures, returning the first
/// to finish (§15.22): the two data directions plus one auxiliary arm (the exec
/// child's stderr drain, the leg's reject-second-peer accept loop) that runs
/// alongside them and typically [`park`]s at its natural end, so it never ends the
/// pump on its own. Neither data half may end the other: an exhausted half
/// [`park`]s, so this returns only on a genuine session end (peer/child/device
/// loss). This is the concurrent-halves rule the exec-pump deadlock violated — two
/// directions pumped sequentially in one branch deadlock the moment one blocks —
/// made structural.
///
/// This is a genuine flat three-arm `tokio::select!`, so it preserves the exact
/// per-poll fairness of the inline pump selects it replaced (a nested pair of
/// two-arm selects would bias the tie-break when two arms are ready in one poll).
pub async fn race3<T>(
    a: impl Future<Output = T>,
    b: impl Future<Output = T>,
    c: impl Future<Output = T>,
) -> T {
    tokio::pin!(a, b, c);
    tokio::select! {
        v = &mut a => v,
        v = &mut b => v,
        v = &mut c => v,
    }
}

/// Reconnect/restart backoff (§7.4/§7.6). Two shapes: [`Backoff::exponential`]
/// doubles the wait toward a cap on each failure (the leg's connect role);
/// [`Backoff::fixed`] waits a constant interval (the exec child restart). A good
/// connection calls [`Backoff::reset`] so the next outage starts from the initial
/// wait again. Millisecond intervals are floored at 1 so a zero config still makes
/// progress.
pub struct Backoff {
    initial: u64,
    max: u64,
    current: u64,
    /// Minimum wait interval. `exponential` uses 1 (matching the leg's original
    /// `sleep_backoff`, which clamped with `.max(1)`); `fixed` uses 0 so a
    /// configured `restart_backoff_ms = 0` waits exactly 0ms, exactly as the exec
    /// codec's original unfloored `sleep(from_millis(ms))` did (item-1 audit).
    floor: u64,
}

impl Backoff {
    /// Exponential backoff from `initial_ms`, doubling toward `max_ms` on each
    /// [`Self::sleep`]. Floored at 1ms, matching the leg's original `sleep_backoff`.
    pub fn exponential(initial_ms: u64, max_ms: u64) -> Backoff {
        Backoff {
            initial: initial_ms,
            max: max_ms,
            current: initial_ms,
            floor: 1,
        }
    }

    /// A constant `ms` wait (initial == max), for a fixed restart interval. Not
    /// floored: `fixed(0)` waits exactly 0ms, preserving the exec codec's original
    /// immediate respawn on a zero-configured backoff.
    pub fn fixed(ms: u64) -> Backoff {
        Backoff {
            initial: ms,
            max: ms,
            current: ms,
            floor: 0,
        }
    }

    /// The next wait interval (ms), advancing the schedule. Pure, so the growth
    /// curve is property-testable without a real timer; [`Self::sleep`] wraps it.
    /// Matches the leg's original `sleep_backoff` (for `exponential`, `floor == 1`):
    /// clamp the current wait to `[floor, max.max(floor)]`, then grow toward that cap.
    fn next_interval(&mut self) -> u64 {
        let cap = self.max.max(self.floor);
        let this = self.current.max(self.floor).min(cap);
        self.current = this.saturating_mul(2).min(cap);
        this
    }

    /// Sleep the current interval, then grow it toward the cap.
    pub async fn sleep(&mut self) {
        let this = self.next_interval();
        tokio::time::sleep(Duration::from_millis(this)).await;
    }

    /// Reset to the initial interval after a good connection.
    pub fn reset(&mut self) {
        self.current = self.initial;
    }
}

/// A hostward reader on a dedicated blocking thread (§15.19) with the two signals
/// its supervisor needs: a loss [`Notify`] the thread pulses on device loss
/// (POLLHUP/EOF/error), and a stop flag plus join handle so the supervisor joins
/// the thread *before* dropping the fd it reads — the fd must outlive the thread or
/// a reused fd races the next open (§7.1 fd-reuse). This is the "notify on loss,
/// join before transition" pair (§16.1), of which the serial reader is the archetype.
#[derive(Default)]
pub struct BlockingReader {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    lost: Arc<Notify>,
}

impl BlockingReader {
    /// Spawn `body` on a named blocking thread, handing it the stop flag (poll it
    /// each iteration; exit promptly when set) and the loss `Notify` (pulse it on
    /// device loss, then return). Any previously-armed reader must already be joined
    /// via [`Self::stop_join`]. Returns the OS error if the thread cannot be spawned
    /// (e.g. `EAGAIN` under a thread/PID limit), for the caller to fault the node
    /// rather than panic its supervisor.
    pub fn arm(
        &mut self,
        name: String,
        body: impl FnOnce(Arc<AtomicBool>, Arc<Notify>) + Send + 'static,
    ) -> std::io::Result<()> {
        debug_assert!(
            self.handle.is_none(),
            "arm called on an un-joined reader; call stop_join first"
        );
        let stop = Arc::new(AtomicBool::new(false));
        let lost = Arc::new(Notify::new());
        let (stop_c, lost_c) = (stop.clone(), lost.clone());
        let handle = std::thread::Builder::new()
            .name(name)
            .spawn(move || body(stop_c, lost_c))?;
        self.stop = stop;
        self.lost = lost;
        self.handle = Some(handle);
        Ok(())
    }

    /// The loss signal the current reader pulses, for the supervisor to await.
    pub fn lost(&self) -> Arc<Notify> {
        self.lost.clone()
    }

    /// Signal the current reader to stop and join it (fd-reuse-safe). On the loss
    /// path the thread has already exited, so this returns at once; on teardown of a
    /// live device it costs at most one reader poll interval.
    pub fn stop_join(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // --- concurrent halves + park-don't-teardown --------------------------------

    #[tokio::test]
    async fn race3_returns_the_live_half_while_the_others_park() {
        // A parked sibling never prevents a live half from completing — the
        // anti-deadlock invariant (§15.22). Exercise each arm position so none is
        // privileged.
        assert_eq!(race3(async { 7u8 }, park::<u8>(), park::<u8>()).await, 7);
        assert_eq!(race3(park::<u8>(), async { 8u8 }, park::<u8>()).await, 8);
        assert_eq!(race3(park::<u8>(), park::<u8>(), async { 9u8 }).await, 9);
    }

    #[tokio::test]
    async fn race3_does_not_starve_a_yielding_half_behind_parked_ones() {
        // The live half yields several times before producing — two parked siblings
        // must not wedge the scheduler against it.
        let live = async {
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            42u8
        };
        assert_eq!(race3(park::<u8>(), live, park::<u8>()).await, 42);
    }

    // --- backoff schedule -------------------------------------------------------

    proptest! {
        #[test]
        fn backoff_is_monotone_capped_and_floored(initial in 0u64..5_000, max in 0u64..5_000, steps in 1usize..12) {
            let mut b = Backoff::exponential(initial, max);
            let cap = max.max(1);
            let mut prev = 0u64;
            for _ in 0..steps {
                let this = b.next_interval();
                prop_assert!(this >= 1, "interval floored at 1ms");
                prop_assert!(this <= cap, "interval never exceeds max.max(1)");
                prop_assert!(this >= prev, "schedule is non-decreasing");
                prev = this;
            }
        }

        #[test]
        fn backoff_reset_returns_to_the_initial_interval(initial in 0u64..5_000, max in 0u64..5_000) {
            let mut b = Backoff::exponential(initial, max);
            let first = b.next_interval();
            for _ in 0..5 { b.next_interval(); }
            b.reset();
            prop_assert_eq!(b.next_interval(), first, "reset restarts the schedule");
        }
    }

    #[test]
    fn fixed_backoff_stays_constant() {
        let mut b = Backoff::fixed(37);
        assert_eq!(b.next_interval(), 37);
        assert_eq!(b.next_interval(), 37);
        b.reset();
        assert_eq!(b.next_interval(), 37);
    }

    #[test]
    fn fixed_zero_backoff_is_not_floored() {
        // A configured restart_backoff_ms = 0 must wait exactly 0ms (immediate
        // respawn), preserving the exec codec's pre-refactor behavior — `fixed` is
        // unfloored, unlike `exponential` (item-1 audit regression guard).
        let mut b = Backoff::fixed(0);
        assert_eq!(b.next_interval(), 0);
        assert_eq!(b.next_interval(), 0);
    }

    // --- blocking reader: loss notify + join-then-transition --------------------

    #[tokio::test]
    async fn blocking_reader_pulses_loss_then_joins_cleanly() {
        let mut r = BlockingReader::default();
        r.arm("test-loss".into(), |_stop, lost| {
            lost.notify_one(); // device loss
        })
        .expect("arm reader");
        // `lost()` is read after `arm` (as the supervisor does), so it observes the
        // reader's own signal. The pulse is durable — notify_one leaves a permit for
        // a later waiter.
        let lost = r.lost();
        tokio::time::timeout(Duration::from_secs(2), lost.notified())
            .await
            .expect("loss signal delivered");
        r.stop_join(); // the thread already exited; join returns at once
    }

    #[tokio::test]
    async fn blocking_reader_stop_join_ends_a_running_thread() {
        let mut r = BlockingReader::default();
        r.arm("test-stop".into(), |stop, _lost| {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        })
        .expect("arm reader");
        r.stop_join(); // sets the flag, the thread exits, join succeeds
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "un-joined reader")]
    async fn arm_without_stop_join_trips_the_precondition() {
        // Re-arming without an intervening `stop_join` would silently detach the
        // previous reader (the fd-reuse hazard the module claims to make
        // unrepresentable); the debug_assert catches it in debug/test builds.
        let mut r = BlockingReader::default();
        r.arm("first".into(), |_stop, _lost| {}).expect("arm first");
        // No `stop_join` here: the second arm must trip the precondition rather
        // than drop the still-joinable handle.
        let _ = r.arm("second".into(), |_stop, _lost| {});
    }
}
