---
summary: Embed one or more texts into vectors.
mcp_description: Use this when the user wants to turn text into embedding vectors with an inference model. The input accepts a single string or an array of strings.
---

Embeds text with an embedding-capable model and returns one vector per input, in order. The `input` field takes either a single string or an array of strings, so single and batch embedding share one command. The vector dimension is fixed by the model. Local embedding models require a build with the local execution feature; cloud embedding providers (OpenAI, Google) require the matching provider feature and an API key.
