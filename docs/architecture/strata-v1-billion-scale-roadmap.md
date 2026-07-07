# Strata V1 Billion-Scale Performance Roadmap (M12)

**Status:** Planning draft
**Milestone:** `M12` — Billion-Scale Performance
**Target:** Close the durable-engine gap to RocksDB and sustain billion-record
scale (1B keys × ~1 KB ≈ 1 TB on disk) across load, point-read, scan, and space,
without regressing cache-mode reads or the COW branching model.

This roadmap defines a dedicated performance-hardening milestone for the V1
stack. It is the V1 successor to the previous engine's billion-scale effort,
which **succeeded on the performance axis** (it reached billion-record scale)
even though that engine's code quality forced the V1 rewrite. The previous
effort is preserved under `docs/architecture/previous-work/`; this document
internalizes its lessons rather than repeating its plan.

> **Evidence update (2026-06-22) — priorities revised before any implementation.**
> Per the previous engine's hardest lesson (*instrument before theorizing*), the
> durable path was measured before writing a fix. The measurement
> (`docs/design/performance/durable-load-amplification-evidence.md`) **refuted the
> original lead hypothesis** (`#3a`, count-based over-compaction): nonzero levels
> are compacted **4–8× over** their byte target, not under it. The real, measured
> root cause is a **single-threaded maintenance pipeline that cannot keep up
> (RC1)** — coalescing away 99.4% of the maintenance it needs — which **starves
> obsolete-table reclaim (SA2)** (93% dead tables on a single branch) and forces
> late, giant, repeated compactions (**≈15× write-amp, 16× space-amp at 5M**). The
> milestone now leads with **maintenance throughput (M12C)** and **eager reclaim
> (M12B)**; the byte-score trigger (M12A) is demoted. Sections 4–7 reflect this.

---

## 1. Purpose And Relationship To Other Documents

The implementation roadmap (`docs/architecture/strata-v1-implementation-roadmap.md`)
takes the architecture to *functional completeness* (M0–M11). It is correctness-
and boundary-driven. It deliberately defers performance: benches are re-baselined
only in M9F/M10D, and M11 readiness merely "compares against threshold policy."

**M12 is where Strata earns its billion-scale numbers.** It is a performance
track, not a feature track. It does not add product surface, does not change the
public engine API, and changes the durable on-disk format only through a gated,
deliberate re-freeze (see M12I). It revisits storage-next and engine-next
*internals* to meet the non-functional requirements at scale.

Authoritative inputs:

| Concern | Design document |
|---|---|
| Durable write pipeline root causes (RC1/RC2/RC3) | `docs/architecture/storage-next/durable-write-pipeline-scaling.md` |
| Single-commit write cliff | `docs/architecture/storage-next/durable-single-commit-write-cliff.md` |
| Compaction problem framing (I/O, convergence, branch GC) | `docs/design/performance/compaction-architecture-brief.md` |
| Compaction I/O starvation under read load | `docs/design/performance/compaction-io-starvation.md` |
| Compaction + COW branching design space | `docs/design/performance/compaction-research.md` |
| Full four-phase compaction plan (reference) | `docs/design/performance/compaction-scheduling-plan.md` |
| Pragmatic two-step compaction plan (current) | `docs/design/performance/simplified-compaction-scheduling-plan.md` |
| RocksDB compaction model (reference) | `docs/design/performance/rocksdb-compaction-model.md` |
| RocksDB read-path model (reference) | `docs/design/performance/rocksdb-read-path-model.md` |
| Scan iterator redesign | `docs/design/performance/scan-iterator-redesign.md` |
| **Measured durable-load amplification evidence (the proof)** | `docs/design/performance/durable-load-amplification-evidence.md` |
| Previous engine's billion-scale roadmap and epics | `docs/architecture/previous-work/` |

M12 depends on the storage substrate (M4) and engine semantics (M6) being
functionally in place, and its exit gate **feeds the M11 performance NFR gate**:
V1 is not promoted to `main` until M12 closes. Per the implementation roadmap,
milestone scheduling is a DAG, not a serial chain — M12 is the performance node
that hangs off M4/M6 and gates promotion.

---

## 2. Lessons From The Previous Billion-Scale Effort

The previous engine reached billion-record scale. The record of *how* —
`billion-scale-roadmap.md`, `reference-implementation-audit.md`,
`read-path-optimization-{v2,v3}.md`, and `scale-up-epics-19-37.md` — is more
valuable for its process scars than its specific epics. These lessons are
binding on M12.

### 2.1 Methodology lessons (these are the expensive ones)

1. **Instrument before theorizing. Per-step timing beats cost tables.**
   The read-path effort went through three iterations (v0 → v2 → v3). Every
   ranking derived from analytical reasoning was *wrong*. v0 blamed "no block
   cache / decompression cost." The real wins, found only by instrumented
   per-step timing, were: a **64 KB block size** causing 50 µs `pread` even on a
   page-cache hit (16-page memcpy), **unpinned index/bloom** reloaded per read,
   **lock contention on every cache hit**, and — most damning — **compaction was
   never actually running** (starved behind flush in a FIFO scheduler). None of
   these were in any cost table.
   - *M12 rule:* the durable commit/compaction path already has perf-trace timers
     (`056a8245`). Extend them to cover compaction selection, merge, install,
     reclaim, and flush. **No M12 slice ships a fix for a bottleneck it did not
     first measure.**

2. **Multiplicative phase projections are fiction.** The previous v2 plan
   projected a clean "19× stack." It never materialized; v3's actual wins came
   from different causes and the numbers never reconciled. Two epics (26 and 33)
   even specced the *same* dynamic-level-sizing work twice with contradictory
   projected outcomes.
   - *M12 rule:* every epic states a **measured** before number and a target
     band, and closes on a **re-measured** after number. Stacked projections are
     hypotheses, never commitments. One technique = one epic.

3. **Measure at the scale you target.** v0 optimized for 1M and got the
   mechanism wrong; the truth only appeared at 1B under profiling. M12 benchmarks
   at 10M (the measured baseline below), 100M, and 1B — it does not extrapolate
   from 1M.

4. **Verify background subsystems actually execute.** The single most
   embarrassing previous finding: months of read-path reasoning while compaction
   was silently dead. This is corroborated by the project's own production memory
   ("compaction priority must be High — Low caused starvation").
   - *M12 rule:* every background subsystem (compaction, flush, reclaim,
     checkpoint) exposes a **progress metric** and the test track asserts forward
     progress under load. A subsystem that isn't provably running is treated as
     broken.

5. **Space amplification is a force multiplier — but find *its* cause first.**
   Less on-disk data ⇒ more working set in cache ⇒ fewer levels searched ⇒ every
   later optimization counts for more. The previous plan sequenced dynamic level
   sizing first for this reason. **V1 caveat (measured):** here the space-amp (16×
   at 5M) is **un-reclaimed dead tables + write-amp downstream of a starved
   maintenance pipeline (RC1)** — so the lever is *making maintenance keep up and
   reclaim run* (M12C/M12B), not a level-sizing or trigger change. Lead with the
   measured cause of the amplification, not the amplification's proxy.

6. **Format density is frozen-cost; decide it up front.** The previous engine
   needed `FORMAT_VERSION` bumps to 4 and 5 to retrofit prefix compression and a
   block hash index. Restart points (built for binary search) happened to enable
   prefix compression for free — good fortune, not design. V1 freezes its durable
   format at M3; M12I audits that frozen format against billion-scale density
   *before* the format is load-bearing at scale.

7. **Scope discipline pays.** The previous v2 kept an explicit "NOT doing and
   why" table (Direct I/O, MultiGet, Ribbon filters, BlobDB, …). M12 keeps an
   equivalent living deferral list (§8).

### 2.2 Technical lessons that transfer cleanly

- **Block size is a first-order read-latency knob**, not a compression detail
  (64 KB → 4 KB was the previous single biggest win).
- **Pin hot metadata (index + bloom) in memory; never route it through the
  evictable block cache.**
- **Lock-free / read-mostly structures on the read hot path** (CLOCK, ArcSwap,
  RwLock+atomic). A mutex on every cache hit is a real bottleneck. (Echoed by the
  project's "DashMap contention / lock-free cache" findings.)
- **Pick compaction strategy with read-amp as the design driver**, then separate
  *policy* (what to compact, scoring) from *mechanism* (how to merge).
- **Two LevelDB correctness details are non-negotiable** in leveled compaction:
  grandparent (L+2) overlap tracking (prevents cascading write amp) and
  boundary-file inclusion (MVCC keys spanning file boundaries).
- **Concurrent-writer throughput needs write coalescing** (one WAL fsync + one
  apply per N writers) and **graceful backpressure** (soft throttle, not
  hard-stall cliffs).
- **Track the full vector, not throughput alone:** load, run (per workload),
  p50/p99, RSS, on-disk bytes, and space amp — together, at fixed scale points.

### 2.3 What is *different* for V1 (do not re-run the old roadmap)

The previous roadmap was **read-path-centric** because the old engine started
from flat size-tiered segments, `mmap`, and an O(N) block cache. Its epics 19–37
*built* leveled compaction, ArcSwap versioning, sharded caches, blooms, restart
points, and dynamic level sizing from nothing.

**V1 already has all of that.** Storage-next ships leveled compaction (8 levels,
×10 growth), versioned segments, a manifest with per-segment level tracking, a
block cache, and bloom filters — by construction, from M3/M4. Re-implementing the
old epics would be wasted motion.

The V1 baseline (§3) shows the bottleneck has **moved to the durable write,
compaction, and space-amplification path**, plus one frontier the old engine
never faced: **branch-aware garbage collection under copy-on-write**. M12 targets
*that* problem set. It carries the old *discipline* (instrument, measure, lead
with space amp, dense format, lock-free reads) onto a *different* set of
bottlenecks.

---

## 3. The V1 Baseline (Measured, 10M Records)

YCSB, 10M records × 1 KB values, 200K operations, 48 GiB memory budget, single
host, measured at commit `949fda1a` — i.e. **with** the WAL reclaim/cap fix
(`ec4063b8`), size-driven flush (`67219ba1`), and proportional write throttle
(`949fda1a`) already landed. Run-phase throughput (ops/s):

| Workload | Cache | RocksDB | Durable | Durable vs RocksDB |
|---|--:|--:|--:|--:|
| A — update-heavy (50r/50u) | 216,820 | 295,038 | 3,805 | **78× slower** |
| B — read-mostly (95r/5u) | 483,213 | 409,761 | 15,596 | 26× slower |
| C — read-only (100r) | 534,224 | 447,613 | 92,964 | 4.8× slower |
| D — read-latest (95r/5i) | 403,318 | 317,916 | **548** | **580× slower** |
| E — short-ranges (95s/5i) | 8,087 | 40,495 | 8,279 | ~5× slower (scan) |
| F — read-modify-write (50r/50rmw) | 182,667 | 219,390 | 17,062 | 13× slower |

Load throughput (ops/s): **Cache** ~387K–454K · **RocksDB** ~950K–977K ·
**Durable** ~28K–79K (12–34× slower than RocksDB).

Durable space: **~43 GB of tables for ~10 GB of logical data** (~4.3× amp), with
observed **multi-second write stalls** (up to ~42 s) during sustained load.

### What the baseline establishes

1. **Durable writes/updates are the crisis, not reads.** C (read-only) is within
   5× of RocksDB; A/D/F (write-bearing) are 13–580× slower. The proportional
   throttle and WAL fixes already landed did **not** rescue durable — so the
   remaining causes are deeper: compaction selection, single-threaded drain, and
   dead-object accumulation.
2. **Space amplification is live and large.** 43 GB for 10 GB. The
   pre-implementation deep measurement (§3.1) shows this is **un-reclaimed dead
   tables** (SA2) compounded by ≈15× write-amp — both downstream of the
   maintenance pipeline not keeping up (RC1), **not** a count-trigger artifact.
3. **The D-workload collapse (548 ops/s) is the signature of write-stall +
   compaction-debt interaction** — read-latest contends with an under-converged
   tree while compaction debt accumulates.
4. **Cache mode is competitive on reads** (B/C/D beat or match RocksDB) but
   **loses on scans (E) and load**, and is **capped by the frozen-mutable pool**
   (budget/4) at higher record counts.
5. **WAL is bounded** (27 MB observed) — RC3 holds. Do not reopen it; guard it.

This baseline is the anchor. Every M12 target is expressed as "close this
specific measured gap," and re-measured at 10M before extrapolating to 100M/1B.

### 3.1 Pre-implementation deep measurement (the proof)

Before writing any fix, the durable load was instrumented and measured at 1M/3M/5M
through the L9 perf-trace harness (full data + reproduce steps in
`docs/design/performance/durable-load-amplification-evidence.md`):

| Scale | Load | On-disk | Space-amp | Compaction out | Write-amp | Live/total tables |
|---|--:|--:|--:|--:|--:|--:|
| 1M | 140,886 ops/s | 1.1 GB | 1.1× | 0 (not run) | 0× | 3 / 3 |
| 3M | 63,530 ops/s | 14 GB | 4.7× | 8.3 GB | 2.8× | — |
| 5M | 16,667 ops/s | 78 GB | **15.8×** | 74.5 GB | **14.9×** | **155 / 2,295** |

Load time is super-linear (5× data → 42× time). At 5M the maintenance pipeline
coalesces away **99.4%** of the maintenance it needs, **93%** of on-disk tables
are dead (un-reclaimed on a single branch), and selected levels sit **4–8× over**
byte target. This **refutes** the count-trigger hypothesis and pins the root cause
on the single-threaded maintenance pipeline (RC1) → starved reclaim (SA2) + late
giant merges (WA1). Sections 4–7 are ordered accordingly.

---

## 4. Root-Cause Inventory And Current State

Status legend: **DONE** (landed, verify holds at scale) · **PARTIAL** (started) ·
**OPEN** (the M12 work).

Ordered by **measured** priority (see the evidence doc and the update box at the
top). Status legend: **DONE** · **PARTIAL** · **OPEN** · **DEMOTED** (hypothesis
refuted by measurement).

| # | Root cause | Effect at scale (measured durable load, 1M→5M) | State | Epic |
|---|---|---|---|---|
| **RC1** | **Single-threaded maintenance pipeline cannot keep up** — one lane, coalescing **99.4%** of the maintenance it needs (190,128 of 191,211 at 5M) | **Root cause.** Levels overflow **4–8× over** target; load collapses **141K→17K ops/s** (1M→5M, super-linear); 849 s background maintenance, 197 s commits blocked on the lane, 48.6 s admission stalls | **OPEN** | **M12C (lead)** |
| **SA2** | **Obsolete tables not reclaimed** — reclaim starved on the same lane; single-branch reclaim (`Arc==1`) should be free | **2,140 / 2,295 tables dead (93%) at 5M → 78 GB for 5 GB = 16× space-amp** | **OPEN** | **M12B (lead)** |
| **WA1** | **Compaction write amplification** — late, giant, repeated bottom-level merges (consequence of RC1) | **74.5 GB compaction output for 5 GB logical ≈ 15×**; each row re-merged ~13× (0 dropped) | **OPEN** | M12C / M12A |
| RC2 | Binary block-or-accept backpressure | Burst→stall→burst; 42 s stalls (10M); hard admission **rejection** at the default profile | **PARTIAL** (`949fda1a`) | M12D |
| RC3 | WAL truncation gated on checkpoint | Was: unbounded WAL → disk exhaustion | **DONE** (`ec4063b8`,`67219ba1`) — verified bounded (≤662 MB at 5M) | M12D (guard) |
| ~~SA1~~ | ~~Nonzero compaction triggers on table count, not bytes~~ | **REFUTED.** Levels are compacted **4–8× over** byte target, not under — `#3a` is not the amplification cause | **DEMOTED** | M12A |
| IO1 | No compaction I/O scheduling under read pressure | Compaction starves foreground reads (100M) | **OPEN** | M12E |
| SC1 | Scan rebuilds iterator/snapshot per call | Short-range scans (E) ~5× slow vs RocksDB; was 300× at 5M | **OPEN** | M12F |
| RD1 | Two read paths; per-read lock/snapshot overhead | Point-read ceiling; lock contention | **OPEN** | M12G |
| CA1 | Cache frozen-mutable pool = budget/4 | Cache load OOMs frozen pool past ~10M at modest budgets | **OPEN** | M12H |
| FD1 | Durable block format density unverified for 1 TB | Metadata bloat dilutes cache; retrofits are frozen-cost | **OPEN** (audit) | M12I |
| VEC | Vector index fan-out across per-segment graphs | ANN QPS scales poorly; parked at whole-collection seal | **PARKED** | M12J |

The **measured load-bearing chain** for durable is **RC1 → SA2 + WA1**: the
single-threaded maintenance lane cannot keep up, which starves obsolete-table
reclaim (SA2) and forces late, giant, repeated merges (WA1). **Fix RC1 first;
SA2 and WA1 largely follow it.** The original "SA1 byte-trigger" hypothesis was
refuted by measurement *before* implementation — M12A is retained only as a
selection-tidiness change, re-validated after RC1/SA2 land, never as the lead.

---

## 5. Milestone M12: Epics And Exit Gates

Per V1 conventions, each epic has an implementation track (`M12{A..}`) and a
matching test track (`M12T{A..}`); the milestone closes only when both pass.
Slice codes (`M12A1`, …) are assigned when an epic is ready to implement.
Identifiers are planning metadata only — they never appear in code, error codes,
metrics, or user-facing text.

Rows are ordered by **measured** priority, not by epic letter (letters are stable
identifiers only). M12C and M12B lead; M12A is demoted.

| Epic | Title | Exit gate (re-measured, not projected) |
|---|---|---|
| **M12C** *(lead)* | Keep-up maintenance pipeline — concurrency + scheduling (RC1) | Maintenance keeps up under load: **<5%** of needed maintenance coalesced at 5M/10M; load no longer collapses super-linearly (5× data ≈ 5× time, not 42×); durable load within ~3× of RocksDB at 10M |
| **M12B** *(lead)* | Eager obsolete-table reclaim (SA2) | Single-branch: **live tables ≈ total on-disk tables** after load (≥90% reclaimed); space-amp ≤ ~1.5× at 5M/10M; **zero** added overhead on the single-branch hot path; COW blocker-set correctness preserved |
| **M12D** | Proportional backpressure completion + WAL guard (RC2/RC3) | No write stall > ~1 s; **no hard admission rejection** on a sustainable load; WAL bytes bounded under all admission pressure |
| **M12E** | Compaction I/O scheduling (SILK three-tier + adaptive limit) | Foreground point-read p99 stays within ~2× of idle while deep compaction runs; flush never throttled |
| **M12A** *(demoted — hypothesis refuted)* | Byte-score compaction selection + sane level ladder | Selection-tidiness only, **re-validated after M12C/M12B**: the trigger change does **not** regress write-amp / space-amp vs the post-M12C baseline. Not the amplification fix |
| **M12F** | Scan path: cursor reuse + seek-in-place | Short-range scan (E) within ~2× of RocksDB at 10M |
| **M12G** | Read-path unification (single snapshot, lock-free common case) | Point read takes **no** per-read lock in the common case; one snapshot mechanism across both paths |
| **M12H** | Cache-mode scaling (frozen-pool ceiling) | Cache loads 100M at a documented budget without frozen-pool exhaustion; reads stay ≥ RocksDB |
| **M12I** | Durable format density audit + (gated) re-freeze | Frozen block format meets billion-scale density targets, or a deliberate re-freeze lands with golden vectors |
| **M12J** | Vector indexing at scale (unpark) | Documented recall@10/QPS curve at ≥ 1M vectors via whole-collection / tiered seal policy |
| **M12K** | Billion-scale benchmark + methodology harness | Standing before/after table at 1K→1B (load, A–F, p50/p99, RSS, disk, space-amp, **live-vs-total table count, maintenance keep-up**); progress metrics on all background subsystems; profiling hooks across the durable path |

Test track:

| Test epic | Title | Links |
|---|---|---|
| **M12TA** | Compaction selection/severity property + boundary tests | M12A |
| **M12TB** | Eager reclaim: single-branch ≥90%-reclaimed + zero-overhead guard + COW blocker-set model | M12B |
| **M12TC** | Maintenance keep-up assertion + concurrent-compaction correctness (disjoint inputs, install races) | M12C |
| **M12TD** | Backpressure smoothness + WAL-bound crash/fault tests | M12D |
| **M12TE** | Read-under-compaction latency harness | M12E |
| **M12TF** | Scan correctness across seek-in-place + MVCC boundary | M12F |
| **M12TG** | Read-path snapshot consistency + lock-free assertions | M12G |
| **M12TH** | Cache-mode budget conformance (no durable objects) | M12H |
| **M12TI** | Format golden vectors + density assertions | M12I |
| **M12TJ** | Vector recall/QPS conformance at scale | M12J |
| **M12TK** | Benchmark harness determinism + progress-metric assertions | M12K |

---

## 6. Epic Detail

### M12A — Byte-score compaction selection + sane level ladder (SA1)

**Problem.** `nonzero_compaction_pressure_for_target` gates on
`table_count < THRESHOLD(4) OR byte_count < target` applied uniformly to every
nonzero level, and escalates severity to `BlockMutatingAdmission` at ≥16 tables
regardless of bytes. A deep level compacts as soon as it holds ~4 tables
(~256 MiB) instead of its byte target (L2 ≈ 2.56 GB, L3 ≈ 25.6 GB). In theory this could
over-rewrite deep levels; **measurement shows it does not** — levels are compacted
**4–8× over** target, not under (evidence doc). `NONZERO_LEVEL_MIN_BASE_TARGET_BYTES`
is still 1 MiB (`compaction.rs:39`): a real selection-tidiness issue, but **not**
the amplification cause.

**Fix (RocksDB model).** Trigger nonzero-level compaction by **byte score**
(`level_bytes / target ≥ 1`), keep L0 on its file-**count** trigger (read-amp
guard). Drive severity from byte tiers only (reuse `nonzero_level_urgent_bytes`
2× / `nonzero_level_blocking_bytes` 4×). Return a byte-pressure score comparable
to L0's count score so the existing L0-vs-nonzero picker stays meaningful. Raise
the base-target floor to 256 MiB so the ladder is RocksDB-sane (L1 ≈ 256 MiB,
L2 ≈ 2.56 GB, L3 ≈ 25.6 GB). **Leave L0 and the bottommost-skip untouched.**

> **Demoted (2026-06-22) — was the lead; measurement refuted its premise.** This
> is **no longer the first slice.** The amplification is caused by RC1/SA2
> (M12C/M12B), not by premature count-based selection — and making the trigger
> *less* eager could let levels overflow even further. Keep this only as a
> selection-tidiness change, sequenced **after** M12C/M12B and gated on "does not
> regress the post-M12C write-amp / space-amp baseline." A detailed slice plan
> exists (byte-only trigger + severity + score, 256 MiB floor, "20 tiny tables at
> L2 must not block admission" guard); PR class: intentional semantic change,
> assurance S3.

**Riskiest point.** Severity → admission-block coupling: the block must become
byte-based purely via the trigger change. The "tiny tables don't block" test is
the guard against a surviving count disjunct. Do not regress L0. Do not run this
before M12C/M12B — a less-eager trigger on a still-starved pipeline is a
regression risk, not a win.

### M12B *(lead)* — Eager obsolete-table reclaim, then COW-aware GC (SA2)

**Problem (measured — dominant on-disk cost).** Obsolete tables are not reclaimed
during a durable load: at 5M, **2,140 of 2,295 on-disk tables (93%) are dead** —
on a **single branch**, where reclaim should be free (`Arc::strong_count == 1`,
no blocker set). That is ~16× space amplification (78 GB for 5 GB). Reclaim is
starved on the same single maintenance lane as compaction (RC1), so it never runs
under load. Under COW branching it compounds: an obsoleted table cannot be deleted
while any fork still references it — no published system solves branch-aware LSM
GC (`compaction-research.md`).

**Fix — two steps, immediate win first.**
1. **Single-branch eager reclaim (immediate, the measured win).** Run reclaim
   eagerly and cheaply on the common single-branch path via the empty-blocker fast
   path (`Arc::strong_count() == 1`), decoupled from the starved maintenance lane
   so it actually executes under load. Target: live ≈ total tables after load.
   Substrate already exists: `table_reachability::live_table_objects` + the
   quarantine service.
2. **COW blocker-set GC (the frontier).** Each segment carries one owner + a
   blocker set (forks still referencing it); deletable when obsoleted **and** the
   blocker set is empty. Reclaimability becomes a compaction-scoring *factor* (not
   a veto; tombstone-load / size-debt / age overrides still force compaction);
   ROI-based materialization when a parent reclaim unblocks ≥ 2× the copy cost.

**Non-negotiable:** **zero overhead for the single-branch common case** — step 1
must not regress the single-branch hot path, and step 2's blocker machinery and
reachability matrix are skipped entirely when `Arc::strong_count() == 1`.

### M12C *(lead)* — Keep-up maintenance pipeline: concurrency + scheduling (RC1)

**Problem (measured — the root cause).** The maintenance pipeline is a single lane
that cannot keep up. At 5M it **coalesces away 99.4% of the maintenance it needs**
(190,128 of 191,211 suggested; only 1,083 run), spends 849 s in background
maintenance with 197 s of commits blocked on the lane and 48.6 s in admission
stalls, and lets levels overflow **4–8× over** target before compacting. Load
collapses super-linearly (141K→17K ops/s, 1M→5M; 5× data → 42× time). This is the
dominant cause of the durable crawl and, via starvation, of SA2 (reclaim never
runs) and WA1 (late giant merges). (`durable-write-pipeline-scaling.md` RC1;
`rocksdb-compaction-model.md`.)

**Fix (RocksDB model).** Move to **one compaction per task with self-re-scheduling**:
after each compaction completes, re-evaluate and submit the next, so the pipeline
stops coalescing away the work it owes. Mark inputs `being_compacted` so concurrent
off-lock builds pick **disjoint** inputs; relax the one-per-lane gate; add a
`max_background_compactions` knob. Give flush/compaction/reclaim enough lanes that
maintenance keeps up (the keep-up counter — % coalesced — is the exit gate). Then
add **subcompactions** (partition one large compaction by key range across
threads). Install stays serialized under the manifest lock. Carry the two LevelDB
correctness details (grandparent overlap, boundary files) into any new merge path
(§2.2).

**This is the lead slice.** It is the measured root cause; M12B (reclaim) and WA1
(write-amp) are largely downstream of it. Sequence M12C and M12B first, together.
The demoted M12A byte-trigger comes *after*, gated on not regressing the
post-M12C baseline — a less-eager trigger on a still-starved pipeline is a
regression risk, not a win.

### M12D — Proportional backpressure completion + WAL guard (RC2/RC3)

**Problem / state.** The proportional throttle (`949fda1a`) is in, but the 10M
baseline (measured *with* it) still showed multi-second stalls — backpressure is
not yet smooth under compaction debt. RC3 (WAL bound) is done and must be guarded
so no future slice reopens unbounded growth.

**Fix.** Verify and complete the proportional throttle so admission degrades
gradiently (no burst→stall→burst); ensure the throttle responds to compaction
debt, not just memtable bytes. Add a standing WAL-bound assertion to the test
track (fault-injection: pressure that cannot proceed must still bound WAL bytes).

### M12E — Compaction I/O scheduling (IO1)

**Problem.** With concurrent compaction (M12C) doing real work, foreground reads
must not starve. (`compaction-io-starvation.md`: at 100M, 4-thread reads took
60+ min vs RocksDB 174 s, because compaction monopolized I/O and evicted hot
pages.)

**Fix (simplified plan Step 1 first).** Turn on the existing rate limiter
(static, e.g. ~50–100 MB/s) and write backpressure defaults; add cooperative
cancellation points inside the compaction loop so it yields. *Then*, only if
measurement demands it, the SILK three-tier model: Tier 1 flush (never
throttled), Tier 2 L0→L1 (lightly throttled under read pressure), Tier 3 deep
compaction (heavily throttled), with adaptive limiting keyed on foreground
cache-miss rate and hysteresis.

### M12F — Scan path: cursor reuse + seek-in-place (SC1)

**Problem.** Each scan rebuilds the snapshot and segment iterators
(`scan-iterator-redesign.md`, #2213): executor dispatch + DashMap + BranchSnapshot
+ per-source seek/heap construction. E-workload is ~5× RocksDB at 10M (was 300×
at 5M before earlier work).

**Fix (two phases).** Phase 1: expose `kv_scan_cursor()` reusing one snapshot
across scans (eliminates dispatch/DashMap/snapshot rebuild). Phase 2: seek-in-
place — reposition persisted segment iterators instead of destroying/rebuilding,
collecting a bounded window per source post-seek. Memtable range iterators stay
separate. Residual gap vs RocksDB is structural (LSM merge + MVCC dedup vs B-tree
sequential) — bound it, don't chase it.

### M12G — Read-path unification (RD1)

**Problem.** Two read paths (`rocksdb-read-path-model.md`): the transaction path
holds a DashMap lock for the whole MVCC traversal; the direct path skips snapshot
pinning and pays per-read lock/cache overhead.

**Fix.** Unify to one `BranchSnapshot` (RocksDB SuperVersion analog) used by both
paths: direct reads at `max_version = u64::MAX` with no read-set; transaction
reads at `start_version` with read-set tracking; both call the same lock-free
lookup. Thread-local snapshot cache invalidated by a monotonic version number
(one atomic load per read in the common case — no DashMap access). Deferred until
M12C/M12E confirm the compaction fixes don't already resolve the concurrent-read
ceiling; sequence by measurement.

### M12H — Cache-mode scaling (CA1)

**Problem.** The frozen-mutable pool is budget/4; cache load exhausts it past
~10M at modest budgets (the 32 GiB cache run failed here; 48 GiB completed).
Cache mode must scale to 100M+ on a documented budget without inheriting any
durable object (no WAL/manifest/snapshot/checkpoint) — that invariant is fixed.

**Fix.** Make the frozen/active pool split scale sensibly with memory budget and
record count; flush/rotate frozen tables under pressure within cache semantics;
document the budget→record-count envelope. Keep the cache-mode conformance guard
(asserts no durable objects).

### M12I — Durable format density audit + gated re-freeze (FD1)

**Problem (frozen-cost lesson, §2.6).** The durable block format is frozen at M3.
At 1 TB, format density (entry-header size, prefix compression, restart points,
index-separator shortening, bloom quality/locality) decides how much working set
fits in cache. The previous engine paid `FORMAT_VERSION` bumps to retrofit these.

**Fix.** **Audit** the M3-frozen format against billion-scale density targets:
entries/block (target RocksDB-class density), index residency at 1 TB, bloom FPR
(previous engine measured 1.8% vs 0.8% theoretical with weak hashes) and
cache-line locality, and block size for the point-read workload. If the frozen
format meets the targets, record the evidence and close. If a gap is material,
schedule a **deliberate, gated re-freeze** with new golden vectors — not an ad
hoc bump. This epic is an audit-with-decision, not an assumed rewrite.

### M12J — Vector indexing at scale (VEC) — *parked*

**Problem.** Similarity search has no key locality, so per-segment HNSW graphs
force query fan-out across every sealed segment. Parked at the "whole-collection
seal" (Option A: one flat + one HNSW over the full visible set), which reached
~5000 QPS at 200K vectors.

**Fix.** Unpark the tiered seal policy and establish a documented recall@10-vs-QPS
curve (ANN-Benchmarks aligned, SIFT1M) at ≥ 1M vectors, with the ef_search sweep.
Sequenced last — it is orthogonal to the durable-engine crisis and the engine
must be billion-scale-sound first.

### M12K — Billion-scale benchmark + methodology harness

**Problem.** §2.1 — without standing measurement and progress metrics, the
project repeats the previous engine's misdiagnoses.

**Fix.** A standing benchmark producing the **full vector** (load, A–F run,
p50/p99, RSS, on-disk bytes, space amp) at fixed scale points (1K, 10K, 100K, 1M,
10M, 100M, 1B) for cache / durable / RocksDB. Profiling hooks across the durable
commit + compaction path (extend `056a8245`). **Progress metrics on every
background subsystem** with test-track assertions of forward progress under load.
This is the M9F/M10D re-baseline made concrete and is a prerequisite measurement
substrate for closing every other epic's gate.

---

## 7. Sequencing And Dependencies

```text
M12K (measure: keep-up % + live-vs-total tables)  ── prerequisite for all gates
        │
        ├─►  M12C (keep-up maintenance pipeline, RC1)  ◄── measured root cause
        │          │  SA2 + WA1 largely follow once the pipeline keeps up
        └─►  M12B (eager single-branch reclaim → COW GC)  ── run WITH M12C
                   │
M12D (backpressure + WAL guard) ── tightens once M12C makes admission sane
        │
M12E (I/O scheduling) ──► after M12C does real concurrent compaction
        │
M12A (byte-score selection) ── DEMOTED: after M12C/M12B, gated on no regression
        │
M12F (scan)   M12G (read-path) ── sequence by measurement after compaction fixes
M12H (cache scaling)           ── independent
M12I (format audit)            ── early audit; re-freeze (if any) gated
M12J (vector)                  ── last, orthogonal
```

Ordering rationale (measured — see §4 and the evidence doc):

1. **M12K first** — you cannot fix what you cannot measure (lesson 2.1.1/2.1.4).
   The maintenance keep-up % and live-vs-total table count are the gates for the
   leads.
2. **M12C + M12B lead, together** — RC1 (the single starved maintenance lane) is
   the measured root cause; SA2 (reclaim) and WA1 (write-amp) are largely
   downstream. Make maintenance keep up and reclaim run, then re-measure.
3. **M12A is demoted, not deleted** — its premise (count-based over-compaction)
   was refuted. Run it only *after* M12C/M12B and only if it does not regress the
   post-M12C write-amp / space-amp baseline; a less-eager trigger on a still
   starved pipeline risks a regression (lesson 2.1.1: measure, don't theorize).
4. **M12D tightens after M12C** — once the pipeline keeps up, backpressure should
   degrade gradiently with no hard admission rejection; guard the WAL bound.
5. **M12E after M12C** — read/compaction interference only matters once
   compaction does real concurrent work.
6. **M12F/M12G sequenced by measurement** — do not assume the read/scan ceiling
   before the compaction fixes; re-measure (lesson 2.1.3).
7. **M12I early as an audit** — format decisions are frozen-cost; surface any
   re-freeze need before the format is load-bearing at 1 TB.
8. **M12J last** — orthogonal to the durable crisis.

---

## 8. Billion-Scale Targets (To Be Validated, Not Promised)

Per §2.1.2, these are target bands anchored to the measured 10M baseline, to be
**re-measured** at each scale point. They are not multiplicative projections.

| Metric | 10M measured (durable) | 10M target | 1B target |
|---|--:|--:|--:|
| Load vs RocksDB | 12–34× slower | ≤ 3× slower | ≤ 3× slower |
| A update-heavy | 3,805 (78×) | ≥ ~80K | within ~3× RocksDB |
| D read-latest | **548 (580×)** | ≥ ~50K | ≥ 50K |
| C read-only | 92,964 (4.8×) | ≥ ~200K | 50–100K (1 TB, disk-bound) |
| E short-range scan | 8,279 (~5×) | within ~2× RocksDB | within ~2× RocksDB |
| Space amplification | ~4.3× (43 GB) | ≤ ~1.3× | ≤ ~1.5× |
| Max write stall | ~42 s | ≤ ~1 s | ≤ ~1 s |
| WAL bound | 27 MB (held) | bounded | bounded |

The 1 TB read target (50–100K random reads/s) matches the previous engine's
validated billion-scale band and RocksDB's measured ~189K at 900M — disk-bound
once the working set exceeds RAM, gated by bloom efficiency and level count.

---

## 9. Out Of M12 Scope (Deferred, With Rationale)

Following the previous v2's discipline, these are explicit non-goals for M12:

- **Key-value separation (BlobDB).** Helps only large values (> ~1 KB rewritten
  repeatedly); revisit only if a workload shows value-dominated write amp.
- **MultiGet batched read API.** A real win (previous projection +200–300% on
  batches) but it is a new surface needing engine/intelligence callers to batch;
  defer until the single-key path is sound and a batching consumer exists.
- **Direct I/O (O_DIRECT).** Alignment complexity for ~10% under specific
  conditions; defer.
- **Universal/size-tiered compaction option.** Leveled is the V1 read-amp choice;
  a second strategy is a post-V1 knob, not an M12 deliverable.
- **Tiered hot/cold storage tiers; cross-machine sync; network server mode.**
  Out of V1 scope entirely (see implementation roadmap §"Out Of V1 Scope").
- **Per-branch adaptive compaction tuning (Dostoevsky/Fluid-LSM extended to
  branch-local).** Publication-interesting (`compaction-research.md`) but beyond
  the billion-scale gate; backlog.

This list is living: an item moves into scope only when a *measured* bottleneck
justifies it.

---

## 10. Acceptance Criteria For This Roadmap

This roadmap is sufficient when:

1. Every epic maps to a measured baseline gap (§3/§4), not a hypothesized one.
2. Every epic has a re-measured exit gate, not a projection.
3. Sequencing leads with the **measured** root cause (M12C maintenance keep-up +
   M12B eager reclaim) and gates the demoted M12A byte-trigger behind
   re-validation against the post-M12C baseline — not behind a hypothesis.
4. M12B leads with single-branch eager reclaim (the measured win) under a hard
   single-branch zero-overhead constraint; the COW blocker-set GC frontier
   follows as step 2.
5. Format-density decisions are surfaced as frozen-cost before 1 TB scale.
6. The methodology lessons (instrument-first, measure-don't-project, verify
   background tasks run, full-vector benchmarking) are encoded as M12K and as
   binding rules, not aspirations.
7. The previous engine's *techniques and discipline* carry over; its
   *read-centric epic list* does not, because V1 already has those mechanics.
