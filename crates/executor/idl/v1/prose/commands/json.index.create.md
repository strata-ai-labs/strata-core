---
summary: Create a JSON secondary index on a field path.
mcp_description: Use this when the user wants to index a JSON field (numeric, tag, or text) for faster lookups.
---

Creates a secondary index over one JSON field path with a numeric, tag, or text kind. Existing documents are indexed at creation and future writes maintain the index automatically. The current wire response is a transitional bare index definition.
