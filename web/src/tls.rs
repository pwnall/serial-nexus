//! TLS for the §15.29 non-loopback tier (plan §11.6): rustls with the permissive
//! `ring` backend (§13, verified by `cargo deny check licenses`). Either loads an
//! operator-supplied cert + key, or — for lab use — generates a self-signed pair on
//! first run (rcgen) and writes it, so `curl --cacert` and the operator can trust it.
//! The token still gates every request; TLS only makes the token safe to send off
//! loopback (§15.29: the token answers *who may act*, the channel *who may read*).
//!
//! The cert and the key are handled as **one atomic pair**: both present is a load,
//! *neither* present is a generate, and a half-present pair is refused. Generating
//! over half a pair destroys operator material that cannot be regenerated, and the
//! ordinary CA workflow — make the key first, install the signed cert later — walks
//! straight into it (WEB-1).

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// The mode a generated TLS private key is written with: owner-only. The web server's
/// threat model is §15.28's — "a loopback TCP port is reachable by every local user" —
/// so the key must not be readable by any of them.
const KEY_FILE_MODE: u32 = 0o600;

/// Build a rustls server config. If **both** `cert_path` and `key_path` exist, load
/// the PEM pair; if **neither** exists, generate a self-signed cert covering `hosts`,
/// write both files (the key mode 0600), and use it. A half-present pair is an error:
/// the missing half is named and nothing is written (WEB-1).
pub fn build_config(
    cert_path: &Path,
    key_path: &Path,
    hosts: &[String],
) -> anyhow::Result<Arc<ServerConfig>> {
    // `symlink_metadata`, not `exists()`. A dangling symlink planted at either path
    // is *present* for this decision — generating "into" it writes wherever it points
    // — while `exists()` follows the link and calls it absent; `exists()` also reports
    // a path it cannot stat as absent, which would send an unreadable operator key to
    // the generate branch.
    let cert_there = cert_path.symlink_metadata().is_ok();
    let key_there = key_path.symlink_metadata().is_ok();
    let (certs, key) = match (cert_there, key_there) {
        (true, true) => {
            tracing::info!(
                cert = %cert_path.display(),
                key = %key_path.display(),
                "loading TLS cert/key"
            );
            load_pem(cert_path, key_path)?
        }
        (false, false) => {
            // Name the key here, before a byte is written. WEB-1's log line named only
            // the cert while the key was the file being destroyed.
            tracing::info!(
                cert = %cert_path.display(),
                key = %key_path.display(),
                "generating a self-signed TLS cert and private key"
            );
            generate_self_signed(cert_path, key_path, hosts)?
        }
        // Half a pair: refuse rather than complete it. Generating would overwrite the
        // half that is there, and for the key that is unrecoverable (WEB-1).
        (cert_present, _) => {
            let (present, missing) = if cert_present {
                (cert_path, key_path)
            } else {
                (key_path, cert_path)
            };
            anyhow::bail!(
                "TLS needs a complete cert/key pair: {} exists but {} does not. A \
                 self-signed pair is generated only when NEITHER path exists, because \
                 generating writes both files and would overwrite the one already \
                 there — a private key destroyed that way is not recoverable. Supply \
                 the missing file, or point --tls-cert/--tls-key at two paths that do \
                 not exist yet.",
                present.display(),
                missing.display()
            );
        }
    };
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("building the rustls server config")?;
    Ok(Arc::new(config))
}

fn load_pem(
    cert_path: &Path,
    key_path: &Path,
) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let certs = CertificateDer::pem_file_iter(cert_path)
        .with_context(|| format!("reading TLS cert {}", cert_path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parsing TLS cert {}", cert_path.display()))?;
    if certs.is_empty() {
        anyhow::bail!("TLS cert {} contained no certificates", cert_path.display());
    }
    let key = PrivateKeyDer::from_pem_file(key_path)
        .with_context(|| format!("reading TLS key {}", key_path.display()))?;
    Ok((certs, key))
}

/// Generate a self-signed cert for `hosts` (a lab convenience), persist it so it can
/// be trusted, and return the DER material for rustls. Both paths are created with
/// `create_new`, so this can only ever *add* files — never truncate one (WEB-1).
fn generate_self_signed(
    cert_path: &Path,
    key_path: &Path,
    hosts: &[String],
) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    // rcgen puts every SAN in as a DNS name; `localhost` is the one that lets
    // `curl https://localhost:PORT` and a browser opened at localhost validate. IP
    // SANs would need the params API; the operator supplies a real cert for IP or
    // public names (the reason `--tls-cert`/`--tls-key` exist).
    let mut sans: Vec<String> = vec!["localhost".into()];
    for h in hosts {
        // Only DNS-name SANs here (rcgen's simple path); IP hosts (127.0.0.1, ::1,
        // [::1]) are skipped — an operator wanting an IP or public SAN supplies a
        // real cert via --tls-cert/--tls-key.
        let bare = h.trim_matches(|c| c == '[' || c == ']');
        let is_ip = bare.parse::<std::net::IpAddr>().is_ok();
        if !is_ip && h != "localhost" && !sans.contains(h) {
            sans.push(h.clone());
        }
    }
    let certified =
        rcgen::generate_simple_self_signed(sans).context("generating a self-signed cert")?;
    let cert_pem = certified.cert.pem();
    let key_pem = certified.key_pair.serialize_pem();

    write_new(cert_path, cert_pem.as_bytes())
        .with_context(|| format!("writing TLS cert {}", cert_path.display()))?;
    if let Err(e) = write_private(key_path, key_pem.as_bytes()) {
        // The cert was created by *this* call moments ago, so removing it is safe —
        // and it keeps the pair atomic: a cert left beside a missing key would make
        // the next start refuse (`build_config`) rather than retry the generation.
        let _ = std::fs::remove_file(cert_path);
        return Err(e).with_context(|| format!("writing TLS key {}", key_path.display()));
    }

    let cert_der = certified.cert.der().clone();
    let key_der =
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    Ok((vec![cert_der], key_der))
}

/// Create `path` — which must not already exist — and write `bytes` to it.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    std::io::Write::write_all(&mut f, bytes)
}

/// Write a private key with owner-only (0600) permissions into a path that must not
/// already exist.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    // `create_new`, not `create` + `truncate`: an existing file at this path is an
    // error, never a target. That is WEB-1's refusal enforced at the syscall (nothing
    // between the `build_config` check and here can turn a truncation into a create),
    // and it closes the pre-placement hazard the `./serial-nexus-web.key` default makes
    // reachable — another local user planting a file, or a symlink, in a
    // world-writable cwd. `O_EXCL` also refuses to follow a symlink.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(KEY_FILE_MODE)
        .open(path)?;
    // Belt and braces, and not redundant: `mode` applies only at *creation* and is
    // masked by the process umask, so the flag alone makes "0600" a property of the
    // open call rather than of the file — which is exactly how WEB-2 got in (a
    // regenerate into a pre-existing 0644 path served a world-readable key while the
    // code still claimed 0600). Narrowing explicitly makes the mode a fact about the
    // file on disk; `serial-nexus-daemon`'s state-file write does the same for the same
    // reason. It runs before `write_all`, so there is no window at a wider mode.
    f.set_permissions(std::fs::Permissions::from_mode(KEY_FILE_MODE))?;
    std::io::Write::write_all(&mut f, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A private temp directory. pid + a monotonic counter is unique within a run
    /// (same shape as the daemon's `scratch_dir`); no dependency needed for it.
    fn scratch_dir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("snx-tls-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn hosts() -> Vec<String> {
        vec!["localhost".to_string()]
    }

    /// PEM-shaped stand-in for operator key material. Its *bytes* are what the
    /// half-present tests assert on — `build_config` must refuse before anything
    /// parses it, so it does not need to be a real key.
    const OPERATOR_KEY: &[u8] =
        b"-----BEGIN PRIVATE KEY-----\noperator material\n-----END PRIVATE KEY-----\n";
    const OPERATOR_CERT: &[u8] =
        b"-----BEGIN CERTIFICATE-----\noperator material\n-----END CERTIFICATE-----\n";

    /// WEB-1, the destructive shape: the ordinary CA workflow generates the private
    /// key first (`openssl genrsa -out tls.key`) and points `--tls-cert`/`--tls-key`
    /// at the intended pair before the signed cert is installed. `build_config` used
    /// to read that half-present pair as "first run" and `generate_self_signed` then
    /// truncated the key (measured: a 1704-byte RSA key replaced by a 241-byte fresh
    /// PKCS#8 one) while the server came up green. It must refuse, name both paths,
    /// and leave the key byte-identical.
    #[test]
    fn a_present_key_with_a_missing_cert_is_refused_with_the_key_untouched() {
        let dir = scratch_dir();
        let cert = dir.join("fullchain.pem");
        let key = dir.join("tls.key");
        std::fs::write(&key, OPERATOR_KEY).unwrap();

        // `let … else` rather than `expect_err`: a built `ServerConfig` Debug-prints
        // several screens of cipher suites and buries the actual failure.
        let Err(err) = build_config(&cert, &key, &hosts()) else {
            panic!("half a pair must be refused; a config was built and the key truncated");
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains(&key.display().to_string()),
            "the refusal must name the key that exists, got: {msg}"
        );
        assert!(
            msg.contains(&cert.display().to_string()),
            "the refusal must name the cert that is missing, got: {msg}"
        );
        assert_eq!(
            std::fs::read(&key).unwrap(),
            OPERATOR_KEY,
            "the operator's private key was modified"
        );
        assert!(
            !cert.exists(),
            "nothing may be generated from half a pair — a cert appeared at {cert:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// WEB-1, the mirror shape (verifier's shape C): a real cert is in place and the
    /// key path is wrong or not yet populated. `std::fs::write(cert_path, …)` used to
    /// overwrite the cert with a self-signed one, with no warning.
    #[test]
    fn a_present_cert_with_a_missing_key_is_refused_with_the_cert_untouched() {
        let dir = scratch_dir();
        let cert = dir.join("fullchain.pem");
        let key = dir.join("tls.key");
        std::fs::write(&cert, OPERATOR_CERT).unwrap();

        let Err(err) = build_config(&cert, &key, &hosts()) else {
            panic!("half a pair must be refused; a config was built and the cert overwritten");
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains(&cert.display().to_string()) && msg.contains(&key.display().to_string()),
            "the refusal must name both paths, got: {msg}"
        );
        assert_eq!(
            std::fs::read(&cert).unwrap(),
            OPERATOR_CERT,
            "the operator's cert was overwritten"
        );
        assert!(
            !key.exists(),
            "no key may be generated beside an existing cert"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// WEB-2: a pre-existing file at the key path is now an error, not a target.
    /// `OpenOptions::mode()` applies only at creation, so the old `create` + `truncate`
    /// regenerated into a 0644 file and served a world-readable private key while both
    /// the code and the CI gate claimed 0600 (reproduced live). The default paths fall
    /// back to `./serial-nexus-web.{crt,key}` when `XDG_RUNTIME_DIR` is unset, so a local
    /// user pre-creating the file in a world-writable cwd reaches this with no operator
    /// mistake at all.
    #[test]
    fn a_pre_created_world_readable_key_is_refused_not_regenerated_into() {
        let dir = scratch_dir();
        let cert = dir.join("tls.crt");
        let key = dir.join("tls.key");
        std::fs::write(&key, b"").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(
            build_config(&cert, &key, &hosts()).is_err(),
            "a pre-created key path must be refused, never regenerated into"
        );
        assert_eq!(
            std::fs::read(&key).unwrap(),
            b"",
            "the pre-created path received generated key material"
        );
        let mode = std::fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o644,
            "the pre-created file was rewritten (mode {mode:o})"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// WEB-1/WEB-2, the positive half. With neither path present the lab pair is
    /// generated, the key is *exactly* 0600 on disk (not merely opened with that mode
    /// — the explicit narrowing in `write_private` is what makes this exact under any
    /// umask), and a second call takes the load branch: both files come back
    /// byte-identical rather than being regenerated.
    #[test]
    fn generation_happens_only_when_neither_path_exists_and_the_key_is_0600() {
        let dir = scratch_dir();
        let cert = dir.join("tls.crt");
        let key = dir.join("tls.key");

        build_config(&cert, &key, &hosts()).expect("a clean pair of paths must generate");
        let cert_bytes = std::fs::read(&cert).unwrap();
        let key_bytes = std::fs::read(&key).unwrap();
        assert!(!cert_bytes.is_empty() && !key_bytes.is_empty());
        let mode = std::fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "the generated TLS key is mode {mode:o}, want 600"
        );

        build_config(&cert, &key, &hosts()).expect("a complete pair must load");
        assert_eq!(
            std::fs::read(&cert).unwrap(),
            cert_bytes,
            "the load branch rewrote the cert"
        );
        assert_eq!(
            std::fs::read(&key).unwrap(),
            key_bytes,
            "the load branch rewrote the key"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// WEB-1's symlink corner, and the reason presence is decided with
    /// `symlink_metadata` rather than `exists()`: a *dangling* symlink planted at the
    /// key path reports `exists() == false`, so the old code would have taken the
    /// generate branch and written the private key through the link, to a path of the
    /// planter's choosing. It must be refused, and the link's target must not appear.
    #[test]
    fn a_dangling_symlink_at_the_key_path_is_present_not_absent() {
        let dir = scratch_dir();
        let cert = dir.join("tls.crt");
        let key = dir.join("tls.key");
        let elsewhere = dir.join("elsewhere.key");
        std::os::unix::fs::symlink(&elsewhere, &key).unwrap();
        assert!(
            !key.exists(),
            "precondition: a dangling link is `exists() == false`"
        );

        assert!(
            build_config(&cert, &key, &hosts()).is_err(),
            "a planted symlink at the key path must be refused"
        );
        assert!(
            !elsewhere.exists(),
            "the private key was written through the symlink to {elsewhere:?}"
        );
        assert!(!cert.exists(), "nothing may be generated from half a pair");

        std::fs::remove_dir_all(&dir).ok();
    }
}
