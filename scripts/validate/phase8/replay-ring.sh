#!/usr/bin/env bash
# Web console track validation (design §5/§17 / plan §11.2): the replay ring.
#
# `replay_ring = <bytes>` on a host-facing endpoint retains the most recent hostward
# bytes so a late `tap --replay` sees what just happened. This asserts:
#   1. exact splice — a replay tap opened mid-stream receives ring-then-live with no
#      gap and no duplication, i.e. exactly a contiguous suffix of the stream;
#   2. an empty-replay marker — a ring-off endpoint (and an as-yet-empty ring)
#      answers `--replay` with replay_bytes = 0;
#   3. the attribute round-trips through dump/load.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase8-replay-ring\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8rr.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; DEV2="$TMPD/dev2"
GO="$TMPD/go"
R=65536             # ring depth: 64 KiB
T=524288            # total streamed: 512 KiB
RATE=262144         # 256 KiB/s → the stream lasts ~2s, so a tap opens mid-stream
SEED=23
mkdir -p "$TMPD/logs"
cleanup() {
  for p in DPID SRCPID STALLPID TAPPID; do kill "${!p:-}" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# usb0's device: a paced seeded source, gated so it starts only after a tap is up.
"$SIM" pty --source --bytes "$T" --seed "$SEED" --rate "$RATE" --wait-file "$GO" \
  --link "$DEV" --hold-ms 2000 --timeout-ms 60000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!
# usb1's device: present but silent, so usb1 is active with a ring that stays empty.
"$SIM" pty --link "$DEV2" --stall --timeout-ms 60000 >/dev/null 2>&1 &
STALLPID=$!
bash "$WAIT" "test -e '$DEV' && test -e '$DEV2'" 5 0.05 || fail "devices never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# usb0 carries a 64 KiB replay ring and a byte-exact log anchor; usb1 has no ring.
cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "free-for-all"
hostward_buffer = 16384
replay_ring = $R
[[node]]
type = "serial"
name = "usb1"
device = "$DEV2"
arbitration = "free-for-all"
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

# --- (3) dump/load round-trip of the attribute -------------------------------
"$C" --json dump > "$TMPD/dump1.json" || fail "dump failed"
"$C" dump | grep -qE "replay_ring = $R" \
  || { "$C" dump; fail "replay_ring did not round-trip through dump"; }

# --- (2) empty-replay marker --------------------------------------------------
# ring-off endpoint (usb1): --replay yields replay_bytes = 0.
ack_replay_bytes() { # $1 endpoint  → prints replay_bytes from the tap.open ack
  timeout 3 "$C" tap "$1" --replay --stall-ms 300 2>"$TMPD/ack.err" >/dev/null || true
  sed -n 's/.*tap opened[^{]*\({.*}\).*/\1/p' "$TMPD/ack.err" | jq -r '.replay_bytes // -1' 2>/dev/null | tail -1
}
RB_OFF=$(ack_replay_bytes usb1)
[ "$RB_OFF" = 0 ] || { cat "$TMPD/ack.err"; fail "ring-off endpoint replay_bytes=$RB_OFF, expected 0"; }
# ring configured but still empty (usb0 before any data): also 0.
RB_EMPTY=$(ack_replay_bytes usb0)
[ "$RB_EMPTY" = 0 ] || { cat "$TMPD/ack.err"; fail "empty ring replay_bytes=$RB_EMPTY, expected 0"; }

# --- (1) exact splice ---------------------------------------------------------
# Release the paced source, then open a replay tap once the ring is full and live
# bytes still remain (log has passed 2*R). The tap receives ring-then-live; because
# ring + live is one contiguous suffix of the stream, tap == tail-of-log of the same
# length, byte-exact. Kill the tap once the stream is done and captured.
touch "$GO"
bash "$WAIT" "[ \"\$(stat -c %s '$TMPD/logs/serial.log' 2>/dev/null || echo 0)\" -ge $((2 * R)) ]" 20 0.05 \
  || { cat "$TMPD/daemon.log"; fail "stream never reached 2*ring before the tap opened"; }

timeout 30 "$C" tap usb0 --replay > "$TMPD/tap.out" 2>"$TMPD/tap.err" &
TAPPID=$!
bash "$WAIT" "\"$C\" --json state | jq -e '(.taps|length)==1'" 5 0.05 || fail "replay tap did not register"
# The ring was full at open, so the replay prefix is exactly R bytes.
RB_FULL=$(sed -n 's/.*tap opened[^{]*\({.*}\).*/\1/p' "$TMPD/tap.err" | jq -r '.replay_bytes // -1' | tail -1)
[ "$RB_FULL" = "$R" ] || { cat "$TMPD/tap.err"; fail "full-ring replay_bytes=$RB_FULL, expected $R"; }

# Source finishes; let the tap drain the tail, then stop it.
wait "$SRCPID" 2>/dev/null; SRCPID=
bash "$WAIT" "[ \"\$(stat -c %s '$TMPD/logs/serial.log' 2>/dev/null || echo 0)\" = '$T' ]" 20 0.1 \
  || { cat "$TMPD/daemon.log"; fail "log never reached the full $T bytes"; }
sleep 0.5   # let the last live bytes reach the tap
kill "$TAPPID" 2>/dev/null; wait "$TAPPID" 2>/dev/null; TAPPID=

TAP_LEN=$(stat -c %s "$TMPD/tap.out")
[ "$TAP_LEN" -ge "$R" ] || fail "replay tap captured $TAP_LEN bytes, expected at least the $R-byte ring"
[ "$TAP_LEN" -le "$T" ] || fail "replay tap captured $TAP_LEN bytes, more than the $T streamed (duplication)"
# The captured replay+live must equal exactly the last TAP_LEN bytes of the stream
# (the log): no gap, no duplication — the exact-splice guarantee.
TAP_SHA=$(sha256sum "$TMPD/tap.out" | cut -d' ' -f1)
TAIL_SHA=$(tail -c "$TAP_LEN" "$TMPD/logs/serial.log" | sha256sum | cut -d' ' -f1)
[ "$TAP_SHA" = "$TAIL_SHA" ] \
  || fail "replay+live != a contiguous suffix of the stream (gap or duplication at the splice)"

"$C" shutdown >/dev/null
echo "{\"check\":\"phase8-replay-ring\",\"pass\":true,\"replay_bytes\":$RB_FULL,\"spliced\":$TAP_LEN}"
