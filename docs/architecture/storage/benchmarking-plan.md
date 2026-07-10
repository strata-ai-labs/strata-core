# Storage-Next Benchmarking Plan

Status: V1 draft, lands during M9F and M10D

Depends on:

- [L1. Backend IO](./l1-backend-io.md)
- [L2. Object Layout](./l2-object-layout.md)
- [L3. Durable Format / Codec](./l3-durable-format-codec.md)
- [L4. Log / Manifest / Snapshot Services](./l4-log-manifest-snapshot-services.md)
- [L5. Table Runtime](./l5-table-runtime.md)
- [L6. Branch-Isolated LSM Runtime](./l6-branch-isolated-lsm-runtime.md)
- [L7. Commit Runtime](./l7-commit-runtime.md)
- [L8. Lifecycle / Recovery / Maintenance](./l8-lifecycle-recovery-maintenance.md)
- [L9. Storage API Boundary](./l9-storage-api-boundary.md)

## Purpose

This plan defines how storage-next is benchmarked end-to-end and layer-by-layer.

The point of the plan is not to make storage-next "win" comparisons against
other KV stores. It is to:

1. Produce absolute baselines for every L9 surface across cache, durable local
   `standard`, and durable local `always` modes.
2. Produce comparative numbers against representative KV stores at every scale
   so engine and primitive overhead can be reasoned about with real data.
3. Produce a per-operation, per-layer cost attribution so we know exactly where
   time is spent in L1-L9 at each scale.
4. Catch performance regressions on PRs, nightly runs, and at release.

The plan tracks the V1 roadmap. Benchmarking infrastructure is scoped to land
during M9F and M10D, after storage-next is functionally complete.

## Goals

1. Storage-next baselines for every L9 surface (commit, read latest, read by
   version, read by timestamp, scan, history, branch fork, materialize, close,
   recovery) across cache and durable local modes.
2. Comparative numbers at the same workloads against redb, fjall, RocksDB, and
   LMDB. Apples-to-apples wherever feasible; apples-to-oranges with explicit
   disclaimers where storage-next does something the competitor does not.
3. Layer-by-layer percentage of latency spent in L1-L9 per operation, at each
   scale.
4. **Comprehensive branching characterization.** Branching is Strata's unique
   value proposition. The headline measurement is fork latency as a function of
   source-branch dataset size, which must be flat. The full branching suite
   covers fork primitives, inherited read paths, hierarchy shapes,
   materialization variants, branch lifecycle pressure, branch-aware
   compaction safety, real-world composite patterns, and recovery with branch
   forests up to 100K branches.
5. Engine + primitive overhead quantification once the same harness runs through
   engine-next in M10D.
6. Stable regression-tracking infrastructure that survives M9 cutover.

## Non-Goals

This plan deliberately excludes:

1. Object-store / OpenDAL / S3 benchmarks. Object durable mode is not V1.
2. Browser/cache backend benchmarks. The browser path runs the same harness in
   principle but needs a separate WASM-shaped runner.
3. Multi-process / IPC benchmarks. IPC is engine-next territory and arrives
   after the storage layer is stable.
4. Engine-next / intelligence-next / inference-next workloads in this plan.
   Engine and primitive overhead measurement is **the payoff** of the plan, but
   the engine driver lands as a follow-on milestone (M10D) using the same
   harness rather than a new one.
5. Workload replay against real customer corpora. That belongs post-V1 once
   StrataHub or operator tooling produces representative captures.

## Scope

The harness benchmarks the L9 surface only. No driver under test calls below
L9 in storage-next. Competitor drivers call the competitor's equivalent public
API.

In scope storage-next modes:

- cache (no durability claim)
- durable local `standard`
- durable local `always`

Out of scope storage-next modes:

- object durable candidate
- any backend that has not declared sufficient capability for the requested
  storage mode

## Workload Taxonomy

Workloads are YCSB-inspired but Strata-shaped. Each workload runs at every
scale tier and in every durability mode where it is defined. All workloads use
fixed seeds.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `load-seq` | Bulk insert sequential keys | WAL throughput, flush, L0->L1 compaction |
| `load-rand` | Bulk insert random keys | WAL throughput, memtable contention, write amp |
| `point-latest` | Latest read by random key | Memtable, block cache, bloom, table seek |
| `point-version` | `getv` at a random retained version | MVCC chain walk, version-bounded seek |
| `point-timestamp` | `as_of` at a random retained timestamp | Timeline substrate, MVCC walk |
| `scan-prefix` | Prefix scan with limit N | Merge cursor, inherited-layer rewriting |
| `scan-range` | Range scan with limit N | Merge cursor under range bounds |
| `history` | Per-key history with limit N | Row-chain scan in descending version order |
| `mixed-95r5w` | 95% latest read / 5% put | Hot read path with light commit |
| `mixed-50r50w` | 50% / 50% | Sustained durability + compaction debt |
| `update-heavy` | Many versions of a small key set | Tombstone + version pruning, compaction pressure |
| `delete-sweep` | Insert N, delete subset | Tombstone safety, compaction-elision rules |
| `recovery-replay` | Crash mid-write, measure recovery time | L8 recovery, WAL replay throughput |
| `checkpoint-during-load` | Periodic checkpoints during `load-rand` | Commit quiesce cost, snapshot publish |
| `sustained-1h` | One-hour `mixed-50r50w` at target rate | Tail latency, compaction steady state, L0 watermark |

Branching workloads have their own dedicated suite. See
[Branching Benchmark Suite](#branching-benchmark-suite) below. They run with no
competitor analog and produce absolute + trend numbers, not comparative ones.

## Scale Matrix

Each cell is keys x value-size, fixed seed. Value-size profiles:

- 64 B (KV-style baseline)
- 1 KB (typical JSON-shaped row)
- 16 KB (large blob)

All three value sizes run for scales 100K through 10M. Scales 50M and above
run 64 B and 1 KB only, to control runtime and disk footprint.

| Scale | What it stresses | Approx 64 B footprint |
| --- | --- | --- |
| 100K | Purely in-memory | 6 MB |
| 1M | Single-segment territory | 64 MB |
| 10M | Multi-level, fits in RAM | 640 MB |
| 50M | Disk pressure starts | 3.2 GB |
| 100M | Working set vs cache | 6.4 GB |
| 500M | Dataset >> RAM, cold reads dominate | 32 GB |
| 1B | Compaction debt, space amp, recovery time | 64 GB |

The point of the sweep is not only "does it scale." It is that the dominant
layer changes with scale. At small scale, CPU work in L3 / L6 / L7 dominates.
At medium scale, L4 publish, L5 block cache, and L8 compaction scheduling
appear. At large scale, working-set vs cache, write amp, space amp, and
recovery time dominate. The shifting layer attribution is itself a published
result.

## Branching Benchmark Suite

Branching is Strata's unique value proposition. No competitor in the comparison
set has an equivalent. That makes the branching numbers absolute rather than
comparative, and it raises the bar for how thoroughly we characterize them.
Branching must be measured as carefully as the primary KV path, because
"branching is cheap" is a load-bearing product claim and we need the data to
defend it.

This suite has its own scale matrix, its own workloads, and its own metrics.
It runs in all durability modes. It runs at every key-scale tier (100K through
1B) where the operation is meaningful, and it adds a separate branch-count
scale axis on top.

### Branch-Count Scale Matrix

Branching scales on a different axis than the primary key matrix. The
branch-count matrix is orthogonal: at each (key-scale, depth, pattern) point
the harness produces an independent result.

| Branches | Depth | Pattern | Why |
| --- | --- | --- | --- |
| 10 | 1-3 | Typical user | Common case; defines absolute baseline |
| 100 | 1-5 | Active project | Realistic AI workflow density |
| 1K | 1-10 | Heavy multi-experiment | Stress the shared-table refcount machinery |
| 10K | 1-20 | Extreme | Catches manifest growth and recovery cost |
| 100K | 1-20 | Saturation | Confirms the forest does not collapse |

Branch-count and key-scale combine multiplicatively only where both axes are
informative. A 100K-branch x 1B-key cell is not in the V1 sweep; a
100K-branch x 1M-key cell is, because that is where branch metadata cost
becomes the dominant factor.

### Branch Operation Primitives

These workloads measure the storage cost of each branch-lifecycle operation in
isolation.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `branch-create-empty` | Empty-branch creation rate | L6 branch state allocation, L4 reachability publish |
| `branch-fork-cold` | Fork from a quiescent source | Inherited-layer construction, applied-max-version capture |
| `branch-fork-hot` | Fork while source receives writes at rate R | Commit quiesce vs fork contention, applied vs allocated version safety |
| `branch-fork-at-history-recent` | Fork at version V within last 1% of retained history | Retained-history proof on the hot path |
| `branch-fork-at-history-edge` | Fork at the retention boundary | Retention vs fork race; correctness of "history available" check |
| `branch-fork-at-history-miss` | Fork past the retention boundary | Typed-error latency, not throughput |
| `branch-delete-leaf` | Delete a branch with no descendants | Reachability release, shared-table protection |
| `branch-delete-heavy` | Delete a branch with N inherited layers and M shared tables | Refcount work proportional to shared-table reachability |
| `branch-clear` | Clear branch state, keep the ID | Mutable + immutable state teardown without ID release |

Per-primitive metrics are documented below; fork latency in particular is
broken out by source-branch dataset size, to validate that fork cost is
proportional to metadata and inherited-table count, **not** dataset size.

### Inherited Read Paths

Inherited reads are the COW hot path. They must be fast, predictable, and
independent of source-branch dataset size beyond cache effects.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `inherit-point-pure` | Point reads on inherited rows only, no child shadowing | Inherited-layer descent, fork-version gate |
| `inherit-point-shadowed-50` | 50% of reads hit child shadowing | Read order: own mutable -> own immutable -> inherited |
| `inherit-point-shadowed-95` | 95% of reads hit child shadowing | Confirms shadowing short-circuits cheaply |
| `inherit-point-tombstone-shadow` | Child tombstones hide inherited values | Tombstone safety on the hot path |
| `inherit-scan-merged` | Scan over mixed own + inherited rows | Merge cursor + inherited-key rewriting |
| `inherit-scan-key-rewrite` | Scan stress with maximum rewrite work | Isolates the key-rewriting cost itself |
| `inherit-getv-cross-fork` | `getv` for a version range that straddles the fork point | Fork-version gate vs requested visibility |
| `inherit-as-of-cross-fork` | `as_of` for a timestamp that straddles the fork point | Timeline substrate against inherited rows |
| `inherit-history-cross-fork` | History over a key whose chain straddles the fork point | Per-key chain traversal across the fork boundary |

Each runs at the branch-count scale matrix above and at three inheritance
depths (1, 5, 20). The depth axis is critical: read latency must remain
bounded as depth grows, and the data tells us where the practical ceiling is.

### Hierarchy Shape Coverage

Branch topology has its own performance surface.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `hierarchy-chain-1` | One parent, one child, point reads on child | Single-layer descent baseline |
| `hierarchy-chain-5` | Chain of depth 5 | Linear descent cost |
| `hierarchy-chain-10` | Chain of depth 10 | Confirms descent stays bounded |
| `hierarchy-chain-20` | Chain of depth 20 | Stress; informs auto-materialization threshold |
| `hierarchy-fanout-10` | One parent, 10 children, reads on each | Shared-table refcount under fan-out |
| `hierarchy-fanout-100` | One parent, 100 children | Confirms fan-out cost is proportional to children, not data |
| `hierarchy-fanout-1000` | One parent, 1000 children | Manifest size, refcount registry rebuild |
| `hierarchy-forest-10x10` | 10 parents x 10 children each | Mixed shape |
| `hierarchy-forest-100x100` | 100 parents x 100 children | Realistic AI-workflow forest |

The chain workloads inform when L8 should schedule auto-materialization. If
chain depth 20 reads are 5x slower than depth 1, the threshold lives somewhere
below 20.

### Materialization Variants

Materialization changes physical ownership without changing read results. The
suite must prove that under load, and characterize cost across shapes.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `materialize-thin` | Small inherited layer, low child shadowing | Baseline materialization cost |
| `materialize-deep` | Large inherited layer, low shadowing | Throughput, table-build scaling |
| `materialize-shadowed` | Large inherited layer, heavy shadowing | "Collect inherited rows still visible" cost |
| `materialize-chain` | Materialize one layer when chained ancestry exists | Correct visibility across remaining layers |
| `materialize-under-write` | Child receives writes during materialization | Pinned read views + branch-state transition safety |
| `materialize-under-read` | Readers in flight during materialization | Reader sees old view or new view, never partial |
| `materialize-crash-mid` | Crash mid-materialization | Recovery distinguishes "intended" / "published" / "installed" |
| `materialize-vs-fork` | Concurrent fork off the child during materialization | Shared-table protection during the transition |

Materialization workloads emit the materialization wall time, the
preflight-vs-publish-vs-install split, and pinned-read-view counts.

### Branch Lifecycle Pressure

Sustained branch creation, churn, and coexistence.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `branch-churn` | Sustained fork + delete at rate R for one hour | Steady-state refcount churn, manifest pressure |
| `branch-many-coexist` | N concurrent branches, light per-branch writes | Independence of per-branch commit paths |
| `branch-many-shared` | N branches sharing M tables, no writes | Shared-table reachability machinery |
| `branch-write-fanout` | One parent, writes routed to N children | Confirms commits on siblings do not contend |

Independence is the key result: per-branch commit throughput should scale
linearly with branch count up to the rig's IO ceiling, because branch commit
locks are per-branch.

### Branch-Aware Correctness And Compaction

These workloads test that branching's correctness invariants do not silently
degrade performance.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `compact-with-inherited-tombstones` | Compaction under child tombstones hiding inherited values | Tombstone safety rule cost |
| `compact-shared-table-no-early-reclaim` | Compaction with shared tables still referenced | Reachability proof in retention |
| `compact-tombstone-elision-safe` | Compaction once safety proof permits elision | Throughput in the elide-allowed regime |
| `retention-with-many-branches` | Retention pass under N coexisting branches | Reachability proof generation time |
| `ttl-cross-inheritance` | TTL expiry across own + inherited rows | Consistency of TTL evaluation across the chain |

The TTL workload exercises the conformance scenario from the L6 doc
(parent writes `k` v10 TTL@50; child forks at v20; reads at t40 and t60) as a
runnable benchmark, not only a correctness test.

### Real-World-Shaped Composite Workloads

These compose primitives into the shapes Strata actually expects from users.
Less precise than the primitives, but the ones we care about most for product
claims.

| Workload | Description |
| --- | --- |
| `pattern-notebook` | Many short-lived child branches, half merged back via the engine, half discarded. Models Jupyter / interactive AI usage. |
| `pattern-long-feature` | N long-lived branches each carrying a 10-30% delta from parent. Models long-running experiments. |
| `pattern-audit` | Read-only historical forks, no writes. Models compliance / debugging. |
| `pattern-ab-siblings` | Two siblings of one parent, balanced read + write on both. Models A/B experiments. |
| `pattern-forest` | Wide-fanout tree mimicking a team's branch sprawl. |

These run at the smaller end of the key-scale matrix (1M - 100M) where
behavior matters most. They produce throughput + p99 latency numbers and a
"branches per second the system can sustain" metric.

### Branch Recovery

Recovery time has to remain bounded as the branch forest grows.

| Workload | Description | Primarily stresses |
| --- | --- | --- |
| `recovery-N-branches` | Open with N branches at depth D | Manifest replay + refcount registry rebuild |
| `recovery-interrupted-fork` | Crash mid-fork before destination visible | Recovery proves no half-visible branch |
| `recovery-interrupted-materialization` | Crash mid-materialization | Recovery reaches the documented state distinctions |
| `recovery-with-quarantine-debt` | Open with pending quarantine inventory | Quarantine reconciliation at scale |

`recovery-N-branches` runs across the branch-count matrix and produces
recovery wall time as a function of branch count, depth, and shared-table
count. This is one of the most product-load-bearing numbers in the entire
plan.

### Branch-Specific Metrics

The primary metrics still apply (throughput, latency tail, etc.). Branching
adds these:

- **Disk footprint per fork.** Target: independent of source dataset size,
  proportional only to inherited-table-reference count and per-branch manifest
  overhead.
- **Refcount memory per fork.** Target: bounded constant per shared-table
  reference.
- **Manifest growth per fork.** Reported in bytes; trend-tracked across
  releases.
- **Shared-table refcount registry rebuild time.** Measured on recovery as a
  function of (branches, shared-tables).
- **Pinned-read-view count during materialization.** Confirms readers are not
  starved.
- **Materialization wall time vs inherited-layer size.** Expected to be linear
  in inherited rows after shadowing; the harness publishes the constant.
- **Materialization throughput in rows/sec.** Comparable across runs.
- **Fork latency by source-branch dataset size.** This is the headline number.
  It must be flat across source sizes from 100K to 1B keys.

### What This Suite Is For

Three things, in order:

1. Defend the product claim that **fork cost is independent of source dataset
   size**. The headline metric is fork latency at 100K, 1M, ..., 1B source
   keys. If the curve isn't flat, branching isn't what we say it is.
2. Establish **bounded inherited-read overhead**. Reads through inherited
   layers must remain predictable; auto-materialization thresholds derive from
   the depth/latency curve we measure here.
3. Quantify the cost of **the things only Strata does** (materialize, fork at
   history, delete with refcount work, recovery with branch forests) so engine
   and product layers can make informed scheduling decisions on top.

### Phasing For Branching

The branching suite tracks the primary phasing but pulls some work forward
because the claims depend on it landing earlier than other comparative work.

| Phase | What lands |
| --- | --- |
| Phase 1 (M5/M6) | Branch operation primitives at 10 / 100 branches, depths 1 - 5. Inherited point reads. Headline fork-latency-by-source-size measurement. |
| Phase 2 (M9F) | Hierarchy coverage, materialization variants, branch lifecycle pressure. Full branch-count matrix up to 1K. |
| Phase 3 (M9F - M10D) | Branch-aware correctness/compaction workloads, real-world composite patterns, recovery scaling to 100K branches. |
| Phase 4 (M10D) | Engine-level branching workloads, once engine-next exposes its branch surface. |

The Phase 1 fork-latency-by-source-size curve is a release-gating result. If
that curve is not flat, branching is broken and the rest of the suite is
deferred until it is.

## Comparative Competitors

| Store | Why chosen | Documented semantic differences |
| --- | --- | --- |
| redb | User-named baseline; pure-Rust, B-tree, append-only MVCC | No LSM, no branching, no retained history, single-writer |
| fjall | Pure-Rust LSM, closest design neighbor | No branching, no retained version queries |
| RocksDB | LSM gold standard, mature compaction, calibrates write amp | C++, no branching, schema-free |
| LMDB | Read-optimized mmap B-tree, calibrates point-read ceiling | Single-writer, no MVCC versioning |

Every comparison report opens with an explicit semantic-difference disclaimer.
Where it is meaningful, we also publish a `storage-next-minimal` configuration:
single root branch, no forks, version queries disabled at the harness level.
That is the closest the comparison can get to apples-to-apples and remains
useful even when the competitor offers no equivalent semantics.

Not included: bbolt and SQLite (different problem class), sled (less actively
maintained), object stores (not V1).

None of these stores have a branching primitive. The full branching benchmark
suite produces absolute and trend numbers only. Where a competitor can model a
branching pattern by hand (e.g., snapshot-and-fork-by-copy), the harness must
not paper over the cost difference; the disclaimer makes the asymmetry
explicit and the competitor result records the bytes copied alongside the
wall time.

## Layer-By-Layer Methodology

Three complementary techniques.

### A. Always-on per-layer spans (primary)

Each L1-L9 layer wraps its public entry points with a lightweight timing span
(cycle counter, not the tracing crate). Spans are gated behind a
`bench-instrumentation` cargo feature. Production builds carry zero cost.

Target span overhead: under 50 ns per span at fixed-cycle CPU. The
uninstrumented vs instrumented delta is itself measured and published; if
overhead is material at hot scales, the harness moves to sampled instrumentation
(one in N operations) and reports both modes.

Output per (workload x scale x mode) cell is a per-operation breakdown:

```text
commit (durable local always, single put, 64 B):
  L9 arg validation         0.05 us  ( 2%)
  L7 validate + version     0.15 us  ( 6%)
  L3 encode commit payload  0.20 us  ( 8%)
  L4 WAL append             0.50 us  (20%)
  L4 WAL fsync (always)     1.20 us  (47%)
  L6 install rows           0.30 us  (12%)
  L7 publish visible        0.10 us  ( 4%)
  other                     0.05 us  ( 1%)
                            -------
                            2.55 us
```

Each L9 surface gets one such table per workload, scale, and durability mode.

### B. Layer-replacement (synthetic isolation)

The harness ships `bench-null` replacements for selected layers:

- null L1: `memcpy`-only backend, zero IO
- null L3 codec: identity passthrough, no CRC, no length validation
- null L4 WAL: accepts append, never writes
- flat L6: single branch, no inherited layers, latest-only MVCC selection

Running the same workload with progressively-replaced layers yields a delta
that distinguishes layer cost from layer contribution to correctness. This
matters when interpreting (A): a layer can be hot because the work is
mandatory, or because the implementation is unoptimized. (B) tells them apart.

These nulls are bench-only artifacts. They do not become production fallbacks
and are not exposed through L9.

### C. System-level profiling (sanity)

`perf` on Linux and `Instruments` on macOS produce flamegraphs for
representative workloads. Mostly a safety net: catches allocator hot spots,
lock contention, syscall overhead that the span instrumentation does not
capture directly.

### Reporting

For each (workload x scale x mode) cell:

- Stacked bar chart of per-layer percentage
- Numeric table of absolute microseconds per layer
- Delta against the prior tracked measurement (regression tracking)
- Flamegraph SVG as artifact, not in the inline report

## Metrics

Every run records:

- Throughput: sustained ops/sec, measured over the last 5 minutes of run length
- Latency: p50, p95, p99, p99.9, max
- Tail: p99.9 over 5-second buckets across the run, to surface stalls
- Read amplification: bytes read per logical read
- Write amplification: bytes written per logical write
- Space amplification: bytes on disk per logical bytes
- Memory: peak RSS, steady RSS
- Compaction CPU: percent of wall time in background compaction
- WAL pressure: bytes, segment count, sync-window distribution
- Recovery time: wall clock to reach `Open` from cold start
- L0 count over time: sampled every second to characterize backpressure regimes

Storage-next-specific facts also recorded when applicable:

- Inherited-layer depth distribution
- Materialization wall time
- Quarantine debt
- Retention debt
- Pinned read view count

## Environment And Reproducibility

A reference rig spec is committed alongside the plan. Canonical numbers come
from that rig. Spec includes:

- CPU model, core count, frequency policy
- RAM size and DIMM configuration
- NVMe model, firmware, and filesystem
- Kernel / OS version
- BIOS settings relevant to performance (turbo, C-states, hyperthreading)

Run pinning during benchmarks:

- CPU governor `performance`
- Swap disabled
- `ulimit` raised for open files
- Transparent hugepages pinned to a known state
- Cold-cache reads enforced via filesystem cache drop or `O_DIRECT` where the
  backend supports it

Repeatability:

- N = 5 runs per cell
- Geometric mean published
- IQR shown for variance
- Fixed key/value generation seeds committed to the harness
- Per-run `run.toml` snapshotting hardware identity, code SHA, rustc version,
  kernel, filesystem, and any tunables in effect

Artifacts per run:

- CSV and JSON results
- Flamegraph SVG when profiling is on
- Layer span dump when `bench-instrumentation` is on

## Implementation Shape

New crate: `crates/storage-next-bench/`.

The bench crate is separate from `crates/storage-next/` so its dependency
graph (criterion, plotters, competitor SDKs) does not poison the lib's
feature surface.

Crate layout:

- `driver-trait` - generic KV driver trait: open, commit, read latest, read by
  version, read by timestamp, scan, history, branch_fork, materialize, close.
- `driver-strata` - implements the trait over storage-next's L9.
- `driver-redb`, `driver-fjall`, `driver-rocksdb`, `driver-lmdb` - each behind
  a cargo feature so a single binary can be built per competitor.
- `workloads` - workload definitions parameterized over the driver trait.
- `runner` - orchestrates a (driver x workload x scale x mode) cell, owns
  timing, span collection, and run artifacts.
- `report` - emits CSV/JSON, renders comparison plots via plotters.

Tooling:

- `criterion` for micro-benchmarks at single-operation granularity.
- A custom long-run harness for sustained and large-scale workloads. Criterion
  is unsuitable for multi-minute and multi-hour runs.

Instrumentation:

- Layer spans live in the storage-next crates themselves, gated by
  `--features bench-instrumentation`. The bench harness flips the feature on
  when invoking storage-next.
- Span data is collected via a low-overhead in-process ring buffer drained at
  end of run. No tracing-crate dependency on the storage-next hot path.

Output format:

- Stable schema across runs so historical comparison plots are trivial.
- A single `results/` tree per run, suitable for pushing to a benchmark
  artifact store later if we want web-rendered tracking.

## Phasing

The plan tracks the roadmap. Each phase has a clear entry condition.

| Phase | When | What |
| --- | --- | --- |
| Phase 0 | During M3 / M4 (in progress) | Per-layer micro-benchmarks already attached to slice plans (L3 codec roundtrip, L5 table seek, L4 publish). Do not duplicate. |
| Phase 1 | M5 / M6 (L9 surface is real) | Build the bench crate, the driver trait, and the storage-next driver. Run end-to-end at 100K / 1M / 10M for durable local modes. **Land the headline fork-latency-by-source-size measurement.** No competitors yet. Validate layer-span instrumentation overhead. |
| Phase 2 | M9F | Add redb and fjall drivers at all scales up to 100M. Land layer attribution reports. Land hierarchy + materialization + branch-lifecycle suites at up to 1K branches. |
| Phase 3 | M9F to M10D | Add RocksDB and LMDB drivers. Add 500M and 1B scale runs. Add sustained, recovery, branch-aware compaction, real-world composite, and 100K-branch recovery workloads. |
| Phase 4 | M10D | Engine-next driver added to the same harness. Same workloads run through engine-next's KV primitive surface. Delta against storage-next quantifies engine + primitive overhead. This is the point of the plan. |

Phase 4 is intentionally deferred. Storage-next must be fast and well
characterized before any engine overhead measurement is meaningful.

## Execution Cadence

Different scales run at different cadences.

| Tier | Approx duration | Coverage | When |
| --- | --- | --- | --- |
| PR-fast | 5 minutes | 100K, point-latest + load-rand + commit + `branch-fork-cold`, cache + durable-standard | Every PR; gates with documented thresholds |
| Nightly | 3 hours | All workloads at 100K / 1M / 10M, all modes, storage-next + redb; **fork-latency-by-source-size headline curve at 100K / 1M / 10M / 100M source keys** | Nightly schedule; tracks tail latency + fork-latency flatness |
| Weekly | 24 hours | All workloads up to 500M, all competitors that support the scale; full branching suite up to 1K branches | Weekly schedule, also pre-release |
| Quarterly | 3-5 days | 1B sweep, all workloads, all competitors; branching suite at 10K / 100K branches | Manual trigger only; produces canonical published numbers |

The 1B sweep is genuinely expensive. A single 1B random load at 500K ops/sec
is roughly 33 minutes, and the full workload set at 1B is multiple days. It is
opt-in by design.

## Regression Policy

PR-fast tier compares against a rolling baseline. Acceptable variance per metric
is defined in the bench crate config, not in this plan. Initial thresholds
must be derived from real variance data, not chosen by intuition.

Nightly and weekly tiers track absolute numbers and produce a trend report.
Regression of more than a documented multiple of historical IQR opens an
issue automatically.

The 1B sweep does not gate. Its results are published as canonical numbers and
tracked over major releases.

## Risks And Open Questions

1. Layer instrumentation overhead. Even at 50 ns per span, on the hot path it
   may distort percentages. Validate the instrumented vs uninstrumented delta
   in Phase 1 and accept, sample, or restructure as needed.
2. Reference rig drift. Numbers move with OS upgrades, NVMe wear, and BIOS
   tuning. The rig spec needs to be a real, maintained document.
3. Apples-to-apples disclaimers. The semantic differences between storage-next
   and competitors will be misread. The report header must be explicit.
4. Disk budget for the 1B scale. 64 GB times value-size multiplier times
   competitors times runs is a meaningful storage commitment.
5. Branching has no comparator. Branch workloads ship absolute numbers only.
   The report must not imply a competitive ranking it cannot defend.
6. Open: criterion vs custom harness boundary. Criterion handles micro
   beautifully. We need a clear cutover at the operation duration where the
   custom harness takes over.
7. Open: should sampled layer instrumentation become the default to avoid the
   instrumentation overhead question entirely?
8. Open: what is the minimum span count per workload that gives statistically
   meaningful per-layer percentages?
9. Open: where do the canonical published numbers live - in the repo, in a
   bench-artifact store, or both?
10. Open: how is "fork latency curve must be flat" formalized as a regression
    gate? A flatness tolerance over the source-size axis, or a hard ceiling at
    each cell?
11. Open: should auto-materialization thresholds be derived from the
    hierarchy-chain workloads automatically, or chosen by hand and validated
    against them?
12. Open: at what branch count does the harness switch from full enumeration
    of branch IDs to sampled coverage? 10K branches enumerated at depth 20 is
    expensive on its own.
13. Open: do the real-world composite patterns belong in the storage bench, or
    do they move to a separate engine-level bench in M10D where they can
    exercise the actual product API shapes?

## V1 Minimum

The first storage-next bench harness needs:

1. `crates/storage-next-bench/` with driver trait and storage-next driver.
2. redb driver behind a feature flag.
3. `bench-instrumentation` feature in storage-next crates with L1-L9 span hooks.
4. Core KV workloads: `load-seq`, `load-rand`, `point-latest`, `point-version`,
   `scan-prefix`, `history`, `mixed-50r50w`, `recovery-replay`.
5. **Branching V1 subset:**
   - `branch-create-empty`, `branch-fork-cold`, `branch-delete-leaf`
   - `inherit-point-pure`, `inherit-point-shadowed-50`,
     `inherit-scan-merged`
   - `materialize-thin`, `materialize-deep`
   - `hierarchy-chain-1`, `-5`, `-10`; `hierarchy-fanout-10`, `-100`
   - `recovery-N-branches` at 10 / 100 / 1K
   - **Headline:** fork-latency-by-source-size across 100K / 1M / 10M / 100M
     source keys, run on every nightly
6. Scales: 100K, 1M, 10M, 100M (primary KV); branch-count matrix to 1K.
7. Durability modes: cache, durable local `standard`, durable local `always`.
8. Per-layer reporting in CSV + JSON.
9. PR-fast and nightly tiers wired up.
10. Reference rig spec committed.

V1 does not require:

1. RocksDB / LMDB / fjall drivers (Phase 3).
2. 500M and 1B scale runs (Phase 3).
3. Branch-count scale matrix beyond 1K (Phase 3).
4. Real-world composite branching patterns (Phase 3).
5. Engine-next driver (Phase 4).
6. Object-store benchmarks.
7. Browser benchmarks.
8. Cross-machine or multi-process runs.

## Implementation Notes

A few constraints worth pinning before the first slice opens.

The harness must not call below L9 in storage-next. The whole point of the
plan is to characterize what L9 consumers pay. Reaching past L9 produces
numbers that lie.

Drivers must declare the storage modes they support. Running an `always`
durability workload against a competitor that has no equivalent must produce
a typed unsupported result, not silently fall through to a weaker guarantee.

Benchmark output must be diffable. CSV/JSON schemas are part of the
contract; column reordering or column renames are breaking changes.

Workload definitions must be deterministic given a seed. Adding randomness or
wall-clock-driven decisions inside a workload breaks regression tracking.

## Next Step

After this plan is reviewed:

1. Open the M9F slice that builds `crates/storage-next-bench/` and the
   storage-next driver.
2. Add the `bench-instrumentation` feature surface to storage-next crates in
   the same slice or the slice immediately following.
3. Reference rig spec committed to `docs/architecture/storage/bench-rig.md`
   once the hardware is procured.
4. The engine-next overhead measurement (Phase 4) opens a slice in M10D after
   the storage harness is stable.
