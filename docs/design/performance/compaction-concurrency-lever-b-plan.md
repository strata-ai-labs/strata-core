# Lever B — concurrent non-conflicting compaction

Implementation + testing plan. Companion to
[`compaction-enqueue-lever-a-plan.md`](./compaction-enqueue-lever-a-plan.md) (Lever A.1+A.3,
committed `8298cde3`) and the ledger. Status: **proposal**. Change class: intentional
semantic change (background concurrency). Assurance: **S3** (concurrency + recovery
oracle + fault sweep required).

## Problem (measured)

After A.1+A.3 the durable writer no longer starves on compaction *enqueue* or
*flush-preemption*, but update-heavy workloads still crawl (baseline: workload A/F run
≈ 94–98 ops/s). The residual limiter is the **single global Rewrite lane**:
`MaintenanceTaskLane::Rewrite` is shared by all Compaction + Materialization tasks, and
the guard `task_lane_is_active` (`lifecycle/maintenance.rs:1432`) permits **at most one
rewrite in flight across the whole runtime** — regardless of the 4 background workers.

Instrumenting the lane on a crawling workload-A run (env-gated sampler + start/done
trace) gave the before-picture:

| measurement | value | meaning |
|---|---|---|
| `active_rewrite` | 0 (20%) / **1 (80%)**, never >1 | single-lane cap confirmed empirically |
| queued+blocked compaction while a rewrite runs | **1, 95% of the time**, usually a *different* level | non-overlapping work is blocked almost always |
| compaction duration | avg 179 ms, **max 24,448 ms** | a slow deep-level compaction monopolizes the lane |
| L0 backlog during a slow compaction | **→ 248 tables** (15× the blocking threshold) | urgent L0 compaction blocked behind the slow one → crawl |
| `lane_busy` turn-aways | **3.26 M** (~17k/s) | workers spin the runtime lock finding the lane busy |

**The pathology:** one slow compaction on a deep level holds the single lane for up to
24 s; the urgent L0→L1 compaction queued behind it cannot run, so L0 explodes and writes
crawl. The blocked compaction is on a **non-adjacent** level (e.g. L0 blocked behind L5)
95% of the time — i.e. it has **disjoint inputs** and could run concurrently.

## Goal / non-goals

**Goal.** Let multiple **non-conflicting** compactions run concurrently across the idle
workers, so an urgent L0 compaction is not blocked behind a slow deep-level one. Target:
L0 stops ballooning, `lane_busy` collapses, `active_rewrite` rises to N, and update-heavy
run throughput lifts out of the ~100 ops/s crawl.

**Non-goals.**
- **Compaction-size bound** (cut the 24 s tail by capping inputs per rewrite). Real and
  complementary, but a separate lever — see §Follow-ups.
- **Broader enqueue** (enqueue *all* eligible levels, not just the top-scored). A.1
  currently keeps ~1 compaction queued; 2 lanes already captures the measured win. If
  B.1's A/B shows the lanes starved for queued work, add this — see §Slices B.2.
- **Materialization concurrency.** Materialization (the other Rewrite-lane kind) is rare;
  it stays serialized (treated as conflicting with everything) in B.1.

## Design

### The conflict model (why concurrency is safe)

Two compactions **conflict** iff their input/output table sets overlap. A level-`L`
compaction reads+writes levels `{L, L+1}` (Ln→Ln+1). Therefore:

- **Same branch, `|level_a − level_b| ≤ 1`** (adjacent or same) → overlap at the shared
  level → **conflict**.
- **Same branch, `|level_a − level_b| ≥ 2`** → disjoint level ranges → **safe**.
- **Different branches** → disjoint tables → **safe**.

Correctness is *already* guaranteed by three existing mechanisms (confirmed in the
flush-preemption investigation), so the conflict rule is an efficiency guard, not the
safety boundary:

1. Compaction inputs are owned-tables-only; `require_candidate_current`
   (`branch/state/compaction.rs:1296`) structurally rejects non-owned refs and re-validates
   inputs at install — a stale candidate is dropped, never published corrupt.
2. The **per-branch publish-slot try-lock** serializes the manifest reserve→record window;
   the loser defers. Two same-branch compactions publish one-at-a-time.
3. Flush and Rewrite are already different lanes; the global runtime lock serializes all
   catalog/version mutation. Only the off-lock `build()` + fsync overlap.

So even a mis-classified conflict cannot corrupt — it can only waste a compaction (caught
as stale). The conflict rule exists to *avoid* that waste and realize genuine parallelism
(non-adjacent same-branch compactions modify different levels → neither goes stale).

### B.1 — relax the lane guard to a conflict predicate + concurrency cap

1. **Conflict predicate** (`lifecycle/maintenance.rs`, self-contained on task scope):
   `fn rewrite_tasks_conflict(a: MaintenanceTask, b: MaintenanceTask) -> bool` —
   for two `MaintenanceTaskScope::TableLevel { branch, level }` tasks, `branch_a == branch_b
   && level_a.abs_diff(level_b) <= 1`; for any Materialization / non-TableLevel scope,
   `true` (conservative serialize). Keeps the executor free of compaction semantics — it
   only reasons about scope.
2. **Relaxed guard.** `next_startable_task_index` (`maintenance.rs:1420`, filter at `:1427`)
   currently excludes a candidate whose lane is active (`!task_lane_is_active`). For a
   Rewrite-lane candidate, replace that with: startable iff (a) **no active Rewrite task
   conflicts** with it (`rewrite_tasks_conflict` against each task in `self.active`), **and**
   (b) `active_rewrite_lane_count() < max_concurrent_rewrites`. Non-Rewrite lanes keep the
   existing single-in-flight guard unchanged.
3. **Concurrency cap.** `max_concurrent_rewrites`, default `worker_count.saturating_sub(1)`
   (leave a worker for flush; = 3 at the default 4 workers). Configurable via lifecycle
   config so it can be tuned/pinned in tests. Never `0`.
4. **Dispatch picks a startable candidate, not just the top-scored.** Today
   `next_scored_table_rewrite_task` (`lifecycle/durable/maintenance.rs:2613`) returns the
   single highest-scored pending rewrite; two workers would both grab it and one loses the
   `start_next_matching` race. Change dispatch to select the highest-scored rewrite that is
   **startable under the relaxed guard** (non-conflicting with actives, under cap), so
   concurrent workers pick *different* non-conflicting compactions. The `start_next_matching`
   race stays as the final serialization point.

No change to the off-lock build, the publish-slot guard, or staleness revalidation — B.1 is
purely the admission guard + dispatch selection.

## Slices

| Slice | Change | Files (est. LOC) |
|---|---|---|
| **B.1** | conflict predicate + relaxed Rewrite guard + concurrency cap + startable-aware dispatch | `lifecycle/maintenance.rs`, `lifecycle/durable/maintenance.rs`, `lifecycle/config.rs` (~180) |
| **B.2** (conditional) | broader enqueue: A.1 enqueues *all* eligible levels, not just top-scored, to feed >2 lanes | `lifecycle/compaction.rs`, `lifecycle/durable/maintenance.rs` (~60) |

B.2 only if B.1's A/B shows lanes idle for lack of queued work (`active_rewrite` capped
below the concurrency cap while backlog persists). Both under the 1,500-LOC guidance.

## Testing plan (TDD)

Unit / decision-level (`lifecycle/tests/maintenance/`, `.../compaction/`):

1. **Conflict predicate.** same branch adjacent (`|Δ|≤1`) ⇒ conflict; same branch
   non-adjacent (`|Δ|≥2`) ⇒ no; different branch ⇒ no; any materialization ⇒ conflict.
2. **Guard admits non-conflicting concurrent rewrites.** With one Rewrite active at level
   L, `next_startable_task_index` admits a pending compaction at level `L+2` (and a
   different-branch one) but rejects `L`, `L±1`.
3. **Concurrency cap respected.** With `max_concurrent_rewrites` active non-conflicting
   rewrites, a further non-conflicting candidate is refused until one finishes.
4. **Non-Rewrite lanes unchanged.** Flush/checkpoint/etc. keep single-in-flight semantics.

Behavioral (`lifecycle/tests/durable.rs`, concurrency harness):

5. **Two non-conflicting compactions run concurrently.** Build a branch with backlog at L0
   and L2 (or two branches); drive the drain with ≥2 workers; assert two compactions are
   active simultaneously (`active_rewrite_lane_count() == 2`) and both complete, both
   levels drain.
6. **Conflicting compactions serialize.** L0 and L1 backlog ⇒ never both active; the second
   waits, and neither publishes a stale/corrupt manifest.
7. **Slow compaction does not block L0.** With a long-running deep-level compaction in
   flight, a freshly-queued L0 compaction still starts and completes (the core pathology fix).

Correctness / durability (the S3 bar — this is the risky part):

8. **Recovery oracle + fault sweep** with concurrency enabled: power-loss / backend-fault /
   disk-full at every write position across interleaved concurrent compactions loses no
   data and recovers a valid state (the publish-slot + staleness guards must hold under
   real concurrency). Assert on the oracle, not prose.
9. **Stale-defer rate stays low.** Instrument (or assert) that concurrent compactions rarely
   go stale — if the conflict rule is right, non-adjacent same-branch compactions never
   invalidate each other.

Suite gates: full `cargo test -p strata-storage-next` (maintenance, compaction, recovery
oracle, fault sweep, commit-hardening, format goldens); `clippy --all-targets -D warnings`;
`fmt --check`.

## Perf validation (control-first — the trace handle is already built)

Re-run the same `STRATA_TRACE` instrumentation (uncommitted, ready) control (`8298cde3`)
vs B.1, 10M/1000B/48g workload A:

- `active_rewrite` distribution — **expect it to rise from ≤1 to up to the cap** (the direct
  proof of concurrency).
- L0 (`owned_levels[0]`) trajectory — **expect it to stop ballooning to ~250** and stay
  bounded near the compaction thresholds.
- `lane_busy` turn-aways — **expect a large collapse** from ~3.26 M.
- `compact_done` rate — **expect it to rise** (more compactions/sec).
- **Median write throughput + crawl-rate over an interleaved n≥9 A/B** (the ledger's convoy
  metric) — the throughput headline. A single run is a coin flip (baseline caught A at 94,
  the A.1+A.3 A/B caught it at 32k), so the crawl claim needs n≥9, not one run.
- Read-only C and load avg — **hold flat** (Lever B must not regress reads or bulk load).

**Falsification.** If `active_rewrite` rises but the L0 backlog / crawl don't improve, the
residual is elsewhere (the 24 s compaction tail itself, or lanes starved for queued work →
B.2). Both are visible in the same trace (`compact done` durations; `pending_compactions`
vs cap).

## Risks & mitigations

- **Concurrency corruption (highest risk).** Mitigated by the *pre-existing* guards — owned-
  table-only inputs, `require_candidate_current` staleness rejection, per-branch publish-slot
  try-lock, global-lock catalog mutation — plus the conflict rule as an efficiency layer.
  Validated by the recovery oracle + fault sweep under concurrency (test 8). This is the gate;
  do not ship without it green.
- **Flush starvation.** Cap `max_concurrent_rewrites < worker_count` so a flush worker is
  always available; verify L0/frozen and the flush-watermark stay healthy in the perf run.
- **Wasted work from a mis-scoped conflict.** Correct but wasteful; bounded by test 9 (stale
  rate) — if it shows up, tighten the conflict rule or the dispatch ordering.
- **Lane churn (`lane_busy` 3.26 M).** B.1 reduces it (fewer turn-aways once multiple can
  run), but a follow-up could back off a worker that finds no startable rewrite rather than
  re-spinning the runtime lock. Note it in the ledger; not required for B.1.

## Follow-ups

- **Compaction-size bound** — cap inputs/output bytes per rewrite so no single compaction
  holds a lane for 24 s. Cuts the duration tail directly; complements B.1.
- **B.2 broader enqueue** — feed >2 lanes when the backlog spans many levels.
- **Turn-away backoff** — stop re-spinning the runtime lock when no rewrite is startable.

## PR discipline

One slice per PR, slice code in the title (assign against the roadmap), e.g.
`perf(storage-next): run non-conflicting compactions concurrently (Lever B.1)`. States
change class (intentional semantic change — background concurrency) + assurance (S3, with
the recovery-oracle/fault-sweep evidence) and links a new ledger row. The `STRATA_TRACE`
debug probes are reverted before the PR.
