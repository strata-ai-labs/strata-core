# L6F Test Plan: Fork And Inherited Layers

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`

## Goal

Prove that L6F correctly implements storage-level fork and inherited-layer read
mechanics over storage-next rows and L5 immutable table readers.

The suite must fail if L6F:

1. copies inherited row bytes into child-owned state during fork;
2. exposes source rows committed after the fork version;
3. hides source rows committed at or before the fork version;
4. fails to rewrite inherited rows into the child branch namespace;
5. groups scans by source branch keys instead of rewritten child keys;
6. lets inherited rows beat child-local rows with the same visible key when a
   child-local row should shadow them;
7. falls through a child tombstone to an inherited put;
8. searches farther ancestors before nearer ancestors for exact ties;
9. mutates pinned child read views after child or source branch changes;
10. accepts self-inheritance, stale inherited counts, or wrong-source rows;
11. imports backend, lifecycle, commit-runtime, engine, product DTO, or old
    storage APIs into production `branch/` code.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local direct tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for source-boundary
   scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated branch-LSM
   scripts and the independent model.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs` if the
   L6 fuzz inventory is created in this slice.
6. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   generated failure captures a minimized seed.
7. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   source-map, sensitivity-probe, and closeout notes.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, L5 immutable table runtime types,
and L6 branch result shells. Tests must not use old storage `Key`, `Value`,
`Namespace`, `TypeTag`, `VersionedValue`, engine workflow types, backend
handles, filesystem paths, wall-clock time, or product payload vocabulary.

## Independent Model

Generated and direct tests should compare production output against a model
that rewrites inherited rows before applying MVCC selection.

Suggested model:

```text
ModelBranch {
  branch_id
  active_rows: Vec<ModelRow>
  frozen_tables: Vec<Vec<ModelRow newest-table-first>>
  owned_levels: Vec<Vec<ModelTable>>
  inherited_layers: Vec<ModelInheritedLayer nearest-first>
}

ModelInheritedLayer {
  source_branch_id
  fork_version
  status
  owned_levels: Vec<Vec<ModelTable>>
}

ModelTable {
  source_kind
  level
  table_index
  rows: Vec<ModelRow>
}

ModelRow {
  physical_key
  commit_version
  commit_timestamp
  expires_at
  is_tombstone
  value
  source
}
```

The model should:

1. collect child-local rows without branch rewrite;
2. collect inherited rows only from active/materializing layers;
3. drop inherited rows whose commit version is greater than layer fork
   version;
4. rewrite inherited row branch id from source to child before grouping;
5. preserve all non-branch row facts during rewrite;
6. group by rewritten physical key;
7. sort row chains by commit version descending;
8. prefer child-local rows over inherited rows for exact equal internal keys;
9. prefer lower inherited layer index for exact equal inherited internal keys;
10. apply latest and version bounds after rewrite;
11. treat selected tombstones as shadowing older rows;
12. preserve tombstones in history according to options;
13. apply history `before_version` as exclusive;
14. apply limits after filtering;
15. scan physical keys in encoded child-key order;
16. emit at most one visible row per physical key in scans;
17. reject timestamp-bound requests as deferred until L6G.

The model must not call production `BranchReadView` candidate collection,
source ordering, or inherited rewrite helpers. It may use L5 builders/readers
to create valid table artifacts for production input, but expected rows should
come from model rows.

## Generators

### Branch Graphs

Generate:

1. root branches with no inheritance;
2. one child forked from one source;
3. siblings forked from the same source;
4. chained forks up to the configured inherited-layer limit;
5. source branches with no owned immutable tables;
6. source branches with one owned immutable table;
7. source branches with multiple L0 tables;
8. source branches with L1+ disjoint tables;
9. child branches with active rows after fork;
10. child branches with frozen rows after fork;
11. child branches with owned immutable rows after fork;
12. invalid direct self-inheritance plans;
13. invalid inherited row source-branch mismatches;
14. inherited layer counts above config limits.

### Fork Versions

Generate:

1. fork at `CommitVersion::ZERO`;
2. fork at the source max version;
3. fork below source max version;
4. requested fork version above source max version is deferred until the
   retained historical fork API exists;
5. parent/source rows at `fork_version - 1`;
6. parent/source rows exactly at `fork_version`;
7. parent/source rows at `fork_version + 1`;
8. parent/source rows at `CommitVersion::MAX` when valid for the fixture.

### Rows

Generate rows over:

1. multiple valid logical spaces;
2. multiple `StorageSpaceId` values;
3. empty user keys;
4. embedded-zero user keys;
5. high-bit user keys;
6. adjacent prefix-like user keys;
7. one physical key with multiple versions;
8. child and source rows with the same logical key after rewrite;
9. child and source rows with different logical keys;
10. put rows with empty values;
11. tombstones;
12. rows with non-monotonic timestamps, preserving but not applying timestamp
    visibility until L6G;
13. `Timestamp::EPOCH` and `Timestamp::MAX` facts.

### Inherited Layer Shapes

Generate:

1. one inherited layer with one L0 table;
2. one inherited layer with multiple overlapping L0 tables;
3. one inherited layer with disjoint L1+ tables;
4. multiple inherited layers with distinct source branches;
5. multiple inherited layers containing the same logical key after rewrite;
6. materializing layers that remain readable;
7. materialized layers that are skipped;
8. unavailable layers that fail read-view construction;
9. empty source-owned layer when preserving only a fork boundary;
10. copied active/materializing inherited layers whose readable status is
    preserved, plus materialized layers that are skipped.

### Operation Scripts

Generated scripts should exercise:

1. build source branch-owned immutable tables through L5;
2. capture fork from source to child;
3. attach inherited layer facts to a child;
4. reject self-inheritance;
5. reject wrong-source inherited rows;
6. reject requested fork version above source max version once the retained
   historical fork API exists;
7. append child put after fork;
8. append child tombstone after fork;
9. install child-owned immutable table after fork;
10. mutate source after child view capture;
11. mutate child after child view capture;
12. latest point read;
13. version-bounded point read;
14. retained history read;
15. prefix scan;
16. range scan;
17. wrong-branch read rejection;
18. timestamp-bound deferred request.

## Required Direct Tests

### 1. Fork Capture

1. Fork from source to child creates inherited layer metadata without copying
   rows into child active, frozen, or owned immutable state.
2. Child starts with empty own state and nonzero inherited layers.
3. Source owned immutable levels become the first inherited layer.
4. Source inherited layers are appended after the source-owned layer in
   deterministic order.
5. Copied active/materializing inherited layers reset status to `Active`.
6. Fork captures source max applied version as fork version.
7. Fork with source branch equal to destination branch is rejected.
8. Fork into a destination with existing own rows is rejected or exposed
   through a deliberately named attach API.
9. Fork failure leaves destination state unchanged.
10. Source active/frozen rows are not silently inherited by L6F.

### 2. Inherited Layer Validation

1. Valid inherited layer with matching source-branch rows is accepted.
2. Inherited row from another source branch is rejected without payload
   leakage.
3. Direct self-inheritance is rejected.
4. Stale descriptor table count is rejected.
5. Layer count above `max_inherited_layers` is rejected.
6. `Active` layer participates in reads.
7. `Materializing` layer participates in reads.
8. `Materialized` layer is skipped by reads.
9. `Unavailable` layer fails closed for normal read-view construction.
10. Error chains preserve lower L5 table errors where applicable.

### 3. Fork-Version Gate

1. Parent/source row below fork version is visible in child.
2. Parent/source row at fork version is visible in child.
3. Parent/source row above fork version is invisible in child latest reads.
4. `getv` below fork version applies the requested lower bound.
5. `getv` above fork version is capped at fork version for inherited rows.
6. Source mutation after fork does not affect an already captured child view.
7. Source mutation after fork does not affect a newly captured child view
   unless the child is explicitly reforked.
8. Fork-at-version above retained/source max version is deferred until the
   retained historical fork API exists.

### 4. Key Rewrite

1. Inherited source branch id rewrites to child branch id for point reads.
2. Rewritten row preserves logical space.
3. Rewritten row preserves storage-space id.
4. Rewritten row preserves user key bytes, including empty, zero, and high-bit
   bytes.
5. Rewritten row preserves commit version.
6. Rewritten row preserves commit timestamp and expiry timestamp.
7. Rewritten row preserves tombstone flag.
8. Rewritten row preserves value bytes.
9. Rewrite happens before scan grouping.
10. Rewrite errors are typed and do not include value bytes.

### 5. Child-Local Shadowing

1. Child put newer than inherited put wins latest reads.
2. Child tombstone newer than inherited put returns no latest row.
3. Child row above requested version does not shadow inherited row visible at
   requested version.
4. Child older row does not shadow inherited newer row when version rules
   choose the inherited row.
5. Child-owned immutable row shadows inherited row the same way active/frozen
   rows do.
6. Child-local exact duplicate internal key wins over inherited exact duplicate
   internal key.
7. Child tombstone shadowing works in point reads, history reads, prefix scans,
   and range scans.

### 6. Chained Ancestry

1. Child of child preserves source-owned layer then prior inherited layers.
2. Nearest ancestor wins exact tie against farther ancestor.
3. Farther ancestor row is still visible when no nearer layer or child-local
   row shadows it.
4. Sibling branches sharing a source do not observe each other's child-local
   rows.
5. Chained fork uses the source child's inherited max version facts for its
   fork version.
6. Configured inherited-layer limit is enforced.

### 7. Point Reads

1. Inherited-only latest returns a visible put.
2. Inherited-only latest returns `None` for a selected tombstone.
3. Inherited L0 rows participate in point reads.
4. Inherited L1+ rows participate in point reads.
5. Overlapping inherited L0 tables choose by commit version.
6. Source facts report `BranchRowSource::Inherited { source_branch_id,
   layer_index }`.
7. Wrong-branch point read is rejected before inherited lookup.

### 8. History Reads

1. History includes child-local and inherited rows newest first after rewrite.
2. History includes inherited tombstones by default.
3. History can exclude inherited tombstones without dropping live inherited
   rows.
4. `before_version` excludes inherited rows at or above the bound.
5. Limit is applied after tombstone filtering.
6. History source facts preserve inherited source branch and layer index.
7. Inherited rows above fork version are absent from history.

### 9. Prefix And Range Scans

1. Prefix scan includes inherited rows after rewrite.
2. Range scan includes inherited rows after rewrite.
3. Scans group child-local and inherited rows by rewritten child physical key.
4. Scans return at most one visible row per physical key.
5. Child tombstone suppresses inherited put in scans.
6. Inherited tombstone suppresses older inherited put in scans.
7. Source row after fork version is absent from scans.
8. Scans preserve logical-space boundaries.
9. Scans preserve storage-space-id boundaries.
10. Degenerate and open/closed range bounds work with inherited rows.

### 10. Pinned Views

1. Child read view captured before child append does not see later child row.
2. Child read view captured before child tombstone does not see later child
   tombstone.
3. Child read view captured before source append does not see later source row.
4. Child read view captured before source immutable install does not see later
   source table.
5. Child read view captured after fork but before materialization remains
   readable when L6H later changes layer status in branch state.
6. Pinned views retain inherited source facts.

## Generated Harness Requirements

Extend `BranchLsmPropertyOutcome` with nonzero counters for:

1. fork capture cases;
2. inherited layer validation cases;
3. inherited latest reads;
4. inherited getv reads;
5. inherited history reads;
6. inherited prefix scans;
7. inherited range scans;
8. inherited key rewrite cases;
9. child put shadow cases;
10. child tombstone shadow cases;
11. post-fork parent invisibility cases;
12. chained ancestry cases;
13. invalid inherited layer rejection cases;
14. pinned inherited view isolation cases.

`branch_lsm_properties.rs` should assert every L6F counter is greater than
zero so generated tests cannot become placeholders.

Default generated runs should remain bounded for CI. Larger inherited-depth and
table-count stress runs may be ignored/manual.

## Source Guards

Update `branch_lsm_source_guard.rs` so production `crates/storage-next/src/branch`
allows only L6F-owned fork/inheritance vocabulary:

Allowed:

1. `InheritedLayerDescriptor`
2. `InheritedLayerStatus`
3. `BranchRowSource::Inherited`
4. `fork_version`
5. storage-level `fork` helpers inside `branch/`
6. source-to-child row rewrite helpers

Still forbidden:

1. `crate::backend`
2. `crate::layout`
3. `crate::service`
4. `crate::commit`
5. `crate::lifecycle`
6. old storage `VersionedValue`, `Value`, `Key`, `Namespace`, `TypeTag`
7. `std::fs`
8. `std::path`
9. `std::env`
10. product DTO or StrataHub vocabulary
11. materialization entrypoints before L6H
12. compaction entrypoints before L6J
13. snapshot install entrypoints before L6K

## Fuzz Target

If L6 fuzz targets are introduced in this slice, add:

```text
crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs
crates/storage-next/fuzz/corpus/branch_lsm_inheritance/
```

The target should call a hidden testkit function dedicated to inheritance
scripts. It should not call a generic scaffold-only function.

Minimum fuzz behavior:

1. arbitrary bytes choose branch graph shape;
2. arbitrary bytes choose fork versions and source rows;
3. arbitrary bytes choose child writes and tombstones;
4. arbitrary bytes choose latest/getv/history/scan checks;
5. corrupt or invalid plans return typed errors, not panics.

If fuzz target registration is deferred, record that explicitly in the L6F
porting-log entry and keep generated property tests as the closure gate.

## Sensitivity Probes

Before closing L6F, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. remove the inherited fork-version cap;
2. rewrite only point reads, not scans;
3. group scans before inherited row rewrite;
4. preserve source branch id in returned inherited rows;
5. let inherited rows sort ahead of child-local exact duplicates;
6. search layer index 1 before layer index 0 for exact inherited ties;
7. treat `Materialized` layers as readable;
8. reject `Materializing` layers as unreadable before L6H;
9. fall through child tombstones to inherited puts;
10. include source rows above fork version in history;
11. let source mutation update an already pinned child view;
12. accept direct self-inheritance;
13. leak row value bytes in inherited validation errors;
14. import backend, lifecycle, commit-runtime, or product DTO APIs from
    production `branch/`.

## Verification

Required after implementation:

1. `cargo test -p strata-storage-next --locked --lib branch`
2. `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo test -p strata-storage-next --locked`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

Optional/manual:

1. `cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=30`

## Completion Criteria

L6F test work is complete when:

1. direct tests cover fork capture, inherited validation, fork gates, key
   rewrite, child-local shadowing, chained ancestry, point reads, history,
   scans, and pinned views;
2. generated tests cover the same behavior with nonzero category counters;
3. source guards enforce the L6 boundary and allowed inheritance vocabulary;
4. no test fixture relies on old product DTOs or old storage row types;
5. all mandatory verification commands pass;
6. any deferred fuzz work is recorded in the L6F porting-log entry.
