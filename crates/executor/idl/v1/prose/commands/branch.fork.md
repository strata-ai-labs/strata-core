---
summary: Fork a new branch from the current head of a source branch.
mcp_description: Use this when the user wants to branch off the latest state of an existing branch.
---

Forks a new branch from the source branch's current head. The new branch sees all data visible on the source at fork time; later writes on either branch stay isolated. The returned branch summary records the parent name, fork version, and generation.

On the CLI, all three fork commands share the single verb `strata branch fork <SOURCE> <BRANCH>`: with no flags it runs this command, while `--version` routes to `branch.fork_at_version` and `--timestamp` routes to `branch.fork_at_timestamp` (both wire-surface commands).
