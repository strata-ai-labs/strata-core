# L7E Implementation Plan: Branch Registry And Commit Guards

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-test-plan.md`

## Objective

Implement the commit-runtime admission layer for target branches.

L7E sits between the L7B/L7C/L7D fact machinery and the later commit paths
that mutate L6 or append WAL. It answers one storage question before any
version allocation, timestamp allocation, WAL append, or L6 mutation:

```text
May this mutating commit start against this branch right now?
```

This slice adds the branch registry model, branch generation validation,
per-branch commit guard, branch deletion marker, and nonblocking quiesce
skeleton needed by later L7F through L7K. It does not implement conflict
validation, timeline rows, cache commits, durable commits, or replay.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
6. `crates/storage-next/src/commit/`
7. `crates/storage-next/src/commit/batch.rs`
8. `crates/storage-next/src/commit/error.rs`
9. `crates/storage-next/src/commit/facts.rs`
10. `crates/storage-next/src/commit/outcome.rs`
11. `crates/storage/src/txn/manager.rs`
12. `crates/storage/src/txn/lock_ordering.rs`
13. `crates/engine/src/database/transaction.rs`

## Existing-Code Source Map

| Current file | L7E evidence | L7E action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Old storage manager had branch commit locks, pending/deleting barriers, quiesce, and visible-version separation. | Port the storage-owned guard model only. Do not port public transaction sessions or transaction ids. |
| `crates/storage/src/txn/lock_ordering.rs` | Old code made lock acquisition order explicit to avoid deadlocks. | Rebuild as simple L7 guard APIs that enforce quiesce-before-branch acquisition by construction. |
| `crates/engine/src/database/transaction.rs` | Engine code validated branch generation and writer health before commit. | Keep product writer-health above L7. L7E only validates storage branch availability and optional generation facts. |
| `crates/storage-next/src/commit/batch.rs` | Validated batches already carry one target branch and reject cross-branch user rows. | L7E should accept only validated target-branch facts; it must not re-parse values or inspect product payloads. |
| `crates/storage-next/src/commit/outcome.rs` | L7D added read-only outcome and visibility facts. | L7E guard admission feeds later mutating outcomes but does not construct visible commit outcomes by itself. |

## Scope

L7E implements:

1. branch registry descriptors keyed by `BranchId`;
2. branch lifecycle state needed for commit admission;
3. branch generation facts and exact generation validation;
4. duplicate branch registration rejection;
5. missing branch rejection;
6. branch deleting/deleted rejection before allocation;
7. per-branch mutating commit guard tokens;
8. nonblocking quiesce skeleton that blocks new mutating commits;
9. branch admission helper for `ValidatedCommitBatch`;
10. generated testkit counters for registry and guard behavior;
11. source-guard coverage for the new commit modules.

L7E does not implement:

1. public branch create/delete API;
2. durable branch catalog persistence;
3. release of L6 branch state during delete;
4. conflict validation over L6 read views;
5. cache/no-WAL commit apply;
6. WAL record construction or append;
7. durable-but-not-visible write gate;
8. replay, repair, or recovery orchestration;
9. blocking waits, condition variables, or timeout scheduling for quiesce;
10. product branch merge, restore, fork-at-history, or retained-history policy.

## Module Layout

Expected production layout after L7E:

```text
crates/storage-next/src/commit/
  allocator.rs
  batch.rs
  branch_registry.rs  # branch descriptors, generation facts, admission
  config.rs
  error.rs
  facts.rs
  guard.rs            # per-branch guard and quiesce skeleton
  outcome.rs
  result.rs
  visibility.rs
  tests/
    allocator.rs
    batch.rs
    branch_registry.rs
    guard.rs
    outcome.rs
    scaffold.rs
    visibility.rs
```

If the implementation remains small, `branch_registry.rs` and `guard.rs` may
start as one file, but split them before registry lifecycle tests and guard
interleaving tests become difficult to review.

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitBranchGeneration`

Suggested shape:

```text
CommitBranchGeneration(u64)
```

Rules:

1. generation is a storage fact used only to reject stale commit targets;
2. generation is not a product branch version;
3. `0` is reserved for "no generation fact supplied" if a sentinel is needed,
   or rejected outright if the type is nonzero;
4. exact equality is the only validation operation in L7E;
5. generation ordering is only used for registry recreate validation, not for
   commit visibility ordering.

### `CommitBranchGenerationGuard`

Suggested shape:

```text
enum CommitBranchGenerationGuard {
    NotSupplied,
    Exact(CommitBranchGeneration),
}
```

Rules:

1. V1 may admit `NotSupplied` because L9 owns branch-id reuse semantics.
2. When `Exact` is supplied, mismatch must reject before allocation.
3. Reuse-after-delete/recreate is just a generation mismatch when L9 supplies
   the old generation.
4. The guard must not infer generation from branch id, timestamps, or commit
   version.
5. The guard must not read L6 branch state.

This keeps the parent deferred map intact: L7 does not claim to own product
branch reuse, but it enforces generation facts when a caller provides them.

### `CommitBranchState`

Suggested shape:

```text
enum CommitBranchState {
    Active,
    Deleting,
    Deleted,
}
```

Rules:

1. `Active` branches can admit mutating commits if generation and quiesce pass.
2. `Deleting` rejects new mutating commits before allocation.
3. `Deleted` rejects new mutating commits before allocation.
4. L7E does not release L6 rows or table references when moving to deleting or
   deleted.
5. Recreating a deleted branch with the same generation is rejected.

The exact state names may be narrower if L7E only needs `Active` and
`Deleting`, but tests must still prove deleted or removed descriptors cannot be
silently accepted.

### `CommitBranchDescriptor`

Suggested shape:

```text
CommitBranchDescriptor {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    state: CommitBranchState,
}
```

Rules:

1. descriptor branch id must match its registry key;
2. descriptor generation must be valid;
3. descriptor state determines mutating admission;
4. descriptors contain no product branch name, dataset id, owner, remote, or
   policy metadata.

### `CommitBranchRegistry`

Suggested shape:

```text
CommitBranchRegistry {
    descriptors: BTreeMap<BranchId, CommitBranchDescriptor>,
}
```

Required operations:

1. register active branch;
2. lookup branch;
3. mark deleting;
4. mark deleted or remove descriptor by policy;
5. recreate/register with a higher caller-supplied generation when supported;
6. validate target branch and generation for a mutating commit;
7. produce branch admission facts for later pipeline stages.

Registry operations must not:

1. allocate commit versions;
2. allocate commit timestamps;
3. mutate L6;
4. append WAL;
5. publish visible versions;
6. inspect row payload bytes.

### `CommitBranchAdmission`

Suggested shape:

```text
CommitBranchAdmission {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    state: CommitBranchState,
}
```

Rules:

1. admission exists only after registry lookup, state validation, and
   generation validation pass;
2. admission is a fact, not a lock token;
3. later L7F/L7H/L7I code should use admission to avoid repeating branch
   availability checks;
4. admission must be invalidated by a later generation mismatch if the caller
   retries with stale facts.

### `CommitBranchGuardSet`

Suggested shape:

```text
CommitBranchGuardSet {
    active_branches: BTreeSet<BranchId>,
    quiesce: CommitQuiesceState,
}
```

Rules:

1. acquiring a mutating guard requires quiesce to be open;
2. acquiring a mutating guard for an already-guarded branch returns a typed
   rejection in L7E;
3. acquiring a mutating guard for another branch may succeed;
4. dropping the guard releases the branch;
5. release must happen on success and on validation/error paths;
6. guard tokens must not be cloneable;
7. guards do not allocate versions or timestamps.

L7L hardens the nonblocking rejection behavior and keeps the same lock order
and token ownership model. L8 owns retry and caller-level deadline policy.

### `CommitQuiesceState`

Suggested shape:

```text
enum CommitQuiesceState {
    Open,
    Quiescing,
}
```

Rules:

1. `try_begin_quiesce` succeeds only when no mutating branch guards are active;
2. successful quiesce returns a token that keeps new mutating commits blocked;
3. while quiesce is active, mutating guard acquisition rejects before
   allocation;
4. dropping the quiesce token reopens the guard set;
5. read-only diagnostics follow the documented L7E policy below.

L7E does not sleep, block a thread, or implement timeouts. L7L owns
deterministic guard/quiesce interleaving tests, while L8 owns retry and
caller-level deadline classification.

## Read-Only Policy During Quiesce

L7D read-only diagnostics do not mutate clocks, L6, WAL, timeline, or visible
facts. L7E should therefore allow read-only diagnostics during quiesce unless a
later slice proves a stricter recovery/checkpoint policy is required.

Rules:

1. mutating commits require branch guard admission;
2. read-only diagnostics do not acquire per-branch mutating guards;
3. read-only diagnostics may read the current visible tracker while quiesce is
   active;
4. checkpoint/recovery callers that need a stronger read barrier must use a
   later L7L/L8 API, not the L7D read-only diagnostic path.

## Admission Order

Mutating commit admission order:

```text
1. validate that batch is mutating and already L7B-validated
2. acquire quiesce read/admission check
3. acquire per-branch guard
4. validate branch registry descriptor exists
5. validate branch state is active
6. validate supplied generation, when present
7. return CommitBranchAdmission + guard token
```

This order preserves the parent lock order. The implementation may validate the
registry descriptor before acquiring the per-branch guard if doing so is needed
to avoid creating guard entries for missing branches, but tests must prove that
no version/timestamp allocation occurs before all branch checks pass.

## Error Mapping

Use existing commit-runtime vocabulary where possible. Extend only if needed.

Required classifications:

1. missing branch: `CommitRuntimeError::BranchUnavailable` or a dedicated
   branch-not-found variant;
2. branch deleting/deleted: `CommitRuntimeError::BranchUnavailable`;
3. generation mismatch: dedicated variant preferred, otherwise
   `CommitRuntimeError::BranchUnavailable` with stable reason text;
4. same-branch guard already active: `CommitRuntimeError::BranchUnavailable`;
5. quiesce active: `CommitRuntimeError::BranchUnavailable`;
6. quiesce cannot start because commits are active:
   `CommitRuntimeError::InvalidCommitState` or a dedicated quiesce variant;
7. duplicate branch registration: `CommitRuntimeError::InvalidCommitState` or
   a dedicated branch-already-exists variant.

Error display must stay storage-shaped and must not mention users, sessions,
transactions, datasets, remotes, or product branch commands.

## Implementation Steps

### L7E-A: Add Branch Registry Types

1. Add `branch_registry.rs`.
2. Add generation, generation guard, descriptor, state, registry, and admission
   types.
3. Add descriptor validation helpers.
4. Add registry register/lookup/mark-deleting/recreate helpers.
5. Export the crate-private surface from `commit/mod.rs`.

Exit gate: direct registry tests prove create, duplicate, lookup, missing,
deleting, deleted, and generation mismatch behavior.

### L7E-B: Add Guard And Quiesce Skeleton

1. Add `guard.rs`.
2. Add guard-set state and RAII branch guard token.
3. Add quiesce token.
4. Add nonblocking `try_acquire_branch_guard`.
5. Add nonblocking `try_begin_quiesce`.
6. Ensure `Drop` releases tokens exactly once.

Exit gate: direct guard tests prove same-branch serialization, different-branch
parallel admission, quiesce blocking, and release after error/drop.

### L7E-C: Wire Admission Helper

1. Add a helper that accepts a `ValidatedCommitBatch`,
   `CommitBranchGenerationGuard`, registry reference, and guard-set reference.
2. Reject read-only batches through mutating admission.
3. Reject missing/deleting/generation-mismatched branches before allocation.
4. Return `CommitBranchAdmission` and branch guard token on success.
5. Keep admission independent from L6, L4 WAL, visible tracker publication, and
   timeline rows.

Exit gate: spy tests prove allocator/timestamp source are not called on branch
admission rejection.

### L7E-D: Add Generated Testkit Contract

1. Add a small generated contract for registry and guard scripts.
2. Vary branch ids, generation facts, lifecycle operations, guard acquire/drop,
   quiesce start/drop, and read-only-vs-mutating admission.
3. Record counters for every meaningful acceptance/rejection bucket.

Exit gate: property harness asserts all buckets are hit.

### L7E-E: Update Source Guards

1. Extend the commit source guard if new files introduce new imports.
2. Confirm `commit/branch_registry.rs` and `commit/guard.rs` do not import
   backend, layout, filesystem, table internals, or engine/product crates.
3. Confirm the new surface remains `pub(crate)`.

Exit gate: source guard passes with the new files in scope.

## Invariants

1. Missing branches cannot allocate versions.
2. Deleting/deleted branches cannot allocate versions.
3. Supplied generation mismatches cannot allocate versions.
4. Read-only diagnostics do not acquire mutating guards.
5. Quiesce blocks new mutating guard acquisition.
6. A quiesce token cannot be created while mutating guards are active.
7. A mutating branch guard is released when its token is dropped.
8. Same-branch mutating commits cannot overlap in L7E.
9. Different-branch mutating commits can be admitted concurrently by L7E.
10. L7E does not claim durability, visibility, or conflict validation.

## Deferred To Later Slices

1. L7F: read-set and CAS conflict validation over L6 read views.
2. L7G: timeline row generation and lookup.
3. L7H: cache/no-WAL apply into L6 after admission.
4. L7I: WAL append after admission and before L6 apply.
5. L7J: durable-but-not-visible write gate and unresolved durable commit block.
6. L7K: replay and allocator catch-up.
7. L7L: nonblocking quiesce hardening and deterministic guard/quiesce
   interleaving coverage.
8. L8: checkpoint/recovery orchestration that uses quiesce.
9. L9: public branch lifecycle API and branch-generation ownership.

## Verification Commands

During implementation:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit --quiet
cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties --quiet
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard --quiet
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Do not add automated tests that only assert these planning documents exist or
link to each other. Automated tests should exercise branch registry behavior,
guard behavior, generated contracts, or source boundaries.
