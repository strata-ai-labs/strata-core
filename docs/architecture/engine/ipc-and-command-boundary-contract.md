# Engine-Next IPC And Serializable Command-Boundary Contract

Status: V1 architecture draft

## Purpose

This document defines the target contract for Strata's local IPC and
serializable command boundary.

IPC exists for one product reason:

```text
same-machine shared access to one running Strata database instance
```

It is not server mode. It is not a network database. It is not a Redis-like
service surface. It is how local processes on the same machine can use one
database safely when one process owns the writable engine handle.

The current product shape is:

```text
strata up
  -> open database locally
  -> start local IPC service for that database
  -> other same-machine clients connect through ordinary Strata APIs/CLI
```

Users should not need a separate `strata ipc ...` command family. The command
boundary exists internally so SDK, CLI, REPL, and future Strata AI clients can
send typed requests and receive typed responses when they are not executing
in-process.

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/strata-v1-user-pathways.md`
4. `docs/product/pathways/operations-and-interfaces.md`
5. `docs/architecture/engine-architecture.md`
6. `docs/architecture/engine/README.md`
7. `docs/architecture/engine/persistence-adapter-contract.md`
8. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
9. `docs/architecture/engine/retrieval-and-derived-state-contract.md`
10. `docs/architecture/v1-error-and-diagnostics-contract.md`

Follow-up contracts that depend on this one:

1. Dataset clone artifact contract.
2. Public API and CLI surface cleanup checklist.
3. Product-pathway conformance plan.

## Requirement Language

1. Must means the IPC or command-boundary contract is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

The current implementation already has the right broad shape.

### Product Open

`crates/engine/src/database/product_open.rs` owns product-open classification:

1. `open_product_database()` tries to open a local primary database.
2. If the primary lock is in use and `<data_dir>/strata.sock` exists, it
   returns `ProductOpenOutcome::Ipc`.
3. If the primary lock is in use and no socket exists, it returns
   `LockedWithoutIpcSocket`.
4. The engine does not construct an IPC client. It reports `socket_path` and
   lets executor own transport.
5. `open_product_cache()` returns a local cache database and should not produce
   IPC fallback.

This is the right boundary: engine classifies product open outcomes; executor
owns IPC transport.

Current residue: the locked-without-socket user-facing message still mentions
follower mode. Follower mode is not the target V1 multi-client story and should
not be preserved by this contract.

### IPC Transport

`crates/executor/src/ipc/*` owns local IPC:

1. `IpcServer::start()` binds a Unix domain socket at `<data_dir>/strata.sock`.
2. The server writes `<data_dir>/strata.pid`.
3. Socket and PID files are set to `0600`.
4. `IpcServer::stop()` reads the PID file, sends `SIGTERM`, and removes the
   socket/PID files.
5. Requests and responses are MessagePack encoded.
6. Framing is a 4-byte big-endian payload length followed by payload bytes.
7. Maximum frame size is currently 64 MiB.
8. Each request has an `id` and a serialized `Command`.
9. Each response echoes the `id` and returns `Result<Output, Error>`.
10. The server creates one executor `Session` per connection.
11. Transaction state is connection-local.

The current implementation is Unix-domain-socket based. The architecture
contract is same-machine local IPC, not a cross-machine network protocol.

### Executor Command Boundary

`crates/executor/src/command.rs`, `output.rs`, and `error.rs` define the current
serializable instruction set:

1. `Command` is a typed enum of executable product operations.
2. `Output` is a typed enum of successful command results.
3. `Error` is a typed, serializable command error.
4. Commands use product concepts such as branch, space, key, recipe, graph,
   vector, event, search, and database operations.
5. Commands are `serde` serializable and reject unknown fields.
6. Commands carry optional branch/space fields where runtime defaults can
   apply.
7. `Command::is_write()` classifies write commands for access-mode guards.

The target should preserve the idea of one command instruction set. The exact
Rust enum variants do not need to freeze before V1, but the boundary must remain
typed and serializable.

### Strata Handle And Session

`crates/executor/src/compat.rs` and `session.rs` route local and IPC execution:

1. `Strata::open_with()` consumes `ProductOpenOutcome`.
2. A local outcome becomes a local `Executor`.
3. An IPC outcome becomes an `IpcClient`.
4. Public `Strata` methods execute the same `Command` shape through either
   local executor or IPC client.
5. `Strata::database()` is unavailable on IPC handles and currently panics.
6. `Strata::is_ipc()` reports handle transport.
7. `Session` may be local or IPC.
8. Local sessions own local transaction state.
9. IPC sessions send commands over the socket and track enough client-side
   transaction state to preserve branch-default behavior.
10. Read-only access checks exist on both `Strata` IPC execution and `Session`
    execution.

This is the right product model: local and IPC handles use the same command
semantics, while direct engine access stays local-only.

### CLI

`crates/cli/src/admin.rs`, `app.rs`, and `open.rs` define current CLI behavior:

1. `strata up` starts the IPC server for a database.
2. `strata down` stops it.
3. Normal CLI commands open a `Strata` handle.
4. If the database is locked and a socket exists, normal commands route through
   IPC transparently.
5. There is no separate user-facing IPC command namespace.

This is the intended product shape.

## Non-Goals

This contract does not define:

1. A remote server mode.
2. A TCP listener.
3. Cross-machine access.
4. Multi-node replication.
5. Redis protocol compatibility.
6. A hosted query service.
7. Remote authentication.
8. TLS, proxy, load balancing, service discovery, or network authorization.
9. A separate `strata ipc ...` command family.
10. A new public transaction product surface.

If Strata ever adds server mode, that should be a separate product and
architecture effort. It should not leak into V1 local IPC.

## Binding Decisions

1. **IPC is local-only.**
   V1 IPC is for same-machine clients sharing one running database instance.
   It must not become a network service by accident.

2. **`strata up` is the user-facing entry point.**
   Users start local sharing with `strata up` and stop it with `strata down`.
   Normal SDK/CLI/agent commands then use the same database surface.

3. **There is one command instruction set.**
   Local executor and IPC clients use the same serializable command shape.
   There should not be separate local commands and IPC commands.

4. **Engine classifies open outcome; executor owns transport.**
   Engine may return local database handles or IPC fallback facts. It must not
   own IPC client/server implementation.

5. **The IPC service owns the writable local database handle.**
   When `strata up` is running, other clients should connect to the local
   service instead of opening independent writable primary handles.

6. **Storage is not part of the command boundary.**
   IPC commands must not expose storage keys, storage-space IDs, WAL records,
   table files, manifests, or compaction internals as normal product fields.

7. **Transport must not change product semantics.**
   A command run locally and the same command run over IPC should produce the
   same product result or the same class of structured error, except for
   transport-specific failures.

8. **Command DTOs use product vocabulary.**
   Commands carry branch, space, temporal selectors, keys, document IDs,
   recipes, vectors, graph IDs, and product options. They do not carry Rust
   closures, engine handles, storage handles, or process-local references.

9. **Access mode is enforced on both sides.**
   Clients should reject obvious read-only writes before sending them. The
   service/session boundary must also reject writes when access mode is
   read-only.

10. **Transactions are connection-local compatibility, not V1 product
    direction.**
    Current IPC sessions preserve transaction state per connection. Future V1
    product docs may remove public transaction workflows, but the command
    boundary must still support internal or compatibility behavior while it
    exists.

11. **Maintenance authority is explicit.**
    Commands that write maintenance state, rebuild derived state, mutate
    recipes, pull models, compact, flush, export, import, or change config are
    writes for access-mode and authority purposes.

12. **Strata AI is an ordinary local client.**
    Future Strata AI should connect through the same command boundary when it
    uses a running local database. It should not get privileged private engine
    APIs that CLI/SDK cannot use.

13. **Protocol compatibility is explicit.**
    Current IPC lacks a protocol handshake. V1 should define enough protocol or
    capability reporting to fail clearly when client and server are
    incompatible.

14. **IPC errors are database errors plus transport errors.**
    Command failures use command/database error semantics. Socket unavailable,
    stale socket, protocol decode failure, response ID mismatch, timeout, and
    server shutdown are transport errors.

## Target Flow

### Local Open Without IPC

```text
client opens database
  -> engine product open succeeds locally
  -> executor wraps Arc<Database>
  -> commands execute in-process
```

### Local Sharing With `strata up`

```text
strata up
  -> open database locally
  -> start same-machine IPC server
  -> write socket + pid files
  -> own writable database handle until shutdown
```

### Client Open While Server Runs

```text
client opens database
  -> engine product open sees primary lock
  -> socket exists
  -> engine returns ProductOpenOutcome::Ipc
  -> executor connects IpcClient
  -> client uses normal Strata handle
```

### Locked Without Server

```text
client opens database
  -> engine product open sees primary lock
  -> socket missing
  -> fail with locked-without-socket diagnostic
  -> message points to `strata up`
```

This path should not recommend follower mode in the target V1 product.

## Command Envelope

The current envelope is:

```text
Request {
    id: u64,
    command: Command,
}

Response {
    id: u64,
    result: Result<Output, Error>,
}
```

This is a reasonable conceptual shape.

V1 should preserve:

1. Correlation ID.
2. Typed command.
3. Typed output.
4. Typed error.
5. Bounded frame size.
6. Deterministic request/response matching.

V1 should add or clarify:

1. Protocol version.
2. Server capability summary.
3. Database identity or handle identity where safe.
4. Access mode.
5. Server shutting-down state.
6. Error status compatible with the V1 error contract.

These additions should not create a second command system.

## Command Semantics

Every serializable command should declare:

1. Name.
2. Category.
3. Whether it is a read, write, maintenance write, or local-only operation.
4. Required access mode.
5. Whether it requires a session.
6. Whether it can run inside an active transaction while transaction
   compatibility exists.
7. Branch and space defaulting behavior.
8. Temporal selector behavior.
9. Expected output variant.
10. Expected error classes.
11. Pagination or streaming behavior, if any.

The current `Command::is_write()` is the seed of this contract. Engine-next
should avoid scattering write classification across CLI, executor, IPC, and
engine.

### Reads

Reads include ordinary lookup, list, search, health, metrics, info, describe,
and status commands.

Reads may still be denied if:

1. The command requires unavailable derived state and policy says fail.
2. The command tries to access system data through a normal product path.
3. The database is closing or recovery failed.
4. The runtime profile does not support the requested operation.

### Writes

Writes include source-data mutations, branch mutations, recipe/config changes,
model pulls, imports, rebuilds, compaction, flush, retention apply, and any
command that changes durable or observable engine state.

Writes must be rejected for read-only handles before mutation.

### Maintenance Writes

Maintenance writes are writes whose purpose is operational rather than user data
mutation:

1. Flush.
2. Compact.
3. Reindex.
4. Repair.
5. Recipe seed.
6. Model pull.
7. Retention apply.
8. Future derived-state rebuilds.

Maintenance writes should be classified distinctly enough that CLI, SDK,
Strata AI, and future policy layers can ask for explicit authority.

V1 maintenance authority:

1. `strata up` owns maintenance authority for the database it opens because it
   owns the writable local engine handle.
2. Ordinary IPC clients do not receive maintenance authority by default, even
   when they are read-write clients.
3. CLI/admin commands that require maintenance authority must either execute in
   the owner process or request an explicit maintenance-capable local command
   path.
4. Read-only clients never have maintenance authority.
5. Missing maintenance authority fails before mutation with a structured
   failed-precondition status.

### Local-Only Operations

Some operations are inherently local:

1. Returning a raw `Arc<Database>`.
2. Inspecting process-local handles.
3. Installing signal handlers.
4. Starting or stopping the IPC server.
5. Accessing credential stores or local process state where not represented as
   normal commands.

These should not be smuggled through ordinary IPC commands.

## Session Semantics

The current IPC server creates one `Session` per connection. That means:

1. Current branch and current space are connection-local.
2. Transaction compatibility state is connection-local.
3. A new client connection starts from default branch and default space unless
   the caller sets context.
4. Dropping an IPC handle closes that connection but does not stop the server.

Target rules:

1. Session state must not leak across clients.
2. Commands should carry explicit branch and space when possible.
3. CLI/REPL context may remain ergonomic client-side state.
4. IPC should not depend on hidden process-global current branch or space.
5. If public transaction commands are removed later, session state should
   remain useful only for command context, not product transactions.

## Access Mode

Current access mode facts:

1. `AccessMode::ReadWrite` and `AccessMode::ReadOnly` live in engine.
2. Product open returns selected access mode.
3. Executor stores access mode on `Strata` and `Session`.
4. Read-only writes are rejected before execution.

Target rules:

1. Open outcome must report selected access mode.
2. IPC client handle must remember selected access mode.
3. Server session must enforce access mode.
4. Client-side early rejection is allowed but not trusted as the only guard.
5. Access denied errors should use stable error status, not only prose.

## Service Lifecycle

`strata up` lifecycle should remain small:

1. Open the database.
2. Bind same-machine socket.
3. Write PID file.
4. Serve command requests.
5. Handle shutdown.
6. Remove socket and PID files.

Target lifecycle diagnostics:

| Condition | Expected behavior |
|---|---|
| Server already running | Report existing server or stale PID clearly. |
| Stale PID file | Remove or report stale state safely. |
| Stale socket file | Remove only when safe, otherwise report. |
| Database locked with socket | Normal clients connect through IPC. |
| Database locked without socket | Tell user to run `strata up`. |
| Server shutting down | Reject or close requests with structured unavailable error. |
| Protocol mismatch | Fail before executing command. |
| Oversized frame | Reject without executing command. |

The target should preserve `0600` socket/PID permissions or an equivalent
same-user local protection model.

Cache mode does not support IPC owners for V1. Cache mode is ephemeral
in-process state; `strata up` must open a durable local database, not host a
cache database over IPC.

For V1, `strata up` always owns a writable local handle. Read-only IPC clients
are allowed. Read-only IPC owners are deferred.

## Transport

Current transport is:

1. Unix domain socket.
2. MessagePack payloads.
3. Length-prefixed frames.
4. Request/response per command.
5. One handler thread per connection up to a fixed limit.

The V1 contract should keep transport modest:

1. Local-only.
2. Bounded frames.
3. Deterministic request/response IDs.
4. No remote network protocol.
5. No command execution after decode failure.
6. No silent response ID mismatch.

Cross-platform support is a portability concern. If Windows support requires
named pipes or another local IPC primitive, that should implement the same
command envelope and local-only semantics rather than changing the product
model.

## Error Boundary

IPC needs two error layers:

1. Command/database errors.
2. Transport/protocol errors.

Command/database errors are normal command results:

```text
Response {
    id,
    result: Err(command_error)
}
```

Transport/protocol errors happen before or outside command execution:

1. Connect failed.
2. Socket missing.
3. Read timeout.
4. Write timeout.
5. Decode failure.
6. Encode failure.
7. Oversized frame.
8. Response ID mismatch.
9. Server shutdown.
10. Server panic converted to internal command error where possible.

V1 should map both layers into the error/diagnostics contract. In particular:

1. Socket unavailable should map to `unavailable.ipc_socket` or its V1
   equivalent.
2. Protocol mismatch should map to an invalid or failed-precondition error.
3. Server panic should map to internal error and should be logged.
4. Read-only write should map to access denied / permission denied.
5. Ambiguous write outcomes over IPC must remain explicit if the connection
   fails after a command may have committed.

The current request/response model does not fully solve ambiguous write
outcomes. The V1 command boundary must not paper over that class of failure.

## Pagination, Streaming, And Frame Limits

Current IPC has a 64 MiB frame limit and returns whole `Output` values.

Target rules:

1. Large result commands need pagination or cursors.
2. History, scan, list, search, branch diff, export, and analytics must not rely
   on unbounded single-frame responses.
3. Export/import may need artifact paths or streaming protocols later, but V1
   should not invent remote streaming semantics as part of local IPC.
4. Frame-limit errors should be structured and actionable.

## Strata AI

Strata AI should use the same local command boundary as CLI and SDK when it
talks to a running local database.

Rules:

1. Strata AI is a local client, not a privileged storage client.
2. It should receive structured command outputs and errors.
3. It should not call private engine APIs that normal clients cannot call.
4. It should respect access mode and command authority.
5. It should expose command provenance when it takes actions on behalf of a
   user.

This is why IPC is V1-relevant. Multiple local clients, including Strata AI,
need one safe local coordination path.

## Cleanup Targets

This contract should guide cleanup without adding functionality immediately.

Known current cleanup targets:

1. Remove stale follower guidance from locked-without-socket messages.
2. Keep follower mode out of the V1 IPC story.
3. Add protocol/capability handshake before freezing IPC behavior.
4. Align executor `Error` with the V1 error/diagnostics contract.
5. Centralize command read/write/maintenance classification.
6. Ensure every command has a documented output variant and access class.
7. Replace panic-prone public local-only APIs with typed unavailable errors
   where they can be called accidentally on IPC handles.
8. Add conformance tests that run representative commands locally and over IPC.
9. Verify oversized frame and protocol decode failures do not execute commands.
10. Verify transaction/session compatibility is connection-local while it
    remains supported.
11. Do not add idempotency keys to V1 write commands. Ambiguous write outcomes
    over IPC keep `unknown` retry policy; clients should reopen or inspect
    state after `ambiguous_commit.*`.

## Conformance Requirements

The product-pathway conformance plan should include:

1. Open local database when no server is running.
2. Start `strata up` foreground and background.
3. Open another client while server is running and verify IPC fallback.
4. Verify normal CLI/SDK commands work through IPC without different user
   syntax.
5. Verify read-only IPC handles reject writes.
6. Verify server-side access mode rejects writes even if a client fails to
   precheck.
7. Verify session state is per connection.
8. Verify transaction compatibility state is per connection while public
   transaction commands remain.
9. Verify socket/PID permissions.
10. Verify stale PID/socket behavior.
11. Verify locked-without-socket error tells the user to run `strata up`.
12. Verify command outputs match local execution for representative KV, JSON,
    event, vector, graph, branch, search, recipe, config, health, and metrics
    commands.
13. Verify transport failures produce structured errors.
14. Verify oversized frames and decode failures do not execute commands.
15. Verify IPC close does not shut down the server.
16. Verify `strata down` stops the service and cleans up files.

## Open Questions And Closed V1 Baselines

1. What protocol version and capability handshake should V1 use?
2. Should the IPC transport remain MessagePack or move to a named stable wire
   format before V1 freeze?
3. What is the cross-platform local IPC primitive for Windows?
4. Ambiguous write outcomes over IPC.
   Closed for V1: use `ambiguous_commit.ipc_disconnect` with `maybe_committed`
   and `unknown` retry policy.
5. Maintenance authority.
   Closed baseline: owner-local maintenance commands require maintenance
   authority; ordinary IPC clients do not have it by default. Read-only clients
   never have it.
6. Should `database()` on IPC handles become a typed error-returning API rather
   than a panic-prone API?
7. How much server identity should be exposed to local clients?
8. Read-only IPC owner.
   Closed for V1: `strata up` owns a writable handle; read-only hosting is
   deferred.

## Bottom Line

IPC is not a new database mode. It is a local coordination mechanism.

The product contract is:

```text
same Strata commands
same product semantics
same machine
one running database owner
typed request/response boundary
```

That keeps Strata embedded-first while allowing practical local multi-client
use, including CLI, SDK processes, notebooks, and Strata AI. It avoids drifting
into server-mode competition where Strata would have to become a networked
database service instead of a powerful embedded database.
