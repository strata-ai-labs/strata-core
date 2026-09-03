# Phase 2 Coverage Audit

Audit date: 2026-09-02

Worktree: `strata-core-test-coverage-audit`

Base reviewed: `origin/main` at `8f0bf36e`

Source log: `docs/architecture/v1-test-coverage-program.md`

## Executive Verdict

Phase 2 is correctly closed against its stated exit rule: every Phase 2 gap has
either a landed implementation, a later landed replacement, or an explicit
deferred-register row. The work materially improved coverage: process-kill
recovery, real-binary CLI execution, reachable engine writer-lock contention,
offline inference error handling, wasm execution, facade coverage, and the
multi-branch orphaned-delta guard all exist in the audited tree.

There are still follow-ups that should not be blurred into "done":

1. CLI clone-over-real-HTTP coverage is still missing at the `strata` binary
   layer. Hub-level real HTTP clone coverage exists, but it does not exercise CLI
   parsing, rendering, exit codes, or the binary command route.
2. The inference runtime cache lifecycle remains thin. The fake engine and
   offline download tests are strong; executor fake-service coverage later
   landed in Phase 3; but a hermetic real-runtime cache fill/status/unload
   lifecycle lane is still not present.
3. Close-time flush #2612 was reasonably resolved as not-a-bug for Phase 2, but
   the code-shape harmonization is still real residual work: close runners and
   one cache maintenance runner still call the direct rotate-budget guard, while
   only some ordinary/background flush paths use `decide_flush_rotation`.
4. The Phase 2 ledger has stale wording: #2618 is described as parked in the 2.4
   row even though the audited tree has the fix and regression, the fuzz-target
   count has grown from the old "30" wording to 39, and several deferred-register
   re-entry conditions have fired.

Practical status: keep Phase 2 as "closed with accepted deferrals." Split the
remaining work into targeted cleanup slices rather than reopening all of Phase 2.

## Slice Status

| Slice | Audited status | Scope verdict | Remaining work |
|---|---|---|---|
| 2.1 Process-level crash harness | Closed | Implemented correctly | No Phase 2 gap. Keep power-loss realism owned by FS/reordering suites. |
| 2.2 CI tiers | Closed | Implemented correctly | Update stale counts/wording; do not claim exactly 11 ignored lanes or 30 fuzz targets. |
| 2.3 CLI integration suite | Closed with headroom | Core real-binary suite is correct | Add `strata clone` over real HTTP at CLI layer; legacy shell suites still need repair/retirement. |
| 2.4 Engine branch concurrency races | Closed | Implemented correctly for reachable races | Repair stale #2618 wording; mark old loom/shuttle deferral superseded by Phase 4.3. |
| 2.5 Inference testkit | Closed with open follow-up | Fake engine and offline download lane are correct | Add hermetic runtime cache lifecycle coverage. |
| 2.6 Small zero-coverage surfaces | Closed with headroom | Wasm, facade, and remote-origin rendering are correct | Same CLI clone-over-HTTP gap as 2.3/2.6. |
| 2.7 Multi-branch orphaned-delta recovery | Closed with accepted deferral | Guard-plus-regression strategy is correct | Keep per-branch recovery fix deferred until multi-branch durable maintenance starts. |
| 2.8 Close-time flush surfaces | Resolved not-a-bug with residual | Saturated close/reopen is pinned; no production drain-before-close flush producer found | Either keep residual deferred or harmonize close/cache/background flush runners onto `decide_flush_rotation`. |

## Findings

### P1 - CLI clone-over-HTTP is still not covered at the binary layer

Scope: Phase 2.3 included clone/info rendering, and Phase 2.6 explicitly left
"CLI `clone` end-to-end over HTTP" as remaining headroom
(`docs/architecture/v1-test-coverage-program.md:148`,
`docs/architecture/v1-test-coverage-program.md:151`).

Current evidence:

- `crates/cli/tests/cli_execution.rs:1` states the CLI suite runs the real
  `strata` binary in separate OS processes.
- The suite covers durable KV, JSON/raw output, vector execution, REPL/pipe,
  init, writer-lock contention, #2618, and `remote` on a never-cloned database
  (`crates/cli/tests/cli_execution.rs:53`,
  `crates/cli/tests/cli_execution.rs:116`,
  `crates/cli/tests/cli_execution.rs:327`,
  `crates/cli/tests/cli_execution.rs:365`,
  `crates/cli/tests/cli_execution.rs:397`,
  `crates/cli/tests/cli_execution.rs:431`,
  `crates/cli/tests/cli_execution.rs:633`,
  `crates/cli/tests/cli_execution.rs:667`).
- Real HTTP clone mechanics exist below the CLI in
  `crates/hub/tests/real_transport.rs`: it serves a bundle over an ephemeral
  HTTP server, runs `clone_dataset`, opens the destination, verifies the cloned
  value, and verifies the recorded origin (`crates/hub/tests/real_transport.rs:1`,
  `crates/hub/tests/real_transport.rs:35`,
  `crates/hub/tests/real_transport.rs:88`,
  `crates/hub/tests/real_transport.rs:125`).
- A repository search of `crates/cli/tests`, `scripts/cli-tests`,
  `crates/hub/tests`, and `crates/executor/tests` found no `CARGO_BIN_EXE_strata`
  invocation of `clone`; the clone coverage is hub/executor-level, not CLI
  binary-level.

Verdict: implementation coverage is strong below the CLI, but the exact Phase
2.3/2.6 headroom remains open.

Fix split:

- TCP2.CLI-HTTP-A: lift the ephemeral HTTP bundle server pattern from
  `crates/hub/tests/real_transport.rs` into a CLI integration test.
- TCP2.CLI-HTTP-B: run `strata clone <dataset>` through `CARGO_BIN_EXE_strata`,
  assert exit code, human output, JSON output if supported, destination
  readability, and `strata remote` origin.
- TCP2.CLI-HTTP-C: add at least one failure case proving a failed CLI clone does
  not leave a partial destination and returns the expected stable error surface.

### P1 - Inference runtime cache lifecycle coverage remains thin

Scope: Phase 2.5 called for a fake inference testkit, offline download
failure-path tests, runtime cache lifecycle tests, and an executor deterministic
inference lane (`docs/architecture/v1-test-coverage-program.md:150`).

Current evidence:

- `FakeInferenceEngine` is real and correctly scoped: deterministic
  generation/embedding/ranking, scripted failures, partial item failures,
  redaction, and capability/health checks
  (`crates/inference/src/testkit.rs:1`,
  `crates/inference/src/testkit.rs:69`,
  `crates/inference/src/testkit.rs:185`,
  `crates/inference/src/testkit.rs:643`,
  `crates/inference/src/testkit.rs:734`,
  `crates/inference/src/testkit.rs:761`,
  `crates/inference/src/testkit.rs:779`).
- Offline download tests cover SHA verification, temp-file cleanup, typed stream
  failure, retryability, lock RAII, and URL shape
  (`crates/inference/src/registry/download.rs:311`,
  `crates/inference/src/registry/download.rs:321`,
  `crates/inference/src/registry/download.rs:347`,
  `crates/inference/src/registry/download.rs:375`,
  `crates/inference/src/registry/download.rs:391`,
  `crates/inference/src/registry/download.rs:407`).
- The feature-gated inference lane runs per PR
  (`.github/workflows/ci.yml:74`).
- The executor deterministic lane later landed as TCP3.9c using
  `FakeInferenceService` through executor dispatch
  (`crates/inference/src/testkit.rs:293`,
  `crates/executor/tests/inference_hermetic_behavior.rs:1`,
  `crates/executor/tests/inference_hermetic_behavior.rs:18`,
  `crates/executor/tests/inference_hermetic_behavior.rs:184`).
- `InferenceRuntime::unload` and `InferenceRuntime::cache_status` exist
  (`crates/inference/src/runtime.rs:656`,
  `crates/inference/src/runtime.rs:692`), but the only direct runtime cache
  status unit test is the empty default case
  (`crates/inference/src/runtime.rs:1114`).
- Gated real GGUF integration tests validate actual local generation/chat/embed/
  rank behavior when env vars and models are present, but they early-return when
  `STRATA_RUN_LOCAL_INFERENCE_INTEGRATION` or model paths are absent
  (`crates/inference/tests/local_integration.rs:1`,
  `crates/inference/tests/local_integration.rs:10`,
  `crates/inference/tests/local_integration.rs:27`,
  `crates/inference/tests/local_integration.rs:169`).

Verdict: the fake engine and offline download scope was implemented correctly,
and the executor deterministic dispatch residual was later closed. The runtime
cache lifecycle residual is still open because current hermetic tests do not
prove non-empty cache transitions.

Fix split:

- TCP2.INF-CACHE-A: introduce a hermetic runtime injection seam or fake local
  engine factory that populates the real runtime caches without network or model
  files.
- TCP2.INF-CACHE-B: test generation, embedding, and ranking cache fill; assert
  `cache_status` lists sorted keys.
- TCP2.INF-CACHE-C: test targeted unload, unload-all, repeated unload idempotence,
  and cache-status transitions after each step.
- TCP2.INF-CACHE-D: keep provider/GGUF quality assertions in gated integration;
  keep this lane focused on cache lifecycle semantics.

### P1 - Close-time flush was resolved, but the residual is still meaningful

Scope: Phase 2.8 originally proposed adopting `decide_flush_rotation`, or a
close-specific equivalent, at durable/cache close runners and the cache
background step. The final ledger resolved #2612 as not-a-bug because close
runner refusal lines were considered production-unreachable and saturated
close/reopen was empirically pinned
(`docs/architecture/v1-test-coverage-program.md:153`).

Current evidence:

- The shared helper exists and encodes the intended saturated-flush behavior:
  rotate when affordable, flush backlog without rotation when the pool is
  exhausted but frozen tables exist, and defer only when no frozen backlog can
  make progress (`crates/storage/src/lifecycle/budget.rs:987`,
  `crates/storage/src/lifecycle/budget.rs:1006`).
- Some durable/cache flush rotation entry points use that helper
  (`crates/storage/src/lifecycle/durable/maintenance.rs:473`,
  `crates/storage/src/lifecycle/durable/maintenance.rs:1792`,
  `crates/storage/src/lifecycle/durable/maintenance.rs:4349`,
  `crates/storage/src/lifecycle/cache.rs:752`).
- Durable/cache close runners, the cache background build path, and the cache
  flush maintenance runner still call `require_rotate_budget` directly before
  rotating active rows for a flush task
  (`crates/storage/src/lifecycle/cache.rs:1580`,
  `crates/storage/src/lifecycle/cache.rs:1581`,
  `crates/storage/src/lifecycle/durable/close.rs:319`,
  `crates/storage/src/lifecycle/durable/close.rs:322`,
  `crates/storage/src/lifecycle/cache.rs:2352`,
  `crates/storage/src/lifecycle/cache.rs:2355`,
  `crates/storage/src/lifecycle/cache.rs:2444`,
  `crates/storage/src/lifecycle/cache.rs:2445`).
- The cache background path wraps the direct refusal as a typed failed
  maintenance outcome (`crates/storage/src/lifecycle/cache.rs:2422`).
- `MaintenanceTaskRequest::flush` creates ordinary coalescing flush tasks, not
  drain-before-close flush tasks (`crates/storage/src/lifecycle/maintenance.rs:387`).
  In the audited production source, calls to `drain_before_close()` and
  `coalescing_drain_before_close()` are confined to tests/testkit plus the policy
  constructors.
- The saturated close/reopen regression is real: it stages a low-memory durable
  store, drives it under pressure, requires `close()` to succeed, reopens, scans,
  and classifies the result as `ZeroLoss`
  (`crates/storage/src/testkit/config_differential.rs:638`,
  `crates/storage/src/testkit/config_differential.rs:687`,
  `crates/storage/src/testkit/config_differential.rs:692`,
  `crates/storage/src/testkit/config_differential.rs:711`).
- Close-drain flush publication is tested for the normal durable path
  (`crates/storage/src/lifecycle/tests/durable.rs:1705`), and cache/durable
  close-drain mechanics are covered mostly with explicit test drain tasks
  (`crates/storage/src/lifecycle/tests/cache.rs:1684`,
  `crates/storage/src/lifecycle/tests/cache.rs:1713`,
  `crates/storage/src/lifecycle/tests/durable.rs:2771`).

Verdict: the not-a-bug resolution is acceptable for Phase 2 because the Phase 2
exit rule allowed accepted deferrals and because close-at-saturation is pinned
at the user-visible recovery level. It should not be documented as if the helper
was adopted everywhere. The deferred row for close-runner harmonization remains
valid unless the team intentionally decides to eliminate the residual now.

Fix split:

- TCP2.CLOSE-A: either leave the deferred register row active, or update durable
  and cache close/cache/background flush runners to use `decide_flush_rotation`.
- TCP2.CLOSE-B: if the direct guard remains, add a focused test that stages an
  active flush at close under frozen-pool saturation and asserts the close-retry
  contract plus post-reopen zero-loss behavior.
- TCP2.CLOSE-C: keep the re-entry trigger: any production
  `DrainBeforeClose` flush producer must reopen this item immediately.

### P2 - The Phase 2 ledger and deferred register need cleanup

Scope: The Phase 2 source log is the program ledger. It should distinguish
implemented tests, accepted deferrals, and re-entered deferred work.

Current evidence:

- The 2.4 row says #2618's first-session SIGKILL regression was parked
  (`docs/architecture/v1-test-coverage-program.md:149`), but the audited tree
  has the creation durability barrier and a CLI regression for the killed first
  session (`crates/engine/src/api/database.rs:546`,
  `crates/cli/tests/cli_execution.rs:633`). The Phase 2 exit paragraph already
  says #2618 was fixed (`docs/architecture/v1-test-coverage-program.md:158`), so
  the row is internally stale.
- The 2.2 row says scheduled fuzzing covers "30 existing targets"
  (`docs/architecture/v1-test-coverage-program.md:147`). The fuzz workflow now
  enumerates `cargo fuzz list` dynamically (`.github/workflows/fuzz.yml:40`),
  and the audited tree has 39 files under `crates/storage/fuzz/fuzz_targets`.
- The deferred-register loom/shuttle row says "Revisit at 2.4"
  (`docs/architecture/v1-test-coverage-program.md:666`). Phase 4.3 later landed
  loom-based schedule exploration, so that row should be marked superseded or
  closed by Phase 4.3, not left as an open Phase 2 deferral.
- The branch-ops deferred row says branch merge/compare/promote/restore/revert/
  cherry-pick tests are deferred until ops land
  (`docs/architecture/v1-test-coverage-program.md:662`). Branch diff, merge, and
  preview now exist and were separately audited in Phase 3/4, while restore,
  revert, and cherry-pick remain distinct future surfaces.
- The cross-version metamorphic row says it should re-enter when a second V1 tag
  ships (`docs/architecture/v1-test-coverage-program.md:671`). The audited repo
  has `v1.0.0`, `v1.1.0`, and `v1.1.1`.

Verdict: this is documentation and planning debt, not a product-code blocker for
Phase 2. It matters because the ledger is being used to split future work.

Fix split:

- TCP2.DOC-A: update the 2.4 row to say #2618 was fixed and cite the regression.
- TCP2.DOC-B: replace fixed fuzz/soak counts with dynamic wording or current
  audited counts.
- TCP2.DOC-C: split the branch-ops deferred row into landed branch operations
  that need audited coverage and still-absent operations that remain deferred.
- TCP2.DOC-D: mark the loom/shuttle row as superseded by Phase 4.3.
- TCP2.DOC-E: re-enter the cross-version metamorphic harness now that multiple
  V1 tags exist.

## Detailed Slice Audit

### 2.1 Process-Level Crash Harness

Scope: build a true child-process workload, kill it with `SIGKILL`, reopen the
store, verify STH-1 recovery-oracle properties, prove the verifier detects
sabotage, add a local soak, add a nightly 200-round soak, and repair the
overstated in-process crash title
(`docs/architecture/v1-test-coverage-program.md:146`).

Evidence:

- `process_crash.rs` states the distinction from in-process crash tests and
  describes the intent/ack journal, `SIGKILL`, prefix oracle, in-doubt tail, and
  resumability contract (`crates/storage/src/testkit/process_crash.rs:1`).
- The child writes `intent`, commits with durable Always, writes `ack`, and
  drains maintenance periodically (`crates/storage/src/testkit/process_crash.rs:64`,
  `crates/storage/src/testkit/process_crash.rs:102`,
  `crates/storage/src/testkit/process_crash.rs:115`,
  `crates/storage/src/testkit/process_crash.rs:121`).
- The parent spawns the current test binary, waits for the ack threshold, kills
  the child, and verifies recovery
  (`crates/storage/src/testkit/process_crash.rs:148`,
  `crates/storage/src/testkit/process_crash.rs:160`,
  `crates/storage/src/testkit/process_crash.rs:206`,
  `crates/storage/src/testkit/process_crash.rs:213`).
- The direct test runs multiple kill points, the sabotage test fabricates an ack
  and expects a typed oracle violation, and the ignored soak scales via
  `STRATA_STORAGE_PROCESS_CRASH_ROUNDS`
  (`crates/storage/src/testkit/process_crash.rs:327`,
  `crates/storage/src/testkit/process_crash.rs:353`,
  `crates/storage/src/testkit/process_crash.rs:393`).
- Nightly CI runs the 200-round process-crash soak
  (`.github/workflows/nightly.yml:136`).

Verdict: implemented correctly. The file explicitly scopes out power-loss
realism beyond the page cache and assigns that to the FS/reordering suites
(`crates/storage/src/testkit/process_crash.rs:13`). That is a correct boundary,
not a Phase 2 gap.

### 2.2 CI Tiers

Scope: wire nightly ignored soaks/stress/process-crash, scheduled fuzzing,
wasm32 test execution, and release format/capability gates
(`docs/architecture/v1-test-coverage-program.md:147`).

Evidence:

- Nightly declares the purpose of heavy bug-finding lanes and runs write-ordering,
  process-crash, compound-fault, config-differential, background-liveness,
  crash-recovery grids, stress, coverage, and release-mode workspace tests
  (`.github/workflows/nightly.yml:1`,
  `.github/workflows/nightly.yml:120`,
  `.github/workflows/nightly.yml:153`,
  `.github/workflows/nightly.yml:246`,
  `.github/workflows/nightly.yml:271`,
  `.github/workflows/nightly.yml:326`).
- Scheduled fuzzing restores a persistent corpus and runs every target returned
  by `cargo +nightly fuzz list` (`.github/workflows/fuzz.yml:1`,
  `.github/workflows/fuzz.yml:33`,
  `.github/workflows/fuzz.yml:40`).
- Per-PR CI runs workspace tests, conformance gates, IDL gates, the offline
  inference feature lane, and wasm browser tests
  (`.github/workflows/ci.yml:53`,
  `.github/workflows/ci.yml:57`,
  `.github/workflows/ci.yml:64`,
  `.github/workflows/ci.yml:74`,
  `.github/workflows/ci.yml:286`).
- Release CI gates builds on storage format goldens and engine capability
  conformance (`.github/workflows/release.yml:16`,
  `.github/workflows/release.yml:27`,
  `.github/workflows/release.yml:35`).

Verdict: implemented correctly for Phase 2. The old lane/target counts are stale
because later phases added more lanes and the fuzz target count is now 39. The
Phase 4 release-tag soak gap is separate from Phase 2.2's release format gate.

### 2.3 CLI Integration Suite

Scope: create `crates/cli/tests` with real-binary durable cross-process coverage
for KV, vector, REPL/pipe, init/open/path/output formats, clone/info rendering,
and move phantom plan flags/subcommands to the deferred register
(`docs/architecture/v1-test-coverage-program.md:148`).

Evidence:

- The suite uses `CARGO_BIN_EXE_strata`, separate OS processes, and real cache/
  durable databases (`crates/cli/tests/cli_execution.rs:1`,
  `crates/cli/tests/cli_execution.rs:12`).
- KV round-trip/list/count, JSON and raw output, vector collection/upsert/query,
  piped REPL durability, and init are covered
  (`crates/cli/tests/cli_execution.rs:53`,
  `crates/cli/tests/cli_execution.rs:88`,
  `crates/cli/tests/cli_execution.rs:116`,
  `crates/cli/tests/cli_execution.rs:131`,
  `crates/cli/tests/cli_execution.rs:327`,
  `crates/cli/tests/cli_execution.rs:365`,
  `crates/cli/tests/cli_execution.rs:397`).
- Cross-process writer lock contention, release on SIGKILL, and the #2618
  first-session SIGKILL regression are covered
  (`crates/cli/tests/cli_execution.rs:431`,
  `crates/cli/tests/cli_execution.rs:633`).
- `remote` rendering for a never-cloned database is covered in both JSON and
  human mode (`crates/cli/tests/cli_execution.rs:661`).

Verdict: the real-binary CLI suite is correctly implemented for the core Phase
2 surface. The remaining missing aspect is CLI clone-over-HTTP. Legacy shell
CLI suites should not be counted as clean evidence until their command drift is
repaired or they are retired in favor of the Rust integration suite.

### 2.4 Engine Branch Concurrency Races

Scope: add branch/open race tests, decide whether loom/shuttle is needed for L7
interleavings, and reconcile the originally proposed threaded branch races with
the actual V1 API (`docs/architecture/v1-test-coverage-program.md:149`).

Evidence:

- `branch_faults.rs` explicitly documents why same-handle threaded branch races
  are unreachable: `Database` is not `Clone`, service accessors take `&mut self`,
  and the executor owns the handle by value
  (`crates/engine/tests/branch_faults.rs:1`,
  `crates/engine/tests/branch_faults.rs:4`).
- The reachable race surface is durable-path handle contention: duplicate opens
  have exactly one winner, losers get typed errors, writer lock release on close
  preserves data, and refused openers succeed after the winner exits
  (`crates/engine/tests/branch_faults.rs:27`,
  `crates/engine/tests/branch_faults.rs:83`,
  `crates/engine/tests/branch_faults.rs:122`).
- The CLI suite adds a cross-process contention leg and SIGKILL release proof
  (`crates/cli/tests/cli_execution.rs:431`).
- The #2618 fix exists in engine open: new stores force creation durability
  before returning (`crates/engine/src/api/database.rs:546`), and the CLI
  regression kills the first session and then proves the database is usable
  (`crates/cli/tests/cli_execution.rs:633`).

Verdict: implemented correctly for the reachable V1 concurrency surface. The
main gap is ledger cleanup: #2618 is no longer parked, and the old loom/shuttle
deferral has been superseded by Phase 4.3's loom work.

### 2.5 Inference Testkit

Scope: implement fake inference providers behind the testkit feature, cover the
18 deterministic harness cases, add offline download failure-path tests, and
record runtime cache lifecycle plus executor deterministic dispatch as the next
increment (`docs/architecture/v1-test-coverage-program.md:150`).

Evidence:

- Fake engine implementation and 18-case behavior matrix are present
  (`crates/inference/src/testkit.rs:1`,
  `crates/inference/src/testkit.rs:69`,
  `crates/inference/src/testkit.rs:643`).
- The fake is intentionally not proof of provider quality, tokenizer behavior,
  local runtime lifecycle, or model output (`crates/inference/src/testkit.rs:6`).
- Offline download tests are present
  (`crates/inference/src/registry/download.rs:311`).
- Per-PR inference feature lane is present (`.github/workflows/ci.yml:74`).
- Later Phase 3 executor fake-service dispatch is present
  (`crates/executor/tests/inference_hermetic_behavior.rs:1`).

Verdict: Phase 2.5 is correctly implemented for fake-provider and offline
download scope. Runtime cache lifecycle remains the actionable gap.

### 2.6 Small Zero-Coverage Surfaces

Scope: add wasm-bindgen-test execution over the serialized command adapter,
expand `stratadb` facade coverage, and cover hub/CLI endpoint rendering
(`docs/architecture/v1-test-coverage-program.md:151`).

Evidence:

- Wasm tests drive the full executor-to-storage cache stack through serialized
  commands on `wasm32-unknown-unknown`
  (`crates/wasm/tests/session.rs:1`).
- Wasm cases cover KV round-trip, malformed JSON/unknown commands, executed
  errors as envelopes, branch scoping, space scoping, close refusal, and version
  reporting (`crates/wasm/tests/session.rs:24`,
  `crates/wasm/tests/session.rs:42`,
  `crates/wasm/tests/session.rs:57`,
  `crates/wasm/tests/session.rs:73`,
  `crates/wasm/tests/session.rs:102`,
  `crates/wasm/tests/session.rs:149`,
  `crates/wasm/tests/session.rs:172`).
- CI runs wasm browser tests rather than compile-only wasm checks
  (`.github/workflows/ci.yml:286`).
- `stratadb` facade tests cover cache/durable round-trip, all six data services,
  stable engine error codes/classes, branch forking, and time travel
  (`crates/stratadb/tests/facade.rs:5`,
  `crates/stratadb/tests/facade.rs:38`,
  `crates/stratadb/tests/facade.rs:113`,
  `crates/stratadb/tests/facade.rs:133`,
  `crates/stratadb/tests/facade.rs:197`).
- CLI remote null-origin rendering is covered
  (`crates/cli/tests/cli_execution.rs:661`).
- Hub real HTTP clone is covered below CLI
  (`crates/hub/tests/real_transport.rs:88`).

Verdict: implemented correctly for wasm, facade, and CLI remote. The CLI
clone-over-HTTP gap remains open and should be tracked once, shared with 2.3.

### 2.7 Multi-Branch Orphaned-Delta Recovery

Scope: decide whether to implement full per-branch orphan recovery or keep the
checkpoint guard plus adversarial regression coverage
(`docs/architecture/v1-test-coverage-program.md:152`).

Evidence:

- The implementation plan states the chosen status as guarded: checkpoints
  defer while a non-seeded branch holds a durable table-manifest base, and the
  per-branch fix is deferred to post-V1 multi-branch durable maintenance
  (`docs/architecture/implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md:3`,
  `docs/architecture/implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md:15`,
  `docs/architecture/implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md:49`,
  `docs/architecture/implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md:80`).
- `LifecycleCheckpointStatus` has an explicit
  `DeferredNonSeededBranchBase` status
  (`crates/storage/src/lifecycle/checkpoint.rs:56`,
  `crates/storage/src/lifecycle/checkpoint.rs:61`).
- The structural deferral funnels through `checkpoint_structural_deferral`, and
  the private guard scans non-seeded branches for owned durable table bases
  (`crates/storage/src/lifecycle/checkpoint.rs:1740`,
  `crates/storage/src/lifecycle/checkpoint.rs:1750`,
  `crates/storage/src/lifecycle/checkpoint.rs:1781`,
  `crates/storage/src/lifecycle/checkpoint.rs:1789`).
- Close-drained checkpoints use the stricter any-non-seeded-branch rule because
  the close runner publishes through a seeded-only collector
  (`crates/storage/src/lifecycle/durable/close.rs:110`,
  `crates/storage/src/lifecycle/durable/close.rs:393`).
- Tests cover the main guard, the #2624 close-drain bypass, and the boundary
  where deleting the flushed non-seeded branch releases the guard
  (`crates/storage/src/lifecycle/tests/recovery.rs:2650`,
  `crates/storage/src/lifecycle/tests/recovery.rs:2754`,
  `crates/storage/src/lifecycle/tests/recovery.rs:2762`,
  `crates/storage/src/lifecycle/tests/recovery.rs:2813`,
  `crates/storage/src/lifecycle/tests/recovery.rs:2901`,
  `crates/storage/src/lifecycle/tests/recovery.rs:2949`,
  `crates/storage/src/lifecycle/tests/recovery.rs:3039`).

Verdict: implemented correctly as a guard-now/fix-later slice. This is not a
full product fix; it is a safe, bounded limitation. The per-branch fix remains
high-value future work because it requires a durable per-branch flushed-branch
set, per-branch recovery changes, and multi-branch crash infrastructure.

Note: the implementation-plan line naming
`crates/storage-next/src/lifecycle/tests/recovery.rs` is stale; the audited path
is `crates/storage/src/lifecycle/tests/recovery.rs`.

### 2.8 Close-Time Flush Surfaces

Scope: evaluate #2612, adopt `decide_flush_rotation` or a close-specific
equivalent if needed, and pin saturated close/reopen behavior
(`docs/architecture/v1-test-coverage-program.md:153`).

Evidence:

- The saturated close regression proves user-visible safety under low-memory
  pressure (`crates/storage/src/testkit/config_differential.rs:638`,
  `crates/storage/src/testkit/config_differential.rs:687`,
  `crates/storage/src/testkit/config_differential.rs:711`).
- The close runner drains active and pending close-required maintenance before
  quiescing and closing the WAL (`crates/storage/src/lifecycle/durable/close.rs:143`,
  `crates/storage/src/lifecycle/durable/close.rs:162`,
  `crates/storage/src/lifecycle/durable/close.rs:179`,
  `crates/storage/src/lifecycle/durable/close.rs:205`).
- Direct close-runner, cache background, and cache maintenance flush paths still use
  `require_rotate_budget`, so the code-shape residual is real
  (`crates/storage/src/lifecycle/cache.rs:1580`,
  `crates/storage/src/lifecycle/durable/close.rs:319`,
  `crates/storage/src/lifecycle/cache.rs:2352`,
  `crates/storage/src/lifecycle/cache.rs:2444`).
- The deferred register records that harmonization should re-enter if a
  production `DrainBeforeClose` flush producer appears
  (`docs/architecture/v1-test-coverage-program.md:667`).

Verdict: the Phase 2 outcome is valid as an accepted deferral/not-a-bug, not as
a complete adoption of the new helper across all close-flush code. Keep the
residual visible.

## Recommended Follow-Up Backlog

1. TCP2.CLI-HTTP: add real-binary `strata clone` over an ephemeral HTTP hub,
   including origin rendering and failed-clone cleanup.
2. TCP2.INF-CACHE: add hermetic runtime cache fill/status/targeted-unload/
   unload-all lifecycle tests.
3. TCP2.CLOSE: either harmonize close/cache/background flush runners onto
   `decide_flush_rotation` or add a focused active-flush-at-saturated-close
   regression proving close retry and zero-loss behavior.
4. TCP2.DOC: repair stale Phase 2 ledger language for #2618, fuzz target counts,
   loom/shuttle, branch ops, and cross-version metamorphic re-entry.
5. TCP2.ORPHAN: leave the per-branch orphaned-delta fix in the deferred register
   until multi-branch durable maintenance starts; when it starts, flip the guard
   tests from "defers safely" to "checkpoint completes and per-branch recovery
   returns a clean prefix."

## Audit Notes

This was a repository-evidence audit. I did not rerun the expensive soaks,
nightly jobs, fuzzing, or gated local-inference integrations.
