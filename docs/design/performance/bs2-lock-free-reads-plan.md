# BS2 — Lock-free reads: implementation and test plan

Status: **ready to implement after BS1**. Milestone BS2 of `billion-scale-plan.md` (gaps G4,
G5, G6). Executes M4P-L8I Group D with RocksDB SuperVersion semantics
(`rocksdb-parity-roadmap.md` RC1). Change class: intentional semantic change (read
concurrency); one deliberate behavior fix in a failure path (below). Assurance: S3.

## Problem (recap)

Every read serializes through the single runtime mutex (`api/runtime/background.rs:13`):
Latest point reads and scans run **entirely under it** (`api/runtime/mod.rs:1242/1362`);
versioned reads hold it while **deep-cloning the whole branch layout** — including a real
`Vec<u8>` byte-copy of every table's bloom filter (`read_hooks.rs:261`,
`table/reader.rs:113`). Reads block writes, writes block reads, and all of it contends with
the four maintenance workers. Scoreboard: run C @10 M = 85 K vs RocksDB 368 K (4.3×); reads
degrade further whenever compaction runs.

RocksDB's model: reads acquire a refcounted `SuperVersion` `{mem, imm, current}` with one
atomic op and never touch the DB mutex; installers publish a new one and old snapshots die
by refcount (`column_family.cc:1366-1485`).

## Design

### What the reconnaissance established (all verified, anchors in the survey)

1. **Freeze is already seal-in-place.** `MutableTable::freeze()` moves the *same*
   `Arc<TableMemoryState>` into `FrozenTable` with a sequence upper bound — zero row copies
   (`table/mutable.rs:282-289`). Rotation re-points slots, never moves bytes
   (`rotation.rs:38-40`). An old snapshot's memtable handle stays valid and complete across
   rotation — RocksDB's key invariant already holds.
2. **The read algorithm is source-parametric.** Latest (borrowed `&BranchLocalState`,
   `branch/read.rs:1192`) and versioned (owned `BranchReadView`, `:969`) both feed
   `select_ordered_visible_point_candidate` (`:1735`) and the shared scan entry (`:2966`)
   with the same four collections. A snapshot type carrying the `BranchReadView` field set
   (`:825-834`) plugs in with no algorithm change.
3. **Visibility is a single global monotonic `CommitVersion`** (`commit/visibility.rs:6`),
   written once per commit *after* apply (`commit/durable.rs:275` → `:293`; in `Always` mode
   the fsync happens inside the append, *before* apply — visibility is never deferred).
   **Latest reads currently apply no visibility bound** — the lookup supports a
   `max_commit_version` bound but Latest passes none (`read.rs:1748`); correctness today
   rests entirely on the mutex. The durable gate (`durable_gate.rs`) gates *writers* on
   apply/publish failures; it never affects the visible value.
4. **No off-lock branch handle exists** — the catalog is `Vec<LifecycleBranchEntry>` inside
   the mutex (`branch_lifecycle.rs:116`). The layout is already `Arc<BranchLayout>` with a
   documented ArcSwap endgame (`read.rs:658-668`).
5. **The bloom filter is `Box`, deep-copied on every owned-table clone**
   (`reader.rs:113`, payload `cache.rs:477`), and is immutable once built (all post-build
   access is read-only; "mutations" are whole-field replacement).

### The read protocol (the correctness core)

```text
V  = visible_version.load(Acquire)          // 1. bound FIRST
S  = branch_slot.snapshot.load()            // 2. ArcSwap load (refcount bump)
    read {S.active, S.frozen, S.layout, S.inherited} with max_commit_version ≤ V
```

Why this ordering is sufficient (the argument each reviewer should check):

- A commit with version `v ≤ V` finished `apply` before `V` was published
  (apply → release-publish order, `durable.rs:275→:293`). The acquire-load of `V`
  therefore happens-after those rows landed.
- The rows live either in the **shared** active memtable — which any current snapshot's
  `active` handle points at (commits do not republish the snapshot; the handle is the same
  `Arc`) — or, if rotation/flush/compaction moved them, in a frozen/L0/Ln table of a
  snapshot published under the lock *before* our load. Loading `S` **after** `V` guarantees
  `S` is at least as new as any structural move that happened before `V` was published.
  Either way every row with `commit_version ≤ V` is reachable through `S`.
- Rows with `commit_version > V` (a commit mid-apply on another thread, or a torn batch —
  batch inserts take the memtable write lock per row) are **filtered by the bound**. All
  rows of a batch share one commit version, so a reader can never observe a partial batch:
  atomicity comes from the bound, not the mutex.

The snapshot holds the **live (unpinned)** active handle; each versioned read that needs a
stable view pins at read time via `clone_for_read_view` (cheap, `mutable.rs:109`). Old
snapshots and their tables die by `Arc` drop — RocksDB's `Version::Unref` for free.

### One deliberate behavior change (failure path)

Today, if `publish_visible` fails after a successful apply (`applied_not_visible`,
`durable.rs:298`), under-lock readers **can** serve the applied rows (no bound). With the
visible bound they cannot — which matches the gate's intent (the commit is not
acknowledged; future commits are blocked pending reconciliation). This is a fix, not a
regression; it gets an explicit test and a note in the PR description.

### Decisions

- **D1 — visible version stays global** (one `AtomicU64` beside the tracker), matching the
  "global for V1" commit-validation comments (`durable.rs:216`). Per-branch atomics are a
  BS2.5 refinement if profiling demands.
- **D2 — plain `ArcSwap::load_full`**, no thread-local sentinel cache (RocksDB's
  `kSVInUse` dance) in the first cut. One atomic refcount bump per read is already ~free;
  add the cache only if BS2.4 profiling shows the cache-line bounce.
- **D3 — publication piggybacks on the BS1 event hooks.** The republication points are
  exactly BS1's shape events (rotation, flush replace, compaction/promotion install,
  materialization, branch lifecycle, recovery) — the same `refresh_observed_row_facts` hook
  plus the two delta paths. BS1 must land first; BS2 adds `publish_snapshot()` at the end of
  the same hooks.
- **D4 — dark launch with an equivalence oracle.** The snapshot infrastructure lands while
  reads stay locked; a debug-mode oracle compares snapshot-path results against locked-path
  results before cutover (mirrors BS1's aggregate oracle).

## Slices

### BS2.1 — Arc-shared bloom filters

**Changes.** `TableReaderFilterState::Bloom(Box<…>)` → `Arc<…>` (`reader.rs:113`); the two
constructors and match arms; no read-side changes (deref-identical). Kills the bloom
byte-copy in `capture_read_view` (`read_hooks.rs:261`), in every `Arc::make_mut` layout
install, and in `clone_active_for_fork`.

**Tests.** Existing filter tests unchanged; one new assert that cloning an owned table does
not duplicate filter memory (Arc pointer equality). Perf note: versioned-read latency drop
measured but not gated.

### BS2.2 — Explicit visibility bound (landed dark, under the lock)

**Changes.**
1. Publish the visible version to an `AtomicU64` (release) at the existing
   `publish_visible` call (`durable.rs:293`); cache-mode mirror.
2. Latest point + latest scan paths thread `max_commit_version = visible` into the lookup
   (`read.rs:1748` — the bound parameter already exists) and scan bounds.
3. Debug assert while still under the lock: `visible == max applied commit version` except
   inside the apply→publish window — i.e., the bound is a no-op under the mutex. This
   proves semantic equivalence before any concurrency changes.

**Tests.** (a) Equivalence: full suite in debug with the assert armed — Latest results
identical with and without the bound. (b) The failure-path fix: force `applied_not_visible`
(fault hook), assert Latest does **not** serve the unpublished commit and the gate blocks
the next commit (error class/code asserts). (c) Multi-row batch atomicity test at the
`BranchLocalState` level: insert a batch, bound at pre-batch V → zero rows visible; at
post-publish V → all rows.

### BS2.3 — Published snapshot + off-lock registry (still dark)

**Changes.**
1. `BranchSnapshot { active: MutableTable, frozen: Vec<FrozenTable>, layout:
   Arc<BranchLayout>, inherited_layers: Vec<BranchInheritedLayer>, facts: BranchStateFacts,
   timestamp_coverage }` — the `BranchReadView` field set, `Arc`-owned.
2. `BranchReadSlot { snapshot: ArcSwap<Arc<BranchSnapshot>> }`; `publish_snapshot()` called
   from the BS1 event hooks + branch construction; slot stored in the catalog entry.
3. Off-lock registry on `RuntimeSlot`: `ArcSwap<HashMap<BranchId, Arc<BranchReadSlot>>>`
   maintained at branch create/fork/clear/delete (rare events; full-map republish is fine).
   Post-delete lookups fail with the existing not-found error; a racing read completes on
   the final snapshot (RocksDB drop-CF semantics).
4. **Equivalence oracle (debug):** locked read paths additionally execute the snapshot path
   and assert identical results (point, scan, history) — armed across the whole suite.

**Tests.** Per-event snapshot-content tests (after each of the Part-A mutation classes, the
published snapshot equals the locked state); registry lifecycle tests (create/fork/clear/
delete visibility); oracle armed across the full suite + recovery oracle + fault sweep.

### BS2.4 — Cutover + concurrency stress

**Changes.** The four read verbs (`read_point` Latest + versioned, `read_history`,
`scan_prefix`, `scan_range`) switch to: registry lookup → protocol read (V, then S, bounded)
— **no `slot.lock()`**. Locked read plumbing deleted (`read_latest_point_or_tombstone_for_branch`,
`read_view_for_branch`'s lock-and-clone body; `capture_read_view` remains for the commit
conflict path, now cheap post-BS2.1). Diagnostics and maintenance keep the lock (out of
scope).

**Tests — the S3 gate for this milestone.**
- **Concurrency stress** (new integration test, real `ThreadedMaintenanceExecutor`): N
  reader threads loop point/scan/versioned reads while a writer commits batches and
  maintenance flushes/compacts. Invariants asserted per read: (i) monotonic visible version
  per reader thread; (ii) batch atomicity — batches write a checksum row; a reader must see
  all-or-none of each batch; (iii) writer read-your-writes after commit ack; (iv) no panics/
  errors. Run long enough to cross many rotations + compaction installs.
  **Branching invariants (constraint C4), named and asserted in the same stress:**
  (v) fork-during-load — a fork taken mid-stress is O(1) (latency pinned) and the child
  serves correct reads through its inherited layers immediately; (vi) parent and child read
  concurrently without interference; (vii) a materialization completing mid-stress
  (inherited layer → owned tables) never produces a torn or incorrect child read.
- **Snapshot lifetime:** hold a loaded snapshot across ≥2 compaction installs; complete the
  read correctly; assert the old tables are freed after drop (`Arc::strong_count` probes).
- **Interleaving determinism tests:** scripted sequences (apply→read-before-publish→publish;
  rotate-between-V-and-S; flush-install mid-scan) exercised single-threaded via direct
  calls, asserting the protocol's guarantees at each step.
- Full suite + recovery oracle + fault sweep (writes untouched, but armed oracles from
  BS2.2/2.3 remain in debug).

### BS2.5 — Measure-first refinements

Profile after cutover; build only what the data demands: thread-local snapshot caching
(RocksDB sentinel dance), per-branch visible atomics, scan-path allocation trims (the
per-key heap clones from the read-path survey — G6). Each gets its own mini-A/B.

## Perf validation (exit criteria)

Control = BS1-final binary; treatment = BS2.4. Standard methodology.

1. **Primary (gate):** run C @10 M ≥ **3×** control (85 K → ≥250 K ops/s target band).
2. **Primary (gate):** reads-under-compaction — run C measured while a background load
   drives continuous flush/compaction: throughput must not collapse (≥70 % of quiescent).
3. **Secondary:** run B, D, E improve; run A improves via read-side relief (the write side
   is BS3's).
4. **No-regression:** load cells within noise (commit path untouched except the atomic
   store); versioned-read latency improved (BS2.1) — measured.
5. Ledger row per slice; scoreboard re-run at milestone close.

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** `arc-swap` is pure atomics — wasm-safe. BS2 adds **no threads** (readers
  are caller threads; publication rides existing install paths). The wasm check-build gate
  applies.
- **C2 (cache mode):** same snapshot machinery through the shared code path (open item
  resolved above); cache-mode semantics, budget model, and suites unchanged.
- **C3 (profiles):** no budget interaction (snapshots are refcounted handles, not copies).
- **C4 (branching):** `BranchSnapshot` carries `inherited_layers`; the registry updates at
  fork/clear/delete; the stress suite's named branching invariants (v)–(vii) gate the
  milestone; fork stays O(1) (snapshot publication at fork is an Arc-clone, pinned by the
  fork-latency assert).

## Risks

| Risk | Mitigation |
|---|---|
| Ordering bug in the read protocol (torn batch / missing rows) | the protocol argument is documented above and enforced by the interleaving tests + batch-atomicity stress invariant; V-before-S ordering is a single code path |
| Stale snapshot after a missed publication point | publication rides the BS1 hooks (universal by construction) + the BS2.3 equivalence oracle across the whole suite |
| Failure-path behavior change (`applied_not_visible`) | deliberate; dedicated test; called out in the PR |
| Long-lived snapshots pin old tables (memory) | measured in stress tests; same trade RocksDB makes; snapshots are per-read, not per-session |
| Registry vs catalog divergence (branch lifecycle races) | registry mutations only at catalog mutation sites under the lock; lifecycle tests |
| `ArcSwap` dependency (new crate) | `arc-swap` is small, widely used, no-unsafe-in-API; add to workspace with the standard vetting; fallback is `RwLock<Arc<…>>` (slower, same semantics) |

## Sequencing & PR discipline

Depends on **BS1 complete** (event hooks + O(1) aggregates). Slices land in order
BS2.1 → 2.2 → 2.3 → 2.4 → (2.5 measure-first), one PR each, `BS2.{n}` titles, ≤1 500 LOC
net, all standing gates per slice (debug suite with oracles armed + release + clippy + fmt).
The dark-launch structure means 2.1–2.3 are individually shippable with zero behavior change
(except the flagged failure-path fix in 2.2); 2.4 is the single cutover PR.

## Open items

- Whether `capture_read_view`'s remaining consumer (commit conflict validation,
  `commit/durable.rs:91`) should also move to snapshots — decide during BS2.3 (it runs
  under the lock already; post-BS2.1 its cost is small).
- ~~Cache-mode runtime: same snapshot machinery or keep locked reads~~ **Resolved by
  constraint C2 (umbrella §2b): cache mode gets the same snapshot machinery via the shared
  code path** — no separate read implementation; its semantics and budget model are
  unchanged, and the cache-mode suites run unchanged as a gate.
- Scan-path allocation work (G6 remainder) — deferred to BS2.5 with profile data.
