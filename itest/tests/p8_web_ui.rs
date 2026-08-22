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
//! **What this gate asserts about the suite itself.** A green Playwright exit says
//! nothing about *how much* ran, so the totals are held against the tool's own
//! enumeration rather than against a number kept here: `--list`, carrying the run's own
//! filters, answers how many specs this lane selects and how many of them are tagged
//! `@device`, and the pass/skip split is asserted equal to those. `SPECS_TOTAL` and
//! `SPECS_SLOW` are the one pair still kept by hand, for the one thing a derived count
//! structurally cannot see — a suite that got smaller.
//!
//! **Three knobs, none of them set by the per-push CI lane.** `SNX_UI_SLOW=1` includes
//! the `@slow` specs and is what `web-ui-nightly` sets; `SNX_UI_GREP` narrows a run while
//! debugging and suspends all of the counting above, so a filtered run can never be
//! mistaken for a full one; `SNX_UI_DEVICE_FREE=1` builds the fixture without its serial
//! devices, which is how the device-free arm becomes reachable on a box that has one.
//!
//! Traces and screenshots for a failing spec land in `web/ui-tests/
//! test-results/`, which the CI job uploads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
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

/// The whole browser suite, in the only number this gate still keeps by hand — and the
/// anchor every other count in it is derived from (review 32 ITEST-4, rebuilt 2026-08-21).
///
/// Every floor here used to be hand-kept, and every one of them carried slack. The repair
/// is plan §3 rule 7 — "CI loops enumerate from the tool, never a hand-kept list" —
/// applied to the gate that was still hand-keeping: before asserting anything, this gate
/// asks Playwright, with the **same filters the run just used**, how many specs that
/// selection holds (`--list`; no browser, no spec executed, under a second), how many of
/// them carry the `@device` tag, and, in the slow lane, how many carry `@slow`. The
/// pass/skip split then falls out as an *equality* instead of a floor, so adding a spec
/// still edits nothing here and a removed one has nowhere to hide.
///
/// **Why one number survives the derivation.** A count taken from the tool cannot see the
/// suite get smaller: delete a spec file and `--list` reports one fewer, the equality
/// holds against the smaller answer, and the gate goes green over exactly the hole it
/// exists to catch — AGENTS §3's tell in its purest form, a passing output identical to a
/// not-running one. So one figure has to stand *outside* the tool's answer. It is written
/// as the whole suite plus the `@slow` share, and each lane's minimum is derived from the
/// pair: `SPECS_TOTAL - SPECS_SLOW` per push, `SPECS_TOTAL` in the slow lane.
///
/// That derivation is half the point. The two lanes previously asserted the *same* floor
/// of 25, and a per-push run reports 28 — so `web-ui-nightly`, a job whose entire reason
/// to exist is the two `@slow` specs, was asserting a number the per-push lane already
/// cleared by three and could not have noticed either slow spec vanishing. The slow lane
/// now also counts `@slow` specs in its own selection, which is a floor the per-push lane
/// cannot reach at all: its `--grep-invert @slow` leaves that count structurally zero.
///
/// **Measured with `--list`** (read-only), this tree, Linux 7.0.0-30, 2026-08-21: 30
/// selected unfiltered · 28 under `--grep-invert @slow` · 2 under `--grep @slow` · 12
/// under `--grep @device` · 11 under `--grep-invert @slow --grep @device`. The forced-shed
/// spec is the one that is both `@slow` and `@device`.
///
/// **And executed**, same box, same session, all four arms — the re-measurement the
/// superseded comment here owed and called blocked on "a Linux rig", which it was not:
/// `serial_echo()` is unconditionally `Some` on Linux (a sim pty stands in for the port),
/// so the *with-device* arm is the one a Mac cannot reach, and the device-free arm is now
/// reachable anywhere through `SNX_UI_DEVICE_FREE=1`:
///
/// | lane | fixture | passed | skipped | wall |
/// |---|---|---|---|---|
/// | per push | with device | 28 | 0 | 28.5 s |
/// | per push | device-free | 17 | 11 | 13.4 s |
/// | `SNX_UI_SLOW=1` | with device | 30 | 0 | 2.6 min |
/// | `SNX_UI_SLOW=1` | device-free | 18 | 12 | 2.2 min |
///
/// Against the floors that stood here — 25 with a device, 14 device-free — that is three,
/// three, five and four specs of slack respectively. All four readings are now equalities
/// the gate derives rather than numbers it carries.
///
/// **The prose those floors rested on contradicted itself, and is replaced rather than
/// patched.** One paragraph said twelve specs carry a device skip; the next said eleven;
/// both then called their own totals "per-push counts". The measured answer is **twelve in
/// the suite, eleven in the per-push selection** — the twelfth is `@slow`, so the per-push
/// lane never sees it. Nothing kept a recount like that true, which is why the count is no
/// longer prose at all: `--grep @device` is the authority and the tag on each spec is what
/// it reads.
const SPECS_TOTAL: usize = 30;

/// How many of [`SPECS_TOTAL`] are tagged `@slow` — excluded per push, run by
/// `web-ui-nightly`. Subtracted from `SPECS_TOTAL` to give the per-push lane its minimum,
/// and asserted directly as the slow lane's own, so the two lanes can never again hold
/// each other to the same number. Adding a `@slow` spec raises the suite and the share
/// together and leaves the per-push minimum where it is, which is the arithmetic this pair
/// exists to keep honest.
const SPECS_SLOW: usize = 2;

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
    // `SNX_UI_DEVICE_FREE=1` builds the *other* fixture on a box that has a device.
    //
    // The two arms below are two different suites — 28 specs against 17 — and until this
    // knob existed neither machine could run both. `serial_echo()` is unconditionally
    // `Some` on Linux, where a `serial-nexus-sim` pty stands in for the port; on macOS a
    // pts is not a serial device at all (`docs/macos.md`), so it is unconditionally
    // `None`. That is precisely why the floors above were *guessed* on whichever arm the
    // author could not reach, and why the superseded comment there owed a re-measurement
    // it described as needing a Linux rig. It never needed a rig — it needed the device-
    // free fixture to be a choice rather than a platform accident. It is one now, and the
    // four rows recorded at `SPECS_TOTAL` were taken through it on a single box.
    //
    // The knob only ever *removes* a device, so it cannot make a device-bearing run look
    // healthier than it is, and it is off unless spelled, so the default on every machine
    // is still whatever the platform actually offers.
    let echo = if std::env::var("SNX_UI_DEVICE_FREE").as_deref() == Ok("1") {
        None
    } else {
        serial_echo()
    };
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
    let slow = std::env::var("SNX_UI_SLOW").as_deref() == Ok("1");

    // The *filters* are kept apart from the verb, because the `--list` enumerations below
    // have to be handed exactly the selection this run executes. A count derived from a
    // different selection than the one that ran is a count about nothing — and it would
    // fail in the reassuring direction, since a broader listing only ever raises the
    // number the run is then held to.
    let mut filters: Vec<&str> = Vec::new();
    if !grep.is_empty() {
        filters.push("--grep");
        filters.push(&grep);
    }
    // `@slow` specs are the project's `#[ignore]` in Playwright's spelling: excluded per
    // push, run in the nightly lane with `SNX_UI_SLOW=1`. Today that is two specs, both in
    // `console.spec.mjs`: the forced tap shed, which costs about a minute of real renderer
    // time, and the unwatched-console tap release, whose subject *is* the grace interval
    // and so spends it twice over — reasons each spec records at its own tag. Excluding by
    // *tag* rather than by name means a new slow spec joins the nightly lane by annotating
    // itself, not by editing this list.
    if !slow && grep.is_empty() {
        filters.push("--grep-invert");
        filters.push("@slow");
    }

    // The fixture description `tests/fixture.mjs` reads at import time, built once and
    // handed to every `npx` this test spawns — the run and the read-only listings alike.
    // A listing that answered about a differently-configured suite would be worse than no
    // listing, because its number looks like a measurement.
    let env: Vec<(&str, OsString)> = vec![
        (
            "SNX_WEB_URL",
            format!("http://127.0.0.1:{port}/?token={TOKEN}").into(),
        ),
        ("SNX_CTL", bin("serial-nexus-ctl").into_os_string()),
        ("SNX_SOCKET", d.socket().into_os_string()),
        ("SNX_RUN", d.run().path().as_os_str().to_owned()),
        ("SNX_REPLACE_CFG", replace_path.clone().into_os_string()),
        ("SNX_FAULT_A", FAULT_A.into()),
        ("SNX_FAULT_B", FAULT_B.into()),
        ("SNX_FAULT_NODE", FAULT_NODE.into()),
        (
            "SNX_ECHO_CONSOLE",
            if echo.is_some() { ECHO_CONSOLE } else { "" }.into(),
        ),
        (
            "SNX_HOSE_CONSOLE",
            if echo.is_some() { HOSE_CONSOLE } else { "" }.into(),
        ),
        ("SNX_HOSE_GO", hose_go.clone().into_os_string()),
        // Playwright writes its browser download cache here; leaving it at the default
        // is what lets a CI cache step hit.
        ("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1".into()),
    ];

    let mut pw_args: Vec<&str> = vec!["playwright", "test"];
    pw_args.extend_from_slice(&filters);
    let out = playwright(&ui, &pw_args, &env);

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
    // "assert execution, not existence", the rule the nightly fuzz loop learned the hard
    // way (§15.36 rule 7, where a stale hand-kept list meant five parsers were compiled
    // every night and fuzzed never).
    //
    // What follows used to be one floor per fixture, both hand-kept and both slack: 25
    // with a device against a run that reports 28, 14 device-free against a run that
    // reports 17. They are now asked of the tool with the run's own filters and asserted
    // as equalities — plan §3 rule 7's "enumerate from the tool" applied to the last gate
    // in this tree that was still keeping a list by hand. `SPECS_TOTAL`/`SPECS_SLOW` stay
    // hand-kept for the one thing a derived count structurally cannot see, and their doc
    // comment says why.
    //
    // `SNX_UI_GREP` narrows the run deliberately, so none of this applies to it; CI never
    // sets it, and `SNX_WEB_UI=required` does not relax anything else.
    let passed = passed_count(&stdout);
    let skipped = skipped_count(&stdout);
    if grep.is_empty() {
        let lane = if slow {
            "the slow lane, SNX_UI_SLOW=1"
        } else {
            "per push"
        };
        let device = if echo.is_some() {
            "with a serial device"
        } else {
            "device-free"
        };

        // What Playwright says this exact argv selects, and how much of that selection is
        // device-gated. Two listings per run, three in the slow lane; measured 4.7–5.8 s
        // each on this box — an `npx` spawn plus the config and spec transpile — and none
        // of them starts a browser or executes a spec. Against a per-push suite that takes
        // 28 s of real browser time and a slow lane that takes 2.6 min, that is the price
        // of not keeping the numbers by hand.
        let selected = listed_specs(&ui, &filters, &env);
        let mut device_filters = filters.clone();
        device_filters.extend_from_slice(&["--grep", "@device"]);
        let device_gated = listed_specs(&ui, &device_filters, &env);

        // The one thing a derived count cannot see: the suite getting smaller. `--list`
        // shrinks with it, so every equality below would still hold — the gate's passing
        // output identical to its not-running output, one directory over. This is the
        // number that stands outside the tool's answer, and it is the reason a deleted
        // file has to be a deliberate edit here rather than a silent one.
        let min_selected = if slow {
            SPECS_TOTAL
        } else {
            SPECS_TOTAL - SPECS_SLOW
        };
        assert!(
            selected >= min_selected,
            "Playwright selects {selected} specs for this lane ({lane}), fewer than the \
             {min_selected} this tree declares ({total} specs, {slow_specs} of them \
             `@slow`) — a deleted file, a rename off `*.spec.mjs`, a `testDir` typo or a \
             filter mistake has shrunk the suite. If a spec is meant to be gone, lower \
             `SPECS_TOTAL` in the commit that removes it; nothing else in this gate can \
             tell the difference between a retirement and a loss.\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            total = SPECS_TOTAL,
            slow_specs = SPECS_SLOW,
        );

        // The nightly lane's entire reason to exist, asserted where it can fail. Both
        // lanes used to hold themselves to the same floor of 25 while a per-push run
        // reports 28, so `web-ui-nightly` cleared its own bar by three specs it had not
        // run and could not have noticed either `@slow` spec disappearing. This count is
        // one the per-push lane cannot satisfy however green it is: its `--grep-invert
        // @slow` leaves the same listing at zero.
        if slow {
            let mut slow_filters = filters.clone();
            slow_filters.extend_from_slice(&["--grep", "@slow"]);
            let slow_selected = listed_specs(&ui, &slow_filters, &env);
            assert!(
                slow_selected >= SPECS_SLOW,
                "the slow lane selected {slow_selected} `@slow` specs, fewer than the \
                 {slow_specs} this suite tags — and running those is the only reason this \
                 lane exists. Either a spec lost its tag (in which case it is now running \
                 per push, at whatever it costs) or it is gone.\n\
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
                slow_specs = SPECS_SLOW,
            );
        }

        // Prove the `@device` matcher is live before deriving anything from it. On a
        // device-bearing fixture — which is every CI run of this job — a tag that stopped
        // matching would leave `device_gated` at 0, and *both* assertions below still
        // hold at 0: `passed == selected - 0` and `skipped == 0`. That is AGENTS §3's tell
        // exactly, so the matcher is asserted rather than assumed. The upper bound rides
        // along because `device_gated` is a subset of the same selection by construction,
        // and the subtraction below trusts that.
        assert!(
            (1..=selected).contains(&device_gated),
            "`--grep @device` selected {device_gated} of this lane's {selected} specs, \
             which cannot be right: this suite tags every device-gated spec `@device`, and \
             this gate derives the device-free spec count and the expected skip count from \
             that tag. Zero means the tag no longer matches — renamed, or a Playwright \
             whose `--grep` stopped reading tags — and on a device-bearing fixture that \
             would go unnoticed, because every assertion below happens to hold at zero \
             too.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );

        // The direction a count of passing specs cannot see: a `test.skip` firing when it
        // should not.
        if echo.is_some() {
            // With a device present *nothing* may skip — every `test.skip(!ECHO, …)` and
            // `test.skip(!HOSE, …)` guard is satisfied — so a non-zero skip count means
            // the fixture handed the browser an empty `SNX_ECHO_CONSOLE`/`SNX_HOSE_CONSOLE`,
            // or a spec skipped itself for a reason nobody asked for. Both look identical
            // to a count of passing specs and both silently retire real coverage.
            assert_eq!(
                skipped, 0,
                "{skipped} browser specs skipped themselves on a fixture that has a \
                 serial device — the device-gated specs (the reload splice, the \
                 `load --replace` re-anchor, the editor round-trip) are the ones that \
                 guard §15.38's defects, and a count of passing specs cannot see them go\n\
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        } else {
            // Device-free, the skips are the point — but *which* specs skipped is not
            // something a total can say, so the two independent statements of that set are
            // held equal instead: the `@device` tag Playwright reads before the run, and
            // the `test.skip(!ECHO, …)` / `test.skip(!HOSE, …)` guard each spec runs. A
            // mismatch means either a device-gated spec landed without its tag, or a spec
            // skipped for a reason nobody declared. This arm is reachable on any platform
            // through `SNX_UI_DEVICE_FREE=1`, which is what makes the cross-check worth
            // stating: before that knob it could only run where the tag was never wrong.
            assert_eq!(
                skipped, device_gated,
                "{skipped} browser specs skipped themselves on a device-free fixture, \
                 against the {device_gated} tagged `@device` in this lane's selection. \
                 These are the same set counted two ways — the tag, and the spec's own \
                 `test.skip(!ECHO, …)` / `test.skip(!HOSE, …)` guard — so one of the two \
                 is wrong about a spec.\n\
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        }

        // The headline, and now an equality in both arms rather than a floor in either.
        // With a device every selected spec runs; device-free, exactly the `@device` ones
        // stand down. Both sides come from the tool, so adding a spec changes nothing here
        // and a removed one has nowhere left to hide.
        let standing_down = if echo.is_some() { 0 } else { device_gated };
        let expected = selected - standing_down;
        assert_eq!(
            passed, expected,
            "the Playwright suite reported {passed} passing specs; this fixture \
             ({device}) is handed the {selected} specs Playwright selects for {lane}, of \
             which {standing_down} stand down as `@device`, so exactly {expected} must \
             pass. A filter, an unasked-for skip, or a deleted file has moved it.\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );
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

/// Run `npx playwright …` in the suite directory with the fixture description
/// `tests/fixture.mjs` reads at import time.
///
/// The real run and every `--list` enumeration go through here, so a listing is always
/// describing the suite the run was handed: same directory, same environment, same `npx`.
/// Splitting those two apart is how a floor ends up being about a selection nobody ran.
fn playwright(ui: &Path, args: &[&str], env: &[(&str, OsString)]) -> std::process::Output {
    let mut cmd = Command::new("npx");
    cmd.args(args).current_dir(ui);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("run `npx {}`: {e}", args.join(" ")))
}

/// How many specs Playwright *selects* for one exact set of filters — the tool's own
/// answer, read out of the `--list` reporter's closing line ("Total: 28 tests in 4
/// files").
///
/// This is plan §3 rule 7's "enumerate from the tool" for a suite that is not `cargo`:
/// `--list` resolves `testDir`, the `*.spec.mjs` pattern, `--grep` and `--grep-invert`
/// through the same code path the run uses, then starts no browser and executes no spec.
/// Measured at roughly a second per call on this tree.
///
/// A missing or unparsable footer **panics** rather than returning 0. Every assertion
/// built on this number is an equality or a floor against it, so a silent zero would
/// relax all of them at once and read as a clean run — the same failure [`count_before`]
/// exists to prevent one parser down.
fn listed_specs(ui: &Path, filters: &[&str], env: &[(&str, OsString)]) -> usize {
    let mut args: Vec<&str> = vec!["playwright", "test", "--list"];
    args.extend_from_slice(filters);
    let out = playwright(ui, &args, env);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    total_specs(&stdout).unwrap_or_else(|| {
        panic!(
            "`npx playwright test --list {}` printed no `Total: N tests` line, so this \
             gate cannot ask the tool what it is about to run — and a gate that cannot \
             read its own enumeration must not pass quietly\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            filters.join(" ")
        )
    })
}

/// The count in Playwright's `--list` footer, "Total: 28 tests in 4 files", or `None`
/// when no such line was printed.
///
/// Read from the **last** matching line, for [`count_before`]'s reason: the listing prints
/// every spec title before its footer, and a spec title is free to contain anything at
/// all, this line included.
fn total_specs(stdout: &str) -> Option<usize> {
    stdout.lines().rev().find_map(|l| {
        l.trim()
            .strip_prefix("Total: ")?
            .split_whitespace()
            .next()?
            .parse::<usize>()
            .ok()
    })
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
