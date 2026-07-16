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
| 2.2 | **CI tiers** | Nightly workflow running the 11 `#[ignore]` soak lanes + `stress.rs` + process-crash harness; scheduled fuzz over the 30 existing targets (libFuzzer, corpus cached); wasm32 memory/cache *test* job (today: compile-only); golden/format gate in `release.yml` |
| 2.3 | **CLI integration suite** | `crates/cli/tests/` with `assert_cmd`: durable cross-process execution (KV + vector execution plans have ready case lists), REPL/pipe scripts, init/new, open/path, output-format round-trips, clone/info rendering. Reconcile plan items with no backing flags (`--memory-budget`, `--profile`, `commands`, `explain`) — implement or move to deferred register |
| 2.4 | **Engine branch concurrency races** | The unwritten `branch_faults.rs`: concurrent same-name create (one winner), delete-vs-write, recreate-vs-stale-write, concurrent fork. Decide loom/shuttle adoption for L7 guard interleavings while here |
| 2.5 | **Inference testkit (M7F)** | Fake `Generator`/`Embedder`/`Reranker` providers behind the (currently empty) `testkit` feature; the 18-case deterministic harness; unblocks the executor deterministic inference lane. Also: download failure-path unit tests (~13 missing cases), runtime cache lifecycle unit tests (~21 missing) |
| 2.6 | **Small zero-coverage surfaces** | `crates/wasm` (wasm-bindgen-test smoke over the serialized-command adapter), `stratadb` facade (public-surface conformance beyond the single round-trip test), hub per-endpoint suite |
| 2.7 | **Multi-branch orphaned-delta recovery** | Currently guarded (checkpoint defers), latent high-severity. Decide: fix per-branch recovery in-program or keep guard + add adversarial regression coverage |
| 2.8 | **Close-time flush surfaces (#2612)** | Adopt `decide_flush_rotation` (or a close-specific flush-backlog-then-rotate variant) at the durable/cache close runners and the cache background step; tests stage a saturated store and assert graceful close drains fully |

Phase 2 exit gate: no known gap without either a merged test lane or an
entry in the deferred register.

## Phase 3 — Layer-by-layer coverage plans

After Phases 1-2, build one coverage plan per layer, in dependency order:
core → storage (L1-L9 sweep against the conformance plan's matrices) →
engine (per capability) → executor/IDL → CLI/SDK → hub/wasm. Each plan is
co-authored, uses the coverage numbers from 1.1 to target the weakest
modules first, and sets a per-crate coverage ratchet. Not specified further
here — plans are written when their phase starts.

## Deferred register

Recorded so absence is a decision, not an accident:

| Item | Why deferred | Re-entry condition |
|---|---|---|
| Branch merge / compare / promote / restore / revert / cherry-pick tests (rule 20) | Operations intentionally unimplemented in V1; guards assert absence | When the ops land post-V1 |
| M6E retrieval / derived-state engine tests | Retrieval stages land with intelligence work | Intelligence crate work begins |
| `strata-intelligence` crate + `fake_provider_paths` gate | Crate does not exist yet (M8 scope) | M8 starts; CLAUDE.md reference stays marked M8+ |
| Hub telemetry tests (M9TD) | No telemetry implementation exists | Telemetry feature decision |
| loom/shuttle for L7 | Hand-rolled deterministic guard interleavings accepted by plan | Revisit at 2.4 |
| OpenDAL/object backend conformance | Backend is post-V1 | Backend work starts |

## Tracking

1. Slices use `TCP{phase}.{n}` in PR titles (e.g., `TCP1.2: STH-5 failure-during-failure`),
   alongside any existing STH code.
2. This document's tables are the ledger — update status on merge.
3. Stale trackers (`v1-progress-tracker.md`, the two test-inventory docs)
   are historical; this document supersedes them for test work. Do not
   resurrect them.
