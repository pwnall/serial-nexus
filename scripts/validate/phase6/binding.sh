#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 2): binding never mutates the graph.
# A peer announces a channel set over the wire; the receiving leg reconciles it
# against its configured channels into bound / waiting / unbound (§8) — a
# configured+announced channel is `bound`, configured-but-unannounced is
# `waiting`, announced-but-unconfigured is `unbound` (visible state, no endpoint).
# Node/endpoint counts before and after the connection are equal.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-binding\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p6b.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
LEG="$TMPD/leg.sock"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${WPID:-}" ] && kill "$WPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# A receiving leg configured with two channels (console, trace).
cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "$LEG"
arbitration = "free-for-all"
channels = ["console", "trace"]
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Node count before the peer connects.
before=$("$C" --json state | jq '.nodes | length')

# The peer announces {console, extra}: console is configured (→ bound), trace is
# configured-but-unannounced (→ waiting), extra is announced-but-unconfigured
# (→ unbound). Hold the connection while we inspect.
"$SIM" wire --transport unix --address "$LEG" \
  --announce console --announce extra --hold-ms 4000 --timeout-ms 5000 \
  >"$TMPD/wire.json" 2>&1 &
WPID=$!

bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection==\"connected\"'" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "leg never connected"; }

# Binding reconciliation.
st=$("$C" --json state)
echo "$st" | jq -e '.nodes[]|select(.name=="downlink")|.channels.console.binding=="bound"' >/dev/null \
  || { echo "$st" >&2; fail "console should be bound (configured + announced)"; }
echo "$st" | jq -e '.nodes[]|select(.name=="downlink")|.channels.trace.binding=="waiting"' >/dev/null \
  || { echo "$st" >&2; fail "trace should be waiting (configured, not announced)"; }
echo "$st" | jq -e '.nodes[]|select(.name=="downlink")|.channels.extra.binding=="unbound"' >/dev/null \
  || { echo "$st" >&2; fail "extra should be unbound (announced, not configured)"; }

# Announcements never grow the graph: node count unchanged, and the unbound
# channel has no endpoint (no lock, no edges) — it exists only as state.
after=$("$C" --json state | jq '.nodes | length')
[ "$before" = "$after" ] || fail "node count changed from announcements ($before -> $after)"
echo "$st" | jq -e '.nodes[]|select(.name=="downlink")|.channels.extra|has("lock")|not' >/dev/null \
  || { echo "$st" >&2; fail "an unbound channel must have no endpoint/lock (§8)"; }
# The configured (bound/waiting) channels DO carry a host-facing lock.
echo "$st" | jq -e '.nodes[]|select(.name=="downlink")|.channels.console|has("lock")' >/dev/null \
  || { echo "$st" >&2; fail "a configured channel must carry its host-facing lock"; }

kill "$WPID" 2>/dev/null
"$C" shutdown >/dev/null
echo '{"check":"phase6-binding","pass":true}'
