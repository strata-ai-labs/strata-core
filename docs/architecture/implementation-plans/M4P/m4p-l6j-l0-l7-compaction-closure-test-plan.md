# M4P-L6J Test Plan: LSM Level Compaction Closure

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Related L6 test plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-test-plan.md`

## Goal

Prove that explicit/manual storage-next compaction can drain branch-owned table
data from L0 through the configured last LSM level, while preserving correctness,
branch isolation, durable publication safety, and source-shape performance
invariants.

The tests must prove:

1. default configured compaction levels match the accepted target;
2. explicit compaction can move or rewrite tables from L0 through the last
   configured level;
3. single-table no-overlap cases promote metadata instead of getting stuck;
4. overlapping cases rewrite through the normal table-compaction path;
5. fixed-point branch drain terminates and is idempotent;
6. durable metadata-only promotion does not rewrite table bytes or lose
   reachability;
7. after manual flush plus explicit compact drain, point reads do not probe L0
   in quiescent benchmark loads;
8. remaining point-read performance gaps are measured separately from compaction
   shape.

## Non-Goals

Do not use this test plan to validate:

1. automatic maintenance scheduling;
2. score-based compaction picking;
3. background queue policy;
4. durable Bloom/filter table bytes;
5. public API redesign;
6. query/index/product behavior;
7. benchmark-only shortcuts.

## Test Targets

Primary files expected to gain or change tests:

1. `crates/storage-next/src/branch/tests/owned_compaction.rs`
2. `crates/storage-next/src/lifecycle/tests/compaction/`
3. `crates/storage-next/src/lifecycle/tests/durable.rs`
4. `crates/storage-next/src/lifecycle/tests/flush.rs`
5. `crates/storage-next/src/testkit/branch_lsm/`
6. `crates/storage-next/tests/branch_lsm_properties.rs`
7. `crates/storage-next/tests/branch_lsm_closeout.rs`
8. `benchmarks/src/bin/storage-next-l9-scale.rs` only if benchmark invocation
   needs to expose explicit compact-drain timing or counters.

Tests may use existing perf/source counters, but only tests that assert
mechanical source shape should require the `perf-trace` feature.

## Coverage Matrix

| Slice | Required proof | Failure caught |
| --- | --- | --- |
| L6J-A | Default config exposes the accepted level count, and custom smaller configs still reject out-of-range compaction. | The code silently tests L0-L6 while the benchmark expects L0-L7. |
| L6J-B | Single non-overlapping nonzero input promotes to the next level without rewriting bytes. | Manual compaction gets stuck at L1+ when overlap count is zero. |
| L6J-C | Single L0 input drains to L1, with rewrite on overlap and promotion on no-overlap. | Manual flush can leave one L0 table forever. |
| L6J-D | Branch drain reaches fixed point, returns stable summary facts, and is idempotent. | Explicit compact performs only one local step or loops indefinitely. |
| L6J-E | Branch-scoped explicit compact drains all eligible levels; table-level queued compact remains single-level work. | L6 manual closeout accidentally changes L8 queue semantics. |
| L6J-F | Durable promotion updates manifest level facts and restart state without republishing table bytes. | Metadata-only moves break reachability, recovery, or reclaim safety. |
| L6J-G | Benchmarks run only after source layout passes; point reads are level-count bounded. | Throughput is interpreted while compaction shape is still broken. |

## Shared Test Fixtures

Use small deterministic table fixtures with physical-key ranges that make overlap
intent obvious:

1. `a..c`, `d..f`, `g..i` for non-overlapping ranges;
2. `b..e` and `c..h` for overlapping ranges;
3. repeated physical keys with multiple commit versions for MVCC checks;
4. tombstone rows before and after visible puts;
5. TTL-bearing rows where existing L6 visibility rules are already documented;
6. inherited materialization source metadata where promotion must preserve
   source facts.

For each fixture, assert both:

1. source layout: table counts by LSM level;
2. read behavior: latest, bounded version read, history where relevant, and
   range/prefix scan correctness.

## Config Tests

Add branch-runtime config tests:

1. Default config reports the accepted number of LSM levels.
2. If the accepted target is L0-L7, default `max_level_count()` returns `8`.
3. A custom two-level config permits L0 and L1 but rejects compacting L1 further.
4. A custom three-level config permits L0 -> L1 and L1 -> L2.
5. Config validation still rejects zero levels.
6. Manifest/recovery helpers preserve explicit level indexes when the recovered
   manifest includes the accepted last level.

Assertions:

1. no test relies on an implicit default if it is testing small-level behavior;
2. source-layout reports include the accepted last level when populated;
3. out-of-range compaction reports a typed no-candidate or typed invalid request,
   not a panic.

## Branch-State Compaction Tests

### L0 Drain Cases

1. One L0 table, empty L1:
   - explicit drain moves the table to L1;
   - L0 count becomes zero;
   - identity and table facts are preserved;
   - reads before and after drain are identical.
2. One L0 table, non-overlapping L1:
   - L0 table moves to L1;
   - existing L1 table remains installed;
   - resulting L1 tables are sorted by physical-key range;
   - L1 ranges remain non-overlapping.
3. One L0 table, overlapping L1:
   - compaction rewrites the L0 table with overlapping L1 table(s);
   - only overlapping L1 tables are removed;
   - non-overlapping L1 tables remain;
   - newest visible row wins for overlapping keys.
4. Multiple L0 tables:
   - all snapshot L0 inputs participate in the rewrite;
   - overlapping L1 tables participate;
   - L0 count becomes zero for a quiescent branch;
   - output validates as sorted non-overlapping L1.
5. Newer L0 precedence:
   - multiple L0 inputs with same physical key preserve current newest-source
     semantics after drain;
   - history retains expected versions and tombstones.

### Nonzero Promotion Cases

1. One L1 table, empty L2:
   - promotion moves it to L2;
   - identity, facts, rows, and materialization source are unchanged;
   - no new table artifact is built.
2. One L1 table, non-overlapping L2:
   - promotion inserts in sorted position;
   - L2 range invariant holds.
3. One L1 table, overlapping L2:
   - rewrite compaction runs;
   - overlapping L2 inputs are removed;
   - output level is L2;
   - visibility and history match the pre-compaction model.
4. Multiple L1 tables:
   - one selected table is compacted or promoted per single-level operation;
   - fixed-point drain eventually handles every eligible table.
5. Repeated promotion:
   - a table can move L1 -> L2 -> ... -> configured last level;
   - last-level compaction returns no candidate.

### Identity And Reachability Cases

1. Promotion may reuse an input identity only if that identity belongs to the
   removed candidate input.
2. Promotion rejects reuse if the same identity remains reachable elsewhere.
3. Rewrite outputs must still use fresh identities and reject collisions.
4. Branch reachability snapshots do not report promoted tables as orphaned.
5. Protected table facts remain correct after promotion and rewrite compaction.

### Invariant Cases

1. Nonzero levels are sorted after promotion.
2. Nonzero physical ranges do not overlap after promotion.
3. Duplicate internal keys across owned levels are rejected.
4. Materialization-source metadata survives promotion.
5. `BranchCompactionNoopReason::LastLevel` is preserved at the configured last
   level.
6. Empty input levels remain no-candidate.
7. Invalid table indexes remain typed errors.

## Fixed-Point Drain Tests

Add lifecycle-level tests for branch drain:

1. Empty branch:
   - drain completes;
   - zero operations installed;
   - source layout unchanged.
2. L0-only branch:
   - drain removes all L0 tables;
   - table data reaches the configured last level or documented stable target;
   - repeated drain installs zero additional operations.
3. Mixed L0/L1/L2 branch with overlaps:
   - drain rewrites where ranges overlap;
   - drain promotes where ranges do not overlap;
   - final nonzero levels satisfy sorted/non-overlap invariants.
4. Multi-pass cascade:
   - first pass creates next-level work;
   - later passes continue until no candidate remains;
   - summary records multiple levels touched.
5. Progress guard:
   - inject or simulate an operation that returns installed without topology
     change;
   - drain returns typed failure instead of looping.
6. Pass-limit guard:
   - configure a small test-only pass limit;
   - drain returns typed failure when exceeded.
7. Idempotence:
   - run drain twice;
   - second run does not rewrite, promote, or alter source layout.

Required summary assertions:

1. attempted operations are greater than or equal to installed operations;
2. installed operations are nonzero for a branch with eligible work;
3. final L0 count is zero for quiescent work;
4. final source layout equals the branch state's observed layout;
5. no level beyond the configured last level is touched.

## Manual Boundary Tests

Add API/runtime tests for explicit maintenance:

1. Branch-scoped `MaintenanceTask::Compact` runs the fixed-point drain.
2. Branch-scoped compact after manual flush drains flushed L0 output.
3. Repeated branch-scoped compact is idempotent.
4. Table-level queued compaction still runs a single-level operation.
5. Existing queued compaction task for level `0` still maps to L0 -> L1 work.
6. Existing queued compaction task for level `N > 0` still maps to `N -> N+1`.
7. Explicit compact on a closed runtime returns the existing invalid-runtime
   error class.
8. Explicit compact on cache and durable runtimes returns equivalent source
   layout facts.

Assertions:

1. branch-scoped explicit compact does not enqueue background work;
2. queued maintenance semantics do not become fixed-point drain semantics;
3. manual flush remains a separate operation;
4. the temporary flush follow-up hook is not the only path that proves drain
   behavior.

## Durable Tests

Durable promotion tests:

1. Publish an immutable table, install it at L1, promote it to L2:
   - table bytes are not rewritten;
   - table identity is unchanged;
   - manifest records level L2 after promotion;
   - restart opens the table from L2.
2. Promote a table repeatedly to the configured last level:
   - each restart sees the latest level;
   - table object remains reachable.
3. Promotion plus non-overlapping existing next-level table:
   - manifest stores both tables in sorted order;
   - restart preserves sorted order.
4. Promotion failure before manifest update:
   - reads remain from old level;
   - no orphan object appears.
5. Manifest failure after branch-state update:
   - existing durable debt path is used consistently;
   - no input table is reclaimed without durable proof.

Durable rewrite tests:

1. L0 overlap rewrite still publishes output before install.
2. Nonzero overlap rewrite still publishes output before install.
3. Reopen failure leaves reads unchanged.
4. Manifest publication failure leaves reads unchanged or records existing
   uncertain debt.
5. Old inputs become reclaim candidates only after manifest proof.

Recovery tests:

1. Restart after fixed-point drain restores the final level layout.
2. Restart after partial durable failure follows the existing recovery/debt
   contract.
3. Table-manifest reachability excludes removed inputs and includes promoted
   identities at their new levels.

## Read Correctness Tests

For each branch-state and lifecycle shape above, assert:

1. latest read before and after drain returns the same visible row;
2. version-bounded read before and after drain returns the same visible row;
3. timestamp-bounded read follows the current timestamp contract;
4. history for a physical key includes the same versions and tombstones according
   to current L6 rules;
5. prefix/range scans return the same visible rows in the same order;
6. inherited rows remain rewritten to the child branch key where applicable;
7. child-local tombstones still hide inherited rows;
8. TTL behavior remains the currently documented L6 behavior.

Use an independent model for generated tests instead of copying branch-state
results into expectations.

## Source-Shape And Perf-Trace Tests

Only use `perf-trace` where the test is explicitly mechanical.

Required source-shape assertions:

1. After explicit drain, owned L0 table count is zero for quiescent inputs.
2. Point reads over compacted data have `point_owned_l0_table_probes == 0`.
3. `point_owned_nonzero_table_probes <= point_owned_nonzero_level_searches`.
4. `point_inherited_nonzero_table_probes <= point_inherited_nonzero_level_searches`
   for inherited compacted shapes.
5. Table seeks are bounded by active/frozen/L0 plus configured nonzero levels,
   not total table count.
6. Rows visited are bounded by the selected table's key/version chain, not total
   retained rows.

Required drain-shape assertions:

1. drain summary final layout matches diagnostics source layout;
2. levels touched never include the configured last level as input;
3. operations installed equals the number of topology-changing promotions or
   rewrites;
4. repeated drain reports zero installed operations.

## Generated And Fuzz Tests

Extend branch LSM generated workloads with:

1. random flush batches creating overlapping L0 tables;
2. random non-overlapping key ranges across L1+;
3. repeated explicit compact drain;
4. random latest/version/history/range queries before and after drain;
5. random custom configured level counts from two through the accepted default;
6. tombstone and put/delete/put sequences;
7. optional inherited source layers after compacted parent data.

Generated invariants:

1. model-visible reads are unchanged by compaction drain;
2. L0 is empty after drain for quiescent generated inputs;
3. nonzero levels remain sorted and non-overlapping;
4. no table identity is reachable from two levels;
5. drain is idempotent;
6. drain does not touch durable table-format bytes.

Fuzz routing:

1. Keep table-format fuzz unchanged unless a separate L3 format amendment is
   accepted.
2. If generated tests uncover a durable byte-format need, stop and open the
   table-format plan instead of patching L6.

## Source Guards

Add or update guards so this slice does not cross boundaries:

1. no durable Bloom/filter block serialization;
2. no backend object naming in branch-state code;
3. no benchmark-only branch compaction path;
4. no automatic scheduling policy in the fixed-point drain helper;
5. no public API DTO wording inside branch/lifecycle internals;
6. no roadmap labels in Rust identifiers, comments, fixture bytes, panic
   messages, or user-visible strings.

## Benchmark Gates

Benchmarks run only after focused tests pass.

Run order:

1. 100K smoke with manual flush plus explicit compact drain;
2. 1M scale with the same maintenance sequence;
3. 5M scale after 1M source-shape counters pass;
4. 10M scale after 5M source-shape counters pass.

Required benchmark facts:

1. git revision and feature set;
2. scale, workload, value size, samples, and flush cadence;
3. explicit compact drain timing and operation count;
4. final source layout by branch;
5. final L0 table count;
6. nonzero table counts by level;
7. point-read source counters;
8. table seek, row-visited, data-block read, and data-block decode counters when
   available;
9. cache hit/miss and Bloom/filter counters when available.

Fail-fast benchmark conditions:

1. final L0 table count is nonzero for a quiescent load;
2. point reads still probe L0 after drain;
3. nonzero table probes scale with table count;
4. drain pass count grows without bound;
5. throughput is interpreted before source-shape counters are clean.

## Validation Commands

Focused commands:

```sh
cargo fmt --manifest-path crates/storage-next/Cargo.toml
cargo check -p strata-storage-next --all-features
cargo test -p strata-storage-next branch::tests::owned_compaction --all-features
cargo test -p strata-storage-next lifecycle::tests::compaction --all-features
cargo test -p strata-storage-next branch_lsm --all-features
```

Perf-trace focused commands:

```sh
cargo test -p strata-storage-next --features perf-trace branch::tests::point_pruning
cargo test -p strata-storage-next --features perf-trace branch_lsm
```

Full storage-next gate before benchmark:

```sh
cargo test -p strata-storage-next --all-features
```

Benchmark invocation must include manual flush and explicit compact drain. If the
benchmark binary cannot express that sequence, update benchmark plumbing before
using its results as evidence.

## Exit Criteria

L6J is complete only when:

1. the accepted configured level target is explicit and tested;
2. explicit branch compact drains L0 through the configured levels;
3. single-table no-overlap cases promote instead of no-oping;
4. overlap cases rewrite and preserve read correctness;
5. fixed-point drain terminates and is idempotent;
6. durable promotion survives restart without rewriting table bytes;
7. focused unit, lifecycle, durable, generated, and source-shape tests pass;
8. 100K, 1M, 5M, and 10M benchmark runs show clean compaction source shape;
9. remaining point-read performance gaps are separately classified.

## Stop Conditions

Stop and re-plan if:

1. L0-L6 versus L0-L7 target remains unresolved;
2. metadata-only promotion cannot be made durable-safe;
3. promotion requires a table-format change;
4. fixed-point drain changes queued maintenance semantics;
5. explicit compact cannot empty L0 on quiescent inputs;
6. repeated drain is not idempotent;
7. source-shape counters still scale by table count after final compaction;
8. benchmark results imply a Bloom/filter issue before compaction shape passes.
