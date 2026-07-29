#![forbid(unsafe_code)]

//! `serial-nexus-web`'s binary: flag parsing, the tracing subscriber, and the runtime.
//! Everything else lives in the library beside it (see `lib.rs` for the design
//! rationale, §17) — the same thin-binary shape `serial-nexus-daemon` has (§15.26).

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "serial-nexus-web",
    version,
    about = "serial_nexus web console (§17): a loopback browser UI over the daemon's RPC surface"
)]
struct Cli {
    #[command(flatten)]
    serve: ServeArgs,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Args)]
struct ServeArgs {
    /// Address to bind the HTTP/WS server to. Loopback by default; a non-loopback
    /// bind is refused unless `--tls` (the sanctioned mode) or `--insecure-bind`
    /// (the named footgun) is set (§15.29).
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,
    /// The daemon control socket to drive (defaults match the daemon, §10).
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Serve TLS (rustls) so a non-loopback bind is safe — the mode in which the
    /// bearer token is genuinely equivalent to an API key, because it then rides an
    /// encrypted channel (§15.29). A self-signed cert is generated on first run for
    /// lab use; supply `--tls-cert`/`--tls-key` for a real one.
    #[arg(long)]
    tls: bool,
    /// TLS certificate chain (PEM). The cert and key are one pair: both paths present
    /// are loaded, NEITHER present generates a self-signed pair into them (lab use),
    /// and a half-present pair is refused — never completed by overwriting the half
    /// that is there. Defaults to a runtime-dir path.
    #[arg(long)]
    tls_cert: Option<PathBuf>,
    /// TLS private key (PEM), written 0600 when generated. See `--tls-cert` for how
    /// the pair is treated.
    #[arg(long)]
    tls_key: Option<PathBuf>,
    /// Bind beyond loopback WITHOUT TLS — a named footgun for a network the operator
    /// genuinely trusts (§15.29). The token stays mandatory, but EVERY console byte,
    /// AND the token itself, is readable and replayable by anyone on the path. Prefer
    /// `--tls`, or SSH port forwarding of the loopback default.
    #[arg(long)]
    insecure_bind: bool,
    /// Override the per-session bearer token (default: a fresh 256-bit random token
    /// printed as a ready-to-open URL). Mainly for tests and scripted access.
    #[arg(long)]
    token: Option<String>,
    /// Extra Host header values to accept beyond localhost/127.0.0.1/[::1] (§15.29
    /// off-loopback Host validation), e.g. a hostname used behind `--tls`.
    #[arg(long = "host", value_name = "HOST")]
    hosts: Vec<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Connect to a running web server's WebSocket as a headless client, tap one
    /// console, and checksum the byte stream — the browser-facing protocol exercised
    /// end to end without a browser (plan §11.3 validation).
    Wsclient(serial_nexus_web::WsclientArgs),
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    match cli.cmd {
        // The headless test client is linear; no local tasks.
        Some(Cmd::Wsclient(args)) => rt.block_on(serial_nexus_web::wsclient(args)),
        // The server spawns `!Send` per-connection and bridge tasks (the daemon
        // socket is `!Send` through the bridge), so it runs on a `LocalSet` — the
        // same single-thread shape the daemon uses (§15.18).
        None => {
            let local = tokio::task::LocalSet::new();
            rt.block_on(local.run_until(serve(cli.serve)))
        }
    }
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    // Resolve the bind address and enforce the §15.29 three-tier policy before we
    // touch the network.
    let addr: SocketAddr = args
        .bind
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("invalid --bind {:?}: {e}", args.bind))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("--bind {:?} resolved to no address", args.bind))?;

    // Off loopback, exactly one of the two sanctioned modes is required: `--tls` (the
    // encrypted mode where the token analogy holds) or `--insecure-bind` (the named
    // footgun). Plaintext beyond loopback with neither is refused.
    if !addr.ip().is_loopback() && !args.tls && !args.insecure_bind {
        anyhow::bail!(
            "refusing to bind {} beyond loopback without --tls or --insecure-bind \
             (§15.29): a plaintext non-loopback bind broadcasts every console byte \
             and the bearer token to anyone on the path. Use --tls (recommended), \
             the loopback default with SSH forwarding, or --insecure-bind for a \
             network you genuinely trust.",
            addr
        );
    }
    if !addr.ip().is_loopback() && args.insecure_bind && !args.tls {
        // The named footgun states its cost in its own output (§15.29).
        tracing::warn!(
            "INSECURE BIND: {} is not loopback and TLS is off. Every console byte, \
             and the bearer token itself, is readable and replayable by anyone on \
             the network path. This is a footgun; prefer --tls or SSH forwarding.",
            addr
        );
    }

    let token = match args.token {
        Some(t) => t,
        None => new_token(),
    };
    let socket = serial_nexus_web::resolve_socket(args.socket);

    // Host values accepted off loopback (DNS-rebinding defense, §15.29): always the
    // localhost family, plus any operator-declared names.
    let mut hosts: Vec<String> = vec![
        "localhost".into(),
        "127.0.0.1".into(),
        "[::1]".into(),
        "::1".into(),
    ];
    hosts.extend(args.hosts);

    // Build the TLS config for the encrypted tier (§15.29), if requested. A default
    // cert/key path lives in the runtime dir when the operator names none.
    let tls = if args.tls {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| ".".into());
        let cert = args
            .tls_cert
            .unwrap_or_else(|| PathBuf::from(&runtime).join("serial-nexus-web.crt"));
        let key = args
            .tls_key
            .unwrap_or_else(|| PathBuf::from(&runtime).join("serial-nexus-web.key"));
        Some(serial_nexus_web::build_tls_config(&cert, &key, &hosts)?)
    } else {
        None
    };

    let config = serial_nexus_web::ServerConfig {
        token,
        socket,
        hosts,
    };
    // `serve` prints the bootstrap URL after binding, so an ephemeral `:0`
    // request reports the port the OS actually chose.
    serial_nexus_web::serve(addr, config, tls).await
}

/// A fresh 256-bit bearer token, hex-encoded (§15.29). `getrandom` reads the OS CSPRNG.
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    // getrandom 0.4 renamed the free function; same OS RNG, same failure discipline.
    getrandom::fill(&mut bytes).expect("OS RNG");
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
