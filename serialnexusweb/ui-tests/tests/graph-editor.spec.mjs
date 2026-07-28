// The graph and editor pages, and the bridge's posture, in a real browser
// (design §15.35/§15.37, plan §15.2).
//
// `p8_web.rs` asserts the API half of all of this — that the bridge relays `dump` and
// `state`, that a scripted fault flips a status, that the editor's verbs pass and the
// lifecycle verbs do not. What only a browser can add is that the *page* draws what
// those verbs return, that its controls issue them, and that the daemon's refusal
// reaches the operator's eyes verbatim rather than being paraphrased into "failed".

import { test, expect } from "@playwright/test";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import {
  ECHO,
  FAULT_A,
  FAULT_B,
  FAULT_NODE,
  RUN,
  ctl,
  open,
  selectConsole,
  selectView,
  send,
  token,
} from "./fixture.mjs";

/** The graph card for one node, matched on its exact name. */
function gnode(page, name) {
  return page.locator(`#graphview .gnode:has(.gname:text-is("${name}"))`);
}

test("the graph page draws the daemon's own topology", async ({ page }) => {
  await open(page);
  await selectView(page, "graph");

  // Topology from `dump`, status from `state` — §15.8's split, kept in the client
  // (graph.mjs). A page that took both from one verb would be inventing a third view
  // of the graph that neither verb reports; `state` does not carry edges and must not.
  await expect(gnode(page, FAULT_A)).toBeVisible();
  await expect(gnode(page, FAULT_NODE)).toBeVisible();
  await expect(gnode(page, FAULT_NODE).locator(".gtype")).toHaveText("map");

  // The edge only exists in `dump`, so drawing it proves the configuration half is
  // being read rather than guessed from observed state.
  const edge = page.locator(`#graphview .gedge:has(.gaddr:text-is("${FAULT_B}"))`);
  await expect(edge).toBeVisible();
  await expect(edge.locator(".gaddr").first()).toHaveText(FAULT_A);

  // A host-facing endpoint wears its address; that is where arbitration lives (§6).
  await expect(
    gnode(page, FAULT_NODE).locator(`.gep.host .gaddr:text-is("${FAULT_NODE}")`),
  ).toBeVisible();
});

test("a scripted fault flips the indicator through waiting and back", async ({
  page,
}) => {
  await open(page);
  await selectView(page, "editor");

  // Disconnecting an interior node's upstream is a real fault an operator can cause,
  // and the honest report of it is `waiting` (§15.8) — device-free, so this runs on
  // every platform. Driven through the editor's own control, confirmation included,
  // because the confirmation is part of what §15.35 specified.
  const row = page.locator(`#editorview .gedge:has(.gaddr:text-is("${FAULT_B}"))`);
  await expect(row).toBeVisible();
  page.once("dialog", (d) => {
    expect(d.message()).toContain("purged");
    d.accept();
  });
  await row.getByRole("button", { name: "disconnect" }).click();
  await expect(page.locator("#editorview .estatus.ok")).toContainText("disconnect");

  await selectView(page, "graph");
  // Live via `subscribe` (§17): no reload, no poll in the test.
  await expect(gnode(page, FAULT_NODE)).toHaveClass(/waiting/);
  await expect(gnode(page, FAULT_NODE).locator(".gstatus")).toHaveText("◌");

  // And back, so the indicator is tracking state rather than latching once.
  await selectView(page, "editor");
  await page.locator('#editorview input[placeholder^="endpoint a"]').fill(FAULT_A);
  await page.locator('#editorview input[placeholder^="endpoint b"]').fill(FAULT_B);
  await page.getByRole("button", { name: "connect", exact: true }).click();
  await expect(page.locator("#editorview .estatus.ok")).toContainText("connect");

  await selectView(page, "graph");
  await expect(gnode(page, FAULT_NODE)).toHaveClass(/active/);
});

test("an illegal connect surfaces the daemon's named structural error", async ({
  page,
}) => {
  await open(page);
  await selectView(page, "editor");

  // Two host-facing endpoints. The editor enforces nothing on purpose (editor.mjs):
  // every §4 rule is the daemon's to apply, and a client that pre-validated would grow
  // a second, drifting copy of the rules.
  await page.locator('#editorview input[placeholder^="endpoint a"]').fill(FAULT_A);
  await page.locator('#editorview input[placeholder^="endpoint b"]').fill(FAULT_NODE);
  await page.getByRole("button", { name: "connect", exact: true }).click();

  const status = page.locator("#editorview .estatus.err");
  await expect(status).toBeVisible();
  // Verbatim: the operator reads the rule that was broken, which is the only precise
  // thing they got.
  await expect(status).toContainText("two host-facing endpoints");
  await expect(status).toContainText("must join one host to one target");

  // Nothing changed — a refusal that half-applied would be worse than one that failed.
  const edges = ctl("dump").edge || [];
  expect(edges.some((e) => e.a === FAULT_A && e.b === FAULT_NODE)).toBe(false);
});

test("adding a console through the editor makes bytes flow end to end", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  const name = `uilog${Date.now().toString(36)}`;
  const file = `${name}.log`;
  const path = join(RUN, file);

  await open(page);
  await selectView(page, "editor");

  try {
    // The palette is data (editor.mjs's PALETTE), so the form follows the type.
    await page.locator("#editorview select").first().selectOption("log");
    const fields = page.locator("#editorview .eform .efield");
    await fields.nth(0).locator("input").fill(name);
    await fields.nth(1).locator("input").fill(RUN);
    await fields.nth(2).locator("input").fill(file);
    await page.getByRole("button", { name: "add node" }).click();
    await expect(page.locator("#editorview .estatus.ok")).toContainText(name);

    await page
      .locator('#editorview input[placeholder^="endpoint a"]')
      .fill(ECHO);
    await page.locator('#editorview input[placeholder^="endpoint b"]').fill(name);
    await page.getByRole("button", { name: "connect", exact: true }).click();
    await expect(page.locator("#editorview .estatus.ok")).toContainText(
      "connect",
    );

    // Built entirely from the browser; now drive bytes through what the browser built.
    await selectView(page, "console");
    await selectConsole(page, ECHO);
    const t = token("editor-built");
    await send(page, t);
    await expect(page.locator("#term")).toContainText(t);

    // The log is a lossless sink (§7.3), so it is the byte-exact oracle here — the
    // terminal is the lossy end of the same stream (§5).
    await expect
      .poll(() => (existsSync(path) ? readFileSync(path, "utf8") : ""), {
        timeout: 15_000,
      })
      .toContain(t);
  } finally {
    // Restore the shared fixture. This spec is the only one that permanently attaches
    // a consumer to the echo console, and leaving it there is not free: a later spec
    // reading that node's `discarded_unattached` as its "did the device emit?" oracle
    // measures zero forever, because the bytes now have somewhere to go. That failed
    // `history.spec.mjs`'s ring-rotation guard in the full run while it passed in
    // isolation — the classic shared-fixture ordering bug. A spec that mutates the
    // graph restores it (`--cascade` takes the edge with the node).
    ctl("remove-node", name, "--cascade");
  }
});

test("lifecycle verbs are still refused over the page's own transport", async ({
  page,
}) => {
  await open(page);

  // Invariant 11, asserted from inside the browser rather than from a Rust client: the
  // allowlist widened for graph editing (§15.35) and `load`/`teardown`/`shutdown`
  // stayed off the wire. Same origin, same cookie, same socket the console uses.
  const replies = await page.evaluate(async () => {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    await new Promise((res, rej) => {
      ws.onopen = res;
      ws.onerror = rej;
    });
    // Log every frame the server sends, and match on *predicates* rather than on
    // arrival order: the server auto-subscribes a fresh connection and answers that
    // with a frame of its own, so "the next frame" is not a stable handle. A refusal
    // of a parsed request keeps its id; a frame the server could not parse as one
    // request has no id to keep, and is matched on being exactly that.
    const seen = [];
    const waiters = [];
    ws.addEventListener("message", (ev) => {
      seen.push(JSON.parse(ev.data));
      for (let i = waiters.length - 1; i >= 0; i--) {
        const hit = seen.find(waiters[i].pred);
        if (hit) waiters.splice(i, 1)[0].res(hit);
      }
    });
    const frame = (pred) => {
      const hit = seen.find(pred);
      return hit ? Promise.resolve(hit) : new Promise((res) => waiters.push({ pred, res }));
    };

    const out = {};
    let nextId = 1000;
    const call = async (method) => {
      const id = nextId++;
      ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: null }));
      return { id, msg: await frame((m) => m.id === id) };
    };
    for (const m of ["shutdown", "teardown", "load", "set-attribute"]) {
      out[m] = await call(m);
    }
    // The screening is structural, not textual: one frame is parsed as exactly one
    // request, so a second request smuggled behind a newline is refused as an invalid
    // request rather than split into two dispatches (review WEB-1/SEC-1).
    ws.send(
      '{"jsonrpc":"2.0","id":4242,"method":"info"}\n' +
        '{"jsonrpc":"2.0","id":4243,"method":"teardown"}',
    );
    out.smuggled = await frame((m) => m.id === null && m.error);
    // And a verb the console legitimately needs still passes, so the assertions above
    // are about the allowlist rather than about a dead socket.
    out.allowed = await call("state");
    out.all = seen;
    ws.close();
    return out;
  });

  const log = JSON.stringify(replies.all);
  for (const m of ["shutdown", "teardown", "load", "set-attribute"]) {
    const { id, msg } = replies[m];
    expect(msg.error, `${m} must be refused; frames were ${log}`).toBeTruthy();
    expect(msg.error.code, `${m} refusal code`).toBe(-32601);
    expect(msg.id, `${m} refusal keeps its id for correlation`).toBe(id);
  }
  // The smuggled frame is refused whole, as an invalid request…
  expect(replies.smuggled.error).toBeTruthy();
  expect(replies.smuggled.error.code).toBe(-32600);
  // …and nothing correlated to the request hiding behind the newline comes back.
  expect(replies.all.some((m) => m.id === 4243)).toBe(false);
  expect(replies.allowed.msg.result).toBeTruthy();

  // The graph is intact: a `teardown` that got through would show as an empty node
  // list (the reviewer saw `{"torn_down":2}`).
  expect((ctl("state").nodes || []).length).toBeGreaterThan(0);
});
