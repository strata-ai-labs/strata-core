#!/usr/bin/env bash
# Vector primitive: collection lifecycle, upsert/get/delete, similarity query
# ordering, metadata filtering and patching, bulk deletes, key listing, and
# dimension/collection guards.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

# The documented VectorMetadataFilter wire shape (AND-composed, tagged scalars).
UNIT_FILTER='{"conditions":[{"field":"kind","op":"eq","value":{"type":"string","value":"unit"}}]}'

echo "[$SUITE_NAME] collection lifecycle"
expect_ok "create a cosine collection" -- "$DB" vector collection create docs 3
expect_ok "create a euclidean collection" -- "$DB" vector collection create geo 2 --metric euclidean
run "$DB" vector collection list
check_ok "collection list succeeds"
check_contains "collection list shows docs" "docs" "$OUT"
check_contains "collection list shows geo" "geo" "$OUT"
# Stats currently answer with the transitional collection-list wire shape
# (declared in the IDL for V1).
expect_json "stats report the dimension" '["data"]["items"][0]["dimension"]' 3 -- "$DB" vector collection stats docs
expect_json "stats report the metric" '["data"]["items"][0]["metric"]' "cosine" -- "$DB" vector collection stats docs
expect_fail "duplicate collection creation fails" "" -- "$DB" vector collection create docs 3
expect_fail "upsert into a missing collection fails" "" -- "$DB" vector upsert ghost k 1,2,3

echo "[$SUITE_NAME] upsert and read"
expect_ok "upsert comma floats" -- "$DB" vector upsert docs a 1,0,0
expect_ok "upsert JSON array with metadata" -- "$DB" vector upsert docs b '[0,1,0]' --metadata '{"kind":"unit","rank":2}'
expect_ok "upsert third vector" -- "$DB" vector upsert docs c '[0.9,0.1,0]'
expect_json "get returns the embedding" '["data"]["data"]["embedding"][1]' 1.0 -- "$DB" vector get docs b
expect_json "get returns metadata" '["data"]["data"]["metadata"]["kind"]' "unit" -- "$DB" vector get docs b
expect_out "get of a missing key prints (nil)" "(nil)" -- "$DB" vector get docs ghost
expect_out "exists is true" "true" -- "$DB" vector exists docs a
expect_out "exists is false for missing keys" "false" -- "$DB" vector exists docs ghost
expect_out "count counts vectors" "3" -- "$DB" vector count docs

echo "[$SUITE_NAME] similarity query ordering"
run "$DB" vector query docs 1,0,0 -k 2
check_ok "query succeeds"
first="$(sed -n '1p' <<<"$OUT" | cut -f1)"
second="$(sed -n '2p' <<<"$OUT" | cut -f1)"
check_eq "nearest match is the identical vector" "a" "$first"
check_eq "second match is the closest neighbor" "c" "$second"
run "$DB" vector query docs 1,0,0 -k 10
check_eq "k caps at available vectors" 3 "$(wc -l <<<"$OUT")"

echo "[$SUITE_NAME] metadata filters"
expect_ok "upsert another tagged vector" -- "$DB" vector upsert docs d '[0.8,0.2,0]' --metadata '{"kind":"unit","rank":9}'
run "$DB" vector query docs 1,0,0 -k 10 --filter "$UNIT_FILTER"
check_ok "filtered query succeeds"
check_contains "filtered query returns tagged key b" "b" "$OUT"
check_contains "filtered query returns tagged key d" "d" "$OUT"
if [[ "$OUT" == *$'a\t'* || "$OUT" == *$'c\t'* ]]; then
  _fail "filter excludes untagged keys" "untagged key leaked into: $OUT"
else
  _ok
fi
run "$DB" vector query docs 1,0,0 -k 10 --filter '{"conditions":[]}'
check_ok "empty-conditions filter succeeds"
check_eq "empty-conditions filter matches everything (vacuous AND)" 4 "$(wc -l <<<"$OUT")"
expect_fail "a bare condition object is rejected (missing conditions)" "conditions" \
  -- "$DB" vector query docs 1,0,0 --filter '{"field":"kind","op":"eq","value":"unit"}'
expect_fail "untagged scalar values are rejected" "" \
  -- "$DB" vector query docs 1,0,0 --filter '{"conditions":[{"field":"kind","op":"eq","value":"unit"}]}'

echo "[$SUITE_NAME] metadata patching"
expect_ok "update metadata" -- "$DB" vector update-metadata docs b '{"kind":"unit","rank":5}'
expect_json "patched metadata reads back" '["data"]["data"]["metadata"]["rank"]' 5 -- "$DB" vector get docs b

echo "[$SUITE_NAME] key listing"
expect_out "keys lists all keys sorted" $'a\nb\nc\nd' -- "$DB" vector keys docs
run "$DB" vector keys docs --limit 2
check_contains "keys paginates with a cursor" "-- more: " "$OUT"

echo "[$SUITE_NAME] history and deletes"
seed "$DB" vector upsert docs a 0.5,0.5,0
expect_json "history keeps both revisions" '["data"]["count"]' 2 -- "$DB" vector history docs a
expect_contains "delete one vector" "deleted" -- "$DB" vector delete docs d
expect_out "deleted vector is gone" "false" -- "$DB" vector exists docs d
expect_ok "delete by metadata filter" -- "$DB" vector delete-by-filter docs --filter "$UNIT_FILTER"
expect_out "filter delete removed the tagged vector" "false" -- "$DB" vector exists docs b
expect_out "count reflects deletes" "2" -- "$DB" vector count docs
expect_ok "delete all" -- "$DB" vector delete-all docs
expect_out "collection is empty after delete-all" "0" -- "$DB" vector count docs

echo "[$SUITE_NAME] dimension guard"
expect_fail "wrong-dimension upsert is rejected" "" -- "$DB" vector upsert docs bad 1,2
expect_fail "wrong-dimension query is rejected" "" -- "$DB" vector query docs 1,2

echo "[$SUITE_NAME] vectors are branch-isolated"
seed "$DB" vector upsert docs base 1,0,0
seed "$DB" branch fork default vec-fork
seed "$DB" vector upsert docs base 0,0,1 --branch vec-fork
expect_json "fork reads its own embedding" '["data"]["data"]["embedding"][2]' 1.0 -- "$DB" vector get docs base --branch vec-fork
expect_json "parent embedding is untouched" '["data"]["data"]["embedding"][0]' 1.0 -- "$DB" vector get docs base

echo "[$SUITE_NAME] collection deletion"
expect_ok "delete a collection" -- "$DB" vector collection delete geo
run "$DB" vector collection list
if [[ "$OUT" == *"geo"* ]]; then
  _fail "deleted collection no longer listed" "geo still present in: $OUT"
else
  _ok
fi

finish
