#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "vector-index-filters-branches"

scenario_section "collection setup and enough rows for indexed diagnostics"
out="$(cli_json vector collection create articles 3 --metric euclidean)"
assert_json "$out" 'data["type"] == "vector_collection_list" and data["data"]["items"][0]["name"] == "articles"' "vector collection create"

for i in $(seq 0 39); do
  if ((i % 4 == 0)); then
    tag="remove"
  elif ((i % 2 == 0)); then
    tag="keep"
  else
    tag="review"
  fi
  x="$(python3 - <<PY
i = $i
print(f"{i / 40.0:.4f},{(40 - i) / 40.0:.4f},{(i % 7) / 7.0:.4f}")
PY
)"
  cli_json vector upsert articles "doc-$i" "$x" --metadata "{\"tag\":\"$tag\",\"rank\":$i}" >/dev/null
done

out="$(cli_json vector count articles)"
assert_json "$out" 'data["type"] == "uint" and data["data"] == 40' "vector count after load"

filter_keep="$CLI_CORPUS_FILES/vector-filter-keep.json"
write_json "$filter_keep" '{"conditions":[{"field":"tag","op":"eq","value":{"type":"string","value":"keep"}}]}'
out="$(cli_json vector query articles '0.5,0.5,0.2' -k 5 --filter-file "$filter_keep")"
assert_json "$out" 'data["type"] == "vector_matches" and len(data["data"]) == 5 and all(item["metadata"]["tag"] == "keep" for item in data["data"])' "vector filtered query"

out="$(cli_json vector query articles '0.5,0.5,0.2' -k 5 --diagnostics)"
assert_json "$out" 'data["type"] == "vector_index_query" and "matches" in data["data"] and "diagnostics" in data["data"] and len(data["data"]["matches"]) == 5' "vector diagnostics query"

scenario_section "metadata patch, history, and delete by filter"
patch="$CLI_CORPUS_FILES/vector-patch.json"
write_json "$patch" '{"tag":"keep","rank":999,"patched":true}'
cli_json vector update-metadata articles doc-1 --file "$patch" >/dev/null
out="$(cli_json vector get articles doc-1)"
assert_json "$out" 'data["type"] == "vector_data" and data["data"]["data"]["metadata"]["patched"] is True and data["data"]["data"]["metadata"]["rank"] == 999' "vector metadata patch"

out="$(cli_json vector history articles doc-1)"
assert_json "$out" 'data["type"] == "vector_version_history" and data["data"]["count"] >= 2 and len(data["data"]["items"]) == data["data"]["count"] and data["data"]["items"][0]["data"]["metadata"]["patched"] is True and data["data"]["items"][1]["data"]["metadata"]["tag"] == "review"' "vector history after patch"

filter_remove="$CLI_CORPUS_FILES/vector-filter-remove.json"
write_json "$filter_remove" '{"conditions":[{"field":"tag","op":"eq","value":{"type":"string","value":"remove"}}]}'
out="$(cli_json vector delete-by-filter articles --filter-file "$filter_remove")"
assert_json "$out" 'data["type"] == "vector_bulk_delete_result" and data["data"]["deleted_count"] == 10' "vector delete by filter"
out="$(cli_json vector query articles '0.5,0.5,0.2' -k 20 --filter-file "$filter_remove")"
assert_json "$out" 'data["type"] == "vector_matches" and len(data["data"]) == 0' "deleted filter no longer matches"

scenario_section "branch and space divergence for vector collections"
out="$(cli_json branch fork default vectors-child)"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "vectors-child"' "vector branch fork"

cli_json_branch vectors-child vector upsert articles child-only '0.9,0.05,0.05' --metadata '{"tag":"keep","branch":"child"}' >/dev/null
cli_json_branch vectors-child vector del articles doc-2 >/dev/null

out="$(cli_json_branch vectors-child vector exists articles child-only)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "child vector write visible"
out="$(cli_json vector exists articles child-only)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "parent does not see child vector"
out="$(cli_json vector exists articles doc-2)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "parent keeps deleted child vector"
out="$(cli_json_branch vectors-child vector exists articles doc-2)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "child delete hides inherited vector"

cli_json space create embeddings >/dev/null
cli_json_space embeddings vector collection create articles 3 --metric euclidean >/dev/null
cli_json_space embeddings vector upsert articles space-only '0.1,0.2,0.3' --metadata '{"tag":"space"}' >/dev/null
out="$(cli_json vector exists articles space-only)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "default space does not see vector space row"
out="$(cli_json_space embeddings vector exists articles space-only)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is True' "vector space row visible in its space"
