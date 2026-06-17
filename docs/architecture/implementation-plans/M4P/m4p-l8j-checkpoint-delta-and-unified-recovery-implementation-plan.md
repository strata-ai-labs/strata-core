# M4P-L8J Implementation Plan: Delta Checkpoint + Unified Recovery

Status: draft

Test plan: `docs/architecture/implementation-plans/M4P/m4p-l8j-checkpoint-delta-and-unified-recovery-test-plan.md`

## Objective

Make a durable checkpoint snapshot a **bounded delta** (the in-memory, not-yet-durable
rows) instead of a **full-database materialization**. This:

1. Eliminates a latent durability/availability landmine: the checkpoint can currently
   produce a snapshot larger than the 64 MiB decode ceiling, which is then unreadable
   (`checkpoint failed to publish snapshot: ... snapshot_materialized_payload length is
   invalid`).
2. Makes checkpoints O(in-memory backlog) instead of O(database size), so they are cheap
   and scale.
3. **Unblocks the real performance lever** (relaxing the over-aggressive admission
   throttle — see the perf investigation below): the throttle currently masks this bug by
   keeping the backlog small enough that checkpoints never fire; a relaxed throttle lets
   the backlog grow, which forces a checkpoint, which crashes. The checkpoint must handle
   large state before the throttle can be safely relaxed.

## Root-cause evidence (from the M4P load-performance investigation)

- `BranchLocalState::checkpoint_rows` (`branch/state/snapshot.rs:298-311`) materializes
  **every live row** — active memtable + all frozen tables + **all owned levels L0–L7** —
  up to the watermark into snapshot sections. The owned-level rows are already durable in
  table objects referenced by the table manifest, so materializing them again is redundant
  and unbounded.
- The format caps a materialized snapshot payload at `MAX_MATERIALIZED_SNAPSHOT_PAYLOAD_BYTES`
  = 64 MiB on decode (`format/snapshot.rs`, per-section and cumulative). Encode now enforces
  the same ceiling (landed in this milestone — see "Already landed"), so an oversized
  snapshot fails fast at encode rather than being written and failing on read-back.
- The path is **dormant in normal operation**: baseline 10M durable load has
  `checkpoint_executions == 0` (checkpoints are coalesced; WAL truncation is driven by the
  flush-watermark, not by checkpoint snapshots). It only fires when the flush-watermark
  stalls (a large un-flushed backlog) and WAL growth forces a checkpoint — which is exactly
  what happens once the writer is allowed to outpace maintenance.
- **Recovery currently requires the snapshot to be a full superset.** `recovery.rs:123-172`
  selects one of two modes:
  - Mode 1 (`flush_watermark > checkpoint_watermark`): owned levels come from the manifest
    (`apply_table_manifest_recovery`). A delta snapshot is safe here.
  - Mode 2 (`checkpoint_watermark >= flush_watermark` — the **common** case immediately
    after a checkpoint): the comment states *"the checkpoint is authoritative ... the staged
    manifest stays staged ... the rows are not promoted into the branch state — the
    checkpoint is a superset"*. It sets `branch_state = recovered_branch` (snapshot rows
    only) and discards the manifest's owned-level rows.
  - The no-manifest path (`recovery.rs:173`) also rebuilds the branch solely from the
    snapshot.

  Therefore **excluding owned levels from the snapshot is unsafe without first reworking
  recovery** to always reconstruct owned levels from the manifest. This plan does both,
  coordinated.

## Coverage argument (why the delta is correct)

At checkpoint time the branch state partitions cleanly:

- Rows in durable owned-level table objects (recorded in the table manifest) — these are the
  flushed rows.
- Rows in the active memtable + frozen tables — the not-yet-flushed rows, durable only via
  the WAL.

A row is in exactly one partition (rotate moves active→frozen; flush moves frozen→owned and
removes it from frozen). So:

`manifest owned levels (flushed)  ⊎  checkpoint delta (active+frozen)  =  full state at checkpoint_watermark`

and WAL replay from `checkpoint_watermark` covers everything after. The union is gap-free and
overlap-free, so a delta snapshot plus the manifest plus WAL replay reconstructs the exact
state — the redundant owned-level copy in today's snapshot adds nothing recovery can't get
from the manifest. WAL truncation up to `checkpoint_watermark` stays safe because the data is
covered by manifest + (durable) snapshot before truncation, exactly as today.

## Already landed (defense-in-depth)

**Encode-time materialization ceiling** (`format/snapshot.rs`): `encode_snapshot_section` /
`encode_snapshot_container` now reject payloads exceeding the 64 MiB decode ceiling via
`*_with_payload_limit` / `*_with_materialized_limits`, returning
`FormatError::InvalidLength { field: "snapshot_materialized_payload" }`. Encode and decode are
now symmetric: a snapshot that cannot be read back is never written. Tests:
`encode_section_rejects_materialized_payload_over_limit`,
`encode_container_rejects_materialized_payload_over_limit`. This converts the failure into a
clean typed error but does **not** by itself let large databases checkpoint — that is the
delta rework below.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
| --- | --- | --- |
| J0 (landed) | Encode-time 64 MiB ceiling, symmetric with decode. | Oversized snapshot fails fast at encode; goldens unchanged. |
| J1 (landed) | **Unified recovery.** `recover()` now COMBINES via a 4-arm `match (recovered_branch, table_manifest_stage)`: the manifest is the durable base (owned L0–L7 + timestamp coverage) and the checkpoint **delta** (rows the manifest does not cover, by internal key — new `checkpoint_delta_rows`) is appended into the active memtable via `append_committed_rows_atomically`; WAL replay from `max(checkpoint_wm, flush_wm)` is unchanged. Deleted the `require_table_manifest_covers_checkpoint_rows` superset invariant (def + import + re-export); kept `preflight_table_manifest_with_checkpoint` (exact-byte dup OK, divergent fails). Files: `lifecycle/recovery.rs`, `lifecycle/table_manifest.rs`, `lifecycle/mod.rs`. | **MET.** Behavior-preserving on full-superset snapshots (Mode 1: delta empty; Mode 2: manifest + delta, logically identical). Verified: fmt + clippy `-D warnings`; lib suite 3226 passed (incl. recovery/manifest/flush/checkpoint 215); `crash_recovery` 11, `lifecycle_recovery` 10, `lifecycle_faults` 20 all green; strengthened disjoint-combine test asserts both manifest and checkpoint rows visible. |
| J2 | **Delta checkpoint.** `checkpoint_rows` materializes active + frozen only (drop the owned-levels loop, `snapshot.rs:307-311`). Adjust checkpoint watermark/extension bookkeeping accordingly. Regenerate snapshot content golden vectors (format structure unchanged; section *content* changes) with a documented rationale. | Checkpoint of a >64 MiB database succeeds and is decodable; snapshot size scales with in-memory backlog, not DB size. |
| J3 | **Crash-consistency + scale validation.** Crash points across {snapshot publish, WAL truncation}; recover == synchronous baseline. Force a checkpoint at 10M (large backlog) and recover. | No data loss across crash windows; 10M forced-checkpoint load + recovery passes. |

Order: J1 before J2 (recovery must handle the delta before the checkpoint produces one). J0
is already in. J3 gates closeout.

## Implementation detail

### J1 — Unified recovery (`lifecycle/recovery.rs`)
- Replace the `use_table_manifest_as_base` branch (123-127) and the Mode 1 / Mode 2 split
  (135-172) with a single path:
  1. `apply_table_manifest_recovery(stage)` always (owned levels from the manifest), when a
     stage is present.
  2. Install the checkpoint delta rows (active+frozen) on top of the manifest-recovered
     branch (extend, do not replace).
  3. Replay WAL from `trusted_replay_start(checkpoint_wm, flush_wm)` (unchanged).
- The combined preflight (`preflight_table_manifest_with_checkpoint`) keeps the
  exact-duplicate-bytes-accepted / divergent-bytes-rejected rule, but the expectation
  changes from "checkpoint is a superset of the manifest" to "checkpoint and manifest are
  **disjoint by commit range** (manifest ≤ flush_wm < snapshot)". Adjust
  `require_table_manifest_covers_checkpoint_rows` to the disjoint invariant.
- Keep the retained-history / timestamp-coverage application (today at 160-164).

### J2 — Delta checkpoint (`branch/state/snapshot.rs`)
- `checkpoint_rows`: remove the `owned_levels.iter().flatten()` loop (307-311); materialize
  only `active` + `frozen`. The watermark filter (`push_checkpoint_row`,
  `commit_version() <= watermark`) is retained.
- Audit callers of `checkpoint_rows` (and any "snapshot is a full superset" assumption) and
  update. `fork_snapshot_rows` (snapshot.rs:318) is a separate path — confirm whether the
  same delta logic applies to fork artifacts or whether fork must stay full (fork crosses
  branch boundaries and may not have a shared manifest base — decide explicitly).
- Snapshot golden vectors: format bytes structure is unchanged, but a checkpoint's section
  content changes (delta vs full). Regenerate and document; the durable **format** is not
  changing, only what the engine chooses to put in a checkpoint.

### Constraints
- No pre-V1 migration: V1 is unreleased, so dev databases written with full-superset
  snapshots may be discarded; J1 should still handle a full-superset snapshot gracefully
  (it is a strict superset of the delta + manifest, so the disjoint preflight must tolerate
  duplicate owned-level rows that also appear in the manifest — accept exact-byte
  duplicates, which the existing preflight already does).
- `#![deny(unsafe_code)]`, error class/code assertions (not text), one canonical path.

## Exit gate
- A durable database whose live state exceeds 64 MiB checkpoints successfully and recovers
  to a byte-identical state.
- Recovery is correct from both full-superset (legacy in-tree) and delta snapshots.
- Crash-consistency suite green across snapshot-publish / WAL-truncation windows.
- Format golden vectors regenerated with rationale; no durable format-structure change.
- Unblocks M4P throttle relaxation (separate slice): with checkpoints bounded, the admission
  throttle can be relaxed without risking the checkpoint crash.

## Relationship to the rest of M4P (perf)
This is the **durability blocker** in the load-performance chain. The full chain, per the
scale-validated investigation:

1. **Admission throttle (perf root cause, 4.5–7.3× lever)** — over-aggressive; slows the
   writer on normal pressure and drives a flush-fragmentation feedback loop. Relaxing it is
   the headline win but is gated by this checkpoint rework (and by keeping the backlog
   bounded).
2. **This slice (L8J)** — make checkpoints handle large state so the throttle can be relaxed.
3. **Off-lock publish (L8I Group C)** — the secondary, scale-only residual (the runtime-lock
   contention that remains at 5M+ even with the throttle off).
