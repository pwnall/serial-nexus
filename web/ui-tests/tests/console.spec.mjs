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
  RUN,
  countInTerminal,
  ctl,
  open,
  railRow,
  selectConsole,
  selectView,
  send,
  token,
} from "./fixture.mjs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

/** How many taps the daemon has open on one endpoint — §17's lazy-tap clause, measured
 * where it is actually true rather than inferred from the page. `-1` when the endpoint
 * is not in `state` at all, so a typo reads differently from a closed tap. */
function tapsOn(endpoint) {
  const found = (ctl("state").endpoints || []).find((e) => e.endpoint === endpoint);
  return found ? found.taps : -1;
}

/** Read a numeric `const NAME = …;` out of the `app.js` the page is actually running.
 *
 * Fetched from the origin rather than restated here on purpose (review HIST-2's guard
 * was worthless for exactly this reason): a bound a spec invents stops describing the
 * product the day someone changes the product, and a window six times the volume the
 * spec produces is a window the *unfixed* client also fits inside. Only `N` and `N * M`
 * forms are accepted, so a constant that grew an expression fails loudly instead of
 * being silently misread. */
async function appConst(page, name) {
  const src = await page.evaluate(async () => (await fetch("/app.js")).text());
  const m = new RegExp(`const ${name} = ([0-9*\\s]+);`).exec(src);
  expect(
    m,
    `app.js no longer declares a plain numeric ${name}; this spec's bound is read from ` +
      `it rather than restated, so it has to be updated with the constant`,
  ).not.toBeNull();
  return m[1].split("*").reduce((n, p) => n * Number(p.trim()), 1);
}

/** Send `line` to `endpoint` over the control socket, C0 bytes intact.
 *
 * Not through the send box, and the reason is worth stating because it is a real
 * constraint on what a browser can express: `<input type="text">`'s value sanitization
 * algorithm strips CR and LF, so `\r` never survives the box no matter how the value is
 * set (`fill()`, `el.value = …`, or typing). The `\r` overwrite is nevertheless something
 * a *device* emits constantly, which is the direction this spec is about — so the bytes
 * are injected on the operator's other transport and the browser's job is to render
 * them. `send` self-acquires the write lock (invariant 8), and the page holds none. */
function sendBytes(endpoint, line) {
  ctl("send", endpoint, "--line", line);
}

/** Add a device-free `map` node to the running graph and return its name. A standalone
 * map is a host-facing endpoint with no upstream, so it appears in the rail as a console
 * and reports `waiting` — which makes it the cheapest honest subject for both the
 * status indicator and the endpoint-removal path, on every platform and without
 * disturbing the fixture the other files depend on. */
function addLoneMap(name) {
  const cfg = join(RUN, `${name}.toml`);
  writeFileSync(
    cfg,
    `[[node]]\ntype = "map"\nname = "${name}"\nhostward = []\ntargetward = []\n`,
  );
  ctl("add-node", cfg);
  return name;
}

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

test("the provenance line names this client and the daemon actually behind it", async ({
  page,
}) => {
  await open(page);
  // Plan §17.3: the operational surfaces speak the family names after §15.40. This one
  // is the page's own answer to "what is serving this?", and the daemon half is read
  // from `info` rather than baked in at build time — so a web console pointed at a
  // daemon of a different generation says so instead of quietly claiming its own.
  const provenance = page.locator("#provenance");
  await expect(provenance).toHaveText(
    /^serial-nexus-web → serial-nexus-daemon \d+\.\d+\.\d+/,
  );
  // …and it is the version the daemon reports, not a constant that happens to match.
  expect(await provenance.textContent()).toContain(ctl("info").daemon_version);
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

// Review HIST-2. Nothing ever removed a node from `#term`, so the rendered scrollback
// grew with everything the console had ever emitted — and the harm was not memory but
// *decay*: the writer forces a synchronous layout of the pane on every chunk, so render
// throughput fell from 34.9 to 14.4 KiB/s over the first 2.9 MB, and a console left
// attached falls further behind its device the longer it runs. The property that fixes
// it is the one asserted here: the DOM holds a bounded window, whatever streamed past.
test("the rendered scrollback stays bounded under a large burst", async ({ page }) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  test.setTimeout(120_000);
  await open(page);
  await selectConsole(page, ECHO);

  // Eight lines of 40 000 characters is 320 KB back through the echo device — past the
  // client's 256 KiB rendered cap, and small enough that no bounded queue on the way
  // sheds, so what the screen holds is a trimming decision rather than a lossy one.
  const filler = "x".repeat(40_000);
  const tail = token("bounded");
  for (let i = 0; i < 7; i++) await send(page, `${i}${filler}`);
  await send(page, `${filler}${tail}`);

  // The tail arriving is what proves the console kept up with the burst rather than
  // simply falling behind it, which would make the size assertion below meaningless.
  await expect(page.locator("#term")).toContainText(tail, { timeout: 90_000 });

  const chars = await page
    .locator("#term")
    .evaluate((el) => (el.textContent || "").length);

  // The bound is the client's own cap, read out of the client (see `appConst`). The
  // version of this assertion that shipped with the fix said `chars < 1_000_000`, which
  // the *unfixed* client also satisfied — it renders everything it receives, and that is
  // ~320 008 characters for this burst — so the guard could not fail. Slack over the cap
  // is `commitNode`'s residue: the pending line (never trimmed, bounded by
  // TERM_LINE_CAP) plus room for the markers `tap.open` writes. It is deliberately far
  // below the 320 kB this spec streams, which is what makes the assertion bite.
  //
  // Measured on this fixture: 259 042 characters against a bound of 278 528 and an
  // unfixed 320 008. Fail-first is arithmetic rather than a run — `app.js` belongs to
  // another agent's change set, so the trimmer was not removed to watch this fail; the
  // three numbers above are what the claim rests on.
  const cap = await appConst(page, "TERM_CHAR_CAP");
  const lineCap = await appConst(page, "TERM_LINE_CAP");
  const bound = cap + 4 * lineCap;
  expect(
    chars,
    `the rendered scrollback is not bounded: ${chars} characters against a ` +
      `TERM_CHAR_CAP of ${cap}`,
  ).toBeLessThanOrEqual(bound);
  // …and it is a *window*, not an empty pane: a trimmer that dropped everything would
  // satisfy the bound above and show the operator nothing. Half the cap, because the
  // burst is comfortably larger than the cap and a correct trimmer leaves it nearly full.
  expect(chars, "the terminal trimmed itself to nothing").toBeGreaterThan(cap / 2);
});

// Review UIR-1's other half. `ansi.mjs`'s parser is covered exhaustively under
// `node --test`, but the writer that turns its operations into DOM — `writeRun`,
// `overwriteRun`, `truncatePending`, `commitPending`, `clearScreen` — lives inside
// `app.js` and can only run in a browser. Before this spec, no test in the tree sent a
// single escape byte through a console (`grep -r $'\x1b' ui-tests/tests` found nothing),
// so the half that decides what the operator actually sees was unexercised — including
// the `\r` overwrite that motivated interpreting ANSI at all.
//
// Three round trips through the echo device, no volume: this belongs in the per-push
// lane, so it costs a second, not a minute.
test("the terminal renders SGR colour, a CR overwrite and an unknown CSI", async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);
  const term = page.locator("#term");
  const tag = token("ansi");

  // 1. SGR → a span wearing `ansi.mjs`'s own class name, with the escape bytes gone from
  //    the text rather than printed as literals.
  sendBytes(ECHO, `${tag}A \x1b[31mRED\x1b[0m PLAIN`);
  await expect(term).toContainText(`${tag}A RED PLAIN`);
  await expect(term.locator("span.a-fg1").last()).toHaveText("RED");
  await expect(term).not.toContainText("[31m");

  // 2. The `\r` overwrite. Eight `A`s, carriage return, three `B`s: a scrollback log that
  //    appended would show `AAAAAAAABBB`, and a progress bar or a `\r`-driven status line
  //    would smear down the pane one copy per update. Sent on its own line so the
  //    overwrite lands on the `A`s and not on the tag.
  sendBytes(ECHO, `${tag}B`);
  sendBytes(ECHO, "AAAAAAAA\rBBB");
  await expect(term).toContainText("BBBAAAAA");
  await expect(term, "the overwritten prefix must be gone, not appended to").not.toContainText(
    "AAAAAAAA",
  );

  // 3. An unknown CSI is consumed, not rendered. `CSI ? 25 l` is what picocom, vim and
  //    every curses program emit constantly; a writer that printed it would fill the
  //    operator's scrollback with line noise.
  sendBytes(ECHO, `${tag}C \x1b[?25lTAIL`);
  await expect(term).toContainText(`${tag}C TAIL`);
  await expect(term).not.toContainText("25l");
});

// Review WEBUI-1 (= WEB-1b), reproduced exactly as the verifier did it: remove the
// selected console out from under the page. `selected` used to stay set with the send
// box live, so the next `send` came back `-32602 … is not a host-facing endpoint with a
// write lock` and the console popped "usb0 is locked by someone. Steal the lock and
// send?" — a holder it did not have, a lock that did not exist, and a retry whose
// identical refusal it then discarded in silence.
test("a console whose endpoint leaves the graph offers no steal and stops accepting input", async ({
  page,
}) => {
  const name = addLoneMap(`sel${Date.now().toString(36)}`);
  // Any dialog at all is the defect: this refusal is not a lock conflict.
  const dialogs = [];
  page.on("dialog", (d) => {
    dialogs.push(d.message());
    d.dismiss();
  });

  await open(page);
  await expect(railRow(page, name)).toBeVisible();
  await selectConsole(page, name);
  await expect(page.locator("#sendline")).toBeEnabled();

  ctl("remove-node", name);

  await expect(railRow(page, name)).toHaveCount(0);
  await expect(page.locator("#term")).toContainText("console detached");
  await expect(page.locator("#sendline")).toBeDisabled();
  await expect(page.locator("#pane-title")).toHaveText("select a console");

  // The mechanism, stated by the daemon itself over the page's own transport: the
  // refusal carries `-32602`, not `AppError::Locked`'s `-32003`. A client that collapsed
  // every error envelope to "it failed" could not tell those apart, which is precisely
  // how one of them came to be reported as the other.
  const refusal = await page.evaluate(async (ep) => {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    await new Promise((res, rej) => {
      ws.onopen = res;
      ws.onerror = rej;
    });
    const answer = new Promise((res) => {
      ws.addEventListener("message", (ev) => {
        const m = JSON.parse(ev.data);
        if (m.id === 7001) res(m);
      });
    });
    ws.send(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 7001,
        method: "send",
        params: { endpoint: ep, line: "x", steal: false },
      }),
    );
    const m = await answer;
    ws.close();
    return m;
  }, name);
  expect(refusal.error.code).toBe(-32602);

  expect(
    dialogs,
    "a removed endpoint is not a lock conflict and must raise no steal prompt",
  ).toEqual([]);
});

// Review DEVR-2. §17 specifies the rail as "display address, node status, lock holder
// and waiter count" and adds that "the layout is the contract"; the status was the one
// it never rendered, so on a lost device every field the rail *did* show was
// byte-identical to before and its only live signal was nothing at all.
test("the rail carries each console's node status and says why", async ({ page }) => {
  const stamp = Date.now().toString(36);
  const up = addLoneMap(`st${stamp}u`);
  const down = addLoneMap(`st${stamp}d`);
  try {
    await open(page);
    const dot = railRow(page, down).locator(".gstatus");
    // A node with no upstream is `waiting`, not `faulted` — §7.1's faulted-and-wait maps
    // absence to `waiting` — and the reason is what turns "quiet" into "gone".
    await expect(dot).toHaveClass(/waiting/);
    await expect(dot).toHaveText("◌");
    await expect(dot).toHaveAttribute("title", /no attached upstream/);

    // Live via `subscribe` (§17): wiring an upstream flips the indicator with no reload
    // and no poll in the page. Same glyph vocabulary as the graph page, so an operator
    // reads one thing in two places.
    ctl("connect", up, `${down}/raw`);
    await expect(dot).toHaveClass(/active/);
    await expect(dot).toHaveText("●");
  } finally {
    ctl("remove-node", down, "--cascade");
    ctl("remove-node", up, "--cascade");
  }
});

// Tagged `@slow` and run in the nightly lane, not per push — the project's `#[ignore]`
// convention, in Playwright's spelling (the gate passes `--grep-invert @slow` unless
// `SNX_UI_SLOW=1`).
//
// It is slow for a reason worth writing down rather than tuning around. Forcing a tap
// shed means making the browser a consumer that cannot keep up; the browser then has to
// render everything that *did* survive before it processes the `state` notification
// carrying the counter, because `tap.data` handlers and `state` handlers share one
// renderer thread. And rendering is the expensive half: the writer sets
// `scrollTop = scrollHeight` per chunk, forcing a synchronous layout of the pane. That
// layout used to get monotonically dearer, because the pane grew without bound — 34.9
// down to 14.4 KiB/s over the first 2.9 MB (review HIST-2). It is now a bounded window,
// which makes the rate roughly flat at 121–144 KiB/s rather than decaying, but 64 MiB
// still does not render in a hurry. Measured from that burst: the daemon's numbers are
// right from its first snapshot and the screen catches up well behind them. Hiding the
// terminal first does not help — the click that would hide it needs the same thread. So
// the observation is correct and it costs real time, which is a nightly test, not a
// per-push one. §5 says as much about the subject anyway: this is a
// control-and-observation tool at serial rates, not a data mover.
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

// Review DEVR-5. §17's Implementation stance says "Taps open lazily per viewed console
// and close on blur after a grace interval"; the client opened lazily and never released
// a tap for any reason short of selecting another console, a daemon-side `tap.closed` or
// the link dropping — so a backgrounded tab, and a tab showing the graph or editor page
// with no console on screen at all, both kept the daemon feeding a console nobody was
// watching.
//
// `@slow` for the plain reason that the grace interval is a minute of real time twice
// over, and the *interval* is the subject: shortening it for the test would assert a
// timer this product does not ship. Playwright's spelling of `#[ignore]`, run nightly.
test(
  "an unwatched console's tap is released after the grace interval and re-opened on return",
  { tag: "@slow" },
  async ({ page }) => {
    test.setTimeout(300_000);
    await open(page);
    // Device-free, so this runs on every platform: `m` is the fixture's interior map.
    await selectConsole(page, "m");
    await expect.poll(() => tapsOn("m"), { timeout: 20_000 }).toBe(1);

    // Backgrounding the tab. `document.hidden` is an accessor on `Document.prototype`,
    // so an own property shadows it — a deterministic visibility change that does not
    // depend on how a headless browser reports tab activation, and it drives the very
    // listener a real background does.
    const setHidden = (hidden) =>
      page.evaluate((h) => {
        Object.defineProperty(document, "hidden", { configurable: true, get: () => h });
        document.dispatchEvent(new Event("visibilitychange"));
      }, hidden);

    await setHidden(true);
    await expect
      .poll(() => tapsOn("m"), { timeout: 120_000, intervals: [2000] })
      .toBe(0);
    // Said out loud, so the ring-gap marker a long absence may produce is attributable.
    await expect(page.locator("#term")).toContainText("not being watched");
    await expect(page.locator("#sendline")).toBeDisabled();

    await setHidden(false);
    // Re-opened through the same `selectConsole` splice path, so §15.38's epoch
    // re-anchor and HIST-1's loss marker still apply to whatever the interval cost.
    await expect.poll(() => tapsOn("m"), { timeout: 30_000 }).toBe(1);
    await expect(page.locator("#sendline")).toBeEnabled();

    // The second reachable case, which fails even the charitable reading of "on blur":
    // the console pane is hidden as a unit by the view switch, so no console is being
    // viewed while its tap keeps streaming.
    await selectView(page, "graph");
    await expect
      .poll(() => tapsOn("m"), { timeout: 120_000, intervals: [2000] })
      .toBe(0);
    await selectView(page, "console");
    await expect.poll(() => tapsOn("m"), { timeout: 30_000 }).toBe(1);
  },
);
