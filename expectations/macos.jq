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
#   - The report is structurally sound: a summary object and all five probes
#     (P1..P5) present, each carrying a status.
#   - P2 (PTY presence, POLLHUP) is POSIX — it must NOT be `unsupported`
#     (`supported` or `degraded` while unverified on a given macOS runner is fine).
#     Presence-gated output has no fallback (§7.2), so a genuine macOS regression
#     here is worth surfacing.
#   - P1 (EXTPROC/TIOCPKT), P3 (serial fit / TIOCGICOUNT), P4 (by-id resolution),
#     and P5 (rig certification) may be any status on macOS — EXTPROC is unverified
#     and degrades to the poll-only backstop, and the by-id/counter mechanisms are
#     Linux-only (the deferred IOKit resolver, §12/§14, is their macOS home).
#
# The macOS CI lane runs this as an INFORMATIONAL check: the gating macOS deliverable
# is that the workspace builds and the portable unit/property tests pass; the doctor
# report and the phase-2 e2e are observational until a hands-on macOS pass (§13).
#
# Evaluates to `true` (exit 0) only when every clause below holds.

(.summary != null)
and (.probes | length >= 5)
and (all(.probes[]; .status != null))
and (any(.probes[]; .id == "P1"))
and (any(.probes[]; .id == "P2" and (.status == "supported" or .status == "degraded")))
and (any(.probes[]; .id == "P3"))
and (any(.probes[]; .id == "P4"))
and (any(.probes[]; .id == "P5"))
