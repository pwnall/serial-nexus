#!/usr/bin/env bash
# Plan §10.3 / §15.26: the out-of-tree codec template builds from the CONSUMER's
# position (its own workspace manifest, path deps standing in for version pins),
# its custom daemon serves the `acme` codec alongside the built-ins, and a config
# naming `acme` loads — so the embedding pattern is proven to compile per push
# rather than promised. Also runs the template's own conformance-kit test.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"external-codec\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

TEMPLATE="$REPO_ROOT/examples/external-codec"

# Build the closed-repo stand-in from its own manifest, and run its conformance
# test against the codec-api kit (§15.26 / plan §10.4) from the consumer position.
cargo build -q --manifest-path "$TEMPLATE/Cargo.toml" || fail "template build failed"
cargo test -q -p acme-codec --features conformance \
  --manifest-path "$TEMPLATE/Cargo.toml" >/dev/null 2>&1 \
  || fail "acme codec conformance-kit test failed"
cargo build -q -p serialnexusctl || fail "ctl build failed"

D="$TEMPLATE/target/debug/acme-daemon"
C="$REPO_ROOT/target/debug/serialnexusctl"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
[ -x "$D" ] || fail "acme-daemon binary not built"

TMPD=$(mktemp -d /tmp/snx-ext.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "acme-daemon socket never appeared"; }

# The custom daemon reports its own codec alongside the built-ins (the CLI, unchanged,
# speaks to it — §15.16).
"$C" --json info | jq -e '.codecs | index("acme")' >/dev/null \
  || { "$C" --json info; fail "acme codec not listed by info"; }
"$C" --json info | jq -e '.codecs | index("reference")' >/dev/null \
  || fail "the built-in reference codec is missing from the custom daemon"

# A config naming the acme codec loads (it comes up waiting: no attached mux upstream).
cat > "$TMPD/acme.toml" <<'EOF'
[[node]]
type = "codec"
name = "mux"
codec = "acme"
faces = "target"
channels = ["console"]
EOF
"$C" load "$TMPD/acme.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "acme config failed to load"; }
"$C" --json state | jq -e '.nodes[] | select(.name == "mux") | .codec == "acme"' >/dev/null \
  || { "$C" --json state; fail "the acme codec node did not load"; }

"$C" shutdown >/dev/null 2>&1
echo '{"check":"external-codec","pass":true}'
