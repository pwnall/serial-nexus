// Browser-side history: the OPFS adapter and the offset splice, asserted where they
// actually run (design §15.32/§15.37, plan §15.2).
//
// `history.test.mjs` already unit-tests the splice arithmetic under `node --test`, and
// its own doc has said since v10 that "the OPFS adapter itself is browser-only and rides
// the manual checklist". This file is what takes that sentence off the checklist: the
// same arithmetic, driven through a real Origin Private File System, a real reload, and
// a real replay ring.

import { test, expect } from "@playwright/test";
import { ECHO, countInTerminal, open, selectConsole, send, token } from "./fixture.mjs";

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
  // the same bytes over the tap. The offsets (§11.8) are what stop the second copy
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
