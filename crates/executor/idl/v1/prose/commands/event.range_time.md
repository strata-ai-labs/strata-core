---
summary: Read a range of events by occurrence time.
mcp_description: Use this when the user wants events whose append timestamps fall inside a time window, optionally filtered by event type.
---

Reads events from the selected branch and space whose append timestamps fall inside an inclusive microsecond window. This queries when events occurred; historical log states are the timestamped read commands' job. An optional event type narrows the results.
