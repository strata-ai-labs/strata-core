# Engine-Next Branch Operation And Capability Adapter Contract

Status: V1 architecture draft

## Purpose

This document defines how engine-next owns branch workflows and how data
capabilities participate in those workflows.

Branches are database workspaces and timelines. Storage-next supplies generic
branch mechanics through the persistence adapter. Engine-next owns the product
meaning of creating, comparing, promoting, copying, restoring, and deleting
branches.

The target boundary is:

```text
product branch request
  -> branch workflow service
  -> persistence adapter for storage mechanics and row scans
  -> capability branch adapters for row interpretation and plans
  -> persistence adapter for resulting commit plans
```

not:

```text
branch workflow
  -> storage internals
  -> ad hoc primitive-specific decoding
  -> one-off merge logic hidden in unrelated modules
```

## Related Documents

Read this with:

1. `docs/product/strata-v1-branching-direction.md`
2. `docs/product/strata-v1-versioning-time-travel.md`
3. `docs/product/pathways/branching-versioning-time-travel.md`
4. `docs/architecture/engine-architecture.md`
5. `docs/architecture/engine/README.md`
6. `docs/architecture/engine/primitive-implementation-contract.md`
7. `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`
8. `docs/architecture/engine/storage-space-id-registry.md`
9. `docs/architecture/engine/persistence-adapter-contract.md`
10. `docs/architecture/storage/l9-storage-api-boundary.md`

Follow-up contracts that depend on this one:

1. Temporal context and timeline resolver contract.
2. Control-plane layout contract.
3. Retrieval and derived-state contract.
4. Dataset clone artifact contract.
5. Product-pathway conformance plan.

## Requirement Language

1. Must means the branch contract is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

Current engine code already contains the core ingredients:

1. `branch_ops/mod.rs` implements fork, diff, three-way diff, merge,
   cherry-pick, revert, materialize, and legacy tag/note helpers.
2. `database/branch_service.rs` wraps branch operations with lifecycle,
   generation, metadata, observer, and race-protection behavior.
3. `branch_ops/branch_control_store.rs` stores branch control records, lineage
   edges, merge-base data, tombstones, and generation facts.
4. `branch_ops/primitive_merge.rs` contains per-primitive merge handlers for
   KV, JSON, event, vector, and graph.
5. JSON merge already has document/path-aware behavior.
6. Event and graph merge behavior already has stricter semantic checks than a
   byte-level last-writer overwrite.
7. Branch deletion already has cleanup, quiesce, and same-name recreation
   protections.

The current code also shows what must change:

1. Branch code imports storage keys and `TypeTag` directly.
2. The public vocabulary is still Git-shaped: merge, merge-base, cherry-pick,
   revert.
3. Tags, notes, and branch bundles remain current implementation residue, not
   V1 branching requirements.
4. Merge strategy names still include `LastWriterWins`, while the product
   concept is source-wins promotion.
5. Capability-specific merge contracts exist but are not yet the uniform
   implementation pattern for every branch workflow.

## Current Operation Mapping

The target architecture should account for the current branch operation surface,
but it does not need to preserve every Git-derived name as primary product
vocabulary.

| Current operation | Target workflow |
|---|---|
| `BranchCreate` | Create an empty branch workspace with branch metadata and required control-plane rows. |
| `BranchFork` | Create branch from existing branch state. |
| `BranchGet` | Inspect one branch. |
| `BranchList` | Inspect/list branches. |
| `BranchExists` | Branch existence check, usually internal validation or command diagnostics. |
| `BranchDelete` | Delete branch safely. |
| `BranchDiff` | Compare branch state. |
| `BranchDiffThreeWay` | Preview promotion conflicts. |
| `BranchMergeBase` | Branch point explanation or diagnostic, not a primary product workflow. |
| `BranchMerge` | Promote source branch changes into target branch. |
| `BranchCherryPick` | Copy selected records or apply selected changes. |
| `BranchRevert` | Restore or undo a version range by writing compensating changes. |
| `materialize_branch` | Internal branch maintenance/materialization, not a normal user workflow. |
| Branch tags and notes | Remove from V1 branch requirements; keep only as migration residue if needed. |
| Legacy branch bundle import/export/validate | Remove or replace with dataset clone artifact workflows. |

## Definitions

### Branch Workflow

A branch workflow is an engine-owned product operation over branches.

V1 workflows:

1. Create branch from current state.
2. Create branch from retained version.
3. Create branch from retained timestamp.
4. Inspect branch state and lineage.
5. Compare branch state.
6. Preview promotion conflicts.
7. Promote source branch changes into a target branch.
8. Delete a branch safely.

Deferred to post-V1 (the designs below are retained for the future ops, but V1
does not ship them; their absence is enforced by
`crates/engine/tests/branch_merge_absence.rs`):

- Copy selected records or selected changes between branches (cherry-pick).
- Restore or undo a version range by writing compensating changes (revert).

Branch and space context selection is a required product behavior, but it is not
a mutating branch workflow. API, runtime, CLI, and IPC surfaces may remember an
active context for ergonomics. Branch workflows receive explicit resolved branch
and space inputs and must not infer planning state from ambient context.

### Capability Branch Adapter

A capability branch adapter is the engine-side contract that lets a data
capability participate in branch workflows.

It owns capability-specific interpretation of:

1. Comparable logical entities.
2. Authored rows.
3. Capability-local metadata rows.
4. Derived rows.
5. Conflict rules.
6. Copy and restore behavior.
7. Branch-delete cleanup behavior.
8. Relationship-layer effects where relevant.

It must not own product workflow orchestration or storage mechanics.

### Branch Point

A branch point is the concrete retained commit version of a source branch from
which another branch state is derived or compared.

Branch points may come from:

1. Current source branch state.
2. Retained commit version.
3. Retained timestamp resolved through the commit timeline.
4. Recorded lineage such as fork anchors and promotion edges.

Branch points must be derived by Strata from retained history and branch
metadata. Users should not inject synthetic merge bases or hidden ancestors.

### Branch State Selector

A branch state selector identifies the branch state a workflow reads.

V1 selectors:

1. Current branch state.
2. Branch at a retained commit version.
3. Branch at a timestamp resolved to a retained version frontier.

The temporal context and timeline resolver contract owns user-facing selector
syntax and timestamp explanation. This contract requires branch workflows to use
one resolved branch-local frontier for each selected historical state.

For version selectors, the selected commit version is exact as a branch
frontier. Capability reads under that frontier still use normal MVCC
visibility, so records last changed earlier than the branch frontier may be the
observed values.

### Conflict

A conflict means source and target both changed state since the shared branch
point in a way the capability cannot combine under the selected strategy. The
common case is both sides changing the same logical entity, but conflicts may
also come from capability invariants across related entities: event ordering,
graph referential integrity, vector collection metadata, relationship bindings,
or branch-local control-plane constraints.

Conflicts are product facts. They should name:

1. Capability.
2. Space.
3. Entity or key.
4. Source value summary or tombstone.
5. Target value summary or tombstone.
6. Conflict kind.
7. Strategy result.

### Derived-State Disposition

Derived-state disposition describes what happens to rebuildable rows during a
branch workflow.

Allowed dispositions:

1. Preserve after validation.
2. Rebuild.
3. Mark stale.
4. Drop.
5. Refuse the workflow because the derived state is authoritative.

Derived rows must not silently merge as user-authored source rows.

## Binding Decisions

1. **Engine owns branch product semantics.**
   Storage owns branch mechanics. Engine owns create-from, compare, preview,
   promote, copy, restore, delete, conflict policy, and user-facing diagnostics.

2. **Persistence is the only storage path.**
   Branch workflows use the persistence adapter for branch mechanics, row scans,
   timeline frontiers, and commit plans. Branch code must not import storage-next
   internals or construct physical keys directly.

3. **Capabilities interpret their own rows.**
   Branch workflows coordinate. Capability branch adapters decode authored rows,
   compare logical entities, classify conflicts, and plan capability-specific
   mutations.

4. **Storage-space classification drives inclusion.**
   Source, control, and derived classification comes from the storage-space
   registry. Capability adapters may refine behavior by key prefix and value
   schema, but they must not reclassify derived rows as source rows.

5. **Lineage is authoritative for shared branch points.**
   Promotion and preview must derive the branch point from recorded branch
   lineage. Unrelated branches fail unless a later explicit transplant/adoption
   workflow is defined.

6. **Strict is the safe default.**
   Strict conflict handling refuses promotion when conflicts exist. Source-wins
   is allowed only as an explicit strategy and must report every overwritten or
   deleted target entity.

7. **Branch operations write new commits.**
   Promotion, selected copy, and restore mutate the target branch by writing a
   new commit. They do not rewrite history or delete old versions.

8. **Branch-visible control metadata is part of workflow atomicity.**
   The rows that make a branch workflow visible and authoritative, such as
   branch catalog state, fork anchors, promotion lineage, target workflow
   metadata, and generation guards, must be committed with the source/control
   mutations that need them or protected by a recoverable workflow intent written
   before mutation. A visible data change without enough branch-control metadata
   to explain or recover it is an invariant failure.

9. **Branch-from-time resolves before storage work.**
   A timestamp branch point must resolve to a concrete retained commit version
   through the timeline resolver before create, compare, promote, copy, or
   restore logic reads rows.

10. **Derived state is explicit.**
   Search indexes, shadow vectors, vector indexes, graph reverse maps, retrieval
   projections, and `0x45` health/watermark rows are preserved, rebuilt, marked
   stale, dropped, or refused according to their owning contracts.

11. **Same-name branch recreation must not inherit stale state.**
    Deletion and recreation must use generation or equivalent guards so stale
    branch data, derived state, observers, DAG events, or cleanup debt cannot
    attach to the new branch.

12. **Branch metadata is control-plane data.**
    Branch catalog, lineage, branch DAG projection, and workflow metadata live
    under the branch control storage-space assignment. They are not graph
    relationship-layer data unless a later adapter explicitly derives graph
    accelerators from them.

13. **Tags, notes, and legacy branch bundles are not V1 branch requirements.**
    If retained during migration, they are compatibility residue and must not
    define the V1 branch contract.

14. **User vocabulary should be activity-shaped.**
    Public docs should prefer create branch from, compare, preview promotion,
    promote, copy selected records/changes, restore range, and delete branch.
    Git-derived internal names may remain as implementation evidence.

## Common Workflow Shape

Every mutating branch workflow should follow this shape:

```text
validate request and access mode
  -> resolve branch names to branch refs and generations
  -> resolve branch point or temporal frontier
  -> collect storage-shaped candidate rows through persistence
  -> ask capability adapters to interpret and plan
  -> validate conflicts, derived-state disposition, and cleanup requirements
  -> commit target-branch mutations and required branch-control rows through persistence
  -> emit diagnostics, observer facts, and derived-state follow-up work
```

Rules:

1. Read-only handles reject mutating workflows before storage mutation.
2. Missing, deleted, archived, or generation-mismatched branches fail with
   structured errors.
3. A workflow that cannot prove its branch point must fail before mutation.
4. A workflow that fails during planning must leave no visible target changes.
5. A workflow must not publish user-visible data changes unless the committed
   state also contains, or can recover, the required branch-control metadata for
   that workflow.
6. Global projections, observer rows, and derived DAG/search/vector/graph
   accelerators may run after the authoritative commit only when they are
   reconstructible from committed source/control rows.
7. A workflow that commits source/control changes but cannot complete derived
   rebuild work must report derived-state debt.
8. User-facing operation summaries must be based on the committed plan, not
   preflight estimates.

## Branch Creation

### Create From Current State

Creating a branch from current state starts a new branch at the source branch's
latest visible version.

Required behavior:

1. Validate source and destination branch names.
2. Refuse duplicate active destination names.
3. Record the source branch, source generation, and branch point version.
4. Use storage COW mechanics through persistence instead of eager full copies
   when storage can support it.
5. Initialize branch-local control-plane state.
6. Preserve, rebuild, or mark branch-local derived state according to owning
   contracts.
7. Report created branch, source branch, branch point, and derived-state debt.

Publication rule:

1. The destination branch must not become a normal user-visible branch until the
   storage fork, branch catalog row, fork anchor, generation guard, and
   branch-local system-space initialization are all committed or recoverable.
2. If storage fork publication succeeds but engine branch-control publication
   fails, recovery must either finish publishing the authoritative branch-control
   rows or clean up/quarantine the destination storage state before it can appear
   as an ordinary branch.

### Create From Version

Creating from version starts a branch from a retained commit version in a source
branch.

Required behavior:

1. Validate that the requested version is retained and belongs to the source
   branch timeline.
2. Use persistence's fork-at-retained-version storage mechanic with the requested
   version as the exact branch point.
3. Preserve historical semantics for reads, relationships, and derived-state
   compatibility.
4. Fail clearly if retention has removed the required history.

### Create From Timestamp

Creating from timestamp starts a branch from the retained commit version at or
before a requested timestamp, when the timestamp is inside retained branch
timeline bounds.

Required behavior:

1. Resolve timestamp through the timeline resolver before scanning rows.
2. Use the resolved version frontier as the fork-at-retained-version branch
   point for the new branch.
3. Report requested timestamp, resolved version, resolved commit timestamp, and
   timeline bounds where useful.
4. Fail clearly when the timestamp is before retained history, after the latest
   retained commit, inside a pruned gap, or otherwise cannot resolve.

### Create Empty Branch

Empty branch creation is optional for V1 product surface, but if retained it
must be distinct from create-from-existing-state.

It must initialize branch control metadata and branch-local system space without
pretending to have a source branch point.

## Branch Inspection

Branch inspection should expose:

1. Branch name and ID.
2. Lifecycle status.
3. Generation or equivalent recreation guard.
4. Current visible version.
5. Created/updated timestamps where retained.
6. Source branch and branch point when known.
7. Derived-state health summary.
8. Protection facts such as default/system branch rules.

System branches should not appear as ordinary user branches unless the caller
explicitly asks for internal diagnostics.

Merge-base or lineage details can appear as explanation fields. They should not
be the primary V1 user workflow.

## Compare And Preview

### Compare Branches

Compare shows what differs between two branch states.

Inputs:

1. Left branch state selector.
2. Right branch state selector.
3. Optional space filters.
4. Optional capability filters.
5. Optional entity/key filters.
6. Limit and pagination facts for large comparisons.

Rules:

1. Historical selectors must resolve to branch-local version frontiers before
   row scans.
2. Results should be grouped by capability and space.
3. Added, removed, modified, and tombstoned entities must be distinguishable.
4. Binary keys and values need stable programmatic representation.
5. Derived rows should be omitted from user data comparison unless the caller
   asks for diagnostic or system comparison.
6. Capability adapters own value comparison and display-safe summaries.

### Preview Promotion

Preview promotion is a three-way comparison:

```text
branch point -> source
branch point -> target
```

It must:

1. Derive the branch point from lineage.
2. Ask capability adapters to classify changes and conflicts.
3. Report conflicts before target mutation.
4. Report capability coverage and unsupported areas.
5. Report derived-state disposition that promotion would trigger.

Preview must not mutate source or target.

## Promotion

Promotion applies completed source branch changes into a target branch.

Rules:

1. Source branch remains unchanged.
2. Target branch receives a new commit if the promotion applies any mutations.
3. Branch point is derived from recorded lineage.
4. Strict strategy fails if any conflict exists.
5. Source-wins strategy applies source values for conflicts and reports every
   overwritten or deleted target entity.
6. Capability adapters produce mutation plans; branch workflow coordinates a
   single target commit plan.
7. The authoritative promotion edge and target workflow metadata are included in
   the target commit plan or protected by a recoverable workflow intent written
   before target mutation.
8. Global branch-DAG projections, observers, and accelerators may update after
   the target commit only if they are reconstructible from committed
   branch-control rows.
9. Derived-state rows are rebuilt, marked stale, validated, dropped, or refused
   according to owning contracts.

Promotion outcome should report:

1. Source and target branch refs.
2. Branch point.
3. Strategy.
4. Applied source rows/entities.
5. Deleted target rows/entities.
6. Conflicts and strategy results.
7. Spaces and capabilities covered.
8. Target commit version and timestamp.
9. Derived-state disposition and debt.

## Selected Copy

> **Deferred to post-V1.** V1 does not ship selected copy (cherry-pick); this
> design is retained for the future op, and the op's absence is guarded by
> `branch_merge_absence.rs`.

Selected copy applies an explicit subset of source state or source changes into
a target branch.

V1 should keep two modes distinct:

1. Copy selected current records/entities from source to target.
2. Apply selected changes discovered by compare/preview.

Rules:

1. Selection must be explicit.
2. Source branch remains unchanged.
3. Target branch receives a new commit when mutations apply.
4. Missing selected records, tombstones, and history-trimmed selections must
   produce structured outcomes.
5. Capability adapters own entity-level expansion for graph, vector collection,
   event stream, JSON path, and relationship-layer selections.
6. Copying relationship-bound graph data must preserve branch/space-relative
   binding semantics or report unresolved references.
7. Derived-state effects must be explicit.

Selected copy should not silently become whole-branch promotion.

## Restore And Undo

> **Deferred to post-V1.** V1 does not ship restore/undo (revert); this design is
> retained for the future op, and the op's absence is guarded by
> `branch_merge_absence.rs`.

Restore or undo writes compensating changes to the selected branch.

V1 restore covers:

1. Restore selected records/entities to an earlier retained version.
2. Undo changes made in a version range when current state still allows a safe
   compensating write.

Rules:

1. Restore does not erase history.
2. Version ranges must be validated before planning.
3. The operation must preserve later work by default. If a record changed after
   the range, the default behavior is to skip or conflict rather than overwrite
   silently.
4. Capability adapters decide how to restore compound entities such as JSON
   paths, graph relationship facts, vector collection metadata, and event
   streams.
5. Deleted/restored/skipped/conflicted entities must be reported separately.
6. Derived-state effects must be explicit.

Any destructive history-rewrite feature is out of scope for V1.

## Delete Branch

Branch delete is destructive in V1 unless a later product decision adds archive.

V1 lifecycle states:

1. Active.
   Reads and writes may proceed according to the handle's access mode.

2. Deleting.
   New writes are refused, in-flight writes are quiesced or rejected, and
   inspection sees a typed deleting/deleted state rather than absence.

3. Deleted with cleanup debt.
   The branch name and generation remain reserved until cleanup either completes
   or the implementation proves stale rows cannot attach to a future generation.

4. Deleted clean.
   Authoritative branch-control state records deletion and no stale
   branch-local source, control, derived, observer, or cleanup debt can attach to
   the same name without a new generation.

Rules:

1. Default, system, protected, or active internal branches require explicit
   protection checks.
2. Delete must reject or quiesce in-flight writes to the branch.
3. Delete must transition branch lifecycle metadata before or atomically with
   storage cleanup so races see a typed deleted/conflict state.
4. Delete must use generation guards or equivalent to protect same-name
   recreation.
5. Same-name recreation is allowed only after the old generation is deleted clean
   or after generation isolation proves all remaining cleanup debt is unreachable
   from the new branch.
6. Branch-local control-plane rows, derived rows, relationship reverse maps,
   shadow vectors, search projections, and `0x45` records must be cleaned,
   quarantined, or marked stale according to owning contracts.
7. Tables, inherited layers, or storage objects still reachable from other
   branches must not be physically deleted by engine branch logic.
8. Losing sides of delete races must not emit orphan lineage edges, observer
   events, or derived-state cleanup for the wrong generation.

Delete outcome should report:

1. Deleted branch ref and generation.
2. Cleanup completed vs cleanup debt.
3. Derived-state disposition.
4. Protected or blocked reasons if delete refused.

## Capability Branch Adapter Requirements

Every data capability must define branch behavior.

The adapter must be able to answer:

1. Which authored rows belong to this capability?
2. Which metadata rows branch with authored data?
3. Which derived rows are rebuildable, staleable, droppable, or authoritative?
4. What is the logical entity identity for comparison and conflict reporting?
5. How are tombstones interpreted?
6. How does latest, version, and timestamp visibility affect comparison?
7. What conflicts can this capability produce?
8. What does strict do?
9. What does source-wins do, if supported?
10. What selected-copy scopes are valid?
11. What restore scopes are valid?
12. What branch-delete cleanup is required?
13. How are relationship-layer references preserved or diagnosed?

Adapter rules:

1. Adapters interpret capability rows. They do not orchestrate full branch
   workflows.
2. Adapters return planned row mutations or diagnostic facts to branch
   workflows. They do not commit directly.
3. Adapters must not call sibling capability internals.
4. Adapters must not bypass persistence to read storage.
5. Adapters must reject malformed capability bytes with structured diagnostics.
6. Adapters must not hide unsupported behavior by falling back to byte equality
   when that would be misleading.
7. Adapters must make derived-state disposition explicit.

## Capability-Specific Minimums

### KV

KV is the reference branch adapter.

Minimum behavior:

1. Compare by space and key.
2. Conflict when source and target changed the same key differently since the
   branch point.
3. Source-wins overwrites or deletes the target key and reports it.
4. Restore writes the prior value or tombstone.

### JSON

Minimum behavior:

1. V1 merge granularity is document-level.
2. Compare by document identity and document version/timestamp.
3. Conflict when source and target changed the same document differently since
   the branch point.
4. Source-wins overwrites or deletes the target document and reports it.
5. Preserve or rebuild JSON secondary rows according to their lifecycle.
6. Path-level disjoint merge may be added later only after a JSON-specific
   merge contract and conformance suite exist.

### Event

Minimum behavior:

1. Treat event records as append-ordered data.
2. Preserve event ordering and chain/integrity facts where the event contract
   requires them.
3. V1 source-wins must refuse divergent appends.
4. Source-wins must not break append-only or hash-chain semantics.
5. A later event-specific divergence contract may define safe reorder-free
   classes, but V1 does not source-wins-apply divergent event histories.

### Vector

Minimum behavior:

1. Compare collection configs separately from vector records.
2. Conflict on metric, dimension, or incompatible collection metadata changes.
3. Treat user-authored vectors as source rows.
4. Treat ANN indexes and acceleration rows as derived unless a later contract
   makes a specific row authoritative.

### Graph

Minimum behavior:

1. Compare graph metadata, nodes, edges, ontology, and relationship bindings.
2. Preserve graph invariants when promoting or selected-copying graph facts.
3. Diagnose dangling/deleted/history-trimmed relationship targets.
4. Treat graph reverse maps and traversal accelerators as derived unless a later
   contract makes a specific row authoritative.
5. Preserve branch-relative and space-relative relationship bindings across
   branch creation and selected copy.
6. Until a richer traversal-copy contract exists, selected graph copy applies
   only explicitly selected graph facts and their required authored graph
   metadata. Copying bound source entities, neighboring edges, or traversal
   closures requires explicit user selection.

### Control Plane

Branch-local control-plane rows participate when they define branch-local
behavior, such as recipes, projection manifests, embedding policy, or
capability state.

Rules:

1. Database-global control rows normally do not participate in user branch
   promotion.
2. Branch-local `_system_` space rows participate according to the owning
   control-plane contract.
3. Registry and format/cutover rows are not ordinary branch data.
4. Branch catalog and lineage rows are updated by branch workflows, not by data
   capability adapters.

### Derived State

Derived state includes search rows, shadow vectors, vector indexes, graph
indexes, retrieval projections, and `0x45` health/watermark rows.

Rules:

1. Derived state is not user-authored data by default.
2. Derived state should usually rebuild or mark stale after branch workflows.
3. Derived state may be copied only when the owning contract can validate it
   against source rows and timeline bounds.
4. If derived state is omitted, matching `0x45` health/watermark rows must be
   dropped or reinitialized.

## Conflict Strategies

V1 strategies:

1. Strict.
   Refuse promotion or selected-change apply when any conflict exists.

2. Source wins.
   Apply source-side values or tombstones for conflicts and report each target
   entity overwritten or deleted.

Rules:

1. Strict should be the default.
2. Source wins must be explicit.
3. Source wins does not bypass structural or referential-integrity conflicts.
4. A capability may refuse source wins for a conflict class that would corrupt
   capability invariants.
5. Additional strategies require their own capability conformance matrix before
   becoming V1 product surface.

The current `LastWriterWins` name is compatibility vocabulary. The product
strategy is source wins.

## Diagnostics And Errors

Branch diagnostics should include:

1. Branch not found.
2. Branch already exists.
3. Branch deleted, archived, protected, or generation-mismatched.
4. Branches unrelated for promotion.
5. Branch point unavailable or history trimmed.
6. Timestamp unresolved.
7. Capability unsupported for requested workflow.
8. Capability conflict.
9. Derived-state stale, rebuilding, omitted, or authoritative.
10. Read-only or write-disabled handle.
11. Cleanup debt after delete.
12. Ambiguous commit outcome during a mutating branch workflow.

Diagnostics must preserve enough context for users and tests:

1. Operation name.
2. Source and target branch refs.
3. Branch point when resolved.
4. Capability and space.
5. Entity/key.
6. Conflict strategy.
7. Commit version/timestamp on success.
8. Derived-state disposition.

Display output must follow the V1 redaction rules.

## Forbidden Dependencies And Shortcuts

Branch workflow production code must not:

1. Import storage-next directly.
2. Construct physical storage keys directly.
3. Use raw numeric storage-space IDs outside the registry/persistence boundary.
4. Decode capability values without going through the owning capability adapter.
5. Treat derived rows as source rows by default.
6. Let callers provide synthetic merge bases for ordinary promotion.
7. Use ambient current branch or space state for planning.
8. Rewrite history to implement restore.
9. Delete storage objects directly instead of using persistence/storage
   reachability mechanics.
10. Preserve tags, notes, or legacy branch bundles as V1 branch requirements.

Allowed exceptions:

1. Tests that intentionally characterize legacy behavior before cutover.
2. Migration tools with explicit documentation.
3. Temporary implementation shims listed in a cleanup plan with removal gates.

## Conformance Tests

Branch conformance tests should prove:

1. Create-from-current records source branch, generation, and branch point.
2. Create-from-version fails when history is trimmed.
3. Create-from-time resolves one retained version frontier before row reads.
4. Compare groups results by capability and space and omits derived rows by
   default.
5. Preview promotion produces conflicts without mutating source or target.
6. Strict promotion fails with no target mutation when conflicts exist.
7. Source-wins promotion reports overwritten/deleted target entities.
8. Promotion records authoritative lineage with the committed target state or a
   recoverable workflow intent.
9. Promotion cannot expose target data changes without authoritative lineage or
   a recoverable workflow intent.
10. Crash/reopen after storage fork but before branch-control publication cannot
    produce an ordinary visible branch without complete metadata.
11. Crash/reopen after target data commit but before observer/projection updates
    reconstructs lineage from committed branch-control rows.
12. Selected current-record copy and selected-change apply are distinguishable.
13. Restore writes compensating changes and preserves later work by default.
14. Branch delete rejects protected branches.
15. Branch delete cannot race ordinary writes into silent success.
16. Crash/reopen after delete lifecycle transition but before cleanup resumes or
    reports cleanup debt without allowing stale same-name inheritance.
17. Same-name branch recreation cannot inherit stale source, control, or derived
    rows.
18. Capability adapters reject malformed value bytes with structured
    diagnostics.
19. KV, JSON, event, vector, and graph pass shared branch adapter tests.
20. Event divergence cannot be source-wins-applied when it violates event
    invariants.
21. Graph relationship bindings remain branch/space-relative after branch
    creation and selected copy.
22. Derived rows rebuild, mark stale, drop, or validate according to their
    owning contracts.
23. `0x45` rows are dropped or reinitialized when derived row families are
    omitted.
24. Read-only handles reject mutating branch workflows before storage mutation.
25. Branch workflows use persistence, not direct storage imports.
26. Branch operation diagnostics preserve source chains, stable error codes,
    and redaction.

## Deferred Questions And Closed V1 Baselines

1. Exact Rust names.
   This contract uses conceptual names. Implementation should keep the
   vocabulary small and avoid one branch trait per operation unless tests prove
   it is necessary.

2. Empty branch product status.
   Closed for V1: empty branch creation is required. It creates branch metadata
   and required control-plane rows without copying user data.

3. Selected copy API split.
   V1 should distinguish current-record copy from selected-change apply. The
   exact public API shape belongs in the public API/CLI cleanup checklist.

4. Event divergence policy.
   Closed for V1: divergent event appends are refused under source-wins.
   Event-specific source-wins classes are post-V1.

5. Graph relationship copy depth.
   The V1-safe default is explicitly selected graph facts only. Copying bound
   source entities, related edges, or a traversal closure requires an explicit
   API mode and conformance tests.

6. Control-plane row coverage.
   The control-plane layout contract must define which branch-local system rows
   participate in compare, promotion, copy, restore, and delete cleanup.

7. Archive versus delete.
   V1 currently treats delete as destructive. Archive can be added later as a
   separate product workflow.

8. Large comparison pagination.
   V1 comparison must provide bounded output. Rich pagination and stable cursors
   are product/API details that can land after the safe bounded path exists.

## V1 Minimum

For V1, the minimum acceptable implementation is:

1. Branch workflows are engine-owned product operations over persistence.
2. Create branch from current state, retained version, and retained timestamp
   are supported for V1 storage modes. They may fail only with explicit
   diagnostics such as retained history unavailable, timestamp unresolved,
   backend capability mismatch, or storage corruption.
3. Compare branches by capability and space.
4. Preview promotion derives branch point from lineage and reports conflicts.
5. Promote supports strict and explicit source-wins strategies.
6. Delete protects default/system branches and prevents same-name stale-state
   inheritance.
7. KV, JSON, event, vector, and graph define capability branch adapters.
8. Derived state disposition is explicit for branch workflows.
9. Branch-local control-plane metadata is handled through control-plane
   contracts, not ad hoc row scans.

Selected copy (cherry-pick) and restore/undo (revert) are **not** part of the V1
minimum — they are deferred to post-V1 (see the "Selected Copy" and "Restore"
sections), and their absence is guarded by `branch_merge_absence.rs`.
12. Tags, notes, and legacy branch bundles do not define the V1 branch model.
13. Branch workflow tests cover lineage, conflict strategy, temporal frontiers,
    capability coverage, derived-state cleanup, delete races, crash/recovery
    windows, and diagnostics.

## Next Step

The temporal context and timeline resolver contract is defined in
`docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`.

The next contract should be the control-plane layout contract. It should define
where branch metadata, capability registries, storage-space registries, derived
state status, provenance, and temporal metadata live.
