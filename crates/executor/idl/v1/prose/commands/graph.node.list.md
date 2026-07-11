---
summary: List graph nodes.
mcp_description: Use this when the user wants to enumerate the nodes of a graph, optionally filtered by node-id prefix.
---

Lists a graph's nodes in node-id order. Accepts an optional id prefix filter, an item limit (default 100), an exclusive cursor, and `as_of` for time travel. Each item carries the full node payload: properties, declared type, binding, and commit coordinates.
