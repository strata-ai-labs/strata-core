# L6I Implementation Plan: Reachability And Shared Table Refs

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-test-plan.md`

## Objective

Add storage-level branch/table reachability facts and a rebuildable runtime
shared-table registry to storage-next L6.

L6I makes branch ownership and copy-on-write table sharing explicit. It lets L6
answer:

1. which immutable tables are reachable from one branch;
2. which inherited layers reference source tables;
3. which materialization state still protects source tables;
4. which tables are protected by any branch or inherited layer;
5. which tables may be offered to L8 as release candidates after a branch,
   inherited layer, or branch-owned level is removed.

L6I is not a durable deletion or garbage-collection slice. Durable reachability
publication is L4/L8. Physical deletion, quarantine, and repair are L8. L6I
only produces deterministic storage-owned facts and validates/rebuilds the
runtime accelerator from those facts.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/future-object-durable-guardrails.md`
4. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
10. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
11. `crates/storage-next/src/table/{facts.rs,reader.rs,key.rs}`
12. `crates/storage/src/segmented/ref_registry.rs`
13. `crates/storage/src/segmented/tests/fork.rs`
14. `crates/storage/src/segmented/tests/lifecycle.rs`
15. `crates/storage/src/segmented/tests/concurrency.rs`
16. `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`

## Existing-Code Source Map

| Current file | L6I evidence | L6I action |
|---|---|---|
| `crates/storage/src/segmented/ref_registry.rs` | Runtime refcount accelerator, deletion barrier, and explicit warning that refcount zero is not a durable deletion proof. | Rebuild the accelerator over storage-next table identities. Keep the same "accelerator only" contract. |
| `crates/storage/src/segmented/tests/fork.rs` | Fork increments shared segment refs, rejects failed fork refcount leaks, and validates chained COW sharing. | Port as branch/table reference facts and generated registry rebuild tests, not filesystem segment tests. |
| `crates/storage/src/segmented/tests/lifecycle.rs` | Branch clear/delete releases inherited refs and preserves parent-owned tables still referenced by children. | Port to L6 release-fact planning. L8 will later own actual cleanup/quarantine. |
| `crates/storage/src/segmented/tests/concurrency.rs` | Races between fork refcount increments and deletion candidate selection are guarded by a barrier. | Represent the race as an L6 invariant: registry mutation is atomic with respect to reachability snapshots. Async locking remains above L6. |
| `crates/storage/src/segmented/tests/quarantine_reconciliation.rs` | Durable manifest reachability beats runtime refcount disagreement. | Preserve the source-of-truth rule: manifest/reachability facts determine safety; registry disagreement blocks release. |
| `crates/storage-next/src/branch/facts.rs` | `BranchReachabilityFacts` currently records only owned/inherited counts. | Expand into deterministic table-reference facts and release/protection facts. |
| `crates/storage-next/src/branch/state.rs` | Branch state owns active/frozen rows, owned immutable levels, inherited layers, and materialization transitions. | Add reachability snapshot and release-delta helpers over existing branch state, without backend IO. |

## Scope

L6I implements:

1. a storage-owned table-reference vocabulary for branch-owned and inherited
   table reachability;
2. deterministic reachability snapshots for one branch;
3. deterministic aggregate reachability over many branches;
4. a rebuildable runtime shared-table registry over table identities;
5. release facts for removing branch-owned tables, inherited layers, and whole
   branch-local state;
6. protection facts that explain why a table is not releasable;
7. materialization release facts for source tables removed from an inherited
   layer only after replacement child-owned reachability is present;
8. validation for duplicate, missing, mismatched, stale, and corrupt
   reachability facts;
9. rebuild-from-manifest model hooks that accept already-decoded
   storage-owned branch/table reachability records;
10. generated branch-LSM model counters and source-guard updates.

L6I does not implement:

1. table object publication;
2. branch manifest publication;
3. backend IO, filesystem deletion, quarantine, or repair;
4. WAL-before-visible discipline;
5. lifecycle scheduling;
6. compaction candidate selection or compaction output install;
7. snapshot row install;
8. product branch delete/clear policy;
9. StrataHub export/push behavior;
10. public storage API exposure.

## Core Rule: Reachability Facts Beat Runtime Refcounts

Durable branch/table reachability facts are the source of truth. The runtime
registry is an acceleration structure.

The registry can answer "is any runtime reference known right now?" It cannot
prove deletion safety by itself. A table may be offered to L8 as a release
candidate only when:

1. the table is absent from the current aggregate reachability snapshot;
2. the table is not protected by any active inherited layer;
3. the table is not protected by a materialization transition whose source
   layer removal is not yet safely represented;
4. the runtime registry either agrees that no refs remain or reports a
   recoverable disagreement that L8 must reconcile before deletion.

If durable reachability and the runtime registry disagree, L6I must report a
typed disagreement/protection fact. It must not silently treat the table as
safe to delete.

## Reachability Vocabulary

The exact Rust names may change, but L6I should add equivalents of:

```text
BranchTableRef
  table_identity
  owner_branch_id
  table_branch_id
  level
  table_index
  reference_kind

BranchTableReferenceKind
  Owned
  Inherited { source_branch_id, fork_version, layer_index }
  MaterializingSource { source_branch_id, fork_version, layer_index }
  Replacement { materialization_layer_index }

BranchReachabilitySnapshot
  branch_id
  table_refs sorted by stable key
  owned_table_count
  inherited_table_count
  protected_table_count

SharedTableRegistry
  table_identity -> sorted reference owners / ref count

BranchReleasePlan
  released_branch_id
  removed_refs
  releasable_tables
  protected_tables
  registry_disagreements
```

Reference facts must use table identities and branch ids, not object paths or
layout strings. They must not carry row value bytes.

## Deterministic Ordering

Every emitted collection must be deterministic:

1. table identity ascending;
2. reference kind in a documented stable order;
3. owner branch id bytes ascending;
4. inherited layer index ascending;
5. branch level ascending;
6. table index ascending.

Generated tests should fail if the same logical state emits different
reachability bytes/facts due to insertion order.

## Branch Reachability Snapshot

`BranchLocalState` should expose a reachability snapshot helper that:

1. includes every branch-owned immutable table in owned levels;
2. excludes active/frozen mutable rows, because they are not durable table
   objects yet;
3. includes every table reachable through each active/materializing inherited
   layer;
4. excludes materialized layers that no longer protect source tables;
5. records table source facts without reading table bytes;
6. validates descriptor/table fact consistency before emitting facts;
7. reports counts and table identities consistently with existing branch facts.

The snapshot is a fact surface for L8/L4 publication. It does not publish.

## Aggregate Reachability

Add a model/API that consumes branch reachability snapshots and produces an
aggregate reachability view:

1. all table identities currently reachable;
2. all references per table;
3. shared tables with more than one reference;
4. tables reachable only from one branch-owned level;
5. tables protected by inherited layers;
6. invalid duplicate references when a single branch snapshot repeats the same
   reference identity in an impossible way;
7. stable digest/fingerprint inputs for future durable manifests, if useful.

The aggregate should be rebuildable from decoded manifest facts. L6I should not
parse object names or manifest bytes.

## Runtime Shared-Table Registry

The registry is an in-memory accelerator over decoded reachability facts.

It should support:

1. `rebuild_from_snapshots`;
2. `register_snapshot`;
3. `unregister_snapshot`;
4. `replace_snapshot` for atomic same-branch reachability transitions;
5. `reference_count(table_identity)`;
6. `is_runtime_referenced(table_identity)`;
7. `release_plan_for_removed_refs`;
8. clear/reset for recovery.

The registry must:

1. avoid underflow on over-release;
2. deduplicate identical refs from the same snapshot;
3. reject mismatched branch/table ownership facts;
4. be deterministic under repeated rebuilds;
5. classify disagreement with aggregate reachability as a protection fact.

This registry should remain single-process and storage-local. Cross-process or
object-store leases are outside L6I.

## Release Facts

L6I should produce release plans for:

1. removing a branch-owned table from a level;
2. replacing an owned level view;
3. clearing a branch's local state;
4. deleting a branch's storage state;
5. removing an inherited layer after materialization;
6. removing all inherited layers during branch clear/delete;
7. rebuilding after recovery discovers missing or stale refs.

Release plans classify each removed reference as:

1. `StillReachable` by at least one other branch/layer;
2. `RuntimeReferenced` by the accelerator despite no durable reachability;
3. `RegistryDisagreement` when runtime and aggregate facts disagree;
4. `ReleasableCandidate` when no current fact protects it;
5. `InvalidPlan` when the removed reference was not present.

The "candidate" word is intentional. L8 still decides whether to delete,
quarantine, retain, or repair the table object.

## Materialization Interaction

After L6H materializes a layer:

1. replacement child-owned tables become reachable as owned L0 tables;
2. the removed inherited layer source tables are no longer protected by that
   layer;
3. source tables may still be protected by other child branches, other
   inherited layers, source branch ownership, or a materialization recovery
   fact;
4. L6I emits release facts for the removed inherited refs only after the
   replacement reachability is part of the branch snapshot.

L6I must not release source-table refs merely because materialization started.

## Fork Interaction

Fork reachability must be all-or-nothing at the L6 fact boundary:

1. destination inherited refs are staged before destination branch visibility;
2. rejected fork attempts leave no registry increments;
3. successful fork snapshots include the inherited source table refs;
4. chained forks preserve nearest-first inherited layer order in reachability
   facts;
5. source tables remain protected even if the source branch later removes or
   compacts its own table view.

## Branch Clear/Delete Interaction

L6I should add storage-local clear/delete planning helpers, not product policy.

A branch clear/delete release plan:

1. removes active/frozen rows from local state facts, but does not report them
   as table releases;
2. removes branch-owned immutable table refs;
3. removes inherited layer refs;
4. preserves any table still referenced by another branch/layer;
5. reports releasable candidates for tables with no remaining refs;
6. is all-or-nothing at the in-memory branch-state boundary.

Engine decides whether a branch is allowed to be cleared or deleted. L8 decides
when to publish the state and reclaim table objects.

## Implementation Steps

### L6I-A: Add Reachability Fact Types

1. Extend `branch/facts.rs` with table reference kinds, reference records,
   reachability snapshots, aggregate facts, release/protection facts, and
   registry disagreement errors.
2. Keep all types `pub(crate)`.
3. Add constructors that validate branch ids, table identities, counts, and
   stable ordering invariants.
4. Add display/debug discipline that never includes row values.

Exit: L6 can express table reachability without a registry or state mutation.

### L6I-B: Emit Branch Snapshots From BranchLocalState

1. Add `BranchLocalState::reachability_snapshot` or equivalent.
2. Include owned immutable tables and inherited layer tables.
3. Exclude active/frozen mutable rows.
4. Validate descriptor/fact consistency.
5. Sort and deduplicate emitted references deterministically.

Exit: one branch can produce complete deterministic reachability facts.

### L6I-C: Add Aggregate Reachability

1. Add an aggregate builder over many branch snapshots.
2. Detect shared tables, single-owner tables, duplicate refs, impossible
   branch ownership, and empty snapshots.
3. Provide stable accessors for table refs by table identity and branch id.

Exit: L6 can compute current global table protection from decoded branch facts.

### L6I-D: Add Runtime SharedTableRegistry

1. Add rebuild/register/unregister/reference-count helpers.
2. Prevent underflow and classify over-release.
3. Treat duplicate identical refs idempotently or reject them according to the
   chosen fact contract.
4. Keep registry state rebuildable from snapshots.

Exit: runtime refcounts exist as an accelerator, not an authority.

### L6I-E: Add Release Planning

1. Produce release plans for removed branch-owned refs, inherited refs, and
   whole branch clear/delete.
2. Classify releasable candidates versus protected tables.
3. Detect registry/aggregate disagreement.
4. Preserve deterministic output ordering.

Exit: L6 can tell L8 what changed and what may be considered for reclaim.

### L6I-F: Wire Fork And Materialization Facts

1. Ensure fork-created inherited layers emit shared refs.
2. Ensure failed fork planning leaves no registry changes.
3. Ensure materialization emits replacement-owned refs before removed inherited
   refs become release candidates.
4. Ensure stale/materializing/materialized statuses protect refs according to
   recovery semantics.

Exit: L6H/L6F transitions produce correct reachability deltas.

### L6I-G: Generated Tests, Guards, And Porting Log

1. Extend `BranchLsmScaffoldOutcome` with reachability counters.
2. Add generated fork/materialization/clear scripts that compare production
   facts with an independent reachability model.
3. Update `branch_lsm_source_guard.rs` to allow reachability/registry
   vocabulary while still rejecting backend, lifecycle, commit, and product
   imports.
4. Update `m4-l6-porting-log.md` with preserved old refcount behavior,
   changed durable-boundary ownership, deferred L8 cleanup, and sensitivity
   probes.

Exit: direct, generated, source-guard, wasm/no-default, clippy, and hygiene
checks pass.

## Deferred

1. Durable manifest byte format changes.
2. L4 table manifest publication calls.
3. Crash recovery orchestration.
4. Quarantine of orphan/unreachable tables.
5. Physical object deletion.
6. Branch compaction install and old-table release after compaction.
7. Snapshot row install reachability.
8. Public branch delete/clear API.
9. Cross-process table leases.
10. StrataHub export manifests.

## Verification Commands

Run at least:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If L6I touches table identity or table descriptor helpers, also run:

```bash
cargo test -p strata-storage-next --locked --lib table
```

## Exit Criteria

L6I is complete when:

1. every branch can emit deterministic table reachability snapshots;
2. aggregate reachability can be rebuilt from snapshots;
3. runtime shared-table refcounts are rebuildable acceleration state only;
4. release plans never mark a table releasable while any branch/layer protects
   it;
5. materialization and fork transitions produce correct reachability deltas;
6. branch clear/delete planning emits protected and releasable candidates
   without doing cleanup;
7. generated tests cover shared-reference safety;
8. source guards enforce L6 boundaries;
9. the porting log records old refcount behavior as preserved, rewritten,
   retired, or deferred.
