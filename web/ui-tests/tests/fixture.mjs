// The contract between the Rust gate (`itest/tests/p8_web_ui.rs`) and these
// specs (design §15.37). Everything the browser needs to know about the running system
// arrives through the environment; nothing here starts or configures a daemon.
//
// The one deliberate exception is `ctl()`: lifecycle verbs (`load`, `teardown`,
// `shutdown`) are refused by the web bridge *by design* (§17/§15.35, invariant 11), so a
// spec that needs to reconfigure the daemon mid-stream has to reach it the way an
// operator would — over the control socket, with `serial-nexus-ctl`. That the browser
// cannot do this is the property `graph-editor.spec.mjs` asserts, not a gap.

import { execFileSync } from "node:child_process";
import { expect } from "@playwright/test";

/** The bootstrap URL, token included: `GET /?token=…` 302s and sets the cookie. */
export const WEB_URL = required("SNX_WEB_URL");

/** The echo console — a serial node over a `serial-nexus-sim pty --echo` device. */
export const ECHO = process.env.SNX_ECHO_CONSOLE || "";
/** The firehose console, whose burst is released by touching {@link HOSE_GO}. */
export const HOSE = process.env.SNX_HOSE_CONSOLE || "";
export const HOSE_GO = process.env.SNX_HOSE_GO || "";

/**
 * The device-free interior pair the fault specs use. Disconnecting `FAULT_A ↔ FAULT_B`
 * is a real fault an operator can cause, and the honest report of it is `waiting`
 * (§15.8) — the same mechanism `p8_web.rs` asserts at the API level, driven here
 * through the editor page instead.
 */
export const FAULT_A = required("SNX_FAULT_A");
export const FAULT_B = required("SNX_FAULT_B");
export const FAULT_NODE = required("SNX_FAULT_NODE");

/** A scratch directory both sides may write into (log nodes, downloads). */
export const RUN = required("SNX_RUN");
/** The fixture graph, written by the gate, for `load --replace` to re-apply. */
export const REPLACE_CFG = required("SNX_REPLACE_CFG");

const CTL = required("SNX_CTL");
const SOCKET = required("SNX_SOCKET");

function required(name) {
  const v = process.env[name];
  if (!v) {
    throw new Error(
      `${name} is unset — the p8_web_ui gate sets it; do not run this suite directly.`,
    );
  }
  return v;
}

/** Run `serial-nexus-ctl --json <args…>` against the gate's daemon and return the result. */
export function ctl(...args) {
  const out = execFileSync(CTL, ["--socket", SOCKET, "--json", ...args], {
    encoding: "utf8",
  });
  return out.trim() ? JSON.parse(out) : null;
}

/**
 * Block until `node` reports `active`, polling the daemon directly. Used after a
 * `load --replace`, where the graph is rebuilt from scratch and a `send` issued into
 * the gap would race the node's own reconnect rather than test anything.
 */
export async function waitNodeActive(node, timeoutMs = 20_000) {
  const until = Date.now() + timeoutMs;
  let last = null;
  for (;;) {
    const st = ctl("state");
    last = (st.nodes || []).find((x) => x.name === node) ?? null;
    if (last && last.status === "active") return;
    if (Date.now() > until) {
      throw new Error(`${node} never came back active: ${JSON.stringify(last)}`);
    }
    await new Promise((r) => setTimeout(r, 100));
  }
}

/**
 * Open the console with a fresh session: the bootstrap URL sets the `SameSite=Strict`
 * cookie and 302s to `/`, after which the page opens its WebSocket. Waiting on the
 * connection badge — rather than on a timeout — is what keeps these specs free of the
 * deadline tuning §15.36 spent a session removing.
 */
export async function open(page) {
  await page.goto(WEB_URL);
  await expect(page.locator("#conn")).toHaveClass("connected");
  // The rail is painted from the first `state` reply; every spec needs it.
  await expect(page.locator("#consoles li").first()).toBeVisible();
  return page;
}

/** The rail row for one console, matched on its exact display address. */
export function railRow(page, name) {
  return page.locator(`#consoles li:has(.cname:text-is("${name}"))`);
}

/** Select a console and wait for the pane to adopt it. */
export async function selectConsole(page, name) {
  await railRow(page, name).click();
  await expect(page.locator("#pane-title")).toHaveText(name);
  // `tap.open` answers with a replay marker or the no-history notice; either way the
  // terminal has been written to once the tap is live, which is when a `send` can
  // round-trip. Waiting on the marker beats waiting on a duration.
  await expect(page.locator("#term .marker").first()).toBeVisible();
}

/** Switch to one of the three views (§17: one shell, three views, hash-routed). */
export async function selectView(page, view) {
  await page.locator(`.viewbtn[data-view="${view}"]`).click();
  await expect(page.locator(`.viewbtn[data-view="${view}"]`)).toHaveClass(
    /active/,
  );
}

/** Type a line into the send box and submit it (§17's bottom-right input). */
export async function send(page, line) {
  await page.locator("#sendline").fill(line);
  await page.locator("#sendbtn").click();
}

/** A token unique to one spec run, so an assertion counting it cannot collide. */
export function token(tag) {
  return `SNX-${tag}-${Math.random().toString(36).slice(2, 10)}`;
}

/** How many times `needle` occurs in the terminal's text — the splice oracle. */
export async function countInTerminal(page, needle) {
  return page.locator("#term").evaluate((el, n) => {
    const text = el.textContent || "";
    let i = 0;
    let at = 0;
    for (;;) {
      const found = text.indexOf(n, at);
      if (found < 0) return i;
      i += 1;
      at = found + n.length;
    }
  }, needle);
}
