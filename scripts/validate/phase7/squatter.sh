#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 3): squatters are refused. After an
# unplug, a DIFFERENT adapter appearing on the same /dev path (a different usb
# identity) must NOT be adopted — the node stays waiting and never opens the
# squatter, so the squatter receives zero bytes. Wrong-device adoption is
# impossible by construction: the resolver returns a path only for the SAME
# identity (§7.1/§12).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-squatter\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"; C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"; WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
source "$REPO_ROOT/scripts/lib/fixture-tree.sh"

TMPD=$(mktemp -d /tmp/snx-p7s.XXXXXX) || fail "mktemp"
ROOT="$TMPD/root"; SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
mkdir -p "$ROOT/dev"
cleanup() { for p in "${DEV:-}" "${SQ:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

# Ours: usb:0403:6001:OURS:00.
make_usb_iface "$ROOT" "1-1" "0403" "6001" "OURS" "ttyUSB0" "00" "usb-FTDI_OURS-if00"
"$SIM" pty --echo --link "$ROOT/dev/ttyUSB0" --hold-ms 60000 >"$TMPD/dev.json" 2>&1 & DEV=$!
bash "$WAIT" "test -e '$ROOT/dev/ttyUSB0'" 5 0.05 || fail "device never appeared"

"$D" --socket "$SOCK" --dev-root "$ROOT" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "usb:0403:6001:OURS:00"
arbitration = "free-for-all"
EOF
$CC load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load"; }
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || fail "serial never active"

# ---- Unplug ours, then a squatter (different identity) takes the /dev path ----
kill "$DEV" 2>/dev/null; wait "$DEV" 2>/dev/null; DEV=
unplug_usb "$ROOT" "ttyUSB0" "usb-FTDI_OURS-if00"
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"waiting\"'" 5 0.1 \
  || fail "serial did not fault-and-wait"

# A DIFFERENT adapter squats the same dev name (usb:...:SQUATTER:00). It is a sink
# that counts bytes the daemon writes to it — which must stay zero.
make_usb_iface "$ROOT" "1-1" "0403" "6001" "SQUATTER" "ttyUSB0" "00" "usb-FTDI_SQUATTER-if00"
"$SIM" pty --sink --bytes 4096 --link "$ROOT/dev/ttyUSB0" --timeout-ms 4000 >"$TMPD/sq.json" 2>&1 & SQ=$!
bash "$WAIT" "test -e '$ROOT/dev/ttyUSB0'" 5 0.05 || fail "squatter device never appeared"

# Push targetward at the (waiting) node — it must park, never reach the squatter.
for i in 1 2 3 4 5; do $CC send usb0 --line "SHOULD-NOT-REACH-SQUATTER-$i" >/dev/null 2>&1 || true; done

# Give the reconnect poll several cycles; the node must NOT adopt the squatter.
sleep 3
$CC --json state | jq -e '.nodes[]|select(.name=="usb0")|.status=="waiting"' >/dev/null \
  || { $CC --json state | jq -c '.nodes[]|{name,status,reason}'; fail "node adopted a squatter (wrong-device adoption)"; }

# The squatter received nothing (the daemon never opened it).
wait "$SQ" 2>/dev/null; SQ=
RECV=$(jq -r '.received // 0' "$TMPD/sq.json" 2>/dev/null || echo 0)
[ "${RECV:-0}" = "0" ] || fail "squatter received $RECV bytes (should be 0 — never opened)"

echo '{"check":"phase7-squatter","pass":true}'
