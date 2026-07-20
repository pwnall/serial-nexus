#!/usr/bin/env bash
# Compare two serial_nexus configuration dumps for semantic equality (§3, §11
# "dump round-trips"). Normalizes each by parsing (TOML or JSON) and re-emitting
# canonical key-sorted JSON, so formatting/ordering differences don't count.
set -uo pipefail

a="${1:?usage: semantic-diff.sh <a> <b>}"
b="${2:?usage: semantic-diff.sh <a> <b>}"

norm() {
  python3 - "$1" <<'PY'
import sys, json
data = open(sys.argv[1], 'rb').read()
try:
    import tomllib
    obj = tomllib.loads(data.decode())
except Exception:
    obj = json.loads(data.decode())
json.dump(obj, sys.stdout, sort_keys=True, indent=2)
print()
PY
}

if diff <(norm "$a") <(norm "$b") >&2; then
  echo '{"check":"semantic-diff","equal":true,"pass":true}'
  exit 0
fi
echo '{"check":"semantic-diff","equal":false,"pass":false}' >&2
exit 1
