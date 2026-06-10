# M4P-L6L Implementation Plan: Branch Read Hot Path Cleanup

Status: draft follow-on implementation plan

Parent branch LSM plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-implementation-plan.md`

Related compaction closure plan:
`docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-implementation-plan.md`

Related compaction hot-path plan:
`docs/architecture/implementation-plans/M4P/m4p-l6k-table-compaction-hot-path-implementation-and-test-plan.md`

Audit context:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Serving-path context:
`docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

Earlier point-read proof plan:
`docs/architecture/perf-tuning/perf-i1-point-read-fix-plan.md`

## Objective

Restore storage-next branch point reads to the old storage mechanical shape:
probe branch sources in visibility order, stop as soon as no remaining source
can beat the selected row, reuse prepared lookup keys across table probes, and
avoid cloning loser rows.

This plan starts from the current post-L6K reality:

1. table-local physical-key seeks exist;
2. nonzero LSM levels already binary-search table key ranges and probe at most
   one table per nonzero level;
3. L9 point reads can already use the borrowed branch state path;
4. both `BranchReadView::read_point` and
   `BranchLocalState::read_point_or_tombstone_borrowed` still call
   `visible_point_candidates`, which collects every point candidate from active,
   frozen, owned, and inherited sources before selecting the winner.

The goal is not to change MVCC, tombstone, TTL, fork-version, timestamp, scan,
history, or durable object semantics. The goal is to make the same semantics
execute with bounded source probes and bounded allocation.

## Current Code Reality

The current point-read hot path is centered on
`crates/storage-next/src/branch/read.rs`.

Important current facts:

1. `BranchReadView::read_point` validates branch id and timestamp coverage, then
   calls `visible_point_candidates`, then `select_visible_row`.
2. `BranchLocalState::read_point_or_tombstone_borrowed` validates branch id and
   timestamp coverage, then calls the same `visible_point_candidates`, then
   `select_visible_row_or_tombstone`.
3. `visible_point_candidates` always probes active, every frozen table, every
   owned level, and every readable inherited layer. It does not return early
   after a local hit.
4. `collect_owned_level_point_candidates` already treats L0 and nonzero levels
   differently: L0 probes every table; nonzero levels use
   `select_nonzero_level_point_table`.
5. Inherited point reads use the same L0/all and nonzero/one-table pattern after
   rewriting the child physical key to the source branch id.
6. Each point hit becomes a `StorageRow` clone inside a `Vec<CandidateRow>`,
   even when a newer row has already made that hit irrelevant.
7. Eager table seeks in `crates/storage-next/src/table/reader.rs` build
   `TablePhysicalKeyBytes` and `TableInternalKeyBytes` inside each table probe.
8. Lazy table seeks already have a filter hook, but eager in-memory table seeks
   do not consult a physical-key filter before binary search.

## Old-Source Shape To Recover

Old storage point reads in `crates/storage/src/segmented/mod.rs` followed this
general shape:

1. encode the lookup key once;
2. probe active;
3. return on a visible active hit;
4. probe frozen tables in newest-first source order;
5. probe L0 tables in source order;
6. probe at most one table per nonzero level;
7. walk inherited sources only if local sources did not answer;
8. use segment/table filters to reject absent keys before touching index/data
   blocks;
9. clone or materialize only the selected row.

Storage-next should preserve its current branch abstractions and table reader
interfaces, but the hot path should have the same asymptotic behavior.

## Non-Goals

This plan does not own:

1. range-scan or prefix-scan source planning;
2. history read source planning;
3. automatic flush or automatic compaction scheduling;
4. durable Bloom/filter block format changes;
5. a new secondary point index;
6. global/sharded block cache replacement;
7. lazy data-block stream decoding, unless profiling after the early slices
   proves lazy block decode is the next dominant point-read cost;
8. public L9 API semantics.

Global block cache and lazy block stream decoding remain valid later work, but
they should not block the first branch read cleanup. The first target is to stop
probing and cloning rows from sources that cannot affect a point read answer.

## Correctness Rules

The implementation must preserve these rules:

1. A returned visible row is the highest commit version that satisfies the
   effective read bound, excluding tombstones and TTL-expired rows when the
   caller asks for visible rows.
2. The borrowed tombstone path must return the selected tombstone instead of
   skipping it, so local deletes continue to shadow inherited rows.
3. Timestamp reads must continue to require timestamp coverage before probing.
4. TTL expiration remains timestamp-bound only; latest and version reads do not
   apply wall-clock TTL filtering at this layer.
5. Inherited rows must still rewrite the source branch id to the child branch
   id only when returning a selected row.
6. Inherited row visibility remains capped by the inherited layer fork version.
7. Source ordering is still the tie-breaker only after commit version ordering.
8. Early exit is allowed only when source facts prove that no remaining source
   can produce a row with a higher visible commit version than the selected
   candidate.

The last rule is important. The implementation should not rely on informal
"active is newer than frozen" assumptions without checking or preserving the
branch/table commit-version facts that make the early return safe.

## Implementation Slices

### L6L-A. Point Read Counters And Baselines

Goal: make source traversal and allocation changes measurable before changing
the selector.

Work:

1. Extend existing perf counters, gated behind the current perf tracing surface,
   to capture:
   - branch point probes by source kind;
   - branch point early exits by source kind;
   - branch point remaining-source skips;
   - branch point candidate row clones;
   - inherited key rewrites;
   - table prepared-key builds;
   - table prepared-key reuses;
   - eager filter probes, negative probes, and unavailable probes;
   - selected source kind.
2. Keep existing `BranchPointSourceCounts`, `point_table_seeks`,
   `point_candidates_materialized`, and `table_point_rows_visited` counters.
3. Add narrow mechanical tests that assert counter reset/snapshot behavior
   without turning every correctness test into a perf-trace test.
4. Record baseline counter output for:
   - active hit;
   - frozen hit;
   - owned L0 hit;
   - owned L1+ hit;
   - inherited hit;
   - miss across a compacted branch.

Exit gate:

1. The baseline proves current reads still traverse unnecessary sources.
2. The counters can distinguish table seeks from candidate clones.
3. The counters can prove a later early exit is real, not just a changed test
   fixture.

### L6L-B. Prepared Point Lookup Keys

Goal: encode point lookup bytes once per branch point read, not once per table
probe.

Work:

1. Add a table-level prepared lookup type, for example
   `TablePreparedPointLookup`, that owns or borrows:
   - `TablePhysicalKeyBytes`;
   - `TableInternalKeyBytes` seek key for `(physical_key, seek_version)`;
   - max commit version;
   - max commit timestamp.
2. Add prepared seek methods for:
   - `MutableTable`;
   - `FrozenTable`;
   - `ImmutableTableReader`.
3. Keep the existing unprepared methods as small wrappers that build the
   prepared lookup once and call the prepared method.
4. Reuse the same prepared lookup for active, frozen, and owned local sources.
5. For inherited sources, build one prepared lookup per readable inherited layer
   after rewriting the key to that layer's source branch id.
6. Move eager and lazy reader internals to consume the prepared lookup so eager
   and lazy paths share the same key bytes.

Exit gate:

1. A point read over many local tables records one local prepared-key build,
   not one per table.
2. A forked point read records at most one inherited prepared-key build per
   readable inherited layer.
3. Existing table seek behavior and table row-visit counters are unchanged
   except for the key-preparation counters.

### L6L-C. Ordered Branch Point Selector With Safe Early Exit

Goal: replace "collect every candidate then sort" with ordered probing plus a
safe stop condition.

Work:

1. Replace `visible_point_candidates` for point reads with a selector that
   probes sources in current source-order groups:
   - active;
   - frozen tables in source order;
   - owned L0 tables in source order;
   - owned nonzero levels, at most one table per level;
   - inherited layers in layer order, each with L0/all and nonzero/one-table
     probing.
2. Maintain the best selected point candidate seen so far.
3. After each source group, compute whether any remaining source can beat the
   candidate by commit version under the current effective bound.
4. Use branch/table facts for the remaining-source max commit version. For
   inherited layers, cap the remaining max by that layer's fork version.
5. Stop immediately when the best candidate exists and the remaining max commit
   version is less than or equal to the selected candidate version.
6. For `BranchReadView::read_point`, return `None` if the selected row is a
   tombstone or timestamp-expired row.
7. For `BranchLocalState::read_point_or_tombstone_borrowed`, return the
   selected tombstone as a `BranchHistoryRow` and only suppress timestamp-expired
   non-tombstone rows.
8. Preserve existing invalid branch, timestamp coverage, and inherited key
   rewrite errors.
9. Keep the old candidate collection available only as a test/reference helper
   if needed, not on the production point-read path.

Exit gate:

1. Active hits do not probe frozen, owned, or inherited sources when source
   facts prove they cannot beat the active row.
2. Local tombstones stop inherited source traversal when they are the selected
   row.
3. Misses still probe every source that might contain the key.
4. Historical and timestamp-bounded reads stop only when remaining source facts
   make the stop safe.
5. Point read output is byte-for-byte equivalent to the previous selector on
   generated branch fixtures.

### L6L-D. Borrowed Point Candidates And Deferred Row Clones

Goal: clone only the selected row, not every row that happens to match the key
in older layers.

Work:

1. Introduce an internal point candidate representation that can hold:
   - a borrowed `&TableRow` for local sources;
   - source metadata;
   - inherited rewrite metadata for inherited sources.
2. Apply visibility checks against borrowed row data.
3. Clone or rewrite only after the selector has chosen the winning candidate.
4. Keep inherited branch-id rewrite delayed until final materialization.
5. Make the borrowed representation local to branch read code. Do not expose
   borrowed table rows through public APIs.
6. Keep history reads on their existing owned collection path in this slice.

Exit gate:

1. Multi-layer point reads clone at most one `StorageRow`.
2. Dropped loser candidates do not clone row values.
3. Inherited rows rewrite branch id only for the selected row.
4. Existing returned `BranchVisibleRow` and `BranchHistoryRow` ownership remains
   unchanged.

### L6L-E. Eager Table Physical-Key Filters

Goal: avoid binary searching eager in-memory immutable tables on definite-absent
point probes.

Work:

1. Attach a physical-key Bloom filter to eager `ImmutableTableReader` state when
   the reader is opened from decoded rows or decoded table bytes.
2. Reuse the existing `TableReaderFilter` validation and probe semantics where
   possible.
3. Keep unavailable-filter behavior as the fallback path.
4. Probe the eager filter before `seek_physical_key_in_slice`.
5. Add equivalent optional physical-key filters for frozen tables only if
   measurement after immutable filters shows frozen misses remain material.
6. Do not introduce durable filter bytes in this slice. Durable filter block
   persistence remains an L8/table-format follow-up.

Exit gate:

1. Eager immutable readers record negative filter probes for absent keys.
2. A negative eager filter probe records zero table rows visited.
3. Positive and false-positive filter probes remain correct.
4. Filter construction cost is paid at reader/table creation, not per point
   read.

### L6L-F. Inherited Layer Short-Circuiting

Goal: prevent forked branch reads from walking inherited sources after a local
row or tombstone already answers the read.

Work:

1. Treat inherited layers as a later source group in the ordered selector.
2. Before entering inherited layers, compare the selected local candidate
   version with the max visible inherited version across readable layers.
3. If inherited sources cannot beat the selected local row or tombstone, record
   an inherited short-circuit and return.
4. Within inherited traversal, apply the same early-exit rule layer by layer.
5. Preserve fork-version caps and materialized-layer skip rules.

Exit gate:

1. Child-local rows skip inherited traversal when they beat the inherited max.
2. Child-local tombstones skip inherited traversal when they beat the inherited
   max.
3. If an inherited layer can contain a higher visible row under the bound, the
   selector still probes it.

### L6L-G. Lazy Data-Block Point Seek Follow-Up

Goal: eliminate full data-block row materialization for lazy point reads if it
remains a measured bottleneck after L6L-A through L6L-F.

Work:

1. Add a lazy block point cursor that scans encoded entries in place instead of
   decoding the entire data block into `Vec<TableRow>`.
2. Reuse existing checksum/frame validation.
3. Decode only candidate rows in the physical-key chain.
4. Keep the existing full-block decode path for scans until scan cleanup owns
   that work.

Exit gate:

1. Lazy point reads decode only the matching key-chain rows on a cold block.
2. Lazy point read results match the existing block decode path.
3. Corrupt block behavior remains unchanged for the queried block.

This slice should not start until benchmark counters show lazy block decode is
still material after source traversal is bounded.

### L6L-H. Block Cache Architecture Decision

Goal: decide whether per-table block caches must become a shared or sharded
cache for point reads.

Work:

1. Profile point reads after L6L-A through L6L-G.
2. Measure block cache lock contention and hit-rate fragmentation.
3. If material, write a separate cache implementation plan that owns:
   - shared cache keying by table id and block index;
   - sharding or lock-free behavior;
   - memory budget accounting;
   - eviction correctness.

Exit gate:

1. A decision note either defers cache architecture or promotes a separate
   cache slice with measured evidence.

## Execution Order

Recommended order:

1. L6L-A counters and baselines.
2. L6L-B prepared lookup keys.
3. L6L-C ordered selector and safe early exit.
4. L6L-D deferred row cloning.
5. L6L-F inherited short-circuiting, if not fully landed in L6L-C.
6. L6L-E eager table filters.
7. Rerun 100K and 1M point-read benchmarks after manual flush and explicit
   compaction drain.
8. Decide whether L6L-G or L6L-H is justified by the new counters.

L6L-B through L6L-D can share some edits, but they should have separate counter
gates. If the early-exit selector changes more than one semantic surface at
once, keep the old selector under a test-only reference helper until generated
equivalence tests are passing.

## Expected Counter Movement

For latest point reads after manual flush and explicit compaction:

1. active hits: one table seek, one candidate, one row clone, no inherited
   probes;
2. owned L1+ hits: active plus frozen plus L0 as needed, at most one table seek
   per nonzero level before the hit can be proven final;
3. misses: table seeks remain proportional to possible sources, but eager
   filter negatives should cut row visits to zero for filtered immutable
   tables;
4. forked child-local hits: zero inherited table seeks when local max version
   beats inherited max;
5. prepared key builds: one local build per point read, plus at most one build
   per readable inherited layer that is actually entered;
6. point candidate row clones: at most one per point read result.

## Stop Conditions

Stop and write a short decision note before continuing if any of these happen:

1. Source facts cannot safely prove early-exit eligibility for common latest
   reads.
2. Generated equivalence tests find a case where the ordered selector disagrees
   with the current candidate collector.
3. The selector fix lands but 100K point-read throughput does not materially
   improve and counters show source traversal is bounded.
4. Eager filter construction cost materially slows writes, flush, or table open.
5. Lazy block decode or block cache contention becomes the new dominant cost
   center.

