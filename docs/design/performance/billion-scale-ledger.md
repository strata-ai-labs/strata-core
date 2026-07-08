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
engaging only near the wall, as designed. NOTE before the W1.4 PR: run the 4T
saturation cell (`storage-next-concurrent-writers`) per the graded-flip lesson —
pacing changes get saturation guardrails.

## Backfilling a row after a perf run

1. Run the scoreboard: `regression.rs --capture-baseline` (writes `baselines/*.json`) and
   the l9 `--scales 10m` + `--scales 100m` cells (see the runbook for exact commands).
2. Read the load/C/A/E throughput from the scoreboard JSON and `db_open_after_load_ms` +
   the fast-open counters from the l9 reopen cell.
3. Replace the `*pending run*` cells in the BS4 row; if BS1–BS3 backfill is needed, capture
   at those HEADs on the same machine and fill their rows too.
4. State the verdict against the exit gates (§ above) and the 1.5× band, not against
   RocksDB alone.
