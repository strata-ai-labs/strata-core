# M4-L6 Implementation Plan: Branch LSM Runtime

Status: draft implementation plan

## Objective

Build the branch-isolated LSM runtime for storage-next.

M4-L6 assembles L5 table mechanics into storage-owned branch state:

1. branch-local mutable and frozen rows;
2. branch-owned immutable table levels;
3. inherited copy-on-write table layers;
4. versioned row-chain visibility;
5. latest, version-bounded, timestamp-bounded, history, prefix, and range
   reads;
6. branch/table reachability facts consumed by L8.

M4-L6 must preserve the core current-storage design: one ordered version chain
per logical row key, encoded by physical key plus descending commit version.
It must not resurrect `VersionedValue` as a lower-layer storage type. L6 should
produce storage-owned row/read facts; L9 or engine adapters can map those facts
to product read DTOs.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/commit-timeline-substrate.md`
5. `docs/architecture/storage/implementation-patterns.md`
6. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
7. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
8. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
9. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
10. `docs/spec/strata-storage-format-v1.md`
11. `crates/storage-next/src/row/`
12. `crates/storage-next/src/table/`
13. `crates/storage-next/src/service/table.rs`
14. `crates/storage-next/src/service/manifest.rs`

## Existing-Code Source Map

The current implementation evidence lives mainly in `crates/storage`.
`SegmentedStore` is the strongest evidence, but it mixes L5 table mechanics,
L6 branch/MVCC semantics, L7 commit behavior, and L8 lifecycle work. Port the
branch mechanics; do not port the mixed ownership.

| Current file | Relevant L6 evidence | Porting rule |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | `BranchState`, active/frozen memtables, `BranchSnapshot`, immutable levels, inherited layers, fork, materialization, branch reads, version/timestamp scans. | Primary behavioral source. Split table, commit, and lifecycle responsibilities away from branch state. |
| `crates/storage/src/key_encoding.rs` | Branch-aware physical key and internal key ordering with descending commit version. | Preserve ordering facts using storage-next `PhysicalKey`, `InternalKey`, and `StorageRow`. |
| `crates/storage/src/memtable.rs` | Active/frozen branch-local mutable state and versioned rows. | Use L5 `MutableTable` and `FrozenTable`; do not port product `Value` or old memtable entry shape. |
| `crates/storage/src/merge_iter.rs` | MVCC iterator and old latest-selection mechanics. | Port MVCC grouping semantics into L6 over L5 cursors; keep raw merge in L5. |
| `crates/storage/src/seekable.rs` | Seekable inherited-layer rewriting and fork-version gating. | Port branch rewrite/fork-gate semantics into L6; keep raw cursor movement in L5. |
| `crates/storage/src/segment.rs` | Immutable table read evidence and segment entry facts. | Use L5 immutable readers and table facts, not old segment bytes. |
| `crates/storage/src/segmented/ref_registry.rs` | Shared segment/table reference tracking. | Rebuild as L6 runtime acceleration over durable reachability facts. |
| `crates/storage/src/manifest.rs` | Branch segment manifests and inherited layer persistence. | Use as evidence for branch/table reachability payloads; durable publication stays L4. |
| `crates/storage/src/segmented/compaction.rs` | Branch-level candidate selection, tombstone safety, install/removal mechanics. | Keep branch-specific candidate/install/safety facts in L6; scheduling stays L8 and table compaction stays L5. |
| `crates/storage/src/durability/decoded_snapshot_install.rs` | Snapshot row install into branch storage state. | Port generic row install preflight and branch-state mutation; snapshot orchestration stays L8. |
| `crates/storage/src/stored_value.rs` | Old conversion from internal entries to `VersionedValue`. | Do not port as L6 API. Storage-next rows already carry value bytes, commit version, timestamp, expiry, and tombstone facts. |

Storage-next already provides:

1. branch-aware physical keys in `crates/storage-next/src/row/`;
2. durable `StorageRow` bytes with commit version, commit timestamp, expiry,
   tombstone flag, and value bytes;
3. L5 mutable/frozen tables, cursors, readers, cache, and generic compaction;
4. L4 table object publication and object-backed table readers.

## L6 Boundaries

L6 owns:

1. branch state creation and lookup;
2. branch-local active mutable table ownership;
3. branch-local frozen table ownership;
4. branch-owned immutable table level ownership;
5. branch read views that pin a consistent state;
6. branch-local append of already-committed rows;
7. latest visible row selection;
8. version-bounded row selection;
9. timestamp-bounded row selection over storage commit timestamps;
10. per-key retained history reads;
11. prefix/range scans with version/timestamp visibility;
12. inherited layer references, fork-version gates, and key rewriting;
13. child-local shadowing of inherited rows;
14. branch-safe tombstone and TTL visibility facts;
15. branch/table reachability facts and shared table reference facts;
16. branch-local immutable table install/removal state transitions;
17. materialization state transitions;
18. snapshot row install preflight and branch-state install;
19. raw branch/table metrics consumed by L8.

L6 must not own:

1. product branch names, DAG UX, merge, cherry-pick, revert, restore, or review
   workflows;
2. engine capability semantics, JSON paths, graph/vector/search/event meaning,
   or query planning;
3. commit validation, commit-version allocation, or commit ordering;
4. WAL-before-visible discipline;
5. durable publish mechanics or backend IO;
6. table byte format or raw table algorithms;
7. compaction scheduling, checkpointing, recovery orchestration, retention
   scheduling, or quarantine policy;
8. public storage API mapping;
9. `VersionedValue` or other product read DTOs.

## Storage Model

L6 keeps the same fundamental model as current storage:

```text
physical key + descending commit version -> row facts
```

For one logical key, retained versions are adjacent in encoded-key order:

```text
branch | space | storage_space | user_key | version 42 -> newest retained row
branch | space | storage_space | user_key | version 31 -> previous row
branch | space | storage_space | user_key | version 12 -> older row or tombstone
```

L5 treats these as ordered table rows. L6 decides which row is visible for a
read request. Cleanup and compaction may remove older rows only when L6/L8 can
prove they are not observable by any retained version, timestamp, branch, fork,
snapshot, or inherited layer.

## Read Result Policy

L6 should introduce storage-owned result types, not expose old product DTOs.

Suggested responsibilities:

1. `BranchVisibleRow` or equivalent for one selected row and its source facts.
2. `BranchHistoryRow` or equivalent for retained row history, including
   tombstone status when the caller requested history.
3. `BranchReadBound` for latest, version-bounded, or timestamp-bounded reads.
4. `BranchReadFacts` for diagnostics such as source branch, inherited layer,
   effective fork version, and table/source kind.

The exact names may change, but the lower layer must preserve:

1. physical key;
2. commit version;
3. commit timestamp;
4. expiry timestamp;
5. tombstone flag;
6. value bytes for live put rows;
7. branch/source facts needed by L8 and diagnostics.

L9 may map these storage facts into `Versioned<T>` or product-specific DTOs.

## Branch State Shape

Target shape:

```text
BranchLsm
  branches: BranchId -> BranchState

BranchState
  branch id
  active MutableTable
  frozen FrozenTable list, newest first
  own immutable levels
    L0 overlapping, newest first
    L1+ non-overlapping, sorted by key range
  inherited layers, nearest ancestor first
  max applied commit version
  timestamp range facts
  table reachability facts
  approximate row/table metrics
```

Inherited layer shape:

```text
InheritedLayer
  source branch id
  fork version
  source immutable levels snapshot
  materialization status
  reachability/shared table facts
```

Production state should use persistent/immutable views or clone-on-write
publication so pinned read views can survive branch mutations.

## Implementation Slices

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L6A` | Source map and module scaffold | Create `branch` module structure, branch error/config/fact/read-bound types, result shells, and porting log entries. Add source guards for branch layer purity. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-test-plan.md`. | Compile-only tests, error display/source-chain tests, guard tests. | Branch module compiles with no behavior and no imports above L6. |
| `L6B` | Branch row identity and read bounds | Add helpers for branch id validation, branch-local physical-key checks, branch-id rewriting, effective read bounds, timestamp/version bound comparison, and row visibility facts. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-test-plan.md`. | Row-chain model tests, branch mismatch rejection, rewrite round trips, timestamp/version bound tests. | L6 can reason about row keys and read bounds without tables or branch state. |
| `L6C` | Branch-local mutable/frozen state | Implement `BranchState` over L5 `MutableTable`/`FrozenTable`, committed-row append, tombstone append, active rotation, frozen ordering, and branch-local max version/timestamp metrics. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-test-plan.md`. | Branch creation, committed put/delete install, duplicate rejection, rotation/freeze facts. | L6 can own active/frozen branch-local rows without immutable tables or inheritance. |
| `L6D` | Pinned read views and own-branch reads | Add pinned branch read views over active/frozen state and implement own-branch latest, getv, history, prefix, and range reads. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`. | Latest/getv/history consistency, tombstone shadowing, pinned-view mutation isolation, explicit timestamp/TTL deferral. | Own-branch reads match an independent row-chain model. |
| `L6E` | Branch-owned immutable levels | Add branch table descriptors, level descriptors, owned table install, L0/L1+ read ordering, table reader handoff, and branch-level table facts. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-test-plan.md`. | Read parity across mutable/frozen/immutable sources, L0 overlap ordering, L1 range selection, install validation. | L6 can read branch-owned immutable tables through L5 without object/backend imports. |
| `L6F` | Fork and inherited layers | Implement storage-level fork metadata, inherited layer capture, fork-version gates, source-to-child key rewriting, inherited read order, and child-local shadowing. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-test-plan.md`. | Fork without row copy, parent writes after fork invisible, child writes/tombstones shadow inherited rows, chained ancestry order. | Branch inheritance reads are correct without materialization. |
| `L6G` | Timestamp reads and TTL semantics | Complete timestamp-bounded selection, timestamp range facts, TTL visibility at read timestamp, and history-unavailable facts if retained timestamp coverage is insufficient. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-test-plan.md`. | `as_of` reads, timestamp scans, TTL before/after expiry, inherited timestamp plus fork-version gates. | L6 can answer timestamp-bounded reads consistently over own and inherited rows. |
| `L6H` | Materialization mechanics | Convert retained inherited rows into child-owned L5 table artifacts, rewrite keys, install replacement tables, remove inherited layer atomically from reader perspective, and expose recovery facts. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-test-plan.md`. | Materialization preserves all visible reads, is idempotent over staged facts, and never removes inherited layer before replacement is visible. | Materialization changes physical ownership without changing read results. |
| `L6I` | Reachability and shared table refs | Add branch/table reachability fact model, runtime shared-table registry, inherited references, release facts, and rebuild-from-manifest model hooks. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-test-plan.md`. | Shared table not released while inherited, branch delete/clear release facts, registry rebuild model tests. | L6 can tell L8 which tables are reachable and which may be released. |
| `L6J` | Branch compaction integration | Select branch-owned compaction candidates, supply L5 caller policies for tombstone/TTL/version safety, install output tables into levels, and preserve pinned views. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-test-plan.md`. | Candidate selection, unsafe tombstone rejection, output install, old table release facts, read parity before/after compaction. | L6 can perform branch-level compaction state transitions without scheduling or backend IO. |
| `L6K` | Snapshot row install | Preflight generic decoded storage rows, validate target branches and row ordering, build/install branch-local tables all-or-nothing, and report install facts. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-test-plan.md`. | All-or-nothing invalid row/branch/duplicate tests, multi-branch install, read parity after install. | L8 can hand generic snapshot rows to L6 without primitive DTOs. |
| `L6L` | L6 conformance closeout | Consolidate generated tests, source guards, fuzz targets, old-code behavior map, and deferred ledger for L7/L8/L9. Detailed plans: `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md`. | Full L6 conformance matrix, closeout inventory, fuzz inventory, and sensitivity probes. | M4-L6 closes and L7 can commit into branch state. |
| `L6M` | Assurance depth | Strengthen L6 closeout with an inheritance-aware independent model, opcode-driven generated and fuzz contracts, richer fuzz corpora, and a concrete sensitivity-probe ledger. Detailed plan: `docs/architecture/implementation-plans/M4/L6/l6m-assurance-depth-implementation-plan.md`. | Model-backed fork/materialization/scan parity, distinct fuzz contract routes, sensitivity probe mutation-to-failure evidence. | M4-L6 closes with reference-grade assurance, not only runtime completeness. |

## Read Semantics

### Latest

Latest read over one branch returns the newest live row in the effective branch
row chain. It returns `None` when the newest visible row is a tombstone or an
expired row at the read timestamp.

### Version-Bounded

`getv` or equivalent reads the newest row with:

```text
row.commit_version <= requested_version
```

For inherited layers:

```text
effective_version = min(requested_version, layer.fork_version)
```

### Timestamp-Bounded

`as_of` reads the newest row whose commit timestamp is at or before the
requested timestamp, after applying tombstone and TTL visibility. L6 may use
the commit timeline substrate to resolve timestamp to version frontiers, but
row timestamps remain authoritative facts for validation and tests.

### History

History returns retained row versions newest first for one physical key. It
should be able to include tombstones when the caller asks for storage history.
Product-facing history filtering belongs above L6.

### Scans

Prefix/range scans merge candidate sources and group by logical physical key.
Inherited keys must be rewritten into the child branch namespace before MVCC
grouping so child-local and inherited rows shadow each other correctly.

## Fork Semantics

Storage-level fork:

1. captures source immutable table reachability;
2. captures source max applied commit version as fork version in the initial
   L6 helper;
3. creates destination branch with empty own state and inherited layers;
4. preserves source inherited layers in deterministic ancestry order;
5. protects shared tables before they can be observed as unreferenced;
6. exposes raw publication/reachability facts for L8.

Retained historical fork-version requests are deferred until a caller-owned
retained-history proof API exists.

L6 does not own product branch workflow semantics.

## Materialization Semantics

Materialization rewrites retained inherited rows into child-owned rows and
installs child-owned tables. It must preserve visible results before and after
the state transition.

Materialization is not cleanup. Shared table references are released only after
replacement reachability is safe or L8 can recover the operation.

## Compaction Policy Boundary

L6 may call L5 compaction and supply branch-aware safety policy. It must not
schedule compaction; that is L8.

L6 policy facts include:

1. branch-local snapshot/version floors;
2. inherited layer fork versions;
3. shared table reachability;
4. child-local shadowing facts;
5. timestamp/as-of retention bounds;
6. TTL and tombstone safety facts.

L5 executes the policy mechanically. L6 decides what is safe. L8 decides when
to run.

## Snapshot Install Boundary

L6 accepts generic storage rows, not engine snapshot DTOs.

Snapshot row install must:

1. validate all target branches before mutation;
2. validate branch id in every row key;
3. reject duplicate internal keys in the install plan;
4. validate ordering and table build plans;
5. stage/build table artifacts locally and install only after preflight
   succeeds;
6. leave no partial branch-state mutation visible on failure.

Durable table-object publication, branch manifest publication, and
crash-window reconciliation remain L8/L4 responsibilities. L6K exposes
branch-state staging, install, and reachability facts only.

## Source Guard Policy

Production `crates/storage-next/src/branch/` may import:

1. `crate::row`;
2. `crate::table`;
3. L4 service fact/result types needed at the L6 boundary, if they do not pull
   durable publish mechanics into branch code;
4. `strata_core_next` storage atoms such as `BranchId`, `CommitVersion`, and
   `Timestamp`;
5. standard library collections/sync primitives.

Production `branch/` must not import:

1. `crate::commit`;
2. `crate::lifecycle`;
3. `crate::api`;
4. engine crates;
5. current `strata_core::VersionedValue`, `Value`, `Key`, `Namespace`, or
   `TypeTag`;
6. filesystem/path/backend APIs;
7. WAL append or checkpoint orchestration code;
8. product branch workflow vocabulary as behavior.

## M4-L6 Exit Gate

M4-L6 is complete when:

1. `crates/storage-next/src/branch/` contains branch state, read views,
   branch-local reads, inherited layers, materialization mechanics,
   reachability facts, compaction install mechanics, and snapshot row install.
2. Latest, getv, history, prefix/range, and timestamp reads match generated
   row-chain models.
3. Fork/inheritance tests prove child-local writes and tombstones shadow
   inherited rows and parent writes after fork are invisible.
4. Materialization and branch compaction preserve visible read results.
5. Reachability/ref tests prove shared tables are not released while inherited.
6. Snapshot row install is all-or-nothing.
7. Source guards prove L6 does not import commit, lifecycle, engine, product
   DTOs, filesystem, or backend APIs.
8. The L6 porting log records old branch/LSM behavior as preserved, rewritten,
   retired, or deferred.
9. L7 can commit already-versioned rows into L6 without adding branch runtime
   mechanics.
