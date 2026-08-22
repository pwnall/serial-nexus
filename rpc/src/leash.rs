//! The stdin-EOF orphan leash (design §15.43), spelled **once** for every binary in
//! the workspace that carries one.
//!
//! # The mechanism
//!
//! A supervisor that wants a process to die with it hands that process the read end of
//! a pipe as stdin and holds the write end. The kernel closes that write end however
//! the supervisor dies — a normal exit, a panic, `abort`, `SIGKILL`, a runner killing
//! the process group — so an EOF here means "my supervisor is gone" and nothing else
//! can produce it. That is also exactly why it is opt-in: under a service manager, or
//! with `< /dev/null`, stdin is at EOF from the first instant, and a leash nobody
//! armed must not be armed by accident.
//!
//! Kernel-independent by construction (AGENTS §7): pipe EOF is POSIX and behaves
//! identically on Linux and Darwin, unlike `PR_SET_PDEATHSIG` (Linux only, and scoped
//! to the *thread* that forked) or a kqueue `NOTE_EXIT` (Darwin only, and `unsafe`
//! outside `serial_nexus_sys`, §16.3). Nothing here is conditional on a target, so
//! every platform the suite runs on exercises the same code.
//!
//! A detached `std` thread in `read(2)` rather than `tokio::io::stdin`, which is
//! §15.43 clause 3 and not a stylistic preference: the tokio reader parks an
//! uncancellable task in the blocking pool, and runtime shutdown waits on that pool —
//! a process stopping for any *other* reason would then hang at exit on a read that
//! never completes. A thread blocked in `read(2)` costs one stack, no CPU, and is
//! reclaimed by process exit.
//!
//! # One reader, many waiters
//!
//! The watch is process-wide and started at most once. Two threads reading the same
//! stdin would race for the bytes, and a process that wants both a leash and a
//! caller-owned hold on the same EOF needs those to be two *waiters* on one reader
//! rather than two readers. [`stdin_eof_watch`] is therefore idempotent: the first
//! call spawns the thread, every later call joins the same watch.
//!
//! Two waiter shapes, because the three binaries are not the same kind of program:
//!
//! * [`StdinEof::wait`] blocks the calling thread — what a synchronous double wants;
//! * [`StdinEofSignal`] is a `Future` — what an async runtime wants, and it is
//!   deliberately runtime-agnostic (hand-rolled over [`std::task`]) so that this
//!   crate, which the synchronous CLI also links, declares no runtime dependency.
//!
//! [`StdinEofSignal::never`] is the **unarmed** case expressed as a value rather than
//! as a second code path: a future that never resolves, so a `select!` arm written
//! once is simply never taken. The idiom it replaces was a held one-shot sender whose
//! only job was to not be dropped, copied into two binaries.
//!
//! # Why a lifetime primitive lives in the RPC crate
//!
//! It was implemented three times — in the daemon, in the web console and in the test
//! double — byte-for-byte in the first two, down to the thread name and the log line
//! (plan §18 item 79). Three copies of a mechanism that must agree is the §7.1 clause
//! 2 shape, and the precedent for the repair is named: the console's second copy of
//! the socket-path policy was *deleted* into [`crate::socket`], which is the only
//! repair that keeps "exactly one implementation" true (plan §18 item 51).
//!
//! `serial-nexus-rpc` is where that precedent put the other cross-binary policy the
//! separate processes must agree on, and it is the one shared crate reachable from
//! every caller at no cost. `serial-nexus-core` would serve the daemon and the console
//! but the double depends on neither, so it buys nothing there;
//! `serial-nexus-sys` is ruled out by the reasoning this crate's own manifest already
//! records against `nix` — depending on it would make a binary link IOKit and
//! CoreFoundation on macOS, and the console has no other reason to.
//!
//! This module declares no dependency at all: it is `std` only, so nothing that links
//! `serial-nexus-rpc` for its wire types pays for a leash it does not arm.

use std::future::Future;
use std::io::{self, Read};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::task::{Context, Poll, Waker};

/// What a leashed process logs on its way out, so the three binaries that can print
/// it cannot drift into three descriptions of one event.
pub const STOPPING_ON_STDIN_EOF: &str = "stdin reached EOF under --exit-on-stdin-eof: the supervisor holding the other end \
     of the pipe is gone; stopping";

/// The name of the reader thread, in one place because it is what an operator reading
/// `ps -L` or a core file has to recognise.
const WATCH_THREAD_NAME: &str = "stdin-eof-watch";

/// The process's stdin-EOF watch: one reader thread, any number of waiters.
///
/// Obtained from [`stdin_eof_watch`], which is the only constructor — a second watch
/// would be a second reader on the same fd, which is the thing this type exists to
/// prevent.
pub struct StdinEof {
    state: Mutex<State>,
    woken: Condvar,
}

struct State {
    /// The reader thread has been spawned. Set only *after* a successful spawn, so a
    /// spawn that failed for want of a thread is retried by the next caller rather
    /// than leaving behind a watch that will never fire.
    started: bool,
    /// stdin has reached EOF. Sticky: once true it never goes back, which is what
    /// lets a waiter that arrives late return immediately.
    seen: bool,
    /// One entry per live [`StdinEofSignal`] that has been polled, keyed by the
    /// signal's id so that a signal dropped before EOF takes its slot with it.
    wakers: Vec<(u64, Waker)>,
}

/// The one watch, or none yet. `OnceLock` gives the `&'static` a [`StdinEofSignal`]
/// holds; the `started` flag inside it — not the `OnceLock` — is what makes the
/// *thread* spawn once, because spawning can fail and a poisoned-forever singleton
/// would be worse than a retry.
static WATCH: OnceLock<StdinEof> = OnceLock::new();

/// Distinct ids for the waker table. Wrapping is not a concern: a `u64` at one per
/// armed signal outlives every process this leashes by many orders of magnitude.
static NEXT_SIGNAL_ID: AtomicU64 = AtomicU64::new(0);

/// Take the state lock, ignoring poisoning.
///
/// A leash that refuses to fire because some unrelated thread panicked while holding
/// this lock would leave precisely the orphan the leash exists to prevent — the same
/// reasoning as the read loop's `Err(_) => break` arm, one level up.
fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start (at most once) the stdin-EOF reader thread and return the process's watch.
///
/// **This reads stdin.** A process that has not opted into the leash must not call it:
/// a mode whose stdin carries data — a codec child's envelope pipe, say — would have
/// that data consumed by the watch. [`stdin_eof_signal`] exists so that the unarmed
/// case is spelled without reaching this function at all.
///
/// # Errors
///
/// Only if the reader thread cannot be spawned. The watch is left unstarted, so a
/// later caller may try again.
pub fn stdin_eof_watch() -> io::Result<&'static StdinEof> {
    let watch = WATCH.get_or_init(|| StdinEof {
        state: Mutex::new(State {
            started: false,
            seen: false,
            wakers: Vec::new(),
        }),
        woken: Condvar::new(),
    });
    let mut state = lock(&watch.state);
    if !state.started {
        std::thread::Builder::new()
            .name(WATCH_THREAD_NAME.to_owned())
            .spawn(move || watch.read_to_eof())?;
        state.started = true;
    }
    Ok(watch)
}

/// Arm — or deliberately do not arm — this process's leash, in one expression.
///
/// `armed` is the operator's `--exit-on-stdin-eof`. When it is false **stdin is not
/// touched**: the returned signal is [`StdinEofSignal::never`], a future that stays
/// pending forever, so the `select!` arm that awaits it is written unconditionally and
/// simply never taken.
///
/// # Errors
///
/// Propagates [`stdin_eof_watch`]'s spawn failure. Never fails when `armed` is false.
pub fn stdin_eof_signal(armed: bool) -> io::Result<StdinEofSignal> {
    if armed {
        Ok(stdin_eof_watch()?.signal())
    } else {
        Ok(StdinEofSignal::never())
    }
}

impl StdinEof {
    /// Block the calling thread until stdin reaches EOF; return immediately if it
    /// already has.
    pub fn wait(&self) {
        let mut state = lock(&self.state);
        while !state.seen {
            state = self
                .woken
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Whether stdin has reached EOF yet. Never goes back to false.
    pub fn reached(&self) -> bool {
        lock(&self.state).seen
    }

    /// A future that resolves when stdin reaches EOF.
    ///
    /// Independent of every other signal and of [`wait`](Self::wait): the watch fans
    /// one close out to all of them.
    pub fn signal(&'static self) -> StdinEofSignal {
        StdinEofSignal {
            watch: Some(self),
            id: NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The reader thread's body: drain stdin, then release every waiter.
    fn read_to_eof(&self) {
        let stdin = io::stdin();
        drain_to_eof(&mut stdin.lock());
        self.complete();
    }

    /// Record the close and release every waiter. Idempotent.
    fn complete(&self) {
        let wakers = {
            let mut state = lock(&self.state);
            state.seen = true;
            std::mem::take(&mut state.wakers)
        };
        // Both wake-ups happen with the lock **released**. `Waker::wake` runs executor
        // code, and an executor that polls inline would re-enter `poll_eof` on the
        // mutex this thread would still be holding.
        self.woken.notify_all();
        for (_, waker) in wakers {
            waker.wake();
        }
    }

    /// [`StdinEofSignal::poll`]'s body, on the watch so the table stays private.
    fn poll_eof(&self, id: u64, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = lock(&self.state);
        if state.seen {
            return Poll::Ready(());
        }
        match state.wakers.iter_mut().find(|(slot, _)| *slot == id) {
            // Re-polled, possibly on a different task: keep the newest waker, and skip
            // the clone when the executor says it would wake the same task anyway.
            Some((_, held)) => {
                if !held.will_wake(cx.waker()) {
                    *held = cx.waker().clone();
                }
            }
            None => state.wakers.push((id, cx.waker().clone())),
        }
        Poll::Pending
    }

    /// Drop a signal's slot, so a signal that goes away before EOF leaves nothing in
    /// the table.
    fn forget(&self, id: u64) {
        lock(&self.state).wakers.retain(|(slot, _)| *slot != id);
    }
}

/// Read `reader` until it ends, discarding everything it carries.
///
/// Split out from the reader thread so the four arms below can be driven directly,
/// without a test having to own the process's real stdin. The end-to-end property —
/// that the *shipped* binaries read their own stdin exactly this way — is asserted
/// against the products, in `itest/tests/p13_stdin_eof_leash.rs`.
fn drain_to_eof(reader: &mut impl Read) {
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            // EOF: the far end of the pipe is closed. The only event this watch
            // reports.
            Ok(0) => break,
            // Anything written is noise, not a protocol — so a supervisor that logs
            // to the pipe by accident does not end the process it is supervising.
            Ok(_) => continue,
            // A signal arrived mid-read. Not an end of anything.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            // A stdin that cannot be read is as good as gone. Failing closed here
            // would leave precisely the orphan this exists to prevent.
            Err(_) => break,
        }
    }
}

/// A future that resolves when stdin reaches EOF — or, from
/// [`StdinEofSignal::never`], one that never resolves at all.
///
/// Hand-rolled over [`std::task`] rather than taken from a runtime: this crate is
/// linked by a synchronous CLI as well as by two async binaries, and the leash is not
/// a reason for the first of them to grow a runtime dependency.
pub struct StdinEofSignal {
    /// `None` is the leash **off**: the unarmed arm as data rather than as a second
    /// code path.
    watch: Option<&'static StdinEof>,
    id: u64,
}

impl StdinEofSignal {
    /// A signal that never resolves — what an unleashed process awaits.
    ///
    /// It returns `Pending` without registering a waker, which is correct precisely
    /// because nothing can ever wake it: the arm is never re-polled and never taken,
    /// and the other branches of the `select!` drive the loop exactly as before.
    pub fn never() -> Self {
        Self { watch: None, id: 0 }
    }
}

impl Future for StdinEofSignal {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.watch {
            Some(watch) => watch.poll_eof(self.id, cx),
            None => Poll::Pending,
        }
    }
}

impl Drop for StdinEofSignal {
    fn drop(&mut self) {
        if let Some(watch) = self.watch {
            watch.forget(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// A watch with no reader thread behind it, so the fan-out can be driven from a
    /// test without owning the process's stdin.
    fn detached_watch() -> &'static StdinEof {
        // Leaked deliberately: `signal()` hands out `&'static`, and a test that wants
        // more than one independent watch cannot use the process singleton.
        Box::leak(Box::new(StdinEof {
            state: Mutex::new(State {
                started: true,
                seen: false,
                wakers: Vec::new(),
            }),
            woken: Condvar::new(),
        }))
    }

    /// A `Read` that plays a scripted sequence, so each arm of [`drain_to_eof`] can be
    /// reached on purpose.
    struct Script {
        steps: Vec<io::Result<u8>>,
        at: usize,
    }

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.steps.get_mut(self.at) {
                None => Ok(0),
                Some(step) => {
                    self.at += 1;
                    match step {
                        Ok(b) => {
                            buf[0] = *b;
                            Ok(1)
                        }
                        Err(e) => Err(io::Error::new(e.kind(), "scripted")),
                    }
                }
            }
        }
    }

    fn waker_counting(hits: &Arc<AtomicUsize>) -> Waker {
        struct Counter(Arc<AtomicUsize>);
        impl std::task::Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        Waker::from(Arc::new(Counter(hits.clone())))
    }

    /// **Bytes on stdin are noise, not a protocol.** The arm that would make them a
    /// protocol is `Ok(_) => break`, and what it costs is a leash that fires on the
    /// first byte a supervisor logs into the pipe by accident.
    #[test]
    fn a_write_on_stdin_is_discarded_rather_than_read_as_the_close() {
        let data = [1u8, 2, 3, 4];
        let mut reader: &[u8] = &data;
        drain_to_eof(&mut reader);
        assert!(
            reader.is_empty(),
            "drain_to_eof stopped with {} of {} bytes unread: it treated a write on \
             stdin as the close, which ends a leashed process the first time its \
             supervisor writes anything into the pipe",
            reader.len(),
            data.len()
        );
    }

    /// **`EINTR` is not an end of anything**, and a read error is.
    #[test]
    fn an_interrupted_read_resumes_and_a_broken_one_ends_the_watch() {
        let mut interrupted = Script {
            steps: vec![
                Err(io::Error::from(io::ErrorKind::Interrupted)),
                Ok(b'x'),
                Err(io::Error::from(io::ErrorKind::Interrupted)),
            ],
            at: 0,
        };
        drain_to_eof(&mut interrupted);
        assert_eq!(
            interrupted.at, 3,
            "drain_to_eof stopped on an EINTR instead of resuming, so a leashed \
             process ends on the next signal it happens to catch mid-read"
        );

        let mut broken = Script {
            steps: vec![
                Ok(b'x'),
                Err(io::Error::from(io::ErrorKind::BrokenPipe)),
                Ok(b'y'),
            ],
            at: 0,
        };
        drain_to_eof(&mut broken);
        assert_eq!(
            broken.at, 2,
            "drain_to_eof kept reading past an unrecoverable error; a stdin that \
             cannot be read is as good as gone, and looping on it spins"
        );
    }

    /// **One close releases every waiter**, which is the whole reason the watch is a
    /// singleton with a fan-out rather than one reader per waiter.
    #[test]
    fn one_close_releases_a_blocking_waiter_and_every_signal() {
        let watch = detached_watch();
        let hits = Arc::new(AtomicUsize::new(0));
        let waker = waker_counting(&hits);
        let mut a = watch.signal();
        let mut b = watch.signal();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut a).poll(&mut cx), Poll::Pending);
        assert_eq!(Pin::new(&mut b).poll(&mut cx), Poll::Pending);
        assert!(!watch.reached());

        let blocked = std::thread::spawn(move || watch.wait());
        watch.complete();
        blocked.join().expect("the blocking waiter returned");

        assert!(watch.reached());
        assert_eq!(Pin::new(&mut a).poll(&mut cx), Poll::Ready(()));
        assert_eq!(Pin::new(&mut b).poll(&mut cx), Poll::Ready(()));
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "the close woke {} of 2 registered signals; a signal the watch forgets to \
             wake is an arm of a `select!` that never fires again",
            hits.load(Ordering::SeqCst)
        );
    }

    /// **A waiter that arrives after the close does not block**, because `seen` is
    /// sticky. Without that, a leash armed a microsecond late waits forever.
    #[test]
    fn a_waiter_that_arrives_after_the_close_is_released_immediately() {
        let watch = detached_watch();
        watch.complete();
        watch.wait();
        let hits = Arc::new(AtomicUsize::new(0));
        let waker = waker_counting(&hits);
        let mut late = watch.signal();
        assert_eq!(
            Pin::new(&mut late).poll(&mut Context::from_waker(&waker)),
            Poll::Ready(())
        );
    }

    /// **A signal dropped before the close leaves nothing behind.** The table is the
    /// only unbounded thing here, and a process that arms and drops signals in a loop
    /// would grow it without this.
    #[test]
    fn a_signal_dropped_before_the_close_releases_its_slot() {
        let watch = detached_watch();
        let hits = Arc::new(AtomicUsize::new(0));
        let waker = waker_counting(&hits);
        let mut cx = Context::from_waker(&waker);
        for _ in 0..8 {
            let mut s = watch.signal();
            assert_eq!(Pin::new(&mut s).poll(&mut cx), Poll::Pending);
        }
        assert_eq!(
            lock(&watch.state).wakers.len(),
            0,
            "eight dropped signals left slots in the waker table"
        );
    }

    /// **The unarmed signal is pending forever, and it never touches stdin.** This is
    /// §15.43 clause 2 as a value: the leash is opt-in, and the code path is one.
    #[test]
    fn an_unarmed_signal_never_resolves() {
        let hits = Arc::new(AtomicUsize::new(0));
        let waker = waker_counting(&hits);
        let mut cx = Context::from_waker(&waker);
        // Order-independent: what is asserted is that *this call* did not start the
        // reader, not that nothing in the binary ever has.
        let started_before = WATCH.get().is_some_and(|w| lock(&w.state).started);
        let mut never = stdin_eof_signal(false).expect("the unarmed arm cannot fail");
        for _ in 0..4 {
            assert_eq!(
                Pin::new(&mut never).poll(&mut cx),
                Poll::Pending,
                "an unarmed leash resolved; every unleashed process would stop at \
                 startup under a service manager (§15.43 clause 2)"
            );
        }
        let started_after = WATCH.get().is_some_and(|w| lock(&w.state).started);
        assert_eq!(
            started_before, started_after,
            "the unarmed arm started the reader thread; a process that did not opt \
             into the leash must not consume its own stdin, and one whose stdin \
             carries data would have that data eaten"
        );
    }

    /// The log line is one string in one place (plan §18 item 79) — pinned here so a
    /// reword has to be deliberate, since the two binaries that print it can no longer
    /// disagree by accident.
    #[test]
    fn the_stopping_line_names_the_flag_and_the_cause() {
        assert!(STOPPING_ON_STDIN_EOF.contains("--exit-on-stdin-eof"));
        assert!(STOPPING_ON_STDIN_EOF.contains("supervisor"));
        assert!(STOPPING_ON_STDIN_EOF.ends_with("is gone; stopping"));
    }
}
