#!/usr/bin/env bash
# Error contract: structured codes and classes on the wire (never display
# text), usage guards, exit codes, and reference ids.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

seed "$DB" kv put seeded yes

echo "[$SUITE_NAME] structured error codes"
expect_error_code "unknown branch reads fail with a branch code" \
  "not_found.engine.branch" -- "$DB" kv get seeded --branch ghost
expect_error_code "missing vector collection has a stable code" \
  "not_found.engine.vector_collection" -- "$DB" vector upsert ghost k 1,2
expect_error_code "unknown graph has a stable code" \
  "not_found.engine.graph" -- "$DB" graph add-node ghost n1

echo "[$SUITE_NAME] error envelope facts"
run --json "$DB" kv get seeded --branch ghost
if printf '%s' "$ERR" | python3 -c '
import json, sys
e = json.load(sys.stdin)["error"]
assert e["class"] == "not_found", e["class"]
assert e["code"].startswith("not_found."), e["code"]
assert e["reference_id"], "missing reference id"
assert e["retry_policy"], "missing retry policy"
assert e["docs_url"].startswith("https://"), "missing docs url"
assert e["suggested_fix"], "missing suggested fix"
'; then _ok; else _fail "envelope carries class/code/reference/retry/docs/fix" "stderr: $ERR"; fi

echo "[$SUITE_NAME] input guards"
expect_fail "empty kv key is rejected" "" -- "$DB" kv put '' v
# The CLI documents relaxed JSON: non-JSON text is stored as a string.
seed "$DB" json set relaxed '$' '{"unclosed":'
expect_out "malformed JSON is stored as a string (documented relaxed mode)" \
  '"{\"unclosed\":"' -- "$DB" json get relaxed '$'
expect_fail "unknown vector metric is rejected" "" -- "$DB" vector collection create c 2 --metric bogus
expect_fail "non-numeric as-of is a usage error" "" -- "$DB" kv get seeded --as-of not-a-number
expect_fail "value and --file together are rejected" "cannot be used with" \
  -- "$DB" kv put k v --file /dev/null
expect_fail "missing value is a usage error" "missing required value" -- "$DB" kv put k

echo "[$SUITE_NAME] exit codes"
run "$DB" kv get seeded
check_eq "success exits 0" 0 "$STATUS"
run "$DB" kv get missing-key
check_eq "a miss is still a successful read (exit 0)" 0 "$STATUS"
run "$DB" kv get seeded --branch ghost
if [[ $STATUS -ne 0 ]]; then _ok; else _fail "executor errors exit non-zero" "exit=0"; fi
run "$DB" definitely-not-a-command
if [[ $STATUS -ne 0 ]]; then _ok; else _fail "unknown commands exit non-zero" "exit=0"; fi

echo "[$SUITE_NAME] errors never write"
run --json "$DB" kv put '' nope
expect_out "a failed write leaves no partial state" "1" -- "$DB" kv count

finish
