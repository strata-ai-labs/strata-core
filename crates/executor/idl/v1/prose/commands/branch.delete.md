---
summary: Delete an active branch and release its storage claims.
mcp_description: Use this when the user wants to remove a branch. The default branch cannot be deleted.
---

Deletes an active branch and reports the deleted branch summary, generation facts, and storage cleanup counts. The `default` branch refuses deletion with `invalid_argument.engine.branch_delete`. Deletion discards the branch's work — promote anything worth keeping onto another branch with `branch merge` before deleting, or keep working on the branch instead.
