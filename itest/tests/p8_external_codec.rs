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
//!    schema — the three suites `tinymux` exists to carry (plan §18 item 39). What is
//!    asserted there is the kit's **execution, by name**, not the run's exit status:
//!    until 2026-08-21 it was the exit status, and a kit dropped from the build left
//!    this test green — a gate hole of AGENTS §3's named class, not a design change,
//!    so it is recorded in the plan's ledger and the notes and nowhere in §15.
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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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

/// The test names libtest reports for one template codec crate, with the `conformance`
/// feature or without it (`cargo test … -- --list`).
///
/// This listing is what makes step (1b)'s expectation *derived*. Nothing in this file
/// hand-keeps a test count or a module name, so adding a suite to the template needs no
/// edit here, renaming `mod conformance` cannot spuriously redden this test, and — the
/// point — removing the kit cannot be papered over by a number that was only ever a
/// transcription of it.
///
/// The mechanism is libtest's own `--list`, and `--format json` was rejected rather than
/// overlooked: the JSON formatter is still `-Z unstable-options`, so a gate built on it
/// would work on a nightly developer box and die on whatever stable toolchain CI ships.
/// `--list`'s `<name>: test` line is what libtest has printed for as long as `--list` has
/// existed (a bench line ends `: bench`, which is why the filter is a suffix rather than
/// a split), and under `-q` it prints nothing else — no `Running unittests …` header, no
/// `N tests, 0 benchmarks` trailer. Neither of those would survive the suffix filter
/// anyway, so the parse does not depend on cargo continuing to pass `-q` down to libtest.
/// Cargo's own chatter is on **stderr** — measured on cargo 1.97.1, both with and without
/// `-q` — so stdout here is libtest and nothing else.
fn listed_tests(
    manifest: &Path,
    target: &Path,
    crate_name: &str,
    conformance: bool,
) -> BTreeSet<String> {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "-q", "--locked", "-p", crate_name]);
    if conformance {
        cmd.args(["--features", "conformance"]);
    }
    cmd.arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target)
        .args(["--", "--list"]);
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("list {crate_name}'s tests (conformance={conformance}): {e}"));
    assert!(
        out.status.success(),
        "listing {crate_name}'s tests failed with conformance={conformance}. The template \
         has to build in *both* feature permutations — the no-feature one is how a \
         consumer builds it by default, and it is the control this step's expectation is \
         derived against.\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_suffix(": test"))
        .map(str::to_owned)
        .collect()
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
    //
    // **The exit status is not the assertion**, and it was until 2026-08-21 — which is
    // AGENTS §3's tell exactly: a kit that passes and a kit that never ran both print
    // exit 0. Measured on the unfixed tree by dropping `--features conformance` from
    // this very invocation: `-p tinymux-codec` ran 3 tests instead of 4 and
    // `-p acme-codec` 1 instead of 2, both exited 0, and this test reported
    // `1 passed; 0 failed`. So the whole conformance kit could leave the build and the
    // gate whose comment says both codecs *pass* it would not notice. AGENTS §11 has the
    // promise this step carries — the extension surface is proven by this template
    // "built from the consumer's position on every push" — and a kit that silently stops
    // running does not carry it. What is asserted below is therefore **execution, by
    // name**.
    for crate_name in ["acme-codec", "tinymux-codec"] {
        // The expectation is derived from the template rather than written down here:
        // list the crate's tests *with* the conformance feature and *without* it, and
        // the difference is exactly the set the kit contributes. No literal floor to
        // carry a measurement for and no module name to go stale — and the derivation
        // is itself the first assertion, because dropping `--features conformance` from
        // the listing makes the two sets coincide.
        //
        // The no-feature listing is not free: it is a second feature permutation of the
        // crate's test target. Measured cold in a scratch `--target-dir` on this box,
        // both crates together: +0.75 s against ~12.9 s for the rest of step (1), and
        // +0.12 s warm, because cargo keys the two permutations to different metadata
        // hashes and keeps both — toggling back and forth recompiles nothing. It also
        // pins a property worth having on its own: the template still builds with the
        // optional test feature *off*, which is how a consumer builds it by default.
        let with_kit = listed_tests(&manifest, &target, crate_name, true);
        let without_kit = listed_tests(&manifest, &target, crate_name, false);
        let kit_tests: Vec<&str> = with_kit
            .difference(&without_kit)
            .map(String::as_str)
            .collect();
        assert!(
            !kit_tests.is_empty(),
            "the `conformance` feature adds no test to {crate_name}: with it the crate \
             lists {with_kit:?}, without it {without_kit:?}. Either the kit is no longer \
             instantiated from the consumer position (§15.26 / plan §10.4), or this \
             invocation stopped asking for it — and either way the extension surface \
             AGENTS §11 says is proven per push is proven by nothing"
        );

        // Run it capturing rather than inheriting, because the per-test lines *are* the
        // evidence and they have to come back here to be asserted on. Capturing is what
        // makes §6's rule load-bearing in the other direction — never filter suite output
        // before the failing test's name is captured — so both streams ride the failure
        // messages below verbatim. That is strictly more than the old `.status()` form
        // gave, which leaked the inner run's whole transcript into this suite's stdout on
        // every *green* run and had nothing but "conformance-kit test failed" on a red one.
        //
        // `-q` is dropped for this one invocation, and that is the whole reason the
        // listing exists as a separate step: quiet libtest prints a dot per test and no
        // names, so a quiet run can report `ok` for having executed something else
        // entirely. The names come from the default human format — `test <name> ... ok` —
        // which libtest has printed since 1.0.
        let conformance = Command::new("cargo")
            .args([
                "test",
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
            .output()
            .unwrap_or_else(|e| panic!("run the {crate_name} conformance-kit test: {e}"));
        let stdout = String::from_utf8_lossy(&conformance.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&conformance.stderr).into_owned();
        assert!(
            conformance.status.success(),
            "{crate_name} conformance-kit test failed\n--- stdout ---\n{stdout}\
             \n--- stderr ---\n{stderr}"
        );
        for name in &kit_tests {
            // A substring, deliberately **not** anchored to the start of a line: this
            // tree has recorded twice that a line anchor over captured test output is
            // unstable (notes §3.78, §3.101 — a splice breaks `^SKIP` and takes the name
            // with it). A libtest result line is unique enough without the anchor, and
            // the ` ... ok` tail is what keeps a test named `foo` from matching
            // `foo_bar`'s line. It is `... ok` rather than just the name because a test
            // that is listed but `ignored`, or filtered out by a stray argument, is
            // precisely the not-running case this assertion exists to redden.
            assert!(
                stdout.contains(&format!("test {name} ... ok")),
                "the conformance kit did not execute: {crate_name} lists `{name}` among \
                 the tests the `conformance` feature adds, but the run reports no \
                 `test {name} ... ok`. Compiling the kit in is not running it (AGENTS §3 \
                 — a gate whose passing output is identical to its not-running output). \
                 The run's own output:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            );
        }
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
