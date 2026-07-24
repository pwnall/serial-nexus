// Unit tests for the pure offset-splice + retention module (§11.9). Run with
// `node --test history.test.mjs`; the nexus-itest `p8_web_history` test invokes exactly
// this and self-skips when `node` is absent, so it is the §11.9 CI-run test.

import { test } from "node:test";
import assert from "node:assert/strict";
import { newHistory, fromStored, splice, bytesOf, trim } from "./history.mjs";

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
