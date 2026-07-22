#!/usr/bin/env bash
# Post-1.0 §9.4 validation (design §16.5): a jq-lint pass. Two checks, both cheap
# and self-judging:
#   (1) every `.jq` program in the repo compiles — a syntax error in an expectation
#       file (e.g. expectations/linux.jq) is caught here, not at CI-gate time;
#   (2) no shell script carries the `X // N == M` precedence tautology that made the
#       soak unfalsifiable (§16.5). The bare-number-before-`==` signature matches the
#       bug (`// 0 == 0`) but not the correct parenthesized form (`(... // 0) == 0`),
#       because the `)` sits between the number and the `==`.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$REPO_ROOT"
fail() { echo "{\"check\":\"jq-lint\",\"pass\":false,\"reason\":\"$*\"}"; exit 1; }

command -v jq >/dev/null 2>&1 || fail "jq not installed"

# (1) Compile every .jq file. jq reports a *syntax* error before reading input and
# with a distinct message, so run each program on null input and fail only on a
# compile/syntax error (a runtime "null has no field" is not a lint failure).
jq_files=0
while IFS= read -r f; do
  jq_files=$((jq_files + 1))
  err=$(jq -f "$f" -n </dev/null 2>&1 >/dev/null || true)
  if grep -qiE 'syntax error|compile error|unexpected' <<<"$err"; then
    echo "$err" >&2
    fail "jq program does not compile: $f"
  fi
done < <(find . -name '*.jq' -not -path './target/*')

# (2) Scan shell scripts for the precedence tautology, skipping comment lines and
# any line explicitly marked `jq-lint-allow` (the harness self-test demonstrates the
# bug on purpose).
hits=$(grep -rnE '//[[:space:]]*[0-9]+[[:space:]]*==' scripts/ \
  | grep -vE ':[[:space:]]*#' \
  | grep -v 'jq-lint-allow' || true)
if [[ -n "$hits" ]]; then
  echo "possible jq precedence tautology (use parentheses: (X // N) == M):" >&2
  echo "$hits" >&2
  fail "a shell script carries the // N == precedence tautology"
fi

echo "{\"check\":\"jq-lint\",\"jq_files_compiled\":$jq_files,\"tautology_free\":true,\"pass\":true}"
