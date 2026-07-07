# BS4.6 exit runbook — perf-environment runs

BS4.6 (re-baseline + exit) lands its harness, benchmark cells, and docs in-tree; the
**measured** exit numbers come from a perf box (loading ~100 GB is infeasible in the dev
loop). This runbook is the exact command set. Run it on the reference machine, then
backfill [`billion-scale-ledger.md`](./billion-scale-ledger.md) and the umbrella §2
scoreboard.

All commands assume a **release** build. The `benchmarks/` crate is workspace-excluded and
carries its own `[workspace]` root, so it builds from a full checkout or a git worktree
alike (via `--manifest-path benchmarks/Cargo.toml`).

## Prerequisites

- Reference machine, quiesced (no other load — the write path carries an intermittent
  convoy; see [`lock-decoupling-perf-ledger.md`](./lock-decoupling-perf-ledger.md)).
- ~120 GB free disk for the 100M exit cell (~100 GB dataset + WAL/manifest headroom).
- `--release` everywhere; `--features perf-trace` for the counter assertions.

## Gate #1/#2/#5 — the 100M-on-8GiB exit test

The `#[ignore]` integration test IS the gate: it loads 100M × ~1 KB (~100 GB) on an 8 GiB
budget, closes, times a cold reopen, and asserts `lazy_full_materialization == 0`,
`table_reader_opens > 0`, and open ≤ 1 s.

```bash
cargo test -p strata-storage-next --features perf-trace --release \
  api::tests::disk_resident_reads::durable_exit_gate_100m_on_8gib_budget \
  -- --ignored --nocapture
```

Pass = the test returns green (dataset > 8 GiB loaded and served, reopen under the 1 s
limit, zero full materializations). If open exceeds 1 s, build the parallel manifest-replay
open (bs4 plan "Open items"; `cfg(not(wasm32))`) and re-run — that is the one reserved
mitigation for gate #2.

The harness logic is smoke-tested on every perf-trace run at a tiny scale via
`durable_exit_gate_harness_smoke` (not `#[ignore]`); the 100M test is the same scenario
with perf-env parameters.

## Gate #2 (benchmark form) — open time at 100M

The l9 scale bin's `reopen-after-load` cell (landed in BS4.6): after the workloads finish
and the runtime closes cleanly, it times a cold reopen of the same durable directory and
emits `db_open_after_load_ms` + the fast-open counters (`reopen_table_reader_opens`,
`reopen_table_lazy_full_materializations`, `reopen_table_data_block_reads`). It runs by
default (part of the workload set) for durable engines; the cache engine skips it.

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100m --engines standard --memory-budget 8g
```

Read `db_open_after_load_ms` from the emitted `BenchmarkResult` (results/ JSON). This is
the benchmark-measurable form of gate #2; it should agree with the exit test's timed
reopen, and `reopen_table_lazy_full_materializations` must be 0.

## Gate #3 — 10M scoreboard re-baseline

The exit band is "10M cells within 1.5× of the BS2/BS3 results." There is **no committed
BS2/BS3 baseline** — compare against the umbrella §2 snapshot (`billion-scale-plan.md`
§2) and capture BS4 as the first committed baseline.

```bash
# scoreboard cells (load/C/A/E at 10M) + capture the committed baseline:
cargo run --release --manifest-path benchmarks/Cargo.toml --bin regression -- --capture-baseline
# → writes baselines/*.json (RegressionRun schema). Re-run without --capture-baseline to
#   check a later HEAD against it (thresholds: 5% throughput / 5% p50 / 10% tail).

# the l9 scoreboard at 10M (fresh open) for the load/read cells:
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 10m --engines standard
```

Verdict: each 10M cell within 1.5× of the §2 value. Read A/E as medians (write-path
convoy — a single run is a point sample).

## Gap G9 — subcompaction honest re-A/B

The regime BS3 Slice 4 was built for now exists (compaction is I/O-bound once tables are
disk-resident). The toggle is already wired — `STRATA_SUBCOMPACTIONS` (default `1`,
serial; the parallel fan-out is kept reachable for exactly this test). No code change; the
A/B is the L0-compaction bin at two settings:

```bash
# serial (control):
STRATA_SUBCOMPACTIONS=1 cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l0-compact
# parallel fan-out (treatment):
STRATA_SUBCOMPACTIONS=4 cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l0-compact
```

Read `mb_per_s` from each. Verdict: does the fan-out win now that compaction is I/O-bound
(the memory-bound A/B in BS3 found no win)? Record the delta in the ledger and set G9's
gap-table verdict.

## Backfill after the runs

1. **Ledger** ([`billion-scale-ledger.md`](./billion-scale-ledger.md)): replace the BS4
   row's `*pending run*` cells with load/C/A/E from the scoreboard JSON and open time from
   the l9 reopen cell; state the verdict against the exit gates and the 1.5× band.
2. **Umbrella §2** (`billion-scale-plan.md`): update the strata (durable) row + gap column
   if the re-baseline moved a cell materially.
3. **Gap table** (`billion-scale-plan.md` §3): set **G9**'s verdict from the subcompaction
   A/B (win → keep the fan-out default-on candidate for BS6; no win → confirm serial stays
   default).
4. **BS6 handoff**: if the block-size sweep or zstd assessment produced numbers, record
   them in the BS6 § (G20 zstd / G21 readahead) as the entry data for that milestone.

## Known limitation exercised by these runs

Time-travel forks (`fork_at_retained_version` / `fork_at_retained_timestamp`) still reach
the one durable-eager install (`build_snapshot_l0_tables`) and materialize the entire
source branch — a V1 feature gated behind a documented limitation, fully scoped in
[`historical-fork-residency-scoping.md`](./historical-fork-residency-scoping.md) (scoped,
not started). The 100M exit cells therefore exercise **live** forks (O(1) COW) only; do
not run a historical fork at 100M until that follow-up lands.
