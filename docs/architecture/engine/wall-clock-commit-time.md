# Wall-clock commit time and time-based time travel

Status: design (approved for slicing) · Issue: #3112 · Companion invariants:
MVCC-007, the locked temporal contract, DUR-013, hard rule 13 (frozen format).

## Problem

Strata sells time travel (`as_of`) and versioning, which makes wall-clock time
the *engine's* responsibility, not the application's — SQLite can punt timestamps
to the app precisely because it has neither. A user cannot ask "as of last
Tuesday" of a store that only knows a logical counter.

Today the commit `timestamp` in write acks and read envelopes is the logical
`ApiTimestampSource` clock (seeded at `DEFAULT_TIMESTAMP = 1µs`, +1 per commit,
MVCC-007 floored). No durable wall-clock instant is recorded for any commit
(snapshot/checkpoint `created_at` reuse the *same logical* clock; only the event
capability records `SystemTime::now()`). So:

- A client reading `timestamp` as Unix micros renders `timestamp: 3` as
  1969-12-31 — every fresh database looks broken (the reported VS Code symptom).
- Time-based time travel is impossible: nothing maps a wall-clock instant to a
  commit.

## The model: two clocks per commit

The MVCC clock cannot *be* wall-clock — wall-clock regresses (NTP, cross-machine
skew) and MVCC ordering must never regress (MVCC-007); deterministic as-of/replay
rests on that. So, like Datomic (`t` vs `txInstant`) and git (monotonic DAG +
absolute committer epoch), we keep two clocks:

| Clock | Role | Monotonic? | Source |
|---|---|---|---|
| `version` / logical `timestamp` | ordering, visibility, diff, deterministic as-of | yes (MVCC-007) | `ApiTimestampSource` |
| `committed_at` | display + wall-clock as-of | no (best-effort) | injectable wall-clock at commit (**new**) |

## Locked decisions

- **D1 — `committed_at` is commit-scoped; clients resolve it for reads.** Stored
  once per commit (never stamped per row). Surfaced inline only where the subject
  *is* a commit: the **write ack** (`CommitReceipt`) and **`history`**. Plus a
  **batch `versions[] → committed_at[]` resolver**. `get`/`scan` row shapes are
  unchanged; the hot read path pays nothing for a datum most read callers never
  want. Rationale: the logical `timestamp` is intrinsic to a row's storage (free);
  `committed_at` is commit metadata requiring a timeline join (not free).
- **D2 — wall-clock `as_of` is the feature.** Additive input `as_of_time:
  Option<u64>` (UTC epoch micros), parallel to the existing logical `as_of`. NOT
  a tagged union — changing `as_of`'s wire shape would break 1.2 fixtures, the
  Python SDK, and docs. Rule: **reject if both `as_of` and `as_of_time` are
  given** (`invalid_argument`). Engine resolves `as_of_time` to a version via the
  timeline index; the existing deterministic as-of machinery then runs unchanged.
- **D3 — past-the-tip / before-first wall-clock target raises**, mirroring the
  locked temporal contract (out-of-window raises, after-latest does not clamp).
  This extends that contract to the wall-clock form; it does not amend it.
- **D4 — no `committed_at` on single-record reads** (batch resolver only);
  revisit only if a concrete "modified-times list" UI needs it.
- **D5 — pre-1.2 commits report `committed_at: null`/unknown, never 1969.**
  Honest absence; wall-clock as-of resolves only over commits that have instants.

### Wall-clock `as_of` resolution semantics

    resolve(target) = greatest version V such that
                      max(committed_at[first..=V]) <= target

The running-max over `committed_at` gives a monotonic *view* for binary search
even though raw `committed_at` can regress (skew). Documented as "the nearest
commit boundary at or before the instant," never exact. Target before the first
instant-bearing commit → raise; target after the tip → raise (D3).

## Determinism

Wall-clock is the enemy of golden vectors and DST replay, so contain it:

- `committed_at` comes from an **injectable source** (mirroring
  `CommitTimestampSource`), pinned by tests/replay via an explicit-committed_at
  policy analogous to the existing `CommitTimestampPolicy::Explicit`.
- Excluded from the deterministic replay/logical clock (the logical clock is
  untouched → MVCC-007 and the logical golden vectors are unaffected).
- Masked in IDL fixtures/examples (same treatment as the volatile fields in
  command-examples.json).

## Implementation map (grounded)

Durable commit record — each commit is one `WalRecord`
(`crates/storage/src/format/wal.rs:135`), built at
`crates/storage/src/commit/durable.rs:777`. `committed_at: u64` inserts at inner
offset 37; requires `WAL_RECORD_FORMAT_VERSION 1→2` and
`WAL_RECORD_MIN_LEN_AFTER_PREFIX 116→124` (`format/mod.rs:138/141`), a v1/v2
decode branch (`validate_wal_record_version`, `wal.rs:301`), threading through
`CommitStamp` (`commit/batch.rs:125/421`), the checkpoint snapshot timeline
section (`format/snapshot_timeline.rs`, `ENTRY_BYTES 16→24`), hand-updated golden
`.hex` vectors (`crates/storage/testdata/goldens/storage-format-v1/`), and the
normative spec (`docs/spec/strata-storage-format-v1.md` §10).

Timeline index (`crates/storage/src/timeline_index.rs`) — add `committed_at` to
`RetainedTimelineEntry` (line 27), a `committed_at_monotonic` flag mirroring
`timestamps_monotonic` (line 76), and `lookup_committed_at_at_or_before`
mirroring `lookup_at_or_before` (line 265, with scan fallback when
non-monotonic). `observe` (line 172) carries `committed_at`; recovery seeds it
from the snapshot timeline groups and the scan (`lifecycle/recovery.rs:935/975`,
`seed_from_scan` at `timeline_index.rs:207`).

as_of resolution — `resolve_read_bound` (`crates/storage/src/api/runtime/data.rs:338`),
add a `ReadBound::AtWallClock` arm using the new lookup; the out-of-window
contract lives in the `match lookup.miss()` at data.rs:361-388 (distinct
`reason` strings, no clamp). Engine: `ReadSelector::AtWallClock`
(`crates/engine/src/persistence/row.rs:74`), `storage_read_bound`
(`adapter.rs:1065`), error mapping (`adapter.rs:1224/1301`).

Clock seam — add a `CommitWallClockSource` mirroring `CommitTimestampSource`
(`crates/storage/src/commit/allocator.rs:49`), stored on `CommitFactAllocator`
(line 28), emitted in `allocate_for_batch` (line 220) into `CommitStamp`;
production impl reads wall-clock via `crates/engine/src/time_compat.rs`; the
runtime injection point is `default_timestamp_source`
(`api/runtime/mod.rs:3797`).

Wire — `CommitReceipt` (`crates/executor/src/types/common.rs:630`, built at
`executor/kv_json_convert.rs:76`), history items (kv `kv_branch.rs:465`, json
`json.rs:194`, vector `vector.rs:136`; builders in `kv_json_convert.rs:246/322`),
`as_of` inputs on ~30 command variants in `crates/executor/src/command.rs`
(mapped in the per-capability executor handlers). All carry
`schemars::JsonSchema` → IDL regeneration is part of every wire slice. Use the
`update-surfaces` runbook.

## Slice plan (epic under #3112)

Each slice is an independent PR referencing #3112; #3112 closes with the last.

1. **S1 — committed_at end-to-end, live only (no durable format change).**
   Injectable wall-clock source in the commit path → `CommitStamp` →
   `CommitOutcome` → `CommitReceipt.committed_at: Option<u64>`; timeline index
   carries `committed_at` in memory. Redocument logical `timestamp`; document
   `committed_at`. IDL regen. Fixes the write-ack "Dec 1969" symptom for the live
   session in both modes. No frozen-format change. *(Absorbs the logical-clock
   clarification originally scoped for #3112.)*
2. **S2 — durable persistence.** WAL v1→v2 + snapshot-timeline widen; recovery
   seeds `committed_at`; pre-1.2 v1 records decode to `committed_at = None` (D5);
   golden `.hex` regen + spec §10. The frozen-format slice — its own review.
3. **S3 — wall-clock `as_of`.** `as_of_time` input (reject-if-both),
   `ReadSelector::AtWallClock` → `ReadBound::AtWallClock` → committed_at lookup
   with the running-max view; past-tip/before-first raise (D3). IDL regen.
4. **S4 — history + batch resolver.** `committed_at` on history items; new
   `versions[] → committed_at[]` command. IDL regen.
5. **S5 — CLI/SDK/docs.** `--as-of-time`, display/localization guidance, temporal
   contract + wire contract docs; file the Python SDK tracking issue.

## Contract interactions

- MVCC-007 — logical clock unchanged; this design depends on it.
- Locked temporal contract — D3 extends it to the wall-clock form (raise, no
  clamp).
- Hard rule 13 — S2 is additive + version-gated with golden-vector regen.
- DUR-013 — timeline index gains the `committed_at` mapping.
