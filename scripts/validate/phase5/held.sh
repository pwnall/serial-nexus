#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 6): the held lock tells the truth.
# A demux codec holds the serial's write lock permanently (its edge is `held`),
# so a raw `send`/`lock` at the serial is refused. Stealing that lock stalls every
# channel — observed here as a channel's targetward `accepted` counter frozen while
# a durable stealer holds the lock, then resuming to completion once it releases
# (the codec re-acquires; commands are delayed, never dropped, §6).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase5-held\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p5h.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; TW="$TMPD/ttyW"
NBYTES=4096
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${DEVPID:-}" ] && kill "$DEVPID" 2>/dev/null
  [ -n "${WPID:-}" ] && kill "$WPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

mux() { "$C" --json state | jq -e ".nodes[]|select(.name==\"mux\")|$1" >/dev/null; }
usb0() { "$C" --json state | jq -e ".nodes[]|select(.name==\"usb0\")|$1" >/dev/null; }
accepted() { "$C" --json state | jq -r '.nodes[]|select(.name=="mux")|.channels.c0.accepted_targetward'; }

# The device: a sink that drains whatever the serial writes (the codec's frames).
"$SIM" pty --sink --bytes 1048576 --link "$DEV" --timeout-ms 40000 >"$TMPD/dev.json" 2>&1 &
DEVPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# serial usb0 (exclusive) with TWO origins: the demux codec (held) and a direct
# raw writer `rawpty` (on-demand) that can durably steal the lock. The codec's one
# channel c0 is free-for-all, so its writer flows without a channel lock.
{
  echo '[[node]]'; echo 'type = "serial"'; echo 'name = "usb0"'; echo "device = \"$DEV\""
  echo '[[node]]'; echo 'type = "codec"'; echo 'name = "mux"'; echo 'codec = "reference"'
  echo 'faces = "target"'; echo 'channels = ["c0"]'; echo 'arbitration = "free-for-all"'
  echo '[[node]]'; echo 'type = "pty"'; echo 'name = "con-c0"'; echo "path = \"$TMPD/tty-c0\""
  echo '[[node]]'; echo 'type = "pty"'; echo 'name = "rawpty"'; echo "path = \"$TW\""
  echo '[[edge]]'; echo 'a = "usb0"'; echo 'b = "mux"'; echo 'write_mode = "held"'
  echo '[[edge]]'; echo 'a = "usb0"'; echo 'b = "rawpty"'
  echo '[[edge]]'; echo 'a = "mux/c0"'; echo 'b = "con-c0"'
} > "$TMPD/g.toml"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# The demux holds the serial lock (§6): holder is the codec's mux origin.
HOLDER_MUX="\"$C\" --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.lock.holder==\"mux\"'"
bash "$WAIT" "$HOLDER_MUX" 5 0.05 || { cat "$TMPD/daemon.log"; "$C" --json state; fail "demux did not hold the serial lock"; }

# Raw contention at the serial is refused while the demux holds it (§6).
if "$C" lock rawpty 2>"$TMPD/lock.err"; then fail "plain lock rawpty should be refused while the demux holds usb0"; fi
grep -qi locked "$TMPD/lock.err" || { cat "$TMPD/lock.err"; fail "lock rawpty refused, but not with a locked error"; }
if "$C" send usb0 --line "raw" --timeout-ms 500 2>"$TMPD/send.err"; then fail "plain send usb0 should be refused while the demux holds it"; fi
grep -qi locked "$TMPD/send.err" || { cat "$TMPD/send.err"; fail "send usb0 refused, but not with a locked error"; }

# Steal the serial lock durably for the raw writer — the demux is ousted (§6).
"$C" --json lock rawpty --steal | jq -e '.acquired==true' >/dev/null || fail "lock rawpty --steal failed"
usb0 '.lock.holder=="rawpty"' || fail "rawpty did not take the lock"
usb0 '.lock.last_steal.from=="mux" and .lock.last_steal.by=="rawpty"' || { "$C" --json state; fail "steal not recorded (from mux by rawpty)"; }

# With the demux ousted, a channel writer's bytes park in the codec: accepted must
# stay frozen at 0 (the §6 stall). Send a bounded burst and hold it open.
"$SIM" client --path "$TMPD/tty-c0" --send "seeded:$NBYTES" --seed 3 --hold-ms 30000 --timeout-ms 40000 >"$TMPD/w.json" 2>&1 &
WPID=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"con-c0\")|.client_present==true'" 8 0.05 \
  || { cat "$TMPD/daemon.log"; fail "channel writer never became present"; }
sleep 0.5
[ "$(accepted)" = "0" ] || { "$C" --json state; fail "accepted advanced while the lock was stolen (the stall was not observed)"; }

# Release the theft: the demux re-acquires (FIFO) and forwards the parked bytes to
# completion — delayed, never dropped. accepted resumes to the full burst.
"$C" unlock rawpty >/dev/null || fail "unlock rawpty failed"
bash "$WAIT" "$HOLDER_MUX" 5 0.05 || { "$C" --json state; fail "demux did not re-acquire the lock after the theft"; }
bash "$WAIT" "[ \"\$($C --json state | jq -r '.nodes[]|select(.name==\"mux\")|.channels.c0.accepted_targetward')\" = \"$NBYTES\" ]" 8 0.1 \
  || { "$C" --json state; fail "accepted did not resume to $NBYTES after the theft ended (data wedged or lost)"; }

"$C" shutdown >/dev/null
echo '{"check":"phase5-held","pass":true}'
