#!/usr/bin/env bash

if [[ -z "${STRATA:-}" || -z "${CLI_CORPUS_TMP:-}" || -z "${CLI_CORPUS_FILES:-}" ]]; then
  echo "cli corpus common.sh requires STRATA, CLI_CORPUS_TMP, and CLI_CORPUS_FILES" >&2
  exit 2
fi

scenario_section() {
  printf '\n[%s] %s\n' "$(basename "$0")" "$1"
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

write_json() {
  local path="$1"
  local json="$2"
  printf '%s\n' "$json" > "$path"
}

write_text() {
  local path="$1"
  local text="$2"
  printf '%s' "$text" > "$path"
}

new_db() {
  local name="$1"
  DB="$CLI_CORPUS_TMP/${name}-db"
  export DB
}

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
  shift || true
  "$STRATA" --db "$DB" --json command run --file "$path" "$@"
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

def bytes_to_text(value):
    return bytes(value).decode("utf-8")

scope = {
    "data": data,
    "bytes": bytes,
    "bytes_to_text": bytes_to_text,
    "len": len,
    "any": any,
    "all": all,
    "str": str,
    "sorted": sorted,
}

try:
    ok = bool(eval(expression, {"__builtins__": {}}, scope))
except Exception as exc:
    print(
        f"{label}: assertion raised {exc}\nexpr: {expression}\njson: {json.dumps(data, indent=2)}",
        file=sys.stderr,
    )
    sys.exit(1)

if not ok:
    print(
        f"{label}: assertion failed\nexpr: {expression}\njson: {json.dumps(data, indent=2)}",
        file=sys.stderr,
    )
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

def bytes_to_text(value):
    return bytes(value).decode("utf-8")

value = eval(
    sys.argv[1],
    {"__builtins__": {}},
    {"data": data, "bytes": bytes, "bytes_to_text": bytes_to_text, "len": len},
)
if isinstance(value, bytes):
    print(value.decode("utf-8"))
else:
    print(value)
PY
}

assert_json_lines() {
  local payload="$1"
  local expression="$2"
  local label="$3"
  JSON_LINES_PAYLOAD="$payload" python3 - "$expression" "$label" <<'PY'
import json
import os
import sys

payload = os.environ["JSON_LINES_PAYLOAD"]
expression = sys.argv[1]
label = sys.argv[2]
lines = [line for line in payload.splitlines() if line.strip()]

try:
    data = [json.loads(line) for line in lines]
except Exception as exc:
    print(f"{label}: output contains invalid JSON line: {exc}\n{payload}", file=sys.stderr)
    sys.exit(1)

def bytes_to_text(value):
    return bytes(value).decode("utf-8")

scope = {
    "data": data,
    "bytes": bytes,
    "bytes_to_text": bytes_to_text,
    "len": len,
    "any": any,
    "all": all,
    "str": str,
    "sorted": sorted,
}

try:
    ok = bool(eval(expression, {"__builtins__": {}}, scope))
except Exception as exc:
    print(
        f"{label}: assertion raised {exc}\nexpr: {expression}\njson-lines: {json.dumps(data, indent=2)}",
        file=sys.stderr,
    )
    sys.exit(1)

if not ok:
    print(
        f"{label}: assertion failed\nexpr: {expression}\njson-lines: {json.dumps(data, indent=2)}",
        file=sys.stderr,
    )
    sys.exit(1)
PY
}

expect_json_error() {
  local label="$1"
  local expression="$2"
  shift 2
  local stdout="$CLI_CORPUS_TMP/${label//[^A-Za-z0-9_]/_}.stdout"
  local stderr="$CLI_CORPUS_TMP/${label//[^A-Za-z0-9_]/_}.stderr"
  if "$@" >"$stdout" 2>"$stderr"; then
    fail "$label: command unexpectedly succeeded"
  fi
  local payload
  payload="$(cat "$stderr")"
  assert_json "$payload" "$expression" "$label"
}
