---
summary: Compute shortest-path distances from a source.
mcp_description: Use this when the user wants weighted shortest-path distances from one node to every reachable node (single-source shortest path / Dijkstra-style queries).
---

Computes weighted shortest-path distances from a source node over a consistent snapshot. Edge weights (default 1.0) accumulate along paths; unreachable nodes are omitted from the result. Direction defaults to `outgoing`. The source node must exist (`not_found.engine.graph_node`). Accepts an optional snapshot budget and `as_of` for time travel.
