#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 3): version and hostility handling —
# the §9 clause-6 clean-refusal contract, driven by `nexus-sim wire`. A version
# mismatch, a bad magic, an oversize frame, and an unknown frame type each leave
# the leg faulted with the reason surfaced in state (never a panic, never a silent
# drop), and the leg heals — a subsequent conforming peer binds. The suite is
# parameterized over the framing by construction (the hostility is threaded as
# wire-mode flags, not hardcoded bytes in this script).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-hostility\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p6h.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
LEG="$TMPD/leg.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "$LEG"
channels = ["console"]
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Run one hostile case: the sim elicits a refusal (peer_closed), and the daemon
# leg's status must go faulted with the given reason substring.
hostile_case() {
  local name="$1" reason_sub="$2"; shift 2
  "$SIM" wire --transport unix --address "$LEG" --announce console \
    --hold-ms 500 --timeout-ms 4000 "$@" >"$TMPD/$name.json" 2>&1 \
    || { cat "$TMPD/$name.json" >&2; fail "$name: sim did not elicit a clean refusal"; }
  jq -e '.pass==true and .peer_closed==true' "$TMPD/$name.json" >/dev/null \
    || { cat "$TMPD/$name.json" >&2; fail "$name: daemon did not close the connection"; }
  bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.status==\"faulted\" and (.reason|test(\"$reason_sub\"))'" 5 0.1 \
    || { "$C" --json state >&2; fail "$name: leg not faulted with reason matching /$reason_sub/"; }
}

# 1. Version mismatch: the version must appear in the fault reason.
hostile_case "version" "999" --hello-version 999
# 2. Bad magic: a not-our-protocol peer.
hostile_case "magic" "magic" --bad-magic
# 3. Oversize frame (§9 clause 4): refused on the length prefix.
hostile_case "oversize" "exceeds" --oversize-frame
# 4. Unknown frame type (§9 clause 6).
hostile_case "unknown" "unknown frame type" --unknown-type

# Heal: after all that hostility, a conforming peer binds cleanly (faulted-and-wait
# self-heals, §7.4).
"$SIM" wire --transport unix --address "$LEG" --announce console \
  --hold-ms 2500 --timeout-ms 3500 >"$TMPD/good.json" 2>&1 &
GOODPID=$!
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.status==\"active\" and .channels.console.binding==\"bound\"'" 5 0.1 \
  || { "$C" --json state >&2; fail "leg did not heal for a conforming peer"; }
kill "$GOODPID" 2>/dev/null

"$C" shutdown >/dev/null
echo '{"check":"phase6-hostility","pass":true}'
