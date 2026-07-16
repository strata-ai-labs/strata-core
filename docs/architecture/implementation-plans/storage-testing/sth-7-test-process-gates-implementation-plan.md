# STH-7 Implementation Plan: Test-Process Gates + Anti-Drift

Status: implemented — 7a (2026-07-16, TCP1.1), 7b/7c/7d (2026-07-16, TCP1.5). See the as-built sections below.
Charter classes: 11 — Coverage/mutation (❌ → ✅), 12 — Memory safety (🟡 → ✅), 7 — deepen (continuous + structure-aware fuzz), plus the anti-drift map guard
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: the suites it measures (run coverage/mutation after STH-1..6 exist; Miri/sanitizer can land now).

## As built (7a) — 2026-07-16, slice TCP1.1

`.github/workflows/nightly.yml` (schedule 03:00 UTC + dispatch) carries the
memory-safety lanes and the remaining legs of the 3-way suite run:

1. `memory-safety-miri` — Miri with `-Zmiri-strict-provenance
   -Zmiri-disable-isolation` over all of `strata-core` and the
   `strata-storage` format layer (`--lib format::`), `PROPTEST_CASES=8`.
   Both lanes verified green locally before landing.
2. `memory-safety-address-sanitizer` — ASAN + LSAN over the full
   `strata-storage` and `strata-engine` suites via `-Zbuild-std`
   (instrumented std, no holes). Engine was leak-clean from the start.
   Storage required program slice 1.7 (TCP1.7, same day): the first LSAN
   run failed on ~190 KB across 5,518 allocations, all traced to the ~609
   intentional `Box::leak(Box::new(...))` fixture sites in storage test
   code. Those now route through `testkit::leak_static` (and
   `testkit::forget_registered` for the `mem::forget` crash simulation),
   which keep intentional leaks reachable from a process-global registry —
   so LSAN runs with leak detection on and only real leaks fail the lane.
   The "leak check wired into integration runs" exit item is met.
3. `memory-safety-thread-sanitizer` — TSAN over `strata-storage`
   (commit guards, maintenance scheduler, close ordering).
4. `coverage-baseline` — `cargo llvm-cov` workspace run publishing the
   per-crate table to the job summary + lcov artifact (90-day retention).
   Baseline only; the merge-blocking threshold gate is 7b, thresholds
   ratchet up only.
5. `release-mode-tests` — workspace tests with `debug_assertions` off.
   Per-PR CI remains the debug-assertions leg; coverage is the third way.

Deviations from the sketch above: Miri covers core + storage format layer
(not engine — interpreter cost; revisit when nightly budget allows); the
per-test allocator-counter/dhat assertion is deferred to 7b in favor of
LSAN whole-process leak checking.

## As built (7b + 7c + 7d) — 2026-07-16, slice TCP1.5

- **7b coverage + mutation.** The nightly coverage job gates the workspace
  region floor (73.0%, ratchet-up-only; baseline 73.64%) after publishing the
  per-crate table. Per-PR, the `mutation-on-diff` job in `ci.yml` runs
  `cargo-mutants --in-diff` over exactly the lines the PR touches — an
  un-killed mutant fails the job; doc/test-only diffs pass trivially.
  Deviations from the sketch: the gate is the llvm region floor, not MC/DC
  (tracked headroom), and mutation is diff-scoped rather than a full-tree
  campaign (affordability, per the plan's own constraint).
- **7c continuous fuzz.** `.github/workflows/fuzz.yml` (04:30 UTC + dispatch)
  runs all 30 targets nightly with the corpus persisted via the actions
  cache, so coverage compounds across nights; crash artifacts upload on
  failure. The generated-script targets (commit runtime, lifecycle, services,
  branch LSM) are the structure-aware layer — they interpret fuzz bytes as
  operation scripts through model-checked harnesses. `arbitrary`-derived
  whole-DB-file generators remain headroom.
- **7d charter guard.** `crates/storage/tests/testing_charter_guard.rs`
  parses the backticked evidence anchors out of the charter documents
  (taxonomy, gold-standard delta, coverage program, STH README) and asserts
  every cited artifact exists — deletion or rename now breaks CI, with an
  explicit reasoned allowlist for future references. A second test pins the
  primary artifact of every implemented STH slice. On its first run the
  guard caught one drifted reference and its vacuity floor is itself
  asserted.

## Objective

Make the discipline enforceable, not aspirational. Add the CI gates that turn the
charter's principles into merge-blocking checks: Miri + sanitizers (class 12),
coverage + mutation testing (class 11), continuous and structure-aware fuzzing
(deepen class 7), and — the principle this whole effort exists to prove — a
**self-verifying charter guard** so the testing map can never silently drift
again.

## Why this matters (blog beat)

Technique without discipline decays. SQLite's reputation rests not only on its
tests but on its *gates*: 100% MC/DC coverage, mutation testing that verifies the
suite kills ~20k injected faults, valgrind and leak checks per test. RocksDB runs
ASAN/TSAN/UBSAN continuously and treats extending the stress test as part of
shipping. The difference between a large suite and a world-class one is that the
world-class one *cannot be weakened without CI noticing*. And the most StrataDB-
specific lesson: this very charter drifted on its sharpest claims in two weeks —
so the map itself gets a guard.

## Seams to build on (verified 2026-06-17)

- CI today (`.github/workflows/ci.yml`): `cargo fmt --check`, `cargo clippy
  --workspace --all-targets --all-features`, `cargo deny`, dependency-direction
  guards, `cargo hack` feature-powerset (check + test), `cargo test --workspace`,
  doc tests, release builds. **No** coverage, mutation, Miri, or sanitizer.
- `#![deny(unsafe_code)]` (`src/lib.rs:11`) — the core is unsafe-free, so Miri is
  cheap and high-signal; sanitizers matter at the backend/FFI edge.
- Fuzz corpus: 28 targets in `fuzz/fuzz_targets/` + 39 goldens — present but not
  run continuously or structure-aware.
- The source-guard/closeout pattern (`tests/*_source_guard.rs`) — the model for
  a self-verifying charter guard.

## Coverage target (not line count)

Exit bars: (12) Miri on the unsafe-free core in CI + sanitizer job over
backend/FFI + per-test leak assertion; (11) coverage gate on core modules +
mutation testing with a committed kill threshold, merge-blocking; (7-deepen)
nightly continuous fuzz with corpus persistence + structure-aware DB-file fuzzing;
(anti-drift) a guard that fails CI if a charter-cited artifact disappears.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 7a | Miri + sanitizer CI (class 12) | A Miri job on the unsafe-free core; an ASAN/TSAN job over backend/FFI tests; leak check wired into integration runs |
| 7b | Coverage + mutation gates (class 11) | `cargo-llvm-cov` gate on core modules with a committed threshold; `cargo-mutants` on changed files with a kill threshold; merge-blocking, incremental to stay affordable |
| 7c | Continuous + structure-aware fuzz (class 7) | Nightly fuzz run with corpus persistence; `arbitrary`-derived structure-aware DB-file fuzzing across decoders |
| 7d | Charter anti-drift guard | A closeout test parses the charter's evidence anchors and asserts the named files/tests/symbols exist; deletes/renames break CI, not trust |

## Implementation detail

### 7a — Memory-safety CI (`.github/workflows/`)
Add a Miri job (`cargo +nightly miri test`) scoped to the unsafe-free crates
(core-next, storage-next, engine-next) — cheap because there is no unsafe to model
around, high-signal for UB in `unsafe`-adjacent FFI assumptions. Add a sanitizer
job (`-Zsanitizer=address,thread`) over the backend/IO tests where real syscalls
and any FFI live. Wire a leak assertion (allocator counter or `dhat`) into the
integration harness teardown.

### 7b — Coverage + mutation (`.github/workflows/` + config)
`cargo-llvm-cov` produces a coverage report; gate core modules
(format, WAL, recovery, commit) at a committed threshold (start where we are,
ratchet up — never down). `cargo-mutants` run on the *diff* of each PR (changed
files only, to bound cost) with a committed kill-rate threshold; an un-killed
mutant is a missing assertion, filed as a test gap. Both merge-blocking.

### 7c — Continuous + structure-aware fuzz (`fuzz/`)
A nightly workflow runs each of the 28 targets for a fixed budget with corpus
persisted between runs (so coverage compounds). Add structure-aware targets:
`arbitrary`-derived DB-file/manifest/WAL generators that produce *plausible*
malformed inputs (the dbsqlfuzz idea), reaching decoder states random bytes never
hit. New crashers auto-minimize into the corpus (the regression protocol).

### 7d — Charter guard (`tests/testing_charter_guard.rs`)
The "the map may not lie" principle, enforced. Parse the charter's status table
and its inline evidence anchors (file paths, test names, symbols), and assert each
named artifact exists in the tree. If a suite is deleted or a seam renamed, this
test fails — so the charter can only drift through a *visible* CI failure, never
silently. This is the structural fix for the drift this whole effort uncovered.

## Constraints

- Gates ratchet one direction: thresholds may rise, never fall, without explicit
  sign-off recorded in the charter.
- Cost-bounded: mutation on diffs, fuzz on a nightly budget, Miri on the
  unsafe-free core — affordable on every PR / nightly.
- The charter guard asserts *existence*, not content, to avoid brittleness — it
  catches deletion/rename, not wording.
- Behavioral names; no class codes in identifiers.

## Exit gate

- CI enforces Miri + sanitizer (class 12 → ✅), coverage + mutation gates
  (class 11 → ✅), and runs continuous + structure-aware fuzz (class 7 deepened).
- The charter guard is green and fails on any missing cited artifact.
- The charter's "Operating discipline" section is now machine-enforced, not just
  documented.
