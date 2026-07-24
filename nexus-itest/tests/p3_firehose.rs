//! Phase 3 firehose integrity + bounded memory, ported from
//! `scripts/validate/phase3/firehose.sh` (design §5 + §15.18/§15.19).
//!
//! A large seeded stream flows device -> daemon -> fast log sink with its
//! checksum intact and at high throughput, while the daemon's resident memory
//! stays bounded — proof that the interior accumulates nothing and the dedicated
//! blocking reader thread (the §15.18/§15.19 escape hatch) delivers line rate.
//! The fast sink is a `log` node (a dedicated blocking writer); the serial reader
//! is a dedicated blocking thread. No hardware.
//!
//! Platform: this needs a high-rate software serial *source* — a seeded
//! `nexus-sim pty --source` flooding a serial node faster than any realistic baud.
//! That software-loopback doctrine is Linux-only (`serial2` rejects a pty on
//! macOS — `ENOTTY`), and the RSS budget reads `/proc/<pid>/status`, so there is
//! no portable analogue. The test skips off Linux (a skip is a valid verdict, §5).

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use nexus_itest::{Daemon, Sim, sha256_hex};

    /// 256 MiB, matching the script's `SIZE_H="256MiB"` / `SIZE_B=256*1024*1024`.
    /// Far larger than the RSS budget, so any interior accumulation of the stream
    /// blows well past it.
    const SIZE: usize = 256 * 1024 * 1024;
    /// The RSS ceiling (`RSS_BUDGET_KB=120*1024`): streaming stays in the tens of
    /// MiB; accumulation would exceed this.
    const RSS_BUDGET_KB: u64 = 120 * 1024;
    /// The source seed (`--seed 7`).
    const SEED: u64 = 7;

    /// The `nexus-sim` deterministic byte stream (splitmix64), copied verbatim from
    /// `nexus-sim`'s `seeded_bytes`. The source's own `sha256` verdict is on a
    /// stdout that `Sim::spawn` discards, so we reconstruct the byte-exact ground
    /// truth from the seed instead — equivalent, since the stream is deterministic
    /// in `(seed, size)`, and hashed through the approved `sha256_hex` oracle.
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

    /// Scan /proc for the `serialnexusd` process whose NUL-separated argv carries
    /// `socket` (unique per test) — the portable-Rust stand-in for the bash's
    /// captured `$!`, which the harness does not expose.
    fn find_daemon_pid(socket: &Path) -> Option<u32> {
        let want = socket.to_string_lossy();
        for entry in std::fs::read_dir("/proc").ok()?.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
                continue;
            };
            let mut is_daemon = false;
            let mut matches_socket = false;
            for arg in cmdline.split(|&b| b == 0) {
                if arg == b"serialnexusd" || arg.ends_with(b"/serialnexusd") {
                    is_daemon = true;
                }
                if arg == want.as_bytes() {
                    matches_socket = true;
                }
            }
            if is_daemon && matches_socket {
                return Some(pid);
            }
        }
        None
    }

    /// Poll /proc until the daemon owning `socket` is found, returning its pid.
    fn wait_for_daemon_pid(socket: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pid) = find_daemon_pid(socket) {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "could not find daemon pid for socket {}",
                socket.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A process's resident set size in KiB from /proc/<pid>/status (`VmRSS:`), or
    /// `None` if the process is gone / the field is absent.
    fn vmrss_kb(pid: u32) -> Option<u64> {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                // e.g. "VmRSS:\t   12345 kB"
                return rest.split_whitespace().next()?.parse::<u64>().ok();
            }
        }
        None
    }

    pub fn run() {
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();

        // The daemon's PID (for the /proc RSS sample), found before the stream.
        let socket = d.socket();
        let pid = wait_for_daemon_pid(&socket);

        // A software serial *source*: a pty double that floods SIZE seeded bytes
        // then exits. Spawned BEFORE the load (as the script does) and held in
        // scope so Drop kills it; `Sim::spawn` waits for the device to appear.
        let dev = run.join("dev");
        let dev_str = dev.to_string_lossy().into_owned();
        // Named binding (not a bare `_`) so the source is held — and killed on
        // Drop — to the end of the test, rather than dropped immediately.
        let _source = Sim::spawn(
            &[
                "pty",
                "--source",
                "--bytes",
                "256MiB",
                "--seed",
                "7",
                "--link",
                dev_str.as_str(),
                "--timeout-ms",
                "120000",
            ],
            Some(&dev),
        );

        // serial(usb0) -> log(sink): the fast sink is a dedicated blocking writer,
        // the serial reader a dedicated blocking thread (§15.18/§15.19).
        let cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "log"
name = "sink"
directory = "{dir}"
filename = "sink.log"
[[edge]]
a = "usb0"
b = "sink"
"#,
            dev = dev.display(),
            dir = run.path().display(),
        );
        rpc.load_toml(&cfg, false).expect("load firehose config");

        // Drain the stream, sampling the daemon's RSS each turn and keeping the
        // peak. The interior must never accumulate: the sink must reach exactly
        // SIZE and RSS must stay under budget within the 60s throughput bound.
        let sink = run.join("sink.log");
        let mut peak_kb: u64 = 0;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let size = std::fs::metadata(&sink).map(|m| m.len()).unwrap_or(0);
            if let Some(rss) = vmrss_kb(pid)
                && rss > peak_kb
            {
                peak_kb = rss;
            }
            if size >= SIZE as u64 {
                break;
            }
            assert!(
                Path::new(&format!("/proc/{pid}")).exists(),
                "daemon exited mid-transfer (sink at {size}/{SIZE} B)"
            );
            assert!(
                Instant::now() < deadline,
                "firehose did not complete within 60s (throughput regression); \
                 sink at {size}/{SIZE} B"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        // Byte-exact: identical size and checksum (a lossy firehose fails here).
        // Reconstruct the source's checksum from the seed, then release that buffer
        // before reading the sink so the test's own footprint stays ~SIZE, not 2x.
        let src_sha = sha256_hex(&seeded_bytes(SEED, SIZE));
        let sink_len = std::fs::metadata(&sink).expect("stat sink.log").len();
        assert_eq!(
            sink_len, SIZE as u64,
            "sink size {sink_len} != source size {SIZE} (lossy firehose)"
        );
        let sink_bytes = std::fs::read(&sink).expect("read sink.log");
        let sink_sha = sha256_hex(&sink_bytes);
        assert_eq!(
            sink_sha, src_sha,
            "sink checksum != source checksum (bytes lost/reordered/duplicated)"
        );

        // Bounded interior: RSS was sampled, and its peak stayed under budget.
        assert!(peak_kb > 0, "could not sample daemon RSS");
        assert!(
            peak_kb < RSS_BUDGET_KB,
            "daemon RSS peak {peak_kb} KB exceeded the {RSS_BUDGET_KB} KB budget \
             (interior accumulation?)"
        );
    }
}

#[test]
fn firehose_is_byte_exact_with_bounded_daemon_memory() {
    #[cfg(target_os = "linux")]
    linux_impl::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP firehose_is_byte_exact_with_bounded_daemon_memory: \
         no software serial source device / /proc RSS on this platform"
    );
}
