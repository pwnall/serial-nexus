#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 6): serial-signal verbs reach the wire,
# and remove-node --cascade flushes the log queue. No-target doctrine (§13):
# a PTY cannot convey DTR/break to its master, so true master-side observation of
# pulse-dtr/send-break is a Tier-3 hardware checklist item (a real null modem).
# Unprivileged, we prove the verbs REACH the live port: send-break latches on a
# pts (succeeds), while set-modem/pulse-dtr reach the driver and are cleanly
# rejected by the pts (ENOTTY — the exact Tier-3 boundary), and the node stays
# healthy throughout. remove-node --cascade flushes the log fully before removal.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-signals\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"; C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"; WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p7g.XXXXXX) || fail "mktemp"
SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
cleanup() { for p in "${DEV:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

# A device that sources a known stream (for the log) then stays present.
"$SIM" pty --source --bytes 256KiB --seed 5 --hold-ms 120000 --link "$TMPD/dev1" >"$TMPD/dev.json" 2>&1 & DEV=$!
bash "$WAIT" "test -e '$TMPD/dev1'" 5 0.05 || fail "device never appeared"

"$D" --socket "$SOCK" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/dev1"
arbitration = "free-for-all"
[[node]]
type = "log"
name = "cap"
directory = "$TMPD"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "cap"
EOF
$CC load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load"; }
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || fail "serial never active"

# ---- send-break: reaches the live port and latches (works on a pts) ----------
$CC send-break usb0 --ms 30 >/dev/null 2>"$TMPD/brk.err" || { cat "$TMPD/brk.err"; fail "send-break did not reach the port"; }

# ---- set-modem / pulse-dtr: reach the driver; a pts rejects (ENOTTY) ----------
# A device-level error (not a routing error) proves the verb reached the live
# port and issued the ioctl — the exact point a real UART would honor it.
if $CC set-modem usb0 --dtr true >/dev/null 2>"$TMPD/sm.err"; then
  fail "set-modem unexpectedly succeeded on a pts (a pts has no modem lines)"
fi
grep -qiE 'ioctl|set-modem on' "$TMPD/sm.err" || { cat "$TMPD/sm.err"; fail "set-modem did not reach the live port"; }
if $CC pulse-dtr usb0 --ms 20 >/dev/null 2>"$TMPD/pd.err"; then
  fail "pulse-dtr unexpectedly succeeded on a pts"
fi
grep -qiE 'ioctl|pulse-dtr on' "$TMPD/pd.err" || { cat "$TMPD/pd.err"; fail "pulse-dtr did not reach the live port"; }

# The node is undisturbed by the signal verbs.
$CC --json state | jq -e '.nodes[]|select(.name=="usb0")|.status=="active"' >/dev/null \
  || fail "signal verbs disturbed the serial node"

# ---- remove-node --cascade flushes the log queue before the node disappears ---
bash "$WAIT" "test \$(stat -c%s '$TMPD/cap.log' 2>/dev/null || echo 0) -eq 262144" 8 0.1 \
  || fail "log never captured the full sourced stream (got $(stat -c%s "$TMPD/cap.log" 2>/dev/null))"
$CC remove-node cap --cascade >/dev/null || fail "remove-node --cascade failed"
# The node is gone, its edge removed, and the file is complete (flushed, not truncated).
$CC --json state | jq -e '[.nodes[].name]|contains(["cap"])|not' >/dev/null || fail "cap still present after removal"
$CC dump | grep -q 'name = "cap"' && fail "cap still in config after removal" || true
[ "$(stat -c%s "$TMPD/cap.log")" = "262144" ] || fail "log file not complete after cascade flush"

# ---- remove-node --cascade of a lock-HOLDING writer releases the host lock ----
# A removed writer must leave the surviving host endpoint's lock cleanly (§6/§15.20)
# — otherwise the endpoint wedges permanently locked by a phantom origin.
cat > "$TMPD/c2.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/dev1"
arbitration = "exclusive"
[[node]]
type = "pty"
name = "ptya"
path = "$TMPD/ptya"
[[edge]]
a = "usb0"
b = "ptya"
EOF
$CC load --replace "$TMPD/c2.toml" >/dev/null || fail "load --replace failed"
$CC lock ptya >/dev/null || fail "lock ptya failed"
$CC --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder=="ptya"' >/dev/null \
  || fail "ptya did not hold usb0's lock"
$CC remove-node ptya --cascade >/dev/null || fail "remove-node ptya --cascade failed"
# The surviving serial's lock must be free (no phantom holder / origin), recoverable.
$CC --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder==null and (.lock.origins|length==0)' >/dev/null \
  || { $CC --json state | jq -c '.nodes[]|select(.name=="usb0")|.lock'; fail "cascade left a phantom lock holder on usb0"; }

echo '{"check":"phase7-signals","pass":true}'
