#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 5): head-of-line, documented by test.
# The v1 wire has whole-connection (not per-channel) targetward flow control (§9):
# when the peer stops reading, EVERY channel's targetward freezes together. This
# test pins two properties: (a) §15.22 direction independence — hostward keeps
# advancing while targetward is wedged, because the leg's two socket directions are
# concurrently polled; and (b) a fully-stalled peer freezes every targetward channel
# together (the connection's targetward flowed, then froze with neither channel
# completing — one may sit at 0, itself blocked by the other on the shared socket).
# (Discriminating whole-connection coupling from independent per-channel stalls would
# need a peer that drains the socket but withholds one channel's credit; deferred.)
#
# A `nexus-sim wire` peer streams sustained hostward on all channels but never
# reads (a stalled sink): the daemon leg's targetward backs up into the socket
# buffer. Writers on c0 and c1 both freeze together; a c2 hostward drain keeps
# advancing throughout.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-head-of-line\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p6l.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
LEG="$TMPD/leg.sock"
cleanup() {
  for p in "${DPID:-}" "${WIRE:-}" "${DRAIN:-}" "${W0:-}" "${W1:-}"; do kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# A receiving leg with three channels fanning out to local PTYs.
{
  echo '[[node]]'; echo 'type = "leg"'; echo 'name = "downlink"'; echo 'faces = "host"'
  echo 'transport = "unix"'; echo 'role = "listen"'; echo "address = \"$LEG\""
  echo 'arbitration = "free-for-all"'; echo 'channels = ["c0", "c1", "c2"]'
  for i in 0 1 2; do echo '[[node]]'; echo 'type = "pty"'; echo "name = \"p$i\""; echo "path = \"$TMPD/p$i\""; done
  for i in 0 1 2; do echo '[[edge]]'; echo "a = \"downlink/c$i\""; echo "b = \"p$i\""; echo 'write_mode = "on-demand"'; done
} > "$TMPD/g.toml"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# The stalled peer: streams sustained hostward on all channels, never reads.
"$SIM" wire --transport unix --address "$LEG" \
  --announce c0 --announce c1 --announce c2 --stall --hold-ms 8000 --timeout-ms 10000 \
  >"$TMPD/wire.json" 2>&1 & WIRE=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection==\"connected\"'" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "leg never connected"; }

# Drain c2's hostward, and confirm it flows despite the peer stalling reads.
"$SIM" client --path "$TMPD/p2" --drain --quiet-ms 20000 --timeout-ms 30000 >"$TMPD/drain.json" 2>&1 & DRAIN=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.channels.c2.delivered_hostward>32768'" 8 0.1 \
  || { "$C" --json state >&2; fail "c2 hostward never started flowing while the peer stalled reads"; }

# Two operators write a large targetward burst on c0 and c1. The peer never reads,
# so the socket send buffer fills and the leg's SEND wedges — both channels'
# targetward freeze together.
"$SIM" client --path "$TMPD/p0" --send seeded:2MiB --seed 10 --timeout-ms 20000 >"$TMPD/w0.json" 2>&1 & W0=$!
"$SIM" client --path "$TMPD/p1" --send seeded:2MiB --seed 20 --timeout-ms 20000 >"$TMPD/w1.json" 2>&1 & W1=$!
sleep 1.5

# Sample the frozen targetward and the still-advancing hostward.
s0a=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c0.accepted_targetward')
s1a=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c1.accepted_targetward')
h2a=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c2.delivered_hostward')
sleep 1.5
s0b=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c0.accepted_targetward')
s1b=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c1.accepted_targetward')
h2b=$("$C" --json state | jq '.nodes[]|select(.name=="downlink")|.channels.c2.delivered_hostward')

# Targetward flowed at the connection level (a positive lower bound on the total, so
# "frozen" cannot be satisfied by a totally-broken targetward path that moved zero
# bytes). We assert the SUM, not each channel: under a fully-stalled peer whichever
# channel wins the race wedges the shared socket, so the other can legitimately sit
# at 0 — itself a head-of-line manifestation (it has data queued but cannot send it).
suma=$((s0a + s1a))
[ "$suma" -gt 0 ] 2>/dev/null \
  || { echo "c0=$s0a c1=$s1a" >&2; fail "targetward never flowed at all (path broken, not wedged)"; }

# Both channels' targetward froze together (no progress over the interval), and
# neither reached its full 2 MiB — the whole-connection head-of-line stall.
[ "$s0a" = "$s0b" ] || { echo "c0 accepted $s0a -> $s0b" >&2; fail "c0 targetward did not freeze (still advancing)"; }
[ "$s1a" = "$s1b" ] || { echo "c1 accepted $s1a -> $s1b" >&2; fail "c1 targetward did not freeze (still advancing)"; }
[ "$s0b" -lt 2097152 ] && [ "$s1b" -lt 2097152 ] \
  || { echo "c0=$s0b c1=$s1b" >&2; fail "targetward should be blocked below the full 2 MiB"; }

# Hostward kept advancing across the same interval — the two socket directions are
# independent (the §9 property this test pins).
[ "$h2b" -gt "$h2a" ] 2>/dev/null \
  || { echo "c2 hostward $h2a -> $h2b" >&2; fail "hostward did not keep advancing during the targetward freeze"; }

# It is a stall, not a disconnect: the leg is still connected.
"$C" --json state | jq -e '.nodes[]|select(.name=="downlink")|.connection=="connected"' >/dev/null \
  || { "$C" --json state >&2; fail "the leg should stay connected during head-of-line blocking"; }

"$C" shutdown >/dev/null
echo '{"check":"phase6-head-of-line","pass":true}'
