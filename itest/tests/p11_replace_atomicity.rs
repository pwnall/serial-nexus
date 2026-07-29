//! `load --replace` must not fault the serial port it is keeping (design §11, §7.1).
//!
//! Found by the v13 browser-UI track (plan §15): the Playwright lifecycle spec could not
//! get its console back after a `load --replace`, and the reason was not the browser.
//!
//! **The mechanism.** `Daemon::load` is one synchronous critical section (§15.20): it
//! runs the teardown and then instantiates the new nodes without ever yielding the
//! runtime thread. `SerialNode::signal_stop` only `abort()`s the supervisor task, and
//! that task's future still holds an `Rc<SerialPort>` clone across its `.await` — so the
//! outgoing node's fd is not released until the `LocalSet` reaps the aborted task, which
//! cannot happen until `load` returns. The replacement node's `open(2)` therefore lands
//! while the old fd is still open, and the old fd carries `TIOCEXCL` (taken on every
//! open, §7.1). The daemon EBUSYs against itself.
//!
//! **What it costs, measured on this bench's real FTDI adapter before the fix** — this is
//! not a sim artifact, and the first version of the fixture note in `p8_web_ui.rs` was
//! wrong to say the hardware "stays active throughout":
//!
//! * the node reports `faulted` with `Device or resource busy` from ~1 ms after the
//!   replace until the next reconnect poll ~1 s later, so a real console is down for a
//!   second and every `subscribe`/web client sees a spurious fault whose reason sends
//!   the operator hunting for a squatter that is the daemon itself;
//! * a `send` issued in that window is **acknowledged and then silently discarded** —
//!   `serial-nexus-ctl send` reported "sent 20 byte(s)" and `purged_on_reconnect` was 20
//!   three seconds later. §5 permits dropping hostward, never targetward;
//! * the same collision reproduces through `remove-node --cascade` + `add-node`.
//!
//! On a **pts** the fault is permanent rather than one second long, because `TIOCEXCL`
//! lives on the tty and is cleared only at the tty's last close — and a pts whose master
//! `serial-nexus-sim` still holds is never last-closed. That is why this guard can prove the
//! fix with a sim: without the fix the node never recovers, so the assertion cannot pass
//! by waiting. (Verified fail-first: against the unfixed tree
//! `a_replace_keeps_its_serial_port` fails with the node stuck `faulted`, reason
//! `reopen …: Device or resource busy (os error 16)`, at every timeout tried.)
//!
//! **The fix** is one ioctl: release `TIOCEXCL` (`sys::set_exclusive(fd, false)`) in
//! `SerialNode::teardown` before the port is dropped. The lingering fd stops blocking
//! anybody, so the replacement opens cleanly on hardware *and* on a pty-backed virtual
//! serial device — socat `PTY,link=`, QEMU `-serial pty` and `serial-nexus-sim` alike, all of
//! which are legal `raw:` device configurations and all of which were permanently
//! unrecoverable before. The window where the port is briefly not exclusive is bounded
//! by the same synchronous critical section, and the replacement re-takes `TIOCEXCL` on
//! open; a guaranteed one-second outage is the worse trade.

use std::time::Duration;

use serial_nexus_itest::{Daemon, Sim, TempRun, serial_echo, sha256_hex, wait_until};

/// The graph, as text, for a serial node over `dev` plus a log capturing it.
fn config(dev: &std::path::Path, dir: &std::path::Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"

[[node]]
type = "log"
name = "cap"
directory = "{dir}"
filename = "cap.log"

[[edge]]
a = "usb0"
b = "cap"
"#,
        dev = dev.display(),
        dir = dir.display(),
    )
}

#[test]
fn a_replace_keeps_its_serial_port() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP a_replace_keeps_its_serial_port: no serial device on this platform");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logs = TempRun::new();
    let cfg = config(echo.device(), logs.path());

    rpc.load_toml(&cfg, false).expect("load");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active before the replace: {:?}",
        rpc.node("usb0")
    );

    // The whole point: replacing a graph with one that keeps the same port must not
    // take the port down. Not "must recover within a second" — the node's status is
    // read immediately, because a console that faults for a second has already lost a
    // second of device output and told every observer it broke.
    rpc.load_toml(&cfg, true).expect("load --replace");
    let node = rpc.node("usb0").expect("usb0 after the replace");
    assert_eq!(
        node["status"], "active",
        "load --replace faulted the port it was keeping: {node}"
    );
}

#[test]
fn a_write_accepted_right_after_a_replace_reaches_the_device() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP a_write_accepted_right_after_a_replace_reaches_the_device: no serial device"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logs = TempRun::new();
    let cfg = config(echo.device(), logs.path());

    rpc.load_toml(&cfg, false).expect("load");
    assert!(rpc.wait_status("usb0", "active", Duration::from_secs(20)));

    // §5 forbids dropping targetward: an accepted write is a promise. Before the fix
    // this line was acknowledged ("sent N byte(s)") into a port that was faulted, and
    // purge-on-reconnect threw it away — a silent loss with a success reported, which
    // is the worst shape a data-plane bug can take.
    rpc.load_toml(&cfg, true).expect("load --replace");
    const LINE: &str = "reaches-the-device-after-a-replace";
    rpc.send("usb0", LINE, false, 5000)
        .expect("send after replace");

    // The echo device returns what it was given, and the log is the lossless sink
    // (§7.3) — so the byte-exact oracle is the file, never a judgement (§5).
    let file = logs.join("cap.log");
    let want = format!("{LINE}\n");
    assert!(
        wait_until(Duration::from_secs(15), || {
            std::fs::read(&file).is_ok_and(|b| String::from_utf8_lossy(&b).contains(LINE))
        }),
        "the accepted line never reached the device: log={:?} node={:?}",
        std::fs::read_to_string(&file).unwrap_or_default(),
        rpc.node("usb0")
    );
    // And it arrived once, whole.
    let got = std::fs::read(&file).expect("read the log");
    let text = String::from_utf8_lossy(&got);
    assert_eq!(
        text.matches(LINE).count(),
        1,
        "the line should arrive exactly once: {text:?}"
    );
    assert_eq!(
        sha256_hex(want.as_bytes()),
        sha256_hex(text.as_bytes()),
        "the log should hold exactly the line that was sent: {text:?}"
    );
}

#[test]
fn remove_then_add_keeps_the_same_port_openable() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP remove_then_add_keeps_the_same_port_openable: no serial device");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logs = TempRun::new();
    rpc.load_toml(&config(echo.device(), logs.path()), false)
        .expect("load");
    assert!(rpc.wait_status("usb0", "active", Duration::from_secs(20)));

    // The same collision, through the incremental verbs rather than through
    // `--replace` — the path an operator rewiring one port actually takes (§11).
    rpc.remove_node("usb0", true)
        .expect("remove-node --cascade");
    rpc.add_node_toml(&format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{}"
"#,
        echo.device().display()
    ))
    .expect("add-node");

    let node = rpc.node("usb0").expect("usb0 after re-adding");
    assert_eq!(
        node["status"], "active",
        "re-adding the port the previous node just released faulted it: {node}"
    );
}

/// The exclusivity the daemon takes must still be taken — the fix releases it on the
/// way out, and a fix that simply stopped taking it would pass the tests above while
/// silently retiring §7.1's whole reason for `TIOCEXCL`.
#[test]
fn the_port_is_still_exclusive_while_the_node_holds_it() {
    let Some(echo) = serial_echo() else {
        eprintln!("SKIP the_port_is_still_exclusive_while_the_node_holds_it: no serial device");
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    let logs = TempRun::new();
    rpc.load_toml(&config(echo.device(), logs.path()), false)
        .expect("load");
    assert!(rpc.wait_status("usb0", "active", Duration::from_secs(20)));

    // A second opener is refused while the node is live. `serial-nexus-sim`'s own `client`
    // mode opens the device the way a stray process would.
    let verdict = Sim::client(&[
        "--path",
        &echo.device().to_string_lossy(),
        "--send",
        "seeded:16",
        "--expect",
        "echo",
        "--timeout-ms",
        "2000",
    ]);
    assert_eq!(
        verdict["pass"], false,
        "a second process opened a port the daemon holds exclusively: {verdict}"
    );
    // **Assert the reason, not just the verdict** (review 32 ITEST-1). `serial-nexus-sim
    // client` reports `pass:false` in two structurally different shapes, and only one
    // of them is exclusivity: the EBUSY *refusal* (`{"error":"Device or resource busy
    // (os error 16)", …}`, `err_verdict`) and a **successful** open whose echo the
    // daemon's own reader consumed first (`{"received":0,"timed_out":true, …}`). The
    // second needs no `TIOCEXCL` at all — invariant 2's serial reader parks in blocking
    // `poll(2)` and eats everything hostward — so a regression that simply stopped
    // calling `sys::set_exclusive(fd, true)` produced a race, not a failure: measured
    // against an LD_PRELOAD shim that swallows the ioctl, this test caught it only
    // ~40 % of the time on an idle box and ~25 % under load. That is precisely the
    // "cannot pass" claim in the doc comment above, undone by scheduling.
    //
    // `error` is present only on the refusal path, and `received` only on the open
    // path, so either discriminates. Assert both, and name the ioctl in the message:
    // the fix this file exists for *releases* exclusivity in `teardown`, and the way
    // that fix goes wrong is by never taking it in the first place.
    assert!(
        verdict.get("received").is_none(),
        "the stray process OPENED the port and merely lost the race for the echo — the \
         daemon is not holding TIOCEXCL (§7.1): {verdict}"
    );
    assert!(
        verdict["error"]
            .as_str()
            .is_some_and(|e| e.to_ascii_lowercase().contains("busy")),
        "the second open failed for a reason that is not exclusivity, so this guard \
         proves nothing about TIOCEXCL (§7.1): {verdict}"
    );
}
