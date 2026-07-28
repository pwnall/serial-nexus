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
// **A queued delete is not a queued snapshot, and coalescing the two is not symmetric.**
// A snapshot enqueued *before* a delete is superseded by it — the operator asked for the
// record to be gone. A snapshot enqueued *after* one does **not** cancel it: the delete
// still runs, and the snapshot runs after it as a new record. The first version of this
// file had a single `pending` slot assigned unconditionally, so `save → remove → save`
// wrote twice and never deleted (audit of review 32, HISTC-2). The window is narrow — the
// replacing snapshot is normally the post-clear buffer and `createWritable()` truncates —
// but if the page dies before that snapshot drains, and a `pagehide` OPFS write is
// measurably not guaranteed to land, the last *committed* record is the pre-clear
// scrollback: clear, walk away, the next viewer sees the secret. Delete-then-write is the
// only order whose every interruption point leaves the record absent rather than stale,
// so a delete gets its own slot and always drains first.
//
// Storage-free and DOM-free (the write function is injected) so it is unit-testable
// under `node --test` alongside the splice core.

/// Build a saver over `write(key, bytes, endOffset, epoch) -> Promise` and, optionally,
/// `remove(key) -> Promise`. `onError(err, key)` is called once per failed write; the
/// caller decides what a failure means (app.js drops to memory-only and says so).
///
/// The delete goes through the same per-key queue as the writes, and that is the point:
/// the operator's "clear" competes with a snapshot that may already be inside
/// `createWritable()`, and an unordered `removeEntry` simply loses that race — the write
/// completes afterwards and re-creates the record that was just deleted (review
/// HISTC-2).
export function makeSaver(write, onError = () => {}, remove = null) {
  // key -> { del: boolean, pending: {bytes, endOffset, epoch} | null, running: boolean }
  // Two slots, not one: `del` and `pending` are different intents and only one of the
  // two coalescing directions is safe (see the header).
  const queues = new Map();

  function drain(key, q) {
    (async () => {
      while (q.del || q.pending) {
        // Deletes first, always. Whatever else is queued, the operator's clear has to be
        // the thing that lands; a snapshot enqueued after it then writes a *new* record
        // rather than resurrecting the old one.
        if (q.del) {
          q.del = false; // claim it before awaiting, so a clear during the delete re-arms
          try {
            if (remove) await remove(key);
          } catch (err) {
            onError(err, key);
          }
          continue;
        }
        const next = q.pending;
        q.pending = null; // claim it before awaiting: a save during the write re-fills it
        try {
          await write(key, next.bytes, next.endOffset, next.epoch);
        } catch (err) {
          onError(err, key);
        }
      }
      // Synchronous from the last slot check through here, so nothing can be queued into
      // a slot nobody will drain.
      q.running = false;
      queues.delete(key);
    })();
  }

  function queueFor(key) {
    let q = queues.get(key);
    if (!q) {
      q = { del: false, pending: null, running: false };
      queues.set(key, q);
    }
    return q;
  }

  function start(q, key) {
    if (q.running) return;
    q.running = true;
    drain(key, q);
  }

  return {
    /// Persist a snapshot for `key`. Returns immediately; the write is serialized
    /// behind any write already in flight for that key, replacing any snapshot that
    /// was merely waiting. It does **not** cancel a queued delete — that delete still
    /// runs, and this snapshot lands after it.
    save(key, bytes, endOffset, epoch) {
      const q = queueFor(key);
      q.pending = { bytes, endOffset, epoch };
      start(q, key);
    },

    /// Delete `key`'s record, serialized behind any write already in flight for it and
    /// discarding any snapshot that was merely waiting — a snapshot taken before the
    /// operator asked for a delete has no business landing after it.
    remove(key) {
      const q = queueFor(key);
      q.del = true;
      q.pending = null;
      start(q, key);
    },

    /// Whether a write is in flight for `key` (tests and diagnostics).
    busy(key) {
      return !!queues.get(key)?.running;
    },
  };
}
