---
summary: Unload cached inference models.
mcp_description: Use this when the user wants to free memory by unloading a cached inference model, or all of them.
---

Removes cached model engines from the runtime to free memory. Pass a model spec to unload one entry, or omit it to unload every cached generation, embedding, and ranking model. The result reports whether any cached entry was actually removed. This affects only the in-memory runtime cache; it never deletes downloaded model files from disk.
