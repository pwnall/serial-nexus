// Unit tests for the browser console's pure (DOM- and storage-free) modules: the
// offset-splice + retention core, and the per-key persistence serializer (§11.9). Run
// with `node --test history.test.mjs`; the nexus-itest `p8_web_history` test invokes
// exactly this file and self-skips when `node` is absent, so it is the §11.9 CI-run
// test — which is why saver.mjs's tests live here rather than in a sibling file the
// gate would not run.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  newHistory,
  fromStored,
  splice,
  bytesOf,
  trim,
  offsetSpaceChanged,
  reanchor,
} from "./history.mjs";
import { makeSaver } from "./saver.mjs";

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
