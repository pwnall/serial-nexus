//! Real-hardware serial data plane over a cross-wired null-modem rig — the macOS
//! analogue of the retired `hardware/crossover-rig.sh` (design §13/§15.21), and the
//! *only* way to exercise the serial path on macOS (a pty cannot stand in for a serial
//! device there — `serial2` → `ENOTTY`).
//!
//! Self-skips when no rig is present (a skip is a valid verdict, §5). One `#[test]`, so
//! the two physical ports are never contended by parallel test threads. It certifies,
//! end to end through the daemon and the (macOS-fixed) PTY injector:
//!
//! * bidirectional byte-exactness — sim client → pty → serial → crossover → serial →
//!   log, each way, checked by SHA-256 (the daemon's own fast reader is lossless, unlike
//!   a raw high-volume read of a flow-control-less UART);
//! * the `send` verb reaching real hardware on the far port;
//! * `TIOCEXCL` exclusivity — a second open of a daemon-held port is refused.

use std::time::Duration;

use nexus_itest::{Daemon, Sim, crossover_ports, sha256_hex, wait_until};

#[test]
fn crossover_rig_data_plane_send_and_exclusivity() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!(
            "SKIP crossover_rig_data_plane_send_and_exclusivity: no crossover rig \
             (attach two cross-wired cu.usbserial adapters, or set SNX_CROSSOVER_A/_B)"
        );
        return;
    };
    eprintln!("crossover rig: {p0} <-> {p1}");

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let inj0 = run.join("inj0");
    let inj1 = run.join("inj1");
    // Symmetric null modem: each port has a pty injector (client → targetward) and a
    // write-never log capturing its hostward stream (what crossed the wire from the far
    // port). free-for-all so the injectors and the `send` verb write without lock ceremony.
    let cfg = format!(
        r#"
[[node]]
type = "serial"
name = "port0"
device = "{p0}"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "{p1}"
baud = 115200
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
        dir = run.path().display(),
        inj0 = inj0.display(),
        inj1 = inj1.display(),
    );
    rpc.load_toml(&cfg, false).expect("load crossover config");
    for port in ["port0", "port1"] {
        assert!(
            rpc.wait_status(port, "active", Duration::from_secs(20)),
            "{port} not active: {:?}",
            rpc.node(port)
        );
    }
    // The usb: / raw: identity resolved to the real /dev path.
    assert_eq!(rpc.node("port0").unwrap()["resolved_path"], p0.as_str());
    assert_eq!(rpc.node("port1").unwrap()["resolved_path"], p1.as_str());
    // Driver counters are absent on macOS (TIOCGICOUNT is Linux-only) — a graceful
    // degradation, never a fault. On Linux they are present; either is fine here.
    eprintln!(
        "port1 driver_counters: {}",
        rpc.node("port1").unwrap()["driver_counters"]
    );

    // A seeded burst injected into `inj` must arrive byte-exact at `rx_log` (which
    // starts empty), across the physical wire. Returns the bytes now in the log.
    let inject_verify = |inj: &std::path::Path, rx_log: &std::path::Path, seed: &str| -> u64 {
        let verdict = Sim::client(&[
            "--path",
            &inj.to_string_lossy(),
            "--send",
            "seeded:32KiB",
            "--seed",
            seed,
            "--timeout-ms",
            "30000",
        ]);
        let sent_sha = verdict["sha256_sent"].as_str().expect("sha256_sent");
        let n = verdict["sent"].as_u64().expect("sent") as usize;
        assert!(n > 0, "sim sent nothing: {verdict}");
        let arrived = wait_until(Duration::from_secs(30), || {
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
    };

    // Direction A: inj0 → port0 TX → wire → port1 RX → rx1. Then B, the mirror.
    let rx1_after_a = inject_verify(&inj0, &run.join("rx1.log"), "21");
    inject_verify(&inj1, &run.join("rx0.log"), "22");

    // The `send` verb reaches real hardware: a nonce sent on port0 appears at port1
    // (after the direction-A bytes already in rx1).
    let nonce = format!("SNX_HW_{}", std::process::id());
    rpc.send("port0", &nonce, false, 5000).expect("send verb");
    let rx1 = run.join("rx1.log");
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
