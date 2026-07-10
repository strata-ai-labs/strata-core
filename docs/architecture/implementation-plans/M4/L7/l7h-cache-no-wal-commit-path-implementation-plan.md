# L7H Implementation Plan: Cache/No-WAL Commit Path

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-test-plan.md`

## Objective

Implement the cache/no-WAL commit path for storage-next L7.

L7H is the first L7 slice that turns the already-defined commit facts into
visible L6 state. It must validate and admit one target-branch batch, allocate
one commit version and timestamp, stamp user rows, generate the L7G timeline
rows, install the whole row set into L6 atomically, publish visible version,
and return a visible non-durable outcome.

L7H deliberately does not provide crash durability. It is the in-memory commit
protocol used when the caller selected `CommitDurabilityMode::Cache` or when a
backend/runtime is configured for cache mode. Durable WAL integration starts in
L7I and must reuse the same stamped user rows plus timeline rows.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/commit-timeline-substrate.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md`
12. `crates/storage-next/src/commit/`
13. `crates/storage-next/src/branch/state.rs`
14. `crates/storage-next/src/branch/read.rs`
15. `crates/storage/src/txn/manager.rs`
16. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L7H evidence | L7H action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Old storage coordinated branch commit locks, version allocation, no-WAL write application, visible-version publication, and failure accounting. | Port the storage ordering. Do not port public transaction ids, long-lived transaction sessions, or product observer hooks. |
| `crates/storage/src/segmented/mod.rs` | `apply_writes_atomic` and `apply_recovery_atomic` stage writes before making them visible. | Preserve atomic apply semantics over L6 rows. Use storage-next `StorageRow` and L6 branch state, not old segmented table internals. |
| `crates/storage/src/traits.rs` | Old storage had a storage-level atomic write surface independent of WAL. | Keep the cache path as storage-internal commit behavior. Public API mapping remains L9. |
| `crates/storage-next/src/commit/batch.rs` | L7B validates batches and stamps user rows with one `CommitStamp`. | Reuse `ValidatedCommitBatch::stamp_user_rows`; do not restamp or bypass validation. |
| `crates/storage-next/src/commit/allocator.rs` | L7C allocates exactly one version and timestamp for mutating batches. | Allocate after admission/conflict validation and before row stamping. Accept version gaps after post-allocation failures. |
| `crates/storage-next/src/commit/branch_registry.rs` | L7E admits writable branches and owns guard acquisition. | Use admission before allocation. Keep guard held through L6 apply and visibility publication. |
| `crates/storage-next/src/commit/conflict.rs` | L7F validates read-set/CAS facts over L6 read views. | Run conflict validation before allocation, using a read view captured before L6 mutation. |
| `crates/storage-next/src/commit/timeline.rs` | L7G creates timeline rows for one commit. | Generate timeline rows after stamping user rows and install them in the same L6 atomic apply unit. |
| `crates/storage-next/src/branch/state.rs` | L6 exposes `BranchLocalState::append_committed_row` and read-view capture. | Add or use a narrow atomic multi-row append helper so L7H cannot partially install a batch. |

## Scope

L7H implements:

1. cache-mode commit orchestration;
2. an L7-to-L6 cache apply boundary;
3. cache-mode durability-mode validation;
4. branch admission and guard integration;
5. conflict validation before allocation;
6. commit fact allocation after validation/admission/conflict checks;
7. user-row stamping through L7B;
8. timeline-row generation through L7G;
9. atomic L6 apply of user rows plus timeline rows;
10. visible-version publication after full L6 apply;
11. visible non-durable `CommitOutcome`;
12. not-visible phase facts for cache-mode failures after allocation;
13. direct tests and generated counters for cache commit behavior;
14. source-guard updates for the intentional L7H-to-L6 mutation boundary.

L7H does not implement:

1. WAL record construction;
2. WAL envelope append;
3. `standard` or `always` durable commit modes;
4. durable-but-not-visible classification;
5. replay or recovery entrypoints;
6. process-open recovery orchestration;
7. checkpoint, compaction, retention, or flush scheduling;
8. public transaction sessions;
9. product `as_of` APIs;
10. engine observer side effects.

## Protocol

The cache/no-WAL mutating commit path is:

```text
validate batch shape
reject non-cache durability mode
admit target branch and acquire branch guard
capture target branch read view
validate read-set/CAS facts at current visible version
allocate one commit version and timestamp
reject allocation if it is not greater than current visible version
stamp user rows
generate two timeline rows
atomically apply user rows + timeline rows into L6
publish visible version
return CommitOutcome { durable: NotDurable, visible: true }
```

Required ordering:

1. malformed batch rejects before branch guard;
2. missing/deleting/generation-mismatched branch rejects before allocation;
3. conflict rejects before allocation;
4. cache mode allocates no WAL facts;
5. user rows and timeline rows share one `CommitStamp`;
6. L6 apply must finish before visible-version publication;
7. visible publication must happen while the branch guard remains live;
8. guard release happens on every return path by RAII.
9. if the target branch already has applied rows above the current visible
   version, L7H must fail closed before allocation;
10. conflict validation uses the current visible version as the L6 read upper
    bound, not raw branch latest.

## Module Layout

Expected production layout after L7H:

```text
crates/storage-next/src/commit/
  allocator.rs
  batch.rs
  branch_registry.rs
  cache.rs          # cache/no-WAL executor and row set assembly
  conflict.rs
  config.rs
  error.rs
  facts.rs
  guard.rs
  outcome.rs
  result.rs
  timeline.rs
  visibility.rs
  tests/
    cache.rs
```

If the L6 atomic apply helper is added, keep it in `branch/state.rs` or a
narrow branch submodule and expose only the branch-runtime primitive through
`branch/mod.rs`. The helper must not import `crate::commit`.

All new production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if responsibilities stay intact.

### `CommitCacheRuntime`

Suggested shape:

```text
CommitCacheRuntime<'a, S> {
    config: &'a CommitRuntimeConfig,
    registry: &'a CommitBranchRegistry,
    guard_set: &'a CommitBranchGuardSet,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut BranchLocalState,
    visible: &'a mut VisibleVersionTracker,
}
```

Suggested entrypoint:

```text
execute(
    &mut self,
    batch: CommitBatch,
    generation_guard: CommitBranchGenerationGuard,
) -> CommitRuntimeResult<CommitOutcome>
```

Rules:

1. `CommitCacheRuntime` is an orchestration helper, not a public transaction
   manager.
2. The runtime validates `CommitDurabilityMode::Cache`.
3. It rejects a branch-state branch id that differs from the batch branch id.
4. It validates the batch before branch admission.
5. It admits the branch before conflict validation and allocation.
6. It captures an L6 read view before applying rows.
7. It holds the branch admission guard until after visibility publication.
8. It returns typed commit-runtime errors with lower-layer source chains where
   useful.

If this shape becomes too broad during implementation, split into:

1. `prepare_cache_commit_rows`;
2. `apply_cache_commit_rows`;
3. `publish_cache_commit_outcome`.

Do not hide phase ordering inside a large untestable function.

### `CacheCommitRows`

Suggested shape:

```text
CacheCommitRows {
    stamp: CommitStamp,
    user_rows: StampedCommitRows,
    timeline_rows: CommitTimelineRows,
}
```

Rules:

1. User rows come only from `ValidatedCommitBatch::stamp_user_rows`.
2. Timeline rows come only from `CommitTimelineRows::from_entry`.
3. The combined row set has user rows first and timeline rows after, unless
   tests prove order independence.
4. The combined row set is validated before L6 apply.
5. The row count used in `CommitMutationCounts` includes the two timeline
   rows.

### Atomic L6 Apply Helper

Suggested branch-runtime helper:

```text
BranchLocalState::append_committed_rows_atomically(
    rows: impl IntoIterator<Item = StorageRow>,
) -> BranchRuntimeResult<BranchBatchAppendOutcome>
```

Rules:

1. All rows must belong to the branch.
2. Every row must have a nonzero commit version.
3. The helper stages changes on a clone or equivalent temporary state.
4. If any row fails validation or insertion, the original branch state is
   unchanged.
5. Duplicate internal keys inside the batch or against existing branch rows
   reject the whole batch.
6. The helper does not publish L7 visible version.
7. The helper does not know about WAL, commit outcomes, branch registry, or
   conflict validation.

If implementation can safely stage inside L7 without adding a branch helper,
the staging logic must still be reusable by L7K replay and covered by direct
atomicity tests. Prefer the branch helper if reuse is clear.

### Visibility Facts

For successful cache commits:

```text
allocated = Some(version)
durable   = None
applied   = Some(version)
timeline  = Some(version)
visible   = Some(version)
```

Rules:

1. `CommitDurabilityClass::NotDurable` is required.
2. `CommitPhase::Visible` is required.
3. The `visible` tracker is advanced only after L6 apply succeeds.
4. `timeline` is set only if the timeline rows were included in the L6 apply.
5. The returned outcome must count puts, deletes, and exactly two timeline
   rows per mutating commit.

For cache-mode failures after allocation and before apply:

```text
allocated = Some(version)
durable   = None
applied   = None
timeline  = None
visible   = previous visible version or None, according to outcome shape
```

For cache-mode failures after apply and before visible publication:

```text
allocated = Some(version)
durable   = None
applied   = Some(version)
timeline  = Some(version)
visible   != Some(version)
```

The first implementation may return a typed error rather than a
`CommitOutcome` for pre-visible cache failures if existing outcome surfaces are
not ready. The error must still preserve enough phase facts in tests to prove
that no durability was claimed and visibility was not published. If a
`CommitOutcomeKind::NotVisible` constructor is missing, L7H should add one
rather than constructing impossible facts by hand.

## Failure Semantics

### Before Allocation

These failures must leave allocator, L6 state, and visible version unchanged:

1. invalid config;
2. invalid batch;
3. unsupported non-cache durability mode;
4. missing target branch;
5. deleting or deleted target branch;
6. branch generation mismatch;
7. branch-state branch mismatch;
8. read-set conflict;
9. CAS conflict;
10. lower-layer read failure during conflict validation;
11. branch already has applied rows above current visible version.

### After Allocation Before Apply

These failures may leave a version gap but must leave L6 and visible version
unchanged:

1. row stamping failure;
2. timeline row construction failure;
3. combined row-count overflow;
4. atomic L6 apply rejection before staged install.
5. allocated commit version is not greater than current visible version.

### After Apply Before Visibility

These failures leave rows installed in L6 but not published visible through the
L7 tracker:

1. visible-version regression or publication failure;
2. impossible visibility facts detected after apply.

This is not durable-but-not-visible because cache mode has no WAL durability.
The phase is `AppliedNotVisible`, durability is `NotDurable`, and later slices
must decide whether the process-local runtime can reconcile or must stop
writes. L7H should at least fail closed and not claim a visible commit.

## Conflict Validation

L7H must invoke L7F as part of the integrated path.

Rules:

1. The branch read view is captured after branch admission and before
   allocation.
2. Read-set facts are checked before CAS facts, preserving L7F behavior.
3. `CommitConflictValidationMode::Skip` performs no source reads.
4. Conflict rejection happens before version allocation.
5. Conflict source lower-layer errors preserve source chains.
6. L7H caps the conflict source at `VisibleVersionTracker::visible_version()`;
   rows applied in L6 but not yet visible do not participate in optimistic
   validation.

## Timeline Integration

For every successful mutating cache commit:

1. create `CommitTimelineEntry` from the allocated `CommitStamp`;
2. create `CommitTimelineRows`;
3. append both timeline rows to the same L6 atomic apply set as user rows;
4. set `timeline_version` in visibility facts to the commit version;
5. count two timeline rows in `CommitMutationCounts`;
6. prove timeline lookup works over the post-commit branch read view.

L7H must not allow callers to supply their own timeline rows. L7B already
rejects storage-owned mutation spaces; L7H uses only L7G helpers to generate
timeline rows.

## Source Guard Policy

L7H intentionally expands the commit-to-branch boundary.

Production `commit/cache.rs` may import:

1. `crate::branch::{BranchLocalState, BranchReadView}` or narrower approved
   symbols needed for cache apply and conflict validation;
2. existing `crate::commit` modules;
3. `crate::row::StorageRow` if needed for combined row sets;
4. core-next branch/version/timestamp atoms.

Production `commit/cache.rs` must not import:

1. `crate::format::wal`;
2. `crate::service::wal`;
3. `crate::backend`;
4. `crate::layout`;
5. `crate::object`;
6. `crate::table` internals directly;
7. filesystem, environment, process clock, or product APIs.

If the current source guard only allows branch imports in `commit/conflict.rs`,
update it to allow exactly the required branch symbols in `commit/cache.rs` and
keep all other branch imports rejected.

## Implementation Steps

### L7H-A: Branch Atomic Append Surface

1. Decide whether to add `BranchLocalState::append_committed_rows_atomically`
   or stage via a reusable L7 helper.
2. Validate same-branch rows before mutation.
3. Validate duplicate internal keys before committing staged state.
4. Ensure a failure leaves original branch state byte-for-byte unchanged.
5. Add direct branch or commit tests for partial-apply rejection.

### L7H-B: Cache Commit Row Preparation

1. Add `commit/cache.rs`.
2. Add cache-mode durability validation.
3. Add combined user+timeline row assembly.
4. Add row-count validation that includes timeline rows.
5. Add mutation count construction with two timeline rows.

### L7H-C: Integrated Cache Executor

1. Validate `CommitBatch`.
2. Admit branch and hold branch guard.
3. Capture L6 read view.
4. Run conflict validation.
5. Allocate commit facts.
6. Prepare rows.
7. Apply rows atomically into L6.
8. Publish visible version.
9. Return visible non-durable outcome.

### L7H-D: Failure Phase Mapping

1. Add or expose a `CommitOutcome::not_visible` constructor if needed.
2. Map pre-allocation failures to no-allocation behavior.
3. Map allocated-not-applied failures to `AllocatedNotDurable` facts where an
   outcome is returned.
4. Map applied-not-visible failures to `AppliedNotVisible` facts where an
   outcome is returned.
5. Preserve lower-layer source chains for branch apply and conflict failures.

### L7H-E: Testkit And Porting Log

1. Add generated cache-commit counters to `commit_runtime_properties.rs`.
2. Add a small independent cache-commit model to the testkit.
3. Record preserved no-WAL behavior, intentionally changed public transaction
   behavior, and deferred WAL behavior in the L7 porting log.
4. Update source guards for the new L7-to-L6 mutation boundary.

## Exit Gate

L7H is complete when:

1. cache-mode put/delete commits appear in L6 after visibility;
2. user rows and timeline rows install atomically;
3. one mutating batch allocates exactly one version and timestamp;
4. no version is allocated on validation, admission, or conflict failure;
5. version gaps after post-allocation failure are accepted and tested;
6. visible version advances only after full L6 apply;
7. successful outcomes are visible and `NotDurable`;
8. cache path performs no WAL construction, append, or durability claim;
9. branch guard and generation checks are integrated;
10. conflict validation runs before allocation;
11. source guards allow only the intended L6 boundary;
12. generated tests compare cache commits against an independent model.

## Deferred

1. WAL record construction and envelope append: `L7I`.
2. Durable-but-not-visible classification: `L7J`.
3. Replay and allocator catch-up over durable rows: `L7K`.
4. Quiesce hardening under concurrent cache commits: `L7L`.
5. Expanded generated/fuzz/fault scripts: `L7M`.
6. Public storage API mapping: L9.
7. Process open/recovery orchestration: L8.
