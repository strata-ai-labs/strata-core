# L6E Test Plan: Branch-Owned Immutable Levels

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`

## Goal

Prove that L6E correctly installs and reads branch-owned immutable L5 tables
without importing durable IO, backend services, inherited branches, or product
DTOs.

The suite must fail if L6E:

1. installs an immutable table containing rows from another branch;
2. mutates branch state after a failed install;
3. removes a frozen table before its immutable replacement is accepted;
4. accepts overlapping L1+ key ranges;
5. treats source order as more important than commit-version visibility;
6. loses immutable rows from latest, getv, history, prefix, or range reads;
7. changes a pinned read view after immutable install or frozen replacement;
8. returns more than one visible row per physical key in scans;
9. falls through selected tombstones to older puts;
10. imports backend, object layout, lifecycle, commit runtime, engine, or
    product payload APIs.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local direct tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for L6 boundary
   source scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated branch-LSM
   scripts.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   generated failure captures a minimized seed.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   sensitivity-probe and source-map recording.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, L5 table runtime types, and L6
branch result shells. Tests must not use old storage `Key`, `Value`,
`Namespace`, `TypeTag`, `VersionedValue`, engine workflow types, backend
handles, filesystem paths, wall-clock time, or product payload vocabulary.

## Independent Model

Generated and direct tests should compare production output against an
independent model that includes active, frozen, and branch-owned immutable
sources.

Suggested model:

```text
ModelBranch {
  branch_id
  active_rows: Vec<ModelRow>
  frozen_tables: Vec<Vec<ModelRow newest-table-first>>
  owned_levels: Vec<Vec<ModelTable>>
}

ModelTable {
  level
  table_index
  identity
  rows: Vec<ModelRow>
  key_range
  commit_range
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

1. group rows by physical key;
2. sort row chains by commit version descending;
3. apply latest and version bounds;
4. select the first in-bound row for visible reads;
5. treat selected tombstones as shadowing older rows;
6. preserve tombstones in history;
7. apply history `before_version` as exclusive;
8. apply limits after filtering;
9. scan physical keys in encoded storage order;
10. emit at most one visible row per physical key;
11. preserve expiry facts without applying TTL policy;
12. reject timestamp-bound requests as deferred until L6G;
13. independently validate L0 overlap and L1+ non-overlap rules.

The model should not call production `BranchReadView`, source-selection
helpers, or branch immutable install helpers. It may use L5 builders/readers to
create valid table artifacts for production input, but expected results should
come from model rows.

## Generators

### Branch IDs

Generate opaque branch ids:

1. all zero bytes;
2. all `0xff` bytes;
3. repeated byte ids;
4. incrementing byte ids;
5. pairs differing only in first byte;
6. pairs differing only in last byte.

No generator should assign branch names or lifecycle meaning.

### Table Identities

Generate table identities:

1. short single-component strings;
2. long bounded single-component strings;
3. similar identities differing at the end;
4. invalid empty identities;
5. invalid identities containing `/`;
6. invalid identities containing NUL.

Invalid identities should be rejected by L5 or descriptor construction before
branch mutation.

### Physical Keys And Rows

Generate rows over:

1. multiple valid space names;
2. engine-owned storage-space ids;
3. storage-owned nonzero storage-space ids when the row layer allows them;
4. empty user keys;
5. user keys containing `0x00`;
6. high-bit bytes;
7. adjacent prefix-like keys;
8. same physical key at multiple commit versions;
9. different physical keys at the same commit version;
10. `CommitVersion::ZERO`;
11. `CommitVersion::MAX`;
12. increasing and non-monotonic commit timestamps;
13. `Timestamp::EPOCH`;
14. `Timestamp::MAX`;
15. put rows with empty values;
16. tombstones.

### Immutable Table Shapes

Generate immutable tables with:

1. one row;
2. many rows;
3. one physical key with many versions;
4. many physical keys with one version each;
5. mixed puts and tombstones;
6. empty-value puts;
7. rows split across multiple L5 data blocks;
8. identity and zstd table encodings if L5 runtime config exposes both;
9. key ranges that exactly touch but do not overlap;
10. key ranges that overlap by one key;
11. key ranges where one range contains another;
12. tables whose row branch id differs from the target branch.

### Branch State Shapes

Generate branch states with:

1. immutable-only state;
2. active plus immutable state;
3. frozen plus immutable state;
4. active, frozen, and immutable state;
5. one L0 table;
6. multiple overlapping L0 tables;
7. multiple disjoint L1 tables;
8. multiple levels with L1 and L2 tables;
9. L0 row newer than active row;
10. active row newer than L0 row;
11. L1 row newer than L0 row;
12. L0 tombstone shadowing an older L1 put;
13. frozen replacement into L0.

### Operation Scripts

Generated scripts should exercise:

1. build valid immutable table through L5;
2. construct branch-owned immutable table descriptor;
3. install L0 table;
4. install L1+ disjoint table;
5. reject overlapping L1+ table;
6. reject wrong-branch table;
7. reject table descriptor/fact mismatch;
8. replace frozen table with L0 table;
9. capture read view before install;
10. capture read view after install;
11. latest point read;
12. version-bounded point read;
13. retained history read;
14. prefix scan;
15. range scan;
16. wrong-branch read rejection;
17. timestamp-bound deferred request.

## Required Direct Tests

### 1. Branch-Owned Table Descriptor

1. Valid descriptor plus matching `ImmutableTableReader` is accepted.
2. Descriptor identity mismatch is rejected.
3. Descriptor facts mismatch is rejected.
4. Descriptor level mismatch is rejected.
5. Reader rows from the wrong branch are rejected.
6. Empty table input is rejected before branch install.
7. Errors do not include value bytes.
8. L5 `TableRuntimeError` sources are preserved where applicable.

### 2. Level Layout

1. Empty branch state has zero owned tables.
2. Installing one L0 table increments owned table count.
3. Installing multiple L0 tables keeps newest table at index 0.
4. L0 overlapping key ranges are accepted.
5. L1+ disjoint key ranges are accepted and sorted.
6. L1+ overlapping key ranges are rejected.
7. Level index outside `BranchRuntimeConfig::max_level_count` is rejected.
8. Failed level install leaves active, frozen, immutable levels, and facts
   unchanged.
9. Branch facts include immutable max commit version and timestamp range.

### 3. Frozen Replacement

1. Replacing frozen table index 0 with an equivalent L0 table succeeds.
2. Replacement inserts the L0 table before older L0 tables.
3. Replacement removes exactly the named frozen table.
4. Replacement rejects out-of-range frozen index.
5. Replacement rejects immutable rows that differ from the frozen source rows.
6. Replacement failure leaves the frozen table visible and immutable levels
   unchanged.
7. A read view captured before replacement still sees the frozen source.
8. A read view captured after replacement sees the L0 source.
9. Reads before and after replacement return the same visible rows.

### 4. Latest Point Reads

1. Immutable-only latest returns the newest live immutable row.
2. Newer immutable tombstone makes latest return `None`.
3. Active newer put beats older immutable put.
4. Immutable newer put beats older active put.
5. Frozen newer put beats older immutable put.
6. Immutable newer put beats older frozen put.
7. Overlapping L0 tables choose by commit version, not table index alone.
8. L1+ tables participate in point reads by key range.
9. Result source facts report `OwnedTable { level, table_index }` for selected
   immutable rows.

### 5. Version-Bounded Reads

1. `getv(V)` returns newest immutable row with commit version `<= V`.
2. Rows above `V` are ignored even if they are in active or L0.
3. Tombstone at or below `V` hides older immutable puts.
4. Tombstone above `V` does not hide older immutable puts visible at `V`.
5. `CommitVersion::ZERO` and `CommitVersion::MAX` work over immutable sources.
6. Version-bounded reads return correct source facts.

### 6. History Reads

1. History includes active, frozen, and immutable rows newest first.
2. History includes immutable tombstones by default.
3. History can exclude tombstones without dropping live immutable rows.
4. `before_version` excludes immutable rows at or above the bound.
5. `limit = 0` returns empty.
6. Limits apply after tombstone filtering.
7. Exact same physical key across multiple immutable levels is not collapsed
   across distinct commit versions.

### 7. Prefix Scans

1. Immutable-only prefix scan returns one visible row per physical key.
2. Prefix scan merges active, frozen, L0, and L1+ sources.
3. Prefix scan omits keys whose selected row is an immutable tombstone.
4. Prefix scan does not fall through immutable tombstones to older puts.
5. Prefix scan respects embedded zero bytes and high-bit user keys.
6. Prefix scan does not cross space or storage-space-id boundaries.
7. Prefix scan output is ordered by encoded physical key.
8. L0 overlap does not emit duplicate visible keys.

### 8. Range Scans

1. Immutable-only range scan returns one visible row per physical key.
2. Inclusive and exclusive lower bounds work with immutable rows.
3. Inclusive and exclusive upper bounds work with immutable rows.
4. Degenerate open range returns empty.
5. Degenerate closed range returns the matching immutable row.
6. L1+ range boundary checks include edge keys exactly at table range bounds.
7. Adjacent disjoint L1+ ranges do not over-include.
8. Range scan output is sorted by encoded physical key.

### 9. Pinned Read Views

1. A read view captured before L0 install does not see the new L0 table.
2. A read view captured after L0 install sees the new L0 table.
3. A read view captured before frozen replacement still sees the frozen table.
4. A read view captured after frozen replacement sees the L0 table.
5. A read view captured before a failed install is unchanged.
6. Captured facts remain stable after immutable install.
7. Captured source counts remain stable after immutable install.

### 10. Boundary Non-Behavior

L6E must still not add:

1. object-backed table loading from backend or L4 services;
2. durable branch/table reachability publication;
3. inherited-layer reads;
4. fork creation;
5. materialization;
6. compaction scheduling or replacement policy;
7. snapshot row install;
8. timestamp/as-of completion;
9. public storage API mapping.

These should be source-guard assertions where possible.

## Generated Testkit Coverage

The branch-LSM testkit route should add nonzero counters for:

1. immutable table descriptor cases;
2. immutable L0 install cases;
3. immutable L1+ install cases;
4. invalid immutable install rejection cases;
5. L1+ overlap rejection cases;
6. frozen replacement cases;
7. pinned immutable install isolation cases;
8. immutable latest read cases;
9. immutable version-bounded read cases;
10. immutable history cases;
11. immutable prefix scan cases;
12. immutable range scan cases;
13. immutable tombstone shadow cases;
14. active/frozen/immutable merge cases;
15. immutable source attribution cases.

The external property test should assert every L6E counter is nonzero.

## Source Guard Requirements

`branch_lsm_source_guard.rs` must continue scanning production
`crates/storage-next/src/branch/` files and fail on forbidden dependencies or
vocabulary.

### Required Guard Update

The premature-behavior guard must be narrowed:

1. allow L6E-owned immutable table descriptor helpers;
2. allow L6E-owned immutable level install helpers;
3. allow L6E-owned immutable source read-view helpers;
4. continue rejecting fork/materialization APIs;
5. continue rejecting compaction APIs;
6. continue rejecting snapshot install APIs;
7. continue rejecting backend/object/lifecycle APIs;
8. continue rejecting completed timestamp/as-of read APIs until L6G lands.

### Forbidden Imports And Vocabulary

Keep the existing L6 guard categories:

1. no `crate::backend`;
2. no `crate::service`;
3. no `crate::layout`;
4. no `crate::commit`;
5. no `crate::lifecycle`;
6. no engine crates;
7. no `std::fs`, `std::path`, `File`, `OpenOptions`, `mmap`, or `pread`;
8. no `std::env`;
9. no product DTO vocabulary such as `VersionedValue`, `Value`, `Key`,
   `Namespace`, `TypeTag`, `EntityRef`, JSON, graph, vector, search, or
   transaction context.

## Sensitivity Probes

Add tests that would fail under these mutations:

1. remove branch-id validation for immutable rows;
2. install L1+ overlapping ranges;
3. remove frozen table before validating replacement table;
4. make L0 oldest-first instead of newest-first for source facts;
5. choose active source before newer immutable commit version;
6. skip immutable rows in history;
7. drop immutable tombstones from visible-read selection;
8. make scans emit one row per source rather than one row per physical key;
9. let invalid direct scan spaces through;
10. let object/backend imports into production branch code.

## Porting Log Requirements

The L6E entry must record:

1. old source files read;
2. storage-next files changed;
3. branch-owned immutable source shape;
4. L0 and L1+ ordering rules;
5. frozen replacement rules;
6. read-view pinning behavior;
7. direct and generated test names;
8. source-guard updates;
9. deferred L6F/L6G/L6I/L6J/L8 work.

## Verification Commands

Mandatory L6E commands:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run the full package suite before closing:

```bash
cargo test -p strata-storage-next --locked
```

## Completion Criteria

L6E test coverage is complete when:

1. direct tests cover immutable descriptor validation, L0 install, L1+ install,
   frozen replacement, failed-install non-mutation, read-view pinning, and all
   read methods over immutable sources;
2. generated tests exercise every L6E counter with nonzero coverage;
3. source guards allow only L6E-owned immutable-level behavior and still reject
   higher-layer responsibilities;
4. tests prove table rows remain storage-owned facts and no product DTOs enter
   L6;
5. all mandatory verification commands pass.

## Deferred

1. Object-backed table loading belongs to L8/L4 integration.
2. Durable branch/table reachability belongs to L6I/L8.
3. Inherited layers belong to L6F.
4. Timestamp/as-of and TTL visibility belong to L6G.
5. Materialization belongs to L6H.
6. Branch compaction and immutable replacement policy belong to L6J.
7. Snapshot row install belongs to L6K.
8. Public API mapping belongs above L6.
