# STH-4 finding: power-loss recovery `Gap` under SplitRename (seed 155)

**Status:** open bug — newly surfaced, not yet triaged into a minimal repro.
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

## Suspect areas (for the root-cause slice)

- `StorageBackend::reordering_crash(FsModel::SplitRename, …)` — how a split/partial rename perturbs the just-checkpointed snapshot + manifest + WAL segment set.
- `crates/storage-next/src/lifecycle/recovery.rs` — reconciling a snapshot/manifest watermark against the surviving WAL segments after a SplitRename crash; whether it can admit a non-contiguous prefix.
- `crates/storage-next/src/service/checkpoint.rs` — the snapshot the pre-crash checkpoint published and which versions it claims to cover.
- The oracle: `classify_recovered(..., CrashFamily::OnDiskDamage)` — confirm `Gap` is correctly disallowed for this case (it should be).

## Repro

```bash
# Full soak — deterministically fails at seed 155:
STRATA_STORAGE_FAULT_CASES=3000 cargo test -p strata-storage-next \
  --features fault-injection,localfs --test simulation_faults -- --ignored \
  fault_simulation_soak_deepens_across_many_seeds

# Single seed (add a focused test calling run_one_crash_case(dir, 155)).
```

## Next

Own slice: isolate → prove → fix (mirror the seed-74 finding's discipline). Capture a `#[ignore]` failing-then-fixed regression for seed 155, fix the engine (or correct the oracle if the violation is a harness artifact), then re-run the soak to confirm it runs clean. Until then, **class 9 stays open.**
