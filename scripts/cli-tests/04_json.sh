#!/usr/bin/env bash
# JSON documents: path writes/reads, nested structures, typed values, document
# lifecycle, history with tombstones, listing/count/sample, secondary indexes,
# and branch isolation for documents.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] document create and path reads"
seed "$DB" json set user:1 '$' '{"name":"Ada","age":36,"tags":["math","code"]}'
expect_out "read the whole document" '{"age":36,"name":"Ada","tags":["math","code"]}' -- "$DB" json get user:1 '$'
expect_out "raw read unwraps to the bare document" '{"age":36,"name":"Ada","tags":["math","code"]}' -- --raw "$DB" json get user:1 '$'
expect_out "read a string path" '"Ada"' -- "$DB" json get user:1 '$.name'
expect_out "raw read unwraps a string leaf" 'Ada' -- --raw "$DB" json get user:1 '$.name'
expect_out "read a number path" "36" -- "$DB" json get user:1 '$.age'
expect_out "read an array element" '"math"' -- "$DB" json get user:1 '$.tags[0]'
expect_out "missing path prints (nil)" "(nil)" -- "$DB" json get user:1 '$.nope'
expect_out "missing document prints (nil)" "(nil)" -- "$DB" json get ghost '$'

echo "[$SUITE_NAME] partial path updates"
expect_contains "path update reports the key" "user:1" -- "$DB" json set user:1 '$.name' '"Grace"'
expect_out "updated path reads back" '"Grace"' -- "$DB" json get user:1 '$.name'
expect_out "sibling fields survive a path update" "36" -- "$DB" json get user:1 '$.age'
seed "$DB" json set user:1 '$.address' '{"city":"Paris","zip":"75001"}'
expect_out "nested object write reads back by path" '"Paris"' -- "$DB" json get user:1 '$.address.city'
seed "$DB" json set user:1 '$.age' 37
expect_out "numeric overwrite reads back" "37" -- "$DB" json get user:1 '$.age'

echo "[$SUITE_NAME] typed values"
seed "$DB" json set types '$' '{}'
seed "$DB" json set types '$.b' true
seed "$DB" json set types '$.n' 'null'
seed "$DB" json set types '$.f' 1.5
seed "$DB" json set types '$.s' plain-text
expect_out "boolean round-trips" "true" -- "$DB" json get types '$.b'
expect_out "explicit null round-trips" "null" -- "$DB" json get types '$.n'
expect_out "float round-trips" "1.5" -- "$DB" json get types '$.f'
expect_out "non-JSON text is stored as a string" '"plain-text"' -- "$DB" json get types '$.s'

echo "[$SUITE_NAME] document lifecycle"
expect_out "exists is true" "true" -- "$DB" json exists user:1
expect_out "exists is false for unknown documents" "false" -- "$DB" json exists ghost
seed "$DB" json set user:2 '$' '{"name":"Katherine"}'
expect_out "list shows both documents" $'types\nuser:1\nuser:2' -- "$DB" json list
expect_out "list honors --prefix" $'user:1\nuser:2' -- "$DB" json list --prefix user:
expect_out "count counts documents" "3" -- "$DB" json count
expect_contains "delete a path removes just that path" "user:1" -- "$DB" json delete user:1 '$.address'
expect_out "deleted path is gone" "(nil)" -- "$DB" json get user:1 '$.address'
expect_out "document survives a path delete" '"Grace"' -- "$DB" json get user:1 '$.name'
expect_contains "delete the root removes the document" "user:2" -- "$DB" json delete user:2 '$'
expect_out "deleted document is gone" "false" -- "$DB" json exists user:2
expect_out "count reflects the delete" "2" -- "$DB" json count

echo "[$SUITE_NAME] history and tombstones"
run --json "$DB" json history user:2
check_ok "history read succeeds"
check_contains "history keeps the pre-delete value" "Katherine" "$OUT"
check_contains "history records the tombstone" '"tombstone":true' "$OUT"

echo "[$SUITE_NAME] sampling"
expect_json "sample reports total count" '["data"]["total_count"]' 1 -- "$DB" json sample --prefix user: --count 1

echo "[$SUITE_NAME] secondary indexes"
expect_ok "create a tag index" -- "$DB" json index create by-name '$.name'
expect_ok "create a numeric index" -- "$DB" json index create by-age '$.age' --index-type numeric
run "$DB" json index list
check_contains "index list shows by-name" "by-name" "$OUT"
check_contains "index list shows by-age" "by-age" "$OUT"
expect_fail "duplicate index names are rejected" "" -- "$DB" json index create by-name '$.other'
expect_ok "drop an index" -- "$DB" json index drop by-age
run "$DB" json index list
if [[ "$OUT" == *"by-age"* ]]; then
  _fail "dropped index no longer listed" "by-age still present in: $OUT"
else
  _ok
fi

echo "[$SUITE_NAME] documents are branch-isolated"
seed "$DB" branch fork default docs-fork
seed "$DB" json set user:1 '$.name' '"Fork-Only"' --branch docs-fork
expect_out "fork reads its own document state" '"Fork-Only"' -- "$DB" json get user:1 '$.name' --branch docs-fork
expect_out "parent document is untouched" '"Grace"' -- "$DB" json get user:1 '$.name'

finish
