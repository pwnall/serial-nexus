# serial_nexus — Implementation Plan

**Status:** Executed. Phases 0–8 (nine phases) and every track through the rename track
(plan §17) are complete, audited, and green; all 82 review-37 findings are dispositioned
(`docs/38-review-37-remediation-ledger.md`); the validation suite is the Rust `serial-nexus-itest`
crate (§15.31); macOS is runtime-verified on real hardware. No implementation track is open:
**plan §18 is the work ledger, and this Status block is the authority on what is open.** Of the
fifteen items the ledger carried in, items 1, 2, 3, 5, 6, 7, 9, 10 and 11 are executed
(2026-08-05/07; notes §3.55, §3.56, §3.57, §3.63, §3.70, §3.73); items 4 (residual only), 8, and
12–15 remain open; items this generation files are appended from 16 — item numbers are
append-only and never reused, and plan §18 carries each item's state, evidence, and validation.
Of items 16–46: **23, 32, 33, 34, 35, 37, 39, 40, 43 and 44 were executed 2026-08-12** by the
alignment pass that ran the v16 pair against the *tree* (notes §3.75; 460 clauses checked,
nineteen deviations confirmed and repaired, two refuted), which also filed items 45 and 46; the
rest remain open. **The v17 revision (2026-08-12) filed items 47–55**: item 47 was the pattern
wait (§10, §15.56) — the one design-ahead-of-tree construction item — and 48–55 are the
residuals and hardenings its input digest surfaced. **Item 47 was executed later the same day**
(notes §3.83), so the design is no longer ahead of the tree at any surface; one of its filed
clauses deviated with the reason recorded (the CLI's exit-code numbering — see the item).
48–55 remain open. The same session ran the v17 pair against the *tree* and repaired six
confirmed deviations (notes §3.82) — four narrative sites still promising the pre-§15.55
one-capability bound, plus one over-stated plan rule and one design figure — and closed the
narrowness hole those sites pointed at.
The two-test whole-suite flake the Linux figure used to be quoted beside is closed (notes §3.70):
a re-enumerated FT232R eats the first 64 bytes — one USB bulk packet — of the first traffic that
crosses afterwards, every daemon-side counter reads 0, and it was never a product defect.

Current measured figures live in the table below and nowhere else in this pair; everything else
cites the table. The figures restate the v15 record exactly, with its scopes, dates, and commits
— none was re-measured or re-derived for this rewrite.

| Figure | Scope | Date | Commit / record | Caveat |
|---|---|---|---|---|
| **1004 passing · 0 failed · 7 ignored**, 126 test-result lines, **14 self-skips** | Linux, default CI scope, `--no-fail-fast`, `--nocapture`, `SNX_EXEC_CODEC=required`, `SNX_TLS=required` | 2026-08-14 | the 5-wire re-cable session, `b58a1c4b7fc8` + this session's doc edits (notes §3.102) | **the Linux default-scope authority row**, and the run that turns §3.101's skip-count instability from an inference into a **direct observation**. 126 result lines over 122 targets (114 `Running` + 8 doc-test). Name extraction yields **13**; the fourteenth, `crossover_rig_signal_verbs`, has **zero** `SKIP <name>` occurrences — and it is identifiable anyway, because the splice that destroyed it is legible in the log: `SKIP test crossover_rig_data_plane_send_and_exclusivity ... crossover_rig_signal_verbsok: no crossover rig (`, which is a `SKIP crossover_rig_signal_verbs: …` line and a `test crossover_rig_data_plane_… ok` line interleaved mid-write by two parallel binaries. **The two prior instances inferred the missing line from "this test must have skipped"; this one shows the interleave itself**, so the 14 the row above carried as a *union across two runs and a floor* is here a single run's actual figure. The instrument is unchanged and still must not be quoted precisely — a splice that landed one byte differently would have destroyed both names instead of one. Same 1004 · 0 · 7 as the rig row above, at the same code (docs-only delta), same session, same box. Supersedes the row below. |
| **1004 passing · 0 failed · 7 ignored**, 126 test-result lines | Linux, default CI scope, `--no-fail-fast`, `--nocapture`, `SNX_EXEC_CODEC=required`, `SNX_TLS=required` | 2026-08-14 | the macOS-tree validation session (notes §3.101) | **superseded by the row above**, which reproduced this figure at the same code and turned this row's inferred skip count into a directly observed one. Kept because it is the row that established the floor and decomposed the +5. Formerly the Linux authority row. 126 result lines over **122 cargo targets** (114 `Running` + 8 doc-test) and **at least 14 self-skips — a lower bound, deliberately, because the count is not stably obtainable and the "13" carried by every Linux row below is an undercount.** Two runs of *this same tree* read 13 by `grep -c '^SKIP'` and 13 and 14 by a name-extracting grep, and their **name sets differ**: run one is missing `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` and run two is missing `crossover_rig_custom_baud_byte_exact`, each absent from its own log *in any form* while both tests must self-skip at this scope (no `SNX_CROSSOVER_A`/`_B`). Their union is 14 and even that is a floor, since a line mangled in both runs is invisible to both. **This is the first Linux instance of the class AGENTS §3 had recorded only for macOS** — under `--nocapture` the suite's parallel binaries interleave writes and break the line anchor mid-message — so the rule "do not quote that skip count precisely" is not a macOS caveat but a property of the instrument (notes §3.101, extending §3.78). **Both ends were measured in this session, so the +5 over the 999 row is quotable rather than reconstructed**: the macOS session's tree read **1003** here before any change of this session's, and it decomposes exactly — four of its new guards compile and run on Linux (`peer_hungup`'s `sys` self-test and the software-arm `sys` test, item 66/67; the `xon-xoff` rig guard, which self-skips at this scope and so *passes*; and `process_cpu_nanos`'s self-test, item 12), while the fifth is this session's `a_held_pts_index_is_never_reallocated_to_a_new_pair` (item 72). **Two of that session's changes are deliberately invisible in this column**: `a_bare_hangup_leaves_the_daemon_cpu_bounded` and `cpu_nanos_reads_this_process_and_never_goes_backwards` were each a `cfg`-paired test before item 12 ungated them, so Linux counted one either way — read them in the skip set, not the total. Item 46's guard reads **0.0792 %/fd** marginal against its 0.1274 %/fd ceiling under full suite parallelism (2.30 % at 8 idle tty fds, 4.20 % at 32), within the band of the artifact's 0.0728 %/fd; `p9_pty_collapse`'s anti-spin guard reads 2.00 % of a core against its 10 % ceiling. Supersedes the 999 row. |
| **999 passing · 0 failed · 7 ignored**, 126 test-result lines | Linux, default CI scope, `--no-fail-fast`, `--nocapture`, `SNX_EXEC_CODEC=required`, `SNX_TLS=required` | 2026-08-13 | the alignment-pass session's second half (notes §3.87–§3.91) | **superseded by the 1004 row above.** 126 result lines over **122 cargo targets** (114 `Running` + 8 doc-test) and **13 self-skips**. The +32 over the 967 row is this half's work: P16 and P15's software reading, the packaging gate's six, the harness primitives' self-tests, the tap-ack pair (item 59a), the orphan sweep's own proof, and the parked-child exit guard. `SNX_TLS=required` joins the scope for the first time — it was set by no lane at all until item 60(a). **Orphan-clean, and that is now asserted rather than observed:** every `Daemon` sweeps its process group at drop and panics naming survivors, after a run of this suite left 3 processes behind per invocation and 260 on the box (item 65). Supersedes the 967 row. |
| **967 passing · 0 failed · 7 ignored**, 125 test-result lines | Linux, default CI scope, `--no-fail-fast`, `--nocapture`, **`SNX_EXEC_CODEC=required`** | 2026-08-12 | the v17 alignment-pass session (notes §3.84–§3.86) | **the Linux authority row, and the first fully green one in this record.** 125 result lines over **121 cargo targets** (113 `Running` + 8 doc-test) and **12 self-skips**, measured with `--nocapture`. The **+36** over the 931 row is measured per target rather than estimated, and closes exactly: `serial_nexus_devprep` +6 (item 52), `serial_nexus_rpc` +5 (item 51's hoisted policy, self-tested per §16.5), `serial_nexus_daemon` +4 and `serial_nexus_codec_api` +4 (items 56, 21, 53), `meta_skip_names` +3, `p5_codec_teardown` +3 (item 38) and `p8_daemon_transcript` +3 (item 36) as new files, `serial_nexus_sys` +2 (item 56's tri-state), `p13_teardown_accounting` +2 (item 21), and one each from `meta_gates` (item 52's derived verb-parity gate), `p13_legacy_defaults`, `serial_nexus_doctor` and **`p3_idle_cost`** — the last being item 46's guard passing rather than a new test. `ignored` moved 6 → 7 for a documentation reason: the kit's new suite carries an ```ignore` doc example like its siblings. **The one failure is gone**: `p3_idle_cost` reads **0.0750 %/fd** against its 0.1274 %/fd ceiling *under full suite parallelism*, within 3 % of the 0.0728 %/fd the artifact records from solo runs — which is the evidence that the marginal form is the right instrument, the absolute form having varied by more than that between one box and another. Supersedes the 931 row. |
| **931 passing · 1 failed · 6 ignored**, 122 test-result lines | Linux, default CI scope, `--no-fail-fast`, `--nocapture` | 2026-08-12 | the item-47 landing (notes §3.83) | **the Linux authority row**, superseding the 894 below. 122 result lines over **118 cargo targets** (110 `Running` + 8 doc-test) and **12 self-skips**, measured with `--nocapture` (notes §3.78). The +37 over the 894 row is exactly this session's new tests, counted rather than estimated: **22** matcher units (`daemon/src/pattern.rs`, a new file), **12** pattern-wait acceptance guards (`itest/tests/p12_pattern_wait.rs`, a new file), **2** hub guards added to `daemon/src/tap.rs` (11 → 13) and **1** devprep capability-fold guard (1 → 2). Both ends were measured in this session, so the delta is quotable. **An earlier row here read 925 and decomposed it as 19+11+1**; that figure was taken before the adversarial review's fixes added six guards, and its decomposition was wrong by one even for its own tree — recorded because a Status row whose arithmetic does not close is exactly what this table's scope discipline exists to prevent. The one failure is `p3_idle_cost::thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget` at **4.10 %** against its 3.50 % tripwire: item 46, and **measured not to be this change's** — runs on this tree read 3.70/3.70/3.90/4.10 %, and three runs on the unchanged tree (a `git worktree` at `849fc8e` with its own target dir, same box, same session) read 3.80/4.30/3.90 %, so the two trees sit in one band and the pattern wait adds no idle-path work (it runs inside `TapHub::ingest`, which an idle endpoint never reaches). |
| **894 passing · 1 failed · 6 ignored**, 121 test-result lines | Linux, default CI scope, `--no-fail-fast` | 2026-08-12 | the v17 landing's gate run (notes, the v17 generation entry) | the one failure is `p3_idle_cost::thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget` at **3.80 %** against its 3.50 % tripwire — item 46's recorded signature verbatim (its message prints "38 ticks over 10s"), reproduced in two of this landing's three suite runs (once under deliberate parallel load, once on a quiet box at load 0.33) and absent in the first; the product tree is unchanged by v17 (documents, two meta-gate consts, one harness message string), so this is item 46 resurfacing, never a fresh finding. Supersedes the 890 row as the default-scope authority; the total moved 896 → 901 because the 2026-08-12 rig session's later commits added tests after that row was taken. **Superseded by the 925 row above**, taken later the same day at the same scope, which is the current Linux authority. |
| **890 passing · 0 failed · 6 ignored**, of which **886 · 0 · 6** are workspace-own | Linux, default CI scope | 2026-08-12 | this session (notes §3.75) | 121 test-result lines over **117 cargo targets** (109 `Running` + 8 doc-test), and **12 self-skips**, measured with `--nocapture` (notes §3.78 — at default capture the count reads 0 because `cargo test` captures a *passing* test's stderr, so "zero SKIP lines" beside a figure taken without it asserts nothing); four passes and two lines are the nested `acme-codec` **and `tinymux-codec`** subprocesses (`p8_external_codec.rs`, which now builds and tests both template crates); rig attached but `SNX_CROSSOVER_A`/`_B` unexported, so `serial_hardware` self-skipped. The `ignored` moved 4 → 6 for a documentation reason, not a coverage one: the two new kit suites carry ```ignore` doc examples like their four siblings. Superseded by the v17 landing row above. |
| **852 passing · 0 failed · 4 ignored**, of which **850 · 0 · 4** are workspace-own | Linux, default CI scope | 2026-08-07 | v15 Status re-measure; no sha recorded | 116 test-result lines over 114 cargo targets; its "zero SKIP lines" is **withdrawn** — it was taken at default capture, where the count is 0 whatever skipped (notes §3.78). Two passes and two lines are the nested `acme-codec` subprocess (`p8_external_codec.rs`). Superseded by the row above; kept because the delta between them was measured in one session at one scope. |
| **1004 passing · 0 failed · 7 ignored**, three self-skips | Linux, **the documented rig lane minus `SNX_WEB_UI` only** (`SNX_CROSSOVER`, `SNX_REPLUG`, `SNX_TLS`, **`SNX_RIG_FLOW`** and `SNX_EXEC_CODEC` all `required`) | 2026-08-14 | the 5-wire re-cable session, `b58a1c4b7fc8` (notes §3.102) | **the rig-lane authority row, and the first row in this table whose scope column names `SNX_RIG_FLOW=required`** — every `required` word the lane defines except the one this box cannot answer (`SNX_WEB_UI`, no `node`), so it is *not* the fully-spelled lane and must not be quoted as one. **Nor is it establishably the first ever to set that word:** `ebf9c52` introduced `SNX_RIG_FLOW` on 2026-08-05 and the 835 row's `17c6e87` is a descendant of it on a bench that was 5-wire that day, so that lane may have carried it — but that row's attribution is already recorded below as unreconciled with its own session record, and nothing in the notes settles it. Every row between then and here explicitly names the variable as dropped. The operator re-cabled the bench from 3-wire to **5-wire** and P5 confirms it (`rts_a_to_cts_b=true rts_b_to_cts_a=true`, six DTR crossings `false`, 3 of 3, committed), so `SNX_RIG_FLOW=required` is licensed for the first time since 2026-08-12. **The distinguishing reading is the self-skip set, 5 → 3, and the two names that left it are exactly the two `skip_no_rig_flow` callers** — `crossover_rig_rts_crosses_to_the_far_ports_cts` and `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` — with no other name moving, which is what attributes the delta to the cable and not to anything else in the session. The three remaining are named measurements: the packaging root arm (no root) and two browser tests (no `node`). Skips counted by **name**, unanchored, per notes §3.101. Every hardware test ran and passed: four replug tests including `identity_survives_a_replug_that_renumbers_the_tty`, five crossover tests, `web_tls_round_trip`, and `xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it`; the blessed `grant` verb fired 7× across the re-enumerations. **The adapter pair is new to the record** (`BH00L4KU` ↔ `BH00LW9U`) — see §15.52's re-cable annotation for the confound that carries. **This session measured both scopes, so it is a second dual-scope observation — with one weakening stated rather than glossed:** the rig lane ran at `b58a1c4b7fc8` clean and the default-scope run at that tree *plus this session's documentation edits*, no code changed between them. Item 30's own standard is "same tree, same session, same box", and a documentation delta is not the same tree, so the 2026-08-13 measurement remains the stricter one and this is a corroboration rather than a replacement. Both figures are in this table with their scopes named and **no delta is derived across them** (rule 19). `scripts/bless` reported *Stale* from an ordinary relink and the lane was run anyway — `grant` and all four replug tests passing is the evidence that the warning was the warning it claims to be. **Both flow guards were fail-first proven against this bench, not merely run:** suppressing the RTS drive reddens both (and reads `cts: true` at *both* polarities — the stuck-high shape a one-polarity test would have passed), and dropping arm 1's transmitter to `flow_control = "none"` reddens the stall with its own message, 40 bytes crossed against 0. Supersedes the 1004 row below. |
| **1004 passing · 0 failed · 7 ignored**, five self-skips | Linux, **the documented rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** (`SNX_CROSSOVER`, `SNX_REPLUG`, `SNX_TLS` and `SNX_EXEC_CODEC` all `required`) | 2026-08-14 | this session, after the operator re-blessed the helper (notes §3.101) | **superseded by the 5-wire row above**, which reproduced its figure at the same tree one session later with `SNX_RIG_FLOW` added to the scope. Kept because it is the last row taken on the 3-wire bench, and the sixth independent confirmation of that wiring. Formerly the rig-lane authority row. Zero failed binaries; 126 result lines over 122 cargo targets. Every hardware test ran and passed — all four replug tests including `identity_survives_a_replug_that_renumbers_the_tty` (which swapped the adapters' `/dev` names and said so), the crossover set, and `web_tls_round_trip` under `SNX_TLS=required`. The blessed `grant` verb was exercised for real on the **rebuilt** helper across each re-enumeration (`granted on ports 3-2.3, 3-2.4`, uid 1000); the re-bless was needed because the `sys/` change relinked `devprep`, which is §15.45's design working, not a defect. The self-skip count falls **13 → 5** against the same tree's default-scope run, which is what distinguishes this row from that one; all five are named measurements — the packaging root arm (no root), two browser tests (no `node`), and the two `rts-cts` tests printing the reading that justifies them (a **3-wire** bench, §15.52's legitimate answer, **sixth** independent confirmation). **Equal to the default-scope figure at the same tree in the same session — plan §18 item 30's dual-scope measurement**, and no delta is derived across the two scopes. **Item 67's owed remainder is discharged here**: `xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it` took its **`Honoured`** arm on real hardware, an arm that had executed on no machine anywhere — so §15.61's discrimination is proven against hardware on both kernels, same two adapters, same cable. Supersedes the 999 rig row. |
| **999 passing · 0 failed · 7 ignored**, five self-skips | Linux, **the documented rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** (`SNX_CROSSOVER`, `SNX_REPLUG`, `SNX_TLS` and `SNX_EXEC_CODEC` all `required`) | 2026-08-13 | the alignment-pass session, after the operator re-blessed the helper | **superseded by the 1004 rig row above**, which reproduced every one of its properties one tree later. Kept because it is the row that first established them. Every hardware test ran and passed: all three replug tests including `identity_survives_a_replug_that_renumbers_the_tty`, all five crossover tests, and `web_tls_round_trip` under `SNX_TLS=required` — which no lane had ever demanded until item 60(a). The blessed helper's `grant` verb (§15.55) is exercised for real, granting on both ports across each re-enumeration, so **item 52's rig surface is discharged**. The self-skip count falls **13 → 5** against the same tree's default-scope run, which is what distinguishes this row from that one; all five are named measurements — the packaging root arm (no root, both routes measured closed), two browser tests (no `node`), and the two `rts-cts` tests printing the reading that justifies them (a **3-wire** bench, §15.52's legitimate answer, fifth independent confirmation). **Equal to the default-scope figure at the same tree in the same session — which is plan §18 item 30's dual-scope measurement**, and no delta is derived across the two scopes. |
| **967 passing · 0 failed · 7 ignored**, seven self-skips | Linux, **rig lane minus `SNX_REPLUG`, `SNX_RIG_FLOW` and `SNX_WEB_UI`** (`SNX_CROSSOVER`, `SNX_TLS` and `SNX_EXEC_CODEC` all `required`) | 2026-08-12 | the alignment-pass session (notes §3.84–§3.86) | **the rig-lane authority row**, equal to the same session's default-scope figure. All five crossover tests executed — the self-skip count falls 12 → 7 against the default-scope run, which is how this row proves it is not the default-scope run wearing a rig label. **Three of the drops are named measurements, not conveniences:** the two `rts-cts` tests print the reading that justifies them (`port1 RTS high -> port0 cts:false`, and `false` again with RTS low — a **3-wire** bench, §15.52's legitimate answer and now a fourth independent confirmation), and the two browser tests find no `node`. **The fourth is a deliberate exclusion this session created and must not be read as a pass:** `SNX_REPLUG` is dropped because plan §18 item 52 changed the privileged helper, so `.snx-bin/<profile>/serial-nexus-devprep` is *Stale* (`preflight` says so and exits 2) and this box cannot `setcap` itself. The three replug tests self-skip on an unset `SNX_REPLUG_DEV` rather than running against a helper that is not the one under test — the failure notes §3.83 discarded a whole lane to avoid. **The replug half of this lane is owed at the current tree**, and item 52's rig surface with it. |
| **931 passing · 1 failed · 6 ignored**, four self-skips | Linux, **rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** | 2026-08-12 | the item-47 landing (notes §3.83) | **the rig-lane authority row**, superseding the 894 below and equal to this session's default-scope figure. Run against a freshly `scripts/bless`ed helper (the operator ran it mid-session; an earlier lane was discarded rather than recorded, because the helper binary was replaced underneath it). Every hardware test passed — both replug tests including `identity_survives_a_replug_that_renumbers_the_tty`, all five crossover tests, `web_tls_round_trip` under `SNX_TLS=required`. The two named drops are measurements taken *in this run*, not conveniences: the four self-skips are the two `rts-cts` tests, which print the reading that justifies them (`port1 RTS high -> port0 cts:false`, low -> `cts:false` — a **3-wire** bench, §15.52's legitimate answer and the third independent confirmation of it), and the two browser tests, on a box with no `node`. The one failure is `p3_idle_cost` (item 46), unrelated to the rig. **One lane before this one hung** and is recorded rather than quietly re-run — see the note under this table. |
| **894 passing · 1 failed · 6 ignored**, four self-skips | Linux, **rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** | 2026-08-12 | this session (notes §3.80) | the rig-lane authority row, and the first green one in this record. Both exclusions are measurements, not conveniences: this box has no `node`, and the bench **measures 3-wire**, which §15.52 makes a legitimate answer — so the two `rts-cts` end-to-end tests skip with their reading printed. The one failure is `p3_idle_cost` (item 46), unrelated to the rig. Every hardware test passed, `identity_survives_a_replug_that_renumbers_the_tty` for the first time ever. **Superseded by the 925 rig row above**, taken later the same day at the same scope. |
| **835 passing · 0 failed · 4 ignored** | Linux, rig lane — and again at default CI scope, same session | 2026-08-05 | `17c6e87` (notes §3.68) | twice on the full rig lane, once at default CI scope, 835/0/4 each time — the last dual-scope measurement; superseded by the 852 re-measure of 2026-08-07; not the current-tree figure. **Attribution unreconciled (v17):** notes §3.68's verbatim session record reads 830 (gates scope) and 834/0 · 833/1 (rig lane), and no 835 appears in it — the figure survives only as a v15 Status-table quotation (the re-cited-not-re-derived class), so neither the number nor the dual-scope equivalence is attributable to §3.68. Superseded either way. |
| **961 passing · 0 failed · 7 ignored**, 126 test-result lines | **macOS, default CI scope**, `--no-fail-fast`, `--nocapture` — the **x86_64 rig box**, load 1.14 | 2026-08-13 | this session, measured at `ad4dfb2` (notes §3.93–§3.100) | **the macOS authority row for the rig box, and the session's closing figure.** 126 result lines over **122 cargo targets** (114 `Running` + 8 doc-test). The **+6** over the 955 row is this session's six new guards, counted rather than estimated: `peer_hungup`'s self-test and the Darwin witness guard (item 66); the software-arm `sys` test and the `xon-xoff` rig guard (item 67); `process_cpu_nanos`'s self-test (item 12); and `cpu_nanos`'s wrapper guard, which was Linux-gated before item 12 ungated it. **`p9_pty_collapse`'s anti-spin guard also moved from a self-skip to a pass here without changing the total**, which is item 12's whole point and is invisible in this column — read it in the skip set, not the count. **A self-skip count is deliberately not quoted**: under `--nocapture` the suite's parallel binaries interleave their writes, so a `SKIP` line frequently loses its line start and `grep -c '^SKIP'` undercounts — two runs of this box at adjacent trees read 105 and 102 with every "missing" line present unanchored, which extends notes §3.78 rather than contradicting it. The claim the figure served needs no precision: a macOS default-scope run skips on the order of a hundred device- and Linux-gated tests against Linux's dozen, because the rig is attached but `SNX_CROSSOVER_A`/`_B` are unexported at default scope. So a macOS default-scope figure is **not** comparable to a Linux one test-for-test and no delta between them is derived. Not the CI arm64 runner and not the M4: three machines, none substituting for another (item 18) — and this session measured why that rule has teeth, shipping a `process_cpu_nanos` that was exact here and 24x low on arm64 (notes §3.100). Supersedes the 960, 959, 957 and 955 rows. | The **+1** over the 959 row is item 12's `process_cpu_nanos` self-test; `p9_pty_collapse`'s anti-spin guard moved from a self-skip to a **pass** here without changing the count, which is the item's whole point and is invisible in this column — read it in the skip set, not the total. Every caveat on the 959 row applies unchanged. |
| **959 passing · 0 failed · 7 ignored**, 126 test-result lines | **macOS, default CI scope**, `--no-fail-fast`, `--nocapture` — the **x86_64 rig box** | 2026-08-13 | this session (notes §3.95) | **superseded by the 960 row above.** The **+4** over the 955 row is that session's four new guards, counted rather than estimated: `peer_hungup`'s self-test and the Darwin witness guard (item 66), the software-arm `sys` test and the `xon-xoff` rig guard (item 67). **A self-skip count is deliberately not quoted, and the reason is a correction to how this table has been reading them.** Under `--nocapture` the suite's parallel binaries interleave their writes, so a `SKIP` line frequently loses its line start and a `grep -c '^SKIP'` undercounts: two runs of this box at adjacent trees read 105 and 102, and all of the "missing" three are present in the log unanchored. So the figure is ~100–105 and is **not stably countable this way** — which extends notes §3.78 rather than contradicting it (that entry established the count is 0 *without* `--nocapture`; this adds that *with* it, and under parallelism, it is approximate). What the figure is *for* still holds and needs no precision: a macOS default-scope run skips on the order of a hundred device- and Linux-gated tests against Linux's dozen, so the two are not comparable test-for-test and no delta between them is derived. Supersedes the 957 and 955 rows. |
| **957 passing · 0 failed · 7 ignored**, 126 test-result lines | **macOS, default CI scope**, `--no-fail-fast`, `--nocapture` — the **x86_64 rig box** | 2026-08-13 | this session (notes §3.94) | item 66's two guards over the 955 row. **Superseded by the 959 row above**, later the same session at the same scope. |
| **955 passing · 0 failed · 7 ignored**, 126 test-result lines | **macOS, default CI scope**, `--no-fail-fast`, `--nocapture` — the **x86_64 rig box** (MacBookPro15,1, Darwin 24.6.0 / macOS 15.7.8, 12 cores) | 2026-08-13 | this session (notes §3.93) | **superseded by the 959 row above.** The first figure ever taken on this box at a tree whose macOS-only guards pass. 126 result lines over **122 cargo targets** (114 `Running` + 8 doc-test). **This row read "105 self-skips" when written; that figure is withdrawn as a precise count** — see the 959 row for the measurement that found `grep -c '^SKIP'` unstable under `--nocapture` parallelism. The claim it was serving survives unchanged and needs no precision: a macOS default-scope run skips on the order of a hundred device- and Linux-gated tests against Linux's dozen, because the rig is physically attached but `SNX_CROSSOVER_A`/`_B` are unexported at default scope. A macOS default-scope figure is therefore **not** comparable to a Linux one test-for-test, and no delta between them is derived here. **The preceding run at this same tree read 953 · 1+1 · 7** and is kept rather than replaced, because it is the measurement that found the defects: `probes::tests::the_software_readback_reports_unmeasurable_rather_than_answering` (a baseline `Termios` taken off a pty master, which Darwin answers `ENOTTY`) and `both_gates_refuse_an_unsupported_verdict_and_are_shown_able_to` (a report-shaping premise that knew about P12 and not P2) — both item 69, both repaired in the same commit as this row, and both red in CI's `macos` job on every push since they landed. Not the CI arm64 runner and not the M4: three machines, none substituting for another (item 18). |
| **896 passing · 0 failed · 6 ignored**, 122 test-result lines | **whole workspace (macOS)**, CI `macos-*` arm64 runner | 2026-08-13 | CI run 31657666919, job 94315579211 (notes §3.83) | **the macOS authority row**, superseding the 860 below. No exclusions — the lane runs `cargo test --workspace --locked --no-fail-fast` — on macOS 26.5.2 / arm64. The +36 over the 860 row closes exactly: this session added 37 tests, of which 36 run here (the devprep capability-fold guard is inside the Linux-only platform module). The six device-gated pattern-wait guards run and self-skip, which is why the acceptance battery was deliberately split so six of its twelve need no serial device. Skip count not stated: CI does not pass `--nocapture`, so it cannot be read from that log (notes §3.78). |
| **860 passing · 0 failed · 6 ignored** | **whole workspace (macOS)**, CI `macos-*` arm64 runner | 2026-08-12 | CI run 31605283603, job 94144842458 (notes §3.76) | the first green macOS lane in this record. **Superseded by the 896 row above**, taken 2026-08-13 at the same scope. No exclusions — CI runs `cargo test --workspace --locked --no-fail-fast`. The skip count is **not stated**: CI does not pass `--nocapture`, so it cannot be read from that log (notes §3.78). The preceding run read 859 · 1 · 6 at the same tree; its one failure is `a_client_clearing_extproc_has_it_re_asserted_so_changes_keep_surfacing`, which asserted Linux's EXTPROC retention on both kernels and is repaired in the same commit as this row; it is the *pre-fix* reading, kept because it is the measurement that found the defect. Not the x86_64 rig box — three machines, none substituting for another (plan §18 item 18). |
| **760 passing · 1 failed · 4 ignored** | macOS, gate scope **plus** `--exclude serial-nexus-devprep` | 2026-08-05 | `60b9d0f` (notes §3.65) | not the documented scope — quote it with both exclusions. Superseded by the row above; its one failure was the `rts-cts` platform gap §15.53 has since turned into an assertion of refusal. |
| **3.94 s passive · 11.6 s Tier-3** (doctor wall clock) | Linux, one box | 2026-08-05 | `f8315cc` (notes §3.53) | a cost figure, not a gate figure; supersedes the 3.74 s of notes §3.50; pre-P14 — the P14 search takes a Tier-3 run to 35.0 s (`77f6798`, `docs/doctor/README.md`). |

**The cargo-target count, re-derived** (plan §18 item 23c): the two prior records disagreed at
104 versus 106 `Running` lines because they were taken at different eras, not because either was
wrong. Measured once on this tree: **109 `Running` + 8 doc-test = 117 cargo targets**, against 121
`test result:` lines. The four-line gap is the nested template subprocesses.

**One rig lane hung and is recorded, not buried.** Between the two rig runs above, a lane stalled
39 minutes on `p6_outage::outage_faults_then_purges_then_recovers_byte_clean` — the receiving leg
sat `waiting` with "peer disconnected; awaiting reconnect" and `reconnect_count: 2` while the box
idled at load 0.19. It is recorded because a lane that is killed and re-run without a note is
indistinguishable from a lane that never hung. **Not attributed to the pattern wait**, and the
attribution is measured rather than asserted: the same test passed in the same session's
default-scope run on the same binaries, passed in the preceding rig lane, and passes **5 of 5**
in isolation at full rig scope (~4 s each); nothing in the change touches `leg.rs` or the outage
path. The shape points at whole-suite parallelism around loopback port reuse — this file's own
sibling test is named `a_refused_listen_bind_retries_until_the_address_frees` — which is a
pre-existing harness hazard, not a finding this session can close. Neither is it dismissed: one
unreproduced hang is exactly the evidence AGENTS §8 says not to reason from.

Two rows need sentences no cell can hold. The equivalence claim: a dual-scope equality was
recorded only at the 835 era (row three), and v17 finds even that attribution unreconciled with
its cited session record (the row carries the annotation) — the equivalence must not be asserted
at any era until plan §18 item 30's run exists. The macOS row: its extra exclusion existed because
`serial-nexus-devprep` did not build off Linux, and the crate split that fixed the build landed in
the same session (notes §3.65), so the documented scope needs no second exclusion today; its one
failure was the `rts-cts` platform gap §15.53 has since turned into an assertion of refusal; and the "no macOS run exists at the current tree" debt — `cargo test` compiled on no macOS at
`3e23c52` (notes §3.71, fixed at `25dcb9d`) — is **half discharged**: the suite half is the top
macOS row, run whole-workspace on the CI arm64 runner 2026-08-12. The **capture** half is still
owed, and a CI artifact is not it: a committed `docs/doctor/` report is a deliberate act with its
era stated (§16.13), not a build by-product. Both doctor jobs now upload the JSON twin beside the
Markdown, so the artifact exists to be taken rather than having to be re-run by hand (plan §18
items 8, 18).

**Named scopes.** A scope name is part of the figure. Four are defined, and a figure taken at one
scope never supersedes a figure taken at another:

- **default CI scope** — Linux, `cargo test --workspace --locked`, no `SNX_*=required` mode
  exported: gated tests self-skip, visibly, under plan §3's skip discipline.
- **rig lane** — Linux, by hand on the crossover box: `SNX_CROSSOVER=required SNX_REPLUG=required
  SNX_TLS=required SNX_RIG_FLOW=required SNX_WEB_UI=required` with both ports named, **both
  `SNX_REPLUG_DEV` and `SNX_REPLUG_DEV_B` named**, and `--no-fail-fast` (notes §3.68; AGENTS §3's
  rig-lane spelling predates `SNX_WEB_UI` and is corrected at landing; `_DEV_B` was missing from
  every spelling until 2026-08-12 — notes §3.75 — which made the documented lane fail rather than
  skip; the env-var table is at plan §3). A named drop is part of the scope:
  `SNX_RIG_FLOW=required` is dropped on a bench P5 measures as 3-wire, and `SNX_WEB_UI=required`
  on a box without `node` — both are measurements, not conveniences, and forcing either fails
  the lane for the operator's cabling or toolchain (notes §3.80); a figure taken with a drop
  names it in its scope cell, as the rig-lane authority row does.
- **macOS gate scope** — `cargo test --workspace --exclude serial-nexus-web --no-fail-fast`, the
  documented Mac scope.
- **whole workspace (macOS)** — no exclusions; one recorded measurement (2026-08-04 at `fa4b12d`,
  held in the notes' dated record), and excluded-scope figures never supersede it.

> **A figure is quotable only with its scope column.** Every count travels as (value, scope name,
> date, commit or artifact). Deltas are quoted only where one session measured both ends — never
> re-derive a delta across scopes, eras, or unmeasured commits. Withdrawn figures stay withdrawn
> — §15.44's entry carries the register.

**Companion:** the current design document — named by filename in AGENTS §2, README's
documentation index, and the ban-statement allowance in `itest/tests/meta_names.rs`, deliberately
not here, so this line can never go stale. One citation rule holds across the pair: a bare `§N`
cites the design — everywhere, including inside this plan — and this plan's own sections are
always spelled `plan §N`, even in self-reference. An intended change of this generation:
the previous plan's scoped exception (bare `§N` naming its own sections) is retired — notation
that varies by document is the shape that once produced a forty-site defect class (review 37,
37-WEBC-8). Implementation-notes entries are `notes §3.NN`; the
operating manual is `AGENTS §N`. The design is normative; where implementation reality disagrees
with it, the design gets a new §15 entry before the code diverges.

**Shape:** plan §1–§3 are live doctrine, plan §4–§17 the executed record — compressed after every
live rule they carried was extracted into doctrine — and plan §18 the work ledger. Section
numbers, sub-item anchors, plan §3 rule numbers, and plan §18 item numbers are append-only.

## 1. Approach

Five principles order everything in this plan and every track the ledger opens.

**Retire risk before writing architecture.** Kernel-behavior questions go to
`serial-nexus-doctor` — one consolidated capability checker run per system, never spike binaries
run one by one (§15.17) — and a probe report that contradicts the design amends the design first
(§13; AGENTS §5, §7). The generation began this way (§15.14's EXTPROC + TIOCPKT) and every later
kernel question was handled the same way: measure, then decide.

**Walking skeleton, then muscles.** The thinnest end-to-end system — config file in, daemon up,
real bytes flowing device↔PTY, CLI talking JSON-RPC — came before any feature depth; every later
phase and track extended a working system, so integration risk is paid once, early.

**Tests pin to the RPC surface, never to CLI output.** Per §15.16 the CLI shape churns freely on
human and agent feedback; integration tests therefore drive `serial-nexus-daemon` over JSON-RPC
directly (or via `serial-nexus-ctl --json`, a pass-through). Nothing about the CLI is contract —
only the RPC surface is.

**Every check is a command whose exit code is the verdict.** This plan is executed with an AI
coding agent in the loop, so validation cannot live in prose. The executable form of every exit
criterion is the `serial-nexus-itest` harness (plan §3, plan §5): each check runs idempotently in
a temporary directory, emits a machine-readable verdict, and passes or fails by exit status;
data-integrity assertions use seeded pseudo-random streams and checksums, so "no bytes lost,
duplicated, or reordered" is a single comparison. The durable principle from the executed
bootstrap (plan §8): the ability to check the work exists before the first feature does.

**Test doubles are in-workspace and permissive.** No mainstream permissively-licensed tool covers
socat's PTY-plumbing role, and external plumbing cannot emit verdicts anyway (plan §3); the
workspace therefore ships `serial-nexus-sim`, a purpose-built double using the same permissive
PTY and socket calls as the daemon — validating with it exercises those calls twice. It ships
with the repository, never with releases.

## 2. Workspace and toolchain

**The map.** One Cargo workspace, edition 2024: thirteen members, two deliberate exclusions.
This table is ground truth for where the code lives; a member change updates it in the same
commit (AGENTS §2's claims-match-reality rule), and the naming gates and the operating manual's
crate list defer to it.

| Directory | Package | Artifact | What it is |
|---|---|---|---|
| `codec-api/` | `serial-nexus-codec-api` | lib | Codec trait, event vocabulary, envelope frame types (§8); no project-internal deps; the envelope's versioning promise (§15.15). |
| `codecs/reference/` | `serial-nexus-codec-reference` | lib | The v1 envelope as a demux/remux codec and the link codec's core (§7.5, §9). |
| `core/` | `serial-nexus-core` | lib | Graph model, data-plane contracts, config/state types (§3–§5); pure logic except the root-prefix-parameterized resolver (§12). |
| `rpc/` | `serial-nexus-rpc` | lib | JSON-RPC wire types, §15.16's stable surface; also the `socket` module, the one socket-path implementation daemon and `ctl` share (notes §3.72). |
| `sys/` | `serial-nexus-sys` | lib | **The one crate carrying `unsafe`** (§16.3): raw ioctls, `ptsname`, `poll(2)`, `peer_hungup` and `process_cpu_nanos` (items 66, 12), `usb_macos` (notes §3.66), `honours_flow_control` (§15.53, §15.61 — `honours_rtscts` until it was generalized to both modes). |
| `daemon/` | `serial-nexus-daemon` | **lib** `serial_nexus_daemon` | The daemon as an embeddable library (§15.26): boundary nodes, data-plane runtime, control plane, state file, codec registry. |
| `daemon-bin/` | `serial-nexus-daemon-bin` | **bin** `serial-nexus-daemon` | The thin binary: flags, tracing subscriber, `run` with the built-in registry. |
| `ctl/` | `serial-nexus-ctl` | bin | RPC client plus rendering; `--json` passes the raw result through. Nothing here is contract. |
| `web/` | `serial-nexus-web` | lib + bin | The web console (§17): a pure loopback RPC client; the daemon never links or knows it. |
| `sim/` | `serial-nexus-sim` | bin | The test double (plan §3); ships with the repository, never with releases. |
| `devprep/` | `serial-nexus-devprep` | bin | USB re-enumeration and device-access helper (§12, §15.45, amended §15.55): platform dispatcher over `src/linux/`; the blessed copy carries `cap_dac_override,cap_fowner+ep`; the commands shown, run, and verified all derive from `REQUIRED_CAPS` (§15.55). |
| `doctor/` | `serial-nexus-doctor` | bin | The capability checker (§13, §15.17); ships with releases — it is the support tool. |
| `itest/` | `serial-nexus-itest` | lib + tests | The integration harness (plan §3): boots daemon, sim, and web as subprocesses; the canonical form of every exit criterion. |
| `fuzz/` | `serial-nexus-fuzz` | fuzz bins | **Workspace-excluded on purpose** (nightly + libFuzzer); own `Cargo.lock`; targets enumerated by `cargo fuzz list`, never a hand-kept list (plan §3). |
| `examples/external-codec/` | own workspace | template | **Workspace-excluded on purpose**: built from a consumer's position on every push — proven, not promised (§15.26, plan §10.3). |

**The daemon naming triple.** The package `serial-nexus-daemon` is the *library* crate in
`daemon/`; the binary `target/debug/serial-nexus-daemon` is built by package
`serial-nexus-daemon-bin` from `daemon-bin/`. This is why every lane runs
`cargo build --workspace` before `cargo test`: the harness boots the plain `target/debug/<name>`
artifacts, which only `cargo build` emits.

**Non-crate directories.** `tests/ext-codec/` is *not* a cargo test target: Python exec-codec
fixtures the harness consumes, including a deliberately broken half-duplex citizen (§8, §15.22)
— its **six** fixtures (`passthrough.py`, `passthrough-codec.py`, `lag.py`, `half-duplex.py`,
`strict.py`, `deaf.py`) and
the kit's `Hoarder` positive/negative pair are named must-preserve by §8 (a tidier session must
not simplify them away). *(This line said four and omitted `strict.py` — the `--error-paths`
positive control shipped with item 34 — until 2026-08-12.)*
`expectations/` — the doctor gate files `linux.jq`/`macos.jq` (plan §3). `scripts/` — exactly one
file, `bless` (§15.45); the bash validation suite is retired and deleted, so a "ported from
`scripts/validate/...`" head comment is provenance, not a path. `packaging/` — systemd unit,
example config, and optional udev rules for narrower device access, a deployment convenience
distinct from the *declined* replug-lane udev rule of §15.45. `web/ui-tests/` — the pinned
Playwright specs, driven from Rust (plan §3). `docs/rpc/` — the normative RPC schema reference
(§10 delegates schemas there). `docs/doctor/` — the frozen measurement-artifact ledger (§13,
§16.13).

**Naming (§15.40; AGENTS §11).** Binaries `serial-nexus-*`; importable crates `serial_nexus_*`;
directories short, carrying no family prefix. The corollary that bit twice during the rename
track: one name has three spellings — a filesystem-scanning gate spells the *directory*, a
manifest or `use` spells the *crate*, a booted artifact spells the *binary* — and a blanket
rename conflates them (plan §17). Retired names appear only in `docs/historical/`, the frozen
reviews, and the captured `docs/doctor/` artifacts, enforced by the meta-gate. And a binary is
named for what its callers invoke it as, never for the mechanism of its oldest verb — the
helper rename's lesson, argued once from the wrong axis before being decided from the callers
(notes §3.81).

**Split discipline.** The workspace stays deliberately small until a second consumer forces a
split — premature crate splits are churn, and node implementations live in the daemon library
until something else needs them. The concurrency architecture is design content, deliberately
not restated here: §5 carries the hybrid data plane (`poll(2)` readiness, the `AsyncFd` ban,
dedicated blocking readers; §15.18/§15.19) and the endpoint-keyed wiring that lets a new node
shape plug in through `shape()` alone (§15.23).

**Rust hygiene.** MSRV is pinned at **1.97** in CI, and it is a two-way constraint: the sources
need the ≥1.88 let-chains, and clippy 0.1.97 requires the let-chain collapse older clippy rejects
— raised deliberately, never by drift, from one env var every compiling job reads (the nightly
fuzz lane is the sole exception). rustfmt and clippy run with warnings denied, on the workspace
and on the minimal no-built-in-codec daemon build. `#![forbid(unsafe_code)]` holds everywhere
except the `serial_nexus_sys` **crate** (§16.3) — a crate, not the daemon-internal module an
earlier generation described. `tracing` is wired so debugging never starts from printf.
Configuration files are TOML; the RPC carries JSON — both through serde, so the §8
attribute-table pattern is uniform.

**Licensing enforcement is CI, not vigilance.** `cargo-deny` runs on every push with the
permissive allowlist (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-DFS) and an explicit
ban list naming the known landmines from §13 — `serialport`, `mio-serial`, `tokio-serial`, and
any libudev binding — so they cannot re-enter transitively; the gate is proven to *reject* a
planted banned crate under `SNX_LICENSE_GATE=required`. `Cargo.lock` is committed: this is a
binary workspace, and the deny gate is only as strong as the graph it inspects. Tool binaries
are version-pinned in the workflow, because `--locked` pins a tool's dependencies, never the
tool itself.

**CI lanes.** `.github/workflows/ci.yml` is the authority for lane commands (plan §3). Push
lanes: `check` (fmt; workspace and minimal-daemon clippy; the Apple cross-check on
**both** triples, `--all-targets` — neither triple is a superset of the other, and `cargo build`
compiles no test target, notes §3.71. That cross-check runs **clippy**, not `check`, as of
2026-08-12 (item 54): lints had run on the Linux lane only, so macOS-only dead code —
a helper whose one caller is `#[cfg(target_os = "linux")]` — was caught by no lane at all,
measured against a plant that Linux clippy and the Darwin `cargo check` both passed and the
Darwin clippy failed; then `cargo build --workspace --locked` and the full
`cargo test --workspace --locked` — the artifacts the harness boots exist only after the
build step); `license-gate` (deny plus the planted-crate rejection proof); `doctor`
(`jq -e -f expectations/linux.jq` over the probe run, report archived; `skipped(no adapter)` is a
valid CI verdict, a failing probe is not — plan §3); `external-codec` (the consumer-position
template build, plan §10.3); `macos` (**whole workspace (macOS)** — `cargo test --workspace --locked --no-fail-fast`,
with no exclusion — plus the `macos.jq` gate; an arm64 runner, not the local x86_64 rig box —
plan §18 item 8 owes its doctor artifact. This line said "the macOS gate scope" until 2026-08-12,
naming a *narrower* scope than the lane runs: harmless to the lane, which is stricter than its
label, and not harmless to a figure taken from it, which rule 19 would have filed under a scope
it was not measured at — notes §3.75);
`web-ui` (Playwright under `SNX_WEB_UI=required`, spec-count floor). Scheduled/dispatch lanes:
`soak-nightly`, `sweep-nightly` (`--include-ignored`), `web-ui-nightly`, `fuzz-nightly` (targets
from `cargo fuzz list`, with an empty-list guard). The rig lane is by hand on the crossover box,
deliberately not CI: CI has no adapters, and rig-gated tests self-skip there visibly (plan §3).

## 3. Validation toolkit

This section is the plan's live core: the tools every track validates with, and the harness
doctrine — twenty-two numbered rules. Rule numbers are cited from code, gates, and the notes, so
they are append-only: a new rule takes the next free number, and no rule is ever renumbered or
reused.

**The external-tool question, answered.** socat does exactly the needed trick — a PTY whose
slave another process opens like a serial line, with a `link` symlink — but it is GPL-2.0, and no
mainstream permissive equivalent covers the PTY side. Under the §13 policy socat may still *run*
beside the project as an optional manual cross-check; but no validation code requires it, for two
better reasons than license comfort: an external relay cannot judge outcomes, and a purpose-built
double can.

**`serial-nexus-sim`: one binary, several doubles.** Every mode is deterministic under `--seed`,
prints one JSON verdict line on exit (`{"tool":"serial-nexus-sim","mode":...,"pass":...,
"sent":...,"received":...,"sha256":...}`), and
exits 0 only on pass; the verdict marks `timed_out` so a deadline is never read as a drop
(rule 3), and budget reads carry `overshoot` — surplus bytes beyond the requested count, a lower
bound by construction — folded as `budget_met = (len == n && overshoot == 0)` in the one shared
function, so a long stream (contamination) and a short one (`timed_out`) are different verdicts
that cannot be re-derived differently per call site. The four sim-double properties are stated
once, at rule 2. The modes:

- `pty` — create a PTY pair via the same permissive calls the daemon uses; maintain `--link PATH`
  (a stable symlink standing in for a device path or a by-id entry); run exactly one behavior —
  the refusal enumerates the set when none is given: `--echo`, `--source`, `--sink`,
  `--report-termios` (what the daemon applied — validates the §7.1 reopen
  ritual and the §7.2 baseline from the far side), or `--stall` (hold the master open and never
  read it — a *total* stall, which is what the head-of-line and fragmentation tests want). Two
  behaviors were deliberately never built, and the reasons are contract (review 37, 37-TOOL-6):
  expect/send scripting — the harness drives `client` mode over the RPC surface, the double
  reports and the harness judges — and client-presence fault injection, done by starting and
  stopping the double as a subprocess (§15.31), with no in-process flag to keep honest.
- `client` — open the *daemon's* PTY like an operator would: seeded data, echo verification, a
  `--read-rate` throttle, attach/HUP observations. Its readiness handshakes are two flags marking
  two different states, because a readiness file must mark the state the hazard is about (§15.48,
  rule 16): `--ready-file` appears on the first byte *read back* — proof the read loop is
  draining — while `--open-file` appears the instant the path is open and configured, before any
  read, the only signal that can gate a hazard about *openness*.
- `nullmodem` — two PTY pairs bridged in-process (`--link-a`/`--link-b`): a software crossed pair
  for CI-testing P5's discovery and classification (whose characterization there correctly
  reports `skipped(not a UART)`), and for any harness wanting a two-port rig without hardware.
- `mux` — emit and verify reference-framed multichannel streams with per-channel manifests, plus
  `--corrupt-every N` with a computed expected-loss manifest for resynchronization tests.
- `envelope` — drive an external codec process through the golden-vector battery; with `--exec`
  it is the any-language driver of the codec-author kit (§8).
- `wire` — a hostile or conforming v1 peer: crafted hellos, oversize and truncated frames,
  unknown-channel data — the driver for the §9 conformance suite. Hostility flags from the
  review-37 hardenings: `--stall` (stream hostward while never reading the socket), `--mute-ms`,
  and `--trickle-hello` (withhold the hello's *final* byte — incompleteness holds by
  construction, not by out-pacing an assumed deadline).
- `tcp-proxy` — sit between two daemons with `--drop-after`/`--restore-after` for unprivileged
  link-outage injection.

**The resolver seam.** The resolver takes a root prefix (`--dev-root`, default `/`), making
`/dev/serial/by-id` a fixture directory in tests: symlink trees pointing at sim pts nodes
reproduce normal adapters, no-serial clones, multi-interface devices, and identity squatters —
the whole §12 matrix, unprivileged, no hardware. A documented, first-class test seam, not a
hidden hook — and the §11 pre-create checks resolve through it too, so a fixture tree exercises
the same doors a real box does.

**`serial-nexus-doctor`: every probe, one report.** The design's kernel-behavior assumptions are
checked by one consolidated binary rather than per-spike one-offs. The probe roster runs P1–P15;
§13 owns the measurement doctrine and `docs/serial-nexus-doctor.md` owns the per-probe table and
the one-shot support protocol — this plan duplicates neither. Every probe is self-judging:
question, observed behavior, `supported`/`degraded`/`unsupported`/`skipped(reason)`, and the
one-line design consequence. JSON is the artifact of record and Markdown the view a human reads;
one invocation produces both (`--json-out` writes the JSON twin beside the Markdown paste), the
report's own header sentence asks for the JSON, and `--field-set` on a report carrying zero
observations is exit 2, never a confident digest (notes §3.74). The gate invocation matches CI, the one authority for gate spellings (rule 21): one run emits
both renderings and the gate reads the JSON twin —
`serial-nexus-doctor --markdown --json-out <path> > <report.md>`, then
`jq -e -f expectations/<platform>.jq <path>`. The `-e` is load-bearing, and so is the plumbing:
the doctor's own exit status must reach the lane — both lanes once piped it into `tee`, which
discarded it (notes §3.77; rule 22) — and the gate reads a *file* so the doctor runs once,
because two runs of one rig are two measurements (plan §18 item 43). Two rules ride the report: `unsupported` fails the process — exit 1, a stop
condition: surface the report for a design amendment rather than coding around it (§13; AGENTS
§7 generalizes it tree-wide) — and on CI, `skipped(no adapter)` is a valid verdict; a failing
probe is not. The doctor is **passive by default**: any probe that would transmit on a serial port
requires that port to be named on the command line, because a listed port could be wired to live
equipment. The shipped rig certificate's scope — a baud mismatch and local break *assertion*
only, the rest riding the Tier-3 checklist, and off Linux the portable UART predicate with
structurally unmeasurable items carried as data, never bare failures — is stated at §15.21 and
§15.47 (review 37, 37-TOOL-3), not re-legislated here. And the daemon never reads doctor output —
runtime degradation paths are unconditional — so a wrong probe can mislead a developer but never
the data plane.

**Hardware tiers: no target device, ever.** Every hardware-dependent check is designed around
USB-serial converters wired to nothing. **Tier 1 — a dangling converter:** enumeration and
identity, TIOCEXCL exclusivity, termios acceptance including custom bauds, DTR/RTS
set-and-readback, unplug surfaces, replug under traffic, squatter swaps — the whole §12 and
faulted-and-wait matrix on real silicon, no receiver required. **Tier 2 — one converter, TX
jumpered to RX:** a true driver-level data path for seeded round-trips and RX-overrun counters;
caveat, one shared clock cannot detect baud *inaccuracy*. **Tier 3 — two converters cross-wired
as a null modem:** independently clocked ends — baud accuracy, parity and framing observation
via TIOCGICOUNT, break reception, and modem-line signaling become assertable, and the pair
doubles as a physical instance of the design's symmetric configuration. Tier detection and
certification is probe P5: opted-in ports classified by nonce (dangling, loopback, or paired —
pairs verified in both directions so half-crossed wiring is named, not mysterious), the rig
characterized into the certificate the tiered checklist requires before any serial_nexus code is
blamed — and it stops there: the doctor certifies the rig, it never drives the daemon through
it. Flow control with floating CTS is driver-dependent: reported, never assumed — deepened by
P15 and §15.53, which turned a driver that silently drops `CRTSCTS` from a late fault into a
refusal at `load` (§7.1, §11).

**Two conventions that outlived the bash era.** The normative harness is `serial-nexus-itest`
(§15.31; plan §5); the bash validation suite is retired and deleted (§16.11, executed), and a
`ported from scripts/validate/...` head comment is provenance, not a path. Two of its conventions
survive as live rules. First, **presence is not readiness**: a harness that releases a data burst
on `client_present` races the client's read loop, because a slave can be open a beat before
anyone drains it — feed gates handshake on an actual first byte read back, never on presence
alone (the once-flaky demux script was exactly this race in the test, not the daemon; its
first-byte retrofit — the sim's `--prime-*`/`--ready-file`/`--wait-file` primitives — holds at
0 failures in 35 runs under full CPU saturation). Second, **control-socket paths are bounded by
`SUN_LEN`** (about 108 bytes): every harness creates its runtime dir with
`mktemp -d /tmp/snx.XXXXXX`, never under a long scratch path — the daemon diagnoses the overflow
clearly, but a harness should never trigger it.

**The provider seam, and the last hop's physics (§15.48).** One seam, `serial_pair_or_rig()`,
whose decision table is a pure function — `choose_pair_source(software, rig, force_rig)` —
unit-testable on any box without opening a port. **Software wins whenever it exists**: a box that
happens to export `SNX_CROSSOVER_A`/`_B` must not silently move tests onto hardware an order of
magnitude slower, CI needs determinism, and the rig is already claimed by the hardware suites.
`SNX_SERIAL_PAIR=rig` forces the fallback so the arm is exercised on the platform of record —
AGENTS §9's proxy rule applied to a provider — and forcing with no rig visible is a hard failure,
never a silent software fallback. A rig-executing test prints its provider before it transmits (a
pure unit test of the decision table is never counted as hardware evidence; the one open residual
lives in §15.48's entry). The physics the seam taught is harness contract: **a USB-serial port
that is not open does not receive** — no URBs are submitted, the adapter's small FIFO
overflows, and the loss is exactly the airtime (at
115200 baud a sink delayed 0.2 s loses ~2304 bytes; measured 2323/2321/2357). The general form is
**prove the far end receives before measuring the wire**: `prime_the_wire_once` pushes a small
payload each way through a throwaway graph and waits for it to *arrive* before any measured test
boots — proving reception rather than inferring it from both nodes reading `active` — once per
process, in its own run dir, so no primed byte reaches a capture under test (notes §3.47, §3.70;
the pattern closed the replug-lane flake, whose 64 missing bytes were one re-enumerated adapter's
first USB bulk packet, never a product defect).

### Harness doctrine: the twenty-two rules

Rules 1–7 come from the flake session (§15.36); rules 8–16 joined between the v14 and v15
generations, each bought by a measured failure (§15.46–§15.50; notes §3.29–§3.54); rules 17–21
consolidate doctrine the record already carried — new numbering, not new practice; rule 22
joined at the v17 revision, its three instances measured in one 2026-08-12 session. The citation
on each rule is what stops re-litigation.

1. Harness protocol clients are *fill-then-commit* — buffer non-consumingly, parse in place,
   drain exactly one whole frame; a deadline expiry never consumes bytes, because the daemon's
   5 Hz snapshot stream means expiry always lands inside a live frame (§15.36).
2. Sim doubles are subprocesses (§15.31 — evaluated and kept: an in-process double has no honest
   way to model client presence), and a double modeling stateless hardware tolerates bare hangups
   forever — pause (never spin, never exit) while peerless — with its idle CPU asserted (§15.36).
3. Assert completeness only where the design promises it: a test demanding a large hostward
   stream arrive complete provisions `hostward_buffer` explicitly with a comment citing
   §5/§15.19, and a shortfall matching `received + dropped_slow_consumer == sent` is the
   lossy-boundary fingerprint, not a data-loss bug; the sim's verdict marks `timed_out` so a
   deadline is never mistaken for a drop (§5's loss taxonomy orients). The depth that protects
   an assertion sits on the node whose own pump feeds the asserted consumer — boundary drops
   never backpressure upstream, so a depth provisioned anywhere else never substitutes (both
   recorded misattributions were placement, not provisioning).
4. A regression guard is valid only with *fail-first proof* — demonstrated to fail against the
   unfixed tree; two guards in the flake session passed against their defects until redesigned
   (§15.36; rule 17 is the proof's own validity checklist).
5. Flake diagnoses get independent adversarial verification before the fix ships (§15.36).
6. Run the suite on a quiet box *and* under parallelism; never filter suite output before the
   failing test's name is captured (§15.36; AGENTS §6). Load cuts both ways: a flake can be
   *suppressed* by load — measured, load once widened a client-spawn latency past the flood
   window it raced — so a green loaded re-run is evidence of nothing; reproduce
   deterministically instead.
7. CI loops enumerate from the tool (`cargo fuzz list`, never a hand-kept list) and meta-gates
   assert *execution*, not file existence (AGENTS §3).
8. **A byte counter is read while the client that fed it is still open** — asserting it after the
   close asserts that the *kernel* retained the bytes, which Linux does and Darwin does not
   (doctor P13, committed 2026-08-05 artifacts on both kernels: `retains` in tens of µs against
   `waits-then-discards` at ~600 ms, and the `O_NONBLOCK` shape loses unconditionally in 29 µs —
   why the rule stays absolute rather than platform-gated). Enforce the ordering by the compiler
   where possible: `settled_while_open(rpc, &client, ..)` borrows the client that `drop(client)`
   moves, so moving the observation below the close is `E0382` on every platform. The
   `OpenWitness` idiom is *the* byte-counter idiom — witnesses proven open twice, before the wait
   and when the condition became true — with its bound stated where the claim is made: the borrow
   forbids *relocation*, not rewriting (notes §3.29, §3.56, §3.60). The exception form is exact:
   what may sit below a close is the single post-close assertion whose subject is the close's
   own edge — never "the guard", and never on the theory that a counter is unreadable before
   the close (notes §3.60: purge-on-acquire moves the same counter while a client is still
   open).
9. Rebuild the booted binaries after any product edit or in-place revert: `cargo test` never
   emits the plain `target/debug/serial-nexus-*` artifacts the harness boots, and
   `cargo test -p serial-nexus-itest --test <name>` does not rebuild the daemon — a fail-first
   proof without a fresh `cargo build -p serial-nexus-daemon-bin` simply passes and reports
   nothing.
10. Every scanning, skipping, or threshold guard ships with an *executed* negative control: plant
    the violation in every spelling the matcher claims (inflections and hyphenations included),
    drive a planted offender through the real walker, prove count floors fire, and prove the skip
    path prints its SKIP line while the verdict stays green — a gate that cannot fail is worse
    than a missing one, because it is counted as coverage (AGENTS §3's "prove the matcher").
    Spellings include every *verdict arm* a clause can meet — `supported`, `degraded`, `skipped`
    — not only textual variants (notes §3.64: `degraded` was the third spelling, and the one a
    real rig produces); and a walker's skip list is a matcher too — a **nested checkout** is
    skipped by the is-a-checkout predicate (a `.git` entry exists: a worktree leaves a file, a
    clone a directory), never by a name list that has to guess where worktrees get put. Build
    outputs are a separate, static skip set and are correctly named there (`SKIP_DIRS`,
    `itest/tests/meta_names.rs`); the rule bans guessing at *checkouts* by name, not the fixed
    set — an absolute "never by name lists" would read as a licence to delete `SKIP_DIRS` and
    re-red every gate on `target/`.
11. Every skip class gets a required mode — one mechanism, mirrored, never reinvented — and a
    skip message names what the provider actually saw and must never be false on the box printing
    it (notes §3.35). The instances are enumerated in the required-mode table below; the roster's
    authority is the code, not any prose list — rule 7 applied to this rule: v15's inline list
    went stale within its own generation. Where a precondition differs by kernel, the skip keys
    on the *measured reading*, citing its artifact, never on `cfg(target_os)` — a cfg-keyed skip
    is the same defect pointing the other way: a Darwin that gains the behavior must run the
    test, and a Linux that loses it must redden rather than skip (notes §3.76; `SNX_RIG_FLOW` is
    the same rule applied to a rig).
12. Verification is blind and frozen: adversarial verifiers get the finding and the tree, never
    the diagnosis, and the tree does not move while they read; refuted diagnoses are recorded —
    they are as load-bearing as confirmed ones (AGENTS §9; §15.34, whose entry keeps its
    founding case study — the v12 audit's 35 of 43 — so the protocol is never "simplified").
13. Every cross-kernel repair carries a pre-registered falsifier written before the run, and its
    outcome is recorded as held, half-met, failed-direction, or fired — notes §3.40's fired,
    refuting its author's own refutation.
14. A quantity that varies run to run gets three sequential runs minimum, on a measured idle box
    with the load recorded; kernel claims cite committed artifacts, never scrollback (§16.13);
    and a guard must not pin the *other* kernel's answer — the prediction lives in prose, where
    being wrong is a record.
15. Retries are zero everywhere; the failing test's name is captured verbatim before any rerun;
    `--no-fail-fast` for platform validation; `git worktree`, never `git stash`, when the tree
    holds a large uncommitted set; and a cleanup path that can throw must not replace the body's
    error (§15.36; AGENTS §8).
16. A harness must fail, not wedge: child-driving harnesses use an *idle* deadline reset by
    progress (a slow subject is never failed for being slow, only for stopping), and a readiness
    file marks the state the hazard is about — `--open-file` at open-and-configured, because
    first-byte-read is definitionally after the openness hazard's window (§15.48).
17. **The fail-first validity checklist.** A fail-first proof is itself an instrument, proven
    like one: (a) the guard names an entry point an operator reaches, with a bound derived from
    the constant the fix introduced, never a window wide enough to admit the unfixed behavior —
    a guard driving the fix's own extracted helper fails to *compile*, a state no fail-first run
    reaches, so the helper gets a unit test as well, never instead (plan §16's audit judged
    nineteen guards unable to fail); (b) the mutation executes against the unfixed tree and its
    *application is verified* — assert the replacement count; a mutation that matched nothing
    runs the unmutated test and reads exactly like "the guard did not redden" (notes §3.63);
    (c) the specific fix is reverted in place, never stashed around, and every spawned binary
    rebuilt first (rule 9); (d) where a hostile platform exists, the proof runs there — the
    friendly one is the substitution AGENTS §9 forbids (plan §18 item 12); (e) every control
    asserts its own application — a planted offender's count, a positive-control arm,
    `!task.is_finished()` against the completed-in-one-poll vacuity (notes §3.59); (f) a
    planted-defect proof is read at its error *text*, never its exit status alone — two
    different failures can share one exit code, and the near-miss is recorded (notes §3.71);
    (g) a retracted "this cannot be tested" justification is retracted visibly where it stood,
    never silently deleted — a future session inherits the claim otherwise (review 32,
    CONC-3).
18. **Failure signatures are recorded verbatim before any clustering claim** — exact byte counts,
    duration against the deadline it may equal, message as printed. "Same two tests" was
    pattern-matched twice before the exact signature (exactly 64 bytes short, a 60 s deadline
    waited out, one verbatim message) settled the replug-lane flake (notes §3.54, §3.62, §3.70);
    a duration equal to a deadline is a deadline, not a performance figure, and a stall and a
    loss stay unseparated until evidence separates them (AGENTS §6).
19. **Figures carry scope.** A measured figure is quotable only with its named scope, date, and
    commit or artifact; the plan's Status table is the sole restatement point for current figures
    and defines the only scope names a claim may use. Deltas are quoted only where one session
    measured both ends — never re-derived across scopes, eras, or unmeasured commits — and a
    withdrawn figure is never re-quoted (the register lives at §15.44). A quoted ratio is a
    figure twice over: it names its numerator's and denominator's instruments and sample bases,
    or it is not quotable — the fused-figures class recurred three times before this clause
    (notes §3.45 and the v15 landing record).
20. **The alignment pass binds every generation of the document pair, including the one that
    states this rule.** A new generation lands as the prior text plus intended changes only:
    sentence-granular diff against the predecessor, every hunk classified, every "the code still
    does this" claim checked against code, acceptance test "the diff contains nothing unintended"
    — the v12 and v13 generations both regenerated from stale bases and silently dropped rules
    the code still enforced, twice in a row. Each generation's notes entry records its execution
    of the pass; this generation's records the digest fan-out and must-preserve sweep.
21. **Gate spellings have one authority: `.github/workflows/ci.yml`.** AGENTS §3 and this plan
    mirror the lane commands and are corrected *to* CI, never the reverse — the doctor gate's
    `jq -e` was missing from the mirrored line while CI carried it, so the command a human copies
    read as a gate and asserted nothing (without `-e`, jq prints `false` and exits 0 — measured
    against a deliberately gutted report; notes §3.74).
22. **A gate proves its own execution.** The tell for a gate that asserts nothing: its passing
    output is identical to its not-running output. Three recorded instances of the class — `jq`
    without `-e` (notes §3.74); a gate that never ran because the step ahead of it was red, a
    failed step skipping the rest of the job while `--no-fail-fast` buys legibility only
    *inside* `cargo test` (notes §3.77); and a SKIP-line grep whose subject `cargo test`
    captures before it can be grepped (notes §3.78). Three structural consequences: a pipeline
    stage that discards an exit status (`tee`) un-gates the command ahead of it; a gate whose
    subject exists regardless of the test outcome is placed where a red test cannot hide it;
    and every antecedent-gated expectation clause ships with a synthetic-antecedent guard,
    because a passive CI run makes its antecedent false on every push and deletion leaves
    everything green (§13's gate design; notes §3.64, §3.68).

### The required-mode lattice (rule 11's table)

One mechanism, several instances: `required` turns a legitimate self-skip into a hard failure,
so a box that has the fixture proves the tests executed rather than counting green skips as
coverage. The table is the reader's map; the authority is the harness code that reads these
variables (rule 11).

| Variable | Gates what | Skip behavior (unset) | `required` behavior | Platform notes |
|---|---|---|---|---|
| `SNX_CROSSOVER` | Tier-3 crossover-rig tests | Self-skip naming the candidate ports (notes §3.35) | Skip is a failure | On macOS also *enables* the `cu.usbserial` scan — it never fires unasked (notes §3.57) |
| `SNX_REPLUG` | Replug lane: blessed helper + hardware replug tests (§15.45) | Self-skip; message names a remedy valid on the printing platform (notes §3.72) | Skip is a failure | macOS `install` deliberately installs nothing (notes §3.66) |
| `SNX_TLS` | `web_tls_round_trip`'s two silent skip causes (no `curl`; sandbox unable to bind `0.0.0.0:0` with `--tls`) | Self-skip | Both arms must run (notes §3.57) | — |
| `SNX_RIG_FLOW` | The two `rts-cts` end-to-end tests (§15.52) | Skip on a 3-wire bench, printing the measurement | Skip is a failure | Precondition *measured*, not declared: a 3-wire bench is §5's stated assumption, so `SNX_CROSSOVER=required` deliberately does not redden it (notes §3.63) |
| `SNX_WEB_UI` | Playwright browser suite (§15.37) | Self-skip without node | Skip is a failure | Retries stay 0 (rule 15) |
| `SNX_LICENSE_GATE` | Licensing-gate lane | Self-skip | Skip is a failure | — |
| `SNX_EXEC_CODEC` | The out-of-process codec battery (§8, §15.26): thirteen tests over four files that drive a codec child through an external interpreter — `exec-conformance`, the any-language envelope battery, the crash-and-restart guard, the unconfigured-channel counter | Self-skip naming what the provider saw (`python3 not found`) | Skip is a failure (item 49) | Named for the **capability**, not the tool, like every instance beside it — `SNX_TLS`'s absent tool is `curl`, `SNX_WEB_UI`'s is `node`. A fixture ported to another language would leave an `SNX_PYTHON` naming nothing while the battery it gated still existed; the tool's name belongs in the message, on the box printing it (rule 11) |
| `SNX_PACKAGING` | The packaged deployment surface (item 31): `systemd-analyze verify` over the unit and `udevadm verify` over the udev rules, each staged past its environmental arm | Self-skip naming what the provider saw (`systemd-analyze not found on PATH`) | Skip is a failure | Named for the **capability**, not the tool, like every instance beside it. The three text checks in the same file need no tool and **never skip**, so the drift class this tree can cause stays covered where this variable's subjects are absent |
| `SNX_PACKAGING_ROOT` | Item 31's owed measurement: the `/var/lib/private/` indirection under `DynamicUser=`, and the EACCES-versus-EROFS pair proving `ReadWritePaths=` flips the mount without chowning | Self-skip naming the precondition that failed — PID 1's name, the effective uid, or `systemd-run`'s absence | Skip is a failure | **Deliberately set nowhere yet.** Whether a CI runner is systemd-as-PID-1 with passwordless root is unmeasured, and shipping `required` on an assumption reddens a lane for someone else's runner image — the discipline §15.52 set for `SNX_RIG_FLOW`, whose precondition was measured before it was demanded |
| `SNX_SERIAL_PAIR=rig` | Forces `serial_pair_or_rig()` onto hardware (§15.48) — not a required mode | Software wins by default | Forcing with no rig visible is a hard failure, never a silent fallback | Provider printed before transmit |

**Four** further variables are parameters, not gates. `SNX_CROSSOVER_A`/`_B` name the two rig port
paths; after a renumbering replug they may name the swapped adapters — harmless on a symmetric
crossover, and the test announces it (notes §3.54). `SNX_REPLUG_DEV` **and `SNX_REPLUG_DEV_B`**
accept only `/dev/serial/by-id/...` and hard-fail on any other form, because re-enumeration can
renumber `ttyUSBn` — the premise §12 is built on (notes §3.54). `_DEV_B` names the *second*
adapter, and it is a parameter with a gate's consequence: `identity_survives_a_replug_that_
renumbers_the_tty` needs two adapters to force a renumbering, so under `SNX_REPLUG=required` its
absence is not a skip but a failure — which is why the lane spellings below name it. It was
missing from every one of them until 2026-08-12 (notes §3.75), so the documented rig lane could
not go green on the rig it describes.

### Context hygiene (§15.41)

Everything this toolkit emits — reports, verdict lines, skip messages, comments, docs — describes
*capabilities*, never consumers: no assertions about the existence, count, or nature of external
users or any business context. Say "out-of-tree", never `closed-source`, `closed repo`, or
`known repository` — the three phrasings are gate-banned tree-wide, including `docs/historical/`
(the privacy rule outranks the frozen-history rule), and this paragraph is the plan's one allowed
statement of them; the meta-gate in `itest/tests/meta_names.rs` holds each stating file to an
exact count. `downstream` survives only in its data-flow sense and `proprietary` only as a
general capability; both are review-judged rather than gated. When quoting older material that
violates this, paraphrase.

### The citation gate

Every `§3.NN` in code must resolve to a section `docs/implementation-notes.md` actually
defines: `itest/tests/meta_names.rs` walks the tree's `.rs` files and asserts it, and it checks
its own scoping premise rather than asserting it — neither normative document may ever grow a
`### 3.N` heading, or a bare `§3.N` in code becomes ambiguous and the gate reds. Its honest
bound is stated first: it catches a citation that resolves to *nothing* and cannot catch a
wrong-but-real one (thirteen sites once cited a neighbouring entry that exists — notes §3.60);
the discipline that closes the rest is write order — the notes entry lands in the same commit
as the code that cites it. The gate is also a filename-keyed site a generation landing must
bump: the full set is AGENTS §2, README's index, and the *four* code sites — `meta_names.rs`'s
ban-statement allowance and this gate's scoping pair, plus `meta_derive.rs`'s `PLAN`/`DESIGN`
consts (deliberately paths, not discovery, so a stale name fails loudly). A landing that misses
one panics that gate on the moved files, which is the design working.

## 4. Phases

**Status:** EXECUTED — all nine phases (0–8), through release 0.2.0 and the tracks that followed
(plan §9–§17). This section is the record; it binds nothing by itself. A citation of the form
plan §4.N resolves to Phase N below (one pre-rule legacy site, `expectations/linux.jq`'s
`plan §4.3`, named the expectation-file gate — extraction-box item 3, now at plan §3 — not
Phase 3; respelled at the v16 landing, plan §18 item 23 d).

> **The extraction box — live rules this section used to carry, now in body.** In v15 each rule
> below lived only inside an executed phase's text — the single highest silent-drop risk in a
> regeneration — and each now has a named body home. Each rule is restated here in full,
> deliberately — the duplication is the tripwire; the named home is authoritative:
>
> 1. **Stop condition.** A probe contradicting the design is a stop condition: surface the
>    report for a design amendment rather than coding around it — verdict lines are written to
>    make that diff obvious. Now at plan §3's doctor paragraph and §13 (doctor contract
>    clause 4).
> 2. **`skipped(no adapter)` is a valid CI verdict; a failing probe is not.** Now at plan §3's
>    doctor paragraph and plan §2's CI-lanes entry.
> 3. **The expectation-file gate.** One doctor run, gated on its JSON twin:
>    `serial-nexus-doctor --markdown --json-out <path> > <report.md>`, then
>    `jq -e -f expectations/<platform>.jq <path>`; per-platform files encode what each
>    supported system must report; CI archives both renderings so every capability claim has a
>    dated artifact. The `-e` is load-bearing — without it jq exits 0 whatever the clauses say
>    — and the doctor's exit status must reach the lane (rule 22). Now at plan §3 and
>    AGENTS §3.
> 4. **The head-of-line SUM pin.** The phase-6 pin asserts the SUM of the targetward counters,
>    never each: under a fully stalled peer, whichever channel wins the race wedges the shared
>    socket, so the other can legitimately sit at 0 — itself a head-of-line manifestation; a
>    per-channel assertion would have been the wrong pin (`itest/tests/p6_head_of_line.rs`).
>    Now at §5.
> 5. **Never weaken the conformance surfaces.** The §9 six-clause conformance suite and the
>    envelope golden-vector corpus must never be weakened to make a protocol change pass.
>    Now at plan §5.
> 6. **The idle-budget escape hatch.** Exceeding the idle-cost budget selects a §15.18 escape
>    hatch — adaptive idle backoff or `spawn_blocking` readers — as a scheduled task, never a
>    return to epoll. Now at plan §5.
> 7. **The validation blocks' behavioral promises.** Loss located, counted, and isolated;
>    rotation loses nothing; purge counted to the byte; a stale lease timer never fires across
>    grants; steal notifications event-driven; binding never mutates the graph; squatters
>    refused with zero received bytes; crash recovery semantic-diff exact; ENOSPC faults the
>    node while the port's other consumers keep flowing. Owned by §5, §6, §7, §8–§9 and
>    §11–§12, so archiving the phase text drops no promise.

### Phase 0 — Doctor and scaffolding

*Goal:* every kernel-behavior assumption confirmed or corrected, per supported system, by one
tool; the repository enforces its own rules. *Settled:* workspace and CI gates; the license gate
proven by injection (a planted banned crate must fail `cargo deny` — plan §2); the doctor with
P1–P4 and the environment checks (§15.17); the `serial_nexus_rpc` type shapes. *Bought:*
extraction-box items 1–3.

### Phase 1 — Contracts in the small

*Goal:* the load-bearing abstractions as pure, property-tested code before any kernel object.
*Settled:* `serial_nexus_core` and `serial_nexus_codec_api` — types, the three-rule validator
(§4), the config/state split in the type system (§15.8), deliver contracts, the holdover slot,
golden vectors frozen as constants in the test itself, regenerable only as a deliberate edit
with a written rationale in the commit (§8) — plus the sim skeleton. *Bought:* judges
calibrated before they judge.

### Phase 2 — Walking skeleton

*Goal:* real bytes device↔PTY through a configured daemon over RPC. *Settled:* the
current-thread runtime, the §10 socket policy, load-on-empty (§11), the serial node on blocking
serial2 with poll readiness (§15.18), the PTY node, `serial-nexus-ctl --json`; the resolver
landed in phase 7 with no config-format change — identity strings were designed for that
(§12). *Bought:* integration risk paid once.

### Phase 3 — Boundaries and logging

*Goal:* every §5 policy exists, measured, with counters in state. *Settled:* boundary drop
policies with counters, serial discard-when-unattached with TIOCGICOUNT surfacing, the log node
whole (§7.3), `subscribe`, client-termios reconciliation; the benchmark and its budget doctrine
(plan §5, Budgets; extraction-box item 6). *Bought:* loss located, counted, and isolated;
ENOSPC isolation (§5, §7.3).

### Phase 4 — Arbitration

*Goal:* the §6 lock machinery end to end, including its failure etiquette. *Settled:* write
modes on edges, the per-endpoint lock, acquire/release/steal/lease on the §15.20 two-lane
dispatch with its FIFO waiter queue, atomic `send`, purge-on-acquire and purge-on-detach,
`free-for-all`. *Bought:* the 3-a.m. regression pinned — an unlocked writer's buffered
bytes never fire, a detach purges exactly their length — and §6's stale-lease,
steal-notification and FIFO-cancel-safety promises.

### Phase 5 — Codecs

*Goal:* the codec runtime, the registry, both first codecs — the v1 frame format (§9, the link
codec's core) and the exec codec (envelope over stdin/stdout, restart-with-backoff, §7.6).
*Settled:* resynchronization accounted, never approximate (`--corrupt-every` with a computed
expected-loss manifest); the any-language envelope proven by a Python-stdlib passthrough in CI;
crash containment (kill → fault → backoff restart → clean checksums, data plane never wedged);
structural rejection of bad attribute tables. *Bought:* the assets of §8's validation chapter.

### Phase 6 — The wire

*Goal:* two daemons, one reference topology, over loopback. *Settled:* the leg node
(listen/connect, single-peer, loopback-only with `insecure_bind`, reconnect backoff,
purge-on-reconnect — §7.4); the v1 protocol (hello, identity binding into
`bound`/`waiting`/`unbound`, bounded frames — §9); the §9 conformance suite, `wire`-driven,
parameterized over the framing (§15.15's substrate-swap promise kept honest). *Bought:*
binding never mutates the graph; extraction-box item 4.

### Phase 7 — Identity and resilience

*Goal:* the daemon survives the real world — replugs, restarts, crashes, wrong adapters.
*Settled:* the resolver with fallback chain and add-time echo-back, identity-form versus
path-form add semantics, polling reappearance, the reopen ritual, the state file preferred at
startup, `remove-node --cascade`, the serial-signal verbs, doctor P5 before first checklist use,
the whole §12 matrix unprivileged over `--dev-root`. *Bought:* squatters refused
with zero received bytes; crash recovery semantic-diff exact (§11–§12). Tagged 0.1.0.

### Phase 8 — Hardening and release

*Goal:* something other people (and agents) can run. *Settled:* the macOS pass with deltas in
`docs/macos.md`; a 24-hour soak; the documentation set — README's five-minute path, the security
page stating "serial consoles are root shells" in §9's words, the codec-author guide, man-style
RPC pages; systemd unit and packaging; fuzzing of the frame and envelope parsers atop proptest.
*Bought:* the full operator scenario driven purely through `serial-nexus-ctl --json` — §15.16's
feedback loop, scripted. Tagged 0.2.0.

**Release marks.** 0.1.0 at the end of phase 7 (lab-usable on Linux); 0.2.0 at the end of phase
8; 0.3.0 at the rename track (plan §17) — a *minor* bump rather than a patch, because §15.40
breaks the §15.26 extension surface: deliberately, at 0.x, before that surface accumulates pins.

**Continuous track — CLI iteration (live).** Run real tasks through the CLI with humans and
agents, collect friction notes, reshape subcommands freely; §15.16 makes it cheap — tests and
agents pin to RPC, so CLI churn costs one crate's diff and the harness never notices.

## 5. Testing strategy

The pyramid, mapped to this system:

**Unit and property tests (many, pure, in `serial_nexus_core`/`serial_nexus_codec_api`).** The
§5 contracts, graph validation, the lock state machine, purge accounting, resolver identity
parsing, envelope and frame codecs — with proptest generators for graph shapes, chunk sequences,
and interleavings. These encode the design's invariants: a failing property test means the
design or the code is wrong, and plan §1's rule says which document changes.

**Integration tests (some, kernel-facing, Linux CI).** Real PTYs and fixtures via
`serial-nexus-sim` and `--dev-root`; the `serial-nexus-itest` harness boots
`serial-nexus-daemon` in a temp `$XDG_RUNTIME_DIR`, drives it over raw JSON-RPC, and asserts on
state and counters. **The harness is the canonical form of every exit criterion — prose in this
plan is commentary on the harness, not the other way around** (§15.31; §16.11 executed — the
bash-era suite is retired and deleted, `scripts/` holds only `bless`, plan §2; "ported from
`scripts/validate/…`" in a test head is provenance, not a path; wrapper-script disposition:
plan §18). Timing-sensitive assertions use `wait-for` bounds and
`subscribe` events, never bare sleeps (plan §3). One naming convention: itest test files are
prefixed by the work family that created them, assigned in era order (`p0_`–`p8_` the phases,
`p9_*` review 26's guards, `p12_*` the review-32 track's, `p13_*` the rename-track era's); the
prefix is unrelated to doctor probe numbers, which are always spelled with a capital P.

**End-to-end scenarios (few, slow, high confidence).** The two-daemon reference topology; the
crash-recovery sequence; the soak. Nightly rather than per-push.

**Conformance and compatibility suites (contract tests).** The §9 six-clause conformance suite,
driven by `serial-nexus-sim wire` and parameterized over framings; the envelope golden-vector
corpus with the in-CI Python codec as the external consumer. These two suites are the executable
form of §15.15's decoupling and must never be weakened to make a protocol change pass (the
codec-author-facing view is §8's validation chapter).

**Tiered hardware checklist (manual, release-gating).** What fixtures cannot fully prove runs on
the plan §3 tiers — never on a target device: replug during write traffic, squatter refusal, and
exclusivity against a second process (Tier 1); seeded round-trips and RX-overrun counters
through the real driver (Tier 2); baud accuracy across independent clocks, framing and parity
observation, break and modem-line verbs asserted from the far side, and the symmetric null-modem
configuration (Tier 3). Three rules govern every run: the doctor runs first — the one-shot
protocol in `docs/serial-nexus-doctor.md` (plan §3) — and must be clean before any tier item, so
a tier failure is attributable to serial_nexus rather than a loose jumper; the negative control
is physical — pull one wire, re-run P5, confirm the asymmetry is named; and any behavior the sim
structurally cannot exercise either appears here or is marked *unverified* in the doctor's
report, never silently untested (§16.7). The end-to-end companion is the rig lane — the suite
under `SNX_*=required` (plan §3; AGENTS §3); the report attaches to the release notes, the macOS
pass rides the same tiers, and each run recalibrates fixtures, sim and doctor against real
adapters (plan §6).

**Budgets.** The benchmark contract is recorded in `docs/benchmarks/phase3.json`: throughput
headroom of at least 10× over 8 ports at 3 Mbaud, and idle cost asserted under a stated budget.
Exceeding the idle budget selects a §15.18 escape hatch — adaptive idle backoff or
`spawn_blocking` readers — as a scheduled task, never a return to epoll.

**What is deliberately not tested.** Rendering details of `serial-nexus-ctl` beyond `--json`
correctness (§15.16); throughput beyond the documented headroom benchmark (this is a
control-and-observation tool at serial rates, not a data mover).

**Read the loss taxonomy first.** New test authors read §5's loss taxonomy before writing any
completeness assertion: which counters mean loss, which mean purge, which mean spy-drop, and the
`received + dropped_slow_consumer == sent` fingerprint — a shortfall matching it is the lossy
boundary's contract observed, not a data-loss bug (plan §3 rule 3).

## 6. Risks and mitigations

**EXTPROC behaves differently across kernels or not at all.** RESOLVED — the decision point
closed at the end of phase 0 and lives in the decision record (§15.14; platform arms §15.30,
§7.2); kept so the risk list's history stays whole.

**serial2 lacks a needed control.** The `serial_nexus_sys` crate (§16.3) applies the missing
ioctl on serial2's raw fd; the full rustix-termios fallback (§13) exists but is not expected to
be needed. Watch item, not a blocker.

**Test doubles or probes drift from real hardware.** `serial-nexus-sim` behaviors and
`serial-nexus-doctor` verdict logic are written from measured findings, never assumptions, and
every tiered-checklist run re-validates both against real adapters (plan §5); a divergence is
treated like a failing probe — design, doctor, or sim amended before code trusts any of them.

**Single-thread data plane hits a ceiling.** Empirical since phase 3, headroom recorded in-tree
(`docs/benchmarks/phase3.json`); the §5 contract permits sharding whole subgraphs per thread
later without changing node code; nothing in v1 should need it.

**Protocol churn.** Contained by construction: the conformance suite and the envelope corpus
were written before any second framing change could be entertained, and §15.15's two-contract
split means wire changes cannot break external codecs.

**Scope creep via the CLI.** §15.16 channels it: shape changes are free, but new *capability*
requests route through the RPC surface and get a design-section home first.

## 7. Out of scope for v1

Restating §14 as a refusal list so the plan stays honest; §14's register is the authority and
tags each entry with the deferral vocabulary. Each standing refusal names what *did* land near
it, because several glosses decayed once — a reader checking the list against the tree must
find the qualifier, not a contradiction.

- **No configuration diffing** — stands; load-on-empty stands with it (§11; registered at §14).
- **No native termios propagation** — stands; observe-only, with the subscribe-plus-RPC
  experiment path (§14).
- **No TLS or non-loopback legs** — stands *for the leg*. The web console's `--tls` (§15.29,
  §17; `SNX_TLS=required` in the rig lane) is a different surface and discharges nothing here
  (the leg still refuses non-loopback without `insecure_bind`, §7.4).
- **No uevent hotplug** — stands; polling reappearance detection stands with it (§12). The
  privileged replug capability (§15.45) landed since: a *test-lane*
  instrument, not a daemon hotplug mechanism.
- **No IOKit resolver** — stands. The macOS IOUSBLib replug backend (notes §3.66) landed since:
  it cycles a named device for the test lane and resolves nothing; resolution remains the
  `cu.*` interim (§12).
- **No yamux substrate** — stands (§15.12, §15.15); the conformance suite is parameterized over
  framings precisely so this refusal stays cheap to revisit.
- **Replay ring** — GRADUATED, no longer out of scope: it landed through the web-console track
  (plan §11; §15.32), default-on at 64 KiB on every host-facing channel. Listed so the v1
  refusal's history is not silently rewritten.
- **No combiner node** — stands; the one-producer invariant (§4) holds, and the recorded
  shape for a future merge is an explicit framing node (§15.4), never an implicit combiner.
- **No systemd socket activation** — stands (§14).

Each standing refusal has a design-section home when its time comes.

## 8. Start here

Executed bootstrap, retired; its one durable principle — the ability to check the work exists
before the first feature does — lives in plan §1.

## 9. Post-1.0 simplification track

**Status:** EXECUTED in full — seven commits, each adversarially re-audited; the notes carry
the record. §16 holds the per-item substance; the installed rules live in body (§5's tripwire
table, §10, §11). Numbered stubs keep the item numbers citable:

1. **Boundary-supervisor library** (§16.1) — one lifecycle abstraction for serial, exec, leg.
2. **Critical-section cell** (§16.2) — `RefCell` clippy ban; tripwire in §5's table.
3. **`serial_nexus_sys` crate** (§16.3) — the unsafe-confinement tripwire, grep-gated.
4. **Harness hardening** (§16.5) — shared assertion helpers with their own tests.
5. **Nightly full sweep** (§16.5).
6. **State-file fsync** (§16.6) — clause at §11.
7. **Error-code registry** (§16.8) — defined once in `serial_nexus_rpc`; the `docs/rpc` table
   asserted from the registry two ways (§10, §16.14).

Declined, by name, and standing: **readiness unification is rejected** (§16.9 — it moves lock
consultation across threads; cited from §5), and §14's deferrals stand (§16.10).

## 10. Extension track: out-of-tree codecs

**Status:** EXECUTED in full — including the audit's `precheck_codecs` hardening: `load
--replace` validates codec names and attribute schemas before teardown, so a bad table can no
longer destroy a good graph (later generalized into §11's pre-create precheck contract,
notes §3.67). Items 3–5 are living contracts restated in §8's codec-validation chapter.
Recorded ordering: items 1–3 make the Rust path the easy path; 4–5 keep the exec route honest.

1. **Library/binary split plus registry-as-value** — thin binary over the `serial_nexus_daemon`
   library; `Registry::with_builtins().register(...)`; the entry surface is the only public API.
2. **The `info` verb** — daemon/wire/envelope versions plus registered codec names; an
   unknown-codec load error includes the available list.
3. **External-consumer template** — `examples/external-codec/` (workspace-excluded), CI-built
   from the consumer's position on every push: proven per push, not promised (§8, AGENTS §11).
4. **`serial_nexus_codec_api` conformance kit** (`test-support`) — generic suites any `Codec`
   instantiates in its own tests; a broken-by-design toy codec fails each suite. Contract at §8.
5. **Exec conformance harness** — a sim mode driving an external codec child through golden
   vectors plus the behavioral battery (§15.22's deadlock class as a liveness test), standard
   JSON verdict; `docs/codec-authors.md` is the out-of-tree CI entry point (§8).

## 11. Web console track

**Status:** EXECUTED — items 1–6 plus the hardenings the adversarial audit forced; items 7–9,
the §15.32 follow-on, also executed. Live contracts are body text at §17, §5/§15.32, and §10.

1. **The tap** — connection-scoped, read-only, bounded queue with drop counters; taps never
   touch configuration. Contract at §10.
2. **The replay ring** — exact-splice guarantee; explicit empty-replay marker when ring-off.
3. **`serial-nexus-web` scaffold under the three-tier bind policy** (§15.29): loopback+token
   default, `--tls`+token off loopback, `--insecure-bind` the token-mandatory named footgun (§17).
4. **The console UI is presentation** per §15.16 — validation is API-level, never rendering.
5. **Docs and the security page** (`docs/security.md`).
6. **The TLS tier** — `--tls` via rustls; the tree clears §13's licensing gate.
7. **Default rings everywhere** — `replay_ring` default-on at 64 KiB on every host-facing
   channel; `replay_ring = 0` opts out.
8. **Tap stream offsets** — monotonic hostward byte offset on `tap.data`, `from_offset` on
   replay, per-boot `instance` nonce in `info`; a restart changes `instance` and the client
   detects the reset instead of splicing across it (§17).
9. **Browser-side history (OPFS)** — keyed by socket path + endpoint + instance; 16 MiB
   trim-oldest; `persist()` surfaced; memory-only fallback visible; splice logic in a pure JS
   module with CI-run tests (§17).

Audit hardenings (live at §5/§10/§17): spy-outside-the-graph accounting (a ring never
hides `discarded_unattached`), counted `feed_dropped`, the cookie-carried token, the graph-verb
denylist. Dependency posture: tokio-tungstenite post-handshake framing only, rustls+rcgen on
the `ring` backend, hand-rolled HTTP (§15.13).

## 12. Console map track

**Status:** EXECUTED, oracle-validated; the contract is body text at §7.8 (§15.33).

1. **The map node** — ordered `hostward`/`targetward` lists over the picocom vocabulary,
   stateless first-match substitution, bounded expansion, per-rule counters, `held` default
   targetward; an unknown mapping name fails load structurally.
2. **Reference configuration and docs** — the example config carries a mapped console; the web
   console shows the mapped stream with no client-side settings: the stated motivation,
   observed.

## 13. Review-26 remediation track

**Status:** EXECUTED in full.
`docs/historical/27-review-26-remediation-ledger.md` maps all 93 finding ids to dispositions,
deliberate declines included — consult it before re-filing anything from that review, because
silently re-fixing a declined item is its own defect (AGENTS §5). Gates still standing:
structural range checks with proptests, `deny_unknown_fields`, the bridge's
parse-one-request/allowlist screen, capped-and-counted remote-fed collections, `tap.closed`
lifecycle tests, the waiting-verb-in-flight refusal's (`-32006`) four-test battery.

## 14. Graph-editing track

**Status:** EXECUTED — four items (§15.35 is the rationale). The invariant this track's intro
used to carry — per-endpoint-permanent channels/counters/origin slot and the three endpoint
states — is promoted to §5 body; read it there before touching the data plane.

1. **The `ports` verb** — passive resolver enumeration: by-id/sysfs readlinks, never `open(2)`;
   the macOS `cu.*` scan fires only under `SNX_CROSSOVER` (plan §18 item 5). Passivity proven
   twice: `meta_gates::port_enumeration_cannot_open_a_device` and writer-less-FIFO fixtures.
2. **`connect`/`disconnect`** — edge surgery under the same critical-section validation as
   load; `is_config_mutation` includes both verbs, without which a rewiring evaporates on
   restart.
3. **Web graph page** — topology from `dump`, status from `state` (§15.8's split kept in the
   client); `state` does not carry edges and must not, since they are configuration.
4. **Web editor and posture** — the allowlist carries the graph verbs; lifecycle verbs remain
   refused end to end; graph editing is daemon-user capability, the token operator trust.

## 15. Browser-UI automation track

**Status:** EXECUTED in full (§15.37 is the rationale). The record worth carrying: the suite's
first run failed three specs and only one was a browser defect — "two of the three live below
the browser and had been shipping through a green suite" (§15.38 carries the record). The
scaffold rules are plan §3 body now.

1. **The Playwright scaffold** — pinned package plus lockfile, Chromium-only, `retries: 0` on
   purpose (§15.36: a retry turns a mechanism into a mystery); `SNX_WEB_UI=required` turns
   every skip into a failure (plan §3 rule 11); a floor on the specs that passed.
2. **The behavior suites** — launched by the `p8_web_ui.rs` gate. The `@slow` **tag**, not a
   name list, is what the gate excludes — a third slow spec needs no gate edit; the
   daemon-side half of the tap-shed property stays byte-exact in `p8_tap_drops.rs`.
3. **CI wiring** — `web-ui` per push, `web-ui-nightly` (`SNX_UI_SLOW=1`); artifacts on failure.
4. **Checklist reduction** — §16.7's manual items shrink to rendering fidelity and real-rig
   interaction; the OPFS round-trip and `tap.closed` re-anchor moved to CI-verified.

## 16. Review-32 remediation track

**Status:** EXECUTED in full. The review holds 80 confirmed findings plus ten refutations with
"do not re-file" written on each; `docs/historical/33-review-32-remediation-ledger.md` is what
is true now — the frozen review still reads as though nothing is fixed. §15.34's reading held
four-for-four — rules living in prose, not in a type, a cap, a parser, or an owner — plus this
round's addition: three clusters were invisible because a green suite was counted as evidence.
The class-prevention records, with guards in `itest/tests/p12_*.rs` (one file per defect area,
module docs naming the finding ids — put a new review-32 guard there, not in a `p0`–`p8` file):

1. **Both identity directions read one source** (§12, §15.10) — `p12_resolver_identity.rs`.
2. **A claim is released by its owner, at most once** — the `TIOCEXCL` leak bricks the device
   for every unprivileged process (§7.1) — `p12_serial_exclusivity.rs`.
3. **A parked verb suspends itself, never its connection**; `send`'s `timeout_ms` bounds the
   whole operation (§10, §15.20) — `p12_control_streams.rs`, `p12_send_deadline.rs`.
4. **Loss is counted at the boundary that sheds it and named in `state`** — four holes, one
   rule (§5) — `p12_leg_accounting.rs`, `p12_codec_signals.rs`, `p12_log_queue.rs`.
5. **Every number carries a stated maximum at the door it enters by** — one rule, three doors
   (§11, §16.12) — `p12_config_rules.rs`, `p12_pty_setup.rs`, `p12_serial_exclusivity.rs`.
6. **Offsets tell the whole truth or say where they do not** — `baseline + Σgap_before` exact
   (§17, §15.32) — `p12_tap_replay.rs`.
7. **A gate that cannot fail is worse than a missing one, because it is counted as coverage** —
   plant the violation, prove the detector fires, applied to harness code too (plan §3 rules 4
   and 10).
8. **The browser is part of the product** — scrollback capped because throughput decays with
   pane size; a rotated-past ring is marked, never spliced over (§15.32, §17).
9. **The credential is split; the pre-auth surface has its own pool.** SUPERSEDED-IN-PART by
   review 37 (37-WEBS-6): a reserve refuses at accept and so refuses the operator too; the
   shipped mechanism is a disjoint 32-slot pre-auth pool evicting its oldest member. The
   residual that cannot close — a sibling-port page can fetch its own `/ws` — is documented in
   `docs/security.md` (§17, §15.29) — `p12_web_session.rs`, `p12_web_tls.rs`,
   `p12_web_socket_default.rs` (the §10 default the console resolves — notes §3.75).
10. **The first-read documents are part of the deliverable** — the 6.18 claim narrowed to its
    evidence, then closed by measurement, both artifacts committed; superseded in detail by
    notes §3.73's same-source comparison — the conclusions agree.

## 17. Rename track

**Status:** EXECUTED in full (§15.40 is the rationale; §15.41's context scrub as item 4). The
tree arrived **red** — two entry-point meta-gates already failing on `HEAD` after the
generation move left README's index and AGENTS §1 stale; the gates were right, for the fourth
generation running (the record behind AGENTS §2's same-commit rule).

1. **Cargo renames** — AMENDED DURING EXECUTION: "directories unchanged" was unsatisfiable (a
   directory carrying a retired name is itself a gate hit), so directories dropped the old
   vocabulary. The corollary that bit twice: a gate scanning the filesystem spells the
   **directory**, a manifest or a `use` spells the **crate** (plan §2, AGENTS §11).
2. **The retired-names meta-gate** — fail-first proven on the **walker** as well as the
   matcher: a planted file is surfaced, and converted spellings do not fire. Three exemptions,
   each with a reason and a non-vacuity assertion (a dead exemption is deleted, not kept); at
   least 200 files walked. The retired tokens — the pre-rename compound daemon, CLI and
   web-console spellings plus the bare `nexus-*`/`nexus_*` crate tokens — are described here
   and spelled only in the gate's own source: spelling them here would make this document the
   gate's first offender.
3. **Compatibility window** — LIVE until retired: a pre-rename snapshot is **adopted**, the
   next mutation rewrites under the current name with the legacy file left byte-identical;
   exactly one live retired-spelling constant remains (`serial_nexus_rpc::LEGACY_DAEMON_NAME`)
   plus the operator note in `packaging/README.md`. The window is stated as one release. The
   operational surfaces moved with the rename (v15 filed them under this item): every binary's
   `--help`, `info`, packaging metadata, and the web console's provenance footer —
   `serial-nexus-web → serial-nexus-daemon <version>`, filled from `info` on connect and
   asserted by a device-free Playwright spec (the gate's spec floors moved 20→21 and 10→11
   with it) — speak the family names, and the state-file and socket naming conventions
   re-derive from the new binary name.
4. **Context scrub** (§15.41) — the matcher finding: a trailing word boundary silently
   exempted every inflected form, and a hyphenated spelling of one term sat unknown in
   `--help`; folding hyphens to spaces and dropping the trailing boundary surfaced eleven
   further sites. Four files may state the ban, each at an exact count; the two judgment
   terms are reviewed by hand, per sense (AGENTS §10).

## 18. The work ledger

This is the work ledger of record. Its posture is unchanged from the v15 generation that opened
it: no construction track is open — the system is complete, and what remains is the set of named
residuals the record itself produced, plus the item-sized construction work this generation's
alignment pass filed. The plan's Status section is the authority on which items are open; this
section is the authority on what each item means.

**Discipline.** The standing rules of plan §3 apply to every item — fail-first proof with
executed mutations, a pre-registered falsifier for any cross-kernel repair, committed artifacts
for any kernel claim — and the ledger discipline applies to this list itself: a decline recorded
here is not silently re-fixed later (AGENTS §5), and overturning one is a recorded decision
naming new evidence, never an edit. At every rewrite, every clause of every item is dispositioned
explicitly — executed, carried, or re-filed under a new number — never dropped. Item 16 exists
because the v15 pass got this wrong once: item 7's third clause was dispositioned by neither its
executed bracket nor the Status line, and only a later audit caught it.

**Numbering.** Items 1–15 keep their v15 meanings; ledger item numbers are cited from code and
docs and are append-only — an executed item keeps its number as a disposition line, and a number
is never reused. New items append from 16: carried-forward residuals are items 16–31,
construction items are 32–44, and items 45 and 46 were filed 2026-08-12 by the alignment pass that
executed most of 32–44 (notes §3.75, §3.76). Items 47–55 were filed by the v17 revision
(2026-08-12): item 47 is §15.56's construction, and 48–55 the residuals and hardenings its input
digest surfaced. Items 56–58 were filed **and executed** by the v17 alignment pass against the
tree (2026-08-12, notes §3.84) — three defects no prior item covered, numbered because a defect
this record has no number for is one the next review cannot check was fixed.

**Schema.** An open item states, in order: **State** (open / executed / declined; size S/M/L;
the session kind where one is needed — this line is the status a landing commit must flip),
**Evidence** (what is already measured, cited), **Remainder** (exactly what is owed, quoted
verbatim where carried from a prior item), **Refuted** (diagnoses refuted along the way, by
name — AGENTS §9), **Declined** (alternatives evaluated and declined, by name), **Validation**
(how the item proves itself when scheduled). An omitted field is empty, not dropped. Executed
items compress to a one-line disposition citing the notes entry carrying their full record;
their embedded declines and refutations live there and in this section's closing register, never
dropped. Product-surface deferrals use §14's vocabulary (refused-at-load / accepted-and-waiting
/ graduated / exited) and live in §14's register — this ledger does not duplicate it.

### Items 1–15 — the v15 ledger, dispositioned

1. **Prose-truth sweep (S).** Executed 2026-08-05 (notes §3.57). Its embedded rules stay live:
   numbers in non-prose files (`*.jq`, probe strings) are claims (notes §3.36); frozen
   `docs/doctor/` artifacts are never edited; every replacement sentence is greppable;
   unartifacted figures are dropped and their relations kept.
2. **Teardown-ledger completion (M).** Executed 2026-08-05 (notes §3.55; its own shipped
   reconnect-purge race fixed in notes §3.59; `load --replace` carries the figure
   `teardown_with` computes). Two residuals survive it: `exec`'s floor is now item 21; the
   pty's held `pending` payload is a recorded non-fixable residual (closing register).
3. **The two-test rig-lane flake, separated (M).** Executed — root-caused and fixed 2026-08-06
   (notes §3.70; earlier partial pass: notes §3.58): a re-enumerated FT232R eats the first 64
   bytes — one USB bulk packet — of the first traffic that crosses afterwards; measured below
   the daemon, the daemon never lost a byte, and this was never a product defect. Fixed by
   `prime_the_wire_once`; fail-first 9 of 11 unfixed, 0 of 8 fixed. Three diagnoses refuted in
   notes §3.70 (settling time; the custom rate; notes §3.54's "0 of 2" figure, which reproduces
   5 of 5); notes §3.54 had already refuted the test-ordering story; why the adapter drops the
   packet is not established, no root cause claimed (AGENTS §9).
4. **`p4_free_for_all` on Darwin, and the owed Mac measurements.**
   **State:** open in residual form only (M, Mac rig session).
   **Evidence:** the headline failure is root-caused as a harness-fidelity defect and fixed
   (notes §3.47); the four pre-registered Mac checks (notes §3.49's P5 line shape, §3.50's
   `writer_pending_input_bytes` discriminator, §3.38's `p6_hostility` 8-way prediction, one P10
   rung below 128) are answered by the committed `42eac2a`/`acb5162` Darwin captures — per the
   v15 Status line's citation; the v15 item text itself carried no executed bracket. The original
   failure's licensed sentence stays exact: a stall or a loss, not separated (§15.48; the
   separation is item 20).
   **Remainder:** the unbuilt probe shape `docs/macos.md` names — "with the master *actively
   drained* at the daemon's own cadence, how long after the slave's `close(2)` may the reader
   still recover — a number parameterized by delay, never a yes/no against a quiescent reader."
   **Validation:** self-testimony fields (§15.46); presence-never-answer clauses in both
   expectation files; measured on both kernels before any sentence about either.
5. **The macOS `crossover_ports` doctrine (S).** Executed 2026-08-05 (notes §3.57): the
   `cu.usbserial` scan no longer fires unasked and is gated behind `SNX_CROSSOVER` (§12). The
   transplanted doctrine stays live: reported, never auto-selected — two adapters being present
   is not two adapters being cross-wired. Notes §3.35's doctrine *question* stays open as a
   question (closing register).
6. **Notes §3.29's seven latent sibling guards (S/M).** Executed 2026-08-05 (notes §3.56, which
   also measured the last-close reference count the conversion rests on — P13's fourth shape).
   The embedded decline stands: `p12_pty_setup.rs`'s instance is not ported to macOS, where
   `discarded_at_last_close` is structurally 0 "by kernel, not by defect" (closing register).
7. **Rig-capability items (M).** Executed: the doctor half as §15.52 (P5 handshake continuity,
   reported-never-judged, DTR arm as in-probe negative control), the end-to-end half as notes
   §3.63 (a `none` transmitter delivers through a CTS stop in 25 ms; an `rts-cts` one never
   does). Its two remaining clauses are re-filed rather than left smeared: §15.21's checklist
   items — break reception and the parity mismatch — are item 17, and the `by-path` identity
   clause, which the v15 bracket dispositioned by silence, is item 16. The replug work proved
   `usb:` identity only (notes §3.54, §3.70).
8. **Kernel-of-record closure.**
   **State:** open (S, one visit per box — two boxes owed).
   **Evidence:** prepared 2026-08-05 (notes §3.64: P13's fourth shape; a P14 gate-clause defect
   fixed before it could redden the first 6.18 jq run; the one-shot capture protocol in
   `docs/serial-nexus-doctor.md`). Narrowed a third time by notes §3.73's 6.18 field report —
   cell-for-cell identical to 7.0 on a same-source basis — which closes none of the item's parts
   but the cross-wired rig.
   **Remainder:** (a) one 6.18 visit — a HEAD binary, both adapters cross-wired, a `--json-out`
   capture, and `cargo test --workspace --locked --no-fail-fast`; neither the suite nor the jq
   gate has *ever executed* there. (b) A committed doctor artifact from the CI lane's own
   `macos-26-arm64` image — every Darwin sentence in the record is x86_64-rig evidence applied
   to CI by assumption, P13 has never run on that image, and the unexplained 08-03 `p8_map` CI
   red is pinned to "a ≥601 ms reader stall" only under that assumption. Item 15 narrows this
   item and does not close it.
   **Validation:** committed artifacts with their era stated; figures quoted with their scope.
9. **P4's no-udev arm, exercised (S).** Executed 2026-08-05 (notes §3.57; two named guards in
   `itest/tests/expectation_gates.rs`). The mechanism note stays useful: `--dev-root` reroots
   both `/dev` and `/sys`, so the arm fires from a fixture tree with no hardware.
10. **TLS skip-class debt (S).** Executed 2026-08-05 (notes §3.57): `SNX_TLS=required`, the
    third instance of the one `required` mechanism (plan §3 rule 11).
11. **P14 — the maximum-rate search (M).** Executed 2026-08-05 (notes §3.57, §3.61; §15.51):
    committed three-run triples on both kernels (`3d850cf`, `42eac2a`), the era boundary
    recorded in `docs/doctor/README.md` — a new probe id moves `probe_set`, and the README row
    says so instead of diffing across it. Two in-item defects found 2026-08-07 and fixed
    (duplicate observation keys; a gate clause admitting a `supported` ceiling that measured
    nothing — notes §3.73): a repair within a shipped item, not a reopening of it. Its embedded
    rules — `structural-cap` is the instrument's limit, never the wire's; `platform-refused`
    never reads as a wire fact; presence-never-answer over the three-way verdict, with the
    item's own recorded in-place correction — live at §15.51's entry.
12. **The anti-spin guard is a proxy in space, and the platform it protects is the one it
    skips** — **EXECUTED 2026-08-13** (notes §3.96), **and its central prediction is
    REFUTED**, which is the more valuable half.
    **The design question the item required first was answered by measurement, not by
    argument.** `p3_idle_cost` stated in-tree that "there is no portable analogue" to
    `/proc/<pid>/stat`. That was true of `/proc` and false of the *question*: Darwin answers
    it with `proc_pid_rusage`, whose `ri_user_time`/`ri_system_time` are already nanoseconds.
    So the deliverable is `serial_nexus_sys::process_cpu_nanos` — one function, two arms,
    `unsafe` where §16.3 puts it — with nanoseconds as the shared unit because the Linux tick
    converts up exactly and rounding the finer answer down to make the kernels look alike
    would discard resolution neither imposes. `getrusage(RUSAGE_SELF)` and `RUSAGE_CHILDREN`
    were both evaluated and rejected first: every caller samples a *child that is still
    running*, which `RUSAGE_CHILDREN` reports as zero.
    **`p9_pty_collapse`'s guard is ungated and runs on Darwin**, reading 1.7–1.9 % of a core
    over its 2 s window against a 10 % ceiling (the same ceiling, converted, not re-chosen).
    **The refutation, pre-registered by the item and taken on the platform it named.** The
    item's case was that "a regression widening `pty.rs`'s last-close predicate or deleting
    the latch drain would burn a core and release operator-held write locks on macOS with the
    suite green". The ungated `|| closed` arm was planted on Darwin, the workspace rebuilt,
    and the binary confirmed current: **planted 1.81/1.88/1.81 %, unplanted 1.87/1.88/1.81 %
    — the bands are identical and the plant moves nothing.** So the plant does not spin on
    *either* kernel, and the guard's own comment (which recorded the same result on Linux) was
    describing a general property rather than a Linux quirk. The reader's backoff is not
    defeated by the handler re-firing: the extra work per pass is small and the cadence still
    relaxes to `IDLE_POLL`. **Recorded as refuted rather than quietly dropped** (AGENTS §9) —
    what the port buys is not the hazard the item predicted but the removal of a guard that
    asserted *nothing* off Linux, plus the same instrument for two other files.
    *Owed, and it is a real remainder rather than a formality:* the property this guard pins
    is still guarded on both kernels only against regressions that actually raise CPU, and no
    plant found on either kernel raises it. What bars the ungated arm remains AGENTS invariant
    16 rule (3) — the collapsed-session write-lock leak — which `p9`'s other two tests assert
    directly and which is the reason the latch stays. The superseded filing follows.
    **State:** open — a design question first, then the port (S/M).
    **Evidence:** `p9_pty_collapse.rs`'s CPU assertion is a bare self-skip off Linux ("needs
    `/proc/<pid>/stat`"), and on Linux the hazard it guards cannot occur — P6 reads
    `pollin_passes: 0` with `read_outcomes {EIO: 64}` there against Darwin's 64 of 64
    `POLLIN|POLLHUP` passes, on x86_64 and arm64 alike — and the guard's own comment concedes
    that planting the ungated `|| closed` arm did not move the Linux number. A regression
    widening `pty.rs`'s last-close predicate or deleting the latch drain would burn a core and
    release operator-held
    write locks on macOS with the suite green (AGENTS §9's proxy-in-space, at its sharpest in
    the tree).
    **Remainder:** filed rather than patched, because the obvious fix is blocked by a recorded
    claim: porting the guard needs a portable CPU-time source, and `p3_idle_cost.rs` states
    in-tree that "there is no portable analogue" — so the first deliverable is a design decision
    (`getrusage(RUSAGE_SELF)`? a coarse wall-clock-versus-work ratio?), not a patch (AGENTS §5).
    **Validation:** fail-first proven **on Darwin**, by planting the ungated arm and watching
    the guard redden there — proving it on Linux is the very substitution this item exists to
    remove.
13. **Idle CPU on any Mac is unmeasured, and the projection is arithmetic** — **MEASURED
    2026-08-13** (notes §3.96) on the x86_64 Mac, taken exactly the way item 12's guard takes
    it, since they now share one instrument; **the gating half is carried**, and that split is
    the item's own finding rather than a shortfall.
    **The number, on the box the item names:** 3.72 % of a core at 8 idle tty fds and
    **9.91 %** at 32, each over a 10 s window in one daemon process, giving a marginal
    **0.2578 %/fd** against the artifact's recorded 0.0728 %/fd — **3.5×**, which is the real
    `kevent` Darwin pays per pass and not a regression. Box at load 1.41.
    **The item's own projection was pessimistic by about a factor of two and is corrected
    here:** it estimated "17–19 % on the Intel box" for 32 fds from P12's per-pass arithmetic;
    the measurement reads 9.91 %. That is why the item asked for a measurement rather than
    letting the projection stand.
    **What it does *not* license, and this is why the guard stays gated off Linux.** Assertion
    (1), the 20 % recorded budget, **passes on Darwin with room**. Assertion (2) — item 46's
    marginal drift tripwire — fails, because it multiplies `per_fd_cpu_percent`, a figure
    measured on another kernel. Ungating on that basis would assert a **Linux ceiling on
    Darwin**, which is a fresh proxy in space pointing the other way: item 12 exists to remove
    that shape, not to relocate it. `p3_idle_cost`'s skip message and header now name this
    measured reason instead of the retired "/proc" one, so the next reader meets the real
    blocker. *Remainder:* a Darwin `per_fd_cpu_percent` (and baseline) row in
    `docs/benchmarks/phase3.json` with its own provenance block — box, date, commit, both
    sweeps — after which assertion (2) can be derived per platform and the gate lifted. One
    box's reading is not an artifact figure; item 46's own history is the argument for that.
    The superseded filing follows.
    **State:** open (S) — scheduled together with item 12 or not at all.
    **Evidence:** not a defect and not a measurement. P12's tight window costs 21.4 µs/pass on
    the M4 (the notes §3.72 report) and 23.1–26.6 µs on the x86_64 Mac against 0.72–0.77 µs on
    Linux (committed captures), with a real `kevent` paid per pass on macOS where Linux's
    `took_edge` is `false`. Projecting the
    phase-3 criterion's 32 idle fds gives roughly 4–15% of a core on the M4 and 17–19% on the
    Intel box — inside the 20% recorded budget, above the 3.5% drift ceiling, and nothing in the
    gate set would notice either way. Predates the M4 report; not an arm64 finding.
    **Remainder:** one measured number per Mac, taken the way item 12's guard would take it —
    which is why the two items share a schedule.
14. **`flow_control = "xon-xoff"` has no pre-check and no probe** — **EXECUTED 2026-08-13**
    (notes §3.89), as the item specified: a P15 *observation*, not a second probe — same open,
    same restore, one more flag pair. It is faithful to what the daemon does rather than to what
    is easy to read: `serial2`'s `XonXoff` sets `IXON|IXOFF` in `c_iflag` and clears `CRTSCTS`,
    and its `matches_requested` compares the **whole** `c_iflag` word, so the shipped cell
    `serial2_readback_would_fault` asserts the property the product promises, with
    `ixon_on_readback`/`ixoff_on_readback` as the human-readable half.
    **The answer, which is the item's whole point:** `ftdi_sio` on Linux 7.0.0-29 **honours** it —
    `c_iflag` `0x5` → `0x1405`, a delta of exactly `IXON|IXOFF`, on both ports,
    `serial2_readback_would_fault: false`. First artifact on any kernel for a question filed as
    *unmeasured, not known-good*. **§15.53 stays un-extended**, exactly as the item's *Declined*
    line requires: one driver on one kernel is not a dropping driver found.
    *Two deliberate non-moves:* the verdict does not move on a software drop (P15's `question`
    names `CRTSCTS`, so its verdict answers for that; a drop is loud in the consequence and in the
    cells and refuses nothing), and `probe_set` does not move. The one route by which the software
    pass *does* reach the verdict is `baseline_restored`, now covering **both** flag words — a
    restore check reading `c_cflag` alone would have certified a port left with `IXON` asserted.
    Corroborated from outside the probe: `stty` on both ports after a full Tier-3 run reads
    `-crtscts -ixoff -ixon -ixany`.
    **The owed Darwin arm was measured 2026-08-13 (notes §3.93), and it met the decline's stated
    condition.** On the *same two adapters and the same cable*, one kernel away: both ports read
    `tcsetattr_ok: true` with `tcsetattr_error: null`, `c_iflag` `0x0` → `0x0` — **a delta of
    nothing** — `ixon_on_readback: false`, `ixoff_on_readback: false`, `silently_dropped: true`,
    **`serial2_readback_would_fault: true`**, `baseline_restored: true`; reproduced **6 of 6**
    across three runs and two ports, against `ftdi_sio`'s `0x5` → `0x1405` on that same hardware.
    **So `IOSerialFamily` drops `IXON`/`IXOFF` exactly as it drops `CRTSCTS`, and a dropping
    driver is found.** The *Declined* line below reads "the refusal follows only if a dropping
    driver is found"; that is a **conditional** decline whose condition was named in advance and
    is now measured true, so extending §15.53 is the recorded decision the decline asked for and
    **not** a silent re-fix of a declined item (AGENTS §5). The extension is filed as **item 67**
    rather than executed inside this item, whose scope was the measurement. What the decline
    still refuses is unchanged and worth keeping: a refusal on a port nobody has measured. The
    line "**§15.53 stays un-extended**, exactly as the item's *Declined* line requires: one driver
    on one kernel is not a dropping driver found" was true when written and is superseded by the
    second kernel, not by an argument. *Owed:* the Darwin arm — `IOSerialFamily` accepts-then-drops
    `CRTSCTS` on this same rig, and whether it does the same to `IXON`/`IXOFF` is unmeasured,
    which is precisely why the decline stands. The superseded filing follows.
    **State:** open (S).
    **Evidence:** §15.53 refuses an `rts-cts` config whose driver drops `CRTSCTS`, and P15
    measures it. Software flow control gets neither, and `serial2` verifies `c_iflag` by
    read-back exactly as `c_cflag` — a driver that accepted `IXON`/`IXOFF` and reported them
    clear would fault the node with the same bare `failed to apply some or all settings` the
    refusal exists to prevent.
    **Unmeasured, not known-good:** no artifact on either kernel says whether any shipped driver
    does this, and the honest statement is that nobody has asked.
    **Declined:** the refusal follows only if a dropping driver is found — extending §15.53 to a
    mode nobody has measured would be policy without evidence.
    **Remainder:** the cheap first step is a P15 observation, not a second probe — same open,
    same restore, one more flag pair.
    **Validation:** both arms tested; presence-never-answer; the `field_set` move announced
    (§15.44).
15. **The arm64 / Darwin 27 artifact is owed, and the hardware replug test is not portable.**
    **State:** open (S/M).
    **Evidence:** the M4 report (notes §3.72) is the first evidence from either arm64 or
    Darwin 27, and it arrived as pasted Markdown — no `--json` capture is committed, so every
    figure there is quoted from a report rather than diffable against one. This narrows item 8
    and does not close it: Darwin 27 is not the `macos-26-arm64` image, and what Darwin version
    that runner carries is itself evidence-by-assumption.
    **Remainder:** (a) a `--json-out` capture per the one-shot protocol, committed at the
    current era. (b) The `p7_replug_hardware` cross-platform contract decision: a
    `/dev/serial/by-id/…` path on Linux against a USB serial number on macOS — a change to a
    test's contract, deliberately deferred (notes §3.66) and not to be resolved silently.
    (c) No line of `sys/src/usb_macos.rs` has ever run on arm64 — the vtable self-check proves
    slot 13, not the destructive slot 37 a real `cycle` exercises.
    **Validation:** committed M4 artifact with its era stated; the contract decision recorded
    before any port.

### Items 16–31 — carried-forward residuals

Each item below uses the schema with its fields inline.

16. **`by-path` topology identity against a real cycle** — **open** (S, rig session). Split out
    of item 7. *Evidence:* the replug work proved `usb:` identity across a renumbering replug
    (notes §3.54, §3.70); `by-path` — the one §12 fallback the replug lane could prove directly,
    the sysfs port name denoting the port and surviving the cycle — never has been.
    *Validation:* the fail-first control §12's founding measurement used — the reauthorization
    order returning the adapter to the minor it already held is refused by the guard.
17. **§15.21's checklist items: break reception and the parity mismatch** — **open** (M, rig
    session). Item 7's named remainder, explicitly separate from it. *Evidence:* §15.21 records
    both as checklist items, not losses; no tier transmits a break into an open peer; and the
    rig was proven 5-wire when this was filed (notes §3.53) — but the 2026-08-12 bench measures
    **3-wire** (notes §3.80; §15.52's re-measure annotation), so the item now begins with a
    bench re-inspection: re-cable or re-verify before any break-reception attempt, the
    feasibility claim conditional on what P5 measures that day. **That re-inspection ran
    2026-08-14 and the bench measures 5-wire** (notes §3.102, §15.52's re-cable annotation), so the
    entry clause is discharged and the conditional feasibility claim resolves in favour of
    proceeding. **Recorded so the next session does not over-read it:** neither clause was ever
    *electrically* gated on the handshake wires — break reception and a parity mismatch both ride
    TX/RX, which the 3-wire bench also carried — so what the re-cable bought this item is its own
    stated entry condition, not a new physical capability. The work is unchanged in size and is
    still owed. *Validation:* rig-gated with a
    `required` spelling (plan §3 rule 11); reported-never-judged where wiring may legitimately
    be absent (§15.52's pattern).
18. **The macOS capture and suite run owed at the current tree** — **EXECUTED 2026-08-13**
    (notes §3.93), both halves. *The capture half, taken this session:* six committed artifacts
    from one clean build on the x86_64 Mac rig box — a passive triple and a Tier-3 triple,
    `b3461886e27a`, era `4317ea5ac187f506`, with `jq -e -f expectations/macos.jq` **executed**
    against all six and exit 0 on every one. The era row in `docs/doctor/README.md` is updated and
    the scope is quoted with its exclusions. **The rig was proven on the wire before it was used**
    (`serial_hardware.rs` 6 passed, 32768 bytes byte-exact each way at 250000 baud), which is the
    protocol §3.45 set and the reason a capture from this box is evidence rather than output.
    **The capture is worth more than the item asked for, for a reason the item could not have
    known:** it was taken on the *same two adapters and the same cable* as the current Linux rows,
    so this is the first Darwin/Linux pair in the record where `probe_set`, adapters, cable and
    wiring are all held fixed and only the kernel differs. Three items are answered off it —
    26's pre-registered reading, 22's remaining kernel, and 14's owed Darwin arm — and two of
    those answers file new work (items 66 and 67). *One thing the item's debt cannot buy any
    more, recorded rather than tidied:* `e79f5fcd86a2e5f0` closed with no Darwin capture ever
    taken in it, so **that** era's macOS half is unobtainable rather than pending. The superseded
    filing follows. *Suite half executed 2026-08-12* (notes §3.76): CI's `macos` job
    ran the **whole workspace** at this tree and read 859 passed · 1 failed · 6 ignored (860 · 0 · 6
    on the re-run once the guard below was repaired), with the one failure being a guard that asserted Linux's EXTPROC retention on both
    kernels and is repaired in the same commit. The figure and its scope are in the Status table.
    *Remainder:* the committed doctor artifact. Both doctor jobs now upload the JSON twin beside
    the Markdown, so the capture exists to be taken — but a CI by-product is not a committed
    artifact: §16.13 wants a deliberate capture with its era stated. *Evidence:* the current
    `probe_set` era still has no macOS triple. Distinct from item 15's arm64 box and item 8's
    `macos-26-arm64` image — three machines, none substituting for another; the run above is the
    CI runner, which is item 8's machine and not item 15's. *Validation:* the documented scope
    quoted with its exclusions; the era row in `docs/doctor/README.md` updated.
19. **P10's drain ladder: both kernels' bounds** — **EXECUTED in its Darwin half 2026-08-13**
    (§15.60; notes §3.93); **the Linux half stays open** (S, one rig visit).
    **The item asked for a rung that already shipped.** The pre-registered next step was "a rung
    below 128"; the ladder is `[512, 1, 128, 900]` and the `1` rung — the *strongest* member of
    that family — landed in `f8315cc`, the same commit whose notes §3.52 pre-registered the step.
    So no code was written: the answer was read off committed artifacts, and the transferable
    finding is the stale pre-registration itself (§15.60 carries the rule).
    **The answer, which is what the item was for:** on Darwin `topped_up_bytes` **equals**
    `drained_bytes` at every rung — 512→512, 1→1, 128→128, 900→900 — with the from-empty rung
    republishing the whole depth (1024 targetward, 1022 hostward), `rungs_refusing: 0`,
    `watermark_threshold_le: null`, in both directions of all three runs. The `D = 1` rung is the
    discriminator: a watermark republishing only below threshold `T` predicts that draining **one
    byte** from a full queue republishes **nothing**, for every `T ≤ C − 1`. One byte drained, one
    byte republished. **Every watermark below capacity is refuted and the survivor is the capacity
    reading**; the empty→nonempty reservation is refuted from the other end by the from-empty
    rung's zero shortfall. What is *not* settled is the **mechanism** — the two-queue XNU source
    read stays a hypothesis under test (§7; notes §3.42), a capacity reading being consistent with
    it and not evidence for it. *One Linux observation recorded rather than folded in:* this
    item's own *Evidence* line says the Linux top-up is "measured drain-size independent", which
    is what the current-era triple's runs 2 and 3 say (9728 at every rung) and not what run 1 says
    (1536 at the first rung, 2560 at the rest, with an anomalous first-rung refill). The deviation
    tracks the **first rung**, not the drain size — a warm-up shape. Filed at §15.60 as an open
    observation for the next Linux session, **not** as a falsification: one run of a quantity known
    to vary is not a refutation. Neither digest moved and no era row is owed. The superseded
    filing follows. *Original state:* **open** (S, one rig visit per kernel).
    *Evidence:* on Linux the ladder is bounded by its largest rung — 900 bytes against a
    ~15360-byte capacity — so no watermark bound is recoverable there, and the top-up is
    measured drain-size independent, refuting the ladder's own "writable iff occupancy < T"
    model on that kernel (notes §3.53; the above-capacity-rung question left open with its
    reasoning, notes §3.64). On Darwin the 1024/1022 asymmetry has a measurement but not a
    decisive one; the pre-registered next step is a rung below 128 (notes §3.52), and the rung
    discriminates two *named* readings of the pair — the XNU source-read hypothesis (two
    different queues: a `TTYHOG`-guarded input bound and a `TTYCLSIZE`-sized output queue, a
    capacity reading) against a watermark/reservation reading; the source read is a hypothesis
    under test, never established prose (§7; notes §3.42, §3.45 A). *Validation:*
    pre-registered falsifiers per kernel; committed fields keep their meanings (512 stays the
    first rung).
20. **`p4_free_for_all` on Darwin: stall against loss, separated** — **EXECUTED 2026-08-13**
    (notes §3.97) by the pre-registered experiment rather than by the new instrument the item
    asked for. **notes §3.56 wrote three outcomes down before the run and the first one
    fired.** Its "Held" arm reads: *"the test passes on Darwin over the rig, repeatedly. Then
    the loss was the writers' last close destroying the tail of a payload the kernel had
    accepted but the daemon had not yet read, which is §3.29's class exactly, and the
    5–31-byte magnitude fits a 1024-byte Darwin pty buffer draining at 11520 B/s against a
    ~90 ms residual."* Measured on the Mac rig at `535594c`, box at load 1.39–1.62:
    **12 passed, 0 failed of 12**, the first run finishing in **5.11 s** — which is the
    record's own "a healthy run finishes in 5 s". Against the frozen 12 of 12 *failing*, that
    is a symmetric reversal of the same sample size.
    **So the licensed sentence resolves, and the item's stall-or-loss question with it: it was
    a loss**, of §3.29's class, fixed by the witness conversion §3.56 landed and explicitly
    could not test ("nothing here was tested on Darwin"). This session is the Darwin run that
    entry was waiting for. **The separation instrument item 20 filed is not owed** — the
    pre-registration answered the question a new instrument was to have asked, which is what
    pre-registration is for.
    *Corroborated from the same session's capture, not from argument:* the prediction names a
    1024-byte Darwin pty buffer, and P13/P10 on this box read exactly that — a from-empty
    depth of 1024 targetward and 1022 hostward, with shape `a` losing 64 of 64 to a
    `waits-then-discards` close. The magnitude the prediction had to assume is now measured on
    the same hardware in the same session.
    **The confound is named rather than allowed to strengthen this.** The 12-of-12 failure was
    taken on the `BH00L4KU` ↔ `BH00LL8O` pair, 5-wire; this run is `ABSCDGL6` ↔ `BH00L4KU`,
    3-wire, and the tree moved a long way between them. So "the conversion fixed it" is the
    outcome the pre-registration licenses, **not** a conclusion this run could have reached on
    its own — a different adapter is a live alternative that only a re-test on the original
    pair could exclude, and that pair is no longer assembled. Recorded as a bound on the
    closure, not as a reason to withhold it. The fourth outcome §3.56 named (the test failing
    on *Linux* over the rig, which would be evidence against the conversion) has not been
    observed. The superseded filing follows. **Original state:** open (M; shares
    item 4's session). *Evidence:* the frozen record — 12 of 12 failing at the committed 30 s
    deadline losing 5–31 bytes of 32768, every failing observation `timed_out: true`, a healthy
    run finishing in 5 s, against 20 of 20 passing on Linux over the same wire at the same
    commit. Mechanism not established; the harness-fidelity fix (notes §3.47) repaired the
    instrument, not the record's sentence. *Remainder:* the licensed sentence stays exact until
    a separation instrument measures it — a stall or a loss, not separated (§15.48).
    *Validation:* the separation instrument first, on notes §3.62's locate-the-gap template;
    the wording never shortened to "drops bytes". The next rig run executes notes §3.56's
    pre-registered outcome table as written — held / fired / half-met, each with its named next
    step — including the fourth outcome: this test failing on *Linux* over the rig is evidence
    against the conversion itself (the witness fd perturbing the graph — it keeps
    `client_present` true and suppresses the detach purge), with 20-of-20 the baseline.
21. **`exec`'s teardown figure: floor to total** — **EXECUTED 2026-08-12** (notes §3.86). The
    inventory came before the change, and it corrected the filing in both directions. What an exec
    node holds targetward at teardown: (a) the per-channel host-facing receivers plus each
    forwarder's in-flight chunk — **already counted**; (b) the internal merged queue `src_rx`,
    moved by value into `pump_child`, which `TaskSet::abort_all` drops unread — **uncounted**,
    notes §3.31's original defect surviving one stage further in than the fix reached; (c) the
    chunk `pump_child` is mid-`write_all` on — uncounted, no `held` slot existing for a bare
    receiver; (d) bytes already inside the child's stdin pipe — **structurally unreachable and
    argued not to be loss**: the exec codec's obligation ends at the child's stdin exactly as a
    serial node's ends at the device fd, which is the line the serial adoption already draws.
    **The note the item rests on gets one thing wrong, and it matters:** the merge queue is
    *bidirectional* — `mux_inbox` is a `target_inbox` entry whose forwarder tags chunks with the
    reserved MUX identity — so roughly half of it is the raw device stream on its way *into* the
    child, and notes §3.55's "the merge queue needs only a watch" sketch would have charged
    hostward bytes to a targetward ledger. *Fail-first:* two reverts by notes §3.55's
    disjoint-reddening method, each against rebuilt binaries, each reddening exactly the two new
    guards (`discarded_at_teardown: 0` where 44000 is owed) and no others. A new
    `tests/ext-codec/deaf.py` fixture supplies the shape the item named — a child that has stopped
    reading stdin, so the merge stage holds bytes at teardown. `daemon/src/nodes/mod.rs`'s
    `discarded_at_teardown` doc, which asserted "`exec`'s answer is a floor; every other kind's is
    exact", is corrected in the same change: it became a shipped falsehood the instant the fix
    landed. The superseded filing follows. Item 2's recorded residual,
    now scheduled. *Evidence:* `exec`'s `discarded_at_teardown` is a floor because its internal
    merge stage is not reached (notes §3.55; named where the counter is documented, §15.50).
    *Validation:* a guard whose child has stopped reading stdin, so the merge stage holds bytes
    at teardown; fail-first per notes §3.55's disjoint-reddening method.
22. **P13's missing shape: a reader arriving during the close-wait** — **EXECUTED 2026-08-13**
    (notes §3.89 for the shape, §3.93 for the second kernel). **The Darwin arm is the one the
    shape was built for, and it separates the two readings the item named.** On the kernel that
    `waits-then-discards`, shape `a` — no reader — pays **600368 µs** and loses all 64 bytes,
    while a reader arriving *inside* that window ends it: `close_microseconds` **14**,
    `arrived_before_close_returned: true`, `bytes_recovered_by_arriving_reader` 64 of 64,
    identical across all three runs. **So the ~600 ms is what a reader that never arrives costs,
    not a floor the close pays regardless.** That bears directly on the reader-stall hypothesis
    for the macOS red (notes §3.29, item 8's `p8_map` pin): a stall long enough to matter has to
    be a reader that does not arrive at all, not one that arrives late — the arrival collapses
    the close by a factor of ~43000. The Linux half stays what it was, **a control proving itself
    inert** on a `retains` kernel, which is what made it trustworthy as an instrument here. The
    superseded filing follows: the shape exists and is measured on **Linux only**; "measured on
    both kernels" is the remainder. `e_reader_arrives_during_close_wait` uses a reader thread
    **already spinning** on an `AtomicBool` when the close is entered — not spawned, not sleeping,
    either of which would be structurally unable to land inside Linux's µs-wide window and would
    have been §13's vacuity taxonomy 2. Both timestamps come off one `Instant` epoch, and the row
    states its own applicability (`arrived_before_close_returned`; `reading` as
    `arrived-inside-the-close-window` / `lost-the-race`; `does_not_license`).
    *Linux 7.0.0-29, 3 of 3:* the arriving reader wins — first `read(2)` at 0–1 µs against a close
    returning in 3–7 µs — recovering 64 of 64, terminal `EIO`. **Here it is a control proving
    itself inert**, this kernel being `retains`; Darwin (`waits-then-discards`, 600104 µs) is where
    it becomes the measurement, and where it answers what notes §3.29 could only predict: does the
    arrival end `ttywait` with the bytes recovered, or does the close still pay its full timeout —
    the reader-stall hypothesis for the macOS red.
    *Two defects caught in the probe's own construction and fixed before landing:* a
    `bytes_lost: 64` printed beside a reader that recovered all 64, and a cell named
    `bytes_recovered_during_close` that would have been **false** on a lost race (renamed). A
    panicked reader thread would also have published `arrived: true` with 0 bytes; the join
    fallback is now `u64::MAX`, so that failure cannot read as a clean measurement.
    The superseded filing follows. **Open** (S).
    *Evidence:* no P13 shape covers a reader that arrives *while* the kernel is inside its
    close-wait — the shape the failing macOS test inhabits (notes §3.29); the committed shapes
    all fix the reader's state before the close. *Validation:* measured on both kernels;
    presence-never-answer; the `field_set` move announced.
23. **Prose and figure corrections, verified first** — **executed 2026-08-12** (notes §3.75).
    Its own instruction — check each sub-item against the current tree before editing — is what
    closed most of it: (a) of notes §3.49's three "TIOCGICOUNT, which is Linux-only" sites, two
    were **already discharged**, both quoting the old wording as superseded history
    (`doctor/src/probes.rs`'s `p5_verdict` arm and constant; `docs/serial-nexus-doctor.md`'s
    bullet); the third, `docs/macos.md`'s "P3 / P5 degrade where `TIOCGICOUNT` is absent", was
    corrected — §15.47 widened the predicate to `TIOCMGET || TIOCGICOUNT`, so P5 degrades on two
    certificate *items*, not on characterization. (b) **Already discharged**: P10's shipped
    consequence string states the raw/cooked relation without figures, and the withdrawal of the
    "~13.8 KiB raw / ~23.5 KiB cooked" pair is recorded where the mode is measured. (c)
    Re-derived once on the current tree: **106 `Running` lines + 8 `Doc-tests` = 114 cargo
    targets**, against 116 `test result:` lines — the two prior records disagreed because they
    were taken at different eras (104 at the 767 era, 106 at the 852 one), not because either was
    wrong; the figure lands in the Status table and nowhere else. (d) Executed at the v16 landing:
    `itest/tests/p8_web.rs:952`'s bare `(§14.3)` and `expectations/linux.jq:1`'s `(plan §4.3)`,
    both respelled.
24. **Leash coverage for `Sim` and raw daemon spawn sites** — **EXECUTED 2026-08-13** (notes
    §3.91), through item 50's shared helper as the item intended. `RawDaemon` is leashed by
    default with `.unleashed()` as the escape hatch; `Sim::spawn` gained `--exit-on-stdin-eof`; and
    `p13_legacy_defaults`'s bare spawn — which cannot use `RawDaemon`, its subject being what the
    daemon derives with **no** `--socket`/`--state-file` — is covered directly. §15.43's opt-in
    semantics are kept on both sides: `Sim::client` deliberately does **not** pass the flag, since
    `Command::output()` gives a null stdin and therefore EOF at instant zero, and the sim
    **refuses** the flag in `transcript` mode, whose stdin is the daemon's envelope pipe. The
    notes §3.39 orphan's trigger is not speculated about anywhere. *Fail-first:* the flag removed
    from `RawDaemon`, and again from `Sim`, each leaving a process that outlived a SIGKILLed
    parent and named it; plus the inverse — an *unleashed* double that died with its parent, the
    leash firing without being opted into. A live pre-change specimen was on the box when the work
    started: an orphaned daemon, 55 minutes old, no leash in its argv. *Evidence:* the
    stdin-EOF leash (§15.43) exists only via `Daemon::start`; `Sim` and the raw spawn sites are
    uncovered, and the notes §3.39 orphan's trigger is still unestablished. *Validation:*
    coverage keeps §15.43's opt-in semantics; the trigger question stays a question unless a
    reproduction answers it.
25. **The sim's `--hold-ms` timer retired for a caller-owned hold** — **EXECUTED 2026-08-13**
    (notes §3.91). `sim client --hold-stdin-eof`, `conflicts_with` `--hold-ms`, sharing one stdin
    watch with the leash so nothing races for the pipe. The timer is **gone, not defaulted**, at
    every background holder — `p4_exclusivity` (2), `p5_held`, `p7_unplug`, `p4_waiting`,
    `data_path` (2). *Scoped, with the reason recorded:* the two surviving `client --hold-ms` uses
    are synchronous `Sim::client` runs where the caller is blocked on the process, so a
    caller-owned hold is structurally impossible; `pty --hold-ms` and `wire --hold-ms` mean
    something else entirely (the device stays plugged in / the peer stays connected) and were
    outside the item's evidence.
    **A recorded fail-first was refuted here, and that is the item's best argument.** Re-running
    notes §3.56's plant — the hold shortened to a timer — against the new mechanism stayed
    **green at both 1000 ms and 300 ms, four runs**, on this 20-core box at load 1.2–1.9 with the
    test's wall time 0.12 s. A shortened-timer plant is not a reliable instrument here; that it
    passes on a fast box and fails on a slow one is precisely what made the old hold a proxy in
    time. The deterministic replacement — the hold made a no-op — reddens. *Evidence:*
    `p4_exclusivity` still holds by timer; the replug helper already proved the caller-owned
    stdin-EOF hold shape — the caller owns the hold length and samples unprivileged (§15.45;
    notes §3.56). *Validation:* existing guards unchanged; the timer gone, not defaulted.
26. **A slave-witness liveness probe for the doctor** — **EXECUTED 2026-08-13** (notes §3.90) as
    **P16**, once the design amendment below unblocked it. Two arms on one held slave fd, **each
    the other's control**: `POLLHUP` must be *absent* while the master is open — polled twice,
    back-to-back and at the PTY node's own 5 ms `IDLE_POLL`, because those are different claims
    (§15.49 clause 3) — and *present* after it closes. `supported` needs both; a control that fired
    is reported **first**, because a hangup that was never absent makes the post-close reading
    unreadable. Beside them, `SlaveWitness::prove_open`'s three steps are mirrored exactly, so the
    report says what the **shipped** check would have done rather than what the probe thinks of it.
    Placed after P8/P9/P10 deliberately: its paced arm parks 320 ms inside `poll(2)`, the syscall
    P9 is timing.
    **The answer, for Linux:** `prove_open` is **sound here, and measured rather than examined** —
    `path_still_resolves` `true` → `false` across the close while `fstat_on_the_held_fd_answers`
    stays `true` both sides, so `shipped_prove_open_would_refuse` moves `false` → `true` and
    `stat_comparison_can_tell` reads `true`. The tautology is visible in the same row: `fstat`
    still answers on a pair that is gone, which is why step 2 is the load-bearing one. Two bounds
    the probe prints rather than leaving to prose: it is sound *for this kernel* (the harness's
    residual was always the off-Linux half) and *for this edge* (whether the comparison sees the
    master's close, not liveness in general).
    *One design-invariant correction on the way:* the first draft reached for
    `BorrowedFd::borrow_raw`, which is `unsafe` — the same §16.3 wall that sent this question to
    the doctor in the first place. `AsFd` does it with a real lifetime instead of a promised one.
    **The pre-registered Darwin readings were answered 2026-08-13** (notes §3.93), and the first
    branch is the one that fired — recorded before the measurement precisely so this sentence
    could not be written afterwards. `path_still_resolves` reads `true` on **both** sides of the
    master's close, `fstat_on_the_held_fd_answers` stays `true` throughout, so
    `shipped_prove_open_would_refuse` never moves off `false` and `stat_comparison_can_tell` is
    **`false`**. **`SlaveWitness::prove_open` is unsound on Darwin, measured rather than
    predicted**, and notes §3.56's seven converted guards are held there by the compile-time
    borrow alone. The other instrument answers in the same row: `poll_can_tell` is `true`, quiet
    through 200 tight and 64 paced passes while the master is open, `POLLHUP` **6–16 µs** after it
    closes with a following `read(2)` answering `eof`. So the item's own conditional resolves:
    **the portable upgrade is a `serial_nexus_sys` `poll` helper rather than an argument**, and it
    is filed as item 66 rather than folded in here, this item's scope having been the probe. The
    two branches that did **not** fire are kept as the record of what was risked: `path_still_resolves:
    false` there would have **refuted** the prediction and been recorded as such (AGENTS §9), and
    `poll_can_tell: false` would have meant neither instrument is portable and the witness argument
    needed re-deriving rather than re-coding. The superseded filing follows.
    *Pre-registered Darwin readings*, so the interpretation is not chosen after the fact:
    `path_still_resolves: true` after the close makes `prove_open` **unsound on Darwin** and leaves
    notes §3.56's seven converted guards held by the compile-time borrow alone — with `poll_can_tell`
    already `true` here, the portable upgrade would then be a `serial_nexus_sys` `poll` helper
    rather than an argument; `path_still_resolves: false` there **refutes** the prediction and is to
    be recorded as such (AGENTS §9); `poll_can_tell: false` there means neither instrument is
    portable and the witness argument needs re-deriving rather than re-coding.
    *Two guards started green and were fixed rather than the code* — the exact failure this
    discipline exists to catch: the stat guard built its reading by hand and so could not see the
    *constructor* choosing `None` versus `Some(false)`, and a dropped negative-control conjunct was
    invisible because the verdict checked the windows separately. One pre-existing gate guard was
    repaired as collateral: it anchored its needle on the *tail* of a list that grows.
    The superseded filing follows. **Open** (S), **unblocked 2026-08-13**.
    The item was attempted and correctly **stopped**: a new probe id is derived from the design's
    §13 roster by `meta_derive`'s gate, so landing P16 first would have made the *tree* ahead of
    the design — which AGENTS §5 forbids in the same words it forbids the reverse. The amendment
    is now made (**§15.59**, plus §13's glance row, the roster string and the era row), so the
    construction proceeds under the amend-first order rather than against it.
    *Recorded with it:* P15's `question` widening rides **with** P16 rather than ahead of it. The
    probe now reports a software-flow reading while its `question` still names `CRTSCTS` only, and
    widening that string would move `probe_set` — spending an era boundary on a **wording** change
    while P16 will spend one on a real instrument. Two boundaries where one will do; the fold
    happens in P16's own commit. Mitigation already shipped: every software block carries its own
    `asks` string, so the JSON is self-describing meanwhile. *Evidence:* the harness's
    Darwin residual — the witness-fd behavior notes §3.56 leans on — is expected, not measured,
    as a standalone observation; notes §3.60 names the doctor as its home. A `poll(POLLHUP)`
    probe makes it measured. *Validation:* a new probe id is a new instrument and moves
    `probe_set` — the era move deliberate and recorded in `docs/doctor/README.md` (notes
    §3.57's rule); presence-never-answer.
27. **The Markdown value grammar wants a design note before any fix** — **EXECUTED 2026-08-12**
    as **§15.57**, and the decision is to **decline the escape**. The item asked for a decision,
    not an edit, and got one: the Markdown is a *view* of the JSON and the JSON is the artifact
    of record (measured — the rendering is a pure function of the model minus one field, while 0
    of 1064 Tier-3 scalar leaves carry their JSON kind, against 22 `type ==` clauses in the gate),
    so making the view injective buys a property nothing reads and does not buy the kinds. The
    cost lands on immutable evidence: §16.13 freezes every committed report, so an escape either
    mass-edits frozen artifacts or splits the corpus's grammar in two forever. The obligation the
    decline carries — that nothing quietly parses the rendering — is discharged by the rule
    §15.57 states and by the `--field-set` message that already names both remedies. No renderer
    change; no frozen artifact touched. Overturning it takes a consumer that genuinely cannot be
    given the JSON, and the sanctioned answer is then a third rendering, never a redefinition of
    this one.
28. **The DTR measurements** — **open** (S; needs a rig with DTR wired — the current rig leaves
    it unwired). **Still blocked after the 2026-08-14 re-cable, and now blocked on evidence rather
    than on absence of it** (notes §3.102): the bench went 3-wire → 5-wire, and P5 reads all six
    DTR crossings `false` on the new pair, 3 of 3, committed. So a *third* independent cabling says
    DTR carries nothing here, and the item's blocker is a property of every crossover cable this
    project has measured rather than of one. Wiring DTR↔DSR is a deliberate act someone must
    perform; it is not a side effect of adding handshake lines.
    *Remainder:* (a) the DTR-pulse cost question — whether the pre-check can ask
    from inside the node's own open rather than as a separate toggle (notes §3.68 measured the
    extra toggle; the falsified "not an *extra* toggle" claim is recorded at §15.53's entry).
    (b) The B→A DTR cells' `true`/`stuck-high`/`inverted` arms, and the transposed-read
    blindness notes §3.73 bounds. *Validation:* pre-registered readings before the wire is
    touched; committed captures.
29. **The tool-wrapper scripts' fate, recorded** — **EXECUTED 2026-08-12**. Answered from the
    git history rather than from memory, which is what the item asked for. The three were
    `scripts/lib/wait-for.sh`, `scripts/validate/phase0/license-gate.sh` and
    `scripts/validate/phase8/external-codec.sh`, and all three were **deleted in one commit** —
    `563fb9c`, 2026-07-24, "v10 track (§11.7–§11.9, §16.11) + fixes found on the real 7.0 rig",
    which is the §16.11 execution itself. The same commit created their successors:
    `itest/tests/p0_license_gate.rs` (new, 77 lines), a rewritten
    `itest/tests/p8_external_codec.rs`, and the `wait_until`/`wait_for` helpers in
    `itest/src/lib.rs`. So the v15 claim was true when written and was discharged by the very
    track it was pending on; nothing was lost and nothing moved elsewhere. `scripts/` holds
    exactly `bless` (§15.45) and the plan's workspace map already says so.
30. **Dual-scope figure equivalence** — **EXECUTED 2026-08-13** (notes §3.92). One run at each
    scope, same tree, same session, same box: **999 · 0 · 7** at default CI scope and **999 · 0 · 7**
    on the documented rig lane. Both rows are in the Status table with their scopes named, and **no
    delta is derived across them** — the equivalence is the observation, not an arithmetic step.
    The distinguishing evidence is the self-skip count, **13 → 5**, without which a rig row and a
    default-scope row are indistinguishable numbers; that is the check the v15 attribution lacked.
    The unreconcilable 835 stays annotated where it stands, superseded rather than rehabilitated —
    the equivalence now has a measurement at *this* era, which is all this item ever asked for.
    *Evidence:* the v15 Status line
    attributed a dual-scope 835 to notes §3.68, and v17 finds the attribution unreconciled —
    §3.68's verbatim record reads 830 at gates scope and 834/0 · 833/1 on the rig lane, with no
    835 anywhere in the session record (the Status row carries the annotation) — so the
    equivalence has no reconcilable measurement at *any* era. *Validation:* one dual-scope run
    at the current era; both figures land in the plan Status table with their scopes named, and
    no delta is derived across them (plan §3's figure-scope rule).
31. **The packaging evidence pass** — **open**, and **re-scoped 2026-08-12** from "needs a root
    box" to "needs a root box *for one half*, and a CI job for the regression guard". The
    re-scope is a decision about where the evidence lives: a measurement taken once on a
    maintainer's laptop is not a regression guard, and the whole point of this item is claims whose
    evidence class nobody can see.
    *Executed 2026-08-12: (a) and (b) in full, (c) built and self-skipping.* The README gained an
    **Evidence classes** section — a three-class vocabulary, a per-directive table for the unit and
    a per-claim table for the page, covering all **41 active directives**. Several claims moved
    from assumed to measured by being checked (`--help` for every flag and verb the page names;
    the tty node's real mode and group), several are honestly **man-page** with the quote verified
    to exist in `systemd.exec(5)` on this box, and six are **unverified** and say so — the
    socket-group static-identity recipe, the upgrade procedure, and — until 2026-08-15 — the
    `/dev/ttyACM*` half of the dialout claim. **That last clause is split out as item 78**: it needs
    a CDC-ACM *device* and no privilege supplies one, so leaving it inside an item routed as "needs
    a root box" made this item cover something root cannot fix, and un-closable for that reason. `itest/tests/p8_packaging.rs`
    is the gate: six tests, **three of which need no tool and never skip**, so the drift class this
    tree can cause is covered on macOS too. Eight plants against the real tree, each reddening and
    each restored md5-identical. What it provably does *not* catch is in its own module doc — a
    `SupplementaryGroups=` naming a nonexistent group (measured: exit 0, empty stderr, reported
    each run rather than asserted, so a stricter systemd is noticed rather than contradicted), and
    a *value* change on a well-formed directive. **(c)** exists and self-skips naming the
    precondition that failed; its `required` mode is set nowhere until a CI run proves a runner is
    systemd-as-PID-1 with passwordless root.
    **(c) EXECUTED 2026-08-13** (item 68, notes §3.99). CI run 31695823765's root arm reads
    **6 passed, 0 failed**: the probe runs under `DynamicUser=yes` as `uid=65180`, prints all
    nine readings — `state_stat=root:777`, `state_real=/var/lib/private/…`, `private_list=ok`,
    `private_stat=root:755` — and both halves of Claim 4 hold, so PKG-2's `DynamicUser`
    id-mapped-mount behaviour is **measured** and the step gates rather than reports. It took
    five successive probe defects to get there, none of them the packaged unit's, which is item
    68's record and the reason that item exists separately from this one. *Remaining:* the four
    `unverified` rows, whose machinery the root arm already has. The superseded line follows.
    *Remaining:* that switch, and the four `unverified`
    rows, whose machinery the root arm already has.
    *Split:* **(a)** the evidence-class pass over `packaging/serial-nexus-daemon.service` and its
    README — marking each deployment claim *measured* / *man-page* / *unverified* — needs no root
    and is the item's first validation clause; **(b)** a no-root gate that runs on every push,
    which is the part that catches regressions *this project can cause*: the unit's directives and
    the README's claims about them, both derived by parsing rather than hand-kept; **(c)** the
    `DynamicUser=` mount measurement itself, which needs root and therefore belongs in a CI job
    that has passwordless sudo, not on a laptop.
    *Instrument validity, measured before building on it* (§13's own rule, applied to a tool
    rather than a probe) — **and the first measurement was wrong in the direction that matters,
    which is why re-verification is rule 17**. The orchestrator read `systemd-analyze verify` as
    catching an unknown directive (exit 1) while missing a bad value (exit 0). Re-run with the
    environmental arm removed, the truth is worse and simpler:

        unstaged (real ExecStart, nothing installed):  real=1  unknown=1  badval=1  noexec=1
        staged   (ExecStart -> an executable stub):    real=0  unknown=0  badval=0  noexec=1
        staged   + --recursive-errors=no:              real=0  unknown=1  badval=1  noexec=1

    The "caught" reading was an **artifact of the missing binary** — every exit 1 in the first row
    is the environmental arm, not the defect. With it removed, systemd 259's default flags exit 0
    on an unknown directive, an unknown *section* and a bad value alike. What survives every
    configuration is **stderr**: the diagnostic is always printed. So the gate requires **exit 0
    AND empty stderr**, and the stderr half is what makes it work on systemd < v250, which has no
    `--recursive-errors`. Second finding, deterministic across three runs and the opposite of what
    the flag name suggests: **`--recursive-errors=no` makes the exit status bite**, so the gate
    probes `--help` and passes it when advertised, as an independent second signal.
    A tool that prints a diagnostic and exits 0 is the `jq -e` instance in a third costume.
    *Sequencing, deliberately:* the root-gated half lands **self-skipping with its reason printed**,
    and its `required` mode is switched on only after a CI run proves the runner actually provides
    systemd as PID 1 — measured, not declared, the way §15.52 made `SNX_RIG_FLOW`'s precondition a
    measurement. Shipping `required` on an assumption reddens a lane for someone else's runner
    image. *Declined:* widening the privileged helper to take the measurement locally. It would need
    root-equivalent capability (starting a unit, reading a foreign process's mount table), and the
    helper is mode 0700 and invocable by the unprivileged user *by design* — §15.45's narrowness is
    the safety argument, not a style, and AGENTS §4 makes widening it an amendment rather than a
    patch. It is also the wrong instrument: the claim under test is systemd's behaviour, which no
    capability this project holds can make true. The superseded filing follows. *Evidence:* PKG-2's
    `DynamicUser` id-mapped-mount behavior is unmeasured; `packaging/serial-nexus-daemon.service`
    and its README carry deployment claims whose evidence class is unrecorded. *Validation:*
    mark each claim measured versus man-page; the mount behavior measured on a root box when one
    is available.

### Items 32–46 — construction items, and the alignment pass's filings

The alignment pass's filings: the codec-author validation surface (§8) named its gaps, and the
derive-from-tools doctrine named its missing gates. All are **open**; none is promised as
existing — §8 names them as ledger items.

32. **Conformance kit: attribute-schema suite** (S). **Executed 2026-08-12** (notes §3.75):
    `attributes_are_structural(factory, good, bad[])` in `codec-api/src/test_support.rs`, generic
    over the table type so the kit still names no TOML crate. Four negatives prove it bites —
    lenient, unwinding, and anonymous-refusal schemas — and three consumers run it: the exec
    codec's schema, the `reference` factory, and `tinymux-codec` from the consumer position.
33. **Conformance kit: Err-then-Ok recovery suite** (S). **Executed 2026-08-12** (notes §3.75):
    `recovers_after_garbage`, opt-in, with `LatchesOnError` — a decoder that drains correctly,
    passes every other suite in the kit, and fails only this one — as its negative. The
    reference codec runs it. Its stated limit is part of the suite: it feeds an *envelope* frame
    with an unknown type byte and does **not** assert re-alignment after unaligned noise, because
    where a correct length-guided resyncer re-aligns depends on the noise (§8's kit-honesty rule).
34. **Exec battery: error-path fixtures** (M). **Executed 2026-08-12** (notes §3.75):
    `--error-paths`, opt-in, three arms — unknown type byte, oversize length prefix, a channel
    length overrunning its body — each requiring the child to terminate, not relay the fault, and
    *signal* the refusal; the verdict names the arm and the byte offset. `strict.py` is the new
    positive control; `passthrough.py` fails all three, which is the fail-first proof and the
    honest statement that a permissive relay is legal but visible. The two remaining decode
    errors (non-UTF-8 channel identity, non-UTF-8 `error` reason) are deliberately not injected:
    the harness's own encoder cannot express them.
35. **Exec conformance for the demux shape** (M). **Executed 2026-08-12** (notes §3.75):
    `--mux-to <channel>` on both exec modes. `passthrough-codec.py console` passes the whole
    battery under its declared mapping and **fails golden without it** — the fail-first proof that
    the mapping is load-bearing. Identity mode is byte-compatible: the map is the identity
    function in every method, and a run with no mapping declares none in its verdict.
36. **Golden transcripts of the daemon boundary** (L). **EXECUTED 2026-08-12** (notes §3.86).
    A transcript is the exec-codec child-pipe conversation as **two ordered byte streams** — `<`
    for what the child writes, `>` for what the daemon writes — never one interleaved log, and
    that is not a simplification: the exec boundary is two pipes polled concurrently (§15.22), so
    there *is* no defined interleaving and pinning one would pin a scheduling artifact. Records
    are byte-exact hex with a run-encoded tail so a near-`MAX_FRAME_SIZE` fragment is one line,
    each carrying a generated annotation (`data channel="console" payload=65526 B frame=65540 B`)
    so the file reads as the conversation the item says nothing else gives an author. A new
    `serial-nexus-sim transcript` mode plays both roles off one file — `--record` to generate,
    `--verdict` to replay — and it is the one sim mode that cannot print a JSON verdict line,
    because stdout *is* the envelope pipe; it also parks rather than exiting, an exec child that
    exits being a crash the daemon respawns over the evidence. Generated against the live daemon,
    so they cannot drift. *Fail-first:* five plants, each rebuilt and reverted — including the
    item's own one-byte mutation, which the comparator caught at the line and the replayer caught
    again at the byte offset (`mismatch_offset: 65684`, expected `0x0b` observed `0x0a`), so the
    two halves are independently proven rather than one trusting the other. Four scope decisions
    are written into the transcripts' own headers rather than only into this ledger, each taken
    because the alternative would have shipped a flaky golden. The superseded filing follows.
    Replayable byte transcripts of the
    daemon driving a demux (empty-channel in both directions, oversize fragmented, open/close,
    unconfigured-channel counted) plus a sim replay mode; generated by a test against the live
    daemon so they cannot drift. *Evidence:* golden vectors cover single frames; nothing gives
    an author the *conversation*. *Validation:* replay fails on a planted one-byte mutation.
37. **Executable doc examples for `docs/codec-authors.md`** (M). **Executed 2026-08-12** (notes
    §3.75): `itest/tests/meta_codec_authors_doc.rs`, five guards, every expectation *parsed from
    the document* rather than typed beside it. Three mutations executed and reverted: one hex
    digit in the frozen table, one digit in the worked `data("", "AB")` example, and one invented
    counter name — each reddened exactly one guard. The doc's §5 TOML loads against a real daemon,
    and stripping its multiplexed `write_mode` is refused naming the edge.
38. **Codec-node teardown-conservation suite** (M). **EXECUTED 2026-08-12** (notes §3.86) as
    `itest/tests/p5_codec_teardown.rs`, asserting `discarded_at_teardown` and the conservation
    equality on a codec node under teardown. *Fail-first:* removing `teardown_loss.drain()` from
    `CodecNode::signal_stop` reddens two of the three guards with the removal reporting
    `discarded_at_teardown: 0` where 8008 is owed, and leaves the third — the read-only demux,
    which charges the *running* discard and not the teardown ledger — green, which is the
    disjoint-reddening the template asks for. **The "folds review 37's codec guards into the kit
    surface" clause is executed as one new suite plus four citations, not five new suites, and the
    reasoning is in the new file's header:** only 37-CODEC-3 had a codec-side property no suite
    could see; unknown-key naming is already `attributes_are_structural`; reserved-identity
    lifecycle and the mid-chunk charge are node properties with existing guards; quoted exec paths
    is a harness property; and 37-EXTC-2's partial-frame tolerance is already asserted byte for
    byte by `fragmentation_tolerance`, so a second suite would be one rule spelled twice. The
    superseded filing follows. A conformance test shape asserting
    `discarded_at_teardown` and the conservation equality on a codec node under teardown, so an
    author inherits the loss-accounting promise (§15.50); folds review 37's codec guards into
    the kit surface (mid-chunk refusal with `accepted + discarded == total`, unknown-key naming,
    reserved-identity lifecycle, quoted exec paths, teardown-tolerant partial frame).
    *Validation:* notes §3.55's disjoint-reddening fail-first is the template.
39. **A second template codec** (S). **Executed 2026-08-12** (notes §3.75):
    `examples/external-codec/tinymux/`, a two-channel tag framer (`tag|kind|len|payload`) with
    parser state, byte-wise resync, and one attribute (`channels`). It runs the three suites
    `acme` cannot, from the consumer position. Deliberately **not** an envelope codec: a device's
    own framing is the commoner case. `info.codecs` moves to
    `["acme", "exec", "reference", "tinymux"]` in the gate and the template README together, and
    the gate additionally asserts a bad attribute table is refused naming the key with nothing
    created — §11's pre-create precheck, from outside the tree.
40. **Derive-from-tools meta-gates** (M). **Executed 2026-08-12** (notes §3.75) as
    `itest/tests/meta_derive.rs`, four gates, each enumerating **both** sides from their real
    sources and each fail-first against a planted stale entry. (a) The `SNX_*=required` roster,
    from the harness code against plan §3's table. (b) Both documented probe rosters against the
    registry in `probes.rs` — and the item's alternative was taken for the third copy: the
    enumeration in `doctor/src/main.rs`'s module doc is **deleted**, because
    `docs/serial-nexus-doctor.md` is the registry of record (§13) and that copy had drifted three
    probes without anyone noticing. (c) A bijection between `fuzz/Cargo.toml`'s `[[bin]]` table
    and the corpus directory: the pre-existing
    `every_unstable_fuzz_api_export_has_a_fuzz_target` enumerates targets by *listing files*,
    while CI's loop iterates `cargo fuzz list`, which reads the manifest — so an unregistered
    `.rs` file satisfied the old gate while never being built or fuzzed. The manifest is parsed
    rather than shelled out to, because `cargo fuzz` needs nightly and is installed only in the
    scheduled lane; a gate that self-skipped on every push is the vacuous green the required-mode
    lattice exists to prevent. (d) The documented verb index against the daemon and `ctl` (the
    error-code table in the same file was already derived). One gate landed red on arrival, which
    is the item working: it found the `main.rs` roster three probes stale.
41. **`actual_baud` read-back on serial open** (M; new design content — the probe already
    proves the instrument). Report the actual rate beside the requested one in node state;
    reporting only, no verdict or fault change. *Evidence:* P14's `adapter-refused` class — a
    4 Mbaud ask silently landing at 9600, with no errno — is visible to the doctor and
    invisible in node state. *Validation:* fail-first against a refusing rate on the rig.
42. **`boundary::BlockingReader` unification** — **EXECUTED 2026-08-12** (notes §3.88).
    `BlockingReader` → **`BlockingWorker`**, the name the record itself proposed (review 32's
    SIMP-3) rather than an invented one. The loss counter is optional **structurally**, not by
    convention: `arm` mints the `Notify` and *returns* it, `arm_quiet` mints none, and the
    `lost()` accessor is gone — so "this worker has nothing to signal" is now a fact about which
    method you called, and the serial supervisor can no longer await a *previous* reader's signal
    (which it never did, but only by call ordering). All three call sites rebased.
    **The log third was measured rather than declined**, and the armchair case against it was
    strong: the log's stop is `Queue::closed` under a mutex and `Condvar`, which an `AtomicBool`
    cannot wake; its join is bounded-with-detach (§7.3), not `join()`; its spawn is injected. The
    measurement said otherwise — `log.rs` −50/+31, `boundary.rs` +40, of which the three additions
    (`arm_with`, `is_armed`, `detach`) are **general primitives rather than per-caller parameters**,
    `detach` being a bounded-wait primitive the library simply lacked. Net +21 lines for one type
    instead of three.
    **Join-after-abort ordering is byte-identical at all nine entry points** (`signal_stop` /
    `teardown` / `Drop` × serial / pty / log), written down before the change and checked after —
    including the pty's deliberate flag-before-tasks order, which is the *reverse* of serial's and
    was preserved rather than "fixed".
    *Fail-first:* three defects planted in the unified worker, each caught by a **pre-existing**
    guard, which is the proof a behaviour-preserving refactor actually owes — D1 (join silently
    becomes detach) by `signal_stop_returns_before_the_thread_exits_and_join_waits_for_it`; D2
    (`arm` returns a different `Notify` than the thread got) at both altitudes, the boundary unit
    guard *and* `p7_unplug::unplug_faults_serial_and_leaves_pty_client_attached`; D3 (a stop flag
    `signal_stop` never sets) by both stop guards — **but only as a deadlock**, which is item 64(h)
    below.
    *One honest cost, stated at the field:* `BlockingWorker::stop_join` is not usable on the log
    worker, whose `signal_stop` sets a flag `writer_drain` never reads. The hazard predates the
    unification — today's `w.join()` blocks forever too if `q.closed` was not set first — but the
    shared name now needs the caveat. The superseded filing follows. Rename, optional loss counter, rebase
    `serial.rs`/`pty.rs`/log on it, preserving join-after-abort ordering — §16.1's recipe
    (notes §3.21) followed as written. *Validation:* existing boundary guards unchanged; no
    behavior delta, asserted by test.
43. **`--json-out` everywhere the doctor runs twice** (S). **Executed 2026-08-12** (notes
    §3.75): the `docs/doctor/README.md` "Adding one" recipe and both CI doctor jobs take one run
    and gate on its twin — `--json-out <path>` then `jq -e -f expectations/<os>.jq <path>`,
    verified end to end (exit 0 on both halves). The recipe now states why: P9's and P10's numbers
    move run to run, which is the same reason the era diffs take three samples.
44. **Aux-doc corrections** (S). **Executed 2026-08-12** (notes §3.75); no clause had been
    discharged by an earlier sweep, and two of them were worse than filed. `docs/macos.md`: the
    withdrawn 32/35 leaf-path pair was still **live** in the sentence it serves, not merely
    unannotated — replaced with the reproducible 65/71 and annotated in place under §15.44's
    register, the counterexample being *bigger* than was stated, not smaller; and the
    `TIOCGICOUNT` bullet (item 23a's third site) was wrong twice over, since P3's verdict never
    read the counters either. `docs/serial-nexus-doctor.md`: the P15 row was not stale but
    **absent** — the roster ran P1–P14 — which is the drift shape item 40 gates. `docs/security.md`:
    its door enumeration is now explicitly the *daemon's*, with the blessed replug helper given its
    own section stating §15.45's five bounds rather than a line implying a fourth daemon door.
    Frozen `docs/doctor/` artifacts untouched.

45. **`existing-terminal`'s refusal, made structural** — **CLOSED AS A DECLINE 2026-08-12.** The
    item offered two answers and named the second explicitly ("or record the decision that a
    schema which never admits the word is the better answer and close the item"); that is the one
    the evidence gives. **Which shape a §14 deferral gets is decided by where it sits in the type
    system, not by taste:** entry 14 is a deferred *role* of a shipped kind whose `faces` field is
    the shared two-valued `Facing`, so the schema cannot exclude it and `validate` must refuse it
    — structural refusal for want of an alternative. Entry 15 is a deferred *whole kind*, and a
    word the schema never admits is unreachable **by construction**, where a `validate` refusal is
    one forgotten call away from admitted. The precedent is §15.8's configuration/state split —
    state fields *do not exist* on configuration types, so the question cannot be asked. *(An
    earlier draft of this disposition cited §15.4's merge diamond instead; the review of this
    change caught it, and it is a counterexample rather than a precedent — the one-producer
    invariant is `TargetEndpointOversubscribed`, a `GraphModel::validate` refusal over a config
    that deserializes perfectly. Corrected in place; the decline is unaffected, its stated
    precedent was not.)*
    *Measured, not argued:* option (A) was built on a scratch tree and costs two things forever —
    serde's internally-tagged unknown-variant error enumerates every variant, so `type = "seriall"`
    would be answered with a list advertising `existing-terminal`, a kind the daemon refuses one
    stage later; and §7.7 states two fields and then "otherwise it behaves as a boundary", so the
    rest of the field set would be a guess frozen by `deny_unknown_fields` and §15.16's additive
    promise, while `NodeConfig::shape` would owe `to_model` an endpoint topology the design never
    states — letting an operator's *edges* validate against a shape nobody designed. The blast
    radius was also measured: exactly one out-of-core match site (`daemon/src/nodes/mod.rs`).
    *The handshake the item designed still works, in the other direction:* the existing pair
    `existing_terminal_is_refused_at_load_listing_the_shipped_kinds` and
    `a_refused_existing_terminal_disturbs_neither_the_running_graph_nor_the_daemon` is now the
    **tripwire on the decline** — planting the variant reddens both at their error-code
    assertions (`-32002` where they demand `-32602`, the word ceasing to be unknown one stage
    early), observed. Recorded at design §14's vocabulary and entry 15, and in
    `core/src/config.rs`'s module header where a future reader reaches for the variant.
    The superseded filing follows.
    Filed 2026-08-12 (notes §3.75) rather than done silently, because it is a change to what an
    operator reads. *Evidence:* §14 entry 15 is *refused-at-load*, and the refusal is real,
    live and now guarded — but it is serde's unknown-variant error at `INVALID_PARAMS`, which
    lists the shipped kinds and cites nothing, where entry 14's sibling is a structural error
    from `GraphConfig::validate` naming the deferral and its section. §7.7's "the same treatment
    §7.1 gives the serial output leg" was the claim that made the gap visible; the design now
    states both shapes instead (§14's vocabulary). *Remainder:* add a `NodeConfig` variant for
    it refused in `validate` with a "not implemented (§14)" message, so the two deferrals of one
    kind read alike — or record the decision that a schema which never admits the word is the
    better answer and close the item. *Validation:* the existing guard
    `existing_terminal_is_refused_at_load_listing_the_shipped_kinds` asserts **today's**
    behaviour, so the upgrade reddens it loudly rather than passing through unnoticed — the
    handshake is deliberate.

46. **`p3_idle_cost` sits too close to its own ceiling under suite parallelism** — **EXECUTED
    2026-08-12.** The controlled measurement was taken and it **refutes the first disposition**:
    the `ACTIVE_POLL`→`IDLE_POLL` backoff has not regressed. On a 20-core box, 10 s window, three
    healthy samples per rung at 1/2/8/16/32 fds against two neutered ones at 8/16/32, least
    squares gives a healthy slope of **0.0728 %/fd** with a **1.194 %** intercept, and a neutered
    slope of **0.2004 %/fd** — a 2.75× separation, with the healthy figure reproducing §15.19's
    recorded ~0.06 %/fd within 20 %. What had moved is the **fixed** term, which the artifact
    never recorded separately: its 2.00 % is a single total at 32 fds with no baseline rung, so
    an absolute ceiling derived from it is dominated by a quantity the mechanism under test does
    not control — which is why 3.50 % fired against a healthy 3.3–3.6 % on this box.
    *What landed:* the guard measures **two rungs in one daemon process** — `load`s the
    artifact's `baseline_tty_fds`, samples 10 s, grows the same process to `idle_tty_fds` with
    `add-node` (§11), samples again — so the fixed term cancels by construction rather than
    merely similarly. Assertion (1), the recorded 20 % budget, is unchanged. Assertion (2) now
    reads `marginal <= per_fd_cpu_percent × DRIFT_FACTOR`, both derived from the artifact, which
    gains `baseline_tty_fds`, `per_fd_cpu_percent` and a provenance block (box, date, commit,
    both sweeps, both fits, the neutered control) — `total_cpu_percent` **kept** and annotated as
    the historical single-rung record it is, never repurposed. `DRIFT_FACTOR` stays 1.75; only
    the figure it multiplies changed.
    *Fail-first:* `back_off`'s body replaced with `let _ = wait;`, workspace rebuilt, guard red at
    **0.2375 %/fd** against the 0.1274 %/fd ceiling, restored, green — run in a `git archive
    HEAD` scratch copy because the working tree was mid-edit by concurrent agents, which is
    AGENTS §9's don't-measure-a-moving-tree rule applied to the measurement itself.
    *The third disposition turned out unnecessary rather than declined:* the marginal form is
    load-insensitive where the absolute form was not — a run at loadavg **12.68** read
    0.0792 %/fd, inside the band of runs at loadavg 1.4 — so there is no separate "under the
    suite's own parallelism" operating condition left to record. **The decline stands and was
    not laundered:** the ceiling was not raised; the neutered control still reddens the guard.
    *Named coverage change, in the guard's own module doc:* a regression inflating the daemon's
    **fixed** cost without scaling per fd is now caught only by the 20 % budget. Recovering that
    sensitivity honestly needs a recorded fixed-cost figure with an era of its own, across more
    than one box — a measurement nobody has taken, not a number to invent. The superseded filing
    follows. Filed 2026-08-12 (notes §3.75 §I).
    *Evidence:* the guard fails at **3.60 / 5.50 / 4.30 %** of a core under heavy external load,
    reads **2.90–3.20 %** in five solo runs on a quiet box, and still read **3.80 % and red** in
    one of two back-to-back whole-suite runs on that same quiet box — the suite runs targets in
    parallel, so the 10 s window overlaps ~20 other binaries and the contention is the suite's own.
    The ceiling is 3.50 %, being 1.75× the 2 % in `docs/benchmarks/phase3.json`. Not a regression
    from this session: it failed on the unmodified tree. CI's Linux lane passes it.
    *Declined, explicitly:* raising the ceiling because it fired — the tripwire is deliberately
    tighter than the 20 % budget and loosening it is silently re-fixing a recorded decision
    (AGENTS §5). *Remainder:* one controlled measurement deciding between the two dispositions the
    test's own failure message names — the `ACTIVE_POLL`→`IDLE_POLL` backoff regressed, or the cost
    is genuinely higher now and the artifact should be re-measured with its era stated. A third
    disposition is available and must be chosen rather than drifted into: measure *under the
    suite's own parallelism* and record that as the guard's operating condition, since that is
    where it actually runs. *Validation:* the reading taken on a box with a stated load, three
    samples minimum (P9/P10's rule — one sample of a varying quantity is indistinguishable from a
    difference); any artifact update carries its date and commit.

### Items 47–55 — filed at the v17 revision

The v17 revision's filings (2026-08-12): item 47 is the pattern wait's construction — the one
design-ahead-of-tree surface, amended first per AGENTS §5 — and 48–55 are what the revision's
input digest surfaced: two vacuous-green risks, one gate that has never executed, four
consolidation debts, and a lint gap. Item 47 is **executed** (2026-08-12; notes §3.83); 48–55
remain **open** and none is promised as existing.

47. **The pattern wait: `tap.wait`** — **EXECUTED 2026-08-12** (notes §3.83). The design is no
    longer ahead of the tree: §10's contract and §15.56's decision record are both implemented,
    and the verb answers on the wire instead of `method not found`.
    *Evidence:* the contract is §10 (The pattern wait); the decision record and its declines are
    §15.56; the client-side and daemon-side mechanics were surveyed against the tree at filing
    (the waiting-verb machinery, the hub/ring register path, and the four existing tap
    consumers). *What landed, against the filed clauses:* (a) the daemon matcher —
    `daemon/src/pattern.rs`, a `regex-automata` meta automaton (linear-time in the haystack in
    every strategy it picks) with the compiled program capped at `MAX_COMPILED_BYTES`, one
    automaton over the whole pattern set so leftmost-wins is deterministic, literals escaped
    byte-wise into that same automaton so there is one engine and one set of limits, all seven
    §16.12 maxima checked before anything is armed, the matcher living **inside** `TapHub` so it
    is fed with no queue between hub and matcher, ring scan and live arming in one
    `arm_wait` critical section, gap-resets-window in `ScanWindow::feed`, and the teardown sweep
    in `TapHub::detach_all`; (b) `AppError::EndpointGone` (`-32008`) through the one registry,
    the two-way README table row asserted by `docs_rpc_table_matches_the_registry`; (c) the
    `serial-nexus-ctl tap-wait` verb on the existing one-shot path — `call()` does park, as
    filed, because it holds its write half open and sets no read timeout — with a multi-value
    exit; (d) the `docs/rpc/observation.md` section plus *Doing it client-side*, the recipe
    §15.56 promised beside the verb; (e) the harness `WaitConn` in `itest/src/lib.rs` and the
    battery in `itest/tests/p12_pattern_wait.rs` — twelve guards, six of which need no serial
    device and so run on **every** platform, with a per-guard fail-first table in its module doc
    and four of those reverts executed and recorded rather than asserted.
    **One clause deviated, deliberately and with the reason recorded** (notes §3.83): the exit
    codes are **0 matched, 1 timed out, 2 could not run**, not the "0 matched, 2 timed out,
    1 error" filed here. Two independent reasons, both discovered against the tree rather than
    reasoned from the filing: `2` is already clap's usage-error code on that binary, so a script
    could not have told "no match" from "you misspelled a flag"; and the doctor precedent this
    clause cites actually puts the *verdict* at 1 and the *operational failure* at 2, so the
    filed numbering inverted the precedent it named. The landed scheme is `grep(1)`'s, for the
    tool that asks `grep`'s question, and it agrees with both the doctor and clap.
    *Declined (at §15.56, not re-arguable here):* deliver-path matching, backtracking engines,
    text patterns, unbounded lookback, broadcast delivery, web-bridge admission at
    introduction — **all still declined**, and the last is visible in the tree as the absence of
    `tap.wait` from the web bridge's allowlist. *Validation, as run:* the eleven-guard battery;
    the `-32006` interplay guard; `state` visibility asserted for an armed wait's lifetime *and*
    its disarm; the maxima refused with nothing armed, one case per dimension; both Apple
    compile triples, the minimal-daemon clippy and `cargo deny` green with the new dependency
    edge — which, as §15.56 predicted, added **zero** lockfile packages (the `Cargo.lock` diff
    is one line: the daemon's own edge).
48. **The macOS doctor gate has never executed green** — **EXECUTED 2026-08-12**; both halves
    now closed. *The placement half, landed this session:* the `macos` job's gate step moved to
    sit immediately after `cargo build` (which is its only real dependency) and ahead of the test
    step, and **both** steps now carry `if: !cancelled() && steps.build.outcome == 'success'`, so
    the reorder does not merely move the hiding in the other direction — a red gate cannot hide
    the suite either. Both doctor jobs' artifact uploads gained `if: !cancelled()`, because a
    report that failed its expectation is the one most worth reading and a red `jq -e` used to
    upload nothing at all. AGENTS §3 and `docs/macos.md`'s roadmap paragraph were corrected in
    the same commit (rule 21: CI is the authority, the mirrors follow). The measurement half was
    discharged 2026-08-13 and its record follows unchanged: CI run 31657666919, job
    94315579211, on macOS 26.5.2 / arm64 — the diagnostic printed `arm64`, `ProductVersion:
    26.5.2` and `Mach-O 64-bit executable arm64` for the binary, the doctor produced its JSON
    twin, and `jq -e -f expectations/macos.jq` printed `true` and exited 0. So the ENOEXEC
    observation below has not recurred, and **no cause is claimed for it** — it was seen once
    and has not been seen since, which is not the same as explained. What remains open is the
    second half, unchanged: the gate still sits *after* the test step, so a red test step
    would hide it again (rule 22's class — the gate needs only the build). Narrows item 18's
    capture half; closes none of it.
    *Evidence:* the gate's first-ever execution (notes §3.77) failed 2.5 s in with
    `cannot execute binary file` (ENOEXEC) on `target/debug/serial-nexus-doctor`, on a runner
    where `cargo build --workspace --locked` had just succeeded; no cause is claimed; CI now
    prints `uname -m`, `sw_vers`, and `file` on the binary, green runs included. *Remainder:*
    one green `expectations/macos.jq` execution at the current tree, and a gate placement a red
    test step cannot hide (rule 22's class — the gate needs only the build). Narrows item 18's
    capture half; closes none of it. *Validation:* the run's log shows the jq step executed and
    the doctor's exit status reached the lane.
49. **Skip-discipline hardening: the python3 class, and skip-message naming** — **open** (S).
    *Evidence:* (a) ~15 exec/envelope-codec tests across four files self-skip when `python3` is
    absent, no lane asserts they executed, and a runner image dropping python3 turns the whole
    item-34/35 conformance battery vacuously green — rule 22's tell, and exactly the hole
    `SNX_WEB_UI=required` closed for `node` (the `p8_web_history` module doc is the template).
    (b) Forty-plus SKIP messages and every `skip_no_*` call site free-type the enclosing test's
    name; a rename leaves a message that is false on the box printing it (rule 11's bar) —
    measured: zero live mismatches today, so this is prevention. *Remainder:* a required mode
    for the python3 class (one mechanism, mirrored — rule 11), set in CI's Linux lane, with its
    lattice row and both `meta_derive` sides; and a source-scan meta-gate that a SKIP message
    naming a test names its enclosing `#[test]` fn, with rule 10's planted offender.
    *Validation:* fail-first: one gated test run with a PATH lacking python3 under the new
    required mode reddens; the planted wrong-name SKIP reddens the new gate.
50. **Harness scaffolding consolidation** — **EXECUTED 2026-08-13** (notes §3.91). Four
    primitives in `itest/src/lib.rs`, each self-tested per §16.5: `KillOnDrop`,
    `wait_daemon_ready`, `RawDaemon`/`RawDaemonBuilder`, and `WebServer`. `itest/src/lib.rs` +561;
    the tests −308 net across 31 files.
    **All four filed counts were verified exact — and two undercount the class**, which is worth
    more than the tidying: the kill-on-drop shape is really **16** files, not 14 (`p8_web.rs`'s
    `Kill(Child)` and `p13_legacy_defaults`'s `Bare(Child)` are the same three lines under other
    names), and there are **5** raw-daemon spawn sites, not 4 — the fifth is inline rather than a
    named type and so escaped a count that looked for types. `p4_purge`'s `Sink` was deliberately
    left alone: piped stdout plus a `verdict()` method is a different abstraction, not a bare
    guard.
    *One intentional behaviour delta, in the stricter direction (AGENTS §9):* the seven
    `wait_socket` copies stopped at `UnixStream::connect(..).is_ok()`; the shared
    `wait_daemon_ready` keeps that — a stale socket file still refuses, which the `test -S` form
    could not tell — and adds the `info` round trip `Daemon::start` always did. No guard changed
    colour. *Not consolidated, deliberately:* the raw RFC 6455 web clients, whose per-file
    capability divergence is load-bearing and whose own module doc says so. *Fail-first:* each
    defect planted **in the primitive** rather than a caller — an emptied `KillOnDrop::drop`, a
    `wait_daemon_ready` weakened back to a bare connect probe, a `WebServer` reporting port+1 —
    each reddening the primitive's own self-test.
    *Evidence:* `KillOnDrop` re-derived in 14 files, `spawn_daemon` in 8, `wait_socket` in 7,
    four one-off raw-daemon wrappers, and the `serial-nexus-web` boot scaffolding in 7 — all
    behavior-preserving duplicates; §16.5's rule is "assertion helpers are shared and
    self-tested", and every raw spawn site boots a daemon without §15.43's leash, which is why
    item 24 lands through the same helper. *Remainder:* lib primitives (a kill-on-drop child
    guard, a raw restartable-daemon spawner with the connect-probing readiness wait, a
    `WebServer::start`), each self-tested; the raw RFC 6455 web clients are deliberately
    excluded — their per-file capability divergence is load-bearing (`p12_web_ws_bounds`'s own
    module doc). *Validation:* existing guards unchanged; no behavior delta, asserted by the
    suite; item 24's leash semantics kept opt-in.
51. **Client socket-fallback policy hoisted into `serial_nexus_rpc`** — **EXECUTED 2026-08-12.**
    One implementation, `rpc::socket::resolve_client_socket`, with the policy split out as a pure
    function of the two candidate paths and an existence predicate (`resolve_client_socket_from`)
    so both arms are reachable from any environment with nothing on disk — the two discriminators
    are *files at paths this box derives*, so a test arranging them for real would have two
    parallel tests fighting over the same two inodes. Ten unit tests in the rpc crate: the shared
    helper self-tested, §16.5's rule. `ctl` and `web` re-pointed; `web/src/rpc.rs` deleted rather
    than left as a re-export, its prose preserved at the new home. Named `resolve_client_socket`
    and not `resolve_socket` deliberately — the daemon keeps a private `resolve_socket` that must
    **not** fall back, and a public same-named function in a crate the daemon depends on is an
    invitation to unify two things that must stay different.
    **The hoist found a live defect, which is why two copies is the shape §16.5 bans:** the web
    console's copy had *two* arms where the policy has three — it never asked whether the process
    was root, so it read `XDG_RUNTIME_DIR` first and handed everyone without a usable one the
    **root** arm `/run/<name>.sock`. An unprivileged console with the variable unset or exported
    empty therefore pointed at a path only a root daemon ever binds, and said so through the
    bridge's 503, naming a socket nothing on that machine will ever create — every unprivileged
    macOS session, and any stripped service environment that exports the variable empty.
    *Fail-first:* three plants, each rebuilt and observed — inverted precedence in the pure
    function; the wrapper's two defaults swapped, which **only** the wrapper-level test caught
    (the other four new tests stayed green, which is why that test exists); and the client call
    sites. Plan §17.3's window retirement now edits one crate. The superseded filing follows.
    *Evidence:* `ctl` and `web` carry byte-duplicate implementations of the rename-window
    fallback (current socket name wins; `LEGACY_DAEMON_NAME` only when the current one is
    absent) — the two-copies-that-must-agree shape `rpc::socket`'s own module doc records as
    the reason that module exists (notes §3.72). *Remainder:* one rpc-crate implementation
    beside `default_socket_path`, both clients re-pointed; plan §17.3's window retirement then
    edits one crate. *Validation:* behavior identical (the single retired-spelling constant
    stays in rpc; the retired-names meta-gate allowance is untouched).
52. **devprep follow-ups: parity, reporting, envelope, orphan hygiene** — **EXECUTED 2026-08-12**,
    all five clauses. No new verb shape, no path argument, no capability: `REQUIRED_CAPS` is
    untouched and stays the one place the set is written; §15.45's five bounds and §15.55's two
    residuals are unchanged. (a) A macOS `Grant` variant taking the same argument shape as the
    Linux arm, which resolves every named adapter **first** — a typo is refused exactly as Linux
    refuses it, because a verb answering "nothing to do" about a port that does not exist is
    clean about the wrong thing — then reports that this arm holds no capability to grant *with*
    and that the callout node's reachability belongs to the machine's device permissions rather
    than to the re-enumeration. Guarded by a new derived meta-gate,
    `every_devprep_verb_answers_on_both_platform_arms`, which **parses** both `enum Verb` blocks
    rather than grepping them (line comments stripped; candidates taken only at the enum's own
    brace depth, so fields and doc comments cannot read as verbs) and asserts set equality in
    **both** directions — a verb macOS accepts and Linux does not is the same defect with the
    platforms swapped. Neither roster is typed; a ninth verb inherits the rule the day it lands.
    (b) `grant_after_reauthorize` returns a `GrantOutcome` value instead of folding a skip into
    an empty success, so `grant_skipped` rides the JSON on both platforms and `hold` emits its
    promised final `up` line before reporting a grant failure. (c) The ~90-line cycle/hold
    envelope factored, the deliberate differences becoming parameters. (d) `scripts/bless`
    enumerates `.snx-bin/<profile>/` and strips capability-carrying files it did not just
    install, the set derived from `REQUIRED_CAPS`. (e) `install`/`install --verify`'s `Ready`
    arm derives its capability line from `REQUIRED_CAPS` instead of printing the pre-§15.55
    single-capability form.
    *Fail-first:* six planted defects, each rebuilt, reddened, restored and re-run green —
    including two that prove a **matcher** rather than a walker (the orphan matcher reimplemented
    as the blessed matcher, and a doc comment naming the missing verb, which a grep would have
    called parity). One ordering finding is recorded because the gate found it about itself: the
    non-vacuity floors originally preceded the parity assertion, so the first fail-first run
    reddened on "only parsed seven" instead of on the sentence naming the defect; the floors now
    follow.
    *One deliberate behaviour change, named:* `Envelope::up()` re-authorizes **every** port even
    after one refuses. `cycle` used to return at the first failure leaving the rest deauthorized
    while `hold` carried on — a divergence nothing in the tree argues for, so the envelope
    resolves it rather than parameterising an accident. Identical on every single-port path; on a
    multi-port write failure `cycle` now also puts the remaining devices back.
    *Unverifiable without root, stated exactly:* clause (d)'s strip could not be exercised
    against a real capability-carrying file — `sudo` needs a password on this box and user
    namespaces are blocked — so the matcher is proven and the `setcap` removal is not.
    **Rig-lane precondition:** `.snx-bin/<profile>/serial-nexus-devprep` is now *Stale* relative
    to the build, and `blessed_devprep_helper()` only **warns** on a byte mismatch (deliberately —
    an ordinary relink changes those bytes), so a rig lane run without `scripts/bless` first would
    silently measure the pre-change helper and report green for code that never ran. The
    superseded filing follows.
    *Evidence:* the macOS verb enum lacks `grant`, violating the file's own clean-answer parity
    contract (an unknown-subcommand error is exactly what its comment bans); §15.55's
    skip-with-a-note promise is silent in JSON mode (`grant_after_reauthorize` notes only when
    `!json`, and `hold`/`cycle --json` take that arm), and a real grant failure returns without
    `hold`'s promised final `up` line; the cycle/hold replug envelope is ~90 duplicated lines
    twice, with the grant step pasted at three sites (one already diverged); and the rename
    left an orphaned blessed copy under the retired name, carrying capabilities that nothing
    references (notes §3.81) — §15.45's narrowness argument does not want capability-carrying files lying
    around. *Remainder:* a no-op macOS `grant` arm answering cleanly; `grant_skipped` carried
    as a JSON field with `hold` emitting `up` before reporting a grant failure; the envelope
    factored so the deliberate cycle/hold differences become parameters; `scripts/bless` (or a
    devprep verb) enumerating `.snx-bin/<profile>/` and stripping or deleting
    capability-carrying files it did not just install, the set derived from `REQUIRED_CAPS`,
    never a hand-kept list; and the last hand-written spellings of the capability set retired in
    favour of derivation — `install`/`install --verify`'s `Ready` arm
    (`devprep/src/linux/mod.rs`, the `println!` reading `ready (mode 0700, cap_dac_override
    +ep)`) still prints the pre-§15.55 single-capability form, and must derive from
    `REQUIRED_CAPS` instead (this evidence line named the `status` verb until 2026-08-12; that
    verb prints no capability line at all, so an implementer grepping `fn status` found
    nothing — corrected in place rather than re-filed). The operator-facing setcap
    *instruction* in the harness and the bless header comment were repaired at the v17
    landing, and the four narrative sites that still promised a one-capability bound
    (`docs/security.md`, `sys/src/caps.rs`, `devprep/src/linux/mod.rs`, `.gitignore`) were
    repaired by the 2026-08-12 alignment pass; this informational line is what remains.
    *Validation:* the rig lane (plan §3) is the surface; §15.45's five
    bounds and §15.55's two residuals unchanged — no new verb shape, path, or capability.
53. **Conformance kit: resync-accounting suite** — **EXECUTED 2026-08-12** (notes §3.86).
    `resync_is_counted(make, malformed_unit, expected_count)` in the kit, opt-in and parameterized
    by the author's own malformed unit — the same own-framing escape `recovers_after_garbage`
    documents, taken as an argument instead of a paragraph. It asserts four things in order: a
    fresh codec reports 0; a `demux` carrying no bytes still reports 0 (a counter that ticks per
    *call* rather than per unit fails here before the malformed unit can supply an excuse); the
    count after the unit is exactly `expected_count`; and a second read agrees with the first,
    because the daemon re-reads `resync_count` on every `state` poll and a reset-on-read counter
    would report the loss once and 0 forever. It refuses its own vacuous parameterization — an
    empty unit or a zero expectation panics naming why. The kit-honesty negative is
    `SilentResyncer`: `GoodFraming` with the override deleted and nothing else changed, proven to
    pass **all eight** other suites and fail only this one. `tinymux` runs it from the consumer
    position, so deleting its counter reddens the consumer-position gate. The superseded filing
    follows. *Evidence:*
    `Codec::resync_count()` is the one trait method no kit suite exercises; the shipped
    consumer-position example (`tinymux`) increments and surfaces it, and deleting that
    increment leaves the whole tree green while node state reports `framing_errors: 0` forever
    — §8's kit-honesty rule affirmatively requires the missing opt-in check and its negative
    codec. The sim corruption recipe covers envelope-framed codecs only; the one shipped
    own-framing example did not replicate it. *Remainder:* an opt-in
    `resync_is_counted(factory, malformed_unit, expected_count)` suite parameterized by the
    author's own malformed unit (the same own-framing escape `recovers_after_garbage` takes),
    with a kit-honesty negative — a resyncer that drains correctly and never increments,
    passing every other suite and failing only this one — and `tinymux` running it from the
    consumer position. *Validation:* the negative codec is the fail-first proof; the tinymux
    counter's deletion reddens the consumer-position gate.
54. **The macOS lint gap** — **EXECUTED 2026-08-12**, by the first of the two remedies the item
    named. The Linux lane's Apple cross-check now runs **clippy** rather than `cargo check` on
    both triples — plus the minimal-daemon spelling on each — so it is strictly the wider
    instrument at nearly the same cost (clippy does everything `check` does and then lints).
    *Fail-first, executed and read at its error text (rule 17 f):* the item's own class was
    planted in `sys/src/lib.rs` — an ungated private fn whose one caller is
    `#[cfg(target_os = "linux")]`, dead off Linux — and the three lanes answered
    `cargo clippy --workspace` (Linux, the lane that exists) **exit 0**,
    `cargo check --target aarch64-apple-darwin` (the lane that exists) **exit 0**, and
    `cargo clippy --target aarch64-apple-darwin` **exit 101**, `error: function
    'item54_planted_helper' is never used`. So neither pre-existing lane saw it and the new one
    does; the plant was removed and `sys/src/lib.rs` left byte-identical. All four invocations
    are green on the unplanted tree, measured the same session. The second remedy (clippy in the
    `macos` job) is **not** taken and is not owed: it costs macOS runner minutes for a class the
    Linux lane now catches at Linux prices. The superseded filing follows. *Evidence:* clippy (workspace and minimal-daemon)
    runs only in the ubuntu `check` job and the `macos` job runs no lint, so the
    cfg-gated-only-caller dead-code class is caught solely by hand runs on a Mac — measured
    when the macOS validation session found the gate had never once executed on the platform
    that fails it; notes §3.71's both-triples argument applies to lints exactly as to compile
    checks. *Remainder:* a Darwin-triple clippy cross-check beside the existing compile
    cross-check on the Linux lane, or clippy in the `macos` job — or a recorded decline.
    *Validation:* a planted macOS-only lint violation reddens the chosen lane (read at its
    error text — rule 17 f).
55. **In-tree comment and dead-code hygiene: the drifted-copy class** — **EXECUTED 2026-08-12**
    for clauses (b), (c), (d), (e) and the reachable half of (a); **(f) and one paragraph of (a)
    are carried**, blocked on `daemon/src/nodes/codec.rs` being owned by a concurrent change and
    refused rather than half-done.
    (a) `daemon.rs`'s verb enumeration is **deleted**, replaced by a statement of why there is no
    list: `docs/rpc/` is the registry of record, `dispatch` is the implementation, and
    `meta_derive`'s verb gate derives both sides from those two — so a prose copy here is ungated
    *by construction*, the gate's scanner stripping line comments before it looks. The drift was
    measured rather than quoted: the deleted list named 10 verbs against a real surface of **22**,
    so **twelve** were missing (`add-node`, `remove-node`, `connect`, `disconnect`, `info`,
    `ports`, `tap.open`, `tap.close`, `tap.wait`, `send-break`, `set-modem`, `pulse-dtr`) — the
    item said "nine-plus". The construction-era framings in `nodes/mod.rs` and `pty.rs` were
    **restated in the present tense** rather than deleted, their surrounding content being
    load-bearing. `codec.rs`'s "Phase 5 implements the demultiplexer" is carried.
    (b) `SOCKET_PROBE_TIMEOUT`'s rationale re-attached to the const, naming `prepare_socket` as
    its one reader and recording that the fusion is why both were misread on sight.
    (c) Both `core/src/config.rs` sites now point at `GraphConfig::effective_write_mode` with
    `Wiring::build` and `GraphConfig::validate` named as its two *callers*, and the reason the
    one-place rule exists (a re-deriving validator misses the promoted shape — review 26 RV-4).
    (d) Three `File::flush` no-ops and the `ok` flag deleted, the no-op **verified in the std
    source before deleting** (`sys::fs::unix::File::flush`'s body is `Ok(())`, shared by Linux and
    Darwin) and `ok` followed to all four of its sites first. §7.3's "flush the queue" is now
    stated as drain-to-`write(2)` where they stood, with log fsync **named as an amendment path
    and not taken** (§16.6 scopes durability to the state file on the ground that config mutations
    are rare; a log at line rate is the opposite case, and the same promise would turn §7.3's
    bounded teardown wait into a bound on the storage stack).
    (f) **Refused as a half hoist, with the reason measured.** `UnboundSet` lives in `leg.rs` and
    `UnconfiguredChannels` in `codec.rs`; sharing needs one type used from both sides, so hoisting
    from `leg.rs` alone would take the copy count from two to **three**. The deltas a future merge
    must survive are recorded rather than guessed: caps identical (256 / 64), `TRUNCATION_MARKER`
    byte-identical, but leg's API is `insert`/`clear` where codec's is `record`/`report_into` with
    a `bytes` field, a first-sighting WARN and no `clear`; the reported counter names are per-kind
    and stay so. `exec.rs` is **not** a third copy — it imports codec's.
    *Fail-first, clause (e):* three plants. Planting the 37-TOOL-2 swallow in `subscribe_stream`
    reddened its guard; planting the identical defect in `tap_stream` reddened **nothing** across
    four suites and eleven tests, which is finding 59(a) below; and after extraction one defect in
    the shared helper reddened **all five guards across both verbs**, which is the "spelled once"
    property made observable. *Clause (d):* no guard covers the deleted code and none can — the
    call's body is `Ok(())` and its result was already discarded — stated plainly rather than
    dressed up; the plausible neighbouring mistake (keep writing into the pre-rotation handle) was
    planted instead and reddened two `p3_log` guards.
    *One deletion beyond the filed text:* `perform_rotation`'s `file: &mut File` parameter, whose
    only use was the deleted flush, so `-D warnings` rejected it once the flush went. Rotation is
    a rename plus a reopen and never needed the handle. The superseded filing follows.
    *Evidence and remainder, as clauses:* (a) `daemon.rs`'s module-doc verb enumeration has
    drifted nine-plus verbs from `dispatch` — an ungated third copy of the verb index; item
    40(b)'s precedent is deletion, the registry of record living in `docs/rpc`; the
    construction-era "Slice 1/Phase 5" framings in `nodes/mod.rs`, `pty.rs`, and `codec.rs`
    retire with it. (b) `SOCKET_PROBE_TIMEOUT`'s five-line rationale is fused onto
    `watch_stdin_eof`'s rustdoc — a different mechanism — while the const sits undocumented;
    re-attach it. (c) `core/src/config.rs` twice points readers at `Wiring::build` as the home
    of the write-mode promotions, contradicting the settled one-place rule
    (`GraphConfig::effective_write_mode`, notes §3.17); correct both. (d) The log writer's
    three `File::flush` no-ops and the `ok` flag that exists to gate one of them are deleted,
    with the meaning stated where they stood: §7.3's "flush the queue" is drain-to-`write(2)`,
    and fsync is deliberately not owed (§16.6 scopes durability to the state file; extending
    it to logs would be an amendment, not a patch). (e) ctl's two ack-consumption loops share
    one `read_ack` helper so the 37-TOOL-2 rule is spelled once. (f) The bounded-identity-set
    core (`UnboundSet`/`UnconfiguredChannels` plus the twin `truncate_identity`) is shared,
    per-kind counter names and per/cumulative semantics unchanged — completing the
    reuse-not-re-derive intent codec.rs already states. *Validation:* comment-only and
    behavior-preserving throughout; existing guards unchanged; (d)'s alternative (log fsync)
    is named as an amendment path, not taken.

### Items 56–58 — filed and executed by the v17 alignment pass against the tree (2026-08-12)

Three defects the pass found that no prior item covered. All are **executed**; they are filed
with numbers because a defect this record did not have a number for is a defect the next review
cannot check was fixed (item 16's lesson).

56. **The three-way CRTSCTS discrimination had a two-valued predicate under it** —
    **EXECUTED 2026-08-12** (notes §3.84). §7.1 clause 7 and §11 both promise honour / honest
    refusal / accept-then-drop, and `serial_nexus_sys::honours_rtscts` discarded the `tcsetattr`
    status, answering on the read-back alone — so an honest refusal was `Ok(false)`,
    indistinguishable from accept-then-drop, and `precheck_flow_control` refused the config, while
    P15 (which does record `tcsetattr_ok`) called the same port `Supported` with the sentence
    "Every named port honoured `CRTSCTS` on read-back". That is §7.1 clause 2's own
    report-says-fine / load-refuses split, with `shipped_predicate_agrees` blind by construction
    because both sides read the same half.
    **Why it survived a generation:** the design stated both shapes and only one was code — clause
    7 the behaviour, clause 2 a two-valued predicate. A reader checking the tree against clause 2
    found agreement. *The tree moved to clause 7*: `RtsCtsOutcome::{Honoured, Refused,
    AcceptedThenDropped}` plus `Err`, read-back asked first (a port already carrying CRTSCTS reads
    back set whatever the set returned), every caller migrated with no two-valued shim kept —
    the bool shape is what let two callers collapse different facts into one answer. Clause 2 then
    moved to describe the tree it now has.
    **The design's premise was verified by measurement, not by reading**, which is the part worth
    keeping: clause 7 rests on "an honest refusal makes the node's own open fail loudly". No driver
    in reach refuses `CRTSCTS` — but a Linux pts refuses `PARENB` through the identical `serial2`
    code path, and it was measured (`parity=none` → open `Ok`; `parity=even` → open
    `Err(failed to apply some or all settings)`). That measurement is now a committed guard rather
    than a source reading a version bump could falsify. Five plants reddened and restored.
    Neither `probe_set` nor `field_set` moved, proven three ways, so no era row is owed.
    *Honestly labelled limit:* no device-level fail-first exists for the honest-refusal arm on this
    bench — measured, not assumed (both rig ports read `honoured_on_readback: true`,
    `tcsetattr_ok: true`, cflag delta exactly `CRTSCTS`) — so that arm is guarded structurally
    against injected readings, which is what AGENTS §9 asks when only one arm is reachable.

57. **Four operator-facing remedies named a config key the schema denies** — **EXECUTED
    2026-08-12** (notes §3.84). The serial node's open failure text (both arms), the
    `precheck_flow_control` refusal, and P15's consequence all offered `flow = "none"` as **the
    remedy**; the key is `flow_control` under `deny_unknown_fields` with no key alias anywhere in
    the tree, so an operator following the refusal got a second error naming a different problem:
    `unknown field \`flow\`, expected one of \`name\`, \`device\`, \`baud\`, …`. **Proven by
    execution, not by reading the struct.** Two tests asserted the wrong spelling and pinned it.
    Rule 11's bar failing on the box that prints the message, and the exact failure §15.53 exists
    to prevent — the refusal was built so nobody would meet `serial2`'s bare "failed to apply some
    or all settings", and then handed them a key the parser denies. Twenty-two spellings repaired
    across five files, the design's two prose shorthands included; the two tests now assert
    `flow_control` and are the tripwire. **Found inside a REFUTED finding** — the alignment pass's
    verifier correctly refuted the *framing* (the design never states the key normatively) and let
    the confirmed sub-facts go. The procedural rule that follows is at notes §3.84: a refutation of
    a framing must dispose of the observations it leaves standing.

58. **`fuzz/Cargo.lock` had been stale since item 47, and the fuzz-nightly lane was red-in-waiting**
    — **EXECUTED 2026-08-12** (notes §3.84). The daemon's `regex-automata` edge never reached the
    excluded crate's lock, and item 51 added a second (`nix` on `serial_nexus_rpc`). CI's
    `fuzz-nightly` runs `cargo +nightly fetch --locked` for exactly this reason — cargo-fuzz takes
    no `--locked` of its own — so the next scheduled run would have failed on a lock nobody had
    looked at since 2026-08-12's landing. Refreshed; the diff is two lines and `cargo fetch
    --locked` exits 0. The gate was right and the lock was wrong, which is the outcome a gate is
    for. *Standing consequence:* a change adding a dependency edge to `serial_nexus_daemon`,
    `serial_nexus_web` or `serial_nexus_rpc` must refresh this lock in the same commit — it is a
    second lockfile the workspace build never touches.

59. **Item 55's residue: the tap ack guard, the fourth framing, the third ack copy, and (f)** —
    **(a) EXECUTED 2026-08-13** (notes §3.91): two guards in `p12_ctl_tap.rs` — the stand-in-socket
    refusal, which needs no daemon, plus the arm the shipped daemon can produce (`tap.open` on an
    unknown endpoint). Fail-first reproduced the item's own measurement exactly: with the
    ack-swallow planted in `tap_stream`, both new guards redden while `p12_ctl_subscribe`,
    `p12_tap_replay` and `p8_replay_ring` stay green — which is what said the tap half was
    unguarded in the first place. **(b), (c) and (d) remain open** (S; four independent clauses, none blocked on anything). Filed 2026-08-12 by item
    55's own execution rather than swept into it, because three of the four are outside that
    item's filed scope and the fourth is a file-ownership conflict, not a difficulty.
    *(a) The tap half of the 37-TOOL-2 rule has no guard at all* — **measured, not inferred**:
    planting the ack-swallow into `tap_stream` alone reddened nothing across `p12_ctl_tap`,
    `p12_ctl_subscribe`, `p12_tap_replay` and `p8_replay_ring`, eleven tests, all green. Item
    55(e)'s shared helper now defends the *class* through the subscribe guard, so this is a
    coverage hole rather than a live defect — but a tap-specific refusal (unknown or
    non-host-facing endpoint) is still asserted by nothing. The fix is `p12_ctl_subscribe.rs`'s
    twenty-line stand-in-socket pattern re-pointed at `tap.open`; it needs no daemon.
    *(b) `core/src/state.rs`'s head carries a fourth construction-era framing* — "Fleshed out with
    counters per boundary in phase 3; phase 1 establishes only the status vocabulary and the
    split" — which item 55(a) did not name and which is stale twice over, the counters existing.
    *(c) After 55(e) the ack rule is spelled twice tree-wide, not once:* `web/src/wsclient.rs`
    carries a third instance over WebSocket frames. It cannot literally share the `BufRead`-shaped
    `read_ack`, and it has **already diverged in the observable that matters** — it prints
    `tap.open failed: {err}`, the whole error object, where `ctl` prints `{message} ({code})`.
    A decision is owed (unify the message, or record the divergence deliberately), since 55(e)'s
    premise was that this rule should read the same everywhere.
    *(d) Item 55's carried clauses:* `codec.rs`'s construction-era framing, and the
    `UnboundSet`/`UnconfiguredChannels` hoist, which needs one change owning both `leg.rs` and
    `codec.rs`. The merge deltas are recorded at item 55 so the next attempt does not re-derive
    them.
    *Validation:* (a) is fail-first by construction — the planted swallow must redden the new
    guard; (b) and (c) are comment/decision work; (d) inherits item 55's "no half hoist" rule.

### Items 60–64 — the guard audit (2026-08-12)

Filed from an audit that asked of every recorded measure: **is there a guard, and could that
guard actually fail?** Rule 22's tell was the instrument — would its passing output differ from
its not-running output. The audit's headline is worth keeping: coverage is **better** than the
question implied, and roughly a dozen invariants are guarded to a standard the auditor could not
break by reading (unsafe containment, the `RefCell` and `AsyncFd` bans, the teardown ledger, the
loss fingerprint, head-of-line, the web bridge's deny-by-default, the derived rosters, the golden
transcripts, the pattern-wait maxima). What follows is the residue.

60. **Four gates that could not fail, closed** — **EXECUTED 2026-08-12**.
    (a) **`SNX_TLS=required` was set by no CI lane.** The mechanism has existed since item 10 and
    `web_tls_round_trip` is the only proof the TLS tier's *handshake* works rather than that it
    binds — carrying two silent skip causes. Now set on the Linux lane, which has `curl` and can
    bind, so a skip there is a lane that lost a prerequisite.
    (b) **The Linux `check` job never installed `jq`**, and it is the only job that runs the suite
    on Linux. `expectation_gates.rs`'s seventeen synthetic-antecedent guards — the machinery
    §13's gate design and rule 22 rest on — call `have_jq()` and return green without it, and
    `platform_expectation()` picks by `cfg!(target_os)`, so `expectations/linux.jq`'s entire
    clause-level guard set was running *by luck of the runner image*. Install step added.
    (c) **The Linux lane lacked `--no-fail-fast`** while macOS has it with a comment explaining
    why it cost six consecutive pushes. Added.
    (d) **The macOS lane did not set `SNX_EXEC_CODEC=required`**, so item 49's battery was
    silently skippable on exactly the platform that lane exists to cover. Added.
    (f) **`expectations/macos.jq` did not refuse `unsupported`, and its own comment said it did** —
    text copied from `linux.jq`, where it is true. Six probe clauses were bare presence checks with
    no status constraint. Repaired 2026-08-13 with the readings verified first, against a
    Darwin-shaped report: at HEAD, `unsupported` on P1/P3/P4/P5/P11 exited **0** where `linux.jq`
    exits 1, and so did a `skipped` on P6/P7; after the repair all four exit 1 and an honest report
    still exits 0. **One correction to the audit's reading:** on a *passive* run P15 was already
    refused — but by the per-port presence clause, a presence clause doing a status clause's job by
    accident; name a port and it passed. The clause is now the leading one, P6/P7 match Linux's
    by-enumeration spelling exactly rather than more strictly, and `docs/macos.md`'s paragraph —
    which had become a *rationale* for the absence — is corrected in place. A synthetic-antecedent
    guard proves the new clause can fail, which none of the unconditional clauses in either file
    had.
    (e) **`REQUIRED_CAPS.len() >= 2` was a floor**, so a third capability — which AGENTS §4 and
    §15.45 call a design *amendment* — reddened nothing. It is now `assert_eq!` with a message
    naming the amend-first order, and **fail-first proven**: planting `cap_sys_admin` reddens it
    (plus three sibling tests), restored byte-identical, 18 passed.

61. **The throughput axis has no guard at its recorded value** — **open** (M; a measurement, then
    a guard). *Evidence:* `docs/benchmarks/phase3.json` records **183.0 MiB/s** and an exit
    criterion of **30 MiB/s**; the only test on that axis, `p3_firehose.rs`, streams 256 MiB under
    a 60 s deadline, so it fails only below **4.27 MiB/s** — 43× under the recorded figure and
    **7× under the recorded exit criterion**, which means the guard admits a daemon that cannot
    meet the design's own stated headroom. It never measures or prints elapsed time, so 183 → 20
    MiB/s is invisible in both the pass and fail paths, and unlike `idle_cost` the throughput
    block carries **no provenance** — no box, no date, no commit. This is exactly the state
    `idle_cost` was in before item 46, and the same defect review 37 filed as 37-TEST-3, fixed for
    one axis of one artifact and not the other. *It also orphans a tripwire:* §5's ring-storage
    row names this benchmark as its detector, and a `VecDeque<u8>` drain+extend rewrite would
    clear 4.27 MiB/s comfortably — so **that tripwire is currently upheld by review**.
    *Remainder:* item 46's shape — time the transfer, print the reading, assert against a factor
    of a **measured** `throughput` provenance block. The ceiling must be measured before the
    factor is chosen (AGENTS §8), and the sim source's own rate is the confound to check first.

62. **§16.12's exhaustiveness is per-field, and is already violated** — **open** (S/M).
    *Evidence:* the promise is "**every** numeric attribute and every wire-riding identifier
    carries a stated, structurally checked maximum", but enforcement is seven hand-written
    `range_error(…)` sites inside `GraphConfig::validate()`, each behind an `if let NodeConfig::X
    { …, .. }` whose `..` means a new field triggers no compiler complaint, plus a property test
    over a **fixed list of four fields**. The gap has bitten: **`NodeConfig::Pty {
    advertised_baud: u32 }` has no range check at all** — it rides the wire into `state` and feeds
    `apply_baseline` → `standard_baud()`, which returns `Option` and silently ignores a nonstandard
    value. Benign today, a literal violation of the promise, and exactly what an exhaustiveness
    guard exists to prevent (contrast `Serial { baud }`, which is checked). *Remainder:* a
    derive-from-code gate in `meta_derive`'s existing style — enumerate `NodeConfig`'s numeric
    fields from source, require each in a `range_error` site or a named exemption — plus the
    missing check. *Validation:* the gate reddens on a planted unchecked field; the exemption list
    is two-sided.

63. **The observation surface has no doc-parity gate, and has drifted** — **open** (M).
    *Evidence:* §5 says `docs/rpc/observation.md` is "the authoritative per-kind enumeration and
    **stays so**", and the tree gates the *error-code* table and the *verb index* two ways — but
    the largest surface is checked only by hand-written per-field tests that cite the doc without
    reading it. Measured drift: **17 keys the daemon emits appear nowhere under `docs/rpc/`**,
    including `delivered_hostward` — which the design itself names at §5 as the counter
    `p6_head_of_line.rs` reads — plus `accepted_targetward`, `client_present`, `reconnect_count`,
    `identity_kind`, `pts_path`, `modem_lines`, `protocol_version`. This is the drift class that
    produced §15.54, where the taxonomy had to be corrected because four shipped counters
    falsified it. *Remainder:* a fifth `meta_derive` gate — enumerate `"key":` literals from the
    node `state` builders, require each in `docs/rpc/*.md` or a named exemption, floors on both
    sides.

64. **Second-tier audit residue** — **open** (S each; independent clauses).
    *(a)* The pattern wait's **active-path cost is unmeasured**: `ScanWindow::scan()` rescans the
    whole window per chunk on the single runtime thread, `MAX_LOOKBACK` is a scan cost as well as
    an allocation, and `waits` is iterated per chunk with no cap on its length. Item 46 established
    only that *idle* cost is unaffected. One test — arm a max-lookback wait on the firehose graph
    and assert the throughput floor — needs item 61 first.
    *(b)* §10 clause 7's "an armed wait never affects `discarded_unattached`" is asserted by
    nothing (`grep` of the battery returns 0). Structurally independent today, so a coverage hole;
    two lines in the existing ring-off guard. Schedule beside item 59(a).
    *(c)* The jq **clause-identity** guards are incomplete: seven "identical to the other file"
    pairs are byte-identical today but only some have identity tests — P9's
    `zero_timeout_by_fd_state`, P10's `peer_pending` and recheck-ladder, P12's anti-spin (which has
    *no antecedent and no plant*), and `build.commit`/`build.probe_set` are unheld. **No plant ever
    hands back a 1-rung ladder**, the exact defect `macos.jq`'s comment says the gate could not
    previously see.
    *(d)* **No gate asserts that every probe has a jq clause** — `meta_derive` names this itself as
    acknowledged-and-unowned, so a probe added with no `.id == "PN"` clause is ungated on both
    platforms.
    *(e)* `AppError::ALL` is **hand-kept**: the docs↔registry gate is two-way over `ALL`, not over
    the enum, so a new variant omitted from it compiles, is emittable, and is silently
    undocumented.
    *(f)* §16.6's `atomic_write_replaces_durably` **passes with both `fsync` calls deleted** — it
    asserts the atomic-write contract, and its own comment concedes it. Either rename it to what it
    asserts (honest and free) or drive it under `strace` behind a required mode.
    *(h)* **A broken stop-flag propagation is caught only as a hang.** Item 42 measured it: three
    boundary guards deadlock rather than fail, and `cargo test` has no per-test timeout, so in CI
    that is a job timeout with **no failing test name** — the one thing AGENTS §8 says to capture
    verbatim, absent. A bounded wait in
    `blocking_reader_stop_join_ends_a_running_thread` and
    `signal_stop_returns_before_the_thread_exits_and_join_waits_for_it` would make the class redden
    with a name.
    *(i)* **The pty's writer-spawn failure path has no guard at all** — `pty.rs` faults the node on
    `spawn pty writer thread: {e}`, a §15.8 environmental-failure arm, while the log's identical arm
    has both a unit guard and an itest. Found by item 42 and reported rather than fixed, that item's
    scope being "no behaviour delta, no new coverage". Now cheap: `arm_with` is the seam and is
    already public.
    *(g)* "One shared helper" rules (§16.1's boundary supervisor, §16.4's three purge instances,
    the one fragmenter) each have every *instance* tested and nothing forbidding a fourth
    hand-rolled one; §16.11 has nothing stopping a `.sh` reappearing under `scripts/validate/`.

65. **The exec-child orphan class: one instance fixed, one live, one surface uncovered** —
    **PARTLY EXECUTED 2026-08-13** (notes §3.91). Found by item 50's agent as **260 accumulated
    orphans** on the development box, from code that had landed the same day with a green suite.
    *(a) EXECUTED — the transcript child.* Root cause confirmed by `strace` on a live specimen and
    it is not what anyone guessed: **`std::io::Stdin` is a plain, non-reentrant `Mutex`** — only
    `Stdout` and `Stderr` use `ReentrantLock` — and `park_transcript` called `stdin().lock()` while
    its caller's `StdinLock` was still alive. A **same-thread self-deadlock, before its first
    read**, so the leash's EOF arrived at a process that had stopped listening. The orchestrator's
    offered hypothesis (a *different* thread holding a reentrant lock) is **refuted in both
    halves**. Fixed by passing the caller's reader in rather than reaching for the global; it still
    parks rather than exits, so a restart cannot overwrite the evidence, and §15.36's idle rule was
    re-measured on the fixed binary (`wchan=anon_pipe_read`, **0 CPU ticks over 5 s**).
    *(b) EXECUTED — the guard, as a harness property rather than one test's assertion.*
    `Daemon::start` now spawns with `process_group(0)` and `Daemon::drop` sweeps the group,
    waits bounded, kills what is left and **panics naming pids and argv**. The process *group* is
    the right relation because the parent link is exactly what has been destroyed by the time
    anyone can ask — the children are on ppid 1. `ps` failure panics rather than reading as
    "nothing found", so the sweep cannot pass vacuously, and a planted stdin-ignoring child
    (`/bin/sh -c 'exec sleep 300'` — nothing serial_nexus about it, since the sweep matches the
    group and not a name) proves it is not.
    *(c) OPEN — `tests/ext-codec/deaf.py` is a second live instance*, masked today only because its
    one test calls `remove-node` first and takes the graceful path. Its docstring claims "nothing
    here outlives the node that started it", which is true **only where the daemon gets to run
    code**. Now guarded: any test that loads it and lets the `Daemon` drop reddens.
    *(d) OPEN — `serial-nexus-web` has no §15.43 leash arm at all.* `WebServer::spawn` has
    `KillOnDrop` and no piped stdin, and the binary has no `--exit-on-stdin-eof` flag. A live
    orphan was found on the box. Daemon ✓, sim ✓, web ✗.
    *(e) OPEN — the latent sibling.* `stdin_eof_watch()` parks a background thread holding
    `stdin.lock()` for the process's whole life, so **any** future sim mode that both arms the
    leash and reads stdin on the main thread deadlocks identically. Unreachable today only because
    `transcript` is the sole stdin reader and refuses the leash — and **that refusal has no test**.
    *(f) OPEN — a policy question worth deciding rather than inheriting.* `Daemon::drop` calls
    `rpc.shutdown()` and then SIGKILLs immediately, so the kill wins the race against the graceful
    teardown that would have reaped the child. The tests therefore never exercise the daemon's real
    teardown path on this axis. Letting `shutdown` land first would have contained this leak; it
    would also add a wait to ~284 call sites, which is why it was not done blind.

### Items 66–69 — filed by the macOS session (2026-08-13)

Filed by the session that took the era's macOS capture (notes §3.93). Two are the work its
pre-registered readings licensed (66, 67); two are defects the same session found by running the
suite and reading CI (68, 69). All four are numbered because a defect this record has no number
for is one the next review cannot check was fixed — item 16's lesson.

66. **`SlaveWitness::prove_open` is unsound on Darwin: the portable upgrade** — **EXECUTED
    2026-08-13** (notes §3.94), as `serial_nexus_sys::peer_hungup` plus a fourth step in
    `prove_open`. **Added, not swapped**, and the brief that said "replace" was wrong about
    that: the stat comparison catches something `poll` cannot see — a node *replaced*
    underneath a still-valid fd, which is §12's replug renumbering — while `poll` catches
    what the stat comparison cannot see on Darwin. Two arms, two failure modes, two
    messages; the hangup arm runs **last** so Linux keeps the more specific "the node was
    unlinked" diagnosis and the new arm is the backstop where step 2 structurally cannot
    answer.
    **One measurement decided the implementation, and it is the kind that only shows up on
    the platform of record.** POSIX says `POLLHUP` is delivered whatever the caller
    requested; Linux does that, and **Darwin gates it on the requested mask** — so a helper
    written and self-tested on Linux with an empty mask passes there and is *silently dead*
    on the only platform that needs it. Measured three ways rather than read: P9 already
    ships the cell (`hangup_delivered_to_a_mask_that_requested_nothing`, `true` on Linux,
    `false` on Darwin, in the committed captures of this era), an independent poll of a
    `posix_openpt` pair on the rig box reproduced it from outside the tree, and the planted
    empty mask reddens the new self-test **here and would not on Linux**. The rule is spelled
    once, in the helper, so it cannot be re-derived wrongly by a caller.
    *Also measured, and it is why adding this to a shared witness is safe rather than merely
    convenient:* a real serial fd **never** reports `POLLHUP` — checked on the rig with the
    far end open and after it closed, mask requested both times — so the arm cannot fire on
    the serial witnesses in `p4_purge`, `p4_exclusivity`, `p4_free_for_all` and `p6_outage`,
    which keep being held by steps 1–3 exactly as before. That is P16's own
    `does_not_license` sentence, confirmed against the hardware it disclaims.
    *Fail-first, both halves, each restored byte-identical:* the mask planted back to empty
    reddens `peer_hungup_is_absent_while_the_master_is_open_and_present_after_it_closes`;
    the fourth step disabled reddens the new Darwin witness guard with the message that
    names the consequence ("on this kernel that leaves every guard built on it asserting
    nothing"). The Darwin guard asserts the **platform fact in the same run as the product
    property** — the path still resolves after the master closes — so if a future Darwin
    started unlinking, it reports that rather than passing for Linux's reason.
    *Owed:* the Linux half of the fail-first for step 4 specifically. Both new guards run on
    Linux and pass there, but the arm's *reddening* has been proven on Darwin only, this box
    being the only one this session had. The superseded filing follows. **Original state:** open (S/M).
    *Evidence:* measured, not predicted, and pre-registered by item 26 before it was taken —
    `path_still_resolves` reads `true` on **both** sides of the master's close on Darwin 24.6.0,
    `fstat_on_the_held_fd_answers` stays `true`, `shipped_prove_open_would_refuse` never moves off
    `false`, and `stat_comparison_can_tell` is `false` (three runs,
    `docs/doctor/macos-24.6.0-2026-08-13-b346188-tier3*.json`). Linux reads the same cells
    `true` → `false` and `stat_comparison_can_tell: true`, so the shipped comparison is sound
    there and inert as a control. The seven guards notes §3.56 converted are therefore held on
    Darwin **by the compile-time borrow alone** — the witness argument's other half is absent on
    the platform where it was supposed to be the addition. *Remainder:* the upgrade item 26's
    conditional names — a `poll(POLLHUP)` liveness check behind a `serial_nexus_sys` helper
    (§16.3 puts the `unsafe` there and nowhere else), matching the instrument that measured it
    (P16 polls the held slave fd), with `prove_open` re-pointed at it. The measured margin is
    generous: `POLLHUP` arrives 6–16 µs after the close on Darwin and 1 µs on Linux, against a
    quiet window that stayed quiet through 200 tight and 64 paced passes on both. *Declined
    in advance:* keeping the `stat` comparison as a Linux-only arm beside the poll — two
    instruments where one answers on both kernels is the two-copies-that-must-agree shape
    §16.5 exists to ban, and the tell is that only one of them has ever been able to fail here.
    *Validation:* fail-first **on both kernels**, since this is the item that exists because a
    check passed vacuously on one of them; the guard asserts the property the product promises
    (the pair is gone) rather than a path lookup that answers it only on Linux (AGENTS §9).
67. **The `xon-xoff` refusal: §15.53 extended, now that a dropping driver is measured** —
    **EXECUTED 2026-08-13** (§15.61; notes §3.95), design amended first as AGENTS §5 requires:
    §15.61 written, §7.1's flow-control clauses 1, 2 and 7 restated, and only then the tree.
    **The predicate generalized rather than being copied**: `honours_rtscts` →
    `honours_flow_control(path, FlowMode)`, `RtsCtsOutcome` → `FlowOutcome`, one
    implementation for both modes because the only difference is which flag in which termios
    word is written and read back — the three-way classification was already a pure function
    of two booleans. Copying it per mode is the two-copies-that-must-agree shape §16.5 bans,
    and that shape is not hypothetical here: it is how the daemon and P15 came to answer
    differently about one port in the first place (item 56).
    **Both arms are proven on one box, which is what makes this a discrimination rather than a
    blanket refusal.** The rig's FT232R is refused (`c_iflag` `0x0` → `0x0` with `tcsetattr`
    reporting success) and a Darwin **pts** beside it is not (`0x2b02` → `0x2f02`, honoured) —
    so the new rig guard `xon_xoff_is_refused_at_load_exactly_where_the_driver_drops_it`
    asserts the refusal *and* the non-refusal, each against real hardware, and a `sys`
    self-test asserts the honouring arm on a tty every box has. A refusal rule proven only
    against ports it refuses is not proven.
    **Three defects the change surfaced, all repaired:** (a) the remedy string offered
    `flow_control = "none"` **(or `xon-xoff`)** for a dropped `rts-cts`, which on the platform
    of record sends the operator from a structural refusal into the exact late fault the
    refusal prevents — the same driver drops both; (b) `open_failure_text` described the
    software mode's failure in `CRTSCTS`'s words until it was parameterized, which would have
    explained a failure with a measurement of a different flag; (c) P15's shipped
    `does_not_license` string told the reader that nothing in the daemon consults the software
    reading and that such a config is "refused at neither `load` nor `add-node`" — true when
    written and false the moment this landed, which is §7.1 clause 2's split in its most
    misleading direction.
    *Fail-first, and it arrived unasked:* two pre-existing daemon guards reddened on the
    behaviour change — one whose table listed `(AcceptedThenDropped, XonXoff)` under "node
    never asked for it", which is precisely the expectation §15.61 overturns. That is the
    handshake a guard is for, and the table entry moved to its own assertion rather than being
    deleted. **The owed remainder was discharged 2026-08-14** (notes §3.101, item 72). It read:
    "the honouring **serial** arm is measured on a pts and on Linux's `ftdi_sio` through the
    committed captures, not by a rig guard executing on Linux — this session had one box." The
    rig guard has now taken its `Honoured` arm on Linux hardware, and that arm had executed on
    **no machine anywhere** — the Mac takes the accept-then-drop one — so the discrimination is
    proven against hardware on both kernels with the same two adapters and the same cable, rather
    than against one kernel plus an artifact. *What that session did not finish, and item 72 did:*
    §15.61 reached the daemon and P15's `does_not_license` cell, but not `p15_soft_note`'s three
    arms, the probe's own doc, §7.1 clause 6's cost bound, or §5's predicate name.
    The superseded filing follows. **Original state:** open (M; design
    amendment first, per AGENTS §5). *Evidence:* item 14's decline was
    **conditional** — "the refusal follows only if a dropping driver is found" — and the
    condition is met: Darwin's `IOSerialFamily` accepts `IXON|IXOFF` (`tcsetattr_ok: true`,
    `tcsetattr_error: null`) and reads back `c_iflag` `0x0` → `0x0`, a delta of nothing, with
    `serial2_readback_would_fault: true`, on both ports, 6 of 6, against `ftdi_sio` honouring it
    (`0x5` → `0x1405`) on **the same two adapters** one kernel away. So a node configured
    `flow_control = "xon-xoff"` on such a port faults at its own open with `serial2`'s bare
    `failed to apply some or all settings` — precisely the outcome §15.53's refusal exists to
    prevent, now demonstrated for the second of the two modes. *Remainder:* (a) the design
    amendment — a new §15 entry extending §15.53's refusal to the software mode, and §7.1's
    flow-control clause 7 restated, its sentence "`xon-xoff` has no pre-check and no probe —
    unmeasured rather than known-good" having been overtaken by measurement. (b) A software
    analogue of `honours_rtscts`'s tri-state in `serial_nexus_sys`, read-back asked **first**
    for item 56's recorded reason (a port already carrying the flag reads back set whatever the
    set returned). (c) `precheck_flow_control` extended, with the remedy text spelling
    `flow_control` and not `flow` — item 57's twenty-two-site repair is the tripwire, and this
    is the change most likely to reintroduce it. **(d) A remedy string that is now wrong on the
    platform of record, and would get worse:** §7.1 clause 1 offers `flow_control = "none"` (or
    `xon-xoff`) as the remedy for a dropped `rts-cts`, and on Darwin `xon-xoff` is dropped by the
    same driver — so the refusal currently points the operator at a second mode that fails the
    same way. *Validation:* fail-first on **both** arms — a dropping port (this rig) must be
    refused, and a honouring port (the Linux rig, same adapters) must **not** be, which is the
    pair that proves the predicate discriminates rather than refuses everything; the harness
    assertion that a refused `load` created **nothing** rides with it, as §15.53 requires.
68. **The packaging root arm reddened CI on every run it ever had, and its message blamed the
    wrong subject** — **EXECUTED 2026-08-13**, flip-back included (notes §3.99). *Executed 2026-08-13:* the root cause
    and the repair. `dynamic_user_state_directory_is_private_and_read_write_paths_do_not_chown`
    put its `ReadWritePaths=` probe directories under `std::env::temp_dir()` while the packaged
    unit sets `PrivateTmp=yes`, so the listed path did not exist inside the service's namespace
    and systemd refused to build it — `status=226`, `EXIT_NAMESPACE`, before `ExecStart`. The
    probe's own assertion said "This is a finding about the packaged sandbox, not about the
    probe"; it was a finding about the probe, and the message is corrected to route by the
    systemd status word rather than by assumption. Directories moved out of `/tmp`, which
    `PrivateTmp=` privatises, into `/run` — which it does not. **The CI step also did not do
    what its own name says:** it is called "reporting only" and gated, which is plan §18 item
    31's sequencing inverted — it now carries `continue-on-error: true`.
    **The measurement that repair was labelled for was taken the same day (CI run
    31689537882) and it moved the failure one stage forward, which is the point of labelling
    it.** `EXIT_NAMESPACE` is gone: the probe unit **runs**, prints its readings, and fails a
    *later* assertion — `listed_write=ok` on a directory the test had just created root-owned
    0755, where the README's claim (`ReadWritePaths` only flips the mount, it does not chown)
    predicts a refusal. **The second cause was the repair's own, and it is the more
    instructive one.** `service_properties` renames the unit's three `*Directory=` values to
    `tag`, so `RuntimeDirectory=<tag>` had systemd create — and, under `DynamicUser=yes`,
    **chown** — `/run/<tag>`, the exact path the repair had just moved the probe directories
    into. The ownership control was handed to the very user whose writes it exists to refuse,
    and it died **silently**, which is strictly worse than the 226 it replaced. This item's
    own first draft argued the move was safe *because* "`RuntimeDirectory=` already writes
    into" `/run` — the collision written down as the reassurance. Repaired again as
    `/run/<tag>-scratch`: two paths that must never be one, distinguished by their names
    rather than by a comment.
    **Third stage, and the message written at the first stage is what found it.** With the
    scratch tree moved, CI reported `status=2` — and the assertion's own routing table says
    "usual exit codes at the payload", so nobody re-litigated the namespace. The readings that
    did arrive are all *correct*: `uid=64190`, `user=snx-pkg-probe-…`, `state_write=ok`,
    `state_stat=root:777`, `state_real=/var/lib/private/…`, and then
    `cannot create /run/…-scratch/rw-listed/probe.txt: Permission denied` — which is the
    README's claim confirmed, the write into a `ReadWritePaths=` directory being refused
    because the mount flip does not chown. **The probe was dying on its own success.** Cause:
    the script tested the write with `: > file`, and `:` is a POSIX **special** built-in, for
    which a redirection error "shall cause the shell to exit". `/bin/sh` is dash on the runner,
    so the refusal killed the script mid-run and `set +e` could not help. **Verified on dash
    itself rather than reasoned this time** — this box has `/bin/dash`: `: >` into a read-only
    directory prints the message, exits **2**, and produces no further output (CI's signature
    exactly), while `true >` prints `write=denied`, reaches the end, and exits **0**. `true` is
    a regular built-in. Repaired at both call sites.
    **Fourth stage, and the last of the same gotcha.** With `true` in place the probe ran to
    completion and reached a *property* assertion for the first time — and the property is
    **passing**: the write into the listed directory is refused, which is the README's claim.
    What failed is the errno capture, `listed_write=fail:` with nothing after the colon.
    Redirections are applied left to right, so `> file 2>/tmp/e2` fails on `> file` and never
    applies the `2>`; the diagnostic went to the inherited stderr, which is exactly where CI
    had been showing it all along. Verified on dash again rather than reasoned: the old order
    reads `fail:`, the new one `fail:/bin/dash: 2: cannot create …: Permission denied`, which
    is what the downstream assertion needs to tell EACCES from EROFS. Reordered at both sites.
    **Fifth stage, and it is the *control* this time.** The errno capture fixed, CI read
    `listed_write=fail:… Permission denied` — Claim 4's ownership arm **passing**, the README
    confirmed — and failed on the arm beside it with `unlisted_write=ok`. `/run` is writable
    for services, so `ProtectSystem=strict` never refuses anything there and the control cannot
    produce the `EROFS` that separates "the mount flipped" from "the ownership changed". Moved
    to `/var/lib/<tag>-scratch`: `/var` *is* read-only under strict, so the unlisted sibling
    gets `EROFS` while the listed one, remounted read-write and root-owned 0755, gets `EACCES`.
    A sibling of `StateDirectory`'s `/var/lib/<tag>`, never inside it. **The four rejected
    locations and why each failed are now written at the code**, since the requirement turns
    out to be exact and each candidate fails differently.
    **The pattern across the five stages is the item's most transferable content**: each fix
    revealed the next defect, every one of them the probe's rather than the packaged unit's,
    and the property under test was never the thing failing — by stage five one of Claim 4's
    two arms is confirmed. A test that cannot run has no verdict, and five different mechanisms
    conspired to make "cannot run" look like "the claim is false".
    **Green, and the escape hatch is gone.** CI run 31695823765: the root arm reads **6 passed,
    0 failed**, the probe running under `DynamicUser=yes` as `uid=65180`, printing all nine
    readings — `state_stat=root:777`, `state_real=/var/lib/private/snx-pkg-probe-3041`,
    `private_list=ok`, `private_stat=root:755`, `host_link=private/…` — with **both** halves of
    Claim 4 holding. So **plan §18 item 31(c)'s owed measurement has executed for the first
    time**, `continue-on-error` is removed, and a regression here fails the lane again. The
    debt is closed on the terms it was taken on: a step that asserts nothing is AGENTS §3's
    tell, and it was carried as a numbered item rather than left as a comment for exactly this
    reason. *Validation:* the flip-back is the item's
    close; a run that is still red names its new status word or its new failing assertion
    rather than re-asserting any of the three diagnoses.
69. **The macOS lane's two Linux-shaped guards, and what they say about coverage** —
    **EXECUTED 2026-08-13** (notes §3.93), filed rather than fixed silently because both are
    instances of a class this ledger already tracks. Found by running the suite on the Mac rig
    box; both had reddened CI's `macos` job on every push since they landed. (a)
    `probes::tests::the_software_readback_reports_unmeasurable_rather_than_answering` took its
    baseline `Termios` off a **pty master**, which Linux answers and Darwin refuses with
    `ENOTTY` — so the test died in its own setup on the platform of record. Repaired by reading
    the *slave*, a terminal on both kernels, which is what every other pty test in that module
    already does. (b) `both_gates_refuse_an_unsupported_verdict_and_are_shown_able_to` assumed
    "the one cross-platform difference is P12" and so could shape a report for the other
    platform's file only from Linux; from a Mac it panicked on its own precondition, **blaming a
    drift that had not happened**. Measured rather than reasoned: splitting `linux.jq` into its
    33 top-level conjuncts and evaluating each against the P12-shaped Darwin report, **exactly
    one fails** — P2 must be `supported`, §7.2's BSD arm, which `docs/macos.md` already names as
    the expected macOS answer. So the premise needed one more cell, not a smaller scope, and the
    guard now genuinely exercises both files from either box. *The class, which is the reason
    this is an item and not a commit message:* both are **proxies in space** in AGENTS §9's
    sense, and both were written on Linux by sessions that could not run them here — the same
    shape item 12 is open for, and evidence that the shape recurs rather than being one guard's
    accident. (b) is worse than (a) in the way that matters: (a) fails loudly, while (b) fails
    with a message that sends the next reader to audit two files that had not drifted.

70. **The rig primer left its own payload on the wire, and its comment said it could not** —
    **EXECUTED 2026-08-13** (notes §3.95). Found by running the rig binary on the Mac, and
    **pre-existing**: `crossover_rig_map_node_both_directions` failed **4 of 4** at
    `e5a305f` in a clean `git worktree` with its own target dir — the attribution taken by
    reverting to an unchanged tree rather than by reasoning, AGENTS §8's rule applied before
    any diagnosis was offered.
    *Evidence, and it is complete rather than plausible:* the failure is **62 bytes** of
    unexpected data prepended to the test's own correct output, and 62 is one FTDI bulk
    packet's payload (64 minus the two status bytes). Those 62 bytes are a **contiguous slice
    of `prime_the_wire_once`'s own seed-9001 stream** — located at offsets 833 and 789 of the
    1 KiB in two separate failures, matching 62 of 62 and 62 of 63 bytes — so the residue is
    identified rather than inferred. The primer waited for `file_len(&path) > 0`, which the
    **first** byte satisfies, then dropped its daemon with most of a KiB still in flight; the
    adapter handed the remainder to the next reader. **Its own closing comment asserted the
    opposite** — "no primed byte can land in a capture under test" — which is the recurring
    shape of this session: a sentence describing an intention as though it were a mechanism.
    *The fix waits for quiescence, not for a byte count, and the distinction is load-bearing:*
    waiting for 1024 would hang on exactly the kernel this primer exists for, where the
    leading packet is **eaten** and the log never reaches 1024. Stability is the same question
    on a kernel that drops bytes and one that retains them. *Fail-first:* 4 of 4 red at the
    unchanged tree, 3 of 3 green at the fixed one, then 7 of 7 for the whole rig binary.
    *Not a product defect, and the record says so in the same words notes §3.70 used for its
    Linux sibling:* the daemon neither lost nor invented a byte — the payload arrived
    byte-exact, behind stale data from an earlier test. **This is §3.70's finding with the sign
    flipped**: there a re-enumerated FT232R *ate* one bulk packet, here Darwin's *retains* one.
    Same 64-byte USB quantum, opposite direction, different kernel — recorded as an
    observation, with no mechanism claimed for why the two kernels differ.

71. **A sim double stops relaying after a peer close on Darwin, and it is not an exit** —
    **open** (M). *Evidence, found by lifting `p12_sim_idle_cpu`'s gate once item 12 made the
    CPU sampler portable, and measured rather than inferred:* on the x86_64 Darwin box both
    doubles **pass** the CPU budget — §15.36's "sim doubles never busy-wait, idle-CPU
    asserted" holds there, the `nullmodem` double reading **0.01 s of CPU over a 3 s idle
    window** — and both then fail the assertion immediately after it, `still_relays`. Checked
    outside the harness to separate the two candidate causes the guard's own message names:
    the process **stays alive and paused** (it is still running at the end, having spent
    0.01 s), and a `ping` written into one link never arrives at the other within 3 s. So it
    is *stopped relaying*, not *exited*.
    **Why this blocks the ungating rather than being a side note:** the relay check is the
    control that keeps the CPU assertion from being vacuous — its own message says "a double
    that exits instead of pausing would pass the CPU budget above" — so shipping the CPU half
    alone off Linux would be a guard whose non-vacuity check is red. `p12_sim_idle_cpu` stays
    gated, with its header and both skip messages now naming **this** measured blocker instead
    of the retired `/proc` one. *No mechanism is claimed*, though the family is suggestive:
    every other Darwin difference this session met was a level-versus-edge readiness question
    (§15.39, P16, and the `POLLHUP` mask), and the sim's pause/resume path is the obvious place
    to look first. *Validation:* the fix is proven by the existing pair — the CPU budget and
    `still_relays` both green on Darwin with the gate lifted; a fix that greens only the
    relay check has not shown the CPU assertion is non-vacuous.

72. **§15.61 landed in the daemon and not in the report: the rest of P15's prose family** —
    **EXECUTED 2026-08-14** (notes §3.101), found by validating the macOS session's tree on
    Linux. Item 67 clause (c) caught **one** string of this family — the
    `does_not_license` cell — and repaired it. It did not catch the rest: all three arms of
    `p15_soft_note`, the function's own doc, and the two verdict remedies still offering
    `flow_control = "none"` **(or `xon-xoff`)** that §15.61 clause 3 explicitly retracts. So a
    report carried the corrected cell and the stale prose *in the same JSON*, and the arm that
    prints on the Linux rig — `ftdi_sio` honours the mode — read "no pre-check consults it and
    no config is refused on it". The dropped arm was worse in kind: it told an operator holding
    a structural refusal that no refusal exists.
    **Why a green suite certified it, which is the finding worth carrying.** The guard
    `p15s_software_finding_degrades_the_verdict_and_refuses_nothing` asserted the stale clauses
    *on purpose*, justified in its own message as "the bound being that the daemon consults
    none of this (item 14's decline)". Sound when written; false when the decline was paid off.
    **A guard that pins a *decision* rather than a *mechanism* must be moved by the commit that
    changes the decision, or it becomes the thing holding the defect in place** — a new register
    of AGENTS §3's tell, and one no gate could reach: `expectations/linux.jq` type-checks
    `.software_flow_control.*` cells and never reads `.consequence`, a blind spot the probe's
    own doc states. Repaired, renamed `…_refuses_the_dropping_port`, and given negative clauses
    that redden on each stale sentence; fail-first proof taken by restoring the honoured arm's
    sentence in place.
    **Two further claims that outlived their decisions, both repaired here.** (a)
    `precheck_flow_control`'s cost statement bounds the DTR/RTS drop as "only nodes that asked
    for `rts-cts`" — §15.61 puts every `xon-xoff` node on that path and says so, and the bound
    did not move; corrected, with the asymmetry recorded that the software arm writes `c_cflag`
    as well as `c_iflag` where the hardware arm writes `c_cflag` alone. (b) Notes §3.94's "Both
    new guards run and pass on Linux" — one is `#[cfg(not(target_os = "linux"))]`, so item 66's
    step 4 had no Linux exercise at all, not merely no reddening.
    **The vacuity in (b) is recorded rather than papered over, and the reason it is acceptable
    is now measured and guarded.** Step 4 has no Linux trigger *by construction*: while any fd
    on a pts is open, Linux will not hand that index to a new pair (six sequential open/close
    cycles all returned `/dev/pts/9`; the same pair with the slave held forced `/dev/pts/10`),
    so a witness cannot observe its path return pointing at a different pair and the path check
    always answers first. `a_held_pts_index_is_never_reallocated_to_a_new_pair` guards that
    kernel property in `sys` — beside `peer_hungup`, because `itest` deliberately carries no
    `nix` — with a control arm proving this kernel *does* reuse a freed index, so it
    discriminates held from free rather than passing on a kernel that reuses nothing.
    *Method, recorded because the refutations are load-bearing (§9):* six readers over disjoint
    areas of the `b346188..4548881` diff, one adversarial verifier per candidate defaulting to
    refuted; four candidates were refuted on the tree and are listed in notes §3.101.

73. **The software flow-control reading has no shipped-predicate cross-check, and since §15.61 it
    is consequential** — **open** (S/M). *Evidence, and why it changed status rather than being
    new:* P15 computes `shipped_predicate_agrees` for the **hardware** mode only —
    `p15_readback` calls `honours_flow_control(path, FlowMode::RtsCts)` and requires it to match
    the read-back the probe took by hand, with its own `degraded` arm ranked above the finding
    itself. `SoftFlowReadback` has no such field: the software cell is produced entirely by
    `p15_soft_readback`, a second hand-rolled implementation, while the daemon refuses on
    `honours_flow_control(path, FlowMode::XonXoff)`. Two implementations, no comparison.
    **This was a narrowness and is now the exact shape §7.1 clause 2 forbids.** While item 14's
    decline stood, nothing consulted the software cell, so a drift between probe and daemon moved
    no verdict and refused no config — reporting without judging is what the decline *meant*.
    §15.61 made the reading consequential: an accept-then-drop answer now refuses an operator's
    `load`. So the same "a report that calls a port fine while `load` refuses it — or the reverse
    — is worse than either verdict alone" that justified the hardware cross-check now applies to
    the software one, unchanged in force and widened in subject.
    *Not hypothetical, and the tree records the precedent:* item 56 is this defect for the
    hardware mode, where the shipped predicate and P15 answered differently about one port and
    `shipped_predicate_agrees` **found agreement** because it compared the halves that agreed.
    The two implementations here can already differ in a way nothing would catch: the probe
    reports `iflag_matches_request` (whole-word, `serial2`'s own comparison) *and* `ixon`/`ixoff`
    separately, while `honours_flow_control`'s `XonXoff` arm answers on
    `contains(IXON | IXOFF)` — a two-flag subset test. On a driver that honours both flags but
    perturbs some third `c_iflag` bit, the predicate says `Honoured` and
    `serial2_readback_would_fault` says the node's open would fail. No box measured here does
    that, which is why this is a filing and not a defect report.
    *Cost, stated because it is the reason this is filed rather than folded into item 72:* it adds
    a leaf key to the `software_flow_control` object, so `field_set` moves. **No era closes** —
    §13's era law clause 4, and the record already carries two `field_set` moves that closed
    nothing (notes §3.89/§3.90) — but `expectations/linux.jq` and `expectations/macos.jq` both
    type-check that object and must gain the key in the same commit, and the committed captures
    on both kernels predate it.
    *Validation:* fail-first by planting a divergence between the two implementations (the subset
    test versus the whole-word one is the ready-made plant) and showing the new arm reddens the
    verdict, exactly as item 56's did; then a capture on each kernel with the ports named, since a
    cross-check that has never run against real hardware is the vacuity this ledger keeps finding.

### Items 74–77 — filed by the 5-wire re-cable session (2026-08-14)

Four defects found while validating the re-cabled bench (notes §3.102), all in the guards that
carry the flow-control claim rather than in the product. **None of them weakens that session's
result**, and each entry says why — a finding that does not move the conclusion still gets a
number, because the next review cannot check that an unnumbered defect was fixed.

74. **`handshake_measured` measures one direction; the test it gates asserts two** —
    **EXECUTED 2026-08-15** (notes §3.103). **The decision the filing left open is made: a
    half-crossed bench SKIPS**, printing its reading, and hard-fails under
    `SNX_RIG_FLOW=required`. The rule behind it is the one this defect broke — *the precondition
    must measure what the promise asserts* — so `handshake_measured` now drives both directions at
    both polarities and returns `carries` only if all four readings are right. §15.52's "not wired
    is a valid answer" is unchanged for 3-wire; a half-crossed bench is a **miswiring**, and
    `SNX_RIG_FLOW=required` is the operator asserting 5-wire, so failing there names the fault
    instead of letting it surface as a mid-test assertion.
    **The filing was wrong about the blast radius, and the correction is a correctness blocker
    rather than a tidy-up.** The item said to "check whether the call sites need any change"; one
    did. The stall test called the precondition *after* loading its arm-1 graph, where port0 runs
    `flow_control = "rts-cts"` and **the kernel's line discipline owns port0's RTS** — the very
    thing `crossover_rig_rts_crosses_to_the_far_ports_cts` keeps both ports at `none` to avoid. On
    an FT232R the chip drives RTS itself under hardware flow control, so the added direction would
    have read "does not carry" on a **fully-crossed** bench, skipping the test and hard-failing the
    documented rig lane. The measurement now runs on a `none`/`none` probe graph booted and dropped
    ahead of arm 1.
    *One consequence recorded rather than discovered later:* the stall test's precondition is now
    **stricter than its own promise** — it asserts only `port1 RTS -> port0 CTS`, so a bench
    half-crossed the other way could still prove that promise and will now skip. Accepted, because
    the gate is defined by the 5-wire declaration `SNX_RIG_FLOW` makes, not per-test.
    *Fail-first, three arms on the real bench (notes §3.103):* with the `port0 -> port1` drive
    suppressed in software, the fixed tree **skips** both tests naming the half-crossed direction;
    with `SNX_RIG_FLOW=required` both **fail** naming it; and the **unfixed** tree passes the
    precondition and reddens at the second loop iteration, which is the defect.
    *Evidence:* `itest/tests/serial_hardware.rs:681-689` drives `port1 → port0` only, both
    polarities, and returns that as `carries`; `crossover_rig_rts_crosses_to_the_far_ports_cts`
    (`:740-754`) then asserts **both** directions, and its own comment gives the reason — "a
    half-crossed handshake is a real wiring state that a single direction cannot see". §15.52 and
    P5's `HALF-CROSSED handshake: RTS/CTS carries one way only` both name that state. So on a bench
    wired `port1 → port0` only, the precondition passes and the promise then reddens, where every
    other `required`-gated capability in this tree skips with its reading printed.
    *The open question this filing does not settle:* whether a half-crossed bench **should** skip
    (it is a rig that cannot answer, like a 3-wire one) or **should** redden (it is miswired, not
    a legitimate cabling). §15.52's "not wired is a valid answer" is stated of 3-wire, and P5
    deliberately names half-crossed without judging it. That decision is owed before code.
    *Did not affect notes §3.102:* the bench there is crossed both ways, P5 measuring all eight
    cells, so the gate and the assertion agree on it.
    *Validation:* suppress the `port0 → port1` direction only; the fixed gate must skip printing
    the half-crossed reading, and the unfixed one must redden at the second loop iteration.

75. **The stall test's 25 ms control is asserted at 5 s, and the comment says otherwise** —
    **EXECUTED 2026-08-15** (notes §3.103). The control arm now times delivery from an `Instant`
    taken before `send` and captured at the first satisfying poll, prints the figure on every run,
    and asserts a **structural margin** instead of a remembered number: `STALL_WINDOW` (1.5 s) is a
    named const both arms read, and the control asserts `latency * MIN_MARGIN <= STALL_WINDOW` with
    `MIN_MARGIN = 4`. The "60x" sentence is replaced by one stating what the code checks.
    **Measured on this bench: 20.35 ms**, against the 25 ms the old comment cited from a different
    adapter pair — so the figure is now re-derived per run, which is what the comment always
    claimed. *Fail-first, both plants on the real bench:* `MIN_MARGIN = 400` reddens the control
    naming the measured latency (proving the assertion reads the measurement, not a constant), and
    `STALL_WINDOW = 40 ms` leaves arm 1 green while printing `held 40 B for 40ms` — proving both
    arms read the one const — and reddens the control. The 5 s `wait_until` stays, being delivery
    *detection*; the margin is the separate assertion, so a 4 s delivery now passes `crossed` and
    then reddens on the margin, which is the inversion the item described. *Evidence:* `itest/tests/serial_hardware.rs:1098-1108` argues the 1.5 s stall
    window is "a 60x margin" over a measured 25 ms and states that "the control arm below
    re-establishes the 25 ms figure on every run rather than trusting this comment". The control
    arm (`:1183-1196`) calls `wait_until(Duration::from_secs(5), …)` and asserts only that the
    bytes crossed; it records no elapsed time and prints none. The assertion is therefore **200×
    looser** than the sentence above it claims, and a bench whose no-flow-control path took 4 s
    would pass the control while making arm 1's 1.5 s window *shorter* than the uncontrolled
    latency — inverting the margin argument with nothing to notice. **A third instance of AGENTS
    §3's "assertion strictly weaker than the comment above it claims"**, and the second found in
    this file. *Did not affect notes §3.102:* arm 1 read zero bytes across 1.5 s and the control
    arm did deliver, so the wire was live and the stall real; what is unproven is the *margin*,
    not the result. *Validation:* time the control arm and either assert a bound or print the
    figure; deleting the 25 ms claim is an acceptable disposition, leaving it unmeasured is not.

76. **The stall test never primes the wire** — **EXECUTED 2026-08-15 with its central prediction
    NOT REPRODUCED** (notes §3.103), which is the more useful half. `prime_the_wire_once` is now
    called after the rig guard, and the post-release assertion's message — which blamed the peer's
    RTS for what a swallowed leading packet looks like — now names the primer as the first thing to
    check. **But the item predicted that an unprimed run fails after a re-enumeration, and it does
    not: 5 of 5 unprimed trials passed**, each after a real `devprep cycle` of both ports.
    **The hazard is nonetheless real on this pair, measured the same session:** with priming
    disabled globally, `crossover_rig_custom_baud_byte_exact`'s 32768-byte transfer failed its
    byte-exact assertion on **1 of 3** re-enumerations. So notes §3.70's swallow reproduces here —
    intermittently, not the 3-of-3 it read on the original adapter pair — while this particular
    test does not expose it in 5 attempts. Two candidate reasons are named and neither is measured:
    the stall test boots three daemons before its 40 bytes cross, and the failing observation was at
    a **custom** baud, so the swallow may track the rate change rather than the enumeration.
    *Disposition:* the primer stays — it is free, it matches every sibling test, and the underlying
    hazard is measured on this bench — but **the item's stated failure mode is unproven here** and
    must not be cited as though it were. A confound is recorded with it: the plant also had to
    convert the precondition probe from `boot_rig` to `boot_rig_raw` to remove the second priming
    path, which changes the boot sequence the trial measures. *Evidence:* the test builds its
    daemons with `Daemon::start()` at four sites (`:953`, `:995`, `:1040`, `:1149`) and never calls
    `prime_the_wire_once`, which is reached only through `boot_rig` (`:282`) and the map test's
    explicit call (`:505`). Its payload is **40 bytes**, smaller than the **64-byte** leading packet
    a freshly re-enumerated FT232R swallows (notes §3.70, item 3), so a first-after-replug run of
    this test alone loses the whole payload and fails at the post-release byte-exact assertion —
    whose message blames the peer's RTS rather than the primer. *Why it has not fired:*
    `prime_the_wire_once` is a process-wide `OnceLock` and the sibling crossover tests in the same
    binary reach it through `boot_rig`, so in practice something else primes first; notes §3.102's
    runs were protected that way. That makes this a latent flake with a misattributing message, not
    a live failure. *Validation:* run this test first in a fresh binary after a physical replug —
    unprimed it must fail, primed it must pass 3 of 3.

77. **A doctor unit test loops over three constants and passes none of them in** —
    **EXECUTED 2026-08-15** (notes §3.103). *The plant came first and proved the vacuity:* a
    `Certificate` failure keyed on the handshake reading was planted at `p5_rig`'s call site, which
    genuinely degrades an honest 3-wire bench — exactly what §15.52 forbids — and **the loop stayed
    green**, as did all 92 doctor unit tests. The loop is deleted and the structural argument put in
    its place, naming the signatures that carry the property: `p5_handshake_line(&HandshakeCells)
    -> String` and `p5_handshake(&Path, &Path) -> String` hand back text and no `Certificate`, and
    `p5_verdict` takes no handshake input at all. The one reachable way to break it — the `p5_rig`
    call site ceasing to be a single `observe` — is named so the next reader does not re-add a
    runtime check the compiler already makes.
    **The item's own *Validation* line was unachievable and is corrected here:** it asked for the
    loop to be "green today and red after any honest rewrite of it". The first half is confirmed;
    the second cannot be, because the fold lives in `p5_rig`, which needs a bench, so **no unit test
    of that shape can ever see the plant**. The honest rewrite removes the claim rather than making
    it fail. *A source-scanning guard was evaluated and declined:* it could pin the two signatures
    and still miss the call-site fold, which is AGENTS §3's second register — an assertion weaker
    than the comment above it — and writing a fresh instance of that is worse than the defect.
    *The vacated slot got a real assertion:* `crosses()` returns four values and **`inverted`** was
    exercised by nothing. Two assertions added, each fail-first proven with its own plant — an
    `any_dtr` counting `inverted` as wiring, and a `both_rts` doing the same — with every
    pre-existing assertion, `stuck-high` included, staying green under both.
    *Evidence:* `doctor/src/probes.rs:9068-9078` iterates `for l in [&five_wire, &three_wire,
    &half]` and calls `p5_verdict(true, true, &[], &[], paired())` — five literals, identical on
    every iteration — using `l` only inside the panic message. Its comment claims the property is
    "asserted over the verdict itself rather than by inspecting the call site". It cannot be: the
    handshake reading is not an argument to `p5_verdict` at all. The loop's passing output is
    identical to its not-running output, which is AGENTS §3's original tell. **The deeper reading,
    which is the reason this is small:** the handshake structurally cannot reach the verdict, so
    what the test wants is already guaranteed by the signature — the defect is that it is written
    to look as though it varied the handshake, and a reader checking coverage would count it.
    *Validation:* plant a `cert.fail_if` on `p5_handshake`'s path; the loop must be green today and
    red after any honest rewrite of it.

### Item 78 — filed by the privilege inventory (2026-08-15)

78. **The `/dev/ttyACM*` half of the dialout claim** — **open** (S; needs a **CDC-ACM device**,
    not privilege). Split out of item 31 on 2026-08-15, because that item is routed as "needs a
    root box" and root cannot conjure a device node — leaving this clause inside it made the
    blocker unactionable and that item un-closable for a reason unrelated to its subject.
    *Evidence:* `packaging/README.md`'s `SupplementaryGroups=` row reads `measured (partially)` —
    `crw-rw---- 1 root dialout 188, 0 /dev/ttyUSB0`, one box, one distro — and states that no
    `/dev/ttyACM*` was present to check, leaving the `ttyACM` half of the sentence and the `uucp`
    remark unverified.
    **Its cheaper half is answered here, unprivileged, and the item narrows to what is left.** The
    claim has two parts: *which group* the node lands in, and *what mode*. The group half is settled
    by reading the shipped rules — `/usr/lib/udev/rules.d/50-udev-default.rules:47` is
    `KERNEL=="tty[A-Z]*[0-9]|ttymxc[0-9]*|pppox[0-9]*|ircomm[0-9]*|noz[0-9]*|rfcomm[0-9]*",
    GROUP="dialout"`, and `ttyACM0` matches `tty[A-Z]*[0-9]` exactly as `ttyUSB0` does. So on this
    distro a CDC-ACM node is `dialout` by the same rule, which is what the unit's
    `SupplementaryGroups=dialout` depends on. **What that reading cannot supply is the mode:** the
    rule sets no `MODE=`, so `0660` comes from the driver default and only a present device shows it.
    *Remainder:* (a) the mode on a real `/dev/ttyACM*`, and (b) the `uucp` remark, which is a claim
    about distros this project has never booted and may be better answered by **deleting** it than
    by acquiring one — a recorded decline is a legitimate disposition for (b).
    *Declined:* inferring the mode from `ttyUSB0`. Different driver, different default, and the whole
    value of the row is that it was measured rather than assumed.
    *Validation:* attach a CDC-ACM device (an Arduino-class board is the cheapest instance), record
    `ls -l` and the resolving udev rule, and move the README row from `measured (partially)` to
    `measured` with its scope named. Reported-never-judged where no such device is attached — this
    must **not** become a fifth `required` spelling, since a box without the device is not a fault.

### Evaluated and deliberately not scheduled — the closing register

Carried so the choices cannot be mistaken for oversights. Overturning any entry here is a
recorded decision naming new evidence (AGENTS §5).

- **Web-console control-lane freshness** (a separate control lane, or a bounded/coalesced data
  lane, for the drop counters a firehose lags): a recorded future nicety with two measurements
  behind it — the drop-counter lag under a firehose is honest behavior for a control-and-
  observation tool at serial rates (§15.38) — re-opened only by an operator need, never by
  tidiness.
- **The per-open PTY generation epoch** stays in §14 on its standing rationale; its §6 blind
  spot stays documented beside the lock lifecycle, not here.
- **§15.44's residual** — neither digest can see a probe body that moves a number without
  moving a key — is a stated limitation carried beside the standing hand-announcement rule, not
  a schedulable fix; the decline not to fold keys into `probe_set` stands beside it, so any
  future per-probe body fingerprint is a recorded decision, not a silent re-fix.
- **Declines that adjoin open items, status marked:** readiness unification (§16.9 — STANDS);
  the no-macOS-port of `p12_pty_setup.rs`'s guard, `discarded_at_last_close` being structurally
  0 there "by kernel, not by defect" (item 6 — STANDS); §15.53's two refusal repairs, the
  post-teardown recheck and inference-from-open (notes §3.68 — STAND); the RES-2 P4-no-udev
  decline (STANDS, narrowed by notes §3.48); the notes §3.37 `serial_pair` decline (OVERTURNED
  by notes §3.43 — quote the overturn, never the decline); the `GraphState` re-key (declined
  twice, notes §3.14 and §3.20 — not re-filed a third time); the replug-lane udev rule
  (DECLINED, §15.45 — distinct from packaging's live optional rules); P10's status, deliberately
  unchanged (§15.49); presence-epoch gating of the pty hostward bridge (declined; re-costed only
  if the bridge changes).
- **Recorded, not re-filed:** the pty's held `pending` payload (not fixable by the inbox shape —
  §15.50); the Darwin FIONREAD input-queue mechanism (confirmed 9 of 9, deliberately
  unestablished — an open observation, not open work); the `p8_web_ui` in-suite contention datum
  (notes §3.58); `crossover_rig_map_node_both_directions` failing under parallelism only
  (notes §3.65 F — the §3.67 citation that stood here until v17 was wrong-but-real, the class
  the citation gate cannot catch); `p3_firehose`'s one-of-three 60 s deadline miss at 99.5% delivered (notes
  §3.69); the flake session's one unnamed load-sensitive failure (§15.36's session; treat any
  lone unnamed load-sensitive failure as one of these two recorded classes resurfacing, never
  as a fresh finding); the manual-checklist residue (rendering fidelity, real-rig
  interaction — plan §5); and the macOS `cu.usbserial` doctrine question, narrowed to opt-in
  by item 5, with notes §3.35's filed
  question still open as a question.
- **Declined, newly recorded here (previously only in the notes):** no `rust-toolchain.toml`
  is committed — pinning contributors' local toolchains is a repo-owner decision, not a CI fix;
  MSRV is pinned in CI (plan §2). Recorded so adding the file as "hardening" is recognized as
  overturning a decision rather than drifting past one (AGENTS §5).
- **Recorded, not re-filed (v17 additions):** the one-time
  `adding a console through the editor makes bytes flow end to end` failure — 20.4 s on an idle
  box, its evidence destroyed by a throwing `finally` since fixed; not diagnosed, not called
  fixed, the next occurrence names its own cause (the record is the rename track's "One
  residual lead" paragraph in the notes; distinct from the §3.58 contention datum) —
  and the open question whether `--show-output` surfaces a passing test's stderr, which decides
  whether notes §3.53's zero-SKIP-lines instance stands or joins §3.78's withdrawn set:
  deliberately not assumed either way.
- **A figure-to-artifact bijection gate over prose** — evaluated at v17 and deliberately not
  scheduled, despite three measured recurrences of artifact-attributed numbers drifting (a jq
  gate's stale constants and an index label no artifact carries, both notes §3.36; an
  un-artifacted range in the design, notes §3.75): nothing reads prose (§13's gate-blind-spots rule is the honest statement), and the
  practical guard is §16.13's discipline plus each generation's alignment pass. Re-opened on
  the next recurrence, not before.
- **The §14 deferral register** enumerates every named product-surface deferral once, under the
  deferral-state vocabulary, each with its declining entry cited — this ledger cross-references
  it and does not duplicate it.
