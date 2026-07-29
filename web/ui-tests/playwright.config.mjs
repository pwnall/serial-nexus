// Playwright configuration for the web console's browser-half suite (design §15.37).
//
// This config deliberately owns *no* server lifecycle. The daemon, the sim doubles and
// `serial-nexus-web` are started by the Rust gate `itest/tests/p8_web_ui.rs`, which
// hands this suite a bootstrap URL and a fixture description through the environment
// (see `tests/fixture.mjs`). Keeping the fixture in Rust is what lets the browser suite
// stay a *client* of the same harness every other integration test uses, rather than
// growing a second, drifting way to boot the system.

import { defineConfig, devices } from "@playwright/test";

const url = process.env.SNX_WEB_URL;
if (!url) {
  throw new Error(
    "SNX_WEB_URL is unset — this suite is launched by serial-nexus-itest's p8_web_ui gate, " +
      "which boots the daemon, the sim devices and serial-nexus-web and passes the " +
      "bootstrap URL. Run `cargo test -p serial-nexus-itest --test p8_web_ui` instead of " +
      "`npx playwright test` by hand.",
  );
}

export default defineConfig({
  testDir: "./tests",
  // One shared daemon, one shared graph, one write lock: the specs select consoles and
  // edit the graph, so running them concurrently would have them reconfigure each
  // other. Serial execution is the correct shape here, not a workaround.
  fullyParallel: false,
  workers: 1,
  // No retries, on purpose (§15.36). A retry converts a mechanism into a mystery, and
  // this project's standing rule is that "flaky" is a symptom with a cause behind it.
  // A spec that needs a retry to pass is a spec (or a product) that needs a fix.
  retries: 0,
  forbidOnly: !!process.env.CI,
  // Generous, because one spec deliberately blocks the renderer's main thread for
  // seconds to force the tap boundary to shed (§5). Playwright's auto-waiting means
  // the *other* specs settle far under this.
  timeout: 90_000,
  expect: { timeout: 20_000 },
  reporter: process.env.CI ? [["list"], ["github"]] : [["list"]],
  outputDir: "test-results",
  use: {
    baseURL: new URL(url).origin,
    // Artifacts only when something failed — §15.37's "traces are captured on failure".
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
    actionTimeout: 20_000,
  },
  // Chromium only, matching §15.37: the console is a plain ES-module page with no
  // engine-specific surface, and one pinned browser is the whole concession. The
  // runner already speaks the others if that ever changes.
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
