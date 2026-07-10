# M3E4 Implementation Brief: Quarantine Service And Recovery Classifications

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Test-suite plan:
`docs/architecture/implementation-plans/m3e4-quarantine-recovery-test-suite-plan.md`

## Goal

Implement the storage-next quarantine service mechanics and the recovery-facing
classification facts needed by later lifecycle layers.

M3E4 is the first V1 quarantine slice. It should create a small durable service
surface that can:

1. Record a branch-local quarantine inventory durably.
2. Move an unreachable storage object into the quarantine namespace through
   portable backend operations.
3. Purge quarantined objects only after a fresh safe proof.
4. Reconcile quarantine inventory and quarantine objects during recovery.
5. Return typed recovery classifications when inventory and object state do not
   agree.

This is still a storage-mechanical layer. M3E4 must not choose table-compaction
policy, prove table reachability itself, run branch retention, or expose a
product API.

## Inputs Read

Architecture and plan inputs:

1. `docs/architecture/storage/l1-backend-io.md`
2. `docs/architecture/storage/l2-object-layout.md`
3. `docs/architecture/storage/l3-durable-format-codec.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
6. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
7. `docs/architecture/storage/implementation-patterns.md`
8. `docs/spec/strata-storage-format-v1.md`
9. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
10. `docs/architecture/implementation-plans/m3-porting-log.md`

Current implementation evidence:

1. `crates/storage/src/quarantine.rs`
2. `crates/storage/src/segmented/quarantine_protocol.rs`
3. `crates/storage/src/segmented/compaction.rs`
4. `crates/storage/src/segmented/recovery.rs`
5. `crates/storage/src/segmented/tests/quarantine_*`, if present.

Storage-next implementation inputs:

1. `crates/storage-next/src/backend/mod.rs`
2. `crates/storage-next/src/backend/local_fs.rs`
3. `crates/storage-next/src/backend/memory.rs`
4. `crates/storage-next/src/layout/mod.rs`
5. `crates/storage-next/src/format/mod.rs`
6. `crates/storage-next/src/service/publish.rs`
7. `crates/storage-next/src/service/manifest.rs`
8. `crates/storage-next/src/service/snapshot.rs`
9. `crates/storage-next/src/service/sidecar.rs`
10. `crates/core-next/src/lib.rs`

## Existing Behavior To Preserve

1. Quarantine is a safety buffer between "not referenced now" and "deleted
   forever."
2. Reclaim is blocked when recovery is unsafe or reachability proof is
   incomplete.
3. Current storage publishes quarantine inventory before moving the candidate
   object, so recovery has a durable fact for in-flight reclaim.
4. Purge is separate from quarantine. Moving an object into quarantine must not
   immediately delete it.
5. Purge rewrites the inventory with entries whose delete failed instead of
   hiding partial failure.
6. Reopen reconciliation treats disagreement between inventory and actual
   quarantine objects as degraded policy state, not as a clean database.
7. Recovery prefers retention over deletion when quarantine state is ambiguous.
8. Absence of a quarantine inventory is a healthy empty state only when no
   quarantine objects exist for that branch.

## Intentional V1 Changes

1. The old local path format (`quarantine.manifest` plus `__quarantine__/`) is
   current-code evidence only. V1 quarantine names come from `ObjectLayout`:
   `quarantine/<branch-id>/manifest` and
   `quarantine/<branch-id>/<object-id>`.
2. V1 uses a storage-format quarantine inventory codec. It does not read or
   write the old storage crate's `STRAQRTN` bytes.
3. The backend abstraction has no portable rename primitive. V1 quarantine
   therefore uses a portable copy/delete shape: publish inventory, publish the
   quarantine object, then delete the source object. Local filesystem may later
   optimize this internally, but the service contract must not require rename.
4. Quarantine object ids are branch-local opaque storage ids. They are not table
   names, file paths, segment ids, or product ids.
5. Source-family information is derived from `source_object` when the
   inventory is decoded or reported. The inventory does not store a redundant
   source-family field. Quarantined objects stay under the global
   `quarantine/` family and are not placed back under their source object
   family.
6. M3E4 accepts caller-supplied reachability and recovery-safety facts. L6/L8
   later compute those facts from branch manifests, live layers, inherited
   layers, and recovery health.
7. Reconciliation classifies mismatches and preserves bytes. It does not
   silently delete unknown quarantine objects or automatically repair corrupt
   inventory.
8. Cache mode has no durable quarantine objects. Memory backend tests may
   exercise unsupported behavior, but cache lifecycle must not wire quarantine
   persistence into runtime state.

## Target Files

Implementation files:

1. `crates/storage-next/src/format/quarantine.rs`
2. `crates/storage-next/src/format/mod.rs`
3. `crates/storage-next/src/service/quarantine.rs`
4. `crates/storage-next/src/service/mod.rs`
5. `crates/storage-next/src/layout/mod.rs`, only if the existing quarantine
   constructors are insufficient.
6. `crates/storage-next/src/testkit/service_fuzz.rs`, only for reusable fuzz
   operation models.

Test files:

1. Module-local tests in `crates/storage-next/src/format/quarantine.rs`.
2. Module-local tests in `crates/storage-next/src/service/quarantine.rs`.
3. Private child modules under `crates/storage-next/src/service/quarantine/`
   if the service file crosses the file-size review threshold.
4. `crates/storage-next/fuzz/fuzz_targets/format_quarantine.rs`.
5. `crates/storage-next/fuzz/fuzz_targets/service_quarantine.rs`.

Documentation files:

1. `docs/architecture/implementation-plans/m3-porting-log.md` must receive an
   M3E4 source-map note before production code changes.
2. `docs/spec/strata-storage-format-v1.md` must document the quarantine
   inventory bytes in the same slice that introduces the codec.
3. `docs/architecture/storage/l2-object-layout.md` should be touched only
   if object names change.
4. `docs/architecture/v1-progress-tracker.md` should be updated only after
   each M3E4/M3TC4 slice is implemented and verified.

## Inventory Format Shape

Exact Rust names can adjust during implementation, but the durable facts should
stay minimal and branch-local.

Required inventory header facts:

1. Magic and format version.
2. Database id as 16 raw bytes.
3. Branch id as 16 raw `BranchId` bytes.
4. Codec id.
5. Entry count.
6. Header and payload CRC coverage consistent with the other M3 codecs.

Required entry facts:

1. `object_id`: branch-local quarantine object id used in
   `quarantine/<branch-id>/<object-id>`.
2. `source_object`: original `ObjectName`.
3. `byte_count`: source byte count verified against `ObjectMetadata` during
   durable quarantine.
4. `quarantined_at`: caller-supplied `Timestamp`.

Validation rules:

1. Durable inventory stores branch id as raw bytes. Service paths must derive
   branch path text from `BranchId` canonical lowercase UUID display and must
   be valid for `ObjectLayout::branch_quarantine_prefix`.
2. Object id must be valid for `ObjectLayout::quarantine_object`.
3. Object id `manifest` is reserved for the branch quarantine inventory and
   must be rejected as a quarantine object id.
4. Source object must be a valid `ObjectName` and must not be inside the
   `quarantine/` family.
5. `ObjectFamily::from_object_name(source_object)` must return a known
   non-quarantine family.
6. Duplicate object ids are rejected.
7. Duplicate source objects are rejected.
8. Entry order is canonical by object id, then source object.
9. Empty inventory is valid.

## Service Shape

### Construction

1. Constructed from `&dyn Backend`.
2. Uses `ObjectLayout::quarantine_manifest`,
   `ObjectLayout::quarantine_object`, and
   `ObjectLayout::branch_quarantine_prefix` for all names.
3. Requires read, list, delete, object metadata, durable publish, and durable
   sync capabilities for durable mutation paths.
4. Optional load may run with read capability only.
5. Listing/reconciliation requires list plus read.
6. Durable quarantine and purge must fail before mutation when required
   capabilities are missing.

### Inventory Operations

1. `load_inventory(branch_id, database_id, codec_id)` returns an empty
   inventory when the manifest object is absent.
2. Corrupt inventory bytes return a typed decode error, not empty state.
3. Database, branch, and codec mismatches return typed mismatch errors.
4. `publish_inventory_replace` uses the durable publisher replace path.
5. Publishing an empty inventory is valid and represents an explicit drained
   state.
6. Inventory results include manifest object name, branch id, entry count, byte
   count, and durable publish facts where available.

### Gate Facts

The quarantine service does not compute reachability. Callers must pass a gate
fact for operations that can move or delete bytes.

Required gate outcomes:

1. `Safe`: operation may proceed.
2. `Referenced`: candidate is still reachable.
3. `UnsafeRecovery`: recovery health cannot support reclaim.
4. `ProofIncomplete`: reachability proof could not cover all live and
   inherited layers.

All non-`Safe` gate outcomes fail before backend mutation.

### Quarantine Object Operation

The operation should be idempotent for already-complete quarantine state and
conservative for ambiguous state.

Required sequence:

1. Validate branch id, object id, source object, timestamp, codec id, and gate.
2. Reject source objects already under `quarantine/`.
3. Reject `Timestamp::EPOCH` unless the caller explicitly marks the request as
   using an epoch timestamp; the service must not invent wall-clock time.
4. Load and validate current inventory.
5. If inventory contains the entry and the quarantine object is present while
   the source object is absent, return `AlreadyQuarantined` without mutating
   the backend.
6. If inventory contains the entry, the quarantine object is present, and the
   source object is still present, validate byte-for-byte equality between
   source bytes and quarantine bytes. If they match, retry source deletion
   without republishing inventory or quarantine bytes. If they differ, return
   an inventory-mismatch classification before mutation.
7. If inventory contains the entry but the quarantine object is missing, return
   an inventory-mismatch classification before mutation.
8. If the quarantine object exists but inventory does not name it, return an
   unlisted-object classification before mutation.
9. Read source object bytes and metadata for a new quarantine request.
10. Verify source metadata size matches the source bytes read before
   publishing inventory.
11. Publish the updated inventory durably.
12. Publish the quarantine object with durable create.
13. Delete the source object only after the quarantine object publication
   returns durable success.
14. Return a report that distinguishes:
    - fully quarantined and source deleted
    - already quarantined with source absent
    - quarantine copy already present and source delete retried
    - quarantined but source delete failed
    - inventory published but quarantine object publish failed
    - inventory publication failed before object movement
    - visibility or durability uncertainty

Crash-window rule:

1. Every failure window must be recoverable by inventory/object
   reconciliation. The operation must never delete the source object before a
   quarantine copy is proven durable.

### Purge Operation

1. Requires a fresh `Safe` gate.
2. Loads and validates inventory first.
3. Deletes listed quarantine objects only.
4. Does not delete adjacent-family objects.
5. Records successful deletes, already-missing entries, protected entries, and
   failed deletes separately.
6. Treats a missing quarantine object during purge as already drained, matching
   the old storage behavior, and removes that entry from the rewritten
   inventory.
7. Rewrites inventory with entries whose delete failed or whose state remained
   ambiguous.
8. Publishing the rewritten inventory can fail independently from object
   deletes; the report must preserve which objects were already deleted.
9. Empty final inventory is valid and may remain as
   `quarantine/<branch-id>/manifest`.

### Reconciliation

Recovery reconciliation should inspect, classify, and preserve.

The service should expose both a branch-local reconciliation path and a
quarantine-family reconciliation path. Branch-local reconciliation consumes a
known `BranchId`. Family reconciliation lists the global `quarantine/` family,
parses branch components, and classifies malformed branch directories that a
branch-local call could not see.

Required classifications:

1. `CleanEmpty`: no inventory and no quarantine objects.
2. `CleanInventory`: inventory entries and listed quarantine objects agree.
3. `CorruptInventory`: inventory object exists but cannot be decoded.
4. `UnlistedQuarantineObject`: object exists under the branch quarantine prefix
   but no inventory entry names it.
5. `MissingQuarantineObject`: inventory entry names an object that is absent.
6. `MalformedListedObject`: object under quarantine family has an invalid
   branch or object id.
7. `BackendUnavailable`: backend read/list/metadata error prevents
   classification.

Classification rules:

1. `CleanEmpty` and `CleanInventory` are healthy.
2. `CorruptInventory`, `UnlistedQuarantineObject`,
   `MissingQuarantineObject`, and `MalformedListedObject` map to
   policy-downgrade recovery facts.
3. `BackendUnavailable` maps to unavailable recovery facts.
4. Reconciliation does not delete, rewrite, or repair by default.
5. Repair can be a later L8 operation with its own proof and audit trail.

## Error Shape

M3E4 should introduce a small service error enum with operation, object, and
source precision.

Required error families:

1. Invalid request.
2. Missing inventory or source object where required.
3. Inventory decode failure.
4. Inventory encode failure.
5. Database id mismatch.
6. Branch id mismatch.
7. Codec mismatch.
8. Invalid or unknown source object family.
9. Unsupported backend capability.
10. Backend read/list/metadata/delete failure.
11. Durable publish failure.
12. Unsafe reclaim gate.
13. Inventory mismatch classification.
14. Visibility or durability uncertainty.

Every error should carry enough facts for L8 to map it into the global V1 error
registry without string matching.

## Comments And Porting Discipline

1. Add comments around crash-window ordering and reconciliation choices. These
   are the highest-risk parts of the service and should be readable without
   reconstructing the old storage implementation.
2. Comments should explain durable ordering, not restate individual lines.
3. Do not write milestone labels such as `M3E4` in production file names,
   function names, comments, or error names.
4. Before production code changes, add the old-to-new source map and the
   intentional V1 changes to `m3-porting-log.md`.
5. If a storage-next behavior differs from old storage, the brief or porting
   log must say why.

## Implementation Order

### Slice A: Source Map And Inventory Codec

1. Record the old quarantine source map in `m3-porting-log.md`.
2. Add `format/quarantine.rs`.
3. Add inventory encode/decode, validation, golden vector, and malformed-byte
   tests.
4. Update `docs/spec/strata-storage-format-v1.md`.

### Slice B: Inventory Service

1. Add `service/quarantine.rs`.
2. Implement construction, capability checks, inventory load, optional load,
   required load, and durable replace.
3. Add precise error variants and comments around empty-vs-corrupt state.

### Slice C: Quarantine And Purge Mechanics

1. Implement gate validation.
2. Implement portable inventory-publish, quarantine-object-publish, and
   source-delete sequence.
3. Implement purge and partial-failure reports.
4. Add comments around publish-before-object and copy-before-delete windows.

### Slice D: Reconciliation And Recovery Classifications

1. Implement branch quarantine listing and classification.
2. Add DTOs for healthy, policy-downgraded, and unavailable quarantine facts.
3. Wire comments and documentation so L8 can consume the classifications
   without guessing storage internals.

## Non-Goals

1. No L6 table reachability proof.
2. No L8 retention scheduler.
3. No compaction integration.
4. No public CLI maintenance command.
5. No object-store fencing or distributed reclaim.
6. No automatic repair of corrupt quarantine inventory.
7. No compatibility reader for the old storage crate's quarantine bytes.
8. No source-family-specific quarantine directories.

## Exit Gate

M3E4 is complete when:

1. Quarantine inventory bytes are specified, golden-tested, and fuzzed.
2. The quarantine service exposes durable inventory, quarantine, purge, and
   reconcile mechanics over storage-next backends.
3. Unsafe or incomplete proof facts stop mutation before backend access.
4. Publish/delete fault windows preserve enough state for deterministic
   recovery classification.
5. Cache mode does not create durable quarantine state.
6. The porting log records what was preserved, what changed, and why.
