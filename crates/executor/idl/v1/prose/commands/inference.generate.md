---
summary: Generate text with an inference model.
mcp_description: Use this when the user wants an inference model to produce or complete text from a prompt, with control over token budget, sampling, stop conditions, and grammar.
---

Runs a text-generation request against a local or cloud model and returns the completion, the reason generation stopped, and the provider-reported prompt and completion token counts. The request controls the maximum completion tokens, sampling temperature, top-k and top-p cutoffs, an optional deterministic seed, string and token-id stop sequences, and an optional GBNF grammar for constrained generation. Chat models expect their chat template already applied in the prompt. Local models require a build with the local execution feature; cloud providers require the matching provider feature and an API key.
