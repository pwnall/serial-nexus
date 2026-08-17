#![forbid(unsafe_code)]

//! The browser half of the web console, gated from Rust (design §15.37, plan §15).
//!
//! §16.7's doctrine says any behaviour the sim structurally cannot exercise must either
//! appear on the tiered hardware checklist or be marked *unverified* — and until this
//! gate existed, the console's browser half was on that checklist in full: the OPFS
//! adapter, the `tap.closed` re-anchor, the editor flows, the storage badge. A checklist
//! does not run per push.
//!
//! So this test boots the same fixture every other integration test would — a daemon on
//! a temp socket, `serial-nexus-sim` doubles standing where `/dev/ttyUSB0` will, and
//! `serial-nexus-web` in front of it — and then hands a **pinned Playwright suite**
//! (`web/ui-tests/`) a bootstrap URL and a description of what it is looking
//! at. The browser is the client; the fixture stays in Rust, where every other test's
//! fixture already lives, so there is exactly one way to boot this system.
//!
//! **Why Playwright and not a hand-rolled CDP client** (§15.37): §15.36's worst harness
//! bug was a hand-rolled protocol client that desynchronised under a deadline. A
//! hand-rolled CDP client is that risk again wearing a bigger protocol, and Playwright's
//! auto-waiting is built to kill exactly the deadline-tuned waiting that session spent
//! itself removing.
//!
//! **Self-skip discipline** (§5). Three prerequisites are environmental, and a missing
//! one is a *skip*, printed with the command that would fix it — the same concession
//! `p8_web_history.rs` already makes for `node --test`. But a gate that can skip
//! silently is a gate CI can pass over a hole, which is precisely the failure mode
//! plan §3 rule 7 exists to close: setting **`SNX_WEB_UI=required`** turns every skip
//! into a failure, and the CI job that installs node and Chromium sets it. So the suite
//! is optional on a developer's laptop and mandatory where it is provisioned.
//!
//! Traces and screenshots for a failing spec land in `web/ui-tests/
//! test-results/`, which the CI job uploads.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serial_nexus_itest::{Daemon, Sim, WebServer, bin, serial_echo};

/// The fixed per-session bearer token. Overriding the random default keeps the
/// bootstrap URL deterministic (`--token`, §15.29) — same choice `p8_web.rs` makes.
const TOKEN: &str = "uitesttoken0123456789abcdef";

/// The device-free interior pair the fault specs drive: disconnecting `up ↔ m/raw`
/// leaves `m` with no upstream, and the honest report of that is `waiting` (§15.8).
/// Device-free on purpose, so the graph/editor specs run on every platform.
const FAULT_A: &str = "up";
const FAULT_B: &str = "m/raw";
const FAULT_NODE: &str = "m";
/// The echo console: a serial node over a `serial-nexus-sim pty --echo` device (Linux).
const ECHO_CONSOLE: &str = "usb0";
/// The firehose console, whose burst the browser releases by touching the wait-file.
const HOSE_CONSOLE: &str = "hose";

/// 64 MiB. `p8_tap_drops.rs` measured 8 MiB as marginal against a tap that is *never*
/// read; a browser that reads until the spec stops it needs more headroom, not less.
/// Everything downstream of the mirror — the feed (256 chunks), the per-connection tap
/// queue (128 chunks of 64 KiB) and both socket buffers — fits inside 16 MiB, which is
/// why 16 MiB shed on one run and not on the next.
const HOSE_BYTES: &str = "64MiB";

/// The floor the browser suite must clear, **as a function of the fixture this gate
/// built** (review 32 ITEST-4).
///
/// `web/ui-tests/tests/` declares **30** specs (console 18, history 6, graph-editor 5,
/// lifecycle 1). Exactly **two** are tagged `@slow` and excluded per push
/// (`--grep-invert @slow` below), leaving **28**; eleven of those 28 `test.skip`
/// themselves when the fixture has no serial device (`!ECHO` / `!HOSE`), leaving **17**
/// that run on any platform. (Twelve carry a device skip in all; one of those is `@slow`
/// and is not in the per-push run, so both counts above are per-push counts. Of the three
/// specs added 2026-08-17, §15.65's rail-click pair is device-free and raises both floors
/// by two; §15.67's paint-deferral spec needs a device to have produced a `tap.data` and
/// so raises only `SPECS_WITH_DEVICE`.) (Eleven specs carry a device skip in all, but one of them is `@slow` and
/// is not in the per-push run; both counts below are per-push counts. Recounted against
/// the suite on 2026-08-12: this prose had said 23/21/11, four specs behind the tree,
/// while the 15-passed/10-skipped measurement below — which is what 25 per-push specs
/// with ten device gates reports — had matched all along.)
///
/// One constant used to do both jobs: `MIN_SPECS = 8` sat at the device-free count, which
/// is tight on macOS and carried six specs of slack on `ubuntu-latest` — the only platform
/// the `web-ui` job runs on, where `serial_echo()` is unconditionally `Some`. Any six
/// specs could vanish (a deleted file, a rename off `*.spec.mjs`, a `testDir` typo, a
/// `--grep` mistake) and the gate stayed green, while its own assertion message promised
/// that "a *removed* spec … trips it". The gate already knows which fixture it built, so
/// it can hold the suite to the right number instead of to the smaller of the two.
/// **Both floors sit three specs under the recount above, deliberately.** Measured on
/// the macOS rig on 2026-07-30 (device-free fixture, so `SPECS_DEVICE_FREE` is the arm
/// that ran): the per-push suite reported **15 passed, 10 skipped**, against a floor of
/// 11 — four of slack. Adding the 1009 close-code spec to `console.spec.mjs` accounts
/// for one of those four; the other three predate it. Each floor is raised by exactly
/// the spec added here rather than to the measured figure, because the with-device arm
/// cannot be measured on a Mac — a pts is not a serial device here (`docs/macos.md`), so
/// `serial_echo()` is `None` and the ten device-gated specs never run — and a floor
/// guessed above the truth reddens the Linux lane for a reason that is not the tree's.
/// Re-measure both on a Linux rig and set them
/// to the counts it reports; the comment above describes what they are *for*, and that
/// intent is right even while the constants lag.
const SPECS_WITH_DEVICE: usize = 25;
const SPECS_DEVICE_FREE: usize = 14;

#[test]
fn the_web_console_passes_its_headless_chromium_suite() {
    let required = std::env::var("SNX_WEB_UI").as_deref() == Ok("required");
    let skip = |why: &str, fix: &str| {
        assert!(
            !required,
            "SNX_WEB_UI=required, but {why}. Fix: {fix}\n\
             (This job is expected to run the browser suite; a skip here would be a \
             gate passing over a hole — plan §3 rule 7.)"
        );
        eprintln!("SKIP the_web_console_passes_its_headless_chromium_suite: {why} ({fix})");
    };

    let ui = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/ui-tests"));

    if !tool_answers("node") {
        return skip("node was not found", "install Node.js");
    }
    if !tool_answers("npx") {
        return skip("npx was not found", "install Node.js");
    }
    if !ui
        .join("node_modules/@playwright/test/package.json")
        .exists()
    {
        return skip(
            "the pinned Playwright package is not installed",
            "run `npm ci` in web/ui-tests",
        );
    }

    // ---- the fixture ------------------------------------------------------------
    //
    // One daemon, one graph, several consoles: the interior map pair (every platform)
    // plus, where a pts can stand in for a serial port, an echo console and a firehose
    // console. The suite's serial-dependent specs skip themselves when those are
    // absent, exactly as the Rust serial tests do (§5).
    let d = Daemon::start();
    let rpc = d.rpc();
    let echo = serial_echo();
    let hose_link = d.run().join("hosedev");
    let hose_go = d.run().join("hose.go");
    // The `load --replace` spec re-applies the fixture onto the *same* device. That
    // only works because this session fixed the collision it first exposed: `load`
    // composes teardown-then-load inside one synchronous critical section, and the
    // outgoing serial node's fd (carrying `TIOCEXCL`) outlived the replacement's
    // `open(2)`, so the daemon EBUSY'd against itself — a one-second `faulted` flap on
    // real hardware, during which an accepted `send` was purged rather than written,
    // and a *permanent* fault on a pts, whose master `serial-nexus-sim` holds open so the tty
    // never reaches the last close that clears the flag. `SerialNode::teardown` now
    // releases exclusivity on the way out; `p11_replace_atomicity.rs` is the guard, and
    // it was proved fail-first against the unfixed tree.
    //
    // The firehose device holds its payload until the browser touches the wait-file
    // (plan §3's presence-is-not-readiness primitive), so the shed the spec asserts is
    // attributable to backpressure rather than to a race with tap setup. Forcing it
    // before the browser existed was tried and does not work: with no tap open the hub
    // only appends to a 64 KiB ring, which it does far faster than the producer fills
    // the feed, so nothing sheds. The loss needs a consumer that cannot keep up, which
    // is exactly what the spec arranges.
    //
    // `--hold-ms` keeps the pts open afterwards: a real serial port stays plugged in
    // when it stops transmitting (§7.1), and the node must not flap to `waiting`
    // mid-suite.
    let _hose_sim = echo.as_ref().map(|_| {
        Sim::spawn(
            &[
                "pty",
                "--source",
                "--bytes",
                HOSE_BYTES,
                "--link",
                &hose_link.to_string_lossy(),
                "--wait-file",
                &hose_go.to_string_lossy(),
                "--timeout-ms",
                "600000",
                "--hold-ms",
                "600000",
            ],
            Some(&hose_link),
        )
    });

    let base_cfg = format!(
        r#"
# The device-free half of the fixture: a standalone map with no upstream (`up`) feeding
# another (`m`). Disconnecting the edge between them is the graph page's scripted fault.
[[node]]
type = "map"
name = "{FAULT_A}"
hostward = []
targetward = []
[[node]]
type = "map"
name = "{FAULT_NODE}"
hostward = []
targetward = []
[[edge]]
a = "{FAULT_A}"
b = "{FAULT_B}"
"#
    );
    let mut cfg = base_cfg.clone();
    if let Some(e) = echo.as_ref() {
        // `free-for-all` so the browser's `send` never lands on a lock-refusal dialog:
        // the subject here is the byte path and the terminal, not §6 (which
        // `p4_*`/`p8_web` already own).
        //
        // `hostward_buffer` on the firehose is load-bearing and must not be
        // "simplified" away: the shed this suite asserts has to happen at the *tap*
        // boundary, where `#pane-drops` can see it (§5). At the default depth the
        // serial node would shed first, into `discarded_slow_consumer`, which the
        // console does not render — and the spec would fail for the wrong reason.
        cfg.push_str(&format!(
            r#"
[[node]]
type = "serial"
name = "{ECHO_CONSOLE}"
arbitration = "free-for-all"
device = "{dev}"

[[node]]
type = "serial"
name = "{HOSE_CONSOLE}"
arbitration = "free-for-all"
device = "{hose}"
hostward_buffer = 16384
"#,
            dev = e.device().display(),
            hose = hose_link.display(),
        ));
    }

    rpc.load_toml(&cfg, false)
        .expect("load the ui fixture graph");

    // The graph the lifecycle spec re-applies with `load --replace` — a verb the browser
    // is refused, by design (§17/§15.35), so it reaches the daemon over the control
    // socket the way an operator would. It keeps the console *address* (`usb0`) and
    // its device; the map pair rides along so the graph page keeps its subject, and the
    // spent firehose does not.
    let replace_path = d.run().join("replace.toml");
    let mut replace_cfg = base_cfg.clone();
    if let Some(e) = echo.as_ref() {
        replace_cfg.push_str(&format!(
            r#"
[[node]]
type = "serial"
name = "{ECHO_CONSOLE}"
arbitration = "free-for-all"
device = "{dev}"
"#,
            dev = e.device().display(),
        ));
    }
    std::fs::write(&replace_path, &replace_cfg).expect("write the replace config");

    assert!(
        rpc.wait_status(FAULT_NODE, "active", Duration::from_secs(10)),
        "{FAULT_NODE} not active: {:?}",
        rpc.node(FAULT_NODE)
    );
    if echo.is_some() {
        for n in [ECHO_CONSOLE, HOSE_CONSOLE] {
            assert!(
                rpc.wait_status(n, "active", Duration::from_secs(20)),
                "{n} not active: {:?}",
                rpc.node(n)
            );
        }
    }

    let mut server = WebServer::spawn("127.0.0.1:0", TOKEN, &d.socket(), d.run().path(), &[]);
    let Some(port) = server.port_for("http", Duration::from_secs(15)) else {
        assert!(
            !server.exited(),
            "serial-nexus-web exited before printing its bootstrap URL"
        );
        panic!("serial-nexus-web never printed its bound http URL");
    };

    // ---- hand it to the browser --------------------------------------------------
    // `SNX_UI_GREP` narrows the run to one spec while debugging a failure. It is a
    // developer convenience only: CI sets `SNX_WEB_UI=required` and never sets this, so
    // a filtered run cannot be mistaken for a full one.
    let grep = std::env::var("SNX_UI_GREP").unwrap_or_default();
    let mut pw_args: Vec<&str> = vec!["playwright", "test"];
    if !grep.is_empty() {
        pw_args.push("--grep");
        pw_args.push(&grep);
    }
    // `@slow` specs are the project's `#[ignore]` in Playwright's spelling: excluded per
    // push, run in the nightly lane with `SNX_UI_SLOW=1`. Today that is two specs, both in
    // `console.spec.mjs`: the forced tap shed, which costs about a minute of real renderer
    // time, and the unwatched-console tap release, whose subject *is* the grace interval
    // and so spends it twice over — reasons each spec records at its own tag. Excluding by
    // *tag* rather than by name means a new slow spec joins the nightly lane by annotating
    // itself, not by editing this list.
    if std::env::var("SNX_UI_SLOW").as_deref() != Ok("1") && grep.is_empty() {
        pw_args.push("--grep-invert");
        pw_args.push("@slow");
    }
    let out = Command::new("npx")
        .args(&pw_args)
        .current_dir(&ui)
        .env(
            "SNX_WEB_URL",
            format!("http://127.0.0.1:{port}/?token={TOKEN}"),
        )
        .env("SNX_CTL", bin("serial-nexus-ctl"))
        .env("SNX_SOCKET", d.socket())
        .env("SNX_RUN", d.run().path())
        .env("SNX_REPLACE_CFG", &replace_path)
        .env("SNX_FAULT_A", FAULT_A)
        .env("SNX_FAULT_B", FAULT_B)
        .env("SNX_FAULT_NODE", FAULT_NODE)
        .env(
            "SNX_ECHO_CONSOLE",
            if echo.is_some() { ECHO_CONSOLE } else { "" },
        )
        .env(
            "SNX_HOSE_CONSOLE",
            if echo.is_some() { HOSE_CONSOLE } else { "" },
        )
        .env("SNX_HOSE_GO", &hose_go)
        // Playwright writes its browser download cache here; leaving it at the default
        // is what lets a CI cache step hit.
        .env("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1")
        .output()
        .expect("run npx playwright test");

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() && missing_browser(&stdout, &stderr) {
        // Playwright's own diagnostic, matched rather than guessed at. The package is
        // installed but its pinned browser is not, which is an environment gap, not a
        // product failure.
        return skip(
            "the pinned Chromium build is not installed",
            "run `npx playwright install --with-deps chromium` in web/ui-tests",
        );
    }
    assert!(
        out.status.success(),
        "the headless Chromium suite failed (traces in web/ui-tests/test-results/)\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // A suite that ran zero specs exits 0, so prove it actually executed the specs —
    // "assert execution, not existence", the rule the nightly fuzz loop learned the
    // hard way (§15.36 rule 7, where a stale hand-kept list meant five parsers were
    // compiled every night and fuzzed never). A floor rather than an exact count, so
    // adding a spec does not have to edit this line; a *removed* spec, a filter typo, or
    // a suite that silently self-skipped its way to nothing all trip it — which is only
    // true now that the floor is the count for *this* fixture rather than the smaller of
    // the two (ITEST-4).
    //
    // `SNX_UI_GREP` narrows the run deliberately, so the floor does not apply to it; CI
    // never sets it, and `SNX_WEB_UI=required` does not relax anything else.
    let passed = passed_count(&stdout);
    if grep.is_empty() {
        let floor = if echo.is_some() {
            SPECS_WITH_DEVICE
        } else {
            SPECS_DEVICE_FREE
        };
        assert!(
            passed >= floor,
            "the Playwright suite reported {passed} passing specs, fewer than the {floor} \
             this fixture ({device}) exists to run — a filter, a skip, or a deleted file \
             has shrunk it\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            device = if echo.is_some() {
                "with a serial device"
            } else {
                "device-free"
            },
        );

        // The direction a floor cannot see: a `test.skip` firing when it should not.
        // With a device present *nothing* may skip — every `test.skip(!ECHO, …)` and
        // `test.skip(!HOSE, …)` guard is satisfied — so a non-zero skip count means the
        // fixture handed the browser an empty `SNX_ECHO_CONSOLE`/`SNX_HOSE_CONSOLE`, or
        // a spec skipped itself for a reason nobody asked for. Both look identical to a
        // count of passing specs and both silently retire real coverage.
        if echo.is_some() {
            let skipped = skipped_count(&stdout);
            assert_eq!(
                skipped, 0,
                "{skipped} browser specs skipped themselves on a fixture that has a \
                 serial device — the device-gated specs (the reload splice, the \
                 `load --replace` re-anchor, the editor round-trip) are the ones that \
                 guard §15.38's defects, and a floor cannot see them go\n\
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        }
    }
}

/// The passing-spec count from Playwright's list-reporter totals line ("19 passed
/// (8.6s)"), or 0 when it printed no such line.
fn passed_count(stdout: &str) -> usize {
    count_before(stdout, " passed (")
}

/// The skipped-spec count from Playwright's totals block ("9 skipped"), or 0 when it
/// printed none — which is what a run where every `test.skip` guard was satisfied looks
/// like, and what the device-bearing fixture must produce (ITEST-4).
fn skipped_count(stdout: &str) -> usize {
    count_before(stdout, " skipped")
}

/// The number immediately preceding the **last** occurrence of `marker` that has digits
/// in front of it, or 0 when there is none.
///
/// Last rather than first, and digit-guarded, because the list reporter echoes every
/// spec *title* before it prints the totals block: a spec one day named "…is skipped
/// when…" would otherwise capture the reader and, with no digits before it, silently
/// return 0 — which for the skip assertion is the *passing* answer. A counter that
/// quietly reads zero is the failure this whole gate is about.
fn count_before(stdout: &str, marker: &str) -> usize {
    let mut found = None;
    let mut at = 0usize;
    while let Some(i) = stdout[at..].find(marker) {
        let end = at + i;
        let digits: String = stdout[..end]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(n) = digits.chars().rev().collect::<String>().parse::<usize>() {
            found = Some(n);
        }
        at = end + marker.len();
    }
    found.unwrap_or(0)
}

/// Whether `<tool> --version` runs and exits 0.
fn tool_answers(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Playwright's own "browser not installed" diagnostics. Matched on the tool's words
/// rather than on an exit code, because the exit code is the same as a real failure.
fn missing_browser(stdout: &str, stderr: &str) -> bool {
    const MARKERS: [&str; 3] = [
        "Executable doesn't exist",
        "Please run the following command to download new browsers",
        "npx playwright install",
    ];
    MARKERS
        .iter()
        .any(|m| stdout.contains(m) || stderr.contains(m))
}
