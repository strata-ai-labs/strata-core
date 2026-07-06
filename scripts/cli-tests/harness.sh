# Shared harness for the Strata CLI end-to-end suites.
#
# Each suite is a standalone bash script that sources this file, drives the
# real `strata` binary against a fresh throwaway database, and asserts on
# stdout/stderr/exit codes. The suites are black-box product tests: they know
# nothing about crate internals, only the documented CLI surface.
#
# Usage:
#   cargo build -p strata-cli-next
#   scripts/cli-tests/run_all.sh            # everything
#   scripts/cli-tests/02_branch.sh          # one suite
#   STRATA_BIN=/path/to/strata scripts/cli-tests/run_all.sh
#
# Conventions:
#   $DB       — per-suite durable database path (fresh temp dir)
#   $WORK_DIR — per-suite scratch dir for value files
#   run …     — invoke the binary, capturing OUT / ERR / STATUS

set -u -o pipefail

if [[ -z "${STRATA_BIN:-}" ]]; then
  _harness_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  STRATA_BIN="$_harness_root/target/debug/strata"
fi
if [[ ! -x "$STRATA_BIN" ]]; then
  echo "error: strata binary not found at $STRATA_BIN — run: cargo build -p strata-cli-next" >&2
  exit 2
fi

SUITE_NAME="$(basename "${0%.sh}")"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/strata-cli-test.XXXXXX")"
DB="$WORK_DIR/db"
# First-run helpers (init) write to the Strata home; point it at the sandbox
# so suites never touch the real ~/.strata. STRATA_DB is a database-target
# fallback (first-run D2) — clear it so an ambient value never leaks into
# assertions that expect explicit targeting.
export STRATA_HOME="$WORK_DIR/strata-home"
unset STRATA_DB
trap 'rm -rf "$WORK_DIR"' EXIT

PASS=0
FAIL=0
KNOWN=0

# ---- execution ------------------------------------------------------------

# Runs the binary with the given args; fills OUT, ERR, STATUS.
run() {
  OUT="$("$STRATA_BIN" "$@" 2>"$WORK_DIR/.stderr")"
  STATUS=$?
  ERR="$(<"$WORK_DIR/.stderr")"
}

# Runs a mutation that is mere setup, aborting the suite if it fails —
# a broken fixture would cascade into misleading assertion failures.
seed() {
  run "$@"
  if [[ $STATUS -ne 0 ]]; then
    echo "  SEED FAILED ($*): exit=$STATUS" >&2
    echo "$ERR" | sed 's/^/        /' >&2
    FAIL=$((FAIL + 1))
    finish
  fi
}

# ---- assertions ------------------------------------------------------------

_ok() { PASS=$((PASS + 1)); }

_fail() { # desc detail…
  FAIL=$((FAIL + 1))
  echo "  FAIL: $1"
  shift
  local line
  for line in "$@"; do
    printf '        %s\n' "$line"
  done
}

check_eq() { # desc expected actual
  if [[ "$3" == "$2" ]]; then _ok; else _fail "$1" "expected: $2" "actual:   $3"; fi
}

# check_ok <desc> — asserts the most recent `run` exited 0.
check_ok() {
  if [[ $STATUS -eq 0 ]]; then _ok; else _fail "$1" "exit=$STATUS" "stderr: $ERR"; fi
}

check_contains() { # desc needle haystack
  if [[ "$3" == *"$2"* ]]; then _ok; else _fail "$1" "needle:   $2" "haystack: $3"; fi
}

# expect_out <desc> <exact-stdout> -- <cli args…>   (must exit 0)
expect_out() {
  local desc="$1" expected="$2"
  shift 3
  run "$@"
  if [[ $STATUS -ne 0 ]]; then
    _fail "$desc" "command failed: exit=$STATUS" "stderr: $ERR"
    return
  fi
  check_eq "$desc" "$expected" "$OUT"
}

# expect_contains <desc> <stdout-needle> -- <cli args…>   (must exit 0)
expect_contains() {
  local desc="$1" needle="$2"
  shift 3
  run "$@"
  if [[ $STATUS -ne 0 ]]; then
    _fail "$desc" "command failed: exit=$STATUS" "stderr: $ERR"
    return
  fi
  check_contains "$desc" "$needle" "$OUT"
}

# expect_ok <desc> -- <cli args…>   (exit 0 is the whole assertion)
expect_ok() {
  local desc="$1"
  shift 2
  run "$@"
  if [[ $STATUS -eq 0 ]]; then _ok; else _fail "$desc" "exit=$STATUS" "stderr: $ERR"; fi
}

# expect_fail <desc> <stderr-needle> -- <cli args…>   (must exit non-zero)
expect_fail() {
  local desc="$1" needle="$2"
  shift 3
  run "$@"
  if [[ $STATUS -eq 0 ]]; then
    _fail "$desc" "expected failure but exit=0" "stdout: $OUT"
    return
  fi
  check_contains "$desc" "$needle" "$ERR"
}

# expect_known_bug <desc> <correct-expected-stdout> -- <cli args…>
#
# Documents a confirmed product defect without hiding it: while the defect
# persists the suite stays green but prints a loud KNOWN-BUG line on every run;
# the day the product starts returning the correct value, this FAILS so the pin
# gets promoted to a real expect_out.
expect_known_bug() {
  local desc="$1" expected="$2"
  shift 3
  run "$@"
  if [[ $STATUS -eq 0 && "$OUT" == "$expected" ]]; then
    _fail "KNOWN BUG appears FIXED: $desc" "promote this expect_known_bug to expect_out"
    return
  fi
  KNOWN=$((KNOWN + 1))
  echo "  KNOWN-BUG: $desc"
  printf '        correct: %s\n        current: %s\n' "$expected" "$OUT"
  _ok
}

# ---- JSON helpers ----------------------------------------------------------

# json_field <accessor> — extracts a field from $OUT parsed as JSON.
# The accessor is a python subscript chain on `d`, e.g. '["data"]["commit"]["timestamp"]'.
json_field() {
  python3 -c '
import json, sys
d = json.load(sys.stdin)
value = eval("d" + sys.argv[1])
if isinstance(value, bool):
    print(str(value).lower())
else:
    print(value)
' "$1" <<<"$OUT"
}

# expect_json <desc> <accessor> <expected> -- <cli args…>
# Runs with the global --json flag prepended and asserts one extracted field.
expect_json() {
  local desc="$1" accessor="$2" expected="$3"
  shift 4
  run --json "$@"
  if [[ $STATUS -ne 0 ]]; then
    _fail "$desc" "command failed: exit=$STATUS" "stderr: $ERR"
    return
  fi
  local actual
  if ! actual="$(json_field "$accessor" 2>&1)"; then
    _fail "$desc" "field $accessor not found" "output: $OUT"
    return
  fi
  check_eq "$desc" "$expected" "$actual"
}

# json_error_field <accessor> — like json_field but parses the error envelope
# on $ERR. Structured logging (the error-boundary tracing line) may precede the
# envelope, so parse the last stderr line.
json_error_field() {
  printf '%s\n' "$ERR" | tail -n 1 | python3 -c '
import json, sys
d = json.load(sys.stdin)
value = eval("d" + sys.argv[1])
print(value)
' "$1"
}

# expect_error_code <desc> <error-code> -- <cli args…>
# Runs with --json and asserts the structured error envelope carries the code.
expect_error_code() {
  local desc="$1" code="$2"
  shift 3
  run --json "$@"
  if [[ $STATUS -eq 0 ]]; then
    _fail "$desc" "expected failure but exit=0" "stdout: $OUT"
    return
  fi
  local actual
  if ! actual="$(json_error_field '["error"]["code"]' 2>&1)"; then
    _fail "$desc" "no structured error envelope on stderr" "stderr: $ERR"
    return
  fi
  check_eq "$desc" "$code" "$actual"
}

# commit_timestamp — extracts data.commit.timestamp from the last --json OUT.
commit_timestamp() {
  json_field '["data"]["commit"]["timestamp"]'
}

# ---- summary ---------------------------------------------------------------

finish() {
  echo
  local known=""
  if [[ $KNOWN -gt 0 ]]; then known=", ${KNOWN} known-bug pins"; fi
  echo "== ${SUITE_NAME}: ${PASS} passed, ${FAIL} failed${known} =="
  if [[ $FAIL -eq 0 ]]; then exit 0; else exit 1; fi
}
