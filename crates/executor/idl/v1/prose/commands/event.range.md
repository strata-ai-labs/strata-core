---
summary: Read a range of events by sequence number.
mcp_description: Use this when the user wants events between two sequence numbers, optionally filtered by event type, in forward or reverse order.
---

Reads events from the selected branch and space by sequence range. The start sequence is inclusive and the optional end sequence is exclusive; reverse direction returns the same `[start_seq, end_seq)` window in descending order (newest first). An optional event type narrows the results.
