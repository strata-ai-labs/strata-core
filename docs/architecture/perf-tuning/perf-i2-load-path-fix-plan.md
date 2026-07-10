# PERF-I2 Measured Load Path Fix Plan

## Goal

Restore storage-next load throughput by correcting the measured hot path, not by
introducing speculative architecture. PERF-I2 is limited to the sequential blind
load path exercised through the L9 storage API.

The first implementation target is the full-branch clone inside
`BranchLocalState::append_committed_rows_atomically`. That clone is now proven by
both counters and an OS sample profile. Conflict read-view construction remains
unchanged until the append fix is implemented, measured, and profiled again.

## Current Evidence

100K sequential keys, cache mode, 1,000 mutations per batch, 150-byte values,
after reverting the speculative PERF-I2 implementation:

| Engine | Throughput | Elapsed |
| --- | ---: | ---: |
| storage-next cache | 38,030 ops/s | 2.63 s |
| old cache | about 477K-497K ops/s | about 200 ms |

Current storage-next load trace:

| Counter | Count |
| --- | ---: |
| commit batches | 100 |
| user mutation rows | 100,000 |
| timeline rows | 200 |
| prepared rows | 100,200 |
| append rows applied | 100,200 |
| branch fact rows observed | 4,959,900 |
| read views captured | 100 |
| read-view rows cloned | 4,959,900 |
| read-view validation rows scanned | 4,959,900 |
| append staging clones | 100 |
| append staging rows cloned | 4,959,900 |
| conflict sources built | 100 |

Profile artifacts:

1. `benchmarks/results/storage-next-l9/profiles/storage-next-cache-load-1m-sample-2026-06-04T18-35-59Z.txt`
2. `benchmarks/results/storage-old-cache/profiles/storage-old-cache-load-1m-sample-2026-06-04T18-36-51Z.txt`

Profile conclusion:

1. storage-next spends the sampled load hot path in:
   `StorageRuntime::commit -> LifecycleCacheRuntime::execute_cache_commit ->
   BudgetedCommitBranch::append_committed_rows_atomically ->
   BranchLocalState::append_committed_rows_atomically -> BranchLocalState::clone
   -> BTreeMap::clone -> TableRow::clone`.
2. old cache spends the analogous write path in:
   `SegmentedStore::apply_writes_atomic -> Memtable::put_entry`, mostly skiplist
   search, key encoding, and allocation.
3. The old path does not clone all prior rows per commit batch. storage-next
   currently does.

## Non-Goals

PERF-I2 must not:

1. add a secondary index;
2. change storage format;
3. change the public L9 API;
4. rework branch/lifecycle/commit architecture;
5. special-case the benchmark;
6. remove WAL ordering or visible-version safety fences;
7. combine unrelated load, point-read, scan, and compaction fixes;
8. add a new commit-only append API unless the existing append API cannot be
   fixed without weakening correctness.

## Architecture Rules

1. Keep the existing `append_committed_rows_atomically` boundary. The preferred
   implementation changes its internals from full-state clone staging to
   batch-local validation plus direct apply.
2. The method name must remain honest: if it returns an error, the branch state
   must be unchanged.
3. All fallible checks that can reject a batch must happen before mutation, or
   the apply step must have an O(batch) rollback guard.
4. Do not move branch identity, duplicate internal-key, or table ownership rules
   into commit executors.
5. Do not optimize conflict read-view construction in the same patch as append
   clone removal. Measure after the append fix first.
6. Durable commits must preserve ordering:
   branch admission and pre-apply validation before WAL append, branch apply
   after successful WAL append, visible publication after branch apply.
7. Cache commits must not introduce partial mutation on validation failure.

## Implementation Steps

### PERF-I2A: Append Atomicity Audit

No production behavior change.

Files:

1. `crates/storage-next/src/branch/state/append.rs`
2. `crates/storage-next/src/table/mutable.rs`
3. `crates/storage-next/src/branch/state/mod.rs`

Work:

1. Enumerate every failure path in `append_committed_row`:
   branch mismatch, duplicate internal key, table duplicate, and invalid batch.
2. Enumerate every metadata field updated during append:
   active table rows, active approximate bytes, table min/max commit,
   branch max commit version, timestamp coverage, tombstone/delete facts.
3. Decide whether direct apply can be made infallible after validation. If not,
   define the O(batch) rollback data needed before implementation.
4. Add a short comment near the atomic append helper documenting why the helper
   is allowed to avoid full-state clone after validation.

Exit gates:

1. No change to benchmark results is expected.
2. The audit identifies a concrete implementation shape for PERF-I2B/I2C.
3. The implementation shape preserves "no mutation on returned error."

### PERF-I2B: Batch-Local Append Validation

Add a branch-owned validation helper for a full incoming row batch. This is not a
new commit executor API; it is internal branch validation used by the existing
atomic append helper.

Files:

1. `crates/storage-next/src/branch/state/append.rs`
2. `crates/storage-next/src/branch/tests/identity_state.rs`

Work:

1. Validate that the row batch is non-empty.
2. Validate every row belongs to the target branch.
3. Build incoming internal keys once.
4. Reject duplicate internal keys within the incoming batch.
5. Reject incoming keys already present in active, frozen, or owned
   branch-local sources using the existing branch duplicate check. Do not reject
   inherited parent rows: current append semantics allow child rows to shadow
   inherited data through normal branch read resolution.
6. Keep payload inspection out of validation; compare branch ids and internal
   keys only.

Correctness fences:

1. The helper must reject every invalid case that `append_committed_row` rejects
   before any mutation happens.
2. Tests must prove wrong-branch, empty batch, existing duplicate, and
   duplicate-within-batch leave branch row count and max commit version
   unchanged.
3. Do not change single-row `append_committed_row` behavior.

Exit gates:

1. Branch identity/duplicate tests pass.
2. No commit executor behavior changes yet.

### PERF-I2C: O(batch) Atomic Append Apply

Replace full-branch clone staging inside `append_committed_rows_atomically` with
batch-local validation plus direct apply.

Files:

1. `crates/storage-next/src/branch/state/append.rs`
2. `crates/storage-next/src/table/mutable.rs` only if an infallible or rollback
   table insertion primitive is required.
3. `crates/storage-next/src/branch/tests/identity_state.rs`

Preferred implementation:

1. `append_committed_rows_atomically` collects the incoming rows.
2. It validates the batch using PERF-I2B.
3. It snapshots only O(1) branch metadata needed to restore scalar facts if an
   unexpected apply error is still possible.
4. It appends incoming rows directly to the active mutable table.
5. It updates branch commit/timestamp/tombstone facts exactly as
   `append_committed_row` does.
6. It returns the same `BranchAppendBatchOutcome` shape as today.

If direct insertion can still return an error after validation:

1. either make the post-validation table insert infallible by construction; or
2. add an O(batch) rollback guard that removes only rows inserted by this batch
   and restores O(1) branch metadata.

Forbidden implementation:

1. no full `BranchLocalState` clone;
2. no active table clone;
3. no executor-level duplicate checking;
4. no new `append_prevalidated_*` commit API unless the existing helper cannot
   be fixed safely.

Exit gates:

1. `append_staging_clones = 0` for 100K cache load.
2. `append_staging_rows_cloned = 0` for 100K cache load.
3. `append_rows_applied = 100,200` for 100K cache load.
4. Existing branch duplicate tests pass.
5. Existing cache and durable commit tests pass.

### PERF-I2D: Post-Append-Fix Measurement

Run the exact same load benchmark and collect another profile before changing
conflict read-view construction.

Commands:

```sh
cargo fmt --all -- --check
cargo check -p strata-storage-next --features perf-trace
cargo test -p strata-storage-next --lib --features perf-trace branch_local_state_rejects_active_and_frozen_duplicates_without_mutation
cargo test -p strata-storage-next --lib --features perf-trace conflict
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache,standard --workloads load-seq --samples 1000 --value-bytes 150
```

Profile command:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale
mkdir -p benchmarks/results/storage-next-l9/profiles
```

Then run a long-enough cache load and attach `/usr/bin/sample` for a bounded
window. The sampled benchmark may be terminated after profile capture; record
that it is a profile run, not a completed throughput run.

Decision gate:

1. If append clone counters are still non-zero, PERF-I2C is incomplete.
2. If append clone counters are zero and throughput is close to old cache for
   100K, stop PERF-I2. Do not implement conflict-source changes.
3. If append clone counters are zero but storage-next remains materially slower,
   inspect the new profile. Only proceed to PERF-I2E if read-view construction
   is now a top measured bottleneck.

### PERF-I2E: Conditional Blind Conflict Read-View Avoidance

This step is conditional. Do not implement it until PERF-I2D proves the append
clone is gone and read-view construction remains a hot path.

Files:

1. `crates/storage-next/src/commit/conflict.rs`
2. `crates/storage-next/src/commit/cache.rs`
3. `crates/storage-next/src/commit/durable.rs`
4. `crates/storage-next/src/commit/tests/cache.rs`
5. `crates/storage-next/src/commit/tests/durable.rs`

Work:

1. Add a commit-level predicate that answers whether conflict validation will
   read from a branch source.
2. It may return false only for:
   - mutating batches with empty read-set and empty CAS-set;
   - validation mode `Skip`;
   - read-only batches already handled by existing read-only paths.
3. Keep source-backed validation unchanged for read-set and CAS batches.
4. Capture `BranchReadView` only when the predicate says the source may be read.
5. Keep conflict-source counter semantics tied to sources actually built.

Correctness fences:

1. Existing read-set and CAS conflict tests pass unchanged.
2. New tests prove blind cache and durable commits do not capture read views.
3. New tests prove read-set and CAS commits still capture read views and reject
   stale facts.
4. Durable pre-WAL validation ordering remains unchanged.

Exit gates:

1. For 100K blind cache load:
   `read_view_captures = 0`, `read_view_rows_cloned = 0`,
   `read_view_validation_rows_scanned = 0`, and `conflict_sources_built = 0`.
2. A new profile shows the old append/read-view clone hot stacks are gone.

### PERF-I2F: Final Load Comparison

Run completed throughput benchmarks after the last implemented step.

Commands:

```sh
cargo fmt --all -- --check
cargo check -p strata-storage-next --features perf-trace
cargo test -p strata-storage-next --features perf-trace
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache,standard --workloads load-seq --samples 1000 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- --scales 100k --workloads load-seq --samples 1000 --value-bytes 150
```

Success criteria:

1. storage-next load work is O(new rows per batch), not O(accumulated rows per
   batch);
2. storage-next 100K cache load is within a small constant factor of old cache;
3. cache and standard modes show the same counter shape;
4. no branch, commit, lifecycle, or durable safety tests regress;
5. any remaining throughput gap has a fresh profile attached before another
   implementation plan is written.

## Review Checklist

Before merging any PERF-I2 implementation patch:

1. Is the patch limited to the step being implemented?
2. Does it preserve the existing public API and commit/lifecycle boundaries?
3. Does `append_committed_rows_atomically` still leave branch state unchanged on
   returned error?
4. Are counters proving row work is no longer proportional to accumulated branch
   size?
5. Was a post-change profile captured before implementing the next step?
