#!/usr/bin/env bash
# Phase 4 validation (arbitration purge rules, §6): purge-on-detach and the
# "free lock never fires a non-holder's backlog" (the 3 a.m. hazard), plus
# purge-on-acquire. A locked-out client types into its kernel buffer, gets no
# grant, and its stale bytes must never reach the device — they are dropped and
# counted when it detaches, or when it belatedly acquires.
#
# No hardware (§15.17): the "device" is a nexus-sim sink, so "the device received
# nothing" is a byte count, not a judgement.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-purge\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4p.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
TB="$TMPD/ttyB"
SB=2048; SEED=13   # a locked-out writer's backlog (fits the PTY buffer, so it is
                   # counted exactly)
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${PC:-}" ] && kill "$PC" 2>/dev/null
  [ -n "${SINKPID:-}" ] && kill "$SINKPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# One ptyb origin on one serial endpoint (exclusive by default, §6). Fresh device
# sink per check so "the device received nothing" is exact.
write_config() {
  local dev=$1
  cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "pty"
name = "ptyb"
path = "$TB"
[[node]]
type = "serial"
name = "usb0"
device = "$dev"
[[edge]]
a = "usb0"
b = "ptyb"
EOF
}
present() { "$C" --json state | jq -e '.nodes[]|select(.name=="ptyb")|.client_present==true' >/dev/null; }
holder()  { "$C" --json state | jq -r '.nodes[]|select(.name=="usb0")|.lock.holder'; }
purged()  { "$C" --json state | jq -r '.nodes[]|select(.name=="usb0")|.lock.origins[]|select(.origin=="ptyb")|.purged'; }

# ============================================================================
# Check 1 — the 3 a.m. hazard + purge-on-detach.
# ============================================================================
DEV1="$TMPD/dev1"
"$SIM" pty --sink --bytes 1048576 --timeout-ms 4000 --link "$DEV1" >"$TMPD/sink1.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV1'" 5 0.05 || fail "dev1 never appeared"
write_config "$DEV1"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load 1 failed"; }

# A locked-out client types SB bytes and walks away (holds the slave, then we
# detach it). It never acquired, so nothing is read from it.
"$SIM" client --path "$TB" --send "seeded:$SB" --seed "$SEED" --hold-ms 5000 --timeout-ms 8000 >/dev/null 2>&1 &
PC=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"ptyb\")|.client_present==true'" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "locked-out client never became present"; }
# No holder, and nothing purged yet: its bytes are simply buffered (§6).
[ "$(holder)" = "null" ] || fail "endpoint has a holder it should not"
[ "$(purged)" = "0" ] || fail "purged should be 0 before detach, got $(purged)"

# Detach the client. Its backlog is purged-on-detach, counted exactly, and never
# fires (the lock was free the whole time, but a non-holder's bytes never fire).
kill "$PC" 2>/dev/null; PC=
bash "$WAIT" "test \"\$(\"$C\" --json state | jq -r '.nodes[]|select(.name==\"usb0\")|.lock.origins[]|select(.origin==\"ptyb\")|.purged')\" = \"$SB\"" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "purge-on-detach did not count exactly $SB (got $(purged))"; }

# The device saw none of it.
wait "$SINKPID" 2>/dev/null || true; SINKPID=
RECV=$(jq -r '.received // -1' "$TMPD/sink1.json")
[ "$RECV" = "0" ] || { cat "$TMPD/daemon.log"; fail "device received $RECV bytes from a non-holder (the 3 a.m. command fired)"; }

"$C" teardown >/dev/null || fail "teardown 1 failed"

# ============================================================================
# Check 2 — purge-on-acquire: pre-grant bytes are discarded on the grant.
# ============================================================================
DEV2="$TMPD/dev2"
"$SIM" pty --sink --bytes 1048576 --timeout-ms 4000 --link "$DEV2" >"$TMPD/sink2.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV2'" 5 0.05 || fail "dev2 never appeared"
write_config "$DEV2"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load 2 failed"; }

# The client writes PRE bytes BEFORE acquiring (the incorrect-but-guarded case),
# and holds the slave open.
"$SIM" client --path "$TB" --send "seeded:$SB" --seed "$SEED" --hold-ms 5000 --timeout-ms 8000 >/dev/null 2>&1 &
PC=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"ptyb\")|.client_present==true'" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "client never became present"; }
[ "$(purged)" = "0" ] || fail "purged should be 0 before acquire, got $(purged)"

# Acquire: purge-on-acquire drains and discards the pre-grant backlog, counted.
"$C" --json lock ptyb | jq -e '.acquired==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "lock ptyb failed"; }
[ "$(holder)" = "ptyb" ] || fail "ptyb should hold the lock after acquire"
bash "$WAIT" "test \"\$(\"$C\" --json state | jq -r '.nodes[]|select(.name==\"usb0\")|.lock.origins[]|select(.origin==\"ptyb\")|.purged')\" = \"$SB\"" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "purge-on-acquire did not discard+count exactly $SB (got $(purged))"; }

# The purged pre-grant bytes never reached the device.
kill "$PC" 2>/dev/null; PC=
wait "$SINKPID" 2>/dev/null || true; SINKPID=
RECV=$(jq -r '.received // -1' "$TMPD/sink2.json")
[ "$RECV" = "0" ] || { cat "$TMPD/daemon.log"; fail "device received $RECV pre-grant bytes (purge-on-acquire leaked)"; }

"$C" teardown >/dev/null || fail "teardown 2 failed"

# ============================================================================
# Check 3 — the purge is synchronous at grant time, so a correct acquire-BEFORE-
# write client loses NOTHING. The daemon drains at the moment of the grant, before
# the reply reaches the client, so the client's later command can never be
# mistaken for stale pre-grant input (a lazy drain in the reader would race it).
# ============================================================================
DEV3="$TMPD/dev3"
"$SIM" pty --sink --bytes "$SB" --link "$DEV3" --timeout-ms 15000 >"$TMPD/sink3.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV3'" 5 0.05 || fail "dev3 never appeared"
write_config "$DEV3"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load 3 failed"; }

# Acquire first (no client attached, so nothing to purge), THEN write.
"$C" --json lock ptyb | jq -e '.acquired==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "lock 3 failed"; }
"$SIM" client --path "$TB" --send "seeded:$SB" --seed "$SEED" --timeout-ms 15000 >"$TMPD/c3.json" 2>&1 \
  || { cat "$TMPD/daemon.log" "$TMPD/c3.json"; fail "post-grant client failed"; }

# The post-grant command reaches the device intact, byte-for-byte, and nothing
# was purged.
wait "$SINKPID" 2>/dev/null || true; SINKPID=
SHA_SENT=$(jq -r '.sha256_sent' "$TMPD/c3.json")
SHA_SINK=$(jq -r '.sha256 // ""' "$TMPD/sink3.json")
[ "$(jq -r '.received // -1' "$TMPD/sink3.json")" = "$SB" ] \
  || { cat "$TMPD/daemon.log"; fail "post-grant command did not reach the device (a racy purge discarded it)"; }
[ -n "$SHA_SENT" ] && [ "$SHA_SENT" = "$SHA_SINK" ] || fail "post-grant command corrupted en route"
[ "$(purged)" = "0" ] || { cat "$TMPD/daemon.log"; fail "purge-on-acquire wrongly counted post-grant bytes ($(purged))"; }

"$C" shutdown >/dev/null
echo '{"check":"phase4-purge","pass":true}'
