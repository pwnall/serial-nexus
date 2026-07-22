#!/usr/bin/env bash
# Phase 3 validation (exact loss accounting, §5): with a throttled PTY client, the
# PTY boundary's drop counters account for every byte the client did not receive,
# to the byte (dropped + discarded == source_sent - client_received), while a log
# on the same serial captures the complete stream. Loss is located, counted, and
# isolated. No hardware (§15.17); the device is a seeded nexus-sim source.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-exact-loss\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3x.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT
nstate() { "$C" --json state | jq -r ".nodes[]|select(.name==\"$1\")|.$2"; }

SIZE_H="24MiB"
SIZE_B=$((24 * 1024 * 1024))

"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

DEV="$TMPD/dev"
# Paced source (20 MB/s): slow enough that the throttled client is attached and
# present while the boundary sheds, so drops are slow-consumer drops, not
# discards-while-absent — and still far faster than the client's 4 MB/s.
"$SIM" pty --source --bytes "$SIZE_H" --seed 7 --rate 20000000 --link "$DEV" --timeout-ms 120000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!; PIDS+=("$SRCPID")
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

TTY="$TMPD/console"
cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "pty"
name = "console"
path = "$TTY"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[node]]
type = "log"
name = "cap"
directory = "$TMPD"
filename = "cap.log"
[[edge]]
a = "usb0"
b = "console"
[[edge]]
a = "usb0"
b = "cap"
EOF
"$C" load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# A throttled client that reads until the stream goes quiet (fully draining, so
# no daemon-delivered byte is left unread — the precondition for exact counting).
# The serial floods far faster than the client drains, so the PTY boundary sheds.
CLI=$("$SIM" client --path "$TTY" --drain --read-rate 4000000 --quiet-ms 1500 --timeout-ms 60000)
echo "$CLI" | jq -e '.pass==true' >/dev/null || { cat "$TMPD/daemon.log"; fail "drain client failed: $CLI"; }
R=$(echo "$CLI" | jq -r .received)
wait "$SRCPID" 2>/dev/null
S=$(jq -r .sent "$TMPD/src.json")
[ "$S" = "$SIZE_B" ] || fail "source sent $S, expected $SIZE_B"

# Let the post-drain counters settle (the writer discards any last in-flight
# bytes once the client detaches), then assert exact conservation.
bash "$WAIT" "
  D1=\$(\"$C\" --json state | jq -r '.nodes[]|select(.name==\"console\")|.dropped_slow_consumer');
  D2=\$(\"$C\" --json state | jq -r '.nodes[]|select(.name==\"console\")|.discarded_no_client');
  [ \$((D1 + D2 + $R)) -eq $S ]
" 5 0.1 || {
  D1=$(nstate console dropped_slow_consumer); D2=$(nstate console discarded_no_client)
  cat "$TMPD/daemon.log"
  fail "not exact: dropped=$D1 discarded=$D2 received=$R sum=$((D1+D2+R)) != sent=$S"
}
DROPPED=$(nstate console dropped_slow_consumer)
[ "$DROPPED" -gt 0 ] || fail "expected some drops with a throttled client (dropped=$DROPPED)"

# The log on the same serial captured the complete stream — loss is isolated.
bash "$WAIT" "test \"\$(stat -c %s '$TMPD/cap.log' 2>/dev/null || echo 0)\" -eq $SIZE_B" 10 0.1 \
  || fail "log did not capture the full stream (size $(stat -c %s "$TMPD/cap.log" 2>/dev/null))"
[ "$(sha256sum "$TMPD/cap.log" | cut -d' ' -f1)" = "$(jq -r .sha256 "$TMPD/src.json")" ] \
  || fail "log checksum != source checksum (lossy on the lossless path)"

"$C" shutdown >/dev/null
echo "{\"check\":\"phase3-exact-loss\",\"pass\":true,\"sent\":$S,\"received\":$R,\"dropped\":$DROPPED}"
