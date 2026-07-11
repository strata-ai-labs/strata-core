---
summary: Verify event log density and hash linkage.
mcp_description: Use this when the user wants to audit the event log's integrity by checking sequence density and the hash chain.
---

Verifies that the visible event log in the selected branch and space is dense and hash-linked: sequences are contiguous from zero, the genesis record links to the all-zeros hash, and every record's hash matches its content and predecessor.
