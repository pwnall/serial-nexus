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
# Evaluates to `true` (exit 0) only when every clause holds.

(.summary.unsupported == 0)
and (any(.probes[]; .id == "P2" and .status == "supported"))
and (any(.probes[]; .id == "P1" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P4" and (.status == "supported" or .status == "skipped")))
and (any(.probes[]; .id == "P5" and (.status == "supported" or .status == "skipped" or .status == "degraded")))
