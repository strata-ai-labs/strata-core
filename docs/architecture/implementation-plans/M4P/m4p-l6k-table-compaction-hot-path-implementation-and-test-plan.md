# M4P-L6K Implementation And Test Plan: Table Compaction Hot Path Closure

Status: draft follow-on implementation and test plan

Parent compaction-shape plan:
`docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-implementation-plan.md`

Related compaction-shape test plan:
`docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-test-plan.md`

Audit context:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Serving-path context:
`docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

## Objective

Close the table-compaction performance gaps that remain after the LSM source
shape is correct. The goal is to make storage-next compaction mechanically
closer to old storage without changing table bytes, branch visibility
semantics, durable object layout, or automatic scheduling policy.

This plan targets compaction work that is already explicit/manual or already
selected by lifecycle maintenance. It does not attempt to restore the automatic
flush/compaction scheduler, score-based compaction picker, durable Bloom/filter
blocks, or ArcSwap-style branch version publishing.

## Current Code Reality

The gap review is directionally correct, with two current-code corrections:

1. Row-content table output fingerprinting has already been removed from table
   compaction output identity. Current output identity hashes source metadata
   plus output index, not every row value.
2. The table compaction policy call is generic over
   `impl TableCompactionPolicy + ?Sized`. It can still dispatch dynamically if
   a caller passes a trait object, but concrete policies are not necessarily
   forced through a vtable.

The remaining current hot-path issues are real:

1. `validate_no_global_duplicate_internal_keys` performs a full merged-stream
   pass before real compaction and clones each key it remembers.
2. The main merge loop clones each `TableRow` before policy evaluation and
   output buffering.
3. `TableCompactionHeapItem` owns a `TableInternalKeyBytes`, and source advance
   clones the key for `last_key` and the heap item.
4. Source key-order validation runs on every advance and clones the latest key.
5. Physical-key boundary checks allocate a `Vec<u8>` for every kept row, and
   allocate again when an output split is being considered.
6. Compaction output still buffers `Vec<TableRow>` until the target output size
   before calling `ImmutableTableBuilder::build_from_rows`.
7. Lifecycle fixed-point drain can do many operations when explicitly invoked,
   and benchmark results must not confuse "one compaction operation" with
   "explicit drain to stable source shape."
8. The L0 maintenance threshold is currently lower than old storage cadence.
9. Metadata-only promotion exists, but the old trivial-move behavior is broader
   than the storage-next planner should assume until parity tests prove it.

## Ownership

Table runtime owns:

1. merge cursor mechanics;
2. duplicate/order invariant placement;
3. physical-key boundary caching;
4. output buffering and streaming builder behavior;
5. table-level compaction counters.

Branch LSM runtime owns:

1. metadata-only promotion eligibility;
2. nonzero-level overlap and non-overlap invariants;
3. compaction candidate shape;
4. source layout after install.

Lifecycle runtime owns:

1. L0 compaction threshold constants;
2. explicit fixed-point drain boundaries;
3. benchmark maintenance sequencing;
4. cache and durable parity at the compaction boundary.

This plan does not own:

1. automatic scheduling and score-based compaction selection;
2. durable table-format changes;
3. durable Bloom/filter bytes;
4. backend object naming;
5. public API wording;
6. ArcSwap-style branch version publication.

## Implementation Slices

### L6K-A. Add Compaction Mechanical Counters

Goal: make each hot-path fix measurable before changing behavior.

Steps:

1. Add table-compaction counters only for mechanical tests and benchmarks:
   - merge cursor opens;
   - merge advances;
   - pre-validation rows scanned;
   - rows cloned for policy/output ownership;
   - heap key clones;
   - source order key clones;
   - physical-key boundary materializations;
   - kept rows;
   - dropped rows;
   - peak buffered rows;
   - output tables built.
2. Keep these counters under the existing perf/observability surface and avoid
   making every correctness test require `perf-trace`.
3. Add reset/snapshot tests for the new counters.
4. Record baseline values for representative compactions before changing the
   hot loop.

Exit gate:

1. A focused compaction test can prove whether a change removed a full
   validation pass, heap key clone, or physical-key allocation.
2. Non-performance tests can still run without perf-gated assertions.

### L6K-B. Remove The Production Pre-Merge Duplicate Pass

Goal: avoid walking the whole merged stream twice for every compaction.

Steps:

1. Move `validate_no_global_duplicate_internal_keys` out of the default
   production compaction path.
2. Preserve duplicate protection in narrower places:
   - source table construction still validates sorted unique table rows;
   - source cursor order validation remains until L6K-C decides whether it can
     be debug-gated;
   - output builder still validates kept output rows;
   - optional strict/debug compaction mode can run the full duplicate pass for
     invariant tests.
3. If the strict path remains callable, make it explicit in test helpers rather
   than hidden in every production compaction.
4. Document that exact internal-key duplication across LSM sources is treated as
   corruption or debug invariant failure, not a required release-mode
   row-resolution feature.

Exit gate:

1. Default compaction scans input rows once.
2. Corrupt duplicate-input tests still have an explicit strict/debug validation
   path.
3. Correct compaction output remains sorted and unique.
4. Existing branch read, history, tombstone, and TTL semantics do not change.

### L6K-C. Reduce Merge Key Cloning

Goal: stop allocating owned key bytes on every source advance.

Steps:

1. Add a small-source merge path for compactions with four or fewer sources:
   - scan current source keys linearly;
   - choose the minimum key with source tie-break ordering;
   - do not store key bytes in a heap item.
2. Keep the heap path for larger source counts initially.
3. For the heap path, choose one of these approaches after measurement:
   - convert `TableInternalKeyBytes` storage to shared bytes such as
     `Arc<[u8]>`, making heap items cheap to clone;
   - store a compact key handle if table-reader row storage can guarantee
     stable backing bytes;
   - keep owned heap keys only for large source counts if the small-source path
     covers the common old-storage shape.
4. Do not borrow row keys into `BinaryHeap` unless the lifetime model is proven
   safe across cursor advances.
5. Preserve source tie-break order exactly.

Exit gate:

1. Compactions with one through four sources report zero heap key clones.
2. Larger-source compactions either reduce key clone bytes or document why the
   heap path remains the measured bottleneck.
3. Merge order remains identical to the current implementation for equal keys
   and source precedence.

### L6K-D. Rework Row Ownership To Avoid Per-Row Cloning

Goal: avoid cloning `TableRow` just to escape the merge cursor borrow.

Steps:

1. Split policy decision from output ownership so the policy can inspect a
   borrowed row.
2. After the policy decides to keep a row, move or copy only the data needed by
   the output builder.
3. Prefer a streaming output path from L6K-F. If streaming is not ready, use a
   bounded pending-row representation that avoids a full `StorageRow` clone
   where possible.
4. Keep dropped rows borrowed only; dropped rows must not allocate.
5. Preserve policy context fields:
   - source id;
   - source index;
   - source row index;
   - merged row index;
   - previous kept key.

Exit gate:

1. Dropped rows incur no `TableRow` clone.
2. Kept rows are copied at most once into the output builder or pending output.
3. Policy semantics are unchanged.

### L6K-E. Cache Physical-Key Boundary Bytes

Goal: remove per-kept-row physical-key `Vec<u8>` allocation from split
decisions.

Steps:

1. Use `TableInternalKeyBytes::physical_key_bytes()` as the canonical
   zero-allocation slice for current-row physical-key comparisons.
2. Compute the current row physical-key slice once per considered row.
3. Pass that slice into both split decision and pending-row append logic.
4. Update the pending last physical-key cache only when the physical key
   changes.
5. Keep output splitting from separating versions of the same physical key.

Exit gate:

1. Physical-key materialization count is bounded by physical-key changes plus
   target-crossing checks, not kept row count.
2. Compaction still never splits a physical-key version chain across output
   tables.
3. Output table bounds remain correct.

### L6K-F. Add Streaming Table Output Builder

Goal: avoid buffering large `Vec<TableRow>` output chunks before writing table
bytes.

Steps:

1. Add an internal streaming builder API alongside `build_from_rows`:
   - begin table with identity and config;
   - append sorted rows one at a time;
   - flush data blocks as they reach block limits;
   - finish index, metadata, Bloom/runtime filter data, and footer exactly as
     `build_from_rows` does today.
2. Keep `build_from_rows` as a compatibility wrapper over the streaming builder
   where possible.
3. Preserve all existing table artifact facts:
   - row count;
   - key bounds;
   - physical-key bounds;
   - commit timestamp bounds;
   - tombstone counts;
   - block metadata;
   - filter facts.
4. Integrate streaming output with compaction splitting:
   - when target output bytes would be crossed on a different physical key,
     finish the current streaming output;
   - start the next output table with a fresh identity;
   - never split versions of the same physical key.
5. Do not reintroduce row-content FNV identity hashing.

Exit gate:

1. Peak buffered rows are bounded by current data-block assembly, not 64 MiB of
   table rows.
2. Table bytes produced from streaming and `build_from_rows` are semantically
   equivalent under existing reader tests.
3. Output splitting remains deterministic.

### L6K-G. Align L0 Compaction Cadence And Drain Boundaries

Goal: make benchmark and maintenance interpretation match old-storage cadence.

Steps:

1. Raise the L0 compaction threshold from two tables to the old cadence of four
   tables unless a current test proves a correctness reason for the lower
   threshold.
2. Separate benchmark modes:
   - single selected compaction operation;
   - explicit fixed-point drain;
   - automatic scheduling once L8 owns it.
3. Ensure scale benchmarks report which mode they used.
4. Keep fixed-point drain available for source-shape stabilization, but do not
   use its throughput as if it were one old-storage scheduler tick.

Exit gate:

1. L0 pressure tests assert the intended threshold.
2. Benchmark output distinguishes single operation from explicit drain.
3. Fixed-point drain remains idempotent and bounded.

### L6K-H. Broaden Metadata-Only Promotion Parity

Goal: avoid table rewrites when old storage would do a safe trivial move.

Steps:

1. Audit current metadata promotion eligibility against old trivial-move rules:
   - selected input table has no next-level overlap;
   - target level is not the configured last input level;
   - grandparent overlap constraints are respected where storage-next tracks
     enough metadata to enforce them;
   - row pruning is not requested.
2. Extend nonzero-level promotion where safe.
3. Keep L0 promotion rules separate because L0 has source-order semantics.
4. Record a typed no-promotion reason when promotion is unsafe.

Exit gate:

1. Safe non-overlap compactions promote metadata instead of rewriting bytes.
2. Overlap cases still rewrite and preserve visibility.
3. Promotion never creates overlapping nonzero ranges.
4. Promotion never leaves one table identity reachable from two levels.

## Test Plan

### Correctness Tests

Add or update table compaction tests:

1. Compaction without the pre-validation pass preserves latest, history,
   tombstone, and TTL behavior.
2. Output rows remain sorted and unique.
3. Source duplicate corruption is caught by explicit strict/debug validation or
   by output validation when duplicates survive policy pruning.
4. Duplicates within one source still fail source/table validation.
5. Multi-source ordering remains stable across source-index tie breaks.
6. Output splitting does not split one physical-key version chain.
7. Streaming builder output opens with `ImmutableTableReader` and returns the
   same reads/scans as `build_from_rows`.

Add or update branch compaction tests:

1. L0 overlap rewrite still preserves L0 source precedence.
2. L0 no-overlap promotion remains correct.
3. L1+ no-overlap promotion avoids byte rewrite where safe.
4. L1+ overlap rewrite still removes only selected input and overlap tables.
5. Repeated fixed-point drain remains idempotent after hot-loop changes.

### Mechanical Counter Tests

Only use perf-gated assertions for mechanical tests.

Required assertions:

1. Default compaction does not increment pre-validation row-scan counters.
2. Strict/debug compaction increments pre-validation counters when explicitly
   requested.
3. One-source and small-source compactions perform zero heap key clones.
4. Dropped rows do not increment row-clone counters after L6K-D.
5. Physical-key materialization count is less than kept rows for multi-version
   same-key workloads after L6K-E.
6. Peak buffered rows falls after streaming output lands.
7. Output table count and split count remain unchanged for deterministic
   fixtures.

### Fault And Failure Tests

1. If streaming builder fails while writing an output, no branch state is
   installed.
2. If a later output table fails, earlier unpublished artifacts are handled by
   the existing lifecycle failure path.
3. Durable compaction still publishes output before branch install.
4. Durable manifest failure keeps reads unchanged or records existing durable
   debt consistently.
5. Promotion failure before manifest publication leaves the old level layout
   readable.

### Generated Tests

Extend generated branch LSM workloads with:

1. random table source counts from one through at least eight;
2. random overlap and non-overlap between adjacent levels;
3. random same-physical-key version chains near output split boundaries;
4. repeated compaction under small output target bytes;
5. random policy drops for tombstone/TTL/pruning cases already supported by the
   current table compaction policy.

Generated invariants:

1. Reads before and after compaction match the model.
2. Output tables remain sorted and non-overlapping where the branch level
   requires non-overlap.
3. No generated workload depends on pre-validation for correctness.
4. Streaming and non-streaming table build paths are semantically equivalent.

### Benchmark Gates

Run only after focused tests pass.

Scenarios:

1. table-only compaction microbench with 1, 2, 4, 8, and 32 sources;
2. branch compaction with L0 overlap rewrite;
3. branch compaction with L1+ non-overlap promotion;
4. explicit fixed-point drain after manual flush;
5. 100K, 1M, 5M, and 10M storage-next scale loads with manual flush and
   explicit compact drain when source-shape proof is needed.

Report:

1. input rows;
2. kept rows;
3. dropped rows;
4. sources;
5. output tables;
6. peak buffered rows;
7. merge advances;
8. heap key clones;
9. row clones;
10. physical-key materializations;
11. output bytes;
12. elapsed compaction time;
13. final source layout.

Interpretation rules:

1. Do not compare fixed-point drain time to one old-storage scheduler tick.
2. Do not attribute point-read slowness to compaction until final source shape
   is verified.
3. Do not tune block size until clone/pass/buffering counters move in the
   expected direction.

## Stop Conditions

Stop and re-plan if any of these occur:

1. Removing the pre-validation pass changes visible results for valid inputs.
2. A borrowed-key heap design requires unsafe lifetimes or cursor aliasing.
3. Streaming builder requires a durable table-format change.
4. Physical-key boundary caching can split versions of one physical key across
   output tables.
5. Promotion broadening creates overlapping nonzero ranges.
6. Benchmarks show no counter movement after the targeted hot-loop change.
7. A proposed fix requires automatic scheduling policy to prove correctness.

## Deferred Work

These findings are intentionally not closed by this plan:

1. automatic flush/compaction scheduling and score-based picking;
2. ArcSwap-style branch version publication;
3. compaction rate limiting;
4. durable Bloom/filter blocks;
5. public runtime API changes;
6. table block-size retuning beyond measurement.

