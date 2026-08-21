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
//!   (set-modem/pulse-dtr `ENOTTY` a pts);
//! * **break *reception* and a deliberate *parity* mismatch, read back through the far
//!   node's `driver_counters`** — §15.21's two remaining checklist items, plan §18 item
//!   17, in the last section of this file. They are asserted rather than observed
//!   best-effort, on Linux only: `TIOCGICOUNT` is the tree's one shipped observable for
//!   either, and off Linux `serial_nexus_sys::read_icounts` is a compile-time stub, so
//!   both self-skip there with that named as the reason.
//!
//! **Both clauses are asserted on the counter, and which surface to assert on was
//! settled by measurement rather than by argument.** This header has now been wrong
//! about the far port's *data* stream twice: first that a break "surfaces at the peer RX
//! as a NUL", then — when that was repaired — that `IGNBRK` means it surfaces as nothing
//! at all. Neither holds, and the flags are not the reason. They really are in force:
//! `serial2`'s `set_raw` sets `IGNBRK | IGNPAR`, `set_configuration` verifies the whole
//! of `c_iflag` back after applying it, and the committed artifact
//! `docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json` reads `iflag_before_hex: "0x5"`
//! — `IGNBRK` (0x1) `| IGNPAR` (0x4) — on **both** of this bench's ports (P15,
//! `software_flow_control`). And yet a break delivers a byte to the far log, and a
//! parity-mismatched payload arrives byte-exact.
//!
//! **The mechanism, which is the transferable finding:** in `ftdi_sio` the driver's
//! `TIOCGICOUNT` counter and the per-character `TTY_BREAK`/`TTY_PARITY` flag are
//! **independent**. The counter is incremented off the FTDI status byte, while the
//! character is handed up unflagged — so the line discipline never receives a flagged
//! character, and `IGNBRK`/`IGNPAR` have nothing to act on. Two citable facts force that
//! conclusion: the flags are set (the artifact above) and the characters were not
//! dropped (measured on the rig). It is a deduction from measurement, not a source
//! reading.
//!
//! So the counter is what these two tests assert and the byte stream is recorded and
//! never asserted. What a driver does with an errored character is a property this
//! project neither promises nor controls; "the far port noticed" is exactly what
//! §15.21's two checklist items ask, and `driver_counters` is the field §7.1 promises
//! it in.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Value, json};
use serial_nexus_itest::{
    Daemon, RigFlowReading, Sim, attach_slave, crossover_ports, file_len, settled_while_open,
    sha256_hex, skip_no_rig, wait_until,
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

/// A rate this rig's FT232R **refuses**: it accepts the ask with no errno and lands at
/// 9600, which is doctor P14's `adapter-refused` class (§15.51).
///
/// Measured on this box, this kernel and *these two adapters*:
/// `docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json` — every rung from 9600 to
/// 3000000 `passed` reading its own request back, and 3062500, 3125000, 3250000,
/// 3500000 and 4000000 all `adapter-refused` with `actual_baud_a`/`_b` = 9600 and
/// `refusal_errno: null` (`ftdi_sio` clamps anything past 3 Mbaud to 9600 and encodes
/// *that* back into the termios). 4000000 is chosen over 3062500 because it is the
/// rung §15.58 argues from and the one furthest from the cliff.
const RIG_REFUSED_BAUD: u32 = 4_000_000;

/// One serial node on one port — every check in the `actual_baud` test below reads
/// `state` and moves no bytes, so it needs neither the far port, nor logs, nor
/// injectors, nor [`prime_the_wire_once`]'s warm-up transfer.
fn single_port_cfg(port: &str, baud: u32) -> String {
    format!(
        r#"
[[node]]
type = "serial"
name = "port0"
device = "{port}"
baud = {baud}
arbitration = "free-for-all"
"#
    )
}

/// **`actual_baud` is the driver's answer, never the configuration echoed back** —
/// design §7.1's *Open, hold, and reopen* clause 7, decided at §15.58, built as plan
/// §18 item 41. The hardware arm of the ladder whose software arms are
/// `itest/tests/p7_actual_baud.rs`.
///
/// Two arms, in this order:
///
/// * **Refused (the subject).** At [`RIG_REFUSED_BAUD`] this adapter accepts the ask
///   with no errno and lands at 9600. `serial2` verifies its own `set_configuration`
///   by read-back with a ±2.5 % tolerance, so the daemon's open **fails** on that
///   divergence and the node never holds the port: `open` is `false`, and
///   `actual_baud` must be *present and `null`* — the field saying it does not know.
///   A `state` that answered `4000000` there would be the configuration agreeing with
///   itself about a port that is demonstrably at 9600 and not even open, which is the
///   shape clause 7 names in as many words.
/// * **Honoured (the control), on the same port, afterwards.** At 115200 the adapter
///   takes the rate and reports it back, so `state` must carry it — the field is live
///   rather than pinned at `null`. It runs *second* on purpose: it is also what rules
///   out the subject having faulted for some reason other than the rate (a leftover
///   claim, a squatter, a dead adapter all read the same in `state`), which a control
///   taken *before* the subject could not do.
///
/// **What this arm does *not* claim.** It does not show a node reporting `active` with
/// a divergent rate, because on this platform that state is unreachable: every
/// divergence this adapter can produce is larger than `serial2`'s tolerance, so it
/// becomes a failed open rather than a silently mis-clocked port (plan §18 item 41's
/// filed evidence supposed otherwise — see the item's execution record). Manufacturing
/// a *small* divergence is the unit test's job
/// (`daemon/src/nodes/serial.rs::actual_baud_follows_the_port_rather_than_the_rate_that_was_asked_for`,
/// where a master-side `TCSETS` moves the tty under an open port); what this arm adds
/// is a **real driver refusing a real rate**, which no pts can do — a pts honours
/// every rate it is handed.
///
/// **The precondition is asserted, not assumed** (AGENTS §9): if this adapter ever
/// accepts 4 Mbaud the subject arm stops measuring a refusal, so it fails and says so
/// rather than going quietly green.
///
/// Cost: the refused open is retried once a second while the node is faulted, and each
/// attempt is an open/close, i.e. a DTR toggle on an auto-reset board (§7.1 clause 6's
/// class of cost). The arm reads `state` immediately and drops the daemon, so it is a
/// handful of toggles, not a soak.
///
/// **It self-skips off Linux rather than being compiled away**, so a Mac rig operator
/// is told the test exists and why it did not run: the refusal is a measured property
/// of `ftdi_sio`, and Darwin sets the rate through `IOSSIOSPEED` on a different driver
/// with no committed measurement of which rungs it refuses. Running it there would
/// assert a prediction nobody has taken (§13: no one-way decision on single-kernel
/// evidence), and compiling it away would hide the gap instead of naming it. Staying
/// compiled on both is also what keeps the Apple cross-check looking at this code.
#[test]
fn crossover_rig_actual_baud_is_a_read_back_not_an_echo() {
    let Some((p0, _p1)) = crossover_ports() else {
        skip_no_rig("crossover_rig_actual_baud_is_a_read_back_not_an_echo");
        return;
    };
    if !cfg!(target_os = "linux") {
        eprintln!(
            "SKIP crossover_rig_actual_baud_is_a_read_back_not_an_echo: the refused \
             rung this arm needs ({RIG_REFUSED_BAUD} → 9600, `adapter-refused`) is \
             measured for `ftdi_sio` on Linux only. Darwin drives the same adapter \
             through IOSerialFamily/`IOSSIOSPEED`, and which rungs it refuses there is \
             not measured — take it with `serial-nexus-doctor --port A --port B` (P14) \
             and file the Darwin arm rather than assuming this one carries over."
        );
        return;
    }
    let _rig = rig_guard();
    eprintln!("crossover rig (actual_baud): {p0}");

    // ---- The subject, first: a rate the adapter refuses. ----
    //
    // It runs *before* the control deliberately. The failure mode this arm has to be
    // defended against is passing for the wrong reason — a port that faults because
    // something else is holding it (a leftover `TIOCEXCL`, a squatter, a dead adapter)
    // would produce the same `faulted` + `open: false` + `actual_baud: null` reading
    // as a refused rate. The control below opens the *same* port seconds later, in the
    // same test, under the same rig mutex, and requires `active`: so if the port had
    // been unopenable, the control reddens and the subject's verdict is not believed.
    // Running the control first would prove only that the port *was* free earlier.
    let node = {
        let d = Daemon::start();
        let rpc = d.rpc();
        rpc.load_toml(&single_port_cfg(&p0, RIG_REFUSED_BAUD), false)
            .unwrap_or_else(|e| {
                panic!(
                    "load one rig port @ {RIG_REFUSED_BAUD}: {e:?} — a rate the driver \
                     refuses must still LOAD (§15.8: an open failure faults the node, \
                     it does not fail the load), so this is a different defect"
                )
            });
        // `SerialNode::create` opens synchronously, so the verdict is already in
        // `state` when `load` returns; the wait is slack for a slow box, not a race.
        wait_until(Duration::from_secs(5), || {
            rpc.node_status("port0") != "active"
        });
        rpc.node("port0").expect("port0 in state")
    };
    eprintln!(
        "rig @ {RIG_REFUSED_BAUD}: status={} reason={} baud={} actual_baud={}",
        node["status"], node["reason"], node["baud"], node["actual_baud"]
    );
    // ---- A transport on which this property cannot be asked about. ----
    //
    // The arm below separates two causes of an `active` node; there is a third, and
    // a CDC-ACM bench is it. `cdc_acm` accepted every rate P14 asked it for and
    // reported each request straight back, so `actual_baud` **is** the echo this
    // test is named against and no rung of any ladder can separate a read-back from
    // one. Measured, not inferred from a driver name: on this pair P14 took 15 of
    // the ladder's 16 body rungs — it stops at the first failure, so 12 Mbaud was
    // never asked for — plus four refinements, without a single `adapter-refused`
    // outcome, ceiling `unreliable-timed-out` at 6 Mbaud (§15.62, plan §18 item 82).
    //
    // So the property is **unmeasurable here rather than violated**, which is a
    // skip with its reading printed — the same disposition a 3-wire bench gets, and
    // for the same reason (§15.52: not wired is a valid answer). Keying the skip on
    // the *reading* rather than on `ftdi_sio`-vs-`cdc_acm` is what stops it hiding a
    // real regression: a port that still refuses the rung never reaches this arm,
    // and a `serial2` that loosened its verification would surface as an `active`
    // node whose `actual_baud` **diverged**, which falls through to the assert below
    // exactly as before.
    if node.get("status").and_then(Value::as_str) == Some("active")
        && node.get("actual_baud") == node.get("baud")
    {
        eprintln!(
            "SKIP crossover_rig_actual_baud_is_a_read_back_not_an_echo: this port \
             accepted {RIG_REFUSED_BAUD} and reported it straight back \
             (actual_baud={}), so this driver echoes the ask instead of reading the \
             rate back and no rung separates the two. If this adapter refuses some \
             rung, take it with `serial-nexus-doctor --port A --port B` (P14) and \
             point `RIG_REFUSED_BAUD` at it: {node}",
            node["actual_baud"]
        );
        return;
    }
    assert_ne!(
        node.get("status").and_then(Value::as_str),
        Some("active"),
        "the node came up ACTIVE at {RIG_REFUSED_BAUD}, which the pre-registered \
         refusal says is impossible (`adapter-refused`, read-back 9600, \
         docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json). The arm below would \
         then assert `null` for a port that never diverged, i.e. pass while measuring \
         nothing (§9). **`actual_baud` in the dump above says which of the two causes \
         this is**, and they want opposite repairs: {RIG_REFUSED_BAUD} means the \
         adapter really honours the rate now — re-measure with `serial-nexus-doctor \
         --port … --port …` and pick a rung it still refuses; 9600 means the *open* \
         stopped refusing (`serial2` loosened its post-set verification, which today \
         is a ±2.5 % read-back compare), and then this is the state §15.58 describes \
         and the arm should be rewritten to assert `actual_baud == 9600` on an \
         **active** node — a strictly better test than this one: {node}"
    );
    // `faulted` rather than `waiting`: the open failed for a reason that is not
    // `NotFound`, which is what §15.8 separates. `waiting` here would mean the device
    // vanished mid-test (a pulled cable), not that the rate was refused. The *reason*
    // string is printed above and deliberately not asserted — matching a driver's
    // error text is not a contract (§7.1's flow-control clause 5 says so for the
    // sibling case).
    assert_eq!(
        node.get("status").and_then(Value::as_str),
        Some("faulted"),
        "a refused rate must fault the node: {node}"
    );
    assert_eq!(
        node.get("open"),
        Some(&Value::Bool(false)),
        "a node whose open was refused still reports a port: {node}"
    );
    assert_eq!(
        node.get("baud").and_then(Value::as_u64),
        Some(u64::from(RIG_REFUSED_BAUD)),
        "the configured rate must still be reported — the pair is the point: {node}"
    );
    assert_eq!(
        node.get("actual_baud").cloned(),
        Some(Value::Null),
        "`actual_baud` reported a rate for a port the daemon does not hold, on an \
         adapter measured to land at 9600 when asked for {RIG_REFUSED_BAUD}. Echoing \
         the ask is the one thing §7.1 clause 7 forbids, and a MISSING key is the same \
         claim made by omission (`Value::get`, not `[...]`, is what tells those apart): \
         {node}"
    );

    // ---- The control: a rate the adapter honours, on the same port. ----
    //
    // Two jobs. It shows the field is *live* rather than pinned at `null` — and it is
    // the positive control for the subject above: this port was openable in this
    // window, so the fault up there was about the rate and not about the port. The
    // previous daemon is reaped by `Daemon`'s `Drop` (kill, `wait`, then the process
    // group sweep), so its fd is closed by the kernel before this open is attempted.
    let d = Daemon::start();
    let rpc = d.rpc();
    rpc.load_toml(&single_port_cfg(&p0, 115_200), false)
        .unwrap_or_else(|e| panic!("load one rig port @ 115200: {e:?}"));
    assert!(
        rpc.wait_status("port0", "active", Duration::from_secs(20)),
        "port0 did not come up at 115200 right after the refused rate. Either this \
         port is unusable — in which case the refusal above proved nothing about the \
         RATE — or the refused open left something behind: {:?}",
        rpc.node("port0")
    );
    let node = rpc.node("port0").expect("port0 in state");
    assert_eq!(
        node.get("actual_baud").cloned(),
        Some(Value::from(115_200)),
        "a real UART running at a rate it honours reported no rate, or the wrong one, \
         in `state`: {node}"
    );
}

/// The serial signal verbs driven end to end through the daemon against a real UART —
/// send-break, set-modem (DTR/RTS high then low), pulse-dtr. On a real port these ioctls
/// succeed (a pts `ENOTTY`s set-modem/pulse-dtr, which is why `p7_signals` cannot cover
/// this).
///
/// **Break *reception* is asserted elsewhere and this test's far-end read stays
/// informational** — see [`a_break_on_one_port_is_counted_at_the_far_ports_driver`]
/// (plan §18 item 17). Two successive justifications for the byte delta printed below
/// have been measurably false: "a break surfaces at the peer RX as a NUL" (it is not a
/// NUL this tree ever promises), and then "`IGNBRK` drops it, so this reads 0 B". The
/// measured reading is **one byte per break**, with `IGNBRK` set and verified — see the
/// module header for the artifact and for why the counter and the per-character flag are
/// independent in this driver. It stays a print rather than becoming an assertion for
/// the reason that finding gives: the byte is a driver artifact, and neither its
/// presence nor its count is anything serial_nexus undertakes to deliver.
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

    // Informational, never asserted (see this test's doc comment): the measured reading
    // is one byte per break, delivered despite `IGNBRK` because this driver's counter
    // and its per-character flag are independent. The far-end *assertion* lives in
    // `a_break_on_one_port_is_counted_at_the_far_ports_driver`, on the counter.
    std::thread::sleep(Duration::from_millis(200));
    let after = std::fs::metadata(&rx1).map(|m| m.len()).unwrap_or(0);
    let delta = std::fs::read(&rx1)
        .map(|b| b[before as usize..].to_vec())
        .unwrap_or_default();
    eprintln!(
        "signal verbs on real port0 all returned Ok — far-end rx1 delta = {} B {:?} \
         (informational; a break is counted AND delivers a character on this driver, so \
         a small nonzero reading here is ordinary and no reading is a verdict)",
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
/// asserted the bench is 5-wire: half-crossed is a miswiring, and it is the one
/// reading whose remedy is an instruction to re-cable.
///
/// **The other negative reading is not a cabling verdict, and this doc used to say
/// it was** (design §15.69 clause 1, plan §18 item 92). It offered two states —
/// "3-wire is a legitimate cabling under §5's stated assumption, half-crossed is a
/// miswiring" — and the all-`false` reading is not either of them, because it is not
/// a reading about a cable at all. A bare CTS input and a transport manufacturing
/// the bit low produce the same two bits; the sentence [`handshake_shape`] returns
/// for that case names both possibilities and says it cannot choose, and
/// [`RigFlowReading`] carries the distinction to the one place that acts on it.
/// §5's stated 3-wire assumption still holds and such a bench still skips as a
/// legitimate rig — what changed is that the tool no longer claims to know which
/// rig it is.
///
/// **Call it only on a graph where the *test* owns both RTS pins** — both ports at
/// `flow_control = "none"`. Under `rts-cts` the kernel owns that port's RTS and a
/// `set-modem` fights the line discipline for the same pin, so the reading would be
/// the discipline's answer rather than the wire's.
fn handshake_measured(rpc: &serial_nexus_itest::Rpc) -> (RigFlowReading, String) {
    let measure = |near: &str, far: &str| -> Direction {
        let hi = drive_rts_read_far(rpc, near, far, true);
        let lo = drive_rts_read_far(rpc, near, far, false);
        let word = cell_word(&hi["cts"], &lo["cts"]);
        Direction {
            word,
            cell: format!(
                "{near} RTS high -> {far} cts={}, RTS low -> {far} cts={}: {word}",
                hi["cts"], lo["cts"],
            ),
        }
    };
    // `port1 → port0` first, because that is the direction the stall test's promise
    // rides on and a reader chasing a skip reads left to right. Which slot each
    // measurement lands in is checked by [`handshake_line`] rather than trusted:
    // transposing these two arguments names the wrong wire to an operator debugging
    // a miswiring, and it is invisible in every reading but the half-crossed pair.
    handshake_line(&HandshakeDirections {
        port1_to_port0: measure("port1", "port0"),
        port0_to_port1: measure("port0", "port1"),
    })
}

/// One direction's measurement: the [`cell_word`], and the operator-facing cell text
/// that opens by naming the direction it was taken in.
///
/// A struct rather than a `(word, cell)` tuple for the reason the doctor's
/// `HandshakeCells` is one: this pair is passed twice, once per direction, and a
/// transposition is exactly what a positional call hides.
struct Direction {
    word: &'static str,
    cell: String,
}

/// Both directions, in named fields.
///
/// The doctor's `HandshakeCells` is a struct for this exact reason — "the fold is
/// the place a transposed argument would be invisible" — and this fold is the same
/// place one field over: the argument transposed here is not the pair but the ports
/// handed to one `measure` call. Named fields do not make that impossible, they make
/// it *legible*; [`handshake_line`] is what makes it impossible, by checking each
/// cell's own text against the slot it arrived in.
struct HandshakeDirections {
    port1_to_port0: Direction,
    port0_to_port1: Direction,
}

/// One modem line's **cell word**, from its readings at the peer's two drive levels
/// — P5's vocabulary, spelled here so an operator holding a skip line and a doctor
/// report never has to translate between them (§15.52).
///
/// **Five words, not two.** `crosses()` in `doctor/src/probes.rs` reports `true` /
/// `false` / `stuck-high` / `inverted` / `?` from exactly this pair of polarities,
/// and [`handshake_measured`] used to reduce them to carries/does-not-carry — so the
/// promise above was false in the one case where translating matters. A Linux
/// `cdc_acm` bench reads `stuck-high` in both directions, and both instruments
/// called a physically 5-wire rig 3-wire (§15.62, plan §18 item 80).
///
/// **`?` included.** `read_modem_lines` returns `null` for the whole object when
/// `TIOCMGET` fails (`daemon/src/nodes/serial.rs`), and an earlier spelling compared
/// `== json!(true)`, which collapses `null` into `false`. That made a failed *high*
/// reading indistinguishable from a measured low one, so a port whose modem reads
/// were erroring could report `true` — a precondition claiming a crossing it never
/// saw.
///
/// **What `false` does not mean** (plan §18 item 92). `false` is "low at both of the
/// peer's drive levels", and that is the whole of it. It is what a **bare** input
/// reads — `docs/doctor/linux-7.0-2026-08-13-8c00078-dirty-tier3.json` is a genuine
/// 3-wire FT232R bench and answers `false` in all eight cells, CTS included — and it
/// is bit-identical to what a transport that **manufactures** the bit low reads.
/// Nothing downstream of this word may therefore claim a conductor is absent.
fn cell_word(hi: &Value, lo: &Value) -> &'static str {
    match (hi.as_bool(), lo.as_bool()) {
        (Some(true), Some(false)) => "true",
        (Some(false), Some(false)) => "false",
        (Some(true), Some(true)) => "stuck-high",
        (Some(false), Some(true)) => "inverted",
        _ => "?",
    }
}

/// The fold from the two RTS→CTS [`cell_word`]s to *([`RigFlowReading`], shape
/// sentence)*.
///
/// **Split out of [`handshake_measured`] so the sentence has a bench-free seam**
/// (plan §18 item 92). Everything above this line needs two adapters and a cable;
/// this is a pure function of two words, so the property item 92 is about — what the
/// all-`false` reading is allowed to claim — is guardable on a box with no rig, on
/// both instruments, which is what that item's validation asks for.
///
/// **The all-`false` arm states what was observed plus the ambiguity, instead of a
/// cabling fact.** §15.62's earlier repair was reading-keyed: `stuck-high`,
/// `inverted` and `?` fold to UNREADABLE because a line that stays high with the peer
/// closed is doing something no wire can do, so the cell announces its own failure.
/// A constant **low** announces nothing, and that is measured rather than argued:
///
/// * `docs/doctor/linux-7.0-2026-08-13-8c00078-dirty-tier3.json` — a genuine 3-wire
///   FT232R bench — reads `false` in all eight cells, so a bare FT232R CTS input
///   reads constant low;
/// * `docs/doctor/macos-24.6.0-2026-08-21-3a39896-ftdi5w-tier3.json` — a **5-wire**
///   FT232R crossover — reads `false` on all six of its *unconnected* DTR-family
///   inputs at both drive levels, on a bench whose RTS/CTS pair reads `true` both
///   ways in the same capture;
/// * Apple's CDC-ACM stack manufactures CTS low, so the same box, session and cable
///   printed a 3-wire cabling sentence one fixture earlier
///   (`docs/doctor/macos-24.6.0-2026-08-21-3a39896-tier3.json`, notes §3.117).
///
/// Item 92 left three candidate remedies and declined to choose between them on one
/// session's evidence. Those readings overturn the decline by **collapsing the
/// choice**: (b), requiring a positive control — some state in which this port's CTS
/// has ever read high — before licensing a cabling negative, fails on every
/// legitimate 3-wire bench too, because a bare input and a manufactured-low input are
/// bit-identical. So (b) licenses the legacy sentence nowhere and is (c), weakening
/// the sentence for every bench, wearing a different name. (a), keying on transport
/// capability, is what §15.62 explicitly refused ("keyed on the reading and never on
/// a driver name"). The repair is (c), reached by measurement.
///
/// **Only the word moves.** The [`RigFlowReading`] returned here is `Crossed` in the
/// same one arm the old `bool` was `true` in, so the routing through
/// [`serial_nexus_itest::skip_no_rig_flow`] is what it was: a 3-wire bench is a
/// legitimate rig that skips, `SNX_RIG_FLOW=required` turns that skip into a hard
/// failure, and a half-crossed bench still lands on the miswiring sentence item 74
/// gave it. What the enum adds over the `bool` is the one thing the `bool` could not
/// carry — *which* negative reading this is, which is what decides whether the
/// operator is told to re-cable.
fn handshake_shape(port1_to_port0: &str, port0_to_port1: &str) -> (RigFlowReading, &'static str) {
    let (b_to_a, a_to_b) = (port1_to_port0 == "true", port0_to_port1 == "true");
    // A cell that answered `stuck-high`, `inverted` or anything else non-boolean
    // said the instrument could not read the wire — never that the wire is bare.
    let inconclusive = [port1_to_port0, port0_to_port1]
        .iter()
        .any(|w| *w != "true" && *w != "false");
    match (b_to_a, a_to_b) {
        (true, true) => (RigFlowReading::Crossed, "5-wire: RTS/CTS carries both ways"),
        (true, false) => (
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port1->port0 only",
        ),
        (false, true) => (
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port0->port1 only",
        ),
        (false, false) if inconclusive => (
            RigFlowReading::NoCrossingRead,
            "UNREADABLE handshake: RTS/CTS gave no usable reading at either drive \
             level — this is not a 3-wire answer",
        ),
        (false, false) => (RigFlowReading::NoCrossingRead, NO_RTS_CTS_CROSSING_READ),
    }
}

/// The sentence [`handshake_shape`] returns when both RTS/CTS cells answered, and
/// both answered a constant low.
///
/// **The tail is a cross-instrument invariant** (design §15.69 clause 1, plan §18
/// items 92 and 56). The doctor spells the same tail in `doctor/src/probes.rs`'s
/// `HANDSHAKE_AMBIGUITY_TAIL`, and the two crates share no dependency edge — adding
/// one would be a design change — so the comparison is a source scan:
/// [`the_doctor_and_the_suite_spell_the_same_handshake_ambiguity_tail`].
///
/// **The noun in front of the tail differs from the doctor's, deliberately.** This
/// helper measures the two RTS/CTS cells and nothing else, so `no RTS/CTS crossing
/// read` is the whole of what it may claim. The doctor's corresponding arm fires
/// only when all **eight** cells — RTS/CTS and the six DTR-family crossings —
/// answered absent, so it says `no handshake crossing read`. That is not drift; do
/// not "fix" the two into agreement.
///
/// **Reviewer's obligation, recorded here because no test can carry it.** The
/// property §15.69 clause 1 binds is a *meaning*: this sentence may not assert a
/// cabling fact. Its guard is a byte-pin, which catches an accidental re-word and
/// judges nothing about what the new words say. **Do not re-crimp a bench on this
/// sentence** — that was the harm.
const NO_RTS_CTS_CROSSING_READ: &str = "no RTS/CTS crossing read: 3-wire, or a transport that \
     manufactures these lines — this reading cannot separate them";

/// The operator-facing handshake line: [`handshake_shape`]'s sentence, then the two
/// cells it was read from.
///
/// **Split out because the composition is a step, and it was an unguarded one**
/// (plan §18 item 92). The item's first guard drove `handshake_shape` alone, so two
/// things it never saw were free: dropping the shape from the printed line
/// altogether, and transposing the two `measure` calls at the one call site so both
/// half-crossed sentences name the wrong port. The first is asserted by
/// [`the_composed_handshake_line_keeps_the_shape_and_both_cells`]; the second is
/// asserted **here**, in the shipped path, because a pure test cannot see a call
/// site — and the two `#[should_panic]` tests beside that one prove these
/// assertions are live, fed the exact strings a transposed call site produces.
///
/// The direction assertions are cheap and they are not decoration. Each cell text
/// opens by naming the direction it was measured in, so the slot it arrives in is
/// checkable rather than being a convention two call sites apart — and a
/// transposition is invisible in every reading but the half-crossed pair, which is
/// the one case where this tool still tells an operator which wire to move.
fn handshake_line(d: &HandshakeDirections) -> (RigFlowReading, String) {
    let HandshakeDirections {
        port1_to_port0,
        port0_to_port1,
    } = d;
    assert!(
        port1_to_port0.cell.starts_with("port1 RTS high -> port0 "),
        "the port1 -> port0 slot was handed a measurement of some other direction, \
         so a half-crossed reading would name the wrong wire: {}",
        port1_to_port0.cell
    );
    assert!(
        port0_to_port1.cell.starts_with("port0 RTS high -> port1 "),
        "the port0 -> port1 slot was handed a measurement of some other direction, \
         so a half-crossed reading would name the wrong wire: {}",
        port0_to_port1.cell
    );
    let (reading, shape) = handshake_shape(port1_to_port0.word, port0_to_port1.word);
    (
        reading,
        format!(
            "{shape} [{} | {}]",
            port1_to_port0.cell, port0_to_port1.cell
        ),
    )
}

/// **The all-`false` handshake reading names its ambiguity instead of asserting a
/// cable** (plan §18 item 92, design §15.69 clause 1).
///
/// Pure, and deliberately so: this is the half of item 92 that no rig can prove. A
/// bench that *has* a handshake never reaches the arm under test, and a bench that
/// does not cannot tell the two readings apart — which is the finding. So the guard
/// drives the shipped fold from the raw cell readings a bare pin and a
/// manufactured-low pin **both** produce, and pins what the sentence says.
///
/// **What this test is: a byte-pin of all five sentences, and it is worth being
/// plain about what that buys.** It catches an accidental re-word: any edit to any
/// arm reddens here, so the operator-facing wording cannot drift by inattention.
///
/// **It does not judge meaning, and no assertion in this file could.** "Does not
/// assert a cabling fact" is semantic. The first spelling of this guard checked
/// `!shape.starts_with("3-wire")` and three `contains` substrings and presented that
/// as the judgement; an adversarial pass measured what it was worth, and *"3wire
/// bench: definitely 3-wire, and NOT a transport that manufactures these lines…"*
/// passed every one of them. So the meaning is a **review obligation**, recorded
/// where the words are: [`NO_RTS_CTS_CROSSING_READ`].
///
/// **All five arms are pinned, not one.** The earlier spelling claimed "the other
/// four arms are asserted unchanged in the same breath" and pinned exactly one of
/// them: gutting either half-crossed sentence to `HALF-CROSSED handshake:`, gutting
/// UNREADABLE's middle, and — the one that matters — **swapping the two
/// half-crossed direction strings**, so the tool names the wrong wire to an operator
/// debugging a miswiring, all measured green. A half-crossed bench is the single
/// case where this tool still issues a re-cabling instruction, which makes naming
/// the right wire part of the promise.
#[test]
fn the_all_false_handshake_reading_does_not_assert_a_cable() {
    // From the readings, not from the word: `false` here is exactly what a bare CTS
    // input and a manufactured-low one both put on the wire, and the chain from bits
    // to sentence is the thing under guard. `cell_word`'s other four rows are pinned
    // by `the_cell_word_maps_each_pair_of_readings_to_its_own_word`.
    let bare_or_manufactured = cell_word(&json!(false), &json!(false));
    assert_eq!(
        bare_or_manufactured, "false",
        "low at both of the peer's drive levels is the `false` cell — if this word \
         moved, the arm below is no longer the one a 3-wire bench takes"
    );

    // Spelled out here rather than built from [`NO_RTS_CTS_CROSSING_READ`]: a pin
    // that quotes the constant it is pinning moves with it and asserts nothing
    // (AGENTS §3's first register). The shared half of the wording is compared
    // against the doctor's copy in
    // `the_doctor_and_the_suite_spell_the_same_handshake_ambiguity_tail`.
    let table = [
        (
            ("true", "true"),
            RigFlowReading::Crossed,
            "5-wire: RTS/CTS carries both ways",
        ),
        (
            ("true", "false"),
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port1->port0 only",
        ),
        (
            ("false", "true"),
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port0->port1 only",
        ),
        // A crossing beside an unreadable direction is still half-crossed: the
        // carrying direction is measured and says so.
        (
            ("true", "stuck-high"),
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port1->port0 only",
        ),
        (
            ("?", "true"),
            RigFlowReading::HalfCrossed,
            "HALF-CROSSED handshake: RTS/CTS carries port0->port1 only",
        ),
        // A line stuck high with the peer closed is doing something no wire can do,
        // so §15.62's reading-keyed arm must claim it rather than the weakened
        // sentence below.
        (
            ("stuck-high", "stuck-high"),
            RigFlowReading::NoCrossingRead,
            "UNREADABLE handshake: RTS/CTS gave no usable reading at either drive \
             level — this is not a 3-wire answer",
        ),
        // A failed TIOCMGET is an unreadable instrument, not a bare wire.
        (
            ("?", "false"),
            RigFlowReading::NoCrossingRead,
            "UNREADABLE handshake: RTS/CTS gave no usable reading at either drive \
             level — this is not a 3-wire answer",
        ),
        (
            ("false", "inverted"),
            RigFlowReading::NoCrossingRead,
            "UNREADABLE handshake: RTS/CTS gave no usable reading at either drive \
             level — this is not a 3-wire answer",
        ),
        (
            (bare_or_manufactured, bare_or_manufactured),
            RigFlowReading::NoCrossingRead,
            "no RTS/CTS crossing read: 3-wire, or a transport that manufactures \
             these lines — this reading cannot separate them",
        ),
    ];

    let mut arms = std::collections::BTreeSet::new();
    for ((b_to_a, a_to_b), reading, sentence) in table {
        let got = handshake_shape(b_to_a, a_to_b);
        arms.insert(got.1);
        assert_eq!(
            got,
            (reading, sentence),
            "({b_to_a}, {a_to_b}) did not read as the agreed sentence. This is a \
             re-word check, not a meaning check: if you changed it on purpose, read \
             NO_RTS_CTS_CROSSING_READ's doc and satisfy yourself the new words still \
             refuse to state a cable (design §15.69 clause 1)"
        );
        // The routing, which is the whole of skip-versus-run: exactly the fully
        // crossed reading runs the promise. Not implied by the pin above — the pin
        // takes whatever `reading` the table names.
        assert_eq!(
            got.0 == RigFlowReading::Crossed,
            (b_to_a, a_to_b) == ("true", "true"),
            "({b_to_a}, {a_to_b}) changed the skip-versus-run routing"
        );
    }
    // **The table reached every arm.** Not implied by the pins: they take whichever
    // arm the table names, so a table that lost its UNREADABLE rows would pass every
    // one of them and prove four arms while claiming five.
    assert_eq!(
        arms.len(),
        5,
        "the table did not reach all five shape arms: {arms:?}"
    );
}

/// **[`cell_word`] driven from bits** — the stage below [`handshake_shape`], and the
/// one item 92's evidence actually lives at (plan §18 item 92).
///
/// The `(false, false) -> "false"` row is the one the item rests on: a bare CTS input
/// and a CTS input a driver manufactures low arrive here as the same two readings and
/// leave as the same word, so nothing downstream can separate them and nothing
/// downstream may claim to. `null` is the fifth row and not a sixth spelling of
/// `false`: `read_modem_lines` answers `null` for the whole object when `TIOCMGET`
/// fails, and an earlier version of this helper compared `== json!(true)`, which
/// collapsed a failed read into a measured low.
#[test]
fn the_cell_word_maps_each_pair_of_readings_to_its_own_word() {
    let word = |hi: Value, lo: Value| cell_word(&hi, &lo);
    assert_eq!(
        word(json!(true), json!(false)),
        "true",
        "a line that followed both of the peer's drive levels"
    );
    assert_eq!(
        word(json!(false), json!(false)),
        "false",
        "**the row item 92 rests on**: low at both drive levels, which a bare \
         conductor and a transport manufacturing the level produce alike"
    );
    assert_eq!(
        word(json!(true), json!(true)),
        "stuck-high",
        "a line high with the peer driven low is doing something no wire can do"
    );
    assert_eq!(
        word(json!(false), json!(true)),
        "inverted",
        "a line that followed both levels backwards is not a crossing"
    );
    assert_eq!(
        word(Value::Null, json!(false)),
        "?",
        "a failed TIOCMGET is an instrument failure, not a measured low"
    );
    assert_eq!(
        word(json!(true), Value::Null),
        "?",
        "a failed TIOCMGET is an instrument failure in either position"
    );
}

/// **The composed operator-facing line carries the shape and both cells, in the
/// slots they were measured in** (plan §18 item 92).
///
/// Item 92's first guard drove [`handshake_shape`] and stopped there, so the step
/// that builds the line an operator actually reads had none. Measured green against
/// that guard: printing `[{b_cell} | {a_cell}]` with the shape deleted outright.
#[test]
fn the_composed_handshake_line_keeps_the_shape_and_both_cells() {
    let (reading, line) = handshake_line(&HandshakeDirections {
        port1_to_port0: Direction {
            word: "true",
            cell: "port1 RTS high -> port0 cts=true, RTS low -> port0 cts=false: true".to_owned(),
        },
        port0_to_port1: Direction {
            word: "false",
            cell: "port0 RTS high -> port1 cts=false, RTS low -> port1 cts=false: false".to_owned(),
        },
    });
    assert_eq!(reading, RigFlowReading::HalfCrossed);
    assert_eq!(
        line,
        "HALF-CROSSED handshake: RTS/CTS carries port1->port0 only \
         [port1 RTS high -> port0 cts=true, RTS low -> port0 cts=false: true \
         | port0 RTS high -> port1 cts=false, RTS low -> port1 cts=false: false]",
        "the composed line dropped the shape, a cell, or the order they are read in"
    );
}

/// The `port1 -> port0` slot refuses a measurement of the other direction — the
/// transposed call site, caught in the shipped path (plan §18 item 92).
#[test]
#[should_panic(expected = "the port1 -> port0 slot was handed a measurement of some other")]
fn a_transposed_port1_to_port0_measurement_is_refused() {
    let _ = handshake_line(&HandshakeDirections {
        port1_to_port0: Direction {
            word: "true",
            cell: "port0 RTS high -> port1 cts=true, RTS low -> port1 cts=false: true".to_owned(),
        },
        port0_to_port1: Direction {
            word: "false",
            cell: "port1 RTS high -> port0 cts=false, RTS low -> port0 cts=false: false".to_owned(),
        },
    });
}

/// The mirror, so the check above is not satisfied by one slot alone: transposing
/// the pair trips whichever assertion reads first, and both must be live.
#[test]
#[should_panic(expected = "the port0 -> port1 slot was handed a measurement of some other")]
fn a_transposed_port0_to_port1_measurement_is_refused() {
    let _ = handshake_line(&HandshakeDirections {
        port1_to_port0: Direction {
            word: "true",
            cell: "port1 RTS high -> port0 cts=true, RTS low -> port0 cts=false: true".to_owned(),
        },
        port0_to_port1: Direction {
            word: "false",
            cell: "port1 RTS high -> port0 cts=false, RTS low -> port0 cts=false: false".to_owned(),
        },
    });
}

/// The declaration the doctor's half of the shared ambiguity tail is spelled on.
const HANDSHAKE_AMBIGUITY_TAIL_DECL: &str = "const HANDSHAKE_AMBIGUITY_TAIL: &str =";

/// The **value** of the string literal declared by the *one* occurrence of `decl` in
/// `src`, joined the way rustc joins one.
///
/// **Why this is not a `find('"')`-to-`find('"')` scan of the raw source**, which is
/// what stood here. That scan is correct only while the literal happens to fit on one
/// line. The doctor's tail sits at 91 columns inside a 100-column `max_width`: one
/// more word in the sentence, or a longer name on the constant, pushes it past
/// rustfmt's width, and a Rust literal that outgrows its line wraps on a
/// `\`-continuation. The raw scan then captures the backslash, the newline and the
/// next line's indentation as text, and the comparison below reddens with *the two
/// instruments no longer spell the same handshake ambiguity* **when nothing
/// diverged**. Measured, against the reader this replaced: wrapping
/// `HANDSHAKE_AMBIGUITY_TAIL` in `doctor/src/probes.rs` without changing a byte of its
/// value left the doctor's own tests green and reddened
/// [`the_doctor_and_the_suite_spell_the_same_handshake_ambiguity_tail`], whose
/// `right:` side was the sentence with a backslash, a newline and five spaces of
/// rustfmt indentation sitting in the middle of it.
///
/// That is worse than it looks. A gate that reddens for a reformat teaches its next
/// reader that its red means "reformat the doctor", and the sibling comment two
/// declarations up says what happens to such a gate: it is the one somebody deletes.
///
/// So this reads the literal the way the compiler does. A `\` at end of line drops the
/// newline **and every whitespace character after it** (rustc's `STRING_CONTINUE`
/// skips space, tab, CR and LF); `\\`, `\"`, `\'`, `\n`, `\t`, `\r` and `\0` take
/// their values; and the closing quote is the first *unescaped* one, so an escaped
/// quote inside the sentence no longer truncates the reading either.
///
/// **Every failure is a panic naming the file, never a silent approximation.** An
/// escape this reader does not know stops it rather than being passed through as
/// text — passing it through would compare two strings that are not the two literals
/// and report agreement, which is AGENTS §3's first register wearing a helper's
/// clothes.
fn string_literal_after(src: &str, decl: &str, whose: &str) -> String {
    // **Exactly one, or this reader is quoting whichever copy happens to sit
    // earliest in the file.** `find` takes the first textual occurrence, and a
    // doc-comment or a test fixture that spells the declaration is a textual
    // occurrence. Measured: with the real constant diverged and one earlier line
    // spelling `decl` beside the *correct* sentence, the cross-instrument
    // comparison went green, while the identical divergence without that line was
    // red — this helper's own doc promises "never a silent approximation" and that
    // was the silent approximation (plan §18 item 92, AGENTS §3's first register).
    let found = src.matches(decl).count();
    assert_eq!(
        found, 1,
        "{whose} spells `{decl}` {found} times, and this reader takes the first. \
         With more than one, it quotes whichever copy sits earliest — a doc comment \
         or a fixture — and reports agreement about a literal nobody compiles; with \
         none, the doctor's half of the shared handshake tail moved or was renamed \
         and this comparison went blind rather than red (plan §18 item 92)"
    );
    let at = src
        .find(decl)
        .expect("the count above proves there is exactly one");
    let rest = &src[at + decl.len()..];
    let open = rest
        .find('"')
        .unwrap_or_else(|| panic!("{whose}: `{decl}` is not followed by a string literal"));
    assert!(
        rest[..open].trim().is_empty(),
        "{whose}: `{decl}` has {:?} between it and the next literal, so this reader \
         would be quoting something other than the constant's own value",
        &rest[..open]
    );

    let mut chars = rest[open + 1..].chars();
    let mut out = String::new();
    loop {
        let Some(c) = chars.next() else {
            panic!("{whose}: the literal after `{decl}` never closes");
        };
        match c {
            '"' => return out,
            '\\' => {
                let Some(escape) = chars.next() else {
                    panic!("{whose}: the literal after `{decl}` ends inside an escape");
                };
                match escape {
                    // The line continuation, which is the whole point of this
                    // reader: the newline goes, and so does the indentation rustfmt
                    // put on the next line.
                    '\n' | '\r' => {
                        let tail = chars.as_str();
                        let resume = tail
                            .find(|ch: char| !matches!(ch, ' ' | '\t' | '\n' | '\r'))
                            .unwrap_or(tail.len());
                        chars = tail[resume..].chars();
                    }
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '0' => out.push('\0'),
                    other => panic!(
                        "{whose}: the literal after `{decl}` uses the escape `\\{other}`, \
                         which this reader does not know. Teach it that escape rather than \
                         letting the comparison run on text that is not the literal's value"
                    ),
                }
            }
            _ => out.push(c),
        }
    }
}

/// **[`string_literal_after`] reads a wrapped literal the way rustc does** (plan §18
/// item 92; the register is AGENTS §3's — a gate that reddens for something that did
/// not happen).
///
/// **The oracle is the compiler.** `WRAPPED_BY_RUSTC` is the same sentence written as
/// a real wrapped Rust literal, so what the middle case asserts is that reading the
/// source agrees with what rustc makes of it, rather than with what this file's author
/// believed rustc makes of it. The flat pin under it is the third leg: it says the
/// wrap changed the formatting and not the value, which is the premise of the whole
/// complaint.
///
/// The two short cases either side are the same bug's siblings, both live against the
/// reader this replaced: it truncated the declaration at the first `;`, which a `;`
/// inside the sentence reaches first, and it took an escaped `\"` for the closing
/// quote.
#[test]
fn the_ambiguity_tail_reader_reads_a_literal_the_way_rustc_does() {
    let read = |src: &str| string_literal_after(src, HANDSHAKE_AMBIGUITY_TAIL_DECL, "<fixture>");

    // One line, which is how the constant is spelled today.
    assert_eq!(
        read("const HANDSHAKE_AMBIGUITY_TAIL: &str = \"a — b\";\n"),
        "a — b"
    );

    // **The case that reddens the reader this replaced.** The fixture is source text
    // — a real backslash, a real newline, real indentation — spelled exactly as
    // rustfmt would leave `doctor/src/probes.rs` if the sentence grew one word.
    const WRAPPED_BY_RUSTC: &str = "3-wire, or a transport that manufactures these \
     lines — this reading cannot separate them";
    let wrapped_src = concat!(
        "const HANDSHAKE_AMBIGUITY_TAIL: &str =\n",
        "    \"3-wire, or a transport that manufactures these \\\n",
        "     lines — this reading cannot separate them\";\n",
    );
    assert_eq!(
        read(wrapped_src),
        WRAPPED_BY_RUSTC,
        "a wrapped literal read as something other than its value — the \
         cross-instrument comparison would now redden for a reformat"
    );
    assert_eq!(
        WRAPPED_BY_RUSTC,
        "3-wire, or a transport that manufactures these lines — this reading cannot \
         separate them",
        "the wrapped fixture is supposed to be a formatting change and nothing else; \
         if this fails, the case above is proving agreement about the wrong sentence"
    );

    // More than one continuation, so the reader is not merely tolerating the first.
    let thrice = concat!(
        "const HANDSHAKE_AMBIGUITY_TAIL: &str = \"one \\\n",
        "        two \\\n",
        "        three\";\n",
    );
    assert_eq!(read(thrice), "one two three");

    // A `;` inside the sentence, which truncated the previous reader before it ever
    // reached the closing quote — it panicked with `the literal closes`, red for a
    // divergence that had not happened.
    assert_eq!(
        read("const HANDSHAKE_AMBIGUITY_TAIL: &str = \"a; b\";"),
        "a; b"
    );
    // An escaped quote, which the previous reader took for the end of the literal and
    // silently compared the truncation.
    assert_eq!(
        read("const HANDSHAKE_AMBIGUITY_TAIL: &str = \"a \\\" b\";"),
        "a \" b"
    );
}

/// The reader stops at an escape it does not know instead of passing it through as
/// text (plan §18 item 92).
///
/// A comparison run on text that is not the literal's value can agree, and its
/// agreement would mean nothing — AGENTS §3's first register, reached through a
/// helper rather than through a gate.
#[test]
#[should_panic(expected = "which this reader does not know")]
fn the_ambiguity_tail_reader_refuses_an_escape_it_cannot_read() {
    let _ = string_literal_after(
        "const HANDSHAKE_AMBIGUITY_TAIL: &str = \"a \\u{2014} b\";",
        HANDSHAKE_AMBIGUITY_TAIL_DECL,
        "<fixture>",
    );
}

/// **The doctor and the suite spell one ambiguity tail, and nothing compared them**
/// (plan §18 item 92, design §15.69 clause 1; the rule is plan §18 item 56's — one
/// question, two implementations, which must not drift).
///
/// The doctor's all-`false` failure message *claimed* this cross-instrument property
/// while reading only the doctor's own literal, which is AGENTS §3's second register:
/// an assertion weaker than the comment above it. This is the comparison it claimed.
///
/// **Why a source scan and not a shared constant.** A shared constant needs a crate
/// both instruments depend on. `serial-nexus-doctor` depends on `core`, `rpc` and
/// `sys`; `serial-nexus-itest` depends on `sys` and, as dev-dependencies, `core`,
/// `rpc` and `codec-api`. Putting a handshake sentence into any of those three puts a
/// doctor's operator-facing wording into the daemon's own dependency graph, and the
/// only alternative — a dependency edge from one of these two crates to the other —
/// is a change to the workspace's shape, which is a design amendment rather than a
/// patch (AGENTS §5). So the tail stays spelled once per crate and the comparison
/// reads the other crate's source, exactly as `meta_names.rs` and `meta_derive.rs`
/// read files from the tree.
///
/// **The first noun in front of the tail differs and must**: the doctor's arm fires
/// only when all eight cells answered absent (`no handshake crossing read`), while
/// this file measures the two RTS/CTS cells and nothing else. Only the tail is the
/// invariant, so only the tail is compared.
#[test]
fn the_doctor_and_the_suite_spell_the_same_handshake_ambiguity_tail() {
    // This file is `<root>/itest/tests/serial_hardware.rs`.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("itest/ has a parent");
    let probes = root.join("doctor/src/probes.rs");
    let src = std::fs::read_to_string(&probes)
        .unwrap_or_else(|e| panic!("read {}: {e}", probes.display()));

    // The doctor keeps its tail in one named constant, so this is a lookup of one
    // declaration rather than a match on the fold. A constant that stopped being used
    // would trip `dead_code` under the workspace's `-D warnings`, so it cannot strand
    // while the fold quietly grows its own copy.
    let doctor_tail = string_literal_after(
        &src,
        HANDSHAKE_AMBIGUITY_TAIL_DECL,
        &probes.display().to_string(),
    );

    let (_noun, suite_tail) = NO_RTS_CTS_CROSSING_READ
        .split_once(": ")
        .expect("the suite's sentence is `<what was read>: <the ambiguity>`");
    assert_eq!(
        suite_tail, doctor_tail,
        "the two instruments no longer spell the same handshake ambiguity. An \
         operator holding a doctor report beside a suite skip line reads one \
         question in two wordings, and the wording is the whole of what item 92 \
         changed (design §15.69 clause 1)"
    );
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

    let (reading, measured) = handshake_measured(rpc);
    if reading != RigFlowReading::Crossed {
        serial_nexus_itest::skip_no_rig_flow(
            "crossover_rig_rts_crosses_to_the_far_ports_cts",
            reading,
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
    let (reading, measured) = {
        let (probe, _run_dir, _i0, _i1) = boot_rig(&p0, &p1, 115_200);
        std::thread::sleep(Duration::from_millis(300));
        handshake_measured(probe.rpc())
    };
    if reading != RigFlowReading::Crossed {
        serial_nexus_itest::skip_no_rig_flow(
            "rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes",
            reading,
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

// ---------------------------------------------------------------------------
// §15.21's two remaining checklist items (plan §18 item 17)
// ---------------------------------------------------------------------------
//
// §15.21 states the shipped P5 certificate exactly: per pair it carries a rate
// ladder and one deliberate **baud** mismatch, and per port a **local** break
// assertion — "not a parity mismatch, break reception, or far-side modem-line
// signalling ... Those three are not lost: they are checklist items the
// certificate is the precondition *for*."
//
// The third of them was answered in this file by
// `crossover_rig_rts_crosses_to_the_far_ports_cts`, and §15.52 sets the boundary
// that made the suite its right home: *the doctor certifies the rig, port to port,
// with no daemon in the path; driving the daemon over the certified rig is the
// suite's job.* These two follow that precedent exactly — both drive the daemon's
// own verbs (`send-break`, `send`) and read the result back through node state
// (`driver_counters`), which is the surface §7.1 promises an operator. Neither
// adds a certificate item, and nothing here moves the doctor's `probe_set`.
//
// **Both are Linux-only, and the gate is the mechanism rather than the platform.**
// `driver_counters` is `TIOCGICOUNT`, whose binding exists on Linux alone; off it
// `serial_nexus_sys::read_icounts` is a compile-time `ENOTSUP` stub, so the daemon
// omits the block for every fd — an FT232R included — and no rig, cable or re-seat
// can change that.
//
// **And the counter is not merely the *first* observable, it is the only sound one.**
// The obvious alternative — read the far port's bytes and require them to be damaged —
// was tried and refuted on the rig: a break delivers a character and a parity-mismatched
// payload arrives byte-exact, with `IGNBRK | IGNPAR` set and verified (the module
// header carries the artifact and the counter/flag independence that explains it). So
// the byte stream answers a question about the driver's flagging, not about whether the
// far port noticed — and "the far port noticed" is the whole of what §15.21 asks and of
// what §7.1 promises. Both tests therefore assert the counter, record the bytes, and
// skip rather than pretend off Linux.

/// This node's `driver_counters` block, insisting it is there.
///
/// **Absence is a failure here, not a skip**, and the distinction is
/// `serial_nexus_sys::ICOUNTS_SUPPORTED`'s reason for existing: on Linux the ioctl
/// is real, so an `Err` from it is a *measurement* — a pts answers `ENOTTY` because
/// its driver implements nothing, and a genuine FT232R must not answer that way.
/// The callers below check `ICOUNTS_SUPPORTED` first and skip on it; reaching this
/// function means the build has the ioctl, and a missing block then says the port
/// under test is not the UART the rig variables claim.
fn driver_counters(rpc: &serial_nexus_itest::Rpc, node: &str) -> Value {
    let state = rpc
        .node(node)
        .unwrap_or_else(|| panic!("{node} has no state"));
    let counters = state["driver_counters"].clone();
    assert!(
        counters.is_object(),
        "{node} reports no driver_counters on a build whose TIOCGICOUNT is real, so \
         this port's driver does not implement it — a pts or a non-UART, not the \
         adapter this test needs: {state}"
    );
    counters
}

/// One field out of a [`driver_counters`] block.
fn icount(counters: &Value, field: &str) -> i64 {
    counters[field]
        .as_i64()
        .unwrap_or_else(|| panic!("driver_counters.{field} is not a number: {counters}"))
}

/// Announce the self-skip of a test whose only observable is `TIOCGICOUNT`, on a
/// build that has none.
///
/// It skips rather than compiling away for the reason
/// `crossover_rig_actual_baud_is_a_read_back_not_an_echo` gives: a Mac rig operator
/// is told the test exists and why it did not run, and the code stays visible to the
/// Apple cross-check. It deliberately does **not** consult `SNX_CROSSOVER=required`:
/// required mode asserts that a *rig* is attached, and no rig makes a Darwin kernel
/// grow an error-counter ioctl.
fn skip_no_driver_counters(test: &str) {
    eprintln!(
        "SKIP {test}: this build has no TIOCGICOUNT (serial_nexus_sys::ICOUNTS_SUPPORTED \
         is false), so the far port's break/parity observation has no shipped observable \
         here — the daemon omits `driver_counters` for every fd off Linux (§5, §13), and \
         the byte stream is not a substitute for it: measured on the rig, an errored \
         character is delivered rather than dropped, so the data plane answers a \
         question about the driver's flagging and not about whether the far port \
         noticed. Answering this clause on Darwin needs an observable that does not \
         exist yet, not a rig."
    );
}

/// **A break asserted on one port is observed at the far port** — §15.21's
/// break-**reception** checklist item, plan §18 item 17, driven through the daemon's
/// own `send-break` verb and read back through the far node's state.
///
/// The certificate cannot answer this and says so: `p5_certify_port` computes
/// `break_ok` as `set_break(true).is_ok() && set_break(false).is_ok()` — local ioctl
/// acceptance, one port at a time — and `p5_certify_pair` transmits a rate ladder and
/// asserts no break at all, so nothing the doctor does can raise `brk` on any kernel
/// (`docs/serial-nexus-doctor.md`). Nor does the tree's other break test:
/// `p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`
/// asserts that the line resumes *transmitting* after a straddled replace and never
/// reads a counter. So this is the first assertion in the workspace that a break
/// crossed a wire.
///
/// **Three controls, because the assertion is that a number went up** and a number
/// that goes up on its own proves nothing:
///
/// * *quiet in time* — the same two counters are read across an idle window of the
///   same order as the break, and must not move. A bench whose `brk` free-runs (a
///   floating RX line, a noisy cable) fails here rather than passing the subject;
/// * *quiet in space* — the **transmitting** port's own `brk` must not move. It is
///   the negative control §15.52's DTR arm is for the handshake test: a rig that is
///   secretly a loopback, or an adapter that echoes its own TX into its RX, raises
///   the far counter and this one together;
/// * *the mechanism* — the far counter is polled only after the verb returns, so a
///   `send-break` that is refused, parked, or answered by a node that no longer
///   holds the port cannot reach the assertion at all.
///
/// **What is deliberately recorded and not asserted, and what those records have
/// already answered.** Two numbers here are driver-internal artifacts rather than
/// promises, so both are re-measured on every run and neither is pinned:
///
/// * *the size of the counter delta.* Whether a driver counts once per break **event**
///   or once per USB packet it flags was open when this test was written, so a second,
///   ten-times-shorter break runs after the first and the two deltas are printed side
///   by side. Measured on this bench in the run that established this arm: `brk +1` for
///   a 250 ms break and `brk +1` for a 25 ms one — **one per event**. Recorded here as
///   the reading of a run, not converted into an assertion: the moment a number like
///   that becomes a guard, a driver's packetization is a serial_nexus promise;
/// * *the far end's byte delta.* Measured **1 B** per break, with `IGNBRK` set and
///   verified — see the module header for the artifact and the counter/flag
///   independence that explains it. The prediction this test shipped with said `0 B`
///   and is refuted (AGENTS §9); the print now says what was seen rather than what was
///   expected. It stays a print because a character the driver hands up unflagged is
///   not something this project undertakes to deliver or to suppress.
#[test]
fn a_break_on_one_port_is_counted_at_the_far_ports_driver() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("a_break_on_one_port_is_counted_at_the_far_ports_driver");
        return;
    };
    if !serial_nexus_sys::ICOUNTS_SUPPORTED {
        skip_no_driver_counters("a_break_on_one_port_is_counted_at_the_far_ports_driver");
        return;
    }
    let _rig = rig_guard();
    eprintln!("crossover rig (break reception): {p0} <-> {p1}");

    let (d, run_dir, _inj0, _inj1) = boot_rig(&p0, &p1, 115_200);
    let rpc = d.rpc();
    std::thread::sleep(Duration::from_millis(300));

    // ---- Control 1: quiet in time. ----
    //
    // **The idle window is at least as long as the window the subject is observed
    // over**, which is the property that makes it a control rather than a gesture: a
    // counter ticking at some background rate would have at least as many chances to
    // tick here as it does there, so "it moved during the break" cannot be
    // "the break's window was longer". `OBSERVE` bounds the subject's poll and
    // `QUIET` equals it; changing one without the other is what turns this back into
    // a gesture (AGENTS §3: an assertion weaker than the comment above it claims).
    const OBSERVE: Duration = Duration::from_secs(2);
    const QUIET: Duration = OBSERVE;
    const BREAK_MS: u64 = 250;
    let near_before = driver_counters(rpc, "port0");
    let far_before = driver_counters(rpc, "port1");
    std::thread::sleep(QUIET);
    let near_quiet = driver_counters(rpc, "port0");
    let far_quiet = driver_counters(rpc, "port1");
    for (node, before, after) in [
        ("port0", &near_before, &near_quiet),
        ("port1", &far_before, &far_quiet),
    ] {
        assert_eq!(
            icount(before, "brk"),
            icount(after, "brk"),
            "{node}'s break counter moved by itself across a {QUIET:?} idle window \
             ({before} -> {after}). Nothing below can attribute a rise to the break \
             verb on a bench whose counter free-runs — check the cabling before \
             reading the failure under this one as a product defect"
        );
    }
    eprintln!("quiet control: no brk on either port across {QUIET:?} of idle wire");

    // ---- The subject. ----
    let rx1 = run_dir.join("rx1.log");
    let bytes_before = file_len(&rx1);
    let far_base = icount(&far_quiet, "brk");

    let echo = rpc
        .send_break("port0", BREAK_MS)
        .expect("send-break on port0 must reach the real UART");
    assert_eq!(echo["break_ms"], json!(BREAK_MS), "send-break echo: {echo}");

    // Polled, not sampled: the far end learns of the break over the USB bus, so the
    // status packet carrying it arrives on the adapter's own latency timer (16 ms by
    // default on an FT232R) rather than synchronously with the verb's return. The
    // poll is bounded by `OBSERVE` and the quiet control ran for exactly that long.
    // Poll for the break on **any** of the three error counters, not on `brk`
    // alone: which one a driver moves is its own choice (see `break_counter`
    // below), and a poll that watches only `brk` burns the whole `OBSERVE` window
    // on a driver that answers on `frame` and then reports `rose=false` about a
    // break it did in fact receive.
    let rose = wait_until(OBSERVE, || {
        let c = driver_counters(rpc, "port1");
        icount(&c, "brk") > far_base
            || icount(&c, "frame") > icount(&far_quiet, "frame")
            || icount(&c, "parity") > icount(&far_quiet, "parity")
    });
    let far_after = driver_counters(rpc, "port1");
    let near_after = driver_counters(rpc, "port0");
    let brk_delta = icount(&far_after, "brk") - far_base;
    let frame_delta = icount(&far_after, "frame") - icount(&far_quiet, "frame");
    let parity_delta = icount(&far_after, "parity") - icount(&far_quiet, "parity");
    let bytes_delta = file_len(&rx1) - bytes_before;
    eprintln!(
        "break {BREAK_MS} ms on port0: far port1 brk +{brk_delta} frame +{frame_delta} \
         parity +{parity_delta}, {bytes_delta} B reached the far log (recorded, never \
         asserted: measured 1 B, delivered despite a verified IGNBRK because this \
         driver's counter and per-character flag are independent); far counters \
         {far_after}, near counters {near_after}"
    );

    // **Which counter a driver moves for a received break is the driver's choice,
    // and this test now names the one it found rather than the one it expected.**
    // The pre-registration named `brk`, on `ftdi_sio`'s precedence — break over
    // parity over framing. That precedence is **refuted**: on `cdc_acm` a 250 ms
    // break arrives as a *framing* error, brk +0 frame +1 parity +0, measured on a
    // CDC-ACM crossover 2026-08-16 (§15.62, plan §18 item 81). Break *reception* is
    // confirmed on both drivers, which is the clause §15.21 actually asks about;
    // only the counter differs, so the counter is the parameter.
    //
    // The genuinely-nothing case keeps its own panic and its own next step, because
    // "answered on a different counter" and "did not answer" are two findings.
    let break_counter = if brk_delta > 0 {
        "brk"
    } else if frame_delta > 0 {
        "frame"
    } else if parity_delta > 0 {
        "parity"
    } else {
        panic!(
            "the far port observed **nothing** from a {BREAK_MS} ms break asserted on \
             port0 (rose={rose}): brk, frame and parity all unmoved on port1, and \
             {bytes_delta} B of data. The verb returned Ok, so the daemon reached the \
             port; either the break is not being put on the wire, or this adapter does \
             not report a received break through TIOCGICOUNT at all — which would \
             answer §15.21's break-reception clause with a **no** on this hardware, \
             and is worth recording as such rather than repairing blindly. \
             far={far_after} near={near_after}"
        );
    };
    if break_counter != "brk" {
        eprintln!(
            "REFUTED and recorded rather than repaired: this driver reports a received \
             break on `{break_counter}` (brk +{brk_delta} frame +{frame_delta} parity \
             +{parity_delta}), so `ftdi_sio`'s break-over-parity-over-framing \
             precedence is not universal. Reception is confirmed either way"
        );
    }
    // **The idle-window control, re-run against the counter that actually carries
    // the signal.** Control 1 above watches `brk`; on a driver that answers on
    // `frame`, `brk` being quiet says nothing about the counter this test is now
    // reading, and a framing counter that free-runs would let `break_counter` be
    // chosen by noise and its rise attributed to the verb. The snapshots are
    // already in hand, so this costs nothing but the assertion.
    for (node, before, after) in [
        ("port0", &near_before, &near_quiet),
        ("port1", &far_before, &far_quiet),
    ] {
        assert_eq!(
            icount(before, break_counter),
            icount(after, break_counter),
            "{node}'s `{break_counter}` counter — the one this driver moves for a \
             received break — free-ran across a {QUIET:?} idle window ({before} -> \
             {after}), so the rise attributed to the break verb is not attributable \
             to it"
        );
    }

    // ---- Control 2: quiet in space. ----
    assert_eq!(
        icount(&near_after, break_counter),
        icount(&near_quiet, break_counter),
        "the **transmitting** port counted a break on its own receiver while it was \
         asserting one ({} -> {}, counter `{break_counter}`). Its RX is wired to the \
         far port's TX, which was idle, so this rig is not the crossover the \
         variables claim — a loopback, or an adapter echoing its own TX — and the \
         far-side rise above proves nothing about a wire: near={near_after}",
        icount(&near_quiet, break_counter),
        icount(&near_after, break_counter)
    );

    // ---- Recorded, never asserted: what the delta's size tracks. ----
    let short_base = icount(&far_after, break_counter);
    const SHORT_BREAK_MS: u64 = 25;
    rpc.send_break("port0", SHORT_BREAK_MS)
        .expect("the second, shorter send-break");
    wait_until(OBSERVE, || {
        icount(&driver_counters(rpc, "port1"), break_counter) > short_base
    });
    let short_delta = icount(&driver_counters(rpc, "port1"), break_counter) - short_base;
    let long_delta = if break_counter == "brk" {
        brk_delta
    } else if break_counter == "frame" {
        frame_delta
    } else {
        parity_delta
    };
    eprintln!(
        "recorded, never asserted: a {BREAK_MS} ms break moved port1's {break_counter} by \
         {long_delta} and a {SHORT_BREAK_MS} ms one by {short_delta}. Equal deltas mean \
         the driver counts one per break *event*; a delta that scales with duration \
         means it counts one per flagged USB packet. Measured 1 and 1 on this bench — \
         per event — and re-measured here every run rather than remembered, because \
         neither number is a promise this tree makes: which is why the assertion above \
         reads `> 0` and not a number"
    );

    // The port survives its own signalling — the same closing check the sibling
    // signal-verb test makes, for the same reason.
    assert!(
        rpc.wait_status("port0", "active", Duration::from_secs(3)),
        "port0 faulted after two breaks: {:?}",
        rpc.node("port0")
    );
}

/// The rig config with a `parity` chosen per port.
///
/// Separate from [`null_modem_cfg`] rather than a parameter on it for the reason
/// [`flow_cfg`] is separate: the test below needs the two ports to **differ**, and a
/// single knob on the shared builder would not express that.
fn parity_cfg(
    p0: &str,
    p1: &str,
    baud: u32,
    parity: (&str, &str),
    dir: &Path,
    inj0: &Path,
    inj1: &Path,
) -> String {
    let (par0, par1) = parity;
    null_modem_cfg(p0, p1, baud, dir, inj0, inj1)
        .replace(
            "name = \"port0\"",
            &format!("name = \"port0\"\nparity = \"{par0}\""),
        )
        .replace(
            "name = \"port1\"",
            &format!("name = \"port1\"\nparity = \"{par1}\""),
        )
}

/// **A deliberate parity mismatch across the pair is noticed by the receiver** —
/// §15.21's parity-mismatch checklist item, plan §18 item 17.
///
/// §15.21 is exact about the gap: "`p5_certify_pair` runs `Parity::None` throughout —
/// only baud is mismatched". So the certificate's one mismatch corrupts the pattern by
/// clocking it wrong, and nothing in this workspace has ever put a *parity* bit on a
/// wire, let alone mismatched one.
///
/// **Even against odd, not none against even, and the choice is the experiment.**
/// A `none` transmitter into an `even` receiver mismatches the *frame length* — 10
/// bits against 11 — so the receiver samples the stop bit as parity and the next start
/// bit as stop, and what it reports is a framing collapse that says nothing about
/// parity checking. Even and odd are complementary for every possible byte: same
/// 11-bit frame, correct start and stop, and the parity bit wrong on **every**
/// character regardless of payload. That makes the subject arm's expectation
/// content-independent rather than a property of the string chosen.
///
/// **What is asserted: the receiver *notices*.** `driver_counters.parity` must rise.
/// That is §15.21's clause in its own words — parity-error *observation* — and it is
/// the observable serial_nexus promises: §7.1 surfaces the driver's input counters
/// precisely because "framing/parity/overrun errors are otherwise invisible loss".
///
/// **What is deliberately not asserted: payload loss.** This arm shipped asserting it
/// and the rig refuted the prediction (AGENTS §9). Measured: an 8E1 transmitter into an
/// 8O1 receiver reads `parity +2` and the payload arrives **byte-exact**, 43 of 43
/// bytes. The flags are not the explanation and were checked rather than assumed —
/// `docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json` reads `iflag_before_hex:
/// "0x5"`, `IGNBRK | IGNPAR`, on both of this bench's ports (P15,
/// `software_flow_control`), and `set_configuration` verifies `c_iflag` back. In
/// `ftdi_sio` the counter and the per-character `TTY_PARITY` flag are **independent**:
/// the counter comes off the FTDI status byte and the character is handed up unflagged,
/// so `IGNPAR` never receives one to drop. A guard on payload loss would therefore pin
/// *another* driver's decision about flagging characters as if it were a serial_nexus
/// promise — AGENTS §3's "a guard pinning a decision rather than a mechanism", one
/// layer below the daemon. It is recorded on every run instead.
///
/// **The four readings stay separated**, because they are four different findings
/// wanting four different next steps, and only the counter decides pass or fail:
///
/// * **counted, payload intact — the expected reading on `ftdi_sio` / Linux 7.0.0-29**,
///   for the reason cited above. Passes;
/// * counted, payload lost — also passes, and is a *different platform*: that driver
///   does flag the characters, so `IGNPAR` drops them. Printed loudly, because it is a
///   finding about that driver and the shape this test was originally written for;
/// * not counted, payload lost — fails: the corruption is not attributable to parity,
///   and the primer or the baud is the place to look;
/// * not counted, payload intact — fails: the configured parity never reached the wire,
///   which is the product defect this guard exists for.
///
/// **The control runs second and it now carries the whole discrimination.** Since the
/// payload arrives in both arms on this hardware, the *only* thing separating a
/// mismatched pair from a matched one is the counter — so the control's job is no
/// longer mainly rig-liveness, it is to show that the counter is quiet when the parities
/// agree. A counter that moved there would mean the subject arm's rise is not measuring
/// the mismatch at all. It still runs second, for
/// `crossover_rig_actual_baud_is_a_read_back_not_an_echo`'s reason: a control taken
/// afterwards also rules out a rig that died mid-test, which a control taken first
/// cannot.
///
/// **`active` is not decoration here, it is the read-back.** `serial2`'s
/// `set_configuration` compares `c_cflag` and `c_iflag` back after applying them, so a
/// driver that accepted `PARENB`/`PARODD` and dropped them fails the open and faults
/// the node — the same shape §15.61 refuses for `xon-xoff`. A node that reaches
/// `active` under a parity setting has had that setting read back off the termios it
/// asked for.
#[test]
fn a_parity_mismatch_is_counted_at_the_far_ports_driver() {
    let Some((p0, p1)) = crossover_ports() else {
        skip_no_rig("a_parity_mismatch_is_counted_at_the_far_ports_driver");
        return;
    };
    if !serial_nexus_sys::ICOUNTS_SUPPORTED {
        skip_no_driver_counters("a_parity_mismatch_is_counted_at_the_far_ports_driver");
        return;
    }
    let _rig = rig_guard();
    eprintln!("crossover rig (parity mismatch): {p0} <-> {p1}");
    // This test builds its own daemons rather than going through `boot_rig`, so it
    // asks for the priming itself — the same reason
    // `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` does (plan §18
    // item 76): its payload is smaller than the 64-byte leading packet a freshly
    // re-enumerated FT232R swallows, and a whole payload inside one swallowed packet
    // reads exactly like the corruption this test is trying to attribute to parity.
    prime_the_wire_once(&p0, &p1);

    // No NUL and no high bytes, so a substituted or dropped character is legible in
    // the failure message. Longer than one FTDI bulk packet is not needed: the driver
    // flags a whole packet at once, so one packet is one counter increment and the
    // payload is small enough to arrive quickly when it is meant to.
    const LINE: &str = "SNX-PARITY-MISMATCH-PROBE-0123456789ABCDEF";
    let want = LINE.len() + 1; // `send` appends a newline.

    let boot = |parity: (&str, &str)| {
        let d = Daemon::start();
        let (run_dir, inj0, inj1) = {
            let run = d.run();
            (run.path().to_path_buf(), run.join("inj0"), run.join("inj1"))
        };
        d.rpc()
            .load_toml(
                &parity_cfg(&p0, &p1, 115_200, parity, &run_dir, &inj0, &inj1),
                false,
            )
            .unwrap_or_else(|e| panic!("load the rig at parity {parity:?}: {e:?}"));
        for port in ["port0", "port1"] {
            assert!(
                d.rpc().wait_status(port, "active", Duration::from_secs(20)),
                "{port} did not come up at parity {parity:?}: {:?}. `serial2` verifies \
                 its own settings by reading them back, so a driver that accepted the \
                 parity request and dropped it faults the node here — which would be a \
                 measurement about this adapter (the `xon-xoff` shape of §15.61, one \
                 attribute over) and not a fault of this test",
                d.rpc().node(port)
            );
        }
        (d, run_dir)
    };

    // ---- The subject: an 8E1 transmitter into an 8O1 receiver. ----
    let (d, run_dir) = boot(("even", "odd"));
    let rpc = d.rpc();
    std::thread::sleep(Duration::from_millis(300));
    let rx1 = run_dir.join("rx1.log");
    let before = driver_counters(rpc, "port1");
    let parity_base = icount(&before, "parity");
    let frame_base = icount(&before, "frame");

    rpc.send("port0", LINE, false, 5_000)
        .expect("send under a parity mismatch must be accepted by the daemon");
    // **The wait is on the counter, because the counter is what this arm asserts.** It
    // used to wait on "a counter moved *or* the payload landed", which on this hardware
    // is satisfied by the payload every time — so the counter would have been read
    // whenever the log happened to be flushed rather than after a bounded chance to
    // move. A wait whose condition is not the assertion's condition is how a guard ends
    // up timing something other than what it reports.
    const NOTICE_WINDOW: Duration = Duration::from_secs(5);
    wait_until(NOTICE_WINDOW, || {
        let c = driver_counters(rpc, "port1");
        icount(&c, "parity") > parity_base || icount(&c, "frame") > frame_base
    });
    // Then settle, so the payload reading below is a fair one in every arm. It is
    // recorded rather than asserted — and a record taken too early is a wrong record,
    // which is worse than no record.
    std::thread::sleep(Duration::from_millis(500));
    let after = driver_counters(rpc, "port1");
    let parity_delta = icount(&after, "parity") - parity_base;
    let frame_delta = icount(&after, "frame") - frame_base;
    let body = std::fs::read(&rx1).unwrap_or_default();
    let intact = body.windows(LINE.len()).any(|w| w == LINE.as_bytes());
    let noticed = parity_delta > 0 || frame_delta > 0;
    eprintln!(
        "8E1 -> 8O1: port1 parity +{parity_delta} frame +{frame_delta}, {} of {want} B \
         in the far log, payload intact = {intact}; far counters {after}; arrived = \
         {:?}",
        body.len(),
        String::from_utf8_lossy(&body[..body.len().min(64)])
    );
    drop(d);

    // Only the counter decides. The payload half is classified beside it because the
    // two together say *which* platform this is, and a run that cannot tell those apart
    // is a run whose green means less than it looks.
    match (noticed, intact) {
        (true, true) => eprintln!(
            "the receiver counted the mismatch (parity +{parity_delta}, frame \
             +{frame_delta}) and delivered the payload byte-exact. §15.21's clause is \
             answered COUNTED, NOT LOST. The mechanism, where it has been measured: on \
             `ftdi_sio` / Linux 7.0.0-29 the counter and the per-character TTY_PARITY \
             flag are independent, so `IGNPAR` — set, and read-back verified as iflag \
             0x5 on both ports in docs/doctor/linux-7.0-2026-08-14-b58a1c4-tier3.json, \
             P15 — never receives a flagged character to drop. **That artifact is an \
             FTDI pair's**, so it explains this arm rather than certifying it: on any \
             other adapter the same reading is this run's own measurement and the \
             mechanism behind it is unmeasured here (§15.62 — do not diff a cell across \
             transports)"
        ),
        (true, false) => eprintln!(
            "the receiver counted the mismatch (parity +{parity_delta}, frame \
             +{frame_delta}) **and** the payload did not survive — a stricter reading \
             than this bench gives, and a finding about this driver rather than about \
             the daemon: it flags the erroring characters, so the line discipline's \
             IGNPAR drops them. Passing, because what serial_nexus promises is that the \
             receiver notices. Record the platform beside the reading (§16.13) — the \
             recorded answer on ftdi_sio / Linux 7.0.0-29 is byte-exact delivery, and a \
             second answer is a comparison worth having, not a regression. Arrived: {:?}",
            String::from_utf8_lossy(&body[..body.len().min(64)])
        ),
        (false, false) => panic!(
            "the payload did not survive but **no** counter moved on the receiver \
             (parity +0, frame +0), so nothing attributes the loss to the parity \
             mismatch and the clause is unanswered either way. Look at the primer first \
             — a freshly re-enumerated FT232R eats the first 64 bytes and this payload \
             is {want} — then at the baud. Arrived: {:?}",
            String::from_utf8_lossy(&body[..body.len().min(64)])
        ),
        (false, true) => panic!(
            "an 8E1 transmitter delivered to an 8O1 receiver with **no parity or \
             framing error counted at all**. The receiver did not notice, which is the \
             one thing §15.21's clause asks and the one thing `driver_counters` exists \
             to report (§7.1). Byte-exact delivery is *not* the failure — it is this \
             platform's ordinary reading and the control arm below expects it — so do \
             not chase the payload: the configured parity never reached the wire. Both \
             nodes reached `active`, so the termios read-back agreed and the request \
             got as far as the tty and no further. That is the product defect this \
             guard exists for (the `map_parity` → line path), one attribute over from \
             §15.61's `xon-xoff` case"
        ),
    }

    // ---- The control: the same pair, parity MATCHED. ----
    //
    // **This arm now carries the discrimination, not just the liveness.** On hardware
    // that delivers the payload through a mismatch, the counter is the *only* thing
    // separating a mismatched pair from a matched one — so the counter-quiet assertions
    // below are what make the subject arm's rise mean "the mismatch", and they would
    // catch a receiver whose parity counter simply ticks whenever parity is enabled.
    // The byte-exact check stays as the rig-liveness half: an FT232R that could not
    // carry 8E1 at all would fail it, and the subject arm would then have been about
    // this adapter rather than about the mismatch.
    let (d2, run_dir2) = boot(("even", "even"));
    let rpc2 = d2.rpc();
    std::thread::sleep(Duration::from_millis(300));
    let rx1b = run_dir2.join("rx1.log");
    let control_before = driver_counters(rpc2, "port1");
    rpc2.send("port0", LINE, false, 5_000)
        .expect("send in the control arm");
    let crossed = wait_until(Duration::from_secs(10), || file_len(&rx1b) as usize >= want);
    let control_after = driver_counters(rpc2, "port1");
    let control_body = std::fs::read(&rx1b).unwrap_or_default();
    eprintln!(
        "8E1 -> 8E1 control: {} of {want} B, parity {} -> {}, frame {} -> {}",
        control_body.len(),
        icount(&control_before, "parity"),
        icount(&control_after, "parity"),
        icount(&control_before, "frame"),
        icount(&control_after, "frame")
    );
    assert!(
        crossed
            && control_body
                .windows(LINE.len())
                .any(|w| w == LINE.as_bytes()),
        "with parity **matched** at both ends the payload did not cross this rig \
         byte-exact ({} of {want} B). So this adapter does not carry 8E1 at all, and \
         the subject arm's counter rise is about the adapter rather than about the \
         mismatch. Arrived: {:?}",
        control_body.len(),
        String::from_utf8_lossy(&control_body[..control_body.len().min(64)])
    );
    assert_eq!(
        icount(&control_after, "parity"),
        icount(&control_before, "parity"),
        "the receiver counted a parity error while both ends ran the **same** parity. \
         The counter the subject arm asserts on is then not measuring the mismatch — it \
         moves whenever parity is *enabled* — and since the payload arrives in both \
         arms on this hardware, nothing at all would separate the two: {control_after}"
    );
    assert_eq!(
        icount(&control_after, "frame"),
        icount(&control_before, "frame"),
        "the receiver counted a framing error with both ends at 8E1 — the frame is \
         mis-clocked or mis-shaped independently of parity, which undermines the \
         subject arm's frame reading too: {control_after}"
    );
}
