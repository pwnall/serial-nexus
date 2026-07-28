# Per-platform expectation for a best-effort macOS system (design §13, plan §Phase 8):
#   nexus-doctor --json | jq -e -f expectations/macos.jq
#
# macOS is explicitly best-effort: PTYs and the poll(2) data plane are plain POSIX
# and portable, but several Linux-only mechanisms have no macOS equivalent yet (no
# /dev/serial/by-id tree, no TIOCGICOUNT driver counters, unverified EXTPROC). So
# this gate is deliberately LENIENT — it checks that the doctor produced a
# well-formed report and that the portable mechanisms did not regress, while letting
# the Linux-only probes skip/degrade/report unsupported without failing CI:
#
#   - The report is structurally sound: a summary object and all twelve probes
#     (P1..P12) present, each carrying a status. (`>= 12` rather than `== 12`
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
#     `epoll(7)` does not exist here, `nexus-sys`'s stub answers ENOTSUP, and the
#     data plane is forbidden from using epoll anyway (invariant 1), so nothing is
#     untested — only unmeasurable. `unsupported` would be flatly wrong.
#   - P9 (poll timeout granularity) and P10 (pty buffer depth) are POSIX and
#     should measure here, but a probe error skips rather than failing the lane:
#     both are informational numbers a tuning decision is read against, and macOS
#     is not the tuning target.
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
and (.probes | length >= 12)
and (all(.probes[]; .status != null))
and (any(.probes[]; .id == "P1"))
and (any(.probes[]; .id == "P2" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P3"))
and (any(.probes[]; .id == "P4"))
and (any(.probes[]; .id == "P5"))
and (any(.probes[]; .id == "P6" and .status != "unsupported"))
and (any(.probes[]; .id == "P7" and .status != "unsupported"))
and (any(.probes[]; .id == "P8" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P9" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P10" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P11"))
# P12 (session-boundary edge, §15.39) is the mechanism that carries §6's
# detach-release on THIS platform — Darwin destroys the readable packet P7 measures
# — so unlike every clause above it, macOS is where P12 is load-bearing and Linux is
# where it is inert. `supported` or `degraded`, never `skipped`: a skip here means
# the latch compiled out on the one platform that needs it, which is exactly the
# silent regression a presence-only clause would wave through. Its numbers are the
# point, like P6/P7's.
and (any(.probes[]; .id == "P12" and (.status == "supported" or .status == "degraded")))
# Provenance, same as `linux.jq` and for the same reason: a report that cannot say
# which build produced it makes every cross-platform claim rest on an accident.
# Structure only — `commit` may read `unknown` off a tarball build.
and (.build.probe_set | type == "string" and length > 0)
and (.build.commit | type == "string" and length > 0)
