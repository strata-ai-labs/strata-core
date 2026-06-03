# PERF-P2 Point-Read Isolation Report

## Scope

PERF-P2 was a benchmark-local spike only. It did not change the production
serving path.

The spike measured the two proven point-read costs independently on the same
100K-key storage-next table shape from PERF-P0:

1. current read-view capture plus current point candidate scan;
2. current read-view capture plus direct ordered-key point seek;
3. borrowed source view plus current point candidate scan;
4. borrowed source view plus direct ordered-key point seek.

The purpose was to decide whether to promote read-view pinning, point seek, or a
small combined point-read correction.

## Command

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-point-spike -- --scale 100k --samples 1000 --value-bytes 150
```

Result file:

`benchmarks/results/storage-next-point-spike-2026-06-03T20-00-44Z-56d9ac5e.json`

The synthetic branch held 100,200 rows: 100,000 user rows plus two timeline rows
for each of the 100 load batches.

## Results

| Case | Throughput | Avg ns/read | Read views | Rows cloned | Rows visited | Seeks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| current-view/current-scan | 35 ops/s | 28,641,773 | 1,000 | 100,200,000 | 100,200,000 | 0 |
| current-view/direct-seek | 44 ops/s | 22,561,994 | 1,000 | 100,200,000 | 1,000 | 1,000 |
| borrowed-view/current-scan | 478 ops/s | 2,093,569 | 0 | 0 | 100,200,000 | 0 |
| borrowed-view/direct-seek | 1,127,342 ops/s | 887 | 0 | 0 | 1,000 | 1,000 |

All cases found all 1,000 sampled rows.

## Interpretation

The isolated seek change is not enough. It cuts point row visits from
100,200,000 to 1,000, but throughput remains effectively at the original
storage-next point-read level because every lookup still clones the full read
view.

The isolated borrowed-view change is also not enough. It removes all read-view
captures and row clones, but every lookup still scans 100,200 branch rows.
Throughput improves from tens of ops/sec to hundreds of ops/sec, but it is still
orders of magnitude below the old storage point-read result from PERF-P0.

The combined borrowed-view plus ordered-key seek path removes both
row-proportional costs. It visits one row per sampled key, performs one ordered
table seek per lookup, and reaches the expected million-ops/sec class on this
machine.

## Decision

Promote a deliberately small combined point-read correction.

Do not implement a broad read-view rearchitecture first, and do not implement
point seek alone as a standalone production milestone. PERF-P2 proves each
isolated change leaves a decisive row-proportional bottleneck.

The next production slice should combine only the point-read pieces needed to:

1. avoid deep-cloning table rows for point reads;
2. use the existing ordered internal-key layout for physical-key seek;
3. preserve MVCC visibility, tombstone handling, TTL behavior, source
   precedence, and inherited branch behavior;
4. leave scan/history/lazy immutable-table work out of scope.

This is a point-read correction, not a new secondary index and not a storage
format change.

## Follow-Up Slice

Create a scoped implementation slice, tentatively `PERF-T3T4A`, with these exit
criteria:

1. 100K point latest no longer records row-proportional read-view clones.
2. 100K point latest no longer records row-proportional point row visits.
3. Point read tests cover active, frozen, historical, tombstone, TTL-expired,
   owned, and inherited sources.
4. The production L9 point benchmark moves by at least one order of magnitude
   before any range-scan or append-path performance work starts.

Stop and profile if the production benchmark does not move materially after the
combined point-read correction, because PERF-P2 says the synthetic hot path is
capable of restoring the old serving shape.
