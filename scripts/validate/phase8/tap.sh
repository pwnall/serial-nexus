#!/usr/bin/env bash
# Web console track validation (design §17 / plan §11.1): the tap.
#
# A tap is a connection-scoped, read-only dynamic attachment on a host-facing
# endpoint (the `never` write mode in dynamic form). This asserts, on a no-hardware
# rig (§15.17), that:
#   1. a tap faithfully mirrors the endpoint's hostward stream — its checksum
#      equals the seeded source's AND a co-attached log consumer's, byte-exact;
#   2. `dump` is unchanged while a tap is open — taps are state, never config (§8);
#   3. dropping the tap's connection detaches it (state shows zero taps).
#
# The seeded source is gated on a readiness file so it cannot outrun a
# not-yet-draining consumer (plan §3, presence != readiness), making the byte-exact
# comparison deterministic.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-tap\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8tap.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
GO="$TMPD/go"
N=262144            # 256 KiB — well under the tap queue bound, so no drops here
SEED=7
mkdir -p "$TMPD/logs"
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SRCPID:-}" ] && kill "$SRCPID" 2>/dev/null
  [ -n "${TAPPID:-}" ] && kill "$TAPPID" 2>/dev/null
  [ -n "${WATCHPID:-}" ] && kill "$WATCHPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The seeded device, gated on $GO so it writes only once the tap and log are ready.
"$SIM" pty --source --bytes "$N" --seed "$SEED" --wait-file "$GO" \
  --link "$DEV" --hold-ms 2000 --timeout-ms 40000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# serial → log: the log is an always-attached, byte-exact co-consumer of the
# hostward stream. The tap will attach to the same serial endpoint.
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

# dump BEFORE any tap: the baseline the open-tap dump must match exactly.
"$C" --json dump > "$TMPD/dump-pre.json" || fail "dump (pre) failed"

# Open the tap; it reads exactly N decoded bytes to stdout, then exits.
timeout 40 "$C" tap usb0 --bytes "$N" > "$TMPD/tap.out" 2>"$TMPD/tap.err" &
TAPPID=$!
# The tap is registered and active once state lists exactly one tap on usb0.
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==1 and .taps[0].endpoint==\"usb0\"'" 5 0.05 \
  || { cat "$TMPD/daemon.log" "$TMPD/tap.err"; fail "tap did not register in state"; }

# dump WHILE the tap is open must be byte-identical: a tap never touches config (§8).
"$C" --json dump > "$TMPD/dump-mid.json" || fail "dump (mid) failed"
diff -q "$TMPD/dump-pre.json" "$TMPD/dump-mid.json" >/dev/null \
  || { echo "--- pre ---"; cat "$TMPD/dump-pre.json"; echo "--- mid ---"; cat "$TMPD/dump-mid.json"; \
       fail "dump changed while a tap was open (a tap leaked into configuration)"; }

# Release the source: N seeded bytes flow device → serial → {log, tap}.
touch "$GO"

# The tap process exits once it has read N bytes; the log file reaches N bytes.
wait "$TAPPID" 2>/dev/null; TAPST=$?; TAPPID=
[ "$TAPST" = 0 ] || { cat "$TMPD/tap.err" "$TMPD/daemon.log"; fail "tap process exited $TAPST before reading $N bytes"; }
bash "$WAIT" "[ \"\$(stat -c %s '$TMPD/logs/serial.log' 2>/dev/null || echo 0)\" = '$N' ]" 10 0.1 \
  || { cat "$TMPD/daemon.log"; fail "log file never reached $N bytes"; }

# The source writes its verdict only after its post-write hold, so wait for it to
# exit before reading the sha (else the file is still empty).
wait "$SRCPID" 2>/dev/null; SRCPID=

# Byte-exact: tap == co-attached log == the seeded source.
SRC_SHA=$(jq -r '.sha256 // ""' "$TMPD/src.json")
TAP_SHA=$(sha256sum "$TMPD/tap.out" | cut -d' ' -f1)
LOG_SHA=$(sha256sum "$TMPD/logs/serial.log" | cut -d' ' -f1)
TAP_LEN=$(stat -c %s "$TMPD/tap.out")
[ -n "$SRC_SHA" ] || { cat "$TMPD/src.json" "$TMPD/src.err"; fail "source produced no verdict"; }
[ "$TAP_LEN" = "$N" ] || fail "tap wrote $TAP_LEN bytes, expected $N"
[ "$TAP_SHA" = "$LOG_SHA" ] || fail "tap checksum != co-attached log checksum"
[ "$TAP_SHA" = "$SRC_SHA" ] || fail "tap checksum != source checksum (tap dropped or corrupted bytes)"

# The tap's connection has closed (its process exited): state shows zero taps.
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==0'" 5 0.05 \
  || { "$C" --json state; fail "tap did not detach after its connection dropped" ; }

# Explicit connection-drop test: open a persistent tap, confirm it registers, kill
# it, and confirm prompt detach even with an idle endpoint (the source is done).
timeout 20 "$C" tap usb0 > /dev/null 2>&1 &
WATCHPID=$!
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==1'" 5 0.05 \
  || fail "persistent tap did not register"
kill "$WATCHPID" 2>/dev/null; wait "$WATCHPID" 2>/dev/null; WATCHPID=
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==0'" 5 0.05 \
  || { "$C" --json state; fail "killed tap did not detach"; }

"$C" shutdown >/dev/null
echo '{"check":"phase8-tap","pass":true}'
