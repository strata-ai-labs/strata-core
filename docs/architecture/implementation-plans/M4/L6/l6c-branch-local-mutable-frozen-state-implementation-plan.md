# L6C Implementation Plan: Branch-Local Mutable And Frozen State

Status: implemented in storage-next; sensitivity probe categories are covered
by permanent direct, generated, and source-guard tests

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-test-plan.md`

## Objective

Implement the branch-local in-memory write state for storage-next L6.

L6C gives one branch an active writable L5 `MutableTable`, a newest-first list
of frozen L5 `FrozenTable`s, committed-row append mechanics, active-table
rotation, and branch-local mechanical facts. It does not implement final
latest/getv/history reads, immutable table installation, inherited layers,
materialization, compaction, snapshot install, or lifecycle flushing.

L6C establishes:

1. a concrete branch-local state object over L5 mutable/frozen tables;
2. committed put and tombstone row installation into the active table;
3. exact branch-id validation through L6B helpers before mutation;
4. exact internal-key duplicate rejection across active and frozen branch-local
   tables;
5. active-table rotation into frozen tables, with frozen tables ordered newest
   first;
6. branch-local row-count, version, timestamp, and frozen-table facts;
7. generated and direct tests for active/frozen state mechanics;
8. porting-log evidence for old memtable and branch-state behavior.

L6C should make L6D own-branch reads and L6E immutable-table install smaller by
centralizing all branch-local in-memory mutation and fact accounting.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
7. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
8. `crates/storage-next/src/row/mod.rs`
9. `crates/storage-next/src/table/{mutable.rs,cursor.rs,key.rs}`
10. `crates/storage/src/memtable.rs`
11. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L6C evidence | L6C action |
|---|---|---|
| `crates/storage/src/memtable.rs` | Old active memtable stores entries ordered by internal key, supports freeze, tracks min/max commit, and rejects writes after freeze. | Rebuild on L5 `MutableTable` and `FrozenTable`; do not port skiplist, bloom filter, product `Value`, TTL millis, or wall-clock writes. |
| `crates/storage/src/segmented/mod.rs` | Old `BranchState` owns active memtable, frozen memtables newest first, max version, min/max timestamps, and live/deletion counters. | Port branch-local active/frozen roles and mechanical version/timestamp facts only. Immutable segments, inherited layers, compaction pointers, and lifecycle flushing remain deferred. |
| `crates/storage-next/src/table/mutable.rs` | L5 `MutableTable` accepts `StorageRow`, rejects duplicate internal keys, produces `FrozenTable`, and exposes deterministic facts/cursors. | Use L5 as the only in-memory table substrate. L6C should not add another row map. |
| `crates/storage-next/src/branch/identity.rs` | L6B validates row branch ids and rewrites inherited rows. | Use `require_row_branch` before branch-local appends. L6C should not duplicate branch-id validation logic. |
| `crates/storage-next/src/branch/facts.rs` | L6A already defines `BranchStateFacts` with active rows, frozen count, max commit, and timestamp range. | Compute these facts from L6C state without adding product diagnostics. |

## Scope

L6C implements:

1. a branch-local state type in `crates/storage-next/src/branch/state.rs`;
2. append of already-committed `StorageRow` values into the active
   `MutableTable`;
3. explicit tombstone support through generic `StorageRow` append, not product
   delete APIs;
4. branch id validation before every append;
5. duplicate internal-key rejection before mutation, including duplicates
   already present in frozen tables;
6. active-to-frozen rotation;
7. frozen table ordering, newest first;
8. frozen-table limit behavior using `BranchRuntimeConfig::max_frozen_tables`;
9. branch-local facts derived from active/frozen state;
10. direct tests, generated tests, source-guard updates, and porting-log entry.

L6C does not implement:

1. commit-version allocation;
2. WAL append or WAL-before-visible discipline;
3. product put/delete APIs;
4. `VersionedValue`, product `Value`, `Key`, `Namespace`, or `TypeTag`;
5. latest/getv/as-of/history reads;
6. prefix/range scan behavior;
7. pinned read views;
8. immutable table install;
9. object or backend IO;
10. fork or inherited-layer capture;
11. materialization;
12. reachability/shared table refs;
13. branch compaction;
14. snapshot row install;
15. lifecycle scheduling or background flush.

## Target Module Shape

Expected production layout after L6C:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs
  read.rs
  state.rs        # extend with concrete branch-local mutable/frozen state
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

Names may change if responsibilities stay intact.

### Branch Local State

Add a concrete branch-local state object:

```text
BranchLocalState {
    branch_id: BranchId,
    config: BranchRuntimeConfig,
    active: MutableTable,
    frozen: Vec<FrozenTable>,      # newest first
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    put_rows: u64,
    tombstone_rows: u64,
}
```

Responsibilities:

1. own branch-local active and frozen in-memory rows;
2. expose branch id, active row count, frozen table count, and facts;
3. expose active/frozen table references for later L6D read-view construction;
4. keep frozen tables ordered newest first;
5. keep all state private to L6.

`put_rows` and `tombstone_rows` are optional if they do not land in public
facts yet. If added, they remain mechanical branch-runtime stats, not product
live-key counts.

### Construction

Suggested constructors:

```text
BranchLocalState::new(branch_id, config) -> BranchRuntimeResult<Self>
BranchLocalState::empty(branch_id) -> Self
```

Rules:

1. config is validated before state construction;
2. new state has an empty active table and no frozen tables;
3. empty state facts have no max commit version and no timestamp range;
4. construction never reads clocks, files, backend objects, WAL, or manifests.

### Append Committed Row

Suggested method:

```text
append_committed_row(&mut self, row: StorageRow) -> BranchRuntimeResult<BranchAppendOutcome>
```

Rules:

1. `row.physical_key().branch_id()` must match the state branch id;
2. branch validation uses L6B `require_row_branch`;
3. put and tombstone rows are accepted;
4. all row facts are preserved exactly;
5. exact internal-key duplicates are rejected before mutation;
6. duplicate detection covers the active table and all frozen tables;
7. same physical key at different commit versions is accepted;
8. different physical keys at the same commit version are accepted;
9. timestamp min/max facts are updated from row commit timestamps;
10. max commit version is updated from row commit version;
11. failed append leaves active/frozen state and facts unchanged.

L6C should not decide whether a tombstone hides older rows or whether an
expired row is visible. That policy lands in L6D/L6G.

### Append Outcome

Suggested shape:

```text
BranchAppendOutcome {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    is_tombstone: bool,
    active_rows: usize,
    approximate_active_bytes: usize,
}
```

The outcome is an in-memory mutation fact. It is not durable publication proof
and must not reference WAL objects, manifests, or table object names.

### Rotation

Suggested method:

```text
rotate_active(&mut self) -> BranchRotationOutcome
```

Implementation note: the shipped method returns `BranchRotationOutcome`
directly because empty-active and frozen-limit cases are modeled as explicit
outcomes, not errors.

Suggested outcome:

```text
BranchRotationOutcome::Rotated {
    frozen_index: usize,           # normally 0 because newest first
    frozen_rows: usize,
    frozen_tables: usize,
}
BranchRotationOutcome::Skipped {
    reason: BranchRotationSkipReason,
}

BranchRotationSkipReason::EmptyActive
BranchRotationSkipReason::FrozenLimitReached
```

Rules:

1. empty active rotation is a no-op with an explicit skipped outcome;
2. non-empty active rotation consumes the active `MutableTable`, freezes it,
   inserts the resulting `FrozenTable` at the front of `frozen`, and replaces
   active with a fresh empty `MutableTable`;
3. frozen tables are newest first;
4. rotation is skipped when `frozen.len() >= max_frozen_tables`;
5. if rotation is skipped due to the frozen limit, active rows remain active
   and no rows are lost;
6. rotation does not reset max commit version or timestamp range;
7. rotation does not create immutable table descriptors or object names.

The old storage path could accumulate frozen memtables and flush the oldest
one to disk. L6C only creates frozen in-memory tables. L6E/L6J/L8 own immutable
table install and flush scheduling.

### Facts

Add or reuse:

```text
BranchLocalState::facts(&self) -> BranchRuntimeResult<BranchStateFacts>
```

Rules:

1. `active_rows` is the active table row count;
2. `frozen_table_count` is `frozen.len()`;
3. `owned_table_count` remains zero in L6C;
4. `inherited_layer_count` remains zero in L6C;
5. `max_commit_version` is the max commit seen in active or frozen rows;
6. `timestamp_min` and `timestamp_max` include active and frozen rows;
7. empty state has no version or timestamp facts;
8. facts stay valid after rotation.

### Accessors For Later Slices

Add minimal crate-private accessors:

```text
branch_id()
active()
frozen()
active_row_count()
frozen_table_count()
is_empty()
```

Avoid adding behavior-specific accessors like `read_latest`, `history`, or
`prefix_scan`. L6D owns read-view construction and selection.

## Error Handling

Use existing `BranchRuntimeError` variants where possible:

1. wrong-branch append -> `InvalidBranchRow`;
2. duplicate internal key -> `TableRuntime` wrapping L5 duplicate error, or a
   new typed branch duplicate variant if needed;
3. invalid config -> `InvalidConfig`;
4. impossible facts -> `InvalidBranchState`.

Do not stringify table errors into branch strings. Preserve L5 source chains.

Displays must not include value bytes, product key names, or product branch
names.

## Source Guard Impact

`branch_lsm_source_has_no_premature_behavior_entrypoints` was an L6A scaffold
guard. L6C must narrow it so `append_committed_row` and rotation helpers are
allowed in the owning slice, while still rejecting premature:

1. latest/getv/as-of/history reads;
2. prefix/range scans;
3. fork creation;
4. inherited-layer materialization;
5. immutable table install;
6. branch compaction;
7. snapshot row install;
8. backend/object/lifecycle calls.

The guard should continue scanning all production branch files for upper-layer
imports, product DTO vocabulary, backend operations, filesystem/path/env calls,
and public API leakage.

## Implementation Steps

### L6C-A: Porting Log And Source Audit

1. Add an L6C entry to `m4-l6-porting-log.md`.
2. Record old `Memtable` and `BranchState` behavior being preserved:
   active/frozen roles, frozen newest-first ordering, rotation, min/max commit,
   and min/max timestamp facts.
3. Record intentional V1 changes:
   no product `Value`, no wall-clock writes, no skiplist, no bloom, no
   durable segment flush in L6C.
4. Record deferred behavior to L6D/L6E/L6F/L6J/L6K/L8.

Exit: the porting log clearly says L6C owns in-memory branch-local mutation
only.

### L6C-B: State Type And Construction

1. Extend `state.rs` with the concrete branch-local state type.
2. Store branch id, config, active table, frozen table list, and mechanical
   facts.
3. Add empty/default construction tests.

Exit: empty branch-local state produces valid `BranchStateFacts`.

### L6C-C: Append Committed Rows

1. Implement generic committed row append.
2. Validate row branch with L6B helpers before mutation.
3. Check active and frozen tables for exact internal-key duplicates.
4. Insert into the active `MutableTable`.
5. Update max version, timestamp range, and optional put/tombstone counters.
6. Return append outcome facts.

Exit: branch-local committed put and tombstone rows can be accepted without
product DTOs or commit allocation.

### L6C-D: Rotation

1. Implement empty-active no-op rotation.
2. Implement non-empty active freeze and newest-first frozen insertion.
3. Enforce `max_frozen_tables`.
4. Preserve state on failed/skipped rotation.

Exit: active rows can become frozen rows without data loss or durable object
side effects.

### L6C-E: Facts And Accessors

1. Implement `facts()`.
2. Add minimal accessors needed by tests and L6D.
3. Ensure facts stay correct after appends and rotations.

Exit: later slices can inspect branch-local active/frozen state mechanically.

### L6C-F: Testkit Generated Route

1. Extend `check_branch_lsm_scaffold_contract` or add a state-specific route.
2. Generate branch ids, rows, appends, duplicate attempts, tombstones,
   rotations, and frozen-limit cases.
3. Add nonzero counters for each L6C category.

Exit: property tests exercise state mutation, not just constructors.

### L6C-G: Source Guards And Documentation

1. Narrow the L6A premature-behavior guard for L6C-owned append/rotation.
2. Keep upper-layer, backend, product DTO, and public surface guards intact.
3. Update this plan and the L6C test plan if names change.
4. Record verification commands and sensitivity probe status in the porting
   log.

Exit: L6C is ready for L6D pinned views and own-branch reads.

## Verification Commands

Minimum L6C commands:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run the wasm no-default check because L6C touches feature-gated testkit routes:

```bash
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
```

Run the full package test before closing L6C:

```bash
cargo test -p strata-storage-next --locked
```

## Sensitivity Probe Coverage

L6C keeps these probe categories as permanent regression tests without relying
on a separate probe task:

1. wrong-branch acceptance and pre-validation fact mutation are covered by
   `branch_local_state_rejects_wrong_branch_rows_without_mutation`;
2. active and frozen duplicate internal-key acceptance is covered by
   `branch_local_state_rejects_active_and_frozen_duplicates_without_mutation`;
3. invalid rejection of same-physical-key/different-version rows and
   same-version/different-key rows is covered by
   `branch_local_state_appends_puts_tombstones_and_preserves_row_facts`;
4. tombstone, empty-value, max-version, and timestamp-edge drift is covered by
   `branch_local_state_appends_puts_tombstones_and_preserves_row_facts` and
   `branch_local_state_tracks_zero_max_version_and_timestamp_edges`;
5. rotation fact reset, oldest-first frozen insertion, and empty-rotation
   frozen creation are covered by
   `branch_local_state_rotation_preserves_rows_and_newest_first_order`;
6. frozen-limit active-row loss is covered by
   `branch_local_state_respects_frozen_limit_without_dropping_active_rows`;
7. generated variants of the same categories are covered by
   `branch_lsm_property_harness_runs_scaffold_contract`;
8. product DTO and backend-call drift are covered by
   `branch_lsm_source_guard_catches_required_forbidden_terms` and
   `branch_lsm_source_guard_catches_backend_operation_call_forms`.

## Exit Criteria

L6C is complete when:

1. branch-local state construction is implemented and tested;
2. committed put and tombstone appends are implemented and tested;
3. wrong-branch appends fail before mutation;
4. duplicate internal keys are rejected before mutation across active and
   frozen tables;
5. active rotation creates frozen tables newest first;
6. empty rotation and frozen-limit behavior are explicit and tested;
7. branch-local facts are correct for empty, active-only, frozen-only, and
   mixed active/frozen states;
8. generated testkit coverage exercises all L6C categories;
9. source guards still enforce L6 boundaries;
10. no read/fork/materialization/compaction/snapshot/lifecycle behavior is
    introduced;
11. closeout commands pass;
12. the L6C porting-log entry records preserved, changed, deferred, and
    sensitivity-probe outcomes.
