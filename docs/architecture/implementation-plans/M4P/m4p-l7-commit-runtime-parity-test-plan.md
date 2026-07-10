# M4P-L7 Test Plan: Commit Runtime Parity

Status: implemented closeout; remaining benchmark and orchestration work is
owned by L8/L9 follow-up plans.

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l7-commit-runtime-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Architecture context:
`docs/architecture/storage/l7-commit-runtime.md`

Audit context:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

## Goal

Prove that storage-next commit runtime preserves old commit correctness,
durability ordering, validation semantics, and visibility facts while remaining
narrowly scoped to L7.

The tests must catch:

1. commits becoming visible out of order;
2. partial visibility of one commit batch;
3. missed read-set or CAS conflicts;
4. blind writes paying conflict-source cost;
5. durable commits becoming visible before WAL acceptance;
6. durable-but-not-visible state being lost or misclassified;
7. recovery replay duplicating or losing committed rows;
8. branch deletion, generation, or quiesce guards admitting unsafe commits;
9. timeline lookup scanning user rows or unrelated timeline rows;
10. L7 importing scheduler, table-source, product DTO, or public transaction
    session behavior.

## Test Matrix

| Slice | Required proof | Failure caught |
| --- | --- | --- |
| L7-A | Commit phase counters and facts are visible in focused tests. | Later work cannot distinguish L7 cost from L6/L8 cost. |
| L7-B | Blind writes skip conflict-source capture; read/CAS facts capture bounded sources. | Blind writes regress to read-path source capture. |
| L7-C | Read-set and CAS validation preserve old first-committer-wins behavior. | Conflicting commits silently overwrite expected versions. |
| L7-D | Visible-version and unresolved-durable gates preserve phase safety. | Durable or applied commits are hidden, leaked, or advanced unsafely. |
| L7-E | Commit timeline lookup is bounded and recovery-preserved. | Timestamp lookup scans user rows or returns wrong tie-breaks. |
| L7-F | WAL-before-visible fault windows are classified. | A crash window loses durable rows or exposes non-durable rows. |
| L7-G | Branch registry, generation, deletion, and quiesce guards reject safely. | Commits land in deleted branches or quiesced branches. |
| L7-H | Pressure/admission facts exist without L7 scheduling. | L7 hides pressure or starts doing L8 work. |
| L7-I | Focused, generated, durable, source-guard, and benchmark gates pass. | A local fix works only for one hand-written scenario. |

## Required Semantic Decisions

Before differential tests may skip or reinterpret old-storage behavior, L7 must
record these decisions with owner, reason, and replacement proof:

1. Global versus independent-branch commit admission.
   - Current storage-next behavior: a global unresolved-durable gate blocks
     mutating admission across branches when any commit is durable-not-applied
     or applied-not-visible, and the same gate currently allows only one
     in-flight mutating commit globally.
   - Old evidence: old storage used per-branch commit locks and pending-version
     advancement. Same-branch contenders blocked; unrelated branches could
     commit concurrently.
   - Owner: L7 for visible safety; L8 for retry/deadline and admission policy.
   - Artifact: record in `docs/architecture/storage/l7-commit-runtime.md`
     under `Semantic Decisions`, unless a shared decision-register file exists
     first.
   - Required proof: tests show later commits cannot advance visibility past
     unresolved durable state, contention returns the documented typed failure,
     and the concurrency/retry tradeoff is explicit.
2. Pending-version advancement versus flat visible-version tracking.
   - Current storage-next behavior: visible-version publication is flat and
     conservative rather than using old pending-version sets.
   - Owner: L7.
   - Required proof: version gaps are allowed, unresolved commits are typed, and
     recovery catches the allocator and visible facts above replayed commits.
   - Semantic note: old pending-version behavior could advance visibility past
     a WAL-durable commit whose L6 apply failed. Storage-next should assert the
     safer unresolved-durable behavior, not reproduce that old leak.
3. Public read-set facts versus product transaction sessions.
   - Current storage-next behavior: L7 has internal read-set/CAS validation;
     L9 does not expose public storage transactions.
   - Owner: L7/L9.
   - Required proof: L7 semantics are tested directly and the L9 handoff is a
     storage-shaped read-fact API, not begin/commit/rollback sessions.
4. Cache, standard, and always defaults.
   - Current storage-next behavior: not yet audited for L7 closeout.
   - Owner: L7/L9.
   - Required proof: cache mode does not claim durability, durable standard and
     always map to distinct WAL policies, and public defaults are documented.
5. Explicit missing observed version versus old zero sentinel.
   - Current storage-next behavior: `Missing` is explicit and `Present(0)` is
     invalid.
   - Owner: L7/L9.
   - Required proof: callers cannot confuse missing with commit version zero.
6. Cache-mode applied-not-visible read-your-writes.
   - Current storage-next behavior: if cache apply succeeds but visible publish
     fails, same-branch reads can still see the applied row while the unresolved
     gate blocks unsafe cross-branch advancement.
   - Owner: L7.
   - Required proof: same-branch read-your-writes and cross-branch blocking are
     both pinned by tests.
7. Per-batch commit limits.
   - Current storage-next behavior: commit runtime enforces maximum mutations,
     validation facts, and total commit rows.
   - Owner: L7/L9.
   - Required proof: limits are tested and documented as storage-next V1
     defaults.

## Correctness Tests

### Batch Validation And Allocation

1. Empty/read-only diagnostic batch does not allocate a commit version.
2. Mutating batch allocates exactly one commit version.
3. Every user row and timeline row in a mutating batch receives the same commit
   version and timestamp.
4. Puts and deletes in one batch become visible atomically.
5. Duplicate user mutations obey the configured duplicate-key policy.
6. Malformed branch ids, row keys, expiry metadata, durability mode, and
   mutation-count limits reject before version allocation where possible.
7. Version counter overflow returns a typed error.
8. Version gaps do not break latest reads, version reads, history, or timeline
   lookup.
9. Mutation count above the configured batch limit rejects with a typed error.
10. Validation fact count above the configured batch limit rejects with a typed
    error.
11. Total commit rows above the configured limit, including timeline rows,
    rejects with a typed error.
12. `CommitObservedVersion::Present(0)` rejects and `Missing` remains the only
    missing-row representation.

### Blind Writes

1. Blind put commits succeed with no read-set/CAS facts.
2. Blind delete commits succeed with no read-set/CAS facts.
3. Concurrent blind writes serialize by branch commit ordering but do not
   report validation conflicts solely because they are blind.
4. Blind writes reject branch deletion, generation mismatch, and quiesce before
   conflict-source capture.
5. Concurrent blind writes on different branches follow the documented global
   admission behavior: either one succeeds and the other fails fast with a typed
   retryable/admission error, or a later implementation explicitly restores
   independent per-branch admission.
6. Concurrent blind writes on the same branch follow the documented
   fail-fast-or-blocking contract.

### Read-Set Validation

1. Read fact matching the current visible row commits.
2. Read fact for a stale version rejects.
3. Read fact for a missing row rejects when a row now exists.
4. Read fact for missing remains valid when the row is still missing.
5. Read fact over a tombstone uses the documented visible/missing semantics.
6. Read facts for multiple keys reject if any fact changed.
7. Empty read-set facts do not force source capture.
8. Read-set validation does not claim serializable isolation or reject allowed
   write skew unless a future product decision changes the model.
9. Per-fact validation cost is compared against the old `get_version_only`
   baseline or explained by the safer visible-bound validation semantics.

### CAS Validation

1. CAS expected version matching the current visible row commits.
2. CAS expected version mismatch rejects.
3. CAS expected missing row commits only while the row is still missing.
4. CAS expected missing row rejects after another commit creates the row.
5. CAS over a deleted/tombstoned row follows the documented row-missing
   semantics.
6. CAS facts and read facts in the same batch both validate before visibility.

### Commit Timeline

1. Each mutating commit writes exactly one timestamp-to-version row and one
   version-to-timestamp row.
2. Timeline rows are committed in the same batch as user rows.
3. Timestamp-to-version lookup returns the greatest retained commit version at
   or before the requested timestamp.
4. Duplicate timestamps tie-break to the greatest commit version.
5. Version-to-timestamp lookup returns the original commit timestamp.
6. Timeline lookup over a branch with many user rows scans no user rows.
7. Timestamp-to-version lookup over many retained commits does not use a linear
   filter over every retained timeline entry.
8. Timestamp-to-version lookup uses binary-search or documented indexed bounds.
9. Timeline fact reconciliation over many commits is O(M) or better, not nested
   O(M^2) timestamp/version scanning.
10. Timeline lookup after compaction returns the same result.
11. Timeline lookup after durable recovery returns the same result.

### Visibility And Version Gaps

1. Visible version advances only after L6 apply succeeds.
2. Failed validation leaves no visible rows.
3. Failure after version allocation but before WAL acceptance may leave a
   version gap but leaves no visible rows.
4. Version gaps preserve sorted history and latest-row selection.
5. A branch whose max applied version is ahead of visible version blocks unsafe
   later commits until the unresolved gate is resolved.
6. Cross-branch reads never observe rows above the current visible version.
7. WAL-accepted apply failure records durable-not-applied and blocks later
   admission until recovery or reconciliation.
8. Applied-not-visible cache failure preserves documented same-branch
   read-your-writes while blocking unsafe cross-branch advancement.

### Branch Admission

1. Active branch accepts mutating commits with the correct generation guard.
2. Missing branch rejects before version allocation.
3. Deleting branch rejects before version allocation.
4. Deleted branch rejects before version allocation.
5. Generation mismatch rejects before version allocation and before WAL work.
6. Generation recreation, if supported, accepts only the exact new generation.
7. Commit guard releases after validation failure, WAL failure, apply failure,
   and visible-publish failure.

### Quiesce

1. Quiesce prevents new mutating commits while active.
2. Quiesce returns a typed busy/unavailable fact when an in-flight commit holds
   the guard and the L7 primitive is non-waiting.
3. In-flight commit completion releases the guard and allows quiesce to succeed.
4. Read-only diagnostics remain allowed or blocked according to the documented
   quiesce mode.
5. Retry, deadline, and close orchestration tests remain in L8.

### Commit Admission Contention

1. A second mutating commit while one mutating commit is active receives the
   documented typed failure or waits only if L7 intentionally introduces a
   blocking primitive.
2. Cross-branch contention and same-branch contention are tested separately.
3. The contention error is distinguishable from validation conflict, branch
   generation mismatch, and unresolved durable state.
4. The caller retry contract is documented through L7/L9 facts.

### Branch Registry Scaling

1. Admission for a small branch registry remains correct.
2. Admission for a large branch registry records descriptor-probe or lookup
   counters.
3. Registry lookup cost has an explicit branch-count bound or a documented
   threshold that triggers indexed-registry work.
4. Active, deleting, deleted, missing, and generation-mismatched descriptors
   are still classified correctly after any registry internal change.

### Lock Ordering Documentation

1. Guard acquisition order is documented in L7 commit code or tested through a
   lock-order helper.
2. Durable commit, cache commit, unresolved durable gate, and quiesce paths use
   the same documented order.
3. Error paths release guards without leaving quiesce or admission state stuck.
4. If global commit serialization is relaxed, lock-ordering tests are expanded
   before the relaxation lands.

### Replay Classification

1. Replay validates existing rows without normal conflict validation.
2. Replay classification counters distinguish rows classified, source probes,
   and history calls.
3. A replay batch with many rows does not perform rows times unrelated sources
   work unless that recovery cost is explicitly accepted.
4. Replay remains idempotent after any bulk-classification optimization.

### Read-Only Diagnostics Configuration

1. Read-only diagnostics execute when enabled.
2. Read-only diagnostics return the documented typed error when disabled.
3. L9 maps disabled diagnostics without exposing internal commit runtime types.

## Mechanical Counter Tests

Perf-gated tests should assert mechanical movement without making every
correctness test depend on perf tracing.

Required assertions:

1. Blind writes increment commit-batch and row-preparation counters but do not
   increment conflict-source capture counters.
2. Empty read-set/CAS validation increments no source-capture counters.
3. Non-empty read-set/CAS validation increments exactly one source capture per
   commit that needs source validation.
4. Point/table probes during validation are bounded by validation fact count
   and L6 source shape.
5. Timeline lookup counters remain separated from user-row scan counters.
6. Timestamp lookup scans zero user rows.
7. Commit row preparation counts user rows and timeline rows separately.
8. WAL build counters record payload bytes separately from L6 append rows.
9. WAL payload allocation or buffer-reuse counters are present before deciding
   whether buffer pooling is required.
10. Commit contention increments a distinct admission/contention counter.
11. Branch admission failures before version allocation do not increment
   allocation counters.
12. Branch registry scaling tests record descriptor probes or indexed lookup
    hits.
13. Timeline reconciliation counters record facts reconciled and avoid nested
    scan growth.
14. Replay classification counters record rows classified and source/history
    probes.
15. Durable-but-not-visible and applied-but-not-visible gates increment typed
    unresolved-state counters.
16. L7 pressure/admission facts can be recorded without running maintenance.

Counter tests should live in focused L7 or API tests and use `perf-trace` only
where the assertion is specifically mechanical.

## Durable Fault Tests

### Cache Mode

1. L6 apply failure before visible publish leaves no visible committed outcome.
2. Visible-publish failure after cache apply returns applied-but-not-visible and
   records the unresolved gate.
3. Applied-but-not-visible cache state blocks unsafe later visible advancement.
4. Cache mode never reports crash durability.

### Durable Standard And Always

1. WAL append failure before acceptance leaves no visible rows.
2. WAL append uncertainty returns a typed uncertainty fact.
3. WAL accepted but L6 apply fails returns durable-but-not-visible.
4. WAL accepted and L6 apply succeeds but visible publish fails returns the
   correct unresolved durable classification.
5. `standard` policy does not claim per-commit forced durability.
6. `always` policy records forced durability when the WAL service reports it.
7. WAL payload includes the original commit version, timestamp, user rows, and
   timeline rows.
8. WAL payload allocation or buffer reuse is measurable in perf-trace.

### Replay And Recovery

1. Replay bypasses normal conflict validation.
2. Replay is idempotent for the same durable commit.
3. Replay preserves original commit timestamp.
4. Replay validates required timeline rows.
5. Replay catches the version allocator above the maximum recovered commit.
6. Replay publishes visible version only after installing durable rows.
7. Recovery after WAL append before visibility makes the row visible.
8. Recovery after WAL append before timeline publication restores timeline
   lookup.
9. Corrupt or partial WAL payloads fail through L3/L4 decoder/fault tests; L7
   consumes only decoded commit facts.
10. Replay of many rows records bounded classification counters or explicitly
    documents accepted recovery-only scaling.

## Generated Tests

Add or extend generated storage-next commit workloads with:

1. random single-branch put/delete batches;
2. random read-set facts derived from a model snapshot;
3. random CAS facts over present, missing, and deleted keys;
4. random version-allocation gaps induced by injected failure phases;
5. random interleavings of commits and quiesce attempts;
6. random branch generation changes and deletion barriers;
7. random duplicate timestamps;
8. random replay of previously durable commits;
9. random cache and durable-mode commit sequences with the same visible model.
10. random high-branch-count admission attempts.
11. random timestamp lookup requests over large retained timelines.
12. random contention attempts on same and different branches.
13. random replay batches with many rows and repeated keys.

Generated invariants:

1. every committed batch becomes visible atomically;
2. no rejected pre-visible batch changes visible rows;
3. read-set/CAS facts either still match and commit or reject cleanly;
4. blind writes do not conflict with each other through validation;
5. visible versions are monotonic and may be non-dense;
6. timestamp lookup matches the model timeline;
7. replaying durable commits is idempotent;
8. quiesce never exposes partial commits;
9. timeline lookup matches the model without scanning user rows;
10. global unresolved-durable admission blocks or admits according to the
    recorded semantic decision;
11. contention behavior matches the recorded fail-fast-or-blocking contract;
12. replay classification is idempotent and bounded by the documented recovery
    shape.

Generated tests should compare against an independent commit model, not
production-derived expectations.

## Differential Tests

Where old storage is still executable, compare storage-next against old storage
for:

1. blind puts and deletes;
2. put/delete/put resurrection;
3. read-set conflict detection;
4. CAS conflict detection;
5. blind write non-conflict behavior;
6. version gaps, if old test harness can induce them;
7. branch deletion/generation behavior where semantics match;
8. durable replay behavior where old WAL harness is available.

Comparison rules:

1. Compare visible rows and commit ordering first.
2. Compare conflict outcomes as storage-shaped conflict facts, not product
   transaction errors.
3. Compare timeline behavior only where old timestamp semantics are
   executable.
4. Record deliberate semantic differences before skipping a case.

## Source Guards

Required guard assertions:

1. L7 commit code does not import L5 table internals.
2. L7 commit code does not implement L6 source planning or compaction picking.
3. L7 commit code does not call L8 maintenance scheduling, flush scheduling,
   compaction scheduling, materialization scheduling, retention, or close loops.
4. L7 commit code does not import backend/local filesystem APIs directly.
5. L7 public API surfaces do not expose begin/commit/rollback transaction
   sessions.
6. L7 tests use storage-shaped keys/rows and not engine primitive DTOs.
7. Production Rust identifiers and user-visible text do not include roadmap
   labels such as `M4P` or `L7`.
8. L7 commit code does not acquire new locks without updating the documented
   lock-order proof.

## Benchmark And Performance Gates

L7 benchmark interpretation must separate commit-runtime cost from source-shape
and maintenance cost.

Required benchmark gates:

1. L9 `load-seq` with cache and standard engines at 100K and 1M.
2. Manual source-shape maintenance remains separated from commit timing until
   L8 automatic scheduling lands.
3. Perf-trace output records:
   - commit validation time;
   - conflict-source captures;
   - row preparation time;
   - WAL payload/build time where durable;
   - visible-publish time;
   - timeline row count;
   - unresolved durable gate events.
4. Blind-write throughput is interpreted only after counters prove zero
   conflict-source capture.
5. Read-set/CAS benchmark, if added, reports fact count and source probes.
6. 5M and 10M benchmark runs are L8 readiness gates unless the workload is
   designed to measure commit runtime without final fixed-point compaction.
7. Multi-thread blind-write workload compares storage-next contention behavior
   with old per-branch commit concurrency at 4, 16, and 64 concurrent commit
   threads across 1, 8, and 32 branches.
8. The multi-thread workload records success, typed contention failures,
   retries, latency, and throughput separately, so fail-fast semantics are not
   mistaken for validation conflicts.

## Verification Commands

Focused L7 commands:

```sh
cargo fmt --manifest-path crates/storage-next/Cargo.toml --all
cargo test --manifest-path crates/storage-next/Cargo.toml --lib commit
cargo test --manifest-path crates/storage-next/Cargo.toml --lib commit::tests
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::commit
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::read
cargo test --manifest-path crates/storage-next/Cargo.toml --lib lifecycle::tests::recovery
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --features perf-trace commit
cargo clippy --manifest-path crates/storage-next/Cargo.toml --lib --all-features -- -D warnings
git diff --check
```

Broader gate:

```sh
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --all-features
```

Benchmark smoke:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100k,1m \
  --engines cache,standard \
  --workloads load-seq \
  --samples 1000 \
  --flush-every 100000 \
  --value-bytes 64
```

## Exit Criteria

L7 is complete only when:

1. blind writes, read-set validation, CAS validation, branch generation,
   branch deletion, quiesce, durable gate, replay, version gap, and timeline
   tests pass;
2. durable fault tests classify every touched WAL-before-visible failure
   window;
3. generated commit workloads preserve the independent model;
4. source guards prove L7 did not absorb L5/L6/L8/L9 ownership;
5. perf counters prove blind writes skip conflict-source capture;
6. timeline lookup counters prove no user-row scans;
7. cache and durable modes preserve the same visible commit semantics after
   durable-only facts are ignored;
8. L9-facing read-set and pressure fact handoffs are documented, even if public
   API exposure remains in L9;
9. every remaining L7 audit finding is closed or explicitly deferred with
   owner layer, reason, and follow-up slice.

Closeout evidence:

1. Focused commit tests cover blind writes, read-set validation, CAS
   validation, branch generation/deletion, quiesce, durable gates, replay,
   version gaps, timeline lookup, cache mode, standard durability, and always
   durability.
2. Perf-gated tests assert mechanical counters for conflict-source capture,
   row preparation, WAL encode/build, visible publication, unresolved durable
   gates, branch registry probes, replay classification, timeline lookup, and
   admission pressure.
3. Generated API read workloads compare committed rows against an independent
   model; generated L6 source-shape and L8 maintenance workloads are tracked
   outside the commit-runtime closeout.
4. Source guards assert that commit runtime does not import L5 table internals,
   L6 source planning, L8 scheduling, backend filesystem APIs, public
   transaction sessions, or roadmap labels.
5. L9-facing handoffs are documented: public CAS conditions map to storage
   facts; future read-set exposure remains storage-shaped; pressure/admission
   facts are emitted but enforcement remains an L8 policy.
6. Benchmark interpretation is documented: 100K/1M public L9 load benchmarks
   are valid smoke gates for commit runtime, while larger source-shape runs
   remain L8 automatic-maintenance readiness gates.

## Stop Conditions

Stop and re-plan if:

1. conflict validation requires scanning unrelated branch rows;
2. timeline lookup cannot be made bounded without moving L6 timestamp
   resolution or durable bytes;
3. durable failure proof requires a new WAL format;
4. independent branch concurrency weakens unresolved durable safety;
5. pressure/backpressure behavior starts running L8 scheduler work in L7;
6. a public transaction-session API is required to express the storage-shaped
   validation facts.
7. timeline lookup remains linear over retained timeline entries, or timeline
   reconciliation remains nested O(M^2), after the L7-E work.
