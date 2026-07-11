---
summary: Import an Arrow-compatible file into a product primitive.
mcp_description: Use this when the user wants to load a Parquet, CSV, or JSONL file into KV, JSON, or a vector collection.
---

Imports an Arrow-compatible file (Parquet, CSV, or JSONL) into a product primitive on the selected branch and space. Rows are written through the standard batch commands, so the import commits like any other write. Returns a summary of the target primitive, the input file, and the imported, skipped, and batch counts.
