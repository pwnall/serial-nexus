#![forbid(unsafe_code)]

//! Phase 3 firehose integrity, bounded memory, and **throughput**, ported from
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
//!
//! # The throughput axis, made executable (plan §18 item 61)
//!
//! `docs/benchmarks/phase3.json` has always recorded this axis twice over — a measured
//! **183.0 MiB/s** and a **30 MiB/s** headroom target — and until 2026-08-15 nothing
//! read either number. The only assertion on the axis was the 60 s deadline below,
//! which over 256 MiB is a floor of **4.27 MiB/s**: 43× under the recorded figure and
//! **7× under the recorded exit criterion**, so the guard admitted a daemon that could
//! not meet the design's own stated headroom. It never printed elapsed time either, so
//! 183 → 20 MiB/s was invisible in the pass path and in the fail path alike. That is
//! exactly the state `idle_cost` was in before item 46, and this is item 46's shape
//! applied to the other axis: time the transfer, print the reading, assert against a
//! **measured** figure that carries its provenance.
//!
//! Four measurements decided the shape, and the last of them refuted the plan's own
//! evidence. They are recorded in `docs/benchmarks/phase3.json` under
//! `throughput.provenance` and summarised at [`criterion`].
//!
//! ## 1. The source is the ceiling, not the daemon
//!
//! The confound item 61 names first — "the sim source's own rate" — turns out not to be
//! a confound to subtract but the **instrument**. Drained by a bare `read` loop with no
//! daemon in the path at all, `serial-nexus-sim pty --source` delivers 256 MiB at
//! 440–535 MiB/s on this box; the daemon delivers the same stream at 460–558 MiB/s.
//! The daemon is not the bottleneck on this path and never was — it keeps pace with a
//! null consumer to within 15 %, which is what [`reference_rate`] measures and what
//! the one assertion below is written over.
//!
//! So the recorded 183.0 MiB/s is not a statement about the daemon's speed; it is a
//! statement about how fast the box it was taken on could push a pty. It carries no
//! box, no date and no commit, and — the part that matters — **no stated window**, so
//! it cannot be compared with the figure printed here, which is timed from the first
//! byte on disk to the last. It is kept as `mib_per_s`, marked HISTORICAL, exactly as
//! item 46 kept `total_cpu_percent`.
//!
//! ## 2. An absolute floor on this axis cannot be asserted per run
//!
//! Measured before choosing anything (AGENTS §8). The same healthy tree, on the same
//! box, under 20 spinning CPU hogs on 20 cores, reads **11.9, 14.1, 18.8, 26.7, 27.5,
//! 28.0, 29.2, 36.3, 44.6, 52.1, 61.4, 62.9 MiB/s** — against 460–558 MiB/s unloaded.
//! Load moves this number by **30×**, and notes §3.69 records the same guard reading
//! ~4 MB/s on an 8-core box under a load of 19.6 while the tree was healthy.
//!
//! So the recorded 30 MiB/s exit criterion, asserted as a per-run wall-clock floor,
//! would have reddened on a *healthy* tree in five of those twelve runs. That is the
//! trap item 46 spent its report on: a tripwire that fires for the machine it runs on
//! teaches its readers to raise it. The exit criterion stays where it belongs — as the
//! recorded criterion the recorded *figure* is checked against, in [`criterion`] — and
//! is deliberately **not** asserted against this run's box. Nothing here is looser than
//! what shipped: the 60 s deadline is unchanged and still catches a stall.
//!
//! The same measurement kills the other obvious normaliser. The daemon's CPU cost per
//! MiB — load-independent in principle, and printed below because it is the quantity a
//! per-byte hot-path structure actually inflates — moves from 2.78–3.63 ms/MiB
//! unloaded to 9.6–22.2 ms/MiB under the same hogs. A 7× spread is better than 30×
//! and still far too wide to carry a ceiling.
//!
//! ## 3. Two rungs on one box, which is what does cancel the load
//!
//! `p3_idle_cost` cancels the box's fixed cost by measuring two rungs inside one daemon
//! process. The analogue here: measure what the box can move through a pty with a null
//! consumer *in the same run*, immediately before and immediately after the daemon's
//! own transfer, and assert their **ratio**. A slow box slows both rungs; a daemon that
//! became the bottleneck moves only one of them.
//!
//! Measured, healthy: **0.85, 0.85, 0.86** unloaded (three runs), and **0.33, 0.56,
//! 3.30, 3.69** under the 20-hog load. The ceiling is the artifact's
//! `reference_ratio_ceiling`, read by [`criterion`], and it sits 2.2× above the worst
//! healthy reading ever taken here — a
//! margin the loaded arm's spread demands, since the two rungs run about a second apart
//! and a load spike that lands on one and not the other is indistinguishable from a
//! regression. The lower of the two reference readings is used, which is the direction
//! that gives the *tree* the benefit of the doubt about the box.
//!
//! ## 4. The failure mode on this path is a stall, not a slope — which refutes the
//! item's own evidence
//!
//! Item 61 states that §5's ring-storage tripwire is orphaned, because "a `VecDeque<u8>`
//! drain+extend rewrite would clear 4.27 MiB/s comfortably". **Measured, in a detached
//! worktree at `7410b62` so nothing moved under the experiment: it does not.** The
//! defect §5 clause 3 names, planted, stalls the transfer at 13–43 % and sits there;
//! its mildest imaginable form — the same fixed circular `Vec`, written one byte at a
//! time — delivers 0.69–0.75 MiB/s. Both blow the 60 s deadline, three runs each. The
//! old guard **does** catch its named defect, and the tripwire was never orphaned.
//!
//! Five regression classes were planted in all (the two ring rewrites, a calibrated
//! spin and a calibrated sleep on the mirror path at eight settings between 4 µs and
//! 2 ms per chunk, and `runtime::READ_BUF` at eight sizes between 64 B and 4 KiB).
//! **None of them lands in the band a tightened throughput floor would cover.** The
//! pipeline is bistable: it keeps pace with a null consumer to within ~3×, or it
//! collapses to ~4.3 MiB/s and below — nothing measured sits between 4.3 and 108 MiB/s
//! — and the collapse is usually a *stall*, several of them within 0.2 % of the end
//! (`READ_BUF = 128` and `64` reach 99.95 % and hang; the ring rewrite stalls at 13 %
//! after its source takes an `EIO`). A throughput ratio cannot judge a transfer that
//! never finishes, which is why the deadline stays where it is and why its message,
//! not its number, is what changed.
//!
//! ## What this establishes, and what it does not
//!
//! **Established.** The axis has a reading, printed on every run in both paths — item
//! 61's first complaint, that 183 → 20 MiB/s was invisible. The recorded figure has
//! provenance: a box, a date, a commit, a stated window, and a null-consumer control,
//! so it can be re-measured, which the unprovenanced 183.0 never could. A deadline miss
//! now names its own cause — the partial rate, the reference rate taken on the same box
//! minutes earlier, and how long ago the sink last grew.
//!
//! **Not established, and said rather than hidden.** Assertion (1)'s ceiling has *no
//! reproduced code failure*. It is wired and non-vacuous — planting the *criterion*
//! (`reference_ratio_ceiling` 8.0 → 0.5, with [`criterion`]'s own sanity floor relaxed
//! to reach it) reddens a healthy run with the full message below, so its passing
//! output is not its not-running output (AGENTS §3's tell) — but every code
//! plant tried arrives as a stall the deadline catches first, so it ships as a
//! **backstop against a future regression that is slow without stalling**, not as a
//! guard with fail-first proof behind it (AGENTS §9). Say so when quoting it. It is
//! also one-sided: if the *reference* rung is what a saturated box starves, the ratio
//! falls and the guard passes more easily. That is the deliberate direction — the
//! alternative flakes — and it is why the reference rate is printed rather than merely
//! used.

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use serde_json::Value;
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
    /// The wall-clock backstop, **unchanged** from the shell script this file replaced.
    /// It is now understood as a *stall* detector rather than a throughput floor
    /// (module doc, measurement 4), and it is deliberately not raised: every regression
    /// this file's fail-first work could construct arrives here as a stall, so loosening
    /// it in favour of something else would trade a guard that fires for one that does
    /// not. What changed is its **message**, which now carries the reading.
    const DEADLINE: Duration = Duration::from_secs(60);
    /// How often the drain loop stats the sink. It bounds the error on the
    /// throughput window's *start*: the window opens on the first poll that sees a
    /// non-empty sink, so the true first byte landed at most one `POLL` earlier. At
    /// 2 ms against a window of well over a second that is under 0.2 %.
    const POLL: Duration = Duration::from_millis(2);
    /// The calibration rung's payload. A quarter of `SIZE` because the rate is flat in
    /// the payload (measured: 447–521 MiB/s at 64 MiB against 474–508 MiB/s at 256 MiB
    /// in the same runs) and the rung is paid for twice.
    const REF_SIZE: usize = 64 * 1024 * 1024;

    /// The recorded phase-3 throughput criterion, read from
    /// `docs/benchmarks/phase3.json` rather than restated here — that artifact is the
    /// record, and a guard that copied the numbers would let the two drift, which is
    /// how this axis came to have no executable meaning in the first place.
    pub struct ThroughputCriterion {
        /// The ceiling on `reference / daemon`, i.e. how far behind a null consumer
        /// the daemon may fall on the same box in the same run.
        pub ratio_ceiling: f64,
        /// The measured figure the artifact records for this box, printed beside this
        /// run's reading so a drift is legible. Not asserted against — see the module
        /// doc, measurement 2.
        pub recorded_mib_per_s: f64,
        /// The recorded exit criterion (§15.19's headroom target). Checked against the
        /// recorded figure, never against this run's box.
        pub headroom_target: f64,
        /// The original benchmark's figure, kept as the record it is.
        pub historical_mib_per_s: f64,
    }

    /// Read the `throughput` axis from `docs/benchmarks/phase3.json`.
    ///
    /// Panics rather than defaults if the artifact is missing or reshaped: a guard that
    /// silently substituted its own numbers for the recorded criterion would be
    /// measuring something nobody wrote down, which is the state this test exists to
    /// end (`p3_idle_cost::criterion`, same rule).
    pub fn criterion() -> ThroughputCriterion {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("itest/ has a parent")
            .join("docs/benchmarks/phase3.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let v: Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let t = &v["throughput"];
        let num = |key: &str| -> f64 {
            t[key].as_f64().unwrap_or_else(|| {
                panic!("{} has no numeric throughput.{key}: {t}", path.display())
            })
        };
        let c = ThroughputCriterion {
            ratio_ceiling: num("reference_ratio_ceiling"),
            recorded_mib_per_s: num("sustained_mib_per_s"),
            headroom_target: num("headroom_target_mib_per_s"),
            historical_mib_per_s: num("mib_per_s"),
        };
        // The artifact must be internally consistent, or the ceiling below is derived
        // from a record that already fails its own criterion. This is where the
        // recorded 30 MiB/s exit criterion is enforced: against the recorded figure,
        // which was taken on a named box on a named date, and never against whatever
        // box happens to be running this (module doc, measurement 2).
        assert!(
            c.recorded_mib_per_s >= c.headroom_target
                && c.historical_mib_per_s >= c.headroom_target,
            "docs/benchmarks/phase3.json records {} MiB/s (and historically {}) against \
             a {} MiB/s headroom target — the recorded runs do not meet the recorded \
             criterion, so nothing derived from them means anything",
            c.recorded_mib_per_s,
            c.historical_mib_per_s,
            c.headroom_target
        );
        // A ceiling at or below 1 would demand the daemon beat a null consumer, which
        // it does only by a margin no box owes anyone; a ceiling that large is a
        // guard nothing can fail. Both ends, so the artifact cannot disarm this file
        // by editing one number.
        assert!(
            c.ratio_ceiling > 1.0 && c.ratio_ceiling < 50.0,
            "docs/benchmarks/phase3.json records a reference ratio ceiling of {} — \
             outside (1, 50) it is either unmeetable or unfailable",
            c.ratio_ceiling
        );
        c
    }

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

    /// **The calibration rung**: what this box, at this instant, moves through a pty
    /// with a *null* consumer.
    ///
    /// The same `serial-nexus-sim pty --source` double the firehose is fed by, drained
    /// by a bare `read` loop in this process — no daemon, no graph, no log node. The
    /// window has the same shape as the daemon rung's (first byte to last), so the two
    /// are comparable, and the sim's payload *generation* — 1.08–1.10 s for 256 MiB in
    /// an unoptimised build, which happens before the first byte moves — is outside
    /// both windows by construction.
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
        // O_NOCTTY: this is a pts, and a test process must never take one as its
        // controlling terminal.
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOCTTY)
            .open(&dev)
            .unwrap_or_else(|e| panic!("open the calibration source {}: {e}", dev.display()));
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
                // The master closes when the source is done; the tail of the payload
                // has already been counted by then.
                Err(_) => break,
            }
        }
        let elapsed = t0
            .expect("the calibration source delivered nothing at all")
            .elapsed();
        // A rung that delivered a fraction of its payload is not a rate this run may
        // divide by: it would understate what the box can do and pass the daemon rung
        // for the box's reasons.
        assert!(
            seen * 100 >= bytes * 99,
            "the calibration source delivered {seen} of {bytes} B — the box or the \
             double failed, and a partial rung cannot calibrate anything"
        );
        let mib = seen as f64 / (1024.0 * 1024.0);
        let rate = mib / elapsed.as_secs_f64();
        eprintln!("p3_firehose: reference[{label}] {seen} B in {elapsed:?} = {rate:.1} MiB/s");
        rate
    }

    pub fn run(c: ThroughputCriterion) {
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();

        // The daemon's PID (for the /proc RSS sample), found before the stream.
        let socket = d.socket();
        let pid = wait_for_daemon_pid(&socket);

        // Rung 1 of the calibration pair, taken before the firehose source exists so
        // its 256 MiB of payload generation cannot compete with this measurement.
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
        // SIZE and RSS must stay under budget within the throughput bound.
        let sink = run.join("sink.log");
        let mut peak_kb: u64 = 0;
        // When the sink was first seen non-empty, how much was already there, and the
        // daemon's CPU at that moment — the start of the throughput window.
        let mut first_bytes: Option<(Instant, u64, u64)> = None;
        // When the sink last grew, and to what. A deadline miss that says only "did not
        // complete" cannot tell a slow box from a stalled pipeline, and every plant this
        // file's fail-first work landed arrives as the second (module doc, measurement
        // 4): three of them stopped within 0.2 % of the end and sat there.
        let mut progress = (Instant::now(), 0u64);
        let deadline = Instant::now() + DEADLINE;
        let done_at;
        loop {
            let size = std::fs::metadata(&sink).map(|m| m.len()).unwrap_or(0);
            if let Some(rss) = vmrss_kb(pid)
                && rss > peak_kb
            {
                peak_kb = rss;
            }
            if size > progress.1 {
                progress = (Instant::now(), size);
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
                "firehose did not complete within {DEADLINE:?}; sink at {size}/{SIZE} B \
                 ({:.1} % of the stream), which is {:.2} MiB/s averaged over the whole \
                 window, and the last byte landed {:?} ago. A null consumer moved \
                 {ref_before:.1} MiB/s through the same double on this box just before \
                 this transfer started, so a reading far under that is the daemon and \
                 not the box. **Read the two numbers before blaming either**: a sink \
                 that stopped growing seconds ago is a stall — every regression class \
                 this file has been planted with (the §5 clause 3 ring rewrite, a \
                 shrunken `runtime::READ_BUF`) arrives that way, three of them within \
                 0.2 % of the end — while a sink still creeping is a slow box, which \
                 notes §3.69 records at ~4 MB/s on a machine under a load of 19.6 with \
                 nothing wrong.",
                size as f64 * 100.0 / SIZE as f64,
                size as f64 / (1024.0 * 1024.0) / DEADLINE.as_secs_f64(),
                progress.0.elapsed(),
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
        // Printed, not asserted: it is the quantity a per-byte hot-path structure
        // inflates, and the one a reader wants when the ratio below reddens — but it
        // moves 7× with load on its own (module doc, measurement 2).
        let cpu_ms_per_mib = (cpu1 - cpu0) as f64 / 1e6 / mib;

        // Rung 2 of the calibration pair, adjacent in time to the daemon rung. The
        // daemon is idle by now and the firehose source has exited.
        let ref_after = reference_rate(run.path(), REF_SIZE, "after");
        // The lower of the two: the reference is what the daemon is measured against,
        // so taking its smaller reading gives the *tree* the benefit of the doubt
        // about the box. Stated in the module doc as the guard's one-sidedness.
        let reference = ref_before.min(ref_after);
        let ratio = reference / mib_per_s;
        eprintln!(
            "p3_firehose: {moved} B in {elapsed:?} = {mib_per_s:.1} MiB/s wall \
             ({cpu_ms_per_mib:.3} ms of daemon CPU per MiB), against a null consumer's \
             {reference:.1} MiB/s on the same box -> ratio {ratio:.2} \
             (ceiling {:.2}; docs/benchmarks/phase3.json records {:.1} MiB/s)",
            c.ratio_ceiling, c.recorded_mib_per_s
        );

        // (1) The drift backstop. Two rungs on one box, seconds apart, so a slow box
        //     slows both and only a daemon that became the bottleneck moves one. Read
        //     the module doc's closing section before quoting this as a proven guard:
        //     it is wired and non-vacuous, and no code plant has ever reddened it.
        assert!(
            ratio <= c.ratio_ceiling,
            "the daemon moved {mib_per_s:.1} MiB/s while a bare `read` loop moved \
             {reference:.1} MiB/s from the same double on the same box in the same run \
             — a ratio of {ratio:.2} against the ceiling of {:.2} recorded in \
             docs/benchmarks/phase3.json, whose healthy readings are 0.85 unloaded and \
             3.69 at their worst under a saturated box. The daemon has become the \
             bottleneck on the hostward path. §5's ring-storage tripwire is the first \
             suspect — the \
             replay ring is on by default on EVERY endpoint (§15.32), so a per-byte \
             structure there is a measured collapse — then the §15.18/§15.19 blocking \
             reader thread and the one shared fragmenter (§15.27). This run cost \
             {cpu_ms_per_mib:.3} ms of daemon CPU per MiB; healthy is 2.8–3.6 ms/MiB \
             unloaded on the recorded box. If the box is simply saturated, the \
             reference rung is the control: it was measured at {ref_before:.1} MiB/s \
             before and {ref_after:.1} MiB/s after, and a spread between those two is \
             load rather than code.",
            c.ratio_ceiling
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
    linux_impl::run(linux_impl::criterion());
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP firehose_is_byte_exact_with_bounded_daemon_memory: \
         no software serial source device / /proc RSS on this platform"
    );
}
