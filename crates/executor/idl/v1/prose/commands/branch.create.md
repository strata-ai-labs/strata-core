---
summary: Create a new empty root branch.
mcp_description: Use this when the user wants a fresh branch that shares no history with any existing branch.
---

Creates an empty root branch with no parent and no data. This is not a fork: the new branch starts from nothing, and its `parent` is null. Use `branch.fork` to start from an existing branch's data. Creating a name that already exists fails with `already_exists.engine.branch`; names reserved for engine control data are rejected.
