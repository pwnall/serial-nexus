#![forbid(unsafe_code)]

//! Phase 0 licensing gate, folded from `scripts/validate/phase0/license-gate.sh` into
//! the Rust harness (§16.11). Proves the §13 permissive-only policy actually *rejects* a
//! banned crate rather than merely being configured to (plan §2: "the gate is proven,
//! not assumed"): the clean workspace passes `cargo deny check bans`, and a scratch crate
//! that pulls in the banned `serialport` fails it.
//!
//! `cargo-deny` is the precondition, not the subject — the dedicated CI gate installs it.
//! Where it is absent the test **self-skips** with a valid verdict (§13/§15.17, the same
//! skip discipline as the doctor's `skipped(no adapter)`), so it runs wherever the tool
//! exists and never blocks a machine without it.
//!
//! **Three outcomes, not two.** The gate has to separate *ban hit* (pass) from *ban list
//! gutted* (fail — the thing it exists to catch) from *could not evaluate* (skip, loudly,
//! naming why), and the third is not hypothetical: step 2 resolves `serialport` from the
//! crates.io index, which an air-gapped or index-throttled runner cannot do. Collapsing
//! that into either of the other two is a bug in both directions — as "not a ban hit" it
//! reddens CI for a network outage, and as "close enough to a rejection" it is the
//! vacuity review 32 `TESTR-2` filed. So the evaluability question is answered by its own
//! probe (`cargo metadata` on the scratch crate) rather than by grepping cargo-deny's
//! prose, and the skip honours **`SNX_LICENSE_GATE=required`** — the `p8_web_ui` shape:
//! optional on a laptop, mandatory on the CI job that provisions the network and the tool
//! (plan §3 rule 7, "a gate that can skip silently is a gate CI passes over a hole").

use std::path::{Path, PathBuf};
use std::process::Command;

use serial_nexus_itest::TempRun;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

fn have_cargo_deny() -> bool {
    Command::new("cargo")
        .args(["deny", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Can this runner resolve the scratch crate's dependency graph at all?
///
/// The discriminator between "the ban list did not fire" and "nothing could be checked",
/// answered mechanically instead of by matching cargo's wording: `cargo deny check bans`
/// builds its graph by shelling out to `cargo metadata`, so a `cargo metadata` that fails
/// here is exactly the input cargo-deny will fail on, and one that succeeds leaves the
/// lock file behind that cargo-deny then reuses. Measured both ways on 2026-07-27: online
/// it exits 0 and cargo-deny then exits **2** with three `error[banned]` diagnostics;
/// with an empty `CARGO_HOME` and `CARGO_NET_OFFLINE=true` it exits **101** ("no matching
/// package named `serialport` found") and cargo-deny exits **1** having consulted no ban
/// list.
///
/// Returns the failure text when the graph cannot be built, so the skip names the cause.
fn resolution_failure(manifest: &Path) -> Option<String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .output()
        .ok()?;
    (!out.status.success()).then(|| String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn cargo_deny_ban_list_rejects_a_banned_crate() {
    let required = std::env::var("SNX_LICENSE_GATE").as_deref() == Ok("required");
    let skip = |why: &str, fix: &str| {
        assert!(
            !required,
            "SNX_LICENSE_GATE=required, but {why}. Fix: {fix}\n\
             (This job is expected to prove the §13 ban list actually fires; a skip here \
             would be a gate passing over a hole — plan §3 rule 7.)"
        );
        eprintln!("SKIP cargo_deny_ban_list_rejects_a_banned_crate: {why} ({fix})");
    };

    if !have_cargo_deny() {
        return skip(
            "cargo-deny is not installed",
            "cargo install --locked cargo-deny",
        );
    }

    // The whole gate rests on cargo being able to build a dependency graph at all, and
    // *both* steps need one. Step 1 usually gets there without the network — the
    // workspace `Cargo.lock` is committed — but only against a warm `CARGO_HOME`; with a
    // cold one it cannot resolve either, which is precisely the air-gapped shape the
    // audit reproduced. Probe it here so that runner skips instead of reporting that this
    // tree has grown a banned dependency.
    let workspace_manifest = PathBuf::from(format!("{REPO}/Cargo.toml"));
    if let Some(why) = resolution_failure(&workspace_manifest) {
        return skip(
            &format!(
                "this runner cannot resolve the workspace dependency graph, so neither \
                 half of the gate can run:\n{}",
                why.trim_end()
            ),
            "give this runner crates.io access (unset CARGO_NET_OFFLINE / warm CARGO_HOME)",
        );
    }

    // 1. The clean tree passes the ban check (offline: the workspace Cargo.lock is
    //    committed, so no index fetch is needed).
    let clean = Command::new("cargo")
        .args(["deny", "--manifest-path"])
        .arg(format!("{REPO}/Cargo.toml"))
        .args(["check", "bans"])
        .output()
        .expect("run cargo deny on the clean tree");
    assert!(
        clean.status.success(),
        "the clean tree unexpectedly fails the ban check:\n{}",
        String::from_utf8_lossy(&clean.stderr)
    );

    // 2. A scratch crate pulling in the banned `serialport` must fail the ban check.
    let scratch = TempRun::new();
    let proj = scratch.join("banned");
    let created = Command::new("cargo")
        .args(["new", "--quiet", "--bin"])
        .arg(&proj)
        .output()
        .expect("cargo new");
    assert!(
        created.status.success(),
        "cargo new failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let cargo_toml = proj.join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&cargo_toml).expect("read scratch Cargo.toml");
    manifest.push_str("\nserialport = \"*\"\n");
    std::fs::write(&cargo_toml, manifest).expect("write scratch Cargo.toml");
    std::fs::copy(format!("{REPO}/deny.toml"), proj.join("deny.toml")).expect("copy deny.toml");

    // …but only if this runner can resolve it. Unlike step 1 — which reads the committed
    // workspace `Cargo.lock` and needs no index — step 2 must reach crates.io for a crate
    // deliberately absent from this tree. Asking first is what keeps a network outage a
    // skip instead of a false accusation that §13 is broken.
    if let Some(why) = resolution_failure(&cargo_toml) {
        return skip(
            &format!(
                "`serialport` cannot be resolved on this runner, so cargo-deny would fail \
                 before ever consulting the ban list:\n{}",
                why.trim_end()
            ),
            "give this runner crates.io access (unset CARGO_NET_OFFLINE / warm CARGO_HOME), \
             or run the dedicated license-gate CI job",
        );
    }

    let banned = Command::new("cargo")
        .args(["deny", "--manifest-path"])
        .arg(proj.join("Cargo.toml"))
        .args(["check", "bans"])
        .output()
        .expect("run cargo deny on the banned crate");

    // **Assert on the diagnostic, not on the exit code** (review 32 TESTR-2). This
    // gate exists so §13's permissive-only policy is "proven, not assumed" (plan §2),
    // and for a whole release it proved nothing: its only assertion was
    // `!status.success()`, which `cargo deny check bans` also satisfies for any failure
    // of the underlying `cargo metadata` — an offline or index-throttled runner, an
    // unresolvable crate, a malformed `CARGO_NET_*` value. Step 1 above resolves from
    // the committed `Cargo.lock` and survives all of those; step 2 must *fetch*
    // `serialport` and does not. So on exactly the runner shape CI uses (a warm
    // `Swatinem/rust-cache` `~/.cargo` with no network) the gate went green having
    // never consulted the ban list — the finder proved it by deleting the ban entry
    // and watching the test stay green.
    //
    // cargo-deny exits **2** for a real ban rejection and **1** for a metadata failure,
    // and a rejection names both `libudev-sys` and `serialport` in two `error[banned]`
    // diagnostics — so requiring the word the tool only writes when the ban list fired
    // is what makes the verdict mean something.
    //
    // What follows is that requirement split three ways rather than two, because
    // "cargo-deny did not say `error[banned]`" is *two* different verdicts and the fix
    // for TESTR-2 first shipped conflating them the other way round: a runner that
    // cannot fetch `serialport` was failing the gate. The resolution probe above already
    // separated them; the arms below re-check on cargo-deny's own exit code so a
    // discrepancy between the probe and the tool is still reported rather than guessed.
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&banned.stderr),
        String::from_utf8_lossy(&banned.stdout)
    );

    // Outcome 1 — the ban list fired. This is the only shape that passes.
    if text.contains("error[banned]") && text.contains("serialport") {
        return;
    }

    // Outcome 2 — the ban list was consulted and let `serialport` through. cargo-deny
    // exits 0 for a clean check, so this is unambiguous: the entry was deleted, renamed,
    // or moved under a section that no longer applies.
    assert!(
        !banned.status.success(),
        "the ban list did NOT reject `serialport` — the §13 gate is a no-op:\n{text}"
    );

    // Outcome 3, residual — the graph resolved for the probe a moment ago but cargo-deny
    // still could not build one (a torn registry cache, an index that went away between
    // the two commands, a `CARGO_NET_*` value it reads and the probe does not). Exit 1 is
    // cargo-deny's *metadata* failure; a rejection is 2 and a clean run is 0, so neither
    // of the two outcomes above can hide in here.
    let metadata_failure = ["`cargo metadata` exited with an error", "failed to fetch"]
        .iter()
        .any(|m| text.contains(m));
    if banned.status.code() == Some(1) && metadata_failure {
        return skip(
            &format!(
                "cargo-deny could not build a dependency graph for the scratch crate, so \
                 it never reached the ban list:\n{}",
                text.trim_end()
            ),
            "give this runner crates.io access, or run the dedicated license-gate CI job",
        );
    }

    panic!(
        "cargo-deny failed without ever saying `error[banned] serialport`, so it failed \
         for a reason that is neither the ban list nor a resolution failure this gate \
         knows how to skip on (exit {:?}; a metadata failure exits 1, a ban rejection \
         exits 2) — this gate proved nothing about §13:\n{text}",
        banned.status.code()
    );
}
