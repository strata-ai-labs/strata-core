# BS4 — Disk-resident tables: implementation and test plan

Status: **in progress** — 4.1, 4.2, 4.3, 4.4a–i landed (all prep + the behavior-neutral
scaffolding); **remaining: 4.4j (flush/recovery lazy) → 4.4k (compaction outputs lazy) → 4.5
(budget remodel) → 4.6 (exit)**. Milestone BS4 of `billion-scale-plan.md` (gaps
G11–G16; unblocks G9/G20/G21). **This is the regime change**: today every durable table is
fully resident *and decoded* in RAM and the dataset cannot exceed the memory budget; after
BS4 the dataset lives on disk, reads fetch blocks on demand, and memory is bounded by caches
(the RocksDB model, `rocksdb-parity-roadmap.md` RC3). Change class: intentional semantic
change (memory model + additive format extensions). Assurance: S3 with format gates.

## Problem (recap)

`BranchOwnedTable::new` force-materializes every reader into decoded `Arc<[TableRow]>`
(`branch/read.rs:586-609`); disk is durability/recovery only; a 10 GB dataset hard-fails an
8 GB budget (`bootstrap.rs:673-689` — *"the dataset cannot exceed the memory envelope until
lazy block reads make reads incremental"*); DB open decodes every table (O(dataset)).
Billion scale (~1 TB) is architecturally unreachable. Meanwhile the write path is absurd:
durable flush opens a **lazy** reader over the just-written object (`flush.rs:749`) and then
reads the entire object back to materialize it.

## What reconnaissance established (all anchors verified)

> **Status (original recon snapshot).** This section records the pre-implementation survey; its
> line numbers reflect the code *before* 4.4a–i landed and have drifted. The substance still
> holds. For current, re-verified anchors of the remaining work, see the **BS4.4j / BS4.4k / BS4.5**
> sections below (post-4.4i recon).

**The lazy machinery is built and dead — BS4 is mostly wiring plus consumer conversion:**

- The `'static` lazy reader already works in production: every backend is
  `BackendHandle::owned(Arc<dyn Backend>)` (`api/backend.rs:157-167`) →
  `TableObjectService<'static>` → `TableObjectByteSource<'static>` → `open_source` yields a
  Lazy `'static` reader, block cache attached (`service/table.rs:711-736`,
  `table/reader.rs:904-922`). Metadata (footer + index + properties) loads at open; data
  blocks stay on disk; per-block CRC verify + zstd decompress on fetch
  (`format/table/mod.rs:478-563`). Blocks: 256 rows / 64 KB target.
- **Every Lazy read op is implemented** (point seek + candidates, history, bounded cursors,
  `get_exact`) — no panics, no gaps. Hot read paths and the compaction/materialization
  merges already call the lazy-capable APIs. The block cache is created, threaded, and
  keyed — inert only because Eager readers ignore it.
- **The OnceLock trap:** `LazyTableRows.rows: OnceLock` — a single `try_rows()` call
  permanently materializes and pins the table (`reader.rs:387,403-411`). ~20 `.rows()`
  consumers exist; two hidden traps: `PartialEq for ImmutableTableReader` materializes both
  sides (`reader.rs:217-228`), and the panicking wrappers (`reader.rs:965-1010`) become
  reachable-on-disk-error once reads do I/O.
- **The footer already reserves the filter slot** (`filter_block_offset` u64 at +12,
  `filter_block_frame_len` u32 at +20, written as zeros — `format/table/mod.rs:222-223`,
  reject-if-nonzero at `:266-270/:309-315`) and block-type 3 is reserved (`:355-365`; spec
  §17 lines 767/804/842). Activating zeros, not format surgery. Old readers hard-reject
  filtered tables via three independent guards → **reader support must ship before any
  writer emits**. The bloom hash is already portable (seeded FNV-1a-64, byte-at-a-time,
  LSB-first bits — `table/cache.rs:485-575,679-693`).
- **The manifest already carries everything** needed to construct tables at recovery
  without I/O — identity, object handle, byte/row/block counts, commit + timestamp ranges,
  physical + internal key bounds, provenance (`format/table_manifest.rs:179-378`) — except
  `put_rows`/`tombstone_rows`, which have zero production consumers. Recovery currently has
  four redundant O(dataset) decode points.
- **Sizing:** at 100 M × 1 KB (~1,600 tables), always-open reader metadata ≈ 0.15–0.3 GB —
  so the RocksDB-style bounded table-reader cache (open-reader eviction, gap G15) is **not
  needed for the 100M@8GB exit** and is structurally expensive (`BranchOwnedTable` holds the
  reader directly). **Deferred to the 1B tier**, recorded in the umbrella gap table.

## Slices

Ordering rationale: consumer conversions are behavior-neutral on Eager readers (cursors and
facts work identically), so they land **before** the constructor flip. The original single
"flip" slice (4.4f) decomposed into **f→g→h→i** (all ✓ — constructor-extras, lazy-prep,
backend-ownership, variant-collapse) plus the remaining **4.4j** (flush/recovery lazy) and
**4.4k** (compaction outputs lazy); keeping those two separate is what keeps each bounded.
BS4.1, BS4.2/4.3, and BS4.4a are mutually independent.

| Slice | Content | Depends on |
|---|---|---|
| BS4.1 | O(1) sharded block cache **(✓ landed, dark)** | — (cache unreachable until the flip (4.4j) — safe window) |
| BS4.2a | Filter-frame **format codec** + goldens + spec + negatives **(✓ landed, dark)** | — |
| BS4.2b | Reader wiring: `open_bytes`/`open_source` load + attach the filter **(✓ landed, dark)** | 4.2a, 4.1 |
| BS4.3 | Filter frame **writer** (config-gated) **(✓ landed, dark — off by default)** | 4.2 shipped |
| BS4.4a | Count conversions (→ `facts().row_count()`) + PartialEq materialization fix **(✓ landed; committed as 4.4a-i)** | — |
| BS4.4b | Full-scan → cursor conversions (STREAM/PAIRED sites + `try_for_each_reader_row` helper) **(✓ landed; committed as 4.4a-ii-a)** | 4.4a |
| BS4.4c | Facts/extras + fork-filtered aggregate folds (fast-path + cursor fallback + debug oracles) + the 2 lifecycle conversions (`checkpoint_delta_rows`, `preflight`) **(✓ landed)** | 4.4b |
| BS4.4d | Materialization guard (`TableMaterializationPolicy` + `DenyRuntime` gate + `LazyMaterializationDenied` + counter) + fallibility audit (point/history reads → `try_`, `resolve_timestamp` fallible + straddle-cursor) **(✓ landed)** | 4.4c |
| BS4.4e | Design-forcing sites: facts-based pruning fingerprint (§1) + streaming tombstone merge (§1b) + per-key materialization probes (§2) + facts-bounds construction validation (§6) **(✓ landed)**. Sites §3/§4/§5 were already cursor-based (BS4.4a-ii-a/4.4d); their perf tweaks ride 4.4f. | 4.4d |
| BS4.4f | Constructor flip + extras provenance (`BranchOwnedTable::new` takes `extras`; `BuiltTableArtifact` carries it; callers thread it; recovery recomputes it) — **behavior-neutral, readers stay eager (✓ landed)**. `into_materialized` kept as a lifetime bridge; manifest put/tombstone extension deferred to 4.4g. | 4.4e |
| BS4.4g | Lazy-readiness prep — **behavior-neutral (✓ landed)**: oracle hatch (`materialize_rows_for_oracle`, bypasses the guard) + convert prod row-scans (fork/checkpoint) to cursors + source-guard; put/tombstone additive manifest extension (positional, `TableSummaryExtras::from_parts`); Arc-wrap `LazyTableState` metadata; resident/object size seam (`resident_size_bytes`); `fill_cache=false` for merge inputs. Nothing goes lazy yet; guard does not yet bite. | 4.4f |
| BS4.4h | Backend ownership — **behavior-neutral (✓ landed)**: the lazy flip needs durable runtimes to be owned/`'static` (a `BranchOwnedTable` holds `ImmutableTableReader<'static>`). `Arc`-share the Mutex-backed fault/reordering test backends so they're `Clone` → ownable; `durable_backend_handle_for_open` prefers owned for every policy (so the soak tests take the owned path → `DurableOwned('static)`), keeping Background/DeterministicInline's owned requirement. Durable now owns in practice; the `Durable<'a>` variant collapse (183-site `'a` removal) is folded into 4.4i. | 4.4g |
| BS4.4i | Collapse the dead borrowed `Durable` runtime variant — **behavior-neutral (✓ landed)**: deleting `into_materialized` (BS4.4j) needs a type-level `'static` durable runtime, but the generic `Durable(<'a>)` variant blocks it. Remove the variant + merge the 70 paired `Durable\|DurableOwned` arms into `DurableOwned`; remove the borrowed handle machinery (`DurableBackendHandleForOpen`, `open_durable_with_backend_handle`, `as_backend_handle`, `RuntimeSlot::new`); in-memory durable opens keep their errors (`durable_backend_handle_for_open` → `InvalidArgument`/`UnsupportedCapability`). Public `StorageRuntime<'a>`/`StorageOpenOutcome<'a>` retained via `PhantomData` (no 138-site API break). | 4.4h |
| BS4.4j | The lazy flip — durable **flush + recovery** hold **lazy** `'static` readers (behavior-changing; passes the existing suite): delete the `into_materialized` bridge (`branch/read.rs:594`), constructor param → `'static`; recovery builds extras via `from_parts` from the manifest record + the BS4.4g row-split (un-gate the decoder `format/mod.rs:85`, re-add `TableRowSplit::put_rows()`/`tombstone_rows()` getters, thread a running **global** table index through the recovery walk) instead of `from_rows(reader.rows())` (`table_manifest.rs:708`); wire `deny_runtime_materialization()` into the **two** durable configs (`flush.rs:754`, `table_manifest.rs:680` — zero production callers today); lazy `resident_size_bytes` (`reader.rs:1159`); greenfield cold-read/eviction/fault suite. Recon-verified: **exactly 2 still-live materializers** trip the guard (both are this slice's work — the bridge + recovery extras); everything else is already cursor/facts-converted or the oracle hatch. Recovery becomes O(metadata) so **fast open is largely achieved here**. Guard bites here. Compaction outputs stay eager-resident (→ 4.4k); reader-budget charge stays full-object (→ 4.5). | 4.4i, 4.1, 4.2 |
| BS4.4k | Compaction / materialization / rewrite **outputs install lazy** — the second half of the memory-model change: convert the two durable-output sites (`rewrite_publication.rs:747` `open_reader_from_validated_rows`, `materialization.rs:865` `open_bytes`) to metadata-only lazy opens over the just-written object (`extras` already in hand — no rescan), reversing the deliberate "reopen-avoided / rows-reused" optimization; thread `fill_cache=false`. **Split out of 4.4j** because eager readers never reach the `DenyRuntime` guard (`config.rs:176`), so post-flip these outputs stay correct + guard-safe — just fully resident; converting them is what would balloon the flip. Needed for the memory benefit (L1+ tables are compaction outputs ≈ most of a 100M dataset). | 4.4j |
| BS4.5 | Budget remodel (unlocks dataset > budget) + fast-open regression: `require_table_reader_budget` charges **metadata-resident bytes** not full object bytes (`manifest_reader_materialized_budget_bytes` = `byte_count()` today, `table_manifest.rs:692`; flush charges too); raise `max_open_readers` (default 1024 < ~1,600 readers at 100M); demote `would_exceed_total` on the durable commit path to an observability gauge (the flip's lazy `resident_size_bytes` already defused it — no admission failure remains); pool rebalance (block cache ~50%, proportional `from_total_bytes` scaling preserved); update `budget_runtime.rs:310` (durable ≠ cache charge now). Fast-open (mostly delivered by 4.4j's O(metadata) recovery) rides here as a re-audit + open-time I/O-count regression test. | 4.4j, 4.4k |
| BS4.6 | Re-baseline + exit runs — build the 100M-tier harness (`#[ignore]` integration test + benchmark cell; **neither exists yet**); run the exit gates (100M@8GB load+serve, open ≤1 s, 10M scoreboard within 1.5×, `lazy_full_materialization == 0`); subcompaction honest re-A/B (G9); ledger + umbrella gap-table updates. | all |

### BS4.1 — O(1) sharded block cache

> **Landed (dark).** `LruSlab` intrusive slab-index LRU (`Vec<LruSlot>` + `HashMap` index + `u32`
> prev/next links + free list, no `unsafe`) replaces the `BTreeMap`+`VecDeque`; `CacheState` delegates
> with the public API / stats / `current_bytes` contract byte-identical. One refinement from the sketch:
> the CPU-derived `[4,64]` shard count is **capped by capacity** so no shard is smaller than one
> `TARGET_SHARD_BYTES` block — a pure-CPU count would give a small cache sub-block shards that
> oversized-reject every insert (the pre-existing `..._without_tiny_shards` guard). This also keeps the
> small-capacity eviction tests deterministic (they stay at their old shard count). Coverage: a
> deterministic `LruSlab`-vs-reference proptest (eviction-victim exactness), machine-independent
> shard-count units, and a concurrent hammer smoke. Verified `--lib` debug 3186 / release 3181,
> clippy + fmt + wasm clean.

Replace `CacheState { entries: BTreeMap, recency: VecDeque }` — whose `touch_recency` does
an O(n) linear scan under the shard mutex (`table/cache.rs:606-615`) — with an intrusive
slab-index LRU (`Vec<Entry>` + free list + u32 prev/next links; no `unsafe`): O(1) get /
touch / insert / evict. Shard count from CPU count clamped [4, 64] (today: capacity/64 KiB
clamped [1, 16] — degenerates to one global lock, `:623`). Keep the existing API, stats
surface, budget wiring (`budget.rs:199-207,420-433`), oversized-insert rejection, and
`remove_table` invalidation.

**Tests.** Model-based property test vs a reference LRU (identical eviction victims under
identical op sequences); `bytes == Σ entries ≤ capacity` invariant after every op;
shard-count units; multi-thread hammer smoke. Safe-window argument: Eager readers ignore
the cache (`reader.rs:365-374`), so this is unreachable from production until the flip (4.4j).

### BS4.2 — Durable filter frame: reader side

- Footer: accept nonzero filter slots (drop the reject guards `mod.rs:266-270/:309-315`;
  validate the range like index/properties). `TableBlockKind::Filter = 3`. Layout validator
  (`artifact.rs:544-560`) learns the filter-between-data-and-index position (spec §17:767):
  nonzero slot ⇒ `data_end == filter_start`, `filter_end == index_start`; zero slot ⇒
  today's layout.
- Payload codec (LE, inside a standard CRC'd frame): `{filter_format_version: u32 = 1,
  probes: u8, key_count: u64, bit_count: u64, bits}`. Unknown version ⇒ typed reject (the
  forward-compat gate for future partitioned/parameterized filters). **Do not touch the
  hash** — the bit layout is already deterministic and endianness-safe.
- `open_bytes` consumes the frame when present (replacing `build_eager_filter`'s
  scan-and-rebuild, `reader.rs:1497-1515`; `BuildOnOpen` remains the fallback for zero-slot
  tables). `open_source` fetches + decodes the frame at open and populates the
  currently-`None` lazy filter (`reader.rs:396-397`). Loaded filters pass the
  `matches_table` fingerprint gate (`reader.rs:180-191`) before attach — **a corrupt filter
  returning a false `DefinitelyAbsent` would drop live rows**; this gate plus the frame CRC
  is the defense.
- **Goldens/spec:** new fixtures (minimal + multi-block filtered table, standalone filter
  frame) with explicit generation; negative mutations (flipped filter bit ⇒ CRC reject,
  slot pointing outside the data/index gap ⇒ layout reject, truncated frame ⇒ reject);
  **all existing goldens stay byte-identical** (zero slots unchanged). Spec §17 (position +
  payload) and §21 (fixture inventory) updated in this slice.

**Tests.** Round-trip property: N random keys → build → serialize → deserialize → every
inserted key probes `MaybePresent`; fuzz-target extension over the filter frame; golden
harness extended (`tests/format_golden.rs`, `format/table/golden_tests.rs`).

### BS4.3 — Durable filter frame: writer side

- Accumulate per-key hash material at `ImmutableTableStreamingBuilder::append`
  (`builder.rs:155-166`) / `append_streaming_row` (`artifact.rs:198-228`) — bloom sizing
  needs the final key count, so buffer the per-key hash pairs (16 B/key ≈ 4 MB per
  256 K-row table, charged transiently) and size + populate the bits in
  `finish_with_metadata`, writing the frame between the header back-patch
  (`artifact.rs:285`) and the index frame (`:287`); footer slots become real values.
- Config-gated `emit_filter_frame`, default **on** once landed (BS4.2 reader shipped
  first); off-switch retained for A/B and unfiltered golden regeneration. The
  compat line is documented in the spec: binaries older than BS4.2 cannot read filtered
  tables (three hard-reject guards) — acceptable for a pre-release format with no
  migration promises, but the ordering is release-mandatory.

**Tests.** Writer goldens (bit-exact filter payload for fixed input); flush → reopen →
probe integration; crash sweeps over the filter-frame write window (torn/truncated writes
quarantine, never load a partial filter); mixed stores (old zero-slot + new filtered tables
read side by side).

### BS4.4a–d — Consumer conversions + the materialization guard (behavior-neutral)

*(Flattened from the original single "BS4.4a" slice: 4.4a = count/PartialEq, 4.4b = cursor
conversions, 4.4c = aggregate folds + lifecycle, 4.4d = guard + fallibility. The narrative below
describes the whole family.)*

**The guard (three layers):**
1. `TableMaterializationPolicy { Allow, DenyRuntime }` on `TableReaderConfig`:
   `LazyTableRows::try_rows`'s init closure returns a typed
   `LazyMaterializationDenied` error in release (debug-asserts first) under `DenyRuntime`;
   `open_source` (the durable runtime path) sets `DenyRuntime`; `open_bytes`/testkit set
   `Allow`.
2. API narrowing: gate `rows()` accessors behind `#[cfg(any(test, feature = "testkit",
   debug_assertions))]` and rename the escape hatch `materialize_rows_for_oracle()`; a
   source-guard test (existing repo pattern) pins the allowlist (the two debug oracles:
   `state.rs:459`, `compaction.rs:2044`).
3. `perf_trace::record_lazy_full_materialization()` counter — asserted **zero** in
   scoreboard and exit runs.

**Companion fixes:** `PartialEq for ImmutableTableReader` (materializes both sides,
`reader.rs:217-228`) → compare `config + facts + content fingerprint` (sound: sealed tables
are content-immutable and the fingerprint is exact-content). Fallibility audit: runtime
read paths move to the `try_` variants (cursor `advance` is already fallible); panicking
wrappers become test-only; `require_absent_internal_key` (`state.rs:289`) propagates errors.

**Mechanical conversions** (all identical-behavior on Eager readers):
- Counts → `facts().row_count()`: `read_hooks.rs:304-320`, `read.rs:3936-3944`,
  `compaction.rs:1269`, `materialization.rs:684`.
- Full scans → facts/extras folds (the production-proven `observe_rows_from_summaries`
  pattern, `state.rs:429-446`, with its debug oracle): `validate_read_view_inputs`
  (`read.rs:3853`), `read.rs:4082/4135/4227`, `validate_manifest_reader_facts`
  (`table_manifest.rs:877/884` — compare manifest fields vs `reader.facts()`+extras only),
  `recovery.rs:788/794`, `table_manifest.rs:375/381`, `checkpoint.rs:1121-1198`,
  `read_hooks.rs:190-198` (facts prefilter + rare-path cursor).

**Tests.** Per-conversion debug oracles (facts-based == full-scan on small tables); the
full suite green; source-guard; counter units.

### BS4.4e — The six design-forcing sites

1. **Pruning proof (`pruning.rs:651, 679-707`).** Verified: the proof is in-memory-only
   (`Copy`, never serialized), built and validated by the *same* fingerprint function in the
   same process — redefinition is safe if both sides move together (they share one fn).
   **Resolution: identity/facts-based hashing, not row streaming** (streaming would make
   every proof build/validation an O(dataset) disk scan). Per owned table hash: identity +
   materialization-source marker + row/byte counts + commit range + key-range bytes +
   timestamp bounds. Semantic-freeze argument: sealed tables are content-immutable and
   install rejects identity collisions (`state.rs:333-341`), so within a process identity ⟹
   content. Active/frozen rows keep per-row hashing (RAM-resident, memtable-bounded).
   Pinned fingerprint tests updated deliberately; a **sensitivity property suite** (any
   single mutation — add/remove/replace table, level reorder, memtable row, source marker —
   flips the fingerprint; unchanged state is stable) becomes the contract.
   The candidate tombstone-resurrection check (`pruning.rs:608-654`) restructures to a
   streaming merged cursor evaluated key-group by key-group (O(versions-per-key) memory);
   property test: streaming verdict == collected-vec verdict.
2. **Materialization precedence (`materialization.rs:777-833`).** The precomputed
   whole-branch `BTreeMap` (every owned row!) becomes **per-key probes** at the merge loop:
   active/frozen direct probes (memtable-bounded); owned levels via `try_get_exact`
   (index+filter-accelerated); closer inherited layers by swapping the child branch id back
   to the source's in the probe key (the branch id is a fixed 16-byte prefix of the encoded
   key, `format/key.rs:15-24`), probing the source table, filtering
   `commit_version ≤ fork_version`, rewriting the hit, and applying the existing
   shadow-skip/collision rules. Materialization is an event, not a hot path — per-key probe
   cost is acceptable. Differential test: probe-based == map-based on randomized layered
   branches, including shadow and collision cases.
3. **Replacement verify (`materialization.rs:1020/1060`).** Artifact-side `open_bytes`
   (in-hand bytes, Eager, bounded) stays. Table-side: content-fingerprint equality
   (`matches_exact_content`) when the lazy reader carries the exact digest; else a
   **cursor-zip** streaming compare (bounded by artifact size, no pin). Verify during
   implementation which fingerprint fields `open_source` populates.
4. **`frozen_rows_match_table` (`state.rs:614-621`).** Flush install compares the new L0
   table against the frozen memtable **using the artifact rows already in hand pre-publish**
   (`BuiltTableArtifact::into_parts_with_rows`), threaded through the flush publish struct —
   zero re-read, memtable-bounded. A debug-mode cursor-zip against the installed lazy
   reader doubles as a durable round-trip oracle.
5. **`validate_install` dedup (`state.rs:300-310`).** New-table iteration via cursor (or
   the threaded artifact rows); cross-table probes are already point lookups → `try_get_exact`
   (bloom-accelerated after 4.2/4.3).
6. **Inherited-layer / owned-table validation (`read.rs:589-602, 706-747`).** Facts-based,
   exactly equivalent: fork check ⟺ `facts().commit_range().max() ≤ fork_version`
   (commit_range is true per-row min/max from the streaming builder); branch check ⟺
   `extras().physical_key_min()/max()` both carry the expected 16-byte branch-id prefix
   (sound: fixed-width prefix + lexicographic row order bounds every row); non-empty ⟺ free
   (`TableRuntimeFacts` rejects `row_count == 0`).

### BS4.4f–i — Landed prep for the flip (the original single "flip" slice, decomposed)

The original plan had **one** "BS4.4f — The flip" slice. It decomposed into five as each part
turned out to be a hard prerequisite; all but the last two have landed:

- **BS4.4f (✓)** — constructor takes `extras`; `BuiltTableArtifact` carries it out-of-band (no
  table-format change); the `put_rows`/`tombstone_rows` manifest extension shipped in 4.4g.
  `into_materialized` kept as the eager bridge. Behavior-neutral.
- **BS4.4g (✓)** — oracle hatch (`materialize_rows_for_oracle`), prod row-scans → cursors,
  the positional row-split manifest extension + `TableSummaryExtras::from_parts`, Arc-wrapped
  `LazyTableState` metadata (the `Arc::make_mut` install-clone fix), the `resident_size_bytes`
  seam, `fill_cache=false` for merge inputs. Behavior-neutral; guard does not yet bite.
- **BS4.4h (✓)** — backend ownership: durable runtimes own their backend (`'static`) in practice.
- **BS4.4i (✓)** — removed the dead borrowed `Durable<'a>` runtime variant so the durable runtime
  is type-level `'static` (the flip's constructor demands a `'static` reader). `'a` retained on
  the public `StorageRuntime<'a>` via `PhantomData`.

What remains is the actual regime change, split into **flush/recovery-lazy (4.4j)** and
**compaction-outputs-lazy (4.4k)** — see below.

### BS4.4j — The flip: flush + recovery hold lazy readers

Recon (post-4.4i) established this is **bounded**: durable flush (`flush.rs:751`) and recovery
(`table_manifest.rs:676`) readers **already open lazy** (`open_reader` → `open_source` →
`TableReaderRows::Lazy`), held `'static` after 4.4h/i but collapsed to eager at install by the
bridge under an `Allow` policy. Exactly **two still-live materializers** trip the `DenyRuntime`
guard once it is wired — both are this slice's work, both already carry in-code `BS4.4g` "delete/
replace when durable readers go lazy" comments. Everything else on the durable path is already
cursor/facts-converted (4.4a–e) or routes through the guard-bypassing oracle hatch.

- **Delete the bridge:** remove `reader.into_materialized()` (`branch/read.rs:594`); change the
  constructor param `reader: ImmutableTableReader<'_>` → `ImmutableTableReader<'static>` (the
  field is already `'static`). The public `into_materialized` method + its one test caller
  (`service/table.rs:2611`, `#[cfg(test)]`) can then go too.
- **Recovery extras from the manifest, not the rows:** replace
  `TableSummaryExtras::from_rows(reader.rows())` (`table_manifest.rs:708`) with `from_parts`
  (`table/facts.rs:235`) from the manifest record bounds (timestamp/physical-key) + the row-split
  put/tombstone counts. Requires: un-gate `decode_table_row_split_extension_section`
  (`format/mod.rs:85`, currently `#[cfg(test)]`); re-add `TableRowSplit::put_rows()`/
  `tombstone_rows()` getters (`format/table_row_split_extension.rs`, fields exist, accessors
  don't); thread a running **global** `&mut usize` index through the recovery walk
  (`recover_manifest_levels`'s two call sites — top-level `:573`, inherited `:608` —
  → `recover_manifest_table` → `branch_table_from_reader`), because `order()` is per-level while
  the row-split Vec is flat/global. This mirrors the write side, which already threads
  `row_splits: &mut Vec<TableRowSplit>` in lockstep.
- **Wire the guard:** add `.deny_runtime_materialization()` (`table/config.rs:205`, **zero
  production callers today**) to the durable flush config (`flush.rs:754`) and recovery config
  (`table_manifest.rs:680`). Keep it off the compaction/build eager configs (harmless if it
  leaked — eager ignores policy — but keep the intent explicit).
- **Lazy `resident_size_bytes`:** override `resident_size_bytes()` (`table/reader.rs:1159`,
  returns `byte_count()` today) to return resident metadata (index Vec + properties + loaded
  filter) + cached-block bytes for the lazy case. Only one caller needs it —
  `BranchOwnedTable::approximate_size_bytes` (`branch/read.rs:691`, memory accounting); compaction
  scoring reads `facts().byte_count()` directly. **NB:** there is no `object_size_bytes` method —
  the seam is `resident_size_bytes()` vs `facts().byte_count()`. This change also **defuses
  `would_exceed_total`** for durable data (the residency total drains through this seam), so no
  existing durable budget test breaks.
- **Fast open, for free:** removing the two materializers makes recovery/open **O(metadata)**
  (lazy `open_source` reads footer + index + properties + a targeted filter range — no row scan).
  So exit gate #2's latency is largely delivered here; BS4.5 only adds the regression test + an
  audit for creep.

**Tests.** Greenfield `tests/disk_resident_reads.rs` cold-read / eviction / recovery suite (below):
`lazy_full_materialization == 0`, `cache miss → hit`, branching cases. **Cost to watch:** tests that
call `.rows()` on a durable-flush/recovery-backed table trip the guard once `DenyRuntime` is wired —
bounded test-conversion work (→ cursors or the oracle hatch); likely small (most tests use
`Allow`-policy testkit readers), verify early.

### BS4.4k — Compaction / materialization / rewrite outputs install lazy

Durable compaction/materialization/rewrite **outputs** install **eager `'static`**
(`rewrite_publication.rs:747` `open_reader_from_validated_rows` reusing in-hand rows;
`materialization.rs:865` `open_bytes`). Eager readers **never reach** the `DenyRuntime` guard
(`table/config.rs:176`), so after 4.4j they are correct and guard-safe — just fully resident. This
slice makes them disk-resident:

- Convert each output site to a **metadata-only lazy open** over the just-written object
  (`open_source`/`open_reader` on `object_facts`) instead of reusing decoded rows; take `extras`
  from `artifact.extras()` (already in hand). For `rewrite_publication` this **reverses** the
  deliberate "reopen-avoided / rows-reused" optimization (`perf_trace::record_table_rewrite_
  reader_reopen_avoided`) — churn those perf-traces.
- Thread `fill_cache=false` so the reopen populates metadata without warming the block cache.
- **Cache-mode compaction stays eager** (`compaction.rs:1491`, C2 — its dataset is RAM-resident).

**Why separate from 4.4j:** bundling it is exactly what made "the flip" balloon before. It is
correctness-neutral relative to 4.4j (guard-safe either way) but MED–LARGE (three reopen sites +
a reversed optimization). **Why not deferrable:** at 100M, L1+ tables (compaction outputs) are the
bulk of the dataset, so leaving them resident blows the 8 GB budget — required for the exit.

**Tests.** Cold-read suite extended to compacted/materialized tables; materialization counter zero
across compaction stress; the "reopen avoided" perf-trace assertions updated to the new lazy path.

### BS4.5 — Budget remodel + fast-open regression

Two independent axes (they share no code); both are needed to make the exit gates reachable.

- **Budget remodel** (`budget.rs`, `lifecycle/durable/bootstrap.rs`, `table_manifest.rs`,
  `flush.rs`, `cache.rs`) — the hard prerequisite for exit gate #1 (dataset > budget):
  - `require_table_reader_budget` charges **metadata-resident bytes** (footer + index +
    properties + filter-frame lengths — all known pre-decode) instead of full object bytes.
    Today `manifest_reader_materialized_budget_bytes` (`table_manifest.rs:692`) returns
    `object_facts.byte_count()` and flush charges `artifact.byte_count()` — so ~1,600 lazy readers
    at 100M still charge the full ~100 GB and **cannot open** until this changes.
  - **Raise `max_open_readers`** (default 1024, `budget.rs:66`) — the count cap also bites at
    ~1,600 tables; keep it as a raised backstop.
  - **Demote `would_exceed_total` on the durable commit path** (now at
    `lifecycle/durable/bootstrap.rs:840`, in `require_projected_mutating_commit_budget`) to an
    observability gauge (health WARN if measured residency exceeds budget) — 4.4j's lazy
    `resident_size_bytes` already stops it tripping, so there is no admission failure to preserve.
    **Cache mode keeps its check** (`cache.rs:1003`, C2).
  - Pool rebalance: block cache becomes the primary (~50 %); proportional `from_total_bytes`
    scaling preserved (512 MB edge ↔ 64 GB server, one code path; final fractions tuned in 4.6).
  - Update `budget_runtime.rs:310` (durable and cache reader charges now diverge). Memtable
    pressure paths (rotation, frozen backlog, throttle) untouched.
- **Fast open** — exit gate #2 (open ≤ 1 s), mostly delivered by 4.4j's O(metadata) recovery:
  re-audit the open path for residual O(rows) creep (checkpoint reconcile, preflight, read-view
  validation must be facts-based on the open path or demoted to the offline oracle); add an
  open-time I/O-count regression test (O(tables), not O(rows)); parallelize manifest-replay opens
  only if the ≤ 1 s gate demands it (`cfg(not(wasm32))`).

**Tests.** Open-time I/O-count regression; the previously-hard-failing dataset>budget bootstrap
test now asserts **success**; cache-mode rejection still passes; budget-fraction units; pressure
paths unchanged-green.

### BS4.6 — Re-baseline + exit

100M@8GB exit run; open-time measurement; full 10 M scoreboard re-baseline; **subcompactions
honest re-A/B** (gap G9 — compaction is now I/O-bound, the regime Slice 4 was built for);
zstd/readahead assessment recorded for BS6; ledger rows + umbrella gap-table updates
(G11–G16 closed; G15 → 1B-tier follow-up).

## Test strategy (cross-slice)

- **Cold reads** — new `tests/disk_resident_reads.rs`: write via public API → drop runtime →
  reopen → point (present/absent) / bounded scan / history / timestamp reads equal a
  pre-close capture; `lazy_full_materialization == 0`; cache stats show miss-then-hit.
  **Branching cases (constraint C4), required in the same suite:** forked branches with
  active inherited layers read correctly through lazy parent tables after reopen; a branch
  mid-materialization (Materializing status) recovers and reads correctly; fork-after-reopen
  is O(1) (latency pinned) and the child's inherited reads hit the parent's lazy readers
  through the shared block cache.
- **Eviction correctness** — block cache sized to 2–4 blocks, randomized reads over
  multi-block tables: correct results + `bytes ≤ capacity` invariant; **cache-disabled
  (capacity 0) still serves reads correctly** (the cache must be an optimization, never a
  correctness dependency).
- **Faults** — read-time I/O fault injection (typed errors, never panics — validating the
  4a fallibility audit); filtered-write crash windows; recovery oracle + fault sweep every
  slice; goldens: old byte-identical + new filtered + negative mutations.
- **100 M tier** — `#[ignore]`-gated integration test + a benchmark-harness step, run in the
  perf environment (nightly/explicit), not per-PR.

## Exit gates (milestone)

1. **100 M × 1 KB (~100 GB) loads and serves on an 8 GB budget** — the cell that hard-fails
   today.
2. **DB open ≤ 1 s at 100 M.**
3. **10 M scoreboard cells within 1.5× of the BS2/BS3 results** (regression priced in;
   reserve mitigations below if exceeded).
4. Goldens + crash-recovery byte-identity + recovery oracle + fault sweep green across both
   format extensions (filter frame, manifest put/tombstone fields).
5. `lazy_full_materialization == 0` in all exit runs; standing gates (clippy `-D warnings`,
   fmt, source guards, fuzz inventory).

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** lazy readers + the block cache are pure computation over the backend trait
  (`read_at`) — wasm-safe with the memory backend; **no new threads** (block fetches run on
  the calling thread). The one optional parallelism (BS4.5's parallel manifest-replay
  opens, if the 1 s gate demands it) goes behind `cfg(not(target_arch = "wasm32"))` /
  the executor abstraction. Wasm check-build in every slice's gates.
- **C2 (cache mode):** cache-mode tables **stay Eager** (its dataset is genuinely
  RAM-resident by product policy — `open_bytes`, no backend objects) and it **keeps** its
  `would_exceed_total` rejection; only the durable path changes regime. Cache suites run
  unmodified as a gate on 4.4j/4.4k and 4.5.
- **C3 (profiles):** the pool rebalance preserves proportional `from_total_bytes` scaling —
  one code path for the 512 MB edge tier through the 64 GB server tier; validated at the
  explicit tier matrix (512 MB / 8 GB / 64 GB) including block-cache minimums (a tier must
  never produce a zero/degenerate cache while enabled). The budget model change (caches
  bound RAM, not dataset) is precisely what makes small-RAM/large-disk profiles (Raspberry
  Pi) possible.
- **C4 (branching):** inherited-layer validation is facts-based (4.4e §6), materialization
  precedence uses per-key probes with the branch-id prefix swap (4.4e §2), the pruning
  fingerprint hashes inherited layers identically (4.4e §1), and the cold-read suite's
  branching cases (above) gate reopen correctness. Fork remains O(1): snapshot/aggregate
  publication at fork is Arc-cloning, pinned by a fork-latency assert.

## Risks

| Risk | Mitigation |
|---|---|
| Hot-workload regression beyond 1.5× (block decode per hit vs decoded slices) | BS4.1 lands a RocksDB-grade cache first; per-slice scoreboard from 4.4j; reserves: cache decoded row-blocks (`Arc<[TableRow]>` entries with approximate charging), pin L0/index-adjacent blocks |
| Filter false-`DefinitelyAbsent` drops live rows | frame CRC + `matches_table` fingerprint gate + bit-exact goldens + round-trip property (every inserted key MaybePresent) + negative mutations |
| Compat ordering (old binaries hard-reject filtered tables) | reader (4.2) fully released before writer (4.3) emits; writer config-gated; mixed-store tests; spec documents the line |
| Pruning-proof semantic drift | in-memory-only proof + single shared fingerprint fn + sensitivity property suite; pinned tests updated deliberately in one slice |
| Snapshot-clone blowup under `Arc::make_mut` | Arc-wrapped metadata/filter in 4.4g; install-path allocation assertion in stress tests |
| Lazy I/O errors panic via `expect` wrappers | 4a fallibility audit + fault-injection read tests |
| OnceLock trap regression | DenyRuntime policy + cfg-gated API + counter + source-guard test |
| Recovery scans creep back to O(rows) | BS4.5 re-audit + open-time I/O-count regression test |

## Sequencing & PR discipline

BS4.1 ∥ BS4.2→4.3 ∥ BS4.4, then 4.4a→4.4b→4.4c→4.4d→4.4e→4.4f→4.4g→4.4h→4.4i (all ✓) →
**4.4j → 4.4k → 4.5 → 4.6** (remaining). One PR per slice, `BS4.{n}` titles, ≤1,500 LOC net each,
standing gates every slice; format-touching slices (4.2/4.3, the manifest extension in 4.4f/g)
additionally gate on goldens + fuzz + crash sweeps. The order is forced from 4.4j onward: the flip
(4.4j) must precede compaction-outputs-lazy (4.4k), and 4.5's budget relaxation is only safe once
**both** are lazy (else resident compaction outputs still blow the ceiling). The umbrella plan's
"table-reader cache" step is amended: **deferred to the 1B tier** (G15), with the sizing arithmetic
recorded.

## Open items

- ~~`open_source` exact content digest for replacement verify vs cursor-zip fallback — decide in
  4.4e.~~ **Resolved in 4.4e.**
- ~~Manifest put/tombstone fields vs Option-degrade — decide in 4.4f.~~ **Resolved: added as the
  positional row-split extension (4.4g); recovery consumes it in 4.4j.**
- Block-size tuning (64 KB blocks vs RocksDB's 4–64 KB) and decoded-row-block caching —
  measure in 4.6, feed BS6.
- Parallel manifest-replay opens for the 1 s gate — build only if the 4.5/4.6 measurement demands.
- `resident_size_bytes` lazy estimate precision (metadata Vec + properties + loaded filter + cached
  blocks) vs a simpler footer-derived constant — decide in 4.4j against the budget-accounting tests.
