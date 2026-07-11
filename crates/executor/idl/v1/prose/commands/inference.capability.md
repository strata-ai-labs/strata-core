---
summary: Report capabilities for a model spec.
mcp_description: Use this when the user wants to know what a model spec can do — generate, tokenize, embed, or rank — and whether it needs the network, an API key, or a compiled provider feature.
---

Parses a model spec into a provider and model name and reports what that combination supports without running the model. The result states whether generation, tokenization, embedding, and ranking are available, whether the operation requires network access or an API key, whether this binary was compiled with the provider feature needed to execute, whether the runtime currently permits network calls, and the known embedding dimension. Model specs are catalog names (`tinyllama`), catalog `name:quant` pairs (`tinyllama:q8_0`), local GGUF paths, or provider specs (`anthropic:claude-...`).
