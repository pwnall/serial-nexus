#!/usr/bin/env bash
# Phase 3 validation (the log node, §7.3): a log captures the hostward stream
# with no loss, on-demand rotation loses nothing and numbers higher-is-newer,
# and the rotation counter is recovered by directory scan across a daemon
# restart (never persisted). No hardware (§15.17); devices are nexus-sim PTYs.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-log\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3l.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
LOGDIR="$TMPD/logs"; mkdir -p "$LOGDIR"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

start_daemon() { "$D" >>"$TMPD/daemon.log" 2>&1 & DPID=$!; bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }; }
sha() { sha256sum "$1" | cut -d' ' -f1; }
fsize() { stat -c %s "$1" 2>/dev/null || echo 0; }
nstate() { "$C" --json state | jq -r ".nodes[]|select(.name==\"$1\")|.$2"; }

start_daemon

# ---- Check 1: the log captures the whole hostward stream, no loss (§7.3) ----
DEV1="$TMPD/dev1"; SRCJSON="$TMPD/src1.json"
"$SIM" pty --source --bytes 256KiB --seed 7 --link "$DEV1" >"$SRCJSON" 2>"$TMPD/src1.err" &
SRCPID=$!; PIDS+=($SRCPID)
bash "$WAIT" "test -e '$DEV1'" 5 0.05 || fail "dev1 never appeared"

cat > "$TMPD/c1.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV1"
[[node]]
type = "log"
name = "cap"
directory = "$LOGDIR"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "cap"
EOF
"$C" load "$TMPD/c1.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c1 failed"; }

wait "$SRCPID" 2>/dev/null   # source completes once the serial has drained it
SRCSHA=$(jq -r .sha256 "$SRCJSON") || fail "no source checksum"
bash "$WAIT" "test \"\$(stat -c %s '$LOGDIR/cap.log' 2>/dev/null || echo 0)\" -ge 262144" 10 0.1 \
  || { cat "$TMPD/daemon.log"; fail "log never reached the sourced size (queued=$(nstate cap queued_bytes))"; }
[ "$(sha "$LOGDIR/cap.log")" = "$SRCSHA" ] || fail "log checksum != source checksum (lossy capture)"
[ "$(nstate cap dropped_bytes)" = "0" ] || fail "log dropped_bytes should be 0 for a keep-up disk"
"$C" teardown >/dev/null || fail "teardown after c1 failed"

# ---- Check 2: rotation loses nothing; each batch lands in its own file -------
# An echo device stays alive; a client drives one seeded batch at a time and we
# rotate between batches. Each rotation file must equal exactly that batch.
DEV2="$TMPD/dev2"
"$SIM" pty --echo --link "$DEV2" --timeout-ms 60000 >"$TMPD/dev2.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV2'" 5 0.05 || fail "dev2 never appeared"

TTY2="$TMPD/console2"
cat > "$TMPD/c2.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY2"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV2"
[[node]]
type = "log"
name = "rot"
directory = "$LOGDIR"
filename = "rot.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "rot"
EOF
"$C" load "$TMPD/c2.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c2 failed"; }

declare -a BATCHSHA
send_batch() { # $1=seed  -> echoes 32KiB, returns after the log has it
  local seed=$1 v
  v=$("$SIM" client --path "$TTY2" --send seeded:32KiB --expect echo --seed "$seed" --timeout-ms 15000)
  echo "$v" | jq -e '.pass==true and .received==32768' >/dev/null || return 1
  echo "$v" | jq -r .sha256_sent
}

# Batch A -> current file; rotate -> rot.log.000 == A
A=$(send_batch 1) || fail "batch A echo failed"
bash "$WAIT" "test \"\$(stat -c %s '$LOGDIR/rot.log' 2>/dev/null || echo 0)\" -ge 32768" 10 0.1 || fail "batch A not logged"
"$C" rotate rot >/dev/null || fail "rotate 1 failed"
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"rot\")|.rotation==0'" 5 0.05 || fail "rotation did not reach 0"
[ "$(sha "$LOGDIR/rot.log.000")" = "$A" ] || fail "rot.log.000 != batch A"

# Batch B -> fresh current; rotate -> rot.log.001 == B
B=$(send_batch 2) || fail "batch B echo failed"
bash "$WAIT" "test \"\$(stat -c %s '$LOGDIR/rot.log' 2>/dev/null || echo 0)\" -ge 32768" 10 0.1 || fail "batch B not logged"
"$C" rotate rot >/dev/null || fail "rotate 2 failed"
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"rot\")|.rotation==1'" 5 0.05 || fail "rotation did not reach 1"
[ "$(sha "$LOGDIR/rot.log.001")" = "$B" ] || fail "rot.log.001 != batch B"

# Batch C stays in the live file. Each batch landed in exactly its own file with
# a matching checksum (A->.000, B->.001, C->live), so rotation lost nothing and
# split no chunk across a boundary.
Cc=$(send_batch 3) || fail "batch C echo failed"
bash "$WAIT" "test \"\$(stat -c %s '$LOGDIR/rot.log' 2>/dev/null || echo 0)\" -ge 32768" 10 0.1 || fail "batch C not logged"
[ "$(sha "$LOGDIR/rot.log")" = "$Cc" ] || fail "live rot.log != batch C"

# ---- Check 3: rotation counter recovered by directory scan on restart -------
# Kill the daemon hard; a fresh daemon loading the same directory must continue
# numbering from the scan (higher is newer), not restart at 000 (§7.3).
kill -9 "$DPID" 2>/dev/null; wait "$DPID" 2>/dev/null; DPID=
# The hard kill skips the clean-shutdown unlink; the daemon's own stale-socket
# dance (§10) reclaims the leftover socket on the next start.
start_daemon
"$C" load "$TMPD/c2.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "reload c2 failed"; }
# Existing rotations are rot.log.000 and rot.log.001, so state must show 1...
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"rot\")|.rotation==1'" 5 0.05 \
  || fail "rotation counter not recovered from directory scan (got $(nstate rot rotation))"
# ...and the next rotation must be 002, never a clobbering 000. The earlier
# batches A and B must survive untouched (higher-is-newer, no cascade).
A_SHA_BEFORE=$(sha "$LOGDIR/rot.log.000")
"$C" rotate rot >/dev/null || fail "post-restart rotate failed"
bash "$WAIT" "test -e '$LOGDIR/rot.log.002'" 5 0.05 || fail "post-restart rotation did not produce rot.log.002"
[ "$(sha "$LOGDIR/rot.log.000")" = "$A_SHA_BEFORE" ] || fail "rotation cascaded/clobbered rot.log.000"

"$C" shutdown >/dev/null
echo '{"check":"phase3-log","pass":true}'
