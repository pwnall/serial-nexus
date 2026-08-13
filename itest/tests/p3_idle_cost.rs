#![forbid(unsafe_code)]

//! Phase 3's **idle-cost exit criterion**, made executable again (plan §Phase 3 item
//! 6; design §15.18/§15.19) — and, since plan §18 item 46, read as a *marginal*
//! per-fd cost rather than an absolute total.
//!
//! The phase-3 benchmark had two axes and `docs/benchmarks/phase3.json` still records
//! both: throughput (183 MiB/s against a 30 MiB/s headroom target) and idle cost
//! (daemon CPU with idle tty fds, under a stated budget). The §16.11 migration folded
//! the shell benchmarks into Rust — but only the throughput half travelled. The idle
//! half lost its producer with the script, so the recorded `idle_cost` block became a
//! number nothing regenerates and nothing checks (review 37, 37-TEST-3), while the
//! mechanism underneath it — the `ACTIVE_POLL`→`IDLE_POLL` adaptive backoff that
//! §15.19 measured at ~0.06 % of a core per polled fd — went back to being upheld by
//! review.
//!
//! That mechanism is exactly the kind that regresses silently: deleting the backoff
//! leaves every functional test green (readiness is still correct, just re-checked
//! 25× more often) and shows up only as an idle daemon eating a core on someone's
//! laptop, or as renewed flakiness in unrelated timing tests. Its sibling guard for
//! the sim doubles (`p12_sim_idle_cpu`) exists for the same reason and reads its CPU
//! the same way.
//!
//! # Two rungs in one process, because the fixed cost is not the mechanism's
//!
//! An idle daemon's CPU is a fixed term plus a per-fd term, and only the second is
//! the one the backoff controls. `total_cpu_percent` in the artifact is a single
//! total at 32 fds with no baseline rung, so it cannot be decomposed — and a ceiling
//! derived from it is dominated by a term the mechanism under test does not touch.
//! Measured (the sweep is tabulated at [`DRIFT_FACTOR`]): the fixed term on a 20-core
//! box is **1.194 %** of a core against a **0.0728 %/fd** slope, so at 32 fds the
//! fixed half is a third of the total and the old ceiling — 1.75× the artifact's
//! 2.00 %, i.e. 3.50 % — sat barely above that box's healthy 3.3–3.6 % reading. It
//! fired, repeatedly, with the backoff measurably intact (plan §18 item 46). The
//! eleven-run validation sweep at [`DRIFT_FACTOR`], on that box at loads from 1.4 to
//! 12.7, read 3.30–4.30 % of a core in total: **nine of the eleven are past the old
//! 3.50 % ceiling on a tree whose `back_off` is the shipped one**, and all eleven are
//! inside the marginal ceiling that replaced it.
//!
//! So the guard measures **two rungs and takes their difference**: a baseline count of
//! idle fds, then the recorded count, with `(full − baseline) / (fds − baseline)` the
//! marginal cost of one idle fd. Both rungs run in **one daemon process**, grown from
//! the baseline with `add-node` (§11), which makes the fixed term identical by
//! construction rather than merely similar — two daemons would differ by whatever the
//! box did between the two starts, which is the whole quantity being cancelled.
//!
//! # What this changes about coverage, said rather than hidden
//!
//! A regression that inflates the daemon's **fixed** cost without scaling per fd — a
//! control loop that stopped sleeping, a timer that stopped coalescing — used to be
//! caught incidentally by the absolute ceiling and is now caught only by assertion
//! (1), the recorded 20 % budget, which is far looser. That is a real loss of
//! sensitivity, and it is the right trade: the absolute form could not tell that class
//! of regression from *a different box*, which is exactly how it came to redden on
//! this one over a 1.19 % fixed cost while the mechanism it names in its own failure
//! message was intact. A tripwire that fires for the machine it runs on teaches its
//! readers to raise it, and item 46 explicitly declines raising it. Recovering the
//! lost sensitivity honestly needs a recorded *fixed-cost* figure with an era of its
//! own — a `baseline_cpu_percent` beside the per-fd one, measured across more than one
//! box — which is a measurement nobody has taken, not a number to invent here.
//!
//! **The two fd counts, the budget and the per-fd figure are all derived from
//! `docs/benchmarks/phase3.json`**, not restated here. That artifact is the recorded
//! exit criterion; a guard that copied the numbers would let the two drift, which is
//! how the axis came to have no executable meaning in the first place. Two ceilings,
//! because the recorded 20 % *budget* is a design-decision trigger rather than a
//! regression detector — a wholly deleted `back_off` measures about 10 % at 32 fds
//! here and would sail through it.
//!
//! What this establishes, and what it does not: it measures the shipped daemon's
//! marginal cost per idle polled fd, so it catches a deleted or weakened backoff
//! (proved fail-first by neutering `back_off`). It cannot attribute a *passing* figure
//! to that specific mechanism — any other change that made an idle poll cheap would
//! also pass — and a ceiling is not a proof of sleeping.
//!
//! **Still Linux-only, and the reason changed from a capability to a calibration on
//! 2026-08-13** (plan §18 items 12 and 13). The old reason — "there is no portable
//! analogue" to `/proc/<pid>/stat` — was the sentence three guards were gated on, and
//! it is retired: `serial_nexus_sys::process_cpu_nanos` answers on both kernels, and
//! `p9_pty_collapse`'s guard now runs on both. What still gates *this* file is that
//! **its ceiling is a Linux measurement**. Run once on the x86_64 Darwin rig box with
//! the gate lifted: 3.72 % of a core at 8 idle tty fds and 9.91 % at 32, a marginal
//! **0.2578 %/fd** against the artifact's recorded 0.0728 %/fd — 3.5x, which is the
//! real `kevent` Darwin pays per pass and not a regression. Assertion (1), the 20 %
//! budget, **passes** there with room. Assertion (2) fails, because it multiplies a
//! figure measured on another kernel. Ungating without a per-platform
//! `per_fd_cpu_percent` in `docs/benchmarks/phase3.json` would assert a Linux ceiling
//! on Darwin — a fresh proxy in space pointing the other way, which is the mistake
//! item 12 exists to remove rather than relocate. The artifact needs a Darwin row
//! first; that is item 13's remainder.

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use serde_json::Value;
    use serial_nexus_itest::{Daemon, cpu_nanos, wait_until};

    /// The measurement window, per rung. Ticks are USER_HZ (100/s on Linux), so a
    /// whole core is 1000 of them here and one tick is 0.1 % — at the sweep's 24-fd
    /// span that is 0.004 %/fd of quantization against a 0.127 %/fd ceiling, which a
    /// shorter window did not give. Two rungs, so the test spends 2×`WINDOW` asleep.
    const WINDOW: Duration = Duration::from_secs(10);

    /// The drift tripwire, as a multiple of the artifact's recorded **per-fd** figure.
    ///
    /// Two ceilings, because the recorded **budget** and the regression this axis
    /// protects are two different sizes. The 20 % budget is a design-decision trigger —
    /// plan §Phase 3 item 6 spends it on selecting a §15.18 escape hatch — and it is far
    /// too loose to notice a lost backoff: with `back_off` neutered the daemon re-polls
    /// every idle fd at `ACTIVE_POLL` and lands around 10 % at 32 fds, inside 20 %.
    ///
    /// Measured before choosing the number (AGENTS §8), on a 20-core box (Linux
    /// 7.0.0-29-generic, tree at `e135b53`, 2026-08-12, load between 1.0 and 2.5 at
    /// every sample), 10 s window — three samples per rung healthy, two per rung with
    /// `back_off`'s body replaced by `let _ = wait;` and the workspace rebuilt:
    ///
    /// | idle fds | healthy, % of a core | `back_off` neutered |
    /// | --- | --- | --- |
    /// | 1  | 1.30 1.10 1.20 | — |
    /// | 2  | 1.40 1.20 1.40 | — |
    /// | 8  | 1.90 1.70 1.80 | 4.80 5.20 |
    /// | 16 | 2.30 2.40 2.70 | 7.10 6.50 |
    /// | 32 | 3.60 3.30 3.50 | 9.80 9.90 |
    ///
    /// Least squares over the fifteen healthy points: **0.0728 %/fd** with a **1.194 %**
    /// intercept. Over the six neutered points: **0.2004 %/fd** with a 3.476 %
    /// intercept. The slopes separate by **2.75×** — and the healthy one reproduces
    /// §15.19's recorded ~0.06 %/fd within 20 %, which is the finding item 46 asked
    /// for: the backoff has *not* regressed, so the first disposition this test's own
    /// failure message names is refuted by measurement. What had moved is the fixed
    /// term, which the artifact never recorded separately.
    ///
    /// 1.75 × the recorded 0.0728 %/fd is **0.127 %/fd**: by construction 1.75× above
    /// the healthy slope, and 1.57× below the neutered one (0.2004 / 0.127 = 1.57). The
    /// two-rung difference this guard actually computes reads a little under the fit —
    /// (3.467 − 1.800) / 24 = 0.069 %/fd healthy, (9.850 − 5.000) / 24 = 0.202 %/fd
    /// neutered — so the ceiling sits 1.84× above the healthy reading and 1.59× below
    /// the regression it exists for.
    ///
    /// The factor is unchanged at 1.75 from the derivation it replaces (an 8-core box's
    /// *absolute* readings: 2.00–2.10 % healthy against 4.60–5.10 % neutered, a 2.4×
    /// separation) — what changed is the figure it multiplies, not the multiplier.
    /// Deliberately not tighter: the two separations agree within 15 % across two
    /// unrelated boxes, the failure this exists for is 2.75× away rather than two
    /// ticks, and a gate that flakes gets deleted rather than fixed — which is the
    /// report item 46 opened with.
    ///
    /// Validated by running *this* guard rather than by arithmetic alone — a sweep of
    /// eleven healthy runs on the same box at loads from 1.4 to 12.7, reading 0.0542
    /// 0.0625 0.0667 0.0750 0.0792 0.0792 0.0792 0.0833 0.0833 0.0875 0.0958 %/fd (mean
    /// 0.077, and the *difference* the guard takes has a standard deviation of 2.8
    /// ticks, so the ceiling sits 4.3σ out), against three neutered runs reading 0.1542
    /// 0.2375 0.2500 %/fd. Both arms are one-sided in the same direction, which is why
    /// the ceiling sits nearer the healthy arm than the midpoint: 1.33× above the worst
    /// healthy reading, 1.21× below the best neutered one. The noisier arm is the
    /// neutered one, because
    /// a `back_off` that never backs off is *scheduler-limited* — under load it burns
    /// less, not more — so a busy box compresses the separation from above while
    /// leaving the healthy arm where it was.
    const DRIFT_FACTOR: f64 = 1.75;

    /// The recorded phase-3 exit criterion.
    pub struct IdleCriterion {
        /// The rung the recorded figures are stated at (`idle_tty_fds`).
        pub fds: usize,
        /// The rung the marginal cost is measured *from* (`baseline_tty_fds`).
        pub baseline_fds: usize,
        pub budget_percent: f64,
        /// The recorded marginal cost of one idle tty fd, in percent of one core.
        pub recorded_per_fd_percent: f64,
        /// The original benchmark's single-rung total at `fds`, kept as the record it
        /// is. Nothing derives a ceiling from it any more (see the module doc); it is
        /// still printed and still checked against the budget for internal
        /// consistency.
        pub historical_total_percent: f64,
    }

    /// Read the `idle_cost` axis from `docs/benchmarks/phase3.json`.
    ///
    /// Panics rather than defaults if the artifact is missing or reshaped: a guard that
    /// silently substituted its own numbers for the recorded criterion would be
    /// measuring something nobody wrote down, which is the state this test exists to
    /// end.
    pub fn criterion() -> IdleCriterion {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("itest/ has a parent")
            .join("docs/benchmarks/phase3.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let v: Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        let idle = &v["idle_cost"];
        let num = |key: &str| -> f64 {
            idle[key].as_f64().unwrap_or_else(|| {
                panic!("{} has no numeric idle_cost.{key}: {idle}", path.display())
            })
        };
        let c = IdleCriterion {
            fds: num("idle_tty_fds") as usize,
            baseline_fds: num("baseline_tty_fds") as usize,
            budget_percent: num("budget_percent"),
            recorded_per_fd_percent: num("per_fd_cpu_percent"),
            historical_total_percent: num("total_cpu_percent"),
        };
        // The artifact must be internally consistent, or the budget below is being
        // read from a record that already fails its own criterion.
        assert!(
            c.fds > 0 && c.historical_total_percent <= c.budget_percent,
            "docs/benchmarks/phase3.json records {} % at {} idle fds against a {} % \
             budget — the recorded run does not meet the recorded criterion",
            c.historical_total_percent,
            c.fds,
            c.budget_percent
        );
        // And the two rungs must actually be two rungs, in that order, with a positive
        // recorded slope between them: a baseline at or above the recorded count makes
        // the marginal cost a division by zero or a sign flip, and a zero recorded
        // per-fd figure makes the drift ceiling zero — which no daemon can meet, so
        // the guard would fail for the artifact's reasons rather than the tree's.
        assert!(
            c.baseline_fds > 0 && c.baseline_fds < c.fds && c.recorded_per_fd_percent > 0.0,
            "docs/benchmarks/phase3.json records a baseline of {} fds against {} \
             recorded fds at {} %/fd — the marginal cost needs 0 < baseline < recorded \
             and a positive per-fd figure",
            c.baseline_fds,
            c.fds,
            c.recorded_per_fd_percent
        );
        c
    }

    /// One `[[node]]` block for idle pty number `i` — a tty master the daemon polls
    /// with nothing ever arriving on it, which is precisely the phase-3 idle shape. No
    /// edges: a target-facing peer would be a serial device, and the axis is about the
    /// polling cost of the fds themselves.
    fn node_toml(run: &Path, i: usize) -> String {
        format!(
            "[[node]]\ntype = \"pty\"\nname = \"con{i}\"\npath = \"{}\"\n",
            run.join(format!("con{i}")).display()
        )
    }

    /// The baseline rung as one `load` config.
    fn cfg(run: &Path, n: usize) -> String {
        (0..n).map(|i| node_toml(run, i)).collect()
    }

    pub fn run(c: IdleCriterion) {
        let d = Daemon::start();
        let rpc = d.rpc();
        rpc.load_toml(&cfg(d.run().path(), c.baseline_fds), false)
            .expect("load the baseline idle pty graph");
        settled(&d, c.baseline_fds);

        let pid = d.pid();
        let base_ns = sample(pid);

        // Grow the *same* process to the recorded rung, one `add-node` at a time (§11).
        // This is the point of the whole shape: the fixed term measured in `base_ns`
        // is this daemon's, on this box, at this instant, so subtracting it leaves the
        // per-fd term and nothing else. `load` cannot do this — it is refused on a
        // non-empty graph unless `replace` is set, and `replace` composes a teardown,
        // which would put the baseline fds back on the wrong side of the measurement.
        for i in c.baseline_fds..c.fds {
            rpc.add_node_toml(&node_toml(d.run().path(), i))
                .unwrap_or_else(|e| panic!("add-node con{i}: {e:?}"));
        }
        settled(&d, c.fds);

        let full_ns = sample(pid);

        // Percent of one core: nanoseconds of CPU over nanoseconds of wall clock,
        // times 100 — i.e. `ns / (seconds * 1e7)`. The `ticks / window_seconds` form
        // this replaces relied on USER_HZ being exactly 100/s, which made the unit
        // conversion invisible and Linux-only (plan §18 item 12).
        let pct = |ns: u64| ns as f64 / (WINDOW.as_secs_f64() * 1e7);
        let (base_percent, full_percent) = (pct(base_ns), pct(full_ns));
        // The marginal cost of one idle fd, with the box's fixed cost differenced out.
        // Clamped at zero rather than signed: a `full` below `base` is a quiet box
        // drifting by a tick or two, and a negative slope has no meaning to report.
        let marginal = (full_percent - base_percent).max(0.0) / (c.fds - c.baseline_fds) as f64;
        eprintln!(
            "p3_idle_cost: {base_ns} ns at {} idle tty fds = {base_percent:.2} %, \
             {full_ns} ns at {} = {full_percent:.2} % (each over {WINDOW:?}, one \
             daemon) -> {marginal:.4} %/fd marginal (recorded {} %/fd, ceiling \
             {:.4} %/fd; budget {} % of a core)",
            c.baseline_fds,
            c.fds,
            c.recorded_per_fd_percent,
            c.recorded_per_fd_percent * DRIFT_FACTOR,
            c.budget_percent
        );

        // (1) The recorded exit criterion itself.
        assert!(
            full_percent <= c.budget_percent,
            "the daemon burned {full_percent:.2} % of a core over {WINDOW:?} with {} idle \
             tty fds, above the {} % budget recorded in docs/benchmarks/phase3.json \
             (that run measured {} %). Per plan §Phase 3 item 6, exceeding this budget \
             selects a §15.18 escape hatch — adaptive backoff or `spawn_blocking` \
             readers — as a phase task, never a return to epoll (invariant 1).",
            c.fds,
            c.budget_percent,
            c.historical_total_percent
        );

        // (2) The drift tripwire, which is what actually notices a weakened backoff —
        //     the budget above is a decision trigger and would not. It reads the
        //     *marginal* cost, because that is the quantity §15.19 promises and the
        //     only one the backoff controls; the box's fixed cost is differenced out
        //     above and is deliberately not asserted (module doc, coverage).
        let drift = c.recorded_per_fd_percent * DRIFT_FACTOR;
        assert!(
            marginal <= drift,
            "one idle tty fd cost {marginal:.4} % of a core here — measured as \
             ({full_percent:.2} % at {} fds − {base_percent:.2} % at {} fds) / {}, in \
             one daemon process so the fixed cost cancels — past the {drift:.4} %/fd \
             ceiling ({DRIFT_FACTOR}× the {} %/fd docs/benchmarks/phase3.json records). \
             The ACTIVE_POLL->IDLE_POLL backoff is what holds this number down \
             (§15.18/§15.19, ~0.06 % per polled fd): check `back_off` and the boundary \
             waits that reset and grow their readiness interval. A neutered `back_off` \
             measures about 0.20 %/fd. If the cost is genuinely higher now and that is \
             intended, re-measure and update the artifact's per_fd_cpu_percent and its \
             provenance block — it is the record this ceiling is derived from, and it \
             carries its box and its date for exactly this conversation.",
            c.fds,
            c.baseline_fds,
            c.fds - c.baseline_fds,
            c.recorded_per_fd_percent
        );

        // Non-vacuity: a daemon that had died, or whose nodes had faulted out from
        // under either window, would score beautifully. The fds must still be there.
        for i in 0..c.fds {
            let name = format!("con{i}");
            assert_eq!(
                rpc.node_status(&name),
                "active",
                "{name} did not survive the idle windows — the readings above were \
                 measured against fewer than {} fds",
                c.fds
            );
        }
    }

    /// Sample the daemon's CPU across one [`WINDOW`], in nanoseconds.
    fn sample(pid: u32) -> u64 {
        let before = cpu_nanos(pid);
        // A *measurement window*, not a readiness wait: the claim under test is how
        // little CPU passes while nothing happens, so the window has to elapse.
        std::thread::sleep(WINDOW);
        cpu_nanos(pid).saturating_sub(before)
    }

    /// Every one of the first `n` fds must actually be open before a window starts: a
    /// node still faulting or still coming up is an fd nobody is polling, and the
    /// measurement would report the backoff working on a graph that is not there. Run
    /// before *both* rungs — the grown half needs it as much as the loaded half, and a
    /// baseline taken while the recorded rung's nodes were still arriving would carry
    /// their startup cost into the term that gets subtracted.
    fn settled(d: &Daemon, n: usize) {
        let rpc = d.rpc();
        for i in 0..n {
            let name = format!("con{i}");
            assert!(
                rpc.wait_status(&name, "active", Duration::from_secs(30)),
                "{name} never reached active: {:?}",
                rpc.node(&name)
            );
            let link = d.run().join(&name);
            assert!(
                wait_until(Duration::from_secs(5), || link.exists()),
                "{name}'s pty symlink never appeared at {}",
                link.display()
            );
        }
    }
}

/// The name is kept verbatim across item 46's rework, deliberately: it names assertion
/// (1), which is unchanged — thirty-two idle tty fds do still have to stay under the
/// recorded budget — and it is the identifier the plan's Status rows and AGENTS §2 quote
/// when they record this guard's readings. Renaming it would invalidate those records to
/// describe an assertion the name never covered in the first place (the drift tripwire
/// was always the second one).
#[test]
fn thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget() {
    #[cfg(target_os = "linux")]
    linux_impl::run(linux_impl::criterion());
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget: the \
         artifact's per-fd ceiling is a Linux measurement and this box is not Linux \
         (measured here at 0.2578 %/fd against a recorded 0.0728; see this file's \
         header and plan §18 item 13)"
    );
}
