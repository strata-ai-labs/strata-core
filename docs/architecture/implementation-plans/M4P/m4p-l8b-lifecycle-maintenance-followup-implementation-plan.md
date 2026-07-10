# M4P-L8B Implementation Plan: Lifecycle Maintenance Follow-Up Parity

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

Architecture context:
`docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

Audit context:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
3. `docs/architecture/implementation-plans/M4P/m4p-l8-automatic-maintenance-scheduling-followup.md`

## Objective

Close the lifecycle maintenance gaps that remain after the first L8 parity
pass, so the 5M/10M L9 benchmark can be treated as a full maintenance parity
proof instead of only a proof of the immediate post-commit scheduler.

The first L8 pass restored automatic scheduling, flush draining, scored
compaction, write-admission pressure facts, and benchmark diagnostics. L8B
covers the old-engine mechanics that still determine sustained-load behavior:

1. compaction shape policy;
2. cross-branch maintenance coverage and chain continuation;
3. pressure/admission contract and measurement;
4. compaction IO and memory resource controls;
5. snapshot-floor and pruning ownership.

## Scope Summary

| Group | Gaps Covered | Required Before Benchmark Closeout | Primary Decision |
| --- | --- | --- | --- |
| L8B-A. Compaction Shape Policy | PF1, PF2, PF12 | Yes for PF1; decision for PF2/PF12 | Restore old shape policy or record V1 simplification with proof. |
| L8B-B. Maintenance Coverage And Chaining | PF3, PF4 | Yes for PF3; decision for PF4 | Add quiet-branch coverage or explicitly defer with counters. |
| L8B-C. Pressure And Admission Contract | V3, PF7, PF8, PF9, PF11 | Urgent policy decision plus measure-first | Decide urgent inline-drive versus accept-under-pressure, then keep fail-fast retry or add bounded wait/wake primitives. |
| L8B-D. Resource Throttling And Release | PF5, PF6 | Yes for PF5; measure-first for PF6 | Add IO throttle or prove compaction does not starve writes. |
| L8B-E. Snapshot And Pruning Ownership | PF10 | Decision required | Assign snapshot floor ownership before automatic pruning broadens. |

## Existing Baseline And Closure Targets

Assume the following first-pass L8 behavior exists:

1. mutating commits evaluate lifecycle pressure;
2. post-commit maintenance is enqueued/coalesced;
3. flush drain can drain all eligible frozen tables for a selected scope;
4. compaction tasks are scored and can chain;
5. benchmark output includes queue, L0, source-probe, and maintenance timing
   diagnostics.

L8B must still close or record decisions for these first-pass edge cases:

1. queued compaction execution must continue to avoid any hardcoded
   table-zero fallback. The branch-aware scored/current request path is the
   intended execution path; any remaining direct conversion helper use must be
   audited or replaced when it can run queued nonzero maintenance.
2. urgent admission must have an explicit policy. Deterministic inline runtimes
   may drive one bounded suggested task before admission, but
   always-accept-under-pressure is also valid only if recorded as the V1
   semantic decision with counters.

If any of those regress while implementing L8B, stop and restore the first-pass
invariant before adding broader parity mechanics.

## Non-Goals

L8B must not implement:

1. public product retry UI or user-facing maintenance commands;
2. a required background thread;
3. new L5 row merge algorithms;
4. new durable byte formats;
5. distributed or multi-process scheduling;
6. benchmark-only maintenance shortcuts;
7. automatic pruning without a snapshot-floor ownership decision.

## L8B-A. Compaction Shape Policy

Gaps covered: PF1, PF2, PF12.

Goal: make scheduled compaction produce an old-engine-like level shape, or
record explicit semantic decisions where storage-next intentionally differs.

Implementation decisions for this slice:

1. Nonzero level targets use a static deterministic pyramid: L1 = 64 MiB and
   each deeper nonzero level multiplies the previous target by 10. Adaptive
   target recalculation remains deferred until a benchmark shows the static
   pyramid is not enough.
2. Nonzero queued compaction uses deterministic largest-input selection rather
   than compact-pointer rotation. The tie-breaker is byte count descending, row
   count descending, then lower table index first. Stateless maintenance-task
   conversion is L0-only; nonzero queued execution must inspect current branch
   state.
3. Grandparent-overlap output split budgeting remains owned by the lower table
   and branch compaction layers. Lifecycle records deeper-overlap bytes and an
   output-split-budget-deferred counter, but does not infer split budgets from
   metadata-promotion eligibility.

Tasks:

1. Replace fixed nonzero-level target bytes with level-specific target bytes.
   - Define the base target and growth factor in lifecycle or branch compaction
     config.
   - Preserve deterministic defaults for tests.
   - Expose target facts in diagnostics and perf trace.
2. Add a recalculation step for level targets from current branch/table shape
   if the old engine behavior requires adaptive targets rather than static
   per-level constants.
3. Decide nonzero input rotation policy.
   - Option A: restore compact-pointer or round-robin advancement.
   - Option B: record deterministic largest-input as the V1 policy.
   - If largest-input is chosen, record the exact tiebreaker order:
     byte count descending, row count descending, then a stable table-index
     direction. The implementation and tests must agree on whether lower or
     higher index wins the final tie.
   - Either option must forbid hardcoded table-zero fallback in queued
     maintenance execution.
4. If rotation is restored, persist or derive enough branch-local state to make
   the next selected table deterministic across retries.
5. Decide grandparent-overlap split ownership.
   - If L8 owns it, include deeper-overlap bytes in compaction request planning
     and pass a split budget to lower layers.
   - If L5/L6 owns it, record the ownership decision and add lower-layer
     follow-up links.
6. Keep metadata-promotion eligibility separate from output split budgeting;
   overlap used for one must not silently imply the other.
7. Update perf-trace counters:
   - level target evaluations by level and target bytes;
   - selected nonzero table index;
   - rotation cursor advances or largest-input selections;
   - deeper-overlap bytes considered;
   - output split budget applied or deferred.

Exit gates:

1. The 5M/10M benchmark cannot fail from an L1/L5 same-target pyramid collapse.
2. Nonzero compaction input selection is deterministic and documented.
3. If always-largest remains the policy, the test plan no longer expects
   round-robin variation.
4. Grandparent-overlap split behavior is implemented or explicitly assigned to
   L5/L6 with a blocking follow-up if benchmark counters show drag.

## L8B-B. Maintenance Coverage And Chaining

Gaps covered: PF3, PF4.

Goal: prevent automatic maintenance from depending only on the currently
committing branch.

Implementation decision: the V1 trigger is the next successful mutating commit
on any branch. The committing branch is handled by the existing post-commit
scheduler, then the coverage pass scans the deterministic live branch list and
queues current pressure suggestions for quiet branches. Idle rounds are
consecutive coverage passes with no eligible quiet-branch work, capped at five;
there is no implicit background scheduler clock in this slice.

Tasks:

1. Add a maintenance coverage pass that can discover quiet branches with:
   - frozen table backlog;
   - L0 backlog;
   - nonzero-level backlog;
   - inherited-layer backlog.
2. Decide when the coverage pass runs:
   - after post-commit scheduling;
   - after a chain reaches local health;
   - during explicit health collection;
   - or under a bounded periodic/idle policy.
3. Preserve flush-before-compaction ordering for every branch selected by the
   coverage pass.
4. Add an idle-round chain anchor if needed.
   - Old storage allowed several idle rounds before stopping maintenance.
   - Storage-next has no implicit scheduler clock, so the trigger model must
     be explicit before implementation:
     - next mutating commit on any branch;
     - next coverage pass;
     - explicit health-collection/soak-mode maintenance call;
     - or a later optional background loop.
   - The V1 in-process model should count idle rounds as consecutive coverage
     passes with no eligible work, not elapsed wall-clock time.
   - If idle rounds are deferred, record which trigger would re-evaluate the
     deferral.
5. Ensure coverage scheduling coalesces by branch/scope and cannot create an
   unbounded duplicate queue.
6. Ensure coverage never starts ordinary maintenance while close-required drain
   or closing state owns the lifecycle.
7. Record counters:
   - coverage scans;
   - branches scanned;
   - quiet branches with pressure;
   - coverage tasks enqueued/coalesced;
   - idle rounds consumed;
   - chain stops due to healthy, idle-limit, queue-full, or failure.

Exit gates:

1. A quiet branch with backlog is either maintained automatically or reported
   as a documented V1 deferral with counters.
2. Multi-branch maintenance remains deterministic.
3. Queue depth remains bounded under branch-count scale tests.
4. Close and quiesce semantics remain unchanged.

## L8B-C. Pressure And Admission Contract

Gaps covered: V3, PF7, PF8, PF9, PF11.

Goal: make pressure evaluation complete enough to drive admission decisions
without forcing L9 product policy into L8.

Tasks:

1. Add measure-first counters for pressure collection:
   - collection calls;
   - branches inspected;
   - levels inspected;
   - tables inspected;
   - collection nanoseconds;
   - skipped expensive scans if sampling is enabled.
2. Decide whether to restore old expensive-check sampling.
   - If yes, add a configurable interval and prove pressure facts remain safe
     between full scans.
   - If no, record that per-commit collection is acceptable and keep counters
     as the guard.
3. Add active/mutable byte pressure facts.
   - Count active rows and active byte estimates separately from frozen tables.
   - Define background, urgent, and blocking thresholds.
   - Ensure slowly growing active state cannot report healthy pressure forever.
4. Decide urgent admission policy.
   - Option A: drive bounded inline maintenance before admission when a
     suggested task exists and the runtime policy allows deterministic inline
     work.
   - Option B: always accept urgent pressure with typed
     accepted-under-pressure facts.
   - Whichever option is chosen must be recorded in the semantic decision
     register and must be visible in commit admission summaries.
   - If the inline branch is implemented outside the core pressure classifier,
     runtime-level tests must cover it directly.
5. Decide write-stall behavior.
   - Option A: keep fail-fast retryable rejection.
   - Option B: add bounded wait with a time budget.
   - Option C: add a lower-level wait primitive but leave public policy to L9.
6. Decide pressure-clear wake behavior.
   - If bounded waits exist, add a condition/event signal when maintenance
     clears pressure.
   - If fail-fast remains, document why cleared-prior-rejection-on-retry is
     sufficient.
7. Keep branch guard and unresolved-durable gate failures distinct from
   pressure rejection.
8. Record counters:
   - active-byte pressure observations by severity;
   - urgent inline admission attempts;
   - urgent accept-under-pressure admissions;
   - sampling skips/full scans;
   - wait attempts;
   - wait timeouts;
   - pressure-clear wakes;
   - retries that observe cleared pressure.

Implementation decision for this slice:

1. Pressure collection stays unsampled for now. Every collection records calls,
   inspected branches/levels/tables, elapsed nanoseconds, and a full-scan
   counter. Sampling counters exist but remain zero until a later measured
   overhead problem justifies an interval.
2. Active mutable byte pressure derives thresholds from the branch active
   rotation size: background at one-half, urgent at three-quarters, and
   blocking at the rotation threshold. Active bytes and frozen bytes remain
   separate facts.
3. Urgent admission accepts under pressure. Deterministic-inline runtimes also
   attempt one bounded suggested maintenance task before admission when the
   pressure fact has a suggested task. Ordinary urgent active-byte pressure has
   no suggested task and records accepted-under-pressure without inline work;
   blocked active-byte rotation with frozen backlog carries a flush suggestion.
4. Write-stall remains fail-fast. Blocking pressure is retryable when the
   pressure fact has a maintenance task or queue-backlog reason that can clear
   the rejection. No bounded wait or pressure-clear wake primitive is added in
   this slice; wait and wake counters are present and must remain zero under the
   fail-fast policy. Cleared-prior-rejection-on-retry remains the V1
   pressure-clear fact.

Exit gates:

1. Pressure facts account for table shape and mutable-memory growth.
2. The caller contract is explicit: fail-fast retry, bounded wait, or deferred
   L9 policy.
3. Any sampling policy proves it cannot hide blocking pressure.
4. Existing L7 failure ordering remains intact.

## L8B-D. Resource Throttling And Release

Gaps covered: PF5, PF6.

Goal: keep maintenance from saturating IO or retaining memory in long runs.

Tasks:

1. Add compaction IO accounting:
   - input bytes read;
   - output bytes written;
   - metadata-only bytes avoided;
   - compaction elapsed time;
   - maintenance bytes per committed row.
2. Decide compaction IO throttle shape.
   - Token bucket;
   - byte budget per maintenance pass;
   - queue priority between flush and compaction;
   - or explicit no-throttle decision with benchmark proof.
3. Ensure urgent/blocking flush work is not starved behind large compaction
   rewrites.
4. Add budget facts to maintenance outcomes so throttled work is deferred with
   a retryable reason, not reported as failure.
5. Add memory-release measurement after flush.
   - Track active/frozen bytes before and after flush drain.
   - Track allocator/RSS facts only through portable test hooks where possible.
6. Decide whether a release-freed-memory hook belongs in storage-next or below
   it.
7. Record counters:
   - throttle waits or deferrals;
   - compaction IO budget consumed;
   - flushes preempting compaction due to pressure;
   - post-flush active/frozen bytes;
   - memory release attempts and observed retained bytes.

Exit gates:

1. 5M/10M load throughput is not dominated by unthrottled compaction IO.
2. Flush pressure can preempt or bound compaction work.
3. Memory retention after flush is measured before adding nonportable release
   hooks.

## L8B-E. Snapshot And Pruning Ownership

Gap covered: PF10.

Goal: record the semantic owner for snapshot-floor advancement before automatic
maintenance expands pruning.

Tasks:

1. Document current storage-next pruning behavior:
   - per-request retention proof;
   - no implicit safe-point advancement from maintenance;
   - no automatic snapshot-floor movement during flush.
2. Compare old engine behavior:
   - `set_snapshot_floor`;
   - `gc_safe_point`;
   - version-floor advancement before or during maintenance.
3. Decide ownership:
   - L8 owns lifecycle snapshot-floor advancement;
   - engine-next/L9 owns public snapshot lifecycle and passes proofs down;
   - or pruning remains explicit until a later parity slice.
4. Add a semantic decision entry with:
   - owner;
   - allowed callers;
   - proof shape;
   - durability/recovery requirements;
   - benchmark impact.
5. Add guardrails so automatic maintenance cannot prune history beyond
   retained snapshots without the chosen proof.

Exit gates:

1. Automatic maintenance cannot silently advance the retention floor.
2. The benchmark closeout distinguishes source-shape maintenance from pruning.
3. Any broader pruning work has an owner and test oracle.

## Execution Order

Recommended order:

1. L8B-C. Pressure And Admission Contract.
2. L8B-A. Compaction Shape Policy.
3. L8B-B. Maintenance Coverage And Chaining.
4. L8B-D. Resource Throttling And Release.
5. L8B-E. Snapshot And Pruning Ownership.

Reasoning:

1. L8B-C should be implemented as one slice. Within that slice, land pressure
   collection counters first, then mutable-byte pressure, urgent admission
   policy, and stall/wake decisions. Splitting those across top-level execution
   steps leaves the admission contract half-defined.
2. L8B-A should be implemented as one slice. Level targets, nonzero input
   policy, and grandparent-overlap ownership all affect compaction shape and
   should share one semantic decision record.
3. L8B-B follows because cross-branch coverage should schedule work using the
   finalized pressure and compaction-shape facts.
4. L8B-D follows once the scheduler knows what work it will drive; IO
   accounting can then justify a throttle or a no-throttle decision, and memory
   release remains measure-first within the same resource slice.
5. L8B-E remains last because snapshot/pruning ownership should not be mixed
   into source-shape maintenance until the mechanical maintenance path is
   stable.

## Stop Conditions

Stop and revise this plan if:

1. level-target fixes require changing L5 table format or L6 install
   semantics;
2. quiet-branch coverage requires a required background thread;
3. IO throttling needs product-visible scheduling policy;
4. bounded wait semantics conflict with L7 fail-fast branch guard or unresolved
   durable gate behavior;
5. snapshot-floor ownership cannot be decided without engine-next API design;
6. 5M/10M benchmark remains dominated by lower-layer table merge counters after
   source fanout is bounded.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
```

Full storage-next gates:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

Benchmark proof:

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

## Completion Criteria

L8B is complete when:

1. level targets and nonzero input policy are implemented or explicitly
   recorded as V1 decisions with benchmark proof;
2. quiet branches with maintenance pressure are covered or explicitly deferred
   with counters;
3. active/mutable byte pressure is represented or explicitly ruled out;
4. write admission has a documented fail-fast, bounded-wait, or L9-owned
   policy;
5. compaction IO is measured and throttled or proven not to starve writes;
6. memory release after flush is measured before adding any nonportable hook;
7. snapshot/pruning ownership is recorded;
8. 5M/10M benchmark source fanout and point-read probes are bounded without
   benchmark-only source-shape drains.
