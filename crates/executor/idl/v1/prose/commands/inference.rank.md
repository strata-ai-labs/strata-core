---
summary: Rank passages against a query.
mcp_description: Use this when the user wants to score or reorder candidate passages by relevance to a query using a reranking model.
---

Scores each candidate passage against a query with a ranking model and returns one outcome per passage. Each item carries the passage's original index and either a relevance score or a per-item error with a stable code and a redacted message, so callers can reorder passages by score while keeping them tied to their inputs. Ranking is a local-only operation: it requires a build with the local execution feature and a ranking-capable model.
