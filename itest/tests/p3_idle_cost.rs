#![forbid(unsafe_code)]

//! Phase 3's **idle-cost exit criterion**, made executable again (plan §Phase 3 item
//! 6; design §15.18/§15.19).
//!
//! The phase-3 benchmark had two axes and `docs/benchmarks/phase3.json` still records
//! both: throughput (183 MiB/s against a 30 MiB/s headroom target) and idle cost
//! (**total daemon CPU with 32 idle tty fds**, under a stated budget). The §16.11
//! migration folded the shell benchmarks into Rust — but only the throughput half
//! travelled. The idle half lost its producer with the script, so the recorded
//! `idle_cost` block became a number nothing regenerates and nothing checks (review 37,
//! 37-TEST-3), while the mechanism underneath it — the `ACTIVE_POLL`→`IDLE_POLL`
//! adaptive backoff that §15.19 measured at ~0.06 % of a core per polled fd — went back
//! to being upheld by review.
//!
//! That mechanism is exactly the kind that regresses silently: deleting the backoff
//! leaves every functional test green (readiness is still correct, just re-checked
//! 25× more often) and shows up only as an idle daemon eating a core on someone's
//! laptop, or as renewed flakiness in unrelated timing tests. Its sibling guard for
//! the sim doubles (`p12_sim_idle_cpu`) exists for the same reason and reads its CPU
//! the same way.
//!
//! **The fd count and both ceilings are derived from `docs/benchmarks/phase3.json`**,
//! not restated here. That artifact is the recorded exit criterion; a guard that copied
//! the numbers would let the two drift, which is how the axis came to have no
//! executable meaning in the first place. Two ceilings, because the recorded 20 %
//! *budget* is a design-decision trigger rather than a regression detector — a wholly
//! deleted `back_off` measures about 5 % here and would sail through it. The second is
//! a multiple of the recorded 2 %; its derivation and the measurements behind it are in
//! `DRIFT_FACTOR`.
//!
//! What this establishes, and what it does not: it measures the shipped daemon's idle
//! cost at the recorded fd count, so it catches a deleted or weakened backoff (proved
//! fail-first by neutering `back_off`). It cannot attribute a *passing* figure to that
//! specific mechanism — any other change that made an idle poll cheap would also pass —
//! and a ceiling is not a proof of sleeping. Linux-only: per-process CPU sampling needs
//! `/proc/<pid>/stat`, and there is no portable analogue (the same skip
//! `p9_pty_collapse` and `p12_sim_idle_cpu` take).

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use serde_json::Value;
    use serial_nexus_itest::{Daemon, cpu_ticks, wait_until};

    /// The measurement window. Ticks are USER_HZ (100/s on Linux), so a whole core is
    /// 1000 of them here and the recorded 2 % is 20 — enough resolution that the
    /// healthy reading reproduces to within one tick run to run (see [`DRIFT_FACTOR`]),
    /// which a shorter window did not give.
    const WINDOW: Duration = Duration::from_secs(10);

    /// The drift tripwire, as a multiple of the artifact's own recorded figure.
    ///
    /// Two ceilings, because the recorded **budget** and the regression this axis
    /// protects are two different sizes. The 20 % budget is a design-decision trigger —
    /// plan §Phase 3 item 6 spends it on selecting a §15.18 escape hatch — and it is far
    /// too loose to notice a lost backoff: with `back_off` neutered the daemon re-polls
    /// every idle fd at `ACTIVE_POLL` and lands around 5 %, comfortably inside 20 %.
    ///
    /// Measured before choosing the number (AGENTS §8), on a quiet 8-core box (load
    /// 0.2–0.8), 10 s window, 32 fds — four healthy runs against three with `back_off`
    /// neutered in place:
    ///
    /// * healthy: 20, 21, 21, 21 ticks (2.00–2.10 %) — the artifact's 2 %, reproduced;
    /// * `back_off` deleted: 46, 49, 51 ticks (4.60–5.10 %).
    ///
    /// 1.75× the recorded figure is 3.5 %: 1.67× above the worst healthy reading and
    /// below every regression reading. Deliberately not tighter — the failure it exists
    /// for is 2.3× away, not two ticks, and a gate that flakes gets deleted rather than
    /// fixed.
    const DRIFT_FACTOR: f64 = 1.75;

    /// The recorded phase-3 exit criterion.
    pub struct IdleCriterion {
        pub fds: usize,
        pub budget_percent: f64,
        pub recorded_percent: f64,
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
            budget_percent: num("budget_percent"),
            recorded_percent: num("total_cpu_percent"),
        };
        // The artifact must be internally consistent, or the budget below is being
        // read from a record that already fails its own criterion.
        assert!(
            c.fds > 0 && c.recorded_percent <= c.budget_percent,
            "docs/benchmarks/phase3.json records {} % at {} idle fds against a {} % \
             budget — the recorded run does not meet the recorded criterion",
            c.recorded_percent,
            c.fds,
            c.budget_percent
        );
        c
    }

    /// A graph of `n` idle `pty` nodes — `n` tty masters the daemon polls with nothing
    /// ever arriving on them, which is precisely the phase-3 idle shape. No edges: a
    /// target-facing peer would be a serial device, and the axis is about the polling
    /// cost of the fds themselves.
    fn cfg(run: &Path, n: usize) -> String {
        let mut s = String::new();
        for i in 0..n {
            s.push_str(&format!(
                "[[node]]\ntype = \"pty\"\nname = \"con{i}\"\npath = \"{}\"\n",
                run.join(format!("con{i}")).display()
            ));
        }
        s
    }

    pub fn run(c: IdleCriterion) {
        let d = Daemon::start();
        let rpc = d.rpc();
        rpc.load_toml(&cfg(d.run().path(), c.fds), false)
            .expect("load the idle pty graph");

        // Every fd must actually be open before the window starts: a node still
        // faulting or still coming up is an fd nobody is polling, and the measurement
        // would report the backoff working on a graph that is not there.
        for i in 0..c.fds {
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

        let pid = d.pid();
        let before = cpu_ticks(pid);
        // A *measurement window*, not a readiness wait: the claim under test is how
        // little CPU passes while nothing happens, so the window has to elapse.
        std::thread::sleep(WINDOW);
        let spent = cpu_ticks(pid).saturating_sub(before);

        // USER_HZ is 100/s on Linux, so `spent / window_seconds` is percent of one core.
        let percent = spent as f64 / WINDOW.as_secs_f64();
        eprintln!(
            "p3_idle_cost: {spent} ticks over {WINDOW:?} at {} idle tty fds = {percent:.2} % \
             of a core (budget {} %, recorded {} %)",
            c.fds, c.budget_percent, c.recorded_percent
        );
        // (1) The recorded exit criterion itself.
        assert!(
            percent <= c.budget_percent,
            "the daemon burned {percent:.2} % of a core over {WINDOW:?} with {} idle \
             tty fds, above the {} % budget recorded in docs/benchmarks/phase3.json \
             (that run measured {} %). Per plan §Phase 3 item 6, exceeding this budget \
             selects a §15.18 escape hatch — adaptive backoff or `spawn_blocking` \
             readers — as a phase task, never a return to epoll (invariant 1).",
            c.fds,
            c.budget_percent,
            c.recorded_percent
        );

        // (2) The tighter drift tripwire, which is what actually notices a weakened
        //     backoff — the budget above is a decision trigger and would not.
        let drift = c.recorded_percent * DRIFT_FACTOR;
        assert!(
            percent <= drift,
            "the daemon burned {percent:.2} % of a core over {WINDOW:?} with {} idle \
             tty fds — inside the {} % budget, but past {drift:.2} % ({DRIFT_FACTOR}× \
             the {} % docs/benchmarks/phase3.json recorded). The ACTIVE_POLL->IDLE_POLL \
             backoff is what holds this number down (§15.18/§15.19, ~0.06 % per polled \
             fd): check `back_off` and the boundary waits that reset and grow their \
             readiness interval. If the cost is genuinely higher now and that is \
             intended, re-measure and update the artifact — it is the record this \
             ceiling is derived from.",
            c.fds,
            c.budget_percent,
            c.recorded_percent
        );

        // Non-vacuity: a daemon that had died, or whose nodes had faulted out from
        // under the window, would score beautifully. The fds must still be there.
        for i in 0..c.fds {
            let name = format!("con{i}");
            assert_eq!(
                rpc.node_status(&name),
                "active",
                "{name} did not survive the idle window — the budget above was \
                 measured against fewer than {} fds",
                c.fds
            );
        }
    }
}

#[test]
fn thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget() {
    #[cfg(target_os = "linux")]
    linux_impl::run(linux_impl::criterion());
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "SKIP thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget: per-process \
         CPU sampling needs /proc/<pid>/stat (Linux)"
    );
}
