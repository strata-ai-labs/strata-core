# Strata Next-Generation Engine And Storage Charter

Status: Superseded historical charter

## Purpose

This file records the original next-generation engine/storage charter as
historical context. It is not a binding design document for V1.

The binding V1 architecture anchor is:

1. `docs/architecture/strata-v1-architecture.md`

The binding storage-next architecture anchors are:

1. `docs/architecture/storage-architecture.md`
2. `docs/architecture/storage/README.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`

The original charter text has been intentionally removed from this file because
it contained as-if-binding sections for rejected or deferred directions:

1. Object storage as the canonical first durable backend.
2. Default-shipped S3/GCS/Azure/IndexedDB/OPFS provider matrices.
3. Content-addressed BLAKE3 segment commitments.
4. Manifest-as-etag-chain as the V1 fencing primitive.
5. Branch sync as a V1 architectural layer.
6. Content-rooted branch IDs.
7. HLC/CRDT total KV merge as the V1 merge contract.
8. SyncProvider conformance as a cutover gate.

Those ideas may return only through focused future architecture documents. They
are not V1 commitments.

## Supersession Resolution

| Original charter point | V1 resolution |
| --- | --- |
| P1 storage abstraction only IO | Adopted in spirit. Direct filesystem/object IO belongs only inside storage backend implementations. The current rationale is direct `std::fs` and path-shaped IO; `memmap2` is not a current storage dependency, while `fs2` is used only behind storage-next's non-wasm `localfs` feature for the local single-writer guard. |
| P2 wasm32 first-class | Adopted for the browser/cache substrate. Durable browser persistence is not required for the first storage rewrite. |
| P3 no async public surface | Adopted for public APIs. Internal async/runtime choices are implementation details and are not committed to tokio or to a WAL-thread runtime. |
| P4 capability-typed providers | Adopted. Storage modes validate backend capabilities before durable side effects. |
| P5 object storage primitives canonical | Rejected for V1. Durable local filesystem is the reference backend. Object-store/OpenDAL durability remains architecture-aware but not the canonical first implementation. |
| P6 etag-fenced manifest chains | Deferred to future object-durable design. V1 durable local uses local writer protection plus durable publish/sync semantics. |
| P7 sync is opt-in | Adopted. No hidden network, sync, registration, telemetry, or model-provider behavior. |
| P8 content-rooted BranchId | Rejected for V1. `BranchId` remains an opaque identity. Remote/dataset/sync association metadata belongs outside the branch ID. |
| P9 total CRDT KV merge | Rejected for V1. V1 merge semantics are engine-owned and product-documented, with Strict and SourceWins as the required strategies. CRDT/HLC merge can return in a later sync design. |
| P10 branch isolation | Adopted as logical branch isolation. Storage may use COW shared immutable tables internally, but branches remain isolated product timelines. |

## V1 Replacement Decisions

The V1 documents replace the original charter with these binding decisions:

1. Durable local filesystem is the reference durable backend.
2. Cache/browser mode is explicit and non-durable.
3. OpenDAL/object storage is architecture-aware but not required as a production
   durable mode in the first rewrite.
4. Sync data movement is post-V1.
5. Storage-next owns persistence mechanics and stays primitive-agnostic.
6. Engine-next owns product semantics, merge semantics, IPC, and user-facing
   errors.
7. Core-next owns only genuinely shared vocabulary.
8. Pre-V1 development databases may be rejected by default at cutover.

## Historical Retrieval

The removed original charter text remains available in git history. If a future
design needs to revive one of the original charter ideas, it should copy only
the relevant historical text into a new focused document and reconcile it with
the V1 anchors.
