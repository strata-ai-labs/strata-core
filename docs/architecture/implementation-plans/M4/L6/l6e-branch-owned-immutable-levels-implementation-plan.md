# L6E Implementation Plan: Branch-Owned Immutable Levels

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-test-plan.md`

## Objective

Add branch-owned immutable table levels to storage-next L6.

L6E extends the L6D read-view model beyond active and frozen L5 tables. A
branch can install already-built L5 immutable table readers into branch-owned
levels and read those tables together with active/frozen state for latest,
version-bounded, history, prefix, and range reads.

L6E establishes:

1. branch-owned immutable table descriptors and level layout;
2. an in-memory install state transition for branch-owned immutable tables;
3. replacement of a frozen table by an L0 immutable table without changing
   visible reads;
4. L0 overlapping-table ordering and L1+ non-overlap validation;
5. pinned read views that include branch-owned immutable tables;
6. row-chain selection across active, frozen, and branch-owned immutable
   sources;
7. storage-owned table/source facts for diagnostics and later L8 reachability;
8. explicit deferral of durable publication, object-backed reads, inherited
   layers, timestamp/as-of policy, and branch compaction.

L6E should make L6F inherited layers and L6J branch compaction smaller by
centralizing branch-owned immutable table state, source ordering, and install
validation.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
7. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
8. `crates/storage-next/src/table/{builder.rs,reader.rs,cursor.rs,facts.rs,key.rs,mutable.rs}`
9. `crates/storage-next/src/row/mod.rs`
10. `crates/storage/src/segmented/mod.rs`
11. `crates/storage/src/segment.rs`
12. `crates/storage/src/segmented/compaction.rs`
13. `crates/storage/src/manifest.rs`

## Existing-Code Source Map

| Current file | L6E evidence | L6E action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | `BranchState` stores active, frozen, and immutable `SegmentVersion` levels. Point reads search active, frozen, L0 newest-first, then L1+ by range. `BranchSnapshot` pins all of those source sets. | Add branch-owned immutable table levels to `BranchLocalState` and `BranchReadView` without adding inherited layers yet. |
| `crates/storage/src/segment.rs` | Immutable segments expose row iteration, key ranges, and commit ranges. | Use L5 `ImmutableTableReader` and `TableRuntimeFacts` instead of old segment bytes. |
| `crates/storage/src/segmented/compaction.rs` | L0 is overlapping; L1+ is sorted and non-overlapping. Compaction installs replacement tables into levels and preserves concurrent newer L0 data. | Implement only the level layout, ordering, and install validation. Candidate selection and replacement compaction stay in L6J. |
| `crates/storage/src/manifest.rs` | Durable branch manifests record branch-owned immutable tables. | L6E records in-memory branch-owned table facts. Durable manifest publication and recovery stay in L8/L4. |
| `crates/storage-next/src/table/reader.rs` | `ImmutableTableReader` opens bytes or a table source and exposes rows, facts, exact lookup, and cursors. | Treat immutable readers as the L5 source type installed into branch state. Do not import backend or service table APIs. |
| `crates/storage-next/src/table/facts.rs` | `TableIdentity`, `TableRuntimeFacts`, `TableKeyRange`, and `TableCommitRange` are stable L5 table facts. | Validate branch table descriptors against reader facts and level invariants. |
| `crates/storage-next/src/branch/read.rs` | L6D already has `BranchRowSource::OwnedTable { level, table_index }`, `BranchReadView`, scan bounds, and row selection. | Extend candidate collection to include owned immutable tables and use the existing source fact variant. |

## Scope

L6E implements:

1. branch-owned immutable table source type wrapping an L5
   `ImmutableTableReader` plus branch table descriptor facts;
2. branch-owned level layout in `BranchLocalState`;
3. immutable table install into L0 and L1+ levels;
4. frozen-to-L0 replacement install that removes the replaced frozen table only
   after the immutable table is accepted;
5. validation that installed immutable rows all belong to the target branch;
6. validation that table descriptor identity, level, and facts match the L5
   reader;
7. validation that L0 may overlap but L1+ tables are non-overlapping and
   sorted by key range;
8. branch facts that include branch-owned table counts and all source row
   version/timestamp facts;
9. pinned read-view capture that includes immutable table sources;
10. latest, version-bounded, history, prefix, and range reads across active,
    frozen, and branch-owned immutable tables;
11. source attribution as `BranchRowSource::OwnedTable { level, table_index }`;
12. generated tests, direct tests, and source-guard updates;
13. M4-L6 porting-log entries for immutable level behavior.

L6E does not implement:

1. L5 table byte format changes;
2. durable table object publication;
3. object-backed table loading from L4 services;
4. backend IO, object layout, or manifest publication;
5. WAL-before-visible discipline;
6. flush scheduling or selecting which frozen table to flush;
7. compaction candidate selection or replacement of old immutable tables;
8. inherited layers or fork behavior;
9. materialization;
10. timestamp/as-of reads or TTL-at-read-time policy;
11. snapshot row install;
12. branch delete/clear reachability release;
13. product DTO conversion or public API exposure.

## Target Module Shape

Expected production layout after L6E:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs          # may extend descriptor/fact shells
  identity.rs
  read.rs           # extend read view candidate collection
  state.rs          # extend branch state and install transitions
  table.rs          # optional if immutable table state makes state.rs too large
  tests.rs
```

Supporting testkit and guard updates:

```text
crates/storage-next/src/testkit/branch_lsm.rs
crates/storage-next/tests/branch_lsm_properties.rs
crates/storage-next/tests/branch_lsm_source_guard.rs
docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md
```

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### Branch-Owned Immutable Table

Add a branch-owned immutable source equivalent to:

```text
BranchOwnedTable {
    descriptor: BranchTableDescriptor,
    reader: ImmutableTableReader,
}
```

Rules:

1. descriptor identity must match `reader.facts().identity()`;
2. descriptor facts must match `reader.facts()`;
3. descriptor level must match the level where the table is installed;
4. the reader must expose at least one row, enforced by L5 facts;
5. every row in the reader must have `physical_key.branch_id == branch_id`;
6. no row value bytes may appear in validation error messages;
7. the type must not store filesystem paths, object names, backend handles, or
   service objects.

### Branch Immutable Levels

Extend branch-local state with branch-owned levels equivalent to:

```text
owned_levels: Vec<Vec<BranchOwnedTable>>
```

Level rules:

1. level count is bounded by `BranchRuntimeConfig::max_level_count`;
2. L0 is overlapping and ordered newest first;
3. L1+ levels are non-overlapping and sorted by `TableKeyRange.first_key`;
4. L1+ overlap is rejected before state mutation;
5. empty levels are allowed;
6. empty tables are rejected by L5 table facts before install;
7. exact duplicate internal keys with already retained branch state are
   rejected unless the install is a frozen-table replacement that removes the
   duplicate frozen source atomically.

### Install Request

Add an install shape equivalent to:

```text
BranchImmutableInstallRequest {
    level: BranchLevel,
    table: BranchOwnedTable,
    replace_frozen_index: Option<usize>,
}

BranchLocalState::install_owned_table(request)
    -> BranchRuntimeResult<BranchImmutableInstallOutcome>
```

Rules:

1. install validates all inputs before mutating branch state;
2. install does not build table bytes;
3. install does not publish objects or manifests;
4. install does not allocate table identities;
5. install does not allocate commit versions;
6. L0 install inserts at index 0 as newest;
7. L1+ install inserts by key range order;
8. frozen replacement removes only the named frozen table;
9. frozen replacement requires the immutable table rows to match the replaced
   frozen table exactly unless a later slice deliberately introduces lossy
   flush semantics;
10. if validation fails, active, frozen, immutable levels, and branch facts are
    unchanged.

The first implementation may expose narrower helper methods instead of a
single request type, for example:

```text
install_l0_from_frozen(frozen_index, table)
install_owned_table_at_level(level, table)
```

That is acceptable if the same invariants are enforced and the test plan names
the shipped surface.

### Branch Facts

Extend `BranchStateFacts` usage so immutable sources are reflected in captured
facts:

1. `active_rows` remains active mutable row count;
2. `frozen_table_count` remains frozen table count;
3. `owned_table_count` becomes the total branch-owned immutable table count;
4. `inherited_layer_count` remains zero until L6F;
5. `max_commit_version`, `timestamp_min`, and `timestamp_max` include active,
   frozen, and branch-owned immutable rows.

L6E may add a separate immutable-level fact type if needed for diagnostics,
but durable reachability payloads belong to L6I/L8.

### Read View Extension

Extend `BranchReadView` equivalent to:

```text
BranchReadView {
    branch_id
    active
    frozen
    owned_levels
    facts
}
```

Capture rules:

1. capture includes the immutable level layout visible at capture time;
2. later immutable installs do not affect a captured view;
3. later frozen replacement does not remove rows from a captured view;
4. view validation accepts `owned_table_count > 0` once L6E lands;
5. view validation still rejects `inherited_layer_count > 0` until L6F.

Read rules:

1. point reads gather candidates from active, frozen, L0, then L1+;
2. final visible selection remains based on row-chain commit version and read
   bound, not source kind alone;
3. source order is only a deterministic tie-break for impossible or defensive
   equal-version cases;
4. L0 table source facts use newest-first table indices;
5. L1+ table source facts use sorted table indices within each level;
6. scans group by physical key across active, frozen, and owned immutable
   sources and emit one visible row per key;
7. history reads include retained immutable rows and preserve tombstones by
   default.

### L0 And L1+ Lookup Policy

For L6E correctness, a simple full-row scan over installed immutable readers is
acceptable. It must preserve the semantic ordering and row facts. Performance
accelerations may follow later.

Required semantic policy:

1. L0 tables can overlap; all potentially matching L0 tables must be considered.
2. L1+ tables must not overlap; range validation should make at most one table
   per level match a physical key range, but full scan is acceptable for V1.
3. Table key range facts may be used to skip definitely non-matching L1+ tables
   only if tests prove boundary correctness.
4. Bloom/cache use remains L5-owned and optional.

## Implementation Steps

### L6E-A: Source Map And Boundaries

1. Read the current immutable-level paths in `SegmentedStore`, especially
   active/frozen/segment read ordering and `SegmentVersion` layout.
2. Confirm the branch source guard still rejects backend/service/layout imports.
3. Add L6E to the porting log as a planned/started slice when implementation
   begins.

### L6E-B: Branch-Owned Table Shell

1. Add the branch-owned immutable table wrapper.
2. Validate descriptor identity/facts/level against the L5 reader.
3. Validate all reader rows target the owning branch.
4. Preserve `TableRuntimeError` as the source for L5-origin failures.
5. Add direct descriptor and payload-safe error tests.

### L6E-C: Level Layout

1. Add immutable levels to `BranchLocalState`.
2. Initialize empty levels from runtime config.
3. Add level/table count accessors for tests and diagnostics.
4. Add branch fact aggregation over active, frozen, and immutable rows.
5. Keep `inherited_layer_count == 0`.

### L6E-D: Install Transitions

1. Add L0 install.
2. Add frozen-to-L0 replacement install.
3. Add L1+ install with non-overlap validation.
4. Reject level indexes outside runtime config.
5. Reject duplicate exact internal keys except validated frozen replacement.
6. Prove failed installs are non-mutating.

### L6E-E: Read View And Reads

1. Extend read-view capture to include immutable levels.
2. Extend read-view constructor validation for immutable source counts and row
   facts.
3. Extend point-read candidate collection to include owned tables.
4. Extend history and scan candidate collection to include owned tables.
5. Preserve tombstone shadowing and timestamp/TTL deferral exactly as L6D.

### L6E-F: Generated Harness And Guards

1. Add generated branch-LSM counters for immutable install, immutable point
   reads, immutable history, immutable scans, L0 overlap, L1 non-overlap, and
   pinned-view immutable isolation.
2. Extend `branch_lsm_properties.rs` to require nonzero counters.
3. Narrow `branch_lsm_source_guard.rs` to allow only L6E-owned immutable-level
   entrypoints; keep fork/materialization/compaction/snapshot/backend guards.

### L6E-G: Closeout

1. Update `m4-l6-porting-log.md`.
2. Run focused branch tests, source guards, generated tests, no-default wasm
   check, clippy, format check, and full package tests.
3. Record deferred items explicitly.

## Ordering And Source Semantics

Source collection order should be deterministic:

1. active table;
2. frozen tables, newest first;
3. L0 owned immutable tables, newest first;
4. L1+ owned immutable tables by ascending level and sorted key range.

Final row visibility remains commit-version based. This ordering only supplies
source facts and defensive tie-breaks. It must not cause an active row with a
lower commit version to hide a newer immutable row, or an L0 row with a lower
commit version to hide a newer L1 row.

## Failure And Recovery Boundary

L6E is an in-memory branch-state slice. It may report typed facts for failed or
ambiguous install validation, but it does not recover durable state.

Durable sequencing remains:

```text
L5 builds immutable table bytes
L4 publishes table object
L6 installs immutable table into branch state
L4/L8 publishes durable branch/table reachability
L8 reconciles crash or ambiguous publication windows
```

If L6E exposes an install outcome, it should contain only storage-owned facts:
branch id, level, table identity, row count, key range, commit range, replaced
frozen index, and source counts. It must not contain object paths or product
payloads.

## Source Guard Updates

Allow only these new behavior families in production `branch/` code:

1. branch-owned immutable table descriptors;
2. immutable level install;
3. immutable level read-view capture;
4. immutable-table source attribution.

Continue rejecting:

1. backend/service/layout imports;
2. filesystem paths and environment access;
3. product DTO vocabulary;
4. fork/materialization/compaction/snapshot install entrypoints;
5. completed timestamp/as-of APIs before L6G.

## Porting Log Requirements

The L6E entry must record:

1. current source files read;
2. old behavior preserved from `SegmentVersion` and `BranchSnapshot`;
3. new storage-next files changed;
4. installed source ordering;
5. immutable table validation rules;
6. frozen replacement behavior;
7. direct and generated tests added;
8. source-guard changes;
9. deferred L6F/L6G/L6I/L6J/L8 responsibilities.

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

L6E is complete when:

1. branch state can own immutable table levels;
2. L0 install and frozen replacement are non-mutating on validation failure;
3. L1+ overlap is rejected;
4. read views pin immutable levels;
5. latest, version-bounded, history, prefix, and range reads include immutable
   table rows correctly;
6. table and branch facts include immutable rows;
7. source facts distinguish active, frozen, and owned table rows;
8. source guards remain clean;
9. generated tests exercise immutable-level categories with nonzero counters;
10. all mandatory verification commands pass.

## Deferred

1. Object-backed table loading belongs to L8/L4 integration.
2. Durable branch/table reachability belongs to L6I/L8.
3. Inherited layers belong to L6F.
4. Timestamp/as-of and TTL visibility belong to L6G.
5. Materialization belongs to L6H.
6. Branch compaction and immutable replacement policy belong to L6J.
7. Snapshot row install belongs to L6K.
8. Public API mapping belongs above L6.
