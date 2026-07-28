# Per-platform expectation for a supported Linux system (plan §4.3):
#   nexus-doctor --json | jq -e -f expectations/linux.jq
#
# Encodes what a supported Linux MUST report. Deliberately lenient where the
# design has a fallback, strict where it does not:
#   - Nothing may be `unsupported` (a probe contradicting the design).
#   - P2 (PTY presence) MUST be `supported` — presence-gated output has no
#     fallback (§7.2).
#   - P1 (EXTPROC/TIOCPKT) may be `supported` OR `degraded` — the §7.2
#     reconciliation poll is an unconditional backstop.
#   - P4 may be `supported` or `skipped` (skipped when no adapter is present).
#   - P5 (rig discovery/certification) may be `supported`, `skipped` OR
#     `degraded` — it is opt-in (transmits), so a run without --port skips; a run
#     against a rig that is miswired, or whose certificate has an uncertified
#     characterization item, is `degraded` with the item named: a rig fault, not a
#     doctor failure (§15.21). `unsupported` stays a gate failure, and since the
#     P5 verdict now folds the certificate in (review 26, DOC-1b) that verdict is
#     reachable: it means the rig did not round-trip data, which §15.21 makes a
#     stop condition before any tiered checklist item runs.
#
# The kernel-diff probes (P6..P12) are gated on PRESENCE, not on a particular
# answer. This is the 6.18 re-gate command (§7: `nexus-doctor --json | jq -e -f
# expectations/linux.jq`), and its job there is to prove the *artifact* is
# complete — every measurement block the 7.0 baseline has, so the two runs are
# diffable field by field. The finding lives in the numbers inside each block, and
# a clause that demanded a verdict word would be asserting the very thing the
# owner went to 6.18 to find out:
#   - P6 (post-hangup pty readiness) and P7 (collapsed-session evidence) may be
#     `supported` OR `degraded`. Both answers are legitimate kernel behaviour and
#     the shipped daemon is correct under either; `degraded` means "a pending
#     simplification is unsafe here" / "one session shape is uncovered", which is
#     a warning to a future editor, not a broken box. They never skip (a probe
#     error degrades, leaving the question open), so those two are the whole set.
#   - P8 (epoll vs read(2)), P9 (poll timeout granularity) and P10 (pty buffer
#     depth) may be `supported` OR `skipped` — never `degraded`, which is the real
#     content of this clause. They are informational: the design is *justified* by
#     P8's answer rather than dependent on it, and P9/P10 report numbers a tuning
#     decision is made against. `skipped` covers a mechanism that does not exist
#     (epoll off Linux) or a probe that could not run, with the reason attached.
#   - P11 (real-port line-state ioctls) may be any of the three: it is opt-in
#     behind --port like P3/P5, so the passive run that this gate normally makes
#     `skipped`, and a port whose driver lacks TIOCGICOUNT is `degraded` by design
#     (the serial node omits the counters rather than faulting, §5).
# `unsupported` remains a gate failure for all of them via the summary clause
# above — no probe here may contradict a design premise, and none can.
#
# Evaluates to `true` (exit 0) only when every clause holds.

(.summary.unsupported == 0)
and (any(.probes[]; .id == "P2" and .status == "supported"))
and (any(.probes[]; .id == "P1" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P4" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P5" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
and (any(.probes[]; .id == "P6" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P7" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P8" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P9" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P10" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P11" and (.status == "supported" or .status == "degraded" or .status == "skipped")))
# P12 (session-boundary edge, §15.39) is inert on Linux by design — the retained
# `TIOCPKT_IOCTL` packet carries §6's detach-release here, which is P7's subject —
# so `skipped` is the *expected* answer and the clause is presence-only. It is
# gated tightly on macOS instead (`expectations/macos.jq`), which is the platform
# where it is the only mechanism. `supported` would mean a Linux kernel grew the
# edge too, which is interesting and not a failure.
and (any(.probes[]; .id == "P12" and .status != "unsupported"))
# And the clause that makes the ones above worth having: a kernel-diff probe that
# RAN must carry measurements. A verdict word cannot be diffed, so a probe whose
# observations went empty would pass every clause above while making the 6.18 run
# useless. (`skipped` is exempt — it measured nothing by definition, and its
# `reason` says why.)
and (all(.probes[]; . as $p
      | ((["P6","P7","P8","P9","P10","P12"] | index($p.id)) == null)
      or ($p.status == "skipped")
      or (($p.observations | length) > 0)))
# And the clause that closes the hole the 2026-07-27 6.18 run walked through: an
# artifact must say what produced it. That run came from a `fe1c52c`-vintage
# binary rather than HEAD, and the only reason anyone noticed was that its P4
# section still carried the pre-`RES-2` *title* — read by eye, after the fact.
# Every clause above passed it, because none of them could see the probe set move.
#
# `probe_set` is the load-bearing half: two reports with the same fingerprint
# asked the same questions of their kernels, which is precisely the precondition
# "diffable field by field" names, and it is computable anywhere. `commit` must be
# *present* but may read `unknown` — a build from a source tarball, or inside a
# container without git, cannot know it, and failing a healthy box over that would
# be the false negative this file's P4 clause already refuses to make.
and (.build.probe_set | type == "string" and length > 0)
and (.build.commit | type == "string" and length > 0)
