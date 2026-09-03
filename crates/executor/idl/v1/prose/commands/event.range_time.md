---
summary: Read a range of events by occurrence time.
mcp_description: Use this when the user wants events whose append timestamps fall inside a time window, optionally filtered by event type.
---

Reads events from the selected branch and space whose append timestamps fall inside a half-open `[start_ts, end_ts)` microsecond window — the start is inclusive and the end is exclusive, matching the sequence-addressed range. This queries when events occurred; historical log states are the timestamped read commands' job. An optional event type narrows the results.
