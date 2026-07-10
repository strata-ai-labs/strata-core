# L8F Implementation Plan: Recovery Orchestration

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-test-plan.md`

## Objective

Implement the durable-local recovery orchestration step.

L8F starts from the `LifecycleDurableLocalShell` assembled by L8E. It reads and
classifies durable recovery inputs in the only safe order: manifest facts,
checkpoint/snapshot facts, table object facts referenced by recovered storage
metadata, WAL records after the durable watermark, and quarantine inventory
facts. It installs checkpoint rows into L6 through the L6 snapshot-install API
when a row-native checkpoint is present, and it packages unreplayed WAL records
for L8G.

L8F does not replay WAL records through L7, advance the visible version, catch
up L7 allocators, finalize `StorageOpenOutcome`, start maintenance, mutate
quarantine state, or call product primitive reconstruction. L8G owns commit
runtime bootstrap from the recovered WAL package.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
5. `crates/storage-next/src/lifecycle/durable.rs`
6. `crates/storage-next/src/lifecycle/health.rs`
7. `crates/storage-next/src/lifecycle/facts.rs`
8. `crates/storage-next/src/service/manifest.rs`
9. `crates/storage-next/src/service/snapshot.rs`
10. `crates/storage-next/src/service/wal.rs`
11. `crates/storage-next/src/service/table.rs`
12. `crates/storage-next/src/service/quarantine.rs`
13. `crates/storage-next/src/branch/state.rs`
14. `crates/storage-next/src/commit/replay.rs`
15. `crates/storage-next/src/format/snapshot.rs`
16. `crates/storage-next/src/format/wal.rs`
17. `crates/storage-next/src/row/`
18. `crates/storage-next/src/testkit/lifecycle/`
19. `crates/storage-next/tests/lifecycle_properties.rs`
20. `crates/storage-next/tests/lifecycle_source_guard.rs`
21. `crates/engine/src/database/recovery.rs`

## Existing-Code Source Map

| Current file | L8F evidence | L8F action |
|---|---|---|
| `lifecycle/durable.rs` | L8E returns a `LifecycleDurableLocalShell` in `Recovering` with service handles and manifest facts. | Add recovery methods over the shell rather than constructing a second service bundle. |
| `lifecycle/facts.rs` | `StorageOpenPlan` already carries `RecoveryStrictness` and `LifecycleLossyRecoveryPolicy`. | Use the plan policy to decide failed versus degraded recovery. |
| `lifecycle/health.rs` | `RecoveryHealth`, `RecoveryFault`, and fault kinds exist. | Extend only where needed for precise L8F classifications, and keep health product-neutral. |
| `service/snapshot.rs` | `SnapshotService::load_required_for_codec` validates snapshot object, id, database id, and codec id; `visit_sections` can stream section validation. | Use the service to load manifest-listed snapshots. Do not decode snapshot bytes by hand. |
| `service/wal.rs` | `WalService::read_after_commit_version` returns decoded records and optional latest-tail truncation; `repair_latest_tail` repairs only the active segment. | Use manifest-derived watermarks to choose the replay start; preserve WAL records for L8G; repair only documented latest-tail truncations. |
| `service/table.rs` | `TableObjectReaderService::open_reader` validates table object bytes against `TableObjectFacts`. | Validate any table object facts referenced by recovered storage metadata. |
| `service/quarantine.rs` | `QuarantineService::load_inventory` treats missing inventory as healthy empty but detects corrupt/identity-mismatched inventory. | Load inventory and classify mismatches; do not purge or rewrite inventory in L8F. |
| `branch/state.rs` | `install_snapshot_rows_into_branches` installs row-native checkpoint rows through L6 validation and table building. | Use this API for checkpoint rows rather than appending directly to branch internals. |
| `commit/replay.rs` | `CommitReplayRuntime` replays already-durable WAL records, catches up allocators/timestamps, publishes visible facts, and reconciles unresolved gates. | Do not call this in L8F. Package WAL records for L8G. |
| Old engine recovery code | Old recovery performed manifest, checkpoint, WAL, and primitive reconstruction in one path. | Port only storage ordering and classification. Product reconstruction stays above L8. |

## Scope

L8F implements:

1. a crate-private `lifecycle::recovery` module;
2. a recovery runtime that accepts `&mut LifecycleDurableLocalShell`;
3. recovery request/fact shapes for strict and explicitly lossy recovery;
4. admission through `LifecycleDurableLocalShell::admit_recovery_step`;
5. checkpoint-derived durable watermark calculation;
6. manifest snapshot fact validation before any WAL skip decision;
7. snapshot load through `SnapshotService`;
8. row-native checkpoint section decoding into `StorageRow` groups;
9. checkpoint install through `install_snapshot_rows_into_branches`;
10. table object validation for recovered table facts, when such facts are
    present in storage-owned recovery metadata;
11. WAL read after the selected recovered checkpoint watermark;
12. latest-segment partial-tail handling through `WalService::repair_latest_tail`;
13. strict failure for non-latest WAL corruption;
14. optional lossy fallback classification when explicitly enabled by the open
    plan;
15. quarantine inventory load and mismatch classification;
16. a recovery package containing checkpoint facts, unreplayed WAL records,
    WAL repair facts, quarantine facts, table validation facts, and health
    facts for L8G;
17. source-chain-preserving error mapping for snapshot, WAL, table, branch, and
    quarantine failures;
18. generated/testkit counters for empty recovery, checkpoint recovery,
    checkpoint plus WAL tail, WAL repair, strict failure, lossy degradation,
    quarantine mismatch, table validation, and no product callback;
19. source guards preventing product/engine/import drift and preventing L8F
    from calling L7 replay;
20. an L8F porting-log entry after implementation.

L8F does not implement:

1. public L9 open APIs;
2. final `StorageOpenOutcome` publication;
3. L7 `CommitReplayRuntime` invocation;
4. L7 allocator catch-up;
5. L7 visible-version publication;
6. L7 timeline pair validation;
7. unresolved durable gate reconciliation;
8. checkpoint creation or manifest checkpoint publication;
9. flush, compaction, materialization, retention, purge, or repair scheduling;
10. quarantine object mutation, inventory rewrite, purge, or retention;
11. product primitive reconstruction, registry wiring, IPC, follower behavior,
    StrataHub sync, or cloud callbacks;
12. background tasks or maintenance executor behavior;
13. broad lossy-recovery heuristics without an explicit open-plan policy.

## Type Surface

Names may change during implementation, but the responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleRecoveryRuntime<'a, S> {
    shell: &'a mut LifecycleDurableLocalShell<'a, S>,
}

pub(crate) struct LifecycleRecoveryRequest {
    strictness: RecoveryStrictness,
    max_faults: usize,
    max_snapshot_sections: usize,
    checkpoint_identity_seed: TableIdentity,
}

pub(crate) struct LifecycleRecoveryOutcome {
    health: RecoveryHealth,
    checkpoint: LifecycleRecoveredCheckpoint,
    wal: LifecycleRecoveredWal,
    quarantine: LifecycleRecoveredQuarantine,
    tables: LifecycleRecoveredTables,
}

pub(crate) struct LifecycleRecoveredWal {
    replay_start: CommitVersion,
    records: Vec<WalRecord>,
    truncation: Option<WalTruncation>,
    repair: Option<WalRepair>,
}
```

The recovery outcome is not a public API. It is the handoff from L8F to L8G.
L8G consumes the WAL records, applies L7 replay, catches up clocks, publishes
visible facts, validates timeline facts, and produces the final open outcome.

## Recovery Ordering

The recovery runtime must run the phases below in order.

1. Validate request and shell state.
2. Admit a recovery step.
3. Read L8E assembly facts from the shell.
4. Validate manifest snapshot facts:
   - snapshot id and snapshot watermark must either both be present or both be
     absent;
   - snapshot id `0` is invalid;
   - snapshot watermark must be a valid commit-version fact.
5. Load the manifest-listed snapshot when present.
6. Validate snapshot identity against manifest database id, codec id, and
   snapshot id.
7. Validate snapshot header watermark equals the manifest snapshot watermark.
8. Decode storage-owned checkpoint sections.
9. Install checkpoint rows through L6 snapshot install.
10. Validate any table object facts referenced by checkpoint sections or table
    recovery metadata.
11. Compute the WAL replay start from durable watermarks.
12. Read WAL records after the replay start.
13. Repair a documented latest-segment truncation, or classify it as failed or
    degraded.
14. Load quarantine inventory for the configured initial branch.
15. Aggregate health and recovery facts.
16. Return the recovery package while the shell remains `Recovering`.

No phase may call L7 replay. No phase may expose the runtime as `Open`.

## Watermark Rules

L8F selects the WAL replay start from recovered state only.

Inputs:

1. manifest snapshot watermark, if a snapshot id is present and the snapshot
   object loaded and validated;
2. `CommitVersion::ZERO` otherwise.

Rules:

1. Use the trusted checkpoint watermark when present.
2. Do not use `active_wal_segment` as a commit-version watermark.
3. Do not use an unloaded or failed snapshot watermark to skip WAL records.
4. If a manifest-listed snapshot is missing in strict mode, fail before
   computing a replay start from that snapshot watermark.
5. If lossy recovery is explicitly allowed and snapshot loss is downgraded, the
   replay start must fall back to zero unless another recovered state source
   exists.
6. WAL records with `commit_version <= replay_start` are not packaged for L8G.
7. WAL records with `commit_version > replay_start` are preserved unchanged.
8. A manifest flush watermark greater than the recovered checkpoint watermark
   requires flushed table-state recovery, which is not implemented in L8F. L8F
   must fail closed instead of using that watermark to skip WAL records.

The replay-start calculation must be a dedicated helper with direct unit tests.

## Snapshot And Checkpoint Recovery

The manifest may name a snapshot checkpoint. L8F owns loading and validating
that snapshot.

Rules:

1. Missing snapshot when the manifest names one is a failure in strict mode.
2. Wrong database id is a failure.
3. Wrong codec id is a failure.
4. Snapshot id mismatch is a failure.
5. Header watermark mismatch is a failure.
6. A zero-section snapshot is a valid empty checkpoint.
7. Unknown non-critical checkpoint telemetry sections may become telemetry
   degradation only if the section type is explicitly documented as optional.
8. Unknown required checkpoint sections fail closed.
9. Section decoding must be bounded by `max_snapshot_sections`.
10. Row-native checkpoint sections must decode to `StorageRow` values without
    product primitive reconstruction.
11. Decoded checkpoint rows must be installed through
    `BranchSnapshotInstallRequest::from_rows` and
    `install_snapshot_rows_into_branches`.
12. L8F must not append snapshot rows directly to active/frozen/owned L6
    internals.
13. Branch/table install errors map to branch-runtime lower-layer errors with
    source preservation.

If the snapshot format needs storage-owned section kinds, add them to L8F as
storage vocabulary only. Do not introduce product schema, primitive ids, or
record DTO reconstruction.

## Table Object Recovery

L8F validates table objects only when recovered storage metadata references
them. The initial V1 checkpoint path may remain row-native, but the API should
be ready for table-backed checkpoint sections from L8J.

Rules:

1. A referenced table object is required unless policy explicitly permits
   degraded recovery for that reference kind.
2. Object name and `TableObjectFacts` must come from storage metadata, not
   hardcoded path strings.
3. Validation must use `TableObjectReaderService::open_reader`.
4. Missing required table object is `MissingTableObject`.
5. Corrupt table object maps to table/runtime or format lower-layer source.
6. The validated table identity and commit range are recorded in recovery
   facts.
7. L8F does not compact, rewrite, publish replacement tables, or mutate L6
   table levels outside the L6 checkpoint install API.

## WAL Recovery

L8F reads the WAL after the selected durable watermark.

Rules:

1. Use `WalService::read_after_commit_version(replay_start)`.
2. Do not manually parse WAL segment names or bytes.
3. Preserve record order returned by L4.
4. Preserve each record's branch id, commit version, timestamp, and payload
   rows exactly.
5. If `WalRead::truncation()` is absent, package records for L8G unchanged.
6. If truncation is present for the active/latest segment, call
   `repair_latest_tail` before open can finish.
7. A successful repair records `WalRepair` facts and may remain healthy if no
   committed record was lost.
8. Repair failure in strict mode fails recovery.
9. Corruption in a non-latest segment fails strict recovery.
10. Explicit lossy fallback may downgrade only documented tail loss. It must
    never report `Healthy` after data loss or policy downgrade.
11. WAL read/repair errors preserve the `WalServiceError` source chain.

L8F packages WAL records only. L8G decides replay action, idempotence,
allocator catch-up, timeline validation, visible-version publication, and
unresolved-durable-gate reconciliation.

## Quarantine Recovery

L8F loads quarantine inventory as a health input.

Rules:

1. Use `QuarantineService::load_inventory`.
2. Missing inventory is healthy empty inventory only for the inventory object
   itself.
3. Corrupt inventory is `QuarantineInventoryMismatch`.
4. Database id, branch id, and codec id mismatch are
   `QuarantineInventoryMismatch`.
5. Inventory load errors preserve source chains.
6. L8F must not rewrite inventory.
7. L8F must not purge quarantine objects.
8. L8F must not treat quarantine mismatch as `Healthy`.
9. L8M owns quarantine repair, purge, and mutation.

## Health Classification

L8F should keep classification deterministic and conservative.

Strict recovery:

1. corrupt manifest/snapshot/WAL/table/quarantine required input fails;
2. missing required snapshot or table object fails;
3. codec mismatch fails;
4. non-latest WAL corruption fails;
5. latest-tail repair is not attempted and returns
   `LifecycleError::WalTailRepairRejected`;
6. repair failure fails;
7. data loss never reports healthy.

Explicit lossy recovery:

1. only documented lossy cases may become `RecoveryHealth::Degraded`;
2. degraded recovery requires at least one `RecoveryFault`;
3. confirmed snapshot/table/WAL-tail data loss uses
   `RecoveryDegradationClass::DataLoss`;
4. quarantine inventory mismatch uses
   `RecoveryDegradationClass::Telemetry`;
5. policy downgrade uses `RecoveryDegradationClass::PolicyDowngrade` only
   for non-lossy policy relaxation cases;
6. codec mismatch and identity mismatch still fail;
7. data-loss degradation blocks later unsafe maintenance until a later owner
   slice makes a deliberate decision.

Ordering rule: checkpoint decode, flush-watermark validation, quarantine load,
table-object validation, and health-fault capacity checks must all run before
any WAL tail repair that can mutate durable bytes.

Add or refine `RecoveryFaultKind` only when an existing kind would collapse a
contractually distinct case. Likely additions include snapshot missing versus
manifest missing, corrupt table object, WAL tail repair failure, and codec
mismatch if the existing vocabulary is not precise enough.

## Source-Chain Policy

L8F must preserve lower-layer causes.

Required mappings:

1. snapshot load/decode/identity errors -> `LifecycleLowerLayer::Service`;
2. table reader errors -> `LifecycleLowerLayer::TableRuntime` or `Service`,
   depending on the source;
3. WAL read/repair errors -> `LifecycleLowerLayer::Service`;
4. branch snapshot install errors -> `LifecycleLowerLayer::BranchRuntime`;
5. quarantine load/decode errors -> `LifecycleLowerLayer::Service`;
6. storage format section decode errors -> `LifecycleLowerLayer::Format`;
7. backend IO faults wrapped by services must remain reachable through
   `Error::source()`.

Do not convert these into only static strings.

## Source Boundaries

L8F may import:

1. lifecycle-local types;
2. L4 services and service errors;
3. L3 format types needed for snapshot/WAL section facts;
4. L2 layout/object types only through existing services unless a recovery
   section type explicitly stores an object name;
5. L6 branch snapshot install APIs;
6. L7 fact types such as `CommitVersion` through shared core types.

L8F must not import:

1. `crate::engine`, `crate::primitive`, product registries, or public database
   APIs;
2. StrataHub modules or remote sync vocabulary;
3. follower/replica paths;
4. raw `std::fs`, `std::path::Path`, `std::env`, or process filesystem APIs;
5. `CommitReplayRuntime` or other L7 replay execution surfaces;
6. table internals beyond public crate-private table reader/fact APIs;
7. hardcoded reserved layout path literals.

## Implementation Steps

### L8F-A: Module And Fact Shapes

1. Add `crates/storage-next/src/lifecycle/recovery.rs`.
2. Export crate-private recovery types from `lifecycle/mod.rs`.
3. Add recovery request, policy, outcome, checkpoint, WAL, table, and quarantine
   fact structs.
4. Add precise fault kinds if the existing `RecoveryFaultKind` vocabulary is
   too coarse.
5. Add validation for nonzero limits and strict/lossy policy consistency.

### L8F-B: Recovery Runtime Shell Integration

1. Add a recovery runtime over `&mut LifecycleDurableLocalShell`.
2. Require `LifecycleState::Recovering`.
3. Call `admit_recovery_step` before doing work.
4. Add controlled mutable access needed for checkpoint install without exposing
   branch internals broadly.
5. Prove ordinary read/commit/maintenance admission remains rejected.

### L8F-C: Manifest Watermark And Snapshot Load

1. Read manifest facts from `LifecycleDurableAssemblyFacts`.
2. Validate snapshot id/watermark pair.
3. Load manifest-listed snapshot through `SnapshotService`.
4. Validate id, database id, codec id, and watermark.
5. Decode storage-owned checkpoint sections.
6. Record zero-section snapshot as empty checkpoint.

### L8F-D: Checkpoint Install And Table Validation

1. Convert row-native checkpoint rows into `BranchSnapshotInstallRequest`.
2. Install rows through `install_snapshot_rows_into_branches`.
3. Record install outcome facts.
4. Validate referenced table objects through `TableObjectReaderService`.
5. Classify missing/corrupt table object failures.

### L8F-E: WAL Tail Package And Repair

1. Compute the trusted replay start.
2. Call `WalService::read_after_commit_version`.
3. Package returned records for L8G.
4. Invoke `repair_latest_tail` for latest-segment truncation.
5. Record repair facts.
6. Classify corruption and repair failure by strictness.

### L8F-F: Quarantine Inventory And Health Aggregation

1. Load initial-branch quarantine inventory.
2. Record inventory presence, byte count, and entry count.
3. Classify corrupt/mismatched inventory.
4. Aggregate all faults into `RecoveryHealth`.
5. Return a deterministic recovery package.

### L8F-G: Testkit, Source Guard, Porting Log

1. Add generated lifecycle recovery counters.
2. Add direct unit tests and integration tests listed in the L8F test plan.
3. Extend source guards for no product imports and no L7 replay calls.
4. Update `m4-l8-porting-log.md` with shipped files, deferred work, tests,
   sensitivity probes, and verification commands.

## Edge Cases

1. Empty new database: no snapshot, no WAL records, empty quarantine inventory.
2. Existing manifest with snapshot id but no watermark.
3. Existing manifest with watermark but no snapshot id.
4. Snapshot id `0`.
5. Snapshot object missing.
6. Snapshot wrong database id.
7. Snapshot wrong codec id.
8. Snapshot watermark lower than manifest watermark.
9. Snapshot watermark higher than manifest watermark.
10. Zero-section snapshot.
11. Snapshot with too many sections.
12. Snapshot row section with unsorted rows.
13. Snapshot row section with duplicate internal keys.
14. Snapshot row section with wrong branch id under strict target policy.
15. Missing required table object.
16. Corrupt table object CRC.
17. WAL empty after checkpoint watermark.
18. WAL contains only records before checkpoint watermark.
19. WAL contains records before and after checkpoint watermark.
20. WAL latest partial tail.
21. WAL non-latest corruption.
22. WAL repair stale object-size precondition.
23. WAL repair active-segment mismatch.
24. WAL record branch ids across multiple branches.
25. Timeline rows present in WAL payload.
26. Timeline rows missing from WAL payload, leaving L8G to reject.
27. Quarantine inventory missing.
28. Quarantine inventory corrupt.
29. Quarantine inventory wrong branch id.
30. Quarantine inventory wrong database id.
31. Quarantine inventory wrong codec id.
32. Strict mode versus explicitly lossy mode for each degradable case.

## Exit Criteria

L8F is complete when:

1. durable shell recovery can produce an empty healthy package;
2. manifest-listed snapshot recovery loads and validates snapshot identity;
3. checkpoint rows install through L6 snapshot install;
4. WAL records after the trusted watermark are packaged for L8G unchanged;
5. latest WAL partial tail repair is handled through L4 only under explicit
   lossy recovery;
6. strict recovery rejects latest WAL partial tail repair;
7. non-latest WAL corruption fails strict recovery;
8. explicit lossy fallback never reports `Healthy` after data loss;
9. quarantine inventory mismatch is classified;
10. table object missing/corruption is classified when referenced;
11. lower-layer source chains are preserved;
11. L8F does not call L7 replay or product callbacks;
12. lifecycle source guards pass;
13. generated lifecycle recovery counters cover the required categories;
14. the porting log records shipped behavior and deferred items.

## Deferred To Later Slices

| Deferred item | Owner | Reason |
|---|---|---|
| L7 WAL replay and allocator/timestamp catch-up | L8G | L8F packages records only. |
| Visible-version publication and final open outcome | L8G | Must happen after replay and timeline validation. |
| Timeline semantic validation | L8G | Timeline rows are L7 commit-runtime facts. |
| Unresolved durable gate reconciliation | L8G | Gate semantics live in L7 replay. |
| Flushed table-state recovery from manifest flush watermark | L8I/L8J | L8F cannot safely use a flush watermark until it has recovered the table state that proves the watermark. |
| Multi-branch checkpoint installation into a runtime branch map | L8G/L9 | The current durable shell owns one open branch state. L8F fails closed on checkpoint rows for unopened branches. |
| Checkpoint creation and manifest checkpoint publication | L8J | L8F reads checkpoints; L8J writes them. |
| Flush and table publication | L8I | L8F only validates recovered table references. |
| Compaction/materialization scheduling | L8K | Maintenance owner slice. |
| Retention, quarantine mutation, purge, and repair orchestration | L8L-L8M | L8F reads inventory only. |
| Durable close, drain, and writer guard release | L8N | L8E owns guard lifetime until close exists. |
| Public open/read/commit API | L9 | L8 remains crate-private. |

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
