#!/usr/bin/env bash
# Phase 1 validation: the pure contracts and their property tests pass
# (nexus-core graph/data/config, codec-api envelope + golden vectors, nexus-rpc).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"

if cargo test -q -p nexus-core -p codec-api -p nexus-rpc >/dev/null 2>&1; then
  echo '{"check":"phase1-unit-tests","crates":["nexus-core","codec-api","nexus-rpc"],"pass":true}'
else
  echo '{"check":"phase1-unit-tests","pass":false}' >&2
  exit 1
fi
