# Strata V1 Testing And Conformance Plan

Status: V1 architecture draft

## Purpose

Strata V1 should be built around a reference-grade testing culture. Storage is
the foundation of that effort. If storage cannot be tested deterministically at
each layer, the higher product promises around branching, time travel, recovery,
IPC, search, graph relationships, vectors, and Strata AI will never be reliable.

This document is the top-level testing plan. It identifies the test families
Strata must build, then walks the storage-next L1-L9 architecture layer by
layer and records what each layer must prove.

Layer-specific test specs can be written later. This document defines the first
shared map.

## Related Documents

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/storage-next-architecture.md`
3. `docs/architecture/storage-next/README.md`
4. `docs/architecture/storage-next/implementation-patterns.md`
5. `docs/architecture/v1-error-and-diagnostics-contract.md`
6. `docs/architecture/runtime-resource-profile-architecture.md`
7. `docs/product/strata-v1-non-functional-requirements.md`

## Goals

1. Make testability a design requirement, not an implementation afterthought.
2. Test storage layer contracts directly before engine behavior depends on
   them.
3. Use deterministic fault injection for every important failure window.
4. Use fuzzing for byte formats, parsers, codecs, command payloads, recovery
   inputs, and cursor state machines.
5. Use crash-recovery tests for every durable state transition.
6. Use backend conformance tests so local filesystem, cache/browser, and future
   object/OpenDAL backends obey the same contract where applicable.
7. Assert stable error codes, classes, retry policy, and commit outcomes rather
   than prose messages.
8. Keep product semantics out of storage tests unless the test is explicitly an
   engine or product-path test.
9. Build reusable harnesses instead of one-off fixtures per feature.
10. Keep the memory/cache backend compiling and passing on
    `wasm32-unknown-unknown` so browser/cache substrate support does not become
    a retrofit.
11. Prove runtime resource profiles with fake host probes and resolved-budget
    assertions so one binary remains safe across edge, desktop, and server
    envelopes.

## Non-Goals

1. This document does not define exact Rust test module names.
2. This document does not require all test families to exist before the first
   storage-next implementation begins.
3. This document does not define benchmark thresholds.
4. This document does not specify a particular fuzzing framework.
5. This document does not require production OpenDAL or S3 durable mode for V1.
6. This document does not turn storage tests into engine primitive tests.

Performance regression benchmarks remain binding where CLAUDE.md or the current
project rules require them. This plan does not redefine thresholds; a benchmark
plan should rederive storage-next thresholds once the new architecture exists.

Engine-next product-path tests are out of scope until the engine-next
architecture exists. This plan covers storage-next conformance and identifies
where engine-next should later add product-path tests over L9.

## Test Taxonomy

Strata should use a small set of repeatable test families.

### Unit Tests

Unit tests prove local invariants for one type, module, or service:

1. Constructor validation.
2. Boundary conditions.
3. Error classification.
4. Redaction.
5. Source-chain preservation.
6. Ordering and comparison invariants.
7. No accidental defaulting of impossible states.

### Property Tests

Property tests generate many valid and invalid states for stable invariants:

1. Object-name round trips.
2. Durable-format encode/decode round trips.
3. Internal-key ordering.
4. Cursor movement.
5. Table compaction equivalence.
6. Branch visibility across inherited layers.
7. Commit ordering and version monotonicity.
8. Retention and tombstone behavior.

### Golden-Vector Tests

Golden vectors freeze stable durable bytes and public wire shapes:

1. WAL segment and record bytes.
2. Commit payload bytes.
3. Manifest bytes.
4. Snapshot envelope bytes.
5. Table header/footer/block/entry bytes.
6. Command boundary error status bytes once the protocol is defined.

Golden vector regeneration must be explicit. Normal tests must never update
goldens incidentally.

### Fuzz Tests

Fuzz tests target untrusted or durable input:

1. Format decoders.
2. Compression and encryption frames.
3. Manifest decoders.
4. WAL readers.
5. Snapshot readers.
6. Table readers and cursors.
7. Recovery input ordering.
8. Object-name parsers.
9. Command and IPC payloads.
10. Import/export formats.

Fuzz targets must prove:

1. No panic.
2. No unbounded allocation from attacker-controlled lengths.
3. No successful decode with unconsumed bytes unless explicitly supported.
4. No success after checksum or authentication failure.
5. Deterministic typed errors for invalid input.

### Backend Conformance Tests

Backend conformance tests prove every backend implements the same L1 contract
for the capabilities it claims.

Required backends for the first storage-next implementation:

1. Memory/cache backend.
2. Local filesystem backend.

Architecture-aware future backend:

1. OpenDAL/object backend.

An unfinished backend may exist only if it reports capabilities honestly and
fails unsupported modes before side effects.

The memory/cache backend must compile and pass its non-durable conformance
tests on `wasm32-unknown-unknown`. Durable browser persistence is not required
for the first storage rewrite.

### Fault-Injection Tests

Fault injection tests simulate failures at explicit layer boundaries:

1. Backend read/write/delete/list failures.
2. Partial writes.
3. Stale metadata or fencing tokens.
4. Permission failures.
5. Sync/fsync failures.
6. Publish failure before visibility.
7. Publish failure after possible visibility.
8. WAL append failures.
9. Manifest publish failures.
10. Snapshot publish failures.
11. Table publish failures.
12. IPC disconnects during write.
13. Provider timeouts and invalid responses.

Fault injection should be deterministic. Randomized failure schedules can be
added after deterministic cases exist.

### Crash-Recovery Tests

Crash-recovery tests should simulate process death between durable state
transitions:

1. Before WAL append.
2. During WAL append.
3. After WAL append before visible apply.
4. After visible apply before manifest update.
5. During manifest publish.
6. After manifest publish before namespace durability confirmation.
7. During snapshot publish.
8. During table publish.
9. During WAL truncation.
10. During retention or quarantine operations.
11. During shutdown.

Each crash test should reopen the database and assert durable state, visible
state, recovery health, error status, and absence of orphaned visible data.

### Integration Tests

Integration tests prove layers compose correctly:

1. L1-L4 durable services over local filesystem.
2. L4-L7 write path with WAL-before-visible.
3. L5-L6 reads over mutable, frozen, immutable, and inherited tables.
4. L7-L8 recovery replay and allocator catch-up.
5. L8-L9 open, close, maintenance, health, and fault hook behavior.
6. Engine-next product pathways over the L9 boundary.

Integration tests should still use storage-shaped rows unless they are
explicitly engine product tests.

### Long-Running And Randomized Tests

Long-running tests exercise state-space combinations:

1. Mixed puts, tombstones, forks, materializations, retention, and compaction.
2. Crash schedules across many commits.
3. Recovery after repeated checkpoint/WAL truncation cycles.
4. Branch inheritance and deletion under maintenance pressure.
5. Cache pressure and table read concurrency.
6. Engine product workflows combining branches, time travel, graph
   relationships, vectors, search, and events.

These tests are not a substitute for deterministic fault cases. They catch
interactions after the base contracts are already specified.

## Required Test Infrastructure

The first storage-next implementation should build test infrastructure early.

### Testkit Crate Or Module

Storage-next should have a reusable testkit for:

1. Backend conformance.
2. Faulting backend wrapper.
3. Durable publisher fault windows.
4. Crash-point orchestration.
5. Golden-vector fixture loading.
6. Storage-row generators.
7. Branch/table state generators.
8. Recovery-state builders.
9. Error-status assertions.
10. Redaction assertions.

The exact crate/module placement can be decided during implementation. The key
requirement is that tests reuse harnesses instead of building bespoke fixtures.

### Fault Backend

The fault backend must be able to fail:

1. Read.
2. Range read.
3. Write.
4. Conditional create.
5. Conditional update.
6. Delete.
7. List.
8. Metadata read.
9. Durable sync where supported.
10. Namespace/directory sync where supported.

It should support failure by operation count, object role, object name, phase,
and injected error class.

### Crash Harness

The crash harness should support two levels:

1. In-process deterministic interruption for fast layer tests.
2. Process-level kill/reopen tests for durable local filesystem confidence.

Process-level tests are slower and should be targeted. In-process tests should
cover the full failure matrix first.

### Concurrency Harness

L7 commit ordering, branch commit guards, maintenance quiescing, and close
ordering need deterministic concurrency tests.

The implementation must choose a concurrency-testing approach before L7 freezes.
Acceptable first choices are:

1. A hand-rolled deterministic scheduler for storage-owned tasks.
2. `loom`.
3. `shuttle`.

The choice should be recorded in the L7 implementation plan. Until then,
ordinary thread races are useful stress coverage but are not sufficient as the
only proof of lock ordering or commit interleavings.

### Error Assertions

All failure tests should assert:

1. Error class.
2. Error code.
3. Retry policy.
4. Commit outcome.
5. Source chain presence where applicable.
6. Redaction.
7. Recovery health facts when applicable.

Tests should not assert prose messages unless the test is specifically about
human-facing CLI or documentation output.

## Storage Layer Test Plan

### L1 Backend IO

L1 proves Strata can depend on backend capabilities honestly.

Conformance tests:

1. Write and read full object.
2. Read byte ranges.
3. Delete object.
4. List by prefix.
5. Metadata changes after write.
6. Conditional create succeeds once and fails on existing object.
7. Conditional update fails with stale metadata or fence.
8. Unsupported operations return unsupported, not success.
9. Capability mismatch is detected before higher layers run.
10. Object names are treated as opaque validated names from L2.

Fault tests:

1. Failed read.
2. Failed range read.
3. Failed write.
4. Failed delete.
5. Failed list.
6. Partial write where the backend can simulate it.
7. Stale metadata.
8. Precondition failure.
9. Permission failure.
10. Transient unavailable.

Backend matrix:

| Backend | Required tests |
| --- | --- |
| Memory/cache | Non-durable conformance and fault tests, no crash durability claim. |
| Local filesystem | Full durable conformance, sync/fencing behavior, process-level crash tests where relevant. |
| Future OpenDAL/object | Capability declaration, unsupported-mode rejection, object semantics conformance for claimed capabilities. |

Acceptance criteria:

1. Every backend declares capabilities.
2. Unsupported durable modes fail before side effects.
3. Backend errors carry storage-local classifications.
4. Local filesystem is the reference durable backend.

### L2 Object Layout

L2 proves all storage object names are validated and namespace-safe.

Unit tests:

1. Constructor rejects empty names.
2. Constructor rejects absolute paths.
3. Constructor rejects `..` and path escape.
4. Constructor rejects trailing slashes where not allowed.
5. Reserved prefixes cannot be used by the wrong object family.
6. Temporary, WAL, manifest, table, snapshot, quarantine, and lock objects use
   their reserved families.
7. Follower-state object names are absent.

Property tests:

1. Random valid IDs roundtrip through object-name construction and parsing.
2. Random invalid strings are rejected or normalized only through explicit
   constructors.
3. Prefix listing prefixes are unambiguous.
4. Ordered IDs preserve lexical ordering where required.
5. Backend path/key mapping cannot escape its namespace.

Conformance tests:

1. Local filesystem mapping preserves validated object names.
2. Memory/cache mapping uses the same logical object names.
3. Future object backend mapping does not assume POSIX paths.

Acceptance criteria:

1. No upper storage layer constructs object names with raw string formatting.
2. L2 has no IO classification.
3. Object names do not carry a V1 namespace prefix by default.

### L3 Durable Format And Codec

L3 proves stable bytes are strict, documented, and fuzzable.

Golden-vector tests:

1. WAL segment and record format.
2. Commit payload format.
3. Manifest format.
4. Snapshot container and section envelope.
5. Storage row encoding.
6. Internal key encoding.
7. Immutable table header, footer, block, and entry format.
8. Compression frame format for uncompressed and zstd blocks.
9. Checksum coverage for each durable object family.

Strict decode tests:

1. Truncation at every boundary.
2. Magic/version rejection.
3. Pre-V1 development-version rejection.
4. Future-version rejection.
5. Checksum mismatch.
6. Trailing-byte rejection unless extension bytes are specified.
7. Unknown storage-owned tags.
8. Oversized count and length fields.
9. Codec mismatch.
10. Authenticated encryption corruption if AES-GCM remains supported.
11. Compression frame corruption.
12. Table block frame corruption.

Fuzz tests:

1. Every public decoder.
2. Internal key parser and ordering.
3. Commit payload parser.
4. Snapshot section envelope parser.
5. Table block parser.
6. Codec stack decode path.

Acceptance criteria:

1. No decoder panics on arbitrary bytes.
2. No decoder allocates unbounded memory from declared lengths.
3. No decoder accepts trailing bytes accidentally.
4. Format errors are typed and product-agnostic.
5. Golden vectors change only through explicit fixture updates.

### L4 Log, Manifest, Snapshot, And Publish Services

L4 proves durable services classify publish windows and recovery inputs
correctly.

Required test groups:

1. Durable publisher success.
2. Durable publisher failure before publish.
3. Durable publisher failure after publish but before durability confirmation.
4. Manifest write/read roundtrip.
5. Manifest corrupt bytes.
6. Manifest publish rollback or old-state preservation.
7. Directory or namespace sync failure after publish.
8. WAL append/read roundtrip.
9. WAL partial-tail detection.
10. WAL mid-segment corruption detection.
11. WAL lossy scan behavior if retained.
12. WAL codec decode failure.
13. WAL active segment protection during truncation.
14. WAL sidecar missing/corrupt fallback if sidecars remain.
15. Snapshot write/read roundtrip.
16. Snapshot temp cleanup.
17. Snapshot codec mismatch.
18. Snapshot CRC failure.
19. Snapshot prune protects manifest-live snapshot.
20. Table manifest publish success.
21. Table manifest publish failure before and after namespace publication.
22. Cache-mode service behavior reports non-durable facts.
23. Local filesystem service behavior reports durable facts.
24. Backend capability mismatch fails before service work starts.

Fault and crash tests:

1. Crash before object visibility.
2. Crash during byte write.
3. Crash during durable barrier.
4. Crash during namespace publication.
5. Crash after publication before durability confirmation.
6. Crash during cleanup.

Acceptance criteria:

1. Publish uncertainty has a typed result.
2. WAL readers distinguish partial tail from corruption.
3. Snapshot and manifest services do not know engine primitive DTOs.
4. Cache mode never claims crash durability.

### L5 Table Runtime

L5 proves sorted table mechanics independent of branch semantics.

Unit and conformance tests:

1. Mutable table insertion and sorted iteration.
2. Frozen table view immutability.
3. Immutable table builder output.
4. Immutable table reader point lookup.
5. Raw prefix and range cursors.
6. Raw sorted merge cursor.
7. Bloom/filter behavior.
8. Index behavior.
9. Block cache hit, miss, and eviction.
10. Compression roundtrip if compression is retained.
11. Output splitting.
12. Object-backed table reads through L4.

Corruption tests:

1. Header decode failure.
2. Footer decode failure.
3. Block checksum failure.
4. Index corruption.
5. Filter corruption.
6. Range read failure.
7. Block cache decode failure.

Property tests:

1. Table builder/reader roundtrip over generated sorted rows.
2. Cursor movement equivalence with a simple model.
3. Merge cursor equivalence with sorted input model.
4. Compaction equivalence over generated sorted inputs.
5. Tombstone and TTL policy behavior under caller-supplied retention policy.

Fuzz tests:

1. Table decode.
2. Cursor state transitions.
3. Block frame decode.
4. Index/filter decode if those bytes are public L5 formats.

Acceptance criteria:

1. L5 has no branch topology.
2. L5 does not call backend IO directly.
3. L5 table tests run against in-memory and local filesystem-backed objects.
4. Block cache ownership is explicit and database-local.

### L6 Branch-Isolated LSM Runtime

L6 proves storage-level branch isolation, inherited layers, COW mechanics, and
visibility over versioned rows.

Core behavior tests:

1. Branch-local active table ownership.
2. Branch-local frozen table ownership.
3. Branch-local immutable level ownership.
4. Latest reads.
5. Version-bounded reads.
6. Timestamp-bounded reads over storage commit timestamps.
7. Per-key history.
8. Prefix and range scans by visibility bound.
9. Tombstone visibility.
10. TTL visibility, if TTL remains in storage row metadata.
11. Pinned read views.

Branch isolation tests:

1. Fork exposes inherited state without copying all rows immediately.
2. Fork-version gates hide later parent writes.
3. Inherited key rewriting preserves lookup and scan behavior.
4. Materialization preserves reads.
5. Branch delete preserves tables still inherited elsewhere.
6. Shared table reachability facts are rebuilt deterministically.
7. Branch-local compaction does not corrupt inherited readers.

Property tests:

1. Generated branch DAGs preserve visibility according to a simple model.
2. Generated writes/tombstones/forks match model latest reads.
3. Generated version bounds match model `getv`.
4. Generated timestamps match model `as_of`.
5. Materialized and non-materialized reads are equivalent.
6. Compaction preserves visible results.
7. Retention never removes data still required by visible history.

Fault and recovery tests:

1. Branch table install failure does not publish partial branch state.
2. Ref/reachability rebuild handles missing orphan tables.
3. Corrupt table is classified as storage recovery fact.
4. Snapshot row install uses generic rows only.

Acceptance criteria:

1. L6 uses synthetic storage rows, not JSON/graph/vector/search primitives.
2. L6 owns mechanics of branch-local isolation, not product merge semantics.
3. L6 exposes raw facts needed by L8 recovery and maintenance.

### L7 Commit Runtime

L7 proves internal commit units preserve ordering and WAL-before-visible.

Commit behavior tests:

1. Internal `CommitBatch` validation.
2. Single-branch mutating commit path.
3. Monotonic commit-version allocation.
4. One commit timestamp per commit.
5. Optional read-set/CAS conflict detection.
6. Per-branch commit guard.
7. Commit quiesce guard.
8. Branch-deleting rejection.
9. Atomic L6 apply for puts and tombstones.
10. Visible-version publication after L6 apply.
11. Version gaps are either allowed and documented or impossible by invariant.
12. Commits on different branches do not corrupt global visible-version facts.

Durability and fault tests:

1. WAL append failure leaves no visible rows.
2. WAL append plus sync uncertainty returns the correct commit outcome.
3. WAL append success plus L6 apply failure returns durable-but-not-visible.
4. Visible publish failure after L6 apply is classified distinctly.
5. Cache mode uses WAL-free path and never claims crash durability.
6. WAL timestamps are preserved through recovery.
7. Replay is idempotent.

Property/fuzz tests:

1. Generated commit sequences preserve version monotonicity.
2. Generated conflicts match the validation model.
3. Generated crash points preserve WAL-before-visible.
4. Generated replay schedules are idempotent.
5. Commit payload fuzzing rejects malformed rows.

Acceptance criteria:

1. Public transaction sessions are not required.
2. Engine-next does not import WAL record structs or transaction internals.
3. L7 tests use storage-shaped rows.
4. Ambiguous commit and durable-but-not-visible states are first-class test
   outcomes.

### L8 Lifecycle, Recovery, And Maintenance

L8 proves open, recovery, checkpoint, compaction, retention, quarantine,
maintenance scheduling, and close behavior.

Open and recovery tests:

1. Deterministic create/open by storage mode.
2. Capability rejection before side effects.
3. Recovery from empty database.
4. Recovery from MANIFEST plus WAL.
5. Recovery from checkpoint-only state.
6. Recovery from checkpoint plus WAL tail.
7. Codec mismatch recovery rejection.
8. WAL partial-tail truncation.
9. WAL corruption strict failure.
10. Lossy WAL fallback characterization if retained.
11. Segment/table recovery health classification.
12. Inherited-layer recovery and loss classification.
13. Commit-runtime bootstrap from recovered storage facts.
14. Recovery health facts with typed degradation.

Maintenance tests:

1. Flush frozen mutable tables.
2. Flush watermark monotonicity.
3. Checkpoint determinism.
4. Checkpoint then WAL truncation.
5. Snapshot retention and pruning.
6. Snapshot prune protects live manifest snapshot.
7. Branch/table compaction scheduling hook.
8. Compaction publish failure rollback.
9. Inherited-layer materialization scheduling hook.
10. Quarantine reconciliation.
11. Quarantine, reclaim, and purge protocol.
12. Reclaim blocked under unsafe degraded recovery.
13. Retention proof incomplete keeps objects.
14. Concurrent flush and branch-delete races.

Close and scheduler tests:

1. Shutdown drains maintenance.
2. Shutdown quiesces commits.
3. Shutdown flushes/syncs required state.
4. Writer guard or lease is released.
5. Close is idempotent.
6. Close timeout is reported and retryable.
7. Scheduler drain/cancel behavior is deterministic.

Crash and fuzz tests:

1. Crash injection between every durable publication step.
2. Fuzz recovery input ordering.
3. Fuzz corrupted manifests.
4. Fuzz recovery with orphaned tables, snapshots, WAL, tmp, and quarantine
   objects.

Acceptance criteria:

1. A deterministic single-threaded maintenance executor exists for tests.
2. Background concurrency is tested separately after operation contracts are
   stable.
3. L8 reports raw recovery facts, not product recovery text.
4. Follower recovery does not exist.

### L9 Storage API Boundary

L9 proves engine-next can consume storage without reaching into lower layers.

Boundary conformance tests:

1. Engine-facing open cannot bypass capability validation.
2. Unsupported durable object-store mode fails before side effects.
3. Cache mode does not claim crash durability.
4. Local filesystem mode reports durable capability facts.
5. Raw open outcome includes recovery and capability facts.
6. Commit outcome includes version, timestamp, durability, conflict/stall, and
   commit-outcome facts.
7. WAL append failure leaves no visible rows.
8. WAL append plus apply failure returns durable-but-not-visible.
9. Read latest works through the boundary.
10. Read by version works through the boundary.
11. Timestamp-bounded read works through the boundary.
12. Prefix/range scan by visibility bound works through the boundary.
13. Per-key history works through the boundary.
14. Basic branch create/fork/materialize/delete mechanics work through the
   boundary.
15. Primitive-neutral checkpoint boundary does not require storage primitive
   knowledge.
16. Raw recovery outcome and health are exposed.
17. Maintenance drain/status/control hooks are exposed for engine/tests.
18. Safe close/shutdown works through the boundary.
19. Fault hooks are unavailable or inert in normal production builds.
20. Upper crates above engine-next cannot import storage-next in normal
   production code.

Error conformance tests:

1. Storage boundary errors expose storage-local categories.
2. Engine-next maps storage errors into product errors explicitly.
3. No storage error exposes JSON, graph, vector, search, recipe, IPC, or Strata
   AI semantics.
4. Error codes, retry policy, and commit outcomes match
   `v1-error-and-diagnostics-contract.md`.

Acceptance criteria:

1. L9 is the only normal production storage surface consumed by engine-next.
2. L9 hides lower-layer implementation detail unless it is a stable storage
   contract.
3. L9 has cache and local filesystem conformance suites.

## Cross-Layer Matrices

### Backend Matrix

| Test family | Memory/cache | Local filesystem | Future OpenDAL/object |
| --- | --- | --- | --- |
| L1 conformance | Required | Required | Required for claimed capabilities |
| wasm32 cache conformance | Required for memory/cache | Not applicable | Not applicable until a wasm-capable adapter exists |
| L2 object names | Required | Required | Required |
| L3 formats | Required where bytes exist | Required | Required where bytes exist |
| L4 durable services | Non-durable facts only | Required | Unsupported-mode tests until durable mode exists |
| L5 table runtime | Required | Required | Required through object abstraction if implemented |
| L6 branch LSM | Required | Required | Required after backend supports needed object semantics |
| L7 commits | WAL-free non-durable | WAL-before-visible | Fail fast unless durable primitive exists |
| L8 recovery | No crash durability claim | Required | Deferred unless durable mode exists |
| L9 API | Required | Required | Unsupported-mode tests |

### Failure Matrix

Every storage layer should map failure tests to stable error facts.

| Failure family | Primary layers |
| --- | --- |
| Invalid object name | L2, L9 |
| Unsupported backend capability | L1, L4, L8, L9 |
| Backend read/write/list/delete failure | L1, L4, L5, L8, L9 |
| Publish before visibility failure | L4, L7, L8 |
| Publish after possible visibility failure | L4, L7, L8 |
| Durable sync failure | L1, L4, L7, L8 |
| Codec mismatch | L3, L4, L8 |
| WAL partial tail | L4, L8 |
| WAL corruption | L3, L4, L8 |
| Table corruption | L3, L5, L8 |
| Snapshot corruption | L3, L4, L8 |
| Retention proof incomplete | L6, L8 |
| Quarantine/reclaim failure | L8 |
| Commit conflict | L7, L9 |
| Durable-but-not-visible commit | L7, L8, L9 |
| Ambiguous commit | L4, L7, L9 |

### Test Maturity Levels

Each layer should move through maturity levels:

1. **T0 - Unit coverage.**
   Local constructors, invariants, and basic success/error cases exist.

2. **T1 - Conformance coverage.**
   The layer has reusable harnesses and multiple implementations or modes pass
   the same tests.

3. **T2 - Property coverage.**
   Generated valid states prove model equivalence and ordering invariants.

4. **T3 - Fault coverage.**
   Deterministic injected failures cover expected failure windows and error
   classification.

5. **T4 - Fuzz coverage.**
   Public decoders, parsers, and state machines survive untrusted inputs.

6. **T5 - Crash coverage.**
   Durable modes survive crash points and reopen with correct recovery facts.

7. **T6 - Long-running coverage.**
   Randomized and stress tests exercise interactions across layers and product
   pathways.

Not every layer needs every maturity level. L3 must reach T4 early. L4/L7/L8
must reach T5 before durable local filesystem can be considered V1-ready. L1,
L5, L6, and L9 require strong T1/T2 coverage and targeted T3/T4 coverage, but
they do not independently need process-level T5 crash suites unless they own a
durable publication boundary. L6 crash behavior is primarily exercised through
L7/L8 replay and recovery tests.

## CI Direction

Detailed CI policy belongs in a later implementation plan, but the testing
architecture expects these tiers:

1. Per-PR fast unit and conformance tests.
2. Per-PR memory/cache tests on native and `wasm32-unknown-unknown` once the
   storage-next crate exists.
3. Per-PR local filesystem storage conformance.
4. Nightly crash and stress tests.
5. Scheduled fuzzing.
6. Release-gate golden vector and format compatibility checks.

## Implementation Sequence

The first implementation sequence should be:

1. Build the storage testkit skeleton.
2. Build L1 backend conformance and fault backend harness.
3. Build L2 object-name property tests.
4. Build L3 golden-vector and fuzz harnesses before freezing durable bytes.
5. Build L4 durable publisher and WAL/manifest/snapshot fault-window tests.
6. Build L5 table model/property tests.
7. Build L6 branch visibility model/property tests.
8. Build L7 commit crash-point tests.
9. Build L8 recovery and maintenance deterministic executor tests.
10. Build L9 cache and local filesystem conformance suites.
11. Build resource-profile tests for embedded, desktop, server, unknown, and
    explicit-profile hosts.
12. After engine-next architecture exists, add engine-next product-path tests
    over L9.
13. Add long-running randomized tests after deterministic contracts are stable.

This sequence intentionally front-loads the harnesses. Without those harnesses,
later tests will become bespoke and brittle.

## What Moves Into Later Documents

Later detailed documents should define:

1. Exact backend conformance test names and fixtures.
2. Exact fault backend API.
3. Exact crash harness API.
4. Golden-vector generation and update procedure.
5. Fuzz target list and corpus strategy.
6. Layer-specific model implementations for property tests.
7. CI tiers and which tests run by default.
8. Slow/nightly/stress test policy.
9. Benchmarks and performance regression thresholds.
10. Product-path test matrix above engine-next.

## Acceptance Criteria

This top-level plan is sufficient when:

1. Every storage-next layer L1-L9 has a named test responsibility.
2. Fuzzing responsibilities are assigned to byte/parsing/state-machine layers.
3. Fault-injection responsibilities are assigned to backend, publish, commit,
   recovery, IPC, and provider boundaries.
4. Crash-recovery responsibilities are assigned to durable local filesystem
   paths and never claimed by cache mode.
5. Error tests are tied to the V1 error and diagnostics contract.
6. Test infrastructure is treated as a first-class implementation deliverable.
7. Engine-next can later add product-path tests without reaching around L9.
