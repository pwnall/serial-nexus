#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 4): the §12 device-identity matrix on
# fixture by-id/sysfs trees (no hardware). Covers: an FT4232-style multi-interface
# device yielding four independently bound nodes with distinct resolved paths; a
# no-serial clone degrading to a by-path identity with the documented instability
# warning in the add-time RPC result; path-form add with the device absent failing
# as designed; and identity-form add succeeding into waiting while absent.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-matrix\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"; C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"; WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
source "$REPO_ROOT/scripts/lib/fixture-tree.sh"

TMPD=$(mktemp -d /tmp/snx-p7m.XXXXXX) || fail "mktemp"
ROOT="$TMPD/root"; SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
mkdir -p "$ROOT/dev"
PIDS=()
cleanup() { for p in "${PIDS[@]:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

# FT4232-style device: one usb device (serial FT4232) with four interfaces →
# ttyUSB0..3, each a present sim device.
for i in 0 1 2 3; do
  make_usb_iface "$ROOT" "2-1" "0403" "6011" "FT4232" "ttyUSB$i" "0$i" "usb-FTDI_FT4232-if0$i"
  "$SIM" pty --echo --link "$ROOT/dev/ttyUSB$i" --hold-ms 120000 >"$TMPD/ft$i.json" 2>&1 & PIDS+=($!)
  bash "$WAIT" "test -e '$ROOT/dev/ttyUSB$i'" 5 0.05 || fail "ft device $i never appeared"
done
# No-serial clone (a cheap CH340) with a by-path entry.
make_usb_iface "$ROOT" "3-1" "1a86" "7523" "" "ttyUSB9" "00" "usb-1a86_CH340-if00"
make_bypath "$ROOT" "pci-0000:00:14.0-usb-0:3:1.0-port0" "ttyUSB9"
"$SIM" pty --echo --link "$ROOT/dev/ttyUSB9" --hold-ms 120000 >"$TMPD/clone.json" 2>&1 & PIDS+=($!)
bash "$WAIT" "test -e '$ROOT/dev/ttyUSB9'" 5 0.05 || fail "clone device never appeared"

"$D" --socket "$SOCK" --dev-root "$ROOT" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
# Empty graph to start; everything below is incremental add-node (§11).
printf '' > "$TMPD/empty.toml"; $CC load "$TMPD/empty.toml" >/dev/null || fail "empty load"

addnode() { # addnode NAME DEVICE  -> writes a single-node toml, adds it, echoes result json
  cat > "$TMPD/n.toml" <<EOF
[[node]]
type = "serial"
name = "$1"
device = "$2"
arbitration = "free-for-all"
EOF
  $CC --json add-node "$TMPD/n.toml" 2>"$TMPD/add.err"
}

# ---- 1. FT4232: four independently bound nodes with distinct resolved paths ---
for i in 0 1 2 3; do
  addnode "ft$i" "usb:0403:6011:FT4232:0$i" >/dev/null 2>&1 || { cat "$TMPD/add.err"; fail "add ft$i failed"; }
done
bash "$WAIT" "$CC --json state | jq -e '[.nodes[]|select(.name|startswith(\"ft\"))|select(.status==\"active\")]|length==4'" 5 0.1 \
  || { $CC --json state | jq -c '.nodes[]|{name,status}'; fail "not all four FT interfaces bound active"; }
DISTINCT=$($CC --json state | jq '[.nodes[]|select(.name|startswith("ft"))|.resolved_path]|unique|length')
[ "$DISTINCT" = "4" ] || fail "FT interfaces did not resolve to 4 distinct paths (got $DISTINCT)"

# ---- 2. No-serial clone → by-path identity + documented warning --------------
CLONE=$(addnode "clone" "/dev/ttyUSB9")
echo "$CLONE" | jq -e '.warning' >/dev/null || { echo "$CLONE"; fail "no-serial clone add carried no .warning"; }
echo "$CLONE" | jq -e '.kind=="by-path"' >/dev/null || { echo "$CLONE"; fail "no-serial clone did not bind by-path"; }
# Its stored identity is the by-path form (dump round-trips it, not the raw path).
$CC dump | grep -q 'by-path:' || fail "clone's config identity is not by-path form"

# ---- 3. Path-form add, device absent → fails as designed (§12) ----------------
if addnode "ghost_path" "/dev/ttyUSBX_absent" >/dev/null 2>&1; then
  fail "path-form add of an absent device unexpectedly succeeded"
fi
grep -qiE 'not present|absent|-3200[0-9]' "$TMPD/add.err" || { cat "$TMPD/add.err"; fail "absent path-form add gave the wrong error"; }

# ---- 4. Identity-form add, device absent → succeeds into waiting --------------
GHOST=$(addnode "ghost_id" "usb:0403:9999:GHOST:00")
echo "$GHOST" | jq -e '.added=="ghost_id"' >/dev/null || { echo "$GHOST"; fail "identity-form absent add did not succeed"; }
$CC --json state | jq -e '.nodes[]|select(.name=="ghost_id")|.status=="waiting"' >/dev/null \
  || fail "identity-form absent node is not waiting"

echo '{"check":"phase7-matrix","pass":true}'
