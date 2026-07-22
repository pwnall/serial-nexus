#!/usr/bin/env bash
# Run every phase validation script up through phase N (§3).
# Usage: scripts/validate/all.sh [--through N] [--json-summary FILE]
#
# With --json-summary, aggregate every script's JSON verdict into FILE (the sweep's
# verdict JSON, §16.5 nightly-lane artifact): {total, passed, failed, scripts:[...]}.
# Without it, behavior is unchanged — each script's verdict streams to stdout and a
# PASS/FAIL line per script to stderr.
set -uo pipefail

THROUGH=8
SUMMARY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --through) THROUGH="${2:?}"; shift 2 ;;
    --json-summary) SUMMARY="${2:?}"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rc=0
items=""
[[ -n "$SUMMARY" ]] && items="$(mktemp)"

for n in $(seq 0 "$THROUGH"); do
  dir="$REPO_ROOT/scripts/validate/phase$n"
  [[ -d "$dir" ]] || continue
  shopt -s nullglob
  for s in "$dir"/*.sh; do
    echo "=== $s ===" >&2
    if [[ -n "$SUMMARY" ]]; then
      out="$(bash "$s")"; sc=$?
      printf '%s\n' "$out"   # preserve the streamed verdicts
    else
      bash "$s"; sc=$?
    fi
    if [[ $sc -eq 0 ]]; then
      echo "  PASS" >&2
    else
      rc=1
      echo "  FAIL: $s" >&2
    fi
    if [[ -n "$SUMMARY" ]]; then
      verdict="$(printf '%s\n' "$out" | grep -E '^\{' | tail -1)"
      [[ -z "$verdict" ]] && verdict='null'
      pass=$([[ $sc -eq 0 ]] && echo true || echo false)
      rel="${s#"$REPO_ROOT"/}"
      jq -cn --arg s "$rel" --argjson pass "$pass" --argjson v "$verdict" \
        '{script:$s,pass:$pass,verdict:$v}' >>"$items" 2>/dev/null \
        || jq -cn --arg s "$rel" --argjson pass "$pass" \
             '{script:$s,pass:$pass,verdict:null}' >>"$items"
    fi
  done
  shopt -u nullglob
done

if [[ -n "$SUMMARY" ]]; then
  jq -s '{total:length,
          passed:(map(select(.pass))|length),
          failed:(map(select(.pass|not))|length),
          scripts:.}' "$items" >"$SUMMARY"
  rm -f "$items"
fi
exit "$rc"
