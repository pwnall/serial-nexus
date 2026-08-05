# Captured `serial-nexus-doctor` reports

The artifacts behind every cross-kernel claim in `docs/serial-nexus-doctor.md`, `AGENTS.md`
§7 and `docs/implementation-notes.md`. Until 2026-07-29 there were none: both the
7.0 baseline and the 6.18 run lived in session scratchpads, which made "P6 and P7
read field-for-field identical" a statement asserted in three documents with
nothing in the repository to check it against. These files are that check.

**Read the `Build` block first, and read *both* digests.** `probe set` covers the
deduplicated, sorted set of each probe's `(id, question)` **strings** — not the code
that asks them, and not the fields they emit. **Only its unequal direction is a
verdict:** unequal means the instrument moved and a field-by-field comparison is
reading two different instruments, whatever the numbers look like. **Equal does not
mean comparable**, and this directory holds the counterexample: `fa4b12d`, `71fc5a8`,
`7ead470` and `4b78fff` all print `a131e1f4b46d6c83`, and the macOS observation set
gains **65 newly-present leaf paths** between `7ead470` and `1a9a8fc` and **71**
between `fa4b12d` and `1a9a8fc` — while between `fa4b12d` and `7ead470` only six paths
appear and P10's hostward figure moves by a factor of **4104**. (`df48bfc` is a fifth
commit printing the same digest: P4 gained four cells there — `canonical`,
`topology_only`, `unidentified`, `sysfs_tty_listing` — measured on the dev box. Every
count here is a recomputation from the committed artifacts, and they are **not** the
32/35 an earlier commit message quoted, which no collapsing of the leaf paths
reproduces — notes §3.51.) What answers "same cells?" is **`field set`**: a
digest over the sorted, deduplicated set of scalar leaf paths under
`.probes[].observations`, values excluded. Equal `field set` means every observation
present in one report is present in the other, so the diff has no missing cells.
Unequal means diff only the intersection — and it does not say *why* they differ, since
a different kernel, a different port list and an unobserved histogram key all move it.
**Neither digest can see a probe body that changed a number without changing a key.**
Two omissions from `probe set` are
deliberate and worth knowing here: the probe **title** (P3's carries the device path
and P3 is emitted once per `--port`, so a two-port and a zero-port run of one binary
would otherwise disagree — which is exactly the pair below) and the
**measurements** (those are what a diff compares). Correcting a report's prose
therefore does not invalidate an archived comparison. `commit` says which tree built
the binary; `generated` is a UTC stamp. A report with **no** `probe set` at all —
anything built before 2026-07-28 — is not comparable with these. A report with no
**`field set`** — anything built before `b21548d`, which is every file indexed below
except the six `f8315cc` rows at the top, the first captures to carry the digest they
are indexed by —
has an *unknown* cell set, and unknown is never "equal": the column below carries the
recomputation (`serial-nexus-doctor --field-set <file>`), because the digest is a pure
function of `.probes[].observations` and so is computable for artifacts captured before
the field existed. That is what let this column be added without touching a frozen
artifact (§16.13).

## Index

| File | Kernel / box | Binary | Probe set | Field set | Rig | Verdicts |
|---|---|---|---|---|---|---|
| [`linux-7.0-2026-08-05-f8315cc-tier3.json`](linux-7.0-2026-08-05-f8315cc-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The first Linux capture of the P9/P10/P12/P4/P5 repairs**, all five of which were developed on the Mac: `df48bfc`, `50af61e`, `5c3e697`, `448f562`, `b21548d`, `f8315cc` landed after the `-05b` triple below and no Linux run existed for any of them. Also the first report to carry `build.field_set` in the file rather than only in this column. Taken on a settled box (load 0.20–0.31); eight sequential runs were taken and the first three are committed — across all eight the P10 subtree is stable and only P9's cold n=16 headline and P13's `close_microseconds` move | `f8315cc54e3d` | **`a131e1f4b46d6c83`** | `3cb816e5b83dcf90` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`), `rate_ladder=true deliberate_mismatch_observed=true`. **Measured this session and not by any probe: RTS↔CTS is cross-wired in both directions, DTR moves no DSR/DCD/RI — a 5-wire crossover, not the 3-wire one the tree assumes** | 22 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-f8315cc-tier3-2.json`](linux-7.0-2026-08-05-f8315cc-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-tier3-3.json`](linux-7.0-2026-08-05-f8315cc-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-passive-1.json`](linux-7.0-2026-08-05-f8315cc-passive-1.json) | ” — **the same binary as the three rows above, run with no `--port`.** This pair is the measured form of the §5 declination recorded below: one binary, one box, one probe set, and **two different field sets** (`3cb816e5b83dcf90` with the rig named, `60a346baeeb0b3d9` without), because naming two ports adds P3/P5/P11 cells. Folding the keys into `probe set` would make these two report themselves incomparable, which is why it was declined | `f8315cc54e3d` | **`a131e1f4b46d6c83`** | `60a346baeeb0b3d9` | none (passive; the adapters are attached but unnamed, so P3/P5/P11 skip) | 18 supported · 0 degraded · 0 unsupported · 4 skipped |
| [`linux-7.0-2026-08-05-f8315cc-passive-2.json`](linux-7.0-2026-08-05-f8315cc-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-passive-3.json`](linux-7.0-2026-08-05-f8315cc-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-1a9a8fc-tier3.json`](macos-24.6.0-2026-08-05-1a9a8fc-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**. **The run that answered both pre-registered experiments of notes §3.44**, and the counterpart of the `-05b` Linux triple directly below: `1a9a8fca1c36` is `4b78fffc4bf2` plus a docs-only commit, so despite the different `commit` string these two triples are the **same binary** — checked with `git diff 4b78fff 1a9a8fc -- '*.rs' '*.toml'`, which is empty | `1a9a8fca1c36` | **`a131e1f4b46d6c83`** | `0c303d4cb11e3893` | **Tier-3 wiring, now partly certified** — the same cross-wired FT232R pair, `SNX_CROSSOVER=required`, proven on the wire the same session (4 passed, 32768 bytes byte-exact each way at 250000 baud) | 14 supported · 8 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-05-1a9a8fc-tier3-2.json`](macos-24.6.0-2026-08-05-1a9a8fc-tier3-2.json) | ” — second sequential run, same box, load 1.89–2.58 throughout | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-1a9a8fc-tier3-3.json`](macos-24.6.0-2026-08-05-1a9a8fc-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05b-tier3.json`](linux-7.0-2026-08-05b-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The counterpart the next macOS capture diffs against**: same binary, and the first Linux report carrying every observation added on 2026-08-05 (P6/P7 baseline block, P13 `baseline_packet_bytes`, P10 `recheck`, P9's zero-timeout 2×2) | `4b78fffc4bf2` | **`a131e1f4b46d6c83`** | `88585243dafb4747` | **Tier 3** — the cross-wired FT232R pair, and the first Linux run to certify it under the portable UART predicate (§3.42) | 22 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05b-tier3-2.json`](linux-7.0-2026-08-05b-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05b-tier3-3.json`](linux-7.0-2026-08-05b-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-tier3.json`](linux-7.0-2026-08-05-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box | `71fc5a815852` | **`a131e1f4b46d6c83`** | `64410fea8995f068` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`) | 22 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-tier3-2.json`](linux-7.0-2026-08-05-tier3-2.json) | ” — second sequential run, same box, same idle state | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-tier3-3.json`](linux-7.0-2026-08-05-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-7ead470-tier3.json`](macos-24.6.0-2026-08-05-7ead470-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**. The first macOS report that names its own kernel: `kernel` reads `24.6.0` and `os` reads `Darwin 24.6.0 (x86_64)` from `uname(2)`, so this column now repeats the artifact rather than supplying what it could not say | `7ead470f594c` | **`a131e1f4b46d6c83`** | `e0047234b499d0c7` | **Tier-3 wiring, uncertified** — the same cross-wired FT232R pair (`/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O`) | 15 supported · 7 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-05-7ead470-tier3-2.json`](macos-24.6.0-2026-08-05-7ead470-tier3-2.json) | ” — second sequential run, same box, same idle state | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-7ead470-tier3-3.json`](macos-24.6.0-2026-08-05-7ead470-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-tier3.json`](macos-24.6.0-2026-08-05-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**; `kernel`/`os` read empty and `unknown`, so the version is recorded here by hand. **That gap is fixed in the binary as of `71fc5a815852`** — the fields come from `uname(2)` now, and the three rows above are that fix landing | `fa4b12d6f529` | **`a131e1f4b46d6c83`** | `36fd95f08831bb38` | **Tier-3 wiring, uncertified** — the same cross-wired FT232R pair (`/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O`) | 15 supported · 7 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-07-30-tier3.json`](macos-24.6.0-2026-07-30-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**; the report's own `kernel`/`os` fields read empty and `unknown`, which is the gap `uname(2)` closes as of `71fc5a815852` (`docs/macos.md` delta 5), so the version is recorded here instead | `a1029778fda9` | `01b257ece8c48470` | `94a11ac201de6613` | **Tier-3 wiring, uncertified** — the same cross-wired FT232R pair, as `/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O` | 14 supported · 7 degraded · 0 unsupported · 3 skipped |
| [`linux-7.0-2026-07-29-tier3-2.json`](linux-7.0-2026-07-29-tier3-2.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `2e5874bbe090` | `01b257ece8c48470` | `76c9b8b293728e8e` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`) | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-6.18-2026-07-29-tier3.md`](linux-6.18-2026-07-29-tier3.md) | 6.18.14-1rodete4-amd64, Debian rodete — **the production box** | `85699d66c5a5` | `01b257ece8c48470` | **n/a** — Markdown, no `observations` array, so not computable: unknown, and unknown is not equal | **Tier 3** — two FTDI FT232R cross-wired (`BH00LL8O` ↔ `BH00L4KU`) | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-07-29-tier3.json`](linux-7.0-2026-07-29-tier3.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `da290c616631` | `01b257ece8c48470` | `76c9b8b293728e8e` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`), moved back | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-07-29-passive-1.json`](linux-7.0-2026-07-29-passive-1.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `85699d66c5a5` | `01b257ece8c48470` | `9612da13d806026c` | none (passive; no adapter attached) | 13 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-07-29-passive-2.json`](linux-7.0-2026-07-29-passive-2.json) | ” | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-07-29-passive-3.json`](linux-7.0-2026-07-29-passive-3.json) | ” | ” | ” | ” | ” | ” |

Files are named for the UTC day of their own `generated` stamp, which is why the
7.0 runs read `2026-07-29` despite being taken on the evening of the 28th local.
**A capture that shares a UTC day with an earlier capture from a different `commit`
carries the short sha as an extra segment** — `macos-24.6.0-2026-08-05-7ead470-tier3`
beside `macos-24.6.0-2026-08-05-tier3` — and the trailing `-2`/`-3` index is reserved
for sequential runs of *one* binary in one session. The two conventions have to be
kept apart because they mean opposite things: `-2` asserts "same instrument, run
again, so a difference is noise", and that is exactly the claim a same-day capture
from a *different* binary must not make. The 08-05 macOS pair is the case that forced
the rule — same box, same rig, same kernel, same `probe_set` fingerprint, and a P10
whose body changed between them, which is precisely the difference a fingerprint
cannot see. **One triple in this directory predates the rule and does not follow it**:
`linux-7.0-2026-08-05b-tier3` distinguishes itself from `linux-7.0-2026-08-05-tier3`
with a day suffix rather than its sha (`4b78fffc4bf2`). It is not renamed, because
renaming a committed artifact rewrites a record's identity; the `f8315cc` rows above
are the first Linux capture to spell the convention as written.

**"Tier 3" in the Rig column means two different things either side of the Linux
boundary, and the macOS rows say so.** P5 certifies a pair by characterizing each
port, and its UART predicate is `TIOCGICOUNT` — a Linux-only ioctl. On Darwin every
port therefore reports `cert: skipped (not characterizable here)`, the cross-pair
rate-ladder and deliberate-mismatch line is absent from the report altogether, and
the string `Tier 3` appears **nowhere in any macOS artifact** (it appears once in each
Linux Tier-3 artifact, in P5's own consequence). What the macOS captures do measure is
the *topology* — P5 pairs `BH00L4KU` ↔ `BH00LL8O` in both directions — which is
Tier-3 wiring. `doctor/src/probes.rs` already carries the exact phrase for this state,
"**Tier 3 wiring, uncertified**", and the rows above now use it rather than borrowing
the Linux word. The wiring is independently corroborated on the harness side: with the
rig attached, `itest`'s four `serial_hardware.rs` tests move 32768 bytes byte-exact in
each direction at 250000 baud.
<!-- ANNOTATION 2026-08-05 (§5). The macOS rows previously read "**Tier 3**", matching
     the Linux rows and their own filenames. That was prose asserting a certificate the
     artifact it indexes explicitly declines to issue — the §16.13 failure in its
     mildest form, and the same shape as the P13-figures defect the 08-05 sweep
     corrected. The filenames keep `tier3` because the *wiring* is Tier 3 and renaming
     a committed artifact rewrites a record's identity; only the claim is corrected. -->

## What "Tier 3" cannot mean on Darwin, and what these captures cost

The 2026-08-05 macOS trio was taken on an otherwise idle box (load 2.14 before, 1.48
after; 12 cores, no competing build), three sequential runs at ~13.75 s each. Three
rather than one for the reason stated below for the Linux side — but with a result the
Linux side does not have: **P10 does not move at all here.** All three runs are
byte-identical across P10's entire observations array, so on Darwin the depth is
deterministic, where the Linux figures needed three runs to show which differences were
noise. The fields that do move run to run are P9's poll granularity, P13's
`close_microseconds`, P6's `elapsed_ms` and P2's zero-timeout poll — timing only.

**Four of these reports predate the §15.40 rename and still say so inside** — the 6.18
Tier-3 report and the three 7.0 passive runs — because they are captured tool output:
their `tool` field carries the binary's pre-rename name, and nothing in this directory is
hand-edited to read more tidily. That is the whole point of committing them — an artifact
edited after the fact is an assertion again, not a check — so the retired-name meta-gate
exempts exactly those four by name, and `README.md` (this file) not at all. The **twenty-two**
remaining reports need no exemption — the two 07-29 7.0 Tier-3 runs, the three 08-05 7.0
Tier-3 runs, the three `-05b` runs, the three `f8315cc` Tier-3 runs, the three `f8315cc`
passive runs, and all eight macOS runs (the 07-30 capture, the 08-05 pre-repair capture, and
the `7ead470` and `1a9a8fc` triples):
all were produced by post-rename binaries and carry the current name on their own, which is
what "a future report will fix itself" looks like in practice.
<!-- ANNOTATION 2026-08-05 (§5). This count read **ten** and enumerated only the 07-29,
     08-05 and macOS-through-`7ead470` reports — it was not bumped when the `-05b` and
     `1a9a8fc` triples landed, so it undercounted by six before the `f8315cc` rows added
     another six. Corrected to twenty-two against the directory rather than against the
     previous sentence. -->

**Two of these rows are a passive/rig pair from one binary**, which is new: `f8315cc`'s
Tier-3 and passive triples share `commit`, box, kernel and `probe set` and differ in
`field set` (`3cb816e5b83dcf90` against `60a346baeeb0b3d9`). That is the §5 declination
below — folding the observation keys into `probe set` would make those two report
themselves incomparable — held as committed evidence rather than as a prediction, and
`itest/tests/meta_doctor_artifacts.rs` now gates on it.

## The probe set moved on 2026-08-05, and this directory now holds three families, not two

`macos-24.6.0-2026-08-05-tier3.json` carries probe set **`a131e1f4b46d6c83`**; the reports taken
before it carry `01b257ece8c48470` and none of them contains a P13 block at all — P13 joined the
probe set after they were taken. By this file's own rule that makes the 08-05 macOS report **not
field-by-field comparable with the older family**, including `macos-24.6.0-2026-07-30-tier3.json`
from the same box and the same rig. Read the difference between those two as the instrument
moving, never as Darwin changing its mind: P10's direction keys were also renamed in the same
window.

**The owed capture landed.** `linux-7.0-2026-08-05-tier3.json` (and its two sequential siblings)
were taken on the dev box at `71fc5a815852` with the FT232R crossover attached, and carry
`a131e1f4b46d6c83` — the *same* fingerprint as the macOS report. That pair is the first lawful
field-by-field cross-kernel diff of the P13 era, and P13's Linux figures are consequently no
longer a recorded measurement awaiting an artifact: they are in this directory and citable.
Three runs rather than one because P10's depths move run to run, and a single capture cannot
show a reader which differences are noise.

<!-- ANNOTATION 2026-08-05 (§5). This section previously ended: "that capture is owed and is not
     in this directory. Until it lands, P13's Linux figures live in docs/implementation-notes.md
     §3.30 as a recorded measurement, not as a citable artifact." Both clauses were true when
     written and are now discharged by the three Linux captures above. The sentence is replaced
     rather than annotated in place because it described the *state of this directory*, which the
     table above already reports — a standing inventory, not a finding. -->

**The probe bodies gained observations on 2026-08-05, and the fingerprint does not say so.**
Announced here because §3.34's standing rule says it must be: the digest covers `(id, question)`,
both unchanged. P6/P7 gained a baseline block (`silence_cause`, `extproc_retained_at_shape`,
`baseline_via_master`, …), P13 gained `slave_termios_mode` and `baseline_packet_bytes`, P10 gained a
`recheck` block, and P9 gained `zero_timeout_by_fd_state_and_mask`. **Every one of them runs after
the existing fields are final**, so reports taken either side of the change stay field-by-field
diffable on everything they share — which is why the fingerprint staying put is correct rather than
merely convenient.

<!-- ANNOTATION 2026-08-05 (§5). That last clause was the right call about `probe set` and the
     wrong conclusion overall. `probe set` staying put is correct — the questions did not move — but
     the sentence reads as though nothing needed to say the cells had moved, and the only thing that
     did say so was this paragraph, written by hand under §3.34's standing rule, *because no field
     could*. `field set` (added 2026-08-05, notes §3.51) now says it: the four commits named in this
     section print one `a131e1f4b46d6c83` and five different `field set` values, and `df48bfc` makes
     a fifth commit and a sixth value. The hand-announcement rule **stays**, and this paragraph with
     it: the digest says *that* the cells moved, never *which* — for that, diff the leaf paths
     (recipe under "Adding one"). -->


**One caution the new pair does not remove.** A shared fingerprint certifies that two runs asked
the same *questions*; it does not certify that they asked them of the same *configuration*. The
P13-era macOS report was taken before P10 learned to re-assert its baseline on the slave it
measures and to report `slave_termios_mode`, so its P10 block carries neither field and its
depths are not known to have been measured on the raw pty the daemon runs (`doctor/src/probes.rs`,
`termios_mode`). Diff P10 across that pair only once a macOS capture at the current binary
reports `slave_termios_mode: "raw"` on both directions.

<!-- ANNOTATION 2026-08-05 (§5). **That capture landed and the caution above is discharged for
     P10.** `macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json` report `slave_termios_mode:
     "raw"` on both directions in all three runs, at probe set `a131e1f4b46d6c83` — the same
     fingerprint as `linux-7.0-2026-08-05-tier3{,-2,-3}.json`, and at probe code provably
     identical to the binary that produced them (`git diff 71fc5a8..HEAD` touches only `AGENTS.md`
     and `docs/`; neither side's build stamp carries the `-dirty` suffix `doctor/build.rs` would
     have added). P10 may now be diffed across that pair, and the answer is below.

     **What the diff says, and how badly the pre-repair reading misled.** Linux 7.0.0-29 accepts
     and fully recovers **15360 bytes in both directions** (11776 first-pass + 3584 settled, 3
     writes, `bytes_unrecoverable: 0`, identical across its three runs). Darwin 24.6.0 accepts and
     fully recovers **1024 targetward and 1022 hostward** (1 write each, `ceiling_hit: false`,
     `bytes_unrecoverable: 0`, identical across its three runs). That is 15.0x and 15.03x, with
     **Linux the deeper kernel**. Read against the pre-repair macOS artifact the direction was
     inverted: 4194304 hostward against Linux's 15360 made Darwin look >=273x deeper, and even that
     was a floor, because `ceiling_hit: true` means the fill stopped at P10's own 4 MiB backstop
     and the blocking point was never observed at all.

     **The pre-repair run measured a pty outside the daemon's baseline — inferred, not measured,
     and the distinction is the point.** The pre-repair artifact carries no `slave_termios_mode`
     field, so it cannot say what discipline it was in; nothing in this directory can settle it
     from the artifact alone. What supports the reading is a single-variable source delta — the
     only functional change on P10's fill path between `fa4b12d6f529` and `71fc5a815852` is
     `set_baseline(&slave)` on the slave the probe measures — together with that same report's own
     P2 reading of `termios_settable_without_slave: false`, the condition under which
     `apply_pty_baseline` takes its open-and-immediately-close path. Write it that way. "Darwin's
     cooked pty accepts 4 MiB" is a sentence this directory cannot support.

     **Two things the new trio does NOT discharge.** (1) The sibling exposure: P6, P7, P9, P12 and
     P13 still take the `apply_pty_baseline` fallback, because only P10 learned to re-assert. That
     caution stands wherever it is written and a sweep must not delete it along with this one.
     (2) A two-byte asymmetry nothing in the tree predicts: Darwin accepts 1024 targetward but
     **1022** hostward, from a single 4096-byte write in each direction, reproducible byte-identically
     across all three runs. (ANNOTATION 2026-08-05: "Linux is symmetric at 15360" is withdrawn —
     six runs gave 13824 and 15360 independently per direction, so Linux's within-run asymmetry is
     768x Darwin's, and Darwin is the reproducible side. Notes §3.44.) It is recorded here as measured and
     unexplained rather than rounded away, because the next reader will otherwise take it for a
     transcription slip. -->

**A fingerprint still cannot see a body change, and this pair is the proof.** Both 08-05 macOS
captures carry `a131e1f4b46d6c83`; P10's *implementation* differs between them, and its hostward
figure differs by a factor of 4104 (4194304 -> 1022 — again a lower bound on the numerator, which
was never driven to a blocking point). That is a probe-versus-probe ratio on one box, not a
cross-kernel one, and it is the strongest available demonstration of why `probe set` equality is
necessary and not sufficient. The filename convention above exists to carry what the fingerprint
cannot — and as of 2026-08-05 so does a second digest: this pair reads `36fd95f08831bb38` against
`e0047234b499d0c7`, six leaf paths apart, which is the same fact stated by the instrument instead of
by a filename. It is still not the whole fact: the 4104x is a *body* change, and no digest computed
from a report can see one. `itest/tests/meta_doctor_artifacts.rs` freezes both halves of this pair
so a later simplification of the digest cannot quietly erase the counterexample.

Same binary, same commit, same fingerprint on both sides of the diff: the pre-P13 reports are
comparable field by field, and `docs/serial-nexus-doctor.md` does the
comparison. The recorded pre-2026-07-28 baselines are **not** in this set and are
not comparable to these — `doctor/src/probes.rs` moved 702 lines between
`a2d3b96` and `85699d6` (P12 arrived, P4 and P5 were rewritten). Those older runs
cannot even say so themselves: `a2d3b96` predates the Build block, so they carry no
fingerprint at all, which is exactly why the absent field has to read as "not
comparable" rather than as agreement. That is the mechanism doing its job on its
first outing, not a problem to work around.

## Two asymmetries in this set, both deliberate

**The 6.18 side is Markdown; the 7.0 side is JSON.** The 6.18 report is a verbatim
transcription of what the operator pasted from the production box's terminal, not a
`--json >` capture, so it is the human-facing rendering and there is no JSON twin.
The consequence is precise and worth stating rather than glossing:
`serial-nexus-doctor --json | jq -e -f expectations/linux.jq` **still has not been executed
on 6.18**. Every clause of that file is decidable by eye from this Markdown and every
one holds — including the `.build.probe_set` / `.build.commit` clauses added on
2026-07-28, which the older `fe1c52c`-vintage 6.18 artifact could not have answered —
but inference is not execution. Closing it costs one `--json` capture on that box.
<!-- ANNOTATION 2026-08-05 (§5). "Every clause holds" is now false for exactly one clause and
     stated rather than quietly amended: `.build.field_set` (notes §3.51) is absent from this
     Markdown, as it is from all nineteen JSON artifacts, because the field postdates every one of
     them. Nothing about 6.18 is implicated — the clause gates fresh `--json` output, and the
     capture that would close the gap above would carry the field. The sentence is left standing
     because it is a record of what was inferred when it was written. -->

(The gate *has* been executed against the 7.0 JSON here, which proves the HEAD probe
set and HEAD `linux.jq` agree; what it cannot prove is anything about 6.18.)

**The 7.0 side was passive, and now is not.** *(2026-07-29: the cross-wired pair came
back to the dev box, so `linux-7.0-2026-07-29-tier3.json` is a Tier-3 run on 7.0 —
same probe-set fingerprint as the 6.18 Tier-3 report above, and therefore diffable
against it field by field, which is the comparison this directory existed to make
possible and could not yet make. It also closes the `--json` gap named just above **on
the 7.0 side**: that clause still stands unchanged for 6.18. The three passive runs stay:
they are the sample-of-three that keeps P9's and P10's run-to-run variance from reading
as a cross-kernel difference, and a Tier-3 run does not replace them.)*

The three passive runs were passive because the dev box had no adapter attached at the
time — the cross-wired pair had physically moved to the production box, which is how that
box became Tier 3 — so P3, P5 and P11 skip in them for want of a `--port` and a port to
name. That costs the diff nothing for the kernel questions:
P1, P2, P6, P7, P8, P9 and P10 all run without hardware, and they are the whole
kernel-diff set. Three runs because P9's and P10's numbers move run to run on one
box, and one sample of a quantity that varies is indistinguishable from a
cross-kernel difference — which is the mistake the 2026-07-28 P10 reading came within
one run of making. Sequential runs on a quiet box (load 0.44, no other load), all
`target/debug`.

## Adding one

```sh
cargo build --workspace --locked
./target/debug/serial-nexus-doctor --json > docs/doctor/<os>-<kver>-<yyyy-mm-dd>[-<commit>][-<rig>][-N].json
./target/debug/serial-nexus-doctor --json | jq -e -f expectations/linux.jq   # or macos.jq
# With a rig, opt the ports in explicitly — this transmits, and a listed port could
# be wired to live equipment (§15.17):
./target/debug/serial-nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1 > docs/doctor/….md
# The cell-set digest of any captured report, including ones taken before the field
# existed (it is a pure function of .probes[].observations) — this is what computed the
# `Field set` column above, with the artifacts untouched:
./target/debug/serial-nexus-doctor --field-set docs/doctor/<file>.json
# WHICH cells moved between two reports — the digest says only that they did:
diff <(jq -r '[.probes[] as $p | $p.observations[] | $p.id + "." + .key] | sort | .[]' A.json) \
     <(jq -r '[.probes[] as $p | $p.observations[] | $p.id + "." + .key] | sort | .[]' B.json)
```

**What the first report carrying `field set` can and cannot be compared against.** Adding
the field is itself an observation-shape change — of the `Build` block, not of any probe
— so say it plainly rather than let a reader assume. Against **any later report**: by
field equality, directly, with no repository access; that is the property the field
exists to provide. Against the **nineteen frozen JSON artifacts**: only by recomputation,
i.e. the comparison needs the tool — which is why the column above records it once.
Against **`linux-6.18-2026-07-29-tier3.md`**: not at all; it is Markdown with no
`observations` array, its digest is not computable, and its cell set is therefore
unknown rather than equal. Nothing under `docs/doctor/` was edited to make room for the
column (§16.13), and the digest of every artifact was computed *before* the field
existed — which is exactly why those values could be published here at all.

Prefer `--json` (it is what the gate consumes and what diffs cleanly); commit the
Markdown when that is what the operator actually produced, as above, rather than
re-rendering it from memory. Add a row to the index and say what the rig was —
"supported" means strictly different things at Tier 1 and Tier 3, and the tier is
not recoverable from the verdict word alone.
