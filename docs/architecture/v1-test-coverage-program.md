# V1 Test Coverage Program

Status: active — Phases 1–3 closed (2026-07-16 / -18); Phase 4 (volume via generation) open
Created: 2026-07-16

## Purpose

This document is the working charter for closing the known test gaps in the V1
tree and then raising coverage layer by layer toward a reference-grade suite.
It sequences the remaining STH storage-testing slices, the gaps found in the
2026-07-16 plan-vs-implementation audit, and the later per-layer coverage
plans. Detailed slice plans stay in their own documents; this file owns the
order, the gates, and the ledger.

Related documents:

1. `docs/architecture/v1-testing-and-conformance-plan.md` — layer test plan (L1-L9, T0-T6 maturity)
2. `docs/architecture/implementation-plans/storage-testing/README.md` — STH program (SQLite-derived)
3. `docs/architecture/v1-storage-testing-gold-standard-delta.md` — SQLite delta analysis
4. `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md` — 12 bug classes

## Principles

1. **Coverage over line count.** The goal is proven behavior, not test LOC.
   The test:code ratio is tracked as a health stat, never gated. SQLite sits
   at ~590x; our working floor is 4-5x, reached by closing coverage gaps, not
   by writing lines.
2. **Oracles are reused, not duplicated.** New failure tests assert through
   the STH-1 recovery oracle and existing reference models.
3. **A suite that CI never runs does not exist.** Every lane built under this
   program gets a CI tier (per-PR, nightly, or scheduled) in the same slice.
4. **Documents never overstate.** Status headers and mapping tables say
   "planned" until code is merged. Part of this program is repairing docs
   that currently claim more than the tree contains.
5. **Deferred is a status, not an absence.** Anything we choose not to test
   (merge semantics, M6E retrieval, intelligence crate) is recorded in the
   deferred register below with a re-entry condition.

## Baseline (2026-07-16)

Measured on `main` (excludes `target/`, docs):

| Bucket | LOC |
|---|---|
| Integration tests (`crates/*/tests/`) + fuzz targets + storage testkit | ~106,000 |
| In-src test modules (`src/**/tests/`) | ~131,000 |
| Inline `#[cfg(test)]` tails (approx.) | ~61,000 |
| **Total test code** | **~298,000** |
| Product source (src minus test modules/testkit) | ~164,000 |
| **Test:code ratio** | **~1.8x** |

Test counts: storage ~4,100 tests, engine ~509, inference ~558, executor
large contract suite, CLI 45 (unit only), wasm 0, stratadb facade 1.

Coverage baseline (region coverage, `cargo llvm-cov --workspace`, default
features, measured 2026-07-16 under TCP1.1; the nightly `coverage-baseline`
job republishes this table daily):

| Crate | Region | Line | Note |
|---|---|---|---|
| core | 94.2% | 94.7% | |
| inference | 91.7% | 92.0% | gated provider/local lanes excluded by default |
| executor | 85.7% | 85.7% | |
| engine | 83.8% | 82.9% | |
| storage | 70.1% (86.6% excl. testkit) | 71.5% (87.1%) | testkit soak harnesses sit at 0% because their `#[ignore]` lanes never run in the default suite |
| hub | 67.4% | 70.6% | |
| gpu-cache | 60.8% | 59.4% | device lanes skip without GPU |
| cli | 39.1% | 38.3% | matches the audit: no integration suite |
| wasm | 0.0% | 0.0% | zero tests |
| **workspace** | **73.6%** | **74.3%** | |

Coverage (not ratio) is the primary metric from here; thresholds ratchet up
only (gate lands in STH-7b).

## Phase 1 — Close STH-1..7 properly

The STH program (SQLite-derived) is the priority: STH-1..4 are landed,
STH-5..7 are drafts, STH-3 has a deferred sub-slice. Order chosen by
cost/unblock value:

| # | Slice | Scope | Status |
|---|---|---|---|
| 1.1 | **STH-7a** (cheap half) | Miri + ASAN/LSAN jobs; `cargo-llvm-cov` baseline job publishing per-crate coverage; 3-way suite run (release / debug-asserts / coverage) | Implemented (2026-07-16) — `.github/workflows/nightly.yml`; baseline coverage recorded below |
| 1.2 | **STH-5** | Failure-during-failure: inject faults *during* recovery, compaction, checkpoint, quarantine; assert through the STH-1 oracle. Plan doc exists (draft) | Implemented (2026-07-16) — `testkit/compound_faults`, nightly `failure-during-failure-soak`; charter class 6 ❌→✅; 2,000-case soak clean, no product defect |
| 1.3 | **STH-3b** | Write-ordering watchdog: assert no dependent publish precedes its WAL sync (SQLite journal-synced-before-db check) | Implemented (2026-07-16) — `testkit/write_ordering_watchdog`, nightly entry; charter class 3 🟡→✅; Always/Standard/rotation/recovery streams clean |
| 1.4 | **STH-6** | Config-sweep differential (cache vs durable vs budget configs, identical results) + metamorphic oracles; liveness deepening | Implemented (2026-07-16) — `testkit/config_differential` + `api/tests/liveness_matrix`; charter class 2 🟡→✅, class 8 broadened; **found issue #2609** (EvaluateAndEnqueue pressure livelock), fixed by #2613 with the invariant-checked audit protocol — regressions live, mutation gate vetted the fix; perf-trace endurance suite now runs nightly |
| 1.5 | **STH-7** (full) | `cargo-mutants` gate on storage; coverage ratchet; `testcase!`/`always!`/`never!` macros; requirements-to-test traceability; anti-drift guard that fails when a plan doc claims ✅ for an unimplemented item | Implemented (2026-07-16) — diff-scoped mutants per PR (`ci.yml`), coverage floor gate (73.0% ratchet-up), nightly persistent-corpus fuzz (`fuzz.yml`), charter guard (`testing_charter_guard.rs`). Deferred with reasons in the STH-7 as-built: MC/DC, `testcase!` macros, full-tree mutation |
| 1.6 | **Doc repair** | Fix STH-1 header (says draft, is implemented); split gold-standard delta table into "mapped" vs "built" columns; update STH README status column | Implemented (2026-07-16) — plus taxonomy frontier prose refreshed and #2609 wording moved from parked to fixed |
| 1.7 | **Storage leak-registry migration** | Replace ~609 `Box::leak` test-fixture sites in `crates/storage` with a testkit `leak_static` helper that keeps leaked fixtures reachable from a global registry, then flip `detect_leaks=1` on the nightly storage ASAN lane. Found under TCP1.1: LSAN reported ~190 KB / 5,518 allocations, all traced to intentional fixture leaks that would drown any real leak | Implemented (2026-07-16) — `testkit::leak_static` + `forget_registered`, 609 sites + 1 `mem::forget` migrated; storage LSAN lane live |

Phase 1 exit gate: all 12 bug classes in the taxonomy doc at their stated
exit bar, coverage baseline published, no stale status headers under
`storage-testing/`.

**Phase 1 closed 2026-07-16.** All twelve classes at their exit bar, coverage
baseline published and gated (73.0% floor), every `storage-testing/` status
header current, and the charter guard enforces the map from here on. Program
results so far: one product bug found and fixed (#2609 → #2613), one latent
close-surface defect filed (#2612), two durability-lane defects in the nightly
workflows caught before their first scheduled run, and the mutation gate's
first production catch (6 missed mutants) killed.

## Phase 2 — Close the 2026-07-16 audit gaps

Ranked by risk (full findings in the audit conversation; re-verify each
before starting):

| # | Gap | Scope |
|---|---|---|
| 2.1 | **Process-level crash harness (T5)** | Child-process workload + `SIGKILL` at oracle-chosen points + reopen assertions through the STH-1 oracle. The plan's level-2 crash requirement; today every "crash" is in-process. Rename or fix `crash_recovery.rs`'s overstated title in the same slice | Implemented (2026-07-16) — `testkit/process_crash.rs`: intent/ack journal child + SIGKILL parent + oracle verify + resume; sabotage test proves the verifier detects loss; 25-round local soak clean, 200-round nightly; `crash_recovery.rs` title repaired |
| 2.2 | **CI tiers** | Nightly workflow running the 11 `#[ignore]` soak lanes + `stress.rs` + process-crash harness; scheduled fuzz over the 30 existing targets (libFuzzer, corpus cached); wasm32 memory/cache *test* job (today: compile-only); golden/format gate in `release.yml`. **Implemented (2026-07-16)** — nightly `storage-soak-lanes` job (fault-sweep/FS-model/simulation soaks + env-scaled crash and stress grids; every command verified locally), scheduled fuzz landed in TCP1.5 (`fuzz.yml`), `release.yml` gained a `format-gate` job (goldens + capability contract on the tagged commit, gating `build`). Deliberately moved: wasm32 *test execution* (not just compile) needs the `wasm-bindgen-test` harness — built once in 2.6 alongside the wasm crate's first tests rather than twice; wasm compile coverage already exists transitively via the `wasm` CI job |
| 2.3 | **CLI integration suite** | `crates/cli/tests/` with `assert_cmd`: durable cross-process execution (KV + vector execution plans have ready case lists), REPL/pipe scripts, init/new, open/path, output-format round-trips, clone/info rendering. Reconcile plan items with no backing flags (`--memory-budget`, `--profile`, `commands`, `explain`) — implement or move to deferred register. **Implemented (2026-07-16)** — `crates/cli/tests/cli_execution.rs` (zero new dependencies: `CARGO_BIN_EXE_strata` process spawns): durable cross-process KV round-trips/list/count, vector collection+upsert+query, branch-scoped isolation, piped-REPL execution with post-session durability, cache-per-process ephemerality, typed no-database/conflict refusals (exit 2), `STRATA_DB` targeting, `--json` envelope + `--raw` contracts, init idempotence, info/health/describe JSON. Phantom plan flags moved to the deferred register; hub-facing `clone`/`remote` rendering lands with the 2.6 hub endpoint suite; the full per-plan case grids remain headroom |
| 2.4 | **Engine branch concurrency races** | The unwritten `branch_faults.rs`: concurrent same-name create (one winner), delete-vs-write, recreate-vs-stale-write, concurrent fork. Decide loom/shuttle adoption for L7 guard interleavings while here. **Implemented (2026-07-16)** — `crates/engine/tests/branch_faults.rs` with the races that are *reachable*: the planned threaded same-name races are unreachable by construction (non-`Clone` `Database`, `&mut self` services, by-value executor ownership — borrowck is the staleness guard; sequential forms already in `branch_semantics.rs`). Now tested: 8-thread duplicate-open race (exactly one winner, typed refusals, functional winner), lock release on close with data survival, refused-opener retry, plus the cross-process CLI leg (typed `unavailable.engine.persistence` contention, lock release on SIGKILL). **Probing the kill leg found issue #2618** (first-session SIGKILL bricks the store; regression parked on the issue). loom/shuttle: rejected for V1, recorded in the deferred register |
| 2.5 | **Inference testkit (M7F)** | Fake `Generator`/`Embedder`/`Reranker` providers behind the (currently empty) `testkit` feature; the 18-case deterministic harness; unblocks the executor deterministic inference lane. Also: download failure-path unit tests (~13 missing cases), runtime cache lifecycle unit tests (~21 missing). **Implemented (2026-07-16)** — `crates/inference/src/testkit.rs`: `FakeInferenceEngine` (deterministic generation/embeddings/ranking, scripted failures phrased to classify through the real `code()` rules, partial-item failures, reported-not-slept latency, redaction) with the full 18-case harness matrix asserting stable codes/classes/retryability; plus 6 offline download failure-path tests (SHA-256 mismatch typed+temp-removed, stream errors retryable, lock guard RAII) and a per-PR CI lane for the feature-gated modules. Remaining, recorded: runtime-cache lifecycle tests and the executor deterministic lane both need a testkit variant inside the `GenerationEngine` dispatch enum (runtime engine injection) — the natural next increment |
| 2.6 | **Small zero-coverage surfaces** | `crates/wasm` (wasm-bindgen-test smoke over the serialized-command adapter), `stratadb` facade (public-surface conformance beyond the single round-trip test), hub per-endpoint suite. **Implemented (2026-07-16)** — `crates/wasm/tests/session.rs`: four wasm-bindgen tests *executed on wasm32-unknown-unknown* (KV round trip through the serialized surface, malformed-JSON/unknown-type throws vs executed-failure error envelopes, branch-scoped isolation incl. reserved-name refusal); the `wasm` CI job upgraded from compile-check to test execution (`wasm-bindgen-cli` pinned to the locked `wasm-bindgen`). `crates/stratadb/tests/facade.rs`: all six data services proven reachable and functional through the crates.io re-export surface, plus stable error code/class conformance (`not_found.engine.branch`). CLI `remote` rendering (never-cloned → typed null-origin envelope, exit 0) landed in `cli_execution.rs`. Scope note: the hub *crate* was never zero-coverage (35 tests incl. real HTTP transport in `crates/hub/tests/`); the genuine gap was CLI-level rendering. Remaining headroom: CLI `clone` end-to-end over HTTP (needs the `real_transport.rs` bundle-serving pattern lifted to a CLI test) |
| 2.7 | **Multi-branch orphaned-delta recovery** | Currently guarded (checkpoint defers), latent high-severity. Decide: fix per-branch recovery in-program or keep guard + add adversarial regression coverage. **Implemented (2026-07-16)** — decision: **keep guard + adversarial coverage** (the per-branch fix is a frozen-format database-manifest change + two-phase recovery rework + net-new multi-branch crash infrastructure, coordinated with post-V1 multi-branch durable maintenance — see `implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md`, which had already settled guard-now/fix-later). The adversarial pass over the guard's "unreachable" claim immediately **found and fixed issue #2624** (high, silent data loss): the guard was enforced at the synchronous path and background *claim* time but not on the close drain — a checkpoint task stranded in the active list (detached-on-timeout or panicked worker) re-ran through the close runner's seeded-only collector with no guard re-check, losing a non-seeded branch's unflushed rows on a *clean close+reopen* and recording exactly the snapshot the guard exists to prevent. Fixed in #2625 (close-drained checkpoints defer whenever non-seeded branches exist; regression `close_drained_checkpoint_does_not_bypass_the_multi_branch_guard`, written first, failed with `base_present=true, delta_present=false`). Boundary pinned: `deleting_the_flushed_non_seeded_branch_releases_the_checkpoint_guard` (durable tombstone releases the guard, checkpoint completes, crash dropping the leftover manifest recovers cleanly, no resurrection). The per-branch fix that lifts the guard + close defer together is in the deferred register |
| 2.8 | **Close-time flush surfaces (#2612)** | Adopt `decide_flush_rotation` (or a close-specific flush-backlog-then-rotate variant) at the durable/cache close runners and the cache background step; tests stage a saturated store and assert graceful close drains fully. **Resolved as not-a-bug (2026-07-16)**: audit-fix verification showed the close runners' refusal lines are production-unreachable (every `DrainBeforeClose` producer is test code) and close-at-saturation is empirically sound — acked commits ride the WAL and reopen recovers them (pinned by `saturated_store_closes_gracefully_and_reopens_complete`). The cache step's refusal is typed and retried. Residual code-shape harmonization moved to the deferred register |

Phase 2 exit gate: no known gap without either a merged test lane or an
entry in the deferred register. **Met 2026-07-16** — 2.1-2.7 implemented
(2.8 resolved as not-a-bug); every remaining absence is a deferred-register
row. Phase 2 surfaced and fixed two shipped bugs (#2618 creation-durability
brick, #2624 close-drain guard bypass) and falsified one (#2612). Phase 3
opens with the layer-by-layer coverage plans.

## Phase 3 — Layer-by-layer coverage plans

Plan ACCEPTED 2026-07-17: `v1-test-coverage-phase3-plan.md` — gap analysis
(five per-layer deep-dives + product-only coverage measurement + error-code
assertion cross-cut), the three-tier tracking methodology (surface guards /
product-only per-crate ratchets / this ledger), 16 slices TCP3.0-TCP3.15 in
dependency order (tracking machinery → core → storage → engine → executor →
CLI → edge crates), and six resolved design decisions. Headline findings:
202 error codes in product sources with 68 never asserted; storage inner
error enums (~85 variants) unassertable through the boundary; executor
error envelopes never replay-tested; CLI weakest at 47.3% product-line
coverage. Slice status tracks in the table below as slices merge.

| # | Slice | Status |
|---|---|---|
| 3.0 | Tracking machinery (product-only ratchets, workspace error-code guard) | **Implemented (2026-07-17)** — `crates/storage/tests/error_code_assertion_guard.rs` (workspace scan: every 3-part code asserted or allowlisted with its owning slice; 65 seeded entries; shrink-only both ways — entries die when their code gains a test or stops existing); nightly coverage gate replaced by `scripts/coverage_floors.py` (per-crate product-only line floors, testkit/test infra excluded, ratchet-up hints; wasm excluded by design — its tests run on wasm32); `cli-command-coverage.md` marked superseded, deleted by TCP3.10's clap-tree guard |
| 3.1 | Core (goldens, adversarial deserialize, doc-parity guard) | **Implemented (2026-07-17)** — `wire_goldens.rs` pins the durable bytes of all three atoms in both directions (JSON + bincode, canonical/boundary/asymmetric vectors, truncation refusals); **falsification-verified**: a symmetric byte-order change (encode+decode reversed together) silently corrupts the durable encoding while all 17 existing round-trip tests pass, and the goldens catch it — the concrete answer to "round-trips are not goldens". `adversarial_decode.rs` covers the crate's only hand-written deserializer (malformed/multibyte/non-string text, wrong-length `visit_bytes`, and the `visit_seq` arm bincode never reaches, via a purpose-built non-human-readable deserializer). `hash_and_boundary.rs` asserts Eq/Hash consistency + map-key behavior, the `Duration`-overflow saturation branch the proptest strategy structurally cannot reach, and unit-truncation direction. `doc_parity_guard.rs` (Tier-1) ties `core-architecture.md`'s M1 boundary tables to `public_api.txt` in both directions (documented-but-absent, and explicitly-rejected-but-leaked) plus the `BranchId`-has-no-`Default` invariant; falsification-verified with a phantom doc item |
| 3.2a | Storage inner-error assertability — boundary plumbing + branch (#2632) | **Implemented (2026-07-17)** — `StorageApiError::LowerLayer` gained `inner_code`; `code()` now returns the layer's own code (8 distinct) instead of one `internal.storage_api.lower_layer` constant for all 85 inner variants; `inner_code()` carries the specific inner failure *without* reclassifying (code/class agreement preserved). `BranchRuntimeError::code()` — which already existed behind a `dead_code` allow predicting "downstream crates will consume it" — is now consumed at the boundary; its two 2-part codes corrected to 3-part. **Also fixed a blind spot in TCP3.0's own guard**: it tracked 9 product areas but storage names its areas by layer (`lifecycle`/`storage_api`/`branch`), so 106 storage codes were invisible; areas extended, 58 unasserted codes seeded with owning slices |
| 3.2b | Storage inner-error assertability — `CommitRuntimeError::code()` (#2632) | **Implemented (2026-07-17)** — `code()` on all 30 `CommitRuntimeError` variants + 4 `CommitLowerLayer` sub-layers, classes derived from how `commit_error` actually maps each variant (so the code never contradicts the class the caller sees), wired through all three boundary arms (timeline, StorageBudget passthrough, catch-all). Exhaustive match with no catch-all — **falsification-verified**: adding a variant without a code arm is a hard `E0004` compile error. A 30-entry variant/code table pins every code literally and proves no two share one; the workspace guard then forced its own allowlist to shrink (`internal.storage_api.commit` asserted ahead of TCP3.3) |
| 3.2c | Storage inner-error assertability — table + lifecycle hop + reachability (#2632 CLOSED) | **Implemented (2026-07-17)** — `TableRuntimeError::code()` (11 variants, exhaustive); `LifecycleError::code()` (which already existed) wired through the API's three lifecycle `LowerLayer` arms (catch-all, publication-failed pair, commit-downcast fallback) so lifecycle failures carry their layer code not one constant. Reachability closed by construct-every-variant tests for all four inner enums (commit 30, branch 13 + compaction 25, table 11) that pin each code and prove uniqueness — the workspace guard then required 34 branch + 11 table codes move from allowlist to asserted (allowlist 65→23, the residual 23 owned by TCP3.3 L9 paths). All three exhaustiveness levers falsification-verified (E0004 on a new variant). **#2632 CLOSED** across 3.2a/b/c |
| 3.3a | Storage L2 codec fuzz + layer-fuzz presence guard (#2632 residual) | **Implemented (2026-07-17)** — two fuzz targets (`layout_object_name`: arbitrary name text through every `classify_*`, no-panic; `layout_id_roundtrip`: canonical WAL/snapshot names classify back to their exact u64 id) closing the one decoder layer that had no fuzzer; testkit entry points + behavioral units; `layer_fuzz_presence_guard.rs` (Tier-1) asserts every decoder layer keeps a target — **falsification-verified**: removing the L2 targets fails the guard. 100K-run object-name + 50K-run id fuzz clean locally |
| 3.3b | Storage L1 recovery-time read/list/metadata fault sweep | **Implemented (2026-07-17)** — `recovery_read_faults` testkit harness: populates a durable store, traces the ListPrefix/ReadObject/ObjectMetadata positions the *open* path touches, fails each under Unavailable/Interrupted, and asserts the safety invariant — **a recovery-scan fault never yields a silently-`Healthy` open** (it fails loudly or degrades). Result: all 24 swept positions failed the open with a typed error, zero silently-healthy, and every fault fired (no vacuous pass). Integration test + `#[ignore]` 64-seed soak wired into nightly next to the fault-sweep soak. The write-path sweep's "read/list/meta exercised incidentally" comment corrected to point here. TCP3.3c (L9 negatives) remains |
| 3.3c | Storage L9 negative paths + method-presence guard | **Implemented (2026-07-17)** — L9 negative-path tests asserting **codes** (not just classes) for the methods the deep-dive found bare: `scan_immutable_sources` (missing branch / closed), timeline lookups (missing branch, before-retained-history), `drain_maintenance`/`run_next_maintenance` (closed). `every_storage_api_code_is_pinned_as_a_literal` pins all 23 `StorageApiError` codes + all 8 lower-layer codes as literals (the contract test only checked class prefixes / method calls, invisible to the workspace guard) — **drained the 6 residual `storage_api` allowlist codes**. `l9_method_presence_guard.rs` (Tier-1): every public `StorageRuntime` method is referenced by a test (empty allowlist — all 33 genuinely covered), **falsification-verified**. Residual allowlist now 17, all lifecycle codes → TCP3.3d |
| 3.3d | Storage lifecycle inner-error construct-all — **storage allowlist → 0** | **Implemented (2026-07-17)** — construct-every-variant test for `LifecycleError` (47 named variants + 7 `LowerLayer` sub-layers = 54 codes), pinning each literal and proving uniqueness; exhaustive `code()` match falsification-verified (E0004 on a new variant). **Drained the final 17 lifecycle allowlist codes → storage error-code allowlist is now 0** (every storage code asserted). Surfaced **issue #2646**: 8 lifecycle codes use `unknown.*`/`deadline_exceeded.*` class prefixes that are not declared error classes (they reclassify correctly at the boundary); tracked in the guard's class list pending #2646's rename (TCP3.5). Closes the storage strand of Phase 3 except 3.4 (concurrency, #2636) |
| 3.4a | Storage commit lock-order enforcement (#2636) | **Implemented (2026-07-17)** — `commit/lock_order.rs`: a debug-only lock-rank tracker enforcing the commit runtime's documented discipline (the branch-admission guard mutex and unresolved-durable gate mutex are leaves, mutually exclusive, never nested). Wired at every commit mutex-acquisition site; compiles to a zero-cost no-op in release (verified). The tracker's own tests drive the assertion (should_panic on nesting); a real-guard-API test proves no false-positive on legitimate concurrent use. **The assertion held across the entire storage suite with zero violations** — the two mutexes are genuinely never nested. #2636's stale contract anchor (the pre-refactor src/txn/lock_ordering.rs path, no backticks so the charter guard does not treat it as a live anchor) repointed at `commit/guard.rs` + `commit/lock_order.rs` |
| 3.4b | Storage threaded L6 COW races (#23/#24) | **Implemented (2026-07-17)** — two threaded tests in `off_lock_concurrency.rs`: `fork_captures_a_consistent_snapshot_while_a_writer_races` (#23 — a fork taken while a writer streams atomic batches into the source must capture one consistent COW snapshot; `read_checked` panics on any torn read, and the test asserts real seeded state was observed, not a vacuous pass), and `pinned_reads_never_tear_across_background_flush_and_compaction` (#24 — durable-background runtime with tiny WAL thresholds forces flush+compaction concurrently with the reader/writer race; readers never tear across a rewrite boundary). Distinct from the existing fork-under-concurrent-*reads* test, which holds the source static. **Closes the storage strand of Phase 3** (except the DST extension, deferred per D3). Also fixed a charter-guard regression from 3.4a (a backticked stale path treated as a live anchor) |
| 3.5a | Error-contract reconciliation + class-parity guard (#2633/#2634/#2646) | **Implemented (2026-07-17)** — the contract doc's 81-row 2-part "registry" was a design-phase starter set that never matched code (spot-checked: `not_found.key`, `io.backend_read`, `conflict.branch_merge` exist in zero source files); the real registry is the authored/guarded `crates/executor/idl/v1/errors.yaml` (99 3-part codes). Rather than hand-rewrite fiction into 3-part fiction (which re-drifts), replaced the starter tables with a pointer to `errors.yaml` and fixed the format examples (#2633); the 4 dead codes (#2634: branch_merge/revert/cherry_pick, retention_window) were only in that fiction, removed with it. #2646: renamed the 8 `unknown.*`/`deadline_exceeded.*` lifecycle codes to declared classes (7 → `ambiguous_commit.*`, close-timeout → `failed_precondition.lifecycle.close_timeout`) across code + tests; added `data_loss` to the doc's class table (a real emitted class it lacked); `strata.engine.*` confirmed as record magics, not errors. New `error_contract_class_parity_guard.rs` (Tier-1): the doc's Error Class table must equal the workspace guard's tracked `CLASSES` — falsification-verified. Slices 3.5b/c/d (graph/vector+json/event+space refusal batches, ~38 codes) remain |
| 3.5b | Engine graph refusal batch (#2651) | **Implemented (2026-07-17)** — `engine_graph_refusals.rs` asserts the 9 reachable graph validation refusals by literal code (name/name_reserved/node_id/edge_type/edge_type_reserved/binding/properties_too_large/property_name/type_hint) via public constructors, no DB setup. The existing graph suite asserted these by *class*, invisible to the workspace guard. Investigation found the other 7 of the deep-dive's 16 are **unreachable** — 6 defensive `serde_json::to_vec` encode-error arms on plain structs (their decode counterparts are the `data_loss.engine.graph_*` codes for TCP3.15) and 1 short-circuited `graph_batch` invariant (the public empty batch succeeds first). Filed #2651; kept allowlisted with precise per-code reasons |
| 3.5c | Engine vector + json refusal batches (#2651) | **Implemented (2026-07-17)** — `engine_json_vector_refusals.rs` asserts the 3 genuinely reachable user-facing refusals by literal code: `json_index_name` (empty), `vector_metadata_too_large` (>16 MiB), `json_batch_duplicate_document` (dup ids via `batch_delete`). Investigation found most of the deep-dive's ~15 vector/json codes are **not user refusals**: 6 defensive `serde_json::to_vec` encode arms (json_value/json_document/json_index/vector_metadata/vector_record/vector_artifact + vector_index_manifest), 2 short-circuited empty-batch invariants (json_batch/vector_batch), and 3 reopen-time incompatible_layout/IO faults (→ TCP3.15). Extended #2651; each allowlisted with a precise disposition. Reachable-refusal tally across 3.5b+c: 12 codes; ~14 defensive-unreachable — a real chunk of the engine's `invalid_argument.*` surface no client can receive |
| 3.5d | Engine event + space refusal batches — **closes TCP3.5** | **Implemented (2026-07-17)** — `engine_event_space_refusals.rs` asserts the 3 reachable refusals by literal code: `event_payload_too_large` (>16 MiB), `space_delete_default` (delete the default space), `space_delete_too_large` (>10k rows via 3 sub-cap batches — reachable and fast). Defensive/unreachable: `event_batch` (short-circuited empty batch), `event_metadata`/`event_record` (serde encode arms, #2651), `space_catalog` (u16 overflow needing >65535 spaces). **TCP3.5 complete**: 15 reachable engine refusal codes asserted across 3.5b/c/d; the 35 remaining engine allowlist entries are all legitimately deferred — 11 `data_loss.*` + reopen-faults → TCP3.15, ~13 defensive-unreachable → #2651, 1 hub code → TCP3.13. No reachable refusal remains unasserted |
| 3.6 | Engine conformance depth (fault dimension, temporal oracle generalization) | **Done (2026-07-18)** — 3.6a (fault dimension) + 3.6b (temporal oracle, i+ii) shipped; 3.6c closed as N/A (finding below) |
| 3.6a | Fault dimension in `capability_conformance` | **Implemented (2026-07-18)** — the shared 5-capability conformance suite tested write/read/fork/space/closed-runtime but **injected no faults**, though the seam existed (`StorageFaultKind` + `inject_commit/read/scan_fault_for_test`, used only in the KV-only `persistence_faults.rs`). Extended the `CapabilityFixture` trait with `point_read` (storage read path) and `scan` (scan path), then added 3 testkit-gated fault tests × 5 capabilities = **15 cases**: a commit fault fails the write and persists nothing; a read fault on the point read and a scan fault on the scan each surface the mapped retryable `unavailable`/`resource_exhausted` status. Proves json/event/vector/graph route through the identical `map_storage_error` path KV pins in detail. Read/scan routing is verified distinct by construction (a mis-routed op leaves the armed fault unfired → the test fails); event reads by sequence (`get`) hit the read path, `get_by_type` the scan path. 45 conformance cases total (was 30) |
| 3.6b | Temporal timeline oracle generalization (json/event/graph) | **Done (2026-07-18)** — split by temporal shape: keyed-mutable (KV/JSON/graph, 3.6b-i) share the MVCC `read_row`+`ReadSelector` path; event is append-only (3.6b-ii) |
| 3.6b-i | Keyed-mutable temporal oracle (KV/JSON/graph) | **Implemented (2026-07-18)** — the timeline property oracle covered KV only. Refactored it into a generic `TemporalFixture`-parameterized harness and instantiated it for KV, JSON documents, and graph nodes (all three keyed, mutable, read through the same MVCC selector; each round-trips a seed byte — JSON via document value, graph via node properties). Covers latest / as-of-version / as-of-timestamp against a per-key reference timeline, the out-of-range `history_unavailable.engine.persistence_history` boundary diagnostics (confirmed capability-agnostic), and the `fork_at_version == source-as-of` parity property — **6 proptest properties** (was 2, KV-only). **Falsification-verified**: breaking graph's as-of-version read (returning latest) fails the oracle; reverting → green |
| 3.6b-ii | Event append-only temporal oracle | **Implemented (2026-07-18)** — events are append-only (immutable, monotonic, no tombstones), so they don't fit the keyed-mutable oracle. New `event_timeline_model.rs` proptest oracle: append N events, record each append's commit (version, timestamp), then assert `len()` == N, `len_at(ts)` == count committed by `ts` (lenient: 0 before first commit), `get(seq)` present iff `seq < N`, and `get_at(seq, ts)` visible iff that event's append committed by `ts` — MVCC visibility on the branch commit timeline, not occurrence time. Out-of-range `get_at` (EPOCH/MAX) → `history_unavailable.engine.persistence_history`, matching the KV boundary contract. Plus a fork property: forking at event `i`'s append version yields a child with exactly `i+1` events. **Falsification-verified**: flipping the visibility boundary (`<=`→`<`) fails the oracle. Closes 3.6b |
| 3.6c | Cross-branch rejection for vector/json | **Closed N/A (2026-07-18)** — investigation found the plan's premise does not hold: **vector and JSON have no user-facing cross-branch reference surface**. Graph is unique — a `GraphEntityBinding` target carries an optional `BranchName`, and naming another branch is rejected with `unsupported.engine.graph_binding_cross_branch` (the only cross-branch rejection code in the registry; tested by `engine_graph::exercise_graph_cross_branch_binding_rejection`). Vector metadata and JSON document values are opaque payloads with no branch-typed field, so there is nothing analogous to reject. The real cross-branch behavior these capabilities have — fork isolation — is already covered for all 5 capabilities by `capability_conformance::fork_inherits_and_isolates`. The vector `source_branch_id` in the index manifest is internal derived-state provenance (COW/GC fork-version tracking), not a user reference; its `validate_identity` checks only non-empty IDs + dimension. No guard added (a cross-branch reference into vector/json is on no roadmap; rule 18 rejects cross-branch refs generally). No new tests warranted |
| 3.7 | Engine contract truth-ups (rule 20 absence guard, dead-code deletion) | **Implemented (2026-07-18)** — closes #2634/#2635/#2638. Rule 20 was the only named branch invariant with no test of any kind: it read "branch merge is strict refusal on divergent history (V1)" but no merge/revert/cherry-pick entrypoint exists in engine, and the charter's "guards assert absence" claim had no guard. (a) Amended CLAUDE.md rule 20 to state merge/revert/cherry-pick are **absent** in V1 (no entrypoint; strict-refusal surface is post-V1). (b) Added `crates/engine/tests/branch_merge_absence.rs` — a source-scan guard over 10 distinctive branch-op tokens (`cherry_pick`, `merge_base`, `branch_merge`, `three_way`, …), all zero in engine src today, chosen not to collide with the implemented vector-candidate merge or document-level JSON merge (rule 21); it fails the day a merge surface appears, forcing the implementer to amend rule 20 and add its strict-refusal tests. **Falsification-verified** (injected `cherry_pick` → fails). (c) Deleted the never-emitted `conflict.branch_merge/_cherry_pick/_revert` codes from `engine/error-and-diagnostics-contract.md` (table row + required-test list + section heading); `retention_window` + these codes were already gone from the V1 error contract (3.5a). #2638 resolved as scope decision (1): event retention is not V1 (user-confirmed), row already deleted |
| 3.8a | Executor error-case fixture format + replay path | **Implemented (2026-07-17)** — the IDL fixture-replay guard treated any execution error as fatal (`request fixture failed to execute`), so no failing command's `ErrorStatus` envelope was ever pinned — a mis-mapped class/retry shipped silently to SDKs. Added `error_cases: Vec<ErrorFixtureCase>` to the fixture schema (option A: parallel to `cases`) and `verify_error_case`: replays the request, requires an `Err`, and diffs the envelope's **structured fields** (code/class/retry_policy/retryable/commit_outcome — not the churning prose/id fields) against a pinned JSON fixture, `--update`-blessable like the success path. Runs in CI via the existing `verify-fixtures` job. Seeded 3 cases across 3 classes (kv unretained-version → history_unavailable, vector missing-collection → not_found, kv duplicate-key → invalid_argument); **falsification-verified** (a corrupted pinned class fails the replay). Finding for 3.8b: **no command declares `errors[]` in YAML** (all empty; errors.yaml is a flat registry), so the coverage guard cannot drive from `command.errors[]` as planned — 3.8b needs a different driver |
| 3.8b | Executor error-case coverage + fixtures per family | **Implemented (2026-07-17)** — corrected the 3.8a premise: each command *does* carry a resolved `errors[]` (defaults + family + per-command `errors+`/`errors-` layering); the union across all 125 commands is **116 codes**. Added the replay-coverage ratchet `enforce_error_replay_coverage` (runs in the `verify-fixtures` CI lane): **(A)** every declared code must be pinned by an error-case replay fixture or listed in the new shrink-only `unreplayed-error-codes.yaml`; **(B)** any code a command *replays* must be *declared* by that command (proof-of-reachability ⇒ SDK-facing docs must list it). Granularity is **per code, not per (command×code)** — the `ErrorStatus` envelope is built centrally from the code, so one replay proves the mapping everywhere. (B) immediately caught 3.8a's `kv.get` replaying `history_unavailable.engine.persistence_history` without declaring it (fixed via `errors+`). Authored 8 new error cases across kv/branch/event/graph×2/space/json/vector (classes invalid_argument + already_exists), draining the allowlist 113→105; **falsification-verified** the ratchet 3 ways (drop-covered, add-replayed, add-undeclared all bite). Also fixed a latent 3.8a gap: `FixtureRefs` gained `error_cases` but the runtime `CliFixtureRefs` loader (`deny_unknown_fields`) did not, so any CLI-index regen produced embedded metadata the runtime rejected — mirrored the field. Remaining 105 need harnesses beyond this lane (closed runtime, live provider, Arrow builds, configured hub) or belong to TCP3.15 (corruption/data_loss) |
| 3.8c | Executor per-command test-existence guard + replay_skip ratchet | **Implemented (2026-07-17)** — the fixture-behavior guard replays every command's primary fixture unless it sets `replay_skip`, but nothing stopped a new `replay_skip` from silently removing a command from behavior coverage (15 of 125 commands skip today: hub-network / arrow-filesystem / inference-model / event-wall-clock). Added `enforce_replay_skip_ratchet` (runs in `resolve_index`, so `check` gates it) backed by the new shrink-only `replay-skipped-commands.yaml`: every command that sets `replay_skip` must be listed, a new skip fails the build unless justified and listed, and a command that becomes replayable must be dropped. Golden-or-replay is now guaranteed per command (the `response` fixture is required + schema-validated even for skipped commands; execution coverage is fenced by the ratchet). Logic extracted to a pure `enforce_replay_skip_lists` helper with 4 committed regression tests (accept, new-unlisted-skip, shrink-only, unknown-id) mirroring the existing exhaustiveness tests; also **falsification-verified** live 3 ways. Closes TCP3.8 |
| 3.9 | Executor hermetic inference lane + branch/session behavior | **Done (2026-07-18)** — facade/vector coverage (a), branch+session behavior (b), hermetic inference lane (c: injection seam + fake service + converted all 8 inference replay-skips) all shipped |
| 3.9a | `facade/vector.rs` coverage | **Implemented (2026-07-18)** — the 19 vector convenience methods on `Executor` (`vector_create_collection`/`upsert`/`query`/… — thin wrappers that fill `branch`/`space` with `None` and forward to `execute`) had 0% coverage because tests built `Command`s directly. New `vector_facade_behavior.rs`: one lifecycle runs on two lockstep cache executors — one via the facade, one via explicit commands — asserting equal `Output` (derives `PartialEq`) at every step, so any argument-transcription bug (swapped key/collection, wrong `as_of` default) fails. **Falsification-verified**: swapping key↔collection in `vector_get` fails the test |
| 3.9b | Executor branch + session behavior | **Implemented (2026-07-18)** — the branch commands (create/list/get/fork-current/fork-at-version/fork-at-timestamp/delete) had no focused executor test; they only appeared as setup in the data suites. New `branch_behavior.rs` (5 tests): full create→list→get→delete lifecycle with `not_found.engine.branch` after delete; `fork_current` inherit-and-isolate (both directions + later-parent-write invisibility); `fork_at_version`/`fork_at_timestamp` reading the source as-of; branch-facade equivalence for the 5 convenience methods. New `session_behavior.rs` (4 tests): omitted `branch`/`space` (`None`) resolves identically to naming the defaults (both directions + output equality), and a missing-branch operation is rejected with `not_found.engine.branch` on read and write. Concrete-value + specific-error-code assertions |
| 3.9c | Hermetic inference lane (executor-injectable fake service + convert 8 replay_skips) | **Done (2026-07-18)** — the executor calls 11 runtime-level methods, so the compute-only `InferenceEngine` trait couldn't back it; solved with a runtime-level `InferenceService` trait + fake (3.9c-i), then converted all 8 inference replay-skips to real replays (3.9c-ii) |
| 3.9c-i | Inference injection seam + fake service + hermetic executor test | **Implemented (2026-07-18)** — new runtime-level `InferenceService` trait in `strata_inference` (11 methods, `impl` for `InferenceRuntime` delegating to inherent methods); `FakeInferenceService` in `inference/testkit.rs` (deterministic model list / chat-echo / pseudo-embeddings / overlap-rank / byte tokenizer / synthetic pull, no network or model files). Executor field `inference: Box<dyn InferenceService>`; `with_inference_runtime(impl InferenceService + 'static)` boxes internally, so existing callers passing `InferenceRuntime` compile unchanged (zero blast radius). Cargo `testkit` forwards to `strata-inference?/testkit`. New `inference_hermetic_behavior.rs` (9 tests) drives all 11 inference commands through the fake — the first hermetic executor-level inference coverage (the other lane needs live keys+network) |
| 3.9c-ii | Convert the 8 inference fixture replay-skips | **Implemented (2026-07-18)** — the `verify-fixtures` harness now injects `FakeInferenceService` for inference-family commands (via `open_executor`), and skips them when the `testkit` feature is off (so the non-testkit lane stays green). Deleted the 8 `replay_skip`s from `inference.yaml` + their `replay-skipped-commands.yaml` entries (atomically, per the ratchet), and re-blessed the inference response fixtures to the fake's deterministic output. The CI IDL-gate invocations gain `testkit` so the coverage runs. The `command_contract` `text.json` golden is shared, so the `detokenize` request now uses byte ids that round-trip to `"hello"`, keeping that golden intact. All 8 inference commands now replay against the fake instead of being schema-checked only |
| 3.10 | CLI renderers + verb-enumeration guard + config write path | **Done (2026-07-18)** — render helper coverage (a), config write-path (c), verb+render guards (b) all shipped |
| 3.10a | CLI render helper coverage | **Implemented (2026-07-18)** — `render.rs` dispatched result-type tags to ~18 `print_*` helpers that `println!`'d directly, so only the JSON-transform half was tested. Behavior-preserving refactor: the human/raw renderers now write into a `&mut String` buffer via a `line!` macro (`writeln!`, byte-identical to `println!`) and `render_value` prints the buffer once; the Json/Pretty/error paths are untouched. 45 print sites converted; **18 new unit tests** assert the exact buffer for every result-type path (event_count, kv, json, vector_matches, all inference renders, `(nil)`/`(empty)`/`(none)` branches). Byte-exactness verified: old↔new format strings match 1:1, and the `cli_execution` integration test (drives the real binary, exact-stdout asserts) stays green. 63 lib + 17 integration tests, clippy + fmt clean |
| 3.10c | CLI config write-path coverage | **Implemented (2026-07-18)** — the `strata config set/unset/path/show` user-config write path (`hub.url`, `<provider>.api_key`, which run before any database opens) had no test. New `config_behavior.rs` (6 tests) drives the real binary against a hermetic `HOME` (temp dir, `XDG_CONFIG_HOME`/`STRATA_HUB_URL`/`STRATA_DB` stripped): hub.url set→show→unset roundtrip (source flips from the config file to `built-in default`), env override (`STRATA_HUB_URL` wins, source reports the env var), the config file is written **0600** (unix), a provider api_key is **redacted** and the raw key is never echoed, `path` reports the config file, and an unknown key exits 2 with `unknown config key`. Concrete-value assertions |
| 3.10b | CLI verb-enumeration + render result-type guards | **Implemented (2026-07-18)** — two executable-inventory guards that lock in the CLI surface and retire the stale hand-maintained `cli-command-coverage.md` (deleted). (1) A clap-verb enumeration guard (`options.rs`) walks `Cli::command()`'s leaf verbs and asserts they equal a maintained 136-verb `EXPECTED_VERBS` inventory — a verb added/removed without updating the list fails CI, so the surface can't silently drift. (2) A render result-type guard (`render.rs`) source-scans the human/raw dispatch `match` arms and asserts they equal a maintained 21-tag `RENDERED_TAGS` inventory (**falsification-verified**: a fake arm is caught), so 3.10a's per-tag render coverage can't regress. Closes 3.10 |
| 3.11 | CLI family coverage (corpus port) | **Done (2026-07-18)** — ported the shell corpus (`scripts/cli-corpus`) to real-binary Rust tests: space+json (a), event+graph (b), arrow+cross-cutting (c). Every CLI command family now has real-binary integration coverage |
| 3.11a | CLI json + space family coverage | **Implemented (2026-07-18)** — json and space had zero CLI integration tests. New `json_space_behavior.rs` (7 tests) drives the real binary against a temp durable db, porting the corpus workflows and pinning the behavior fixes the corpus first surfaced: a stored JSON `null` is a live document (found, inner value null) distinct from a missing one; json `list` paginates by `--cursor`/`--prefix` and includes the stored-null document; json index create/list/drop; json history is newest-first; json count; space create/list/exists, `--space` isolation of the same key, and space delete |
| 3.11b | CLI event + graph family coverage | **Implemented (2026-07-18)** — event and graph had zero CLI integration tests. New `event_graph_behavior.rs` (9 tests) drives the real binary against a temp durable db, porting the corpus workflows (and correcting the stale corpus where the CLI moved on — e.g. `event count`, not `event len`). Events: append assigns monotonic sequences; `list` paginates by sequence-cursor; `by-type` filter; reverse `range`; **`verify-chain` reports `valid` (not the old `is_valid`)**; `count` is per-branch isolated (fork → child count diverges). Graph: typed create/add-node/add-edge results; `list-nodes` cursor pagination; **`neighbors` filter by `--direction` (outgoing/incoming) and `--edge-type`**; and a fork's edits (remove-edge + add-node/edge) leave the parent unchanged while the child diverges |
| 3.11c | CLI arrow + cross-cutting coverage | **Implemented (2026-07-18)** — arrow import/export, pipe/raw mode, `command print`/`run`, and structured error envelopes had zero CLI integration tests. New `arrow_pipe_behavior.rs` (8 tests) + `inference_verb_behavior.rs` (4 tests) drive the real binary. Arrow: kv export→CSV import→read round-trip; **graph export splits the requested stem into concrete `_nodes`/`_edges` files, never writing or reporting the stem itself**; unknown `--primitive` refused by clap. Cross-cutting: `command print` echoes / `command run` executes (ping→pong); pipe mode runs a newline-delimited command stream skipping `#` comments; cache mode is process-local; **structured error envelopes go to stderr (stdout stays clean) with `class`/`code`/`retry_policy: never`/`retryable: false`** for missing-branch (`not_found.`) and invalid-vector-dim (`invalid_argument.`). Inference: the four non-model verbs (`models list`, `models local`, `cache-status`, `capability`) are pure functions of the static catalog/provider facts — pinned hermetically under a temp `HOME` so no downloaded model leaks in |
| 3.12 | Inference deterministic residuals (request goldens, wire mapping) | **Implemented (2026-07-18)** — the three cloud providers already have dense field-by-field wire tests, but no test pinned a **whole request body**, so whole-shape drift (an extra/dropped/renamed key elsewhere in the body) was invisible, and the cross-provider silent-drop contract was covered asymmetrically. New in-crate module `crates/inference/src/provider/wire_goldens.rs` (10 tests) pins the **entire JSON** each provider emits for one canonical chat/generate/embed request (parsed-`Value` goldens: key-order-insensitive, but extra/missing/changed keys fail), across openai/anthropic/google. Gotchas encoded: floats restricted to exactly-f32-representable values (0.5/0.25/0.75) so the f32→f64 JSON widening stays lossless; chat sets `temperature` alone because Anthropic rejects temperature+top_p. Also pins the **silent-drop parity** (OpenAI serializes `logit_bias`/penalties/`seed`; Anthropic and Google must drop them) and fills the symmetric gap where OpenAI's `parse_chat_response_json` had no malformed-JSON test (asserts on the `Provider` error variant, not display text). Deferred residuals (documented, low-value): `map_http_error` string branches (all collapse to one `Provider` class — display-text-only) and `parse_tool_calls`/`parse_logprobs` malformed inputs |
| 3.13 | Hub transport fault injection | **Implemented (2026-07-18)** — `clone_flow.rs` fault-tested only `get_object` (missing object); the earlier §3.6 clone steps and the integrity boundary were unguarded. New `crates/hub/tests/clone_faults.rs` (5 tests): a configurable fault transport that fails at any step and **counts every call**, proving each fault (a) surfaces the right `CloneError`, (b) **short-circuits** — a `default_branch`/`resolve_ref`/`get_manifest` failure never reaches the next wire call — and (c) leaves **no destination state**. Adds two paths the missing-object test can't reach: **corrupted object bytes** (valid hash, tampered body → import's per-object hash check rejects with `ObjectHashMismatch`, no dest) and a **malformed engine requirement** (non-semver → refused before any download). Closed the two error codes the charter deferred here (shrink-only guard entries removed): `failed_precondition.executor.hub_clone` — asserted end-to-end in `hub_clone_behavior.rs` by serving a manifest whose engine requirement this build can't satisfy (compat gate → executor envelope); and `not_found.engine.database` — asserted in `remote_tracking.rs` by reading a remote ref from a non-database path. **Verified** the executor's `clone_error` `_ =>` arm is not a class/code bug: `public_class_for_executor` derives the public class from the code prefix (`failed_precondition.`), overriding the passed `Unavailable` |
| 3.14 | wasm + stratadb residuals | **Implemented (2026-07-18)** — 2.6 gave both facades a first test; this closes the residuals. **wasm** (`session.rs`, +4 tests, executed on wasm32 via `wasm-bindgen-test-runner`): the **space isolation axis had zero coverage** (branch was tested, space — the parallel scoping axis exposed via `space()`/`setSpace()` — was not), so a stored key under one space must be invisible under another; plus `setSpace` rejects a reserved name (throws), `close()` then a further command comes back as an **error envelope, not a throw** (a closed handle refuses work), and `engine_version()` reports a non-empty semver. **stratadb** (`facade.rs`, +2 tests): the crate's "branchable, time-traveling" headline was never exercised through the re-exported names — `fork_current` inherits the parent seed then diverges without touching the parent (note `create` makes an *empty* branch per rule 19; `fork_current` is the inheriting fork), and a version pinned by `get_versioned().version()` still reads its old value via `get_at_version` after an overwrite (the `CommitVersion` threads by inference, proving the versioned-read surface is reachably re-exported) |
| 3.15 | Engine corruption injection (data_loss assertion) | **Done (2026-07-18)** — closed the 14 allowlisted corruption/reopen/IO codes across 4 sub-slices (a: infra + KV/JSON, b: graph, c: vector + reopen + IO, d: control-plane + Phase 3 exit gate). Of the 14: **7 asserted through the real read path or a genuine fault** (kv/json + the 4 graph codes via the content-corruption seam; vector `unavailable` via a durable IO fault), **5 asserted at the decoder** (the 4 vector `data_loss`/`failed_precondition` codes, which the runtime swallows into a `"corrupt"` status by design — rule 26 graceful degradation — so the decoder is the only place they surface; plus `control_name`, whose branch-catalog decode runs only at open), and **2 re-classified as genuinely unreachable** (`vector_artifacts` plural, `branch_id`) after verification. The existing `StorageFaultKind` seam only *fails* an op; the seam this phase added (`RowCorruption`) mangles a *successful* read's bytes so the `data_loss.*` decoders run on the real path |
| 3.15a | Corruption infra + KV/JSON | **Implemented (2026-07-18)** — built the content-corruption seam as a sibling to the storage-fault seam: `RowCorruption` (`DropValue` / `SetValue(bytes)`) + a `CorruptionSchedule` in `persistence/fault.rs`, armed via `Database::inject_scan_corruption_for_test` / `inject_read_corruption_for_test`, applied to the just-read rows inside the adapter's `read_row`/`scan_prefix`/`scan_range` (a gated `corrupt_rows` step next to `guard_fault`; a production no-op twin, so shipped builds carry nothing). New `corruption_injection.rs` (2 tests) closes `data_loss.engine.kv_value` (a scanned KV row with its value stripped → the scan decoder rejects it) and `data_loss.engine.json_index` (an index-definition row with a wrong leading format-version byte → `decode_index_definition` rejects it). Both drive the **real read path** (not a leaf-decoder unit test); the exact-code assertions confirm the corruption hit the intended decoder. Two allowlist entries removed |
| 3.15b | Graph corruption | **Implemented (2026-07-18)** — the 4 graph corruption codes, all reached through public graph queries on the real scan path. Three are value-byte decoders (`SetValue` with a wrong leading format-version byte, the row key left valid): `graph_node_record` via `list_nodes`, `graph_edge_record` via `neighbors`, `graph_type_index_record` via `nodes_by_type`. The fourth, `graph_type_index_key`, is a **key** decoder — so the seam gained a `RowCorruption::SetKey(bytes)` variant, and the same `nodes_by_type` scan with a malformed key is rejected by the key decoder *before* the value is read (`SetKey` vs `SetValue` on the one scan cleanly separates the type-index key/record codes). 4 allowlist entries removed |
| 3.15c | Vector corruption + reopen + IO | **Implemented (2026-07-18)** — the 6 vector-family codes, but an Explore audit found the plan's premise only partly holds: **4 of the 6 codes are swallowed by design**. At `service.rs:1853` and `artifact.rs:851/910` the decode errors are discarded (`let Ok(x) = decode(..) else { return Corrupt }`) — vector retrieval degrades gracefully to authoritative source rows on corrupt derived state (rule 26), so the corruption codes never propagate to a caller (they surface only as a `"corrupt"` diagnostic status). The row-corruption seam can mangle the bytes but the error is dropped, so those 4 are only assertable at the decoder. Per a plan-mode decision, closed them with **decoder unit tests** (the detector's whole contract is to return the code): `data_loss.engine.vector_index_manifest` + `failed_precondition.engine.vector_index_manifest` (unsupported `policy_version`) in `manifest.rs`; `data_loss.engine.vector_artifact` + `failed_precondition.engine.vector_artifact` (unsupported format version) in `artifact.rs`. The two `failed_precondition` codes can't be hit by arbitrary bytes — the checksum covers the version field, so a byte-flip trips the `data_loss` checksum path first — so the payloads are rebuilt at the bad version with a matching checksum. The one genuinely-propagating code, `unavailable.engine.vector_artifacts`, is asserted by a **durable IO-fault** integration test (`engine_vector.rs`): seal a flat artifact to disk, replace the artifact directory with a regular file so the next write's `create_dir_all` fails (a not-a-directory error — root-proof, unlike a chmod). The 6th, `data_loss.engine.vector_artifacts` (plural), is genuinely **unreachable** (a `path.parent() == None` guard, but the durable store always joins subdirectories; result also `let _`-discarded) — re-classified in the allowlist as defensive/unreachable rather than asserted. 5 allowlist entries removed, 1 re-worded |
| 3.15d | Control-plane corruption + Phase 3 exit gate — **closes Phase 3** | **Implemented (2026-07-18)** — the two control-plane codes come from the branch-catalog record decoder (`decode_branch_record`), which runs only at open/bootstrap, so the post-open seam can't reach them. `data_loss.engine.control_name` is asserted in `records.rs` by decoding a branch record whose length-prefixed name bytes are corrupted to invalid UTF-8; `data_loss.engine.branch_id` is **genuinely unreachable** (its field always `take`s exactly `BranchId::BYTE_LEN` bytes and `try_from_slice` validates length only) and re-classified. **Exit gate:** the error-code guard's allowlist now holds only legitimately-deferred entries and carries **zero `asserted by TCP3.15` promises**; trued-up the stale forward-references left by completed slices (the graph encode-arm `#2651` entries whose decode counterparts are now asserted; the CLI doctor codes mislabelled "asserted by TCP3.10" that 3.10 never took) to honest deferral reasons |

**Phase 3 closed 2026-07-18.** All 15 slices implemented; the layer-by-layer
coverage plans (core, storage L1–L9, engine conformance, executor/IDL, CLI,
inference, hub, wasm/stratadb, corruption) are done. The workspace error-code
guard is the durable artifact: its allowlist now holds **32 entries, every one
legitimately deferred** — 22 defensive/unreachable-via-API (the `#2651` encode
arms + 2 verified-unreachable guards) and 10 asserted by IDL replay fixtures
(JSON, invisible to the grep-guard). No stale "a future slice will assert this"
promise remains. The finale also surfaced **#2682** (a possible off-lock
torn-read / atomicity question), filed with a classification plan rather than
dismissed.

**Addendum — TCP3.16 (2026-07-18).** The exit gate first re-labelled the four
CLI `doctor` codes as "deferred — needs a host-env harness." A follow-up
question exposed that as too charitable: `doctor.rs` simply had zero tests, and
all four codes are reachable hermetically by perturbing one environment axis
(`--db` at a missing path, `STRATA_HOME` at a file, `HOME`/`STRATA_HOME` unset,
`PATH` without the binary). New `crates/cli/tests/doctor_behavior.rs` (5 tests,
real-binary) asserts all four plus the healthy path, dropping them off the
allowlist (36 → 32). The remaining 32 are the only two honest categories:
defensive-unreachable and IDL-fixture-asserted.

## Phase 4 — Volume via generation (bug-hunting to 10×)

Phases 1–3 built the *oracles*: the STH-1 recovery oracle, the config-sweep
differential, the temporal/event timeline models, the IDL (125 commands with
schemas/errors/examples), the golden-vector infrastructure, and 30 fuzz
targets. Those are the fuel a test *generator* burns. Phase 4 turns them loose
to hunt bugs exhaustively.

The target is a **10× test:code ratio — as an internal robustness target, not a
published number.** The point is to iron out every findable bug; 10× is simply
the scale of machinery required to hunt that exhaustively, and the ratio is a
*byproduct* of building it, not the goal. Whether and how to publish the number
as a credibility headline is a **separate review round after Phase 4 closes** —
gated on the mutation-kill and coverage evidence behind it, so the figure can
withstand a sophisticated reader.

**You reach 10× by generation and vendoring, not by typing.** Current: ~1.9×
(~314K test / ~163K product). 10× is ~1.6–2.0M test LOC — north of a million
new lines, unreachable by hand. SQLite's ~590× is TH3 (generated MC/DC), SQL
Logic Test (~7M generated queries run differentially), and fuzz corpora — not
92M hand-written lines. Same model here: the volume comes from a few
oracle-checked generators, so every generated case is a *bug-detection surface*,
never a vanity line. The single biggest lever (4.2) is the multi-model analog of
SLT — **differential testing each capability against a reference database**
(KV vs RocksDB/Redis, JSON vs Mongo, graph vs Neo4j), since a database built by
other people is a stronger oracle than any model we could write ourselves.

Phase 4 is also **evidence-directed**, not a blind sweep: the #2686 adversarial
pass gave us a labelled sample of what our coverage misses, so each generator is
aimed at a class the codebase has already proven it harbours (see below).

### What Phase 4 must hunt — the empirical gap classes (#2686 evidence)

The #2686 adversarial pass (19 engine defects, reproduced through the Python SDK
torture suite) is a **labelled sample of the bugs a ~1.9× suite and a green
error-code guard let through**. The root-cause retro shows they are not 19
unrelated mistakes — they cluster into a small set of *coverage classes*, and
the operative fact is this:

> **Each found bug is a sample of a class, not a singleton.** We fixed 19; the
> codebase has now *proven* it harbours these kinds of defect, so the remaining
> population is almost certainly larger. Phase 4's generators are therefore
> aimed at the *class*, and each is seeded with a real #2686 instance it must
> re-find before we trust it to find new ones. Fixing the seed does not close
> the class — the lane stays until its mutation-kill on that surface plateaus.

The classes, with confirmed instances and the slice that owns hunting the rest:

| Gap class | Confirmed #2686 instances | Why the current suite missed it | Owning slice |
|---|---|---|---|
| **Round-trip / inverse-pair fidelity** (`read == write`, `import∘export == id`) | #2687, #2688, #2689 | We assert an op returns a plausible *shape*, never value-equality. Golden vectors pin the *encoding*, so loss *before* the codec (int→f64) round-trips cleanly and looks healthy | 4.2 (diff), 4.5 (matrix), 4.6 (fidelity oracle, not just crash) |
| **Boundary / extreme-value input domain** | #2687 (2⁶³/2⁶⁴), #2689 (subnormals), #2693 (huge norms), #2698 (as_of 0/∞) | Hand-written tests use *typical* values; nothing systematically generates numeric/time boundaries or non-representable inputs | 4.4 (float/edge corpora), 4.5 (boundary vectors) |
| **Output invariants / cross-algorithm differential** | #2692 (bfs vs sssp), #2693 (cosine ∈ [-1,1]), #2706 (cdlp convergence) | Graph/vector tests pin outputs on small inputs, not invariants that must hold for *all* inputs | 4.2e (exact k-NN), 4.7 |
| **Error-contract *correctness*** (condition → right code/class/retryable) | #2699 | The error-code guard proves a code is asserted *somewhere*, never that *this condition* yields *that* code. #2699's codes are asserted elsewhere, so the guard stayed green | 4.8 (new) |
| **Cross-surface parity** (parallel surfaces obey one contract) | #2691, #2694, #2695, #2700, #2701, #2702, #2704 | Every surface is tested in isolation; nothing diffs export vs import, branch vs space, the batch family, forward vs reverse range, or as_of across reads | 4.7 (new) |
| **Adversarial deserialization on nested structs** | #2696, #2705 (batch_write, non-dict metadata, metric aliases) | `deny_unknown_fields` / type-rejection is tested at the top-level `Command`, not recursively on nested option structs | 4.1 (extend to nested) |
| **Fault breadth: absence/removal + health-vs-truth** | #2690 | The recovery oracle mutates bytes, never *removes* a file, and trusts self-reported health over ground truth | 4.9 (new) |
| **Inert-feature / dead-surface** | #2703 (indexes do nothing), #2704 (catalog not executable) | We test that `create_index` *succeeds*, not that it changes any observable behaviour | 4.7 (parity), 4.1 (catalog executability) |

**Two instruments actively misled us, and Phase 4 corrects both.** The
error-code assertion guard and the test:code ratio both measure *breadth of
touch*, not *depth of correctness* — a green guard and a rising ratio were fully
consistent with all 19 bugs. This is the concrete case for the program's
standing rule that **mutation-kill, not ratio, is the gate**: every bug in the
table is one a mutation test or an oracle-checked round-trip would kill.

**Root cause of the blind spot.** Every one of these bugs lives in a branch the
author did not picture (the else-arm of a finiteness check on the wrong type, a
second `strip_prefix`, an import enum never extended). Tests written by the
author encode the author's mental model and are blind exactly where it is wrong.
The cure is structural — the *input choice* and the *correctness verdict* must
both come from somewhere other than the author: generators, differential
oracles, round-trip identities, invariants. That is the whole of Phase 4.

### Classes evidenced outside #2686

The older open bugs on the tracker cluster the same way — they are earlier
samples of classes the #2686 pass did not happen to hit, found by other passes
(the executor audit, the billion-scale campaign, the retention audit, ad hoc
use). Same treatment: each class gets a seeded lane and stays open until its
mutation-kill plateaus.

| Gap class | Confirmed instances | Why the current suite missed it | Owning slice |
|---|---|---|---|
| **Resource-bound correctness** (memory budget, disk-full, OOM) | #2567 (1B-key recovery ~56 GB RSS, OOM-killed — recovery memory unbounded by the budget) | The fault seam covers corrupt/truncate/io-fail but never *exhaustion*; the memory budget is a product contract nothing asserts, and correctness-at-scale lives only in the benchmarks repo | 4.9 (taxonomy gains resource faults; budget-adherence oracle) |
| **Branch-lineage depth + lifecycle cycling** | #2521 (fork-of-a-fork drops the middle branch's inherited state), #2522 (fork as-of pre-fork timestamp fails closed), #2466/#2467 (retention breaks under delete-recreate and clear-with-descendants cycles) | Hand tests fork once from the default branch and never cycle lifecycles; no generator produces deep fork DAGs or delete-recreate churn | 4.2 metamorphic tier (op-sequence generator must emit deep DAGs + lifecycle churn) |
| **Outer-surface input fidelity** (REPL/CLI/SDK text → command) | #2571 (REPL tokenizer strips quotes from inline JSON, silently storing corrupted values) | Round-trip testing starts at the `Command` layer; the outermost parse path — where users actually type — has no fidelity oracle | 4.1 (extend round-trip to REPL/pipe input) |
| **Declared-vs-observed schema parity** (the IDL/docs tell the truth) | #2596 (`json_get` emits a second output variant the IDL doesn't declare), #2569 (help text says microseconds; both flags take the logical commit clock) | The IDL guards check *structure and drift*, never that declarations match observed behavior; help text and prose are unguarded entirely | 4.1 (observed output variants ⊆ declared; help-text facts ↔ IDL) |

**Seed caveat.** Unlike the #2686 rows, some of these seeds predate the V1
promotion (#2521, #2522, #2466, #2467). Gate 7 applies unchanged, with one
preliminary: each seed is first re-verified against `main`. A seed that still
reproduces is the lane's re-find target; one that no longer reproduces gets its
regression test on the spot and the lane runs anyway — the class evidence
stands either way.

### Phase 4 gates (how the volume stays honest)

1. **Mutation-kill is the primary ratchet** (only ratchets up). A generation
   lane that adds LOC but does not raise the workspace mutation-kill rate is
   cut. This single rule is the entire defense against LOC-farming.
2. **Every generated case is oracle-checked** — differential, metamorphic,
   round-trip identity, output invariant, property, or golden-pinned. The
   verdict must judge *correctness*, not existence: existence/shape-only
   assertions (`is_some`, status-only, "op succeeded", "returned a vector")
   are banned in a generator template, because the #2686 class of bug passes
   every one of them.
3. **Regeneration is deterministic.** Committed corpora regenerate
   byte-identically in CI (a drift guard asserts no diff), so reviewers review
   the *generator and its oracle*, never a million generated lines.
4. **CI is tiered.** A fast representative subset runs per-PR; the full corpora
   and soaks run nightly. Volume must not wreck PR latency.
5. **Every bug found → issue → fix → regression test**, immediately (standing
   program rule). Phase 4's deliverable is *bugs found and fixed*; the ratio is
   the receipt.
6. **The ratio is a thermometer, never a gate** — consistent with the program's
   coverage-over-line-count principle. Coverage and mutation-kill are the gates.
7. **Every gap class carries a seed, and a class is never "done".** Each
   class-hunting lane must first reproduce its known #2686 instance (the seed) —
   a generator that cannot re-find the bug we already know is there is not
   searching that class. The seed's fix is *not* the exit; the lane keeps
   running (nightly) until its mutation-kill on that surface plateaus, because
   the found instances are a sample and the population is unknown.

### Phase 4 slices

Sequenced foundation-first: 4.1 establishes the generate → drift-guard →
mutation-gate pattern that every later lane reuses. The seeded class-hunters
(**4.7–4.9**) are small, each already has a known #2686 bug to validate against,
and they directly answer *"how many more of these are there?"* — so they run in
parallel with 4.1/4.2a rather than waiting behind the big differential lanes.

| # | Slice | Scope | Status |
|---|---|---|---|
| 4.1 | **IDL conformance generator** | Emit per-command generated test files from the 125-command IDL: request/response round-trip × render mode (json/raw/human), every declared error envelope, schema/boundary conformance, adversarial deserialize. Two seeded extensions: **outer-surface input fidelity** — the round-trip starts at REPL/pipe *text*, not the `Command` struct, so tokenizer loss fails the oracle (seed #2571) — and **declared-vs-observed parity** — every output variant a replay observes must be declared by the schema, and help-text facts must match the IDL (seeds #2596, #2569). Regenerate on IDL change; drift guard + mutation gate. Highest leverage per effort (monetizes an owned asset) and proves the generation pattern | Planned |
| 4.2 | **Differential testing vs reference databases (the SLT model)** | The bulk of the 10× and the highest bug yield. StrataDB is *multi-model*, so each capability is diffed against the mature reference implementation of that model — KV vs **RocksDB + Redis**, JSON vs **MongoDB**, graph vs **Neo4j**, etc. — on the *shared* semantic contract, plus in-house metamorphic relations (branches, time-travel) reusing the Phase 1–3 oracles. Seed-pinned op-sequences, committed corpora, divergence = filed bug. **Detailed below; starts with KV (4.2a).** | Planned |
| 4.3 | **Exhaustive concurrency-schedule exploration** | loom/shuttle over the commit / BS5 write-group / latest-scan interleavings, turning rare CI flakes into deterministic, seed-reproducible findings. #2682 (off-lock torn read) is the seed case. High bug yield for a database; the highest-value non-LOC lever | Planned |
| 4.4 | **Vendored public conformance suites** | Import battle-tested corpora wholesale: JSONPath Compliance Test Suite, Unicode collation/normalization, float-format edge cases, and analogous KV/vector/graph reference sets. A small runner over large vendored data | Planned |
| 4.5 | **Golden + combinatorial matrix expansion** | Every record type × canonical/boundary/adversarial vectors across the frozen codec; the full config × capability × operation cross-product (STH-6 extended), generated and result-equality-checked | Planned |
| 4.6 | **Committed fuzz corpora + surface expansion** | Version the accumulated persistent-corpus interesting-inputs; extend the 30 fuzz targets to the full decoder/API surface — and past crash-only oracles to **round-trip fidelity** (`decode∘encode == id`, `read == write`) so value loss (not just panics) fails a target; commit crash-triage regressions. Seeds: #2688, #2689 | Planned |
| 4.7 | **Cross-surface parity harness (internal differential)** | Strata-vs-analogous-Strata: assert parallel surfaces obey one contract — export↔import symmetry, branch↔space lifecycle/resolution, the batch family's failure channels, the range direction × endpoint matrix (reverse ↔ forward walk, `range` ↔ `range_by_time` bounds), the `as_of` × read-command matrix, and catalog-id → wire-name executability (construct a real call from every catalog entry). No external DB; the oracle is Strata's *own other surface*, so it is cheap and needs no service container. Also flags inert features (a surface whose output never differs). Seeds: #2691, #2694, #2695, #2700, #2701, #2702, #2703, #2704 | Planned |
| 4.8 | **Error-contract correctness harness** | Fixture per (failure condition → expected code / class / retryable / redaction), driven through the fault seam and the bad-input surface, asserting the *mapping* — not mere code presence, which the Phase 3 error-code guard already covers and which #2699 shows is insufficient. Catches transient-vs-permanent and wrong-area misclassification across the open/read/write paths. Seed: #2699 | Planned |
| 4.9 | **Fault-taxonomy extension + health-vs-truth oracle** | Extend the recovery/fault seam from {corrupt, truncate, io-fail} to **{delete, missing, reorder}** at the segment/artifact level and **{disk-full, allocation failure}** as resource faults, plus a budget-adherence oracle (peak recovery/maintenance memory must respect the configured budget — asserted at a CI-sized scale, with the 1B-key leg in the benchmarks repo's soak). Add an oracle that diffs self-reported health (`verify_chain`, `doctor`, `HEALTHY`) against known ground truth after each fault — a database that lost data must never self-report healthy. Extends the STH-1 recovery oracle from Phase 1. Seeds: #2690, #2567 | Planned |

### 4.2 in detail — differential testing against reference databases

The strongest oracle is not one we write — it is a mature database built by
other people who do not share our assumptions or our bugs. That is SQLite's SLT
model: it diffs SQL semantics against PostgreSQL, MySQL, and Oracle. StrataDB is
**multi-model**, so the same move applies *per capability* — each has a
reference implementation of its model to diff against:

| Capability | Reference oracle(s) | Validates | Sub-slice |
|---|---|---|---|
| KV | **RocksDB** (ordered, embedded) + **Redis** (hash, independent) | put/get/delete/exists/count; ordered scan & range; snapshot/as-of reads (RocksDB) | 4.2a — **first** |
| JSON / document | **MongoDB** | document set/get/patch, path reads, index-backed queries | 4.2b |
| Graph | **Neo4j** | nodes/edges, neighbors by direction/type, reachability/traversal | 4.2c |
| Event | append-log oracle (Redis Streams / in-house) | append ordering, per-type sequence, range/reverse reads | 4.2d |
| Vector | **brute-force exact k-NN** (in-house; external ANN is approximate) | exact top-k and recall bounds, not byte-equality | 4.2e |

**The contract is the whole game.** StrataDB is not RocksDB and not Redis; a
naive "same ops, diff everything" is a noise machine the moment semantics
diverge. Each harness diffs only the *intersection where the backends provably
agree*, and excludes the rest **by construction** — not by triaging failures
after the fact. KV is the worked example, in three tiers:

| Tier | Ops | Shared with |
|---|---|---|
| A — universal | put / get / delete / exists / overwrite / count / batch | RocksDB + Redis |
| B — ordered keyspace | `list(prefix)`, `scan_range`, ordered pagination | RocksDB only (Redis is unordered) |
| C — snapshot / as-of | `get_at_version`, `get_at`, `count_at` | RocksDB only (RocksDB snapshot after op N ↔ Strata as-of N) |
| Strata-only | branches (incl. deep fork DAGs), spaces, lifecycle churn (delete-recreate, clear), deep history (`get_versions`) | *none* → in-house metamorphic |

So 4.2 is a **hybrid** per capability: external differential on the shared
tiers, plus in-house metamorphic relations on the Strata-specific parts (fork
isolation, "as-of(v) = the state after ops ≤ v", history monotonicity,
cache ≡ durable, reopen-preserves-state — reusing the STH-1 oracle,
config-differential, and timeline models from Phases 1–3). The op-sequence
generator must emit the shapes hand tests never produce: **deep fork DAGs**
(fork-of-fork — inheritance is transitive through the middle branch) and
**lifecycle churn** (delete-recreate cycles, clear with live descendants,
fork-then-delete-parent), the branch-lineage class seeded by #2521/#2522 and
#2466/#2467.

**Architecture** (small): a `KvOracle` trait with `StrataKv`/`RocksKv`/`RedisKv`
adapters; a seed-pinned generator emitting op-sequences over a
**domain-restricted** key/value space (the intersection of what all backends
accept — e.g. non-empty keys, bounded byte values); after each op, diff the
tiers each backend supports; divergence on a supported tier = filed bug +
committed seed. The seed corpora are the SLT-analog volume.

**Infra and honesty.** RocksDB is an ordered embedded LSM — nearly a semantic
twin of Strata's KV substrate — so it validates Tiers A–C and runs
**in-process** (a feature-gated dev-dependency, zero external infra); it does
most of the bug-finding. Redis covers only Tier A, but it is architecturally
*unlike* Strata (in-memory hash, not LSM), so it catches design-family blind
spots RocksDB and Strata might share — real defense-in-depth. Redis/Mongo/Neo4j
run as **service containers in a nightly lane**; nothing external touches a
per-PR build. Every 4.2 lane sits behind a `differential` feature + nightly CI.

**Sub-slices**, ordered by semantic simplicity and foundation value: **4.2a KV
(RocksDB + Redis)** — builds the harness/generator/contract pattern the rest
reuse — then 4.2b JSON (Mongo), 4.2c graph (Neo4j), 4.2d event, 4.2e vector
(exact k-NN oracle). 4.2a first.

### Milestones and exit

The ratio is a thermometer read at three checkpoints — **4–5× (the program's
existing working floor) → 7× → 10×** — and at *each* checkpoint the mutation-kill
floor must have risen. If marginal generated volume stops finding bugs
(mutation-kill plateaus), we stop adding it: 10× is the aspiration, bugs-found
is the constraint.

**Phase 4 exit** = "all findable bugs ironed out," operationalized: mutation-kill
plateaus at a high floor, **every gap class in both tables above — the #2686
retro and the tracker-evidenced additions — has a running lane whose
mutation-kill on that surface has itself plateaued** (each class
searched to exhaustion, not just its seed fixed), the differential and fuzz
soaks run sustained-clean, concurrency-schedule exploration exhausts the
tractable interleavings, and the ratio has reached ~10× as a byproduct. Only
then does the separate review round decide whether to publish the ratio as an
external credibility headline.

## Deferred register

Recorded so absence is a decision, not an accident:

| Item | Why deferred | Re-entry condition |
|---|---|---|
| Branch merge / compare / promote / restore / revert / cherry-pick tests (rule 20) | Operations intentionally unimplemented in V1; guards assert absence | When the ops land post-V1 |
| M6E retrieval / derived-state engine tests | Retrieval stages land with intelligence work | Intelligence crate work begins |
| `strata-intelligence` crate + `fake_provider_paths` gate | Crate does not exist yet (M8 scope) | M8 starts; CLAUDE.md reference stays marked M8+ |
| Hub telemetry tests (M9TD) | No telemetry implementation exists | Telemetry feature decision |
| loom/shuttle for L7 | Hand-rolled deterministic guard interleavings accepted by plan | Revisit at 2.4 |
| Close-runner rotate-refusal harmonization onto `decide_flush_rotation` (#2612 residual) | Production-unreachable (no production `DrainBeforeClose` flush producer); close-at-saturation verified sound | A production `DrainBeforeClose` flush producer appears |
| OpenDAL/object backend conformance | Backend is post-V1 | Backend work starts |
| CLI `--memory-budget` / `--profile` flags and `commands` / `explain` subcommands (cli-next plan items) | The shipped CLI never grew these surfaces; testing them would test nothing | If/when the CLI adds resource-profile flags or the IDL-generated command explorer |
| Per-branch orphaned-delta fix (durable flushed-branch set + per-branch recovery, lifting the checkpoint guard and the #2624 close defer; multi-branch crash harness extension) | Frozen-format manifest change + two-phase recovery rework, coordinated with post-V1 multi-branch durable maintenance; guard verified airtight across all three publish paths (2.7) | Multi-branch durable-maintenance work starts (plan: `implementation-plans/storage-testing/multi-branch-orphaned-delta-recovery-gap.md`) |

## Tracking

1. Slices use `TCP{phase}.{n}` in PR titles (e.g., `TCP1.2: STH-5 failure-during-failure`),
   alongside any existing STH code.
2. This document's tables are the ledger — update status on merge.
3. Stale trackers (`v1-progress-tracker.md`, the two test-inventory docs)
   are historical; this document supersedes them for test work. Do not
   resurrect them.
