#!/usr/bin/env bash
# Output formats and the serialized-command boundary: human vs --json vs --raw,
# base64 wire-truth, structured error envelopes, and `command run`/`print`
# (the exact path MCP/agents use).
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

seed "$DB" kv put greeting hello

echo "[$SUITE_NAME] one row, three formats"
expect_out "human shows text" "hello" -- "$DB" kv get greeting
expect_out "raw shows the bare value" "hello" -- --raw "$DB" kv get greeting
run --json "$DB" kv get greeting
check_ok "json format succeeds"
check_eq "json is the wire-true envelope" \
  '{"data":{"timestamp":2,"value":"aGVsbG8=","version":2},"type":"kv_versioned_value"}' \
  "$(python3 -c 'import json,sys;d=json.load(sys.stdin);d["data"]["timestamp"]=2;d["data"]["version"]=2;print(json.dumps(d,sort_keys=True,separators=(",",":")))' <<<"$OUT")"

echo "[$SUITE_NAME] json envelopes are machine-parseable everywhere"
for args in "kv list" "branch list" "space list" "info" "health"; do
  # shellcheck disable=SC2086
  run --json "$DB" $args
  check_ok "--json $args succeeds"
  if python3 -c 'import json,sys;d=json.load(sys.stdin);assert "type" in d' <<<"$OUT" 2>/dev/null; then
    _ok
  else
    _fail "--json $args emits a tagged envelope" "output: $OUT"
  fi
done

echo "[$SUITE_NAME] structured errors"
run --json "$DB" kv put '' empty-key
if [[ $STATUS -eq 0 ]]; then
  _fail "empty key is rejected" "exit=0 out=$OUT"
else
  _ok
fi
if python3 -c 'import json,sys;d=json.load(sys.stdin);e=d["error"];assert e["code"] and e["class"] and e["reference_id"]' <<<"$ERR" 2>/dev/null; then
  _ok
else
  _fail "error envelope carries code/class/reference_id" "stderr: $ERR"
fi
run "$DB" kv put '' empty-key
check_contains "human error is a single code: message line" ": " "$ERR"

echo "[$SUITE_NAME] the serialized command boundary (agent path)"
expect_ok "command print validates without a database" -- command print --command-json '{"type":"ping"}'
expect_fail "command print rejects unknown commands" "" -- command print --command-json '{"type":"not_a_command"}'
run --json "$DB" command run --command-json '{"type":"kv_put","key":"YWdlbnQ=","value":"ZnJvbS1qc29u"}'
check_ok "raw kv_put with base64 bytes executes"
expect_out "the CLI reads what the agent wrote" "from-json" -- "$DB" kv get agent
run --json "$DB" command run --command-json '{"type":"kv_get","key":"YWdlbnQ="}'
check_ok "raw kv_get executes"
check_contains "raw read returns base64 wire bytes" '"value":"ZnJvbS1qc29u"' "$OUT"
run --json "$DB" command run --command-json '{"type":"kv_scan","limit":1}'
check_ok "raw kv_scan executes"
check_contains "raw scan reports honest pagination" '"has_more":true' "$OUT"
cursor="$(python3 -c 'import json,sys;print(json.load(sys.stdin)["data"]["cursor"])' <<<"$OUT")"
run --json "$DB" command run --command-json "{\"type\":\"kv_scan\",\"start\":\"$cursor\",\"limit\":10}"
check_ok "cursor from the wire feeds back through the wire"

finish
