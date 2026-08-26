---
summary: Preview promoting one branch into another, reporting conflicts without mutating either branch.
mcp_description: Use this when the user wants to see what a merge would do — which entries conflict — before promoting one branch into another.
---

Previews promoting the `source` branch into the `target` branch: it derives the
branch point from the recorded fork lineage, runs a three-way comparison, and
reports the conflicts a promotion would hit — entries both branches changed
differently since the branch point. Preview is read-only: it mutates neither
branch.

Each conflict reports what the selected `strategy` would do — `strict` refuses
(`refused`), `source_wins` overwrites the target with the source value. A preview
with no conflicts is clean and a promotion under `strict` would apply. Preview
covers the capabilities a promotion applies — key-value, JSON, and vectors;
events and graphs are diff-only and never appear as promotion conflicts.
Branches with no shared fork lineage are rejected with
`invalid_argument.engine.branch_point`.
