//! Real-hardware serial data plane over a cross-wired null-modem rig — the macOS
//! analogue of the retired `hardware/crossover-rig.sh` (design §13/§15.21), and the
//! *only* way to exercise the serial path on macOS (a pty cannot stand in for a serial
//! device there — `serial2` → `ENOTTY`).
//!
//! Self-skips when no rig is present (a skip is a valid verdict, §5). Every test here
//! grabs the two physical ports, so they share a process-wide [`RIG`] mutex and run one
//! at a time even under the default multi-threaded test harness — the two ports are
//! never contended. Through the daemon and the (macOS-fixed) PTY injector, this file
//! certifies:
//!
//! * bidirectional byte-exactness — sim client → pty → serial → crossover → serial →
//!   log, each way, checked by SHA-256 (the daemon's own fast reader is lossless, unlike
//!   a raw high-volume read of a flow-control-less UART) — at 115200 and, in a dedicated
//!   test, at the custom rate 250000 (baud is a termios setting, so a byte-exact
//!   round-trip at each rate needs no TIOCGICOUNT and runs on macOS);
//! * the `send` verb reaching real hardware on the far port;
//! * `TIOCEXCL` exclusivity — a second open of a daemon-held port is refused;
//! * the serial *signal* verbs (`send-break`/`set-modem`/`pulse-dtr`) driven end to end
//!   through the daemon against a real UART — the Tier-3 property (design §13) this
//!   null-modem rig exists to test, unreachable on the pts that `p7_signals` uses
//!   (set-modem/pulse-dtr `ENOTTY` a pts). Far-end break *reception* is observed
//!   best-effort (a break surfaces at the peer RX as a NUL) but not asserted — macOS
//!   has no frame-error counter (TIOCGICOUNT is Linux-only).

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use nexus_itest::{Daemon, Sim, crossover_ports, sha256_hex, wait_until};
use serde_json::json;

/// Serializes the rig tests: each holds the two physical ports exclusively, so they must
/// not run concurrently even though the default harness runs a binary's tests in parallel.
/// A panicking test poisons the mutex; we recover the guard (`into_inner`) so a failure in
/// one rig test does not cascade the rest into spurious poison-panics — they still run and
/// report their own verdicts.
static RIG: Mutex<()> = Mutex::new(());

fn rig_guard() -> MutexGuard<'static, ()> {
    RIG.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The symmetric null-modem graph at `baud`: each port has a pty injector (client →
/// targetward) and a write-never log capturing its hostward stream (what crossed the wire
/// from the far port). `free-for-all` so the injectors and the `send` verb write without
/// lock ceremony.
fn null_modem_cfg(p0: &str, p1: &str, baud: u32, dir: &Path, inj0: &Path, inj1: &Path) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "port0"
device = "{p0}"
baud = {baud}
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "{p1}"
baud = {baud}
arbitration = "free-for-all"
[[node]]
type = "log"
name = "rx0"
directory = "{dir}"
filename = "rx0.log"
[[node]]
type = "log"
name = "rx1"
directory = "{dir}"
filename = "rx1.log"
[[node]]
type = "pty"
name = "inj0"
path = "{inj0}"
[[node]]
type = "pty"
name = "inj1"
path = "{inj1}"
[[edge]]
a = "port0"
b = "rx0"
write_mode = "never"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
[[edge]]
a = "port0"
b = "inj0"
[[edge]]
a = "port1"
b = "inj1"
"#,
        dir = dir.display(),
        inj0 = inj0.display(),
        inj1 = inj1.display(),
    )
}

/// Inject `size` (e.g. `"32KiB"`) seeded bytes into `inj`; they must arrive byte-exact at
/// `rx_log` (which starts empty) across the physical wire. Returns the log length after.
fn inject_verify(inj: &Path, rx_log: &Path, seed: &str, size: &str) -> u64 {
    let verdict = Sim::client(&[
        "--path",
        &inj.to_string_lossy(),
        "--send",
        &format!("seeded:{size}"),
        "--seed",
        seed,
        "--timeout-ms",
        "60000",
    ]);
    let sent_sha = verdict["sha256_sent"].as_str().expect("sha256_sent");
    let n = verdict["sent"].as_u64().expect("sent") as usize;
    assert!(n > 0, "sim sent nothing: {verdict}");
    let arrived = wait_until(Duration::from_secs(60), || {
        std::fs::metadata(rx_log)
            .map(|m| m.len() as usize >= n)
            .unwrap_or(false)
    });
    let got = std::fs::metadata(rx_log).map(|m| m.len()).unwrap_or(0);
    assert!(
        arrived,
        "{}: only {got}/{n} B crossed the wire",
        rx_log.display()
    );
    let data = std::fs::read(rx_log).expect("read capture log");
    assert_eq!(
        sent_sha,
        sha256_hex(&data[..n]),
        "{}: bytes lost/reordered across the wire",
        rx_log.display()
    );
    got
}

/// Boot a daemon on the rig at `baud` and wait for both ports active. Returns the daemon
/// (keep it alive; drop releases the ports and reaps the child) plus owned paths for the
/// `run` dir and the two injector ptys.
fn boot_rig(
    p0: &str,
    p1: &str,
    baud: u32,
) -> (
    Daemon,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let d = Daemon::start();
    let (run_dir, inj0, inj1) = {
        let run = d.run();
        (run.path().to_path_buf(), run.join("inj0"), run.join("inj1"))
    };
    d.rpc()
        .load_toml(&null_modem_cfg(p0, p1, baud, &run_dir, &inj0, &inj1), false)
        .unwrap_or_else(|e| panic!("load rig config @ {baud}: {e:?}"));
    for port in ["port0", "port1"] {
        assert!(
            d.rpc().wait_status(port, "active", Duration::from_secs(20)),
            "{port} not active @ {baud}: {:?}",
            d.rpc().node(port)
        );
    }
    (d, run_dir, inj0, inj1)
}

#[test]
fn crossover_rig_data_plane_send_and_exclusivity() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!(
            "SKIP crossover_rig_data_plane_send_and_exclusivity: no crossover rig \
             (attach two cross-wired cu.usbserial adapters, or set SNX_CROSSOVER_A/_B)"
        );
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig: {p0} <-> {p1}");

    let (d, run_dir, inj0, inj1) = boot_rig(&p0, &p1, 115_200);
    let rpc = d.rpc();
    // The usb: / raw: identity resolved to the real /dev path.
    assert_eq!(rpc.node("port0").unwrap()["resolved_path"], p0.as_str());
    assert_eq!(rpc.node("port1").unwrap()["resolved_path"], p1.as_str());
    // Driver counters are absent on macOS (TIOCGICOUNT is Linux-only) — a graceful
    // degradation, never a fault. On Linux they are present; either is fine here.
    eprintln!(
        "port1 driver_counters: {}",
        rpc.node("port1").unwrap()["driver_counters"]
    );

    // Direction A: inj0 → port0 TX → wire → port1 RX → rx1. Then B, the mirror.
    let rx1_after_a = inject_verify(&inj0, &run_dir.join("rx1.log"), "21", "32KiB");
    inject_verify(&inj1, &run_dir.join("rx0.log"), "22", "32KiB");

    // The `send` verb reaches real hardware: a nonce sent on port0 appears at port1
    // (after the direction-A bytes already in rx1).
    let nonce = format!("SNX_HW_{}", std::process::id());
    rpc.send("port0", &nonce, false, 5000).expect("send verb");
    let rx1 = run_dir.join("rx1.log");
    let saw_nonce = wait_until(Duration::from_secs(5), || {
        std::fs::read(&rx1)
            .map(|b| b.len() as u64 > rx1_after_a && String::from_utf8_lossy(&b).contains(&nonce))
            .unwrap_or(false)
    });
    assert!(
        saw_nonce,
        "send-verb nonce {nonce:?} never reached port1 over the wire"
    );

    // TIOCEXCL: the daemon holds both ports exclusively, so a second open is refused.
    let second_open = std::fs::OpenOptions::new().read(true).write(true).open(&p0);
    assert!(
        second_open.is_err(),
        "a second open of {p0} succeeded while the daemon holds it — TIOCEXCL not enforced"
    );
}

/// Byte-exact bidirectional transfer at 250000 — a high, non-default custom rate the
/// 115200 test does not cover, proving the FTDI actually *clocks* a custom baud on the
/// wire (the doctor's P3 only proves the driver stores/reads the divisor back). A fresh
/// daemon per baud (each Drop releases the ports). No TIOCGICOUNT needed: the check is a
/// SHA-256 over captured log bytes, so it runs on macOS.
///
/// Only rates fast enough to drain before the one-shot `Sim::client` injector closes its
/// pty are reliable here; very slow rates (e.g. 9600) race that close and are covered by
/// the daemon's own sim/unit tests, not this rig test.
#[test]
fn crossover_rig_custom_baud_byte_exact() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!("SKIP crossover_rig_custom_baud_byte_exact: no crossover rig");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (custom baud): {p0} <-> {p1}");

    let baud = 250_000u32;
    let (_d, run_dir, inj0, inj1) = boot_rig(&p0, &p1, baud);
    // Let both FTDI adapters settle at the new line rate before the first byte — an FTDI
    // can garble the first bytes sampled right after open+set_baud at 115200 and above
    // (the doctor's P5_OPEN_SETTLE finding); the discovery-style re-send that masks it
    // elsewhere is not available here, so settle explicitly.
    std::thread::sleep(Duration::from_millis(500));
    let a = inject_verify(&inj0, &run_dir.join("rx1.log"), "2500001", "32KiB"); // A→B
    let b = inject_verify(&inj1, &run_dir.join("rx0.log"), "2500002", "32KiB"); // B→A
    eprintln!("baud {baud}: A→B {a} B, B→A {b} B — byte-exact both directions");
}

/// The serial signal verbs driven end to end through the daemon against a real UART —
/// send-break, set-modem (DTR/RTS high then low), pulse-dtr. On a real port these ioctls
/// succeed (a pts `ENOTTY`s set-modem/pulse-dtr, which is why `p7_signals` cannot cover
/// this). Far-end break reception is observed best-effort (a break surfaces at the peer
/// RX as a NUL) and logged, not asserted — deterministic break→frame-error detection needs
/// TIOCGICOUNT, which is Linux-only.
#[test]
fn crossover_rig_signal_verbs() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!("SKIP crossover_rig_signal_verbs: no crossover rig");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (signals): {p0} <-> {p1}");

    let (d, run_dir, _inj0, _inj1) = boot_rig(&p0, &p1, 115_200);
    let rpc = d.rpc();
    std::thread::sleep(Duration::from_millis(300));

    let rx1 = run_dir.join("rx1.log");
    let before = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);

    // send-break through the full daemon stack must succeed against the real UART.
    let br = rpc
        .send_break("port0", 200)
        .expect("send-break RPC on real port0");
    assert_eq!(br["break_ms"], json!(200), "send-break echo: {br}");

    // set-modem: DTR+RTS high, then low — both must succeed against real hardware.
    let hi = rpc
        .call(
            "set-modem",
            json!({ "node": "port0", "dtr": true, "rts": true }),
        )
        .expect("set-modem hi on real port0");
    assert_eq!(hi["dtr"], json!(true));
    assert_eq!(hi["rts"], json!(true));
    let lo = rpc
        .call(
            "set-modem",
            json!({ "node": "port0", "dtr": false, "rts": false }),
        )
        .expect("set-modem lo on real port0");
    assert_eq!(lo["dtr"], json!(false));

    // pulse-dtr: the classic auto-reset toggle, over real hardware.
    let pd = rpc
        .call(
            "pulse-dtr",
            json!({ "node": "port0", "ms": 100, "assert": true }),
        )
        .expect("pulse-dtr on real port0");
    assert_eq!(pd["pulse_ms"], json!(100));

    // The port must stay active after all signal manipulation (no fault).
    assert!(
        rpc.wait_status("port0", "active", Duration::from_secs(3)),
        "port0 faulted after signal verbs: {:?}",
        rpc.node("port0")
    );

    // Best-effort far-end observation (informational, not asserted): a break on port0 may
    // surface at port1 RX as a framing anomaly / NUL. macOS has no frame-error counter, so
    // we only report whether bytes appeared.
    std::thread::sleep(Duration::from_millis(200));
    let after = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);
    let delta = std::fs::read(&rx1)
        .map(|b| b[before as usize..].to_vec())
        .unwrap_or_default();
    eprintln!(
        "signal verbs on real port0 all returned Ok — far-end rx1 delta = {} B {:?} \
         (informational; macOS has no frame counter)",
        after - before,
        &delta[..delta.len().min(16)]
    );
}

/// The v11 **map node** (§7.8) driven over the physical crossover rig — the map's
/// data plane is sim-exercisable (`p8_map.rs` on a Linux null modem), but per the
/// §16.7 doctrine the new node also earns a real-silicon drive. A `map` sits in front
/// of `port0`: its held raw edge OMITS `write_mode`, so this doubles as the on-real-
/// hardware regression for the audit's held-default fix (an omitted map raw edge must
/// acquire the upstream lock, not park). Both directions are checked byte-exact over
/// the wire against an independent oracle:
///
/// * **targetward** — `send console` (mapped side) → `lfcrlf` → `port0` TX → wire →
///   `port1` RX → `rx1`, which must equal the oracle-mapped line;
/// * **hostward** — `send port1` (raw, CR-laden) → wire → `port0` RX → `crlf` map →
///   `maplog`, the mapped view, which must equal the CR→LF oracle.
#[test]
fn crossover_rig_map_node_both_directions() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!("SKIP crossover_rig_map_node_both_directions: no crossover rig");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (map node): {p0} <-> {p1}");

    let d = Daemon::start();
    let rpc = d.rpc();
    let run_dir = d.run().path().to_path_buf();
    // port0 is exclusive so the map genuinely HOLDS its write lock; port1 is
    // free-for-all so `send port1` (the raw hostward injection) needs no ceremony.
    // The map's raw edge omits write_mode → must default to held (the audit fix).
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "port0"
device = "{p0}"
baud = 115200
[[node]]
type = "serial"
name = "port1"
device = "{p1}"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "map"
name = "console"
hostward = ["crlf"]
targetward = ["lfcrlf"]
[[node]]
type = "log"
name = "maplog"
directory = "{dir}"
filename = "maplog.log"
[[node]]
type = "log"
name = "rx1"
directory = "{dir}"
filename = "rx1.log"
[[edge]]
a = "port0"
b = "console/raw"
[[edge]]
a = "console"
b = "maplog"
write_mode = "never"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
"#,
        dir = run_dir.display(),
    );
    rpc.load_toml(&cfg, false).expect("load map rig config");
    for node in ["port0", "port1", "console"] {
        assert!(
            rpc.wait_status(node, "active", Duration::from_secs(20)),
            "{node} not active: {:?}",
            rpc.node(node)
        );
    }

    // The audit fix on real hardware: an omitted map raw-edge write_mode defaults to
    // `held`, so the map acquires port0's lock on attach (holder = "console/raw").
    assert!(
        wait_until(Duration::from_secs(5), || {
            rpc.node("port0")
                .and_then(|n| n["lock"]["holder"].as_str().map(str::to_owned))
                == Some("console/raw".to_owned())
        }),
        "an omitted map raw-edge write_mode must default to held on real hardware (§7.8): {:?}",
        rpc.node("port0")
    );

    // Let both FTDIs settle at line rate before the first byte (the P5 first-byte
    // garble at 115200+); short exact-match messages are unforgiving of a garbled byte.
    std::thread::sleep(Duration::from_millis(500));

    // --- Targetward over the wire: send at the map → lfcrlf → port0 TX → port1 RX ---
    let rx1 = run_dir.join("rx1.log");
    rpc.send("console", "MAP-TARGET-hello", false, 5000)
        .expect("send console (targetward through the map)");
    // "MAP-TARGET-hello" + the send's trailing '\n' → lfcrlf → ...hello\r\n.
    let want_rx1 = b"MAP-TARGET-hello\r\n".to_vec();
    assert!(
        wait_until(Duration::from_secs(10), || std::fs::read(&rx1)
            .map(|b| b == want_rx1)
            .unwrap_or(false)),
        "targetward map output did not cross the wire byte-exact; rx1={:?}",
        std::fs::read(&rx1).unwrap_or_default()
    );

    // --- Hostward over the wire: send raw (CR-laden) at port1 → port0 RX → crlf map ---
    let maplog = run_dir.join("maplog.log");
    rpc.send("port1", "MAP\rHOST\rEND", false, 5000)
        .expect("send port1 (raw hostward injection)");
    // port1 TX = "MAP\rHOST\rEND\n"; crlf maps each CR(0x0d)→LF(0x0a); the trailing \n
    // is untouched → "MAP\nHOST\nEND\n" in the mapped view.
    let want_maplog = b"MAP\nHOST\nEND\n".to_vec();
    assert!(
        wait_until(Duration::from_secs(10), || std::fs::read(&maplog)
            .map(|b| b == want_maplog)
            .unwrap_or(false)),
        "hostward map output (CR→LF) did not match the oracle; maplog={:?}",
        std::fs::read(&maplog).unwrap_or_default()
    );

    // The map's per-direction/per-rule counters reflect the two mapped lines.
    let node = rpc.node("console").expect("map node in state");
    assert_eq!(
        node["targetward"]["rules"]["lfcrlf"].as_u64(),
        Some(1),
        "targetward lfcrlf fired once (the one LF in the sent line): {node}"
    );
    assert_eq!(
        node["hostward"]["rules"]["crlf"].as_u64(),
        Some(2),
        "hostward crlf fired for both CRs in the injected line: {node}"
    );
    eprintln!("map node byte-exact both directions over the physical crossover ✓");
}
