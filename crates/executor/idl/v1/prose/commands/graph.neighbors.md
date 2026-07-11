---
summary: List a node's neighbors.
mcp_description: Use this when the user wants the nodes adjacent to a given node - who it points to, who points to it, or both - optionally filtered by edge type.
---

Walks a node's edges and returns one hit per traversed edge. Direction is `outgoing`, `incoming`, or `both`; an optional edge-type filter restricts the walk. Each hit embeds both the traversed edge and the neighbor node in full, so a follow-up read is rarely needed. A missing node yields an empty page.
