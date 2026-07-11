---
summary: Read one graph edge.
mcp_description: Use this when the user wants to read an edge's weight or properties by its source, edge type, and destination. Returns null for a missing edge.
---

Reads one edge by its `(src, edge_type, dst)` triple, returning its weight, properties, and last-write commit coordinates. A missing edge reads back as no data. Accepts `as_of` for time travel.
