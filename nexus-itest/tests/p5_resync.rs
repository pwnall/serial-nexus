//! Phase 5 resynchronization accounting, ported from `scripts/validate/phase5/resync.sh`
//! (plan §Phase 5 item 2; design §7.5 framing / §9 length-guided resync).
//!
//! `nexus-sim mux --corrupt-every N` mangles one in every N emitted frames' type byte
//! while leaving the length prefix intact; the reference demux codec skips exactly that
//! frame and resyncs by frame length. Two provable properties, both checked against the
//! mux *manifest* — the deterministic oracle giving each channel's expected-loss set
//! (surviving byte count + SHA-256) and the total corrupted-frame count:
//!
//! 1. Each channel's recovered stream (delivered byte count **and** SHA-256) equals the
//!    manifest's expected-loss set — recovery after garbage is provably byte-exact.
//! 2. The codec's framing-error (resync) counter equals the manifest's corruption
//!    count, and each channel's codec-side `delivered_hostward` (the demux's own count,
//!    before any consumer-boundary drop) equals the manifest — deterministic proof of
//!    exact recovery.
//!
//! Ground truth is the manifest oracle (byte counts + SHA-256), never a judgement (§5).
//!
//! Platform: the multiplexed stream is driven through a `serial` node fed by a
//! `nexus-sim mux` pty double — the pty-as-serial doctrine, which is Linux-only
//! (`serial2` rejects a pty on macOS: `ENOTTY`). The test skips off Linux, where a skip
//! is a valid verdict (§5).

/// Channels, matching the script's `CHANNELS=(c0 c1 c2 c3)`.
#[cfg(target_os = "linux")]
const CHANNELS: [&str; 4] = ["c0", "c1", "c2", "c3"];

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    use nexus_itest::{Daemon, Sim, bin, wait_until};
    use serde_json::Value;

    use super::CHANNELS;

    // Script params: SEED=7 BYTES=512KiB FRAME=4096 CORRUPT=30. Frame 4096 keeps the
    // per-64KiB-read chunk count under the PTY bridge depth, so the demuxed burst is not
    // slow-consumer-dropped at the channel boundary — the client then receives exactly
    // the codec's delivered set (§5).
    const SEED: &str = "7";
    const BYTES: &str = "512KiB";
    const FRAME: &str = "4096";
    const CORRUPT: &str = "30";

    /// The shared mux knobs plus the repeatable `--channel c0 --channel c1 …`, exactly
    /// as the script's `MUXARGS`.
    fn mux_args() -> Vec<String> {
        let mut v = vec![
            "--seed".into(),
            SEED.into(),
            "--bytes".into(),
            BYTES.into(),
            "--frame-size".into(),
            FRAME.into(),
            "--corrupt-every".into(),
            CORRUPT.into(),
        ];
        for ch in CHANNELS {
            v.push("--channel".into());
            v.push(ch.into());
        }
        v
    }

    /// The manifest's `(delivered, sha256)` for channel `id`.
    fn manifest_channel(manifest: &Value, id: &str) -> (u64, String) {
        let ch = manifest["channels"]
            .as_array()
            .expect("manifest channels array")
            .iter()
            .find(|c| c["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("manifest missing channel {id}: {manifest}"));
        (
            ch["delivered"].as_u64().expect("channel delivered"),
            ch["sha256"].as_str().expect("channel sha256").to_owned(),
        )
    }

    pub fn run() {
        // ---- The manifest oracle: `nexus-sim mux --manifest` (no PTY, then exit) ----
        let mut margs: Vec<String> = vec!["mux".into(), "--manifest".into()];
        margs.extend(mux_args());
        let out = Command::new(bin("nexus-sim"))
            .args(&margs)
            .output()
            .expect("run nexus-sim mux --manifest");
        let manifest: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "parse mux manifest: {e}; stdout={:?} stderr={:?}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        let corrupted = manifest["corrupted"]
            .as_u64()
            .expect("manifest reports a corrupted count");
        assert!(
            corrupted > 0,
            "manifest reports no corruption; pick different params: {manifest}"
        );

        // ---- Boot the daemon; a `nexus-sim mux` pty double is the serial device ----
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();
        let dev = run.join("dev"); // the mux's pts symlink the serial node opens
        let go = run.join("go"); // the --wait-file gate that releases the burst

        // Feed mode: create DEV, wait for GO, then write the framed stream and hold the
        // device present. Spawned before the load (Sim::spawn waits for DEV to appear),
        // held in scope so Drop kills it.
        let mut fargs: Vec<String> = vec!["mux".into()];
        fargs.extend(mux_args());
        fargs.extend([
            "--link".into(),
            dev.to_string_lossy().into_owned(),
            "--wait-file".into(),
            go.to_string_lossy().into_owned(),
            "--timeout-ms".into(),
            "30000".into(),
        ]);
        let fargs_ref: Vec<&str> = fargs.iter().map(String::as_str).collect();
        let _mux = Sim::spawn(&fargs_ref, Some(&dev));

        // ---- The graph: serial → held demux codec → one never-write pty per channel --
        let mut cfg = format!(
            r#"
[[node]]
type = "serial"
name = "usb0"
device = "{dev}"
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0", "c1", "c2", "c3"]
"#,
            dev = dev.display(),
        );
        for ch in CHANNELS {
            cfg.push_str(&format!(
                "[[node]]\ntype = \"pty\"\nname = \"con-{ch}\"\npath = \"{path}\"\n",
                path = run.join(&format!("tty-{ch}")).display(),
            ));
        }
        cfg.push_str("[[edge]]\na = \"usb0\"\nb = \"mux\"\nwrite_mode = \"held\"\n");
        for ch in CHANNELS {
            cfg.push_str(&format!(
                "[[edge]]\na = \"mux/{ch}\"\nb = \"con-{ch}\"\nwrite_mode = \"never\"\n",
            ));
        }
        rpc.load_toml(&cfg, false).expect("load resync graph");

        assert!(
            rpc.wait_status("usb0", "active", Duration::from_secs(20)),
            "usb0 (serial) not active: {:?}",
            rpc.node("usb0")
        );

        // The pty channel symlinks appear shortly after load; wait for each before
        // attaching its client so the open never races the symlink into existence.
        for ch in CHANNELS {
            let tty = run.join(&format!("tty-{ch}"));
            assert!(
                wait_until(Duration::from_secs(5), || tty.exists()),
                "pty channel symlink never appeared: {}",
                tty.display()
            );
        }

        // ---- Attach fully-draining clients, one per channel ------------------------
        // Under corruption delivered < sent, so each reads until quiet (no bytes for
        // --quiet-ms) rather than to a fixed count. Spawned as threads so all four
        // drain concurrently while the burst flows; each returns its client verdict on
        // join. `received`/`sha256` are the recovered stream; nothing else is judged.
        let handles: Vec<thread::JoinHandle<Value>> = CHANNELS
            .iter()
            .map(|&ch| {
                let path = run
                    .join(&format!("tty-{ch}"))
                    .to_string_lossy()
                    .into_owned();
                thread::spawn(move || {
                    Sim::client(&[
                        "--path",
                        &path,
                        "--drain",
                        "--quiet-ms",
                        "700",
                        "--timeout-ms",
                        "30000",
                    ])
                })
            })
            .collect();

        // Wait for every channel's client to be present, then release the burst — the
        // presence-then-GO ordering keeps the initial burst from outrunning a not-yet-
        // attached reader (§5, plan §3).
        for ch in CHANNELS {
            let node = format!("con-{ch}");
            let present = wait_until(Duration::from_secs(8), || {
                rpc.node(&node).and_then(|n| n["client_present"].as_bool()) == Some(true)
            });
            assert!(
                present,
                "channel client {node} never became present: {:?}",
                rpc.node(&node)
            );
        }
        std::fs::File::create(&go).expect("touch the GO wait-file");

        // ---- Property 1: each recovered stream matches the manifest's loss set -----
        let verdicts: Vec<Value> = handles
            .into_iter()
            .map(|h| h.join().expect("drain client thread"))
            .collect();
        for (i, &ch) in CHANNELS.iter().enumerate() {
            let (want_n, want_sha) = manifest_channel(&manifest, ch);
            let v = &verdicts[i];
            let got_n = v["received"].as_u64().unwrap_or(u64::MAX);
            let got_sha = v["sha256"].as_str().unwrap_or("");
            assert_eq!(
                got_n, want_n,
                "channel {ch}: received {got_n} != manifest delivered {want_n}: {v}"
            );
            assert_eq!(
                got_sha, want_sha,
                "channel {ch}: recovered checksum != manifest (lossy/misaligned recovery): {v}"
            );
        }

        // ---- Property 2: framing_errors == corrupted; codec delivered == manifest --
        // The codec counts hostward delivery as it fans out, so by the time every drain
        // client has gone quiet the counters are final; a short bounded poll absorbs any
        // state-snapshot lag before the exact asserts.
        let settled = wait_until(Duration::from_secs(5), || {
            let Some(mux) = rpc.node("mux") else {
                return false;
            };
            if mux["framing_errors"].as_u64() != Some(corrupted) {
                return false;
            }
            CHANNELS.iter().all(|&ch| {
                let (want_n, _) = manifest_channel(&manifest, ch);
                mux["channels"][ch]["delivered_hostward"].as_u64() == Some(want_n)
            })
        });
        let mux = rpc.node("mux").expect("mux codec node in state");
        assert!(
            settled,
            "codec counters did not settle to the manifest; state={mux}"
        );
        assert_eq!(
            mux["framing_errors"].as_u64(),
            Some(corrupted),
            "framing_errors {:?} != manifest corrupted frames {corrupted}",
            mux["framing_errors"]
        );
        for ch in CHANNELS {
            let (want_n, _) = manifest_channel(&manifest, ch);
            let got = mux["channels"][ch]["delivered_hostward"].as_u64();
            assert_eq!(
                got,
                Some(want_n),
                "codec delivered_hostward[{ch}] {got:?} != manifest {want_n}: {mux}"
            );
        }
    }
}

/// Resynchronization past corrupt frames is accounted, not approximate: each channel's
/// recovered stream matches the mux manifest byte-exact, the codec's framing-error
/// counter equals the corruption count, and each channel's codec-side delivered count
/// equals the manifest (design §7.5 / §9).
#[test]
fn resync_past_corruption_is_byte_exact_and_accounted() {
    #[cfg(target_os = "linux")]
    linux_impl::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP resync_past_corruption_is_byte_exact_and_accounted: needs a Linux \
         pty-as-serial mux device (serial2 rejects a pty on macOS: ENOTTY)"
    );
}
