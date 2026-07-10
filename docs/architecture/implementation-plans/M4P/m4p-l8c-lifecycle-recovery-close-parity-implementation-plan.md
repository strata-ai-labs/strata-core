# M4P-L8C Implementation Plan: Lifecycle Recovery, Close, And Quarantine Parity

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Sibling plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8c-lifecycle-recovery-close-parity-test-plan.md`

Architecture context:

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Objective

Close the lifecycle parity gaps that sit *outside* the maintenance-scheduling
critical path covered by the parent L8 parity slice and L8B.

L8 and L8B restored automatic maintenance, flush drain, scored compaction,
write-admission pressure facts, and benchmark diagnostics, then queued the
remaining maintenance-shape work for the 5M/10M benchmark closeout. The
broader L8 implementation plan also requires that storage-next preserve
old-engine recovery, close, quarantine, open-time, and recovery-health
contracts. The current implementation has measurable deltas in those areas
that this plan addresses.

L8C covers five groups:

1. recovery robustness against partial-tail, codec evolution, fault-cap, and
   bootstrap gaps;
2. close ordering, shutdown deadlines, and drop-on-uncalled-close;
3. quarantine cleared-branch race and purge batching;
4. open-time writer-guard and directory-layout guarantees;
5. recovery-health classification surface and tie-breaks.

## Scope Summary

| Group | Gaps Covered | Required Before V1 | Primary Decision |
| --- | --- | --- | --- |
| L8C-A. Recovery Robustness | R1, R2, R3, R4, R5, R6 | Yes for R2/R4/R5; decision for R3 | Restore tail-repair safety net, bootstrap max-version, document codec policy. |
| L8C-B. Close And Shutdown Contract | C1, C2, C3, C4, C5 | Yes for C3/C4; decision for C1/C2 | Choose deadline policy, drop policy, and verify manifest write-through. |
| L8C-C. Quarantine Coverage | Q1, Q2, Q3 | Yes for Q1; decision for Q2/Q3 | Close cleared-branch race; record batching and global-gate decisions. |
| L8C-D. Open-Time Guarantees | O1, O2 | Decision required | Define writer-guard acquisition contract and durable layout surface. |
| L8C-E. Recovery Health Surface | H1, H2 | Yes for stability of facts | Lock display contract and degradation tie-break. |

## Existing Baseline

Assume the following first-pass L8 + L8B behavior exists:

1. cache and durable runtimes implement a typed lifecycle state machine;
2. capability validation rejects unsupported modes before any durable side
   effect;
3. durable open assembles WAL, manifest, snapshot, table, and quarantine
   services;
4. recovery converges into typed `RecoveryHealth` with degradation classes
   `DataLoss`, `Telemetry`, `PolicyDowngrade`, plus `Healthy`;
5. WAL recovery uses checkpoint and flush-manifest watermarks to skip
   already-applied records;
6. close transitions through `Open -> Closing -> Closed` with idempotent retry
   and typed `ClosePhase` facts;
7. quarantine offers `safe_for_candidate`, `fresh_for_candidate`, `stale`, and
   `incomplete` proof variants and a four-stage `QuarantineStage` taxonomy;
8. mutating commits go through `evaluate_mutating_write_admission` and
   `schedule_post_commit_maintenance_for_branch` after success.

If any of those regress while implementing L8C, stop and restore the
first-pass invariant before adding broader parity mechanics.

## Non-Goals

L8C must not implement:

1. public product retry UI or user-facing close commands;
2. cross-process or distributed close coordination;
3. forwards/backwards durable byte-format migration;
4. follower or multi-process recovery;
5. legacy-format engine compatibility for V1 production databases;
6. retention-aware row pruning, lazy table reads, or memory budgets — these
   belong to later slices (L8V, L8X, L8W);
7. durable table-manifest byte changes — those remain L8Q-owned;
8. engine-next, intelligence-next, or product surfaces.

## L8C-A. Recovery Robustness

Gaps covered: R1, R2, R3, R4, R5, R6.

Goal: ensure durable recovery converges on the same outcomes the old engine
produced for partial-tail crashes, large fault sets, and codec evolution, and
restores all bootstrap facts the next session depends on.

Tasks:

1. Decide partial-tail repair policy.
   - Option A: always truncate partial WAL tail during recovery, matching old
     engine behavior, with a counter `recovery_wal_tail_truncated`.
   - Option B: keep strictness-gated repair as today and document the
     default `RecoveryStrictness` chosen by L9.
   - Option C: introduce a third `RecoveryStrictness::TailRepairOnly` that
     allows tail truncation but rejects any other lossy fallback.
   - Whatever option is chosen, the default strictness used by the typical L9
     open path must be recorded in the semantic decision register.
2. Restore sidecar rebuild during recovery.
   - Walk closed WAL segments after replay.
   - For any segment whose sidecar is missing or fails header/CRC check,
     rebuild the sidecar from the segment records.
   - Best-effort: log warnings on rebuild failure, do not escalate.
   - Skip the active (last) segment so the writer continues to own it.
   - Counter: `recovery_wal_sidecars_rebuilt`, `recovery_wal_sidecars_skipped`.
3. Restore WAL-derived max-version and max-txn-id bootstrap.
   - Walk replay records and record `max_observed_commit_version`.
   - If the L7 allocator and visible tracker depend on a starting version,
     seed them from the recovered max instead of zero.
   - Counter: `recovery_max_commit_version`, `recovery_max_txn_id`.
4. Decide `max_faults` policy.
   - Option A: increase default `max_faults` to an effectively unbounded value
     for production, keeping the cap as a defensive limit only.
   - Option B: keep the small cap but classify "fault budget exhausted" as a
     distinct degradation class instead of `RecoveryFailed`.
   - In either case, expose the configured `max_faults` in
     `StorageOpenOutcome` so callers know whether the fault list is complete.
5. Decide codec-evolution policy for V1.
   - Option A: keep strict codec-id equality; reject all mismatches.
   - Option B: introduce a `LegacyCodecFallback` strictness or capability flag
     that accepts a documented evolution path.
   - Either way, the semantic decision register must record the choice and
     name the owner for any future codec evolution.
6. Add replay accounting counters.
   - `recovery_replay_records_applied`,
   - `recovery_replay_records_skipped_below_watermark`,
   - `recovery_replay_callback_failures`,
   - `recovery_replay_partial_tail_detected`.
   - Use these for the L8C-A exit gate and for the L8 closeout inventory.

Exit gates:

1. Default `RecoveryStrictness` and partial-tail policy are explicit decisions
   recorded in the register, and the chosen behavior is wired into L9 open.
2. Sidecar rebuild runs during recovery or the deferred decision is recorded
   with a counter that would trigger re-evaluation.
3. L7 allocator and visible tracker reflect the recovered max version on the
   first commit after open.
4. Recovery never aborts before classifying *some* fault set; the cap is
   either lifted or classified separately.
5. Codec policy is documented; tests assert the chosen behavior under
   matched and mismatched codec ids.

## L8C-B. Close And Shutdown Contract

Gaps covered: C1, C2, C3, C4, C5.

Goal: make close behavior deterministic, recoverable, and resilient against
hangs and silent data loss.

Tasks:

1. Decide deadline policy for close.
   - Option A: add a deadline parameter to `close` that bounds quiesce wait,
     drain wait, and manifest sync. Returns `CloseTimeout` on expiry with
     state left retryable.
   - Option B: keep fail-fast quiesce but add a deadline on drain phases.
   - Option C: record fail-fast as the V1 policy and assign the deadline
     decision to L9.
   - Whatever option is chosen, the timeout semantics across `QuiesceCommits`,
     `DrainMaintenance`, `SyncDurableState`, and `ReleaseGuards` phases must
     be uniform.
2. Decide state behavior after a close timeout.
   - Option A: restore the lifecycle state to `Open` so the database remains
     usable after a failed close.
   - Option B: leave the lifecycle state in `Closing` and require explicit
     retry close.
   - Option C: split into two facts: retryable from `Closing` versus restored
     to `Open` based on which phase failed.
   - The chosen option must be observable through `CloseOutcome` and the
     `LifecycleState` returned by status queries.
3. Decide drop policy.
   - Option A: implement `Drop for LifecycleDurableLocalRuntime` and
     `Drop for LifecycleCacheRuntime` that runs a best-effort close.
   - Option B: keep no-op drop but require the L9 wrapper layer to enforce
     close in every path. Add a debug assertion or test guard that catches a
     drop without close in test builds.
   - The semantic decision register must record the chosen option and the
     owner for any data-safety guarantee it implies.
4. Verify `active_wal_segment` write-through during the session.
   - Audit every WAL segment rotation path to confirm
     `manifest.persist_active_wal_segment` is called with an fdatasync before
     the rotation is considered durable.
   - If any path is missing the persist call, add it.
   - Counter: `manifest_active_wal_segment_persists`.
5. Add per-task drain deadline inside close.
   - `drain_active_for_close` and `drain_for_close` must observe a deadline so
     a single hung task cannot stall close indefinitely.
   - On per-task deadline expiry, mark the task as deferred and continue the
     drain with health debt recorded.
   - Counter: `close_drain_task_deadline_expiries`,
     `close_drain_total_deferred_tasks`.

Exit gates:

1. Close deadline policy is documented and uniform across phases.
2. Close timeout never leaves the database silently unusable or silently
   usable — the post-timeout state is part of the contract.
3. Drop policy is documented and either implemented or guarded against in
   tests.
4. `active_wal_segment` is provably persisted at every rotation, removing the
   close-time conditional fsync's dependence on session-time mutations.
5. A hung maintenance task cannot indefinitely stall close.

## L8C-C. Quarantine Coverage

Gaps covered: Q1, Q2, Q3.

Goal: close the cleared-branch quarantine race and record purge-batching and
global-gate decisions before retention closes out.

Tasks:

1. Restore the cleared-branch quarantine path.
   - Audit branch clear and delete flows to identify the window where a
     branch's tables transition between owned and unreferenced.
   - Add a quarantine entry point that accepts a cleared-branch hint and
     refuses to use the branch's live reachability facts during the
     transition window.
   - Counter: `quarantine_cleared_branch_candidates`,
     `quarantine_cleared_branch_blocked`.
2. Decide purge batching.
   - Option A: keep per-object purge with a single fresh proof each, and
     measure 10K-quarantine performance to confirm acceptable cost.
   - Option B: add a batched-purge entry point that takes one proof for many
     candidates whose proof is mutually consistent.
   - Either option must avoid silently purging without a fresh proof, and the
     decision must be recorded.
3. Decide global recovery-health gate.
   - Option A: add a single barrier check that gates all reclaim under
     degraded recovery, alongside the per-candidate `safe_for_candidate`.
   - Option B: keep per-candidate-only and accept that operational toggling
     requires per-candidate refusal hints.
   - Either option must be recorded; the per-candidate behavior must not
     silently weaken under degraded recovery.

Exit gates:

1. A branch clear concurrent with quarantine cannot end with an orphaned
   table object or a wrongly purged live table object.
2. Purge batching policy is documented and tests cover the chosen path.
3. Global health gating policy is documented and tests cover the chosen
   path.

## L8C-D. Open-Time Guarantees

Gaps covered: O1, O2.

Goal: make open-time contracts explicit before L9 commits to an open API
shape.

Tasks:

1. Decide writer-guard acquisition policy.
   - Option A: fail-fast on writer-guard contention.
   - Option B: bounded-wait acquisition with a deadline parameter on open.
   - Option C: assign deadline policy to L9; lifecycle stays fail-fast.
   - The chosen option must propagate through `LifecycleCapabilityOutcome` so
     callers can predict behavior.
2. Verify durable directory layout parity.
   - Document the directory layout produced by `LocalFsBackend` durable open.
   - Compare to the old engine's layout to confirm either an exact match or
     an explicit incompatibility.
   - If the layout is intentionally different, record the deviation in the
     decision register and ensure pre-V1 databases produce a typed
     `LifecycleError::LayoutMismatch` instead of opening into an undefined
     state.

Exit gates:

1. Writer-guard acquisition policy is documented and tests cover the chosen
   path.
2. Durable directory layout is either provably identical to old or the
   incompatibility is explicit and caught by typed errors at open.

## L8C-E. Recovery Health Surface

Gaps covered: H1, H2.

Goal: lock the degradation tie-break and stabilize the public health
display.

Tasks:

1. Specify the tie-break when multiple `RecoveryFaultKind`s map to different
   degradation classes.
   - The current implementation makes `DataLoss` dominate
     `Telemetry`/`PolicyDowngrade`. Document this rule.
   - Add a test that mixes one `WalTailRepairFailed` with one
     `QuarantineInventoryMismatch` and asserts `DataLoss`.
2. Stabilize `RecoveryHealth` display text.
   - Audit every `Display` implementation in `lifecycle/health.rs` for
     product-neutral wording.
   - Add a property test that ensures display strings include no product
     vocabulary, IPC vocabulary, follower vocabulary, or roadmap labels.
3. Record `recovery_faults_total` and per-class counters in
   `StorageOpenOutcome` so callers can render health without re-walking the
   fault list.

Exit gates:

1. Tie-break rule is implemented, documented, and tested.
2. Display text is stable, product-neutral, and tested.
3. Health surface counters are part of the open outcome contract.

## Execution Order

Recommended order:

1. L8C-A2 sidecar rebuild and L8C-A6 replay counters first — measurement
   infrastructure for the rest of the plan.
2. L8C-B4 manifest write-through audit — answers a yes/no question that
   determines whether L8C-B's conditional close-time fsync is safe.
3. L8C-A1 partial-tail and L8C-A4 `max_faults` decisions — affect the public
   open contract and should land before L9 starts.
4. L8C-B1, L8C-B2, L8C-B3 close decisions — depend on L8C-B4.
5. L8C-C1 cleared-branch quarantine — narrow correctness fix; can land in
   parallel with the close work.
6. L8C-A3 max-version bootstrap and L8C-A5 codec policy — semantic decision
   plus targeted plumbing.
7. L8C-D1 writer-guard and L8C-D2 layout decisions — block L9 open API but
   not the rest.
8. L8C-E health surface — depends on the recovery faults landed in L8C-A.

## Stop Conditions

Stop and revise this plan if:

1. partial-tail repair cannot be made strictness-aware without changing the
   WAL service surface;
2. sidecar rebuild requires touching L3 codec internals;
3. close deadlines force the maintenance executor to gain a background
   thread;
4. drop semantics require returning errors from `Drop`, which is not
   expressible in Rust;
5. cleared-branch quarantine race requires changing the branch lifecycle
   API L8Y is about to ship;
6. writer-guard deadline cannot be expressed without changing the backend
   trait;
7. recovery-health display stabilization conflicts with an L9 product wording
   choice that has already shipped.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
cargo test -p strata-storage-next --locked --test lifecycle_faults
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test crash_recovery
```

Full storage-next gates:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

Crash window proofs:

```bash
cargo test -p strata-storage-next --locked --test crash_recovery -- --include-ignored
```

## Completion Criteria

L8C is complete when:

1. partial-tail policy and default `RecoveryStrictness` are explicit and
   wired into L9 open;
2. sidecar rebuild runs at recovery time or its deferral is documented with a
   counter trigger;
3. WAL replay restores `max_observed_commit_version` to L7 allocator and
   visible tracker;
4. `max_faults` does not silently truncate the fault list without a typed
   class change;
5. codec evolution policy for V1 is documented and tested;
6. close deadline policy and post-timeout state are explicit and tested;
7. drop policy is explicit, implemented or guarded against in tests, and
   recorded;
8. `manifest.persist_active_wal_segment` is provably called for every WAL
   rotation;
9. close drain cannot be stalled indefinitely by a single hung task;
10. cleared-branch quarantine race is closed and tested;
11. purge batching and global health-gate policies are documented and tested;
12. writer-guard acquisition policy is documented and tested;
13. durable directory layout matches old engine or rejects pre-V1 databases
    with a typed error;
14. recovery-health tie-break is implemented and tested;
15. recovery-health display text is product-neutral and asserted by tests.
