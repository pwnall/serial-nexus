// Thin Origin Private File System persistence for console scrollback (design §15.32,
// §17's browser-side history; the track item is plan §11.9). Storage only — the
// splice/retention math is history.mjs. The I/O itself is exercised in a real browser by
// `ui-tests/tests/history.spec.mjs`; the one piece of pure logic here, the key→filename
// mapping, is exported and unit-tested, because a non-injective mapping silently merged
// two consoles' records (review HIST-5).
//
// One flat file per console-history key holds an 8-byte little-endian hostward end-offset
// header followed by the (already 16-MiB-capped) bytes, so a reload restores both the
// scrollback and the frontier the next `--replay` trims against. Best-effort by nature:
// origin storage is evictable, so callers surface the persistence-grant status rather
// than pretend durability (§15.32). Everything here degrades to a no-op / null when OPFS
// is absent, leaving the caller on its memory-only fallback.

/// Whether this browser exposes OPFS at all.
export function opfsAvailable() {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.storage &&
    typeof navigator.storage.getDirectory === "function"
  );
}

/// Ask the browser to make origin storage durable. Returns one of:
///   "persisted"   — durable (won't be evicted under pressure)
///   "best-effort" — stored, but evictable
///   "unavailable" — no persistence API
/// The UI shows this verbatim so the operator knows whether history survives eviction.
export async function requestPersistence() {
  try {
    if (navigator.storage?.persisted && (await navigator.storage.persisted())) return "persisted";
    if (navigator.storage?.persist) {
      return (await navigator.storage.persist()) ? "persisted" : "best-effort";
    }
  } catch {
    /* fall through */
  }
  return "unavailable";
}

/// The storage name for one console-history key. **The mapping has to be injective**,
/// and the obvious sanitiser is not: replacing every unsafe character with `_` maps the
/// endpoint separator `/` onto a character that is itself legal inside a node name (§3
/// bans only `/`), so the consoles `mux/console` and `mux_console` shared one record and
/// `save()`'s `createWritable()` truncated one console's scrollback into the other's
/// (review HIST-5). Percent-escaping is injective because `%` is itself escaped.
///
/// The escape body is a *fixed* four hex digits, which is the half that is easy to get
/// wrong: a variable-width body is not injective either — `!` (`%21`) followed by the
/// literal digits `92` and the single code unit `→` (`%2192`) would produce the same
/// name. Exported so `history.test.mjs` can assert the injectivity directly.
export function fileName(key) {
  return "hist_" + encodeKey(key) + ".bin";
}

function encodeKey(key) {
  return key.replace(
    /[^A-Za-z0-9.-]/g,
    (c) => "%" + c.charCodeAt(0).toString(16).padStart(4, "0"),
  );
}

async function root() {
  return navigator.storage.getDirectory();
}

/// The record header: an 8-byte magic-and-version tag, then the hostward end-offset and
/// the offset-space `epoch` that offset counts in (§15.38), both little-endian u64.
///
/// The tag is what makes the format upgradable at all. The first version stored the
/// end-offset alone with no framing, so a reader could not tell a v1 record from a v2
/// one; a record without the tag is therefore treated as absent, and that console's
/// stored scrollback starts over once. That is the honest trade for scrollback the
/// design already calls best-effort and evictable (§15.32) — the alternative is
/// guessing an epoch for bytes that were stored before epochs existed, which is exactly
/// the guessing §15.38 removed.
const MAGIC = "SNXHIST2";
const HEADER = 24;

/// Load `{ bytes, endOffset, epoch }` for `key`, or `null` if there is nothing stored
/// in a format this version understands.
export async function load(key) {
  try {
    const fh = await (await root()).getFileHandle(fileName(key), { create: false });
    const buf = new Uint8Array(await (await fh.getFile()).arrayBuffer());
    if (buf.length < HEADER) return null;
    for (let i = 0; i < MAGIC.length; i++) {
      if (buf[i] !== MAGIC.charCodeAt(i)) return null;
    }
    const dv = new DataView(buf.buffer, buf.byteOffset, HEADER);
    return {
      bytes: buf.subarray(HEADER),
      endOffset: Number(dv.getBigUint64(8, true)),
      epoch: Number(dv.getBigUint64(16, true)),
    };
  } catch {
    return null;
  }
}

/// Persist `bytes` (ending at hostward offset `endOffset` in offset space `epoch`) for
/// `key`. Overwrites — the buffer is already capped in memory, so this is a bounded
/// snapshot, not unbounded growth. Rejects only on a genuine storage error, which the
/// caller may surface.
export async function save(key, bytes, endOffset, epoch) {
  const fh = await (await root()).getFileHandle(fileName(key), { create: true });
  const w = await fh.createWritable();
  const header = new Uint8Array(HEADER);
  for (let i = 0; i < MAGIC.length; i++) header[i] = MAGIC.charCodeAt(i);
  const dv = new DataView(header.buffer);
  dv.setBigUint64(8, BigInt(endOffset), true);
  dv.setBigUint64(16, BigInt(epoch ?? 0), true);
  await w.write(header);
  if (bytes.length) await w.write(bytes);
  await w.close();
}

/// Delete `key`'s stored history (the UI's "clear" control).
export async function clear(key) {
  try {
    await (await root()).removeEntry(fileName(key));
  } catch {
    /* already gone */
  }
}

/// Reclaim every stored history record that does not belong to this origin and this
/// daemon boot, returning how many were removed. Never throws: an enumeration this
/// browser does not support, or a record another tab is holding, degrades to "reclaimed
/// fewer than we hoped", which is the correct failure for a best-effort cache.
///
/// Records are keyed by `host::endpoint::instance`, and `instance` is the daemon's
/// *per-boot* nonce (§10's `info`) — so every restart mints a whole new set of filenames
/// and, with only `clear(key)` for deletion, the old set was immortal. §15.32's retention
/// model is "a per-console cap (default 16 MiB, trim-oldest)", i.e. bounded by
/// consoles; the nonce silently multiplied that bound by the number of restarts, until
/// the origin hit its quota and persistence died for good with no in-product way to
/// reclaim the space (review HIST-3). One sweep per connection restores the stated
/// bound. It also reclaims records written by earlier versions of this client, whose
/// filenames used a different (non-injective — HIST-5) escaping and therefore match
/// neither the kept prefix nor the kept suffix.
export async function pruneForeign(prefix, keepInstance) {
  try {
    const dir = await root();
    if (typeof dir.entries !== "function") return 0;
    const head = "hist_" + encodeKey(prefix);
    const tail = encodeKey("::" + keepInstance) + ".bin";
    // Collect first, delete second: removing entries from a directory while iterating
    // it is exactly the kind of thing that skips half the list.
    const doomed = [];
    for await (const [name] of dir.entries()) {
      if (!name.startsWith("hist_") || !name.endsWith(".bin")) continue;
      if (name.startsWith(head) && name.endsWith(tail)) continue;
      doomed.push(name);
    }
    let removed = 0;
    for (const name of doomed) {
      try {
        await dir.removeEntry(name);
        removed += 1;
      } catch {
        /* raced with another tab; it is a cache */
      }
    }
    return removed;
  } catch {
    return 0;
  }
}
