# L6J Test Plan: Branch Compaction Integration

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-implementation-plan.md`

## Goal

Prove that L6J compacts branch-owned immutable tables through L5 without
changing supported branch reads, without crossing the L6 boundary, and without
turning compaction into unproved cleanup.

The suite must fail if L6J:

1. selects inherited, active, frozen, unavailable, or wrong-branch sources as
   compaction inputs;
2. omits overlapping lower-level tables needed to preserve non-overlap and read
   precedence;
3. changes latest, version-bounded, timestamp-bounded, history, prefix scan, or
   range scan results under keep-all compaction;
4. drops old versions without retained-version proof;
5. drops tombstones without proof that lower/inherited rows cannot resurrect;
6. drops TTL-expired rows without timestamp/as-of retention proof;
7. mutates branch state before all output artifacts decode and validate;
8. invalidates pinned read views captured before compaction;
9. emits old-table release candidates before replacement reachability is
   visible;
10. imports backend, service publication, lifecycle, old storage, product DTO,
    wall-clock, or scheduler APIs into production `branch/` code.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct module-local tests.
2. `crates/storage-next/src/testkit/branch_lsm.rs` for generated compaction
   scripts and independent model checks.
3. `crates/storage-next/tests/branch_lsm_properties.rs` for generated property
   tests behind the `testkit` feature.
4. `crates/storage-next/tests/branch_lsm_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs` if the L6
   fuzz inventory is extended before L6L.
6. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   minimized generated failure is captured.
7. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   source-map, sensitivity-probe, and closeout notes.

Tests must use storage-next `BranchId`, `CommitVersion`, `Timestamp`,
`StorageRow`, `BranchLocalState`, `BranchOwnedTable`, L5 table compaction
types, L6 read views, and L6I reachability facts. Tests must not use old
storage `SegmentId`, filesystem paths, object layout strings, backend handles,
product branch names, `VersionedValue`, product `Value`, old `Key`,
`Namespace`, or `TypeTag`.

## Independent Model

Generated tests should compare production compaction against a model that
treats keep-all compaction as a pure branch-owned table replacement.

Suggested model:

```text
ModelBranch {
  branch_id
  active_rows
  frozen_tables
  owned_levels: level -> Vec<ModelTable newest_first_for_l0_sorted_for_l1_plus>
  inherited_layers nearest-first
}

ModelCompactionCandidate {
  input_refs
  overlap_refs
  output_level
  preserved_refs
  bottommost_for_branch
}

ModelCompactionInstall {
  removed_refs
  output_tables
  release_candidates
}
```

The model should:

1. select only branch-owned immutable tables;
2. preserve active/frozen rows outside the candidate;
3. preserve inherited layers outside the candidate;
4. include overlapping lower-level tables when compacting into a lower level;
5. keep every row for keep-all compaction;
6. sort output rows using L5 table internal key order;
7. install output tables in the target level without mutating unrelated levels;
8. preserve L0 newest-to-oldest precedence for unselected tables;
9. enforce L1+ non-overlap after install;
10. compute removed-ref release candidates only after output refs are present.

The model must not call production candidate selection, production compaction
install, or production branch read helpers to derive expected state. It may use
L5 builders/readers to construct valid test tables.

## Generators

### Branch Shapes

Generate:

1. branch with no immutable tables;
2. branch with one L0 table;
3. branch with multiple overlapping L0 tables;
4. branch with L0 tables plus overlapping L1 tables;
5. branch with non-overlapping L1 tables;
6. branch with L1 table overlapping L2 tables;
7. branch with active rows plus immutable tables;
8. branch with frozen rows plus immutable tables;
9. branch with inherited layers plus child-owned immutable tables;
10. branch with materialized replacement tables;
11. branch with sparse levels;
12. branch at max configured level count.

### Rows

Generate rows over:

1. empty user keys;
2. embedded-zero and high-bit user keys;
3. adjacent prefix-like keys;
4. multiple logical spaces;
5. multiple `StorageSpaceId` values;
6. one physical key with many commit versions;
7. tombstones older, middle, and newest in a row chain;
8. TTL rows expiring before, exactly at, and after generated read timestamps;
9. `Timestamp::EPOCH` and `Timestamp::MAX` expiry facts;
10. non-monotonic commit timestamps;
11. child-local rows that shadow inherited rows;
12. rows that sit exactly on candidate key-range edges.

### Operation Scripts

Generated scripts should exercise:

1. install L0 tables;
2. install nonzero-level tables;
3. attach inherited layers;
4. materialize inherited layers before compaction;
5. capture pinned read view before compaction;
6. plan L0 compaction;
7. plan L0-to-L1 compaction;
8. plan nonzero-level compaction;
9. run keep-all compaction;
10. request unsafe old-version pruning;
11. request unsafe tombstone pruning;
12. request unsafe TTL pruning;
13. inject stale candidate by mutating state between plan and install;
14. compare latest/getv/as-of/history/prefix/range reads before and after;
15. rebuild reachability before and after install;
16. compute release plans for removed refs.

## Required Direct Tests

### 1. Request And Candidate Validation

1. Empty branch compaction is a typed no-op.
2. Single-table candidate is a typed no-op unless the shipped contract supports
   metadata-only moves.
3. Last-level compaction request is rejected or no-op according to the shipped
   contract.
4. Invalid branch id in request is rejected before mutation.
5. Invalid level index is rejected before mutation.
6. L0 candidate includes selected L0 tables in deterministic order.
7. L0-to-L1 candidate includes overlapping L1 tables.
8. L0-to-L1 candidate preserves non-overlapping L1 tables.
9. L1+ candidate includes overlapping next-level tables.
10. L1+ candidate preserves non-overlapping next-level tables.
11. Active rows are excluded from candidate sources.
12. Frozen rows are excluded from candidate sources.
13. Inherited layer tables are excluded from direct candidate sources.
14. Materialized replacement tables are accepted as ordinary owned inputs.
15. Candidate facts contain table identities and branch facts, not object paths
    or row value bytes.

### 2. Keep-All Compaction Read Parity

1. Latest point reads match before and after L0 compaction.
2. Version-bounded point reads match before and after.
3. Timestamp-bounded point reads match before and after.
4. History reads including tombstones match before and after.
5. History reads excluding tombstones match before and after.
6. Prefix scans match before and after.
7. Range scans match before and after.
8. Read parity holds with active child rows outside the candidate.
9. Read parity holds with frozen child rows outside the candidate.
10. Read parity holds with inherited rows outside the candidate.
11. Read parity holds with materialized replacement tables as inputs.
12. Read parity holds when output splits into multiple tables.
13. Read parity holds for empty values and high-bit keys.
14. Read parity holds for non-monotonic commit timestamps.
15. Read parity holds for TTL rows at before/at/after expiry timestamp bounds.

### 3. Retention Safety

1. Dropping old versions without a retained-version proof is rejected before L5
   can drop rows.
2. Dropping old versions with a stale or contradictory proof is rejected.
3. Dropping tombstones without bottommost/no-resurrection proof is rejected.
4. Dropping a tombstone that could hide a lower-level row is rejected.
5. Dropping a tombstone that could hide an inherited row is rejected.
6. Dropping TTL-expired rows without timestamp/as-of retention proof is
   rejected.
7. TTL pruning uses the requested retention timestamp, not wall-clock time.
8. Proof-backed pruning, if implemented, reports exact L5 drop reasons:
   `OlderVersion`, `TombstoneElided`, and `Expired`.
9. Keep-all policy never reports dropped rows.
10. Error strings for unsafe pruning do not include value bytes.

### 4. Atomic Install And Stale Candidate Handling

1. Output artifact build failure leaves branch state unchanged.
2. Output artifact decode failure leaves branch state unchanged.
3. Invalid output descriptor leaves branch state unchanged.
4. Stale candidate after concurrent branch mutation is rejected without
   removing old tables.
5. Successful install removes exactly selected input/overlap refs.
6. Successful install preserves unselected L0 tables.
7. Successful install preserves non-overlapping lower-level tables.
8. Successful L1+ install keeps target level sorted and non-overlapping.
9. State facts are refreshed after install.
10. Branch table counts match installed output refs.
11. No partial output refs are visible after a failed install.
12. Re-running a stale request requires replanning rather than silently using
    stale indexes.
13. Output identity collisions with existing branch-owned or inherited reachable
    tables are rejected before mutation.

### 5. Pinned Read Views

1. Read view captured before compaction still reads old table refs.
2. New read view captured after compaction reads output refs.
3. Old and new views return identical latest results under keep-all.
4. Old and new views return identical getv/as-of/history/scan results under
   keep-all.
5. Mutating branch state after old view capture does not mutate old view table
   facts.
6. Failed compaction install does not change old or new view results.

### 6. Reachability And Release Facts

1. Output tables appear as owned refs in the post-compaction reachability
   snapshot.
2. Removed old table refs are included in the compaction outcome.
3. Release plan is computed only after output refs are visible.
4. Removed refs still reachable from another branch/layer are protected.
5. Runtime registry disagreement blocks release.
6. Final unreferenced old refs become release candidates.
7. Release facts are deterministic and sorted.
8. Compaction outputs are not marked as materialization replacements.
9. Branch clear after compaction releases output refs, not already-removed old
   refs.

### 7. Boundary And Source Guards

1. Production `branch/` code does not import `crate::backend`.
2. Production `branch/` code does not call L4 service publication.
3. Production `branch/` code does not import `crate::lifecycle`.
4. Production `branch/` code does not import `crate::commit`.
5. Production `branch/` code does not import old `crates/storage/src` APIs.
6. Production `branch/` code does not mention `SegmentId`,
   `SegmentRefRegistry`, or filesystem path vocabulary.
7. Production `branch/` code does not mention `VersionedValue`, product
   `Value`, old `Key`, `Namespace`, or `TypeTag`.
8. Production `branch/` code does not read wall-clock time or environment
   variables.
9. L6J-owned compaction vocabulary is allowed only in `branch/` and testkit
   files where expected.

## Generated Test Counters

Extend `BranchLsmScaffoldOutcome` or equivalent with counters for:

1. compaction candidate no-op cases;
2. L0 compaction candidate cases;
3. L0-to-L1 compaction candidate cases;
4. nonzero-level compaction candidate cases;
5. keep-all compaction cases;
6. compaction output install cases;
7. compaction output split cases;
8. stale-candidate rejection cases;
9. unsafe old-version pruning rejection cases;
10. unsafe tombstone pruning rejection cases;
11. unsafe TTL pruning rejection cases;
12. compaction latest parity cases;
13. compaction version parity cases;
14. compaction timestamp parity cases;
15. compaction history parity cases;
16. compaction prefix scan parity cases;
17. compaction range scan parity cases;
18. compaction pinned-view isolation cases;
19. compaction release candidate cases;
20. compaction protected release cases;
21. invalid compaction request rejection cases.

The property test must assert every required L6J counter is nonzero.

## Sensitivity Probes

Before closing L6J, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. include active rows as compaction inputs;
2. include inherited rows as direct compaction inputs;
3. omit overlapping L1 tables from an L0-to-L1 candidate;
4. install outputs before every output artifact is decoded;
5. remove old tables before output refs are inserted;
6. drop older versions under keep-all policy;
7. drop tombstones under keep-all policy;
8. drop TTL-expired rows under keep-all policy;
9. treat wall-clock time as TTL proof;
10. mutate pinned read views after install;
11. emit old table release candidates before replacement reachability exists;
12. classify registry disagreement as releasable;
13. allow stale candidate indexes to remove the wrong table;
14. allow output identity collision with an inherited reachable table;
15. import backend/lifecycle/service APIs into production branch code.

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

If L6J changes L5 compaction behavior, also run:

```bash
cargo test -p strata-storage-next --locked --lib table::tests::compaction
cargo test -p strata-storage-next --locked --test table_runtime_properties
```

## Exit Criteria

L6J test coverage is complete when:

1. direct tests cover candidate validation, keep-all read parity, retention
   rejection, atomic install, pinned views, reachability/release facts, and
   source guards;
2. generated tests cover L0, L0-to-L1, nonzero-level, stale-candidate, and
   split-output scenarios;
3. unsafe old-version, tombstone, and TTL pruning paths are rejected or
   proof-backed and tested;
4. every supported read mode is compared before and after compaction;
5. release facts are tested against L6I aggregate/registry disagreement;
6. source guards enforce L6 boundaries;
7. the porting log records preserved old behavior, intentional V1 changes,
   deferred durable work, and sensitivity probes;
8. all verification commands pass.
