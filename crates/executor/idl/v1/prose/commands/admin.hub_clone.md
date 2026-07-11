---
summary: Clone a dataset from a hub into a new local database.
mcp_description: Use this when the user wants to download or clone a published hub dataset into a new local database directory.
---

Clones a dataset from a hub into a new local database directory. Resolution, download, verification, reconstitution, and origin recording run once behind this command; the session database is not touched. The destination directory must not exist or must be empty. When the hub URL is not given, the layered resolver selects it from the flag, environment, and config layers.
