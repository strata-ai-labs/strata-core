# M4P-L8I Group D: ArcSwap layout + atomic visible-version — test plan

Status: draft.
Implementation plan: `docs/architecture/implementation-plans/M4P/m4p-l8i-d-arcswap-layout-implementation-plan.md`.

## Goal

Prove that moving the per-branch level layout to an immutable `ArcSwap<Arc<BranchLayout>>`
snapshot and the visible-version to an atomic — so reads/scoring/coverage take **no runtime
lock** — does **not** weaken read correctness, MVCC/visibility semantics, fork COW sharing,
recovery, the frozen durable format, or cache-mode behavior. Correctness is the gate;
the convoy crawl-rate / workload-F numbers are the benchmark target.

The suite must **fail** if any change:

1. lets a lock-free reader observe a layout (`owned_levels`) inconsistent with the folded
   facts it carries (layout `vN` + facts `v(N−1)`);
2. lets a reader observe a *partial* install (a mix of old and new levels);
3. changes any read result (latest / getv / history / as_of / scan / reachability /
   timestamp coverage) versus the locked baseline for the same state;
4. regresses or tears the visible version, or changes its three publish outcomes
   (advance / unchanged / reject-regress);
5. breaks fork COW sharing or the fork-version visibility gate under a concurrent parent
   install;
6. resurrects a cleared/deleted branch via a racing in-flight install;
7. recovers to a state differing from the fully-synchronous baseline for the same write
   history;
8. changes any on-disk byte / golden vector, or regresses cache-mode (L8G) behavior;
9. makes the deterministic-inline path nondeterministic or introduces a wall-clock
   dependency in the drive path.

## Test matrix

| Area (slice) | Required proof | Failure caught |
| --- | --- | --- |
| Refactor equivalence (D.1) | Full existing suite + goldens pass unchanged; `BranchLayout` folded facts (`timestamp_coverage`, `observed_rows`) equal the prior per-field computation for the same level set. | Refactor changed behavior or fact computation. |
| Fact-split correctness (D.1) | For a branch with active + frozen + owned rows: layout-folded facts reflect **only owned levels**; branch-total facts (`max_commit_version`, `put_rows`/`tombstone_rows`) reflect the whole branch; all queries identical to baseline. | Facts mis-split (active rows leak into the layout snapshot, or vice versa). |
| Snapshot self-consistency (D.2) | A loaded `Arc<BranchLayout>`'s `owned_levels` and folded facts always mutually consistent — facts recomputed from the loaded levels equal the carried facts. | Facts not folded into the swapped `Arc`. |
| Lock-free read concurrency (D.2) | Stress + loom: concurrent installs ∥ reads; every loaded snapshot is whole and self-consistent; read results == locked baseline. | Torn/partial layout; layout/facts skew. |
| Clone / Eq (D.2) | Manual `Clone` yields an independent equal state; mutating the clone doesn't affect the original; `Eq` compares loaded layouts. | ArcSwap shared-mutation bug; wrong equality. |
| Read precedence + MVCC (D.2) | Existing read-order/visibility tests pass: active → frozen(newest) → L0(newest) → L1+ → inherited; newest-commit-first; tombstone/TTL. | Precedence or MVCC ordering regressed. |
| Fork COW under concurrent install (D.2) | Child's inherited `Arc` is immutable across parent stores; fork-version gate hides post-fork rows; shared segments not freed while inherited. | Parent mutation leaks to child; premature reclaim. |
| Clear/delete vs install race (D.2) | Concurrency oracle: clear/delete racing an install never resurrects the branch. | Deleted-branch resurrection. |
| Visible-version atomicity (D.3) | Concurrent publish ∥ read: visible version monotonic, never torn; advance/unchanged/reject-regress outcomes preserved; reads/checkpoints load lock-free. | Torn / non-monotonic visibility. |
| Recovery equivalence | Recovery/fault oracle: recovered layout == synchronous baseline for the same history; crash windows recover; no observed table pointer without a durable manifest entry. | Decoupling changed recovered state. |
| Format goldens | All format goldens pass. | On-disk drift (must be none — D is in-memory). |
| Cache regression | Cache-mode branch state + L8G counters unchanged. | Durable change leaked into cache. |
| Determinism boundary | Deterministic-inline path bit-identical across seeds; all waits use the injected clock. | Wall-clock / nondeterminism entered the drive path. |
| Convoy benchmark | Crawl-rate A/B (convoy rate ↓) + workload-F run-phase throughput at 10M (↑) vs control. | Read path still contends the runtime lock. |

## Slice detail

### D.1 — refactor equivalence (regression-only)
- **Run the full suite + goldens unchanged.** D.1 is behavior-preserving; any diff is a bug.
- **Fact-folding equivalence test:** construct branches with assorted owned-level sets
  (varying levels, table key ranges, commit/timestamp spans) and assert the new
  `BranchLayout`-derived `timestamp_coverage` + `observe_rows_from_summaries` equal the
  values the pre-refactor code produced for the identical level set. Reuse the existing
  `branch/state` test builders.
- **Fact-split test:** a branch holding rows in `active` + `frozen` + `owned_levels`; assert
  the layout snapshot's folded facts count only owned levels while `max_commit_version` /
  row counters count the whole branch; latest/getv/history/scan results unchanged.
- **No in-place layout mutation remains:** an install (flush/compaction/materialization)
  produces a *new* `BranchLayout` (assert the pre-install layout value is unchanged where a
  reference was retained) — sets up the D.2 atomic-swap contract.

### D.2 — ArcSwap + lock-free reads (the keystone; risk concentrates here)
- **Snapshot self-consistency (property):** for randomized layouts, `load_full()` returns a
  snapshot whose facts == facts recomputed from its own `owned_levels`. proptest over level
  sets.
- **Lock-free read concurrency:**
  - *Loom* model (preferred for the swap/load correctness): N installer steps storing
    distinct layouts ∥ M reader steps; every observed snapshot is one of the stored layouts
    in full (no field-tear), and its facts match its levels. (Add `loom` dev-dep if absent;
    gate behind `cfg(loom)` like standard lock-free crates.)
  - *Thread stress* (integration): one thread loops flush/compaction installs while several
    threads loop reads (latest/getv/scan); every read result is valid for *some* committed
    state (validate against the row oracle), and no read returns a partial/middle layout.
    Run under ASAN/TSAN in CI.
- **Clone/Eq:** clone a populated `BranchLocalState`; mutate the clone (install a table);
  assert the original is unchanged and `!=` the clone; assert two independently-built equal
  states are `==`.
- **Read precedence + MVCC + reachability/timestamp:** the existing `immutable_reads`,
  `row_pruning`, `history_pruning`, `facts_reachability`, snapshot, and inheritance/
  materialization test families pass unchanged (these encode invariants 2–3).
- **Fork COW under concurrent install:** fork a child off a parent; on a background thread
  install new layouts on the parent; assert (a) the child's inherited `Arc<BranchLayout>` is
  pointer-stable / value-immutable, (b) rows committed to the parent after the fork are not
  visible in the child (fork-version gate), (c) segments inherited by the child are not
  reclaimed while referenced.
- **Clear/delete vs install race:** drive the concurrency/fault oracle with interleaved
  clear/delete + flush/compaction installs; assert a deleted branch never reappears and a
  cleared branch ends empty, identical to the serialized oracle.
- **Convoy A/B:** rebuild engine-ycsb; interleaved control-vs-fixed crawl-rate harness
  (n ≥ 9, 10M, 32/48g) + workload-F run-phase throughput — D.2 should remove the worker-side
  read stall (full convoy kill may also need Group C's off-lock install).

### D.3 — atomic visible-version
- **Monotonic concurrent publish:** many threads publishing increasing/equal/lower versions
  ∥ readers; assert visible version never regresses, never tears (always a previously-published
  value), and the advance/unchanged/reject-regress classification matches the serial impl.
  Loom model for the CAS + load.
- **Replay catch-up:** `catch_up_visible_after_replay` keeps the monotonic rule under the
  atomic backing; recovery restores visible only after rows are installed.
- **Existing `commit/tests/visibility.rs`** pass unchanged (made concurrent where they assert
  monotonicity).

## Cross-cutting (every slice)
- **Standing gate:** full `cargo test -p strata-storage-next` + the recovery/fault oracle
  (`testkit::simulation` / fault sweep) + `format_goldens` green before the next slice.
- **TSAN** clean for the threaded stress tests (data-race detector is the backstop for the
  lock-free paths); **loom** for exhaustive interleaving of the swap/load and CAS/load.
- **Cache mode:** the ArcSwap change touches cache-mode `BranchLocalState` too — run the
  cache test families + L8G absence counters; behavior must be unchanged.
- **Determinism:** deterministic-inline path replays bit-identical across seeds.

## Tooling notes
- The lock-free correctness (D.2 swap/load, D.3 CAS/load) is exactly what **loom** is for;
  prefer a small loom model over hoping a thread-stress test hits the bad interleaving. Keep
  the thread-stress + TSAN test as the integration backstop.
- The convoy benchmark uses the existing engine-ycsb harness (crawl-rate A/B + workload-F),
  not a unit test — it is the Group F closeout signal, run per slice that touches the read path.
