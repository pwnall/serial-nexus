# Per-platform expectation for a best-effort macOS system (design §13, plan §Phase 8):
#   serial-nexus-doctor --json | jq -e -f expectations/macos.jq
#
# macOS is explicitly best-effort: PTYs and the poll(2) data plane are plain POSIX
# and portable, but several Linux-only mechanisms have no macOS equivalent yet (no
# /dev/serial/by-id tree, no TIOCGICOUNT driver counters, unverified EXTPROC). So
# this gate is deliberately LENIENT — it checks that the doctor produced a
# well-formed report and that the portable mechanisms did not regress, while letting
# the Linux-only probes skip/degrade/report unsupported without failing CI:
#
#   - The report is structurally sound: a summary object and all fifteen probes
#     (P1..P15) present, each carrying a status. (`>= 15` rather than `== 15`
#     because P3 emits one probe per --port.)
#   - P2 (PTY presence, POLLHUP) is POSIX — it must NOT be `unsupported`
#     (`supported` or `degraded` while unverified on a given macOS runner is fine).
#     Presence-gated output has no fallback (§7.2), so a genuine macOS regression
#     here is worth surfacing.
#   - P1 (EXTPROC/TIOCPKT), P3 (serial fit / TIOCGICOUNT), P4 (by-id resolution),
#     and P5 (rig certification) may be any status on macOS — EXTPROC is unverified
#     and degrades to the poll-only backstop, and the by-id/counter mechanisms are
#     Linux-only (the deferred IOKit resolver, §12/§14, is their macOS home).
#   - P6 (post-hangup pty readiness) and P7 (collapsed-session evidence) are pty
#     probes and portable, so they must not be `unsupported` — but either verdict
#     word is fine, and `degraded` is a genuinely likely macOS answer (§7.2's BSD
#     arm applies the baseline termios through the slave, which is exactly the
#     mechanism P7 measures). Their numbers are the point on this platform too.
#   - P8 (epoll vs read(2)) is **Linux-only and must be allowed to skip**:
#     `epoll(7)` does not exist here, `serial-nexus-sys`'s stub answers ENOTSUP, and the
#     data plane is forbidden from using epoll anyway (invariant 1), so nothing is
#     untested — only unmeasurable. `unsupported` would be flatly wrong.
#   - P9 (poll timeout granularity) and P10 (pty buffer depth) are POSIX and
#     should measure here, but a probe error skips rather than failing the lane:
#     both are informational numbers a tuning decision is read against, and macOS
#     is not the tuning target.
#   - P10 additionally admits `degraded`, and this platform is *why*. Its depths are
#     only this kernel's depths if the pty was in the daemon's raw baseline, and off
#     Linux that baseline is applied through a slave `apply_pty_baseline` immediately
#     closes — which BSD does not carry to the next open. P10 now re-asserts on the
#     slave it measures and reports `slave_termios_mode`; if that re-assert ever
#     fails here, the probe must be able to SAY the number is unsound rather than be
#     forced to `supported`. A `degraded` P10 on this lane means exactly that and
#     should be read, not silenced (notes §3.34).
#     ANNOTATION 2026-08-05 (§5): the re-assert has now been exercised on Darwin and
#     it takes. `docs/doctor/macos-24.6.0-2026-08-05-7ead470-tier3{,-2,-3}.json`
#     report `slave_termios_mode: "raw"` on both directions with P10 `supported` in
#     all three runs, so the arm above stays as insurance rather than as the expected
#     answer. Recorded because an expectation that was measured and held is as
#     load-bearing as one that was refuted (§9) — the arm must not be read later as
#     dead code merely because it has never fired.
#   - P11 (real-port line-state ioctls) may be any status: it is opt-in behind
#     --port (so the CI run skips), and on a named macOS port it is `degraded` by
#     design, because TIOCGICOUNT is Linux-only and the serial node omits the
#     counters rather than faulting (§5, §13).
#
# The macOS CI lane runs this with `jq -e`, unguarded, so a failing clause FAILS the
# job — the check has been gating since the §15.30 hands-on macOS pass, not
# informational as this comment said while it was still awaiting one. What keeps that
# honest rather than brittle is the leniency above: the clauses assert structure and
# the portable mechanisms, and every Linux-only probe is free to skip, degrade or
# report unsupported. Widen a clause here rather than un-gating the step (§13).
#
# Evaluates to `true` (exit 0) only when every clause below holds.

(.summary != null)
and (.probes | length >= 15)
and (all(.probes[]; .status != null))
and (any(.probes[]; .id == "P1"))
and (any(.probes[]; .id == "P2" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P3"))
and (any(.probes[]; .id == "P4"))
# The one P4 clause that is NOT lenient, and it is not a capability demand — it is a
# well-formedness demand, of exactly the kind the clauses at the top of this file
# make. A `supported` P4 asserts "Resolver produces canonical identities; configs
# survive replug and cold start", and on this platform it asserted that off a loop
# that ran zero times: `count: 0` with `status: supported` in all three of
# `docs/doctor/macos-24.6.0-2026-08-05-1a9a8fc-tier3{,-2,-3}.json` (notes §3.45 (ii)).
# The status stays free — `degraded` is the honest answer here today, `supported`
# becomes reachable if the deferred IOKit backend (§14) ever lands, and both must
# pass — but the report may not claim the property while reporting a population of
# zero. Identical to `expectations/linux.jq`'s clause, deliberately: the defect is a
# report lying about itself, which is not a platform property.
#
# **This gate is only ever run against a LIVE report** — the lane pipes
# `serial-nexus-doctor --json` straight into it (`.github/workflows/ci.yml`), and nothing
# runs it over `docs/doctor/*.json`. So requiring the key costs nothing an operator will
# meet: a capture they have just taken comes from the current binary and carries it. It
# rejects only genuinely old artifacts, which nobody validates and which honestly cannot
# answer the question. (The archive was never uniformly gate-clean anyway — six of the
# nineteen committed reports predate P13 and fail on that clause regardless, one of them
# a macOS capture.)
and (all(.probes[]; . as $p
      | ($p.id != "P4")
      or ($p.status != "supported")
      or (([$p.observations[] | select(.key == "canonical") | .value] | last) as $c
          | (($c | type) == "number") and ($c > 0))))
and (any(.probes[]; .id == "P5"))
# **A certified pair must carry its handshake reading** (§15.52). The certificate's
# own modem map is read with the peer port *closed*, so it cannot answer what the
# wire carries; the handshake block is the only read taken with both ports open,
# and it is what turns "this rig might have RTS/CTS" into a measurement. Presence
# only, and conditioned on a pair certificate having run — whether *this* rig
# crosses RTS is the operator's cabling, a 3-wire rig is §5's own stated
# assumption, and a clause that pinned the answer would redden every honest bench
# (plan §3 rule 14). A passive run has no pair, so the antecedent is false and the
# clause is silent.
and (all(.probes[]; . as $p | ($p.id != "P5")
      or (($p.observations | any((.key | endswith(" cert"))
             and (.value | type == "string") and (.value | startswith("rate_ladder")))) | not)
      or ($p.observations | any((.key | endswith(" handshake")) and (.value | type == "string")))))
and (any(.probes[]; .id == "P6" and .status != "unsupported"))
and (any(.probes[]; .id == "P7" and .status != "unsupported"))
and (any(.probes[]; .id == "P8" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P9" and (.status == "supported" or .status == "skipped")))
# P9's mask column must MEASURE its own framing, and this lane is where that stopped being
# theoretical: Darwin does not deliver POLLHUP to a mask that requested nothing, so the
# `shape` and the rationale §3.52 hardcoded from POSIX were printed beside a field reading
# `false` (notes §3.65). Gated: the measurement is present, and the report names which
# separation and which mask-spread figures go with it — the pair must be present so the two
# can never be read from different cell sets. The ANSWER stays free: either kernel reading
# is legitimate, and a kernel that gates the hangup is `degraded`-worthy at most, never a
# gate failure (§7).
and (all(.probes[]; . as $p | ($p.id != "P9") or ($p.status == "skipped") or
      ($p.observations | any(.key == "zero_timeout_by_fd_state"
         and (.value.hangup_delivered_to_a_mask_that_requested_nothing | type == "boolean")
         and (.value.shape | type == "string")
         and (.value.mask_role | type == "string")
         and (.value.read_the_separation_from | type == "string")
         and (.value.read_the_mask_spread_from | type == "string")
         and (.value.order_control_says | type == "string")
         and (.value.order_control_does_not_license | type == "string")))))
and (any(.probes[]; .id == "P10" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
# P10 carries an instrument check on its own FIONREAD reading, and THIS is the lane where
# it fires: `peer_pending_input_bytes` answers 0 on a Darwin pty master with 1024 bytes
# recoverable, 6 of 6 captures across two binaries (notes §3.45 iv / §3.50), so a bare `0`
# there reads as "empty" and is not. What is gated is the PRESENCE of the cross-check, not
# its answer — a kernel whose FIONREAD differs must REPORT, not fail (§7), which is also
# why P10's status is deliberately left alone by that finding.
and (all(.probes[]; . as $p | ($p.id != "P10") or ($p.status == "skipped") or
      (["slave_to_master_targetward","master_to_slave_hostward"] | all(. as $d |
         $p.observations | any(.key == $d
            and (.value.peer_pending_input_trust | type == "string")
            and (.value | has("peer_pending_input_bytes_at_drain"))
            and (.value | has("writer_pending_input_bytes")))))))
# P10's recheck must be a LADDER, and this clause exists because the gate could not tell
# the difference. `f8315cc` replaced a one-rung recheck — which could not separate a
# capacity from any watermark above the single drain size it sampled — with a multi-rung
# one, and neither expectation file was touched: deleting `recheck` outright, emptying
# `ladder`, or handing back exactly the one rung the commit exists to replace all left
# both gates green (notes §3.65). Cardinality, not answer: a kernel is free to refuse
# every rung or none, but a report may not claim a bracket it computed from one point.
and (all(.probes[]; . as $p | ($p.id != "P10") or ($p.status == "skipped") or
      (["slave_to_master_targetward","master_to_slave_hostward"] | all(. as $d |
         $p.observations | any(.key == $d
            and ((.value.recheck.ladder | type == "array") and (.value.recheck.ladder | length >= 2))
            and (.value.recheck.ladder_reading | has("rungs_carrying_a_bound"))
            and (.value.recheck.ladder_reading | has("watermark_threshold_gt"))
            and (.value.recheck.ladder_reading | has("watermark_threshold_le")))))))
and (any(.probes[]; .id == "P11"))
# P15 (flow-control honouring, notes §3.65 E) may be any status, and this lane is the
# one where it is expected to be `degraded`: Apple's IOSerialFamily driver accepts a
# CRTSCTS request and reads the flag back clear, so an `rts-cts` edge faults the node
# here. What is gated is that the probe SAYS so per port — the reading and the
# restore, never the answer, because "this driver honours it" and "this driver drops
# it" are both legitimate kernel facts (§7). `baseline_restored` is required because a
# probe that reconfigures a real adapter and cannot say it put it back is worse than
# one that never ran.
and (any(.probes[]; .id == "P15"))
and (all(.probes[]; . as $p | ($p.id != "P15") or ($p.status | startswith("skipped")) or
      ($p.observations | any((.value | type == "object")
         and (.value.honoured_on_readback | type == "boolean")
         and (.value.tcsetattr_ok | type == "boolean")
         and (.value.baseline_restored | type == "boolean")))))
# P12 (session-boundary edge, §15.39) is the mechanism that carries §6's
# detach-release on THIS platform — Darwin destroys the readable packet P7 measures
# — so unlike every clause above it, macOS is where P12 is load-bearing and Linux is
# where it is inert. `supported` or `degraded`, never `skipped`: a skip here means
# the latch compiled out on the one platform that needs it, which is exactly the
# silent regression a presence-only clause would wave through. Its numbers are the
# point, like P6/P7's.
and (any(.probes[]; .id == "P12" and (.status == "supported" or .status == "degraded")))
# P12's anti-spin claim must arrive with its witness: six committed captures print
# `idle_edges_in_200_passes: 0` with no elapsed time and no control, and a zero from a
# latch that cannot post an edge is not a measurement (§9). On this lane the latch is the
# live mechanism, so `control_session_edge` is the field that separates "quiet kernel"
# from "inert instrument" — and P12's own `supported` arm now refuses to be reached
# without it (notes §3.50). The `idle_windows` arm is the escape hatch for a box where the
# windows could not run at all — an error that NAMES itself, which is what §7 asks.
and (all(.probes[]; . as $p | ($p.id != "P12") or
      ((($p.observations | any(.key == "control_session_edge"))
        or ($p.observations | any(.key == "idle_windows")))
       and (($p.observations | any(.key == "idle_window_paced"
              and (.value.elapsed_us | type == "number")
              and (.value.passes | type == "number")))
        or ($p.observations | any(.key == "idle_windows"))))))
# P13 (last-close disposition) is pty-only and portable, so it must measure here —
# and macOS is the platform it was built for. The XNU reading behind this clause is
# no longer a prediction: it was measured on Darwin 24.6.0 / macOS 15.7.8 and the
# answer is `waits-then-discards`, `close_waits_for_reader` true, with
# `a_no_reader_blocking_slave` at 600104 us and 0 of 64 recovered (`ttywait` running
# to its 60-tick `t_timeout` at hz 100), `b_reader_drains_before_close` at 23 us and
# 64 of 64, and `c_no_reader_nonblocking_slave` at 29 us and 0 of 64 — the O_NONBLOCK
# arm of the same `ttylclose` branch, measured as an A/B rather than inferred. See
# `docs/doctor/macos-24.6.0-2026-08-05-tier3.json` (binary `fa4b12d6f529`, probe set
# `a131e1f4b46d6c83`); Linux 7.0.0-29 reads `retains`, with the no-reader close at
# 20/10/13 us across `docs/doctor/linux-7.0-2026-08-05-tier3{,-2,-3}.json` (binary
# `71fc5a815852`, same probe set) and 64 of 64 recovered in all three shapes.
# <!-- ANNOTATION 2026-08-05 (§5). The four figures above previously read 601087 /
#      13 / 28 us for Darwin and "7 us" for Linux, all attributed to the artifacts
#      named here. None of those numbers appears in any committed docs/doctor/
#      report: the Darwin file reads 600104/23/29 and the three Linux files read
#      20/3/15, 10/13/15 and 13/2/19. This is the same scrollback-for-artifact
#      substitution (§16.13) that the 2026-08-05 sweep corrected in five documents
#      and did not reach here, because this is a gate file rather than prose. The
#      numbers live in `#` comments, so no clause evaluated them and CI never
#      failed on them — the defect is the attribution, not the gate. Corrected
#      2026-08-05 against the artifacts themselves. -->
# The clause is
# still presence-and-status only, deliberately: pinning the word would make a kernel
# that changed its mind fail the lane instead of reporting the change, which is the
# opposite of what this probe is for. Read the numbers, diff them, then decide.
and (any(.probes[]; .id == "P13" and (.status == "supported" or .status == "degraded")))
# P14 (maximum-rate search, §15.51) is opt-in behind `--port` *and* needs a
# cross-paired rig, so `skipped` is the expected answer on every passive run and
# on every box with one adapter. `degraded` is admissible and is not a
# loosening: §15.51 gives that word to exactly three states in which the probe
# could not *ask* its question — P5's rate ladder did not round-trip under it,
# the search did not complete, or the closing restore did not put the rig back —
# and forbidding it here would force the one probe that knows its own
# measurement is unsound to report a confident number instead, which is the
# defect `expectations/linux.jq`'s P10 clause already exists to prevent.
# `unsupported` stays a gate failure through the summary clause: a rig that tops
# out at 115200 is slow, not a contradicted design premise.
and (any(.probes[]; .id == "P14" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
# **And the answer is never pinned.** The ceiling is a property of the operator's
# silicon — an FT232R stops at 3 Mbaud, an H-series part advertises 12, and a
# Darwin ask-surface may refuse below either — so a clause naming a number, or
# even a `ceiling_kind`, would be this file asserting the very thing the probe
# was written to find out (plan §3 rule 14). What is gated is that the two cells
# a reader needs are *present*: the number, and the reason the search stopped.
# Both may read `null` — an incomplete search has no ceiling and says so, and the
# absence of a reason must not be dressed up as a fifth reason — so the clause
# tests `has`, never a type and never a value.
and (all(.probes[]; . as $p | ($p.id != "P14") or ($p.status == "skipped")
      or (($p.observations | any(.key == "max_reliable_baud"))
          and ($p.observations | any(.key == "ceiling_kind"))
          and ($p.observations | any(.key == "structural_max_baud" and (.value | type == "number")))
          and ($p.observations | any(.key == "ceiling_is_a_floor_over" and (.value | type == "string"))))))
# **And a `supported` P14 must have measured something.** The clause above tests
# presence and deliberately admits `null`, because a search that could not finish
# still has to carry its keys — that is what the `degraded` arm is for. It left a
# hole this file could not see: a P14 reading `supported` with both cells `null`
# satisfied every clause here, proven by mutating a committed artifact (notes
# §3.73). `p14_verdict` already refuses that combination — it degrades whenever
# either cell is `None` — so this clause pins a property the probe *promises*
# rather than an answer it might give, and no honest report can trip it. It names
# no number and no `ceiling_kind`, so plan §3 rule 14 is untouched. Read through
# `last`: until 2026-08-07 the probe stamped a `null` placeholder and *appended*
# the measurement, so a frozen artifact carries each key twice and only the later
# occurrence is the answer (§16.13 leaves those files untouched).
and (all(.probes[]; . as $p | ($p.id != "P14") or ($p.status != "supported")
      or ((([$p.observations[] | select(.key == "max_reliable_baud") | .value] | last) != null)
          and (([$p.observations[] | select(.key == "ceiling_kind") | .value] | last) != null))))
# ...and a probe that RAN must carry the numbers, which is the whole reason the
# clauses above stay presence-and-status only. `linux.jq` has always asserted this;
# macOS did not, so a P13 whose observations went empty reported `supported` and
# sailed through the one lane whose answer is interesting. A verdict word cannot be
# diffed. `skipped` is exempt (P8 here) — it did not run, so it owes nothing.
and (all(.probes[]; . as $p
      | ((["P6","P7","P8","P9","P10","P12","P13","P14"] | index($p.id)) == null)
      or ($p.status == "skipped")
      or (($p.observations | length) > 0)))
# Provenance, same as `linux.jq` and for the same reason: a report that cannot say
# which build produced it makes every cross-platform claim rest on an accident.
# Structure only — `commit` may read `unknown` off a tarball build.
and (.build.probe_set | type == "string" and length > 0)
and (.build.commit | type == "string" and length > 0)
# The cell-set digest, same clause and same reason as `linux.jq`: `probe_set`
# equality says the two runs asked the same questions, never that they carry the
# same cells — and this lane is the one where that misread happened, four commits
# printing one `a131e1f4b46d6c83` across five observation sets. Structure only:
# the value is a property of the run (ports named, kernel, which histogram keys
# were observed), and on Darwin it legitimately differs from every Linux run, so
# pinning a value would redden a healthy box. Reports captured before 2026-08-05
# fail it, which costs nothing — this file gates fresh `--json` output.
and (.build.field_set | type == "string" and length == 16)
