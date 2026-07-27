// The console page, in a real browser (design §15.37, plan §15.2).
//
// What the API-level tests in `p8_web.rs` already prove is that the *bytes* reach the
// browser-facing protocol. What only a browser can prove is that they reach the
// *terminal* — through `atob`, the offset splice, the UTF-8 stream decoder and the DOM
// — and that the send box drives the `send` verb the console's own transport allows.

import { test, expect } from "@playwright/test";
import {
  ECHO,
  HOSE,
  HOSE_GO,
  countInTerminal,
  ctl,
  open,
  railRow,
  selectConsole,
  send,
  token,
} from "./fixture.mjs";
import { writeFileSync } from "node:fs";

test("the rail lists every host-facing endpoint the daemon reports", async ({
  page,
}) => {
  await open(page);
  // §17's left rail is "every host-facing endpoint as a console". The fixture's
  // interior map pair supplies two of them without a serial device, so this assertion
  // holds on every platform.
  await expect(railRow(page, "up")).toBeVisible();
  await expect(railRow(page, "m")).toBeVisible();
  if (ECHO) await expect(railRow(page, ECHO)).toBeVisible();
});

test("a send round-trips through the echo oracle and renders in the terminal", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const t = token("echo");
  await send(page, t);
  // The line goes targetward through the daemon, the sim device echoes it, and it
  // returns hostward through the tap. Seeing it in `#term` exercises the whole chain
  // the API tests can only see half of.
  await expect(page.locator("#term")).toContainText(t);
  // Exactly once: a duplicated splice would show it twice, and this is the cheapest
  // place that property is observable.
  expect(await countInTerminal(page, t)).toBe(1);
});

test("the send box is disabled until a console is selected", async ({ page }) => {
  await open(page);
  await expect(page.locator("#sendline")).toBeDisabled();
  await expect(page.locator("#sendbtn")).toBeDisabled();
  await selectConsole(page, "m");
  await expect(page.locator("#sendline")).toBeEnabled();
});

test("the drop counter is silent on a quiet console", async ({ page }) => {
  await open(page);
  await selectConsole(page, "m");
  // §5's honesty runs both ways: loss is always shown, and a console that lost nothing
  // must not wear a warning. `updateHead` only renders the counter above zero.
  await expect(page.locator("#pane-drops")).toHaveText("");
});

// Tagged `@slow` and run in the nightly lane, not per push — the project's `#[ignore]`
// convention, in Playwright's spelling (the gate passes `--grep-invert @slow` unless
// `SNX_UI_SLOW=1`).
//
// It is slow for a reason worth writing down rather than tuning around. Forcing a tap
// shed means making the browser a consumer that cannot keep up; the browser then has to
// render everything that *did* survive before it processes the `state` notification
// carrying the counter, because `tap.data` handlers and `state` handlers share one
// renderer thread. And rendering is the expensive half: `appendText` sets
// `scrollTop = scrollHeight` per chunk, forcing a synchronous layout of a `<pre>` that
// is growing by megabytes, which measures at roughly 45 KB/s. Measured twice, from a
// 64 MiB burst: the daemon's numbers are right from its first snapshot and the screen
// catches up at t+60 s. Hiding the terminal first does not help — the click that would
// hide it needs the same thread. So the observation is correct and it costs a minute,
// which is a nightly test, not a per-push one. §5 says as much about the subject
// anyway: this is a control-and-observation tool at serial rates, not a data mover.
test("the drop counter surfaces when the tap boundary sheds", { tag: "@slow" }, async ({
  page,
}) => {
  test.skip(!HOSE, "no serial device on this platform (§5): nothing to firehose");
  test.setTimeout(240_000);
  await open(page);
  await selectConsole(page, HOSE);

  // Forcing a shed needs a consumer that is genuinely not consuming, and the ordering
  // has to be *caused*, not hoped for. Releasing the burst from Node and then asking
  // the renderer to block races the two: on the run that exposed this, the whole burst
  // landed in the queues before the busy loop began and nothing shed. So the page
  // releases it itself, through a binding that runs in Node while the renderer is
  // already inside the call — the burst starts, and the very next statement stops the
  // renderer from draining the socket.
  await page.exposeBinding("snxReleaseBurst", () => writeFileSync(HOSE_GO, ""));
  await page.evaluate(async () => {
    await window.snxReleaseBurst();
    const until = Date.now() + 3000;
    while (Date.now() < until) {
      /* deliberately busy: a paused tab, forced */
    }
  });

  // Diagnose the force before judging the UI: if the daemon recorded no loss, the burst
  // never overran the boundary and this spec is measuring nothing — which is a
  // different failure from "the console hid a real drop", and must not read the same.
  await expect
    .poll(
      () => {
        const st = ctl("state");
        return (st.taps || []).reduce(
          (n, t) => n + (t.dropped || 0) + (t.feed_dropped || 0),
          0,
        );
      },
      {
        timeout: 30_000,
        message:
          "the daemon recorded no tap loss at all — the burst did not overrun the " +
          "boundary, so this spec never exercised §5's shed (check the gate's burst " +
          "size against TAP_QUEUE_CAP/TAP_FEED_CAP)",
      },
    )
    .toBeGreaterThan(0);

  // §5: a slow spy costs itself data — and says so. The daemon's own numbers ride in
  // the failure message, because "the console did not show a drop" and "there was no
  // drop to show" are different bugs and must not read the same.
  const snap = ctl("state");
  await expect(
    page.locator("#pane-drops"),
    `daemon taps=${JSON.stringify(snap.taps)}`,
  ).toContainText(/tap bytes dropped/, { timeout: 150_000 });
});
