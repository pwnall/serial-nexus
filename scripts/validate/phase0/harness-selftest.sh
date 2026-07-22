#!/usr/bin/env bash
# Post-1.0 §9.4 validation (design §16.5): the validation harness's shared
# assertion helpers are themselves tested, so a regression in a helper (the soak's
# jq-precedence tautology being the motivating example) fails this suite rather than
# silently shipping. Self-judging like every validation script.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=scripts/lib/assert.sh
source "$REPO_ROOT/scripts/lib/assert.sh"

fails=0
note() { echo "  ok: $*" >&2; }
bad() { echo "FAIL: $*" >&2; fails=$((fails + 1)); }

# A `state`-shaped fixture with nested nodes, so the recursive `..` descent in the
# loss-counter helper is exercised the way the real state JSON is shaped.
zero_state='{"nodes":[{"name":"usb0","dropped_full":0,"lock":{"purged":0}},
                      {"name":"cap","discarded_absent":0,"channels":{"c1":{"discarded_hostward":0}}}]}'
grew_state='{"nodes":[{"name":"usb0","dropped_full":0,"lock":{"purged":0}},
                      {"name":"cap","discarded_absent":0,"channels":{"c1":{"discarded_hostward":4096}}}]}'

# 1. assert_loss_counters_zero passes on an all-zero graph.
if echo "$zero_state" | assert_loss_counters_zero; then
  note "loss-counters-zero passes when all counters are zero"
else
  bad "loss-counters-zero should pass on an all-zero graph"
fi

# 2. THE anti-tautology regression: it must FAIL when a counter has grown. If the
#    helper ever regresses to `add // 0 == 0` (= `add // true`), this nonzero input
#    would wrongly pass and this check catches it.
if echo "$grew_state" | assert_loss_counters_zero; then
  bad "loss-counters-zero passed on a nonzero counter — the tautology is back"
else
  note "loss-counters-zero fails on a grown counter (falsifiable, not a tautology)"
fi

# 3. Demonstrate the bug the helper avoids, so the reason for the parentheses is
#    pinned in an executable form: the tautological expression *does* wrongly accept
#    the same nonzero graph. (If jq ever changed `//` precedence so this no longer
#    held, the helper's contract note would need revisiting — hence asserting it.)
if echo "$grew_state" \
  | jq -e '([.. | objects | to_entries[] | select(.key|test("drop|discard|purge")) | .value | numbers] | add // 0 == 0)' >/dev/null; then # jq-lint-allow: proves the tautology
  note "the tautological form wrongly accepts a grown counter (why the parens matter)"
else
  bad "the tautological form unexpectedly rejected — the demonstration is stale"
fi

# 4. loss_counters_nonzero surfaces the offender for diagnostics.
offenders=$(echo "$grew_state" | loss_counters_nonzero)
if echo "$offenders" | jq -e 'length == 1 and .[0].value == 4096' >/dev/null; then
  note "loss_counters_nonzero names the grown counter"
else
  bad "loss_counters_nonzero did not surface the offender (got: $offenders)"
fi

# 5. assert_eq and assert_json behave.
if assert_eq foo foo; then note "assert_eq accepts equal"; else bad "assert_eq rejected equal"; fi
if assert_eq foo bar 2>/dev/null; then bad "assert_eq accepted unequal"; else note "assert_eq rejects unequal"; fi
if echo '{"pass":true}' | assert_json '.pass == true'; then note "assert_json accepts truthy"; else bad "assert_json rejected truthy"; fi
if echo '{"pass":false}' | assert_json '.pass == true' 2>/dev/null; then bad "assert_json accepted falsy"; else note "assert_json rejects falsy"; fi

if [[ "$fails" -eq 0 ]]; then
  echo '{"check":"harness-selftest","pass":true}'
else
  echo "{\"check\":\"harness-selftest\",\"pass\":false,\"failures\":$fails}"
  exit 1
fi
