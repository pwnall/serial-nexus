#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 4): exec-codec crash containment.
# An exec codec (a child process speaking the envelope on stdin/stdout) sits
# between a serial and a channel PTY, echoing a full round-trip. `kill -9` of the
# child mid-life faults the node and restarts it within the backoff (observed via
# restart_count); a fresh round-trip afterward has clean checksums (resumed) — and
# a concurrent echo on an unrelated serial keeps passing throughout (the data plane
# never wedged). No hardware (§15.17): both "devices" are nexus-sim echo PTYs.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase5-exec-crash\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

command -v python3 >/dev/null 2>&1 || { echo '{"check":"phase5-exec-crash","pass":false,"reason":"python3 not found"}'; exit 1; }
cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p5e.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV0="$TMPD/dev0"; DEV1="$TMPD/dev1"
# A per-test copy of the child so `pkill -f` targets only this run's child.
CHILD="$TMPD/passthrough-codec.py"
cp "$REPO_ROOT/tests/ext-codec/passthrough-codec.py" "$CHILD"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${E0:-}" ] && kill "$E0" 2>/dev/null
  [ -n "${E1:-}" ] && kill "$E1" 2>/dev/null
  pkill -9 -f "$CHILD" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# Two echo "devices".
"$SIM" pty --echo --link "$DEV0" --timeout-ms 60000 >"$TMPD/e0.json" 2>&1 & E0=$!
"$SIM" pty --echo --link "$DEV1" --timeout-ms 60000 >"$TMPD/e1.json" 2>&1 & E1=$!
bash "$WAIT" "test -e '$DEV0' && test -e '$DEV1'" 5 0.05 || fail "devices never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# usb0 → exec codec (held) → con-c0 (the exec path); usb1 → con2 (the unrelated
# echo probe). The exec codec's channel is free-for-all so the client writes freely.
{
  echo '[[node]]'; echo 'type = "serial"'; echo 'name = "usb0"'; echo "device = \"$DEV0\""
  echo '[[node]]'; echo 'type = "codec"'; echo 'name = "mux"'; echo 'codec = "exec"'
  echo 'faces = "target"'; echo 'channels = ["c0"]'; echo 'arbitration = "free-for-all"'
  echo "attributes = { argv = [\"python3\", \"$CHILD\", \"c0\"], restart_backoff_ms = 150 }"
  echo '[[node]]'; echo 'type = "pty"'; echo 'name = "con-c0"'; echo "path = \"$TMPD/tty-c0\""
  echo '[[node]]'; echo 'type = "serial"'; echo 'name = "usb1"'; echo "device = \"$DEV1\""; echo 'arbitration = "free-for-all"'
  echo '[[node]]'; echo 'type = "pty"'; echo 'name = "con2"'; echo "path = \"$TMPD/tty2\""
  echo '[[edge]]'; echo 'a = "usb0"'; echo 'b = "mux"'; echo 'write_mode = "held"'
  echo '[[edge]]'; echo 'a = "mux/c0"'; echo 'b = "con-c0"'
  echo '[[edge]]'; echo 'a = "usb1"'; echo 'b = "con2"'
} > "$TMPD/g.toml"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# The exec codec starts a child and reports active.
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"mux\")|.status==\"active\" and .codec==\"exec\"'" 8 0.1 \
  || { cat "$TMPD/daemon.log"; "$C" --json state; fail "exec codec never became active"; }

# A round-trip echo through the exec codec (client → c0 → child → serial → device
# → back) must checksum clean. 256 KiB fills the child's 64 KiB pipes many times
# over, so it also proves the pump does not deadlock under sustained flow (the two
# directions are polled concurrently, not coupled in one select branch).
roundtrip() { "$SIM" client --path "$TMPD/tty-c0" --send "seeded:256KiB" --seed "$1" --expect echo --timeout-ms 30000; }
probe() { "$SIM" client --path "$TMPD/tty2" --send "seeded:$1" --seed "$1" --expect echo --timeout-ms 20000; }

probe 4096 | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "unrelated echo probe failed before the crash"; }
roundtrip 11 | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "exec-codec round-trip failed before the crash"; }

# Kill the child mid-life; the node must fault and restart within the backoff.
BEFORE=$("$C" --json state | jq -r '.nodes[]|select(.name=="mux")|.restart_count')
pkill -9 -f "$CHILD" || fail "could not find the exec child to kill"
bash "$WAIT" "[ \"\$($C --json state | jq -r '.nodes[]|select(.name==\"mux\")|.restart_count')\" -gt \"$BEFORE\" ]" 8 0.1 \
  || { cat "$TMPD/daemon.log"; "$C" --json state; fail "restart_count did not increase after kill -9"; }
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"mux\")|.status==\"active\"'" 8 0.1 \
  || { cat "$TMPD/daemon.log"; fail "exec codec did not return to active after restart"; }

# The unrelated echo probe still passes — the data plane never wedged.
probe 5121 | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "unrelated echo probe failed after the crash (data plane wedged)"; }

# A fresh round-trip through the restarted child has clean checksums (resumed).
roundtrip 22 | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; "$C" --json state; fail "exec-codec round-trip failed after restart (did not resume clean)"; }

AFTER=$("$C" --json state | jq -r '.nodes[]|select(.name=="mux")|.restart_count')
"$C" shutdown >/dev/null
echo "{\"check\":\"phase5-exec-crash\",\"pass\":true,\"restart_count\":$AFTER}"
