// Pure offset-splice + retention for one console's scrollback (design §11.9 / §15.32).
//
// The daemon stamps every `tap.data` with a monotonic hostward byte `offset` (§11.8).
// This module folds chunks into a single contiguous byte log, trimming whatever a
// reconnect's `--replay` re-sends (so a reload never duplicates ring bytes), and caps
// the retained bytes with a trim-oldest policy. It is deliberately free of the DOM and
// of any storage backend — those live in `app.js` / `opfs.mjs` — so this splice/retention
// core is unit-testable under `node --test` (that is the §11.9 CI-run test).
//
// State shape (a plain object, easy to snapshot to storage):
//   { cap, frontier, chunks: Uint8Array[], total, dropped }
// `frontier` is the next hostward offset not yet stored; `dropped` counts bytes lost to
// a real gap (the ring rotated past our frontier before we reconnected).

/// The default per-console retention cap: 16 MiB, trim-oldest (§11.9).
export const DEFAULT_CAP = 16 * 1024 * 1024;

/// A fresh, empty history. `frontier === null` until the first chunk anchors it.
export function newHistory(cap = DEFAULT_CAP) {
  return { cap, frontier: null, chunks: [], total: 0, dropped: 0 };
}

/// Rebuild a history around already-stored bytes ending at hostward offset `endOffset`
/// (what a reload loads from OPFS). The stored bytes are one chunk; `frontier` is set to
/// `endOffset` so the next `--replay` trims everything at or before it.
export function fromStored(bytes, endOffset, cap = DEFAULT_CAP) {
  const h = newHistory(cap);
  if (bytes && bytes.length) {
    h.chunks.push(bytes);
    h.total = bytes.length;
    h.frontier = endOffset;
    trim(h);
  } else {
    h.frontier = endOffset;
  }
  return h;
}

/// Fold one `(offset, bytes)` chunk into the log. Returns the *fresh* bytes that were
/// actually appended (a `Uint8Array`, possibly empty) — the caller renders exactly these,
/// so overlap a reconnect replays is neither re-rendered nor re-stored. Rules:
///   - wholly at/behind the frontier  → trimmed (empty);
///   - straddling the frontier        → only the tail past the frontier;
///   - starting past the frontier     → a gap: the whole chunk, gap size counted.
export function splice(h, offset, bytes) {
  if (h.frontier === null) {
    h.frontier = offset; // the first chunk anchors the log at its own offset
  }
  const end = offset + bytes.length;
  let fresh;
  if (end <= h.frontier) {
    return bytes.subarray(bytes.length); // wholly seen → empty, frontier unchanged
  } else if (offset <= h.frontier) {
    fresh = bytes.subarray(h.frontier - offset); // straddles → fresh tail only
  } else {
    h.dropped += offset - h.frontier; // a real gap (ring rotated past us)
    fresh = bytes;
  }
  h.chunks.push(fresh);
  h.total += fresh.length;
  h.frontier = end;
  trim(h);
  return fresh;
}

/// Whether stored scrollback belongs to a different offset space than the tap that was
/// just opened — i.e. whether the daemon rebuilt the endpoint's hub beneath it.
///
/// Hostward offsets are monotonic *per endpoint hub*, and `load --replace`,
/// `remove-node` and `add-node` all rebuild those hubs — so offsets go back to 0 while
/// the daemon `instance` nonce, which is per *boot*, does not change. A stored history
/// whose frontier sits at (say) 900 000 then rejects every fresh chunk as already-seen,
/// and the console silently freezes.
///
/// **This is answered from the daemon's `epoch`, never inferred from offsets** (§15.38),
/// because offsets cannot answer it. Both a rebuild and an ordinary reconnect present
/// as `from_offset < frontier` — the second one by construction, since a replay ring
/// exists precisely to re-send bytes the client already has. The version of this
/// function that guessed from `from_offset` shipped for two revisions and was wrong in
/// the common direction: every reload with any replay overlap was declared a restart,
/// the ring was re-rendered, and the duplicate was written back to storage, so stored
/// scrollback grew by one copy of the ring per reload. Guessing the other way would
/// have frozen the console instead. There is no third answer available from offsets.
///
/// A stored record with no epoch (written before the daemon reported one) counts as
/// changed: re-anchoring costs one duplicated ring, freezing costs the console.
export function offsetSpaceChanged(storedEpoch, epoch) {
  return storedEpoch === null || storedEpoch === undefined || storedEpoch !== epoch;
}

/// Move the log's frontier to where a new offset space actually starts, returning
/// `true` iff it moved.
///
/// The retained bytes are real and stay — they happened — but they belong to a space
/// the daemon no longer counts in. The caller is expected to mark the seam, because
/// concatenating two offset spaces without saying so is exactly the silent splice
/// §10's offset contract exists to prevent.
export function reanchor(h, fromOffset) {
  if (h.frontier === fromOffset) return false;
  h.frontier = fromOffset;
  return true;
}

/// Enforce the retention cap by dropping oldest whole chunks, then, if a single surviving
/// chunk still exceeds the cap, keeping only its tail. Never trims below the cap.
export function trim(h) {
  while (h.total > h.cap && h.chunks.length > 1) {
    h.total -= h.chunks.shift().length;
  }
  if (h.total > h.cap && h.chunks.length === 1) {
    const only = h.chunks[0];
    const keep = only.subarray(only.length - h.cap);
    h.chunks[0] = keep;
    h.total = keep.length;
  }
}

/// The whole retained log as one contiguous `Uint8Array` (for export / persistence).
export function bytesOf(h) {
  const out = new Uint8Array(h.total);
  let o = 0;
  for (const c of h.chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
}
