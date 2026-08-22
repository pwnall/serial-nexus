// The epoch re-anchor, in a real browser (design §15.37/§15.38, plan §15.2).
//
// This is the one behaviour on §16.7's list that had no automated coverage at any
// layer: a `load --replace` under an open console rebuilds the endpoint's hub, which
// retires the tap and restarts the endpoint's offset space. The daemon's per-boot
// `instance` nonce does *not* rotate for that — it tracks the process, not the
// graph — so what the client keys on is the per-hub **`epoch`** `tap.open` reports
// (§15.38, AGENTS.md invariant 10): `history.mjs`'s `offsetSpaceChanged(storedEpoch,
// epoch)` decides that the space restarted and `reanchor(h, from_offset)` moves the
// frontier onto the new one, marking the seam instead of splicing across it.
// `tap.closed` is what tells the console its *stream* ended — a detached pane rather
// than a quiet one — and it is asserted below for that, not as the re-anchor trigger.
// Until this spec, nothing exercised any of it against a real stored history.

import { test, expect } from "@playwright/test";
import {
  ECHO,
  REPLACE_CFG,
  countInTerminal,
  ctl,
  open,
  selectConsole,
  send,
  token,
  waitNodeActive,
} from "./fixture.mjs";

test("load --replace under an open console detaches, re-anchors, and does not duplicate", { tag: "@device" }, async ({
  page,
}) => {
  test.skip(!ECHO, "no serial device on this platform (§5): nothing echoes");
  await open(page);
  await selectConsole(page, ECHO);

  const before = token("pre-replace");
  await send(page, before);
  await expect(page.locator("#term")).toContainText(before);
  await page.waitForTimeout(1500); // let the debounced OPFS snapshot land

  // The browser cannot do this, by design: `load` is off the bridge's allowlist
  // (§17/§15.35, invariant 11), so the operator's own path is the control socket. The
  // gate wrote `REPLACE_CFG` from the same fixture it loaded, so the graph comes back
  // identical and every console stays addressable.
  ctl("load", "--replace", REPLACE_CFG);

  // §10's `tap.closed`: without it the stream simply stops, which is
  // indistinguishable from a quiet console — an operator watching a dead pane
  // believing it is live.
  await expect(page.locator("#term")).toContainText("console detached");

  await waitNodeActive(ECHO);

  // Re-selecting re-opens the tap. The endpoint's offsets restarted while `instance`
  // did not, so the stored scrollback would otherwise reject every new chunk as
  // already-seen and the console would freeze forever. The client says so and
  // re-anchors.
  await selectConsole(page, ECHO);
  await expect(page.locator("#term")).toContainText("offsets restarted");

  // The re-anchor must not duplicate what was already stored…
  expect(await countInTerminal(page, before)).toBe(1);
  // …and the console must be alive again, which is the failure this whole path exists
  // to prevent.
  const after = token("post-replace");
  await send(page, after);
  await expect(page.locator("#term")).toContainText(after);
  expect(await countInTerminal(page, before)).toBe(1);
});
