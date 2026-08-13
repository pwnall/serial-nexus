#![forbid(unsafe_code)]

//! Out-of-tree codec template slice, folded from
//! `scripts/validate/phase8/external-codec.sh` into the Rust harness (§16.11; design
//! §15.26 / plan §10.3-10.4). The last of the three surviving shell scripts to retire —
//! its `wait-for.sh` helper is now `wait_until`.
//!
//! The supported way to ship a proprietary codec is **source-level composition** against
//! two semver'd contracts (`serial-nexus-codec-api` + `serial-nexus-daemon`), never a dynamically loaded
//! plugin. `examples/external-codec/` is a self-contained workspace standing in for that
//! out-of-tree repository: two codec crates depending only on `serial-nexus-codec-api`
//! — `acme-codec` (a passthrough) and `tinymux-codec` (a two-channel tag framer with
//! parser state and one attribute) — plus `acme-daemon` (the in-tree
//! `serial-nexus-daemon` plus two chained `Registry::with_builtins().register(…)` calls).
//! This test proves the embedding pattern *builds and works from the consumer position*
//! per push rather than by promise:
//!
//! 1. The template workspace **builds from its own manifest** (path deps standing in for
//!    version pins) and **both** codecs pass the `serial-nexus-codec-api` conformance kit
//!    (`cargo test -p <crate> --features conformance`). Two crates, because a passthrough
//!    cannot exercise the control-event round-trip, the buffer bound, or the attribute
//!    schema — the three suites `tinymux` exists to carry (plan §18 item 39).
//! 2. The custom `acme-daemon` reports **its own** codecs **alongside** the built-in
//!    `reference` codec via the unchanged `info` RPC (§15.16) — the CLI / RPC surface
//!    never bakes in the codec list. The *whole* list is pinned as well, because the
//!    template's README prints it as what an embedder should expect and a containment
//!    check cannot see it drift (37-DOC-4).
//! 3. A config naming `codec = "acme"` loads, and the resulting node's state carries
//!    `codec == "acme"` (it comes up `waiting` — no attached mux upstream).
//! 4. A `codec = "tinymux"` node loads **with its attributes**, and an unknown attribute
//!    key is refused *naming the key* with nothing created — the out-of-tree half of §11's
//!    pre-create precheck: the daemon never sees that schema, the codec crate owns it,
//!    and the operator still gets a sentence pointing at the mistake (§8 clause 12).
//!
//! Ground truth is structured RPC (`info.codecs`, the codec node's `state` object), never
//! parsed CLI text (§5). Unlike the batch-2b port, this test **builds the template
//! itself** — §16.11 lifts the "the harness may not invoke cargo" constraint — so it is
//! self-contained and the dedicated bash CI job retires with the script.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use serial_nexus_itest::{KillOnDrop, Rpc, TempRun, wait_daemon_ready};

/// The excluded template workspace root (`examples/external-codec/`), derived from this
/// crate's compile-time manifest dir (`itest/` — the directory, §15.40) → the workspace root.
fn template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("serial-nexus-itest has a parent (the workspace root)")
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

    // (1a) Build the out-of-tree stand-in from its own manifest, at the consumer position.
    // `--locked` is not decoration: `examples/external-codec/Cargo.lock` is committed, and
    // without the flag a drifted lock is silently regenerated here — the template would
    // build against whatever crates.io serves today while reporting green, which is the
    // opposite of what a committed lock is for. Same reasoning as `--locked` everywhere
    // else in the workspace (plan §2); a stale template lock must fail loudly.
    let built = Command::new("cargo")
        .args(["build", "-q", "--locked", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("run cargo build on the external-codec template");
    assert!(built.success(), "external-codec template build failed");

    // (1b) The template's own conformance-kit tests, from the consumer position — also
    // `--locked`, so the conformance run cannot resolve a different graph than (1a).
    // Both codec crates: `acme` is the passthrough that proves the embedding pattern,
    // `tinymux` the two-channel framer that proves the three suites a passthrough
    // cannot reach — the control-event round-trip, the buffer bound, and the attribute
    // schema (plan §18 item 39). Running only the first is what left those three with
    // no consumer at all.
    for crate_name in ["acme-codec", "tinymux-codec"] {
        let conformance = Command::new("cargo")
            .args([
                "test",
                "-q",
                "--locked",
                "-p",
                crate_name,
                "--features",
                "conformance",
                "--manifest-path",
            ])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .status()
            .unwrap_or_else(|e| panic!("run the {crate_name} conformance-kit test: {e}"));
        assert!(
            conformance.success(),
            "{crate_name} conformance-kit test failed"
        );
    }

    let daemon_bin = acme_daemon_bin();
    assert!(
        daemon_bin.exists(),
        "acme-daemon binary not built at {}",
        daemon_bin.display()
    );

    // Hand-managed lifecycle: a *different* binary from `serial-nexus-daemon`, with its own
    // `--socket` flag (it derives `<socket-stem>.state.toml` for the state file —
    // the socket's *stem*, review 32 RV-6).
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
    // `RawDaemon` cannot boot this one — it is a *different* binary, the out-of-tree
    // template's own daemon — but readiness is the same question, so it is asked with
    // the same shared answer rather than a local `test -S` substitute. It is also the
    // one daemon in this suite with no §15.43 leash: the flag is the consumer's to add.
    assert!(
        wait_daemon_ready(&socket),
        "acme-daemon control socket never answered at {}",
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
    // The whole list, not just membership: `examples/external-codec/README.md` prints
    // this exact array as what an embedder should expect, and the containment asserts
    // above cannot catch it drifting. `exec` is in it without being a registry entry —
    // it is a child-process boundary routed before the registry (§7.6/§15.22) whose
    // name is reserved, and `info` answers "which names may a config use", so it
    // unions the reserved names in (`Registry::usable_codec_names`). Omitting it is
    // what the README did (review 37, 37-DOC-4).
    assert_eq!(
        codecs,
        &json!(["acme", "exec", "reference", "tinymux"]),
        "the custom daemon's info.codecs is not the list the template README prints \
         (37-DOC-4): {info}"
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

    // (4) The second template codec takes an **attribute**, so it exercises the half
    // of the embedding contract `acme` cannot: the opaque table reaches the codec's own
    // schema through the registry factory, and the schema's verdict is the load's
    // verdict (§8 clause 12, §11). A good table loads…
    let good = r#"
[[node]]
type = "codec"
name = "tiny"
codec = "tinymux"
faces = "target"
channels = ["console", "trace"]

  [node.attributes]
  channels = ["console", "trace"]
"#;
    rpc.add_node_toml(good)
        .expect("the tinymux node failed to load");
    let tiny = rpc
        .node("tiny")
        .unwrap_or_else(|| panic!("the tinymux codec node did not load: {}", rpc.state()));
    assert_eq!(
        tiny.get("codec").and_then(Value::as_str),
        Some("tinymux"),
        "the tiny node's codec is not \"tinymux\": {tiny}"
    );

    // …and a bad one is refused **naming the key**, with nothing created. This is the
    // out-of-tree half of the pre-create precheck contract (§11): the daemon never
    // sees the schema, the codec crate owns it, and the operator still gets a sentence
    // that points at the mistake.
    let bad = r#"
[[node]]
type = "codec"
name = "tiny2"
codec = "tinymux"
channels = ["console"]

  [node.attributes]
  channels = ["console"]
  widgets = 3
"#;
    let err = rpc
        .add_node_toml(bad)
        .expect_err("an unknown tinymux attribute key must be refused");
    let text = format!("{err:?}");
    assert!(
        text.contains("widgets"),
        "the refusal must name the offending key (§8 clause 12): {text}"
    );
    assert!(
        rpc.node("tiny2").is_none(),
        "a refused load created a node anyway — §11 promises nothing is created: {}",
        rpc.state()
    );

    // Clean shutdown; KillOnDrop is the backstop.
    rpc.shutdown();
    drop(daemon);
}
