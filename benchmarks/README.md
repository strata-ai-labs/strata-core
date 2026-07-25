# strata-benchmarks

Two instrument families live in this crate, deliberately outside the main
workspace so heavy dependencies (jemalloc, RocksDB) stay out of the product
graph.

## Wall-clock bins (`src/bin/`)

The original regression instruments: YCSB A–F through the public engine API,
storage-level scale/concurrency probes, and the `bench-compare` A/B table
tool. Results are recorded to `results/` with provenance (git commit,
hardware); these are hand-run instruments, not CI gates. The lib pins
jemalloc as the global allocator for these bins — a bin must reference the
lib (`extern crate strata_benchmarks;`) or it silently runs on glibc malloc.

## Instruction-count benches (`benches/`, TCP5.2)

Per-PR CI gates over the shipping paths. Callgrind instruction counts are
deterministic (~±0.5%) and hardware-independent, so committed ceilings can
block merges without flapping — see `scripts/perf_floors.py` in the repo
root and the Phase 5 charter in
`docs/architecture/v1-test-coverage-program.md`.

Targets:

| Bench | Path measured |
|---|---|
| `storage_commit` | `commit_small_batch` (3 mutations), `commit_medium_batch` (64), `wal_append_burst` (32 sequential commits) on a warmed durable Standard runtime |
| `storage_reopen` | `recovery_reopen` — reopen of a ~200-commit store (WAL replay + assembly) |
| `wire_commands` | one executor wire command each: `kv_set`, `kv_get`, `kv_scan`, `json_set`, `json_get` against a warmed cache executor |

### Running locally

```bash
sudo apt-get install -y valgrind
cargo install iai-callgrind-runner --version 0.16.1 --locked
cd benchmarks
cargo bench --bench storage_commit   # or storage_reopen / wire_commands
```

Setup (runtime open, fixture fill, executor warm-up) runs outside the
measured section via iai-callgrind's `setup =` attribute; the reported
instruction count covers exactly the shipping-path body.

Two determinism notes:

- The bench targets do **not** link the `strata_benchmarks` lib, so they run
  on the system allocator — jemalloc stays a wall-clock-bin concern and
  counts are allocator-stable under valgrind.
- Counts are hardware-independent but **toolchain-dependent**: ceilings in
  `scripts/perf_floors.py` are pinned to the workspace toolchain
  (`rust-toolchain.toml`) and regenerate on toolchain bumps.

### Calibration (run-to-run spread)

Five consecutive runs per bench on one machine (toolchain 1.94.1,
valgrind 3.22.0, iai-callgrind 0.16.1); the spread validates the gate
tolerance in `perf_floors.py`. Six of nine benches are bit-identical
across runs; the worst spread is 0.017% — the initial 10% gate band is
deliberately conservative and ratchets tighter as CI data accumulates.

| bench | min | max | spread |
|---|---|---|---|
| `commit_small_batch` | 69,035 | 69,035 | 0.0000% |
| `commit_medium_batch` | 1,155,846 | 1,156,042 | 0.0170% |
| `wal_append_burst` | 2,209,953 | 2,210,257 | 0.0138% |
| `recovery_reopen` | 11,723,519 | 11,723,607 | 0.0008% |
| `kv_put_wire` | 62,932 | 62,932 | 0.0000% |
| `kv_get_wire` | 17,593 | 17,593 | 0.0000% |
| `kv_scan_wire` | 1,080,389 | 1,080,389 | 0.0000% |
| `json_set_wire` | 79,006 | 79,006 | 0.0000% |
| `json_get_wire` | 27,856 | 27,856 | 0.0000% |
