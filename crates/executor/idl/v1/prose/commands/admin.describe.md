---
summary: Read a compact description of the database.
mcp_description: Use this when the user wants an overview of a database — its branches, spaces, primitive counts, and available capabilities.
---

Returns a compact description of the database for one branch: engine version, open target, the default and described branches, all active branches, the registered product spaces, per-primitive counts (KV, JSON, event, plus vector-collection and graph summaries), the sanitized config, and the available capabilities. The branch defaults to the handle branch when omitted.
