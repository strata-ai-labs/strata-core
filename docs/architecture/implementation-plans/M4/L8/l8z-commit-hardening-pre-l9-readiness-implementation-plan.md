# L8Z Implementation Plan: Commit Hardening And Pre-L9 Readiness

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`

## Objective

Harden the commit runtime before L9 exposes storage operations.

L7 established the internal commit protocol: validate a storage batch, allocate
one version/timestamp, append WAL when required, apply rows into L6, install
timeline facts, and publish visibility only after the batch is fully applied.
L8 recovery, maintenance, branch lifecycle, and durable-table work add more
callers and more failure windows. L8Z closes the commit-runtime seams that are
too cross-cutting for the earlier slices: transaction-id policy, branch
generation guard coverage, conflict/concurrency edges, quiesce integration,
global visibility safety, durable uncertainty, replay hardening, minimal
automatic checkpoint/WAL-growth policy, and final Q-Z closeout.

This is still storage-internal. It must not add public transaction sessions,
product ACID claims, merge/cherry-pick/revert semantics, remote sync policy, or
engine DTO behavior. L9 will wrap the hardened storage facts in public API
shapes.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
4. `docs/architecture/storage/l9-storage-api-boundary.md`
5. `docs/architecture/storage/commit-timeline-substrate.md`
6. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
8. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
9. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
11. `crates/storage-next/src/commit/allocator.rs`
12. `crates/storage-next/src/commit/batch.rs`
13. `crates/storage-next/src/commit/branch_registry.rs`
14. `crates/storage-next/src/commit/cache.rs`
15. `crates/storage-next/src/commit/conflict.rs`
16. `crates/storage-next/src/commit/durable.rs`
17. `crates/storage-next/src/commit/durable_gate.rs`
18. `crates/storage-next/src/commit/guard.rs`
19. `crates/storage-next/src/commit/outcome.rs`
20. `crates/storage-next/src/commit/replay.rs`
21. `crates/storage-next/src/commit/timeline.rs`
22. `crates/storage-next/src/commit/visibility.rs`
23. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
24. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
25. `crates/storage-next/src/lifecycle/durable/close.rs`
26. `crates/storage-next/src/lifecycle/checkpoint.rs`
27. `crates/storage-next/src/lifecycle/retention.rs`
28. `crates/storage/src/txn/context.rs`
29. `crates/storage/src/txn/manager.rs`
30. `crates/storage/src/txn/validation.rs`
31. `crates/storage/src/txn/lock_ordering.rs`
32. `crates/storage/src/durability/commit_adapter.rs`
33. `crates/engine/src/database/transaction.rs`

## Existing-Code Source Map

| Current file | Evidence | L8Z action |
|---|---|---|
| `commit/allocator.rs` | Owns commit-version allocation, timestamp guard, and recovery catch-up. | Verify no V1 transaction-id allocator exists; harden timestamp catch-up and monotonicity around recovery/bootstrap. |
| `commit/batch.rs` | Validates storage mutations, branch ids, duplicate keys, durability mode, and timeline/user row split. | Add final pre-L9 validation coverage and source guards for storage-only mutation shapes. |
| `commit/branch_registry.rs` | Branch generation descriptors and writable-state admission exist. | Ensure every commit, replay, lifecycle, and queued storage task crossing a boundary carries generation facts or has an explicit no-generation reason. |
| `commit/guard.rs` | Per-branch commit guards and quiesce token exist. | Integrate quiesce with branch lifecycle, checkpoint, close, recovery, and L9-bound admission helpers. |
| `commit/conflict.rs` | Read-set and CAS validation run through a branch read-view source. | Harden the validation window, document single-process assumptions, and add concurrency/fault tests. |
| `commit/cache.rs` | Cache commits apply into L6 and publish visible facts without WAL. | Prevent applied-not-visible rows on one branch from becoming visible by side effect of another branch. |
| `commit/durable.rs` | Durable commits append WAL, apply rows, publish visibility, and record unresolved durable failures. | Harden cross-branch post-WAL failures, forced/not-forced durability uncertainty, and phase classification. |
| `commit/durable_gate.rs` | Tracks one unresolved durable fact. | Decide whether to serialize durable admission globally or support multiple unresolved durable facts; do not misclassify later failures. |
| `commit/replay.rs` | Replays durable WAL records and reconciles unresolved durable gates. | Ensure replay validates user rows, timeline rows, branch generations, and idempotent duplicate records consistently. |
| `commit/timeline.rs` | Stores timestamp-to-version entries and range lookup facts. | Pin duplicate timestamp tiebreakers, structured bounds, and branch isolation for timeline queries. |
| `commit/outcome.rs` | Carries commit outcome kinds, phases, durability, and visibility facts. | Reject impossible durability/visibility combinations and preserve phase-specific residual facts. |
| `lifecycle/durable/bootstrap.rs` | Calls replay and catches up visible/allocator facts. | Ensure bootstrap cannot bypass commit hardening or leave hidden lower-version rows behind. |

## Old Codebase Porting Map

The old transaction machinery is behavioral evidence, not the target public API.
L8Z ports the safety mechanics and leaves product transaction surfaces behind.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `storage/src/txn/manager.rs` | Commit version allocation, branch commit locks, quiesce, visible-version tracking, pending versions, branch deletion barriers. | Keep version, guard, quiesce, visibility, and branch-generation safety. Retire storage transaction ids for V1. | Allocation, quiesce, hidden-row, generation, and recovery catch-up tests. |
| `storage/src/txn/context.rs` | Staged mutations, read-your-writes overlay, CAS/read-set facts, delete/TTL facts. | Keep only internal commit-batch validation facts. Do not expose public transaction sessions. | Batch validation and conflict model tests. |
| `storage/src/txn/validation.rs` | Read-set and CAS validation against a captured branch view. | Preserve storage snapshot-isolation style conflict checks. | Stale read-set/CAS and concurrency tests. |
| `storage/src/txn/lock_ordering.rs` | Explicit lock-order discipline for commit path. | Rebuild as lock-order comments, source guards, and deterministic tests around branch guard, unresolved gate, visible tracker, and lifecycle quiesce. | No deadlock, no guard leak, no lock-order inversion. |
| `storage/src/durability/commit_adapter.rs` | WAL-before-visible ordering and ambiguous durability classification. | Harden durable local commit phases over storage-next WAL records and L4 services. | WAL append/apply/visible failure matrix. |
| `engine/src/database/transaction.rs` | Writer health, backpressure, branch generation checks, post-commit observers. | Keep storage pressure and branch generation facts; product observers stay above L9. | Storage facts are available without product observer hooks. |

Do not port:

1. public begin/commit/rollback storage transaction sessions;
2. storage transaction id allocation in V1;
3. product transaction timeouts or user-facing transaction metrics;
4. product observer callbacks;
5. engine DTO mapping;
6. distributed transaction coordination;
7. cross-branch atomic product commits;
8. remote/hub synchronization.

## Scope

L8Z implements:

1. final V1 transaction-id decision and source guards;
2. branch-generation guard coverage across commit, replay, branch lifecycle, and
   queued storage maintenance;
3. conflict-validation hardening for read-set, CAS, blind writes, stale views,
   and validation-window documentation;
4. quiesce integration for checkpoint, fork, clear, delete, recovery, close, and
   L9-bound admission helpers;
5. durable gate hardening so cross-branch post-WAL failures are classified and
   tracked correctly;
6. global visibility safety so applied-but-not-visible rows cannot become
   readable by another branch advancing visible version;
7. durability-uncertain handling for not-forced durable appends and replay;
8. timeline hardening for duplicate timestamps, structured bounds, timeline-only
   WAL payload rejection, and branch isolation;
9. outcome validation for impossible durability/visibility/fact combinations;
10. lock-order and guard-release assurance;
11. minimal automatic checkpoint/WAL-growth policy using existing checkpoint and
    WAL retention hooks;
12. generated, fault, fuzz, and closeout coverage for L8Q-L8Z;
13. pre-L9 source guards for public visibility, product vocabulary, and milestone
    labels in Rust code, tests, fixture bytes, and user-facing errors.

L8Z does not implement:

1. public storage API methods;
2. public transaction sessions;
3. transaction IDs for V1;
4. cross-branch atomic commits;
5. distributed writer coordination;
6. product branch workflows;
7. remote/hub commit synchronization;
8. query-layer sort/filter/index behavior;
9. object-store production provider semantics;
10. rich background checkpoint scheduling or adaptive maintenance policy beyond
    the minimal bounded-WAL trigger;
11. physical format freeze, backwards compatibility, migration policy, and
    format golden vectors. L10 owns that workstream.

## Transaction-Id Policy

V1 storage-next uses commit versions as the durable storage ordering identity.
It does not keep a separate durable transaction id.

Rules:

1. No commit, WAL, replay, lifecycle, or public-ready storage shape may require a
   transaction id.
2. Old transaction-id references must be either removed or recorded in the
   deferred map as non-V1 private optimization work.
3. Recovery catch-up is commit-version and timestamp catch-up only.
4. If a private transaction-id allocator is added later, it must include durable
   WAL encoding, recovery catch-up, replay idempotency, and closeout tests in
   the same slice.

## Branch Generation Guard Coverage

Every operation that crosses a queue, durable boundary, or asynchronous-looking
fault window must carry or validate branch generation.

Required surfaces:

1. cache commit;
2. durable commit;
3. replay;
4. branch create/recreate/delete/clear/fork;
5. flush;
6. compaction;
7. materialization;
8. checkpoint row capture when branch state is targeted;
9. row pruning;
10. table-manifest publication;
11. retention/quarantine branch-scoped decisions;
12. close drain of branch-scoped maintenance.

Rules:

1. Stale generation rejects before table-object publication or L6 mutation.
2. Generation mismatch errors include branch id, expected generation, and actual
   generation.
3. Not-supplied generation is allowed only for explicitly storage-internal
   bootstrapping paths that prove exclusivity.
4. Deleted lifecycle branches reject commit admission
   (`LifecycleBranchStatus::Deleted`).
5. `CommitBranchState::Deleting` is transient inside `delete_branch` and not
   externally observable; no caller outside `delete_branch` may observe or set
   this state.

## Conflict And Concurrency Hardening

Rules:

1. Conflict validation happens after branch admission and before allocation.
2. Read-set validation uses the captured visible bound, not latest mutable state.
3. CAS validation compares the target branch only.
4. Blind writes do not require a read-set.
5. Same-branch commits are serialized by branch guard.
6. Cross-branch commits remain independent unless global visible/durable safety
   requires admission blocking.
7. Single-process assumptions must be documented next to the conflict source.
   Multi-process writer coordination remains L4/backend writer-lock scope.
8. Validation failures release guards and leave allocator/visible/durable facts
   unchanged.

## Quiesce Integration

Quiesce is the storage mechanism for operations that need no active commit
writers while they capture or replace state.

Required users:

1. checkpoint row capture;
2. branch fork and fork-at-history;
3. branch clear and delete;
4. durable close;
5. L9-bound maintenance and administrative operations that need a stable commit
   boundary.

Recovery replay runs under exclusive open rather than quiesce: the runtime
handle has not been returned to any caller during `complete_recovery`, and
`LifecycleStateMachine::admit` rejects mutating commits while the runtime is
in the `Recovering` state. Adding quiesce to bootstrap would be defense in
depth but is not required for V1 correctness; see Open Questions §A.

Rules:

1. Quiesce rejects while branch guards are active.
2. New branch guards reject while quiesce is active.
3. Quiesce release is RAII and tested under every error path.
4. Quiesce does not publish visibility or durability facts by itself.
5. Quiesce errors preserve source chains and stable error codes.

## Global Visibility Safety

The visible-version tracker is global. It must not let any lower-version row
that was applied but intentionally not visible become readable because an
unrelated branch later publishes a higher visible version.

Rules:

1. Cache-mode applied-not-visible states either block global visible advancement
   or are represented by an explicit hidden-applied gate.
2. Durable applied-not-visible states remain in the unresolved durable gate until
   replay or repair proves visibility.
3. Cross-branch commits cannot advance visible version past hidden lower-version
   rows without first resolving them.
4. Recovery and close preserve hidden/applied facts.
5. Tests must distinguish branch-local max version from global visible safety.

## Durable Gate Hardening

The durable gate must classify every post-WAL failure.

The durable gate ships as single-admission: `active_admission: bool` plus
`Option<unresolved>` (`crates/storage-next/src/commit/durable_gate.rs:38-42`).
Keyed multi-entry tracking is explicitly deferred — implementing it requires
first removing the global serialization. L8Z does not change this design.

Rules:

1. Cross-branch post-WAL admission is mutually exclusive under the
   single-admission lock; a second cross-branch WAL append is structurally
   unreachable (witnessed by `crates/storage-next/src/commit/tests/durable.rs:1290`,
   `durable_active_global_admission_blocks_other_branch_before_wal_append`).
   The verification target is a structural assertion that admission cannot
   reach `record_unresolved` from a second branch while the first is unresolved.

   The reachable mismatch path is the sequential same-branch
   `record_unresolved` mismatch at
   `crates/storage-next/src/commit/durable_gate.rs:266-268`; that path keeps
   its generic error code because existing tests
   (`crates/storage-next/src/commit/tests/durable_gate.rs:369-405`,
   `unresolved_durable_gate_rejects_different_fact_and_exact_clear`) depend
   on it.
2. Idempotent duplicate replay must clear or preserve the matching gate
   deterministically.
3. Durable-but-not-applied and applied-but-not-visible remain distinct.
4. Not-durable, durability-uncertain, and durable states must not be conflated.
5. The gate is closed before final lifecycle close reports clean durable state.

The cross-branch admission lock is held from `admit_mutating_commit` through
`record_unresolved` or successful resolution; cache-mode commits also acquire
it (see Cache Mode Participation below).

### Cache Mode Participation

Cache-mode commits acquire the global admission lock via
`crates/storage-next/src/commit/cache.rs:77` →
`durable_gate.admit_mutating_commit()`. On visibility failure they record an
`applied_not_visible` gate entry whose durability class is `NotDurable`; this
is kept distinct from durable-mode `AppliedButNotVisible` (which carries
durable facts).

Phase 3 will add tests covering cache-mode admission-lock participation and
the `NotDurable` gate class (working titles).

## Durability-Uncertain Handling

Rules:

1. A durable append that may have reached storage but was not forced durable
   returns a typed durability-uncertain outcome.
2. The caller receives enough facts to avoid reporting success.
3. Recovery tests prove that a surviving uncertain WAL record is replayed.
4. Recovery tests prove that an absent uncertain WAL record does not create a
   phantom commit.
5. Allocator and timestamp catch-up remain monotonic in either case.

## Timeline Hardening

Rules:

1. Timeline lookup for timestamp `T` returns the greatest retained version whose
   timestamp is at or before `T`.
2. Equal timestamps use commit version as the deterministic tiebreaker.
3. Timeline index keys are branch id plus timestamp plus version.
4. Timeline-only WAL payloads reject because they carry no user mutation rows.
5. Timeline bounds must be structured as earliest/latest entries or documented
   as loose independent bounds.
6. Timeline corruption maps to typed recovery/commit errors.
7. Branch A timeline rows must never satisfy Branch B as-of reads.

## Minimal Automatic Checkpoint And WAL-Growth Policy

Status: shipped. The rules below describe the live policy; test plan §11 is the
verification matrix. Implementation lives in `crates/storage-next/src/lifecycle/wal_growth.rs`
(policy facts and threshold evaluation), `lifecycle/durable/maintenance.rs::evaluate_wal_growth_policy`
(post-commit hook + maintenance enqueue), and `lifecycle/cache.rs::evaluate_wal_growth_policy`
(cache-mode `NoDurableAction` return). No new implementation work is required for L8Z.

V1 must not depend on a user or product layer to prevent unbounded local WAL
growth. L8Z adds a minimal storage-owned policy over the existing checkpoint,
flush-watermark, and WAL-retention hooks. This is not a threaded scheduler and
not a product maintenance policy.

Rules:

1. Durable local mode records WAL growth pressure using deterministic facts:
   retained WAL bytes, retained WAL segments, and commits since the last
   checkpoint where those counters are available.
2. Crossing a configured threshold enqueues or requests a checkpoint through
   the existing deterministic maintenance executor.
3. The policy may be disabled only through an explicit storage configuration
   intended for tests or advanced embedding; the default V1 configuration keeps
   WAL growth bounded.
4. The policy never truncates WAL unless L8J/L8T retention proof says the
   covered versions are safe to remove.
5. If checkpoint capture is unsafe because recovery, close, quiesce, or branch
   lifecycle work is active, the policy reports pressure/deferred facts instead
   of forcing a checkpoint.
6. If checkpoint publication fails, the policy reports health debt and keeps
   the WAL retained.
7. Cache mode performs no durable checkpoint/WAL action and reports a no-op or
   unsupported durable-maintenance fact.
8. The minimal policy does not choose product scheduling intervals, background
   threads, provider-specific sync policy, or adaptive tuning.

## Outcome And Error Hardening

Rules:

1. `CommitOutcome` rejects visible outcomes without matching visibility facts.
2. `CommitOutcome` rejects not-durable outcomes that claim durable facts.
3. `CommitOutcome` rejects not-visible outcomes that claim visible facts.
4. Phase-specific errors preserve lower-layer source chains.
5. Tests assert stable error codes/classes, not display strings.
6. Errors must not include product transaction wording.

## Pre-L9 Surface Readiness

Before L9 wraps storage-next:

1. commit/lifecycle items remain `pub(crate)` unless explicitly needed by a
   lower-layer test harness;
2. public-ready structs have stable validation methods and outcome facts;
3. docs state which facts L9 may expose and which remain internal;
4. no storage code depends on engine/product modules;
5. milestone labels are absent from Rust code, test names, fixture bytes, fuzz
   corpora, and user-facing error strings;
6. minimal WAL-growth policy facts are available for L9 to expose without
   adding product policy;
7. closeout records the command matrix for L8Q-L8Z;
8. L10-owned format compatibility work is not implied by L8Z closeout.

## Implementation Steps

1. Add a transaction-id V1 decision guard and remove stale transaction-id
   assumptions from commit/lifecycle docs and code.
2. Inventory branch-generation use across commit, lifecycle, replay, and queued
   maintenance.
3. Add missing generation guards and no-generation justifications.
4. Harden conflict-validation documentation and tests around the admission
   window.
5. Wire quiesce helpers through branch lifecycle, checkpoint, recovery, and
   close boundaries where missing.
6. Fix or replace single-entry durable gate behavior for cross-branch post-WAL
   failures.
7. Add global hidden-applied/visibility safety for cache and durable modes.
8. Add durability-uncertain replay tests and any missing residual facts.
9. Harden timeline lookup, bounds, and replay validation.
10. Expand outcome validation for impossible durability/visibility facts.
11. Add generated/fault/fuzz assurance and sensitivity probes.
    (was step 11: minimal automatic checkpoint / WAL-growth trigger — shipped;
    see verification matrix in test plan §11.)
12. Add Q-Z closeout source guards and command matrix records.
13. Update the porting log with old-code behavior, deferrals, probes, and command
    outcomes.

## Deferred

| Deferred item | Owner | Reason |
|---|---|---|
| Public storage API methods | L9 | L8Z is pre-L9 hardening only. |
| Public transaction sessions | Post-V1 or explicit API design | V1 exposes storage operations, not transaction handles. |
| Durable transaction ids | Deferred private optimization | Commit version is the V1 durable ordering id. |
| Cross-branch atomic commits | Later design | Requires deterministic multi-branch lock ordering and product semantics. |
| Distributed/multi-process commit consensus | Backend/provider work | L4 writer lock is the V1 coordination boundary. |
| Product branch workflows | Above L9 | Merge/cherry-pick/revert/restore are product operations. |
| Remote/hub commit sync | StrataHub integration workstream | Local commit runtime only returns raw facts. |
| Rich/background checkpoint scheduler | Post-V1 or runtime policy work | L8Z only adds the minimal bounded-WAL trigger; background threads, adaptive intervals, and product policy remain outside V1. |
| Physical format freeze and compatibility | L10 | Storage byte compatibility, golden vectors, migration/rejection policy, and post-freeze format evolution deserve a dedicated workstream. |

## Open Questions

### A. Recovery quiesce path

Locked in L8Z Phase 1: **exclusive open**. Recovery replay relies on the
exclusive-open contract (`complete_recovery` runs before the runtime handle
is returned) plus `LifecycleStateMachine::admit` rejecting mutating commits
during the `Recovering` state. Adding quiesce to
`crates/storage-next/src/lifecycle/durable/bootstrap.rs` would be defense in
depth but is not required for V1 correctness. Phase 4 does not wire quiesce
into bootstrap.

### B. Fork timeline inheritance

Locked in L8Z Phase 6: **Option C**. Fork timeline inheritance is
implemented via inherited-layer reads + per-row timestamps. The
shipped code path uses `BranchEffectiveReadBound::for_inherited_layer`
(`branch/read.rs:63-81`) to cap as-of reads at `(fork_version,
timestamp)`. Parent's physical rows are read directly via inherited
layers; row `commit_timestamp` drives timestamp matching. No timeline
transcription happens at fork time and no parent-timeline lookup
happens at read time.

The candidates rejected:

- *Option A* (transcribe parent timeline rows under child `branch_id`
  at fork time): unnecessary because the read path uses inherited-
  layer rows directly; no centralized timeline lookup is needed.
- *Option B* (as-of reads consult parent timeline when
  `T < fork_version`): same — the shipped code does not need a
  parent-timeline consultation step.

The `from_rows` filter contract is preserved
(`from_rows` filters by `branch_id`, so a forked child has no parent
timeline rows of its own). The §"Timeline Hardening" rule
*"Branch A timeline rows must never satisfy Branch B as-of reads"*
holds: forked children read parent physical rows via inherited
layers, not parent timeline metadata.

Pinning tests (Phase 6): three tests in
`crates/storage-next/src/lifecycle/tests/branch_lifecycle/fork.rs`
verify the contract:

- `forked_branch_at_timestamp_before_fork_returns_parent_row`
- `forked_branch_at_timestamp_after_fork_returns_child_row`
- `forked_branch_isolated_from_parent_post_fork_commits`

The `for_inherited_layer` docstring documents the contract for
source readers.

### C. Generation field in WAL record

Deferred to Phase 5 plan mode. Two candidate approaches:

- add `branch_generation` to `WalRecord` (format-version bump and
  golden-vector regeneration; touches the M3-frozen format gate);
- derive generation from the catalog at replay dispatch time (no format
  change; relies on the catalog manifest replaying before WAL replay, which
  is already true after L8Y B Phase 2).

Default is catalog-derived. The format-change path requires reviewer approval
under the M3 freeze rules.

## Exit Gate

L8Z is complete when:

1. transaction ids are explicitly absent from V1 commit/runtime code or fully
   implemented with recovery catch-up —
   **Shipped (Phase 2)**: source-guard sweeps in
   `tests/commit_runtime_source_guard.rs` and
   `tests/wal_commit_payload_source_guard.rs`. Probe S1.
2. every boundary-crossing branch operation is generation-checked or has a
   recorded exclusive no-generation reason —
   **Shipped (Phases 2-5)**: existing `stale_*_generation_rejects_after_recreate`
   tests cover cache, durable, flush, compaction, materialization paths.
   `RecoveryExclusivityToken` (Phase 5) compile-time-enforces the
   `set_parent_for_recovery` exclusivity. The replay-time
   stale-generation gate is deferred (see porting log).
3. same-branch and cross-branch conflict/concurrency windows are tested —
   **Shipped**: `commit/tests/conflict.rs` (12 tests),
   `commit/tests/durable.rs:1290`
   `durable_active_global_admission_blocks_other_branch_before_wal_append`.
4. quiesce blocks and releases correctly across commit, recovery, branch
   lifecycle, checkpoint, and close paths —
   **Shipped (Phase 4)**: 10 wrapper edits + 11 tests covering durable
   and cache modes. Recovery uses exclusive open instead of quiesce
   (Phase 1 Open Question §A decision).
5. applied-not-visible and durable-uncertain facts cannot become silently
   visible by side effect —
   **Shipped (Phase 6)**: `cache_applied_not_visible_row_is_visible_to_same_branch_read_your_writes`
   pins the RYW + cross-branch gate contract. Durable-uncertain is
   structurally closed (uncertain records don't reach disk).
6. cross-branch post-WAL failures retain typed phase classification —
   **Structurally unreachable (Phase 3)**: single-admission lock
   prevents the scenario; documented in §"Durable Gate Hardening" rule 1.
7. timeline lookup, duplicate timestamps, and timeline-only WAL validation are
   pinned —
   **Shipped**: `commit/tests/timeline.rs` covers lookups/duplicates;
   `replay_rejects_timeline_only_payload_without_user_mutation`
   (`commit/tests/replay.rs:548`) rejects timeline-only payloads.
8. outcome validation rejects impossible fact combinations —
   **Shipped**: `commit/tests/outcome.rs` covers the validation matrix.
9. minimal automatic checkpoint/WAL-growth policy prevents unbounded WAL growth
   or records typed pressure/deferred facts without unsafe truncation —
   **Shipped (pre-Phase 1)**: `lifecycle/wal_growth.rs` +
   `lifecycle/tests/commit_hardening.rs` (16 tests; verification
   matrix in test plan §11). Phase 1 marked §11 as shipped.
10. generated/fault/fuzz tests cover commit ordering and not only examples —
    **Shipped + Annotated (Phase 7)**: existing `tests/lifecycle_faults.rs`
    (19 tests) + `commit/tests/*` rejection tests cover the 15 plan
    fault windows + 4 audit-flagged edges. Existing 4 fuzz targets
    cover the audit's hardening intent. Test plan §"Fault Windows"
    and §"Fuzz Targets" annotated with the dispositions.
11. Q-Z source guards and command matrix pass and are recorded —
    **Shipped (Phase 7)**: 4 new Q-Z closeout tests in
    `tests/lifecycle_closeout.rs` (lists plans, fuzz distinctness,
    sensitivity ledger, pre-L9 crate-private surface); porting log
    carries the sensitivity ledger + command matrix + closeout
    summary.
