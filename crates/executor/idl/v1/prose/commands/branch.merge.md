---
summary: Promote one branch's changes into another as a single atomic commit.
mcp_description: Use this when the user wants to merge or promote one branch into another — applying the source branch's changes to the target.
---

Promotes the `source` branch's changes into the `target` branch as a single
atomic commit, leaving the source unchanged. The branch point is derived from
the recorded fork lineage, and a three-way merge applies every change the source
made since that point.

Merge applies to key-value, JSON, and vector data. Event streams and graphs are
compared (see `branch.diff`) but never merged — divergent append-only and
structural data cannot be three-way merged — so a promotion leaves them
untouched.

The `strict` strategy (the default) refuses with `conflict.engine.promotion`,
mutating nothing, when the two branches changed the same entity differently since
the branch point. The `source_wins` strategy applies the source side's value or
tombstone for each such conflict and reports every overwritten or deleted target
entry. A promotion that applies nothing writes no commit and leaves the target
unchanged.

Branches with no shared fork lineage are rejected with
`invalid_argument.engine.branch_point`; a missing branch with
`not_found.engine.branch`.
