#!/usr/bin/env bash
# Phase 3 validation (firehose integrity, §5 + §15.18): a large seeded stream
# flows device -> daemon -> fast sink with its checksum intact and at high
# throughput, while the daemon's resident memory stays bounded — proof that the
# interior accumulates nothing and the §15.18 reader-thread escape hatch delivers
# line rate. The fast sink is a log node (a dedicated blocking writer); the
# serial reader is a dedicated blocking thread. No hardware (§15.17).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-firehose\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p3h.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

SIZE_H="256MiB"
SIZE_B=$((256 * 1024 * 1024))
RSS_BUDGET_KB=$((120 * 1024))   # streaming stays ~tens of MiB; accumulation would blow past this

"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

DEV="$TMPD/dev"
"$SIM" pty --source --bytes "$SIZE_H" --seed 7 --link "$DEV" --timeout-ms 120000 >"$TMPD/src.json" 2>"$TMPD/src.err" &
SRCPID=$!; PIDS+=("$SRCPID")
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

cat > "$TMPD/c.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[node]]
type = "log"
name = "sink"
directory = "$TMPD"
filename = "sink.log"
[[edge]]
a = "usb0"
b = "sink"
EOF
T0=$(date +%s.%N)
"$C" load "$TMPD/c.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# Sample the daemon's resident memory while the stream flows.
PEAK=0
until [ "$(stat -c %s "$TMPD/sink.log" 2>/dev/null || echo 0)" -ge "$SIZE_B" ]; do
  rss=$(awk '/VmRSS/{print $2}' "/proc/$DPID/status" 2>/dev/null || echo 0)
  [ -n "$rss" ] && [ "$rss" -gt "$PEAK" ] && PEAK=$rss
  kill -0 "$DPID" 2>/dev/null || { cat "$TMPD/daemon.log"; fail "daemon exited mid-transfer"; }
  # bail out if it stalls badly
  [ "$(echo "$(date +%s.%N) - $T0 > 60" | bc)" = "1" ] && fail "firehose did not complete within 60s (throughput regression)"
done
T1=$(date +%s.%N)
wait "$SRCPID" 2>/dev/null
SECS=$(echo "$T1 - $T0" | bc)
MBPS=$(echo "scale=1; $SIZE_B / 1048576 / $SECS" | bc)

SRC_SHA=$(jq -r .sha256 "$TMPD/src.json") || fail "no source checksum"
LOG_SHA=$(sha256sum "$TMPD/sink.log" | cut -d' ' -f1)
[ "$(stat -c %s "$TMPD/sink.log")" = "$SIZE_B" ] || fail "sink size != source size (lossy firehose)"
[ "$LOG_SHA" = "$SRC_SHA" ] || fail "sink checksum != source checksum"
[ "$PEAK" -gt 0 ] || fail "could not sample daemon RSS"
[ "$PEAK" -lt "$RSS_BUDGET_KB" ] || fail "daemon RSS peak ${PEAK}KB exceeded the ${RSS_BUDGET_KB}KB budget (interior accumulation?)"

"$C" shutdown >/dev/null
echo "{\"check\":\"phase3-firehose\",\"pass\":true,\"bytes\":$SIZE_B,\"mib_per_s\":$MBPS,\"rss_peak_kb\":$PEAK}"
