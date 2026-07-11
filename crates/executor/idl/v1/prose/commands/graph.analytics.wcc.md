---
summary: Compute weakly connected components.
mcp_description: Use this when the user wants to find connected components, clusters of mutually reachable nodes, or check whether a graph is fully connected. Reachable via the generic command runner or the CLI verb `graph wcc`.
---

Computes weakly connected components over a consistent snapshot of the graph, ignoring edge direction. Every node maps to its component representative - the smallest node id in its component - and the response carries the distinct component count. Accepts an optional snapshot budget and `as_of` for time travel.
