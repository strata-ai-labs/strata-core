---
summary: Compute local clustering coefficients.
mcp_description: Use this when the user wants per-node clustering coefficients - how densely each node's neighbors connect to each other (triangle density).
---

Computes the local clustering coefficient for every node over a consistent snapshot: the fraction of a node's neighbor pairs that are themselves connected. Nodes in fully-triangulated neighborhoods score 1.0. Accepts an optional snapshot budget and `as_of` for time travel.
