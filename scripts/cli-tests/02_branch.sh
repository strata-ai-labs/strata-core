#!/usr/bin/env bash
# Branch semantics: fork isolation (set, branch, read on both sides), write
# divergence, empty root branches, fork-at-version/timestamp time anchoring,
# branch lifecycle, and cross-primitive visibility on branches.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] the core scenario: set, fork, read on both branches"
seed "$DB" kv put city paris
expect_ok "fork default -> feature" -- "$DB" branch fork default feature
expect_out "parent still reads the value" "paris" -- "$DB" kv get city
expect_out "fork sees the pre-fork value" "paris" -- "$DB" kv get city --branch feature

echo "[$SUITE_NAME] post-fork writes diverge"
seed "$DB" kv put city london                       # parent moves on
seed "$DB" kv put city tokyo --branch feature       # fork moves elsewhere
expect_out "parent reads its own write" "london" -- "$DB" kv get city
expect_out "fork reads its own write" "tokyo" -- "$DB" kv get city --branch feature
seed "$DB" kv put parent-only yes
expect_out "post-fork parent keys are invisible on the fork" "(nil)" -- "$DB" kv get parent-only --branch feature
seed "$DB" kv put fork-only yes --branch feature
expect_out "fork keys are invisible on the parent" "(nil)" -- "$DB" kv get fork-only

echo "[$SUITE_NAME] fork of a fork"
expect_ok "fork feature -> feature-child" -- "$DB" branch fork feature feature-child
# Regression from the read-path perf campaign: a fork of a fork silently
# loses the middle branch's inherited state (reads (nil) instead of the
# fork's value). Same family as the timeline-index pin in 08_time_travel —
# the fork-COW plan's deferred "inherited-source re-clamping". Unlike that
# pin this one is silent wrong data, not a fail-closed error. Issue #2521.
expect_known_bug "grandchild inherits the fork's state" "tokyo" -- "$DB" kv get city --branch feature-child
seed "$DB" kv put city berlin --branch feature-child
expect_out "grandchild write stays on the grandchild" "berlin" -- "$DB" kv get city --branch feature-child
expect_out "middle branch is untouched" "tokyo" -- "$DB" kv get city --branch feature

echo "[$SUITE_NAME] empty root branch"
expect_ok "create an empty root branch" -- "$DB" branch create scratch
expect_out "root branch starts empty" "(nil)" -- "$DB" kv get city --branch scratch
expect_out "root branch has no keys at all" "(empty)" -- "$DB" kv list --branch scratch
seed "$DB" kv put city madrid --branch scratch
expect_out "root branch takes independent writes" "madrid" -- "$DB" kv get city --branch scratch
expect_out "default branch is unaffected by the root branch" "london" -- "$DB" kv get city

echo "[$SUITE_NAME] fork at a historical version"
run --json "$DB" kv put epoch v1
v1_version="$(json_field '["data"]["commit"]["version"]')"
v1_ts="$(commit_timestamp)"
seed "$DB" kv put epoch v2
expect_ok "fork at the v1 commit version" -- "$DB" branch fork default at-version --version "$v1_version"
expect_out "version-anchored fork reads the historical value" "v1" -- "$DB" kv get epoch --branch at-version
expect_ok "fork at the v1 commit timestamp" -- "$DB" branch fork default at-time --timestamp "$v1_ts"
expect_out "timestamp-anchored fork reads the historical value" "v1" -- "$DB" kv get epoch --branch at-time
expect_out "the source branch still reads the latest value" "v2" -- "$DB" kv get epoch

echo "[$SUITE_NAME] branch listing and metadata"
run "$DB" branch list
check_contains "list shows default" "default" "$OUT"
check_contains "list shows the fork" "feature" "$OUT"
check_contains "list shows the root branch" "scratch" "$OUT"
expect_json "branch get reports active status" '["data"]["status"]' "active" -- "$DB" branch get feature
expect_json "branch get reports the parent" '["data"]["parent"]["name"]' "default" -- "$DB" branch get feature

echo "[$SUITE_NAME] branch lifecycle and errors"
expect_ok "delete a branch" -- "$DB" branch delete feature-child
run "$DB" branch list
if [[ "$OUT" == *"feature-child"* ]]; then
  _fail "deleted branch no longer listed" "feature-child still present in: $OUT"
else
  _ok
fi
expect_fail "reads on a deleted branch fail" "" -- "$DB" kv get city --branch feature-child
expect_fail "forking an unknown source fails" "" -- "$DB" branch fork ghost new-branch
expect_fail "duplicate branch names are rejected" "" -- "$DB" branch fork default feature

finish
