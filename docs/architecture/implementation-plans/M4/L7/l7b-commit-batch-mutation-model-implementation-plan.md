# L7B Implementation Plan: Commit Batch And Mutation Model

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-test-plan.md`

## Objective

Define the internal storage commit batch model.

L7B is the first behavioral L7-Core slice, but it still must not allocate
versions, allocate timestamps, append WAL, mutate L6 branch state, publish
visibility, install timeline rows, validate read-set/CAS facts against live
state, or replay durable data.

The slice should make one thing true: given a caller-supplied target branch,
storage-shaped mutations, validation facts, options, and explicit commit facts
from a later clock slice, L7 can validate the batch shape and stamp the user
mutations into `StorageRow` values with one branch, one commit version, and one
commit timestamp.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/commit-timeline-substrate.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-test-plan.md`
8. `crates/storage-next/src/commit/`
9. `crates/storage-next/src/row/mod.rs`
10. `crates/storage-next/src/format/storage_row.rs`
11. `crates/storage-next/src/format/wal/commit_payload.rs`
12. `crates/storage/src/txn/context.rs`
13. `crates/storage/src/txn/validation.rs`
14. `crates/storage/src/traits.rs`

## Existing-Code Source Map

| Current file | L7B evidence | L7B action |
|---|---|---|
| `crates/storage/src/txn/context.rs` | Buffered puts, deletes, CAS facts, read-set facts, TTL map, write buffer limits, and branch guard checks. | Port the storage-shaped intent, not the public transaction context. Replace product `Key`/`Value` with `PhysicalKey` and opaque bytes. |
| `crates/storage/src/txn/validation.rs` | Read-set and CAS fact shapes plus "missing means version zero" behavior. | Define validation fact DTOs only. L7F performs live validation. |
| `crates/storage/src/traits.rs` | `WriteMode::Append` and `KeepLast` as retention hints, atomic puts/deletes sharing one version. | Preserve append-by-default as a storage hint. Keep compaction/retention behavior out of L7B. |
| `crates/storage-next/src/row/mod.rs` | `PhysicalKey`, `StorageSpaceId`, and `StorageRow::put/tombstone`. | Use these as the only row construction surface. Do not introduce product key/value types. |
| `crates/storage-next/src/format/wal/commit_payload.rs` | Durable WAL payload accepts bounded row-native `StorageRow` vectors. | Mirror the durable row-count concern in config tests, but do not construct WAL payloads in L7B. |

## Scope

L7B implements:

1. `CommitBatch` shape;
2. `CommitMutation` shape;
3. commit batch options;
4. duplicate physical-key policy;
5. read-set and CAS validation fact shapes;
6. branch and storage-space validation for caller-supplied rows;
7. configured limit checks;
8. explicit commit-stamp input type;
9. stamped user-row output type;
10. stamping helpers from valid mutation batches to `StorageRow`;
11. direct and generated tests for the above.

L7B does not implement:

1. commit-version allocation;
2. timestamp source or monotonic guard;
3. read-only diagnostic execution;
4. branch registry or generation guard;
5. live conflict validation;
6. timeline row construction;
7. cache/no-WAL commit apply;
8. WAL record or envelope construction;
9. durable phase classification;
10. visible-version publication;
11. replay or allocator catch-up.

## Module Layout

Expected production layout after L7B:

```text
crates/storage-next/src/commit/
  mod.rs
  batch.rs
  config.rs
  error.rs
  facts.rs
  result.rs
  stamp.rs              # optional; may be folded into batch.rs if small
  tests.rs or tests/
```

If direct tests push `tests.rs` past a readable size, split module-local tests
into:

```text
crates/storage-next/src/commit/tests/
  mod.rs
  batch.rs
  stamping.rs
```

Do not add public crate-root exports. All production items remain
`pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitBatch`

Suggested shape:

```text
CommitBatch {
    branch_id: BranchId,
    kind: CommitBatchKind,
    mutations: Vec<CommitMutation>,
    validation: CommitValidationFacts,
    options: CommitBatchOptions,
}
```

Rules:

1. mutating batches require at least one mutation;
2. read-only diagnostic batches carry no mutations and are not stampable;
3. all mutation and validation keys must belong to `branch_id`;
4. caller-supplied mutations may only target engine-owned storage spaces;
5. storage-owned spaces, including commit timeline space, are reserved for L7;
6. row order is the caller's mutation order unless validation rejects the
   batch;
7. batch construction must not allocate commit versions or timestamps;
8. batch construction must not inspect L6 read state.

### `CommitBatchKind`

Suggested shape:

```text
CommitBatchKind::Mutating
CommitBatchKind::ReadOnlyDiagnostic
```

L7B only validates the shape. L7D owns read-only execution.

### `CommitMutation`

Suggested shape:

```text
CommitMutation::Put {
    key: PhysicalKey,
    value: Vec<u8>,
    expires_at: CommitExpiry,
    retention: CommitRetentionHint,
}

CommitMutation::Delete {
    key: PhysicalKey,
}
```

Rules:

1. put value bytes are opaque and may be empty;
2. deletes produce tombstone rows with no value and no expiry;
3. puts may carry a no-expiry marker or a concrete expiry timestamp;
4. an explicit expiry at `Timestamp::EPOCH` is rejected to avoid colliding with
   the existing `StorageRow` no-expiry sentinel;
5. no mutation stores product value types, JSON values, entity refs, graph
   facts, vectors, search terms, or event payload semantics.

### `CommitExpiry`

Suggested shape:

```text
CommitExpiry::None
CommitExpiry::At(Timestamp)
```

Mapping:

1. `None` stamps `StorageRow::put(..., Timestamp::EPOCH, value)`;
2. `At(timestamp)` stamps that exact timestamp;
3. `At(Timestamp::EPOCH)` is invalid.

### `CommitRetentionHint`

Suggested shape:

```text
CommitRetentionHint::Append
CommitRetentionHint::KeepLastNonZero(NonZeroUsize)
```

This is a hint carried forward for later compaction/retention policy. L7B must
not prune historical rows or change L6 visibility based on it. If carrying the
hint into `StorageRow` requires row-format work, keep it in the stamped-batch
metadata and defer durable representation explicitly.

### `CommitBatchOptions`

Suggested shape:

```text
CommitBatchOptions {
    durability: CommitDurabilityMode,
    conflict_validation: CommitConflictValidationMode,
    duplicate_policy: CommitDuplicateKeyPolicy,
    timestamp_policy: CommitTimestampPolicy,
    origin: CommitOrigin,
}
```

Rules:

1. options are explicit enums, not boolean flags;
2. `durability` is a request, not an outcome;
3. `conflict_validation` controls whether L7F will read L6 facts;
4. `duplicate_policy` is `Reject` for V1 unless a later normalizing builder
   lands before validation;
5. `timestamp_policy` is only shape in L7B. L7C owns timestamp allocation and
   monotonicity;
6. `origin` is diagnostic and storage-owned. It must not contain product
   operation names.

### `CommitDurabilityMode`

Suggested shape:

```text
CommitDurabilityMode::Cache
CommitDurabilityMode::Standard
CommitDurabilityMode::Always
```

Do not reuse `CommitDurabilityClass` for requested mode. `CommitDurabilityMode`
is caller intent; `CommitDurabilityClass` is outcome/fact vocabulary.

### `CommitConflictValidationMode`

Suggested shape:

```text
CommitConflictValidationMode::Validate
CommitConflictValidationMode::Skip
```

L7B only records and validates fact shape. L7F decides whether facts match live
branch state.

### `CommitDuplicateKeyPolicy`

Suggested shape:

```text
CommitDuplicateKeyPolicy::Reject
```

Do not add last-write-wins in the core validator. A future builder may normalize
an operation stream before creating a validated `CommitBatch`, but once L7B
validates a batch, duplicate physical keys in `mutations` are an error.

### `CommitTimestampPolicy`

Suggested shape:

```text
CommitTimestampPolicy::RuntimeGenerated
CommitTimestampPolicy::Explicit(Timestamp)
```

`Explicit` exists for replay/test hooks and future controlled imports. L7B does
not generate or validate monotonicity; it only rejects structurally impossible
values if the row model cannot represent them.

### `CommitValidationFacts`

Suggested shape:

```text
CommitValidationFacts {
    read_set: Vec<CommitReadFact>,
    cas_set: Vec<CommitCasFact>,
}

CommitReadFact {
    key: PhysicalKey,
    observed: CommitObservedVersion,
}

CommitCasFact {
    key: PhysicalKey,
    expected: CommitObservedVersion,
}

CommitObservedVersion::Missing
CommitObservedVersion::Present(CommitVersion)
```

Rules:

1. facts must belong to the target branch;
2. caller-supplied facts target engine-owned spaces only;
3. duplicate read facts for the same physical key are rejected;
4. duplicate CAS facts for the same physical key are rejected;
5. read and CAS facts may both mention the same key. L7F interprets them as
   independent validation requirements;
6. `Missing` replaces the old version-zero sentinel at the L7 API boundary;
7. if `Present(CommitVersion::ZERO)` is not meaningful in core-next, reject it
   and use `Missing`.

### `CommitStamp`

Suggested shape:

```text
CommitStamp {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}
```

Rules:

1. supplied by L7C/L7K later;
2. must match the batch target branch;
3. commit version must be nonzero unless storage-core explicitly permits zero
   as a real committed version;
4. commit timestamp may equal another commit's timestamp. Timeline tiebreaking
   uses commit version later.

### `StampedCommitRows`

Suggested shape:

```text
StampedCommitRows {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    rows: Vec<StorageRow>,
    retention_hints: Vec<Option<CommitRetentionHint>>,
}
```

Rules:

1. contains user mutation rows only in L7B;
2. timeline rows are generated in L7G;
3. rows preserve mutation order;
4. every row carries the stamp's branch/version/timestamp;
5. every put preserves the original value bytes and expiry;
6. every delete becomes a tombstone with empty value and `Timestamp::EPOCH`
   expiry;
7. per-row retention hints preserve put retention metadata in mutation order;
8. deletes carry no retention hint;
9. if stamping would produce more rows than config permits, fail before
   returning partial rows.

## Validation Order

L7B validation should run before any allocation-heavy row stamping:

1. validate config;
2. validate batch kind and mutation count;
3. validate validation-fact count;
4. validate all keys match target branch;
5. reject caller use of storage-owned key spaces;
6. reject duplicate mutation physical keys;
7. reject duplicate read facts;
8. reject duplicate CAS facts;
9. validate expiry representation;
10. validate row-count limit;
11. stamp rows.

This order prevents later slices from allocating a commit version for a batch
that would be rejected anyway.

## Error Shape

Extend `CommitRuntimeError` with narrowly-scoped variants rather than routing
all failures through `InvalidCommitState`.

Suggested additions:

```text
InvalidBatch { reason }
InvalidMutation { reason }
InvalidValidationFacts { reason }
DuplicateMutationKey { key_fingerprint_or_len }
BranchMismatch { expected, actual }
StorageOwnedMutationSpace { storage_space_id }
```

If the actual implementation keeps fewer variants, tests must still prove that
each failure category has stable, storage-vocabulary display text and does not
leak value bytes.

## Source Boundary Policy

L7B may import:

1. `crate::row::{PhysicalKey, StorageRow, StorageSpaceId}`;
2. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
3. local `crate::commit` modules.

L7B must not import:

1. `crate::branch` runtime state;
2. `crate::table` internals;
3. `crate::backend`;
4. `crate::layout`;
5. `crate::object`;
6. `crate::service::wal`;
7. `crate::format::wal`;
8. product API or engine crates;
9. filesystem/path/env APIs.

The L7A source guard should be updated only if needed to permit row imports and
still reject branch/table/backend/layout/service/format bypasses for L7B.

## Implementation Steps

### L7B-1: Batch Types

Add `batch.rs` with:

1. batch kind;
2. mutations;
3. options;
4. validation fact types;
5. duplicate-key policy;
6. small constructors and accessors.

Keep constructors validating by default. If unchecked constructors are needed
for tests, hide them behind `#[cfg(test)]`.

### L7B-2: Error Additions

Add typed error variants for batch, mutation, branch, duplicate-key, and
validation-fact failures.

Displays must:

1. include the failing category;
2. avoid value bytes;
3. avoid product transaction terms;
4. avoid object/backend naming.

### L7B-3: Config Integration

Use existing `CommitRuntimeConfig` limits:

1. `max_mutations_per_batch`;
2. `max_validation_facts_per_batch`;
3. `max_commit_rows_per_batch`.

L7B enforces the commit-runtime config limits only. It must not import WAL
format constants or services. L7I owns the additional durable-mode check that a
stamped durable batch fits the V1 WAL payload row and byte caps before it
allocates durable protocol work.

### L7B-4: Validation

Implement `CommitBatch::validate(config)` or a `ValidatedCommitBatch` wrapper.

Prefer a wrapper:

```text
ValidatedCommitBatch
```

so later slices can accept only validated input and avoid re-running structural
checks before allocation.

### L7B-5: Stamping Helpers

Implement stamping from validated mutating batches:

```text
ValidatedCommitBatch::stamp_user_rows(stamp) -> StampedCommitRows
```

The helper must not:

1. allocate a version;
2. allocate a timestamp;
3. append WAL;
4. mutate L6;
5. generate timeline rows.

### L7B-6: Testkit Route

Extend `crates/storage-next/src/testkit/commit_runtime.rs` or split it before
it becomes large.

The generated route should include batch-specific counters:

1. valid batch cases;
2. invalid batch cases;
3. duplicate-key cases;
4. branch-mismatch cases;
5. stamping cases.

### L7B-7: Porting Log

Add an `L7B` section to `m4-l7-porting-log.md` during implementation with:

1. source evidence read;
2. old transaction behavior preserved;
3. old behavior intentionally changed;
4. deferrals to L7C-L7N;
5. tests and guards added;
6. sensitivity probe results.

## Deferred To Later L7 Slices

| Behavior | Owner | Reason |
|---|---|---|
| Commit-version allocation | L7C | L7B only validates and stamps with supplied facts. |
| Timestamp source and monotonic guard | L7C | L7B accepts explicit stamp input only. |
| Read-only diagnostic execution | L7D | L7B only distinguishes read-only batch shape. |
| Branch registry/generation checks | L7E | Requires branch lifecycle state. |
| Read-set/CAS live validation | L7F | Requires L6 read views. |
| Timeline rows | L7G | Storage-owned timeline substrate has separate encoding/query rules. |
| Cache/no-WAL apply | L7H | Requires L6 mutation path and visibility publication. |
| WAL payload/envelope | L7I | L7B must not touch WAL services or format code. |
| Durable-but-not-visible | L7J | Requires WAL success followed by L6/visibility failure. |
| Replay stamping | L7K | Reuses `CommitStamp` but bypasses normal validation in controlled ways. |

## Exit Gate

L7B is complete when:

1. valid mutating batches can be constructed and validated;
2. valid read-only diagnostic batch shape can be represented but not executed;
3. malformed batches fail before stamping;
4. branch-mismatched mutation and validation keys are rejected;
5. caller-supplied storage-owned keys are rejected;
6. duplicate mutation keys are rejected under V1 policy;
7. validation fact limits and duplicate facts are enforced;
8. puts and deletes stamp to correct `StorageRow` values using supplied commit
   facts;
9. stamping preserves mutation order and opaque value bytes;
10. no allocation, WAL, L6 apply, timeline, or visibility behavior is present;
11. direct, generated, source-guard, no-default, wasm, fmt, clippy, and diff
    checks pass.
