# M4P-L8J Test Plan: Delta Checkpoint + Unified Recovery

Status: A/B/C and F implemented (J0/J1/J2 landed and verified); D/E deferred to J3
(crash-consistency + scale gates, which land with the throttle/off-lock-publish slice).

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8j-checkpoint-delta-and-unified-recovery-implementation-plan.md`

## Landed coverage (lifecycle/format/testkit tests)

- **A** — `encode_section_rejects_materialized_payload_over_limit`,
  `encode_container_rejects_materialized_payload_over_limit` (+ round-trip in the latter).
- **B1** — `checkpoint_rows_emit_active_frozen_delta_excluding_owned_and_newer`,
  `checkpoint_rows_exclude_materialized_owned_rows`, and the testkit `check_input_rows`
  contract (owned excluded). **B2** — `checkpoint_rows_delta_size_is_independent_of_owned_levels`.
  **B (empty-delta publish)** — `checkpoint_publishes_empty_delta_and_advances_watermark_when_all_rows_flushed`
  (vs `checkpoint_defers_when_branch_has_no_rows_under_visible_watermark` for a genuinely
  empty branch). **B3** — watermark filter asserted in the emit/testkit tests. **B4** —
  `format_golden` unchanged (no checkpoint-content golden exists; structure frozen).
- **C1/C2/C5** — `recovery_preflights_checkpoint_and_table_manifest_disjoint_succeeds`
  (both rows visible; Mode `ckpt≥flush`), `recovery_accepts_flush_watermark_above_checkpoint_when_table_manifest_covers`
  (Mode `flush>ckpt`), `checkpoint_recovery_round_trip_after_frozen_flush` (owned from
  manifest + delta from checkpoint; delta `row_count` bounded), `checkpoint_recovery_restores_rows_without_covered_log_records`
  (no-manifest path). **C3** — `recovery_accepts_exact_duplicate_checkpoint_table_manifest_rows`.
  **C4** — `recovery_rejects_checkpoint_table_manifest_duplicate_internal_key_conflict`.
- **F** — full `--all-features` suite (the pruned-compaction recovery + commit-hardening
  auto-checkpoint suites exercise the empty-delta watermark advance end-to-end); cache
  absence counters unchanged.

D and E (crash windows, fsync-failure, >64 MiB / forced-checkpoint-at-scale,
settle-to-quiescence) remain for the J3 slice. The mechanism they gate is already covered
by B2 + the empty-delta-publish test (delta bounded by backlog, not DB size) and the
existing checkpoint fault tests; J3 adds the literal scale/crash gates.

## Goal

Prove that making the checkpoint snapshot a bounded delta (active + frozen only) and
recovering by combining {manifest owned levels + checkpoint delta + WAL replay} preserves
durability, crash-consistency, and recovery correctness **and** removes the >64 MiB snapshot
failure — without changing the durable format structure or cache-mode behavior.

The suite must fail if any change:

1. loses any owned-level row on recovery (the delta omits them; recovery must source them
   from the manifest);
2. recovers to a state differing from a fully-synchronous baseline for the same write
   history;
3. writes a checkpoint snapshot that cannot be decoded (size ceiling), or truncates WAL
   before the covering checkpoint + manifest are durable;
4. changes the durable on-disk **format structure** (vs intended snapshot *content* change);
5. regresses cache-mode behavior or the admission watchdog.

## A — Encode/decode ceiling symmetry (landed in J0)

1. `encode_snapshot_section` / `encode_snapshot_container` reject a payload exceeding the
   64 MiB ceiling with `FormatError::InvalidLength { field: "snapshot_materialized_payload" }`
   (tests `encode_section_rejects_materialized_payload_over_limit`,
   `encode_container_rejects_materialized_payload_over_limit`). ✔ landed.
2. A successfully-encoded container always round-trips through `decode_snapshot_container`
   (encode/decode symmetry — covered by the new encode tests' round-trip assertion).

## B — Delta checkpoint content (J2)

1. **Delta excludes owned levels**: after flushing some rows to owned levels and leaving
   others in active/frozen, the checkpoint snapshot's materialized rows equal exactly the
   active+frozen set — assert the owned-level rows are absent from the snapshot and the
   active/frozen rows are present (differential against `checkpoint_rows`).
2. **Snapshot size scales with backlog, not DB size**: a large owned-level set with a small
   active/frozen backlog produces a small snapshot (regression guard: snapshot bytes bounded
   by in-memory backlog, independent of total rows).
3. **Watermark filter retained**: rows with `commit_version > watermark` are excluded.
4. **Golden vectors**: regenerate checkpoint snapshot goldens; assert the durable header /
   section *structure* is unchanged (format frozen) while content reflects the delta. The
   structural format golden (`format_golden`) must pass unchanged.

## C — Unified recovery correctness (J1) — the core gate

1. **Owned levels reconstructed from the manifest, not the snapshot**: recover a database
   whose checkpoint snapshot is a delta (no owned-level rows); assert every owned-level row
   is present after recovery (sourced from manifest table objects), proving the delta is
   safe.
2. **Combine, not choose**: recovery applies manifest owned levels AND the checkpoint delta
   AND WAL replay; the recovered branch equals a synchronous-execution baseline of the same
   logical history (differential test) for both orderings: checkpoint_wm > flush_wm and
   flush_wm > checkpoint_wm.
3. **Legacy full-superset snapshot still recovers**: a checkpoint snapshot that still
   contains owned-level rows (pre-J2 in-tree state) recovers correctly — the disjoint
   preflight accepts exact-byte duplicates between snapshot and manifest. (No pre-V1 DB
   migration is promised, but in-tree compatibility within a build must hold.)
4. **Divergent bytes rejected**: a snapshot row and a manifest row at the same internal key
   with different bytes fails recovery (corruption guard) — unchanged from today.
5. **No-manifest path**: a checkpoint with no table manifest still recovers from the
   snapshot delta + WAL (define and test the small-database / pre-first-flush case).

## D — Durability ordering + crash consistency (J3)

1. **Ack/visibility ordering** unchanged: a commit becomes durable/visible only after its WAL
   record is durable (existing guarantee; regression check).
2. **WAL truncation gate**: WAL truncation up to `checkpoint_watermark` happens only after
   the checkpoint snapshot AND the covering manifest are durable. Fault-inject a delay on
   snapshot persist and assert truncation does not outrun it.
3. **Crash windows** (fault injection + recovery):
   - crash after snapshot publish, before WAL truncation ⇒ recovery uses snapshot + manifest
     + WAL; state == synchronous baseline;
   - crash before snapshot publish ⇒ recovery uses manifest + full WAL replay; state ==
     baseline;
   - crash mid-truncation ⇒ recovery consistent.
4. **fsync failure** on snapshot publish ⇒ checkpoint fails cleanly, WAL not truncated,
   recovery reconstructs from manifest + WAL.

## E — Scale / forced-checkpoint (J3) — the original repro

1. **Large-backlog forced checkpoint**: drive a load that forces a checkpoint against a
   large in-memory + owned-level state (the regime that crashed at 10M with the throttle
   relaxed); the checkpoint succeeds (delta is small) and the database recovers correctly.
2. **>64 MiB database checkpoints**: a database whose total durable state exceeds 64 MiB
   checkpoints successfully (delta bounded), proving the fix removes the size ceiling crash.
3. Settle-to-quiescence recovery after a 10M load with at least one executed checkpoint
   matches the synchronous baseline.

## F — Regression

1. All existing durable commit / conflict / timestamp / branch / read / recovery / WAL /
   checkpoint tests pass unchanged (no asserted class/code changes).
2. Cache-mode absence counters unchanged (cache has no checkpoint/snapshot).
3. `format_golden` structural goldens pass; snapshot content goldens regenerated with
   rationale.
4. Any new named storage boundary type is documented with its owning layer and
   rationale.

## Regression commands

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features
cargo test -p strata-storage-next --features localfs,perf-trace --test format_golden
cargo test -p strata-storage-next checkpoint        # checkpoint + recovery filters
cargo test -p strata-storage-next recovery
```

## Failure interpretation

1. An owned-level row missing after recovery ⇒ J1 unified recovery did not source owned
   levels from the manifest; data loss, stop.
2. A recovered state differing from the synchronous baseline ⇒ a coverage gap/overlap in the
   manifest/snapshot/WAL partition; correctness bug.
3. A checkpoint still failing at >64 MiB ⇒ J2 did not actually bound the delta (something
   still materializes owned levels).
4. A `format_golden` structural failure ⇒ unintended on-disk format-structure change; out of
   scope, revert.
