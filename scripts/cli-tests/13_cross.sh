#!/usr/bin/env bash
# Cross-primitive permutations: identical names across primitives, the full
# branch × space × primitive divergence matrix, and mixed workloads on forks.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] identical names never collide across primitives"
seed "$DB" kv put shared kv-value
seed "$DB" json set shared '$' '{"kind":"json"}'
seed "$DB" vector collection create shared 2
seed "$DB" vector upsert shared shared 1,0
seed "$DB" graph create shared
seed "$DB" graph add-node shared shared
expect_out "kv row is intact" "kv-value" -- "$DB" kv get shared
expect_out "json document is intact" '"json"' -- "$DB" json get shared '$.kind'
expect_out "vector is intact" "true" -- "$DB" vector exists shared shared
expect_json "graph node is intact" '["data"]["node_id"]' "shared" -- "$DB" graph get-node shared shared
expect_out "kv namespace does not see json/graph rows" "1" -- "$DB" kv count

echo "[$SUITE_NAME] branch × space × primitive divergence matrix"
seed "$DB" space create tenant
seed "$DB" kv put matrix default-default
seed "$DB" kv put matrix default-tenant --space tenant
seed "$DB" json set matrix '$' '"default-default"'
seed "$DB" json set matrix '$' '"default-tenant"' --space tenant
seed "$DB" branch fork default side
seed "$DB" kv put matrix side-default --branch side
seed "$DB" kv put matrix side-tenant --branch side --space tenant
seed "$DB" json set matrix '$' '"side-default"' --branch side
seed "$DB" json set matrix '$' '"side-tenant"' --branch side --space tenant

expect_out "kv (default,default)" "default-default" -- "$DB" kv get matrix
expect_out "kv (default,tenant)" "default-tenant" -- "$DB" kv get matrix --space tenant
expect_out "kv (side,default)" "side-default" -- "$DB" kv get matrix --branch side
expect_out "kv (side,tenant)" "side-tenant" -- "$DB" kv get matrix --branch side --space tenant
expect_out "json (default,default)" '"default-default"' -- "$DB" json get matrix '$'
expect_out "json (default,tenant)" '"default-tenant"' -- "$DB" json get matrix '$' --space tenant
expect_out "json (side,default)" '"side-default"' -- "$DB" json get matrix '$' --branch side
expect_out "json (side,tenant)" '"side-tenant"' -- "$DB" json get matrix '$' --branch side --space tenant

echo "[$SUITE_NAME] a mixed workload diverges cleanly on a fork"
seed "$DB" event append mix.base '{}'
seed "$DB" branch fork default workbench
seed "$DB" kv put mix-kv fork-side --branch workbench
seed "$DB" json set mix-doc '$' '{"on":"fork"}' --branch workbench
seed "$DB" vector upsert shared mix-vec 0,1 --branch workbench
seed "$DB" graph add-node shared mix-node --branch workbench
seed "$DB" event append mix.fork '{}' --branch workbench

expect_out "fork kv write is fork-local" "(nil)" -- "$DB" kv get mix-kv
expect_out "fork json write is fork-local" "false" -- "$DB" json exists mix-doc
expect_out "fork vector write is fork-local" "false" -- "$DB" vector exists shared mix-vec
expect_out "fork graph write is fork-local" "(nil)" -- "$DB" graph get-node shared mix-node
expect_out "fork event append is fork-local" "1" -- "$DB" event len
expect_out "the fork sees all five of its writes" "fork-side" -- "$DB" kv get mix-kv --branch workbench
expect_out "fork json reads back" '"fork"' -- "$DB" json get mix-doc '$.on' --branch workbench
expect_out "fork vector reads back" "true" -- "$DB" vector exists shared mix-vec --branch workbench
expect_out "fork event log counts both" "2" -- "$DB" event len --branch workbench

echo "[$SUITE_NAME] deleting a fork leaves the parent whole"
seed "$DB" branch delete workbench
expect_out "parent kv is whole" "kv-value" -- "$DB" kv get shared
expect_out "parent json is whole" '"json"' -- "$DB" json get shared '$.kind'
expect_out "parent vector is whole" "true" -- "$DB" vector exists shared shared
expect_out "parent event log is whole" "1" -- "$DB" event len

finish
