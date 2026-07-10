# Strata Vector Indexing — Design (new branch-aware LSM architecture)

**Purpose:** the indexing approach to build into the new architecture. Reference for implementation today; benchmark target after.

**Layer:** vector indexing is an **engine-next capability** layered over the storage substrate — `storage-next` stays semantics-free (see §2.5). Filed under `docs/architecture/engine/`.

---

## 0. The scale regime decision (read this first — it drives everything)

Strata is an **embedded database.** The engine *supports* billion-scale, but **no realistic embedded deployment holds a billion vectors** — the working range is **thousands to low tens of millions** per collection/branch. That single fact **reverses** the algorithm choice we'd make for a Pinecone-style billion-scale, object-storage, disaggregated system.

- **Billion-scale, object storage (Pinecone/Turbopuffer):** avoid graph indexes — compaction = re-index, and rebuilding a graph over billions is the write-amplification killer → use centroid/IVF/SPFresh.
- **Embedded, small-to-mid scale (Strata):** the data fits in RAM, slabs are bounded and few, and per-slab graph builds are cheap and amortized. **HNSW's "doesn't scale" objection doesn't apply, and its best-in-class recall/latency is exactly what an embedded DB should sell.** Plus — the key insight below — **the LSM model solves HNSW's one real operational weakness for free.**

**So the deliberate call: per-slab HNSW at the durable levels, progressive by layer, MVCC- and branch-aware — with a configurable IVF/centroid mode reserved for the scale-out ceiling.** Same reasoning as Pinecone (compaction = re-index), opposite conclusion (because the scale regime differs). *Making the opposite call on purpose, with the reasoning, is the Principal-grade move.*

### The insight that makes HNSW-on-LSM elegant
HNSW has two classic problems: (1) can't fit at huge scale — **irrelevant here**; (2) **deletes/updates degrade the graph** (tombstones accumulate, recall drops, you must periodically rebuild). **The LSM already periodically rebuilds (compaction) and drops tombstones.** So compaction produces a **clean HNSW graph over only live vectors every time** — the LSM *automatically* solves HNSW's delete-degradation problem. The "re-index tax" that's fatal at billion scale is, at embedded scale, *affordable and even beneficial* (it keeps graphs clean). HNSW + LSM is a better marriage than it first looks.

---

## 1. Goals / non-goals

**Goals**
- HNSW-class recall/latency at embedded scale (≤ ~10M vectors/collection).
- Native fit with the substrate: per-slab immutable indexes, compaction, MVCC visibility, branch isolation/COW, crash-safe durability.
- Progressive indexing by level (the thing the old architecture lacked).
- Cheap inserts (no per-insert global graph repair — the old architecture's pain).
- First-class filtered (metadata + vector) search using the existing per-slab roaring-bitmap indexes.
- Embedding/shadow-row lifecycle + model-version safety baked in.

**Non-goals (now)**
- Billion-scale optimization (supported via the IVF mode, not optimized as default).
- Distributed/multi-node serving.
- SPFresh-style in-place cluster rebalancing (the IVF-mode refinement, later).

---

## 2. Architecture overview: per-slab, progressive indexing

The vector index is **not one global structure** — it's a **per-slab derived artifact**, exactly like the slab's existing roaring-bitmap metadata index. This is what preserves immutability, MVCC, branch-COW, and crash safety. Index quality escalates with level:

| Tier | Contents | Index | Rationale |
|---|---|---|---|
| **Memtable** (mutable, RAM) | newest writes, tiny | **brute-force flat** (exact) | small set; no structure needed; inserts are free |
| **L0 slabs** (fresh, small, churny) | recent flushes | **flat, optionally scalar-quantized** | cheap to build on every flush; small N; thrown away on compaction |
| **L1…Ln slabs** (compacted, settled) | the bulk of data | **per-slab HNSW** (the quality tier) | built once per compaction; bounded per slab; clean graph over live vectors |

**Below-threshold shortcut (important for embedded):** if a collection/branch holds fewer than `BRUTE_FORCE_THRESHOLD` (default ~2,000–4,000) live vectors total, **skip HNSW entirely and brute-force** — it's exact, faster than building/traversing a tiny graph, and embedded collections are often this small. Only build HNSW once a slab crosses the threshold. (pgvector/sqlite-vec do exactly this.)

---

## 2.5 Who builds the index — the storage/engine boundary

The hard rule (from the architecture): **`storage-next` is semantics-free.** It must not know what a vector is, what HNSW is, or what "recall" means. So the vector index cannot be built *by* storage — even though it lives *in* the slab. Resolve it with one split:

> **storage = WHEN · engine = WHAT.**

- **Storage owns:** the slab lifecycle (flush, compaction, manifest commit, branch-COW, MVCC) and an **opaque per-slab side-artifact slot** — bytes it persists and ships with the slab but never interprets. (Storage already does *generic*, semantics-free indexing — sorted keys, roaring-bitmap metadata indexes over typed fields. The vector index is **not** that: ANN needs vector semantics, so it can't be a storage-built index. To storage it is an opaque blob.)
- **Engine owns:** the vector capability — the shadow-vector rows, the embedding model/metric/params, and **building/rebuilding the index.** It produces the index as opaque bytes and hands them to storage to persist in the slab's side-artifact slot.

**The mechanism: a flush/compaction derived-state hook.** When storage flushes or compacts, it emits a rebuild event — *"input slab set X → output slab Y, here are Y's rows"* — and the engine's vector capability:
1. takes the live rows for Y (already MVCC-resolved by compaction — tombstoned/superseded dropped),
2. builds the index (flat/SQ for L0, HNSW for L1+),
3. returns it as an opaque blob,
4. and storage **commits the blob atomically with the slab in the same manifest swap** — so a slab and its index are never out of sync, and recovery treats them as one unit.

**Why this is clean:**
- **No layering violation.** All ANN logic stays in the engine; the hook is generic ("derived-state rebuild on slab change") and reusable for any engine-owned per-slab derived structure.
- **Branch/MVCC for free.** The hook fires only on **owned-slab** compactions; **inherited** slabs keep their COW-shared index blob — the child never rebuilds them. Identical to how the substrate already treats owned vs. inherited slabs.
- **The index is pure derived state → never a durability risk.** The slab's vectors are the source of truth; the blob is reconstructible. If a blob is missing/corrupt on recovery, the engine **rebuilds it from the slab** — so the index can only ever cost a one-time rebuild, never data loss.

**What this means for building today** — do **not** put HNSW in `storage-next`. Build two pieces:
1. **In `storage-next`:** a flush/compaction **lifecycle hook** that (a) emits the input→output slab event with the output's rows and (b) accepts an opaque side-artifact blob to commit atomically with the slab. Generic, semantics-free.
2. **In `engine-next` (the vector capability):** a subscriber to that hook that owns build/rebuild + query, plus a recovery path that rebuilds a missing blob from the slab.

The flat-search MVP (build-order step 3) can start engine-side, reading rows through the storage API directly; the hook is what makes index **persistence + rebuild-on-compaction** layering-safe. Wire the hook early so HNSW (step 4) slots in without leaking semantics downward.

---

## 3. Write & compaction path

1. **Insert/update:** vector (a derived "shadow" row, see §7) → WAL → memtable. **No index work** beyond inserting into the flat memtable. Inserts stay cheap; this is the win over the old global-mutable-HNSW (which repaired the graph on every insert).
2. **Flush (memtable → L0 slab):** build the **cheap** index (flat or SQ) over the slab's vectors as part of the flush. Fast, no training.
3. **Compaction (→ L1+):** storage merges input slabs and drops tombstoned/superseded vectors (MVCC); the **engine** then rebuilds a fresh HNSW over the merged live set via the compaction hook (§2.5). Bounded per-slab cost, amortized. **The graph is always clean** (no accumulated deletes). Consolidation into fewer/larger bottom slabs **improves recall over time** (bigger graph ≈ closer to a global index) — the recall-by-age gradient, working *in your favor*.
4. **Branch-COW:** inherited slabs keep their HNSW; a branch never rebuilds inherited indexes (shared, immutable). Only owned-slab compactions rebuild. Branch isolation is inherited from the substrate's existing owned+inherited model.

---

## 4. Query path

```
query(vec q, k, read_bound, branch, filter?):
  candidates = []
  for each searchable slab in (memtable ∪ owned L0..Ln ∪ inherited slabs):
      if filter: prefilter_ids = slab.bitmap_index.match(filter)     # §6
      cand = slab_search(q, k·OVERFETCH, prefilter_ids)              # HNSW / flat / brute-force
      candidates += cand
  # unify across slabs + enforce correctness
  candidates = dedup_by_vector_id(candidates, keep newest visible)
  candidates = filter_mvcc_visibility(candidates, read_bound)         # §5
  candidates = filter_branch_fork_cap(candidates, branch)            # §5
  topk = rerank_full_precision(q, candidates)[:k]                    # exact distances
  return topk
```

Key points:
- **Over-fetch per slab** (`OVERFETCH` default 3–5×): merging top-k' from many small graphs under-recovers the global top-k unless you over-fetch, then rerank. This is how you recover recall across the fan-out.
- **Full-precision rerank** at the end: per-slab indexes may be quantized/approximate; the final ranking is computed on full vectors over the merged candidate set. (The two-stage retrieval pattern — candidate generation then exact rescore.)
- **Fan-out** = number of searchable slabs (≈ levels × slabs/level). Modest at embedded scale; bounded by compaction. This is the LSM read-amp tradeoff, applied to vectors.
- **MVCC & branch filters happen *after* candidate generation** (or via pre-filtered ID sets), so the index never returns a vector the reader shouldn't see.

---

## 5. MVCC & branch integration (the substrate gives most of this)

- **Versioning:** each vector is at a fixed `commit_version` in its (immutable) slab. The query's `read_bound` (`Latest` / `AtVersion` / `AtTimestamp`) determines visibility; a candidate whose version is newer than the bound, or that's superseded/tombstoned, is filtered out post-retrieval. Same model as KV.
- **Branch isolation:** the vector query iterates **the same owned + inherited slab set** that KV reads use, applying the **fork-version cap** to inherited slabs (never return inherited vectors newer than the fork point). So branch isolation is *free* — it's the substrate's existing behavior, the index just rides on the slabs.
- **Deletes:** tombstones; vectors physically leave the index at the next compaction that rebuilds the graph (which is also when the graph gets clean — see §0 insight).
- **Snapshots/time-travel:** because slabs are immutable and versioned, a vector search `AtVersion v` is consistent as of `v` — the substrate's MVCC gives you reproducible vector queries (and per-result provenance/version), which is exactly the citation-integrity property a Nexus-style layer would need.

---

## 6. Filtered (metadata + vector) search

Use the slab's **existing roaring-bitmap metadata indexes** to get a candidate ID set, then choose strategy by **selectivity** (the standard correct approach — HNSW pre-filtering can disconnect the graph, so don't naively traverse a filtered graph):

- **High selectivity** (filter keeps few rows): **brute-force the filtered subset** with full precision — fast because the subset is small, and exact. Skip HNSW.
- **Low selectivity** (filter keeps most rows): run HNSW, then **post-filter** the results (over-fetch more to absorb the loss).
- **Mid:** pre-filter to the bitmap ID set and restrict the slab search to it (allow brute-force fallback if the graph-restricted search under-returns).

This is a genuine differentiator: most embedded vector libs do filtering poorly; the substrate's per-slab bitmap indexes let you do selectivity-aware filtering correctly.

---

## 7. Embedding / shadow-vector lifecycle & model versioning

- **Vectors are derived ("shadow") rows** keyed to a source row — the engine owns them. The vector index is built over these shadow rows.
- **Auto-embedding (roadmap tie-in):** on write/change, the engine generates the embedding (or accepts a provided one). Re-embedding on change produces a **new shadow-row version** → re-indexed at the next compaction.
- **Model version is part of the index identity.** A query resolves against one embedding-model version; **mixing model versions in a single index is rejected** (`failed_precondition.embedding_model_mismatch`). A model upgrade is a **re-embed migration** = bulk re-derivation → re-index via compaction. (This is exactly the engine-owns-the-derived-row + mismatch-detection contract — build it in from day one; retrofitting it is painful.)

---

## 8. Distance metrics, quantization, tuning

- **Metrics:** cosine (store L2-normalized vectors → dot product), L2, inner product. Configurable per index.
- **Quantization:** optional scalar quantization for the graph payload (memory/speed), **full-precision vectors retained for rerank**. Default: full precision at embedded scale; SQ as a memory-pressure option.
- **HNSW params (defaults to start):** `M=16`, `efConstruction=200`, `efSearch` tunable per query (start 64–128); `OVERFETCH=4`. Expose all as config — the recall/latency dial.

---

## 9. The scale-out path (honest about billion-capability)

The same per-slab, progressive framework supports a **configurable IVF/centroid index type at the durable levels** instead of HNSW, for the rare large deployment that approaches the engine's billion ceiling. IVF merges/builds more cheaply at compaction (centroids + posting lists) and trades some recall — the right tradeoff *only* when re-index cost dominates (i.e., at scales an embedded DB won't hit). **Default = HNSW (embedded); IVF = opt-in (scale-out).** This keeps "one substrate, configurable capability" rather than two engines, and lets you say truthfully: *"embedded default is tuned for quality; the scale path exists and uses the Pinecone/Turbopuffer-style centroid approach for exactly the reasons we discussed."*

---

## 10. Build order for today

1. **Shadow-vector row + collection schema** (vector dim, metric, model-version in identity). §7
2. **Flat / brute-force search** over memtable + a slab (exact baseline; also the below-threshold path). §2
3. **Query merge + MVCC/branch visibility filter + full-precision rerank** across the slab set. §4–5 *(get correctness right before adding HNSW — flat results are the recall ground truth to validate HNSW against.)*
4. **Per-slab HNSW** build at compaction (engine-side, via the storage hook — §2.5) + search. §2–3
5. **Progressive wiring:** flat/SQ at L0, HNSW at L1+, brute-force-below-threshold. §2
6. **Filtered search** via roaring-bitmap pre-filter + selectivity strategy. §6
7. **Tuning knobs + metrics hooks** (recall@k harness comparing HNSW vs the flat ground truth). §8, §11

Step 3 is the unlock: once flat search + correct merge/MVCC/branch is working, you have **exact results to measure HNSW recall against**, and a shippable (if slow) vector capability. Everything after is quality/speed.

---

## 11. Benchmark hooks (for after the build)

- **V1 (the money chart):** **old global-mutable-HNSW insert/rebuild cost vs. new per-slab-HNSW-via-compaction.** Inserts: old (per-insert graph repair) vs new (cheap memtable/L0, batched build at compaction). → proves the incremental-write thesis on your own engine, old-vs-new.
- **V2:** recall@10 vs p99 latency, sweeping `efSearch`/`OVERFETCH`, on **SIFT1M** (so recall is comparable to published HNSW numbers); validate HNSW recall against the flat ground truth from step 3.
- **V3 (bonus, very on-thesis):** **recall improvement after compaction** — show recall rising as fresh small-graph data consolidates into larger bottom-level graphs (the recall-by-age gradient, working for you).
- **V4 (bonus):** filtered-query latency at varying selectivity (shows the bitmap-pre-filter strategy beating naive post-filter).
- Methodology: state hardware, dataset, dim, metric, params, warm/cold, and recall held constant. Run at 100K / 1M / (10M if feasible).

---

## 12. Open design decisions / risks

1. **Per-slab fan-out recall** — many small graphs under-recover the global top-k. Mitigated by over-fetch + rerank + consolidation; validate against flat ground truth (step 3). The main quality risk; measure early.
2. **Compaction cost vs query quality** — HNSW build at compaction is the cost you accept for clean graphs; watch it doesn't dominate compaction time at the upper embedded range (~10M). If it does, that's the signal to allow IVF at the bottom level. *(This is the exact tradeoff you'd discuss with Jeff — and you'd have the number.)*
3. **efSearch × fan-out latency** — per-slab efSearch multiplies across slabs; tune jointly, or search bottom (largest) slabs harder and upper slabs lighter.
4. **Memory** — full-precision + graph in RAM is fine at embedded scale; SQ is the pressure valve above a few million.
5. **Brute-force threshold** — tune so tiny collections never pay graph overhead.

---

### The one-paragraph narrative this design gives you
*"For the new architecture I'm building per-slab, progressive vector indexing on the branch-aware LSM. I deliberately chose HNSW at the durable levels rather than the centroid/IVF approach Pinecone and Turbopuffer use — because Strata is embedded: the data fits in RAM, slabs are bounded, and crucially the LSM's compaction solves HNSW's delete-degradation problem for free, handing me a clean graph over live vectors every cycle, with MVCC and branch isolation from the substrate. It's the same compaction-equals-re-index reasoning that pushes a billion-scale system toward IVF — applied to the opposite scale regime, so it lands on the opposite answer. And the IVF path is there, configurable, for the scale-out ceiling."*
