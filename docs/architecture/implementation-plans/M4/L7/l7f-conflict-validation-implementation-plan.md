# L7F Implementation Plan: Conflict Validation

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-test-plan.md`

## Objective

Implement the commit-runtime conflict validation step.

L7F answers one storage question after L7E branch admission succeeds and before
any commit version, timestamp, WAL record, L6 mutation, timeline row, or visible
version is produced:

```text
Do the optional read-set and CAS facts still match the target branch view?
```

This slice preserves the V1 internal conflict model from the old storage
transaction validator:

1. read-set facts detect changed observed versions;
2. CAS facts detect mismatched expected versions;
3. blind writes do not conflict;
4. write skew is possible and is not a serializable transaction guarantee;
5. conflict rejection happens before allocation.

L7F does not add public transactions. It validates storage facts already present
on a `ValidatedCommitBatch`.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`
6. `crates/storage/src/txn/validation.rs`
7. `crates/storage/src/txn/context.rs`
8. `crates/storage-next/src/commit/batch.rs`
9. `crates/storage-next/src/commit/branch_registry.rs`
10. `crates/storage-next/src/commit/error.rs`
11. `crates/storage-next/src/branch/read.rs`
12. `crates/storage-next/src/branch/state.rs`

## Existing-Code Source Map

| Current file | L7F evidence | L7F action |
|---|---|---|
| `crates/storage/src/txn/validation.rs` | Old validator compares read-set and CAS expected versions against current storage versions; blind writes are intentionally omitted from conflict checks. | Preserve the model with storage-next physical keys and L6 read views. Do not port product transaction types. |
| `crates/storage/src/txn/context.rs` | Old transaction context captures read-set and CAS facts separately from writes. | Keep `CommitValidationFacts` as facts supplied by callers; L7F only validates them. |
| `crates/storage-next/src/commit/batch.rs` | L7B already validates fact branch, storage space, duplicate facts, and nonzero observed versions. | Reuse validated facts; do not repeat structural batch validation except where needed for defensive checks. |
| `crates/storage-next/src/branch/read.rs` | L6 exposes `BranchReadView::latest` and `BranchVisibleRow::row().commit_version()` for current target-branch visibility. | Use only the narrow read-view surface to compute current observed versions. |
| `crates/storage-next/src/commit/branch_registry.rs` | L7E returns branch admission and guard facts before later commit stages. | Conflict validation runs after admission, while the mutating branch guard is live in integrated paths. |

## Scope

L7F implements:

1. a conflict-read source abstraction or narrow L6 read-view adapter;
2. current observed-version lookup for a physical key;
3. read-set validation;
4. CAS validation;
5. `CommitConflictValidationMode::Skip` bypass behavior;
6. typed conflict result/error vocabulary;
7. source-chain preservation for L6 read failures;
8. direct tests with real `BranchReadView` fixtures;
9. generated testkit counters for conflict scenarios;
10. source-guard updates that allow only the intended L7-to-L6 read-view path.

L7F does not implement:

1. version allocation;
2. timestamp allocation;
3. row stamping;
4. L6 apply or mutable-table install;
5. WAL append or durable records;
6. timeline rows;
7. visible-version publication;
8. public transaction sessions;
9. serializable isolation;
10. read-your-writes staging overlays.

## Module Layout

Expected production layout after L7F:

```text
crates/storage-next/src/commit/
  allocator.rs
  batch.rs
  branch_registry.rs
  conflict.rs      # read-set/CAS validation
  config.rs
  error.rs
  facts.rs
  guard.rs
  outcome.rs
  result.rs
  visibility.rs
  tests/
    allocator.rs
    batch.rs
    branch_registry.rs
    conflict.rs
    guard.rs
    outcome.rs
    scaffold.rs
    visibility.rs
```

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitConflictValidation`

Suggested entrypoint:

```text
validate_commit_conflicts(
    batch: &ValidatedCommitBatch,
    source: &impl CommitConflictReadSource,
) -> CommitRuntimeResult<CommitConflictReport>
```

Rules:

1. `batch.batch().options().conflict_validation() == Skip` returns a skipped
   report without reading the source.
2. Read-only diagnostic batches do not require conflict validation.
3. Mutating batches with empty validation facts return a passed report without
   reading the source.
4. Validation mode `Validate` checks all read facts before CAS facts.
5. Conflict validation does not allocate versions or timestamps.
6. Conflict validation does not mutate L6.

### `CommitConflictReadSource`

Suggested shape:

```text
trait CommitConflictReadSource {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion>;
}
```

Rules:

1. The trait returns storage versions only, never row values.
2. `Missing` means no current visible row for the key in the target branch view.
3. `Present(version)` means the current latest visible non-tombstone row has
   that commit version.
4. Lower-layer read failures map to `CommitRuntimeError::LowerLayer` with
   `CommitLowerLayer::BranchRuntime` and preserve the source chain.
5. Tests may implement the trait with a small in-memory model; production must
   provide an adapter for L6 `BranchReadView`.

### `BranchReadView` Adapter

Suggested shape:

```text
CommitBranchReadViewConflictSource<'a>(&'a BranchReadView)
```

Rules:

1. The adapter uses `BranchReadView::latest(key)`.
2. A visible row maps to `Present(row.commit_version())`.
3. `None` maps to `Missing`.
4. Branch mismatches and L6 read errors are mapped as lower-layer errors.
5. The adapter must not import table internals, backend APIs, WAL APIs, layout,
   or filesystem types.

The source guard should be changed from "no `crate::branch` imports in commit
runtime" to "only the approved branch read-view symbols are allowed in
`commit/conflict.rs`." This is an intentional L7-to-L6 dependency.

### `CommitConflictKind`

Suggested shape:

```text
enum CommitConflictKind {
    ReadSet,
    Cas,
}
```

Rules:

1. `ReadSet` conflicts come from `CommitReadFact`.
2. `Cas` conflicts come from `CommitCasFact`.
3. The kind must be present in diagnostics and tests.
4. The kind must not use product vocabulary such as transaction, document, or
   entity.

### `CommitConflict`

Suggested shape:

```text
CommitConflict {
    kind: CommitConflictKind,
    branch_id: BranchId,
    storage_space_id: StorageSpaceId,
    key_fingerprint: u64,
    user_key_len: usize,
    expected: CommitObservedVersion,
    actual: CommitObservedVersion,
}
```

Rules:

1. Store enough facts for diagnostics without dumping row values.
2. Do not store product keys or value payloads in error display.
3. Preserve the storage space id to make duplicate/conflict tests precise.
4. Preserve a stable storage-local key fingerprint so equal-length keys remain
   distinguishable without storing or displaying user-key bytes.
5. Preserve expected and actual observed versions.
6. Clone no row bytes.

### `CommitConflictReport`

Suggested shape:

```text
CommitConflictReport {
    checked_read_facts: usize,
    checked_cas_facts: usize,
    skipped: bool,
}
```

Rules:

1. Success reports are facts for later generated tests.
2. Skipped reports prove source reads were not performed.
3. Reports do not imply a commit has been allocated, applied, durable, or
   visible.

### Error Vocabulary

Add a typed conflict error, for example:

```text
CommitRuntimeError::CommitConflict { conflict: CommitConflict }
```

Rules:

1. The error is comparable in tests without relying on full display strings.
2. Display text is storage-shaped and bounded.
3. Display text includes conflict kind, branch, storage space, expected
   version, and actual version.
4. Display text must not include product transaction/session vocabulary.
5. Conflict errors have no lower-layer source; L6 read failures remain
   `LowerLayer`.

## Validation Semantics

For each read-set fact:

1. read current observed version from the target branch view;
2. compare current observed version with the fact's observed version;
3. mismatch rejects with `ReadSet` conflict;
4. match continues.

For each CAS fact:

1. read current observed version from the target branch view;
2. compare current observed version with the fact's expected version;
3. mismatch rejects with `Cas` conflict;
4. match continues.

Blind writes:

1. are mutations with no matching read/CAS fact;
2. do not read the source;
3. do not conflict, even when the key changed since the caller last saw it;
4. may still fail later for branch, allocation, L6, WAL, or visibility reasons.

Missing/present semantics:

1. visible non-tombstone latest row means `Present(commit_version)`;
2. no visible latest row means `Missing`;
3. L6 tombstones hide older rows through `BranchReadView::latest`, so they map
   to `Missing` for this V1 conflict model;
4. a prior `Present(v)` fact conflicts with a later delete because current
   observed version becomes `Missing`;
5. a prior `Missing` fact conflicts with a later put because current observed
   version becomes `Present(v)`;
6. a prior `Missing` fact does not conflict with another missing result.

## Ordering In The Commit Pipeline

Integrated L7H/L7I paths should run in this order:

```text
validated batch
  -> L7E branch admission + branch guard
  -> L7F conflict validation
  -> L7C version/timestamp allocation
  -> L7B row stamping
  -> L7G timeline row construction
  -> L7H/L7I apply or WAL
  -> L7D visible-version publication/outcome
```

If conflict validation fails, the branch guard is released by RAII and no later
stage runs.

## Source Guard Update

L7F is the first L7 slice that intentionally touches L6. Update
`commit_runtime_source_guard.rs` so it still forbids:

1. table internals;
2. backend APIs;
3. object/layout APIs;
4. WAL format/service APIs;
5. filesystem/environment/time APIs;
6. engine/product APIs;
7. public transaction/session vocabulary.

It should allow only the minimal branch read-view vocabulary needed by
`commit/conflict.rs`, such as:

1. `crate::branch::BranchReadView`;
2. `crate::branch::BranchVisibleRow`;
3. `crate::branch::BranchRuntimeError` or `BranchRuntimeResult` only if needed
   for source-chain mapping.

Any broader `crate::branch::*` import remains disallowed.

## Generated Testkit

Add a conflict contract helper under either:

```text
crates/storage-next/src/testkit/commit_runtime_conflicts.rs
```

or, if the testkit is split first:

```text
crates/storage-next/src/testkit/commit_runtime/conflicts.rs
```

The generated contract should vary:

1. read-set present match;
2. read-set present mismatch;
3. read-set missing match;
4. read-set missing becoming present;
5. CAS present match;
6. CAS present mismatch;
7. CAS missing match;
8. CAS missing becoming present;
9. blind put after source change;
10. blind delete after source change;
11. skip mode with facts present;
12. lower-layer read failure after some successful checks.

Generated counters should be added to
`CommitRuntimeScaffoldOutcome` and asserted by
`crates/storage-next/tests/commit_runtime_properties.rs`.

## Implementation Steps

### L7F-A: Conflict Types

1. Add `commit/conflict.rs`.
2. Add `CommitConflictKind`.
3. Add `CommitConflict`.
4. Add `CommitConflictReport`.
5. Add a typed conflict error in `CommitRuntimeError`.
6. Export the new types from `commit/mod.rs` as `pub(crate)`.

### L7F-B: Read Source Abstraction

1. Add `CommitConflictReadSource`.
2. Add helper to convert an optional visible row to `CommitObservedVersion`.
3. Add an L6 `BranchReadView` adapter.
4. Preserve L6 errors through `CommitRuntimeError::lower_layer_with`.
5. Keep source reads version-only from L7's point of view.

### L7F-C: Validator

1. Implement skip-mode short-circuit.
2. Implement empty-facts short-circuit.
3. Validate read-set facts.
4. Validate CAS facts.
5. Return the first conflict as a typed error.
6. Return checked-fact counts on success.

### L7F-D: Direct Tests

1. Add `crates/storage-next/src/commit/tests/conflict.rs`.
2. Build real `BranchReadView` fixtures using L6 test helpers or local storage
   rows.
3. Add fake read-source tests for skip/no-read and lower-layer failure.
4. Cover read-set, CAS, blind writes, missing/present transitions, and error
   display.

### L7F-E: Generated Tests

1. Add generated conflict contract helper.
2. Wire it into `CommitRuntimeScaffoldOutcome`.
3. Widen script length if new script indices are used.
4. Assert every conflict counter in the property harness.
5. Keep generated tests deterministic and bounded.

### L7F-F: Source Guards And Porting Log

1. Update source guard to allow only narrow branch read-view imports.
2. Add source-guard fixture assertions for forbidden broad branch imports.
3. Add L7F evidence to `m4-l7-porting-log.md`.
4. Run the full L7F command set.

## Required Verification

Run at minimum:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## Exit Gate

L7F is complete when:

1. read-set matches pass;
2. read-set mismatches reject before allocation;
3. CAS matches pass;
4. CAS mismatches reject before allocation;
5. blind writes do not conflict;
6. skip mode reads nothing and passes;
7. lower-layer read failures preserve source chains;
8. direct and generated tests cover missing/present transitions;
9. source guards still prevent table/backend/WAL/product leakage;
10. the parent L7 conflict-model requirements are reflected in tests.

## Deferred

1. Public transaction sessions remain retired.
2. Serializable isolation is not implemented.
3. Read-your-writes staging overlays remain above or outside L7F.
4. Replay conflict bypass is implemented in L7K.
5. Durable/cache commit integration is implemented in L7H and L7I.
6. Full generated/fuzz conflict scripting is strengthened in L7M.
