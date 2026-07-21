#!/usr/bin/env bash
# Phase 7 validation (plan §Phase 7, item 7): doctor probe P5 (rig discovery and
# certification, §15.21) without a bench. Against a nexus-sim nullmodem pair, a
# loopback (pty --echo), and a dangling port (pty --stall), discovery classifies
# each correctly — pairs verified in BOTH directions — and characterization
# reports skipped(not a UART) for the pts, so P5's logic never waits for hardware.
# On adapter machines the full certificate populates; skipped(no adapter) is a
# valid CI verdict, a failing probe is not.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase7-p5\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p nexus-doctor -p nexus-sim || fail "build failed"
DOC="$REPO_ROOT/target/debug/nexus-doctor"; SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p7p.XXXXXX) || fail "mktemp"
PIDS=()
cleanup() { for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMPD"; }
trap cleanup EXIT

# A crossed pair, a loopback, and a dangling port — all software, no hardware.
# The paired/dangling classes are exercised in one run; the software echo loopback
# is exercised in its own run. A `pty --echo` peer competes for the CPU to reflect
# each byte, and mixing it in the same run as other active peers is timing-sensitive
# on a loaded box — a sim/scheduling artifact, not a P5 logic issue (a real TX↔RX
# jumper reflects in hardware, instantly, with no process to schedule). Splitting
# keeps the test deterministic while still validating every classification.
"$SIM" nullmodem --link-a "$TMPD/pair_a" --link-b "$TMPD/pair_b" --timeout-ms 30000 >/dev/null 2>&1 & PIDS+=($!)
"$SIM" pty --stall --link "$TMPD/dangle" --timeout-ms 30000 >/dev/null 2>&1 & PIDS+=($!)
for f in pair_a pair_b dangle; do
  bash "$WAIT" "test -e '$TMPD/$f'" 5 0.05 || fail "$f never appeared"
done
sleep 0.3

# ---- Run 1: paired (both directions) + dangling ------------------------------
"$DOC" --json --port "$TMPD/pair_a" --port "$TMPD/pair_b" --port "$TMPD/dangle" \
  > "$TMPD/r1.json" 2>/dev/null || true
jq -e '.probes[]|select(.id=="P5")|.status=="supported"' "$TMPD/r1.json" >/dev/null \
  || { jq -c '.probes[]|select(.id=="P5")' "$TMPD/r1.json"; fail "P5 run 1 not supported"; }
obs1() { jq -r --arg k "$TMPD/$1" '.probes[]|select(.id=="P5")|.observations[]|select(.key==$k)|.value' "$TMPD/r1.json"; }
echo "$(obs1 pair_a)" | grep -q "paired with $TMPD/pair_b" || fail "pair_a not paired (got: $(obs1 pair_a))"
echo "$(obs1 pair_b)" | grep -q "paired with $TMPD/pair_a" || fail "pair_b not paired both-directions (got: $(obs1 pair_b))"
echo "$(obs1 dangle)" | grep -qi 'dangling' || fail "dangle not classified dangling (got: $(obs1 dangle))"
# Characterization skips the non-UART pts.
cert1() { jq -r --arg k "$TMPD/$1 cert" '.probes[]|select(.id=="P5")|.observations[]|select(.key==$k)|.value' "$TMPD/r1.json"; }
for f in pair_a pair_b dangle; do
  echo "$(cert1 "$f")" | grep -qi 'not a UART' || fail "$f characterization not skipped(not a UART) (got: $(cert1 "$f"))"
done

# ---- Run 2: loopback ---------------------------------------------------------
"$SIM" pty --echo --link "$TMPD/loop" --hold-ms 30000 --timeout-ms 30000 >/dev/null 2>&1 & PIDS+=($!)
bash "$WAIT" "test -e '$TMPD/loop'" 5 0.05 || fail "loop never appeared"
sleep 0.3
"$DOC" --json --port "$TMPD/loop" > "$TMPD/r2.json" 2>/dev/null || true
jq -e '.probes[]|select(.id=="P5")|.status=="supported"' "$TMPD/r2.json" >/dev/null \
  || { jq -c '.probes[]|select(.id=="P5")' "$TMPD/r2.json"; fail "P5 run 2 not supported"; }
LOOPV=$(jq -r --arg k "$TMPD/loop" '.probes[]|select(.id=="P5")|.observations[]|select(.key==$k)|.value' "$TMPD/r2.json")
echo "$LOOPV" | grep -qi 'loopback' || fail "loopback not classified (got: $LOOPV)"

echo '{"check":"phase7-p5","pass":true}'
