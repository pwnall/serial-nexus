#![forbid(unsafe_code)]

//! **The rate a port actually got, on the wire** — design §7.1's *Open, hold, and
//! reopen* clause 7, decided at §15.58, built as plan §18 item 41.
//!
//! A serial node's `state` carries `actual_baud` beside `baud`: the rate the driver
//! answers with beside the rate the configuration asked for. Reporting only — no
//! verdict, no fault, no refusal (§15.58 records why this is *not* §15.53's refusal:
//! a driver quantizing to its clock divisor is ordinary, and only the operator knows
//! which margin their device tolerates).
//!
//! **What each altitude can prove, because no one of them proves the field alone:**
//!
//! 1. *The helper reads the port* —
//!    `daemon/src/nodes/serial.rs::actual_baud_follows_the_port_rather_than_the_rate_that_was_asked_for`,
//!    which manufactures a divergence (a master-side `TCSETS` moves the slave's
//!    termios under an open port) and requires the reported number to follow the tty.
//!    That is the only altitude where ask and answer can be made to disagree without
//!    hardware.
//! 2. *The key reaches the wire, and never invents an answer* — [`a_serial_node_with_no_port_reports_no_rate_rather_than_the_one_it_asked_for`]
//!    here, which needs no device and runs on every platform. It is the arm an
//!    **echo** dies on: a `state` that reported the configured rate whatever the port
//!    was doing would answer `250000` for a device that does not exist.
//! 3. *The key is live rather than pinned at `null`* — [`an_open_serial_node_reports_the_rate_its_port_answers_with`],
//!    which needs a serial device and so is Linux-only here. It is the arm a
//!    **hardcoded `null`** dies on. On a pts the answer necessarily equals the ask, so
//!    this arm is deliberately *not* offered as proof that the field is a read-back;
//!    arm 1 and the rig are.
//! 4. *A real driver landing somewhere else* —
//!    `itest/tests/serial_hardware.rs::crossover_rig_actual_baud_is_a_read_back_not_an_echo`,
//!    on the cross-wired FT232R rig, where the adapter does the diverging itself.
//!
//! **Presence is asserted with `get`, never with `[...]`.** `Value::index` answers
//! `Null` for a key that is not there, so `node["actual_baud"] == Null` passes
//! unchanged against a tree that emits no such field at all — a guard whose passing
//! output equals its not-running output (AGENTS §3). The design's rule is that an
//! unknown must *say so*, which is a key that exists and carries `null`, so that is
//! what these read.

use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, Rpc, TempRun, serial_echo};

/// A rate no default anywhere in the tree would produce, so a number that appears in
/// `actual_baud` came from this configuration or from the port and from nowhere else.
const ASKED: u32 = 250_000;

/// One serial node at [`ASKED`], no edges — every check here reads `state` only.
fn one_serial_node(device: &str) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "usb0"
device = "{device}"
baud = {ASKED}
arbitration = "free-for-all"
"#
    )
}

/// The `actual_baud` cell of a node, distinguishing **absent** (`None`) from
/// **reported as unknown** (`Some(Value::Null)`) — the distinction the whole field
/// turns on.
fn actual_baud(rpc: &Rpc, node: &str) -> Option<Value> {
    rpc.node(node)?.get("actual_baud").cloned()
}

/// A node holding no port reports `actual_baud: null` — present, and unknown — while
/// `baud` still reports what was asked for.
///
/// This is the anti-echo arm. §7.1 clause 7: *"Where a platform cannot report the rate
/// back, the field says so rather than echoing the request: an unknown rendered as an
/// answer is the shape §12's `has_identity_source` exists to prevent, and echoing the
/// ask would make the field agree with itself everywhere and assert nothing."* A
/// `waiting` node is the cheapest reachable instance of "no answer to read", and it
/// needs no device, so this runs on every platform.
#[test]
fn a_serial_node_with_no_port_reports_no_rate_rather_than_the_one_it_asked_for() {
    let d = Daemon::start();
    let rpc = d.rpc();
    let run = TempRun::new();
    let absent = run.join("absent-device");
    rpc.load_toml(&one_serial_node(&absent.display().to_string()), false)
        .expect("a serial node on an absent device loads and waits (§15.8)");

    assert_eq!(
        rpc.node_status("usb0"),
        "waiting",
        "the premise of this arm is a node with no port: {:?}",
        rpc.node("usb0")
    );
    let node = rpc.node("usb0").expect("usb0 in state");
    assert_eq!(
        node.get("open"),
        Some(&Value::Bool(false)),
        "a waiting node reported an open port: {node}"
    );
    assert_eq!(
        node.get("baud").and_then(Value::as_u64),
        Some(u64::from(ASKED)),
        "the configured rate is still reported beside the read-back: {node}"
    );
    assert_eq!(
        actual_baud(rpc, "usb0"),
        Some(Value::Null),
        "a serial node with no open port must report `actual_baud: null` — present, \
         and saying it does not know. Reporting {ASKED} here would be the \
         configuration echoing itself, which is the one shape §7.1 clause 7 names; a \
         *missing* key is the same claim made by omission: {node}"
    );
}

/// A node whose port is open reports the rate that port answers with.
///
/// The anti-`null` arm: it fails a `state_extra` that carries the key but never wires
/// the read-back in. It is honest about what it does **not** prove — a pts honours
/// every rate it is given, so ask and answer agree here by construction, and an
/// implementation that echoed the ask would pass this arm. That is what the arm above
/// and the rig arm are for.
///
/// Linux-only: a pts cannot be a `serial2` device on macOS (`ENOTTY`, AGENTS §7), so
/// it self-skips there — a skip is a valid verdict (§5).
#[test]
fn an_open_serial_node_reports_the_rate_its_port_answers_with() {
    let Some(echo) = serial_echo() else {
        eprintln!(
            "SKIP an_open_serial_node_reports_the_rate_its_port_answers_with: \
             no serial device on this platform"
        );
        return;
    };
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(
        &one_serial_node(&echo.device().display().to_string()),
        false,
    )
    .expect("load a serial node on the software device");
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 not active: {:?}",
        rpc.node("usb0")
    );

    let node = rpc.node("usb0").expect("usb0 in state");
    assert_eq!(
        node.get("open"),
        Some(&Value::Bool(true)),
        "the premise of this arm is a node holding a port: {node}"
    );
    assert_eq!(
        actual_baud(rpc, "usb0"),
        Some(Value::from(ASKED)),
        "an open port reported no rate (or the wrong one). This tty accepted \
         {ASKED} — the daemon's own open verifies that by read-back, so the node \
         could not be `active` otherwise — and `state` must carry what it answers \
         with: {node}"
    );
}
