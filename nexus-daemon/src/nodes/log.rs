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
//!
//! Rotation is on demand (`rotate <node>`, §7.3): the writer renames the current
//! file to `<name>.NNN` (higher is newer, no shifting cascade) and reopens fresh
//! at a byte boundary. The counter is *state*, recovered at start by scanning the
//! directory and never persisted. Removal and clean shutdown flush the queue
//! within a bounded wait before closing.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver as StdReceiver, RecvTimeoutError, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle as ThreadHandle;
use std::time::Duration;

use nexus_core::Chunk;
use nexus_core::NodeStatus;
use nexus_core::config::{NodeConfig, OverflowPolicy};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::runtime::DropCounters;

/// Upper bound on in-memory queued log bytes before the overflow policy fires
/// (§5 bounded interior). Generous enough that a briefly slow disk buffers
/// rather than drops, small enough to stay bounded.
const QUEUE_CAP_BYTES: usize = 16 * 1024 * 1024;

/// How long removal/shutdown waits for the writer to flush before detaching it
/// (§7.3 "within a bounded wait").
const FLUSH_WAIT: Duration = Duration::from_secs(2);

/// State shared between the pump (async), the writer (thread), and `state`/
/// `rotate` (control plane). One mutex guards the queue and its bookkeeping; the
/// condvar wakes the writer on new data, a rotation request, or close.
struct Shared {
    q: Mutex<Queue>,
    cv: Condvar,
}

struct Queue {
    chunks: VecDeque<Chunk>,
    queued_bytes: usize,
    dropped_bytes: u64,
    /// Number of rotations requested but not yet performed by the writer. A
    /// counter (not a flag) so rapid `rotate` calls don't collapse into one.
    rotate_pending: u32,
    /// Highest rotation suffix on disk (`<name>.NNN`); `None` until the first
    /// rotation. Recovered by directory scan at start, never persisted.
    rotation: Option<u64>,
    closed: bool,
    status: NodeStatus,
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
    pump: Option<JoinHandle<()>>,
    writer: Option<ThreadHandle<()>>,
    /// Signalled by the writer when it exits, so teardown can bound its flush
    /// wait without an unbounded `join()`.
    writer_done: Option<StdReceiver<()>>,
}

impl LogNode {
    pub fn create(config: &NodeConfig) -> LogNode {
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
        // (§7.3); a missing/unreadable directory leaves it None and surfaces as
        // an open fault below.
        let rotation = scan_rotation(&directory, filename);

        let (status, file) = match open_append(&directory.join(filename)) {
            Ok(f) => (NodeStatus::Active, Some(f)),
            Err(e) => (
                NodeStatus::Faulted {
                    reason: format!("open {}: {e}", directory.join(filename).display()),
                },
                None,
            ),
        };

        let shared = Arc::new(Shared {
            q: Mutex::new(Queue {
                chunks: VecDeque::new(),
                queued_bytes: 0,
                dropped_bytes: 0,
                rotate_pending: 0,
                rotation,
                closed: false,
                status: status.clone(),
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
            pump: None,
            writer: None,
            writer_done: None,
        };

        // Start the blocking writer thread only if the file opened; a faulted
        // node keeps no writer, and the pump (started later) drops-and-counts.
        if let Some(file) = file {
            let (done_tx, done_rx) = sync_channel::<()>(1);
            let w = std::thread::Builder::new()
                .name(format!("log-{name}"))
                .spawn({
                    let shared = shared.clone();
                    let dir = directory.clone();
                    let fname = filename.clone();
                    let padding = rotation_padding(config);
                    move || {
                        writer_loop(&shared, dir, fname, padding, file);
                        let _ = done_tx.send(());
                    }
                })
                .expect("spawn log writer thread");
            node.writer = Some(w);
            node.writer_done = Some(done_rx);
        }
        node
    }

    /// Start the ingest pump: drain the hostward channel into the shared queue,
    /// applying the overflow policy (§7.3). The counters ride from the wiring so
    /// full-channel ingest drops are folded into reported loss.
    pub fn start(
        &mut self,
        hostward: Option<mpsc::Receiver<Chunk>>,
        counters: Option<Arc<DropCounters>>,
    ) {
        if let Some(counters) = counters {
            self.ingest_counters = counters;
        }
        if let Some(rx) = hostward {
            self.pump = Some(tokio::task::spawn_local(pump(self.shared.clone(), rx)));
        }
    }

    /// Request an on-demand rotation (§7.3). Non-blocking: it queues the request
    /// and wakes the writer, which performs it between write batches — so the
    /// control plane never blocks on a `write(2)`. Returns the number the next
    /// completed rotation will carry.
    pub fn rotate(&self) -> Result<u64, String> {
        let mut q = self.shared.q.lock().unwrap();
        if let NodeStatus::Faulted { reason } = &q.status {
            return Err(format!("log node faulted: {reason}"));
        }
        let next = q
            .rotation
            .map_or(0, |n| n.saturating_add(1))
            .saturating_add(u64::from(q.rotate_pending));
        q.rotate_pending += 1;
        self.shared.cv.notify_all();
        Ok(next)
    }

    pub fn status(&self) -> NodeStatus {
        self.shared.q.lock().unwrap().status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        let q = self.shared.q.lock().unwrap();
        json!({
            "current_file": self.directory.join(&self.filename).display().to_string(),
            "rotation": q.rotation,
            "queued_bytes": q.queued_bytes,
            // All hostward loss for this node: queue overflow plus any ingest
            // drops the serial reader counted against a full channel (§5).
            "dropped_bytes": q.dropped_bytes + self.ingest_counters.dropped_full(),
        })
    }

    /// Stop ingest, then flush and close the writer within a bounded wait (§7.3).
    pub fn teardown(&mut self) {
        // Stop new bytes first so the writer drains a fixed backlog.
        if let Some(p) = self.pump.take() {
            p.abort();
        }
        {
            let mut q = self.shared.q.lock().unwrap();
            q.closed = true;
            self.shared.cv.notify_all();
        }
        // Bounded flush wait: if the writer is wedged on a stuck disk we detach
        // it rather than block teardown indefinitely (§7.3).
        if let Some(done) = self.writer_done.take() {
            match done.recv_timeout(FLUSH_WAIT) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    if let Some(w) = self.writer.take() {
                        let _ = w.join();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Detach the wedged writer; the process owns it until exit.
                    self.writer = None;
                }
            }
        }
    }
}

impl Drop for LogNode {
    fn drop(&mut self) {
        if self.pump.is_some() || self.writer.is_some() {
            self.teardown();
        }
    }
}

/// The ingest pump (async, LocalSet): move hostward bytes into the bounded queue,
/// applying the overflow policy on a full queue (§7.3).
async fn pump(shared: Arc<Shared>, mut rx: mpsc::Receiver<Chunk>) {
    while let Some(chunk) = rx.recv().await {
        let len = chunk.len();
        let mut q = shared.q.lock().unwrap();
        if q.queued_bytes + len > QUEUE_CAP_BYTES {
            match q.overflow {
                OverflowPolicy::DropOldest => {
                    // Evict oldest until the new chunk fits (or the queue empties).
                    while q.queued_bytes + len > QUEUE_CAP_BYTES {
                        let Some(old) = q.chunks.pop_front() else {
                            break;
                        };
                        q.queued_bytes -= old.len();
                        q.dropped_bytes += old.len() as u64;
                    }
                }
                OverflowPolicy::Fault => {
                    q.dropped_bytes += len as u64;
                    if q.status.is_active() {
                        q.status = NodeStatus::Faulted {
                            reason: "log queue overflow".to_owned(),
                        };
                    }
                    continue; // do not enqueue past the bound
                }
            }
        }
        q.queued_bytes += len;
        q.chunks.push_back(chunk);
        shared.cv.notify_all();
    }
}

/// The blocking writer thread: drain the queue, `write(2)` each chunk, honor
/// rotation requests between batches, and flush on close (§7.3).
fn writer_loop(shared: &Shared, dir: PathBuf, filename: String, padding: usize, mut file: File) {
    let current = dir.join(&filename);
    loop {
        let (batch, rotations, closing) = {
            let mut q = shared.q.lock().unwrap();
            while q.chunks.is_empty() && !q.closed && q.rotate_pending == 0 {
                q = shared.cv.wait(q).unwrap();
            }
            let batch: Vec<Chunk> = q.chunks.drain(..).collect();
            q.queued_bytes = 0;
            (batch, q.rotate_pending, q.closed)
        };

        // Write the drained batch (blocking). On error, honor the policy: fault
        // the node (and stop), or drop-and-count and keep going.
        let mut ok = true;
        for (i, chunk) in batch.iter().enumerate() {
            if let Err(e) = file.write_all(chunk) {
                let mut q = shared.q.lock().unwrap();
                match q.overflow {
                    OverflowPolicy::Fault => {
                        // The failing chunk and every remaining chunk of the
                        // drained batch are abandoned; count them so reported
                        // loss stays exact (§5 "all loss is counted").
                        for lost in &batch[i..] {
                            q.dropped_bytes += lost.len() as u64;
                        }
                        q.status = NodeStatus::Faulted {
                            reason: format!("write {}: {e}", current.display()),
                        };
                        shared.cv.notify_all();
                        return; // stop draining; the pump drops-and-counts
                    }
                    OverflowPolicy::DropOldest => {
                        q.dropped_bytes += chunk.len() as u64;
                    }
                }
                ok = false;
            }
        }
        if ok {
            let _ = file.flush();
        }

        // Perform any requested rotations (one file per request; higher is
        // newer). Rotating renames the current file and reopens fresh, so bytes
        // never cross a rotation boundary mid-chunk (§7.3).
        for _ in 0..rotations {
            let _ = file.flush();
            let next = {
                let q = shared.q.lock().unwrap();
                q.rotation.map_or(0, |n| n.saturating_add(1))
            };
            let rotated = dir.join(format!("{filename}.{next:0padding$}"));
            // A failed rename means nothing rotated: no `.NNN` file was created
            // and the writer would otherwise keep appending to the unrotated
            // file forever. Fault the node (like a write/reopen failure) rather
            // than silently no-op the operator's `rotate` (§7.3).
            if let Err(e) = std::fs::rename(&current, &rotated) {
                let mut q = shared.q.lock().unwrap();
                q.status = NodeStatus::Faulted {
                    reason: format!("rotate {} -> {}: {e}", current.display(), rotated.display()),
                };
                q.rotate_pending = q.rotate_pending.saturating_sub(1);
                shared.cv.notify_all();
                return;
            }
            match open_append(&current) {
                Ok(f) => file = f,
                Err(e) => {
                    let mut q = shared.q.lock().unwrap();
                    q.status = NodeStatus::Faulted {
                        reason: format!("reopen after rotate {}: {e}", current.display()),
                    };
                    q.rotate_pending = q.rotate_pending.saturating_sub(1);
                    shared.cv.notify_all();
                    return;
                }
            }
            let mut q = shared.q.lock().unwrap();
            q.rotation = Some(next);
            q.rotate_pending = q.rotate_pending.saturating_sub(1);
            shared.cv.notify_all();
        }

        if closing {
            let _ = file.flush();
            return;
        }
    }
}

fn open_append(path: &std::path::Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// Scan `dir` for `<filename>.NNN` and return the highest N (§7.3 counter
/// recovery). `None` if the directory is unreadable or has no rotations yet.
fn scan_rotation(dir: &std::path::Path, filename: &str) -> Option<u64> {
    let prefix = format!("{filename}.");
    let mut max: Option<u64> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(suffix) = name.strip_prefix(&prefix)
            && let Ok(n) = suffix.parse::<u64>()
        {
            max = Some(max.map_or(n, |m| m.max(n)));
        }
    }
    max
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
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh, unique temp directory per call (tests may run in parallel).
    fn unique_dir(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("snx-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
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

        let data: [Chunk; 3] = [
            Chunk::from_static(b"aaaa"),
            Chunk::from_static(b"bbbbbb"),
            Chunk::from_static(b"cc"),
        ];
        let total: u64 = data.iter().map(|c| c.len() as u64).sum();
        let queued: usize = data.iter().map(|c| c.len()).sum();
        let chunks: VecDeque<Chunk> = data.into_iter().collect();

        let shared = Shared {
            q: Mutex::new(Queue {
                chunks,
                queued_bytes: queued,
                dropped_bytes: 0,
                rotate_pending: 0,
                rotation: None,
                closed: true,
                status: NodeStatus::Active,
                overflow: OverflowPolicy::Fault,
            }),
            cv: Condvar::new(),
        };

        // Synchronous: the first write_all fails, the Fault arm counts the batch
        // and returns.
        writer_loop(&shared, tmp.clone(), "ro.log".to_owned(), 3, ro);

        let q = shared.q.lock().unwrap();
        assert_eq!(
            q.dropped_bytes, total,
            "the abandoned batch must be fully counted"
        );
        assert!(
            matches!(q.status, NodeStatus::Faulted { .. }),
            "the node must fault"
        );
        drop(q);
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

        let shared = Shared {
            q: Mutex::new(Queue {
                chunks: VecDeque::new(),
                queued_bytes: 0,
                dropped_bytes: 0,
                rotate_pending: 1,
                rotation: Some(u64::MAX),
                closed: true,
                status: NodeStatus::Active,
                overflow: OverflowPolicy::DropOldest,
            }),
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
        let config = NodeConfig::Log {
            name: "lg".to_owned(),
            directory: tmp.to_string_lossy().into_owned(),
            filename: "app.log".to_owned(),
            overflow: OverflowPolicy::DropOldest,
            rotation_padding: 3,
        };
        let mut node = LogNode::create(&config);
        assert_eq!(node.rotate().unwrap(), u64::MAX);
        node.teardown();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
