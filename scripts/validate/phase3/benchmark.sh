#!/usr/bin/env bash
# Phase 3 benchmark (§5 single-thread assumption; §15.18 escape hatches). Records
# two axes to docs/benchmarks/phase3.json:
#   * throughput — device -> daemon -> fast sink (log), asserting >= 10x headroom
#     over 8 ports at 3 Mbaud (= 3 MB/s), i.e. >= 30 MiB/s, making the data plane's
#     capacity a recorded fact.
#   * idle cost — total daemon CPU with 32 idle tty (PTY) fds, asserting it stays
#     under budget. Exceeding either selects a §15.18 escape hatch (dedicated
#     reader/writer threads for throughput; adaptive idle backoff for CPU) — both
#     already in place — never a return to epoll.
# No hardware (§15.17).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase3-benchmark\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

THROUGHPUT_MIN=30      # MiB/s: 10x over 8 ports @ 3 Mbaud (3 MB/s)
IDLE_FDS=32
IDLE_CPU_BUDGET=20     # percent, total daemon CPU

TMPD=$(mktemp -d /tmp/snx-bm.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
PIDS=()
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  for p in "${PIDS[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# ---- Throughput: device -> serial -> log (both dedicated blocking threads) ----
"$D" >"$TMPD/daemon.log" 2>&1 & DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

SIZE_B=$((256 * 1024 * 1024))
"$SIM" pty --source --bytes 256MiB --seed 7 --link "$TMPD/dev" --timeout-ms 120000 >"$TMPD/src.json" 2>&1 & SRC=$!
PIDS+=($SRC)
bash "$WAIT" "test -e '$TMPD/dev'" 5 0.05 || fail "device never appeared"
printf '[[node]]\ntype="serial"\nname="usb0"\ndevice="%s"\n[[node]]\ntype="log"\nname="sink"\ndirectory="%s"\nfilename="bench.log"\n[[edge]]\na="usb0"\nb="sink"\n' "$TMPD/dev" "$TMPD" > "$TMPD/tp.toml"
T0=$(date +%s.%N)
"$C" load "$TMPD/tp.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }
bash "$WAIT" "test \"\$(stat -c %s '$TMPD/bench.log' 2>/dev/null || echo 0)\" -ge $SIZE_B" 60 0.02 \
  || fail "throughput run did not complete in 60s"
T1=$(date +%s.%N)
wait "$SRC" 2>/dev/null
[ "$(sha256sum "$TMPD/bench.log" | cut -d' ' -f1)" = "$(jq -r .sha256 "$TMPD/src.json")" ] \
  || fail "throughput run checksum mismatch (lossy)"
MBPS=$(echo "scale=1; $SIZE_B / 1048576 / ($T1 - $T0)" | bc)
"$C" teardown >/dev/null || fail "teardown failed"

# ---- Idle cost: 32 idle PTY fds, total daemon CPU over 3s ----
{ for i in $(seq 1 "$IDLE_FDS"); do printf '[[node]]\ntype="pty"\nname="p%d"\npath="%s/tty%d"\n' "$i" "$TMPD" "$i"; done; } > "$TMPD/idle.toml"
"$C" load "$TMPD/idle.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "idle load failed"; }
[ "$("$C" --json state | jq '.nodes|length')" = "$IDLE_FDS" ] || fail "expected $IDLE_FDS idle nodes"
CLK=$(getconf CLK_TCK)
read u1 s1 < <(awk '{print $14, $15}' "/proc/$DPID/stat")
sleep 3
read u2 s2 < <(awk '{print $14, $15}' "/proc/$DPID/stat")
IDLE_CPU=$(echo "scale=2; (($u2 + $s2) - ($u1 + $s1)) / ($CLK * 3) * 100" | bc)

"$C" shutdown >/dev/null

# ---- Record the facts, then assert the budgets ----
mkdir -p "$REPO_ROOT/docs/benchmarks"
cat > "$REPO_ROOT/docs/benchmarks/phase3.json" <<JSON
{
  "throughput": {
    "path": "device -> serial(reader thread) -> log(writer thread)",
    "bytes": $SIZE_B,
    "mib_per_s": $MBPS,
    "headroom_target_mib_per_s": $THROUGHPUT_MIN,
    "headroom_basis": "10x over 8 ports at 3 Mbaud (3 MB/s aggregate)"
  },
  "idle_cost": {
    "idle_tty_fds": $IDLE_FDS,
    "total_cpu_percent": $IDLE_CPU,
    "budget_percent": $IDLE_CPU_BUDGET,
    "mechanism": "adaptive poll backoff to IDLE_POLL when quiescent (§15.18)"
  }
}
JSON

# jq -e for numeric comparisons (bc handles the arithmetic, jq the verdict).
awk -v v="$MBPS" -v m="$THROUGHPUT_MIN" 'BEGIN{exit !(v+0 >= m+0)}' \
  || fail "throughput ${MBPS} MiB/s below the ${THROUGHPUT_MIN} MiB/s headroom target"
awk -v v="$IDLE_CPU" -v b="$IDLE_CPU_BUDGET" 'BEGIN{exit !(v+0 < b+0)}' \
  || fail "idle CPU ${IDLE_CPU}% over the ${IDLE_CPU_BUDGET}% budget"

echo "{\"check\":\"phase3-benchmark\",\"pass\":true,\"throughput_mib_per_s\":$MBPS,\"idle_cpu_percent\":$IDLE_CPU}"
