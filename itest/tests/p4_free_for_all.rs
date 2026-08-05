//! Phase 4 arbitration opt-out (design §6), ported from
//! `scripts/validate/phase4/free-for-all.sh`.
//!
//! A `free-for-all` endpoint has **no lock**: every writer's bytes are read
//! targetward with no acquisition. Two PTY origins on one free-for-all `serial`
//! endpoint write **concurrently** and BOTH reach the device — the distinguishing
//! behavior versus the exclusive default, under which neither would reach it without
//! a lock (§6/§8 invariant 8). "Both got through" is proven by a byte-exact count at
//! the device, not a judgement.
//!
//! The original's "device" is a `serial-nexus-sim pty --sink` that counts exactly `2N`
//! bytes. That software-loopback trick is Linux-only (a pts cannot be a serial device
//! on macOS — `serial2` → `ENOTTY`), so here the serial node opens one end of a
//! cross-wired pair from [`serial_pair`] (a Linux `serial-nexus-sim nullmodem`, or real
//! crossover hardware on macOS) and a `serial-nexus-sim client --recv` on the *far* end is
//! the byte-counting sink. Self-skips when no serial device is available — the same
//! self-skip discipline the bash hardware rig used (a skip is a valid verdict, §5).
//!
//! Loss-free by construction **up to the wire, and not past it**: every hop between the
//! two PTY clients and the sink backpressures (targetward is never dropped, §5 invariant
//! 3), so no queue inside the daemon can drop. This file used to conclude from that
//! "ordering of the writers vs. the sink cannot lose bytes — only block until drained",
//! and that conclusion is **false on real hardware**: the last hop is a UART with no
//! flow control and no queue, so a byte clocked onto the wire while the far port is
//! closed is simply gone. Measured on an FT232R crossover at 115200 — a sink opening
//! 0.2 s late loses 2321 bytes, 0.5 s late loses 5800, against a wire rate of 11520
//! B/s, with the daemon's `tx` counter reading a full 32768 every time (notes §3.47).
//! The sink therefore opens **first** and the writers gate on its `--open-file`.
//!
//! The writers are spawned as *killable* children so a regression (a blocked
//! free-for-all writer) fails cleanly on the sink's bounded timeout with
//! `received != 2N` instead of hanging.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use serial_nexus_itest::{Daemon, bin, serial_pair_or_rig, skip_no_pair, wait_until};

/// Each writer sends `N`; the device must see `2N` (both writers got through).
const N: usize = 16384;
const TOTAL: usize = 2 * N;

#[test]
fn free_for_all_endpoint_lets_concurrent_writers_both_reach_device() {
    let Some(pair) = serial_pair_or_rig() else {
        skip_no_pair("free_for_all_endpoint_lets_concurrent_writers_both_reach_device");
        return;
    };
    // `dev` is the end the daemon's serial node owns; `sink` is the far, cross-wired
    // end where a client counts every byte that reached "hardware".
    let (dev, sink) = pair.ports();
    let dev = dev.to_string();
    let sink = sink.to_string();

    let d = Daemon::start();
    let rpc = d.rpc();
    let run = d.run();
    let ta = run.join("ttyA");
    let tb = run.join("ttyB");
    let ta_s = ta.to_string_lossy().into_owned();
    let tb_s = tb.to_string_lossy().into_owned();

    // free-for-all.sh's graph, verbatim: two PTY origins on one free-for-all serial
    // endpoint. Baud stays at the default 115200 (as the bash left it); the sink pins
    // the same rate via --set-baud so the real-hardware crossover path matches, while
    // on the Linux nullmodem baud is cosmetic.
    let cfg = format!(
        r#"
[[node]]
type = "pty"
name = "ptya"
path = "{ta}"
[[node]]
type = "pty"
name = "ptyb"
path = "{tb}"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "{dev}"
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
"#,
        ta = ta.display(),
        tb = tb.display(),
        dev = dev,
    );
    rpc.load_toml(&cfg, false)
        .expect("load free-for-all config");

    // Bounded readiness: the port must open and both PTYs go live before bytes flow
    // (the bash relied on the sim client's own device-wait).
    assert!(
        rpc.wait_status("usb0", "active", Duration::from_secs(20)),
        "usb0 (serial) never became active: {:?}",
        rpc.node("usb0")
    );
    assert!(
        rpc.wait_status("ptya", "active", Duration::from_secs(10)),
        "ptya never became active: {:?}",
        rpc.node("ptya")
    );
    assert!(
        rpc.wait_status("ptyb", "active", Duration::from_secs(10)),
        "ptyb never became active: {:?}",
        rpc.node("ptyb")
    );

    // State reports usb0's endpoint as free-for-all with NO holder — no acquisition is
    // needed, so every origin already `may_write` (the bash `.lock.arbitration ==
    // "free-for-all" and .lock.holder == null` assertion). Bounded wait tolerates the
    // just-finished `load` settling.
    let lock_ok = wait_until(Duration::from_secs(5), || {
        rpc.node("usb0")
            .and_then(|n| n.get("lock").cloned())
            .map(|l| {
                l.get("arbitration").and_then(Value::as_str) == Some("free-for-all")
                    && l.get("holder").map(Value::is_null).unwrap_or(true)
            })
            .unwrap_or(false)
    });
    assert!(
        lock_ok,
        "usb0 endpoint not reported free-for-all / holderless: {:?}",
        rpc.node("usb0")
    );

    // **The sink opens FIRST, and the writers wait for it.** On the sim null modem the
    // ordering is invisible — a pts buffers whether or not a reader is attached — but
    // on a real UART it is the whole test: a USB-serial port that is not open does not
    // receive, so every byte that crosses the wire before the far end opens is gone,
    // and the daemon's `tx` counter still says it sent them. Measured on an FT232R
    // crossover at 115200 with the old ordering: a sink opening 0.2 s late lost 2321
    // bytes and one opening 0.5 s late lost 5800, against a wire rate of 11520 B/s —
    // the loss is exactly the airtime, linear in the delay, with `overrun`, `frame`
    // and `buf_overrun` all zero. Even spawning them together lost 21-31 bytes to
    // process start-up. With this gate: 0 bytes lost, and the driver's own rx counter
    // matches tx exactly (notes §3.47).
    //
    // `--open-file` rather than `--ready-file`: the latter fires on the first byte
    // *read*, which cannot be signalled before any byte is sent — the chicken and egg
    // this ordering creates. Open is the earlier and, here, the load-bearing fact.
    let total_s = TOTAL.to_string();
    let opened = run.join("sink.open");
    let sink_child = Command::new(bin("serial-nexus-sim"))
        .arg("client")
        .args([
            "--path",
            &sink,
            "--recv",
            &total_s,
            "--set-baud",
            "115200",
            "--open-file",
            &opened.to_string_lossy(),
            "--timeout-ms",
            "30000",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the sink");
    assert!(
        wait_until(Duration::from_secs(10), || opened.exists()),
        "the sink never reported its port open, so the writers below would have raced it"
    );

    // Both clients write concurrently; with no lock, BOTH streams are read targetward.
    // Spawned as children (not `Sim::client`, which blocks) so they run alongside the
    // sink and can be killed if a regression leaves one blocked.
    let mut writer_a = spawn_writer(&ta_s, 1);
    let mut writer_b = spawn_writer(&tb_s, 2);

    // The sink must receive exactly 2N bytes — both writers got through. (Under the
    // exclusive default with no lock, NEITHER writer may write, so it would receive 0.)
    // The 30s bound turns a regression into a clean timeout, never a hang; in the
    // healthy path 2N=32 KiB drains in well under a second on a pts and in ~3 s over
    // the rig's 115200 wire.
    let sink_out = sink_child.wait_with_output().expect("await the sink");
    let verdict: Value = serde_json::from_slice(&sink_out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse sink verdict: {e}; stdout={:?}",
            String::from_utf8_lossy(&sink_out.stdout)
        )
    });

    // Reap the writers. On success they have already exited (all their bytes drained);
    // on a regression one may still be blocked in `write_all`, so kill it so the test
    // ends promptly rather than leaking a hung child.
    let _ = writer_a.kill();
    let _ = writer_a.wait();
    let _ = writer_b.kill();
    let _ = writer_b.wait();

    // Byte-exact ground truth: exactly 2N bytes crossed to the device. A single writer
    // can produce at most N distinct bytes, so a total of 2N proves BOTH contributed —
    // the free-for-all opt-out let both writers reach the device without a lock (§6).
    let received = verdict.get("received").and_then(Value::as_u64).unwrap_or(0);
    assert_eq!(
        received, TOTAL as u64,
        "device received {received} bytes, expected {TOTAL} \
         (a free-for-all writer was blocked): {verdict}"
    );
}

/// Spawn a `serial-nexus-sim client --send seeded:N` writer against a PTY path as a
/// background, killable child (its verdict is not needed — the sink's byte count is
/// the ground truth, exactly as free-for-all.sh discards the writers' output).
fn spawn_writer(path: &str, seed: u64) -> Child {
    let send = format!("seeded:{N}");
    let seed_s = seed.to_string();
    Command::new(bin("serial-nexus-sim"))
        .arg("client")
        .args([
            "--path",
            path,
            "--send",
            send.as_str(),
            "--seed",
            seed_s.as_str(),
            "--timeout-ms",
            "30000",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn serial-nexus-sim writer")
}
