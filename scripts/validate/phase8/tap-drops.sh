#!/usr/bin/env bash
# Web console track validation (design §5/§17 / plan §11.1): a slow tap costs only
# itself. An unread ("paused browser tab") tap's bounded queue fills and drops with
# a counter, while a co-attached log consumer of the same endpoint stays byte-exact
# and complete — §5's "a slow spy costs itself data, never its neighbors", in its
# dynamic-attachment form.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-tap-drops\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8td.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
GO="$TMPD/go"
N=8388608            # 8 MiB — comfortably over the tap queue + socket buffer bound
SEED=11
mkdir -p "$TMPD/logs"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SRCPID:-}" ] && kill "$SRCPID" 2>/dev/null
  [ -n "${TAPPID:-}" ] && kill "$TAPPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$SIM" pty --source --bytes "$N" --seed "$SEED" --wait-file "$GO" \
  --link "$DEV" --hold-ms 2000 --timeout-ms 60000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "free-for-all"
hostward_buffer = 16384
[[node]]
type = "log"
name = "logx"
directory = "$TMPD/logs"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# A paused tab: open the tap, then stop reading for the stall window so its bounded
# queue fills and the hub drops-with-counter.
timeout 30 "$C" tap usb0 --stall-ms 10000 >/dev/null 2>"$TMPD/tap.err" &
TAPPID=$!
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==1'" 5 0.05 \
  || { cat "$TMPD/daemon.log" "$TMPD/tap.err"; fail "stalled tap did not register"; }

# Release the source: 8 MiB flows to the log (fast, byte-exact) and the stalled tap
# (queue fills → drops).
touch "$GO"

# While the stalled tap is still open, its own drop counter climbs above zero.
bash "$WAIT" "\"$C\" --json state | jq -e '.taps[0].dropped > 0'" 15 0.1 \
  || { "$C" --json state; cat "$TMPD/daemon.log"; fail "the unread tap recorded no drops"; }
DROPPED=$("$C" --json state | jq -r '.taps[0].dropped // 0')

# The co-attached log stays byte-exact and complete despite the tap dropping.
wait "$SRCPID" 2>/dev/null; SRCPID=
bash "$WAIT" "[ \"\$(stat -c %s '$TMPD/logs/serial.log' 2>/dev/null || echo 0)\" = '$N' ]" 30 0.2 \
  || { cat "$TMPD/daemon.log"; fail "log did not reach $N bytes (a slow tap starved a neighbor)"; }
SRC_SHA=$(jq -r '.sha256 // ""' "$TMPD/src.json")
LOG_SHA=$(sha256sum "$TMPD/logs/serial.log" | cut -d' ' -f1)
[ -n "$SRC_SHA" ] || { cat "$TMPD/src.json"; fail "source produced no verdict"; }
[ "$LOG_SHA" = "$SRC_SHA" ] || fail "log checksum != source (the slow tap corrupted a neighbor's stream)"

# Clean up the stalled tap.
kill "$TAPPID" 2>/dev/null; wait "$TAPPID" 2>/dev/null; TAPPID=

"$C" shutdown >/dev/null
echo "{\"check\":\"phase8-tap-drops\",\"pass\":true,\"tap_dropped\":$DROPPED}"
