---
summary: Delete a whole JSON document or one path inside it.
mcp_description: Use this when the user wants to remove a JSON document or delete one nested field from it.
---

Deletes the root path `$` to remove the whole document, or a nested path to remove one field or array element. Missing documents and paths produce a no-op delete acknowledgement rather than an error.
