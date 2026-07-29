//! Review 32, item 1 — "give exclusivity an owner", end to end (design §7.1, §15.38
//! D2/D3, §11 invariant 13). One file per defect area, the `p9_*` convention; `p12_*`
//! is this review's family.
//!
//! Finding IDs pinned here:
//!
//! * **`RV-8`** — `open_port` takes `TIOCEXCL` and *then* applies the configured modem
//!   lines. On a pty-backed device (socat `PTY,link=`, QEMU `-serial pty`, this
//!   project's own `serial-nexus-sim pty` — all legal `raw:` devices per §7.1/§12) `set_dtr`
//!   `ENOTTY`s, and the port used to be dropped still exclusive. The flag lives on the
//!   *tty* and clears only at its last close, which a held master never reaches, so an
//!   ordinary `[node.modem] dtr = true` bricked the device for every unprivileged
//!   process on the machine — permanently, surviving `teardown` and daemon exit — and
//!   the node's reported reason flipped from the true cause to a self-inflicted
//!   "Device or resource busy", sending the operator hunting a squatter that does not
//!   exist. No test in the tree configured `[node.modem]` at all, so this open path had
//!   zero coverage.
//! * **`CONC-4` / `SERX-1`** — the v13 release lived only in `SerialNode::teardown`,
//!   one of several places the supervisor lets a port go. Exclusivity is now owned by
//!   the port (`ExclusivePort`), so every path that discards one returns the claim.
//! * **`CTRL-1` / `SERX-2`** — `send-break`/`pulse-dtr` held an `Rc<SerialPort>` clone
//!   across a sleep of caller-supplied `ms` with no upper bound and no re-check that
//!   the node still existed. Reproduced on real hardware: `pulse-dtr --ms 12000` in
//!   flight, `load --replace` onto a successor node on the same device, and at
//!   t=12.1 s DTR flipped on the *successor's* line with nobody asking. The break half
//!   needs no race at all — break is tty state, so the successor came up transmitting
//!   under an asserted break. Both halves are fixed: `ms` is range-checked
//!   structurally before any line is asserted, and the verb is node-scoped, so the fd
//!   goes with the node rather than with the caller's duration.
//!
//! The `ms` range check needs no device and runs on every platform; the two
//! exclusivity checks drive a real pts and **self-skip off Linux** (a pts cannot be a
//! serial device on macOS — `serial2` → `ENOTTY`, AGENTS §7), as does the fd census,
//! which reads `/proc/<pid>/fd`.
//!
//! **One clause here needs real hardware and says so.** A pts has no `break_ctl`:
//! `TIOCSBRK`/`TIOCCBRK` succeed on it and change nothing, so no pts-backed test can
//! observe what state a break leaves a line in — which is exactly the half of SERX-2 a
//! remediation regressed past a green suite. The line-state assertion therefore lives in
//! [`a_break_straddled_by_a_replace_leaves_the_line_transmitting`], which needs a
//! cross-wired null-modem rig (`crossover_ports()`) and self-skips without one.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Rpc, TempRun, bin, crossover_ports, daemon_answers, serial_echo, wait_until,
};

/// The daemon's documented signal-verb ceiling (`Daemon::MAX_SIGNAL_MS`). Duplicated
/// here on purpose: a guard that imported the constant would still pass if the cap
/// were quietly widened to an hour, which is the change this pins against.
const MAX_SIGNAL_MS: u64 = 60_000;

// ---------------------------------------------------------------------------
// A daemon this test owns the `Child` of, so it can census the daemon's open fds.
// `serial_nexus_itest::Daemon` deliberately does not expose its pid, and `src/lib.rs` is
// shared with every other suite — so this is local, the `p7_*` "manage your own
// Child" pattern, and it cleans up on `Drop` like the shared one does.
// ---------------------------------------------------------------------------

struct OwnDaemon {
    child: Child,
    rpc: Rpc,
    run: TempRun,
}

impl OwnDaemon {
    fn start() -> OwnDaemon {
        let run = TempRun::new();
        let socket = run.socket();
        let child = Command::new(bin("serial-nexus-daemon"))
            .arg("--socket")
            .arg(&socket)
            .arg("--state-file")
            .arg(run.state_file())
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn serial-nexus-daemon");
        assert!(
            wait_until(Duration::from_secs(10), || socket.exists()
                && daemon_answers(&socket)),
            "daemon never answered `info` on {}",
            socket.display()
        );
        let rpc = Rpc::new(socket);
        OwnDaemon { child, rpc, run }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for OwnDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// How many fds the daemon holds on `device` right now, read from `/proc/<pid>/fd`.
/// The device is a symlink to the pts, and `/proc` reports the resolved target, so
/// both sides are canonicalized before comparing.
fn daemon_fds_on(pid: u32, device: &Path) -> usize {
    let target = std::fs::canonicalize(device).expect("resolve the device symlink");
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| std::fs::read_link(e.path()).is_ok_and(|l| l == target))
        .count()
}

/// A third party's plain `open(2)` of the device — what picocom, `cu`, or the daemon's
/// own reconnect poll does. `false` means somebody still holds `TIOCEXCL` on the tty.
/// `O_NOCTTY` for the same reason the daemon uses it (§7.1): a test must not acquire a
/// controlling terminal as a side effect of asking whether a device is free.
fn third_party_can_open(device: &Path) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(device)
        .is_ok()
}

/// Read one NDJSON response line from a control-socket stream. The daemon keeps the
/// connection open for further requests, so reading to EOF would block forever.
fn read_response_line(stream: &mut UnixStream) -> Value {
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("set read timeout");
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => buf.push(byte[0]),
            Err(e) => panic!("read control-socket response: {e}"),
        }
    }
    serde_json::from_slice(&buf).unwrap_or_else(|e| {
        panic!(
            "parse control-socket response: {e}; raw={:?}",
            String::from_utf8_lossy(&buf)
        )
    })
}

// ---- RV-8: a modem-line config must not brick a pty-backed device ---------------

/// `RV-8` (with `CONC-4`/`SERX-1`): a serial node configured with `[node.modem]
/// dtr = true` over a pty-backed device faults with the **true** cause and leaves the
/// device openable by everybody else. Before the fix the failed open dropped the port
/// still exclusive, the 1 s reconnect poll then EBUSY'd against the claim it had left
/// behind — masking the real reason forever — and the pts stayed un-openable for the
/// life of the process holding its master.
#[test]
fn a_modem_line_failure_leaves_the_device_openable_and_the_true_cause_reported() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_modem_line_failure_leaves_the_device_openable_and_the_true_cause_reported: \
             no serial device on this platform"
        );
        return;
    };
    let d = OwnDaemon::start();
    let rpc = &d.rpc;

    // The exact shape §7.1 documents and nothing in the tree covered: an initial
    // modem-line assertion. A pts has no TIOCMSET, so `set_dtr` ENOTTYs *after*
    // `open_port` has taken TIOCEXCL — the whole defect in one attribute.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"

[node.modem]
dtr = true
"#,
        dev = echo.device().display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("an environmental failure faults the node, it never fails the load (§15.8)");

    assert!(
        rpc.wait_status("usb0", "faulted", Duration::from_secs(10)),
        "usb0 did not fault on the modem-line failure: {:?}",
        rpc.node("usb0")
    );

    // Give the reconnect poll (1 s) several rounds. With the claim leaked, the reason
    // flips to the daemon's own EBUSY within the first round and never comes back.
    let masked = wait_until(Duration::from_secs(4), || {
        rpc.node("usb0")
            .and_then(|n| n["reason"].as_str().map(str::to_owned))
            .is_some_and(|r| r.contains("busy"))
    });
    assert!(
        !masked,
        "the node masked its own cause with a self-inflicted EBUSY: {:?}",
        rpc.node("usb0").map(|n| n["reason"].clone())
    );
    let reason = rpc
        .node("usb0")
        .and_then(|n| n["reason"].as_str().map(str::to_owned))
        .unwrap_or_default();
    assert!(
        reason.contains("set DTR"),
        "the reported reason is not the modem-line failure: {reason:?}"
    );

    // And the device is not bricked: anyone can still open it. Polled rather than
    // sampled once, because the reconnect poll legitimately holds the claim for the
    // few microseconds between its own `open` and its `set_dtr` failure.
    assert!(
        wait_until(Duration::from_secs(5), || third_party_can_open(
            echo.device()
        )),
        "the failed open left TIOCEXCL on the tty — the device is bricked for every \
         unprivileged process on the machine, permanently"
    );

    // Teardown does not resurrect it either: the claim was never held to begin with.
    rpc.teardown();
    assert!(
        third_party_can_open(echo.device()),
        "the device is still un-openable after teardown"
    );
}

// ---- CTRL-1/SERX-2: a signal verb must not outlive its node ---------------------

/// `CTRL-1` / `SERX-2`: with a `send-break` in flight, `remove-node` must take the
/// device's fd — and the asserted break with it — immediately. Before the fix the
/// verb's `Rc<SerialPort>` clone kept the fd open for the whole caller-supplied `ms`,
/// so `state` and `ports` reported the device free while the daemon still held it with
/// a break asserted, and the deferred restore later drove a line a successor node
/// owned.
///
/// The verb's own outcome is asserted too, and that is what keeps this honest: only a
/// break that was genuinely in flight can come back "node was removed while
/// signalling", so a lost race fails the test rather than passing it vacuously.
///
/// **What this test measures, and what it cannot** (§15.36 — stated rather than
/// implied). It measures fds and the verb's outcome, which is one abstraction level
/// below the finding: the finding is about the *line*. It cannot close that gap on the
/// device it runs on — `serial_echo()` is a pts, a pts has no `break_ctl`, and
/// `TIOCSBRK`/`TIOCCBRK` therefore succeed while changing nothing observable, so an
/// assertion about break state here would be green whatever the daemon did. That is not
/// a hypothetical weakness: the first remediation of SERX-2 made the deferred restore
/// generation-scoped, removed the only `TIOCCBRK` in the tree, and shipped a
/// *permanently* stuck break on real UARTs straight past this file.
/// [`a_break_straddled_by_a_replace_leaves_the_line_transmitting`] is the clause that
/// covers it, on the rig where the question can actually be asked.
#[test]
fn remove_node_during_a_send_break_takes_the_device_fd_with_it() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP remove_node_during_a_send_break_takes_the_device_fd_with_it: \
             no serial device on this platform"
        );
        return;
    };
    let d = OwnDaemon::start();
    let rpc = &d.rpc;
    let device = echo.device().to_owned();

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
"#,
        dev = device.display(),
    );
    rpc.load_toml(&cfg, false).expect("load the serial node");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    // Detector proof: the daemon really does hold exactly one fd on this device, so
    // "zero afterwards" below is a measurement and not a permanently-true statement.
    assert_eq!(
        daemon_fds_on(d.pid(), &device),
        1,
        "the daemon does not hold the device fd — the census is measuring nothing"
    );

    // A long break on its own connection. The raw stream is deliberate: the thread
    // signals once the *request bytes are written*, so the daemon — which accepts
    // connections in order on one runtime thread — dispatches the break before the
    // `remove-node` connection this test opens afterwards.
    let socket = d.run.socket();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Value>();
    let breaker = std::thread::spawn(move || {
        let mut s = UnixStream::connect(&socket).expect("connect for send-break");
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "send-break",
            // Just under the cap: long enough that a leaked fd is unmistakable,
            // short enough that a broken build cannot wedge the suite.
            "params": { "node": "usb0", "ms": 30_000 },
        });
        s.write_all(format!("{req}\n").as_bytes())
            .expect("write send-break");
        s.flush().expect("flush send-break");
        started_tx.send(()).expect("signal in-flight");
        done_tx
            .send(read_response_line(&mut s))
            .expect("report send-break outcome");
    });
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("send-break request never went out");
    // Three control-plane round trips: each completes a full request cycle on the
    // daemon's single runtime thread, so by the third the parked break is certainly
    // dispatched. (`send-break` releases the thread at its `.await`, §15.20.)
    for _ in 0..3 {
        let _ = rpc.state();
    }

    rpc.remove_node("usb0", true).expect("remove-node usb0");
    assert!(rpc.node("usb0").is_none(), "usb0 still in state");

    // The fd goes with the node. It is polled, not sampled: the last `Rc` clone lives
    // in the aborted supervisor's future, which the `LocalSet` drops once the runtime
    // thread comes back — right after the verb returns, but not synchronously with it.
    assert!(
        wait_until(Duration::from_secs(3), || daemon_fds_on(d.pid(), &device)
            == 0),
        "the daemon still holds {} fd(s) on the device after remove-node — with a break \
         asserted on it, and a deferred restore still to come",
        daemon_fds_on(d.pid(), &device)
    );

    // The break was genuinely in flight, and the verb declined instead of driving on.
    let resp = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("send-break never answered: it slept out its `ms` past its node");
    let message = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("node was removed while signalling"),
        "send-break did not decline as a node-scoped signal (so the race was lost and \
         this run proved nothing): {resp}"
    );
    breaker.join().expect("join the send-break thread");

    // And the device is left usable, exclusivity returned with the port.
    assert!(
        wait_until(Duration::from_secs(5), || third_party_can_open(&device)),
        "remove-node left TIOCEXCL on the device"
    );
}

// ---- SERX-2, the line-state half: real UART only --------------------------------

/// `SERX-2` (the clause the first remediation regressed): a `send-break` straddled by
/// `load --replace` on the same device must leave the line **transmitting**.
///
/// Break is *tty* state, exactly as `TIOCEXCL` is (invariant 15): it outlives the fd
/// that asserted it whenever the tty outlives that fd, and under `--replace` the tty
/// never reaches last close, so nothing clears it. Scoping the deferred `RestoreGuard`
/// to the port it was asserted on is right — a departed node must not drive a
/// successor's line — but on its own it removed the only `TIOCCBRK` in the tree and
/// turned an `ms`-bounded outage into an unbounded one: the successor reports `active`
/// and `open: true`, `send` reports the bytes accepted, `driver_counters.tx` climbs, and
/// the peer receives nothing at all, indefinitely, with no counter or status attributing
/// it. Recovery needed a full `teardown` (which destroys the tty) or another
/// `send-break`. The remedy is the same as exclusivity's: the assertion is a claim the
/// node made, so it goes back in the *ordered discard* (`release_port` →
/// `ExclusivePort::release_claims`) before the port is handed on.
///
/// Reproduced and pinned on the bench crossover rig. Fail-first is direct: against the
/// pre-fix binary the `AFTER-REPLACE` line never arrives and this times out; against the
/// fixed one it arrives within a second of the replace, with ~19 s of the break's `ms`
/// still nominally to run.
///
/// Self-skips without a rig (a skip is a valid verdict, §5) — set `SNX_CROSSOVER_A` and
/// `SNX_CROSSOVER_B` to two cross-wired adapters (plain `/dev` paths: the rig suite
/// compares them against `resolved_path`), or attach `cu.usbserial-*` pairs on macOS. It
/// is the only test in this file that needs one.
///
/// **Why not `serial_pair()`**, which needs no hardware and would run in CI: the Linux
/// sim null modem is a byte-copy loop between two ptys, and neither a pts nor that loop
/// models a break condition at all. The test would be green against the stuck-break
/// binary — the "cannot fail" shape this whole exercise is about. A rig-gated guard that
/// really fails is worth more than a portable one that cannot.
#[test]
fn a_break_straddled_by_a_replace_leaves_the_line_transmitting() {
    let Some((port_a, port_b)) = crossover_ports() else {
        eprintln!(
            "SKIP a_break_straddled_by_a_replace_leaves_the_line_transmitting: no crossover \
             rig (set SNX_CROSSOVER_A/_B to two cross-wired adapters)"
        );
        return;
    };
    // Deliberately far longer than anything this test waits for: if the fix regresses to
    // "the break clears when `ms` elapses", the assertions below must still fail.
    const BREAK_MS: u64 = 20_000;

    let d = Daemon::start();
    let rpc = d.rpc();
    let rx_log = d.run().join("rx.log");
    // `usb0` drives the wire; `peer` reads the far end into a write-never log, which is
    // the lossless sink AGENTS §6 says to ground a byte claim on.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{port_a}"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "peer"
device = "{port_b}"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "log"
name = "rx"
directory = "{dir}"
filename = "rx.log"
[[edge]]
a = "peer"
b = "rx"
write_mode = "never"
"#,
        dir = d.run().path().display(),
    );
    rpc.load_toml(&cfg, false).expect("load the rig graph");
    for node in ["usb0", "peer"] {
        assert!(
            rpc.wait_status(node, "active", Duration::from_secs(20)),
            "{node} not active: {:?}",
            rpc.node(node)
        );
    }

    // Detector proof: the wire really carries bytes, so "nothing arrived" below is a
    // measurement about the break and not about a rig that was never wired.
    rpc.call(
        "send",
        json!({ "endpoint": "usb0", "line": "BEFORE-BREAK" }),
    )
    .expect("send BEFORE-BREAK");
    assert!(
        wait_until(Duration::from_secs(10), || log_contains(
            &rx_log,
            "BEFORE-BREAK"
        )),
        "the crossover rig is not carrying bytes; this test would prove nothing"
    );

    // A long break on its own connection, dispatched before the replace (same ordering
    // discipline as the fd-census test above).
    let socket = d.run().socket();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Value>();
    let breaker = std::thread::spawn(move || {
        let mut s = UnixStream::connect(&socket).expect("connect for send-break");
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "send-break",
            "params": { "node": "usb0", "ms": BREAK_MS },
        });
        s.write_all(format!("{req}\n").as_bytes())
            .expect("write send-break");
        s.flush().expect("flush send-break");
        started_tx.send(()).expect("signal in-flight");
        done_tx
            .send(read_response_line(&mut s))
            .expect("report send-break outcome");
    });
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("send-break request never went out");
    for _ in 0..3 {
        let _ = rpc.state();
    }

    // The straddle: teardown-then-load onto the same nodes and the same devices, with
    // the break still nominally 19 s from expiring.
    rpc.load_toml(&cfg, true).expect("load --replace the graph");
    for node in ["usb0", "peer"] {
        assert!(
            rpc.wait_status(node, "active", Duration::from_secs(20)),
            "{node} not active after --replace: {:?}",
            rpc.node(node)
        );
    }

    // The break was genuinely in flight and the verb declined (as it must — the port it
    // asserted on is gone). Without this the run could pass having tested nothing.
    let resp = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("send-break never answered");
    let message = resp["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("node was removed while signalling"),
        "send-break did not decline as a node-scoped signal, so the straddle never \
         happened and this run proved nothing: {resp}"
    );
    breaker.join().expect("join the send-break thread");

    // The whole point: the successor's bytes reach the peer. `send` reporting them
    // accepted is *not* the property — that reported success throughout the defect.
    rpc.call(
        "send",
        json!({ "endpoint": "usb0", "line": "AFTER-REPLACE" }),
    )
    .expect("send AFTER-REPLACE");
    assert!(
        wait_until(Duration::from_secs(10), || log_contains(
            &rx_log,
            "AFTER-REPLACE"
        )),
        "the successor transmits nothing: the outgoing node left the tty in break and \
         the ordered discard did not clear it. `send` reported the bytes accepted and \
         the node reports {:?}",
        rpc.node("usb0")
    );
}

/// Whether the capture log currently contains `needle`. The break's own arrival at the
/// peer is a NUL plus whatever framing noise the UART made of it, so this is a substring
/// search rather than an equality: the claim is "the line transmits again", and the
/// bytes around it are the physical evidence that it had stopped.
fn log_contains(path: &Path, needle: &str) -> bool {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).contains(needle))
        .unwrap_or(false)
}

// ---- CTRL-1 half 1: `ms` is range-checked structurally -------------------------

/// `CTRL-1`: `ms` is the one input that makes a signal verb's window arbitrarily wide,
/// and it was taken straight from the params with no bound (`--ms 12000` is what
/// reproduced the hardware defect; `--ms 86400000` was accepted). It is now
/// range-checked **before** the port is resolved and before any line is asserted —
/// invariant 13's structural-refusal shape, applied at the verb because `ms` is an RPC
/// param rather than a configuration field.
///
/// Device-free by construction, so it runs on every platform: an absent device gives a
/// `waiting` serial node with no open port, and the *contrast* between the two errors
/// is the proof of ordering — over-range is refused by name, in-range gets as far as
/// the port.
#[test]
fn an_out_of_range_signal_ms_is_refused_by_name_before_the_port_is_touched() {
    let d = OwnDaemon::start();
    let rpc = &d.rpc;
    let absent = d.run.join("absent-usb0");

    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
"#,
        dev = absent.display(),
    );
    rpc.load_toml(&cfg, false)
        .expect("load the absent-device node");
    assert!(
        rpc.wait_status("usb0", "waiting", Duration::from_secs(10)),
        "usb0 not waiting: {:?}",
        rpc.node("usb0")
    );

    for (verb, params) in [
        (
            "send-break",
            json!({ "node": "usb0", "ms": MAX_SIGNAL_MS + 1 }),
        ),
        (
            "pulse-dtr",
            json!({ "node": "usb0", "ms": MAX_SIGNAL_MS + 1, "assert": true }),
        ),
    ] {
        let err = rpc
            .call(verb, params)
            .expect_err("an over-range `ms` must be refused");
        assert_eq!(err.code, -32602, "{verb}: wrong error code: {err:?}");
        assert!(
            err.message.contains("ms = ")
                && err
                    .message
                    .contains(&format!("above the maximum {MAX_SIGNAL_MS}")),
            "{verb}: the refusal does not name the field and its bound: {}",
            err.message
        );
        // Refused *before* the port is looked at: the node has none, and that is not
        // what the operator was told.
        assert!(
            !err.message.contains("no open port"),
            "{verb}: `ms` was checked after the port was resolved: {}",
            err.message
        );
    }

    // The contrast: at the cap the range check passes and the verb proceeds to the
    // port — which this node does not have. Same node, same call, different reason.
    for (verb, params) in [
        ("send-break", json!({ "node": "usb0", "ms": MAX_SIGNAL_MS })),
        (
            "pulse-dtr",
            json!({ "node": "usb0", "ms": MAX_SIGNAL_MS, "assert": true }),
        ),
    ] {
        let err = rpc
            .call(verb, params)
            .expect_err("a waiting node has no port to signal");
        assert!(
            err.message.contains("no open port"),
            "{verb}: an in-range `ms` was refused as out of range: {}",
            err.message
        );
    }
}
