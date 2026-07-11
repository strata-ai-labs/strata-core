---
summary: List locally downloaded inference models.
mcp_description: Use this when the user wants to know which inference models are already downloaded and ready to run offline.
---

Lists the catalog models that have at least one quantization variant present in the local model directory, as a terminal page. Entries carry the same facts as `inference models list` but are restricted to models that can run without a further download. The local model directory is resolved from `STRATA_MODELS_DIR`, falling back to `~/.strata/models`.
