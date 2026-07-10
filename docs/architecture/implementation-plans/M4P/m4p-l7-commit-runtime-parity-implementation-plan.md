# M4P-L7 Implementation Plan: Commit Runtime Parity

Status: implemented closeout; remaining work is tracked as explicit L8/L9
handoffs or V1 semantic decisions.

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Architecture context:
`docs/architecture/storage/l7-commit-runtime.md`

Timeline context:
`docs/architecture/storage/commit-timeline-substrate.md`

Audit context:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Serving-path context:
`docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

## Objective

Close storage-next L7 commit-runtime parity without absorbing the L5/L6 serving
path or L8 scheduler work.

L7 owns the internal storage commit unit: validation, version/timestamp
allocation, branch admission, WAL-before-visible ordering, atomic L6 apply,
visible-version publication, conflict validation, commit timeline rows, replay
rules, and commit facts consumed by L8/L9.

L7 does not own automatic maintenance scheduling, compaction scoring, write
stall policy, branch LSM source selection, public product transaction sessions,
or engine-specific side effects.

## Audit Findings

Primary audit section:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`,
`### L7. Commit Runtime`.

Important current reality: storage-next's unresolved-durable gate also
serializes all mutating commits globally and fails fast on contention. This is
not just weaker per-branch concurrency; it is a different caller contract. See
L7-D for the required semantic decision.

Findings to close or explicitly defer:

1. independent branch write concurrency is weaker than old per-branch commit
   concurrency because storage-next currently uses stronger global visible
   safety;
2. visible-version tracking no longer has old pending-version advancement
   machinery;
3. conflict-source construction can be expensive if blind writes or tiny
   read/CAS sets capture broad branch views unnecessarily;
4. durable commit payload serialization and row preparation can allocate more
   than the old WAL bridge;
5. commit timeline lookup is storage-backed but still needs proof that
   timestamp-to-version resolution does not scan user rows or unrelated
   timeline rows;
6. storage-next exposes cache-leaning internal defaults that need an explicit
   L7/L9 mapping decision;
7. branch registry lookup shape should be proven acceptable before large
   branch-count workloads depend on it;
8. quiesce is a fast-fail L7 primitive today; retry/deadline orchestration is
   L8, but the L7 primitive and facts must be tested.

Current delta status:

| Finding | Current status | Planning consequence |
| --- | --- | --- |
| Independent per-branch write concurrency | Closed as an explicit V1 semantic decision: current storage-next admits at most one mutating commit globally and fails fast on contention. | L8/L9 owns retry, deadline, and future admission policy. Do not loosen until pending-version or equivalent facts exist. |
| Pending-version visible advancement | Closed as an explicit V1 semantic decision: flat visible-version tracking plus unresolved gate replaces old `pending_versions` advancement and fixes the old WAL-durable apply-failure visibility leak. | Tests assert safer unresolved-durable blocking, not the old leak. |
| Conflict-source cost for blind writes | Closed. Blind writes and empty validation sets skip source capture; read/CAS facts build at most one source per commit that needs validation. | Keep perf-trace counters in place for regressions. |
| Durable payload/row allocation | Closed for correctness and measurement. One-pass row preparation and WAL encode counters exist; buffer pooling is a measured future optimization, not a parity blocker. | Revisit pooling only if durable-write profiles justify it. |
| Timeline lookup isolation | Closed for L7. Timeline rows live in `COMMIT_TIMELINE_SPACE`; lookup and reconciliation counters prove no user-row scan and bounded vector/index work inside the L7 view. | L6/L8 still own source-shape and timestamp-source planning outside the commit runtime. |
| Cache-leaning internal defaults | Closed. L9 runtime-default durability resolves from the opened runtime, durable paths construct explicit durability, and durable runtime rejects cache-only batches. | Source guards prevent durable production paths from using cache-default options accidentally. |
| Branch registry lookup shape | Closed as a documented V1 bound. Registry lookup remains descriptor-vector based, but branch-count perf counters and scale tests record descriptor probes. | Switch to indexed registry only if branch-count workloads exceed the documented envelope. |
| Quiesce primitive | Closed as an L7 primitive: fast-fail typed guard exists and is tested. | Retry/deadline and close orchestration stay in L8. |

Additional L7 deltas to track:

| Gap | Owner | Required plan action |
| --- | --- | --- |
| N1: global commit serialization through unresolved-durable admission | L7-D | Rewrite the finding as global fail-fast serialization, not merely weaker per-branch concurrency. Decide whether V1 accepts it or designs per-branch admission. |
| N2: same-branch and cross-branch commit contention changed from blocking to fail-fast | L7-G | Document caller retry contract or add a blocking variant. Tests must prove the returned error/fact shape. |
| N3: timeline fact reconciliation is nested and can be O(M^2) | L7-E | Reconcile sorted timestamp/version facts with a linear merge/zip pass. |
| N4: timestamp-to-version lookup is linear over retained timeline entries | L7-E | Use sorted entries and binary search or an equivalent indexed lookup. |
| N5: replay classifies each row with a history lookup | L7-D | Measure recovery replay row/source cost and bulk-classify rows if the cost scales with rows times sources. |
| N6: old explicit lock-ordering documentation is gone | L7-G | Restore acquisition-order documentation and guard tests before relaxing global serialization. |
| N7: old apply-failure pending-version behavior could advance visibility past a WAL-durable unapplied commit | Semantic decision | Record as an intentional correctness improvement; tests should assert unresolved-durable blocking. |
| N8: `CommitObservedVersion::Missing` replaces old version-zero missing sentinel | L7-C | Record as an intentional semantic improvement and test `Present(0)` rejection. |
| N9: cache applied-not-visible rows remain same-branch readable while blocking unsafe cross-branch advancement | Semantic decision | Document and test the dual cache-mode semantic. |
| N10: storage-next enforces per-batch mutation, validation-fact, and commit-row limits | L7-A | Test and document defaults; verify benchmark and API callers stay below them. |
| N11: old WAL serialization buffer pooling is absent | L7-F | Inspect WAL service allocation behavior, then port pooling only if counters justify it. |
| N12: read-only diagnostics can be disabled by config | L7-C | Test disabled behavior and confirm the L9 surface maps it intentionally. |

Related L9 audit section:
`Public read-set validation is not restored through L9`.

Related L8 audit section:
`Write-admission backpressure is diagnostic-only today`.

## Old Source Map

Old storage evidence:

1. `crates/storage/src/txn/context.rs`
   - staged writes, read-set facts, CAS facts, delete set, TTL map, write modes;
2. `crates/storage/src/txn/manager.rs`
   - commit version allocation, branch commit locks, quiesce, pending versions,
     visible-version tracking, branch deletion barriers;
3. `crates/storage/src/txn/validation.rs`
   - read-set and CAS validation;
4. `crates/storage/src/txn/lock_ordering.rs`
   - explicit commit-path lock ordering;
5. `crates/storage/src/durability/commit_adapter.rs`
   - WAL-before-storage bridge;
6. `crates/storage/src/durability/payload.rs`
   - old durable commit payload shape;
7. `crates/storage/src/segmented/mod.rs`
   - atomic versioned apply and recovery-specific timestamp preservation.

## Storage-Next Target Map

Primary L7 targets:

1. `crates/storage-next/src/commit/allocator.rs`
2. `crates/storage-next/src/commit/batch.rs`
3. `crates/storage-next/src/commit/branch_registry.rs`
4. `crates/storage-next/src/commit/cache.rs`
5. `crates/storage-next/src/commit/conflict.rs`
6. `crates/storage-next/src/commit/durable.rs`
7. `crates/storage-next/src/commit/durable_gate.rs`
8. `crates/storage-next/src/commit/facts.rs`
9. `crates/storage-next/src/commit/guard.rs`
10. `crates/storage-next/src/commit/outcome.rs`
11. `crates/storage-next/src/commit/replay.rs`
12. `crates/storage-next/src/commit/timeline.rs`
13. `crates/storage-next/src/commit/visibility.rs`

Integration targets:

1. `crates/storage-next/src/api/commit.rs`
2. `crates/storage-next/src/api/runtime.rs`
3. `crates/storage-next/src/lifecycle/cache.rs`
4. `crates/storage-next/src/lifecycle/durable/`
5. `crates/storage-next/src/lifecycle/recovery.rs`
6. `crates/storage-next/src/branch/state/append.rs`
7. `crates/storage-next/src/branch/read.rs`

## Preconditions

Required before behavior work:

1. L5/L6 serving-path counters exist and can explain point/table source work.
2. L6 branch apply and read-view mechanics are stable enough that L7 conflict
   validation can use them without broad row scans.
3. L4 WAL append behavior and durable fault windows are documented well enough
   for durable commit tests to classify WAL-before-visible failures.
4. L8 automatic scheduling is not required for L7 unit correctness; if a test
   requires scheduler retry/deadline behavior, move that test to L8.

## Non-Goals

This plan must not implement:

1. public begin/commit/rollback transaction sessions;
2. serializable isolation;
3. distributed commits or two-phase commit;
4. cross-branch atomic commit batches;
5. automatic flush, compaction, materialization, or retention scheduling;
6. write-stall enforcement or background maintenance policy;
7. branch LSM source planning, table selection, scan planning, or compaction
   picking;
8. WAL, table, manifest, or checkpoint byte-format changes;
9. engine/product side effects such as graph, vector, search, observer, or
   embedding updates.

If any of those become necessary, stop and move the work to the owning L8, L9,
L6, L5, L4, or engine plan.

## Correctness Rules

L7 parity must preserve these rules:

1. A mutating commit allocates exactly one commit version and one commit
   timestamp for every user row and commit timeline row in the batch.
2. Read-only diagnostic work does not allocate a commit version.
3. Blind writes do not construct conflict sources and do not conflict with
   concurrent blind writes except through branch admission/deletion/quiesce
   guards.
4. Read-set and CAS validation reject when supplied storage-shaped facts no
   longer match the current visible state.
5. A durable commit is not visible until the WAL append path has accepted the
   commit record under the requested durability policy.
6. A WAL-accepted commit that fails before visibility is classified as
   durable-but-not-visible and is replayable by L8 recovery.
7. A cache-mode apply that fails before visible publication is classified as
   applied-but-not-visible and blocks unsafe later visibility advancement.
8. Version gaps are allowed only when documented and tested as monotonic,
   non-dense versions.
9. Recovery replay preserves WAL commit version and timestamp, bypasses normal
   conflict validation, is idempotent, and catches the allocator above recovered
   versions.
10. Branch deletion, generation, and quiesce guards reject before visibility
    and before avoidable durable work.
11. L7 emits commit facts that L8/L9 can consume without importing L7 internals.

## Implementation Slices

### L7-A. Commit Runtime Audit Baseline And Counters

Goal: make L7 cost and phase movement visible before changing behavior.

Current state: the phase model is richer than old storage because commit
outcomes classify allocated-not-durable, durable-not-applied,
applied-not-visible, visible, and replay phases. The missing work is concrete
counters and assertions, not a new phase taxonomy.

Tasks:

1. Review current commit perf-trace coverage for:
   - validation source construction;
   - blind-write conflict-source skips;
   - conflict validation facts;
   - branch registry admission;
   - commit row preparation;
   - timeline row preparation;
   - WAL record build and append;
   - WAL payload allocation or buffer reuse;
   - visible publication;
   - unresolved durable gate admission;
   - branch registry descriptor scans;
   - timeline lookup probes and scanned rows.
2. Add missing counters only where they describe L7 mechanics, not L6 table
   reads or L8 maintenance scheduling.
3. Add phase-specific facts to `CommitOutcome` or diagnostics only when L9 can
   expose them without leaking WAL/table internals.
4. Record a baseline on blind writes and small read-set/CAS commits in cache
   and durable modes.

Exit gate:

1. Blind-write commits report zero conflict-source captures.
2. Read-set/CAS commits report exactly one conflict-source capture per commit
   when validation needs a source.
3. Row-preparation, timeline-row, WAL-build, visible-publish, and guard counters
   are visible in focused tests or L9 benchmark perf traces.
4. No new counter requires a production roadmap label in Rust identifiers,
   panic text, fixture bytes, or user-visible strings.

### L7-B. Blind-Write And Conflict-Source Fast Path

Goal: prove and, if needed, tighten the old blind-write fast path.

Current state: this appears implemented because conflict-source capture is
gated by `commit_conflict_validation_needs_source` before branch read-view
capture in cache and durable runtimes. Treat this slice as a proof-and-test
closure unless counters disprove the shape.

Tasks:

1. Keep `commit_conflict_validation_needs_source` ahead of branch read-view
   capture in cache and durable runtimes.
2. Ensure empty read-set/CAS facts validate without capturing branch sources.
3. Ensure malformed validation facts fail without broad source capture when the
   malformed shape is detectable locally.
4. For small read/CAS sets, validate against a pinned L6 read view with source
   work bounded by the number of facts and the L6 point-read source counters.
5. Preserve branch admission, unresolved durable gate admission, and generation
   guards before any avoidable source capture.

Exit gate:

1. Blind write perf counters show no read-view capture and no conflict-source
   build.
2. Small read/CAS validation probes no more rows/tables than the number of facts
   times the L6 point-read bound.
3. Per-fact validation cost is within 2x of the old `get_version_only` baseline,
   or the difference is explained by the safer visible-bound semantics.
4. Conflict outcomes match old read-set/CAS semantics.

### L7-C. Internal Read-Set/CAS Semantics And L9 Handoff

Goal: preserve internal validation semantics while preparing a storage-shaped
public mapping for L9.

Current state: direct L7 read-set/CAS semantics appear aligned with old
first-committer-wins validation. The remaining open work is the L9 handoff:
storage-shaped read facts should become reachable without exposing public
storage transaction sessions.

Tasks:

1. Audit `CommitValidationFacts`, `CommitReadFact`, `CommitCasFact`, and API
   `CommitCondition` mapping.
2. Preserve old first-committer-wins behavior for supplied read facts and CAS
   facts.
3. Record any public semantic reduction: storage-next should not claim public
   serializable transactions.
4. Define the exact L9 handoff for future storage-shaped read-set facts without
   adding product transaction sessions.
5. Keep product DTOs and engine-specific conditions above L9.
6. Record `CommitObservedVersion::Missing` versus old version-zero missing
   sentinel as a semantic decision, and keep `Present(0)` invalid.
7. Test disabled read-only diagnostics and document the L9 mapping for the
   typed disabled-diagnostics error.

Exit gate:

1. Direct L7 tests cover read-set and CAS conflicts, non-conflicts, deletes,
   tombstones, missing rows, and version gaps.
2. A L9 follow-up can expose storage-shaped read facts without changing L7
   internals.
3. Public transaction-session exposure remains absent.

### L7-D. Version, Visibility, And Pending-Durable Invariants

Goal: prove visible-version safety and decide whether old pending-version
machinery needs an equivalent.

Current state: storage-next intentionally uses a stronger global
unresolved-durable gate than old per-branch commit-lock plus pending-version
advancement. That tradeoff is acceptable only if it is recorded as a semantic
decision and tested as conservative admission, not left as an undocumented
parity gap.

Important detail: the current gate is not only conservative around unresolved
durable state. It also has one global active-admission token for all mutating
commits. That means a second mutating commit on the same or a different branch
fails fast while the first commit is in flight. Old storage blocked same-branch
contenders and allowed unrelated branches to commit concurrently.

Design note: the current cache and durable commit runtimes hold mutable
references to the allocator, target branch, and visible-version tracker. The
Rust ownership shape therefore also assumes a single executor invocation path.
A per-branch admission design is not just a different gate; it requires
per-branch ownership of those components, a shared visible-version advancement
model, or carefully documented interior mutability.

Tasks:

1. Audit `VisibleVersionTracker` and `CommitUnresolvedDurableGate` against old
   pending-version behavior.
2. Keep global visible safety unless a concrete per-branch concurrency design
   proves equivalent durable/visibility ordering.
3. Record the semantic decision for global versus independent-branch commit
   admission:
   - artifact: `docs/architecture/storage/l7-commit-runtime.md`, under a
     new `Semantic Decisions` section unless a shared decision-register file is
     created first;
   - owner: L7 for the primitive, L8 for retry/admission policy;
   - reason: global unresolved-durable safety avoids visible-version advancement
     past a durable-not-applied or applied-not-visible commit and fixes an old
     visibility leak;
   - caller-visible delta: contention returns a typed error instead of blocking;
   - replacement proof: tests showing later commits cannot hide unresolved
     durable state, plus explicit throughput/concurrency/retry tradeoff.
4. Add tests for out-of-order phase failures:
   - allocation gap before WAL;
   - WAL accepted before apply failure;
   - apply succeeded before visible-publish failure;
   - later commits blocked or admitted according to gate facts.
5. Measure or test replay classification cost:
   - current replay may call history per row;
   - recovery correctness is more important than hot-path speed;
   - bulk-classify only if recovery counters show rows times sources scaling.
6. If independent branch commit concurrency is restored, add pending-version
   facts or an equivalent advancement model before loosening global admission.
7. Ensure version gaps are explicitly tested for latest, version reads, history,
   and timeline lookup.

Exit gate:

1. A durable-but-not-visible commit cannot be hidden by later commits.
2. Recovery replay catches allocator and visible-version facts above durable
   replayed commits.
3. Any remaining global-admission restriction is documented as an intentional
   L7 tradeoff, not an accidental parity gap.

### L7-E. Commit Timeline Lookup Efficiency

Goal: prove timestamp-to-version and version-to-timestamp lookups use the
storage-owned commit timeline substrate without broad scans.

Current state: timeline rows are isolated in the storage-owned timeline space,
but retained in-memory timeline lookup can still be linear over retained
timeline entries, and timeline fact reconciliation can be quadratic if it uses
nested timestamp/version scans. L7-E should treat both as algorithmic gaps.

Tasks:

1. Audit `CommitTimelineRows`, `CommitTimelineView`, API timestamp lookup, and
   L6 timestamp resolution.
2. Ensure L7 writes the two required timeline rows in the same commit unit as
   user rows.
3. Ensure recovery validates or installs the same timeline facts from durable
   WAL records.
4. Replace retained timeline linear filtering with a binary-search probe over
   sorted timeline rows, or add an indexed/fact-backed lookup with an explicit
   bound that is independent of retained user-row count and not linear in all
   retained commits.
5. Replace nested timestamp/version reconciliation with a linear merge over
   sorted facts or an equivalent indexed validation.
6. Add counters for timeline rows scanned/probed separately from user rows and
   timeline facts reconciled.

Exit gate:

1. Timestamp lookup scans no user rows.
2. Timestamp lookup over N retained commits is logarithmic, or bounded by a
   documented compact timeline index. A linear filter over all retained
   timeline entries does not satisfy this gate.
3. Duplicate timestamps return the greatest commit version at or before the
   requested timestamp.
4. Recovery preserves timeline lookup results.
5. Timeline view construction is O(M) or better over retained timeline facts,
   not O(M^2).

### L7-F. Durable WAL Bridge Allocation And Failure Classification

Goal: keep WAL-before-visible semantics while reducing avoidable allocation
and proving every failure phase.

Current state: storage-next prepares commit rows in one pass, which closes the
largest old-vs-new allocation concern. The remaining question is whether the
old thread-local WAL buffer reuse matters for burst commit workloads.

Tasks:

1. Audit durable commit row preparation and WAL payload construction for
   unnecessary `StorageRow`, key, value, and timeline-row clones.
2. Avoid preparing user rows twice for WAL encode and L6 apply.
3. Keep WAL payload encoding in L3/L4-owned byte surfaces; L7 may choose the
   storage-shaped payload inputs but must not invent durable bytes.
4. Preserve standard vs always durability policy classification.
5. Keep `CommitWalAppendError` uncertainty facts precise.
6. Add counters or benchmarks that separate WAL payload allocation from branch
   apply row preparation.
7. Inspect `service/wal.rs` to determine whether pooling already belongs there.
8. Add buffer pooling only if the counters show durable commit throughput is
   allocation-bound.

Exit gate:

1. WAL append failure before acceptance leaves no visible rows.
2. WAL accepted plus L6 apply failure returns durable-but-not-visible with
   replayable facts.
3. WAL accepted plus visible-publish failure is classified distinctly from
   pre-WAL failure.
4. Durable commit row preparation does not materially regress blind-write load
   counters.
5. Buffer pooling is either implemented with tests or explicitly skipped with
   perf evidence.

### L7-G. Branch Registry, Generation, Deletion, And Quiesce Primitives

Goal: prove branch admission safety and keep orchestration ownership in L8.

Current state: quiesce is already the intended fast-fail primitive. Registry
lookup still needs scale proof because the current descriptor search is linear.
Same-branch commit contention is also fast-fail today, whereas old storage
blocked on the per-branch commit mutex.

Tasks:

G1, executable before a per-branch admission decision:

1. Test active, missing, deleting, deleted, and generation-mismatched branch
   admission before version allocation.
2. Prove commit guards release on every error path.
3. Measure branch registry lookup cost under many branches.
4. Replace registry internals with an indexed structure only if counters show
   branch-count scaling.
5. Keep `try_begin_quiesce` as a fast-fail primitive and return facts that L8
   can use for retry/deadline orchestration.

G2, gated on L7-D's admission decision:

6. Restore old-style lock-ordering documentation as comments or tests around
   guard acquisition sites. If L7-D keeps global admission, document the current
   simple order. If L7-D restores per-branch admission, design the full
   acquisition hierarchy before changing code.
7. Decide whether same-branch commit contention remains fail-fast or gains a
   blocking API. If it remains fail-fast, document the caller retry contract.

Exit gate:

1. Branch generation/deletion failures occur before durable work.
2. Quiesce blocks new mutating commits and waits for or rejects around
   in-flight commits according to the documented primitive.
3. Retry/deadline close behavior remains assigned to L8.

### L7-H. Pressure And Backpressure Fact Handoff

Goal: expose commit-local pressure/admission facts without implementing L8
policy in L7.

Current state: commit phases and outcome facts are a useful foundation, but the
specific L8 admission facts are not fully emitted yet.

Tasks:

1. Identify L7 facts needed by L8 admission:
   - commit accepted under pressure;
   - commit rejected by branch guard;
   - commit blocked by unresolved durable state;
   - commit would require maintenance before admission;
   - mutation counts and approximate commit bytes.
2. Keep L7 outcome/error facts storage-shaped.
3. Do not add sleeps, waits, compaction calls, flush calls, or rate-limiter
   loops inside L7.
4. Document how L8 will consume these facts in the automatic maintenance plan.
5. Include commit-contention and unresolved-durable blocking facts so L8/L9 can
   distinguish retryable admission pressure from validation failure.

Exit gate:

1. L7 facts are sufficient for L8 to implement write admission and pressure
   policy.
2. Normal commits still do not perform scheduler work in L7.
3. Any missing fact is listed as an L8/L9 follow-up with owner and reason.

### L7-I. Closeout And Benchmark Gate

Goal: prove L7 parity and document remaining handoffs.

Closeout status:

1. Focused cache and durable commit tests cover blind writes, read-set/CAS
   validation, branch generation/deletion, quiesce, durable gates, replay,
   version gaps, timeline rows, and every touched WAL-before-visible phase.
2. Generated public API read workloads preserve an independent visible-row
   model after commit mutations; generated L6/L8 source-shape tests remain
   outside L7.
3. Source guards prove the commit runtime does not import table-source planning,
   maintenance scheduling, backend filesystem APIs, public transaction sessions,
   or roadmap labels.
4. L9 runtime-default durability mapping is tested and durable production paths
   are guarded against cache-default options.
5. L7 perf counters prove blind writes avoid conflict-source capture, timeline
   lookup does not scan user rows, WAL build facts are separated from L6 apply
   rows, and pressure/admission facts are emitted without running maintenance.
6. Benchmark interpretation remains separated from L8 source-shape maintenance:
   100K/1M L9 `load-seq` runs are valid L7 smoke tests, while 5M/10M runs are
   L8 readiness checks until automatic maintenance scheduling lands.

Tasks:

1. Run focused L7 unit tests in cache and durable modes.
2. Run generated commit/runtime tests.
3. Run durable fault tests for every commit phase touched by L7 work.
4. Run L9 load benchmarks with manual source-shape maintenance separated from
   L7 commit timing.
5. Compare old-vs-new commit correctness and L7 counters before interpreting
   throughput.
6. Update audit docs or implementation notes with every closed/deferred L7
   finding.

Exit gate:

1. Commit correctness tests pass unchanged under restored L5/L6 topology.
2. Blind writes, read-set/CAS validation, durable gate, replay, generation, and
   quiesce tests pass.
3. Commit timeline lookup counters are bounded and separate from user-row
   scans.
4. L7 does not import or implement L5/L6 source planning or L8 maintenance
   scheduling.
5. Every remaining L7 audit finding is closed or explicitly deferred with
   owner layer, reason, and replacement proof.

## Recommended Execution Order

Based on the current code delta review, execute the slices in this order:

1. L7-A counters and baseline.
2. L7-D semantic decision and unresolved-durable visibility tests for findings
   1 and 2.
3. L7-G branch-registry scaling test and quiesce/lock-order proof.
4. L7-E binary-search or indexed timeline lookup.
5. L7-H pressure/admission fact handoff for L8.
6. L7-F WAL buffer-pooling measurement and decision.
7. L7-C L9 read-set handoff design.
8. L7-B blind-write fast-path counter assertion and closure.
9. L7-I closeout.

## Expected Counter Movement

Performance-sensitive changes should move counters this way:

1. Blind writes:
   - `conflict_sources_built = 0`;
   - read-view captures for conflict validation stay zero;
   - validation source probes stay zero.
2. Read-set/CAS commits:
   - conflict source captures are at most one per commit;
   - point probes scale with validation fact count and L6 source shape;
   - no unrelated user-row scans.
3. Timeline lookup:
   - user-row scans stay zero;
   - timeline probes are bounded by timeline source shape.
4. Durable commits:
   - WAL build counters separate payload bytes from L6 apply rows;
   - row preparation is one pass through user mutations plus timeline rows.
5. Branch registry:
   - admission cost does not grow with unrelated rows or tables;
   - if it grows with branch count, the branch-count threshold is documented.

## Verification Commands

Focused commands:

```sh
cargo fmt --manifest-path crates/storage-next/Cargo.toml --all
cargo test --manifest-path crates/storage-next/Cargo.toml --lib commit
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::commit
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::read
cargo test --manifest-path crates/storage-next/Cargo.toml --lib lifecycle::tests::recovery
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --features perf-trace commit
cargo clippy --manifest-path crates/storage-next/Cargo.toml --lib --all-features -- -D warnings
git diff --check
```

Broader gate before L9 benchmarking:

```sh
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --all-features
```

Benchmark gate:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100k,1m \
  --engines cache,standard \
  --workloads load-seq \
  --samples 1000 \
  --flush-every 100000 \
  --value-bytes 64
```

Use 5M and 10M for commit-source-shape confirmation only after L8 automatic
maintenance work removes the fixed-point compaction cliff from the benchmark
path.

## Stop Conditions

Stop and re-plan if:

1. a proposed L7 fix requires automatic flush, compaction, materialization, or
   write-stall enforcement;
2. a proposed validation fix requires broad branch/table scans after L6 point
   source pruning is available;
3. durable failure tests require a new WAL/table/checkpoint byte format;
4. independent branch commit concurrency would weaken durable-but-not-visible
   safety without an equivalent pending-version model;
5. public API work starts exposing transaction sessions instead of
   storage-shaped commit/read facts;
6. L7 counters show the measured bottleneck is actually L6 source work or L8
   scheduler work.
7. timeline lookup remains linear over retained timeline entries after L7-E, or
   timeline reconciliation remains nested O(M^2).
