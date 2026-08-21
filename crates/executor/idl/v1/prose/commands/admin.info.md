---
summary: Read database identity and a catalog summary.
mcp_description: Use this when the user asks what database this is, whether it was just created, or how many branches and spaces it holds.
---

Returns database identity and a catalog summary for one branch: engine version, open target, whether this open created the database, durability, the default branch, the active branch count, the registered space count for the selected branch, and the resolved storage `memory_budget` (its `total_bytes` and its `source` — `explicit`, `derived_from_host`, or `fixed_default`). When no budget is set, the engine derives the default from host memory at open — 25% of usable memory, clamped to a ceiling — so `memory_budget` reveals what the database is actually sized to. The branch defaults to the handle branch when omitted.
