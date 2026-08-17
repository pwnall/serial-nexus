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
**`field set`** — anything built before `b21548d`, the `f8315cc` rows being the first
captures to carry the digest they are indexed by —
has an *unknown* cell set, and unknown is never "equal": the column below carries the
recomputation (`serial-nexus-doctor --field-set <file>`), because the digest is a pure
function of `.probes[].observations` and so is computable for artifacts captured before
the field existed. That is what let this column be added without touching a frozen
artifact (§16.13).

**One row is neither, and it is the top one.** `linux-6.18-2026-08-07-3e23c52-tier3.md`
is Markdown, so its digest can be neither recomputed nor verified here — the column
carries what the report *printed*. Trust it exactly as far as you trust the box that
produced it, which is the reason plan §18 item 8 asks for `--json` and stays open until
it gets it. The older `linux-6.18-2026-07-29-tier3.md` has the same limit and predates
the field entirely, so it reads `n/a`.

**A new fingerprint era opened 2026-08-07: `82a8e2198e54626a` → `e79f5fcd86a2e5f0`.**
P15's `question` cited §15.51, which is P14's section; design §15.53 is P15's own entry
and had already recorded the debt as discharged while the code still carried the wrong
number. Correcting it moves the digest, and **that was the right trade rather than a
regrettable side effect** — the alternative was accumulating captures under a citation
known to be wrong. Everything indexed below is therefore a **closed era**: the rows
remain comparable with each other, including the 6.18↔7.0 pair immediately below, which
is still the first lawful cross-kernel Linux comparison this repository has. But no
capture from a current binary joins them, and P1–P14 must not be diffed across the
boundary without the mismatch stated (§15.44 — the unequal direction is a verdict, and
here it is announcing a real instrument change). **The Linux half of the new era is
committed** — the six `2b44c17` rows, passive and Tier-3 triples
from one clean build, with `jq -e` *executed* against both halves rather than inspected.
The macOS half and a 6.18 capture are owed.
*(Read "everything indexed below is a closed era" as **everything below the `2b44c17`
rows**. The `e79f5fcd86a2e5f0` era ran until 2026-08-13 and took a second Linux
binary before it closed — the `8c00078-dirty` triple — and the paragraphs below are
its whole story: one `field_set`-only move inside it, then the `probe_set` move that
closed it.)*
*(**"The macOS half is owed" is discharged one era later, not in this one, and the
difference is permanent.** `e79f5fcd86a2e5f0` closed on 2026-08-13 with no Darwin
capture ever taken in it, so that half is now **unobtainable** rather than pending —
the same shape as the never-taken passive half of the era-closing `8c00078-dirty`
triple, and recorded for the same reason. What landed instead is the macOS half of the
**successor** era, `4317ea5ac187f506`, as the six `b346188` rows at the top of the
index: both halves, one clean build, `jq -e` executed against all six. So the standing
debt moves from "a Darwin capture at the current era" — paid — to the 6.18 visit,
which is plan §18 item 8(a) and is unaffected by any of this.)*

**Two moves landed on 2026-08-13, and only the second one is an era boundary.**
That distinction is the point of having two digests, so it is spelled out rather
than left to the fingerprints.

**Move one: `field_set` only, and the era did NOT move.** Plan §18 items 14 and 22
each added observation **keys** to an existing probe — P15 gained a
software-flow-control (`IXON`/`IXOFF`) block per port, P13 gained a fifth shape,
`e_reader_arrives_during_close_wait` — and neither touched a `question` string. So
`probe_set` stayed `e79f5fcd86a2e5f0` (era law clause 4: cells added under
unchanged questions close nothing) while `field_set` moved: Tier-3
`fc01990f2e38876e` → `c4ed6e7ef3f8088f`, passive `141e256d40c1e83e` →
`544dca850580d430`. The `8c00078-dirty` triple is that move's artifact and it is an
**era-mate** of the `2b44c17` rows: diff them on the intersection, which is every
cell both carry.

Two things about that move the digests cannot say:

1. **Which cells appeared.** P13 gained one top-level key
   (`e_reader_arrives_during_close_wait`) carrying seventeen leaves, of which
   `bytes_recovered_by_arriving_reader` and the whole `reader_arrival` block are new
   shapes of cell rather than new instances of an old one. The other four P13
   shapes are **unchanged** — no `bytes_recovered_by_arriving_reader: 0` was stamped on
   them, deliberately, because a structurally-zero cell on four shapes costs every
   future intersection and buys nothing. P15 gained thirteen leaves per named
   port, all under `software_flow_control`.
2. **What the new cells do not license.** Both blocks say so in their own output —
   `does_not_license` on P13's arrival row and on P15's software block — because
   nothing in the gate set reads prose and a bound that lives only here is a bound
   the next reader will not meet (§13's gate-blind-spot rule).

**Move two: a new fingerprint era, `e79f5fcd86a2e5f0` → `4317ea5ac187f506`,
carrying two changes deliberately folded into one boundary** (design §15.59).

- **P16 is why it moved.** A new probe id is a new question and a new question is a
  new instrument, so the digest moves and this era closes — the recorded kind of
  move, not a drifted one. P16 asks whether a held pts **slave** fd can tell that
  its master has gone: `poll(POLLHUP)` while the master is open, where it must stay
  quiet, and again after it closes, where it must fire. Beside it, the `stat`
  comparison `itest`'s `SlaveWitness::prove_open` performs, mirrored step for step,
  so a report says what the *shipped* check would have done here rather than what
  the probe thinks of it.
- **P15's `question` widening rode with it, in the same commit.** The string now
  names both flow-control kinds instead of `CRTSCTS` alone, and P15's verdict
  degrades on a silently-dropped software request. That widening was **deliberately
  held back** from move one: correcting a `question` moves `probe_set` (era law
  clause 3), and spending a boundary on a *wording* change while P16's real
  instrument change was already owed would have closed two eras where one would do.
  Folding them cost the archive one boundary instead of two, and it is the whole
  reason move one shipped with a header narrower than its body for one commit.
  (What the daemon did was unchanged *at that boundary*: no `xon-xoff` pre-check, no
  refusal at `load`. That held for one day. §15.61 then met item 14's decline on its
  own stated condition — a dropping driver was measured — so the pre-check consults
  this cell for `xon-xoff` too and an accept-then-drop reading predicts a refusal at
  `load`/`add-node`. Recorded here rather than rewritten away, because the sentence
  was true of the boundary it describes; notes §3.101, plan §18 item 72.)

So a capture from `4317ea5ac187f506` and one from `e79f5fcd86a2e5f0` must not be
diffed field by field without the mismatch stated, **including for P1–P14**, whose
questions did not move: the unequal direction of `probe_set` is a verdict, and here
it is announcing a real instrument change (§15.44).

## Index

| File | Kernel / box | Binary | Probe set | Field set | Rig | Verdicts |
|---|---|---|---|---|---|---|
| [`linux-7.0-2026-08-17-a7e6070-tier3.json`](linux-7.0-2026-08-17-a7e6070-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, 20 cores. **The first CDC-ACM artifacts in this directory, and the first of any device class other than FTDI.** The bench is a WCH `1a86:55d3` pair on the `cdc_acm` driver at `/dev/ttyACM0`/`ttyACM1`, cabled 5-wire by the operator. P5's handshake block reads **`UNREADABLE handshake: RTS/CTS gave no usable reading at either drive level — this is not a 3-wire answer`** with both CTS cells `stuck-high`, byte-identical across all three runs. **Read that as a statement about the transport, not the cable:** `cdc_acm` synthesises CTS (the CDC `SERIAL_STATE` notification has no CTS field), so on this device class *no instrument in this tree can tell a 5-wire bench from a 3-wire one* — §15.52's closing claim is scoped at §15.62 for exactly this. **Nothing here may be diffed against a `ttyUSB` row**: chip, driver, device class, node name, cable and adapter pair all moved at once. `probe_set` is unchanged, so no era is owed. `jq -e -f expectations/linux.jq` was **executed** against all ten of this session's artifacts, exit 0 on every one — which measures the handshake clause's presence-never-answer form from a third side, the same gate having passed on 5-wire and 3-wire reports (plan §3 rule 14) | `a7e6070c1000` | **`4317ea5ac187f506`** | `bbaedf7e08672ecd` | **Tier 3** — the cross-wired CDC-ACM pair (`5A7C298854` ↔ `5A7C297954`), **wired 5-wire, unreadable**: both RTS/CTS cells `stuck-high`, all six DTR crossings `false` | 25 supported · 1 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-17-a7e6070-tier3-2.json`](linux-7.0-2026-08-17-a7e6070-tier3-2.json) | ” — second sequential run, same box, same build | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-17-a7e6070-tier3-3.json`](linux-7.0-2026-08-17-a7e6070-tier3-3.json) | ” — third sequential run; the eight handshake cells are identical in all three | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-17-a7e6070-passive-1.json`](linux-7.0-2026-08-17-a7e6070-passive-1.json) | ” — **the passive half of the same binary**, taken before the Tier-3 triple, so this day has both halves from one clean build | `a7e6070c1000` | **`4317ea5ac187f506`** | `5ba96312f8aa436c` | none (passive; the adapters are attached and cross-wired but unnamed, so P3/P5/P11/P14/P15 skip) | 20 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-17-a7e6070-passive-2.json`](linux-7.0-2026-08-17-a7e6070-passive-2.json) | ” | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-17-a7e6070-passive-3.json`](linux-7.0-2026-08-17-a7e6070-passive-3.json) | ” | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-16-8759516-tier3.json`](linux-7.0-2026-08-16-8759516-tier3.json) | ” — **the same bench one commit earlier, kept as the committed evidence of the defect the row above repairs.** Its handshake line reads **`3-wire: no handshake lines carried [rts_a_to_cts_b=stuck-high rts_b_to_cts_a=stuck-high …]`** — the shape sentence contradicting its own cells, on a bench the operator wired 5-wire (plan §18 item 80). **The pair is the demonstration of §15.52's stated residual blindness**: the handshake is one string cell, so its value changing moves *neither* digest — `probe_set` and `field_set` are identical to the row above, and only the sentence differs. Also the pre-repair P14 cost: `search_elapsed_ms` **2089033** against a 180 s budget, whole-run wall clock **4148 s**, against ~90 s for each of the three runs above (item 83) | `87595165743a` | **`4317ea5ac187f506`** | `bbaedf7e08672ecd` | **Tier 3** — the same CDC-ACM pair, same cabling | 25 supported · 1 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-16-8759516-passive-1.json`](linux-7.0-2026-08-16-8759516-passive-1.json) | ” — the passive half of the pre-repair build | `87595165743a` | **`4317ea5ac187f506`** | `5ba96312f8aa436c` | none (passive) | 20 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-16-8759516-passive-2.json`](linux-7.0-2026-08-16-8759516-passive-2.json) | ” | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-16-8759516-passive-3.json`](linux-7.0-2026-08-16-8759516-passive-3.json) | ” | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-14-b58a1c4-tier3.json`](linux-7.0-2026-08-14-b58a1c4-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, 20 cores, load 1.12 before and 1.33 after. **The first 5-wire artifact of the `4317ea5ac187f506` era, and the first in this directory since 2026-08-07.** The operator re-cabled the bench and P5's handshake block reads `5-wire crossover: RTS/CTS both ways, DTR moves nothing` — `rts_a_to_cts_b=true rts_b_to_cts_a=true`, all six DTR crossings `false` — byte-identical across all three runs. **What this row is worth, and what it is not.** It is worth that this era now holds *both* wirings on one kernel with `probe_set` fixed, which no other era does, so a handshake-conditional cell can be diffed within an era for the first time. It is **not** a cable-only diff: the adapter pair moved too (`BH00LW9U` ↔ `BH00L4KU` here against `ABSCDGL6` ↔ `BH00L4KU` in the `8c00078` rows), one adapter common, so anything read across those rows carries the same confound plan §18 item 20 records one instrument over. `field_set` moves `c83ba6dd08faf8e3` → `969b084fd6f920c8` **within** the era — cells added by the trees between `8c00078` and here (items 66, 67, 12, 72 and the macOS session's `doctor+sys` commits), while `probe_set` does not move, so no question changed and the rule is the directory's usual one: diff the intersection. `jq -e -f expectations/linux.jq` was **executed** against all six of this day's artifacts, exit 0 on every one — which also measures the handshake clause's presence-never-answer form from its other side, the same gate having passed on 3-wire reports (§15.52, plan §3 rule 14) | `b58a1c4b7fc8` | **`4317ea5ac187f506`** | `969b084fd6f920c8` | **Tier 3** — the cross-wired FT232R pair (`BH00LW9U` ↔ `BH00L4KU`), **5-wire, measured**: RTS/CTS crossed both ways, DTR moving nothing on all six crossings | 27 supported · 1 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-14-b58a1c4-tier3-2.json`](linux-7.0-2026-08-14-b58a1c4-tier3-2.json) | ” — second sequential run, same box, same build | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-14-b58a1c4-tier3-3.json`](linux-7.0-2026-08-14-b58a1c4-tier3-3.json) | ” — third sequential run; the eight handshake crossings are identical in all three | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-14-b58a1c4-passive-1.json`](linux-7.0-2026-08-14-b58a1c4-passive-1.json) | ” — **the passive half of the same binary**, taken before the Tier-3 triple, so this day has both halves from one clean build and `jq -e` was executed against all six rather than inspected. Sixth committed instance of the §5 folding declination: same `probe set` as the Tier-3 rows, different `field set`, one binary | `b58a1c4b7fc8` | **`4317ea5ac187f506`** | `8442d39d90a106c5` | none (passive; the adapters are attached and cross-wired but unnamed, so P3/P5/P11/P14/P15 skip) | 19 supported · 1 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-14-b58a1c4-passive-2.json`](linux-7.0-2026-08-14-b58a1c4-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-14-b58a1c4-passive-3.json`](linux-7.0-2026-08-14-b58a1c4-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-13-b346188-tier3.json`](macos-24.6.0-2026-08-13-b346188-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — MacBookPro15,1, 12 cores, load 2.3 before and after. **The macOS half of the `4317ea5ac187f506` era, which the `-p16` row below records as owed** — and it is the row P16 was built for. **The two booleans disagree, exactly as that row pre-registered.** `poll_can_tell_a_live_pair_from_a_dead_one` is `true` — the held slave fd is quiet in all 200 tight passes and all 64 paced ones while the master is open, then `POLLHUP` arrives **6–16 µs** after it closes with a following `read(2)` answering `eof` — while `stat_comparison_can_tell` is **`false`**: `path_still_resolves` reads `true` on **both** sides of the close, so `shipped_prove_open_would_refuse` never moves off `false` and `fstat_on_the_held_fd_answers` stays `true` throughout. **So `itest`'s shipped `SlaveWitness::prove_open` is unsound on this kernel — measured rather than predicted**, and plan §18 item 26's pre-registered consequence follows as written: the portable upgrade is a `serial_nexus_sys` `poll` helper, not an argument. **Two more readings this triple is the first artifact for.** (a) **P13's fifth shape as the measurement rather than the control.** On the kernel that `waits-then-discards` a reader arriving *inside* the close-wait ends it: `close_microseconds` **14** against shape `a`'s **600368** on the same run, 64 of 64 recovered by the arriving reader, `arrived_before_close_returned: true`. So the ~600 ms is what a reader that *never arrives* costs, not a floor the close pays regardless — which is the discrimination plan §18 item 22 filed the shape to make. (b) **P15's software half, and it is the dropping driver plan §18 item 14's decline named its condition on.** Both ports: `tcsetattr_ok: true` with `tcsetattr_error: null`, `c_iflag` `0x0` → `0x0` — a delta of **nothing** — `ixon_on_readback: false`, `ixoff_on_readback: false`, `silently_dropped: true`, **`serial2_readback_would_fault: true`**, `baseline_restored: true`. Reproduced 6 of 6 across three runs and two ports, against `ftdi_sio`'s `0x5` → `0x1405` on **the same two adapters** one row below. **That sameness is what makes this pair the cleanest cross-kernel comparison in this directory**: same `probe_set`, same adapters, same 3-wire cable, same wiring — only the kernel differs, so a cell that moves is a kernel fact and not a rig one | `b3461886e27a` | **`4317ea5ac187f506`** | `27a9975a8e460e7c` | **Tier 3** — the cross-wired FT232R pair (`ABSCDGL6` ↔ `BH00L4KU`), **3-wire, measured**: P5's handshake block reads `3-wire: no handshake lines carried` on all eight crossings. **The first macOS artifact in this directory to print `Topology: **Tier 3**`**, which is notes §3.49's pre-registered fix holding — see the paragraph at the foot of this file | 16 supported · 10 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-13-b346188-tier3-2.json`](macos-24.6.0-2026-08-13-b346188-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-13-b346188-tier3-3.json`](macos-24.6.0-2026-08-13-b346188-tier3-3.json) | ” — third sequential run. All three share a `field set`; P16's stat/poll disagreement and P15's zero `c_iflag` delta are identical across them, and P13's arriving-reader close reads 14 µs in all three against a shape-`a` close of 600368 / 600104 / 600125 µs (plan §3 rule 14: three runs, because the quantity varies) | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-13-b346188-passive-1.json`](macos-24.6.0-2026-08-13-b346188-passive-1.json) | ” — **the passive half of the same binary**, taken before the Tier-3 triple, so this era has both halves from one build on *both* kernels and `jq -e -f expectations/macos.jq` was **executed** against all six rather than inspected. Fifth committed instance of the §5 folding declination: same `probe set` as the Tier-3 rows, different `field set`, one binary. P16 runs identically here — it needs no `--port` | `b3461886e27a` | **`4317ea5ac187f506`** | `16ecf4f0004ff026` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14/P15 skip) | 13 supported · 7 degraded · 0 unsupported · 8 skipped |
| [`macos-24.6.0-2026-08-13-b346188-passive-2.json`](macos-24.6.0-2026-08-13-b346188-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-13-b346188-passive-3.json`](macos-24.6.0-2026-08-13-b346188-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-tier3.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, load 1.78 before, 1.26 after (20 cores). **The first artifact of the `4317ea5ac187f506` era**, opened by P16 (§15.59) with P15's `question` widening folded into the same boundary. **P16's Linux reading, and it is a two-instrument answer.** The held slave fd is quiet in all 200 back-to-back passes (93–107 µs) and all 64 paced passes (~328 ms) while the master is open, then `POLLHUP` arrives **1 µs** after it closes — `revents` `POLLERR|POLLHUP`, a following `read(2)` answering `eof` — identical in all three runs. So `poll_can_tell_a_live_pair_from_a_dead_one` is `true`, and each arm is the other's control: the quiet window is the negative arm the firing one needs, and vice versa. Beside it, `stat_comparison_can_tell` is **also** `true`: after the master's close `fstat_on_the_held_fd_answers: true` while `path_still_resolves: false`, which is the `/dev/pts/N` unlink notes §3.60 measured by hand, now in an artifact. **On this kernel the harness's shipped comparison is sound and P16 is a control proving it so.** The row this probe exists for is the one where those two booleans **disagree** — `poll` `true`, `stat` `false` — and that is Darwin's expected shape and was owed until the `b346188` triple at the top of this index paid it, on the same adapters and the same cable, reading exactly that. Note the filename's `-p16` segment: this triple shares a UTC day *and* a `commit` with the era-closing triple below, and only the instrument differs, so neither existing convention could tell them apart (see the naming paragraph at the foot of this file) | `8c00078466c2-dirty` | **`4317ea5ac187f506`** | `c83ba6dd08faf8e3` | **Tier 3** — the cross-wired FT232R pair (`ABSCDGL6` ↔ `BH00L4KU`), **3-wire, measured**: P5's handshake block reads `3-wire: no handshake lines carried` on all eight crossings | 25 supported · 1 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-tier3-2.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-tier3-3.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-tier3-3.json) | ” — third sequential run. All three share a `field set`, and P16's reading is identical across them down to the microsecond (plan §3 rule 14: three runs, because the quantity varies) | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-passive-1.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-passive-1.json) | ” — **the passive half of the same binary**, so this era has both halves from one build at its opening and `jq -e` was *executed* against both rather than inspected. Fourth committed instance of the §5 folding declination: same `probe set` as the Tier-3 rows, different `field set`, one binary. P16 runs identically here — it needs no `--port`, which is the point of putting a pty question in a pty probe | `8c00078466c2-dirty` | **`4317ea5ac187f506`** | `49da8ea5ad078d90` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14/P15 skip) | 19 supported · 1 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-passive-2.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-p16-passive-3.json`](linux-7.0-2026-08-13-8c00078-dirty-p16-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-tier3.json`](linux-7.0-2026-08-13-8c00078-dirty-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, load 1.21 before, 1.96 after (20 cores). **The first artifact carrying plan §18 items 14 and 22, and the last of the `e79f5fcd86a2e5f0` era** — an era-mate of the `2b44c17` rows rather than an era-opener: same `probe_set`, moved `field_set`, for the reasons in the paragraph above. The era closed hours later with P16 (§15.59); the `-p16` rows at the top of this index are the other side of that boundary, and this triple must not be diffed against them field by field without the mismatch stated. **Read three things off it.** (a) **P13's fifth shape**, `e_reader_arrives_during_close_wait` — a reader that arrives *while the kernel is inside its close-wait*, which is the shape notes §3.29's unexplained macOS red inhabits and which the four committed shapes structurally cannot produce. Here it is a **control proving itself inert**, in P12's sense: this kernel `retains`, so an arriving reader changes no byte count, and what the row shows is that the instrument works — the reader wins the race 3 of 3 across this triple, first `read(2)` at 0–1 µs against a close returning in 3–7 µs, 64 of 64 recovered, terminal `EIO`. On Darwin, where the close parks for ~600 ms, the same row is the measurement: whether the arrival ends the wait with the bytes, or whether the close still pays its full timeout — which is the difference between "a lost microsecond race" and "a reader stalled for 601 ms". **That capture is owed.** (b) **P15's software half**: `ftdi_sio` **honours** `IXON`/`IXOFF` on both ports, `c_iflag` `0x5` → `0x1405`, a delta of exactly the two flags, `serial2_readback_would_fault: false`, `baseline_restored: true`. That is the first artifact on any kernel to answer the question plan §18 item 14 filed as *unmeasured, not known-good* — and it answers it for one driver on one kernel, which is why the item's decline (no `load`-time refusal without a dropping driver) is unchanged by it. (c) **The `commit` stamp reads `-dirty` and the filename says so.** Three sessions were editing this tree concurrently, so `doctor/build.rs` could not stamp a clean sha and the same-binary claim (§15.44 rung 2) is **not** available for this row. What *is* available and was checked: `git diff a2054a9 8c00078 -- doctor/ core/ sys/ rpc/ codec-api/ Cargo.lock` is **empty**, so the only source difference between this capture and the pre-change capture it is diffed against is the change itself — the same-source rung, scoped to the one pair it was taken for. The `group:dialout` environment check reads `degraded` (this box reaches the adapters through an ACL rather than through group membership), which is why the totals below carry a degraded where the `2b44c17` rows do not | `8c00078466c2-dirty` | **`e79f5fcd86a2e5f0`** | `c4ed6e7ef3f8088f` | **Tier 3** — the cross-wired FT232R pair (`ABSCDGL6` ↔ `BH00L4KU`), **3-wire, measured**: P5's handshake block reads `3-wire: no handshake lines carried` on all eight crossings. A different pair from the 5-wire `BH00LL8O` ↔ `BH00L4KU` rig every row below was taken on — read P5's own handshake line, never this column's memory | 24 supported · 1 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-13-8c00078-dirty-tier3-2.json`](linux-7.0-2026-08-13-8c00078-dirty-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-13-8c00078-dirty-tier3-3.json`](linux-7.0-2026-08-13-8c00078-dirty-tier3-3.json) | ” — third sequential run. All three share a `field set`, and the arriving-reader row reads the same way in all three (plan §3 rule 14: three runs, because the quantity varies). **The passive half of this binary was never taken and now cannot be** — the era closed with P16 before it was, so this triple's counterpart is the `-p16` passive triple at the top of the index, on the other side of a `probe_set` boundary. Recorded rather than quietly dropped: it is the cost of taking a rig capture and an instrument change in one session, and the next such move should take both halves before the boundary | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-07-2b44c17-tier3.json`](linux-7.0-2026-08-07-2b44c17-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, load 0.20–0.27. **The first artifact of the `e79f5fcd86a2e5f0` era**, and the capture the era boundary above owes. The era opened because P15's `question` cited §15.51, P14's section, against design §15.53 which is P15's own entry (notes §3.73). **Three things here are new cells rather than new numbers.** P9 publishes its within-group order control for the first time — `order_control_says: excludes-warmup-above-tolerance` with both groups `flat` and the fitted `order_control_tolerance_x100: 150` beside them, so the threshold can be re-derived rather than re-argued (notes §3.74); read `flat` as "nothing exceeded the resolution", **not** as a strong pass, since no committed Linux group has ever discriminated. P5's handshake carries **eight** crossings against the previous six — `dtr_b_to_dcd_a` and `dtr_b_to_ri_a` were asserted by the verdict and never measured — and the pre-registered reading held exactly, both `false`, verdict unchanged. And **no probe carries a duplicate observation key**, which every artifact below this row does for P14 (27 observations, 24 distinct): `Probe::observe` replaces in place now, so `max_reliable_baud` appears once, with its measured value, in the placeholder's position | `2b44c1700d17` | **`e79f5fcd86a2e5f0`** | `5d99bdc231f8376d` | **Tier 3** — the cross-wired FT232R pair (`BH00LL8O` ↔ `BH00L4KU`), 5-wire, RTS/CTS both ways, both ports named | 25 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-07-2b44c17-tier3-2.json`](linux-7.0-2026-08-07-2b44c17-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-07-2b44c17-tier3-3.json`](linux-7.0-2026-08-07-2b44c17-tier3-3.json) | ” — third sequential run. All three share a `field set`, and P9's order-control reading is identical across them | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-07-2b44c17-passive-1.json`](linux-7.0-2026-08-07-2b44c17-passive-1.json) | ” — **the passive half of the same binary**, so this era has both halves from one build, and the `jq -e` gate was *executed* against both rather than inspected. Third committed instance of the §5 folding declination: same `probe set` as the Tier-3 rows, different `field set`, one binary | `2b44c1700d17` | **`e79f5fcd86a2e5f0`** | `a02e07f3f25e45b9` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14/P15 skip) | 19 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-07-2b44c17-passive-2.json`](linux-7.0-2026-08-07-2b44c17-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-07-2b44c17-passive-3.json`](linux-7.0-2026-08-07-2b44c17-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-6.18-2026-08-07-3e23c52-tier3.md`](linux-6.18-2026-08-07-3e23c52-tier3.md) | 6.18.14-1rodete4-amd64, Debian rodete — **the production box**. **The first lawful field-by-field 6.18↔7.0 comparison this repository has ever been able to make**, and the artifact behind notes §3.73. It pairs with the `7cf0338` Linux triple below on a basis stronger than either digest: `git diff 7cf0338 3e23c52 -- doctor/ core/ sys/ rpc/ codec-api/ Cargo.lock` is **empty**, so both came from the *same doctor source*, which closes the one blindness both digests share — a probe body that moves a number without moving a key. Read the two together and **nothing differs**: zero booleans, strings, errno histograms, byte counts, ceilings, ioctl-availability flags or termios modes moved across 16 probe sections, so all five deferred "diff against the production kernel" decisions are discharged and none licenses a change. P13, P14 and P15 execute on 6.18 for the first time here — the 2026-07-29 row below is P1–P12 at a dead probe era and is **not** comparable with this. **Read it with its two bounds.** It is **Markdown**, so `field set` is not recomputable from it and the digest below is the one the report printed rather than one this repository verified; and it is a *pasted* capture, which is why plan §18 item 8 stays open — no test has ever run on 6.18 and `jq -e -f expectations/linux.jq` has still never been **executed** there. Committed as a record under the precedent the 2026-07-29 row set | `3e23c524184c` | **`82a8e2198e54626a`** | `179c9d15c6e450f5` — **as printed, not recomputed** (Markdown carries no `observations` array); equal to the `7cf0338` triple, and note that a *later* binary would not match it, since `b8e4d8f` sorts P5's pair key | **Tier 3** — the same cross-wired FT232R pair (`BH00LL8O` ↔ `BH00L4KU`), 5-wire, RTS/CTS both ways | 24 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-7cf0338-tier3.json`](linux-7.0-2026-08-05-7cf0338-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box, load 0.26–0.9. **The Linux half of the P15 era, and the artifact behind every "Linux honours `CRTSCTS`" sentence in the tree** (notes §3.68). Until this capture that claim was asserted as measured fact by notes §3.65 E′/§3.67 with nothing committed behind it — `grep -l '"P15"' docs/doctor/*.json` returned only the three `acb5162` macOS files — which is exactly what §7 forbids. **P15 reads `supported` here and it is the first execution of that arm on any kernel:** `honoured_on_readback: true`, `silently_dropped: false`, `shipped_predicate_agrees: true` and `baseline_restored: true` on both ports, with `cflag_before_hex` `0x10021cb2` → `cflag_after_hex` `0x90021cb2` — a delta of exactly `CRTSCTS`. Darwin reads `0x4b00` → `0x4b00` and `honoured_on_readback: false`, so the probe's three-way discrimination is now measured on both arms rather than tested purely on one. **Read the cross-kernel pair with its bound**: the `acb5162` macOS triple shares this `probe set` and does *not* share the `field set`, because `shipped_predicate_agrees` did not exist until `17c6e87` — equal `probe set` is not comparability, and this is that rule's first cross-kernel instance rather than a same-binary pair. The macOS lane owes a capture at this tree | `7cf0338973d8` | **`82a8e2198e54626a`** | `179c9d15c6e450f5` | **Tier 3** — the cross-wired FT232R pair, `SNX_CROSSOVER=required`, both ports named | 24 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-7cf0338-tier3-2.json`](linux-7.0-2026-08-05-7cf0338-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-7cf0338-tier3-3.json`](linux-7.0-2026-08-05-7cf0338-tier3-3.json) | ” — third sequential run. All three share a `field set`, and P15's reading is identical across them | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-7cf0338-passive-1.json`](linux-7.0-2026-08-05-7cf0338-passive-1.json) | ” — **the passive half of the same binary**, so this era has both halves from one build. It is also the second committed instance of the §5 folding declination: same `probe set` as the Tier-3 rows above, different `field set`, from one binary — which is what makes a passive and a rig run comparable-by-instrument and not comparable-by-cell | `7cf0338973d8` | **`82a8e2198e54626a`** | `eaf4323f4213618b` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14/P15 skip) | 18 supported · 0 degraded · 0 unsupported · 6 skipped |
| [`linux-7.0-2026-08-05-7cf0338-passive-2.json`](linux-7.0-2026-08-05-7cf0338-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-7cf0338-passive-3.json`](linux-7.0-2026-08-05-7cf0338-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-acb5162-tier3.json`](macos-24.6.0-2026-08-05-acb5162-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**, load 1.35–1.63. **A new fingerprint era: `probe_set` moves `94d64d8bbacf1174` → `82a8e2198e54626a` because P15 is a new question** (notes §3.65 E′). Do not diff this triple field-by-field against anything above it; read P1–P14 across the boundary only with the digest mismatch stated. **P15 is why the era moved, and this is its first artifact on any kernel:** it asks each named port for `CRTSCTS`, reads it back, and restores the termios it found. Here it reads `degraded` with `silently_dropped: true` on **both** ports — `tcsetattr_ok: true`, `cflag_before_hex` and `cflag_after_hex` both `0x4b00`, `baseline_restored: true` — which is Apple's `IOSerialFamily` driver accepting a request and discarding it. That is the measurement behind the one red test this platform carries (`rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes`), and it is deliberately an observation rather than a verdict: whether a `rts-cts` edge should degrade or keep faulting is a design question P15 exists to inform, not to answer. A driver that *refuses* the request would read `supported` here, which is the distinction the probe is built around | `acb516289a4c` | **`82a8e2198e54626a`** | `157543c1242ddd20` | **Tier 3** — the same cross-wired FT232R pair, `SNX_CROSSOVER=required`, both ports named | 14 supported · 10 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-05-acb5162-tier3-2.json`](macos-24.6.0-2026-08-05-acb5162-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-acb5162-tier3-3.json`](macos-24.6.0-2026-08-05-acb5162-tier3-3.json) | ” — third sequential run. All three share a `field set` | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-42eac2a-tier3.json`](macos-24.6.0-2026-08-05-42eac2a-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**, load 1.30–2.12. **The first Darwin capture of the P14 era, and the cross-kernel counterpart of the `3d850cf` Linux triple below**: same `probe set`, so P1–P14 diff field by field on every cell both carry. A Tier-3 run costs **50.1 s** here against Linux's 35.0 s, essentially all of it P14. **Two results are worth reading first.** P14 finds the *same* ceiling as Linux — `max_reliable_baud 3000000`, `first_unreliable_baud 3062500`, identical in all three runs — but classifies the refusal differently: `ceiling_kind` is **`platform-refused`** here against **`adapter-refused`** on Linux, so the same physical limit is attributed to different layers by the two kernels, which is exactly the kind of thing §15.51's `ceiling_kind` exists to expose. And P13's fourth shape **agrees across kernels while the policy disagrees**: `d_no_reader_second_fd_held` reads `bytes_recovered_total: 64` with terminal `EAGAIN` on *both*, while `policy` is `waits-then-discards` here and `retains` there — the witness fd normalises two opposite last-close dispositions to one reading, which is the measured form of notes §3.56's argument and the reason the seven guards it converted are sound off Linux (notes §3.65 C) | `42eac2aa4919` | **`94d64d8bbacf1174`** | `da21ac7678aeebaa` | **Tier 3** — the same cross-wired FT232R pair, `SNX_CROSSOVER=required`, both ports named | 14 supported · 9 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-05-42eac2a-tier3-2.json`](macos-24.6.0-2026-08-05-42eac2a-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-42eac2a-tier3-3.json`](macos-24.6.0-2026-08-05-42eac2a-tier3-3.json) | ” — third sequential run. All three share a `field set`, and P14's ceiling and bound are identical across them | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-3d850cf-tier3.json`](linux-7.0-2026-08-05-3d850cf-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The counterpart triple for the owed 6.18 visit** (plan §18 item 8), taken from a clean tree so `commit` carries no `-dirty` and the same-binary claim stays provable. New this capture: **P13's fourth shape**, `d_no_reader_second_fd_held`, which holds a second fd on the same pts across the writer's close and so measures the **last-close reference count** — the premise notes §3.56's whole harness architecture rests on and that nothing measured before. On this kernel it is *not* inert and the moving cell is not the expected one: the byte counts agree (64 of 64 either way, because Linux retains) while the **terminal read** moves, `EIO` when the writer's close is the last one against `EAGAIN` while the witness is held. The hangup itself is deferred to the reference-count edge. A kernel that discards at last close must move the byte counts instead, which is the diff a 6.18 or Darwin run should be read for. Also new: P14 stamps `max_reliable_baud`/`ceiling_kind`/`ceiling_is_a_floor_over` on **every** path, so a `degraded` report no longer fails the gate — the defect that would have reddened the first ever executed 6.18 gate on a marginal cable (notes §3.64) | `3d850cf4417e` | **`94d64d8bbacf1174`** | `0d4ec9c1d2f31766` | **Tier 3** — the same cross-wired FT232R pair, `5-wire crossover`, `max_reliable_baud 3000000` / `adapter-refused` | 23 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-3d850cf-tier3-2.json`](linux-7.0-2026-08-05-3d850cf-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-3d850cf-tier3-3.json`](linux-7.0-2026-08-05-3d850cf-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-3d850cf-passive-1.json`](linux-7.0-2026-08-05-3d850cf-passive-1.json) | ” — **the passive half of the same binary**, and it exists because a cross-kernel diff needs both halves from one build. The previous passive triple was `77f6798`, one code-commit behind its own Tier-3 rows, so a 6.18 passive capture would have had nothing same-binary to sit beside | `3d850cf4417e` | **`94d64d8bbacf1174`** | `4fb7323b4ccfcf11` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14 skip) | 18 supported · 0 degraded · 0 unsupported · 5 skipped |
| [`linux-7.0-2026-08-05-3d850cf-passive-2.json`](linux-7.0-2026-08-05-3d850cf-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-3d850cf-passive-3.json`](linux-7.0-2026-08-05-3d850cf-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-bf29500-tier3.json`](linux-7.0-2026-08-05-bf29500-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The P14 repairs, and a clean demonstration of what the second digest is for**: `probe_set` is *unchanged* from the `77f6798` triple below — the question did not move — while `field_set` goes `df20be9b6c22d1d9` → `5de37cdbb1c94c23`, because the probe body grew cells. That is the pair working as designed (§15.44), and it is the direction §15.44 can see; the residual it cannot see is a body that moves a *number* without moving a *key*. The new cells are `achieved_baud_floor` per direction, a per-trial `failure`, and `search_stops_at`. **Why `achieved_baud_floor` exists is the finding of that repair**: the driver's read-back is not the wire's rate. Timed over this rig, every rate from 115200 to 3000000 reads back exactly what was asked and achieves ~0.94 of it (this instrument's overhead) — but 2500000 and 2750000 achieve **0.76 and 0.70**, both landing at ~1.9 Mbaud, because the FT232R rounds to its nearest divisor and `ftdi_sio` echoes the request back. The bytes still round-trip, since both ends are mis-set identically. So `max_reliable_baud` is a **requested** rate, and this column now carries the cell that shows the difference | `bf29500d18de` | **`94d64d8bbacf1174`** | `5de37cdbb1c94c23` | **Tier 3** — the same cross-wired FT232R pair, `5-wire crossover` measured, `max_reliable_baud 3000000` / `adapter-refused` identical across all three runs | 23 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-bf29500-tier3-2.json`](linux-7.0-2026-08-05-bf29500-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-bf29500-tier3-3.json`](linux-7.0-2026-08-05-bf29500-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-77f6798-tier3.json`](linux-7.0-2026-08-05-77f6798-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The first P14 capture, and it opens a new fingerprint era: `probe_set` moves for the first time since `a131e1f4b46d6c83`.** That is deliberate and it is the *sound* direction — a new probe id is a new question, so every row below is now refused by the digest rather than diffed across (§15.44). **Do not diff this triple field-by-field against anything above `f8315cc`; read P1–P13 across the boundary only with the digest mismatch stated.** P14 reads `max_reliable_baud: 3000000`, `ceiling_kind: adapter-refused`, bracketed to `first_unreliable_baud: 3062500` — identical in all three runs, with the search costing 21951/21964/21961 ms. **What the ceiling actually is:** `ftdi_sio` accepts a 4000000 ask at the syscall and reads back **9600**, so the refusal is the driver landing four hundred times below the request with no errno. P5 also carries §15.52's handshake block for the first time, and it reproduces the hand measurement of notes §3.53 (i): **`5-wire crossover: RTS/CTS both ways, DTR moves nothing`** — the first time that fact has been in a report rather than in a session note. Taken on a settled box (load 0.39–0.44); a Tier-3 run now costs **35.0 s** against §3.53's 11.6 s, all of the increase being P14 | `77f6798fe02e` | **`94d64d8bbacf1174`** | `df20be9b6c22d1d9` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`), `rate_ladder=true deliberate_mismatch_observed=true`, and now `5-wire crossover` measured rather than assumed | 23 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-77f6798-tier3-2.json`](linux-7.0-2026-08-05-77f6798-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-77f6798-tier3-3.json`](linux-7.0-2026-08-05-77f6798-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-77f6798-passive-1.json`](linux-7.0-2026-08-05-77f6798-passive-1.json) | ” — **the same binary as the three rows above, run with no `--port`**, and the §5 folding declination's evidence reproduced in the new era: one binary, one box, one probe set, two field sets. P14 `skipped` here, which is the whole reason a passive run still costs ~3.9 s | `77f6798fe02e` | **`94d64d8bbacf1174`** | `4f64bb873eca052f` | none (passive; the adapters are attached but unnamed, so P3/P5/P11/P14 skip) | 18 supported · 0 degraded · 0 unsupported · 5 skipped |
| [`linux-7.0-2026-08-05-77f6798-passive-2.json`](linux-7.0-2026-08-05-77f6798-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-77f6798-passive-3.json`](linux-7.0-2026-08-05-77f6798-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-tier3.json`](linux-7.0-2026-08-05-f8315cc-tier3.json) | 7.0.0-29-generic, Ubuntu 26.04 — the dev box. **The first Linux capture of the P9/P10/P12/P4/P5 repairs**, all five of which were developed on the Mac: `df48bfc`, `50af61e`, `5c3e697`, `448f562`, `b21548d`, `f8315cc` landed after the `-05b` triple below and no Linux run existed for any of them. Also the first report to carry `build.field_set` in the file rather than only in this column. Taken on a settled box (load 0.20–0.31); eight sequential runs were taken and the first three are committed — across all eight the P10 subtree is stable and only P9's cold n=16 headline and P13's `close_microseconds` move | `f8315cc54e3d` | **`a131e1f4b46d6c83`** | `3cb816e5b83dcf90` | **Tier 3** — the same cross-wired FT232R pair (`BH00L4KU` ↔ `BH00LL8O`), `rate_ladder=true deliberate_mismatch_observed=true`. **Measured this session and not by any probe: RTS↔CTS is cross-wired in both directions, DTR moves no DSR/DCD/RI — a 5-wire crossover, not the 3-wire one the tree assumes** | 22 supported · 0 degraded · 0 unsupported · 1 skipped |
| [`linux-7.0-2026-08-05-f8315cc-tier3-2.json`](linux-7.0-2026-08-05-f8315cc-tier3-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-tier3-3.json`](linux-7.0-2026-08-05-f8315cc-tier3-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-passive-1.json`](linux-7.0-2026-08-05-f8315cc-passive-1.json) | ” — **the same binary as the three rows above, run with no `--port`.** This pair is the measured form of the §5 declination recorded below: one binary, one box, one probe set, and **two different field sets** (`3cb816e5b83dcf90` with the rig named, `60a346baeeb0b3d9` without), because naming two ports adds P3/P5/P11 cells. Folding the keys into `probe set` would make these two report themselves incomparable, which is why it was declined | `f8315cc54e3d` | **`a131e1f4b46d6c83`** | `60a346baeeb0b3d9` | none (passive; the adapters are attached but unnamed, so P3/P5/P11 skip) | 18 supported · 0 degraded · 0 unsupported · 4 skipped |
| [`linux-7.0-2026-08-05-f8315cc-passive-2.json`](linux-7.0-2026-08-05-f8315cc-passive-2.json) | ” — second sequential run | ” | ” | ” | ” | ” |
| [`linux-7.0-2026-08-05-f8315cc-passive-3.json`](linux-7.0-2026-08-05-f8315cc-passive-3.json) | ” — third sequential run | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-f8315cc-tier3.json`](macos-24.6.0-2026-08-05-f8315cc-tier3.json) | Darwin 24.6.0 / macOS 15.7.8, x86_64 — **the Mac**, load 2.21–2.83. **The Darwin half of the pair whose Linux half is four rows up**: same `commit`, same `probe set`, so this is a lawful field-by-field diff on every cell both carry, and the first one this directory has ever held for the P9/P10/P12/P4/P5 repairs. Rig proven on the wire the same session (4 passed in 15.38 s, 32768 bytes byte-exact each way at 250000 baud). **Read it as the artifact of a defect as well as a measurement** (notes §3.65 B): it prints P9 `shape: "1x2"` and a rationale asserting that an unrequested `POLLHUP` is delivered, beside its own `hangup_delivered_to_a_mask_that_requested_nothing: false` — the Linux row above answers `true` and is why the premise survived review. This is the last report of that framing. Its `field set` differs from the Linux row for the ordinary reasons (device-identity keys, P8 skipping here, P12 carrying observations here and none there), so diff the intersection | `f8315cc54e3d` | **`a131e1f4b46d6c83`** | `b453fd23ef659240` | **Tier 3** — the same cross-wired FT232R pair, `SNX_CROSSOVER=required`, both ports named | 13 supported · 9 degraded · 0 unsupported · 3 skipped |
| [`macos-24.6.0-2026-08-05-f8315cc-tier3-2.json`](macos-24.6.0-2026-08-05-f8315cc-tier3-2.json) | ” — second sequential run, same box | ” | ” | ” | ” | ” |
| [`macos-24.6.0-2026-08-05-f8315cc-tier3-3.json`](macos-24.6.0-2026-08-05-f8315cc-tier3-3.json) | ” — third sequential run. The whole P10 subtree is byte-identical across all three | ” | ” | ” | ” | ” |
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
from a *different* binary must not make.

**A third segment joined them on 2026-08-13, for a case neither covers.** The
`-p16` rows share a UTC day *and* a `commit` with the era-closing triple below
them — the tree was uncommitted through both captures, so `doctor/build.rs` stamped
the same `<sha>-dirty` on each — and what separates them is the **instrument**:
`e79f5fcd86a2e5f0` against `4317ea5ac187f506`. The sha segment cannot say that
(it is identical), and `-2`/`-3` would say the opposite of the truth. So a capture
taken across a `probe_set` boundary from an indistinguishable build carries a short
name for the move that opened the era. Prefer a clean commit and let the sha do this
work; the segment exists because a dirty tree cannot. The 08-05 macOS pair is the case that forced
the rule — same box, same rig, same kernel, same `probe_set` fingerprint, and a P10
whose body changed between them, which is precisely the difference a fingerprint
cannot see. **One triple in this directory predates the rule and does not follow it**:
`linux-7.0-2026-08-05b-tier3` distinguishes itself from `linux-7.0-2026-08-05-tier3`
with a day suffix rather than its sha (`4b78fffc4bf2`). It is not renamed, because
renaming a committed artifact rewrites a record's identity; the `f8315cc` rows above
are the first Linux capture to spell the convention as written.

**"Tier 3" in the Rig column means two different things either side of the Linux
boundary, and the macOS rows say so.** P5 certifies a pair by characterizing each
port; its UART predicate **was** `TIOCGICOUNT`, a Linux-only ioctl, and under that
predicate every Darwin port reported `cert: skipped (not characterizable here)` with
the cross-pair rate-ladder line absent from the report altogether — the state the
`fa4b12d` and `7ead470` rows below index. §15.47 widened the predicate to
`TIOCMGET || TIOCGICOUNT`, and from `1a9a8fc` onward the Darwin captures **do** carry
per-port certificates and a `rate_ladder=true` pair line; read the artifact, not this
paragraph, for which state a given row is in. **The sentence that stood here until
2026-08-13 said the string `Tier 3` appears "nowhere in any macOS artifact committed
here", and it has been overtaken by a measurement rather than by an argument.** It was
true of every row it was written about, and it named its own expiry: "every macOS
capture in this directory predates §3.49's hoist of the tier sentence out of the
certified arm, so on those runs the tier was computed and never printed. A macOS
capture taken after that hoist would print it." The `b346188` triple is the first
macOS capture taken after that hoist, and it **does** print it — `Topology: **Tier
3** — 1 cross-wired pair, independent clocks, so the rate ladder and the deliberate
baud mismatch ran`, in P5's own consequence, exactly where the Linux artifacts carry
it. The other half of §3.49's pre-registration held in the same line: the two
uncertifiable items are **named as unmeasurable on this kernel** rather than listed
bare — `icounter` on each port and the pair's `deliberate_mismatch`, all three
attributed to `TIOCGICOUNT` being Linux-only, with "that is the platform, not the rig:
re-seating a cable cannot change it". So P5 still reads `degraded` here and the tier
line is printed beside the declined items rather than instead of them, which is the
distinction the annotation below was drawn to protect. For the rows *below* the
`b346188` triple the original sentence stands unchanged. What those older macOS
captures do measure is the *topology* — P5 pairs `BH00L4KU` ↔ `BH00LL8O` in both
directions — which is Tier-3 wiring. `doctor/src/probes.rs` already carries the exact
phrase for that state, "**Tier 3 wiring, uncertified**", and those rows use it rather
than borrowing the Linux word. The wiring is independently corroborated on the harness side: with the
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

**One run, both consumers.** The capture and the gate read the *same* `Report`: `--json-out`
writes the JSON twin of the run that is already happening, so the artifact you commit is the
report the gate passed. Two invocations of one binary on one box are **two measurements** — P9's
and P10's numbers move run to run, which is the whole reason the diffs above take three samples —
so a recipe that captured one run and gated another was quietly comparing across them (notes
§3.74, plan §18 item 43).

```sh
cargo build --workspace --locked
# ONE run. The `.json` is committed; the gate reads that same file.
./target/debug/serial-nexus-doctor --json-out docs/doctor/<os>-<kver>-<yyyy-mm-dd>[-<commit>][-<rig>][-N].json
jq -e -f expectations/linux.jq docs/doctor/<...>.json   # or macos.jq
# With a rig, opt the ports in explicitly — this transmits, and a listed port could
# be wired to live equipment (§15.17). Markdown to read, JSON twin to keep, one run:
./target/debug/serial-nexus-doctor --port /dev/ttyUSB0 --port /dev/ttyUSB1 \
    --markdown --json-out docs/doctor/<...>.json > docs/doctor/….md
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
exists to provide — **with one exception, measured rather than predicted (notes
§3.73). `b21548d`-era digests are not field-equal to a post-`b8e4d8f` capture, and the
inequality is a key *spelling*, not a cell.** `b8e4d8f` canonicalised P5's pair key by
sorting the two port names, so the committed Linux Tier-3 family splits in two:
`7cf0338`, `77f6798`, `f8315cc` and `-05b` carry the unsorted spelling, while
`bf29500`, `3d850cf`, `linux-7.0-2026-08-05-tier3*`, the 2026-07-29 pair and every
macOS artifact carry the sorted one. Re-spelling only those two keys in a `7cf0338`
artifact moves its digest `179c9d15c6e450f5` → `f81d4a7b0828d37f`. So a report from the
unsorted family and one from a current binary read *unequal* while carrying the same
cells; "diff only the intersection" would drop exactly the two P5 cells that carry the
rig certificate and the 5-wire handshake. Diff within a family, or re-capture both
sides at one tree. Against the **nineteen frozen JSON artifacts**: only by recomputation,
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

**The date in a filename is the operator's session date, not the UTC one, and the two
disagree often enough to say so** — **27 of the 82 committed artifacts** carry a filename
date one day behind their own `generated_utc`, which is what an evening capture west of
UTC produces (`macos-24.6.0-2026-08-05-acb5162-*` is stamped `2026-08-06Z`; the
`linux-7.0-2026-08-14-b58a1c4-*` family is stamped `2026-08-15Z`). This is a convention,
not a defect, and the artifacts are unedited either way (§16.13) — but the line above
saying `generated` is a UTC stamp is true of the *field* and not of the *name*, and
reading it as both makes a correctly-named capture look misfiled. **Order rows by
`generated_utc` when the order matters**; the filename date is for finding the session,
not for sequencing captures.
