# STH-4 Implementation Plan: Deterministic Simulation Driver (DST)

Status: **4b + 4c + 4d implemented; class 9 closed (2026-06-19)**; 4a descoped. 4c's DST surfaced two durability bugs — a publish fault during checkpoint+flush (**fixed 2026-06-18**) and a power-loss `Gap` at seed 155 (**fixed 2026-06-19**); both regressions are green and the 3000-seed fault soak runs clean end-to-end. See the two finding docs' Resolutions.
Charter class: 9 — Rare-interleaving / fault-combination bugs (🟡 Partial → advanced; full ✅ blocked on the durability bug the DST found)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: **STH-1** (safety oracle), **STH-2** (fault dimension). Substrate already landed.

## Objective

Build the seeded explorer that drives the *production* `Background` path under a
single source of randomness — sweeping background-task orderings, clock
advancement, and fault combinations — and asserts safety (the STH-1 oracle) plus
liveness every step. Any failure prints its seed and replays bit-exact. This is
the single highest-leverage technique in the taxonomy; the hard precondition work
is already done, so this plan is additive.

## Why this matters (blog beat)

FoundationDB and TigerBeetle owe their reputations to one idea: put every source
of nondeterminism behind a seam, then let a seeded simulator run millions of
adversarial schedules, knowing any failure replays exactly. It is normally an
impossible retrofit. StrataDB paid that cost already — `MaintenanceExecutor`,
`MaintenanceClock`, the inline executor that drives the real lifecycle path — it
just hasn't built the explorer on top yet. This plan is the payoff: the moment a
database can hand you a seed that reproduces any failure, its testing story
becomes credible. This is the blog's climax.

## Seams to build on (verified 2026-06-17 — the retrofit has LANDED)

- `trait MaintenanceExecutor` + `Arc<dyn MaintenanceExecutor>`
  (`src/lifecycle/background.rs:138`, `src/api/runtime/background.rs:47`);
  `InlineMaintenanceExecutor` runs drains synchronously under step control.
- `trait MaintenanceClock` + `ManualMaintenanceClock` — decision/admission timing
  (block-wait deadlines, pressure slowdown, drain limits) reads the clock.
- `DeterministicInline` drives the **production** `Background` path (proven by
  `deterministic_inline_uses_background_drive_path_without_worker_threads`).
- Replay primitive: `run_inline_replay_scenario` already proves bit-exact replay;
  `threaded_and_inline_background_executors_converge_on_compaction_shape` proves
  the inline path matches the threaded one.
- Residual: a handful of perf-trace **duration** `Instant::now()` calls in
  `lifecycle/{cache,durable/maintenance,compaction,rewrite_publication}.rs` are
  not yet behind the clock (state is deterministic; timing *numbers* are not).

## Coverage target (not line count)

Exit bar = "a seeded interleaving + fault-combination driver over the production
path; replay-on-failure; nightly long-seed soak." Measured by: the driver
randomizes task ordering AND clock AND faults AND client ops under one seed;
failures replay; the soak runs. Not measured by harness size.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 4a | Residual clock injection | Route the perf-trace duration `Instant::now()` calls through `MaintenanceClock`; timing facts become reproducible under `ManualMaintenanceClock` |
| 4b | The simulation driver | Seeded step loop over {advance clock, run next task in chosen order, issue client op}; safety (oracle) + liveness asserted each step |
| 4c | Fault-combination dimension | Compose the STH-2 fault backend into the sim; the seed also schedules faults; recovery oracle holds across combinations |
| 4d | Seed capture/replay + soak | Failures print the seed; a seed replays the exact trajectory; CI smoke (bounded seeds) + nightly `#[ignore]` soak (100k+ seeds) |

## As-built (2026-06-18)

**Delivered: 4b + 4d** (`src/testkit/simulation/{mod.rs, driver.rs}` + `tests/simulation_smoke.rs`) plus one testkit clock hook. Deferred: 4c. Descoped: 4a.

- **One production-tree change — the clock hook.** `MaintenanceClock` gained `advance(&self, Duration)` (`ManualMaintenanceClock` → real advance; `RealMaintenanceClock` → **no-op**, so it can never block a threaded runtime), and `StorageRuntime::advance_maintenance_clock_for_test(Duration) -> bool` routes through the existing slot dispatch. Gated `#[cfg(any(test, feature="fault-injection"))]`, `pub(crate)`, behavioral name — compiled out of release. The runtime hook lives in the lifetime-generic `impl<'a> StorageRuntime<'a>` (not the `<'static>` block) so it is callable on a borrowed-backend runtime.
- **4b driver.** Opens durable on an **owned** `local_fs` backend under `DeterministicInline` (the production `Background` path, inline executor + manual clock, no worker threads). A seeded `SplitMix64` drives a step loop over {commit, drain-maintenance, enqueue flush/checkpoint/snapshot-pruning, advance-clock, no-op}. **Per-step safety** reuses the recovery oracle under `ZeroLoss` (a clean backend has nothing in-doubt, so the live scan must equal `model.live_state_at(last_acked)` exactly) — asserted after *every* step, catching any maintenance that transiently corrupts visible state. **Liveness:** no maintenance failure per step; at quiesce the queue drains to empty, admission is not blocked, and the oracle still holds.
- **Key substrate finding (corrected from the draft pseudocode):** under `DeterministicInline` the inline executor owns the maintenance queue, so `run_next_maintenance` serves the *empty manual queue* (returns `None`); enqueued work is driven by **`drain_maintenance`**. The interleaving knob is therefore *when enqueued maintenance is drained relative to client commits*, crossed with seeded clock advancement — faithful production scheduling, no task-reorder hook, no false positives. The budget is **Default** (the `LowMemory` profile's 16 KB frozen-mutable pool starves maintenance — `StorageBudgetExceeded` on rotation — and is not a realistic interleaving regime).
- **4d replay + soak.** `SimFacts` (`Clone+Debug+PartialEq`, excludes all timing numbers) captures the action trace, commit versions, queue trajectory, maintenance-completed, and final live state; the in-module `same_seed_replays_bit_exact` test is the determinism guard. `run_simulation_harness(root, case_limit)` scales seeds with the case budget; `tests/simulation_smoke.rs` is the CI-fast smoke + an `#[ignore]` soak honoring `STRATA_STORAGE_FAULT_CASES`. Non-vacuousness asserted: maintenance completed > 0 and the manual clock advanced > 0.
- **4a descoped.** The residual `Instant::now()` are perf-trace *durations* only — not state-affecting — and `SimFacts` excludes timing numbers, so replay is bit-exact without routing them through the clock. Revisit only if a future fact asserts on timing.
- **4c — the fault/crash dimension** (`src/testkit/simulation/faults.rs` + `tests/simulation_faults.rs`). Crosses the seeded interleaving (commits + maintenance drained at a sim-chosen cadence, on the borrowed `EvaluateAndEnqueue` path) with two crash substrates: a seed-chosen **STH-2 backend-op fault** (op × call-number × Once/Continuously × Unavailable/NoSpace — verified loss-free, the faulted commit in-doubt) and a seed-chosen **STH-3 power-loss crash** (FsModel × crash point × durability — `Always` loses nothing, `Standard` a clean prefix, garbage tail fail-loud-or-prefix). The interleaving exercises all four fault ops (snapshot-pruning drives the `DeleteObject` fault) and all four FS models, each oracle-verified. `run_fault_simulation_harness` runs one fault + one crash case per seed; seed-scaled `#[ignore]` soak. **The DST immediately did its job: the soak found a silent durability bug** — a `PublishObject` NoSpace fault during a batched `[Checkpoint, Flush]` drain left the flush's L0 table installed but its manifest unpublished while the checkpoint advanced the WAL-replay floor past those rows, so a clean reopen recovered nothing (the failure was swallowed; the drain returned `Ok`). **Now fixed (2026-06-18):** a checkpoint defers while a table-manifest publish is outstanding (shared catalog `manifest_publish_pending` flag, checked in both the background and synchronous checkpoint paths); the regression (`regression_publish_fault_during_checkpoint_flush_loses_no_data`) is un-ignored + green for all positions, with a self-healing companion (`transient_manifest_publish_failure_defers_then_resumes_checkpoint`). See `sth-4-finding-checkpoint-flush-publish-fault.md` (Resolution). **With seed 74 fixed the soak reached seed 155 and surfaced a separate, pre-existing power-loss `Gap`** (SplitRename/Standard, no injected fault), now **fixed (2026-06-19)**: the checkpoint records its delta base floor durably (`flushed_through_commit_id`) and recovery requires the table-manifest base, recovering a clean prefix as `DataLoss` (`sth-4-finding-splitrename-power-loss-gap.md`, Resolution). **Both regressions green; the 3000-seed soak runs clean end-to-end — class 9 closed.**

## Implementation detail

### 4a — Finish clock injection (`src/lifecycle/...`)
Replace the residual `std::time::Instant::now()` duration measurements with
`clock.now()` so `inline_start.elapsed()`-style perf facts are reproducible. Pure
seam completion; no behavior change. After this, *both* state and timing replay
deterministically.

### 4b — Simulation driver (`src/testkit/simulation/driver.rs`)
Open a runtime with `InlineMaintenanceExecutor` + `ManualMaintenanceClock`. A
seeded `SimRng` (SplitMix64) drives a step loop:
```
loop {
    match rng.choice(&[AdvanceClock, RunNextTask, ClientOp, Quiesce]) {
        AdvanceClock => clock.advance(rng.jitter()),
        RunNextTask => executor.run_one(rng.pick_pending_task()),  // order is the interleaving
        ClientOp     => apply_and_record(rng.gen_commit_or_branch_op()),  // feeds STH-1 model
        Quiesce      => break,
    }
    assert_safety(oracle);        // invariants hold mid-flight, not just at end
    assert_liveness(progress, bounded_resources);
}
```
The interleaving freedom is `pick_pending_task` (which queued maintenance runs
next) crossed with clock advancement — exactly the rare orderings nothing else
reaches. Safety = STH-1 oracle (no data loss / phantom); liveness = queue drains,
WAL bounded, no permanent commit failure.

### 4c — Fault dimension (`src/testkit/simulation/faults.rs`)
The seed also arms the STH-2 fault backend at sim-chosen points (mid-publish,
mid-compaction, mid-recovery). The sim then crosses *interleaving × fault* — the
combination space that defines class 9. After a fault-induced crash, reopen and
run the oracle, then resume the sim.

### 4d — Replay + soak (`tests/simulation_smoke.rs`, soak target)
The whole trajectory is a pure function of the seed. On any assertion failure,
print the seed; a `replay(seed)` test re-runs it identically (regression seed).
CI runs a bounded seed budget in seconds; nightly runs the soak.

## Constraints

- One seed → one trajectory, always. No wall-clock, `Math.random`, or thread
  nondeterminism in the sim path (the seams guarantee this; 4a closes the last gap).
- Drives the **production** path (inline executor on the real `Background` logic),
  not a parallel simulation of it.
- Typed assertions; behavioral names; seeds are the only "magic numbers."

## Exit gate

**Delivered by 4b + 4c + 4d (2026-06-18):**
- Seeded driver sweeps client-op × maintenance-cadence × clock interleavings over
  the production path (4b); the fault-combination dimension crosses it with the
  STH-2/STH-3 fault and crash substrates (4c); every step safety- (recovery oracle)
  and liveness-checked, with bit-exact replay (the `same_seed_replays_bit_exact`
  determinism guard) + CI-fast smoke + `#[ignore]` soaks (4d). clippy
  `--all-features --all-targets -D warnings` + fmt clean; full `--lib` + the STH-1/2/3
  integration targets stay green.

**Class 9 closed (2026-06-19) — the DST did its job twice:**
- The 4c fault soak surfaced a **silent durability bug** (publish fault during
  checkpoint+flush). **Fixed (2026-06-18)** — checkpoint defers on outstanding
  table-manifest publish debt; regression un-ignored + green
  (`sth-4-finding-checkpoint-flush-publish-fault.md`, Resolution).
- With that fixed, the soak progressed and surfaced a **separate, pre-existing
  power-loss `Gap` at seed 155** (SplitRename/Standard, no injected fault). **Fixed
  (2026-06-19)** — the checkpoint records its delta base floor durably and recovery
  requires the table-manifest base, recovering a clean prefix as `DataLoss`; regression
  un-ignored + green (`sth-4-finding-splitrename-power-loss-gap.md`, Resolution).
- **Both regressions green; the 3000-seed fault-simulation soak runs clean end-to-end.**

**Descoped:**
- 4a residual clock injection — perf-trace durations only; not needed for replay;
  revisit only if a fact asserts on timing numbers.
