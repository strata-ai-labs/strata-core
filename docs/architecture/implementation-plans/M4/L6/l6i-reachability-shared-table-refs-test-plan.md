# L6I Test Plan: Reachability And Shared Table Refs

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-implementation-plan.md`

## Goal

Prove that L6I reports branch/table reachability and shared-table release facts
without crossing into durable publication, backend cleanup, or product branch
policy.

The suite must fail if L6I:

1. omits a branch-owned immutable table from reachability;
2. omits an inherited table from reachability;
3. treats runtime refcount zero as a deletion proof;
4. releases a table still referenced by another branch/layer;
5. releases source tables when materialization starts instead of after
   replacement reachability is visible;
6. leaks registry increments after a rejected fork/rebuild plan;
7. emits nondeterministic reachability facts;
8. includes active/frozen mutable rows as durable table objects;
9. imports backend, service publication, lifecycle, commit, old storage, or
   product DTO APIs into production `branch/` code.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct module-local tests.
2. `crates/storage-next/src/testkit/branch_lsm.rs` for generated reachability
   scripts and independent model checks.
3. `crates/storage-next/tests/branch_lsm_properties.rs` for generated property
   tests behind the `testkit` feature.
4. `crates/storage-next/tests/branch_lsm_source_guard.rs` for source-boundary
   scans.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   minimized generated failure is captured.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   source-map, sensitivity-probe, and closeout notes.

Tests must use storage-next `BranchId`, `CommitVersion`, `StorageRow`,
`BranchLocalState`, `BranchOwnedTable`, `BranchInheritedLayer`, `TableIdentity`,
and table facts. Tests must not use old storage `SegmentId` in new production
surfaces, filesystem paths, object layout strings, backend handles, product
branch names, `VersionedValue`, `Value`, `Key`, `Namespace`, or `TypeTag`.

## Independent Model

Generated tests should compare production facts against a separate model.

Suggested model:

```text
ModelReachability {
  branches: branch_id -> ModelBranchReachability
  table_refs: table_identity -> Vec<ModelTableRef sorted>
}

ModelBranchReachability {
  branch_id
  owned_refs
  inherited_refs
}

ModelTableRef {
  table_identity
  owner_branch_id
  reference_kind
  source_branch_id optional
  fork_version optional
  layer_index optional
  level optional
  table_index
}
```

The model should:

1. collect owned table refs from branch-owned immutable levels only;
2. collect inherited refs from active/materializing inherited layers;
3. ignore active/frozen mutable rows;
4. treat materialized layers as no longer protecting source tables;
5. sort refs deterministically;
6. aggregate refs by table identity;
7. mark a table protected when at least one ref remains;
8. mark a table releasable only when no model ref remains;
9. classify runtime-registry disagreement as protected/needs-reconcile.

The model must not call production aggregate reachability or registry code to
derive expected results.

## Generators

### Branch Graphs

Generate:

1. root branch with no tables;
2. branch with one owned table;
3. branch with multiple owned levels;
4. parent with one child inheriting tables;
5. parent with multiple sibling children sharing the same tables;
6. chained forks with grandparent/parent/child refs;
7. child with multiple inherited layers nearest-first;
8. branch with materializing inherited layer;
9. branch with materialized inherited layer;
10. branch clear/delete operation over owned and inherited refs.

### Table References

Generate table identities over:

1. empty-ish but valid opaque names if the `TableIdentity` contract allows
   them;
2. long names near table identity limits;
3. adjacent names that sort differently only near the end;
4. names shared by multiple branches/layers;
5. names owned by one branch and inherited by another;
6. deliberately duplicated refs within one branch snapshot;
7. refs whose descriptor facts disagree with the table identity.

### Operation Scripts

Generated scripts should exercise:

1. install branch-owned immutable table;
2. fork branch and attach inherited refs;
3. materialize inherited layer;
4. remove one inherited layer;
5. clear one branch;
6. delete one branch's storage state;
7. rebuild registry from snapshots;
8. unregister stale snapshot;
9. over-release a ref;
10. compare aggregate facts before and after each transition.

## Required Direct Tests

### 1. Fact Type Validation

1. Valid owned table ref preserves table identity, owner branch, level, and
   table index.
2. Valid inherited table ref preserves child branch, source branch, fork
   version, layer index, and table identity.
3. Table identity mismatch between descriptor and facts is rejected.
4. Empty reachability snapshot is valid and reports zero refs.
5. Duplicate impossible refs are rejected or deduplicated according to the
   shipped contract.
6. Debug/display strings do not include row value bytes.
7. Reference ordering is stable independent of insertion order.

### 2. Branch Snapshot Emission

1. Owned immutable L0 table appears as an owned ref.
2. Owned L1+ table appears with its level fact.
3. Active mutable rows are excluded.
4. Frozen mutable tables are excluded until installed as immutable tables.
5. Inherited layer table appears as an inherited ref.
6. `Materializing` layer still protects source refs.
7. `Materialized` layer no longer protects source refs if it has been removed
   from readable inherited state.
8. Unavailable/corrupt inherited layer fails closed.
9. Snapshot counts match emitted refs exactly.

### 3. Aggregate Reachability

1. One branch-owned table is reachable and not shared.
2. Parent-owned table inherited by one child is shared/protected.
3. Parent-owned table inherited by two siblings reports all refs.
4. Chained fork reports nearest and farther inherited refs distinctly.
5. Removing one child leaves parent and sibling refs protected.
6. Removing the final ref makes the table a release candidate.
7. Aggregate facts are sorted by table identity and ref identity.
8. Rebuilding aggregate facts from the same snapshots twice is byte/fact
   identical.

### 4. Runtime Registry

1. Registry rebuilt from one snapshot reports correct ref counts.
2. Registry rebuilt from multiple snapshots reports shared counts.
3. Registering the same snapshot twice is rejected or idempotent according to
   the shipped contract.
4. Unregistering a snapshot decrements only that snapshot's refs.
5. Replacing an already-registered branch snapshot atomically removes stale
   refs and adds replacement refs.
6. Replacing an unregistered branch snapshot is rejected.
7. Over-release does not underflow.
8. Clear/reset removes all runtime refs.
9. Runtime refcount zero alone does not classify a table as durably safe to
   delete.
10. Registry/aggregate disagreement, including positive count mismatch, is
   reported as a protection fact.

### 5. Fork Reachability

1. Successful fork creates inherited refs for source immutable tables.
2. Failed fork leaves registry and aggregate facts unchanged.
3. Forking a branch with no immutable tables creates no inherited table refs
   but remains valid.
4. Forking from a source with inherited layers preserves inherited refs in
   deterministic ancestry order.
5. Sibling forks share refs without double-counting one sibling's refs.
6. Source branch clear/delete after fork does not make source tables
   releasable while the child still inherits them.

### 6. Materialization Reachability

1. Before materialization, source tables are protected by inherited refs.
2. During `Materializing`, source tables remain protected.
3. After replacement owned tables are visible and the inherited layer is
   removed, removed source refs become release candidates only if no other refs
   remain.
4. Replacement tables appear as child-owned refs.
5. Replacement tables preserve replacement provenance in their reachability
   ref kind.
6. Materializing one of multiple inherited layers releases only that layer's
   refs.
7. Materializing a deep layer preserves refs for nearer layers.
8. Empty materialization releases no table refs.
9. Idempotent materialization replay does not double-release.

### 7. Branch Clear/Delete Plans

1. Clearing empty branch produces an empty release plan.
2. Clearing branch-owned immutable tables reports removed owned refs.
3. Clearing inherited refs reports removed inherited refs.
4. Clearing a parent whose tables are still inherited protects those tables.
5. Clearing the last referencing child makes inherited source tables release
   candidates if the parent no longer owns them.
6. Branch active/frozen mutable rows never appear as table release candidates.
7. Invalid branch id or stale branch facts leave state unchanged.

### 8. Rebuild From Durable Facts

1. Rebuild from decoded branch reachability snapshots recreates the same
   aggregate facts as live state.
2. Missing table identity in decoded facts is rejected.
3. Duplicate decoded refs are classified deterministically.
4. Decoded facts referencing an unknown branch are rejected or classified
   according to the shipped contract.
5. Decoded facts with registry disagreement block release.

## Generated Test Counters

Extend `BranchLsmScaffoldOutcome` or equivalent with counters for:

1. reachability snapshots;
2. owned table refs;
3. inherited table refs;
4. materializing protection refs;
5. aggregate rebuilds;
6. shared table detections;
7. release candidates;
8. protected release attempts;
9. registry rebuilds;
10. registry unregisters;
11. registry disagreement cases;
12. fork reachability cases;
13. failed fork rollback cases;
14. materialization release cases;
15. branch clear release cases;
16. deterministic ordering cases;
17. invalid reachability rejection cases.

The property test must assert every required counter is nonzero.

## Source Guards

`branch_lsm_source_guard.rs` must continue to reject production `branch/`
matches for:

1. `crate::backend`;
2. direct `crate::service` publication calls;
3. `crate::lifecycle`;
4. `crate::commit`;
5. `crates/storage/src` old storage APIs;
6. `SegmentRefRegistry` in production storage-next;
7. `VersionedValue`;
8. product `Value`, `Namespace`, `TypeTag`, and old `Key`;
9. `std::fs`, `std::path::Path`, object layout path literals, or object-name
   parsing;
10. wall-clock time APIs.

The guard may allow storage-next reachability and registry vocabulary inside
`crate::branch`.

## Sensitivity Probes

Before closing L6I, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. omit inherited refs from branch snapshot;
2. include active/frozen mutable rows as durable table refs;
3. treat runtime refcount zero as a release proof;
4. release parent-owned table while a child still inherits it;
5. release materialization source refs before replacement owned refs are
   visible;
6. forget to roll back failed fork registry increments;
7. sort refs by insertion order;
8. allow refcount underflow;
9. treat registry/manifest disagreement as releasable;
10. import backend or lifecycle APIs into production branch code.

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

If L6I changes table identity validation, also run:

```bash
cargo test -p strata-storage-next --locked --lib table
```

## Exit Criteria

L6I test coverage is complete when:

1. direct tests cover fact validation, snapshot emission, aggregate rebuild,
   registry behavior, release planning, fork, materialization, and branch
   clear/delete;
2. generated tests cover shared-reference safety over many branches and
   inherited layers;
3. runtime refcount behavior is tested as an accelerator, not a deletion proof;
4. release plans never mark a protected table as releasable;
5. source guards enforce L6 boundaries;
6. the porting log records preserved old behavior, changed ownership,
   deferred durable work, and sensitivity probes;
7. all verification commands pass.
