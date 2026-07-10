# L6K Test Plan: Snapshot Row Install

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-implementation-plan.md`

## Goal

Prove that L6K installs already-decoded storage rows into branch-local LSM state
without primitive DTOs, without backend or snapshot-service access, and without
partial branch-state visibility.

The suite must fail if L6K:

1. accepts old storage `DecodedSnapshotEntry`, `TypeTag`, product `Value`, old
   `Key`, `Namespace`, `VersionedValue`, or product branch names;
2. reads durable snapshot bytes, object names, or backend handles;
3. mutates one branch before the full multi-branch install is validated and
   staged;
4. installs rows into the wrong branch;
5. silently creates missing branches without explicit policy;
6. merges snapshot rows into a non-empty branch under the V1 require-empty
   policy;
7. drops tombstones, TTL facts, commit timestamps, empty values, or high-bit
   keys;
8. changes latest, version-bounded, timestamp-bounded, history, prefix scan, or
   range scan semantics after install;
9. invalidates pinned read views captured before install;
10. omits reachability facts for installed tables.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct module-local tests.
2. `crates/storage-next/src/testkit/branch_lsm.rs` for generated snapshot
   install scripts and independent model checks.
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
`StorageRow`, `BranchLocalState`, `BranchOwnedTable`, L5 table builders/readers,
L6 read views, and L6I reachability facts. Tests must not use old storage
`DecodedSnapshotEntry`, `TypeTag`, `Value`, old `Key`, `Namespace`, object
layout strings, backend handles, snapshot object names, product branch names,
or StrataHub vocabulary.

## Independent Model

Generated tests should compare production snapshot install against a model that
treats install as a pure replacement of empty target branch states with sorted
row chains.

Suggested model:

```text
ModelSnapshotInstallRequest {
  missing_branch_policy
  target_state_policy
  rows
}

ModelBranchInstall {
  branch_id
  rows sorted by internal key
  row_chains grouped by physical key without version
}

ModelInstalledBranch {
  branch_id
  immutable_l0_tables
  read_model
  reachability_identities
}
```

The model should:

1. derive branch id from each row physical key;
2. reject rows whose declared group branch differs from the physical key branch;
3. reject duplicate internal keys across the full request;
4. reject unsorted branch groups when the production contract requires sorted
   input;
5. reject non-empty target branches under V1 require-empty policy;
6. preserve every row, including tombstones and expired rows;
7. implement latest/version/timestamp/history/prefix/range reads over the
   installed row chains independently from production read views;
8. report installed table identities as owned refs only after successful
   install.

The model must not call production snapshot install, production branch read
helpers, or production reachability helpers to derive expected results. It may
use L5 builders/readers only to construct valid input tables when needed by
fixtures.

## Generators

### Branch Shapes

Generate:

1. no existing branches;
2. one existing empty branch;
3. multiple existing empty branches;
4. missing target branch with reject policy;
5. missing target branch with create policy;
6. non-empty target branch with active rows;
7. non-empty target branch with frozen rows;
8. non-empty target branch with branch-owned immutable tables;
9. branch with inherited layers;
10. branch with materialized replacement tables;
11. branch set where one target is valid and another is invalid;
12. branch set where target ordering differs from input ordering.

### Rows

Generate rows over:

1. empty user keys;
2. embedded-zero and high-bit user keys;
3. adjacent prefix-like keys;
4. multiple `StorageSpaceId` values;
5. one physical key with many commit versions;
6. rows with same user key across different branches;
7. tombstones older, middle, and newest in a row chain;
8. TTL rows expiring before, exactly at, and after generated read timestamps;
9. `Timestamp::EPOCH` and `Timestamp::MAX`;
10. non-monotonic commit timestamps;
11. empty values;
12. large but valid values near L5 builder limits;
13. rows exactly at generated prefix/range boundaries.

### Invalid Inputs

Generate invalid plans with:

1. duplicate internal key in the same branch group;
2. duplicate internal key split across two groups for the same branch;
3. unsorted internal-key order;
4. row physical-key branch id different from group branch id;
5. missing branch under reject policy;
6. non-empty target under require-empty policy;
7. output identity collision with an existing reachable table;
8. table builder limit failure;
9. empty branch group when grouped input is used;
10. duplicate branch group.

## Required Direct Tests

### 1. Request And Policy Validation

1. Empty install returns typed no-op and does not mutate.
2. Missing branch is rejected under reject policy before mutation.
3. Missing branch is created only under explicit create policy.
4. Existing non-empty branch is rejected under require-empty policy.
5. Duplicate branch group is rejected.
6. Group branch id mismatch is rejected.
7. Invalid table builder config is rejected before row validation side effects.
8. Error/debug strings do not include row value bytes.

### 2. Row Preflight

1. Valid single-branch sorted rows pass preflight.
2. Valid multi-branch sorted rows pass preflight.
3. Duplicate internal key in one branch group is rejected.
4. Duplicate internal key across groups for the same branch is rejected.
5. Same internal key bytes in different branches are accepted only when the
   branch bytes differ.
6. Unsorted row group is rejected.
7. Empty user keys and high-bit user keys are accepted.
8. Tombstones are accepted as rows, not filtered.
9. TTL-expired rows are accepted as retained rows, not filtered.
10. `Timestamp::MAX` expiry is preserved as a real far-future expiry.

### 3. Table Build And Identity

1. Output tables are built through L5 and decoded before install.
2. Output tables are installed as ordinary branch-owned L0 refs.
3. Output identities include branch id so identical rows in two branches do not
   alias.
4. Output identity collision with an existing reachable table is rejected before
   mutation.
5. Oversized row/key/table build failure leaves every target unchanged.
6. Multi-table output reports every installed table identity.
7. Table facts match decoded reader facts.

### 4. All-Or-Nothing Install

1. Valid single-branch install mutates only that branch.
2. Valid multi-branch install mutates every target branch.
3. Multi-branch install with one invalid target mutates no target branch.
4. Table build failure after at least one branch plan was staged mutates no
   target branch.
5. Branch-state validation failure mutates no target branch.
6. Created missing branches do not become visible on failed batch install.
7. Branch facts refresh after successful install.
8. Install outcome row/table/branch counts match installed state.

### 5. Read Parity After Install

1. Latest point reads match model.
2. Version-bounded point reads match model.
3. Timestamp-bounded point reads match model.
4. History including tombstones matches model.
5. History excluding tombstones matches model.
6. Prefix scans match model.
7. Range scans match model.
8. Tombstone newest row suppresses older live rows.
9. TTL row is visible before expiry and suppressed at/after expiry.
10. Empty values and high-bit keys survive install.
11. Multiple storage spaces remain isolated.
12. Non-monotonic commit timestamps are preserved.

### 6. Pinned Views And Reachability

1. Read view captured before failed install remains unchanged.
2. Read view captured before successful install remains pinned to old state.
3. New read view after successful install sees installed rows.
4. Reachability snapshot after install reports owned output refs.
5. Installed output refs are not materialization replacements.
6. Branch clear/release planning after install can release installed output refs
   when no aggregate/registry protects them.
7. Runtime registry disagreement protects installed output refs.

### 7. Boundary And Source Guards

1. Production `branch/` code does not import `crate::backend`.
2. Production `branch/` code does not import L4 snapshot services.
3. Production `branch/` code does not import L3 snapshot codecs.
4. Production `branch/` code does not import `crate::layout` or object names.
5. Production `branch/` code does not import `crate::lifecycle`,
   `crate::commit`, or engine crates.
6. Production `branch/` code does not mention old
   `DecodedSnapshotEntry`, `TypeTag`, `Value`, old `Key`, `Namespace`,
   `VersionedValue`, product branch names, or StrataHub.
7. Production `branch/` code does not read wall-clock time or environment
   variables.
8. L6K-owned snapshot row install vocabulary is allowed only in branch/testkit
   files where expected.

## Generated Test Counters

Extend `BranchLsmScaffoldOutcome` or equivalent with counters for:

1. empty snapshot install no-op cases;
2. single-branch snapshot install cases;
3. multi-branch snapshot install cases;
4. missing-branch rejection cases;
5. missing-branch create-policy cases;
6. non-empty target rejection cases;
7. empty branch-group rejection cases;
8. duplicate branch-group rejection cases;
9. duplicate row rejection cases;
10. unsorted row rejection cases;
11. branch mismatch rejection cases;
12. output identity collision rejection cases;
13. table build failure atomicity cases;
14. snapshot latest parity cases;
15. snapshot version parity cases;
16. snapshot timestamp parity cases;
17. snapshot history parity cases;
18. snapshot prefix scan parity cases;
19. snapshot range scan parity cases;
20. snapshot tombstone preservation cases;
21. snapshot TTL preservation cases;
22. snapshot pinned-view isolation cases;
23. snapshot reachability cases;
24. snapshot source-boundary guard cases.

The property test must assert every required L6K counter is nonzero.

## Sensitivity Probes

Before closing L6K, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. skip full preflight and mutate the first valid branch before seeing a later
   invalid branch;
2. accept missing branches under reject policy;
3. merge into a non-empty branch under require-empty policy;
4. ignore row physical-key branch id mismatch;
5. sort unsorted input instead of rejecting it;
6. drop tombstones during install;
7. drop TTL-expired rows during install;
8. strip empty values;
9. generate output identities without branch id;
10. expose a created missing branch after failed batch install;
11. update branch facts before output table readers decode;
12. mutate pinned pre-install read views;
13. emit installed output refs as materialization replacements;
14. import snapshot service, backend, object layout, old decoded snapshot DTO,
    or product value vocabulary into production branch code.

## Verification Commands

Run at least:

```bash
cargo test -p strata-storage-next --locked --lib branch_snapshot
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If L6K changes L5 builder behavior, also run:

```bash
cargo test -p strata-storage-next --locked --lib table::tests::builder
cargo test -p strata-storage-next --locked --test table_runtime_properties
```

## Exit Criteria

L6K test coverage is complete when:

1. direct tests cover request policy, row preflight, table build/decode,
   all-or-nothing install, read parity, pinned views, reachability, and source
   guards;
2. generated tests cover single-branch, multi-branch, missing-branch,
   non-empty-target, duplicate, unsorted, branch-mismatch, table-build-failure,
   and read-parity scenarios;
3. every supported read mode is compared against an independent model after
   install;
4. tombstones, TTL rows, empty values, high-bit keys, and multiple storage
   spaces are preserved;
5. all validation failures leave every target branch unchanged;
6. source guards enforce L6 boundaries;
7. the porting log records preserved old behavior, intentional V1 changes,
   deferred durable work, and sensitivity probes;
8. all verification commands pass.
