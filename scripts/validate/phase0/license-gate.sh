#!/usr/bin/env bash
# Phase 0 validation: prove the §13 licensing gate actually rejects a banned
# crate rather than merely being configured to (§2: "the gate is proven, not
# assumed"). Injects `serialport` into a scratch crate and asserts cargo-deny
# fails there while the clean tree passes.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fail() { echo "FAIL: $*" >&2; echo '{"check":"license-gate","pass":false}'; exit 1; }

command -v cargo-deny >/dev/null 2>&1 || fail "cargo-deny not installed"

# 1. The clean tree must pass the ban check.
if ! cargo deny --manifest-path "$REPO_ROOT/Cargo.toml" check bans 2>/dev/null; then
  fail "clean tree unexpectedly fails the ban check"
fi

# 2. A scratch crate that pulls in a banned crate must fail the ban check.
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
cargo new --quiet --bin "$SCRATCH/banned" >/dev/null 2>&1 || fail "cargo new failed"
printf '\nserialport = "*"\n' >> "$SCRATCH/banned/Cargo.toml"
cp "$REPO_ROOT/deny.toml" "$SCRATCH/banned/deny.toml"

if cargo deny --manifest-path "$SCRATCH/banned/Cargo.toml" check bans 2>/dev/null; then
  fail "ban list did NOT reject 'serialport' — the gate is a no-op"
fi

echo '{"check":"license-gate","clean_pass":true,"banned_rejected":true,"pass":true}'
