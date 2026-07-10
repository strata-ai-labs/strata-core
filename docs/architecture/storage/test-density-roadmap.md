# Storage-Next Test Density Roadmap

Status: V1 discipline draft, scoped initially to storage-next

## Purpose

This document defines what it would take for storage-next testing to go from
"adequate-for-V1-milestone" to SQLite-grade, and the tiers in between.

The trigger for this roadmap was an honest reassessment of M3 completion. The
M3 audit concluded that L1-L4 had ~1.06x test-to-production LOC ratio and
called the result "reference-grade." That framing was overconfident. SQLite
runs at roughly 600x test-to-production LOC with continuous fuzzing, 100%
branch coverage, IO error injection at every syscall, and 25 years of soak.
Strata-storage-next is not in that league and pretending it is helps nobody.

This roadmap exists so:

1. We track test density as an explicit, durable engineering metric instead of
   declaring "reference-grade" once and moving on.
2. We have a shared language for what each tier of test density means and what
   gating infrastructure it requires.
3. Other crates (core-next, engine-next, intelligence-next, inference-next)
   can adopt the same tier model when they reach M3-equivalent maturity.
4. Future Strata reviewers can see the gap to SQLite as a concrete, planned
   workstream rather than an aspiration.

## Why LOC Ratio Is A Lousy Metric

Pure test-to-production LOC ratio is gameable and crude. A thousand lines of
trivial roundtrip tests are worth less than a hundred lines of property tests
that find real bugs. Goldens are tiny in LOC but lock the byte format
permanently. Fuzz corpus is bytes, not lines.

So this roadmap uses LOC ratio as a **coarse signal** alongside more meaningful
measures. The tier definitions below describe the metric *suite* required at
each tier, not just the LOC bar.

## Metric Suite

Every tier from T1 upward should be characterized by at least:

| Metric | What it measures | Why it matters |
| --- | --- | --- |
| **Test-to-production LOC ratio** | Sheer volume of test code | Coarse signal that testing grows with the codebase. Cheap to game; only useful in combination. |
| **Branch coverage** | Fraction of branches actually executed by the suite | Objective, hard to game, catches dead code and untested error paths. |
| **Fuzz target count + corpus size** | How many public decoders/parsers are fuzzed and the seeded corpus per target | Drives parser robustness. Each public byte-format decoder should have a fuzz target. |
| **Continuous fuzz compute hours / week** | Wall-clock time fuzzers run per target | Distinguishes "fuzzed once" from "continuously fuzzed." |
| **Distinct property-test invariants** | Number of named proptest invariants | Density of structural properties checked. |
| **Distinct fault-window scenarios** | Service-publish, IO-error, OOM, crash-point combinations exercised | Directly measures the failure surface area covered. |
| **Anomaly-test coverage** | Randomized crash/error injection across the codebase vs scripted at known points | T1-T3 use scripted; T4 randomizes across every syscall. |
| **Platform coverage** | CI lanes (Linux, macOS, Windows, BSD, etc.) | Cross-platform branches that aren't tested are tested by reviewers, badly. |
| **Differential testing** | Same workload against storage-next and a reference (redb, RocksDB, LMDB) with agreement asserted | Catches behavioral divergence that unit tests miss. |
| **Long-soak duration** | Continuous mutation+retention+recovery run length without failure | Catches leaks, retention debt, recovery state corruption. |
| **Mutation testing kill rate** | Percentage of artificially introduced bugs the test suite catches | Detects test theater (high coverage, low assertiveness). |

The lower-numbered tiers do not require every metric. The metrics ratchet on
as we climb tiers.

## Tier Definitions

### T1 — Foundational (current state, ~1x LOC ratio)

**What it signals:** "Thorough greenfield. Will catch most bugs before
commit. Format won't drift silently. Service contracts are tested."

**Mandatory:**

- ≥1x test-to-production LOC ratio
- Unit tests for every public function
- Golden vector for every stable byte format
- Fuzz target for every public decoder, with at least one seeded corpus entry
  per target
- Property tests for layout / name validation / sort invariants
- Fault-window tests for every service publish step
- Cache-mode absence tests for every durable object family
- Source-vocabulary guard tests against ad-hoc magic-byte construction
- Crash-recovery integration harness with at least one scripted scenario per
  durable service

**Not required at T1:**

- Branch coverage measurement
- Continuous fuzzing
- OOM injection
- Multi-platform CI
- Differential testing
- Mutation testing
- Long-soak runs

**Where storage-next L1-L4 is right now.** Exit criteria for M3 already met
this bar.

### T2 — Hardened (target: ~5x LOC ratio)

**What it signals:** "Won't surprise you under typical production load.
Recovery and fault paths exercised. Branch coverage is measured and gated."

**Mandatory additions over T1:**

- **Branch coverage measured every PR.** llvm-cov or tarpaulin in CI, gate
  starts at 85% on storage-next core code, excluding tests and testkit.
- **Continuous fuzzing.** Each fuzz target gets dedicated compute, on the order
  of hours per target per week. Crashes flow to issues automatically.
- **IO error injection at every L1 backend operation,** not just publish.
  Read, list, metadata, range, append, sync, delete all get fault-injection
  hooks in the conformance harness.
- **OOM injection.** Allocator wrapper that fails the Nth allocation; verify no
  panic, no corruption, no silent loss.
- **Anomaly/crash-point coverage** randomized within each service, not only at
  the documented publish windows. E.g., a crash injected at any line within
  `quarantine_object` or `repair_latest_tail` must leave a recoverable state.
- **Test-density tracking metric published in CI.** Ratio, coverage, fuzz target
  count, fault-window scenario count all reported per build.
- **Windows CI lane.** Even if Windows is not a V1 target, `cfg!(unix)` branches
  must be exercised by tests, not reviewers.
- **Long-soak harness** of at least 1 hour continuous mutation + retention +
  reopen cycles, run nightly. Looks for leaked temp files, leaked locks,
  retention debt growth, recovery time creep.

**What scales to T2:**

- Test-to-production LOC ratio climbs from ~1x to ~5x driven mostly by fault
  injection plumbing, branch-coverage gap filling, and the soak harness state
  machine.
- Goldens grow modestly (new format slices add their own).
- Fuzz corpora grow substantially as continuous fuzzing discovers new
  interesting inputs.

**Approximate timing:** 12-18 months after M3 closes, ideally in parallel with
M5-M8. T2 is the bar before V1 ships to external users.

### T3 — Battle-Tested (target: ~20x LOC ratio)

**What it signals:** "Production-ready for serious workloads. Independently
cross-validated against reference storage engines."

**Mandatory additions over T2:**

- **Differential testing harness.** Same KV workload routed through storage-next
  L9 and through a reference KV (redb for B-tree, RocksDB or fjall for LSM)
  with agreement asserted on results, ordering, and visibility. Differential
  testing is one of the highest-yield bug-finding techniques available.
- **Recipe-driven random workload corpus.** SQLite's SLT-equivalent: a grammar
  that generates random (workload, expected outcome) pairs, run continuously.
  Storage-next's grammar covers branch fork, write, scan, history, fork-at-
  history, retention, recovery, materialization.
- **Multi-day long-soak runs.** The 1-hour T2 soak grows to days. Production-
  representative workload mixtures. Coverage of every L4 service operation
  during the soak. Verified bounded resource usage, no retention debt, no
  recovery time degradation.
- **Mutation testing.** A mutation-testing tool (e.g., `cargo-mutants`) runs
  weekly; kill rate is measured and tracked. Target: 90% kill rate on L3 and
  L4 core code.
- **Concurrency stress with deterministic schedulers.** A pluggable executor
  that explores schedule interleavings systematically (loom or shuttle for
  Rust). Critical for the WAL writer, commit runtime, and lifecycle.
- **Performance regression suite.** Separate from correctness, but required at
  T3 because performance regressions are bugs. See the storage-next benchmark
  plan; T3 implies the bench harness is operational and PR-gated.
- **Cross-version compatibility tests.** Format-v1 bytes produced by today's
  code must be readable by future code. The format-frozen contract demands
  this; T3 enforces it with stored-byte goldens that survive across releases.
- **Branch coverage gate raised to 95%.**

**What scales to T3:**

- LOC ratio 5x → 20x driven mostly by the recipe-driven corpus, the
  differential harness, the soak state machines, and concurrency-stress
  schedulers.

**Approximate timing:** 24-36 months. T3 is the bar before Strata can claim
"production-ready for serious workloads." Aligns roughly with V1.x maturity.

### T4 — Reference-Grade (target: ~100x LOC ratio)

**What it signals:** "SQLite-grade. Bet your business on it."

**Mandatory additions over T3:**

- **100% branch coverage gate** on storage-next core. No exceptions; uncovered
  branches must be deleted, marked unreachable with a typed panic-free
  sentinel, or have a test.
- **OSS-fuzz-equivalent continuous fuzzing compute.** Dedicated infrastructure,
  not opportunistic. Every public decoder and service entry point fuzzed
  continuously at parallelism that competes with the codebase's growth rate.
- **Anomaly testing fires at every syscall, every test iteration.** SQLite's
  approach: every IO operation has a fault counter; tests run in N parallel
  passes, each forcing failure at IO operation k. Storage-next adapts this to
  the backend trait.
- **OOM injection at every allocation site,** not just service-level. Same
  approach as syscall anomaly testing, applied to allocations.
- **Multi-CPU testing matrix.** x86-64, aarch64, riscv64, plus weak-memory-
  ordering hardware (ARM) and strong-memory-ordering (x86). Memory model
  bugs surface on different architectures.
- **Formal model for core invariants.** TLA+ or P specification for: WAL-
  before-visible, snapshot-before-watermark, flush-watermark-before-WAL-
  deletion, active-segment protection. Model-checked against the
  implementation via a refinement layer or property-based testing keyed off
  the model.
- **Reliability statistics over weeks.** Mean time between failure tracked
  continuously across diverse hardware. Published as a number.
- **Anomaly testing for power loss simulation.** Simulated power loss between
  fsync and rename; between rename and parent fsync; etc. Beyond what the
  current publish-window classification covers; reaches into the kernel
  filesystem layer with tools like `dm-flakey` on Linux.

**What scales to T4:**

- LOC ratio 20x → 100x is driven by formal model code, anomaly-injection
  infrastructure across the entire codebase, multi-platform CI parallelism,
  and reliability-stat collection harnesses.

**Approximate timing:** Five years post-V1 minimum. T4 is a destination, not
a near-term goal. Recording it here so the trajectory is named.

## Trust Thresholds

The tiers are not just engineering bookkeeping. They map directly to who
should trust Strata with which kinds of data. This section is the load-bearing
externally-facing claim of the roadmap.

| Tier | Who can trust this | What they're betting | What they accept |
| --- | --- | --- | --- |
| **T1** | Strata engineers, internal dogfooding | Code correctness during development | Surprises under production load. Data loss is plausible. Not for any production use. |
| **T2** | Early adopters, dev tools, ephemeral data, AI workflows where data can be regenerated | Crash-recovery and fault-window correctness under typical load | Edge cases under sustained stress. Concurrency surprises. Performance regressions. Acceptable for non-canonical data. |
| **T3** | Production workloads where dataloss is recoverable from upstream | Behavior under serious workloads, cross-validated against reference engines | Rare anomaly bugs surfaced only under enterprise-scale soak. Multi-CPU memory model issues. Power-loss-mid-syscall edge cases. |
| **T4** | Enterprise, regulated, "bet your business" deployments | Multi-year track record + formal invariants + exhaustive anomaly coverage | The risks that even SQLite carries: undiscovered bugs in cold paths, hardware-firmware misbehavior, edge cases not yet imagined. |

**T2 is the minimum bar before any external user should run Strata.** Below
T2, recovery and fault classification are tested but not exhaustively
characterized; sustained load and platform-specific bugs are likely.

**T3 is the bar for production work.** Below T3, dataloss is statistically
unlikely but not ruled out. T3 is roughly where mature open-source databases
sit at v1.x maturity (PostgreSQL after its first decade; LMDB; redb).

**T4 is the bar for enterprise / regulated / bet-your-business deployments.**
SQLite is at T4. IBM Db2, Oracle, Microsoft SQL Server, and DB2 z/OS are
significantly beyond T4 — their test surfaces span the configuration
cross-product (every OS, every buffer-pool size, every locking mode, every
storage layout, every supported version-to-version upgrade path) over decades
of accumulated regression coverage. That is millions of tests; the scale
exists because each customer deployment historically produced its own
regression. Strata will not reach Db2-scale testing under its current
trajectory; reaching T4 brings it into the same conversation as SQLite, not
the same conversation as a 40-year-old enterprise RDBMS.

**Practical reading:** the trajectory from M3 close (T1) through V1.x to V2
should aim for T2 by V1 ship and T3 within two years of V1. T4 is a
generational target, reachable only with the same kind of sustained
test-investment-as-product discipline that produced SQLite.

## CI Gates Per Tier

| Tier | CI gate |
| --- | --- |
| T1 | Unit + property + fuzz-once + golden conformance + cache-absence + crash-recovery harness must pass on every PR. Workspace dependency-direction guard enforced. |
| T2 | T1 + branch coverage ≥ 85% on storage-next core + Windows CI lane + nightly long-soak (1 hour) + continuous-fuzz crash gate. Test-density metrics published per build. |
| T3 | T2 + branch coverage ≥ 95% + differential-test agreement gate + recipe-driven corpus runs weekly + mutation-test kill rate ≥ 90% + perf regression gate + cross-version goldens enforced. |
| T4 | T3 + branch coverage = 100% + continuous fuzzing on dedicated infra + multi-CPU CI matrix + power-loss simulation gate + formal-model refinement check on every PR + reliability-stat reporting. |

Each gate is **cumulative**. T2 must satisfy T1, T3 must satisfy T2, T4
must satisfy T3.

## What This Roadmap Will Not Do, Ever

A few things stay explicitly out of scope even at T4:

1. **Hand-written assembly-level testing.** Not our codebase.
2. **Hardware-in-the-loop testing of storage media.** Real-device behavior
   (SSD garbage collection, MLC retention) is the responsibility of the
   filesystem and hardware vendors. We test against the kernel filesystem
   contract.
3. **Property-based testing as a substitute for goldens.** Goldens lock
   bytes. Properties prove invariants. Both required; neither substitutes
   for the other.
4. **Test-LOC parity for purely declarative code** (e.g., the error-enum
   definitions). LOC ratio is a *coarse* metric; declarative code dilutes it
   without changing real coverage.

## Adoption Beyond Storage-Next

Each Strata crate runs its own test density assessment when it reaches
M3-equivalent maturity. The tier definitions in this doc are intended as a
template:

- **core-next** is small enough that T2 should already be achievable.
- **engine-next** will likely sit at T1 through M5-M6, with a stretch goal
  toward T2 before M9 cutover. Engine cross-validation against intelligence
  and inference belongs in T3.
- **intelligence-next** has different testing concerns (stochastic outputs,
  model-dependent reranking, prompt-injection coverage) that need a separate
  tier definition. This roadmap is the structural template; the categories
  differ.
- **inference-next** lives outside the storage substrate but should adopt the
  same fuzz-target / continuous-fuzz / mutation-test discipline once provider
  trait surfaces stabilize.

A crate-wide test density tracker would publish a per-crate tier with the
metric values for each. Tracking that lives outside this document.

## How To Track Progress

Two simple recurring artifacts:

1. **A storage-next test-density dashboard** updated nightly: LOC ratio,
   branch coverage, fuzz target count + corpus size, distinct property
   invariants, distinct fault-window scenarios, soak runtime, mutation kill
   rate (when T3+), differential-test agreement rate (T3+). Published as a
   stable artifact, diffable across releases.

2. **A quarterly tier review** that asks: which tier are we at, what concrete
   work is needed to climb to the next tier, and is the trajectory matching
   the roadmap. The output is a one-page status update in the V1 progress
   tracker.

## Why This Document Exists

The phrase "reference-grade" was used in the M3 audit and was wrong. SQLite
is reference-grade. We are not. Saying we are makes future engineers think
the work is done when it has barely begun.

This roadmap converts that overconfidence into a multi-year discipline. Each
tier is a real bar with measurable, gateable infrastructure. Climbing tiers
is the difference between "ships and works" and "billions of devices, twenty
years, no data loss."

We are at T1. The honest path to T4 takes a decade. Track it.
