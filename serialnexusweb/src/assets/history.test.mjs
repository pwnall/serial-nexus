// Unit tests for the browser console's pure (DOM- and storage-free) modules: the
// offset-splice + retention core, and the per-key persistence serializer (§11.9). Run
// with `node --test history.test.mjs`; the nexus-itest `p8_web_history` test invokes
// exactly this file and self-skips when `node` is absent, so it is the §11.9 CI-run
// test — which is why saver.mjs's tests live here rather than in a sibling file the
// gate would not run. `opfs.mjs` is storage code and stays on the browser suite, with
// one exception imported here: its key→filename mapping is pure, and a non-injective
// one merged two consoles' records (review HIST-5).

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  newHistory,
  fromStored,
  splice,
  bytesOf,
  trim,
  noteGap,
  offsetSpaceChanged,
  reanchor,
} from "./history.mjs";
import { makeSaver } from "./saver.mjs";
import { fileName, pruneForeign } from "./opfs.mjs";

// A byte range [start, end) whose byte at position p equals p & 0xff — so a concatenation
// is verifiable by value, and any gap or duplication shows up immediately.
function seq(start, end) {
  const b = new Uint8Array(end - start);
  for (let i = 0; i < b.length; i++) b[i] = (start + i) & 0xff;
  return b;
}

test("fresh append anchors the frontier and stores the bytes", () => {
  const h = newHistory();
  const fresh = splice(h, 0, seq(0, 3));
  assert.deepEqual([...fresh], [0, 1, 2]);
  assert.equal(h.frontier, 3);
  assert.deepEqual([...bytesOf(h)], [0, 1, 2]);
});

test("a mid-stream chunk anchors at its own offset (starting late is fine)", () => {
  const h = newHistory();
  splice(h, 100, seq(100, 104));
  assert.equal(h.frontier, 104);
  assert.deepEqual([...bytesOf(h)], [...seq(100, 104)]);
});

test("reconnect replay overlap is trimmed — the log holds each byte exactly once", () => {
  const h = newHistory();
  splice(h, 0, seq(0, 10)); // first connection stored [0,10)
  // Reconnect: the replay re-sends [4,14) at offset 4. Only [10,14) is fresh.
  const fresh = splice(h, 4, seq(4, 14));
  assert.deepEqual([...fresh], [...seq(10, 14)], "only the tail past the frontier appends");
  assert.equal(h.frontier, 14);
  assert.deepEqual([...bytesOf(h)], [...seq(0, 14)], "exactly once, no gap, no duplication");
});

test("a chunk wholly behind the frontier is dropped entirely", () => {
  const h = newHistory();
  splice(h, 0, seq(0, 10));
  const fresh = splice(h, 2, seq(2, 8)); // wholly within [0,10)
  assert.equal(fresh.length, 0);
  assert.equal(h.frontier, 10);
  assert.deepEqual([...bytesOf(h)], [...seq(0, 10)]);
});

test("a real gap is counted and the whole chunk is kept", () => {
  const h = newHistory();
  splice(h, 0, seq(0, 10));
  const fresh = splice(h, 20, seq(20, 25)); // offset 20 > frontier 10 → gap of 10
  assert.deepEqual([...fresh], [...seq(20, 25)]);
  assert.equal(h.dropped, 10);
  assert.equal(h.frontier, 25);
});

test("retention trims oldest whole chunks past the cap, keeping the newest bytes", () => {
  const h = newHistory(8);
  splice(h, 0, seq(0, 6)); // total 6
  splice(h, 6, seq(6, 12)); // total 12 > 8 → drop the oldest 6-byte chunk
  assert.equal(h.total, 6);
  assert.deepEqual([...bytesOf(h)], [...seq(6, 12)]);
});

test("a single chunk larger than the cap keeps only its tail", () => {
  const h = newHistory(4);
  splice(h, 0, seq(0, 10)); // one 10-byte chunk, cap 4 → keep last 4
  assert.equal(h.total, 4);
  assert.deepEqual([...bytesOf(h)], [...seq(6, 10)]);
  trim(h); // idempotent
  assert.equal(h.total, 4);
});

test("fromStored resumes a reload: replay overlap against the stored end is trimmed", () => {
  // A prior session persisted [0,100); on reload the ring replays [50,110) at offset 50.
  const h = fromStored(seq(0, 100), 100);
  assert.equal(h.frontier, 100);
  const fresh = splice(h, 50, seq(50, 110));
  assert.deepEqual([...fresh], [...seq(100, 110)]);
  assert.deepEqual([...bytesOf(h)], [...seq(0, 110)]);
});

test("fromStored with empty bytes still sets the frontier for trimming", () => {
  const h = fromStored(new Uint8Array(0), 42);
  assert.equal(h.frontier, 42);
  const fresh = splice(h, 30, seq(30, 50)); // replay overlaps 30..42, fresh is 42..50
  assert.deepEqual([...fresh], [...seq(42, 50)]);
});

// ---- saver.mjs: one write in flight per key (review WEB-5) ----------------------

/// A write function whose promises are resolved by the test, recording the order in
/// which writes actually started.
function controllableWrites() {
  const started = [];
  const gates = [];
  const write = (key, bytes, endOffset) => {
    started.push({ key, endOffset, concurrent: gates.length });
    return new Promise((resolve, reject) => gates.push({ resolve, reject }));
  };
  return { write, started, gates };
}

test("saves on one key never overlap — the second waits for the first", async () => {
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write);
  saver.save("k", new Uint8Array(1), 10);
  saver.save("k", new Uint8Array(2), 20);
  assert.equal(started.length, 1, "the second save must not start a second writable");
  assert.equal(started[0].endOffset, 10);
  gates[0].resolve();
  await Promise.resolve(); // let the queue drain
  await Promise.resolve();
  assert.equal(started.length, 2, "the queued snapshot runs once the first completes");
  assert.equal(started[1].endOffset, 20);
  gates[1].resolve();
});

test("only the newest queued snapshot is written — each save is a full rewrite", async () => {
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write);
  saver.save("k", new Uint8Array(1), 10); // starts immediately
  saver.save("k", new Uint8Array(1), 20); // queued
  saver.save("k", new Uint8Array(1), 30); // replaces the queued one
  gates[0].resolve();
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(started.map((s) => s.endOffset), [10, 30], "20 is superseded, not written");
  gates[1].resolve();
});

test("different keys are independent — one console never blocks another", async () => {
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write);
  saver.save("a", new Uint8Array(1), 1);
  saver.save("b", new Uint8Array(1), 2);
  assert.deepEqual(started.map((s) => s.key), ["a", "b"]);
  gates[0].resolve();
  gates[1].resolve();
});

test("a write failure is reported, not swallowed", async () => {
  const errors = [];
  const { write, gates } = controllableWrites();
  const saver = makeSaver(write, (err, key) => errors.push([key, err.message]));
  saver.save("k", new Uint8Array(1), 10);
  gates[0].reject(new Error("QuotaExceeded"));
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(errors, [["k", "QuotaExceeded"]]);
  assert.equal(saver.busy("k"), false, "the queue is released after a failure");
});

test("the queue is released after a successful drain", async () => {
  const { write, gates } = controllableWrites();
  const saver = makeSaver(write);
  saver.save("k", new Uint8Array(1), 10);
  assert.equal(saver.busy("k"), true);
  gates[0].resolve();
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(saver.busy("k"), false);
});

// The `load --replace` console freeze, from the client side: the daemon rebuilds an
// endpoint's hub, so hostward offsets restart at 0 while `instance` — which is per
// *boot* — does not change. A restored history whose frontier sits past the new
// stream's start would reject every fresh chunk as already-seen.
test("a restarted offset space re-anchors the log instead of freezing it", () => {
  const h = fromStored(new Uint8Array([1, 2, 3]), 900_000);

  // Pre-condition: without re-anchoring, a post-replace chunk at offset 0 is invisible.
  const probe = fromStored(new Uint8Array([1, 2, 3]), 900_000);
  assert.equal(splice(probe, 0, new Uint8Array([9, 9])).length, 0, "the freeze itself");

  // The daemon reports which offset space the tap is in; a different one is a rebuild.
  assert.equal(offsetSpaceChanged(7, 8), true, "a different epoch is a restart");
  assert.equal(reanchor(h, 0), true);
  const fresh = splice(h, 0, new Uint8Array([9, 9]));
  assert.deepEqual(Array.from(fresh), [9, 9], "the new stream renders again");
  assert.equal(h.frontier, 2);
  // The bytes recorded before the restart are kept — they happened.
  assert.deepEqual(Array.from(bytesOf(h)), [1, 2, 3, 9, 9]);
});

// The defect the v13 browser suite found (§15.38). The previous rule inferred a
// restart from `from_offset < frontier` — which is *also* what an ordinary reload with
// a non-empty replay ring looks like, by construction, because the ring exists to
// re-send bytes the client already has. Every such reload was therefore declared a
// restart, the frontier was thrown back to the ring base, the ring was re-rendered,
// and the duplicate was written back to storage; in a real browser the stored
// scrollback grew by one copy of the ring per reload (19 → 38 → 57 bytes over three
// reloads, measured) under a false "the graph was reconfigured" marker.
//
// FAIL-FIRST: with the old two-argument rule this test's first assertion reads
// `offsetSpaceReset(h, 934464) === false` against `frontier = 1_000_000`, which is
// exactly the case that rule got wrong — it returned `true`.
test("an ordinary reload with replay overlap is not mistaken for a restart", () => {
  // A tab reloads: it stored 1 MB, and the daemon's 64 KiB ring replays the tail it
  // already has. Same hub, therefore same epoch.
  const EPOCH = 42;
  const h = fromStored(seq(0, 256), 1_000_000);
  assert.equal(
    offsetSpaceChanged(EPOCH, EPOCH),
    false,
    "the same epoch is the same offset space, whatever the offsets look like",
  );
  assert.equal(h.frontier, 1_000_000, "the frontier is untouched");

  // …and the replay that follows is trimmed rather than re-rendered.
  const replay = seq(0, 64);
  assert.equal(
    splice(h, 934_464, replay).length,
    0,
    "the ring's overlap must not be re-rendered",
  );
  assert.equal(h.frontier, 1_000_000, "and must not move the frontier backwards");
});

test("a record stored before epochs existed re-anchors rather than freezing", () => {
  // Absent is treated as changed: re-anchoring costs one duplicated ring, and the
  // alternative — assuming continuity — costs the console (it freezes for good).
  assert.equal(offsetSpaceChanged(null, 1), true);
  assert.equal(offsetSpaceChanged(undefined, 1), true);
});

test("reanchor is a no-op when the frontier is already where the stream starts", () => {
  const h = fromStored(new Uint8Array([1, 2, 3]), 100);
  assert.equal(reanchor(h, 100), false);
  assert.equal(h.frontier, 100);
});

// ---- the browser-history cluster of review 32 -----------------------------------

// HIST-1. The daemon's replay ring rotated past what this tab had stored — the ordinary
// outcome of leaving a talkative console with the default 64 KiB ring — so the stream
// resumes at an offset past our frontier, in the *same* offset space. The client used to
// splice straight across it under a `— replay (N bytes) —` marker that positively
// suggests continuity, while `dropped` counted the hole and nothing ever read it.
//
// FAIL-FIRST: `noteGap` did not exist, and the only other reader of this hole was
// `splice`'s `h.dropped +=`, which no caller inspected.
test("a ring that rotated past the stored history reports the hole, exactly once", () => {
  // The verifier's own reproduction, to the byte: 264 stored, the tap re-opens at 2050.
  const h = fromStored(seq(0, 264), 264, 1);
  assert.equal(noteGap(h, 2050), 1786, "the hole is measured and handed to the caller");
  assert.equal(h.dropped, 1786, "…and charged, so it is not lost accounting either");
  assert.equal(h.frontier, 2050, "the log resumes where the stream really starts");

  // The replay that follows is a plain append, not a second gap: charging the hole in
  // `noteGap` is what keeps it reported once rather than twice.
  const before = h.dropped;
  const fresh = splice(h, 2050, seq(50, 60));
  assert.deepEqual([...fresh], [...seq(50, 60)]);
  assert.equal(h.dropped - before, 0, "the same hole must not be counted a second time");
});

test("noteGap is silent when the stream resumes at or behind the frontier", () => {
  // The ordinary reload: the ring re-sends bytes we already have, which is what a ring
  // is for. Nothing was lost, so nothing is said.
  const h = fromStored(seq(0, 100), 100, 1);
  assert.equal(noteGap(h, 40), 0);
  assert.equal(noteGap(h, 100), 0);
  assert.equal(h.frontier, 100, "and the frontier never moves backwards");
  assert.equal(h.dropped, 0);
  // A history that has not anchored yet has no frontier to measure a hole against.
  assert.equal(noteGap(newHistory(), 500), 0);
});

// HIST-4. The key, the buffer and the epoch are one record; as three module globals the
// epoch lagged the other two by an await, so a flush in that window stamped the previous
// console's epoch onto this console's record — and because the daemon's hub epochs come
// from a process-global counter, two endpoints never share one, so the stamp was wrong by
// construction. The next selection then declared a false restart and re-rendered the ring.
test("the epoch is part of the history object, adopted with the bytes and the frontier", () => {
  const h = fromStored(seq(0, 8), 8, 7);
  assert.equal(h.epoch, 7, "restored in the same synchronous step as the buffer");
  assert.equal(h.frontier, 8);
  assert.deepEqual([...bytesOf(h)], [...seq(0, 8)]);

  // A fresh history has no offset space yet — the daemon has not said which one it is
  // talking about — and app.js's `flushSave` treats that as "nothing honest to persist".
  assert.equal(newHistory().epoch, null);
  assert.equal(fromStored(new Uint8Array(0), 0).epoch, null, "absent stays absent");
  assert.equal(offsetSpaceChanged(newHistory().epoch, 3), true);
});

// HIST-5. `/` is the endpoint separator and the one character §3 forbids inside a node
// name; `_` is legal in one. Collapsing the first onto the second made the consoles
// `mux/console` and `mux_console` share a record, and `save()`'s `createWritable()`
// truncates, so the collision was a silent mutual overwrite.
//
// FAIL-FIRST: with the old sanitiser both keys produced `hist_mux_console.bin`.
test("the storage filename distinguishes consoles that differ only by the separator", () => {
  assert.notEqual(fileName("h::mux/console::9"), fileName("h::mux_console::9"));
  // Injective over the characters that actually appear in a key — host, separator,
  // endpoint address, nonce — plus the escape character itself, whose own escaping is
  // what makes the mapping reversible.
  const keys = [
    "127.0.0.1:18099::mux/console::9",
    "127.0.0.1:18099::mux_console::9",
    "127.0.0.1:18099::mux%console::9",
    "127.0.0.1:18099::mux console::9",
    "127.0.0.1:18099::mux/console::10",
    "[::1]:18099::mux/console::9",
    "a%0025b",
    "a%b",
  ];
  const names = keys.map(fileName);
  assert.equal(new Set(names).size, keys.length, `collision among ${names.join(" ")}`);
  for (const n of names) {
    assert.match(n, /^hist_[A-Za-z0-9.%-]+\.bin$/, "a name OPFS will accept");
  }
});

// HIST-3. The record key folds in the daemon's *per-boot* `instance` nonce, and the only
// deletion the client had was `clear(key)` for the selected console — so every restart
// orphaned one capped record per console, forever, until the origin hit its quota and
// persistence died with no in-product way to reclaim the space. §15.32's retention model
// is a per-console cap; this is what keeps it one.
//
// OPFS is browser-only, so the directory is a stub here — the subject is the *filter*,
// which is where a sweep gets dangerous: too greedy and it deletes the record the client
// is about to write.
test("the sweep reclaims other boots' records and keeps this one's", async () => {
  const keep = [fileName("h:1::usb0::7"), fileName("h:1::mux/console::7")];
  const sweep = [
    fileName("h:1::usb0::6"), // the previous daemon boot
    fileName("h:1::usb0::70"), // a nonce this one is merely a prefix of
    fileName("g:2::usb0::7"), // another origin's record, if one ever appears
    "hist_h_1__usb0__7.bin", // an older client's non-injective escaping (HIST-5)
  ];
  const removed = [];
  const dir = {
    entries() {
      return (async function* () {
        for (const n of [...keep, ...sweep, "unrelated.txt"]) yield [n, { kind: "file" }];
      })();
    },
    async removeEntry(name) {
      removed.push(name);
    },
  };
  const original = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { storage: { getDirectory: async () => dir } },
  });
  try {
    assert.equal(await pruneForeign("h:1::", 7), sweep.length);
    assert.deepEqual(removed.sort(), [...sweep].sort());
  } finally {
    if (original) Object.defineProperty(globalThis, "navigator", original);
    else delete globalThis.navigator;
  }
});

test("a browser that cannot enumerate its storage loses nothing but the sweep", async () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { storage: { getDirectory: async () => ({}) } }, // no `entries`
  });
  try {
    assert.equal(await pruneForeign("h:1::", 7), 0, "degrades quietly, never throws");
  } finally {
    if (original) Object.defineProperty(globalThis, "navigator", original);
    else delete globalThis.navigator;
  }
});

// HISTC-2. `clear` competes with a debounced snapshot that may already be inside
// `createWritable()`; an unordered `removeEntry` loses that race and the write re-creates
// the record the operator just destroyed.
test("a delete is serialized behind the write already in flight for that key", async () => {
  const removed = [];
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write, () => {}, (key) => {
    removed.push(key);
    return Promise.resolve();
  });
  saver.save("k", new Uint8Array(1), 10); // in flight
  saver.remove("k"); // the operator clicks clear
  assert.deepEqual(removed, [], "the delete must not overtake the running write");
  gates[0].resolve();
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(started.length, 1, "no second write was started");
  assert.deepEqual(removed, ["k"], "and the delete lands after it, so the record is gone");
  assert.equal(saver.busy("k"), false);
});

test("a delete supersedes a snapshot that was only waiting", async () => {
  const removed = [];
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write, () => {}, (key) => {
    removed.push(key);
    return Promise.resolve();
  });
  saver.save("k", new Uint8Array(1), 10); // in flight
  saver.save("k", new Uint8Array(1), 20); // queued — taken before the clear
  saver.remove("k");
  gates[0].resolve();
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(
    started.map((s) => s.endOffset),
    [10],
    "the pre-clear snapshot must not be written after the clear",
  );
  assert.deepEqual(removed, ["k"]);
});

// The other direction, which the single `pending` slot got silently wrong (the audit of
// review 32 reproduced exactly this against the real module: the log was [write, write]
// and the delete never ran). It is HISTC-2 through a narrower window — the operator's
// clear is queued behind a large in-flight snapshot, then a `pagehide`/`visibilitychange`
// flush or the next debounce enqueues another snapshot on top of it. Losing the delete
// there means the last *committed* record is the pre-clear scrollback.
test("a save enqueued after a delete does not cancel the delete", async () => {
  const removed = [];
  const { write, started, gates } = controllableWrites();
  const saver = makeSaver(write, () => {}, (key) => {
    removed.push(key);
    return Promise.resolve();
  });
  saver.save("k", new Uint8Array(1), 10); // in flight, inside createWritable()
  saver.remove("k"); // the operator clicks clear
  saver.save("k", new Uint8Array(1), 20); // a flush lands before the write finishes
  gates[0].resolve();
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(removed, ["k"], "the operator's clear must still happen");
  assert.deepEqual(
    started.map((s) => s.endOffset),
    [10, 20],
    "and the later snapshot follows it, rather than replacing it",
  );
  gates[1].resolve();
});

// Ordering, not just presence: every interruption point of `delete → write` leaves the
// record absent or freshly written, never stale. `write → delete` would too, but
// `write → write` (the defect) leaves the pre-clear record committed.
test("a delete queued with a later save runs before it, not after", async () => {
  const order = [];
  const { write, gates } = controllableWrites();
  const recording = (key, bytes, endOffset, epoch) => {
    order.push("write");
    return write(key, bytes, endOffset, epoch);
  };
  const saver = makeSaver(recording, () => {}, () => {
    order.push("delete");
    return Promise.resolve();
  });
  saver.save("k", new Uint8Array(1), 10);
  saver.remove("k");
  saver.save("k", new Uint8Array(1), 20);
  gates[0].resolve();
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(order, ["write", "delete", "write"]);
  gates[1].resolve();
});
