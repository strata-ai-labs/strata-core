# Storage-Next Performance Proof Plan

## Purpose

Restore storage-next serving-path performance to match the old storage engine's
implementation model while keeping the L9 public storage APIs. The current
100K-key benchmark results show a systemic regression in both cache and
standard modes, but the active work in this plan is proof and measurement only.
No serving-path correction is approved until the proof phase identifies the
dominant cost center and shows that the proposed correction should materially
move the benchmark.

This is not an index-building project. The old engine already had the right
shape: ordered internal keys, pinned read snapshots, blind-write fast paths, and
lazy merge scans. Storage-next should recover those mechanics only where
measurement proves they were lost and are responsible for the regression.

This also is not another rearchitecture. The goal is deliberate performance
tuning: measure, prove cause, make the smallest correction, rerun benchmarks,
and stop if the data does not support the next change.

## Baseline

The working baseline from recent local runs:

| Engine | Mode | Scale | Load | Point latest | Range scan |
| --- | --- | ---: | ---: | ---: | ---: |
| old storage | cache | 100K | ~493K ops/s | ~583K ops/s | ~88K ops/s |
| storage-next | cache | 100K | ~35K ops/s | ~45 ops/s | ~22 ops/s |
| storage-next | standard | 100K | ~35K ops/s | ~48 ops/s | ~22 ops/s |

The standard-mode result confirms the regression is not caused by cache-mode
divergence alone. The likely shared causes are branch snapshot cloning,
full-source scans for point reads, scan limit application too late, and blind
commit setup work that the old engine skipped.

## Non-Goals

1. Do not add a new secondary index.
2. Do not change L9 API semantics to win benchmark numbers.
3. Do not change durable table or manifest formats unless a later slice proves
   it is required.
4. Do not special-case cache mode in a way that makes it a different engine.
5. Do not remove correctness tests or weaken MVCC, tombstone, TTL, inherited
   branch, or snapshot-isolation behavior.
6. Do not begin broad branch/table/lifecycle rewrites before the proof phase has
   established the dominant regression source.
7. Do not treat the candidate correction slices below as an approved execution
   backlog.

## Old Invariants To Restore

1. Internal keys are ordered by physical key ascending and commit/version
   descending, so newest visible rows for one key can be reached by seek and
   bounded walk.
2. Read snapshots pin shared state with `Arc`-like ownership instead of cloning
   whole mutable or immutable tables.
3. Blind writes with no read set and no CAS set skip conflict validation source
   construction.
4. Point reads probe each source with keyed lookup, not `iter().filter(...)`
   over all rows.
5. Range and prefix scans are lazy merge/MVCC walks with limit pushdown.
6. Cache and durable modes share the same serving-path algorithms; durable mode
   adds persistence, WAL, recovery, and object services around the same logic.

## Active Scope

Only `PERF-P0` is active for now. The `PERF-T*` items are candidate corrections
kept below so we know what the likely fixes are, but they are deferred until
`PERF-P0` produces evidence and a specific go/no-go decision.

Any implementation work after `PERF-P0` must be promoted one slice at a time,
with a short decision note that states:

1. the measured cause;
2. the expected performance effect;
3. the smallest code path that needs to change;
4. the correctness risks;
5. the benchmark and test gates for that exact change.

## Proof Slices

### PERF-P0A: Counter Audit

Identify the smallest set of counters needed to prove whether storage-next is
doing materially more work than the old engine on the hot paths.

Counters to confirm or add:

1. branch read-view captures per operation;
2. rows cloned during read-view capture;
3. rows cloned or copied during append staging;
4. conflict-validation sources built for blind commits;
5. table rows visited during one point lookup;
6. table rows materialized during one limited scan;
7. table seeks performed during point and scan operations.

Exit criteria:

1. Every counter maps to one suspected regression mechanism.
2. Counters can run in release benchmarks without dominating runtime.
3. Counter output is attached to benchmark results for cache and standard modes.

### PERF-P0B: 100K Proof Run

Run the current implementation with counters before changing serving behavior.

Required runs:

1. storage-next cache at 100K keys;
2. storage-next standard at 100K keys;
3. old storage cache at 100K keys, where comparable counters or proxy metrics
   are available.

Exit criteria:

1. The proof run reports load, point latest, range scan, and close latency.
2. The proof run reports the hot-path counters from `PERF-P0A`.
3. The result identifies whether the point-read regression is linear row visits,
   branch/table cloning, another source, or still unknown.
4. The result identifies whether the scan regression is full materialization
   before limit, cursor overhead, another source, or still unknown.
5. The result identifies whether the load regression is validation setup,
   append staging, durable overhead, another source, or still unknown.

### PERF-P0C: Tiny Spike Comparisons

Build only benchmark-local or test-local spikes that compare current behavior
against the old mechanical shape without changing production serving semantics.

Allowed spikes:

1. current point lookup versus direct ordered-key seek on the same in-memory
   table data;
2. current limited scan versus bounded ordered cursor walk on the same table
   data;
3. current blind commit setup versus a dry-run path that skips validation source
   construction when validation is empty;
4. current append staging versus incoming-batch-only staging in a test-local
   harness.

Exit criteria:

1. Each spike isolates one mechanism and reports before/after work counts.
2. No spike becomes production behavior in this phase.
3. A spike must show a material expected gain before its matching correction
   slice can be promoted.

### PERF-P0D: Decision Report

Write a short report before any correction slice begins.

The report must include:

1. the measured 100K baseline;
2. the top one or two confirmed cost centers;
3. rejected hypotheses;
4. the single next correction slice to promote, if any;
5. the expected benchmark movement for that slice;
6. the stop condition if the slice does not move the benchmark.

Exit criteria:

1. There is no implementation work queued without a measured cause.
2. There is no multi-slice performance campaign approved as a bundle.
3. The next step is either one promoted correction slice or more measurement.

## Deferred Candidate Corrections

### PERF-T0: Benchmark And Profiling Harness

Capture the current regression in a repeatable harness before changing code.

Scope:

1. Keep the existing L9 benchmark binaries as the public measurement surface.
2. Add or update result capture so cache, standard, and old-cache runs can be
   compared from one command matrix.
3. Record 100K results as the starting ledger and add optional 1M runs for
   follow-up validation.
4. Add low-overhead counters or trace hooks only where they directly expose hot
   path behavior: branch read-view captures, table row iterations, table seeks,
   and append-state clone counts.

Exit criteria:

1. One benchmark command can produce comparable load, point, and scan metrics.
2. The ledger records machine, build profile, scale, value size, scan limit, and
   engine mode.
3. A profiling run identifies whether time is dominated by branch cloning,
   point-source scans, scan materialization, or commit setup.

### PERF-T1: Blind Commit Fast Path

Move the empty-validation fast path ahead of read-view capture in both cache and
durable commit runtimes.

Scope:

1. Detect empty read set and empty CAS set before constructing conflict sources.
2. Keep WAL and durability behavior unchanged in standard mode.
3. Preserve all existing commit-result facts and error-code behavior.
4. Add a test hook or targeted test proving a blind batch does not capture a
   branch read view for validation.

Exit criteria:

1. Blind 100K load no longer pays read-view capture cost per commit.
2. Conflict-validation tests still pass unchanged except for import/path edits.
3. Cache and standard modes both use the same fast-path decision.

### PERF-T2: Atomic Append Without Whole-Branch Clone

Replace clone-then-apply append staging with bounded staging over incoming rows.

Scope:

1. Audit `append_committed_rows_atomically` and related branch append helpers.
2. Stage only the incoming batch and any small rollback metadata required for
   atomicity.
3. Validate duplicate-row, memory-budget, branch-state, and table-rotation
   preconditions before mutating shared state.
4. Preserve the current all-or-nothing guarantee when any row in the batch is
   invalid.

Exit criteria:

1. Appending a batch does not clone existing branch tables.
2. Failure tests prove duplicate or budget errors leave branch state unchanged.
3. Load throughput improves independently of read-path changes.

### PERF-T3: Read Snapshot Pinning

Make `BranchReadView` pin shared branch/table state instead of owning deep
clones of active, frozen, owned, and inherited sources.

Scope:

1. Convert read views to hold shared immutable snapshots of table sources.
2. Keep active-table mutation isolated from pinned read snapshots, using
   copy-on-write or table rotation where needed.
3. Ensure branch rotations, materialization, and snapshot installs cannot mutate
   data visible through an already captured read view.
4. Keep snapshot lifetime explicit so durable object cleanup cannot reclaim
   sources still visible to readers.

Exit criteria:

1. Capturing a read view is proportional to source count, not row count.
2. A read view remains stable across later commits, rotations, and flushes.
3. No point read or scan clones full branch/table contents.

### PERF-T4: Point Read Seek Over Existing Internal Keys

Replace full-row filtering in point reads with bounded seeks over the existing
ordered internal-key storage.

Scope:

1. Add table helpers that seek to the newest internal key for a physical key and
   stop as soon as the physical key changes.
2. Apply the helper to active, frozen, owned, and inherited table sources.
3. Preserve MVCC visibility, tombstone, TTL, source precedence, and inherited
   branch rules.
4. Remove or quarantine point-read code paths that scan all rows looking for a
   key match.

Exit criteria:

1. Point latest is `O(source_count * (log rows + versions_for_key))`, not
   `O(total_rows)`.
2. Source scans for a single key are bounded by that key's version chain.
3. Tests cover latest, historical, tombstone, TTL-expired, frozen, owned, and
   inherited point reads.

### PERF-T5: Lazy Range Scan With Limit Pushdown

Restore the old lazy merge/MVCC scan shape and pass limits into the branch/table
layer instead of collecting first and truncating later.

Scope:

1. Audit table cursor implementations for hidden full-materialization.
2. Use ordered cursors from each source and merge by physical key/version.
3. Apply MVCC collapse, tombstone filtering, TTL filtering, and branch-source
   precedence during the walk.
4. Pass scan limits from the API/runtime layer into the branch scan call.
5. Stop reading once the requested visible rows have been produced.

Exit criteria:

1. Limit-10 scans do not collect the whole branch before truncation.
2. Range and prefix scan complexity is `O(source_count * log rows + limit +
   skipped_versions_for_returned_keys)`.
3. Tests cover active/frozen/owned/inherited merge order and limit boundaries.

### PERF-T6: Immutable Table Lazy Serving Parity

Audit immutable table reads so flushed/durable tables do not force full-object
materialization on point and bounded range reads.

Scope:

1. Compare storage-next immutable table readers against the old segment reader's
   bloom/index/block-cache model.
2. Preserve current durable object format for the first pass if possible.
3. If the current format cannot support lazy serving, document the exact format
   limitation before proposing a format change.
4. Ensure cache-mode tables and durable-mode tables expose the same seek/cursor
   interface to branch reads.

Exit criteria:

1. Opening a durable table reader does not decode every row for ordinary point
   or small scan operations.
2. Flushed-table point reads and range scans use the same branch-level hot path
   as in-memory tables.
3. Durable localfs tests cover post-flush serving behavior.

### PERF-T7: Cache And Standard Parity Matrix

Run the corrected paths across cache and standard modes before scaling up.

Scope:

1. Re-run 100K cache, 100K standard, and old-cache comparison after each hot-path
   slice.
2. Add 1M runs once 100K point and scan throughput are within an acceptable
   range.
3. Track load, point latest, point historical if available, range scan, memory
   growth, and close latency.

Exit criteria:

1. Cache and standard mode differ only by durability overhead for the same
   workload.
2. 100K point and range results are within the expected order of magnitude of
   the old engine on the same machine.
3. Any remaining gap is attributed to a measured cost center.

### PERF-T8: 10M And 100M Scale Gates

Only move to 10M and 100M once 100K and 1M have the correct asymptotic shape.

Scope:

1. Define disk, memory, runtime, and cleanup expectations before launching large
   runs.
2. Run standard mode first for persisted serving behavior.
3. Run cache mode only where memory limits make the run meaningful.
4. Record compaction, flush, maintenance, and recovery side effects separately
   from serving throughput.

Exit criteria:

1. 10M results do not show point or limit-scan throughput collapsing linearly
   with total key count.
2. 100M runs are used as capacity evidence, not as the first place to discover
   hot-path algorithm bugs.

## Proof Acceptance Gates

`PERF-P0` must satisfy these gates before any correction slice is promoted:

1. The 100K cache and standard benchmark runs include hot-path counters.
2. The decision report names the dominant cost center or says it is still
   unknown.
3. Any proposed correction has a measured cause and an expected benchmark
   movement.
4. Any spike used to justify a correction is benchmark-local or test-local only.
5. No production serving behavior changes in the proof phase except low-overhead
   measurement hooks.
6. The next step is one promoted correction slice or more measurement, never a
   bundled performance campaign.

## Correction Acceptance Gates

Every promoted correction slice must satisfy these gates:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test -p strata-storage-next`
4. `cargo test -p strata-storage-next --test format_goldens`
5. Any localfs-specific tests touched by the slice with `--features localfs`
6. 100K benchmark rerun for any slice that changes commit, read, scan, table, or
   branch snapshot code

Hot-path-specific gates:

1. No point-read path may scan all rows in a table source for one physical key.
2. No ordinary read-view capture may clone full table contents.
3. No blind commit may build conflict-validation sources before proving it needs
   them.
4. No limited scan may collect all visible rows before applying the limit.
5. No benchmark fix may depend on cache-only behavior that standard mode cannot
   share.

## Risks And Review Focus

1. Snapshot isolation is the main risk when replacing clones with pinned shared
   state. Tests must prove old read views remain stable after later writes.
2. Atomic append rollback is the main risk when removing whole-branch staging.
   Tests must prove every precondition failure leaves state unchanged.
3. MVCC collapse is the main risk when replacing collected scans with lazy merge
   cursors. Tests must cover tombstones, historical visibility, inherited
   layers, and source precedence.
4. Durable table laziness can accidentally become a format change. Keep the
   first slice adapter-shaped unless measurement proves format work is required.
5. Benchmark numbers depend on machine class. Use old-vs-new ratios on the same
   host as the primary signal, not absolute throughput alone.

## Deferred Promotion Order

This order is not active implementation scope. It is the order to consider only
after `PERF-P0D` promotes a specific correction based on measured evidence.

1. Promote `PERF-T1` or `PERF-T2` only if load-path counters prove blind commit
   setup or append staging dominates load time.
2. Promote `PERF-T3` only if read-view capture counters prove row-proportional
   cloning is material.
3. Promote `PERF-T4` only if point-read counters prove full-row scans or an
   equivalent linear path.
4. Promote `PERF-T5` only if scan counters prove full materialization before
   limit or another linear scan bottleneck.
5. Promote `PERF-T6` only if flushed/durable table serving is the measured
   blocker after in-memory paths are understood.
6. Run `PERF-T7` only after at least one correction has landed and needs cache
   versus standard parity confirmation.
7. Run `PERF-T8` only after 100K and 1M results show the correct asymptotic
   shape.
