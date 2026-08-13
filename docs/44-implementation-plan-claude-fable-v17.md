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
| **931 passing · 1 failed · 6 ignored**, 122 test-result lines | Linux, default CI scope, `--no-fail-fast`, `--nocapture` | 2026-08-12 | the item-47 landing (notes §3.83) | **the Linux authority row**, superseding the 894 below. 122 result lines over **118 cargo targets** (110 `Running` + 8 doc-test) and **12 self-skips**, measured with `--nocapture` (notes §3.78). The +37 over the 894 row is exactly this session's new tests, counted rather than estimated: **22** matcher units (`daemon/src/pattern.rs`, a new file), **12** pattern-wait acceptance guards (`itest/tests/p12_pattern_wait.rs`, a new file), **2** hub guards added to `daemon/src/tap.rs` (11 → 13) and **1** devprep capability-fold guard (1 → 2). Both ends were measured in this session, so the delta is quotable. **An earlier row here read 925 and decomposed it as 19+11+1**; that figure was taken before the adversarial review's fixes added six guards, and its decomposition was wrong by one even for its own tree — recorded because a Status row whose arithmetic does not close is exactly what this table's scope discipline exists to prevent. The one failure is `p3_idle_cost::thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget` at **4.10 %** against its 3.50 % tripwire: item 46, and **measured not to be this change's** — runs on this tree read 3.70/3.70/3.90/4.10 %, and three runs on the unchanged tree (a `git worktree` at `849fc8e` with its own target dir, same box, same session) read 3.80/4.30/3.90 %, so the two trees sit in one band and the pattern wait adds no idle-path work (it runs inside `TapHub::ingest`, which an idle endpoint never reaches). |
| **894 passing · 1 failed · 6 ignored**, 121 test-result lines | Linux, default CI scope, `--no-fail-fast` | 2026-08-12 | the v17 landing's gate run (notes, the v17 generation entry) | the one failure is `p3_idle_cost::thirty_two_idle_tty_fds_stay_under_the_recorded_cpu_budget` at **3.80 %** against its 3.50 % tripwire — item 46's recorded signature verbatim (its message prints "38 ticks over 10s"), reproduced in two of this landing's three suite runs (once under deliberate parallel load, once on a quiet box at load 0.33) and absent in the first; the product tree is unchanged by v17 (documents, two meta-gate consts, one harness message string), so this is item 46 resurfacing, never a fresh finding. Supersedes the 890 row as the default-scope authority; the total moved 896 → 901 because the 2026-08-12 rig session's later commits added tests after that row was taken. **Superseded by the 925 row above**, taken later the same day at the same scope, which is the current Linux authority. |
| **890 passing · 0 failed · 6 ignored**, of which **886 · 0 · 6** are workspace-own | Linux, default CI scope | 2026-08-12 | this session (notes §3.75) | 121 test-result lines over **117 cargo targets** (109 `Running` + 8 doc-test), and **12 self-skips**, measured with `--nocapture` (notes §3.78 — at default capture the count reads 0 because `cargo test` captures a *passing* test's stderr, so "zero SKIP lines" beside a figure taken without it asserts nothing); four passes and two lines are the nested `acme-codec` **and `tinymux-codec`** subprocesses (`p8_external_codec.rs`, which now builds and tests both template crates); rig attached but `SNX_CROSSOVER_A`/`_B` unexported, so `serial_hardware` self-skipped. The `ignored` moved 4 → 6 for a documentation reason, not a coverage one: the two new kit suites carry ```ignore` doc examples like their four siblings. Superseded by the v17 landing row above. |
| **852 passing · 0 failed · 4 ignored**, of which **850 · 0 · 4** are workspace-own | Linux, default CI scope | 2026-08-07 | v15 Status re-measure; no sha recorded | 116 test-result lines over 114 cargo targets; its "zero SKIP lines" is **withdrawn** — it was taken at default capture, where the count is 0 whatever skipped (notes §3.78). Two passes and two lines are the nested `acme-codec` subprocess (`p8_external_codec.rs`). Superseded by the row above; kept because the delta between them was measured in one session at one scope. |
| **931 passing · 1 failed · 6 ignored**, four self-skips | Linux, **rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** | 2026-08-12 | the item-47 landing (notes §3.83) | **the rig-lane authority row**, superseding the 894 below and equal to this session's default-scope figure. Run against a freshly `scripts/bless`ed helper (the operator ran it mid-session; an earlier lane was discarded rather than recorded, because the helper binary was replaced underneath it). Every hardware test passed — both replug tests including `identity_survives_a_replug_that_renumbers_the_tty`, all five crossover tests, `web_tls_round_trip` under `SNX_TLS=required`. The two named drops are measurements taken *in this run*, not conveniences: the four self-skips are the two `rts-cts` tests, which print the reading that justifies them (`port1 RTS high -> port0 cts:false`, low -> `cts:false` — a **3-wire** bench, §15.52's legitimate answer and the third independent confirmation of it), and the two browser tests, on a box with no `node`. The one failure is `p3_idle_cost` (item 46), unrelated to the rig. **One lane before this one hung** and is recorded rather than quietly re-run — see the note under this table. |
| **894 passing · 1 failed · 6 ignored**, four self-skips | Linux, **rig lane minus `SNX_RIG_FLOW` and `SNX_WEB_UI`** | 2026-08-12 | this session (notes §3.80) | the rig-lane authority row, and the first green one in this record. Both exclusions are measurements, not conveniences: this box has no `node`, and the bench **measures 3-wire**, which §15.52 makes a legitimate answer — so the two `rts-cts` end-to-end tests skip with their reading printed. The one failure is `p3_idle_cost` (item 46), unrelated to the rig. Every hardware test passed, `identity_survives_a_replug_that_renumbers_the_tty` for the first time ever. **Superseded by the 925 rig row above**, taken later the same day at the same scope. |
| **835 passing · 0 failed · 4 ignored** | Linux, rig lane — and again at default CI scope, same session | 2026-08-05 | `17c6e87` (notes §3.68) | twice on the full rig lane, once at default CI scope, 835/0/4 each time — the last dual-scope measurement; superseded by the 852 re-measure of 2026-08-07; not the current-tree figure. **Attribution unreconciled (v17):** notes §3.68's verbatim session record reads 830 (gates scope) and 834/0 · 833/1 (rig lane), and no 835 appears in it — the figure survives only as a v15 Status-table quotation (the re-cited-not-re-derived class), so neither the number nor the dual-scope equivalence is attributable to §3.68. Superseded either way. |
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
| `sys/` | `serial-nexus-sys` | lib | **The one crate carrying `unsafe`** (§16.3): raw ioctls, `ptsname`, `poll(2)`, `usb_macos` (notes §3.66), `honours_rtscts` (§15.53). |
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
— its four fixtures (`passthrough.py`, `passthrough-codec.py`, `lag.py`, `half-duplex.py`) and
the kit's `Hoarder` positive/negative pair are named must-preserve by §8 (a tidier session must
not simplify them away).
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
lanes: `check` (fmt; workspace and minimal-daemon clippy; the Apple compile cross-check on
**both** triples, `--all-targets` — neither triple is a superset of the other, and `cargo build`
compiles no test target, notes §3.71; then `cargo build --workspace --locked` and the full
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
digest surfaced.

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
    skips.**
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
13. **Idle CPU on any Mac is unmeasured, and the projection is arithmetic.**
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
14. **`flow = "xon-xoff"` has no pre-check and no probe.**
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
    feasibility claim conditional on what P5 measures that day. *Validation:* rig-gated with a
    `required` spelling (plan §3 rule 11); reported-never-judged where wiring may legitimately
    be absent (§15.52's pattern).
18. **The macOS capture and suite run owed at the current tree** — **open in its capture half
    only** (S, Mac rig session). *Suite half executed 2026-08-12* (notes §3.76): CI's `macos` job
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
19. **P10's drain ladder: both kernels' bounds** — **open** (S, one rig visit per kernel).
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
20. **`p4_free_for_all` on Darwin: stall against loss, separated** — **open** (M; shares
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
21. **`exec`'s teardown figure: floor to total** — **open** (S). Item 2's recorded residual,
    now scheduled. *Evidence:* `exec`'s `discarded_at_teardown` is a floor because its internal
    merge stage is not reached (notes §3.55; named where the counter is documented, §15.50).
    *Validation:* a guard whose child has stopped reading stdin, so the merge stage holds bytes
    at teardown; fail-first per notes §3.55's disjoint-reddening method.
22. **P13's missing shape: a reader arriving during the close-wait** — **open** (S).
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
24. **Leash coverage for `Sim` and raw daemon spawn sites** — **open** (S). *Evidence:* the
    stdin-EOF leash (§15.43) exists only via `Daemon::start`; `Sim` and the raw spawn sites are
    uncovered, and the notes §3.39 orphan's trigger is still unestablished. *Validation:*
    coverage keeps §15.43's opt-in semantics; the trigger question stays a question unless a
    reproduction answers it.
25. **The sim's `--hold-ms` timer retired for a caller-owned hold** — **open** (S). *Evidence:*
    `p4_exclusivity` still holds by timer; the replug helper already proved the caller-owned
    stdin-EOF hold shape — the caller owns the hold length and samples unprivileged (§15.45;
    notes §3.56). *Validation:* existing guards unchanged; the timer gone, not defaulted.
26. **A slave-witness liveness probe for the doctor** — **open** (S). *Evidence:* the harness's
    Darwin residual — the witness-fd behavior notes §3.56 leans on — is expected, not measured,
    as a standalone observation; notes §3.60 names the doctor as its home. A `poll(POLLHUP)`
    probe makes it measured. *Validation:* a new probe id is a new instrument and moves
    `probe_set` — the era move deliberate and recorded in `docs/doctor/README.md` (notes
    §3.57's rule); presence-never-answer.
27. **The Markdown value grammar wants a design note before any fix** — **open** (S; a
    decision, not an edit). *Evidence:* the doctor's Markdown value grammar is non-injective
    (`", "`, `=`, `[`, `]` unescaped inside values), and fixing it would rewrite every frozen
    `.md` artifact (notes §3.74). *Validation:* the decision recorded as a design entry before
    any renderer change; frozen artifacts never edited either way (§16.13).
28. **The DTR measurements** — **open** (S; needs a rig with DTR wired — the current rig leaves
    it unwired). *Remainder:* (a) the DTR-pulse cost question — whether the pre-check can ask
    from inside the node's own open rather than as a separate toggle (notes §3.68 measured the
    extra toggle; the falsified "not an *extra* toggle" claim is recorded at §15.53's entry).
    (b) The B→A DTR cells' `true`/`stuck-high`/`inverted` arms, and the transposed-read
    blindness notes §3.73 bounds. *Validation:* pre-registered readings before the wire is
    touched; committed captures.
29. **The tool-wrapper scripts' fate, recorded** — **open** (S; a record to complete, not code
    to write). *Evidence:* the v15 plan asserted three shell scripts survive as external-tool
    wrappers pending §16.11; the tree's `scripts/` directory now holds only `bless`, and this
    pair treats §16.11 as executed (plan §5: the harness is canonical, the bash suite retired).
    No notes entry records where the three wrappers went. *Validation:* verify their fate in
    the git history, then record the retirement (or the new home) in the notes, so the v15
    claim is discharged rather than dropped.
30. **Dual-scope figure equivalence** — **open** (S). *Evidence:* the v15 Status line
    attributed a dual-scope 835 to notes §3.68, and v17 finds the attribution unreconciled —
    §3.68's verbatim record reads 830 at gates scope and 834/0 · 833/1 on the rig lane, with no
    835 anywhere in the session record (the Status row carries the annotation) — so the
    equivalence has no reconcilable measurement at *any* era. *Validation:* one dual-scope run
    at the current era; both figures land in the plan Status table with their scopes named, and
    no delta is derived across them (plan §3's figure-scope rule).
31. **The packaging evidence pass** — **open** (S; needs a root box). *Evidence:* PKG-2's
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
36. **Golden transcripts of the daemon boundary** (L). Replayable byte transcripts of the
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
38. **Codec-node teardown-conservation suite** (M). A conformance test shape asserting
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
42. **`boundary::BlockingReader` unification** (M). Rename, optional loss counter, rebase
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

45. **`existing-terminal`'s refusal, made structural** (S; a decision first, then a small patch).
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

46. **`p3_idle_cost` sits too close to its own ceiling under suite parallelism** — **open**
    (S/M; a measurement and then a decision, not an edit). Filed 2026-08-12 (notes §3.75 §I).
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
48. **The macOS doctor gate has never executed green** — **HALF DISCHARGED 2026-08-13**; the
    remainder is placement (S). The measurement now exists: CI run 31657666919, job
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
50. **Harness scaffolding consolidation** — **open** (M; schedule beside item 24).
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
51. **Client socket-fallback policy hoisted into `serial_nexus_rpc`** — **open** (S).
    *Evidence:* `ctl` and `web` carry byte-duplicate implementations of the rename-window
    fallback (current socket name wins; `LEGACY_DAEMON_NAME` only when the current one is
    absent) — the two-copies-that-must-agree shape `rpc::socket`'s own module doc records as
    the reason that module exists (notes §3.72). *Remainder:* one rpc-crate implementation
    beside `default_socket_path`, both clients re-pointed; plan §17.3's window retirement then
    edits one crate. *Validation:* behavior identical (the single retired-spelling constant
    stays in rpc; the retired-names meta-gate allowance is untouched).
52. **devprep follow-ups: parity, reporting, envelope, orphan hygiene** — **open** (S/M).
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
53. **Conformance kit: resync-accounting suite** — **open** (S/M). *Evidence:*
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
54. **The macOS lint gap** — **open** (S). *Evidence:* clippy (workspace and minimal-daemon)
    runs only in the ubuntu `check` job and the `macos` job runs no lint, so the
    cfg-gated-only-caller dead-code class is caught solely by hand runs on a Mac — measured
    when the macOS validation session found the gate had never once executed on the platform
    that fails it; notes §3.71's both-triples argument applies to lints exactly as to compile
    checks. *Remainder:* a Darwin-triple clippy cross-check beside the existing compile
    cross-check on the Linux lane, or clippy in the `macos` job — or a recorded decline.
    *Validation:* a planted macOS-only lint violation reddens the chosen lane (read at its
    error text — rule 17 f).
55. **In-tree comment and dead-code hygiene: the drifted-copy class** — **open** (S/M).
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
