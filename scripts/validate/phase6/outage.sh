#!/usr/bin/env bash
# Phase 6 validation (plan §Phase 6, item 4): outage semantics. A tcp-proxy
# between two daemons severs the link mid-stream (`--drop-after`) and restores it
# (`--restore-after`). During the gap the leg is faulted-and-wait — targetward
# writers pause (backpressure, no drop). After restore the connect-role leg
# reconnects, purge-on-reconnect discards outage-era targetward backlog with a
# counter (§7.4), and a fresh round-trip is byte-clean.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase6-outage\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"
free_port() { python3 -c "import socket;s=socket.socket();s.bind(('127.0.0.1',0));print(s.getsockname()[1]);s.close()"; }

TMPD=$(mktemp -d /tmp/snx-p6o.XXXXXX) || fail "mktemp"
mkdir -p "$TMPD/a" "$TMPD/b"
SOCKA="$TMPD/a/serialnexusd.sock"; SOCKB="$TMPD/b/serialnexusd.sock"
CA="$C --socket $SOCKA"; CB="$C --socket $SOCKB"
PORT_B=$(free_port); PORT_P=$(free_port)
cleanup() {
  for p in "${DEV:-}" "${PROXY:-}" "${CLI:-}" "${DA:-}" "${DB:-}"; do kill "$p" 2>/dev/null; done
  rm -rf "$TMPD"
}
trap cleanup EXIT

# Echo device behind daemon A's serial.
"$SIM" pty --echo --link "$TMPD/dev0" --timeout-ms 60000 >"$TMPD/dev0.json" 2>&1 & DEV=$!
bash "$WAIT" "test -e '$TMPD/dev0'" 5 0.05 || fail "device never appeared"

# Daemon B (receiver, listen on PORT_B).
XDG_RUNTIME_DIR="$TMPD/b" "$D" >"$TMPD/b.log" 2>&1 & DB=$!
bash "$WAIT" "test -S '$SOCKB'" 5 0.05 || { cat "$TMPD/b.log"; fail "daemon B socket"; }
cat > "$TMPD/b.toml" <<EOF
[[node]]
type = "leg"
name = "downlink"
faces = "host"
transport = "tcp"
role = "listen"
address = "127.0.0.1:$PORT_B"
arbitration = "free-for-all"
channels = ["c0"]
[[node]]
type = "pty"
name = "p0"
path = "$TMPD/p0"
[[edge]]
a = "downlink/c0"
b = "p0"
write_mode = "on-demand"
EOF
$CB load "$TMPD/b.toml" >/dev/null || { cat "$TMPD/b.log"; fail "daemon B load"; }

# The proxy: A dials PORT_P; the proxy forwards to PORT_B, severing after 8KiB of
# A's outward (hostward echo) flow, then restoring after a 2.5s outage window.
"$SIM" tcp-proxy --listen "127.0.0.1:$PORT_P" --connect "127.0.0.1:$PORT_B" \
  --drop-after 8KiB --restore-after-ms 2500 --timeout-ms 40000 >"$TMPD/proxy.json" 2>&1 & PROXY=$!

# Daemon A (sender, connect to PORT_P through the proxy).
XDG_RUNTIME_DIR="$TMPD/a" "$D" >"$TMPD/a.log" 2>&1 & DA=$!
bash "$WAIT" "test -S '$SOCKA'" 5 0.05 || { cat "$TMPD/a.log"; fail "daemon A socket"; }
cat > "$TMPD/a.toml" <<EOF
[[node]]
type = "serial"
name = "usb0"
device = "$TMPD/dev0"
arbitration = "free-for-all"
[[node]]
type = "leg"
name = "uplink"
faces = "target"
transport = "tcp"
role = "connect"
address = "127.0.0.1:$PORT_P"
reconnect_initial_ms = 150
reconnect_max_ms = 600
channels = ["c0"]
[[edge]]
a = "usb0"
b = "uplink/c0"
write_mode = "on-demand"
EOF
$CA load "$TMPD/a.toml" >/dev/null || { cat "$TMPD/a.log"; fail "daemon A load"; }

# Both legs connect and bind.
bash "$WAIT" "$CB --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection==\"connected\" and .channels.c0.binding==\"bound\"'" 8 0.1 \
  || { cat "$TMPD/b.log"; fail "receiver leg never bound"; }
bash "$WAIT" "$CA --json state | jq -e '.nodes[]|select(.name==\"uplink\")|.connection==\"connected\"'" 8 0.1 \
  || { cat "$TMPD/a.log"; fail "sender leg never connected"; }

# 1. Pre-outage: a small round-trip is clean (well under the 8KiB drop threshold).
"$SIM" client --path "$TMPD/p0" --send seeded:4KiB --expect echo --seed 11 --timeout-ms 8000 >"$TMPD/pre.json" 2>&1 \
  || { cat "$TMPD/pre.json" "$TMPD/a.log" "$TMPD/b.log" >&2; fail "pre-outage round-trip failed"; }
jq -e '.pass==true and .sha256_sent==.sha256_received' "$TMPD/pre.json" >/dev/null || fail "pre-outage checksum mismatch"

# 2. A burst whose echo crosses the 8KiB threshold trips the outage (its own
#    round-trip is interrupted; not asserted).
"$SIM" client --path "$TMPD/p0" --send seeded:64KiB --expect echo --seed 22 --timeout-ms 3000 >"$TMPD/burst.json" 2>&1 & CLI=$!

# 3. The receiver leg detects the outage: it stops being connected while the link
#    is down (faulted-and-wait). During this window a writer's bytes back up,
#    paused not dropped.
bash "$WAIT" "$CB --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection!=\"connected\"'" 15 0.1 \
  || { cat "$TMPD/b.log"; $CB --json state >&2; fail "receiver leg never registered the outage"; }
kill "$CLI" 2>/dev/null; CLI=""

# 4. An operator types targetward *during* the outage. With the leg disconnected,
#    these bytes back up at the receiver (paused, not dropped) — exactly the stale
#    command hazard purge-on-reconnect exists to defuse (§6/§7.4).
"$SIM" client --path "$TMPD/p0" --send seeded:12KiB --seed 99 --timeout-ms 2000 >"$TMPD/stale.json" 2>&1 || true

# 5. After restore the connect-role leg reconnects (reconnect_count rises,
#    connection returns to connected, channel rebinds).
bash "$WAIT" "$CB --json state | jq -e '.nodes[]|select(.name==\"downlink\")|.connection==\"connected\" and .channels.c0.binding==\"bound\" and .reconnect_count>=1'" 20 0.2 \
  || { cat "$TMPD/b.log" "$TMPD/proxy.json" >&2; $CB --json state >&2; fail "receiver leg never reconnected after restore"; }

# 6. Purge-on-reconnect: the outage-era targetward backlog was discarded with a
#    counter (§7.4), so stale commands never fire post-restore.
purged=$($CB --json state | jq '[.nodes[]|select(.name=="downlink")|.channels.c0.purged_on_reconnect] | add // 0')
[ "$purged" -gt 0 ] 2>/dev/null \
  || { $CB --json state >&2; fail "purge-on-reconnect counter did not record outage-era backlog (got $purged)"; }

# 7. Post-restore: a fresh round-trip is byte-clean (the data plane recovered).
"$SIM" client --path "$TMPD/p0" --send seeded:4KiB --expect echo --seed 33 --timeout-ms 8000 >"$TMPD/post.json" 2>&1 \
  || { cat "$TMPD/post.json" "$TMPD/a.log" "$TMPD/b.log" >&2; fail "post-restore round-trip failed"; }
jq -e '.pass==true and .sha256_sent==.sha256_received' "$TMPD/post.json" >/dev/null || fail "post-restore checksum mismatch"

$CA shutdown >/dev/null; $CB shutdown >/dev/null
echo '{"check":"phase6-outage","pass":true}'
