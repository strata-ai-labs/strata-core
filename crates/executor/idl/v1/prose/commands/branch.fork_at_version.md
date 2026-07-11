---
summary: Fork a new branch from a retained source commit version.
mcp_description: Use this when the user wants to branch from a specific historical commit version of an existing branch.
---

Forks a new branch anchored at a retained commit version of the source branch, giving time-travel semantics: the new branch sees exactly the data visible at that version. A version outside retained history fails with `history_unavailable.engine.persistence_history`.

This command has no dedicated CLI verb: the CLI expresses it as `strata branch fork <SOURCE> <BRANCH> --version <VERSION>` (one shared `branch fork` verb routes to all three fork commands, so only `branch.fork` owns the CLI path). It remains fully reachable through the generic wire surface — `strata command run`, MCP, and SDKs.
