# M4P-L8B Test Plan: Lifecycle Maintenance Follow-Up Parity

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`

Parent lifecycle test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove or explicitly defer the remaining lifecycle maintenance parity gaps that
can affect sustained 5M/10M L9 loads.

This plan treats unrecorded semantic differences as failures. A gap can close in
one of two ways:

1. implementation restores the old mechanical behavior and tests prove it; or
2. the plan records a V1 decision, counters prove the behavior is safe for the
   benchmark, and a later owner is linked if broader parity is still needed.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Compaction shape policy | Nonzero targets, input choice, and overlap handling are deterministic and documented. | Flat LSM pyramid, table-zero fallback, or hidden output-drag regression. |
| Maintenance coverage | Quiet branch backlog is discovered or explicitly deferred with counters. | Only the committing branch receives automatic maintenance. |
| Pressure/admission | Mutable bytes, sampling, and stall/wake decisions are measurable and typed. | Large active state reports healthy pressure or callers grow ad hoc retry loops. |
| Resource control | Compaction IO and post-flush memory behavior are measured or bounded. | Maintenance starves writes or long loads retain avoidable memory. |
| Snapshot/pruning | Automatic maintenance cannot prune beyond retained snapshots without proof. | Source-shape maintenance silently becomes history-pruning maintenance. |
| Benchmark closeout | 5M/10M source fanout stays bounded without manual drain. | Follow-up gaps remain hidden until point-read measurement. |

## Semantic Decision Register Tests

Every decision test should assert that the decision is recorded in the lifecycle
architecture or implementation plan before weakening an old-engine oracle.

Required decisions:

1. **Nonzero input policy**
   - Allowed outcomes: compact-pointer/round-robin restored, or
     deterministic largest-input recorded as V1.
   - If largest-input is chosen, the decision must record byte-count
     descending, row-count descending, and the final stable table-index
     direction.
   - Current decision: deterministic largest-input; final tie chooses the lower
     table index.
   - Test failure: selected index is always zero without a decision.
2. **Grandparent-overlap ownership**
   - Allowed outcomes: L8 passes split-budget facts, or L5/L6 owns output
     split budgeting with a linked follow-up.
   - Current decision: lower layers own split budgeting; L8 records
     deeper-overlap bytes and deferred split-budget counters.
   - Test failure: deeper-overlap bytes are ignored without a decision.
3. **Write-stall policy**
   - Allowed outcomes: fail-fast retryable, bounded wait, or L9-owned retry.
   - Test failure: multiple call sites introduce untracked retry loops.
4. **Pressure-clear wake policy**
   - Allowed outcomes: wake signal exists for bounded waits, or fail-fast
     retry remains the documented policy.
   - Test failure: waiters can block without a wake or callers must poll
     without an owned policy.
5. **Snapshot-floor ownership**
   - Allowed outcomes: L8 owns advancement, L9 owns proofs, or pruning remains
     explicit.
   - Test failure: automatic maintenance advances retention floor implicitly.
6. **Urgent admission policy**
   - Allowed outcomes: bounded inline maintenance before admission, or
     always-accept-under-pressure recorded as V1.
   - Test failure: urgent pressure silently accepts without inline-drive facts
     or an explicit accept-under-pressure decision.

## Compaction Shape Policy Tests

Coverage for PF1, PF2, and PF12.

Correctness tests:

1. Level target facts are level-specific; L1 and deeper levels do not all use
   the same target unless a fixed-target decision is recorded.
2. Level target growth is deterministic for a given config.
3. Adaptive target recalculation, if implemented, is stable across repeated
   pressure collection with unchanged branch shape.
4. A branch with L0 and nonzero pressure chooses the highest-scored level using
   level-specific target facts.
5. A queued nonzero compaction uses the selected current input table for that
   level, never a hardcoded table-zero fallback.
6. If compact-pointer or round-robin is implemented later, repeated eligible
   compactions advance the selected index deterministically.
7. If largest-input is the policy, fixtures with unequal table sizes choose the
   largest table and tie-break by the recorded byte-count, row-count, and
   stable-index order.
8. Grandparent-overlap fixtures either produce bounded output split facts or
   record an explicit lower-layer deferral.
9. Metadata-promotion fixtures remain eligible independently from
   grandparent-overlap split budgeting.
10. Reads, scans, history, tombstones, and TTL-visible rows match the model
    after every scheduled compaction.

Mechanical counter tests:

1. Level target evaluations by level and target bytes increment once per
   inspected level.
2. Selected nonzero table index is recorded for every nonzero compaction.
3. Largest-input selections increment the configured policy counter.
4. Deeper-overlap bytes are recorded when overlap facts exist.
5. Output split budget applied/deferred counters match the semantic decision.

Generated tests:

1. Random level sizes around each target threshold.
2. Random table sizes within one nonzero level.
3. Random deeper-overlap layouts.
4. Random mixed L0/nonzero pressure where the selected level changes after one
   rewrite.

Pass gates:

1. Nonzero-level source fanout stays bounded by level count and configured
   shape.
2. No queued compaction path hardcodes table zero.
3. Any intentionally missing old behavior has a named owner and benchmark
   safety proof.

## Maintenance Coverage And Chaining Tests

Coverage for PF3 and PF4.

Correctness tests:

1. A committing branch with no backlog and a quiet branch with frozen backlog
   schedules or records coverage work for the quiet branch.
2. A quiet branch with L0 backlog is discovered without requiring another
   commit on that branch.
3. A quiet branch with nonzero backlog is discovered and scored against active
   branch work.
4. A quiet branch with inherited-layer backlog is eligible for materialization
   coverage.
5. Flush work preempts compaction for every covered branch.
6. Coverage pass coalesces duplicate branch/scope tasks.
7. Coverage does not enqueue ordinary work while close-required drain or close
   owns lifecycle state.
8. Idle-round anchoring, if implemented, declares its trigger model: next
   mutating commit on any branch, next coverage pass, explicit
   health-collection/soak-mode call, or optional background loop.
9. Idle-round anchoring, if implemented, runs no more than the configured bound
   under the declared trigger model.
10. Idle-round anchoring, if deferred, records the deferral and exposes counters
    that would trigger re-evaluation.

Mechanical counter tests:

1. Coverage scans increment when the policy runs.
2. Branches-scanned equals the deterministic branch list size.
3. Quiet-branch-pressure counter increments for stranded backlog.
4. Coverage-enqueued and coverage-coalesced counters match queue outcomes.
5. Chain-stop reason counters distinguish healthy, idle-limit, queue-full, and
   failure.

Generated tests:

1. Random branch counts from 1 through at least 64.
2. Random backlog placement across active and quiet branches.
3. Random close/maintenance interleavings.
4. Random enqueue capacity faults during coverage.

Pass gates:

1. Quiet branch backlog cannot remain invisible without an explicit V1
   deferral.
2. Queue depth remains bounded under multi-branch coverage.
3. Branch ordering is deterministic.

## Pressure And Admission Contract Tests

Coverage for V3, PF7, PF8, PF9, and PF11.

Correctness tests:

1. Active mutable byte growth changes pressure severity before frozen-table
   thresholds are crossed.
2. Active bytes and frozen bytes are reported as distinct pressure facts.
3. Pressure collection counters report branches, levels, tables, and elapsed
   time.
4. Sampling, if enabled, never hides blocking pressure after a mutating commit.
5. Urgent pressure follows the recorded policy: bounded inline maintenance
   before admission when configured, or accepted-under-pressure facts when V1
   always-accept is selected.
6. If urgent inline maintenance is implemented outside the core pressure
   classifier, runtime-level tests assert the wrapper records inline-drive
   facts in commit admission summaries.
7. Fail-fast pressure rejection returns a typed retryable error before avoidable
   commit allocation.
8. Bounded wait, if implemented, respects max wait time and returns typed
   timeout facts.
9. Pressure-clear wake, if implemented, wakes waiters only after maintenance
   actually clears pressure.
10. Cleared-prior-rejection facts are visible on retry when fail-fast remains
    the policy.
11. Branch guard and unresolved-durable gate failures still take precedence
   where L7 ordering requires them.

Mechanical counter tests:

1. Pressure collection calls and level iterations increment under sustained
   writes.
2. Sampling skip/full-scan counters match configured interval.
3. Active-byte pressure observation counters match threshold classifications.
4. Urgent inline admission attempts and urgent accept-under-pressure counters
   match the configured urgent policy.
5. Wait attempts, wait timeouts, and wake counters are zero when fail-fast is
   configured.
6. Cleared retry counters increment after maintenance clears a prior rejection.

Generated tests:

1. Random active byte growth with delayed rotation.
2. Random pressure oscillation around background, urgent, and blocking
   thresholds.
3. Random sampling intervals.
4. Random retry/wait timeout budgets.

Pass gates:

1. Slowly growing active state cannot report healthy pressure indefinitely.
2. The admission contract is one documented policy, not mixed ad hoc behavior.
3. Measurement overhead is visible before any sampling optimization is accepted.

## Resource Throttling And Release Tests

Coverage for PF5 and PF6.

Correctness tests:

1. Compaction records input bytes, output bytes, elapsed time, and
   metadata-promotion bytes avoided.
2. Throttle policy, if enabled, defers compaction with retryable maintenance
   facts rather than failure facts.
3. Flush pressure can preempt or bound compaction work under the configured
   policy.
4. Compaction IO budget is consumed deterministically across repeated tasks.
5. Post-flush active/frozen byte counters drop when frozen state drains.
6. Memory-release hook, if implemented, runs only after flush state is no
   longer visible through the branch.
7. Memory-release deferral, if chosen, records the measurement threshold for
   re-evaluation.

Mechanical counter tests:

1. Compaction IO bytes per committed row are emitted in benchmark snapshots.
2. Throttle wait/defer counters are zero when throttling is disabled.
3. Throttle wait/defer counters are nonzero in constrained-budget fixtures.
4. Flush-preempted-compaction counter increments under simultaneous flush and
   compaction pressure.
5. Post-flush retained-byte counters are emitted for long-load fixtures.

Generated tests:

1. Random compaction output sizes under small IO budgets.
2. Random flush and compaction pressure overlap.
3. Random metadata-promotion versus rewrite candidates.
4. Long repeated flush cycles with retained-byte measurement.

Pass gates:

1. 5M/10M load time is not dominated by unbounded compaction IO.
2. Flush progress is not starved behind compaction.
3. Memory release remains measure-first unless portable evidence justifies a
   hook.

## Snapshot And Pruning Ownership Tests

Coverage for PF10.

Correctness tests:

1. Automatic flush does not advance snapshot floor.
2. Automatic compaction does not prune retained history without an explicit
   retention proof.
3. Materialization does not prune source history beyond retained snapshot
   bounds.
4. If L8 owns floor advancement, every advancement records old floor, new
   floor, owner, and proof.
5. If L9 owns floor advancement, lifecycle rejects implicit advancement and
   accepts only caller-supplied proofs.
6. Recovery preserves the chosen floor facts.

Mechanical counter tests:

1. Snapshot-floor advancement counter remains zero when pruning is explicit.
2. Pruning-with-proof counters match requested retention operations.
3. Rejected implicit pruning counters increment in negative tests.

Generated tests:

1. Random retained snapshots around compaction windows.
2. Random historical reads before and after maintenance.
3. Random pruning proofs with stale, future, and exact floors.

Pass gates:

1. Source-shape maintenance and pruning maintenance remain distinguishable.
2. Automatic maintenance cannot silently delete history protected by retained
   snapshots.
3. Ownership decision is documented before benchmark closeout claims pruning
   parity.

## Benchmark Tests

Run the L9 scale benchmark after L8B-A, L8B-B, and L8B-D have either
implemented the required mechanics or recorded benchmark-safe deferrals.

Required command:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache,standard \
  --workloads load-seq,point-latest,point-throughput \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --progress
```

Required benchmark fields:

1. per-level target bytes;
2. per-level table counts and bytes;
3. selected nonzero input policy;
4. maintenance coverage scans and quiet-branch hits;
5. pressure collection calls and level iterations;
6. active/mutable byte pressure;
7. compaction input/output bytes;
8. throttle wait/defer counts;
9. snapshot-floor/pruning counters;
10. point-source probes per read.

Benchmark pass conditions:

1. 5M and 10M runs reach point-read phases without benchmark-only source-shape
   drains.
2. Final L0 table count and nonzero-level probe count do not grow linearly with
   row count.
3. Load time is not dominated by final fixed-point compaction.
4. Compaction IO counters explain any remaining load slowdown.
5. Any deferred gap has counters showing it was not exercised by the benchmark
   or did not affect source fanout.

## Source Guards

Source guards must reject:

1. production lifecycle imports of product, benchmark, engine, IPC, or app
   modules;
2. benchmark-only flags in lifecycle production code;
3. direct lower-layer private table merge internals from L8;
4. roadmap labels in production Rust code, comments, panic messages, fixture
   bytes, or user-visible strings;
5. automatic pruning without a retention proof or ownership decision;
6. hardcoded nonzero compaction table index selection in queued maintenance
   execution.

## Verification Commands

Focused:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
```

Full storage-next:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

## Closeout Checklist

L8B test closeout requires:

1. all semantic decisions recorded;
2. all focused tests pass;
3. source guards pass;
4. generated tests include multi-branch, pressure, and compaction-shape cases;
5. benchmark JSON for 100K, 1M, 5M, and 10M is stored;
6. 5M/10M runs show bounded source fanout without manual drains;
7. any remaining deferral has a trigger counter and named owner.
