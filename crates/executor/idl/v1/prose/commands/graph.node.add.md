---
summary: Add or replace a graph node.
mcp_description: Use this when the user wants to insert or upsert a node in a graph, optionally with JSON properties, a declared object type, or a binding to a KV/JSON/vector/event entity.
---

Adds a node to a graph or replaces it if the node id already exists. A node carries optional JSON properties, an optional declared object type (validated once the graph's ontology is frozen), and an optional entity binding that links the node to a row in another primitive. Cross-branch bindings are rejected.
