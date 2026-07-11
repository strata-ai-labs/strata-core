---
summary: Read control-plane health facts.
mcp_description: Use this when the user asks whether the database is healthy or wants to diagnose a degraded or unavailable control plane.
---

Returns control-plane health facts: the worst overall status plus per-subsystem status for identity, registry, branch catalog, and the optional branch-local space catalog. Also reports the default branch and active branch count. A healthy result means every required control-plane fact is present and readable.
