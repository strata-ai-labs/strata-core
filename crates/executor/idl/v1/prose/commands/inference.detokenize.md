---
summary: Detokenize token ids with a local model.
mcp_description: Use this when the user wants to turn a local model's token ids back into text.
---

Decodes an ordered list of token ids back into text using a local model's vocabulary, returning the reconstructed string. Detokenization is a local-only operation: it requires a build with the local execution feature and returns `inference.unsupported_operation` for cloud provider specs.
