#!/usr/bin/env bash
# Product spaces: lifecycle, isolation of identical keys across spaces,
# default-space behavior, spaces × branches, and deletion guards.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] space lifecycle"
run "$DB" space list
check_contains "a fresh database has the default space" "default" "$OUT"
expect_contains "create a space" "created tenant_a applied=true" -- "$DB" space create tenant_a
expect_ok "create a second space" -- "$DB" space create tenant_b
expect_out "space exists is true" "true" -- "$DB" space exists tenant_a
expect_out "space exists is false for unknown names" "false" -- "$DB" space exists ghost
run "$DB" space list
check_contains "list shows tenant_a" "tenant_a" "$OUT"
check_contains "list shows tenant_b" "tenant_b" "$OUT"

echo "[$SUITE_NAME] the same key is isolated per space"
seed "$DB" kv put color red
seed "$DB" kv put color green --space tenant_a
seed "$DB" kv put color blue --space tenant_b
expect_out "default space reads its own value" "red" -- "$DB" kv get color
expect_out "tenant_a reads its own value" "green" -- "$DB" kv get color --space tenant_a
expect_out "tenant_b reads its own value" "blue" -- "$DB" kv get color --space tenant_b
expect_out "keys never leak across spaces" "(nil)" -- "$DB" kv get only-default --space tenant_a
expect_out "count is per-space" "1" -- "$DB" kv count --space tenant_a

echo "[$SUITE_NAME] spaces compose with branches"
seed "$DB" branch fork default topic
expect_out "fork carries every space's data" "green" -- "$DB" kv get color --branch topic --space tenant_a
seed "$DB" kv put color yellow --branch topic --space tenant_a
expect_out "post-fork space write stays on the fork" "yellow" -- "$DB" kv get color --branch topic --space tenant_a
expect_out "the parent's space value is unchanged" "green" -- "$DB" kv get color --space tenant_a

echo "[$SUITE_NAME] global --space flag matches the per-verb flag"
expect_out "global --space flag reads the same row" "green" -- --space tenant_a "$DB" kv get color

echo "[$SUITE_NAME] deletion guards"
expect_fail "deleting a non-empty space is refused" "" -- "$DB" space delete tenant_a
seed "$DB" kv delete color --space tenant_b
expect_ok "deleting an emptied space succeeds" -- "$DB" space delete tenant_b
expect_out "deleted space is gone from exists" "false" -- "$DB" space exists tenant_b
expect_fail "the default space cannot be deleted" "" -- "$DB" space delete default

finish
