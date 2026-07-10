# L7A Implementation Plan: Commit Runtime Scaffold

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-test-plan.md`

## Objective

Create the storage-next commit-runtime scaffold without implementing commit
batch validation, version allocation, WAL append, L6 apply, conflict
validation, timeline writes, or replay behavior.

L7A establishes:

1. the `crates/storage-next/src/commit/` module shape;
2. commit-runtime configuration, error, fact, and result shells;
3. crate-private exports for later L7 slices;
4. source-boundary guards for L7 ownership;
5. a generated-property harness stub for later commit-runtime slices;
6. the initial M4-L7 porting-log file and source map.

The slice should make L7B-L7N implementation easier without resurrecting
public transaction sessions, exposing storage transaction ids, importing engine
or product DTO vocabulary, or reaching around L6/L4 ownership boundaries.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/commit-timeline-substrate.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
7. `crates/storage-next/src/commit/mod.rs`
8. `crates/storage-next/src/branch/mod.rs`
9. `crates/storage-next/src/format/wal.rs`
10. `crates/storage-next/src/service/wal.rs`
11. `crates/storage/src/txn/context.rs`
12. `crates/storage/src/txn/manager.rs`
13. `crates/storage/src/txn/validation.rs`
14. `crates/storage/src/txn/lock_ordering.rs`
15. `crates/storage/src/durability/commit_adapter.rs`

## Existing-Code Source Map

| Current file | L7A evidence | L7A action |
|---|---|---|
| `crates/storage/src/txn/context.rs` | Names for staged writes, read facts, CAS facts, read-only behavior, and apply summaries. | Record vocabulary and deferrals only. `CommitBatch` and mutation behavior land in L7B. |
| `crates/storage/src/txn/manager.rs` | Version clock, branch commit locks, quiesce, visible-version tracking, pending versions, and no-WAL path. | Reserve config/error/fact vocabulary. No allocator, lock, or visibility behavior in L7A. |
| `crates/storage/src/txn/validation.rs` | Read-set and CAS conflict model. | Record as source evidence. Conflict validation lands in L7F. |
| `crates/storage/src/txn/lock_ordering.rs` | Existing lock-order discipline. | Reserve error/fact vocabulary and source-map notes. Lock-order implementation lands in L7E/L7L. |
| `crates/storage/src/durability/commit_adapter.rs` | WAL-before-storage protocol and ambiguous durability classification. | Reserve durable phase vocabulary only. WAL integration lands in L7I/L7J. |
| `crates/storage/src/durability/payload.rs` | Old payload construction. | Record retired behavior. L7 uses storage-next `WalRecord` construction later; L7A does not touch WAL bytes. |

## Scope

L7A implements scaffolding only:

1. commit module submodule declarations;
2. commit runtime config shell;
3. commit runtime error/result type;
4. commit phase, durability, visibility, replay, and guard fact shells;
5. commit runtime stats shell;
6. crate-private re-exports from `commit/mod.rs`;
7. source guard tests for production `commit/` boundaries;
8. small unit tests for construction, display, source chains, and field access;
9. testkit/property harness placeholders with nonzero scaffold counters;
10. initial `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
    entry.

L7A does not implement:

1. `CommitBatch` or `CommitMutation`;
2. duplicate-key policy;
3. storage row stamping;
4. commit-version allocation;
5. timestamp allocation;
6. branch registry or per-branch guards;
7. conflict validation;
8. timeline row construction or lookup;
9. cache/no-WAL commit apply;
10. WAL record construction or envelope append;
11. durable-but-not-visible write gates;
12. recovery replay;
13. quiesce behavior;
14. public storage API exposure.

## Module Layout

Target initial layout:

```text
crates/storage-next/src/commit/
  mod.rs
  config.rs
  error.rs
  facts.rs
  result.rs
  tests.rs
```

The exact split may change during implementation, but L7A should avoid a large
`mod.rs`. The scaffold should leave clear ownership for later slices:

1. `config.rs`: L7 limits and feature switches that are independent of a
   concrete commit batch.
2. `error.rs`: typed commit runtime errors and source-chain wrappers.
3. `facts.rs`: phase, durability, visibility, replay, guard, and stats facts.
4. `result.rs`: `CommitRuntimeResult<T>` alias and small result helpers.
5. `tests.rs`: module-local scaffold tests.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production types stay
`pub(crate)`.

### `CommitRuntimeConfig`

Suggested fields:

```text
CommitRuntimeConfig {
    max_mutations_per_batch: usize,
    max_validation_facts_per_batch: usize,
    max_commit_rows_per_batch: usize,
    read_only_diagnostics: CommitReadOnlyDiagnostics,
}
```

Rules:

1. defaults must be valid;
2. zero limits that would make mutating commits impossible are rejected;
3. read-only diagnostics are internal helpers, not a public transaction API;
4. config must not include backend paths, object layout, engine options,
   lifecycle scheduling, or product branch policy.

Use an explicit `CommitReadOnlyDiagnostics` enum instead of a boolean control
field.

### `CommitRuntimeError`

Initial variants should cover scaffold and later-slice routes without encoding
behavior prematurely:

```text
CommitRuntimeError::InvalidConfig { field }
CommitRuntimeError::InvalidCommitState { reason }
CommitRuntimeError::InvalidCommitPhase { reason }
CommitRuntimeError::InvalidVisibilityFacts { reason }
CommitRuntimeError::BranchUnavailable { reason }
CommitRuntimeError::DurabilityUnavailable { reason }
CommitRuntimeError::LowerLayer { layer, source }
```

Rules:

1. displays are bounded;
2. displays use storage terms, not product transaction claims;
3. displays do not include row value bytes or product DTO names;
4. wrapped L4/L6 source errors must be preserved through `Error::source()`;
5. transaction-id errors are not included in V1.

### `CommitPhase`

Suggested shape:

```text
CommitPhase::RejectedBeforeAllocation
CommitPhase::AllocatedNotDurable
CommitPhase::DurableNotApplied
CommitPhase::AppliedNotVisible
CommitPhase::Visible
CommitPhase::Replay
```

L7A only defines vocabulary. L7H-L7K assign phases to concrete commit
outcomes.

### `CommitDurabilityClass`

Suggested shape:

```text
CommitDurabilityClass::NotDurable
CommitDurabilityClass::Standard
CommitDurabilityClass::Always
CommitDurabilityClass::Uncertain
```

This is an outcome/fact vocabulary shell. L7A must not implement `standard` or
`always` WAL behavior.

### `CommitVisibilityFacts`

Suggested fields:

```text
CommitVisibilityFacts {
    allocated_version: Option<CommitVersion>,
    durable_version: Option<CommitVersion>,
    applied_version: Option<CommitVersion>,
    visible_version: Option<CommitVersion>,
    timeline_version: Option<CommitVersion>,
}
```

Rules:

1. absent facts are explicit for an empty runtime;
2. impossible ordering is rejected by construction or validation;
3. durable and visible remain distinguishable;
4. no storage transaction-id fact exists in V1.

### `CommitRuntimeStats`

Suggested fields:

```text
CommitRuntimeStats {
    committed_batches: u64,
    read_only_batches: u64,
    rejected_batches: u64,
    replayed_batches: u64,
    durable_but_not_visible: u64,
}
```

L7A can expose default/empty stats only. Counters move in later slices.

## Source Boundary Policy

Production `commit/` code may import:

1. `crate::branch` types and errors;
2. `crate::row` storage-row types;
3. `crate::format::wal` types for later durable slices;
4. `crate::service::wal` types for later durable slices;
5. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
6. standard library error/sync/type utilities.

Production `commit/` code must not import:

1. engine crates;
2. product DTOs or old `strata_core` product payload vocabulary;
3. `crate::table` internals directly;
4. `crate::backend` directly;
5. `crate::layout` or object-name builders directly;
6. lifecycle/recovery schedulers;
7. filesystem, path, mmap, environment, or process-global mutable state APIs.

Production `commit/` code must default to `pub(crate)`. L7A should not add any
crate-root export for `commit`.

## Testkit Scaffold

Create a small `commit_runtime` testkit namespace only behind test/testkit
configuration. It should expose a scaffold contract with counters for:

1. valid config construction;
2. invalid config rejection;
3. error display checks;
4. error source-chain checks;
5. phase/fact construction;
6. visibility fact validation;
7. stats default checks;
8. source guard fixture checks.

The scaffold contract must be narrow and mechanical. It must not encode commit
semantics that belong to L7B-L7N.

## Porting Log

Create `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
before behavior lands. The L7A entry must record:

1. current files read;
2. behavior preserved as vocabulary/source evidence;
3. behavior intentionally retired from V1, especially public transaction
   sessions and durable transaction ids;
4. behavior deferred by owner slice;
5. source guards and scaffold tests added;
6. sensitivity probes planned or run.

The entry must not claim commit batches, validation, allocation, WAL append,
L6 apply, timeline writes, or replay are implemented.

## Implementation Order

1. Create the L7 porting log with the L7A source map.
2. Split `commit/mod.rs` into the scaffold module layout.
3. Add config/error/fact/result shells with crate-private exports.
4. Add module-local scaffold tests.
5. Add source guard integration test and guard fixtures.
6. Add commit-runtime testkit scaffold contract and property route.
7. Run the L7A command matrix from the test plan.
8. Update the porting log with command evidence and any deferred behavior.

## Exit Gate

L7A is complete when:

1. the commit module has a clear crate-private scaffold;
2. config/error/fact/result shells compile under default, no-default, and
   all-feature builds;
3. source guards prevent product, engine, backend, layout, table-internal,
   filesystem, and public API leakage;
4. the generated scaffold route has nonzero counters for every category;
5. the porting log records source evidence, retired behavior, and deferrals;
6. no commit behavior has landed ahead of its owning L7 slice.
