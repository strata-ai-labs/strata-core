# STH-5 Implementation Plan: Failure-During-Failure (compound anomalies)

Status: implemented (2026-07-16, TCP1.2) — see "As built" below

## As built (2026-07-16, slice TCP1.2)

`crates/storage/src/testkit/compound_faults.rs` + `tests/compound_faults.rs`.
Slices 5a and 5c merged into one sweep (a single-position 5a case is just an
early position of the 5c grid):

1. **Fault-during-recovery sweep** (`run_compound_fault_recovery_sweep`) —
   stages the first failure as a faulted checkpoint publish (position computed
   from a baseline publish trace, so the store-creation manifest publish is
   not the one faulted) followed by a crash without close; traces every
   backend op the reopen performs; then sweeps the second fault across every
   traced position, once and continuously. Each case reopens a byte-copy of
   the crashed store (a successful reopen heals the directory, so cases never
   share one), asserts the faulted open either fails typed or succeeds
   oracle-valid, then proves a clean reopen is oracle-valid and accepts a
   resume commit.
2. **Fault-during-maintenance cases** (`run_compound_fault_maintenance_cases`)
   — measures per-op call-number windows for each maintenance transition
   (flush, checkpoint, compact, snapshot-pruning) on a clean baseline, then
   faults every write-side position inside each window. The typed-trace
   contract learned here: a maintenance fault surfaces through one of three
   channels — a drain error, a `Failed` summary with a source error code, or
   a **completed pass that records the sub-failure as a source error code**
   (the manifest-publish-debt path: the failed table-manifest publish becomes
   reserved debt, dependent checkpoints defer with "outstanding table-manifest
   publish debt", and a later flush republishes). A `Failed` summary without
   a source code, or a fired fault with no trace on any channel, fails the
   harness.
3. **CI tiers** — bounded grid runs per-PR via the in-crate unit tests; the
   full integration grid plus a 20,000-case seed-scaled soak runs in the
   nightly `failure-during-failure-soak` job (`STRATA_STORAGE_COMPOUND_CASES`
   sets depth). A 2,000-case local soak ran clean before landing; no product
   defect found — the publish-debt design held under compound attack.
Charter class: 6 — Failure-during-failure (❌ Missing → ✅)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: **STH-1** (oracle), **STH-2** (fault seams), **STH-3** (crash). Optionally composes **STH-4**.

## Objective

Break recovery *while it is recovering*. Inject a second fault during crash
recovery, during compaction, and during checkpoint, and prove the system either
completes to an oracle-valid prefix or fails with a *typed, resumable* error that
a subsequent clean attempt recovers from. This is the compound bug class — the
one that only appears when two unlikely things happen at once — and it is the
natural capstone once the oracle, fault sweeps, and crash realism exist.

## Why this matters (blog beat)

The worst data-loss bugs are not single failures; they are the I/O error that
arrives *during* the recovery from the previous crash, when invariants are
half-restored. SQLite tests this explicitly because that is where real corruption
hides. It is also the cheapest class to add *last* and the most expensive to add
*first*: it is almost entirely composition of the machinery STH-1 through STH-3
already built. This plan is the proof that the pieces compose — that StrataDB
stays consistent not just under failure, but under failure-during-failure.

## Seams to build on (all from prior STH plans)

- STH-1 recovery oracle — the post-condition after the compound event.
- STH-2 counting fault backend — to arm the *second* fault precisely during the
  recovery/compaction/checkpoint path.
- STH-3 crash harness + reordering backend — to stage the *first* crash.
- The durability contract: recovery is resumable; the WAL writer halts on fsync
  failure and resumes explicitly — so "fail then resume then succeed" is the
  contract under test.

## Coverage target (not line count)

Exit bar = "a fault injected during recovery, compaction, and checkpoint;
integrity and the recovery oracle still hold; the intermediate failure is a typed
resumable error." Measured by which in-flight phases are interrupted (recovery
replay, compaction publish, checkpoint publish) and that a clean retry after the
compound failure recovers oracle-valid.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 5a | Fault-during-recovery | Crash mid-workload; during the reopen's WAL replay / manifest read, fire a second fault; assert typed resumable error, then a clean reopen recovers oracle-valid |
| 5b | Fault-during-compaction / -checkpoint | Fire a fault inside the compaction-publish and checkpoint-publish transitions; assert no torn durable state; oracle-valid after resume |
| 5c | Double-fault sweep | Compose the STH-2 sweep with a staged first crash: sweep the *second* fault position across the recovery path, oracle each |

## Implementation detail

### 5a — Fault-during-recovery (`tests/compound_fault_recovery.rs`)
Stage: drive a durable workload, crash via STH-3 (drop or mid-publish). Arm the
STH-2 backend to fail the Nth backend op of the *reopen* path (WAL segment read,
manifest read, snapshot load). `open_local` must return a typed, resumable error
(class+code) — never a panic, never silent partial state. Then disarm, reopen
clean, and assert the STH-1 oracle: the second failure must not have advanced or
corrupted durable state.

### 5b — Fault-during-maintenance (`tests/compound_fault_maintenance.rs`)
Using the inline executor (STH-4 substrate) for determinism, drive a compaction
and a checkpoint; arm a fault inside the publish transition of each. Assert the
durable state is all-or-nothing (the half-published artifact is ignored or
quarantined per contract), and post-resume recovery is oracle-valid.

### 5c — Double-fault sweep (`tests/compound_fault_sweep.rs`)
Generalize: stage a first crash at a fixed point, then run the STH-2 *sweep* over
the second fault across the recovery path. Each (first-crash, second-fault-N)
pair is oracle-verified. Bounded in CI (scaled workload); larger nightly.

## Constraints

- Deterministic, seeded; both fault positions printed on failure.
- The intermediate failure must assert a *typed resumable* error class — the
  contract is "fail safe and resume," not "succeed regardless."
- Behavioral names only; reuses `testkit/` machinery — minimal new surface.

## Exit gate

- Faults during recovery, compaction, and checkpoint are covered; each leaves
  oracle-valid state after a clean resume; intermediate failures are typed and
  resumable.
- The double-fault sweep is green in CI and has a nightly extension.
- Charter class 6 flips ❌ → ✅ with this plan as evidence.
