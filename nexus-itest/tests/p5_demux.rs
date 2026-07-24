//! Phase 5 deterministic demultiplexing, ported from `scripts/validate/phase5/demux.sh`
//! (plan §Phase 5 item 1; design §7.5 codec, §5 loss accounting, §6 held lock,
//! §15.17 no-hardware).
//!
//! `nexus-sim mux` feeds a reference-framed four-channel stream into a "device" PTY; a
//! demux `codec` node (`faces = target`) splits it into per-channel PTYs. Correctness is
//! a byte-exact per-channel checksum comparison against the mux *manifest* — the
//! deterministic oracle giving each channel's delivered byte count + SHA-256 — never a
//! judgement (§5). The properties preserved from the script:
//!
//! 1. Every channel client receives exactly its manifest bytes (`received == 262144`)
//!    and its manifest SHA-256 — the demux split is byte-exact and per-channel.
//! 2. The codec node is `active`, `reference`, and reports its four channels; the
//!    serial's write lock is held by the demux edge (`mux` origin) — the §6 held edge.
//! 3. On a clean stream the codec reports `framing_errors == 0`, and c0's codec-side
//!    `delivered_hostward` equals `payload + primer` (the demux delivers the primer too).
//!
//! The two-phase presence-vs-readiness handshake (plan §3): once every channel client is
//! *present*, release a 256-byte primer (`--prime-file`); once every client has *drained*
//! its primer (`--ready-file`), release the payload burst (`--wait-file`) — so the burst
//! can never outrun a not-yet-reading client. `hostward_buffer = 512` per channel PTY so
//! a briefly-starved client drains losslessly rather than the default bridge shedding.
//!
//! Platform: the multiplexed stream is driven through a `serial` node fed by a `nexus-sim
//! mux` pty double — the pty-as-serial doctrine, which is Linux-only (`serial2` rejects a
//! pty on macOS: `ENOTTY`). The test skips off Linux, where a skip is a valid verdict (§5).

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

    // Script params: SEED=7 BYTES=256KiB (262144 B/channel) PRIMER=256. 256 KiB/channel
    // is 64 frames/channel at the default 4096-byte frame, kept small so the single
    // daemon thread completes it comfortably; correctness (not throughput) is the subject.
    const SEED: &str = "7";
    const BYTES: &str = "256KiB";
    const NBYTES: u64 = 262144;
    const PRIMER: u64 = 256;

    /// The shared mux knobs plus the repeatable `--channel c0 --channel c1 …`.
    fn mux_args() -> Vec<String> {
        let mut v = vec!["--seed".into(), SEED.into(), "--bytes".into(), BYTES.into()];
        for ch in CHANNELS {
            v.push("--channel".into());
            v.push(ch.into());
        }
        v
    }

    /// The manifest's expected SHA-256 for channel `id`.
    fn manifest_sha(manifest: &Value, id: &str) -> String {
        manifest["channels"]
            .as_array()
            .expect("manifest channels array")
            .iter()
            .find(|c| c["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("manifest missing channel {id}: {manifest}"))["sha256"]
            .as_str()
            .expect("channel sha256")
            .to_owned()
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
        assert_eq!(
            manifest["pass"].as_bool(),
            Some(true),
            "mux manifest not clean: {manifest}"
        );
        assert_eq!(
            manifest["corrupted"].as_u64(),
            Some(0),
            "mux manifest reports corrupted frames on a clean stream: {manifest}"
        );

        // ---- Boot the daemon; a `nexus-sim mux` pty double is the serial device ----
        let d = Daemon::start();
        let rpc = d.rpc();
        let run = d.run();
        let dev = run.join("dev"); // the mux's pts symlink the serial node opens
        let prime = run.join("prime"); // phase-1 gate: clients present → send primer
        let go = run.join("go"); // phase-2 gate: clients draining → release the burst

        // Feed mode: create DEV, send a per-channel primer once PRIME exists, then write
        // the payload burst once GO exists. Spawned before the load (Sim::spawn waits for
        // DEV to appear), held in scope so Drop kills it.
        let mut fargs: Vec<String> = vec!["mux".into()];
        fargs.extend(mux_args());
        fargs.extend([
            "--link".into(),
            dev.to_string_lossy().into_owned(),
            "--prime-file".into(),
            prime.to_string_lossy().into_owned(),
            "--prime-bytes".into(),
            PRIMER.to_string(),
            "--wait-file".into(),
            go.to_string_lossy().into_owned(),
            "--timeout-ms".into(),
            "90000".into(),
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
            // hostward_buffer = 512: comfortably holds the whole per-channel burst so a
            // briefly-starved client drains losslessly (this test checks demux
            // correctness, not the drop policy — that is exact-loss/counters).
            cfg.push_str(&format!(
                "[[node]]\ntype = \"pty\"\nname = \"con-{ch}\"\npath = \"{path}\"\nhostward_buffer = 512\n",
                path = run.join(&format!("tty-{ch}")).display(),
            ));
        }
        cfg.push_str("[[edge]]\na = \"usb0\"\nb = \"mux\"\nwrite_mode = \"held\"\n");
        for ch in CHANNELS {
            cfg.push_str(&format!(
                "[[edge]]\na = \"mux/{ch}\"\nb = \"con-{ch}\"\nwrite_mode = \"never\"\n",
            ));
        }
        rpc.load_toml(&cfg, false).expect("load demux graph");

        assert!(
            rpc.wait_status("usb0", "active", Duration::from_secs(20)),
            "usb0 (serial over the mux pty) not active: {:?}",
            rpc.node("usb0")
        );
        assert!(
            rpc.wait_status("mux", "active", Duration::from_secs(10)),
            "codec node not active: {:?}",
            rpc.node("mux")
        );

        // The codec is active, reference, and reports its four channels.
        let mux_node = rpc.node("mux").expect("mux node present");
        assert_eq!(
            mux_node["codec"].as_str(),
            Some("reference"),
            "codec name is not reference: {mux_node}"
        );
        let mut keys: Vec<String> = mux_node["channels"]
            .as_object()
            .expect("codec channels object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            ["c0", "c1", "c2", "c3"],
            "codec did not report its four channels: {mux_node}"
        );

        // The serial's write lock is held by the demux edge (mux origin) — a §6 held
        // edge, so a raw send would be refused.
        assert_eq!(
            rpc.node("usb0").expect("usb0 node")["lock"]["holder"].as_str(),
            Some("mux"),
            "demux edge should hold the serial lock (§6 held): {:?}",
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

        // ---- Attach one receiving client per channel ------------------------------
        // Each discards a --skip 256 primer, creates its --ready-file on the first byte
        // it reads back (proof it is draining), then counts/checksums exactly its
        // 256 KiB payload. Spawned as threads so all four drain concurrently while the
        // burst flows; each returns its client verdict on join. `received`/`sha256` are
        // the demuxed stream; nothing else is judged.
        let handles: Vec<thread::JoinHandle<Value>> = CHANNELS
            .iter()
            .map(|&ch| {
                let path = run
                    .join(&format!("tty-{ch}"))
                    .to_string_lossy()
                    .into_owned();
                let ready = run
                    .join(&format!("ready-{ch}"))
                    .to_string_lossy()
                    .into_owned();
                thread::spawn(move || {
                    Sim::client(&[
                        "--path",
                        &path,
                        "--recv",
                        BYTES,
                        "--skip",
                        "256",
                        "--ready-file",
                        &ready,
                        "--timeout-ms",
                        "90000",
                    ])
                })
            })
            .collect();

        // Phase 1 — once every channel client is present, release the primer.
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
        std::fs::File::create(&prime).expect("touch the PRIME prime-file");

        // Phase 2 — once every client has drained a primer byte (its read loop is live
        // and parked), release the payload burst. This closes the presence-vs-readiness
        // race that would otherwise let the burst outrun a not-yet-reading client.
        for ch in CHANNELS {
            let ready = run.join(&format!("ready-{ch}"));
            assert!(
                wait_until(Duration::from_secs(8), || ready.exists()),
                "channel client con-{ch} never signalled ready (drained the primer)"
            );
        }
        std::fs::File::create(&go).expect("touch the GO wait-file");

        // ---- Property 1: each channel receives exactly its manifest bytes + sha -----
        let verdicts: Vec<Value> = handles
            .into_iter()
            .map(|h| h.join().expect("channel client thread"))
            .collect();
        for (i, &ch) in CHANNELS.iter().enumerate() {
            let v = &verdicts[i];
            let got_n = v["received"].as_u64().unwrap_or(u64::MAX);
            let got_sha = v["sha256"].as_str().unwrap_or("");
            let want_sha = manifest_sha(&manifest, ch);
            assert_eq!(
                got_n, NBYTES,
                "channel {ch}: received {got_n} != {NBYTES}: {v}"
            );
            assert!(
                !got_sha.is_empty(),
                "channel {ch}: no sha256 in verdict: {v}"
            );
            assert_eq!(
                got_sha, want_sha,
                "channel {ch}: demuxed stream did not match its manifest: {v}"
            );
        }

        // ---- Property 2: no framing errors on the clean stream; c0 delivered set ----
        // The codec counts hostward delivery as it fans out, so by the time every client
        // has received its payload the counters are final; a short bounded poll absorbs
        // any state-snapshot lag before the exact asserts. c0's delivered set is the
        // primer plus the payload (the demux delivers the primer too).
        let want_delivered = NBYTES + PRIMER;
        let settled = wait_until(Duration::from_secs(10), || {
            let Some(mux) = rpc.node("mux") else {
                return false;
            };
            mux["framing_errors"].as_u64() == Some(0)
                && mux["channels"]["c0"]["delivered_hostward"].as_u64() == Some(want_delivered)
        });
        let mux = rpc.node("mux").expect("mux codec node in state");
        assert!(
            settled,
            "codec state (framing_errors/delivered) did not settle: {mux}"
        );
        assert_eq!(
            mux["framing_errors"].as_u64(),
            Some(0),
            "codec reported framing errors on a clean stream: {mux}"
        );
        assert_eq!(
            mux["channels"]["c0"]["delivered_hostward"].as_u64(),
            Some(want_delivered),
            "codec c0 delivered_hostward != {want_delivered} (primer + payload): {mux}"
        );
    }
}

/// Deterministic demultiplexing is byte-exact per channel: each channel client receives
/// exactly its mux-manifest bytes and SHA-256, the codec is active/reference with its four
/// channels and the serial's held lock, and on a clean stream framing_errors is zero with
/// c0's delivered set equal to primer + payload (design §7.5 / §5 / §6).
#[test]
fn demux_splits_multichannel_stream_byte_exact() {
    #[cfg(target_os = "linux")]
    linux_impl::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP demux_splits_multichannel_stream_byte_exact: needs a Linux pty-as-serial \
         mux device (serial2 rejects a pty on macOS: ENOTTY)"
    );
}
