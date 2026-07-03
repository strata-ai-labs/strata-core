# BS3 — Compaction throughput & graceful admission: implementation and test plan

Status: **ready to implement after BS1 (BS2 recommended first per the umbrella sequencing)**.
Milestone BS3 of `billion-scale-plan.md` (gaps G7, G8, G9, G10). Change class: perf +
intentional semantic change (admission thresholds/pacing). Assurance: S3.

## Problem (recap)

Update-heavy workloads collapse: A and F @10 M run at **~110 ops/s** (RocksDB: 272 K / 200 K
— a 1,500–2,500× gap). The mechanism, established by the admission diagnostic (1.8 M
`l0_paced` retry-waits, **0** flush-paced): L0→L1 compaction cannot drain L0 as fast as
64 MiB flushes fill it → L0 hits the blocking threshold (16) → commits hard-block into the
paced retry loop → throughput collapses to the compaction drain rate.

Three compounding causes:

1. **Compaction pipeline throughput.** A ~300 MB L0→L1 takes ~1.1 s end-to-end (~270 MB/s)
   — and that elapsed (`PreparedDurableCompaction.elapsed`) includes the *synchronous
   per-output publish chain*: for each ~64 MiB output table, backend write + `sync_all` +
   rename + parent-dir fsync (`backend/local_fs.rs:621/678/708`), plus two budget-ledger
   locks (`rewrite_publication.rs:708`). Whether the bottleneck is merge CPU (per-row policy
   dispatch, `previous_kept_key` bookkeeping, encode/CRC) or publish I/O is **unmeasured** —
   BS3 profiles before cutting (the hard-learned lesson of this branch).
2. **Admission is tight and abrupt.** L0 grades are Background/Urgent/Block = 4/8/16 tables
   (`lifecycle/compaction.rs:33-35`) vs RocksDB's slowdown/stop = 20/36; the soft throttle
   (fix #2: quadratic, 20 ms cap — `config.rs:21-24`, `mod.rs:3584`) is stateless
   fullness-proportional, not debt-adaptive, and the hard block bites 2× earlier than
   RocksDB's stop with no wide delay band before it.
3. **Subcompactions (Slice 4, `3bceb4c3`) currently default ON (=4) and are a measured ~25 %
   regression** in the resident (memory-bound) regime; the reunion test is still owed.

The compaction budget arithmetic that frames the milestone: sustaining the exit target of
**≥50 K update-ops/s at 1 KB** with write-amp ~10 requires **~500 MB/s aggregate compaction
throughput** — ~125–170 MB/s per lane across Slice 3's 3–4 concurrent lanes. NVMe supplies
multi-GB/s sequential; the current ~270 MB/s per lane (with publish serialized inside it)
is the thing to fix or excuse.

## What BS3 inherits

- **BS1**: pressure/score reads are O(levels) from cached sums; the retry loop no longer
  pays O(tables) per retry; per-level byte sums give a free **compaction-debt signal**
  (Σ max(0, level_bytes − level_target)).
- **BS2**: reads are off-lock, so admission pacing throttles *writers only*.
- **Slice 3** (committed): concurrent compaction (cap 4, per-(branch,level) tasks,
  conflict-aware dispatch). **Slice 4** (committed, WIP): subcompaction machinery, correct
  but not profitable while memory-bound.
- The RocksDB admission mechanism, fully extracted (`rocksdb-parity-roadmap.md` +
  the stall deep-read): two-grade classifier (delay/stop tested stop-first), **stateful
  multiplicative rate** (×0.8 debt-flat/growing, ×0.6 near-stop, ×1.25 recovering, ×1.4 on
  return-to-normal; floor 16 KB/s; default max 16 MB/s), token-bucket enforcement (1 ms
  refill, sleep = overage/rate), hard stop = wait-for-background (strata's retry loop is
  already exactly this).

## Slices

### BS3.1 — Hygiene: subcompactions default-off + the reunion test

**Changes.**
1. `DEFAULT_SUBCOMPACTIONS` 4 → 1 (`lifecycle/maintenance.rs`) — removes a known ~25 %
   L0→L1 regression from the default build. The machinery stays, env-gated
   (`STRATA_SUBCOMPACTIONS`), for its honest re-test in BS4 when compaction becomes
   I/O-bound.
1b. **G23 / constraint C1 (wasm):** the subcompaction fan-out's `std::thread::scope`
   (`lifecycle/rewrite_publication.rs`) is currently unconditional — gate it
   `cfg(not(target_arch = "wasm32"))` with a serial fallback (`n_eff = 1`), so the durable
   path can never attempt a thread spawn on wasm32. Add the wasm check-build to this
   slice's gates.
2. **The reunion test** (owed from Slice 4; the recon already established the full recipe):
   in `branch/tests/owned_compaction.rs`, build ≥5 distinct-key L0 tables
   (`branch_owned_table` helper — distinct keys are required or boundaries collapse), force
   the split with `TableCompactionConfig::new(1, max)` on the request, then assert
   `concat(prepare_branch_compaction_plan_bounded(range_i))` row-for-row equals the serial
   `prepare_branch_compaction_plan` output (rows via `into_parts_with_rows().2`), with
   anti-vacuous guards (`ranges.len() > 1`, `!candidate.is_metadata_promotion()`).

**Tests.** The reunion test itself + full suite. One scoreboard sanity cell (10 M workload A
load) confirming the default flip removes the regression.

### BS3.2 — Compaction pipeline decomposition (profile gate)

**Objective.** Attribute the ~1.1 s / 300 MB L0→L1 across: plan / merge-loop (cursor
advance vs `policy.decide` vs `push_row`) / output finish (encode + CRC) / per-artifact
publish (backend write, `sync_all`, rename, dir-fsync, byte-validation, reader handoff,
budget-ledger locks). Temporary `STRATA_TRACE` probes inside
`prepare_durable_compaction_publication` (`rewrite_publication.rs:92`),
`compact_table_inputs` (`table/compaction.rs:596`), and `publish_rewrite_artifact`
(`rewrite_publication.rs:584+`) — stripped before commit, per standing discipline.

**Hypotheses to confirm/refute** (each with its BS3.3 fix):
- **H1 — publish I/O dominates**: N outputs × (write 64 MiB + fsync + rename + dirsync)
  serialized inside the build.
- **H2 — per-row merge overhead**: dyn `policy.decide` dispatch + per-row
  `TableCompactionRowContext` construction + `previous_kept_key` clone per kept row.
- **H3 — encode/CRC**: block build + checksum in `finish_current`.
- **H4 — k-way merge**: `MERGE_HEAP_THRESHOLD = 4` pushes typical 5–9-source L0→L1 merges
  onto the `BinaryHeap`; linear selection may win at these widths.

**Exit of the slice:** a decomposition table (ms and % per stage at ~300 MB and at a larger
synthetic compaction) + the ranked fix list for BS3.3. No production code change.

#### Measured decomposition (BS3.2 complete)

Driver: `benchmarks/src/bin/storage_next_l0_compact.rs` (committed; public API only, durable-local).
The runtime auto-compacts L0 at `LEVEL_ZERO_COMPACTION_THRESHOLD` (4) and the frozen byte budget
blocks a 2nd un-flushed rotation, so the public-API ceiling for a *single* L0→L1 is ~3×~47 MiB L0
tables (~142 MB) — not the 300 MB the plan assumed. This **under-states** publish: at 300 MB / 5
tables the merge scales with bytes but the publish syscalls + reader rebuild scale with the
*output-table count*, so the publish share only grows. Two operating points vary row-density at equal
bytes to separate per-row from per-byte cost (this dev box, temporary `STRATA_TRACE` stage probes now
stripped; coarse split from committed `perf-trace` timers, reproducible via the bin):

```
single durable L0→L1, lifecycle-rewrite elapsed (plan + merge + per-artifact publish):

                          arm A  1 KB / 129k rows      arm B  8 KB / 18k rows
  merge loop                 150 ms  (36%)                129 ms  (34%)
  plan + finish + publish    260 ms  (63%)                241 ms  (65%)   <- dominates
  ----------------------------------------------------------------------
  lifecycle elapsed          410 ms  (~350 MB/s)          370 ms  (~386 MB/s)

  full attribution (arm A, ~410 ms, summed over the 4 output tables; publish stages
  probed at both the syscall and publish-step granularity — now ~96% attributed):
    merge loop                              150 ms  (36%)
    publish_io  write+temp_fsync+dir_fsync  108 ms  (26%)   [write ~20, temp_fsync ~63, dir_fsync ~25]
    byte_validate  re-read + full memcmp     67 ms  (16%)   <- redundant; CONFIRMED (see below)
    reader_handoff  build in-memory reader   67 ms  (16%)   <- byte-bound; not in H1–H4
    budget + plan + final_finish + catalog  ~18 ms  ( 4%)
```

Key facts the profile establishes:
- **Publish, not merge, dominates** (~63% vs ~36%). Within `publish_io` the *durability syscalls*
  (`temp_fsync` ~63 + `dir_fsync` ~25 ≈ 88 ms) dwarf the actual data write (~20 ms) — NVMe sequential
  write is fast; the per-output-table fsyncs are the cost. **H1 confirmed.**
- **`byte_validate` (~67 ms) is a redundant re-read.** `publish_or_load_rewrite_output` returns
  `exact_bytes_validated = false` on the Create success path (`rewrite_publication.rs:853`), so
  `publish_rewrite_artifact` then calls `require_exact_bytes`, which **re-reads the entire
  just-written+fsynced object from the backend and full-`memcmp`s it** against the in-memory bytes
  (`service/table.rs:759`, `read_all_table_object_for_exact_match`). A whole extra read pass over
  every output table (~184 MB here) to re-verify bytes we hold in memory and already fsynced.
- **The in-memory reader build (`open_reader_from_validated_rows`, ~67 ms) is as large as the
  fsyncs** and is **byte-bound** (~21 ms/46 MB in *both* arms despite 6.6× fewer rows in B). The rows
  are *already* reused (`record_table_rewrite_reader_rows_reused`), so the cost is building the
  reader's in-memory index/filter structure, not re-parsing rows. Not in the plan's H1–H4.
- **H2 refuted:** merge is byte-bound, not row-bound — a 7× row-density drop (A→B) moved merge share
  by ~2 pts and `row_clones = 0`; `policy.decide` is static dispatch (monomorphized), no vtable.
- **H3 refuted as a separate stage:** block encode + per-block CRC is incremental inside
  `builder.append` (already inside the 36% merge); the trailing `finish_current` is ~0.5 ms.
- **H4 deprioritized:** the merge loop is the minority cost (~36%) and byte-bound.

#### Ranked fix list for BS3.3

> **Status: BS3.3 paused.** A prototype of fix #1 (byte_validate elision) confirmed the +16% win
> (A/B: 401→347 ms, 354→410 MB/s, merge unchanged) but also disproved its "redundant / lowest-risk"
> framing — see the correction on #1. Decision (owner): keep both integrity checks for now; do not
> take on the H1b Backend-trait + fault-sweep work yet; move to BS3.4 (graceful admission, the
> primary A/F-crawl exit lever) and revisit publish efficiency later.

1. **~~Elide the `byte_validate` re-read~~ — NOT redundant; do not elide lightly** (~67 ms ≈ 16%,
   measured). Correction: the Create-path `require_exact_bytes` is the compaction path's **only
   backend-write-integrity check**, not belt-and-suspenders — it re-reads the durable object and
   catches a backend that corrupts on write. It is *deliberately* tested: 8 tests
   (`lifecycle/tests/compaction/publication_plan.rs` + `remaining.rs`,
   `corrupt_table_object_create_on_call`) assert it raises `rewrite_publication_orphan` with no
   install. The reader can't stand in (it serves the correct in-memory bytes and never reads the
   backend); there is no cheaper equivalent (any CRC check still re-reads the whole object). Eliding
   it is a **durability-posture change** — corruption then surfaces only on a later reopen (CRC-on-open),
   not at publish — and requires rewriting those 8 tests. Viable (RocksDB's `paranoid_file_checks` is
   off by default) but a deliberate safety reduction, not a free win.
2. **H1b — batch the durability syscalls** (~88 ms ≈ 21%; biggest raw lever, safety-PRESERVING —
   keeps all corruption detection, just batches fsyncs): one `sync_all` pass + one parent-dir fsync
   per compaction instead of per output table saves ~(N−1)/N of both. Larger implementation: extend
   the `Backend` trait with staged-publish primitives, add `BackendOperation` variants so the STH-2
   fault sweep still covers the new syscalls, and rewrite the 5 per-step publish-fault tests. Gated by
   the recovery oracle + fault sweep (objects unreferenced until the manifest fsync). Win *grows* at
   the production 5-table size. **The safety-preserving alternative to #1.**
3. **Defer the reader build** (~67 ms ≈ 16%; new — not in H1–H4): NOTE — not cleanly deferrable as
   first hoped. The 67 ms is the eager SHA-256 content digest (`table_content_digest`), not the
   reader structure (the bloom is already `eager_filter_unavailable`); the digest is needed later to
   bind a filter (`matches_exact_content`) and the eager reader doesn't retain the bytes to recompute
   it. Deferring would force retaining ~46 MB×N resident or weakening the filter-content guard.
   Revisit only with a redesign of how the content fingerprint is carried.
4. **H1a — pipeline publish with merge** (~20 ms data write): lower priority; the write is already
   cheap, so overlap buys little until the fsyncs (H1b) are batched.
5. **Merge internals (H2/H3/H4): deprioritized** — refuted or minority; revisit only if the above
   don't reach the throughput target.

### BS3.3 — Pipeline efficiency fixes (profile-driven)

Implement the top offenders from BS3.2; each fix gets its own control-first A/B (compaction
wall-time per input MB is the metric; load/crawl cells as secondary). Pre-scoped candidates:

- **H1 fixes:** (a) pipeline publish with merge — publish artifact *k* while merging *k+1*
  (the streaming builder already emits artifacts incrementally); (b) batch the durability
  syscalls across one compaction's outputs: write all temps → one `sync_all` pass → renames
  → **one** parent-dir fsync. Crash-safety argument: output objects are unreferenced until
  the manifest publishes (the manifest fsync is the barrier; torn temps are cleanup
  candidates) — **gated by the recovery oracle + fault sweep**, which exercise
  publish-window crashes; (c) drop the two per-table budget-ledger locks to one reservation
  per compaction.
- **H2 fixes:** devirtualize the policy (enum or generic dispatch), hoist per-row context
  construction, reuse the `previous_kept_key` buffer (swap instead of clone).
- **H3 fixes:** encode-buffer reuse; verify crc32fast SIMD path engages.
- **H4 fix:** raise the linear-selection threshold past typical L0→L1 source counts;
  re-measure.

**Tests.** No behavior change permitted: compaction output byte-identity tests (same inputs
→ identical artifacts pre/post fix, reusing the reunion-test comparison machinery); full
suite; fault sweep specifically for the H1(b) sync-batching change.

### BS3.4 — Graceful admission (grades + adaptive rate)

> **BS3.4a landed:** change 1 (the regrade) shipped — L0 urgent 8→20, block 16→36
> (`compaction.rs:34-35`). The quadratic throttle band auto-widened (its L0 fullness denominator is
> the block threshold, now ÷36), the grade-boundary / block-wait / rejection-precedence fixtures moved
> to 20/36 across 5 test files (`lifecycle/tests/compaction/{mod,remaining}.rs`, `lifecycle/tests/durable.rs`,
> `api/tests/background.rs`), and a **C3 profile-tier matrix** (`lifecycle/tests/budget.rs`,
> `profile_tier_byte_pools_bound_write_memory_independent_of_l0_count_grade`) proves the active/frozen
> byte pools bound write-path memory before the count grade at 512 MB / 8 GB / 48 GB. The
> debt-adaptive rate ramp (change 2) is **BS3.4b**.

**Changes.**

1. **Regrade L0 admission** (`lifecycle/compaction.rs:33-35`):
   `LEVEL_ZERO_COMPACTION_THRESHOLD = 4` (unchanged — scheduling trigger),
   **urgent/delay 8 → 20**, **blocking/stop 16 → 36** (RocksDB grades). Frozen thresholds
   unchanged (diagnostic showed zero frozen pacing). Non-zero-level table grades unchanged
   in BS3 (their rejects were secondary — 14 after A.3); revisit with BS4 data.
   Note the ceilings this implies: 36 × 64 MiB ≈ 2.3 GiB resident L0 worst-case (fine at
   the benchmark budget; small-budget configs remain bounded by the existing byte-pressure
   paths, which are unchanged).
2. **Debt-adaptive write rate (the RocksDB `SetupDelay` port), replacing the quadratic
   P-controller as the delay-band mechanism:**
   - State (on the durable runtime, beside `last_write_admission`):
     `current_write_rate: AtomicU64` (bytes/s), `last_debt: u64`, token-bucket
     `{credit_bytes, next_refill}`.
   - **Updated at event cadence, not per commit** (RocksDB recomputes in
     `InstallSuperVersion`): in the BS1 event hooks (rotation, flush install, compaction
     install), when the delay grade is active: debt = Σ max(0, cached level bytes − target)
     + L0-count pressure; rate ×= 0.8 if debt ≥ last_debt, ×= 1.25 if shrinking (cap
     max_rate), ×= 0.6 if near-stop (L0 ≥ stop−2 or debt in the top quarter of the
     soft→hard band); floor 16 KB/s; on return to normal, rate ×= 1.4 toward max.
   - **Per commit (O(1)):** if the delay grade is active, token-bucket delay =
     `bytes_over_credit / rate` (min 1 ms), applied where the throttle already sleeps
     off-lock (`background_wait_after_write_throttle`, `mod.rs:3051`). Defaults: max rate
     16 MB/s (RocksDB's), configurable via `LifecycleWriteThrottlePolicy` (extended).
   - The **stop** grade keeps the existing retry-wait loop (`mod.rs:2850`) unchanged — it
     is already the RocksDB hard-stop analog (wait for background progress, stall
     watchdog) — it just fires at 36 instead of 16.
   - A/B gate during validation: `STRATA_ADMISSION={legacy|graded}` env hook, baked to
     `graded` and removed before milestone close.
   - **Scope (constraint C2):** the regrade and the rate ramp apply to the **durable path
     only**. Cache mode keeps its neutralized throttle (`mod.rs:2643`) and its existing
     pressure semantics unchanged — verified by the cache-mode suites running unmodified.
   - **Timing (constraint C1):** the token bucket and ramp read time through the existing
     `MaintenanceClock` abstraction, not raw `Instant::now` — wasm-safe and deterministic
     for the state-machine tests (`ManualMaintenanceClock` drives them).
   - **Profiles (constraint C3):** validate the grades at small budgets — at embedded tiers
     the frozen/active **byte**-pressure paths must remain the binding constraint before
     the L0 **count** grades (36 tables × small rotation sizes must never exceed the tier's
     byte budget unchecked). Add a profile-tier threshold matrix test (512 MB / 8 GB /
     48 GB) asserting which constraint binds first per tier.

**Tests.**
- **Unit — the ramp state machine** (pure function): decay sequence under flat/growing
  debt (geometric ×0.8); recovery ×1.25 capped at max; near-stop ×0.6; floor clamp;
  return-to-normal ×1.4. Table-driven.
- **Unit — token bucket**: credit accrual, overage delay math, 1 ms minimum, refill
  granularity.
- **Unit — grade thresholds**: severity mapping at L0 = 4/19/20/35/36 boundaries.
- **Event-cadence test**: rate changes only at install/rotation events, never from a
  commit alone.
- **Behavioral**: drive L0 from 0 → 36 under a paused compaction lane (deterministic
  executor): commits proceed un-delayed <20, delayed with growing pacing 20–35, rejected
  (retryable) at 36; resume compaction → rate recovers ×1.25/×1.4 and pacing releases.
- **Existing-test sweep**: tests that construct 16-table L0s to hit the old blocking
  threshold need their constants updated (they assert error class/code, not thresholds —
  the survey rule — but their fixtures encode 16); enumerate via the
  `LEVEL_ZERO_BLOCKING` grep before starting.

## Perf validation (milestone exit)

Control = BS2-final binary, treatment = BS3-final; standard methodology; env hooks and
probes stripped before the closing commit.

1. **Primary (gate):** workload A and F @10 M run throughput **≥50 K ops/s** (from ~110)
   with **zero deep-crawls in n≥9** interleaved runs.
2. **Primary (gate):** L0 table count at steady state stays below the delay grade (20)
   under sustained workload-A load — measured via the compaction trace.
3. **Secondary:** sustained load within 25 % of burst load; aggregate compaction
   throughput ≥500 MB/s across lanes (the budget arithmetic); compaction wall-time per MB
   from BS3.3's A/Bs.
4. **No-regression:** load cells and read cells within noise; recovery oracle + fault
   sweep green (mandatory for the H1(b) sync-batching change).
5. Ledger row per slice + milestone scoreboard re-run.

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** G23 fixed in BS3.1 (thread::scope cfg-gated, serial fallback); the rate
  ramp/token bucket use `MaintenanceClock`, no raw time; pipeline-publish parallelism in
  BS3.3 (H1a), if built, goes behind the same non-wasm gate; wasm check-build in every
  slice's gates.
- **C2 (cache mode):** regrade + ramp are durable-scoped; cache-mode pressure, thresholds,
  and the neutralized throttle are unchanged; cache suites run unmodified as a gate.
- **C3 (profiles):** the profile-tier threshold matrix (BS3.4) asserts byte-pressure binds
  before count grades at embedded budgets; no budget-dependent code forks.
- **C4 (branching):** compaction remains per-branch (cross-branch refs rejected — existing
  invariant); the pruning-proof and merge changes in BS3.2/3.3 are content-neutral
  (byte-identity tests); no fork-path interaction.

## Risks

| Risk | Mitigation |
|---|---|
| Profile implicates format-encode CPU (H3) with no cheap fix | acceptable finding — feeds BS6 (block sizing/compression trade); the admission work (BS3.4) still delivers the graceful-degradation exit criteria |
| Sync-batching (H1b) weakens crash safety | explicit crash-consistency argument (objects unreferenced until manifest fsync) + recovery oracle + fault sweep as the hard gate; revert if any sweep position fails |
| Higher L0 stop (36) raises read amplification during bursts | blooms cut absent-key probes; BS2 makes reads lock-free; measured in validation (read-cell no-regression gate); the *delay* band exists precisely to keep steady-state L0 low |
| Rate-ramp oscillation (over-throttle ↔ over-release) | RocksDB's constants adopted verbatim first (decade-tuned); event-cadence updates prevent per-commit racing to the floor; ramp state machine is a pure function with table-driven tests |
| Threshold changes ripple through test fixtures | pre-enumerated fixture sweep in BS3.4; tests assert codes/classes so semantic asserts survive |

## Sequencing & PR discipline

BS3.1 → BS3.2 (profile gate) → BS3.3 (top fixes only) → BS3.4, one PR per slice,
`BS3.{n}` titles, ≤1 500 LOC net, standing gates every slice. BS3.2's decomposition table
is a committed artifact (ledger + doc update) even though it ships no production code.
Depends on BS1 (debt signal, cheap retries); the umbrella sequencing puts BS2 first so the
A/F exit numbers are measured with reads off-lock.

## Open items

- Non-zero-level admission grades and a true `estimated_compaction_needed_bytes` soft/hard
  byte pair (RocksDB's 64 GB/256 GB analogs, scaled to budget) — revisit with BS4 data
  when compaction becomes I/O-bound and level shapes change.
- Whether the delay band should also pace the *load* phase (pure inserts) or only
  update-in-place workloads — decide from BS3.4's behavioral tests (RocksDB paces both).
- Subcompactions' honest re-test is **BS4's** re-baseline, not BS3's (recorded in
  `billion-scale-plan.md` G9).
