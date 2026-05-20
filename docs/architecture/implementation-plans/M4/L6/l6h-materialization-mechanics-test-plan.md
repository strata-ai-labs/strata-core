# L6H Test Plan: Materialization Mechanics

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md`

## Goal

Prove that L6H materializes inherited branch layers into child-owned L5 table
rows without changing storage read results or crossing the L6 boundary.

The suite must fail if L6H:

1. removes an inherited layer before replacement child-owned tables are visible
   to new read views;
2. changes latest, version-bounded, timestamp-bounded, history, prefix scan, or
   range scan results;
3. drops inherited history just because a newer child row exists;
4. applies TTL cleanup or tombstone pruning during materialization;
5. materializes source rows above the layer fork version;
6. forgets to rewrite source branch ids into the child branch namespace;
7. mutates pinned read views captured before materialization;
8. lets an unavailable or corrupt inherited layer materialize silently;
9. loses source-chain information from L5 table build/decode errors;
10. imports backend, service, lifecycle, old storage, product DTO, or wall-clock
    APIs into production `branch/` code.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct module-local tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for source-boundary
   scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated branch-LSM
   scripts and the independent model.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs` if the
   L6 fuzz inventory is extended before L6L.
6. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   generated failure captures a minimized seed.
7. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   source-map, sensitivity-probe, and closeout notes.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, L5 table runtime types, and L6 branch
runtime types. Tests must not use old storage `Key`, `Value`, `Namespace`,
`TypeTag`, `VersionedValue`, engine workflow types, backend handles, filesystem
paths, object layout strings, wall-clock time, or product payload vocabulary.

## Independent Model

Generated and direct tests should compare production output against a model
that treats materialization as a pure ownership rewrite.

Suggested model:

```text
ModelBranch {
  branch_id
  active_rows
  frozen_tables
  owned_levels
  inherited_layers nearest-first
}

ModelInheritedLayer {
  source_branch_id
  fork_version
  status
  tables
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

The model materialization operation should:

1. identify the target inherited layer by layer index or stable source/fork
   identity;
2. reject unavailable layers;
3. collect rows only from the target layer;
4. drop rows with `commit_version > fork_version`;
5. rewrite retained rows from source branch id to child branch id;
6. preserve every non-branch row fact;
7. skip only byte-identical rewritten row duplicates already represented by a
   higher-precedence child-local source or nearer inherited layer;
8. retain same physical-key rows at different commit versions;
9. retain tombstones and expired rows;
10. append the retained rows to child-owned L0 model tables;
11. remove the target inherited layer from the model, or mark it materialized
    if the production state keeps ledger entries;
12. leave all other inherited layers in their original order.

For read parity, the model should run the same independent latest, getv,
as-of, history, prefix scan, and range scan selection used by L6F/L6G tests
before and after materialization.

The model must not call production materialization, read-view candidate
collection, source-order helpers, or rewrite helpers when deriving expected
rows. It may use L5 builders/readers to build valid production tables.

## Generators

### Branch Graphs

Generate:

1. root branch with no inheritance;
2. child with one inherited layer;
3. child with multiple inherited layers nearest-first;
4. chained fork shape with at least three branches;
5. sibling branches sharing the same source tables;
6. empty inherited layer used only as a fork boundary;
7. target layer at index 0;
8. target layer at the deepest valid index;
9. target layer already materializing;
10. target layer already materialized;
11. unavailable target layer;
12. stale layer index with source/fork identity still present at a different
    index.

### Rows

Generate rows over:

1. multiple valid logical spaces;
2. multiple `StorageSpaceId` values;
3. empty user keys;
4. embedded-zero user keys;
5. high-bit user keys;
6. adjacent prefix-like user keys;
7. one physical key with many versions;
8. child and inherited rows with the same physical key and different commit
   versions;
9. child and inherited rows with identical rewritten row facts;
10. child and inherited rows with the same rewritten internal key but different
    timestamp, expiry, tombstone bit, or value bytes;
11. nearer and farther inherited rows with identical rewritten row facts;
12. inherited rows above the target fork version;
13. put rows with empty values;
14. tombstones at old, middle, and newest versions;
15. rows expiring before, exactly at, and after generated timestamp read
    bounds;
16. `Timestamp::EPOCH` and `Timestamp::MAX` facts;
17. non-monotonic commit timestamps.

### Operation Scripts

Generated scripts should exercise:

1. build source branch-owned immutable tables through L5;
2. fork source to child;
3. append child active rows after fork;
4. rotate child active into frozen rows;
5. install child-owned immutable rows after fork;
6. attach multiple inherited layers;
7. capture a pinned read view before materialization;
8. materialize one inherited layer;
9. retry materialization with stale staged facts;
10. materialize an empty layer;
11. read latest before and after;
12. read getv before and after;
13. read as-of before and after;
14. read history before and after with and without tombstones;
15. scan prefix before and after;
16. scan range before and after;
17. mutate source after child fork and prove post-fork rows remain invisible;
18. mutate child after pinned view capture and prove pinned view isolation.

## Required Direct Tests

### 1. Request And Status Validation

1. Missing layer index returns a typed no-op or missing-layer error according
   to the shipped API.
2. `Unavailable` inherited layer fails closed.
3. `Active` inherited layer can be materialized.
4. `Materializing` inherited layer remains readable and can be retried.
5. `Materialized` inherited layer produces an idempotent no-op.
6. Source branch id equal to child branch id is rejected.
7. Descriptor table count mismatch is rejected before row copying.
8. Wrong-source row in an inherited table is rejected.
9. Error messages and debug facts do not include row value bytes.

### 2. Row Rewrite And Fact Preservation

1. Materialized rows have the child branch id in their physical key.
2. Logical space, storage-space id, user key, commit version, commit
   timestamp, expiry timestamp, tombstone bit, and value bytes are preserved.
3. Source rows above the fork version are not materialized.
4. A source row exactly at the fork version is materialized.
5. Empty value bytes survive materialization.
6. High-bit and embedded-zero user keys survive materialization.
7. Table facts for replacement tables match the rewritten child rows.
8. Replacement table identities are opaque and contain no path separators.

### 3. Read Parity

1. Latest point reads match before and after materializing one layer.
2. Version-bounded point reads match before and after.
3. Timestamp-bounded point reads match before and after.
4. Prefix scans match before and after.
5. Range scans match before and after.
6. History reads including tombstones match before and after.
7. History reads excluding tombstones match before and after.
8. Read parity holds with active child rows.
9. Read parity holds with frozen child rows.
10. Read parity holds with child-owned immutable rows.
11. Read parity holds with multiple inherited layers where only one layer is
    materialized.
12. New read views report zero inherited layers after the final layer is
    materialized and still return the same rows.

### 4. Historical Row Retention

1. A newer child row does not cause an older inherited row to be dropped when
   that inherited row is visible to an earlier `AtVersion` read.
2. A newer child row does not cause an inherited row to be dropped when that
   inherited row is visible to an earlier as-of timestamp.
3. A child row with identical rewritten row facts suppresses the inherited
   duplicate without changing reads.
4. A child row with the same internal key but different row facts rejects
   materialization without mutation.
5. A nearer inherited row with identical rewritten row facts suppresses a
   farther inherited exact duplicate.
6. Same physical key with different commit versions is retained, not treated
   as a duplicate.
7. Same physical key with different timestamps is retained.

### 5. Tombstone And TTL Preservation

1. Inherited tombstone is materialized and still suppresses older puts.
2. Child-local tombstone continues to suppress materialized inherited puts.
3. Inherited expired put is materialized and remains invisible at timestamps
   at or after expiry.
4. Inherited expired put remains visible in storage history when requested.
5. Materialization does not evaluate TTL against wall-clock time.
6. Materialization does not physically delete expired rows.
7. Tombstone expiry sentinel is preserved.

### 6. Layer Ordering

1. Materializing deepest layer first preserves reads.
2. Materializing nearest layer first preserves reads.
3. Remaining inherited layers keep nearest-first order.
4. Materializing one layer does not change source attribution for other
   inherited layers.
5. A farther layer row hidden by a nearer layer exact duplicate is not
   resurrected after materializing the farther layer.
6. A farther layer row with a different commit version remains available for
   historical bounds after materialization.

### 7. State Transition And Pinned Views

1. Failure during row collection leaves inherited layer readable.
2. Failure during table building leaves inherited layer readable.
3. Replacement tables are installed before the inherited layer is removed from
   new read views.
4. Pinned read view captured before materialization still reads from its
   inherited layer clone.
5. Pinned read view captured after materialization reads replacement
   child-owned tables.
6. Branch facts refresh max commit version and timestamp ranges after
   materialization.
7. Empty materialization removes or marks the layer according to the shipped
   state model and creates no table.
8. Replaying a completed materialization is idempotent.

### 8. Table Building And Splitting

1. Single small layer creates one replacement L0 table.
2. Large layer creates multiple replacement L0 tables without exceeding L5
   limits.
3. Replacement rows are sorted and unique before L5 build.
4. Duplicate rewritten internal keys inside one target layer are rejected.
5. L5 builder errors preserve source chains in `BranchRuntimeError`.
6. Replacement L0 tables are inserted in deterministic order.
7. Nonzero-level replacement is rejected until explicitly supported.

### 9. Generated Test Counters

Extend `BranchLsmScaffoldOutcome` or equivalent with counters for:

1. materialization attempts;
2. successful materializations;
3. empty materializations;
4. idempotent materialization retries;
5. materialized rows;
6. materialized tables;
7. skipped post-fork rows;
8. skipped exact duplicates;
9. latest read parity checks;
10. version read parity checks;
11. timestamp read parity checks;
12. history read parity checks;
13. prefix scan parity checks;
14. range scan parity checks;
15. pinned materialization view isolations;
16. tombstone preservation cases;
17. TTL preservation cases;
18. invalid materialization rejections.

The property test should assert each counter that corresponds to a required
generated behavior is nonzero.

### 10. Source Guards

`branch_lsm_source_guard.rs` must continue to reject production `branch/`
matches for:

1. `crate::backend`;
2. `crate::service`;
3. `crate::lifecycle`;
4. `crate::commit`;
5. `crate::format` direct table codec use outside L5 APIs;
6. `crates/storage/src` old storage APIs;
7. `VersionedValue`;
8. product `Value`, `Namespace`, `TypeTag`, and old `Key`;
9. `std::fs`, `std::path::Path`, and object layout path literals;
10. `SystemTime`, `Instant`, wall-clock `now`, or current-process time.

The guard should allow L5 table builder/reader/runtime types and storage-next
row/key types.

## Sensitivity Probes

Before closing L6H, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. remove the fork-version filter during materialization;
2. copy source branch id instead of rewriting to child branch id;
3. drop all inherited rows for a key when any child row exists;
4. drop inherited tombstones;
5. drop expired rows during materialization;
6. compare TTL against wall-clock time;
7. remove inherited layer before replacement tables are installed;
8. let a stale pinned read view observe post-materialization state;
9. treat different commit versions of one physical key as duplicates;
10. materialize an unavailable layer;
11. skip table build source-chain preservation;
12. import backend or old storage APIs into production branch code.

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

If L6H adds table output splitting helpers, also run the L5 table builder and
reader tests:

```bash
cargo test -p strata-storage-next --locked --lib table
```

## Exit Criteria

L6H test coverage is complete when:

1. direct tests cover request validation, row rewrite, table build, transition,
   pinned views, and idempotency;
2. generated tests cover fork/materialize/read parity scripts;
3. read parity is checked for latest, getv, as-of, history, prefix, and range
   reads;
4. tombstones, TTL-expired rows, and historical versions are preserved;
5. materialization performs no cleanup without a retention proof;
6. source guards enforce L6 boundaries;
7. the porting log records preserved old behavior, changed boundaries,
   deferred durable work, and sensitivity probes;
8. all verification commands pass.
