//! The doctor's report model and its two renderings (design §15.17, plan §3).
//!
//! Every probe is self-judging: a question, the behavior observed, a
//! [`Status`] verdict, and the one-line design consequence. The Markdown
//! rendering is written to be pasted whole into an issue thread; the JSON twin
//! is for CI (`serial-nexus-doctor --json | jq -e ...`).

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
/// was a bare `serial-nexus-doctor v0.2.0`, the JSON carried `tool`/`version` and a
/// `generated_unix_ms` the Markdown dropped entirely. Establishing that the
/// 2026-07-27 Linux 6.18 report predated HEAD took forensics on a *section title*
/// — its P4 block still carried the pre-`RES-2` wording — and `expectations/
/// linux.jq`, whose stated job is proving the artifact "diffable field by field",
/// had no clause that could see it.
///
/// Three identifiers, because they answer three different questions: *which
/// tree*, *which questions*, and *which cells*.
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
    /// Answers **did these two runs ask the same questions?** in one glance and
    /// with no repository access, which is what the commit alone cannot do.
    ///
    /// **Only the unequal direction is a verdict.** Unequal means the probe set
    /// moved and a field-by-field diff is reading two different instruments.
    /// Equal means the two runs used the same question *text* — it does **not**
    /// mean they measured the same way, and it does **not** mean the reports are
    /// comparable field by field. The digest covers the question *strings*, not
    /// the code that asks them: between `fa4b12d6f529` and `7ead470f594c` this
    /// value held at `a131e1f4b46d6c83` while P10's hostward figure moved by a
    /// factor of 4104, between `7ead470f594c` and `1a9a8fca1c36` it held while 65
    /// observation leaf paths appeared, and at `df48bfcc4fac` it held again while
    /// P4 gained four (`canonical`, `topology_only`, `unidentified`,
    /// `sysfs_tty_listing`). [`Build::field_set`] is the field that sees that;
    /// `docs/doctor/README.md` carries the counterexample.
    ///
    /// Not cryptographic and does not need to be: it detects drift, it does not
    /// resist an adversary, and a hash with that job must not drag a dependency
    /// through the licensing gate.
    pub probe_set: String,
    /// A digest over the sorted, deduplicated set of **scalar leaf paths** this
    /// report's observations actually carry — `<probe id>.<key>[.nested…]`, with
    /// arrays collapsed to one `[]` step and every *value* excluded. See
    /// [`field_set_fingerprint`].
    ///
    /// **Equal means the two reports carry exactly the same cells**, so a
    /// field-by-field diff has no missing ones. That is provable rather than
    /// implied, because the digest is computed from the very thing it certifies
    /// — and it is the statement `probe_set` was being read as making and does
    /// not make. Unequal means the cell sets differ and the diff must be
    /// restricted to their intersection; it does *not* say why they differ
    /// (binary, platform, attached hardware, or a histogram key this run did not
    /// observe are all causes). **Equal still does not certify that the probe
    /// bodies match**: a body change that moves a number without adding a key is
    /// invisible here, exactly as it is to `probe_set`.
    ///
    /// Unlike `commit` and `probe_set` this is a property of the **run**, not of
    /// the binary: naming two pty slaves adds 19 leaf paths to this binary's
    /// output on this box, and the same binary emits 72 Linux-only and 22
    /// macOS-only paths across the two kernels — both measured, and both honest,
    /// because those diffs really are partial (notes §3.51).
    pub field_set: String,
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
            field_set: field_set_fingerprint(&probes),
        };
        Report {
            tool: "serial-nexus-doctor",
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
        m.push_str("# serial-nexus-doctor report\n\n");
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
            "| field set (this run) | `{}` |\n",
            self.build.field_set
        ));
        m.push_str(&format!(
            "| generated | {} |\n",
            self.generated_utc.as_deref().unwrap_or("unknown")
        ));
        m.push_str(
            "\n**Diffing this against another kernel?** Compare both digests. An unequal `probe \
             set` means the two runs do not ask the same questions and the numbers below are not \
             comparable at all (§13). An **equal** `probe set` is not a green light: it digests \
             the question *text*, not the code that asks it. `field set` is the one that answers \
             \"same cells?\" — equal means every observation present in one report is present in \
             the other, unequal means diff only their intersection. Neither digest can see a \
             probe body that changed a number without changing a key.\n\n",
        );

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
/// FNV-1a over a length-delimited encoding ([`fnv1a_delimited`]).
///
/// **What it cannot do**, stated here because the report used to print the
/// opposite: equality does not license a field-by-field diff. It digests the
/// question *strings*, and a probe body can gain cells — or change what it
/// measures — with its question untouched. [`field_set_fingerprint`] is the
/// digest that sees that.
fn probe_set_fingerprint(probes: &[Probe]) -> String {
    let mut pairs: Vec<(&str, &str)> = probes
        .iter()
        .map(|p| (p.id.as_str(), p.question.as_str()))
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    // Flattening to [id, question, id, question, …] reproduces the original
    // encoder byte for byte, which is why `a131e1f4b46d6c83` survives this
    // refactor — pinned by `the_shared_encoder_is_byte_stable`.
    fnv1a_delimited(pairs.into_iter().flat_map(|(id, question)| [id, question]))
}

/// FNV-1a over a length-delimited encoding of `fields`, 16 lowercase hex chars.
///
/// The delimiters are not decoration — concatenating raw fields lets one field's
/// tail move into the next with no change in the digest, which is the classic way
/// a fingerprint quietly stops fingerprinting.
fn fnv1a_delimited<'a>(fields: impl Iterator<Item = &'a str>) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(PRIME);
        }
    };
    for field in fields {
        eat(&(field.len() as u64).to_le_bytes());
        eat(field.as_bytes());
    }
    format!("{h:016x}")
}

/// Digest the **cells** this report carries (see [`Build::field_set`]).
///
/// Why this exists: `probe_set` digests each probe's `question` *string*, and a
/// probe body can gain fields — or change what it measures — with the question
/// untouched. On 2026-08-05 five commits printed one `probe_set` while the macOS
/// observation set gained 65 leaf paths and P10's hostward figure moved 4104x.
/// The standing workaround was prose in `docs/doctor/README.md` announcing each
/// addition by hand; this field makes the announcement machine-checkable.
///
/// Four deliberate choices, each measured rather than assumed (§7):
///
/// 1. **Paths, never values.** Two healthy boxes differ in every measurement, so
///    a value-sensitive digest reports every real pair as incomparable — the same
///    trap [`probe_set_fingerprint`]'s choice 3 avoids.
/// 2. **The scalar's JSON *kind* is excluded too.** Measured on the same-binary
///    cross-kernel pair of 2026-08-05: 4 of 213 shared paths differ in kind, and
///    all four are measurements — `P10.*.pending_output_bytes` reads a number
///    where `TIOCOUTQ` answers and `null` where it does not, and
///    `P7.*.leading_bytes_hex` is a populated array on one kernel and empty on
///    the other. An empty array therefore digests to the same `…[]` path as a
///    populated one.
/// 3. **An empty object digests as the path itself**, so an observation can never
///    vanish from the shape. `P8.slave_open_idle.epoll_flags_seen` is `{}` in the
///    committed Linux runs; a populated one contributes
///    `…epoll_flags_seen.EPOLLHUP` instead, and the digest moves. That is
///    correct: those cells really are absent.
/// 4. **Nothing is excluded by name.** Device-identity keys (`P11./dev/ttyUSB0.…`)
///    and outcome-keyed histograms (`read_outcomes.EIO` vs `…eof`) are included,
///    because a diff across them genuinely has missing cells. An exclusion list
///    would be a hand-kept list in a gate, and would rot on the next probe that
///    keys a map by what it observed.
///
/// Consequently this is a property of the **run**, not of the binary, and it is
/// deliberately *not* a second instrument-identity digest: no digest computed
/// from a report can be one.
fn field_set_fingerprint(probes: &[Probe]) -> String {
    field_set_core(probes.iter().flat_map(|p| {
        p.observations
            .iter()
            .map(move |o| (p.id.as_str(), o.key.as_str(), &o.value))
    }))
}

/// [`field_set_fingerprint`] over an already-captured report, so the frozen
/// artifacts in `docs/doctor/` can be indexed without being edited (§16.13).
///
/// One core, two callers: the emitting path and this one must never drift, which
/// `the_two_field_set_paths_agree_on_a_real_report` asserts by round-tripping a
/// built report through `to_json`.
///
/// `None` on anything that is not a doctor report. Recursion depth is bounded by
/// `serde_json`'s own 128-level parse limit, so a malformed input is rejected by
/// the parser before [`field_paths`] ever sees it.
pub fn field_set_of_report_json(report: &serde_json::Value) -> Option<String> {
    let probes = report.get("probes")?.as_array()?;
    let mut triples: Vec<(&str, &str, &serde_json::Value)> = Vec::new();
    for p in probes {
        let id = p.get("id")?.as_str()?;
        let Some(observations) = p.get("observations") else {
            continue;
        };
        for o in observations.as_array()? {
            let key = o.get("key")?.as_str()?;
            let value = o.get("value")?;
            triples.push((id, key, value));
        }
    }
    Some(field_set_core(triples.into_iter()))
}

fn field_set_core<'a, I>(observations: I) -> String
where
    I: Iterator<Item = (&'a str, &'a str, &'a serde_json::Value)>,
{
    let mut paths: Vec<String> = Vec::new();
    for (id, key, value) in observations {
        field_paths(&format!("{id}.{key}"), value, &mut paths);
    }
    paths.sort_unstable();
    paths.dedup();
    fnv1a_delimited(paths.iter().map(String::as_str))
}

/// Every scalar leaf path under `value`, rooted at `prefix`.
fn field_paths(prefix: &str, value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                field_paths(&format!("{prefix}.{k}"), v, out);
            }
        }
        serde_json::Value::Array(items) => {
            let step = format!("{prefix}[]");
            if items.is_empty() {
                out.push(step);
            } else {
                for item in items {
                    field_paths(&step, item, out);
                }
            }
        }
        // Scalars — and the empty object, whose path is its own leaf (choice 3).
        _ => out.push(prefix.to_owned()),
    }
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
        assert!(
            md.contains(&r.build.field_set),
            "no field-set digest in Markdown — the reader is back to one digest \
             that cannot answer \"same cells?\":\n{md}"
        );

        let v: serde_json::Value = serde_json::from_str(&r.to_json()).expect("json");
        assert_eq!(v["build"]["commit"], r.build.commit);
        assert_eq!(v["build"]["probe_set"], r.build.probe_set);
        assert_eq!(v["build"]["field_set"], r.build.field_set);
        assert_eq!(v["generated_unix_ms"], 1_785_260_059_000u64);
        assert_eq!(v["generated_utc"], "2026-07-28T17:34:19Z");

        // A run whose clock could not be read still renders, and says so rather
        // than inventing an epoch date.
        let undated = Report::new(None, vec![], vec![probe("P1", "t", "q")]);
        assert!(undated.to_markdown().contains("| generated | unknown |"));
        assert!(!undated.to_markdown().contains("1970-01-01"));
    }

    /// The refactor onto the shared encoder must not move a single committed
    /// fingerprint: `docs/doctor/` holds 19 artifacts and a README index built on
    /// `a131e1f4b46d6c83` and `01b257ece8c48470`, and §16.13 forbids editing them.
    #[test]
    fn the_shared_encoder_is_byte_stable() {
        assert_eq!(
            probe_set_fingerprint(&[probe("P4", "t", "q")]),
            "ea5afd6873507ab9",
            "probe_set digest changed — every value in docs/doctor/ and its README index is now wrong"
        );
    }

    /// The whole point of the second digest is a discrimination, so both halves
    /// are asserted: it moves when the instrument gains a cell, and holds still
    /// when a measurement moves. The nested case is the 2026-08-05 defect
    /// verbatim — P10's `recheck` block arrived *under* an existing top-level key,
    /// so a top-level-only digest would not have seen it either.
    #[test]
    fn the_field_set_moves_on_a_new_observation_key_and_not_on_a_measurement() {
        let dir = |recheck: bool, depth: i64| {
            let mut o = serde_json::json!({
                "total_bytes_accepted": depth,
                "pending_output_bytes": depth,
            });
            if recheck {
                o["recheck"] = serde_json::json!({ "topped_up_bytes": 512 });
            }
            o
        };
        let p10 = |recheck: bool, depth: i64| {
            vec![
                Probe::new("P10", "pty buffer depth", "How deep …?")
                    .observe("slave_to_master_targetward", dir(recheck, depth)),
            ]
        };

        // (1) A NESTED key gained — the defect.
        assert_ne!(
            field_set_fingerprint(&p10(false, 1024)),
            field_set_fingerprint(&p10(true, 1024)),
            "P10 gained a nested `recheck` block and the field set did not move — \
             this is the 2026-08-05 defect verbatim"
        );

        // (2) A top-level observation gained — the `df48bfc` shape, where P4 grew
        //     `canonical`/`topology_only`/`unidentified`/`sysfs_tty_listing` under
        //     an unchanged question.
        assert_ne!(
            field_set_fingerprint(&[probe("P13", "t", "q")]),
            field_set_fingerprint(&[
                Probe::new("P13", "t", "q").observe("baseline_packet_bytes", 1)
            ]),
            "a new top-level observation did not move the field set"
        );

        // (3) Measurements must NOT move it — every scalar differs here, nested
        //     ones included, and two healthy boxes differ in all of them.
        assert_eq!(
            field_set_fingerprint(&p10(true, 1024)),
            field_set_fingerprint(&p10(true, 15360)),
            "a measurement moved the field set — every healthy pair of boxes \
             would now report itself incomparable"
        );

        // (4) Kind is excluded, and this case is measured, not hypothetical:
        //     P10.pending_output_bytes reads a number where TIOCOUTQ answers and
        //     null where it does not, on one binary across two kernels.
        let with = |v: serde_json::Value| {
            vec![Probe::new("P10", "t", "q").observe("pending_output_bytes", v)]
        };
        assert_eq!(
            field_set_fingerprint(&with(serde_json::json!(4096))),
            field_set_fingerprint(&with(serde_json::Value::Null)),
            "null-vs-number moved the field set — a kind-sensitive digest calls \
             two healthy runs of one binary incomparable"
        );
        // Same rule for an array that happened to be empty (P7.leading_bytes_hex).
        assert_eq!(
            field_set_fingerprint(&with(serde_json::json!([]))),
            field_set_fingerprint(&with(serde_json::json!(["0d", "0a"]))),
            "an empty array moved the field set"
        );

        // (5) Verdict, consequence, title and report order are not cells.
        let quiet = vec![Probe::new("P6", "t", "q").observe("pollin_passes", 4)];
        let busy = vec![
            Probe::new("P6", "OTHER TITLE", "q")
                .observe("pollin_passes", 9001)
                .verdict(Status::Degraded, "different kernel"),
        ];
        assert_eq!(field_set_fingerprint(&quiet), field_set_fingerprint(&busy));

        // (6) 16 hex, deterministic, and path boundaries are delimited. The
        //     boundary pair is a genuine one: `["P1.aP2.b"]` and `["P1.a",
        //     "P2.b"]` concatenate to the same bytes, so an encoder that dropped
        //     the length prefix would digest two different cell sets alike. It
        //     builds its probes directly because the `probe(…)` helper above
        //     makes observation-free probes, whose field set is empty for every
        //     input.
        let f = field_set_fingerprint(&p10(true, 1024));
        assert_eq!(f, field_set_fingerprint(&p10(true, 1024)));
        assert_eq!(f.len(), 16);
        assert!(f.chars().all(|c| c.is_ascii_hexdigit()), "{f}");
        assert_ne!(
            field_set_fingerprint(&[Probe::new("P1", "t", "q").observe("aP2.b", 1)]),
            field_set_fingerprint(&[
                Probe::new("P1", "t", "q").observe("a", 1),
                Probe::new("P2", "t", "q2").observe("b", 1),
            ]),
            "a path boundary shift went undetected"
        );
    }

    /// The emitting path and the recompute path must never drift — the README's
    /// index of the frozen artifacts is computed by the second and compared
    /// against the first.
    #[test]
    fn the_two_field_set_paths_agree_on_a_real_report() {
        let probes = vec![
            Probe::new("P7", "t", "q").observe(
                "a_open_close",
                serde_json::json!({"leading_bytes_hex": [], "read_outcomes": {"EIO": 2}}),
            ),
            Probe::new("P8", "t", "q").observe(
                "slave_open_idle",
                serde_json::json!({"epoll_flags_seen": {}}),
            ),
        ];
        let report = Report::new(Some(0), Vec::new(), probes.clone());
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json()).expect("report parses");
        assert_eq!(
            Some(report.build.field_set.clone()),
            field_set_of_report_json(&json),
            "the emitted digest and the recomputed digest disagree — the README \
             index of docs/doctor/ is computed by the recompute path"
        );
    }
}
