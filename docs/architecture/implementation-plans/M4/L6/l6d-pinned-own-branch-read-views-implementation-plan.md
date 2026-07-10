# L6D Implementation Plan: Pinned Own-Branch Read Views

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`

## Objective

Implement the first concrete branch read path for storage-next L6.

L6D adds pinned read views over one branch's local active and frozen L5 tables
and implements own-branch point reads, retained history, prefix scans, and
range scans over those in-memory sources. It does not read branch-owned
immutable tables yet, does not read inherited layers, does not perform durable
IO, and does not convert rows into product DTOs.

L6D establishes:

1. a read-view snapshot of `BranchLocalState` that survives later mutations;
2. point read selection over a versioned row chain;
3. latest and version-bounded `getv` own-branch reads;
4. retained per-key history reads, including tombstones;
5. prefix and range scans that return at most one selected row per physical key;
6. tombstone shadowing for visible-value reads;
7. explicit deferral of timestamp/as-of and TTL-at-read-time policy to L6G;
8. generated and direct tests for own-branch read model parity;
9. porting-log evidence for the old `BranchSnapshot`, MVCC iterator, and
   active/frozen read behavior.

L6D should make L6E immutable table install and L6F inherited reads smaller by
centralizing own-branch read-view mechanics and row-chain selection.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
8. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
9. `crates/storage-next/src/table/{cursor.rs,key.rs,mutable.rs}`
10. `crates/storage-next/src/row/mod.rs`
11. `crates/storage/src/memtable.rs`
12. `crates/storage/src/merge_iter.rs`
13. `crates/storage/src/seekable.rs`
14. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L6D evidence | L6D action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | `BranchSnapshot` pins active/frozen/segment/inherited references so reads do not hold branch-map guards and do not see partial state. | Rebuild the pinned view over storage-next `BranchLocalState` active/frozen tables only. Immutable and inherited references land later. |
| `crates/storage/src/memtable.rs` | `get_versioned_preencoded`, `get_all_versions`, `iter_prefix`, and `iter_range` read one ordered row chain or ordered prefix from active/frozen memtables. | Use L5 `MutableTable`, `FrozenTable`, `MemoryTableCursor`, and key wrappers rather than old skiplist APIs. |
| `crates/storage/src/merge_iter.rs` | `MergeIterator` merges sources in key order; `MvccIterator` groups by logical key and selects the newest row with `commit_id <= max_version`. | Port the MVCC grouping rule over L5 cursors for active/frozen sources. Do not port inherited rewriting here. |
| `crates/storage/src/seekable.rs` | Seekable merge iterators support repeated seek/next cycles over a pinned snapshot. | L6D can start with simpler owned/cloned read views over active/frozen rows. Seekable reusable iterators are optional unless needed for clean scans. |
| `crates/storage-next/src/table/cursor.rs` | L5 already has memory cursors, bounded cursors, and merge cursors with deterministic source tie-breaks. | Reuse these cursors for scan paths where practical. L6 owns MVCC grouping and tombstone decisions. |
| `crates/storage-next/src/branch/read.rs` | L6B already defines read bounds, row-source facts, candidate facts, visible rows, and history rows. | Extend this vocabulary rather than introducing product `VersionedValue`. |
| `crates/storage-next/src/branch/state.rs` | L6C exposes active/frozen table references and branch facts. | Add read-view capture from this state without changing append/rotation semantics. |

## Scope

L6D implements:

1. a pinned own-branch read view, likely in `crates/storage-next/src/branch/read.rs`
   or a new `crates/storage-next/src/branch/view.rs`;
2. read-view capture from `BranchLocalState`;
3. validation that point-read and scan bounds target the view branch id;
4. latest own-branch point reads over active and frozen tables;
5. version-bounded own-branch point reads over active and frozen tables;
6. retained history reads for one physical key, newest first;
7. prefix scans over active and frozen tables;
8. range scans over active and frozen tables;
9. tombstone handling for visible-value reads and scans;
10. storage-owned result facts that preserve row source, commit version,
    commit timestamp, expiry, tombstone flag, and value bytes;
11. generated tests, direct tests, and source-guard updates;
12. M4-L6 porting-log entries for pinned read views and own-branch reads.

L6D does not implement:

1. commit-version allocation;
2. WAL append or WAL-before-visible discipline;
3. product put/delete/get APIs;
4. `VersionedValue`, product `Value`, `Key`, `Namespace`, or `TypeTag`;
5. timestamp/as-of reads as a completed behavior;
6. TTL-at-read-time filtering or wall-clock expiry checks;
7. immutable table install or reads from branch-owned table objects;
8. object-backed table reads;
9. inherited layer reads or child-local/inherited shadowing;
10. source-to-child branch rewriting in read iterators;
11. fork creation;
12. materialization;
13. reachability/shared table refs;
14. branch compaction;
15. snapshot row install;
16. lifecycle scheduling or backend IO.

## Target Module Shape

Expected production layout after L6D:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs
  read.rs          # extend with read-view/query/result mechanics
  state.rs         # add read-view capture helper if this fits better locally
  view.rs          # optional if read.rs becomes too large
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

### Branch Read View

Add a pinned read-view type equivalent to:

```text
BranchReadView {
    branch_id: BranchId,
    active: MutableTable,
    frozen: Vec<FrozenTable>,      # newest first
    facts: BranchStateFacts,
}
```

The shipped implementation may store `Arc` references instead of cloned tables
if L6C state publication is refactored first. For L6D, either representation
is acceptable if the invariant is true:

> A read view sees the active/frozen state captured at view creation, even if
> the source `BranchLocalState` later appends rows or rotates active tables.

Responsibilities:

1. expose the captured branch id;
2. expose captured state facts;
3. expose active/frozen source counts for diagnostics/tests;
4. own or pin active/frozen table snapshots;
5. never expose mutable table handles;
6. never import L4, backend, object layout, lifecycle, or engine APIs.

### View Capture

Add a capture method equivalent to one of:

```text
BranchLocalState::capture_read_view(&self) -> BranchRuntimeResult<BranchReadView>
BranchReadView::capture(state: &BranchLocalState) -> BranchRuntimeResult<Self>
```

Rules:

1. capture is in-memory only;
2. capture does not validate or allocate commit versions;
3. capture does not read clocks, files, backend objects, WAL, or manifests;
4. capture preserves frozen-table newest-first order;
5. capture stores the facts visible at capture time;
6. later state mutation cannot affect a captured view.

The old `BranchSnapshot` cloned `Arc` references. L6D may clone L5 in-memory
tables because active/frozen tables are local data structures and this slice is
not optimizing for large pinned-view retention yet. L6E/L6J can move to
reference-counted table views if needed for immutable integration.

### Point Read Request

Add an own-branch point-read request shape or methods equivalent to:

```text
BranchReadView::latest(&self, key: &PhysicalKey)
    -> BranchRuntimeResult<Option<BranchVisibleRow>>

BranchReadView::getv(&self, key: &PhysicalKey, version: CommitVersion)
    -> BranchRuntimeResult<Option<BranchVisibleRow>>

BranchReadView::read_point(&self, key: &PhysicalKey, bound: BranchReadBound)
    -> BranchRuntimeResult<Option<BranchVisibleRow>>
```

Rules:

1. key branch id must match the view branch id;
2. latest uses no version cap;
3. `getv(v)` uses `commit_version <= v`;
4. `BranchReadBound::AtTimestamp` is rejected with a typed "deferred" or
   "unsupported in this slice" error until L6G wires timestamp/as-of reads;
5. the selected row is the newest row in the physical-key chain that satisfies
   the effective version bound;
6. if the selected row is a tombstone, visible read returns `None`;
7. if the selected row is a put, visible read returns `BranchVisibleRow`;
8. visible read does not fall through a selected tombstone to an older put;
9. visible read preserves expiry facts but does not apply TTL policy in L6D;
10. result source is `Active` or `Frozen { index }`.

### History Request

Add a retained-history method equivalent to:

```text
BranchHistoryOptions {
    before_version: Option<CommitVersion>,
    limit: Option<usize>,
    include_tombstones: bool,      # default true for storage history
}

BranchReadView::history(&self, key: &PhysicalKey, options: BranchHistoryOptions)
    -> BranchRuntimeResult<Vec<BranchHistoryRow>>
```

Rules:

1. key branch id must match the view branch id;
2. history returns retained rows newest first by commit version;
3. `before_version` is exclusive, matching old storage behavior;
4. `limit = Some(0)` returns an empty vector;
5. tombstones are included by default for storage history;
6. optional tombstone exclusion may be added only if tests prove the default
   storage-history path still preserves tombstones;
7. expiry facts are preserved and not filtered in L6D;
8. exact duplicate internal keys should be impossible after L6C, but history
   must still behave deterministically if a future source is added.

### Scan Bounds

Add a storage-owned scan-bound shape if the existing L5 key bounds are not
ergonomic enough:

```text
BranchScanBounds {
    branch_id: BranchId,
    space: String,
    storage_space_id: StorageSpaceId,
    user_key_prefix: Option<Vec<u8>>,
    lower_user_key: Option<BranchRangeBound<Vec<u8>>>,
    upper_user_key: Option<BranchRangeBound<Vec<u8>>>,
}
```

The exact type can differ. The important rules are:

1. branch id is explicit and must match the view;
2. space and storage-space id remain opaque storage routing facts;
3. user-key bytes are opaque bytes;
4. prefix scans and range scans do not use product `Key`, `Namespace`, or
   `TypeTag`;
5. bounds can be converted to L5 `TableKeyBounds` or cursor seeks without
   ad hoc encoded-byte parsing.

### Prefix And Range Reads

Add scan methods equivalent to:

```text
BranchReadView::scan_prefix(&self, prefix: &BranchScanBounds, bound: BranchReadBound)
    -> BranchRuntimeResult<Vec<BranchVisibleRow>>

BranchReadView::scan_range(&self, bounds: &BranchScanBounds, bound: BranchReadBound)
    -> BranchRuntimeResult<Vec<BranchVisibleRow>>
```

Rules:

1. L6D supports latest and version-bounded scans;
2. timestamp-bounded scans are deferred to L6G;
3. scans merge active plus frozen sources in internal-key order;
4. scans group rows by physical key, ignoring commit-version suffix;
5. each physical key contributes at most one visible row;
6. a selected tombstone suppresses that key from visible scan output;
7. scans preserve storage order by physical key;
8. scans respect empty prefixes, embedded zero bytes, high-bit user keys, and
   shared prefixes;
9. scans do not cross space or storage-space-id boundaries accidentally;
10. scans report active/frozen source facts on returned rows.

### MVCC Selection Helper

Add a pure helper for one row chain:

```text
select_visible_row(candidates, effective_bound) -> Option<SelectedRow>
collect_history_rows(candidates, options) -> Vec<BranchHistoryRow>
```

Rules:

1. candidates must all share the same physical key;
2. candidates are ordered newest first by internal-key order or sorted before
   selection;
3. latest selects the first candidate;
4. version-bounded selection skips rows above the version cap;
5. tombstone rows are selected as shadowing facts, then mapped to `None` for
   visible-value point and scan results;
6. the helper must not know product value semantics;
7. the helper must not apply wall-clock expiry checks.

This helper is the storage-next replacement for the old `MvccIterator` behavior
within active/frozen own-branch sources.

## Read Order

For L6D, own-branch source order is:

```text
active mutable table
  -> frozen tables, newest first
```

This order is source tie-break order, not a substitute for version selection.
Rows with the same physical key but different commit versions must be selected
by commit version and read bound. Exact duplicate internal keys should already
be rejected by L6C across active/frozen state.

L6E extends the source list with branch-owned immutable levels:

```text
active -> frozen -> L0 -> L1+
```

L6F extends it with inherited layers:

```text
active -> frozen -> L0 -> L1+ -> inherited layers
```

L6D must not add placeholder immutable or inherited behavior that silently
returns incomplete reads once those sources exist. If a method documents
own-branch active/frozen only, tests should pin that scope.

## Timestamp And TTL Deferral

L6D uses only version visibility for final read selection.

`StorageRow` already carries `commit_timestamp` and `expires_at` facts. L6D
must preserve and return those facts, but it must not:

1. call `Timestamp::now`;
2. call wall-clock APIs;
3. decide whether a row is expired at "now";
4. expose completed `as_of` semantics;
5. fall back from an expired-looking row to an older row.

L6G owns timestamp-bounded reads and TTL visibility at an explicit read
timestamp. Until L6G lands, `BranchReadBound::AtTimestamp` should be rejected
or kept out of public L6D methods, with tests proving the deferral is explicit.

## Error Handling

Add or reuse typed errors for:

1. point-read key branch mismatch;
2. scan-bound branch mismatch;
3. unsupported timestamp read bound in L6D;
4. invalid scan bound shape if a new scan-bound type is introduced;
5. L5 cursor/table runtime failures, preserving the source chain.

Error display must not include value bytes or product payload text.

## Source Guard Update

`branch_lsm_source_guard.rs` must be narrowed for L6D:

1. allow L6D-owned read-view and own-branch read method names;
2. continue rejecting fork/materialization/compaction/snapshot install;
3. continue rejecting backend/object/layout/lifecycle/engine imports;
4. continue rejecting product DTO vocabulary;
5. continue rejecting filesystem, path, env, mmap, and direct backend
   operation vocabulary.

The guard should scan all production `src/branch/*.rs` files, excluding only
test modules.

## Testkit Update

Extend `crates/storage-next/src/testkit/branch_lsm.rs` with generated
active/frozen read scripts and nonzero counters for:

1. read-view capture cases;
2. pinned-view append isolation cases;
3. pinned-view rotation isolation cases;
4. latest point-read cases;
5. version-bounded point-read cases;
6. selected tombstone shadow cases;
7. history cases with tombstones;
8. history limit and before-version cases;
9. prefix scan cases;
10. range scan cases;
11. scan tombstone suppression cases;
12. active/frozen merge cases;
13. branch-mismatch read rejection cases;
14. timestamp-bound deferral cases.

Generated expected results must be independent of production L6D selection
helpers. A compact generated fixture with independently computed expected
vectors is acceptable for this slice; a reusable randomized model can be added
when immutable and inherited sources join the read path.

## Porting-Log Requirements

The L6D entry in `m4-l6-porting-log.md` must record:

1. current files read;
2. old `BranchSnapshot` and MVCC iterator behavior preserved;
3. old behavior intentionally not ported;
4. deferred immutable, inherited, timestamp, TTL, materialization, and
   lifecycle behavior;
5. direct tests added;
6. generated tests added;
7. source guards updated;
8. sensitivity probes mapped to permanent tests;
9. retirement status of old storage read behavior.

The entry must not claim object-backed immutable reads, inherited reads,
timestamp/as-of reads, TTL policy, materialization, branch compaction, or
snapshot install are complete.

## Verification Commands

Mandatory L6D commands:

```text
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
cargo test -p strata-storage-next --locked
```

## Exit Gate

L6D is complete when:

1. a pinned own-branch read view exists;
2. read views are isolated from later append and rotation mutations;
3. latest own-branch point reads match the independent model;
4. version-bounded own-branch point reads match the independent model;
5. retained history reads return rows newest first and preserve tombstones;
6. prefix scans return one selected live row per physical key in storage order;
7. range scans respect inclusive/exclusive or documented bound semantics;
8. selected tombstones suppress visible-value point and scan results;
9. expiry/timestamp facts are preserved but not interpreted as wall-clock TTL;
10. timestamp/as-of reads are explicitly deferred to L6G;
11. no product DTOs or backend/lifecycle APIs enter production branch code;
12. direct, generated, source-guard, wasm/no-default, clippy, and full package
    test gates pass;
13. the porting log records the behavior map and deferrals.
