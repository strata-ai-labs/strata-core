# BS5 — Write concurrency: implementation and test plan

Status: **ready to implement after BS4** (numbers below re-validated against BS4's
re-baseline). Milestone BS5 of `billion-scale-plan.md` (gaps G17, G18, G19; M4P-L8I Groups
B/E). Change class: intentional semantic change (commit protocol concurrency). Assurance:
S3 — the recovery oracle and crash sweeps are the hard gates; this milestone touches the
durability-ordering core.

## Problem (recap)

Every commit executes serially under the single runtime mutex (`api/runtime/mod.rs:2648`):
N writer threads serialize at `slot.lock()`, and in `Always` durability every commit pays
its own fsync (`wal.rs:1016-1021`). RocksDB's write path: writers join a lock-free group,
one leader batches the whole group's WAL under a dedicated WAL mutex with **one** fsync,
followers insert into a concurrent memtable, and the DB mutex appears only on `UNLIKELY`
structural transitions (`rocksdb-parity-roadmap.md` RC1; write-path extract). The current
scoreboard is single-threaded — BS5's wins are invisible to it, which is why this milestone
starts by building its own benchmark.

## What reconnaissance established (all anchors verified)

**Exactly five shared serialization points; everything else is already per-branch-ready:**

1. **Global version allocator** — `CommitFactAllocator.last_allocated` monotonic counter,
   `&mut` per commit (`allocator.rs:62-66`); timestamps assigned jointly under the same
   borrow (monotonic frontier: generated clamped up, explicit rejected below —
   `allocator.rs:116-139`).
2. **The WAL** — single held append descriptor (`active_append`, `wal.rs:824`,
   single-writer by construction); one record per commit; `Always` fsyncs per append
   (`wal.rs:1016-1021`); thread-local encode buffers; segment rotation force-syncs and
   reopens the descriptor (`wal.rs:1399-1428`).
3. **Global visible tracker** — one scalar, strictly non-regressing
   (`visibility.rs:35-38`).
4. **The global durable gate** — single-slot `active_admission` bool + one
   `Option<unresolved>` fact (`durable_gate.rs:46-50, 264-289`): at most one mutating
   commit in flight database-wide, taken *first* ("global for V1 visible-version safety",
   `durable.rs:214-220`). On failure after WAL durability it records `DurableNotApplied` /
   `AppliedNotVisible` keyed to a **single** commit stamp; a second distinct fact is
   rejected (`:299`) and all mutation freezes until replay/reconciliation clears it.
5. **The runtime mutex** itself.

**Already per-branch and concurrency-ready:** the memtable (per-branch
`Arc<TableMemoryState>` with its own `RwLock`), branch-local metadata, and the branch
commit guards — whose module doc states the intent verbatim: *"serialize mutating work for
one branch, allow independent branches to proceed at the same time"* (`guard.rs:3-7`;
per-branch `active_branches` set, brief mutex, logical RAII tokens; `try_begin_quiesce`
is the structural-transition interlock).

**The ordering invariant chain** (what MUST stay ordered across concurrent commits):
(i) version allocation is monotonic and must equal WAL-append order — recovery replays in
version order and `catch_up_to` restores the counter from the WAL; (ii)
`allocated_version > visible` (`durable.rs:243/440`) and `branch_applied ≤ visible`
(`durable.rs:229/452`); (iii) visible publish is strictly monotone; (iv) the timestamp
frontier is monotone, assigned jointly with versions. **Memtable apply order is NOT
load-bearing** (keyed inserts + visibility-bounded reads) — only the four above are.

**Three facts that shape the design:**
- **Groups fit the gate.** Nothing in the gate counts commits — one `active_admission`
  span can cover a whole group. But partial group failure cannot be represented by the
  single-fact slot.
- **Mid-group apply failure is structurally near-impossible.** Internal keys are
  `physical_key ‖ ~commit_version`; each group member holds a distinct version, so
  cross-member duplicate-key conflicts cannot occur, and intra-batch duplicates are
  pre-validated before any mutation (`append.rs:137-139, 170-198`). A mid-group apply
  failure therefore indicates an invariant violation, not an expected runtime event —
  whole-group atomicity is the natural failure model.
- **Rollback is single-writer-only.** The scalar-baseline restore
  (`mutable.rs:169-182`: size/min/max/sequence snapshots) cannot survive concurrent
  inserters. But visibility-bounding (sequence pins + `versions > visible` blocked) means
  unpublished rows are *invisible* — reader correctness never needs rollback, only space
  cleanup does.
- **The timeline rides the batch.** Every commit carries 2 timeline rows (space `0x01`)
  through the same WAL record and memtable apply (`cache.rs:176-208`) — group machinery
  handles them for free.

**Cache-mode sharing (constraint C2 precision):** cache commits share the allocator, the
durable gate, the visible tracker, and the branch guards (`commit/cache.rs:79-121`) but
have **no WAL and no replay path** — the gate is cache mode's only backstop. So WAL/group
machinery is durable-only, but any change to the four shared structures must keep cache
commits byte-identical in behavior.

## Design

### The target shape (RocksDB's, adapted)

```text
writer threads ──► join queue (outside all locks) ──► leader drains ≤K writers
  leader: [gate: one admission span for the group]
          [allocator: reserve contiguous version block v..v+n, timestamps jointly]
          [WAL lock: append n records in version order → ONE force_durable (Always)]
          [apply: each member's rows into the per-branch memtables]
          [visible: publish once, to the group's max version]
          [distribute per-member outcomes; release]
fast path: no runtime mutex. Structural transitions (rotation, install, branch lifecycle)
take the runtime mutex — the RocksDB `UNLIKELY` pattern.
```

Group-of-1 degenerates to exactly today's serial protocol — the dark-launch equivalence
anchor (WAL records byte-identical).

### Decisions

- **D1 — whole-group atomicity.** Any member failure after the group's WAL is durable is
  group-fatal: one unresolved fact covering the group. The `CommitUnresolvedDurable` fact
  gains a **version range** (`first..=last` of the group; a group of 1 reproduces today's
  single-stamp fact) — a modest generalization of `durable_gate.rs:25-33` and its
  exact-CAS `clear_exact`/`replace_exact`, with recovery replaying the whole range from
  the WAL. Chosen over per-member fact sets because member-specific failure is structurally
  near-impossible (above) and range-replay matches WAL recovery's existing shape.
- **D2 — leader-executes-all first, parallel apply later.** BS5.1's leader performs every
  member's memtable apply itself — the memtable stays effectively single-writer, so the
  existing scalar-baseline rollback remains valid (taken across the whole group). The
  concurrent memtable (BS5.3) is measure-gated on BS5.2 profiling.
- **D3 — simple join structure first.** `Mutex<VecDeque> + Condvar` for the group queue
  (first-comer leads, drains up to K, wakes followers with outcomes). RocksDB's lock-free
  CAS list + spin/yield/park ladder is a recorded optimization, adopted only if the
  benchmark shows queue-lock contention. On wasm (C1) the process is single-threaded, so
  every group has size 1 and no waiting path is ever exercised.
- **D4 — admission per member, evaluated by the leader.** The leader runs each member's
  (post-BS1 cached, O(1)) admission + BS3 pacing before building the group; a member
  rejected by its branch's stop grade is failed individually *before* the group's WAL
  write (pre-WAL failures are clean rejections, not unresolved facts). Exact
  pacing-vs-grouping interplay (one leader sleep for the max delay vs per-member) is a
  design-during-implementation item with a test either way.
- **D5 — allocator/visible become atomics only when the mutex leaves.** In BS5.1 (still
  under the runtime mutex) they stay as-is. BS5.2 converts: allocator → fetch-add block
  reservation under the commit-protocol lock; visible → release-store atomic (BS2 already
  made readers acquire-load it).

## Slices

### BS5.0 — Concurrent-writer benchmark + baseline (measure first) — LANDED

**Changes (as landed).** New `benchmarks/src/bin/storage_next_concurrent_writers.rs`
(modeled on the concurrent-reads bin — the original `engine-ycsb --writers` idea was
retargeted: that bin is single-threaded and drives the old engine): N writer threads share
one `&runtime` (commit is `&self`; the runtime is `Send + Sync`), distinct-key batches, a
fresh runtime per measurement point; `--engines cache,standard,always`,
`--branches shared,per-writer`, optional `--readers M`, thread sweep {1,2,4,8}. Output:
CSV + a `BenchmarkReport` row with a `threads` parameter (the permanent write-scaling
column). A `rocksdb-ycsb` comparison mode remains an open item for the scoreboard.

**Tests (as landed).** Multi-writer S3 stress in `api/tests/off_lock_concurrency.rs`
(4 writers × cache/durable): per-writer acked versions strictly monotonic, globally unique
across writers, read-your-writes after every ack, checker threads enforce per-writer batch
atomicity + monotonicity.

**Bugs found by this slice (fixed with it):** (1) internally generated commit timestamps
were routed through the strict `Explicit` allocator path and spuriously rejected below the
monotonic floor under concurrent writers — new `RuntimeGeneratedBase` policy clamps like
`RuntimeGenerated` (only genuinely caller-supplied stamps stay strict); (2) rotation did
not republish the Model-2 snapshot — the background flush's phase-1 rotation (off-lock
build window) and commit-triggered auto-rotation both left the published view without the
fresh active, so acked commits were invisible to readers for 15–140 ms (V-before-S
coverage violation). Both republish in the same lock hold now.

### BS5.1 — Write groups (leader-executes-all, under the existing runtime lock) — LANDED

Landed as designed, with these deltas discovered during implementation:

- **Two pre-lock serializers had to fall first.** `next_commit_timestamp()` and
  `resolve_commit_durability()` each took the full runtime lock per commit, so writers
  queued behind the in-flight fsync BEFORE reaching the commit path and the join queue
  always drained empty (groups of 1–2, flat curves). The timestamp base now reads an
  off-lock atomic mirror on `StorageRuntime` (clamp semantics unchanged — the allocator
  still enforces the monotonic floor under the lock; the old locked read was equally
  stale-by-interleaving); the mode comes from the open summary.
- **Members wait on their own condvar, never the runtime lock.** The everyone-blocks-on-
  the-mutex fallback design measurably starved formation (parking_lot barging interleaved
  fresh fsync holds into the wake chain). Leadership is a queue-state flag handed off by
  promotion, with a panic-safe drop guard and a 100 ms timed-wait self-promotion fallback
  for lost wake-ups.
- **`Always`-only 150 µs formation window.** Served members re-join microseconds after the
  handoff; without holding formation open they ride only every second fsync round
  (cohort alternation, measured). Gated on observed contention and on `Always` mode — a
  window under Standard's ~µs holds dominates them (measured 21K → 8K before gating).
- `require_append_satisfies_policy` is skipped for members instead of stamping
  `forced_durable` (the leader's finalize owns the `Always` guarantee); the
  `WalAppend::covered_by_group_durable` helper was deleted as dead.
- Bootstrap-level per-member admission runs interleaved (admit → execute per member, not
  all-admissions-first) so budget projections see earlier members' consumption — a
  same-branch group is serially equivalent, including its rejections.

Measured (dev box, medians of 3): `Always` shared 161 → 224/278/373 commits/s at 2/4/8
threads (was ~159 flat); `Always` per-writer 305 at 4 threads (was ≤1.28×); `Standard`
unregressed (~20–22K flat). Group traces confirm fsync batching (size-7 groups cost one
solo hold). The residual gap to the ≥4× gate is formation/fsync pipelining — the two live
under one mutex, which is exactly BS5.2's cut.

Test coverage landed: group-of-1 byte-identity (whole-backend object snapshots, Standard +
Always), version contiguity in member order, per-member clean rejection, all-rejected
groups, mid-group WAL rotation, join-queue protocol units (FIFO/cap/promotion/handoff/
guard), 4-writer S3 stress green across repeated release runs. **Open for the test track
(carry into BS5.2's matrix): the group-boundary crash sweeps** — the range-fact replay
seams are unit-covered (`covers_version` replay admission, fact widening round-trip) but
`fault_sweep`/`crash_recovery_oracle` do not yet inject at group boundaries.

**Changes.**
1. The join queue on `RuntimeSlot` (D3): writers enqueue prepared batches; the leader
   drains ≤K (default: worker-count-independent, e.g. 16), executes the group under **one**
   `slot.lock()`, distributes outcomes.
2. Group execution inside the lock: one gate admission span (D1 range fact); contiguous
   version block + joint timestamps from the allocator; per-member branch-guard
   acquisition (different branches proceed; same-branch members execute in version order);
   N WAL appends in version order + **one `force_durable`** for `Always`
   (`wal.rs:1016-1021` seam), stamping every member's `WalAppend.forced_durable = true`
   so `require_append_satisfies_policy` (`durable.rs:494-509`) passes; **mid-group segment
   rotation** handled (rotation's own force-sync covers pre-rotation records; the group's
   final sync covers the new segment); leader applies each member's rows; one visible
   publish to the group max; whole-group rollback on any post-WAL failure (D2) with the
   range unresolved fact.
3. `Standard` mode grouping is a smaller win (no per-commit fsync today) but still
   amortizes lock acquisitions and admission overhead — measured, not assumed.

**Tests.**
- **Equivalence anchor:** group-of-1 produces byte-identical WAL records and identical
  outcomes to today's serial path (differential test).
- Protocol units: version-block contiguity; WAL order == version order across interleaved
  groups; publish == group max; gate admits a group as one span; the range fact
  round-trips `record → clear_exact`; mid-group rotation.
- **Crash sweeps (the hard gate):** crash after group fsync before apply → recovery
  replays the whole group; crash mid-group-WAL-write → prefix replay (torn tail rejected
  by CRC, all-or-nothing per record, ack'd members only after group fsync — no ack can
  precede durability); crash between apply and publish → range `AppliedNotVisible`
  reconciliation. Extend `fault_sweep`/`crash_recovery_oracle` with group-boundary
  positions.
- Multi-writer stress (from BS5.0 harness): per-writer monotonic acks, batch atomicity
  (BS2 invariants), read-your-writes after ack.
- Cache-mode suites unchanged (groups are durable-path; cache commits keep the serial
  path in this slice).

### BS5.2 — Commit path off the runtime mutex

**Changes.** The leader's group execution stops taking the runtime mutex on the fast path:
- A dedicated **commit-protocol lock** serializes group leaders (allocator block
  reservation, WAL descriptor ownership, visible publish ordering); D5 converts the
  allocator to block fetch-add under it and visible to a release-store atomic.
- Per-branch apply goes through the branch's own structures (memtable `Arc` + a per-branch
  commit-metadata lock or the existing branch guard extended to cover metadata writes) —
  the branch guards already provide cross-branch independence.
- **Structural transitions keep the runtime mutex** (the RocksDB `UNLIKELY` pattern): a
  commit whose append crosses the rotation threshold takes the mutex to rotate + run the
  BS1 aggregate hooks + BS2 snapshot publication; flush/compaction installs and branch
  lifecycle are unchanged (already mutex-scoped). Lock ordering documented and enforced:
  join-queue → gate → commit-protocol lock → branch guards → (structural only) runtime
  mutex — no path acquires in reverse.
- BS1's cached-pressure reads and BS3's pacing move to the leader's pre-group phase
  (reading cached/atomic state only).

**Tests.** Lock-order guard (debug assertion or lockdep-style test); the full BS5.1 test
matrix re-run off-mutex; a maintenance-interference stress (groups committing while
flush/compaction/rotation run — asserting structural transitions still exclude correctly);
recovery oracle + fault sweep green; BS2's reader invariants re-run against concurrent
writers (readers never see torn groups; visible monotonicity).

### BS5.3 — Concurrent memtable + parallel group apply (measure-gated)

**Gate:** build only if BS5.2 profiling shows leader-side apply serialization as the
residual bottleneck at N ≥ 4 writers.

**Changes.** Memtable storage `BTreeMap`-under-`RwLock` → `crossbeam-skiplist` `SkipMap`
(already a workspace-vetted dependency): concurrent inserts by group followers
(RocksDB `allow_concurrent_memtable_write` analog); the sequence counter and size
accounting become atomics (**additive** deltas — the scalar-baseline snapshot/restore
rollback is retired); sequence-pinning read views (`clone_for_read_view` upper bound) and
seal-in-place freeze semantics preserved. Rollback → **group-orphan model**: on the
(structurally near-impossible) member failure, rows stay unpublished-invisible
(visibility-bounded) and are swept by the existing rotation→flush→compaction pipeline;
the range unresolved fact still freezes mutation until reconciliation.

**Tests.** Memtable differential suite (SkipMap vs BTreeMap: identical visible rows,
sequence pinning, freeze/rotation semantics, iterator order) run as a property test;
concurrent-insert stress with pinned readers (no torn reads, pins stable); orphan-sweep
test (unpublished rows never surface, eventually collected); BS2 stress re-run.

### BS5.4 — Per-branch sharding (deferred unless measured)

With the branch guards already per-branch and BS5.2's off-mutex fast path, different-branch
commits already parallelize up to the commit-protocol lock. Sharding the catalog/registry
(per-branch runtime state partitions, M4P-L8I Group E) is recorded as
**deferred-unless-measured**: build only if the BS5.0 multi-branch benchmark shows the
commit-protocol lock or structural-transition mutex as the multi-branch ceiling.

## Perf validation (milestone exit)

Control = BS4-final binary; treatment = per slice; the BS5.0 benchmark is the instrument.

1. **Primary (gate):** write scaling at 4 writer threads — `Always` mode ≥ **4×**
   single-writer throughput (group fsync amortization is the dominant term); `Standard`
   mode ≥ **2.5×**. Near-linearity band to 8 threads recorded, not gated (memory-bandwidth
   and WAL-lock ceilings expected).
2. **Primary (gate):** single-threaded scoreboard cells within noise of BS4 baseline
   (group-of-1 equivalence makes this structural, the gate verifies it).
3. **Secondary:** `Always`-mode single-writer latency (group formation must not add
   latency when uncontended — empty-queue fast path); mixed writers+readers stress
   throughput; multi-branch scaling (BS5.4 gate data).
4. Recovery oracle + fault sweep + group-boundary crash sweeps green — **mandatory every
   slice**; ledger rows per slice.

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** groups form from caller threads — **no spawned threads**; on wasm the
  process is single-threaded so every group has size 1 and the wait path is never
  exercised (queue code must still compile — no `std::thread::park` on the enqueue fast
  path, condvar wait only in the multi-writer branch). Wasm check-build in every slice's
  gates.
- **C2 (cache mode):** WAL/group machinery is durable-only. The four shared structures
  (allocator, gate, visible, guards) change shape in BS5.2/D5 — cache commits must remain
  behaviorally identical (its suites gate every slice); the gate's range-fact
  generalization degenerates to single-stamp for cache mode's group-of-1.
- **C3 (profiles):** group size K and queue depth are budget-independent constants; no
  profile interaction beyond the standing tier matrix re-run at milestone close.
- **C4 (branching):** per-branch guards already permit cross-branch group members;
  same-branch members apply in version order. The BS5.0 multi-branch benchmark +
  fork-during-concurrent-load stress (BS2's C4 invariants re-run under N writers) gate
  branch isolation; cross-branch reference rejection is untouched.

## Risks

| Risk | Mitigation |
|---|---|
| Durability-ordering bug (ack before fsync; torn group) | ack only after group `force_durable`; crash sweeps at every group boundary; the WAL-order==version-order invariant unit-tested; group-of-1 byte-equivalence anchors the protocol |
| Gate generalization breaks reconciliation | range fact degenerates to today's single stamp for groups of 1; exact-CAS clear semantics preserved; recovery-oracle replay over range facts |
| Deadlock from new lock ordering (BS5.2) | single documented order (queue → gate → protocol lock → branch guards → runtime mutex), enforced by a debug lock-order guard; no reverse acquisition exists by construction (structural transitions never enqueue commits) |
| Group formation adds uncontended latency | empty-queue fast path (single writer never waits); measured in exit gate 3 |
| Concurrent memtable subtly changes read semantics (BS5.3) | measure-gated; differential property suite; BS2 stress invariants re-run; seal-in-place + sequence pinning explicitly tested |
| Rollback retirement leaves garbage rows (BS5.3) | visibility-bounding proven by BS2; orphan-sweep test; unresolved fact still freezes mutation until reconciliation |
| Wins invisible to the single-threaded scoreboard | BS5.0 builds the instrument first; the scoreboard gains the write-scaling column permanently |

## Sequencing & PR discipline

BS5.0 → BS5.1 → BS5.2 → (BS5.3 measure-gated) → (BS5.4 deferred-unless-measured). One PR
per slice, `BS5.{n}` titles, ≤1,500 LOC net, standing gates every slice (full suite +
recovery oracle + fault sweep + wasm check-build + cache-mode suites + clippy/fmt).
Depends on BS4 (re-baselined numbers; the block cache absorbs the read side of mixed
workloads); BS2's visible-atomic and snapshot machinery are prerequisites for BS5.2's D5.

## Open items

- Pacing × grouping interplay (D4): one leader sleep at the max member delay vs per-member
  pacing — decide in BS5.1 with a test either way.
- Group size K and drain policy (fixed vs adaptive to queue depth) — tune from BS5.0 data.
- Whether `Standard` mode should also batch WAL *writes* (fewer syscalls) or only lock
  acquisitions — measure in BS5.1.
- The lock-free CAS join + spin/park ladder (RocksDB `write_thread.cc`) — adopt only if
  the queue mutex shows contention at N ≥ 8.
- BS5.4 trigger criteria — defined by the BS5.0 multi-branch baseline.
