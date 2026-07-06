#!/usr/bin/env bash
# Admin surface: init, liveness, info/health/metrics/describe, and config reads.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] init and liveness"
# `init` is currently a first-run placeholder: it prepares the Strata home and
# prints next steps, but does not open/create the database at the given path
# despite its help text (tracked in the first-run workstream,
# docs/design/first-run-experience.md).
run --json init
check_ok "init prepares the Strata home"
check_contains "init reports its envelope" '"type":"init"' "$OUT"
check_contains "init points at the sandboxed home" "$STRATA_HOME" "$OUT"
if [[ -d "$STRATA_HOME" ]]; then _ok; else _fail "init creates the home directory" "missing: $STRATA_HOME"; fi
expect_json "a fresh database is durable" '["data"]["durable"]' "true" -- "$DB" info
expect_contains "ping answers pong" "pong" -- "$DB" ping

echo "[$SUITE_NAME] info facts"
expect_json "info reports the default branch" '["data"]["default_branch"]' "default" -- "$DB" info
expect_json "info reports an open handle" '["data"]["open"]' "true" -- "$DB" info
seed "$DB" branch fork default extra
expect_json "branch count tracks forks" '["data"]["branch_count"]' 2 -- "$DB" info

echo "[$SUITE_NAME] health, metrics, describe"
run --json "$DB" health
check_ok "health succeeds"
check_contains "health has a status" '"status"' "$OUT"
run --json "$DB" metrics
check_ok "metrics succeeds"
run --json "$DB" describe
check_ok "describe succeeds"

echo "[$SUITE_NAME] config reads"
run --json "$DB" config get
check_ok "config get succeeds"
run "$DB" config get-key target
check_ok "config get-key succeeds for a known key"
expect_out "unknown config keys read as nil" "(nil)" -- "$DB" config get-key nonexistent_key

finish
