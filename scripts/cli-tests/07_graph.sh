#!/usr/bin/env bash
# Graph primitive: graph/node/edge lifecycle, properties, weighted edges,
# neighbor traversal by direction, and branch isolation.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] graph lifecycle"
expect_ok "create a graph" -- "$DB" graph create deps
expect_ok "create a second graph" -- "$DB" graph create social
run "$DB" graph list
check_ok "graph list succeeds"
check_contains "list shows deps" "deps" "$OUT"
check_contains "list shows social" "social" "$OUT"
expect_fail "duplicate graph creation fails" "" -- "$DB" graph create deps
run --json "$DB" graph meta deps
check_ok "graph meta succeeds"

echo "[$SUITE_NAME] node lifecycle"
expect_ok "add a node with properties" -- "$DB" graph add-node deps core --properties '{"lang":"rust"}'
expect_ok "add a bare node" -- "$DB" graph add-node deps cli
expect_ok "add a third node" -- "$DB" graph add-node deps sdk
expect_json "get-node returns properties" '["data"]["properties"]["lang"]' "rust" -- "$DB" graph get-node deps core
expect_out "get-node of a missing id prints (nil)" "(nil)" -- "$DB" graph get-node deps ghost
run --json "$DB" graph list-nodes deps
check_ok "list-nodes succeeds"
check_eq "list-nodes returns every node" 3 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] edge lifecycle"
expect_ok "add a weighted edge" -- "$DB" graph add-edge deps cli depends_on core --weight 2.5
expect_ok "add a second edge" -- "$DB" graph add-edge deps sdk depends_on core
expect_ok "add an edge with properties" -- "$DB" graph add-edge deps core exposes sdk --properties '{"since":"v1"}'
expect_json "get-edge returns the weight" '["data"]["weight"]' 2.5 -- "$DB" graph get-edge deps cli depends_on core
expect_json "get-edge returns properties" '["data"]["properties"]["since"]' "v1" -- "$DB" graph get-edge deps core exposes sdk
expect_out "get-edge of a missing edge prints (nil)" "(nil)" -- "$DB" graph get-edge deps cli exposes core

echo "[$SUITE_NAME] neighbor traversal"
run --json "$DB" graph neighbors deps core --direction incoming
check_ok "incoming neighbors succeed"
check_eq "core has two incoming dependents" 2 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"
run --json "$DB" graph neighbors deps core --direction outgoing
check_eq "core has one outgoing edge" 1 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"
run --json "$DB" graph neighbors deps core --direction both
check_eq "both directions combine" 3 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] removal"
expect_ok "remove an edge" -- "$DB" graph remove-edge deps core exposes sdk
expect_out "removed edge is gone" "(nil)" -- "$DB" graph get-edge deps core exposes sdk
expect_ok "remove a node" -- "$DB" graph remove-node deps sdk
expect_out "removed node is gone" "(nil)" -- "$DB" graph get-node deps sdk
run --json "$DB" graph neighbors deps core --direction incoming
check_eq "edges from a removed node disappear" 1 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] graphs are branch-isolated"
seed "$DB" branch fork default graph-fork
seed "$DB" graph add-node deps fork-node --branch graph-fork
expect_json "fork sees its own node" '["data"]["node_id"]' "fork-node" -- "$DB" graph get-node deps fork-node --branch graph-fork
expect_out "parent does not see the fork's node" "(nil)" -- "$DB" graph get-node deps fork-node
seed "$DB" graph create parent-only
run "$DB" graph list --branch graph-fork
if [[ "$OUT" == *"parent-only"* ]]; then
  _fail "post-fork graphs stay off the fork" "parent-only leaked into: $OUT"
else
  _ok
fi

echo "[$SUITE_NAME] graph deletion"
expect_ok "delete a graph" -- "$DB" graph delete social
run "$DB" graph list
if [[ "$OUT" == *"social"* ]]; then
  _fail "deleted graph no longer listed" "social still present in: $OUT"
else
  _ok
fi
expect_fail "operations on a deleted graph fail" "" -- "$DB" graph add-node social n1

finish
