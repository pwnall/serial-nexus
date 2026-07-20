#!/usr/bin/env bash
# Phase 2 validation (data-plane slice): real bytes flow client↔daemon↔device
# through a serial→PTY graph, presence gating tracks the client, and the §7.2
# baseline termios is confirmed end to end (design §5, §7.1, §7.2).
#
# Topology (no hardware, per the no-target doctrine §15.17):
#
#   nexus-sim client  ─▶  [ pty "console" ]──edge──[ serial "usb0" ]  ─▶  nexus-sim pty --echo
#      (operator)            $TTY symlink              device=$DEV            (the "device")
#
# The device echoes; a 64 KiB seeded round-trip that comes back byte-identical
# proves client→PTY→serial→device→serial→PTY→client with nothing lost.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase2-data-path\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"

# A SHORT socket dir — Unix socket paths are bounded by SUN_LEN (~108 bytes).
TMPD=$(mktemp -d /tmp/snx-p2d.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/device"    # the device pts (serial node opens this)
TTY="$TMPD/console"   # the daemon's PTY (clients open this)
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SIMPID:-}" ] && kill "$SIMPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The "device": a PTY that echoes. Long idle timeout so it stays up across steps.
"$SIM" pty --echo --link "$DEV" --timeout-ms 60000 >"$TMPD/device.log" 2>&1 &
SIMPID=$!
bash "$REPO_ROOT/scripts/lib/wait-for.sh" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

# The daemon.
"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$REPO_ROOT/scripts/lib/wait-for.sh" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# serial(usb0, host) → pty(console, target).
cat > "$TMPD/demo.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/demo.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Both nodes active: the device is present, so the serial node opened it.
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|.status=="active"' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "usb0 not active (serial did not open the device)"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="console")|.status=="active"' >/dev/null \
  || fail "console not active"
[ -L "$TTY" ] || fail "pty symlink not created"

# Baseline termios, confirmed from the client's side of the slave (§7.2):
# raw (no OPOST, no ICANON), echo off, EXTPROC on. Observe before any client
# has disturbed it.
"$SIM" client --path "$TTY" --report-termios \
  | jq -e '.echo==false and .icanon==false and .extproc==true and .opost==false' >/dev/null \
  || fail "baseline termios wrong (want raw + echo-off + EXTPROC)"

# Presence starts false — no client is attached.
"$C" --json state | jq -e '.nodes[]|select(.name=="console")|.client_present==false' >/dev/null \
  || fail "client_present should be false with no client"

# THE DATA PATH: 64 KiB seeded out, the device echoes it back byte-identical.
"$SIM" client --path "$TTY" --send seeded:64KiB --expect echo --seed 42 --timeout-ms 15000 \
  | jq -e '.pass==true and .sent==65536 and .received==65536' >/dev/null \
  || { cat "$TMPD/daemon.log" "$TMPD/device.log"; fail "64KiB echo round-trip failed (bytes lost/mangled)"; }

# Presence transitions: hold the slave open, watch it go true, then false on close.
( exec 3<>"$TTY"; sleep 1 ) &
HOLDER=$!
bash "$REPO_ROOT/scripts/lib/wait-for.sh" \
  "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"console\")|.client_present==true'" 3 0.05 \
  || fail "client_present never went true while a client held the slave"
wait "$HOLDER"
bash "$REPO_ROOT/scripts/lib/wait-for.sh" \
  "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"console\")|.client_present==false'" 2 0.05 \
  || fail "client_present never returned false within a second of client exit"

"$C" shutdown >/dev/null
echo '{"check":"phase2-data-path","pass":true}'
