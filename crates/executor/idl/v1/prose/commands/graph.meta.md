---
summary: Read graph metadata and counts.
mcp_description: Use this when the user wants a graph's node count, edge count, or creation and update commit coordinates. Returns null if the graph does not exist.
---

Reads a graph's metadata: live node and edge counts plus the create and last-update commit versions and timestamps. Reading a graph that does not exist returns no data rather than an error. Accepts `as_of` for time travel.
