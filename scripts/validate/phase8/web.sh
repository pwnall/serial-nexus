#!/usr/bin/env bash
# Web console track validation (design §17/§15.29 / plan §11.3): the serialnexusweb
# HTTP/WebSocket server. Asserts the §15.29 security gates and the browser-facing
# byte path end to end, on a no-hardware rig (§15.17):
#   1. every request needs the session token — no cookie → 401;
#   2. the Host header is validated (DNS-rebinding defense) — bad Host → 403;
#   3. the bootstrap URL (?token=) sets the cookie (302); a wrong token → 401;
#   4. a valid cookie serves the app (200);
#   5. a non-loopback --bind without --tls/--insecure-bind exits with the documented
#      error; and --tls (not yet built) is refused clearly;
#   6. the WebSocket byte stream for a tapped console checksums against the seeded
#      source, end to end (headless client → server → daemon → device).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-web\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p serialnexusweb -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
W="$REPO_ROOT/target/debug/serialnexusweb"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8web.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; GO="$TMPD/go"
TOK="testtoken0123456789abcdef"
N=262144; SEED=31
mkdir -p "$TMPD/logs"
cleanup() {
  for p in DPID SRCPID WEBPID WSPID TLSPID NLTLSPID; do kill "${!p:-}" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# Seeded, gated device, plus a byte-exact log anchor so the serial is always fed.
"$SIM" pty --source --bytes "$N" --seed "$SEED" --wait-file "$GO" \
  --link "$DEV" --hold-ms 2000 --timeout-ms 40000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "free-for-all"
hostward_buffer = 8192
[[node]]
type = "log"
name = "logx"
directory = "$TMPD/logs"
filename = "serial.log"
[[edge]]
a = "usb0"
b = "logx"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# --- (5) bind policy (§15.29) — checked before starting the real server ----------
# A non-loopback plaintext bind is refused with the documented error.
if "$W" --bind 0.0.0.0:0 --token "$TOK" --socket "$SOCK" >"$TMPD/nl.out" 2>"$TMPD/nl.err"; then
  fail "a non-loopback --bind without --tls/--insecure-bind should have failed"
fi
grep -qiE "insecure-bind|15.29|loopback" "$TMPD/nl.err" || { cat "$TMPD/nl.err"; fail "non-loopback bind error lacked the documented reason"; }

# --- start the real server on an ephemeral loopback port -------------------------
"$W" --bind 127.0.0.1:0 --token "$TOK" --socket "$SOCK" >"$TMPD/web.out" 2>"$TMPD/web.err" &
WEBPID=$!
bash "$WAIT" "grep -qE 'http://127.0.0.1:[0-9]+/' '$TMPD/web.out'" 5 0.05 \
  || { cat "$TMPD/web.err"; fail "web server never printed its URL"; }
PORT=$(sed -n 's#.*http://127.0.0.1:\([0-9]\+\)/.*#\1#p' "$TMPD/web.out" | head -1)
[ -n "$PORT" ] || { cat "$TMPD/web.out"; fail "could not parse the bound port"; }
BASE="http://127.0.0.1:$PORT"
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }

# --- (1) no token → 401 ----------------------------------------------------------
[ "$(code "$BASE/app.js")" = 401 ] || fail "GET /app.js without a token should be 401"
# --- (2) bad Host → 403 (checked before the token) -------------------------------
[ "$(code -H 'Host: evil.example' "$BASE/?token=$TOK")" = 403 ] || fail "a bad Host should be 403"
# --- (3) bootstrap: right token → 302 (+cookie); wrong token → 401 ---------------
[ "$(code "$BASE/?token=$TOK")" = 302 ] || fail "the bootstrap URL with the token should 302"
[ "$(code "$BASE/?token=wrong")" = 401 ] || fail "the bootstrap URL with a wrong token should 401"
# --- (4) valid cookie → 200 ------------------------------------------------------
[ "$(code -b "nexus_session=$TOK" "$BASE/app.js")" = 200 ] || fail "GET /app.js with the cookie should be 200"
[ "$(code -b "nexus_session=$TOK" "$BASE/")" = 200 ] || fail "GET / with the cookie should be 200"

# --- (item 4) API-level: the bridge relays state and enforces the §17 denylist ---
WS_STATE=$(timeout 10 "$W" wsclient --url "ws://127.0.0.1:$PORT/ws" --token "$TOK" --rpc state 2>/dev/null)
echo "$WS_STATE" | jq -e '.result.nodes | map(.name) | index("usb0") != null' >/dev/null \
  || { echo "$WS_STATE"; fail "state via the WS bridge did not list the usb0 console"; }
# The console list from the bridge matches the daemon's directly.
WS_NODES=$(echo "$WS_STATE" | jq -c '.result.nodes | map(.name) | sort')
D_NODES=$("$C" --json state | jq -c '.nodes | map(.name) | sort')
[ "$WS_NODES" = "$D_NODES" ] || fail "console list via the WS ($WS_NODES) != the daemon's ($D_NODES)"
# A graph-mutating verb is refused at the bridge (§17: the web console never mutates
# the graph), never reaching the daemon.
WS_LOAD=$(timeout 10 "$W" wsclient --url "ws://127.0.0.1:$PORT/ws" --token "$TOK" --rpc load 2>/dev/null)
echo "$WS_LOAD" | jq -e '.error' >/dev/null \
  || { echo "$WS_LOAD"; fail "a load via the WS bridge should be refused (§17)"; }

# --- (6) WebSocket byte stream, end to end ---------------------------------------
timeout 40 "$W" wsclient --url "ws://127.0.0.1:$PORT/ws" --token "$TOK" \
  --endpoint usb0 --bytes "$N" > "$TMPD/ws.out" 2>"$TMPD/ws.err" &
WSPID=$!
# The server's bridge opened a daemon tap on usb0; wait for it to register+activate.
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|map(select(.endpoint==\"usb0\"))|length)>=1'" 8 0.1 \
  || { cat "$TMPD/web.err" "$TMPD/ws.err" "$TMPD/daemon.log"; fail "the web tap did not register in the daemon"; }

# Release the source: N bytes flow device → serial → {log, web tap}.
touch "$GO"
wait "$WSPID" 2>/dev/null; WSST=$?; WSPID=
[ "$WSST" = 0 ] || { cat "$TMPD/ws.err" "$TMPD/web.err"; fail "wsclient exited $WSST before reading $N bytes"; }
wait "$SRCPID" 2>/dev/null; SRCPID=

SRC_SHA=$(jq -r '.sha256 // ""' "$TMPD/src.json")
WS_SHA=$(sha256sum "$TMPD/ws.out" | cut -d' ' -f1)
WS_LEN=$(stat -c %s "$TMPD/ws.out")
[ -n "$SRC_SHA" ] || { cat "$TMPD/src.json"; fail "source produced no verdict"; }
[ "$WS_LEN" = "$N" ] || fail "the WS stream delivered $WS_LEN bytes, expected $N"
[ "$WS_SHA" = "$SRC_SHA" ] || fail "the WS byte stream checksum != the source (browser path corrupted or dropped bytes)"

# --- (item 6) the TLS tier (§15.29 / §11.6) --------------------------------------
"$W" --tls --bind 127.0.0.1:0 --token "$TOK" --socket "$SOCK" \
  --tls-cert "$TMPD/tls.crt" --tls-key "$TMPD/tls.key" >"$TMPD/tlsweb.out" 2>"$TMPD/tlsweb.err" &
TLSPID=$!
bash "$WAIT" "grep -qE 'https://127.0.0.1:[0-9]+/' '$TMPD/tlsweb.out'" 8 0.1 \
  || { cat "$TMPD/tlsweb.err"; fail "TLS server never listened"; }
TPORT=$(sed -n 's#.*https://127.0.0.1:\([0-9]\+\)/.*#\1#p' "$TMPD/tlsweb.out" | head -1)
[ "$(stat -c %a "$TMPD/tls.key" 2>/dev/null)" = 600 ] || fail "the generated TLS key is not mode 0600"
tcode() { curl -s -o /dev/null -w '%{http_code}' --cacert "$TMPD/tls.crt" --resolve "localhost:$TPORT:127.0.0.1" "$@"; }
[ "$(tcode "https://localhost:$TPORT/app.js")" = 401 ] || fail "TLS: a request without the token should be 401"
[ "$(tcode -b "nexus_session=$TOK" "https://localhost:$TPORT/app.js")" = 200 ] || fail "TLS: a valid cookie should be 200"
# The encryption is real: an untrusted self-signed cert (no --cacert) is refused.
if curl -s -o /dev/null --resolve "localhost:$TPORT:127.0.0.1" "https://localhost:$TPORT/app.js"; then
  fail "TLS: an untrusted self-signed cert should be rejected without --cacert"
fi
# A non-loopback bind is permitted WITH --tls (§15.29 tier 2) — the same bind the
# plaintext check above refused.
"$W" --tls --bind 0.0.0.0:0 --token "$TOK" --socket "$SOCK" \
  --tls-cert "$TMPD/nl.crt" --tls-key "$TMPD/nl.key" >"$TMPD/nltls.out" 2>"$TMPD/nltls.err" &
NLTLSPID=$!
bash "$WAIT" "grep -qE 'https://' '$TMPD/nltls.out'" 8 0.1 \
  || { cat "$TMPD/nltls.err"; fail "--tls should permit a non-loopback bind (§15.29 tier 2)"; }
kill "$NLTLSPID" 2>/dev/null; NLTLSPID=
kill "$TLSPID" 2>/dev/null; TLSPID=

"$C" shutdown >/dev/null
echo '{"check":"phase8-web","pass":true}'
