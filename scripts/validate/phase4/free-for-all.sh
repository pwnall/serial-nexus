#!/usr/bin/env bash
# Phase 4 validation (arbitration opt-out, §6): a `free-for-all` endpoint has no
# lock, so every writer's bytes are read targetward with no acquisition. Two PTYs
# on one free-for-all serial endpoint both write concurrently and BOTH reach the
# device — the distinguishing behavior versus the exclusive default, under which
# neither would reach it without a lock.
#
# No hardware (§15.17): the "device" is a nexus-sim sink, so "both got through" is
# an exact byte count.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-free-for-all\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4f.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; TA="$TMPD/ttyA"; TB="$TMPD/ttyB"
N=16384; TOTAL=$((2 * N))   # each writer sends N; the device must see 2N
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SINKPID:-}" ] && kill "$SINKPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The device records exactly TOTAL bytes reaching hardware.
"$SIM" pty --sink --bytes "$TOTAL" --link "$DEV" --timeout-ms 20000 >"$TMPD/sink.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "pty"
name = "ptya"
path = "$TA"
[[node]]
type = "pty"
name = "ptyb"
path = "$TB"
[[node]]
type = "serial"
name = "usb0"
arbitration = "free-for-all"
device = "$DEV"
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# State reports the endpoint as free-for-all with no holder, and every origin as a
# writer that already may_write (no acquisition needed).
usb0() { "$C" --json state | jq -e ".nodes[]|select(.name==\"usb0\")|$1" >/dev/null; }
usb0 '.lock.arbitration=="free-for-all" and .lock.holder==null' || fail "endpoint not free-for-all/holderless"

# Both clients write concurrently; with no lock, BOTH streams are read targetward.
"$SIM" client --path "$TA" --send "seeded:$N" --seed 1 --timeout-ms 15000 >/dev/null 2>&1 &
PA=$!
"$SIM" client --path "$TB" --send "seeded:$N" --seed 2 --timeout-ms 15000 >/dev/null 2>&1 &
PB=$!

# The device must receive exactly 2N bytes — both writers got through. (Under the
# exclusive default with no lock, it would receive 0.)
wait "$SINKPID" 2>/dev/null || true; SINKPID=
wait "$PA" "$PB" 2>/dev/null || true
RECV=$(jq -r '.received // -1' "$TMPD/sink.json")
[ "$RECV" = "$TOTAL" ] || { cat "$TMPD/daemon.log" "$TMPD/sink.json"; fail "device received $RECV bytes, expected $TOTAL (a free-for-all writer was blocked)"; }

"$C" shutdown >/dev/null
echo '{"check":"phase4-free-for-all","pass":true}'
