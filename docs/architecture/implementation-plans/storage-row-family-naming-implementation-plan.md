# Storage Row Family Naming Implementation Plan

## Status

Draft.

## Goal

Clean up the confusing `space` / `space_id` terminology without changing the
durable storage byte format.

The current durable physical key is:

```text
branch_id | space_name\0 | storage_space_id | escaped_user_key
```

This shape is performance-motivated and should stay intact for V1. Product
spaces remain physically grouped so space-wide scans, drops, exports,
diagnostics, and backups can use tight prefix ranges.

The problem is vocabulary. `space_name` is a product namespace. The current
`storage_space_id` byte is not another product space; it is a row family / row
class discriminator.

## Non-Goals

1. Do not remove `space_name` from the physical key.
2. Do not change the V1 durable byte layout.
3. Do not regenerate golden bytes for a semantic layout change.
4. Do not merge product spaces with row families.
5. Do not move product-space bytes out of the physical key in this slice.
6. Do not redesign the public storage API unless explicitly approved.

## Current Model

### Product Space

`space_name` is the product namespace inside a branch:

```text
default
analytics
tenant-a
```

It is not an isolation boundary. Branches isolate data. Spaces partition data
within a branch for product ergonomics and scan locality.

### Row Family

The current `storage_space_id` byte identifies the internal row family:

```text
0x01 commit timeline
0x20 KV
0x22 JSON
0x24 JSON index
0x26 vector collection
0x28 vector entry
0x2a event
0x30 branch control
0x31 space control
0x32 registry
...
```

These IDs separate row families under the same branch and product space. They
also let storage perform bounded prefix scans without interpreting primitive
payloads.

## Current Impact Inventory

Search terms:

```text
StorageSpaceId
storage_space_id
storage_space
StorageSpace
storage space
storage-space
```

Raw impact:

```text
Code files with refs:        141
Docs files with refs:        102
Golden/testdata files:         5

storage-next code files:     126
engine-next code files:       11
executor-next code files:      1
bench files:                  2
```

Exact term counts in code:

```text
StorageSpaceId       390
storage_space_id     139
storage_space        318
storage-space         16
"storage space"       76
```

Important split:

```text
Internal row::StorageSpaceId files:   93
Public api::StorageSpaceId files:     23
Golden/comment files:                  5
```

There are two separate `StorageSpaceId` types today:

1. `crates/storage-next/src/row/mod.rs`
   - Internal durable row type.
   - One byte.
   - Should become row-family vocabulary.

2. `crates/storage-next/src/api/atoms.rs`
   - Public/testkit storage API type.
   - Opaque `Vec<u8>`.
   - Exposed in read and commit API requests.
   - Renaming this is a larger public API vocabulary change.

## Target Vocabulary

Preferred internal names:

```text
StorageSpaceId        -> RowFamilyId
storage_space_id      -> row_family_id
storage_space         -> row_family
storage-space         -> row-family
storage space         -> row family
```

Engine-side symbolic names stay as `RowClass`:

```text
RowClass::storage_space_id() -> RowClass::row_family_id()
```

`RowClass` is the engine symbol. `RowFamilyId` is the storage byte assigned to
that symbol.

## Durable Format Contract

The durable physical key remains byte-for-byte compatible:

```text
branch_id | space_name\0 | row_family_id | escaped_user_key
```

Only names change. The encoded byte position currently called
`storage_space_id` remains exactly one byte in the same location.

Golden hex files do not need byte changes. Their comments should be updated
from `storage_space_id` to `row_family_id`.

## Implementation Order

### 1. Engine Vocabulary Cleanup

Scope:

1. `crates/engine-next/src/persistence/space.rs`
2. `crates/engine-next/src/persistence/adapter.rs`
3. Engine tests and dependency guards that mention storage-space IDs.

Changes:

1. Rename `RowClass::storage_space_id()` to `RowClass::row_family_id()`.
2. Rename helper functions such as:

```text
storage_space_for_class -> row_family_for_class
storage_space           -> row_family
```

3. Keep generated storage API calls unchanged in this slice if the public API
   still expects `storage_space`.
4. Update source guards to forbid raw storage row-family bytes outside the
   adapter boundary.

Exit criteria:

1. Engine code no longer describes row classes as spaces.
2. No engine behavior or durable bytes change.
3. `engine-next` tests pass.

### 2. Internal Storage Row Rename

Scope:

1. `crates/storage-next/src/row/mod.rs`
2. Storage format codecs.
3. Branch/table/commit/lifecycle runtimes.
4. Storage testkit internals.
5. Storage tests that construct physical rows directly.

Changes:

1. Rename internal row type:

```text
crate::row::StorageSpaceId -> crate::row::RowFamilyId
```

2. Rename accessors:

```text
PhysicalKey::storage_space_id() -> PhysicalKey::row_family_id()
```

3. Rename constants and constructors:

```text
StorageSpaceId::COMMIT_TIMELINE -> RowFamilyId::COMMIT_TIMELINE
StorageSpaceId::engine(...)     -> RowFamilyId::engine(...)
StorageSpaceId::from_raw(...)   -> RowFamilyId::from_raw(...)
```

4. Rename errors:

```text
InvalidStorageSpaceId  -> InvalidRowFamilyId
StorageReservedSpaceId -> StorageReservedRowFamilyId
```

5. Update comments and test names where the meaning is internal row-family
   partitioning.

Exit criteria:

1. Storage internal code uses row-family vocabulary for the one-byte durable
   discriminator.
2. The encoded durable bytes are unchanged.
3. Storage golden hex payloads still match.
4. `storage-next` tests pass.

### 3. Public Storage API Decision

Scope:

1. `crates/storage-next/src/api/atoms.rs`
2. Storage API commit/read request types.
3. Storage API runtime adapter.
4. Testkit API helpers.
5. Engine persistence adapter.
6. Benchmarks using public storage API.

Decision required before implementation:

1. Rename the public API type now:

```text
api::StorageSpaceId -> api::StorageFamilyId
storage_space       -> storage_family
```

2. Or keep public API vocabulary stable for V1 and only fix internal naming.

Recommendation:

Defer the public API rename unless we want storage-next to expose the more
accurate vocabulary before V1. This API is not intended for product users, but
it is still a crate boundary used by engine-next, tests, and benchmarks.

If renamed, keep a temporary compatibility alias only if needed:

```rust
pub type StorageSpaceId = StorageFamilyId;
```

The alias should be short-lived and must not appear in new code.

Exit criteria if implemented:

1. Public storage API no longer uses `storage_space` for row-family selectors.
2. Engine adapter uses the new public API names.
3. Public storage API tests and benchmarks pass.

### 4. Documentation Cleanup

Scope:

1. Active architecture docs under `docs/architecture/storage`.
2. Active architecture docs under `docs/architecture/engine`.
3. V1 docs that describe the durable format.
4. Golden fixture comments.

Changes:

1. Rename active docs:

```text
storage-space-id-registry.md -> row-family-id-registry.md
```

2. Update active references to say row family where the current text means the
   internal byte discriminator.
3. Preserve historical implementation-plan docs unless they are actively used
   as current references.
4. Update `docs/spec/strata-storage-format-v1.md` to describe:

```text
branch_id | product_space_name | row_family_id | user_key
```

Exit criteria:

1. Current architecture docs distinguish product spaces from row families.
2. Historical plans can remain historical, but current docs must not teach the
   confusing terminology.
3. Golden fixture comments match the new vocabulary.

## Testing Plan

### Required Tests

1. `cargo fmt`
2. `cargo test -p storage-next`
3. `cargo test -p engine-next`
4. `cargo test -p executor-next`
5. Storage golden fixture tests.
6. Source guards that enforce row-family vocabulary at current boundaries.

### Regression Assertions

Add or update tests to prove:

1. Physical key encoding bytes are unchanged.
2. Internal key ordering is unchanged.
3. Prefix scans still do not cross product-space boundaries.
4. Prefix scans still do not cross row-family boundaries.
5. Commit timeline rows still use the same durable byte assignment.
6. Engine `RowClass` assignments are unchanged.
7. No public product API exposes row-family IDs.

## Risk Assessment

### Low Risk

1. Engine `RowClass` method rename.
2. Internal storage type rename.
3. Test helper rename.
4. Comment and documentation cleanup.

### Medium Risk

1. Public storage API rename.
2. Guard tests that match source text exactly.
3. Docs with historical references to prior milestone terminology.

### High Risk

1. Any change to physical key byte order.
2. Any change to row-family byte assignments.
3. Any change that removes `space_name` from the physical key.

These high-risk changes are explicitly out of scope.

## Recommended Commit Slices

1. Engine vocabulary only.
2. Internal storage row vocabulary.
3. Source guards, goldens comments, and active docs.
4. Optional public storage API vocabulary.

This keeps each commit reviewable and makes it easy to confirm that no durable
format bytes changed.

## Open Questions

1. Should the public storage API rename happen before V1, or is internal
   cleanup enough?
2. Should we keep a temporary compatibility alias for `StorageSpaceId` if the
   public API is renamed?
3. Should historical implementation plans be left unchanged to preserve
   chronology, or should all docs be swept for vocabulary consistency?
4. Should `RowFamilyId` or `StorageFamilyId` be the canonical storage name?

Recommendation: use `RowFamilyId` internally and keep `RowClass` in
engine-next. Only introduce `StorageFamilyId` if we rename the public
storage-next API.
