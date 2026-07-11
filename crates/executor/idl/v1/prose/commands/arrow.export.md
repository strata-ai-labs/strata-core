---
summary: Export a product primitive to an Arrow-compatible file.
mcp_description: Use this when the user wants to write KV, JSON, event, vector, or graph data out to a Parquet, CSV, or JSONL file.
---

Exports a product primitive from the selected branch and space to an Arrow-compatible file (Parquet, CSV, or JSONL). Graph exports treat the path as a stem and write separate node and edge files. Returns a summary of the exported primitive, the concrete output paths, the row count, and the total output size.
