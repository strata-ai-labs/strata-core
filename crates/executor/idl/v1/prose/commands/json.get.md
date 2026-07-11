---
summary: Read the current or historical JSON value at a document path.
mcp_description: Use this when the user wants to fetch a JSON document or one nested field by key and path.
---

Reads the JSON value at a path inside a document. Current reads return the value with commit metadata; passing a timestamp returns the bare value visible at that point in time. A missing document or path is a found-false result, distinct from a stored JSON null.
