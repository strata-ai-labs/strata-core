# STH-6 Implementation Plan: Differential + Liveness Deepening

Status: implemented (2026-07-16, TCP1.4) — see "As built" below

## As built (2026-07-16, slice TCP1.4)

**6a + 6b** — `crates/storage/src/testkit/config_differential.rs`. The shared
seeded workload (reusing the recovery-oracle generator) drives commits on two
branches (one forked mid-stream) with interleaved maintenance under the full
config matrix {cache, durable-Standard, durable-Always} × {default,
low-memory budget}, all under `EvaluateAndEnqueue` for determinism. Three
oracles per run: cross-config equality of full logical snapshots (keys,
values, *and* commit versions, per branch, at three checkpoints — divergence
names the config, checkpoint, branch, and key), a metamorphic point-read
check inside every config (every scanned row must be reproducible by a point
read — the NoREC-style oracle), and expected-state-model equality on the
default branch. Green across the matrix and a 16-seed soak (nightly lane).

**Finding — issue #2609 (the program's first product bug).** The
pressure-equivalence cell (a long stream against the low-memory budget)
exposed a livelock: under `EvaluateAndEnqueue`, sustained pressure wedges
admission permanently — `BlockMutatingAdmission`/`FrozenBacklog` with an
empty queue, every drain refused by the frozen-rotation `ResourceExhausted`
it is supposed to relieve, and explicit enqueue of every maintenance kind
does not help. The production-default background scheduler survives the
identical budget and load (verified). The regression test
(`pressure_retries_are_invisible_to_readers`) is `#[ignore]`d referencing
#2609 and must be un-ignored by the fix (the 3d failing-then-fixed
discipline).

**6c** — `crates/storage/src/api/tests/liveness_matrix.rs`. Deterministic
liveness matrix: {cache, durable-Standard, durable-Always} × all 11
maintenance kinds, each kind cycled against live write traffic; per cell:
commits never fail permanently, drains succeed with no failed task, queue
ends empty, storage never ends blocked on pressure, and every written row
reads back. Runs in the default per-PR suite. The perf-trace-gated
background-scheduler endurance suite (`background_scale.rs`) — previously
invisible to every CI lane because `perf-trace` is not a default feature —
now runs in the nightly durable-invariants job.
Charter classes: 2 — Silent wrong results (🟡 → ✅) and 8 — Trajectory/liveness (✅, deepen)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: none (independent; can run in parallel with STH-1..5).

## Objective

Two independent deepenings that share a workload generator:
1. **Config-sweep differential (class 2):** run the same workload under every
   storage configuration — cache vs durable-standard vs durable-always, each
   scheduling policy, each budget profile — and assert *identical logical read
   results*. Durability and timing may differ; the data the caller sees may not.
2. **Liveness matrix (class 8):** the endurance suite proves bounded resources and
   progress for *one* path today; extend it to every mode × every maintenance
   kind so no scheduling regime can silently fall behind.

## Why this matters (blog beat)

A database has many internal paths that should produce one logical answer:
optimized and unoptimized, cached and durable, eager and deferred maintenance.
DuckDB finds silent wrong-result bugs by diffing optimized against unoptimized;
SQLite diffs against four other engines. StrataDB has model-parity (good) but has
never asserted that its *own* configurations agree with each other — the place
where a cache-only fast path or a scheduling variant quietly diverges. And while
the June endurance suite caught the perf collapse and the admission deadlock, it
covers one trajectory; world-class means every maintenance kind, in every mode,
is proven to keep up.

## Seams to build on (verified 2026-06-17)

- Model-parity oracles (`src/testkit/api/{model,commit,branch,maintenance,
  diagnostics}.rs`) — the reference for "correct logical result," extended to
  cross-config diffing.
- Endurance substrate: `src/api/tests/background_scale.rs` +
  `scaled_closed_loop_test_profile` (`src/lifecycle/budget.rs:247`).
- Mode/policy surface: `StorageMode` (cache / durable-standard / durable-always),
  `StorageMaintenanceSchedulingPolicy`, `StorageBudgetPolicy`,
  `StorageWalGrowthPolicy`; maintenance kinds: flush, compaction, materialization,
  retention, snapshot-pruning, checkpoint, WAL-growth.

## Coverage target (not line count)

Exit bar (2) = "the same workload under every config produces identical logical
read results." Exit bar (8) = "bounded-resource + progress asserted for every
maintenance kind in every mode." Measured by the config matrix breadth and the
maintenance-kind × mode breadth, not by harness size.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 6a | Shared workload generator | Seeded op stream (commits, branches, reads, maintenance triggers) replayable across configs |
| 6b | Config-sweep differential | Run the stream under the full {mode × policy × budget} matrix; assert identical logical reads (durability/timing excepted); diff reports the diverging config + op |
| 6c | Liveness matrix | Parametrize the endurance suite over {mode × maintenance kind}; assert WAL bounded, queue drains, no permanent commit failure, shape converges, per cell |

## Implementation detail

### 6a — Workload generator (`src/testkit/workload.rs`)
A seeded generator emitting a deterministic op stream and the expected logical
read-set (via the existing model-parity oracle). The same stream feeds both the
differential matrix and (optionally) STH-1/STH-4, so generators are shared, not
duplicated.

### 6b — Config-sweep differential (`tests/config_differential.rs`)
For each config in the matrix, run the stream and capture the logical read
results at checkpoints. Assert all configs agree with the model and with each
other on logical content; only durability outcomes and timing facts may differ.
A divergence yields a typed report naming the config and op index. This is the
cache-vs-durable logical-equivalence the charter exit bar calls for.

### 6c — Liveness matrix (`src/api/tests/background_scale.rs`, parametrized)
Generalize the two existing closed-loop tests into a matrix over
{cache, durable-standard, durable-always} × {flush, compaction, materialization,
retention, snapshot-pruning, checkpoint, WAL-growth}, each with the scaled
profile. Per cell, assert the charter's liveness invariants. Scaled so the full
matrix runs in CI seconds; a larger sustained version runs nightly.

## Constraints

- Deterministic, seeded; the diverging config/op or the breaching cell is printed
  on failure.
- Differential asserts *logical* equality only — it must not over-constrain
  durability or timing (those legitimately differ by config).
- Behavioral names only; the workload generator lives in `testkit/` for reuse.

## Exit gate

- Config-sweep differential green across the full {mode × policy × budget} matrix.
- Liveness invariants asserted for every maintenance kind in every mode.
- Charter class 2 flips 🟡 → ✅; class 8 coverage broadened from one trajectory to
  the full matrix, with this plan as evidence.
