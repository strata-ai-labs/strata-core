#!/usr/bin/env bash
# Event log: append-only sequencing, reads, ranges (forward/reverse/time),
# type filtering, chain verification, and branch isolation of the log.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] append assigns dense sequences"
expect_json "first append is sequence 0" '["data"]["sequence"]' 0 -- "$DB" event append user.created '{"id":1}'
expect_json "second append is sequence 1" '["data"]["sequence"]' 1 -- "$DB" event append user.created '{"id":2}'
expect_json "third append is sequence 2" '["data"]["sequence"]' 2 -- "$DB" event append order.placed '{"total":9.5}'
expect_out "len counts all events" "3" -- "$DB" event len

echo "[$SUITE_NAME] point reads"
expect_json "get returns the payload" '["data"]["event"]["payload"]["id"]' 2 -- "$DB" event get 1
expect_json "get returns the event type" '["data"]["event"]["event_type"]' "user.created" -- "$DB" event get 1
expect_json "events are hash-chained" '["data"]["event"]["previous_hash"].__len__()' 64 -- "$DB" event get 1
expect_out "get of an unknown sequence prints (nil)" "(nil)" -- "$DB" event get 99
expect_out "exists is true" "true" -- "$DB" event exists 2
expect_out "exists is false beyond the head" "false" -- "$DB" event exists 99

echo "[$SUITE_NAME] ranges"
run --json "$DB" event range 0 --limit 2
check_ok "forward range succeeds"
check_eq "forward range respects the limit" 2 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"
run --json "$DB" event range 2 --direction reverse --limit 10
check_ok "reverse range succeeds"
check_eq "reverse range walks backward from the anchor" 0 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["data"]["items"][-1]["event"]["sequence"])' <<<"$OUT")"
run --json "$DB" event range 1 --end-seq 2
check_eq "end-seq is exclusive" 1 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] type filtering"
run "$DB" event types
check_ok "types listing succeeds"
check_contains "types include user.created" "user.created" "$OUT"
check_contains "types include order.placed" "order.placed" "$OUT"
run --json "$DB" event by-type user.created
check_eq "by-type returns only matching events" 2 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"
run --json "$DB" event list
check_ok "event list succeeds"
check_eq "event list sees the whole log" 3 "$(python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d["data"]["items"]))' <<<"$OUT")"

echo "[$SUITE_NAME] chain verification"
run --json "$DB" event verify-chain
check_ok "verify-chain succeeds"
check_contains "chain verification reports validity" '"valid":true' "$OUT"

echo "[$SUITE_NAME] the log is branch-isolated"
seed "$DB" branch fork default audit
seed "$DB" event append fork.only '{"where":"audit"}' --branch audit
expect_out "fork sees inherited plus its own events" "4" -- "$DB" event len --branch audit
expect_out "parent log is unchanged" "3" -- "$DB" event len
seed "$DB" event append parent.only '{}'
expect_out "post-fork parent appends stay off the fork" "4" -- "$DB" event len --branch audit

finish
