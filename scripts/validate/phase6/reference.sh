#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 1): the reference topology (§2), scripted.
# Two daemons in separate runtime dirs, joined over a loopback (unix) leg:
#   Daemon A (sender, faces=target): two echo "devices" behind serial nodes, each
#     feeding one channel of a connect-role leg.
#   Daemon B (receiver, faces=host): a listen-role leg whose channels fan out to
#     local PTYs, where operators sit.
# An operator on each B-side PTY sends a seeded stream targetward; it crosses
# B → wire → A → device (which echoes) → A → wire → B and returns. Per-channel
# checksums must match exactly — bytes traverse device ↔ remote-clients end to end.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-reference\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p6r.XXXXXX) || fail "mktemp"
mkdir -p "$TMPD/a" "$TMPD/b"
SOCKA="$TMPD/a/serialnexusd.sock"
SOCKB="$TMPD/b/serialnexusd.sock"
LEG="$TMPD/leg.sock"
CA="$C --socket $SOCKA"
CB="$C --socket $SOCKB"
cleanup() {
  for p in "${DEVPIDS[@]:-}" "${CLIPIDS[@]:-}" "${DA:-}" "${DB:-}"; do kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# Two echo devices (the targets), standing where /dev/ttyUSB* would.
DEVPIDS=()
"$SIM" pty --echo --link "$TMPD/dev0" --timeout-ms 30000 >"$TMPD/dev0.json" 2>&1 & DEVPIDS+=($!)
"$SIM" pty --echo --link "$TMPD/dev1" --timeout-ms 30000 >"$TMPD/dev1.json" 2>&1 & DEVPIDS+=($!)
bash "$WAIT" "test -e '$TMPD/dev0' && test -e '$TMPD/dev1'" 5 0.05 || fail "devices never appeared"

# Daemon B (receiver) first, so its leg is listening before A dials in.
XDG_RUNTIME_DIR="$TMPD/b" "$D" >"$TMPD/b.log" 2>&1 & DB=$!
bash "$WAIT" "test -S '$SOCKB'" 5 0.05 || { cat "$TMPD/b.log"; fail "daemon B socket never appeared"; }
cat > "$TMPD/b.toml" <<EOF
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "unix"
role = "listen"
address = "$LEG"
arbitration = "free-for-all"
channels = ["c0", "c1"]
[[node]]
type = "pty"
name = "p0"
path = "$TMPD/p0"
[[node]]
type = "pty"
name = "p1"
path = "$TMPD/p1"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
[[edge]]
a = "downlink/c1"
b = "p1"
write_mode = "on-demand"
EOF
$CB load "$TMPD/b.toml" >/dev/null || { cat "$TMPD/b.log"; fail "daemon B load failed"; }

# Daemon A (sender): two echo devices behind serials, feeding a connect-role leg.
XDG_RUNTIME_DIR="$TMPD/a" "$D" >"$TMPD/a.log" 2>&1 & DA=$!
bash "$WAIT" "test -S '$SOCKA'" 5 0.05 || { cat "$TMPD/a.log"; fail "daemon A socket never appeared"; }
cat > "$TMPD/a.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/dev0"
arbitration = "free-for-all"
[[node]]
type = "serial"
name = "usb1"
device = "$TMPD/dev1"
arbitration = "free-for-all"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "unix"
role = "connect"
address = "$LEG"
channels = ["c0", "c1"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
[[edge]]
a = "usb1"
b = "uplink/c1"
write_mode = "on-demand"
EOF
$CA load "$TMPD/a.toml" >/dev/null || { cat "$TMPD/a.log"; fail "daemon A load failed"; }

# Both legs connect and bind both channels.
bash "$WAIT" "$CB --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection==\"connected\" and .channels.c0.binding==\"bound\" and .channels.c1.binding==\"bound\"'" 8 0.1 \
  || { cat "$TMPD/b.log"; $CB --json state >&2; fail "receiver leg never bound both channels"; }
bash "$WAIT" "$CA --json state | jq -e '.nodes[]|select(.name==\"uplink\")|.connection==\"connected\" and .channels.c0.binding==\"bound\" and .channels.c1.binding==\"bound\"'" 8 0.1 \
  || { cat "$TMPD/a.log"; $CA --json state >&2; fail "sender leg never bound both channels"; }

# An operator on each B-side PTY sends a distinct seeded stream and expects the
# device's echo back — the full device ↔ remote-client round trip, per channel.
CLIPIDS=()
"$SIM" client --path "$TMPD/p0" --send seeded:32KiB --expect echo --seed 101 --timeout-ms 15000 >"$TMPD/c0.json" 2>&1 & CLIPIDS+=($!)
"$SIM" client --path "$TMPD/p1" --send seeded:32KiB --expect echo --seed 202 --timeout-ms 15000 >"$TMPD/c1.json" 2>&1 & CLIPIDS+=($!)
rc=0
for p in "${CLIPIDS[@]}"; do wait "$p" || rc=1; done
if [ "$rc" != 0 ]; then cat "$TMPD/c0.json" "$TMPD/c1.json" >&2; cat "$TMPD/a.log" "$TMPD/b.log" >&2; fail "a per-channel echo round-trip failed"; fi
for f in c0 c1; do
  jq -e '.pass==true and .sent==32768 and .received==32768 and (.sha256_sent==.sha256_received)' "$TMPD/$f.json" >/dev/null \
    || { cat "$TMPD/$f.json" >&2; fail "$f: checksums did not reconcile end to end"; }
done

# Both directions advanced on both legs' channels (device data hostward, commands
# targetward), so the counters corroborate the checksums.
$CB --json state | jq -e '.nodes[]|select(.name=="downlink")|.channels.c0.accepted_targetward>=32768 and .channels.c0.delivered_hostward>=32768' >/dev/null \
  || { $CB --json state >&2; fail "receiver leg counters did not advance in both directions"; }

$CA shutdown >/dev/null; $CB shutdown >/dev/null
echo '{"check":"phase6-reference","pass":true}'
