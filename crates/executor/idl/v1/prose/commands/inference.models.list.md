---
summary: List catalog inference models.
mcp_description: Use this when the user wants to see which inference models Strata knows about, including their task, embedding dimension, and whether each is already downloaded.
---

Lists every model in Strata's built-in catalog as a terminal page. Each entry reports the model's task (embed, generate, or rank), architecture, default quantization, embedding dimension, HuggingFace repository, approximate artifact size, and whether the model artifact is already present in the local model directory. Use `inference models local` to see only the downloaded models, or `inference models pull` to fetch one.
