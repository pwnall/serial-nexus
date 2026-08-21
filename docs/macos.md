# macOS support status

macOS is a **best-effort** tier (design §13). Linux is the required platform and
the one every mechanism is specified against; macOS is supported where plain
POSIX carries the design, and degrades — never crashes, never silently misbehaves
— everywhere the design leans on a Linux-only facility.

## Update — 2026-08-21 at `3a39896`: the §15.62 adapters on Apple's CDC stack, and three refuted conclusions

**The box.** The same x86_64 MacBookPro15,1 rig box as the `b346188` block below, now on **macOS 15.8
(24H16)** — that block reads 15.7.8, so the OS build moved and provenance here says so. Darwin 24.6.0.

**The rig is new to this page and is not new hardware.** The two WCH `1a86:55d3` CDC-ACM adapters
that produced design §15.62 on Linux — serials `5A7C297954` and `5A7C298854`, confirmed by
`system_profiler` rather than by node name — arrived attached to this box on the same cable, as
`/dev/cu.usbmodem5A7C2979541` and `/dev/cu.usbmodem5A7C2988541` on `AppleUSBCDCCompositeDevice` →
`AppleUSBACMData` → `IOSerialBSDClient`. Hardware and cable fixed; kernel and driver moved. Six
artifacts are committed at `docs/doctor/macos-24.6.0-2026-08-21-3a39896-*`, and
`jq -e -f expectations/macos.jq` exits 0 on all six.

**Figures.** **1000 passing · 0 failed · 7 ignored** at default CI scope. **997 · 3 · 7** on a rig
lane spelled `SNX_CROSSOVER=required` + `SNX_CROSSOVER_A`/`_B` + `SNX_TLS=required` +
`SNX_WEB_UI=required` + `SNX_EXEC_CODEC=required`, **with `SNX_RIG_FLOW` and `SNX_REPLUG` dropped** —
the first legitimately (no handshake is readable on this transport and both flow modes are refused
at `load`), the second because the replug lane is Linux-only. **This page's older figure of 955 for
the 2026-08-13 session disagrees with the plan's Status row for the same session, and that row's own
arithmetic does not close** — filed as plan §18 item 94; no delta is quotable across the two.

**Four deltas this platform adds, all in the transport rather than in the tree.**

1. **A byte-exact count is not a whole transfer.** 1024 bytes in, 1024 bytes out, **8 of 128
   position-tagged records missing and 8 delivered twice**, 5 of 5 runs at 115200. Present in 54 of
   54 trials across both directions and 9600/115200/921600; **eliminated by pacing writes ~20 ms
   apart** (0 faults over 3 reps), which puts it in concurrent in-flight transfers rather than the
   wire. Transmit-side versus receive-side is **not established**, and both adapters hang off USB
   hubs here, which no run separated. **The tree detects it**: three rig guards fail with a SHA-256
   mismatch — `crossover_rig_data_plane_send_and_exclusivity`,
   `crossover_rig_custom_baud_byte_exact`, `exclusive_write_lock_is_byte_exact`. A length check
   passes all three, and so does plan §3's `received + dropped_slow_consumer == sent` fingerprint.
2. **A rate this stack accepts and echoes exactly can carry nothing.** With payload held constant at
   240 bytes: 15000, 15600, 16800 and 20000 delivered **0 or 1 byte**; 9600, 14400, 19200, 38400,
   57600 and 115200 were byte-exact. `IOSSIOSPEED` returns success at every one and `tcgetattr`
   echoes the ask — and `serial2` uses that same path here, so a node at 15000 comes up
   `status="active" actual_baud=15000` and is dead, past §7.1 clause 7's ±2.5 % read-back. **This is
   not "non-standard rates are dead":** P5's ladder round-trips the non-standard `CUSTOM_BAUD =
   250_000` on this same pair. Four rates were asked and four carried nothing; what selects them is
   unknown, and **the Linux arm is untested because that ladder has never asked these rates.**
3. **P14's `max_reliable_baud = 14400` on this bench is not a rate ceiling** and must not be quoted
   as one. Two probe policies met two transport defects: the constant-airtime payload (`baud/40`)
   handed the 19200 rung 480 bytes, which delta 1 breaks, and refinement then landed every midpoint
   between 14400 and 19200 on a line coding delta 2 kills.
4. **Apple's `IOSerialFamily` drops `CRTSCTS` on a second device class.** Both ports read
   `honoured_on_readback: false`, `silently_dropped: true`, `c_cflag 0x4b00 → 0x4b00` — so
   §15.53's refusal **fires** here where it does not on Linux `cdc_acm`, and an `rts-cts` node is
   refused at `load`. `IXON`/`IXOFF` are dropped too. The protection is real and **not aimed**: it
   fires on this driver being honest about dropping the flag, not on any instrument that can see
   wire inertness.

**P5 prints `3-wire: no handshake lines carried` here**, on a bench the operator reports as 5-wire
and which Linux calls `UNREADABLE … this is not a 3-wire answer`. Both sentences describe one
physical bench and both cannot be licensed; §15.68 and plan §18 item 92 carry it. **Darwin's CTS
path itself works** — `macos-24.6.0-2026-08-05-42eac2a-tier3.json` reads `true`/`true` on an FTDI
pair on this same OS — so this is a device-class limit, not a platform one.

**The cable was then moved, and it settles the cabling question** (notes §3.118). On the FT232R
fixture `BH00L4KU` ↔ `BH00LW9U`, three independent instruments read the same thing: P5 prints
`5-wire crossover: RTS/CTS both ways, DTR moves nothing`, a standalone `TIOCMGET` probe follows the
far CTS at both drive levels in both directions, and the suite's
`crossover_rig_rts_crosses_to_the_far_ports_cts` — the daemon's own `state.modem_lines`, which is
the field §7.1 promises an operator — **runs and passes** instead of self-skipping. **So the cable
the CDC-ACM bench called `3-wire` carries RTS/CTS**, and the re-crimp harm is demonstrated. All six
DTR crossings are `false`, so **item 28 stays blocked for a fourth cabling**. Artifacts:
`docs/doctor/macos-24.6.0-2026-08-21-3a39896-ftdi5w-tier3{,-2,-3}.json`.

**That move is also the control for delta 1.** The identical probe reads **128 of 128 distinct
records, 0 lost, 0 duplicated, 5 of 5** on FTDI — same box, same USB hubs, same cable, same payload
and rate — against 8 lost and 8 duplicated on the CDC-ACM bench. Not topology, not the harness, not
Darwin generally: **this transport.**

**The FTDI lane reads 1000 · 0 · 7** with `SNX_CROSSOVER` / **`SNX_RIG_FLOW`** / `SNX_TLS` /
`SNX_WEB_UI` / `SNX_EXEC_CODEC` all `required` — **the first macOS lane here to carry
`SNX_RIG_FLOW=required`**, and still **not** the documented lane, which also spells
`SNX_REPLUG=required` and is Linux-only on this platform. `rts_cts_flow_control_stalls_the_writer…`
passes on its *load-refusal* arm: a 5-wire bench does not make `rts-cts` usable on an OS that drops
the flag.

**The method note worth carrying.** Three of the session's four conclusions were refuted by its own
adversarial pass, including one bug in its measuring program that inverted a finding's severity: a
counter named `records_seen` counted *parsed* records rather than *distinct* ones, so a loss masked
by an equal duplication read as "nothing lost". Notes §3.117 records all of it.

## Update — 2026-08-13 at `b346188`: the era's macOS capture, two Linux-shaped guards found, and three pre-registrations answered

Same box (MacBookPro15,1, Darwin 24.6.0 / macOS 15.7.8, x86_64, 12 cores), tree clean at
`b346188`. **The rig is the same cross-wired FT232R pair the current Linux rows were taken on**
(`ABSCDGL6` ↔ `BH00L4KU`), same 3-wire cable — so this and
`linux-7.0-2026-08-13-8c00078-dirty-p16-tier3` are the first Darwin/Linux pair in the record with
`probe_set`, adapters, cable and wiring all held fixed. Full reading in notes §3.93.

**The rig proven on the wire first**, per the protocol §3.45 set: `SNX_CROSSOVER=required` with
both ports named, `serial_hardware.rs` **6 passed** in 19.88 s, 32768 bytes byte-exact each way at
250000 baud. P5's handshake block reads `3-wire: no handshake lines carried` on all eight
crossings, and `rts_cts_flow_control_stalls_the_writer_instead_of_losing_bytes` prints "driver
accepts rts-cts and drops it — asserting the load refusal instead", which is §15.53's shipped
behaviour meeting the driver it was written for.

**Six committed captures, one build, gate executed on all six.** Passive triple and Tier-3 triple,
`--json-out` on each, `jq -e -f expectations/macos.jq` exit 0 every time — plan §18 item 18's
capture half, discharged. **This capture is the first macOS artifact in `docs/doctor/` to print
`Topology: **Tier 3**`**, which is §3.49's pre-registration holding: the tier line is in P5's
consequence exactly where the Linux artifacts carry it, with `icounter` and `deliberate_mismatch`
named as unmeasurable on this kernel rather than listed bare. P5 still reads `degraded`, which is
the honest direction §3.42 established.

**Three pre-registered readings, all answered.**

- **`SlaveWitness::prove_open` is unsound here** (item 26's first branch, item 66 filed).
  `path_still_resolves` reads `true` on **both** sides of the master's close — Darwin's devfs nodes
  persist where Linux unlinks `/dev/pts/N` — so `shipped_prove_open_would_refuse` never moves and
  `stat_comparison_can_tell` is `false`. The seven guards notes §3.56 converted are held here by
  the compile-time borrow alone. `poll_can_tell` is `true` in the same row: `POLLHUP` 6–16 µs after
  the close, `read(2)` answering `eof`, quiet through 200 tight and 64 paced passes before it.
- **`IOSerialFamily` drops `IXON`/`IXOFF` exactly as it drops `CRTSCTS`** (item 14's owed Darwin
  arm; item 67 filed). Both ports, 6 of 6: `tcsetattr_ok: true`, `c_iflag` `0x0` → `0x0`,
  `serial2_readback_would_fault: true`. Against `ftdi_sio` honouring it (`0x5` → `0x1405`) on the
  same two adapters. Item 14's decline was **conditional** on a dropping driver being found; one is.
- **A reader arriving inside a pts close-wait ends it** (item 22's second kernel). Shape `a` — no
  reader — pays **600368 µs** and loses all 64 bytes; a reader arriving inside that window returns
  the close in **14 µs** with 64 of 64 recovered. So the ~600 ms is what a reader that *never*
  arrives costs, not a floor. That narrows notes §3.29's reader-stall hypothesis for the `p8_map`
  CI red: a stall long enough to matter is a reader that does not arrive at all.

**Two macOS-only test defects, found by running the suite here and red in CI's `macos` job on
every push since they landed** (plan §18 item 69). Both are **proxies in space** in AGENTS §9's
sense — guards written on Linux by sessions that could not run them here, which is the shape item
12 is open for.

1. `probes::tests::the_software_readback_reports_unmeasurable_rather_than_answering` took its
   baseline `Termios` off a **pty master**. Linux answers `tcgetattr` there; **Darwin answers
   ENOTTY**, so the test died in its own setup. Repaired by reading the *slave*, which is a
   terminal on both kernels and what every other pty test in that module already does.
2. `both_gates_refuse_an_unsupported_verdict_and_are_shown_able_to` assumed "the one
   cross-platform difference is P12", so it could shape a report for the other platform's
   expectation file only from Linux. From a Mac it panicked on its own precondition and **blamed a
   drift that had not happened**. Measured rather than reasoned: splitting `linux.jq` into its 33
   top-level conjuncts and evaluating each against the P12-shaped Darwin report, **exactly one
   fails** — `(any(.probes[]; .id == "P2" and .status == "supported"))`, §7.2's BSD arm, which this
   page already names as the expected macOS answer for P2. The premise needed one more cell, not a
   smaller scope.

**955 passing · 0 failed · 7 ignored** at default CI scope with `--no-fail-fast --nocapture`, 126
result lines over 122 cargo targets. The run before the two repairs read 953 · 2 · 7 and is kept in
the Status table, because it is the measurement that found them. **105 self-skips**, against the
Linux authority row's 13 at the same scope: the rig is attached but `SNX_CROSSOVER_A`/`_B` are
unexported at default scope, so every serial test skips by design — which is why a macOS
default-scope figure is not comparable to a Linux one test-for-test, and no delta between them is
derived anywhere.

## Update — 2026-08-05 hardware validation at `1a9a8fc`: both §3.44 experiments answered, and one converted test goes red on the rig

Same box (MacBookPro15,1, Darwin 24.6.0 / macOS 15.7.8, x86_64, 12 cores), same cross-wired FT232R
pair (`BH00L4KU` ↔ `BH00LL8O`), tree clean at `1a9a8fc`. Full reading in notes §3.45 and §3.46.

**The rig is proven on the wire first, then used.** `SNX_CROSSOVER=required` with both ports named:
`serial_hardware.rs` 4 passed in 15.46 s, 32768 bytes byte-exact each way at 250000 baud. *That
duration is genuinely re-measured this session and coincides with the `7ead470` figure recorded
below to the hundredth of a second; it is not carried over.* Doctor
captures were taken next, on a box at load 1.89–2.58 with no competing builds and before any
analysis agents started (§8) — three sequential runs, `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json`.

**These captures and the `-05b` Linux triple are the same binary**, not merely the same fingerprint:
`1a9a8fca1c36` is `4b78fffc4bf2` plus a docs-only commit, and `git diff 4b78fff 1a9a8fc -- '*.rs'
'*.toml'` is empty. That matters because `probe_set` digests only `(id, question)` and cannot state
it — four binaries in `docs/doctor/` print `a131e1f4b46d6c83` while macOS alone gains **65**
newly-present observation leaf paths between `7ead470` and `1a9a8fc` (**71** against `fa4b12d`) — all
under one unchanged digest. So "equal fingerprint ⇒ field-by-field comparable" is false and this tree
carries the counterexample; only the converse ("an *unequal* fingerprint means the runs ask different
questions") is sound.

**WITHDRAWN FIGURES, annotated in place (§15.44's withdrawn-figures register; notes §3.51).** Until
2026-08-12 the paragraph above quoted **32** and **35** for those two pairs, carried over from an
earlier commit message. That pair is withdrawn and must never be re-quoted: notes §3.51 recomputed
every leaf path — scalar paths under `.probes[].observations`, arrays collapsed to one `[]` step —
with an independent `jq` walker cross-checked against the shipped `--field-set`, and **no collapsing
tried reproduces 32/35** (collapsing sibling repetitions to `<probe id>.<leaf name>` gives 38 and 41).
65 and 71 are the reproducible figures for the same two pairs, and the *direction* of the claim is
unaffected: the counterexample is bigger than was stated, not smaller. The sentence the numbers serve
is now also a shipped field rather than only prose — `build.field_set` digests exactly this cell set,
so a reader holding two JSON files can check comparability without recomputing anything (§15.44).

**Both pre-registered experiments answered.** P10: `room_republished_minus_room_freed` **0** and
`refill_reproduced_total` **true** in 6 of 6, against Linux's +2048 in 6 of 6 — Darwin republishes
exactly the room freed. The entire P10 subtree is byte-identical across all three runs where all
three Linux subtrees differ. *The inference is still underdetermined*: the drain is a hardcoded 512
against a capacity of 1024/1022, so D = C/2 and the experiment has one bit of resolution here; a
watermark model with any threshold > 512 predicts the same result, and a reservation charged only at
the empty→nonempty transition is invisible by construction. P9: the zero-timeout gap is the **fd
state**, 7.46–10.12× across fd state at fixed mask against 0.968–1.314× across mask at fixed fd
state, with Linux flat at 258–260 ns. §3.41's "undecomposed" is closed.

**A pre-registered falsifier fired.** §3.40 said *"If `baseline_via_master` comes back `false`,
refutation 2 above is itself wrong."* It reads `false` in 12 of 12 observations. Darwin answers
**Err** to `set_baseline(&master)` at pty creation and **Ok** to the identical call after the hangup,
so at creation Darwin *does* always take the momentary-slave fallback. §3.40 is annotated in place.

**Whole gate: 690 passing, 1 failing, 4 ignored across 109 test binaries**
(`cargo test --workspace --locked --exclude serial-nexus-web --no-fail-fast`), at load 3.2–3.3 with
analysis agents running — so §8 disqualifies that run as a *flake-rate* sample, and the failure below
was re-measured on a quiet box instead. `expectations/macos.jq` passes on all three captures.
<!-- ANNOTATION 2026-08-05 (§5). True of the gate as it stood that day, and no longer true of
     HEAD's — stated rather than corrected away, because the sentence is a record of what was run.
     `macos.jq` has since gained clauses requiring observations these captures' binary did not emit
     (P4's `canonical`, notes §3.48; P10's `peer_pending_input_trust` and P12's witness, §3.50), so
     they now fail it. That is the intended direction: the gate is only ever run against a LIVE
     report — the lane pipes `serial-nexus-doctor --json` into it and nothing runs it over
     `docs/doctor/*.json` — so a capture taken today from today's binary carries the fields and
     passes, and only genuinely old artifacts are rejected. Note the archive was never uniformly
     gate-clean regardless: `macos-24.6.0-2026-07-30-tier3.json` has no P13 block at all and fails
     on that clause independently of any of this. Read the sentence as "these captures satisfied
     the gate of their own vintage", which is what it was asserting. -->

**The `p6_hostility` flake this page records did not recur in this run** — no `Connection refused` /
`os error 61` appears anywhere in the log — and **no rig-gated test self-skipped**, which
`SNX_CROSSOVER=required` would have turned into a hard failure (§3.35). One clean run is not evidence
that flake is gone; it is one sample, recorded so the count below is read against it.

**The one failure is new, is not the `p6_hostility` flake on this page, and does not go away on an
idle box.** `p4_free_for_all::free_for_all_endpoint_lets_concurrent_writers_both_reach_device`,
verbatim: `device received 32754 bytes, expected 32768 (a free-for-all writer was blocked)`, with
`timed_out: true`, and preceded by its own `RIG: this test is running on the crossover rig ... not
the sim null modem`. On a quiet box **12 of 12 reps failed**, losing 5–31 bytes of 32768. Raising
only the sink deadline to 120 s gave one clean pass **in 5.00 s** and one failure that still lost 2
bytes after 122 s — so the 30 s budget was never tight. Say the rest precisely, because §6 forbids
the short form: every failing observation carries `timed_out: true`, and the sim sets that flag so a
deadline is never read as a drop. The measured claim is **"not recovered within 4× the committed
deadline, on a path where a healthy run finishes in 5 s"** — a stall or a loss, not separated here.

This is the first time that test has ever executed on Darwin: at `7ead470` it self-skipped with "no
serial device on this platform", and §3.43's rig fallback is what makes it run. §3.43's safety
property held exactly — a red test that names the provider it used. Its "6 of 7 pass byte-exact" is a
**Linux** figure; on Darwin it is 5 of 6. **Mechanism not established, no root cause claimed:** the
same rig is byte-exact under `serial_hardware.rs`, and the other five `serial_pair_or_rig()` call
sites — `p4_send`, `p4_exclusivity` and three in `p8_map` — all pass on it, so what is left is two
concurrent writers merging onto one free-for-all serial endpoint. (`harness_contract` names the
provider but is not a call site: its test never opens a port.)

**P5 now certifies here, and `supported` → `degraded` is the honest direction** (§3.42 landing). On
the old predicate no certificate item could ever fail on macOS, so a *cleanly wired* rig always read
`supported` whatever the hardware did — discovery still ran, so a half-crossed or hung-up rig would
still have degraded — and zero certificate items were evaluated. It now evaluates five and certifies
`custom_baud`, `break` and `rate_ladder=true` over the physical crossover, naming `icounter` and
`deliberate_mismatch` as the two it cannot. The modem map is reported and never judged; it reads all
four lines false, which is a 3-wire crossover having none to assert. The report still never prints **Tier 3**, because the
`!uncertified.is_empty()` arm returns before the tier-naming arm. *(Fixed 2026-08-05, notes §3.49; the
observation above is left as the true record of that run, and the next capture must print
`Topology: **Tier 3**` in the same line, with the two counter items named as unmeasurable on this
kernel rather than listed bare.)*

**P13 did not invert.** `baseline_packet_bytes` reads **1** in all three shapes, same as Linux — the
feared ~72 (XNU appending the termios struct) did not happen. `waits-then-discards` reproduces at
601084 / 19 / 28 µs against the 600104 / 23 / 29 recorded in `expectations/macos.jq`.

## Update — 2026-08-04 Tier-3 rig pass (macOS 15.7.8 / Darwin 24.6.0, x86_64, real FTDI crossover): P13 answers the last-close question

**The prediction the block below pre-registered came back positive, and it is now an artifact
rather than a source reading.** `docs/doctor/macos-24.6.0-2026-08-05-tier3.json` (binary
`fa4b12d6f529`, probe set `a131e1f4b46d6c83`; named for the UTC day of its own `generated`
stamp, `2026-08-05T00:22:48Z`, taken on the evening of the 4th local — the directory's naming
convention, not a different session). Tier 3 on the cross-wired FT232R pair
`/dev/cu.usbserial-BH00L4KU` ↔ `BH00LL8O`. Verdicts: **15 supported · 7 degraded · 0
unsupported · 3 skipped**.

**P13 on Darwin 24.6.0 reads `waits-then-discards`, `close_waits_for_reader: true`:**

| shape | `close_microseconds` | recovered | meaning |
|---|---|---|---|
| `a_no_reader_blocking_slave` | **600104** | 0 of 64 | the drain-wait runs to its timeout, then destroys |
| `b_reader_drains_before_close` | 23 | **64 of 64** | a drained queue closes immediately — the daemon's case |
| `c_no_reader_nonblocking_slave` | 29 | 0 of 64 | `O_NONBLOCK` takes the destructive branch at once |

Against Linux, which notes §3.30 records as `retains`, 7 µs, 64/64 — **not artifact-backed: no
committed report in `docs/doctor/` contains a P13 block at all**, so treat the Linux column as a
recorded measurement pending a HEAD-vintage Linux capture, not as a citation.
<!-- ANNOTATION 2026-08-05 (§5). DISCHARGED. The HEAD-vintage Linux capture this paragraph waits
     on was taken and committed: `docs/doctor/linux-7.0-2026-08-05-tier3.json` and two sequential
     siblings, binary `71fc5a815852`, probe set `a131e1f4b46d6c83` — the SAME fingerprint as the
     artifact this block is about, so the Linux column is now a citation and the pair is a lawful
     field-by-field diff. The Linux figures read `retains`, `close_waits_for_reader: false`, 64/64
     recovered in all three shapes, with the no-reader close at 20/10/13 µs across the three
     captures. Read "7 µs" above as the scale it was always asserting, not as a reproducible digit:
     the shape's close is a handful of microseconds and moves every run, which is precisely why the
     policy classifier keys on the microsecond-vs-100-millisecond gap and never on the digit. -->

**Why this is a confirmation and not a fit.** The block below derived, from XNU source alone,
that `ptsclose` → `ttylclose` → `ttywflush` → `ttywait` waits up to `t_timeout` = 60 ticks
(≈0.6 s at `hz` 100) before any destructive flush, and that `O_NONBLOCK` skips the wait. Both
halves were written down *before* the probe ran here. `sysctl kern.clockrate` on this box reads
`hz = 100, tick = 10000`, so the source reading predicts **600 000 µs** exactly; six independent
captures measured 600104, 600115, 600249, 600363, 601087 and 601095 µs — the predicted timeout plus
scheduling slop.
<!-- ANNOTATION 2026-08-05 (§5). The list read "five … 600115, 600249, 600363, 601087 and 601095"
     and the figures quoted elsewhere in this block were 601087 / 13 / 28. **None of those is the
     committed artifact's**, which reads 600104 / 23 / 29 — so every sentence citing
     `macos-24.6.0-2026-08-05-tier3.json` by name was quoting a *sibling* capture from the same
     session. The scrollback figures were right about the mechanism and wrong about the provenance,
     which is exactly the failure §16.13 exists to prevent: a committed report is the citable one
     precisely so a reader can check the number, and here the check would have failed. Corrected
     throughout to the artifact's own figures; the sixth capture added to this list is the committed
     one. The reading is unaffected — 600104 µs is the same 60 ticks — and that is the point: the
     defect was never in the physics, only in what could be verified. -->
Shape `c` is the controlled A/B on the single flag `ttylclose` branches on:
same absence of a reader, `O_NONBLOCK` set, 29 µs instead of 600104. The mechanism is measured,
not inferred.

**What this does *not* settle, kept separate on purpose:**

- **It does not explain the 08-03/08-04 `p8_map` red.** No P13 ran on `macos-26-arm64`; this is
  Darwin 24.6.0 / x86_64. And 600104 µs ≈ 60 ticks at `hz` 100 is itself an `hz`-dependent
  quantity that nothing measured on the runner. The §7 limits recorded at the end of the next
  block apply to this artifact exactly as they applied to the P7 one.
- **It does not measure the shape the failing test inhabits.** All three shapes decide the
  reader's fate *before* `close(2)`; none has a reader arriving *during* the close-wait. The
  question "how long after the slave's `close(2)` may the master first read and still recover"
  is still unbuilt, exactly as the last bullet of the next block says.
- **It measures the targetward direction only** — bytes the slave wrote that the master has not
  read (`t_outq`). It says nothing about the hostward/`FREAD` queue, so it must not be cited for
  delta 3's control-packet claim or for `discarded_at_last_close` being structurally 0 here.
- **What it does dissolve** is the arithmetic in the next block's first bullet. That bullet
  reasons from "for a *quiescent* master the flush is unconditional here" and concludes the test
  should have been *near-always red*, leaving 29 green runs as a paradox. The flush is not
  unconditional: it is the timeout arm of a drain-wait, reached in ~0.6 s precisely *because*
  P7's master is quiescent. Against the daemon's reader cadence (`ACTIVE_POLL` 200 µs doubling to
  `IDLE_POLL` 5 ms, `runtime.rs`) a 601 ms window is ~120 idle-poll periods of slack, so 29/29
  green is the *predicted* outcome. Destruction against a quiescent master stands as the
  **outcome**; "unconditional" is refuted as the **mechanism**. The red still wants an
  explanation, and it is now pinned to a ≥601 ms reader stall — a daemon-side event, a different
  and more serious defect class than a lost microsecond race.

**Comparability: this artifact diffs against nothing committed.** Every other report in
`docs/doctor/` carries probe set `01b257ece8c48470` and none contains P13; this one carries
`a131e1f4b46d6c83`. By that directory's own rule an unequal fingerprint means a field-by-field
comparison is reading two different instruments — including against
`macos-24.6.0-2026-07-30-tier3.json` from this same box. Restoring a lawful cross-kernel diff
needs a fresh Linux capture at the HEAD fingerprint; that is **owed**.
<!-- ANNOTATION 2026-08-05 (§5). DISCHARGED, and the paragraph above is left standing because it
     was an accurate statement of the directory at the time. The owed capture landed the same day:
     three sequential Linux Tier-3 runs at `71fc5a815852` carrying `a131e1f4b46d6c83`, so this
     artifact now diffs field for field against a Linux sibling. What that diff shows, at an equal
     fingerprint: P13 `retains` vs `waits-then-discards` (~40000x in `close_microseconds`); P9's
     zero-timeout poll 170 ns vs 23122 ns.
     **P10 is the exception, and it is the reason this annotation is not simply good news.** Its
     Darwin block reads 1024 bytes targetward and 4194304 hostward against Linux's 11776-15360 — and that gap is NOT known to be a kernel difference. `apply_pty_baseline` sets
     the baseline through a slave it immediately closes wherever the master is not a terminal
     (which P2 measures as this platform's case), and Darwin resets slave termios at last close —
     a fact `daemon/src/nodes/pty.rs` already states in its own non-Linux re-assert. P10 then
     opened a fresh slave and never re-asserted, so its Darwin figures are very likely a COOKED
     pty's, a configuration the daemon never runs. Measured on Linux, the mode is worth an order of
     magnitude in recoverability: raw accepts ~13.8 KiB and returns all of it, cooked accepts
     ~23.5 KiB and returns none. P10 now re-asserts on the slave it measures, reports
     `slave_termios_mode`, reports `bytes_recovered_by_peer`, and degrades when the mode is not
     raw. **Do not diff P10 across this pair until a macOS capture at the current binary reports
     `slave_termios_mode: "raw"`.** That capture is the new owed item. -->
<!-- ANNOTATION 2026-08-05 (§5), on the block above. **The owed capture landed and the prohibition
     is lifted.** `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json` report
     `slave_termios_mode: "raw"` on both directions in all three runs, at probe set
     `a131e1f4b46d6c83` and at probe code identical to the Linux captures' binary.
     The diff the block above deferred now reads: Linux 15360 bytes accepted and fully recovered in
     BOTH directions, against Darwin 1024 targetward and 1022 hostward, also fully recovered — a
     factor of 15, with **Linux the deeper kernel**. The pre-repair Darwin hostward figure of
     4194304 was not a depth at all: `ceiling_hit: true` means the fill stopped at P10's own 4 MiB
     backstop and the blocking point was never observed, so it was a floor on a quantity that turns
     out to be 1022.
     TWO CORRECTIONS TO THE BLOCK'S OWN WORDING, both about confidence rather than fact. (1) "very
     likely a COOKED pty's" is still the right hedge and must not be hardened now: the pre-repair
     artifact carries no `slave_termios_mode` field, so it cannot testify to its own configuration.
     What the tree can support is a single-variable source delta plus that report's P2
     `termios_settable_without_slave: false`. (2) The "raw ~13.8 KiB / cooked ~23.5 KiB" pair quoted
     here is a scratchpad measurement backed by no committed artifact, and the raw half does not
     even match the committed Linux figure of 15360 (= 15.0 KiB). See the annotation on notes
     §3.34; `expectations/linux.jq` is the correct form of that claim.
     UNEXPLAINED AND LEFT THAT WAY: Darwin accepts 1024 targetward but 1022 hostward, identical
     across all three runs, where Linux varies. No probe asks why.
     ANNOTATION 2026-08-05 (§5): "where Linux is symmetric" is WITHDRAWN — it is not supported.
     Six runs of the shipped binary gave 13824 and 15360 independently PER DIRECTION, so Linux's
     own within-run direction asymmetry (1536 bytes) is 768x Darwin's (2 bytes), and Darwin's
     figures do not vary run to run at all. The probe's own doc comment on `settled_bytes` already
     recorded that spread; the prose contradicted the code beside it. The contrast worth drawing is
     the opposite one: Darwin is the reproducible side. P10's `recheck` block (notes §3.44) now asks
     why. -->

**Whole-gate hardware validation, same tree (`fa4b12d`), rig attached.** `cargo test --workspace
--locked --exclude serial-nexus-web --no-fail-fast`: **680 passing, 1 failing, 4 ignored** across
109 test binaries, with `fmt`, clippy, `cargo deny` and `expectations/macos.jq` green, and **all
four `serial_hardware.rs` rig tests passing** on the physical crossover
(`crossover_rig_data_plane_send_and_exclusivity`, `crossover_rig_signal_verbs`,
`crossover_rig_custom_baud_byte_exact`, `crossover_rig_map_node_both_directions`) — the macOS
auto-detect arm of `crossover_ports()` fires on exactly two `/dev/cu.usbserial-*` nodes, so these
ran rather than self-skipping. The one failure is **`p6_hostility::a_trickling_peer_trips_the_handshake_deadline_and_the_leg_heals`**,
captured verbatim: `assertion left == right failed … {"error":"Connection refused (os error 61)",
"mode":"wire","pass":false,"tool":"serial-nexus-sim"}`. It is **contention-dependent and
reproduced, not argued**: 0 failures in 20 serial runs, 1 in 40 under 8-way concurrency, same
fingerprint. ECONNREFUSED rather than ENOENT means the socket path existed with nothing yet
accepting, which points at the gap between `load`'s RPC reply and the leg's `UnixListener::bind`
in its spawned task — i.e. §9's "proxy in time", a config-accepted reply standing in for a
listener's readiness. **That is a located suspect, not a root cause**: it has had no independent
adversarial verification (§9) and no fail-first proof, so nothing here is fixed on the strength
of it. Filed for a session that can do both.
<!-- ANNOTATION 2026-08-05 (§5). ROOT-CAUSED AND FIXED IN THE PRODUCT. The suspect above was
     correct and is now settled by measurement rather than by inspection; the competing
     backlog hypothesis this page insisted on keeping alive is REFUTED, and the refutation is
     recorded because §9 makes it as load-bearing as the confirmation.
     THE MEASUREMENTS (Linux 7.0.0-29, 8 cores). Failure rate falls monotonically with the delay
     between `load`'s reply and the first connect: 40.5% at 0 us, 0% at 5 ms, 200 trials per
     point, ONE connection each — the readiness shape, and one a full backlog cannot produce.
     Against a provably listening leg, a connection-count sweep reached 4097 simultaneous
     pending connections before the kernel refused, and refused with EAGAIN, never
     ECONNREFUSED. So backlog saturation cannot be the mechanism here at any concurrency.
     THE CAUSE. `load` replies immediately after `node.start`, and a listen leg's `start` only
     spawn_locals its supervisor — bind(2)/listen(2) run in that task afterwards. `state` could
     not reveal the gap either: `LegShared::new` initialises to the same
     `Waiting{"no peer connected yet"}` a successful bind sets.
     THE FIX is product-side (design §15.42, notes §3.38): the verb holds its reply until every
     listen leg it created has finished its first bind ATTEMPT. `p6_hostility` is deliberately
     unchanged — a harness retry would have hidden the defect from every other consumer of the
     RPC — so its three tests are now the regression coverage.
     WHAT THIS DOES NOT SETTLE, and the reason it is stated rather than assumed: the fix was
     measured on Linux. The Darwin failures on this page are explained by the same mechanism but
     were not re-measured here. PRE-REGISTERED PREDICTION (§7): all three p6_hostility tests, and
     the `wire_hostility_faults_cleanly_then_leg_heals` sibling recorded in 1186c74, pass under
     8-way concurrency on the next Mac run. If any still fails, this root cause is wrong and the
     ECONNREFUSED has a third source neither hypothesis names. -->
<!-- ANNOTATION 2026-08-05 (§5). Still not root-caused, and still not fixed — but the record is
     sharpened by one verifiable observation, and one competing hypothesis is named so the next
     session does not have to rediscover it.
     SHARPER: `handshake_deadline_case` (itest/tests/p6_hostility.rs) goes from `load_toml`'s reply
     straight into `run_wire`, with **no readiness wait of any kind** — not even the
     `leg_sock.exists()` check its siblings use, which is itself only a proxy (the path exists once
     `bind(2)` returns, which is before `listen(2)`). So this test is the *most* exposed of the
     group, not a representative member of it, which fits a failure that needs contention to
     appear. The in-tree remedy already exists and was applied elsewhere: `dial_leg`
     (itest/tests/p6_binding.rs) retries `ConnectionRefused | NotFound` against a 10 s deadline.
     COMPETING HYPOTHESIS, not eliminated: ECONNREFUSED on a unix socket also means a full accept
     backlog, which 8-way concurrency is exactly the condition for. The errno alone does not
     separate "not yet listening" from "listening and saturated", and the existing reasoning reads
     it as though it does.
     WHY NOTHING CHANGED HERE: no reproduction on Linux, so a fix would have no fail-first proof,
     and §9's independent verifier cannot run against a tree that is moving. Patching the test to
     wait would very likely make the symptom go away — which is precisely why it must not be done
     without first establishing which of the two hypotheses it would be papering over. -->

## Update — 2026-08-05 hardware validation at `7ead470`, rig attached

Same box (MacBookPro15,1, Darwin 24.6.0 / macOS 15.7.8, x86_64, 12 cores), same cross-wired FT232R
pair (`BH00L4KU` ↔ `BH00LL8O`), tree clean at `7ead470`.

**The rig is proven on the wire, not merely detected.** With `SNX_CROSSOVER=required` and both
ports named, `serial_hardware.rs` runs 4 passed in 15.46 s: 32768 bytes byte-exact in each
direction at 250000 baud, a custom-baud round trip, a map node byte-exact both ways, and the signal
verbs against the far end. Doctor P3 reports `custom_baud_ok`, `tiocexcl_refuses_second_open`,
`modem_calls_ok` and `break_ok` true on **both** ports; P5 pairs them in both directions.
`tiocgicount_supported` is false on both, which is why P5 certifies neither — see notes §3.36 on
what "Tier 3" can and cannot mean here.

**Whole gate: 684 passing, 0 failing, 4 ignored across 109 test binaries**
(`cargo test --workspace --locked --exclude serial-nexus-web --no-fail-fast`), on the clean run.
That is exactly the `fa4b12d` baseline recorded above **plus the three doctor guards** notes §3.34
added — 681 tests run there, 684 here — and the arithmetic closes with nothing unaccounted for.
`fmt`, workspace clippy, `cargo deny check licenses bans sources`, both meta-gates and
`expectations/macos.jq` are green alongside it.

**Two of the three whole-gate runs this session were not clean, and quoting only the third would
be the §3.35 mistake in a different costume.** Run 1 lost all five rig tests to a leaked daemon
(below); it also overlapped this session's analysis agents, so §8 disqualifies it as a flake-rate
sample. Run 2 read **683 passing, 1 failing, 4 ignored**, the failure being the `p6_hostility`
sibling described next. Run 3 read 684/0/4.

**Two distinct causes, and they must not be averaged into one rate.** The `p6_hostility` failure
appeared in 1 of the 3 runs; the daemon leak in 1 of 3, and in the one run taken under load it did
not control for. Three runs support neither rate to a useful precision, and saying "one failure per
two runs" would fuse a socket-readiness race with a process-lifetime leak that share nothing but a
session. What the three runs do support: the gate reaches green on this box, it did so once out of
three attempts, and both failure modes are recorded below with their evidence. The clean number is
the right headline only because the other two runs are written down beside it; alone it would
assert a stability this box has not demonstrated.

**The one failure is a sibling of the flake already on this page, and that is the finding.** It is
**`p6_hostility::wire_hostility_faults_cleanly_then_leg_heals`**, captured verbatim:
`assertion left == right failed: sim did not report a clean refusal for ["--hello-version","999"]:
{"error":"Connection refused (os error 61)","mode":"wire","pass":false,"tool":"serial-nexus-sim"}`.
Different test, **same binary and same errno** as
`a_trickling_peer_trips_the_handshake_deadline_and_the_leg_heals`. Read against the annotation
above, this is corroboration for the located suspect rather than a second, separate mystery: that
suspect — the window between `load`'s RPC reply and the leg's `UnixListener::bind` in its spawned
task — is a property of the *binary's* setup path, not of one test's assertions, so a second test
in the same binary hitting the same errno is what that hypothesis predicts. **It does not upgrade
the suspect to a root cause.** The competing hypothesis recorded above (ECONNREFUSED on a unix
socket also means a saturated accept backlog) is equally consistent with a second test failing, and
this session ran no fail-first proof and no independent verifier. What changes is only the record's
scope: naming one test made the entry narrower than the evidence.

**A daemon outlived its test process and held the rig hostage — observed once, not reproduced.**
In the first whole-gate run, all five rig-touching tests failed with
`reopen /dev/cu.usbserial-BH00L4KU: Resource busy (os error 16)`. The cause was found still
running: a `serial-nexus-daemon` reparented to `launchd` (**PPID 1**), holding fds on *both* FTDI
ports, alive 4m41s and still alive after the suite exited. Its state file
(`snx-it-<pid>-3/state.toml`: `port0`+`port1`+`inj0`+`inj1`+`rx0`+`rx1`) matches exactly one test
file, `itest/tests/serial_hardware.rs`, and its temp dir was the only `snx-it-*` left on the box —
every other test cleaned up. After killing it, a second whole-gate run on a quieter box produced
**zero** leaked daemons, free ports, and all five rig tests passing; each of the eight
rig-claiming binaries also passes in isolation with no leak. So: mechanism established (an orphaned
daemon holds `TIOCEXCL` and every later opener gets EBUSY), origin narrowed to one file, trigger
**not** established.

<!-- REFUTED DIAGNOSIS, recorded per §9 because a refuted one is as load-bearing as a confirmed one.
     The first hypothesis for the EBUSY cascade was cross-binary contention: eight test binaries
     call `crossover_ports()`/`serial_pair()`, `itest/src/lib.rs` has no cross-process lock, and
     `serial_hardware.rs`'s `static RIG: Mutex<()>` is process-local and therefore cannot serialize
     across binaries. All of that is true and none of it is the explanation. **Cargo runs test
     binaries strictly sequentially** — sampled 12 times during a live whole-gate run, one
     `target/debug/deps/` binary running at every sample, advancing in order. With no two binaries
     ever concurrent, there is no cross-binary race to lose; the process-local mutex is sufficient
     for the concurrency that actually exists. The leak is the mechanism, and contention was a
     plausible story that the measurement killed. -->

**Load discipline, stated because it changes what the runs are worth (§8).** The first whole-gate
run overlapped analysis agents this session had started; §8 forbids reading flake rates under
uncontrolled load, so that run is evidence *for* the leak (which left a corpse to inspect) and
evidence for nothing else. The doctor captures were taken before any of that, on an idle box —
load 2.14 before, 1.48 after, three sequential runs at ~13.75 s each.

## Update — 2026-08-04 CI triage (macos-26-arm64 runner; diagnosed and re-proved on Linux)

The nightly macOS lane went red on 08-03 and 08-04 on one target, `p8_map`'s
`a_read_only_map_leaves_its_writers_pty_alive` (`never` arm): *targetward accounting did not
settle*, with every counter on the map at 0 while `client_present` had gone true→false and the
mapped endpoint's lock had detach-released. So the reader task was alive and the close was
observed — the daemon simply never saw the 64 typed bytes.

**Nothing about the environment moved, which is the part worth recording.** The tree has been
`fa523b1` since 07-30; the runner image was `macos-26-arm64` version `20260707.563` on the red runs
*and* on the green ones before them. Across the 40 most recent CI runs the macOS job's `p8_map` line
reads `... ok` **29 times, spanning 2026-07-26 to 08-02, with no observed failure** before 08-03
(nine further runs are silent here — their macOS job failed at an earlier target and never reached
`p8_map` — so this is 29 observed passes, not a 29-long unbroken chain). A defect that arrives with
no input change is a race whose odds shifted, not a regression, and it should be diagnosed as one.

**A latent harness defect is fixed here. The cause of the red is NOT settled, and this section is
written to keep those two apart.** The guard asserted that bytes typed at a pts *survive the slave's
last close* — which no kernel promises — instead of that they *flowed during the live session*,
which is what the map promises. That is §9's proxy "in time", after delta 4's Linux-only closure
predicate and delta 6's RST-read-as-a-live-peer, and it is worth removing on its own merits. What it
is **not** is a demonstrated explanation of 08-03.

**Correcting this page's own model of the Darwin close, which the first draft of this section leaned
on.** Delta 3 below and its restatements say `ptsclose` → `ttyclose` → `ttyflush(FREAD|FWRITE)`, i.e.
the queues are emptied at the slave's last close. Read against the source
(`apple-oss-distributions/xnu`, `bsd/kern/tty_dev.c` and `bsd/kern/tty.c`, fetched 2026-08-04), that
names the **fallback** path as though it were the only one. The actual sequence is:

```c
/* ptsclose(), tty_dev.c — note the timeout, and that l_close runs BEFORE ttyclose */
save_timeout = tp->t_timeout;  tp->t_timeout = 60;
err = (*linesw[tp->t_line].l_close)(tp, flag);        /* ttylclose */
(void)ttyclose(tp);

/* ttylclose(), tty.c */
if ((flag & FNONBLOCK) || ttywflush(tp)) { ttyflush(tp, FREAD | FWRITE); }

/* ttywflush() → ttywait(): drain-WAIT, not flush */
while ((tp->t_outq.c_cc || ISSET(tp->t_state, TS_BUSY)) &&
       ISSET(tp->t_state, TS_CONNECTED) && tp->t_oproc) {
        (*tp->t_oproc)(tp);                            /* ptsstart: wake the master's readers */
        error = ttysleep(tp, TSA_OCOMPLETE(tp), TTOPRI | PCATCH, "ttywai", tp->t_timeout);
        ...
}
```

For a **blocking** slave fd with a live master — which is exactly what this test opens — the last
close therefore *waits for the reader to drain*, nudging the master awake each round, for up to
`t_timeout` = 60 ticks (≈0.6 s at `hz` 100). The destructive `ttyflush(FREAD|FWRITE)` runs **only**
if that wait errors or times out, or if the fd carried `O_NONBLOCK`. When the drain succeeds,
`ttywflush` flushes `FREAD` alone and returns 0, and the destructive flush is skipped.

**That reading predicts the CI history the "destruction" reading cannot.** The daemon's reader polls
at ≤5 ms, which is 120× inside a 0.6 s window, so the bytes get drained and the old ordering passes
— 29 times out of 29, which is what happened. Under the destruction reading the same 29 runs have a
probability on the order of 10⁻⁸⁴. The corrected model also says what a **red** run means: the
reader did not drain within ~0.6 s, i.e. a **reader stall** — a daemon- or runner-side event, not a
kernel coin flip. So the fix below removes a real latent defect *and may mask that signal*, and the
probe named at the end of this section is how the next macOS run tells us which it was. Nothing here
should be read as "08-03 is explained."

**The fix makes the ordering a compile error, not a kernel race.** The wait now lives in
`settled_while_open(rpc, &client, ..)`, which borrows the client that `drop(client)` moves — so
moving the observation back below the close fails to build (`error[E0382]: borrow of moved value`),
on every platform, deterministically. Fix and rule: `docs/implementation-notes.md` §3.29.

**One instructive detour, recorded so it is not retried.** The first attempt was to emulate the
Darwin close on Linux — precede the close with `tcflush(TCOFLUSH)`, "the same `ttyflush`" — so a
Linux run could referee the ordering dynamically. It reproduced the CI panic verbatim, which is why
it was believed. It is nevertheless **not the same operation**: on Linux a slave-side `TCOFLUSH`
empties the peer's flip buffer and never the master's ldisc `read_buf`, so it races the
flip-to-ldisc push rather than flushing a queue. Measured, 300 trials per delay: 100% destroyed at
0 µs, **0% at 20 µs and every delay beyond**. In the position the shipped test would have used it —
after an RPC round-trip — it destroys nothing at all. It reproduced the failure only because the
reproduction ran it microseconds after the write. An emulation is evidence only once you have
measured that it emulates (§9); this one was withdrawn on that measurement.

Two things this pass did **not** settle, recorded so the next one does not re-derive them:

- **Why the test was ever green here is the open question, and it is sharper than it looks.** The
  intuition to resist is "rare race": the arithmetic points the other way. `docs/doctor/macos-24.6.0-2026-07-30-tier3.json`
  (commit `a1029778fda9`, probe set `01b257ece8c48470`) has P7 `c_open_write_close` at
  `bytes_readable_after_close: 0`, `terminal_read: "eof"` — a written byte destroyed even with a
  settle before the close, so for a *quiescent* master the flush is unconditional here. And the
  daemon's reader is a spin-sleep poller, not a blocking one (`ACTIVE_POLL` 200 µs doubling to
  `IDLE_POLL` 5 ms, `runtime.rs`), so a client's write cannot wake it; against a write-then-close
  window microseconds wide it should lose nearly every time. Both facts predict **near-always red on
  macOS**, and the observed record is 29 passes and 0 failures from 07-26 to 08-02. Those 29 were
  real runs of this test, not skips or fail-fast aborts — the job logs carry
  `a_read_only_map_leaves_its_writers_pty_alive ... ok` on each. So the model is wrong somewhere,
  and **no guess is recorded here**: §7 does not permit one, and the fix deliberately no longer
  depends on the answer.

  Two limits on that P7 citation, both §7-relevant and neither closed here. **It is a different
  kernel and a different architecture from the one that went red**: the artifact is macOS 15.7.8 /
  Darwin 24.6.0 on x86_64, and the CI lane is `macos-26-arm64`. There is no committed report from
  the runner's kernel at all, so every Darwin sentence on this page is evidence about the *rig*,
  applied to CI by assumption. And the *mechanism* named — `ttyflush` at last close — is an
  attribution, not an observation: P7 measures that nothing is readable afterwards, which an XNU
  `ptcread` returning EOF once `TS_ISOPEN` clears would produce identically. macOS P7 reports
  `terminal_read: "eof"` for all three shapes where Linux gives `EIO`, which is consistent with
  either. Prefer "the bytes are not recoverable here" over the mechanism until a probe separates
  them.
- **The probe now exists: `P13`, added in the same change set.** It writes at the slave, closes, and
  reports `policy` (`retains` / `discards` / `waits-then-discards`) alongside `close_microseconds`,
  across three shapes — no reader, reader-drains-first, and a no-reader `O_NONBLOCK` slave, that last
  one because `ttylclose` branches on exactly that flag. On Linux 7.0.0-29 it measures **`retains`,
  7 µs, 64/64 recovered**. If the reading above is right, macOS will report **`waits-then-discards`
  with a `close_microseconds` in the hundreds of thousands**, and that single number settles which of
  the two hypotheses is live — because it is the one measurement `discards` and `waits-then-discards`
  do not share. **Run the doctor on a Mac and commit the artifact (§16.13); until then this remains
  open.** The paragraph below is what P13 was built to replace.
  <!-- ANNOTATION 2026-08-04 (§5: the prediction above is left standing because it was
       pre-registered — it is evidence only if both halves survive). DISCHARGED. The doctor
       ran on the rig and the artifact is committed: `docs/doctor/macos-24.6.0-2026-08-05-tier3.json`
       (binary `fa4b12d6f529`, probe set `a131e1f4b46d6c83`). Measured `waits-then-discards`,
       `close_waits_for_reader: true`, `close_microseconds` 600104 with 0 of 64 recovered —
       "hundreds of thousands", as predicted — against 23 µs / 64-of-64 when the reader drains
       first and 29 µs / 0-of-64 with `O_NONBLOCK`. See the 2026-08-04 Tier-3 rig block at the
       top of this file, including what it does not settle. -->
- **What the answer changed, and what it did not.** The 601 ms figure dissolves the
  near-always-red arithmetic in the first bullet (see the top block): the flush is the timeout arm
  of a drain-wait, not an unconditional destruction, so 29/29 green is predicted rather than
  paradoxical. It leaves the two §7 limits above untouched — different kernel, different
  architecture — and it does not reach the shape the bullet below names.
- **Why the existing set could not answer it.** P7 asks
  what a collapsed session leaves readable against a master nobody drains. The unmeasured question
  is the one this test actually asks: with a master being *actively drained* on the daemon's poll
  cadence, how long after the slave's `close(2)` may the master first read and still recover the
  bytes — a number, parameterized by delay, rather than a yes/no against a quiescent reader. That
  is what would turn this page's "is entitled to destroy them" into evidence. Not built here.

## Update — 2026-07-30 validation pass (macOS 15.7.8 / Darwin 24.6.0, x86_64, real FTDI crossover rig)

A whole-gate pass on the Mac, run to settle the red macOS CI lane on `a102977`. **710 pass,
0 fail, 4 ignored** across 101 test binaries + 8 doc-test targets, with `fmt`, both clippy
gates, `cargo deny`, `expectations/macos.jq`, `p8_web_ui`, `p8_web_history` and all ten
meta-gates green, and **all four `serial_hardware.rs` rig tests passing** on the physical
crossover. The first committed macOS `serial-nexus-doctor` artifact came out of this pass —
`docs/doctor/macos-24.6.0-2026-07-30-tier3.json`, same `probe set` fingerprint as the Linux
reports, so it diffs against them field for field. Until now every macOS claim on this page
cited a scrollback, which §7 does not permit.
<!-- ANNOTATION 2026-08-04 (§5: scoping a sentence that stayed true of its own subject while
     the directory moved under it). "Same fingerprint as the Linux reports" remains correct for
     THIS artifact — it and every report committed before 2026-08-05 carry `01b257ece8c48470`.
     It is no longer true of `docs/doctor/` as a whole: the Tier-3 run at the top of this file
     carries `a131e1f4b46d6c83`, because P13 joined the probe set. Do not read the sentence as
     licensing a diff between the 07-30 artifact and the 08-05 one — they are two instruments. -->


Two defects, **both in the harness, neither in the product**, and both invisible to CI's
matrix for the same structural reason — the macOS lane runs a *subset* of the gates:

- **The CI failure: `p12_web_ws_bounds`' closure oracle, not the caps.** Both over-cap tests
  failed on `ws.closed()`. The caps were measured correct at the boundary on this platform
  (`cap-1` and `cap` served, `cap+1` refused; 246 over-cap trials, 0 daemon mutations, no fd
  leak, the session genuinely dead afterwards). What was wrong is delta 6 below. Fixed, and
  the fix re-proved fail-first with both `WebSocketConfig` calls deleted.
- **`cargo clippy --workspace --all-targets -- -D warnings` did not pass on macOS at all.**
  `p12_pty_setup.rs`'s helper `two_consoles_that_both_come_up` is reachable only from a
  `#[cfg(target_os = "linux")]` test, so off Linux it is dead code and `-D warnings` makes
  that fatal. Its sibling `fd_flags` was gated; this one was missed. **CI cannot catch this
  class**: clippy runs in the `check` job, which is `ubuntu-latest` only. A Mac is the only
  place this gate has ever been run.

Two process gaps closed in the same pass. The macOS lane still ran plain `cargo test
--workspace` — no `--no-fail-fast` — even though the 2026-07-28 block below is the record of
what that costs; it has it now. And `docs/implementation-notes.md` cited two rules as living
in `AGENTS.md` that were not in it (the section numbers had shifted under the rename track);
both rules are now actually written there, in §6 and §9.

### 6. TCP close semantics: an RST is an ending, and `read() == Ok(0)` does not see one

**New with this pass, and the cause of the CI failure.** When `serial-nexus-web` refuses an
over-cap frame, tungstenite raises the capacity error while parsing the frame **header**, so
the payload is never drained; the server's `close(2)` therefore runs with ~132 KiB still
queued, and every mainstream stack answers that with an **RST** rather than a FIN (RFC 1122
§4.2.2.13). A test that asserts closure as `matches!(read, Ok(0))` scores that reset the same
as a live-but-silent peer — backwards, since an RST is the *stronger* ending.

Linux passed anyway, and the reason is a Darwin detail worth recording on its own:
**`setsockopt(SO_RCVTIMEO)` returns `EINVAL` on a socket carrying a pending RST.** The
harness re-arms the read deadline before every read, so on Darwin that call fails, the helper
returns *without reading*, and the pending `ECONNRESET` is left for the next bare probe —
which reports it as an error. On Linux the same call succeeds, the read inside the helper
consumes the one-shot socket error, and the probe that follows sees a plain `Ok(0)`. Measured
here at the syscall level: `setsockopt` → `EINVAL(22)`, then `read` → `ECONNRESET(54)`, then
`read` → `0`. Same server, same bytes, opposite verdicts — and it is a coin flip on both
platforms, which is why one of the two tests passed locally while both failed in CI.

**Lesson, the delta-4 one again in a second dress: derive a closure predicate from what the
server promises — the session is over — not from which of the two shutdown paths a kernel
took.** The oracle now accepts a Close frame (latched where it is drained, because the
helper that waits for a reply consumes it first), EOF, or a terminal `ErrorKind`, and treats
only a deadline as "still open". That is *stricter* on Linux than what it replaced: a Close
frame is positive evidence the server ended the session deliberately, where an EOF alone is
merely consistent with it. **verified.**

## Update — 2026-07-28 full-suite pass (macOS 15.7.8 / Darwin 24.6.0, x86_64, real FTDI crossover rig)

The first pass to run the **whole** suite on a Mac. That had never happened: the macOS
CI lane runs `cargo test --workspace`, which **fail-fasts at the first crate**, and one
`serial-nexus-daemon` unit test had been failing there since the probe set grew — so every
macOS job since had stopped before the integration harness and the lane had been red on
six consecutive pushes for one reason while three further failures sat behind it,
unseen. `--no-fail-fast` surfaced all four at once. **623 pass, 0 fail** now.

- **All four hardware-rig tests pass** (`serial_hardware.rs`): byte-exact both directions
  at 115200 and 250000, the `send` verb on real silicon, `TIOCEXCL`, the signal verbs, and
  the v11 `map` node both directions over the physical crossover. **verified.**
- **Three of the four failures were guards asserting a Linux-specific proxy**, and the
  daemon was measured correct on macOS in each case. They are now written against the
  portable property (see deltas 3 and 4 below for the two mechanisms).
- **The fourth is a real, operator-visible macOS defect, and it is *not* fixed** — the
  collapsed termios-only pty session leaks its write lock (delta 3). Its guard skips here
  rather than being retired, and the skip names the gap.
- **P1 is answered on hardware** — this page said "needs a Mac" for four generations. It
  is `degraded`, and the *mechanism* is now measured, not inferred (delta 3).

## Update — 2026-07-24 hands-on pass (macOS 15.7.8 / Darwin 24.6.0, x86_64, real FTDI crossover rig)

The first hands-on pass on real hardware ran, and settled the open questions; a
follow-up pass on the same rig extended the coverage (250000-baud data, the serial
signal verbs through the daemon, and a physical unplug→replug heal) and closed the
license-gate skip. Two things below that this page previously marked *"expected"* /
*"needs a Mac"* turned out to be **real defects, now fixed**; the rest is confirmed.
See `docs/implementation-notes.md` (2026-07-24 session) for the mechanism.

- **Build / test / lint:** all clean; **248 tests pass, 0 fail** (`cargo test --workspace`).
- **Serial data plane over the real crossover cable:** byte-exact both directions at
  **115200 and 250000** (32 KiB each way, SHA-256), the `send` verb reaches hardware,
  **a second open of a held port is refused**, driver counters gracefully absent.
  *TIOCEXCL nuance:* the daemon sets `TIOCEXCL` unconditionally and the second open is
  genuinely refused, but macOS `cu.*` call-out devices are single-open at the driver
  layer regardless — so on macOS the refusal is real yet **not attributable to the ioctl
  alone** (`serial-nexus-doctor` P3 confirms the ioctl is *accepted*, not that it is what refuses
  the open). The clean isolation — a second open that succeeds without `TIOCEXCL` and
  fails with it — is a Linux-rig property. **verified.**
- **Serial signal verbs over real hardware:** `send-break` / `set-modem` / `pulse-dtr`
  driven end-to-end through the daemon against a real UART all succeed — the Tier-3
  property (§13) this null modem exists to test, unreachable on a pts (which `ENOTTY`s
  set-modem/pulse-dtr, so `p7_signals` cannot cover it). A break is observed at the far
  end as a NUL (best-effort; deterministic frame-error detection needs Linux-only
  `TIOCGICOUNT`). **verified.**
- **Unplug → replug heal on real hardware:** pulling a USB adapter parks its serial node
  in **`waiting` (device lost)** — the graph is unchanged, only the *state* moves
  (config-vs-state split) — and replugging re-resolves the same stable `cu.usbserial-*`
  path, reopens (termios + modem lines reapplied), returns to `active`, and carries data
  again (a `send` nonce crossed the healed wire). The §7/§12 "survive replug" property on
  real macOS hardware (the Linux `p7_replug`/`p7_unplug` self-skip there). **verified.**
- **License gate proven, not just configured:** with `cargo-deny` installed,
  `cargo deny check` is clean and the folded `p0_license_gate` test (plants a banned crate,
  asserts cargo-deny rejects it) passes. **verified.**
- **PTY nodes — FIXED.** They previously *faulted* on macOS (`tcgetattr: ENOTTY`): the
  §7.2 baseline termios was applied through the pty **master**, which BSD rejects. Now
  cfg-gated to apply through the **slave** on non-Linux
  (`daemon/src/nodes/pty.rs::with_termios_fd`),
  re-asserted on the client's presence rising edge (the macOS slave termios resets on
  last-close). Presence tracking works; the full client→pty→serial→crossover path is
  **verified byte-exact.** Linux path unchanged.
- **Doctor P1 → `degraded`** (EXTPROC/packet-mode notifications don't surface; §7.2 runs
  poll-only — benign, as designed). **Doctor P2 → `degraded`** (was `unsupported`): POLLHUP
  presence works via priming + slave-termios, so the probe now says `degraded`, not
  `unsupported`. **`expectations/macos.jq` now PASSES** (0 unsupported).
- **`serial-nexus-sim` PTY doubles — FIXED** for the same master-termios reason (BSD leaves termios
  to the consumer).

### macOS test-infrastructure limitation: a pty is not a usable serial device

`serial2::SerialPort::open` on a macOS **pts** returns `ENOTTY` (it sets baud via a
macOS-specific ioctl a pty rejects). So the Linux "no-target doctrine" — a pty standing in
for a serial device — **does not work on macOS**. Serial-*device* tests on macOS use a
**real crossover rig** or **skip**; the product's real-UART path is unaffected (proven
byte-exact above). The Rust harness (`serial-nexus-itest`) encodes exactly this in three
providers (`itest/src/lib.rs`): `serial_echo()` (one `serial-nexus-sim pty --echo`
double) and `serial_pair()` (a `serial-nexus-sim nullmodem`) are **Linux-only** and return
`None` elsewhere, so every test built on them self-skips on macOS; `crossover_ports()`
finds the real rig and drives the `serial_hardware` test, itself skipping when no rig
is attached.

**`crossover_ports()` auto-detects on macOS only, and that asymmetry bites on Linux.**
It has two arms: `SNX_CROSSOVER_A`/`_B` if both are set, on every platform, and — under
`#[cfg(target_os = "macos")]` alone — a scan that accepts exactly two
`/dev/cu.usbserial-*` nodes. There is **no by-id arm**, so on Linux a physically
cross-wired pair is invisible until those two variables are exported, and every
rig-gated test (`serial_hardware.rs`, and
`p12_serial_exclusivity::a_break_straddled_by_a_replace_leaves_the_line_transmitting`,
the guard for the break clause) self-skips on a box whose rig is attached and working.
A green run is then hardware coverage that never executed — the failure mode a
self-skip is otherwise safe against. Export both variables on a Linux rig; do not read
`serial-nexus-doctor` P5 reporting Tier 3 as evidence that these tests ran.

The validation harness was fully migrated from the bash `scripts/validate/**` (which used
`stat -c`, `nc -q`, `sha256sum`, `timeout`, `/dev/serial/by-id` — none macOS-portable) to
the cross-platform **`serial-nexus-itest`** crate; `scripts/` is gone entirely (v10 §16.11).
macOS-verified: control-plane + the hardware crossover byte-exact test.

The feature matrix below is the original Phase-8 *predicted* table, kept for reference; where
this update block and the table disagree, **this block is the observed truth.**

**What Phase 8 actually delivered:** the whole workspace now *compiles* for
`*-apple-darwin` and *degrades gracefully* at every Linux-specific edge. That is
verified by a clean cross-compile —

```
cargo check --target x86_64-apple-darwin --workspace --exclude serial-nexus-web   # Finished, no errors
```

`serial-nexus-web` is excluded **from the Linux-side cross-check only**: its TLS
dependency `ring` builds C with `cc` and cannot cross-build to Darwin from Linux
(`cc: error: unrecognized command-line option '-arch'`). The real macOS gate is
`cargo test --workspace` *on a Mac*, where `ring` builds natively and no exclusion
is needed.

— and by reading the platform gates in the source (below). Beyond that compile-time
proof, the mechanisms that depend on live macOS behavior **have now been
runtime-verified on a real Mac** — see the hands-on update block at the top of this
page, which is the authoritative record. The feature matrix below is kept as the
original Phase-8 *prediction*; where it still reads *expected* / *needs a Mac* for an
item the update block reports **verified**, the update block wins.

## How to read the verdicts

| Marker | Meaning |
|---|---|
| **cross-checked** | Verified against the source and a clean `cargo check --target x86_64-apple-darwin`. Compile-time and code-path facts. |
| **expected** | Follows deterministically from the code and design, but has not been exercised on a Mac. Should hold; unproven. |
| **unverified** | Depends on real macOS kernel/device behavior we cannot predict from Linux. **Needs a Mac** — this is what the CI lane and a hands-on pass exist to settle. |

## Feature matrix

| Feature | Linux | macOS | Notes |
|---|---|---|---|
| Workspace build | ✅ | ✅ | Compiles for `*-apple-darwin`. **cross-checked.** |
| Data plane (PTY pair, `read`/`write`/`poll(2)`) | ✅ | ✅ | Plain POSIX; no Linux-only syscalls on the hot path. **verified** (byte-exact over the real rig at 115200 & 250000). |
| PTY client-termios observation (EXTPROC + `TIOCPKT`) | ✅ | ❓ | Packet-mode signaling is **unverified** on macOS (§7.2). **needs a Mac.** |
| Reconciliation poll (termios backstop) | ✅ | ✅ | Unconditional; becomes the *sole* observation path if EXTPROC misbehaves. **cross-checked** (code) / macOS timing **expected.** |
| Driver error counters (`TIOCGICOUNT`: overrun/framing/parity) | ✅ | ➖ omitted | `TIOCGICOUNT` is Linux-only; the binding is gated, the reader stubs to `ENOTSUP`, counters are simply absent — exactly as a pts behaves on Linux. **cross-checked.** |
| Modem-line read/set, break (`TIOCMGET`, DTR/RTS) | ✅ | ✅ | Not gated; serial2 + the shared `sys` ioctls are cross-platform. **verified** (`send-break`/`set-modem`/`pulse-dtr` driven through the daemon on the real rig; break seen far-end as a NUL). |
| Advertised PTY baud ≥ 460800 (`B460800`/`B921600`) | ✅ | ➖ capped | macOS termios tops out at `B230400`; the high arms are gated out. Advertised baud is cosmetic on a PTY, so it falls through to "unset." **cross-checked.** |
| Identity: `usb:` / `by-path:` resolve to a live path | ✅ | ✖ inert | No `/dev/serial/by-id`, no `by-path` tree, no `/sys`. A node configured this way resolves to nothing and stays **`waiting`** forever. **expected.** |
| Identity: add by present raw path | ✅ | ✅ | Captures a `raw:<path>` identity with the standard instability warning. **verified** (a physical unplug→replug faults to `waiting` then heals to `active` at the same `cu.*` path). |
| Identity: add by bare serial number | ✅ | ✖ unsupported | Needs the deferred IOKit resolver backend (§14). **cross-checked** (falls through to an empty adapter scan). |
| Device-node convention | `/dev/ttyUSB*`, `/dev/ttyACM*` | `/dev/cu.*` | Use the **call-out** (`cu.*`) nodes, **not** `tty.*` (those block on carrier detect). **expected** (macOS convention). |
| Root control socket | `/run/serial-nexus-daemon.sock` | `/run/serial-nexus-daemon.sock` | `/run` exists on macOS (symlink to `/var/run`). **expected.** |
| Non-root control socket | `$XDG_RUNTIME_DIR/…` | `/tmp/serial-nexus-daemon-<uid>.sock` | `XDG_RUNTIME_DIR` is conventionally unset on macOS, so the fallback applies. **cross-checked** (code) / convention **expected.** Since notes §3.72 the doctor **computes and prints** this path in its environment block instead of describing the policy — the description was wrong here specifically, naming `/run` (the *root* arm) for an unprivileged Mac. |
| Stale PTY symlink auto-recovery after a crash | ✅ | ✖ faults | Recovery is keyed on `/dev/pts` (Linux devpts); macOS pts nodes are `/dev/ttys###`, so a stale symlink is **not** reclaimed — the node faults instead. Minor degradation. **cross-checked.** |
| Doctor P1 (EXTPROC/`TIOCPKT`) | ✅ | ❓ | Reports the real delta on a given Mac; a `degraded` verdict means §7.2 runs poll-only. **needs a Mac.** |
| Doctor P2 (PTY presence / `POLLHUP`) | ✅ | ❓ | Presence detection is POSIX but the exact `POLLHUP` timing is **unverified.** **needs a Mac.** |
| Doctor P3 (serial-port fit / UART cert) | ✅ | ✅ | Custom baud, `TIOCEXCL`, modem lines, break all pass on a named `--port`; only the `TIOCGICOUNT` sub-clause is absent. **verified** (P3 `supported` on both rig ports). |
| Doctor P4 (by-id resolution) | ✅ | ⚠️ degraded | **Corrected 2026-08-06 (notes §3.72).** This row said `skipped ("no adapter")` and that has not been reachable on a Mac for some time: the `skipped` arm now needs *both* device lists empty, and the `cu.*` scan makes that impossible on any Mac with a serial node — the M4 report reads `degraded` with `count: 0`, `unidentified: 4`, every device classified `raw:<path>`. Left uncorrected it sends a maintainer triaging a macOS report against this table chasing a regression that never happened. No by-id tree and no `<sys>/class/tty`, so no `usb:`/`by-path:` identity resolves at all (§12/§13; the IOKit backend is deferred, §14) — a node configured with one waits forever, and since §3.72 it says so by name rather than reporting the device "not present". **verified** (M4, Darwin 27.0.0 arm64). |
| Doctor P5 (rig certification) | ✅ | ➖ partial | Discovery **pairs both ports** (crossover proven bidirectionally) **and the certificate now runs**: `p5_is_uart` is the portable disjunction `TIOCMGET \|\| TIOCGICOUNT` since §15.47, so a real adapter certifies here — `custom_baud=true break=true` per port and `rate_ladder=true` over the wire in `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json` (`1a9a8fca1c36`). Exactly two items stay unmeasurable, `icounter` and `deliberate_mismatch`, both reading `TIOCGICOUNT`; P5 is `degraded` naming them, with the mechanism carried as data rather than as a bare `false`. **verified** (discovery + five certificate items) / two items platform-excused. |
| Doctor env: `dialout`/`plugdev` membership | ✅ | ➖ skipped | `getgroups` is unavailable in nix on Apple, so supplementary membership is reported **unknown/skipped**. macOS serial access is governed by device-node ownership, not these groups. **cross-checked.** |
| Doctor env: device-node access check | ✅ | ✅ (expected) | `access(2)` on the node path is cross-platform. **expected.** |

`➖` = a design fallback engages (a feature is omitted or skipped, by design, with
no fault). `✖` = the feature does not function on macOS today.

## The concrete deltas

### 1. Build: what is gated, and why it is safe

The tree compiles for `*-apple-darwin` because four Linux-only touch-points are
gated behind `cfg`, each onto a fallback the design already had — and each lives
either in **`sys/src/lib.rs`**, the workspace's one crate with `unsafe`
(§16.3), or in the single node that owns the behavior, so no platform arm is
scattered:

- **`TIOCGICOUNT`** (driver overrun/framing/parity counters). libc exports the
  request code only under `target_os = "linux"/"android"`, so the ioctl binding —
  and only the binding — is Linux-gated. Off Linux, `serial_nexus_sys::read_icounts`
  returns `ENOTSUP`, which callers already map to "driver counters unsupported →
  omit them." That is the *same* graceful path a pts takes on Linux (a pts has no
  such counters either), so the code path is well-worn, not new. See
  `sys/src/lib.rs`; the doctor reaches it as `use serial_nexus_sys as sys`.
- **`ptsname_r(3)`** (the reentrant slave-name resolver, a glibc extension). It
  does not exist on macOS, so `serial_nexus_sys::ptsname` uses the static-buffer
  `ptsname(3)` there, copying the `String` out before returning — under a process
  mutex, since that buffer is process-wide and two concurrent callers would
  otherwise hand each other another pty's path. One wrapper hides the split, and
  since §16.3 it is the *only* one: the daemon, the doctor and the sim all call
  `serial_nexus_sys::ptsname`, so none of them carries `unsafe` of its own.
- **High-baud `BaudRate` arms** (`B460800`, `B921600`). macOS termios caps
  standard speeds at `B230400`, and nix gates those arms out on Apple. The PTY's
  advertised baud is cosmetic anyway, so an out-of-range value simply falls
  through to "unset" rather than being approximated
  (`daemon/src/nodes/pty.rs::standard_baud`).
- **`getgroups`** is unavailable in nix on Apple, so the doctor's `dialout`/
  `plugdev` membership check reports *unknown/skipped* rather than a false verdict
  (`doctor/src/probes.rs::is_group_member`).

None of these change the design; each is the platform arm of a fallback §13
already promised. **cross-checked.**

### 2. Device identity and the `cu.*` convention

macOS has **no `/dev/serial/by-id` tree, no `by-path` tree, and no `/sys`.** The
resolver's Linux backend reads exactly those, so on macOS it enumerates nothing.
The consequences are concrete:

- **`usb:` and `by-path:` identities are inert at runtime.** A node configured with
  one resolves to no path and stays permanently **`waiting`** (the faulted-and-wait
  posture of §15.25). This is not an error — it is the honest state for "I cannot
  find this device" — but it means the squatter-safe identity forms do not
  function until an IOKit backend lands.
- **Operators use raw call-out paths: `/dev/cu.*`.** Use the **`cu.*`** (call-out)
  nodes, **never** `/dev/tty.*` — the `tty.*` nodes block on carrier-detect and
  will hang an open. A *present* `cu.*` device added by path captures a `raw:`
  identity and carries the standard instability warning (the escape hatch of §12):
  a `raw:` path is "whatever is at this path now," with no squatter protection.
- **Bare serial-number adds are unsupported** until the deferred IOKit resolver
  backend (§14). On macOS the bare-serial branch scans an empty adapter list and
  finds nothing. That backend slots *behind* the existing `Resolver` API with no
  design change (§12) — the `usb:`/`by-path:`/`raw:` fallback chain and the
  identity-vs-path split are already in place; only the discovery source changes.

Adding by raw path still requires the device present at that moment (identity must
be captured); adding or loading by identity never does — but on macOS the only
capturable identity today is `raw:`. **expected** (the code paths are
cross-checked; the live `cu.*` capture **needs a Mac**).

### 3. PTY observation runs the poll-only path

The design observes client termios two ways: promptly, via EXTPROC + packet-mode
(`TIOCPKT`) control packets; and, as an unconditional backstop, via a slow
reconciliation poll (one ioctl every few seconds, effectively free). **EXTPROC/
packet-mode observation is unverified on macOS** (§7.2 says so explicitly). If it
misbehaves there, the reconciliation poll becomes the *sole* mechanism — i.e.
macOS runs the **poll-only observation path**, and the only thing that degrades is
client-termios *latency*; nothing in the data plane depends on the fast path. The
daemon never consults a probe to decide this — the poll is always running.

`serial-nexus-doctor` P1 reports the *actual* delta on a given Mac: `supported` means the
fast path works; `degraded` means poll-only. **Measured 2026-07-28 on 15.7.8:
`degraded`** — and the mechanism, which the "unverified" wording left open, is now
pinned. A packet **is** produced by a client `tcsetattr` while the slave is open, but
Darwin's leading byte is `0x20` (`TIOCPKT_DOSTOP`), not `0x40` (`TIOCPKT_IOCTL`), so
`read_and_poll`'s `buf[0] & sys::TIOCPKT_IOCTL != 0` arm never matches and termios
reconciliation runs entirely off the `RECONCILE_INTERVAL` (3 s) backstop. That is
exactly the fallback this delta describes, working as designed; only client-termios
*latency* degrades. **verified.**

**The one macOS defect on this path — found here, and fixed here.** A pty client that
opens, calls `tcsetattr` and closes **inside one 5 ms reader poll gap** — a scripted
probe, a health check, a bare `stty` — used to leave its `usb0` write lock held forever.
XNU's `ptsclose` → `ttyclose` flushes both tty queues at the slave's last close, so
the packet above is destroyed before the daemon's next poll, and `read_and_poll`'s
`saw_session` latch (§6 detach-release, invariant 16) has nothing to arm on: `was` is
<!-- ANNOTATION 2026-08-04 (superseding sentence, not a rewrite — §5): the sentence
     above names the FALLBACK path as if it were the only one. `ptsclose` runs
     `l_close`/`ttylclose` first, which calls `ttywflush` → `ttywait` and *waits* up to
     `t_timeout` (60 ticks) for the master to drain, flushing `FREAD|FWRITE` only if
     that wait fails or the fd is `O_NONBLOCK`. See the 2026-08-04 block at the top of
     this file for the source excerpt. The delta's own *conclusion* is unaffected: the
     session shape it describes carries a control packet in the FREAD queue, which
     `ttywflush` flushes even on the success path, so the packet is destroyed either
     way and `SessionLatch` is still the mechanism that carries detach-release here.
     What the sentence must not be reused for is the TARGETWARD direction — data the
     slave wrote and the master has not read — which the drain-wait normally
     preserves. -->
false because no poll landed during the 53 µs session, and every level-triggered
observable — poll revents, `FIONREAD`, `TIOCOUTQ`, `TIOCGPGRP`, `TIOCMGET`,
`TIOCGWINSZ`, the pts inode's timestamps — is byte-identical to no session at all.
Measured against the shipped daemon: **20 of 20** real `stty -f` invocations leak,
`usb0.lock.holder = "console"` while `console.client_present` reads `false`, past
30 s, with another origin's `send` failing `-32003 … is locked`. It heals on the next
*observed* (≳5 ms) session, or `lock --steal`.

**The fix, and why it is not a widened predicate.** Level state cannot carry an edge,
so no amount of looking harder at the observables above could answer it —
`serial_nexus_sys::SessionLatch` (design **§15.39**) does, via a kqueue
`EVFILT_READ | EV_CLEAR` knote on the master, inert off Darwin. `p9_pty_collapse`'s
third test now runs **unskipped on both platforms**, and it was proved fail-first here:
0/8 sessions release with the latch neutered, 8/8 with it. Four things bind a future
editor. (1) **Do not widen the predicate instead** — an ungated `|| closed` arm would
fire on every 5 ms pass (a Darwin master with no slave reports `POLLIN|POLLHUP` and
`read → 0` forever, doctor P6 64/64), releasing a lock an operator took with no client
attached. (2) **The latch never marks the pass productive**, so the idle backoff is
untouched: measured 1.62% → 1.75% of a core idle, against the 74%-of-a-core spin this
area's other rules exist to prevent. (3) **The daemon forges these edges itself** — the
baseline re-assert, the last-close flush, the reconciliation backstop — so `watch`
swallows its own registration edge and the close block discards after running; removing
either makes the handler re-fire on its own footsteps, which
`collapsed_client_sessions_still_release_the_write_lock` catches. (4) **Invariant 1 is
intact**: its ban is on `AsyncFd`/epoll as a *readiness* source, and readiness is still
`poll(2)` alone. `serial-nexus-doctor` **P7** measures the packet mechanism and the new **P12**
the edge one, so a report always says which is carrying detach-release here.
**verified as a defect, and fixed.**

One asymmetry is recorded rather than levelled: the *bare* open→close that Linux leaves
deliberately uncovered (nothing readable, nothing to latch on, harmless) **does** post
an edge on Darwin, so macOS is here the stricter platform.

A related consequence with no fix needed: **`discarded_at_last_close` is structurally
always 0 on macOS.** §7.2's hostward flush counts what the *daemon* discards, and this
kernel destroys the pts's undelivered hostward queue at last close first — so the
guarantee ("a fresh session never inherits the previous operator's scrollback") holds
here for free, and the counter that names the discard has nothing to name. **verified.**

### 4. Sockets and paths

- **Root:** `/run` exists on macOS (a symlink to `/var/run`), so the default root
  socket `/run/serial-nexus-daemon.sock` works unchanged.
- **Non-root:** `XDG_RUNTIME_DIR` is conventionally unset on macOS, so the daemon's
  socket resolver falls through to **`/tmp/serial-nexus-daemon-<uid>.sock`** (see
  `daemon/src/lib.rs::resolve_socket`). This is short enough for the
  `sockaddr_un` length limit. Pass `--socket` to override.
- **The AF_UNIX socket buffer is 26× smaller, and it is a trap for test authors.**
  `net.local.stream.sendspace` and `recvspace` are **8192** bytes here against Linux's
  ~208 KiB (`net.core.wmem_default`); measured, a writer whose peer never reads places
  exactly 8192 bytes before `EWOULDBLOCK`. Nothing in the product depends on the size —
  a full buffer is backpressure, which §5 sanctions targetward (delay, never drop) — but
  it is **narrower than one 64 KiB wire frame**, and a `leg` credits
  `accepted_targetward` only once a whole frame has cleared `write_all`. So against a
  stalled peer a large-chunk test sees that counter legitimately pinned at **0** on
  macOS where Linux shows it climb and plateau. Two LEG-2 guards were written against
  the Linux number and hung for 30 s each here; the fix was to state the real predicate
  — *frozen while bytes are still owed* — rather than "frozen at a nonzero value", which
  is also strictly stricter on Linux (the old form accepted a plateau at `== sent`, a
  fully drained peer, which is not the parked state at all). `p6_fragmentation` already
  used the portable form. **If you assert on a socket-buffer-derived quantity, derive
  the predicate from what the daemon promises, not from what Linux happens to buffer.**
  **verified.**
- **Stale PTY symlink after a crash:** the auto-recovery that silently reclaims a
  symlink dangling into devpts is keyed on the target starting with `/dev/pts`
  (`daemon/src/nodes/pty.rs::PtyNode::install_symlink`). On macOS, pts nodes are
  `/dev/ttys###`, so that predicate is false: a stale PTY symlink left by a crash
  is **not** reclaimed, and the node **faults** on the pre-existing path instead of
  recovering. A minor degradation — the operator removes the stale symlink by hand
  and restarts the node. **cross-checked.**

### 5. Doctor behavior on macOS

- **P3 does not degrade on `TIOCGICOUNT` at all, and P5 degrades on two certificate
  *items* rather than on its predicate.** P3's verdict turns on `custom_baud_ok &&
  exclusivity_ok` and on nothing else (`doctor/src/probes.rs::p3_serial`); counter
  availability is reported as the observation `tiocgicount_supported` and moves no
  status. P5's is-this-a-UART predicate has been the disjunction `TIOCMGET ||
  TIOCGICOUNT` since §15.47 (`p5_is_uart`) — a widening, never a replacement, because
  a widening cannot lose a port — so a real FTDI answers it here and characterization
  runs. What stays unmeasurable on this kernel is exactly two certificate items, both
  of which read `TIOCGICOUNT`: `icounter` per port, and the pair item
  `deliberate_mismatch`, whose second half is the receiver's frame-error counter, so
  `after > before` reduces to `0 > 0` however the wire behaved — the bulk pattern is
  still transmitted. **Nothing else is excused**, and that list is load-bearing:
  `custom_baud` and `break`, the `reopen` / `pair_reopen` / `mismatch_reopen` rig
  states, and the integrity item `rate_ladder` are measured on every kernel — the
  excuse is carried as data on the two counter-reading sites and can never widen to
  them. P3's own checks (custom baud, `TIOCEXCL`, modem lines, break) likewise all run
  against a named `--port` here. Measured, in
  `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3.json` (binary `1a9a8fca1c36`,
  probe set `a131e1f4b46d6c83`, 2026-08-05): P3 `supported` on both rig ports with
  `tiocgicount_supported: false`, and P5 `degraded` naming three uncertified items —
  `icounter` on each port and `deliberate_mismatch` on the pair — beside
  `custom_baud=true break=true` on both ports and `rate_ladder=true` over the physical
  wire — a capture whose rig was proven on the wire the same session (top of this file).
  *(Superseded wording, kept so it is not re-derived: this bullet read "**P3 / P5
  (UART certification)** degrade where `TIOCGICOUNT` is absent" until 2026-08-12. It
  was true of the pre-widening predicate and is false of the shipped one twice over —
  P3's verdict never read the counters, and P5's predicate no longer does. notes §3.49
  filed it as the third of three sites owed this correction; the other two, the
  uncharacterized arm in `doctor/src/probes.rs` and the `supported` bullet in
  `docs/serial-nexus-doctor.md`, were discharged earlier and quote the old sentence as
  history.)*
- **Environment group checks are Linux-centric.** `dialout`/`plugdev` do not govern
  serial access on macOS — device-node **ownership** (often `wheel`, or the owning
  user) does. With `getgroups` unavailable on Apple, the doctor reports these as
  *unknown/skipped* rather than guessing. The `access:<node>` read+write check is
  the meaningful permission signal on macOS.
- The `kernel` and `os` environment fields read Linux files
  (`/proc/sys/kernel/osrelease`, `/etc/os-release`); on macOS they render empty /
  `unknown`. Cosmetic — the report is still valid and copy-pasteable. Still true at
  `fa4b12d`, which is why the committed artifacts record the OS version in
  `docs/doctor/README.md`'s index row instead.
  <!-- ANNOTATION 2026-08-05 (§5). FIXED at `71fc5a815852`. Both fields now come from
       `uname(2)`, which is POSIX and answers on Darwin: `kernel` reads the release
       (`24.6.0`) and `os` falls back to `Darwin 24.6.0 (x86_64)` where no
       `/etc/os-release` publishes a `PRETTY_NAME`. `nodename` is deliberately not
       read — it is the machine's hostname and nothing here needs it. "Cosmetic"
       understated it: §16.13 says provenance is *recorded, never asserted*, and a
       report that could not name its own kernel forced the one fact every
       cross-kernel claim rests on to be typed in by hand beside the file. The
       fallback arm is injectable (`distro_from`) so Darwin's path is exercised by
       the Linux suite on every push rather than trusted until someone next opens a
       Mac — a guard asserting only "the kernel field is non-empty" would have passed
       on Linux throughout the four generations this was broken. The two committed
       macOS artifacts still read empty: they are frozen records of what the tool
       printed on their date (§16.13) and are not rewritten. The next macOS capture
       fixes itself. -->
- **The doctor's pty probes apply their baseline through a slave they immediately
  close, and BSD does not carry that to the next open.** `apply_pty_baseline` tries the
  master first and falls back to a momentary slave open — and on this platform the
  master is not a terminal (P2's `termios_settable_without_slave: false`), so the
  fallback is always the path taken. `nodes/pty.rs` already states, at its own non-Linux
  re-assert, that such a set does not survive to the client's open; that is *why* the
  node re-asserts on the rising presence edge. **P10 has been repaired** (it re-asserts
  on the slave it measures and reports `slave_termios_mode`), because its output was
  demonstrably wrong: its Darwin depths were a cooked pty's, and mode is worth an order
  of magnitude — measured on Linux 7.0.0-29, raw accepts less hostward and returns all
  of it while cooked accepts more and returns none. (The figures this sentence used to
  quote are withdrawn: a scratchpad pair no committed `docs/doctor/` artifact backs,
  whose raw half disagreed with the committed Linux capture — notes §3.34's filing,
  discharged by plan §18 item 1. The annotation at the top of this file records the
  finding and stays.) **Six siblings are not
  repaired and this is deliberate**: P6, P7, P8, P9, P12 and P13 take the same fallback,
  so their Darwin answers are not *known* to be measured on the daemon's configuration.
  They are not thereby wrong — a readability question or a targetward write survives a
  cooked discipline far better than a buffer *depth* does — but moving six cross-kernel
  instruments at once with no Mac to re-measure on is the one-way decision on
  single-kernel evidence §7 forbids. Re-measure here before touching them (notes §3.34).
- **P13 (last-close disposition) is pty-only and portable, and macOS is the platform
  it was built for.** It never judges the policy — every policy is legitimate and the
  daemon is correct under each — so its verdict is `supported` whenever the
  measurement completes, and a `waits-then-discards` answer is not a degradation.
  Measured here: `waits-then-discards`, `close_waits_for_reader: true`, 600104 µs with
  0 of 64 recovered against no reader; 23 µs with 64 of 64 when the master drains
  first; 29 µs with 0 of 64 for an `O_NONBLOCK` slave. Note the harness cost this
  implies: on Darwin every never-drained blocking close in a test pays up to 0.601 s of
  wall clock, so P13 itself is ~1.2 s slower here than on Linux (shapes a and c).

**cross-checked** for the gating, and the live verdicts are now measured on the rig —
`docs/doctor/macos-24.6.0-2026-08-05-tier3.json` (binary `fa4b12d6f529`, probe set
`a131e1f4b46d6c83`, 2026-08-04 local / `2026-08-05T00:22:48Z`).

### 6. How to check on a Mac

Run these and attach the output to any macOS bug report:

```
cargo build                              # confirm it builds on the Mac itself
cargo run -p serial-nexus-doctor -- --markdown  # the capability report; attach it
```

The doctor's P1/P2 verdicts and its environment section are the ground truth for
what actually works on that machine — they turn "macOS is different" into a named
delta instead of a mystery (§13, §15.17).

Exercise the control plane, data path, codecs, legs, taps, and the web console with
the portable **`serial-nexus-itest`** harness (the former bash `scripts/validate/**`, now Rust):

```
cargo test --workspace                        # the whole suite; serial-device tests self-skip on macOS
cargo test -p serial-nexus-itest --test control_plane
cargo test -p serial-nexus-itest --test serial_hardware -- --nocapture   # runs when a crossover rig is attached
```

**CI gate.** The Linux lane runs `serial-nexus-doctor --json | jq -e -f
expectations/linux.jq`. The macOS lane runs, and gates on, the
`expectations/macos.jq` counterpart — the *looser* profile this page describes, and
looser clause by clause rather than wholesale. **The sentence that stood here until
2026-08-13 was wrong, and it had become a rationale for a hole:** it said "nothing may
report `unsupported`" had "stopped being the right shape once the probe set grew past
P5". What actually happened is that the summary clause enforcing it was never written
into this file at all — `expectations/linux.jq` carries `(.summary.unsupported == 0)`
and this one carried a bare `(.summary != null)`, while its own head comment claimed,
in text copied from its Linux sibling, that unsupported stayed a gate failure through
the summary clause. Six probes' clauses were bare presence checks with no status
constraint, so an `unsupported` verdict on any of them passed the file. Measured on a
Darwin-shaped report and repaired the same day (plan §18 item 60's class; notes §3.89):
the clause is now the leading one, and per-probe looseness means a probe may be
`supported`, `degraded` or `skipped` — **never `unsupported`**, which is the same
stop condition Linux has. What it requires today: a well-formed report carrying every
probe in the roster, each with a status; P2, P6 and P7 — the POSIX pty mechanisms — not `unsupported` (either verdict
word is fine, and `degraded` is the *expected* macOS answer for P2, §7.2's BSD arm);
P8 `supported` **or** `skipped`, because `epoll(7)` does not exist here, so it is
unmeasurable rather than broken, and the data plane is forbidden from using epoll
anywhere anyway (§15.18); P9 and P10 the same, being informational numbers read
against a tuning target macOS is not; **P12 `supported` or `degraded`** — the one
clause where this file is *stricter* than `linux.jq`, because the session-boundary
edge is the mechanism that carries §6's detach-release here (§15.39) where Linux has
the retained packet, so `skipped` is the expected Linux answer and would be a real
failure on a Mac; **P13 `supported` or `degraded`**, presence-and-status only and
deliberately never a required policy word — pinning `waits-then-discards` would make
a kernel that changed its mind fail the lane instead of reporting the change, which
is the opposite of what a kernel-diff probe is for; and P1, P3, P4, P5, P11 and P15
any status **except `unsupported`** — EXTPROC being unverified and the by-id and
driver-counter mechanisms Linux-only, none of which is a reason to admit the one word
that means "this build cannot answer". *(This clause read "*any* status" until
2026-08-13, which is what the missing summary clause made true in practice.)* The
`linux.jq` gate is the template it is modeled on, and the deltas this one tolerates
are the ones this page enumerates.

## Roadmap to "verified"

Both things this section used to name as future have landed, and the update blocks at
the top of this page are their record. The **macOS CI lane** runs on every push
(`.github/workflows/ci.yml`, job `macos`): `cargo build --workspace --locked`, then the
doctor run gated against `expectations/macos.jq`, then
`cargo test --workspace --locked --no-fail-fast` — a real gate rather than a smoke test,
since P2 reports `degraded` there rather than `unsupported`. **The gate runs before the
tests as of 2026-08-12** (plan §18 item 48): it sat last until then, and since a failed
step skips the rest of the job, every red test step from at least 2026-08-10 hid the one
gate this lane exists to run. The gate needs only the build, so it now sits where its
real dependency is, and both steps carry `steps.build.outcome == 'success'` so a red gate
cannot hide the suite either. One property of that lane is worth keeping
in view because the 2026-07-28 block is what it cost: `cargo test --workspace` stops at
the first failing test binary, so a red macOS lane reports one failure and says nothing
about what sits behind it. Reach for `--no-fail-fast` before concluding a single crate
is the whole of it. The **hands-on pass** ran twice on a real FTDI crossover rig
(2026-07-24, and the whole-suite pass of 2026-07-28): the EXTPROC/packet-mode question
is settled and its *mechanism* named — Darwin's leading packet byte is `0x20`, so §7.2
runs off the reconciliation backstop — and the `cu.*` raw-capture path is exercised end
to end through a physical unplug and replug.

What remains genuinely future is the **IOKit-backed resolver** (§14), which would
restore `usb:`/`by-path:` identities and bare-serial-number adds; it slots behind the
existing `Resolver` API with no design change. Until it lands, the `➖`/`✖` rows of the
feature matrix above are the standing list of what "best-effort" costs on this
platform.
