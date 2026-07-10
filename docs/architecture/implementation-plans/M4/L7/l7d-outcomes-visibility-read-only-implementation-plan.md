# L7D Implementation Plan: Outcomes, Visibility, And Read-Only Path

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-test-plan.md`

## Objective

Implement the commit-runtime outcome and visibility-fact layer for L7-Core.

L7D takes the validated batch and fact-allocation types from L7B/L7C and adds
the storage-shaped facts that later slices return after cache, durable, and
replay commit paths. It also implements the read-only diagnostic path: a
validated read-only batch returns a coherent visible-version snapshot without
allocating a commit version, reading a timestamp source, mutating L6, appending
WAL, or writing timeline rows.

This slice should make one thing true: L7 can represent what happened to a
commit or read-only diagnostic request, and can publish/query visible-version
facts in a monotonic way, without yet performing a mutating commit.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
7. `crates/storage-next/src/commit/`
8. `crates/storage-next/src/commit/batch.rs`
9. `crates/storage-next/src/commit/facts.rs`
10. `crates/storage-next/src/commit/allocator.rs`
11. `crates/storage/src/txn/manager.rs`
12. `crates/storage/src/txn/context.rs`
13. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L7D evidence | L7D action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Old transaction manager separated allocated version and visible version, exposed `set_visible_version`, and read-only paths did not allocate storage versions. | Port the visible-version separation, but keep it storage-shaped and crate-private. Do not port transaction IDs or public transaction sessions. |
| `crates/storage/src/txn/context.rs` | Old read-only transaction mode prevented writes and avoided read-set mutation in diagnostic paths. | Port only the no-allocation/no-mutation read-only diagnostic fact path. Do not port public transaction context APIs. |
| `crates/storage/src/segmented/tests/batch.rs` | Historical bug coverage around atomic visibility and partial-state exposure. | Reserve the outcome/fact shape needed by later cache/durable slices; L7D itself does not apply rows. |
| `crates/storage-next/src/commit/facts.rs` | `CommitVisibilityFacts`, `CommitPhase`, `CommitDurabilityClass`, and `CommitRuntimeStats` exist as L7A shells. | Extend or reuse these facts for outcome/read-snapshot constructors and visible-version tracker validation. |
| `crates/storage-next/src/commit/batch.rs` | `CommitBatchKind::ReadOnlyDiagnostic` and validated read-only batches exist. | Execute read-only diagnostics by returning snapshot facts, not by allocating a stamp. |
| `crates/storage-next/src/commit/allocator.rs` | L7C proves read-only allocation returns a no-allocation allocation outcome. | L7D read-only execution should not call the allocator at all. |

## Scope

L7D implements:

1. `CommitOutcome` storage-shaped result facts;
2. `CommitOutcomeKind` or equivalent classification for read-only, visible
   mutating, not-visible, durable-but-not-visible, and replay outcomes;
3. `CommitMutationCounts` or equivalent put/delete/timeline count facts;
4. `CommitReadSnapshot` carrying the current visible version for diagnostics;
5. `VisibleVersionTracker` for monotonic visible-version publication;
6. constructors that validate impossible outcome/fact combinations;
7. read-only diagnostic execution over a `ValidatedCommitBatch`;
8. config handling for `CommitReadOnlyDiagnostics::{Enabled, Disabled}`;
9. generated testkit counters for read-only and visibility facts;
10. source-guard coverage that L7D does not import L6, WAL, backend, layout,
    table internals, filesystem, or product transaction APIs.

L7D does not implement:

1. mutating cache commits;
2. branch registry, branch generation, branch deletion, quiesce, or commit
   locks;
3. conflict validation against L6 read views;
4. timeline row construction or lookup;
5. WAL record construction or append;
6. durable-but-not-visible recovery gates beyond outcome vocabulary;
7. replay of durable rows;
8. public storage API methods.

## Module Layout

Expected production layout after L7D:

```text
crates/storage-next/src/commit/
  allocator.rs
  batch.rs
  config.rs
  error.rs
  facts.rs
  outcome.rs          # commit outcome, mutation counts, read snapshot
  visibility.rs       # visible-version tracker
  result.rs
  tests/
    allocator.rs
    batch.rs
    outcome.rs
    scaffold.rs
    visibility.rs
```

If the implementation remains small, `outcome.rs` and `visibility.rs` may be
one module. Split them before either file becomes difficult to review.

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitReadSnapshot`

Suggested shape:

```text
CommitReadSnapshot {
    branch_id: BranchId,
    visible_version: CommitVersion
}
```

Rules:

1. A read snapshot is a diagnostic fact, not a durable commit.
2. The visible version is the maximum version L7 currently allows new read
   snapshots to target.
3. `CommitVersion::ZERO` is a valid empty-state snapshot.
4. The snapshot does not prove timestamp retention coverage; L6 timestamp
   coverage remains separate.
5. The snapshot does not pin retention by itself. L8/L9 retention integration
   owns pin accounting.

### `VisibleVersionTracker`

Suggested shape:

```text
VisibleVersionTracker {
    visible_version: CommitVersion
}
```

Rules:

1. The default visible version is `CommitVersion::ZERO`.
2. The tracker is monotonic: publishing a lower version is either a no-op or a
   typed invalid state error. Pick one behavior and test it explicitly.
3. Publishing the same version is idempotent.
4. Publishing a greater version advances the tracker.
5. `catch_up_visible_after_replay(version)` is allowed only as a local fact
   update after L8 has installed recovered rows into L6.
6. The tracker never allocates versions. L7C remains the only allocator owner.
7. The tracker never reads timestamps.
8. The tracker never mutates L6.

### Cross-Branch Visibility Policy

L7D should use a single global visible-version tracker for V1, because commit
versions are globally ordered.

The global tracker may only be advanced by later mutating/replay slices after
the relevant rows are fully installed. L7D does not prove cross-branch atomic
apply; it only provides the monotonic fact container. Later slices must not
publish a global visible version if doing so could expose lower-version rows
that were applied but intentionally not visible.

In V1, cache and durable executors receive only the target `BranchLocalState`.
They therefore perform a target-branch `max_commit_version <= visible_version`
preflight and rely on the process-global unresolved durable gate plus L8
recovery ownership to exclude cross-branch applied-not-visible durable rows.
Externally seeding a non-target branch with rows above the global visible
version without also installing the corresponding unresolved/recovery fact is
invalid runtime construction, not a commit executor responsibility.

This keeps the policy explicit:

1. `allocated_version` can be ahead of `visible_version`;
2. `durable_version` can be ahead of `visible_version`;
3. `applied_version` can be ahead of `visible_version` only when the outcome
   reports not-visible or durable-but-not-visible;
4. new read snapshots target only `visible_version`;
5. branch-local read APIs still enforce branch isolation in L6.

If a later architecture review chooses per-branch visible versions, that is a
separate change. L7D should document the global V1 choice in code comments and
tests.

### `CommitMutationCounts`

Suggested shape:

```text
CommitMutationCounts {
    puts: u32,
    deletes: u32,
    timeline_rows: u32
}
```

Rules:

1. read-only outcomes have all counts equal to zero;
2. mutating outcomes count caller put/delete mutations after L7B validation;
3. timeline rows stay zero until L7G;
4. counters must not overflow silently; use `usize` internally if that matches
   L7B limits better, but expose bounded storage facts.

### `CommitOutcome`

Suggested shape:

```text
CommitOutcome {
    branch_id: BranchId,
    kind: CommitOutcomeKind,
    phase: CommitPhase,
    durability: CommitDurabilityClass,
    commit_version: Option<CommitVersion>,
    commit_timestamp: Option<Timestamp>,
    mutation_counts: CommitMutationCounts,
    visibility_facts: CommitVisibilityFacts,
    read_snapshot: Option<CommitReadSnapshot>
}
```

Rules:

1. read-only outcome:
   - no commit version;
   - no commit timestamp;
   - `durability = NotDurable`;
   - `phase = RejectedBeforeAllocation` or a dedicated read-only phase if one
     is added;
   - zero mutation counts;
   - read snapshot present;
   - empty visibility facts except current visible snapshot fact if the type
     stores it separately.
2. visible mutating outcome:
   - commit version and timestamp present;
   - visible fact equals the commit version;
   - applied fact is at least the visible fact;
   - timeline fact is at least the visible fact once L7G is in use.
3. durable-but-not-visible outcome:
   - commit version and timestamp present;
   - durability is `Standard`, `Always`, or `Uncertain`;
   - visible fact is absent or below the commit version;
   - later L7J owns write-gate behavior.
4. invalid combinations are rejected at construction time.
5. outcome display/debug output must not dump values or product payloads.

### Read-Only Diagnostic Execution

Suggested operation:

```text
execute_read_only(batch, config, visible_tracker) -> CommitOutcome
```

Rules:

1. the batch must already be `ValidatedCommitBatch`;
2. the batch kind must be `ReadOnlyDiagnostic`;
3. if `CommitReadOnlyDiagnostics::Disabled`, return a typed error before any
   fact mutation;
4. return a `CommitReadSnapshot` using the tracker's current visible version;
5. ignore the batch timestamp policy because no commit timestamp is allocated;
6. do not call `CommitFactAllocator`;
7. do not call L6, WAL, timeline, backend, or layout code;
8. do not claim crash durability even if the batch options request
   `CommitDurabilityMode::Always`;
9. do not mutate runtime stats unless L7D also adds an explicit stats recorder.

## Implementation Steps

### L7D-A: Outcome Type Surface

1. Add `outcome.rs`.
2. Define `CommitReadSnapshot`, `CommitMutationCounts`, `CommitOutcomeKind`,
   and `CommitOutcome`.
3. Add crate-private exports from `commit/mod.rs`.
4. Add constructors for read-only outcome and future mutating outcome shapes.
5. Keep WAL/object diagnostic fields out until L7I.

### L7D-B: Visibility Tracker

1. Add `visibility.rs` or a visibility section in `outcome.rs`.
2. Define `VisibleVersionTracker`.
3. Implement default zero state.
4. Implement monotonic publish and replay catch-up helpers.
5. Ensure publish helpers validate `CommitVisibilityFacts`.
6. Keep the tracker independent of L6 and WAL.

### L7D-C: Read-Only Diagnostic Path

1. Add a function or small executor type for validated read-only batches.
2. Enforce `CommitReadOnlyDiagnostics`.
3. Return a read-only `CommitOutcome`.
4. Prove no allocator/timestamp source is needed by the function signature.
5. Reject mutating batches with `InvalidBatch` or `InvalidCommitPhase`.

### L7D-D: Generated Testkit Coverage

1. Extend `crates/storage-next/src/testkit/commit_runtime.rs` or split a
   `commit_runtime_outcome.rs`.
2. Add generated checks for read-only outcome facts, disabled read-only
   rejection, visible tracker monotonicity, and impossible visibility facts.
3. Extend `commit_runtime_properties.rs` counters.

### L7D-E: Source Guard And Porting Log

1. Extend source guards only if new vocabulary creates a boundary hole.
2. Record old-code behavior preserved/retired in `m4-l7-porting-log.md` during
   implementation.
3. Do not add tests that merely assert planning documents exist.

## Error Behavior

L7D may add or reuse errors for:

1. read-only diagnostics disabled;
2. read-only executor given a mutating batch;
3. impossible outcome facts;
4. visible version publication before applied/timeline facts permit it;
5. visible version regression if the implementation chooses typed rejection
   rather than no-op.

Do not use backend, WAL, table, engine, or product error vocabulary in L7D
errors.

## Sensitivity Probes To Record During Implementation

When implementing L7D, record probe results in the L7 porting log:

1. read-only diagnostic allocates a version; direct/generated read-only tests
   fail;
2. read-only diagnostic reads timestamp source; failing-source read-only test
   fails;
3. disabled read-only diagnostics are accepted; config test fails;
4. visible tracker regresses; monotonic visibility test fails;
5. visible tracker publishes from allocated facts without applied facts;
   impossible-facts test fails;
6. outcome reports visible when visibility facts are absent; outcome
   constructor test fails;
7. read-only outcome reports durability; outcome test fails;
8. mutating batch enters read-only executor; phase test fails;
9. import L6, WAL, backend, layout, table internals, filesystem, or engine code
   into outcome/visibility modules; source guard fails.

## Exit Gate

L7D is complete when:

1. `CommitOutcome` can represent read-only, visible, not-visible, and
   durable-but-not-visible facts without product vocabulary;
2. impossible outcome and visibility fact combinations are rejected;
3. visible-version tracker starts at zero and moves monotonically;
4. read-only diagnostics return snapshot facts without allocation;
5. disabled read-only diagnostics reject before allocation;
6. generated testkit coverage exercises every L7D counter category;
7. source guards remain green;
8. no L6, WAL, timeline, branch guard, backend, layout, or engine code is used;
9. the focused verification commands in the L7D test plan pass.
