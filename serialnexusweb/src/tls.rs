//! TLS for the §15.29 non-loopback tier (plan §11.6): rustls with the permissive
//! `ring` backend (§13, verified by `cargo deny check licenses`). Either loads an
//! operator-supplied cert + key, or — for lab use — generates a self-signed pair on
//! first run (rcgen) and writes it, so `curl --cacert` and the operator can trust it.
//! The token still gates every request; TLS only makes the token safe to send off
//! loopback (§15.29: the token answers *who may act*, the channel *who may read*).

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Build a rustls server config. If both `cert_path` and `key_path` exist, load the
/// PEM pair; otherwise generate a self-signed cert covering `hosts`, write it to
/// those paths (the key mode 0600), and use it.
pub fn build_config(
    cert_path: &Path,
    key_path: &Path,
    hosts: &[String],
) -> anyhow::Result<Arc<ServerConfig>> {
    let (certs, key) = if cert_path.exists() && key_path.exists() {
        tracing::info!(cert = %cert_path.display(), "loading TLS cert/key");
        load_pem(cert_path, key_path)?
    } else {
        tracing::info!(cert = %cert_path.display(), "generating a self-signed TLS cert");
        generate_self_signed(cert_path, key_path, hosts)?
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
/// be trusted, and return the DER material for rustls.
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

    std::fs::write(cert_path, &cert_pem)
        .with_context(|| format!("writing TLS cert {}", cert_path.display()))?;
    write_private(key_path, key_pem.as_bytes())
        .with_context(|| format!("writing TLS key {}", key_path.display()))?;

    let cert_der = certified.cert.der().clone();
    let key_der =
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    Ok((vec![cert_der], key_der))
}

/// Write a private key with owner-only (0600) permissions.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    std::io::Write::write_all(&mut f, bytes)
}
