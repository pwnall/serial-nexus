#!/usr/bin/env bash
# Phase 8 validation (item 2): the soak. A daemon under continuous synthetic load,
# asserting the four release-soak signals throughout (design §5, plan §Phase 8):
#   (1) bounded VmRSS — no interior accumulation;
#   (2) an allowlist of growing counters — drop/discard/purge counters stay flat at
#       zero on a keep-up baseline; any growth outside throughput is a failure;
#   (3) zero unexplained faulted nodes;
#   (4) final per-stream checksum reconciliation — the sink equals the generator.
#
# Parameterized so one script serves both the fast CI smoke and the 24-hour nightly:
#   SOAK_SECONDS   total run time      (default 8; nightly sets 86400)
#   SOAK_RATE_MIB  source rate MiB/s   (default 4)
#   SOAK_INTERVAL  sample period sec   (default 2)
#   SOAK_RSS_KB    VmRSS budget in KB  (default 150000 ~= 150 MB)
#
# Topology (no hardware, §15.17): a paced firehose device → serial → log sink. The
# log captures every byte losslessly; a paced source keeps the port "present" the
# whole run (§7.1), so a fault or a drop is a real regression, not absence.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
# shellcheck source=scripts/lib/assert.sh
source "$REPO_ROOT/scripts/lib/assert.sh"
fail() { echo "{\"check\":\"phase8-soak\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

SOAK_SECONDS=${SOAK_SECONDS:-8}
SOAK_RATE_MIB=${SOAK_RATE_MIB:-4}
SOAK_INTERVAL=${SOAK_INTERVAL:-2}
SOAK_RSS_KB=${SOAK_RSS_KB:-150000}
RATE_BYTES=$(( SOAK_RATE_MIB * 1048576 ))
# Size the source to stream for ~the whole duration, plus a margin so it does not
# run dry before the last sample.
TOTAL_BYTES=$(( RATE_BYTES * (SOAK_SECONDS + SOAK_INTERVAL + 2) ))

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p8soak.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/device"
LOGDIR="$TMPD/logs"; mkdir -p "$LOGDIR"
CC=("$C" --socket "$SOCK")
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SIMPID:-}" ] && kill "$SIMPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

"$D" --socket "$SOCK" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

cat > "$TMPD/demo.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
arbitration = "free-for-all"
[[node]]
type = "log"
name = "cap"
directory = "$LOGDIR"
filename = "soak.log"
[[edge]]
a = "usb0"
b = "cap"
write_mode = "never"
EOF
"${CC[@]}" load "$TMPD/demo.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

# The paced firehose: emit TOTAL_BYTES at RATE_BYTES/s, then stay "plugged in" while
# the final reconciliation runs. Its verdict (sha256) lands in source.json on exit.
"$SIM" pty --source --seed 7 --bytes "$TOTAL_BYTES" --rate "$RATE_BYTES" \
    --link "$DEV" --timeout-ms $(( (SOAK_SECONDS + 30) * 1000 )) --hold-ms 5000 \
    >"$TMPD/source.json" 2>&1 &
SIMPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"
bash "$WAIT" "\"$C\" --socket '$SOCK' --json state | jq -e '.nodes[]|select(.name==\"usb0\")|.status==\"active\"'" 5 0.1 \
  || { cat "$TMPD/daemon.log"; fail "usb0 never reached active"; }

# --- Sampling loop: assert signals (1)(2)(3) every SOAK_INTERVAL for the duration.
peak_rss=0
samples=0
START=$SECONDS
while [ $(( SECONDS - START )) -lt "$SOAK_SECONDS" ]; do
  kill -0 "$DPID" 2>/dev/null || { cat "$TMPD/daemon.log"; fail "daemon exited mid-soak"; }

  rss=$(awk '/VmRSS/{print $2}' "/proc/$DPID/status" 2>/dev/null || echo 0)
  [ -n "$rss" ] || rss=0
  [ "$rss" -gt "$peak_rss" ] && peak_rss=$rss
  [ "$rss" -le "$SOAK_RSS_KB" ] || fail "VmRSS ${rss}KB exceeded budget ${SOAK_RSS_KB}KB (interior accumulation)"

  st=$("${CC[@]}" --json state 2>/dev/null) || fail "state query failed mid-soak"
  # (3) no unexplained faulted nodes.
  echo "$st" | jq -e '[.nodes[]|select(.status=="faulted")]|length==0' >/dev/null \
    || { echo "$st" | jq -c '[.nodes[]|select(.status=="faulted")|{name,reason}]' >&2; fail "a node faulted mid-soak"; }
  # (2) allowlist: every drop/discard/purge counter across the graph stays at zero,
  # via the shared, self-tested helper (§16.5) — no hand-inlined jq that could carry
  # the precedence tautology the phase-8 audit caught.
  echo "$st" | assert_loss_counters_zero \
    || { echo "$st" | loss_counters_nonzero >&2; fail "a loss counter grew on the keep-up baseline"; }

  samples=$(( samples + 1 ))
  sleep "$SOAK_INTERVAL"
done

# --- (4) Checksum reconciliation: the log equals the generator, byte for byte.
wait "$SIMPID" 2>/dev/null
SIMPID=
src_sha=$(jq -r '.sha256 // empty' "$TMPD/source.json" 2>/dev/null)
src_sent=$(jq -r '.sent // empty' "$TMPD/source.json" 2>/dev/null)
[ -n "$src_sha" ] && [ -n "$src_sent" ] || { cat "$TMPD/source.json"; fail "source did not report a verdict"; }
# The log writer drains its queue; wait until it has captured every emitted byte.
bash "$WAIT" "[ \"\$(cat '$LOGDIR'/soak.log.* '$LOGDIR'/soak.log 2>/dev/null | wc -c)\" -eq $src_sent ]" 15 0.1 \
  || { cat "$TMPD/daemon.log"; fail "log captured $(cat "$LOGDIR"/soak.log.* "$LOGDIR"/soak.log 2>/dev/null | wc -c)/$src_sent bytes"; }
log_sha=$(cat "$LOGDIR"/soak.log.* "$LOGDIR"/soak.log 2>/dev/null | sha256sum | awk '{print $1}')
[ "$log_sha" = "$src_sha" ] || fail "log checksum != source checksum (bytes lost/duplicated/reordered)"

"${CC[@]}" shutdown >/dev/null
echo "{\"check\":\"phase8-soak\",\"pass\":true,\"seconds\":$SOAK_SECONDS,\"samples\":$samples,\"bytes\":$src_sent,\"rss_peak_kb\":$peak_rss,\"rss_budget_kb\":$SOAK_RSS_KB}"
