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
# The kernel-diff probes (P6..P11) are gated on PRESENCE, not on a particular
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
# And the clause that makes the ones above worth having: a kernel-diff probe that
# RAN must carry measurements. A verdict word cannot be diffed, so a probe whose
# observations went empty would pass every clause above while making the 6.18 run
# useless. (`skipped` is exempt — it measured nothing by definition, and its
# `reason` says why.)
and (all(.probes[]; . as $p
      | ((["P6","P7","P8","P9","P10"] | index($p.id)) == null)
      or ($p.status == "skipped")
      or (($p.observations | length) > 0)))
