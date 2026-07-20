#!/usr/bin/env bash
# Phase 2 validation (control-plane slice): boot the daemon, load-on-empty with
# structural atomicity, truthful state, dump→load→dump round-trip, JSON-RPC
# hygiene, and socket permissions (design §10, §11). The data-path and presence
# checks are added with slice 2.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase2-control-plane\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"

# A SHORT socket dir — Unix socket paths are bounded by SUN_LEN (~108 bytes).
TMPD=$(mktemp -d /tmp/snx-p2.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
cleanup() { [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null; rm -rf "$TMPD"; }
trap cleanup EXIT

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$REPO_ROOT/scripts/lib/wait-for.sh" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# Socket permissions ARE the authorization model (§10): 0600.
[ "$(stat -c '%a' "$SOCK")" = "600" ] || fail "socket perms are $(stat -c '%a' "$SOCK"), want 600"

# Structural rejection: a host↔host edge (two serial nodes) — rejected, nothing
# created (§11 structural atomicity).
cat > "$TMPD/broken.toml" <<EOF
[[node]]
type = "serial"
name = "a"
device = "$TMPD/x"
[[node]]
type = "serial"
name = "b"
device = "$TMPD/y"
[[edge]]
a = "a"
b = "b"
EOF
"$C" load "$TMPD/broken.toml" >/dev/null 2>&1 && fail "structurally-invalid config was accepted"
[ "$("$C" --json state | jq '.nodes | length')" = "0" ] || fail "rejected load left nodes behind"

# Valid load + truthful state: pty active, serial waiting (device absent).
cat > "$TMPD/demo.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TMPD/console"
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/absent-device"
[[edge]]
a = "usb0"
b = "console"
EOF
"$C" load "$TMPD/demo.toml" >/dev/null || fail "valid load failed"
"$C" --json state | jq -e '.nodes | length == 2' >/dev/null || fail "expected 2 nodes"
"$C" --json state | jq -e '.nodes[] | select(.name=="console") | .status=="active"' >/dev/null || fail "console not active"
"$C" --json state | jq -e '.nodes[] | select(.name=="usb0") | .status=="waiting"' >/dev/null || fail "usb0 not waiting"
[ -L "$TMPD/console" ] || fail "pty symlink not created"

# Load-on-empty: a second load is refused (§11).
"$C" load "$TMPD/demo.toml" >/dev/null 2>&1 && fail "second load on a non-empty graph was accepted"

# JSON-RPC hygiene (§10): method-not-found and batch rejection.
mnf="$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"bogus"}' | nc -U -q1 "$SOCK" | jq '.error.code')"
[ "$mnf" = "-32601" ] || fail "method-not-found returned $mnf, want -32601"
batch="$(printf '%s\n' '[{"jsonrpc":"2.0","id":1,"method":"state"}]' | nc -U -q1 "$SOCK" | jq '.error.code')"
[ "$batch" = "-32600" ] || fail "batch returned $batch, want -32600"

# Round-trip: dump → teardown → load → dump, semantically equal (§11).
"$C" dump > "$TMPD/dump1.toml"
"$C" teardown >/dev/null || fail "teardown failed"
[ "$("$C" --json state | jq '.nodes | length')" = "0" ] || fail "teardown left nodes"
"$C" load "$TMPD/dump1.toml" >/dev/null || fail "reload of dump failed"
"$C" dump > "$TMPD/dump2.toml"
bash "$REPO_ROOT/scripts/lib/semantic-diff.sh" "$TMPD/dump1.toml" "$TMPD/dump2.toml" >/dev/null || fail "dump→load→dump mismatch"

"$C" shutdown >/dev/null
echo '{"check":"phase2-control-plane","pass":true}'
