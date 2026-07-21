#!/usr/bin/env bash
# Phase 4 validation (arbitration §6, plan item 4): steal and lease.
#  - `lock --steal` transfers the lock, records the theft in state, and emits an
#    IMMEDIATE id-less notification (event-driven, faster than the 200 ms snapshot).
#  - an expired `--lease-ms` releases a silent holder within the configured bound.
#  - a stale lease timer NEVER fires across grants: unlock, re-lock, let the old
#    timer elapse, and the new grant survives (generation-guarded, §6).
#
# No hardware (§15.17): a nexus-sim sink stands in for the device so the daemon and
# its lock state are exercised end to end; no bytes are asserted here.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-steal-lease\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4sl.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
TA="$TMPD/ttyA"
TB="$TMPD/ttyB"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SINKPID:-}" ] && kill "$SINKPID" 2>/dev/null
  [ -n "${SUBP:-}" ] && kill "$SUBP" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$SIM" pty --sink --bytes 1000000 --link "$DEV" --timeout-ms 30000 >"$TMPD/sink.json" 2>&1 &
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
device = "$DEV"
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }
holder() { "$C" --json state | jq -r '.nodes[]|select(.name=="usb0")|.lock.holder'; }

# ============================================================================
# Check 1 — steal transfers the lock, records it, and notifies immediately.
# ============================================================================
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "lock ptya failed"; }

# Subscribe, then steal, and assert a `lock` NOTIFICATION arrives — event-driven,
# not the periodic "state" snapshot (which uses a different method name). A 2 s
# window with a 200 ms snapshot cadence would surface a snapshot too, so we require
# a method=="lock" frame specifically.
( timeout 4 "$C" subscribe >"$TMPD/sub.json" 2>&1 ) &
SUBP=$!
# Wait for the first periodic snapshot (method "state"), which only flows once a
# subscriber is registered — a bounded proof the subscription is live (not a bare
# sleep, plan §3) before we trigger the steal.
bash "$WAIT" "grep -q '\"method\":\"state\"' '$TMPD/sub.json'" 3 0.05 \
  || { cat "$TMPD/sub.json"; fail "subscription never registered (no snapshot)"; }

"$C" --json lock ptyb --steal | jq -e '.acquired==true and .stole_from=="ptya"' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "steal did not report acquired + stole_from=ptya"; }
[ "$(holder)" = "ptyb" ] || fail "holder not ptyb after steal (got $(holder))"
# State records the steal so the ousted holder can see it (§6).
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.last_steal=={"from":"ptya","by":"ptyb"}' >/dev/null \
  || fail "state did not record the steal (from ptya, by ptyb)"

bash "$WAIT" "grep -q '\"method\":\"lock\"' '$TMPD/sub.json'" 3 0.1 \
  || { cat "$TMPD/sub.json"; fail "no immediate 'lock' notification after the steal"; }
jq -e 'select(.method=="lock")|.params.lock.holder=="ptyb"' "$TMPD/sub.json" >/dev/null 2>&1 \
  || { cat "$TMPD/sub.json"; fail "the lock notification did not carry holder=ptyb"; }
kill "$SUBP" 2>/dev/null; wait "$SUBP" 2>/dev/null || true; SUBP=
"$C" unlock ptyb >/dev/null

# ============================================================================
# Check 2 — an expired lease releases a silent holder within the bound.
# ============================================================================
"$C" --json lock ptya --lease-ms 300 | jq -e '.acquired==true' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "lease-lock failed"; }
[ "$(holder)" = "ptya" ] || fail "ptya should hold immediately after a lease grant"
# Within a generous bound (the lease is 300 ms), the holder auto-releases.
bash "$WAIT" "test \"\$(\"$C\" --json state | jq -r '.nodes[]|select(.name==\"usb0\")|.lock.holder')\" = null" 3 0.05 \
  || { cat "$TMPD/daemon.log"; fail "lease did not auto-release the holder"; }

# ============================================================================
# Check 3 — a stale lease timer never fires across grants (generation guard, §6).
# ============================================================================
"$C" --json lock ptya --lease-ms 400 >/dev/null || fail "lease-lock (check 3) failed"
"$C" unlock ptya >/dev/null                         # release before the lease fires
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || fail "re-lock failed"
# The stale 400 ms timer from the released grant must NOT release this new (plain,
# lease-free) grant. Assert the holder stays ptya continuously across a window that
# outlives the old lease (poll, not a bare sleep): if the stale timer wrongly
# fired, holder would flip to null and we'd catch it.
END=$(( $(date +%s%N) / 1000000 + 700 ))
while [ "$(( $(date +%s%N) / 1000000 ))" -lt "$END" ]; do
  [ "$(holder)" = "ptya" ] || { cat "$TMPD/daemon.log"; fail "a stale lease timer released a later grant (holder became null)"; }
  sleep 0.05
done
"$C" unlock ptya >/dev/null

# ============================================================================
# Check 4 — re-arming a lease EXTENDS it: the earlier timer must not fire (§6).
# ============================================================================
"$C" --json lock ptya --lease-ms 400 >/dev/null || fail "lease-lock (check 4) failed"
# Re-arm to a much longer lease well before the original 400 ms elapses. The
# renewal bumps the grant generation, so the first (400 ms) timer is invalidated.
"$C" --json lock ptya --lease-ms 4000 | jq -e '.held==true' >/dev/null || fail "lease re-arm failed"
# Across the ORIGINAL 400 ms deadline the holder must NOT be released (renewal won).
END=$(( $(date +%s%N) / 1000000 + 700 ))
while [ "$(( $(date +%s%N) / 1000000 ))" -lt "$END" ]; do
  [ "$(holder)" = "ptya" ] || { cat "$TMPD/daemon.log"; fail "lease renewal did not extend (released at the original deadline)"; }
  sleep 0.05
done
"$C" unlock ptya >/dev/null

"$C" shutdown >/dev/null
echo '{"check":"phase4-steal-lease","pass":true}'
