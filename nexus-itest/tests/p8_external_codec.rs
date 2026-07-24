//! Out-of-tree (closed-repo) codec template slice, folded from
//! `scripts/validate/phase8/external-codec.sh` into the Rust harness (§16.11; design
//! §15.26 / plan §10.3-10.4). The last of the three surviving shell scripts to retire —
//! its `wait-for.sh` helper is now `wait_until`.
//!
//! The supported way to ship a proprietary codec is **source-level composition** against
//! two semver'd contracts (`codec-api` + `nexus-daemon`), never a dynamically loaded
//! plugin. `examples/external-codec/` is a self-contained workspace standing in for that
//! closed repo: `acme-codec` (a codec depending only on `codec-api`) and `acme-daemon`
//! (the in-tree `serialnexusd` plus one line — `Registry::with_builtins().register("acme",
//! …)`). This test proves the embedding pattern *builds and works from the consumer
//! position* per push rather than by promise:
//!
//! 1. The template workspace **builds from its own manifest** (path deps standing in for
//!    version pins) and its codec passes the `codec-api` conformance kit
//!    (`cargo test -p acme-codec --features conformance`).
//! 2. The custom `acme-daemon` reports its own `acme` codec **alongside** the built-in
//!    `reference` codec via the unchanged `info` RPC (§15.16) — the CLI / RPC surface
//!    never bakes in the codec list.
//! 3. A config naming `codec = "acme"` loads, and the resulting node's state carries
//!    `codec == "acme"` (it comes up `waiting` — no attached mux upstream).
//!
//! Ground truth is structured RPC (`info.codecs`, the codec node's `state` object), never
//! parsed CLI text (§5). Unlike the batch-2b port, this test **builds the template
//! itself** — §16.11 lifts the "the harness may not invoke cargo" constraint — so it is
//! self-contained and the dedicated bash CI job retires with the script.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nexus_itest::{Rpc, TempRun, wait_until};
use serde_json::Value;

/// The excluded template workspace root (`examples/external-codec/`), derived from this
/// crate's compile-time manifest dir (`nexus-itest/`) → the workspace root.
fn template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nexus-itest has a parent (the workspace root)")
        .join("examples")
        .join("external-codec")
}

/// The template builds to its **own** `target/` (it is excluded from the root workspace);
/// we pin `--target-dir` there explicitly so an ambient `CARGO_TARGET_DIR` can't relocate
/// the binary out from under this path.
fn template_target() -> PathBuf {
    template_dir().join("target")
}

fn acme_daemon_bin() -> PathBuf {
    template_target().join("debug").join("acme-daemon")
}

/// A child that is SIGKILLed and reaped on drop, so a panicking test never leaks the
/// custom daemon.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_socket(sock: &Path) -> bool {
    wait_until(Duration::from_secs(10), || {
        std::os::unix::net::UnixStream::connect(sock).is_ok()
    })
}

fn codecs_contains(codecs: &Value, name: &str) -> bool {
    codecs
        .as_array()
        .map(|a| a.iter().any(|c| c.as_str() == Some(name)))
        .unwrap_or(false)
}

#[test]
fn external_codec_template_builds_and_serves_acme_alongside_builtins() {
    let manifest = template_dir().join("Cargo.toml");
    let target = template_target();

    // (1a) Build the closed-repo stand-in from its own manifest, at the consumer position.
    let built = Command::new("cargo")
        .args(["build", "-q", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("run cargo build on the external-codec template");
    assert!(built.success(), "external-codec template build failed");

    // (1b) The template's own conformance-kit test, from the consumer position.
    let conformance = Command::new("cargo")
        .args([
            "test",
            "-q",
            "-p",
            "acme-codec",
            "--features",
            "conformance",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("run the acme conformance-kit test");
    assert!(
        conformance.success(),
        "acme codec conformance-kit test failed"
    );

    let daemon_bin = acme_daemon_bin();
    assert!(
        daemon_bin.exists(),
        "acme-daemon binary not built at {}",
        daemon_bin.display()
    );

    // Hand-managed lifecycle: a *different* binary from `serialnexusd`, with its own
    // `--socket` flag (it derives `<socket>.state.toml` for the state file).
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

    // (2) The custom daemon reports its own codec alongside the built-ins (§15.16).
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

    // (3) A config naming the acme codec loads (it comes up waiting: no attached mux
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

    // Clean shutdown; KillOnDrop is the backstop.
    rpc.shutdown();
    drop(daemon);
}
