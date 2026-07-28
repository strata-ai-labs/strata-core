---
summary: Stop hosting the multi-process broker socket.
mcp_description: Use this when the user wants to stop sharing this database across processes — tear down the broker socket so no new clients can attach.
---

Stops hosting the same-machine broker socket for this store: the listener stops accepting connections and the store is no longer reachable by new clients. Run from a client, it forwards to the owner, which stops hosting (ending that client's own connection). The store stays open in this process; the socket files are unlinked when the owner closes. Idempotent — a process that was not hosting reports `stopped: false`.
