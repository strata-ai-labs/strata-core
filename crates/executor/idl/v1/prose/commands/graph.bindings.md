---
summary: Find graph nodes bound to an entity.
mcp_description: Use this when the user wants to find every graph node bound to a specific KV, JSON, vector, event, or graph entity (reverse lookup from entity to nodes). Wire-only; invoke via the generic command runner.
---

Searches every graph in the selected branch and space for nodes whose entity binding matches the given target (primitive, space, key). This is the reverse index of node bindings: given an entity, find the graph facts attached to it. Results paginate by an opaque cursor.
