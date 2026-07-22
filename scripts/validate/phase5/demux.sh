#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 1): deterministic demultiplexing.
# `nexus-sim mux` feeds a reference-framed multichannel stream into a device PTY;
# a demux codec node splits it into per-channel PTYs; each channel client's
# received checksum must equal the sim's per-channel manifest. No hardware
# (§15.17): the "device" is the sim's PTY, and correctness is a byte-exact
# per-channel checksum comparison.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase5-demux\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p5d.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"
GO="$TMPD/go"
SEED=7
# 256 KiB/channel (1 MiB across the four channels) — 64 frames/channel, 256 frames
# of round-robin interleave, which exercises per-channel framing and byte-exactness
# thoroughly; correctness, not throughput, is the subject here (throughput is
# firehose.sh). Kept small so the single-threaded daemon completes it comfortably
# even when heavily CPU-starved, rather than the test being hostage to scheduling.
BYTES=256KiB
NBYTES=262144
PRIMER=256      # per-channel primer bytes for the presence-vs-readiness handshake
PRIME="$TMPD/prime"
CHANNELS=(c0 c1 c2 c3)
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${MUXPID:-}" ] && kill "$MUXPID" 2>/dev/null
  for p in "${CLIPIDS[@]:-}"; do kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The deterministic manifest: each channel's expected delivered bytes + checksum.
"$SIM" mux --manifest --seed "$SEED" --bytes "$BYTES" \
  --channel c0 --channel c1 --channel c2 --channel c3 >"$TMPD/manifest.json" \
  || fail "manifest failed"
jq -e '.pass==true and .corrupted==0' "$TMPD/manifest.json" >/dev/null || fail "manifest not clean"

# Start the device feed. Two-phase handshake (plan §3, presence != readiness): once
# the clients are present (--prime-file), send a small primer per channel; each client
# reads it and proves it is draining (--ready-file); only then is the payload burst
# released (--wait-file), so it cannot outrun a not-yet-reading client.
"$SIM" mux --seed "$SEED" --bytes "$BYTES" \
  --channel c0 --channel c1 --channel c2 --channel c3 \
  --link "$DEV" --prime-file "$PRIME" --prime-bytes "$PRIMER" \
  --wait-file "$GO" --timeout-ms 90000 >"$TMPD/mux.json" 2>&1 &
MUXPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# serial(usb0, host) → demux codec (held edge) → four read-only channel PTYs.
{
  echo '[[node]]'; echo 'type = "serial"'; echo 'name = "usb0"'; echo "device = \"$DEV\""
  echo '[[node]]'; echo 'type = "codec"'; echo 'name = "mux"'; echo 'codec = "reference"'
  echo 'faces = "target"'; echo 'channels = ["c0", "c1", "c2", "c3"]'
  for ch in "${CHANNELS[@]}"; do
    echo '[[node]]'; echo 'type = "pty"'; echo "name = \"con-$ch\""; echo "path = \"$TMPD/tty-$ch\""
    # This test checks demux CORRECTNESS (byte-exact per-channel split), not the
    # drop policy (that is exact-loss.sh / counters.sh). So size the hostward buffer
    # to comfortably hold the whole per-channel burst (128 chunks of 4 KiB) — a
    # channel client briefly starved under CPU load then drains it losslessly, rather
    # than the default 32-chunk bridge shedding under contention (§5, §7.2).
    echo 'hostward_buffer = 512'
  done
  echo '[[edge]]'; echo 'a = "usb0"'; echo 'b = "mux"'; echo 'write_mode = "held"'
  for ch in "${CHANNELS[@]}"; do
    echo '[[edge]]'; echo "a = \"mux/$ch\""; echo "b = \"con-$ch\""; echo 'write_mode = "never"'
  done
} > "$TMPD/g.toml"
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# The codec is active, reference, and reports its four channels; the serial's lock
# is held by the demux edge (mux origin), so a raw send would be refused (§6).
"$C" --json state | jq -e '.nodes[]|select(.name=="mux")|.status=="active" and .codec=="reference" and (.channels|keys|sort==["c0","c1","c2","c3"])' >/dev/null \
  || { cat "$TMPD/daemon.log"; "$C" --json state; fail "codec node not active/complete"; }
"$C" --json state | jq -e '.nodes[]|select(.name=="usb0")|.lock.holder=="mux"' >/dev/null \
  || fail "demux edge should hold the serial lock (§6 held)"

# Attach one receiving client per channel. Each discards a --skip primer and creates
# its --ready-file on the first byte it reads back (proof it is draining, not merely
# present), then counts/checksums exactly its payload.
CLIPIDS=()
for ch in "${CHANNELS[@]}"; do
  "$SIM" client --path "$TMPD/tty-$ch" --recv "$BYTES" \
    --skip "$PRIMER" --ready-file "$TMPD/ready-$ch" --timeout-ms 90000 >"$TMPD/recv-$ch.json" 2>&1 &
  CLIPIDS+=($!)
done

# Phase 1 — once every channel client is present, release the primer.
for ch in "${CHANNELS[@]}"; do
  bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"con-$ch\")|.client_present==true'" 8 0.05 \
    || { cat "$TMPD/daemon.log"; fail "channel client con-$ch never became present"; }
done
touch "$PRIME"

# Phase 2 — once every client has read a primer byte (so its read loop is live and
# parked), release the payload burst. This is the fix for the presence-vs-readiness
# race: a small primer reliably reaches a present client, and only a *reading* client
# advances the handshake.
for ch in "${CHANNELS[@]}"; do
  bash "$WAIT" "test -e '$TMPD/ready-$ch'" 8 0.05 \
    || { cat "$TMPD/daemon.log" "$TMPD/recv-$ch.json"; fail "channel client con-$ch never signalled ready (drained the primer)"; }
done
touch "$GO"

# Every channel client must receive exactly its manifest bytes and checksum.
rc=0
for ch in "${CHANNELS[@]}"; do
  wait_pid=""
  for i in "${!CHANNELS[@]}"; do [ "${CHANNELS[$i]}" = "$ch" ] && wait_pid="${CLIPIDS[$i]}"; done
  wait "$wait_pid" 2>/dev/null || true
  want_sha=$(jq -r ".channels[]|select(.id==\"$ch\")|.sha256" "$TMPD/manifest.json")
  got_sha=$(jq -r '.sha256 // ""' "$TMPD/recv-$ch.json")
  got_n=$(jq -r '.received // -1' "$TMPD/recv-$ch.json")
  if [ "$got_n" != "$NBYTES" ] || [ -z "$got_sha" ] || [ "$got_sha" != "$want_sha" ]; then
    echo "channel $ch: received=$got_n want=$NBYTES sha_ok=$([ "$got_sha" = "$want_sha" ] && echo yes || echo no)" >&2
    rc=1
  fi
done
[ "$rc" = 0 ] || { cat "$TMPD/daemon.log"; "$C" --json state >&2; fail "a channel's demuxed stream did not match its manifest"; }

# The codec reports no framing errors on a clean stream, and per-channel delivery.
# The demux delivers the primer too, so c0's hostward count is primer + payload.
"$C" --json state | jq -e ".nodes[]|select(.name==\"mux\")|.framing_errors==0 and (.channels.c0.delivered_hostward==$((NBYTES + PRIMER)))" >/dev/null \
  || { "$C" --json state >&2; fail "codec state: framing_errors/delivered wrong"; }

"$C" shutdown >/dev/null
echo '{"check":"phase5-demux","pass":true}'
