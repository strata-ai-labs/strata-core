---
summary: Run a bounded breadth-first traversal.
mcp_description: Use this when the user wants to explore outward from a node level by level - reachability, hop distance, or a bounded expansion with depth, node, and edge-type limits.
---

Runs a breadth-first traversal from a start node over a consistent snapshot, bounded by `max_depth` (default 100) and `max_nodes` (default 10000). Returns visited node ids in traversal order, a depth per node, and the tree edges in discovery order. Direction defaults to `outgoing`; an optional edge-type list restricts every hop. The start node must exist (`not_found.engine.graph_node`).
