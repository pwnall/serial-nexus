// Thin Origin Private File System persistence for console scrollback (design §11.9 /
// §15.32). Storage only — the splice/retention math is history.mjs, and this holds no
// logic worth unit-testing; it is exercised by the browser/manual checklist per §16.7.
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

function fileName(key) {
  return "hist_" + key.replace(/[^A-Za-z0-9._-]/g, "_") + ".bin";
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
