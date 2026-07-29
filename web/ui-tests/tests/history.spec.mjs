// Browser-side history: the OPFS adapter and the offset splice, asserted where they
// actually run (design §15.32/§15.37, plan §15.2).
//
// `history.test.mjs` already unit-tests the splice arithmetic under `node --test`, and
// its own doc has said since v10 that "the OPFS adapter itself is browser-only and rides
// the manual checklist". This file is what takes that sentence off the checklist: the
// same arithmetic, driven through a real Origin Private File System, a real reload, and
// a real replay ring.

import { test, expect } from "@playwright/test";
import {
  ECHO,
  countInTerminal,
  ctl,
  open,
  selectConsole,
  send,
  token,
} from "./fixture.mjs";

/** The names of the stored history records whose bytes contain `needle`, read out of the
 * page's own Origin Private File System. OPFS is reachable only from the renderer, so
 * "is it really gone from storage?" is a question only the browser can answer — `#term`
 * cannot, which is why the older clear spec could not have caught HISTC-2. */
async function recordsContaining(page, needle) {
  return page.evaluate(async (want) => {
    const root = await navigator.storage.getDirectory();
    const hits = [];
    for await (const [name, handle] of root.entries()) {
      if (!name.startsWith("hist_") || handle.kind !== "file") continue;
      try {
        const text = await (await handle.getFile()).text();
        if (text.includes(want)) hits.push(name);
      } catch {
        // A record being rewritten right now is unreadable for an instant. Count it as
        // present: the callers poll, so a conservative answer costs one retry, while an
        // optimistic one would let "gone" mean "momentarily unreadable".
        hits.push(name);
      }
    }
    return hits;
  }, needle);
}

/** How many bytes the echo node has read from its device and handed to no graph consumer.
 * The fixture wires no consumer to it, so this counts every hostward byte — the daemon's
 * own measure of how far the endpoint's offset space has advanced, used to prove the ring
 * really did rotate before judging what the console said about it. */
function hostwardBytes() {
  const st = ctl("state");
  const n = (st.nodes || []).find((x) => x.name === ECHO);
  return (n && n.discarded_unattached) || 0;
}

/**
 * `discarded_unattached` counts hostward bytes that found no graph consumer (§5), so it
 * only reads "what the device emitted" while the echo console has no edges. Assert that
 * precondition rather than discovering its violation as a mystery: a sibling spec that
 * attaches a sink and forgets to detach it makes the counter sit at zero forever, which
 * reads exactly like "the device never spoke". It has happened once already
 * (`graph-editor.spec.mjs` left a log node attached), and the failure was ordering-
 * dependent — green in isolation, red in the suite.
 */
function assertEchoHasNoConsumer() {
  const cfg = ctl("dump");
  const attached = (cfg.edge || []).filter(
    (e) => e.a === ECHO || e.b === ECHO || String(e.a).startsWith(`${ECHO}/`),
  );
  expect(
    attached,
    `${ECHO} has an edge attached, so discarded_unattached cannot be used as the ` +
      "emitted-bytes oracle — a sibling spec mutated the shared graph without restoring it",
  ).toEqual([]);
}

test("the storage badge reports what the browser actually granted", async ({
  page,
}) => {
  await open(page);
  await selectConsole(page, "m");
  // §15.32: "the client requests navigator.storage.persist() and *shows* whether the
  // browser granted it, because origin storage is evictable and honesty about
  // best-effort persistence beats pretending." Headless Chromium answers `false`, so
  // the badge must read best-effort — never "persisted", and never blank.
  await expect(page.locator("#pane-storage")).toHaveText(
    /^history: (OPFS \((persisted|best-effort)\)|memory only)$/,
  );
  // OPFS is available in this browser, so the memory-only fallback would be a
  // regression in `opfs.mjs`, not an environment difference.
  await expect(page.locator("#pane-storage")).toContainText("OPFS");
});

test("a reload splices stored history against the replay ring exactly once", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const first = token("before");
  await send(page, first);
  await expect(page.locator("#term")).toContainText(first);
  // The debounced OPFS snapshot fires a second after the last chunk; `pagehide` also
  // flushes, but waiting for the badge-visible write keeps the reload honest rather
  // than racing the debounce.
  await page.waitForTimeout(1500);

  await page.reload();
  await expect(page.locator("#conn")).toHaveClass("connected");
  await selectConsole(page, ECHO);

  // The stored scrollback is replayed from OPFS, and then the daemon's ring replays
  // the same bytes over the tap. The offsets (plan §11.8) are what stop the second copy
  // from being appended — this is the splice, asserted in the browser.
  await expect(page.locator("#term")).toContainText("stored history");
  await expect(page.locator("#term")).toContainText(first);
  expect(await countInTerminal(page, first)).toBe(1);

  // And the live stream continues past the splice point rather than being trimmed
  // away with the overlap.
  const second = token("after");
  await send(page, second);
  await expect(page.locator("#term")).toContainText(second);
  expect(await countInTerminal(page, first)).toBe(1);
  expect(await countInTerminal(page, second)).toBe(1);
});

test("clear drops the stored scrollback without breaking the live stream", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const before = token("cleared");
  await send(page, before);
  await expect(page.locator("#term")).toContainText(before);

  page.once("dialog", (d) => d.accept());
  await page.locator("#clearbtn").click();
  await expect(page.locator("#term")).toContainText("history cleared");
  expect(await countInTerminal(page, before)).toBe(0);

  // `clear` keeps the live frontier on purpose (app.js), so the stream that was
  // running does not re-deliver what it already showed. New bytes still arrive.
  const after = token("post-clear");
  await send(page, after);
  await expect(page.locator("#term")).toContainText(after);
});

// The default replay ring is 64 KiB, so any console that emits more than that while its
// tab is looking elsewhere resumes at an offset past the stored frontier — in the *same*
// offset space, so the §15.38 epoch machinery correctly says nothing. The client used to
// splice straight across that hole under a `— replay (N bytes) —` marker that positively
// suggests continuity, while `history.dropped` counted the missing bytes and no code
// anywhere read the counter (review HIST-1).
//
// Looking away is a plain console switch: `selectConsole` flushes to OPFS and closes the
// tap, and a hub with a ring keeps ingesting with no tap open (`refresh_active`), which
// is exactly the state that rotates the ring out from under the stored history.
test("a ring that rotated past the stored history is announced, not spliced over", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const before = token("pre-gap");
  await send(page, before);
  await expect(page.locator("#term")).toContainText(before);
  await page.waitForTimeout(1500); // let the debounced OPFS snapshot land

  // Look away. The outgoing console's flush is fire-and-forget, so give it a moment
  // before the device starts talking again — a snapshot still in flight is not the
  // subject here.
  await selectConsole(page, "m");
  await page.waitForTimeout(500);

  // Overrun the ring while nobody is watching. The browser cannot write to a console it
  // has not selected, so the bytes are pushed the way an operator's script would, over
  // the control socket (§17's `send` is the same verb).
  assertEchoHasNoConsumer();
  const baseline = hostwardBytes();
  const filler = "x".repeat(4000); // under any line-discipline limit, whatever the tty
  for (let i = 0; i < 48; i++) ctl("send", ECHO, "--line", filler);

  // Diagnose the force before judging the UI: if the endpoint never emitted more than
  // its 64 KiB ring, no hole opened and this spec is measuring nothing — a different
  // failure from "the console hid a real hole", and it must not read the same. Twice the
  // ring is the threshold, so the hole survives whatever the lossy mirror hop sheds.
  await expect
    .poll(() => hostwardBytes() - baseline, {
      timeout: 30_000,
      message:
        "the echo console never emitted past its 64 KiB replay ring, so no hole " +
        "opened — the filler did not reach the device (check the `send` calls above)",
    })
    .toBeGreaterThan(128 * 1024);

  await selectConsole(page, ECHO);
  // §5: loss is always visible and attributable. The bytes between the stored frontier
  // and where the stream resumes are gone, and the console says so *before* the replay
  // marker rather than presenting a fabricated adjacency.
  await expect(page.locator("#term")).toContainText(
    "bytes lost (the daemon's ring rotated past this history)",
  );
  // The stored scrollback is still rendered exactly once — announcing the hole must not
  // cost the splice its exactness…
  expect(await countInTerminal(page, before)).toBe(1);
  // …and the console is live, not frozen: the hole is reported, not treated as a reason
  // to reject everything that follows.
  const after = token("post-gap");
  await send(page, after);
  await expect(page.locator("#term")).toContainText(after);
});

// `clear` competes with the debounced snapshot: `window.confirm` blocks the renderer, so
// a save scheduled before the click comes due while the dialog is open and lands at the
// first `await` inside the handler — with the pre-clear buffer still in `history`,
// re-creating the record the operator just destroyed (review HISTC-2). The scenario is a
// shared lab machine and a console that carried credentials, so the assertion has to be
// about storage, not about `#term`.
test("a slow-confirmed clear leaves no stored record behind", async ({ page }) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const secret = token("secret");
  await send(page, secret);
  await expect(page.locator("#term")).toContainText(secret);

  // Prove the record exists first: a "nothing is stored" assertion that passes because
  // nothing was ever stored would be worth nothing. (The traced defect did not even need
  // the record to pre-exist — the overdue debounce created it — but a vacuous assertion
  // would hide a regression in persistence itself.)
  await expect
    .poll(async () => (await recordsContaining(page, secret)).length, {
      timeout: 20_000,
      message: "the scrollback was never persisted, so this spec cannot test its removal",
    })
    .toBeGreaterThan(0);

  // Arm a fresh debounce immediately before the click, then hold the dialog open for
  // longer than the one-second debounce window so the pending save is overdue at the
  // moment the handler resumes.
  const trailing = token("trailing");
  await send(page, trailing);
  await expect(page.locator("#term")).toContainText(trailing);
  page.once("dialog", (d) => setTimeout(() => d.accept(), 2000));
  await page.locator("#clearbtn").click();
  await expect(page.locator("#term")).toContainText("history cleared");

  // Storage, not the terminal: nothing the operator asked to destroy may survive. The
  // older clear spec asserts on `#term`, which the defect never touched — the scrollback
  // came back on the *next* viewer's page load, out of OPFS.
  await expect
    .poll(async () => (await recordsContaining(page, secret)).length, { timeout: 20_000 })
    .toBe(0);
  await expect
    .poll(async () => (await recordsContaining(page, trailing)).length)
    .toBe(0);
});

test("export hands the scrollback to the browser as a download", async ({ page }) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);
  const t = token("export");
  await send(page, t);
  await expect(page.locator("#term")).toContainText(t);

  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.locator("#exportbtn").click(),
  ]);
  // §15.32's export control: the file is named for the console, sanitised.
  expect(download.suggestedFilename()).toBe(`${ECHO}.log`);
  const path = await download.path();
  const { readFileSync } = await import("node:fs");
  expect(readFileSync(path, "utf8")).toContain(t);
});
