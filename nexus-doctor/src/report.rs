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

/// The whole report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_unix_ms: Option<u64>,
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
        Report {
            tool: "nexus-doctor",
            version: env!("CARGO_PKG_VERSION"),
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

fn render_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => md_escape(s),
        other => other.to_string(),
    }
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}
