#!/usr/bin/env bash
# Phase 4 validation (arbitration §6, plan item 5): the atomic `send` verb.
# `send` names the ENDPOINT; the CLI is a transient origin that acquires the write
# lock (with a timeout), writes one line, and releases — one daemon-side operation.
# While another origin holds the lock, a plain `send` fails with the locked error
# at its deadline; `send --steal` takes the lock and delivers the line exactly once.
#
# No hardware (§15.17): the "device" is a nexus-sim sink sized to the stolen line,
# so "delivered exactly once" is a byte-exact count + checksum, not a judgement.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-send\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4s.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
TA="$TMPD/ttyA"
LINE="steal-me"                       # delivered as "steal-me\n"
EXPLEN=$(( ${#LINE} + 1 ))
EXPSHA=$(printf '%s\n' "$LINE" | sha256sum | cut -d' ' -f1)
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SINKPID:-}" ] && kill "$SINKPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The device records exactly the stolen line and nothing else.
"$SIM" pty --sink --bytes "$EXPLEN" --link "$DEV" --timeout-ms 20000 >"$TMPD/sink.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# One on-demand PTY holder plus the CLI `send` origin fan into one exclusive serial.
cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "pty"
name = "ptya"
path = "$TA"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[edge]]
a = "usb0"
b = "ptya"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# ptya grabs the lock, so the endpoint is held by another origin.
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "lock ptya failed"; }

# A plain `send` joins the queue with its deadline and fails with the locked error
# when the deadline elapses (§6). Nothing is delivered.
if "$C" send usb0 --line "should-not-arrive" --timeout-ms 400 2>"$TMPD/send.err"; then
  cat "$TMPD/daemon.log"; fail "plain send should have failed while ptya holds the lock"
fi
grep -qi 'lock' "$TMPD/send.err" || { cat "$TMPD/send.err"; fail "send failed, but not with a locked error"; }
# The queue is intact after the deadline (the transient origin was cleaned up).
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.waiters==[]' >/dev/null \
  || fail "waiter queue not empty after a timed-out send"
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder=="ptya"' >/dev/null \
  || fail "ptya should still hold the lock after the failed send"

# `send --steal` takes the lock and delivers the line exactly once.
"$C" --json send usb0 --line "$LINE" --steal | jq -e ".delivered==true and .sent==$EXPLEN" >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "send --steal did not report delivery"; }

# The device received exactly the stolen line, once — byte-exact.
wait "$SINKPID" 2>/dev/null || true; SINKPID=
RECV=$(jq -r '.received // -1' "$TMPD/sink.json")
SHA=$(jq -r '.sha256 // ""' "$TMPD/sink.json")
[ "$RECV" = "$EXPLEN" ] || { cat "$TMPD/daemon.log" "$TMPD/sink.json"; fail "device received $RECV bytes, expected $EXPLEN (delivered != once)"; }
[ "$SHA" = "$EXPSHA" ] || { cat "$TMPD/sink.json"; fail "device checksum != the stolen line (wrong bytes delivered)"; }

# After the atomic send, the transient origin released and unregistered: holder is
# clear and no phantom "send" origin lingers.
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.lock.holder==null'" 3 0.05 \
  || { cat "$TMPD/daemon.log"; fail "send did not release the lock"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|(.lock.origins|map(.origin))==["ptya"]' >/dev/null \
  || fail "a transient send origin lingered on the endpoint"

"$C" shutdown >/dev/null
echo '{"check":"phase4-send","pass":true}'
