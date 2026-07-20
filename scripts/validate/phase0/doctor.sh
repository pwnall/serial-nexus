#!/usr/bin/env bash
# Phase 0 validation: nexus-doctor runs every capability probe (design §15.17)
# and reports no *unsupported* capability — a probe contradicting the design is
# a stop condition; `skipped` (no adapter) and `degraded` (a fallback applies)
# are both acceptable CI verdicts (plan §4).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

JSON="$(cargo run -q -p nexus-doctor -- --json 2>/dev/null)"
printf '%s' "$JSON" | jq -c '{summary}'

# Platform expectation: presence must work, EXTPROC may degrade to poll, nothing
# unsupported. `-e` turns the predicate into the exit code.
printf '%s' "$JSON" | jq -e -f "$REPO_ROOT/expectations/linux.jq" >/dev/null
