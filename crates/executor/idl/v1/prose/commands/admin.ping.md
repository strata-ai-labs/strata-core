---
summary: Check that the database handle is live.
mcp_description: Use this for a fast liveness probe that confirms the database responds and reports its engine version.
---

Lightweight liveness check. Returns the engine package version without touching branches, spaces, or primitive data. Use it to confirm the handle is open and responsive before issuing heavier commands.
