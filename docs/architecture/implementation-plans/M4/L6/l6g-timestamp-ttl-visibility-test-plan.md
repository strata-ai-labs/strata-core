# L6G Test Plan: Timestamp Reads And TTL Visibility

Status: implemented in storage-next; direct and generated sensitivity probes are covered

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md`

## Goal

Prove that L6G correctly implements timestamp-bounded branch reads and TTL
visibility over storage-next row chains.

The suite must fail if L6G:

1. still rejects `BranchReadBound::AtTimestamp`;
2. selects rows with commit timestamps greater than the requested timestamp;
3. sorts timestamp reads by timestamp instead of filtering by timestamp and
   selecting by newest commit version;
4. evaluates TTL against wall-clock time;
5. treats exact expiry as visible;
6. falls through a selected tombstone to an older put;
7. falls through a selected expired put to an older put;
8. lets inherited rows bypass fork-version gates during timestamp reads;
9. groups inherited scans before rewriting inherited keys into the child
   branch namespace;
10. returns `None` without typed facts when a coverage proof says timestamp
    history is insufficient;
11. imports old storage DTOs, product value wrappers, backend APIs, or
    wall-clock APIs into production `branch/` code.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for direct module-local tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for source-boundary
   and wall-clock guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated scripts and
   the independent model.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only when a
   generated failure captures a minimized seed.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   source-map, sensitivity-probe, and closeout notes.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, L5 table runtime types, and L6 branch
read result types. Tests must not use old storage `Key`, `Value`, `Namespace`,
`TypeTag`, `VersionedValue`, engine workflow types, backend handles, filesystem
paths, product payload vocabulary, wall-clock time, or current-process time.

## Independent Model

Generated and direct tests should compare production output against a model
that applies timestamp bounds and TTL after collecting own and inherited row
candidates.

Suggested model:

```text
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

For a timestamp read at `T`, the model should:

1. collect child-local rows without branch rewrite;
2. collect inherited rows only from readable inherited layers;
3. drop inherited rows whose commit version is greater than the layer fork
   version;
4. rewrite inherited row branch id from source to child before grouping;
5. drop any row whose commit timestamp is greater than `T`;
6. group by rewritten physical key;
7. sort each row chain by commit version descending;
8. break exact internal-key ties using existing L6 source ordering;
9. select the first row in the sorted eligible chain;
10. return no visible row when the selected row is a tombstone;
11. return no visible row when the selected put row has
    `expires_at != Timestamp::EPOCH && expires_at <= T`;
12. return the selected put row otherwise;
13. preserve source facts, commit timestamp, expiry, tombstone flag, and value
    bytes in expected rows;
14. reject a timestamp read before candidate collection only when a model
    coverage proof says history is known insufficient.

The model must not call production `BranchReadView` candidate collection,
visibility helpers, or source ordering helpers. It may use L5 builders/readers
to build valid immutable tables for production input, but expected rows must
come from model rows.

## Generators

### Timestamp Bounds

Generate timestamp reads at:

1. `Timestamp::EPOCH`;
2. exactly one row commit timestamp;
3. one microsecond before a row commit timestamp when representable;
4. one microsecond after a row commit timestamp when representable;
5. between two row commit timestamps;
6. after every retained row timestamp;
7. `Timestamp::MAX`;
8. random bounded timestamps drawn from the row set.

### Row Chains

Generate row chains with:

1. one version;
2. many versions of one physical key;
3. increasing timestamps with increasing commit versions;
4. equal timestamps at different commit versions;
5. non-monotonic timestamps;
6. newest row above timestamp bound and older row inside bound;
7. newest row inside timestamp bound and older row with a later timestamp;
8. empty values;
9. high-bit and embedded-zero user keys;
10. multiple named spaces and storage-space ids;
11. tombstones at old, middle, and newest versions.

### TTL Facts

Generate put rows with:

1. `expires_at == Timestamp::EPOCH`;
2. expiry before requested timestamp;
3. expiry exactly equal to requested timestamp;
4. expiry after requested timestamp;
5. expiry at `Timestamp::MAX`;
6. empty value plus expiry;
7. newer expired put above older live put;
8. older expired put below newer live put;
9. tombstones, which ignore expiry.

### Inherited Layers

Generate inherited scenarios with:

1. inherited row version below fork and timestamp below bound;
2. inherited row version exactly at fork and timestamp below bound;
3. inherited row version above fork and timestamp below bound;
4. inherited row version below fork and timestamp above bound;
5. child-local put shadowing inherited put at timestamp;
6. child-local tombstone shadowing inherited put at timestamp;
7. child-local expired put suppressing inherited put at timestamp;
8. nearest inherited layer winning exact ties after timestamp filtering;
9. parent/source mutation after child read-view capture;
10. inherited prefix/range scans that require rewrite before grouping.

### Timestamp Coverage

Generate coverage facts:

1. unknown coverage;
2. complete coverage;
3. complete-since coverage where requested timestamp is before the floor;
4. complete-since coverage where requested timestamp equals the floor;
5. complete-since coverage where requested timestamp is after the floor;
6. own-state insufficient coverage;
7. inherited-layer insufficient coverage once inherited coverage facts exist;
8. combined coverage where one side is unknown and the other is complete.

## Required Direct Tests

### 1. Timestamp Point Reads

1. Own active timestamp read returns the newest row with
   `commit_timestamp <= T`.
2. Own frozen timestamp read returns the newest row with
   `commit_timestamp <= T`.
3. Own immutable timestamp read returns the newest row with
   `commit_timestamp <= T`.
4. Rows above the timestamp bound are ignored and cannot shadow older eligible
   rows.
5. A row exactly at the timestamp bound is eligible.
6. A row one tick after the timestamp bound is ineligible.
7. A non-monotonic timestamp row chain selects by highest eligible commit
   version, not by highest timestamp.
8. Empty values survive timestamp selection.
9. Wrong-branch timestamp reads fail before candidate collection or payload
   inspection.

### 2. Tombstone Semantics

1. Tombstone at or before `T` hides older puts for the same physical key.
2. Tombstone after `T` does not hide older puts visible at `T`.
3. Tombstone exactly at `T` hides older puts.
4. Tombstone selection returns `None`, not an empty value row.
5. History with tombstones included preserves tombstone timestamp and source
   facts.

### 3. TTL Semantics

1. Put row with `expires_at == Timestamp::EPOCH` is visible at any timestamp
   bound where its commit timestamp is eligible.
2. Put row is visible before expiry.
3. Put row is invisible exactly at expiry.
4. Put row is invisible after expiry.
5. Expired selected put suppresses the key instead of falling through to an
   older put.
6. TTL is evaluated against the requested timestamp, not wall-clock time.
7. Tombstone expiry sentinel is ignored.
8. `Timestamp::MAX` expiry behaves as far-future expiry.

### 4. Timestamp Scans

1. Prefix scan applies timestamp eligibility per key.
2. Range scan applies timestamp eligibility per key.
3. Prefix scan suppresses a key whose selected row is a tombstone.
4. Prefix scan suppresses a key whose selected row is expired at `T`.
5. Range scan preserves inclusive/exclusive user-key edges under timestamp
   filtering.
6. Scan output remains sorted by branch-local physical key.
7. Scan does not leak across branch id, named space, or storage-space id.
8. Scan with no eligible rows returns an empty result.

### 5. Inherited Timestamp Reads

1. Inherited timestamp point read returns a row below/equal fork version and
   below/equal timestamp bound.
2. Inherited row above fork version is hidden even if timestamp is below the
   requested timestamp.
3. Inherited row below fork version is hidden when timestamp is above the
   requested timestamp.
4. Child-local put shadows inherited put at the timestamp read.
5. Child-local tombstone shadows inherited put at the timestamp read.
6. Child-local expired put suppresses inherited put at the timestamp read.
7. Inherited scans rewrite source keys into child keys before grouping.
8. Nearest inherited layer wins exact internal-key ties after timestamp
   filtering.
9. Pinned child timestamp view is stable after source branch mutation.

### 6. Timestamp Coverage

1. Unknown coverage does not fail a timestamp read by itself.
2. Complete coverage never fails due to retained-history coverage.
3. Complete-since coverage rejects requested timestamps before its floor with
   typed insufficient-history facts.
4. Complete-since coverage accepts requested timestamp equal to its floor.
5. Complete-since coverage accepts requested timestamp after its floor.
6. Observed `timestamp_min` alone does not cause an insufficient-history error.
7. Insufficient-history errors include branch id, requested timestamp,
   earliest available timestamp when known, and source classification.
8. Insufficient-history errors do not include user value bytes.

### 7. Pinned Views

1. Timestamp point read over a captured read view remains stable after active
   append.
2. Timestamp point read over a captured read view remains stable after active
   rotation/freeze.
3. Timestamp point read over a captured read view remains stable after owned
   table install.
4. Timestamp inherited read over a captured child view remains stable after
   source branch mutation.
5. Timestamp scans over captured views remain stable after later branch-local
   mutations.

### 8. History Preservation

1. Storage history preserves commit timestamp facts.
2. Storage history preserves expiry facts.
3. Storage history preserves tombstone facts when requested.
4. Storage history preserves empty values.
5. Storage history remains newest-first by commit version.
6. History does not apply TTL filtering unless an explicit timestamp-history
   option is added in L6G.

### 9. Source Guards

1. Production `branch/` code contains no `Timestamp::now`.
2. Production `branch/` code contains no `SystemTime`.
3. Production `branch/` code contains no `Instant::now`.
4. Production `branch/` code contains no `std::time`.
5. Production `branch/` code contains no old storage `VersionedValue`.
6. Production `branch/` code contains no old storage `Value`, `Key`,
   `Namespace`, or `TypeTag` DTO vocabulary.
7. Production `branch/` code contains no backend, service, lifecycle, WAL, or
   manifest IO calls.
8. Source guard self-tests prove each forbidden token is caught.
9. The previous L6F "timestamp reads are premature" guard is removed or
   narrowed so it no longer rejects the L6G implementation.

## Required Generated Counters

The generated branch-LSM property harness must assert nonzero counts for:

1. own active timestamp point reads;
2. frozen timestamp point reads;
3. owned immutable timestamp point reads;
4. inherited timestamp point reads;
5. timestamp prefix scans;
6. timestamp range scans;
7. TTL before expiry;
8. TTL exact expiry;
9. TTL after expiry;
10. `Timestamp::MAX` expiry as a real far-future expiry;
11. tombstone-at-timestamp shadowing;
12. tombstone-after-timestamp non-shadowing;
13. timestamp scan boundary preservation;
14. timestamp scan key-space isolation;
15. timestamp scan empty-result behavior;
16. non-monotonic timestamp row chains;
17. inherited fork-version gate plus timestamp gate;
18. inherited child-local put shadowing;
19. inherited child-local tombstone shadowing;
20. nearest inherited layer exact-tie selection;
21. insufficient-history rejection when coverage proof is known insufficient;
22. unknown-coverage timestamp reads.

## Sensitivity Probes

Add direct tests or generated probes that would fail if a mutation:

1. changes timestamp comparison from `<=` to `<`;
2. changes exact expiry from invisible to visible;
3. evaluates expiry against wall clock;
4. sorts timestamp reads by timestamp instead of commit version;
5. lets expired selected rows fall through to older puts;
6. lets selected tombstones fall through to older puts;
7. skips inherited fork-version gates for timestamp reads;
8. groups inherited scans before rewriting inherited branch ids;
9. treats `Timestamp::EPOCH` expiry as already expired;
10. treats `Timestamp::MAX` expiry as the no-expiry sentinel;
11. ignores timestamp scan range inclusivity/exclusivity;
12. leaks timestamp scan rows across named or storage-space key domains;
13. lets child-local timestamp-visible puts/tombstones fall through to
    inherited rows;
14. picks a farther inherited layer for an exact timestamp/key/version tie;
15. infers insufficient history from observed `timestamp_min` without coverage
    proof;
16. leaks value bytes in wrong-branch or insufficient-history errors.

## Verification Commands

Required L6G command set:

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

Run the broader storage-next suite if L6G changes shared branch, row, or table
helpers:

```bash
cargo test -p strata-storage-next --locked
```

## Exit Criteria

L6G testing is complete when:

1. direct tests cover every required timestamp, TTL, tombstone, inherited, and
   coverage case above;
2. generated tests exercise every required counter at least once;
3. source guards reject wall-clock, product DTO, old storage, and IO drift;
4. wrong-branch and insufficient-history errors do not leak payload bytes;
5. no test relies on current process time;
6. all verification commands pass;
7. the porting log names every direct/generated test category and records old
   wall-clock TTL behavior as intentionally not ported.
