---
summary: Delete a product space from a branch.
mcp_description: Use this when the user wants to remove a space, optionally force-deleting its data.
---

Drops the product space from the branch catalog. The `default` space refuses deletion with `invalid_argument.engine.space_delete_default`. A space that still contains visible data refuses deletion with `failed_precondition.engine.space_not_empty` unless `force: true` is set, which tombstones the visible rows first and reports the count. Deleting a space that does not exist succeeds with `deleted: false`.
