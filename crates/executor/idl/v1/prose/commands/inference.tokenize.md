---
summary: Tokenize text with a local model.
mcp_description: Use this when the user wants to convert text into a local model's token ids, for example to measure prompt length or inspect tokenization.
---

Encodes text into the token id sequence a local model would see and returns the ids in order. Set `add_special` to include the model's special tokens (such as beginning-of-sequence markers). Tokenization is a local-only operation: it requires a build with the local execution feature and returns `inference.unsupported_operation` for cloud provider specs.
