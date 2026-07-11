---
summary: Append one event to the branch event log.
mcp_description: Use this when the user wants to record, log, or emit a single application event with a type and JSON payload.
---

Appends one event to the selected branch and space. Strata assigns the next sequence number, stamps the event with its append timestamp, and links it into the tamper-evident hash chain. Events are immutable once appended.
