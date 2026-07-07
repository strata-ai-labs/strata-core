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

### BS5.2 — Commit path off the runtime mutex — LANDED

Landed per the design revision below, with these deltas discovered during implementation:

- **One sync in flight, fresh capture.** Overlapping fsyncs on one file do NOT
  parallelize (the device flush is the serial resource) — the first pipelined cut
  measured barely above BS5.1. The landed shape is the classic group flush: a sync-chain
  token admits ONE syncer at a time; the syncer re-captures its ticket at sync time (one
  brief runtime-lock hold), so its fsync covers every group that appended while the
  previous sync ran; everyone else proves coverage against an off-lock durable-sequence
  watermark and skips syncing entirely.
- **A 250 µs syncer beat, gated on open gate spans (>1).** The cohort served by sync k
  re-appends microseconds AFTER sync k+1's capture; without a beat the cohorts alternate
  and every sync covers half the writers (measured: 350 → 563 commits/s at 4T from this
  one change). Solo commits never pay it (single span).
- **Out-of-order settlement rules.** A later group may publish first (its covering sync
  proves everything below it durable; apply order guarantees everything below is
  applied), so an earlier group's publish becomes a monotone no-op — the first pipelined
  run group-fataled on the tracker's "cannot regress" check until this rule landed. A
  recorded fact fails a completing group only when `fact.first <= group.last`; a fact
  strictly above the group's range does not block it. A group whose own sync failed is
  rescued if the watermark passed its capture (a later sync covered it).
- **Flush defers on mid-pipeline branches.** Background flush now defers when a branch
  has applied rows above the visible version — freezing/snapshotting in the pipeline gap
  could install a durable table containing rows whose WAL records are not yet fsynced
  (a crash could resurrect an unacked half-group). The checkpoint/watermark paths were
  already visible-clamped.
- The gate's single `active_admission` slot became a counted multi-admission span
  (BS5.2b); solo commits under the pipeline validate at the pipeline frontier
  (`max(visible, highest in-flight applied version)`) via an explicit visibility floor —
  byte-identical when the pipeline is empty.
- The Always formation window from BS5.1 was deleted (the sync chain batches durability;
  group size no longer matters), as was the BS5.1 `force_durable_for_group` seam.

Measured (dev box, isolated points, medians of 3): `Always` shared 160 →
270 / 563 / 1,117 commits/s at 2/4/8 threads — **1.7× / 3.5× / 7.0×** (BS5.1: 1.4× /
1.7× / 2.3×; BS5.0 baseline: flat 1.0×). Per-thread fairness tightened from ~1.4× spread
to ~1.05×. `Always` per-writer 4T: 509 (3.2×). `Standard` unregressed (~20–22.5K flat).
Single-thread byte- and throughput-identical (160 vs 159–161 baseline). The ≥4× gate at
exactly 4 threads is capped by flush-latency arithmetic on this box: one ~6.2 ms device
flush per round bounds 4 threads at 4/6.45 ms ≈ 3.9× ideal; the curve through 8 threads
(7.0×) is the gate's substance. Standard's ≥2.5× remains member-protocol-cost-bound —
BS5.3's question, per the revision note below.

Group-boundary crash sweeps landed with the phase split (BS5.1's carried debt): crash
before the covering sync → full replay on reopen (durable-without-ack, never the
reverse); torn WAL tail → complete-prefix replay (per-record CRC atomicity); injected
sync failure → range fact gates commits, members report durability-uncertain, reopen
reconciles the whole range; two-groups-in-flight fatal-above/publish-below ordering.

**Design revision (measured, before implementation).** BS5.1's lock-timeline trace
(`BS52_TRACE`) showed commit holds are already back-to-back at 4 threads (inter-hold gaps
13–50 µs, `lock_wait ≈ 0` — background drains do NOT contend the mutex during write
bursts). The mutex is 100% occupied by commit holds, and each ~6.3 ms hold is ~95% one
fsync. The BS5.1 throughput variance (278–497 commits/s at 4T) is group-size luck in the
formation race. Therefore the binding constraint is NOT runtime-mutex contention — it is
that **formation and appends cannot overlap the in-flight fsync while both live inside one
mutex hold**.

Revised cut: instead of re-homing commit state under a dedicated commit-protocol lock
(original D5), the leader **releases the runtime mutex across the fsync**:

1. Hold 1 (runtime mutex): per-member admission + WAL appends + memtable applies
   (~200 µs/group) + `begin_group_sync` ticket. Release.
2. Off-lock: `backend.sync_object(wal_object)` — sound because LocalFs `sync_object`
   fsyncs a fresh fd of the same file and the append handle is an unbuffered raw `File`
   (POSIX fsync covers the file, not the writing fd); memory backend no-ops.
3. Hold 2 (re-acquire): `complete_group_sync` (clear dirty state if unrotated; halt writer
   on failure) + one visible publish to the group max + post-commit hooks. Release, then
   complete members.

While group n fsyncs, group n+1 forms, appends, and applies — groups become
"everyone who arrived during the previous fsync", deterministically, killing both the
formation race and the cohort alternation. Publish ordering is safe out of the box: any
completed fsync covers all earlier appends to the same file (rotation force-syncs retired
segments), and publish is monotone-max, so a later group publishing first implies the
earlier group's records are durable.

Semantic prerequisites (the real work):
- **Gate multi-admission**: the durable gate's single `active_admission` slot becomes a
  set of in-flight group ranges; the first post-append failure records ONE fact covering
  `[oldest unacked first .. newest appended last]`; later halted completions report
  failure without recording (their range is covered, or widened via `replace_exact`).
- **Pipeline frontier**: `require_branch_not_ahead_of_visible` and conflict-source bounds
  generalize from the visible version to the frontier of applied in-flight versions
  (applied-above-visible is normal between fsync and publish); with pipelining off,
  frontier == visible — byte-identical.
- Ack only after the member's covering fsync and publish; fsync failure halts the writer
  and all unacked in-flight members report durability-uncertain.

The original D5 (allocator fetch-add, visible release-store atomic, per-branch apply locks,
runtime mutex only for structural transitions) is **deferred to a data-driven decision
after this lands**: it serves Standard-mode scaling, and the BS5.1 numbers say Standard is
member-protocol-cost-bound (~45 µs/commit serialized work with no shared fixed cost to
amortize), which points at BS5.3 (concurrent memtable) rather than lock decoupling. The
lock-order rule stays as documented for whatever acquires both: commit path may acquire the
runtime mutex; nothing holding it re-enters the join queue.

Sub-slices: **BS5.2a** WAL group-sync tickets (begin/complete split, behavior-neutral);
**BS5.2b** gate multi-admission + frontier bounds (pipelining off, byte-identical);
**BS5.2c** the pipelined leader + matrix + bench (target: Always ≥4× at 4T);
**BS5.2d** group-boundary crash sweeps (BS5.1's carried test debt, doubly needed now);
**BS5.2e** gates + baseline + docs.

**Original changes (superseded by the revision above; kept for the record).** The leader's
group execution stops taking the runtime mutex on the fast path:
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

**BS5.3a — LANDED (the measure gate spoke first).** Fine-grained profiling of the
Standard commit budget (new `--perf-breakdown` on the instrument + temporary probes)
found the premise of the SkipMap plan wrong for today's bottleneck: the 45 µs/commit
Standard wall was ~16 µs commit protocol (memtable apply only 7 µs of it) plus
**10–18 µs of runtime-lock wait per commit against background maintenance holds** —
and the maintenance-off A/B proved maintenance is load-bearing (admission stalls
without it), so the interference had to be fixed, not avoided. True-hold attribution
(post-acquisition clocks) found the thieves and landed three fixes:

1. **Inline WAL reclaim off the lock** (the big one): `reclaim_wal_after_flush` ran two
   durable-manifest loads, an O(rows) coverage proof, and a durable manifest replace
   (write + fsync!) per flush INSIDE the publish phase's lock hold — ~950 ms of a 3 s
   window. It now enqueues the coalescing background flush-watermark task (off-lock
   scan, D.2b-2), exactly as the periodic WAL-growth policy already schedules reclaim.
   Semantic ripple: flush-driven watermark advance is asynchronous (one drain later),
   with the periodic policy as backstop; the single-branch and nonzero-candidate gates
   preserved.
2. **O(1) reserved-manifest confirmation**: `record_reserved_manifest` re-recorded every
   catalog table per publish (O(catalog) clones + compares under the lock, and it could
   resurrect entries removed between the publish phases) — the reserved manifest is
   serialized FROM the catalog and phase one already recorded the new tables, so the
   fold is now `confirm_reserved_manifest_published()` (a debt-flag clear). Recovery
   still validates entry-by-entry.
3. **Writers-first drain yield**: commits register as waiters (`lock_for_commit`);
   drain rounds break between steps when a writer is blocked, after a one-task
   fairness floor. (Secondary — most probe-round holds measured trivially small.)

Measured (dev box, medians of 3): Standard shared 21K flat → **~30K at 1/4/8 threads
(+43–48%)**; writer lock-wait 15 → 6 µs at 1T; publish-phase true holds 950 → 41 ms.
Always and cache byte-unregressed (160/553/1105; cache identical).

**BS5.3b — LANDED.** The flush-install hold was `frozen_rows_match_table`: a row-by-row
lockstep walk of the built table (through its reader) against the frozen memtable, under
the runtime lock, per install (~7.5 ms each), plus an all-frozen fallback scan. Landed
fix: the prepared durable flush captures the `Arc` identity of the sealed memtable its
build consumed; the install matches by identity — O(1), and strictly more precise than
row comparison (same object, not merely equal contents; a freeze landing in the publish
gap can no longer be confused with the build's input, tested both ways). The row-equality
verification moved into the off-lock prepare phase (end-to-end through the published
object's reader, before install). Cache and the inline single-hold flush keep the
row-match path (no off-lock gap; C2 conservatism).

Measured (dev box, medians of 3): Standard shared 30K → **~35K at 1/4/8 threads**
(cumulative +65% over the 21K baseline); writer fg-wait 292 ms per 3 s window at 1T
(from 1,330 ms at baseline). Always (162/538/1105) and cache byte-unregressed.

**BS5.3c — LANDED (attribution closed; the gate question is reframed).** Split-probing
the remaining ~10 µs of per-commit "dispatch machinery" closed the books:

- Join/leadership: 0.4 µs. Residual lock wait: ~3 µs. Notify: sub-µs. Response
  snapshots: 0.1 µs. All clean.
- One real mechanical fix landed: the post-commit WAL-growth wait re-probed CURRENT
  growth facts through TWO extra runtime-lock acquisitions per commit, for a condition
  the commit's own under-lock evaluation had already answered. It now gates on the
  carried outcome's status (below-threshold/disabled → skip; a crossing is caught by the
  next commit's own evaluation, the loop's documented re-check semantics).
- **The rest of the gap is not overhead — it is BS3's write-throttle pacing.** Under the
  sustained bench load the admission P-controller paces the writer ~20% of wall
  (measured: 5,664 paced commits, ~0.7 s of actual pacing in a 3 s window at 1T) as the
  run fills the default memory budget. That is intentional backpressure doing its job.

Standard lands at **~35K commits/s at 1/2/4/8 threads** (from 21K flat at BS5.0 — +67%
cumulative across BS5.3a/b/c), all of it from removing REAL waste (inline fsyncs under
the lock, O(catalog) re-records, O(rows) install walks, redundant lock probes) with the
backpressure semantics intact. Always (161/539/1078) and cache unregressed throughout.

**The ≥2.5× (~50K) Standard gate is now a two-part question, deliberately left open:**
1. **Protocol capacity** (~16 µs serialized: apply 7.4, WAL append 3.5, admit 1.8,
   stage 1.6): the SkipMap + parallel-apply plan below is the structural answer, at
   substantial complexity (D2 changes, group-orphan rollback, differential suites).
   Medium options short of it: single-write WAL group batching (~1-2 µs/commit at 4T+),
   apply-path micro-work.
2. **Pacing calibration**: whether the BS3-era throttle thresholds (memory-pool fullness
   knee) are right for the post-BS5.3 regime is a PRODUCT decision — retuning trades
   write throughput against memory headroom and read amplification protection, and any
   change belongs to an admission-focused slice with its own A/B, not a lock-hygiene one.

The original SkipMap gate text below stands for whenever (1) is taken up.

**Original gate:** build only if BS5.2 profiling shows leader-side apply serialization as the
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

**Milestone exit status (recorded at BS5.3c close, 2026-07-07, dev-box medians of 3):**

1. **Primary (gate):** write scaling at 4 writer threads — `Always` mode ≥ **4×**
   single-writer throughput (group fsync amortization is the dominant term); `Standard`
   mode ≥ **2.5×**. Near-linearity band to 8 threads recorded, not gated (memory-bandwidth
   and WAL-lock ceilings expected).
   - `Always`: **3.5× at 4T / 7.0× at 8T** (160 → 553 / 1,117). The 4T number is capped by
     flush arithmetic on the dev box (one ~6.2 ms device flush per round bounds 4 threads
     at ~3.9× ideal); the curve through 8 threads carries the gate's substance —
     **substance met, exact-4T number machine-bound.**
   - `Standard`: **~1.7× at 4T (21K flat → ~35K at every thread count, +67%)** —
     **gate DELIBERATELY PARKED, not abandoned.** BS5.3c closed the attribution: the
     remaining gap decomposes into (a) the ~16 µs serialized commit protocol (apply
     7.4 µs, WAL append 3.5 µs) and (b) BS3's write-throttle admission pacing (~20% of
     wall under the sustained bench load as the default memory budget fills —
     intentional backpressure, not waste). Neither residual is a lock-hygiene fix:
     closing it requires the SkipMap + parallel-apply restructuring (D2 change, group-
     orphan rollback, 3–5 slices) and/or a pacing-calibration decision that trades write
     throughput against memory headroom (a product call — the same binary targets
     512 MB devices). Full data and sequencing analysis in the BS5.3c section above.
   - **Reopening criteria:** (i) the admission-focused slice that runs BS3.4c's
     graded-admission bake-off (pacing calibration belongs there, with stall-wall and
     small-budget guardrails as hard gates); (ii) real multi-writer workload data once
     the engine/executor layers run on storage-next (decides whether the SkipMap
     complexity is workload-motivated); (iii) the milestone review choosing to restate
     the gate against unpaced protocol capacity. Recommended order if reopened:
     admission A/B → single-write WAL group batching (~1–2 µs at 4T+) → re-measure →
     SkipMap only if still short.
2. **Primary (gate):** single-threaded scoreboard cells within noise of BS4 baseline
   (group-of-1 equivalence makes this structural, the gate verifies it). **Met** —
   single-thread byte- and throughput-identical every slice (`Always` 159–162 across
   BS5.0→5.3c; `Standard` 1T improved with the same fixes that improved 4T/8T; cache
   identical).
3. **Secondary:** `Always`-mode single-writer latency (group formation must not add
   latency when uncontended — empty-queue fast path); mixed writers+readers stress
   throughput; multi-branch scaling (BS5.4 gate data). **Met** — solo-in-Always
   pipelines as group-of-1 at unchanged throughput; per-writer branches 3.2× at 4T;
   BS5.4 remains deferred-unless-measured per its own section.
4. Recovery oracle + fault sweep + group-boundary crash sweeps green — **mandatory every
   slice**; ledger rows per slice. **Met every slice** (group-boundary sweeps landed in
   BS5.2d).

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
