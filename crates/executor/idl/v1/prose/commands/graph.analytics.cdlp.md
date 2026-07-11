---
summary: Detect communities via label propagation.
mcp_description: Use this when the user wants community detection, graph clustering, or group structure via the community detection label propagation (CDLP) algorithm.
---

Detects communities by label propagation over a consistent snapshot: every node repeatedly adopts the most common label among its neighbors until labels stabilize or the iteration bound (default 10) is reached. Every node maps to its community representative node id. Propagation direction defaults to `both`. Accepts an optional snapshot budget and `as_of` for time travel.
