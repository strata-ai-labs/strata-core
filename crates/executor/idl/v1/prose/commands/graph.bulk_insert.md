---
summary: Bulk-load nodes and edges in chunks.
mcp_description: Use this when the user wants to load many nodes and edges into a graph at once. Nodes commit before edges in chunked commits; edge endpoints must exist or arrive in the same payload.
---

Ingests a payload of nodes and edges in chunked commits: nodes first, then edges, so edges may reference nodes from the same payload. Node objects use the key `node_id`; edges use `src`, `edge_type`, `dst`, and optional `weight` (default 1.0) and `properties`. `chunk_size` bounds items per commit (default 512, clamped at 800). The acknowledgement reports inserted counts, the number of chunk commits, and the final chunk's commit receipt.
