# Historical-fork eager residency — scoping

**Status:** scoped, not started. Surfaced in the BS4.4l review (2026-07). Owner: TBD.
**Severity:** V1 correctness-vs-spec + a hard 100M-scale memory blocker for time-travel workloads.
**Related:** BS4 (`bs4-disk-resident-tables-plan.md`, Open items); product Pathway 29
(`docs/product/pathways/branching-versioning-time-travel.md:176`).

## Summary

Historical / time-travel forks — `fork_at_retained_version` and `fork_at_retained_timestamp`
(public: `BranchAction::ForkAtVersion` / `ForkAtTimestamp` → `api/runtime/mod.rs:1975/1987` →
`branch_lifecycle.rs:721/793`) — **materialize the entire visible state of the source branch into
RAM** and install it as fresh eager tables. Ordinary `fork_current` is copy-on-write (references the
parent's tables via an inherited layer, zero row copy). The historical-fork path is a first-class,
**V1-required** product feature that is exposed through the full `-next` stack (executor-next command →
engine-next service → storage API), and it **violates its own product spec**, which states: *"Avoid
materialized full copies unless COW cannot support the requested point"* (Pathway 29, line 217).

The good news from the root-cause dig: **COW *can* support the requested point — materialization is a
deliberate simplification, not a correctness necessity.** The fix is a MEDIUM change concentrated in the
branch-state straddle invariant + the reachability/compaction lifecycle, and it composes with BS4.4l
(the on-demand fallback materialization is now lazy/disk-resident).

## Current behavior (the cost)

`fork_at_retained_version` (`branch_lifecycle.rs:721`) validates `retained_floor ≤ V ≤ visible_max`, then:

1. `source.fork_snapshot_rows(V, child)` (`branch/state/snapshot.rs:322`) scans the branch's **entire**
   visible state at version `V` — active memtable + frozen tables + **every durable owned-level table**
   (`snapshot.rs:352`, full cursor scan) + all inherited layers — filtering per row to
   `commit_version ≤ V`, re-stamping every row to the **child** branch id, and deduping into a
   `BTreeMap<TableInternalKeyBytes, StorageRow>`. Reports `inherited_layer_count: 0`
   (`branch_lifecycle.rs:788`) — the child references nothing.
2. The `Vec<StorageRow>` is chunked at `SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE = 4096`
   (`snapshot.rs:21`) and rebuilt into fresh L0 tables by `build_snapshot_l0_tables` (`snapshot.rs:618`),
   each opened **eager** via `ImmutableTableReader::open_bytes` (`snapshot.rs:635`).

**Scale:** at 100M rows this is the whole dataset in a `BTreeMap` (plus transient full-dataset copies at
peak: `into_values().collect()` → `from_rows` re-sort), then **~24,400 eager L0 tables** (100M / 4096)
all held fully decoded in RAM simultaneously. **Residency is persistent** — the eager tables stay
resident until background compaction rewrites them (reopened lazily post-4.4l) or the process restarts
(recovery reopens `SnapshotInstall` tables lazily). There is no eager→lazy transition at install.

`checkpoint_rows` (`snapshot.rs:294`) deliberately does the opposite — it excludes owned-level rows with
the comment "materializing them here again would make the snapshot O(database size)." The fork path does
exactly what the checkpoint path warns against.

## Root cause — why it materializes instead of referencing

Materialization is a **deliberate simplification**, not a necessity. The read-path machinery for a COW
historical fork already exists:

- **Version-capped inherited reads work.** Every inherited-layer read caps visibility via
  `BranchEffectiveReadBound::for_inherited_layer(bound, fork_version)` (`branch/read.rs:106-124`) and
  filters per row (`read.rs:134-157`). "Reference parent tables, show only `version ≤ V`" is the normal
  inherited-read mechanism — not something a reference "can't express."
- **A straddle read path exists** (streams + filters a table that contains both `≤ V` and `> V` rows:
  `read.rs:4489-4497`, facts fold `state.rs:693-711`) — but it is gated as "reachable only via unchecked
  test construction."
- **Reachability + on-demand materialization exist.** Inherited tables are pinned by identity
  (`branch/state.rs:167-204`); `materialize_inherited_layer` (`branch/state/materialization.rs:312`)
  re-derives the child's own tables (filtered to `fork_version`) and cuts the reference when the parent
  needs to reclaim them — **and post-BS4.4l that materialization output is lazy/disk-resident.**

**The one hard blocker is a construction-time policy:** `BranchInheritedLayer::new` rejects any
referenced table whose `facts().commit_range().max() > fork_version` (`read.rs:771-775`). `fork_current`
sets `fork_version = current max`, so no current table straddles and the check trivially passes. A
historical fork at `V < current` needs to reference the parent's *current* tables, which straddle `V`
(they hold post-`V` commits interleaved after flush/compaction) — and are therefore rejected. So the
fork sidesteps inherited layers entirely and materializes via the generic snapshot-install pipeline.

**Retention is not the differentiator.** The V-era table *objects* are gone (compaction merged them);
only the logical rows in `[retained_floor, V]` survive *inside* the current straddle tables. Both COW
and materialization read the same current tables and reconstruct `V` by per-row filtering — retention
only bounds how old `V` may be, it does not force materialization.

## Options

### Option A — COW historical fork (recommended; matches the spec)

Construct an inherited layer with `fork_version = V` referencing the parent's current (straddle) tables;
let read-time version filtering serve the as-of-`V` view. Removes the `Vec<StorageRow>` materialization
entirely. Requires:

1. **Relax the whole-table `≤ fork_version` construction invariant** (`read.rs:771`) to admit straddle
   tables, and **promote the currently test-only straddle facts/read paths** (`read.rs:4489`,
   `state.rs:708`) to audited production correctness.
2. **Flush-first precondition** (an inherited layer can only reference sealed durable tables, not the
   active memtable). `fork_current` already requires this; the historical fork would too — or reference
   frozen tables (sealed) and flush the active memtable.
3. **Reachability / release / compaction lifecycle for an *old* `fork_version`** — the heavy part.
   Unlike `fork_current` (pins tables at the live frontier), a historical fork pins the parent's
   *actively-compacting* current tables to an old version, so parent compaction will trip the pin and
   trigger on-demand `materialize_inherited_layer` sooner and more often. That materialization is now
   lazy (BS4.4l), so it is disk-resident and incremental — but the pinning pressure and its interaction
   with parent retention/release must be designed and tested.

Effort: **MEDIUM**, concentrated in (1) the branch-state straddle invariant and (3) the
compaction/reachability lifecycle. Peak RAM becomes O(inherited-layer metadata), not O(dataset).

### Option B — streaming materialization (fallback / simpler)

Keep the independent copy but **stream it to disk** instead of buffering: replace the `BTreeMap` +
`Vec<StorageRow>` + eager `build_snapshot_l0_tables` with a k-way merge over the source layers
(reuse the compaction merge machinery — version-capped, re-keyed to the child), piped through the table
builder and **published + lazy-opened via the BS4.4l path** (`publish_rewrite_artifact`). Bounds peak
RAM to the merge/builder buffers; output is disk-resident.

Effort: **MEDIUM**. Downside: it still *materializes a full copy* (violates the spec's intent) and still
does an O(dataset) durable write per fork — but it has **no cross-branch pinning pressure** and is a
more contained change (no straddle-invariant relaxation, no reachability rework).

### Option C — interim mitigation (not a fix)

Until A or B lands: **document the limitation** (time-travel forks of large branches are memory-bound)
and have the BS4.6 100M exit either avoid historical forks or gate them behind a size guard. Optionally,
bound peak residency by flushing each L0 chunk immediately in `build_snapshot_l0_tables` (drops eager
tables from RAM as they publish) — but the dedup `BTreeMap` stays O(dataset), so this is partial.

## Recommendation

**Pursue Option A (COW).** It is what the product spec asks for, it eliminates the copy rather than
relocating it, and its on-demand fallback is already lazy after BS4.4l. Keep **Option B as the
materialization fallback** for any point COW genuinely cannot serve (with `retained_floor ≤ V` and
flush-first, that set should be empty — but the fallback also covers cross-runtime snapshot loads, which
already use this pipeline). Treat Option C as the interim stance for the BS4.6 exit if A/B do not land
first.

## Effort, dependencies, risks

- **Dependency:** BS4.4l (the on-demand materialization + any fallback build is now lazy). No format
  change required for Option A (inherited layers already persist `fork_version` + table refs); Option B
  reuses the existing snapshot-install format.
- **Related surface — vector index.** `engine_vector/index_manifest.rs:676`
  (`vector_index_manifest_retained_version_fork_materializes_capped_refs`) shows the vector index *also*
  materializes capped references on a historical fork. Any fork-COW work must extend to or preserve the
  vector-index capped-ref path.
- **Risk (Option A):** the old-`fork_version` pinning creates sustained compaction/release pressure on
  the parent; mis-designed, it can stall the parent's compaction or leak references. This is the part to
  design carefully and soak-test.

## Tests a fix must keep green (and one to add)

Existing (must stay green — value + semantic equality is the contract):
- `lifecycle/tests/branch_lifecycle/fork.rs` (13 `fork_at_retained_version` sites; esp.
  `fork_at_history_visible_latest_matches_current_fork:713` and
  `fork_at_history_retained_version_succeeds:687`).
- `api/tests/branch.rs:217/241/270/318` (succeeds / watermark-between-commits / unretained-rejects /
  timestamp-resolves); `tests/api_properties.rs:178`.
- `engine-next/tests/temporal_timeline_model.rs:142` `fork_at_version_equals_source_as_of` (the core
  correctness invariant: a v-fork equals reading the source `as_of` that version).
- `lifecycle/tests/recovery.rs:2073` (recovery after historical fork); `engine_vector/index_manifest.rs:676`.

New (the fix's proof):
- A perf-trace test that forks a large branch and asserts **bounded RAM** — Option A: an inherited layer
  is created (`inherited_layer_count > 0`), no `Vec<StorageRow>` materialization, `lazy_full_materialization
  == 0`; Option B: peak resident stays O(merge buffers) and the output tables install lazy.

## Exit criteria

A historical fork of a 100M-row branch:
1. does **not** materialize the whole dataset in RAM (COW: no copy; or streamed: O(buffer) peak, output
   disk-resident);
2. preserves as-of-`V` correctness (`fork_at_version == source as_of V`) and the retained-version
   rejection errors;
3. is spec-compliant (Pathway 29: "avoid materialized full copies unless COW cannot support the point").
