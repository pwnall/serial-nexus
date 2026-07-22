#!/usr/bin/env bash
# Plan §10.5 / §15.26: the exec-conformance harness drives an external codec child
# through golden vectors, full-duplex liveness (the §15.22 deadlock class),
# fragmented-frame reassembly, and kill-and-restart cleanliness. The stdlib Python
# passthrough passes every check; the deliberately half-duplex fixture fails the
# liveness case, so the harness CATCHES the deadlock class rather than shipping it.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"exec-conformance\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

command -v python3 >/dev/null 2>&1 || fail "python3 not found"
cargo build -q -p nexus-sim || fail "build failed"
SIM="$REPO_ROOT/target/debug/nexus-sim"

# (1) The full-duplex passthrough passes every conformance check.
OK="$("$SIM" exec-conformance --exec "python3 $REPO_ROOT/tests/ext-codec/passthrough.py")"
printf '%s' "$OK" | jq -e '
  .pass == true
  and .checks.golden and .checks.liveness
  and .checks.fragmentation and .checks.restart
' >/dev/null || { echo "$OK"; fail "the passthrough failed a conformance check"; }

# (2) A CORRECT bounded-lag codec (echoes one frame behind, flushes at EOF) still
# passes every check — the check is not a lock-step ping-pong that would reject any
# legitimately buffering codec (§15.26).
LAG="$("$SIM" exec-conformance --exec "python3 $REPO_ROOT/tests/ext-codec/lag.py")"
printf '%s' "$LAG" | jq -e '
  .pass == true and .checks.liveness == true and .checks.restart == true
' >/dev/null || { echo "$LAG"; fail "a valid bounded-lag codec was wrongly rejected"; }

# (3) The deliberately half-duplex fixture is CAUGHT: golden still passes (finite,
# closed input), but liveness fails — the §15.22 deadlock class, made a test.
BAD="$("$SIM" exec-conformance \
  --exec "python3 $REPO_ROOT/tests/ext-codec/half-duplex.py" --frame-timeout-ms 800)"
printf '%s' "$BAD" | jq -e '
  .pass == false and .checks.liveness == false and .checks.golden == true
' >/dev/null || { echo "$BAD"; fail "the half-duplex fixture was not caught by liveness"; }

echo '{"check":"exec-conformance","pass":true}'
