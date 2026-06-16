# M4P-L8I Implementation Plan: Runtime Lock Decoupling

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Predecessor / motivation:
`docs/architecture/implementation-plans/M4P/m4p-l8h-closeout.md` (L8H closed with
the perf gap profiled to a single root cause; this plan is the deferred
"locking-architecture milestone").

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-test-plan.md`

> Milestone code `L8I` is provisional (next slice in the M4P/L8 family). Given the
> scope (runtime concurrency architecture), it may be promoted to its own milestone.

## Objective

Eliminate contention on the single global runtime mutex so durable throughput
approaches or exceeds the old engine. The end state: **the runtime lock is held
only for microsecond in-memory pointer swaps; all durable I/O (WAL fsync, manifest
and checkpoint persistence, table-object writes) and all heavy work (merge/build,
large clones) happen off that lock.** Durability, crash-consistency, recovery,
admission liveness, cache-mode behavior, and the frozen durable format are
preserved exactly.

This is a coupling/architecture fix, not a storage-format or semantics change.

## Diagnosis (from the L8H contention investigation)

At 10M, the foreground writer's ~263s `api_runtime` is **~16s real commit/WAL work +
~44s admission + ~200s global-mutex acquisition & admission-retry churn**. The
single mutex `RuntimeSlot.runtime: Arc<ParkingMutex<LifecycleDurableLocalRuntime>>`
(`api/runtime.rs:460`, acquired via `RuntimeSlot::lock()` at `:468`) is contended by
the foreground writer, up to four background drains (post-L8H C3 concurrency), and
the admission wait-loop. Four structural causes:

1. **WAL fsync under the commit lock.** `execute_durable_commit` holds the mutex
   across WAL append + `force_durable()` → `backend.sync_object()`
   (`service/wal.rs:988`). Every commit serializes a full fsync inside the lock.
2. **Manifest/checkpoint writes under the publish lock.** `finish_background_maintenance`
   (`lifecycle/durable/maintenance.rs:1436`) holds the mutex across the table-manifest
   persist (`lifecycle/table_manifest.rs:381`) and, for checkpoints, three backend
   writes. L8H/C1–C2 moved the merge/build off-lock but left the durable *writes*
   on-lock.
3. **On-lock clones.** Commit does `runtime_batch.clone()` under the lock; the drain
   snapshot phase does `branch.clone()` of frozen/all-branch state under the lock.
4. **Admission churn.** The foreground re-executes the whole commit every ~250ms
   slice (647,702 wait-attempts / 10,000 commits at 10M), re-acquiring the mutex
   ~6× per iteration — a self-throttling storm against the very mutex the drains
   need to relieve pressure.

The holds are long **because I/O runs under the lock**, which makes acquisition-wait
explode; the churn piles cheap acquisitions on top. The real work is competitive
with the old engine (~16s foreground + ~44s compaction-merge at ~1.3M rows/s); the
~8× gap is entirely this contention.

### The old engine is the blueprint

The pre-V1 engine (`crates/storage/src`, `crates/engine/src`) completes 10M in ~37s
with no such contention. Its model — to be ported:

- **`ArcSwap<SegmentVersion>` for the LSM layout** (`segmented/mod.rs`): flush and
  compaction are atomic single-pointer swaps; reads/commits never block on the layout.
- **Per-branch commit locks** (`txn/manager.rs`, `DashMap<BranchId, Mutex>`) +
  a shared-read quiesce `RwLock` taken for write only by rare maintenance.
- **Lock-free memtable** (`memtable.rs`, crossbeam `SkipMap`) and an **atomic
  visible-version** separate from the allocator.
- **snapshot (short guard) → build off-lock → atomic install (short guard, with
  `Arc::ptr_eq` reconciliation of concurrent flushes)** for all maintenance.
- **WAL fsync off the WAL lock** (group commit via a cloned fd;
  `durability/wal/writer.rs`, `durability/commit_adapter.rs`).

storage-next already does the build off-lock; it must also move the writes off-lock
and stop funneling everything through one mutex.

## Required invariants (must hold at every group's exit)

1. **Durability is not weakened.** A `Standard`/`Always` commit is acknowledged only
   after its WAL record is durable (fsync completed), even though the fsync runs off
   the runtime lock. Group commit may batch fsyncs, but no ack precedes its record's
   durability.
2. **WAL-fsync-failure halt-and-resume is preserved** (WAL writer halts on fsync
   failure; recovery via explicit resume).
3. **Manifest/snapshot durability ordering.** A table is relied upon for recovery,
   and the WAL retiring its inputs is truncated, only after its manifest entry is
   durably persisted. Per-branch manifest writes are serialized so durable manifest
   sequence never regresses (one full-snapshot object per branch, no CAS guard).
4. **Crash consistency.** A crash at any window — commit appended-but-not-fsynced,
   pointer-swapped-but-manifest-not-persisted, mid-group-commit — recovers to a
   consistent state via WAL replay. Recovery output is identical to a fully
   synchronous baseline for the same write history.
5. **Lock-free reads are consistent.** A reader/commit that loads the layout without
   the big lock observes the table set and its derived facts
   (`ObservedBranchRows`, timestamp coverage) atomically — never layout vN with
   facts v(N−1).
6. **Admission liveness (L8H Slice 1) is preserved.** `admission_wait_timeouts == 0`
   for a serviceable overload; a provably dead/stuck executor still surfaces a
   bounded typed failure; the progress-gated watchdog and its counters keep their
   semantics.
7. **No durable format change.** Table, manifest, checkpoint, WAL formats and codec
   are frozen (M3 golden vectors). On-disk bytes for a given logical state are
   identical.
8. **Cache-mode (L8G) behavior is unchanged.** Cache has no controller/admission;
   changes are durable-only or structurally cache-neutral.
9. **Dependency DAG and public surface unchanged.** All changes are `pub(crate)`/
   private within storage-next `api`/`lifecycle`/`branch`; no new D4 public types.

## Scope summary

| Group | Work | Exit gate |
| --- | --- | --- |
| A. Lock-free admission wait-loop | Pressure as a lock-free atomic snapshot updated by maintenance; foreground parks on a condvar the drain notifies on relief; retry once on relief instead of re-executing the whole commit per slice. | wait-attempts/commit drops from ~65 to low single digits; per-iteration runtime-lock acquisitions 6→≤2; `admission_wait_timeouts == 0` preserved. |
| B. WAL fsync off the commit lock | Hold the runtime lock only for WAL append (buffered) + in-memory apply; perform fsync off-lock (group commit / background sync), ack only after the record is durable. | No commit holds the runtime lock during fsync; durability + WAL-fsync-failure halt preserved; foreground lock-hold per commit excludes fsync. |
| C. Off-lock publish writes + per-branch serialization (+ crash consistency) | Split maintenance publish into {lock: pointer swap + sequence reserve} / {off-lock: manifest/checkpoint persist under a per-branch publish lock} / {lock: record}; gate WAL truncation/flush-watermark on durable persist. | Publish holds the runtime lock only for the swap; no durable manifest sequence regression under concurrency; crash-between-swap-and-persist recovers via WAL; recovery byte-identical to synchronous baseline. |
| D. ArcSwap layout + atomic visible-version | `owned_levels` → `ArcSwap` (publish stores a new `Arc`; reads/commits load lock-free, with derived facts folded into the same `Arc`); visible-version → atomic. | Point/scan reads and the commit's layout read take no runtime lock; layout+facts observed atomically; reads correct under concurrent publish. |
| E. Per-branch sharding (stretch; may split to its own milestone) | Replace the single `ParkingMutex<runtime>` with per-branch state guards + a shared-read quiesce `RwLock`, à la the old engine's `TransactionManager`. | Commits/maintenance on different branches do not contend; single global lock retained only for cross-branch/quiesce operations. |
| F. Benchmark closeout | Settle-to-quiescence old-vs-new comparison across 100K–10M, standard and always. | Durable standard ≤ 2× old at 10M (target); honest "quiesced" (L0 drained) comparison documented. |

## Implementation order

Sequenced to bank low-risk wins first, isolate the durability-critical work, and
keep each step independently measurable against the benchmark.

1. **Group A first (lowest risk, high churn reduction).** No durability change; it
   only changes how the foreground waits. De-risks the rest by removing the
   647k-attempt storm so later measurements are clean.
2. **Group B (foreground fsync off-lock).** The single biggest foreground lever.
   Durability-sensitive (ack ordering) but well-bounded; the old engine's group
   commit is the reference.
3. **Group C (maintenance publish off-lock + per-branch serialization + Group D
   crash-consistency).** The corruption-critical work; ship behind the full crash
   suite. Reuses the off-lock-fsync mechanics already mapped in the L8H investigation.
4. **Group D (ArcSwap layout + atomic visible-version).** Lock-free reads; completes
   the "lock held only for pointer swaps" end state.
5. **Group E (per-branch sharding)** — only if A–D do not reach the 2× target, and
   likely as its own milestone given blast radius.
6. **Group F (benchmark closeout)** — run after each group; formal close at the end.

Each group is sliced to ≤1,500 LOC per PR; C and D will each be multiple slices
(C: per-branch-lock + sequence-reservation; off-lock manifest; off-lock checkpoint;
crash suite. D: arc-swap field + read-guard plumbing; visible-version atomic).

## Group detail

### A. Lock-free admission wait-loop
- Add a per-branch lock-free pressure cell (small `AtomicU64`/`Arc<PressureCell>`
  of severity rank + L0/frozen/active counts) updated by the maintenance drain in
  its completion hook (`submit_drain`, already runs after each round and fires
  `record_lifecycle_pressure_clear_wake`, `api/runtime.rs:~860`) and by the commit
  path (it already holds the lock when admission is computed).
- Rework `background_wait_after_pressure_rejection` (`api/runtime.rs:3722`): read
  pressure + progress from the lock-free cell + the executor's lock-free
  `tasks_completed`; park on a condvar the drain notifies on relief (replace the
  per-slice re-snapshot that re-locks 4× and the per-slice whole-commit re-exec).
  Make `enqueue_pressure_maintenance_for_background_wait` idempotent per pressure
  episode (force flush/compaction once, not every slice).
- Preserve the Slice-1 watchdog: 30s stall deadline, reset on
  `backlog_reduced || maintenance_completed_task`, computed from the lock-free
  reads; keep `record_lifecycle_write_admission_wait_progress_reset` and
  `_wait_timeout` semantics. Inline (deterministic) executor degrades to the
  existing run-one-then-recheck behavior under `drain_immediately`.
- Exit gate above. Blast radius: `api/runtime.rs` admission paths +
  `BackgroundRuntimeController`; no format/public change.

### B. WAL fsync off the commit lock
- Restructure the commit critical section so the runtime lock is held for: append
  the WAL record to the in-memory/buffered log + apply to the active memtable +
  reserve the commit version. Release the lock; then ensure the record is durable
  (group commit: one fsync covers a batch of appended records) before returning the
  durable ack. Reference the old engine's `begin_background_sync` / `SyncHandle`
  (clone the fd, fsync off the WAL lock) and group-commit deferral
  (`durability/wal/writer.rs`, `durability/commit_adapter.rs`).
- Keep the WAL writer's halt-on-fsync-failure; a failed group fsync fails the
  covered commits' acks and halts, recovered by explicit resume.
- Do not change WAL format or replay. The visible-version publish ordering must
  still make a commit visible only after its record is durable (or define the
  not-yet-durable visibility window to be recovery-safe via replay).
- Exit gate above. Blast radius: `commit/`, `service/wal.rs`, the commit path in
  `api/runtime.rs`.

### C. Off-lock publish writes + per-branch serialization (+ crash consistency)
- 3-phase publish: {lock: pointer swap + reserve manifest sequence (atomic
  increment under the lock so concurrent cross-branch publishes get unique
  monotonic sequences)} → {off-lock: `encode` + `publish_replace_manifest` (fsync),
  and the checkpoint writes, under a **per-branch publish lock** held from
  sequence-reserve through the durable write so same-branch writes are ordered and
  cannot regress} → {lock: `record_manifest` (advance the durable sequence marker)}.
- Order: flush-watermark advance / WAL truncation already gate on the durable
  manifest loaded from storage (`load_required`, `lifecycle/durable/maintenance.rs:1831`),
  so the widened swap→persist window is recovery-safe — confirm and add the
  Group D crash test (crash between swap and persist → recovery loads the prior
  manifest + replays WAL → reconstructs the table).
- fsync failure → existing `table_manifest_debt_outcome` (visible but not durable;
  watermark gate refuses to advance; recovery reconstructs).
- Exit gate above. Blast radius: `lifecycle/durable/maintenance.rs`,
  `lifecycle/table_manifest.rs`, `lifecycle/rewrite_publication.rs`,
  `lifecycle/checkpoint.rs`, the drain loop in `api/runtime.rs`.

### D. ArcSwap layout + atomic visible-version
- `BranchLocalState.owned_levels` → `ArcSwap<OwnedLevels>` (tables are already
  Arc-backed, cheap to swap). Publish builds the new `OwnedLevels` off-lock (it
  already builds the vector) and `store`s it; **fold the derived `ObservedBranchRows`
  facts + timestamp coverage into the same `Arc`** so a lock-free reader sees layout
  and facts atomically. Convert flush install (in-place today) and compaction
  install (already copy-on-write) to build-new-then-`store`.
- Reads: add an owning guard (`load()` → `Guard`/`Arc`) so `owned_levels()` no longer
  hands out a borrow tied to `&self`; the lower-risk path is to snapshot once into
  the read view (`capture_read_view` already clones) so only that path changes.
- `VisibleVersionTracker` → atomic visible-version (commit publishes monotonically;
  reads/checkpoints load).
- Handle `BranchLocalState`'s derived `Clone/Eq` (ArcSwap isn't Clone/Eq) — manual
  impls cloning the Arc.
- Exit gate above. Blast radius: `branch/state.rs`, `branch/state/compaction.rs`,
  `branch/state/read_hooks.rs`, `branch/read.rs`, `commit/visibility.rs`,
  `lifecycle/branch_lifecycle.rs`.

### E. Per-branch sharding (stretch)
- Replace the single `ParkingMutex<runtime>` with per-branch state guards
  (`DashMap<BranchId, _>`) + a shared-read quiesce `RwLock` (write side only for
  cross-branch ops: fork, clear, delete, global maintenance) — the old engine's
  `TransactionManager` model. Likely its own milestone; in scope here only as the
  documented end state and a feasibility spike.

### F. Benchmark closeout
- A **settle-to-quiescence** harness: load completes AND L0 is fully compacted
  (no backlog) for both engines, so the old-vs-new comparison is apples-to-apples.
- Run 100K/1M/5M/10M, standard and always, vs `storage-old-cache-scale --engine standard`.

## Stop conditions

1. If group commit cannot guarantee a `Standard` commit's durability before ack
   without re-serializing on the runtime lock, stop and design the ack/fsync
   ownership before moving I/O.
2. If off-lock manifest persistence can regress the durable sequence under
   concurrency despite per-branch serialization, stop — keep publish serialized.
3. If a crash/recovery test fails after any decoupling, treat it as a durability
   regression, not a tuning issue; stop and fix the ordering contract.
4. If ArcSwap layout can expose layout/facts skew to a lock-free reader, stop and
   fold the facts into the swapped Arc before shipping.
5. If a change weakens the L8H admission watchdog (a serviceable load times out, or
   a dead executor hangs), stop and restore the progress-gated semantics.
6. If throughput improves only with a benchmark-specific path, reject the patch.

## Non-goals

1. No durable format, codec, manifest, checkpoint, or WAL format change.
2. No conflict/timestamp/branch-semantics change.
3. No second commit/read/maintenance path; one canonical implementation.
4. No new public (D4) surface beyond approved diagnostics counters.
5. No change to cache-mode lifecycle policy (L8G).
6. No migration tooling; no benchmark shortcut/retry/scale-specific fast path.
