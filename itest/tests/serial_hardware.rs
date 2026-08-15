#![forbid(unsafe_code)]

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

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, Sim, attach_slave, crossover_ports, file_len, settled_while_open, sha256_hex,
    skip_no_rig, wait_until,
};

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
///
/// **The arrival is observed while a client still holds the injector's pty**, which is
/// the whole reason `witness` exists (notes §3.29 / §3.56, plan §3 rule 8). The
/// one-shot `Sim::client` below closes its slave the moment its `write_all` returns —
/// and `write_all` returns when the *kernel* accepted the last byte, not when the
/// daemon read it, so up to a pty buffer's worth of this payload is still inside the
/// pair at that instant. Reading `rx_log`'s length afterwards therefore asserted that
/// the kernel retained those bytes across the slave's last close: true on Linux
/// (doctor P13 `retains`), not promised anywhere, and measured false on Darwin
/// (`waits-then-discards`). The harness's own fd on the same slave is what makes that
/// close not be the last one — every kernel hangs its flush on the reference count
/// reaching zero — so the wire is measured against a live session on every platform.
///
/// The witness is opened *before* the injector, not after: a pty node that has never
/// seen a client is a different configuration from one that has, and the point is that
/// the session is continuous from the first byte to the last observation.
fn inject_verify(inj: &Path, rx_log: &Path, seed: &str, size: &str) -> u64 {
    let mut witness = attach_slave(inj);
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
    let sent_sha = verdict["sha256_sent"]
        .as_str()
        .expect("sha256_sent")
        .to_owned();
    let n = verdict["sent"].as_u64().expect("sent") as usize;
    assert!(n > 0, "sim sent nothing: {verdict}");
    let arrived = settled_while_open(
        &mut [&mut witness],
        &format!("{} crossing the wire", rx_log.display()),
        Duration::from_secs(60),
        || file_len(rx_log) as usize >= n,
    );
    // The session ends here, and nowhere earlier: moving the observation above this
    // line is the point of the borrow, and moving it below is `E0382`.
    drop(witness);

    let got = file_len(rx_log);
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

/// **Absorb the packet a freshly re-enumerated FT232R swallows, once per process**
/// (notes §3.70, plan §18 item 3).
///
/// A USB re-enumeration makes the receiving adapter drop the **first 64 bytes** —
/// exactly one bulk packet — of the first traffic that crosses afterwards. Measured
/// below the daemon and attributed there rather than guessed at: at the stall the
/// kernel's own `TIOCGICOUNT` reads `tx 32768` on the sender and `rx 32704` on the
/// receiver with `frame`/`overrun`/`parity`/`buf_overrun` all zero, while every
/// daemon-side counter (`discarded_unattached`, `dropped_bytes`, `queued_bytes`,
/// `purged_on_reconnect`) reads 0. The bytes never reached the receiving driver, and
/// the missing ones are the **first** 64: a failing capture is byte-identical to a
/// good one from offset 64 (`bad == good[64..]`), which is what makes this a lost
/// leading packet rather than a truncation or a stall.
///
/// **Idle time does not fix it and traffic does**, which is why this is a primer and
/// not a `sleep`: gaps of 2 s and 5 s between the replug and the measurement still
/// failed 3 of 3, while in a six-test run only the *first* test ever failed — every
/// later one is protected by the traffic its predecessor already pushed. The 500 ms
/// settle in `crossover_rig_custom_baud_byte_exact` is for a different FTDI quirk
/// (garbled bytes right after `open`+`set_baud`, P5_OPEN_SETTLE) and does not cover
/// this one.
///
/// This is the same shape as §3.47's `--open-file` handshake: **prove the far end is
/// really receiving before you start counting**, rather than assume the link is live
/// because both nodes report `active`. Once per process because the loss is once per
/// enumeration, and a test binary cannot replug its own adapters — the replug lane is
/// a different binary that has already finished.
fn prime_the_wire_once(p0: &str, p1: &str) {
    static PRIMED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    PRIMED.get_or_init(|| {
        let (d, run_dir, inj0, inj1) = boot_rig_raw(p0, p1, 115_200);
        // 1 KiB each way: comfortably more than the one packet that may be eaten, so
        // "something arrived" stays a usable signal even when the loss fires.
        for (inj, log) in [(&inj0, "rx1.log"), (&inj1, "rx0.log")] {
            let _ = Sim::client(&[
                "--path",
                &inj.to_string_lossy(),
                "--send",
                "seeded:1KiB",
                "--seed",
                "9001",
                "--timeout-ms",
                "5000",
            ]);
            let path = run_dir.join(log);
            assert!(
                serial_nexus_itest::wait_until(Duration::from_secs(15), || file_len(&path) > 0),
                "priming {p0} <-> {p1}: nothing reached {}, so the rig is not carrying \
                 bytes at all and every measurement below would be meaningless",
                path.display()
            );

            // **Then wait for the priming traffic to FINISH, which is not the same
            // question and cost this a real failure on macOS** (plan §18 item 70).
            // The assertion above is satisfied by the *first* byte, so the primer used
            // to drop its daemon with most of a KiB still in flight — and this
            // function's own closing comment claimed the opposite ("no primed byte can
            // land in a capture under test"). On Darwin 24.6.0 the adapter hands the
            // residue to the *next* reader: `crossover_rig_map_node_both_directions`
            // failed 4 of 4 with **62 bytes** — one FTDI bulk packet's payload, 64
            // minus the two status bytes — prepended to its own correct output, and
            // those 62 bytes are a *contiguous slice of this function's seed-9001
            // stream* (found at offsets 833 and 789 of the 1 KiB), which is how the
            // attribution was made rather than guessed.
            //
            // **Quiescence, not a byte count.** Waiting for 1024 would hang on exactly
            // the kernel this primer exists for: where the leading packet is *eaten*
            // the log never reaches 1024. Stability is the portable question, and it
            // is the same one on a kernel that drops bytes and one that retains them.
            let mut last = 0u64;
            let mut stable = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
                let now = file_len(&path);
                if now == last {
                    stable += 1;
                    if stable >= 3 {
                        break;
                    }
                } else {
                    stable = 0;
                    last = now;
                }
            }
            eprintln!(
                "primed {} with {last} B, quiet for {}00 ms",
                path.display(),
                stable
            );
        }
        // The primer's own graph, logs and run dir go away with it, and the wait above
        // is what makes the rest of this sentence true rather than merely intended:
        // the measured tests each boot their own graph, so no primed byte can land in
        // a capture under test.
        drop(d);
    });
}

/// Boot a daemon on the rig at `baud` and wait for both ports active, priming the
/// link first (see [`prime_the_wire_once`]). Returns the daemon (keep it alive; drop
/// releases the ports and reaps the child) plus owned paths for the `run` dir and the
/// two injector ptys.
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
    prime_the_wire_once(p0, p1);
    boot_rig_raw(p0, p1, baud)
}

/// [`boot_rig`] without the priming, so the primer itself can use it without
/// recursing. Nothing else should call this: a measurement that skips the prime is
/// the flake.
fn boot_rig_raw(
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
        skip_no_rig("crossover_rig_data_plane_send_and_exclusivity");
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
/// **Corrected 2026-08-05 (notes §3.57).** This comment used to read "only rates fast
/// enough to drain before the one-shot `Sim::client` injector closes its pty are
/// reliable here; very slow rates (e.g. 9600) race that close" — which named the wrong
/// variable. Baud sets how *long* the residual takes to drain, but whether a residual
/// survives at all is the kernel's last-close policy, and doctor P13 measures the two
/// answers that exist: Linux 7.0.0-29 `retains` (so no rate ever raced it there, which
/// is why the sentence was never falsified), Darwin 24.6.0 `waits-then-discards` with a
/// ~600 ms wait for a blocking slave and an unconditional 29 µs discard for an
/// `O_NONBLOCK` one. A slow rate is dangerous only in combination with a policy that
/// discards; the rate alone is not the hazard, and treating it as one led every reader
/// to reach for a faster rate instead of a live session. [`inject_verify`] now holds
/// the session open across the arrival observation, so this test no longer depends on
/// either variable — a slow rate here would cost wall clock, not bytes.
#[test]
fn crossover_rig_custom_baud_byte_exact() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("crossover_rig_custom_baud_byte_exact");
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
        skip_no_rig("crossover_rig_signal_verbs");
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
        skip_no_rig("crossover_rig_map_node_both_directions");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (map node): {p0} <-> {p1}");
    // This test builds its own graph rather than going through `boot_rig`, so it has
    // to ask for the priming itself — it carries bulk data and was one of the three
    // observed losing the post-replug packet (notes §3.70).
    prime_the_wire_once(&p0, &p1);

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

// ---------------------------------------------------------------------------
// Hardware flow control, end to end (plan §18 item 7, design §5, §15.52)
// ---------------------------------------------------------------------------

/// The rig config with a `flow_control` chosen per port.
///
/// Separate from [`null_modem_cfg`] rather than a parameter on it, because the two
/// tests below need the two ports to differ: the port under test runs `rts-cts` so
/// its kernel owns the handshake, and its peer runs `none` so **the test** owns
/// RTS and can drive the far end's CTS deliberately. A rig with `rts-cts` on both
/// sides cannot be stalled from a test at all — §5 has the daemon reading its port
/// continuously and never idling, so the receiver's buffer never fills and its
/// kernel never has a reason to drop RTS.
fn flow_cfg(
    p0: &str,
    p1: &str,
    baud: u32,
    flow: (&str, &str),
    dir: &Path,
    inj0: &Path,
    inj1: &Path,
) -> String {
    let (flow0, flow1) = flow;
    null_modem_cfg(p0, p1, baud, dir, inj0, inj1)
        .replace(
            "name = \"port0\"",
            &format!("name = \"port0\"\nflow_control = \"{flow0}\""),
        )
        .replace(
            "name = \"port1\"",
            &format!("name = \"port1\"\nflow_control = \"{flow1}\""),
        )
}

/// Drive `rts` on `node` and return the far node's `modem_lines`, once the daemon
/// has had a chance to re-read them.
///
/// The read goes through the daemon's **own state**, not through a raw ioctl the
/// test issues: design §7.1 promises "current modem-line readings" as node state,
/// and that promise is what a guard here must assert. An ioctl in the test would
/// measure the wire while leaving the product's report unproven, which is §9's
/// proxy in space (`serial_nexus_sys`'s reader is also unavailable to itest — the
/// unsafe lives in one crate).
fn drive_rts_read_far(rpc: &serial_nexus_itest::Rpc, node: &str, far: &str, rts: bool) -> Value {
    rpc.call("set-modem", json!({ "node": node, "rts": rts }))
        .unwrap_or_else(|e| panic!("set-modem rts={rts} on {node}: {e:?}"));
    // A USB adapter's control transfer and the peer's TIOCMGET are separate round
    // trips over the bus; without a pause the read races the write and a wired line
    // reads unwired. The doctor's P5_MODEM_SETTLE is the same constant for the same
    // reason.
    std::thread::sleep(Duration::from_millis(120));
    rpc.node(far).unwrap_or_else(|| panic!("{far} has state"))["modem_lines"].clone()
}

/// Whether this rig carries a **fully crossed** hardware handshake, **measured** —
/// the precondition the two tests below gate on, and the reason their skip is honest
/// rather than a declaration. Returns the measurement either way so the skip can
/// print it.
///
/// **Four cells, because the promise this gates is a two-direction one** (plan §18
/// item 74). It drove `port1 → port0` alone until this landed, while
/// `crossover_rig_rts_crosses_to_the_far_ports_cts` asserted **both** directions —
/// so a half-crossed bench, which P5 names as a real wiring state
/// (`HALF-CROSSED handshake: RTS/CTS carries one way only`), passed the precondition
/// and then reddened the promise mid-test, where every other `required`-gated
/// capability in this tree skips with its reading printed. A precondition that
/// measures less than its promise asserts is not a precondition. Both polarities in
/// each direction for the reason §15.52 gives: a line stuck high passes a
/// one-polarity test.
///
/// A half-crossed bench therefore **skips** here and, under `SNX_RIG_FLOW=required`,
/// **fails** naming that state — which is the right verdict for an operator who
/// asserted the bench is 5-wire: 3-wire is a legitimate cabling under §5's stated
/// assumption, half-crossed is a miswiring.
///
/// **Call it only on a graph where the *test* owns both RTS pins** — both ports at
/// `flow_control = "none"`. Under `rts-cts` the kernel owns that port's RTS and a
/// `set-modem` fights the line discipline for the same pin, so the reading would be
/// the discipline's answer rather than the wire's.
fn handshake_measured(rpc: &serial_nexus_itest::Rpc) -> (bool, String) {
    let measure = |near: &str, far: &str| -> (bool, String) {
        let hi = drive_rts_read_far(rpc, near, far, true);
        let lo = drive_rts_read_far(rpc, near, far, false);
        let carries = hi["cts"] == json!(true) && lo["cts"] == json!(false);
        (
            carries,
            format!(
                "{near} RTS high -> {far} cts={}, RTS low -> {far} cts={}: {}",
                hi["cts"],
                lo["cts"],
                if carries { "carries" } else { "does not carry" }
            ),
        )
    };
    // `port1 → port0` first, because that is the direction the stall test's promise
    // rides on and a reader chasing a skip reads left to right.
    let (b_to_a, b_cell) = measure("port1", "port0");
    let (a_to_b, a_cell) = measure("port0", "port1");
    // P5's own vocabulary, deliberately: an operator holding this skip line and a
    // doctor report should not have to translate between them (§15.52).
    let shape = match (b_to_a, a_to_b) {
        (true, true) => "5-wire: RTS/CTS carries both ways",
        (true, false) => "HALF-CROSSED handshake: RTS/CTS carries port1->port0 only",
        (false, true) => "HALF-CROSSED handshake: RTS/CTS carries port0->port1 only",
        (false, false) => "3-wire: no RTS/CTS handshake in either direction",
    };
    (b_to_a && a_to_b, format!("{shape} [{b_cell} | {a_cell}]"))
}

/// **RTS on one node moves CTS on the other, through the daemon's own state**
/// (plan §18 item 7, design §7.1, §15.52).
///
/// The doctor measures this continuity too, and deliberately measures a different
/// thing: P5's handshake block drives the lines port-to-port with no daemon in the
/// path, because §13's boundary is that the doctor certifies the rig and never
/// drives the daemon through it. This test is the other half — the same wire, read
/// back through `state`'s `modem_lines`, which is the field §7.1 actually promises
/// an operator. Neither subsumes the other: the doctor's arm would still pass if
/// the daemon's state reporting were broken, and this one would still pass if the
/// doctor's were.
///
/// **Both polarities, because a line stuck high passes a one-polarity test**, and
/// **both directions**, because a half-crossed handshake is a real wiring state
/// that a single direction cannot see — the same reason P5's discovery refuses to
/// call a half-crossed data pair "paired".
///
/// [`handshake_measured`] now measures those same four cells, so the loop below is
/// no longer asserting anything the precondition did not gate on (plan §18 item
/// 74). That is deliberate rather than redundant: the precondition decides
/// skip-versus-run, and the loop is the promise — a bench that goes half-crossed
/// *between* the two, a connector working loose being the obvious way, reddens here
/// instead of turning the run green.
///
/// The DTR arm is the **negative control** and it is not decoration: on this rig
/// DTR moves no DSR, DCD or RI (measured, notes §3.53 i), so a `modem_lines` that
/// returned constants, and a rig with every line bridged to every other, both fail
/// it. Without it the CTS assertions could pass on an instrument that is not
/// looking.
#[test]
fn crossover_rig_rts_crosses_to_the_far_ports_cts() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("crossover_rig_rts_crosses_to_the_far_ports_cts");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (handshake continuity): {p0} <-> {p1}");

    // `flow_control` stays at its `none` default on both ports, so the *test* owns
    // RTS. Under `rts-cts` the kernel owns it and a `set-modem` would be fighting
    // the line discipline for control of the same pin.
    let (d, _run_dir, _inj0, _inj1) = boot_rig(&p0, &p1, 115_200);
    let rpc = d.rpc();
    std::thread::sleep(Duration::from_millis(300));

    let (carries, measured) = handshake_measured(rpc);
    if !carries {
        serial_nexus_itest::skip_no_rig_flow(
            "crossover_rig_rts_crosses_to_the_far_ports_cts",
            &measured,
        );
        return;
    }
    eprintln!("handshake: {measured}");

    // Both directions, both polarities.
    for (near, far) in [("port0", "port1"), ("port1", "port0")] {
        let hi = drive_rts_read_far(rpc, near, far, true);
        assert_eq!(
            hi["cts"],
            json!(true),
            "{near} RTS high did not raise {far} CTS: {hi}"
        );
        let lo = drive_rts_read_far(rpc, near, far, false);
        assert_eq!(
            lo["cts"],
            json!(false),
            "{near} RTS low did not drop {far} CTS — a line stuck high passes a \
             one-polarity test, which is why both are driven: {lo}"
        );
    }

    // The negative control. DTR is a different pin on the same connector; on this
    // rig it is not wired through, so a reader that answers from a cache, a
    // constant, or a bridged connector fails here while passing above.
    rpc.call("set-modem", json!({ "node": "port0", "dtr": false }))
        .expect("set-modem dtr=false on port0");
    std::thread::sleep(Duration::from_millis(120));
    let before = rpc.node("port1").expect("port1 state")["modem_lines"].clone();
    rpc.call("set-modem", json!({ "node": "port0", "dtr": true }))
        .expect("set-modem dtr=true on port0");
    std::thread::sleep(Duration::from_millis(120));
    let after = rpc.node("port1").expect("port1 state")["modem_lines"].clone();
    for line in ["dsr", "dcd", "ri"] {
        assert_eq!(
            before[line], after[line],
            "driving port0 DTR moved port1 {line} ({} -> {}), which this rig is \
             measured not to do (notes §3.53 i). Either the cabling is not what \
             the record says, or `modem_lines` is not reading what it claims — \
             and the CTS assertions above cannot be trusted until that is settled",
            before[line], after[line]
        );
    }
    eprintln!("negative control: port0 DTR moved none of port1's dsr/dcd/ri");
}

/// **`flow_control = "rts-cts"` stalls the writer instead of losing bytes**
/// (plan §18 item 7, design §5's promise, §15.52).
///
/// §5 states it as a *transparency* claim: "If a port does have flow control
/// configured … the kernel pausing transmission surfaces as Busy, so hardware flow
/// control transparently extends across the graph to remote writers." The serde
/// path for the attribute has been proven since it shipped; **the wire path had
/// never been exercised by any test in this workspace**, which is the gap plan §18
/// item 7 names and §15.52's measured precondition is what finally makes gateable.
///
/// The shape, and why it is this shape. The daemon reads its own port continuously
/// and never idles (§5), so a receiver inside the graph can never fill its buffer
/// and its kernel never has a reason to drop RTS — the obvious test, "stall the
/// consumer and watch flow control engage", cannot be written against this design
/// at all. So the stall is driven from the **peer's** side: port1 runs
/// `flow_control = "none"`, which leaves its RTS pin under `set-modem`'s control,
/// and dropping it takes port0's CTS low. port0 runs `rts-cts`, so its kernel sees
/// CTS low and holds the line.
///
/// What is asserted is the pair of facts §5 promises together, since either alone
/// is satisfiable by a bug: the bytes **do not arrive** while CTS is low (the
/// stall is real, not a slow path), and they arrive **byte-exact** when it rises
/// (the stall cost latency and not data). A transmitter that dropped them, and one
/// that ignored CTS, fail different halves.
///
/// **The control arm runs every time and is the fail-first proof** (plan §3 rule
/// 10). The identical scenario with port0 at `flow_control = "none"` must deliver
/// the payload *while CTS is low* — so a green first half cannot be explained by a
/// rig that was slow, a log that was buffered, or a send that never happened.
/// **`xon-xoff` is refused at `load` on a driver that drops it** — the second mode's
/// arm of §7.1's flow-control contract, added once a dropping driver was measured
/// (§15.61, plan §18 item 67).
///
/// The structure is deliberately the same as the `rts-cts` test's opening arms, and
/// for the same reason: which promise the daemon owes depends on what the driver in
/// front of it does, so the test measures first and then asserts the promise that
/// driver is owed. All three arms ship:
///
/// * **accept-then-drop → refused at `load`, nothing created.** This is the arm the
///   rig takes on Darwin 24.6.0: `IOSerialFamily` on an FT232R returns success from
///   `tcsetattr` and reads `c_iflag` back `0x0` → `0x0` (notes §3.93).
/// * **honoured → the config loads.** `ftdi_sio` on Linux takes this arm (`0x5` →
///   `0x1405`), and so does a pts on either kernel. **This arm is what stops the
///   refusal being a rule that refuses everything** — a refusal proven only against
///   ports it refuses is not proven at all (AGENTS §9).
/// * **honest refusal → loads, and faults loudly at the node's own open**, exactly as
///   `rts-cts` does. No driver this project has measured takes it.
///
/// Both of the first two arms are reachable from one box: this rig's FT232R drops the
/// request while a pts beside it honours it, which is the discrimination §15.61 rests
/// on and the reason the entry could be written from a single-kernel session.
#[test]
fn xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (xon-xoff pre-check): {p0} <-> {p1}");

    let measured = serial_nexus_sys::honours_flow_control(
        std::path::Path::new(&p0),
        serial_nexus_sys::FlowMode::XonXoff,
    );
    let d = Daemon::start();
    let run_dir = d.run().path().to_path_buf();
    let inj0 = d.run().join("inj0");
    let inj1 = d.run().join("inj1");
    let cfg = flow_cfg(
        &p0,
        &p1,
        115_200,
        ("xon-xoff", "none"),
        &run_dir,
        &inj0,
        &inj1,
    );

    match measured {
        Ok(serial_nexus_sys::FlowOutcome::AcceptedThenDropped) => {
            eprintln!("{p0}: driver accepts xon-xoff and drops it — asserting the load refusal");
            let err = d.rpc().load_toml(&cfg, false).expect_err(
                "a driver that drops IXON/IXOFF must make this config a REFUSAL at load, \
                 not a node that faults later (§15.61)",
            );
            let msg = format!("{err:?}");
            assert!(
                msg.contains("xon-xoff") && msg.contains(&p0),
                "the refusal must name the flow control and the port: {msg}"
            );
            assert!(
                !msg.contains("rts-cts"),
                "the refusal must be about the mode the config asked for, not about the \
                 one this check used to be the only one for: {msg}"
            );
            assert!(
                d.rpc().node("port0").is_none(),
                "a refused load must create nothing: {:?}",
                d.rpc().node("port0")
            );
        }
        Ok(serial_nexus_sys::FlowOutcome::Honoured) => {
            eprintln!("{p0}: driver honours xon-xoff — asserting the config LOADS");
            d.rpc().load_toml(&cfg, false).expect(
                "a port that honours IXON/IXOFF must not be refused — a refusal rule that \
                 fires on a honouring driver refuses everything and proves nothing",
            );
            assert_ne!(
                d.rpc().node_status("port0"),
                "faulted",
                "the driver honours the mode; the node must not fault for it"
            );
        }
        Ok(serial_nexus_sys::FlowOutcome::Refused) => {
            eprintln!("{p0}: driver REFUSES xon-xoff outright — asserting the honest-refusal path");
            d.rpc().load_toml(&cfg, false).expect(
                "an honest refusal is not the defect §7.1 clause 7 refuses — the config \
                 must load and the failure must arrive loudly at the node's own open",
            );
            let node = d.rpc().node("port0").expect("the node was created");
            assert_eq!(
                d.rpc().node_status("port0"),
                "faulted",
                "an xon-xoff node on a refusing driver must fault at open rather than run \
                 silently without the flow control it asked for: {node:?}"
            );
        }
        Err(e) => {
            // Unmeasured is never a refusal (§7.1 clause 2). Printed rather than
            // silently passed, so a lane that stopped measuring is visible.
            eprintln!(
                "SKIP xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it: {p0} could not be interrogated ({e})"
            );
        }
    }
}

#[test]
fn rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes");
        return;
    };
    let _rig = rig_guard();
    eprintln!("crossover rig (rts-cts flow control): {p0} <-> {p1}");

    // **Prime the wire** (plan §18 item 76). This test builds its daemons directly
    // rather than through `boot_rig`, so it reached the primer only by accident —
    // through whichever sibling crossover test happened to run before it in the same
    // binary. Run first in a fresh binary after a replug it would lose its whole
    // **40-byte** payload to the **64-byte** leading packet a freshly re-enumerated
    // FT232R swallows (see [`prime_the_wire_once`]), and fail at the post-release
    // byte-exact assertion — whose message blames the peer's RTS. A process-wide
    // `OnceLock`, so one call here covers both arms and costs nothing when a sibling
    // already primed. The handshake probe below reaches the primer through
    // `boot_rig` as well; this call is the deliberate one, and it stays so that a
    // later edit to that probe cannot silently take the priming with it.
    prime_the_wire_once(&p0, &p1);

    // **On a driver that cannot honour RTS/CTS, the promise under test is a
    // different one, and it is asserted rather than skipped.** `serial2` verifies
    // line settings by reading them back, so a driver that accepts a `CRTSCTS`
    // request and discards it used to surface as a `faulted` node some time after
    // `load` had already returned success. The daemon now refuses such a config at
    // load (notes §3.67), so this test asserts *that* here instead — which is a
    // stricter check than a self-skip, and it is the §9 tell that the portable form
    // was found: the platform that cannot do the thing still has a promise to keep.
    //
    // **Three drivers, three promises** (§7.1 clause 7, §15.53). This branched on a
    // two-valued predicate until 2026-08-12 and so demanded the *refusal* from an
    // honestly-refusing driver too — a promise §7.1 does not make and the daemon no
    // longer keeps. Each arm asserts the promise that driver is owed:
    //
    // * accept-then-drop → refused at `load`, nothing created. Measured on Darwin
    //   24.6.0 with an FT232R on Apple's `IOSerialFamily`: `tcsetattr` succeeds and
    //   the flag reads back clear.
    // * honest refusal → **loads**, and faults loudly on the node's own open. No
    //   driver measured by this project takes this arm; it ships unexecuted, like
    //   the arm above did before a Mac ran it.
    // * honoured → the real flow-control test below. Linux + FT232R takes this one.
    let measured = serial_nexus_sys::honours_flow_control(
        std::path::Path::new(&p0),
        serial_nexus_sys::FlowMode::RtsCts,
    );
    if measured == Ok(serial_nexus_sys::FlowOutcome::Refused) {
        eprintln!("{p0}: driver REFUSES rts-cts outright — asserting the honest-refusal path");
        let d = Daemon::start();
        let run_dir = d.run().path().to_path_buf();
        let inj0 = d.run().join("inj0");
        let inj1 = d.run().join("inj1");
        // Not refused: §7.1 clause 7 says an honest refusal is not the defect, so
        // the config loads and the graph is built.
        d.rpc()
            .load_toml(
                &flow_cfg(
                    &p0,
                    &p1,
                    115_200,
                    ("rts-cts", "none"),
                    &run_dir,
                    &inj0,
                    &inj1,
                ),
                false,
            )
            .expect(
                "a driver that REFUSES CRTSCTS is honest and its config must LOAD — only \
                 accept-then-drop is refused (§7.1 clause 7)",
            );
        // ...and the failure is loud rather than silent: the node's own open carries
        // the driver's error, and the fault names the flag and the port so the
        // operator is not left with a bare errno (§7.1 clause 5).
        let node = d.rpc().node("port0").expect("the node was created");
        assert_eq!(
            d.rpc().node_status("port0"),
            "faulted",
            "an rts-cts node on a refusing driver must fault at open, not run \
             silently without the flow control it asked for: {node:?}"
        );
        let reason = format!("{node:?}");
        assert!(
            reason.contains("rts-cts") && reason.contains(&p0),
            "the fault must name the flow control and the port: {reason}"
        );
        return;
    }
    if measured == Ok(serial_nexus_sys::FlowOutcome::AcceptedThenDropped) {
        eprintln!("{p0}: driver accepts rts-cts and drops it — asserting the load refusal instead");
        let d = Daemon::start();
        let run_dir = d.run().path().to_path_buf();
        let inj0 = d.run().join("inj0");
        let inj1 = d.run().join("inj1");
        let err = d
            .rpc()
            .load_toml(
                &flow_cfg(
                    &p0,
                    &p1,
                    115_200,
                    ("rts-cts", "none"),
                    &run_dir,
                    &inj0,
                    &inj1,
                ),
                false,
            )
            .expect_err(
                "a driver that drops CRTSCTS must make this config a REFUSAL at load, not a \
                 node that faults later",
            );
        let msg = format!("{err:?}");
        assert!(
            msg.contains("rts-cts") && msg.contains(&p0),
            "the refusal must name the flow control and the port, so the operator can act \
             on it: {msg}"
        );
        // And the graph must not have been half-built on the way to failing.
        assert!(
            d.rpc().node("port0").is_none(),
            "a refused load must create nothing: {:?}",
            d.rpc().node("port0")
        );
        return;
    }

    // A payload that comfortably exceeds one FTDI bulk packet, so "nothing arrived"
    // cannot be one packet still in flight, and small enough that the whole of it
    // fits the kernel's transmit buffer while the line is held — the point is that
    // the *wire* stops, not that the daemon backs up.
    const LINE: &str = "SNX-RTSCTS-STALL-PROBE-0123456789ABCDEF";

    // How long arm 1 holds the line down and watches for bytes. **One const, read
    // by both arms**, because the window and the margin the control arm checks it
    // against are one claim and must not be able to drift apart (plan §18 item 75).
    const STALL_WINDOW: Duration = Duration::from_millis(1_500);
    // The floor the control arm holds that window to: STALL_WINDOW must be at least
    // this many times the uncontrolled latency measured on the same rig in the same
    // run, or arm 1's "nothing crossed" is satisfiable by slowness.
    const MIN_MARGIN: u32 = 4;

    // ---- The precondition: does this bench carry the handshake at all?
    //
    // **Measured on a graph where the test owns both RTS pins**, which is why it
    // happens here and not on arm 1's graph. Arm 1 runs port0 at `rts-cts`, so
    // port0's RTS belongs to the kernel's line discipline; driving it with
    // `set-modem` there would read the discipline's answer rather than the wire's,
    // and `handshake_measured` needs both directions (plan §18 item 74). This
    // daemon is dropped before arm 1 opens the ports.
    let (carries, measured) = {
        let (probe, _run_dir, _i0, _i1) = boot_rig(&p0, &p1, 115_200);
        std::thread::sleep(Duration::from_millis(300));
        handshake_measured(probe.rpc())
    };
    if !carries {
        serial_nexus_itest::skip_no_rig_flow(
            "rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes",
            &measured,
        );
        return;
    }
    eprintln!("handshake: {measured}");

    // ---- Arm 1: rts-cts on the transmitter. The line must hold.
    let (d, run_dir, _i0, _i1) = {
        let d = Daemon::start();
        let (run_dir, inj0, inj1) = {
            let run = d.run();
            (run.path().to_path_buf(), run.join("inj0"), run.join("inj1"))
        };
        d.rpc()
            .load_toml(
                &flow_cfg(
                    &p0,
                    &p1,
                    115_200,
                    ("rts-cts", "none"),
                    &run_dir,
                    &inj0,
                    &inj1,
                ),
                false,
            )
            .unwrap_or_else(|e| panic!("load rts-cts rig config: {e:?}"));
        for port in ["port0", "port1"] {
            assert!(
                d.rpc().wait_status(port, "active", Duration::from_secs(20)),
                "{port} not active under rts-cts: {:?}",
                d.rpc().node(port)
            );
        }
        (d, run_dir, inj0, inj1)
    };
    let rpc = d.rpc();
    std::thread::sleep(Duration::from_millis(300));

    let rx1 = run_dir.join("rx1.log");
    let baseline = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);

    // Hold the line: port1 drops RTS, so port0's CTS goes low.
    drive_rts_read_far(rpc, "port1", "port0", false);
    let held = rpc.node("port0").expect("port0 state")["modem_lines"].clone();
    assert_eq!(
        held["cts"],
        json!(false),
        "port0 CTS did not follow port1 RTS low, so nothing below tests flow \
         control: {held}"
    );

    let sent = rpc
        .send("port0", LINE, false, 5_000)
        .expect("send into a CTS-held port must be accepted by the daemon, not refused");
    eprintln!("sent while CTS low: {sent}");

    // The stall is real: nothing crosses while the peer holds the line.
    //
    // **The window is a ratio, and the ratio is checked** (plan §18 item 75). What
    // could produce a false green here is a rig on which delivery simply takes
    // longer than the window, so the window has to be read against this bench's own
    // uncontrolled latency rather than against a remembered number. Probed on this
    // rig when the test was written, the payload landed in the log **25 ms** after
    // `send` returned while CTS was held low, and with `rts-cts` it did not arrive
    // in **6 s** — but that figure is a record, not the assertion. The control arm
    // below **times** its delivery on every run, prints it, and asserts that
    // `STALL_WINDOW` covers it at least `MIN_MARGIN` times; a bench slow enough to
    // make this window meaningless reddens there. This comment said the control arm
    // re-established the 25 ms figure while the control arm recorded no time at all
    // and waited 5 s — the assertion was 200x looser than the sentence describing
    // it, which is AGENTS §3's "assertion strictly weaker than the comment above it
    // claims".
    std::thread::sleep(STALL_WINDOW);
    let during = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);
    assert_eq!(
        during,
        baseline,
        "{} bytes crossed the wire while the peer held RTS low — the kernel \
         transmitted through a CTS stop, so `flow_control = \"rts-cts\"` is not \
         reaching the line (design §5)",
        during - baseline
    );

    // Release it: the bytes arrive, and they arrive whole.
    drive_rts_read_far(rpc, "port1", "port0", true);
    let want = baseline + LINE.len() as u64 + 1; // `send` appends a newline
    let arrived = serial_nexus_itest::wait_until(Duration::from_secs(10), || {
        std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0) >= want
    });
    let after = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);
    assert!(
        arrived,
        "the payload never arrived after the peer raised RTS: {after} of {want} \
         bytes. A stall that loses its payload is not backpressure, it is loss \
         under another name (design §5: commands are delayed, never lost). \
         **Before believing that, check the primer**: this payload is 40 bytes and \
         a freshly re-enumerated FT232R swallows the first 64 — one bulk packet — \
         of the traffic that follows (notes §3.70). A whole payload inside one \
         swallowed packet reads exactly like the loss this message names, so a \
         `{after}` of 0 immediately after a replug is the primer's absence, not \
         the peer's RTS. `prime_the_wire_once` above exists for that; if it was \
         removed or bypassed, this is the first assertion to notice"
    );
    let body = std::fs::read(&rx1).expect("read rx1.log");
    assert!(
        body.windows(LINE.len()).any(|w| w == LINE.as_bytes()),
        "the payload arrived but not byte-exact — a stall must cost latency, not \
         data: {:?}",
        String::from_utf8_lossy(&body[baseline as usize..])
    );
    eprintln!(
        "held {} B for {STALL_WINDOW:?}, then delivered byte-exact",
        want - baseline
    );
    drop(d);

    // ---- Arm 2, the control: the same scenario with flow control OFF must NOT
    // stall. Without this, arm 1 is satisfiable by a rig that was simply slow, a
    // log that had not been flushed, or a `send` that never reached the port.
    let d2 = Daemon::start();
    let (run_dir2, inj0b, inj1b) = {
        let run = d2.run();
        (run.path().to_path_buf(), run.join("inj0"), run.join("inj1"))
    };
    d2.rpc()
        .load_toml(
            &flow_cfg(
                &p0,
                &p1,
                115_200,
                ("none", "none"),
                &run_dir2,
                &inj0b,
                &inj1b,
            ),
            false,
        )
        .unwrap_or_else(|e| panic!("load control rig config: {e:?}"));
    for port in ["port0", "port1"] {
        assert!(
            d2.rpc()
                .wait_status(port, "active", Duration::from_secs(20)),
            "{port} not active in the control arm"
        );
    }
    let rpc2 = d2.rpc();
    std::thread::sleep(Duration::from_millis(300));
    let rx1b = run_dir2.join("rx1.log");
    let base2 = std::fs::metadata(&rx1b).map(|m| m.len()).unwrap_or(0);

    drive_rts_read_far(rpc2, "port1", "port0", false);
    let want2 = base2 + LINE.len() as u64 + 1;
    // **Timed, because arm 1's window is an argument about this number** (plan §18
    // item 75). The clock starts before the `send` so the figure covers the same
    // span arm 1 sleeps through — RPC round trip, daemon write, wire, log — and is
    // read the instant the log first satisfies the condition, not after the loop.
    let started = std::time::Instant::now();
    rpc2.send("port0", LINE, false, 5_000)
        .expect("send in the control arm");
    let mut latency = None;
    let crossed = serial_nexus_itest::wait_until(Duration::from_secs(5), || {
        if std::fs::metadata(&rx1b).map(|m| m.len()).unwrap_or(0) >= want2 {
            latency = Some(started.elapsed());
            true
        } else {
            false
        }
    });
    // Put the line back before asserting, so a red here cannot leave the rig held.
    drive_rts_read_far(rpc2, "port1", "port0", true);
    assert!(
        crossed,
        "the control arm did not deliver with flow control OFF and RTS low — so \
         arm 1's stall proves nothing about `rts-cts`, and something else on this \
         rig is holding the line"
    );
    let latency = latency.expect("wait_until answered true, so the arrival was timed");
    // Printed on every run, so the figure arm 1's comment argues from is in the
    // record of the run that argued from it rather than in a sentence.
    eprintln!(
        "control: flow_control=none delivered {} B through the same CTS-low state in \
         {latency:?} (stall window {STALL_WINDOW:?}, floor {MIN_MARGIN}x)",
        want2 - base2
    );
    assert!(
        latency * MIN_MARGIN <= STALL_WINDOW,
        "the uncontrolled path took {latency:?} to cross this rig, so arm 1's \
         {STALL_WINDOW:?} window is under {MIN_MARGIN}x that latency — \"nothing \
         crossed in {STALL_WINDOW:?}\" is then satisfiable by a bench that is merely \
         slow, and arm 1's green says nothing about `rts-cts`. Widen STALL_WINDOW \
         (both arms read it) or find out why this rig delivers 40 bytes that slowly"
    );
}
