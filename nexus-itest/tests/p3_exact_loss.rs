//! Phase 3 exact loss accounting, ported from `scripts/validate/phase3/exact-loss.sh`
//! (design §5 loss accounting, §7.2 the PTY boundary).
//!
//! The property: with a throttled PTY client, the PTY boundary's two drop counters
//! account for **every** byte the client did not receive, to the byte
//! (`dropped_slow_consumer + discarded_no_client + received == sent`), while a `log`
//! on the same serial captures the complete stream byte-exact. Loss is *located*
//! (the slow console boundary), *counted* (the counters), and *isolated* (the log
//! path stays lossless). The device is a seeded `nexus-sim pty --source` flooding at
//! 20 MB/s — far faster than the client's 4 MB/s drain, so the boundary must shed.
//!
//! Platform: this needs a high-rate software source standing in as a serial device
//! (a pty flooding faster than the consumer drains). That software-loopback doctrine
//! is Linux-only (`serial2` rejects a pty on macOS — `ENOTTY`), and no realistic
//! serial baud can outrun a 4 MB/s consumer, so there is no hardware analogue. The
//! test skips off Linux (a skip is a valid verdict, §5).

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::time::Duration;

    use nexus_itest::{Daemon, Sim, sha256_hex, wait_until};

    /// 24 MiB, matching the script's `SIZE_H="24MiB"` / `SIZE_B=24*1024*1024`.
    const SIZE: usize = 24 * 1024 * 1024;
    /// The source seed (`--seed 7`).
    const SEED: u64 = 7;

    /// The `nexus-sim` deterministic byte stream (splitmix64), copied verbatim from
    /// `nexus-sim`'s `seeded_bytes`. The source's own `sent`/`sha256` verdict is on a
    /// stdout that `Sim::spawn` discards, so we reconstruct the byte-exact ground
    /// truth from the seed instead — equivalent, since the stream is deterministic in
    /// `(seed, size)`.
    fn seeded_bytes(seed: u64, len: usize) -> Vec<u8> {
        let mut s = seed;
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            out.extend_from_slice(&z.to_le_bytes());
        }
        out.truncate(len);
        out
    }

    /// Read a node's two hostward PTY drop counters (§7.2 `state_extra`), 0 if absent.
    fn drop_counters(rpc: &nexus_itest::Rpc, node: &str) -> (u64, u64) {
        let n = rpc.node(node).expect("console node present in state");
        (
            n["dropped_slow_consumer"].as_u64().unwrap_or(0),
            n["discarded_no_client"].as_u64().unwrap_or(0),
        )
    }

    pub fn run() {
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();

        let dev = run.join("dev"); // serial device: the sim source's pts symlink
        let console = run.join("console"); // pty slave symlink the client opens
        let cap = run.join("cap.log"); // the lossless capture log

        // Paced 20 MB/s source: fast enough that the throttled client is attached and
        // present while the boundary sheds (drops are slow-consumer drops, not merely
        // discards-while-absent), yet still ~5x the client's 4 MB/s drain. Spawned
        // BEFORE the load, exactly as the script does, and held in scope so Drop kills
        // it (a bare `_` would drop — and kill — it immediately).
        let dev_str = dev.to_string_lossy().into_owned();
        let _source = Sim::spawn(
            &[
                "pty",
                "--source",
                "--bytes",
                "24MiB",
                "--seed",
                "7",
                "--rate",
                "20000000",
                "--link",
                dev_str.as_str(),
                "--timeout-ms",
                "120000",
            ],
            Some(&dev),
        );

        // usb0 (serial) fans out to a throttled `console` (pty, lossy) and to `cap`
        // (log, lossless): the same stream, one boundary that sheds and one that must
        // not.
        let cfg = format!(
            r#"
[[node]]
type = "pty"
name = "console"
path = "{console}"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "log"
name = "cap"
directory = "{dir}"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
"#,
            console = console.display(),
            dev = dev.display(),
            dir = run.path().display(),
        );
        rpc.load_toml(&cfg, false).expect("load config");

        assert!(
            rpc.wait_status("usb0", "active", Duration::from_secs(10)),
            "usb0 (serial) not active: {:?}",
            rpc.node("usb0")
        );
        assert!(
            rpc.wait_status("console", "active", Duration::from_secs(10)),
            "console (pty) not active: {:?}",
            rpc.node("console")
        );

        // A throttled client that reads until the stream goes quiet, fully draining so
        // no daemon-delivered byte is left unread (the precondition for exact
        // counting). The serial floods far faster than this drains, so the PTY
        // boundary sheds. `Sim::client` runs it to completion and returns its verdict.
        let console_str = console.to_string_lossy().into_owned();
        let verdict = Sim::client(&[
            "--path",
            console_str.as_str(),
            "--drain",
            "--read-rate",
            "4000000",
            "--quiet-ms",
            "1500",
            "--timeout-ms",
            "60000",
        ]);
        assert_eq!(
            verdict["pass"].as_bool(),
            Some(true),
            "drain client failed: {verdict}"
        );
        let received = verdict["received"]
            .as_u64()
            .expect("client reported received count");

        // Ground truth: the source emits exactly `SIZE` bytes of the seeded stream, so
        // `sent == SIZE` and the captured stream's checksum is that stream's checksum.
        let sent = SIZE as u64;
        let expected_sha = sha256_hex(&seeded_bytes(SEED, SIZE));

        // Exact conservation (§5): every byte the client did not receive is accounted
        // for by the two PTY drop counters. Let the post-drain counters settle — the
        // writer discards any last in-flight bytes once the client detaches — then
        // assert conservation to the byte.
        let exact = wait_until(Duration::from_secs(5), || {
            let (dropped, discarded) = drop_counters(rpc, "console");
            dropped + discarded + received == sent
        });
        let (dropped, discarded) = drop_counters(rpc, "console");
        assert!(
            exact,
            "loss not exact: dropped={dropped} discarded={discarded} received={received} \
             sum={} != sent={sent}",
            dropped + discarded + received
        );

        // A throttled client must have produced *some* slow-consumer drops: loss is
        // located at the console boundary, not merely discarded while absent.
        assert!(
            dropped > 0,
            "expected some slow-consumer drops with a throttled client (dropped={dropped})"
        );

        // The log on the same serial captured the complete stream — loss is isolated;
        // the lossless path stayed lossless. Bounded wait for the async writer to
        // flush all 24 MiB to disk.
        let captured = wait_until(Duration::from_secs(30), || {
            std::fs::metadata(&cap)
                .map(|m| m.len() == sent)
                .unwrap_or(false)
        });
        let size = std::fs::metadata(&cap).map(|m| m.len()).unwrap_or(0);
        assert!(
            captured,
            "log did not capture the full stream (size {size} != {sent})"
        );

        let data = std::fs::read(&cap).expect("read capture log");
        assert_eq!(
            sha256_hex(&data),
            expected_sha,
            "log checksum != source checksum (lossy on the lossless path)"
        );
    }
}

/// With a throttled PTY client the boundary's drop counters account for every
/// undelivered byte to the byte, while a log on the same serial captures the whole
/// stream — loss located, counted, and isolated (§5 / §7.2).
#[test]
fn pty_boundary_accounts_for_every_dropped_byte() {
    #[cfg(target_os = "linux")]
    linux_impl::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP pty_boundary_accounts_for_every_dropped_byte: needs a Linux software \
         source device (pty-as-serial); no hardware analogue for outrunning a 4 MB/s consumer"
    );
}
