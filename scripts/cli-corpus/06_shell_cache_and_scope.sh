#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "shell-cache-and-scope"

scenario_section "first-run init uses STRATA_HOME"
out="$("$STRATA" --json init)"
assert_json "$out" 'data["type"] == "init" and data["data"]["home"] is not None' "init json"
[[ -d "$STRATA_HOME" ]] || fail "init did not create STRATA_HOME"

scenario_section "pipe mode preserves branch and space context"
pipe_db="$CLI_CORPUS_TMP/shell-scope-db"
pipe_out="$(
  {
    printf 'space create docs\n'
    printf 'kv put root-key root-value\n'
    printf 'use default docs\n'
    printf 'kv put scoped-key docs-value\n'
    printf 'kv get scoped-key\n'
    printf 'use default\n'
    printf 'kv get scoped-key\n'
    printf 'branch fork default shell-child\n'
    printf 'use shell-child docs\n'
    printf 'kv put scoped-key child-docs-value\n'
    printf 'kv get scoped-key\n'
    printf 'use default docs\n'
    printf 'kv get scoped-key\n'
  } | "$STRATA" --db "$pipe_db" --json
)"
assert_json_lines "$pipe_out" 'len(data) == 13 and data[4]["type"] == "kv_versioned_value" and bytes_to_text(data[4]["data"]["value"]) == "docs-value" and data[6]["type"] == "kv_versioned_value" and data[6]["data"] is None and data[10]["type"] == "kv_versioned_value" and bytes_to_text(data[10]["data"]["value"]) == "child-docs-value" and data[12]["type"] == "kv_versioned_value" and bytes_to_text(data[12]["data"]["value"]) == "docs-value"' "pipe context branch and space"

scenario_section "cache mode is process-local"
cache_out="$(printf 'kv put transient yes\nkv get transient\n' | "$STRATA" --cache --raw)"
assert_eq "$cache_out" "yes" "cache pipe keeps state inside process"
one_shot_cache="$("$STRATA" --cache --raw kv get transient)"
assert_eq "$one_shot_cache" "" "cache one-shot starts empty"

scenario_section "human and raw render modes remain scriptable"
cli_json kv put human-key human-value >/dev/null
out="$(cli_raw kv get human-key)"
assert_eq "$out" "human-value" "raw point read"
human_out="$("$STRATA" --db "$DB" kv get human-key)"
assert_eq "$human_out" "human-value" "human point read"
