#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 2): resynchronization is accounted, not
# approximate. `nexus-sim mux --corrupt-every N` mangles one in every N frames'
# type byte (length prefix intact); the demux codec skips exactly that frame and
# resyncs. The codec's framing-error counter must equal the manifest's corruption
# count, and each channel's delivered bytes + checksum must equal the manifest's
# computed expected-loss set — recovery after garbage is provable.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase5-resync\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p5r.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"; GO="$TMPD/go"
# Frame 4096 keeps ≤16 chunks per 64KiB serial read (under the PTY bridge depth),
# so the demuxed burst is not slow-consumer-dropped at the channel boundary — the
# client then receives exactly the codec's delivered set (§5).
SEED=7; BYTES=512KiB; FRAME=4096; CORRUPT=30
CHANNELS=(c0 c1 c2 c3)
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${MUXPID:-}" ] && kill "$MUXPID" 2>/dev/null
  for p in "${CLIPIDS[@]:-}"; do kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

MUXARGS=(--seed "$SEED" --bytes "$BYTES" --frame-size "$FRAME" --corrupt-every "$CORRUPT"
         --channel c0 --channel c1 --channel c2 --channel c3)

"$SIM" mux --manifest "${MUXARGS[@]}" >"$TMPD/manifest.json" || fail "manifest failed"
CORRUPTED=$(jq -r '.corrupted' "$TMPD/manifest.json")
[ "$CORRUPTED" -gt 0 ] || fail "manifest reports no corruption; pick different params"

"$SIM" mux "${MUXARGS[@]}" --link "$DEV" --wait-file "$GO" --timeout-ms 30000 >"$TMPD/mux.json" 2>&1 &
MUXPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

{
  echo '[[node]]'; echo 'type = "serial"'; echo 'name = "usb0"'; echo "device = \"$DEV\""
  echo '[[node]]'; echo 'type = "codec"'; echo 'name = "mux"'; echo 'codec = "reference"'
  echo 'faces = "target"'; echo 'channels = ["c0", "c1", "c2", "c3"]'
  for ch in "${CHANNELS[@]}"; do
    echo '[[node]]'; echo 'type = "pty"'; echo "name = \"con-$ch\""; echo "path = \"$TMPD/tty-$ch\""
  done
  echo '[[edge]]'; echo 'a = "usb0"'; echo 'b = "mux"'; echo 'write_mode = "held"'
  for ch in "${CHANNELS[@]}"; do
    echo '[[edge]]'; echo "a = \"mux/$ch\""; echo "b = \"con-$ch\""; echo 'write_mode = "never"'
  done
} > "$TMPD/g.toml"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Attach fully-draining clients (delivered < sent under corruption, so they read
# until quiet rather than to a fixed count), wait for presence, then release.
CLIPIDS=()
for ch in "${CHANNELS[@]}"; do
  "$SIM" client --path "$TMPD/tty-$ch" --drain --quiet-ms 700 --timeout-ms 30000 >"$TMPD/recv-$ch.json" 2>&1 &
  CLIPIDS+=($!)
done
for ch in "${CHANNELS[@]}"; do
  bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"con-$ch\")|.client_present==true'" 8 0.05 \
    || { cat "$TMPD/daemon.log"; fail "channel client con-$ch never became present"; }
done
touch "$GO"

rc=0
for i in "${!CHANNELS[@]}"; do
  ch="${CHANNELS[$i]}"
  wait "${CLIPIDS[$i]}" 2>/dev/null || true
  want_n=$(jq -r ".channels[]|select(.id==\"$ch\")|.delivered" "$TMPD/manifest.json")
  want_sha=$(jq -r ".channels[]|select(.id==\"$ch\")|.sha256" "$TMPD/manifest.json")
  got_n=$(jq -r '.received // -1' "$TMPD/recv-$ch.json")
  got_sha=$(jq -r '.sha256 // ""' "$TMPD/recv-$ch.json")
  if [ "$got_n" != "$want_n" ] || [ "$got_sha" != "$want_sha" ]; then
    echo "channel $ch: received=$got_n want=$want_n sha_ok=$([ "$got_sha" = "$want_sha" ] && echo yes || echo no)" >&2
    rc=1
  fi
done
[ "$rc" = 0 ] || { cat "$TMPD/daemon.log"; "$C" --json state >&2; fail "a channel's recovered stream did not match its expected-loss manifest"; }

# The codec's framing-error (resync) counter equals the manifest's corruption
# count, and each channel's delivered set (the codec's own count, before any
# boundary drop) equals the manifest — the deterministic proof of exact recovery.
STATE=$("$C" --json state)
GOT_FE=$(printf '%s' "$STATE" | jq -r '.nodes[]|select(.name=="mux")|.framing_errors')
[ "$GOT_FE" = "$CORRUPTED" ] || { printf '%s' "$STATE" >&2; fail "framing_errors=$GOT_FE, expected $CORRUPTED corrupted frames"; }
for ch in "${CHANNELS[@]}"; do
  want_n=$(jq -r ".channels[]|select(.id==\"$ch\")|.delivered" "$TMPD/manifest.json")
  got_n=$(printf '%s' "$STATE" | jq -r ".nodes[]|select(.name==\"mux\")|.channels.$ch.delivered_hostward")
  [ "$got_n" = "$want_n" ] || { printf '%s' "$STATE" >&2; fail "codec delivered_hostward[$ch]=$got_n, expected $want_n"; }
done

"$C" shutdown >/dev/null
echo "{\"check\":\"phase5-resync\",\"pass\":true,\"corrupted\":$CORRUPTED}"
