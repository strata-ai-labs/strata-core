# M4P-L8H Test Plan: Durable Maintenance Liveness and Publish Decoupling

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8h-durable-maintenance-liveness-implementation-plan.md`

Parent test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-test-plan.md`
3. `docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-test-plan.md`

## Goal

Prove that durable-local mode survives sustained write load by bounded
backpressure, never by rejecting commits, and that the publish critical section
no longer does work proportional to the resident dataset (full-dataset fact
rescans or full manifest rebuilds) under the commit lock — without weakening
durability, recovery, or the deterministic simulation boundary.

The suite must fail if durable mode:

1. rejects a mutating commit while background maintenance is making progress;
2. performs work proportional to the resident dataset inside the runtime-locked
   publish critical section — rescanning owned-table rows to recompute branch or
   per-table facts, rebuilding the full manifest, or cloning the full catalog —
   or (once Group C completes) persists/fsyncs the manifest there;
3. truncates WAL or advances the flush watermark before the corresponding
   manifest entry is durable;
4. recovers to a state where an installed table pointer has neither a durable
   manifest entry nor replayable WAL;
5. hangs instead of surfacing a typed failure when the maintenance executor is
   genuinely dead;
6. preserves performance by adding a benchmark-only bypass, retry loop, or
   scale-specific path;
7. regresses cache-mode behavior established in L8G.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Admission liveness | A sustained durable overload completes; no caller-visible pressure rejection while maintenance progresses. | Backlog still converts to commit failure. |
| Liveness backstop | A dead/stuck executor surfaces a typed, bounded failure. | Removing the deadline introduces an unbounded hang. |
| L0 enqueue gap | The blocking-pressure wait path always has compaction enqueued/active before give-up. | `LevelZeroTableBacklog` gives up with no work scheduled. |
| Publish cost | Per-task lock-held publish is flat across scales (no full-dataset rescan/rebuild); manifest bytes and recovery byte-identical. | Publish still does O(resident-dataset) work under the commit lock. |
| Durability ordering | WAL truncation/flush watermark never outrun manifest durability. | A retired segment's data is neither durable in manifest nor in WAL. |
| Crash recovery | Mid-publish crash recovers via WAL replay. | Decoupling drops a not-yet-persisted table. |
| Throughput shape | Maintenance scales with worker count; admission ramps, not cliffs. | Single-flight drain and cliff backpressure persist. |
| Durable regression | Existing durable commit/read/recovery/WAL tests pass. | Coupling fix changes durable semantics. |
| Cache regression | Cache lifecycle absence counters from L8G stay clean. | Durable fix leaks into cache policy. |
| Simulation boundary | All admission/maintenance waits use the injected clock. | A wall-clock dependency re-enters the drive path. |
| Benchmark closeout | 100K-10M durable runs complete with bounded backpressure. | The result is explained by a benchmark shortcut. |

## Admission Liveness Tests

Correctness tests:

1. A scripted sustained overload — writer faster than a deliberately slowed
   maintenance executor — completes every commit; the caller never observes a
   retryable `StoragePressureRejected`.
2. While overloaded, the stall deadline resets on each maintenance task
   completion, proven by completing the load past a duration longer than the
   configured stall deadline.
3. With `LevelZeroTableBacklog` pressure active, the wait path has a compaction
   task enqueued or active before any give-up evaluation.
4. With `FrozenBacklog` pressure active, the existing forced-flush behavior is
   preserved.
5. Admission still slows under Urgent pressure and still block-waits under
   Blocking pressure, but block-wait converges to admission rather than
   rejection while maintenance progresses.

Manual-clock tests:

1. A deterministic overload script using the manual clock completes without real
   time and without a pressure rejection.
2. The stall deadline, wait slice, and no-relief rounds are all evaluated
   against the injected clock.

Pass gates:

1. Sustained overload produces zero caller-visible pressure rejections.
2. The give-up decision is a function of maintenance liveness, not of an
   absolute clock deadline alone.

## Liveness Backstop Tests

Correctness tests:

1. A maintenance executor injected as dead (no active task, no pending task, no
   completions) surfaces a typed, bounded failure rather than hanging.
2. A maintenance executor that completes one task then dies still terminates the
   wait with a typed failure after the liveness window, not before.
3. Runtime-closed, panic/shutdown health, and recovery-health rejections remain
   unchanged and take precedence over liveness waits.

Pass gates:

1. No code path can wait forever on a dead executor.
2. The backstop failure is typed and asserts on class and code, not display
   text.

## Publish Critical-Section Cost Tests

The dominant lock-held cost is per-publish recomputation of branch-wide facts and
the full manifest from a full scan of the resident dataset, not durable I/O
(manifest fsync is 2-4% of publish-lock). These tests prove publish work is a
function of the tables the task changed, not of total dataset size.

Correctness tests:

1. Cached per-table bounds and facts (physical/internal key bounds, timestamp
   min/max, manifest table facts) equal a full row scan of the same table, across
   flush outputs, compaction outputs, and after recovery — a divergent cached
   value fails as a durability correctness bug.
2. Branch observed-row facts (`max_commit_version`, timestamp min/max,
   put/tombstone counts) updated incrementally over a flush/compaction sequence
   equal the values a full rescan would produce.
3. The manifest bytes produced incrementally are byte-identical to a full
   `build_manifest` for the same logical state, and recovery reconstructs the same
   layout from them.
4. A durable flush and a durable compaction each visit zero owned-table rows
   during publish (proven by a publish-scoped row-visit counter / the absence of
   bounds/observe calls in the publish path).

Scaling tests:

1. Per-task lock-held publish time (`publish_lock_ns / tasks_completed`) does not
   grow with resident dataset size across a small→large in-process load: it stays
   flat within noise rather than doubling per scale. The pre-Group-C
   O(dataset)-per-publish behavior is detected and fails.

Source guards:

1. The durable publish critical section under the runtime lock does not call the
   row-scanning helpers (`observe_own_rows` / `refresh_observed_row_facts`,
   `manifest_table_bounds`, `timestamp_bounds`) over owned tables, and does not
   clone the full table catalog; bounds and facts come from cached values.
2. Once task 4 of Group C lands, off-lock durable persistence (manifest fsync,
   snapshot/checkpoint writes) is reachable only after the locked pointer/state
   swap returns.

Pass gates:

1. Per-task lock-held publish time is independent of total resident row/table
   count.
2. Foreground wait-on-background-lock drops by the non-fsync residual fraction
   (~96% of publish-lock measured in Group A), not merely the manifest-persist
   fraction.
3. Manifest bytes and recovery output are byte-identical to the full-rebuild
   baseline for the same write history.

## Durability Ordering And Crash Recovery Tests

Correctness tests:

1. A simulated crash between pointer swap and manifest persistence recovers a
   runtime whose affected table is reconstructed from WAL.
2. WAL truncation never deletes a segment whose retired inputs are not yet
   durable in the manifest.
3. Flush-watermark advancement is gated on manifest durability for the retired
   frozen state.
4. Snapshot/checkpoint publication remains consistent with the decoupled
   ordering after a simulated crash.
5. WAL-fsync-failure still halts the writer and requires explicit resume.

Generated tests:

1. Randomized crash points across the pointer-swap / persist / truncate sequence
   recover to a consistent layout every time.
2. Randomized interleavings of commit, flush, compaction, and truncation
   preserve the ordering invariant.

Pass gates:

1. No recovered runtime observes a table pointer without a durable manifest
   entry or replayable WAL.
2. Recovery results match a synchronous-publish baseline for identical write
   histories.

## Throughput And Backpressure Shape Tests

Correctness tests:

1. Under sustained durable load, more than one worker makes maintenance progress
   concurrently.
2. A drain round completes more than one task when the per-wake runtime budget
   exceeds per-task time.
3. Admission slowdown ramps with backlog instead of jumping from none to a hard
   block, observed across increasing L0 depth.
4. Durable correctness and recovery tests still pass with concurrent drain
   progress enabled.

Counter tests:

1. Durable maintenance wall-clock decreases as worker count increases for a
   fixed load.
2. The single-flight regression — only one worker ever active under sustained
   load — is detected and fails.

Pass gates:

1. Worker-pool parallelism is observable, not nominal.
2. The backpressure curve is graduated, with no admission cliff at the blocking
   threshold.

## Durable Regression Tests

Correctness tests:

1. Existing durable commit, conflict, timestamp, branch, and read tests pass
   unchanged.
2. Existing durable flush, compaction, checkpoint, WAL truncation, retention,
   and recovery tests pass unchanged.
3. Durable always mode preserves its stronger commit durability behavior.
4. Diagnostics report durable source/table shape unchanged except for the new
   publish-cost counters.

Pass gates:

1. No durable semantic test changes its asserted class or code.
2. New counters are additive and do not alter existing fact meanings.

## Cache Regression Tests

Correctness tests:

1. Cache lifecycle absence counters from L8G remain zero after a cache load.
2. Cache mode still requires no background worker, condvar, parking primitive,
   or wall-clock wait for correctness.

Source guards:

1. The durable liveness and publish changes do not import into or alter
   cache-mode policy paths.

Pass gates:

1. L8G cache gates remain green.

## Benchmark Gates

Run storage-next durable benchmarks one scale at a time:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape

cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 1m \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
```

Repeat for `5m` and `10m`, and repeat all four scales with `--engines always`.

Run old-engine standard benchmarks one scale at a time in the same environment:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-old-cache-scale -- \
  --engine standard \
  --scales 100k \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000
```

Repeat for `1m`, `5m`, and `10m`.

Hard gates:

1. Storage-next durable standard completes every scale with no commit rejection.
2. No load surfaces a caller-visible pressure rejection while maintenance is
   alive.
3. Per-task lock-held publish time (`publish_lock_ns / tasks_completed`) is flat
   across 100K→10M — independent of resident dataset size — not doubling per
   scale.
4. Foreground wait-on-background-lock is a small fraction of commit time at
   every scale.
5. Durable recovery, snapshot, and WAL tests pass after the 10M load.
6. Durable always mode completes every scale.
7. Cache lifecycle absence counters from L8G remain clean.

Soft targets:

1. 10M durable standard throughput is within 2x of old standard.
2. If the soft target fails with liveness clean and per-task publish cost flat
   across scale, the next owner is a commit/WAL/flush hot-path slice, not
   admission or publish coupling.

## Regression Commands

Required before closeout:

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features
cargo test -p strata-storage-next durable_write_admission_liveness
cargo test -p strata-storage-next durable_publish_lock_flat_with_scale
cargo test -p strata-storage-next durable_publish_crash_recovery
cargo test -p strata-storage-next lifecycle::tests::recovery
```

The named test filters are required discoverability targets. If the actual
module names differ, keep them equally descriptive and update this plan.

## Failure Interpretation

1. A caller-visible pressure rejection during a live load means admission still
   couples write success to maintenance keeping pace.
2. An unbounded hang on a dead executor means the liveness backstop was removed
   instead of being liveness-gated.
3. Per-task lock-held publish time that grows with scale means publish still does
   O(resident-dataset) work — the row rescans, full manifest rebuild, or full
   catalog clone were not eliminated.
4. A recovered table pointer without a durable manifest entry or replayable WAL
   means the durability ordering was violated; this is a durability regression,
   not a tuning issue.
5. An admission cliff at the blocking threshold means the backpressure ramp was
   not smoothed.
6. A durable regression belongs to durable lifecycle correctness, not to
   benchmark tuning.
7. A cache counter regression means the durable fix leaked into cache policy.
8. Throughput misses with clean liveness and per-task publish cost flat across
   scale belong to a commit/WAL/flush hot-path slice.
