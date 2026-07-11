---
summary: Read one branch summary by name.
mcp_description: Use this when the user asks about a specific branch's status, lineage, or fork point.
---

Reads the summary for one branch: name, deterministic branch id, generation, status, parent lineage, and logical creation version. A branch that does not exist is a `not_found.engine.branch` error, not an empty result.
