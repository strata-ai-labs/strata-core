# M4P-L6 Implementation Plan: Branch-Isolated LSM Runtime Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-test-plan.md`

## Objective

Restore old-storage branch-isolated LSM serving mechanics inside storage-next
L6 without changing the L1-L9 architecture.

Storage-next already has the right L6 boundary: branch-local mutable state,
frozen tables, owned L0/L1+ levels, inherited layers, fork-version gates,
materialization, compaction planning, snapshot install, reachability, and
branch tests. The gap is not a missing architecture layer. The gap is that the
standard L6 runtime does not yet preserve old-storage asymptotic source
selection:

1. point reads over nonzero levels probe every table instead of at most one
   table per non-overlapping level;
2. scans create one cursor per table instead of one lazy cursor per nonzero
   level;
3. history, timestamp resolution, and hot branch facts can scan unrelated rows;
4. read views clone more table data than the old pinned superversion model;
5. branch compaction and materialization still prepare row vectors where old
   storage used iterator/table-source mechanics.

M4P-L6 restores those mechanics through the normal branch read, scan, history,
compaction, and materialization pathways. It must not add L9 benchmark fast
paths, cache-only special cases, or table-format changes.

## Audit Finding References

Primary audit and perf evidence:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
   - `L6. Branch-Isolated LSM Runtime`
   - `9. Differential Tests And Perf Counters`
   - `10. Final Parity Matrix And Architecture-Aligned Gap Plan`
   - `Immediate Next Step`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
   - `Old Invariants To Restore`
   - `Proof Slices`
   - `PERF-T3: Read Snapshot Pinning`
   - `PERF-T4: Point Read Seek Over Existing Internal Keys`
   - `PERF-T5: Lazy Range Scan With Limit Pushdown`
   - `Hot-path-specific gates`
3. `docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`
   - `First Executable Slices`
   - `Audit Coverage Backlog`
   - `M4P-L6: Branch-Isolated LSM Runtime`
4. `docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`
   - `Layer Test Matrix`
   - `Performance Testing Methodology`
   - `Fail-fast performance invariants`
   - `Source Guards`

Findings covered by this plan:

1. L1+ point-read source pruning is incomplete.
2. Scan source planning creates one cursor per table instead of one lazy cursor
   per non-overlapping level.
3. Read-view capture is correct but not old pinned snapshot parity.
4. Timestamp-to-version resolution scans branch rows.
5. Branch facts recomputation walks retained rows.
6. Branch compaction source preparation is eager and row-vector based.
7. Materialization is eager and fixed-chunk L0 output.
8. Fork ergonomics are stricter than old storage-level behavior.
9. L6 tests do not yet assert old asymptotic source counts.

## Old-Code Source Map

Use old storage as behavioral and mechanical evidence, not as a direct code
port.

| Old source | Behavior to preserve | Storage-next target |
| --- | --- | --- |
| `crates/storage/src/segmented/mod.rs` | Branch state, active/frozen tables, branch snapshots, point reads, range scans, inherited COW layers, fork gates, and materialization entry points. | `crates/storage-next/src/branch/state.rs`, `crates/storage-next/src/branch/read.rs`, and `crates/storage-next/src/branch/state/`. |
| `crates/storage/src/segment.rs` | `KVSegment`, point lookup over non-overlapping levels, table key-range facts, `OwnedSegmentIter`, and `LevelSegmentIter`. | L6 source planner over `BranchOwnedTable` facts plus L5 table seek/cursor surfaces. |
| `crates/storage/src/seekable.rs` | Seekable per-level scan behavior that positions by key range and opens table iterators lazily. | Lazy L6 level scan cursor over nonzero branch levels. |
| `crates/storage/src/merge_iter.rs` | MVCC merge, tombstone/TTL filtering, and inherited-row ordering. | L6 branch merge/collapse rules for point, history, and scans. |
| `crates/storage/src/segmented/compaction.rs` | Branch-owned compaction input selection, overlap planning, and level install shape. | `crates/storage-next/src/branch/state/compaction.rs`. |
| `crates/storage/src/compaction.rs` | Streaming compaction mechanics and output splitting consumed by branch state. | L6 source preparation plus L5 streaming table compaction/building. |
| `crates/storage/src/segmented/tests/fork.rs` | Fork-frontier, inherited COW, and child shadowing behavior. | `crates/storage-next/src/branch/tests/inheritance_materialization/` and fork tests. |
| `crates/storage/src/segmented/tests/leveled.rs` | L0/L1+ invariants and non-overlap source behavior. | L6 source-layout and level-pruning tests. |
| `crates/storage/src/segmented/tests/materialize.rs` | Materialization correctness and retry shape. | L6 materialization streaming and reachability tests. |
| `crates/storage/src/segmented/tests/resurrection.rs` | Put/delete/put, tombstone, and versioned visibility behavior. | L6 MVCC/history tests and branch model scripts. |
| `crates/storage/src/segmented/tests/post_restart_branch.rs` | Branch-visible behavior after durable restart. | L6/L8/L4 integration tests after durable publication slices. |

Do not port:

1. old table bytes or old durable manifest layout;
2. direct filesystem IO or old path construction;
3. public API semantics into L6;
4. L8 maintenance scheduling decisions into L6;
5. L7 commit validation, version allocation, or WAL-before-visible discipline
   into L6;
6. product DTOs or benchmark-specific shortcuts.

## Storage-Next Source Map

Current storage-next targets:

| Surface | Current file | Parity action |
| --- | --- | --- |
| Branch state | `crates/storage-next/src/branch/state.rs` | Preserve active/frozen/owned/inherited topology; add cheap source-layout facts and avoid row-scale facts in hot paths. |
| Read planner | `crates/storage-next/src/branch/read.rs` | Restore point source pruning and lazy scan source planning. |
| Read hooks | `crates/storage-next/src/branch/state/read_hooks.rs` | Make read-view capture and timestamp/facts lookup bounded by source/facts shape rather than retained rows. |
| Fork state | `crates/storage-next/src/branch/state/fork.rs` | Preserve current L6 preconditions; define handoff points for L7/L8 quiesce/flush if old public ergonomics are restored. |
| Materialization | `crates/storage-next/src/branch/state/materialization.rs` | Replace eager row-vector collection with cursor/source streaming once L5 supports the required table artifact path. |
| Branch compaction | `crates/storage-next/src/branch/state/compaction.rs` | Pass table handles/cursors to L5 instead of row vectors; preserve stale-candidate and install checks. |
| Snapshot install | `crates/storage-next/src/branch/state/snapshot.rs` | Preserve all-or-nothing replacement while proving source layout and read-view pinning behavior. |
| Branch pruning/facts | `crates/storage-next/src/branch/pruning.rs`, `crates/storage-next/src/branch/facts.rs` | Keep retention and reachability policy in L6 while moving normal facts calls away from row scans. |
| L6 tests | `crates/storage-next/src/branch/tests/` | Add source-count, model, and generated tests for old asymptotic behavior. |
| Branch testkit | `crates/storage-next/src/testkit/branch_lsm/` | Extend generated workloads with source topology and inherited-layer scripts. |
| Perf trace | `crates/storage-next/src/observability/perf_trace.rs` | Add source-class counters fed by L6, not benchmark adapters. |
| L8 consumers | `crates/storage-next/src/lifecycle/flush.rs`, `compaction.rs`, `branch_lifecycle.rs` | Consume L6 shape/facts; do not embed L6 source-planning logic in lifecycle code. |

## Layer Ownership Check

L6 owns:

1. branch-local source topology: active, frozen, L0, nonzero levels, inherited
   layers, fork-version gates, and child shadowing;
2. source selection and source ordering for branch point reads, history, prefix
   scans, and range scans;
3. branch-level MVCC collapse, tombstone handling, TTL visibility, and source
   precedence;
4. branch source-layout diagnostics and source-class perf counters;
5. read-view shape and lifetime for branch-local sources;
6. branch facts and timestamp facts surfaces;
7. branch compaction and materialization source preparation;
8. branch reachability snapshots and table-reference facts.

L6 does not own:

1. table bytes, table index/filter/cache internals, data-block decoding, or
   table-local cursors;
2. backend IO, object names, durable table publication, WAL, manifests,
   checkpoints, or recovery policy;
3. version allocation, commit validation, CAS/read-set semantics, or
   WAL-before-visible ordering;
4. maintenance scheduling, compaction scoring, background queues, retention
   execution, or write pressure policy;
5. public L9 APIs, benchmark-only knobs, product DTOs, or UX wording.

## Predecessors

Required:

1. M4P-L1 keeps direct IO behind backend boundaries.
2. M4P-L2 keeps object-family parsing and canonical names out of L6.
3. M4P-L3 table format remains unchanged unless a separate format decision
   slice proves durable bytes must change.
4. M4P-L4 table publication/rewrite windows remain the durable service
   boundary.
5. M4P-L5 exposes table key bounds, point seek, raw cursor, source facts, and
   table-local counters without requiring L6 to inspect table bytes.

Conditional:

1. Bounded timestamp lookup requires an L7 commit timeline/facts surface. L6
   must not invent version allocation or timeline ownership to close that gap.
2. Streaming branch compaction and materialization require L5 streaming
   compaction/build support. L6 can prepare table handles/cursors, but L5 owns
   table-local output construction.
3. Old public fork ergonomics require L7/L8 quiesce/flush orchestration. L6 can
   expose typed preconditions and safe fork mechanics, but higher layers own
   making public fork calls ergonomic.

## Execution Plan

### M4P-L6A. Source Layout Diagnostics And Source-Class Counters

Goal: make branch source shape and hot-path source work measurable before
changing planner behavior.

Steps:

1. Add a branch source-layout diagnostic surface that reports:
   - active row count;
   - frozen table count and rows;
   - owned L0 table count;
   - owned nonzero level count and table counts by level;
   - owned total table count;
   - inherited layer count;
   - inherited L0 table count;
   - inherited nonzero level count and table counts by level;
   - readable, materializing, materialized, and unavailable inherited layers.
2. Add point-read counters by source class:
   - active probes;
   - frozen probes;
   - owned L0 table probes;
   - owned nonzero level searches;
   - owned nonzero table probes;
   - inherited layer searches;
   - inherited L0 table probes;
   - inherited nonzero level searches;
   - inherited nonzero table probes;
   - table seeks;
   - candidates materialized;
   - rows visited.
3. Add scan counters by source class:
   - active cursors;
   - frozen cursors;
   - owned L0 cursors;
   - owned nonzero level cursors;
   - owned nonzero table cursors opened;
   - inherited L0 cursors;
   - inherited nonzero level cursors;
   - inherited nonzero table cursors opened;
   - cursor seeks;
   - rows visited;
   - rows returned.
4. Add history, timestamp, and branch-facts counters that expose unrelated row
   scans separately from expected key-local work.
5. Feed counters through the existing perf-trace sink and expose test-only
   source-layout facts without requiring `perf-trace`.
6. Record a 100K baseline for point, scan-prefix, scan-range, and history using
   the L9 benchmark surface.

Exit gate:

1. Point, scan, and history counters distinguish active, frozen, owned L0,
   owned nonzero levels, inherited L0, and inherited nonzero levels.
2. Source-layout facts are available to tests without benchmark adapters.
3. No behavior changes beyond diagnostics and counters.

Stop condition:

If the counters cannot identify whether work scales with table count, level
count, or row count, stop here and fix observability before changing read or
scan planners.

### M4P-L6B. Nonzero-Level Point-Read Pruning

Goal: restore old point-read source shape for non-overlapping levels.

Steps:

1. Introduce an L6 point source planner for one physical key.
2. Preserve current source precedence:
   - active;
   - frozen newest to oldest;
   - owned L0 newest to oldest;
   - owned nonzero levels;
   - readable inherited layers nearest ancestor first, each with its own L0
     and nonzero levels after key rewrite and fork-bound application.
3. Keep active, frozen, and L0 behavior intentionally linear in source count.
   These sources are overlapping by design.
4. For each owned nonzero level, use level key-range facts to binary-search the
   sorted non-overlapping table list and probe at most one table for the
   physical key.
5. Apply the same rule to every readable inherited nonzero level after
   effective inherited key/bound rewriting.
6. Use L5 table seek/filter behavior for the selected tables. L6 must not
   inspect table bytes or implement table-local lookup.
7. Preserve latest, version-bounded, timestamp-bounded, tombstone, TTL,
   inherited fork-version, and child-shadowing semantics.
8. Remove or quarantine any branch point path that probes every nonzero-level
   table for one key.

Exit gate:

1. Point reads probe all active/frozen/L0 sources and at most one table per
   nonzero level per readable layer.
2. Latest, version, timestamp, tombstone, TTL, inherited, and missing-key point
   results match the independent branch model.
3. Counters show nonzero table probes are bounded by level count, not total
   table count.

Stop condition:

If point-read correctness regresses or counters still scale with nonzero table
count, stop before scan work and isolate the branch source planner.

### M4P-L6C. Lazy Nonzero-Level Scan Planning

Goal: restore old scan setup shape where nonzero levels contribute lazy level
cursors rather than one eager cursor per table.

Steps:

1. Keep per-source cursors for active, frozen, and L0 sources.
2. Add an L6 lazy level cursor for sorted non-overlapping nonzero levels:
   - binary-search to the first table whose range can overlap the scan bounds;
   - open the first table cursor only when needed;
   - advance to subsequent tables lazily;
   - skip tables whose range cannot overlap the remaining scan bounds.
3. Apply prefix and range overlap pruning before creating any table cursor.
4. Preserve inherited-layer behavior:
   - rewrite requested child keys to source branch keys;
   - apply fork-version bounds;
   - preserve nearest-ancestor ordering and child shadowing;
   - use lazy level cursors for inherited nonzero levels.
5. Preserve MVCC collapse, tombstone filtering, TTL filtering, source
   precedence, scan ordering, and stable limit behavior.
6. Pass scan limits down to the earliest L6/L5 boundary supported by existing
   APIs. If an upper layer cannot yet pass a limit, record that as an L9/API
   follow-up rather than implementing an L6 fast path.

Exit gate:

1. Scan setup creates per-table cursors for active/frozen/L0 and lazy level
   cursors for nonzero levels.
2. Prefix and range overlap pruning happens before cursor creation.
3. Cursor setup counters are bounded by source count plus nonzero level count,
   not total table count.
4. Scan-prefix and scan-range results match the independent branch model.

Stop condition:

If limited scans still open every nonzero table before yielding rows, stop and
fix cursor planning before touching materialization or compaction paths.

### M4P-L6D. Bounded History, Timestamp, And Hot Facts

Goal: remove retained-row scans from normal branch history, timestamp lookup,
and branch facts.

Steps:

1. Split implementation if needed:
   - history boundedness;
   - timestamp-to-version boundedness;
   - branch facts boundedness.
2. Make history for one physical key use the L6 point/level source planner
   shape rather than walking unrelated physical keys.
3. Preserve history semantics:
   - version ordering;
   - tombstone inclusion where currently documented;
   - TTL filtering where currently documented;
   - inherited fork-version caps;
   - child shadowing;
   - history limits.
4. Replace timestamp-to-version retained-row scans with a branch timeline/facts
   lookup. If the required L7 timeline surface is not available yet, document a
   typed deferral and keep counters proving the remaining scan.
5. Replace normal branch facts recomputation with maintained/recovered facts:
   - max commit version;
   - timestamp coverage;
   - min/max timestamp facts;
   - put/tombstone counters where required;
   - source-layout facts from installed table descriptors.
6. Reserve full row scans for explicit validation/debug/rebuild paths, and
   ensure those paths are named and measured separately.

Implementation note:

- Branch-local history and normal branch facts are restored in this slice.
- Timestamp-to-version lookup still uses the branch row scan in
  `BranchLocalState::resolve_timestamp_to_commit_version` because the branch
  runtime does not yet own a branch timeline/facts lookup surface.
- Deferral owner: L7 commit timeline integration.
- Counter proof while deferred: `timestamp_*_rows_scanned` remains nonzero for
  timestamp lookups that touch retained branch rows.
- Closure slice: the L7 timeline/facts restoration must replace this row scan
  and update these counters to zero for the normal lookup path.

Exit gate:

1. History for one key does not scan unrelated physical keys.
2. Normal timestamp lookup does not scan retained branch rows after the L7 facts
   dependency is available.
3. Normal branch facts calls do not scan table rows.
4. Any deferred timestamp or facts sub-gap records owner layer, reason, current
   counter proof, and the later slice that closes it.

Stop condition:

If correctness requires scanning unrelated keys, stop and record the semantic
or ownership blocker before proceeding to read-view or compaction changes.

### M4P-L6E. Cheap Pinned Read Views

Goal: make read-view capture proportional to source handles/facts rather than
retained row count.

Steps:

1. Audit `BranchReadView` ownership for active, frozen, owned, and inherited
   sources.
2. Convert read views to pin shared immutable handles or snapshots for table
   sources instead of deep-cloning table rows.
3. Preserve snapshot isolation across:
   - later commits;
   - active-table rotation;
   - flush installation;
   - compaction replacement;
   - materialization replacement;
   - snapshot branch replacement;
   - cleanup/reachability release.
4. Keep lifetime and reachability explicit so L8/L4 cleanup cannot reclaim a
   table still visible to a read view.
5. Add counters for read-view captures, source handles cloned, rows cloned, and
   clone bytes.

Exit gate:

1. Capturing a read view is bounded by source count and does not clone table
   rows.
2. A read view remains stable after subsequent branch mutations and lifecycle
   operations.
3. Cleanup/reachability tests prove pinned sources are not dropped while a read
   view can still use them.

Stop condition:

If pinned read views require an L5 reader ownership change, stop and open the
smallest L5 prerequisite rather than adding an L6 workaround.

### M4P-L6F. Streaming Branch Compaction Source Preparation

Goal: stop preparing branch compaction inputs as L6-owned row vectors.

Steps:

1. Audit every branch compaction source path that calls row collection or
   `rows().to_vec()`.
2. Replace eager row-vector source preparation with sorted table handles,
   source descriptors, or L5 cursors.
3. Keep L6 ownership of:
   - compaction candidate selection;
   - overlap selection;
   - stale-candidate validation;
   - pruning proof validation;
   - replacement installation;
   - level invariant validation.
4. Keep L5 ownership of:
   - cursor movement;
   - table-local merge mechanics;
   - output table building;
   - table-local split boundaries.
5. Add peak buffered row and source-open counters for branch compaction.
6. Preserve compaction correctness for tombstones, TTL, retention, duplicate
   internal-key rejection, and shared-table pruning proofs.

Exit gate:

1. L6 branch compaction preparation no longer clones full input tables into
   row vectors on the standard path.
2. Peak buffered rows are bounded by streaming cursor/output mechanics.
3. Existing compaction install and stale-candidate tests still pass.

Stop condition:

If L5 cannot consume streaming sources yet, keep L6 candidate planning intact
and open a focused L5 prerequisite instead of reimplementing table compaction
inside L6.

### M4P-L6G. Streaming Materialization Source Preparation

Goal: make materialization consume normal L6/L5 cursor mechanics instead of a
separate eager collection path.

Steps:

1. Audit inherited-layer materialization row collection and fixed L0 output
   table construction.
2. Replace source collection with cursor/source descriptors that apply:
   - inherited key rewriting;
   - fork-version filtering;
   - child shadowing;
   - tombstone/TTL visibility rules;
   - duplicate replacement-row checks.
3. Emit replacement artifacts through the same streaming table artifact path
   used by branch compaction.
4. Preserve current materialization state transitions:
   - active to materializing;
   - retry after partial work;
   - replacement install;
   - inherited-layer removal;
   - reachability binding;
   - recovery classification.
5. Add counters for source tables opened, rows rewritten, rows skipped by
   fork-version, rows skipped by shadowing, output tables produced, and peak
   buffered rows.

Exit gate:

1. Materialization no longer scans an inherited layer into one L6-owned row
   vector on the standard path.
2. Current retry, reachability, fork-version, and child-shadowing semantics are
   preserved.
3. Materialized branch reads and scans match pre-materialization results.

Stop condition:

If durable replacement publication is the blocker, keep the L6 source
preparation plan separate and defer publication proof to L4/L8.

### M4P-L6H. Fork Contract And Higher-Layer Handoff

Goal: keep L6 fork mechanics safe while documenting and testing the boundary
for old public fork ergonomics.

Steps:

1. Preserve L6's safe fork preconditions unless a measured and tested higher
   layer change is ready.
2. Document the current L6 fork contract:
   - source branch must satisfy L6 capture preconditions;
   - inherited layers must be readable or explicitly skipped/rejected by
     status;
   - fork-version facts must cap inherited visibility;
   - child-local writes shadow inherited rows.
3. Define the L7/L8 handoff for public old-style ergonomics:
   - quiesce or reject pending commits;
   - flush active/frozen source rows if required;
   - retry fork capture with typed failure if source shape changes.
4. Add tests that prove L6 reports typed precondition failures without losing
   fork-frontier correctness.

Current L6 fork contract:

1. `fork_into_empty_child` is branch-local source capture only. It requires the
   destination branch id to differ from the source branch id.
2. The source branch must have no active or frozen rows. L6 rejects that shape
   with a typed `InvalidInheritedLayer` error instead of flushing or waiting.
3. The source branch must have at least one retained row across owned or
   inherited sources, so the fork frontier is a real retained version rather
   than an ambiguous zero-version fork.
4. The child receives an inherited layer for the source branch's owned table
   topology and then receives forkable inherited layers from the source in
   nearest-first order.
5. Active and materializing inherited layers are forkable. Materialized layers
   are skipped because their replacement rows are already expected to be
   visible through the source branch. Unavailable layers are rejected.
6. Inherited read bounds cap source visibility at the captured fork version.
   Parent commits, compaction, or materialization after the fork must not change
   child-visible latest, historical, or bounded-version rows.
7. Child-local puts and tombstones remain higher precedence than inherited
   source rows.

Higher-layer handoff:

1. L7 owns commit ordering, pending-commit quiesce, version allocation, and
   typed retry/failure if the source branch changes while a public fork request
   is being prepared.
2. L8 owns maintenance orchestration needed to make the source forkable, such
   as flushing active/frozen rows or scheduling/retrying the operation when
   branch pressure allows it.
3. L9 owns public API wording and old-style ergonomic behavior. It may expose a
   convenient fork call, but it must drive L7/L8 work before invoking this L6
   capture primitive.
4. L6 must not wait for commits, trigger flushes, publish durable objects, or
   reinterpret public fork modes. Those are explicit deferrals, not branch-local
   mechanics.

Exit gate:

1. L6 fork behavior is explicit and tested.
2. Any old public ergonomic gap is assigned to L7/L8/L9 with a clear reason.
3. No L6 change absorbs commit ordering or maintenance scheduling.

Stop condition:

If a proposed fork change requires commit/runtime orchestration, stop and move
that work to the owning L7/L8 plan.

### M4P-L6I. Closeout And Benchmark Gate

Goal: prove L6 parity through model tests, generated workloads, source guards,
and old-vs-new benchmarks.

Steps:

1. Update the L6 independent branch model to cover:
   - active/frozen/owned/inherited sources;
   - L0 overlapping tables;
   - nonzero non-overlapping levels;
   - fork-version caps;
   - child shadowing;
   - tombstones;
   - TTL behavior as documented;
   - materialization and compaction transitions.
2. Extend generated branch scripts with source-shape operations that create
   many L0 tables, many nonzero levels, many tables per level, and inherited
   chains.
3. Add source guards proving L6 does not import commit, lifecycle, API,
   backend, filesystem, object-layout construction, or product DTOs.
4. Run storage-next tests with relevant feature gates.
5. Run L9 benchmarks serially against old and new engines at:
   - 100K keys after every hot-path slice;
   - 1M, 5M, and 10M after point/scan/history corrections;
   - 50M and 100M only after 10M source-shape counters are clean.
6. Record benchmark metadata and derived counters:
   - `point_source_probes_per_read`;
   - `point_nonzero_table_probes_per_read`;
   - `scan_source_cursors_per_call`;
   - `scan_table_cursors_opened_per_call`;
   - `scan_rows_visited_per_row_returned`;
   - `l0_tables_per_million_rows_after_load`.
7. Update audit documents with closed findings, deferred findings, owner layer,
   reason, and replacement proof.

Exit gate:

1. Latest, version, timestamp, history, prefix, and range reads match the
   independent branch model.
2. Point source counters are bounded by active/frozen/L0 plus one nonzero table
   per level per readable layer.
3. Scan cursor setup is bounded by active/frozen/L0 plus nonzero level count
   per readable layer.
4. History for one key does not scan unrelated physical keys.
5. Read-view capture does not clone table rows.
6. L6 source guards pass.
7. L9 benchmarks show old-equivalent source shape before throughput is used as
   the final comparison.

Stop condition:

If source-shape counters are clean but throughput remains materially worse,
start a new measured plan for the next owning layer rather than widening this
L6 slice.

## Benchmark Methodology

Benchmarks are proof gates, not implementation owners.

Rules:

1. Run old and new engines serially on the same host. Do not run them
   simultaneously unless the run is explicitly testing interference.
2. Include git revision, machine, target architecture, build profile, engine,
   mode, durability policy, backend, feature state, key count, value size,
   scan samples, scan limit, maintenance policy, and perf-trace state.
3. Compare source-shape counters before comparing wall-clock throughput.
4. Use 100K for fast slice validation.
5. Use 1M, 5M, and 10M for scale-shape validation.
6. Use 50M and 100M only after 10M source-shape counters are clean.

## Non-Goals

1. Do not change public L9 API semantics to improve a benchmark.
2. Do not special-case cache mode in a way durable mode cannot share.
3. Do not change durable table or manifest bytes in this L6 plan.
4. Do not move table-reader/index/filter/cache logic into L6.
5. Do not move lifecycle scheduling or compaction scoring into L6.
6. Do not weaken MVCC, tombstone, TTL, inherited-layer, materialization, or
   snapshot-isolation behavior.
7. Do not add roadmap labels to Rust identifiers, comments, fixture bytes,
   panic messages, or user-visible text.

## Expected Counter Movement

| Slice | Expected movement |
| --- | --- |
| M4P-L6A | New source-layout and source-class counters populate without behavior changes. |
| M4P-L6B | Nonzero table probes per point read fall from table-count scaled to level-count scaled. |
| M4P-L6C | Nonzero scan cursor setup falls from table-count scaled to level-count scaled; table cursors open lazily as rows are consumed. |
| M4P-L6D | History unrelated-row scans go to zero; normal timestamp/facts row scans go to zero once their facts dependency is in place. |
| M4P-L6E | Read-view row clone count and clone bytes go to zero on ordinary reads. |
| M4P-L6F | Branch compaction peak buffered rows become bounded by streaming source/output mechanics instead of full input size. |
| M4P-L6G | Materialization peak buffered rows become bounded by streaming source/output mechanics instead of full inherited-layer size. |
| M4P-L6I | L9 old-vs-new benchmarks report old-equivalent source shape before throughput comparison. |

## Deferred Findings

Deferrals are allowed only with owner, reason, and replacement proof.

Known conditional deferrals:

1. Durable table format changes for bloom/filter blocks stay with L3 and are
   not part of this L6 plan.
2. Public fork ergonomics stay with L7/L8/L9 unless L6 safety preconditions can
   be removed without commit or scheduling ownership.
3. Automatic maintenance drain and L0 fanout under sustained writes stay with
   L8 after L6 serving topology is restored.
4. Commit timeline construction stays with L7; L6 may consume timeline facts
   for timestamp lookup but must not allocate or validate commit versions.
