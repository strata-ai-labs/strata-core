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

## Backfilling a row after a perf run

1. Run the scoreboard: `regression.rs --capture-baseline` (writes `baselines/*.json`) and
   the l9 `--scales 10m` + `--scales 100m` cells (see the runbook for exact commands).
2. Read the load/C/A/E throughput from the scoreboard JSON and `db_open_after_load_ms` +
   the fast-open counters from the l9 reopen cell.
3. Replace the `*pending run*` cells in the BS4 row; if BS1–BS3 backfill is needed, capture
   at those HEADs on the same machine and fill their rows too.
4. State the verdict against the exit gates (§ above) and the 1.5× band, not against
   RocksDB alone.
