#!/usr/bin/env bash
# Time travel: --as-of reads across KV, JSON, graph, event, and vector — pinned
# to real commit timestamps captured from write receipts.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] KV as-of"
run --json "$DB" kv put city paris
check_ok "first write succeeds"
t1="$(commit_timestamp)"
run --json "$DB" kv put city london
check_ok "second write succeeds"
t2="$(commit_timestamp)"
expect_out "as-of the first commit reads the first value" "paris" -- "$DB" kv get city --as-of "$t1"
expect_out "as-of the second commit reads the second value" "london" -- "$DB" kv get city --as-of "$t2"
expect_out "a current read gets the latest value" "london" -- "$DB" kv get city
run --json "$DB" kv put newcomer hello
t3="$(commit_timestamp)"
expect_out "keys created later are invisible as-of t1" "(nil)" -- "$DB" kv get newcomer --as-of "$t1"
expect_out "as-of list excludes later keys" "city" -- "$DB" kv list --as-of "$t1"

echo "[$SUITE_NAME] JSON as-of"
run --json "$DB" json set doc '$' '{"rev":"one"}'
check_ok "document write succeeds"
j1="$(commit_timestamp)"
seed "$DB" json set doc '$.rev' '"two"'
expect_out "as-of reads the historical path value" '"one"' -- "$DB" json get doc '$.rev' --as-of "$j1"
expect_out "a current read gets the latest path value" '"two"' -- "$DB" json get doc '$.rev'

echo "[$SUITE_NAME] graph as-of"
seed "$DB" graph create net
run --json "$DB" graph add-node net n1
check_ok "first node write succeeds"
g1="$(commit_timestamp)"
seed "$DB" graph add-node net n2
seed "$DB" graph add-edge net n1 links n2
expect_json "as-of node read sees the existing node" '["data"]["node_id"]' "n1" -- "$DB" graph get-node net n1 --as-of "$g1"
expect_out "nodes added later are invisible as-of g1" "(nil)" -- "$DB" graph get-node net n2 --as-of "$g1"
expect_out "edges added later are invisible as-of g1" "(nil)" -- "$DB" graph get-edge net n1 links n2 --as-of "$g1"
run --json "$DB" graph neighbors net n1 --as-of "$g1"
check_ok "as-of neighbors succeed"
check_eq "as-of traversal sees no later edges" 0 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"
run --json "$DB" graph list-nodes net --as-of "$g1"
check_eq "as-of node listing excludes later nodes" 1 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] event as-of"
run --json "$DB" event append tick '{"n":1}'
check_ok "first event append succeeds"
e1="$(commit_timestamp)"
seed "$DB" event append tick '{"n":2}'
expect_out "event as-of uses the commit-timestamp domain like every other primitive" \
  "1" -- "$DB" event len --as-of "$e1"
expect_out "a current len sees everything" "2" -- "$DB" event len
expect_json "as-of point reads see committed events" '["data"]["event"]["payload"]["n"]' 1 \
  -- "$DB" event get 0 --as-of "$e1"
expect_out "later sequences are invisible as-of e1" "(nil)" -- "$DB" event get 1 --as-of "$e1"
expect_out "as-of type listing excludes later types" "tick" -- "$DB" event types --as-of "$e1"
expect_error_code "an after-latest as-of is a diagnostic, not a clamp" \
  "history_unavailable.engine.persistence_history" -- "$DB" event len --as-of 9999999999999999

echo "[$SUITE_NAME] vector as-of"
seed "$DB" vector collection create emb 2
run --json "$DB" vector upsert emb a 1,0
check_ok "first vector write succeeds"
v1="$(commit_timestamp)"
seed "$DB" vector upsert emb b 0,1
run "$DB" vector query emb 0,1 -k 10 --as-of "$v1"
check_ok "as-of query succeeds"
if [[ "$OUT" == *b* ]]; then
  _fail "vectors added later are invisible as-of v1" "b leaked into: $OUT"
else
  _ok
fi

echo "[$SUITE_NAME] as-of composes with branches"
seed "$DB" branch fork default tt-fork
seed "$DB" kv put city tokyo --branch tt-fork
expect_out "fork as-of a pre-fork commit reads pre-fork state" "paris" -- "$DB" kv get city --branch tt-fork --as-of "$t1"
expect_out "fork current read gets the fork's write" "tokyo" -- "$DB" kv get city --branch tt-fork

finish
