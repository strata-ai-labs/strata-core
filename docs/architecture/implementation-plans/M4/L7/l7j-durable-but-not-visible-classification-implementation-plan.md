# L7J Implementation Plan: Durable-But-Not-Visible Classification

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-test-plan.md`

## Objective

Implement the in-process safety layer for durable commits that have crossed the
WAL durability boundary but have not reached normal L6 visibility.

L7I proved the normal durable path:

```text
validate/admit/conflict -> allocate/stamp -> WAL append -> L6 apply -> visible publish
```

L7J handles the two windows after WAL append succeeds:

1. WAL durable, L6 apply failed;
2. WAL durable, L6 apply succeeded, visible publication failed.

Both states must be explicit, typed, and must block later mutating commits until
L7K/L8 repair or replay the durable fact. L7J does not replay WAL records and
does not decide recovery policy.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md`
9. `crates/storage-next/src/commit/durable.rs`
10. `crates/storage-next/src/commit/cache.rs`
11. `crates/storage-next/src/commit/outcome.rs`
12. `crates/storage-next/src/commit/visibility.rs`
13. `crates/storage-next/src/branch/state.rs`
14. `crates/storage/src/durability/commit_adapter.rs`
15. `crates/storage/src/txn/manager.rs`

## Existing-Code Source Map

| Current file | L7J evidence | L7J action |
|---|---|---|
| `crates/storage/src/durability/commit_adapter.rs` | Old storage distinguished ambiguous durable windows from clean pre-durable failures. | Port the classification idea only. Do not port old WAL bytes or public transaction vocabulary. |
| `crates/storage/src/txn/manager.rs` | Old manager halted forward progress when writer health or durable state was unresolved. | Port the storage safety rule: unresolved durable state blocks later writes. Keep product observer hooks above L7. |
| `crates/storage-next/src/commit/durable.rs` | L7I already maps post-WAL L6/visibility failures to `DurableButNotVisible` errors but has no persistent in-process gate. | Add durable-unresolved recording and gate checks around the existing durable executor. |
| `crates/storage-next/src/commit/cache.rs` | Cache commits can currently continue even if a durable commit is unresolved elsewhere. | Add the same normal-write gate check to cache/no-WAL commits. |
| `crates/storage-next/src/commit/outcome.rs` | `CommitOutcomeKind::DurableButNotVisible` and phase validation already exist. | Reuse these facts for recorded unresolved durable state. Do not invent a second phase vocabulary. |
| `crates/storage-next/src/commit/visibility.rs` | Visible publication can fail with typed visibility errors. | Make visible failure after WAL durable success record `AppliedNotVisible`, not generic lower-layer failure. |
| `crates/storage-next/src/branch/state.rs` | `append_committed_rows_atomically` is all-or-nothing for L6 apply. | Apply-failure gate facts may assume no partial rows were installed. |

## Scope

L7J implements:

1. an in-process unresolved durable commit fact;
2. a normal-write gate that blocks cache and durable mutating commits while the
   fact is unresolved;
3. explicit post-WAL classification for L6 apply failure;
4. explicit post-WAL classification for visible publication failure;
5. enough recovery handoff metadata for L7K/L8 to identify the branch, version,
   timestamp, phase, durability class, and visible progress facts;
6. exact-idempotent recording of the same unresolved fact;
7. fail-closed behavior when a different unresolved fact is already present;
8. narrow injectable L6 apply and visibility-publish adapters for L7J tests;
9. direct tests for both post-WAL failure windows;
10. generated counters for durable-but-not-visible gates;
11. source guard updates if new commit modules are added.

L7J does not implement:

1. WAL replay;
2. allocator catch-up from replay;
3. process-open recovery;
4. retained durable row reconstruction;
5. clearing the gate based on actual replay; L7K/L8 will own the real repair
   transition;
6. manifest/checkpoint interaction;
7. branch deletion or clear semantics;
8. public transaction/session APIs;
9. product observers or post-commit hooks;
10. remote/Hub behavior.

## Proposed Type Surface

Keep the surface crate-private.

```text
CommitUnresolvedDurableKind {
  DurableNotApplied,
  AppliedNotVisible,
}

CommitUnresolvedDurable {
  branch_id: BranchId,
  commit_version: CommitVersion,
  commit_timestamp: Timestamp,
  durability: CommitDurabilityClass,
  kind: CommitUnresolvedDurableKind,
  visibility_facts: CommitVisibilityFacts,
  reason: &'static str,
}

CommitUnresolvedDurableGate {
  unresolved: Option<CommitUnresolvedDurable>,
}
```

Required operations:

```text
CommitUnresolvedDurable::new(...)
CommitUnresolvedDurable::validate()
CommitUnresolvedDurableGate::new()
CommitUnresolvedDurableGate::unresolved()
CommitUnresolvedDurableGate::require_open_for_mutation()
CommitUnresolvedDurableGate::record_unresolved(fact)
CommitUnresolvedDurableGate::clear_exact(fact)       # hook only; L7K/L8 uses it later
```

The gate may use interior mutability if that avoids making every commit runtime
constructor take a mutable registry. It must not be process-global.

## Failure Vocabulary

Existing L7I errors:

1. `CommitRuntimeError::DurabilityUncertain`: WAL may or may not be durable.
2. `CommitRuntimeError::DurableButNotVisible`: WAL is durable; visibility is
   unresolved.

L7J adds or finalizes:

1. `DurableButNotVisible` error source chains are preserved.
2. A new typed blocked-write error, for example:

```text
CommitRuntimeError::UnresolvedDurableCommit {
  branch_id,
  commit_version,
  reason,
}
```

If the existing error enum can represent this without ambiguity, reuse it only
when the display text still clearly says a later mutating commit was blocked by
an unresolved durable commit.

## Protocol Changes

### Normal Mutating Commit Admission

Both cache and durable mutating runtimes must check the gate before allocating
or mutating:

```text
validate batch shape
reject unsupported durability mode
check unresolved durable gate
admit target branch and acquire branch guard
...
```

The gate should block all mutating commits, not only the affected branch. L7's
visible-version and commit-version facts are global enough that publishing a
later commit while an earlier durable version is unresolved would make replay
ordering harder and unsafe for V1.

Read-only diagnostic paths may remain allowed because they do not allocate,
write WAL, apply L6 rows, or publish visibility.

### WAL Durable, L6 Apply Failed

After L4 append succeeds and before L6 apply:

```text
append WAL success
L6 apply returns error
build CommitUnresolvedDurable {
  kind: DurableNotApplied,
  visibility_facts: allocated=version, durable=version, applied=None,
                    timeline=None, visible=None
}
record gate
return CommitRuntimeError::DurableButNotVisible
```

The L6 atomic apply boundary guarantees no partial rows are visible for this
failure class.

### WAL Durable, L6 Apply Succeeded, Visible Publish Failed

After L6 apply succeeds and before visible publication:

```text
append WAL success
L6 apply success
visible publication returns error
build CommitUnresolvedDurable {
  kind: AppliedNotVisible,
  visibility_facts: allocated=version, durable=version, applied=version,
                    timeline=version, visible=None
}
record gate
return CommitRuntimeError::DurableButNotVisible
```

The branch read view may contain rows above the visible version. The gate is
what prevents later normal commits from observing that as safe forward progress.

### Gate Recording Rules

1. Recording an empty or invalid fact fails closed.
2. Recording the exact same fact twice is idempotent.
3. Recording a different fact while one is already present fails closed.
4. Clearing requires an exact fact match.
5. A gate fact must never claim `NotDurable`.
6. A `DurableNotApplied` fact must not carry applied/timeline/visible versions.
7. An `AppliedNotVisible` fact must carry applied and timeline versions but not
   visible version.
8. The gate must not clone or retain user value bytes. It stores commit facts,
   phase facts, and reason strings only.

## Adapter Refactor

L7I currently depends directly on concrete `BranchLocalState` and
`VisibleVersionTracker`, which makes post-WAL fault injection awkward. L7J
should introduce narrow adapters, similar to `CommitWalAppender`:

```text
trait CommitBranchApplyTarget {
  fn branch_id(&self) -> BranchId;
  fn max_commit_version(&self) -> Option<CommitVersion>;
  fn capture_read_view(&self) -> CommitRuntimeResult<BranchReadView>;
  fn append_committed_rows_atomically(rows) -> CommitRuntimeResult<...>;
}

trait CommitVisiblePublisher {
  fn visible_version(&self) -> CommitVersion;
  fn publish_from_facts(facts) -> CommitRuntimeResult<...>;
}
```

Production impls wrap `BranchLocalState` and `VisibleVersionTracker`. Tests can
inject failure after WAL success without corrupting real L6 state.

If genericizing both cache and durable runtimes is too large for one patch, do
the durable runtime first and add a smaller gate-only check to cache runtime.
The exit gate still requires cache commits to be blocked by an unresolved
durable fact.

## Module Layout

Expected production layout after L7J:

```text
crates/storage-next/src/commit/
  cache.rs
  durable.rs
  durable_gate.rs     # unresolved durable facts and normal-write gate
  tests/
    durable.rs
    durable_gate.rs
```

If the adapter traits become large, use:

```text
  apply.rs            # L7-to-L6 apply/visible adapter traits
```

Only add that file if it keeps `durable.rs` below the review budget.

## Implementation Steps

### L7J-A: Gate Facts

1. Add `CommitUnresolvedDurableKind`.
2. Add `CommitUnresolvedDurable`.
3. Add validation for phase/fact consistency.
4. Add bounded display/debug output with no value bytes.
5. Add direct tests for valid and invalid facts.

Exit gate: unresolved durable facts are representable without claiming
visibility.

### L7J-B: Gate State

1. Add `CommitUnresolvedDurableGate`.
2. Implement `require_open_for_mutation`.
3. Implement idempotent `record_unresolved`.
4. Implement exact-match clear hook for L7K/L8.
5. Add typed blocked-write error.

Exit gate: the gate blocks all mutating commits and preserves the unresolved
fact for inspection.

### L7J-C: Apply/Visible Injection Boundary

1. Introduce narrow L6 apply and visibility-publish adapters.
2. Preserve existing production behavior for `BranchLocalState` and
   `VisibleVersionTracker`.
3. Keep source guards tight: no backend/layout/object imports.
4. Avoid changing L6 behavior.

Exit gate: tests can inject L6 apply failure and visible publish failure after
WAL success.

### L7J-D: Durable Runtime Classification

1. On post-WAL L6 apply failure, record `DurableNotApplied`.
2. On post-WAL visible publish failure, record `AppliedNotVisible`.
3. Return `DurableButNotVisible` error with branch/version/reason/source.
4. Ensure branch guard releases after gate recording.
5. Ensure no later writes can proceed while the gate is set.

Exit gate: both post-WAL failure windows are typed, recorded, and write-gated.

### L7J-E: Cache Runtime Gate Check

1. Add the unresolved durable gate check before cache-mode allocation.
2. Preserve all L7H cache tests.
3. Add a cache-specific blocked-write test.

Exit gate: unresolved durable state blocks both durable and cache mutating
commits.

### L7J-F: Generated Harness

1. Add durable-gate counters to the commit-runtime testkit.
2. Exercise successful gate recording for both post-WAL windows.
3. Exercise blocked durable and cache commits after gate recording.
4. Exercise exact-idempotent recording and different-fact rejection.

Exit gate: generated commit-runtime properties include durable-but-not-visible
coverage.

## Sensitivity Probes

The L7J suite should fail if:

1. L6 apply failure after WAL success is reported as clean WAL failure;
2. visible publish failure after WAL success is reported as visible success;
3. the gate is not recorded before returning the post-WAL error;
4. a later cache commit bypasses the gate;
5. a later durable commit bypasses the gate;
6. a different unresolved durable fact overwrites the first fact;
7. `DurableNotApplied` facts claim applied rows;
8. `AppliedNotVisible` facts omit applied/timeline versions;
9. the gate stores user value bytes;
10. read-only diagnostics are unnecessarily blocked.

Record any probes that are run in
`docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`.

## Verification Commands

Minimum commands for this slice:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults
cargo test -p strata-storage-next --no-default-features --locked --lib commit
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Run the L7I durable tests before and after the gate refactor to prove the
successful durable path did not regress:

```bash
cargo test -p strata-storage-next --all-features --locked --lib commit::tests::durable
```

## Exit Criteria

L7J is complete when:

1. post-WAL L6 apply failure records `DurableNotApplied`;
2. post-WAL visible publish failure records `AppliedNotVisible`;
3. both return typed durable-but-not-visible errors;
4. both preserve lower-layer source chains;
5. all later mutating commits are blocked while the gate is set;
6. read-only diagnostics remain allowed;
7. exact same unresolved fact can be recorded idempotently;
8. different unresolved facts fail closed;
9. generated tests exercise both durable-but-not-visible windows;
10. L7I successful durable path still passes unchanged.
