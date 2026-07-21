#!/usr/bin/env bash
# Phase 5 validation (plan §Phase 5, item 3): any-language envelope conformance.
# `nexus-sim envelope` drives an external codec child — here the stdlib-only
# Python passthrough — through the golden-vector battery: every event kind plus
# edge cases, encoded to the child's stdin, decoded back from its stdout, and
# compared frame for frame. A conforming child re-emits the exact sequence (§8).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

if ! command -v python3 >/dev/null 2>&1; then
  echo '{"check":"envelope","pass":false,"reason":"python3 not found"}'
  exit 1
fi

cargo build -q -p nexus-sim || {
  echo '{"check":"envelope","pass":false,"reason":"build failed"}'
  exit 1
}
SIM="$REPO_ROOT/target/debug/nexus-sim"

OUT="$("$SIM" envelope --exec "python3 $REPO_ROOT/tests/ext-codec/passthrough.py")"
echo "$OUT"
printf '%s' "$OUT" | jq -e '
  .pass == true
  and .sent_frames == .received_frames
  and .received_frames == 10
  and .trailing_bytes == 0
' >/dev/null
