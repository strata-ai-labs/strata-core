---
summary: Delete multiple JSON documents or paths in one itemwise batch.
mcp_description: Use this when the user wants to remove several JSON documents or nested fields together.
---

Deletes multiple document/path entries and returns one positional mutation result per entry. Missing documents and paths are represented as no-op item results; applied items share one engine commit.
