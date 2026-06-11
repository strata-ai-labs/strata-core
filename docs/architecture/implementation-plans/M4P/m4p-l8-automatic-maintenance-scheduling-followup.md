# L8 Follow-Up: Automatic Maintenance Scheduling And Compaction Prep

Status: follow-up required before L9 scale benchmark closeout

Related docs:

1. `docs/architecture/implementation-plans/M4P/README.md`
2. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-test-plan.md`
4. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
5. `docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4P/m4p-l6l-branch-read-hot-path-implementation-plan.md`

## Why This Exists

During the L6 read-path closeout, the broad storage-next L9 scale benchmark was
rerun after the eager-filter work:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m \
  --engines cache,standard \
  --workloads load-seq,point-latest,point-throughput \
  --value-bytes 150 \
  --batch-size 1000 \
  --flush-every 100000 \
  --samples 1000 \
  --progress
```

The 100K and 1M point-read sections completed. The 5M cache run completed the
load phase, then spent more than ten minutes CPU-bound in final source-shape
preparation before point reads could start. The run was interrupted before a
JSON report was written.

This is not primarily an L6 point-read problem. The completed point-read
counters showed:

1. one active probe per read;
2. one owned nonzero-level table probe per read;
3. no data-block reads or decodes during point reads;
4. one Bloom filter probe per table point read;
5. no inherited-layer work in the benchmark shape.

The immediate 5M blocker is the explicit final fixed-point compaction drain used
to prepare a stable source shape after load. Proper L8 parity should avoid a
large post-load compaction cliff by scheduling and draining flush/compaction
work during sustained writes.

## Benchmark Evidence

Comparable previous result:
`benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-09T18-59-49Z-6800a24d.json`

New partial run, after `3b73af94 Add eager filters to table point reads`:

| Scale | Engine | Workload | Previous | New partial run | Notes |
|---|---|---|---:|---:|---|
| 100K | cache | point p50 | 2.33 us | 2.04 us | Improved. |
| 100K | cache | point throughput | 405K/s | 437K/s | Improved. |
| 100K | standard | point p50 | 2.00 us | 2.12 us | Slight regression. |
| 100K | standard | point throughput | 501K/s | 468K/s | Regression. |
| 1M | cache | point p50 | 5.46 us | 5.54 us | Flat/slight regression. |
| 1M | cache | point throughput | 169K/s | 186K/s | Improved. |
| 1M | standard | point p50 | 4.38 us | 4.88 us | Regression. |
| 1M | standard | point throughput | 247K/s | 198K/s | Regression. |
| 5M | cache | load-seq | n/a | 26.2K/s | Load completed. |
| 5M | cache | final prep | n/a | interrupted after >10m CPU-bound | Did not reach point reads. |

The 5M cache load phase reported:

1. `load-seq` elapsed about `190.76s`;
2. maintenance during load took about `185.67s`;
3. `table_compaction_merge_advances=31,262,400`;
4. `table_compaction_output_tables_built=180`;
5. `table_data_block_decodes=141,812`;
6. final fixed-point compaction after load did not complete within the allowed
   manual observation window.

## Ownership

This belongs to L8 scheduling and admission policy, not to the L6 read-path
cleanup:

1. L6 owns branch-local LSM mechanics, compaction candidates, and install
   invariants.
2. L5 owns table merge/build mechanics and compaction hot-loop efficiency.
3. L8 owns when maintenance is scheduled, how queued work is coalesced, how much
   work is drained, and whether write admission should drive or wait for
   maintenance.
4. L9 benchmarks are proof gates. They should not carry benchmark-specific
   manual maintenance scripts as the only way to obtain a healthy LSM shape.

L8K V1 is not enough. It added conservative compaction/materialization hooks and
pressure facts, but explicitly deferred background scheduling threads and the
old score/resubmit compaction behavior.

## Required L8 Work

### L8A: Automatic Maintenance Scheduling

After mutating commit completion, L8 should inspect storage pressure facts and
enqueue the required maintenance work without L9 benchmark-specific calls.

Required behavior:

1. coalesce repeated flush, compaction, and materialization tasks by branch and
   task scope;
2. schedule flush before compaction when frozen tables exist;
3. schedule compaction when L0 or nonzero-level pressure exceeds threshold;
4. schedule materialization when inherited-layer pressure exceeds threshold;
5. avoid duplicate in-flight chains for the same branch/scope;
6. expose counters for scheduled, coalesced, completed, deferred, failed, and
   resubmitted tasks.

### L8B: Score-Based Compaction Drain

The scheduler needs old-style score selection, not a benchmark-only fixed-point
drain.

Required behavior:

1. compute compaction pressure across branches and levels;
2. pick the highest-scoring unit of work;
3. run one compaction task;
4. re-read facts, re-score, and resubmit while the level structure remains
   unhealthy;
5. use L6 compaction candidates rather than selecting rows in L8;
6. do not map every nonzero level to table index 0 indefinitely;
7. report selected level, input/output table counts, score before/after, and
   post-drain L0/L1+ shape.

### L8C: Write Admission And Pressure Policy

L8 should decide what happens when maintenance falls behind.

Required behavior:

1. consult L6/L8 pressure facts before accepting mutating commits;
2. drive maintenance synchronously or asynchronously according to severity;
3. slow, stall, or reject only with typed storage errors and storage vocabulary;
4. document any intentional no-stall policy with a bounded-fanout proof;
5. wake or unblock writers after compaction makes progress if blocking is used.

Commit-runtime fact handoff:

1. `CommitAdmissionPressureFacts` reports mutation count, put/delete counts,
   approximate commit bytes, whether the commit is above configured pressure
   thresholds, and whether the commit would merit maintenance before
   admission.
2. Perf-trace exposes `commit_admission_pressure_facts`,
   `commit_admission_under_pressure`,
   `commit_admission_accepted_under_pressure`,
   `commit_admission_requires_maintenance`,
   `commit_admission_mutations`, and `commit_admission_approx_bytes`.
3. Retryable admission failures remain distinguishable through existing commit
   gate and branch-guard counters:
   `commit_unresolved_gate_rejected_unresolved`,
   `commit_unresolved_gate_rejected_active`, and
   `commit_branch_guard_rejected`.
4. L8 should consume these facts to select enqueue/drive/stall/reject policy;
   commit runtime must continue to avoid sleeps, waits, flushes, compactions,
   and scheduler calls.

## Non-Goals

Do not fix this by:

1. adding benchmark-only maintenance shortcuts;
2. changing L9 API semantics;
3. moving branch candidate selection into L8;
4. reimplementing L5 merge logic in L8;
5. hiding the fixed-point drain cost by increasing benchmark timeout;
6. using product write-stall wording below L9.

The fixed-point drain helper should remain available for explicit diagnostics
and closeout tests, but it should not be the normal steady-state serving shape
mechanism.

## Test Plan Reminder

L8 tests should prove:

1. sustained commits enqueue flush work automatically;
2. flush scheduling drains all currently eligible frozen state for a branch;
3. compaction scheduling chains until L0 and nonzero-level shape is healthy;
4. scheduler re-scores after each compaction instead of blindly draining every
   level;
5. duplicate task enqueue coalesces;
6. write admission sees urgent/blocking pressure and drives or rejects work by
   documented policy;
7. read results are unchanged before and after scheduled maintenance;
8. no source guard permits product modules, benchmark-only flags, or public UX
   wording in L8 maintenance code.

Generated or long-running tests should include:

1. random sustained write streams with automatic rotation/flush/compaction;
2. repeated pressure oscillation around L0 thresholds;
3. nonzero-level pressure with multiple candidate tables;
4. multi-branch pressure where one branch should not starve another;
5. failure/defer paths where a scheduled task cannot run and must be retried or
   reported without corrupting branch state.

## Benchmark Exit Gate

Before L8 is considered ready for L9 scale closeout:

1. run the storage-next L9 benchmark at 100K, 1M, 5M, and 10M without
   benchmark-specific manual flush/compact scripts;
2. source-shape diagnostics after load must show bounded L0 and bounded nonzero
   fanout;
3. 5M and 10M must reach point-read measurement without a large final
   fixed-point compaction cliff;
4. point-read metrics should be interpreted only after the source shape passes;
5. explicit fixed-point drain timing should be reported separately from
   steady-state point-read throughput.

## Implementation Notes For Future L8 Work

Start from:

1. `crates/storage-next/src/lifecycle/maintenance.rs`
2. `crates/storage-next/src/lifecycle/compaction.rs`
3. `crates/storage-next/src/lifecycle/cache.rs`
4. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
5. `crates/storage-next/src/lifecycle/pressure.rs`
6. `crates/storage-next/src/branch/state/compaction.rs`
7. `crates/storage-next/src/lifecycle/compaction.rs::compaction_request_from_maintenance_task`

Specific code review targets:

1. ensure commit completion has a path to enqueue pressure-suggested work;
2. ensure queued maintenance can run enough work to bound source fanout;
3. replace any fixed table-index selection policy for nonzero levels with
   score/candidate-driven selection;
4. separate explicit diagnostic drains from automatic steady-state maintenance;
5. keep all row merge and branch visibility semantics delegated to L5/L6.
