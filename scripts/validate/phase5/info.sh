#!/usr/bin/env bash
# Plan §10.2 / §15.26: the `info` verb reports the daemon's capability surface —
# its version, the wire and envelope protocol versions, and the registered codec
# names — so tools discover what a (possibly custom) daemon supports rather than
# assume it. And an unknown codec in configuration fails STRUCTURALLY, nothing
# created, with the available list in the error payload.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"info\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-info.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# (1) The info verb reports the full capability surface.
INFO="$("$C" --json info)"
printf '%s' "$INFO" | jq -e '
  (.codecs | index("reference"))
  and (.wire_version | numbers)
  and (.envelope_version | numbers)
  and (.daemon_version | type == "string")
' >/dev/null || { echo "$INFO"; fail "info payload incomplete"; }

# (2) An unknown codec is structural, nothing created, with the available list in
# the error payload (the CLI prints error.data on stderr).
cat > "$TMPD/unknown.toml" <<'EOF'
[[node]]
type = "codec"
name = "mux"
codec = "does-not-exist"
faces = "target"
channels = ["c0"]
EOF
if "$C" load "$TMPD/unknown.toml" >/dev/null 2>"$TMPD/err.txt"; then
  fail "load with an unknown codec should have failed"
fi
if ! grep -q '"available"' "$TMPD/err.txt" || ! grep -q 'reference' "$TMPD/err.txt"; then
  cat "$TMPD/err.txt"
  fail "unknown-codec error missing the available list"
fi
"$C" --json state | jq -e '.nodes == []' >/dev/null \
  || { "$C" --json state; fail "a rejected load created nodes (must be atomic)"; }

"$C" shutdown >/dev/null 2>&1
echo '{"check":"info","pass":true}'
