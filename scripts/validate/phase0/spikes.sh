#!/usr/bin/env bash
# Phase 0 validation: run every self-judging spike and require each to pass
# (skipped hardware spikes still report pass:true with skipped:true). A nonzero
# spike is a stop condition (plan §1).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

rc=0
for s in s1_extproc s2_presence s3_serial2 s4_resolver s5_rpc; do
  out="$(cargo run -q -p spikes --bin "$s" 2>/dev/null)"
  echo "$out"
  pass="$(printf '%s' "$out" | jq -r '.pass // false' 2>/dev/null)"
  if [ "$pass" != "true" ]; then
    echo "SPIKE FAILED: $s" >&2
    rc=1
  fi
done
exit "$rc"
