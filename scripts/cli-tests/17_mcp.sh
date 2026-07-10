#!/usr/bin/env bash
# The MCP channel (first-run D8): `strata mcp serve` speaks newline-delimited
# JSON-RPC over stdio — same wire envelopes and error codes as the CLI.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

mcp_session() { # reads JSON-RPC lines on stdin, serves against $DB
  "$STRATA_BIN" "$DB" mcp serve 2>"$WORK_DIR/mcp-stderr"
}

echo "[$SUITE_NAME] a full client session"
SESSION_OUT="$WORK_DIR/mcp-session.jsonl"
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"strata_kv_put","arguments":{"key":"greeting","value":"hello"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"strata_kv_get","arguments":{"key":"greeting"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"strata_branch_fork","arguments":{"source":"default","branch":"side"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"strata_kv_put","arguments":{"key":"greeting","value":"forked","branch":"side"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"strata_command","arguments":{"command":{"type":"kv_scan","limit":1}}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"strata_guide"}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"strata_kv_get","arguments":{"key":"x","bogus":1}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"strata_kv_get","arguments":{"key":"x","branch":"ghost"}}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"no_such_tool"}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":12,"method":"nonexistent/method"}'
  printf '%s\n' '{"jsonrpc":"2.0","id":13,"method":"ping"}'
} | mcp_session >"$SESSION_OUT"
check_eq "the server answers every request and no notification" 13 "$(wc -l <"$SESSION_OUT")"

verdicts="$(python3 - "$SESSION_OUT" <<'PY'
import json, sys

by_id = {}
for line in open(sys.argv[1]):
    m = json.loads(line)
    by_id[m["id"]] = m

def result(rid): return by_id[rid]["result"]
def body(rid): return json.loads(result(rid)["content"][0]["text"])

checks = []
init = result(1)
checks.append(("initialize names the server", init["serverInfo"]["name"] == "strata"))
checks.append(("initialize echoes the protocol version", init["protocolVersion"] == "2025-06-18"))
checks.append(("instructions teach strata_guide first", "strata_guide" in init["instructions"]))

tools = result(2)["tools"]
names = {t["name"] for t in tools}
checks.append(("the curated surface is 20 tools", len(tools) == 20))
checks.append(("meta-tools are present", {"strata_guide", "strata_command"} <= names))
checks.append(("every tool has a schema and teaching description",
               all(t["inputSchema"]["type"] == "object" and len(t["description"]) > 20 for t in tools)))

checks.append(("kv_put returns the wire write envelope", body(3)["type"] == "write_result"))
checks.append(("kv_get returns wire-true base64", body(4)["data"]["value"] == "aGVsbG8="))
checks.append(("branch fork works over MCP", body(5)["type"] == "branch_create_result"
               or body(5)["type"].startswith("branch")))
checks.append(("branch-scoped writes work over MCP", not result(6)["isError"]))
checks.append(("strata_command executes raw wire JSON", body(7)["type"] == "kv_scan_result"))
checks.append(("the guide flows through the meta-tool", "usage guide" in result(8)["content"][0]["text"]))

bad = result(9)
checks.append(("invalid arguments are teaching errors", bad["isError"] and "unknown field" in bad["content"][0]["text"]))
ghost = result(10)
checks.append(("executor errors carry the stable code", ghost["isError"]
               and json.loads(ghost["content"][0]["text"])["error"]["code"] == "not_found.engine.branch"))
checks.append(("unknown tools are protocol errors", by_id[11]["error"]["code"] == -32602))
checks.append(("unknown methods are protocol errors", by_id[12]["error"]["code"] == -32601))
checks.append(("ping answers", result(13) == {}))

for description, ok in checks:
    print(("PASS" if ok else "FAIL") + " " + description)
PY
)"
while IFS= read -r verdict; do
  if [[ "$verdict" == PASS* ]]; then _ok; else _fail "${verdict#FAIL }" "see $SESSION_OUT"; fi
done <<<"$verdicts"

echo "[$SUITE_NAME] the session's writes are durable"
expect_out "the CLI reads what MCP wrote" "hello" -- "$DB" kv get greeting
expect_out "the branch-scoped write landed on its branch" "forked" -- "$DB" kv get greeting --branch side

echo "[$SUITE_NAME] server refuses without a database target"
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"ping"}' | "$STRATA_BIN" mcp serve >/dev/null 2>"$WORK_DIR/refuse-err"
if [[ $? -ne 0 ]]; then _ok; else _fail "bare mcp serve refuses" "exit=0"; fi
check_contains "refusal is the standard teaching error" "invalid_argument.cli.no_database" "$(cat "$WORK_DIR/refuse-err")"

echo "[$SUITE_NAME] stdout stays protocol-clean"
if python3 -c '
import json, sys
for line in open(sys.argv[1]):
    json.loads(line)
' "$SESSION_OUT"; then _ok; else _fail "every stdout line is a JSON-RPC message" "see $SESSION_OUT"; fi

finish
