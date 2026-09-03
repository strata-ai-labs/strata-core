# Phase 4.1-4.12 Coverage Audit

Audit date: 2026-09-02

Worktree: `strata-core-test-coverage-audit`

Base reviewed: `origin/main` at `8f0bf36e`

Source log: `docs/architecture/v1-test-coverage-program.md`

## Executive Verdict

Phase 4 has substantial, high-quality implementation, but it is not fully
certifiable as closed. The individual slice harnesses are mostly real and
well-structured: they have deterministic seeds, non-vacuity checks, sabotage
twins, shrink-only ledgers, and nightly/per-PR tiering. The gaps are now
concentrated in four places:

1. Release certification is missing: the charter requires a pre-release soak in
   `release.yml`, but the release workflow only gates format goldens and the
   capability contract before building artifacts.
2. 4.1 is only the 4.1a implementation. The declared 4.1b scope still has no
   matching generator.
3. Several slices are implemented as harnesses but intentionally not closed
   because they still carry open product divergences or pinned defects: 4.7,
   4.8, and 4.9.
4. 4.12 is landed for lineage and read-atomicity histories, but it is still a
   subset of the announced elle/Adya scope.

## Phase 4 Gate Findings

### P0 - Release-tag Phase 4 soak is not wired

The Phase 4 charter explicitly requires a four-tier CI structure, including a
pre-release soak on the release tag that runs the full generated volume across
corpora, config matrix, and fault schedules in `release.yml`
(`docs/architecture/v1-test-coverage-program.md:367`). The current release
workflow's first gate runs only:

- `cargo test --locked -p strata-storage --test format_golden`
- `cargo test --locked -p strata-engine --test capability_conformance`

That is visible in `.github/workflows/release.yml:16`. This means a release tag
does not re-run the Phase 4 generated/differential/fuzz/DST/history volume the
charter says must certify it. This is a certification blocker even where the
underlying slice implementations are strong.

### P0 - No evidence of the promised mutation-kill plateau by gap class

The charter makes mutation-kill the primary ratchet and says every gap class is
not done until its surface-specific mutation-kill rate plateaus
(`docs/architecture/v1-test-coverage-program.md:355`,
`docs/architecture/v1-test-coverage-program.md:380`,
`docs/architecture/v1-test-coverage-program.md:499`). CI has a valuable
per-PR `cargo mutants --in-diff` gate (`.github/workflows/ci.yml:194`), but I
did not find a Phase 4 artifact that records plateau results by slice/gap class.

The practical fix is to add a small certification ledger: slice, surface,
mutant set, killed/survived/timed-out, equivalent exclusions, date, commit,
and whether the surface has plateaued. Without that, "Phase 4 closed" cannot be
audited independently.

### P1 - Active shrink-only pins mean several slices are hunt-ready, not closed

Shrink-only pins are the right implementation pattern, but they are not closure.
4.7 has nine live parity divergences in
`crates/executor/idl/v1/cross-surface-divergences.yaml:11`. 4.8 has #2749 and
#2750 pinned in `crates/executor/tests/error_contract.rs:18`. 4.9 has #2754
and #2567 pinned in `crates/executor/tests/artifact_faults.rs:405` and
`crates/engine/tests/recovery_budget.rs:180`.

### P2 - Phase 4 documentation is stale in multiple places

The code is often ahead of the log:

- 4.4 says #3024 is pinned open, but `graph_conformance.rs` says the CDLP pin
  was promoted by the fix (`crates/executor/tests/graph_conformance.rs:349`).
- 4.5 starts as "In progress" in the table, while the implemented code covers
  4.5a/b/c.
- 4.11 is closed in the main log and nightly workflow, but
  `crates/storage/src/testkit/simulation/README.md:72` still says the
  whole-DB soak line is deferred until #2828 closes.
- 4.12's simulation README still labels 4.12b as planned
  (`crates/storage/src/testkit/simulation/README.md:136`), even though
  `concurrent_history.rs` exists and nightly runs it.

## Slice Audit

### 4.1 - IDL conformance generator

Scope: Generate per-command conformance from the IDL: request/response
round-trip across render modes, declared error envelopes, schema/boundary
conformance, adversarial deserialize, outer-surface REPL/pipe fidelity for
#2571, and declared-vs-observed output/help parity for #2596/#2569.

Implemented: 4.1a only. `tests_gen.rs` declares and emits four generated
families: wire round-trip idempotence, nested unknown-key rejection, declared
error-envelope replay, and observed-output tag subset checks
(`crates/executor/src/idl_tooling/tests_gen.rs:1`,
`crates/executor/src/idl_tooling/tests_gen.rs:160`). The generated suite is
CI-gated by `check-tests` and `generated_conformance`
(`.github/workflows/ci.yml:64`). The unknown-key divergence ledger is now empty,
which means #2739 was resolved and the generated unknown-key checks are no
longer skipping known exceptions
(`crates/executor/idl/v1/unknown-key-divergences.yaml:12`).

Correctness verdict: Implemented correctly for 4.1a. The structure is good:
generation is deterministic, assertion logic is centralized in support helpers,
and stale divergence entries fail the build.

Remaining aspects:

- Render-mode goldens for json/raw/human output.
- Schema-guided adversarial mutations beyond unknown keys.
- REPL/pipe/argv text round-trip starting outside the `Command` struct (#2571).
- Boundary-value generation from IDL schemas.
- Help text parity against the IDL (#2569).

Status: Partial. Do not mark 4.1 closed until 4.1b lands.

### 4.2 - Differential testing vs reference databases

Scope: Differential test each model against a mature reference where possible:
KV vs RocksDB/Redis, JSON vs MongoDB, graph vs Neo4j, event vs Redis Streams,
vector vs exact kNN. Also promised deeper Strata-only metamorphic sequences for
branches, time travel, and lifecycle churn.

Implemented: All five current capability families have landed reference oracles.
KV uses RocksDB plus Redis and includes an as-of/snapshot tier
(`crates/executor/tests/differential_kv.rs:1`,
`crates/executor/tests/differential_kv.rs:490`). JSON, graph, and event use
live external references with positive controls and record-time validated
corpora (`crates/executor/tests/differential_json.rs:1`,
`crates/executor/tests/differential_graph.rs:1`,
`crates/executor/tests/differential_event.rs:1`). Vector uses an in-process
exact kNN oracle and runs without external service dependencies
(`crates/executor/tests/differential_vector.rs:1`). Committed JSONL corpora
replay per PR without live services
(`crates/executor/tests/corpus_replay.rs:1`).

Correctness verdict: Implemented correctly for the external/reference sweep.
The harnesses restrict domains to shared semantics, use positive controls before
trusting a diff, and have deterministic corpus recording/replay discipline.

Remaining aspects:

- Deep branch/time-travel op-sequence metamorphic tier from the slice scope.
- Deep fork DAGs and delete/recreate lifecycle churn called out in the gap table.
- Corpus growth, especially if KV is expected to have committed replay corpora
  symmetrical with JSON/graph/event/vector.
- Nightly workflow comment is stale: it describes 4.2a/b while the job runs
  4.2c/d/e too (`.github/workflows/nightly.yml:365`).

Status: Implemented for 4.2a-e; partial against the full written scope because
the Strata-only metamorphic tier remains.

### 4.3 - Exhaustive concurrency schedule exploration

Scope: Explore commit/write-group/latest-scan interleavings with loom or shuttle,
using #2682 as the seed torn-read class.

Implemented: The `crate::sync` seam swaps std synchronization for loom under
`--cfg loom`, and the write-group protocol is generic over that seam
(`crates/storage/src/sync.rs:1`,
`crates/storage/src/api/runtime/commit_group.rs:19`). The loom models drive the
real protocol code for queue leadership, apply handoff, sync-chain coverage, and
visibility publishing (`crates/storage/src/api/runtime/commit_group_loom.rs:1`).
The branch visibility loom models drive the actual product code below
`load_published_snapshot` and cover live memtable scans, rotation republish,
applied-not-visible, flush install, and sabotage twins
(`crates/storage/src/branch/visibility_loom.rs:1`).

Correctness verdict: Implemented correctly for the targeted V1 concurrency
surfaces. The important part is that the models exercise real protocol code
rather than handwritten shadows, and the sabotage twins prove oracle power.

Remaining aspects:

- Optional higher preemption bounds.
- Optional shuttle/randomized whole-runtime schedule exploration on the same
  seam.

Status: Closed for the stated V1 scope; optional expansion remains.

### 4.4 - Vendored public conformance suites

Scope: Import external conformance corpora where V1 exposes an external contract
or validated ground truth: JSON parsing, number semantics, vector exact kNN, and
graph analytics.

Implemented: JSONTestSuite is driven through the real wire ingress
(`crates/executor/tests/json_conformance.rs:1`). JSON number semantics are an
authored no-bless contract (`crates/executor/tests/json_number_contract.rs:1`).
Vector conformance uses SIFT/TEXMEX exact kNN ground truth across all three
metrics (`crates/executor/tests/vector_conformance.rs:1`). Graph conformance
uses LDBC Graphalytics reference outputs for BFS, PageRank, WCC, CDLP, LCC, and
SSSP (`crates/executor/tests/graph_conformance.rs:1`).

Correctness verdict: Implemented correctly and scoped to actual V1 claims. The
N/A decisions for JSONPath, Cypher, collation, and KV external specs are
reasonable because those are not V1 public contracts.

Remaining aspects:

- Update the program row: #3024 is no longer pinned open; CDLP was promoted to
  a fixed contract (`crates/executor/tests/graph_conformance.rs:349`).
- Future external contracts need their own vendored suites when they become
  real surfaces.

Status: Closed, with documentation drift.

### 4.5 - Golden/combinatorial matrix

Scope: Complete durable-format golden vectors, add adversarial decode contract,
and run a config x capability x operation cross-product.

Implemented: The storage format golden matrix includes canonical and boundary
vectors for durable record types (`crates/storage/src/format/tests.rs:941`).
The adversarial matrix mutates every golden by truncation, byte flip, and junk
extension, then compares against a pinned contract
(`crates/storage/src/format/tests.rs:1159`). The integration entry point checks
golden inventory and fuzz-corpus seed drift
(`crates/storage/tests/format_golden.rs:9`,
`crates/storage/tests/format_golden.rs:66`). The wire config-invariance test
runs one seeded script across cache/durable, Standard/Always durability, and
budget configurations for KV, JSON, events, vectors, and graph
(`crates/executor/tests/config_invariance.rs:1`). Nightly has both debug and
release deep tiers (`.github/workflows/nightly.yml:231`,
`.github/workflows/nightly.yml:340`).

Correctness verdict: Implemented correctly. The combination of byte goldens,
adversarial decode, corpus seed drift checks, semantic equality across configs,
and mutation-volume floors matches the slice intent.

Remaining aspects:

- Keep adding goldens when new durable record types or format extensions are
  introduced.
- Make release-tag soak run this matrix, not only nightly.
- Clean up stale "In progress" wording in the program row.

Status: Closed for current V1 surfaces; release certification still missing.

### 4.6 - Committed fuzz corpora and surface expansion

Scope: Version persistent fuzz corpora, expand fuzz targets across decoder/API
surfaces, add round-trip/value-fidelity oracles, add dual-mutation fuzzing, and
seed corpus recombination from real artifacts.

Implemented: Wire value-fidelity sweeps cover #2689 vector f64 to f32 narrowing
and #2688 vector Arrow metadata export/import fidelity
(`crates/executor/tests/value_fidelity.rs:1`). Format fuzz targets carry a
round-trip fidelity oracle and every `FormatDecoder` variant has a dedicated
target (`crates/storage/fuzz/README.md:8`,
`crates/storage/fuzz/README.md:50`). The layer presence guard prevents new
decoder layers from silently lacking a fuzz target
(`crates/storage/tests/layer_fuzz_presence_guard.rs:17`). The dual-mutation
target co-mutates operations and on-disk bytes over real durable stores
(`crates/storage/src/testkit/dual_mutation.rs:1`,
`crates/storage/fuzz/fuzz_targets/dual_mutation.rs:1`). Corpus harvest drives a
real durable store and emits real artifacts as seeds, then per-PR drift gates
verify committed seeds still decode and round-trip
(`crates/storage/src/testkit/corpus_harvest.rs:1`,
`crates/storage/src/testkit/corpus_harvest.rs:420`). Scheduled fuzzing restores
and extends a persistent corpus, then enumerates every target with
`cargo fuzz list` (`.github/workflows/fuzz.yml:1`).

Correctness verdict: Implemented correctly. This is one of the strongest Phase
4 slices: it has value-loss oracles, live target inventory, real-artifact seed
generation, and a whole-store dual-mutation harness.

Remaining aspects:

- Add a fuzz target for the separate `meta/wal-watermark` format if that remains
  outside the existing `FormatDecoder::Watermark` target.
- Continue promoting minimized fuzz findings into named corpus seeds.
- Release-tag soak should restore/run the accumulated corpus, not just nightly.

Status: Closed for the current committed scope; one noted surface-expansion
candidate remains.

### 4.7 - Cross-surface parity harness

Scope: Internal differential testing across analogous Strata surfaces:
export/import, branch/space, batch failures, event range endpoints/direction,
as-of read coverage, catalog executability, and inert feature detection.

Implemented: The harness is present and well-disciplined. A YAML divergence
ledger records each open parity discrepancy and requires a filed issue
(`crates/executor/idl/v1/cross-surface-divergences.yaml:1`). Shared support
enforces both pin-implies-entry and entry-implies-pin
(`crates/executor/tests/parity/support.rs:136`). Tests cover event range
direction/endpoint (#2694/#2695), branch/space lifecycle/resolution/naming
(#2700), batch failure channels and caps (#2701), as-of coverage (#2702), JSON
scan/catalog naming (#2704), and the fixed export/import primitive symmetry
contract (#2691).

Correctness verdict: Implemented correctly as a parity detection harness, but
not closed as a product-quality slice because the ledger is still populated.

Remaining aspects:

- Resolve or explicitly defer the nine current ledger entries:
  #2694, #2695, #2700 x3, #2701 x2, #2702, #2704 x2.
- Add the vector query envelope-shape parity wrinkle observed by 4.10 if it is
  meant to be a stable cross-surface contract.
- Inert feature detection still belongs with 4.10's planner/de-optimization
  work, not the current 4.7 ledger.

Status: Harness implemented; slice not closed until the ledger shrinks or each
entry is explicitly accepted as V1 design.

### 4.8 - Error-contract correctness harness

Scope: Drive real failure conditions through the public surface and assert the
condition-to-error mapping: code, class, retryability, commit outcome, redaction,
and registry coherence.

Implemented: `error_contract.rs` drives the #2699 open-path seed failures
through the wire layer and checks mapping plus envelope coherence
(`crates/executor/tests/error_contract.rs:1`,
`crates/executor/tests/error_contract.rs:167`). It sweeps public registry
semantics for permanent, transient, ambiguous, and internal classes
(`crates/executor/tests/error_contract.rs:371`). It is included in CI
conformance gates (`.github/workflows/ci.yml:57`).

Correctness verdict: The harness is implemented correctly. It catches the exact
class of bug that existence-only error-code tests miss.

Remaining aspects:

- #2749: `data_loss.*` codes still surface public class `corruption`; this is
  pinned in the harness (`crates/executor/tests/error_contract.rs:639`) and in
  the public class mapping (`crates/engine/src/diagnostics/error.rs:496`).
- #2750: feature-disabled codes are still `invalid_argument` with
  `AfterStateChange`; pinned at `crates/executor/tests/error_contract.rs:375`.
- The adjacent replay allowlist still has 110 entries
  (`crates/executor/idl/v1/unreplayed-error-codes.yaml:21`). This is not a
  4.8 harness defect, but it is remaining error-contract coverage debt.
- Extend write-path fault fixtures beyond KV and add engine-side
  `EngineErrorClass` to public-class parity sweeps.

Status: Harness complete; error-contract closure blocked by pinned divergences
and large replay debt.

### 4.9 - Fault-taxonomy extension and health-vs-truth oracle

Scope: Extend faults from corrupt/truncate/io-fail to delete/missing/reorder at
artifact granularity, include disk-full/allocation failure, assert
health-vs-truth after faults, add recovery/maintenance memory budget adherence,
and evolve process crash into a continuous randomized crash test.

Implemented: Artifact absence/reorder tests run through the public executor
surface and compare read-back truth against health reporting
(`crates/executor/tests/artifact_faults.rs:1`). Sole WAL segment deletion and
whole-WAL-directory loss were promoted to permanent corruption after #2765
(`crates/executor/tests/artifact_faults.rs:298`). Multi-segment WAL loss is
covered in-crate with a tiny segment-size knob
(`crates/storage/src/testkit/wal_segment_loss.rs:1`). Disk-full/quota and
continuous sync failure are covered in the storage fault sweep
(`crates/storage/src/testkit/fault_sweep/mod.rs:610`). Recovery budget
measurement exists as a Linux/localfs test
(`crates/engine/tests/recovery_budget.rs:1`). The process-level SIGKILL harness
exists and has a nightly 200-round soak
(`crates/storage/src/testkit/process_crash.rs:1`,
`.github/workflows/nightly.yml:136`).

Correctness verdict: Implemented correctly for 4.9a/b, but the slice is
genuinely partial. The code itself says the public artifact stage is one WAL
segment only and pushes multi-segment faults to the storage-seam increment
(`crates/executor/tests/artifact_faults.rs:152`).

Remaining aspects:

- #2754 snapshot-object absence still reports retryable `unavailable` instead
  of permanent corruption (`crates/executor/tests/artifact_faults.rs:405`).
- #2567 recovery memory budget is still ignored and pinned shrink-only
  (`crates/engine/tests/recovery_budget.rs:180`).
- Durable-table family faults need a storage-seam increment.
- Health-vs-truth after runtime faults, not only reopen/artifact faults.
- Continuous randomized full-surface crash testing with whitebox filesystem-op
  crash points plus blackbox SIGKILL under sanitizer builds.

Status: Partial. Good harnesses exist, but product defects and scope gaps remain.

### 4.10 - Single-system logic-bug oracles

Scope: SQLancer-style single-system oracles: pivot containment, de-optimization,
predicate partitioning, write/read predicate consistency, compound graph
metamorphic relations, and eventually query-plan-guided generation.

Implemented: Pivot containment covers KV prefix/scan, JSON prefix/path, and
event containment (`crates/executor/tests/oracle_pivot.rs:1`). Partitioning/TLP
covers KV and event partitions plus the fixed unit-weight SSSP vs BFS contract
(`crates/executor/tests/oracle_partition.rs:1`). De-optimization compares
`vector_query` against `vector_index_query` and JSON results before/after
index create/drop (`crates/executor/tests/oracle_deopt.rs:1`). DQE-style
write/read predicate agreement covers vector filter deletes and JSON prefix
batch deletes (`crates/executor/tests/oracle_dqe.rs:1`). Graph MRs cover edge
type partitioning, node type partitioning, BFS monotonicity, and WCC merge
under a bridge edge (`crates/executor/tests/oracle_graph_mr.rs:1`).

Correctness verdict: Implemented correctly for the current oracle set. The
assertions check algebraic identities or containment properties rather than
hard-coded expected outputs.

Remaining aspects:

- It does not detect an inert acceleration path by itself; `oracle_deopt.rs`
  explicitly says equality is trivially satisfied while JSON indexes are inert
  and that inertness requires an explain/stats observable
  (`crates/executor/tests/oracle_deopt.rs:6`).
- Query-plan-guided generation remains unimplemented.
- More capability coverage is possible: richer JSON predicates, vector
  filter/path combinations, branch/time-travel variants, and graph analytics MRs
  over generated rather than tiny authored graphs.

Status: Implemented for 4.10a/b; partial against the broader logic-oracle
ambition.

### 4.11 - Deterministic whole-DB simulation harness

Scope: Seed-reproducible control of time, I/O, entropy, workload, and fault
schedule at storage L1 boundaries, with continuous invariant oracles and
bit-exact replay from seed plus commit.

Implemented: The simulation substrate is real and shared across clean,
fault/crash, and whole-DB lanes (`crates/storage/src/testkit/simulation/mod.rs:1`).
The whole-DB driver runs multi-branch, multi-epoch crash/recover/continue
trajectories with per-branch prefix checks, temporal probes, maintenance failure
ring, write-ordering watchdog, and branch-catalog health-vs-truth
(`crates/storage/src/testkit/simulation/whole_db.rs:1`). Replay lines are
generated from failing seeds and `replay_single_seed` executes them
(`crates/storage/tests/simulation_whole_db.rs:87`). Nightly runs both wide and
deep whole-DB soaks (`.github/workflows/nightly.yml:201`).

Correctness verdict: Implemented correctly and closed for the DST exit gate.
The code has determinism twins, non-vacuity counters, exact-counter pins, and
promoted regression seeds for prior findings.

Remaining aspects:

- Clean up stale simulation README text that still says the whole-DB soak is
  deferred until #2828 closes (`crates/storage/src/testkit/simulation/README.md:72`).
- Non-blocking expansion: read-only continuation after degraded health and
  in-doubt commit recording under fault epochs.

Status: Closed for Phase 4, with documentation drift and optional hardening.

### 4.12 - History-based isolation and lineage checking

Scope: Record client-observed operation histories and check them offline using
elle-style history reconstruction, Adya-style anomaly inference where
applicable, and Strata-specific invariants: as-of state, fork isolation,
lineage transitivity, and event ordering/monotonicity. The scope explicitly
includes fault-free and faulted generated workloads.

Implemented: 4.12a records single-session lineage/temporal histories and checks
reads by reconstructing expected state from the recorded writes and forks alone
(`crates/storage/src/testkit/simulation/history.rs:1`). It covers fork-current
chains, latest reads, at-version reads, version monotonicity, and sabotage twins
for dropped inheritance and wrong as-of reads
(`crates/storage/src/testkit/simulation/history.rs:571`). 4.12b adds concurrent
storage-session histories for read atomicity: linked keys are written together
with one stamp, readers scan the linked set, and the offline checker flags
fractured reads and phantom values
(`crates/storage/src/api/tests/concurrent_history.rs:1`,
`crates/storage/src/api/tests/concurrent_history.rs:102`). Nightly runs both
lineage and concurrent history soaks (`.github/workflows/nightly.yml:206`,
`.github/workflows/nightly.yml:218`).

Correctness verdict: Implemented correctly for 4.12a and the 4.12b subset. The
workloads are traceability-co-designed, and the offline checkers have sabotage
twins.

Remaining aspects:

- Full Adya SSG cycle inference is explicitly still headroom
  (`crates/storage/src/api/tests/concurrent_history.rs:25`).
- Faulted concurrent histories are still headroom.
- Pruned-history trajectories are outside the current 4.12a lane
  (`crates/storage/src/testkit/simulation/history.rs:20`).
- Event-log ordering/monotonicity histories are described in the slice scope but
  are not a distinct 4.12 implementation target in the code I found.
- Update `simulation/README.md`, which still calls 4.12b planned
  (`crates/storage/src/testkit/simulation/README.md:136`).

Status: Partial. The landed subset is strong, but the full advertised elle
scope is not complete.

## Recommended Split For Follow-up Work

1. Certification lane: wire the release-tag Phase 4 soak, add the mutation
   plateau ledger, and refresh stale docs.
2. 4.1b: render-mode goldens, schema-guided boundary/adversarial generation,
   REPL/pipe/argv round-trip, and help parity.
3. Product divergence cleanup: drain or explicitly accept 4.7 ledger entries,
   fix #2749/#2750, fix #2754, and replace #2567 with a budget envelope
   contract.
4. 4.9 expansion: durable-table artifact faults, runtime health-vs-truth, and
   continuous randomized crash testing under sanitizer builds.
5. 4.10 expansion: query-plan observability and QPG/inert-index detection.
6. 4.12 expansion: full Adya SSG inference, faulted/pruned histories, and
   explicit event-log history checking.
