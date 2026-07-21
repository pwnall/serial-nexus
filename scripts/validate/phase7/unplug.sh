#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 1): unplug keeps clients alive.
# A serial node (addressed by usb identity, resolved through a fixture by-id/sysfs
# tree, §12) fans out to a PTY with an attached client. Killing the serial device
# and removing its fixture symlink faults-and-waits the serial node (§7.1) WITHIN
# the poll interval, while the PTY's client stays attached — the unplug of one
# boundary never disturbs another (no HUP to the consumer).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-unplug\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
source "$REPO_ROOT/scripts/lib/fixture-tree.sh"

TMPD=$(mktemp -d /tmp/snx-p7u.XXXXXX) || fail "mktemp"
ROOT="$TMPD/root"; SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
mkdir -p "$ROOT/dev"
cleanup() { for p in "${DEV:-}" "${CLI:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

# Fixture: an FTDI-like device at usb:0403:6001:UNPLUG1:00 behind ttyUSB0.
make_usb_iface "$ROOT" "1-1" "0403" "6001" "UNPLUG1" "ttyUSB0" "00" "usb-FTDI_UNPLUG1-if00"
"$SIM" pty --echo --link "$ROOT/dev/ttyUSB0" --hold-ms 60000 >"$TMPD/dev.json" 2>&1 & DEV=$!
bash "$WAIT" "test -e '$ROOT/dev/ttyUSB0'" 5 0.05 || fail "device never appeared"

"$D" --socket "$SOCK" --dev-root "$ROOT" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "usb:0403:6001:UNPLUG1:00"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con"
path = "$TMPD/con"
[[edge]]
a = "usb0"
b = "con"
EOF
$CC load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }
# Serial resolves the identity to the fixture device and comes up active.
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || fail "serial never became active (identity did not resolve through the fixture)"

# Attach a client to the PTY and confirm presence (--set-baud reaches the hold).
"$SIM" client --path "$TMPD/con" --set-baud 115200 --hold-ms 60000 >"$TMPD/cli.json" 2>&1 & CLI=$!
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"con\")|.client_present==true'" 5 0.1 \
  || fail "PTY client never attached"

# ---- Unplug: kill the device sim and remove its fixture entry ----------------
kill "$DEV" 2>/dev/null; wait "$DEV" 2>/dev/null; DEV=
unplug_usb "$ROOT" "ttyUSB0" "usb-FTDI_UNPLUG1-if00"

# The serial node reaches `waiting` within the poll interval...
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"waiting\"'" 5 0.1 \
  || { $CC --json state | jq -c '.nodes[]|{name,status,reason}'; fail "serial did not reach waiting after unplug"; }

# ...and the PTY's client is undisturbed (fd still open, no HUP propagated).
$CC --json state | jq -e '.nodes[]|select(.name=="con")|.client_present==true' >/dev/null \
  || fail "PTY client HUP'd by an unrelated serial unplug (should be undisturbed)"

echo '{"check":"phase7-unplug","pass":true}'
