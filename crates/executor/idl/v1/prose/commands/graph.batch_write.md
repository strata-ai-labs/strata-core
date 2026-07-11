---
summary: Apply graph mutations atomically.
mcp_description: Use this when the user wants several graph mutations (node/edge upserts and deletes) applied together in one atomic commit - all succeed or none do. Wire-only; invoke via the generic command runner.
---

Applies a list of graph operations - `upsert_node`, `delete_node`, `upsert_edge`, `delete_edge` - in one engine commit. Validation failures (bad ids, missing edge endpoints, frozen-ontology violations) reject the whole batch; nothing is partially applied. The response reports one positional item result per operation, all sharing the same commit receipt.
