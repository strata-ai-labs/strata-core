---
summary: Remove a graph edge.
mcp_description: Use this when the user wants to delete one directed edge identified by source, edge type, and destination.
---

Removes one directed edge by its `(src, edge_type, dst)` triple. The endpoints are untouched. Removing an edge that does not exist is not an error: the acknowledgement reports `deleted: false`.
