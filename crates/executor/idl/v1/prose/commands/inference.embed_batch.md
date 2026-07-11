---
summary: Embed multiple texts into vectors.
mcp_description: Use this when the user wants to embed several texts at once and get one vector per input, preserving order.
---

Embeds an ordered list of texts with an embedding-capable model and returns one outcome per input in the same order, alongside the embedding dimension. Each item is either a successful vector or a per-item error carrying a stable code and a redacted message, so one bad input does not fail the whole batch. Local embedding models require a build with the local execution feature; cloud embedding providers (OpenAI, Google) require the matching provider feature and an API key.
