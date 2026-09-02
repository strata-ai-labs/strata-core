---
summary: List yanked refs from the selected StrataHub.
mcp_description: Use this when a frontend needs the hub deny-list or an incremental yanked-ref refresh.
---

Reads `GET /v1/yanked` from the effective hub URL. `since`, when supplied, must be an RFC 3339 timestamp.
