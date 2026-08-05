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
#   - The report is structurally sound: a summary object and all thirteen probes
#     (P1..P13) present, each carrying a status. (`>= 13` rather than `== 13`
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
and (.probes | length >= 13)
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
and (all(.probes[]; . as $p
      | ($p.id != "P4")
      or ($p.status != "supported")
      or ((([$p.observations[] | select(.key == "canonical") | .value] | first) as $c
           | $c == null or $c > 0))))
and (any(.probes[]; .id == "P5"))
and (any(.probes[]; .id == "P6" and .status != "unsupported"))
and (any(.probes[]; .id == "P7" and .status != "unsupported"))
and (any(.probes[]; .id == "P8" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P9" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P10" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
and (any(.probes[]; .id == "P11"))
# P12 (session-boundary edge, §15.39) is the mechanism that carries §6's
# detach-release on THIS platform — Darwin destroys the readable packet P7 measures
# — so unlike every clause above it, macOS is where P12 is load-bearing and Linux is
# where it is inert. `supported` or `degraded`, never `skipped`: a skip here means
# the latch compiled out on the one platform that needs it, which is exactly the
# silent regression a presence-only clause would wave through. Its numbers are the
# point, like P6/P7's.
and (any(.probes[]; .id == "P12" and (.status == "supported" or .status == "degraded")))
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
# ...and a probe that RAN must carry the numbers, which is the whole reason the
# clauses above stay presence-and-status only. `linux.jq` has always asserted this;
# macOS did not, so a P13 whose observations went empty reported `supported` and
# sailed through the one lane whose answer is interesting. A verdict word cannot be
# diffed. `skipped` is exempt (P8 here) — it did not run, so it owes nothing.
and (all(.probes[]; . as $p
      | ((["P6","P7","P8","P9","P10","P12","P13"] | index($p.id)) == null)
      or ($p.status == "skipped")
      or (($p.observations | length) > 0)))
# Provenance, same as `linux.jq` and for the same reason: a report that cannot say
# which build produced it makes every cross-platform claim rest on an accident.
# Structure only — `commit` may read `unknown` off a tarball build.
and (.build.probe_set | type == "string" and length > 0)
and (.build.commit | type == "string" and length > 0)
