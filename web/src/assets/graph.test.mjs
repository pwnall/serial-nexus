// Unit tests for the graph and editor pages' pure cores (§17, §15.35) — the
// DOM-free half: endpoint derivation, the dump+state fold, the `add-node` request
// builder, and the cascade the removal confirmation names. Run by
// `serial-nexus-itest`'s `p8_web_history`, which discovers every `*.test.mjs` here.
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

// ---- what lets the graph page skip a repaint (§15.70, plan §18 item 91) -------------
//
// `app.js` repaints the graph page only when `model(dump, state)` serializes to
// something it has not already drawn, because §10 publishes a `state` snapshot five
// times a second whether or not anything changed and the page was rebuilt from scratch on
// every one of them. That skip is sound because `render` is a pure function of this
// model — it is handed the model and reads nothing else — so the safety half needs no
// test: a field the page draws is necessarily a field the model carries.
//
// The half that can rot is the other one. The model must carry **nothing that moves on
// its own**, or the skip stops skipping and the page silently goes back to five rebuilds
// a second with every other test in this file still green. That is the regression these
// two tests exist for, and it is the shape AGENTS §3 warns about: nothing would look
// broken.
test("the model carries no counter and no clock, so a busy console does not repaint the graph", () => {
  const dump = {
    node: [
      { type: "serial", name: "s", device: "usb:1:2:X:00" },
      { type: "map", name: "m" },
    ],
    edge: [{ a: "s", b: "m/raw" }],
  };
  // One `state` snapshot, and the next one 200 ms later on a console that is passing
  // bytes: every counter has moved, the timestamps have moved, a tap has come and gone,
  // and the graph itself has not changed at all.
  const quiet = {
    nodes: [
      {
        name: "s",
        status: "active",
        since_unix_ms: 1_000,
        lock: { arbitration: "free-for-all", holder: null, waiters: [] },
        hostward: { bytes_in: 0, bytes_out: 0, discarded_unattached: 0, rules: {} },
        targetward: { bytes_in: 0, bytes_out: 0, discarded_at_teardown: 0, rules: {} },
      },
      {
        name: "m",
        status: "active",
        since_unix_ms: 1_000,
        raw: { dropped_slow_consumer: 0 },
      },
    ],
    taps: [],
  };
  const busy = {
    nodes: [
      {
        ...quiet.nodes[0],
        since_unix_ms: 1_200,
        hostward: { bytes_in: 98_304, bytes_out: 98_304, discarded_unattached: 41, rules: {} },
        targetward: { bytes_in: 12, bytes_out: 12, discarded_at_teardown: 0, rules: {} },
      },
      { ...quiet.nodes[1], since_unix_ms: 1_200, raw: { dropped_slow_consumer: 7 } },
    ],
    taps: [{ tap: "t1", endpoint: "s" }],
  };
  assert.equal(
    JSON.stringify(model(dump, quiet)),
    JSON.stringify(model(dump, busy)),
    "a snapshot that differs only in counters and clocks must fold to the same model",
  );
});

test("the model moves for every field the graph page draws", () => {
  // Every case is a **pair** rather than a mutation of one shared base, and each pair
  // differs in exactly one drawn field wherever the data model allows it. The first
  // draft of this test compared everything to one base and moved two fields at once in
  // its waiter case — so dropping the waiter list from the model left it green, which is
  // AGENTS §3's assertion-weaker-than-its-comment in a test I had just written. It was
  // found by planting that exact drop and watching nothing redden.
  const node = (over) => ({ type: "serial", name: "s", device: "usb:1:2:X:00", ...over });
  const obs = (over) => ({ name: "s", status: "active", ...over });
  const pair = (d, st) => [{ node: [d], edge: [] }, { nodes: [st] }];
  const cases = {
    status: [pair(node(), obs()), pair(node(), obs({ status: "faulted" }))],
    reason: [pair(node(), obs({ reason: null })), pair(node(), obs({ reason: "device … lost" }))],
    "lock holder": [
      pair(node(), obs({ lock: { holder: null, waiters: [] } })),
      pair(node(), obs({ lock: { holder: "ctl", waiters: [] } })),
    ],
    // Holder fixed on both sides: only the queue behind it moves, which is the `+n`
    // badge and nothing else.
    "waiter count": [
      pair(node(), obs({ lock: { holder: "ctl", waiters: [] } })),
      pair(node(), obs({ lock: { holder: "ctl", waiters: [{ origin: "web" }] } })),
    ],
    // `pty` and `log` draw the same single target-facing endpoint, so this pair moves the
    // type word on the card and nothing else.
    "node type": [
      pair({ type: "pty", name: "p" }, { name: "p", status: "active" }),
      pair({ type: "log", name: "p" }, { name: "p", status: "active" }),
    ],
    // A name change necessarily moves the endpoint addresses with it — the address is
    // derived from the name — so this case cannot isolate further, and does not claim to.
    "node name": [pair(node(), obs()), pair(node({ name: "s2" }), { name: "s2", status: "active" })],
    "an endpoint set": [
      pair({ type: "codec", name: "c", faces: "target", channels: ["c0"] }, { name: "c", status: "active" }),
      pair(
        { type: "codec", name: "c", faces: "target", channels: ["c0", "c1"] },
        { name: "c", status: "active" },
      ),
    ],
    "an edge": [
      [{ node: [node()], edge: [] }, { nodes: [obs()] }],
      [{ node: [node()], edge: [{ a: "s", b: "m/raw" }] }, { nodes: [obs()] }],
    ],
    "a write mode": [
      [{ node: [node()], edge: [{ a: "s", b: "m/raw" }] }, { nodes: [obs()] }],
      [{ node: [node()], edge: [{ a: "s", b: "m/raw", write_mode: "held" }] }, { nodes: [obs()] }],
    ],
  };
  for (const [what, [[da, sa], [db, sb]]] of Object.entries(cases)) {
    assert.notEqual(
      JSON.stringify(model(da, sa)),
      JSON.stringify(model(db, sb)),
      `${what} is drawn on the graph page, so it must move the model`,
    );
  }
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
