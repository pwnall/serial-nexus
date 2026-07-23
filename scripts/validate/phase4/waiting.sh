#!/usr/bin/env bash
# Phase 4 validation (arbitration §6, plan item 7): the FIFO waiter queue is fair
# and cancel-safe (§15.20).
#  A. Two `lock --wait` waiters are granted in arrival order across an unlock AND a
#     detach-release, and each grant runs purge-on-acquire first.
#  B. Killing the first waiter mid-wait dequeues it (the queue shrinks) and the
#     second waiter is granted next — not the cancelled one.
#  C. A deadline `send` against a stubborn holder returns the locked error and
#     leaves the queue intact.
#
# No hardware (§15.17): a nexus-sim sink stands in for the device; ptyb's client
# types pre-grant bytes so the purge-on-acquire count is byte-exact.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-waiting\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4w.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
TA="$TMPD/ttyA"; TB="$TMPD/ttyB"; TC="$TMPD/ttyC"
PRE=64; SEED=5     # ptyb's pre-grant backlog, purged exactly on the queued grant
cleanup() {
  for p in "${DPID:-}" "${SINKPID:-}" "${WB:-}" "${WC:-}" "${CLB:-}"; do
    [ -n "$p" ] && kill "$p" 2>/dev/null
  done
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$SIM" pty --sink --bytes 1000000 --link "$DEV" --timeout-ms 40000 >"$TMPD/sink.json" 2>&1 &
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
type = "pty"
name = "ptyc"
path = "$TC"
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
[[edge]]
a = "usb0"
b = "ptyc"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

holder()   { "$C" --json state | jq -r '.nodes[]|select(.name=="usb0")|.lock.holder'; }
waiters()  { "$C" --json state | jq -c '.nodes[]|select(.name=="usb0")|.lock.waiters'; }
purged_b() { "$C" --json state | jq -r '.nodes[]|select(.name=="usb0")|.lock.origins[]|select(.origin=="ptyb")|.purged'; }
wait_waiters() { bash "$WAIT" "test \"\$($C --json state | jq -c '.nodes[]|select(.name==\"usb0\")|.lock.waiters')\" = '$1'" "${2:-5}" 0.05; }

# ============================================================================
# A — FIFO across an unlock and a detach-release; purge-on-acquire per grant.
# ============================================================================
# ptyb has a client that types PRE bytes (buffered, since ptyb is not the holder)
# and then holds the slave open, so it can later detach-release.
"$SIM" client --path "$TB" --send "seeded:$PRE" --seed "$SEED" --hold-ms 30000 --timeout-ms 35000 >/dev/null 2>&1 &
CLB=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"ptyb\")|.client_present==true'" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "ptyb client never became present"; }

"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "lock ptya failed"; }

( timeout 15 "$C" --json lock ptyb --wait >"$TMPD/wb.json" 2>&1 ) &
WB=$!
wait_waiters '["ptyb"]' || { cat "$TMPD/daemon.log"; fail "ptyb did not enqueue (waiters=$(waiters))"; }
( timeout 15 "$C" --json lock ptyc --wait >"$TMPD/wc.json" 2>&1 ) &
WC=$!
wait_waiters '["ptyb","ptyc"]' || { cat "$TMPD/daemon.log"; fail "queue not [ptyb,ptyc] (got $(waiters))"; }

# Unlock ptya: the head (ptyb) is granted — arrival order — and its queued grant
# runs purge-on-acquire, discarding its PRE pre-grant bytes exactly.
"$C" unlock ptya >/dev/null
wait "$WB" 2>/dev/null || true
jq -e '.acquired==true' "$TMPD/wb.json" >/dev/null 2>&1 || { cat "$TMPD/daemon.log" "$TMPD/wb.json"; fail "ptyb's --wait was not granted after unlock"; }
[ "$(holder)" = "ptyb" ] || fail "holder not ptyb after unlock (got $(holder))"
wait_waiters '["ptyc"]' || fail "queue not [ptyc] after ptyb granted (got $(waiters))"
bash "$WAIT" "test \"\$($C --json state | jq -r '.nodes[]|select(.name==\"usb0\")|.lock.origins[]|select(.origin==\"ptyb\")|.purged')\" = \"$PRE\"" 3 0.05 \
  || { cat "$TMPD/daemon.log"; fail "purge-on-acquire on the queued grant did not count $PRE (got $(purged_b))"; }

# Detach ptyb's client: detach-release frees the lock and the head (ptyc) is
# granted next — the second grant path.
kill "$CLB" 2>/dev/null; CLB=
wait "$WC" 2>/dev/null || true
jq -e '.acquired==true' "$TMPD/wc.json" >/dev/null 2>&1 || { cat "$TMPD/daemon.log" "$TMPD/wc.json"; fail "ptyc's --wait was not granted after ptyb's detach-release"; }
[ "$(holder)" = "ptyc" ] || fail "holder not ptyc after detach-release (got $(holder))"
wait_waiters '[]' || fail "queue not empty after ptyc granted (got $(waiters))"
"$C" unlock ptyc >/dev/null

# ============================================================================
# B — cancel-safety: killing the first waiter dequeues it; the second is granted.
# ============================================================================
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || fail "lock ptya (B) failed"
( timeout 15 "$C" lock ptyb --wait >/dev/null 2>&1 ) &
WB=$!
wait_waiters '["ptyb"]' || { cat "$TMPD/daemon.log"; fail "ptyb did not enqueue (B)"; }
( timeout 15 "$C" --json lock ptyc --wait >"$TMPD/wc2.json" 2>&1 ) &
WC=$!
wait_waiters '["ptyb","ptyc"]' || fail "queue not [ptyb,ptyc] (B, got $(waiters))"

# Kill the first waiter's client: its control connection drops, so the daemon
# dequeues it (cancel-safe, §15.20) and the queue shrinks to [ptyc].
kill "$WB" 2>/dev/null; wait "$WB" 2>/dev/null || true; WB=
wait_waiters '["ptyc"]' || { cat "$TMPD/daemon.log"; fail "cancelled waiter not dequeued (got $(waiters))"; }

# Unlock ptya: ptyc (not the cancelled ptyb) is granted next.
"$C" unlock ptya >/dev/null
wait "$WC" 2>/dev/null || true
jq -e '.acquired==true' "$TMPD/wc2.json" >/dev/null 2>&1 || { cat "$TMPD/daemon.log" "$TMPD/wc2.json"; fail "ptyc not granted after the cancelled ptyb"; }
[ "$(holder)" = "ptyc" ] || fail "holder not ptyc after cancel (got $(holder))"
"$C" unlock ptyc >/dev/null

# ============================================================================
# C — a deadline `send` against a stubborn holder returns LOCKED, queue intact.
# ============================================================================
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || fail "lock ptya (C) failed"
( timeout 15 "$C" lock ptyb --wait >/dev/null 2>&1 ) &
WB=$!
wait_waiters '["ptyb"]' || { cat "$TMPD/daemon.log"; fail "ptyb did not enqueue (C)"; }

if "$C" send usb0 --line "nope" --timeout-ms 400 2>"$TMPD/sendc.err"; then
  cat "$TMPD/daemon.log"; fail "deadline send should have failed against a held lock"
fi
grep -qi 'lock' "$TMPD/sendc.err" || { cat "$TMPD/sendc.err"; fail "deadline send failed, but not with a locked error"; }
# The pre-existing waiter queue is untouched by the transient send origin.
[ "$(waiters)" = '["ptyb"]' ] || { cat "$TMPD/daemon.log"; fail "send disturbed the queue (got $(waiters))"; }

kill "$WB" 2>/dev/null; wait "$WB" 2>/dev/null || true; WB=
"$C" unlock ptya >/dev/null

# ============================================================================
# E — remove-node --cascade of a WAITING writer's node wakes its parked --wait
# (§6/§15.20, DLC-1): unregistering a queued waiter from a SURVIVING host lock
# must wake it so it leaves with a defined error, not park until an unrelated wake.
# ============================================================================
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || fail "lock ptya (E) failed"
( timeout 12 "$C" lock ptyc --wait >"$TMPD/we.json" 2>&1; echo "exit:$?" >>"$TMPD/we.json" ) &
WC=$!
wait_waiters '["ptyc"]' || { cat "$TMPD/daemon.log"; fail "ptyc did not enqueue (E)"; }
# Remove the *waiting* writer's node while ptya still holds usb0's lock. usb0
# survives; ptyc's origin is unregistered from usb0's lock and its parked --wait
# must return promptly rather than hang until its own 12 s timeout.
"$C" remove-node ptyc --cascade >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "remove-node ptyc --cascade (E) failed"; }
bash "$WAIT" "grep -q 'exit:' '$TMPD/we.json'" 4 0.05 \
  || { cat "$TMPD/daemon.log" "$TMPD/we.json"; fail "the --wait did not return after the waiter's node was cascade-removed (stuck waiter)"; }
# It returned an ERROR (non-zero exit), not a spurious grant.
grep -q 'exit:0' "$TMPD/we.json" && { cat "$TMPD/we.json"; fail "the --wait wrongly succeeded after its origin was removed"; }
wait "$WC" 2>/dev/null || true; WC=
# usb0 is unharmed: ptya still holds it and the queue is empty.
[ "$(holder)" = "ptya" ] || fail "usb0 holder not ptya after cascade-remove of waiter (got $(holder))"
wait_waiters '[]' || fail "usb0 queue not empty after cascade-remove of waiter (got $(waiters))"
"$C" unlock ptya >/dev/null

# ============================================================================
# D — teardown wakes a parked waiter with a defined error (§6/§15.20): a
# deadline-less `lock --wait` must not hang forever when its endpoint vanishes.
# ============================================================================
"$C" --json lock ptya | jq -e '.acquired==true' >/dev/null || fail "lock ptya (D) failed"
( timeout 12 "$C" lock ptyb --wait >"$TMPD/wd.json" 2>&1; echo "exit:$?" >>"$TMPD/wd.json" ) &
WB=$!
wait_waiters '["ptyb"]' || { cat "$TMPD/daemon.log"; fail "ptyb did not enqueue (D)"; }
"$C" teardown >/dev/null || { cat "$TMPD/daemon.log"; fail "teardown (D) failed"; }
# The parked --wait must RETURN promptly (not hang until its own 12 s timeout).
bash "$WAIT" "grep -q 'exit:' '$TMPD/wd.json'" 4 0.05 \
  || { cat "$TMPD/daemon.log" "$TMPD/wd.json"; fail "the --wait did not return after teardown (stuck waiter)"; }
# It returned an ERROR (non-zero exit), not a spurious grant.
grep -q 'exit:0' "$TMPD/wd.json" && { cat "$TMPD/wd.json"; fail "the --wait wrongly succeeded after teardown"; }
wait "$WB" 2>/dev/null || true; WB=

"$C" shutdown >/dev/null
echo '{"check":"phase4-waiting","pass":true}'
