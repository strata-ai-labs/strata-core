#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "kv-json-pages-batches"

scenario_section "KV pagination, scan, history, and delete"
for i in 01 02 03 04 05 06 07; do
  cli_json kv put "page-$i" "value-$i" >/dev/null
done

out="$(cli_json kv list --prefix page- --limit 3)"
assert_json "$out" 'data["type"] == "keys_page" and data["data"]["has_more"] is True and [bytes_to_text(item) for item in data["data"]["items"]] == ["page-01", "page-02", "page-03"]' "kv first page"

out="$(cli_json kv list --prefix page- --limit 3 --cursor page-03)"
assert_json "$out" 'data["type"] == "keys_page" and data["data"]["has_more"] is True and [bytes_to_text(item) for item in data["data"]["items"]] == ["page-04", "page-05", "page-06"]' "kv second page"

out="$(cli_json kv list --prefix page- --limit 3 --cursor page-06)"
assert_json "$out" 'data["type"] == "keys_page" and data["data"]["has_more"] is False and data["data"]["cursor"] is None and [bytes_to_text(item) for item in data["data"]["items"]] == ["page-07"]' "kv terminal page"

out="$(cli_json kv scan --start page-03 --limit 2)"
assert_json "$out" 'data["type"] == "kv_scan_result" and [bytes_to_text(item["key"]) for item in data["data"]["items"]] == ["page-03", "page-04"]' "kv scan start limit"

cli_json kv put page-03 value-03b >/dev/null
out="$(cli_json kv history page-03)"
assert_json "$out" 'data["type"] == "version_history" and data["data"]["count"] >= 2 and len(data["data"]["items"]) == data["data"]["count"] and bytes_to_text(data["data"]["items"][0]["value"]) == "value-03b" and bytes_to_text(data["data"]["items"][1]["value"]) == "value-03"' "kv version history"

cli_json kv del page-07 >/dev/null
out="$(cli_json kv exists page-07)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "kv delete removes key"

scenario_section "KV raw batch with itemwise get semantics"
kv_batch="$CLI_CORPUS_FILES/kv-corpus-batch-put.json"
write_json "$kv_batch" '{"type":"kv_batch_put","entries":[{"key":[98,97,116,99,104,45,97],"value":[65]},{"key":[98,97,116,99,104,45,98],"value":[66]}]}'
out="$(raw_command_file "$kv_batch")"
assert_json "$out" 'data["type"] == "batch_results" and data["data"]["mode"] == "itemwise" and data["data"]["status"] == "ok" and len(data["data"]["items"]) == 2' "kv batch put"

kv_batch_get="$CLI_CORPUS_FILES/kv-corpus-batch-get.json"
write_json "$kv_batch_get" '{"type":"kv_batch_get","keys":[[98,97,116,99,104,45,97],[109,105,115,115,105,110,103],[98,97,116,99,104,45,98]]}'
out="$(raw_command_file "$kv_batch_get")"
assert_json "$out" 'data["type"] == "batch_get_results" and [item["status"] for item in data["data"]["items"]] == ["ok", "ok", "ok"] and [item["result"]["found"] for item in data["data"]["items"]] == [True, False, True]' "kv batch get miss"

scenario_section "JSON null, missing, pagination, indexes, and batches"
cli_json json set doc-01 '$' '{"name":"Ada","rank":1,"tags":["engine"]}' >/dev/null
cli_json json set doc-02 '$' '{"name":"Grace","rank":2,"tags":["compiler"]}' >/dev/null
cli_json json set doc-03 '$' '{"name":"Lin","rank":3,"tags":[]}' >/dev/null
cli_json json set doc-null '$' null >/dev/null

out="$(cli_json json get doc-null '$')"
assert_json "$out" 'data["type"] == "json_versioned_value" and data["data"]["found"] is True and data["data"]["value"]["value"] is None' "json stored null is present"
out="$(cli_json json get missing '$')"
assert_json "$out" 'data["type"] == "json_versioned_value" and data["data"]["found"] is False' "json missing is distinct"

out="$(cli_json json list --prefix doc- --limit 2)"
assert_json "$out" 'data["type"] == "json_list_result" and data["data"]["has_more"] is True and data["data"]["items"] == ["doc-01", "doc-02"]' "json first page"
out="$(cli_json json list --prefix doc- --limit 2 --cursor doc-02)"
assert_json "$out" 'data["type"] == "json_list_result" and data["data"]["items"] == ["doc-03", "doc-null"] and data["data"]["has_more"] is False' "json terminal page includes stored null"

out="$(cli_json json index create by-rank '$.rank' --index-type numeric)"
assert_json "$out" 'data["type"] == "json_index_definition" and data["data"]["name"] == "by-rank"' "json numeric index create"
out="$(cli_json json index create by-name '$.name' --index-type text)"
assert_json "$out" 'data["type"] == "json_index_definition" and data["data"]["name"] == "by-name"' "json text index create"
out="$(cli_json json index list)"
assert_json "$out" 'data["type"] == "json_index_list" and sorted(item["name"] for item in data["data"]["items"]) == ["by-name", "by-rank"]' "json index list"
cli_json json index drop by-name >/dev/null
out="$(cli_json json index list)"
assert_json "$out" 'data["type"] == "json_index_list" and [item["name"] for item in data["data"]["items"]] == ["by-rank"]' "json index drop"

json_batch="$CLI_CORPUS_FILES/json-corpus-batch.json"
write_json "$json_batch" '{"type":"json_batch_set","entries":[{"key":"batch-json-a","path":"$","value":{"n":1}},{"key":"batch-json-b","path":"$","value":{"n":2}}]}'
out="$(raw_command_file "$json_batch")"
assert_json "$out" 'data["type"] == "json_batch_results" and data["data"]["mode"] == "itemwise" and data["data"]["status"] == "ok"' "json batch set"
