#![forbid(unsafe_code)]

//! `acme-daemon` — a custom serial_nexus daemon, standing in for one maintained
//! deployment (§15.26).
//!
//! It is the in-tree `serial-nexus-daemon` plus **one line**: registering the `acme`
//! codec before calling [`serial_nexus_daemon::run`]. Everything else in the ecosystem —
//! `serial-nexus-ctl`, `serial-nexus-sim`, `serial-nexus-doctor`, the validation scripts — works
//! against this binary unchanged, because they speak the RPC surface and the
//! envelope, never the codec list (§15.16). The daemon's internals stay private;
//! this consumer only touches the two semver'd contracts (`serial-nexus-daemon` and
//! `serial-nexus-codec-api`), so it keeps compiling across daemon refactors.

use std::path::PathBuf;

use clap::Parser;
use serial_nexus_codec_api::Codec;
use serial_nexus_daemon::{Registry, RunOptions};

#[derive(Parser)]
#[command(name = "acme-daemon", about = "a custom serial_nexus daemon (§15.26)")]
struct Cli {
    #[arg(long)]
    socket: Option<PathBuf>,
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long, default_value = "/")]
    dev_root: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // The one line an out-of-tree daemon adds: its own codec, registered by name.
    // A name collision with a built-in (or a reserved name) is a startup error, so
    // this `?` refuses before the daemon ever serves traffic (§8/§15.26).
    let registry = Registry::with_builtins()
        .register("acme", |_attributes| {
            let codec: Box<dyn Codec> = Box::new(acme_codec::AcmeCodec::new());
            Ok(codec)
        })?
        // A second codec, registered by chaining — and the shape of a factory that
        // *takes attributes*: the opaque table goes straight to the codec crate's own
        // schema, whose `Err` is a structural load failure with nothing created (§8
        // clause 12, §11). Note what this closure does not do: it does not validate,
        // default, or reinterpret anything. The schema belongs to the codec, which is
        // why the codec's own tests can prove it (plan §18 item 39).
        .register("tinymux", |attributes| {
            let codec: Box<dyn Codec> =
                Box::new(tinymux_codec::TinyMuxCodec::from_attributes(attributes)?);
            Ok(codec)
        })?;

    let options = RunOptions {
        socket: cli.socket,
        config: cli.config,
        dev_root: cli.dev_root,
        ..Default::default()
    };
    serial_nexus_daemon::run(options, registry)
}
