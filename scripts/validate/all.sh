#!/usr/bin/env bash
# Run every phase validation script up through phase N (§3).
# Usage: scripts/validate/all.sh [--through N]
set -uo pipefail

THROUGH=8
while [[ $# -gt 0 ]]; do
  case "$1" in
    --through) THROUGH="${2:?}"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rc=0
for n in $(seq 0 "$THROUGH"); do
  dir="$REPO_ROOT/scripts/validate/phase$n"
  [[ -d "$dir" ]] || continue
  shopt -s nullglob
  for s in "$dir"/*.sh; do
    echo "=== $s ===" >&2
    if bash "$s"; then
      echo "  PASS" >&2
    else
      rc=1
      echo "  FAIL: $s" >&2
    fi
  done
  shopt -u nullglob
done
exit "$rc"
