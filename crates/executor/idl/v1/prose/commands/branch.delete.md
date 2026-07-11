---
summary: Delete an active branch and release its storage claims.
mcp_description: Use this when the user wants to remove a branch. The default branch cannot be deleted.
---

Deletes an active branch and reports the deleted branch summary, generation facts, and storage cleanup counts. The `default` branch refuses deletion with `invalid_argument.engine.branch_delete`. There is no merge in V1: work on a fork is either kept by continuing on that branch or discarded by deleting it.
