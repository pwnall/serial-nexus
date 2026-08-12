#![forbid(unsafe_code)]

//! Phase 12 (review 32 remediation): the web console's TLS tier treats its cert and
//! key as **one atomic pair**.
//!
//! Pins `WEB-1` (a half-present pair used to take the generate branch, and
//! `generate_self_signed` unconditionally wrote *both* paths — truncating whichever
//! half existed; the measured shape was a 1704-byte operator RSA key replaced by a
//! 241-byte fresh PKCS#8 one while the server came up green and its single log line
//! named only the cert) and, end to end, `WEB-2` (`OpenOptions::mode()` applies only
//! at creation, so regenerating into a pre-existing path served a private key at
//! whatever mode that file had).
//!
//! The byte-level and mode-level assertions live where they are cheapest and most
//! precise — `web/src/tls.rs`'s unit tests, which call `build_config`
//! directly. This file carries the one **end-to-end** shape those cannot: that the
//! shipped binary refuses at startup, names both paths where an operator will see
//! them, exits non-zero, and leaves the key it was pointed at byte-identical. No
//! daemon is needed — the TLS pair is built before the listener binds, and long
//! before any RPC.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;

use serial_nexus_itest::{TempRun, bin, sha256_hex, wait_until};

/// Stand-in for operator key material. `build_config` must refuse before anything
/// parses it, so its *bytes* are the whole subject.
const OPERATOR_KEY: &[u8] = b"-----BEGIN PRIVATE KEY-----\noperator material, not recoverable\n\
      -----END PRIVATE KEY-----\n";

/// WEB-1 end to end, in the shape the verifier found most likely to be reached: the
/// ordinary CA workflow. The operator generates the private key first
/// (`openssl genrsa -out tls.key`) and points `--tls-cert`/`--tls-key` at the intended
/// pair while the signed cert is not yet installed. Before the fix, `serial-nexus-web`
/// read that as "first run", overwrote the key with a throwaway one, printed its
/// bootstrap URL and served normally.
#[test]
fn serial_nexus_web_refuses_a_half_present_tls_pair_and_leaves_the_key_byte_identical() {
    let run = TempRun::new();
    let cert = run.join("fullchain.pem");
    let key = run.join("tls.key");
    std::fs::write(&key, OPERATOR_KEY).expect("plant the operator key");
    let before = sha256_hex(OPERATOR_KEY);

    let cert_str = cert.to_string_lossy().into_owned();
    let key_str = key.to_string_lossy().into_owned();
    let socket_str = run.socket().to_string_lossy().into_owned();
    let mut child = Command::new(bin("serial-nexus-web"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "--socket",
            &socket_str,
            "--token",
            "p12tls",
            "--tls",
            "--tls-cert",
            &cert_str,
            "--tls-key",
            &key_str,
        ])
        .env("XDG_RUNTIME_DIR", run.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn serial-nexus-web");

    // It must refuse *at startup*, so it exits on its own. If it is still running at
    // the deadline it came up over a half pair — which is the defect itself, since
    // coming up is what it did while the key was being destroyed.
    let exited = wait_until(Duration::from_secs(20), || {
        matches!(child.try_wait(), Ok(Some(_)))
    });
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "serial-nexus-web came up with a half-present TLS pair (WEB-1): it must refuse \
             rather than generate over the operator's key"
        );
    }
    let status = child.wait().expect("wait for serial-nexus-web");
    assert!(
        !status.success(),
        "a half-present TLS pair must be a startup failure, got {status}"
    );

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("piped serial-nexus-web stderr")
        .read_to_string(&mut stderr)
        .expect("read serial-nexus-web stderr");
    assert!(
        stderr.contains(&key_str),
        "the refusal must name the key that exists — the operator has to know which \
         file it kept. stderr was: {stderr}"
    );
    assert!(
        stderr.contains(&cert_str),
        "the refusal must name the cert that is missing — that is the file to supply. \
         stderr was: {stderr}"
    );

    // The whole point: the key is untouched, to the byte.
    let after = std::fs::read(&key).expect("read the operator key back");
    assert_eq!(
        sha256_hex(&after),
        before,
        "the operator's private key was rewritten ({} bytes now, {} before)",
        after.len(),
        OPERATOR_KEY.len()
    );
    assert!(
        !cert.exists(),
        "nothing may be generated from half a pair, but a cert appeared at {cert:?}"
    );
}
