---
summary: Download an inference model locally.
mcp_description: Use this when the user wants to download or fetch an inference model so it can run locally, offline.
---

Resolves a catalog name or model spec and downloads the model artifact into the local model directory, returning the resolved local path. Honors `STRATA_MODELS_DIR` for the destination and `STRATA_HF_ENDPOINT` and `STRATA_HF_TOKEN` (or `HF_TOKEN`) for gated HuggingFace repositories. Downloading requires network access and a build with the local execution feature; cloud-only builds return `inference.unsupported_operation`.
