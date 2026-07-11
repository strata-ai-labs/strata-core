---
summary: Read where this database was cloned from.
mcp_description: Use this when the user asks where a database came from or whether it was cloned from a hub dataset.
---

Reads the remote origin recorded when this database was cloned from a hub: the remote URL, dataset, branch, manifest hash, fetch time, and per-branch base frontier. Returns a null origin when the database was created locally and never cloned.
