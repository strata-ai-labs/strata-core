# M4P-L8C Test Plan: Lifecycle Recovery, Close, And Quarantine Parity

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8c-lifecycle-recovery-close-parity-implementation-plan.md`

Sibling test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove or explicitly defer the recovery, close, quarantine, open-time, and
recovery-health parity gaps that affect the lifecycle of a durable
storage-next database outside the maintenance-scheduling critical path.

A gap can close in one of two ways:

1. implementation restores the old-engine mechanical behavior and tests prove
   it; or
2. the plan records a V1 decision, counters prove the behavior is safe for
   the expected workload, and a later owner is linked if broader parity is
   still needed.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Recovery robustness | Partial-tail policy, sidecar rebuild, max-version bootstrap, fault-cap, codec policy. | Crashes open into degraded state silently, sidecars never rebuild, bootstrap loses max version, or fault list truncates. |
| Close contract | Deadline, post-timeout state, drop policy, manifest write-through, per-task deadline. | Close hangs on a stuck task, silently leaves the database unusable, or drops without flush. |
| Quarantine coverage | Cleared-branch race, batched purge, global gate. | Branch clear orphans a live table or quarantine purges under degraded recovery. |
| Open-time guarantees | Writer-guard policy, durable layout. | Open fails or succeeds without a documented contract. |
| Recovery health surface | Degradation tie-break, display text, public counters. | Health classification flips between releases, display leaks product wording. |

## Semantic Decision Register Tests

Every decision test should assert that the decision is recorded in the
lifecycle architecture or implementation plan before weakening an old-engine
oracle.

Required decisions:

1. **Default `RecoveryStrictness`**
   - Allowed outcomes: `Strict`, `AllowExplicitLossyFallback`, or a new
     `TailRepairOnly` variant.
   - Test failure: typical L9 open path uses a strictness that is not
     recorded.
2. **Partial-tail repair policy**
   - Allowed outcomes: always truncate, strictness-gated, or never.
   - Test failure: partial-tail repair runs under a strictness it is not
     allowed to run under.
3. **`max_faults` cap policy**
   - Allowed outcomes: unbounded, bounded with separate classification,
     bounded with `RecoveryFailed`.
   - Test failure: fault list is silently truncated and reported as a clean
     class.
4. **Codec evolution policy**
   - Allowed outcomes: strict equality, opt-in legacy fallback, deferred to
     post-V1.
   - Test failure: codec mismatch is reported as recoverable without an opt-in
     fact.
5. **Close deadline policy**
   - Allowed outcomes: fail-fast, bounded wait, L9-owned wait.
   - Test failure: deadline semantics differ between `QuiesceCommits`,
     `DrainMaintenance`, `SyncDurableState`, and `ReleaseGuards`.
6. **Close post-timeout state**
   - Allowed outcomes: restored to `Open`, left in `Closing`, phase-specific.
   - Test failure: same close-timeout fact appears with different post-states
     in the same release.
7. **Drop-on-uncalled-close policy**
   - Allowed outcomes: best-effort drop close, no-op drop with test guard,
     no-op drop with L9 owner.
   - Test failure: data loss after dropping the runtime without calling
     close, with no recorded ownership.
8. **Writer-guard acquisition policy**
   - Allowed outcomes: fail-fast, bounded wait, L9-owned wait.
   - Test failure: writer-guard wait time is observable without a documented
     contract.
9. **Cleared-branch quarantine policy**
   - Allowed outcomes: pre-clear barrier, two-phase clear, accept-current.
   - Test failure: branch clear concurrent with quarantine produces an
     orphan or wrongly purged live table object.

## Recovery Robustness Tests

Coverage for R1, R2, R3, R4, R5, R6.

Correctness tests:

1. Partial-tail recovery under the chosen default strictness either
   truncates or returns a typed rejection — never silent corruption.
2. Sidecar rebuild, when enabled, runs only over closed segments and never
   touches the active segment.
3. Sidecar rebuild failure is logged and never escalates to recovery
   failure.
4. Recovery records the WAL's `max_observed_commit_version` and seeds the L7
   allocator and visible tracker; the next commit allocates a version above
   the recovered max.
5. Recovery fault accumulation respects the configured `max_faults` and
   classifies "fault budget exhausted" distinctly from the original fault
   classes.
6. Codec mismatch is classified per the recorded policy and never silently
   accepted.
7. Replay records below `replay_start` are skipped without callback
   invocation.
8. Replay callback failure halts replay immediately and is recorded as a
   typed fault.

Mechanical counter tests:

1. `recovery_wal_tail_truncated` increments when partial-tail repair runs.
2. `recovery_wal_sidecars_rebuilt` and `recovery_wal_sidecars_skipped`
   increment in fixtures with missing or corrupt sidecars.
3. `recovery_max_commit_version` and `recovery_max_txn_id` are emitted in
   open outcomes.
4. `recovery_replay_records_applied`,
   `recovery_replay_records_skipped_below_watermark`,
   `recovery_replay_callback_failures`, and
   `recovery_replay_partial_tail_detected` increment under the corresponding
   fixtures.
5. `recovery_faults_total` and per-class counters match the recovered fault
   list.

Generated tests:

1. Random crash points across WAL append and segment rotation.
2. Random sidecar corruption across closed segments.
3. Random fault distributions across the chosen `max_faults` value.
4. Random codec id mismatches under the configured policy.

Pass gates:

1. Partial-tail behavior is identical across runs for the same crash fixture.
2. Sidecar rebuild never advances or rewrites WAL data.
3. Recovered max version is preserved across reopen.
4. Fault list completeness is observable.

## Close And Shutdown Contract Tests

Coverage for C1, C2, C3, C4, C5.

Correctness tests:

1. Close with no in-flight commits and no pending maintenance completes in a
   single call.
2. Close with an active commit either waits up to the configured deadline or
   returns `CloseTimeout` immediately, depending on the recorded policy.
3. Close timeout leaves the lifecycle state in the documented state and
   exposes it through `LifecycleState` queries.
4. Retry close after timeout completes when the blocking condition clears.
5. Drop without calling close behaves per the recorded drop policy: a
   best-effort close, a typed test-only assertion, or a documented L9
   responsibility.
6. `manifest.persist_active_wal_segment` is called for every WAL segment
   rotation observed in the test harness.
7. Per-task drain deadline expires the hung task and continues the close
   drain with health debt.
8. Cache close cannot leave queued maintenance work after the state
   transitions to `Closed`.
9. Durable close cannot leave the writer guard held after transitioning to
   `Closed` on the success path.

Mechanical counter tests:

1. `close_drain_task_deadline_expiries` increments when a per-task deadline
   fires.
2. `close_drain_total_deferred_tasks` matches the count of tasks deferred by
   deadline.
3. `manifest_active_wal_segment_persists` increments per WAL rotation.
4. `close_quiesce_wait_observed_ms` is zero when fail-fast and bounded
   otherwise.

Generated tests:

1. Random close interleavings with active commits and pending maintenance.
2. Random hung-task injection across `MaintenanceTaskKind`.
3. Random drop-without-close interleavings.
4. Random WAL rotations during close to confirm rotation persists segment
   facts before close depends on them.

Pass gates:

1. Close never silently leaves a database unusable or silently usable.
2. Drop policy is observable in test builds.
3. Manifest write-through is provable without close-time fsync of arbitrary
   pages.
4. A hung task cannot stall close past its per-task deadline.

## Quarantine Coverage Tests

Coverage for Q1, Q2, Q3.

Correctness tests:

1. A branch clear concurrent with quarantine candidate evaluation does not
   produce an orphaned live table object.
2. A branch clear concurrent with quarantine candidate evaluation does not
   wrongly delete a referenced table object.
3. Purge batching, if implemented, never purges a candidate without a fresh
   proof.
4. Global health-gate, if implemented, prevents reclaim under degraded
   recovery even when per-candidate proofs would otherwise allow it.
5. Repeated cleared-branch quarantine attempts are deterministic and
   idempotent.

Mechanical counter tests:

1. `quarantine_cleared_branch_candidates` increments on each cleared-branch
   candidate.
2. `quarantine_cleared_branch_blocked` increments when the cleared-branch
   path refuses to act on transition-window reachability facts.
3. Batched purge counters (if implemented) report `purge_batches` and
   `purge_objects` separately.
4. Global health-gate (if implemented) emits `reclaim_blocked_by_global_gate`
   under degraded recovery.

Generated tests:

1. Random interleavings of branch clear, quarantine, and purge.
2. Random degraded-recovery transitions during reclaim.
3. Random large quarantine inventories (1K, 10K, 100K) under the chosen
   purge policy.

Pass gates:

1. No combination of clear/quarantine/purge produces an orphan or unsafe
   delete.
2. Purge cost is bounded by the chosen batching policy.
3. Global gate, if chosen, takes precedence over per-candidate proofs in
   degraded recovery.

## Open-Time Guarantees Tests

Coverage for O1, O2.

Correctness tests:

1. Writer-guard acquisition under the chosen policy returns a typed outcome:
   acquired, timed out, or unavailable.
2. Concurrent opens against the same durable root produce a deterministic
   ordering observable through the writer-guard outcome.
3. Durable open against a pre-V1 directory layout, if rejected, returns a
   typed `LifecycleError::LayoutMismatch` and creates no durable side
   effects.
4. Durable open against a current-V1 directory layout produces the documented
   layout on disk.

Mechanical counter tests:

1. `open_writer_guard_acquired`, `open_writer_guard_unavailable`, and
   `open_writer_guard_timed_out` increment per the chosen policy.
2. `open_layout_mismatch_rejected` increments per pre-V1 rejection.

Generated tests:

1. Random concurrent open contention scenarios.
2. Random pre-V1 layout fixtures.

Pass gates:

1. Writer-guard contract is observable through outcomes, not heuristics.
2. Pre-V1 databases are explicitly rejected at open or explicitly accepted
   by a recorded decision.

## Recovery Health Surface Tests

Coverage for H1, H2.

Correctness tests:

1. Mixed fault sets with both `DataLoss` and `Telemetry` faults classify as
   `DataLoss`.
2. Mixed `Telemetry` and `PolicyDowngrade` faults classify as the documented
   tie-break.
3. `RecoveryHealth` display strings include no product, IPC, follower, or
   StrataHub vocabulary.
4. `RecoveryHealth` display strings include no roadmap labels.

Mechanical counter tests:

1. `recovery_faults_total` matches the fault count reported in
   `StorageOpenOutcome`.
2. Per-class counters (`recovery_faults_data_loss`,
   `recovery_faults_telemetry`, `recovery_faults_policy_downgrade`) match
   the classification of recovered faults.

Generated tests:

1. Random fault subsets across the configured `RecoveryFaultKind` values.
2. Random `Display` invocations across the lifecycle health types.

Pass gates:

1. Tie-break rule is deterministic across runs.
2. Display text is stable across releases.

## Source Guards

Source guards must reject:

1. raw filesystem APIs in lifecycle production code outside backend modules;
2. product, engine, IPC, StrataHub, or follower modules in lifecycle
   production code;
3. roadmap labels in production Rust code, comments, panic messages, fixture
   bytes, or user-visible strings;
4. lifecycle production code that imports L9 wrapper modules;
5. close paths that bypass the typed `ClosePhase` taxonomy;
6. recovery paths that bypass the typed `RecoveryFaultKind` taxonomy;
7. quarantine paths that delete objects without a recorded proof outcome;
8. open paths that create durable objects before capability validation
   succeeds.

## Verification Commands

Focused:

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

Full storage-next:

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

## Closeout Checklist

L8C test closeout requires:

1. all semantic decisions recorded;
2. all focused tests pass;
3. source guards pass;
4. generated tests include partial-tail, sidecar, fault-cap, close-deadline,
   cleared-branch, and codec-evolution cases;
5. crash window tests cover open/recovery/close fault families;
6. recovery counters are part of `StorageOpenOutcome`;
7. close counters are part of `CloseOutcome`;
8. any remaining deferral has a trigger counter and named owner.
