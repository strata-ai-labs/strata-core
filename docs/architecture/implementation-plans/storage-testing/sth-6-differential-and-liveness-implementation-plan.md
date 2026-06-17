# STH-6 Implementation Plan: Differential + Liveness Deepening

Status: draft
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
