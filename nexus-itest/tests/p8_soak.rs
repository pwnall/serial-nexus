//! Phase 8 soak slice, ported from `scripts/validate/phase8/soak.sh`
//! (design §5, plan §Phase 8). A daemon under continuous synthetic load, asserting
//! the four release-soak signals throughout:
//!
//! 1. **Bounded VmRSS** — no interior accumulation; the daemon's resident set stays
//!    under budget for the whole run.
//! 2. **Loss-counter allowlist** — every `drop`/`discard`/`purge` counter anywhere in
//!    the `state` graph stays flat at zero on a keep-up baseline; any growth is a
//!    failure. This mirrors the shared `assert_loss_counters_zero` helper (§16.5): a
//!    recursive walk summing every numeric key matching `drop|discard|purge`.
//! 3. **Zero unexplained faulted nodes** — no node reaches `status == "faulted"`.
//! 4. **Final per-stream checksum reconciliation** — the `log` sink equals the
//!    generator, byte for byte.
//!
//! Topology (no hardware, §15.17): a paced firehose device (`nexus-sim pty --source
//! --rate …`) → `serial` (`usb0`, free-for-all) → `log` sink (`cap`, `write_mode =
//! "never"`). A paced source keeps the port "present" the whole run (§7.1), so a fault
//! or a drop is a real regression, not absence.
//!
//! ## Platform (Linux-only, skips elsewhere)
//! This needs a software serial *source* — a `nexus-sim pty --source` flooding a
//! `serial` node — which is Linux-only (`serial2` rejects a pty on macOS, `ENOTTY`),
//! and the VmRSS budget reads `/proc/<pid>/status`, for which there is no portable
//! analogue. A skip is a valid verdict (§5).
//!
//! ## Deviations from the bash, each preserving the original *assertions*
//! * `awk /VmRSS/ /proc/$DPID/status` → [`vmrss_kb`]; the daemon pid (which
//!   `Daemon::start` does not expose, the bash's `$!`) is found by scanning `/proc`
//!   cmdlines for the one carrying this test's unique socket ([`find_daemon_pid`], the
//!   `p3_firehose` pattern).
//! * `jq '.nodes[]|select(.status=="faulted")'` and `assert_loss_counters_zero` →
//!   structured walks over the `state` RPC `Value` ([`faulted_nodes`],
//!   [`loss_counter_sum`]). The precedence tautology the phase-8 audit caught cannot
//!   recur: this sums numeric counters directly and asserts `== 0`.
//! * `pty --source`'s `sha256`/`sent` verdict lands on a stdout that `Sim::spawn`
//!   discards, so — as in `p3_firehose` — the byte-exact ground truth is reconstructed
//!   from the seed via the deterministic [`seeded_bytes`] and hashed through the
//!   sanctioned [`sha256_hex`]; `sent` is exactly the `--bytes` this test dictates.
//! * `sleep`/`timeout`/`wait-for.sh` → bounded [`wait_until`] and an `Instant`-bounded
//!   sample loop; `stat -c %s`/`cat|wc -c` → [`file_len`]; `sha256sum` → [`sha256_hex`].
//!
//! ## Fast vs. nightly
//! [`soak_short`] is a short, always-run smoke (a few seconds) that exercises all four
//! signals; [`soak_endurance`] is `#[ignore]`d (nightly), parameterized by the same
//! `SOAK_SECONDS`/`SOAK_RATE_MIB`/`SOAK_INTERVAL`/`SOAK_RSS_KB` env vars as the bash.
//! A literal 24 h / multi-hundred-GB run is *not* portably expressible here: the sim
//! source pre-allocates its whole payload (a huge `--bytes` OOMs it) and the harness's
//! sha API is whole-buffer (no streaming hash). The endurance test therefore streams a
//! long-but-bounded volume; above [`SHA_RECONCILE_CAP`] it reconciles by exact byte
//! count (still a strong loss check) and notes the limitation.

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use nexus_itest::{Daemon, Sim, sha256_hex, wait_until};
    use serde_json::Value;

    /// The source seed (`--seed 7`).
    const SEED: u64 = 7;
    /// Above this streamed volume, reconcile by exact byte count instead of a
    /// whole-file SHA-256 (the harness sha API buffers the whole file; keep the test's
    /// own footprint bounded). 256 MiB comfortably covers the default endurance run.
    const SHA_RECONCILE_CAP: u64 = 256 * 1024 * 1024;
    /// Never ask the sim source to pre-allocate more than this (it buffers its entire
    /// `--bytes` payload in memory before writing). Caps the endurance volume so a
    /// huge `SOAK_SECONDS` clamps rather than OOMs.
    const SIM_ALLOC_CAP: u64 = 2 * 1024 * 1024 * 1024;

    /// Parameters for one soak run (the bash's `SOAK_*` knobs).
    pub struct Soak {
        pub seconds: u64,
        pub rate_mib: u64,
        pub interval: Duration,
        pub rss_budget_kb: u64,
    }

    /// The `nexus-sim` deterministic byte stream (splitmix64), copied verbatim from
    /// `nexus-sim`'s `seeded_bytes` (and matching `p3_firehose`). The source's own
    /// `sha256` verdict is on a stdout `Sim::spawn` discards, so we reconstruct the
    /// byte-exact ground truth from `(seed, size)` and hash it through `sha256_hex`.
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

    /// Current on-disk length of `p` (0 if absent) — the portable replacement for
    /// `stat -c %s`.
    fn file_len(p: &Path) -> u64 {
        std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
    }

    /// Scan `/proc` for the `serialnexusd` process whose NUL-separated argv carries
    /// `socket` (unique per test) — the stand-in for the bash's captured `$!`, which
    /// the harness does not expose (the `p3_firehose` pattern).
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

    /// Poll `/proc` until the daemon owning `socket` is found, returning its pid.
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

    /// A process's resident set size in KiB from `/proc/<pid>/status` (`VmRSS:`), or
    /// `None` if the process is gone / the field is absent.
    fn vmrss_kb(pid: u32) -> Option<u64> {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                return rest.split_whitespace().next()?.parse::<u64>().ok();
            }
        }
        None
    }

    /// The names of any nodes reporting `status == "faulted"` in a `state` snapshot
    /// (signal 3). Empty on a healthy soak.
    fn faulted_nodes(state: &Value) -> Vec<String> {
        state
            .get("nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter(|n| n.get("status").and_then(Value::as_str) == Some("faulted"))
                    .map(|n| {
                        n.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sum of every numeric value under a key matching `drop|discard|purge`, anywhere
    /// in the `state` JSON (signal 2). The Rust twin of `assert_loss_counters_zero`
    /// (§16.5): a recursive descent, summing directly — no jq-precedence tautology.
    /// Also records the nonzero counters for a useful failure message.
    fn loss_counter_sum(v: &Value, nonzero: &mut Vec<(String, i64)>) -> i64 {
        let mut sum = 0i64;
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    if (k.contains("drop") || k.contains("discard") || k.contains("purge"))
                        && let Some(n) = val.as_i64()
                    {
                        sum += n;
                        if n != 0 {
                            nonzero.push((k.clone(), n));
                        }
                    }
                    sum += loss_counter_sum(val, nonzero);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    sum += loss_counter_sum(val, nonzero);
                }
            }
            _ => {}
        }
        sum
    }

    /// Run one soak: boot a daemon, wire the paced-firehose topology, sample signals
    /// (1)(2)(3) for `p.seconds`, then reconcile the sink checksum (4).
    pub fn run_soak(p: Soak) {
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();

        // The daemon pid (for the /proc RSS sample), found before the stream.
        let socket = d.socket();
        let pid = wait_for_daemon_pid(&socket);

        // Size the source to stream for ~the whole duration plus a margin, so it does
        // not run dry before the last sample (the bash's TOTAL_BYTES formula). Clamp
        // to SIM_ALLOC_CAP: the sim pre-allocates its whole payload, so a huge
        // SOAK_SECONDS must clamp (and the source's long hold keeps the port present
        // for the remainder) rather than OOM.
        let rate_bytes = p.rate_mib * 1024 * 1024;
        let margin_secs = p.seconds + p.interval.as_secs() + 2;
        let mut total_bytes = rate_bytes.saturating_mul(margin_secs);
        if total_bytes > SIM_ALLOC_CAP {
            total_bytes = SIM_ALLOC_CAP - (SIM_ALLOC_CAP % rate_bytes.max(1));
            eprintln!(
                "NOTE p8_soak: streamed volume clamped to {total_bytes} B (the sim source \
                 pre-allocates its whole payload; a true multi-hundred-GB 24 h run is not \
                 expressible with this harness)"
            );
        }

        // The paced firehose: emit total_bytes at rate_bytes/s, then stay "plugged in"
        // (the daemon holds it present) well past sampling + reconciliation. Spawned
        // before load so the device exists when the serial node opens it; held in
        // scope so Drop kills it (`Sim::spawn` waits for the device to appear).
        let dev = run.join("device");
        let dev_str = dev.to_string_lossy().into_owned();
        let total_str = total_bytes.to_string();
        let rate_str = rate_bytes.to_string();
        let seed_str = SEED.to_string();
        let hold_ms = ((p.seconds + 60) * 1000).to_string();
        let timeout_ms = ((p.seconds + 120) * 1000).to_string();
        let _source = Sim::spawn(
            &[
                "pty",
                "--source",
                "--seed",
                seed_str.as_str(),
                "--bytes",
                total_str.as_str(),
                "--rate",
                rate_str.as_str(),
                "--link",
                dev_str.as_str(),
                "--timeout-ms",
                timeout_ms.as_str(),
                "--hold-ms",
                hold_ms.as_str(),
            ],
            Some(&dev),
        );

        // serial(usb0, free-for-all) -> log(cap): the log captures every hostward byte
        // losslessly and is always read-only toward the device (`write_mode = never`).
        let logdir = run.join("logs");
        std::fs::create_dir_all(&logdir).expect("mkdir log directory");
        let cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
arbitration = "free-for-all"
[[node]]
type = "log"
name = "cap"
directory = "{logdir}"
filename = "soak.log"
[[edge]]
a = "usb0"
b = "cap"
write_mode = "never"
"#,
            dev = dev.display(),
            logdir = logdir.display(),
        );
        rpc.load_toml(&cfg, false).expect("load soak config");
        assert!(
            rpc.wait_status("usb0", "active", Duration::from_secs(20)),
            "usb0 never reached active: {:?}",
            rpc.node("usb0")
        );

        // --- Sampling loop: assert signals (1)(2)(3) every interval for the duration.
        let mut peak_kb: u64 = 0;
        let mut samples: u64 = 0;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(p.seconds) {
            // The daemon must be alive throughout (the bash's `kill -0 $DPID`).
            assert!(
                Path::new(&format!("/proc/{pid}")).exists(),
                "daemon exited mid-soak (after {samples} samples)"
            );

            // (1) Bounded VmRSS — no interior accumulation.
            if let Some(rss) = vmrss_kb(pid) {
                peak_kb = peak_kb.max(rss);
                assert!(
                    rss <= p.rss_budget_kb,
                    "VmRSS {rss} KB exceeded budget {} KB (interior accumulation)",
                    p.rss_budget_kb
                );
            }

            let state = rpc.state();
            // (3) No unexplained faulted nodes.
            let faulted = faulted_nodes(&state);
            assert!(
                faulted.is_empty(),
                "a node faulted mid-soak: {faulted:?} (state: {state})"
            );
            // (2) Every drop/discard/purge counter stays at zero on the keep-up baseline.
            let mut nonzero = Vec::new();
            let sum = loss_counter_sum(&state, &mut nonzero);
            assert_eq!(
                sum, 0,
                "a loss counter grew on the keep-up baseline: {nonzero:?}"
            );

            samples += 1;
            std::thread::sleep(p.interval);
        }
        assert!(samples > 0, "the soak took no samples");

        // --- (4) Checksum reconciliation: the log equals the generator, byte for byte.
        // The log writer drains its queue; wait until it has captured every emitted byte.
        let log = logdir.join("soak.log");
        assert!(
            wait_until(Duration::from_secs(60), || file_len(&log) >= total_bytes),
            "log captured {}/{total_bytes} bytes",
            file_len(&log)
        );
        let log_len = file_len(&log);
        assert_eq!(
            log_len, total_bytes,
            "log length {log_len} != streamed {total_bytes} (bytes lost/duplicated)"
        );
        if total_bytes <= SHA_RECONCILE_CAP {
            let data = std::fs::read(&log).expect("read soak.log");
            let want = sha256_hex(&seeded_bytes(SEED, total_bytes as usize));
            assert_eq!(
                sha256_hex(&data),
                want,
                "log checksum != source checksum (bytes lost/duplicated/reordered)"
            );
        } else {
            eprintln!(
                "NOTE p8_soak: streamed {total_bytes} B exceeds the {SHA_RECONCILE_CAP} B \
                 in-memory sha reconcile cap; asserting exact byte-count reconciliation only \
                 (a streaming-hash API would restore full byte-exactness at scale)"
            );
        }

        eprintln!(
            "p8_soak ok: {}s, {samples} samples, {total_bytes} bytes, rss_peak {peak_kb} KB, \
             budget {} KB",
            p.seconds, p.rss_budget_kb
        );
        assert!(peak_kb > 0, "could not sample daemon RSS at all");
    }
}

/// The always-run smoke: a few seconds of paced load, all four signals asserted. Kept
/// short so the default suite stays fast (the bash's fast CI variant).
#[test]
fn soak_short() {
    #[cfg(target_os = "linux")]
    linux_impl::run_soak(linux_impl::Soak {
        seconds: 3,
        rate_mib: 4,
        interval: std::time::Duration::from_millis(750),
        rss_budget_kb: 150_000,
    });
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP soak_short: needs a software serial source device / /proc VmRSS \
         (Linux-only; a pty cannot be a serial device on macOS)"
    );
}

/// The nightly endurance run: `#[ignore]`d by default, parameterized by the same env
/// knobs as the bash. Streams a long-but-bounded volume (see the module doc on why a
/// literal 24 h / multi-hundred-GB run is not portably expressible here).
#[test]
#[ignore = "endurance soak; run with `--ignored` (nightly)"]
fn soak_endurance() {
    #[cfg(target_os = "linux")]
    {
        fn env_u64(key: &str, default: u64) -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        }
        linux_impl::run_soak(linux_impl::Soak {
            seconds: env_u64("SOAK_SECONDS", 20),
            rate_mib: env_u64("SOAK_RATE_MIB", 4),
            interval: std::time::Duration::from_secs(env_u64("SOAK_INTERVAL", 2)),
            rss_budget_kb: env_u64("SOAK_RSS_KB", 150_000),
        });
    }
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP soak_endurance: needs a software serial source device / /proc VmRSS \
         (Linux-only)"
    );
}
