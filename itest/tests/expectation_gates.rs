#![forbid(unsafe_code)]

//! The expectation files are gates, and a gate proves its **matcher** (AGENTS §3).
//!
//! `expectations/linux.jq` and `expectations/macos.jq` are the per-platform gates CI
//! runs against live `serial-nexus-doctor --json` output. Every clause in them was
//! previously asserted only by CI passing on a healthy box — which is precisely the
//! shape that let the P4 defect through: the probe reported `supported` with a
//! population of zero, and both files admitted it, on Darwin for three consecutive
//! captures (`docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`, notes
//! §3.45 (ii)). A gate nobody has ever seen fail is not known to be a gate.
//!
//! So this file plants the violation, in every spelling the clause claims to cover —
//! a `supported` P4 whose `canonical` count is zero, and one whose `canonical` key is
//! missing entirely — and requires the platform's own expectation file to **reject**
//! both, having first accepted the unmutated report from the same binary.
//!
//! It self-skips without `jq`, the same way the Playwright gate self-skips without
//! node (AGENTS §3); CI has jq, because CI runs these files.
//!
//! The fixture is Darwin's shape reproduced on Linux through the `--dev-root` seam
//! (plan §3) — four `cu.*` callout nodes, no by-id tree, no by-path tree, no sysfs —
//! so the report under test is the one the defect was found in rather than a
//! stand-in for it (§9, no proxy in space).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("doctor/ has a parent")
        .to_path_buf()
}

/// The expectation file this box's CI lane actually gates on. Running the *other*
/// platform's file here would be a proxy in space (§9).
fn platform_expectation() -> PathBuf {
    let name = if cfg!(target_os = "macos") {
        "expectations/macos.jq"
    } else {
        "expectations/linux.jq"
    };
    repo_root().join(name)
}

fn have_jq() -> bool {
    Command::new("jq")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// `jq -e -f <file>` over `json`, returning whether the gate accepted it.
fn gate_accepts(expectation: &Path, json: &str) -> bool {
    let out = Command::new("jq")
        .arg("-e")
        .arg("-f")
        .arg(expectation)
        .arg("/dev/stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .as_mut()
                .expect("stdin piped")
                .write_all(json.as_bytes())?;
            c.wait_with_output()
        })
        .expect("jq runs");
    out.status.success()
}

/// One `jq` filter over `json`, as a string.
fn jq_filter(filter: &str, json: &str) -> String {
    let out = Command::new("jq")
        .arg(filter)
        .arg("/dev/stdin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .as_mut()
                .expect("stdin piped")
                .write_all(json.as_bytes())?;
            c.wait_with_output()
        })
        .expect("jq runs");
    assert!(out.status.success(), "jq filter failed: {filter}");
    String::from_utf8(out.stdout).expect("jq emits utf-8")
}

struct TmpTree(PathBuf);

impl TmpTree {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("snx-expect-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dev")).unwrap();
        TmpTree(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_p4_clause_rejects_a_certified_resolver_that_resolved_nothing() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let t = TmpTree::new("p4");
    for n in [
        "cu.usbserial-BH00L4KU",
        "cu.usbserial-BH00LL8O",
        "cu.Bluetooth-Incoming-Port",
        "cu.BLTH",
    ] {
        std::fs::write(t.path().join("dev").join(n), b"").unwrap();
    }
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--dev-root")
        .arg(t.path())
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // The honest report from this fixture must PASS. A gate that rejects the truth is
    // not a stricter gate, it is a broken one.
    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest report: {}",
        expectation.display(),
        jq_filter(r#".probes[] | select(.id=="P4")"#, &report)
    );

    // Spelling 1 — the defect exactly as captured on Darwin: `supported` over a
    // population of zero.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P4") | .status) = "supported""#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P4 whose canonical count is 0 — the vacuous pass \
         it exists to refuse (§9, notes §3.45 (ii))",
        expectation.display()
    );

    // Spelling 2 — the same claim from a report that does not carry the population at
    // all, which is what every pre-2026-08-05 binary emitted. Also rejected, and that is
    // the deliberate half: this gate is only ever run against a LIVE report (CI pipes
    // `--json` straight into it; nothing runs it over `docs/doctor/*.json`), so a report
    // that cannot state its population is one no operator will meet — a capture just
    // taken comes from the current binary and carries the key. Requiring it therefore
    // costs nothing real and keeps the clause saying what it means.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P4") | .observations) |= map(select(.key != "canonical"))
           | (.probes[] | select(.id=="P4") | .status) = "supported""#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P4 with no `canonical` observation at all",
        expectation.display()
    );
}

/// The provenance half of the same discipline: a report that cannot say **which
/// cells it carries** cannot be diffed cell by cell against one that can.
///
/// Note the disposition on absence is the **opposite** of the P4 clause's above, and
/// deliberately so. `canonical` is a measurement, and rejecting a report that lacks it
/// would make the gate an instrument-version detector. `field_set` is provenance — it
/// sits with `commit` and `probe_set`, which this file has always required to be
/// *present* — and its absence is exactly the state the field exists to name: unknown,
/// which must never read as "equal". Nothing is lost, because these files gate live
/// `--json` output and never `docs/doctor/*` (which every clause here would fail for
/// the same reason: those artifacts predate the field).
#[test]
fn the_field_set_clause_rejects_a_report_that_cannot_say_which_cells_it_carries() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let t = TmpTree::new("fieldset");
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--dev-root")
        .arg(t.path())
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest report carrying its own field set: {}",
        expectation.display(),
        jq_filter(".build", &report)
    );

    // Spelling 1 — the field gone, which is every report captured before 2026-08-05
    // and any future binary that drops it.
    let stripped = jq_filter("del(.build.field_set)", &report);
    assert!(
        !gate_accepts(&expectation, &stripped),
        "{} admitted a report with no field set — a reader has no machine-checkable \
         way to know the two reports carry the same cells, which is the gap \
         `probe_set` equality was being read as closing",
        expectation.display()
    );

    // Spelling 2 — present but not a digest. The clause pins the *shape* (16 hex
    // chars' worth of length) and never the value, because the value is a property of
    // the run: naming ports, a different kernel, or a histogram key this box did not
    // observe all move it on a perfectly healthy machine.
    let truncated = jq_filter(r#".build.field_set = "abc""#, &report);
    assert!(
        !gate_accepts(&expectation, &truncated),
        "{} admitted a field set that is not a 16-character digest",
        expectation.display()
    );
}

/// The proof above runs against one platform's file per box. This is what carries it
/// to the other: the two gates must hold the *same* clause, byte for byte, so an edit
/// to one that forgets the other fails here instead of silently reopening the hole on
/// the lane nobody is standing on.
#[test]
fn both_expectation_files_carry_the_same_field_set_clause() {
    let clause = r#"and (.build.field_set | type == "string" and length == 16)"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        assert!(
            text.contains(clause),
            "{name} does not carry the field-set clause verbatim — the behavioural \
             proof in this file runs on one platform's gate and reaches the other only \
             through this identity"
        );
    }
}

/// The proof above runs against one platform's file per box. This is what carries it
/// to the other: the two gates must hold the *same* clause, byte for byte, so an edit
/// to one that forgets the other fails here instead of silently reopening the hole on
/// the lane nobody is standing on.
#[test]
fn both_expectation_files_carry_the_same_p4_population_clause() {
    let clause = r#"and (all(.probes[]; . as $p
      | ($p.id != "P4")
      or ($p.status != "supported")
      or (([$p.observations[] | select(.key == "canonical") | .value] | last) as $c
          | (($c | type) == "number") and ($c > 0))))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        assert!(
            text.contains(clause),
            "{name} does not carry the P4 population clause verbatim — the behavioural \
             proof in this file runs on one platform's gate and reaches the other only \
             through this identity"
        );
    }
}

/// The clause-identity guard for P14 (§15.51), for the reason the two above give.
#[test]
fn both_expectation_files_carry_the_same_p14_clauses() {
    let status = r#"and (any(.probes[]; .id == "P14" and (.status == "supported" or .status == "skipped" or .status == "degraded")))"#;
    let presence = r#"and (all(.probes[]; . as $p | ($p.id != "P14") or ($p.status == "skipped")
      or (($p.observations | any(.key == "max_reliable_baud"))
          and ($p.observations | any(.key == "ceiling_kind"))
          and ($p.observations | any(.key == "structural_max_baud" and (.value | type == "number")))
          and ($p.observations | any(.key == "ceiling_is_a_floor_over" and (.value | type == "string"))))))"#;
    let measured = r#"and (all(.probes[]; . as $p | ($p.id != "P14") or ($p.status != "supported")
      or ((([$p.observations[] | select(.key == "max_reliable_baud") | .value] | last) != null)
          and (([$p.observations[] | select(.key == "ceiling_kind") | .value] | last) != null))))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        for (what, clause) in [
            ("status", status),
            ("presence", presence),
            ("measured", measured),
        ] {
            assert!(
                text.contains(clause),
                "{name} does not carry the P14 {what} clause verbatim — the behavioural \
                 proof in this file runs on one platform's gate and reaches the other \
                 only through this identity"
            );
        }
        // Membership in the measurement list, not the list's exact tail: the list
        // grows (P16 joined it at §15.59's landing), and a needle anchored on
        // whichever id happened to be last would fail on every addition while
        // asserting nothing about P14. It is checked inside the `index(...)` clause
        // so a stray `"P14"` elsewhere in the file cannot satisfy it.
        let list = text
            .split_once("| index($p.id)) == null)")
            .and_then(|(head, _)| head.rsplit_once('[').map(|(_, l)| l.to_owned()))
            .unwrap_or_default();
        assert!(
            list.contains(r#""P14""#),
            "{name} does not list P14 among the probes that must carry measurements \
             when they run — the list reads [{list}"
        );
    }
}

/// **Both gates must refuse `unsupported`, and both must be shown able to** — the
/// unconditional clause that had no behavioural proof anywhere, and on one lane no
/// clause either.
///
/// §13 gives `unsupported` one meaning on every platform: a design premise
/// contradicted with no fallback, a stop condition asking for an amendment rather
/// than a workaround. `expectations/linux.jq` has always opened with
/// `(.summary.unsupported == 0)`. `expectations/macos.jq` opened with a bare
/// `(.summary != null)` while its own P14 paragraph said "`unsupported` stays a gate
/// failure through the summary clause" — a sentence copied from the Linux file,
/// where it was true. Measured before the repair, on this box against a
/// Darwin-shaped report: that file exited **0** with P1, P3, P4, P5 or P11 forced to
/// `unsupported`, and 0 with P15 forced to it on a run that named a port. Five of
/// those clauses are bare presence checks with no status constraint, and P5 can
/// genuinely reach `unsupported` (§15.21's data-integrity failure).
///
/// It was a hole in the *file* rather than live blindness — the doctor binary exits
/// 1 on any `unsupported`, CI redirects rather than tees, and `meta_gates.rs`
/// asserts the same count in portable Rust on both platforms. What was missing is
/// the assertion AGENTS §3 names as the macOS gate.
///
/// **Both files are exercised from whichever box runs this**, not just the local
/// one, because the defect was that the two had drifted: the report is shaped for
/// each file (the one cross-platform difference is P12, inert on Linux and
/// load-bearing on Darwin), required to be ACCEPTED first — that acceptance is what
/// makes the rejection below mean something — and then planted in two spellings.
#[test]
fn both_gates_refuse_an_unsupported_verdict_and_are_shown_able_to() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let expectation = repo_root().join(name);

        // Shape the live report for this file. P12 is the one probe whose expected
        // word differs by platform — `skipped` on Linux (the retained packet P7
        // measures carries §6 there), `supported`/`degraded` on Darwin — so a run
        // from either box needs that one cell moved to satisfy the other's file.
        // Nothing else is touched: a shaping that had to rewrite measurements would
        // mean this guard was testing a fixture rather than a report.
        let shaped = if gate_accepts(&expectation, &report) {
            report.clone()
        } else {
            let darwinish = jq_filter(
                r#"(.probes[] | select(.id=="P12")) |= (.status="degraded" | del(.reason))"#,
                &report,
            );
            let linuxish = jq_filter(
                r#"(.probes[] | select(.id=="P12")) |= (.status="skipped"
                     | .reason="serial-nexus-sys's SessionLatch is inert on this platform")"#,
                &report,
            );
            [darwinish, linuxish]
                .into_iter()
                .find(|c| gate_accepts(&expectation, c))
                .unwrap_or_else(|| {
                    panic!(
                        "{name} rejects this box's honest report even with P12 shaped either \
                         way, so the plants below would prove nothing. The two files have \
                         drifted in some clause other than P12 and this guard has to learn \
                         the new difference rather than be deleted."
                    )
                })
        };

        // Spelling 1 — the clause's literal subject. A count the report itself
        // publishes, moved without touching a probe: this is what the clause reads,
        // and a file that lost the clause cannot see it.
        let planted = jq_filter(r#".summary.unsupported = 1"#, &shaped);
        assert!(
            !gate_accepts(&expectation, &planted),
            "{name} admitted a report whose own summary counts an `unsupported` \
             probe — §13 makes that a stop condition on every platform, and this \
             file is the gate AGENTS §3 names"
        );

        // Spelling 2 — the realistic shape: a probe carrying the verdict, with the
        // summary recounted the way the doctor would. P5 because it is the one
        // probe on a rig lane that can genuinely reach `unsupported` (§15.21: the
        // rig did not deliver the bytes it was handed), and because its own clause
        // in both files is presence-only, so nothing but the summary clause can
        // refuse it.
        let planted = jq_filter(
            r#"(.probes[] | select(.id=="P5")) |= (.status="unsupported" | del(.reason))
               | .summary.unsupported = 1
               | .summary.skipped = ((.summary.skipped // 1) - 1)"#,
            &shaped,
        );
        assert!(
            !gate_accepts(&expectation, &planted),
            "{name} admitted an `unsupported` P5 — a rig that did not deliver the \
             bytes it was handed is §15.21's stop condition, not a lane to pass"
        );
    }
}

/// The clause-identity guard for the summary clause and for the P6/P7 words, for the
/// reason the three above give — and with one difference worth stating: this pair
/// had *drifted*, so the identity assert is what stops it drifting again.
#[test]
fn both_expectation_files_carry_the_same_summary_and_pty_probe_clauses() {
    let summary = r#"(.summary.unsupported == 0)"#;
    // Spelled by enumeration, never by exclusion. `.status != "unsupported"` admits
    // `skipped` — the one word every conditional clause in these files exempts,
    // including "a probe that RAN must carry measurements" — so a P6 or P7 error arm
    // wearing it would pass the whole file. That is the §13 defect notes §3.75
    // repaired for P8/P9/P10; `expectations/macos.jq` carried the by-exclusion
    // spelling for these two until 2026-08-12, latent because neither probe
    // constructs `Status::skipped`.
    let p6 = r#"and (any(.probes[]; .id == "P6" and (.status == "supported" or .status == "degraded")))"#;
    let p7 = r#"and (any(.probes[]; .id == "P7" and (.status == "supported" or .status == "degraded")))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        for (what, clause) in [("summary", summary), ("P6", p6), ("P7", p7)] {
            assert!(
                text.contains(clause),
                "{name} does not carry the {what} clause verbatim — these two files \
                 drifted here once and the drift was invisible to everything except a \
                 hand audit"
            );
        }
        assert!(
            !text.contains(r#".id == "P6" and .status != "unsupported""#)
                && !text.contains(r#".id == "P7" and .status != "unsupported""#),
            "{name} still spells P6/P7 by exclusion, which admits `skipped` and so \
             exempts the probe from every conditional clause in the file (§13)"
        );
    }
}

/// P16's status clause, spelled by enumeration for the reason the P6/P7 guard
/// above records: `.status != "unsupported"` would admit `skipped`, and `skipped`
/// is the one word every conditional clause in these files exempts — including
/// P16's own content clause and the "a probe that RAN must carry measurements"
/// clause at the bottom.
const P16_STATUS_CLAUSE: &str =
    r#"and (any(.probes[]; .id == "P16" and (.status == "supported" or .status == "degraded")))"#;

/// P16's content clause. Both arms, with the quiet arm's witnesses (§15.49) and
/// with every answer left free.
const P16_CONTENT_CLAUSE: &str = r#"and (all(.probes[]; . as $p | ($p.id != "P16") or
      (($p.observations | any(.key == "quiet_window_tight"
             and (.value.passes | type == "number") and (.value.passes > 0)
             and (.value.hangup_passes | type == "number")
             and (.value.elapsed_us | type == "number")))
       and ($p.observations | any(.key == "quiet_window_paced"
             and (.value.passes | type == "number") and (.value.passes > 0)
             and (.value.hangup_passes | type == "number")
             and (.value.elapsed_us | type == "number")))
       and ($p.observations | any(.key == "hangup_after_master_closed"
             and (.value.hangup_delivered | type == "boolean")
             and (.value.microseconds_to_hangup | type == "number")))
       and ($p.observations | any(.key == "stat_comparison_while_master_open"
             and (.value.shipped_prove_open_would_refuse | type == "boolean")))
       and ($p.observations | any(.key == "stat_comparison_after_master_closed"
             and (.value.shipped_prove_open_would_refuse | type == "boolean")))
       and ($p.observations | any(.key == "poll_can_tell_a_live_pair_from_a_dead_one"
             and (.value | type == "boolean")))
       and ($p.observations | any(.key == "stat_comparison_can_tell"
             and (.value | type == "boolean")))
       and ($p.observations | any(.key == "does_not_license" and (.value | type == "string"))))))"#;

/// The clause-identity guard for the clauses plan §18 items 14, 22 and 26 added,
/// for the reason the three above give.
#[test]
fn both_expectation_files_carry_the_same_new_flow_and_close_shape_clauses() {
    let soft = r#"and (all(.probes[]; . as $p | ($p.id != "P15") or ($p.status | startswith("skipped")) or
      (($p.observations | map(select(.value | type == "object")) | length) > 0
       and ($p.observations | map(select(.value | type == "object")) | all(
             (.value.software_flow_control | type == "object")
             and (.value.software_flow_control.asks | type == "string")
             and (.value.software_flow_control.measured | type == "boolean")
             and ((.value.software_flow_control.measured | not)
                  or ((.value.software_flow_control.tcsetattr_ok | type == "boolean")
                      and (.value.software_flow_control.honoured_on_readback | type == "boolean")
                      and (.value.software_flow_control.silently_dropped | type == "boolean")
                      and (.value.software_flow_control.serial2_readback_would_fault | type == "boolean"))))))))"#;
    let arrival = r#"and (all(.probes[]; . as $p | ($p.id != "P13") or
      ($p.observations | any(.key == "e_reader_arrives_during_close_wait"
         and (.value | type == "object")
         and (.value.bytes_recovered_by_arriving_reader | type == "number")
         and (.value.reader_arrival | type == "object")
         and (.value.reader_arrival.arrived_before_close_returned | type == "boolean")
         and (.value.reader_arrival.reading | type == "string")
         and (.value.reader_arrival.does_not_license | type == "string")))))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        for (what, clause) in [
            ("P15 software-flow-control", soft),
            ("P13 arriving-reader shape", arrival),
            ("P16 status", P16_STATUS_CLAUSE),
            ("P16 two-arm content", P16_CONTENT_CLAUSE),
        ] {
            assert!(
                text.contains(clause),
                "{name} does not carry the {what} clause verbatim — the behavioural \
                 proof in this file runs on one platform's gate and reaches the other \
                 only through this identity"
            );
        }
    }
}

/// **P16's two arms must survive the gate, and every way of losing one must not**
/// (§15.59, plan §18 item 26, plan §3 rule 10).
///
/// P16 needs no hardware, so this runs against the live report the CI lane itself
/// produces — the unmutated run is required to pass first, so each plant is known
/// to be rejected for the reason claimed.
///
/// The plants cover the four ways this probe can stop measuring while still
/// looking like a probe: gone from the roster, wearing `skipped` (the one word
/// every conditional clause in these files exempts, which is why an error path may
/// never spell it), observations emptied, and — the two that matter most — a quiet
/// window that lost its wall clock or ran zero passes. That last pair is §15.49's
/// rule in gate form: `hangup_passes: 0` over an unsized window is not a
/// measurement, and a zero-pass window is the vacuous fold this file's P4 clause
/// already exists to refuse.
///
/// The controls are the point of the probe. Both `hangup_delivered: false` and
/// `stat_comparison_can_tell: false` must be **accepted**: the first is a kernel
/// with no portable liveness signal and the second is the Darwin-shaped reading
/// this probe was built to be able to report. A clause that reddened on either
/// would be this file asserting the answer §15.59 went looking for.
#[test]
fn the_p16_clauses_reject_an_arm_that_stopped_measuring() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest passive report: {}",
        expectation.display(),
        jq_filter(r#".probes[] | select(.id=="P16")"#, &report)
    );

    for (what, filter) in [
        (
            "P16 gone from the roster",
            r#".probes |= map(select(.id!="P16"))"#,
        ),
        (
            "P16 wearing `skipped`",
            r#"(.probes[]|select(.id=="P16")) |= (.status="skipped" | .reason="compiled out")"#,
        ),
        (
            "P16 with no observations",
            r#"(.probes[]|select(.id=="P16")) |= (.observations=[])"#,
        ),
        (
            "a quiet window with no wall clock",
            r#"(.probes[]|select(.id=="P16")|.observations[]|select(.key=="quiet_window_tight")|.value) |= del(.elapsed_us)"#,
        ),
        (
            "a quiet window that ran zero passes",
            r#"(.probes[]|select(.id=="P16")|.observations[]|select(.key=="quiet_window_paced")|.value.passes) |= 0"#,
        ),
        (
            "the paced window deleted, leaving only the tight one",
            r#"(.probes[]|select(.id=="P16")|.observations) |= map(select(.key!="quiet_window_paced"))"#,
        ),
        (
            "the post-close stat reading deleted",
            r#"(.probes[]|select(.id=="P16")|.observations) |= map(select(.key!="stat_comparison_after_master_closed"))"#,
        ),
        (
            "the bound deleted",
            r#"(.probes[]|select(.id=="P16")|.observations) |= map(select(.key!="does_not_license"))"#,
        ),
    ] {
        let planted = jq_filter(filter, &report);
        assert!(
            !gate_accepts(&expectation, &planted),
            "{} admitted {what}",
            expectation.display()
        );
    }

    // Control 1 — a kernel with no portable liveness signal. `degraded` with the
    // observation named is §7's rule, not a lane to fail.
    let no_hangup = jq_filter(
        r#"(.probes[]|select(.id=="P16")) |= (.status="degraded"
             | (.observations[]|select(.key=="hangup_after_master_closed")|.value.hangup_delivered)=false
             | (.observations[]|select(.key=="poll_can_tell_a_live_pair_from_a_dead_one")|.value)=false)"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &no_hangup),
        "{} reddened on a kernel that does not deliver POLLHUP to a held slave — \
         that is the observation §15.59 asks for, and pinning it would make this \
         file assert the thing the probe measures",
        expectation.display()
    );

    // Control 2 — the Darwin-shaped reading: a devfs node that outlives its pair,
    // so the shipped `stat` comparison cannot tell. This is the row the probe was
    // built to be able to report, and the gate must never punish it.
    let stat_blind = jq_filter(
        r#"(.probes[]|select(.id=="P16")|.observations[]|select(.key=="stat_comparison_can_tell")|.value)=false
           | (.probes[]|select(.id=="P16")|.observations[]|select(.key=="stat_comparison_after_master_closed")|.value.shipped_prove_open_would_refuse)=false"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &stat_blind),
        "{} reddened on a kernel where `SlaveWitness::prove_open` cannot tell a live \
         pair from a dead one — that is the finding, not a failure",
        expectation.display()
    );
}

/// **P13's arriving-reader shape must survive the gate, and its absence must not**
/// (plan §18 item 22, plan §3 rule 10).
///
/// P13 needs no hardware, so unlike the P15 guard below this one runs against the
/// live report the CI lane itself produces — the strongest form available: the
/// unmutated run is required to pass first, so the plants below are known to be
/// rejected *for the reason claimed* rather than by some unrelated clause.
///
/// Three plants, because the three ways this row can go missing are three different
/// edits: the shape deleted from the list, the shape *erroring* (its value becomes
/// a bare string, which is what `p13_shape`'s `Err` arm emits and which nothing else
/// in the gate set notices — P13 only degrades when shape `a` is the one that
/// failed), and the row surviving with the sentence that bounds it removed. The
/// fourth case is the control: flipping the answer to `false` must still be
/// accepted, because winning or losing that race is the kernel's business and a
/// clause that pinned it would redden an honest box (§13, plan §3 rule 14).
#[test]
fn the_p13_arrival_clause_rejects_a_shape_that_stopped_running() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest passive report: {}",
        expectation.display(),
        jq_filter(r#".probes[] | select(.id=="P13")"#, &report)
    );

    // Plant 1 — the shape deleted. The verdict word and every other shape survive.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P13") | .observations) |=
             map(select(.key != "e_reader_arrives_during_close_wait"))"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a P13 that had quietly stopped running its arriving-reader shape",
        expectation.display()
    );

    // Plant 2 — the shape errored. `p13_shape`'s Err arm observes a bare string
    // under the same key, and P13's verdict does not degrade for it: only shape `a`
    // failing does that. So this is the spelling with no other instrument behind it.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P13") | .observations[]
             | select(.key=="e_reader_arrives_during_close_wait") | .value) |= "probe error: boom""#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a P13 whose arriving-reader shape reported an error instead of a \
         measurement",
        expectation.display()
    );

    // Plant 3 — the row present, the sentence bounding it deleted. A race result
    // with no statement of what it licenses is exactly the reading §13 refuses:
    // `lost-the-race` and `arrived-inside-the-close-window` are not interchangeable.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P13") | .observations[]
             | select(.key=="e_reader_arrives_during_close_wait") | .value.reader_arrival)
           |= del(.does_not_license)"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted an arrival row with no statement of what it does not license",
        expectation.display()
    );

    // The control: the ANSWER is free. A kernel whose close returns before the
    // reader gets there is a legitimate reading — it is the expected one wherever
    // the close does not wait — and this clause must never punish it.
    let honest = jq_filter(
        r#"(.probes[] | select(.id=="P13") | .observations[]
             | select(.key=="e_reader_arrives_during_close_wait") | .value.reader_arrival)
           |= (.arrived_before_close_returned = false | .reading = "lost-the-race")"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &honest),
        "{} rejected an honest `lost-the-race` reading — the plants above then prove \
         nothing, and every kernel that does not wait would fail this lane",
        expectation.display()
    );
}

/// **P15's software-flow-control cells must be present per port, and the answer must
/// stay free** (plan §18 item 14, plan §3 rule 10).
///
/// P15 is opt-in behind `--port` and CI has no adapter, so unlike the P13 guard the
/// subject here is *synthesised* onto the live report: the clause is conditioned on
/// P15 not skipping, and a passive run skips, which would make every plant below
/// vacuously green. The honest block is injected first and required to PASS — that
/// is the control that makes the plants mean something — and the probe half of the
/// two-guard pattern (notes §3.64) is carried by
/// `probes::tests::p15_reports_the_software_reading_without_letting_it_move_the_verdict`
/// and by the committed rig capture, not by this test.
#[test]
fn the_p15_software_clause_rejects_a_reading_that_was_never_taken() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // A rig-shaped P15 block: one port, honoured both ways, restored.
    const HONEST: &str = r#"(.probes[] | select(.id=="P15")) |= (.status = "supported"
       | .observations = [{"key":"/dev/ttyUSB0","value":{
           "requested":"rts-cts (CRTSCTS)","tcsetattr_ok":true,"tcsetattr_error":null,
           "cflag_before_hex":"0x10021cb2","cflag_after_hex":"0x90021cb2",
           "honoured_on_readback":true,"shipped_predicate_agrees":true,
           "silently_dropped":false,"baseline_restored":true,
           "software_flow_control":{"asks":"…","measured":true,
             "requested":"xon-xoff (IXON|IXOFF)","tcsetattr_ok":true,"tcsetattr_error":null,
             "iflag_before_hex":"0x5","iflag_after_hex":"0x1405",
             "ixon_on_readback":true,"ixoff_on_readback":true,
             "honoured_on_readback":true,"silently_dropped":false,
             "serial2_readback_would_fault":false,"does_not_license":"…"}}}])"#;
    let honest = jq_filter(HONEST, &report);
    assert!(
        gate_accepts(&expectation, &honest),
        "{} rejected a complete P15 block — every plant below would then prove nothing",
        expectation.display()
    );

    // Plant 1 — the software block gone entirely. This is the shape a revert takes.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P15") | .observations[].value) |= del(.software_flow_control)"#,
        &honest,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a P15 carrying no software-flow-control reading at all",
        expectation.display()
    );

    // Plant 2 — the block present, `measured` gone. A reading with no statement of
    // whether it was taken is the "unknown wearing an answer's clothes" shape
    // §15.47 names: unmeasurable is data, and its absence is not.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P15") | .observations[].value.software_flow_control)
           |= del(.measured)"#,
        &honest,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a software reading that does not say whether it was taken",
        expectation.display()
    );

    // Plant 3 — `measured: true` with the cell that decides the node's fate gone.
    // `serial2_readback_would_fault` is the whole-`c_iflag` comparison the product
    // actually performs; the two flag booleans are the human-readable half.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P15") | .observations[].value.software_flow_control)
           |= del(.serial2_readback_would_fault)"#,
        &honest,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `measured` software reading with no read-back verdict",
        expectation.display()
    );

    // Plant 4 — the port population emptied while the verdict stays `supported`.
    // An `all` over zero ports passes forever; this is the same vacuity the P4
    // population clause exists to refuse, in P15's clothes.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P15")) |= (.status = "supported" | .observations = [])"#,
        &honest,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P15 whose port population is empty",
        expectation.display()
    );

    // Control A — the ANSWER is free. A driver that drops the request is precisely
    // what plan §18 item 14 went looking for, and the item declines turning that
    // finding into policy; a clause that reddened on it would encode the policy the
    // item refused to write.
    let dropping = jq_filter(
        r#"(.probes[] | select(.id=="P15") | .observations[].value.software_flow_control)
           |= (.honoured_on_readback = false | .ixon_on_readback = false
               | .ixoff_on_readback = false | .silently_dropped = true
               | .serial2_readback_would_fault = true)"#,
        &honest,
    );
    assert!(
        gate_accepts(&expectation, &dropping),
        "{} reddened on a driver that silently drops IXON/IXOFF — that is a finding to \
         report, not a lane to fail (plan §18 item 14 declines the refusal)",
        expectation.display()
    );

    // Control B — an honest `measured: false`. A port whose termios could not be
    // read back says so and carries no reading cells, and that must pass: refusing
    // it would push the probe toward inventing an answer.
    let unmeasured = jq_filter(
        r#"(.probes[] | select(.id=="P15") | .observations[].value.software_flow_control)
           |= {asks:"…", measured:false, unmeasurable_here:"tcgetattr after xon-xoff: ENOTTY",
               does_not_license:"…"}"#,
        &honest,
    );
    assert!(
        gate_accepts(&expectation, &unmeasured),
        "{} rejected an honest unmeasurable-here reading (§15.47)",
        expectation.display()
    );
}

/// **The P14 presence clause, proven against a planted violation in every spelling
/// it claims to cover** (plan §3 rule 10).
///
/// The hazard this refuses is specific: P14's answer is a number, and a gate that
/// pinned it would assert the very thing the probe measures. So the clause gates
/// *presence* — and a presence clause is exactly the kind that passes vacuously if
/// its matcher is wrong, because on the box that wrote it every key happens to be
/// there. Two spellings, because the two ways a report can lose a cell are not the
/// same edit: a probe that reports `supported` having dropped the number, and one
/// that keeps the number but drops the sentence bounding what it licenses.
///
/// It runs passively — no `--port`, so P14 itself reports `skipped`. That is
/// deliberate and is the whole reason the planted reports force `.status`: the
/// clause exempts `skipped`, so a plant that left it alone would prove nothing, and
/// the exemption is what a rig-less CI lane relies on.
#[test]
fn the_p14_presence_clause_rejects_a_ceiling_reported_without_its_number_or_its_bound() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // The honest passive report must PASS: P14 skips without a named port, and a
    // gate that rejected that would fail every CI run.
    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest passive report: {}",
        expectation.display(),
        jq_filter(r#".probes[] | select(.id=="P14")"#, &report)
    );
    assert_eq!(
        jq_filter(r#".probes[] | select(.id=="P14") | .status"#, &report).trim(),
        "\"skipped\"",
        "a passive run must skip P14 — it transmits, so it is opt-in (§13)"
    );

    // Spelling 1 — `supported` with no ceiling reported. This is the shape a probe
    // takes when its search is rewritten and the observation is forgotten: the
    // verdict word survives, the measurement does not, and a verdict word cannot be
    // diffed across kernels.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P14")) |= (.status = "supported" | .observations = [])"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a `supported` P14 carrying no observations at all",
        expectation.display()
    );

    // Spelling 2 — the number present, the sentence that bounds it deleted. A
    // ceiling with no statement of what it is a floor over is the reading §15.51
    // spends a paragraph refusing: it invites "3 Mbaud works" when what was measured
    // is "three constant-airtime round-trips per direction passed at 3 Mbaud".
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P14")) |= (.status = "supported"
           | .observations = [{"key":"max_reliable_baud","value":3000000},
                              {"key":"ceiling_kind","value":"adapter-refused"},
                              {"key":"structural_max_baud","value":4294967295}])"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a P14 ceiling with no statement of what the number is a floor over",
        expectation.display()
    );

    // And the control that keeps the two plants honest: the same forced `supported`
    // with the full cell set present is ACCEPTED. Without this, a clause that
    // rejected every forced-`supported` report — for some reason having nothing to
    // do with P14 — would pass both plants above while testing nothing.
    let honest = jq_filter(
        r#"(.probes[] | select(.id=="P14")) |= (.status = "supported"
           | .observations = [{"key":"max_reliable_baud","value":3000000},
                              {"key":"ceiling_kind","value":"adapter-refused"},
                              {"key":"structural_max_baud","value":4294967295},
                              {"key":"ceiling_is_a_floor_over","value":"the rates this ladder probed"}])"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &honest),
        "{} rejected a complete P14 block — the two plants above then prove nothing",
        expectation.display()
    );
}

// ---------------------------------------------------------------------------
// P4's no-udev arm, driven through the shipped binary (plan §18 item 9)
// ---------------------------------------------------------------------------

/// The sysfs half of a USB tty device — the USB device dir, its interface dir, and
/// the `class/tty/<dev>/device` link the resolver's ancestor walk starts from.
/// Deliberately separate from the `/dev` node and from the udev symlinks, because
/// what these fixtures vary is exactly *which* of the three exist.
fn sysfs_usb_tty(root: &Path, usbdir: &str, dev_name: &str, serial: &str) {
    let usbdev = root.join("sys/bus/usb/devices").join(usbdir);
    std::fs::create_dir_all(&usbdev).expect("mkdir usb device dir");
    for (f, v) in [
        ("idVendor", "0403"),
        ("idProduct", "6001"),
        ("serial", serial),
        ("manufacturer", "FTDI"),
        ("product", "FT232R USB UART"),
    ] {
        std::fs::write(usbdev.join(f), v).expect("write sysfs attribute");
    }
    let iface = usbdev.join(format!("{usbdir}:1.0"));
    std::fs::create_dir_all(&iface).expect("mkdir interface dir");
    std::fs::write(iface.join("bInterfaceNumber"), "00").expect("write bInterfaceNumber");
    let class = root.join("sys/class/tty").join(dev_name);
    std::fs::create_dir_all(&class).expect("mkdir class/tty");
    std::os::unix::fs::symlink(
        format!("../../../bus/usb/devices/{usbdir}/{usbdir}:1.0"),
        class.join("device"),
    )
    .expect("symlink class/tty device");
    // A plain file, not a FIFO: `is_dev_node` accepts one, and nothing in this
    // file may block on a read. (`meta_gates.rs` carries the structural guard
    // that resolution stays passive; this is only the fixture.)
    let dev = root.join("dev").join(dev_name);
    std::fs::create_dir_all(dev.parent().expect("dev has a parent")).expect("mkdir dev");
    std::fs::write(&dev, b"").expect("write dev node");
}

fn by_id_link(root: &Path, name: &str, dev_name: &str) {
    let by_id = root.join("dev/serial/by-id");
    std::fs::create_dir_all(&by_id).expect("mkdir by-id");
    std::os::unix::fs::symlink(format!("../../{dev_name}"), by_id.join(name))
        .expect("symlink by-id");
}

/// Run the shipped doctor over a fixture tree and return its P4 probe object.
fn p4_over(root: &Path) -> serde_json::Value {
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--dev-root")
        .arg(root)
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("the doctor emits JSON");
    report["probes"]
        .as_array()
        .expect("the report has a probes array")
        .iter()
        .find(|p| p["id"] == serde_json::json!("P4"))
        .cloned()
        .expect("the report contains P4")
}

fn p4_obs(p4: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
    p4["observations"]
        .as_array()
        .expect("P4 has an observations array")
        .iter()
        .find(|o| o["key"].as_str() == Some(key))
        .map(|o| o["value"].clone())
}

/// **P4's no-udev arm, executed through the shipped binary for the first time**
/// (plan §18 item 9).
///
/// §12 grew a `<sys>/class/tty` fallback precisely so a box with no udev by-id
/// tree still resolves canonical identities — a container's bare `--device=`, a
/// busybox-mdev image. Until now that arm was exercised only by the doctor's own
/// in-crate unit tests and by a daemon-level resolver test; **no committed
/// `docs/doctor/` artifact has ever printed its consequence sentence**, because
/// every capture came from a box where udev names everything. `--dev-root` reroots
/// `/dev` and `/sys` together, so the whole arm fires with no hardware at all.
#[test]
fn p4_reports_the_no_udev_arm_from_the_shipped_binary() {
    let t = TmpTree::new("p4-noudev");
    sysfs_usb_tty(t.path(), "1-1", "ttyUSB0", "UNIQ01");
    assert!(
        !t.path().join("dev/serial").exists(),
        "the fixture must have no by-id tree at all, or this is not the no-udev arm"
    );

    let p4 = p4_over(t.path());
    assert_eq!(p4["status"], serde_json::json!("supported"), "{p4}");
    assert_eq!(p4_obs(&p4, "by_id_tree"), Some("absent".into()));
    assert_eq!(p4_obs(&p4, "sysfs_tty_listing"), Some("present".into()));
    // udev named nothing, and the sysfs listing named everything.
    assert_eq!(p4_obs(&p4, "count"), Some(0.into()));
    assert_eq!(p4_obs(&p4, "sysfs_only"), Some(1.into()));
    assert_eq!(p4_obs(&p4, "canonical"), Some(1.into()));
    assert_eq!(
        p4_obs(&p4, &t.path().join("dev/ttyUSB0").display().to_string()),
        Some("usb:0403:6001:UNIQ01:00".into()),
        "the sysfs-only device must resolve to a canonical identity: {p4}"
    );
    let why = p4["consequence"].as_str().unwrap_or_default();
    assert!(
        why.contains("No /dev/serial/by-id tree here"),
        "the no-udev consequence never fired: {why}"
    );
    assert!(
        why.contains("identities came from the <sys>/class/tty listing"),
        "the provenance sentence never fired: {why}"
    );

    // And the honest report from this tree must PASS the lane's own gate — a gate
    // that rejects a supported box is not a stricter gate, it is a broken one.
    if have_jq() {
        let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
            .arg("--dev-root")
            .arg(t.path())
            .arg("--json")
            .output()
            .expect("the doctor runs");
        let report = String::from_utf8(out.stdout).expect("the report is utf-8");
        assert!(
            gate_accepts(&platform_expectation(), &report),
            "the expectation file rejected an honest no-udev tree"
        );
    }
}

/// **The `<sys>/class/tty` listing contributes a device no other source reaches —
/// and that is now witnessed rather than assumed** (plan §18 item 9).
///
/// This is the test the ledger item is actually about. The listing's contribution
/// is a `seen.entry(name).or_insert(None)` merge in the resolver, and on every box
/// anyone has ever captured a report from, udev names every port — so the merge
/// finds every key already present, `sysfs_only` reads **0**, and *the report would
/// have printed identically had the merge returned nothing*. Checked against the
/// committed artifacts: `sysfs_only` is 0 in
/// `docs/doctor/linux-7.0-2026-08-05-f8315cc-tier3.json` and its passive sibling,
/// and 0 on Darwin (`macos-24.6.0-2026-08-05-1a9a8fc-tier3.json`), which has no
/// such listing at all.
///
/// The discriminating tree is therefore a **mixed** one: one device udev names,
/// one reachable only through sysfs. The status is deliberately *not* the
/// discriminator — it reads `supported` either way, which is exactly why a status
/// clause could never have caught this.
#[test]
fn the_sysfs_tty_listing_contributes_a_device_no_other_source_can_reach() {
    let t = TmpTree::new("p4-mixed");
    // Distinct usbdir AND distinct serial per device: two devices sharing a serial
    // are ambiguous, and the resolver demotes both to by-path — which would make
    // `canonical` collapse for a reason that has nothing to do with the merge.
    sysfs_usb_tty(t.path(), "1-1", "ttyUSB0", "UNIQ01");
    by_id_link(
        t.path(),
        "usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0",
        "ttyUSB0",
    );
    sysfs_usb_tty(t.path(), "1-2", "ttyUSB1", "UNIQ02");

    let p4 = p4_over(t.path());
    assert_eq!(p4_obs(&p4, "by_id_tree"), Some("present".into()));
    assert_eq!(
        p4_obs(&p4, "count"),
        Some(1.into()),
        "udev named exactly one of the two devices"
    );
    assert_eq!(
        p4_obs(&p4, "sysfs_only"),
        Some(1.into()),
        "the second device is reachable ONLY through the sysfs listing, so it is \
         the listing's contribution made visible: {p4}"
    );
    assert_eq!(
        p4_obs(&p4, "canonical"),
        Some(2.into()),
        "both devices must resolve canonically — the merge adds, it does not \
         replace: {p4}"
    );
    // The by-id device keeps its by-id key: `or_insert` must not clobber an entry
    // udev already named. An `insert` there would leave this observation absent.
    assert_eq!(
        p4_obs(&p4, "usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0"),
        Some("usb:0403:6001:UNIQ01:00".into()),
        "the sysfs merge overwrote a device udev had already named: {p4}"
    );
    assert_eq!(
        p4_obs(&p4, &t.path().join("dev/ttyUSB1").display().to_string()),
        Some("usb:0403:6001:UNIQ02:00".into()),
        "the sysfs-only device is missing from the report: {p4}"
    );
    // The provenance sentence must NOT fire here: a by-id tree exists, so the
    // consequence's precondition is correctly false.
    assert!(
        !p4["consequence"]
            .as_str()
            .unwrap_or_default()
            .contains("No /dev/serial/by-id tree"),
        "the no-udev sentence fired over a tree that has a by-id listing: {p4}"
    );

    // **The negative control** (plan §3 rule 10 — prove the matcher, not only the
    // walker). The same assertions run against a tree with the by-id device alone:
    // `sysfs_only` must fall to 0 and the second device's key must vanish. Without
    // this, every number above could be satisfied by a probe that reported the same
    // figures for any tree at all.
    let c = TmpTree::new("p4-control");
    sysfs_usb_tty(c.path(), "1-1", "ttyUSB0", "UNIQ01");
    by_id_link(
        c.path(),
        "usb-FTDI_FT232R_USB_UART_UNIQ01-if00-port0",
        "ttyUSB0",
    );
    let control = p4_over(c.path());
    assert_eq!(
        p4_obs(&control, "sysfs_only"),
        Some(0.into()),
        "a tree udev names completely must report no sysfs-only device: {control}"
    );
    assert_eq!(p4_obs(&control, "canonical"), Some(1.into()));
    assert_eq!(
        p4_obs(
            &control,
            &c.path().join("dev/ttyUSB1").display().to_string()
        ),
        None,
        "the control tree has no second device, so no key may name one"
    );
}

/// The clause-identity guard for §15.52's handshake presence clause.
#[test]
fn both_expectation_files_carry_the_same_handshake_clause() {
    let clause = r#"and (all(.probes[]; . as $p | ($p.id != "P5")
      or (($p.observations | any((.key | endswith(" cert"))
             and (.value | type == "string") and (.value | startswith("rate_ladder")))) | not)
      or ($p.observations | any((.key | endswith(" handshake")) and (.value | type == "string")))))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        assert!(
            text.contains(clause),
            "{name} does not carry the P5 handshake clause verbatim — a rig-only \
             clause reaches the other platform's lane only through this identity"
        );
    }
}

/// **The handshake clause has teeth, proven against a planted violation** (§15.52,
/// plan §3 rule 10).
///
/// A conditional clause is the shape that passes vacuously most easily: if its
/// antecedent never fires, it is silent forever and looks green. So this builds the
/// antecedent synthetically — a P5 block carrying a pair certificate line that
/// starts with `rate_ladder`, which is what "a pair was certified" looks like — and
/// asserts the gate REJECTS it when the handshake reading is absent and ACCEPTS it
/// when present. Synthetic rather than rig-driven on purpose: the clause must be
/// provable on a CI box with no adapters, which is every box that runs this file.
///
/// The by-hand rig version was run too, on the real crossover: deleting the
/// handshake observation from a live Tier-3 capture makes `jq -e -f
/// expectations/linux.jq` reject it, and the untouched capture passes.
#[test]
fn the_handshake_clause_rejects_a_certified_pair_that_reports_no_handshake() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // Antecedent present, handshake absent — must be REJECTED.
    let planted = jq_filter(
        r#"(.probes[] | select(.id=="P5")) |= (.status = "supported"
           | .observations = [{"key":"a ↔ b cert","value":"rate_ladder=true deliberate_mismatch_observed=true"}])"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &planted),
        "{} admitted a certified pair with no handshake reading — the certificate's \
         own modem map is taken with the peer port closed and cannot answer what the \
         wire carries (§15.52)",
        expectation.display()
    );

    // The same block WITH the reading — must be ACCEPTED, or the rejection above
    // proves nothing about the handshake and everything about some other clause.
    let honest = jq_filter(
        r#"(.probes[] | select(.id=="P5")) |= (.status = "supported"
           | .observations = [{"key":"a ↔ b cert","value":"rate_ladder=true deliberate_mismatch_observed=true"},
                              {"key":"a ↔ b handshake","value":"3-wire: no handshake lines carried [rts_a_to_cts_b=false]"}])"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &honest),
        "{} rejected a certified pair that DID report its handshake — and note the \
         value says 3-wire, which must pass: whether this rig crosses RTS is the \
         operator's cabling, not a gate condition",
        expectation.display()
    );

    // And the antecedent really is conditional: a passive report, which has no pair
    // certificate at all, must pass untouched. Without this the clause could be
    // demanding a handshake from every report and nobody would notice until a
    // rig-less lane went red.
    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest passive report",
        expectation.display()
    );
}

/// **The P14 presence clause must admit a `degraded` report, and it did not**
/// (plan §3 rule 10 — plant the violation in *every* spelling the clause covers).
///
/// The clause exempts `skipped` and requires the cells otherwise, which is right.
/// What was wrong was the probe: two of `p14_verdict`'s six `degraded` arms —
/// a pair whose P5 rate ladder did not round-trip, and a pair that would not
/// reopen — returned *before* the observation block, so they emitted a `degraded`
/// verdict with no `max_reliable_baud` at all and the gate rejected them. A rig
/// whose cable seated a millimetre differently would have turned the lane red for
/// a reason that is not a defect, which is the exact failure the `degraded` arm
/// exists to prevent.
///
/// The original matcher proof missed it because it planted only against
/// `supported` and asserted the honest *passive* report passed. `degraded` is a
/// third spelling and it is the one a real rig produces. Both directions are
/// asserted here: the complete-but-null shape is **accepted**, and the shape with
/// the cells missing is still **rejected**, so the fix widened the probe and not
/// the gate.
#[test]
fn the_p14_presence_clause_admits_a_degraded_report_that_carries_null_cells() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // The shape a rig with a failed baseline ladder produces: `degraded`, the
    // search never ran, the cells present and honestly null.
    let degraded_complete = jq_filter(
        r#"(.probes[] | select(.id=="P14")) |= (.status = "degraded" | del(.reason)
           | .observations = [{"key":"max_reliable_baud","value":null},
                              {"key":"ceiling_kind","value":null},
                              {"key":"structural_max_baud","value":4294967295},
                              {"key":"ceiling_is_a_floor_over","value":"nothing yet — the search did not run on this path; the verdict says why."},
                              {"key":"pairs_discovered","value":1}])"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &degraded_complete),
        "{} rejected a `degraded` P14 that carries every cell with an honest \
         null — so a rig whose P5 ladder did not round-trip reddens the lane for \
         a reason that is not a defect",
        expectation.display()
    );

    // And the fix must not have loosened the clause: the same `degraded` with the
    // cells gone is still refused.
    let degraded_bare = jq_filter(
        r#"(.probes[] | select(.id=="P14")) |= (.status = "degraded" | del(.reason)
           | .observations = [{"key":"pairs_discovered","value":1},
                              {"key":"structural_max_baud","value":4294967295}])"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &degraded_bare),
        "{} admitted a `degraded` P14 carrying no ceiling cells at all — the \
         repair was supposed to widen the probe, not the gate",
        expectation.display()
    );
}

/// **A `supported` P14 that measured nothing is rejected** (notes §3.73).
///
/// The presence clause above deliberately admits `null`, because a search that
/// could not finish still has to carry its keys. That left the complementary hole,
/// and it was a real one: a P14 reading `supported` with `max_reliable_baud: null`
/// and `ceiling_kind: null` satisfied every clause in the file. It was found by
/// mutating a committed artifact — `docs/doctor/linux-7.0-2026-08-05-7cf0338-tier3.json`
/// with P14's measured trio stripped back to its placeholders — and the gate
/// returned 0.
///
/// `p14_verdict` already refuses that combination (it degrades whenever either cell
/// is `None`), so the clause pins a property the probe *promises* rather than an
/// answer it might give: no honest report can trip it, and a refactor that broke
/// the promise now reddens here instead of shipping a confident `supported` with
/// nothing behind it.
///
/// The positive control matters as much as the plant: the same report with the
/// measured values present must still pass, or this would prove only that the gate
/// rejects something.
#[test]
fn the_p14_measured_clause_rejects_a_supported_ceiling_that_measured_nothing() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // A P14 that answered: `supported`, with the number and the reason. This is
    // the shape a rig run produces, reconstructed on a passive box so the guard
    // needs no hardware.
    let answered = r#"(.probes[] | select(.id=="P14")) |= (.status = "supported" | del(.reason)
           | .observations = [{"key":"max_reliable_baud","value":3000000},
                              {"key":"ceiling_kind","value":"adapter-refused"},
                              {"key":"structural_max_baud","value":4294967295},
                              {"key":"ceiling_is_a_floor_over","value":"the rates this ladder probed."},
                              {"key":"pairs_discovered","value":1}])"#;
    assert!(
        gate_accepts(&expectation, &jq_filter(answered, &report)),
        "{} rejected a `supported` P14 that carries its measurement — the clause \
         is supposed to bite only on a ceiling that was never measured",
        expectation.display()
    );

    // The plant, in both spellings that lose the answer while keeping the keys:
    // the number gone, and the reason gone. Either alone must redden, because a
    // reader needs both and the clause claims to require both.
    for (what, null_key) in [
        ("the number", "max_reliable_baud"),
        ("the reason", "ceiling_kind"),
    ] {
        let gutted = jq_filter(
            &answered.replace(
                &format!(r#"{{"key":"{null_key}","value":"#),
                &format!(r#"{{"key":"{null_key}","value":null,"was":"#),
            ),
            &report,
        );
        assert!(
            !gate_accepts(&expectation, &gutted),
            "{} admitted a `supported` P14 whose ceiling is present but null \
             ({what} missing) — the vacuity notes §3.73 found is still open",
            expectation.display()
        );
    }
}

/// **The P4 population clause rejects a count that is not a number** (notes §3.73).
///
/// The clause reads `canonical` and requires it to exceed zero. Until 2026-08-07 it
/// spelled that as `(… // -1) > 0`, and jq orders **strings above numbers** — so a
/// `canonical` of `"2"`, or of any string at all, compared greater than `0` and the
/// clause passed. That is the same defect class the clause itself was written for
/// (notes §3.45 (ii)): a population assertion that cannot see a population it does
/// not recognise.
///
/// Reading through `last` rather than `first` is the second half, and it is what
/// makes the clause safe against a probe that stamps a placeholder and overwrites
/// it — the shape P14 uses and P4 could adopt tomorrow.
#[test]
fn the_p4_population_clause_rejects_a_count_that_is_not_a_number() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // Positive control first: a numeric, positive population passes.
    let numeric = jq_filter(
        r#"(.probes[] | select(.id=="P4")) |= (.status = "supported" | del(.reason)
           | .observations = [{"key":"canonical","value":2}])"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &numeric),
        "{} rejected a `supported` P4 with two canonical identities",
        expectation.display()
    );

    // The plant: the same count as a string. `"2" > 0` is true in jq.
    let stringly = jq_filter(
        r#"(.probes[] | select(.id=="P4")) |= (.status = "supported" | del(.reason)
           | .observations = [{"key":"canonical","value":"2"}])"#,
        &report,
    );
    assert!(
        !gate_accepts(&expectation, &stringly),
        "{} admitted a `supported` P4 whose canonical count is the string \"2\" — \
         jq compares strings above numbers, so the clause is passing on type \
         confusion rather than on a population",
        expectation.display()
    );
}

/// The clause-identity guard for P15's flow-control clause (notes §3.65 E′, §3.68).
///
/// Both files must carry it *verbatim* for the same reason the handshake clause
/// must: the defect P15 instruments was found on Darwin, and a clause that lived
/// only in the lane where the flag is honoured could never have caught it.
#[test]
fn both_expectation_files_carry_the_same_flow_control_clause() {
    let clause = r#"and (any(.probes[]; .id == "P15"))
and (all(.probes[]; . as $p | ($p.id != "P15") or ($p.status | startswith("skipped")) or
      ($p.observations | any((.value | type == "object")
         and (.value.honoured_on_readback | type == "boolean")
         and (.value.tcsetattr_ok | type == "boolean")
         and (.value.baseline_restored | type == "boolean")))))"#;
    for name in ["expectations/linux.jq", "expectations/macos.jq"] {
        let path = repo_root().join(name);
        let text = std::fs::read_to_string(&path).expect("the expectation file is readable");
        assert!(
            text.contains(clause),
            "{name} does not carry the P15 flow-control clause verbatim — the reading \
             it requires is the one the daemon's `load` refusal consults (notes §3.67), \
             so the two lanes must demand the same cells"
        );
    }
}

/// **P15's clause has teeth, proven against planted violations** (plan §3 rule 10,
/// notes §3.68).
///
/// This clause was the one addition of its generation with no in-tree guard, and it
/// is the shape that hides best: CI runs the doctor **passively**, so P15 reports
/// `skipped`, the antecedent is false, and the clause is never evaluated on either
/// automated lane. Deleting it — or dropping `baseline_restored` from the emitter —
/// left `cargo test` and both CI gates green. So the antecedent is built
/// synthetically here, exactly as the handshake guard does, and the four cases that
/// matter are asserted on a box with no adapters.
///
/// The answer stays free on purpose (§7): a kernel that honours `CRTSCTS` and one
/// that drops it are both legitimate facts, so flipping `honoured_on_readback` must
/// *not* redden the gate. That is asserted too — a clause that pinned the answer
/// would have failed on Darwin the moment it was written.
#[test]
fn the_flow_control_clause_rejects_a_reading_that_cannot_say_what_it_measured() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    let p15 = |body: &str| {
        jq_filter(
            &format!(
                r#"(.probes[] | select(.id=="P15")) |= (.status = "supported" | del(.reason) | .observations = {body})"#
            ),
            &report,
        )
    };
    // The software block joined the honest shape at plan §18 item 14: P15's clause
    // set now spans both flag words, so a "complete P15 reading" that carried only
    // the hardware cells would be refused by the sibling clause and every rejection
    // below would then be proving that one instead of this one. Its own plants live
    // in `the_p15_software_clause_rejects_a_reading_that_was_never_taken`.
    const FULL: &str = r#"[{"key":"/dev/ttyUSB0","value":{"honoured_on_readback":true,"tcsetattr_ok":true,"baseline_restored":true,"software_flow_control":{"asks":"…","measured":true,"tcsetattr_ok":true,"honoured_on_readback":true,"silently_dropped":false,"serial2_readback_would_fault":false}}}]"#;

    // The honest shape must be ACCEPTED, or every rejection below proves nothing
    // about this clause and everything about some other one.
    assert!(
        gate_accepts(&expectation, &p15(FULL)),
        "{} refused a complete P15 reading — the rejections below would be vacuous",
        expectation.display()
    );

    // **The answer is free.** A driver that drops the flag is a fact, not a gate
    // failure (§7); this is the whole reason the clause checks type and not value.
    let dropped = FULL.replace(
        r#""honoured_on_readback":true"#,
        r#""honoured_on_readback":false"#,
    );
    assert!(
        gate_accepts(&expectation, &p15(&dropped)),
        "{} reddened on a kernel that drops CRTSCTS — that is Darwin's honest \
         reading and §7 says it is an observation, never a gate failure",
        expectation.display()
    );

    // Each required cell, deleted in turn, must be REJECTED. `baseline_restored`
    // is the one the verdict ranks above its own finding: a probe that reconfigures
    // a real adapter and cannot say it put it back is worse than one that never ran.
    for key in ["honoured_on_readback", "tcsetattr_ok", "baseline_restored"] {
        let holed = FULL.replace(&format!(r#""{key}":true"#), r#""unrelated":true"#);
        assert!(
            !gate_accepts(&expectation, &p15(&holed)),
            "{} admitted a P15 reading with `{key}` absent",
            expectation.display()
        );
        // Present-but-null is the shape a half-written emitter produces, and the
        // clause must not accept it either: `has()` would, `type ==` does not.
        let nulled = FULL.replace(&format!(r#""{key}":true"#), &format!(r#""{key}":null"#));
        assert!(
            !gate_accepts(&expectation, &p15(&nulled)),
            "{} admitted a P15 reading whose `{key}` is null",
            expectation.display()
        );
    }

    // A probe reporting *only* its unreadable ports carries no reading at all. The
    // clause must not be satisfied by that array-valued observation — the `(.value
    // | type == "object")` conjunct is what stops it.
    assert!(
        !gate_accepts(
            &expectation,
            &p15(r#"[{"key":"unreadable_ports","value":["/dev/ttyUSB0: open: EBUSY"]}]"#)
        ),
        "{} admitted a `supported` P15 that measured nothing and only listed the \
         ports it could not open",
        expectation.display()
    );
}

/// **The kernel-diff clauses bite on a probe that ERRORED — until 2026-08-12 they
/// could not** (§13, "`skipped` is never an error path's output"; plan §3 rule 10).
///
/// `skipped` is the one word every conditional clause in both expectation files
/// exempts, and the exemption is right: a `--port`-gated probe must not redden a
/// passive lane. What it cannot survive is an *error* path wearing it. P8, P9 and
/// P10 each routed their catch-all `Err` arm to `skipped (probe error: …)`, so a
/// report in which the measurement never happened satisfied P8's and P9's status
/// clauses, P9's `zero_timeout_by_fd_state` discriminator and both of P10's
/// content clauses — every clause written to notice a missing measurement went
/// green *because* the measurement was missing. Measured before the repair, by
/// splicing the probes' own error-arm output into a live report: exit **0** with
/// the old spelling, exit **1** with the new one, for each of the three
/// separately.
///
/// This is the **gate half** of the two-guard pattern (§13's gate-design rule 2).
/// It proves the clauses reject the shape the repaired probes emit, and — the half
/// that keeps it honest — that they *accept* the identical failure spelled
/// `skipped`, which is what makes the probe's choice of word the whole defence:
/// `reason` is free text and no clause can read a failure out of it. The **probe
/// half**, that the three arms really do emit `degraded` with the error named,
/// is `a_kernel_diff_probe_that_errored_degrades_and_names_the_error` in
/// `doctor/src/probes.rs`; neither half substitutes for the other, and a repair
/// that moved to the gate instead would have to amend §13 rather than delete a
/// control here.
///
/// The plants carry the emitter's shape exactly: `degraded` has no `reason` (that
/// field belongs to `skipped`) and one `probe_error` observation. That last detail
/// is load-bearing — it satisfies the "a probe that RAN must carry measurements"
/// clause, so the rejections below are attributable to the status and content
/// clauses rather than to an empty observation list.
#[test]
fn the_kernel_diff_clauses_bite_on_a_probe_that_errored() {
    if !have_jq() {
        eprintln!("skipping: jq is not on PATH (CI has it — it runs these files)");
        return;
    }
    let expectation = platform_expectation();
    let out = Command::new(serial_nexus_itest::bin("serial-nexus-doctor"))
        .arg("--json")
        .output()
        .expect("the doctor runs");
    let report = String::from_utf8(out.stdout).expect("the report is utf-8");

    // The honest passive report must PASS, or every rejection below is about some
    // other clause entirely.
    assert!(
        gate_accepts(&expectation, &report),
        "{} rejected an honest passive report",
        expectation.display()
    );

    // The realistic failure for all three: a box whose pty allocation does not
    // work. One error, three probes, because they share `new_master`.
    const ERROR: &str = "posix_openpt: ENOENT: No such file or directory";

    for id in ["P8", "P9", "P10"] {
        // The shape `measurement_failed` emits (`doctor/src/probes.rs`).
        let degraded = jq_filter(
            &format!(
                r#"(.probes[] | select(.id=="{id}")) |= (.status = "degraded" | del(.reason)
                   | .observations = [{{"key":"probe_error","value":"{ERROR}"}}])"#
            ),
            &report,
        );
        assert!(
            !gate_accepts(&expectation, &degraded),
            "{} admitted a {id} that ERRORED: the measurement was never taken and \
             the clauses that read it certified it anyway (§13)",
            expectation.display()
        );

        // The same failure in the spelling it wore until 2026-08-12 — accepted,
        // and that is the point rather than an oversight. No clause can tell this
        // from a legitimate skip, because `reason` is free text; the word is the
        // only signal, so the probe is where the rule has to live. Tightening the
        // gate here instead is a design change (§13's rule 3), not an edit to this
        // control.
        let skipped = jq_filter(
            &format!(
                r#"(.probes[] | select(.id=="{id}")) |= (.status = "skipped"
                   | .reason = "probe error: {ERROR}" | .observations = [])"#
            ),
            &report,
        );
        assert!(
            gate_accepts(&expectation, &skipped),
            "{} rejected a {id} that spelled its error `skipped` — if that is now \
             deliberate, §13's rule 3 and the three arms in `doctor/src/probes.rs` \
             need re-reading together, because this control is the reason they \
             route to `degraded`",
            expectation.display()
        );
    }

    // P10's own control, and it is not the same as the two above. Its status clause
    // *admits* `degraded` — a slave measured in a non-raw line discipline degrades
    // by design (§7.2) — so the rejection above has to come from the two content
    // clauses rather than from the word. This asserts exactly that: the same
    // `degraded`, with the direction measurements left in place, still passes. A
    // repair that reddened this arm would turn a legitimate reading into a lane
    // failure.
    let degraded_with_content = jq_filter(
        r#"(.probes[] | select(.id=="P10")) |= (.status = "degraded" | del(.reason))"#,
        &report,
    );
    assert!(
        gate_accepts(&expectation, &degraded_with_content),
        "{} rejected a `degraded` P10 that carries both directions' measurements — \
         that is the non-raw line-discipline arm, a legitimate reading, and the \
         rejection above is then about the verdict word rather than the missing \
         measurement",
        expectation.display()
    );
}
