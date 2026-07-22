#!/usr/bin/env bash
# Post-1.0 §9.3 validation (design §16.3): prove `unsafe` is confined to the one
# `nexus-sys` crate. Every other workspace crate `#![forbid(unsafe_code)]`, so a
# stray `unsafe` anywhere else is a hard compile error — but this gate additionally
# catches an `unsafe` that a future author might try to unlock with a localized
# `#[allow]`, keeping the audit surface for unsafe code a single file set.
#
# Like the license gate, the detector is proven, not assumed: a planted `unsafe`
# in a scratch file must be caught by the same pattern.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fail() { echo "FAIL: $*" >&2; echo '{"check":"unsafe-gate","pass":false}'; exit 1; }

# A real `unsafe` usage: the keyword introducing a block/fn/impl/trait/extern. The
# `\bunsafe\b` word boundary excludes `unsafe_code` (as in `#![forbid(unsafe_code)]`),
# so the attribute never trips the gate.
PAT='\bunsafe\b[[:space:]]*(\{|fn|impl|trait|extern)'

# 1. Prove the detector actually catches an `unsafe` (not a silent no-op).
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
printf 'fn planted() { unsafe { let _ = 1; } }\n' > "$SCRATCH/planted.rs"
grep -qE "$PAT" "$SCRATCH/planted.rs" || fail "the detector does not catch a planted unsafe"

# 2. No workspace crate other than nexus-sys may contain an `unsafe` usage. Scan
#    every tracked .rs outside nexus-sys/, target/, and the excluded fuzz crate.
hits="$(cd "$REPO_ROOT" && grep -rnE "$PAT" --include='*.rs' . \
  | grep -v '/target/' \
  | grep -v '/nexus-sys/' \
  | grep -v '/fuzz/' || true)"
if [[ -n "$hits" ]]; then
  echo "unsafe found outside nexus-sys/:" >&2
  echo "$hits" >&2
  fail "unsafe is not confined to nexus-sys/"
fi

# 3. Sanity: nexus-sys genuinely does carry the unsafe (else the split is a lie).
grep -qE "$PAT" "$REPO_ROOT/nexus-sys/src/lib.rs" \
  || fail "nexus-sys carries no unsafe — the extraction target is wrong"

echo '{"check":"unsafe-gate","detector_proven":true,"confined_to_nexus_sys":true,"pass":true}'
