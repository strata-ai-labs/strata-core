#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "arrow-pipe-raw-errors"

scenario_section "raw command print/run and pipe mode"
ping_cmd="$CLI_CORPUS_FILES/raw-ping.json"
write_json "$ping_cmd" '{"type":"ping"}'
out="$("$STRATA" --json command print --file "$ping_cmd")"
assert_json "$out" 'data["type"] == "ping"' "raw command print"
out="$(raw_command_file "$ping_cmd")"
assert_json "$out" 'data["type"] == "pong"' "raw command run"

pipe_db="$CLI_CORPUS_TMP/pipe-corpus-db"
pipe_out="$(printf '# comments are skipped\nkv put pipe-a A\nkv put pipe-b B\nkv get pipe-a\nkv get pipe-b\n' | "$STRATA" --db "$pipe_db" --raw)"
assert_eq "$pipe_out" $'A\nB' "pipe mode multi-command raw reads"

scenario_section "arrow import and multi-primitive export"
cli_json kv put export-kv export-value >/dev/null
cli_json json set export-json '$' '{"ok":true}' >/dev/null
cli_json event append export.event '{"ok":true}' >/dev/null
cli_json vector collection create export-vectors 2 --metric cosine >/dev/null
cli_json vector upsert export-vectors export-vector '0.1,0.9' --metadata '{"ok":true}' >/dev/null
cli_json graph create export-graph >/dev/null
cli_json graph add-node export-graph export-node --properties '{"ok":true}' >/dev/null
cli_json graph add-node export-graph export-node-2 --properties '{"ok":true}' >/dev/null
cli_json graph add-edge export-graph export-node relates export-node-2 --weight 1.0 >/dev/null

for primitive in kv json event; do
  path="$CLI_CORPUS_FILES/export-$primitive.jsonl"
  out="$(cli_json arrow export --primitive "$primitive" --format jsonl "$path" --limit 10)"
  assert_json "$out" 'data["type"] == "arrow_export_result" and data["data"]["row_count"] >= 1' "arrow export $primitive"
  assert_file_nonempty "$path"
done

vector_export="$CLI_CORPUS_FILES/export-vector.jsonl"
out="$(cli_json arrow export --primitive vector --format jsonl "$vector_export" --collection export-vectors --limit 10)"
assert_json "$out" 'data["type"] == "arrow_export_result" and data["data"]["row_count"] >= 1' "arrow export vector"
assert_file_nonempty "$vector_export"

graph_export="$CLI_CORPUS_FILES/export-graph.jsonl"
out="$(cli_json arrow export --primitive graph --format jsonl "$graph_export" --graph export-graph --limit 10)"
assert_json "$out" 'data["type"] == "arrow_export_result" and data["data"]["row_count"] >= 2 and len(data["data"]["paths"]) == 2' "arrow export graph"
JSON_PAYLOAD="$out" REQUESTED_PATH="$graph_export" python3 - <<'PY'
import json
import os
import sys

data = json.loads(os.environ["JSON_PAYLOAD"])
requested = os.environ["REQUESTED_PATH"]
paths = data["data"]["paths"]
if requested in paths:
    print("graph export should report concrete node/edge paths, not the requested stem", file=sys.stderr)
    sys.exit(1)
if os.path.exists(requested):
    print(f"graph export stem should not be consumed as a data file: {requested}", file=sys.stderr)
    sys.exit(1)
if not paths[0].endswith("_nodes.jsonl") or not paths[1].endswith("_edges.jsonl"):
    print(f"unexpected graph export paths: {paths}", file=sys.stderr)
    sys.exit(1)
for path in data["data"]["paths"]:
    if not os.path.isfile(path):
        print(f"missing graph export path: {path}", file=sys.stderr)
        sys.exit(1)
    if os.path.getsize(path) == 0:
        print(f"empty graph export path: {path}", file=sys.stderr)
        sys.exit(1)
PY

kv_import="$CLI_CORPUS_FILES/import-kv.csv"
printf 'key,value\nimported-a,alpha\nimported-b,beta\n' > "$kv_import"
out="$(cli_json arrow import "$kv_import" --format csv --target kv --key-column key --value-column value)"
assert_json "$out" 'data["type"] == "arrow_import_result" and data["data"]["rows_imported"] == 2' "arrow import kv csv"
out="$(cli_json kv get imported-b)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "beta"' "arrow imported kv value"

scenario_section "executor errors render structured JSON"
expect_json_error \
  "missing branch error" \
  '"error" in data and data["error"]["code"].startswith("not_found.") and data["error"]["retry_policy"] == "never" and data["error"]["retryable"] is False' \
  "$STRATA" --db "$DB" --json --branch missing kv get anything

expect_json_error \
  "invalid vector dimension" \
  '"error" in data and data["error"]["code"].startswith("invalid_argument.") and data["error"]["retry_policy"] == "never" and data["error"]["retryable"] is False' \
  "$STRATA" --db "$DB" --json vector upsert export-vectors bad '1.0'

if "$STRATA" --db "$DB" search >/tmp/strata-cli-corpus-deferred.out 2>/tmp/strata-cli-corpus-deferred.err; then
  fail "deferred search command unexpectedly succeeded"
fi
