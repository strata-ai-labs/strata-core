# STH-4 Implementation Plan: Deterministic Simulation Driver (DST)

Status: draft
Charter class: 9 — Rare-interleaving / fault-combination bugs (🟡 Partial → ✅)
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

- Seeded driver sweeps interleavings + fault combinations over the production
  path; every step safety- and liveness-checked.
- Failures replay bit-exact from a printed seed; nightly soak runs.
- Residual clock injection complete (state *and* timing reproducible).
- Charter class 9 flips 🟡 → ✅ with this plan as evidence.
