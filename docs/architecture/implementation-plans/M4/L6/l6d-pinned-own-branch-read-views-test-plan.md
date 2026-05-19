# L6D Test Plan: Pinned Own-Branch Read Views

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md`

## Goal

Prove that L6D correctly reads own-branch active/frozen state through pinned
read views, using storage-owned row facts and L5 table mechanics only.

The suite must fail if L6D:

1. returns a row from the wrong branch;
2. loses a retained row version from history;
3. chooses a lower commit version when a newer in-bound version exists;
4. falls through a selected tombstone to an older put for visible reads;
5. returns more than one visible row per physical key in scans;
6. changes a captured read view after later appends or rotations;
7. treats source order as more important than commit-version ordering;
8. applies wall-clock TTL or timestamp/as-of policy before L6G;
9. imports backend, object layout, lifecycle, commit runtime, or engine APIs;
10. exposes product DTOs such as `VersionedValue`, `Value`, `Key`,
    `Namespace`, or `TypeTag`.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local direct tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for L6 boundary
   source scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated read-view
   script checks.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only if a
   generated failure captures a minimized seed.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   sensitivity-probe and source-map recording.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, L5 table runtime types, and L6
branch result shells. Tests must not use old storage `Key`, `Value`,
`Namespace`, `TypeTag`, `VersionedValue`, engine branch workflow types, backend
handles, filesystem paths, wall-clock time, or product payload vocabulary.

## Independent Model

Generated and direct tests should compare production output against an
independent own-branch model, not against production helper functions.

Suggested model:

```text
ModelBranch {
  branch_id
  rows: Vec<ModelRow>
}

ModelRow {
  physical_key
  commit_version
  commit_timestamp
  expires_at
  is_tombstone
  value
  source_index
}
```

The model should:

1. group rows by physical key;
2. sort each row chain by commit version descending;
3. apply latest and version bounds;
4. select the first in-bound row for visible point reads;
5. treat selected tombstones as shadowing older rows;
6. preserve tombstones in history;
7. apply history `before_version` as exclusive;
8. apply history limits after filtering;
9. scan physical keys in encoded storage order;
10. emit at most one visible row per physical key for scans;
11. preserve expiry facts without applying TTL policy;
12. reject timestamp-bound requests as deferred until L6G.

The model should not call production `BranchReadView`, `BranchVisibleRow`
selection helpers, L5 merge cursors, or encoded-key helper functions except for
constructing valid storage-next rows and comparing final encoded ordering when
needed.

## Generators

### Branch IDs

Generate opaque branch ids:

1. all zero bytes;
2. all `0xff` bytes;
3. repeated byte ids;
4. incrementing byte ids;
5. pairs differing only in first byte;
6. pairs differing only in last byte;
7. source/read key pairs where the wrong branch is one bit away.

No generator should assign branch names or lifecycle meaning.

### Physical Keys

Generate physical keys over:

1. valid nonempty space strings;
2. multiple space names sharing prefixes;
3. storage-owned nonzero `StorageSpaceId` values;
4. engine-owned `StorageSpaceId` values;
5. empty user keys;
6. user keys containing `0x00`;
7. user keys containing `0x00 0x00`;
8. high-bit bytes;
9. long shared-prefix keys;
10. adjacent keys where one user key is a prefix of another;
11. many keys sharing the same physical-key prefix but different commit
    versions.

### Storage Rows

Generate committed rows with:

1. put rows with empty, small, and bounded values;
2. tombstone rows;
3. same physical key at adjacent commit versions;
4. same physical key at non-adjacent commit versions;
5. different physical keys at the same commit version;
6. `CommitVersion::ZERO`;
7. `CommitVersion::MAX`;
8. timestamps that increase with commit version;
9. timestamps that decrease relative to commit version;
10. equal timestamps across different versions;
11. `Timestamp::EPOCH`;
12. `Timestamp::MAX`;
13. nonzero expiry on put rows;
14. tombstones with empty values.

L6D should never allocate commit versions. The generator must supply already
committed rows.

### State Shapes

Generate active/frozen layouts:

1. empty active and no frozen tables;
2. active-only state;
3. frozen-only state;
4. mixed active/frozen state;
5. multiple frozen tables newest first;
6. one row chain split across active and frozen tables;
7. one row chain split across multiple frozen tables;
8. many independent physical keys distributed across active/frozen sources;
9. active rows with lower versions than frozen rows, to prove version
   selection does not blindly prefer active;
10. frozen rows with lower versions than active rows, to prove normal newest
    active cases still work.

### Operation Scripts

Generated scripts should exercise:

1. state construction;
2. append committed put row;
3. append committed tombstone;
4. rotate active;
5. capture read view;
6. append after capture;
7. rotate after capture;
8. latest point read;
9. version-bounded point read;
10. retained history read;
11. prefix scan;
12. range scan;
13. wrong-branch point read;
14. wrong-branch scan;
15. timestamp-bound deferred request.

The harness must compute or encode expected results independently for every
read operation. L6D may use compact generated fixtures with independent
expected vectors; later immutable/inherited slices can promote this into a
reusable randomized model as the source list grows.

## Required Direct Tests

### 1. Read View Capture

1. Capturing a view from an empty branch succeeds.
2. Captured view records the expected branch id.
3. Captured view records empty branch facts.
4. Capturing a view from active-only state pins active rows.
5. Capturing a view from frozen-only state pins frozen rows.
6. Capturing a view from mixed active/frozen state pins both source sets.
7. Captured frozen order is newest first.
8. Captured view exposes no mutable handles.
9. Direct construction rejects stale facts, wrong-branch source rows, and
   immutable/inherited fact counts before those sources are implemented.

### 2. Pinned View Isolation

1. Appending a row after capture does not affect the captured view.
2. Rotating active after capture does not affect the captured view.
3. Capturing a second view after mutation sees the new state.
4. A view captured before a tombstone append still sees the prior put.
5. A view captured after a tombstone append sees the tombstone shadow.
6. A frozen-limit skip after capture does not affect the captured view.
7. Captured view facts remain unchanged after source-state mutation.

### 3. Latest Point Reads

1. Empty state latest returns `None`.
2. Single put latest returns that put row.
3. Multiple versions of one physical key return the highest commit version.
4. Newer tombstone makes latest return `None`.
5. Older tombstone below a newer put does not hide the newer put.
6. Newer empty-value put is returned as a live put.
7. Latest preserves commit version, commit timestamp, expiry, and source facts.
8. Latest over mixed active/frozen state chooses by row-chain order, not by
   source order alone.
9. Latest rejects wrong-branch physical keys without reading table rows.
10. Latest error display does not include value bytes.

### 4. Version-Bounded Point Reads

1. `getv(V)` returns newest row with commit version `<= V`.
2. Rows with commit version greater than `V` are ignored.
3. Empty result is returned when all rows are above `V`.
4. Tombstone at or below `V` hides older put rows.
5. Tombstone above `V` does not hide older put rows visible at `V`.
6. `CommitVersion::ZERO` is a valid exact bound.
7. `CommitVersion::MAX` is equivalent to latest for version selection.
8. Version bounds work over active-only, frozen-only, and mixed states.
9. Version-bounded read result source facts point to the selected source.

### 5. History Reads

1. Empty state history returns an empty vector.
2. History returns all retained versions newest first.
3. History includes tombstones by default.
4. History preserves empty put values.
5. History preserves expiry facts without filtering.
6. `before_version` excludes rows with version `>= before_version`.
7. `before_version = ZERO` returns empty.
8. `limit = Some(0)` returns empty.
9. `limit = Some(n)` truncates after filtering.
10. History over mixed active/frozen state deduplicates only exact duplicate
    internal keys if such a defensive path exists; it must not collapse
    distinct commit versions.

### 6. Prefix Scans

1. Empty prefix over an empty state returns empty.
2. Prefix scan returns one visible put per physical key under the prefix.
3. Prefix scan omits a key whose selected row is a tombstone.
4. Prefix scan does not fall through a selected tombstone to an older put.
5. Prefix scan includes empty-value puts.
6. Prefix scan respects embedded zero bytes in user-key prefixes.
7. Prefix scan respects high-bit user-key bytes.
8. Prefix scan does not cross space boundaries.
9. Prefix scan does not cross storage-space-id boundaries.
10. Prefix scan output is ordered by physical key bytes.
11. Prefix scan over mixed active/frozen state matches the independent model.

### 7. Range Scans

1. Empty range over an empty state returns empty.
2. Range scan respects inclusive lower bounds if supported.
3. Range scan respects exclusive lower bounds if supported.
4. Range scan respects inclusive upper bounds if supported.
5. Range scan respects exclusive upper bounds if supported.
6. Degenerate empty ranges return empty.
7. Unbounded lower or upper shapes work if exposed.
8. Range scan returns one visible put per physical key in range.
9. Range scan omits tombstone-selected keys.
10. Range scan output is ordered by physical key bytes.
11. Range scan over adjacent prefix-like keys does not over-include.

If L6D chooses a narrower first range-bound surface, document the exact bound
forms and defer the rest in the porting log.

### 8. Source Merge Semantics

1. Active-only reads match model.
2. Frozen-only reads match model.
3. Mixed active/frozen reads match model.
4. Multiple frozen tables are searched newest first for source tie-break facts.
5. A higher-version frozen row beats a lower-version active row for latest.
6. A higher-version active row beats a lower-version frozen row for latest.
7. Same physical key at different versions is not treated as duplicate.
8. Different physical keys at the same version are independent.

### 9. Timestamp And TTL Deferral

1. `BranchReadBound::AtTimestamp` is explicitly rejected or unreachable from
   L6D public methods.
2. Timestamp-bound rejection is typed and payload-safe.
3. Rows with nonzero `expires_at` are not filtered by wall-clock time in L6D.
4. Expiry facts are preserved in returned rows and history.
5. Source guard rejects `Timestamp::now` and wall-clock imports in production
   `branch/` code.
6. Test names and docs point timestamp/as-of completion to L6G.

### 10. Boundary Non-Behavior

L6D may add read-view and own-branch read methods. It must still not add:

1. immutable table install;
2. object-backed table reads;
3. inherited-layer reads;
4. fork creation;
5. inherited-layer materialization;
6. reachability/shared table refs;
7. branch compaction;
8. snapshot row install;
9. backend or object publication;
10. product DTO conversion.

These should be source-guard assertions where possible.

## Generated Testkit Coverage

The branch-LSM testkit route should add nonzero counters for:

1. read-view capture cases;
2. pinned append isolation cases;
3. pinned rotation isolation cases;
4. latest point-read cases;
5. version-bounded point-read cases;
6. tombstone shadow point-read cases;
7. history cases;
8. history tombstone cases;
9. history limit cases;
10. prefix scan cases;
11. range scan cases;
12. scan tombstone suppression cases;
13. active/frozen merge cases;
14. wrong-branch read rejection cases;
15. timestamp-bound deferral cases.

The external property test should assert every L6D counter is nonzero.

## Source Guard Requirements

`branch_lsm_source_guard.rs` must continue scanning production
`crates/storage-next/src/branch/` files and fail on forbidden dependencies or
vocabulary.

### Required Guard Update

The premature-behavior guard must be narrowed:

1. allow L6D-owned read-view capture helpers;
2. allow L6D-owned own-branch latest/getv/history/prefix/range read helpers;
3. continue rejecting fork/materialization APIs;
4. continue rejecting immutable table install APIs;
5. continue rejecting compaction APIs;
6. continue rejecting snapshot install APIs;
7. continue rejecting backend/object/lifecycle APIs;
8. continue rejecting completed timestamp/as-of read APIs until L6G lands.

### Forbidden Imports And Vocabulary

Keep the existing L6 guard categories:

1. `crate::commit`
2. `crate::lifecycle`
3. `crate::api`
4. engine crates
5. `crate::backend`
6. direct `crate::service::wal`
7. direct `crate::service::checkpoint`
8. `crate::testkit` in production branch code
9. `VersionedValue`
10. `Versioned<`
11. `strata_core::Value`
12. `strata_core::Key`
13. `Namespace`
14. `TypeTag`
15. `EntityRef`
16. graph/vector/search/product payload vocabulary
17. filesystem/path/env APIs
18. wall-clock APIs such as `SystemTime`, `Instant::now`, or `Timestamp::now`
19. backend operation names such as `read_object`, `publish_object`,
    `delete_object`, and `list_prefix`
20. bare public production items such as `pub struct`, `pub fn`, and
    `pub use`

`pub(crate)` remains allowed.

## Porting-Log Requirements

The L6D entry must record:

1. current files read;
2. old `BranchSnapshot`, `get_versioned_from_snapshot`,
   `get_all_versions_from_snapshot`, `scan_prefix_from_snapshot`, and
   `scan_range_from_snapshot` behavior preserved;
3. old behavior intentionally not ported;
4. deferred behavior by owning slice;
5. tests and source guards added;
6. sensitivity probe categories mapped to permanent tests;
7. retirement status of old storage code.

The entry must not claim immutable table reads, inherited reads, timestamp/as-of
reads, TTL policy, materialization, compaction, or snapshot install are
implemented.

## Cross-Feature Matrix

Mandatory L6D commands:

| Mode | Purpose | Command |
|---|---|---|
| branch unit | branch read-view and own-read tests | `cargo test -p strata-storage-next --locked --lib branch` |
| source guards | L6 purity | `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` |
| generated state | branch testkit route | `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` |
| no-default generated | no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` |
| wasm/no-default | browser-compatible branch reads | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Run `cargo test -p strata-storage-next --locked` before closing L6D.

## Sensitivity Probe Coverage

L6D should close sensitivity coverage through permanent tests:

1. removing branch validation is covered by wrong-branch point and scan tests;
2. returning older puts after selected tombstones is covered by latest, getv,
   prefix, and range tombstone-shadow tests;
3. treating active source priority as higher than commit-version order is
   covered by mixed active/frozen model tests;
4. losing frozen rows after active rotation is covered by pinned-view mutation
   isolation tests;
5. collapsing history to only one visible row is covered by retained history
   tests;
6. dropping tombstones from storage history is covered by history tombstone
   tests;
7. crossing space or storage-space-id boundaries in scans is covered by prefix
   and range boundary tests;
8. adding wall-clock TTL behavior is covered by source guards and expiry-fact
   preservation tests;
9. adding product DTOs or backend reads is covered by source guards.

## Exit Gate

L6D test coverage is complete when:

1. direct tests cover read-view capture, latest, getv, history, prefix scans,
   range scans, tombstone shadowing, and pinned-view isolation;
2. direct tests prove wrong-branch reads fail without value-byte leakage;
3. direct tests prove timestamp/as-of and TTL policy are explicitly deferred;
4. generated tests exercise every L6D category with nonzero counters;
5. generated tests compare against an independent own-branch model;
6. source guards allow only L6D-owned read behavior and still reject
   upper-layer/backend/product behavior;
7. no test relies on product DTOs, wall-clock time, backend IO, or lifecycle
   scheduling;
8. no immutable table, inherited read, materialization, compaction, or snapshot
   behavior is implemented early;
9. all mandatory commands pass;
10. porting log records source map, deferrals, verification, and sensitivity
    probe coverage.
