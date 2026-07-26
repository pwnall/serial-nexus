// Unit tests for the graph and editor pages' pure cores (§17, §15.35) — the
// DOM-free half: endpoint derivation, the dump+state fold, the `add-node` request
// builder, and the cascade the removal confirmation names. Run by
// `nexus-itest`'s `p8_web_history`, which discovers every `*.test.mjs` here.
//
// What is deliberately NOT tested here is validation, because the page does none:
// every §4 rule belongs to the daemon and the editor's job is to surface the
// refusal. A test asserting the client rejects a same-facing edge would be
// asserting a second copy of the rules into existence.

import { test } from "node:test";
import assert from "node:assert/strict";
import { endpointsOf, addressOf, model } from "./graph.mjs";
import { PALETTE, nodeParams, cascadeOf } from "./editor.mjs";

test("endpoint shapes follow the node type, not whatever state happens to show", () => {
  assert.deepEqual(endpointsOf({ type: "serial", name: "s" }), [{ name: "", facing: "host" }]);
  assert.deepEqual(endpointsOf({ type: "pty", name: "p" }), [{ name: "", facing: "target" }]);
  assert.deepEqual(endpointsOf({ type: "log", name: "l" }), [{ name: "", facing: "target" }]);
  // A map is host on its default endpoint and target on `raw` (§7.8).
  assert.deepEqual(endpointsOf({ type: "map", name: "m" }), [
    { name: "", facing: "host" },
    { name: "raw", facing: "target" },
  ]);
});

test("a demultiplexer and a re-multiplexer are mirrors of each other", () => {
  const demux = { type: "codec", name: "mux", faces: "target", channels: ["a", "b"] };
  assert.deepEqual(endpointsOf(demux), [
    { name: "", facing: "target" },
    { name: "a", facing: "host" },
    { name: "b", facing: "host" },
  ]);
  const remux = { ...demux, faces: "host" };
  assert.deepEqual(endpointsOf(remux), [
    { name: "", facing: "host" },
    { name: "a", facing: "target" },
    { name: "b", facing: "target" },
  ]);
});

test("a leg has no default endpoint — its socket lives off-graph", () => {
  const leg = { type: "leg", name: "lk", faces: "host", channels: ["c0"] };
  assert.deepEqual(endpointsOf(leg), [{ name: "c0", facing: "host" }]);
  assert.equal(addressOf("lk", "c0"), "lk/c0");
  assert.equal(addressOf("s", ""), "s");
});

test("the model folds configuration and observed state without merging them", () => {
  const dump = {
    node: [
      { type: "serial", name: "s", device: "usb:1:2:X:00" },
      { type: "pty", name: "p" },
    ],
    edge: [{ a: "s", b: "p" }],
  };
  const state = {
    nodes: [
      { name: "s", status: "active", lock: { holder: "p", waiters: [] } },
      { name: "p", status: "waiting", reason: "no client" },
    ],
  };
  const m = model(dump, state);
  assert.equal(m.nodes.length, 2);
  const s = m.nodes[0];
  assert.equal(s.type, "serial"); // configuration
  assert.equal(s.status, "active"); // observed
  assert.equal(s.endpoints[0].lock.holder, "p");
  // The pty's reason rides along, so a waiting node says why on the page.
  assert.equal(m.nodes[1].reason, "no client");
  // An omitted write_mode reads as the configured default, and the page does not
  // re-derive the runtime promotions (invariant 12).
  assert.deepEqual(m.edges, [{ a: "s", b: "p", write_mode: "on-demand" }]);
});

test("a node in the dump with no state yet renders rather than vanishing", () => {
  // add-node then a race with the next `state` tick: the node must appear, marked
  // unknown, instead of silently missing from the topology.
  const m = model({ node: [{ type: "pty", name: "fresh" }], edge: [] }, { nodes: [] });
  assert.equal(m.nodes.length, 1);
  assert.equal(m.nodes[0].status, "unknown");
});

test("channel locks land on the channel endpoints they belong to", () => {
  const dump = {
    node: [{ type: "codec", name: "mux", faces: "target", channels: ["c0", "c1"] }],
    edge: [],
  };
  const state = {
    nodes: [
      {
        name: "mux",
        status: "active",
        channels: { c0: { lock: { holder: "p0", waiters: [] } } },
      },
    ],
  };
  const m = model(dump, state);
  const byAddr = Object.fromEntries(m.nodes[0].endpoints.map((e) => [e.address, e]));
  assert.equal(byAddr["mux/c0"].lock.holder, "p0");
  assert.equal(byAddr["mux/c1"].lock, null);
  assert.equal(byAddr["mux"].lock, null, "the multiplexed side faces target: no lock");
});

test("nodeParams builds exactly the add-node body, omitting what was left blank", () => {
  const p = nodeParams("pty", { name: "p0", path: "/run/p0" });
  assert.deepEqual(p, { node: { type: "pty", name: "p0", path: "/run/p0" } });
  // A blank optional field is omitted, never sent as "": the daemon's schema has
  // real defaults and `deny_unknown_fields`, so an empty string is a value it must
  // reject rather than a field it can default.
  const m = nodeParams("map", { name: "m", hostward: "", targetward: " crlf , bsdel " });
  assert.deepEqual(m, { node: { type: "map", name: "m", targetward: ["crlf", "bsdel"] } });
  // Numbers are numbers, not strings.
  const s = nodeParams("serial", { name: "s", device: "usb:x", baud: "9600" });
  assert.equal(s.node.baud, 9600);
});

test("a missing required field fails in the page, before a pointless round trip", () => {
  assert.throws(() => nodeParams("pty", { name: "p0" }), /symlink path is required/);
  assert.throws(() => nodeParams("serial", { name: "s", device: "usb:x", baud: "fast" }), /number/);
  assert.throws(() => nodeParams("nonsuch", {}), /unknown node type/);
});

test("the palette offers no node type the daemon does not implement", () => {
  // §7.7's existing-terminal is specified and unimplemented; a palette entry that
  // always fails is worse than no entry.
  assert.ok(!PALETTE.some((p) => p.type === "existing-terminal"));
  for (const p of PALETTE) {
    assert.ok(p.fields.some((f) => f.key === "name"), `${p.type} needs a name field`);
  }
});

test("the cascade names every edge a removal takes, on either end and any endpoint", () => {
  const dump = {
    edge: [
      { a: "s", b: "mux" },
      { a: "mux/c0", b: "p0" },
      { a: "mux/c1", b: "lg" },
      { a: "other", b: "p1" },
    ],
  };
  assert.deepEqual(cascadeOf(dump, "mux"), ["s ↔ mux", "mux/c0 ↔ p0", "mux/c1 ↔ lg"]);
  assert.deepEqual(cascadeOf(dump, "p1"), ["other ↔ p1"]);
  assert.deepEqual(cascadeOf(dump, "unwired"), []);
  assert.deepEqual(cascadeOf({}, "anything"), []);
});
