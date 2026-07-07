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

BS5.1 also removed two pre-lock writer serializers found with the new instrument: the
commit-timestamp base and the durability-mode resolution both took the full runtime lock per
commit (writers queued behind an in-flight fsync before ever reaching the commit path — the
join queue always looked empty). The timestamp base now reads an off-lock atomic mirror
(clamp semantics unchanged; the allocator still enforces the floor under the lock) and the
mode comes from the open summary.

## Backfilling a row after a perf run

1. Run the scoreboard: `regression.rs --capture-baseline` (writes `baselines/*.json`) and
   the l9 `--scales 10m` + `--scales 100m` cells (see the runbook for exact commands).
2. Read the load/C/A/E throughput from the scoreboard JSON and `db_open_after_load_ms` +
   the fast-open counters from the l9 reopen cell.
3. Replace the `*pending run*` cells in the BS4 row; if BS1–BS3 backfill is needed, capture
   at those HEADs on the same machine and fill their rows too.
4. State the verdict against the exit gates (§ above) and the 1.5× band, not against
   RocksDB alone.
