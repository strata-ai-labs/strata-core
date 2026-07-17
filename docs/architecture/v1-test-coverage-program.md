# V1 Test Coverage Program

Status: active — Phase 1 not started
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
| 3.2c | Storage inner-error assertability — `TableRuntimeError::code()` (11) + lifecycle hop + reachability guard | Planned |
| 3.3 | Storage decode + fault edges (L2 fuzz, L1 recovery faults, L9 negatives) | Planned |
| 3.4 | Storage concurrency (lock-order guard, threaded COW races) | Planned |
| 3.5 | Engine error registry (guard + doc reconciliation + refusal batches) | Planned |
| 3.6 | Engine conformance depth (fault dimension, temporal oracle generalization) | Planned |
| 3.7 | Engine contract truth-ups (rule 20 absence guard, dead-code deletion) | Planned |
| 3.8 | Executor error-envelope replay + per-command guards | Planned |
| 3.9 | Executor hermetic inference lane + branch/session behavior | Planned |
| 3.10 | CLI renderers + verb-enumeration guard + config write path | Planned |
| 3.11 | CLI family coverage (corpus port) | Planned |
| 3.12 | Inference deterministic residuals (request goldens, wire mapping) | Planned |
| 3.13 | Hub transport fault injection | Planned |
| 3.14 | wasm + stratadb residuals | Planned |
| 3.15 | Engine corruption injection (data_loss assertion) | Planned |

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
