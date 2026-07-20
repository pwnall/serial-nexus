#!/usr/bin/env bash
# Phase 1 validation: calibrate the judges before they judge (plan §4, phase 1).
# `nexus-sim pty --echo` against `nexus-sim client --send seeded:1MiB --expect
# echo` must round-trip with matching checksums — proving the PTY doubles work
# before any feature relies on them.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

cargo build -q -p nexus-sim || { echo '{"check":"sim-selftest","pass":false,"reason":"build failed"}'; exit 1; }
SIM="$REPO_ROOT/target/debug/nexus-sim"

TMP="$(mktemp -d)"
PTY_PID=""
cleanup() { [ -n "$PTY_PID" ] && kill "$PTY_PID" 2>/dev/null; rm -rf "$TMP"; }
trap cleanup EXIT

LINK="$TMP/dut"
"$SIM" pty --echo --link "$LINK" --timeout-ms 20000 >"$TMP/pty.json" 2>/dev/null &
PTY_PID=$!

if ! "$REPO_ROOT/scripts/lib/wait-for.sh" "test -e '$LINK'" 5 0.05; then
  echo '{"check":"sim-selftest","pass":false,"reason":"pty link never appeared"}'
  exit 1
fi

OUT="$("$SIM" client --path "$LINK" --send seeded:1MiB --expect echo --seed 42 --timeout-ms 20000)"
echo "$OUT"
printf '%s' "$OUT" | jq -e '.pass == true and .sent == 1048576 and .sent == .received and .sha256_sent == .sha256_received' >/dev/null
