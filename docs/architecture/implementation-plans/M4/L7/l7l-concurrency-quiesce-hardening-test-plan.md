# L7L Test Plan: Concurrency And Quiesce Hardening

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that L7 commit admission is single-process safe for normal mutating
commits and that L8 can rely on quiesce/guard behavior before it adds
checkpoint and recovery orchestration.

The suite must fail if L7L:

1. admits two mutating commits for the same branch;
2. rejects independent different-branch guards without cause;
3. starts quiesce while branch guards are active;
4. admits a mutating branch guard while quiesce is active;
5. forgets to release branch guards on success or failure;
6. allocates a commit version before branch admission;
7. validates conflicts outside the guarded single-branch window;
8. publishes visible version after the branch guard has been released;
9. allows normal writes while an unresolved durable commit is recorded;
10. allows a target branch with applied rows above global visible version to
    accept another normal commit;
11. introduces sleeps, async runtime requirements, or wall-clock waits into the
    crate-private commit path;
12. stores or prints user value bytes in guard/quiesce errors or debug output.

Do not add tests that only prove planning documents exist or link to each
other. L7L tests should exercise guard behavior, runtime ordering, failure
release, quiesce policy, source boundaries, and deterministic interleavings.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/guard.rs` for direct guard and quiesce
   primitive tests.
2. `crates/storage-next/src/commit/tests/branch_registry.rs` for branch
   admission and registry-state tests.
3. `crates/storage-next/src/commit/tests/conflict.rs` for guarded
   conflict-source comments and direct staleness cases.
4. `crates/storage-next/src/commit/tests/cache.rs` for cache runtime guard
   lifetime and visible-version safety tests.
5. `crates/storage-next/src/commit/tests/durable.rs` for durable runtime guard
   lifetime and unresolved durable gate tests.
6. `crates/storage-next/src/commit/tests/durable_gate.rs` for gate behavior
   that is independent of cache/durable executors.
7. `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs` for the
   deterministic guard/quiesce scheduler-style contract.
8. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   scaffold counter assertions.
9. `crates/storage-next/tests/commit_runtime_faults.rs` for commit fault
   boundaries that involve guard release.
10. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source and
    vocabulary boundaries.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. deterministic timestamps;
3. real `CommitBranchGuardSet`;
4. real `CommitBranchRegistry`;
5. real `VisibleVersionTracker`;
6. real `CommitUnresolvedDurableGate`;
7. real `CommitCacheRuntime` and `CommitDurableRuntime` for runtime-ordering
   tests;
8. narrow fake L6 apply/visible publishers only when a specific failure window
   must be injected;
9. opaque value bytes that are never asserted through error text;
10. no product branch names, dataset ids, remotes, graph/search/vector terms,
    public transaction-session types, or Hub vocabulary.

The deterministic scheduler-style harness must not spawn threads. It should
model interleavings by holding and dropping guard tokens in different orders.

## Direct Test Matrix

### 1. Guard Primitive Semantics

Required cases:

1. same-branch double acquisition rejects with `BranchGuardUnavailable`;
2. same-branch guard can be reacquired after the first guard drops;
3. different branches can hold guards concurrently;
4. `active_guard_count` reflects held and dropped branch guards;
5. debug output includes branch/guard facts but no user value bytes;
6. poisoned-lock cleanup path does not leave a permanently held branch guard if
   an existing poison test can be expressed safely;
7. cloned guard sets share one state.

Assertions:

1. guard release is RAII-based;
2. failure to acquire a guard does not mutate active-guard state;
3. different-branch guard independence is not implemented by sorting or
   blocking all branches.

### 2. Quiesce Primitive Semantics

Required cases:

1. quiesce starts when no branch guard is active;
2. quiesce rejects while any branch guard is active;
3. quiesce rejects while quiesce is already active;
4. mutating branch guard rejects while quiesce is active;
5. quiesce guard drop reopens mutating admission;
6. failed quiesce attempt does not leave the guard set quiescing;
7. read-only diagnostic during quiesce follows the documented policy and does
   not acquire a mutating guard;
8. quiesce does not allocate versions, write WAL, mutate L6, or publish
   visibility.

Assertions:

1. V1 quiesce is nonblocking and returns typed unavailable when active guards
   prevent immediate quiesce;
2. L8 owns retry/deadline behavior above L7;
3. no test should sleep waiting for quiesce.

### 3. Branch Registry Admission

Required cases:

1. missing branch rejects before guard acquisition;
2. deleting branch rejects before guard acquisition;
3. deleted branch rejects before guard acquisition;
4. generation mismatch rejects before guard acquisition;
5. duplicate branch registration rejects;
6. guard acquisition failure leaves descriptor state unchanged;
7. branch mark-deleting failure leaves descriptor state unchanged;
8. registry storage shape is either aligned with the parent plan or documented
   as a crate-private bounded Vec with uniqueness checks.

Assertions:

1. registry validation has no dependency on L6 row state;
2. admission failures do not allocate timestamps or commit versions;
3. registry errors do not expose value bytes.

### 4. Conflict Validation Window

Required cases:

1. conflict validation reads through the visible-version-capped L6 read view;
2. same-branch guard contention prevents a second commit from validating
   against a stale read view while the first guarded commit is in flight;
3. conflict rejection releases the branch guard;
4. blind writes remain accepted by the conflict validator;
5. stale read-set and stale CAS facts reject before allocation;
6. generated scaffold has a counter for the guarded conflict-window case.

Assertions:

1. the doc comment states that L7F is single-process safe through branch
   admission and is not a multi-process isolation primitive;
2. conflict validation is not moved after version allocation.

### 5. Cache Runtime Guard Lifetime

Required cases:

1. cache happy path holds branch guard through L6 apply and visible publication;
2. cache conflict failure releases branch guard;
3. cache allocator/visible mismatch rejection releases branch guard;
4. cache L6 apply failure releases branch guard;
5. cache visible publication failure releases branch guard and reports
   `AppliedButNotVisible`;
6. cache unresolved durable gate rejection happens before branch guard
   acquisition;
7. cache target-branch applied-above-visible rejection happens before
   allocation;
8. cache read-only diagnostic does not require a mutating branch guard.

Assertions:

1. after each failure, reacquiring the same branch guard succeeds;
2. visible version is unchanged for pre-visible failures;
3. applied-not-visible errors preserve phase facts.

### 6. Durable Runtime Guard Lifetime

Required cases:

1. durable happy path holds branch guard through WAL append, L6 apply, and
   visible publication;
2. WAL append failure releases branch guard;
3. L6 apply failure after WAL success records unresolved durable state and
   releases branch guard;
4. visible publication failure after WAL success records applied-not-visible
   state and releases branch guard;
5. unresolved durable gate rejection happens before branch guard acquisition;
6. durable target-branch applied-above-visible rejection happens before
   allocation;
7. segment-roll or segment-id-overflow failure is classified before L6 apply
   and releases guard if a direct fake can inject it.

Assertions:

1. after each failure, reacquiring the same branch guard succeeds unless an
   unresolved durable gate intentionally blocks normal mutation;
2. unresolved durable state blocks later cache and durable commits globally;
3. read-only diagnostics remain allowed with unresolved durable state.

### 7. Cross-Branch Visible-Version Safety

Required cases:

1. branch A and branch B can hold guards concurrently;
2. branch A failure does not retain branch B's guard;
3. branch A unresolved durable gate blocks normal commit on branch B;
4. externally seeded hidden rows on the target branch reject normal cache
   commit;
5. externally seeded hidden rows on the target branch reject normal durable
   commit;
6. executor tests document that registry-wide hidden-row scans are outside
   L7H/L7I because those runtimes receive only one target branch.

Assertions:

1. visible-version safety is target-branch local plus durable-gate global in
   V1;
2. invalid test setup without a durable gate is not treated as a normal
   cross-branch runtime state.

### 8. Replay Interaction

Required cases:

1. L7K replay does not acquire normal mutating branch guards;
2. replay documentation states that L8 should quiesce before replay when
   process-wide exclusion is required;
3. replay clears a matching unresolved durable gate before normal writes can
   resume;
4. replay failure leaves unresolved durable gate state intact.

Assertions:

1. replay remains distinct from normal conflict/admission path;
2. L7L does not add public recovery orchestration.

### 9. Deterministic Scheduler-Style Contract

Required operations:

```text
AcquireBranch(A)
AcquireBranch(B)
ReleaseBranch(A)
ReleaseBranch(B)
BeginQuiesce
ReleaseQuiesce
ReadOnlyDiagnostic
AssertOpen
AssertQuiescing
```

Required generated cases:

1. same-branch double acquisition fails;
2. different-branch acquisition succeeds;
3. quiesce after releasing all branch guards succeeds;
4. quiesce while branch guard is held fails;
5. branch acquisition while quiesce is held fails;
6. final cleanup leaves guard set open;
7. read-only diagnostic does not change guard state;
8. operation scripts never require sleeping, threads, async runtime, or wall
   clock.

Assertions:

1. `CommitRuntimeScaffoldOutcome` exposes counters for each L7L category;
2. `commit_runtime_properties.rs` asserts each counter is nonzero;
3. broad random commit/fault scripts remain an L7M responsibility.

### 10. Source Guards

Required checks:

1. `commit/` does not expose public transaction/session APIs;
2. `commit/` does not import product graph/search/vector/Hubble/Hub terms;
3. `commit/` does not import table internals except through L6-approved
   surfaces;
4. guard/quiesce code does not introduce `std::thread::sleep`;
5. guard/quiesce code does not introduce async runtime dependencies;
6. guard/quiesce code does not introduce process-global mutable state;
7. replay remains allowed to use durable WAL facts but not backend IO directly.

## Fault Windows

L7L direct or integration tests should cover these windows:

1. branch guard unavailable before allocation;
2. quiesce active before allocation;
3. conflict failure after guard acquisition but before allocation;
4. allocation/visible mismatch after guard acquisition but before L6 apply;
5. WAL append failure after allocation but before L6 apply;
6. L6 apply failure after WAL success;
7. visible publication failure after L6 apply;
8. unresolved durable gate rejection before guard acquisition;
9. target-branch applied-above-visible rejection before allocation.

For each window, assert:

1. branch guard state is not leaked;
2. visible version does not move unless the documented phase says it can;
3. unresolved durable gate changes only in the post-WAL durable windows;
4. error display/debug stays value-free.

## Sensitivity Probe Ledger

Record probe rows in the L7 porting log after implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| `L7L-S1` | Allow same-branch double guard acquisition. | Guard primitive same-branch test. |
| `L7L-S2` | Reject different-branch guard acquisition. | Guard primitive different-branch test. |
| `L7L-S3` | Allow quiesce while branch guard is active. | Quiesce active-guard test. |
| `L7L-S4` | Allow branch guard while quiesce is active. | Quiesce blocks mutating guard test. |
| `L7L-S5` | Do not clear quiesce flag on token drop. | Quiesce release/reacquire test. |
| `L7L-S6` | Drop branch guard before conflict validation completes. | Guarded conflict-window test. |
| `L7L-S7` | Allocate before branch admission. | Admission no-allocation test. |
| `L7L-S8` | Publish visible after branch guard release. | Cache/durable guard lifetime test. |
| `L7L-S9` | Ignore unresolved durable gate for cache commit. | Cross-branch durable-gate test. |
| `L7L-S10` | Ignore target-branch applied-above-visible rows. | Cache/durable applied-above-visible tests. |

## L7M Handoff

L7M should extend L7L with:

1. generated multi-branch commit scripts;
2. generated cache/durable/replay interleavings;
3. fuzz targets for commit-runtime operation scripts;
4. richer fault corpora for post-WAL and visibility windows;
5. a model that tracks branch guard state, visible version, unresolved durable
   gate, allocator floors, and branch rows.

L7L should leave enough counters and helper functions for L7M to reuse without
retesting only fixed examples.

## Verification Commands

Minimum commands after implementing L7L:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit::tests::guard
cargo test -p strata-storage-next --locked --lib commit::tests::branch_registry
cargo test -p strata-storage-next --locked --lib commit::tests::conflict
cargo test -p strata-storage-next --locked --lib commit::tests::cache
cargo test -p strata-storage-next --locked --lib commit::tests::durable
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --no-default-features --locked --lib commit
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Optional before L7 closeout if `cargo-hack` is installed:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

No nightly fuzz command is required for L7L. Fuzz target creation belongs to
L7M.

## Exit Criteria

L7L is complete when:

1. direct guard/quiesce tests cover all primitive cases;
2. runtime tests cover guard release after cache and durable failures;
3. conflict validation window is documented and tested;
4. cross-branch visible-version policy is documented and tested;
5. deterministic scheduler-style guard contract is wired into the generated
   scaffold;
6. source guards pass;
7. no wall-clock waiting dependency is introduced;
8. no public transaction/session surface is exposed;
9. the porting log records the nonblocking quiesce decision and sensitivity
   probes.
