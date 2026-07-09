# Strata GPU Hot Tier — Design (2026-07-08)

**Status: UNLOCKED (GT0-GT5 landed, 2026-07-08).** All six milestones shipped
on the `gpu-hot-tier` branch and the §13 acceptance gates pass on the RTX
4070 Super:

- **Microbench** (64Ki resident slots, 16-dim summaries): top-k k=64 40.3 µs,
  +expand F=32 43.0 µs, materialize k=64×64KiB 10.5 µs / 398 GB/s — the full
  decode-path chain ≈ 54 µs against the 400 µs budget. Selection is exact
  hierarchical top-k (bitonic per-256 shortlists + shrinking merge rounds),
  bitwise-equal to the host-sim oracle including ties. Promotion pipeline
  1.55 GB/s; append (hot) mean 157 µs.
- **Decode driver** (Python, stock torch; 64Ki resident × 4KiB pages, 512Ki-page
  durable store = 8×; zipfian/window/graph-walk traces, 1536 steps): decode-path
  host overhead p50 52 µs / p95 69 µs; **zero** decode-path host syncs;
  cold fetch 32/32 promoted on demand; 3.9 pages/step promotion sustained.
- Maintenance (overlapped, off the decode path) is reported, not gated: at the
  full-scale store it is bound by engine read latency and a ~20× on-disk
  amplification under thousands of small batch commits — an **engine-side
  finding** for the storage-next scaling work, not tier machinery (the quick
  run's nominal store shows machinery-only rounds at ~1 ms).

Lithos can adopt via `materialize()` + DLPack with zero custom kernels; Moho
can start MO-2/MO-1 against this measured baseline.

This is the Strata-side design answering the requirements
recorded in `lithos/docs/strata-gpu-hot-tier.md` (HT-1..10, 2026-07-04) and the
open questions its §8 deferred. Companion requirement docs: `lithos/docs/moho.md`
(the kernel layer that consumes this tier), Lithos implementation plan §R1/R2.

**Scope discipline inherited from the requirements:** this is a tiered cache
over the existing store of record, not a port. T2 (Strata proper) is unchanged
— its durability semantics, formats, and commit paths are consumed, never
modified. Losing T0/T1 loses warmth, never data.

**The unlock criterion.** Strata's side is "done enough for Lithos/Moho to
proceed at their own pace" when: the five-call API works end-to-end with
baseline (correct, unfused) kernels; the tier passes its microbenchmark gates
on the RTX 4070 Super driven by a synthetic decode loop; and the Python/DLPack
seam is consumable from stock PyTorch with no custom kernels. Nothing in the
unlock depends on Lithos's models or Moho's kernels existing.

---

## 1. Decisions at a glance

| # | Deferred question (req. §8) | Decision | §
|---|---|---|---|
| D1 | Rust↔CUDA story | Extract strata-inference's dlopen driver-API pattern into the `device/` module; no cudarc/cust, no link-time CUDA dependency | 3 |
| D2 | Page size | Init-time constant per tier instance (not build-time); opaque fixed-size blob; content schema owned by the consumer via metadata tags | 4 |
| D3 | Index form | Deferred as required — but the *contract* is fixed now: per-page summary blobs in a device-resident parallel array, scoring kernel pluggable; baseline = mean-vector max-inner-product | 7 |
| D4 | HMM/unified memory vs explicit staging | Explicit staging (T2→T1 pinned→T0 arena). HMM on consumer parts is unpredictable under pressure; explicit staging is measurable and portable | 6 |
| D5 | How much T0 the index may consume | Fixed region ratios at init (default: pages 85%, summaries 8%, adjacency 5%, tables 2%); caps are hard | 5 |
| D6 | fp8 pages / attention numerics | Not the tier's decision: pages are opaque bytes. The tier guarantees alignment and size only. (Quantization is Moho/Lithos co-design.) | 4 |
| D7 | Eviction safety (HT-1 refcounting) | Epoch pinning + event-fenced slot reuse instead of per-page device refcounts — no host syncs, satisfies "never evicted under an in-flight step" | 5 |
| D8 | Adjacency form | Bounded-degree adjacency table (slot-major, fixed fan-out F) at v0, not general CSR; one-hop bounded expansion is the requirement, the table is the simplest structure that serves it | 8 |
| D9 | Where the code lives | ONE in-workspace non-default crate (`crates/gpu-cache`, package `strata-gpu-cache`) holding the whole GPU workstream — `device/` runtime + `tier/` semantics as modules; the workspace stays product-shaped | 2 |
| D10 | Testing without a GPU fleet | Backend trait with `cuda` and `host-sim` implementations; all machinery testable in CI on CPU, CUDA specifics tested on the dev 4070S | 11 |
| D11 | New (not in requirements) | **HT-11: COW page-table fork** — forkable working memory. Designed in from day 0 via page immutability; implemented after the core loop | 9 |

---

## 2. Crate architecture

```text
crates/gpu-cache      ONE crate (package strata-gpu-cache) for the whole GPU
                      workstream. Plainly descriptive on purpose: evocative
                      names (Lithos, Moho, Petra) belong to ecosystem
                      projects; strata-core crates say what they do. Moho is
                      the named boundary; this is Strata's GPU cache.
  src/device/         runtime: driver loading, context, streams, events,
                      arena + pinned pools, PTX JIT     (the only unsafe)
  src/tier/           page pool, page table, promotion/eviction, summaries,
                      adjacency, write-behind, API      (safe Rust)
                        └── consumes engine-next public surface
python: strata_tier   PyO3 binding over strata-gpu-cache: DLPack, numpy-free
```

Rules:

- The tier layer imports **engine-next's public (D4) surface only** — it is a
  consumer like the executor, never a storage-next importer. The one
  anticipated D4 addition is a batched page-read call sized for promotion
  (§6); until approved, `kv_batch_get` suffices.
- `#![deny(unsafe_code)]` at the crate root; only `src/device/` carries
  `#[allow(unsafe_code)]` — module-scoped isolation, the inference-next
  `local/` discipline (workspace rule 38). The tier layer is safe Rust.
- The crate is a **workspace member but not a default-member**; the tier
  machinery compiles and tests against `host-sim` (§11) with no GPU. Device
  tests are `#[ignore]`d and run explicitly on hardware.
- Naming: no `-next` suffix. These crates are not part of the V1 cutover
  train; they track engine-next's public surface and take the one-line rename
  when M9B sheds suffixes.
- Errors follow the V1 contract (`<class>.<area>.<detail>`): areas `gpu` and
  `tier` (e.g. `unavailable.gpu.driver_missing`,
  `resource_exhausted.gpu.arena`, `invalid_argument.tier.page_size_mismatch`,
  `failed_precondition.tier.geometry_mismatch`). Executor/CLI command surface
  is explicitly **post-v0**; the tier is a library API.

## 3. Device runtime (`device/`)

Extraction of the proven strata-inference pattern, generalized:

- **Driver loading:** `dlopen("libcuda.so.1")` at first use; every entry point
  resolved dynamically; absence of a driver is a clean
  `unavailable.gpu.driver_missing`, never a link failure. No CUDA toolkit at
  build time. Consumer-GPU floor: compute capability ≥ 8.0 checked at init.
- **Context/stream model:** one primary context per device; the tier owns
  three streams — `copy_in` (T1→T0), `copy_out` (T0→T1 write-behind), and
  `maintenance` (summary/adjacency updates) — and **never launches or
  synchronizes on the caller's compute stream**. All cross-stream ordering is
  via events.
- **Arena:** one `cuMemAlloc` of the configured T0 budget at init; monotonic
  region carve-up (D5), then a free-slot allocator inside the page region.
  After init the tier never calls the CUDA allocator again — it cannot
  fragment or steal VRAM from the hosting model (HT-7).
- **Pinned pools (T1):** `cuMemHostAlloc` slabs at init, ring-buffer staging
  for promotion and a spill region for warm pages, same never-allocate-again
  rule.
- **PTX modules:** baseline kernels embedded as PTX strings, JIT-compiled and
  cached per context (the strata-inference mechanism verbatim). ASCII-only
  PTX; `sm_80` floor.
- **Host functions:** completion plumbed via `cuLaunchHostFunc` on the copy
  streams (visibility flips, §6), keeping the host poll-free.

## 4. Pages

- A **page** is an opaque, immutable, fixed-size byte blob. `page_bytes` is an
  init-time constant of the tier instance (validated against the T2 manifest
  row on reopen — mismatch is `failed_precondition.tier.geometry_mismatch`).
- **Immutability is the load-bearing invariant.** KV state is append-only;
  a page, once appended, never changes. Consequences: T0/T1 are pure caches
  (no dirty-page tracking for promoted pages; only *newly appended* pages are
  write-behind dirty), eviction is always safe once fenced (§5), and fork
  (§9) is refcount bookkeeping, not copying.
- **Metadata tags:** each page carries up to 4 `u64` tags (e.g. layer,
  sequence id, position range) that the tier stores device-side but does not
  interpret. `topk_pages` filters are exact-match predicates over tags,
  evaluated in the selection kernel. Content layout inside the blob —
  K/V packing, heads, precision (D6) — is the consumer's schema.
- Expected geometry at 500M (illustrative, not baked in): per-layer pages of
  16–64 tokens; GQA at that scale gives ~0.5–1.5 KB/token/layer, so pages of
  16–96 KB. The design is insensitive to the choice; alignment is 256 B.

## 5. Page table, slots, and eviction safety

Device-side, everything is **slot-indexed parallel arrays** — kernels never
chase pointers:

```text
slot s: page blob        at pool_base + s * page_bytes      (implicit address)
        summary blob     at summ_base + s * summary_bytes
        tags             tags[s][4]         (u64 × 4)
        validity bit     valid[s]           (selectable by kernels)
        adjacency row    adj[s][F]          (§8)
```

The host owns the authoritative `page_id → slot` map and per-slot state
(id, score, last-selectable epoch, dirty flag). `page_id` is a monotone u64
assigned at append and is the stable T2 key; slots are transient placements.

**Eviction safety (D7) — epoch pinning, not refcounts.** The requirement is
that an in-flight attention step never has a page evicted under it. Device
refcounts would need host round-trips to manage. Instead:

1. The consumer's decode loop brackets steps with `step_begin(stream) → epoch`
   (host-async: records an event on the caller's stream and bumps the epoch).
2. Selection kernels only see slots with `valid[s] = 1`. Eviction first flips
   `valid[s] = 0` (a slot no new step can select), records the current
   epoch's event, and only **reuses** the slot after that event has completed
   — i.e., after every step that could have selected it has finished.
3. Slot reuse is therefore fenced by CUDA events, costs zero syncs, and the
   pinned window is one decode step — exactly the requirement, no more.

**Eviction policy** (host-side, between steps): score-and-edge-aware — a
slot's eviction priority is `f(recency of selection, selection score EMA,
resident-neighbor count)`, so pages whose graph neighbors are hot stay warm
(HT-4's edge-driven principle applied symmetrically). Policy is a pure host
function over host-side state: trivially testable.

## 6. Tiering machinery

```text
        promotion                            write-behind (append only)
T2 ──batched reads──▶ T1 pinned ring ──copy_in stream──▶ T0 arena
T2 ◀──batched engine commits── T1 spill ◀──copy_out stream── T0 (new pages)
```

- **Promotion.** A host-side promotion scheduler owns a priority queue fed by
  (a) explicit prefetch hints from `topk_pages` results (selected pages'
  neighbors — the edge-driven prefetch banked in R2), and (b) consumer hints
  (`prefetch(page_ids)`). Batches are read from T2 via the engine batch read
  path into the T1 ring, then copied T1→T0 on `copy_in`; a
  `cuLaunchHostFunc` completion flips `valid[s] = 1`. **A page becomes
  selectable only when its bytes are resident** — a miss shrinks retrieval
  breadth for that step; it never stalls the decode stream (HT-4).
- **Demotion.** Clean pages evict by dropping (T2 has them). T0-appended
  pages are dirty until their batched T2 commit completes; dirty pages are
  never slot-reused (the write-behind queue holds them in T1 spill if T0
  pressure demands the slot).
- **Write-behind (HT-6).** `append()` places the page in T0 (and its edges in
  host-side staging), enqueues a batched commit. Batches flow to T2 through
  the canonical engine commit path — one commit per batch, pages as rows,
  edges through the graph capability (§10). `flush()` drains the queue and
  returns the T2 commit receipt: that receipt *is* the durability point.
  Backpressure: if the dirty backlog exceeds its cap, `append` degrades per
  HT-7 (shrinks the clean pool first, then fails the append with
  `resource_exhausted.tier.write_backlog` — never silent loss).
- **Crash story:** T0/T1 vanish; T2 is exactly as durable as its last
  `flush()`/batch commit, by construction. v0 restarts cold. (A warm-start
  manifest — persist the hot set's ids at checkpoint — is a v1 nicety, noted
  and deferred.)

## 7. Summaries and baseline selection (HT-2)

- Every page carries a `summary_bytes` blob (init-time constant), co-promoted
  and co-resident in the summary region. Content is consumer-defined
  (Quest-style per-head min/max key bounds is the expected schema); the tier
  treats it as bytes.
- **Baseline scoring kernel** (until Moho replaces it): treats the summary as
  an f16 vector, computes inner product against the query, masks by validity
  and tag filters, then a single-block top-k select (k ≤ 64, resident pages
  ≤ 256k — microseconds at these sizes). Results (slot indices + scores) land
  in a device buffer owned by the returned `PageSet`. No host round-trip
  (HT-2 verbatim).
- The scoring function is a PTX module slot: Moho's fused Quest-bound kernel
  replaces the baseline by registration, not by forking the tier.

## 8. Hot adjacency and baseline expansion (HT-3)

- **Bounded-degree adjacency table** (D8): `adj[s][F]` slot-major, F fixed at
  init (default 32), entries are neighbor *slots* (resident neighbors only;
  non-resident neighbors live in the promotion queue instead — an edge to a
  cold page is a prefetch instruction, not an expansion target).
- Maintained incrementally: on promotion/append, the page's edge list (from
  T2 graph or `append(edges)`) is translated id→slot for resident endpoints
  and written into both endpoints' rows on the `maintenance` stream; on
  eviction, rows are lazily invalidated (slot generation counters guard
  stale entries).
- **Baseline expansion kernel:** one thread block per frontier page, gathers
  `adj[s][0..F]`, dedupes against the selected set via a slot bitmap, appends
  up to the caller's expansion budget. Bounded fan-out, bounded output,
  additive latency — the requirement's exact shape.

## 9. HT-11 (new): COW page-table fork

Because pages are immutable, forking working memory is metadata-only:

- `fork()` clones the host page table (id→slot map + per-slot state) into a
  new tier handle; device arrays are shared; per-slot residency is tracked
  with a table-count so eviction consults the union of handles.
- Semantics align 1:1 with Strata branches: the forked handle is scoped to a
  forked T2 branch (`branch_fork_current`), so durable history diverges with
  the working set — fork the branch, fork the model's memory.
- Payoffs recorded for the consumers: GRPO/RLVR rollouts share the prompt
  prefix's pages N-ways for free; tree-of-thought/speculative paths are
  branch operations with time travel and provenance.
- **Sequencing:** designed now (immutability + table-count are day-0
  decisions), implemented at v0.5 after the microbench gates pass. The API
  reserves the call.

## 10. T2 schema

One dedicated product space per tier instance (default `_tier/<name>`):

| Row | Key | Value |
|---|---|---|
| Manifest | `manifest` | geometry: page_bytes, summary_bytes, F, version |
| Page | `page/<page_id BE u64>` | the blob |
| Page meta | `meta/<page_id>` | tags, summary blob, edge list snapshot |
| Append log watermark | `watermark` | highest durably committed page_id |

Edges additionally land in the graph capability (graph name = tier name) so
they are queryable/auditable as first-class Strata edges — the provenance
story. Batched commits use existing engine batch APIs; the candidate D4
addition is a `page-read` batch call that avoids double-buffering blobs
through `Bytes` (measured first — it may not matter at promotion batch
sizes).

## 11. Backends and testing (D10)

The tier layer is written against a `DeviceBackend` trait implemented twice:

- **`cuda`** — real: the `device/` arena, streams, events, kernels.
- **`host-sim`** — plain host memory; "streams" are FIFO queues drained
  synchronously or step-wise under test control; "events" are counters;
  kernels are Rust functions with identical semantics.

Everything above the backend — page table, epoch fencing, promotion
scheduling, eviction policy, write-behind, backpressure, fork bookkeeping,
T2 schema, error paths — runs and is asserted in ordinary CI with no GPU.
Fault injection (copy failures, driver loss mid-run, backlog saturation) is a
host-sim test knob, mirroring storage-next's fault-injection discipline.
CUDA-specific correctness (stream ordering, event fencing under real
concurrency, DLPack lifetime, PTX kernels) runs on the 4070S dev box behind
`--features cuda`; those tests are part of the acceptance gate, not CI.

## 12. API (the Moho seam, concretized)

Rust (PyO3 mirrors 1:1; all calls host-async unless noted):

```rust
pub struct TierConfig {
    pub t0_bytes: u64, pub t1_bytes: u64,
    pub page_bytes: u32, pub summary_bytes: u32,
    pub adjacency_degree: u16,            // F
    pub region_ratios: RegionRatios,      // D5 defaults
}

impl Tier {
    /// Opens over an engine database handle; validates the T2 manifest.
    pub fn open(db: &Database, name: &str, cfg: TierConfig) -> Result<Tier>;

    /// Brackets one decode step; returns the pinning epoch. Host-async.
    pub fn step_begin(&self, stream: Stream) -> Epoch;

    /// Device-side selection + optional one-hop expansion. Results stay on
    /// device. Host-async; ordered on `stream`.
    pub fn topk_pages(&self, q: DeviceRef, k: u16, expand_hops: u8,
                      filter: Option<TagFilter>, stream: Stream) -> PageSet;

    /// Zero-copy: block-table tensor (slot indices) + pool view — the paged-
    /// attention convention Moho consumes. `materialize` instead copies the
    /// selected pages into one contiguous tensor so stock PyTorch attention
    /// works with no custom kernels (the Lithos-without-Moho path).
    pub fn gather(&self, pages: &PageSet, stream: Stream) -> BlockTable;
    pub fn materialize(&self, pages: &PageSet, stream: Stream) -> DeviceTensor;

    /// Immutable append; edges optional; write-behind to T2 (HT-6).
    pub fn append(&self, page: &[u8], summary: &[u8], tags: [u64; 4],
                  edges: &[PageId], stream: Stream) -> PageId;

    pub fn prefetch(&self, pages: &[PageId]);          // promotion hints
    pub fn flush(&self) -> Result<CommitReceipt>;      // durability point (sync)
    pub fn stats(&self) -> TierStats;                  // HT-9 counters
    pub fn fork(&self) -> Result<Tier>;                // HT-11, v0.5
}
```

DLPack: `PageSet`, `BlockTable`, and `DeviceTensor` implement
`__dlpack__`/`__dlpack_device__` (DLPack ≥ 1.0, versioned capsules,
stream-aware per protocol). The zero-implicit-sync rule (HT-5) is a tested
invariant: the harness asserts no `cuStreamSynchronize`/`cuCtxSynchronize`
occurs inside any decode-loop call (driver shim counts entry points — a
benefit of owning the dlopen layer).

## 13. Observability and the harness (HT-9, MO-7's Strata half)

`stats()` counters: per-tier hit/miss, promotion/demotion pages+bytes,
copy-stream occupancy, decode-stream stall time attributable to the tier
(event-measured), selection breadth achieved vs requested (the degradation
signal), write-behind backlog depth, recall proxy (baseline-vs-exhaustive
score overlap on a sampled step). Exposed through `stats()` and mirrored into
the engine metrics surface.

**Microbench suite** (Rust, event-timed, runs on the 4070S):
gather bandwidth vs page scatter; top-k latency vs k and resident-set size;
expansion latency vs fan-out; promotion throughput and overlap efficiency
(copy/compute concurrency); append→durable latency distribution.

**Synthetic decode driver** (Python, stock PyTorch): fakes a 500M-shaped
decode loop (no model weights needed) issuing `step_begin → topk → gather →
append` at realistic shapes, with workload traces: zipfian reuse, sliding
window, and graph-walk locality (the R2-shaped case). Gates, from the
requirements' §5 restated as numbers the harness enforces:

- tier overhead ≤ 20% of a 2 ms synthetic step budget (i.e. ≤ 400 µs) at
  k ≤ 64, F = 32, resident set ≥ 64k pages;
- effective context ≥ 8× the native-KV budget at that overhead under the
  graph-walk trace;
- zero decode-stream syncs (counted, not asserted by hope).

## 14. Milestones

| Milestone | Delivers | Exit gate |
|---|---|---|
| GT0 | Design accepted; device runtime extracted (arena, streams, events, PTX smoke on 4070S) | device smoke green |
| GT1 | Page pool, page table, epoch fencing, promotion/eviction on `host-sim` | CI-green machinery incl. fault injection |
| GT2 | T2 schema, write-behind, flush, backpressure | durability tests green (sim + real) |
| GT3 | Summaries, baseline top-k, adjacency, expansion on device | kernel correctness vs sim oracle |
| GT4 | DLPack/PyO3 seam, block-table + materialize paths | stock-PyTorch consumption demo |
| GT5 | Harness, budgets, counters; acceptance run | §13 gates pass on 4070S |

GT0–GT5 ≈ 6–10 weeks solo+AI. After GT5 the tier is **unlocked**: Lithos can
adopt via `materialize()` with zero custom kernels; Moho can start MO-2/MO-1
against a measured baseline whenever its own gate (R1.1) clears. v0.5 = fork
(§9). HT-v1 (consolidation hooks) and HT-v2 (trainable memory, MO-5) follow
the requirements' staging and are out of this document's scope — with one
design commitment kept: nothing above assumes pages are read-only forever
(the write path is append-plus-immutable, which trainable rows will relax by
mapping optimizer-state rows to their own pages).

## 15. Remaining cross-project questions (not blockers for GT0–GT5)

1. Page content schema + summary schema (Moho/Lithos own; tier is bytes).
2. Per-layer vs shared selection (Moho; affects only how many tier calls per
   step, not the tier).
3. fp8 page interaction with attention numerics (Moho/Lithos).
4. Whether R1's chunk-level retrieval shares this tier or starts on FAISS-GPU
   as planned (requirements say FAISS for R1.1; revisit at R1.2).
5. The MLA/quantized-KV baseline in the end-to-end harness (flagged during
   review: the tier must beat compressed-KV baselines, not just full-KV —
   belongs in the Lithos e2e harness spec).
