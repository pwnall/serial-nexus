#!/usr/bin/env bash
# Phase 8 validation (item 4): the full operator scenario driven purely through
# `serialnexusctl --json` (+ jq) — the scripted stand-in for §15.16's agent feedback
# loop. Inspect state, lock a channel, send a command, verify the device received
# it, rotate its log, verify continuity, unlock (design §6, §7.3, §10).
#
# Topology (no hardware, §15.17):
#
#   [ log "cap" ]──edge(never)──┐
#                               ├─[ serial "usb0" exclusive ]  ─▶  nexus-sim pty --echo
#   [ pty "console" ]──edge─────┘        (device double)              (echoes targetward)
#
# Every command is a KNOWN line, so `printf … | sha256sum` is an independent oracle:
# the echo device reflects each command hostward, the log captures it, and the
# concatenation of rotated+live log files must equal the exact command transcript —
# proving exclusivity (a locked-out `send` leaks nothing) and lossless rotation.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-agent-task\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8at.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/device"
TTY="$TMPD/console"
LOGDIR="$TMPD/logs"; mkdir -p "$LOGDIR"
CC=("$C" --socket "$SOCK")
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SIMPID:-}" ] && kill "$SIMPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# combined checksum of the log's rotated + live files, in chronological order
log_sha() { cat "$LOGDIR"/console.log.* "$LOGDIR"/console.log 2>/dev/null | sha256sum | awk '{print $1}'; }
log_bytes() { cat "$LOGDIR"/console.log.* "$LOGDIR"/console.log 2>/dev/null | wc -c; }

# The echo "device", present for the whole run.
"$SIM" pty --echo --link "$DEV" --timeout-ms 60000 >"$TMPD/device.log" 2>&1 &
SIMPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" --socket "$SOCK" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/demo.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "exclusive"
[[node]]
type = "pty"
name = "console"
path = "$TTY"
[[node]]
type = "log"
name = "cap"
directory = "$LOGDIR"
filename = "console.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
write_mode = "never"
EOF
"${CC[@]}" load "$TMPD/demo.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }
bash "$WAIT" "\"$C\" --socket '$SOCK' --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || fail "usb0 never reached active"

# 1. INSPECT STATE — the graph is healthy and the write lock is free.
"${CC[@]}" --json state | jq -e \
  '.nodes[]|select(.name=="usb0")|.status=="active" and .lock.holder==null' >/dev/null \
  || fail "step 1: usb0 not active with a free lock"

# 2. LOCK A CHANNEL — the operator grabs the console's write floor.
"${CC[@]}" --json lock console | jq -e '.acquired==true and .held==true' >/dev/null \
  || fail "step 2: lock console did not acquire"
"${CC[@]}" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder=="console"' >/dev/null \
  || fail "step 2: state does not show console as the holder"

# 3. NEGATIVE CONTROL — a competing plain `send` is refused (exclusivity holds).
if "${CC[@]}" send usb0 --line "denied" --timeout-ms 300 2>"$TMPD/denied.err"; then
  fail "step 3: contended send should have failed but succeeded"
fi
grep -qi "lock" "$TMPD/denied.err" || { cat "$TMPD/denied.err"; fail "step 3: send error was not the locked error"; }

# 4. SEND A COMMAND — the operator escalates with the steal escape hatch and fires
#    a one-shot command atomically (acquire-write-release, taking the floor).
"${CC[@]}" --json send usb0 --line "reboot" --steal | jq -e '.delivered==true and .sent==7' >/dev/null \
  || fail "step 4: send --steal did not deliver 'reboot'"

# 5. VERIFY THE DEVICE RECEIVED IT — the echo device reflects "reboot\n" hostward
#    and the log captures it. The locked-out "denied" from step 3 left no trace.
bash "$WAIT" "[ \"\$(cat '$LOGDIR'/console.log 2>/dev/null | wc -c)\" -eq 7 ]" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "step 5: log never captured the 7-byte echo of 'reboot'"; }
want5=$(printf 'reboot\n' | sha256sum | awk '{print $1}')
[ "$(log_sha)" = "$want5" ] || fail "step 5: device did not receive exactly 'reboot' (or 'denied' leaked)"

# 6. ROTATE ITS LOG + VERIFY CONTINUITY — a stream split across a rotation loses
#    nothing: rotated + live files concatenate to the exact command transcript.
"${CC[@]}" --json send usb0 --line "status" >/dev/null || fail "step 6: send 'status' failed"
bash "$WAIT" "[ \"\$(cat '$LOGDIR'/console.log.* '$LOGDIR'/console.log 2>/dev/null | wc -c)\" -eq 14 ]" 5 0.05 \
  || fail "step 6: log did not reach 14 bytes after 'status'"
"${CC[@]}" rotate cap >/dev/null || fail "step 6: rotate failed"
bash "$WAIT" "test -e '$LOGDIR/console.log.000'" 5 0.05 || fail "step 6: rotation did not create console.log.000"
"${CC[@]}" --json send usb0 --line "ping" >/dev/null || fail "step 6: send 'ping' failed"
bash "$WAIT" "[ \"\$(cat '$LOGDIR'/console.log.* '$LOGDIR'/console.log 2>/dev/null | wc -c)\" -eq 19 ]" 5 0.05 \
  || fail "step 6: combined log did not reach 19 bytes after 'ping'"
want6=$(printf 'reboot\nstatus\nping\n' | sha256sum | awk '{print $1}')
[ "$(log_sha)" = "$want6" ] || fail "step 6: rotation lost/duplicated bytes (continuity broken)"
"${CC[@]}" --json state | jq -e '.nodes[]|select(.name=="cap")|.rotation==0' >/dev/null \
  || fail "step 6: rotation counter did not advance to 0"

# 7. UNLOCK — re-acquire and explicitly release, leaving the floor free.
"${CC[@]}" --json lock console | jq -e '.acquired==true' >/dev/null || fail "step 7: re-lock failed"
"${CC[@]}" --json unlock console | jq -e '.released==true' >/dev/null || fail "step 7: unlock did not release"
"${CC[@]}" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder==null' >/dev/null \
  || fail "step 7: lock still held after unlock"

"${CC[@]}" shutdown >/dev/null
echo '{"check":"phase8-agent-task","pass":true}'
