#!/usr/bin/env bash
# Phase 3 validation (boundary counters): every hostward drop is located,
# counted, and attributable in state (design §5, §7.1, §7.2). No hardware, per
# the no-target doctrine (§15.17); the "device" is a seeded nexus-sim source.
#
# Three checks:
#   1. A serial node with nothing attached reads-and-discards with a counter
#      (§5): serial `discarded_unattached` tracks the sourced bytes.
#   2. A serial→PTY graph with no client attached discards at the PTY boundary
#      (§7.2 presence gating): console `discarded_no_client` tracks the bytes,
#      while the serial's own discard counter stays 0 (something *is* attached)
#      and no slow-consumer drops occur.
#   3. A present, draining client loses nothing: both PTY drop counters stay 0.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-counters\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3c.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

nstate() { "$C" --json state | jq -r ".nodes[]|select(.name==\"$1\")|.$2"; }

# ---- Check 1: serial discards when nothing is attached (§5) ----------------
DEV1="$TMPD/dev1"
"$SIM" pty --source --bytes 256KiB --seed 7 --link "$DEV1" >"$TMPD/dev1.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV1'" 5 0.05 || fail "dev1 never appeared"

cat > "$TMPD/c1.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV1"
EOF
"$C" load "$TMPD/c1.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c1 failed"; }

# The lone serial reads the sourced stream and discards it, counting every byte.
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.discarded_unattached >= 200000'" 10 0.1 \
  || { cat "$TMPD/daemon.log" "$TMPD/dev1.log"; fail "serial discarded_unattached did not reach the sourced bytes"; }
"$C" teardown >/dev/null || fail "teardown after c1 failed"

# ---- Check 2: PTY discards when no client is attached (§7.2) ----------------
DEV2="$TMPD/dev2"
"$SIM" pty --source --bytes 256KiB --seed 7 --link "$DEV2" >"$TMPD/dev2.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV2'" 5 0.05 || fail "dev2 never appeared"

cat > "$TMPD/c2.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TMPD/console2"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV2"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/c2.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c2 failed"; }

bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"console\")|.discarded_no_client >= 200000'" 10 0.1 \
  || { cat "$TMPD/daemon.log" "$TMPD/dev2.log"; fail "console discarded_no_client did not reach the sourced bytes"; }
# Something IS attached to the serial (the PTY), so its own discard stays 0,
# and a fast-draining discard means no slow-consumer full-buffer drops.
[ "$(nstate usb0 discarded_unattached)" = "0" ] \
  || fail "serial discarded_unattached should be 0 when a consumer is attached"
[ "$(nstate console dropped_slow_consumer)" = "0" ] \
  || fail "console dropped_slow_consumer should be 0 (writer keeps up while discarding)"
"$C" teardown >/dev/null || fail "teardown after c2 failed"

# ---- Check 3: a present, draining client loses nothing ----------------------
DEV3="$TMPD/dev3"
"$SIM" pty --echo --link "$DEV3" --timeout-ms 60000 >"$TMPD/dev3.log" 2>&1 &
PIDS+=($!)
bash "$WAIT" "test -e '$DEV3'" 5 0.05 || fail "dev3 never appeared"

TTY3="$TMPD/console3"
cat > "$TMPD/c3.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY3"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV3"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/c3.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load c3 failed"; }

"$SIM" client --path "$TTY3" --send seeded:64KiB --expect echo --seed 9 --timeout-ms 15000 \
  | jq -e '.pass==true and .received==65536' >/dev/null \
  || { cat "$TMPD/daemon.log" "$TMPD/dev3.log"; fail "echo round-trip failed with a present client"; }

# The client was present and kept up for the whole transfer: no drops of either
# kind at the PTY boundary.
[ "$(nstate console discarded_no_client)" = "0" ] \
  || fail "discarded_no_client must stay 0 while a client is present"
[ "$(nstate console dropped_slow_consumer)" = "0" ] \
  || fail "dropped_slow_consumer must stay 0 for a draining client"

"$C" shutdown >/dev/null
echo '{"check":"phase3-counters","pass":true}'
