# L6C Test Plan: Branch-Local Mutable And Frozen State

Status: implemented in storage-next; sensitivity probe categories are covered
by permanent direct, generated, and source-guard tests

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md`

## Goal

Prove that L6C correctly owns branch-local in-memory state over L5
`MutableTable` and `FrozenTable` without implementing reads, immutable table
install, inheritance, compaction, snapshot install, lifecycle, or backend IO.

The suite must fail if L6C:

1. accepts wrong-branch rows into a branch-local state;
2. mutates state before validation completes;
3. drops, rewrites, or interprets row facts during append;
4. accepts exact duplicate internal keys across active/frozen state;
5. rejects valid row-chain versions that share one physical key;
6. rotates active tables incorrectly or loses rows during rotation;
7. orders frozen tables oldest first;
8. reports incorrect max version or timestamp facts;
9. interprets tombstones or expiry as visible-value policy;
10. imports backend, filesystem, object layout, lifecycle, commit runtime, or
    engine APIs;
11. exposes public production branch API.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local direct tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for L6 boundary
   source scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated state-mutation
   script checks.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only if a
   generated failure captures a minimized seed.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   sensitivity-probe and source-map recording.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, `Timestamp`, and L5 table runtime types. They
must not use old storage `Key`, `Value`, `Namespace`, `TypeTag`,
`VersionedValue`, engine branch workflow types, backend handles, filesystem
paths, or wall-clock time.

## Generators

### Branch IDs

Generate opaque branch ids:

1. all zero bytes;
2. all `0xff` bytes;
3. repeated byte ids;
4. incrementing byte ids;
5. pairs differing only in first byte;
6. pairs differing only in last byte;
7. source/target pairs where the wrong branch is one bit away.

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
10. many keys sharing one physical-key prefix but different commit versions.

Invalid physical-key construction remains owned by the row layer. L6C tests
should focus on branch-state behavior after valid row construction.

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

L6C should never allocate commit versions. The generator must supply already
committed rows.

### Operation Scripts

Generated scripts should exercise:

1. state construction;
2. successful put append;
3. successful tombstone append;
4. wrong-branch append;
5. active duplicate append;
6. frozen duplicate append;
7. same physical key with different versions;
8. active rotation;
9. empty active rotation;
10. frozen-limit rotation;
11. facts after active-only state;
12. facts after frozen-only state;
13. facts after mixed active/frozen state.

## Required Direct Tests

### 1. State Construction

1. `BranchLocalState::new(branch_id, valid_config)` succeeds.
2. `BranchLocalState::empty(branch_id)` or equivalent succeeds.
3. Invalid config construction fails before state is created.
4. New state exposes the expected branch id.
5. New state has an empty active table.
6. New state has no frozen tables.
7. New state facts equal `BranchStateFacts::empty(branch_id)`.
8. New state has no max commit version.
9. New state has no timestamp range.

### 2. Successful Append

1. Appending a matching-branch put row succeeds.
2. Appending a matching-branch tombstone row succeeds.
3. Append outcome records branch id.
4. Append outcome records commit version.
5. Append outcome records commit timestamp.
6. Append outcome records tombstone status.
7. Active row count increases by one per successful append.
8. Put value bytes are preserved exactly, including empty values.
9. Tombstone shape is preserved and no value bytes are invented.
10. Expiry facts on put rows are preserved.
11. Same physical key at different commit versions is accepted.
12. Different physical keys at the same commit version are accepted.

### 3. Wrong-Branch Rejection

1. Wrong-branch put row is rejected.
2. Wrong-branch tombstone row is rejected.
3. Error is typed as `InvalidBranchRow` or the selected branch-row error.
4. Active row count is unchanged after rejection.
5. Frozen table count is unchanged after rejection.
6. Max commit version is unchanged after rejection.
7. Timestamp range is unchanged after rejection.
8. Error display does not include value bytes.

### 4. Duplicate Rejection

1. Exact duplicate internal key in active is rejected.
2. Exact duplicate internal key already frozen is rejected.
3. Duplicate rejection happens before mutation.
4. Active row count is unchanged after duplicate rejection.
5. Frozen table count is unchanged after duplicate rejection.
6. Facts are unchanged after duplicate rejection.
7. Same physical key with a different commit version is not rejected.
8. Different physical key with the same commit version is not rejected.
9. Duplicate error preserves L5 source chain if it wraps `TableRuntimeError`.
10. Duplicate error display does not include value bytes.

### 5. Version And Timestamp Facts

1. First append sets max commit version to the row version.
2. Later higher version updates max commit version.
3. Later lower version does not lower max commit version.
4. `CommitVersion::ZERO` is a valid max on a non-empty state.
5. `CommitVersion::MAX` is a valid max.
6. First append sets timestamp min and max to the row timestamp.
7. Lower timestamp updates timestamp min.
8. Higher timestamp updates timestamp max.
9. Equal timestamps are stable.
10. `Timestamp::EPOCH` is a valid min/max on a non-empty state.
11. `Timestamp::MAX` is a valid max.
12. Timestamp facts do not assume timestamps correlate with commit versions.

### 6. Rotation

1. Rotating an empty active table returns an explicit skipped outcome.
2. Empty rotation does not create an empty frozen table.
3. Rotating a non-empty active table succeeds.
4. Rotated active rows become a frozen table.
5. Active table is empty after successful rotation.
6. Frozen table count increases by one.
7. Frozen table is inserted at index zero.
8. Repeated rotations keep frozen tables newest first.
9. Frozen table contents remain unchanged after later appends to active.
10. Rotation preserves max commit version.
11. Rotation preserves timestamp min/max.
12. Rotation does not create immutable table descriptors or object names.

### 7. Frozen Limit

1. Config with `max_frozen_tables = 1` allows the first non-empty rotation.
2. A second non-empty rotation is skipped when frozen count is at the limit.
3. Skipped frozen-limit rotation has an explicit reason.
4. Active rows remain active after frozen-limit skip.
5. Frozen rows remain unchanged after frozen-limit skip.
6. Facts remain correct after frozen-limit skip.
7. Appending additional rows after frozen-limit skip remains allowed unless a
   later slice introduces write stalling.

### 8. Branch State Facts

1. Empty state facts are valid.
2. Active-only state reports active row count and zero frozen tables.
3. Frozen-only state reports zero active rows and nonzero frozen count.
4. Mixed active/frozen state reports both active rows and frozen count.
5. L6C facts report zero owned immutable table count.
6. L6C facts report zero inherited layer count.
7. Facts include max commit across active and frozen rows.
8. Facts include timestamp range across active and frozen rows.
9. Facts after failed append are identical to facts before the failure.
10. Facts after failed rotation are identical except for active rows that were
    intentionally kept active.

### 9. Boundary Non-Behavior

L6C may add append and rotation methods. It must still not add:

1. latest reads;
2. `getv` reads;
3. timestamp/as-of reads;
4. history reads;
5. prefix scans;
6. range scans;
7. fork creation;
8. inherited-layer materialization;
9. immutable table install;
10. branch compaction;
11. snapshot row install;
12. backend or object publication.

These may be source-guard assertions rather than runtime tests.

## Source Guard Requirements

`branch_lsm_source_guard.rs` must continue scanning production
`crates/storage-next/src/branch/` files and fail on forbidden dependencies or
vocabulary.

### Required Guard Update

The L6A premature-behavior guard must be narrowed:

1. allow L6C-owned committed-row append helpers;
2. allow L6C-owned active rotation helpers;
3. continue rejecting read APIs;
4. continue rejecting fork/materialization APIs;
5. continue rejecting immutable table install APIs;
6. continue rejecting compaction APIs;
7. continue rejecting snapshot install APIs;
8. continue rejecting backend/object/lifecycle APIs.

### Forbidden Imports And Vocabulary

Keep the existing L6A/L6B guard categories:

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
18. backend operation names such as `read_object`, `publish_object`,
    `delete_object`, and `list_prefix`
19. bare public production items such as `pub struct`, `pub fn`, and
    `pub use`

`pub(crate)` remains allowed.

## Generated Testkit Coverage

The branch-lsm testkit route should add nonzero counters for:

1. branch-local state construction;
2. successful put appends;
3. successful tombstone appends;
4. wrong-branch append rejections;
5. active duplicate rejections;
6. frozen duplicate rejections;
7. same-key different-version appends;
8. same-version different-key appends;
9. active rotations;
10. empty rotation skips;
11. frozen-limit rotation skips;
12. active-only facts;
13. frozen-only facts;
14. mixed active/frozen facts;
15. timestamp min/max edge cases;
16. max commit edge cases.

The external property test should assert every L6C counter is nonzero.

## Porting-Log Requirements

The L6C entry must record:

1. current files read;
2. old active/frozen memtable behavior preserved;
3. old behavior intentionally not ported;
4. deferred behavior by owning slice;
5. tests and source guards added;
6. sensitivity probe categories mapped to permanent tests;
7. retirement status of old storage code.

The entry must not claim latest reads, immutable table flushing, inherited
reads, materialization, reachability, compaction install, or snapshot install
are implemented.

## Cross-Feature Matrix

Mandatory L6C commands:

| Mode | Purpose | Command |
|---|---|---|
| branch unit | branch-local state tests | `cargo test -p strata-storage-next --locked --lib branch` |
| source guards | L6 purity | `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` |
| generated state | branch testkit route | `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` |
| no-default generated | no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` |
| wasm/no-default | browser-compatible branch state | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Run `cargo test -p strata-storage-next --locked` before closing L6C.

## Sensitivity Probe Coverage

L6C closes sensitivity coverage through permanent tests:

1. removing branch validation or mutating facts before validation is covered by
   `branch_local_state_rejects_wrong_branch_rows_without_mutation`;
2. allowing active or frozen duplicate internal keys is covered by
   `branch_local_state_rejects_active_and_frozen_duplicates_without_mutation`;
3. rejecting same physical key with a different commit version, rejecting
   different physical keys at the same commit version, dropping tombstones, or
   dropping empty put values is covered by
   `branch_local_state_appends_puts_tombstones_and_preserves_row_facts`;
4. mishandling zero/MAX version or timestamp edges is covered by
   `branch_local_state_tracks_zero_max_version_and_timestamp_edges`;
5. resetting facts during rotation, inserting frozen tables oldest-first, or
   creating an empty frozen table is covered by
   `branch_local_state_rotation_preserves_rows_and_newest_first_order`;
6. dropping active rows on frozen-limit skip is covered by
   `branch_local_state_respects_frozen_limit_without_dropping_active_rows`;
7. generated variants are covered by
   `branch_lsm_property_harness_runs_scaffold_contract`;
8. adding `VersionedValue` or `read_object(...)` is covered by the branch-LSM
   source guard self-tests.

## Exit Gate

L6C test coverage is complete when:

1. direct tests cover state construction, append, duplicate rejection,
   rotation, frozen-limit behavior, and facts;
2. direct tests prove wrong-branch append failure is non-mutating;
3. direct tests prove duplicate failures are non-mutating;
4. direct tests prove rotation preserves rows and facts;
5. generated tests exercise every L6C category with nonzero counters;
6. source guards allow only L6C-owned append/rotation behavior and still
   reject upper-layer/backend/product behavior;
7. no test relies on product DTOs, wall-clock time, backend IO, or lifecycle
   scheduling;
8. no immutable table, inherited read, materialization, compaction, or snapshot
   behavior is implemented early;
9. all mandatory commands pass;
10. porting log records source map, deferrals, verification, and sensitivity
    probe coverage.
