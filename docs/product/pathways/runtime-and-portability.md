# Runtime And Portability Pathways

Status: Draft pathway group

This document expands the V1 pathways for opening Strata databases, using
portable datasets, and choosing storage backends intentionally.

## Pathway 1: Create Or Open A Local Embedded Database

### Goal

A developer opens a filesystem path and gets a durable, usable Strata database
inside the current process.

### Flow

1. Choose a database path.
2. Call the SDK open API or run the CLI with that path.
3. Strata validates or initializes the database directory.
4. Strata acquires the required local process lock.
5. Strata runs recovery if needed.
6. The user receives a database handle ready for reads and writes.

### Surface

SDK open API, CLI `--db`, database config, local filesystem backend,
durability/recovery diagnostics, health and info commands.

### Guarantees

Strata must preserve committed data across process restart, recover cleanly
after ordinary crashes, reject incompatible formats or configs clearly, and
prevent unsafe concurrent local writers.

### Failures

Invalid path, permission denied, lock conflict, unsupported format, corruption,
invalid config, recovery failure, and backend capability failure should surface
as explicit open errors.

### V1 Decision

Required.

### Cleanup

Keep durable local open as the reference path. Remove any product language that
makes cache, follower, or manual maintenance appear equivalent to normal durable
open.

## Pathway 2: Open An Ephemeral Cache Database

### Goal

A developer or test runtime creates a temporary Strata database that does not
promise durable persistence.

### Flow

1. Request cache mode explicitly.
2. Strata creates an in-memory or explicitly ephemeral runtime.
3. The user runs ordinary commands against the temporary database.
4. Strata avoids WAL, manifest, checkpoint, and durable file behavior.
5. The database disappears when the process or handle ends.

### Surface

SDK cache API, CLI `--cache`, open options, runtime config, health/info output.

### Guarantees

Cache mode must be visibly non-durable, must not create a hidden disk durability
model, and should support the normal data model where durability is not needed.

### Failures

Unsupported cache configuration, memory pressure, invalid runtime options, and
commands that require durable storage should fail clearly.

### V1 Decision

Required.

### Cleanup

Keep explicit ephemeral cache mode. Remove disk-backed cache as a product mode.

## Pathway 3: Open A Database Read-Only

### Goal

A user inspects or queries an existing database without being able to mutate it.

### Flow

1. Open an existing database with read-only access.
2. Strata validates the database and runs safe recovery or refuses if recovery
   would require mutation.
3. The user runs read, search, inspect, export, or describe commands.
4. Any write-classified command is rejected before mutation.

### Surface

Open options, CLI `--read-only`, command write classification, health/info
output, error reporting.

### Guarantees

Read-only mode must reject writes consistently across all product commands,
must not mutate user data, and must explain when recovery cannot proceed without
write access.

### Failures

Missing database, unsupported backend, recovery requiring mutation, lock
conflict, and attempted write commands should surface as clear user-facing
errors.

### V1 Decision

Required.

### Cleanup

Keep read-only open. Test the command write classification as product behavior,
not just executor plumbing.

## Pathway 4: Share A Local Database Through IPC

### Goal

A second local process, including Strata AI, uses an already-open database
through IPC instead of opening the database directly as another writer.

### Flow

1. A primary process opens the database.
2. The primary process exposes an IPC endpoint.
3. A second process or Strata AI asks to open the same database.
4. Strata detects the primary and routes the second process through IPC where
   configured.
5. The second process receives a handle with documented local/IPC behavior.

### Surface

IPC server commands, product open API, CLI open flags, access mode reporting,
IPC errors.

### Guarantees

IPC must preserve database correctness, avoid unsafe concurrent writers, expose
whether the handle is local or IPC-backed, preserve access-mode semantics, and
give the user predictable errors when IPC is unavailable.

### Failures

Primary not running, stale socket, permission denied, protocol mismatch, IPC
server error, and read/write access mismatch should fail explicitly.

### V1 Decision

Required.

### Cleanup

Keep IPC as the required multi-process local access story. Do not preserve
follower mode as a parallel shared-access mechanism.

## Pathway 5: Clone A Portable Dataset

### Goal

A user runs `strata clone <source> <destination>` and receives a normal Strata
database at the destination.

### Flow

1. Choose a dataset source and destination.
2. Run clone through CLI or SDK.
3. Strata fetches and validates the source artifact.
4. Strata writes a complete database at the destination.
5. Strata verifies enough metadata to open the cloned database normally.
6. The user opens the destination with standard Strata APIs.

### Surface

CLI clone command, SDK clone API, dataset bundle format, storage backend
contract, verification output, StrataHub dataset URLs later.

### Guarantees

Clone must produce a normal database under the user's control, preserve required
branch/version metadata, validate format and integrity, and avoid hidden
dependency on the source after clone completes.

### Failures

Unavailable source, unsupported source scheme, invalid artifact, checksum or
manifest failure, destination conflict, partial write, and backend capability
failure should produce clear clone errors and avoid leaving a misleading
database behind.

### V1 Decision

Required.

### Cleanup

Make clone the cold-start data movement primitive. Do not carry legacy branch
bundle workflows forward as the V1 product artifact.

## Pathway 6: Use A Cloned Dataset Offline

### Goal

A user branches, modifies, searches, exports, and inspects a cloned database
without contacting the source.

### Flow

1. Clone a dataset.
2. Open the cloned database locally.
3. Inspect dataset metadata and branches.
4. Create branches or workspaces for local changes.
5. Query, search, relate, export, or generate from local data.
6. Continue using the dataset without network access.

### Surface

Clone output, open API, branch commands, search, export, describe, dataset
metadata.

### Guarantees

A cloned dataset must be self-contained for normal use. Branching, history,
search, graph relationships, and exported metadata must behave like any other
Strata database within the supported feature set.

### Failures

Missing optional derived indexes, unsupported features in the cloned artifact,
backend mismatch, or corrupted cloned files should surface through normal open,
health, repair, or reindex paths.

### V1 Decision

Required.

### Cleanup

Treat dataset clone as a database creation path, not as a remote runtime mode.
Document any derived-state rebuild required after clone.

## Pathway 39: Choose A Storage Backend Intentionally

### Goal

A user selects local filesystem, browser/cache, object storage, or
OpenDAL-backed targets based on explicit capability errors and guarantees.

### Flow

1. Choose a database location or storage URL.
2. Strata resolves the backend adapter.
3. Strata checks required capabilities for the selected mode.
4. Strata opens, creates, or rejects the database.
5. The user sees backend capability and durability information in diagnostics.

### Surface

Open API, storage URL syntax, backend capability contract, OpenDAL adapters,
WASM/cache target, health/info output, docs.

### Guarantees

Strata must own the backend capability contract. Local filesystem is the
reference durable backend. Other backends must fail clearly when they cannot
support selected durability, atomicity, locking, timeline, or recovery
requirements.

### Failures

Unsupported scheme, missing backend feature, weak consistency, unsupported
atomic rename, missing locking, credential failure, latency timeout, and
durability mismatch should surface as explicit capability errors.

### V1 Decision

Required.

### Cleanup

Keep OpenDAL as an adapter family, not Strata's storage contract. Do not claim
all OpenDAL backends are production-ready.
