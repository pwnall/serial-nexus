#!/usr/bin/env bash
# Phase 8 validation (item 1): the five-minute quickstart, wall-clocked. A clean
# checkout reaches a passing end-to-end echo verdict well under the five-minute
# budget — the README happy path, exactly (design §2, plan §Phase 8).
#
# Topology (no hardware, §15.17):
#
#   nexus-sim client  ─▶  [ pty "console" ]──edge──[ serial "usb0" ]  ─▶  nexus-sim pty --echo
#      (operator)            $TTY symlink          free-for-all             (the "device")
#
# free-for-all skips the write lock, so an operator just types (§6). A 64 KiB
# seeded round-trip that returns byte-identical is the success condition.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-quickstart\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

BUDGET_S=300   # the five-minute wall-clock budget (plan §Phase 8 item 1)
START=$SECONDS

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8qs.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/device"
TTY="$TMPD/console"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SIMPID:-}" ] && kill "$SIMPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# 1. The fake device — an echoing PTY standing where /dev/ttyUSB0 will be.
"$SIM" pty --echo --link "$DEV" --timeout-ms 60000 >"$TMPD/device.log" 2>&1 &
SIMPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

# 2. The daemon, in a short-path runtime dir (SUN_LEN, §10).
"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }
# The socket IS the authorization model — mode 0600 (§10).
[ "$(stat -c '%a' "$SOCK")" = "600" ] || fail "control socket is not mode 0600"

# 3. The demo config: serial (host-facing, free-for-all) -> pty (target-facing).
cat > "$TMPD/demo.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "console"
path = "$TTY"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/demo.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "usb0 never reached active"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="console")|.status=="active"' >/dev/null \
  || fail "console not active"
[ -L "$TTY" ] || fail "pty symlink not created"

# 4. The echo verdict: 64 KiB out, byte-identical back — the whole round trip.
"$SIM" client --path "$TTY" --send seeded:64KiB --expect echo --seed 42 --timeout-ms 15000 \
  | jq -e '.pass==true and .sent==65536 and .received==65536' >/dev/null \
  || { cat "$TMPD/daemon.log" "$TMPD/device.log"; fail "64KiB echo round-trip failed"; }

# The no-terminal path README also shows: `send` is an atomic acquire-write-release.
"$C" --json send usb0 --line "hello" | jq -e '.delivered==true and .sent==6' >/dev/null \
  || fail "send usb0 --line did not deliver"

"$C" shutdown >/dev/null

ELAPSED=$(( SECONDS - START ))
[ "$ELAPSED" -lt "$BUDGET_S" ] || fail "took ${ELAPSED}s, over the ${BUDGET_S}s budget"
echo "{\"check\":\"phase8-quickstart\",\"pass\":true,\"elapsed_s\":$ELAPSED,\"budget_s\":$BUDGET_S}"
