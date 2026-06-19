# STH-4 finding: power-loss recovery `Gap` under SplitRename (seed 155)

**Status:** root cause **proven** (2026-06-19) + captured as a standing `#[ignore]` regression (`regression_split_rename_power_loss_recovers_clean_prefix`). **The fix is a durable-invariant redesign**, not a surgical patch — see "Why a clean fix is a durable-invariant change". Class 9 stays open on it.
**Found by:** STH-4 fault-simulation soak (`fault_simulation_soak_deepens_across_many_seeds`) after the seed-74 publish-fault fix landed, at `STRATA_STORAGE_FAULT_CASES=3000`. The soak now clears seeds 0–154 (including 74) and fails at **seed 155**.
**Severity:** **high** (provisional) — recovery returns a *non-contiguous* committed history (`Gap`), which is a phantom-class violation, not a tolerated prefix loss.
**Relationship to the publish-fault fix:** **independent / pre-existing.** Seed 155 is a power-loss crash case (`run_one_crash_case`) on a reordering backend with **no injected backend fault**, so the checkpoint-defer fix (which only fires on table-manifest publish debt) is inert on this path. Fixing seed 74 merely let the soak run far enough to reach it.

## Symptom

```
fault-simulation power-loss violation [seed=155]:
  Gap { branch: BranchId([1;16]), missing_version: CommitVersion(3) }
```

Recovery recovered a committed history with a hole at `CommitVersion(3)` while (apparently) retaining later versions — a gap, not a clean truncated prefix.

## Deterministic parameters (decoded from the seed)

`run_one_crash_case(root, 155)` in `crates/storage-next/src/testkit/simulation/faults.rs`:

- **durability** = `Standard` (`seed & 1 == 1`)
- **FS model** = `SplitRename` (`seed % 4 == 3`)
- **crash_index** = `1 + (seed >> 2) % FAULT_SIM_STEPS` = `1 + (38 % 24)` = **15**
- **oracle family** = `OnDiskDamage` (Standard ⇒ a clean prefix may be lost, but a gap may **not**)
- SplitRename additionally drives a `Checkpoint` + `drain_maintenance` **before** the crash (faults.rs ~309–315), then `backend.reordering_crash(SplitRename, 155)`.

## Why a `Gap` is a real violation here

`OnDiskDamage` tolerates losing a *suffix* of acknowledged history (Standard durability + power loss). It does **not** tolerate a *gap*: recovering v1, v2, v4, … while v3 is missing means the recovered state is not any prefix of the real history. That points at a recovery path that trusts a snapshot/manifest watermark covering v3 while the segment/object actually carrying v3 was lost or renamed away by the SplitRename crash — or an oracle that should treat this branch's family differently. Both possibilities are in scope for the root-cause slice.

## Proven mechanism (instrumented, seed 155)

Decisive observation — the model acked v1–v5, but recovery returned **only `{v5}`** (`recovered_versions=[CommitVersion(5)]`), with `snapshot_id=Some(2)`, `trusted_wm=Some(5)`, `flush_wm=None`, `wal_records=0`, `tables_stage=false`. The dropped object (instrumented) is `tables/<branch>/manifest` — the **table manifest**.

1. A flush wrote an L0 table object + the **table manifest** = the durable base for v1–v4 (the flushed rows).
2. A **delta checkpoint** wrote snapshot 2 — content **`{v5}` only** (the checkpoint snapshot is "a bounded delta (active + frozen rows)", `lifecycle/checkpoint.rs:1429`), watermark 5. The database manifest records `snapshot_watermark=5` but **`flushed_through_commit_id=None`** (the proof-gated flush-watermark task had not recorded it; `persist_snapshot_facts` preserves whatever was there — `service/manifest.rs:363`).
3. `SplitRename` force-drops one published object — here the table manifest (`testkit/reordering_backend.rs:182` → `drop_object_file`). This is **in contract** (`split_rename_falls_back_to_the_log_without_loss`): recovery must fall back to a clean prefix.
4. Recovery loads the delta snapshot, **trusts watermark 5**, and `recover_tables` finds the table manifest missing and returns `(absent, None)` (a dropped manifest *object* is indistinguishable from "never flushed" — it is **not** a lossy error, so no fault is recorded). The combine arm `(Some checkpoint, None table-manifest)` at `recovery.rs:152` installs the orphaned delta alone → `{v5}` = a gap. The WAL can't help: it was truncated past v4, and `replay_start=5` leaves `wal_records=0`.

## Why a clean fix is a durable-invariant redesign

The fix must let recovery distinguish two states it currently cannot:
- a **delta** snapshot whose table-manifest base was lost (seed 155 → must recover a clean prefix), versus
- a **full**, self-sufficient snapshot with no base needed (no flush → must keep).

The discriminator is the snapshot's **delta base floor** (the commit boundary below which data lives in the table manifest, not the snapshot). **No durable metadata records it today:**
- The snapshot format records only the **top** watermark (`format/snapshot.rs:29`), and the durable format is **frozen at M3** (golden-gated).
- `flushed_through_commit_id` is either `None` or — via the checkpoint's `CheckpointCovered` follow-up (`lifecycle/checkpoint.rs:1528`) — set to the **top** (`visible_version`), never the base floor.
- A recovery-only content heuristic ("snapshot's lowest row version > 1") is **unsound**: an overwrite-heavy *full* snapshot also has its lowest live row at a high version, so the heuristic would discard valid full snapshots and *cause* data loss in healthy recoveries.

So a correct fix requires recording the snapshot's base floor durably (a new manifest semantic, designed around the frozen format and the proof-gated flush-watermark invariant) **plus** the recovery change: when the base floor `F > 0` and the table manifest covering `[1..F]` is absent, the delta is orphaned → recover the WAL-contiguous prefix from v1 (empty here) + record `DataLoss`. This entangles the checkpoint flush-watermark semantics, the manifest recovery facts, recovery reconciliation, and golden vectors — a dedicated slice, not a surgical patch.

## Repro

```bash
# Standing regression (deterministic single seed) — un-ignore when fixed:
cargo test -p strata-storage-next --features fault-injection,localfs --lib \
  regression_split_rename_power_loss_recovers_clean_prefix -- --ignored

# Full soak — deterministically fails at seed 155:
STRATA_STORAGE_FAULT_CASES=3000 cargo test -p strata-storage-next \
  --features fault-injection,localfs --test simulation_faults -- --ignored \
  fault_simulation_soak_deepens_across_many_seeds
```

## Next

Own slice (mirror the seed-74 discipline, now with the design work scoped above): record the snapshot's delta base floor durably, add the recovery requirement + WAL-contiguity guard so recovery yields a clean prefix, un-ignore `regression_split_rename_power_loss_recovers_clean_prefix`, and re-run the fault soak to confirm it runs clean end-to-end. Until then, **class 9 stays open.**
