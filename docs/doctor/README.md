# Captured `serial-nexus-doctor` reports

The artifacts behind every cross-kernel claim in `docs/serial-nexus-doctor.md`, `AGENTS.md`
§7 and `docs/implementation-notes.md`. Until 2026-07-29 there were none: both the
7.0 baseline and the 6.18 run lived in session scratchpads, which made "P6 and P7
read field-for-field identical" a statement asserted in three documents with
nothing in the repository to check it against. These files are that check.

**Read the `Build` block first.** A cross-kernel diff is only meaningful between two
reports whose **`probe set`** fingerprints are equal — that digest covers the
deduplicated, sorted set of each probe's `(id, question)`, so equal fingerprints mean
the two runs asked their kernels the same questions. Unequal means the instrument
moved between the runs and a field-by-field comparison is reading two different
instruments, whatever the numbers look like. Two omissions from the digest are
deliberate and worth knowing here: the probe **title** (P3's carries the device path
and P3 is emitted once per `--port`, so a two-port and a zero-port run of one binary
would otherwise disagree — which is exactly the pair below) and the
**measurements** (those are what a diff compares). Correcting a report's prose
therefore does not invalidate an archived comparison. `commit` says which tree built
the binary; `generated` is a UTC stamp. A report with **no** `probe set` at all —
anything built before 2026-07-28 — is not comparable with these.

## Index

| File | Kernel / box | Binary | Probe set | Rig | Verdicts |
|---|---|---|---|---|---|
| [`linux-7.0-2026-08-05-tier3.json`](linux-7.0-2026-08-05-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box | `71fc5a815852` | **`a131e1f4b46d6c83`** | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`) | 22 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-tier3-2.json`](linux-7.0-2026-08-05-tier3-2.json) | ” — second sequential run, same box, same idle state | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-tier3-3.json`](linux-7.0-2026-08-05-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-tier3.json`](macos-24.6.0-2026-08-05-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**; `kernel`/`os` read empty and `unknown`, so the version is recorded here by hand. **That gap is fixed in the binary as of `71fc5a815852`** — the fields come from `uname(2)` now, and the next macOS capture will name its own kernel rather than needing this column to do it | `fa4b12d6f529` | **`a131e1f4b46d6c83`** | **Tier 3** — the same cross-wired FT232R pair (`/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O`) | 15 supported · 7 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-07-30-tier3.json`](macos-24.6.0-2026-07-30-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**; the report's own `kernel`/`os` fields read empty and `unknown`, which is the gap `uname(2)` closes as of `71fc5a815852` (`docs/macos.md` delta 5), so the version is recorded here instead | `a1029778fda9` | `01b257ece8c48470` | **Tier 3** — the same cross-wired FT232R pair, as `/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O` | 14 supported · 7 degraded · 0 unsupported · 3 skipped |
| [`linux-7.0-2026-07-29-tier3-2.json`](linux-7.0-2026-07-29-tier3-2.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `2e5874bbe090` | `01b257ece8c48470` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`) | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-6.18-2026-07-29-tier3.md`](linux-6.18-2026-07-29-tier3.md) | 6.18.14-1rodete4-amd64, Debian rodete — **the production box** | `85699d66c5a5` | `01b257ece8c48470` | **Tier 3** — two FTDI FT232R cross-wired (`BH00LL8O` ↔ `BH00L4KU`) | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-07-29-tier3.json`](linux-7.0-2026-07-29-tier3.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `da290c616631` | `01b257ece8c48470` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`), moved back | 21 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-07-29-passive-1.json`](linux-7.0-2026-07-29-passive-1.json) | 7.0.0-28-generic, Ubuntu 26.04 — the dev box | `85699d66c5a5` | `01b257ece8c48470` | none (passive; no adapter attached) | 13 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-07-29-passive-2.json`](linux-7.0-2026-07-29-passive-2.json) | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-07-29-passive-3.json`](linux-7.0-2026-07-29-passive-3.json) | ” | ” | ” | ” | ” |

Files are named for the UTC day of their own `generated` stamp, which is why the
7.0 runs read `2026-07-29` despite being taken on the evening of the 28th local.

**Four of these reports predate the §15.40 rename and still say so inside** — the 6.18
Tier-3 report and the three 7.0 passive runs — because they are captured tool output:
their `tool` field carries the binary's pre-rename name, and nothing in this directory is
hand-edited to read more tidily. That is the whole point of committing them — an artifact
edited after the fact is an assertion again, not a check — so the retired-name meta-gate
exempts exactly those four by name, and `README.md` (this file) not at all. The **seven**
remaining reports need no exemption — the two 07-29 7.0 Tier-3 runs, the three 08-05 7.0 Tier-3
runs and both macOS Tier-3 runs:
all were produced by post-rename binaries and carry the current name on their own, which is
what "a future report will fix itself" looks like in practice.

## The probe set moved on 2026-08-05, and this directory now holds two comparable families

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

**One caution the new pair does not remove.** A shared fingerprint certifies that two runs asked
the same *questions*; it does not certify that they asked them of the same *configuration*. The
P13-era macOS report was taken before P10 learned to re-assert its baseline on the slave it
measures and to report `slave_termios_mode`, so its P10 block carries neither field and its
depths are not known to have been measured on the raw pty the daemon runs (`doctor/src/probes.rs`,
`termios_mode`). Diff P10 across that pair only once a macOS capture at the current binary
reports `slave_termios_mode: "raw"` on both directions.

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
./target/debug/serial-nexus-doctor --json > docs/doctor/<os>-<kver>-<yyyy-mm-dd>[-<rig>].json
./target/debug/serial-nexus-doctor --json | jq -e -f expectations/linux.jq   # or macos.jq
# With a rig, opt the ports in explicitly — this transmits, and a listed port could
# be wired to live equipment (§15.17):
./target/debug/serial-nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1 > docs/doctor/….md
```

Prefer `--json` (it is what the gate consumes and what diffs cleanly); commit the
Markdown when that is what the operator actually produced, as above, rather than
re-rendering it from memory. Add a row to the index and say what the rig was —
"supported" means strictly different things at Tier 1 and Tier 3, and the tier is
not recoverable from the verdict word alone.
