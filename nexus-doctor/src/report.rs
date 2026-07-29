//! The doctor's report model and its two renderings (design §15.17, plan §3).
//!
//! Every probe is self-judging: a question, the behavior observed, a
//! [`Status`] verdict, and the one-line design consequence. The Markdown
//! rendering is written to be pasted whole into an issue thread; the JSON twin
//! is for CI (`nexus-doctor --json | jq -e ...`).

use serde::Serialize;

/// A probe or environment-check verdict. `Degraded` means a design fallback
/// applies (e.g. EXTPROC notify absent → §7.2 runs poll-only) — the daemon
/// still works. `Unsupported` means an assumption with no fallback failed and
/// is a stop condition (plan §4). `Skipped` carries the reason.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Status {
    Supported,
    Degraded,
    Unsupported,
    Skipped { reason: String },
}

impl Status {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Status::Skipped {
            reason: reason.into(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Status::Supported => "supported",
            Status::Degraded => "degraded",
            Status::Unsupported => "unsupported",
            Status::Skipped { .. } => "skipped",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Status::Supported => "✅",
            Status::Degraded => "⚠️",
            Status::Unsupported => "❌",
            Status::Skipped { .. } => "⏭️",
        }
    }

    /// Markdown badge + label, e.g. `✅ supported` or `⏭️ skipped (no --port)`.
    pub fn badge_label(&self) -> String {
        match self {
            Status::Skipped { reason } => format!("{} skipped ({reason})", self.badge()),
            _ => format!("{} {}", self.badge(), self.label()),
        }
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Status::Unsupported)
    }
}

/// One observed fact within a probe (key → value), preserving order for a
/// stable report.
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub key: String,
    pub value: serde_json::Value,
}

pub fn obs(key: &str, value: impl Into<serde_json::Value>) -> Observation {
    Observation {
        key: key.to_owned(),
        value: value.into(),
    }
}

/// A single capability probe.
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub id: String,
    pub title: String,
    pub question: String,
    pub observations: Vec<Observation>,
    #[serde(flatten)]
    pub status: Status,
    pub consequence: String,
}

impl Probe {
    pub fn new(id: &str, title: &str, question: &str) -> Self {
        Probe {
            id: id.to_owned(),
            title: title.to_owned(),
            question: question.to_owned(),
            observations: Vec::new(),
            status: Status::Supported,
            consequence: String::new(),
        }
    }

    pub fn observe(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.observations.push(obs(key, value));
        self
    }

    pub fn verdict(mut self, status: Status, consequence: &str) -> Self {
        self.status = status;
        self.consequence = consequence.to_owned();
        self
    }
}

/// An environment check (kernel, permissions, by-id presence, …).
#[derive(Debug, Clone, Serialize)]
pub struct EnvCheck {
    pub name: String,
    pub value: String,
    #[serde(flatten)]
    pub status: Status,
}

impl EnvCheck {
    pub fn new(name: &str, value: impl Into<String>, status: Status) -> Self {
        EnvCheck {
            name: name.to_owned(),
            value: value.into(),
            status,
        }
    }
}

/// What produced this report — the provenance a cross-kernel diff rests on.
///
/// P6–P11 exist to be diffed field by field between two boxes (§13), and that
/// comparison is only meaningful between two runs of the *same probe set*. Until
/// this block existed, neither renderer said anything about the build: the header
/// was a bare `nexus-doctor v0.2.0`, the JSON carried `tool`/`version` and a
/// `generated_unix_ms` the Markdown dropped entirely. Establishing that the
/// 2026-07-27 Linux 6.18 report predated HEAD took forensics on a *section title*
/// — its P4 block still carried the pre-`RES-2` wording — and `expectations/
/// linux.jq`, whose stated job is proving the artifact "diffable field by field",
/// had no clause that could see it.
///
/// Two identifiers, because they answer different questions.
#[derive(Debug, Clone, Serialize)]
pub struct Build {
    /// The git commit the binary was built from (`<short sha>` or
    /// `<short sha>-dirty`), or `unknown` where git could not answer — a tarball,
    /// a container without git, a `cargo package` staging tree. Stamped by
    /// `build.rs`. Answers *which tree*, and requires the reader to know what
    /// changed between two commits.
    pub commit: &'static str,
    /// A digest over the deduplicated, sorted set of every probe's
    /// `(id, question)` — see [`probe_set_fingerprint`] for why each of
    /// "deduplicated", "sorted" and the omission of the title is load-bearing.
    /// Answers the question a diff actually needs —
    /// **are these two artifacts comparable?** — in one glance and with no
    /// repository access, which is what the commit alone cannot do. Equal
    /// fingerprints mean the two runs asked the same questions of their kernels;
    /// different ones mean the probe set moved and a field-by-field diff is
    /// reading two different instruments.
    ///
    /// Not cryptographic and does not need to be: it detects drift, it does not
    /// resist an adversary, and a hash with that job must not drag a dependency
    /// through the licensing gate.
    pub probe_set: String,
}

/// The whole report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub version: &'static str,
    pub build: Build,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_unix_ms: Option<u64>,
    /// The same instant as `generated_unix_ms`, rendered — so the Markdown twin,
    /// which is what actually gets pasted into a support request and committed
    /// beside a design note, carries a date at all. It used to carry none: a
    /// committed 6.18 report would have been datable only by its commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_utc: Option<String>,
    pub environment: Vec<EnvCheck>,
    pub probes: Vec<Probe>,
    pub summary: Summary,
}

/// Verdict tallies, for a quick machine read and the report footer.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub supported: usize,
    pub degraded: usize,
    pub unsupported: usize,
    pub skipped: usize,
}

impl Report {
    pub fn new(
        generated_unix_ms: Option<u64>,
        environment: Vec<EnvCheck>,
        probes: Vec<Probe>,
    ) -> Self {
        let mut summary = Summary::default();
        for s in environment
            .iter()
            .map(|c| &c.status)
            .chain(probes.iter().map(|p| &p.status))
        {
            match s {
                Status::Supported => summary.supported += 1,
                Status::Degraded => summary.degraded += 1,
                Status::Unsupported => summary.unsupported += 1,
                Status::Skipped { .. } => summary.skipped += 1,
            }
        }
        let build = Build {
            // `option_env!`, not `env!`: under cargo the build script always emits
            // this, but a build system that skips build scripts (bazel, nix) would
            // otherwise fail to compile rather than report what it does not know.
            commit: option_env!("SNX_BUILD_COMMIT").unwrap_or("unknown"),
            probe_set: probe_set_fingerprint(&probes),
        };
        Report {
            tool: "nexus-doctor",
            version: env!("CARGO_PKG_VERSION"),
            build,
            generated_utc: generated_unix_ms.map(format_unix_ms_utc),
            generated_unix_ms,
            environment,
            probes,
            summary,
        }
    }

    /// True if any probe is `unsupported` — the process exit code hinges on it
    /// (a probe contradicting the design is a stop condition, plan §4).
    pub fn any_unsupported(&self) -> bool {
        self.probes.iter().any(|p| p.status.is_unsupported())
            || self.environment.iter().any(|c| c.status.is_unsupported())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serializes")
    }

    /// The copy-pasteable Markdown report.
    pub fn to_markdown(&self) -> String {
        let mut m = String::new();
        m.push_str("# nexus-doctor report\n\n");
        m.push_str(&format!(
            "`{}` v{} — paste this whole report into a support request.\n\n",
            self.tool, self.version
        ));
        // Provenance before anything else. A reader diffing this against another
        // kernel's report compares `probe_set` first: equal means the two runs
        // asked the same questions, unequal means a field-by-field diff is
        // reading two different instruments regardless of how alike the numbers
        // look. `generated_utc` is here because the Markdown carried no date at
        // all, and it is the twin that gets pasted and committed.
        m.push_str("## Build\n\n");
        m.push_str("| Field | Value |\n|---|---|\n");
        m.push_str(&format!("| commit | `{}` |\n", self.build.commit));
        m.push_str(&format!("| probe set | `{}` |\n", self.build.probe_set));
        m.push_str(&format!(
            "| generated | {} |\n",
            self.generated_utc.as_deref().unwrap_or("unknown")
        ));
        m.push_str("\n**Diffing this against another kernel?** Compare `probe set` first — an unequal fingerprint means the two runs do not ask the same questions, and the numbers below are not comparable field by field (§13).\n\n");

        m.push_str("## Environment\n\n");
        m.push_str("| Check | Value | Verdict |\n|---|---|---|\n");
        for c in &self.environment {
            m.push_str(&format!(
                "| {} | {} | {} |\n",
                c.name,
                md_escape(&c.value),
                c.status.badge_label()
            ));
        }
        m.push('\n');

        m.push_str("## Probes\n\n");
        for p in &self.probes {
            m.push_str(&format!(
                "### {} — {} — {}\n\n",
                p.id,
                p.title,
                p.status.badge_label()
            ));
            m.push_str(&format!("**Question:** {}\n\n", p.question));
            if !p.observations.is_empty() {
                m.push_str("**Observed:**\n\n");
                for o in &p.observations {
                    m.push_str(&format!("- `{}`: {}\n", o.key, render_value(&o.value)));
                }
                m.push('\n');
            }
            if !p.consequence.is_empty() {
                m.push_str(&format!("**Consequence:** {}\n\n", p.consequence));
            }
        }

        m.push_str("## Summary\n\n");
        m.push_str(&format!(
            "{} supported · {} degraded · {} unsupported · {} skipped\n",
            self.summary.supported,
            self.summary.degraded,
            self.summary.unsupported,
            self.summary.skipped
        ));
        m
    }
}

/// Digest the probe set into the 16 hex chars a reader compares across two
/// reports (see [`Build::probe_set`]).
///
/// **It must be a property of the binary, not of the invocation** — that is the
/// whole contract, and getting it wrong is worse than having no field, because
/// the report prints "the numbers below are not comparable" off the back of it.
/// Three deliberate choices make it one:
///
/// 1. **`(id, question)`, never `title`.** P3's title embeds the device path
///    (`serial-port fit (/dev/ttyUSB0)`), so digesting titles fingerprints *which
///    ports were named*. The 6.18 box has one adapter and the 7.0 box has two:
///    the very diff this block exists to underwrite would have reported itself
///    invalid. The question still catches the motivating case, `RES-2` having
///    rewritten P4's question along with its title.
/// 2. **Deduplicated and sorted**, so N `--port`s (N copies of P3, one question)
///    fingerprint as one, and report ordering is not mistaken for probe-set
///    identity. Order and arity are properties of the run, not of the instrument.
/// 3. **Observations and verdicts stay out.** They are the measurements the diff
///    exists to compare; two healthy boxes differ in every one of them by design,
///    so folding them in reports every real cross-kernel pair as incomparable —
///    the field being wrong in the direction nobody would notice.
///
/// Placeholder probes (`main`'s no-`--port` arms) must therefore carry the real
/// probe's question verbatim, which is what [`crate::probes::P3_QUESTION`] and
/// [`crate::probes::P5_QUESTION`] exist for.
///
/// FNV-1a over a length-delimited encoding. The delimiters are not decoration —
/// concatenating raw fields lets an id's tail move into the next question with no
/// change in the digest, which is the classic way a fingerprint quietly stops
/// fingerprinting.
fn probe_set_fingerprint(probes: &[Probe]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut pairs: Vec<(&str, &str)> = probes
        .iter()
        .map(|p| (p.id.as_str(), p.question.as_str()))
        .collect();
    pairs.sort_unstable();
    pairs.dedup();

    let mut h = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(PRIME);
        }
    };
    for (id, question) in pairs {
        for field in [id, question] {
            eat(&(field.len() as u64).to_le_bytes());
            eat(field.as_bytes());
        }
    }
    format!("{h:016x}")
}

/// Render a Unix millisecond timestamp as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Hand-rolled because the doctor's dependency list is part of the licensing gate
/// and a date crate is a poor trade for one line of a report. The civil-date
/// conversion is Howard Hinnant's `civil_from_days`, which is exact for the whole
/// proleptic Gregorian range and needs no table — the shape to keep if this is
/// ever touched, since the tempting "days / 365 plus leap years" version drifts.
///
/// UTC only, deliberately: the reports it stamps get compared across two machines
/// in two places, and a local-time stamp would make that comparison a puzzle.
fn format_unix_ms_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Days since the Unix epoch → `(year, month, day)`, proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap day lands at the end of the cycle.
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March = 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Render an observation value for the Markdown arm.
///
/// Scalars render bare. A structured value (P6/P7 report whole measurement
/// blocks, because a verdict word is not diffable across kernels) renders as
/// `key=value` pairs rather than raw JSON — the same facts the `--json` twin
/// carries, in a line a human can read in an issue thread.
fn render_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => md_escape(
            &map.iter()
                .map(|(k, val)| format!("{k}={}", inline_value(val)))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        serde_json::Value::Array(items) => md_escape(&inline_array(items)),
        other => md_escape(&inline_value(other)),
    }
}

/// One value inside a rendered structure: strings unquoted, containers compact.
fn inline_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => inline_array(items),
        serde_json::Value::Object(map) => format!(
            "[{}]",
            map.iter()
                .map(|(k, val)| format!("{k}={}", inline_value(val)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        other => other.to_string(),
    }
}

fn inline_array(items: &[serde_json::Value]) -> String {
    if items.is_empty() {
        return "(none)".to_owned();
    }
    format!(
        "[{}]",
        items.iter().map(inline_value).collect::<Vec<_>>().join(" ")
    )
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(id: &str, title: &str, question: &str) -> Probe {
        Probe::new(id, title, question).verdict(Status::Supported, "fine")
    }

    /// The UTC stamp must agree with `date -u` on the cases a naive
    /// days-since-epoch conversion gets wrong: the epoch itself, a leap day in a
    /// 400-divisible year, a leap day in an ordinary leap year, and the day after
    /// a century that is **not** a leap year. All four verified against `date -u`
    /// on the dev box.
    #[test]
    fn the_utc_stamp_agrees_with_date_on_the_cases_that_break_naive_conversions() {
        for (secs, expect) in [
            (0u64, "1970-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (1_709_164_800, "2024-02-29T00:00:00Z"),
            (4_107_542_400, "2100-03-01T00:00:00Z"),
            (1_785_260_059, "2026-07-28T17:34:19Z"),
        ] {
            assert_eq!(format_unix_ms_utc(secs * 1000), expect, "at {secs}");
        }
        // Sub-second precision is truncated, never rounded into the next second.
        assert_eq!(format_unix_ms_utc(999), "1970-01-01T00:00:00Z");
    }

    /// The fingerprint's whole job is telling a reader whether two reports are
    /// comparable, so it must move on the change that actually broke a diff and
    /// hold still on the ones that must not.
    ///
    /// The motivating case is real: review 32's `RES-2` renamed P4 from "by-id
    /// resolution ground truth" and rewrote its question, and nothing in either
    /// renderer noticed — establishing that the 2026-07-27 6.18 report predated
    /// HEAD took reading a section title by eye.
    #[test]
    fn the_probe_set_fingerprint_moves_on_a_probe_rewrite_and_not_on_a_measurement() {
        let before = vec![probe(
            "P4",
            "by-id resolution ground truth",
            "Does /dev/serial/by-id …?",
        )];
        let after = vec![probe(
            "P4",
            "device identity resolution",
            "Does the resolver's one source …?",
        )];
        assert_ne!(
            probe_set_fingerprint(&before),
            probe_set_fingerprint(&after),
            "a renamed-and-rewritten probe kept its fingerprint — the RES-2 case"
        );

        // Deterministic across calls, and 16 hex chars.
        let f = probe_set_fingerprint(&before);
        assert_eq!(f, probe_set_fingerprint(&before));
        assert_eq!(f.len(), 16);
        assert!(f.chars().all(|c| c.is_ascii_hexdigit()), "{f}");

        // **Measurements must not feed it.** Two healthy boxes differ in every
        // observation and verdict by design; folding those in would report every
        // real cross-kernel pair as incomparable, which is the field being wrong
        // in the direction nobody would notice.
        let quiet = vec![probe("P6", "t", "q")];
        let busy = vec![
            Probe::new("P6", "t", "q")
                .observe("pollin_passes", 41)
                .verdict(Status::Degraded, "different kernel"),
        ];
        assert_eq!(
            probe_set_fingerprint(&quiet),
            probe_set_fingerprint(&busy),
            "an observation or verdict changed the fingerprint"
        );

        // Length delimiters: without them a field's tail slides into the next
        // field with no change in the digest, and the fingerprint silently stops
        // fingerprinting.
        assert_ne!(
            probe_set_fingerprint(&[probe("P1x", "t", "q")]),
            probe_set_fingerprint(&[probe("P1", "t", "xq")]),
            "a field boundary shift went undetected"
        );

        // A probe genuinely added or removed is a different instrument.
        assert_ne!(
            probe_set_fingerprint(&[probe("P1", "t", "q")]),
            probe_set_fingerprint(&[probe("P1", "t", "q"), probe("P2", "t", "q2")])
        );
    }

    /// **The fingerprint is a property of the binary, not of the invocation.**
    /// This is the contract the report prints ("an unequal fingerprint means the
    /// two runs do not ask the same questions"), and the first implementation
    /// broke it in the one way that mattered: it digested `title`, and P3's title
    /// embeds the device path while P3 is emitted once per `--port`. The 6.18 box
    /// has one adapter and the 7.0 box has two, so the very cross-kernel diff the
    /// Build block was added for would have declared itself invalid.
    #[test]
    fn the_fingerprint_ignores_how_many_ports_were_named_and_which() {
        // One binary, three invocations: passive, one port, two ports.
        let passive = vec![
            probe("P3", "serial-port fit", crate::probes::P3_QUESTION),
            probe(
                "P5",
                "rig discovery and certification",
                crate::probes::P5_QUESTION,
            ),
        ];
        let one_port = vec![
            probe(
                "P3",
                "serial-port fit (/dev/ttyUSB12)",
                crate::probes::P3_QUESTION,
            ),
            probe(
                "P5",
                "rig discovery and certification",
                crate::probes::P5_QUESTION,
            ),
        ];
        let two_ports = vec![
            probe(
                "P3",
                "serial-port fit (/dev/ttyUSB0)",
                crate::probes::P3_QUESTION,
            ),
            probe(
                "P3",
                "serial-port fit (/dev/ttyUSB1)",
                crate::probes::P3_QUESTION,
            ),
            probe(
                "P5",
                "rig discovery and certification",
                crate::probes::P5_QUESTION,
            ),
        ];
        let f = probe_set_fingerprint(&passive);
        assert_eq!(
            f,
            probe_set_fingerprint(&one_port),
            "the device path moved it"
        );
        assert_eq!(
            f,
            probe_set_fingerprint(&two_ports),
            "the port count moved it"
        );

        // Report ordering is a property of the run too.
        let mut shuffled = two_ports.clone();
        shuffled.reverse();
        assert_eq!(f, probe_set_fingerprint(&shuffled), "report order moved it");

        // And the placeholder arms `main` emits without `--port` must carry the
        // real questions verbatim, or the invariance above is accidental.
        assert!(!crate::probes::P3_QUESTION.is_empty());
        assert!(!crate::probes::P5_QUESTION.is_empty());
    }

    /// Both renderers must carry the provenance. The Markdown arm is the one that
    /// regressed in practice: it dropped `generated_unix_ms` entirely, so a
    /// committed report was datable only by its commit — and it never said which
    /// build produced it at all.
    #[test]
    fn both_renderers_carry_the_build_identity_and_the_timestamp() {
        let r = Report::new(Some(1_785_260_059_000), vec![], vec![probe("P1", "t", "q")]);

        let md = r.to_markdown();
        assert!(
            md.contains("2026-07-28T17:34:19Z"),
            "no date in Markdown:\n{md}"
        );
        assert!(md.contains(r.build.commit), "no commit in Markdown:\n{md}");
        assert!(
            md.contains(&r.build.probe_set),
            "no probe-set fingerprint in Markdown:\n{md}"
        );

        let v: serde_json::Value = serde_json::from_str(&r.to_json()).expect("json");
        assert_eq!(v["build"]["commit"], r.build.commit);
        assert_eq!(v["build"]["probe_set"], r.build.probe_set);
        assert_eq!(v["generated_unix_ms"], 1_785_260_059_000u64);
        assert_eq!(v["generated_utc"], "2026-07-28T17:34:19Z");

        // A run whose clock could not be read still renders, and says so rather
        // than inventing an epoch date.
        let undated = Report::new(None, vec![], vec![probe("P1", "t", "q")]);
        assert!(undated.to_markdown().contains("| generated | unknown |"));
        assert!(!undated.to_markdown().contains("1970-01-01"));
    }
}
