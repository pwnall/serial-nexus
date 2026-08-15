#![forbid(unsafe_code)]

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
//! `serial-nexus-sim pty --source` flooding a serial node faster than any realistic baud.
//! That software-loopback doctrine is Linux-only (`serial2` rejects a pty on
//! macOS — `ENOTTY`), and the RSS budget reads `/proc/<pid>/status`, so there is
//! no portable analogue. The test skips off Linux (a skip is a valid verdict, §5).

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serial_nexus_itest::{Daemon, Sim, cpu_nanos, seeded_bytes, sha256_hex};

    /// 256 MiB, matching the script's `SIZE_H="256MiB"` / `SIZE_B=256*1024*1024`.
    /// Far larger than the RSS budget, so any interior accumulation of the stream
    /// blows well past it.
    const SIZE: usize = 256 * 1024 * 1024;
    /// The RSS ceiling (`RSS_BUDGET_KB=120*1024`): streaming stays in the tens of
    /// MiB; accumulation would exceed this.
    const RSS_BUDGET_KB: u64 = 120 * 1024;
    /// The source seed (`--seed 7`).
    const SEED: u64 = 7;
    /// How often the drain loop stats the sink. It bounds the error on the
    /// throughput window's *start*: the window opens on the first poll that sees
    /// a non-empty sink, so the true first byte landed at most one `POLL` earlier.
    /// At 2 ms against a window of well over a second that is under 0.2 %.
    const POLL: Duration = Duration::from_millis(2);
    /// The calibration rung's payload.
    const REF_SIZE: usize = 64 * 1024 * 1024;

    /// Scan /proc for the `serial-nexus-daemon` process whose NUL-separated argv carries
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
                if arg == b"serial-nexus-daemon" || arg.ends_with(b"/serial-nexus-daemon") {
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

    /// What this box, at this instant, moves through a pty with a *null* consumer:
    /// the same `serial-nexus-sim pty --source` double the firehose is fed by, drained
    /// by a bare `read` loop in this process. Returns MiB/s over the same window shape
    /// the daemon rung uses (first byte to last), and the bytes it actually saw.
    fn reference_rate(run: &Path, bytes: usize, label: &str) -> f64 {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;

        let dev = run.join(format!("refdev-{label}"));
        let _src = Sim::spawn(
            &[
                "pty",
                "--source",
                "--bytes",
                &format!("{bytes}"),
                "--seed",
                "7",
                "--link",
                &dev.to_string_lossy(),
                "--timeout-ms",
                "120000",
            ],
            Some(&dev),
        );
        // O_NOCTTY: this is a pts, and a test process must never acquire one as its
        // controlling terminal.
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&dev)
            .unwrap_or_else(|e| panic!("open the reference source {}: {e}", dev.display()));
        let mut buf = vec![0u8; 64 * 1024];
        let mut seen = 0usize;
        let mut t0: Option<Instant> = None;
        while seen < bytes {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if t0.is_none() {
                        t0 = Some(Instant::now());
                    }
                    seen += n;
                }
                Err(_) => break,
            }
        }
        let elapsed = t0
            .expect("the reference source delivered nothing")
            .elapsed();
        let mib = seen as f64 / (1024.0 * 1024.0);
        let rate = mib / elapsed.as_secs_f64();
        eprintln!("p3_firehose: reference[{label}] {seen} B in {elapsed:?} = {rate:.1} MiB/s");
        rate
    }

    pub fn run() {
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();

        // The daemon's PID (for the /proc RSS sample), found before the stream.
        let socket = d.socket();
        let pid = wait_for_daemon_pid(&socket);

        let ref_before = reference_rate(run.path(), REF_SIZE, "before");

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
        // (when the sink was first seen non-empty, how much was already there) —
        // the start of the throughput window. See `moved`/`elapsed` below.
        let mut first_bytes: Option<(Instant, u64, u64)> = None;
        let deadline = Instant::now() + Duration::from_secs(900);
        let done_at;
        loop {
            let size = std::fs::metadata(&sink).map(|m| m.len()).unwrap_or(0);
            if let Some(rss) = vmrss_kb(pid)
                && rss > peak_kb
            {
                peak_kb = rss;
            }
            if first_bytes.is_none() && size > 0 {
                first_bytes = Some((Instant::now(), size, cpu_nanos(pid)));
            }
            if size >= SIZE as u64 {
                done_at = (Instant::now(), cpu_nanos(pid));
                break;
            }
            assert!(
                Path::new(&format!("/proc/{pid}")).exists(),
                "daemon exited mid-transfer (sink at {size}/{SIZE} B)"
            );
            assert!(
                Instant::now() < deadline,
                "firehose did not complete within 900s (throughput regression); \
                 sink at {size}/{SIZE} B"
            );
            std::thread::sleep(POLL);
        }
        let (t0, size0, cpu0) = first_bytes.expect(
            "the sink went from empty to complete inside one poll, so there is no \
             throughput window to measure",
        );
        let (t1, cpu1) = done_at;
        let elapsed = t1.duration_since(t0);
        let moved = SIZE as u64 - size0;
        let mib = moved as f64 / (1024.0 * 1024.0);
        let mib_per_s = mib / elapsed.as_secs_f64();
        let cpu_ms_per_mib = (cpu1 - cpu0) as f64 / 1e6 / mib;
        eprintln!(
            "p3_firehose: {moved} B in {elapsed:?} = {mib_per_s:.1} MiB/s wall, \
             {:.3} ms of daemon CPU per MiB ({} ms total) \
             (window opens at the first byte on disk, {size0} B already there)",
            cpu_ms_per_mib,
            (cpu1 - cpu0) / 1_000_000
        );
        let ref_after = reference_rate(run.path(), REF_SIZE, "after");
        let reference = ref_before.min(ref_after);
        eprintln!(
            "p3_firehose: ratio[min] {:.2}  ratio[before] {:.2}  ratio[after] {:.2}",
            reference / mib_per_s,
            ref_before / mib_per_s,
            ref_after / mib_per_s
        );

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
