//! Meta-gates ported from phase 0: the checks that guard the codebase itself rather
//! than the daemon. From `scripts/validate/phase0/unsafe-gate.sh` (design §16.3 —
//! `unsafe` confined to `nexus-sys`) and `phase0/doctor.sh` (§15.17 — no probe reports
//! `unsupported`). Portable Rust: no `jq`, no shell `grep`.
//!
//! (The license gate is now `tests/p0_license_gate.rs`, which self-skips without
//! cargo-deny; §16.11 folded the last three shell scripts into Rust, so no bash remains.)

use std::path::{Path, PathBuf};
use std::process::Command;

use nexus_itest::bin;
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <root>/nexus-itest.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Does `src` contain a real `unsafe` *usage* — the keyword as a whole word followed
/// by a block/fn/impl/trait/extern? Mirrors the bash gate's `\bunsafe\b\s*(\{|fn|impl|
/// trait|extern)`, so `#![forbid(unsafe_code)]` (word `unsafe_code`) never trips it.
fn has_unsafe_usage(src: &str) -> bool {
    let b = src.as_bytes();
    let is_word = |c: u8| c == b'_' || c.is_ascii_alphanumeric();
    let mut i = 0;
    while let Some(pos) = src[i..].find("unsafe") {
        let start = i + pos;
        let end = start + "unsafe".len();
        let before_ok = start == 0 || !is_word(b[start - 1]);
        let after_ok = b.get(end).map(|&c| !is_word(c)).unwrap_or(true);
        if before_ok && after_ok {
            let rest = src[end..].trim_start();
            if rest.starts_with('{')
                || rest.starts_with("fn")
                || rest.starts_with("impl")
                || rest.starts_with("trait")
                || rest.starts_with("extern")
            {
                return true;
            }
        }
        i = end;
    }
    false
}

fn walk_rs(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // Skip build output, VCS, the excluded fuzz crate, and vendored trees.
            if matches!(name.as_ref(), "target" | ".git" | "fuzz" | "node_modules") {
                continue;
            }
            walk_rs(&path, visit);
        } else if name.ends_with(".rs")
            && let Ok(src) = std::fs::read_to_string(&path)
        {
            visit(&path, &src);
        }
    }
}

#[test]
fn unsafe_is_confined_to_nexus_sys() {
    // 1. Prove the detector actually catches an `unsafe` usage. The sample is built by
    //    concatenation so this source file itself carries no literal match.
    let planted = format!("fn f() {{ {} {{ let _ = 1; }} }}", "unsafe");
    assert!(
        has_unsafe_usage(&planted),
        "the detector does not catch a planted unsafe usage"
    );

    // 2. No `.rs` outside `nexus-sys/` may contain an `unsafe` usage.
    let root = repo_root();
    let detector = Path::new(file!()).file_name().map(|n| n.to_os_string());
    let mut offenders = Vec::new();
    walk_rs(&root, &mut |path, src| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        if rel.starts_with("nexus-sys/") {
            return;
        }
        // Self-exclude this detector file (it names the keywords it scans for).
        if path.file_name().map(|n| n.to_os_string()) == detector {
            return;
        }
        if has_unsafe_usage(src) {
            offenders.push(rel);
        }
    });
    assert!(
        offenders.is_empty(),
        "`unsafe` found outside nexus-sys/: {offenders:?}"
    );

    // 3. Sanity: nexus-sys genuinely carries the unsafe (else the split is a lie).
    let sys = std::fs::read_to_string(root.join("nexus-sys/src/lib.rs")).expect("read nexus-sys");
    assert!(
        has_unsafe_usage(&sys),
        "nexus-sys carries no unsafe — the extraction target is wrong"
    );
}

#[test]
fn doctor_reports_no_unsupported_capability() {
    let out = Command::new(bin("nexus-doctor"))
        .arg("--json")
        .output()
        .expect("run nexus-doctor");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse doctor json: {e}; stdout={:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });

    // No probe may contradict the design (§15.17). `skipped`/`degraded` are fine.
    assert_eq!(
        v["summary"]["unsupported"],
        json!(0),
        "a capability is unsupported: {}",
        v["summary"]
    );

    let status = |id: &str| -> String {
        v["probes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == json!(id))
            .unwrap_or_else(|| panic!("probe {id} missing"))["status"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // P2 (POLLHUP presence): `supported` on Linux; `supported` or `degraded` elsewhere
    // (the §7.2 platform arm on BSD/macOS). P1 (EXTPROC) may always degrade to poll-only.
    let p2 = status("P2");
    #[cfg(target_os = "linux")]
    assert_eq!(p2, "supported", "P2 must be supported on Linux, was {p2}");
    #[cfg(not(target_os = "linux"))]
    assert!(p2 == "supported" || p2 == "degraded", "P2 was {p2}");

    let p1 = status("P1");
    assert!(p1 == "supported" || p1 == "degraded", "P1 was {p1}");
}
