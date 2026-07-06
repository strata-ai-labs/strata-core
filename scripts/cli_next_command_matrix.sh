#!/usr/bin/env bash
set -euo pipefail

# End-to-end CLI command matrix for the handwritten V1 Strata CLI.
#
# Permutations covered:
# - no-database local command: init;
# - admin/config command families;
# - output modes: JSON and raw;
# - raw executor escape hatch: command print/run and representative batch commands;
# - branch lifecycle: list/get/create/fork/delete, fork-at-version, fork-at-timestamp;
# - branch isolation: fork, update child, verify parent remains unchanged;
# - space lifecycle and isolation: create/exists/list/delete, same key in different spaces;
# - KV: put/get/list/scan/exists/history/count/sample/delete, alias, file input;
# - JSON: set/get/list/exists/history/count/sample/index create/list/drop/delete, file input;
# - Vector: collection create/list/stats/delete, upsert/get/history/exists/keys/query,
#   query diagnostics, metadata update, delete, delete-by-filter, delete-all, file inputs;
# - Event: append/get/exists/len/list/types/by-type/range/range-time/verify-chain, file input;
# - Graph: create/list/meta/add-node/get-node/list-nodes/add-edge/get-edge/neighbors,
#   remove-edge/remove-node/delete, alias, property file inputs;
# - Arrow: export and import for KV;
# - pipe mode: multiple commands in one process;
# - deferred old commands: recognized and rejected with intentional errors.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for JSON assertions" >&2
  exit 2
fi

section() {
  printf '\n== %s ==\n' "$1"
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  local actual="$1"
  local expected="$2"
  local label="$3"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label: expected [$expected], got [$actual]"
  fi
}

assert_file_nonempty() {
  local path="$1"
  [[ -s "$path" ]] || fail "expected non-empty file: $path"
}

assert_json() {
  local payload="$1"
  local expression="$2"
  local label="$3"
  JSON_PAYLOAD="$payload" python3 - "$expression" "$label" <<'PY'
import json
import os
import sys

payload = os.environ["JSON_PAYLOAD"]
expression = sys.argv[1]
label = sys.argv[2]
try:
    data = json.loads(payload)
except Exception as exc:
    print(f"{label}: output is not valid JSON: {exc}\n{payload}", file=sys.stderr)
    sys.exit(1)

scope = {"data": data, "bytes": bytes, "len": len, "any": any, "all": all, "str": str}
try:
    ok = bool(eval(expression, {"__builtins__": {}}, scope))
except Exception as exc:
    print(f"{label}: assertion raised {exc}\nexpr: {expression}\njson: {json.dumps(data, indent=2)}", file=sys.stderr)
    sys.exit(1)
if not ok:
    print(f"{label}: assertion failed\nexpr: {expression}\njson: {json.dumps(data, indent=2)}", file=sys.stderr)
    sys.exit(1)
PY
}

json_value() {
  local payload="$1"
  local expression="$2"
  JSON_PAYLOAD="$payload" python3 - "$expression" <<'PY'
import json
import os
import sys

data = json.loads(os.environ["JSON_PAYLOAD"])
value = eval(sys.argv[1], {"__builtins__": {}}, {"data": data, "bytes": bytes, "len": len})
if isinstance(value, bytes):
    print(value.decode("utf-8"))
else:
    print(value)
PY
}

write_json() {
  local path="$1"
  local json="$2"
  printf '%s\n' "$json" > "$path"
}

section "Build CLI"
cargo build -q -p strata-cli-next --bin strata
STRATA="$ROOT/target/debug/strata"

TMP="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT

DB="$TMP/admin-db"
FILES="$TMP/files"
mkdir -p "$FILES"
export STRATA_HOME="$TMP/home"

cli_json() {
  "$STRATA" --db "$DB" --json "$@"
}

cli_raw() {
  "$STRATA" --db "$DB" --raw "$@"
}

cli_json_branch() {
  local branch="$1"
  shift
  "$STRATA" --db "$DB" --json --branch "$branch" "$@"
}

cli_json_space() {
  local space="$1"
  shift
  "$STRATA" --db "$DB" --json --space "$space" "$@"
}

cli_json_branch_space() {
  local branch="$1"
  local space="$2"
  shift 2
  "$STRATA" --db "$DB" --json --branch "$branch" --space "$space" "$@"
}

raw_command_file() {
  local path="$1"
  shift
  "$STRATA" --db "$DB" --json command run --file "$path" "$@"
}

section "Init, admin, config, output modes"
out="$("$STRATA" --json init)"
assert_json "$out" 'data["type"] == "init" and data["data"]["database_created"] is False' "init"
[[ -d "$STRATA_HOME" ]] || fail "init did not create STRATA_HOME"

out="$(cli_json ping)"
assert_json "$out" 'data["type"] == "pong"' "ping"

out="$(cli_json info)"
assert_json "$out" 'data["type"] == "database_info"' "info"

out="$(cli_json health)"
assert_json "$out" 'data["type"] == "health"' "health"

out="$(cli_json metrics)"
assert_json "$out" 'data["type"] == "metrics"' "metrics"

out="$(cli_json describe)"
assert_json "$out" 'data["type"] == "described"' "describe"

out="$(cli_json config get)"
assert_json "$out" 'data["type"] == "config"' "config get"

out="$(cli_json config get-key runtime.profile)"
assert_json "$out" 'data["type"] == "config_value"' "config get-key"

out="$("$STRATA" --db "$DB" --json ping)"
assert_json "$out" 'data["type"] == "pong"' "--json output mode"

section "Raw command escape hatch and raw output"
ping_cmd="$FILES/ping-command.json"
write_json "$ping_cmd" '{"type":"ping"}'
out="$("$STRATA" --json command print --file "$ping_cmd")"
assert_json "$out" 'data["type"] == "ping"' "command print"

out="$(raw_command_file "$ping_cmd")"
assert_json "$out" 'data["type"] == "pong"' "command run"

out="$(cli_raw kv put raw-mode raw-value && cli_raw kv get raw-mode)"
assert_eq "$out" "raw-value" "raw kv get"

section "Branch lifecycle and isolation"
DB="$TMP/branch-db"
out="$(cli_json kv put branch-key main-v1)"
assert_json "$out" 'data["type"] == "write_result"' "kv put before branch fork"
main_v1="$(json_value "$out" 'data["data"]["commit"]["version"]')"
main_ts="$(json_value "$out" 'data["data"]["commit"]["timestamp"]')"

out="$(cli_json branch list)"
assert_json "$out" 'data["type"] == "branches" and any(item["name"] == "default" for item in data["data"]["items"])' "branch list default"

out="$(cli_json branch get default)"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "default"' "branch get default"

out="$(cli_json branch create isolated-root)"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "isolated-root"' "branch create"

out="$(cli_json_branch isolated-root kv get branch-key)"
assert_json "$out" 'data["type"] == "kv_versioned_value" and data["data"] is None' "root branch has no inherited kv"

out="$(cli_json branch fork default feature)"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "feature"' "branch fork current"

out="$(cli_json_branch feature kv get branch-key)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "main-v1"' "fork inherited value"

cli_json_branch feature kv put branch-key feature-v2 >/dev/null
out="$(cli_json_branch feature kv get branch-key)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "feature-v2"' "feature updated value"

out="$(cli_json kv get branch-key)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "main-v1"' "default branch unchanged"

out="$(cli_json branch fork default by-version --version "$main_v1")"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "by-version"' "branch fork at version"

out="$(cli_json_branch by-version kv get branch-key)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "main-v1"' "version fork reads retained value"

out="$(cli_json branch fork default by-timestamp --timestamp "$main_ts")"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "by-timestamp"' "branch fork at timestamp"

out="$(cli_json branch del isolated-root)"
assert_json "$out" 'data["type"] == "branch_delete_result"' "branch delete alias"

out="$(cli_json space create after-branch-delete)"
assert_json "$out" 'data["type"] == "space_create_result" and data["data"]["space"] == "after-branch-delete"' "space create after branch delete"
cli_json_space after-branch-delete kv put branch-delete-space-value ok >/dev/null
out="$(cli_json space del after-branch-delete --force)"
assert_json "$out" 'data["type"] == "space_delete_result" and data["data"]["space"] == "after-branch-delete" and data["data"]["deleted"] is True' "space force delete after branch delete"

for deferred in "branch diff default feature" "branch merge feature default" "search" "recipe" "txn" "begin" "commit" "rollback" "flush" "compact" "up" "down" "uninstall"; do
  if "$STRATA" --db "$DB" $deferred >/tmp/strata-cli-deferred.out 2>/tmp/strata-cli-deferred.err; then
    fail "deferred command unexpectedly succeeded: $deferred"
  fi
done

section "Space lifecycle and isolation"
DB="$TMP/space-db"
out="$(cli_json space list)"
assert_json "$out" 'data["type"] == "space_list" and "default" in data["data"]["items"]' "space list default"

out="$(cli_json space create docs)"
assert_json "$out" 'data["type"] == "space_create_result" and data["data"]["space"] == "docs"' "space create"

out="$(cli_json space exists docs)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "space exists"

cli_json space create scratch >/dev/null
out="$(cli_json space del scratch)"
assert_json "$out" 'data["type"] == "space_delete_result" and data["data"]["space"] == "scratch"' "space delete alias"

cli_json kv put shared-space default-value >/dev/null
cli_json_space docs kv put shared-space docs-value >/dev/null

out="$(cli_json kv get shared-space)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "default-value"' "default space value"

out="$(cli_json_space docs kv get shared-space)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "docs-value"' "docs space value"

section "KV commands"
DB="$TMP/kv-db"
printf 'file-value' > "$FILES/kv-file.txt"
printf 'at-file-value' > "$FILES/kv-at-file.txt"
cli_json kv put kv-a a1 >/dev/null
cli_json kv put kv-b --file "$FILES/kv-file.txt" >/dev/null
cli_json kv put kv-c "@$FILES/kv-at-file.txt" >/dev/null

out="$(cli_json kv get kv-b)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "file-value"' "kv get file value"

out="$(cli_json kv get kv-c)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "at-file-value"' "kv get @file value"

out="$(cli_json kv exists kv-a)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "kv exists"

out="$(cli_json kv list --prefix kv- --limit 10)"
assert_json "$out" 'data["type"] in ("keys", "keys_page") and len(data["data"]["items"]) >= 3' "kv list"

out="$(cli_json kv scan --start kv-a --limit 2)"
assert_json "$out" 'data["type"] == "kv_scan_result" and len(data["data"]["items"]) >= 1' "kv scan"

out="$(cli_json kv count --prefix kv-)"
assert_json "$out" 'data["type"] == "uint" and data["data"] >= 3' "kv count"

out="$(cli_json kv sample --prefix kv- --count 2)"
assert_json "$out" 'data["type"] == "sample_result" and len(data["data"]["items"]) >= 1' "kv sample"

out="$(cli_json kv history kv-a)"
assert_json "$out" 'data["type"] == "version_history" and data["data"] is not None' "kv history"

cli_json kv del kv-a >/dev/null
out="$(cli_json kv get kv-a)"
assert_json "$out" 'data["type"] == "kv_versioned_value" and data["data"] is None' "kv delete alias"

kv_batch="$FILES/kv-batch.json"
write_json "$kv_batch" '{"type":"kv_batch_put","entries":[{"key":[114,97,119,45,107,49],"value":[114,97,119,45,118,49]},{"key":[114,97,119,45,107,50],"value":[114,97,119,45,118,50]}]}'
out="$(raw_command_file "$kv_batch")"
assert_json "$out" 'data["type"] == "batch_results" and data["data"]["status"] == "ok"' "kv batch put raw"

kv_batch_get="$FILES/kv-batch-get.json"
write_json "$kv_batch_get" '{"type":"kv_batch_get","keys":[[114,97,119,45,107,49],[114,97,119,45,107,50]]}'
out="$(raw_command_file "$kv_batch_get")"
assert_json "$out" 'data["type"] == "batch_get_results" and len(data["data"]["items"]) == 2' "kv batch get raw"

section "JSON commands"
DB="$TMP/json-db"
json_file="$FILES/json-doc.json"
write_json "$json_file" '{"name":"File","rank":2}'
cli_json json set doc-a '$' '{"name":"Ada","rank":1}' >/dev/null
cli_json json set doc-file '$' --file "$json_file" >/dev/null

out="$(cli_json json get doc-a '$')"
assert_json "$out" 'data["type"] == "json_versioned_value" and data["data"]["found"] is True and data["data"]["value"]["value"]["name"] == "Ada"' "json get"

out="$(cli_json json exists doc-a)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "json exists"

out="$(cli_json json list --prefix doc --limit 10)"
assert_json "$out" 'data["type"] == "json_list_result" and len(data["data"]["items"]) >= 2' "json list"

out="$(cli_json json count --prefix doc)"
assert_json "$out" 'data["type"] == "uint" and data["data"] >= 2' "json count"

out="$(cli_json json sample --prefix doc --count 1)"
assert_json "$out" 'data["type"] == "json_sample_result" and len(data["data"]["items"]) >= 1' "json sample"

out="$(cli_json json history doc-a)"
assert_json "$out" 'data["type"] == "json_version_history" and data["data"] is not None' "json history"

out="$(cli_json json index create by-rank '$.rank' --index-type numeric)"
assert_json "$out" 'data["type"] == "json_index_definition"' "json index create"

out="$(cli_json json index list)"
assert_json "$out" 'data["type"] == "json_index_list" and any(item["name"] == "by-rank" for item in data["data"]["items"])' "json index list"

out="$(cli_json json index drop by-rank)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "json index drop"

cli_json json del doc-file '$' >/dev/null
out="$(cli_json json get doc-file '$')"
assert_json "$out" 'data["type"] == "json_versioned_value" and data["data"]["found"] is False' "json delete alias"

json_batch="$FILES/json-batch.json"
write_json "$json_batch" '{"type":"json_batch_set","entries":[{"key":"jbatch1","path":"$","value":{"ok":1}},{"key":"jbatch2","path":"$","value":{"ok":2}}]}'
out="$(raw_command_file "$json_batch")"
assert_json "$out" 'data["type"] == "json_batch_results" and data["data"]["status"] == "ok"' "json batch set raw"

section "Vector commands"
DB="$TMP/vector-db"
vector_file="$FILES/vector.txt"
metadata_file="$FILES/vector-metadata.json"
patch_file="$FILES/vector-patch.json"
filter_remove="$FILES/vector-filter-remove.json"
printf '[0.0,1.0]' > "$vector_file"
write_json "$metadata_file" '{"tag":"keep","source":"file"}'
write_json "$patch_file" '{"tag":"updated","source":"patch"}'
write_json "$filter_remove" '{"conditions":[{"field":"tag","op":"eq","value":{"type":"string","value":"remove"}}]}'

out="$(cli_json vector collection create vecs 2 --metric euclidean)"
assert_json "$out" 'data["type"] == "vector_collection_list"' "vector collection create"

out="$(cli_json vector collection list)"
assert_json "$out" 'data["type"] == "vector_collection_list" and any(item["name"] == "vecs" for item in data["data"]["items"])' "vector collection list"

out="$(cli_json vector collection stats vecs)"
assert_json "$out" 'data["type"] == "vector_collection_list" and len(data["data"]["items"]) == 1 and data["data"]["items"][0]["name"] == "vecs"' "vector collection stats"

cli_json vector upsert vecs va '1.0,0.0' --metadata '{"tag":"keep","source":"inline"}' >/dev/null
cli_json vector upsert vecs vb --file "$vector_file" --metadata-file "$metadata_file" >/dev/null
cli_json vector upsert vecs vr '0.5,0.5' --metadata '{"tag":"remove"}' >/dev/null

out="$(cli_json vector get vecs vb)"
assert_json "$out" 'data["type"] == "vector_data" and data["data"]["key"] == "vb" and data["data"]["data"]["metadata"]["source"] == "file"' "vector get"

out="$(cli_json vector exists vecs va)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "vector exists"

out="$(cli_json vector keys vecs --limit 10)"
assert_json "$out" 'data["type"] == "vector_key_page" and len(data["data"]["items"]) >= 3' "vector keys"

out="$(cli_json vector query vecs '1.0,0.0' -k 2)"
assert_json "$out" 'data["type"] == "vector_matches" and len(data["data"]) >= 1' "vector query"

out="$(cli_json vector query vecs '1.0,0.0' -k 2 --diagnostics)"
assert_json "$out" 'data["type"] == "vector_index_query" and "matches" in data["data"] and "diagnostics" in data["data"]' "vector query diagnostics"

cli_json vector update-metadata vecs vb --file "$patch_file" >/dev/null
out="$(cli_json vector get vecs vb)"
assert_json "$out" 'data["data"]["data"]["metadata"]["tag"] == "updated"' "vector update metadata"

out="$(cli_json vector count vecs)"
assert_json "$out" 'data["type"] == "uint" and data["data"] >= 3' "vector count"

out="$(cli_json vector history vecs vb)"
assert_json "$out" 'data["type"] == "vector_version_history" and data["data"] is not None' "vector history"

cli_json vector delete-by-filter vecs --filter-file "$filter_remove" >/dev/null
out="$(cli_json vector exists vecs vr)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "vector delete by filter"

cli_json vector del vecs va >/dev/null
out="$(cli_json vector exists vecs va)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "vector delete alias"

vector_batch="$FILES/vector-batch.json"
write_json "$vector_batch" '{"type":"vector_batch_upsert","collection":"vecs","entries":[{"key":"vbatch1","vector":[0.2,0.8],"metadata":{"tag":"batch"}},{"key":"vbatch2","vector":[0.8,0.2],"metadata":{"tag":"batch"}}]}'
out="$(raw_command_file "$vector_batch")"
assert_json "$out" 'data["type"] == "vector_batch_upsert_results" and data["data"]["status"] == "ok"' "vector batch upsert raw"

out="$(cli_json vector delete-all vecs)"
assert_json "$out" 'data["type"] == "vector_bulk_delete_result"' "vector delete all"

out="$(cli_json vector collection del vecs)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "vector collection delete alias"

section "Event commands"
DB="$TMP/event-db"
event_file="$FILES/event.json"
write_json "$event_file" '{"source":"file","n":2}'
out="$(cli_json event append user.created '{"name":"Ada"}')"
assert_json "$out" 'data["type"] == "event_append_result" and data["data"]["sequence"] == 0' "event append"
event_ts="$(json_value "$out" 'data["data"]["timestamp"]')"

out="$(cli_json event append user.updated --file "$event_file")"
assert_json "$out" 'data["type"] == "event_append_result" and data["data"]["sequence"] == 1' "event append file"

out="$(cli_json event get 0)"
assert_json "$out" 'data["type"] == "event_record" and data["data"]["event"]["sequence"] == 0' "event get"

out="$(cli_json event exists 0)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "event exists"

out="$(cli_json event len)"
assert_json "$out" 'data["type"] == "event_length" and data["data"]["count"] >= 2' "event len"

out="$(cli_json event list --limit 10)"
assert_json "$out" 'data["type"] == "event_records" and len(data["data"]["items"]) >= 2' "event list"

out="$(cli_json event types)"
assert_json "$out" 'data["type"] == "event_type_list" and "user.created" in data["data"]["items"]' "event types"

out="$(cli_json event by-type user.created --limit 5)"
assert_json "$out" 'data["type"] == "event_records" and len(data["data"]["items"]) >= 1' "event by type"

out="$(cli_json event range 0 --end-seq 2 --limit 10)"
assert_json "$out" 'data["type"] == "event_range_result" and len(data["data"]["items"]) >= 2' "event range"

out="$(cli_json event range-time "$event_ts" --limit 10)"
assert_json "$out" 'data["type"] == "event_range_result" and len(data["data"]["items"]) >= 1' "event range time"

out="$(cli_json event verify-chain)"
assert_json "$out" 'data["type"] == "event_chain_verification"' "event verify chain"

event_batch="$FILES/event-batch.json"
write_json "$event_batch" '{"type":"event_batch_append","entries":[{"event_type":"batch.one","payload":{"x":1}},{"event_type":"batch.two","payload":{"x":2}}]}'
out="$(raw_command_file "$event_batch")"
assert_json "$out" 'data["type"] == "event_batch_append_results" and data["data"]["status"] == "ok"' "event batch append raw"

section "Graph commands"
DB="$TMP/graph-db"
node_props="$FILES/node-props.json"
edge_props="$FILES/edge-props.json"
write_json "$node_props" '{"kind":"person","name":"Ada"}'
write_json "$edge_props" '{"since":2026}'

out="$(cli_json graph create people)"
assert_json "$out" 'data["type"] == "graph_info" and data["data"]["graph"] == "people"' "graph create"

out="$(cli_json graph list --limit 10)"
assert_json "$out" 'data["type"] == "graph_name_page" and "people" in data["data"]["items"]' "graph list"

out="$(cli_json graph meta people)"
assert_json "$out" 'data["type"] == "graph_info_result" and data["data"]["graph"] == "people"' "graph meta"

cli_json graph add-node people ada --properties-file "$node_props" >/dev/null
cli_json graph add-node people bob --properties '{"kind":"person","name":"Bob"}' >/dev/null

out="$(cli_json graph get-node people ada)"
assert_json "$out" 'data["type"] == "graph_node_result" and data["data"]["node_id"] == "ada"' "graph get node"

out="$(cli_json graph list-nodes people --limit 10)"
assert_json "$out" 'data["type"] == "graph_node_page" and any(item["node_id"] == "ada" for item in data["data"]["items"]) and any(item["node_id"] == "bob" for item in data["data"]["items"])' "graph list nodes"

cli_json graph add-edge people ada knows bob --weight 0.7 --properties-file "$edge_props" >/dev/null

out="$(cli_json graph get-edge people ada knows bob)"
assert_json "$out" 'data["type"] == "graph_edge_result" and data["data"]["src"] == "ada" and data["data"]["dst"] == "bob"' "graph get edge"

out="$(cli_json graph neighbors people ada --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and len(data["data"]["items"]) >= 1' "graph neighbors"

cli_json graph remove-edge people ada knows bob >/dev/null
cli_json graph remove-node people bob >/dev/null

graph_batch="$FILES/graph-batch.json"
write_json "$graph_batch" '{"type":"graph_batch_write","graph":"people","operations":[{"type":"upsert_node","node_id":"carol","data":{"properties":{"kind":"person"}}},{"type":"upsert_node","node_id":"dave","data":{"properties":{"kind":"person"}}},{"type":"upsert_edge","src":"carol","edge_type":"knows","dst":"dave","data":{"weight":1.0,"properties":{"via":"batch"}}}]}'
out="$(raw_command_file "$graph_batch")"
assert_json "$out" 'data["type"] == "graph_batch_write_result" and data["data"]["mode"] == "atomic" and data["data"]["status"] == "ok" and len(data["data"]["items"]) == 3' "graph batch write raw"

out="$(cli_json graph del people)"
assert_json "$out" 'data["type"] == "graph_delete_result"' "graph delete alias"

for deferred in "graph ontology list" "graph analytics bfs"; do
  if "$STRATA" --db "$DB" $deferred >/tmp/strata-cli-deferred.out 2>/tmp/strata-cli-deferred.err; then
    fail "deferred graph command unexpectedly succeeded: $deferred"
  fi
done

section "Arrow import/export"
DB="$TMP/arrow-db"
cli_json kv put arrow-source arrow-value >/dev/null
arrow_export="$FILES/kv-export.jsonl"
out="$(cli_json arrow export --primitive kv --format jsonl "$arrow_export" --prefix arrow --limit 10)"
assert_json "$out" 'data["type"] == "arrow_export_result" and data["data"]["row_count"] >= 1' "arrow export kv"
assert_file_nonempty "$arrow_export"

arrow_import="$FILES/kv-import.csv"
cat > "$arrow_import" <<'CSV'
key,value
arrow-imported,from-csv
CSV
out="$(cli_json_space default arrow import "$arrow_import" --format csv --target kv --key-column key --value-column value)"
assert_json "$out" 'data["type"] == "arrow_import_result" and data["data"]["rows_imported"] == 1' "arrow import kv"

out="$(cli_json kv get arrow-imported)"
assert_json "$out" 'bytes(data["data"]["value"]).decode() == "from-csv"' "arrow imported value"

section "Pipe mode"
pipe_out="$(printf 'kv put pipe-key pipe-value\nkv get pipe-key\n' | "$STRATA" --db "$TMP/pipe-db" --raw)"
assert_eq "$pipe_out" "pipe-value" "pipe mode raw kv round trip"

section "Done"
echo "CLI command matrix passed"
