#!/usr/bin/env bash
# Phase 3 validation (log fault isolation, §5/§7.3): a log node whose file is on
# a full disk faults with an ENOSPC reason, while the port and its other
# consumers keep flowing — loss is faulted and isolated, never a wedged data
# plane. Uses /dev/full (always ENOSPC on write); no privilege required.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-log-enospc\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }
skip() { echo "{\"check\":\"phase3-log-enospc\",\"pass\":true,\"skipped\":\"$*\"}"; exit 0; }

[ -w /dev/full ] || skip "no writable /dev/full on this system"

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3f.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT
nstate() { "$C" --json state | jq -r ".nodes[]|select(.name==\"$1\")|.$2"; }

# Point the log's file at /dev/full via a symlink in a small dir (so the §7.3
# rotation-counter directory scan stays cheap).
ln -s /dev/full "$TMPD/full"

DEV="$TMPD/dev"
"$SIM" pty --echo --link "$DEV" --timeout-ms 60000 >"$TMPD/dev.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

TTY="$TMPD/console"
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "$DEV"
[[node]]
type = "log"
name = "diskfull"
directory = "$TMPD"
filename = "full"
overflow = "fault"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "diskfull"
EOF
"$C" load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Drive hostward bytes (client sends -> device echoes -> reaches the log), which
# forces the log's write to /dev/full and, under overflow=fault, faults the node.
"$SIM" client --path "$TTY" --send seeded:8KiB --expect echo --seed 1 --timeout-ms 15000 \
  | jq -e '.pass==true' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "first echo probe failed"; }

bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"diskfull\")|.status==\"faulted\"'" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "log node did not fault on ENOSPC (status=$(nstate diskfull status))"; }
REASON=$(nstate diskfull reason)
case "$REASON" in
  *write*|*space*|*"os error 28"*) : ;;
  *) fail "fault reason does not mention the write failure: $REASON" ;;
esac

# The data plane is not wedged: the port and its live PTY consumer keep flowing.
"$SIM" client --path "$TTY" --send seeded:8KiB --expect echo --seed 2 --timeout-ms 15000 \
  | jq -e '.pass==true and .received==8192' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "echo probe failed after the log faulted (data plane wedged)"; }
[ "$(nstate console status)" = "active" ] || fail "console should stay active while the log is faulted"

"$C" shutdown >/dev/null
echo '{"check":"phase3-log-enospc","pass":true}'
