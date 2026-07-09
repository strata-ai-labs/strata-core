# Strata GPU Cache Tier — Moho Integration Guide

**Audience:** the Moho team (fused-kernel layer) and anyone consuming
`crates/gpu-cache` from a decode loop.
**Status:** current as of the GT5 unlock (2026-07-08) plus the kernel
registration seam. Companion design: `docs/design/gpu-hot-tier.md` — that
document says *why*; this one says *how*.

The tier is unlocked and measured on the RTX 4070 Super: selection+expansion
43 µs and materialization 10.5 µs at 64Ki resident pages (against a 400 µs
budget), zero decode-path host syncs, durable store 8× VRAM with on-demand
cold fetch. You consume it two ways, in increasing order of ambition:

1. **Stock consumption** — call `topk()`, take the materialized `[k,
   page_bytes]` tensor via DLPack, run stock attention. No custom kernels.
2. **Fused consumption** — take the block table + the raw page pool and
   gather inside your own attention kernel (the paged-attention pattern).
3. **Kernel replacement** — register your own PTX module for the selection
   pipeline itself (scoring, top-k, expansion, gather). The tier's
   machinery (residency, fencing, durability) is unchanged; only the math
   runs your code.

---

## 1. Mental model

The tier owns **device memory and residency**; Strata owns **durability**;
you own **page content, summaries, and attention math**.

Device memory is one arena, reserved once at open, never reallocated. It is
carved into fixed regions (all 256-byte aligned):

| Region | Layout | Writable by |
|---|---|---|
| Pages | `[slots][page_bytes]` opaque bytes | tier only (promotion/append) |
| Summaries | `[slots][dim]` f32, `dim = summary_bytes/4` | tier only |
| Adjacency | `[slots][degree]` u32 slot indices, `0xFFFFFFFF` = empty | tier only |
| Validity | `[slots]` u8, `1` = selectable | tier only |
| Tags | `[slots][4]` u64 metadata tags | tier only |
| Scratch | kernel workspace (scores, selection, candidates, bitmap, cursor, staged query) | selection kernels |
| Materialize | `[MAX_K][page_bytes]` gathered selection | `gather_pages` |

Hard rules the whole design rests on:

- **Pages are immutable.** Append creates a page; nothing ever rewrites one.
  Your kernels read pages and summaries; they never write them.
- **Epoch pinning replaces refcounts.** `step_begin()` fences the previous
  step; a slot selected in the current epoch cannot be evicted or reused
  until that epoch's fence completes. Consequence: device pointers into the
  pool are valid *for slots named by the current step's selection, during
  the current step*. Do not cache slot indices across steps.
- **Zero host syncs on the decode path.** The tier never calls a blocking
  driver wait during decode; completion is event-polled. Your integration
  must preserve this — the sync counter (`sync_calls()`) is how it is
  audited, not by hope.
- **Losing T0/T1 loses warmth, never data.** Every page is durable in
  Strata at the latest by `flush()`; the VRAM pool is a cache.

Ceilings (compiled into the scratch layout): `k ≤ 64` (`MAX_K`), expansion
budget ≤ 256 (`MAX_EXPAND`).

## 2. Build and install

No CUDA toolkit is required at build time — the driver is dlopen'd at
runtime and PTX is JIT-compiled (`sm_80` floor, i.e. Ampere or newer).

```bash
# Rust consumer
cargo build -p strata-gpu-cache --release

# Python consumer (PyO3 extension)
cargo build -p strata-gpu-cache --release --features python
mkdir -p /path/on/pythonpath
cp target/release/libstrata_gpu_cache.so /path/on/pythonpath/strata_tier.so
PYTHONPATH=/path/on/pythonpath python your_loop.py
```

Requires a CUDA-enabled torch in the venv for DLPack consumption (any
torch ≥ 2.x; tested against 2.12+cu130).

## 3. The decode loop

Python surface (`strata_tier`):

```python
import torch, strata_tier

tier = strata_tier.Tier(
    path="/path/to/db", space="kv-tier",
    page_bytes=4096,        # opaque; you own the content schema
    summary_bytes=64,       # f32 dim = summary_bytes / 4
    page_slots=65536,       # VRAM pool size
    adjacency_degree=32,    # fan-out F
    promotion_batch=8, write_behind_batch=256, write_backlog_cap=1024,
)

# Ingest (prefill / memory writes). Hot immediately, durable at the next
# batch commit or flush(). Activation is asynchronous: poll maintain().
pid = tier.append(page_bytes_blob, summary_f32_bytes,
                  tags=[a, b, c, d], edges=[pid1, pid2])
while not tier.is_selectable(pid):
    tier.step_begin(); tier.maintain()
tier.flush()                          # durability receipt (version, ts)

# Decode step:
tier.step_begin()                     # installs the epoch fence
tier.request(some_page_id, priority)  # returns True on a hit; misses queue
sel = tier.topk(query_f32_list, k=64, expand=128,
                filter_index=None, filter_value=None)   # host-async
pages = torch.from_dlpack(sel.pages())        # ordered via stream contract
# ... enqueue your model compute ...
tier.maintain()                       # promotions/commits; overlap it with
                                      # GPU compute, NOT on the critical path
```

Notes that bite people:

- `topk()` **enqueues** selection + materialization and returns immediately.
  `torch.from_dlpack` passes torch's current stream to `__dlpack__`, which
  makes that stream wait on the tier's fence — ordering is correct with no
  sync. `sel.ready()` / `tier.selection_ready()` are non-blocking probes.
- `maintain()` is where promotions, activations, evictions sweeps, and
  write-behind commits happen. Run it every step, positioned after your
  model compute is enqueued so its host time overlaps GPU execution. Cost
  is bounded by `promotion_batch`; at multi-GB stores it is currently
  bound by engine read latency (issue #2524).
- `append()` refuses with `resource_exhausted.tier.write_backlog` when the
  uncommitted backlog hits `write_backlog_cap` — call `maintain()`/`flush()`
  and retry. Do not treat it as fatal.
- Reopening a database with different geometry (page_bytes, summary_bytes,
  degree) refuses with `failed_precondition.tier.geometry_mismatch`.
- `selection_page_ids()` is a **documented host sync** — verification and
  debugging only, never in the decode loop.

## 4. Consuming selections

### 4.1 Materialized path (stock kernels)

`sel.pages()` → DLPack `uint8 [k, page_bytes]`, selection-ordered (best
first), pad rows zero-filled. Reshape/view into your KV layout and run
stock attention. Costs one device-side gather (measured 10.5 µs / 398 GB/s
for 64 × 64 KiB) and `k * page_bytes` of Materialize-region traffic.

### 4.2 Block-table path (fused kernels, zero copy)

Skip materialization entirely and gather inside your kernel:

```python
bt   = torch.from_dlpack(sel.block_table())   # int32 [k], -1 pads
pool = torch.from_dlpack(tier.pool())         # uint8 [slots, page_bytes]
out  = moho_fused_attention(q, pool, bt, scores=torch.from_dlpack(sel.scores()))
```

- `block_table()` is the paged-attention convention: physical slot indices,
  best-first, `-1` beyond the actual selection size.
- `tier.pool()` aliases the whole page pool. **Contents contract:** rows
  named by the *current* step's block table are stable for this step (epoch
  pinning); any other row may be overwritten by promotion at any time. Read
  only through the block table, only this step.
- Ordering is the same DLPack stream contract; if you launch on a stream
  torch doesn't manage, wait on the fence yourself (§4.3).
- Rust in-process equivalents: `CudaBackend::selection_addresses()`
  (`slots_ptr`, `scores_ptr`, `materialized_ptr`, `k`, `page_bytes`) and
  `CudaBackend::pool_address()` (`base`, `slots`, `page_bytes`), plus
  `unsafe wait_external_stream(raw_stream, fence)` with
  `tier.selection_fence()`.

Slot address math: `page_ptr = pool.base + slot * page_bytes` — the pool is
dense, no per-slot headers, 256-byte aligned base.

## 5. Running your kernels instead of the default

The selection module is replaceable **by registration, not by forking**
(design D3). The tier keeps its dispatcher, fences, scratch plan, and
machinery; you supply a PTX module that implements the same six entry
points with the same ABI.

### 5.1 How to register

```python
tier = strata_tier.Tier(..., selection_ptx=open("moho_selection.ptx").read())
```

```rust
let mut backend = CudaBackend::new(staging_bytes)?;
backend.register_selection_ptx(MOHO_SELECTION_PTX)?;   // before Tier::open
let tier = Tier::open(backend, store, config)?;
```

Registration is validated at open: the module is JIT-compiled and **every
entry point is resolved eagerly**. A missing or misnamed entry, non-ASCII
source, or JIT failure fails `open()` with a typed error — never the first
decode step. Start from the baseline: `strata_gpu_cache::
BASELINE_SELECTION_PTX` (source) and `SELECTION_KERNEL_NAMES` (the required
entry list).

### 5.2 The entry-point ABI

All kernels are launched on the tier's stream with `block = (256,1,1)` and
no dynamic shared memory. Pointer params are raw device addresses (u64).
Grids are chosen by the dispatcher — your kernels must be correct for the
stated grid, not assume a different decomposition.

**`score_slots`** — grid `(ceil(capacity/256), 1, 1)`; one thread per slot.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_scores` | u64 | out: f32 `[capacity]` |
| 2 | `p_summaries` | u64 | in: f32 `[capacity][dim]` |
| 3 | `p_valid` | u64 | in: u8 `[capacity]` |
| 4 | `p_tags` | u64 | in: u64 `[capacity][4]` |
| 5 | `p_query` | u64 | in: f32 `[dim]` (staged into scratch by the dispatcher) |
| 6 | `p_capacity` | u32 | slot count |
| 7 | `p_dim` | u32 | summary dimension |
| 8 | `p_findex` | u32 | tag filter index 0..=3, or `0xFFFFFFFF` = no filter |
| 9 | `p_fvalue` | u64 | tag value that must match when filtered |

Contract: `scores[slot] = your_score(query, summary[slot])` for slots with
`valid != 0` that pass the filter; **`f32::MIN` (bits `0xFF7FFFFF`) for
everything else**. `f32::MIN` is the mask sentinel the whole pipeline keys
on. The baseline scores by dot product; this kernel is where a learned or
fused scoring function goes.

**`block_topk`** — grid `(ceil(capacity/256), 1, 1)`; block b owns slots
`[b*256, b*256+256)`.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_scores` | u64 | in: f32 `[capacity]` |
| 2 | `p_capacity` | u32 | slot count |
| 3 | `p_k` | u32 | selection size (≤ 64) |
| 4 | `p_cand_scores` | u64 | out: f32, entry `b*k + i` |
| 5 | `p_cand_slots` | u64 | out: u32, entry `b*k + i` |

Contract: emit the block's exact top-k as `(score, slot)` pairs, best
first, ties toward the **lower slot**. Entries beyond the real candidates
carry `(f32::MIN, 0xFFFFFFFF)`.

**`merge_topk`** — launched in rounds; grid `(ceil(count/256), 1, 1)`,
block b owns candidates `[b*256, b*256+256)`. The dispatcher loops
(`count → ceil(count/256)*k`) until one block remains; that final launch's
outputs are the selection buffers.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_src_scores` | u64 | in: f32 `[count]` |
| 2 | `p_src_slots` | u64 | in: u32 `[count]` |
| 3 | `p_count` | u32 | live candidate entries |
| 4 | `p_k` | u32 | selection size |
| 5 | `p_out_slots` | u64 | out: u32, entry `b*k + i` |
| 6 | `p_out_scores` | u64 | out: f32, entry `b*k + i` |

Contract: same as `block_topk` over `(score, slot)` pairs. Exactness of the
hierarchy holds because any global top-k element is in its window's top-k —
your implementation must preserve that property (emit a *superset-safe*
exact window top-k, not an approximation).

**`seed_bitmap`** — grid `(1,1,1)`, block `(max(k,1),1,1)`; thread i marks
selected slot i.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_slots` | u64 | in: u32 `[k]` selection (`0xFFFFFFFF` pads) |
| 2 | `p_k` | u32 | selection size |
| 3 | `p_bitmap` | u64 | out: u32 bitmap `[ceil(capacity/32)]`, pre-zeroed |

Contract: set bit `slot` for each non-pad selected slot (bit `s%32` of word
`s/32`). The bitmap deduplicates expansion against the selection.

**`expand`** — grid `(ceil(k*degree/256), 1, 1)`; one thread per
(selection, edge) pair.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_slots` | u64 | in: u32 `[k]` selection |
| 2 | `p_k` | u32 | selection size |
| 3 | `p_adj` | u64 | in: u32 `[capacity][degree]`, `0xFFFFFFFF` = empty |
| 4 | `p_degree` | u32 | fan-out F |
| 5 | `p_valid` | u64 | in: u8 `[capacity]` |
| 6 | `p_bitmap` | u64 | in/out: dedup bitmap (selection pre-seeded) |
| 7 | `p_out` | u64 | out: u32 expanded slots, dense from 0 |
| 8 | `p_cursor` | u64 | in/out: u32 output cursor, pre-zeroed |
| 9 | `p_budget` | u32 | max expanded entries (≤ 256) |

Contract: one-hop walk — for each selected slot's each edge: skip empties,
skip invalid targets, atomically test-and-set the bitmap bit (dedup against
both the selection and other expansion threads), claim an output index via
`atom.add` on the cursor, drop writes at/beyond `budget`. Output order is
unspecified; the readback truncates at `min(cursor, budget)`.

**`gather_pages`** — grid `(ceil(k*page_bytes/4/256), 1, 1)`; one thread
per output u32 word.

| # | Param | Type | Meaning |
|---|---|---|---|
| 1 | `p_slots` | u64 | in: u32 `[k]` selection (`0xFFFFFFFF` pads) |
| 2 | `p_k` | u32 | selection size |
| 3 | `p_pool` | u64 | in: page pool base |
| 4 | `p_words` | u32 | `page_bytes / 4` |
| 5 | `p_out` | u64 | out: Materialize region, `[k][page_bytes]` |

Contract: row i of the output is the full page of selected slot i; pad rows
are zero-filled. (`Tier::open` enforces `page_bytes` as a nonzero multiple
of 256 and `summary_bytes` as a multiple of 4, so word addressing is safe.)

### 5.3 Semantic invariants (implementation-independent)

The oracle-equivalence suite enforces these; the machinery depends on them:

1. **Sentinels.** Masked/absent score = `f32::MIN` exactly (bit pattern
   `0xFF7FFFFF`); pad slot = `0xFFFFFFFF` (reads back as `-1` in the int32
   block table). The readback truncates the selection at the first pad —
   pads must be *trailing*.
2. **Exactness + determinism.** The selection is the exact top-k under
   (score desc, slot asc). The lower-slot tie rule is what makes runs
   reproducible and lets the host-sim oracle assert bitwise equality. If
   you change the tie rule you own updating the oracle to match — do not
   ship a nondeterministic selection.
3. **Never select masked.** A slot with validity 0 or a failed filter must
   be unselectable no matter its summary bytes. Validity is the eviction
   safety mechanism — violating this reads pages that may be mid-overwrite.
4. **Expansion never duplicates** a selected slot or another expanded slot,
   never emits invalid slots, never exceeds the budget.
5. **Write discipline.** Kernels write only their declared outputs (scratch
   buffers and the Materialize region). Pages, summaries, adjacency,
   validity, and tags are read-only from kernels — the host machinery is
   the only writer.
6. **No device-side sync assumptions.** Kernels run stream-ordered on the
   tier's stream; nothing may require host intervention mid-pipeline.

### 5.4 PTX constraints

- **ASCII only.** A single non-ASCII byte (smart quote, em dash in a
  comment) fails the JIT with `CUDA_ERROR_INVALID_PTX`; the tier validates
  and refuses at open with a clear error instead.
- Target `sm_80` (the HT-8 floor); the driver JIT specializes upward. If
  you ship arch-specific modules (e.g. `sm_89` with dp4a), select the
  source string yourself before registration — one module per tier open.
- No interior NUL bytes. Entry names must match `SELECTION_KERNEL_NAMES`
  exactly (all six, even if one of your kernels is trivial).
- v0 scope: the six-stage decomposition and grid shapes are fixed by the
  dispatcher. Fuse freely *within* a stage (e.g. a fused score that reads
  tags+summary once); cross-stage fusion (score+topk in one kernel) needs a
  dispatcher seam that doesn't exist yet — ask, it's a small addition.

### 5.5 Validating a replacement module

The tier ships the harness; point it at your module:

1. **Oracle equivalence** (correctness): `crates/gpu-cache/tests/
   tier_kernels.rs` runs the CUDA pipeline against the host-sim oracle on
   identical state with integer-valued summaries (exact f32 dots → bitwise
   comparison). Vendor the file, add `backend.register_selection_ptx(...)`
   after `CudaBackend::new`, run `-- --ignored` on a GPU box. All five
   tests (bitwise match, padding, expansion set, truncated expansion,
   materialization) must pass. If your scoring function differs from dot
   product, mirror it in `host_sim.rs`'s oracle first — the point is
   equivalence against *an* executable spec, not against dot products.
2. **Seam behavior**: `tests/tier_custom_kernels.rs` shows the fail-fast
   contract (missing entry, non-ASCII, late registration).
3. **Performance**: `cargo run -p strata-gpu-cache --release --example
   microbench` prints per-op latencies; swap the registration in
   `seeded_backend`. Budgets below.
4. **End to end**: `python crates/gpu-cache/python/decode_driver.py
   [--quick]` runs the full decode loop with acceptance gates
   (`selection_ptx=` plumbs through `Tier(...)`).

### 5.6 Budgets to stay inside (4070S baselines)

| Op | Baseline | Budget context |
|---|---|---|
| `topk` 64Ki slots, k=64 | 40 µs | select+expand+gather ≤ 400 µs (20% of a 2 ms step) |
| `topk+expand` 64Ki, k=64, F=32 | 43 µs | same envelope |
| `materialize` 64 × 64 KiB | 10.5 µs (398 GB/s) | bandwidth-bound; yours should hit the same roofline |
| decode-path host overhead | p95 69 µs | enqueue + DLPack wiring; unchanged by kernel choice |
| host syncs per step | 0 | hard gate, counted |

A replacement that is *slower* than baseline but semantically richer can
still be fine — the gate is the 400 µs envelope, not the baseline.

## 6. Gotchas (each one cost us a debugging session)

- **CUDA context:** the tier retains the device's *primary* context and
  rebinds it at entry points, so it coexists with torch in either init
  order. If Moho manages its own driver contexts, use the primary context
  too — a private `cuCtxCreate` context invalidates everyone's handles.
- **Tensor lifetimes:** DLPack tensors keep the tier alive (capsule
  keepalive). Tensors destroyed during interpreter shutdown are handled
  (GIL-free deleter); still, don't stash tier tensors in long-lived Python
  globals you never release.
- **Don't hold block tables across steps.** Slot indices are epoch-scoped.
  Page **ids** (u64, from `append`) are the stable names; re-`request` and
  re-select each step.
- **Activation is asynchronous.** After `append`/`request`, a page becomes
  selectable only after a `maintain()` round observes its copies complete.
  Poll `is_selectable`; never assume same-call visibility.
- **Backlog refusal is flow control**, not an error: `maintain()` then
  retry the append.
- **The store of record has scaling limits today** (#2524): thousands of
  small commits amplify on-disk size and slow promotion reads. Prefer
  larger `write_behind_batch` (256+) for ingest-heavy phases.

## 7. API quick reference

Python (`strata_tier`): `Tier(path, space, page_bytes, summary_bytes,
page_slots, adjacency_degree=32, promotion_batch=8, write_behind_batch=8,
write_backlog_cap=64, selection_ptx=None)`; methods `append(bytes, summary,
tags=None, edges=None) -> id`, `request(id, priority) -> bool`,
`maintain()`, `step_begin() -> epoch`, `flush() -> (version, ts) | None`,
`is_selectable(id)`, `topk(query, k, expand=None, filter_index=None,
filter_value=None) -> Selection`, `pool() -> DeviceTensor`,
`selection_ready()`, `selection_page_ids()` *(sync)*, `stats()`,
`sync_calls()`, `write_backlog()`. `Selection`: `block_table()`,
`scores()`, `pages()`, `ready()`.

Rust: `Tier<CudaBackend, EnginePageStore>` mirrors the above
(`topk_enqueue`/`materialize_enqueue`/`selection_fence`/`page_of_slot`);
`CudaBackend::{new, register_selection_ptx, selection_addresses,
pool_address, wait_external_stream, context}`; crate root exports
`BASELINE_SELECTION_PTX`, `SELECTION_KERNEL_NAMES`, `GpuError`.

Errors are typed and coded (`unavailable.gpu.driver_missing`,
`invalid_argument.gpu.config`, `resource_exhausted.gpu.arena`,
`resource_exhausted.tier.write_backlog`,
`failed_precondition.tier.geometry_mismatch`, `unavailable.tier.store`) —
match on codes, not message text.
