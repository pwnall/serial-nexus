#!/usr/bin/env bash
# Phase 4 validation (arbitration, §6): the per-endpoint exclusive write lock.
# Two on-demand PTYs and a write=never spy fan into one serial endpoint (a legal
# §4 fan-out). Only the lock holder's bytes are read targetward; a non-holder is
# paused (its bytes buffer, never fire) and a spy cannot contend at all.
#
# No hardware (§15.17): the "device" is a nexus-sim sink that records exactly what
# reaches "hardware", so exclusivity is a byte-exact checksum comparison, not a
# judgement call.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"phase4-exclusivity\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

cargo build -q -p serialnexusd -p serialnexusctl -p nexus-sim || fail "build failed"
D="$REPO_ROOT/target/debug/serialnexusd"
C="$REPO_ROOT/target/debug/serialnexusctl"
SIM="$REPO_ROOT/target/debug/nexus-sim"
WAIT="$REPO_ROOT/scripts/lib/wait-for.sh"

TMPD=$(mktemp -d /tmp/snx-p4x.XXXXXX) || fail "mktemp"
export XDG_RUNTIME_DIR="$TMPD"
SOCK="$TMPD/serialnexusd.sock"
DEV="$TMPD/dev"          # the device pts (serial node opens this)
TA="$TMPD/ttyA"          # holder PTY
TB="$TMPD/ttyB"          # locked-out on-demand PTY
TS="$TMPD/ttyS"          # write=never spy PTY
LEN_A=65536; SEED_A=42   # the holder's stream
LEN_B=512;   SEED_B=7    # the locked-out writer's stream (fits the PTY buffer)
LEN_S=512;   SEED_S=9    # the spy's stray write
cleanup() {
  [ -n "${DPID:-}" ] && kill "$DPID" 2>/dev/null
  [ -n "${SINKPID:-}" ] && kill "$SINKPID" 2>/dev/null
  rm -rf "$TMPD"
}
trap cleanup EXIT

# The "device": a sink recording exactly LEN_A bytes that reach hardware.
"$SIM" pty --sink --bytes "$LEN_A" --link "$DEV" --timeout-ms 20000 >"$TMPD/sink.json" 2>&1 &
SINKPID=$!
bash "$WAIT" "test -e '$DEV'" 5 0.05 || fail "device never appeared"

"$D" >"$TMPD/daemon.log" 2>&1 &
DPID=$!
bash "$WAIT" "test -S '$SOCK'" 5 0.05 || { cat "$TMPD/daemon.log"; fail "socket never appeared"; }

# serial(usb0, host) ← three target-facing consumers: two on-demand PTYs and one
# write=never spy (§6). The spy's edge is explicitly write=never.
cat > "$TMPD/g.toml" <<EOF
[[node]]
type = "pty"
name = "ptya"
path = "$TA"
[[node]]
type = "pty"
name = "ptyb"
path = "$TB"
[[node]]
type = "pty"
name = "spy"
path = "$TS"
[[node]]
type = "serial"
name = "usb0"
device = "$DEV"
[[edge]]
a = "usb0"
b = "ptya"
[[edge]]
a = "usb0"
b = "ptyb"
[[edge]]
a = "usb0"
b = "spy"
write_mode = "never"
EOF
"$C" load "$TMPD/g.toml" >/dev/null || { cat "$TMPD/daemon.log"; fail "load failed"; }

usb0() { "$C" --json state | jq -e ".nodes[]|select(.name==\"usb0\")|$1" >/dev/null; }

# The endpoint reports its lock (§6): exclusive by default, no holder yet, three
# origins with the right write modes.
usb0 '.status=="active"' || { cat "$TMPD/daemon.log"; fail "usb0 not active"; }
usb0 '.lock.arbitration=="exclusive" and .lock.holder==null' || fail "lock not exclusive/free at start"
usb0 '(.lock.origins|map(.origin)|sort)==["ptya","ptyb","spy"]' || fail "lock origins wrong"
usb0 '.lock.origins[]|select(.origin=="spy")|.write_mode=="never"' || fail "spy edge not write=never"

# Grab the lock for the holder.
"$C" --json lock ptya | jq -e '.acquired==true and .held==true' >/dev/null \
  || { cat "$TMPD/daemon.log"; fail "lock ptya failed"; }
usb0 '.lock.holder=="ptya"' || fail "holder not ptya after acquire"
usb0 '.lock.origins[]|select(.origin=="ptya")|.holds_lock==true' || fail "ptya not marked holds_lock"

# A non-holder cannot acquire while ptya holds it (§6): refused with a locked error.
if "$C" lock ptyb 2>"$TMPD/lockb.err"; then
  cat "$TMPD/daemon.log"; fail "lock ptyb should have been refused while ptya holds it"
fi
grep -qi 'locked' "$TMPD/lockb.err" || { cat "$TMPD/lockb.err"; fail "lock ptyb refused, but not with a locked error"; }

# The locked-out writers attach, send, and HOLD their slaves open while the holder
# streams — so a broken gate would drain their buffered bytes and leak them into
# the device. ptyb is a paused non-holder (its bytes must stay buffered); the spy
# is write=never (it cannot write at all). Holding open (not send-then-close) is
# what makes this a genuine test of the lock gate rather than of a close race.
"$SIM" client --path "$TB" --send "seeded:$LEN_B" --seed "$SEED_B" --hold-ms 6000 --timeout-ms 8000 >/dev/null 2>&1 &
PB=$!
"$SIM" client --path "$TS" --send "seeded:$LEN_S" --seed "$SEED_S" --hold-ms 6000 --timeout-ms 8000 >/dev/null 2>&1 &
PS=$!
# Wait until the locked-out writer is present (its bytes are then buffered), so the
# exclusivity check below actually exercises the gate.
bash "$WAIT" "\"$C\" --json state | jq -e '.nodes[]|select(.name==\"ptyb\")|.client_present==true'" 5 0.05 \
  || { cat "$TMPD/daemon.log"; fail "locked-out writer never became present"; }

# The holder sends; only its bytes may flow to the device.
"$SIM" client --path "$TA" --send "seeded:$LEN_A" --seed "$SEED_A" --timeout-ms 15000 >"$TMPD/a.json" 2>&1 \
  || { cat "$TMPD/daemon.log" "$TMPD/a.json"; fail "holder send failed"; }
kill "$PB" "$PS" 2>/dev/null || true

# The device must have received exactly the holder's bytes — byte-exact
# exclusivity. Both checksums are computed by nexus-sim from the same seed, so a
# match proves the device saw A's stream and nothing else.
wait "$SINKPID" 2>/dev/null || true
SHA_A=$(jq -r '.sha256_sent' "$TMPD/a.json")
SHA_SINK=$(jq -r '.sha256 // ""' "$TMPD/sink.json")
RECV=$(jq -r '.received // -1' "$TMPD/sink.json")
[ "$RECV" = "$LEN_A" ] || { cat "$TMPD/daemon.log" "$TMPD/sink.json"; fail "device received $RECV bytes, expected $LEN_A (a non-holder leaked?)"; }
[ -n "$SHA_A" ] && [ "$SHA_A" = "$SHA_SINK" ] || { cat "$TMPD/daemon.log"; fail "device checksum != seeded-A (exclusivity broken)"; }

# Release cleanly.
"$C" --json unlock ptya | jq -e '.released==true' >/dev/null || fail "unlock ptya failed"
usb0 '.lock.holder==null' || fail "holder not cleared after unlock"

"$C" shutdown >/dev/null
echo '{"check":"phase4-exclusivity","pass":true}'
