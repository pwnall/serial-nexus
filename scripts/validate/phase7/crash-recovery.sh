#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 5): crash recovery is exact. An
# incremental add-node (a log node) is persisted to the state file (§11/§15.9); a
# kill -9 then restart auto-recovers the whole graph from that file — the restored
# dump semantic-diffs equal to the pre-kill dump, PTY symlinks are recreated, and
# a fresh client passes the echo probe. Restart, replug, and first boot are one
# code path.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-crash-recovery\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"; C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"; WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
SEMDIFF="$REPO_ROOT/scripts/lib/semantic-diff.sh"

TMPD=$(mktemp -d /tmp/snx-p7c.XXXXXX) || fail "mktemp"
SOCK="$TMPD/s.sock"; CC="$C --socket $SOCK"
cleanup() { for p in "${DEV:-}" "${DPID:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

start_device() {
  "$SIM" pty --echo --link "$TMPD/dev1" --hold-ms 120000 >"$TMPD/dev.json" 2>&1 & DEV=$!
  bash "$WAIT" "test -e '$TMPD/dev1'" 5 0.05 || fail "device never appeared"
}
start_daemon() {
  "$D" --socket "$SOCK" >>"$TMPD/daemon.log" 2>&1 & DPID=$!
  bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket"; }
}

start_device; start_daemon
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/dev1"
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
  || fail "serial never active"

# Incrementally add a log node (persisted to the state file).
cat > "$TMPD/log.toml" <<EOF
[[node]]
type = "log"
name = "cap"
directory = "$TMPD"
filename = "cap.log"
EOF
$CC add-node "$TMPD/log.toml" >/dev/null || fail "add-node log failed"
$CC dump > "$TMPD/pre.toml" || fail "pre-kill dump"
grep -q 'name = "cap"' "$TMPD/pre.toml" || fail "added log node not in dump"

# ---- kill -9 and restart -----------------------------------------------------
kill -9 "$DPID" 2>/dev/null; wait "$DPID" 2>/dev/null; DPID=
# The device stays plugged in across a daemon restart; a real adapter releases its
# fd on the daemon's death, so restart the sim fresh (its held master would keep
# the crashed daemon's TIOCEXCL alive on the pts — a sim-only artifact).
kill "$DEV" 2>/dev/null; wait "$DEV" 2>/dev/null; DEV=
rm -f "$TMPD/dev1" "$TMPD/con"
start_device
start_daemon

# The graph auto-recovered from the persisted state file (no manual reload).
bash "$WAIT" "$CC --json state | jq -e '[.nodes[].name]|sort==[\"cap\",\"con\",\"usb0\"]'" 5 0.1 \
  || { $CC --json state | jq -c '.nodes[].name'; fail "graph did not auto-recover from the state file"; }
$CC dump > "$TMPD/post.toml" || fail "post-restart dump"
bash "$SEMDIFF" "$TMPD/pre.toml" "$TMPD/post.toml" >/dev/null \
  || { echo "--- pre ---"; cat "$TMPD/pre.toml"; echo "--- post ---"; cat "$TMPD/post.toml"; fail "recovered dump differs from pre-kill dump"; }

# PTY symlink recreated.
[ -L "$TMPD/con" ] || fail "PTY symlink not recreated on restart"

# Fresh client passes the echo probe (data plane healed).
bash "$WAIT" "$CC --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 6 0.1 \
  || { $CC --json state | jq -c '.nodes[]|{name,status,reason}'; fail "serial not active after restart"; }
"$SIM" client --path "$TMPD/con" --send seeded:2KiB --expect echo --seed 3 --timeout-ms 8000 \
  >"$TMPD/echo.json" 2>&1 || true
jq -e '.pass==true' "$TMPD/echo.json" >/dev/null || { cat "$TMPD/echo.json"; fail "post-restart echo probe failed"; }

echo '{"check":"phase7-crash-recovery","pass":true}'
