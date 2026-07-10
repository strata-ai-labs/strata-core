# L8K Implementation Plan: Compaction And Materialization Scheduling Hooks

Status: implemented for the conservative V1 table-rewrite scope

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`

## Objective

Implement lifecycle-owned scheduling hooks for branch table compaction and
inherited-layer materialization.

This slice connects existing lower-layer mechanics:

1. L6 owns branch compaction candidates, branch table replacement validation,
   inherited-layer materialization handles, and row rewrite semantics.
2. L5 owns immutable table compaction, table building, output splitting, and
   compaction reports.
3. L4 owns durable table-object publication and object validation.
4. L8 owns maintenance task routing, operation admission, mode-specific
   orchestration, health debt, storage pressure facts, and retry/defer facts.

The slice must not reimplement L5 merge semantics or L6 visibility semantics.
It should make lifecycle able to decide when compaction/materialization work is
safe to attempt, run that work through the lower-layer APIs, and report exactly
what happened.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l5-table-runtime.md`
4. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
11. `crates/storage-next/src/lifecycle/maintenance.rs`
12. `crates/storage-next/src/lifecycle/cache.rs`
13. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
14. `crates/storage-next/src/lifecycle/outcome.rs`
15. `crates/storage-next/src/branch/state.rs`
16. `crates/storage-next/src/branch/read.rs`
17. `crates/storage-next/src/table/compaction.rs`
18. `crates/storage-next/src/service/table.rs`
19. `crates/storage-next/src/testkit/lifecycle/`

## Existing-Code Source Map

| Current file | Evidence | L8K action |
|---|---|---|
| `lifecycle/maintenance.rs` | `MaintenanceTaskKind::{Compaction, Materialization}` and matching scopes already exist, but there are no request constructors or concrete runners. | Add explicit task requests and route them through runtime-owned compaction/materialization handlers. |
| `branch/state.rs` | `BranchCompactionRequest`, `plan_branch_compaction`, `install_branch_compaction_plan`, and `compact_branch_owned_tables` already own candidate selection, stale-candidate checks, table merge invocation, output validation, and branch state replacement. | Treat L6 as the compaction authority. Lifecycle may request work and report facts, but must not inspect rows to choose merge inputs. |
| `branch/state.rs` | `BranchMaterializationHandle`, `mark_inherited_layer_materializing`, `BranchMaterializationRequest::from_handle`, and `materialize_inherited_layer` already define intent, handle binding, retry, and atomic layer replacement. | Use handle-based materialization. Do not create materialization requests from a naked layer index in lifecycle handlers. |
| `table/compaction.rs` | `TableCompactor`, `TableCompactionConfig`, and `TableCompactionReport` own generic table merge behavior. | L8 should surface reports from L6/L5, not create its own merge policy. |
| `service/table.rs` | `TableObjectService::publish_create` and `TableObjectReaderService::open_reader` validate table bytes and object facts. | Deferred for compaction/materialization until table-manifest recovery exists. V1 reports checkpoint debt instead of standalone table-object reachability. |
| `lifecycle/flush.rs` | Flush already shows the durable publication pattern: build table, publish object, reopen/validate, then call L6 install. | Reuse the same partial-progress discipline for any durable compaction/materialization output publication. |
| `lifecycle/checkpoint.rs` | Checkpoint is the current recovery-valid way to persist branch state after volatile table rewrites. | Compaction/materialization must not claim replay shortening or manifest flush coverage. Checkpoint remains the durable recovery boundary. |
| Old `crates/storage/src/segmented/compaction.rs` | Shows branch compaction selection, level movement, overlap expansion, and score-based scheduling. | Port only storage scheduling evidence and pressure thresholds. Candidate choice stays with L6. |
| Old `crates/storage/src/segmented/tests/publish_failures.rs` | Exercises publish failure windows for compaction and materialization. | Port fault windows as typed lifecycle health/outcome tests rather than old manifest/path-specific behavior. |
| Old `crates/storage/src/test_hooks.rs` | Provides install-pause hooks around compaction/materialization I/O and atomic install. | Port deterministic fault seams through maintenance hooks and fake publishers, not global pause hooks. |
| Old `crates/engine/src/database/compaction.rs` | User-triggered and background compaction lived above storage. | Public commands and product wording stay above L8. This slice only adds storage lifecycle hooks. |

## Old Codebase Porting Map

The old implementation is reference material for sequencing, invariants, and
failure windows. It is not the API shape for storage-next.

| Old file / function | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/segmented/compaction.rs::compute_compaction_scores` | Computes storage pressure from per-branch level facts, especially L0 backlog and level byte pressure. | Port the idea as storage pressure facts. Do not port exact byte targets, product stall thresholds, or scheduling thread policy yet. | Pressure facts are deterministic and suggest flush/compaction/materialization without product wording. |
| `crates/storage/src/segmented/compaction.rs::pick_and_compact` | Picks one highest-scoring unit of work and re-evaluates after each compaction. | L8K should enqueue/run one deterministic maintenance task at a time through the existing executor. Background self-resubmission remains deferred. | Generated scripts alternate candidate selection and execution rather than batching all work blindly. |
| `crates/storage/src/segmented/compaction.rs::compact_branch` | Merges all branch-owned segments for broad cleanup and preserves read results. | Use L6 `BranchCompactionKind::CompactL0` and future L6 candidate kinds instead of porting old segment loops. | Read parity after keep-all compaction; no L8 row merge logic. |
| `crates/storage/src/segmented/compaction.rs::compact_tier` | Merges a caller-chosen subset of same-tier segments. | Do not expose caller-chosen arbitrary table subsets in L8K. L6 candidates define valid inputs. | Invalid/stale requested candidates fail closed before mutation. |
| `crates/storage/src/segmented/compaction.rs::compact_l0_to_l1` | Expands L0 compaction with overlapping L1 tables and preserves non-overlapping L1 tables. | Preserve through L6 `CompactL0ToLevelOne` candidate facts. L8K only routes the task and reports output facts. | Candidate reports overlap refs; non-overlapping tables remain readable. |
| `crates/storage/src/segmented/compaction.rs::compact_level` | Moves/merges one nonzero-level table with overlaps in the next level; last/empty levels no-op. | Preserve no-op and overlap semantics through L6 candidate planning. | Last-level, empty-level, and overlap cases map to deferred or completed lifecycle outcomes. |
| `crates/storage/src/segmented/mod.rs::materialize_layer` | Marks inherited layer materializing, builds replacement child-owned tables, preserves child-local precedence, and removes the inherited layer only after replacement is visible. | Use L6 handle-based materialization. Do not rewrite rows or remove layers directly in lifecycle code. | Handle binding, child-local precedence, fork-version gates, and retry/idempotence outcomes. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | Manifest publish or directory fsync failure around compaction/materialization either leaves pre-publish state visible or records forward progress. | Translate to L4 publish/read failure and L6 install failure windows. Report structured health debt and published objects. | Publish failure unchanged reads; install failure after publish is retryable and lists published outputs. |
| `crates/storage/src/segmented/tests/resurrection.rs` | Compaction/materialization must not resurrect a branch cleared or deleted during the I/O window. | Preserve through stale-candidate checks and lifecycle admission. Branch clear/delete APIs are later, so test the stale-candidate form now. | Candidate stale before install fails closed and preserves current reads. |
| `crates/storage/src/test_hooks.rs` | Test-only pause hooks split I/O from atomic install for race tests. | Replace global pause hooks with deterministic fake publishers, maintenance fault hooks, and stale-candidate tests. | Fault windows are local to the test and do not add global runtime hooks. |
| `crates/engine/src/database/transaction.rs::schedule_background_compaction` | Schedules one background compaction chain, prevents duplicate in-flight chains, and wakes write-stall waiters after progress. | Port only coalescing/in-flight intent as maintenance queue behavior. Background threads and condition variables are not in L8K. | Duplicate compaction tasks coalesce; pressure facts change after one task. |
| `crates/engine/src/database/transaction.rs::pick_and_run_one` | Runs the highest-scoring branch compaction and then materializes deepest inherited layers. | Preserve ordering as a recommendation only: compaction/materialization are distinct task kinds, with pressure facts choosing the next one. | Task priority/coalescing tests prove deterministic order. |
| `crates/engine/src/database/transaction.rs::check_write_stall` | Product write-stall policy combines L0 backlog, memtable memory, and segment metadata pressure. | Do not port user-facing stall behavior. L8K emits storage pressure facts only; L9 or later policy can decide blocking. | Source/vocabulary tests reject product stall strings and public write errors. |
| `crates/engine/src/database/open.rs` post-recovery compaction scheduling | Schedules compaction after recovery so L0 does not remain permanently large. | Defer automatic post-open scheduling. L8K supplies explicit scheduling hooks and pressure facts. | Open/recovery tests do not start background compaction implicitly. |
| `crates/engine/src/database/tests/shutdown.rs` maintenance rejection tests | Mutating maintenance APIs reject during shutdown/in-progress close. | Preserve via lifecycle admission and maintenance close policy. | Compaction/materialization reject once close begins unless explicitly drain-required. |

Do not port these old-code details:

1. segment file names, directory fsync calls, or direct filesystem operations;
2. manifest publication from the compaction handler;
3. product `compact()` commands or product error wording;
4. public write-stall blocking policy;
5. global test pause hooks;
6. background thread self-resubmission;
7. pruning of older versions, tombstones, or expired rows without a retention
   proof;
8. direct deletion of replaced table files or objects;
9. primitive value, graph, vector, JSON, embedding, or query-layer vocabulary.

## Scope

L8K implements:

1. a concrete lifecycle module for compaction and materialization scheduling;
2. crate-private request/outcome types for compaction tasks;
3. crate-private request/outcome types for materialization tasks;
4. `MaintenanceTaskRequest` constructors for compaction and materialization;
5. cache-runtime handlers for:
   - branch compaction through L6;
   - inherited-layer materialization through L6;
6. durable-runtime handlers with the same lifecycle admission and health shape;
7. explicit durable-local deferral: table rewrites run through L6 and report
   that a checkpoint is required before recovery can trust the rewritten table
   graph;
8. no standalone durable table-object publication from this slice, because
   table-manifest recovery is not present yet;
9. storage pressure facts that can request or defer maintenance without product
   write-stall vocabulary;
10. maintenance outcome conversion for completed, deferred, and failed work;
11. direct unit coverage for compaction request, no-candidate, installed
    replacement, materialization intent, materialization success, durable
    checkpoint debt, and pressure facts;
12. source guards preventing lifecycle from importing product modules, raw
    filesystem APIs, table merge internals beyond public L5/L6 surfaces, or old
    primitive vocabulary;
13. a porting-log entry after implementation.

L8K does not implement:

1. new L5 merge algorithms;
2. retention pruning during compaction;
3. deletion, quarantine, purge, or repair of replaced table objects;
4. table-manifest recovery or manifest reachability publication;
5. checkpoint execution or WAL truncation;
6. background thread policy;
7. public compaction commands;
8. product write-stall errors or user-facing maintenance wording;
9. memory-budget enforcement beyond reporting storage pressure facts;
10. close-time drain/sync policy beyond the generic maintenance executor.

## Code Organization

Recommended files:

1. `crates/storage-next/src/lifecycle/compaction.rs`
2. `crates/storage-next/src/lifecycle/tests/compaction.rs`
3. `crates/storage-next/src/testkit/lifecycle/compaction.rs`
4. optional `crates/storage-next/src/lifecycle/tests/compaction/` helpers if
   direct tests approach 1,000 lines.

Do not put concrete compaction or materialization handlers into
`maintenance.rs`; the executor should stay generic.

Do not put architecture milestone labels, slice labels, or parent-plan names in
Rust code, test names, comments, fixture bytes, or panic messages. Keep that
vocabulary in planning documents and the porting log only.

## Type Surface

Names can change during implementation, but the responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleCompactionRequest {
    branch_id: BranchId,
    kind: BranchCompactionKind,
    output_identity_seed: CompactionOutputIdentitySeed,
    durability: LifecycleTableRewriteDurability,
}

pub(crate) struct LifecycleCompactionOutcome {
    status: LifecycleCompactionStatus,
    branch_id: BranchId,
    plan: Option<BranchCompactionPlan>,
    branch_outcome: Option<BranchCompactionOutcome>,
    checkpoint_required: bool,
    health: Option<RecoveryHealth>,
}

pub(crate) enum LifecycleCompactionStatus {
    Completed,
    CompletedCheckpointRequired,
    DeferredNoCandidate,
}

pub(crate) struct LifecycleMaterializationRequest {
    child_branch_id: BranchId,
    layer_index: usize,
    handle: Option<BranchMaterializationHandle>,
    output_identity_prefix: MaterializationOutputIdentityPrefix,
    durability: LifecycleTableRewriteDurability,
}

pub(crate) struct LifecycleMaterializationOutcome {
    status: LifecycleMaterializationStatus,
    intent: Option<BranchMaterializationIntent>,
    branch_outcome: Option<BranchMaterializationOutcome>,
    checkpoint_required: bool,
    health: Option<RecoveryHealth>,
}

pub(crate) enum LifecycleMaterializationStatus {
    Completed,
    CompletedCheckpointRequired,
    DeferredNoLayer,
    AlreadyMaterialized,
}

pub(crate) enum LifecycleTableRewriteDurability {
    VolatileOnly,
    CheckpointRequiredAfterRewrite,
}
```

The implementation starts conservatively. Because branch/table manifest
recovery is still absent, durable compaction and materialization do not publish
standalone table objects in this slice. They mutate the recovered branch state
through L6 and report checkpoint debt. A later table-manifest slice can replace
that conservative behavior with publish-before-install table-object facts.

## Compaction Protocol

Target cache-mode sequence:

```text
require lifecycle Open + ordinary maintenance admission
build BranchCompactionRequest from lifecycle request
ask L6 to plan branch compaction
if no candidate: return deferred outcome
call L6 compaction install path
return maintenance outcome with L6 candidate/output/table report facts
```

Target durable-local sequence for V1:

```text
require lifecycle Open + ordinary maintenance admission
build BranchCompactionRequest from lifecycle request
ask L6 to plan branch compaction
if no candidate: return deferred outcome
run the same L6 table rewrite as volatile branch state
report that checkpoint is required before durable replay can be shortened
```

The implementation must avoid claiming durable reachability for table rewrites
until checkpoint or table-manifest recovery can prove those outputs. Published
table-object windows are deferred to the table-manifest slice.

## Materialization Protocol

Target sequence:

```text
require lifecycle Open + ordinary maintenance admission
mark inherited layer materializing through L6
capture the returned BranchMaterializationHandle
build BranchMaterializationRequest from the handle
run materialize_inherited_layer through L6
return materialization outcome with rows/tables/skipped/recovery facts
```

Rules:

1. Lifecycle must not construct materialization from a naked layer index after
   the intent step.
2. Lifecycle retry requests that already have a materialization handle must
   carry the source identity back into L6, including the absent-layer retry
   case after a prior successful materialization removed the layer.
3. Lifecycle must preserve and surface L6 retry outcomes:
   - replacement visible and layer removed;
   - replacement already visible and layer removed;
   - layer already materialized.
4. Child-owned rows must remain higher precedence than materialized inherited
   rows because L6 owns the replacement install semantics.
5. L8 does not rewrite inherited rows directly.
6. Durable-local materialization follows the same V1 rule as compaction:
   report checkpoint-required rewrite facts and do not claim standalone table
   object reachability yet.

## Storage Pressure Facts

This slice introduces storage-shaped pressure facts, not product write-stall
policy. Initial facts should be simple and deterministic:

1. frozen table backlog;
2. level-zero table count;
3. nonzero-level table count by level;
4. inherited layer count;
5. materializing layer count;
6. pending maintenance queue depth;
7. estimated table bytes where L5/L6 facts expose them.

Output facts:

1. recommended maintenance task kind;
2. branch id/scope;
3. severity: none, background, urgent, or block-new-writes;
4. storage-only reason enum;
5. no product wording.

Actual admission blocking can remain a later memory-budget or close-policy
slice. L8K should only surface the fact in a way L9 can consume deliberately.

## Retention Policy

Compaction in this slice is keep-all.

Rules:

1. `BranchCompactionRetentionPolicy::KeepAll` is the only policy lifecycle may
   request directly in this slice.
2. Drop-older-version, tombstone, and expired-row compaction require a later
   retention proof.
3. Replaced table refs become candidates for L8L/L8M retention/quarantine, not
   immediate deletion.
4. Materialization replacement of inherited layers similarly produces
   reachability changes, not physical deletion.

## Failure Handling

Required failure classifications:

1. invalid request or bad task scope -> `InvalidConfig` or `MaintenanceFailed`;
2. lifecycle not open -> `InvalidLifecycleState`;
3. L6 planning/install failure -> lower-layer branch runtime source chain;
4. L5 table compaction/build failure -> lower-layer table runtime source chain
   preserved through L6 or lifecycle;
5. L4 table publication/read failure -> deferred until standalone rewrite
   output publication exists;
6. publish succeeded but install failed -> deferred until standalone rewrite
   output publication exists;
7. no candidate -> deferred maintenance outcome;
8. retention/pruning requested -> rejected until retention proof exists.

## Implementation Steps

### L8K-A: Plan The Local Surface

1. Add `lifecycle/compaction.rs`.
2. Add request/outcome/status types.
3. Add strict validation for branch id, output seed/prefix, level/index, and
   allowed retention mode.
4. Export crate-private types from `lifecycle/mod.rs`.

### L8K-B: Maintenance Request Constructors

1. Add compaction constructors to `MaintenanceTaskRequest` for table-level
   scopes.
2. Add materialization constructors for inherited-layer scopes.
3. Ensure coalescing keys include branch, level/index, or layer facts.
4. Kind-specific runtime runners must select only matching queued task kinds so
   they never consume unrelated pending maintenance.
5. Keep scope validation inside the executor.

### L8K-C: Cache Runtime Handlers

1. Add cache runtime methods for explicit compaction and materialization.
2. Add maintenance runners that convert queued tasks into requests.
3. Return `MaintenanceOutcome` with affected-object counts derived from branch
   replacement facts and no durable claims.

### L8K-D: Durable Runtime Handlers

1. Add durable runtime methods with the same request/outcome shape.
2. Use table-object services only if the implementation can publish before L6
   install.
3. If durable publication is not yet wired, report volatile rewrite plus
   checkpoint-required fact instead of a false durable claim.
4. Preserve source chains for L4/L5/L6 failures.

### L8K-E: Materialization Hook

1. Use `mark_inherited_layer_materializing`.
2. Build request from the returned handle.
3. Call `materialize_inherited_layer`.
4. For retry requests that already carry a handle, call L6 with that source
   identity instead of deferring merely because the layer index is absent.
5. Surface retry/idempotence outcomes in lifecycle vocabulary.

### L8K-F: Storage Pressure Facts

1. Add a small pressure fact type.
2. Compute facts from L6 branch state and maintenance executor status.
3. Map facts to suggested maintenance tasks.
4. Do not block writes in this slice unless an existing lower-layer admission
   guard already does so.

### L8K-G: Testkit And Source Guards

1. Add direct unit coverage for the new task routes.
2. Add source guard checks for no product modules, no raw fs/path APIs, no table
   merge reimplementation, and no reserved layout literals.
3. Update the porting log with implementation files, tests, verification, and
   deferred items.

## Deferred To Later Slices

1. Retention-proof compaction that drops old versions, tombstones, or expired
   rows: L8L or later.
2. Replaced object quarantine/purge: L8L-L8M.
3. Branch/table manifest recovery and durable table reachability publication:
   later durable-manifest work.
4. Background scheduling threads: post deterministic executor closeout.
5. Memory-budget enforcement and block-cache budget admission: future storage
   budget slice.
6. Product write-stall language or user commands: L9 and above.
7. Crash/localfs compaction harnesses beyond deterministic unit tests: L8O.
8. Generated lifecycle testkit counters for compaction/materialization scripts:
   later assurance-depth work, after the direct runtime surface is closed.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional if `cargo-hack` is installed:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Close Criteria

L8K can close when:

1. compaction and materialization work are routed through maintenance tasks;
2. L8 asks L6 for candidate/intent facts rather than selecting rows itself;
3. cache runtime can compact/materialize without durable claims;
4. durable runtime either publishes/validates outputs before install or clearly
   reports checkpoint-required volatile rewrite facts;
5. no-candidate and already-materialized paths are deferred/idempotent, not
   errors;
6. read results are unchanged by keep-all compaction and materialization;
7. child-owned precedence and fork-version gates remain L6-owned;
8. standalone output publication failures are explicitly deferred to the
   table-manifest slice;
9. replaced tables are not deleted in this slice;
10. pressure facts are storage vocabulary only;
11. direct unit tests exercise compaction and materialization routes;
12. source guards prevent product, raw filesystem, and lower-layer algorithm
    leakage;
13. the verification command set passes.
