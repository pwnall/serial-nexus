//! Real-hardware serial data plane over a cross-wired null-modem rig, the macOS
//! analogue of `scripts/validate/hardware/crossover-rig.sh` (design §13/§15.21).
//!
//! Self-skips when no rig is present (a skip is a valid verdict, §5). On macOS this is
//! the *only* way to exercise the serial path (a pty cannot stand in for a serial
//! device there — serial2 → `ENOTTY`), so it doubles as the macOS serial gate. It also
//! exercises the (macOS-fixed) PTY injector end to end: sim client → pty → serial →
//! crossover → serial → log, byte-exact.

use std::time::Duration;

use nexus_itest::{Daemon, Sim, crossover_ports, sha256_hex, wait_until};

#[test]
fn crossover_byte_exact_data_plane() {
    let Some((p0, p1)) = crossover_ports() else {
        eprintln!(
            "SKIP crossover_byte_exact_data_plane: no crossover rig \
             (attach two cross-wired cu.usbserial adapters, or set SNX_CROSSOVER_A/_B)"
        );
        return;
    };

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let inj0 = run.join("inj0");
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
name = "rx1"
directory = "{dir}"
filename = "rx1.log"
[[node]]
type = "pty"
name = "inj0"
path = "{inj0}"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
[[edge]]
a = "port0"
b = "inj0"
"#,
        dir = run.path().display(),
        inj0 = inj0.display(),
    );
    rpc.load_toml(&cfg, false).expect("load crossover config");
    assert!(
        rpc.wait_status("port0", "active", Duration::from_secs(20)),
        "port0 not active: {:?}",
        rpc.node("port0")
    );
    assert!(
        rpc.wait_status("port1", "active", Duration::from_secs(20)),
        "port1 not active"
    );

    // Inject 32 KiB of seeded data into the pty injector; it flows targetward out
    // port0, crosses the physical wire, and lands hostward at port1's capture log.
    let verdict = Sim::client(&[
        "--path",
        &inj0.to_string_lossy(),
        "--send",
        "seeded:32KiB",
        "--seed",
        "21",
        "--timeout-ms",
        "30000",
    ]);
    let sent_sha = verdict["sha256_sent"]
        .as_str()
        .expect("sim reported sha256_sent");
    let bytes = verdict["sent"].as_u64().expect("sim reported sent") as usize;
    assert!(bytes > 0, "sim sent nothing: {verdict}");

    let rx = run.join("rx1.log");
    let arrived = wait_until(Duration::from_secs(30), || {
        std::fs::metadata(&rx)
            .map(|m| m.len() as usize >= bytes)
            .unwrap_or(false)
    });
    let got = std::fs::metadata(&rx).map(|m| m.len()).unwrap_or(0);
    assert!(arrived, "only {got} / {bytes} B crossed the crossover wire");

    let data = std::fs::read(&rx).expect("read capture log");
    let recv_sha = sha256_hex(&data[..bytes]);
    assert_eq!(
        sent_sha, recv_sha,
        "bytes were lost/reordered across the crossover wire"
    );
}
