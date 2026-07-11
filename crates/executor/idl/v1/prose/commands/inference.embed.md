---
summary: Embed one text into a vector.
mcp_description: Use this when the user wants to turn a single piece of text into an embedding vector with an inference model.
---

Embeds one text with an embedding-capable model and returns the resulting vector. The vector's dimension is fixed by the model. Local embedding models require a build with the local execution feature; cloud embedding providers (OpenAI, Google) require the matching provider feature and an API key. To embed several texts in one call, use `inference embed-batch`.
