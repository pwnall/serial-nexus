#!/usr/bin/env bash
# Bounded polling on a condition — never a bare sleep (§3).
# Usage: wait-for.sh '<command>' <timeout-seconds> [interval-seconds]
# Exits 0 as soon as <command> exits 0; 1 on timeout.
set -uo pipefail

cmd="${1:?usage: wait-for.sh <command> <timeout> [interval]}"
timeout="${2:-5}"
interval="${3:-0.1}"

end=$(( $(date +%s) + timeout ))
while :; do
  if bash -c "$cmd" >/dev/null 2>&1; then
    exit 0
  fi
  if [[ $(date +%s) -ge $end ]]; then
    echo "wait-for: timed out after ${timeout}s waiting for: $cmd" >&2
    exit 1
  fi
  sleep "$interval"
done
