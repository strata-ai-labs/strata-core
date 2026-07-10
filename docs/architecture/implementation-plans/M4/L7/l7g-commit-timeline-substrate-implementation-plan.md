# L7G Implementation Plan: Commit Timeline Substrate

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-test-plan.md`

## Objective

Implement the storage-owned commit timeline substrate for L7.

L7G answers one storage question after L7C can allocate commit facts and before
L7H applies a cache/no-WAL commit into L6:

```text
Given one branch, one commit version, and one commit timestamp, what storage
rows and lookup facts represent this commit in the generic per-branch timeline?
```

The timeline substrate is required for future `as_of`, branch-from-time, and
retained-history diagnostics, but L7G does not implement product time-travel
APIs. It creates storage-owned rows and helper queries that later slices can
install, persist, replay, and expose through L9.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/storage/storage-space-id-registry.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
9. `crates/storage-next/src/commit/`
10. `crates/storage-next/src/row/mod.rs`
11. `crates/storage-next/src/format/key.rs`
12. `crates/storage-next/src/branch/read.rs`
13. `crates/storage/src/txn/manager.rs`
14. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L7G evidence | L7G action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Old storage assigned monotonic commit versions and tracked visible versions, but did not have a storage-native timestamp index. | Preserve version/timestamp commit facts. Add the missing explicit timeline substrate instead of depending on scans over user rows. |
| `crates/storage/src/segmented/mod.rs` | Old storage stored commit timestamps on rows and used version-bounded reads. | Keep row timestamps as row facts, but add timeline rows so timestamp-to-version lookup does not require scanning key histories. |
| `crates/storage-next/src/row/mod.rs` | `StorageSpaceId::COMMIT_TIMELINE` is the storage-owned `0x01` row family. `StorageRow` already carries branch, version, timestamp, tombstone, expiry, and value facts. | Build timeline rows as ordinary `StorageRow::put` rows under storage space `0x01`. |
| `crates/storage-next/src/format/key.rs` | Physical key encoding preserves branch, space, storage-space id, and arbitrary user-key bytes. | Use deterministic timeline user-key bytes so ordinary key ordering supports timestamp/version lookup. |
| `crates/storage-next/src/commit/batch.rs` | L7B rejects caller-supplied storage-owned spaces. | Keep caller-supplied mutations rejected; L7G is the only commit slice that generates timeline-space rows. |

## Scope

L7G implements:

1. timeline row/fact types;
2. deterministic timeline physical-key construction;
3. timestamp-to-version row construction;
4. version-to-timestamp row construction;
5. value encoders and decoders for timeline rows;
6. timeline row validation helpers;
7. timestamp lookup helpers over retained timeline rows;
8. version lookup helpers over retained timeline rows;
9. branch-isolated timeline bounds/report facts;
10. direct tests and generated testkit counters for timeline behavior;
11. source-guard updates for the new timeline module.

L7G does not implement:

1. cache/no-WAL commit apply into L6;
2. durable WAL append;
3. durable-but-not-visible classification;
4. replay or repair;
5. retention policy;
6. compaction policy;
7. public `as_of` APIs;
8. branch-from-time product workflows;
9. timeline object services or new L2/L4 object families;
10. scanning arbitrary user rows to synthesize timeline answers.

## Module Layout

Expected production layout after L7G:

```text
crates/storage-next/src/commit/
  allocator.rs
  batch.rs
  branch_registry.rs
  conflict.rs
  config.rs
  error.rs
  facts.rs
  guard.rs
  outcome.rs
  result.rs
  timeline.rs      # storage-owned timeline row/fact/query helpers
  visibility.rs
  tests/
    allocator.rs
    batch.rs
    branch_registry.rs
    conflict.rs
    guard.rs
    outcome.rs
    scaffold.rs
    timeline.rs
    visibility.rs
```

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitTimelineEntry`

Suggested shape:

```text
CommitTimelineEntry {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}
```

Rules:

1. `commit_version` must not be `CommitVersion::ZERO`.
2. `Timestamp::EPOCH` is allowed because L7C permits epoch commit timestamps.
3. The entry is a storage fact, not a product event time.
4. The entry does not contain user rows or value bytes.
5. The same entry must produce the same timeline rows every time.

### Timeline Physical Keys

L7G should define one storage-owned physical-key space string and two user-key
prefixes inside `StorageSpaceId::COMMIT_TIMELINE`.

Suggested constants:

```text
COMMIT_TIMELINE_SPACE = "timeline"
TIMESTAMP_INDEX_PREFIX = b"ts-v1\0"
VERSION_INDEX_PREFIX = b"ver-v1\0"
```

Suggested user-key layouts:

```text
timestamp index:
  b"ts-v1\0" + commit_timestamp.as_micros().to_be_bytes()
             + commit_version.as_u64().to_be_bytes()

version index:
  b"ver-v1\0" + commit_version.as_u64().to_be_bytes()
```

Rules:

1. Both physical keys use the entry branch id.
2. Both physical keys use `StorageSpaceId::COMMIT_TIMELINE`.
3. Both physical keys use the timeline space constant.
4. The timestamp index includes commit version in the key so equal timestamps
   order deterministically by version.
5. Byte ordering must support "greatest retained version at or before
   timestamp T" by scanning timestamp-index keys.
6. The key layout is storage-owned and must not expose product row keys.
7. The exact constants become durable V1 compatibility facts once implemented.

### Timeline Values

Suggested values:

```text
timestamp index value:
  commit_version.as_u64().to_be_bytes()

version index value:
  commit_timestamp.as_micros().to_be_bytes()
```

Rules:

1. Values are fixed-width 8-byte big-endian integers.
2. Values duplicate the key facts intentionally, so corruption can be detected.
3. Decode must reject short, long, or trailing bytes.
4. Decode must reject mismatched key/value facts.
5. Values must not contain product data.

### `CommitTimelineRows`

Suggested shape:

```text
CommitTimelineRows {
    entry: CommitTimelineEntry,
    timestamp_to_version: StorageRow,
    version_to_timestamp: StorageRow,
}
```

Rules:

1. Both rows are `StorageRow::put`.
2. Both rows carry the entry commit version.
3. Both rows carry the entry commit timestamp.
4. Both rows use `Timestamp::EPOCH` as no-expiry.
5. Neither row is a tombstone.
6. The rows are installed with user rows by L7H/L7I/L7K, not by L7G.
7. `timeline_row_count` for one mutating commit is exactly `2`.

### `CommitTimelineRowKind`

Suggested shape:

```text
enum CommitTimelineRowKind {
    TimestampToVersion,
    VersionToTimestamp,
}
```

Rules:

1. The kind is inferred from the user-key prefix.
2. Unknown prefixes are typed invalid timeline rows.
3. Strict timeline-row validators fail closed on other storage spaces; view
   builders that accept arbitrary retained branch rows skip non-timeline rows
   before validating timeline candidates.

### Query Helpers

Suggested helper shapes:

```text
CommitTimelineView::from_rows(branch_id, rows) -> CommitRuntimeResult<Self>

CommitTimelineView::version_at_or_before(timestamp)
    -> CommitRuntimeResult<CommitTimelineLookup>

CommitTimelineView::timestamp_for_version(version)
    -> CommitRuntimeResult<Option<Timestamp>>

CommitTimelineView::bounds() -> CommitTimelineBounds
```

Suggested lookup result:

```text
CommitTimelineLookup {
    query_timestamp: Timestamp,
    matched_version: Option<CommitVersion>,
    matched_timestamp: Option<Timestamp>,
    miss: CommitTimelineMiss,
}

CommitTimelineMiss {
    Matched,
    BeforeRetainedHistory,
    AfterLatestRetained,
    Empty,
}
```

Rules:

1. Lookup at exact timestamp returns the greatest commit version at that
   timestamp.
2. Lookup between two timestamps returns the greatest retained version before
   the query timestamp.
3. Lookup before the first retained timestamp reports `BeforeRetainedHistory`.
4. Lookup after the latest retained timestamp returns the latest retained
   version and reports `AfterLatestRetained` only if the API needs miss
   diagnostics separate from the match.
5. Empty timeline returns no version and an empty miss fact.
6. Version lookup returns the original commit timestamp for that version.
7. Duplicate version rows with identical facts are idempotent.
8. Duplicate version rows with conflicting timestamps are typed corruption.
9. Timestamp-index and version-index rows for the same version must agree.

## Commit Path Integration Points

L7G supplies pure helpers to later commit paths:

1. L7H calls `CommitTimelineRows::from_entry` after stamping user rows and
   applies the returned rows atomically with the user rows.
2. L7I includes the same rows in the `WalRecord` payload before L6 visibility.
3. L7K replay validates durable timeline rows and catches up lookup facts
   without allocating new versions.
4. L7N closeout verifies generated/fuzz coverage and source boundaries.

L7G must not call L6 mutation APIs itself. This keeps the slice reviewable and
prevents a partial cache commit path from landing before L7H.

## Error And Outcome Changes

Add typed errors as needed, for example:

```text
CommitRuntimeError::InvalidTimelineFact { reason: &'static str }
CommitRuntimeError::TimelineConflict { reason: &'static str }
```

Rules:

1. Invalid timeline rows are distinct from invalid user mutations.
2. Error display must stay storage-shaped.
3. Error display must not mention `as_of`, transaction sessions, JSON, graph,
   vector, search, documents, datasets, or remotes.
4. Lower-layer errors are not expected in pure L7G helpers; L7H/L7I own apply
   and WAL failures.

## Generated Testkit

Add generated timeline coverage under either:

```text
crates/storage-next/src/testkit/commit_runtime_timeline.rs
```

or:

```text
crates/storage-next/src/testkit/commit_runtime/timeline.rs
```

The generated contract should build independent model entries and compare:

1. row construction facts;
2. timestamp lookup;
3. version lookup;
4. duplicate timestamp tiebreak;
5. branch isolation;
6. malformed row rejection.

## Source Guard Policy

`commit/timeline.rs` may import:

1. `crate::row::{PhysicalKey, StorageRow, StorageSpaceId}`;
2. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
3. standard collection types.

`commit/timeline.rs` must not import:

1. `crate::branch`;
2. `crate::table`;
3. `crate::backend`;
4. `crate::layout`;
5. `crate::object`;
6. `crate::service`;
7. `crate::format::wal`;
8. filesystem, environment, or clock APIs;
9. engine/product crates or product vocabulary.

The module is pure row/fact construction. L7H and L7I own the layers that need
L6 and L4.

## Implementation Steps

### L7G-A: Timeline Module Shell

1. Add `commit/timeline.rs`.
2. Export its crate-private surface from `commit/mod.rs`.
3. Add `commit/tests/timeline.rs`.
4. Update source guards for the new module.

### L7G-B: Entry And Key Layout

1. Add `CommitTimelineEntry`.
2. Add timeline key constants.
3. Add timestamp-index physical-key constructor.
4. Add version-index physical-key constructor.
5. Reject zero commit versions.

### L7G-C: Row Construction

1. Add fixed-width value encoders.
2. Add `CommitTimelineRows::from_entry`.
3. Ensure both rows share entry commit facts.
4. Ensure row count is stable at two.

### L7G-D: Row Decode And Validation

1. Decode row kind from prefix.
2. Decode timestamp/version facts from key and value.
3. Validate row storage space, branch id, row version, row timestamp, expiry,
   tombstone bit, and value length.
4. Reject mismatched key/value/row facts.

### L7G-E: Lookup Helpers

1. Build a branch-local view from retained timeline rows.
2. Resolve timestamp to greatest retained version at or before `T`.
3. Resolve commit version to timestamp.
4. Report retained bounds.
5. Detect conflicting duplicate facts.

### L7G-F: Testkit And Porting Log

1. Add generated timeline contract counters.
2. Wire counters into `commit_runtime_properties.rs`.
3. Record old-code source map and intentional new substrate in the L7 porting
   log.
4. Update parent L7 plan links.

## Exit Gate

L7G is complete when:

1. one timeline entry deterministically creates exactly two storage-owned rows;
2. both rows carry the same commit version and timestamp as the entry;
3. timestamp lookup returns the greatest retained version at or before `T`;
4. equal timestamps tie-break by greatest commit version;
5. version lookup returns the original commit timestamp;
6. branch isolation is proven;
7. malformed timeline rows fail closed with typed errors;
8. generated tests cover construction, lookup, branch isolation, and
   corruption;
9. source guards prove the timeline module stays pure and does not import L6,
   L4, backend, table, filesystem, or product APIs;
10. no public `as_of` or product timeline API is introduced.

## Deferred

1. Atomic installation of user rows plus timeline rows: `L7H`.
2. WAL durability of timeline rows: `L7I`.
3. Durable-but-not-visible timeline failure classification: `L7J`.
4. Replay and recovery validation of timeline rows: `L7K` and L8.
5. Retention and compaction policy over timeline rows: later L6/L8 retention
   integration.
6. Public timestamp selectors and branch-from-time APIs: L9/engine-next.
7. Fuzz targets and larger generated timeline scripts: `L7M`.
