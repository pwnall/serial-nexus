# AGENTS.md — operating manual for serial_nexus implementation sessions

## §1 What this repository is
A Rust workspace implementing the serial_nexus design: `serial-nexus-daemon` (lib `serial_nexus_daemon`), `serial-nexus-ctl`, `serial-nexus-web`, `serial-nexus-doctor`, `serial-nexus-sim`, the `serial-nexus-itest` harness, and the libraries `serial_nexus_core`, `serial_nexus_rpc`, `serial_nexus_sys`, `serial_nexus_codec_api`. Directory names stay short; Cargo `name` fields carry the family prefix (design §15.40). The normative documents are the current design/plan pair named in §2; §ref numbers always point at the current design. Older doc versions, frozen reviews, and remediation ledgers live in `docs/historical/` and are records, not law.

## §2 Current state, and the claims discipline
The normative pair is `35-design-claude-fable-v14.md` + `36-implementation-plan-claude-fable-v14.md`, both directly under `docs/`; when a generation lands, the superseded pair moves to `docs/historical/` and this line and README's documentation index — the only two places that name the pair by filename — are bumped in the same commit. A meta-gate fails if either goes stale. All tracks through the rename track (plan §17) are executed, and review 37's 82 findings are dispositioned (ledger: `docs/38-review-37-remediation-ledger.md`); the plan's Status line is the authority on what is open, and it currently names none. The suite is **723 passing, 0 failed, 4 ignored** on Linux (102 → 111 test targets: the review-37 track added `p3_idle_cost`, `p7_clean_exit`, `p7_snapshot_lifecycle`, `p12_ctl_subscribe`, `p12_web_host_names`, `p12_web_ws_bounds`, `p12_web_wsclient_token`, `p13_socket_claim` and `harness_contract`). Every headline claim here (suite count, gate set, verified platforms) must match `docs/implementation-notes.md` — reviews check this file against reality, and a stale claim is a defect. Update this section in the same commit as the change it describes.

## §3 Gates (all must be green before "done")
`cargo build --workspace --locked` · `cargo test --workspace --locked` · `cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` **and** the minimal-daemon clippy · `cargo deny check licenses bans sources` · macOS cross-check (`--exclude serial-nexus-web`) · `serial-nexus-doctor --json | jq -f expectations/linux.jq` · the Playwright gate (self-skips without node) · the meta-gates, including the retired-name grep (§15.40) and the consumer-context grep (§15.41), both in `itest/tests/meta_names.rs`. CI enumerates loops from tools (`cargo fuzz list`), never hand-kept lists; meta-gates assert *execution*, not file existence, and a scanning gate proves its **matcher** as well as its walker — plant the violation in every spelling it claims to cover.

## §4 Invariants with tripwire status (violations die in review)
- A `RefCell` borrow never crosses an `.await`; `std::cell::RefCell` is clippy-banned in the daemon — use the critical-section cell (§16.2).
- `AsyncFd` is banned workspace-wide for pty masters (§15.18; read the P8 annotation before re-litigating).
- `unsafe` lives only in `serial_nexus_sys` (§16.3); everything else is `#![forbid(unsafe_code)]`.
- Every targetward framer fragments oversize chunks via the one shared helper; never skip-on-encode-error (§5, §15.27).
- Purge is one invariant, three instances (§6); the tap/ring mirror never affects `discarded_unattached` (§15.32).
- Every numeric attribute and every wire-riding identifier carries a stated, structurally checked maximum (§15.34, §16.12).
- The web bridge parses exactly one request per frame and forwards only the allowlist; lifecycle verbs stay off it (§15.34, §15.35).
- Sim doubles are subprocesses, HUP-tolerant, never busy-waiting, idle-CPU asserted (§15.31, §15.36).

## §5 Documents: amend-first, and the ledgers
When implementation and design disagree, amend the design first (a new §15 entry; annotate superseded entries, never rewrite them), then implement. Deviations land in `docs/implementation-notes.md` §3 as numbered entries. Review findings get a remediation ledger mapping every id to a disposition — including deliberate declines; silently re-fixing a declined item is a defect. Consult the prior reviews' cleared-candidate tables before filing anything.

## §6 Harness doctrine (plan §3)
Fill-then-commit protocol clients (deadline expiry never consumes); completeness asserted only where the design promises it, with `hostward_buffer` provisioned and cited where a test needs it; the loss fingerprint is `received + dropped_slow_consumer == sent`; the sim marks `timed_out` so a deadline is never read as a drop; run quiet and under parallelism; never filter suite output before the failing test's name is captured.

## §7 Kernel evidence
No one-way decision on single-kernel evidence. When two credible sources disagree about kernel behavior, prefer the kernel-independent design and add a doctor probe (P6–P11 pattern) that measures instead of assumes. A kernel that differs is `degraded` with the observation named, never `unsupported`. Kernel claims in prose cite a committed `docs/doctor/` report (its commit, fingerprint, and date — §16.13), never a terminal scrollback.

## §8 The box, and the tree
Measure the machine before attributing anything to code: load average, competing builds, core count. Flake reproduction rates are meaningless under uncontrolled load. Do not trust `git stash` to bisect a large uncommitted change set — reproduce defects by *reverting the specific fix* in place. Capture failing-test names verbatim before any rerun.

## §9 Verification protocol
A regression guard is valid only with fail-first proof against the unfixed tree. Root-cause claims for flaky or security-relevant findings get an independent adversarial verifier that has **not** read the diagnosis — only the finding and the tree — and the tree must not move while verification runs (the v12 audit returned 35/43 wrong verdicts because it moved). Record refuted diagnoses; they are as load-bearing as confirmed ones.

## §10 Context hygiene (design §15.41)
Documents, notes, ledgers, reviews, and comments describe *capabilities*, never consumers: no assertions about the existence, count, or nature of external users or any business context. Say "out-of-tree", never `closed-source`, `closed repo`, or `known repository` (gate-banned tree-wide, including `docs/historical/` — the privacy rule outranks the frozen-history rule). `downstream` is allowed only in its data-flow sense; `proprietary` only as a general capability. When quoting older material that violates this, paraphrase.

## §11 Naming
Binaries `serial-nexus-*`; importable crates `serial_nexus_*`; directories short and carrying no family prefix at all — `core/`, `rpc/`, `sys/`, `daemon/` (the library), `daemon-bin/` (the thin binary), `ctl/`, `web/`, `sim/`, `doctor/`, `itest/`, `codec-api/`, `codecs/reference/`. A gate that scans the filesystem therefore spells the *directory*, never the crate. Old names only in `docs/historical/`, the frozen reviews, and the captured `docs/doctor/` artifacts, enforced by the meta-gate. The extension surface is exactly `serial_nexus_codec_api` plus the `serial_nexus_daemon` entry API, semver'd, proven by the external-consumer template built from the consumer's position on every push.
