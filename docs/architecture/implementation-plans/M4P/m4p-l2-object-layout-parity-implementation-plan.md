# M4P-L2 Implementation Plan: Object Layout Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l2-object-layout-parity-test-plan.md`

## Objective

Close the L2 object-layout audit gaps without moving backend IO, durable bytes,
service policy, recovery policy, or lifecycle cleanup behavior into L2.

M4P-L2 restores the missing old-storage namespace discipline that belongs at
the object-layout boundary:

1. make `ObjectLayout` the single source of truth for object-family names,
   object roles, prefixes, and parse/classification helpers;
2. remove production lifecycle/service code that re-parses canonical object
   paths with raw string checks;
3. sync the L2 architecture document with the manifest objects already present
   in storage-next;
4. document the V1 `tmp/` namespace decision;
5. add a source guard that prevents future durable services from constructing
   canonical storage object names outside L2.

The first executable slice is `M4P-L2A`: object/table classifier helpers. This
document covers the full L2 package so later L4/L8 cleanup and recovery slices
can consume L2 role facts without reopening object namespace decisions.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L2. Object Layout`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings closed by this plan:

1. L2 documentation has drifted behind implemented manifest objects:
   `manifest/branch-catalog` and `manifest/pending-releases`;
2. table object shape parsing leaks above L2 in
   `crates/storage-next/src/lifecycle/table_reachability.rs`;
3. CI does not yet enforce the L2 naming boundary;
4. `tmp/` namespace semantics need an explicit V1 decision.

Supporting architecture:

1. `docs/architecture/storage/l2-object-layout.md`
2. `docs/architecture/storage/implementation-patterns.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

The serving-path proof plan is not a direct performance input for L2. L2 is not
the current point-read, load, or scan bottleneck. Its role in M4P is boundary
hardening: later L4/L8 slices should be able to reason about table objects,
manifests, WAL segments, snapshots, quarantine inventory, and temporary objects
without re-encoding L2 naming rules.

## Predecessors

Required before implementation:

1. parent M4P program plan;
2. M4P test methodology;
3. L2 audit findings listed above;
4. M4P-L1 delete/backend IO contract complete, so L2 does not need to model
   delete durability.

No lower-layer implementation predecessor exists beyond L1 consuming
already-validated `ObjectName` values. L2 must remain independent of backend
path mapping and filesystem behavior.

## Layer Ownership Check

M4P-L2 owns canonical names, prefixes, object families, object roles, and object
name classification. It must not grow responsibilities from adjacent layers:

1. L1 owns backend mapping from validated object names to backend keys or local
   paths, including parent-directory sync and writer-lock behavior.
2. L3 owns durable byte formats. L2 may validate that an object name has the
   canonical shape for a WAL segment or snapshot object, but it must not parse
   WAL, manifest, snapshot, table, sidecar, or quarantine bytes.
3. L4 owns service semantics and service error mapping. L2 may return a
   malformed object-role fact; L4 decides whether a weak-prefix listed object is
   ignored, treated as corrupt recovery state, or reported as a service error.
4. L5 owns table bytes, readers, writers, and table compaction output. L2 only
   names immutable table objects and branch table manifests.
5. L6 owns branch LSM mechanics and branch visibility. L2 may carry a branch-id
   component as a validated string; it must not decide branch existence,
   inheritance, materialization, or source layout.
6. L8 owns retention, quarantine, cleanup, and recovery health. L2 must not
   decide whether a table object is live, orphaned, safe to delete, or health
   debt.
7. L9 owns public storage APIs. L2 helpers remain crate-private unless a later
   L9 diagnostics slice explicitly exposes lower-layer facts through an L9 DTO.

## Existing-Code Source Map

| Current file | Evidence | L2 action |
| --- | --- | --- |
| `crates/storage-next/src/object/mod.rs` | Validated `ObjectName` and `ObjectPrefix` primitives. | Keep low-level validation here. Do not add database-family policy to `object`. |
| `crates/storage-next/src/layout/mod.rs` | Defines `ObjectFamily` and constructors for manifest, WAL, WAL metadata, tables, snapshots, temporary objects, quarantine, locks, and database metadata. | Add role/classification helpers here or in a sibling `layout` module if file size requires a split. |
| `crates/storage-next/src/layout/tests.rs` | Covers many constructor, prefix, ordering, invalid-component, reserved-family, and old-name absence cases. | Add explicit tests for the newer manifest objects and for every new role/classification helper. |
| `crates/storage-next/tests/object_layout_properties.rs` | Existing source guard for reserved layout names. | Harden the guard so production services/lifecycle code cannot construct or parse canonical layout names with raw strings outside L2. |
| `crates/storage-next/src/lifecycle/table_reachability.rs` | Uses `starts_with("tables/")`, `ends_with("/manifest")`, and slash-count checks. | Replace with L2-owned table object classification helpers. |
| `crates/storage-next/src/service/manifest.rs` | Parses branch table manifest objects by splitting canonical object names. | Replace service-side shape parsing with L2 role helpers when the helper exists. Keep service-specific error mapping in L4. |
| `crates/storage-next/src/service/wal.rs` | Parses listed WAL segment object names and fixed-width hex IDs. | Move canonical WAL object-name shape parsing into L2 helpers. Keep WAL service policy and errors in L4. |
| `crates/storage-next/src/service/snapshot/listing.rs` | Parses listed snapshot object names and fixed-width hex IDs. | Move canonical snapshot object-name shape parsing into L2 helpers. Keep snapshot service policy and errors in L4. |
| `crates/storage-next/src/service/quarantine.rs` and `crates/storage-next/src/service/quarantine/reconcile.rs` | Derive the reserved quarantine manifest object id by splitting an object name and parse quarantine object shape locally. | Use L2's reserved quarantine component helper and role helpers instead of local string splitting. |
| `crates/storage-next/src/format/table_manifest.rs` | Decodes persisted table object names and currently parses `tables/<branch>/<level>/<table>` components to validate table-manifest entries. | Call L2 table-object classification for canonical object shape. Keep table-manifest branch/provenance consistency checks in L3. |
| `crates/storage-next/src/format/quarantine.rs` | Decodes persisted object-name strings and validates quarantine object ids/source object names. | Leave raw byte-to-`ObjectName` decoding validation in L3, but use L2 helpers for any canonical role decision that is not format-local. |

## Current Raw Layout Parsing Inventory

This inventory was produced from the current tree before implementation so the
slice does not need to rediscover the call sites. Line numbers are approximate
and should be rechecked when editing.

Search shape used:

```text
ObjectName::new(...)
ObjectPrefix::new(...)
format!(...) with reserved families
starts_with("<family>/")
ends_with("/manifest")
split('/')
rsplit('/')
```

### Must Convert To L2 Helpers

These are production service/lifecycle call sites that currently encode
canonical object-layout grammar outside L2.

| File | Current pattern | Required action |
| --- | --- | --- |
| `crates/storage-next/src/lifecycle/table_reachability.rs:702-712` | `starts_with("tables/")`, `ends_with("/manifest")`, and slash-count checks classify table objects. | Replace with L2 table object classification. This is the audit's required first conversion. |
| `crates/storage-next/src/service/manifest.rs:738-748` | Splits branch table manifest object names to verify shape and extract branch component. | Replace shape parsing with L2 table-manifest classification; keep service-specific branch-id parse/error mapping in L4. |
| `crates/storage-next/src/service/wal.rs:1217-1268` | Splits `wal/<segment-id>` and validates fixed-width hex locally. | Replace canonical shape parsing with L2 WAL segment classification; keep WAL service zero-id policy and error mapping in L4 unless L2 explicitly owns zero rejection. |
| `crates/storage-next/src/service/snapshot/listing.rs:153-188` | Splits `snapshots/<snapshot-id>` and validates fixed-width hex locally. | Replace canonical shape parsing with L2 snapshot classification; keep weak-prefix ignore behavior and snapshot service errors in L4. |
| `crates/storage-next/src/service/quarantine/reconcile.rs:740-813` | Splits `quarantine/<branch>/<object-id>` and reconstructs expected quarantine object names locally. | Replace with L2 quarantine classification; keep reconciliation policy in L4. |
| `crates/storage-next/src/service/quarantine/reconcile.rs:819-822` | Derives the reserved quarantine inventory object id using `rsplit('/')`. | Replace with `ObjectLayout::quarantine_inventory_object_id()` or equivalent L2 helper. |
| `crates/storage-next/src/service/quarantine.rs:583-590` | Derives the reserved quarantine inventory object id using `rsplit('/')`. | Replace with `ObjectLayout::quarantine_inventory_object_id()` or equivalent L2 helper. |

### Explicit Decision Point

This call site crosses L2/L3 ownership. It must not remain an unnoticed second
layout parser.

| File | Current pattern | Required decision |
| --- | --- | --- |
| `crates/storage-next/src/format/table_manifest.rs:1228-1241` | Splits `tables/<branch>/<level>/<table>` to validate persisted table object names in a durable table-manifest payload. | Call L2 table-object classification for canonical shape, then keep branch/provenance checks in L3. Do not keep a parallel table-layout parser in L3. |

### Already Acceptable Or Test-Only

These hits are not M4P-L2 conversion targets unless implementation changes make
them fail the source guard.

| File or area | Reason |
| --- | --- |
| `crates/storage-next/src/object/mod.rs` | Owns primitive `ObjectName`/`ObjectPrefix` validation, including low-level slash/component parsing. |
| `crates/storage-next/src/layout/mod.rs` | Owns canonical constructors, family strings, prefixes, and the new classification helpers. Raw layout strings are allowed here. |
| `crates/storage-next/src/backend/local_fs.rs` | Owns L1 mapping from validated object names to private filesystem paths and list-prefix filtering. This is backend path translation, not object-role classification. |
| `crates/storage-next/src/backend/memory.rs` | Owns backend key-prefix filtering for `list_prefix`. This is backend implementation, not object-role classification. |
| `crates/storage-next/src/format/quarantine.rs` | Decodes persisted object-name strings and object ids from durable bytes. Keep byte-to-`ObjectName` validation in L3; route canonical role decisions through L2 if any are added. |
| `crates/storage-next/src/lifecycle/flush.rs:624` | Validates that a caller-supplied artifact component is a single valid object-name component. This is not reserved-family layout construction. |
| `crates/storage-next/src/test_support/`, `crates/storage-next/src/testkit/`, inline `#[cfg(test)]` modules, and `src/*/tests/` | Test fixtures may use raw valid and malformed object names intentionally. Source guards must skip test-only code or provide narrow fixture allowances. |

## Downstream Consumer Map

These files are important consumers of the L2 contract, but they do not become
L2-owned behavior.

| Downstream file | Evidence | Owning follow-up |
| --- | --- | --- |
| `crates/storage-next/src/lifecycle/table_reachability.rs` | Determines whether table inventory entries are table data, table manifests, malformed table objects, or non-table objects. | L8 keeps retention decisions; L2 only supplies classification. |
| `crates/storage-next/src/lifecycle/table_object_retention.rs` and related tests | Consume table reachability proof outcomes. | Existing retention semantics should remain unchanged after replacing raw parsing with L2 helpers. |
| `crates/storage-next/src/service/manifest.rs` | Lists table manifest objects and maps object names to branch ids. | L4 keeps manifest service errors and recovery facts; L2 supplies canonical role parsing. |
| `crates/storage-next/src/service/wal.rs` | Lists WAL segment objects and orders parsed segment ids. | L4 keeps zero-segment rejection, recovery ordering, and WAL service errors. L2 supplies shape and parsed id facts. |
| `crates/storage-next/src/service/snapshot/listing.rs` | Lists snapshot objects and orders parsed snapshot ids. | L4 keeps weak-prefix handling and snapshot service errors. L2 supplies shape and parsed id facts. |
| `crates/storage-next/src/service/quarantine.rs` and `crates/storage-next/src/service/quarantine/reconcile.rs` | Validate inventory object ids, source families, and listed quarantine objects. | L4 keeps quarantine reconciliation policy; L2 supplies quarantine role parsing and reserved component facts. |
| `crates/storage-next/src/lifecycle/recovery.rs`, `crates/storage-next/src/lifecycle/durable.rs`, and future L8 cleanup slices | Need object-family/role facts for cleanup and recovery diagnostics. | L8 consumes L2 facts but keeps cleanup, health debt, and recovery policy. |

## Old-Code Porting Map

The old architecture is behavioral evidence, not an API template. Storage-next
keeps the cleaner object namespace instead of restoring old filesystem paths.

| Old source | Behavior to preserve | Storage-next decision | Test focus |
| --- | --- | --- | --- |
| `crates/storage/src/durability/layout.rs` | One layout source defined WAL, segment, snapshot, manifest, and follower files. | Preserve the single-source-of-truth property through `ObjectLayout`, not old path names. | Source guard rejects ad-hoc canonical layout strings outside L2. |
| `crates/storage/src/durability/wal/mod.rs` and `crates/storage/src/durability/format/wal_record.rs` | WAL segment names were recognized consistently by WAL code. | L2 owns WAL segment object-name parsing; L4 owns WAL service behavior. | Listed WAL objects classify as WAL segments or malformed WAL objects without service-local path grammar. |
| `crates/storage/src/durability/format/snapshot.rs` | Snapshot/checkpoint names were recognized consistently by checkpoint code. | L2 owns snapshot object-name parsing; L4 owns snapshot service behavior. | Listed snapshot objects classify by canonical fixed-width ID. |
| `crates/storage/src/manifest.rs` | Segment/table manifest file names were stable and centrally known. | Storage-next keeps `manifest/current`, `manifest/branch-catalog`, `manifest/pending-releases`, and `tables/<branch>/manifest` as L2 names. | Explicit constructor/classifier tests pin all manifest-family objects. |
| `crates/storage/src/quarantine.rs` and `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine inventory and quarantined object locations were stable and centrally known. | Storage-next keeps global `quarantine/<branch>/...` names and exposes the reserved inventory component from L2. | Quarantine parser tests distinguish inventory object, quarantined object, malformed quarantine object, and non-quarantine object. |

Do not port:

1. old `DatabaseLayout` path APIs into storage-next;
2. old filename literals such as `MANIFEST`, `wal-NNNNNN.seg`,
   `snap-NNNNNN.chk`, `segments.manifest`, `quarantine.manifest`, or
   `__quarantine__/` as target storage-next object names;
3. follower-state or follower-audit object names;
4. filesystem directory creation, rename, or sync semantics;
5. old segment directory traversal into L2;
6. retention, quarantine, or recovery policy into L2;
7. product open policy or engine error mapping.

## Scope

M4P-L2 implements:

1. documentation updates for implemented manifest object names and the V1 `tmp/`
   namespace decision;
2. L2-owned role/classification helpers for the durable object families already
   defined by `ObjectLayout`;
3. at minimum, a table object classifier that distinguishes non-table objects,
   branch table manifests, table data objects, and malformed table-family
   objects;
4. manifest, WAL, snapshot, and quarantine role helpers where service code
   currently duplicates canonical layout parsing;
5. replacement of production raw object-name parsing in lifecycle/service code
   with L2 helpers;
6. a hardened source guard for raw canonical object construction outside L2;
7. tests that prove constructor, classifier, source-guard, and consumer
   behavior.

M4P-L2 does not implement:

1. backend IO, path conversion, parent sync, delete durability, or writer-lock
   behavior;
2. durable format byte changes;
3. manifest, WAL, snapshot, sidecar, table, quarantine, or checkpoint service
   policy changes;
4. table-object retention, cleanup, quarantine, recovery, or health-debt policy;
5. table reader, writer, compaction, or LSM source-layout changes;
6. object-store hot-prefix tuning;
7. manifest history;
8. public L9 API changes;
9. benchmark fast paths.

## V1 `tmp/` Decision

M4P-L2 should record this decision in
`docs/architecture/storage/l2-object-layout.md`:

1. `tmp/` remains a reserved object-visible family for future durable service
   operations that need named temporary objects.
2. Current local filesystem publish temporary files remain backend-private L1
   paths and are not L2 `tmp/` objects.
3. Durable services must not begin using `tmp/` for visible objects without
   consuming `ObjectLayout::temporary_*` constructors and adding L4/L8 publish
   and cleanup tests.
4. L2 tests should continue to prove `tmp/` is reserved even if no V1 service
   publishes object-visible temporary objects.

## Target Helper Shape

Exact Rust names can change during implementation, but the helper shape should
stay within these constraints.

L2 should expose crate-private role facts such as:

```text
ObjectLayout::classify_object(&ObjectName) -> LayoutClassification
ObjectLayout::classify_table_object(&ObjectName) -> TableObjectClassification
ObjectLayout::classify_manifest_object(&ObjectName) -> ManifestObjectClassification
ObjectLayout::classify_wal_object(&ObjectName) -> WalObjectClassification
ObjectLayout::classify_snapshot_object(&ObjectName) -> SnapshotObjectClassification
ObjectLayout::classify_quarantine_object(&ObjectName) -> QuarantineObjectClassification
```

The helpers should support these facts:

1. family not recognized;
2. recognized family with malformed canonical shape;
3. database manifest;
4. branch catalog manifest;
5. pending releases manifest;
6. WAL segment with parsed segment id;
7. WAL segment metadata sidecar with parsed segment id;
8. branch table manifest with branch component;
9. table data object with branch component, level, and table id;
10. snapshot with parsed snapshot id;
11. temporary object with operation id and object id;
12. quarantine manifest with branch component;
13. quarantine object with branch component and object id;
14. writer lock;
15. database metadata.

Parsing rules:

1. role helpers validate by canonical shape and, where practical, reconstruct
   the expected `ObjectLayout` name and compare it to the input object;
2. fixed-width hex ID parsing belongs to L2 because it is part of object-name
   shape;
3. table-level parsing belongs to L2 because `l0000` level encoding is part of
   object-name shape;
4. branch-id string validation in object names belongs to L2, but branch-id
   semantic parsing and branch existence remain L4/L6/L8 depending on caller;
5. L2 malformed-role errors use `LayoutError` or a layout-owned role error, not
   service-specific errors;
6. L4/L8 callers map L2 classification failures into their own typed service or
   lifecycle errors.

If implementation shows that a general `classify_object` helper pulls service
policy into L2, stop and narrow the slice to focused family helpers. The table
object classifier is the non-negotiable first helper because it directly closes
the audit leak.

## Implementation Steps

### 1. Sync The L2 Specification

1. Update `docs/architecture/storage/l2-object-layout.md` so the
   implemented canonical layout block includes:
   - `manifest/current`;
   - `manifest/branch-catalog`;
   - `manifest/pending-releases`;
   - `meta/wal/<segment-id>`;
   - existing table, snapshot, temporary, quarantine, lock, and metadata names.
2. Record the V1 `tmp/` decision from this plan.
3. Check L3/L4 documentation for manifest-family object enumerations and update
   only the object-name portions if they drifted.
4. Do not change durable bytes or service policy while syncing docs.

Acceptance:

1. docs no longer imply `manifest/current` is the only manifest-family object;
2. `tmp/` is explicitly reserved but unused by current backend-private publish
   temps;
3. old-storage filenames remain listed as evidence or retired names, not target
   storage-next names.

### 2. Add L2 Role/Classifiers

1. Add layout-owned role vocabulary in `crates/storage-next/src/layout/mod.rs`
   or a small sibling module under `crates/storage-next/src/layout/`.
2. Keep helpers crate-private.
3. Add table object classification first:
   - non-table object;
   - branch table manifest;
   - table data object;
   - malformed table-family object.
4. Add manifest classification for current database manifest, branch catalog,
   pending releases, and malformed/unknown manifest-family object.
5. Add WAL and snapshot classification for fixed-width lowercase hex object ids.
6. Add quarantine classification for branch inventory object, quarantined
   object, malformed quarantine object, and non-quarantine object.
7. Use existing constructors and component validators rather than duplicating
   raw string grammar in each parser.
8. Add small helper functions for reusable canonical components, including the
   quarantine inventory object id.

Acceptance:

1. every classifier can classify objects produced by its matching
   `ObjectLayout` constructor;
2. malformed objects under a reserved family fail closed with a layout-owned
   error/fact;
3. nonmatching families can be ignored by family-specific callers without
   pretending the object is valid in that family;
4. classifiers do not import backend, format, service, lifecycle, branch, commit,
   API, or product modules.

### 3. Replace Raw Parsing In Consumers

1. Replace `is_non_table_object`, `is_table_manifest_object`, and
   `is_table_data_object` in
   `crates/storage-next/src/lifecycle/table_reachability.rs` with L2 table
   classification.
2. Replace table-manifest object shape parsing in
   `crates/storage-next/src/service/manifest.rs` with L2 table/manifest
   classification.
3. Replace WAL segment object parsing in `crates/storage-next/src/service/wal.rs`
   with L2 WAL classification while preserving service-specific errors and
   zero-segment policy.
4. Replace snapshot object parsing in
   `crates/storage-next/src/service/snapshot/listing.rs` with L2 snapshot
   classification while preserving weak-prefix ignore behavior and service
   errors.
5. Replace quarantine reserved-component and quarantine-object parsing in
   `crates/storage-next/src/service/quarantine.rs` and
   `crates/storage-next/src/service/quarantine/reconcile.rs` with L2 quarantine
   helpers while preserving reconciliation policy.
6. Update `format/table_manifest.rs` to use L2 table-object classification for
   canonical decoded table-object shape. Keep format-specific branch/provenance
   consistency checks in L3.
7. Leave pure L3 persisted-object decoding in `format/quarantine.rs` as
   `ObjectName::new(decoded_string)` validation, while routing any canonical
   role decisions through L2 helpers.

Acceptance:

1. production L4/L8 code no longer performs canonical object role decisions
   using raw `starts_with("tables/")`, slash counts, or equivalent string
   grammar;
2. L3 table-manifest validation uses L2 table-object classification for
   canonical shape and keeps only branch/provenance checks in L3;
3. service and lifecycle tests preserve behavior before and after helper
   adoption;
4. L4/L8 still own their error messages, health outcomes, retention decisions,
   weak-prefix policy, and recovery policy.

### 4. Harden The Source Guard

1. Update `crates/storage-next/tests/object_layout_properties.rs` or add an
   equivalent integration test for the L2 naming boundary.
2. Reject production raw literals and format strings that assemble canonical
   reserved-family object names outside L2.
3. Reject production object role parsing outside L2 when it uses reserved-family
   raw string grammar such as:
   - `starts_with("tables/")`;
   - `ends_with("/manifest")`;
   - `split('/')` followed by reserved-family matching;
   - `ObjectName::new("wal/...")`;
   - `format!("tables/{...}")`.
4. Keep explicit allowances for:
   - `src/object/` low-level validation;
   - `src/layout/` constructors, classifiers, and tests;
   - `src/backend/local_fs.rs` backend-private path mapping;
   - L3 format decoders validating persisted strings through `ObjectName::new`
     without constructing target canonical names;
   - `#[cfg(test)]`, `src/*/tests/`, and `src/testkit/` fixtures.
5. Add self-tests with seeded fixture strings proving the guard catches each
   forbidden pattern and skips test-only code.

Acceptance:

1. the guard fails if a new production service constructs a canonical object
   path outside L2;
2. the guard fails if lifecycle/service code reintroduces table object
   classification with raw string shape checks;
3. the guard does not fail on L3 decoding of persisted object names or on
   test-only fixtures;
4. the guard remains maintainable and does not require every service test
   fixture to use production constructors.

### 5. Close Documentation And Deferrals

1. Record any deliberately retained service-local parsing with owner layer,
   reason, and replacement proof.
2. Update this plan only if implementation proves a narrower or safer helper
   shape.
3. Do not add public APIs or benchmark-only paths.

Acceptance:

1. every L2 audit gap is closed or explicitly deferred with an owner layer;
2. no M4P roadmap labels appear in production Rust identifiers, comments,
   fixture bytes, panic messages, or user-visible text;
3. later L4/L8 implementation plans can cite L2 role helpers as their object
   namespace proof.

## Integration Boundaries

L2 to L1:

1. L2 outputs validated `ObjectName` and `ObjectPrefix` values.
2. L1 treats them as opaque names and maps them to backend keys or paths.
3. L2 does not expose filesystem paths, parent directories, temp-file names, or
   sync operations.

L2 to L3:

1. L3 can decode persisted object-name strings and validate them with
   `ObjectName::new`.
2. L3 must not derive object-family policy from durable bytes without handing
   the object name to L2 role helpers when object role matters.
3. L2 does not know durable format versions or codec IDs.

L2 to L4/L8:

1. L4/L8 consume object roles and map them into service/lifecycle outcomes.
2. L4/L8 keep weak-prefix, corrupt listing, recovery, retention, and cleanup
   decisions.
3. L4/L8 must not construct or parse canonical names with raw strings once the
   L2 helper exists.

## Source Guards

Required guard outcomes:

1. production storage-next code outside L2 cannot construct canonical
   reserved-family object names with raw strings;
2. production service/lifecycle code cannot classify canonical object roles with
   raw `starts_with`, `ends_with`, slash-count, or equivalent path-shape checks;
3. production L3 format code can validate decoded persisted object names but
   cannot become an alternate source of target object layout constructors;
4. tests and testkit can keep explicit raw fixtures.

## Performance Expectations

M4P-L2 is not a performance-restoration slice. Expected counter movement is
none.

Stop condition:

1. If benchmarks improve or regress materially after M4P-L2, treat that as a
   side effect and investigate separately.
2. If the implementation requires service policy in L2 to remove a parser,
   stop and split the parser change into an L4/L8 consumer slice.
3. If the source guard reveals broad production raw naming that cannot be
   cleaned up in this slice without semantic changes, close the table-object
   leak first and record the remaining raw naming with owner-layer deferrals.

## Closeout Requirements

M4P-L2 closes only when:

1. L2 architecture docs match implemented manifest object names;
2. the V1 `tmp/` namespace decision is documented;
3. table object classification is L2-owned and consumed by lifecycle
   reachability code;
4. service-local raw parsing for durable object families is removed or
   explicitly deferred with owner and reason;
5. source guards catch raw canonical object construction outside L2;
6. layout constructor/classifier tests pass;
7. affected service/lifecycle tests pass;
8. `cargo test -p strata-storage-next --locked --test object_layout_properties`
   passes;
9. `cargo test -p strata-storage-next --locked` passes, or any skipped command
   is recorded with reason;
10. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
    passes if code is edited;
11. deferred items are listed with owner layer and reason.
