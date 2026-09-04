# Write Amplification and Flash/SD Endurance

**Status:** Reference derivation (satisfies invariant SCALE-005).
**Scope:** The default durable configuration. Cache mode writes nothing to disk
and is out of scope except where noted.

This document derives the theoretical write amplification (WA) of Strata's
default LSM configuration and turns it into a flash/SD-card endurance envelope,
so the edge-market claim has math behind it. It is a *bound* derivation, not a
benchmark; measured evidence is referenced in the last section.

## Default configuration

All values are the shipped defaults, anchored to source so this document fails
review when a default moves.

| Parameter | Value | Source |
|---|---|---|
| L0 compaction trigger | 4 tables | `LEVEL_ZERO_COMPACTION_THRESHOLD` — `crates/storage/src/lifecycle/compaction.rs:41` |
| Nonzero-level compaction trigger | 4 tables | `NONZERO_LEVEL_COMPACTION_THRESHOLD` — `compaction.rs:110` |
| Nonzero-level urgent trigger | 8 tables | `NONZERO_LEVEL_URGENT_COMPACTION_THRESHOLD` — `compaction.rs:111` |
| Level size growth factor (T) | 10 | `NONZERO_LEVEL_TARGET_GROWTH_FACTOR` — `compaction.rs:114` |
| Base (L1) target, min…max | 1 MiB … 256 MiB | `NONZERO_LEVEL_MIN/MAX_BASE_TARGET_BYTES` — `compaction.rs:112-113` |
| Max level count | 8 (L0 + L1…L7) | `DEFAULT_MAX_LEVEL_COUNT` — `crates/storage/src/branch/config.rs:5` |
| Regression gate (measured) | ≤ 4× | `SCALED_COMPACTION_AMPLIFICATION_GATE` — `crates/storage/src/api/tests/mod.rs:122` |

The terminal nonzero level is `max_level_count − 1 = L7`
(`api/tests/mod.rs:116-119`). L1 target bytes are `1 MiB × T^(level−1)`
(`nonzero_level_target_bytes`, `compaction.rs:626`); the live path anchors the
base from the bottommost populated level, RocksDB dynamic-level style
(`nonzero_level_targets_from_level_bytes`, `compaction.rs:636`), which keeps
*fewer* levels populated than the static formula and is therefore never worse
than the bound below.

### Level fill sizes

A level `Lk` reaches its target at `~1 MiB × 10^(k−1)`:

| Level | Target size | Cumulative data to populate |
|---|---|---|
| L1 | ~1 MiB | ~1 MiB |
| L2 | ~10 MiB | ~11 MiB |
| L3 | ~100 MiB | ~111 MiB |
| L4 | ~1 GB | ~1.1 GB |
| L5 | ~10 GB | ~11 GB |
| L6 | ~100 GB | ~111 GB |
| L7 | ~1 TB | ~1.1 TB |

The number of *populated* levels — and therefore the WA — is set by the live
dataset size. This is the crux of the edge story: a small device holds a small
dataset, populates few levels, and pays little amplification.

## Write amplification derivation

Total device write amplification is the sum of independent contributions:

```
WA_device = WA_wal + WA_flush + WA_compaction
```

- **`WA_wal` = 1×** in durable mode. Every committed record is appended to the
  WAL once before it is later written into a table. (Cache mode: 0× — no WAL,
  invariant DUR/cache.) This is a sequential, coalesced append
  (`DEFAULT_WAL_APPEND_BUFFER_BYTES = 128 KiB`, `service/wal.rs:113`), the
  gentlest possible pattern for flash.
- **`WA_flush` = 1×.** Each record is written from the memtable to an L0 SST
  exactly once. The memtable is sized from the storage memory budget (see
  [ENGINE_INVARIANTS](../../audit/ENGINE_INVARIANTS.md) SCALE-001); a larger
  budget means larger, fewer L0 tables and less downstream churn.
- **`WA_compaction`** is where the LSM shape dominates. Leveled compaction
  rewrites data as it descends. For the transition `Ln → Ln+1` the target level
  holds ~T× the data of the source for the same key range, so absorbing the
  source rewrites ~T bytes of the target per byte promoted — each
  nonzero→nonzero boundary costs ~T. Data that reaches level `Lk` crosses the
  L0→L1 step plus `(k−1)` nonzero→nonzero boundaries:

```
WA_compaction(k) ≈ 1 (L0→L1) + T × (k − 1)
WA_device(k)     ≈ 1 (WAL) + 1 (flush) + 1 (L0→L1) + T × (k − 1)
```

With T = 10, this gives the steady-state worst case (continuous uniform
overwrite that keeps every populated level at its trigger):

| Deepest populated level | ~Dataset size | WA_device (worst case) |
|---|---|---|
| L1 | ≤ 1 MiB | ~3× |
| L2 | ~10 MiB | ~13× |
| L3 | ~100 MiB | ~23× |
| L4 | ~1 GB | ~33× |
| L5 | ~10 GB | ~43× |
| L6 | ~100 GB | ~53× |
| L7 | ~1 TB | ~63× |

**~63× is the pessimal envelope** for a fully populated 8-level tree under
adversarial, uniformly-overwriting load. It is an upper bound, not a typical
figure: append-mostly or key-local workloads leave upper levels cold, the
dynamic base anchor collapses empty intermediate levels, and partial key-range
overlap makes real per-boundary cost `< T`.

## What is actually measured

- **Regression gate (default scale):** `SCALED_COMPACTION_AMPLIFICATION_GATE = 4`
  asserts that observed compaction input (bytes and rows fed into compaction, the
  rewrite work) stays **≤ 4× logical** over the closed-loop workload
  (`assert_scaled_compaction_amplification_below_gate`, `api/tests/mod.rs:136`).
  That workload (~50k rows × 150 B ≈ 7.5 MB, `SCALED_CLOSED_LOOP_CACHE_*`)
  populates ~2 levels, where the bound above predicts ~13× worst case — the
  measured ≤4× reflects the sub-`T` real overlap and confirms no regression.
- **Load evidence (large scale):** the 2026-06-22 durable-load study measured
  compaction WA ≈ 15× at 5M records *before* the maintenance-priority and
  reclaim fixes landed. See
  [durable-load-amplification-evidence.md](../../design/performance/durable-load-amplification-evidence.md).

## Flash/SD endurance envelope

A flash device's endurance is commonly rated as TBW (terabytes written to the
device). Host-visible endurance is that budget divided by total amplification:

```
Host_TBW = Device_TBW / (WA_device × WA_ftl)
Lifetime_days = Host_TBW / daily_host_writes
```

`WA_ftl` is the card's own filesystem/FTL amplification (garbage collection,
wear leveling) — typically ~1.1–4× on SD/eMMC and independent of Strata; treat
it as a multiplier on top of `WA_device`. The table below sets `WA_ftl = 1` so
the figures are Strata's contribution alone; divide by your card's FTL factor
for the real number.

For a representative high-endurance microSD rated **100 TBW**:

| Workload footprint | WA_device | Host writes before wear-out |
|---|---|---|
| ≤ 1 MiB (config/KV state) | ~3× | ~33 TB |
| ~10 MiB | ~13× | ~7.7 TB |
| ~1 GB | ~33× | ~3.0 TB |
| ~1 TB (full tree) | ~63× | ~1.6 TB |

Worked example: an edge agent writing **1 GB/day** of host data to a 100 TBW
card with a ~100 MB working set (~23× WA) sustains
`100e12 / (23 × 1e9) ≈ 4,300 days ≈ 12 years` before the endurance budget is
spent — well beyond the device's practical service life. The endurance ceiling
only becomes a design constraint for large (multi-hundred-GB), continuously
overwritten datasets on low-TBW media.

## Levers to reduce write amplification

- **Keep the working set small.** WA is set by the deepest populated level;
  fewer levels is cheaper. This is automatic on constrained hosts.
- **Raise the memory budget** (SCALE-001 / [#2905](https://github.com/stratalab/strata-core/issues/2905)):
  a larger memtable produces larger, fewer L0 tables and defers compaction.
- **Raise the base target** (up to 256 MiB): a larger L1 removes an entire
  ×10 level for the same dataset, cutting `WA_compaction` by ~T.
- **Prefer append/key-local access** over uniform random overwrite: the ~63×
  figure is specific to the adversarial overwrite pattern.

## References

- Invariant **SCALE-005** — `docs/audit/ENGINE_INVARIANTS.md`.
- Measured load evidence — `docs/design/performance/durable-load-amplification-evidence.md`.
- Compaction sizing — `crates/storage/src/lifecycle/compaction.rs`.
- Level configuration — `crates/storage/src/branch/config.rs`.
- Regression gate — `crates/storage/src/api/tests/mod.rs`.
