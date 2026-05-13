# L1. Backend IO

Status: V1 architecture draft

## Purpose

Backend IO is the portability layer for storage-next.

It exists so the rest of storage can be written against Strata's own object IO
contract instead of local filesystem assumptions. The first implementation must
prove two backend shapes:

1. Browser/cache backend for live demos and WASM-oriented development.
2. Local filesystem backend for the durable reference implementation.

The design should leave room for a later OpenDAL/object-store backend, but that
backend is not required for the first storage rewrite. The goal is to define the
smallest backend contract that supports cache/browser and local durable storage
well, while avoiding POSIX-only assumptions that would make object storage
impossible later.

## Core Decision

The backend layer is object-first, not filesystem-first.

Higher layers should think in terms of named objects under a database root, not
paths, directories, file descriptors, append handles, `fsync`, or POSIX rename.
The local filesystem backend can implement object operations using files and
should remain highly efficient. Browser/cache backends should not be forced to
emulate durable filesystem behavior. Future object-store backends should not
need a rewrite of the storage architecture.

## Responsibilities

Backend IO owns:

- reading an object
- reading an object byte range
- writing a full object
- appending bytes to an existing object when the backend supports WAL segment
  append
- deleting an object
- listing objects by prefix
- reading object metadata
- conditional object creation/update when supported
- durable publish/sync primitives when supported
- lock or lease primitives when supported
- backend capability declaration
- backend-local error classification

Backend IO does not own:

- database object naming
- WAL semantics
- manifest semantics
- snapshot semantics
- segment/table semantics
- branch/version semantics
- recovery policy
- engine data capability semantics
- product open policy

Object names come from L2. Bytes come from L3 and above. Backend IO only moves
opaque bytes and reports backend facts.

## Required Backend Shapes

### Browser / Cache Backend

The browser/cache backend exists for live demos and WASM-oriented development.

V1 minimum:

- in-memory object map
- synchronous storage-facing API
- read object
- read range
- write object
- delete object
- list prefix
- metadata enough for tests
- no crash durability claim
- no multi-process or cross-tab correctness claim

This backend is allowed to be cache-mode only. It does not need IndexedDB or
durable browser storage in the first storage-next rewrite.

`read_range` should be implemented directly over the in-memory object bytes.
Keeping range reads in the cache backend gives L5 one table-read contract across
cache and durable backends instead of forcing table code to carry a full-object
fallback path.

### Local Filesystem Backend

The local filesystem backend is the reference durable backend. The first V1
implementation treats Unix-like local filesystems as the durable reference
because the required publish sequence depends on atomic rename/link behavior
and durable parent-directory sync. Non-Unix local filesystem builds may compile,
but they must not advertise durable publish/sync capabilities until they provide
equivalent backend-owned primitives.

V1 minimum:

- object read/range-read/write/delete/list
- durable publish for full-object writes
- directory or parent sync where required
- database lock or equivalent single-writer guard
- clear permission, not-found, already-exists, and corruption-adjacent error
  reporting
- crash-recovery test support

This backend is the correctness baseline for durable local databases.

## Deferred Backend Shape

### OpenDAL / Object Backend

The OpenDAL/object backend is a design constraint, not a first-rewrite
requirement.

Storage-next should not bake in assumptions that make OpenDAL/S3 impossible,
but the first implementation may ship without any OpenDAL code. If an OpenDAL
adapter is added early, its minimum is:

- capability declaration
- basic object read/write/delete/list where available
- explicit `Unsupported` for unproven durable modes
- no claim that S3 durable database mode is production-supported

S3 end-to-end support is a later proof step. Object-store durability requires
the later L2-L8 designs to prove manifest fencing, WAL/snapshot behavior,
recovery, retention, and compaction economics on that backend class.

## Backend Capabilities

Every backend declares capabilities. Open must compare the requested storage
mode against those capabilities and fail fast if the backend cannot satisfy it.

Initial capability vocabulary:

- `read_object`
- `read_range`
- `write_object`
- `delete_object`
- `list_prefix`
- `object_metadata`
- `append_object`
- `conditional_create`
- `conditional_update`
- `durable_publish`
- `conditional_publish`
- `durable_sync`
- `single_writer_lock`
- `lease`
- `consistent_list`
- `monotonic_metadata`

The exact names can change during implementation. The important property is
that capabilities are explicit, inspectable, and used by open-time validation.

No backend is "supported" because the adapter compiles.

## Durable Publish Boundary

L1 should expose a backend-owned durable publish primitive, not a POSIX-shaped
sequence of `write temp`, `fsync file`, `rename`, and `fsync directory` to the
rest of storage.

Target operation shape:

```text
publish_object(object_name, bytes, publish_mode) -> PublishOutcome
```

Where `publish_mode` can express:

- create new object
- replace object
- conditional create
- conditional update against an opaque backend fence
- non-durable cache write

The local filesystem backend may implement this with unique temporary files,
file sync, atomic no-clobber link for create, atomic replace rename, and parent
directory sync. An object backend may implement it with conditional writes,
generations, etags, or multipart/object-specific publish rules. L4 consumes the
`PublishOutcome` and applies service meaning such as "manifest publish" or
"snapshot publish."

Higher storage layers should not call POSIX-shaped primitives directly.

## Storage Modes And Requirements

### Cache Mode

Cache mode requires:

- read object
- write object
- delete object
- list prefix

Cache mode does not require:

- durable sync
- crash recovery
- single-writer locking
- conditional manifest update

Cache mode must be visibly non-durable.

### Durable Local Mode

Durable local mode requires:

- read object
- read range
- write object
- append object
- delete object
- list prefix
- object metadata
- durable publish or equivalent
- durable sync or equivalent
- single-writer lock or equivalent

This mode is the reference mode for crash recovery.

### Object Durable Candidate Mode

Object durable candidate mode requires, at minimum:

- read object
- read range
- write object
- delete object
- list prefix
- object metadata
- conditional create/update or another manifest-fencing mechanism
- documented list consistency assumptions

This mode should remain experimental until L2-L8 prove the full manifest,
WAL/snapshot, recovery, and retention story on the object backend.

This mode is not required for the first storage rewrite.

## Failure Model

Backend IO should classify failures without inventing product meaning.

Expected backend error categories:

- not found
- already exists
- precondition failed
- permission denied
- invalid object name
- unsupported operation
- capability mismatch
- transient unavailable
- interrupted operation
- checksum or metadata mismatch if detected at this layer
- backend corruption if the backend returns impossible metadata
- unknown backend error

Higher layers can map these into storage-specific recovery or lifecycle errors.
Engine maps storage errors into product-facing errors.

## Concurrency Model

Backend IO should not assume multi-writer correctness.

Durable database open should require a single-writer guard or a manifest-fencing
protocol appropriate for the selected backend. If neither exists, durable open
must fail. Cache mode may run without a writer lock because it makes no durable
multi-process promise.

For object backends, conditional manifest update is the likely fencing primitive.
For local filesystem, a lock file or platform lock is the likely primitive.
The backend layer exposes capabilities; higher storage layers choose protocols.

## Sync And Durability

Backend IO must be honest about sync.

Local filesystem can expose durable sync primitives. Browser/cache cannot. S3
does not expose `fsync`; durability must be expressed through object-store
commit semantics, metadata, and manifest fencing.

The backend layer should not fake `fsync` on backends that do not have an
equivalent.

## Public Runtime Shape

The storage-facing backend API should not leak an async runtime to engine-next
or upper storage layers.

Adapters may use backend-specific internals. If a future OpenDAL adapter needs
async internally, that runtime choice must remain inside the adapter or behind a
feature-gated implementation detail. The storage-next contract should stay
embeddable and runtime-neutral.

## Testing Requirements

Backend IO needs a conformance suite.

Minimum tests:

1. Write then read an object.
2. Read a byte range.
3. Delete an object.
4. List by prefix.
5. Metadata changes after write.
6. Conditional create succeeds once and fails on existing object.
7. Conditional update fails when metadata/fence is stale.
8. Unsupported operations return `Unsupported`, not success.
9. Capability mismatch is detected before higher layers run.
10. Object names are treated as opaque backend object names from L2.

Fault tests:

1. Failed read.
2. Failed write.
3. Failed delete.
4. Failed list.
5. Partial write if the backend/fault harness can simulate it.
6. Stale metadata.
7. Precondition failure.
8. Permission failure.

The memory/browser backend should run the same conformance tests except for
durability-specific requirements. The local filesystem backend should run the
full durable conformance suite. If an OpenDAL backend is added before S3
durable mode is complete, it should run capability and unsupported-mode tests.

## V1 Minimum

The first storage-next implementation needs:

1. A backend trait or equivalent backend contract.
2. A memory/cache backend.
3. A local filesystem backend.
4. A capability model.
5. Open-time capability validation.
6. A backend conformance test suite.
7. A fault-injection backend wrapper.

The first implementation does not need:

1. OpenDAL adapter code.
2. Production S3 durable mode.
3. IndexedDB durable browser mode.
4. Multi-writer object-store protocol.
5. Background sync.
6. Provider plugin loading.
7. Provider-specific tuning APIs.

## Open Questions

1. Should the backend contract expose one conditional write operation or
   separate conditional create and conditional update operations?
2. What exact metadata token should higher layers use: etag, generation,
   version ID, or a storage-owned opaque fence?
3. Should local filesystem emulate object-store conditional update through a
   sidecar metadata file, lock-held compare, or both?
4. How much list consistency would storage require from future
   OpenDAL-backed providers?
5. Should cache mode use the same object-name validation as durable backends?
6. Is range read required for all backends, or can small-object backends emulate
   it by full reads?

## Next Layer Dependency

L2 object layout consumes this layer. It should define canonical object names
without assuming directories, path separators beyond the object-name convention,
file descriptors, append handles, or POSIX rename.
