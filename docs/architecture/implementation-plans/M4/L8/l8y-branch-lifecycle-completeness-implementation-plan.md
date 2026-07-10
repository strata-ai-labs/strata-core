# L8Y Implementation Plan: Branch Lifecycle Completeness

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md`

## Objective

Complete storage-internal branch lifecycle mechanics before the public storage
API layer is added.

The baseline branch runtime can hold branch-local rows, inherited copy-on-write
layers, pinned read views, and per-branch commit-generation guards. The durable
lifecycle runtime still behaves mostly like a single-branch shell. L8Y closes
that gap by adding the storage-owned catalog and orchestration needed for branch
create, list, clear, delete, fork, fork-at-history, generation reuse, pinned-view
retention, and recovery reconciliation.

This slice remains below product policy. It must not decide public branch names,
permissions, merge behavior, workspace rules, remote sync semantics, or product
delete policy. It returns raw storage facts so the later API layer can expose
the behavior safely.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
4. `docs/architecture/storage/l9-storage-api-boundary.md`
5. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
7. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
8. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
9. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
10. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
11. `crates/storage-next/src/branch/state.rs`
12. `crates/storage-next/src/branch/read.rs`
13. `crates/storage-next/src/branch/facts.rs`
14. `crates/storage-next/src/commit/branch_registry.rs`
15. `crates/storage-next/src/commit/guard.rs`
16. `crates/storage-next/src/lifecycle/durable.rs`
17. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
18. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
19. `crates/storage-next/src/lifecycle/recovery.rs`
20. `crates/storage-next/src/lifecycle/checkpoint.rs`
21. `crates/storage-next/src/lifecycle/compaction.rs`
22. `crates/storage-next/src/lifecycle/retention.rs`
23. `crates/storage/src/segmented/mod.rs`
24. `crates/storage/src/segmented/compaction.rs`
25. `crates/storage/src/segmented/ref_registry.rs`
26. `crates/storage/src/segmented/recovery.rs`
27. `crates/storage/src/segmented/quarantine_protocol.rs`

## Existing-Code Source Map

| Current file | Evidence | L8Y action |
|---|---|---|
| `branch/state.rs` | `BranchLocalState` owns branch rows, inherited layers, pinned-read snapshots, fork capture, table install, compaction, materialization, and snapshot row install. | Keep branch mutation and visibility rules in L6. Add only lifecycle-level orchestration and catalog ownership around those APIs. |
| `branch/read.rs` | Read views merge active, frozen, owned, and inherited sources under version/timestamp bounds. | Use pinned views as the safety contract for clear/delete/fork; do not invent a second visibility model. |
| `branch/facts.rs` | Branch facts track observed rows, reachability, release references, and timestamp coverage. | Promote those facts into branch lifecycle outcomes and durable manifest updates. |
| `commit/branch_registry.rs` | Branch generation descriptors, duplicate-create rejection, deleting/deleted states, generation guard validation, and admission guards exist. | Make this registry part of lifecycle branch catalog state instead of a single-root helper. |
| `commit/guard.rs` | Per-branch guards and quiesce behavior serialize commits. | Use L7 guards around clear, delete, fork, and fork-at-history so stale commits cannot race lifecycle transitions. |
| `lifecycle/durable.rs` | Durable open currently builds one `BranchLocalState` and one registry entry from `initial_branch_id`. | Replace single-branch shell assumptions with a storage branch catalog and selected default branch view. |
| `lifecycle/durable/bootstrap.rs` | Recovery bootstrap validates WAL records against the opened branch and catches up L7 facts. | Rebuild all recovered branch catalog entries and replay records for every recovered branch, then publish global visible facts. |
| `lifecycle/checkpoint.rs` | Checkpoint rows are row-native and branch-tagged. | Ensure checkpoint creation and recovery preserve multi-branch rows and branch lifecycle descriptors. |
| `lifecycle/compaction.rs` | Maintenance tasks target branch-scoped compaction and materialization. | Reject stale/missing/deleting targets by branch generation and route release facts to retention. |
| `lifecycle/retention.rs` | Retention consumes reachability, table-object, snapshot, WAL, and health proof. | Add branch-deletion release facts as retention inputs. L8Y must not directly delete shared objects. |

## Old Codebase Porting Map

The old segmented storage code combined branch state, recovery, compaction, and
reference tracking in a larger store object. L8Y ports only the storage-shaped
branch lifecycle behaviors and keeps product semantics above this layer.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `storage/src/segmented/mod.rs` | Branch create/fork/clear/delete operated over segmented branch state and shared tables. | Rebuild as a lifecycle branch catalog plus L6 branch states, not as a monolithic segmented store. | Create/list/fork/clear/delete produce deterministic raw facts. |
| `storage/src/segmented/ref_registry.rs` | Shared table references prevented unsafe deletion while branches or readers still referenced a table. | Use L6 reachability and L8S/L8M retention proof as the authority; do not add another ref registry. | Delete/clear emits release candidates but does not delete live shared objects. |
| `storage/src/segmented/compaction.rs` | Stale compaction and materialization candidates could not resurrect deleted or cleared branch state. | Route maintenance through generation-checked branch lifecycle admission. | Stale candidate tests fail closed without mutation. |
| `storage/src/segmented/recovery.rs` | Recovery reconstructed branch state and degraded health from durable metadata. | Reconstruct branch descriptors, generations, lifecycle statuses, and branch states from storage-next manifests/checkpoints/WAL. | Deleted branches do not resurrect after restart. |
| `storage/src/segmented/quarantine_protocol.rs` | Unsafe or ambiguous objects were quarantined instead of immediately purged. | Preserve the quarantine/repair handoff for released branch table objects. | Delete/clear sends ambiguous objects to retention/quarantine proof. |
| Old resurrection tests | Clear/delete racing with flush or compaction must not make old rows visible again. | Keep the race tests as storage-lifecycle sensitivity probes. | Clear/delete followed by stale install/rewrite rejects. |

Do not port:

1. product branch naming or workspace policy;
2. primitive DTO reconstruction;
3. merge, cherry-pick, revert, restore, or remote branch semantics;
4. raw filesystem path ownership;
5. process-global reference registries;
6. object-store provider behavior;
7. public user-facing API wording.

## Scope

L8Y implements:

1. a lifecycle-owned branch catalog containing branch id, generation, lifecycle
   status, parent/source facts, fork version, created/deleted facts, and durable
   manifest identity;
2. deterministic branch listing over active, deleting, deleted, and recovered
   descriptors;
3. storage-internal branch create with duplicate detection and generation
   initialization;
4. storage-internal fork from current visible source state using L6 inherited
   layers and L7 quiesce/admission guards;
5. storage-internal fork at an explicit retained commit version;
6. clear branch as an atomic lifecycle operation that preserves branch identity
   and generation while releasing rows/table references;
7. delete branch as a deleting-to-deleted lifecycle operation that rejects new
   commits, preserves pinned views, and emits reachability release facts;
8. generation reuse for deleted branch ids with stale-generation rejection;
9. recovery of branch catalog, branch state, generations, deleted markers, and
   in-flight lifecycle operations;
10. maintenance routing that rejects stale, missing, deleting, or deleted branch
    targets before publishing new table state;
11. checkpoint/table-manifest integration for multi-branch branch states;
12. source guards preventing product vocabulary, raw IO, branch API policy, and
    milestone labels in Rust code, test names, fixture bytes, and error strings.

L8Y does not implement:

1. public storage API methods;
2. branch name validation or user-visible naming;
3. product permissions;
4. merge, cherry-pick, revert, restore, branch comparison, or branch history UI;
5. remote/hub branch synchronization;
6. object-store production provider semantics;
7. direct physical deletion of table objects;
8. row-retention policy beyond the proof-gated L8V surfaces.

## Branch Catalog Model

Suggested shape:

```rust
pub(crate) struct LifecycleBranchCatalog {
    descriptors: Vec<LifecycleBranchDescriptor>,
    branches: Vec<BranchLocalState>,
}

pub(crate) struct LifecycleBranchDescriptor {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    state: LifecycleBranchState,
    created_at: LifecycleBranchFact,
    parent: Option<LifecycleBranchParent>,
    deleted_at: Option<LifecycleBranchFact>,
}
```

Exact names can change. Required properties:

1. Branch id is the logical storage branch key.
2. Generation is nonzero and increases when a deleted branch id is reused.
3. Catalog ordering is deterministic and independent of insertion order when
   serialized or listed.
4. The commit registry and lifecycle catalog cannot disagree about branch state.
5. Deleted branch descriptors remain long enough to reject stale generation
   guards and reconcile durable metadata.
6. The catalog does not own table-object deletion. It emits release facts.

## Branch States

Storage-internal clear and delete are atomic synchronous transitions
at the storage layer; transient observable states (Creating /
Clearing / Deleting visible to external admission guards) belong at
higher layers where async work happens.

Minimum lifecycle states:

1. `Active`: commits, reads, maintenance, fork, clear, and delete may be
   admitted subject to normal guards.
2. `Deleted`: new commits and reads reject unless a later generation recreates
   the branch id.

State-transition rules:

1. Successful clear preserves `Active`; the branch's row state is
   atomically replaced while pinned views retain their references to
   the released tables until the next retention pass.
2. Successful delete transitions `Active -> Deleted` atomically.
3. Recreate transitions `Deleted -> Active` only with a strictly
   greater generation.
4. Commit admission must reject `Deleted`.
5. Maintenance admission must reject stale generations and non-active
   states unless the task is the lifecycle operation that owns the
   transition.

## Create Protocol

Target sequence:

```text
require runtime open and not closing
validate branch id and requested generation
check catalog and commit registry for existing descriptor
reserve branch catalog mutation
install empty BranchLocalState
register active commit descriptor
publish branch manifest/catalog fact if durable mode
return BranchCreateOutcome
```

Rules:

1. Duplicate create rejects with a typed branch-already-exists error.
2. Create has no product name validation.
3. Create initializes no table objects and no WAL rows.
4. Durable create is recoverable: an in-flight descriptor never creates a
   writable branch without its matching empty branch state.
5. Cache create is volatile and reports no durable claims.

## List Protocol

Branch listing returns raw descriptors sorted deterministically.

Rules:

1. Active branches are listed by default.
2. Deleted branches can be listed by explicit storage-internal option so
   generation guards and recovery can be inspected.
3. Listing never reads table objects.
4. Listing does not run product access filtering.
5. Listing while close is in progress returns either a stable snapshot or a
   typed lifecycle-state rejection.

## Fork Protocol

Fork from current source state:

```text
require source branch active
require destination absent or deleted-with-new-generation
acquire source quiesce/read guard
acquire destination create guard
capture source visible branch facts
call L6 fork/attach inherited-layer APIs
install destination branch descriptor and state atomically
publish durable branch catalog/table-manifest facts
return fork outcome with source, destination, generation, fork version, refs
```

Rules:

1. Missing source rejects before destination mutation.
2. Non-empty destination rejects.
3. Source active/frozen rows must be handled by an explicit policy: either
   quiesce and flush/rotate before capture, or fail closed with a typed
   "source contains unflushed rows" error. Silent row loss is forbidden.
4. The fork version is the source's applied visible version at capture, not a
   product branch-history marker.
5. Child-local writes must outrank inherited rows.
6. Shared table references become reachability facts, not object copies.

## Fork-At-History Protocol

Fork at a retained version is storage-internal readiness for the public API
layer. It is not a user-facing branch-history policy in this slice.

Target sequence:

```text
require source branch active
validate requested commit version is retained and <= source visible version
validate timestamp/history coverage is sufficient when requested by timestamp
capture source rows/tables visible at requested version
construct inherited layer with fork_version = requested version
install destination descriptor and state atomically
return fork-at-history outcome with coverage proof
```

Rules:

1. Requested version after visible rejects.
2. Requested version below retained floor rejects.
3. Requested timestamp without coverage proof rejects or returns a typed
   insufficient-history result.
4. Rows newer than the fork version must not appear in the child.
5. Recovered fork-at-history descriptors must preserve the requested version,
   not recompute from the latest source state.

## Clear Protocol

Clear removes branch-visible rows while keeping the branch id and generation
active.

Target sequence:

```text
mark branch clearing and reject new commits
quiesce branch commits and maintenance installs
capture old reachability and pinned-view facts
replace branch state with empty state for the same branch id
publish durable branch/table manifest update if durable mode
release old refs to retention/quarantine proof
mark branch active
return BranchClearOutcome
```

Rules:

1. Pinned read views captured before clear remain valid.
2. New read views after clear see an empty branch.
3. Clear does not increment generation unless the implementation chooses to
   make clear equivalent to delete-plus-recreate; if it does, that rule must be
   explicit and tested.
4. Clear never physically deletes shared table objects.
5. Stale flush/compaction/materialization outputs for the old state must reject
   instead of resurrecting rows.

## Delete Protocol

Delete removes branch storage state while preserving enough descriptor facts to
reject stale handles and recover safely.

Target sequence:

```text
mark branch deleting in catalog and commit registry
reject new commits and ordinary maintenance
quiesce active commits and branch maintenance
capture pinned-view and reachability facts
remove active/frozen/owned/inherited branch state from catalog
publish durable branch tombstone/catalog fact if durable mode
emit release candidates to retention/quarantine proof
mark branch deleted
return BranchDeleteOutcome
```

Rules:

1. Delete while pinned views exist is allowed only if their referenced table
   objects remain retained until the views are released.
2. Delete does not physically purge objects.
3. Reads, commits, flush, compaction, materialization, checkpoint-only branch
   mutation, and row pruning reject deleted branches.
4. A later branch-id reuse must use a greater generation.
5. Recovery must not resurrect deleted branch state from stale table manifests
   or WAL rows without a matching newer generation.

## Generation Guards

Rules:

1. Every commit, branch maintenance task, flush, compaction, materialization,
   clear, delete, and fork destination mutation carries the expected branch
   generation when it crosses a queue or durable boundary.
2. Stale generation rejects before table-object publication.
3. Generation exhaustion returns a typed error.
4. Cache and durable mode share the same generation semantics.
5. Recovery catches up the catalog generation from the highest durable branch
   descriptor and deleted marker.

## Pinned Views And Reachability

Rules:

1. A pinned view is a retention root for its active/frozen/owned/inherited table
   references.
2. Clear/delete may remove branch catalog visibility but must not release table
   objects still referenced by pinned views.
3. Release facts must include branch id, generation, table identity, source kind,
   and reason.
4. Retention and quarantine consume release facts; branch lifecycle does not
   directly call backend delete for table objects.
5. Pinned views are storage facts, not product snapshot handles.

## Recovery

Recovery must rebuild:

1. branch descriptors and generations;
2. branch states from checkpoints, table manifests, WAL, and deleted markers;
3. inherited layers and fork versions;
4. clear/delete release facts that were durable but not yet reclaimed;
5. commit registry descriptors;
6. maintenance queue rejection state for stale branch tasks;
7. global visible/timestamp facts already owned by L7/L8G.

Recovery rules:

1. Deleted branch markers outrank older table manifests for the same generation.
2. Newer generation active descriptors outrank older deleted markers.
3. WAL rows for missing/deleted/stale-generation branches reject unless recovery
   can prove they belong to a newer recovered descriptor.
4. Recovery never chooses product policy. It returns raw conflicts and health
   facts.

## Error Vocabulary

Add or reuse typed storage errors for:

1. branch already exists;
2. branch not found;
3. branch not writable;
4. branch deleting;
5. branch deleted;
6. branch generation mismatch;
7. branch generation exhausted;
8. source branch missing;
9. destination branch not empty;
10. fork version not retained;
11. insufficient timestamp history;
12. pinned-view release blocked;
13. stale lifecycle task;
14. branch manifest mismatch;
15. branch recovery conflict.

Errors must expose stable code accessors that follow the repository error-code
format and tests must assert on codes/classes, not display strings.

## Source Boundary Rules

1. L8Y code remains `pub(crate)` unless a lower layer already requires another
   visibility.
2. Branch lifecycle code must not import engine, primitive, query, remote, hub,
   product, path, raw filesystem, environment, or network APIs.
3. Branch lifecycle code must not include milestone labels in Rust code, test
   names, fixture bytes, or user-facing error strings.
4. Lifecycle code may call L6 branch APIs, L7 guard/registry APIs, L8
   manifest/recovery/retention APIs, and L4 services through existing service
   abstractions.
5. L6 must not import lifecycle.
6. L7 must not import lifecycle branch catalog code.
7. Tests that enforce source boundaries should scan production source, test
   source, fuzz targets, and seed corpora.

## Implementation Steps

1. Add branch lifecycle descriptor/status/outcome vocabulary.
2. Add a lifecycle branch catalog that keeps `BranchLocalState` and commit
   registry descriptors coherent.
3. Replace durable single-branch shell assumptions with catalog lookup while
   keeping an initial/default branch convenience for existing tests.
4. Add create and list operations.
5. Add generation-checked maintenance admission helpers.
6. Add fork from current state over L6 inherited layers.
7. Add fork-at-history with retained-version and timestamp coverage checks.
8. Add clear with pinned-view and release-fact preservation.
9. Add delete with deleting/deleted transitions and generation reuse.
10. Add durable branch catalog/table-manifest publication hooks.
11. Extend recovery to rebuild branch catalog state and reconcile in-flight
    branch lifecycle states.
12. Wire checkpoint, table-manifest, retention, quarantine, flush, compaction,
    and materialization surfaces through branch generation checks.
13. Add generated/fault coverage and source guards.
14. Update the porting log with old-code behavior, sensitivity probes, command
    results, and deferred items.

## Deferred

| Deferred item | Owner | Reason |
|---|---|---|
| Public branch API, branch names, and user-visible policy | L9 | This slice is storage-internal. |
| Product merge/cherry-pick/revert/restore/compare | Above L9 | These are semantic product operations. |
| Remote/hub branch synchronization | StrataHub integration workstream | Remote refs and sync are not local lifecycle. |
| Object-store production provider behavior | Post-V1 object-store work | L8Y only uses local/durable services already in scope. |
| Physical deletion of released table objects | L8S/L8M | Lifecycle emits release facts; retention/quarantine proves deletion. |
| Query/index API over branches | Later query layer | Storage exposes raw ordered records and branch facts only. |

## Exit Gate

L8Y is complete when:

1. branch create/list/clear/delete/fork/fork-at-history work in cache and durable
   local modes;
2. duplicate create, missing source, non-empty destination, stale generation,
   deleted branch, and unretained fork version reject with typed errors;
3. pinned read views remain valid across clear/delete/fork and protect
   reachability;
4. stale flush/compaction/materialization tasks cannot resurrect cleared or
   deleted rows;
5. recovery preserves branch catalog, generation, deleted markers, inherited
   layers, and fork-at-history facts;
6. table-object retention receives release facts but branch lifecycle never
   directly deletes table objects;
7. source guards prevent product policy and milestone labels in code/tests;
8. generated/fault tests cover branch lifecycle ordering, not only examples;
9. the full slice command matrix is recorded in the porting log.
