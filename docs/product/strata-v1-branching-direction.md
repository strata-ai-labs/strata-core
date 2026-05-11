# Strata V1 Branching Direction

Status: Draft product direction

This document defines how Strata should describe branching for V1. The current
implementation has commands named after Git operations: fork, diff, merge,
merge-base, revert, and cherry-pick. Those names are useful implementation
shorthand, but they are not enough as product requirements.

Strata branches are database workspaces and timelines. The user should
understand what state is created, compared, copied, promoted, or undone without
having to import Git's mental model.

## Thesis

Branching is a core Strata capability because users need safe ways to work with
data state:

1. Start an experiment from existing data.
2. Modify data without changing the base branch.
3. Compare two lines of state.
4. Promote a finished branch into another branch.
5. Copy selected records or selected changes between branches.
6. Undo a bad range by writing a compensating change.
7. Keep cloned datasets useful offline.

The product should explain those activities directly. Git names may remain in
internal code or compatibility APIs, but V1 documentation and future API design
should prefer database-native language.

## Product Vocabulary

### Branch

A branch is a named line of database state. It contains user data, branch-local
configuration, and derived state that belongs to that line of state.

A branch is not a remote collaboration object by itself. StrataHub may later add
dataset forks, public releases, publishing, and review workflows. Local Strata
branching should remain a reliable embedded-database capability.

### Version

A version is a committed point in database history. A branch advances through
versions as writes occur.

Users may use versions to read historical values, compare state, restore earlier
values, and explain how a branch changed.

### Branch Point

A branch point is the version of a source branch from which another branch was
created. It is the baseline used to explain what changed on either side.

Users do not need to operate on branch points directly in common workflows, but
Strata should expose enough information to explain comparisons and promotions.

### Workspace

A workspace is the user-facing role of a branch: an isolated place to make
changes. Examples include:

1. Try a data cleanup.
2. Build a RAG index.
3. Normalize JSON documents.
4. Add graph relationships.
5. Test a schema or ontology update.
6. Curate a cloned dataset.
7. Run an agent task with isolated side effects.

## Current Implementation Evidence

The current command surface already has most of the required mechanics:

1. `BranchCreate`
2. `BranchGet`
3. `BranchList`
4. `BranchExists`
5. `BranchDelete`
6. `BranchFork`
7. `BranchDiff`
8. `BranchDiffThreeWay`
9. `BranchMergeBase`
10. `BranchMerge`
11. `BranchRevert`
12. `BranchCherryPick`

The engine also has branch lifecycle records, branch lineage, merge-base
calculation, data-capability-aware merge hooks, same-name race protection,
branch DAG hooks, and branch operation observers.

This is a good substrate. The product work is to name the user activities
correctly and tighten the guarantees.

## User Activities

### Select Working Context

User activity:

1. Choose the branch and space that subsequent reads and writes should use.
2. Override branch or space per command when needed.
3. See the active context in CLI or SDK diagnostics.

Required semantics:

1. The default branch must be explicit.
2. Branch and space are separate concepts.
3. Read-only opens must preserve context while rejecting writes.
4. A missing branch should fail before mutation.

Current evidence:

1. Data commands accept optional branch fields that default to `default`.
2. CLI session state can track current branch and space.

V1 direction:

1. Product docs should describe this as context selection, not branch mutation.
2. Context selection should never imply branch creation.

### Create A Workspace From Existing Data

User activity:

1. Start a new branch from an existing branch.
2. Make changes in the new branch without changing the source branch.
3. Use the new branch as an experiment, task workspace, or dataset variant.

Required semantics:

1. The destination branch starts from a specific source branch state.
2. The source branch remains unchanged.
3. The branch point is recorded.
4. The operation is atomic from the user's perspective.
5. Derived branch-local state is either copied, rebuilt, or explicitly marked as
   needing rebuild.

Current evidence:

1. `BranchFork` creates a destination branch from a source branch.
2. `ForkInfo` reports source, destination, copied-key counts, copied-space
   counts, and fork version.

V1 direction:

1. Product language should say "create branch from" or "create workspace from",
   not "fork" as the primary term.
2. If an empty branch remains supported, document it separately as "create empty
   branch." It should not be confused with branching from existing data.
3. Branch creation from a retained version and from a timestamp resolved to a
   retained version is V1 required because it is part of the time-travel product
   promise. It must fail before visible branch metadata is published if Strata
   cannot prove the retained branch point or derived-state status.

### Inspect Branches

User activity:

1. List available branches.
2. Check whether a branch exists.
3. Inspect branch status, creation time, update time, and current version.
4. Understand where a branch came from when lineage is available.

Required semantics:

1. System branches should not appear as ordinary user branches.
2. Deleted or archived branches should have clear visibility rules.
3. Branch names should have validation rules.
4. Branch metadata should not include tags or notes as V1 requirements.

Current evidence:

1. `BranchGet`, `BranchList`, and `BranchExists`.
2. Branch lifecycle records and status.
3. Reserved system-branch filtering in executor handlers.

V1 direction:

1. Branch inspection should be product-facing.
2. Merge-base and lineage details can appear as explanation fields without
   becoming primary commands.

### Compare Branches

User activity:

1. See what changed between two branches.
2. Filter comparison by data capability and space.
3. Use summaries before loading full details.
4. Compare branch state at a point in time where supported.

Required semantics:

1. Comparison results should be organized by space and data capability.
2. Added, removed, and modified records should be clear from the user's
   perspective.
3. Binary keys and values need stable display and programmatic forms.
4. Large diffs need pagination or bounded output before V1 scale claims.
5. Derived indexes should not appear as user data unless the product explicitly
   treats them as branch-visible records.

Current evidence:

1. `BranchDiff` reports per-space added, removed, and modified entries.
2. Diff options can filter by primitive type and space.
3. Diff can accept an `as_of` timestamp.

V1 direction:

1. Product language should say "compare branches."
2. The low-level word "diff" can remain in API names, but docs should explain
   concrete output.
3. Comparison must define coverage for KV, JSON, events, graph, vectors,
   recipes, auto-embedding shadow data, and search-derived state.

### Preview Promotion

User activity:

1. Ask whether a source branch can be applied to a target branch.
2. See conflicts before mutating the target.
3. Understand what both sides changed since the shared branch point.

Required semantics:

1. The shared branch point must be derived by Strata, not supplied by the user.
2. Unrelated branches should fail with a clear error unless the product later
   defines an explicit adoption or transplant workflow.
3. Conflict entries should name data capability, space, key or entity, source
   value, target value, and why the conflict exists.

Current evidence:

1. `BranchDiffThreeWay` explains source and target differences from the merge
   base.
2. `BranchMergeBase` exposes the shared base.
3. Merge prechecks exist for data-capability-specific behavior.

V1 direction:

1. Product language should say "preview promotion" or "preview combine."
2. `merge-base` should be an explanation detail or diagnostic, not a primary
   user pathway.
3. Three-way comparison should be documented through promotion/conflict
   planning rather than Git vocabulary.

### Promote A Branch

User activity:

1. Apply completed changes from a source branch into a target branch.
2. Keep the source branch available after promotion.
3. Receive a summary of applied records, deletions, spaces, conflicts, and the
   new target version.

Required semantics:

1. Promotion mutates the target branch by creating a new commit/version.
2. The source branch is not mutated.
3. Strata must derive the branch point from recorded lineage.
4. Promotion should be all-or-nothing.
5. Conflict strategy must be explicit.
6. Data-capability-specific merge behavior must be documented.

Current evidence:

1. `BranchMerge` applies a source branch into a target branch.
2. Current strategies are `Strict` and `LastWriterWins`.
3. `MergeInfo` reports applied keys, deleted keys, spaces merged, conflicts,
   and merge version.

V1 direction:

1. Product language should say "promote branch" or "apply branch to target."
2. `LastWriterWins` is a poor user-facing name for branch promotion. The product
   strategy is closer to "source wins on conflict."
3. `Strict` should be the safer default unless product review chooses otherwise.
4. Promotion should explain which data capabilities were fully covered and which
   were skipped, rebuilt, or unsupported.

### Copy Selected Records Or Changes

User activity:

1. Copy specific records from one branch to another.
2. Copy only selected spaces.
3. Copy only selected data capabilities.
4. Apply a small subset of useful work without promoting the whole branch.

Required semantics:

1. Selection must be explicit.
2. The operation mutates only the target branch.
3. The source branch remains unchanged.
4. The result should report applied records, deleted records, and new target
   version.
5. Missing selected records and tombstones need documented behavior.

Current evidence:

1. `BranchCherryPick` can accept explicit `(space, key)` pairs.
2. It can also apply diff-selected changes filtered by space, key, and primitive
   type.
3. `CherryPickInfo` reports applied keys, deleted keys, and version.

V1 direction:

1. Product language should say "copy selected records" or "apply selected
   changes", not "cherry-pick."
2. The current command mixes two activities:
   - copy current selected records from source to target
   - apply selected changes found by branch comparison
3. V1 should either split these concepts or document the mode clearly.
4. Selection should eventually support richer entity scopes, such as graph,
   vector collection, event type, JSON document, or relationship-layer entity
   reference.

### Undo A Bad Range

User activity:

1. Choose a branch and a version range.
2. Restore records touched in that range to the values they had before the
   range.
3. Preserve later work when records changed again after the range.
4. Get a summary of what was restored.

Required semantics:

1. Undo writes a new compensating change. It does not erase history.
2. The selected version range must be validated.
3. Records modified after the range should be preserved unless the user
   explicitly chooses a stronger destructive mode.
4. Deletions and restorations must be reported separately where possible.

Current evidence:

1. `BranchRevert` accepts a branch and inclusive version range.
2. Current implementation restores keys only when current state still matches
   the state at the end of the selected range.
3. `RevertInfo` reports reverted count and revert version.

V1 direction:

1. Product language should say "undo range" or "restore affected records."
2. The docs must be clear that this is not history rewriting.
3. Record-level skipped cases should be visible enough for users to trust the
   result.

### Delete A Branch Safely

User activity:

1. Delete a branch that is no longer needed.
2. Avoid deleting protected branches by accident.
3. Ensure branch-local data and derived state do not leak into future branches
   with the same name.

Required semantics:

1. Default and system branches must have explicit protection rules.
2. Delete must be atomic from the user's perspective.
3. In-flight writes must not race branch deletion.
4. Branch-local indexes, sidecars, graph/search/vector derived state, and
   metadata must be cleaned or quarantined according to documented rules.
5. Same-name recreation must not inherit stale data.

Current evidence:

1. `BranchDelete`.
2. Branch deletion quiesce/drain/OCC protection.
3. Branch cleanup hooks for graph, vector, search, and branch metadata.

V1 direction:

1. Branch delete should be documented as destructive.
2. If soft delete, archive, or restore is desired later, it should be a separate
   product feature.

## Data Capability Coverage

Branching only works as a product feature if users know what participates.

V1 must define branch behavior for:

1. Key-value records.
2. JSON documents and JSON path updates.
3. Events and event ordering.
4. Graph nodes, edges, ontology, and relationship-layer entity references.
5. Vector collections, vector records, and vector metadata.
6. Branch-local recipes and configuration.
7. Auto-embedding shadow data.
8. Search indexes and derived retrieval state.

The likely product stance is:

1. User-authored data participates in branch comparison and promotion.
2. Branch-local configuration participates where users intentionally authored
   it.
3. Derived indexes should be rebuilt or validated rather than treated as
   primary user data.
4. Auto-generated shadow data should be branch-local, observable, and repairable
   but not confused with authored user records.
5. Data-capability-specific limitations must be visible in comparison,
   promotion, and selective-copy results.

## Conflict Model

A conflict occurs when source and target both changed the same logical entity
since their shared branch point and Strata cannot combine those changes
automatically.

V1 should define conflict behavior by data capability:

1. KV.
   Same key changed differently on both sides.

2. JSON.
   Same document or path changed incompatibly. Disjoint path changes may be
   mergeable if the JSON merge contract supports it.

3. Events.
   Event logs are append-only. Divergent appends may require
   data-capability-specific ordering rules or may be rejected.

4. Graph.
   Node, edge, ontology, and relationship changes need graph-specific conflict
   rules. Relationship-layer entity references make this more important.

5. Vectors.
   Vector collection metadata conflicts, metric or dimension mismatches, and
   record-level changes need explicit rules.

6. Search and derived state.
   Derived state should usually rebuild, not conflict as user data.

Minimum V1 strategies:

1. Strict.
   Fail if any conflict is detected.

2. Source wins.
   Apply source values for conflicts and report what was overwritten.

Any additional strategies should wait until the data-capability-specific rules
are well tested.

V1 does not require a total CRDT merge contract, HLC-stamped rows, replica IDs,
or per-branch sync tombstone GC. Those may be useful for a future explicit sync
design, but V1 local branch promotion should remain engine-owned product
behavior over retained storage history.

## Relationship To StrataHub

Local branches and StrataHub dataset forks are related but distinct:

1. A local branch is an embedded database workspace.
2. A StrataHub dataset fork is a published or tracked dataset lineage.
3. Clone should produce a normal local Strata database with ordinary branches.
4. A user can branch, modify, search, and export a cloned dataset offline.
5. Publishing a derived dataset later should build on local branch lineage, but
   should not change local branch correctness.

This distinction matters because "fork" is overloaded. For V1 product docs,
"create branch from" should describe local database branching. "Fork dataset"
should be reserved for StrataHub or dataset-publication workflows.

## V1 Scope

### Required

V1 should require:

1. Create branch from existing branch state.
2. Create branch from a retained commit version.
3. Create branch from a timestamp resolved to a retained commit version.
4. Create or inspect ordinary branch metadata.
5. Select branch context for reads and writes.
6. Compare branches with summaries and filters.
7. Preview promotion conflicts.
8. Promote a branch into another branch with explicit conflict strategy.
9. Copy selected records or selected changes between branches.
10. Undo a version range by writing a compensating change.
11. Delete branches safely.
12. Document data-capability coverage and limitations.

### Optional For V1

V1 may include:

1. Create an empty branch.
2. Rename branch.
3. Archive branch instead of deleting it.
4. Paginated large-diff browsing beyond bounded V1 comparison output.
5. Conflict-resolution assistance.
6. Rich selective copy by entity reference, graph, vector collection, event
   type, or JSON path.

### Remove Before V1

V1 should not carry these as core branching requirements:

1. Public tags and notes.
2. Legacy branch bundle import/export/validate workflow.
3. User-facing merge-base as a primary command.
4. Git vocabulary as the only explanation of branch behavior.

Tags, notes, and release labels may return later if dataset releases,
provenance, StrataHub publishing, or collaboration workflows need them.

## Architecture Implications

1. Engine owns branch semantics.
2. Storage may expose raw branch-scoped mechanics, but it should not own branch
   product policy.
3. Branch operations must be primitive-aware at the engine layer.
4. Search, graph, vector, and auto-embedding derived state need explicit branch
   cleanup and rebuild contracts.
5. Public APIs should prefer activity names even if internal code keeps existing
   Git-derived names.
6. Testing must cover branch lifecycle, comparison, promotion, selected copy,
   undo, deletion races, derived-state cleanup, and same-name recreation.

## Open Questions

1. Should V1 expose empty branch creation, or only branch-from-existing-state?
2. What user-facing syntax should represent current, version, and timestamp
   branch points?
3. Is strict conflict handling the default for promotion?
4. Should source-wins be renamed and exposed as an advanced strategy?
5. How should event-log divergence be resolved or rejected?
6. Which derived system-space records are visible in branch comparison?
7. Should selective copy operate on current records, changes since base, or two
   explicitly separate modes?
8. How should relationship-layer entity references behave when copied across
   branches?
9. Should delete be permanent in V1, or should archive be the user-facing safe
   operation?
10. What branch metadata is required for StrataHub publishing later?

## Acceptance Criteria

The branching direction is working when all of these are true:

1. A new user can explain what a branch is without knowing Git.
2. A user can create an isolated workspace from existing data.
3. A user can compare branch state by space and data capability.
4. A user can promote completed work into another branch.
5. A user can copy selected records or selected changes without promoting the
   whole branch.
6. A user can undo a bad range without rewriting history.
7. Branch deletion cannot race ordinary writes or leak stale derived state.
8. Data-capability coverage and limitations are explicit.
9. Tags, notes, and branch bundles do not define the V1 branching model.
