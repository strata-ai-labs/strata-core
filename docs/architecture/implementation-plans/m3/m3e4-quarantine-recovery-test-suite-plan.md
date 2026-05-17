# M3E4 / M3TC4 Test Suite Plan: Quarantine And Recovery Classification

Status: test-suite plan

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Implementation brief:
`docs/architecture/implementation-plans/m3e4-quarantine-recovery-implementation-brief.md`

## Goal

Bring the quarantine service to reference-grade coverage before M4/M6/M8 depend
on it for safe reclaim, recovery health, and maintenance reporting.

Quarantine exists because immediate deletion is too dangerous when recovery,
retention, or reachability facts might be stale. The tests must therefore prove
the hard cases: unsafe proof stops mutation, inventory/object disagreement is
classified, delete failures preserve inventory, and every crash window leaves a
state that recovery can explain.

## Testing Principles

1. Quarantine tests model physical storage mechanics, not table policy.
2. Reachability proof is an input fact in M3E4 tests. Tests must not invent an
   L6 table scanner.
3. No source object is deleted before a quarantine object is proven durable.
4. Inventory corruption is not empty state.
5. Unknown quarantine objects are retained and classified, not silently
   deleted.
6. Purge requires a fresh safe gate even if the object was previously
   quarantined safely.
7. Every required test family needs a sensitivity probe: temporarily mutate the
   implementation so at least one test fails for the intended reason.
8. Test labels such as `M3TC4` belong in docs and tracker entries only. They
   must not appear in production file names, type names, comments, or error
   names.

## Scope

In scope:

1. Quarantine inventory codec.
2. Quarantine inventory service load and publish.
3. Quarantine object operation over backend read, publish, and delete.
4. Purge operation and partial-failure reports.
5. Reconciliation over inventory and branch quarantine prefix listings.
6. Recovery classification DTOs returned by the quarantine service.
7. Local filesystem durable behavior.
8. Memory/cache backend rejection of durable quarantine mutation.
9. Format fuzz and service-level operation fuzz.

Out of scope:

1. L6 reachability proof.
2. L8 retention policy.
3. Background maintenance loops.
4. Public maintenance commands.
5. Process-kill crash tests.
6. Object-store distributed fencing.
7. Compatibility with old `STRAQRTN` quarantine manifests.

## Current Coverage

Already covered by earlier M3 work:

1. M3B layout tests cover `quarantine/<branch-id>/<object-id>` and
   `quarantine/<branch-id>/manifest` constructors.
2. M3D/M3TC1 durable publisher tests cover local filesystem temp write, sync,
   rename, parent sync, create, replace, and cleanup behavior.
3. M3E1/M3TE1 manifest tests cover durable replace and publish uncertainty.
4. M3E3/M3TC3 snapshot tests cover recovery classification style for optional
   sidecar state.
5. M3TB format fuzz infrastructure exists and must be reused.

Baseline gaps before M3TC4:

1. No quarantine inventory codec exists in storage-next.
2. No quarantine service exists.
3. No service-level tests prove unsafe proof stops mutation.
4. No test proves inventory publish happens before source deletion.
5. No test classifies inventory/object disagreement.
6. No service fuzz target exercises quarantine operation streams.

## Target Test Files

Primary files:

1. `crates/storage-next/src/format/quarantine.rs`
2. `crates/storage-next/src/service/quarantine.rs`
3. `crates/storage-next/src/service/quarantine/tests.rs`, if needed.
4. `crates/storage-next/src/service/quarantine/tests/support.rs`, if needed.

Fuzz files:

1. `crates/storage-next/fuzz/fuzz_targets/format_quarantine.rs`
2. `crates/storage-next/fuzz/fuzz_targets/service_quarantine.rs`
3. `crates/storage-next/src/testkit/service_fuzz.rs`, only for reusable
   operation models.

The default should remain module-local tests. Expose testkit helpers only for
fuzz or integration coverage that cannot be expressed module-locally.

## Test Families

### 1. Inventory Format And Golden Tests

Required cases:

1. Empty inventory encodes and decodes exactly.
2. Single-entry inventory round trips all facts.
3. Multiple entries are encoded in canonical object-id/source-object order.
4. Golden vector locks the empty inventory with fixed database id, branch id,
   codec id, and timestamp facts.
5. Golden vector locks a representative multi-entry inventory.
6. Future version is rejected.
7. Pre-V1 development version is rejected if any exists.
8. Bad magic is rejected.
9. Header CRC or container CRC mismatch is rejected.
10. Truncated header is rejected.
11. Truncated entry header is rejected.
12. Truncated object id is rejected.
13. Truncated source object is rejected.
14. Trailing bytes are rejected.
15. Empty object id is rejected.
16. Object id with separator is rejected.
17. Object id with invalid object-name bytes is rejected.
18. Object id `manifest` is rejected because it collides with the branch
    quarantine inventory object.
19. Object id valid as a component but overlong in the assembled quarantine
    object name is rejected with the assembled-name error path.
20. Source object under `quarantine/` is rejected.
21. Source object with unknown family is rejected.
22. Duplicate object ids are rejected.
23. Duplicate source objects are rejected.
24. Oversized entry count relative to bytes is rejected before allocation.
25. Raw durable inventory stores the branch id as 16 bytes, so invalid branch
    UUID text is not an inventory-codec case. Text fixtures and request
    builders must rely on the core-next `BranchId::parse_str` contract until a
    quarantine text request builder exists.

Exit gate:

1. Durable quarantine inventory bytes cannot drift without spec and golden
   updates.

### 2. Construction And Capability Tests

Required cases:

1. Service can be constructed over memory backend for optional inventory load.
2. Optional inventory load on absent memory state returns empty.
3. Required inventory load distinguishes absent and corrupt inventory.
4. Durable inventory publish on memory backend returns unsupported durable
   publish.
5. Quarantine object operation requires read capability before source access.
6. Quarantine object operation requires object metadata capability before
   recording and verifying byte count.
7. Quarantine object operation requires durable publish capability before
   inventory mutation.
8. Quarantine object operation requires delete capability before source delete.
9. Purge requires delete capability before deleting any object.
10. Reconcile requires list capability before parsing object names.

Exit gate:

1. Unsupported backend modes fail at the service boundary and never pretend to
   provide crash-safe quarantine.

### 3. Inventory Service Tests

Required cases:

1. Missing inventory object loads as empty.
2. Corrupt inventory object returns decode error.
3. Backend read failure other than `NotFound` returns typed read error.
4. Database id mismatch returns typed mismatch.
5. Branch id mismatch returns typed mismatch.
6. Codec id mismatch returns typed mismatch.
7. Publish replace creates a missing inventory object.
8. Publish replace replaces an existing inventory object.
9. Publish replace preserves old bytes on no-visible-replacement failures.
10. Publish uncertainty returns typed uncertainty without returning
    `QuarantineInventoryWrite` facts. Tests must cover both old-inventory-still-
    visible and replacement-inventory-visible outcomes.
11. Empty inventory publish is valid.
12. Inventory result includes branch id, object name, entry count, byte count,
    and publish facts.

Publish failure matrix:

1. `Unsupported`.
2. `PreconditionFailed`.
3. `FailedBeforeVisibility`.
4. `VisibilityUnknown`.
5. `VisibleDurabilityUnconfirmed`.

Exit gate:

1. Inventory load and publish distinguish absent, corrupt, mismatch, backend,
   no-visible failure, and visibility-uncertain states.

### 4. Gate And Request Validation Tests

Required cases:

1. Safe gate allows the operation to reach backend access.
2. Referenced gate fails before backend access.
3. Unsafe-recovery gate fails before backend access.
4. Proof-incomplete gate fails before backend access.
5. Empty branch id is not a quarantine request-service case because requests
   carry a typed `BranchId`; text parsing stays in the core atom and
   reconciliation path parsing tests.
6. Invalid branch id separators are not a quarantine request-service case for
   the same reason; malformed durable path text is covered by reconciliation
   tests.
7. Uppercase or non-canonical branch path text discovered during reconciliation
   is rejected as malformed; service-generated paths use `BranchId` display.
8. Empty object id is rejected before backend access.
9. Invalid object id separator is rejected before backend access.
10. Object id `manifest` is rejected before backend access.
11. Source object under quarantine family is rejected before backend access.
12. Source object with unknown family is rejected before backend access.
13. `Timestamp::EPOCH` without the explicit epoch-allowed request flag is
    rejected before backend access.
14. Duplicate inventory entry with missing object returns mismatch before
    mutation.
15. Existing quarantine object without an inventory entry returns mismatch
    before mutation.

Exit gate:

1. No unsafe or malformed reclaim request mutates backend state.

### 5. Quarantine Object Fault-Window Tests

Required success cases:

1. A new quarantine request reads the source object exactly once for bytes.
2. Source metadata byte count is recorded in inventory.
3. Inventory is durably published before quarantine object publish.
4. Quarantine object is published under `quarantine/<branch-id>/<object-id>`.
5. Source object is deleted only after quarantine object publication returns
   durable success.
6. Result reports source object, quarantine object, byte count, entry count,
   and delete outcome.

Required failure cases:

1. Source missing before inventory publish returns missing and mutates nothing.
2. Source read failure returns read error and mutates nothing.
3. Metadata failure returns metadata error and mutates nothing.
4. Metadata size mismatch returns typed backend-state error and mutates
   nothing.
5. Current inventory decode failure mutates nothing.
6. Inventory publish `Unsupported` mutates no quarantine or source object.
7. Inventory publish `PreconditionFailed` mutates no quarantine or source
   object.
8. Inventory publish `FailedBeforeVisibility` mutates no quarantine or source
   object.
9. Inventory publish `VisibilityUnknown` returns uncertainty and does not
   publish or delete source.
10. Inventory publish `VisibleDurabilityUnconfirmed` returns uncertainty and
   does not publish or delete source.
11. Quarantine object publish fails before visibility after inventory publish:
    source remains, quarantine object absent, reconcile reports missing
    quarantine object.
12. Quarantine object publish `VisibilityUnknown` with no visible quarantine
    object: source remains and reconcile reports `MissingQuarantineObject`.
13. Quarantine object publish `VisibilityUnknown` with a visible quarantine
    object: source remains and reconcile reports `CleanInventory` for the
    quarantine namespace.
14. Quarantine object publish visible but durability unconfirmed: source is not
    deleted and the result is classified as durability-unconfirmed.
15. Source delete failure after quarantine object publish reports quarantined
    but source-delete-failed state.
16. Source delete `NotFound` after quarantine object publish is reported
    separately from successful delete.
17. Re-running an already-complete quarantine request with source absent
    returns idempotent already-quarantined state.
18. Re-running after source delete failure validates source/quarantine bytes
    match and retries source deletion without republishing inventory.
19. Re-running with inventory entry, visible quarantine object, and differing
    source/quarantine bytes returns mismatch before mutation.

Exit gate:

1. Every publish/delete failure leaves either previous durable state or a state
   that reconciliation classifies without guessing.

### 6. Purge Tests

Required cases:

1. Safe gate and empty inventory reports no work.
2. Non-safe gates fail before delete.
3. Purge deletes only objects listed in inventory.
4. Adjacent-family objects are not deleted.
5. Unknown quarantine objects not listed in inventory are not deleted by purge.
6. Delete successes are reported.
7. Delete `NotFound` is reported as already missing and removed from the
   rewritten inventory.
8. Delete backend failure keeps the entry in rewritten inventory.
9. Multiple delete failures keep only failed entries.
10. Rewritten inventory publish failure reports which quarantine objects were
    already deleted.
11. Empty final inventory is valid and reloads as empty.
12. Purge report ordering is deterministic by object id.

Exit gate:

1. Purge never expands deletion scope beyond the inventory and never hides
   partial failure.

### 7. Reconciliation And Classification Tests

Required cases:

1. No inventory and no objects returns `CleanEmpty`.
2. Empty inventory and no objects returns `CleanInventory`.
3. Inventory entries and matching objects return `CleanInventory`.
4. Inventory object corrupt returns `CorruptInventory`.
5. Inventory entry points to missing quarantine object returns
   `MissingQuarantineObject`.
6. Quarantine object with no inventory entry returns
   `UnlistedQuarantineObject`.
7. Malformed object id under branch quarantine prefix returns
   `MalformedListedObject`.
8. Malformed branch id under the global quarantine family returns
   `MalformedListedObject` from family reconciliation.
9. Adjacent family names matched by backend prefix behavior are ignored unless
   the first path component is exactly `quarantine`.
10. Backend list failure returns `BackendUnavailable`.
11. Backend read failure for inventory returns `BackendUnavailable` unless it
    is `NotFound`.
12. Classification includes branch id, manifest object, listed object names,
    missing object ids, unlisted object ids, and corrupt source facts.
13. Reconcile performs no write, delete, or publish operations.

Exit gate:

1. Recovery can map quarantine state to healthy, policy-downgraded, or
   unavailable without string matching.

### 8. Cache-Mode Absence Tests

Required cases:

1. Cache mode open path does not create quarantine manifest objects.
2. Cache mode open path does not create quarantine object prefixes.
3. Cache mode close path does not create quarantine objects.
4. Cache mode maintenance path does not call quarantine durable publish.
5. Memory backend may load empty quarantine state but durable mutation returns
   unsupported capability.

Exit gate:

1. Cache mode remains explicitly non-durable for quarantine.

### 9. State-Machine Property Test

Model:

1. Operation stream length: 1 to 96.
2. Branch ids: 1 to 8 canonical branch strings.
3. Object ids: 1 to 32 valid object ids.
4. Source objects: generated valid non-quarantine object names.
5. Payload sizes: 0 to 4096 bytes, with occasional exact boundary values and a
   total generated payload cap of 64 KiB per case.
6. Gates: safe, referenced, unsafe recovery, proof incomplete.
7. Faults: inventory publish failure, quarantine publish failure, source delete
   failure, inventory corruption, unlisted object insertion.

Operations:

1. Seed source object.
2. Quarantine object.
3. Purge branch.
4. Corrupt inventory.
5. Insert unlisted quarantine object.
6. Delete quarantine object behind inventory.
7. Reconcile branch.
8. Load inventory.

Invariants after every operation:

1. Model-visible source object is never absent unless a quarantine object is
   proven durable or the source was explicitly seeded absent.
2. Quarantine-side identifiers derived from inventory entries never point
   outside the branch quarantine namespace; `source_object` remains in its
   original non-quarantine family.
3. Reconcile classification matches model mismatch state.
4. Purge deletes only model inventory entries.
5. Non-safe gates do not change model or backend state.
6. Errors are typed as request, capability, backend, publish, decode, mismatch,
   gate, or uncertainty.
7. No operation panics.

Implementation guidance:

1. Use hand-rolled `proptest`, not `proptest-state-machine`, unless the first
   implementation becomes unreadable.
2. Check failing seeds into
   `crates/storage-next/proptest-regressions/quarantine_state_machine.txt` if
   any seed fails during development or CI.
3. Start with deterministic fake backend faults before adding random faults.
4. Run the closeout property with `PROPTEST_CASES=2048`; keep the default test
   budget lower if needed for normal `cargo test` runtime.

Exit gate:

1. The property catches stale-inventory, unsafe-gate, and delete-before-copy
   mutations.

### 10. Fuzz Tests

Required targets:

1. `format_quarantine`: arbitrary bytes into inventory decode.
2. `service_quarantine`: arbitrary operation streams over an in-memory fake
   backend with bounded object and payload counts.

Required fuzz invariants:

1. No panic.
2. No unbounded allocation.
3. Invalid bytes return typed format errors.
4. Service operation streams preserve the delete-after-durable-quarantine
   invariant.
5. Reconcile never mutates backend state.
6. All errors are typed service errors.

Minimum local loop before closeout:

1. `cargo +nightly fuzz run format_quarantine -- -runs=4096`
2. `cargo +nightly fuzz run service_quarantine -- -runs=2048`

Exit gate:

1. Fuzz targets exist, are registered in `crates/storage-next/fuzz/Cargo.toml`,
   and run locally with the bounded loops above.

Intent:

1. These fuzz targets are required for M3TC4. M3TC3 established the
   service-fuzz scaffold, and quarantine has enough stateful crash-window
   behavior to justify pulling service fuzzing into the test slice instead of
   deferring it.

## Adversarial Implementation Protocol

Each slice closeout must record the following fields in
`docs/architecture/v1-progress-tracker.md`:

1. Suite cases covered.
2. Narrow command.
3. Sensitivity probe.
4. Failure observed under the probe.
5. Revert proof.
6. Broad command.

Acceptable sensitivity probes:

1. Treat corrupt inventory as empty.
2. Allow unsafe gate to proceed.
3. Delete source before quarantine object durable publish.
4. Ignore an unlisted quarantine object during reconcile.
5. Drop failed-delete entries from rewritten inventory.
6. Recast visibility uncertainty as success.
7. Skip source-object family validation.
8. Let cache mode publish durable quarantine objects.

## Suggested Slice Order

### M3TC4A: Inventory Codec Coverage

1. Implement format golden, malformed, validation, and fuzz tests.
2. Sensitivity probe: bypass duplicate object-id validation.

### M3TC4B: Inventory Service Coverage

1. Implement inventory load, mismatch, publish, and capability tests.
2. Sensitivity probe: treat corrupt current inventory as empty.

### M3TC4C: Quarantine And Purge Coverage

1. Implement gate, quarantine sequence, publish fault, source delete, purge,
   and partial-failure tests.
2. Sensitivity probe: delete source before quarantine object durable publish.

### M3TC4D: Reconciliation And Service Fuzz Coverage

1. Implement reconciliation classification, state-machine property, and
   service fuzz tests.
2. Sensitivity probe: ignore unlisted quarantine objects during reconcile.

## Exit Gate

M3TC4 is complete when:

1. All required test families pass.
2. Inventory format fuzz and service operation fuzz run locally.
3. Every quarantine publish/delete failure has an assertion for durable state.
4. Every recovery classification has a concrete object-state fixture.
5. Cache mode durable quarantine absence is tested or explicitly covered by
   M3TD1 with a cross-reference.
6. Sensitivity probes are recorded for every slice.
