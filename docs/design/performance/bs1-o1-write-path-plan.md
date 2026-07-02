# BS1 — O(1) write path: implementation and test plan

Status: **ready to implement**. Milestone BS1 of `billion-scale-plan.md` (gaps G1, G2, G3).
Change class: perf refactor with behavior-identical semantics (pressure/admission decisions
must be bit-identical). Assurance: S3.

## Problem (recap)

Every commit performs 5–6 independent folds over every owned SSTable **under the global
runtime lock**, so per-commit cost grows with database size: load throughput decays
330 K → 130 K → 90 K ops/s from 100 K → 10 M records while RocksDB stays flat (~660–935 K).
The folds:

| Fold | Anchor | Called per commit |
|---|---|---|
| `owned_table_byte_count` (all tables) | `branch/read.rs:111` | 2× via `refresh_runtime_memory_total` (`durable/bootstrap.rs:714`; call sites `:677`, `durable/maintenance.rs:670`) |
| `frozen_byte_count` | `branch/read.rs:102` | per pressure collect |
| per-level byte sums via `nonzero_level_targets_for_branch` / `level_byte_count` | `lifecycle/compaction.rs:548`, `:2562` | 2× via `collect_storage_pressure_with_budget` (`:1770`; admission `durable/maintenance.rs:623`, post-commit `:673`) + 1× via `eligible_compaction_tasks` (`:1867`, enqueue at `durable/maintenance.rs:688`) |
| per-level scoring scans (`selected_table_rewrite_score` → `selected_compaction_score`) | `compaction.rs:1785`, `:2286` | same call sites |
| `owned_table_count` | `read_hooks.rs:120` | per pressure collect |

Amplifier: under backpressure the commit retry loop re-collects pressure per retry
(`api/runtime/mod.rs:2772-2779` via `background_wait_after_pressure_rejection:2892`) —
1.8 M retries measured in one 10 M workload-A run.

RocksDB's model (`rocksdb-parity-roadmap.md` RC2): every size-dependent quantity is computed
**once per version install** and cached (`VersionStorageInfo`, `ComputeCompactionScore` called
only from `AppendVersion`); the write path reads O(1) flags and atomics.

## Design

### Key insight from the mutation-point survey

Pressure inputs classify cleanly (survey, Part B3):

- **Per-commit inputs** — `active_rows`, `active_bytes`: already O(1) (cached on
  `MutableTable`, `table/mutable.rs:18/147/192`).
- **Shape-event inputs** — frozen bytes/count, per-level byte sums, owned totals, inherited
  counts: change **only** at the 18 structural mutation points (survey, Part A), all of which
  run under the runtime lock.
- **Live-cheap inputs** — pending-maintenance count, budget pool limits: O(1) already.

So caching the shape-event byte/count aggregates makes **every** pressure/score/budget call
O(levels) ≈ O(1), with no change to when or how pressure is computed — the smallest possible
semantic footprint.

### Decision 1 — recompute-at-event, not per-site deltas

`BranchLocalState` already has the exact hook we need: **`refresh_observed_row_facts`**
(`branch/state.rs:406`) — a fold-from-table-summaries invoked by every structural mutator
(compaction install, materialization, promotions, snapshot/recovery install, fork, clear…),
with one deliberate exception (the flush fast path, below). BS1 extends that hook to also
recompute the byte aggregates. O(#table-summaries) at *event* cadence (one flush per 64 MiB,
one compaction per rewrite) amortizes to ~zero per commit — this is precisely RocksDB's
`PrepareForVersionAppend` recompute model, and it makes correctness structural: any mutator
that calls the hook (i.e., all of them) can never leave a stale cache.

The one exception: `replace_frozen_with_level_zero_table` (`branch/state.rs:240`) skips the
refresh deliberately (`state.rs:255-258`, hot flush path). Its aggregate change is trivially
computable — frozen −1 table, L0 +1 table — so it applies an **incremental delta** instead.

### Decision 2 — keep the pressure computation, feed it cached sums

We do **not** cache the pressure object itself in BS1. With cached sums, each
`collect_storage_pressure_with_budget` call is O(levels); at 5–6 calls/commit that is
negligible. (Full event-computed pressure — the strict RocksDB shape — remains available as a
follow-up if profiling demands; recorded as an open item, not built now.)

### Decision 3 — memory total stays a refresh, becomes O(branches)

`branch_resident_bytes` (`lifecycle/budget.rs:1250`) becomes O(1) (cached components), so
`refresh_runtime_memory_total`'s branch fold becomes O(branches). No atomics, no new
invalidation surface; the existing `runtime_total_bytes: Arc<AtomicU64>` publication is
unchanged. (Atomic delta-tracking is the BS4-era refinement when branches × tables grows.)

### The cached aggregates

New struct on `BranchLocalState` (name indicative):

```text
BranchShapeAggregates {
    per_level_bytes: Vec<u64>,   // one entry per owned level (index-aligned with owned_levels)
    owned_bytes:     u64,        // sum of per_level_bytes
    owned_tables:    usize,
    frozen_bytes:    u64,
    inherited_tables: usize,     // per-layer table count sum (changes at fork/materialization)
}
```

- `refresh_shape_aggregates(&mut self)` — full fold from table summaries; called from
  `refresh_observed_row_facts` and at every construction site (`BranchLocalState::new`
  `state.rs:85`, manifest recovery `manifest_recovery.rs:168`, snapshot install
  `snapshot.rs:255`, fork `fork.rs:50`).
- `apply_flush_shape_delta(frozen_removed_bytes, l0_added_bytes)` — the flush fast path.
- Rotation (`rotation.rs:38-43`): active→frozen moves bytes into `frozen_bytes` — rotation
  does not call the refresh hook either; it applies the incremental delta
  (`frozen_bytes += sealed size`), mirroring the flush path.
- Accessors **replace** the fold bodies (same names, O(1)): `owned_table_byte_count`,
  `frozen_byte_count`, `owned_table_count`, and a new `level_byte_count(level)` /
  `per_level_bytes()` consumed by the scoring path.

Note the rollback path needs no handling: `append`'s rollback
(`append.rs:145` → `rollback_direct_append:221`) only touches the active memtable, whose
size is read through the already-correct `MutableTable` counter — BS1 caches nothing for
the active memtable.

## Slices

### BS1.1 — cached shape aggregates + oracle

**Changes.**
1. Add `BranchShapeAggregates` + `refresh_shape_aggregates` to `branch/state.rs`; wire into
   `refresh_observed_row_facts` (`state.rs:406`) and all construction sites (survey Part A,
   rows 10–13, 17).
2. Incremental deltas at the two hook-skipping mutators: rotation (`rotation.rs:40`) and
   flush replace (`state.rs:240`).
3. Rewire the fold accessors (`read.rs:94-122` region) to the cached fields.
4. **The oracle**: `debug_assert!(cached == fresh_fold())` inside
   `collect_storage_pressure_with_budget` (and in `refresh_shape_aggregates` itself against
   a recount). Debug builds only. This turns the entire existing suite — 3 141 tests
   including the recovery oracle, fault sweep, and simulation faults — into a stale-cache
   detector, because those tests exercise every mutator in Part A.

**Files.** `branch/state.rs`, `branch/state/rotation.rs`, `branch/read.rs` (accessors),
constructor sites (`manifest_recovery.rs`, `snapshot.rs`, `fork.rs`).

**Tests.**
- Unit, one per mutation class (in `branch/tests/`): append + batch append + rollback;
  rotate; flush replace; compaction rewrite install; **metadata promotion** (level sums
  shift, total constant); materialization install + inherited-layer removal; fork/attach;
  clear; manifest-recovery install; snapshot install. Each asserts
  `cached aggregates == reference fold` after the operation (the reference fold lives in
  test support, copied from the pre-BS1 implementations).
- Property-style sequence test: a randomized sequence of the above operations on one branch,
  oracle-checked after each step (deterministic seed).
- Full suite in debug (oracle armed) — the primary gate.

### BS1.2 — O(branches) memory total

**Changes.** `branch_resident_bytes` (`budget.rs:1250`) reads the cached components;
`refresh_runtime_memory_total` (`bootstrap.rs:714`) keeps its shape (now O(branches)); no
call-site changes. Cache-mode mirror (`cache.rs:874-887` budget check path) gets the same.

**Tests.** Existing budget-runtime tests (`lifecycle/tests/budget_runtime.rs`) already assert
the total's semantics; add one test that the total matches a full fold after a
flush+compaction sequence (catches component-composition drift). The 10 M-at-8 GB rejection
behavior must be byte-identical (existing `StorageBudgetExceeded` tests).

### BS1.3 — scoring and eligibility on cached sums

**Changes.**
1. `nonzero_level_targets_for_branch` (`compaction.rs:548`) consumes `per_level_bytes()`
   instead of folding; `level_byte_count` fold retired from the hot path (kept for the
   oracle/reference).
2. `selected_compaction_score` / `level_zero_compaction_score` / `eligible_compaction_tasks`
   (`compaction.rs:2286/2314/1867`): byte thresholds from cached sums; verify (and if needed
   enforce) that the per-level **seed-table scan** runs only for levels whose cached-byte
   threshold already qualifies — steady state then scans nothing.
3. `storage_pressure_throttle_ratio_permille` (`compaction.rs:2132`) reads cached counters.
4. Coverage-scan churn: `schedule_maintenance_coverage_after_branch`
   (`durable/maintenance.rs:732`) allocates + sorts `list_branches` per commit
   (`branch_lifecycle.rs:419`) — reuse a cached branch-id list invalidated on branch
   create/delete (or iterate without allocating).

**Tests.**
- **Pressure-equivalence** (the behavioral gate): for a matrix of constructed branch shapes —
  empty; active-only; frozen backlog at/over threshold; L0 at 4/8/16; multi-level byte
  pressure; inherited layers; pending-maintenance backlog — assert the full
  `LifecycleStoragePressure` (severity, reason, suggested_task, all counters,
  `throttle_ratio_permille`) is **equal** under cached vs reference-fold computation.
- Existing A.1/A.3 and Slice-3 compaction-enqueue tests must pass unchanged (they pin the
  enqueue semantics BS1 must not alter).
- Admission tests already assert on error class/code — unchanged, re-run.

### BS1.4 — backpressure retry de-amplification (measure-first)

With BS1.1–1.3, each retry's pressure snapshot is O(levels) and its forced-enqueue coalesces —
the amplifier may already be gone. **Measure before building**: re-run the 10 M workload-A
admission diagnostic (the `l0_paced` counter probe, temporary). If retry overhead still
registers, implement enqueue-once-per-backpressure-episode (a flag beside
`pressure_wait_deadline` in `execute_commit`, `mod.rs:2630`) and cache the wait-path pressure
snapshot for the episode. If not, close BS1.4 with the measurement recorded.

**Tests (if built).** Existing backpressure-wait tests (`retryable` pacing, stall-deadline
watchdog) unchanged; one new test that a multi-retry episode enqueues its relief task once.

## Perf validation (exit criteria)

Control = branch HEAD (`3bceb4c3`), treatment = BS1, one binary per arm, standard
methodology (load is the stable signal; probes stripped before commit):

1. **Primary (gate):** scoreboard load cells — 100 K / 1 M / 10 M, workloads A–F.
   **Exit: 10 M load ≥ 75 % of 100 K load** (today: 27 %). Directional expectation: 10 M load
   moves from ~90 K toward ≥200 K ops/s (the residual gap is RC1's lock, not RC2).
2. **Secondary (measured, not gated):** workload A/F run throughput and crawl frequency at
   10 M (n≥9 if a claim is made) — the retry-convoy relief should show here.
3. **No-regression:** 100 K cells within noise; run C/B/D/E cells within noise (BS1 touches
   no read path).
4. Ledger row recorded per the standing convention.

## Correctness gates (every slice)

Full `cargo test -p strata-storage-next` in **debug** (oracle armed) and release; recovery
oracle + fault sweep + simulation faults; `clippy --all-targets -- -D warnings`; `fmt --check`.
The pressure-equivalence matrix (BS1.3) is the semantic-freeze gate: BS1 must not change any
admission/scheduling decision, only its cost.

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** no threads, no raw timing — BS1 is pure bookkeeping; the wasm check-build
  gate (`cargo check --target wasm32-unknown-unknown --no-default-features`) joins the
  standing gates from this milestone on.
- **C2 (cache mode):** the aggregates live on shared `BranchLocalState` mechanics and are
  behavior-identical by the equivalence gates — cache-mode suites run unchanged as a gate.
- **C3 (profiles):** aggregates are budget-independent; no profile interaction.
- **C4 (branching):** fork/attach (`fork.rs:50`) and clear/delete are enumerated aggregate
  construction/teardown sites (Part A rows 12–13, 17–18); the per-mutation unit tests
  include fork and clear cases, and the oracle covers inherited-layer counts.

## Risks

| Risk | Mitigation |
|---|---|
| Stale cache via a mutator that bypasses the refresh hook | recompute-at-event design (hook is already universal); the two known exceptions get explicit deltas; debug-assert oracle armed across the whole suite |
| A future mutator forgets the hook | doc-comment contract on `owned_levels`/`frozen` fields + the oracle catches it in CI debug runs |
| Behavioral drift in pressure/admission | pressure-equivalence matrix + existing code/class-asserting tests |
| `refresh_observed_row_facts` cost grows (it now also folds bytes) | same O(#summaries) event-cadence fold it already is; measured in BS1.1 A/B |
| Hidden per-commit fold not in the inventory | the survey was exhaustive (grep-verified `self.layout =`, `Arc::make_mut`, frozen/active mutations); the load-flatness exit criterion would expose any residual O(tables) cost |

## Sequencing & PR discipline

BS1.1 → BS1.2 → BS1.3 → BS1.4 (measure-first), one PR per slice, `BS1.{n}` in the title,
≤1 500 LOC net each, each independently green on all gates. BS1.1 lands the oracle first so
every later slice develops against it. After BS1.3: full scoreboard A/B and the milestone
ledger row; BS1.4 decision from the diagnostic.

## Open items resolved from `billion-scale-plan.md` §8

- Milestone nomenclature: `BS1.{n}` slice codes in PR titles (no collision with the M-track).
- Event-computed *pressure objects* (the strict RocksDB shape): deferred; cached sums
  suffice unless BS1's exit A/B says otherwise — revisit at BS2 kickoff with profile data.
