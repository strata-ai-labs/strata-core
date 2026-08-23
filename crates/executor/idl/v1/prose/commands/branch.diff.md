---
summary: Compare two branches and report the entities that differ.
mcp_description: Use this when the user wants to see what changed between two branches — which keys and documents were added, removed, or modified.
---

Compares two branches and reports the authored key-value and JSON entities that
differ, grouped by capability and space: entries `added` on `branch_b`,
`removed` relative to `branch_a`, and `modified` on both sides. The comparison
is directional from `branch_a` to `branch_b`. Derived rows are omitted; a
missing branch is rejected with `not_found.engine.branch`.
