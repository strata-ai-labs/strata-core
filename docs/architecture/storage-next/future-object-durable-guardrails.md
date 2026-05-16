# Future Object-Durable And Compute/Storage Separation Guardrails

Status: Background architecture guardrail

## Purpose

V1 is embedded-first and uses durable local filesystem storage as the reference
durable backend. A later serverless deployment may need S3/R2/OpenDAL-backed
storage, horizontally scalable compute, and compute nodes that can attach to and
detach from durable storage independently.

This document is not an implementation plan for that future mode. It records
the structural constraints storage-next should preserve while building the V1
embedded path, so V1 code does not accidentally couple compute lifecycles to
today's local durable object layout.

## Non-Goals

- Do not implement production S3/R2/OpenDAL durability for V1.
- Do not require a multi-writer distributed commit protocol during the embedded
  era.
- Do not delay durable-local implementation waiting for object-store WAL,
  leasing, or manifest-fencing designs.
- Do not expose object-store details to engine-next as a shortcut.

## Core Guardrail

Compute must attach to a storage runtime through L9/L8/L7 contracts, not through
lower-level storage objects.

The durable substrate may later change from local WAL segments and local
manifest publication to object-store commit chunks, conditional manifests,
leases, or generation-fenced pointers. Engine and product compute should not
need to know which durable shape is in use.

## Structural Invariants

1. Engine-next consumes storage through L9. It must not import WAL records,
   manifest services, table object names, backend handles, object layout
   constructors, or publish primitives during normal production operation.
2. L8 owns open, recovery, checkpoint, compaction, pruning, repair, and close
   orchestration. Engine may choose product policy, but it should not sequence
   lower-level storage objects directly.
3. L7 owns commit ordering and exposes storage-mechanical commit outcomes. Its
   contract should be "make this commit durable before visible when the selected
   mode requires it," not "append to local segment N at offset X."
4. L4 owns durable service mechanics. Future object-durable mode may implement
   commit durability with immutable chunks and conditional publication instead
   of local append/sync, without changing engine-facing APIs.
5. L5 and L6 must treat table objects as immutable artifacts plus reachability
   facts. They must not depend on local paths, file descriptors, mmap handles,
   or process-local table files as authoritative durable identity.
6. Backend-specific facts are allowed below L1/L4 and in typed diagnostics, but
   normal compute code must not branch on POSIX rename, fsync, S3 ETags, R2
   behavior, or OpenDAL provider names.
7. Local caches are disposable. A compute node may cache blocks, tables, decoded
   manifests, or read views, but durable object storage remains authoritative.
8. Storage modes stay explicit. Future object-durable mode is a separate
   storage mode with its own capability requirements and conformance tests; it
   must not be hidden behind durable-local mode.

## Design Pressure For V1 Work

V1 implementation may optimize for embedded durable-local performance, but new
M4-L9 code should preserve these seams:

- Commit APIs should accept commit batches and return commit outcomes, not WAL
  offsets or local file facts.
- Recovery APIs should return typed recovery facts and health, not paths that
  engine must inspect.
- Table publication should flow through L4 services and branch reachability
  manifests, not direct backend writes from L5/L6 callers.
- Maintenance should be invoked through L8 runtime operations, not by engine
  deleting or rewriting storage objects.
- Capability checks should remain the gate for storage-mode selection.

## Future Object-Durable Questions

When object-durable mode becomes active work, it needs its own focused design
for at least:

1. Commit durability primitive: immutable commit objects, WAL chunks, or another
   object-store-native record shape.
2. Visibility fencing: conditional manifest update, generation/ETag fences,
   lease service, or an external coordinator.
3. Listing assumptions: what consistency is required, what is only an
   accelerator, and how recovery avoids inventing state from stale listings.
4. Checkpoint and compaction economics: object counts, multipart thresholds,
   garbage collection, and safe retention proofs.
5. Compute attach/detach behavior: cache invalidation, stale manifest handling,
   health reporting, and recovery classification.

These questions are future work. The V1 requirement is only that embedded-era
code does not make them impossible.

## Review Checklist

When adding M4-L9 code, treat these as review blockers:

- Does engine-next import an L1-L4 object/service type for normal production
  behavior?
- Does a public or engine-facing API expose local paths, WAL offsets, segment
  filenames, fsync/rename facts, or backend provider details as required inputs?
- Does L5/L6 write backend objects directly instead of going through L4?
- Does L7 assume the durable commit primitive must be append-based?
- Does L8 require compute and durable storage to be created, checkpointed, or
  destroyed as one inseparable lifecycle unit?
- Does cache or local durable behavior silently stand in for future
  object-durable semantics without a distinct storage mode?

If the answer is yes, the design is coupling compute to the current lower-level
storage shape and should be revised before the slice closes.
