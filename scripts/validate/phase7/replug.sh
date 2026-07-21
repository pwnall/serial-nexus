#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 2): replug heals and reapplies. After an
# unplug, targetward writes buffer during the outage; recreating the device at the
# SAME identity (§12) heals the node to active, the reopen ritual reapplies raw
# termios + TIOCEXCL + modem lines (§7.1), and purge-on-reconnect discards the
# outage-era backlog (counter > 0) so stale commands never fire into the booting
# device. A fresh echo round-trip being byte-clean proves the raw termios was
# reapplied (a non-raw reopen would corrupt via echo/newline translation).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-replug\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"; C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"; WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
source "$REPO_ROOT/scripts/lib/fixture-tree.sh"

TMPD=$(mktemp -d /tmp/snx-p7r.XXXXXX) || fail "mktemp"
ROOT="$TMPD/root"; SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
mkdir -p "$ROOT/dev"
cleanup() { for p in "${DEV:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

ID="usb:0403:6001:REPLUG1:00"
start_device() {
  "$SIM" pty --echo --link "$ROOT/dev/ttyUSB0" --hold-ms 120000 >"$TMPD/dev.json" 2>&1 & DEV=$!
  bash "$WAIT" "test -e '$ROOT/dev/ttyUSB0'" 5 0.05 || fail "device never appeared"
}
make_fixture() { make_usb_iface "$ROOT" "1-1" "0403" "6001" "REPLUG1" "ttyUSB0" "00" "usb-FTDI_REPLUG1-if00"; }

make_fixture; start_device
"$D" --socket "$SOCK" --dev-root "$ROOT" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$ID"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "con"
path = "$TMPD/con"
[[edge]]
a = "usb0"
b = "con"
EOF
$CC load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load"; }
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || fail "serial never active on first plug"

# Baseline echo works (device present, data plane healthy).
"$SIM" client --path "$TMPD/con" --send seeded:1KiB --expect echo --seed 1 --timeout-ms 8000 \
  >"$TMPD/base.json" 2>&1 || true
jq -e '.pass==true' "$TMPD/base.json" >/dev/null || { cat "$TMPD/base.json"; fail "baseline echo failed"; }

# ---- Unplug ------------------------------------------------------------------
kill "$DEV" 2>/dev/null; wait "$DEV" 2>/dev/null; DEV=
unplug_usb "$ROOT" "ttyUSB0" "usb-FTDI_REPLUG1-if00"
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"waiting\"'" 5 0.1 \
  || fail "serial did not fault-and-wait on unplug"

# Buffer stale targetward commands during the outage (they park in the serial's
# bounded channel, backpressured, never dropped — §5).
for i in 1 2 3 4 5 6 7 8; do $CC send usb0 --line "STALE-COMMAND-$i" >/dev/null 2>&1 || true; done

# ---- Replug at the SAME identity ---------------------------------------------
make_fixture; start_device
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 8 0.1 \
  || { $CC --json state | jq -c '.nodes[]|{name,status,reason}'; fail "serial did not heal on replug"; }

# purge-on-reconnect discarded the outage backlog (the stale commands never fired).
PURGED=$($CC --json state | jq -r '.nodes[]|select(.name=="usb0")|.purged_on_reconnect')
[ "${PURGED:-0}" -gt 0 ] 2>/dev/null || fail "purge-on-reconnect counter not set (got $PURGED)"

# The reopen ritual reapplied raw termios: a fresh echo round-trips byte-clean
# (a non-raw reopen would echo/translate and corrupt it), and no stale byte leaks.
"$SIM" client --path "$TMPD/con" --send seeded:2KiB --expect echo --seed 9 --timeout-ms 8000 \
  >"$TMPD/heal.json" 2>&1 || true
jq -e '.pass==true' "$TMPD/heal.json" >/dev/null \
  || { cat "$TMPD/heal.json"; fail "post-replug echo not byte-clean (termios not reapplied, or stale leak)"; }

echo "{\"check\":\"phase7-replug\",\"pass\":true,\"purged\":$PURGED}"
