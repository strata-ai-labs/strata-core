---
summary: Add or replace a graph edge.
mcp_description: Use this when the user wants to connect two existing nodes with a directed, typed edge, optionally weighted or carrying JSON properties.
---

Adds a directed edge `src -[edge_type]-> dst` or replaces it if the same triple already exists. Both endpoints must already exist; writing an edge to a missing node fails with `invalid_argument.engine.graph_edge_endpoint`. Weight defaults to 1.0 and must not be negative. Once the graph's ontology is frozen, the edge type and its endpoint object types are validated against the declared link types.
