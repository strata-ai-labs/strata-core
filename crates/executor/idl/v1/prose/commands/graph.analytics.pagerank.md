---
summary: Compute PageRank importance scores.
mcp_description: Use this when the user wants node importance, influence ranking, or personalized PageRank with seed weights.
---

Computes PageRank over a consistent snapshot. Tunable damping (default 0.85), iteration bound (default 20), and convergence tolerance (default 1e-6); the response reports how many iterations actually ran. Optional personalization seeds steer both teleport and dangling mass toward weighted nodes, and the response flags `personalized: true`. Results are deterministic for a fixed graph state. Accepts an optional snapshot budget and `as_of` for time travel.
