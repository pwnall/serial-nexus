// Per-key write serialization for console scrollback (design §11.9 / §15.32).
//
// Persisting history is a *full-buffer rewrite* of a capped snapshot, and the OPFS
// adapter's `createWritable()` truncates the file before the first `write` lands. Two
// overlapping saves on one key therefore interleave a truncate into the middle of the
// other's write, and the debounced save in app.js is fire-and-forget, so overlap is not
// hypothetical: a slow write plus a visibilitychange flush is enough (review WEB-5).
//
// The rule here: at most one write in flight per key, and at most one snapshot waiting.
// Coalescing to the newest snapshot is correct precisely because each save is a complete
// rewrite — an older snapshot is a strict prefix of the newer one's information, so
// skipping it loses nothing. Failures surface through `onError` rather than vanishing
// into an unobserved promise.
//
// Storage-free and DOM-free (the write function is injected) so it is unit-testable
// under `node --test` alongside the splice core.

/// Build a saver over `write(key, bytes, endOffset) -> Promise`. `onError(err, key)` is
/// called once per failed write; the caller decides what a failure means (app.js drops to
/// memory-only and says so).
export function makeSaver(write, onError = () => {}) {
  // key -> { pending: {bytes, endOffset} | null, running: boolean }
  const queues = new Map();

  function drain(key, q) {
    (async () => {
      try {
        while (q.pending) {
          const next = q.pending;
          q.pending = null; // claim it before awaiting: a save during the write re-fills it
          await write(key, next.bytes, next.endOffset);
        }
      } catch (err) {
        q.pending = null;
        onError(err, key);
      }
      // Synchronous from the last `pending` check through here, so no snapshot can be
      // queued into a slot nobody will drain.
      q.running = false;
      queues.delete(key);
    })();
  }

  return {
    /// Persist a snapshot for `key`. Returns immediately; the write is serialized
    /// behind any write already in flight for that key, replacing any snapshot that
    /// was merely waiting.
    save(key, bytes, endOffset) {
      let q = queues.get(key);
      if (!q) {
        q = { pending: null, running: false };
        queues.set(key, q);
      }
      q.pending = { bytes, endOffset };
      if (q.running) return;
      q.running = true;
      drain(key, q);
    },

    /// Whether a write is in flight for `key` (tests and diagnostics).
    busy(key) {
      return !!queues.get(key)?.running;
    },
  };
}
