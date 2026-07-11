---
summary: Set a JSON value at a document path, creating the document when missing.
mcp_description: Use this when the user wants to write, update, or upsert JSON data — a whole document (path `$`) or one nested field.
---

Writes a JSON value at a path inside a document, creating the document and any missing intermediate objects when needed. Setting the root path `$` replaces the whole document; setting a nested path like `$.profile.name` updates one field and records a new document version.
