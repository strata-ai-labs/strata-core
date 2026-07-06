#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

new_db "event-graph-workflows"

scenario_section "event sequence, type, time, and branch reads"
out="$(cli_json event append user.created '{"user":"ada","plan":"free"}')"
assert_json "$out" 'data["type"] == "event_append_result" and data["data"]["sequence"] == 0' "event append first"
first_ts="$(json_value "$out" 'data["data"]["timestamp"]')"
cli_json event append user.updated '{"user":"ada","plan":"pro"}' >/dev/null
cli_json event append system.audit '{"action":"upgrade"}' >/dev/null

out="$(cli_json event list --limit 2)"
assert_json "$out" 'data["type"] == "event_records" and data["data"]["has_more"] is True and data["data"]["cursor"] == 1 and [item["event"]["sequence"] for item in data["data"]["items"]] == [0, 1]' "event list first page"
out="$(cli_json event list --limit 2 --cursor 1)"
assert_json "$out" 'data["type"] == "event_records" and data["data"]["has_more"] is False and data["data"]["cursor"] is None and [item["event"]["sequence"] for item in data["data"]["items"]] == [2]' "event list terminal page"

out="$(cli_json event by-type user.updated --limit 5)"
assert_json "$out" 'data["type"] == "event_records" and len(data["data"]["items"]) == 1 and data["data"]["items"][0]["event"]["event_type"] == "user.updated"' "event by type"

out="$(cli_json event range 2 --direction reverse --limit 2)"
assert_json "$out" 'data["type"] == "event_range_result" and [item["event"]["sequence"] for item in data["data"]["items"]] == [2, 1]' "event reverse range"

out="$(cli_json event range-time "$first_ts" --limit 10)"
assert_json "$out" 'data["type"] == "event_range_result" and len(data["data"]["items"]) >= 1' "event range by time"

out="$(cli_json event verify-chain)"
assert_json "$out" 'data["type"] == "event_chain_verification" and data["data"]["valid"] is True and "is_valid" not in data["data"]' "event chain valid"

cli_json branch fork default event-child >/dev/null
cli_json_branch event-child event append child.only '{"branch":"child"}' >/dev/null
out="$(cli_json event len)"
assert_json "$out" 'data["type"] == "event_length" and data["data"]["count"] == 3' "parent event length unchanged"
out="$(cli_json_branch event-child event len)"
assert_json "$out" 'data["type"] == "event_length" and data["data"]["count"] == 4' "child event length includes child append"

scenario_section "graph pagination, neighbor direction, and branch divergence"
out="$(cli_json graph create social)"
assert_json "$out" 'data["type"] == "graph_info" and data["data"]["graph"] == "social"' "graph create"
for node in ada bob carol dave erin; do
  cli_json graph add-node social "$node" --properties "{\"kind\":\"person\",\"name\":\"$node\"}" >/dev/null
done
cli_json graph add-edge social ada follows bob --weight 0.4 --properties '{"since":2024}' >/dev/null
cli_json graph add-edge social carol follows ada --weight 0.9 --properties '{"since":2025}' >/dev/null
cli_json graph add-edge social ada mentors dave --weight 1.0 --properties '{"since":2026}' >/dev/null

out="$(cli_json graph list-nodes social --limit 2)"
assert_json "$out" 'data["type"] == "graph_node_page" and data["data"]["has_more"] is True and [item["node_id"] for item in data["data"]["items"]] == ["ada", "bob"]' "graph first node page"
out="$(cli_json graph list-nodes social --limit 2 --cursor bob)"
assert_json "$out" 'data["type"] == "graph_node_page" and [item["node_id"] for item in data["data"]["items"]] == ["carol", "dave"]' "graph second node page"

out="$(cli_json graph neighbors social ada --direction outgoing --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and sorted(item["node"]["node_id"] for item in data["data"]["items"]) == ["bob", "dave"]' "graph outgoing neighbors"
out="$(cli_json graph neighbors social ada --direction incoming --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and [item["node"]["node_id"] for item in data["data"]["items"]] == ["carol"]' "graph incoming neighbors"
out="$(cli_json graph neighbors social ada --edge-type mentors --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and [item["node"]["node_id"] for item in data["data"]["items"]] == ["dave"]' "graph edge-type neighbors"

cli_json branch fork default graph-child >/dev/null
cli_json_branch graph-child graph remove-edge social ada follows bob >/dev/null
cli_json_branch graph-child graph add-node social frank --properties '{"kind":"person"}' >/dev/null
cli_json_branch graph-child graph add-edge social ada follows frank --weight 0.8 >/dev/null

out="$(cli_json graph neighbors social ada --direction outgoing --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and sorted(item["node"]["node_id"] for item in data["data"]["items"]) == ["bob", "dave"]' "parent graph unchanged"
out="$(cli_json_branch graph-child graph neighbors social ada --direction outgoing --limit 10)"
assert_json "$out" 'data["type"] == "graph_neighbor_page" and sorted(item["node"]["node_id"] for item in data["data"]["items"]) == ["dave", "frank"]' "child graph diverged"

graph_batch="$CLI_CORPUS_FILES/graph-corpus-batch.json"
write_json "$graph_batch" '{"type":"graph_batch_write","graph":"social","operations":[{"type":"upsert_node","node_id":"gina","data":{"properties":{"kind":"person"}}},{"type":"upsert_node","node_id":"hank","data":{"properties":{"kind":"person"}}},{"type":"upsert_edge","src":"gina","edge_type":"knows","dst":"hank","data":{"weight":0.5,"properties":{"via":"batch"}}}]}'
out="$(raw_command_file "$graph_batch")"
assert_json "$out" 'data["type"] == "graph_batch_write_result" and data["data"]["mode"] == "atomic" and data["data"]["status"] == "ok" and len(data["data"]["items"]) == 3' "graph batch write"
