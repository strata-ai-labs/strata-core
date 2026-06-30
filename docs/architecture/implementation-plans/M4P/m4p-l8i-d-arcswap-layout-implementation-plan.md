# M4P-L8I Group D: ArcSwap layout + atomic visible-version — implementation plan

Status: draft (first detailed slice of M4P-L8I).
Parent: `docs/architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md` (Group D).
Root cause: `docs/design/performance/durable-background-lock-convoy.md`.
Reference design: old engine `crates/storage/src/segmented/mod.rs` (`SegmentVersion` + `ArcSwap`).

## Why this is the first group

The measured convoy is **worker-side**: at 10M the maintenance workers (and the
foreground readers in the `flush_watermark` coverage scan) collapse onto the runtime
mutex while reading/scanning the per-branch level layout (`owned_levels`). Group D
makes those **reads lock-free**, which:
- directly removes the worker-side contention the re-pin found (the convoy holder);
- **structurally subsumes Fix B** — the O(rows) coverage scan stops needing the lock;
- implements the immutable-`ArcSwap`-snapshot design **L6 already specifies**
  (`l6-branch-isolated-lsm-runtime.md` "the current `BranchSnapshot`/`SegmentVersion`
  shape is the right evidence") but V1 skipped (it ships `owned_levels: Vec<Vec<…>>`
  mutated in place under the mutex, `branch/state.rs:71`).

Important sequencing note: Group D delivers the lock-free **read** win on its own — the
layout *install* can stay under a brief runtime-lock store for now; moving the install
fully off-lock composes later with Group C's per-branch publish guard. So D is
independently shippable and is the shortest path to the convoy fix.

## Target end state

`BranchLocalState` stops owning a mutable `owned_levels` vector. Instead it owns an
immutable, reference-counted **layout snapshot** behind `ArcSwap`:

```text
BranchLocalState {
    layout: ArcSwap<BranchLayout>,   // replaces owned_levels + its layout-derived facts
    active, frozen, inherited_layers, compact_pointers, … (unchanged)
    branch-total facts that include active/frozen (max_commit_version, …) stay here
}
BranchLayout {                       // immutable; rebuilt + stored on every install
    owned_levels: Vec<Vec<BranchOwnedTable>>,   // tables already Arc-backed
    // layout-derived read-planning facts, folded in so a reader sees them atomically:
    timestamp_coverage, observed_rows, per-table key ranges, …
}
```
Reads `layout.load()` (lock-free `Arc`); flush/compaction/materialization installs build
a new `BranchLayout` and `store` it. `VisibleVersionTracker` becomes atomic.

## Invariants this group MUST preserve (guardrails)

From the L6/L7/L8/L4 contract review (citations in those docs):
1. **Atomic layout + facts.** A lock-free reader must never see layout `vN` with facts
   `v(N−1)`. The folded facts live in the *same* `Arc` and are recomputed on rebuild.
   (L8I Group D; stop-condition 4.)
2. **No partial branch view.** Every install is a single `ArcSwap::store` — readers see
   old-or-new, never a mix; frozen stays visible until its flushed table is in the new
   layout. (`l6:260-276, 316-326`.)
3. **Read-source precedence + MVCC ordering unchanged:** active → frozen(newest) →
   L0(newest) → L1+ → inherited; newest-commit-first row chains. (`l6:382-392`.)
4. **Fork COW sharing intact.** `fork.rs:98` currently clones `owned_levels`; it becomes
   an `Arc` clone of the parent layout. A child's inherited `Arc<…>` must stay immutable
   across the parent's later stores; the fork-version gate still hides post-fork rows.
   (`l6:416-439`.)
5. **Clear/delete vs flush/compaction race.** A racing clear/delete must not be undone by
   an in-flight install (no resurrection). Highest-risk item; covered by the concurrency
   oracle. (`l6:324`.)
6. **Crash-safe ordering is Group C's job, unchanged here.** D is an in-memory swap; the
   manifest persist + old-file reclaim ordering (swap→persist→reclaim) stays exactly as
   today. D must not reorder install relative to the manifest record. (`l4:593-619`.)
7. **Visible-version monotonicity.** Atomic publish never regresses; published only after
   L6 apply; reads/checkpoints load it. (`commit/visibility.rs:35-46`.)

## Slices

### D.1 — Introduce the immutable `BranchLayout` snapshot (behavior-preserving refactor)

The de-risking step: bundle the layout + its derived read-planning facts into a
`BranchLayout` value and route **all** reads and mutations through it, **without** any
concurrency change (`BranchLocalState` holds a plain `BranchLayout`, still mutated under
the runtime lock). No observable behavior change; the full suite must pass unchanged.

- Define `BranchLayout` (new, in `branch/state.rs` or `branch/read.rs`): `owned_levels`
  plus the facts that are **derived from `owned_levels`** (decide the exact set in this
  slice — `timestamp_coverage` and `observe_rows_from_summaries` output are layout-derived;
  branch-total facts that also count `active`/`frozen` — e.g. `max_commit_version`,
  `put_rows`/`tombstone_rows` — stay on `BranchLocalState`). Provide a constructor that
  computes the folded facts from the levels, so "rebuild = recompute facts" is enforced
  in one place.
- Replace the `owned_levels` field with `layout: BranchLayout`; update the install sites
  (`state.rs:208/211/238`, `materialization.rs:648`, `state/compaction.rs`) to **build a
  new `BranchLayout` and assign** rather than `insert`/`push` in place. Keep them under
  the runtime lock (no `ArcSwap` yet).
- Update read accessors (`owned_levels()` at `read_hooks.rs:82` / `read.rs:735`, the
  internal `self.owned_levels` reads at `state.rs:129/320/424/443`, `snapshot.rs`,
  `materialization.rs`, `pruning.rs`) to read through `layout`.
- `fork.rs:98` clones the layout value (still a deep clone here; becomes `Arc` clone in D.2).
- **Exit gate:** full `cargo test -p strata-storage-next` + format goldens pass unchanged;
  no public-surface change; the install sites all rebuild-then-assign (no in-place mutation
  of the layout remains). This slice is pure structure.

### D.2 — `ArcSwap<BranchLayout>` + lock-free reads (the keystone)

- `BranchLocalState.layout: BranchLayout` → `layout: ArcSwap<Arc<BranchLayout>>`
  (tables are `Arc`-backed; the swap is one pointer store). Installs become
  `self.layout.store(Arc::new(new_layout))` (built under the existing brief runtime lock).
- Reads: `layout.load_full()` returns an owning `Arc<BranchLayout>`; the `owned_levels()`
  borrow tied to `&self` is replaced by an owned `Arc` snapshot. **Lower-risk routing
  (L8I's recommendation):** push the change through the existing `capture_read_view`
  (already clones into a read view) and the maintenance coverage/scoring paths, so callers
  hold an `Arc` snapshot rather than a `&self` borrow — minimizing the borrow-lifetime
  churn. Convert the `flush_watermark` coverage scan + compaction scoring to read the
  loaded `Arc` (this is where the convoy contention disappears).
- Derived `Clone`/`Eq` on `BranchLocalState` break (`ArcSwap` is neither). Add manual
  `Clone` (load_full + new ArcSwap) and `PartialEq`/`Eq` (compare loaded layouts) impls.
- Same-branch installs stay serialized by the existing runtime lock during this slice
  (the brief store is under the lock), so no concurrent-install reconciliation is needed
  yet; the old engine's `Arc::ptr_eq` reconciliation becomes relevant only when the store
  moves off-lock (deferred, composes with Group C).
- **Exit gate:** point/scan reads and the maintenance coverage/scoring reads take **no
  runtime lock**; layout + folded facts observed atomically (test below); reads correct
  under concurrent install (concurrency oracle); convoy A/B shows the worker-side stall
  gone on the read path.

### D.3 — Atomic visible-version

- `VisibleVersionTracker` (`commit/visibility.rs`): back `visible_version` with an
  `AtomicU64` (CommitVersion is a `u64` newtype). `publish_visible` becomes a monotonic
  CAS (reject regress, no-op on equal, advance on greater — same three outcomes);
  `visible_version()` and `catch_up_visible_after_replay` load atomically. Commit still
  publishes after L6 apply; reads/checkpoints load without the runtime lock.
- **Exit gate:** visible-version reads take no runtime lock; monotonicity preserved
  (existing `visibility.rs` tests pass, made concurrent); no torn/regressed visibility.

Order: D.1 → D.2 → D.3. D.1 and D.3 are low-risk; D.2 is the keystone and the risk
concentrates there (Clone/Eq, read-borrow plumbing, atomic-publish read correctness).
Each slice ≤ ~1,500 LOC.

## Verification

- **Correctness gate (per slice):** full `cargo test -p strata-storage-next`, the
  **recovery/fault oracle**, and **format goldens** — D changes no on-disk bytes.
- **Layout-consistency test (D.2):** under concurrent install + read, a loaded `Arc`
  snapshot's `owned_levels` and its folded facts are always mutually consistent (never
  layout `vN` + facts `v(N−1)`); read results equal the locked baseline. (L8I test matrix
  "Layout consistency (D)".)
- **Concurrency oracle:** randomized commit + flush + compaction + materialization + read
  + clear/delete interleavings recover to a layout identical to the synchronous baseline;
  fork COW sharing + fork-version gate hold under concurrent parent installs.
- **Convoy signal:** the interleaved control-vs-fixed **crawl-rate A/B** and **workload-F
  run-phase throughput at 10M** (the L8I test-plan closeout) — D.2 should move the
  worker-side convoy; full kill may need Group C (off-lock install) too.

## Out of scope / dependencies

- **Off-lock install** (store without the runtime lock) is deferred — it composes with
  **Group C**'s per-branch publish guard + `Arc::ptr_eq` reconciliation of concurrent
  same-branch installs. D ships the lock-free *read* win first.
- No durable format/codec/manifest/WAL change; no semantics change; one canonical read
  path (no second layout representation left behind after D.1).
- Lock-free memtable (`active`) and per-branch sharding are separate L8I work (the
  old roadmap's D3/D4), not this group.
