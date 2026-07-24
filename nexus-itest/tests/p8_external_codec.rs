//! Out-of-tree (closed-repo) codec template slice, ported from
//! `scripts/validate/phase8/external-codec.sh` (design §15.26 / plan §10.3-10.4).
//!
//! The supported way to ship a proprietary codec is **source-level composition**
//! against two semver'd contracts (`codec-api` + `nexus-daemon`), never a
//! dynamically loaded plugin. `examples/external-codec/` is a self-contained
//! workspace standing in for that closed repo: `acme-codec` (a codec depending only
//! on `codec-api`) and `acme-daemon` (the in-tree `serialnexusd` plus one line —
//! `Registry::with_builtins().register("acme", …)`). This test proves the embedding
//! pattern actually *works at runtime*:
//!
//! 1. The custom `acme-daemon` reports its own `acme` codec **alongside** the
//!    built-in `reference` codec via the unchanged `info` RPC (§15.16) — the CLI /
//!    RPC surface never bakes in the codec list.
//! 2. A config naming `codec = "acme"` loads, and the resulting node's state carries
//!    `codec == "acme"` (it comes up `waiting` — no attached mux upstream).
//!
//! Ground truth is structured RPC (`info.codecs`, the codec node's `state` object),
//! never parsed CLI text (§5).
//!
//! ## Deviations from the bash, and why (each preserves the original *assertions*)
//!
//! * The bash first `cargo build`s the template workspace from the consumer's
//!   position and runs its own conformance-kit test (`cargo test -p acme-codec
//!   --features conformance`). This harness may **not** invoke `cargo`, so those two
//!   *build/conformance* steps are out of scope here — a dedicated CI job
//!   (`external-codec`) builds the template and runs its conformance test per push.
//!   This test instead consumes the already-built `acme-daemon` binary and proves
//!   the *runtime* embedding (info + load), which the bash asserted next.
//! * If the `acme-daemon` binary is absent (the template was not pre-built — e.g. a
//!   plain `cargo test --workspace` that never touches the excluded example), the
//!   test **skips** rather than failing, exactly as a missing serial device skips.
//! * The custom daemon is hand-managed via `std::process::Command` (like the
//!   restart tests in `p3_log.rs`), not `Daemon::start`, because it is a *different*
//!   binary (`acme-daemon`, not `serialnexusd`) with its own CLI (`--socket`).
//!   Readiness is a bounded `UnixStream::connect` poll, not `test -S`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Rpc, TempRun, wait_until};
use serde_json::Value;

/// Absolute path to the pre-built `acme-daemon` binary in the excluded
/// `examples/external-codec/` workspace, derived from this crate's compile-time
/// manifest dir (`nexus-itest/`) → the workspace root. This is the portable
/// replacement for the bash's `REPO_ROOT`/`$TEMPLATE/target/debug/acme-daemon` dance.
/// The template builds to its **own** `target/` (it is excluded from the root
/// workspace), matching the bash's `$TEMPLATE/target/debug`.
fn acme_daemon_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nexus-itest has a parent (the workspace root)")
        .join("examples")
        .join("external-codec")
        .join("target")
        .join("debug")
        .join("acme-daemon")
}

/// A child that is SIGKILLed and reaped on drop, so a panicking test never leaks the
/// custom daemon (mirrors `p3_log.rs`'s `KillOnDrop`).
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Wait until a daemon is actually listening on `sock` (a bound listener accepts the
/// connection). Bounded poll — the restart-safe replacement for `test -S`.
fn wait_socket(sock: &Path) -> bool {
    wait_until(Duration::from_secs(10), || {
        std::os::unix::net::UnixStream::connect(sock).is_ok()
    })
}

/// Whether `codecs` (the `info.codecs` array) contains `name`.
fn codecs_contains(codecs: &Value, name: &str) -> bool {
    codecs
        .as_array()
        .map(|a| a.iter().any(|c| c.as_str() == Some(name)))
        .unwrap_or(false)
}

/// The custom `acme-daemon` serves the `acme` codec alongside the built-in
/// `reference` codec, and a config naming `acme` loads with `codec == "acme"`
/// (§15.26). Needs no serial device — runs on every platform *where the template was
/// pre-built* (else it skips).
#[test]
fn external_codec_daemon_serves_acme_alongside_builtins() {
    let daemon_bin = acme_daemon_bin();
    if !daemon_bin.exists() {
        eprintln!(
            "SKIP external_codec_daemon_serves_acme_alongside_builtins: acme-daemon not built \
             at {} — the CI `external-codec` job builds the excluded template workspace \
             (this harness may not invoke cargo)",
            daemon_bin.display()
        );
        return;
    }

    // Hand-managed lifecycle: this is a *different* binary from `serialnexusd`, with
    // its own `--socket` flag and no `--state-file` (it derives `<socket>.state.toml`).
    let run = TempRun::new();
    let socket = run.socket();
    let daemon = KillOnDrop(
        Command::new(&daemon_bin)
            .arg("--socket")
            .arg(&socket)
            .env("XDG_RUNTIME_DIR", run.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn acme-daemon"),
    );
    assert!(
        wait_socket(&socket),
        "acme-daemon control socket never appeared at {}",
        socket.display()
    );
    let rpc = Rpc::new(socket);

    // (1) The custom daemon reports its own codec alongside the built-ins. The CLI /
    // RPC surface is unchanged (§15.16): the codec list lives only in `info.codecs`.
    let info = rpc.info();
    let codecs = &info["codecs"];
    assert!(
        codecs_contains(codecs, "acme"),
        "the acme codec is not listed by info: {info}"
    );
    assert!(
        codecs_contains(codecs, "reference"),
        "the built-in reference codec is missing from the custom daemon: {info}"
    );

    // (2) A config naming the acme codec loads (it comes up waiting: no attached mux
    // upstream). Assert on the codec node's structured state, never CLI text.
    let cfg = r#"
[[node]]
type = "codec"
name = "mux"
codec = "acme"
faces = "target"
channels = ["console"]
"#;
    rpc.load_toml(cfg, false)
        .expect("acme config failed to load");

    let mux = rpc
        .node("mux")
        .unwrap_or_else(|| panic!("the acme codec node did not load: {}", rpc.state()));
    assert_eq!(
        mux.get("codec").and_then(Value::as_str),
        Some("acme"),
        "the mux node's codec is not \"acme\": {mux}"
    );

    // Clean shutdown, as the bash did (`serialnexusctl shutdown`); KillOnDrop is the
    // backstop.
    rpc.shutdown();
    // Keep the guard alive until here so a panic above still reaps the child.
    drop(daemon);
}
