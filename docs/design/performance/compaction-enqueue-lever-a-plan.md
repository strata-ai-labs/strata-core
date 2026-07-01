# Lever A — reliable non-zero-level compaction enqueue

Implementation + testing plan. Companion to
[`lock-decoupling-perf-ledger.md`](./lock-decoupling-perf-ledger.md). Status:
**A.1 + A.3 landed** (measured **+18% load throughput**); **A.2 dropped**.
Change class: intentional semantic change (compaction scheduling). Assurance: S3
(maintenance + recovery test suites + control-first perf gate).

## Progress (2026-07-01)

- **A.1 — implemented, tested, measured.** Shipped as `pub(crate) fn
  eligible_compaction_task(branch, budget, global)` in `lifecycle/compaction.rs`
  (not the struct field the A.1 design below sketched: a second
  `Option<MaintenanceTaskRequest>` pushed the `Copy` `LifecycleStoragePressure` past
  clippy's 256-byte pass-by-value limit, so it derives the task from the branch
  instead, with memory deferral keyed on the rewrite's own severity). The durable
  post-commit scheduler enqueues it for `EvaluateAndEnqueue`/`Background`. 3 unit + 1
  durable behavioral test; full lib suite green (3134 pass), clippy/fmt clean.
- **A.1 A/B (control vs A.1, same probes/config).** A.1 works as designed:
  `empty_q` **52% → 39%**, `lane_busy` **48% → 25%** — compaction is now reliably
  queued. But the backlog did **not** move (`owned` still → ~149, `run` still ~623,
  throttle/load unchanged): A.1 **relocated** the blocker rather than removing it —
  `flush_preempt` exploded **9 → 688,883**. Compaction is now dispatched constantly
  and then **deferred at run time** by the flush-preemption gate. The plan's
  falsification prediction ("residual = the single lane, Lever B") was **wrong** —
  `lane_busy` *fell*; the true residual is flush-preemption. (This is why we measure.)
- **A.2 — DROPPED.** It was an on-reject enqueue backstop, but A.1 enqueues on every
  post-commit (strictly more coverage) → A.2's forced enqueue almost always finds a
  task already queued (coalesced) → no-op. And enqueue is no longer the constraint,
  so a third enqueue path adds nothing. Superseded by A.1 + A.3.
- **A.3 — implemented, tested, measured (the win).** Loosened the flush-preemption
  gate (`compaction.rs:1373`) to fire at `FROZEN_BLOCKING_FLUSH_THRESHOLD` instead of
  any single frozen table. A.1+A.3 A/B vs the pre-A.1 control (10M/1000B/48g, workload
  A): **load throughput +18%** (76.2k → 90.2k ops/s), **`flush_preempt` 688,883 → 0**,
  **`NonZeroLevelTableBacklog` rejections −60%** (36 → 14). The residual limiter is now
  the single global Rewrite lane (`lane_busy` rose 25% → 62%) — the A→B handoff, this
  time confirmed. Full lib suite green (3136), clippy/fmt clean.

## Problem (evidence-grounded)

Under the update-heavy YCSB workload the durable writer is throttled/rejected by
`LevelZeroTableBacklog` / **`NonZeroLevelTableBacklog`** while background workers
sit ~half idle. Instrumenting the drain's table-rewrite dispatcher over a 10M/48g
run (counters emitted 1/sec) showed, across **955,690** reaches of the compaction
step during the load:

| outcome | share | meaning |
|---|---|---|
| `empty_q` (no rewrite task queued) | **52.3%** | nothing to run |
| `lane_busy` (single Rewrite lane occupied) | 47.6% | Lever B territory |
| `flush_preempt` | ~0% (9 events) | **not** the cause |
| `run` (dispatched) | 0.07% (667) | compaction almost never ran |

`owned` tables grew monotonically 5 → 153; `throttle_permille` sat at ~500 (≈50%
writer pacing) the whole load. The backlog is in the **non-zero levels** (L0 stayed
4–9; flush + the L0 forced-enqueue keep L0 in check).

### Root cause of `empty_q` (why no non-zero task is ever queued)

The source branch's only compaction-creation path is the post-commit scheduler
(`schedule_post_commit_maintenance_for_branch`, `lifecycle/durable/maintenance.rs:655`),
which enqueues exactly one task: `pressure.suggested_task()`. That single task comes
from `storage_pressure_decision` (`lifecycle/compaction.rs:1829`), a **priority
cascade that returns one task**, and:

- `frozen_tables > 0 → flush` at **`compaction.rs:1869`** short-circuits *before*
  every compaction branch (blocking `:1876`, urgent `:1920`, normal `:1939`).
- Under sustained writes there is essentially always ≥1 frozen table (`frozen=1`
  in every sample), so `suggested_task()` returns **flush, never compaction**.

Two backstops that would otherwise create the task do not apply here:

- The coverage scan (`schedule_maintenance_coverage_after_branch`, `:704`) **skips
  the source branch** (`:732-734`) — it only covers *other* branches. Single-branch
  workloads get nothing from it.
- The backpressure forced-enqueue (`enqueue_pressure_maintenance_for_background_wait`,
  `api/runtime/mod.rs:3148`) force-enqueues a compaction only for
  **`LevelZeroTableBacklog`** (`:3178`, `:3200`); `NonZeroLevelTableBacklog` has no
  branch.

Net: a non-zero-level backlog on the write-taking branch has **no reliable
task-creation path** while writes flow. The ~667 compactions that did run were
mostly the L0 forced path plus rare `frozen==0` windows.

## Goal / non-goals

**Goal.** Ensure an eligible compaction task for the source branch's backed-up
non-zero level (and L0) is reliably enqueued while writes flow — decoupled from the
flush-first `suggested_task` cascade — so the Rewrite lane always has non-zero
compaction work available to pick.

**Non-goals (explicitly out of scope for Lever A).**

- **Lever B** (breaking the single global Rewrite lane for concurrent compaction).
  A makes the work exist; B lets it run in parallel. Measured separately.
- **Admission semantics.** `suggested_task()`, `severity()`, `reason()`, and the
  `StoragePressureRejected` reason mapping are unchanged. Lever A is *additive* to
  scheduling only.
- **Multi-branch coverage idle-stop** (`MAINTENANCE_COVERAGE_IDLE_ROUND_LIMIT`).
  The coverage scan skips the source branch, so the idle-stop is not on the path
  for the single-branch repro; a multi-branch backlog robustness pass is a
  follow-up (see §Follow-ups), not part of A.

## Design

### A.1 — Enqueue an eligible compaction independently of the flush suggestion

Compaction and flush are orthogonal work (different levels, different resources);
the scheduler must be able to enqueue both.

1. **Carry a compaction suggestion on the pressure struct.** In
   `collect_storage_pressure_with_budget` (`compaction.rs:1757`) the
   `table_rewrite_score` is already computed. Add a second field to
   `LifecycleStoragePressure`, `compaction_task: Option<MaintenanceTaskRequest>`,
   set to `table_rewrite_score.map(|s| s.task_request(branch_id))` — i.e. the
   scored level's compaction/materialization request whenever a level is at/above
   its eligibility trigger (4 tables / byte target), **independent of
   `frozen_tables`**. Expose `compaction_task()` (mirror `suggested_task()` at
   `:1274`). Route it through the same memory-pressure neutralization as
   `suggested_task` (`deferred_under_global_memory_pressure`, `:1253`) so a
   Background-severity suggestion under global memory pressure is still deferred.
2. **Enqueue it post-commit.** In `schedule_post_commit_maintenance_for_branch`
   (`:655`), after the existing `schedule_suggested_post_commit_maintenance(...)`
   call (`:675`), also enqueue `pressure.compaction_task()` when present, gated by
   the same `policy` (skip when `Disabled`/`DeterministicInline` handles it inline).
   Coalescing (`MaintenanceTaskPolicy::coalescing()`, key = `(kind, TableLevel{branch,
   level})`, `maintenance.rs:520`) makes repeated enqueues idempotent — at most one
   task per (branch, level), so this cannot flood the queue.

Result: whenever a non-zero level (or L0) is at/above its trigger, a compaction task
for it is present in the queue on the next commit, even while `frozen>0` keeps
`suggested_task` pinned to flush.

### A.2 — Non-zero forced-enqueue backstop — **DROPPED**

Subsumed by A.1 (which enqueues on every post-commit, strictly more coverage than an
on-reject backstop) and orthogonal to the true constraint (flush-preemption, not
enqueue). See §Progress.

### A.3 — Loosen the flush-preemption gate (the binding constraint)

`defer_compaction_for_resource_policy` (`compaction.rs:1373`) defers a compaction as
"flush-preempted" whenever `branch.frozen_table_count() > 0` — i.e. *any single*
frozen memtable. Under sustained writes a frozen table is essentially always present,
so once A.1 keeps a compaction queued it is dispatched → preempted → requeued with
**no backoff** (`requeue_flush_preempted_compaction`), ~688K times, and never runs.

**Investigation (2026-07-01) — loosening is safe.** The gate is a *prioritization
policy*, not a correctness barrier:

- **Compaction inputs are strictly the durable owned tables.** `compaction_sources` /
  `table_for_candidate_ref` (`branch/state/compaction.rs:1339`, `:1268`) resolve only
  owned tables; `require_candidate_current` (`:1296`, `:1317-1327`) structurally
  **rejects** any non-owned (frozen/inherited) ref ("compaction candidate must
  reference branch-owned tables"). Rows still in a frozen memtable are simply not in
  the input set — compaction never reads unflushed state.
- **Flush and compaction are different lanes** (`MaintenanceTaskLane::Flush` vs
  `Rewrite`, `maintenance.rs:707-712`); the lane guard already permits them to run
  concurrently. A concurrent flush that lands a new L0 during a compaction is caught
  by `require_candidate_current` staleness re-validation and serialized by the
  per-branch publish-slot try-lock (the loser defers) — not by flush-before-compact
  ordering. **No invariant mandates flush-before-compact.**
- The only stated "why" is the reason string `"flush pressure preempted compaction"`;
  the intent is to reclaim mutable-memory/WAL by flushing first. That intent only
  matters when the frozen backlog is genuinely urgent — which the codebase already
  quantifies as `FROZEN_BLOCKING_FLUSH_THRESHOLD = 4` (`compaction.rs:47`), the point
  where the frozen backlog *blocks* write admission (`:1863`).

**Change (one line).** Gate on the blocking threshold instead of `> 0`:

    // compaction.rs:1373
    if branch.frozen_table_count() >= FROZEN_BLOCKING_FLUSH_THRESHOLD {
        return Ok(Some(flush_pressure_preempted_compaction_outcome()));
    }

So compaction runs concurrently with flush in the steady state (frozen 1–3) and
yields to flush *only* when flush is so far behind it is blocking writes (frozen ≥ 4).
This unblocks A.1's queued work *and* collapses the no-backoff requeue churn.
Memory-pressure deferral is unaffected — A.1 already skips enqueue under global memory
pressure, and the IO-byte-budget deferral just below the gate is preserved. The gate
is shared with the cache path, so the single change covers both. Threshold is tunable;
the perf run validates flush stays healthy.

## Slices

| Slice | Change | Files (est. LOC) |
|---|---|---|
| **A.1** ✅ | `eligible_compaction_task` helper + durable post-commit enqueue | `lifecycle/compaction.rs`, `lifecycle/durable/maintenance.rs` (~90) |
| ~~A.2~~ | ~~`NonZeroLevelTableBacklog` forced-enqueue backstop~~ — **dropped** (subsumed by A.1) | — |
| **A.3** | loosen the flush-preemption gate to the blocking threshold | `lifecycle/compaction.rs` (~1 line + tests) |

A.1 landed; A.3 is the slice that delivers the measured win. **A.1 + A.3 commit
together** — A.1's enqueue only churns until A.3 lets it run. Both well under the
1,500-LOC guidance.

## Testing plan

### A.1 (TDD — write these first)

Unit / decision-level (`lifecycle/tests/compaction/`):

1. **Compaction suggested despite frozen memtables.** Build a branch state with a
   non-zero level at the eligibility trigger **and** `frozen_tables > 0`. Assert
   `collect_storage_pressure_with_budget(...).compaction_task()` is `Some` with
   `kind == Compaction` at the expected level, while `suggested_task()` is still the
   flush (regression guard on the unchanged cascade).
2. **No compaction below trigger.** Level with < trigger tables and no byte
   pressure ⇒ `compaction_task()` is `None` (no queue flooding).
3. **Memory-pressure deferral respected.** Background-severity compaction under
   global memory pressure ⇒ `compaction_task()` neutralized to `None`, matching
   `suggested_task` behavior.
4. **L0 still covered.** L0 at trigger ⇒ `compaction_task()` yields the L0→L1
   compaction (no regression vs the existing forced path).

Behavioral (`lifecycle/tests/durable.rs` / `api/tests/`):

5. **Post-commit enqueues compaction under frozen backlog.** Drive commits that
   produce a non-zero backlog with frozen tables present; assert the maintenance
   queue contains a `Compaction` task at the backed-up level after the post-commit
   scheduler runs.
6. **Coalescing / no flood.** Repeated post-commit scheduling with the same backlog
   ⇒ exactly one pending compaction task per (branch, level) (`was_coalesced`).
7. **Backlog drains.** Integration: sustained commits + background drain rounds ⇒
   the non-zero level's table count trends **down** (or bounded) rather than growing
   unbounded. Assert on the per-level owned-table count before/after N rounds.
8. **Admission unchanged (regression).** Assert `StoragePressureRejected` reason
   codes and severities are identical to pre-A.1 for the same shapes (Lever A must
   not alter admission — assert on error class/code per the error contract, never
   display text).

### A.3 (TDD)

Unit / decision-level (`lifecycle/tests/compaction/`):

9. **Compaction runs below the blocking threshold.** `defer_compaction_for_resource_policy`
   with `frozen_table_count()` 1–3 ⇒ returns `None` (not preempted); at `frozen ≥ 4`
   ⇒ returns `Some` (flush-preempted). Assert the boundary at
   `FROZEN_BLOCKING_FLUSH_THRESHOLD`.
10. **IO-budget deferral intact.** With a `PerTaskByteBudget` configured, an
    over-budget plan, and `frozen < 4`, the io-budget deferral still fires — the gate
    change only touched the frozen branch.

Behavioral (`lifecycle/tests/durable.rs`):

11. **Queued compaction actually runs with frozen present.** With a backlog and 1–3
    frozen tables, driving the background drain runs the compaction (a `Compaction`
    outcome that is *not* `Deferred`/flush-preempted) and the level's table count
    drops — where pre-A.3 it would flush-preempt. Direct behavioral proof of the
    A.1→A.3 pair (compaction is queued *and* runs).
12. **Flush keeps exclusive priority when blocking.** At `frozen ≥ 4` the compaction
    is still deferred flush-preempted, so a severe flush backlog drains without
    compaction competing.

### Suite gates (both slices)

- `cargo test -p strata-storage-next` — full maintenance/compaction, recovery
  oracle, flush/checkpoint/commit-hardening, format goldens (format unaffected, but
  run to confirm).
- `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`.
- Source-guard: no architecture labels in new source comments; slice code in PR
  title only.

## Perf validation (control-first — this is the real gate)

Use the `STRATA_TRACE` compaction counters already in the tree. Interleaved A/B,
same binary where possible, 10M/1000B/48g workload A, recording per run:

- `empty_q` share of `reached` — **expect a large drop** (the direct target).
- `run` count and, ideally, a per-level split — **expect non-zero compaction runs
  to rise**.
- `owned` / per-level table trajectory — **expect the backlog to flatten** instead
  of climbing to 150+.
- `throttle_permille` — **expect it to fall** from ~500.
- **Load throughput** (the ledger's stable signal) — expect it to rise if
  compaction was the throttle; this is the headline number, not the noisy
  single-run `run` ops/s.
- `commit reject` counts by reason — expect `NonZeroLevelTableBacklog` to shrink.

Because run-phase throughput is intermittent (ledger §"signal vs noise"), pass/fail
is judged on the **mechanistic signals** (`empty_q`↓, non-zero `run`↑, backlog
flattens, `throttle`↓) plus **load throughput**, not a single run-phase sample.
Record a new ledger row.

**A.1 result: the falsification fired — informatively.** `empty_q` dropped but the
backlog/throttle did not, and the residual was **not** the lane (`lane_busy` *fell* to
25%) — it was flush-preemption (`flush_preempt` 9 → 688,883). A.1 was the necessary
probe that revealed the true gate.

**A.3 acceptance gate.** With A.1 + A.3, expect `flush_preempt` to **collapse**, `run`
to **climb** by orders of magnitude, `owned`/per-level backlog to **flatten** instead
of climbing to ~150, and `throttle_permille` + load throughput to improve; also verify
L0/frozen stay low (flush not starved). New falsification: if `flush_preempt` collapses
but the backlog still doesn't drain, the next residual is the single Rewrite lane
(Lever B) — and *then* we'd expect `lane_busy` to rise.

## Risks & mitigations

- **Queue flooding.** Mitigated by coalescing (one task per branch/level); assert in
  test 6.
- **Memory pressure from scheduling compaction under frozen backlog.** Enqueue ≠
  run; run-time IO/memory deferral (`defer_compaction_for_resource_policy`) and the
  Background+memory neutralization are preserved (test 3). No change to run-time
  admission of compaction.
- **Starving flush (A.3).** Loosening the gate lets compaction compete with flush for
  I/O and the per-branch publish slot. Mitigations: flush is drain step 1 (dispatched
  first) and a *separate* lane, and A.3 still yields compaction entirely at
  `frozen ≥ FROZEN_BLOCKING_FLUSH_THRESHOLD`. Verify L0/frozen stay low and the
  flush-watermark keeps advancing in the perf run.
- **Concurrency/recovery.** No new locks, no format change. A.3 *does* newly allow a
  compaction to run while a flush is in flight, but that path is already safe:
  compaction inputs are owned-tables-only, a racing flush is caught by
  `require_candidate_current` staleness re-validation, and the per-branch publish-slot
  try-lock serializes manifest writes. Run the recovery oracle + fault sweep (S3).

## Follow-ups (after A.1 + A.3)

- **Lever B** — concurrent compaction (break the single global Rewrite lane so >1
  compaction runs at once). Deprioritized: `lane_busy` fell to 25% under A.1, so it is
  not the binding constraint now — but if A.3's A/B shows `flush_preempt` collapsing
  yet the backlog still not draining, the lane is the next residual.
- **Requeue backoff** — `requeue_flush_preempted_compaction` re-dispatches with no
  backoff; A.3 removes most of the churn, but a small backoff would harden the
  `frozen ≥ 4` window.
- **Multi-branch backlog** — the coverage scan skips the source branch and idle-stops
  after 5 rounds; a robustness pass so *non-source* branches with a backlog keep
  getting scheduled.

## PR discipline

**A.1 + A.3 land together** in one PR — A.1's enqueue only pays off once A.3 lets it
run, and committing A.1 solo ships the flush-preempt churn with no win. Slice code in
the title (assign against the roadmap), e.g. `perf(storage-next): enqueue eligible
compaction and loosen flush-preemption so it runs (Lever A.1+A.3)`. States change class
(intentional semantic change) + assurance (S3) and links a new ledger row. The
`STRATA_TRACE` / `STRATA_SNAP_CAP_MB` debug probes are reverted before the PR.
