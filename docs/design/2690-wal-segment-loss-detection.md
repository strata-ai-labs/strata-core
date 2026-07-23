# WAL segment-loss detection (#2690)

**Status:** design — open for deliberation, not yet implemented on `main`.
**Severity:** S1 (silent, undetected data loss that self-reports HEALTHY). Part of the #2686 adversarial-defect line. Enabling prerequisite #2699 (open-error classification) merged as #2713.

---

## 1. The bug

Removing a single WAL segment file discards data (events, KV, branches). The database still opens cleanly, self-reports `healthy`, and `verify_chain()` returns `valid=true` on the now-empty log; the next append restarts at sequence 0.

**Live repro:** 2 events + 2 KV keys present → `rm` the sole WAL segment → reopen reports `count=0`, `verify-chain valid=true length=0`, `health=healthy`, `doctor issues=[]`, plus a freshly recreated 36-byte header-only segment.

Byte-level forgery is already well defended (`deny_unknown_fields`, no delete/truncate command, on-disk byte mutations correctly fail `open()`). The gap is **removal**, not mutation: an actor who can `rm` one file wipes the log with no evidence.

### Root cause (confirmed by code-trace)

`crates/storage/src/service/wal.rs`:
- `open_or_create_segment` (~1949): a `NotFound` on the resolved active segment is treated as "fresh store" → `create_segment` silently creates an empty segment.
- `resolve_resume_segment` (~2060): resolves `max(on_disk_max, manifest.active_wal_segment())`, so an empty WAL directory resolves back to the manifest seed.
- `list_segments` (~2074): globs the WAL prefix, sorts by id, **no contiguity check**.

`crates/storage/src/lifecycle/durable.rs::assemble`: the only manifest↔WAL reconciliation checks the `>` direction (writer resumed past a stale manifest pointer — expected post-checkpoint) and WARN-logs. The `<` direction (an expected segment is *absent*) is structurally unreachable.

`verify_chain` passes vacuously because the event `next_sequence` metadata is itself WAL-derived and resets with the log.

### Why "detectable in principle" is only half-true

- A deleted **middle** segment leaves a contiguity gap → detectable by inspecting the on-disk set.
- A deleted **sole or tail** segment is only *weakly* detectable: the manifest's `active_wal_segment` deliberately lags (it is checkpoint-published), and **no durable upper bound on committed data is recorded**. The "erases ALL data" outcome specifically requires WAL-resident (un-checkpointed) data.

---

## 2. The core difficulty

To fail closed on a deleted segment we must answer, at open time: *"was there acknowledged data that this missing segment held?"* Three on-disk states are relevant, and two of them are **indistinguishable** without additional durable state:

| State on reopen | Correct behavior |
|---|---|
| Fresh DB, no manifest | Create segment 1, open empty |
| Manifest exists, **crash during creation** (manifest published, first segment not yet written), no data ever committed | Open empty (nothing lost) |
| Manifest exists, sole segment **deleted after commits** | **Refuse** (data lost) |

The last two present identically — `manifest exists, no checkpoint, active segment absent` — because the manifest is durably published *before* the first WAL segment is created. The only thing that separates them is a durable record that *acknowledged commits happened*. This is the crux.

This was proven empirically: a disposition-only check (fail closed when `OpenedExisting` + no checkpoint + active segment absent) passes the sole-deletion repro but **false-positives on crash-during-creation**, which `crates/storage/src/testkit/fault_sweep` exercises directly (faults land during open/init). Gating on "no checkpoint" (`flushed_through_commit_id().is_none() && snapshot_watermark().is_none()`) cleared 33 of 36 induced failures — all of them legitimate checkpoint-recovery scenarios where an absent WAL segment is fine because the snapshot holds the data — but the residual `fault_sweep` failures are the irreducible creation-window ambiguity.

**Conclusion:** correct detection of the sole/tail case requires a durable marker recording that committed data exists (and up to where). A disposition-only heuristic cannot be made correct.

---

## 3. Approaches considered

### A. Disposition-only (no format change) — rejected
Fail closed when the manifest exists but the active segment is absent and no checkpoint covers the data. **Fatal flaw:** indistinguishable from crash-during-creation (§2). Also misses tail deletion entirely (a lagging manifest pointer can't tell segment N ever existed). Caught only sole + middle; broke crash recovery.

### B. Creation-order reorder (no format change)
Create and sync the first WAL segment *before* publishing the manifest, establishing the invariant "manifest present ⟹ segment was durably created." Then an absent segment on an existing DB is unambiguously deletion.
- **Pro:** no on-disk format change.
- **Con:** restructures the delicate `assemble` creation sequence (blast radius: operation-order tests, crash-recovery paths). Still only catches sole + middle — the **tail** case needs a per-segment durable record regardless, so this doesn't reach "full."

### C. Durable watermark (format change) — **selected**
Record durable evidence of the highest WAL segment that has been created. Distinguishes all cases and catches sole + middle + tail. The format is pre-release, so the change is cheap now and expensive later. Details in §4.

### Sub-decision: where the watermark lives

- **Manifest field** (`highest_wal_segment` in `DatabaseManifest`): idiomatic (the manifest already holds `active_wal_segment` and recovery facts). **But** it forces either `WalService` to write the manifest (cross-service coupling) or the lifecycle to update it asynchronously (a lagging marker that misses recent tail). The "two manifest writers" (checkpoint + rotation) is the coordination cost.
- **WAL-owned watermark object** (selected): `WalService` writes a small dedicated object directly via its own backend at segment creation. Self-contained, no cross-service coupling, synchronous at rotation. Cost: a new durable object + format + golden vector.

---

## 4. Selected design — WAL-owned durable watermark

### Object + format
A new object (`ObjectLayout::wal_watermark()`) holding the highest segment id ever durably created. Small CRC'd format (`format/wal_watermark.rs`): `magic | version | u64 highest_segment | crc`. Modeled on `format/watermark.rs` but **with a CRC**, because it drives a data-loss decision and a torn write must be detectable.

### Write path — at segment **creation**
`WalService::create_segment(N)` (called by `open_or_create_segment` for the initial segment and by `rotate_segment` on roll): after the segment object is created **and synced**, write **and sync** `watermark = N` (monotonic max).

Writing it **at creation, before any commit** is the key that resolves the creation-window ambiguity:
- Crash *before* the watermark is synced → watermark **absent** (or the previous value); the segment may or may not exist, but there is never "watermark says N while N is gone." Reopen treats it as fresh / resumes past → **no false positive**.
- Only an **external deletion** *after* the watermark is durable produces "watermark says N, but N is absent" → **refuse**.

**Ordering guarantee:** `segment create+sync` → `watermark write+sync`. A crash between them leaves the watermark a **safe lower bound** (`< true highest`); recovery resumes at the higher on-disk segment via the existing `resolve_resume_segment` path. The watermark never over-claims.

### Open path
`verify_wal_segment_inventory(backend)` reads the watermark:
- **Present, `= N`:** require on-disk segments contiguous from the lowest present id up to `N`, and `N` present. Otherwise `MissingActiveSegment` / `SegmentInventoryGap`. (Retention only trims a contiguous prefix, so a hole in the middle, or an absent `N`, is out-of-band removal.)
- **Absent:** fresh DB or pre-first-watermark crash → allow `create_segment`.
- **Corrupt / torn decode:** treat as absent + log a warning (degrade to non-detection rather than false-positive). *(Open question — see §7.)*

The check is authoritative on the watermark, replacing the disposition/checkpoint gating from Approach A.

### Failure classification
A removed/gapped segment surfaces as `WalServiceError::{MissingActiveSegment, SegmentInventoryGap}`, added to `WalServiceError::is_durable_corruption()`, so #2699's `wal_open_error` maps it to `RecoveryCorruption` → `StorageApiError::RecoveryDegraded` → engine `corruption.engine.persistence_recovery` / `RetryPolicy::Never`. Refuse-to-open, non-retryable.

### Known limitation (accepted)
A crash in the narrow window *after* creating segment N but *before* syncing `watermark = N` leaves `watermark = N-1`; a subsequent deletion of N (above the watermark) is not caught. This is inherent to any non-per-commit marker and is a small, bounded window. Per-commit durability of the watermark would close it but defeats WAL batching.

---

## 5. What each approach catches

| Deletion scenario | A: disposition | B: reorder | C: watermark (selected) |
|---|---|---|---|
| Sole segment (the S1 "erases ALL data" repro) | ✅ (but breaks crash-recovery) | ✅ | ✅ |
| Middle segment (gap) | ✅ | ✅ | ✅ |
| Tail segment on a multi-segment DB | ❌ | ❌ | ✅ (except the §4 window) |
| Crash during creation (no false positive) | ❌ | ✅ | ✅ |

---

## 6. Current implementation state (branch `fix/issue-2690`, not on `main`)

Committed as a labeled WIP (do **not** merge):
- `WalServiceError::{MissingActiveSegment, SegmentInventoryGap}` + Display + `is_durable_corruption()` classification. **Keep.**
- `verify_wal_segment_inventory` contiguity/presence skeleton. **Keep; rework to read the watermark.**
- Engine reproduction test `deleting_the_sole_wal_segment_refuses_to_open_instead_of_silently_erasing` (asserts `corruption.engine.persistence_recovery` / Never). **Keep.**
- Unit test `segment_inventory_check_catches_removed_and_gapped_segments`. **Keep; rework to watermark-driven.**
- **Superseded:** the disposition/checkpoint gating in `durable.rs` — to be replaced by unconditional watermark-driven verification.

Remaining: `format/wal_watermark.rs` + mod wiring + tests; `ObjectLayout::wal_watermark`; the `create_segment` write path; open-time verify rework; golden vector (`testdata/goldens/storage-format-v1/wal-watermark-*.hex`) + `format_golden.rs` registration; tail-deletion test; green `fault_sweep`/`durable`/full workspace; invariant-check (ARCH-004 / ACID-005 — recovery), code-review, clippy/fmt, mutation-on-diff; PR + CI.

---

## 6b. Implementation finding — the checkpoint interaction (RESOLVED by the commit-version pivot)

**Resolution (implemented):** the watermark now records the durable **highest
committed version** (`meta/wal-watermark`, STWW, CRC-guarded), published at
seal points only — segment rotation and close, strictly AFTER the sync that
made the attested records durable — and monotonic. Recovery compares it
against every recoverable source: `max(checkpoint watermark, table-manifest
flush watermark, replay start, max surviving WAL record version)`. Strict
recovery refuses (`RecoveryCorruption` → `corruption.engine.persistence_recovery`,
non-retryable) when the marker exceeds all of them; lossy recovery records a
`WalCommittedSuffixMissing` fault and continues. The segment-id inventory
check is retained for interior holes only (id-contiguity — checkpoint-free by
construction, since retention only trims a contiguous prefix).

The fault-simulation seed that falsified the segment-id design now passes: a
dropped segment whose data the snapshot covers satisfies the comparison, and
`deleting_checkpoint_covered_segment_still_opens_with_all_data` pins that
inverse permanently.

**Accepted limitations (documented, follow-up filed):**
1. **The active-tail window** (§4, unchanged): commits appended after the last
   seal point are not yet attested; deleting the active segment loses them
   undetected. Bounded by the rotation/close cadence.
2. **Prefix deletion of uncheckpointed data**: a sealed *lowest* segment whose
   versions sit above the checkpoint can be deleted without tripping the
   marker (later segments carry higher versions) or the contiguity check (the
   suffix stays contiguous). Detecting it needs a lowest-recoverable bound
   (min-present WAL version vs replay start), which is sound only if failed
   commits never burn version numbers — the allocator question is open.
   Reaching this state requires a size-threshold rotation without an
   intervening checkpoint.

The original finding is preserved below for the design record.

### Original finding (historical)

Implementing §4 and running the full storage suite surfaced a crash-consistency
gap the design did not anticipate. The `fault_simulation_sweep` (seed 3,
`SplitRename`, Standard) fails:

- `SplitRename` drops one published object at random. That seed also forces a
  checkpoint, so the committed data is safely in the snapshot. With the watermark
  now among the published objects, the seed drops a **WAL segment** whose data
  the snapshot already holds — a legitimate, recoverable state.
- The unconditional watermark verify refuses on the segment's **absence alone**,
  not knowing the data is checkpointed → **false positive** (`MissingActiveSegment`,
  which maps to the generic `RecoveryDegraded` "WAL segment failed to decode").

This contradicts §4's claim that the watermark "replaces the disposition/checkpoint
gating." A checkpointed database legitimately tolerates an absent segment (§2's
own observation), so the check must be checkpoint-aware. **But gating on the
manifest's checkpoint facts does not work either:** `SplitRename` can drop the
manifest update that recorded the checkpoint, so the reopened manifest claims
`snapshot_watermark = None` even though the snapshot object survives — the gate
then runs the verify and false-positives anyway (and gating breaks the sole-
deletion repro when a clean close records `flushed_through`).

**Root cause:** a bare highest-*segment-id* watermark is not checkpoint-comparable.
It cannot answer the only question that matters — "did the missing segment hold
data that is **not** otherwise durable (i.e. above the checkpoint)?" — so it
cannot separate *segment dropped but data checkpointed (safe)* from *segment
deleted with un-checkpointed data (loss)*.

**Direction (needs deliberation, ties to §7 Q7):** record a durable **highest
committed version** (not segment id), and refuse only when a missing segment
would have held commits **above the durable checkpoint/snapshot watermark**. That
is checkpoint-comparable and crash-robust (it does not depend on a manifest
update that a crash can drop). This is a design change, not a test fix.

Current WIP state: the watermark codec/object/write path and the sole-deletion
detection are implemented and green (engine repro + inventory unit test); the
`fault_simulation_sweep` fails on exactly this gap. Everything else in the
storage suite passes (3447/3448).

## 7. Open questions (for deliberation)

1. **Object vs manifest field.** The WAL-owned object gives clean layering and synchronous-at-rotation writes but adds a durable object + golden. Is the extra format surface worth avoiding the manifest coupling, or is a `highest_wal_segment` manifest field (with the lifecycle owning the write) preferable despite the two-writer coordination?
2. **The tail window (§4 limitation).** Is the "crash after segment-create, before watermark-sync" gap acceptable, or do we want a stronger guarantee (e.g. write the watermark *before* the segment and reconcile, or fold it into the same durable op as segment creation)?
3. **Corrupt watermark policy.** Degrade-and-log (proposed, avoids false positives) vs. fail-closed as corruption (stricter, but a torn watermark write during a legit crash could refuse a recoverable DB). Which bias do we want?
4. **Defense in depth.** Should we *also* do the creation-order reorder (Approach B) so the invariant holds even independent of the watermark?
5. **Repair path.** This slice refuses to open. Do we want an explicit "open the recoverable prefix" entrypoint (mirroring "WAL halts on fsync, recovery via explicit resume"), or is refuse-only sufficient for V1?
6. **Independent detection surfaces.** The issue notes the DB self-reports `healthy` and `verify_chain valid=true` on the erased log. Should `health` / `verify_chain` / `doctor` *also* surface the loss independently of open refusal — i.e. is open-refusal enough, or do we want the diagnostics to stop lying?
7. **Watermark granularity.** Highest *segment id* (proposed) vs. highest durable *commit version*. The commit version is checkpoint-comparable and slightly richer for diagnostics; the segment id is simpler and directly drives the inventory check.
8. **Retention floor.** Confirm the interaction: retention trims a contiguous prefix, so "contiguous from lowest-present up to watermark" is the right invariant. Is there any path where retention could delete a segment ≤ watermark legitimately (which would make the check false-positive)?

---

## 8. Invariants touched

Pure open-time detection + a new durable artifact; no change to what valid databases recover. Relevant: **ARCH-004** (one recovery model, deterministic ordering) and **ACID-005** (recovery replay idempotent) — both should *hold* (the change refuses on detected loss and adds a create-time durable write; it does not alter valid-DB replay). The new write path needs its own crash-consistency argument (§4 ordering).
