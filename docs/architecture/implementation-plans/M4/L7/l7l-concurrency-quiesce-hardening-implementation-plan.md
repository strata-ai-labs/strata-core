# L7L Implementation Plan: Concurrency And Quiesce Hardening

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-test-plan.md`

## Objective

Harden the in-process commit admission, guard lifetime, quiesce, and
visible-version ordering rules that L8 will rely on for checkpoint, recovery,
and maintenance gates.

L7A through L7K delivered the functional commit paths:

```text
cache commit:   admit -> conflict -> allocate -> L6 apply -> visible publish
durable commit: admit -> conflict -> allocate -> WAL append -> L6 apply -> visible publish
replay:         decoded durable row set -> L6 install/confirm -> visible publish
```

L7L makes the concurrency contract around those paths explicit and hard to
regress. It is not a generated fuzz closeout slice. L7M owns broad generated
operation scripts, fuzz targets, and expanded fault corpora.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md`
12. `crates/storage-next/src/commit/guard.rs`
13. `crates/storage-next/src/commit/branch_registry.rs`
14. `crates/storage-next/src/commit/conflict.rs`
15. `crates/storage-next/src/commit/cache.rs`
16. `crates/storage-next/src/commit/durable.rs`
17. `crates/storage-next/src/commit/durable_gate.rs`
18. `crates/storage-next/src/commit/replay.rs`
19. `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs`
20. `crates/storage-next/src/testkit/commit_runtime_cache.rs`
21. `crates/storage-next/src/testkit/commit_runtime_durable.rs`
22. `crates/storage/src/txn/manager.rs`
23. `crates/storage/src/txn/lock_ordering.rs`

## Existing-Code Source Map

| Current file | L7L evidence | L7L action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Old storage serialized branch commits, tracked active writers, quiesced writes for maintenance, and advanced global visible state only after apply. | Port the safety rules only. Do not reintroduce public transaction sessions, transaction ids, product timeouts, or observer hooks. |
| `crates/storage/src/txn/lock_ordering.rs` | Old code made lock acquisition order reviewable. | Encode the new storage-next order in comments, tests, and helper boundaries rather than adding a large lock manager. |
| `crates/storage-next/src/commit/guard.rs` | L7E added `CommitBranchGuardSet`, branch guard RAII, and quiesce token state. | Document and harden the V1 nonblocking quiesce contract, poison behavior, release-on-drop, and same/different-branch behavior. |
| `crates/storage-next/src/commit/branch_registry.rs` | Admission validates branch lifecycle/generation and then acquires a guard. | Pin admission order and ensure guard acquisition failure leaves registry state unchanged. |
| `crates/storage-next/src/commit/conflict.rs` | Conflict validation reads through a visible-version-capped L6 read view. | Document the single-process serialization boundary: same-branch staleness windows are closed by the branch guard, not by multi-process isolation. |
| `crates/storage-next/src/commit/cache.rs` | Cache commits hold admission guard through L6 apply and visible publication. | Add direct evidence for guard release after every failure phase and for target-branch applied-above-visible rejection. |
| `crates/storage-next/src/commit/durable.rs` | Durable commits hold admission guard through WAL append, L6 apply, gate recording, and visible publication. | Add direct evidence for guard release after WAL/apply/visible failures and for unresolved durable gate ordering. |
| `crates/storage-next/src/commit/durable_gate.rs` | L7J blocks normal writes while a durable commit is unresolved. | Treat this as the V1 cross-branch global visible-version safety barrier. |
| `crates/storage-next/src/commit/replay.rs` | Replay bypasses normal admission because L8 supplies already-durable rows. | Document that replay callers should run under an L8 quiesce/recovery gate when process-wide exclusion is needed. |
| `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs` | Existing generated scaffold already exercises several guard cases. | Extend with deterministic interleavings that are still direct enough for L7L; leave broad generated scripts to L7M. |

## Scope

L7L implements:

1. explicit V1 guard/quiesce semantics in code comments and tests;
2. same-branch mutating commit serialization;
3. different-branch guard independence;
4. release-on-drop for branch guards and quiesce guards;
5. quiesce token behavior that blocks new mutating branch guards;
6. typed fast-fail when quiesce cannot begin because guards are active;
7. typed fast-fail when a mutating guard is requested while quiesce is active;
8. read-only diagnostic policy during quiesce;
9. guard-acquisition failure leaves registry descriptors unchanged;
10. guard release after cache conflict, allocation, L6 apply, and visible
    publication failures;
11. guard release after durable WAL, L6 apply, gate-recording, and visible
    publication failures;
12. conflict-validation staleness-window documentation for the single-process
    branch guard model;
13. target-branch applied-above-visible rejection in cache and durable paths;
14. unresolved durable gate as the V1 cross-branch safety barrier for durable
    applied-not-visible rows;
15. deterministic scheduler-style tests over guard/quiesce operations without
    introducing real sleeps or async runtime dependencies;
16. generated scaffold counters for the new L7L categories that L7M can expand.

L7L does not implement:

1. a public transaction manager;
2. blocking condition-variable waits inside L7;
3. wall-clock timeout scheduling inside L7;
4. async runtime integration;
5. process-wide or multi-process distributed commit locks;
6. L1 writer-lock interaction;
7. L8 checkpoint/recovery orchestration;
8. WAL scanning, replay ordering, or recovery health;
9. branch clear/delete public APIs;
10. product branch merge/fork commands;
11. generated fuzz targets or seed corpora, which belong to L7M;
12. closeout inventory tests, which belong to L7N.

## V1 Quiesce Decision

The old storage architecture had quiesce behavior that could wait for active
transactions. Storage-next L7 deliberately keeps V1 quiesce nonblocking.

V1 behavior:

```text
try_begin_quiesce()
  active branch guards present -> CommitQuiesceUnavailable
  quiesce already active       -> CommitQuiesceUnavailable
  no active branch guards      -> CommitQuiesceGuard

try_acquire_branch_guard(branch)
  quiesce active               -> CommitQuiesceUnavailable
  same branch guard active     -> BranchGuardUnavailable
  otherwise                    -> CommitBranchGuard
```

Rationale:

1. L7 currently has no scheduler, clock, or condition-variable abstraction.
2. `wasm32-unknown-unknown` no-default-feature checks remain part of the
   storage-next command matrix.
3. L8 owns recovery/checkpoint retry loops and can decide whether repeated
   `CommitQuiesceUnavailable` becomes a maintenance timeout.
4. The storage safety property is still satisfied: once a quiesce token exists,
   no new mutating branch guard can start.

L7L should update parent-plan language where needed so "timeout" means a typed
unavailable fact that an L8 caller may turn into a deadline failure, not a
wall-clock wait implemented inside L7.

## Required Ordering

### Cache Commit

```text
validate batch and cache mode
check unresolved durable gate
validate branch registry/generation
acquire branch guard
read current visible version
reject target branch if applied rows are above visible
capture L6 read view capped at visible version
validate read-set/CAS conflicts
allocate version and timestamp
stamp rows and timeline
append all rows into L6 atomically
publish visible version
drop branch guard
```

### Durable Commit

```text
validate batch and durable mode
check unresolved durable gate
validate branch registry/generation
acquire branch guard
read current visible version
reject target branch if applied rows are above visible
capture L6 read view capped at visible version
validate read-set/CAS conflicts
allocate version and timestamp
stamp rows and timeline
build WalRecord through format layer
append WAL through L4
append all rows into L6 atomically
publish visible version
drop branch guard
```

### Replay

Replay is not a normal mutating commit. It applies already-durable rows selected
by L8 and therefore bypasses ordinary branch admission and conflict validation.
When L8 needs process-wide exclusion for replay, it should acquire quiesce
before calling L7K replay. L7L should document this handoff but not move replay
into the normal branch guard path.

## Cross-Branch Visible-Version Safety

The visible-version tracker is global, while L7H/L7I executors receive one
target `BranchLocalState`.

V1 safety is enforced by two rules:

1. The target branch may not contain applied rows above the current global
   visible version when a normal commit starts.
2. Durable applied-not-visible state on any branch must be represented by
   `CommitUnresolvedDurableGate`, which blocks all normal mutating commits.

L7L should document that L7H/L7I cannot scan every branch for externally seeded
hidden rows because they are intentionally passed only the target branch. A
future L8/L9 registry-wide commit coordinator may add a stronger global scan.
For V1, externally injected hidden rows without an unresolved durable gate are
invalid test setup, not a normal runtime state.

## Implementation Work

### L7L-A: Guard And Quiesce Contract Comments

Update `guard.rs` doc comments to state:

1. branch guards are single-process, in-memory admission tokens;
2. same branch has at most one active mutating guard;
3. different branches may hold guards concurrently;
4. quiesce is nonblocking in V1;
5. `CommitQuiesceGuard` blocks new mutating guards until dropped;
6. guards release by RAII `Drop`, including poisoned-lock cleanup paths;
7. direct callers should not hold a guard across unrelated blocking IO outside
   the documented commit protocol.

### L7L-B: Admission Order Documentation

Update `branch_registry.rs`, `cache.rs`, and `durable.rs` comments to make the
order reviewable:

1. registry/generation validation happens before guard acquisition;
2. guard acquisition happens before conflict validation and allocation;
3. branch guard is held through visibility publication;
4. unresolved durable gate is checked before normal branch admission;
5. replay remains separate.

If the current `CommitBranchRegistry` vector storage stays in place, document
that branch counts are expected to be small in this crate-private V1 registry
and that uniqueness is enforced by validation. If review prefers matching the
parent plan exactly, convert the registry storage to `BTreeMap<BranchId, ...>`
as a mechanical cleanup in this slice.

### L7L-C: Conflict Staleness Boundary

Add a short comment near `CommitBranchReadViewConflictSource` or the cache and
durable call sites:

```text
Conflict validation is single-process safe because target-branch admission is
held from read-view capture through visibility publication. It is not a
multi-process isolation primitive.
```

This prevents future callers from assuming that L7F alone replaces durable
writer-lock or recovery coordination.

### L7L-D: Guard Release And Failure Paths

Review cache and durable executor tests and add missing direct cases for guard
release after:

1. conflict rejection;
2. allocator/visible mismatch rejection;
3. L6 apply failure;
4. visible publication failure;
5. WAL append failure;
6. unresolved durable gate rejection;
7. branch-generation rejection;
8. quiesce rejection.

Each case should attempt to reacquire the branch guard after the failure.

### L7L-E: Quiesce Direct Tests

Add or extend direct guard tests for:

1. active branch guard prevents quiesce start;
2. quiesce token prevents mutating guard acquisition;
3. quiesce token drop reopens mutating admission;
4. repeated quiesce while active rejects;
5. read-only diagnostic does not acquire a mutating branch guard;
6. failed quiesce attempt does not latch closed state.

### L7L-F: Deterministic Scheduler-Style Harness

Add a small deterministic operation harness, probably under
`crates/storage-next/src/testkit/commit_runtime_branch_guards.rs`, that
interprets script bytes as guard/quiesce operations:

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

The harness should not spawn threads. Its job is to explore interleavings of
token acquisition and release deterministically, then assert:

1. no operation leaves a stuck guard after its token is dropped;
2. quiesce and mutating guards are mutually exclusive;
3. same-branch double acquisition fails;
4. different-branch acquisition can coexist;
5. read-only diagnostic is independent of guard state;
6. final cleanup leaves the guard set open.

L7M can later lift this into broader generated commit scripts that include
cache, durable, replay, and fault windows.

### L7L-G: Cross-Branch Safety Evidence

Add direct tests proving:

1. branch A and branch B can hold guards concurrently;
2. branch A commit failure releases branch A without touching branch B;
3. target-branch applied-above-visible rejection happens in cache mode;
4. target-branch applied-above-visible rejection happens in durable mode;
5. unresolved durable gate on branch A blocks normal commit on branch B;
6. read-only diagnostic remains allowed while unresolved durable gate exists.

The tests should state that registry-wide hidden-row scans are outside L7H/L7I
because those executors only own one `BranchLocalState`.

### L7L-H: Porting Log And Deferred Map

Update `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
after implementation with:

1. preserved old-storage behavior;
2. intentionally changed V1 behavior, especially nonblocking quiesce;
3. deferred blocking wait/deadline scheduling;
4. verification commands run;
5. sensitivity probes for guard/quiesce ordering.

Do not add automated tests that only check planning-document links. The
implementation and test suites should exercise behavior.

## Sensitivity Probes

L7L should record probes that would fail if a developer:

1. allows a mutating branch guard while quiesce is active;
2. allows quiesce while branch guards are active;
3. forgets to release branch guards on drop;
4. holds a same-branch guard twice;
5. blocks different-branch guards unnecessarily;
6. moves guard acquisition after conflict validation;
7. moves allocation before guard acquisition;
8. publishes visible version after dropping the branch guard;
9. allows cache commit while unresolved durable gate is set;
10. allows durable commit while unresolved durable gate is set;
11. treats read-only diagnostic as a mutating guard;
12. changes target-branch applied-above-visible rejection to acceptance.

## Exit Criteria

L7L is complete when:

1. guard/quiesce V1 semantics are documented in code and plans;
2. all direct guard/quiesce tests pass;
3. cache and durable failure paths prove guard release;
4. conflict-validation staleness boundary is documented;
5. cross-branch visible-version safety policy is documented and tested;
6. deterministic scheduler-style guard harness is present;
7. generated scaffold counters cover the L7L categories needed by L7M;
8. source guards still pass;
9. no public transaction/session API is exposed;
10. no wall-clock sleep, async runtime, or blocking wait dependency is added to
    the commit runtime.
