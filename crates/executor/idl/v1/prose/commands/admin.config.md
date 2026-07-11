---
summary: Read sanitized configuration facts.
mcp_description: Use this when the user asks how the database is configured — its open target, durability, and default branch.
---

Returns sanitized configuration facts: the open target, whether this open created the database, durability, and the default branch. Only allowlisted, non-sensitive facts are exposed; no filesystem paths, credentials, or provider keys are ever returned.
