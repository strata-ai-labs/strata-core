---
summary: Read one graph node.
mcp_description: Use this when the user wants to read a node's properties, declared type, or binding by node id. Returns null for a missing node.
---

Reads one node by id, returning its properties, declared object type, entity binding, and last-write commit coordinates. A removed or never-written node reads back as no data. Accepts `as_of` for time travel.
