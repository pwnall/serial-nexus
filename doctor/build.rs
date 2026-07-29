//! Stamp the build's git identity into the binary, so a report says which tree
//! produced it (§15.17).
//!
//! **Why this exists.** The doctor's whole purpose for P6–P12 is that two runs on
//! two kernels get diffed field by field (§13, `expectations/linux.jq`). When the
//! 2026-07-27 Linux 6.18 report arrived, establishing *which build* had produced
//! it took forensics on a section title — its P4 block was the pre-`RES-2`
//! wording, which is the only reason anyone noticed the run predated HEAD. A
//! report that cannot say what produced it makes every cross-kernel claim rest on
//! that kind of accident.
//!
//! **Dependency-free on purpose.** `serial-nexus-doctor`'s dependency list is part of the
//! licensing gate (`cargo deny check licenses bans sources`), so this uses `std`
//! and the `git` binary and nothing else — no `vergen`, no `git2`. Every failure
//! path degrades to `unknown` rather than failing the build: the crate must build
//! from a source tarball, inside a container with no git, and from a `cargo
//! package` staging directory where `.git` does not exist at all.

use std::process::Command;

fn main() {
    let commit = git_identity().unwrap_or_else(|| "unknown".to_owned());
    // One line only: a value carrying a newline would inject a second cargo
    // directive. Only a hand-set `SNX_BUILD_COMMIT` can contain one, but a
    // provenance stamp is the wrong place to trust its input.
    let commit = commit.lines().next().unwrap_or("unknown");
    println!("cargo:rustc-env=SNX_BUILD_COMMIT={commit}");

    // **Re-run on every build, deliberately.** Emitting a path that cannot exist
    // is the documented idiom for it, and the alternatives were both measured to
    // be wrong in the same direction — a *confidently wrong* stamp, which is
    // worse than no stamp and is the failure this file exists to remove.
    //
    // Watching only the git refs (`HEAD`, the resolved ref, `packed-refs`) breaks
    // the `-dirty` flag: emitting any `rerun-if-changed` replaces cargo's default
    // "any file in the package", so editing a source file did not re-run this
    // script and a modified tree kept reporting its pristine sha — exactly the
    // "two builds called by the same commit" case the dirty flag is for. Worse,
    // when git fails at build time (a freshly-copied checkout owned by another
    // uid: `detected dubious ownership`) no ref path is emitted at all, so the
    // stamp froze at `unknown` *permanently* — surviving the operator fixing the
    // ownership, recoverable only by `cargo clean` or `touch .git/HEAD`, and
    // silent because the jq gate accepts `unknown` by design.
    //
    // The cost is three `git` execs per build of this crate and no recompilation
    // (cargo compares the emitted directives; an unchanged stamp relinks nothing).
    println!("cargo:rerun-if-changed=.snx-build-identity-always-rerun");
    println!("cargo:rerun-if-env-changed=SNX_BUILD_COMMIT");
}

/// `<short sha>` or `<short sha>-dirty`, or `None` when git cannot answer.
///
/// The dirty flag is not decoration: the two report-text defects fixed on
/// 2026-07-28 were found by running a *modified* tree beside a pristine one, and
/// a stamp that called both builds by the same commit would have been the same
/// misdirection this file exists to remove.
fn git_identity() -> Option<String> {
    if let Ok(explicit) = std::env::var("SNX_BUILD_COMMIT")
        && !explicit.trim().is_empty()
    {
        return Some(explicit.trim().to_owned());
    }
    // **The repository must actually track this crate.** git's ancestor walk will
    // happily answer from an *unrelated* enclosing repository — `cargo vendor`
    // into a consumer's tree, or a `$CARGO_HOME` inside a git-managed `$HOME` —
    // and stamping that repository's HEAD as serial-nexus-doctor's provenance is the
    // confidently-wrong answer this file exists to prevent. Say `unknown` instead.
    //
    // Probe `Cargo.toml`, never `build.rs`: the manifest is tracked for as long as
    // the crate exists, whereas a file that is merely *present* may be untracked —
    // this check first reported `unknown` on the very commit that introduced
    // `build.rs`, because the script was asking git about a file git had not been
    // told about yet.
    run(&["ls-files", "--error-unmatch", "Cargo.toml"])?;

    let sha = run(&["rev-parse", "--short=12", "HEAD"])?;
    // `--porcelain` is empty exactly when the tree matches HEAD. `status` can fail
    // on a repository git refuses to read (ownership checks), in which case say
    // nothing rather than guess clean. `--no-optional-locks` so a build never
    // takes `.git/index.lock` out from under a concurrent `git commit`, and so a
    // read-only checkout reports clean rather than degrading the stamp.
    let dirty = match run(&[
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=no",
    ]) {
        Some(out) if !out.is_empty() => "-dirty",
        Some(_) => "",
        None => "-unknown-worktree",
    };
    Some(format!("{sha}{dirty}"))
}

/// Run `git` in the crate's own directory and return trimmed stdout, or `None` on
/// any failure — a missing binary, a non-zero exit, or non-UTF-8 output.
fn run(args: &[&str]) -> Option<String> {
    let cwd = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    Some(s.trim().to_owned())
}
