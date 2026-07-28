---
summary: Report this process's multi-process IPC state.
mcp_description: Use this when the user asks whether this database is being shared across processes — whether this process owns it or is a client, whether a broker socket is hosted, and how many clients are connected.
---

Reports the same-machine multi-process state for this handle: whether this process owns the store (holds the writer lock) or is a client of another owner, whether it is hosting a broker socket, the socket path and owning process id when one exists, and the number of clients currently connected to the host. A single-process open (cache, or a durable open with IPC off) reports `is_owner: true`, `hosting: false`, and no socket.
