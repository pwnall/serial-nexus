//! Log node (design §7.3). Faces target; its write mode is inherently `never`,
//! so it only ever *consumes* hostward bytes and appends them to a file.
//!
//! Regular-file writes cannot be made non-blocking (`O_NONBLOCK` is a no-op on
//! them, §5), so the log owns a **bounded queue feeding a dedicated writer
//! thread** — the one place the data plane leaves the async runtime for a
//! blocking "writer task" (§5). An async *pump* task on the LocalSet drains the
//! node's hostward channel into the shared queue (applying the overflow policy);
//! the writer thread drains the queue and does the blocking `write(2)`s. Loss is
//! always counted — `dropped_bytes` — so a slow disk is visible, never silent.
//! A refusing filesystem is counted *separately* (`write_errors`), because loss
//! alone cannot tell a slow consumer from a disk that rejects every write.
//!
//! Rotation is on demand (`rotate <node>`, §7.3): the writer renames the current
//! file to `<name>.NNN` (higher is newer, no shifting cascade) and reopens fresh
//! at a byte boundary. It is ordered against the queue — `rotate` pushes a marker
//! *item*, so everything accepted before the request lands in the old file and
//! everything after in the new one. The counter is *state*, recovered at start by
//! scanning the directory and never persisted.
//!
//! Removal and clean shutdown flush the queue within a bounded wait before
//! closing, in **two phases**: [`LogNode::signal_stop`] is the cheap non-blocking
//! half (stop ingest, tell the writer to close) and [`LogNode::teardown`] is the
//! bounded collector. §7.3 mandates the bound; it does not sanction paying it
//! serially on the runtime thread, so a caller signals every node first and only
//! then collects — N wedged log directories then cost one `FLUSH_WAIT`, not N.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver as StdReceiver, RecvTimeoutError, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use serial_nexus_core::Chunk;
use serial_nexus_core::config::{NodeConfig, OverflowPolicy};
use serial_nexus_core::{NodeState, NodeStatus};

use crate::boundary::{BlockingWorker, TaskSet, ThreadSpawn, spawn_named};
use crate::runtime::{DropCounters, EdgeInbox};

/// Upper bound on in-memory queued log bytes before the overflow policy fires
/// (§5 bounded interior). Generous enough that a briefly slow disk buffers
/// rather than drops, small enough to stay bounded.
const QUEUE_CAP_BYTES: usize = 16 * 1024 * 1024;

/// How long removal/shutdown waits for the writer to flush before detaching it
/// (§7.3 "within a bounded wait"). Measured from `signal_stop`, not from the
/// moment this node's turn comes round — see the module header.
const FLUSH_WAIT: Duration = Duration::from_secs(2);

/// Creation mode for log files: owner read/write, group read, world nothing.
/// Console bytes are frequently root shells, so they must not inherit a
/// permissive umask (0664 was observed). The group bit is what a deployment
/// widens by placing the directory under a group, mirroring the control
/// socket's and the PTY slave's 0660 shape (§7.2/§10). `mode` applies only at
/// creation, which is right here: an operator who has re-chmod'ed an existing
/// log keeps their choice, and rotation's reopen creates the fresh file.
const LOG_FILE_MODE: u32 = 0o640;

/// State shared between the pump (async), the writer (thread), and `state`/
/// `rotate` (control plane). One mutex guards the queue and its bookkeeping; the
/// condvar wakes the writer on new data, a rotation request, or close.
struct Shared {
    q: Mutex<Queue>,
    cv: Condvar,
}

/// One entry in the writer's queue. Rotation is an *item* rather than a side
/// flag so it happens at exactly its point in the byte stream (§7.3 "reopens
/// fresh at a byte boundary"): bytes accepted after the operator's `rotate` can
/// no longer land in the pre-rotation file.
enum QueueItem {
    Bytes(Chunk),
    Rotate,
}

impl QueueItem {
    fn len(&self) -> usize {
        match self {
            QueueItem::Bytes(chunk) => chunk.len(),
            QueueItem::Rotate => 0,
        }
    }
}

struct Queue {
    items: VecDeque<QueueItem>,
    queued_bytes: usize,
    /// Bytes drained into the writer's current batch but not yet written. The
    /// batch vector holds them until it is dropped, so they are still resident
    /// memory and `state` must keep counting them — zeroing `queued_bytes` at
    /// drain understated the real in-flight depth.
    draining_bytes: usize,
    dropped_bytes: u64,
    /// Failed `write(2)`s the drop-oldest policy absorbed, and the most recent
    /// one's message. §5 sanctions drop-oldest-with-counters for exactly the
    /// full-disk case, so the node stays `active` — but then `dropped_bytes`
    /// alone cannot tell "the consumer is slow" from "the filesystem is refusing
    /// every write". These two separate them without changing the policy.
    write_errors: u64,
    last_write_error: Option<String>,
    /// Number of rotations requested but not yet performed by the writer. A
    /// counter (not a flag) so rapid `rotate` calls don't collapse into one; it
    /// mirrors the `QueueItem::Rotate` markers still in `items` and exists so
    /// `rotate` can predict the number its request will carry.
    rotate_pending: u32,
    /// Highest rotation suffix on disk (`<name>.NNN`); `None` until the first
    /// rotation. Recovered by directory scan at start, never persisted.
    rotation: Option<u64>,
    closed: bool,
    /// Set once nothing will ever drain this queue again: the writer never
    /// started (the file would not open, or the thread would not spawn), or
    /// [`writer_loop`] has returned. [`enqueue`] then drops-and-counts rather
    /// than piling bytes into a queue with no consumer — which reported up to
    /// `QUEUE_CAP_BYTES` of provably-unwritable console data as *pending*
    /// instead of *lost*, and held all 16 MiB of it resident (LOGQ-1). The
    /// writer's own `return // the pump drops-and-counts` comment always
    /// claimed this; the flag is what makes the claim true.
    writer_gone: bool,
    status: NodeState,
    overflow: OverflowPolicy,
}

pub struct LogNode {
    pub name: String,
    directory: PathBuf,
    filename: String,
    shared: Arc<Shared>,
    /// Shared with the serial reader: counts hostward bytes dropped because the
    /// node's ingest channel was full (§5). Folded into reported `dropped_bytes`.
    ingest_counters: Arc<DropCounters>,
    /// The ingest pump. A [`TaskSet`] rather than a bare handle so "this node's
    /// tasks die with the node" is the type's property, not this module's convention
    /// (§16.1, SIMPB-10) — a log's `Drop` is a genuine teardown (it must flush and
    /// join a blocking writer thread), so the two halves are deliberately distinct.
    tasks: TaskSet,
    /// The blocking writer thread (§16.1, plan §18 item 42). Its stop flag is
    /// **not** the worker's: a `Condvar::wait` cannot be woken by an `AtomicBool`,
    /// so this writer is told to close through `Queue::closed` under the same mutex
    /// it waits on. [`BlockingWorker::stop_join`] is therefore not usable here.
    writer: BlockingWorker,
    /// Signalled by the writer when it exits, so teardown can bound its flush
    /// wait without an unbounded `join()`.
    writer_done: Option<StdReceiver<()>>,
    /// When `signal_stop` ran. `teardown`'s bounded wait is measured from here,
    /// so the bound is "how long the writer got after being told to close"
    /// rather than "how long this node's turn in the teardown loop took".
    stop_signalled_at: Option<Instant>,
}

impl LogNode {
    pub fn create(config: &NodeConfig) -> LogNode {
        Self::create_with_spawner(config, spawn_named)
    }

    /// [`LogNode::create`], with the thread-spawn injected (see [`ThreadSpawn`]).
    fn create_with_spawner(config: &NodeConfig, spawn: ThreadSpawn) -> LogNode {
        let NodeConfig::Log {
            name,
            directory,
            filename,
            overflow,
            ..
        } = config
        else {
            unreachable!("LogNode::create called with non-Log config");
        };

        let directory = PathBuf::from(directory);
        // Recover the rotation counter by scanning for existing `<name>.NNN`
        // (§7.3). A scan that could not *run* is not "no rotations yet": adopting
        // `None` there resets the counter, and the next `rotate` then renames the
        // live file onto the newest rotation already on disk, which `rename(2)`
        // replaces without a word — the log node destroying the one thing it
        // exists to keep (LOG-2). The two are distinguishable here because they
        // are separate permissions: a mode-0300 directory grants create-and-
        // traverse without list, so `read_dir` fails while `open_append` succeeds.
        // An unreadable directory is an environmental failure, so it faults the
        // node exactly as an unopenable file does (§7), and `rotate` refuses for
        // as long as the fault stands.
        let scan = scan_rotation(&directory, filename);

        // The open is attempted either way, so a *missing* directory — where both
        // fail — keeps naming the open, which is the more useful diagnosis.
        let current = directory.join(filename);
        let (status, file) = match (open_append(&current), &scan) {
            (Err(e), _) => (
                NodeStatus::Faulted {
                    reason: format!("open {}: {e}", current.display()),
                },
                None,
            ),
            (Ok(_), Err(e)) => (
                NodeStatus::Faulted {
                    reason: format!("scan {} for rotations: {e}", directory.display()),
                },
                None,
            ),
            (Ok(f), Ok(_)) => (NodeStatus::Active, Some(f)),
        };
        let rotation = scan.ok().flatten();

        let shared = Arc::new(Shared {
            q: Mutex::new(Queue {
                items: VecDeque::new(),
                queued_bytes: 0,
                draining_bytes: 0,
                dropped_bytes: 0,
                write_errors: 0,
                last_write_error: None,
                rotate_pending: 0,
                rotation,
                closed: false,
                // Pessimistic until a writer thread is actually running, so the
                // two ways a node comes up without one — an unopenable file and
                // a refused `spawn` — both drop-and-count from the first byte.
                // Race-free: nothing can enqueue before `start`, which the
                // caller runs after `create` returns.
                writer_gone: true,
                // A freshly built node has no history, so the stamp is now
                // (§7 "status … with reason and timestamp", §15.8).
                status: NodeState::new(status),
                overflow: *overflow,
            }),
            cv: Condvar::new(),
        });

        let mut node = LogNode {
            name: name.clone(),
            directory: directory.clone(),
            filename: filename.clone(),
            shared: shared.clone(),
            ingest_counters: Arc::new(DropCounters::default()),
            tasks: TaskSet::default(),
            writer: BlockingWorker::default(),
            writer_done: None,
            stop_signalled_at: None,
        };

        // Start the blocking writer thread only if the file opened; a faulted
        // node keeps no writer, and the pump (started later) drops-and-counts.
        if let Some(file) = file {
            let (done_tx, done_rx) = sync_channel::<()>(1);
            let armed = node.writer.arm_with(spawn, format!("log-{name}"), {
                let shared = shared.clone();
                let dir = directory.clone();
                let fname = filename.clone();
                let padding = rotation_padding(config);
                // The `stop` flag the worker hands over is unused here: this writer
                // waits on a `Condvar` and is told to close through `Queue::closed`
                // under the same mutex, which an `AtomicBool` cannot do.
                move |_stop| {
                    writer_loop(&shared, dir, fname, padding, file);
                    let _ = done_tx.send(());
                }
            });
            apply_spawn(&mut node, done_rx, armed);
        }
        node
    }

    /// Start the ingest pump: drain each attached edge's hostward channel into the
    /// shared queue, applying the overflow policy (§7.3). The counters ride from
    /// the wiring so full-channel ingest drops are folded into reported loss.
    ///
    /// The pump outlives every individual edge (§15.35): it parks on the endpoint's
    /// [`EdgeInbox`](crate::runtime::EdgeInbox) when nothing is connected, drains
    /// the edge it is given until `disconnect` closes that channel, and parks
    /// again. So a log connected mid-stream starts capturing at the join point,
    /// and one disconnected keeps every byte already queued.
    pub fn start(&mut self, inbox: Option<EdgeInbox>, counters: Option<Arc<DropCounters>>) {
        if let Some(counters) = counters {
            self.ingest_counters = counters;
        }
        if let Some(inbox) = inbox {
            self.tasks
                .push(tokio::task::spawn_local(pump(self.shared.clone(), inbox)));
        }
    }

    /// Request an on-demand rotation (§7.3). Non-blocking: it queues the request
    /// and wakes the writer, which performs it when the queue reaches that point
    /// — so the control plane never blocks on a `write(2)`. Returns the number
    /// the next completed rotation will carry.
    pub fn rotate(&self) -> Result<u64, String> {
        let mut q = self.shared.q.lock().unwrap();
        if let NodeStatus::Faulted { reason } = q.status.status() {
            return Err(format!("log node faulted: {reason}"));
        }
        let next = q
            .rotation
            .map_or(0, |n| n.saturating_add(1))
            .saturating_add(u64::from(q.rotate_pending));
        q.rotate_pending += 1;
        // Ordered against the queue: the marker takes its place *behind* the
        // bytes already accepted and *ahead* of everything accepted from now on,
        // which is the operator's mental model of `rotate` (§7.3).
        q.items.push_back(QueueItem::Rotate);
        self.shared.cv.notify_all();
        Ok(next)
    }

    pub fn status(&self) -> NodeState {
        self.shared.q.lock().unwrap().status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        let q = self.shared.q.lock().unwrap();
        json!({
            "current_file": self.directory.join(&self.filename).display().to_string(),
            "rotation": q.rotation,
            // Every byte still resident: waiting in the queue plus the batch the
            // writer is holding while it writes (§7.3 "queued bytes").
            "queued_bytes": q.queued_bytes + q.draining_bytes,
            // All hostward loss for this node: queue overflow plus any ingest
            // drops the serial reader counted against a full channel (§5).
            "dropped_bytes": q.dropped_bytes + self.ingest_counters.dropped_full(),
            // Write refusals absorbed by the drop-oldest policy — the part of
            // the loss that is the filesystem's doing, not the consumer's (§5).
            "write_errors": q.write_errors,
            "last_write_error": q.last_write_error.clone(),
        })
    }

    /// Phase one of teardown: stop ingest and tell the writer to close, without
    /// waiting for it. Cheap and non-blocking, so a caller tearing down several
    /// nodes can signal them all before paying anybody's flush bound — §7.3
    /// bounds the flush, it does not sanction paying that bound serially on the
    /// thread that carries the data plane.
    ///
    /// Idempotent: a second call is a no-op and, in particular, does not restart
    /// the flush deadline.
    pub fn signal_stop(&mut self) {
        // Stop new bytes first so the writer drains a fixed backlog.
        self.tasks.abort_all();
        if self.stop_signalled_at.is_some() {
            return;
        }
        self.stop_signalled_at = Some(Instant::now());
        let mut q = self.shared.q.lock().unwrap();
        q.closed = true;
        self.shared.cv.notify_all();
    }

    /// Phase two: flush and close the writer within the bounded wait (§7.3).
    /// Safe to call directly — it runs `signal_stop` first — but a multi-node
    /// teardown should signal everyone first, because the bound runs from the
    /// signal: N wedged log directories then cost one `FLUSH_WAIT` in total
    /// instead of one each.
    pub fn teardown(&mut self) {
        self.signal_stop();
        let remaining = self
            .stop_signalled_at
            .map_or(FLUSH_WAIT, |t| FLUSH_WAIT.saturating_sub(t.elapsed()));
        // Measuring the budget from the *signal* rather than from here is what makes
        // the total bound one `FLUSH_WAIT` instead of N (LOG-1), and it does not
        // shorten anyone's flush: the writer is its own thread, so the time spent
        // joining the nodes ahead of this one is time this writer spent draining.
        // `remaining == 0` therefore means "this writer has already had the full
        // bounded wait and is still going", which is precisely the wedged case §7.3
        // says to detach — and `recv_timeout(ZERO)` still reports a writer that
        // finished in the meantime, so a healthy node is joined, never abandoned.
        //
        // Bounded flush wait: if the writer is wedged on a stuck disk we detach
        // it rather than block teardown indefinitely (§7.3).
        if let Some(done) = self.writer_done.take() {
            match done.recv_timeout(remaining) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => self.writer.join(),
                // Detach the wedged writer; the process owns it until exit.
                Err(RecvTimeoutError::Timeout) => self.writer.detach(),
            }
        }
    }
}

/// Apply the writer thread's spawn result to a node under construction.
///
/// A thread that will not start is an **environmental** failure — EAGAIN under
/// `RLIMIT_NPROC`, a thread cgroup limit, memory pressure — and §15.8 says
/// environmental failure changes a node's *state*, never the graph, and never
/// fails the operation that created it. Panicking here unwound out of
/// `startup_load` → `serve` → `run`, straight through the `if let Err(e)` arm
/// that exists so a bad state file cannot cost the daemon its life; and because
/// the graph is persisted, one transient thread limit turned a log node that had
/// loaded fine into a crash loop across restarts (CONC-3). So fault the node
/// exactly as an unopenable file already does above, and leave `writer_gone`
/// set so the pump drops-and-counts instead of filling a queue nobody drains.
///
/// Factored out of [`LogNode::create`] for readability; the `spawn` that reaches
/// it is injectable ([`ThreadSpawn`]), so the guard drives the real `create`
/// rather than this handler.
fn apply_spawn(node: &mut LogNode, done_rx: StdReceiver<()>, armed: std::io::Result<()>) {
    match armed {
        Ok(()) => {
            node.writer_done = Some(done_rx);
            // The queue has a consumer now; enqueue may queue rather than shed.
            node.shared.q.lock().unwrap().writer_gone = false;
        }
        Err(e) => {
            // `done_rx` drops here: teardown then has nothing to wait on, which
            // is right — there is no writer to flush.
            let mut q = node.shared.q.lock().unwrap();
            q.status.set(NodeStatus::Faulted {
                reason: format!("spawn log writer thread: {e}"),
            });
        }
    }
}

impl Drop for LogNode {
    fn drop(&mut self) {
        if !self.tasks.is_empty() || self.writer.is_armed() {
            self.teardown();
        }
    }
}

/// The ingest pump (async, LocalSet): move hostward bytes into the bounded queue,
/// applying the overflow policy on a full queue (§7.3).
async fn pump(shared: Arc<Shared>, mut inbox: EdgeInbox) {
    while let Some(mut rx) = inbox.recv().await {
        while let Some(chunk) = rx.recv().await {
            let mut q = shared.q.lock().unwrap();
            if enqueue(&mut q, chunk) {
                shared.cv.notify_all();
            }
        }
        // The edge went away (`disconnect`, or the producing node's teardown).
        // Everything it had already buffered was drained above, so nothing is lost;
        // park until the endpoint is connected again.
    }
}

/// The pump's enqueue step: apply the overflow policy, then queue the chunk.
/// Returns whether anything was queued (i.e. whether the writer needs waking).
/// Factored out of [`pump`] so the policy is exercisable without a runtime.
fn enqueue(q: &mut Queue, chunk: Chunk) -> bool {
    let len = chunk.len();
    // Nobody will ever drain this queue again (LOGQ-1), so these bytes are loss,
    // not backlog: count them now. Queueing them instead reported a whole
    // `QUEUE_CAP_BYTES` of provably-unwritable console data as `queued_bytes`
    // while `dropped_bytes` stayed flat — an operator sizing the outage from the
    // loss counter saw nothing for the first 16 MiB — and held it in RAM until
    // the node was removed. §5: loss is always visible and attributable.
    if q.writer_gone {
        q.dropped_bytes += len as u64;
        return false;
    }
    if q.queued_bytes + len > QUEUE_CAP_BYTES {
        match q.overflow {
            OverflowPolicy::DropOldest => {
                // Evict the oldest *bytes* until the new chunk fits (or no bytes
                // remain). Rotation markers are stepped over, never evicted:
                // dropping one would silently discard the operator's `rotate`
                // and merge two files (§7.3).
                while q.queued_bytes + len > QUEUE_CAP_BYTES {
                    let Some(pos) = q
                        .items
                        .iter()
                        .position(|it| matches!(it, QueueItem::Bytes(_)))
                    else {
                        break;
                    };
                    let Some(QueueItem::Bytes(old)) = q.items.remove(pos) else {
                        break;
                    };
                    q.queued_bytes -= old.len();
                    q.dropped_bytes += old.len() as u64;
                }
            }
            OverflowPolicy::Fault => {
                q.dropped_bytes += len as u64;
                if q.status.is_active() {
                    q.status.set(NodeStatus::Faulted {
                        reason: "log queue overflow".to_owned(),
                    });
                }
                return false; // do not enqueue past the bound
            }
        }
    }
    q.queued_bytes += len;
    q.items.push_back(QueueItem::Bytes(chunk));
    true
}

/// The blocking writer thread. Drains via [`writer_drain`] and then — on **every**
/// exit path — marks the queue dead and accounts for whatever is left in it.
///
/// The wrapper exists because the drain has three exits (a fatal `write(2)` under
/// `overflow = "fault"`, any rotation failure under either policy, and the clean
/// close) and only the clean one leaves an empty queue. Handling "no writer any
/// more" at each `return` is how the first two came to say *the pump
/// drops-and-counts* in a comment while the pump did no such thing for the next
/// 16 MiB (LOGQ-1); saying it once, here, is the version that cannot drift.
fn writer_loop(shared: &Shared, dir: PathBuf, filename: String, padding: usize, file: File) {
    writer_drain(shared, dir, filename, padding, file);
    let mut q = shared.q.lock().unwrap();
    abandon_queue(&mut q);
    // Wake anything parked on the condvar (teardown's collector, a `rotate`).
    shared.cv.notify_all();
}

/// The blocking writer drain: pull the queue, `write(2)` each chunk, and perform
/// each rotation *at its place in the stream* (§7.3).
///
/// **§7.3's "flush the queue" is drain-to-`write(2)`, and this loop is the whole of
/// it.** A chunk has left the daemon when [`write_chunk`] returns: [`File`] carries
/// no userspace buffer, and `std`'s `impl Write for File` implements `flush` as
/// `Ok(())` on unix — it forwards to `sys::fs::unix::File::flush`, whose body is
/// literally `Ok(())`. Three `let _ = file.flush()` calls used to stand here, at the
/// `closing` return below, and in [`perform_rotation`], one of them gated by an `ok`
/// flag that existed for nothing else. They were deleted rather than left as
/// documentation, because a no-op spelled like a durability step gets read as one —
/// and the `ok` flag made the first of them look like a *policy* decision about when
/// data is safe (plan §18 item 55).
///
/// **fsync is deliberately not owed here, and adding it is an amendment.** §16.6
/// scopes durability to the state file — fsync the temp file and its directory
/// around the rename — on the stated ground that config mutations are rare, so the
/// cost is unmeasurable. A log is the opposite case: a console at line rate, where
/// the same promise would put a disk round-trip on the per-batch path and turn
/// §7.3's *bounded* teardown wait into a bound on the storage stack rather than on
/// the queue. Extending §16.6 to logs is therefore a design amendment with a
/// measurement behind it (AGENTS §5), never a patch to this function.
fn writer_drain(shared: &Shared, dir: PathBuf, filename: String, padding: usize, mut file: File) {
    let current = dir.join(&filename);
    loop {
        let (batch, closing) = {
            let mut q = shared.q.lock().unwrap();
            // The previous batch's vector is gone, so its bytes are no longer
            // resident.
            q.draining_bytes = 0;
            while q.items.is_empty() && !q.closed {
                q = shared.cv.wait(q).unwrap();
            }
            let batch: Vec<QueueItem> = q.items.drain(..).collect();
            // The drained bytes move from "queued" to "being written" — still
            // held by `batch`, so the reported depth must not drop to zero.
            q.draining_bytes = q.queued_bytes;
            q.queued_bytes = 0;
            (batch, q.closed)
        };

        // Write the drained batch (blocking). On error, honor the policy: fault
        // the node (and stop), or drop-and-count and keep going.
        for (i, item) in batch.iter().enumerate() {
            match item {
                QueueItem::Bytes(chunk) => {
                    if let Err((unwritten, e)) = write_chunk(&mut file, chunk) {
                        let mut q = shared.q.lock().unwrap();
                        // Only the remainder the file never took is loss. A prefix
                        // the failing `write(2)` already stored is durably in the
                        // file, and charging the whole chunk made
                        // `file_len + dropped_bytes` exceed the produced total —
                        // the exactness §5 requires of the loss accounting, and
                        // what `p12_log_queue` pins (LOG-1).
                        q.dropped_bytes += unwritten as u64;
                        match q.overflow {
                            OverflowPolicy::Fault => {
                                // Every remaining item of the drained batch is
                                // abandoned as well; count them so reported loss
                                // stays exact (§5 "all loss is counted").
                                count_abandoned(&mut q, &batch[i + 1..]);
                                q.status.set(NodeStatus::Faulted {
                                    reason: format!("write {}: {e}", current.display()),
                                });
                                shared.cv.notify_all();
                                return; // stop draining; the pump drops-and-counts
                            }
                            OverflowPolicy::DropOldest => {
                                // The node deliberately stays `active` here (§5
                                // sanctions drop-oldest-with-counters for the
                                // full-disk case), so record the refusal
                                // separately — otherwise a disk that rejects
                                // every write is indistinguishable from a merely
                                // slow one. The fault arm above needs no
                                // equivalent: its reason string says it.
                                q.write_errors += 1;
                                q.last_write_error =
                                    Some(format!("write {}: {e}", current.display()));
                            }
                        }
                    }
                }
                QueueItem::Rotate => {
                    match perform_rotation(shared, &dir, &filename, padding) {
                        Ok(f) => {
                            file = f;
                        }
                        Err(()) => {
                            // Rotation faulted the node and the writer stops here,
                            // so the rest of the drained batch is abandoned — count
                            // it (§5 "all loss is counted").
                            let mut q = shared.q.lock().unwrap();
                            count_abandoned(&mut q, &batch[i + 1..]);
                            shared.cv.notify_all();
                            return;
                        }
                    }
                }
            }
        }
        if closing {
            {
                let mut q = shared.q.lock().unwrap();
                q.draining_bytes = 0;
            }
            // Nothing to flush on the way out (see this function's doc): every
            // chunk reached the file inside `write_chunk`, and the queue is now
            // empty and `closed` — which is exactly the condition §7.3's bounded
            // teardown collector waits on.
            return;
        }
    }
}

/// One chunk's blocking write, hand-rolled so a **partial** write stays visible.
///
/// `write_all` reports the error and not how much of the buffer reached the file,
/// yet the ordinary full-disk shape is exactly a partial one: `write(2)` stores
/// what fits and the retry gets ENOSPC. Charging the whole chunk to
/// `dropped_bytes` there made `file_len + dropped_bytes` exceed the produced
/// total, which is the identity §5's "loss is always visible and attributable"
/// rests on (LOG-1 — the review-26 PTY-3 remedy applied here). Returns the
/// unwritten remainder together with the error that stopped it.
fn write_chunk(file: &mut File, chunk: &[u8]) -> Result<(), (usize, std::io::Error)> {
    let mut rest = chunk;
    while !rest.is_empty() {
        match file.write(rest) {
            // `write_all`'s own reading of a zero-length write on a non-empty
            // buffer: the destination is taking nothing, so this is a refusal to
            // count rather than a loop to spin in.
            Ok(0) => {
                return Err((
                    rest.len(),
                    std::io::Error::new(std::io::ErrorKind::WriteZero, "write returned zero bytes"),
                ));
            }
            Ok(n) => rest = &rest[n..],
            // A signal is not progress lost; retry what is left.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err((rest.len(), e)),
        }
    }
    Ok(())
}

/// Count an abandoned tail of a drained batch as loss and clear the in-flight
/// depth: the writer is returning, so those bytes are never written and nothing
/// holds them any more (§5 "all loss is counted").
fn count_abandoned(q: &mut Queue, rest: &[QueueItem]) {
    for item in rest {
        q.dropped_bytes += item.len() as u64;
    }
    q.draining_bytes = 0;
}

/// Mark the queue dead and account for everything still resident in it: the
/// writer has returned, so those bytes are loss (§5 "all loss is counted") and
/// nothing may hold them. Rotation markers count zero bytes but go with the rest
/// — an unperformed rotation cannot be performed by a writer that is gone.
fn abandon_queue(q: &mut Queue) {
    q.writer_gone = true;
    let mut orphaned = std::mem::take(&mut q.items);
    q.queued_bytes = 0;
    count_abandoned(q, orphaned.make_contiguous());
}

/// Perform one queued rotation exactly here in the stream (§7.3): rename the
/// current file to `<name>.NNN` (higher is newer), reopen fresh. Returns the
/// reopened file, or `Err` after faulting the node — no bytes cross a rotation
/// boundary mid-chunk either way.
///
/// The rename needs nothing flushed ahead of it: every chunk queued before this
/// `Rotate` marker already went through `write(2)`, so the inode being renamed is
/// complete by construction ([`writer_drain`]'s doc — the `let _ = file.flush()`
/// that stood on the first line of this body was one of three no-ops deleted with
/// plan §18 item 55). That flush was also the *only* use of the current file this
/// function had, which is why it no longer takes one: rotation is a directory
/// operation plus a reopen, and it never needed the open handle. The caller still
/// owns that handle and replaces it with the returned one.
fn perform_rotation(
    shared: &Shared,
    dir: &Path,
    filename: &str,
    padding: usize,
) -> Result<File, ()> {
    let current = dir.join(filename);
    let next = {
        let q = shared.q.lock().unwrap();
        q.rotation.map_or(0, |n| n.saturating_add(1))
    };
    let rotated = dir.join(format!("{filename}.{next:0padding$}"));
    // A failed rename means nothing rotated: no `.NNN` file was created and the
    // writer would otherwise keep appending to the unrotated file forever. Fault
    // the node (like a write/reopen failure) rather than silently no-op the
    // operator's `rotate` (§7.3).
    if let Err(e) = std::fs::rename(&current, &rotated) {
        let mut q = shared.q.lock().unwrap();
        q.status.set(NodeStatus::Faulted {
            reason: format!("rotate {} -> {}: {e}", current.display(), rotated.display()),
        });
        q.rotate_pending = q.rotate_pending.saturating_sub(1);
        shared.cv.notify_all();
        return Err(());
    }
    match open_append(&current) {
        Ok(f) => {
            let mut q = shared.q.lock().unwrap();
            q.rotation = Some(next);
            q.rotate_pending = q.rotate_pending.saturating_sub(1);
            shared.cv.notify_all();
            Ok(f)
        }
        Err(e) => {
            let mut q = shared.q.lock().unwrap();
            q.status.set(NodeStatus::Faulted {
                reason: format!("reopen after rotate {}: {e}", current.display()),
            });
            q.rotate_pending = q.rotate_pending.saturating_sub(1);
            shared.cv.notify_all();
            Err(())
        }
    }
}

fn open_append(path: &std::path::Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(LOG_FILE_MODE)
        .open(path)
}

/// Scan `dir` for `<filename>.NNN` and return the highest N (§7.3 counter
/// recovery). `Ok(None)` is "no rotations yet"; `Err` is "the scan could not run",
/// and the caller must not read one as the other (LOG-2). An entry the directory
/// listing itself refuses counts as a failed scan for the same reason: it is a
/// rotation number that cannot be ruled out.
fn scan_rotation(dir: &std::path::Path, filename: &str) -> std::io::Result<Option<u64>> {
    let prefix = format!("{filename}.");
    let mut max: Option<u64> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(suffix) = name.strip_prefix(&prefix)
            && let Ok(n) = suffix.parse::<u64>()
        {
            max = Some(max.map_or(n, |m| m.max(n)));
        }
    }
    Ok(max)
}

fn rotation_padding(config: &NodeConfig) -> usize {
    match config {
        NodeConfig::Log {
            rotation_padding, ..
        } => *rotation_padding as usize,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, unique temp directory per call (tests may run in parallel).
    fn unique_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("snx-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn log_config(dir: &Path, filename: &str, overflow: OverflowPolicy) -> NodeConfig {
        NodeConfig::Log {
            name: "lg".to_owned(),
            directory: dir.to_string_lossy().into_owned(),
            filename: filename.to_owned(),
            overflow,
            rotation_padding: 3,
        }
    }

    /// A queue in the shape the writer expects, closed so `writer_loop` runs to
    /// completion synchronously.
    fn test_queue(overflow: OverflowPolicy) -> Queue {
        Queue {
            items: VecDeque::new(),
            queued_bytes: 0,
            draining_bytes: 0,
            dropped_bytes: 0,
            write_errors: 0,
            last_write_error: None,
            rotate_pending: 0,
            rotation: None,
            closed: true,
            // A writer is about to run over this queue, so it is not dead yet.
            writer_gone: false,
            status: NodeState::new(NodeStatus::Active),
            overflow,
        }
    }

    /// Enqueue through the real pump path, so the queue bookkeeping under test
    /// is the bookkeeping production uses.
    fn push_bytes(q: &mut Queue, b: &'static [u8]) {
        assert!(enqueue(q, Chunk::from_static(b)));
    }

    // LOG-6: under overflow=fault a write(2) error abandons the whole drained
    // batch; every byte in it must still be counted so `dropped_bytes` stays
    // exact (§5 "all loss is counted"). A read-only File makes write_all fail.
    #[test]
    fn write_error_under_fault_counts_the_abandoned_batch() {
        let tmp = unique_dir("log6");
        let path = tmp.join("ro.log");
        std::fs::write(&path, b"").unwrap();
        let ro = OpenOptions::new().read(true).open(&path).unwrap();

        let mut queue = test_queue(OverflowPolicy::Fault);
        push_bytes(&mut queue, b"aaaa");
        push_bytes(&mut queue, b"bbbbbb");
        push_bytes(&mut queue, b"cc");
        let total = queue.queued_bytes as u64;

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };

        // Synchronous: the first write_all fails, the Fault arm counts the batch
        // and returns.
        writer_loop(&shared, tmp.clone(), "ro.log".to_owned(), 3, ro);

        let mut q = shared.q.lock().unwrap();
        assert_eq!(
            q.dropped_bytes, total,
            "the abandoned batch must be fully counted"
        );
        assert!(
            matches!(q.status.status(), NodeStatus::Faulted { .. }),
            "the node must fault"
        );
        // LOG-4: the batch is gone, so nothing is in flight any more.
        assert_eq!(q.draining_bytes, 0);

        // LOGQ-1: the writer has returned, so its `the pump drops-and-counts`
        // comment must be true from the very next chunk — not 16 MiB later. A
        // post-return enqueue is loss (`dropped_bytes` moves) and must not be
        // reported as backlog (`queued_bytes` stays put) or held in RAM.
        assert!(q.writer_gone, "a returned writer must mark the queue dead");
        assert!(
            !enqueue(&mut q, Chunk::from_static(b"dddddddd")),
            "a chunk offered to a dead queue must not be queued"
        );
        assert_eq!(
            q.dropped_bytes,
            total + 8,
            "the post-return chunk must be counted as loss"
        );
        assert_eq!(q.queued_bytes, 0, "it must not be reported as pending");
        assert!(q.items.is_empty(), "and it must not be retained in memory");
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-1: the ordinary full-disk shape is a **partial** write — `write(2)` stores
    // what fits and the retry fails — and the prefix it stored is durably in the file,
    // not lost. Charging the whole chunk made `written + dropped` exceed `produced`,
    // the identity §5's loss accounting rests on and `p12_log_queue` pins end to end
    // (its own guards fail at offset 0, so they never see partial progress).
    //
    // The vehicle is a non-blocking `AF_UNIX` stream standing in for the file: it is
    // the one `write(2)` target that partial-writes on demand with no privilege, no
    // filesystem to fill, and no `RLIMIT_FSIZE` whose SIGXFSZ would take the test
    // process with it. `written` is then *measured* at the far end rather than
    // assumed — the writer's `File` is the only remaining sender, so the peer reads
    // to EOF exactly what the socket took.
    #[test]
    fn a_partial_write_charges_only_the_unwritten_remainder() {
        use std::io::Read;
        use std::os::fd::OwnedFd;
        use std::os::unix::net::UnixStream;

        let tmp = unique_dir("log1-partial");
        let (sink, mut peer) = UnixStream::pair().expect("socketpair");
        sink.set_nonblocking(true).expect("non-blocking sink");

        // Comfortably past any plausible socket buffer, so the first `write(2)` stores
        // a prefix and the retry gets EWOULDBLOCK.
        let produced = 4 * 1024 * 1024_u64;
        let mut queue = test_queue(OverflowPolicy::DropOldest);
        assert!(enqueue(
            &mut queue,
            Chunk::from(vec![0x5a_u8; produced as usize])
        ));

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };
        writer_loop(
            &shared,
            tmp.clone(),
            "sock".to_owned(),
            3,
            File::from(OwnedFd::from(sink)),
        );

        let mut drained = Vec::new();
        peer.read_to_end(&mut drained).expect("drain the peer");
        let written = drained.len() as u64;

        let q = shared.q.lock().unwrap();
        if written == 0 || written == produced {
            eprintln!(
                "SKIP a_partial_write_charges_only_the_unwritten_remainder: the socket \
                 took {written} of {produced} bytes, so no partial write happened here"
            );
            drop(q);
            let _ = std::fs::remove_dir_all(&tmp);
            return;
        }
        assert_eq!(
            written + q.dropped_bytes,
            produced,
            "written ({written}) + dropped ({}) must equal produced ({produced}): the \
             prefix the failing write already stored is not loss (LOG-1)",
            q.dropped_bytes
        );
        assert_eq!(
            q.write_errors, 1,
            "one refusal, whatever the syscall count behind it"
        );
        assert!(
            q.status.is_active(),
            "drop-oldest is a sanctioned arm; a partial write must not fault the node"
        );
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-2: a directory the node can create in but not *list* (mode 0300) fails the
    // §7.3 rotation scan while `open_append` succeeds. Reading that failure as "no
    // rotations yet" reset the counter, and the next `rotate` renamed the live file
    // onto `<name>.000` — `rename(2)` replaces without a word, so an already-rotated
    // log was destroyed by the node that exists to keep it. The scan failure must
    // fault instead, which is what `create`'s comment always claimed and what makes
    // `rotate` refuse. Skipped where the mode does not bite (root, or a filesystem
    // that ignores modes).
    #[test]
    fn an_unreadable_directory_faults_the_node_instead_of_resetting_the_counter() {
        let tmp = unique_dir("log2-scan");
        std::fs::write(tmp.join("app.log.000"), b"the rotated log").unwrap();
        std::fs::write(tmp.join("app.log"), b"live").unwrap();
        // Write + traverse, no list: create and rename work, `read_dir` does not.
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o300)).unwrap();
        if std::fs::read_dir(&tmp).is_ok() {
            eprintln!(
                "SKIP an_unreadable_directory_faults_the_node_instead_of_resetting_the_counter: \
                 an unlistable directory is still listable here"
            );
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).unwrap();
            let _ = std::fs::remove_dir_all(&tmp);
            return;
        }

        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        let status = node.status();
        assert!(
            status.reason().is_some_and(|r| r.starts_with("scan ")),
            "a scan that could not run must fault the node rather than pass for a \
             fresh directory: {:?}",
            status.reason()
        );
        assert!(
            !node.writer.is_armed(),
            "a scan-faulted node starts no writer"
        );
        // The operation that would do the damage is refused for as long as the fault
        // stands, so no rename can reach the counter's blind spot (§7.3).
        assert!(
            node.rotate().is_err(),
            "rotate must be refused while the rotation counter is unknown"
        );
        node.teardown();

        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            std::fs::read(tmp.join("app.log.000")).unwrap(),
            b"the rotated log",
            "the newest rotation on disk must survive"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-3: the `overflow = "fault"` arm of a full queue, which had no test at any
    // level — its reason string appeared nowhere outside the source. Three behaviors:
    // the arriving chunk is dropped and counted rather than queued past the bound,
    // the node faults naming the overflow, and — §15.27's recorded decline — a log
    // faulted by overflow **keeps consuming** rather than wedging its pump.
    #[test]
    fn a_full_queue_under_fault_drops_counts_and_keeps_consuming() {
        let mut q = test_queue(OverflowPolicy::Fault);
        // Exactly at the bound: the last chunk that still fits, so anything after it
        // takes the overflow arm whatever its size.
        let head = QUEUE_CAP_BYTES;
        assert!(enqueue(&mut q, Chunk::from(vec![0u8; head])));
        assert!(
            q.status.is_active(),
            "a queue within the bound does not fault"
        );

        assert!(
            !enqueue(&mut q, Chunk::from_static(b"overflowing")),
            "a chunk past the bound must not be queued under the fault policy"
        );
        assert_eq!(
            q.queued_bytes, head,
            "the queue must not grow past the bound"
        );
        assert_eq!(q.dropped_bytes, 11, "the arriving chunk is counted as loss");
        assert_eq!(
            q.status.reason(),
            Some("log queue overflow"),
            "the fault must name the overflow"
        );

        // Keeps consuming: the next oversize chunk is dropped and counted too, and
        // once the writer has drained the backlog the queue accepts bytes again. An
        // overflow-faulted log goes on reporting its loss; it does not park the pump
        // behind the bound.
        assert!(!enqueue(&mut q, Chunk::from_static(b"more")));
        assert_eq!(q.dropped_bytes, 15);
        assert_eq!(q.queued_bytes, head, "the bound still holds");
        q.items.clear();
        q.queued_bytes = 0;
        assert!(
            enqueue(&mut q, Chunk::from_static(b"after")),
            "a drained queue accepts bytes again; overflow faults, it does not wedge"
        );
        assert_eq!(q.dropped_bytes, 15, "and accepting is not loss");
        assert_eq!(
            q.status.reason(),
            Some("log queue overflow"),
            "the fault itself stands until the operator acts"
        );
    }

    // LOGQ-1, the other return path: *any* rotation failure ends the writer under
    // *either* policy, so the drop-and-count must start there too. A read-only
    // directory makes the rename fail (skipped when the process can write it
    // anyway — running as root, or a filesystem that ignores modes).
    #[test]
    fn a_failed_rotation_ends_the_writer_and_the_queue_starts_shedding() {
        let tmp = unique_dir("logq1-rotate");
        let current = tmp.join("app.log");
        let file = open_append(&current).unwrap();
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o500)).unwrap();
        if std::fs::write(tmp.join("probe"), b"x").is_ok() {
            eprintln!(
                "SKIP a_failed_rotation_ends_the_writer_and_the_queue_starts_shedding: \
                 a read-only directory is still writable here"
            );
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).unwrap();
            let _ = std::fs::remove_dir_all(&tmp);
            return;
        }

        let mut queue = test_queue(OverflowPolicy::DropOldest);
        push_bytes(&mut queue, b"kept");
        queue.rotate_pending = 1;
        queue.items.push_back(QueueItem::Rotate);
        push_bytes(&mut queue, b"abandoned");

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };
        writer_loop(&shared, tmp.clone(), "app.log".to_owned(), 3, file);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700)).unwrap();

        let mut q = shared.q.lock().unwrap();
        assert!(
            matches!(q.status.status(), NodeStatus::Faulted { .. }),
            "a failed rename must fault the node"
        );
        assert!(q.writer_gone, "the writer returned; the queue is dead");
        assert_eq!(q.dropped_bytes, 9, "the abandoned tail is counted");
        assert!(!enqueue(&mut q, Chunk::from_static(b"more")));
        assert_eq!(q.dropped_bytes, 13, "post-return bytes are loss too");
        assert_eq!(q.queued_bytes, 0);
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOGQ-1: `abandon_queue` must release what the queue still holds rather than
    // leave it resident — the 16 MiB an operator could not get back short of
    // removing the node. Exercised directly because reaching a *full* queue
    // through the writer would mean actually buffering 16 MiB.
    #[test]
    fn abandoning_the_queue_releases_and_counts_what_it_still_holds() {
        let mut q = test_queue(OverflowPolicy::DropOldest);
        push_bytes(&mut q, b"aaaa");
        q.items.push_back(QueueItem::Rotate);
        push_bytes(&mut q, b"bb");
        q.draining_bytes = 7;

        abandon_queue(&mut q);

        assert!(q.writer_gone);
        assert!(q.items.is_empty(), "nothing may stay resident");
        assert_eq!(q.queued_bytes, 0);
        assert_eq!(q.draining_bytes, 0);
        assert_eq!(q.dropped_bytes, 6, "every orphaned byte is counted once");
    }

    /// A [`ThreadSpawn`] that refuses with the `EAGAIN` `pthread_create` returns
    /// under `RLIMIT_NPROC`, dropping the body unrun.
    fn refuse_spawn(
        _name: String,
        _body: Box<dyn FnOnce() + Send + 'static>,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        Err(std::io::Error::from_raw_os_error(libc::EAGAIN))
    }

    // CONC-3: a writer thread that will not start is an environmental failure
    // (§15.8) — it faults the node, it does not panic the daemon out of
    // `startup_load`.
    //
    // Driven through the real `LogNode::create` (via its [`ThreadSpawn`] seam), not
    // through `apply_spawn`. That is the whole point: the defect was a
    // `.spawn(…).expect(…)` *inside* `create`, so a guard aimed at the handler the
    // fix introduced could not have failed against the shipped code — the gap the
    // review-32 audit found. With the seam, this test compiles and runs against
    // either version of `create`. **Proved fail-first**: restoring the `.expect(…)`
    // shape in `create_with_spawner` makes this test fail by panicking out of the
    // `create` call below, which is exactly how the daemon died.
    //
    // The end-to-end half lives in `itest/tests/p12_log_queue.rs`
    // (`a_daemon_that_cannot_spawn_the_log_writer_starts_and_faults_the_node`), which
    // boots `serial-nexus-daemon` under `ulimit -u 1` and provokes the real EAGAIN.
    #[test]
    fn a_refused_writer_thread_faults_the_node_instead_of_panicking() {
        let tmp = unique_dir("conc3");
        // A directory that exists and a file that opens: everything but the thread
        // works, so the only thing under test is the refused spawn.
        let node = LogNode::create_with_spawner(
            &log_config(&tmp, "app.log", OverflowPolicy::DropOldest),
            refuse_spawn,
        );

        assert!(!node.writer.is_armed(), "no writer may be recorded");
        assert!(
            node.writer_done.is_none(),
            "teardown must have nothing to wait on"
        );
        let mut q = node.shared.q.lock().unwrap();
        assert!(
            q.status
                .reason()
                .is_some_and(|r| r.starts_with("spawn log writer thread:")),
            "the fault must name the refused spawn: {:?}",
            q.status.reason()
        );
        // …and the node behaves like any other writer-less log: it sheds and
        // counts from the first byte (LOGQ-1), rather than filling 16 MiB.
        assert!(q.writer_gone);
        assert!(!enqueue(&mut q, Chunk::from_static(b"hello")));
        assert_eq!(q.dropped_bytes, 5);
        assert_eq!(q.queued_bytes, 0);
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // CONC-3, the success arm: the real `create` over the real spawner starts a
    // writer and clears the pessimistic `writer_gone`, so the queue accepts bytes
    // normally. Same entry point as the failure arm, so the two are comparable.
    #[test]
    fn a_spawned_writer_thread_opens_the_queue_for_business() {
        let tmp = unique_dir("conc3-ok");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));

        assert!(node.writer.is_armed(), "the writer thread is recorded");
        assert!(node.writer_done.is_some());
        assert!(!node.shared.q.lock().unwrap().writer_gone);
        // The queue really is open for business, and the writer really is draining
        // it: the byte lands on disk within the flush bound.
        {
            let mut q = node.shared.q.lock().unwrap();
            assert!(enqueue(&mut q, Chunk::from_static(b"hello")));
        }
        node.shared.cv.notify_all();
        node.teardown();
        assert_eq!(
            std::fs::read(tmp.join("app.log")).expect("the log file exists"),
            b"hello",
            "the spawned writer must actually drain the queue"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // CONC-3: the writer-less node an unopenable file produces sheds from the
    // first byte too — the behaviour `create`'s comment has always claimed.
    #[test]
    fn a_log_whose_file_will_not_open_drops_and_counts_from_the_first_byte() {
        let tmp = unique_dir("conc3-noopen");
        let node = LogNode::create(&log_config(
            &tmp.join("no-such-dir"),
            "app.log",
            OverflowPolicy::DropOldest,
        ));
        assert!(!node.writer.is_armed(), "a faulted open starts no writer");
        let mut q = node.shared.q.lock().unwrap();
        assert!(q.writer_gone);
        assert!(!enqueue(&mut q, Chunk::from_static(b"hello")));
        assert_eq!(q.dropped_bytes, 5);
        assert_eq!(q.queued_bytes, 0);
        drop(q);
        assert_eq!(node.state_extra()["dropped_bytes"], json!(5));
        assert_eq!(node.state_extra()["queued_bytes"], json!(0));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // xc-panics-1 (writer site): a planted `<filename>.<u64::MAX>` makes rotation
    // recover Some(u64::MAX); the writer's next-number arithmetic must saturate,
    // not overflow-panic (debug) nor wrap to 0 (release, defeating §7.3).
    #[test]
    fn writer_rotation_number_saturates_at_u64_max() {
        let tmp = unique_dir("panics-writer");
        let current = tmp.join("app.log");
        std::fs::write(&current, b"live").unwrap();
        let file = open_append(&current).unwrap();

        let mut queue = test_queue(OverflowPolicy::DropOldest);
        queue.rotate_pending = 1;
        queue.rotation = Some(u64::MAX);
        queue.items.push_back(QueueItem::Rotate);

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };

        // Synchronous: performs the one pending rotation, then returns on
        // `closed`. Without the fix this panics at the `n + 1` in debug builds.
        writer_loop(&shared, tmp.clone(), "app.log".to_owned(), 3, file);

        let q = shared.q.lock().unwrap();
        assert_eq!(
            q.rotation,
            Some(u64::MAX),
            "rotation must pin at u64::MAX, not wrap"
        );
        assert_eq!(q.rotate_pending, 0);
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // xc-panics-1 (rotate RPC site): the operator-facing `rotate` computes the
    // next number from the directory-recovered counter; a planted
    // `<filename>.<u64::MAX>` must make it saturate rather than overflow-panic.
    #[test]
    fn rotate_rpc_number_saturates_at_u64_max() {
        let tmp = unique_dir("panics-rpc");
        std::fs::write(tmp.join(format!("app.log.{}", u64::MAX)), b"").unwrap();
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        assert_eq!(node.rotate().unwrap(), u64::MAX);
        node.teardown();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-3: rotation is ordered against the queue. Bytes accepted before the
    // `rotate` request belong to the old file, bytes accepted after to the new
    // one (§7.3 "reopens fresh at a byte boundary"). Driven through the writer
    // synchronously so the split is asserted, not raced.
    #[test]
    fn rotation_splits_the_stream_at_the_request_point() {
        let tmp = unique_dir("log3-writer");
        let current = tmp.join("app.log");
        let file = open_append(&current).unwrap();

        let mut queue = test_queue(OverflowPolicy::DropOldest);
        push_bytes(&mut queue, b"before");
        queue.rotate_pending = 1;
        queue.items.push_back(QueueItem::Rotate);
        push_bytes(&mut queue, b"after");

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };
        writer_loop(&shared, tmp.clone(), "app.log".to_owned(), 3, file);

        assert_eq!(std::fs::read(tmp.join("app.log.000")).unwrap(), b"before");
        assert_eq!(std::fs::read(&current).unwrap(), b"after");
        assert_eq!(shared.q.lock().unwrap().rotation, Some(0));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-3, through the operator-facing path: a chunk enqueued *after*
    // `LogNode::rotate` returns must land in the new file, whatever way the live
    // writer thread happens to batch the three items.
    #[test]
    fn a_chunk_enqueued_after_rotate_lands_in_the_new_file() {
        let tmp = unique_dir("log3-node");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));

        {
            let mut q = node.shared.q.lock().unwrap();
            push_bytes(&mut q, b"before");
        }
        node.shared.cv.notify_all();
        assert_eq!(node.rotate().unwrap(), 0);
        {
            let mut q = node.shared.q.lock().unwrap();
            push_bytes(&mut q, b"after");
        }
        node.shared.cv.notify_all();
        node.teardown();

        assert_eq!(std::fs::read(tmp.join("app.log.000")).unwrap(), b"before");
        assert_eq!(std::fs::read(tmp.join("app.log")).unwrap(), b"after");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-3 corollary: drop-oldest eviction must step over a rotation marker.
    // Evicting one would silently discard the operator's `rotate` and merge the
    // two files.
    #[test]
    fn overflow_eviction_never_drops_a_rotation_marker() {
        let mut q = test_queue(OverflowPolicy::DropOldest);
        let big = Chunk::from(vec![0u8; QUEUE_CAP_BYTES - 1]);
        assert!(enqueue(&mut q, big));
        q.items.push_back(QueueItem::Rotate);

        // One more byte would exceed the cap, so the oldest *bytes* are evicted.
        assert!(enqueue(&mut q, Chunk::from_static(b"xy")));
        assert_eq!(q.dropped_bytes, (QUEUE_CAP_BYTES - 1) as u64);
        assert_eq!(q.queued_bytes, 2);
        assert!(
            q.items.iter().any(|it| matches!(it, QueueItem::Rotate)),
            "the rotation marker must survive eviction"
        );
        // …and it must still precede the newly accepted bytes.
        assert!(matches!(q.items.front(), Some(QueueItem::Rotate)));
    }

    // LOG-4: `queued_bytes` must report every byte still resident — the queue
    // plus the batch the writer is holding. Built on a faulted node (unwritable
    // directory) so no writer thread races the planted values.
    #[test]
    fn queued_bytes_counts_the_batch_the_writer_still_holds() {
        let tmp = unique_dir("log4");
        let node = LogNode::create(&log_config(
            &tmp.join("no-such-dir"),
            "app.log",
            OverflowPolicy::DropOldest,
        ));
        assert!(!node.writer.is_armed(), "a faulted open starts no writer");
        {
            let mut q = node.shared.q.lock().unwrap();
            q.queued_bytes = 10;
            q.draining_bytes = 32;
        }
        assert_eq!(node.state_extra()["queued_bytes"], json!(42));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-2 (the surviving observability half): under the default drop-oldest
    // policy a filesystem refusing every write is *specified* to keep the node
    // `active` with rising `dropped_bytes` (§5). `write_errors` /
    // `last_write_error` are what separate it from a merely slow consumer.
    #[test]
    fn write_refusals_are_counted_separately_from_slow_consumer_loss() {
        let tmp = unique_dir("log2");
        let path = tmp.join("ro.log");
        std::fs::write(&path, b"").unwrap();
        let ro = OpenOptions::new().read(true).open(&path).unwrap();

        let mut queue = test_queue(OverflowPolicy::DropOldest);
        push_bytes(&mut queue, b"aaaa");
        push_bytes(&mut queue, b"bb");
        let total = queue.queued_bytes as u64;

        let shared = Shared {
            q: Mutex::new(queue),
            cv: Condvar::new(),
        };
        writer_loop(&shared, tmp.clone(), "ro.log".to_owned(), 3, ro);

        let q = shared.q.lock().unwrap();
        assert_eq!(q.write_errors, 2, "one per refused write(2)");
        assert!(
            q.last_write_error
                .as_deref()
                .is_some_and(|m| m.contains("ro.log")),
            "the message names the file: {:?}",
            q.last_write_error
        );
        assert_eq!(q.dropped_bytes, total);
        assert!(
            q.status.is_active(),
            "drop-oldest is a sanctioned policy arm; it must not fault the node"
        );
        drop(q);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // SEC-4/RV-6: console bytes are frequently root shells, so a log file must
    // not inherit a permissive umask (0664 was observed). `mode` is masked by
    // the process umask, so assert the ceiling: no bit outside 0640.
    #[test]
    fn log_files_are_created_no_wider_than_0640() {
        let tmp = unique_dir("sec4");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        let mode = std::fs::metadata(tmp.join("app.log"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode & !0o640,
            0,
            "log file mode {mode:o} is wider than 0640"
        );
        node.teardown();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-1: `signal_stop` is the cheap half — it closes the queue and returns
    // without waiting — and the following `teardown` still flushes everything
    // the writer had accepted (§7.3 bounded flush).
    #[test]
    fn signal_stop_closes_the_queue_and_teardown_still_flushes() {
        let tmp = unique_dir("log1-signal");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        {
            let mut q = node.shared.q.lock().unwrap();
            push_bytes(&mut q, b"payload");
        }
        node.shared.cv.notify_all();

        node.signal_stop();
        assert!(
            node.shared.q.lock().unwrap().closed,
            "signal_stop must tell the writer to close"
        );
        assert!(node.stop_signalled_at.is_some());
        // Idempotent: the second call must not restart the deadline.
        let first = node.stop_signalled_at;
        node.signal_stop();
        assert_eq!(node.stop_signalled_at, first);

        node.teardown();
        assert_eq!(std::fs::read(tmp.join("app.log")).unwrap(), b"payload");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // LOG-1: the flush bound runs from `signal_stop`, not from the moment this
    // node's turn in the teardown loop comes round — that is what makes N log
    // nodes cost one FLUSH_WAIT instead of N. A writer that never reports done
    // stands in for a wedged log directory.
    #[test]
    fn teardown_measures_the_flush_bound_from_signal_stop() {
        let tmp = unique_dir("log1-deadline");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        node.teardown(); // retire the real writer thread first

        let Some(long_ago) = Instant::now().checked_sub(FLUSH_WAIT) else {
            eprintln!("SKIP teardown_measures_the_flush_bound_from_signal_stop: clock too young");
            let _ = std::fs::remove_dir_all(&tmp);
            return;
        };
        // A never-signalled channel whose sender stays alive: recv_timeout can
        // only end by timing out.
        let (_keep_alive, wedged) = sync_channel::<()>(1);
        node.writer_done = Some(wedged);
        node.stop_signalled_at = Some(long_ago);

        let t0 = Instant::now();
        node.teardown();
        assert!(
            t0.elapsed() < FLUSH_WAIT / 4,
            "teardown paid a fresh FLUSH_WAIT ({:?}) instead of the remainder",
            t0.elapsed()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // STATE-1: §7 wants "a status … with reason and timestamp". The node reports
    // a NodeState, and the stamp is the age of the condition.
    #[test]
    fn status_carries_the_transition_timestamp() {
        let tmp = unique_dir("state1");
        let mut node = LogNode::create(&log_config(&tmp, "app.log", OverflowPolicy::DropOldest));
        let active = node.status();
        assert!(active.is_active());
        assert!(active.since_unix_ms() > 0, "the transition must be stamped");

        // A repeat of the identical status is not a transition, so the stamp
        // stays the age of the condition rather than of the last poll.
        {
            let mut q = node.shared.q.lock().unwrap();
            assert!(!q.status.set(NodeStatus::Active));
            assert!(q.status.set(NodeStatus::Faulted {
                reason: "disk gone".to_owned(),
            }));
        }
        let faulted = node.status();
        assert_eq!(faulted.reason(), Some("disk gone"));
        assert!(faulted.since_unix_ms() >= active.since_unix_ms());

        node.teardown();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
