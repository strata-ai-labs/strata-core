#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "branch-space-time-travel"

scenario_section "default branch writes and time reads"
out="$(cli_json kv put account/name Ada)"
assert_json "$out" 'data["type"] == "write_result" and data["data"]["effect"]["applied"] is True' "initial kv put"
first_version="$(json_value "$out" 'data["data"]["commit"]["version"]')"
first_timestamp="$(json_value "$out" 'data["data"]["commit"]["timestamp"]')"

cli_json kv put account/name Grace >/dev/null
out="$(cli_json kv get account/name --as-of "$first_timestamp")"
assert_json "$out" 'data["type"] == "kv_versioned_value" and bytes_to_text(data["data"]["value"]) == "Ada" and data["data"]["version"] == '"$first_version"' and data["data"]["timestamp"] == '"$first_timestamp" "kv get as-of timestamp"

out="$(cli_json branch fork default audit --version "$first_version")"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "audit"' "fork at retained version"
out="$(cli_json_branch audit kv get account/name)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "Ada"' "version fork preserves old value"

scenario_section "space isolation within a branch"
cli_json space create docs >/dev/null
cli_json space create cache >/dev/null
cli_json kv put shared default-value >/dev/null
cli_json_space docs kv put shared docs-value >/dev/null
cli_json_space cache kv put shared cache-value >/dev/null

out="$(cli_json kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "default-value"' "default space shared key"
out="$(cli_json_space docs kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "docs-value"' "docs space shared key"
out="$(cli_json_space cache kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "cache-value"' "cache space shared key"

scenario_section "branch fork inherits spaces but diverges independently"
out="$(cli_json branch fork default feature)"
assert_json "$out" 'data["type"] == "branch" and data["data"]["name"] == "feature"' "fork current"

out="$(cli_json_branch_space feature docs kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "docs-value"' "child inherited docs space"
cli_json_branch_space feature docs kv put shared child-docs-value >/dev/null
cli_json_branch feature space create child-only >/dev/null
cli_json_branch_space feature child-only kv put key child-only-value >/dev/null

out="$(cli_json_space docs kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "docs-value"' "parent docs unchanged"
out="$(cli_json_branch_space feature docs kv get shared)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "child-docs-value"' "child docs changed"
out="$(cli_json space exists child-only)"
assert_json "$out" 'data["type"] == "bool" and data["data"] is False' "parent does not see child space"

scenario_section "space force delete and branch delete do not poison durable reopen"
out="$(cli_json_branch feature space del child-only --force)"
assert_json "$out" 'data["type"] == "space_delete_result" and data["data"]["deleted"] is True' "child space deleted"
out="$(cli_json branch del feature)"
assert_json "$out" 'data["type"] == "branch_delete_result" and data["data"]["deleted"] is True and data["data"]["effect"]["kind"] == "deleted" and data["data"]["branch"]["name"] == "feature" and data["data"]["branch"]["status"] == "deleted"' "feature branch deleted"

out="$(cli_json space create after-delete)"
assert_json "$out" 'data["type"] == "space_create_result" and data["data"]["space"] == "after-delete"' "space create after branch delete"
cli_json_space after-delete kv put durable-check ok >/dev/null
out="$(cli_json_space after-delete kv get durable-check)"
assert_json "$out" 'bytes_to_text(data["data"]["value"]) == "ok"' "post-delete space usable"
