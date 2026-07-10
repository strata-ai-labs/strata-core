# L8F Test Plan: Recovery Orchestration

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that L8F orchestrates durable recovery inputs in the correct order and
returns typed recovery facts without taking over L8G, maintenance, or product
responsibilities.

Tests should fail if L8F:

1. skips a manifest-listed checkpoint before selecting the WAL replay start;
2. trusts a snapshot watermark without loading and validating the snapshot;
3. uses the manifest active WAL segment id as a commit-version watermark;
4. reads WAL from zero when a trusted recovered checkpoint watermark exists;
5. drops or rewrites WAL records that should be handed to L8G;
6. reports `Healthy` after data loss or lossy policy downgrade;
7. treats non-latest WAL corruption as repairable;
8. repairs a WAL tail without calling the L4 repair path;
9. ignores missing/corrupt referenced table objects;
10. ignores quarantine inventory corruption or identity mismatch;
11. appends checkpoint rows directly to L6 internals;
12. calls `CommitReplayRuntime` in L8F;
13. advances L7 visible version or allocators in L8F;
14. starts maintenance, retention, purge, or quarantine mutation;
15. calls product primitive reconstruction;
16. collapses lower-layer source errors into unstructured strings.

Do not add tests whose only assertion is that a plan document exists or links
to another plan.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/recovery.rs` for direct L8F unit
   tests.
2. `crates/storage-next/src/lifecycle/tests/mod.rs` only for shared helpers.
3. `crates/storage-next/src/testkit/lifecycle/recovery.rs` for generated
   recovery scripts, counters, and reference checks.
4. `crates/storage-next/tests/lifecycle_recovery.rs` for integration-level
   durable recovery tests over memory and localfs where available.
5. `crates/storage-next/tests/lifecycle_properties.rs` for generated recovery
   property checks behind `testkit`.
6. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
7. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8F verification and sensitivity-probe record after implementation.

## Test Data Principles

1. Build test storage through L4/L6/L7 helpers whenever possible.
2. Use raw byte corruption only for explicit corrupt-object tests.
3. Use storage-owned rows and keys, never product primitive DTOs.
4. Do not hardcode reserved layout paths in production fixtures.
5. Keep canonical smoke fixtures separate from generated-input coverage.
6. Generated tests must count input-derived paths separately from fixed setup.
7. Localfs tests may be gated by feature/platform, but memory tests must cover
   the contract by default.

## Direct Unit Tests

### 1. Admission And Slice Boundary

Required tests:

1. `recovery_requires_recovering_durable_shell`
2. `recovery_step_admission_is_checked_before_service_reads`
3. `ordinary_reads_remain_rejected_after_l8f_outcome`
4. `commits_remain_rejected_after_l8f_outcome`
5. `ordinary_maintenance_remains_rejected_after_l8f_outcome`
6. `l8f_does_not_call_l7_replay`
7. `l8f_does_not_advance_visible_version`
8. `l8f_does_not_catch_up_commit_allocator`
9. `l8f_does_not_start_maintenance`

Assertions:

1. shell state remains `Recovering`;
2. L7 replay spy/counter remains zero;
3. visible version remains unchanged;
4. allocator and timestamp guard remain unchanged;
5. the returned package is accepted as L8G input but is not a final open
   outcome.

### 2. Recovery Request Validation

Required tests:

1. strict recovery request accepts default limits;
2. explicit lossy request accepts only when the open plan allowed lossy
   recovery;
3. zero `max_faults` rejects;
4. zero `max_snapshot_sections` rejects;
5. invalid checkpoint identity seed rejects;
6. request strictness must match or narrow the open plan policy;
7. request validation happens before any service read.

### 3. Manifest Snapshot Fact Validation

Required tests:

1. no snapshot id and no snapshot watermark is valid;
2. snapshot id without watermark rejects;
3. watermark without snapshot id rejects;
4. snapshot id `0` rejects;
5. snapshot watermark `CommitVersion::ZERO` rejects when a snapshot id exists;
6. manifest flush watermark alone is valid;
7. snapshot watermark and flush watermark both present is valid;
8. validation failure happens before snapshot service load;
9. validation failure happens before WAL read.

### 4. Replay Start Calculation

Required tests:

1. no trusted snapshot and no flush watermark uses `CommitVersion::ZERO`;
2. flush watermark alone fails closed until flushed table-state recovery lands;
3. trusted snapshot watermark alone uses snapshot watermark;
4. trusted snapshot watermark greater than flush watermark uses snapshot
   watermark;
5. flush watermark greater than trusted snapshot watermark fails closed until
   flushed table-state recovery lands;
6. failed snapshot load does not contribute a replay-start watermark;
7. lossy fallback after snapshot loss does not trust a flush watermark by
   itself;
8. lossy fallback after snapshot loss falls back to zero when no other
   watermark exists;
9. active WAL segment id is ignored by replay-start calculation;
10. records equal to the replay start are excluded from the WAL package;
11. records greater than the replay start are included.

### 5. Empty Durable Recovery

Required tests:

1. new durable database with no WAL records recovers healthy;
2. new durable database returns zero checkpoint rows;
3. new durable database returns zero WAL records;
4. new durable database reports missing quarantine inventory as healthy empty;
5. new durable database does not call snapshot load;
6. new durable database does not call table reader;
7. new durable database remains in `Recovering`.

### 6. Snapshot Load And Identity

Required tests:

1. manifest-listed snapshot loads through `SnapshotService`;
2. loaded snapshot id must match manifest snapshot id;
3. loaded snapshot database id must match assembly database id;
4. loaded snapshot codec id must match open plan codec id;
5. loaded snapshot watermark must match manifest snapshot watermark;
6. zero-section snapshot is a valid empty checkpoint;
7. too many sections rejects before exposing section bytes;
8. corrupt snapshot magic returns `CorruptSnapshot` with source chain;
9. corrupt snapshot CRC returns `CorruptSnapshot` with source chain;
10. future snapshot version fails closed;
11. missing snapshot in strict mode fails recovery;
12. missing snapshot in explicitly lossy mode returns degraded health, not
    healthy, only if the implementation intentionally supports that downgrade;
13. snapshot codec mismatch is failed recovery even in lossy mode.

### 7. Checkpoint Row Decode And Install

Required tests:

1. row-native checkpoint section decodes into `StorageRow` values;
2. multiple row sections concatenate deterministically;
3. checkpoint rows install through `BranchSnapshotInstallRequest::from_rows`;
4. empty checkpoint install is a no-op outcome;
5. checkpoint rows for one branch install into that branch;
6. checkpoint rows for unopened branches fail closed until the runtime owns a
   multi-branch state map;
7. duplicate internal keys fail through L6 snapshot install;
8. unsorted checkpoint rows are normalized by `from_rows`;
9. invalid row bytes fail with format/source preservation;
10. branch snapshot install error maps to `LifecycleLowerLayer::BranchRuntime`;
11. no test can observe direct active/frozen/owned-level mutation outside L6.

### 8. Table Object Validation

Required tests:

1. recovery metadata with no table references does not call table reader;
2. referenced table object opens through `TableObjectReaderService`;
3. missing referenced table object is failed in strict mode;
4. missing referenced table object is degraded only if explicitly documented as
   lossy;
5. corrupt table object CRC preserves table/runtime source;
6. table object byte-count mismatch preserves service source;
7. table object row-count mismatch rejects;
8. table object commit range is recorded in recovery facts;
9. table reader is not used to mutate L6 state directly;
10. table-object errors never become `Healthy`.

### 9. WAL Read And Package

Required tests:

1. WAL read uses `read_after_commit_version(replay_start)`;
2. WAL records before or equal to replay start are not packaged;
3. WAL records after replay start are packaged unchanged;
4. packaged records preserve branch id;
5. packaged records preserve commit version;
6. packaged records preserve commit timestamp;
7. packaged records preserve payload rows;
8. mixed-branch records preserve original order;
9. timeline rows are preserved for L8G;
10. timeline rows are not validated by L8F;
11. WAL read source error maps to service lower-layer error;
12. WAL record decode source remains reachable through `Error::source()`.

### 10. WAL Partial Tail And Repair

Required tests:

1. latest partial tail produces a `WalTruncation` fact;
2. L8F calls `WalService::repair_latest_tail`;
3. successful repair records `WalRepair`;
4. repair preserves valid-prefix records for L8G;
5. repair removes only bytes after `valid_end_offset`;
6. repair active-segment mismatch fails strict recovery;
7. repair stale object-size check fails strict recovery;
8. repair backend read failure preserves source;
9. non-latest WAL corruption is not repaired;
10. non-latest WAL corruption fails strict recovery;
11. explicitly lossy fallback never reports healthy after lost WAL tail bytes.

### 11. Quarantine Inventory

Required tests:

1. missing quarantine inventory returns healthy empty inventory facts;
2. valid quarantine inventory records present flag and byte count;
3. valid inventory records entry count;
4. corrupt inventory bytes classify `QuarantineInventoryMismatch`;
5. wrong branch id classifies `QuarantineInventoryMismatch`;
6. wrong database id classifies `QuarantineInventoryMismatch`;
7. wrong codec id classifies `QuarantineInventoryMismatch`;
8. backend read failure preserves source;
9. L8F does not publish replacement inventory;
10. L8F does not delete quarantine objects;
11. L8F does not purge inventory entries.

### 12. Health Aggregation

Required tests:

1. healthy recovery has no faults;
2. strict corrupt snapshot returns failed health/error;
3. strict corrupt WAL returns failed health/error;
4. strict missing table object returns failed health/error;
5. explicit lossy downgrade returns `RecoveryHealth::Degraded`;
6. degraded recovery requires at least one fault;
7. confirmed data loss uses `RecoveryDegradationClass::DataLoss`;
8. quarantine inventory mismatch uses `RecoveryDegradationClass::Telemetry`;
9. policy downgrade uses `RecoveryDegradationClass::PolicyDowngrade` only
   for non-lossy policy relaxation cases;
10. codec mismatch is failed, not degraded;
11. data-loss recovery never reports `Healthy`;
12. strict partial WAL tail returns `WalTailRepairRejected`;
13. quarantine recovery failure occurs before WAL tail repair side effects;
14. snapshot section count above request limit fails;
15. zero snapshot ids are rejected before lifecycle trusts manifest recovery
    facts;
16. too many faults fails with a typed recovery error instead of truncating
    silently.

### 13. Lower-Layer Source Chains

Required tests:

1. snapshot decode error is visible as an error source;
2. WAL decode error is visible as an error source;
3. WAL repair backend error is visible as an error source;
4. table reader error is visible as an error source;
5. quarantine decode error is visible as an error source;
6. branch snapshot install error is visible as an error source;
7. display text remains storage-owned and product-neutral;
8. equality tests ignore source identity only where existing error policy
   requires that behavior.

## Integration Tests

Place in `crates/storage-next/tests/lifecycle_recovery.rs`.

Required cases:

1. memory backend empty durable recovery;
2. memory backend checkpoint-only recovery;
3. memory backend checkpoint plus WAL tail recovery package;
4. memory backend flush watermark without snapshot;
5. memory backend latest partial WAL tail repair;
6. memory backend corrupt snapshot strict failure;
7. memory backend corrupt WAL strict failure;
8. memory backend quarantine mismatch classification;
9. localfs empty durable recovery, gated by `localfs` and platform support;
10. localfs checkpoint plus WAL tail recovery, gated by `localfs`;
11. localfs writer-lock remains held through recovery package production.

Integration tests should assert storage behavior, not plan document structure.

## Generated Property Tests

Extend `crates/storage-next/src/testkit/lifecycle/recovery.rs` and
`crates/storage-next/tests/lifecycle_properties.rs`.

Generated script inputs should vary:

1. manifest disposition: new, existing no checkpoint, existing checkpoint;
2. snapshot presence: absent, valid empty, valid rows, missing, corrupt;
3. snapshot watermark relative to flush watermark;
4. WAL record counts from 0 to a bounded generated maximum;
5. WAL record versions before/equal/after replay start;
6. WAL truncation: none, latest repairable, latest repair failure, non-latest
   corruption;
7. quarantine inventory: missing, valid, corrupt, identity mismatch;
8. table references: none, valid, missing, corrupt;
9. strictness: strict, explicitly lossy when open plan allows;
10. multiple branch ids in checkpoint and WAL rows.

Counters must separate fixed canonical setup from input-derived coverage.

Required counters:

1. empty recovery;
2. checkpoint loaded;
3. checkpoint installed;
4. replay start from zero;
5. replay start from snapshot;
6. flush watermark rejected unless recovered checkpoint/table state covers it;
7. WAL records packaged;
8. WAL repair attempted;
9. strict failure;
10. lossy degradation;
11. quarantine mismatch;
12. table validation with validated identity/facts retained in recovery facts;
13. source-chain failure;
14. no L7 replay;
15. no product callback.

The property test must fail if any required input-derived counter is zero after
the generated run set.

## Source Guards

Extend `tests/lifecycle_source_guard.rs`.

Required guards:

1. production lifecycle code must not import engine modules;
2. production lifecycle code must not import product primitive modules;
3. production lifecycle code must not import StrataHub or remote sync modules;
4. production lifecycle code must not import follower/replica modules;
5. production lifecycle code must not use `std::fs`;
6. production lifecycle code must not use `std::path::Path`;
7. production lifecycle code must not use `std::env`;
8. lower layers must not import `crate::lifecycle`;
9. `lifecycle/recovery.rs` must not mention `CommitReplayRuntime`;
10. `lifecycle/recovery.rs` must not call normal commit runtime execute paths;
11. `lifecycle/recovery.rs` must not hardcode reserved layout strings such as
    WAL, snapshot, table, quarantine, or writer-lock prefixes;
12. recovery tests may use fixture strings, but production source may not.

## Sensitivity Probes

Record the probe result in `m4-l8-porting-log.md` after implementation.

| Probe | Mutation | Expected failing test |
|---|---|---|
| L8F-S1 | Skip snapshot load but still trust manifest snapshot watermark. | Replay-start/snapshot validation tests fail. |
| L8F-S2 | Use active WAL segment id as replay-start commit version. | Replay-start calculation tests fail. |
| L8F-S3 | Always read WAL from zero even after trusted checkpoint. | WAL package tests fail. |
| L8F-S4 | Include records equal to replay start. | WAL filtering tests fail. |
| L8F-S5 | Drop WAL records after replay start. | WAL package preservation tests fail. |
| L8F-S6 | Treat latest partial tail as healthy without repair. | WAL repair tests fail. |
| L8F-S7 | Repair latest partial tail in strict mode. | Strict tail-repair rejection tests fail. |
| L8F-S8 | Repair non-latest WAL corruption. | Non-latest corruption tests fail. |
| L8F-S9 | Report corrupt snapshot as healthy. | Health aggregation tests fail. |
| L8F-S10 | Report lossy data loss as healthy. | Lossy health tests fail. |
| L8F-S11 | Ignore missing required table object. | Table validation tests fail. |
| L8F-S11 | Ignore quarantine identity mismatch. | Quarantine tests fail. |
| L8F-S12 | Collapse lower-layer source into a static string. | Source-chain tests fail. |
| L8F-S13 | Call `CommitReplayRuntime` from L8F. | Source guard and replay spy tests fail. |
| L8F-S14 | Advance visible version in L8F. | Slice-boundary tests fail. |
| L8F-S15 | Call product primitive reconstruction. | Source guard tests fail. |

## Fuzz Targets

If L8F adds fuzz targets in this slice, use real surface-specific decoders:

1. `lifecycle_recovery_snapshot`:
   - arbitrary bytes as snapshot object bytes;
   - must reject corrupt bytes with typed errors and never panic;
   - must never expose section bytes before header identity validation.
2. `lifecycle_recovery_wal_tail`:
   - deterministic generated WAL segment scripts with partial-tail mutations;
   - must preserve valid-prefix records and classify repair failures.
3. `lifecycle_recovery_script`:
   - opcode stream for manifest/snapshot/WAL/quarantine/table combinations;
   - must drive the same generated counters as property tests.

Seed corpora should include:

1. empty durable database;
2. valid empty checkpoint;
3. valid checkpoint plus one WAL record after watermark;
4. latest partial WAL tail;
5. corrupt snapshot header;
6. corrupt WAL segment;
7. quarantine identity mismatch.

Fuzz file-presence tests alone are not sufficient.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If localfs is enabled and supported:

```bash
cargo test -p strata-storage-next --features localfs --locked --test lifecycle_recovery
```

## Closeout Checklist

L8F can close when:

1. direct recovery tests pass;
2. integration recovery tests pass on memory backend;
3. localfs recovery tests pass when supported;
4. generated recovery counters prove input-derived coverage;
5. source guards prove no product, engine, raw filesystem, or L7 replay drift;
6. strict and lossy health classifications are pinned;
7. source-chain tests cover snapshot, WAL, table, branch, and quarantine errors;
8. sensitivity probe ledger is recorded in the porting log;
9. verification commands are recorded in the porting log;
10. L8G handoff facts are documented and tested enough for L8G to consume.
