# L8Q Implementation Plan: Durable Table Manifest Format

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-implementation-plan.md`

## Objective

Define a storage-owned durable table-manifest byte format.

L8Q is intentionally a format slice. It gives later lifecycle slices stable
bytes for table reachability, but it does not publish, recover, retain, prune,
or delete table objects. L8R will publish and recover table manifests. L8S will
consume table manifests for reachability and retention. L8T will use durable
table-manifest coverage for flush watermarks.

The format must encode the semantic table graph that checkpoints currently
hide inside row-native snapshots:

1. which branch owns each immutable table object;
2. which level and ordering position each branch-owned table has;
3. which table identities map to which local durable table-object names;
4. which table bounds, commit ranges, timestamp ranges, and object facts were
   known when the manifest was written;
5. which inherited layers keep source table objects reachable;
6. which materialization or rewrite provenance still matters for recovery,
   diagnostics, and safe reclaim;
7. which manifest version and checksum guard the bytes.

The format must remain primitive-neutral. It records storage rows, table facts,
branch ids, object names, and provenance. It must not record graph/vector/JSON,
query, StrataHub, product branch workflow, or engine primitive semantics.

## Inputs

1. `docs/architecture/storage/l2-object-layout.md`
2. `docs/architecture/storage/l3-durable-format-codec.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
5. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
8. `crates/storage-next/src/format/manifest.rs`
9. `crates/storage-next/src/format/table.rs`
10. `crates/storage-next/src/service/manifest.rs`
11. `crates/storage-next/src/service/table.rs`
12. `crates/storage-next/src/layout.rs`
13. `crates/storage-next/src/object.rs`
14. `crates/storage-next/src/table/mod.rs`
15. `crates/storage-next/src/branch/read.rs`
16. `crates/storage-next/src/branch/state.rs`
17. `crates/storage/src/manifest.rs`
18. `crates/storage/src/segmented/recovery.rs`
19. `crates/storage/src/segmented/quarantine_protocol.rs`
20. `crates/storage/src/segmented/ref_registry.rs`

## Existing-Code Source Map

| Current file | Evidence | L8Q action |
|---|---|---|
| `format/manifest.rs` | Database manifest already has explicit magic, version, bounded strings, optional fact sentinels, validation, and CRC. | Reuse the format discipline, but create a separate table-manifest format. Do not overload database MANIFEST bytes. |
| `service/manifest.rs` | `ManifestRole::Table` already exists, but the service currently encodes/decodes only database manifests. | L8Q should add format bytes. L8R should wire service load/publish behavior. |
| `service/table.rs` | `TableObjectFacts` names object, byte count, row count, data block count, and commit range. | Use these facts as the minimum durable object facts embedded in table-manifest entries. Add bounds/provenance fields supplied by table/branch runtime facts. |
| `layout.rs` and `l2-object-layout.md` | `tables/<branch-id>/manifest` is reserved for branch/table reachability metadata. | Keep the first implementation branch-scoped. Do not introduce a global table-manifest object in L8Q. |
| `table/mod.rs` and `format/table.rs` | L5 owns immutable table bytes and validation. | Table manifest records facts about table objects; it does not duplicate table row bytes or table block contents. |
| `branch/read.rs` | Branch table references carry owned/replacement/inherited source information and ordering semantics. | Encode enough branch table-reference facts to reconstruct the reachable table graph later. |
| `branch/state.rs` | Branch state owns level ordering, inherited layers, materialization handles, and replacement provenance. | The manifest format must represent level order, inherited-layer order, and materialization status without storing live runtime handles. |
| `lifecycle/retention.rs` | Table-object retention is currently proof-limited. | L8Q supplies the durable evidence that L8S will need. It does not change retention behavior itself. |

## Old Codebase Porting Map

The old manifest format is reference material for durable reachability, not an
API to copy.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/manifest.rs` | `segments.manifest` records branch-owned segment files, levels, inherited layers, fork version, status, and CRC. | Port the semantic reachability set. Replace filenames with validated `ObjectName`, table identity, table facts, bounds, and provenance. | Owned table and inherited-layer round trips preserve ordering and facts. |
| `crates/storage/src/segmented/recovery.rs` | Recovery trusts a valid manifest, rejects a corrupt manifest, and treats missing manifest as degraded fallback. | L8Q only defines corrupt/missing/future-version decode behavior. L8R will decide recovery policy. | Corrupt/future/truncated bytes reject with typed format errors. |
| `crates/storage/src/segmented/tests/leveled.rs` | Manifest recovery restores levels and rejects corrupt manifests; missing manifest fallback is policy downgrade. | Preserve level ordering and corrupt rejection. Missing-manifest policy remains L8R. | Canonical level ordering and corrupt rejection tests. |
| `crates/storage/src/segmented/tests/concurrency.rs` | A corrupt manifest must not cause recovery to load orphaned SST files. | The format must fail closed on checksum, count, bounds, and object-name corruption. | Decoder rejects before yielding partial table refs. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Manifest reachability is durable reclaim proof; runtime registries are accelerators. | Make table manifest the durable reachability input for later retention. Do not delete or quarantine anything in this slice. | Manifest facts can be enumerated deterministically for future proof construction. |
| `crates/storage/src/segmented/ref_registry.rs` | Runtime ref registry is rebuilt from manifests and must not replace durable proof. | L8Q records stable durable facts. Runtime caches remain rebuildable. | Tests assert no runtime-only fields are required to decode. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | Manifest publish failure windows are typed and recoverable. | Publication windows are L8R scope. L8Q only makes manifest bytes deterministic and self-validating. | No tests should require publish/recovery side effects in L8Q. |

Do not port:

1. raw filesystem path handling;
2. direct `std::fs` manifest writes;
3. old segment filename-only entries;
4. missing-manifest recovery fallback;
5. old global pause hooks;
6. direct reclaim, purge, or quarantine mutation;
7. row pruning policy;
8. product branch names, tags, notes, merge, revert, or cherry-pick vocabulary.

## Scope

L8Q implements:

1. a new table-manifest format module;
2. crate-private in-memory types for branch-scoped table manifests;
3. owned-table entries with table identity, object name, level, order, bounds,
   object facts, and provenance;
4. inherited-layer entries with source branch, fork version, status, order, and
   table references;
5. materialization/rewrite provenance facts that are durable diagnostics, not
   runtime handles;
6. explicit manifest versioning and checksum validation;
7. canonical encode ordering and strict decode validation;
8. optional storage-owned extension sections with required/optional flags;
9. golden vectors and format tests;
10. fuzz/testkit decode contract hooks;
11. source guards preventing product or raw-IO imports in the format code;
12. a porting-log entry after implementation.

L8Q does not implement:

1. table-manifest publication;
2. table-manifest recovery into L6;
3. database-manifest pointers to table manifests;
4. table-object reachability proof;
5. retention, quarantine, purge, or deletion;
6. flush watermark advancement from table manifests;
7. compaction/materialization durable publication;
8. lazy table reads;
9. memory-budget admission;
10. L9 public API exposure.

## Manifest Granularity

The first table manifest is branch-scoped and corresponds to the reserved
object namespace `tables/<branch-id>/manifest`.

Reasons:

1. old storage persisted `segments.manifest` per branch;
2. L6 branch state is the authority for per-branch table ordering;
3. branch-scoped publication minimizes unrelated rewrite conflicts;
4. a later aggregate index can be derived from branch manifests if needed.

The manifest bytes must include the branch id even though the object name also
contains the branch id. L8R must reject a branch/object mismatch before trusting
the payload.

## Type Surface

Names may change during implementation, but responsibilities should remain
stable.

```rust
pub(crate) struct TableManifest {
    branch_id: BranchId,
    branch_generation: Option<BranchGeneration>,
    manifest_sequence: u64,
    levels: Vec<TableManifestLevel>,
    inherited_layers: Vec<TableManifestInheritedLayer>,
    extension_sections: Vec<TableManifestExtensionSection>,
}

pub(crate) struct TableManifestLevel {
    level: BranchLevel,
    tables: Vec<TableManifestTableRef>,
}

pub(crate) struct TableManifestTableRef {
    table_identity: TableIdentity,
    object: ObjectName,
    order: u32,
    facts: TableManifestTableFacts,
    bounds: TableManifestTableBounds,
    provenance: TableManifestTableProvenance,
}

pub(crate) struct TableManifestInheritedLayer {
    order: u32,
    source_branch_id: BranchId,
    source_branch_generation: Option<BranchGeneration>,
    fork_version: CommitVersion,
    status: TableManifestInheritedLayerStatus,
    tables: Vec<TableManifestTableRef>,
}

pub(crate) enum TableManifestTableProvenance {
    Flush,
    SnapshotInstall,
    Compaction,
    MaterializationReplacement {
        source_branch_id: BranchId,
        fork_version: CommitVersion,
    },
    Recovered,
}
```

Generation fields are optional until L8Y completes branch generation behavior.
The format reserves them now so durable bytes do not need a breaking redesign
when generation guards become mandatory.

## Format Rules

Encoding must be deterministic.

Rules:

1. Header includes magic, format version, branch id, manifest sequence, section
   count, and checksum.
2. Format version starts at `1`; future versions fail closed.
3. Checksums cover all bytes before the checksum field.
4. Object names are encoded as validated `ObjectName` strings, not raw paths.
5. Table identities are encoded through their storage-owned stable bytes/string.
6. Numeric counts are bounded before allocation.
7. String lengths are bounded before UTF-8 conversion.
8. Empty table manifests are valid only for an existing branch with no durable
   immutable tables and no inherited layers.
9. Empty table entries, zero row counts, and invalid level values are rejected.
10. Duplicate table identities inside a manifest are rejected unless a later
    slice explicitly introduces shared identity aliases with a separate proof.
11. Duplicate object names inside a manifest are rejected.
12. L0 ordering is explicit: `order = 0` means highest-precedence/newest.
13. L1+ tables must be sorted by physical key range and non-overlapping within
    a level.
14. Inherited layers are ordered nearest ancestor first by explicit `order`.
15. Inherited-layer `(source_branch_id, fork_version)` pairs are unique.
16. Materializing status is durable state; runtime handles are not encoded.
17. Unknown required extension sections are rejected.
18. Unknown optional extension sections are preserved by decode/encode only if
    the in-memory type explicitly carries raw section bytes. Otherwise they are
    ignored with a typed "not preserved" fact and must not be rewritten.

## Bounds And Facts

Each table reference should carry:

1. byte count;
2. row count;
3. data block count;
4. commit min and max;
5. optional timestamp min and max;
6. physical key first and last;
7. internal key first and last;
8. table identity;
9. durable object name;
10. provenance.

The table object remains authoritative for row bytes. L8Q only records facts
that let L8R reject manifest/object mismatches before installing recovered
branch state.

## Extension Sections

Extension sections are storage-owned. They are not engine primitive snapshot
sections.

Rules:

1. Section kind is a bounded ASCII identifier.
2. Section flags include `required` and `preserve_on_rewrite`.
3. Required unknown sections fail decode.
4. Optional unknown sections may be skipped by read-only decoders.
5. No section may contain product primitive names or external system names.
6. Extension sections must not affect canonical ordering of core table refs.

The initial implementation may support the section framing with no known
extension sections.

## Source Boundaries

L8Q format code may import:

1. core storage atom types such as branch id, commit version, and timestamp;
2. `crate::object::ObjectName`;
3. `crate::table` identity/fact/bounds types through stable public crate-private
   accessors;
4. local byte-reader/format helpers.

It must not import:

1. `std::fs`, `std::path::Path`, `std::env`, or raw IO;
2. backend publish services;
3. lifecycle maintenance code;
4. engine/product crates;
5. StrataHub code;
6. primitive DTOs;
7. query/index/autosearch modules.

## Implementation Steps

1. Add `crates/storage-next/src/format/table_manifest.rs`.
2. Re-export crate-private `encode_table_manifest`, `decode_table_manifest`,
   and manifest data types from `format/mod.rs`.
3. Add validation constructors for manifest, level, table ref, inherited layer,
   facts, bounds, provenance, and extension sections.
4. Implement deterministic encoding.
5. Implement fail-closed decoding with bounded allocations.
6. Add golden bytes under `crates/storage-next/src/testdata/goldens/` or the
   existing storage-format golden directory.
7. Add a `format::fuzzing::decode_table_manifest` contract hook.
8. Add source guards for the format file.
9. Add a porting-log entry recording old manifest behavior preserved, changed,
   retired, and deferred.

## Deferred Behavior

Deferred to L8R:

1. `TableManifestService` load/publish methods;
2. database manifest references to current table-manifest generations;
3. recovery from branch table manifests;
4. manifest/object mismatch health classification.

Deferred to L8S:

1. live table-object reachability graph;
2. orphan table-object classification;
3. table-object quarantine candidates;
4. deletion proof.

Deferred to L8T:

1. flush watermark proof from table-manifest coverage;
2. WAL truncation after table-manifest-backed flush.

Deferred to L8U:

1. durable publication of compaction and materialization outputs;
2. manifest updates after rewrite outputs.

Deferred to L8V:

1. row-version pruning;
2. tombstone pruning;
3. TTL pruning.

## Exit Gate

L8Q is complete when:

1. table-manifest bytes have a dedicated format module;
2. branch-owned and inherited-layer table graphs round trip deterministically;
3. golden vectors pin canonical bytes;
4. corrupt, truncated, future-version, duplicate, and invalid-order bytes reject;
5. unknown required extension sections reject;
6. optional extension behavior is explicit and tested;
7. source guards prevent product/raw-IO imports;
8. fuzz/decode contracts reject arbitrary corrupt bytes without panic;
9. the parent plan links to this plan and test plan;
10. L8R can build on the format without changing its core semantics.
