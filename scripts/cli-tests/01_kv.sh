#!/usr/bin/env bash
# KV primitive: write/read/delete lifecycle, listing, scanning, pagination
# cursors, history, sampling, counting, binary values, and file/stdin input.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] basic write/read lifecycle"
expect_contains "put reports created" "created alpha applied=true" -- "$DB" kv put alpha one
expect_out "get returns the value" "one" -- "$DB" kv get alpha
expect_out "raw get returns the bare value" "one" -- --raw "$DB" kv get alpha
expect_contains "overwrite reports updated" "updated alpha applied=true" -- "$DB" kv put alpha uno
expect_out "get returns the overwritten value" "uno" -- "$DB" kv get alpha
expect_out "exists is true" "true" -- "$DB" kv exists alpha
expect_out "exists is false for a missing key" "false" -- "$DB" kv exists missing
expect_out "get of a missing key prints (nil)" "(nil)" -- "$DB" kv get missing
expect_contains "delete reports deleted" "deleted alpha applied=true" -- "$DB" kv delete alpha
expect_out "get after delete prints (nil)" "(nil)" -- "$DB" kv get alpha
expect_contains "deleting a missing key reports not_found" "not_found" -- "$DB" kv delete alpha

echo "[$SUITE_NAME] listing, counting, prefixes"
seed "$DB" kv put app:1 first
seed "$DB" kv put app:2 second
seed "$DB" kv put web:1 third
expect_out "list returns all keys sorted" $'app:1\napp:2\nweb:1' -- "$DB" kv list
expect_out "list honors --prefix" $'app:1\napp:2' -- "$DB" kv list --prefix app:
expect_out "count counts all keys" "3" -- "$DB" kv count
expect_out "count honors --prefix" "2" -- "$DB" kv count --prefix app:
expect_out "count is 0 for an unused prefix" "0" -- "$DB" kv count --prefix nope

echo "[$SUITE_NAME] cursor pagination round-trip (list)"
run "$DB" kv list --limit 2
check_contains "first page shows a continuation cursor" "-- more: " "$OUT"
cursor="$(sed -n 's/^-- more: //p' <<<"$OUT")"
expect_out "printed cursor continues to the last page" "web:1" -- "$DB" kv list --limit 2 --cursor "$cursor"
expect_fail "garbage cursor is a usage error" "invalid --cursor" -- "$DB" kv list --cursor 'not!base64'

echo "[$SUITE_NAME] cursor pagination round-trip (scan)"
run "$DB" kv scan --limit 1
check_contains "scan first row is app:1" '"key":"app:1"' "$OUT"
check_contains "scan shows a continuation cursor" "-- more: " "$OUT"
cursor="$(sed -n 's/^-- more: //p' <<<"$OUT")"
expect_contains "scan cursor continues at the next row" '"key":"app:2"' -- "$DB" kv scan --limit 1 --cursor "$cursor"
expect_fail "scan rejects --start with --cursor" "cannot be used with" -- "$DB" kv scan --start a --cursor "$cursor"
expect_contains "scan --start seeks inclusively" '"key":"web:1"' -- "$DB" kv scan --start web:

echo "[$SUITE_NAME] version history"
seed "$DB" kv put story draft
seed "$DB" kv put story final
seed "$DB" kv delete story
run "$DB" kv history story
check_contains "history keeps the first version" '"value":"draft"' "$OUT"
check_contains "history keeps the second version" '"value":"final"' "$OUT"
check_contains "history records the delete tombstone" '"tombstone":true' "$OUT"
expect_out "history of an unknown key prints (nil)" "(nil)" -- "$DB" kv history never-written

echo "[$SUITE_NAME] sampling"
run "$DB" kv sample --prefix app: --count 1
check_contains "sample returns a row from the prefix" '"key":"app:' "$OUT"
expect_json "sample reports the total matching count" '["data"]["total_count"]' 2 -- "$DB" kv sample --prefix app: --count 1

echo "[$SUITE_NAME] binary and file-based values"
printf '\xff\xfe' >"$WORK_DIR/binary.bin"
expect_ok "put accepts a non-UTF8 --file value" -- "$DB" kv put binkey --file "$WORK_DIR/binary.bin"
expect_out "non-UTF8 value renders as base64" "//4=" -- "$DB" kv get binkey
expect_json "json format is wire-true base64" '["data"]["value"]' "//4=" -- "$DB" kv get binkey
printf 'from-a-file' >"$WORK_DIR/value.txt"
expect_ok "put accepts @file shorthand" -- "$DB" kv put filekey "@$WORK_DIR/value.txt"
expect_out "value read from @file round-trips" "from-a-file" -- "$DB" kv get filekey
if printf 'from-stdin' | "$STRATA_BIN" "$DB" kv put stdinkey - >/dev/null 2>&1; then _ok; else _fail "put accepts stdin via -"; fi
expect_out "value read from stdin round-trips" "from-stdin" -- "$DB" kv get stdinkey

echo "[$SUITE_NAME] structured output facts"
expect_json "put reports a commit version" '["data"]["commit"]["put_count"]' 1 -- "$DB" kv put wired yes
expect_json "get reports the row version fact" '["type"]' "kv_versioned_value" -- "$DB" kv get wired

finish
