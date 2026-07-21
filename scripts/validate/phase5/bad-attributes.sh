#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 5): bad codec configuration is
# structural. A codec whose attribute table the codec rejects — or an unknown
# codec name — fails the load with the codec's own error, and nothing is created
# (§8, §11): the graph stays empty.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase5-bad-attributes\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p5b.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

empty() { "$C" --json state | jq -e '.nodes==[]' >/dev/null; }
empty || fail "graph not empty at start"

# (1) The reference codec takes no attributes; a config bearing one is rejected,
# nothing created.
cat > "$TMPD/attr.toml" <<'EOF'
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
attributes = { misspelled_option = true }
EOF
if "$C" load "$TMPD/attr.toml" 2>"$TMPD/attr.err"; then fail "load with a bad codec attribute should have failed"; fi
grep -qi 'reference' "$TMPD/attr.err" || { cat "$TMPD/attr.err"; fail "rejection did not mention the codec's own error"; }
empty || { "$C" --json state; fail "a rejected load created nodes (must be atomic, nothing created)"; }

# (2) An unknown codec name is likewise structural.
cat > "$TMPD/unknown.toml" <<'EOF'
[[node]]
type = "codec"
name = "mux"
codec = "does-not-exist"
faces = "target"
channels = ["c0"]
EOF
if "$C" load "$TMPD/unknown.toml" 2>"$TMPD/unknown.err"; then fail "load with an unknown codec should have failed"; fi
grep -qi 'unknown codec' "$TMPD/unknown.err" || { cat "$TMPD/unknown.err"; fail "unknown-codec rejection wrong"; }
empty || { "$C" --json state; fail "a rejected load created nodes"; }

# (3) A valid reference codec (no attributes) still loads — the gate is the bad
# attribute, not codec nodes in general.
cat > "$TMPD/ok.toml" <<'EOF'
[[node]]
type = "codec"
name = "mux"
codec = "reference"
faces = "target"
channels = ["c0"]
EOF
"$C" load "$TMPD/ok.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "a valid codec config failed to load"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="mux")|.codec=="reference"' >/dev/null || fail "valid codec did not load"

"$C" shutdown >/dev/null
echo '{"check":"phase5-bad-attributes","pass":true}'
