# L8G Implementation Plan: Commit Bootstrap And Recovery Health

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`

Predecessor:
`docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`

## Objective

Implement the durable-local recovery completion step.

L8G consumes the `LifecycleDurableLocalShell` assembled by L8E and the
`LifecycleRecoveryOutcome` produced by L8F. It replays the recovered WAL package
through the L7 replay runtime, catches up L7 version/timestamp/visible facts,
reconciles unresolved durable-gate facts, validates the final recovery health,
and returns an opened durable lifecycle runtime with a `StorageOpenOutcome`.

L8G must not decode WAL bytes, repair WAL tails, load snapshots, rebuild product
objects, or mutate quarantine/maintenance state. Those responsibilities belong
to L8F, later L8 maintenance slices, or layers above storage.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/durable.rs`
9. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
10. `crates/storage-next/src/lifecycle/recovery.rs`
10. `crates/storage-next/src/lifecycle/outcome.rs`
11. `crates/storage-next/src/lifecycle/state.rs`
12. `crates/storage-next/src/commit/replay.rs`
13. `crates/storage-next/src/commit/durable.rs`
14. `crates/storage-next/src/commit/allocator.rs`
15. `crates/storage-next/src/commit/visibility.rs`
16. `crates/storage-next/src/commit/durable_gate.rs`
17. `crates/storage-next/src/branch/state.rs`
18. `crates/storage-next/src/testkit/lifecycle/`

## Existing-Code Source Map

| Current file | L8G evidence | L8G action |
|---|---|---|
| `lifecycle/durable.rs` | L8E creates a durable shell in `Recovering` with services, branch state, registry, guard set, allocator, visible tracker, durable gate, and commit config. | Keep durable assembly separate from commit bootstrap. |
| `lifecycle/durable/bootstrap.rs` | Consumes the L8F package and owns L7 replay, allocator catch-up, visible catch-up, and final open publication. | Add a recovery-completion path that transforms the shell after L8F without re-reading durable objects. |
| `lifecycle/recovery.rs` | L8F returns checkpoint facts, validated table facts, WAL records, repair facts, quarantine facts, and health. | Treat this as the only recovery input. Do not re-read durable objects in L8G. |
| `commit/replay.rs` | `CommitReplayRuntime` validates WAL row facts, applies absent rows idempotently, validates timeline rows, catches up allocators/timestamps, publishes visible facts, and clears matching gates. | Use this for every recovered WAL record. Do not reimplement replay rules in lifecycle code. |
| `commit/visibility.rs` | `VisibleVersionTracker::catch_up_visible_after_replay` can publish a trusted visible version without fabricating a commit. | Use it for checkpoint-only recovery when no WAL record advances visibility past the checkpoint watermark. |
| `commit/durable_gate.rs` | Unresolved durable facts block normal commits and are reconciled by exact replay. | Preserve mismatched gates and fail recovery with typed health/error. Clear only through L7 replay. |
| `lifecycle/outcome.rs` | `StorageOpenOutcome` can report durable recovered visible version and recovery health. | Construct the final durable open outcome only after replay/bootstrap succeeds. |
| `lifecycle/state.rs` | `RecoveryAccepted` transitions `Recovering -> Open`. | Transition only after replay, visible catch-up, and outcome construction succeed. |
| Old engine recovery code | Old open path mixed recovery replay and product primitive reconstruction. | Port only storage commit bootstrap; product reconstruction remains above L8. |

## Scope

L8G implements:

1. a crate-private durable recovery completion runtime, likely in
   `lifecycle/durable/bootstrap.rs`;
2. a final opened durable-local runtime shape, or a state transition on the
   existing durable shell, with `StorageOpenOutcome`;
3. replay of every `LifecycleRecoveredWal::records()` entry through
   `CommitReplayRuntime`;
4. deterministic WAL replay order preservation;
5. durability-class selection from the durable storage mode:
   `DurableLocalStandard -> CommitDurabilityClass::Standard` and
   `DurableLocalAlways -> CommitDurabilityClass::Always`;
6. checkpoint-only visible catch-up from
   `LifecycleRecoveredCheckpoint::trusted_watermark()`;
7. allocator and timestamp catch-up through L7 replay reports, plus
   checkpoint-only timestamp catch-up from installed checkpoint row facts;
8. final visible-version calculation as the max of trusted checkpoint watermark
   and replayed WAL commit versions;
9. unresolved durable-gate reconciliation through L7 replay exact-match clear;
10. typed failure if a different unresolved durable fact blocks replay;
11. recovery-health finalization that preserves L8F degraded health and adds
    L8G failures only when replay/bootstrap fails;
12. final `StorageOpenOutcome` with durable mode, disposition, recovered visible
    version, recovered max commit version, backend capabilities, database id,
    codec id, checkpoint/WAL/table/quarantine recovery facts, L7 bootstrap
    report, recovery health, raw stats, and maintenance readiness `false` for
    V1;
13. transition from `Recovering` to `Open` only after the outcome is valid;
14. ordinary read admission after open using the recovered visible version;
15. durable commit admission after open by composing `CommitDurableRuntime` with
    the existing shell fields and WAL service;
16. close idempotence no worse than cache mode, if the final runtime owns a
    close surface in this slice;
17. testkit counters for replayed records, checkpoint-only visible catch-up,
    idempotent duplicate replay, degraded-health preservation, and replay
    failure classification;
18. source guards that allow L7 replay imports in L8G while still blocking
    product, engine, StrataHub, follower, and raw IO drift;
19. a porting-log entry after implementation.

L8G does not implement:

1. snapshot load, WAL read, WAL tail repair, table-object validation, or
   quarantine inventory load;
2. checkpoint creation or table-backed checkpoint metadata production;
3. branch/table manifest recovery beyond facts already installed by L8F;
4. flush, compaction, materialization, retention, purge, or repair scheduling;
5. maintenance executor behavior or background tasks;
6. public L9 APIs or product primitive reconstruction;
7. multi-branch runtime maps beyond the initial branch carried by the durable
   shell;
8. transaction-id allocation; V1 has no transaction-id allocator;
9. lossy replay heuristics that skip WAL records. If replay fails, recovery
   fails or returns a typed degraded/failed health according to explicit policy.

## Type Surface

Names may change during implementation, but the responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleDurableRecoveryBootstrap<'shell, 'backend, S> {
    shell: &'shell mut LifecycleDurableLocalShell<'backend, S>,
}

pub(crate) struct LifecycleDurableOpenRuntime<'backend, S> {
    state: LifecycleStateMachine,
    open_plan: StorageOpenPlan,
    open_outcome: StorageOpenOutcome,
    services: LifecycleDurableLocalServices<'backend>,
    branch: BranchLocalState,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<S>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    commit_config: CommitRuntimeConfig,
}

pub(crate) struct LifecycleReplayBootstrapReport {
    records_seen: usize,
    records_applied: usize,
    records_already_applied: usize,
    checkpoint_visible_publish: Option<VisibleVersionPublish>,
    recovered_visible_version: CommitVersion,
    recovery_health: RecoveryHealth,
}
```

If moving fields from the shell into an opened runtime would cause churn, the
implementation may instead add a validated "open shell" state wrapper. The
contract is the same: after L8G succeeds, ordinary reads and durable commits are
admitted through the existing lifecycle state machine and L7 runtime components.

## Recovery Completion Protocol

L8G must run these phases in order:

1. require the durable shell is still in `LifecycleState::Recovering`;
2. validate the L8F outcome shape:
   - health is not `Failed`;
   - WAL package records are sorted by commit version in the order returned by
     L4;
   - all packaged records target the shell's initial branch for V1;
   - every record contains L7-valid user rows and timeline rows;
3. compute the durability class from the storage mode;
4. replay WAL records through `CommitReplayRuntime` in package order;
5. fail closed if replay sees mismatch, partial installed state, wrong branch,
   missing timeline rows, or an unresolved durable-gate conflict;
6. after replay, publish/checkpoint catch-up visibility to at least the trusted
   checkpoint watermark when the checkpoint is newer than all replayed records;
7. confirm the visible tracker equals the recovered visible version;
8. construct `StorageOpenOutcome` with durable recovery and bootstrap facts;
9. transition lifecycle state with `RecoveryAccepted`;
10. return the opened durable runtime/report.

No phase may construct user/product objects. No phase may mutate durable
backend objects except through normal L7 replay side effects on L6 in-memory
state and visible/gate facts.

## Replay Rules

L8G delegates replay semantics to L7:

1. construct `CommitReplayRequest::new(record, durability_class)` for each
   recovered WAL record;
2. use `CommitReplayRuntime::new(commit_config, allocator, branch, visible,
   durable_gate)` for each replay step, or one runtime whose borrow lifetime
   covers the ordered loop;
3. preserve L8F WAL record order exactly;
4. do not run normal read-set/CAS conflict validation;
5. do not allocate a new commit version or timestamp;
6. require L7 timeline rows to match the WAL record's branch/version/timestamp;
7. treat `AlreadyApplied` as successful idempotent recovery;
8. treat duplicate mismatch and partial installed rows as recovery failure;
9. clear only a matching unresolved durable gate through L7 replay;
10. leave non-matching unresolved durable gate facts intact and fail recovery;
11. if visible publication fails after apply, surface the L7
    durable-but-not-visible error and keep/record the unresolved durable gate.

## Checkpoint-Only Visibility

L8F can install checkpoint rows into L6 without replaying WAL records. L8G owns
making those rows visible.

Rules:

1. if `checkpoint.trusted_watermark()` is `None`, checkpoint catch-up target is
   `CommitVersion::ZERO`;
2. if a trusted checkpoint watermark exists, the visible tracker must publish to
   at least that watermark before open completes;
3. replayed WAL records may advance visibility beyond the checkpoint watermark;
4. the final recovered visible version is
   `max(checkpoint_watermark.unwrap_or(ZERO), max_replayed_commit_version)`;
5. checkpoint visibility catch-up must use `VisibleVersionTracker` monotonic
   APIs, not direct field mutation;
6. if visible catch-up fails, recovery fails before the runtime becomes `Open`;
7. checkpoint-only open must not catch up the version allocator above the
   checkpoint watermark unless the L7 allocator API has an explicit recovered
   version catch-up hook. If the final visible watermark is nonzero and no WAL
   records replayed, L8G must explicitly catch up the version allocator to that
   watermark before accepting normal commits.

## Allocator And Timestamp Bootstrap

The L7 replay runtime catches up the version allocator and timestamp guard for
each replayed WAL record. L8G must add only the missing checkpoint-only case:

1. after all replay, the next generated commit version must be greater than the
   final recovered visible version;
2. after replay, generated timestamps must not move below any replayed WAL
   timestamp;
3. if no WAL records replayed, timestamp guard catches up to the maximum
   timestamp in installed checkpoint rows when the checkpoint contains rows;
4. V1 has no transaction-id allocator and L8G must not add one;
5. allocator catch-up failure is a recovery failure and must not transition to
   `Open`.

If existing L7 APIs do not expose version-only catch-up cleanly from lifecycle,
add a narrow crate-private shell method instead of reaching into allocator
fields.

## Recovery Health Finalization

L8F reports health for input classification. L8G finalizes health after replay.

Rules:

1. `RecoveryHealth::Healthy` from L8F remains healthy only if replay/bootstrap
   succeeds;
2. `RecoveryHealth::Degraded` from L8F remains degraded and is preserved in the
   open outcome;
3. L8G replay/bootstrap failure in strict mode returns a typed lifecycle error;
4. L8G must not convert replay failure into `Healthy`;
5. timeline mismatch maps to `RecoveryFaultKind::TimelineMismatch` or a
   lower-layer commit error with source preservation;
6. visible publication failure after replay is durable-but-not-visible health
   debt and should preserve the L7 source chain;
7. maintenance readiness is `false` when health is degraded or unresolved
   durable state remains;
8. maintenance readiness may stay `false` for all durable opens until later
   maintenance slices wire executor policy.

## Open Runtime Surface

After L8G succeeds, the durable runtime should expose only storage-shaped
operations needed by later L8/L9:

1. `state() -> LifecycleState` returns `Open`;
2. `open_outcome() -> &StorageOpenOutcome`;
3. `read_view()` admits ordinary reads and returns an L6 read view capped by the
   visible tracker where the L7 read source requires it;
4. `execute_durable_commit(batch, generation_guard)` composes
   `CommitDurableRuntime` with the existing WAL service and commit fields;
5. commit execution must still honor the unresolved durable gate;
6. close may remain minimal if later close/maintenance slices own durable drain;
7. no public API is exposed from `storage-next`.

## Source Boundaries

L8G may import:

1. lifecycle-local shell/outcome/state types;
2. L7 commit replay, durable runtime, allocator, visible, and gate types;
3. L6 branch local state/read-view types through existing commit traits;
4. L4 WAL service only for normal durable commit execution after open;
5. shared core facts such as `CommitVersion` and `Timestamp`.

L8G must not import:

1. `crate::engine`, product primitive registries, public database APIs, or IPC;
2. StrataHub, remote sync, follower, or replica modules;
3. raw `std::fs`, `std::path::Path`, `std::env`, mmap, or process filesystem
   APIs;
4. table/format internals beyond types already carried in L8F handoff facts;
5. hardcoded layout path strings;
6. L4 snapshot/table/quarantine services for new recovery reads.

## Implementation Steps

### L8G-A: Bootstrap Types And Shell Access

1. Add the L8G module or durable-shell methods.
2. Add the final durable open runtime/report shape.
3. Add narrow shell accessors for allocator, visible tracker, branch, gate, and
   state transition if current accessors are insufficient.
4. Validate mode is `DurableLocalStandard` or `DurableLocalAlways`.

Exit gate: L8G can accept a shell plus L8F outcome without replaying yet.

### L8G-B: Replay Loop

1. Map storage mode to `CommitDurabilityClass`.
2. Iterate `recovery.wal().records()` in order.
3. Submit each record to `CommitReplayRuntime`.
4. Collect replay report counts.
5. Fail closed on replay error without transitioning lifecycle state.

Exit gate: WAL package rows become L6-visible through L7 replay.

### L8G-C: Checkpoint-Only Visible And Allocator Catch-Up

1. Compute final recovered visible version.
2. Publish checkpoint-only visibility when needed.
3. Catch up the version allocator to the final recovered visible version.
4. Preserve timestamp guard semantics when no trusted timestamp exists.

Exit gate: checkpoint-only recovery is readable and the next commit version is
above the recovered watermark.

### L8G-D: Health And Open Outcome

1. Preserve L8F health.
2. Add replay/bootstrap failure mapping.
3. Construct `StorageOpenOutcome`.
4. Transition `Recovering -> Open`.

Exit gate: durable open returns storage-shaped recovery facts and admitted
ordinary reads.

### L8G-E: Durable Runtime Operations

1. Add read-view access after open.
2. Compose post-open durable commits through `CommitDurableRuntime`.
3. Keep close minimal or explicitly defer durable close drain to later slices.

Exit gate: durable runtime can read recovered rows and execute one normal
durable commit after recovery.

### L8G-F: Testkit, Source Guard, Porting Log

1. Extend lifecycle testkit recovery/bootstrap counters.
2. Add `check_lifecycle_bootstrap_contract` so the generated/integration
   harness exercises `complete_recovery`, L7 replay, checkpoint catch-up,
   degraded-health propagation, and malformed replay rejection rather than only
   L8F recovery packaging.
3. Add direct unit tests and integration tests from the L8G test plan.
4. Extend source guards so L8G is allowed to call L7 replay but still blocked
   from product/raw IO/service-recovery drift.
5. Record shipped files, deferred items, sensitivity probes, and verification in
   `m4-l8-porting-log.md`.

## Edge Cases

1. Empty L8F recovery outcome opens with visible version zero.
2. Checkpoint-only recovery opens with visible version equal to checkpoint
   watermark.
3. Checkpoint plus WAL tail opens at the latest WAL commit version.
4. WAL package contains record equal to checkpoint watermark: should not happen
   from L8F; L8G should still be idempotent if rows are exact duplicates.
5. WAL package has records out of order: fail closed or sort only if explicitly
   documented. Preferred: fail closed.
6. WAL package has mixed branch ids: fail closed in V1.
7. Replay record missing timeline rows: fail closed.
8. Replay record has timeline row mismatch: fail closed.
9. Replay exact duplicate rows: success and no duplicate L6 rows.
10. Replay partial installed rows: fail closed.
11. Replay mismatch installed row: fail closed.
12. L6 apply failure after durable WAL package: record or preserve unresolved
    durable gate.
13. Visible publication failure after replay apply: durable-but-not-visible
    failure and gate preserved.
14. Matching unresolved durable gate is cleared only after visible publication.
15. Different unresolved durable gate blocks replay.
16. Degraded L8F health remains degraded after successful replay.
17. Strict L8F failure is never accepted by L8G.
18. DurableLocalAlways records use `CommitDurabilityClass::Always`.
19. DurableLocalStandard records use `CommitDurabilityClass::Standard`.
20. Cache/ObjectCandidate modes reject L8G bootstrap.
21. Next generated version after checkpoint-only recovery is greater than the
    recovered visible version.
22. Next generated timestamp after WAL replay is greater than or equal to the
    last recovered timestamp according to L7 timestamp-guard policy.

## Deferred

1. Multi-branch runtime maps and replay across branch maps: L9 or a later L8
   extension.
2. Table-backed checkpoint production and flushed table-state recovery: L8I/L8J.
3. Maintenance readiness policy beyond conservative `false`: L8H+.
4. Durable close drain/checkpoint-on-close: later lifecycle close slice.
5. Public product open API and primitive reconstruction: L9 and engine layer.
6. Remote/StrataHub recovery facts: post-core storage integration.
7. Fuzz targets and full generated fault windows: L8O/L8P closeout unless pulled
   forward.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::bootstrap
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If localfs is enabled and supported:

```bash
cargo test -p strata-storage-next --features localfs --locked --test lifecycle_recovery
```

## Closeout Checklist

L8G can close when:

1. durable shell plus L8F outcome transitions to an open durable runtime only
   after replay/bootstrap success;
2. empty, checkpoint-only, WAL-only, and checkpoint-plus-WAL recovery paths pass;
3. replay uses L7 `CommitReplayRuntime` and does not duplicate replay logic;
4. exact duplicate replay is idempotent and mismatch/partial replay fail closed;
5. version allocator, timestamp guard, and visible tracker are coherent after
   recovery;
6. unresolved durable gate matching and mismatch behavior is pinned;
7. recovery health and `StorageOpenOutcome` preserve raw storage facts;
8. post-open reads and one normal durable commit work through existing L6/L7
   surfaces;
9. source guards allow intended L7 replay and block product/raw IO drift;
10. porting log records tests, verification commands, and sensitivity probes.
