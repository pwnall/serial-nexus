#!/usr/bin/env bash
# Phase 3 validation (subscribe + client-termios, §10/§7.2): the `subscribe`
# stream carries node status and counter snapshots, and a client changing its
# termios surfaces in state. No hardware (§15.17); devices are nexus-sim PTYs.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-subscribe\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3s.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

start_daemon() { "$D" >>"$TMPD/daemon.log" 2>&1 & DPID=$!; bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }; }
node_of() { jq -es "any(.[] | .params.nodes[]? | select(.name==\"$1\"); $2)" "$3"; }

start_daemon

# ---- Check 1: the stream carries node status and counter snapshots ----------
DEV1="$TMPD/dev1"
"$SIM" pty --source --bytes 256KiB --seed 7 --link "$DEV1" >"$TMPD/src1.json" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV1'" 5 0.05 || fail "dev1 never appeared"

cat > "$TMPD/c1.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TMPD/console1"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "$DEV1"
[[node]]
type = "log"
name = "cap"
directory = "$TMPD"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
EOF
"$C" load "$TMPD/c1.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c1 failed"; }
# Data has flowed and the no-client PTY has discarded some (a counter to observe).
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"console\")|.discarded_no_client>0'" 10 0.1 \
  || fail "no discard accrued to observe"

timeout 8 "$C" subscribe --count 4 >"$TMPD/sub1.json" || true
[ -s "$TMPD/sub1.json" ] || { cat "$TMPD/daemon.log"; fail "subscribe produced no notifications"; }
# Every line is a state notification.
jq -es 'all(.[]; .method=="state" and (.params.nodes|type=="array"))' "$TMPD/sub1.json" >/dev/null \
  || fail "subscribe emitted a non-state or malformed notification"
# Status is streamed...
node_of usb0 '.status=="active"' "$TMPD/sub1.json" >/dev/null || fail "usb0 active status not in stream"
# ...and counters are streamed (the no-client discard the source produced).
node_of console '(.discarded_no_client // 0) > 0' "$TMPD/sub1.json" >/dev/null \
  || fail "console discard counter not in the subscribe stream"
# ...including the log node's dropped_bytes counter.
node_of cap '(.dropped_bytes|type)=="number"' "$TMPD/sub1.json" >/dev/null \
  || fail "log dropped_bytes counter not in the subscribe stream"
"$C" teardown >/dev/null || fail "teardown after c1 failed"

# ---- Check 2: a client's termios change surfaces in state (§7.2) -------------
DEV2="$TMPD/dev2"
"$SIM" pty --echo --link "$DEV2" --timeout-ms 60000 >"$TMPD/dev2.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV2'" 5 0.05 || fail "dev2 never appeared"

TTY2="$TMPD/console2"
cat > "$TMPD/c2.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY2"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "$DEV2"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/c2.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c2 failed"; }

# Subscribe in the background, then attach a client that sets a distinctive baud
# and holds the slave open long enough for a snapshot to capture it.
( timeout 6 "$C" subscribe --count 25 >"$TMPD/sub2.json" ) & SUBPID=$!
# Wait for the subscription to register before the client attaches — bounded, not
# a bare sleep (plan §3). The daemon's periodic state snapshot no-ops until a
# subscriber exists, so the first bytes landing in sub2.json prove it is live.
bash "$WAIT" "test -s '$TMPD/sub2.json'" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "subscription never produced its first snapshot"; }
"$SIM" client --path "$TTY2" --set-baud 9600 --hold-ms 1800 --seed 1 --timeout-ms 15000 \
  | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "termios-setting client failed"; }
wait "$SUBPID" 2>/dev/null || true

# A snapshot taken while the client was present must report its baud, and while
# it was attached, client_present must have been observed true.
node_of console '.client_termios.baud=="B9600"' "$TMPD/sub2.json" >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "client_termios baud change (B9600) never surfaced in the stream"; }
node_of console '.client_present==true' "$TMPD/sub2.json" >/dev/null \
  || fail "client_present never observed true in the stream"

# Last-close reset (§7.2): once the B9600 client departs, the daemon re-asserts
# the baseline termios and forgets the client's settings — on whichever path
# (POLLHUP or the read-path EOF/EIO) observes the close first. Verify the invariant
# directly: presence returns false, state clears client_termios to null, and a
# fresh probe reads the baseline (EXTPROC on, echo off) rather than the departed
# client's B9600. (Previously untested; a bare last-close reset that fired only on
# POLLHUP would pass the stream checks above but fail here.)
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"console\")|.client_present==false'" 3 0.05 \
  || { cat "$TMPD/daemon.log"; fail "client_present never returned false after the B9600 client exited"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="console")|.client_termios==null' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "client_termios not cleared to null on last close"; }
"$SIM" client --path "$TTY2" --report-termios \
  | jq -e '.echo==false and .extproc==true' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "baseline termios not restored after the B9600 client closed"; }

"$C" shutdown >/dev/null
echo '{"check":"phase3-subscribe","pass":true}'
