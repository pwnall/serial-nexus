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
//! * **loss notification + join-then-transition** → [`BlockingWorker`]: a boundary
//!   half on a dedicated blocking thread (§15.19), in either direction, that may
//!   pulse a loss [`Notify`] and is joined before the supervisor transitions
//!   (fd-reuse-safe).
//! * **back off, retry** → [`Backoff`]: the reconnect/restart backoff, reset on a
//!   good connection.
//! * **a node's tasks die with the node** → [`TaskSet`]: the spawned-task half of
//!   every node's teardown, which §16.1 left per node and six modules then wrote
//!   eleven times (SIMPB-10).

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

/// One half of a boundary on a dedicated blocking thread (§15.19), with the signals
/// its supervisor needs: a stop flag plus join handle so the supervisor joins the
/// thread *before* dropping the fd it works on — the fd must outlive the thread or a
/// reused fd races the next open (§7.1 fd-reuse) — and, for a half that can observe
/// the environment going away, a loss [`Notify`] the thread pulses on device loss
/// (POLLHUP/EOF/error). This is the "notify on loss, join before transition" pair
/// (§16.1), of which the serial reader is the archetype.
///
/// **Direction-neutral, and the loss signal is opt-in** (plan §18 item 42, notes
/// §3.21). The type was `BlockingReader` and every arming minted a loss `Notify`,
/// which is why the pty's hostward *writer* thread — same stop flag, same join
/// handle, same EAGAIN-faults-the-node spawn, but with no device loss to report and
/// no supervisor awaiting one — stayed a hand-rolled second copy. [`Self::arm`] is
/// the loss-reporting form and hands the caller the `Notify` it minted, so a
/// supervisor cannot accidentally await a *previous* reader's signal;
/// [`Self::arm_quiet`] mints none at all, which is what makes the loss signal
/// optional structurally rather than by convention.
#[derive(Default)]
pub struct BlockingWorker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

/// How a [`BlockingWorker`] starts its thread. A **seam, not a second code path**:
/// production passes [`spawn_named`] and the CONC-3 guard passes a refusing
/// stand-in, so the guard drives the whole of the caller's construction. That
/// distinction is the finding: CONC-3's defect was a `.spawn(…).expect(…)` written
/// inline, and a guard that can only call the handler the fix introduced cannot fail
/// against it.
pub type ThreadSpawn =
    fn(String, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<JoinHandle<()>>;

/// The production [`ThreadSpawn`]: a named OS thread.
pub fn spawn_named(
    name: String,
    body: Box<dyn FnOnce() + Send + 'static>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name(name).spawn(body)
}

impl BlockingWorker {
    /// Spawn `body` on a named blocking thread, handing it the stop flag (poll it
    /// each iteration; exit promptly when set) and a freshly-minted loss `Notify`
    /// (pulse it on device loss, then return). The same `Notify` is returned, for the
    /// supervisor to await. Any previously-armed worker must already be joined via
    /// [`Self::stop_join`]. Returns the OS error if the thread cannot be spawned
    /// (e.g. `EAGAIN` under a thread/PID limit), for the caller to fault the node
    /// rather than panic its supervisor.
    pub fn arm(
        &mut self,
        name: String,
        body: impl FnOnce(Arc<AtomicBool>, Arc<Notify>) + Send + 'static,
    ) -> std::io::Result<Arc<Notify>> {
        let lost = Arc::new(Notify::new());
        let lost_c = lost.clone();
        self.arm_quiet(name, move |stop| body(stop, lost_c))?;
        Ok(lost)
    }

    /// [`Self::arm`] for a half with no device loss to report — the pty's hostward
    /// writer, whose environment (the client on the slave) coming and going is
    /// presence, not loss, and is already observed by its sibling reader task. No
    /// `Notify` is minted, so "this worker has nothing to signal" is a fact about the
    /// call rather than an unread field.
    pub fn arm_quiet(
        &mut self,
        name: String,
        body: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
    ) -> std::io::Result<()> {
        self.arm_with(spawn_named, name, body)
    }

    /// [`Self::arm_quiet`] over an injected [`ThreadSpawn`].
    pub fn arm_with(
        &mut self,
        spawn: ThreadSpawn,
        name: String,
        body: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
    ) -> std::io::Result<()> {
        debug_assert!(
            self.handle.is_none(),
            "arm called on an un-joined worker; call stop_join first"
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stop_c = stop.clone();
        let handle = spawn(name, Box::new(move || body(stop_c)))?;
        self.stop = stop;
        self.handle = Some(handle);
        Ok(())
    }

    /// Whether a worker is armed and not yet joined — for a `Drop` deciding between
    /// a no-op and a full teardown.
    pub fn is_armed(&self) -> bool {
        self.handle.is_some()
    }

    /// Forget the worker without joining it. `join` has no timeout, so a caller with
    /// a *bounded* wait (§7.3's flush bound) needs a way to give up on a wedged
    /// thread; the thread then lives until the process exits.
    pub fn detach(&mut self) {
        self.handle = None;
    }

    /// Ask the current worker to stop, and return immediately — the cheap,
    /// non-blocking half of teardown (BND-1).
    ///
    /// Teardown used to be `stop_join` per node inside one critical section on the
    /// single runtime thread, so `load --replace` and shutdown stalled the whole
    /// daemon for the *sum* of every node's stop latency (each bounded by one poll
    /// interval, but paid serially). Splitting the signal from the join lets the
    /// caller signal *all* nodes first and then join them, paying the latency once
    /// rather than N times. Nothing is joined here, so the fd this worker is
    /// working on must stay alive until [`Self::join`] has run (§7.1 fd-reuse).
    pub fn signal_stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Join the current worker thread, if any — the blocking half of teardown. Must
    /// run before the fd the worker holds is dropped (fd-reuse-safe). On the loss
    /// path the thread has already exited, so this returns at once; otherwise it
    /// costs at most one worker poll interval *after* [`Self::signal_stop`].
    pub fn join(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Signal the current worker to stop and join it (fd-reuse-safe) — the thin
    /// composition of [`Self::signal_stop`] and [`Self::join`], for a caller that is
    /// tearing down exactly one worker and has nothing to overlap the wait with.
    pub fn stop_join(&mut self) {
        self.signal_stop();
        self.join();
    }
}

/// A node's set of spawned data-plane tasks, which **die with the node** (§16.1).
///
/// Every node kind kept a bare `Vec<JoinHandle<()>>` and wrote
/// `for t in self.tasks.drain(..) { t.abort(); }` in both `signal_stop` and `Drop` —
/// eleven copies across six modules, with four different relationships between
/// `Drop`, `signal_stop` and `teardown` (SIMPB-10). Nothing in the type system,
/// clippy or the suite said a node *must* have a `Drop`; it was a convention held by
/// six copies of a loop, and the same class already bit this project once as
/// `signal_stop`/`teardown`/`Drop` drift (BND-1). Owning the abort here makes it a
/// type property instead: a node that forgets its `Drop` — or a *new* node kind, which
/// is the reachable case — cannot leave `LocalSet` tasks running against `Rc` state
/// after the node value is gone, holding an endpoint's `Rc<EdgeSlot>`/`SharedLock`
/// alive and draining channels a torn-down node should have released.
///
/// This covers only the *task* half of teardown. A node with a blocking thread or a
/// filesystem artifact still needs its own `Drop`, because the ordering there is
/// load-bearing: the serial reader and the pty writer must be **joined after their
/// tasks abort and before their fd drops** (§7.1 fd-reuse), and field-drop order alone
/// would not give that.
#[derive(Default)]
pub struct TaskSet(Vec<tokio::task::JoinHandle<()>>);

impl TaskSet {
    /// Take ownership of one spawned task.
    pub fn push(&mut self, handle: tokio::task::JoinHandle<()>) {
        self.0.push(handle);
    }

    /// Abort every task and forget it — the cheap, non-blocking half of teardown
    /// (BND-1): nothing is joined, so a caller may signal every node before paying any
    /// node's join cost. Idempotent, because the handles are drained.
    pub fn abort_all(&mut self) {
        for t in self.0.drain(..) {
            t.abort();
        }
    }

    /// Whether **no** task is still held — true for a node that was never started,
    /// and true again once [`Self::abort_all`] has drained the handles. Used by a
    /// node whose `Drop` must decide between a no-op and a full teardown, which
    /// negates it (`!tasks.is_empty()` = "there is still something to stop").
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for TaskSet {
    fn drop(&mut self) {
        self.abort_all();
    }
}

// The purge's bounded drain-to-quiescence used to live here, as
// `drain_to_quiescence(&mut Receiver<Chunk>)`. It moved onto
// `runtime::TargetwardInbox::purge_to_quiescence` when `serial` and `leg` adopted the
// §15.50 teardown ledger, and the move was forced rather than tidy: a helper holding
// the receiver on its own stack across the yields between rounds is a window in which
// the node cannot reach its own queue, so a `remove-node` landing there would charge
// `0` and destroy whatever a suspended sender had just pushed — the exact shape of the
// defect notes §3.31 fixed. Draining through the inbox's slot closes it. There is no
// raw-receiver form left on purpose: two spellings of one purge rule is what §16.1
// forbids, and it is the drift notes §3.31 predicted when it scheduled this work
// (notes §3.55). The rule's three guards moved with it, to `runtime.rs`'s tests.

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

    // --- blocking worker: loss notify + join-then-transition --------------------

    #[tokio::test]
    async fn blocking_reader_pulses_loss_then_joins_cleanly() {
        let mut r = BlockingWorker::default();
        // `arm` hands back the very `Notify` it gave the thread, so the supervisor
        // structurally cannot await a *previous* reader's signal. The pulse is
        // durable — notify_one leaves a permit for a later waiter.
        let lost = r
            .arm("test-loss".into(), |_stop, lost| {
                lost.notify_one(); // device loss
            })
            .expect("arm worker");
        tokio::time::timeout(Duration::from_secs(2), lost.notified())
            .await
            .expect("loss signal delivered");
        r.stop_join(); // the thread already exited; join returns at once
    }

    #[tokio::test]
    async fn blocking_reader_stop_join_ends_a_running_thread() {
        let mut r = BlockingWorker::default();
        r.arm("test-stop".into(), |stop, _lost| {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        })
        .expect("arm worker");
        r.stop_join(); // sets the flag, the thread exits, join succeeds
    }

    #[tokio::test]
    async fn a_quiet_worker_gets_the_stop_flag_and_no_loss_signal() {
        // The pty writer's shape (item 42): same flag, same join, no `Notify` minted.
        // `arm_quiet`'s body cannot name a loss signal, which is the whole point — the
        // absence is in the type, not in a field the caller remembers not to read.
        let mut w = BlockingWorker::default();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_thread = ran.clone();
        w.arm_quiet("test-quiet".into(), move |stop| {
            ran_thread.store(true, Ordering::Relaxed);
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        })
        .expect("arm quiet worker");
        w.stop_join();
        assert!(
            ran.load(Ordering::Relaxed),
            "the body ran and observed stop"
        );
    }

    #[tokio::test]
    async fn signal_stop_returns_before_the_thread_exits_and_join_waits_for_it() {
        // BND-1: the two halves must be separable — `signal_stop` returns while the
        // thread is still winding down (so a caller can signal N nodes and pay the
        // latency once), and `join` is what actually waits.
        let mut r = BlockingWorker::default();
        let exited = Arc::new(AtomicBool::new(false));
        let exited_thread = exited.clone();
        r.arm("test-split".into(), move |stop, _lost| {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(30));
            exited_thread.store(true, Ordering::Relaxed);
        })
        .expect("arm reader");
        r.signal_stop();
        assert!(
            !exited.load(Ordering::Relaxed),
            "signal_stop must not wait for the thread"
        );
        r.join();
        assert!(exited.load(Ordering::Relaxed), "join must wait for it");
        r.join(); // idempotent: the handle was already taken
    }

    // --- a node's tasks die with the node ---------------------------------------

    /// SIMPB-10: the property that replaced six copies of an abort loop and four
    /// relationships between `Drop` and `signal_stop`. A node value that dies without
    /// anyone calling `signal_stop`/`teardown` — a `load --replace` rollback, a panic
    /// unwind, a unit test that just drops its node — must not leave `LocalSet` tasks
    /// running against the `Rc` state it held.
    #[test]
    fn dropping_a_task_set_aborts_the_tasks_it_holds() {
        // A current-thread runtime + LocalSet mirrors the daemon (§5).
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            // The guard's `Drop` fires when the task's *future* is dropped, which is
            // what abort causes — so the flag is direct evidence the task was cancelled
            // rather than merely detached.
            struct Guard(Arc<AtomicBool>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Relaxed);
                }
            }
            let alive = Arc::new(AtomicBool::new(true));
            let guard = Guard(alive.clone());
            let mut tasks = TaskSet::default();
            tasks.push(tokio::task::spawn_local(async move {
                let _guard = guard;
                park::<()>().await;
            }));
            tokio::task::yield_now().await; // let it reach the park
            assert!(alive.load(Ordering::Relaxed), "the task must be running");
            assert!(!tasks.is_empty());

            drop(tasks); // no explicit `abort_all`: the `Drop` impl must do it

            for _ in 0..16 {
                if !alive.load(Ordering::Relaxed) {
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert!(
                !alive.load(Ordering::Relaxed),
                "dropping the set must abort its tasks"
            );
        });
    }

    #[test]
    fn abort_all_empties_the_set_and_is_idempotent() {
        // `signal_stop` may run before `teardown` and before `Drop`, on every node, so
        // the second and third calls must be no-ops (BND-1's two-phase teardown).
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            let mut tasks = TaskSet::default();
            assert!(tasks.is_empty(), "a fresh set holds nothing");
            tasks.push(tokio::task::spawn_local(park::<()>()));
            assert!(!tasks.is_empty());
            tasks.abort_all();
            assert!(tasks.is_empty(), "abort_all drains the handles");
            tasks.abort_all(); // idempotent
            assert!(tasks.is_empty());
        });
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "un-joined worker")]
    async fn arm_without_stop_join_trips_the_precondition() {
        // Re-arming without an intervening `stop_join` would silently detach the
        // previous worker (the fd-reuse hazard the module claims to make
        // unrepresentable); the debug_assert catches it in debug/test builds.
        let mut r = BlockingWorker::default();
        r.arm("first".into(), |_stop, _lost| {}).expect("arm first");
        // No `stop_join` here: the second arm must trip the precondition rather
        // than drop the still-joinable handle.
        let _ = r.arm("second".into(), |_stop, _lost| {});
    }
}
