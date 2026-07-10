# Billion-scale track — per-milestone performance ledger

Tracks the single-threaded durable scoreboard milestone to milestone so we can tell,
across the BS-track, whether each milestone moved perf where it was supposed to and
left everything else flat. Companion to the plan in
[`billion-scale-plan.md`](./billion-scale-plan.md) (§2 scoreboard, §3 gap inventory)
and the root-cause in [`rocksdb-parity-roadmap.md`](./rocksdb-parity-roadmap.md). The
lock-decoupling work has its own finer-grained ledger
([`lock-decoupling-perf-ledger.md`](./lock-decoupling-perf-ledger.md)); this one is the
umbrella scoreboard.

## Reference config (frozen — every ledger row uses this)

```text
single-threaded, durable engine, 1 KB values, keys 8-digit zero-padded.
scoreboard cells:  load 100K · load 1M · load 10M · run C @10M · run A @10M · run E @10M
                   (C = read-only, A = read-modify-write, E = short scan)
harness:           benchmarks/src/bin/storage_next_l9_scale.rs  (load/read cells + reopen-after-load; --scales, --memory-budget)
                   benchmarks/src/bin/regression.rs             (scoreboard capture → baselines/*.json)
memory-budget:     scoreboard cells at the machine's standard budget; exit-gate cells at 8 GiB.
```

Machine-local; compare rows only against other rows captured on the same machine. Each
load cell builds a fresh N-record durable database; the run cells then execute against it.

## How to read a row — signal vs. noise in the disk-resident regime

BS4 changed the memory model: hot data now flows through the block cache instead of a
fully-decoded resident slice. That makes some cells structurally comparable across the
BS4 boundary and some not:

- **Stable / trustworthy 1:1:**
  - **Load throughput** (bulk insert) — stable to ~5% run-to-run; the write path is the
    same shape before and after BS4.
  - **Read-only C** at a budget **≥ dataset** — a fully-cached read still resolves from
    cache; use this to prove BS4 did not regress the hot-read path.
  - **Open time** and the fast-open counters (`table_lazy_full_materializations`,
    `table_reader_opens`) — these are the BS4 deliverable; they *should* move (open goes
    O(dataset) → O(tables)).
- **NOT stable / needs care:**
  - **Read-only C** at a budget **< dataset** — this is a *new* regime (cold block
    fetches). It is not comparable to a pre-BS4 resident read; read it against RocksDB,
    not against the pre-BS4 row.
  - **A / E under load** — carry the write-path convoy (see the lock-decoupling ledger);
    a single run is a point sample of an intermittent crawl. Compare medians, not single
    runs.

## Exit-gate cells (BS4.6, disk-resident regime)

Separate from the scoreboard: the milestone's disk-residency claim.

| Gate | Cell | Target | Source |
|---|---|---|---|
| #1 | 100M × ~1 KB (~100 GB) loads **and serves** on an 8 GiB budget | success (today: hard-fails) | `durable_exit_gate_100m_on_8gib_budget` (`#[ignore]`) + l9 `--scales 100m --memory-budget 8g` |
| #2 | DB open after that load | ≤ 1 s | l9 reopen cell (`db_open_after_load_ms`) + the exit test's timed reopen |
| #3 | 10M scoreboard cells | within 1.5× of BS2/BS3 | regression.rs capture vs §2 |
| #5 | `lazy_full_materialization` on the exit open | 0 | perf-trace counter, asserted by the exit test |

Gate #4 (goldens / recovery byte-identity / oracle / fault sweep) is the existing standing
suites, green every slice.

## Ledger

| Milestone | HEAD | load 100K | load 1M | load 10M | C @10M | A @10M | E @10M | open @100M | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| pre-BS baseline | umbrella §2¹ | 330 K | 130 K | 90 K | 85 K | 110 | 7.2 K | O(dataset), 10 GB hard-fails @ 8 GB | reference — the "where we stand" snapshot |
| RocksDB (default) | — | 760 K | 935 K | 660 K | 368 K | 272 K | 39 K | ≤ 1 s | parity target (peer, not committed baseline) |
| BS1 install-time aggregates | landed² | *pending run* | *pending run* | *pending run* | *pending run* | *pending run* | *pending run* | — | fold-per-commit removed (G1–G3); expected: load ↑, A/F crawl ↓ |
| BS2 snapshot reads | landed² | — | — | — | *pending run* | *pending run* | *pending run* | — | reads off the global lock (G4–G6); expected: C ↑, read tail ↓ |
| BS3 admission (dark) | landed² | — | — | — | — | *pending run* | *pending run* | — | graceful admission behind `STRATA_ADMISSION` (G10); tail smoothing, throughput compaction-bound |
| **BS4 disk-resident (re-baseline)** | this branch³ | *pending run* | *pending run* | *pending run* | *pending run* | *pending run* | *pending run* | *pending run* (target ≤ 1 s) | **the regime change** — dataset on disk, memory = caches (G11–G16). Exit gates above. Numbers filled from the BS4.6 perf run (runbook). |
| BS4 + fork-manifest/GC fixes (dev-box l9, 10M×1KB, default budget)⁴ | `7fad3cff` | — | — | **41.1 K** | point p50 671 µs · 418 ops/s (cold-regime) | — | scan-prefix p50 612 µs | reopen w/ 100 forks: **24.8 s**, `lazy_full_materializations=0`, bounded RSS (pre-fix: OOM-killed) | first completed end-to-end 10M run on the disk-resident regime; fork p50 97.9 ms (+31%, fork-time manifest fsync); GC reclaims at lulls/close/reopen (space peaked ~85 GB mid-load — in-flight-registry follow-up needed for load-time reclaim); reopen dominated by O(children × tables) reader opens (37,902) — reader-sharing follow-up |

¹ `billion-scale-plan.md` §2 — a single pre-BS snapshot, single-threaded, 1 KB values, on
the reference machine. There is **no committed per-milestone baseline** for BS1–BS3, so
those rows carry the qualitative change only; the BS4.6 re-baseline is the first committed
scoreboard capture on this track (via `regression.rs --capture-baseline`).
² BS1–BS3 landed on prior tranches; their scoreboard cells were not committed as ledger
rows at the time. Backfill from a `regression.rs` capture on the reference machine if a
before/after per-milestone comparison is later required.
³ BS4.6 is the re-baseline slice: it builds the exit-gate harness + benchmark cells and
this ledger; the measured numbers are captured in the perf environment per
[`bs4-6-exit-runbook.md`](./bs4-6-exit-runbook.md) and backfilled here. Until then the
BS4 row reads *pending run* — do not infer a regression or an improvement from an empty
cell.
⁴ Dev-box run (not the quiesced reference machine), captured while validating the reopen-OOM
(fork-time child manifests) and table-object-GC fixes the first BS4.6 bench run surfaced.
The 1 KB point/scan cells are the **cold disk-resident regime** (10 GB dataset, 240 MiB
cache) — compare against RocksDB at the same operating point, not the pre-BS resident
snapshot. Known follow-ups: in-flight-output registry (reclaim during sustained load),
recovered-reader sharing across fork children (reopen ≤ 1 s at high fan-out),
deleted-branch manifest cleanup, tombstone-only quarantine staging.

## Write-scaling baseline (BS5.0, dev box, `storage-next-concurrent-writers` defaults: batch 10 × 16 B, 3 s windows)

The BS5 instrument's control capture — commits/s by writer-thread count on one shared
`&runtime`. The milestone's exit gates move these curves (Always ≥4× at 4 threads via group
fsync amortization; Standard ≥2.5×); single-thread must stay within noise.

| engine · branches | 1 thread | 2 | 4 | 8 | reading |
|---|---|---|---|---|---|
| standard · shared | 19,494 | 21,418 | 21,548 | 19,986 | **flat — the runtime mutex serializes (G17)** |
| standard · per-writer | 30,115 | 32,858 | 32,713 | 38,491 | ≤1.28× at 8 threads — cross-branch commits still serialize on the one mutex (G18) |
| always · shared | 159 | 154 | 160 | 159 | **flat — one fsync per commit under the lock; the group-commit target** |
| always · per-writer | 160 | 154 | 160 | 160 | per-branch does not help fsync-bound writes |
| cache · shared | 15,428 | 15,405 | 15,410 | 15,410 | wall-bound (frozen-budget stalls grow 1,138→8,469; totals identical) — use smaller payloads for pure protocol reads |

BS5.0 also hardened the multi-writer path itself: concurrent commits could spuriously fail
with "explicit commit timestamp is before the monotonic floor" (internally generated
timestamps now clamp), and rotation did not republish the Model-2 snapshot (acked commits
invisible to readers for 15–140 ms during flush build windows) — both caught by the new
4-writer S3 stress before any protocol change.

## Write groups (BS5.1, dev box, same instrument, medians of 3 isolated points)

Leader-executes-all write groups under the existing runtime lock: one durable-gate span per
group (range-widened unresolved fact), N deferred WAL appends + one covering fsync
(`Always`), one visible publish to the group max. Callers join a leadership queue; members
wait on their own condvar (never the runtime lock) and re-join immediately on completion; an
`Always`-only 150 µs formation window absorbs the just-served cohort into the next group.

| engine · branches | 1 thread | 2 | 4 | 8 | vs BS5.0 baseline |
|---|---|---|---|---|---|
| always · shared | 161 | 224 | 278 | 373 | **flat → 1.7× at 4 / 2.3× at 8 threads** (was 159 flat) |
| always · per-writer (4T) | — | — | 305 | — | 1.9× (was ≤1.28×) |
| standard · shared | 20,284 | — | 21,945 | 21,522 | unregressed, flat-to-slightly-positive (mutex-bound; BS5.2's target) |

Single-thread is byte- and throughput-identical to solo (group-of-1 equivalence is
test-anchored on whole-backend object snapshots). Group traces show fsync batching works —
size-7 groups take the same ~6.2 ms hold as solos — so the residual gap to the ≥4× milestone
gate is hold pipelining: formation can't overlap the in-flight fsync while both live under
the one runtime mutex. That is exactly BS5.2 (commit path off the mutex).

## Pipelined covering fsync (BS5.2, dev box, same instrument, isolated points, medians of 3)

The covering fsync moved OUT of the runtime mutex: an `Always` leader appends + applies
under one short hold (phase 1), hands leadership off, settles durability off-lock through a
sync chain (one fsync in flight at a time, captured fresh at sync time so it covers every
group that appended since; everyone covered skips their own), then publishes under a second
short hold (phase 2). Semantics that made it safe: gate multi-admission spans, a pipeline
frontier bounding admissions at max(visible, in-flight applied), monotone no-op publishes
for out-of-order settlement, fact-ordering rules (`fact.first <= group.last` fails a group;
above it doesn't), a durable-watermark rescue for sync failures covered by later syncs, and
flush deferral on branches with applied-above-visible rows.

| engine · branches | 1 thread | 2 | 4 | 8 | vs baseline (159 flat) |
|---|---|---|---|---|---|
| always · shared | 160 | 270 | 563 | 1,117 | **1.7× / 3.5× / 7.0×** (BS5.1: 1.4/1.7/2.3×) |
| always · per-writer (4T) | — | — | 509 | — | 3.2× |
| standard · shared | 19,783 | — | 21,955 | 22,151 | unregressed (protocol-cost-bound; BS5.3's question) |

Per-thread fairness tightened from ~1.4× spread (BS5.1) to ~1.05× — every writer rides
every sync round. The ≥4× exit gate at exactly 4 threads is capped by flush arithmetic on
this box (one ~6.2 ms flush per round → 3.9× ideal at 4T); the curve through 8 threads is
the gate's substance. Group-boundary crash sweeps (BS5.1's carried debt) landed with the
phase split: crash-before-sync full replay, torn-tail prefix replay, injected sync-failure
range-fact reconciliation, and two-groups-in-flight ordering.

## Standard-mode lock hygiene (BS5.3a, dev box, same instrument, medians of 3)

The measure gate redirected BS5.3: fine-grained profiling showed Standard's flat ~21K was
NOT memtable-bound (apply = 7 µs of a 16 µs protocol) — writers lost 10–18 µs per commit
to background-maintenance lock holds, and true-hold attribution found the theft: the
flush publish's INLINE WAL reclaim (durable-manifest loads + O(rows) coverage proof + a
durable manifest replace with fsync, per flush, under the runtime lock — ~950 ms of a 3 s
window) and an O(catalog) re-record of every table per manifest confirmation. Fixes:
route reclaim through the coalescing off-lock flush-watermark task (periodic-policy
backstop; advance is now one drain later), make the reserved-manifest confirmation an
O(1) debt-flag clear, and add a writers-first drain yield (one-task fairness floor).

| engine · branches | 1 thread | 4 | 8 | vs BS5.2 |
|---|---|---|---|---|
| standard · shared | 29,337 | 29,936 | 29,715 | **21K flat → ~30K (+43–48%)** |
| always · shared | 160 | 553 | 1,105 | unregressed |
| cache · shared | 15,431 | 15,402 | — | identical |

Remaining Standard rocks (BS5.3b question): the flush-install lock holds (~7.5 ms per
flush) and then the ~16 µs serialized protocol (~60K/s ceiling), where the original
SkipMap/parallel-apply plan meets the ≥2.5× (~50K) gate.

## Flush-install identity (BS5.3b, dev box, same instrument, medians of 3)

The ~7.5 ms flush-install hold was a row-by-row verification of the built table against
the frozen memtable UNDER the runtime lock (plus an all-frozen fallback scan). The
prepared flush now captures its input memtable's `Arc` identity at build time; the
install matches by identity in O(1) — strictly more precise than row equality — and the
row verification runs off-lock in the prepare phase, end to end through the published
object's reader. Standard shared: 30K → **~35K commits/s at 1/4/8 threads** (cumulative
+65% over the flat 21K baseline); 1T writer lock-wait down to ~292 ms per 3 s window
(from 1,330 ms). Always (162/538/1105) and cache unregressed. Next (BS5.3c): ~16 µs
serialized protocol + ~9-12 µs dispatch machinery vs the ≥2.5× (~50K) gate — the
SkipMap/parallel-apply decision point.

## Dispatch attribution closed (BS5.3c, dev box, same instrument, medians of 3)

Split-probing the residual ~10 µs "dispatch machinery" found it was mostly NOT machinery:
one real fix (the post-commit WAL-growth wait took two extra runtime-lock acquisitions
per commit to re-probe facts the commit itself had just evaluated — now gated on the
carried outcome), and the remainder is **BS3's write-throttle pacing working as designed**
(~20% of wall under the sustained bench load as the default memory budget fills; 5,664
paced commits ≈ 0.7 s actual pacing per 3 s window at 1T). Standard settles at **~35K
commits/s at 1/4/8 threads** — +67% over the flat 21K baseline across BS5.3a/b/c, with
backpressure semantics intact; Always (161/539/1078) and cache unregressed. The ≥2.5×
(~50K) gate decision is recorded in the plan doc as a two-part question: protocol
capacity (SkipMap/parallel-apply vs medium options) and pacing calibration (a product
decision for an admission-focused slice).

**BS5 milestone close-out (2026-07-07): the Standard ≥2.5× gate is deliberately parked,
not abandoned.** Always carries its gate's substance (7.0× at 8T; 4T flush-arithmetic-
capped); single-thread and cache unregressed throughout; crash sweeps green every slice.
The Standard remainder is attributed (protocol capacity + intentional throttle pacing)
with reopening criteria and recommended sequencing recorded in the plan doc's milestone-
exit section — see `bs5-write-concurrency-plan.md` § Perf validation.

## Parallel per-branch group apply (BS5.4, dev box, same instrument, medians of 3)

The multi-branch measurement met BS5.4's trigger: per-writer-branch Standard was flat at
~35–39K across 1–8T (identical to shared-branch — the serialized group protocol, not the
branch guards, was the ceiling), with ~13 µs of the ~17 µs per-member protocol
per-branch-parallelizable (apply 8.3 µs the dominant term). The landed remedy keeps the
memtable single-writer (D2) and parallelizes across branches: first-of-branch group
members hand their WAL-durable rows back instead of applying; the leader checks each
deferred branch's state out of the catalog (ownership transfer; accessors fail closed
while out) and hands each apply to its member's parked thread; a barrier restores every
state before the runtime lock drops (checked-out states are never observable across
groups). Results: **per-writer 8T 39.2K → 53.2K (+36%, 1.51× single-writer)** — 2.5× the
21K pre-milestone baseline — with per-thread fairness tightening from a 23–42K spread to
20.1–20.2K; 1T/2T/4T within noise (group occupancy too low to pay the barrier at ≤4T);
shared-branch, Always (fsync-bound), single-thread, and cache all unchanged. New
multi-branch multi-writer stress + checkout fail-closed tests; recovery oracle, fault
sweeps, and group byte-identity anchors green (the deferral moves WHEN rows apply, never
what the WAL or the publish contains).

BS5.1 also removed two pre-lock writer serializers found with the new instrument: the
commit-timestamp base and the durability-mode resolution both took the full runtime lock per
commit (writers queued behind an in-flight fsync before ever reaching the commit path — the
join queue always looked empty). The timestamp base now reads an off-lock atomic mirror
(clamp semantics unchanged; the allocator still enforces the floor under the lock) and the
mode comes from the open summary.

## V1 end-to-end baseline (2026-07-07, dev box, post-merge `d5f50899`, engine-level)

First end-to-end numbers on the v1 line (engine-next `Database` API; bench harness with
the jemalloc pin). YCSB: 100K records, 100K ops, 1KB values, 8GiB budget. Run throughput
(ops/s):

| Workload | cache | durable |
|---|---|---|
| A (50r/50u zipf) | 250,276 | 2,642 |
| B (95r/5u zipf) | 927,347 | 5,451 |
| C (100r zipf) | 1,195,705 | 874,907 |
| D (95r/5i latest) | 844,021 | 14,433 |
| E (5i/95scan zipf) | 20,717 | 2,290 |
| F (50r/50rmw zipf) | 278,633 | 1,927 |

engine-kv-scale (1M × 64B): load 437K/332K rows/s (cache/durable); point reads 865K/14.5K
ops/s (durable p50 42.6µs = cold block reads); scans 9.0K/3.1K ops/s.
engine-vector-indexing (10K × d64): writes 130–160K ops/s; HNSW query p50 ~10–13ms.
storage-next-l9 10M standard: load-seq 260K rows/s; point-latest 2,245 ops/s p50 491µs
and fork p50 72.8ms / p95 2.7s — both measured against live post-load compaction debt;
reopen-after-load 15.78s (8,947 reader opens, 78K data-block reads — needs its own
investigation against the BS4.5b fast-open expectation).

**Durable write-path attribution (new `engine-ycsb --perf-breakdown`): the runtime lock
is starved by background maintenance.** Workload A durable, one 36.5s run: commit-stage
work totals ~0.5s; foreground commits spent **19.9s waiting for the runtime lock**;
background tasks held it **38.6s cumulative (~93ms per task)** — compaction merges and
flush work running under the lock. No admission stalls (0 wait timeouts), no
checkpoints, no inline maintenance — pure lock theft. Update p50 is healthy (~80–105µs);
p99 ≈ 3–4ms and single multi-second maxima (12.9–22.6s) are individual long
merges/flushes holding the lock. The 1KB single-put engine workload rotates memtables
constantly, which the small-row storage-level instrument never exercised — this is
exactly the "real engine-layer workload data" the BS5 exit criteria reserved judgment
for. Next lever (new slice, measure-gated): move compaction merge/build off the runtime
lock (flush already builds off-lock via BS5.3b's identity install) or chunk merges with
writers-first yields.

## Off-lock GC staging (BS5.5, dev box, engine-ycsb instrument, 2026-07-07)

Correction to the entry above: the "under-lock compaction merge" attribution was wrong —
builds were already off-lock. The dominant stall was the GC low tier: retention's
O(tables) mark scan ran on every empty drain poll BEFORE checking a task existed
(29.7s/36.5s under lock, 37K probes), and the sweep/purge executions held the lock
~320ms each for per-object quarantine publishes and deletes. BS5.5 landed: existence
check before the mark, a pending guard at the drain ladder bottom, and `SweepStage` /
`PurgeStage` off-lock staging steps (mark and interlocks stay under the lock;
unreachability is monotone so staging cannot race builds). YCSB durable: fg lock wait
23.0s → 0.05–0.2s, update max 22.6s → **50ms**, B 5.5K → 11.9K ops/s, A/F +20–25%.
Remaining wall = BS3 throttle pacing (the parked calibration question, now isolated).
The l9-10M fork-latency re-validation (baseline p50 72.8ms / p95 2.7s) is PENDING: a
fork-only run samples at maximum post-load compaction debt and did not converge in-session
— re-run with the full workload ladder (as the baseline was measured) alongside the
reopen-after-load investigation. Full narrative in bs5-write-concurrency-plan.md § BS5.5.

## Next-levers session (2026-07-07, post-BS5.5): fork validated, reopen attributed, graded admission bake-off

**Fork latency (10M, full ladder): VALIDATED.** p50 72.8ms → 86.9ms, **p95 2.72s → 100ms,
p99 107.8ms** — the BS5.5 off-lock GC removed the fork tail entirely.

**Reopen-after-load: attributed to WAL replay, two layers.** The full-ladder 10M reopen
measured 223.5s (was 15.8s at the baseline) — but reader opens were flat (8,826 vs 8,947)
and the delta is replay volume, not the BS4.5b lazy-open path:
1. **Pre-existing elephant: replay runs at ~365µs/row.** `classify_replay_row` performs a
   FULL history walk (`BranchHistoryOptions::all()`) per replayed row for idempotence
   classification — measured 7 source probes per row (1.39M probes for a 198K-row tail at
   1M; 72s reopen after a load-only close). The duplicate check needs an exact
   (key, version) existence probe with early exit, not an all-sources history walk.
   Fix candidate: bounded/exact-version probe in replay classification — expected 10-50×
   on replay-heavy reopens. NEW SLICE.
2. **BS5.5 fattened the un-checkpointed tail at close** (223s vs 15.8s ≈ 600K-row vs
   ~45K-row tail at 365µs/row): live GC (sweep staging + purge + re-mark chains) competes
   for drain slots/worker time with checkpoint and flush-watermark cadence during the
   ladder. Quantify alongside the replay fix; the 1M control showed close-tail parity
   pre/post BS5.5 at load-only, so the interaction is ladder-cadence-specific.
   Instrument landed: the l9 reopen cell now prints replay_rows / replay_probes /
   replay_history_calls.

**Graded admission bake-off (BS3.4c): graded wins every cell.** engine-ycsb durable on
the idle second NVMe (/data2), interleaved 3×3 on A plus B/F and a 512MiB small-budget
gate, legacy vs `STRATA_ADMISSION=graded`:

| Cell | legacy | graded |
|---|---|---|
| A (50/50) median of 3 | 10.8K ops/s (7.9–13.2K) | **15.8K** (13.4–16.2K) |
| A update p99 | 0.6–2.2ms | ~1.1ms |
| A update max | 41–52ms | 304–337ms (bounded near-stop brake) |
| B (95r/5u) | 17.7K | **576K** (32×) |
| F (50r/50rmw) | 7.6K | **12.1K** |
| A @ 512MiB budget | 6.6K, 21K pressure rejects | **8.9K**, 6K rejects |

Stall-wall preserved (wait_timeouts=0 in every cell); the small-budget cell IMPROVES
under graded (fewer rejects, third the block-wait). The legacy P-controller paces by pool
fullness, so light writers (B) pay constantly; graded paces by compaction debt, so they
ride free. The one trade is the bounded ~330ms worst-case pause from the near-stop brake
(vs legacy ~45ms) — by design, and 70× better than the pre-BS5.5 22.6s stalls.
**Recommendation: flip graded to the default admission mode** (keep `STRATA_ADMISSION`
as the escape hatch until M10 hardening); decision is the product call reserved by the
BS5 milestone exit.

## Graded admission becomes the default (BS3.4c decision, 2026-07-07)

The bake-off's recommendation is enacted: `STRATA_ADMISSION` now defaults to `graded`
(`legacy` is the escape hatch until M10 retires the P-controller). Flipping the default
and re-running the storage concurrent-writers matrix as the guardrail exposed TWO latent
saturation defects that legacy's early pacing had always masked — both fixed in the same
change:

1. **Coverage generate-and-defer spin.** When every task the coverage scan enqueues
   defers instantly (saturation interlocks), the drain re-scheduled coverage on each
   empty-queue observation — measured 161–312K generated-and-deferred tasks/s, the churn
   holding the runtime lock the deferred-upon flush/compaction needed. Fixed with
   coverage hysteresis: re-fire only after a real maintenance completion. A companion
   fix makes the pressure-wait slice exhaust its 250ms unless a REAL (lifecycle)
   completion lands — executor step-wakes no longer let a stalled writer cycle
   enqueue→defer→wake at ~16µs.
2. **Flush/rotate budget livelock.** Flush deferred ENTIRELY when rotating the active
   memtable would exceed the FrozenMutable pool — but the frozen backlog those flushes
   would drain is precisely what frees that budget. With 4 writer branches at default
   budgets the pool wedged (67MB/84MB + 16.8MB rotation), every flush deferred (~13/s
   per branch), compaction yielded to frozen pressure, and writers ate the full 30s
   stall-wall watchdog before rejecting. Fixed: flush the existing frozen backlog
   WITHOUT rotating when the rotate budget is short; defer only when nothing is frozen.

**Post-fix matrix (dev box, graded default, medians of 3):** per-writer 1/4/8T =
35.5/30.1/51.0K (the 4/8T cells carry graded's post-window pacing tail in the
denominator; in-window rates match or beat legacy), shared flat ~34.5-35.8K, and the
previously-stalling 4T per-writer cell runs with ZERO stalls and zero deferrals
(was: 3× 30s watchdog timeouts per run). YCSB durable A on the second NVMe: graded
13.1K vs legacy 9.1K (+45%), consistent with the bake-off.

## Consolidated v1 baseline (2026-07-07 end-of-day, post `164f70cf`, dev box nvme0)

One day of levers after the first v1 end-to-end baseline (BS5.5 off-lock GC staging,
graded admission default, coverage-spin + flush/rotate-livelock fixes). Same instrument,
disk, and methodology as the opening baseline. YCSB run throughput (ops/s):

| Workload | cache | durable | durable @ open baseline | durable change |
|---|---|---|---|---|
| A (50r/50u) | 253,320 | **12,975** | 2,642 | **4.9×** |
| B (95r/5u) | 913,552 | **605,371** | 5,451 | **111×** |
| C (100r) | 1,144,900 | 914,326 | 874,907 | par |
| D (95r/5i) | 859,169 | **502,333** | 14,433 | **35×** |
| E (5i/95scan) | 21,400 | 2,435 | 2,290 | par |
| F (50r/50rmw) | 285,693 | **11,992** | 1,927 | **6.2×** |

Durable A update tail: p50 99µs / p99 1.17ms / p99.9 1.54ms / max 250ms (was max 22.6s).

Reading the gaps: the read-mostly workloads (B/D) now sit within 1.5–1.7× of CACHE mode
— reads and light-write pacing are essentially settled. The remaining structural gaps:
(1) write-heavy durable (A/F ~13K, ~20× to cache) = per-commit WAL write + debt pacing on
1KB single-put commits — the known Standard-mode floor, next addressable by WAL
single-write group batching (recorded reopening lever) if product demands it;
(2) scans (E, 8.8×; l9 scan cells) = BS6 territory (readahead, block compression);
(3) reopen replay at ~365µs/row = the replay-probe slice, next up.

## Replay-probe slice (2026-07-07, dev box /data2, medians where noted)

Recovery replay's per-row cost drops **365µs → 52µs (7×)**; the fixed-config 1M
load-then-reopen cell (77MB un-checkpointed WAL, ~185K-row tail) drops **72.5s → 9.66s
(7.5×)**. Three stacked changes, each driven by a fresh profile:

1. **Bounded idempotence probe.** `classify_replay_row` walked the FULL key history
   (`BranchHistoryOptions::all()`) per replayed row. Replaced with
   `BranchReadView::classify_own_internal_row` — own-sources-only (inherited layers never
   hold the branch's own WAL rows), first-byte-equal early exit. Probes 7 → 2.8/row.
2. **Capture-once-per-branch read view.** `CommitReplayRuntime::replay` captured a fresh
   view per WAL record — O(tables) clone+validation per record (~600 records × 8.8K
   tables at the 10M ladder). `replay_with_view` + a per-branch cache in
   `replay_wal_into_catalog`; sound because replayed versions are unique/ascending, so a
   record's (key, version) row can only pre-exist from a pre-crash apply, which the first
   capture observed.
3. **Point-seek instead of decode-all.** The gdb profile showed the remaining wall inside
   `read_data_block_rows` → `decode_table_data_block` — every probe of an already-flushed
   row decoded its whole block (~64 rows) to check one. The probe now uses
   `TablePreparedPointLookup` bounded AT the target version (newest ≤ v == v iff present):
   an in-block point seek, no decode-all. This was the big one: 29s → 9.7s at 1M.

10M full ladder (this tree): fork p50 81.8ms / p95 89.2ms (tail fix holds); reopen
16.58s with a ZERO-row replay tail this run — i.e. the pure BS4.5b O(tables) floor
(8,850 lazy reader opens ≈ 1.9ms each). Follow-ups now cleanly separated:
(a) the table-open floor is compaction debt at close (fewer tables → faster open);
(b) the replay-tail SIZE varies 0–1.5M rows run-to-run with checkpoint cadence at close
— the cadence question flagged at the v1 baseline; (c) residual 52µs/row only matters
after (a)/(b). The old `history_with_source_probe_count` is deleted (replay was its only
consumer).

## 10M three-way: Strata cache / Strata durable / RocksDB (2026-07-07, /data2 nvme1)

YCSB A/B/C at 10M records × 1KB values, 100K ops, zipfian; engine-ycsb vs rocksdb-ycsb
(identical harness shape). RocksDB is the storage-ENGINE reference point (Strata's product
peers are SQLite/DuckDB/Redis; RocksDB bounds the single-writer LSM KV floor).

| | Strata cache | Strata durable | RocksDB |
|---|---|---|---|
| Load (rows/s, batch 1000) | 457K | 85–99K | **1.03M** |
| A (50r/50u) run ops/s | **257,941** | 984 | 337,385 |
| B (95r/5u) | **1,050,928** | 5,108 | 412,603 |
| C (read-only) | **1,576,332** | 3,311 | 423,709 |
| C read p50 / p99 | 591ns / 1.1µs | 65.6µs / 2.05ms | 2.83µs / 4.5µs |
| A update max | sub-ms | **52.8s** (!) | 217µs |

**Reading it.** Strata CACHE beats RocksDB 2.5–3.7× on every run phase (in-memory reads at
591ns) and loses the load 2.2× (457K vs 1.03M rows/s single-writer ingest). Strata DURABLE
at this scale/shape is 80–340× behind RocksDB — the compounding of every gap already on
the books, now measured together at scale: cold-read path with a 4GiB block-cache pool and
no readahead plus post-load compaction-debt tails (BS6 territory; read p50 65µs is fine,
p99 2ms is the debt), the per-commit WAL floor + debt pacing on writes (recorded levers),
and ingest sinking from 330K (100K records) to ~90K rows/s as compaction debt accumulates.
Both engines were measured immediately after load (maximum debt) — a drained-state re-run
would flatter reads on both sides.

**Two defects found by this run:**
1. **Cache-mode budget profile (FIXED same day):** `with_memory_budget(32g)` on a CACHE
   database scaled the durable pool profile — active_mutable got total/8 (4GiB) and 5/8 of
   the budget sat in block-cache/table-reader/artifact pools cache mode can never use; the
   10M cache load hard-failed with `resource_exhausted` at 4.29GB. Cache opens now derive
   a cache-shaped split (`from_total_bytes_for_cache`: active 68% / frozen 20% / artifact
   5% / 1% floors on the durable-only pools) — a cache database's effective capacity is
   ~the declared total, and exceeding it still fails closed with the same typed error.
   Validated: the exact failing config (cache 10M × 1KB @ 32g) now runs A/B/C clean at
   260K / 1.08M / 1.62M ops/s.
2. **52.8s single-commit stall at 10M durable (FIXED same day):** root-caused to the
   NonZeroLevelTableBacklog admission wall. Blocking on Ln shape (16 tables / 4× target
   bytes per level) is STRUCTURAL at scale — L1+ legitimately holds dataset/table-size
   tables while the cascade catches up — and relief needs multi-GB level merges
   (30–60s), so the wall either held one commit 52.8s (watchdog kept resetting on
   unrelated completions) or, when one merge exceeded a 30s window with no other
   completions, fired the watchdog into a caller-visible `failed_precondition` load
   abort (both observed). Fix: NonZeroLevelTableBacklog severity now CAPS at Urgent —
   Ln debt paces writers through the graded ramp (`compaction_debt`), matching the
   RocksDB position that Ln debt is a pacing signal, never a hard stop. The bounded-
   relief walls stay: L0 backlog (one L0 merge) and frozen backlog (a flush drain).
   Validated at the failing config (10M × 1KB, workload A): load completes, update max
   **52,774ms → 425ms**, throughput 984 → 2,045 ops/s; p50 rises to 332µs as pacing
   spreads across commits — the intended trade.

## 10M three-way, post-fixes rerun (2026-07-07 evening, /data2, all fixes in)

Rerun of the three-way with the day's fixes (off-lock GC, graded default + saturation
fixes, replay probe, cache profile, Ln-wall demotion, watchdog liveness). One process per
mode — the both-mode run OOM-killed at 60.4GB anon RSS (cache-phase retention + durable
pools + unaccounted overhead on a 61GB box; RSS-vs-budget gap tracked separately).

| | Strata cache | Strata durable | RocksDB |
|---|---|---|---|
| Load (rows/s) | 388–460K | 82–96K | 1.02M |
| A (50r/50u) | **257,926** | 488–2,045 (see below) | 334,382 |
| B (95r/5u) | **1,049,453** | 4,256 | 403,830 |
| C (read-only) | **1,602,047** | 3,673 | 424,751 |
| C read p50/p99 | 591ns / 641ns | 59.5µs / 3.3ms | 2.83µs / 4.4µs |

Cache is stable run-to-run and 2.5–3.8× over RocksDB on every run phase. Durable
workload A varies WILDLY across runs (984 / 2,045 / 488 ops/s; update max 425ms /
48.4s): with the Ln wall demoted and the watchdog no longer aborting, the remaining
disease is isolated — **the L0-blocking wall's relief is one L0→L1 compaction pass, and
at 10M a single pass rewrites GBs of L1 overlap (~50s)**. A writer that hits an
L0-blocking episode while a giant pass runs now waits it out legally (48.4s max this
run) instead of aborting (the pre-watchdog-fix behavior) — better, but the pass size is
the root cause. **Identified next slice: bounded L0→L1 passes** (trim the L0 input set
per pass so relief is incremental and seconds-scale) — correctness-sensitive (partial
L0 consumption must respect table recency ordering), needs its own design pass.

Watchdog fix recorded here: a RUNNING maintenance task now counts as liveness (the
stall watchdog fires only on a provably dead executor — zero completions, no backlog
reduction, AND no active task). Before it, one >30s giant pass with no other
completions converted a busy executor into a caller-visible `failed_precondition`
load abort (observed twice: NonZeroLevel pre-demotion, L0 post-demotion).

## T4 attribution + allocator-linkage caveat (2026-07-07 evening, v2 branch)

**Evidence-base caveat:** every engine-level bench bin except
`storage-next-concurrent-writers` was silently running GLIBC MALLOC — the jemalloc
`#[global_allocator]` lives in the benchmark lib crate and unreferenced libs don't
link. All engine-ycsb rows above (three-ways, stall investigations) are glibc-measured:
internally consistent (deltas stand), absolute values carry the confound. Fixed on the
v2 branch (`1279809b`): every bin force-links the lib; the probe prints live
`tikv-jemalloc-ctl` gauges per phase.

**T4 verdict (jemalloc truly active, YCSB A durable 10M @ 32g):** the RSS runaway is
APP-HELD — post-load allocated=13.66GB (block cache filling its 15GiB pool), post-run
**allocated=39.25GB ≈ resident=41.84GB**: +26GB of live heap accumulated during the run
phase's compaction churn. Stalls persist under jemalloc (max 56.3s) — the allocator
caused neither the 61GB OOMs nor the stall lottery. Eliminated by inspection: rewrite
outputs lazy-reopen (BS4.4l), block cache evicts per shard. The heap profile NAMED it (peak
dump: 25.75GB live): **20.57GB in `ImmutableTableStreamingEncoder` output buffers** —
compaction/flush builds hold every output table's complete encoded bytes (+ rows) in
heap until the pass publishes; concurrent builds × unbounded mid-level pass outputs =
the 26–50GB peaks, both OOMs, and (via page-cache eviction) the stall physics. Fix
slice W1.2c: stream outputs to disk per completed table inside the build loop. Key
cells re-baseline under jemalloc once landed.

## W1.2c landed: memory term (T4) closed for compaction; stalls persist (2026-07-07, `2399d7b1`)

Streaming output publish (sink through `compact_inputs_into` →
`prepare_branch_compaction_plan_bounded_into` → `build_range_with_publishing_sink`;
each completed output table publishes and frees inside the build loop). Same cell,
before → after (YCSB A durable 10M @ 32g, jemalloc gauges):

| gauge | pre-W1.2c | post-W1.2c |
|---|---|---|
| post-load allocated | 13.66GB | 12.25GB |
| post-run allocated | **39.25GB** | **16.92GB** |
| post-run resident | 41.84GB | 17.98GB |
| post-run retained (VM high-water proxy) | **37.65GB** | **5.25GB** |
| load / run ops/s | 120,594 / 610 | 115,693 / 697 |
| update max | 56.3s | **46.1s (persists)** |

The +26GB run-phase heap accumulation and the ~50GB VM peaks are GONE — resident now
tracks block cache + memtables; both 61GB OOM modes are physically impossible at this
shape. Flush-side accumulation (~2.5GB) is a recorded smaller follow-up (same sink
pattern through the flush build). **Stall attribution is now fully open again:** pass
size (W1.1), parallelism (W1.2a), and memory (W1.2c) are all eliminated as causes of
the 40–70s update max. Per the W6 rule, next action is a live stack sample of a stall
window — no scheduling/memory slices until the stacks name the holder.

## Stall root cause NAMED by stack sample (2026-07-07)

gdb-sampled YCSB A durable 10M reproduced the stall under observation (max 45.9s =
one blocked commit; `block_wait_ms=46301`). 17 consecutive samples: the writer waits
in `execute_commit → wait_for_progress` (L0 wall) while at most 2 of 4 workers run
giant mid-level pass builds and the rest idle. L0→L1 relief is excluded by the
dispatch conflict rule `same branch && level.abs_diff <= 1` (an in-flight L1→L2
conflicts with L0→L1) — and the L1→L2 passes are monsters because L1 tables are built
with unbounded L2 (grandparent) overlap. Full chain and fix design in
`v2-w1-compaction-engine-plan.md` § stack sample. **Next slice: W1.3a
grandparent-overlap-bounded output cutting** (bounds every future mid-level pass,
collapses the conflict window from ~50s to seconds). Not lanes, not locks (fg lock
wait 92ms — BS5.5 holds), not memory (W1.2c gauges stable this run too).

## W1.3a landed: grandparent-overlap output cutting kills the stall lottery (2026-07-07)

Every table-rewrite pass now cuts its output tables so each overlaps ≤256MiB of the
level below the output level (RocksDB `max_compaction_bytes` analogue), bounding every
FUTURE mid-level pass and with it the level±1 conflict window that held L0 relief.
5× YCSB A durable 10M: update max **3.0/4.6/3.2/3.6/1.8s** (was 21/55/1.1/50/1.8s and
45.9–56.3s), block_wait totals 2.2–4.6s (was 46.3s in one episode), run throughput
median 1,419 ops/s (was 610–802), memory line unchanged (~16.8GB post-run). Residual
seconds-scale max = one bounded pass's relief window (calibration: W1.4; granularity:
W1.2b). Load dipped to 85.7–107K on some runs (cutting write-amp) — W1.4 input.
Design + full gate table in `v2-w1-compaction-engine-plan.md` § W1.3a.

## 10M three-way, post-W1 re-baseline (2026-07-07 night, v1 @ 4dfcc376, /data2, jemalloc)

First three-way with the engine bins genuinely on jemalloc (prior absolutes were
glibc-confounded) and the first on the merged W1 compaction engine. One process per
Strata mode; RocksDB re-run same-day as control.

| | Strata cache | Strata durable | RocksDB |
|---|---|---|---|
| Load (rows/s) | 449–459K | 97–117K | 635–958K (control noise; see note) |
| A (50r/50u) run ops/s | **273,103** | 1,036 | 303,868 |
| B (95r/5u) | **1,082,049** | 5,017 | 436,577 |
| C (read-only) | **1,654,098** | 6,778 | 426,083 |
| C read p50 / p99 | 571ns / 631ns | 66.8µs / **930µs** | 2.83µs / 4.4µs |
| A update max | 79µs | **3.26s** | 205µs |
| durable post-run allocated | — | 16.7–17.0GB | — |

**W1 verdict at the three-way level.** Durable is transformed from chaotic to
predictable: A max 3.26s (was 425ms–48.4s lottery; in-family with the W1.3a gate's
1.8–4.6s bounded relief window), block_wait 3.4s total, zero wait timeouts, memory
flat at ~17GB across all three workloads (retained gauge grows across workloads only
because one process runs A→B→C; per-load allocated returns to ~12.4GB). Read-heavy
run phases nearly doubled: C 3,673 → **6,778**, B 4,256 → 5,017, and C read p99
improved 3.5× (3.3ms → 930µs) — bounded shape means no monster passes evicting page
cache mid-run. C read p50 ticked 59.5 → 66.8µs (more, smaller tables = slightly wider
probe fan-out — a W2 input, not a regression story). Durable load 97–117K vs 82–96K
pre-W1: the cutting write-amp did NOT regress load at three-way conditions.

**Control note:** RocksDB A/B loads measured 635–647K vs 1.02M in prior rounds (C
load 958K ≈ prior); run-phase numbers match prior rounds (A 304K vs 334K, B 437K vs
404K, C 426K ≈ 425K). Same binary, same volume — load variance is environmental
(cold page cache on first workloads); Strata comparisons are same-day, same
conditions.

**The post-W1 gap (what W2 must close), durable vs RocksDB-default:**
A 293× / B 87× / C 63×; load 6–9×. The stall/memory terms are spent — the gap is now
owned by the read path: C p50 66.8µs vs 2.83µs (24×) and p99 930µs vs 4.4µs (211×)
with a 15GiB block-cache pool that should hold the zipfian hot set. Roadmap targets
(A ≥230K, B ≥280K, C ≥290K) require ~34–64× on reads — next action per W6: read-path
profile on this exact shape (block-cache hit accounting, probe fan-out, decode cost)
BEFORE any W2 slice.

## Read-path profile: the 66.8µs median read, fully attributed (2026-07-07 night)

perf-trace read counters added to `engine-ycsb --perf-breakdown` (`[probe] point /
table read / table io` lines). Durable C at 10M: p50 = a **254-entry full-block walk**
(blocks are ~256 rows ≈ 280KB because the encoder cuts on rows only — the 64KB
`target_data_block_size` never gates); p99 = **0.33 cache misses/read × 279KB** block
reads; **3.63 table seeks/read with zero bloom-filter probes** (BS4.3 filter machinery
is config-gated dark). Hit rate 69.7% is churn-limited (116 rewrites during the 14s
run), not capacity-limited. Full anatomy + revised W2 slices in
`billion-scale-roadmap-v2.md` § W2. Model closes: 254 × ~240ns ≈ 61µs vs 62.4µs
measured — no flamegraph needed.

## W2.1: block byte target enforced — read median halved; B variance trade (2026-07-07)

The 64KB `target_data_block_size` now gates the streaming block cut (was rows-only →
~280KB blocks). Durable 10M before → after: C read p50 62.4 → **29.2µs**, lazy walk
254.5 → **58.7** entries, per-miss I/O 279 → **64KB**, C run 6,953 → **11,272 ops/s**;
loads unchanged; A unchanged (952 ops/s, max 4.6s). Trade discovered by the confirm
sweep (n=3 B runs): B now draws occasional L0-wall episodes (2.3–3.9s) it didn't
pre-W2.1 — floor 2,077, good draw 5,037 (read p50 52µs vs 134 pre) — build wall-time
varies 2× across identical B runs (29.9s vs 58.4s for ~same task count), pointing at
per-block build/read fixed costs × 4.3 more blocks. Recorded as the W1.4/W2.1b
calibration input (128KB A/B or per-block cost shave). Cache hit rate note: C hits
dropped to 50% (more distinct blocks for the same hot keys + unchanged churn
invalidation) — W2.4's churn-surviving cache matters more at 64KB.

## W2.2: bloom filters on — L0 probe amplification absorbed (2026-07-07 night)

Flush + compaction builds persist 10-bits/key filters; the reader's lazy point path
already consulted them. Durable 10M single-process C→B→A run: **B 5,158 ops/s / read
p50 31µs / update p99 175µs / zero L0-wall episodes** (post-W2.1 lottery was
{5,037/2,982/2,077} with p50 52–134µs and 2.3–3.9s walls); **A 1,696 ops/s (best on
record) / read p50 43.9µs** (was 255µs), one bounded 4.5s wall. Negative filter
probes 554K (B) / 2.33M (A) — block-scans per read on B fell 15.7 → 0.87. C
unchanged (28.4µs p50; losers die at range checks pre-filter; throughput 7,594 this
draw — miss/churn-bound, W2.4's lever). Memory line holds (~16.9GB post-run;
filters ≈ 82KB per 64MB table, budget-charged via resident metadata).

## W2.4: publish-time cache warming — read-tail collapse (2026-07-08)

Rewrite + flush publishes now warm the block cache from the just-encoded bytes
(no-evict `insert_if_free`; fresh blocks never displace demand-cached ones; full
shards skip — scale-safe). Durable 10M C→B→A vs post-W2.2: C **11,455 ops/s**
(campaign best; was 7,594) read p99 **704 → 430µs**, p99.9 2.28 → 1.17ms; A read
p99 2.23ms → 827µs; B read p99 1.60 → 807µs; read p50s ~26-32µs across all three.
SkippedFull 34K (C) / 107K (B) / 382K (A) — the no-evict gate works under churn.
Cache pool now actually utilized (post-run allocated ~19GB, resident ~21GB, within
32g). Recorded: two low load draws (80-87K vs 99-115K band) — W1.4 input; blended
hit-rate counter mixes compaction cursor traffic (split before quoting).

## W1.4: pacing calibrated — the throttle stops punishing writers for structural debt (2026-07-08)

Attribution chain (new `[probe] pacing` line): (1) the token bucket slept **101.7s of
workload A's 134s run — 76% of wall** — with the rate pinned at ~149KB/s against
942MB of debt that was almost entirely post-load structural overage; (2) excluding
nonzero-level overage below budget/8 (`pacing_debt`, RocksDB
soft_pending_compaction_bytes_limit analog) cut sleeps to 37–86s but the rate still
walked low — the remaining debt was L0 *load residue* and the ±20-25%-per-install
multiplicative walk is noise-driven at ~1/s cadence; (3) replaced the walk with a
**proportional-quadratic controller on L0 depth**: max rate below the urgent count,
`min + (max−min)·h²` through the band, floor at blocking−2. Deterministic,
self-correcting, walls unchanged as backstop.

Results (10M durable): **A 3,059 / 2,926 ops/s** (was 746–1,696; update p99 44.3ms →
**4.2–5.2ms**, pacing sleeps 101.7s → 18.5–24.3s, maxes 1.7–2.8s bounded);
**B 5,903 — best on record** (zero pacing delays, rate rode the 16MiB/s ceiling,
update max 692ms, zero wall episodes). Floor events 17–50 per A run show the brake
engaging only near the wall, as designed. Saturation gate (post-commit, same night): `storage-next-concurrent-writers` at
default budgets — **PASS**. Standard shared 33.3–34.2K at 1/4/8T (BS5 family ~35K),
per-writer 8T 45.7K with tight fairness (16.8–17.3K/writer); Always 158/551/1,046 at
1/4/8T (BS5 gate family: 159/553/1,117); **zero stalls, no hangs, no watchdog
aborts** at any point. Two non-blocking notes: per-writer 8T is ~14% under the 53.2K
BS5.4 baseline (W2 write-path additions + 3s windows — re-baseline at the next
concurrency slice), and one 4T per-writer draw showed a transient fairness spread
(29K vs 52K; 8T tight, so not systemic starvation).

## W3.1 complete: the timeline is derived, not materialized (2026-07-08)

Three slices (index cache → checkpoint persistence → elision), each proven by
oracles before the next. Single-put commits write **1 row instead of 3**; WAL
payloads shrink accordingly; `as_of` resolution is O(log commits) against a
retained per-branch index whose completeness is invariant at every open (fresh,
recovered-with-section, pre-elision-rows bridge, forks). A durable 10M:
**3,000 → 4,884 ops/s**, update p99 **4.2ms → 1.13ms**, max **879ms** (first
sub-second A). Load in-band (~81K — batched load only carried 0.2% timeline
overhead; the win is single-put commits). Details + O2 bounds wording in
`v2-w3-write-path-plan.md`.

## W3.2 resolved by measurement: the solo path is already lean (2026-07-08)

Attribution closed the slice instead of surgery. With new probes (grouped
dispatch envelope, drain notify, batch clone, pressure collection) the
commit-path anatomy at 100k/A: **true path ≈ 11.5µs** — wal_append 3.9µs (one
`write()` per commit → W3.3's coalescing carries the ≤8µs target), apply 1.4,
admit 1.3, post_maint 1.2, notify 0.9 (wanted drain-kicking, wake-bit-coalesced
at cap), fragments ~2.8. The roadmap's levers were already spent: group
bookkeeping ≈ 0, encode buffers at 0 allocs / 997K reuses; the batch-clone
theory measured dead at 0.12µs. Policy waits — pacing ~7µs/commit at 100k,
**87µs/op at 10M (42% of A's wall)** — dominate A at scale; commit-path µs do
not move A@10M, the M-A/M-B lanes do. Shipped: `api_commit_runtime_ns` no
longer wraps the post-completion policy sleeps (it misattributed pacing as path
cost twice this workstream); `CommitTimelineRows` construction is now
test/testkit-gated (production structurally cannot stage timeline rows);
probes are permanent. Pressure collection runs 3×/commit — 0.27µs/call on a
100k shape, 0.42µs/call at 10M (140 tables/call, 1.3µs/commit total): the
O(levels×tables) walk has a tiny constant, so no epoch-cache slice until the
~100M tier (~750-table bottom levels). Validation cell (A durable 10M, single
run): **5,343 ops/s** vs 4,884 baseline — no regression; update p50 58.2µs,
max 752ms. At 10M the fixed timer decomposes exactly: in-path 54.6µs/commit =
dispatch 30.2 (stages 14.7, of which wal_append 7.7 + fg lock wait 12.6) +
rare pressure block-waits 22 + notify 2 — wal_append nearly doubles vs 100k
and lock waits against busier maintenance sextuple, reinforcing W3.3 and the
M-A/M-B lanes as the carriers at scale.

## W3.3a: WAL append coalescing — the 150K single-put gate passes (2026-07-08)

One user-space buffer turns per-commit `write()` syscalls into one coalesced
write per 128 KiB (measured 114 appends/flush on A). Control-first A/B
(control = `ce68b5ef` built in a temp worktree, same box/config, medians of 3):

- **l9 single-put Standard 100k (the roadmap exit cell): 129,178 → 162,121
  ops/s (+25.5%) — the ≥150K gate PASSES.** Tight cells (±0.6% / ±0.7%).
- A@100k: wal_append stage 3.7 → 2.7µs/commit, update p50 25.7 → 22.6µs.
- A@10M stage probes: wal_append 7.7 → 1.8µs/commit; best load yet (87.7K
  rows/s) and best run yet (8,289 ops/s at pacing≈1s).

**Protocol finding — the A@10M run phase is a settling lottery.** Three
identical treatment invocations: run = 8,289 / 3,464 / 2,784 ops/s with pacing
1.0s / 19.3s / 23.7s — the run phase measures whatever L0/compaction backlog
the load left behind (worse when the load runs faster), not steady state.
Every historical A@10M run-phase number, including the 4,884/5,343 baselines,
sits inside this lottery. engine-ycsb gained `--settle-secs` (drain window
between phases); the next re-baseline (W6/M10 discipline) should use it.

Semantics: power-loss exposure unchanged (last barrier); orderly teardown
keeps page-cache parity via flush-on-drop (the battery's crash-sim harnesses
demanded ZeroLoss on drop-without-close and got it as a product fix — zero
oracle re-basing); abrupt kill loses ≤ one buffer (trickle flush = W3.3b,
task #82). Differential oracle pins byte-identical segments + identical growth
facts vs the direct path at every durability boundary. `Backend{Flush}` errors
are durability-uncertain; pending is never discarded; partial flush writes are
caught by handle re-open reconciliation (no blind re-append).

## SETTLED 10M baseline — first trustworthy A/B/C numbers (2026-07-08, post-W3.3)

Protocol: `engine-ycsb --workload a,b,c --durable --records 10m --ops 100k
--memory-budget 32g --settle-secs 120` — each workload gets a fresh load, then a
120s drain window before the measured run. The window works: pacing collapsed
from the lottery's 1-24s to 0-1s across all nine cells (A/B/C x 3 runs).
Medians of 3, spreads noted; v1 @ `ca3b1a11` (all of W3 in):

| Cell | Settled median | Spread | Old unsettled row | RocksDB | Gap |
|---|---|---|---|---|---|
| load 10M (batched) | **103K rows/s** | 91-119K | ~81K | 660K | 6.4x |
| A run | **9,127 ops/s** | 7.7-15.5K | 4.9-5.3K | 304K | 33x |
| B run | **7,595 ops/s** | 7.5-8.2K (tight) | 5.9K | 437K | 57x |
| C run | **7,150 ops/s** | 5.3-7.6K | 11.5K (flattered) | 426K | 59x |

Latency shape (settled): read p50 24-26us everywhere; read p99 ~460us at good
shape (2ms when the load leaves a worse level shape — A retains a 2x run
spread even settled, driven by read-tail differences, i.e. SHAPE lottery
survives the DEBT lottery fix); update p50 36-42us, **update p99 70-77us,
p99.9 88-141us** on clean runs — the W3 write path is essentially flat now.

Readings:
- The old C row (11.5K) was flattered by unsettled conditions; honest settled
  C is ~7.2K. C's mean (135-140us) is 5.4x its p50 (25us): the zipfian head is
  cache-hot at 25us, and the cold middle (block-cache misses to disk) carries
  the mean — this is the api-vs-storage read split's attribution target.
- B ~= C: the 5% writes cost nothing now; both are read-bound.
- Load 103K median is the best yet (W3.3a coalescing); load itself needs no
  settling (91-119K spread is shape/IO variance).
- A@10M went 4.9K -> 9.1K median across W3 (settled-to-settled would be
  cleaner, but no settled pre-W3 control exists; directionally consistent
  with the per-commit wins).

## B1 (W2.6) trusted block reads: landed, measured ~NIL at 10M C — estimate falsified (2026-07-08)

Cache hits (79% of block reads, counter-verified) now skip the per-read CRC32
+ 64KB payload copy; verification moved to admission (demand path already
verified pre-insert; the W2.4 warm path now verifies before insert — a
design-review finding). Equivalence property + taxonomy mirror + fuzz target
pin trusted == checked minus only the stored-CRC class. All 44 targets green.

**Same-session interleaved A/B (settled C, 10M, 500K ops): no measurable win.**
Control 25,357/15,483 vs treatment 14,664/15,873/15,733 ops/s — within the
shape lottery; read p50 UNMOVED (21.2-21.6 control vs 20.8-21.3 treatment).

Why the audit's 8-15us estimate was wrong (recorded for the method ledger):
the gdb profile's crc32/memcpy samples could not distinguish hit-path from
miss-path work — and B1 deliberately KEEPS the miss-path CRC (verify before
insert). The median hit's CRC+copy was evidently <=1us (hot blocks are
CPU-cache-resident; hashing them is cheap). C's mean is dominated by the
21% miss rate x ~300us disk reads, which a hit-path shave cannot touch.
Fifth falsified lever this workstream (batch clone, encode buffers, group
bookkeeping, hit-CRC) — control-first A/B keeps paying for itself.

Where the hot p50 actually lives (probe data from the treatment runs):
**28.5M entries scanned / 500K ops = 57 linear entry-scan steps per read**
(~12-17us of the 21us p50) — that is B3 (restart points, W2.3). The miss
side is B2 (64KB per miss, miss RATE from blocks-per-pool-byte). B1 stays
(strictly removes wasted CPU, hardens cache admission, prerequisite for an
honest B2 sweep), but the C levers are B3 then B2.

## B3 (W2.3): entry-offset accelerator — hot read p50 HALVED (2026-07-08)

Trusted seeks bisect a derived per-block entry-offset index (cached under the
previously unused Accelerator kind, ~260B per 64KB block) instead of walking
~57 entries linearly. Zero durable-format change (payload is M3-frozen; the
index is derived at admission/first-hit from verified payloads). Same-session
INTERLEAVED A/B (settled C, 10M, 500K ops, T1-C1-T2-C2-T3):

| | read p50 | run ops/s |
|---|---|---|
| Control (B1) | 21.04 / 21.31µs | 12,766 / 13,744 |
| **Treatment (B3)** | **9.83 / 10.07 / 9.84µs** | **16,514 / 14,317 / 17,050** |

**p50 21.2 → 9.8µs (−54%)** — the predicted 8-13µs band, first single-digit
hot read. Every treatment run beat every control run on throughput too
(median 16.5K vs 13.3K, ~+24%) — the bisection also trims the mean's hit
term. Counters airtight: indexed == trusted in every run (395-400K), zero
self-heal rebuilds, ~117K builds (misses + first hits).

Read-path arithmetic now: hot p50 9.8µs ≈ engine-layer tax (B4: 4 value
copies, 5 key copies, branch-by-string lookup, ~1-2µs) + probe/bisect/window
+ per-op fixed costs. C's MEAN remains miss-IO-dominated (~20% × 64KB) — B2
(block-size sweep) owns the next throughput bite; B4 owns the next p50 bite.

## B4: move-don't-copy point reads — landed, measured ~nil; the C picture is now miss-IO (2026-07-08)

The read path sheds 3 value copies, 3 key copies, and the per-read branch
record clone (moves through every layer; parity-asserted refactor, both crate
batteries green). Same-session interleaved A/B (settled C, T1-C1-T2-C2-T3):
control p50 9.97/10.37us vs treatment 10.65/10.79/10.77us, throughput
15.5-16.5K both sides — **no measurable movement**. Second B1-class outcome:
the audit's 1-2us engine-tax estimate was high, and the new
api_read_point_runtime_ns probe (Phase A of this slice) shows why — the
engine layer above the storage api costs only **~0.3-1us mean**; the layers
were already thin. B4 stays as hygiene (fewer allocs/copies = CPU + allocator
headroom, matters under concurrency and bigger values), cost ~0 risk.

**The standing C@10M picture the probe pair now gives** (mean ~62us/read):
- hit path (79%): ~10us p50 -> ~8us of the mean
- miss path (21%): ~250us each -> **~52us of the 62us mean**
B2 (block-size sweep: 64KB per miss, miss RATE from blocks-per-pool-byte) is
now overwhelmingly the C-throughput lever. The hit p50's own next terms
(bisect+window ~1-2us, candidate decode, cache shard ops, source-walk
machinery) need a fresh stack profile if a sub-5us hit becomes the goal.

## B2: data-block size calibrated — DEFAULT FLIPPED 64 KiB -> 16 KiB (2026-07-08)

The knob went production-configurable (per-database, options-validated
4KiB..=1MiB, durable-only), then the sweep ran on settled C at 10M (fresh
load per point, medians of 3 with interleaved fine pass):

| block | C run (median) | read p99 | miss IO / 500K reads | misses | RSS post-load |
|---|---|---|---|---|---|
| 64 KiB (old default) | 15,519 | 434-452us | 6.4-6.7 GB | 103-107K | 20.4-20.6 GB |
| 32 KiB | 15,668¹ | 339us | 3.8 GB | 120K | — |
| **16 KiB (new default)** | **18,960 (+22%)** | **295-297us, sub-1% spread** | 2.0-2.2 GB | 123-131K | 22.5 GB |
| 8 KiB | 16,305 | 218-618us (erratic) | 1.2 GB | 134K | 18.7 GB |

¹ single coarse run.

Winner checks at 16 KiB: **B improves too** — 21.5K/19.1K vs control 16.0K
(+20-34%; the W2.1 "smaller blocks hurt B" variance concern inverted — B is
read-mostly and the read win dominates), update max 28-30ms vs control's
604ms draw. A@100k parity (24.1K vs 23.3K, better p99.9/max). Load within
the historical spread. Index metadata at 16 KiB ≈ 75MB at 10M (4x the 64KiB
cost, well within budget); 8 KiB was rejected for erratic tails despite the
lowest miss IO.

Read p50 stays ~10.5-11.5us (hit path unchanged — B1/B3/B4 territory). The
C mean's miss term shrank from ~52us to ~30us; the remaining gap to the
290K target is now split between miss RATE (cache-pool sizing / W2.4b
heat-aware admission), the ~10us hit path, and engine-level concurrency
(single-threaded cells throughout).

Subsumes task #72 (W2.1b calibration). Sweep protocol: settled C
(`--settle-secs 120`), `--block-bytes` per point, same-session interleaved.

## 10M three-way, post-campaign (2026-07-08 night, v1 @ f5940ddd, /data2, settled durable)

The first three-way after the full W3 + read-path campaign (16 KiB blocks,
settled durable protocol; single runs per cell — treat run cells as point
samples per the standing caveat):

| cell | Strata cache | Strata durable | RocksDB | durable gap | gap at campaign start |
|---|---|---|---|---|---|
| load 10M | 431-445K rows/s | 82-111K | 764K-1.0M | ~9-11x | — |
| A run | **353,732** | 17,754 | 286,871 | **16x** | 62x |
| B run | **1,167,803** | **54,509** | 436,638 | **8x** | 74x |
| C run | **1,564,172** | 19,475 | 428,300 | **22x** | 37x |

- **Cache mode beats RocksDB on every run cell** (reads ~600ns p50) — the
  memory-first story holds.
- **Durable B 54.5K is a new best by 2.5x** and exposes a mechanism worth
  keeping in the model: under zipfian A/B the hot keys are continually
  UPDATED, so their newest versions sit in the memtable and hot reads never
  touch the block path; C pays the full table path on every read. B's read
  p99 was 177us vs C's 294us.
- Durable read tails post-B2: B p99.9 230us, C p99.9 318us. A's update max
  drew 1.09s (the surviving shape lottery — W1.3's territory).
- RocksDB measured 287/437/428K — consistent with the standing reference row
  (304/437/426K).

## C1 — fair-baseline re-base: compression hypothesis falsified (2026-07-08, b65207e5)

Protocol slice (`c-campaign-60-70.md` C1): both harnesses now default to
**incompressible values** (`ValueFill::Random`, unique splitmix64 payload per
value; `--value-fill constant` kept for historical comparability), rocksdb-ycsb
gained `--compression default|lz4|none`, and both bins print an on-disk
post-load probe (engine-ycsb also post-settle).

**The compression-flattered-reference hypothesis is falsified.** Fill ×
compression matrix, RocksDB 10M × 1KB, 500K-op C:

| fill | compression | on-disk | run C | read p50 | read max |
|---|---|---|---|---|---|
| constant | default | 9.59GB | 435K | 2.16us | 30.8us |
| random | default | 9.59GB | 428K | 2.29us | 34.2us |
| random | none | 9.59GB | 434K | 2.19us | 44.6us |
| random | lz4 | 9.55GB | 425K | 2.18us | 232us |

Stock rust-rocksdb writes uncompressed; all cells within 2.4%. RocksDB's C
edge is **full page-cache residency** (read max ~34us across 500K reads =
zero disk misses on a raw 9.6GB dataset), not compression.

Re-based reference and parity cells (same session, interleaved):

- **RocksDB C reference: 432K median** (428/432/448K), read p50 ~2.2us,
  9.59GB on disk.
- **Strata durable settled C, random fill: 19,244** (p50 12.45us, p99
  295us) — inside the constant-fill band from the same session
  (18,658-19,475). Value content costs nothing on the durable path.
  Durable C gap: **22.4x**. The same-session constant control drew a
  degraded shape (9.8K, p99 2.18ms, api_point_ms 2x — background compaction
  overlapped the run window; shape lottery, not fill).
- **Strata cache C: no fill effect** — interleaved constant 1.51M/1.53M vs
  random 1.56M/1.43M (p50 611-671ns). A one-off 876K cache draw earlier in
  the session was state, not protocol (2.5GB swap in use after the durable
  marathon).
- New probe, durable 10M load: on-disk **37GiB post-load → 12GiB
  post-settle** — the load's transient churn (WAL + L0 + intermediate
  tables) is ~3x the settled dataset, quantifying the page-cache eviction
  mechanism behind C's ~150us misses (campaign doc Finding 2). Settled
  Strata dataset 12GiB vs RocksDB 9.59GB (+25%, uncompressed both).

Also: cleaned 229GB of stale engine-ycsb tempdirs (killed runs skip tempfile
cleanup) from /data2/strata-bench/ycsb10m/.benchmark.

C-campaign standing math after C1: target 60-70% of 432K = **259-302K**;
today 19.2K = h 0.75 x 12.5us + 0.25 x ~110-150us. C2 (fill the 15GB pool →
h≥0.98) then C3 (allocation-free hit path → ~3.5us) carry the plan.

## C2 — background block-cache preheat (2026-07-09, 877d30ba + 365eba9d)

`c-campaign-60-70.md` C2: fill the pool so C stops missing to disk. New
`CachePreheat` low-tier maintenance (dirty-flag armed by table installs and
reopen — never a standing queued task), off-lock 128 MiB chunks walking live
tables deepest-first through `warm_data_blocks_from_source` (verify-then-
insert, recency-neutral presence probe, cursor resume), sweep-time dead-table
cache invalidation (`remove_table` had zero production callers), and
recovery-time walks moved to no-fill cursors (reopen is now cache-neutral).
Knob: `StorageCachePreheatPolicy` / engine `CachePreheat` / bench
`--preheat on|off` (default on).

Two measured design corrections: no-evict preheat inserts starved against
dead-block pool pollution (full-shard skips at ~40% live occupancy → fair
inserts; LRU displaces never-touched dead blocks first), and a saturation-
stopped chain never resumed in a quiet settle (sweep completion now re-arms).

Settled C, 10M x 1KB, 500K ops, 32g, settle 120s, same-session interleaved,
medians of 3:

| cell | run ops/s | miss rate | read p50 | read p99 |
|---|---|---|---|---|
| preheat off | **18,818** (16.1-18.9K) | 24-26% | 10.4-10.7us | 298-644us |
| preheat on | **56,919** (54.9-58.1K) | **5.5-6.1%** | 9.3-10.4us | 204-217us |

- **C x3.0** (18.8K → 56.9K). Preheat walks ~8.9GB in ~70 chunks during
  settle (zero full-shard skips after the fair-insert fix), ~350-410
  build-active/pressure deferrals absorbed harmlessly.
- **ON cells are stable** (±3% spread vs the historical ~2x cell lottery) —
  a full cache removes load-shape luck from the read path.
- Regression cells (single runs): A on 29,356 vs off 6,099 (off drew a
  lottery-bad cell; on is the best A ever recorded — 50% of A's ops are
  reads), B on 53,386 vs off 21,199, read p99 improved in every cell, no
  load regression beyond spread.
- **Residual ~5.5-6% miss floor is structural**: settle-300 converges to the
  same rate (55.8K, 6.0%), so it is not the late-compaction tail. Anatomy
  (which blocks still miss with full walk coverage) → follow-up, folded into
  C3's profiling pass.
- Exit gate: miss ≤2-5% BORDERLINE (5.5-6.1%), C ≥ 60-80K NOT met (56.9K)
  — the gate assumed mean ≈ H at h→1; the 6% floor at ~150us/miss costs
  ~8-9us of the 17.6us mean. C3 (hit path ~10us → 3.5us) now carries the
  260-300K target with h at 0.94: projected mean ≈ 0.94x3.5 + 0.06x~120 ≈
  10.5us ≈ 95K without the floor fixed; **the miss-floor anatomy is
  therefore on C3's critical path**, not optional.
- Pre-existing (NOT this slice): 14 storage-next lib tests fail under
  `--features perf-trace` at the C1 baseline too (commit/api perf-count
  asserts); tracked for a separate fix.

## C3a — preheat coverage convergence: the miss floor to ZERO (2026-07-09, d45eb2af + d6fde965)

`c-campaign-60-70.md` C3, commit 1: kill C2's residual ~6% miss floor and land
the probes that make cache claims checkable. Two mechanisms, both found by the
new probes rather than by theory (two prior theories falsified en route):

1. **Dead-block pool pinning.** The capped quarantine sweep leaves ~260K
   dead-table blocks in the pool; the occupancy gauges showed 15.0/15.0 GB
   "full" with the live set at ~10.7 GB. Fix: `TableBlockCache::
   retain_tables(live)` purge at fresh-pass START and pass COMPLETION —
   the cache converges to exactly the live set.
2. **Pressure self-throttle.** Preheat gated on global memory pressure that
   its own fill raised, freezing coverage at ~85%. Fix:
   `global_pressure_excluding(cache bytes)` — the pool is self-evicting and
   budget-bounded, so preheat now gates on non-cache pressure only (80%
   high-water).

Plus the trigger/chaining state machine (`rearm`/`cursor`/`paused`): a
mid-pass table install now survives to a follow-up fresh pass (C2 consumed
the trigger at every chunk start, permanently skipping tables sorted before
the resume cursor); deferrals pause keeping the cursor (resume ≠ restart).

New probes (perf-trace): table-cache eviction counter, bytes/entries/capacity
occupancy gauges (gauges excluded from `reset()`), warm-publish SkippedFull
counter, `last_pass_blocks` gauge, and a bench-side jemalloc
`[probe] alloc: bytes_per_op` around the run loop — the C3b gate.

Settled C, 10M x 1KB, 500K ops, 32g, settle 120s, medians of 3:

| cell | run ops/s | miss | read p50 | read p99 |
|---|---|---|---|---|
| C2 close-out | 56,919 | 5.5-6.1% | 9.3-10.4us | 204-217us |
| C3a | **103.0K** | **0 / 500K** | 8.9-9.7us | **~14us** |

- evict=0, cache 10.74/15.00 GB = exact live set (820K entries), post-run
  preheat idle. The ms-scale read max disappears (best cell max 238us).
- Settle-300 converges identically → convergence is trigger-driven, not
  time-driven. Exit gate (miss ≤1%) PASSED with margin.

## C3b — hot-read allocation strip (2026-07-09, 96c40909 + a350f464 + c497cc95)

`c-campaign-60-70.md` C3, commit 2: strip the ~13-15 heap allocations per hit
read down to the necessary ones. Landed, each wire-neutral (format goldens
byte-identical):

- **B1** `EntryOffsetsView`: bisect probes the cached accelerator bytes
  in place (checked LE u32 reads) — no per-seek `Vec<u32>`; shape-mismatch
  still self-heals via rebuild.
- **B2** version-only visited-entry parse (`internal_key_commit_version`,
  trusted indexed path only; the checked walk keeps full decode) +
  `decode_storage_row_matching_key` byte-compares the row's embedded key
  region (canonical encoding → strictly stronger than decoded equality).
- **B3** single escape-encode per read: the prepared lookup's physical key
  IS the seek-key prefix slice; the bloom-probe chain takes `&[u8]`.
- **B4** `TablePointLookupRow::Owned(StorageRow)` — the lazy hit returns the
  seek's row directly, no TableRow re-encode; cold callers wrap at the edge.
- **B5** `PhysicalKey.space: Cow<'static, str>` with well-known spaces
  interned at decode — kills the per-read space String x2.
- **Capacity fix** (c497cc95): B3's seek-key build into an unreserved Vec
  realloc'd several times per read — the re-profile ranked the storm at
  ~15% of hit CPU. One upfront `reserve()`.

Settled C, same protocol, medians of 3 per group (same-session groups):

| cell | run ops/s | read p50 | read p99 | alloc bytes/op |
|---|---|---|---|---|
| C3a baseline | 103.0K | 8.9-9.7us | ~14us | 16.9K |
| B1+B2 | 103.0K | 9.3-9.9us | ~14.5us | 10.5-11.9K |
| B3+B4+B5 | 113.0K | 8.4-8.9us | 12.8-13.7us | 11.3-12.1K |
| + capacity fix | **118.4K** | **8.37us** | 13.0-13.7us | 11.4-12.2K |

- Net: C 103.0K → 118.4K (+15%), read p50 ~9.7 → 8.37us, C is now ~27% of
  the honest RocksDB 432K reference (was 4.5% at campaign start).
- The alloc probe is noisy (±15% between reps); bytes/op roughly halved
  from the pre-strip 16.9K but a ~10-12K gross remains (inventory predicted
  ~2-3K) — unexplained gross is background-inclusive (the probe brackets
  the whole run phase, including flush/compaction on the run's writes).
- Clean 214-sample gdb re-profile at 109.7K (5M-op window, post-settle
  gate): **index bisect + memcmp ≈26%**, allocator ≈15% (the realloc storm,
  since fixed), key codec ~8%, bloom ~3%, `max_commit_for_owned_levels`
  fold ~3%, rest long-tail.
- Exit gate: miss ≤1% PASSED (zero), per-read allocs materially down, but
  **p50 8.4us > the 3.5us gate → documented probe-fundamental floor**: the
  top residual is the index bisect + key memcmp in the format layer, which
  no allocation strip touches. Per the plan's definition of done this stops
  the slice and hands C4 the decision: index-format acceleration
  (prefix-truncated / layout-aware index) and/or mmap'd table reads.
- A/B spot cells (probed, 3 cells each): **B on = 139.4K / 138.4K / 24.7K**
  (median 138.4K vs 53.4K at C2 close — B is 95% reads, the C3 gains
  transfer; the 24.7K cell and the preheat-off control at 27.6K both drew
  the write-stall lottery, update max ~870ms). **A = 10.5K / 17.1K / 32.8K**
  (median 17.1K vs C2's 29.4K best-ever draw) — A remains update-stall
  dominated (p99.9 1-5ms, max 0.4-1s in every cell; W1.3 shape lottery),
  not read-bound; read p50 improved to 7.3-9.5µs in all cells. Mid-run
  flush-triggered preheat passes do run during A (4-7 passes, 0.5-0.9GB
  read) and one A cell showed run-phase churn re-pinning the pool
  (15.00/15.00, warm_full=266K, evict=12.5K) — noted as a C4-adjacent
  observation (pass-boundary purge can lag run-phase table churn), not the
  dominant A term. No read-side regression in any cell.

## 10M FULL YCSB suite (a-f), post-C3 baseline (2026-07-09, v1 @ 30021d5b, /data2)

First full six-workload baseline, both modes, settled durable protocol
(500K ops, 32g, settle 120s), medians of 3, rep-outer interleave. First
ledger coverage of D (latest-distribution reads), E (scans), F (RMW).

| workload | durable run (spread) | cache run | durable read p50 | vs RocksDB* |
|---|---|---|---|---|
| A 50/50 update | **8,510** (7.1-30.8K) | 348,325 | 7.4-9.1us | 36x |
| B 95/5 read | **99,079** (71.8-139.1K) | 1,239,659 | 8.9-9.9us | 4.4x |
| C 100 read | **111,051** (80.5-112.8K) | 1,750,882 | 8.9-9.1us | 3.9x |
| D latest-read | **69,797** (61.0-96.4K) | 1,285,400 | 9.5-11.8us | n/a |
| E scan(≤100) | **6,135** (5.3-6.5K) | 543,291 | scan p50 154-190us | n/a |
| F read-RMW | **12,321** (10.1-12.7K) | 324,022 | 7.6-8.2us | n/a |

*RocksDB refs from the C1-era scoreboard (A 304K / B 437K / C 432K);
no D/E/F reference cells recorded yet.

- **vs the 2026-07-08 post-campaign row** (durable A/B/C 17.8/54.5/19.5K):
  C x5.7 (the C campaign), B x1.8, cache C +12% (the C3b strip reaches the
  cache-mode read path too: 1.56M → 1.75M). A's median moved 17.8K → 8.5K
  BUT the spread is 7.1-30.8K across reps — the documented write-stall
  lottery (update p99.9 1-13ms, max 0.4-1.5s in every A cell) dominates A
  at 10M; no read-side regression (read p50 improved in every cell).
  Same for F (RMW = A-shaped).
- Write-heavy workloads (A, F) are now the scoreboard's weak column, gated
  on the W1.3-class stall lottery, not the read path.
- E durable ≈ 6.1K ops/s ≈ ~300K rows/s scanned (avg scan ~50 rows) —
  per-row cursor cost ~1.5-1.9us, consistent with the hot point path;
  E's low ops/s is scan-length arithmetic, not a defect.
- Every durable cell's on-disk probe: 24-39GiB post-load → **12GiB
  post-settle** — settle reclaim converges for the YCSB shape on current
  v1 within 120s (relevant baseline for #2524, where a multi-zone hot-key
  shape reported non-converging 40-68GB).

## #2524 attribution: zone-gluing churn convicted; reclaim starves under load; v1 converges (2026-07-09)

New `amp-repro` bin (GT5 `EnginePageStore` shape: 2N+1 rows/commit — 4KiB
`page/<BE u64>` + 150B `meta/<BE u64>` + per-commit `watermark` + `manifest`
hot keys; sequential ids; default open options like GT5) + new perf-trace
reclaim counters (sweep runs/deferrals by cause, objects+bytes quarantined,
purge runs+bytes, retention runs). 2,048 commits x 256 = 2.07GiB logical.

| cell | post-seed | settle peak | converged | seed compaction input | wide passes (>8 overlap) |
|---|---|---|---|---|---|
| full (GT5) | 10.1x | 15.3x @60s | **1.3x** @165s | 16.7GiB (8.1x) | 30/338 |
| nowatermark | 10.9x | 15.4x @60s | **1.3x** @165s | 18.5GiB (8.9x) | 30/436 |
| pagesonly | 2.7x | — | **1.3x** @30s | **2.21GiB (1.07x)** | **0**/285 |
| full, settle-600 | 12.7x | 17.5x @75s | **1.3x** @195s, flat 400s | 22.6GiB (10.9x) | 36/426 |

Verdicts against the three hypothesized mechanisms:

1. **Zone-gluing churn (mechanism A): CONVICTED.** Multi-zone commits pay
   8-11x compaction rewrite where the single-zone control pays 1.07x; the
   wide-overlap pass fingerprint (30-36 passes pulling >8 L1 tables vs
   ZERO) is the predicted gluing signature. Decisive refinement: the
   watermark hot key is MARGINAL (nowatermark ≈ full) — the `meta/*` /
   `page/*` prefix interleave alone glues every flush table's span across
   all older pages. Any workload writing two key prefixes per commit pays
   this. Trivial moves: 120 (pagesonly) vs 49-71 (glued).
2. **Reclaim starvation under load (mechanism B): CONFIRMED, mechanism
   re-scoped.** ZERO reclaim during every seed (retention_runs=0,
   quarantined=0) even though rewrite publishes enqueue the retention mark
   — the chain starves at the SCHEDULER (strict ladder + the low-tier
   interleave's `!has_active_build_task()` gate), upstream of the sweep's
   own builds_active defer (which never fired: deferred_builds=0). Preheat
   ran fine during seed via its drain-hook bypass — proof the slots
   existed. The plan's Fix B (de-gate interleave + registry pins for the
   mark/sweep interlocks it then hits) targets the right chain.
3. **GT5's non-convergence (mechanism C, pre-#2523): CONVICTED.** Current
   v1 converges to 1.3x in ~3min of idle and stays flat for 400+ more
   seconds. GT5's binary predated the #2523 queue-liveness fixes and
   watched only 15s of settle — the ascending limb of the reclaim curve.

New finding — **the reclaim transient overshoots**: during early settle the
store GROWS from ~10x to 15-17.5x (36.3GiB peak for 2.07GiB logical)
before collapsing — continued churn plus quarantine's copy-then-delete
staging (29-35GiB copied per cell). On a nearly-full disk this transient
is an ENOSPC hazard independent of convergence; Fix A removes most of the
bytes that feed it.

Reads (post-settle, converged store, DEFAULT budget = 240MiB cache vs
2.7GiB live): round p50 534-651us ≈ 33-40us/read, p99 6-13ms — the
issue's ms-scale read symptom is partly the default-budget miss regime
(GT5 also opened with defaults) and partly the amplified store at the
time. Guidance for the tier: set a memory budget ≥ the live set.

Phase-1 gate: PR-2 (reclaim liveness) and PR-3/4/5 (flush zone cuts +
compaction edge cuts) both proceed per the approved plan.

## #2524 Fix B: reclaim lives under load — in-flight output registry (2026-07-09)

The wholesale `has_active_build_task` interlock is replaced with precise
pins: builds reserve each output's object name in a runtime-owned in-flight
registry BEFORE publishing its bytes (`reserve_inflight_flush_output` /
`reserve_inflight_rewrite_output`); the table-object mark unions the
registry into its pinned set; reservations release when the prepared build
value is consumed (install → in-memory pins take over under the same lock
hold) or abandoned (orphans become sweepable — the crash-window analog,
since the registry is in-memory by design). De-gated: the sweep + retention
build defers and the low-tier interleave's `!has_active_build_task()` skip.
Kept: the retired-read-view defer (correct interlock), the 32-object cap.

amp-repro `full` (GT5 shape), same protocol as the PR-1 attribution row:

| metric | pre-Fix-B | Fix B |
|---|---|---|
| post-seed amp | 10.1-12.7x | **1.9x** |
| reclaim during seed | ZERO (retention_runs=0) | **87 retention / 46 sweeps / 21.1GiB purged** |
| settle transient peak | 15.3-17.5x (36GiB) | **5.8x** |
| converged 1.3-1.4x at | 165-195s | **75s** |
| post-settle read round p99 | 6-13ms | **744us** |

- YCSB regression spots (10M, settled): C 109.6K run / read p50 9.12us
  (suite baseline median 111.1K — parity); A 10.6K inside the documented
  7.1-30.8K write-stall lottery, A load 107.6K (top of range). Side
  benefit: **post-load on-disk 14GiB vs 24-39GiB** — mid-load reclaim keeps
  the store near live size during YCSB loads too.
- Zone-gluing churn is UNCHANGED as predicted (wide-overlap passes 34,
  seed compaction input 21.6GiB) — that is Fix A's target (flush zone cuts
  + compaction input-edge cuts).
- New behavioral tests: mark+sweep complete during a held off-lock build
  (bait reclaimed, pinned outputs survive, pins hand off to manifest
  reachability at install); abandoned build releases pins and its outputs
  are reclaimed next cycle.

## #2524 A2: flush zone cuts — the gluing churn collapses (2026-07-09)

Fix A slice 2 (after A1's plural plumbing, merged #2532): flush cuts the
frozen memtable BEFORE any key whose gap from its predecessor skips >=32MiB
of whole L1 tables (largest gaps first, <=4 outputs, monotone two-pointer
over the sorted L1 spans + frozen keys). Cuts land only at physical-key
transitions; per-segment identities are content-derived (same recipe as the
whole-memtable identity — idempotent retries, layout drift orphans safely);
segments build+publish one at a time (artifact memory stays one segment);
any post-publication failure reports every published name. Single-zone
workloads measure gaps of ~0 and NEVER cut — today's one-table flush,
byte-identical.

amp-repro, same protocol as the PR-1/Fix-B rows (2.07GiB logical):

| metric (full/GT5 shape) | Fix B only | A2 |
|---|---|---|
| seed compaction rewrite | 21.6GiB (10.4x) | **4.98GiB (2.4x)** |
| wide-overlap passes (>8 tables) | 34 | **1** |
| seed wall time | 107s | **28s** |
| settle transient peak | 5.8x | **2.5x** |
| converged 1.3x at | 75s | **~30s** |
| trivial moves / metadata avoided (seed) | 69 / 2.1GiB | **147 / 4.3GiB** |

- Cuts fire exactly where predicted: 41-44 cuts across ~77 flushes on the
  multi-zone shapes (~1.5 outputs/flush); `nowatermark` matches `full`
  (the meta/page interleave is the gluing, as the PR-1 attribution found).
- **Zero-regression pin holds: `pagesonly` took 0 cuts** (57 flushes, 57
  outputs, wide passes 0) — single-zone workloads are untouched by
  construction.
- The byte-free `MetadataPromotion` path is ALIVE for the glued shape:
  trivial moves x2, metadata-avoided bytes x2 during seed.
- Cumulative churn incl. settle: 31.4GiB -> 8.0GiB (3.9x logical). The
  plan's A3 gate ("churn still > ~3x after A2") technically still fires —
  residual disk (1.3x) and the transient (2.5x) are near-ideal, so A3's
  marginal value is now mostly seed-churn CPU/IO, not disk.

Rider (same PR): the Fix-B purge/sweep inventory race — reclaim now runs
during builds, so a sweep can advance the quarantine inventory between a
purge's token capture and its mutation; that hard-failed the purge task
with health debt (the flaky `background_scale` closed-loop assert,
reproduced on clean v1 HEAD 1-in-5). New typed
`InventoryTokenMismatch` service error -> `LifecyclePurgeStatus::
InventoryAdvanced` -> Deferred WITHOUT health debt (the staging sweep's own
follow-up purge covers the entries). Regression test pins the deferral;
the closed-loop suite ran 7x green after the fix.

## #2524 A3: input-edge output cuts — PARKED after falsification (2026-07-09)

Fix A slice 3 (dissolve pre-A2 glued straddlers by cutting compaction
outputs at input-table edges) was built, gated three times, and PARKED on
branch `a3-input-edge-cuts-parked`. The mechanism WORKS — the dissolution
test pins a straddler splitting at the zone boundary and the append zone
reaching byte-free metadata promotion; repro cells kept residual 1.3x,
wide passes 0, and trimmed cumulative churn — but the policy cannot be
made safe for consolidation:

1. Unconditional per-input edges: YCSB C **-28% throughput / +40% read
   p50** (interleaved same-session T-C-T vs the A2 build). Mechanism: the
   sequential YCSB load's narrow disjoint L0 tables look exactly like
   zones, and edge cuts blocked CONSOLIDATION (merging small tables into
   big ones) — L1 kept per-flush tables forever.
2. Cluster gate (edges only from >=2 disjoint input clusters): -25%.
3. Dissolve gate (edge must fall inside an overlap table's span): -6%,
   with a deterministic 6 residual cuts per load.
4. Straddler gate (overlap table must fully contain >=1 cluster AND
   intersect >=2): STILL 6 deterministic cuts, still -5% / +0.44us —
   because a legitimately CONSOLIDATED table (built by merging several
   sequential flushes) fully contains multiple input clusters by
   construction and is indistinguishable from a glued straddler at the
   pass level.

The missing discriminator is key-gap MASS — A2's flush policy has it
(L1-skip bytes across the gap); the compaction pass does not. A shippable
A3 must bring that signal to pass time (e.g., score the gap between
adjacent input clusters by the bytes of overlap-table content strictly
inside it, mirroring `flush_zone_cut_keys`). Parked with machinery,
counters (`compaction_input_edge_cuts`, bench `[probe] a3:` line), and
behavioral tests intact on the parked branch.

Standing #2524 result WITHOUT A3 (all merged): load-time amp 1.8x,
transient peak 2.5x, residual 1.3x converging ~30s, seed churn 2.4x
(single-zone control 1.07x), YCSB C at its best-ever band. The remaining
A3-class value (seed churn 2.4 -> ~2x) stays on the shelf until the
gap-mass design.

## #2527: hybrid COW fork — unflushed rows no longer force O(dataset) forks (2026-07-09)

Diagnosis: the COW fork gate required `inherited_layers().is_empty() &&
!has_in_fork_unsealed_rows(V)` — ONE unflushed row at fork time demoted
the whole fork to `fork_snapshot_rows`, an eager O(dataset)
materialization (full reader scan of the source plus a physical duplicate
written through the child). Any warm writer therefore forks eagerly:
GT5's rollout forks always carry a hot unsealed watermark row, so every
fork paid seconds of latency and duplicated the store.

Fix (hybrid COW): sealed rows ride the inherited COW layer exactly as
before; the unsealed slice (rows <= fork version still in the
active/frozen memtable — bounded by the memory budget, not the dataset)
is built into ONE durably published child-owned L0 table at fork time.
Content-derived identity (child, version, row count, span digest) keeps
replays idempotent through `publish_or_load_existing`; the table is
recorded in the table catalog before the fork-time child manifest
publish; recovery restores layered children from the child manifest, so
the slice is durable before the fork commits. Fork runs under the runtime
lock, so the new object cannot race the sweep. Cache-mode forks and the
eager fallbacks (fork-of-fork with inherited layers, all-unsealed
sources) are unchanged.

A/B (same box, same session, `amp-repro --shape full --settle-secs 60
--forks 4`; ~2.07GiB logical, one 64B dirty write before each fork):

| cell | fork p50 | fork max | disk growth (4 forks) |
|---|---|---|---|
| control (v1 HEAD 78c7212d, pre-fix) | 50,653 ms | 53,317 ms | 8.95 GiB |
| hybrid, fat memtable (~35MiB slice) | 255 ms | 342 ms | 139.45 MiB |
| hybrid, drained memtable | 33 ms | 61 ms | 171.89 KiB |

Fork cost is now O(unsealed bytes): ~200x faster at the worst measured
shape, ~1,500x drained, and per-fork growth went from a full logical
duplicate (~2.2GiB) to the slice itself. GT5's real fork shape (KB-scale
dirty state per rollout) lands at the drained end — tens of ms.

Tests: `fork_with_unflushed_rows_is_cow_and_survives_reopen` (fork with a
sealed + unsealed mix, divergence isolation both directions, reopen reads
both the post-fork write and the fork-time unsealed slice);
`fork_with_unsealed_rows_builds_a_cow_child` (1 inherited layer + child
L0 slice); `fork_of_an_all_unsealed_source_stays_eager`. Battery: lib
default 3361 / perf-trace 3558 green, all-targets + clippy clean on both
feature sets, format goldens, engine-next, e2e 17/17.

Shelf: fork-of-fork (sources WITH inherited layers) still takes the eager
path — needs layer-chain composition, separate slice.

## 100M YCSB degradation scout — the out-of-cache cliff (2026-07-09, v1-content @ 88715bca, /data2)

First 100M-record suite (1KB random values, ~103GiB logical). Single pass
(scout, not medians), durable, 500K ops, 32g budget, settle 120s at 10M /
300s at 100M, fresh load per workload, same-session interleaved 10M
control, same binary/box. 10M control took 28min; 100M took 3h30 (six
~28.5min loads dominate).

| workload | 10M control | 100M | ratio |
|---|---|---|---|
| load (rows/s) | 75.3-86.4K | 57.5-62.7K | -29% |
| A 50/50 update | 10,967 | 8,414 | -23% |
| B 95/5 read | 139,273 | 9,920 | **14.0x** |
| C read-only | 117,990 | 9,413 | **12.5x** |
| D read-latest | 97,986 | 5,376 | **18.2x** |
| E scan(<=100) | 5,908 | 2,683 | 2.2x |
| F read-RMW | 8,115 | 3,778 | 2.1x |

Read p50/p99 (C): 8.59us/13.3us -> 25.2us/320us. D read p50 218us.
E scan p50 170 -> 246us, p99 236us -> 911us. A update p99.9 12.1 -> 4.4ms
(single-pass lottery noise; A/F medians need reps).

Readings:

1. **The B/C/D cliff is the out-of-cache transition, and its unit cost is
   the target.** At 10M the whole 12GiB store fits in cache (miss ~0 after
   preheat); at 100M the C cell runs at **40.6% block-cache miss** (203K
   misses / 500K reads, exactly one 16KiB data block and 16.5KB read per
   miss — the format layer is fine), and `api_point_ms` = 52.9s of the
   53.1s run: the whole regression is read-path wall. Derived miss cost
   **~248us per missed block vs ~100us device floor** for a QD1 16KiB
   NVMe read — ~2.5x of overhead (checked-path checksum + decode + cache
   insert + eviction churn: 537K evictions over the run). D is worse (75%
   miss, p50 218us) because the latest-distribution head is exactly what
   preheat did not retain.
2. **Preheat thrashes when store >> cache.** During D's 300s settle the
   preheat cycled 110GiB through the 15GiB block pool (868 passes, 6.7M
   blocks admitted, self-evicting). At over-budget scale it needs an
   admit-until-full / hot-first policy instead of full-store passes —
   today it burns settle-window IO for a cache that ends up holding an
   arbitrary residue. (Probe oddity to check while there: `cache_gb`
   prints 45-60/15.00 — used 3-4x capacity — either an accounting bug or
   a mislabeled cumulative counter.)
3. **Write side and space health scale cleanly.** Load -29% at 10x data;
   A -23% (already stall-lottery-bound, not read-bound); residual
   118-120GiB post-settle ~= **1.15x logical** — the #2524 fixes hold at
   10x scale, slightly better than the 10M shape.
4. E/F degrade only ~2.2x/2.1x — both were already bound elsewhere (scan
   arithmetic; RMW stall lottery + the read tax on the R half).

Next candidates, in leverage order: (a) RocksDB 100M control on the same
box (both engines pay the disk at this scale — the honest gap is the
miss-cost ratio, not the 10M cache-race); (b) miss-cost decomposition
slice (~248us -> device floor: pread, checksum/decode, insert/evict); (c)
preheat policy for over-budget stores; (d) medians-of-3 re-run of the
cells that matter after any fix.

### RocksDB 100M control (same session, same box, stock options)

`rocksdb-ycsb` a-f at 10M and 100M, identical generators/keys/values,
500K ops, stock `Options::default()` (the C1 peer framing; tiny block
cache + OS page cache, no settle window — its post-ordered-load
compaction is trivial-move-dominated). 10M control reproduced the
C1-era refs (A 279K / B 417K / C 440K / D 307K / E 36.7K / F 190K;
store 9.6GB raw). 100M: store 95.6GB raw, load 619-652K rows/s.

| workload | RocksDB 10M -> 100M | RocksDB degr. | Strata degr. | gap @10M | gap @100M |
|---|---|---|---|---|---|
| load | 613-736K -> 619-652K | ~nil | -29% | 6.4x | **10.8x** |
| A | 279,027 -> 44,588 | 6.3x | 1.3x | 25.4x | **5.3x** |
| B | 416,859 -> 29,328 | 14.2x | 14.0x | 3.0x | **3.0x** |
| C | 439,838 -> 29,464 | 14.9x | 12.5x | 3.7x | **3.1x** |
| D | 307,240 -> 15,898 | 19.3x | 18.2x | 3.1x | **3.0x** |
| E | 36,718 -> 5,682 | 6.5x | 2.2x | 6.2x | **2.1x** |
| F | 190,068 -> 25,329 | 7.5x | 2.1x | 23.4x | **6.7x** |

Readings:

1. **The cliff is physics, not a Strata defect.** RocksDB's B/C/D
   degrade 14.2x/14.9x/19.3x vs our 14.0x/12.5x/18.2x — the same
   out-of-cache transition at near-identical ratios. Nobody outruns the
   disk at 100M x 1KB on a 61GB box.
2. **The read gap is scale-stable at ~3x and is the miss-cost ratio.**
   RocksDB C at 100M: mean 33.9us/op, p99 ~97us — its per-miss cost sits
   at the device floor (~100us buffered pread), while ours is ~248us.
   That 2.5x per-miss overhead IS the 3x C/B/D gap. Confirms the
   miss-cost decomposition slice as the highest-leverage read work; a
   second term is the hot-path p50 (ours rose 8.6 -> 25us at 100M —
   deeper fringe: nz_search 5/op, 2 seeks/op — while RocksDB's hit p50
   stays ~5us).
3. **Write-heavy gaps compress dramatically at scale** (A 25.4x -> 5.3x,
   F 23.4x -> 6.7x): RocksDB's mixed-workload throughput pays 6-7x for
   compaction-vs-read interference at 100M while our A barely moved
   (stall-lottery-bound either way).
4. **Load is the honest outlier.** RocksDB loads 100M at the same speed
   it loads 10M (~645K rows/s ~= device bandwidth; ordered keys =
   trivial moves), while we drop 75-86K -> 58K. We are structure-bound,
   not bandwidth-bound, on ingest at scale — a real scaling defect worth
   its own attribution pass (suspects: flush/compaction overlap, commit
   admission, WAL cadence).

## 1B YCSB matrix night — two store-bricking bugs, partial numbers (2026-07-10, @61b3f19a)

First billion-record attempt. 100B random values (1KB x 1B does not fit
the box), NEW shared-store protocol (`engine-ycsb --data-dir/--skip-load`:
load once per scale, C on the virgin store, then b,d,e, then a,f), 500K
ops, 32g, settles 120/300/600s; RocksDB fresh-per-workload at all three
scales (stock options).

**The headline is two product bugs, not the numbers:**

1. **#2553 — 1B: manifest-listed compaction output missing on disk
   (in-process data loss).** Load (288GiB, ~2.7h) + 600s settle clean;
   the C run's reads then hit
   `tables/.../l0001/maintenance-compaction-...-c04f315e4c7f9cb2-00000001`
   -> NotFound; every reopen fails `TableManifestRecoveryMismatch`. The
   index-0 SIBLING of the same multi-output family exists (51MB); index 1
   is in neither tables/ nor quarantine/. Suspect seam: multi-output
   publish->install->reclaim (the #2531-#2533 machinery). Needed 1B rows
   of sustained load+maintenance; 10M/100M never showed it.
2. **#2555 — reopen WAL next-segment collision (clean close + reopen +
   first write bricks the store).** At 10M AND 100M: reopen + reads +
   settle fine; first update commit fails
   `CreateSegment wal/<max-existing-id> AlreadyExists`; the NEXT recovery
   refuses `recovered WAL package must be strictly ordered`. Scale-gated
   by WAL truncation history (a -q store reopens fine — e2e's reopen
   tests are all tiny = CI blind spot). Killed all non-C Strata cells.
   Cutover-blocking.

Numbers that survived (100B family, single pass):

| cell | Strata | RocksDB | gap |
|---|---|---|---|
| 10M load (rows/s) | 294,586 | ~3.2M | 10.9x |
| 100M load | 207,645 | ~3.2M | 15.5x |
| 1B load | ~100K (wall-clock est; result line lost to #2553) | ~3.2M (flat!) | ~32x |
| 10M C | 124,396 (store cache-resident) | 392,816 | 3.2x |
| 100M C | 23,789 (26GiB vs 15GiB pool) | 48,710 | **2.0x** |
| 1B C | n/a (#2553) | 23,538 (p50 58us) | — |

RocksDB full 1B row for later reference: A 41.5K / B 23.8K / C 23.5K /
D 15.0K / E 10.5K / F 23.6K; read p50 58-78us (page cache holds ~half of
its 107GB store).

Readings:

1. Strata ingest slope within one value family: 295K -> 208K -> ~100K
   rows/s (-30% then -52% per decade, steepening at depth) while RocksDB
   loads FLAT at ~3.2M rows/s across three decades (ordered keys,
   no-sync batches). The 100M attribution pass (prior row) is now a 1B
   attribution pass.
2. 100M C at 2.0x RocksDB is Strata's best-ever relative read cell —
   the 1.7x-over-budget regime is kind to us; the 40%-miss regime
   (100M x 1KB: 3.1x) and their page-cache-heavy 1B regime remain to be
   fought after the read-miss cost work.
3. Evidence preserved: store-10m (3GiB, #2555 cheap repro), store-100m
   (26GiB, #2555), store-1b (288GiB, #2553). run.log alongside.

## Backfilling a row after a perf run

1. Run the scoreboard: `regression.rs --capture-baseline` (writes `baselines/*.json`) and
   the l9 `--scales 10m` + `--scales 100m` cells (see the runbook for exact commands).
2. Read the load/C/A/E throughput from the scoreboard JSON and `db_open_after_load_ms` +
   the fast-open counters from the l9 reopen cell.
3. Replace the `*pending run*` cells in the BS4 row; if BS1–BS3 backfill is needed, capture
   at those HEADs on the same machine and fill their rows too.
4. State the verdict against the exit gates (§ above) and the 1.5× band, not against
   RocksDB alone.
