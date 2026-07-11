---
summary: Read lightweight database metrics.
mcp_description: Use this when the user wants quick database metrics such as branch and space counts and the control-plane status.
---

Returns lightweight database metrics: the open target, durability, whether the handle is open, the active branch count, the registered space count for the selected branch, and the control-plane health status. The branch defaults to the handle branch when omitted.
