#!/usr/bin/env bash
# Durability: every CLI invocation is a separate process, so each read after a
# write is already a reopen. This suite makes that explicit across all
# primitives, exercises a burst of writes, and pins cache-mode volatility.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] all primitives survive reopen"
seed "$DB" kv put durable-kv yes
seed "$DB" json set durable-doc '$' '{"ok":true}'
seed "$DB" vector collection create durable-vec 2
seed "$DB" vector upsert durable-vec a 1,0 --metadata '{"tag":"keep"}'
seed "$DB" event append durable.event '{"n":1}'
seed "$DB" graph create durable-graph
seed "$DB" graph add-node durable-graph n1 --properties '{"p":1}'
seed "$DB" graph add-edge durable-graph n1 self n1
seed "$DB" branch fork default durable-fork
seed "$DB" kv put fork-row yes --branch durable-fork
seed "$DB" space create durable-space
seed "$DB" kv put spaced yes --space durable-space

expect_out "kv row survives" "yes" -- "$DB" kv get durable-kv
expect_out "json document survives" "true" -- "$DB" json get durable-doc '$.ok'
expect_out "vector survives" "true" -- "$DB" vector exists durable-vec a
expect_json "vector metadata survives" '["data"]["data"]["metadata"]["tag"]' "keep" -- "$DB" vector get durable-vec a
expect_out "event log survives" "1" -- "$DB" event len
expect_json "graph node survives" '["data"]["properties"]["p"]' 1 -- "$DB" graph get-node durable-graph n1
expect_ok "graph edge survives" -- "$DB" graph get-edge durable-graph n1 self n1
expect_out "branch row survives" "yes" -- "$DB" kv get fork-row --branch durable-fork
expect_out "space row survives" "yes" -- "$DB" kv get spaced --space durable-space
run --json "$DB" event verify-chain
check_contains "event chain verifies after reopen" '"valid":true' "$OUT"

echo "[$SUITE_NAME] write burst then reopen"
for i in $(seq 1 50); do
  "$STRATA_BIN" "$DB" kv put "burst:$i" "value-$i" >/dev/null || { _fail "burst write $i" "write failed"; break; }
done
expect_out "all burst rows are visible after reopen" "50" -- "$DB" kv count --prefix burst:
expect_out "a middle burst row reads back" "value-25" -- "$DB" kv get burst:25

echo "[$SUITE_NAME] history survives reopen"
seed "$DB" kv put versioned v1
seed "$DB" kv put versioned v2
run --json "$DB" kv history versioned
check_ok "history read succeeds"
check_eq "both versions survive" 2 "$(python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["count"])' <<<"$OUT")"

echo "[$SUITE_NAME] cache mode is per-process volatile"
run --cache kv put ephemeral gone
check_ok "cache-mode write succeeds"
run --cache kv get ephemeral
check_ok "cache-mode read succeeds in a fresh process"
check_eq "a fresh cache process starts empty by design" "(nil)" "$OUT"

echo "[$SUITE_NAME] database targeting guards"
expect_fail "--db and a positional path conflict" "" -- --db "$DB" "$DB" info
expect_fail "--cache and a durable path conflict" "" -- --cache "$DB" info

finish
