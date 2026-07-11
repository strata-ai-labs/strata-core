---
summary: Report loaded model cache state.
mcp_description: Use this when the user wants to see which inference models are currently loaded in memory.
---

Reports the runtime model cache as three lists of specs: the generation, embedding, and ranking models currently loaded in memory. Use it to check what is resident before generating, or to confirm that `inference unload` freed the models you expected. The lists reflect only in-memory engines, not models available on disk.
