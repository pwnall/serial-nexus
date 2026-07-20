#!/usr/bin/env bash
# Phase 4 validation (arbitration, §6): a `write = held` origin acquires the lock
# on attach and holds it INDEFINITELY — a client detach must NOT release it (only
# node removal does). This is the demux codec's permanent hold in miniature; here
# a PTY edge stands in for it. Regression guard for detach-release wrongly firing
# on a held holder.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-held\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4h.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; TH="$TMPD/ttyH"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${DEVPID:-}" ] && kill "$DEVPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$SIM" pty --echo --link "$DEV" --timeout-ms 30000 >"$TMPD/dev.log" 2>&1 &
DEVPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "pty"
name = "ptyh"
path = "$TH"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[edge]]
a = "usb0"
b = "ptyh"
write_mode = "held"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

usb0() { "$C" --json state | jq -e ".nodes[]|select(.name==\"usb0\")|$1" >/dev/null; }

# A held origin acquires the lock on attach (register), with no explicit lock.
usb0 '.lock.holder=="ptyh"' || { cat "$TMPD/daemon.log"; fail "held origin did not acquire the lock on attach"; }
usb0 '.lock.origins[]|select(.origin=="ptyh")|.write_mode=="held" and .holds_lock==true' \
  || fail "held origin not reported as holding the lock"

# A client attaches, writes, and detaches. The held lock must survive the detach.
"$SIM" client --path "$TH" --send seeded:256 --seed 5 --timeout-ms 8000 >/dev/null 2>&1 || true
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"ptyh\")|.client_present==false'" 3 0.05 \
  || { cat "$TMPD/daemon.log"; fail "client never detached"; }
usb0 '.lock.holder=="ptyh"' \
  || { cat "$TMPD/daemon.log"; fail "held origin released its lock on client detach (must be held indefinitely, §6)"; }

"$C" shutdown >/dev/null
echo '{"check":"phase4-held","pass":true}'
