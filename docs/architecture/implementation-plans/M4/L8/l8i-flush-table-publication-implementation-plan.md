# L8I Implementation Plan: Flush Frozen State And Table Publication

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-test-plan.md`

Predecessor:
`docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`

## Objective

Implement the first concrete lifecycle maintenance handler: flushing branch
frozen mutable state into immutable table state.

L8I connects existing lower-layer pieces:

1. L6 owns active and frozen branch state plus replacement install validation;
2. L5 owns immutable table building and table facts;
3. L4 owns durable table-object publication and table-object read validation;
4. L8 owns lifecycle admission, maintenance execution, orchestration order,
   typed health facts, and partial-progress reporting.

The slice must make frozen rows durable as table objects before removing them
from L6 frozen state. It must not checkpoint, advance database manifest flush
watermarks, truncate WAL segments, run compaction, prune snapshots, quarantine
objects, or expose public maintenance commands. Those are later slices.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L5/m4-l5-table-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7h-cache-commit-path-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/maintenance.rs`
9. `crates/storage-next/src/lifecycle/cache.rs`
10. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
11. `crates/storage-next/src/lifecycle/outcome.rs`
12. `crates/storage-next/src/branch/state.rs`
13. `crates/storage-next/src/branch/read.rs`
14. `crates/storage-next/src/table/builder.rs`
15. `crates/storage-next/src/service/table.rs`
16. `crates/storage-next/src/service/manifest.rs`
17. `crates/storage-next/src/testkit/lifecycle/`

## Existing-Code Source Map

| Current file | L8I evidence | L8I action |
|---|---|---|
| `branch/state.rs` | `rotate_active`, `frozen`, `frozen_table_count`, and `replace_frozen_with_l0_table` already define the L6 frozen-to-owned install rule. | Use L6 as the authority. Do not mutate frozen vectors directly in lifecycle code. |
| `branch/read.rs` | `BranchOwnedTable::new` validates table facts, branch ids, and non-empty rows. | Build branch-owned table descriptors from L5/L4 facts and let L6 reject mismatches. |
| `table/builder.rs` | `ImmutableTableBuilder::build_from_frozen` builds deterministic table bytes/facts from a `FrozenTable`. | Use the builder; do not encode table bytes in L8. |
| `service/table.rs` | `TableObjectService::publish_create` validates table bytes, layout, capabilities, publish outcome, and object facts. `TableObjectReaderService` reopens published table objects. | Publish through L4 and reopen/validate before L6 install. |
| `service/manifest.rs` | Database manifest has `persist_flush_watermark`; table manifest service exists. | Do not update flush watermark in L8I. L8J owns global watermark and WAL retention. |
| `lifecycle/maintenance.rs` | Provides task queue, runner protocol, task ids, status, stats, health debt, and fault points. | Add a concrete flush runner/handler that returns `MaintenanceOutcome`. |
| `lifecycle/cache.rs` | Cache runtime owns L6/L7 state and maintenance executor without durable services. | Cache flush can replace frozen state in memory, but must not claim durable table publication. |
| `lifecycle/durable/bootstrap.rs` | Durable runtime owns L6/L7 state, table-object service, table reader service, and maintenance executor after recovery. | Durable flush publishes table object before replacing frozen state. |
| Old `segmented/mod.rs` | `flush_oldest_frozen` shows the old architecture's frozen-to-table boundary. | Port sequencing only; avoid old path/file/object naming and product callbacks. |

## Scope

L8I implements:

1. a crate-private flush module or runtime handler under `src/lifecycle/`;
2. a `FlushFrozenRequest` or equivalent explicit request type naming:
   - branch id;
   - optional frozen table index, defaulting to the oldest or documented target;
   - table identity seed;
   - table object id suffix;
   - table level target, initially L0 only;
   - mode-specific durability expectation;
3. a `FlushFrozenOutcome` or `MaintenanceOutcome` extension with:
   - task id;
   - branch id;
   - frozen index replaced;
   - row count;
   - table identity;
   - table object name when durable;
   - L5 table facts;
   - L4 publication facts when durable;
   - L6 install outcome;
   - health debt or deferred reason;
4. cache-mode flush behavior that builds an L5 table and installs it into L6
   without L4 durable publication or manifest claims;
5. durable-local flush behavior that:
   - snapshots the selected frozen table through L6 read-only facts;
   - builds table bytes through L5;
   - publishes immutable table object through L4;
   - reopens/validates the table object through L4/L5 reader service;
   - creates `BranchOwnedTable`;
   - calls `BranchLocalState::replace_frozen_with_l0_table`;
   - reports exact partial-progress facts;
6. idempotent/retry-aware behavior for publication-created-but-install-failed
   windows where the immutable object exists but frozen rows are still present;
7. source-chain preservation from L4 publish/read, L5 build/decode, and L6
   install errors;
8. maintenance task routing for `MaintenanceTaskKind::Flush`;
9. generated testkit counters for flush request, no-op/deferred flush, cache
   flush, durable flush, publication failure, install failure, and retry;
10. source guards preventing product, engine, raw filesystem, and architecture
    slice-label vocabulary from implementation/test code;
11. a porting-log entry after implementation.

L8I does not implement:

1. checkpoint object creation;
2. database manifest snapshot watermark updates;
3. database manifest flush watermark updates;
4. WAL truncation or `WalRetentionProof`;
5. branch/table manifest durable schema beyond what L4 already exposes;
6. compaction candidate selection;
7. inherited-layer materialization scheduling;
8. retention, quarantine, purge, or repair;
9. close drain policy beyond using the existing maintenance task policy;
10. public APIs, user maintenance commands, or product wording.

## Code Organization

Recommended files:

1. `crates/storage-next/src/lifecycle/flush.rs`
2. `crates/storage-next/src/lifecycle/tests/flush.rs`
3. `crates/storage-next/src/testkit/lifecycle/flush.rs`
4. optional shared helpers under `src/lifecycle/tests/flush/` if the direct
   test file approaches 1,000 lines.

Do not place the flush handler inside `maintenance.rs`. The executor remains
generic; flush is a concrete lifecycle handler.

Do not put architecture slice labels, milestone names, or parent-plan labels in
Rust code, test names, comments, fixture bytes, or panic strings. Keep that
vocabulary in the plan and porting log only.

## Type Surface

Names may change during implementation, but responsibilities should remain
stable.

```rust
pub(crate) struct FlushFrozenRequest {
    branch_id: BranchId,
    frozen_index: Option<usize>,
    table_identity_seed: FlushTableIdentitySeed,
    table_object_id: FlushTableObjectId,
}

pub(crate) struct FlushFrozenOutcome {
    status: FlushFrozenStatus,
    branch_id: BranchId,
    frozen_index: Option<usize>,
    rows_flushed: usize,
    table_identity: Option<TableIdentity>,
    table_object: Option<ObjectName>,
    install_outcome: Option<BranchImmutableInstallOutcome>,
    health: Option<RecoveryHealth>,
}

pub(crate) enum FlushFrozenStatus {
    Completed,
    DeferredNoFrozenState,
    DeferredUnsupportedMode,
    PublishedNotInstalled,
    Failed,
}

pub(crate) trait FlushTablePublisher {
    fn publish_table(
        &mut self,
        request: &FlushFrozenRequest,
        artifact: &BuiltTableArtifact,
    ) -> LifecycleResult<PublishedFlushTable>;
}
```

The implementation can use direct runtime methods instead of a trait if that
fits the existing lifecycle runtime better. Tests still need a deterministic
fault seam for the publication and install windows.

## Flush Protocol

Target durable-local sequence:

```text
require lifecycle Open
select branch frozen table
if none: return deferred/no-op outcome
copy/snapshot frozen rows without mutating branch state
derive deterministic table identity and object id
build immutable table artifact through L5
publish table object through L4 create
reopen table object through L4 table reader service
construct BranchOwnedTable from descriptor and reader
replace selected frozen table through L6
return maintenance outcome with publish/install facts
```

Target cache-mode sequence:

```text
require lifecycle Open
select branch frozen table
if none: return deferred/no-op outcome
build immutable table artifact through L5
open reader from built bytes
construct BranchOwnedTable
replace selected frozen table through L6
return maintenance outcome with cache-only facts
```

Cache mode must never call table-object service, manifest service, checkpoint
service, WAL service, quarantine service, raw filesystem APIs, or object-layout
path construction directly.

## Frozen Table Selection

Rules:

1. V1 flushes one frozen table per task.
2. If the request names an index, it must exist at execution time.
3. If the request omits an index, choose the oldest frozen table by documented
   L6 ordering.
4. Empty active state alone is not a flush candidate.
5. Active rows may be rotated into frozen state by a separate maintenance policy
   only if explicitly implemented; L8I should not implicitly rotate active rows
   unless the handler name and tests make that behavior explicit.
6. If the selected frozen table disappears before execution because another
   flush already replaced it, return an idempotent deferred/no-op outcome rather
   than failing with a misleading install error.

The chosen ordering must be pinned by tests. If L6 stores newest frozen at index
0, "oldest" means the highest valid index.

## Identity And Object Naming

L8I must mint deterministic identities without using reserved layout literals in
source or tests.

Rules:

1. `TableIdentity` must include enough source facts to avoid collisions across
   branch id, frozen generation/index, commit range, and retry.
2. Table object id should be deterministic for a given frozen table and flush
   attempt, but retry behavior must not wedge if an object already exists and
   matches the expected bytes/facts.
3. Collision with existing reachable L6 table identity must fail before
   replacing frozen state.
4. Table object creation collision should be classified:
   - exact same object facts: treat as retry candidate;
   - different bytes/facts: fail closed with typed health debt.
5. L8I should not encode branch/table manifest naming policy. It should use L4
   table-object service for layout.

## Failure Windows

The implementation must preserve state facts for these windows:

1. no frozen table exists;
2. selected frozen index no longer exists;
3. L5 table build fails;
4. table object publish fails before object creation;
5. table object publish returns invalid metadata;
6. table object already exists with identical facts;
7. table object already exists with conflicting facts;
8. table object published, then reopen/read validation fails;
9. table object published and validated, then `BranchOwnedTable::new` fails;
10. table object published and validated, then L6 install fails;
11. L6 install succeeds but outcome publication/stats construction fails;
12. cache-mode build succeeds but install fails;
13. flush task is canceled before start;
14. flush task is deferred because lifecycle is closing or recovery health is
    unsafe.

For windows 8-10, durable state may contain an unreferenced immutable table
object. L8I must report that as health debt and leave cleanup to retention or
quarantine slices. It must not delete the object without a retention proof.

## Recovery And Idempotency

L8I does not implement durable recovery for newly published table objects, but
it must produce facts that later recovery/reclaim can understand.

Rules:

1. If publish succeeded and install did not, frozen state remains authoritative
   in memory.
2. If retry sees the same frozen rows and an existing matching object, it may
   reuse the object and continue to L6 install.
3. If retry sees frozen rows already replaced by a matching L0 table, report an
   idempotent completed/deferred outcome.
4. If retry sees a matching object but L6 frozen rows differ, fail closed.
5. If process crashes after object publication before manifest/watermark
   publication, L8F/L8M/L8L later decide whether the object is reachable,
   retained, or quarantined. L8I only records the window.

## Flush Watermark

L8I may compute candidate flush coverage facts, but it must not persist the
global flush watermark.

Rules:

1. A successful flush may report `commit_max` for the table it installed.
2. The global flush watermark is not simply the max installed table commit.
3. Branch absence must never advance the watermark.
4. L8J owns manifest `persist_flush_watermark` and WAL retention proof.
5. L8I tests should assert that manifest flush watermark is unchanged.

## Maintenance Integration

Flush task handling should use the executor from L8H:

1. `MaintenanceTaskKind::Flush` maps to the flush handler.
2. Coalescing remains kind + branch scope.
3. If a flush task runs and no frozen state exists, return
   `MaintenanceOutcomeStatus::Deferred` with no health debt.
4. If a flush task fails after publication, return failed maintenance outcome
   with telemetry or policy health debt and table object facts.
5. Queue admission remains lifecycle-owned; the flush handler must not bypass
   executor admission.

## Error Mapping

Map lower-layer errors into `LifecycleError` without discarding sources:

1. L5 build/decode errors preserve `TableRuntimeError` source chains.
2. L4 table publish errors preserve `TableObjectServiceError` source chains.
3. L4 table read/reopen errors preserve `TableObjectReadError` source chains.
4. L6 install errors preserve `BranchRuntimeError` source chains.
5. No error path should rely on display-string matching.
6. Tests should assert `LifecycleError::code()` and source presence where
   relevant.

If the current `LifecycleError` variants are too coarse, add typed lifecycle
variants now rather than encoding flush failures in generic strings.

## Implementation Steps

### L8I-A: Flush Facts And Request Types

1. Add the flush module.
2. Add request/outcome/fact types.
3. Add validation for branch id, frozen index, identity seed, and object id.
4. Add outcome conversion into `MaintenanceOutcome`.

### L8I-B: Cache Flush Handler

1. Add cache-runtime method to enqueue/run flush through the executor.
2. Build table from frozen rows through L5.
3. Open reader from built bytes.
4. Call `replace_frozen_with_l0_table`.
5. Return cache-only flush facts with no durable object or manifest claim.

### L8I-C: Durable Flush Handler

1. Add durable-runtime method to enqueue/run flush through the executor.
2. Snapshot selected frozen rows.
3. Build L5 table artifact.
4. Publish with `TableObjectService::publish_create`.
5. Reopen with `TableObjectReaderService`.
6. Construct `BranchOwnedTable`.
7. Install through L6 replacement API.
8. Return durable publication and install facts.

### L8I-D: Fault Windows

1. Add deterministic fault hooks around build, publish, reopen, branch-owned
   construction, and L6 install.
2. Preserve state and sources on each failure.
3. Add retry/idempotency helpers for exact matching existing table object.

### L8I-E: Testkit And Source Guards

1. Add generated flush scripts and counters.
2. Add direct unit tests for cache/durable handlers.
3. Add integration tests in `tests/lifecycle_maintenance.rs`.
4. Extend source guards for cache-mode durable-service absence and no
   architecture labels in implementation/test code.
5. Update the porting log after implementation.

## Edge Cases

1. zero frozen tables;
2. one frozen table;
3. multiple frozen tables with oldest/newest ordering;
4. frozen table at index 0;
5. frozen table at last index;
6. active rows present but no frozen rows;
7. tombstone-only frozen table;
8. expired-row facts in frozen table;
9. many rows near table builder limits;
10. table identity collision with existing reachable L6 table;
11. object create collision with matching bytes;
12. object create collision with different bytes;
13. publish success followed by install failure;
14. install success followed by stats/outcome failure;
15. cache mode accidentally attempting durable publish;
16. durable mode missing table publish capability;
17. flush after close rejected by lifecycle admission;
18. duplicate pending flush coalesces before execution;
19. repeated flush after prior success reports no frozen state;
20. manifest flush watermark remains unchanged.

## Deferred To Later Slices

1. Global flush watermark persistence: L8J.
2. WAL truncation after flush proof: L8J.
3. Checkpoint object publication: L8J.
4. Compaction and materialization scheduling: L8K.
5. Retention of orphaned published table objects: L8L.
6. Quarantine/purge/repair for orphaned or corrupt table objects: L8M.
7. Full close drain/sync policy: L8N.
8. Crash/reopen proof for all publication windows: L8O.
9. Final conformance inventory: L8P.

## Verification Commands

After implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

When `cargo-hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Exit Criteria

L8I can close when:

1. cache and durable flush both use the same lifecycle maintenance task shape;
2. durable flush publishes immutable table objects before L6 replacement;
3. cache flush never imports or calls durable services;
4. all lower-layer failures preserve typed source chains;
5. publication-success/install-failure is observable and retry-safe;
6. reads before and after successful flush are equivalent;
7. manifest flush watermark and WAL truncation are untouched;
8. generated properties cover input-derived flush cases;
9. source guards prevent scope drift and architecture-label leakage;
10. the porting log records shipped files, commands, and sensitivity probes.
