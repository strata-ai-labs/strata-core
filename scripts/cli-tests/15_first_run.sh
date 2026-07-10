#!/usr/bin/env bash
# First contact with the binary (first-run D2): database-target resolution for
# one-shot and piped invocations — explicit path/--db, then STRATA_DB, then a
# teaching refusal. Never an implicit database in cwd.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] bare one-shot commands refuse with a teaching error"
sandbox="$WORK_DIR/bare-cwd"
mkdir -p "$sandbox"
cd "$sandbox" || exit 1
run kv put greeting hello
if [[ $STATUS -ne 0 ]]; then _ok; else _fail "bare one-shot refuses" "exit=0 out=$OUT"; fi
check_contains "refusal names the error code" "invalid_argument.cli.no_database" "$ERR"
check_contains "refusal teaches the fix" "hint: " "$ERR"
check_contains "refusal mentions STRATA_DB" "STRATA_DB" "$ERR"
if [[ -z "$(ls -A "$sandbox")" ]]; then
  _ok
else
  _fail "refusal creates no files in cwd" "cwd contents: $(ls -A "$sandbox")"
fi

echo "[$SUITE_NAME] piped sessions refuse the same way"
printf 'kv get x\n' | "$STRATA_BIN" >"$WORK_DIR/pipe-out" 2>"$WORK_DIR/pipe-err"
if [[ $? -ne 0 ]]; then _ok; else _fail "bare piped session refuses" "$(cat "$WORK_DIR/pipe-out")"; fi
check_contains "piped refusal names the code" "invalid_argument.cli.no_database" "$(cat "$WORK_DIR/pipe-err")"

echo "[$SUITE_NAME] STRATA_DB is the per-session fallback target"
export STRATA_DB="$WORK_DIR/env-db"
run kv put greeting from-env
check_ok "STRATA_DB write succeeds without a path"
expect_out "STRATA_DB read sees the write" "from-env" -- kv get greeting

echo "[$SUITE_NAME] explicit targets beat STRATA_DB"
seed "$DB" kv put greeting from-path
expect_out "positional path wins over the env var" "from-path" -- "$DB" kv get greeting
expect_out "the env target is untouched" "from-env" -- kv get greeting
unset STRATA_DB

echo "[$SUITE_NAME] --cache is the explicit ephemeral escape"
run --cache kv put ephemeral yes
check_ok "--cache one-shot works with no path"
if [[ -z "$(ls -A "$sandbox")" ]]; then
  _ok
else
  _fail "cache mode creates no files" "cwd contents: $(ls -A "$sandbox")"
fi

finish
