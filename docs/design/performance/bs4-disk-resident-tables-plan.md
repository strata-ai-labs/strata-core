# BS4 — Disk-resident tables: implementation and test plan

Status: **ready to implement after BS3**. Milestone BS4 of `billion-scale-plan.md` (gaps
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
facts work identically), so they land **before** the constructor flip; the flip itself
becomes small and revertable. BS4.1, BS4.2/4.3, and BS4.4a are mutually independent.

| Slice | Content | Depends on |
|---|---|---|
| BS4.1 | O(1) sharded block cache | — (cache unreachable until 4c — safe window) |
| BS4.2 | Filter frame **reader** + goldens + spec | — |
| BS4.3 | Filter frame **writer** (config-gated) | 4.2 shipped |
| BS4.4a | `.rows()` conversions + OnceLock guard + equality fix + fallibility audit | — |
| BS4.4b | The six design-forcing sites | 4a |
| BS4.4c | The lazy constructor flip + write-side installs | 4a, 4b, 4.1, 4.2 |
| BS4.5 | Fast open + budget remodel | 4c |
| BS4.6 | Re-baseline + exit runs | all |

### BS4.1 — O(1) sharded block cache

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
the cache (`reader.rs:365-374`), so this is unreachable from production until 4c.

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

### BS4.4a — Consumer conversions + the OnceLock guard (behavior-neutral)

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

### BS4.4b — The six design-forcing sites

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

### BS4.4c — The flip

- **Constructor:** delete `into_materialized()` (`read.rs:586-588`) and
  `TableSummaryExtras::from_rows` (`:608`); new signature
  `BranchOwnedTable::new(branch_id, descriptor, reader, extras)`. Extras provenance:
  build-time sites get them from the streaming builder (accumulated during
  `append_streaming_row` alongside the existing commit min/max — carried **out-of-band on
  `BuiltTableArtifact`**, no table-format change); recovery gets them from the manifest
  record, with `put_rows`/`tombstone_rows` added as **two u64s in an additive, versioned
  manifest extension** (goldens + fuzz updated; fallback if judged not worth it:
  Option-degrade the two fields — they feed only a debug oracle).
- **Durable flush keeps its lazy reader** (`flush.rs:749`) — kills the write-then-re-read.
  **Durable compaction outputs install lazy** post-publish (metadata-only open over the
  just-written object) instead of `from_validated_rows` Eager. **Cache mode stays Eager**
  (its dataset is genuinely RAM-resident by product policy).
- **`fill_cache=false`:** a cursor/read option so compaction + materialization merge inputs
  check the block cache but never insert (RocksDB semantics); plumbed through
  `TableCompactionInput` cursor creation.
- **Arc-wrap reader metadata + filter** inside `LazyTableState`: `BranchLayout` mutates via
  `Arc::make_mut` (deep-cloning every table per install) — without this, each install would
  copy ~80–200 KB of index per table. Clones become pointer bumps; the `OnceLock` clones
  empty (the guard guarantees it is never populated at runtime).
- **`approximate_size_bytes` splits**: `object_size_bytes()` (full object bytes — compaction
  scoring/level sizing keep this) vs `resident_size_bytes()` (metadata estimate — memory
  accounting). Every caller audited for which semantic it needs.
- Reader-budget charging unchanged in this slice (BS4.5 fixes the cost model); verify
  default budgets still admit the test workloads.

**Tests.** The cold-read suite (below); all suites + fuzz targets green with lazy tables;
materialization counter zero across stress + scoreboard smoke; crash sweeps green; 10 M
scoreboard cells run **from this slice onward** (not only at 4.6).

### BS4.5 — Fast open + budget remodel

- **Fast open.** With 4a's facts-based manifest validation and 4c's lazy constructor,
  recovery's O(dataset) decodes are gone by construction. Re-audit the open path for
  O(rows) creep: anything converted to *cursor streaming* rather than facts (checkpoint
  reconcile, preflight, read-view validation) must be facts-based on the open path or
  demoted to the offline recovery oracle. Reader open at recovery = footer + index +
  properties + filter reads (3–4 I/Os × ~1,600 tables at 100 M); parallelize manifest-replay
  opens if the ≤1 s gate demands.
- **Budget remodel** (`budget.rs`, `bootstrap.rs`, `cache.rs`):
  - `memory_budget` = memtables + block cache + reader metadata + side pools. Pool
    fractions rebalanced — block cache becomes the primary (~50 %); proportional
    `from_total_bytes` scaling preserved so the 512 MB edge tier and the 64 GB server tier
    stay one code path (final fractions tuned in BS4.6).
  - `require_table_reader_budget` charges **metadata-resident bytes** (footer + index +
    properties + filter frame lengths, all known pre-decode) instead of full object bytes;
    `max_open_readers` stays as a count backstop, default raised.
  - **Retire `would_exceed_total` on the durable path** (`bootstrap.rs:677-689`): no
    replacement failure mode — caches bound residency; the runtime total remains as an
    observability gauge with a health WARN if measured residency exceeds the budget
    (accounting-bug detector, not admission control). **Cache mode keeps its check** —
    its dataset genuinely is resident.
  - Memtable pressure paths (rotation, frozen backlog, throttle) untouched.

**Tests.** Open-time regression test (O(tables) I/O counts via perf-trace, not O(rows));
the previously-hard-failing dataset>budget bootstrap test now asserts success; cache-mode
rejection still passes; budget-fraction units; pressure paths unchanged-green.

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
  unmodified as a gate on 4.4c and 4.5.
- **C3 (profiles):** the pool rebalance preserves proportional `from_total_bytes` scaling —
  one code path for the 512 MB edge tier through the 64 GB server tier; validated at the
  explicit tier matrix (512 MB / 8 GB / 64 GB) including block-cache minimums (a tier must
  never produce a zero/degenerate cache while enabled). The budget model change (caches
  bound RAM, not dataset) is precisely what makes small-RAM/large-disk profiles (Raspberry
  Pi) possible.
- **C4 (branching):** inherited-layer validation is facts-based (4.4b §6), materialization
  precedence uses per-key probes with the branch-id prefix swap (4.4b §2), the pruning
  fingerprint hashes inherited layers identically (4.4b §1), and the cold-read suite's
  branching cases (above) gate reopen correctness. Fork remains O(1): snapshot/aggregate
  publication at fork is Arc-cloning, pinned by a fork-latency assert.

## Risks

| Risk | Mitigation |
|---|---|
| Hot-workload regression beyond 1.5× (block decode per hit vs decoded slices) | BS4.1 lands a RocksDB-grade cache first; per-slice scoreboard from 4c; reserves: cache decoded row-blocks (`Arc<[TableRow]>` entries with approximate charging), pin L0/index-adjacent blocks |
| Filter false-`DefinitelyAbsent` drops live rows | frame CRC + `matches_table` fingerprint gate + bit-exact goldens + round-trip property (every inserted key MaybePresent) + negative mutations |
| Compat ordering (old binaries hard-reject filtered tables) | reader (4.2) fully released before writer (4.3) emits; writer config-gated; mixed-store tests; spec documents the line |
| Pruning-proof semantic drift | in-memory-only proof + single shared fingerprint fn + sensitivity property suite; pinned tests updated deliberately in one slice |
| Snapshot-clone blowup under `Arc::make_mut` | Arc-wrapped metadata/filter in 4c; install-path allocation assertion in stress tests |
| Lazy I/O errors panic via `expect` wrappers | 4a fallibility audit + fault-injection read tests |
| OnceLock trap regression | DenyRuntime policy + cfg-gated API + counter + source-guard test |
| Recovery scans creep back to O(rows) | BS4.5 re-audit + open-time I/O-count regression test |

## Sequencing & PR discipline

BS4.1 ∥ BS4.2→4.3 ∥ BS4.4a, then 4.4b → 4.4c → 4.5 → 4.6. One PR per slice, `BS4.{n}`
titles, ≤1,500 LOC net each, standing gates every slice; format-touching slices (4.2/4.3,
the manifest extension in 4.4c) additionally gate on goldens + fuzz + crash sweeps. The
umbrella plan's "table-reader cache" step is amended: **deferred to the 1B tier** (G15),
with the sizing arithmetic recorded.

## Open items

- Whether `open_source` can carry the exact content digest for replacement verify
  (3) or the cursor-zip fallback is permanent — decide in 4.4b.
- Manifest put/tombstone fields vs Option-degrade — decide in 4.4c (default: add fields).
- Block-size tuning (64 KB blocks vs RocksDB's 4–64 KB) and decoded-row-block caching —
  measure in 4.6, feed BS6.
- Parallel manifest-replay opens for the 1 s gate — build only if the measurement demands.
