---
summary: Apply graph mutations atomically.
mcp_description: Use this when the user wants several graph mutations (node/edge upserts and deletes) applied together in one atomic commit - all succeed or none do. Wire-only; invoke via the generic command runner.
---

Applies a list of graph operations - `upsert_node`, `delete_node`, `upsert_edge`, `delete_edge` - in one engine commit. Validation failures (bad ids, missing edge endpoints, frozen-ontology violations) reject the whole batch; nothing is partially applied. The response reports one positional item result per operation, all sharing the same commit receipt.

This whole-batch atomicity is deliberate, and differs from the itemwise `kv`, `json`, and `event` batch writes (where the valid items of a mixed batch still commit and the invalid one carries a typed per-item error). A graph batch mixes node and edge upserts, and an edge references its endpoint nodes, so applying only the valid items could leave an edge pointing at a node that never landed. Rejecting the whole batch keeps graph references consistent - referential integrity is the reason the graph channel is atomic where its siblings are itemwise.
