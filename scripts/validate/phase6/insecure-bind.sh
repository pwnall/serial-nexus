#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 6): the loopback-only security gate.
# A leg bound/dialed to a non-loopback address without `insecure_bind` is a
# structural load error (§7.4/§9) — the load fails with the offender named and
# nothing is created; with the flag it loads (and the node comes up faulted, an
# environmental state, not a load failure). Config-level only: no sockets.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-insecure-bind\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p6i.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# 1. A non-loopback tcp bind WITHOUT insecure_bind: structural refusal (§7.4).
cat > "$TMPD/bad.toml" <<EOF
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "10.0.0.5:9999"
channels = ["console"]
EOF
# The CLI reports an RPC error as text on stderr and exits non-zero; assert the
# structural code, the offender, and the reason are all named.
out=$("$C" --json load "$TMPD/bad.toml" 2>&1) && fail "non-loopback bind should have failed to load"
echo "$out" | grep -q -- "-32002" || { echo "$out" >&2; fail "expected structural error code -32002"; }
echo "$out" | grep -q "uplink" || { echo "$out" >&2; fail "structural error must name the offending node"; }
echo "$out" | grep -q "insecure_bind" || { echo "$out" >&2; fail "structural error must name insecure_bind"; }
# Nothing created: the graph is still empty.
"$C" --json state | jq -e '.nodes == []' >/dev/null \
  || { "$C" --json state >&2; fail "a refused load must create nothing (empty graph)"; }

# 2. The same address WITH insecure_bind loads (the node then faults on the bind,
#    an environmental state — the load itself succeeds and the node is present).
cat > "$TMPD/ok.toml" <<EOF
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "10.0.0.5:9999"
insecure_bind = true
channels = ["console"]
EOF
"$C" --json load "$TMPD/ok.toml" | jq -e '.loaded == 1' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "insecure_bind=true must let the leg load"; }
"$C" --json state | jq -e '.nodes | map(select(.name=="uplink")) | length == 1' >/dev/null \
  || { "$C" --json state >&2; fail "the leg node must be present after an insecure load"; }
# The §9 named footgun is a visible, greppable confession in `state` (§15.12).
"$C" --json state | jq -e '.nodes[]|select(.name=="uplink")|.insecure_bind==true' >/dev/null \
  || { "$C" --json state >&2; fail "an insecure leg must carry the insecure_bind marker in state"; }

# 3. A loopback bind loads without the flag (the default, safe case).
"$C" teardown >/dev/null
cat > "$TMPD/loop.toml" <<EOF
[[node]]
type = "leg"
name = "uplink"
faces = "host"
transport = "tcp"
role = "listen"
address = "127.0.0.1:0"
channels = ["console"]
EOF
"$C" --json load "$TMPD/loop.toml" | jq -e '.loaded == 1' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "a loopback leg must load without insecure_bind"; }

"$C" shutdown >/dev/null
echo '{"check":"phase6-insecure-bind","pass":true}'
