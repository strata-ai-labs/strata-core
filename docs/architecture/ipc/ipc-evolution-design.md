# IPC Evolution Design — Hardening the Transport for Multi-Reader Surfaces

**Status:** Draft for review
**Date:** 2026-08-04
**Track:** Post-V1 IPC hardening (new track; slices defined in §8)
**Relationship to existing docs:** This document amends
`docs/architecture/engine/ipc-and-command-boundary-contract.md`. That contract's
*Binding Decisions*, *Non-Goals*, and *Open Questions* remain normative; its "Current
Code Evidence" sections describe the pre-V1 implementation and are superseded by §2
here, which describes the transport as actually landed (2026-07-28, PRs #2840–#2846).
The first out-of-process observer surface — the VS Code extension
(`stratalab/strata-vscode`, `docs/requirements.md`) — is the forcing consumer; the
gaps below are ordered by what it needs, but every gap benefits any external surface
(future Node SDK wire clients, MCP, multi-terminal workflows).

---

## 1. Design guardrail: still not a server

The roadmap names the risk explicitly: *"IPC becoming a server product by accident"*
(risk #8). Everything in this document preserves the landed philosophy — the socket is
"the moral equivalent of SQLite's file lock":

- **Owner-hosted.** The socket exists only while a process holds the writer lock and
  opened with `IpcMode::Host`. No daemon, no service manager, no `strata serve`.
- **Same-user only.** `0600` socket, filesystem permissions are the security boundary.
  No authentication, no credentials, no network transport — ever, on this surface.
- **One engine process per store.** Multi-process access remains purely a transport
  concern (`crates/executor/src/ipc/mod.rs` header comment is canonical).
- **Engine stays IPC-free.** The dependency guard
  (`crates/engine/tests/dependency_guards.rs`) forbids IPC concepts in engine source;
  classification and gating live in the executor, consistent with the inversion stated
  in `crates/executor/src/ipc_mode.rs`.

A change that violates any of these is out of scope for this track regardless of how
useful it would be to a consumer.

## 2. Current state (ground truth, as landed)

### 2.1 Wire

- **Framing** (`crates/executor/src/ipc/wire.rs`): 4-byte big-endian `u32` length +
  payload; `MAX_FRAME_SIZE = 64 MiB`, enforced on both write and read (hostile length
  rejected before allocation).
- **Request envelope** (`crates/executor/src/ipc/protocol.rs`, `WireRequest`):
  `{branch?, space?, command}` — `command` is a `RawValue` so original bytes reach the
  lossy-integer ingress guard. Scope is fully request-determined: the server applies
  `branch`/`space` (or the owner's baseline captured at server start) under the same
  lock hold as dispatch; the command's own explicit fields still win.
- **Response**: the standard executor envelope, byte-identical to local execution —
  `{"type": …, "data": …}` or `{"error": {class, code, message, …}}`. There is no
  transport-level wrapper.
- **No protocol version, no handshake, no correlation IDs, no push frames.** The first
  frame a client sends is a `WireRequest`.

### 2.2 Server (`crates/executor/src/ipc/server.rs`)

Thread-per-connection over one `Arc<Mutex<Executor>>` (the mutex is forced: the engine
is single-session by construction — `Database` is non-`Clone`, operations take
`&mut self`). Constants: `MAX_CONNECTIONS = 128` (excess connections **dropped
silently**, not queued), `HANDLER_READ_TIMEOUT = 2 s` (shutdown-flag recheck),
`ACCEPT_POLL = 50 ms`. Per-request `catch_unwind` returns
`internal.executor.wire_response` instead of killing the owner. On-disk artifacts:
`strata.sock` (`0600`), `strata.pid`, optional `strata.sock.path` long-path pointer;
stale-socket cleanup at bind; RAII unlink on drop.

### 2.3 Client (`crates/executor/src/ipc/client.rs`, `connection.rs`)

30 s read / 5 s write timeouts; strictly one outstanding request per connection
(stream ordering is the only correlation); `Connection` brokers transparently on
`unavailable.engine.persistence` contention (bounded retry ≈ 500 ms), per `IpcMode`
(`Host` default / `Client` / `Off`).

### 2.4 Known absences (each becomes a gap in §3)

No access modes (every client is read-write; `AccessMode` from the contract is
unimplemented). No version negotiation or skew detection. No client identity
(`ipc_status` reports only `{is_owner, hosting, socket_path?, owner_pid?,
client_count}`). No cancellation and no server-side deadlines (a client timeout
orphans a command that keeps holding the lane). No subscriptions (consumers poll). No
parallel read execution (audit finding NODE-11, high, confirmed). No structured
rejection at the connection cap. No Windows transport (`#[cfg(unix)]` module; CLI
imports it unconditionally and does not compile on Windows). `Command::is_write()`
(`crates/executor/src/command.rs`) exists and is complete but has **no non-test
callers**.

## 3. Gap inventory

| Gap | Title | Consumer pain today | Slice (§8) |
|-----|-------|--------------------|------------|
| G1 | Protocol hello frame | Skew detection is heuristic; nowhere to declare access or identity | A |
| G2 | Server-side read-only sessions | Read-only is client-side courtesy only | A |
| G3 | Correlation IDs | One-in-flight is a protocol hole, not a rule; blocks cancel + push | A |
| G4 | Connection-cap rejection frame | Cap overflow looks like a hang | A |
| G5 | Version-tick notifications | Every consumer polls on the single execution lane | B |
| G6 | Per-request deadlines + cancel | Timed-out commands keep blocking everyone | C |
| G7 | Client identities in `ipc_status` | Status surfaces can't say who is attached | B |
| G8 | Parallel read execution | N readers serialize through one mutex (NODE-11) | D |
| G9 | Large-result bounding | 64 MiB frame cap vs. unpaged commands is unaudited | E |
| G10 | Windows transport | No IPC on Windows; CLI doesn't compile there | E |

Adjacent adoption gaps tracked in sibling repos (not IPC framework changes, but they
gate who can attach at all): `strata-python` builds the executor with
`default-features = false` and omits the `ipc` feature — Python-hosted apps neither
host nor broker; `strata-nodesdk` still binds the embedded `stratadb` facade —
no executor, no wire, no IPC.

## 4. Protocol revision 2 (G1–G4)

One coherent wire change. The current implicit protocol is retroactively "protocol 1".

### 4.1 G1 — Hello frame

The first frame on a connection MAY be a hello. Frames are distinguished by their
single top-level intent key (`hello` here; `subscribe`/`cancel` in later slices) —
a `WireRequest` is recognized by its `command` key, so sniffing is unambiguous.

Client → server:

```json
{"hello": {
  "protocol": 2,
  "idl": {"schema_version": "strata.idl.v1", "generator_version": "strata-executor-idl.1"},
  "client": {"name": "strata-vscode", "version": "0.1.0", "pid": 4242},
  "access": "read",
  "capabilities": ["notify.version"]
}}
```

Server → client (the executor response envelope, so error shapes are uniform):

```json
{"type": "ipc_hello", "data": {
  "protocol": 2,
  "release": "1.0.0",
  "idl": {"schema_version": "strata.idl.v1", "generator_version": "strata-executor-idl.1"},
  "granted_access": "read",
  "capabilities": ["notify.version"],
  "owner_pid": 1234
}}
```

Rules:

- `access` defaults to `read_write`; the server echoes `granted_access` (today always
  the requested mode; the field exists so a future policy can clamp).
- `capabilities` is a want-list; the server grants the intersection with what it
  supports and ignores unknown names — the forward-compatibility point for G5/G6.
- DTOs follow workspace policy: `deny_unknown_fields`, evolution via optional fields
  with serde defaults.
- A malformed hello or unsupported `protocol` fails the connection with a new
  registered code `invalid_argument.executor.ipc_hello` (retry `Never`).
- **Legacy acceptance (transitional):** a first frame that parses as `WireRequest`
  marks the connection protocol 1 — full access, no IDs, no pushes, anonymous in
  `ipc_status`. All in-family consumers (CLI, MCP) send hello in the same slice.
  Legacy acceptance is removed before the release train (no permanent compatibility
  shims, per V1 cutover rules); the removal condition is stated in code at the sniff
  site.

### 4.2 G2 — Server-side read-only sessions

`AccessMode { ReadWrite, ReadOnly }` lands in the executor (reviving the contract's
§Access Mode as a *session* property, not an open-mode — a local read-only **open** is
a separate engine feature and out of this track's scope).

- **Gate:** in the server dispatch path, after envelope decode and before execution: a
  `ReadOnly` session submitting a command classified as a write is rejected with a new
  registered code `access_denied.executor.read_only_session` (class `access_denied`
  already exists in the error contract vocabulary; retry `Never`; commit outcome
  `definitely_not_committed`). Exact code name goes through error-contract review.
- **Classification:** `Command::is_write()` becomes the runtime authority — its first
  real caller. The IDL `access` facet remains the authored truth. A **generated
  conformance test** (via the `strata-idl` tests_gen pipeline) asserts
  `is_write(cmd) == (idl.access == "write")` for all 127 commands, so the runtime gate
  and the authored classification cannot drift. `ipc_stop` stays permitted for
  read-only sessions (it is transport administration, not data mutation) — the
  conformance test carries an explicit allowlisted exception if its IDL kind says
  otherwise.
- **Client courtesy:** `Connection` pre-rejects locally on a read-only session to save
  the round trip; the server remains the authority.
- **CLI surface:** a global `--read-only` flag maps to `AccessMode::ReadOnly` for
  brokered sessions (and is honored client-side for local opens as a courtesy until a
  real read-only open exists).

### 4.3 G3 — Correlation IDs

Protocol-2 connections wrap both directions in a minimal transport envelope that
leaves the executor payload untouched (local/remote byte-parity of the *payload* is
preserved; the transport wrapper is transport-only):

- Request: `WireRequest` gains `id: u64` (required on protocol 2).
- Response: `{"id": 7, "payload": {…executor envelope…}}`.
- Notifications (G5) are `{"notify": …}` frames with no `id`.

Server behavior in this slice is unchanged — frames are read and answered in order;
one-in-flight becomes a *client discipline* rather than a protocol hole. Pipelining is
permitted (responses arrive in request order); out-of-order completion arrives only
with G8, and the wire is already shaped for it. IDs are the prerequisite for cancel
(G6) and for distinguishing pushes from responses (G5).

### 4.4 G4 — Connection-cap rejection frame

At the cap (`MAX_CONNECTIONS`), the listener currently drops the connection with no
bytes written; the client sees a hang until timeout. Change: best-effort write of a
single error frame — new registered code `resource_exhausted.executor.ipc_connections`
(class `resource_exhausted` exists; retry `SameRequest`) — with a short write timeout,
then close. Clients map it to an actionable "owner is at connection capacity" state.

## 5. Liveness (G5, G7)

### 5.1 G5 — Version-tick notifications

The minimal subscription that removes polling, deliberately **metadata-only** (no row
data on the push path — nothing to redact, and it keeps the surface far from
server-product territory).

- Subscribe (protocol 2, capability `notify.version` granted at hello):
  `{"id": 3, "subscribe": {"events": ["version"]}}` → acked with an
  `{"id": 3, "payload": {"type": "ipc_subscribed", …}}` envelope.
- Push: `{"notify": {"event": "version", "branch": "main", "version": 812}}`.
- Semantics: **coalesced and lossy** — latest-wins per branch; a slow client gets
  fewer ticks, never a backlog. No acks. Delivery is best-effort; a reconnecting
  client re-reads state (the same rule as `MaybeCommitted` recovery).

Implementation without touching the engine:

1. **Server-dispatched writes:** after a successful dispatch classified as a write
   (G2's `is_write()`), read the current commit version while still under the lock and
   broadcast if advanced.
2. **Owner-local writes** (the owner process writing through its own `Local`
   connection bypasses server dispatch): a watcher thread polls the version under the
   mutex at a coarse interval (100–250 ms) **only while subscribers exist**. This
   converts N clients × their individual polling into one in-process check.

Refinement (optional, later): the engine exposing a lock-free version watermark
(`AtomicU64`) would eliminate the watcher's lock acquisitions; not required for the
slice and touches engine, so it is explicitly deferred.

### 5.2 G7 — Client identities in `ipc_status`

`IpcStatus` (`crates/executor/src/types/admin.rs`) gains
`clients: [{name?, version?, pid?, access, protocol}]`, populated from the hello
registry the server keeps per connection. Protocol-1 connections appear as anonymous
entries. `client_count` is preserved for compatibility. Names are display identifiers,
not authentication — same-user trust model is unchanged.

## 6. Deadlines and cancellation (G6)

- `WireRequest` gains `deadline_ms: u64?` — a relative budget from server receipt.
- **Phase 1 (transport only):** the handler checks the deadline after acquiring the
  executor lock and *before* dispatch; an expired request is answered with a new
  registered code (proposed `unavailable.executor.ipc_deadline`; retry `SameRequest`;
  `definitely_not_committed`) instead of executing pointlessly. This bounds the damage
  of queue-wait behind a long command: the lane still serializes, but expired work is
  shed instead of piling on.
- **Cancel frame:** `{"cancel": {"id": 7}}` — best effort. Phase 1 cancels only
  requests not yet dispatched (their response is the deadline/canceled error);
  an in-flight command cannot be interrupted.
- **Phase 2 (engine cooperation, separate slice):** cooperative deadline checks at
  pagination boundaries inside long reads (scans, analytics). Full mid-command
  cancellation is engine work and rides with G8's read-path changes if at all.
- The server-side deadline never replaces client timeouts; it makes them honest — a
  client that gave up is no longer charged to everyone else's latency.

## 7. Structural tracks (G8–G10)

### 7.1 G8 — Parallel read execution (NODE-11)

The single-lane bottleneck is the deepest gap: every request from every client plus
the owner's own work serializes through `Arc<Mutex<Executor>>`, exactly as the July
audit predicted for any IPC front-end. Direction (per the audit recommendation):
a shareable read handle or interior mutability on the engine read path — MVCC already
provides snapshot semantics; the executor splits into a `&self` read path and a
`&mut self` write path (or an `RwLock` at the executor boundary), and the server
routes by G2's classification: reads execute concurrently, writes exclusively.
This is engine + executor surgery with its own design document when picked up; it is
tracked here because it changes server threading and is the payoff that makes
"multiple readers" mean parallel execution rather than merely concurrent access.
G2 is a hard prerequisite (the router must trust the classification; the conformance
test makes it trustworthy).

### 7.2 G9 — Large-result bounding

The 64 MiB frame cap coexists with commands that have no enforced pagination (the IDL
pagination facet: 100 of 127 commands are `none`; most are small by construction, but
`arrow.export`, `event.range`, graph analytics outputs, and embedding-bearing vector
reads are not). Phase 1 is an audit: enumerate commands whose responses can approach
the cap, give each an enforced server-side bound or required pagination, and record
the result as an IDL-facet-driven conformance check. Phase 2 — chunked streaming
frames (`{"partial": {"id", "seq", "last"}}`) — only if a real need survives the
audit. Do not build streaming speculatively.

### 7.3 G10 — Windows transport

Named pipes behind the existing `resolve_binding` seam; `IpcMode` is already
platform-neutral by design. Includes fixing the CLI's unconditional
`strata_executor::ipc::Connection` imports (`crates/cli/src/{lib,open,mcp,repl,doctor}.rs`)
so the CLI compiles on Windows with IPC compiled out, independent of when the pipe
transport lands. Contract Open Question #3. Separate track; gates VS Code marketplace
reach on Windows but nothing in extension V1.

## 8. Sequencing

| Slice | Contents | Shape |
|-------|----------|-------|
| A — protocol rev 2 | G1 hello + G3 IDs + G4 cap frame; then G2 access gate + conformance test + CLI `--read-only` | Two PRs over the same envelope code; land before external consumers harden their transport layers |
| B — liveness | G5 version ticks + G7 identities | One PR; requires A |
| C — deadlines | G6 phase 1 (+ cancel-queued) | One PR; requires A |
| D — parallel reads | G8, own design doc first | Engine + executor track; requires G2 |
| E — bounding & platform | G9 audit (then maybe streaming); G10 Windows | Independent |
| Sibling repos | `strata-python`: add `ipc` to executor features; `strata-nodesdk`: executor-wire cutover | Filed in their own repos |

Slice A is deliberately front-loaded: it is one envelope change touching one code
seam, every later gap negotiates through the hello it introduces, and it will never be
cheaper than before the release train and before external surfaces ship transport
code against protocol 1.

## 9. Testing requirements

- **Cross-process harness:** extend the real-process pattern of
  `crates/cli/tests/ipc_start_stop.rs` — hello round-trip and stamp echo; legacy
  first-frame acceptance (until removed); cap-overflow rejection frame; deadline
  shedding; tick coalescing under a writing owner; identity listing.
- **Generated from the IDL:** the G2 conformance test (`is_write` vs. `access` facet,
  all 127 commands); a read-only-session rejection test that iterates every
  write-class command from the catalog rather than a hand-picked sample.
- **Error discipline:** all new codes registered in `errors.yaml` + the executor error
  registry with class, retry policy, and commit outcome; tests assert class + code,
  never message text (workspace standard).
- **Guard hygiene:** new envelope fields covered by the existing unknown-key and
  lossy-integer ingress guards; `deny_unknown_fields` on all new DTOs.

## 10. Consumer mapping (strata-vscode requirements)

| Extension requirement | Served by |
|---|---|
| AR-4.3 server-side read-only flip | G2 |
| AR-6 version-skew handling | G1 |
| AR-2.3 one-in-flight discipline | G3 (becomes explicit contract) |
| AR-5 polling liveness | G5 (replaces polling) |
| AR-2.5 orphaned-timeout story | G6 |
| AR-3.5 status bar attachment state | G7 |
| N2 guest discipline on the lane | G8 (removes the need) |
| §7 D6/D7 SDK reachability | Sibling-repo issues |

## 11. Issue tracking

The umbrella issue owns the live checklist.

| Gap | Issue |
|-----|-------|
| Umbrella | [#2871](https://github.com/stratalab/strata-core/issues/2871) |
| G1 hello frame | [#2872](https://github.com/stratalab/strata-core/issues/2872) |
| G2 read-only sessions | [#2873](https://github.com/stratalab/strata-core/issues/2873) |
| G3 correlation IDs | [#2874](https://github.com/stratalab/strata-core/issues/2874) |
| G4 cap rejection frame | [#2875](https://github.com/stratalab/strata-core/issues/2875) |
| G5 version ticks | [#2876](https://github.com/stratalab/strata-core/issues/2876) |
| G6 deadlines + cancel | [#2877](https://github.com/stratalab/strata-core/issues/2877) |
| G7 client identities | [#2878](https://github.com/stratalab/strata-core/issues/2878) |
| G8 parallel reads (NODE-11) | [#2879](https://github.com/stratalab/strata-core/issues/2879) |
| G9 large-result bounding | [#2880](https://github.com/stratalab/strata-core/issues/2880) |
| G10 Windows transport | [#2881](https://github.com/stratalab/strata-core/issues/2881) |
| strata-python `ipc` feature | [strata-python#68](https://github.com/stratalab/strata-python/issues/68) |
| strata-nodesdk executor cutover | [strata-nodesdk#22](https://github.com/stratalab/strata-nodesdk/issues/22) |
