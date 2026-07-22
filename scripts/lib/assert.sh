#!/usr/bin/env bash
# Shared, self-tested assertion helpers for the validation harness (post-1.0 §9.4,
# design §16.5). Three of the project's audits found bugs in the *tests* — the worst
# a jq-precedence tautology that made the soak unfalsifiable. Centralizing the
# fragile assertions here, with a regression suite in
# `scripts/validate/phase0/harness-selftest.sh`, turns "remember not to write the
# tautology" into a test that fails if anyone does.
#
# Source this file; do not execute it. It defines functions only.

# assert_loss_counters_zero — read a `state` JSON on stdin; exit 0 iff every
# drop/discard/purge counter anywhere in the graph is zero.
#
# The precedence is load-bearing: `(add // 0) == 0`, NEVER `add // 0 == 0`. jq's
# `//` binds looser than `==`, so `add // 0 == 0` parses as `add // (0 == 0)` =
# `add // true` — which yields the counter *sum* (a truthy number) whenever any
# counter is nonzero, so the check passes exactly when it should fail. That bug
# shipped in the soak (phase-8 audit) and is pinned dead by the harness self-test,
# which feeds this a nonzero counter and asserts it fails.
assert_loss_counters_zero() {
  jq -e '([.. | objects | to_entries[]
          | select(.key|test("drop|discard|purge"))
          | .value | numbers] | add // 0) == 0' >/dev/null
}

# loss_counters_nonzero — read a `state` JSON on stdin; print the nonzero
# drop/discard/purge counters as a compact JSON array (a diagnostic for a failed
# assert_loss_counters_zero).
loss_counters_nonzero() {
  jq -c '[.. | objects | to_entries[]
          | select((.key|test("drop|discard|purge")) and (.value|numbers) and .value > 0)]'
}

# assert_eq EXPECTED ACTUAL [MESSAGE] — exit 1 with a diagnostic on mismatch.
assert_eq() {
  if [[ "$1" != "$2" ]]; then
    echo "assert_eq failed: expected [$1] got [$2]${3:+ — $3}" >&2
    return 1
  fi
}

# assert_json — read JSON on stdin; exit 0 iff the jq filter in $1 is truthy.
# `jq -e` exits non-zero on false/null, so this is a clean boolean assertion.
assert_json() {
  jq -e "$1" >/dev/null
}
