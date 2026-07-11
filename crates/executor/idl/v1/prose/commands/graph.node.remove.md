---
summary: Remove a graph node and its edges.
mcp_description: Use this when the user wants to delete a node from a graph. Incident edges are removed with it.
---

Removes a node and every edge incident to it in one commit. Removing a node that does not exist is not an error: the acknowledgement reports `deleted: false`.
