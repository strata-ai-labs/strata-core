---
summary: Create a product space on a branch.
mcp_description: Use this when the user wants a new named data namespace (space) on a branch.
---

Creates a product space in the branch catalog. Creation is idempotent: creating a space that already exists succeeds with `created: false` and no mutation effect. Names reserved for engine control data are rejected with `invalid_argument.engine.product_space_reserved`.
