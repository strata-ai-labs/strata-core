---
summary: Set multiple JSON values in one itemwise batch.
mcp_description: Use this when the user wants to write several JSON documents or fields together in one commit.
---

Writes multiple document/path/value entries using the executor batch contract. Valid items share one engine commit; entries targeting the same document are merged in order into a single new document version.
