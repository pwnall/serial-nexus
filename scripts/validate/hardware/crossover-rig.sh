#!/usr/bin/env bash
# Tier-3 hardware integration test (design §13/§15.17/§15.21, plan §5) — the
# cross-wired null-modem rig, driven end to end through the daemon.
#
# REQUIRES exactly two USB-serial adapters connected to each other by a crossover
# (null-modem) UART cable — TX0<->RX1, RX0<->TX1, GND<->GND. No target device
# (the no-target doctrine): the two adapters ARE each other's target. If the rig
# is absent the test SKIPS (exit 0) — a skip is a valid verdict, a failing probe
# is not (§13). Run it directly:
#
#     bash scripts/validate/hardware/crossover-rig.sh
#
# It is intentionally NOT in the per-push `all.sh` sweep (that lane has no
# hardware); wire it into a hardware CI lane if you have a rig.
#
# The doctrine (§15.21): nexus-doctor P5 certifies the rig FIRST — a clean
# certificate is the precondition — and only then does the daemon get driven
# through it, so a failure here is attributable to serial_nexus, not a loose wire.
set -uo pipefail

CHECK="hw-crossover-rig"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT" || exit 1

pass_json() { echo "{\"check\":\"$CHECK\",\"pass\":true,\"rig\":\"$RIG_DESC\",\"stage\":\"$1\"}"; exit 0; }
fail()      { echo "{\"check\":\"$CHECK\",\"pass\":false,\"stage\":\"${STAGE:-?}\",\"reason\":\"$*\"}"; exit 1; }
skip()      { echo "{\"check\":\"$CHECK\",\"skipped\":true,\"reason\":\"$*\"}"; exit 0; }

STAGE="build"
cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim -p nexus-doctor \
  || fail "cargo build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
DOC="$REPO_ROOT/target/debug/nexus-doctor"

# -------------------------------------------------------------------------------
# Stage 0 — discover the rig (exactly two accessible USB-serial adapters), or skip.
# -------------------------------------------------------------------------------
STAGE="discover"
[ -d /dev/serial/by-id ] || skip "no /dev/serial/by-id (no USB-serial adapters plugged in)"
declare -A SEEN=()
PORTS=()
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  p=$(readlink -f "/dev/serial/by-id/$entry" 2>/dev/null) || continue
  case "$p" in /dev/tty*) ;; *) continue ;; esac
  if [ -z "${SEEN[$p]:-}" ]; then SEEN[$p]=1; PORTS+=("$p"); fi
done < <(ls -1 /dev/serial/by-id/ 2>/dev/null)

[ "${#PORTS[@]}" -eq 2 ] || skip "need exactly 2 USB-serial adapters, found ${#PORTS[@]} (${PORTS[*]:-none})"
P0="${PORTS[0]}"; P1="${PORTS[1]}"
for p in "$P0" "$P1"; do
  [ -r "$p" ] && [ -w "$p" ] || skip "no read/write access to $p — grant 'dialout' membership or a udev rule (§13)"
done
RIG_DESC="$P0 <-> $P1 (crossover)"

# -------------------------------------------------------------------------------
# Stage 1 — rig certificate (nexus-doctor P5), ports still free. §15.21 precondition.
# -------------------------------------------------------------------------------
STAGE="rig-certificate"
"$DOC" --json --port "$P0" --port "$P1" > /tmp/.snxcert.$$ 2>/dev/null || true
CERT=/tmp/.snxcert.$$
trap 'rm -f /tmp/.snxcert.'"$$" EXIT
jq -e '[.probes[]|select(.id=="P3")|.status]|length>=2 and all(.=="supported")' "$CERT" >/dev/null \
  || fail "P3 serial-fit not supported on both ports (see nexus-doctor --port $P0 --port $P1)"
jq -e '.probes[]|select(.id=="P5")|.status=="supported"' "$CERT" >/dev/null \
  || fail "P5 rig discovery not supported"
# Both ports must classify as PAIRED with each other (crossover wired both ways).
jq -e '[.probes[]|select(.id=="P5")|.observations[]|.value]|(map(test("HALF-CROSSED|dangling"))|any|not)' "$CERT" >/dev/null \
  || fail "rig miswired (half-crossed or dangling — check TX/RX and GND); the daemon is not to blame (§15.21)"
jq -e '[.probes[]|select(.id=="P5")|.observations[]|.value|select(test("paired with"))]|length>=2' "$CERT" >/dev/null \
  || fail "the two ports are not paired — is the crossover cable connected? (§15.21)"
# The independent-clock certificate: the rate ladder round-trips and a deliberate
# baud mismatch is observable (rate_ladder=true deliberate_mismatch_observed=true).
PAIRCERT=$(jq -r '.probes[]|select(.id=="P5")|.observations[]|select(.key|test("↔"))|.value' "$CERT")
case "$PAIRCERT" in
  *"rate_ladder=true"*"deliberate_mismatch_observed=true"*) : ;;
  *) fail "pair certificate not clean: '$PAIRCERT' (rig fault; §15.21)" ;;
esac

# -------------------------------------------------------------------------------
# Bring up a daemon in a short runtime dir (SUN_LEN, §7). Everything below drives
# the DAEMON through the physical rig.
# -------------------------------------------------------------------------------
STAGE="daemon-start"
TMPD=$(mktemp -d /tmp/snx-hw.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DPID=""
cleanup() {
  [ -n "$DPID" ] && "$C" --socket "$SOCK" shutdown >/dev/null 2>&1
  [ -n "$DPID" ] && kill "$DPID" 2>/dev/null
  rm -rf "$TMPD" /tmp/.snxcert.$$
}
trap cleanup EXIT
"$D" --socket "$SOCK" --state-file "$TMPD/state.toml" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
end=$(( $(date +%s) + 5 ))
while [ ! -S "$SOCK" ]; do [ "$(date +%s)" -ge "$end" ] && { cat "$TMPD/daemon.log"; fail "control socket never appeared"; }; sleep 0.1; done

dstate() { "$C" --socket "$SOCK" --json state 2>/dev/null; }
nf() { dstate | jq -r --arg n "$1" ".nodes[]|select(.name==\$n)|$2"; }  # node field
wait_active() { # node names...; waits through the FTDI close->reopen transient
  local end=$(( $(date +%s) + 15 ))
  while :; do
    local ok=1 n
    for n in "$@"; do [ "$(nf "$n" .status)" = "active" ] || ok=0; done
    [ "$ok" = 1 ] && return 0
    [ "$(date +%s)" -ge "$end" ] && return 1
    sleep 0.3
  done
}
wait_present() { # node timeout
  local end=$(( $(date +%s) + ${2:-8} ))
  while [ "$(nf "$1" .client_present)" != "true" ]; do
    [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.1
  done
}
wait_size() { # file target timeout
  local end=$(( $(date +%s) + ${3:-25} ))
  while [ "$(stat -c%s "$1" 2>/dev/null || echo 0)" -lt "$2" ]; do
    [ "$(date +%s)" -ge "$end" ] && return 1; sleep 0.2
  done
}

# -------------------------------------------------------------------------------
# Stage 2 — identity resolution through the daemon (§12), both directions.
# add-node by RAW PATH echoes the captured usb: identity (input->identity); that
# same identity is then used as a device string (identity->path, squatter-safe).
# -------------------------------------------------------------------------------
STAGE="identity"
cat > "$TMPD/probe.toml" <<EOF
[[node]]
type = "serial"
name = "probe0"
device = "$P0"
EOF
ID0=$("$C" --socket "$SOCK" --json add-node "$TMPD/probe.toml" | jq -r '.identity // ""')
case "$ID0" in usb:*|by-path:*) : ;; *) fail "add-node echoed no resolver identity for $P0 (got '$ID0')" ;; esac
cat > "$TMPD/probe1.toml" <<EOF
[[node]]
type = "serial"
name = "probe1"
device = "$P1"
EOF
ID1=$("$C" --socket "$SOCK" --json add-node "$TMPD/probe1.toml" | jq -r '.identity // ""')
case "$ID1" in usb:*|by-path:*) : ;; *) fail "add-node echoed no resolver identity for $P1 (got '$ID1')" ;; esac
"$C" --socket "$SOCK" teardown >/dev/null 2>&1

# -------------------------------------------------------------------------------
# Stage 3 — symmetric null-modem graph (§4 symmetric config in physical form),
# devices addressed by IDENTITY (proves identity->path). Injector PTYs on each TX,
# capture logs on each RX. free-for-all so injectors write without lock ceremony.
# -------------------------------------------------------------------------------
STAGE="load-symmetric"
cat > "$TMPD/sym.toml" <<EOF
[[node]]
type = "serial"
name = "port0"
device = "$ID0"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "$ID1"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "log"
name = "rx0"
directory = "$TMPD"
filename = "rx0.log"
[[node]]
type = "log"
name = "rx1"
directory = "$TMPD"
filename = "rx1.log"
[[node]]
type = "pty"
name = "inj0"
path = "$TMPD/inj0"
[[node]]
type = "pty"
name = "inj1"
path = "$TMPD/inj1"
[[edge]]
a = "port0"
b = "rx0"
write_mode = "never"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
[[edge]]
a = "port0"
b = "inj0"
[[edge]]
a = "port1"
b = "inj1"
EOF
"$C" --socket "$SOCK" load "$TMPD/sym.toml" >/dev/null 2>&1 || { cat "$TMPD/daemon.log"; fail "load symmetric config failed"; }
wait_active port0 port1 || { cat "$TMPD/daemon.log"; fail "serial nodes not active (device access / cabling)"; }
# identity->path (squatter-safe, §12): the usb: identity resolved to the real path.
[ "$(nf port0 .resolved_path)" = "$P0" ] || fail "port0 identity $ID0 resolved to $(nf port0 .resolved_path), expected $P0"
[ "$(nf port1 .resolved_path)" = "$P1" ] || fail "port1 identity $ID1 resolved to $(nf port1 .resolved_path), expected $P1"

# -- Check A: bidirectional byte-exact data path over the physical wire (§4/§5/§7.1)
STAGE="data-path"
inject_verify() { # inj_path rx_log seed dir_label
  local inj="$1" rx="$2" seed="$3" label="$4" bytes=32768
  local base; base=$(stat -c%s "$rx" 2>/dev/null || echo 0)
  local v; v=$("$SIM" client --path "$inj" --send "seeded:32KiB" --seed "$seed" --timeout-ms 30000 2>/dev/null)
  local sent; sent=$(echo "$v" | jq -r '.sha256_sent')
  wait_size "$rx" "$((base + bytes))" 30 || fail "$label: only $(( $(stat -c%s "$rx") - base ))/$bytes B crossed the wire"
  local got; got=$(tail -c +"$((base + 1))" "$rx" | sha256sum | cut -d' ' -f1)
  [ "$sent" = "$got" ] || fail "$label: checksum mismatch (sent $sent, received $got) — bytes lost/reordered on the wire"
}
inject_verify "$TMPD/inj0" "$TMPD/rx1.log" 21 "port0->port1"
inject_verify "$TMPD/inj1" "$TMPD/rx0.log" 22 "port1->port0"

# -- Check B: the `send` verb reaches real hardware (§6/§10)
STAGE="send-verb"
NONCE="SNX_SEND_$$_$(date +%s)"
"$C" --socket "$SOCK" send port0 --line "$NONCE" >/dev/null 2>&1 || fail "send verb failed"
end=$(( $(date +%s) + 5 )); ok=0
while [ "$(date +%s)" -lt "$end" ]; do tail -c 200 "$TMPD/rx1.log" | grep -aq "$NONCE" && { ok=1; break; }; sleep 0.1; done
[ "$ok" = 1 ] || fail "send-verb nonce '$NONCE' never arrived at port1"

# -- Check C: far-side break reception (§7.1 Tier-3) — a break on port0 counts at port1
STAGE="break"
B0=$(nf port1 .driver_counters.brk); [ -n "$B0" ] && [ "$B0" != "null" ] || B0=0
"$C" --socket "$SOCK" send-break port0 --ms 60 >/dev/null 2>&1 || fail "send-break failed"
end=$(( $(date +%s) + 4 )); ok=0
while [ "$(date +%s)" -lt "$end" ]; do
  B1=$(nf port1 .driver_counters.brk); [ "${B1:-0}" != "null" ] && [ "${B1:-0}" -gt "$B0" ] && { ok=1; break; }; sleep 0.2
done
[ "$ok" = 1 ] || fail "break on port0 was not observed at port1 (brk counter did not rise from $B0)"

# -- Check D: TIOCEXCL exclusivity (§7.1 Tier-1) — a 2nd opener is refused while held
STAGE="exclusivity"
if timeout 3 bash -c "exec 3<>$P0" 2>/dev/null; then
  fail "a second open of $P0 succeeded while the daemon holds it — TIOCEXCL not enforced"
fi

# -------------------------------------------------------------------------------
# Stage 4 — exclusive write arbitration on real hardware (§6): lock -> LOCKED -> steal.
# -------------------------------------------------------------------------------
STAGE="arbitration"
cat > "$TMPD/excl.toml" <<EOF
[[node]]
type = "serial"
name = "port0"
device = "$ID0"
baud = 115200
arbitration = "exclusive"
[[node]]
type = "serial"
name = "port1"
device = "$ID1"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "log"
name = "rx1"
directory = "$TMPD"
filename = "arb_rx1.log"
[[node]]
type = "pty"
name = "inj0"
path = "$TMPD/inj0"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
[[edge]]
a = "port0"
b = "inj0"
EOF
"$C" --socket "$SOCK" load --replace "$TMPD/excl.toml" >/dev/null 2>&1 || fail "load exclusive config failed"
wait_active port0 port1 || fail "nodes not active after exclusive load"
"$C" --socket "$SOCK" lock inj0 >/dev/null 2>&1 || fail "lock inj0 failed"
[ "$(nf port0 .lock.holder)" = "inj0" ] || fail "inj0 did not become the lock holder"
# A contending send must fail with the LOCKED error (-32003).
if "$C" --socket "$SOCK" send port0 --line "MUSTNOTARRIVE" --timeout-ms 800 >/dev/null 2>&1; then
  fail "a contending send succeeded while inj0 holds the lock (exclusivity broken)"
fi
# steal delivers the line and it reaches port1.
STEALN="SNX_STEAL_$$_$(date +%s)"
"$C" --socket "$SOCK" send port0 --steal --line "$STEALN" >/dev/null 2>&1 || fail "send --steal failed"
end=$(( $(date +%s) + 5 )); ok=0
while [ "$(date +%s)" -lt "$end" ]; do tail -c 200 "$TMPD/arb_rx1.log" | grep -aq "$STEALN" && { ok=1; break; }; sleep 0.1; done
[ "$ok" = 1 ] || fail "stolen send '$STEALN' never reached port1"

# -------------------------------------------------------------------------------
# Stage 5 — slow-consumer drop isolation (§5): a throttled PTY spy drops-with-
# counters while a co-attached log stays byte-exact complete; loss accounts exactly.
# -------------------------------------------------------------------------------
STAGE="slow-consumer-drop"
cat > "$TMPD/slow.toml" <<EOF
[[node]]
type = "serial"
name = "port0"
device = "$ID0"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "$ID1"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "src0"
path = "$TMPD/src0"
[[node]]
type = "pty"
name = "slowcon"
path = "$TMPD/slowcon"
hostward_buffer = 16
[[node]]
type = "log"
name = "rx1"
directory = "$TMPD"
filename = "slow_rx1.log"
[[edge]]
a = "port0"
b = "src0"
[[edge]]
a = "port1"
b = "slowcon"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
EOF
"$C" --socket "$SOCK" load --replace "$TMPD/slow.toml" >/dev/null 2>&1 || fail "load slow-consumer config failed"
wait_active port0 port1 || fail "nodes not active after slow-consumer load"
BURST=65536
"$SIM" client --path "$TMPD/slowcon" --drain --read-rate 2000 --quiet-ms 2500 --timeout-ms 90000 >"$TMPD/slow.json" 2>/dev/null &
SPID=$!
wait_present slowcon 8 || fail "slow consumer never attached"
SND=$("$SIM" client --path "$TMPD/src0" --send "seeded:64KiB" --seed 55 --timeout-ms 90000 2>/dev/null)
SSHA=$(echo "$SND" | jq -r '.sha256_sent')
wait_size "$TMPD/slow_rx1.log" "$BURST" 60 || fail "log did not capture the full $BURST B (a fast consumer dropped?)"
wait "$SPID" 2>/dev/null || true
LSHA=$(sha256sum "$TMPD/slow_rx1.log" | cut -d' ' -f1)
[ "$SSHA" = "$LSHA" ] || fail "co-attached log is not byte-exact complete (isolation broken)"
RECV=$(jq -r '.received // 0' "$TMPD/slow.json")
DROP=$(nf slowcon .dropped_slow_consumer); DROP=${DROP:-0}
[ "$DROP" -gt 0 ] || fail "slow consumer did not drop (its bounded buffer never overflowed) — expected drop-with-counters (§5)"
[ "$((RECV + DROP))" -eq "$BURST" ] || fail "loss accounting off: received($RECV)+dropped($DROP) != sent($BURST)"

# -------------------------------------------------------------------------------
# Stage 6 — error counters observable through the daemon (§5/§15.21): a deliberate
# baud mismatch corrupts data AND raises the framing-error counter.
# -------------------------------------------------------------------------------
STAGE="baud-mismatch"
cat > "$TMPD/mm.toml" <<EOF
[[node]]
type = "serial"
name = "port0"
device = "$ID0"
baud = 115200
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "$ID1"
baud = 9600
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "src0"
path = "$TMPD/src0"
[[node]]
type = "log"
name = "rx1"
directory = "$TMPD"
filename = "mm_rx1.log"
[[edge]]
a = "port0"
b = "src0"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
EOF
"$C" --socket "$SOCK" load --replace "$TMPD/mm.toml" >/dev/null 2>&1 || fail "load mismatch config failed"
wait_active port0 port1 || fail "nodes not active after mismatch load"
F0=$(nf port1 .driver_counters.frame); F0=${F0:-0}; [ "$F0" = "null" ] && F0=0
"$SIM" client --path "$TMPD/src0" --send "seeded:4KiB" --seed 77 --timeout-ms 20000 >/dev/null 2>&1
end=$(( $(date +%s) + 5 )); ok=0
while [ "$(date +%s)" -lt "$end" ]; do
  F1=$(nf port1 .driver_counters.frame); [ "${F1:-0}" != "null" ] && [ "${F1:-0}" -gt "$F0" ] && { ok=1; break; }; sleep 0.3
done
[ "$ok" = 1 ] || fail "deliberate baud mismatch raised no framing-error counter (expected observable loss, §5)"

# -------------------------------------------------------------------------------
# Stage 7 — parity-error counter observable through the daemon (§5).
# -------------------------------------------------------------------------------
STAGE="parity-error"
cat > "$TMPD/par.toml" <<EOF
[[node]]
type = "serial"
name = "port0"
device = "$ID0"
baud = 115200
parity = "odd"
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "port1"
device = "$ID1"
baud = 115200
parity = "even"
arbitration = "free-for-all"
[[node]]
type = "pty"
name = "src0"
path = "$TMPD/src0"
[[node]]
type = "log"
name = "rx1"
directory = "$TMPD"
filename = "par_rx1.log"
[[edge]]
a = "port0"
b = "src0"
[[edge]]
a = "port1"
b = "rx1"
write_mode = "never"
EOF
"$C" --socket "$SOCK" load --replace "$TMPD/par.toml" >/dev/null 2>&1 || fail "load parity config failed"
wait_active port0 port1 || fail "nodes not active after parity load"
P0C=$(nf port1 .driver_counters.parity); P0C=${P0C:-0}; [ "$P0C" = "null" ] && P0C=0
"$SIM" client --path "$TMPD/src0" --send "seeded:4KiB" --seed 88 --timeout-ms 20000 >/dev/null 2>&1
end=$(( $(date +%s) + 5 )); ok=0
while [ "$(date +%s)" -lt "$end" ]; do
  P1C=$(nf port1 .driver_counters.parity); [ "${P1C:-0}" != "null" ] && [ "${P1C:-0}" -gt "$P0C" ] && { ok=1; break; }; sleep 0.3
done
[ "$ok" = 1 ] || fail "mismatched parity raised no parity-error counter (expected observable, §5)"

"$C" --socket "$SOCK" teardown >/dev/null 2>&1
pass_json all
